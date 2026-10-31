#!/usr/bin/env python3
"""codex-watch-walk.py — in the regular app, Codex's entries reach Stu by themselves, once each.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      codex-watch-walk.py --out DIR --expect-sha SHA

THE QUESTION (gap X of richos-hq docs/plans/2026-10-21-switching-to-the-app.md, item 5d). Codex
writes to Rich in `to-rich.md` of a channel folder and Rich answers in the `to-codex.md` beside it.
The app runs the engine's codex-watch as its own child (richos-core `codex_watch.rs`) on the user's
channel folders (the CODEX_CHANNELS setting; unset, the conventional
`~/.richos-coordination/rich-codex` and `rich-codex-questions`) and saves each new entry for Stu in
the conversation on screen. Stu acts on them; until 2026-10-31 they went to Rich, who passed most of
them on to Stu at the cost of a turn of his own (richos-hq docs/research/2026-10-31-app-token-burn,
1d). This walk shows that in the real app, on fixture data in a guest.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65). The channel is the
conventional folder in the GUEST's home (the setting is unset, as on a new install):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" with a folder that is a Git repository
  unopened   an entry written while NO conversation is open survives a quit (follow-ups of
             2026-10-28, rv-20261028T095119Z-32768ffd-e1eb#1): the app relaunched with
             RICHOS_ENTITY naming no registered company, so it opens on no conversation; an
             entry is written and the watcher marks it told; the app is quit and opened again
             as usual (Acme, its conversation open). PASS needs exactly one row from codex-watch
             carrying it in THIS conversation's inbox for Stu
  channel    the channel's to-rich.md is written with one entry while the app runs: PASS needs
             exactly one row from codex-watch carrying it in THIS conversation's inbox for Stu
             (<app state>/messages/<company>/<conversation>/stu.jsonl)
  relaunch   the app quit and opened again (command-walk.py's): its sessions now have the
             channel folder, and the watcher starts again where it stopped
  append     a second entry is appended: PASS needs exactly one inbox row carrying it, the first
             entry still told once only, Stu's native transcript carrying it (his turn on it) and
             no turn of Rich's carrying it in the conversation ledger
  answer     typed to Rich: add one line to the to-codex.md beside that to-rich.md. PASS needs the
             line in the guest's to-codex.md
  context    what each input added to its session's context (the entries' are Stu's), from the
             guest's native transcripts: a
             session's first input carries the full blocks; every later one, the second entry's
             included, under 1,000 tokens (chars/4) with neither the full continuity block nor
             the company material. For one session, run it without relaunch:
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

STEPS = ['identity', 'first-run', 'unopened', 'channel', 'relaunch', 'append', 'answer', 'context']
# `context` measures one session, which `relaunch` ends, so it runs only when asked for.
DEFAULT_STEPS = [s for s in STEPS if s != 'context']
CHANNEL = '.richos-coordination/rich-codex'


def judge_context(rows):
    """The `context` step's verdict over its rows (one per input, in transcript order): None when
    it passes, else why not. Each entry's input must be on record. The first input of a session is
    given the full blocks by design (the model holds no earlier copy); every LATER input of that
    session, and so the second entry's always, must add under 1,000 tokens (chars/4) and carry
    neither the full continuity block nor the company material."""
    for name in ('first', 'second'):
        if not any(r['entry'] == name for r in rows):
            return 'no input carrying the %s entry has host context on record' % name
    seen = set()
    for r in rows:
        later = r['session'] in seen
        seen.add(r['session'])
        if r['entry'] == 'second' and not later:
            return 'the second entry was the first input of its session, so it proves nothing about a repeat'
        if later and (r['tokens'] >= 1000 or r['full_continuity'] or r['full_company']):
            return 'a later input (%s entry) added %d tokens (chars/4), full continuity %s, full company material %s' % (
                r['entry'] or 'no', r['tokens'], r['full_continuity'], r['full_company'])
    return None


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
        """THIS conversation's inbox rows for Stu from codex-watch."""
        script = ('import glob,json,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/messages/*/"+sys.argv[2]+"/stu.jsonl"):\n'
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

    def told_to_end(self):
        """Has the watcher told the channel's to-rich.md up to its end (its own offsets record)?"""
        script = ('import json,os,sys\n'
                  'f=sys.argv[2]+"/to-rich.md"\n'
                  'try: o=json.load(open(sys.argv[1]+"/engine-state/codex-watch/watch/app-host/offsets.json"))\n'
                  'except Exception: o={}\n'
                  'print(json.dumps({"offset":(o.get(f) or {}).get("offset"),"size":os.path.getsize(f) if os.path.exists(f) else None}))\n')
        seen = json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), shlex.quote(self.data), shlex.quote(self.channel)), 60) or '{}')
        return seen.get('size') is not None and seen.get('offset') == seen.get('size')

    def unopened(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        from relaunch import relaunch
        closed = relaunch(self.vm, environment={'RICHOS_ENTITY': 'no-such-company'})
        end = time.monotonic() + self.a.relaunch_within
        log = ''
        while time.monotonic() < end and '[richos] Codex watch: watching' not in log:
            log = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(closed['log']), 60)
            time.sleep(2)
        if 'RICHOS_ENTITY="no-such-company" is not a registered entity' not in log:
            raise StepFailed('the relaunch did not open on no company; see its log %s' % closed['log'])
        if '[richos] Codex watch: watching' not in log:
            raise StepFailed('the watcher did not start within %g s of the relaunch' % self.a.relaunch_within)
        mark, text = self.entry('unopened')
        self.write(text, append=True)
        end = time.monotonic() + self.a.within
        while time.monotonic() < end and not self.told_to_end():
            time.sleep(2)
        if not self.told_to_end():
            raise StepFailed('the watcher never told the entry within %g s' % self.a.within)
        early = [r for r in self.inbox() if mark in r['text']]
        opened = relaunch(self.vm)
        settled = self.after_relaunch()
        rows = self.told(mark, self.a.within)
        evidence = {'mark': mark, 'closed_launch_log': closed['log'], 'rows_before_quit': early,
                    'reopened': opened, 'settled': settled, 'rows': rows}
        (self.out / 'unopened.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if early:
            raise StepFailed('the entry was saved while no conversation was open, so the quit proves nothing; see unopened.json')
        if len(rows) != 1:
            raise StepFailed('the entry written with no conversation open reached Stu\'s inbox %d time(s) after the relaunch, '
                             'not once; see unopened.json' % len(rows))
        return {'mark': mark, 'row': rows[0]}

    def channel_step(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        mark, text = self.entry('first')
        self.write(text, append=False)
        rows = self.told(mark, self.a.within)
        (self.out / 'channel.json').write_text(json.dumps({'mark': mark, 'rows': rows}, indent=2) + '\n')
        if len(rows) != 1:
            raise StepFailed('the first entry reached Stu\'s inbox %d time(s), not once; see channel.json' % len(rows))
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
        heard = []
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            heard = self.stu_heard(mark)
            if heard:
                break
            time.sleep(5)
        rich = [{'turn_id': r.get('turn_id'), 'source': r.get('source'), 'prompt': str(r.get('text', ''))[:2000]}
                for r in self.ledger() if r.get('event') == 'PromptReceived' and r.get('thread_id') == self.facts['thread']
                and mark in str(r.get('text', '')) and r.get('at', 0) >= sent - 5000]
        evidence = {'mark': mark, 'rows': rows, 'first_entry_rows_after_relaunch': again,
                    'stu_transcripts_carrying_it': heard, 'rich_turns_carrying_it': rich}
        (self.out / 'append.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if len(rows) != 1:
            raise StepFailed('the appended entry reached Stu\'s inbox %d time(s), not once; see append.json' % len(rows))
        if len(again) != 1:
            raise StepFailed('the first entry was told %d time(s) after the relaunch, not once; see append.json' % len(again))
        if not heard:
            raise StepFailed('no transcript of Stu\'s carried the appended entry within %g s; see append.json' % self.a.within)
        if rich:
            raise StepFailed('%d turn(s) of Rich\'s carried the appended entry; see append.json' % len(rich))
        return {'mark': mark, 'stu_transcripts': heard}

    def stu_heard(self, mark):
        """The native transcripts of THIS conversation's Stu sessions that carry `mark`: each
        session bound as Stu (`<state>/evidence/<session>/binding.json`), its transcript the path its
        callbacks name (richos-core `session_tools::native_path`)."""
        script = ('import glob,json,os,sys\n'
                  'out=[]\n'
                  'for b in glob.glob(sys.argv[1]+"/engine-state/evidence/*/binding.json"):\n'
                  '    try: s=json.load(open(b))\n'
                  '    except Exception: continue\n'
                  '    if s.get("role")!="stu" or s.get("thread_id")!=sys.argv[2]: continue\n'
                  '    paths=set()\n'
                  '    try:\n'
                  '        for l in open(os.path.join(os.path.dirname(b),"callbacks.jsonl")):\n'
                  '            try: p=(json.loads(l).get("callback") or {}).get("transcript_path")\n'
                  '            except Exception: continue\n'
                  '            if p: paths.add(p)\n'
                  '    except OSError: pass\n'
                  '    for p in sorted(paths):\n'
                  '        try:\n'
                  '            if sys.argv[3] in open(p,errors="replace").read(): out.append({"session":s.get("session_id"),"transcript":p})\n'
                  '        except OSError: pass\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s %s' % (
            shlex.quote(script), shlex.quote(self.data), shlex.quote(self.facts['thread']), shlex.quote(mark)), 120) or '[]')

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
        """What each input of the conversation added to its session's context (the entries' are
        Stu's): every UserPromptSubmit `hook_additional_context` in the guest's native transcripts, beside the input it came
        with ([`judge_context`]: each later input of a session under 1,000 tokens, chars/4, with
        neither the full continuity block nor the company material; follow-up "Rich's context
        fills with the same injected blocks on every message", 2026-10-30). Run it without
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
        problem = judge_context(rows)
        if problem:
            raise StepFailed(problem + '; see context.json')
        return {'inputs': summary}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help="seconds for an entry to reach Stu and his turn on it")
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
