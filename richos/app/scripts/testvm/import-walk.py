#!/usr/bin/env python3
"""import-walk.py: the one-time import (scripts/import-setup.py) against the real app, in this run's guest.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      import-walk.py --out DIR

run-walk.py passes the owned VM name as the first argument.

WHAT IT ANSWERS, in the order the owner would meet it, all inside the guest and against the guest
app's own home (<payload>/home, the HOME run.sh launched it with):

  W1  before: the rail's name is unset on screen, and the boot log says no memory resolved
  W2  the plan only reads: printed, exit 0, nothing written
  W3  --go while the app runs is REFUSED, naming the app's own recorded process
  W4  after the app is stopped (its recorded pid only): --go imports and verifies
  W5  --go again finds nothing to change and writes nothing (the import's write set is unchanged)
  W6  relaunched: the name is on the window (a real frame, its rail foot read by qa/ocr-find.sh;
      W1's frame through the same reader is the control), and the boot log names the memory
      through the loro-root pointer

The stand-in for his setup is synthetic and staged in the guest: a record with wiki/, loro/ and a
roster page, a team folder, and that folder's Claude Code memory index. Nothing of the host's
home is read. The person is "Pat Example".

Writes <out>/import-walk.json and the frames. Exit 0 only when every W passed.
It quits nothing but the app it stops in W4, by the pid run.sh recorded; run-walk.py quits the
relaunched app by its pid, stops the guest and deletes the clone (CEO §54).
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import margin  # noqa: E402
from relaunch import guest, relaunch  # noqa: E402

IMPORT = HERE.parent / 'import-setup.py'
NAME = 'Pat Example'


def utc():
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())


def run_in_guest(vm, command, timeout=60):
    """(exit status, stdout+stderr) of one guest command; a nonzero status is an answer, not an error."""
    r = subprocess.run([str(HERE / 'guest.sh'), vm, command], capture_output=True, text=True, timeout=timeout)
    return r.returncode, (r.stdout + r.stderr).strip()


QA = HERE.parent / 'qa'
# The rail's foot, where the app renders his initials and name (ui/main.js "The foot of the
# rail"), sits in the lower-left of the guest's 1680x1050 display. A whole-frame read MISSES it
# whenever a sheet is up: on 2026-09-28 the company picker's scrim dimmed the rail and both
# whole-frame tesseract (shot.sh --ocr) and the accessibility find came back empty while the
# name was plainly on screen. Cropping that region and enlarging it before reading is what the
# committed toolkit is for (qa/frame.py crop, qa/ocr-find.sh), and it read "PE Pat Example".
#
# Tesseract's answer on that dimmed strip depends on the exact crop, so the reader tries each
# crop below (x, y, w, h, scale) and a hit on ANY of them is a hit. Measured on the 2026-09-28
# frames: the after frame was read by the 1st, 3rd and 5th and missed by the 2nd and 4th; the
# before frame (no name set) was read as the name by NONE of the five. So more crops cost a
# false pass nothing and buy tolerance to a few pixels of window placement.
RAIL_FOOT_READS = [
    ('150', '885', '280', '50', '3'),
    ('140', '860', '300', '100', '3'),
    ('100', '840', '400', '140', '3'),
    ('0', '800', '560', '250', '3'),
    ('140', '860', '300', '100', '2'),
]


def read_name(frame, out, label):
    """(found, detail): whether the committed QA reader finds NAME in any rail-foot crop of `frame`."""
    details = []
    for n, (x, y, w, h, scale) in enumerate(RAIL_FOOT_READS, 1):
        crop = out / ('%s-rail-foot-%d.png' % (label, n))
        made = subprocess.run([str(QA / 'frame.py'), 'crop', str(frame), str(crop), x, y, w, h, '--scale', scale],
                              capture_output=True, text=True, timeout=60)
        if made.returncode:
            return False, 'crop failed: ' + (made.stderr or made.stdout).strip()[-200:]
        found = subprocess.run([str(QA / 'ocr-find.sh'), NAME, str(crop), '--show'],
                               capture_output=True, text=True, timeout=120)
        if found.returncode == 2:
            return False, 'the reader could not run: ' + (found.stderr or found.stdout).strip()[-200:]
        if found.returncode == 0:
            return True, 'crop %d: %s' % (n, found.stdout.strip()[-200:])
        details.append('crop %d: no hit' % n)
    return False, '; '.join(details)


def name_on_screen(vm, out, label):
    """A real frame of the guest's screen, read by the committed QA reader; the accessibility
    tree's answer is recorded beside it and decides nothing (it missed a name that was there)."""
    ax = subprocess.run([str(HERE / 'ax.sh'), vm, 'find', '--value', NAME, '--contains'],
                        capture_output=True, text=True, timeout=60)
    frame = out / (label + '.png')
    shot = subprocess.run([str(HERE / 'shot.sh'), vm, str(frame)], capture_output=True, text=True, timeout=120)
    if shot.returncode:
        return {'ocr_found': False, 'frame': None, 'ax_found': ax.returncode == 0 and NAME in ax.stdout,
                'frame_error': (shot.stderr or shot.stdout).strip()[-300:]}
    found, detail = read_name(frame, out, label)
    return {'ocr_found': found, 'ocr_detail': detail, 'frame': str(frame), 'frame_error': None,
            'ax_found': ax.returncode == 0 and NAME in ax.stdout}


