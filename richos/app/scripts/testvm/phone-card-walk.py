#!/usr/bin/env python3
"""phone-card-walk.py — the Mac's paired-phone card on the real app, as round 49 draws it (CEO,
2026-10-31: "round-49/index.html: OK"), for a phone that switches notifications on and then off.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --tailnet --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      phone-card-walk.py --out DIR --expect-sha SHA [--steps a,b,c]

THE QUESTION. Round 49 tells "Ready" from "Notifications not on" with one fact the Mac's phone status
now reports: whether the paired phone signed up for notifications through the RichConnect app. On
main the card read only the retired web page's subscription, so it could not know. Asked on the
real app in the guest, never on the host's screen (CEO ruling §65); no physical phone is touched.
The phone is walkphone.py's (the RichConnect client on the host, over the native apps' API).

WHAT IT DOES, reusing adopt-walk.py's steps for everything up to the pairing:
  identity    the running app says it was built from --expect-sha
  first-run   memory setup declined, the company "Acme" with a folder, the first conversation
  pair        "Use Rich from your phone" -> Use Tailscale -> Set my phone up; the walk phone pairs
              and says They match, and the Mac presses its own They match
  ready       the phone switches "Notify me when Rich replies" on; the card, reopened, says
              Connected and round 49's Ready line, and is photographed
  off         the phone switches it off; the card, reopened, says round 49's Notifications not on
              line, and is photographed
Every card is also checked for the retired words (Safari, Chrome, Home Screen, web page, the
RichOS iPhone app). Exit 0 when every step passes. The app is quit by run-walk.py's stop.sh (§54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import walkphone  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed

STEPS = ['identity', 'first-run', 'pair', 'ready', 'off']
PAIRED = 'It is paired. Open RichConnect on it and keep talking.'
EXPECT = {
    'ready': ['Connected', PAIRED, 'It can reach you with a notification when Rich has something for you.'],
    'off': ['Connected', PAIRED, 'It cannot send you notifications yet.', 'Notify me when Rich replies'],
}
RETIRED = re.compile(r'Safari|Chrome|Home Screen|web page|RichOS iPhone app', re.I)


class Walk(adopt.Walk):
    def card(self, state):
        """Reopens the phone sheet (a settled card does not poll) and returns its words, after
        waiting for round 49's words for STATE and photographing the screen."""
        try:
            self.press('Close', contains=False)
        except StepFailed:
            pass  # the sheet was already closed
        self.press('Settings', role='AXPopUpButton')
        self.press('Use Rich from your phone')  # the settings menu's row is a button
        end = time.monotonic() + 30
        text = ''
        while time.monotonic() < end:
            rows = [r for r in self.ax('tree', '--in', 'dialog', '--max', '200') if not r.get('meta')]
            text = '\n'.join(str(r.get(k, '')) for r in rows for k in ('title', 'desc', 'value'))
            if all(want in text for want in EXPECT[state]):
                break
            time.sleep(1)
        (self.out / ('card-%s.txt' % state)).write_text(text + '\n')
        self.shot('card-%s.png' % state)
        missing = [w for w in EXPECT[state] if w not in text]
        if missing:
            raise StepFailed('the %s card is missing %s; it said:\n%s' % (state, json.dumps(missing), text[-1200:]))
        retired = RETIRED.findall(text)
        if retired:
            raise StepFailed('the %s card still says %s' % (state, json.dumps(retired)))
        return {'words': [w for w in EXPECT[state]], 'photo': str(self.out / ('card-%s.png' % state))}

    def switch(self, on):
        try:
            return walkphone.notifications(self.phone_state, on=on)
        except walkphone.PhoneFailed as exc:
            raise StepFailed('the phone could not switch notifications %s: %s' % ('on' if on else 'off', exc)) from exc

    def ready(self):
        phone = self.switch(True)
        return {'phone': phone, **self.card('ready')}

    def off(self):
        phone = self.switch(False)
        return {'phone': phone, **self.card('off')}


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
        print(f"{step}: {row['outcome']}" + (f" ({row.get('detail')})" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
