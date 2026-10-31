#!/usr/bin/env python3
"""woken-turn-walk.py — Rich starts a background command, his turn ends, the command finishes,
and he carries on in the turn Claude Code starts for it (Hank's app-terminal finding 2,
2026-10-31; CEO §154).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      woken-turn-walk.py --out DIR --expect-sha SHA

THE QUESTION. In the terminal, a background command that ends after the lead's turn wakes the
lead and the lead carries on. In the app Claude Code wakes Rich too, and until the fix the app's
own hook refused every tool he called in that turn ("This app turn is stopped or is supplying
context"; measured in the guest for 958f4a277).

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: memory setup declined, company "Acme" registered, questions declined
  connect    command-walk.py's: the Acme folder connected
  task       typed into the composer: start `sleep N; echo MARKER-done` yourself in the background,
             end your turn, and when it has finished run `echo MARKER-carried-on` yourself
  woken      within --within + N seconds, in Rich's own session (no agent_id), in this order: the
             background launch (run_in_background), the end of his turn (a Stop callback), then a
             Bash run that printed MARKER-carried-on. No tool call of the guest was refused with
             the turn gate's sentence. Rich's words since the send are kept (woken.json).

Evidence in --out: report.json, woken.json, screenshots, the app log, the hook callbacks and every
refused tool call word for word (refusals.txt). Exit 0 when every step passes. Every app instance
is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('toolchain_ask_walk', HERE / 'toolchain-ask-walk.py')
toolchain_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(toolchain_walk)
StepFailed = toolchain_walk.StepFailed

STEPS = ['identity', 'first-run', 'connect', 'task', 'woken']
TASK = ('Please do this yourself with your own shell tool, in my Acme folder: start this harmless test '
        'command with run_in_background set to true, so you do not wait for it, and end your turn right '
        'away with one short sentence saying it has started: sleep {seconds}; echo {marker}-done '
        'When it has finished, run this yourself with your shell tool and tell me the line it printed: '
        'echo {marker}-carried-on')
GATE = 'This app turn is stopped or is supplying context'


def verdict(rows, marker):
    """rows: [{'session', 'callback'}] in file order. -> (failures, facts). Passes when one session
    with no agent_id launched MARKER-done in the background, then ended a turn, then ran a Bash
    command that printed MARKER-carried-on."""
    by_session = {}
    for row in rows:
        by_session.setdefault(row['session'], []).append(row['callback'])
    for session, calls in by_session.items():
        launch = stop = carried = None
        for i, c in enumerate(calls):
            if c.get('agent_id'):
                continue
            event, tool = c.get('hook_event_name'), c.get('tool_name')
            command = (c.get('tool_input') or {}).get('command', '') if tool == 'Bash' else ''
            if (launch is None and event == 'PostToolUse' and marker + '-done' in command
                    and (c.get('tool_input') or {}).get('run_in_background') is True):
                launch = i
            elif launch is not None and stop is None and event == 'Stop':
                stop = i
            elif stop is not None and event == 'PostToolUse' and marker + '-carried-on' in command:
                response = c.get('tool_response') or {}
                out = response.get('stdout', '') if isinstance(response, dict) else str(response)
                if marker + '-carried-on' in out:
                    carried = i
                    break
        if launch is not None:
            facts = {'session': session, 'launch_index': launch, 'turn_end_index': stop, 'carried_on_index': carried}
            failures = []
            if stop is None:
                failures.append('his turn never ended after the background launch')
            elif carried is None:
                failures.append('after his turn ended, no Bash run of his printed %s-carried-on' % marker)
            return failures, facts
    return ['no Bash call of Rich\'s own started %s-done in the background' % marker], {}


class WokenTurnWalk(toolchain_walk.ToolchainAskWalk):
    def __init__(self, a):
        super().__init__(a)
        self.facts.setdefault('marker', 'woken-' + uuid.uuid4().hex[:8])
        self.save()

    def task(self):
        self.facts['sent_ms'] = self.send(TASK.format(seconds=self.a.command_seconds, marker=self.facts['marker']))
        self.save()
        return {'sent_between_guest_ms': [round(v) for v in self.facts['sent_ms']], 'marker': self.facts['marker']}

    def callbacks(self):
        script = ('import json,glob,os,sys\n'
                  'out=[]\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl")):\n'
                  '    s=os.path.basename(os.path.dirname(p))\n'
                  '    for l in open(p, errors="replace"):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except ValueError: continue\n'
                  '        if c.get("hook_event_name") in ("PostToolUse","Stop") and (c.get("tool_name") in (None,"Bash")):\n'
                  '            r=c.get("tool_response")\n'
                  '            if isinstance(r,dict): r={"stdout":str(r.get("stdout",""))[:2000]}\n'
                  '            out.append({"session":s,"callback":{"hook_event_name":c.get("hook_event_name"),\n'
                  '                "tool_name":c.get("tool_name"),"agent_id":c.get("agent_id"),\n'
                  '                "tool_input":c.get("tool_input") or {},"tool_response":r}})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def gate_refusals(self):
        script = ('import glob,sys\n'
                  'n=0\n'
                  'for p in glob.glob(sys.argv[1]+"/.claude/projects/*/*.jsonl")+glob.glob(sys.argv[1]+"/.claude/projects/*/*/*.jsonl"):\n'
                  '    n+=sum(1 for line in open(p, errors="replace") if sys.argv[2] in line and "tool_result" in line)\n'
                  'print(n)\n')
        return int(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.home) + ' '
                         + shlex.quote(GATE), 60) or 0)

    def woken(self):
        sent = self.facts.get('sent_ms')
        if not sent:
            raise StepFailed('the task step must have run (no send time on record)')
        marker = self.facts['marker']
        end = time.monotonic() + self.a.within + self.a.command_seconds
        failures, facts = ['not read yet'], {}
        while time.monotonic() < end:
            failures, facts = verdict(self.callbacks(), marker)
            if not failures:
                break
            time.sleep(5)
        turns = toolchain_walk.turns_after(self.ledger_rows(), sent[0])
        refused = self.gate_refusals()
        evidence = {'marker': marker, 'order': facts, 'gate_refusals': refused,
                    'rich_turns_since_send': [t['text'][:600] for t in turns]}
        (self.out / 'woken.json').write_text(json.dumps(evidence, indent=2) + '\n')
        self.shot('woken.png')
        if refused:
            failures = [f for f in failures if f != 'not read yet'] + ['%d tool call(s) refused with "%s"' % (refused, GATE)]
        if failures:
            raise StepFailed('; '.join(failures))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help='seconds for Rich\'s turns, beyond the command')
    p.add_argument('--command-seconds', type=int, default=40, help='how long the background command sleeps')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = WokenTurnWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
