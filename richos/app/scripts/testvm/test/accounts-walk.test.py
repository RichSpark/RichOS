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


def run(root, no_door=False, until='d13'):
    d = Path(root)
    shutil.copy2(WALK, d / 'accounts-walk.sh')
    log = d / 'ax.log'
    (d / 'guest.sh').write_text(f'#!/bin/sh\necho "$*" >> "{d / "guest.log"}"\necho /Users/admin/testvm/data\nexit 0\n')
    refuse = 'case "$*" in *"Talk to Rich"*) exit 1 ;; esac\n' if no_door else ''
    (d / 'ax.sh').write_text(f'#!/bin/sh\necho "$*" >> "{log}"\n{refuse}exit 0\n')
    (d / 'shot.sh').write_text('#!/bin/sh\n: > "$2"\n')
    (d / 'relaunch.py').write_text('def relaunch(*a, **k):\n    return {"pid": 1}\n')
    bin_dir = d / 'bin'
    bin_dir.mkdir()
    (bin_dir / 'sleep').write_text('#!/bin/sh\nexit 0\n')
    for f in ('guest.sh', 'ax.sh', 'shot.sh', 'bin/sleep'):
        (d / f).chmod(0o755)
    env = {**os.environ, 'PATH': f'{bin_dir}:{os.environ["PATH"]}', 'PYTHONDONTWRITEBYTECODE': '1'}
    if until:
        env['ACCOUNTS_WALK_UNTIL'] = until
    else:
        env.pop('ACCOUNTS_WALK_UNTIL', None)
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
# Step 9, item 8 (2026-10-31): a week's usage only goes up, so Home, switched away from at 99% in
# step 6, is never fed 97% again and never chosen; the 97% account is a third one, Side, added from
# the sheet, chosen, and then fed 99% so Rich switches again.
with tempfile.TemporaryDirectory() as root:
    r, calls = run(root, until='')
    guest = (Path(root) / 'guest.log').read_text().splitlines()
    home = [g for g in guest if 'usage-1.json' in g]
    check(not any('"weekly":97' in g for g in home) and sum('"weekly":99' in g for g in home) == 1,
          'Home is fed 99% once (step 6) and never a lower week after it', f'{home}')
    side = [i for i, g in enumerate(guest) if 'claude-accounts/3/usage.json' in g]
    side97 = [i for i in side if '"weekly":97' in guest[i]]
    side99 = [i for i in side if '"weekly":99' in guest[i]]
    check(len(side97) == 1 and len(side99) == 1 and side97[0] < side99[0],
          "Side's week is read at 97%, then at 99%", f'{[guest[i] for i in side]}')
    added = next((i for i, c in enumerate(calls) if 'click --title Add another account' in c), None)
    named = next((i for i, c in enumerate(calls) if 'type Side --title The new account' in c), None)
    chose = next((i for i, c in enumerate(calls) if 'click --title Use Side now' in c), None)
    check(None not in (added, named, chose) and added < named < chose,
          'Side is added from the sheet, named, and then chosen with Use this one now',
          f'add {added}, name {named}, chose {chose}')
    check(not any('Use Home now' in c for c in calls), 'Home, resting at 99%, is never chosen',
          f'{[c for c in calls if "Use Home now" in c]}')
print(f'accounts-walk.test.py: {failed} failed')
sys.exit(1 if failed else 0)
