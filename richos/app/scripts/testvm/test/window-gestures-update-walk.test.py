#!/usr/bin/env python3
"""window-gestures-update-walk.py's walker options, refused before any guest is touched."""
import json
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

    harness = root / 'harness'
    harness.mkdir()
    (harness / 'update-in-place-walk.py').write_text("""
class Walk:
    def __init__(self, args):
        self.vm, self.home, self.work = args.vm, '/fixture-home', '/fixture-work'
        args.out.mkdir()
        self.pid = 1

    def __init_subclass__(cls):
        for name in ('prepare_inputs', 'maximize', 'folder_open', 'folder_delete', 'edge',
                     'keyboard_tile', 'switcher', 'dock_minimize', 'reopen'):
            setattr(cls, name, lambda self, *args, **kwargs: None)
        cls.grant_existing_permission = lambda self: self.pid
        cls.helper_ready = lambda self: self.pid

    def serve(self): pass
    def install(self): pass
    def ready(self): pass
    def download(self): self.pid += 1
    def version(self): return '1.3.8'
    def running_build(self, *_args): return True
    def wait(self, check, *_args): return check()
    def pull(self, *_args): pass
    def cleanup(self): pass
""")
    (harness / 'steps-walk.py').write_text("""
from pathlib import Path
import sys

if sys.argv[1] != '--check':
    out = Path(sys.argv[sys.argv.index('--out') + 1])
    out.mkdir()
    (out / 'steps.json').write_text('[{"i": 0, "exit": 0}]')
""")

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

    cycle = ['--harness', str(harness), '--steps', 'cycle=' + str(good)]
    code, out = walk(*cycle, '--cycles', '-1', '--out', str(root / 'negative'))
    check('negative cycles are refused before setup or the cycle plan can be skipped',
          code == 2 and '--cycles' in out and '0' in out and not (root / 'negative').exists(), (code, out))
    code, out = walk(*cycle, '--cycles', '0')
    report = json.loads((root / 'out' / 'report.json').read_text())
    check('zero restarts still runs cycle zero and its selected steps',
          code == 0 and report.get('cycle-0-steps', {}).get('result', {}).get('steps') == 1
          and 'cycle-1-steps' not in report, (code, out))

print('window-gestures-update-walk.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)
