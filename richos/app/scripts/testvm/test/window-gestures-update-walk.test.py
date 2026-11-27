#!/usr/bin/env python3
"""window-gestures-update-walk.py's walker options, refused before any guest is touched."""
import subprocess
import sys
import tempfile
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / 'window-gestures-update-walk.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    for name in ('bundle.zip', 'update.tar.gz', 'update.sig'):
        (root / name).write_text('x')
    good = root / 'good.json'
    good.write_text('[{"op": "wait", "seconds": 1}]')
    bad = root / 'bad.json'
    bad.write_text('[{"op": "no-such-op"}]')
    base = ['no-such-vm', '--bundle', str(root / 'bundle.zip'), '--update', str(root / 'update.tar.gz'),
            '--signature', str(root / 'update.sig'), '--update-version', '9.9.9', '--out', str(root / 'out')]

    def walk(*extra):
        r = subprocess.run([sys.executable, str(TOOL), *base, *extra], capture_output=True, text=True, timeout=60)
        return r.returncode, r.stdout + r.stderr

    code, out = walk('--steps', 'sometime=' + str(good))
    check('an unknown stage is refused with the stages named: exit 2', code == 2 and 'before-download' in out, (code, out))
    code, out = walk('--steps', 'cycle=' + str(root / 'missing.json'))
    check('a missing steps file is refused: exit 2', code == 2 and 'existing FILE' in out, (code, out))
    code, out = walk('--steps', 'cycle=' + str(good), '--steps', 'cycle=' + str(good))
    check('a stage given twice is refused: exit 2', code == 2 and 'given twice' in out, (code, out))
    code, out = walk('--steps', 'end=' + str(bad))
    check('a steps list that does not check is refused before a guest: exit 2', code == 2 and 'does not check' in out, (code, out))
    code, out = walk('--engine', str(root / 'missing.tar.gz'))
    check('a missing engine archive is refused: exit 2', code == 2 and 'no such file' in out, (code, out))
    check('nothing was written for a refused run', not (root / 'out').exists())

print('window-gestures-update-walk.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)
