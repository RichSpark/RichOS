#!/usr/bin/env python3
"""git-iconv-walk.py — the candidate's own bundled git, in a guest, on a repository whose file name has an accent.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest, launches the candidate's
signed app with the candidate's engine tarball handed in (RICHOS_ENGINE_DIR), and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE.tar.gz --report REPORT -- \\
      git-iconv-walk.py --out DIR --expect-git-sha SHA256 [--control-git-dir HOST_GIT_DIR] [--notices-sha SHA256]

WHY IT EXISTS. Nightly 78's bundled git was built without the system iconv: with core.precomposeunicode=true
and a file whose name carries an accent, every git command failed with
"iconv_open(UTF-8,UTF-8-MAC) failed" (9e8d8a796 links the system libiconv; merged at 161701f70). This walk asks
the question in the guest, with the engine the app was handed, never on the host.

STEPS (in the guest, never on the host's screen, CEO ruling 65):
  app        the app process is alive and has at least one window
  git-ident  the engine's git in the guest is byte-identical to the one the host measured (--expect-git-sha)
             and links /usr/lib/libiconv
  git-run    in a repository with core.precomposeunicode=true and a decomposed (NFD) accented file name:
             status, add, commit and ls-files each exit 0 and none prints iconv_open
  control    (with --control-git-dir) the same script with the PREVIOUS nightly's git must fail with
             iconv_open: a test that cannot fail proves nothing. Without it the step is SKIPPED, not PASS
  notices    the installed app's THIRD-PARTY-NOTICES.md names sentencepiece-model and carries its BSD-2-Clause
             text (and matches --notices-sha). The app has no notices screen: its About item shows name and
             version only, so the About panel is captured as a frame, best effort
  frames     the app window, and the About panel

Writes <out>/report.json and frames. Exit 0 only when no step is FAIL and git-run passed.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent

REPO_SCRIPT = r'''#!/bin/bash
# one repository, one accented file, five git commands; every exit code is printed
G="$1"; D="$(mktemp -d "$HOME/iconv-walk.XXXXXX")"; cd "$D" || exit 99
echo "git: $G"; "$G" --version
"$G" init -q repo 2>&1; echo "init rc=$?"; cd repo || exit 99
"$G" config core.precomposeunicode true; echo "config rc=$?"
"$G" config user.email walk@example.invalid; "$G" config user.name walk
: > "$(printf 'cafe\xcc\x81.txt')"
echo "precompose=$("$G" config core.precomposeunicode)"
ls | od -c | head -2
"$G" status --porcelain 2>&1; echo "status rc=$?"
"$G" add . 2>&1; echo "add rc=$?"
"$G" commit -q -m walk 2>&1; echo "commit rc=$?"
"$G" ls-files 2>&1; echo "ls-files rc=$?"
"$G" status 2>&1 | head -3; echo "status2 rc=${PIPESTATUS[0]}"
cd "$HOME"; rm -rf "$D"
'''


def vm_tool(name, *args, timeout=120, check=True):
    r = subprocess.run([str(HERE / name), *args], capture_output=True, text=True, timeout=timeout)
    if check and r.returncode:
        raise RuntimeError(f'{name} {" ".join(args)[:80]} -> exit {r.returncode}: {(r.stderr or r.stdout).strip()[:400]}')
    return r


def guest(vm, command, timeout=120):
    return vm_tool('guest.sh', vm, command, timeout=timeout, check=False)


def rcs(text):
    out = {}
    for line in text.splitlines():
        if ' rc=' in line:
            k, v = line.rsplit(' rc=', 1)
            out[k.strip()] = int(v) if v.strip().isdigit() else v.strip()
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-git-sha', required=True, help='sha256 of the engine git the host measured')
    p.add_argument('--control-git-dir', type=Path, help='host dir holding the previous nightly runtime/git (bin/, libexec/)')
    p.add_argument('--notices-sha', help='sha256 of the THIRD-PARTY-NOTICES.md the host extracted from the bundle')
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    run = Path.home() / '.richos-testvm' / 'run' / a.vm
    pid = (run / 'app.pid').read_text().strip()
    payload = (run / 'payload').read_text().strip()
    steps = []

    def step(name, outcome, detail, **extra):
        steps.append({'step': name, 'outcome': outcome, 'detail': detail, **extra})
        print(f'{name}: {outcome} — {detail}', flush=True)

    # app
    comm = guest(a.vm, f'ps -p {pid} -o command= 2>/dev/null').stdout.strip()
    win = guest(a.vm, f"osascript -e 'tell application \"System Events\" to count windows of (first process whose unix id is {pid})' 2>/dev/null || echo 0").stdout.strip()
    app_path = comm.split('/Contents/MacOS/')[0] if '/Contents/MacOS/' in comm else ''
    step('app', 'PASS' if comm.endswith('richos-tauri') and win.isdigit() and int(win) >= 1 else 'FAIL',
         f'pid {pid} {comm.rsplit("/", 1)[-1]}, {win} window(s), app {app_path}')

    # git identity
    git = f'{payload}/engine/runtime/git/bin/git'
    sha = guest(a.vm, f"shasum -a 256 '{git}' | cut -d' ' -f1").stdout.strip()
    link = guest(a.vm, f"otool -L '{git}' 2>&1 || DYLD_PRINT_LIBRARIES=1 '{git}' --version 2>&1").stdout
    iconv = [l.strip().split(' ')[0] for l in link.splitlines() if 'iconv' in l]
    (a.out / 'git-linkage.txt').write_text(link)
    ok = sha == a.expect_git_sha and any('/usr/lib/libiconv' in l for l in iconv)
    step('git-ident', 'PASS' if ok else 'FAIL', f'guest git sha256 {sha[:16]}.. (host measured {a.expect_git_sha[:16]}..); iconv links: {iconv or "none"}')

    # git run, candidate
    (a.out / 'repo-script.sh').write_text(REPO_SCRIPT)
    vm_tool('guest.sh', a.vm, '--push', str(a.out / 'repo-script.sh'), 'repo-script.sh')
    r = guest(a.vm, f"bash ./repo-script.sh '{git}' 2>&1")
    (a.out / 'git-run.txt').write_text(r.stdout)
    codes = rcs(r.stdout)
    want = ['init', 'config', 'status', 'add', 'commit', 'ls-files', 'status2']
    good = r.returncode == 0 and all(codes.get(k) == 0 for k in want) and 'iconv_open' not in r.stdout
    step('git-run', 'PASS' if good else 'FAIL', f'exit codes {codes}; iconv_open in output: {"iconv_open" in r.stdout}')

    # control
    if a.control_git_dir:
        tarball = a.out / 'control-git.tar'
        subprocess.run(['tar', '-C', str(a.control_git_dir), '-cf', str(tarball), '.'], check=True)
        vm_tool('guest.sh', a.vm, '--push', str(tarball), 'control-git.tar', timeout=300)
        guest(a.vm, 'rm -rf control-git && mkdir control-git && tar -C control-git -xf control-git.tar')
        c = guest(a.vm, "bash ./repo-script.sh \"$HOME/control-git/bin/git\" 2>&1")
        (a.out / 'git-control.txt').write_text(c.stdout)
        failed = 'iconv_open' in c.stdout
        step('control', 'PASS' if failed else 'FAIL',
             f'previous nightly git, same script: iconv_open failure reproduced = {failed}; exit codes {rcs(c.stdout)}')
    else:
        step('control', 'SKIPPED', 'no --control-git-dir: the test was not shown able to fail')

    # notices
    notes = f'{app_path}/Contents/Resources/THIRD-PARTY-NOTICES.md'
    nsha = guest(a.vm, f"shasum -a 256 '{notes}' | cut -d' ' -f1").stdout.strip()
    hit = guest(a.vm, f"grep -n 'sentencepiece-model 0.1.4 — BSD-2-Clause notice' '{notes}'; grep -c 'Redistribution and use in source and binary forms' '{notes}'").stdout.split()
    ok = bool(hit) and 'sentencepiece-model' in ' '.join(hit) and (a.notices_sha is None or nsha == a.notices_sha)
    step('notices', 'PASS' if ok else 'FAIL', f'installed {notes.rsplit("/", 3)[-3:]} sha256 {nsha[:16]}..; heading line {hit[:1]}; BSD-2 text occurrences {hit[-1:]}')

    # frames
    shot = vm_tool('shot.sh', a.vm, str(a.out / 'app-window.png'), '--ocr', check=False)
    (a.out / 'app-window.txt').write_text(shot.stdout)
    about = vm_tool('ax.sh', a.vm, 'tell application "System Events" to tell (first process whose unix id is ' + pid + ') to click menu item 1 of menu 1 of menu bar item 2 of menu bar 1', check=False)
    time.sleep(2)
    s2 = vm_tool('shot.sh', a.vm, str(a.out / 'about-panel.png'), '--ocr', check=False)
    (a.out / 'about-panel.txt').write_text(s2.stdout)
    step('frames', 'PASS' if shot.returncode == 0 and s2.returncode == 0 else 'FAIL',
         f'app-window.png rc {shot.returncode}; about-panel.png rc {s2.returncode} (about click rc {about.returncode})')

    (a.out / 'report.json').write_text(json.dumps({'vm': a.vm, 'steps': steps}, indent=2) + '\n')
    bad = [s for s in steps if s['outcome'] == 'FAIL']
    return 1 if bad or steps[2]['outcome'] != 'PASS' else 0


if __name__ == '__main__':
    sys.exit(main())
