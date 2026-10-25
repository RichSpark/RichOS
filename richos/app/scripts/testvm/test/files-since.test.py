#!/usr/bin/env python3
"""files-since.py against a temporary tree: every class, every verdict, and the control."""
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / 'files-since.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


def write(path, text='x', at=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    if at is not None:
        os.utime(path, (at, at))


def scan(*args):
    r = subprocess.run([sys.executable, str(TOOL), 'scan', *args], capture_output=True, text=True)
    return r.returncode, (json.loads(r.stdout) if r.stdout.strip() else {}), r.stderr


with tempfile.TemporaryDirectory() as t:
    root = Path(os.path.realpath(t))
    home = root / 'home'
    # Every directory exists BEFORE the first marker, so only what the window itself
    # creates is born inside it. Real sleeps, never utime: a change time cannot be forged.
    for d in ['Library/Caches/com.richos.app', 'Library/Preferences', 'Library/Logs/DiagnosticReports',
              'Library/Application Support', 'myrichos-nightly-a/home/Library/WebKit/com.richos.app',
              '.claude/projects', 'walk', 'Library/WebKit/com.richos.app/WebsiteData']:
        (home / d).mkdir(parents=True, exist_ok=True)
    write(home / 'Library/Caches/com.richos.app/old.db')
    write(home / 'Library/Preferences/noise.plist')
    time.sleep(1.1)
    m0 = root / 'm0'; m1 = root / 'm1'
    subprocess.run([sys.executable, str(TOOL), 'mark', str(m0)], check=True)
    time.sleep(1.1)
    # The baseline window (m0..m1) holds one OS write that recurs later.
    write(home / 'Library/Preferences/noise.plist')
    time.sleep(1.1)
    subprocess.run([sys.executable, str(TOOL), 'mark', str(m1)], check=True)
    time.sleep(1.1)
    base_rc, base, _ = scan('--since', str(m0), '--until', str(m1), '--root', str(home))
    check('baseline window reports exactly the one write inside it',
          base_rc == 0 and [r['path'] for r in base['files']] == [str(home / 'Library/Preferences/noise.plist')],
          base.get('files'))
    bfile = root / 'baseline.json'; bfile.write_text(json.dumps(base))

    # The test window (after m1).
    write(home / 'myrichos-nightly-a/home/Library/WebKit/com.richos.app/store')
    write(home / '.claude/projects/x.jsonl')
    write(home / 'walk/launcher.log')
    write(home / 'Library/Preferences/noise.plist')
    write(home / 'Library/Logs/DiagnosticReports/unrelated.ips')
    control = home / 'Library/Application Support/richos-positive-control.txt'
    write(control)
    (home / 'link-to-claude').symlink_to(home / '.claude')

    common = ['--since', str(m1), '--root', str(home),
              '--inside', str(home / 'myrichos-nightly-a'),
              '--declared', f"{home / '.claude'}=linked sign-in",
              '--harness', str(home / 'walk'),
              '--baseline', str(bfile), '--named', 'richos']
    rc, out, err = scan(*common, '--control', str(control))
    cls = {Path(r['path']).relative_to(home).as_posix(): r['class'] for r in out.get('files', [])}
    check('a clean window with its control found exits 0, verdict pass', rc == 0 and out['verdict'] == 'pass', (rc, out.get('verdict'), err))
    check('writes inside the folder under test are "inside"', cls.get('myrichos-nightly-a/home/Library/WebKit/com.richos.app/store') == 'inside', cls)
    check('writes under a declared link target are "declared", with the reason',
          cls.get('.claude/projects/x.jsonl') == 'declared'
          and any(r.get('reason') == 'linked sign-in' for r in out['files']), out['files'])
    check('the walk\'s own files are "harness"', cls.get('walk/launcher.log') == 'harness', cls)
    check('a path that also changed in the baseline window is "baseline"', cls.get('Library/Preferences/noise.plist') == 'baseline', cls)
    check('every file row carries its size in bytes, a directory 0',
          all(r.get('bytes') == 1 for r in out['files'] if r['kind'] == 'file')
          and all(r.get('bytes') == 0 for r in out['files'] if r['kind'] != 'file'), out['files'])
    check('anything else is "other", reported and never dropped', cls.get('Library/Logs/DiagnosticReports/unrelated.ips') == 'other', cls)
    check('the control is found, and would have been flagged named',
          cls.get('Library/Application Support/richos-positive-control.txt') == 'control'
          and any(r.get('would_be') == 'named' for r in out['files']), out['files'])
    check('files before the window are never reported, even named ones', 'Library/Caches/com.richos.app/old.db' not in cls, cls)
    check('a symlink is reported as a link and never followed', cls.get('link-to-claude') == 'declared' or cls.get('link-to-claude') in ('other', 'baseline'),
          cls)
    check('...and nothing is reported twice through it', sum(1 for k in cls if k.endswith('x.jsonl')) == 1, cls)
    check('the markers themselves are never reported', not any(k in ('m0', 'm1') for k in cls), cls)

    # The finding the search exists for: a named write outside the folder.
    write(home / 'Library/WebKit/com.richos.app/WebsiteData/leak.db')
    rc, out, _ = scan(*common, '--control', str(control))
    check('a named write outside the folder exits 1, verdict named-outside',
          rc == 1 and out['verdict'] == 'named-outside'
          and any(r['class'] == 'named' and r['path'].endswith('leak.db') for r in out['files']), (rc, out.get('verdict')))

    # The search must prove itself.
    rc, out, _ = scan(*common, '--control', str(home / 'Library/never-written-richos.txt'))
    check('a control that was never written exits 2: the search is not trusted', rc == 2 and out['verdict'] == 'search-broken', (rc, out.get('verdict')))
    plain = home / 'Library/plain-control.txt'; write(plain)
    rc, out, _ = scan(*common, '--control', str(plain))
    check('a control the named detector would not flag exits 2', rc == 2 and out['controls_not_flagged_named'] == [str(plain)], (rc, out.get('controls_not_flagged_named')))

    # A root that cannot be listed is a place the search could not cover; the control found
    # under the other root cannot vouch for it.
    gone = root / 'no-such-root'
    rc, out, _ = scan(*common, '--root', str(gone), '--control', str(control))
    check('an unreadable root exits 2 even though the control was found elsewhere',
          rc == 2 and out['verdict'] == 'search-broken' and out['unreadable_roots'] == [str(gone)], (rc, out.get('verdict'), out.get('unreadable_roots')))

# R31: a nested directory that cannot be listed is a subtree the search was told to cover and
# did not. A control found elsewhere cannot vouch for it, unless the caller declared that place
# a known-unreadable system folder.
if os.geteuid() != 0:
    with tempfile.TemporaryDirectory() as t2:
        r2 = Path(os.path.realpath(t2))
        (r2 / 'home/hidden').mkdir(parents=True)
        (r2 / 'sys/locked').mkdir(parents=True)
        subprocess.run([sys.executable, str(TOOL), 'mark', str(r2 / 'm0')], check=True)
        time.sleep(1.1)
        ctl = r2 / 'home/richos-control.txt'
        write(ctl)
        write(r2 / 'home/hidden/richos-leak.db')
        write(r2 / 'sys/locked/whatever')
        os.chmod(r2 / 'home/hidden', 0)
        os.chmod(r2 / 'sys/locked', 0)
        try:
            base = ['--since', str(r2 / 'm0'), '--root', str(r2 / 'home'), '--root', str(r2 / 'sys'),
                    '--control', str(ctl), '--named', 'richos']
            rc, out, _ = scan(*base)
            check('an unreadable nested directory exits 2 even though the control was found elsewhere',
                  rc == 2 and out.get('verdict') == 'search-broken'
                  and sorted(out.get('unreadable_dirs', [])) == sorted([str(r2 / 'home/hidden'), str(r2 / 'sys/locked')]),
                  (rc, out.get('verdict'), out.get('unreadable_dirs')))
            rc, out, _ = scan(*base, '--tolerate-unreadable', str(r2 / 'sys'))
            check('a declared system folder is tolerated and counted; the undeclared one still breaks the search',
                  rc == 2 and out.get('unreadable_dirs') == [str(r2 / 'home/hidden')] and out.get('tolerated_unreadable_count') == 1,
                  (rc, out.get('unreadable_dirs'), out.get('tolerated_unreadable_count')))
            os.chmod(r2 / 'home/hidden', 0o700)
            rc, out, _ = scan(*base, '--tolerate-unreadable', str(r2 / 'sys'))
            check('with only the declared system folder unreadable the search is trusted and finds the leak',
                  rc == 1 and out.get('verdict') == 'named-outside' and out.get('unreadable_dirs') == [],
                  (rc, out.get('verdict'), out.get('unreadable_dirs')))
        finally:
            for d in ('home/hidden', 'sys/locked'):
                os.chmod(r2 / d, 0o700)

print(f'files-since.test.py: {len(failures)} failed')
sys.exit(1 if failures else 0)
