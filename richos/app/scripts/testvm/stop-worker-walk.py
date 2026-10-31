#!/usr/bin/env python3
"""stop-worker-walk.py — Stop on Stu's FOREGROUND command ends it, and a worker keeps going (§129).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  TESTVM_APP_MODEL=sonnet run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      stop-worker-walk.py --out DIR --expect-sha SHA

THE QUESTION (2026-10-30, about §129). In the terminal, Esc during a foreground command stops it
and background teammates keep running. §129 forced Stu's own Bash into the background
(`engine/mega-lander/app.py` worker_context) so that Stop could end it by its own task; that is
removed, and Stu's commands run as he issues them. Does the app's Stop end Stu's command when it
runs in the FOREGROUND, while a worker of another job keeps running and still answers? `foreground`
refuses an engine that still forces the background, so the walk can never pass on the background
path by mistake.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity     the running app says it was built from --expect-sha
  first-run    adopt-walk.py's first run: memory setup declined, company Acme with a Git folder
  connect      Settings -> Connected folders -> Acme's folder
  seed         heartbeat.py into the repository (reap-walk.py's helper, unchanged)
  watch        reap-guest.py's death watcher: every recorded heartbeat's start and end, guest ms
  foreground   the engine in the guest does not force Stu's Bash into the background
  worker       job A: one of Stu's helpers runs a 240-s heartbeat (`wk-fg`); the call must be a
               helper's (an agent id on the hook evidence), never Stu's own
  stop         job B: Stu runs a 300-s heartbeat himself, in the foreground (`stop-fg`; his own
               session, no agent id, no run_in_background). Job B's Stop is pressed. stop-fg ends
               within 2 s of the press (reap-walk.py's bound); wk-fg is still beating 3 s later
  answers      wk-fg ends BY ITSELF (.done written, never killed) and its helper reports back:
               the helper's receipt reaches its recorded end; job A is not stopped

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('reap_walk', HERE / 'reap-walk.py')
reap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(reap)
StepFailed, BEATS, BOUND_MS = reap.StepFailed, reap.BEATS, reap.BOUND_MS

STEPS = ['identity', 'first-run', 'connect', 'seed', 'watch', 'foreground', 'worker', 'stop', 'answers']
WORKER_SECONDS = 240
TAIL = ('heartbeat.py is my own harmless test script at the top of that folder: it records its own '
        'process id and writes the time into small files under %s until it ends.' % BEATS)


def worker_ask():
    """Job A, worded so Stu hands it to a helper rather than running it himself."""
    return ('Please give this to one of your helpers (a worker agent), and have that helper run this harmless '
            'test command in my Acme folder in the foreground and wait for it, then tell me what it printed: '
            'python3 heartbeat.py beat wk-fg %s --seconds %d   %s' % (BEATS, WORKER_SECONDS, TAIL))


def forces_background(app_py):
    """Does this engine's app.py still rewrite Stu's own Bash to run_in_background (§129)? Only
    `worker_context`'s branch for a call with no agent id counts; a worker's own dispatch is
    backgrounded elsewhere in app.py and stays so."""
    start = app_py.find('def worker_context(')
    if start < 0:
        return True  # cannot tell, so never treated as switched off
    end = app_py.find('with locked(scope)', start)
    return '"run_in_background":True' in app_py[start:end if end > 0 else None].replace(' ', '')


class Walk(reap.Walk):
    def calls(self, tag):
        """Every Bash call that started one of `tag`'s heartbeats, with whose it was (role,
        session, helper agent) and its input as the hook saw it."""
        script = ('import json,glob,sys\n'
                  'root=sys.argv[1]+"/engine-state"\n'
                  'out=[]\n'
                  'for p in glob.glob(root+"/evidence/*/callbacks.jsonl"):\n'
                  '    try: role=json.load(open(p.rsplit("/",1)[0]+"/binding.json")).get("role")\n'
                  '    except Exception: role=None\n'
                  '    for line in open(p,errors="replace"):\n'
                  '        if "heartbeat.py" not in line: continue\n'
                  '        try: c=json.loads(line).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        if c.get("tool_name")!="Bash" or c.get("hook_event_name")!="PreToolUse": continue\n'
                  '        out.append({"role":role,"session":c.get("session_id"),"agent":c.get("agent_id"),\n'
                  '                    "tool_use_id":c.get("tool_use_id"),"input":c.get("tool_input") or {}})\n'
                  'print(json.dumps(out))\n')
        rows = json.loads(guest(self.vm, 'python3 -c %s %s' % (shlex.quote(script), shlex.quote(self.data)), 60))
        return [r for r in rows if reap.runs_heartbeat(str(r['input'].get('command', '')), tag)]

    def wait_recorded(self, name, sent, within=420):
        end, approved = time.monotonic() + within, 0
        while time.monotonic() < end:
            if reap.recorded(self.rows(), [name], sent):
                return approved
            approved += self.approve_pending()
            time.sleep(3)
        raise StepFailed('prerequisite unavailable: %s never recorded itself within %d s\n%s'
                         % (name, within, self.not_started(name.split('-')[0], sent)))

    def beat_ms(self, name):
        return self.file_ms('%s/%s.beat' % (BEATS, name))

    # --- steps --------------------------------------------------------------------------------
    def foreground(self):
        path = self.payload + '/engine/mega-lander/app.py'
        text = guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true', 60)
        if not text.strip():
            raise StepFailed('REFUSED: the guest has no engine app.py at %s; give run-walk.py --engine' % path)
        if forces_background(text):
            raise StepFailed('REFUSED: the engine under test still forces Stu\'s Bash into the background (§129); '
                             'this walk needs an engine with that switched off for the test only')
        return {'engine_app_py': path, 'forces_background': False}

    def worker(self):
        sent = self.now()
        self.ask(worker_ask())
        approved = self.wait_recorded('wk-fg', sent)
        calls = self.calls('wk')
        print('worker_calls %s' % json.dumps(calls), flush=True)
        if not calls or not all(c.get('agent') for c in calls):
            raise StepFailed('REFUSED: wk-fg was not started by a helper, so no worker is running: %s' % json.dumps(calls))
        assignments = self.assignments_since(sent)
        self.facts['worker'] = {'sent_ms': sent, 'agent': calls[0]['agent'], 'assignments': assignments}
        self.save()
        return {'approved': approved, 'calls': calls, 'assignments': assignments}

    def stop(self):
        if 'worker' not in self.facts:
            raise StepFailed('REFUSED: the worker step did not run')
        sent = self.now()
        self.ask(reap.ask_text('stop', background=False))
        approved = self.wait_recorded('stop-fg', sent)
        calls = self.calls('stop')
        print('stop_calls %s' % json.dumps(calls), flush=True)
        own = [c for c in calls if c.get('role') == 'stu' and not c.get('agent')]
        if not own:
            raise StepFailed('REFUSED: stop-fg was not run by Stu himself, so this is not his foreground command: %s'
                             % json.dumps(calls))
        if any(c['input'].get('run_in_background') for c in own):
            raise StepFailed('REFUSED: Stu\'s stop-fg call ran in the background, not the foreground: %s' % json.dumps(own))
        if self.file_ms(BEATS + '/wk-fg.done') is not None:
            raise StepFailed('REFUSED: the worker\'s wk-fg had already finished before Stop; nothing to keep alive')
        target, why = reap.stoppable(self.assignments_since(sent))
        if why:
            raise StepFailed('%s\n%s' % (why, self.stop_evidence()))
        self.wait_for('Open the work summary')
        self.press('Open the work summary')
        press = reap.pressed_at(self.press('Stop ' + (target.get('title') or '')[:60]))
        try:
            ended = self.verdict(['stop-fg'], press['pressed_at_ms'], BOUND_MS)
        except StepFailed as exc:
            raise StepFailed('%s\n%s' % (exc, self.stop_evidence())) from exc
        time.sleep(2)
        beat, gone = self.beat_ms('wk-fg'), [r for r in self.rows() if r.get('kind') == 'gone' and r.get('name') == 'wk-fg']
        if gone or beat is None or beat < press['pressed_at_ms'] + 2500:
            raise StepFailed('the worker\'s wk-fg stopped when Stu\'s foreground command was stopped: last beat %s, '
                             'pressed at %d, gone rows %s' % (beat, press['pressed_at_ms'], json.dumps(gone)))
        record = self.closed_record(target['id'])
        told = reap.told_stopped(record)
        if told:
            raise StepFailed(told)
        self.facts['stopped'] = target['id']
        self.save()
        return dict(ended, press=press, approved=approved, calls=own, worker_beat_ms=beat,
                    stopped=target['id'], detail=record.get('detail'))

    def answers(self):
        worker = self.facts.get('worker')
        if not worker:
            raise StepFailed('REFUSED: the worker step did not run')
        end = time.monotonic() + WORKER_SECONDS + 120
        while time.monotonic() < end and self.file_ms(BEATS + '/wk-fg.done') is None:
            self.approve_pending()
            time.sleep(5)
        evidence, failure = reap.grade_normal(self.rows(), 'wk-fg', self.file_ms(BEATS + '/wk-fg.done'))
        if failure:
            raise StepFailed(failure)
        # The helper reports back: its receipt reaches its recorded end after the command finished.
        script = ('import json,glob,sys\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/work-receipts/*/*.json"):\n'
                  '    try: r=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    if r.get("agent_id")==sys.argv[2]: print(json.dumps({k:r.get(k) for k in ("id","status","request")}))\n')
        receipt, end = '', time.monotonic() + 180
        while time.monotonic() < end:
            receipt = guest(self.vm, 'python3 -c %s %s %s' % (shlex.quote(script), shlex.quote(self.data),
                                                               shlex.quote(worker['agent'])), 60).strip()
            if '"run-ended"' in receipt:
                break
            time.sleep(5)
        if '"run-ended"' not in receipt:
            raise StepFailed('the worker never reported back after its command finished: receipt %r' % receipt[:600])
        jobs = {a['id']: a for a in self.assignments_since(worker['sent_ms'])}
        stopped = [a for a in jobs.values() if a['id'] != self.facts.get('stopped') and a.get('state') == 'interrupted']
        if stopped:
            raise StepFailed('job A was stopped too: %s' % json.dumps(stopped))
        return dict(evidence, receipt=receipt, assignments=list(jobs.values()))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--model', default='sonnet', help='ANTHROPIC_MODEL for a relaunched app (default sonnet)')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, StopIteration, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'REFUSED' if str(exc).startswith('REFUSED') else 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
