#!/usr/bin/env python3
"""steps-walk.py: a list of steps runs in order, each is recorded, the first failure stops the walk
unless that step says allow_fail, and an invalid plan (empty, unknown op, bad argument) is refused
before the first step, by execution and by run-walk.py before any guest is booted (R37). A typed
message that carries a quote or backslash reaches AppleScript escaped (V04).
Only the steps that need no guest are used."""
import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / 'steps-walk.py'
RUN_WALK = TOOL.parent / 'run-walk.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


def walk(tmp, name, steps):
    plan, out = Path(tmp) / (name + '.json'), Path(tmp) / name
    plan.write_text(json.dumps(steps))
    r = subprocess.run([sys.executable, str(TOOL), 'no-such-vm', '--steps', str(plan), '--out', str(out)],
                       capture_output=True, text=True)
    record = json.loads((out / 'steps.json').read_text()) if (out / 'steps.json').exists() else []
    return r.returncode, record, r.stdout + r.stderr


with tempfile.TemporaryDirectory(prefix='steps-walk-test-') as tmp:
    rc, rec, said = walk(tmp, 'ok', [{'op': 'wait', 'seconds': 0}, {'op': 'wait', 'seconds': 0}])
    check('two passing steps exit 0 and both are recorded', rc == 0 and len(rec) == 2, (rc, rec, said))

    # A step that fails at run time without a guest: an `until` whose time is already up.
    failing = {'op': 'until', 'args': [], 'contains': 'x', 'seconds': 0}
    rc, rec, said = walk(tmp, 'stop', [failing, {'op': 'wait', 'seconds': 0}])
    check('a failing step fails the walk and stops it before the next step',
          rc == 1 and len(rec) == 1 and rec[0]['exit'] == 1, (rc, rec, said))

    rc, rec, said = walk(tmp, 'allow', [dict(failing, allow_fail=True), {'op': 'wait', 'seconds': 0}])
    check('allow_fail records the failure and carries on; the walk still exits 0',
          rc == 0 and len(rec) == 2 and rec[0]['exit'] == 1 and rec[1]['exit'] == 0, (rc, rec, said))

    # R37: execution enforces the validator --check runs.
    for name, plan in [('empty', []), ('unknown', [{'op': 'nonsense'}, {'op': 'wait', 'seconds': 0}]),
                       ('badtype', [{'op': 'wait', 'seconds': 0}, {'op': 'wait', 'seconds': '6'}]),
                       ('notlist', {'op': 'wait', 'seconds': 0})]:
        rc, rec, said = walk(tmp, 'refuse-' + name, plan)
        check('R37 an invalid plan (%s) is refused (exit 2) before any step runs, nothing recorded' % name,
              rc == 2 and rec == [], (rc, rec, said))

    # R37: run-walk.py refuses the same plan before a slot or a boot (argparse error, exit 2).
    plan = Path(tmp) / 'run-walk-empty.json'
    plan.write_text('[]')
    r = subprocess.run([sys.executable, str(RUN_WALK), '--no-app', '--home', tmp, '--engine', tmp,
                        '--report', str(Path(tmp) / 'report.json'), '--', str(TOOL), '--steps', str(plan), '--out', tmp],
                       capture_output=True, text=True)
    check('R37 run-walk.py refuses an empty step list before any guest is booted',
          r.returncode == 2 and 'refused before any guest is booted' in r.stderr and not (Path(tmp) / 'report.json').exists(),
          (r.returncode, r.stderr))

    # V04: a quote or backslash in a typed message is escaped for the AppleScript literal.
    spec = importlib.util.spec_from_file_location('steps_walk_under_test', TOOL)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    for text, literal in [('Say "hello"', 'Say \\"hello\\"'), ('a\\b', 'a\\\\b'), ('plain', 'plain')]:
        got = mod.keys_script(text)
        check('V04 keys_script(%r) types the text as written' % text,
              got == 'tell application "System Events" to keystroke "%s"' % literal, got)
    seen = []
    mod.run = lambda args, timeout=120: (seen.append(args) or (0, ''))
    mod.step_run('no-such-vm', {'op': 'keys', 'text': 'Say "hi"'}, Path(tmp))
    check('V04 the keys step hands guest.sh the escaped script', seen and seen[0][-1].endswith('keystroke "Say \\"hi\\""'), seen)

    # A push names its source relative to the repository root (the folder holding `richos/`), so a
    # committed step list carries no home path. Candidate 49's walk: it resolved one folder too high,
    # and every relative push failed "no such file on this Mac".
    seen.clear()
    mod.step_run('no-such-vm', {'op': 'push', 'src': 'richos/app/scripts/qa/frame.py', 'dest': '/tmp/frame.py'}, Path(tmp))
    pushed = Path(seen[0][-2]) if seen else None
    check('a relative push source resolves inside this repository and names a file that exists',
          pushed is not None and pushed.exists() and pushed == TOOL.parents[4] / 'richos/app/scripts/qa/frame.py', seen)
    seen.clear()
    mod.step_run('no-such-vm', {'op': 'push', 'src': '/abs/where.txt', 'dest': '/tmp/x'}, Path(tmp))
    check('an absolute push source is used as written', seen and seen[0][-2] == '/abs/where.txt', seen)

    # S8: a relaunch is a step, so "the choice survives a relaunch" is a walk's data, not a new script.
    seen.clear()
    mod.step_run('no-such-vm', {'op': 'relaunch'}, Path(tmp))
    check('relaunch runs relaunch.py on the walk\'s own VM',
          len(seen) == 1 and seen[0][0].endswith('/relaunch.py') and seen[0][1:] == ['no-such-vm'], seen)
    check('relaunch is a valid step with no arguments', mod.problems([{'op': 'relaunch'}]) == [], mod.problems([{'op': 'relaunch'}]))
    mod.run = lambda args, timeout=120: (2, 'recorded app PID no longer belongs to this payload')
    code, text = mod.step_run('no-such-vm', {'op': 'relaunch'}, Path(tmp))
    check('a relaunch that fails fails its step, with relaunch.py\'s sentence', code == 2 and 'no longer belongs' in text, (code, text))

    # A real mouse click is a step: `ax click --at` is an accessibility press at a point, which does
    # not focus a text box (candidate 50 rewalk, run K). The same point twice is a click.
    seen.clear()
    mod.run = lambda args, timeout=120: (seen.append(args) or (0, ''))
    mod.step_run('no-such-vm', {'op': 'pointer', 'points': ['850,902', '850,902']}, Path(tmp))
    check('pointer runs pointer-drag.sh on the walk\'s own VM with the points as given',
          len(seen) == 1 and seen[0][0].endswith('/pointer-drag.sh') and seen[0][1:] == ['no-such-vm', '850,902', '850,902'], seen)
    seen.clear()
    mod.step_run('no-such-vm', {'op': 'pointer', 'points': ['1,2', '1,2'], 'right': True}, Path(tmp))
    check('pointer with right adds --right', seen and seen[0][-1] == '--right', seen)
    check('a two-point pointer step is valid', mod.problems([{'op': 'pointer', 'points': ['1,2', '1,2']}]) == [],
          mod.problems([{'op': 'pointer', 'points': ['1,2', '1,2']}]))
    for bad in ([], ['1,2'], ['12', '1,2'], [1, 2], '1,2'):
        got = mod.problems([{'op': 'pointer', 'points': bad}])
        check('a pointer step with points %r is refused with a sentence' % (bad,), bool(got), got)

print('steps-walk.test.py: %d failed' % len(failures))
sys.exit(1 if failures else 0)
