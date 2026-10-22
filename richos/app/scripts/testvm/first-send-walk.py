#!/usr/bin/env python3
"""first-send-walk.py — the first message he types after a relaunch reaches Rich, sent the way a person sends it.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      first-send-walk.py --out DIR --expect-sha SHA

THE QUESTION (esc-20261020T171021Z-8fd0fca7). After a relaunch, a walk typed its first message into
the composer and pressed Send, the composer emptied, no message appeared and nothing reached Rich;
a second send worked. That walk typed with an accessibility paste and pressed Send with AXPress, so
whether a PERSON hits it was open. This walk answers it with a person's input only: real key events
(System Events keystrokes) and the real mouse (pointer-drag.sh), never a paste or an AXPress.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" and its first conversation, so a relaunch restores one
  return     relaunch; the home screen (recognized by OCR: its live canvas keeps accessibility reads
             past their deadline) is left with Return, its own key, which puts the hand in the
             composer; the message typed at once with real keys, sent with the Return key
  click      the same after a second relaunch, sent with a real mouse click on Send
  two        after a relaunch, two messages typed and each sent with the Return key before the launch
             has opened his conversation; both must reach Rich, the first before the second (handover
             review rv-20261020T191206Z-fa2ee4a4-778c: the second used to stay in the box)

Each send is judged by the app's own durable record: a PromptReceived row in conversation-ledger.jsonl
carrying the exact words, within --within seconds. Each records whether the launch had already
opened his conversation when the words were typed (`opened_in`: the Output buttons on the frame
taken right after typing; without them, the words were typed in the window the escalation's walk
typed in), the composer's value before and after the send, the guest-clock times and two screenshots. Evidence: <out>/<step>.json, <out>/<step>-typed.png and -sent.png.

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
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed
command = adopt_walk.command

STEPS = ['identity', 'first-run', 'return', 'click', 'two']
WORDS = {'return': 'First send after a relaunch, sent with the Return key. Reply with one word: received.',
         'click': 'First send after a relaunch, sent with a click on Send. Reply with one word: received.'}
TWO = ('Please send the proposal today. Reply with one word: noted.',
       'Correction: do not send it until I approve. Reply with one word: received.')
RETURN_KEY = '36'


def keystrokes(text):
    """The AppleScript that types TEXT with real key events (steps-walk.py's `keys`)."""
    literal = text.replace('\\', '\\\\').replace('"', '\\"')
    return f'tell application "System Events" to keystroke "{literal}"'


def center(node):
    return '%d,%d' % (node['x'] + node['w'] // 2, node['y'] + node['h'] // 2)


def verdict(record):
    """None when the send reached Rich as he typed it, else the sentence saying what happened."""
    if record.get('typed') != record.get('words'):
        return 'the keystrokes did not reach the composer: it read %s after typing' % json.dumps(record.get('typed'))
    if record.get('received'):
        return None
    after = record.get('composer_after')
    if after == '':
        return ('SWALLOWED: the composer emptied and no PromptReceived row carries the words within %g s'
                % record.get('within', 0))
    return 'not sent: the composer still reads %s and no PromptReceived row carries the words' % json.dumps(after)


def two_verdict(record):
    """None when both messages reached Rich in the order he sent them, else what happened."""
    if record.get('conversation_open_when_sent') is not False:
        return ('did not test the window: the conversation was %s when both had been sent'
                % ('already open' if record.get('conversation_open_when_sent') else 'not read as closed'))
    order = record.get('received_order') or []
    missing = [w for w in TWO if w not in order]
    if missing:
        return ('not received: no PromptReceived row carries %s within %g s; the composer reads %s'
                % (' or '.join(json.dumps(w) for w in missing), record.get('within', 0),
                   json.dumps(record.get('composer_after'))))
    if [w for w in order if w in TWO] != list(TWO):
        return 'OUT OF ORDER: Rich received %s' % json.dumps(order)
    if record.get('composer_after'):
        return 'a sent message is still in the box: %s' % json.dumps(record.get('composer_after'))
    return None


class FirstSendWalk(adopt_walk.Walk):
    def ledger_rows(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        out = []
        for line in rows.splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
        return out

    def composer(self):
        found = self.ax('find', '--title', 'Message to Rich', '--role', 'AXTextArea', '--contains', '--first')
        nodes = [n for n in found if not n.get('meta')]
        if not nodes:
            raise StepFailed('the composer is not on screen')
        return nodes[0]

    def opened_in(self, frame):
        """Whether the launch had already opened his conversation in FRAME: the Output buttons
        are drawn only then (output-panel.js `paintButtons`), so "Output" is on the frame or not.
        Read off the picture by OCR, because accessibility reads of this window miss their
        deadline on a busy host and a probe that cannot answer is not evidence. None: unread."""
        found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Output', str(self.out / frame), '--quiet'],
                               capture_output=True, text=True, timeout=60)
        return {0: True, 1: False}.get(found.returncode)

    def shown(self, words):
        """Whether his message is drawn in the conversation (a text node carrying its first words)."""
        try:
            return bool([n for n in self.ax('find', '--value', words[:40], '--contains', '--first') if not n.get('meta')
                         and n.get('role') != 'AXTextArea'])
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return False
            raise

    def through_home(self, seconds=120):
        began = time.monotonic()
        while time.monotonic() - began < seconds:
            self.shot('home.png')
            found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Talk to Rich', str(self.out / 'home.png'),
                                    '--first', '--quiet'], capture_output=True, text=True, timeout=60)
            if found.returncode not in (0, 1):
                raise StepFailed('ocr-find could not read the frame: ' + found.stderr)
            if found.returncode == 0:
                command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY])
                return round(time.monotonic() - began, 1)
            time.sleep(1)
        raise StepFailed('the home screen was not on screen within %g s of the relaunch' % seconds)

    def first_send(self, how):
        words = WORDS[how]
        record = {'how': how, 'words': words, 'within': self.a.within}
        launched = relaunch(self.vm)
        record['relaunched'] = launched
        record['home_left_after_s'] = self.through_home()
        # Straight on, as a person does: leaving the home screen puts the hand in the composer
        # (home.js focuses #input as it hides), and every read before the keys is time in which
        # the launch can finish opening his conversation, the window under test.
        record['typed_at_ms'] = round(self.clock())
        command([HERE / 'guest.sh', self.vm, 'osascript', '-e', keystrokes(words)], 120)
        self.shot(how + '-typed.png')
        record['typed'] = self.composer().get('value')
        record['sent_at_ms'] = round(self.clock())
        if how == 'return':
            command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY])
        else:
            send = [n for n in self.ax('find', '--title', 'Send', '--role', 'AXButton', '--first') if not n.get('meta')]
            if not send:
                raise StepFailed('no Send button on screen')
            record['send_button'] = {k: send[0][k] for k in ('x', 'y', 'w', 'h')}
            command([HERE / 'pointer-drag.sh', self.vm, center(send[0]), center(send[0])], 60)
        record['shown_after_send'] = self.shown(words)
        record['conversation_open_when_typed'] = self.opened_in(how + '-typed.png')
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            rows = [r for r in self.ledger_rows() if r.get('event') == 'PromptReceived' and r.get('text', '').strip() == words]
            if rows:
                at = rows[-1].get('at')
                record['received'] = {'turn_id': rows[-1].get('turn_id'), 'thread_id': rows[-1].get('thread_id'), 'at': at,
                                      'after_send_s': round((at - record['sent_at_ms']) / 1000, 1)
                                      if isinstance(at, (int, float)) else None}
                break
            time.sleep(2)
        record['composer_after'] = self.composer().get('value')
        self.shot(how + '-sent.png')
        record['app_log_tail'] = guest(self.vm, 'tail -c 4000 ' + shlex.quote(launched['log']) + ' 2>/dev/null || true', 60)
        (self.out / (how + '.json')).write_text(json.dumps(record, indent=2) + '\n')
        failure = verdict(record)
        if failure:
            raise StepFailed(failure + '; see %s.json' % how)
        return {k: record[k] for k in ('conversation_open_when_typed', 'shown_after_send', 'received',
                                       'composer_after', 'home_left_after_s')}

    def return_(self):
        return self.first_send('return')

    def two(self):
        """Two messages, each sent with the Return key, before the launch opens his conversation."""
        record = {'how': 'two', 'words': list(TWO), 'within': self.a.within}
        launched = relaunch(self.vm)
        record['relaunched'] = launched
        record['home_left_after_s'] = self.through_home()
        record['typed_at_ms'] = round(self.clock())
        for words in TWO:
            command([HERE / 'guest.sh', self.vm, 'osascript', '-e', keystrokes(words)], 120)
            command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY])
        record['sent_at_ms'] = round(self.clock())
        self.shot('two-sent-at-once.png')
        record['conversation_open_when_sent'] = self.opened_in('two-sent-at-once.png')
        record['shown_after_send'] = [self.shown(w) for w in TWO]
        end = time.monotonic() + self.a.within
        rows = []
        while time.monotonic() < end:
            rows = [r for r in self.ledger_rows() if r.get('event') == 'PromptReceived'
                    and r.get('text', '').strip() in TWO and (r.get('at') or 0) >= record['typed_at_ms']]
            if len(rows) >= len(TWO):
                break
            time.sleep(2)
        # The ledger is appended in the order Rich received them, so its line order is the order.
        record['received_order'] = [r.get('text', '').strip() for r in rows]
        record['received'] = [{'turn_id': r.get('turn_id'), 'thread_id': r.get('thread_id'), 'at': r.get('at'),
                               'intake_id': r.get('intake_id')} for r in rows]
        record['composer_after'] = self.composer().get('value')
        self.shot('two-sent.png')
        record['app_log_tail'] = guest(self.vm, 'tail -c 4000 ' + shlex.quote(launched['log']) + ' 2>/dev/null || true', 60)
        (self.out / 'two.json').write_text(json.dumps(record, indent=2) + '\n')
        failure = two_verdict(record)
        if failure:
            raise StepFailed(failure + '; see two.json')
        return {k: record[k] for k in ('conversation_open_when_sent', 'shown_after_send', 'received_order',
                                       'received', 'composer_after', 'home_left_after_s')}

    def click(self):
        return self.first_send('click')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=120,
                   help='seconds for the PromptReceived row after a send: a Send pressed before the launch '
                        'opened his conversation goes out when it does, over 30 s later in the guest')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = FirstSendWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        name = {'return': 'return_'}.get(step, step.replace('-', '_'))
        try:
            row['evidence'] = getattr(walk, name)()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            # A swallowed first send is what this walk measures, so the other way of sending is
            # still tried after it; anything earlier stops the walk.
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if row['outcome'] != 'PASS':
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            if step not in ('return', 'click', 'two'):
                break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
