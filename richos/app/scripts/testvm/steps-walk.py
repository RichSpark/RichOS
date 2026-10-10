#!/usr/bin/env python3
"""steps-walk.py - drive the app booted in this run's guest through a JSON list of steps.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest, runs this
and removes the clone (run-walk.py passes the owned VM name as the first argument):

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TAR --report REPORT -- \\
      steps-walk.py --steps steps.json --out DIR

A step is an object with "op" and its arguments. Every step is recorded in <out>/steps.json with
its exit status, its output and the clock, so a walk's evidence is one file and not a transcript.

  tree   [app]                         ax.sh VM tree, written to <out>/<name>.tree
  shot   name [ocr]                    shot.sh VM <out>/<name>.png (a black frame fails)
  ax     args[]                        ax.sh VM <args...>  (find / click / focus / type)
  guest  command                       guest.sh VM <command>, output recorded
  wait   seconds                       sleep (a plain pause; prefer "until")
  until  args[] contains seconds       repeat ax.sh VM <args...> until its output contains the text
  keys   text                          guest.sh osascript keystroke into the frontmost app
  push   src dest                      guest.sh VM --push src dest (src relative to the repository root, or absolute)
  handfile mode path [to]              hand-file.sh VM paste|drag path [--to x,y]
  relaunch                             relaunch.py VM: the recorded app stopped by its captured PID and
                                       started again with its fixture environment (a choice that must
                                       survive a relaunch is checked after this step)

A step list is checked without a guest: `steps-walk.py --check FILE...` prints each problem (not a
list, an unknown op, a missing or mistyped argument) and exits 1. The committed walk records'
step lists are checked by steps-walk-data.test.sh.

"allow_fail": true records a failing step and carries on; otherwise the first failure stops the walk
(exit 1). The final exit is 0 only when every step that was not allowed to fail passed.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent


def run(args, timeout=120):
    try:
        r = subprocess.run(args, text=True, capture_output=True, timeout=timeout)
        return r.returncode, (r.stdout + r.stderr).strip()
    except subprocess.TimeoutExpired as e:
        return 124, f'timeout after {timeout}s: {e}'


def keys_script(text):
    """The AppleScript that types TEXT. A backslash and a double quote are the two characters an
    AppleScript string literal gives meaning to; each is escaped so a message that carries one
    is typed as written and never ends the literal early."""
    literal = text.replace('\\', '\\\\').replace('"', '\\"')
    return f'tell application "System Events" to keystroke "{literal}"'


def step_run(vm, step, out):
    op = step['op']
    if op == 'tree':
        cmd = [str(HERE / 'ax.sh'), vm, 'tree'] + (['--app', step['app']] if step.get('app') else [])
        code, text = run(cmd)
        (out / f"{step['name']}.tree").write_text(text + '\n')
        return code, f"{len(text.splitlines())} lines"
    if op == 'shot':
        cmd = [str(HERE / 'shot.sh'), vm, str(out / f"{step['name']}.png")] + (['--ocr'] if step.get('ocr') else [])
        return run(cmd)
    if op == 'ax':
        return run([str(HERE / 'ax.sh'), vm] + step['args'])
    if op == 'guest':
        return run([str(HERE / 'guest.sh'), vm, step['command']])
    if op == 'keys':
        return run([str(HERE / 'guest.sh'), vm, 'osascript', '-e', keys_script(step['text'])])
    if op == 'push':
        # A plan names its source relative to the repository root so it is portable and carries no
        # home path; an absolute source is used as written.
        src = Path(step['src'])
        src = src if src.is_absolute() else HERE.parents[4] / src
        return run([str(HERE / 'guest.sh'), vm, '--push', str(src), step['dest']])
    if op == 'handfile':
        return run([str(HERE / 'hand-file.sh'), vm, step['mode'], step['path']] + (['--to', step['to']] if step.get('to') else []))
    if op == 'relaunch':
        return run([str(HERE / 'relaunch.py'), vm], timeout=90)
    if op == 'wait':
        time.sleep(step['seconds'])
        return 0, ''
    if op == 'until':
        deadline = time.monotonic() + step.get('seconds', 30)
        text = ''
        while time.monotonic() < deadline:
            code, text = run([str(HERE / 'ax.sh'), vm] + step['args'])
            if code == 0 and step['contains'] in text:
                return 0, text[:400]
            # ax.sh's own refusal of the request (a bad role, a missing matcher) cannot become true by
            # waiting: fail the step now, so the walk ends and its guest and slot are released.
            if code and '[testvm] ERROR:' in text:
                return code, f'ax.sh refused the request, not retried: {text[:400]}'
            time.sleep(2)
        return 1, f"never saw {step['contains']!r}; last: {text[:400]}"
    return 2, f'unknown op {op!r}'


# op -> {argument: type} every step of that op must carry; OPTIONAL ones are typed when present.
REQUIRED = {
    'tree': {'name': str}, 'shot': {'name': str}, 'ax': {'args': list}, 'guest': {'command': str},
    'keys': {'text': str}, 'push': {'src': str, 'dest': str}, 'handfile': {'mode': str, 'path': str},
    'wait': {'seconds': (int, float)}, 'until': {'args': list, 'contains': str}, 'relaunch': {},
}
OPTIONAL = {'tree': {'app': str}, 'shot': {'ocr': bool}, 'handfile': {'to': str},
            'until': {'seconds': (int, float)}, '*': {'allow_fail': bool}}


def problems(steps):
    """Every defect of a step list, as sentences; an empty list means each step is runnable."""
    if not isinstance(steps, list) or not steps:
        return ['a step list is a non-empty JSON list']
    found = []
    for i, step in enumerate(steps):
        if not isinstance(step, dict):
            found.append(f'step {i}: not an object')
            continue
        op = step.get('op')
        if op not in REQUIRED:
            found.append(f'step {i}: unknown op {op!r}')
            continue
        for key, kind in REQUIRED[op].items():
            if key not in step:
                found.append(f'step {i} ({op}): missing {key!r}')
        for key, kind in {**REQUIRED[op], **OPTIONAL.get(op, {}), **OPTIONAL['*']}.items():
            if key in step and (not isinstance(step[key], kind)
                                or (isinstance(step[key], bool) and kind != bool)):
                found.append(f'step {i} ({op}): {key!r} has the wrong type')
    return found


def plan_problems(path):
    """Every defect of the step list in the file PATH; an unreadable file is a defect too."""
    try:
        return problems(json.loads(Path(path).read_text()))
    except (OSError, ValueError) as e:
        return [f'unreadable: {e}']


def check_files(paths):
    bad = 0
    for path in paths:
        found = plan_problems(path)
        for line in found:
            print(f'{path}: {line}')
        bad += bool(found)
    return 1 if bad else 0


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--check':
        return check_files(sys.argv[2:]) if sys.argv[2:] else 2
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--steps', required=True, type=Path)
    p.add_argument('--out', required=True, type=Path)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    # The same check `--check` runs, enforced before the first step: an empty or malformed plan is
    # refused here, not run to a successful nothing (or to a failure found after a guest was booted).
    found = plan_problems(a.steps)
    if found:
        for line in found:
            print(f'{a.steps}: {line}', file=sys.stderr)
        return 2
    steps = json.loads(a.steps.read_text())
    record, failed = [], False
    for i, step in enumerate(steps):
        started = time.strftime('%H:%M:%SZ', time.gmtime())
        code, text = step_run(a.vm, step, a.out)
        record.append({'i': i, 'step': step, 'at': started, 'exit': code, 'output': text[:2000]})
        print(f'{i:02d} {started} {step["op"]} exit={code} {text[:160]!r}', flush=True)
        (a.out / 'steps.json').write_text(json.dumps(record, indent=2) + '\n')
        if code and not step.get('allow_fail'):
            failed = True
            print(f'FAILED step {i} ({step["op"]}): exit {code}; the walk ends here', file=sys.stderr, flush=True)
            break
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
