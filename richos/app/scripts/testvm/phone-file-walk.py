#!/usr/bin/env python3
"""phone-file-walk.py — a file Rich sends, on the PHONE PAGE the real app serves (§138 item 12).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --tailnet --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      phone-file-walk.py --out DIR --expect-sha SHA [--steps a,b,c]

THE QUESTION. The Mac shows a file Rich attached as a card under his reply and opens it in the
Mac's usual app (output-walk.py's rich-file step). The Mac also sends that file to phones as a row
of kind "file" (phone/rows.rs) with the bytes on a signed GET /api/messages?kind=file
(phone/routes.rs). Does the phone page (richos/web/web-app, served by this app on 8443) show it as
a file and open it? Asked on the real app in the guest, with the guest's Safari as the phone, never
on the host's screen (CEO ruling §65).

WHAT IT DOES, reusing adopt-walk.py's steps for everything before the file:
  identity    the running app says it was built from --expect-sha
  first-run   memory setup declined, the company "Acme" with a folder, the first conversation
  pair        "Use Rich from your phone" -> Use Tailscale -> Set my phone up -> the pairing page in
              the guest's Safari -> They match, on both sides. Safari is then set to a phone's
              width (PHONE_FRAME) beside the app
  rich-file   a one-page PDF is made in the Acme folder in the guest (cupsfilter, which every Mac
              has), and Rich is asked FROM THE PHONE to put it in front of the user with his
              attach_file tool. Waits for the record (engine-state/rich-files/files.jsonl) and for
              the turn's end, then photographs the phone page (light) and keeps Safari's tree.
              PASS when the phone page has a button named "Open rich-file-check.pdf" whose name
              carries the card's line ("PDF · … · 1 page").
  open        the card is pressed on the phone page. PASS when Safari has a tab holding the bytes the
              Mac sent (a blob: address, Safari's own PDF view); photographed, then that tab is closed.
  dark        the guest's appearance is set to dark (the page follows it), the card photographed,
              the appearance put back.
  missing     the PDF is deleted in the guest and the card pressed again. PASS when the card says
              the Mac's own sentence, "No longer on this Mac. It was moved or deleted.", and offers
              "Ask Rich again"; photographed in light and dark.

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

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed

STEPS = ['identity', 'first-run', 'pair', 'rich-file', 'open', 'dark', 'missing']
FILE_NAME = 'rich-file-check.pdf'
GONE = 'No longer on this Mac. It was moved or deleted.'
# The phone page at a phone's width (430 px, the widest iPhone the page is measured at), beside
# the app's window so both are in one frame.
PHONE_FRAME = (1040, 30, 430, 900)
TASK = ('Please put the PDF at %s in front of me in this conversation with your attach_file tool. '
        'Do nothing else, and reply with one short sentence.')


class Walk(adopt.Walk):
    def ledger(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        return [json.loads(r) for r in rows.splitlines() if r.startswith('{')]

    def records(self):
        path = self.data + '/engine-state/rich-files/files.jsonl'
        rows = guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true', 60)
        return [json.loads(r) for r in rows.splitlines() if r.startswith('{')]

    def appearance(self, dark):
        guest(self.vm, shlex.join(['osascript', '-e', 'tell application "System Events" to tell appearance preferences '
                                   'to set dark mode to %s' % ('true' if dark else 'false')]))
        time.sleep(2)

    def phone_texts(self):
        nodes = [n for n in self.ax('tree', app='Safari') if not n.get('meta')]
        (self.out / ('safari-%d.tree.jsonl' % int(time.time()))).write_text(''.join(json.dumps(n) + '\n' for n in nodes))
        return nodes

    def card(self):
        """The phone page's file card: a button whose name begins "Open <file>"."""
        return [n for n in self.phone_texts() if n.get('role') == 'AXButton'
                and any(str(n.get(k) or '').startswith('Open ' + FILE_NAME) for k in ('title', 'desc'))]

    def front_safari(self):
        guest(self.vm, shlex.join(['osascript', '-e', 'tell application "Safari" to activate']))
        time.sleep(1)

    # --- steps --------------------------------------------------------------------------------
    def pair(self):
        evidence = super().pair()
        x, y, w, h = PHONE_FRAME
        script = ('tell application "System Events" to tell process "Safari" to set {position, size} of window 1 '
                  'to {{%d, %d}, {%d, %d}}' % (x, y, w, h))
        guest(self.vm, shlex.join(['osascript', '-e', script]))
        evidence['phone_frame'] = PHONE_FRAME
        return evidence

    def rich_file(self):
        path = self.company + '/' + FILE_NAME
        guest(self.vm, "printf 'Rich file check\\n\\nA PDF Rich puts in front of you.\\n' > /tmp/rich-file-check.txt "
                       '&& /usr/sbin/cupsfilter /tmp/rich-file-check.txt > %s 2>/dev/null' % shlex.quote(path))
        known = {r.get('turn_id') for r in self.ledger() if r.get('event') == 'PromptReceived'}
        self.type_into(TASK % path, '--role', 'AXTextArea', '--title', 'Write to Rich', app='Safari')
        self.press('Send', app='Safari')
        end = time.monotonic() + 420
        turn, rows = None, []
        while time.monotonic() < end:
            ledger = self.ledger()
            if turn is None:
                new = [r for r in ledger if r.get('event') == 'PromptReceived' and r.get('turn_id') not in known
                       and r.get('source') != 'internal']
                turn = new[-1]['turn_id'] if new else None
            rows = [r for r in self.records() if r.get('name') == FILE_NAME]
            done = turn and [r for r in ledger if r.get('turn_id') == turn
                             and r.get('event') in ('TurnCompleted', 'TurnInterrupted')]
            if rows and done:
                break
            time.sleep(3)
        self.facts.update(turn=turn, record=rows)
        self.save()
        if not rows:
            raise StepFailed('no attachment of %s reached the record (turn %s)' % (FILE_NAME, turn))
        time.sleep(4)  # the row's live arrival and the reply's last repaint on the phone page
        self.front_safari()
        self.shot('phone-file-light.png')
        cards = self.card()
        texts = [n.get('title') or n.get('value') or n.get('desc') for n in self.phone_texts()
                 if FILE_NAME in str(n.get('title') or n.get('value') or n.get('desc') or '')]
        evidence = {'turn': turn, 'record': rows, 'card': cards, 'texts_naming_the_file': texts}
        if not cards:
            raise StepFailed('the phone page has no "Open %s" button; what names the file there: %s'
                             % (FILE_NAME, json.dumps(texts)))
        name = cards[0].get('title') or cards[0].get('desc') or ''
        if '1 page' not in name or 'PDF' not in name:
            raise StepFailed("the card's name does not carry its line (PDF, size, 1 page): " + name)
        return evidence

    def tabs(self):
        out = guest(self.vm, shlex.join(['osascript', '-e',
                    'tell application "Safari" to get URL of every tab of every window']))
        return [s.strip() for s in out.replace('{', '').replace('}', '').split(',') if s.strip()]

    def open(self):
        self.front_safari()
        before = self.tabs()
        self.press('Open ' + FILE_NAME, app='Safari')
        end = time.monotonic() + 30
        urls = before
        while time.monotonic() < end:
            urls = self.tabs()
            if any(u.startswith('blob:') for u in urls):
                break
            time.sleep(1)
        time.sleep(1.5)
        self.shot('phone-file-opened.png')
        evidence = {'tabs_before': before, 'tabs_after': urls}
        if not any(u.startswith('blob:') for u in urls):
            raise StepFailed('pressing the card opened no tab holding the file: ' + json.dumps(evidence))
        # Back to the conversation: close the file's tab (it is front), keep the phone page.
        guest(self.vm, shlex.join(['osascript', '-e', 'tell application "Safari" to close current tab of front window']))
        time.sleep(1)
        return evidence

    def dark(self):
        self.appearance(True)
        try:
            self.front_safari()
            self.shot('phone-file-dark.png')
            return {'cards': self.card()}
        finally:
            self.appearance(False)

    def missing(self):
        guest(self.vm, 'rm -f ' + shlex.quote(self.company + '/' + FILE_NAME))
        self.front_safari()
        self.press('Open ' + FILE_NAME, app='Safari')
        end = time.monotonic() + 30
        texts = []
        while time.monotonic() < end:
            texts = [n.get('title') or n.get('value') or n.get('desc') or '' for n in self.phone_texts()]
            if any(GONE in t for t in texts):
                break
            time.sleep(1)
        self.shot('phone-file-missing-light.png')
        self.appearance(True)
        try:
            self.front_safari()
            self.shot('phone-file-missing-dark.png')
        finally:
            self.appearance(False)
        ask = self.present('Ask Rich again', app='Safari')
        evidence = {'gone_said': any(GONE in t for t in texts), 'ask_again': ask}
        if not evidence['gone_said'] or not ask:
            raise StepFailed('the card did not say the file is gone with Ask Rich again: ' + json.dumps(evidence))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
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
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
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
