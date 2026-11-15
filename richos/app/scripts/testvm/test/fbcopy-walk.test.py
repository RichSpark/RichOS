#!/usr/bin/env python3
"""fbcopy-walk.sh's exit status is its verdict (the CEO's feedback of 2026-10-06, items 3 to 7).

No guest: the walk is copied into a box beside stand-in guest.sh, ax.sh, shot.sh and relaunch.py,
and `sleep` on PATH returns at once. ax.sh's `tree` prints one fixed screen text. Every guest
operation failing must end the walk nonzero; a screen holding everything the feedback asks for
exits 0; the same screen with an m-dash on it, or with the splash row back in the quick settings,
exits nonzero and names the item."""
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

WALK = Path(__file__).resolve().parents[1] / 'fbcopy-walk.sh'
GOOD = ('Settings Theme Text size Technical view Company\n'
        'Use Rich from your phone RichConnect for RichOS Set up RichConnect for RichOS\n'
        'Keep the stored output Forever Nothing is ever removed.\n'
        'Play its sound\n')
failed = 0


def check(ok, what, detail=''):
    global failed
    print(('  ok    ' if ok else '  FAIL  ') + what + ('' if ok else f'  {detail}'))
    failed += 0 if ok else 1


def box(root, screen=GOOD, rc=0):
    d = Path(root)
    shutil.copy2(WALK, d / 'fbcopy-walk.sh')
    (d / 'screen.txt').write_text(screen)
    (d / 'guest.sh').write_text(f'#!/bin/sh\nexit {rc}\n')
    (d / 'ax.sh').write_text('#!/bin/sh\n'
                             f'[ {rc} -eq 0 ] || exit {rc}\n'
                             f'[ "$2" = tree ] && cat "{d}/screen.txt"\nexit 0\n')
    (d / 'shot.sh').write_text(f'#!/bin/sh\n[ {rc} -eq 0 ] && : > "$2"\nexit {rc}\n')
    (d / 'relaunch.py').write_text('def relaunch(*a, **k):\n    return {"pid": 1}\n')
    bin_dir = d / 'bin'
    bin_dir.mkdir()
    (bin_dir / 'sleep').write_text('#!/bin/sh\nexit 0\n')
    for f in ('guest.sh', 'ax.sh', 'shot.sh', 'bin/sleep'):
        (d / f).chmod(0o755)
    out = d / 'out'
    env = {**os.environ, 'PATH': f'{bin_dir}:{os.environ["PATH"]}', 'PYTHONDONTWRITEBYTECODE': '1'}
    r = subprocess.run(['/bin/bash', str(d / 'fbcopy-walk.sh'), 'fixture-vm', str(out)],
                       env=env, capture_output=True, text=True, timeout=60)
    verdict = (out / 'verdict.txt').read_text() if (out / 'verdict.txt').exists() else ''
    return r, verdict


with tempfile.TemporaryDirectory() as root:
    r, v = box(root, rc=1)
    check(r.returncode != 0, 'every guest operation failing ends the walk nonzero', f'exit {r.returncode}, {v!r}')
with tempfile.TemporaryDirectory() as root:
    r, v = box(root)
    check(r.returncode == 0 and 'FAIL' not in v and v.count('PASS') == 7,
          'a screen with everything the feedback asks for exits 0, seven PASS lines', f'exit {r.returncode}, {v!r}')
with tempfile.TemporaryDirectory() as root:
    r, v = box(root, screen=GOOD + 'Saved — all good\n')
    check(r.returncode != 0 and 'FAIL  item 3' in v, 'an m-dash on screen fails item 3', f'exit {r.returncode}, {v!r}')
with tempfile.TemporaryDirectory() as root:
    r, v = box(root, screen=GOOD + 'Splash screen\n')
    check(r.returncode != 0 and 'FAIL  item 7' in v, 'a splash row in the quick settings fails item 7', f'exit {r.returncode}, {v!r}')
with tempfile.TemporaryDirectory() as root:
    # The 2026-10-06 guest: every tree read answered with the harness's own error, whose text
    # carries a dash. No item may be judged on it, PASS or FAIL.
    r, v = box(root, screen='{"error": "guest_deadline", "detail": "TCC grant did not take — re-run setup"}\n')
    check(r.returncode != 0 and 'UNKNOWN' in v and 'PASS' not in v and 'FAIL  item' not in v,
          'a tree read that returns the harness error is UNKNOWN, never a verdict', f'exit {r.returncode}, {v!r}')
print(f'fbcopy-walk.test.py: {failed} failed')
sys.exit(1 if failed else 0)
