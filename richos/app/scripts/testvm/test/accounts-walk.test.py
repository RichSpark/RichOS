#!/usr/bin/env python3
"""accounts-walk.sh reaches the conversation on the current start screen (STATUS row R1c).

No guest: the walk is copied into a box beside stand-in guest.sh, ax.sh, shot.sh and relaunch.py,
and `sleep` on PATH returns at once. ax.sh logs every call. The walk must (a) end nonzero, saying
so, when the start screen's "Talk to Rich" is not there; (b) press "Talk to Rich" before it types
the company name, and answer the first-run questions only after that press; (c) press the Settings
rows as AXButton, never AXMenuItem (the rows are buttons on this build)."""
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

WALK = Path(__file__).resolve().parents[1] / 'accounts-walk.sh'
failed = 0


def check(ok, what, detail=''):
    global failed
    print(('  ok    ' if ok else '  FAIL  ') + what + ('' if ok else f'  {detail}'))
    failed += 0 if ok else 1


def run(root, no_door=False):
    d = Path(root)
    shutil.copy2(WALK, d / 'accounts-walk.sh')
    log = d / 'ax.log'
    (d / 'guest.sh').write_text('#!/bin/sh\necho /Users/admin/testvm/data\nexit 0\n')
    refuse = 'case "$*" in *"Talk to Rich"*) exit 1 ;; esac\n' if no_door else ''
    (d / 'ax.sh').write_text(f'#!/bin/sh\necho "$*" >> "{log}"\n{refuse}exit 0\n')
    (d / 'shot.sh').write_text('#!/bin/sh\n: > "$2"\n')
    (d / 'relaunch.py').write_text('def relaunch(*a, **k):\n    return {"pid": 1}\n')
    bin_dir = d / 'bin'
    bin_dir.mkdir()
    (bin_dir / 'sleep').write_text('#!/bin/sh\nexit 0\n')
    for f in ('guest.sh', 'ax.sh', 'shot.sh', 'bin/sleep'):
        (d / f).chmod(0o755)
    env = {**os.environ, 'PATH': f'{bin_dir}:{os.environ["PATH"]}', 'PYTHONDONTWRITEBYTECODE': '1',
           'ACCOUNTS_WALK_UNTIL': 'd13'}
    r = subprocess.run(['/bin/bash', str(d / 'accounts-walk.sh'), 'fixture-vm', str(d / 'out')],
                       env=env, capture_output=True, text=True, timeout=60)
    return r, (log.read_text().splitlines() if log.exists() else [])


with tempfile.TemporaryDirectory() as root:
    r, calls = run(root, no_door=True)
    check(r.returncode != 0 and 'no start screen with Talk to Rich' in r.stdout,
          'no Talk to Rich on the start screen ends the walk nonzero, named',
          f'exit {r.returncode}, {r.stdout[-300:]!r}')
with tempfile.TemporaryDirectory() as root:
    r, calls = run(root)
    door = next((i for i, c in enumerate(calls) if 'click --title Talk to Rich' in c), None)
    typed = next((i for i, c in enumerate(calls) if 'type Northwind Traders' in c), None)
    check(door is not None and typed is not None and door < typed,
          'Talk to Rich is pressed before the company name is typed', f'door {door}, typed {typed}')
    notnow = [i for i, c in enumerate(calls) if 'click --title Not now' in c]
    check(door is not None and all(i > door for i in notnow),
          'every Not now is pressed after the door', f'door {door}, not-now {notnow}')
    rows = [c for c in calls if 'Claude accounts' in c and '--role' in c]
    check(not any('AXMenuItem' in c for c in calls) and any('--role AXButton' in c for c in rows),
          'the Settings rows are pressed as AXButton, never AXMenuItem', f'{rows[:3]}')
print(f'accounts-walk.test.py: {failed} failed')
sys.exit(1 if failed else 0)
