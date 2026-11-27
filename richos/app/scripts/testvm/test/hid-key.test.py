#!/usr/bin/env python3
"""hid-key.py without posting anything: the flags it would post, each refusal, and the refusal to
post outside a virtual machine (this test never runs where a key could land in a person's typing)."""
import json
import subprocess
import sys
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / 'hid-key.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


def tool(*args):
    r = subprocess.run([sys.executable, str(TOOL), *args], capture_output=True, text=True)
    return r.returncode, (json.loads(r.stdout) if r.stdout.strip().startswith('{') else {}), r.stderr


code, out, _ = tool('117', '--fn', '--keypad', '--char', 'f728', '--dry-run')
check('Forward Delete as a Mac keyboard sends it: key 117, Fn and keypad flags, U+F728, at the HID tap',
      code == 0 and out == {'posted': False, 'tap': 'hid', 'keycode': 117, 'flags': (1 << 23) | (1 << 21),
                            'char': 'f728', 'times': 1}, (code, out))
code, out, _ = tool('45', '--cmd', '--shift', '--dry-run')
check('Command-Shift-N carries the Command and Shift flags and no character', code == 0 and out['flags'] == (1 << 20) | (1 << 17)
      and out['char'] is None, out)
for bad, why in ((['200', '--dry-run'], '0..127'), (['117', '--times', '0', '--dry-run'], '1..20'),
                 (['117', '--char', 'zz', '--dry-run'], 'hexadecimal')):
    code, out, err = tool(*bad)
    check('refused with a sentence: %s' % ' '.join(bad), code == 2 and why in err and not out, (code, err))
vm = subprocess.run(['sysctl', '-n', 'kern.hv_vmm_present'], capture_output=True, text=True).stdout.strip()
if vm != '1':
    code, out, err = tool('117')
    check('outside a virtual machine it posts nothing and says so: exit 2', code == 2 and 'outside a virtual machine' in err
          and not out, (code, err))

print('hid-key.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)
