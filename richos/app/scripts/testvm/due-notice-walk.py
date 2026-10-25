#!/usr/bin/env python3
"""due-notice-walk.py — in the regular app, the due-work notice names a tool Rich has, and he uses it.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      due-notice-walk.py --out DIR --expect-sha SHA

THE QUESTION (CEO ruling 138: everything he uses in the terminal is a feature of the regular app).
While no stall watcher evaluates a conversation, Rich's turn opens with the due work
(app-engine-hook.py held_work_context, workspaces.fallback_notice). Up to engine 95b10c215 that
notice named workspaces.sh commands, which the app's lease PATH does not carry. This walk shows, on
the real app with fixture data in a guest, that the notice names a richos_work tool and Rich acts on
it with that tool.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  identity    the running app says it was built from --expect-sha
  first-run   adopt-walk.py's: company "Acme" with a folder that is a Git repository
  hello       a first message, so Rich's conversation lease is up and its session is registered
              in this conversation's registry partition; waits for the end of his turn
  held        a paused teammate of Rich's session ("notice-mate") is registered in that partition
              (due-notice-guest.py), and the engine's own due set, read with the app's environment,
              names mcp__richos_work__resume_message for it and no workspaces.sh
  ask         a message asking Rich to handle due work himself; waits for the end of his turn
  act         PASS needs: the notice in his transcript names the tool and no workspaces.sh; his
              session called mcp__richos_work__resume_message for notice-mate; and the registry
              record of notice-mate is no longer paused, with a "resumed" entry in its history

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
Evidence in --out: report.json, facts.json, due.json, record.json, transcript-notice.txt, calls.json.
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

_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
stu_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(stu_walk)
StepFailed = stu_walk.StepFailed
command = stu_walk.command_walk.command

STEPS = ['identity', 'first-run', 'hello', 'held', 'ask', 'act']
MATE = 'notice-mate'
TOOL = 'mcp__richos_work__resume_message'
HELLO = 'Hello Rich. This is a short test conversation; a one-line hello back is all I need.'
ASK = 'Please handle any work that is due right now yourself, then tell me in one line what you did.'


class DueNoticeWalk(stu_walk.StuReminderWalk):
    def py(self, script, *args, timeout=60):
        return guest(self.vm, 'python3 -c %s %s' % (shlex.quote(script), ' '.join(map(shlex.quote, args))), timeout)

    def rich_session(self):
        """Rich's session: the evidence binding whose role is rich (engine-state/evidence/<session>)."""
        script = ('import glob,json,os,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/binding.json"):\n'
                  '    try: d=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    if d.get("role")=="rich": out.append((os.path.getmtime(p),os.path.basename(os.path.dirname(p))))\n'
                  'print(sorted(out)[-1][1] if out else "")\n')
        return self.py(script, self.data).strip()

    def stops(self, session):
        """How many Stop hook calls the app recorded for this session: one per finished turn."""
        script = ('import json,sys\n'
                  'n=0\n'
                  'try:\n'
                  '    for l in open(sys.argv[1]+"/engine-state/evidence/"+sys.argv[2]+"/callbacks.jsonl"):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        n+=c.get("hook_event_name")=="Stop"\n'
                  'except OSError: pass\n'
                  'print(n)\n')
        out = self.py(script, self.data, session).strip()
        return int(out) if out.isdigit() else 0

    def calls(self, session, tool):
        script = ('import json,sys\n'
                  'out=[]\n'
                  'try:\n'
                  '    for l in open(sys.argv[1]+"/engine-state/evidence/"+sys.argv[2]+"/callbacks.jsonl"):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        if c.get("tool_name")==sys.argv[3] and not c.get("agent_id"):\n'
                  '            out.append({"event":c.get("hook_event_name"),"input":c.get("tool_input"),\n'
                  '                        "response":str(c.get("tool_response",""))[:1500]})\n'
                  'except OSError: pass\n'
                  'print(json.dumps(out))\n')
        return json.loads(self.py(script, self.data, session, tool) or '[]')

    def turn_ends(self, session, after, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if self.stops(session) > after:
                return True
            time.sleep(5)
        return False

    def hello(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        self.send(HELLO)
        end = time.monotonic() + self.a.within
        session = ''
        while time.monotonic() < end and not session:
            session = self.rich_session()
            time.sleep(3)
        if not session:
            raise StepFailed("no evidence binding with role rich appeared within %g s" % self.a.within)
        if not self.turn_ends(session, 0, self.a.within):
            raise StepFailed("Rich's first turn did not end within %g s" % self.a.within)
        partition = self.partition(session)
        entity = json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/entities.json'), 60))['entities'][0]['id']
        self.facts.update(rich_session=session, partition=partition, entity=entity)
        self.save()
        return {'session': session, 'partition': partition, 'entity': entity}

    def helper(self, *args):
        remote = self.payload + '/due-notice-guest.py'
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'due-notice-guest.py', remote], 300)
        return guest(self.vm, 'python3 %s %s' % (shlex.quote(remote), ' '.join(map(shlex.quote, args))), 60)

    def held(self):
        f = self.facts
        if not f.get('rich_session'):
            raise StepFailed('hello must have run (no Rich session on record)')
        engine = self.payload + '/engine'
        path = self.helper('paused', engine, f['partition'], f['rich_session'], MATE).strip()
        due = json.loads(self.helper('due', engine, f['partition'], f['rich_session'], f['entity'], f['thread']) or '[]')
        (self.out / 'due.json').write_text(json.dumps(due, indent=2) + '\n')
        ours = [d for d in due if d['name'] == MATE]
        if not ours:
            raise StepFailed('the engine does not count %s as due; see due.json' % MATE)
        if TOOL not in ours[0]['command'] or 'workspaces.sh' in ours[0]['command']:
            raise StepFailed('the due command is not the app tool: %r' % ours[0]['command'])
        f['registered'] = path
        self.save()
        return {'record': path, 'command': ours[0]['command']}

    def ask(self):
        f = self.facts
        if not f.get('registered'):
            raise StepFailed('held must have run')
        before = self.stops(f['rich_session'])
        self.send(ASK)
        if not self.turn_ends(f['rich_session'], before, self.a.turn_within):
            raise StepFailed("Rich's turn after the request did not end within %g s" % self.a.turn_within)
        return {'stops_before': before}

    def act(self):
        f = self.facts
        session = f['rich_session']
        found = guest(self.vm, 'find %s -path "*/projects/*" -name %s 2>/dev/null | head -1' % (
            shlex.quote(self.home), shlex.quote(session + '.jsonl')), 60).strip()
        text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(found), 120) if found else ''
        notice = [l for l in text.splitlines() if 'STALL WATCH NOT ACTIVE' in l and MATE in l]
        (self.out / 'transcript-notice.txt').write_text('\n'.join(notice) + '\n')
        calls = self.calls(session, TOOL)
        (self.out / 'calls.json').write_text(json.dumps(calls, indent=2) + '\n')
        rec = json.loads(self.helper('record', self.payload + '/engine', f['partition'], session, MATE) or 'null') or {}
        (self.out / 'record.json').write_text(json.dumps(rec, indent=2) + '\n')
        failures = []
        if not notice:
            failures.append('no line of Rich\'s transcript (%s) carries the due notice naming %s' % (found or 'not found', MATE))
        elif not any(TOOL in l for l in notice) or any('workspaces.sh' in l for l in notice):
            failures.append('the notice in his transcript does not name only the app tool; see transcript-notice.txt')
        if not any((c.get('input') or {}).get('to') == MATE for c in calls):
            failures.append('Rich never called %s for %s; see calls.json' % (TOOL, MATE))
        resumed = [h for h in rec.get('history') or [] if h.get('fact') == 'resumed']
        if rec.get('pause') or not resumed:
            failures.append('the registry still holds %s paused (pause %r, %d resumed entries)'
                            % (MATE, rec.get('pause'), len(resumed)))
        if failures:
            raise StepFailed('; '.join(failures))
        return {'transcript': found, 'notice_lines': len(notice), 'tool_calls': calls, 'resumed': resumed}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help="seconds for the walk's other waits")
    p.add_argument('--turn-within', type=float, default=600, help="seconds for Rich's turn on the request")
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = DueNoticeWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, IndexError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
