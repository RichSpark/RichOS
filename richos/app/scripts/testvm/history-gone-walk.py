#!/usr/bin/env python3
"""history-gone-walk.py — a chat whose saved Claude Code record is gone, with messages queued, on the real app.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      history-gone-walk.py --out DIR --expect-sha SHA

THE QUESTION (the CEO, 2026-11-01: "Look at all the fuckshit that was going in the app after I
started it ... Is that all supposed to happen that way??"). A chat that cannot start fails once with
the true reason and leaves nothing behind; a chat whose history is gone says so and answers his next
message (native.rs `HistoryGone`, `start_with_engine`; spine.rs `drive_shared_turns`).

STEPS, in the guest, never on the host's screen (CEO ruling 65):
  identity    the running app says it was built from --expect-sha
  first-run   adopt-walk.py's: company "Acme" and its first conversation
  chat        one short message to Rich, waited for until its turn is closed: the chat now has a
              Claude Code session record. The app's `onboarding-scopes` folder is listed (BEFORE).
  lose        the app is quit (relaunch.py, the captured pid only) and every Claude Code record in the
              guest's `~/.claude/projects` that carries the chat message is deleted; the scope folder
              is listed again (AFTER-QUIT)
  queue       relaunch; the home screen is left with Return; QUEUED messages are typed with real keys
              and sent with the Return key, one after the other. PASS needs exactly ONE turn closed
              by the history-gone sentence (conversation-ledger.jsonl TurnInterrupted), its words on
              the screen (screenshot read by OCR), and no further history-gone sentence in --settle
              seconds. The scope folder is listed (AFTER-QUEUE).
  answer      one more message; PASS needs its turn TurnCompleted with a reply, on a Claude Code
              record that did not exist before the queue (a NEW session), and the scope folder
              (AFTER-ANSWER) holding no more `*-continuity.json` files than AFTER-QUIT plus BEFORE

Evidence in <out>: report.json, scopes-<phase>.txt, queue.json, answer.json, screenshots.
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
from relaunch import guest, relaunch, quit_app  # noqa: E402

_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
StepFailed = base.StepFailed
command = base.command_walk.command

STEPS = ['identity', 'first-run', 'chat', 'lose', 'queue', 'answer']
CHAT = 'Hello Rich. Reply with one word: ready. (history-walk-chat)'
QUEUED = ('Queued one: reply with one word: one. (history-walk-q1)',
          'Queued two: reply with one word: two. (history-walk-q2)',
          'Queued three: reply with one word: three. (history-walk-q3)')
LAST = 'Is anyone there? Reply with one word: here. (history-walk-last)'
GONE_WORDS = "earlier history is gone from this Mac"  # native.rs HISTORY_GONE, in the ledger's reason
GONE_ON_SCREEN = 'earlier words in this chat are gone'  # interruption.rs, the card's sentence
RETURN_KEY = '36'


def keystrokes(text):
    literal = text.replace('\\', '\\\\').replace('"', '\\"')
    return f'tell application "System Events" to keystroke "{literal}"'


def continuity_files(listing):
    return [line for line in listing.splitlines() if line.endswith('-continuity.json')]


def queue_verdict(record):
    """None when the lost history was told once and left nothing behind, else the sentence."""
    told = record.get('gone_turns', [])
    if len(told) != 1:
        return 'the history-gone sentence closed %d turn(s), not exactly one: %s' % (len(told), json.dumps(told))
    if not record.get('on_screen'):
        return 'the history-gone sentence was not on the screen (OCR of queue-gone.png)'
    if record.get('gone_after_settle', 0) != 1:
        return 'after %g s the sentence had closed %d turns' % (record.get('settle', 0), record.get('gone_after_settle', 0))
    return None


class HistoryGoneWalk(base.StuReminderWalk):
    def ledger(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        rows = []
        for line in text.splitlines():
            try:
                rows.append(json.loads(line))
            except ValueError:
                pass
        return rows

    def scopes(self, phase):
        """The files in every `onboarding-scopes` folder under the guest's home, one path a line."""
        listing = guest(self.vm, 'find %s -type d -name onboarding-scopes -exec find {} -type f \\; 2>/dev/null | sort || true'
                        % shlex.quote(self.home), 60)
        (self.out / ('scopes-%s.txt' % phase)).write_text(listing + '\n')
        self.facts.setdefault('scopes', {})[phase] = listing.splitlines()
        self.save()
        return listing

    def records(self):
        """Claude Code's own session records in the guest: path to size."""
        text = guest(self.vm, 'find %s/.claude/projects -name "*.jsonl" 2>/dev/null | sort || true' % shlex.quote(self.home), 60)
        return text.splitlines()

    def turn_closed(self, turn_id):
        for row in self.ledger():
            if row.get('turn_id') == turn_id and row.get('event') in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed'):
                return row
        return None

    def prompt_row(self, words, since=0):
        rows = [r for r in self.ledger() if r.get('event') == 'PromptReceived' and r.get('text', '').strip() == words
                and (r.get('at') or 0) >= since]
        return rows[-1] if rows else None

    def wait_prompt(self, words, since=0, seconds=180):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            row = self.prompt_row(words, since)
            if row:
                return row
            time.sleep(2)
        raise StepFailed('no PromptReceived row carries %s within %g s' % (json.dumps(words), seconds))

    def wait_closed(self, turn_id, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            row = self.turn_closed(turn_id)
            if row:
                return row
            time.sleep(3)
        raise StepFailed('turn %s was not closed within %g s' % (turn_id, seconds))

    def chat(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        sent = self.send(CHAT)
        row = self.wait_prompt(CHAT, sent[0])
        closed = self.wait_closed(row['turn_id'], self.a.within)
        if closed.get('event') != 'TurnCompleted':
            raise StepFailed('the first chat turn ended as %s, not TurnCompleted: %s' % (closed.get('event'), json.dumps(closed)[:300]))
        self.facts['chat_turn'] = row['turn_id']
        self.facts['records_before'] = self.records()
        self.scopes('before')
        self.save()
        self.shot('chat.png')
        return {'turn': row['turn_id'], 'records': len(self.facts['records_before']),
                'continuity_files_before': len(continuity_files(self.scopes('before')))}

    def lose(self):
        quit_app(self.vm)
        self.scopes('after-quit')
        # The records carrying the chat message, found by the words only the chat has.
        found = guest(self.vm, 'grep -l %s %s/.claude/projects/*/*.jsonl 2>/dev/null || true'
                      % (shlex.quote('history-walk-chat'), shlex.quote(self.home)), 60).splitlines()
        if not found:
            raise StepFailed('no Claude Code record in the guest carries the chat message: nothing to lose')
        for path in found:
            guest(self.vm, 'rm -f ' + shlex.quote(path), 30)
        left = guest(self.vm, 'grep -l history-walk-chat %s/.claude/projects/*/*.jsonl 2>/dev/null || true' % shlex.quote(self.home), 60)
        if left.strip():
            raise StepFailed('the record is still there after removal: ' + left)
        self.facts['lost_records'] = found
        self.facts['records_after_loss'] = self.records()
        self.save()
        return {'removed': found}

    def queue(self):
        record = {'queued': list(QUEUED), 'settle': self.a.settle}
        launched = relaunch(self.vm)
        record['relaunched'] = launched
        record['home'] = self.after_relaunch()
        start = self.clock()
        record['started_at_ms'] = start
        for words in QUEUED:
            command([HERE / 'guest.sh', self.vm, 'osascript', '-e', keystrokes(words)], 120)
            command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY])
        record['sent_at_ms'] = self.clock()
        self.shot('queue-sent.png')
        end = time.monotonic() + self.a.within
        gone = []
        while time.monotonic() < end:
            gone = [r for r in self.ledger() if r.get('event') == 'TurnInterrupted' and GONE_WORDS in r.get('reason', '')
                    and (r.get('at') or 0) >= start]
            if gone:
                break
            time.sleep(3)
        record['gone_turns'] = [{'turn_id': r['turn_id'], 'at': r.get('at'), 'reason': r['reason']} for r in gone]
        self.shot('queue-gone.png')
        found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), GONE_ON_SCREEN, str(self.out / 'queue-gone.png'),
                                '--first', '--quiet'], capture_output=True, text=True, timeout=60)
        record['on_screen'] = found.returncode == 0
        time.sleep(self.a.settle)
        again = [r for r in self.ledger() if r.get('event') == 'TurnInterrupted' and GONE_WORDS in r.get('reason', '')
                 and (r.get('at') or 0) >= start]
        record['gone_after_settle'] = len(again)
        record['events_since'] = [{k: r.get(k) for k in ('event', 'turn_id', 'text', 'reason', 'at')}
                                  for r in self.ledger() if (r.get('at') or 0) >= start
                                  and r.get('event') in ('PromptReceived', 'TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed')]
        record['records_after_queue'] = self.records()
        record['scopes_after_queue'] = self.scopes('after-queue').splitlines()
        self.shot('queue-settled.png')
        (self.out / 'queue.json').write_text(json.dumps(record, indent=2) + '\n')
        failure = queue_verdict(record)
        if failure:
            raise StepFailed(failure + '; see queue.json')
        return {k: record[k] for k in ('gone_turns', 'on_screen', 'gone_after_settle')}

    def answer(self):
        record = {'words': LAST}
        before = set(self.facts.get('records_after_loss', []))
        sent = self.send(LAST)
        row = self.wait_prompt(LAST, sent[0])
        closed = self.wait_closed(row['turn_id'], self.a.within)
        record['turn'] = {k: closed.get(k) for k in ('event', 'turn_id', 'reason', 'at')}
        replies = [r for r in self.ledger() if r.get('turn_id') == row['turn_id'] and r.get('event') == 'AssistantDelta']
        record['reply_chars'] = sum(len(r.get('text', '')) for r in replies)
        self.shot('answer.png')
        after = self.scopes('after-answer')
        records = self.records()
        record['new_records'] = [p for p in records if p not in before]
        record['continuity_files'] = {phase: len(continuity_files('\n'.join(self.facts['scopes'][phase])))
                                      for phase in ('before', 'after-quit', 'after-queue', 'after-answer')}
        (self.out / 'answer.json').write_text(json.dumps(record, indent=2) + '\n')
        failures = []
        if closed.get('event') != 'TurnCompleted' or not record['reply_chars']:
            failures.append('the next message was not answered: %s, %d reply characters' % (closed.get('event'), record['reply_chars']))
        if not record['new_records']:
            failures.append('no new Claude Code record: the answer did not come from a new session')
        files = record['continuity_files']
        if files['after-answer'] > files['after-quit'] + files['before']:
            failures.append('scope files were left behind: %s' % json.dumps(files))
        if failures:
            raise StepFailed('; '.join(failures) + '; see answer.json')
        return record


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help="seconds for each turn and for the walk's other waits")
    p.add_argument('--settle', type=float, default=60, help='seconds the queue is watched after the sentence appeared')
    p.add_argument('--job-within', type=float, default=1200)
    p.add_argument('--idle-within', type=float, default=900)
    p.add_argument('--approvals', type=int, default=3)
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = HistoryGoneWalk(a)
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
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
