#!/usr/bin/env python3
"""context-once-walk.py — in the regular app, Rich's and Stu's inputs carry the host context only
at session start and after a compaction, as the terminal lead's do.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      context-once-walk.py --out DIR --expect-sha SHA

THE QUESTION (richos-hq docs/research/2026-10-31-app-terminal-differences.md finding 6). The
terminal lead gets its continuity block when the session starts and after a compaction, and
nothing per prompt. The app gave Rich and Stu the block once and then a note or its changed lines
on every later input, with the Loro receipts and open questions in full every time. This walk
reads what Claude Code itself recorded as each input's prompt-hook context in the real app.

STEPS, in the guest, never on the host's screen (CEO ruling 65):
  identity, first-run, relaunch, job   stu-reminder-walk.py's: the job opens Stu's session, which
              takes the job and the notices that follow it as inputs
  chat        CHATS more short messages to Rich, each waited for until its turn has closed
  context     Claude Code's own transcripts in the guest (~/.claude/projects/*/<session>.jsonl): for
              each session, in file order, every input (a user row that is not a tool result, or a
              message queued into a running turn) and every UserPromptSubmit hook context Claude
              Code recorded. PASS needs Rich's session (the one holding the chat messages) and Stu's
              job session each to have at least 2 inputs and at most 1 + (compactions) hook
              contexts naming the continuity block at all (the block, an "unchanged" note or its
              changed lines). Evidence: context.json, every input and the start of every context.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
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
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
StepFailed = base.StepFailed

STEPS = ['identity', 'first-run', 'relaunch', 'job', 'chat', 'context']
CHATS = ('Thanks. In one short sentence: is notes.md in the Acme repository now? (walk-chat-1)',
         'And in one short sentence: what is the last commit subject there? (walk-chat-2)')
MARK = 'executive-continuity'

# Read in the guest, over Claude Code's own transcripts: per session, in file order, its inputs (a
# user row that is not a tool result, or a message queued into a running turn) and every
# UserPromptSubmit hook context Claude Code recorded, and its compactions.
SESSIONS = r'''
import glob,json,os,sys
out={}
for p in sorted(glob.glob(sys.argv[1]+"/.claude/projects/*/*.jsonl")):
    events,compactions=[],0
    for line in open(p,errors="replace"):
        try: r=json.loads(line)
        except Exception: continue
        if r.get("type")=="system" and r.get("subtype")=="compact_boundary":
            compactions+=1; events.append({"kind":"compaction","at":r.get("timestamp")}); continue
        a=r.get("attachment") if r.get("type")=="attachment" else None
        if isinstance(a,dict) and a.get("type")=="hook_additional_context" and a.get("hookEvent")=="UserPromptSubmit":
            text="\n".join(map(str,a.get("content") or []))
            events.append({"kind":"context","at":r.get("timestamp"),"chars":len(text),"continuity":sys.argv[2] in text,"start":text[:240]})
            continue
        if isinstance(a,dict) and a.get("type")=="queued_command":
            events.append({"kind":"input","at":r.get("timestamp"),"text":str(a.get("prompt"))[:120]}); continue
        if r.get("type")!="user" or r.get("isMeta") or r.get("isCompactSummary"): continue
        c=(r.get("message") or {}).get("content")
        if isinstance(c,list):
            if any(isinstance(b,dict) and b.get("type")=="tool_result" for b in c): continue
            c="\n".join(b.get("text","") for b in c if isinstance(b,dict))
        events.append({"kind":"input","at":r.get("timestamp"),"text":str(c)[:120]})
    out[os.path.basename(p)[:-6]]={"events":events,"compactions":compactions}
print(json.dumps(out))
'''


def ended(rows, marker):
    """True when the turn whose prompt holds `marker` has a closing row in the app's ledger."""
    turns = {}
    for r in rows:
        tid = r.get('turn_id')
        if not tid:
            continue
        t = turns.setdefault(tid, {'prompt': '', 'open': True})
        if r.get('event') == 'PromptReceived':
            t['prompt'] += str(r.get('text') or '')
        elif r.get('event') in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed'):
            t['open'] = False
    return any(marker in t['prompt'] and not t['open'] for t in turns.values())


def counts(session):
    """(inputs, contexts, contexts naming the block) of one session."""
    events = session['events']
    return (sum(e['kind'] == 'input' for e in events), sum(e['kind'] == 'context' for e in events),
            sum(e['kind'] == 'context' and e['continuity'] for e in events))


def judge(name, session):
    """None when this session's inputs carried the block only where the terminal gives it, else why not."""
    inputs, contexts, carried = counts(session)
    if inputs < 2:
        return '%s had %d input(s); at least 2 are needed to see a later one' % (name, inputs)
    allowed = 1 + session['compactions']
    if carried > allowed:
        return '%s: %d of its %d inputs\' hook contexts carried the continuity block or a note of it (allowed: %d, ' \
               'its start and one per compaction)' % (name, carried, inputs, allowed)
    return None


class ContextOnceWalk(base.StuReminderWalk):
    def ledger(self):
        text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.data + '/conversation-ledger.jsonl'), 120)
        rows = []
        for line in text.splitlines():
            try:
                rows.append(json.loads(line))
            except ValueError:
                continue
        return rows

    def chat(self):
        said = []
        for text in CHATS:
            marker = text[text.rindex('(') + 1:-1]
            self.send(text)
            end = time.monotonic() + self.a.within
            while time.monotonic() < end and not ended(self.ledger(), marker):
                time.sleep(3)
            if not ended(self.ledger(), marker):
                raise StepFailed("Rich's turn for %s did not close within %g s" % (marker, self.a.within))
            said.append(marker)
        return {'closed': said}

    def context(self):
        sessions = json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(SESSIONS), shlex.quote(self.home), shlex.quote(MARK)), 120) or '{}')
        rich = [s for s, v in sessions.items()
                if any(e['kind'] == 'input' and 'walk-chat-1' in e['text'] for e in v['events'])]
        stu = (self.facts.get('job') or {}).get('work_session')
        (self.out / 'context.json').write_text(json.dumps(
            {'rich_session': rich, 'stu_session': stu, 'sessions': sessions}, indent=2) + '\n')
        if len(rich) != 1:
            raise StepFailed('no single session holds the chat messages (found: %s); see context.json' % rich)
        if stu not in sessions:
            raise StepFailed("Stu's job session %s has no transcript in the guest; see context.json" % stu)
        problems = [p for p in (judge('Rich', sessions[rich[0]]), judge('Stu', sessions[stu])) if p]
        if problems:
            raise StepFailed('; '.join(problems) + '; see context.json')
        return {name: dict(zip(('inputs', 'hook_contexts', 'contexts_naming_the_block'), counts(sessions[s])),
                           session=s, compactions=sessions[s]['compactions'])
                for name, s in (('rich', rich[0]), ('stu', stu))}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=base.JOB)
    p.add_argument('--within', type=float, default=300, help="seconds for the walk's other waits")
    p.add_argument('--job-within', type=float, default=1200, help='seconds for the job to be settled')
    p.add_argument('--idle-within', type=float, default=900)
    p.add_argument('--approvals', type=int, default=3, help='most Approve presses on the job')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--reminder-seconds', type=float, default=120)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = ContextOnceWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
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
    try:
        walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
        pass
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
