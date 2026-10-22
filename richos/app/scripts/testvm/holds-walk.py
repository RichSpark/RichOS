#!/usr/bin/env python3
"""holds-walk.py — in the regular app, Rich is told of held work in his own turn and tells the user.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      holds-walk.py --out DIR --expect-sha SHA

THE QUESTION (richos-hq docs/plans/2026-10-22-no-silent-holds.md; the CEO: "WHEN WILL [THIS] STOP
ONCE AND FOR ALL???"). In the terminal, every turn end names each hold with nothing checkable to
end it. The app shows no hook notice to the user, so the app's Rich gets the same lines as context
in his own turn (engine scripts/app-engine-hook.py, UserPromptSubmit) and his instruction file
(engine app/agents/rich.md) has him tell the user in plain words. This walk shows it on fixture data
in a guest: an unresolved wait on file and a correction a teammate's run never acted on.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" with a folder that is a Git repository
  hold       in THIS conversation's own registry partition (<app data>/engine-state/workspaces/
             <sha256 of [entity, thread]>, engine_profile.rs workspace_state), 40 minutes old
             (holds-guest.py): spelling-mate, finished with the wait "code done, real-app checks
             left", and fix-mate, finished with a correction sent after its last commit. The
             engine's own hold lines for that partition must name both.
  ask        typed to Rich: an ordinary check-in that names neither. PASS needs, for the first
             turn after the send: Rich's own native transcript carries the hold lines as hook
             context (HELD BY RICH, NOT BY YOU, both names), and his reply, as the app recorded
             it (conversation-ledger.jsonl AssistantDelta of that turn), names both held pieces
             of work. Evidence: ask.json.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import argparse
import hashlib
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
stu_reminder_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(stu_reminder_walk)
command_walk = stu_reminder_walk.command_walk
StepFailed = stu_reminder_walk.StepFailed

STEPS = ['identity', 'first-run', 'hold', 'ask']
MARK = 'HELD BY RICH, NOT BY YOU'
NAMES = ('spelling-mate', 'fix-mate')
# Plain words, his reply's own (rich.md): each held piece of work is named by its teammate or by what it is.
SAID = {'spelling-mate': ('spelling-mate', 'spelling', 'real-app'),
        'fix-mate': ('fix-mate', 'correction', 'sign-in', 'finding')}
ASK = 'Good morning. Quick check-in before I head out: how are things looking today?'


def ms(at):
    """A ledger `at` in guest milliseconds, whether it was written in seconds or milliseconds."""
    at = float(at or 0)
    return at if at > 1e11 else at * 1000


class HoldsWalk(stu_reminder_walk.StuReminderWalk):
    def ledger(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        out = []
        for line in rows.splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                continue
        return out

    def partition(self):
        thread = self.facts.get('thread')
        entity = None
        for row in self.ledger():
            if row.get('thread_id') == thread and row.get('entity_id'):
                entity = row['entity_id']
        if not entity:
            raise StepFailed('the conversation %s has no company bound in the ledger' % thread)
        digest = hashlib.sha256(json.dumps([entity, thread], separators=(',', ':')).encode()).hexdigest()
        return self.data + '/engine-state/workspaces/' + digest

    def hold(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        made = self.payload + '/holds-walk'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'holds-guest.py', made + '/'], 300)
        partition = self.partition()
        said = guest(self.vm, '/usr/bin/python3 %s file %s %s %s 40' % (
            shlex.quote(made + '/holds-guest.py'), shlex.quote(self.payload + '/engine'),
            shlex.quote(partition), shlex.quote(self.company)), 60).strip().splitlines()
        filed = json.loads(said[-1]) if said else {}
        (self.out / 'hold.json').write_text(json.dumps(dict(filed, partition=partition), indent=2) + '\n')
        lines = filed.get('lines') or []
        missing = [n for n in NAMES if not any(MARK in l and n in l for l in lines)]
        if missing:
            raise StepFailed("the engine's own hold lines for the partition do not name %s: %s" % (missing, lines))
        self.facts['hold_lines'] = lines
        self.save()
        return {'partition': partition, 'lines': lines}

    def transcript_rows(self, session):
        script = ('import glob,json,sys\n'
                  'out=[]\n'
                  'for root in (sys.argv[1]+"/.claude", sys.argv[4]):\n'
                  '  for p in glob.glob(root+"/**/"+sys.argv[2]+".jsonl",recursive=True):\n'
                  '    for l in open(p):\n'
                  '      if sys.argv[3] in l: out.append(l.strip()[:4000])\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s %s %s' % (
            shlex.quote(script), shlex.quote(self.home), shlex.quote(session), shlex.quote(MARK),
            shlex.quote(self.data)), 120) or '[]')

    def ask(self):
        if not self.facts.get('hold_lines'):
            raise StepFailed('hold must have run (no hold lines on record)')
        sent = self.send(ASK)
        end = time.monotonic() + self.a.reply_within
        turn, session, reply, done = None, None, '', False
        while time.monotonic() < end:
            rows = self.ledger()
            # ledger.rs Event, #[serde(tag = "event")]: TurnStarted, AssistantDelta, TurnCompleted.
            started = [r for r in rows if r.get('event') == 'TurnStarted' and ms(r.get('at')) >= sent[0]]
            if started:
                turn, session = started[0]['turn_id'], started[0]['session_id']
                reply = ''.join(r.get('text', '') for r in rows
                                if r.get('event') == 'AssistantDelta' and r.get('turn_id') == turn)
                done = any(r.get('event') == 'TurnCompleted' and r.get('turn_id') == turn for r in rows)
                if done:
                    break
            time.sleep(5)
        context = self.transcript_rows(session) if session else []
        evidence = {'sent_guest_ms': sent, 'turn': turn, 'session': session, 'turn_completed': done,
                    'reply': reply, 'transcript_rows_with_the_lines': context,
                    'filed_lines': self.facts['hold_lines']}
        (self.out / 'ask.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failure = ask_verdict(turn, done, session, context, reply, self.a.reply_within)
        if failure:
            raise StepFailed(failure + '; see ask.json')
        return {'turn': turn, 'session': session, 'reply': reply}


def ask_verdict(turn, done, session, context, reply, within):
    """'' when the check-in passes, else why not: a finished turn, both holds in Rich's own
    transcript as context, and both named in his reply (by teammate or by what they are)."""
    if not turn or not done:
        return 'Rich did not finish a turn on the check-in within %g s' % within
    if not any(all(n in row for n in NAMES) for row in context):
        return "Rich's own transcript for session %s carries no context naming both held pieces of work" % session
    low = (reply or '').lower()
    unsaid = [n for n in NAMES if not any(word in low for word in SAID[n])]
    if unsaid:
        return 'Rich was told in his turn but his reply does not name %s' % unsaid
    return ''


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help="seconds for the walk's other waits")
    p.add_argument('--reply-within', type=float, default=420, help="seconds for Rich's reply to the check-in")
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--reminder-seconds', type=float, default=120)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = HoldsWalk(a)
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
        walk.shot('end.png')
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
        pass
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