def loro_lines(vm, log):
    _, text = run_in_guest(vm, 'grep -a "\\[richos\\] memory:" ' + shlex.quote(log) + ' 2>/dev/null || true')
    return [line for line in text.splitlines() if line.strip()]


def write_set(vm, home):
    """The import's own write set, read back: config.json, loro-root, and the backups folder."""
    code = (
        'import hashlib,json,os,sys\n'
        'h=sys.argv[1];d=h+"/Library/Application Support/com.richos.app";l=h+"/Library/Application Support/RichOS/loro-root"\n'
        'c=d+"/config.json";b=d+"/import-backups"\n'
        'r={"config":[os.stat(c).st_mtime_ns,hashlib.sha256(open(c,"rb").read()).hexdigest()] if os.path.exists(c) else None,'
        '"link":[os.readlink(l),os.lstat(l).st_mtime_ns] if os.path.islink(l) else None,'
        '"backups":sorted(os.path.join(p,n) for p,ds,fs in os.walk(b) for n in ds+fs) if os.path.isdir(b) else []}\n'
        'print(json.dumps(r))\n'
    )
    rc, text = run_in_guest(vm, 'python3 -c ' + shlex.quote(code) + ' ' + shlex.quote(home))
    return json.loads(text) if rc == 0 else {'error': text}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    vm = a.vm
    state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / vm
    payload = (state / 'payload').read_text().strip()
    if not payload.startswith('/Users/') or '/testvm/' not in payload:
        raise SystemExit('not a guest payload: ' + payload)
    pid = (state / 'app.pid').read_text().strip()
    if not pid.isdigit():
        raise SystemExit('recorded app pid is not numeric')
    home = payload + '/home'
    record = home + '/ab/record'
    team = home + '/ab/team'
    script = payload + '/import-setup.py'
    rec = {'vm': vm, 'started': utc(), 'app_pid_at_boot': int(pid), 'checks': {}}
    checks = rec['checks']

    def cli(*extra):
        args = ['python3', script, '--home', home, '--record', record, '--name', NAME, '--team-folder', team] + list(extra)
        return run_in_guest(vm, shlex.join(args))

    # ---- stage the stand-in for his setup, in the guest -----------------------------------
    subprocess.run([str(HERE / 'guest.sh'), vm, '--push', str(IMPORT), script], check=True, timeout=120)
    enc = ''.join(ch if ch.isalnum() and ch.isascii() else '-' for ch in team)
    stage = ' && '.join([
        'mkdir -p ' + shlex.quote(record + '/wiki') + ' ' + shlex.quote(record + '/loro') + ' ' + shlex.quote(team),
        'printf "# Team roster\\n\\nFrank: devil\'s advocate.\\n" > ' + shlex.quote(record + '/wiki/team-roster.md'),
        'mkdir -p ' + shlex.quote(home + '/.claude/projects/' + enc + '/memory'),
        'printf -- "- [a memory](a.md)\\n" > ' + shlex.quote(home + '/.claude/projects/' + enc + '/memory/MEMORY.md'),
    ])
    rc, text = run_in_guest(vm, stage)
    if rc:
        raise SystemExit('could not stage the fixture in the guest: ' + text)

    # ---- W1: before ----------------------------------------------------------------------
    # 10 s since the app started, kept on purpose: past the splash has no signal a walk can read (run.sh
    # has already required a window; the splash is a window). The check is that the name is ABSENT, so a
    # poll for a change cannot replace it. What the boot and the staging above already spent counts
    # (R41), because the margin is about the app's age, not about this line. Recorded, not a verdict.
    w1_margin = margin.wait_since_start(vm, int(pid), 10)
    before_screen = name_on_screen(vm, a.out, 'w1-before')
    before_log = loro_lines(vm, payload + '/app.log')
    checks['W1'] = {
        'name_absent_on_screen': not before_screen['ocr_found'],
        'frame_read': before_screen['frame'] is not None,
        'boot_log_no_memory': any('no corpus configured' in x for x in before_log),
        'screen': before_screen, 'boot_log': before_log, 'margin': w1_margin,
    }

    # ---- W2: plan only -------------------------------------------------------------------
    ws0 = write_set(vm, home)
    rc, text = cli()
    ws1 = write_set(vm, home)
    checks['W2'] = {'exit': rc, 'plan_only': 'PLAN ONLY' in text, 'nothing_written': ws0 == ws1, 'output': text}

    # ---- W3: refused while the app runs --------------------------------------------------
    rc, text = cli('--go')
    ws2 = write_set(vm, home)
    checks['W3'] = {'exit': rc, 'refused': 'REFUSED' in text and ('process ' + pid) in text,
                    'nothing_written': ws1 == ws2, 'output': text}

    # ---- W4: stop the recorded app, import ----------------------------------------------
    comm = guest(vm, 'ps -p ' + pid + ' -o comm= 2>/dev/null || true')
    if not (payload in comm and comm.endswith('/richos-tauri')):
        raise SystemExit('recorded pid does not belong to this payload: ' + repr(comm))
    guest(vm, 'kill -TERM ' + pid + ' 2>/dev/null || true')
    for _ in range(50):
        if guest(vm, 'kill -0 ' + pid + ' 2>/dev/null && echo alive || true') != 'alive':
            break
        time.sleep(0.2)
    else:
        guest(vm, 'kill -KILL ' + pid + ' 2>/dev/null || true')
    stopped = guest(vm, 'kill -0 ' + pid + ' 2>/dev/null && echo alive || echo gone')
    rc, text = cli('--go')
    ws3 = write_set(vm, home)
    _, config = run_in_guest(vm, 'cat ' + shlex.quote(home + '/Library/Application Support/com.richos.app/config.json'))
    checks['W4'] = {'app_stopped': stopped == 'gone', 'exit': rc, 'done': 'IMPORT DONE AND VERIFIED' in text,
                    'link': ws3.get('link'), 'config': config, 'backups': ws3.get('backups'), 'output': text}

    # ---- W5: again ------------------------------------------------------------------------
    rc, text = cli('--go')
    ws4 = write_set(vm, home)
    checks['W5'] = {'exit': rc, 'nothing_to_change': 'Nothing to change' in text, 'write_set_unchanged': ws3 == ws4,
                    'output': text}

    # ---- W6: relaunched, on screen -------------------------------------------------------
    relaunched = relaunch(vm)
    rec['app_pid_relaunched'] = relaunched['pid']
    found = {'ax_found': False, 'ocr_found': False}
    deadline = time.monotonic() + 120
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        found = name_on_screen(vm, a.out, 'w6-after-%d' % attempt)  # look first; sleep only between looks
        if found['ocr_found']:
            break
        time.sleep(8)
    after_log = loro_lines(vm, relaunched['log'])
    checks['W6'] = {
        'name_on_screen_ocr': found['ocr_found'], 'name_in_ax_tree_recorded_only': found['ax_found'],
        # The resolver hands back the POINTER, not its target (loro.rs resolve_corpus pushes
        # `support.join("loro-root")`), so the boot line names .../RichOS/loro-root.
        'boot_log_names_record': any('RichOS/loro-root' in x for x in after_log)
                                 and not any('no corpus configured' in x for x in after_log),
        'screen': found, 'boot_log': after_log, 'attempts': attempt,
    }

    passed = {
        'W1': checks['W1']['name_absent_on_screen'] and checks['W1']['frame_read'] and checks['W1']['boot_log_no_memory'],
        'W2': checks['W2']['exit'] == 0 and checks['W2']['plan_only'] and checks['W2']['nothing_written'],
        'W3': checks['W3']['exit'] == 1 and checks['W3']['refused'] and checks['W3']['nothing_written'],
        'W4': checks['W4']['app_stopped'] and checks['W4']['exit'] == 0 and checks['W4']['done']
              and bool(checks['W4']['link']) and checks['W4']['link'][0] == record,
        'W5': checks['W5']['exit'] == 0 and checks['W5']['nothing_to_change'] and checks['W5']['write_set_unchanged'],
        'W6': checks['W6']['name_on_screen_ocr'] and checks['W6']['boot_log_names_record'],
    }
    rec['passed'] = passed
    rec['ended'] = utc()
    (a.out / 'import-walk.json').write_text(json.dumps(rec, indent=2) + '\n')
    print(json.dumps(passed))
    return 0 if all(passed.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
