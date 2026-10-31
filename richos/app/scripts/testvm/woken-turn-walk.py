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
             the turn gate's sentence. And his words are in the conversation: a completed turn of
             his in the ledger after the send says MARKER-carried-on (woken.json)
  stop       the same, except the follow-up is a long foreground command (`ping -c 600
             127.0.0.1 > /dev/null; echo MARKER-stop-late`; not `sleep 600`, which Claude Code
             itself refuses to run in the foreground, seen in the guest at 2b19585de). Once his
             session has started it, the walk presses Stop on screen
             ("Stop Rich", shown because that turn is a live turn); within --within seconds the
             ledger records a turn of his stopped, and MARKER-stop-late is never printed
             (stop.json)

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

STEPS = ['identity', 'first-run', 'connect', 'task', 'woken', 'stop']
TASK = ('Please do this yourself with your own shell tool, in my Acme folder: start this harmless test '
        'command with run_in_background set to true, so you do not wait for it, and end your turn right '
        'away with one short sentence saying it has started: sleep {seconds}; echo {marker}-done '
        'When it has finished, run this yourself with your shell tool and tell me the line it printed: '
        'echo {marker}-carried-on')
STOP_TASK = ('Please do this yourself with your own shell tool, in my Acme folder: start this harmless test '
             'command with run_in_background set to true, so you do not wait for it, and end your turn right '
             'away with one short sentence saying it has started: sleep {seconds}; echo {marker}-done '
             'When it has finished, run this yourself with your shell tool in the foreground, not in the '
             'background, and wait for it: ping -c 600 127.0.0.1 > /dev/null; echo {marker}-late')
GATE = 'This app turn is stopped or is supplying context'


def stop_verdict(runs, ledger, marker, pressed_ms):
    """runs: Rich's Bash calls [{'event', 'command', 'stdout'}]; ledger: the conversation ledger's
    rows. -> failures. Passes when a turn was recorded stopped at or after the press and the long
    command never printed its line."""
    failures = []
    if not any(r.get('event') == 'TurnStopped' and (r.get('at') or 0) >= pressed_ms - 1000 for r in ledger):
        failures.append('no turn of his was recorded stopped after Stop was pressed')
    if any(marker + '-late' in (r.get('stdout') or '') for r in runs):
        failures.append('the long command ran on past Stop and printed %s-late' % marker)
    return failures


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
        turns, said = [], False
        while time.monotonic() < end + self.a.within and not failures:
            turns = toolchain_walk.turns_after(self.ledger_rows(), sent[0])
            said = any(marker + '-carried-on' in t['text'] for t in turns)
            if said:
                break
            time.sleep(5)
        if not failures and not said:
            failures = ['his words from the turn after his background command are not in the conversation']
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

    def bash_calls(self):
        """Rich's own Bash calls (no agent_id), started and finished, in file order."""
        script = ('import json,glob,sys\n'
                  'out=[]\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl")):\n'
                  '    for l in open(p, errors="replace"):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except ValueError: continue\n'
                  '        if c.get("tool_name")!="Bash" or c.get("agent_id"): continue\n'
                  '        r=c.get("tool_response")\n'
                  '        out.append({"event":c.get("hook_event_name"),"command":(c.get("tool_input") or {}).get("command",""),\n'
                  '                    "stdout":str(r.get("stdout","")) if isinstance(r,dict) else ""})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 60))

    def stop(self):
        marker = self.facts['marker'] + '-stop'
        sent = self.send(STOP_TASK.format(seconds=self.a.command_seconds, marker=marker))
        end = time.monotonic() + self.a.within + self.a.command_seconds
        started = False
        while time.monotonic() < end and not started:
            started = any(r['event'] == 'PreToolUse' and marker + '-late' in r['command'] for r in self.bash_calls())
            if not started:
                time.sleep(3)
        if not started:
            raise StepFailed('his session never started the follow-up %s-late after the background command' % marker)
        self.wait_for('Stop Rich', seconds=30)
        self.shot('stop-shown.png')
        pressed = self.clock()
        self.press('Stop Rich')
        failures = ['not read yet']
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            failures = stop_verdict(self.bash_calls(), self.ledger_rows(), marker, pressed)
            if not failures:
                break
            time.sleep(5)
        self.shot('stopped.png')
        turns = toolchain_walk.turns_after(self.ledger_rows(), sent[0])
        evidence = {'marker': marker, 'pressed_guest_ms': round(pressed), 'failures': failures,
                    'rich_turns_since_send': [t['text'][:600] for t in turns]}
        (self.out / 'stop.json').write_text(json.dumps(evidence, indent=2) + '\n')
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
