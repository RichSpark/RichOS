#!/usr/bin/env python3
"""codex-watch-walk.py — in the regular app, Codex's entries reach Rich by themselves, once each.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      codex-watch-walk.py --out DIR --expect-sha SHA

THE QUESTION (gap X of richos-hq docs/plans/2026-10-21-switching-to-the-app.md, item 5d). Codex
writes to Rich in `to-rich.md` of a channel folder and Rich answers in the `to-codex.md` beside it.
The app runs the engine's codex-watch as its own child (richos-core `codex_watch.rs`) on the user's
channel folders (the CODEX_CHANNELS setting; unset, the conventional
`~/.richos-coordination/rich-codex` and `rich-codex-questions`) and saves each new entry for Rich in
the conversation on screen. This walk shows that in the real app, on fixture data in a guest.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65). The channel is the
conventional folder in the GUEST's home (the setting is unset, as on a new install):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" with a folder that is a Git repository
  channel    the channel's to-rich.md is written with one entry while the app runs: PASS needs
             exactly one row from codex-watch carrying it in THIS conversation's inbox for Rich
             (<app state>/messages/<company>/<conversation>/rich.jsonl)
  relaunch   the app quit and opened again (command-walk.py's): its sessions now have the
             channel folder, and the watcher starts again where it stopped
  append     a second entry is appended: PASS needs exactly one inbox row carrying it, the first
             entry still told once only, and Rich's own turn on it in the conversation ledger (a
             prompt from codex-watch carrying it, ended with a reply)
  answer     typed to Rich: add one line to the to-codex.md beside that to-rich.md. PASS needs the
             line in the guest's to-codex.md
  context    what each input added to Rich's context, from the guest's native transcripts: each
             entry's turn under 1,000 tokens (chars/4), carrying neither the full continuity block
             nor the company material. For one session, run it without relaunch:
             --steps identity,first-run,channel,append,context

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
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
stu_reminder_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(stu_reminder_walk)
StepFailed = stu_reminder_walk.StepFailed

STEPS = ['identity', 'first-run', 'channel', 'relaunch', 'append', 'answer', 'context']
# `context` measures one session, which `relaunch` ends, so it runs only when asked for.
DEFAULT_STEPS = [s for s in STEPS if s != 'context']
CHANNEL = '.richos-coordination/rich-codex'


class CodexWatchWalk(stu_reminder_walk.StuReminderWalk):
    # garbage-walk.py's: OCR reads "to Rich" on the relaunched app's home frame.
    HOME_TEXT = 'to Rich'

    def __init__(self, a):
        super().__init__(a)
        self.channel = self.home + '/' + CHANNEL
        self.marks = self.facts.setdefault('marks', {})

    def entry(self, name):
        """One entry as Codex writes one: a `## <UTC time> — <subject>` header and its lines, with a
        mark no other text in the guest carries."""
        mark = self.marks.setdefault(name, 'codexwalk-%s-%s' % (name, uuid.uuid4().hex[:10]))
        self.save()
        stamp = time.strftime('%Y-%m-%d %H:%MZ', time.gmtime())
        return mark, '## %s — WALK %s\n%s\nSecond line of the %s entry.\n\n' % (stamp, name.upper(), mark, name)

    def write(self, text, append):
        guest(self.vm, 'mkdir -p %s && printf %%s %s %s %s/to-rich.md' % (
            shlex.quote(self.channel), shlex.quote(text), '>>' if append else '>', shlex.quote(self.channel)), 60)

    def inbox(self):
        """THIS conversation's inbox rows for Rich from codex-watch."""
        script = ('import glob,json,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/messages/*/"+sys.argv[2]+"/rich.jsonl"):\n'
                  '    for l in open(p):\n'
                  '        try: r=json.loads(l)\n'
                  '        except Exception: continue\n'
                  '        if r.get("from")=="codex-watch": out.append({"file":p,"text":r.get("text","")})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), shlex.quote(self.data), shlex.quote(self.facts['thread'])), 120) or '[]')

    def ledger(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 120)
        out = []
        for line in rows.splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                continue
        return out

    def told(self, mark, within):
        """Wait up to `within` seconds for an inbox row carrying `mark`, then one look more (a
        second row would come at a later look). The rows carrying it."""
        end = time.monotonic() + within
        rows = []
        while time.monotonic() < end:
            rows = [r for r in self.inbox() if mark in r['text']]
            if rows:
                time.sleep(20)            # a look of 15 s and more: a second telling would be in by now
                return [r for r in self.inbox() if mark in r['text']]
            time.sleep(3)
        return rows

    def channel_step(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        mark, text = self.entry('first')
        self.write(text, append=False)
        rows = self.told(mark, self.a.within)
        (self.out / 'channel.json').write_text(json.dumps({'mark': mark, 'rows': rows}, indent=2) + '\n')
        if len(rows) != 1:
            raise StepFailed('the first entry reached Rich\'s inbox %d time(s), not once; see channel.json' % len(rows))
        return {'mark': mark, 'row': rows[0]}

    def append(self):
        first = self.marks.get('first')
        if not first:
            raise StepFailed('channel must have run (no first entry on record)')
        mark, text = self.entry('second')
        sent = self.clock()
        self.write(text, append=True)
        rows = self.told(mark, self.a.within)
        again = [r for r in self.inbox() if first in r['text']]
        turn, reply, ended = None, '', None
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            ledger = self.ledger()
            prompts = [r for r in ledger if r.get('event') == 'PromptReceived' and r.get('thread_id') == self.facts['thread']
                       and mark in str(r.get('text', '')) and r.get('at', 0) >= sent - 5000]
            if prompts:
                turn = prompts[0]
                reply = ''.join(str(r.get('text', '')) for r in ledger
                                if r.get('event') == 'AssistantDelta' and r.get('turn_id') == turn.get('turn_id'))
                ended = next((r.get('event') for r in ledger if r.get('turn_id') == turn.get('turn_id') and r.get('event')
                              in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed')), None)
                if ended:
                    break
            time.sleep(5)
        evidence = {'mark': mark, 'rows': rows, 'first_entry_rows_after_relaunch': again,
                    'rich_turn': {'turn_id': turn.get('turn_id'), 'source': turn.get('source'),
                                  'prompt': str(turn.get('text', ''))[:2000]} if turn else None,
                    'turn_ended': ended, 'reply': reply[:4000]}
        (self.out / 'append.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if len(rows) != 1:
            raise StepFailed('the appended entry reached Rich\'s inbox %d time(s), not once; see append.json' % len(rows))
        if len(again) != 1:
            raise StepFailed('the first entry was told %d time(s) after the relaunch, not once; see append.json' % len(again))
        if not turn:
            raise StepFailed('no turn of Rich\'s carried the appended entry within %g s; see append.json' % self.a.within)
        if ended != 'TurnCompleted' or not reply.strip():
            raise StepFailed('Rich\'s turn on the entry ended %s with %d characters of reply; see append.json' % (ended, len(reply)))
        return {'mark': mark, 'turn_id': turn.get('turn_id'), 'source': turn.get('source'), 'reply_characters': len(reply)}

    def answer(self):
        mark = self.marks.get('second')
        if not mark:
            raise StepFailed('append must have run (no second entry on record)')
        line = 'walk-ack %s' % mark
        self.send('Codex just wrote to you in %s/to-rich.md. Please answer Codex yourself, now: add the line '
                  '"%s" at the end of the to-codex.md file in that same folder. Nothing else.' % (self.channel, line))
        end = time.monotonic() + self.a.answer_within
        text = ''
        while time.monotonic() < end:
            text = guest(self.vm, 'cat %s/to-codex.md 2>/dev/null || true' % shlex.quote(self.channel), 60)
            if line in text:
                break
            time.sleep(5)
        (self.out / 'to-codex.md').write_text(text)
        if line not in text:
            raise StepFailed('the line did not reach to-codex.md within %g s; see to-codex.md' % self.a.answer_within)
        return {'line': line, 'to_codex_bytes': len(text.encode())}

    def context(self):
        """What each input of the conversation added to Rich's context: every UserPromptSubmit
        `hook_additional_context` in the guest's native transcripts, beside the input it came
        with. PASS needs each codex-watch entry's input to add under 1,000 tokens (chars/4) and to
        carry neither the full continuity block nor the company material (follow-up "Rich's
        context fills with the same injected blocks on every message", 2026-10-30). Run it without
        `relaunch`: a relaunched app is a new session and gives the full blocks once again."""
        marks = {k: v for k, v in self.marks.items() if k in ('first', 'second')}
        if set(marks) != {'first', 'second'}:
            raise StepFailed('channel and append must have run (no entries on record)')
        script = ('import glob,json,sys\n'
                  'marks=json.loads(sys.argv[2])\n'
                  'out=[]\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/.claude/projects/*/*.jsonl")):\n'
                  '    said=""\n'
                  '    for l in open(p):\n'
                  '        try: r=json.loads(l)\n'
                  '        except Exception: continue\n'
                  '        a=r.get("attachment") or {}\n'
                  '        m=r.get("message") if isinstance(r.get("message"),dict) else {}\n'
                  '        c=m.get("content","")\n'
                  '        result=isinstance(c,list) and any(isinstance(x,dict) and x.get("type")=="tool_result" for x in c)\n'
                  '        if r.get("type")=="user" and not result:\n'
                  '            said=json.dumps(c)\n'
                  '        elif a.get("type")=="queued_command":\n'
                  '            said=json.dumps(a)\n'
                  '        elif a.get("type")=="hook_additional_context" and a.get("hookEvent")=="UserPromptSubmit":\n'
                  '            text="\\n".join(x for x in a.get("content",[]) if isinstance(x,str))\n'
                  '            out.append({"session":p.rsplit("/",1)[-1],"entry":next((k for k,v in marks.items() if v in said),None),\n'
                  '                        "input":said[:200],"characters":len(text),"tokens":len(text)//4,\n'
                  '                        "full_continuity":"</executive-continuity>" in text,\n'
                  '                        "full_company":"Current company material from the host" in text,"context":text})\n'
                  'print(json.dumps(out))\n')
        rows = json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), shlex.quote(self.home), shlex.quote(json.dumps(marks))), 120) or '[]')
        (self.out / 'context.json').write_text(json.dumps(rows, indent=2) + '\n')
        summary = [{k: r[k] for k in ('session', 'entry', 'input', 'characters', 'tokens', 'full_continuity', 'full_company')}
                   for r in rows]
        for name in ('first', 'second'):
            told = [r for r in rows if r['entry'] == name]
            if not told:
                raise StepFailed('no input carrying the %s entry has host context on record; see context.json' % name)
            r = told[0]
            if r['tokens'] >= 1000 or r['full_continuity'] or r['full_company']:
                raise StepFailed('the %s entry added %d tokens (chars/4), full continuity %s, full company material %s; '
                                 'see context.json' % (name, r['tokens'], r['full_continuity'], r['full_company']))
        return {'inputs': summary}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help="seconds for an entry to reach Rich and his turn on it")
    p.add_argument('--answer-within', type=float, default=300, help='seconds for the line to reach to-codex.md')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--steps', default=','.join(DEFAULT_STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = CodexWatchWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            name = 'channel_step' if step == 'channel' else step.replace('-', '_')
            row['evidence'] = getattr(walk, name)()
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
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
