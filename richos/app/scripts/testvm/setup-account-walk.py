#!/usr/bin/env python3
"""setup-account-walk.py — the setup sheet's account sentence names the Ultra plan, and its link
opens the pricing page (CEO 2026-10-31: "in the app where we're linking to that page, the wording
needs to adjust ... Instead of Max/Max20 it's now 'Ultra'").

Run by run-walk.py with --new-user, which boots the guest, launches the app as a first run and
removes the clone:

  run-walk.py --new-user --bundle ZIP --home EMPTY_DIR --engine none --report REPORT --wait 600 -- \\
      setup-account-walk.py --out DIR [--expect-sha SHA]

STEPS, in the guest, never on the host's screen (CEO ruling 65):
  identity   the running app says it was built from --expect-sha (skipped without one)
  sentence   the setup sheet is up; its text says "an Ultra subscription" and "pick the Ultra tier"
             and never "Max"; one frame in the guest's light appearance, one in its dark one
  link       the link "sign up there first and pick the Ultra tier" pressed once; Safari's current
             page is claude.com/pricing

Evidence: <out>/report.json, a frame (png) and its OCR text per moment. Nothing is installed: the
sheet's "Set it up" is never pressed. The guest appearance is left light.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time
from urllib.parse import urlsplit

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('new_user_walk', HERE / 'new-user-walk.py')
new_user_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(new_user_walk)
StepFailed = new_user_walk.StepFailed

STEPS = ['identity', 'sentence', 'link']
LINK_TEXT = 'sign up there first and pick the Ultra tier'
PRICING_HOST, PRICING_PATH = 'claude.com', '/pricing'


def sentence_verdict(text):
    """None when the sheet's OCR TEXT carries the new plan wording and no old one, else why not.
    Whitespace is folded: OCR keeps the sheet's line breaks."""
    flat = re.sub(r'\s+', ' ', text or '')
    if not re.search(r'Anthropic account and an Ultra subscription there', flat):
        return 'the sheet does not say "an Ultra subscription there"'
    if not re.search(r'pick the Ultra tier', flat):
        return 'the sheet does not say "pick the Ultra tier"'
    if re.search(r'\bMax\b', flat):
        return 'the sheet still names Max'
    return None


def pricing_page(url):
    """True when URL is claude.com/pricing (an optional www., any query)."""
    parsed = urlsplit((url or '').strip())
    return (parsed.hostname or '').removeprefix('www.') == PRICING_HOST and parsed.path.rstrip('/') == PRICING_PATH


class AccountWalk(new_user_walk.NewUserWalk):
    def appearance(self, dark):
        guest(self.vm, "osascript -e 'tell application \"System Events\" to tell appearance preferences "
                       "to set dark mode to %s'" % ('true' if dark else 'false'))
        time.sleep(2)

    def sheet_text(self, name):
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            text = self.frame(name)
            if 'Anthropic account' in re.sub(r'\s+', ' ', text):
                return text
            time.sleep(3)
        raise StepFailed('the setup sheet never showed its account sentence within %g s' % self.a.within)

    def sentence(self):
        self.appearance(False)
        light = self.sheet_text('setup-sheet-light')
        why = sentence_verdict(light)
        if why:
            raise StepFailed('light: ' + why)
        self.appearance(True)
        dark = sentence_verdict(self.frame('setup-sheet-dark'))
        self.appearance(False)
        if dark:
            raise StepFailed('dark: ' + dark)
        return {'light': 'setup-sheet-light.png', 'dark': 'setup-sheet-dark.png'}

    def link(self):
        self.press(LINK_TEXT, 'AXLink', contains=False)
        end = time.monotonic() + 30
        url = ''
        while time.monotonic() < end:
            url = guest(self.vm, "osascript -e 'tell application \"Safari\" to if (count windows) > 0 then "
                                 "get URL of current tab of front window'").strip()
            if pricing_page(url):
                break
            time.sleep(2)
        self.frame('pricing-page')
        if not pricing_page(url):
            raise StepFailed('the link did not open claude.com/pricing; Safari shows %r' % urlsplit(url).netloc)
        return {'host': urlsplit(url).hostname, 'path': urlsplit(url).path, 'frame': 'pricing-page.png'}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', default='')
    p.add_argument('--within', type=float, default=240, help='seconds for the setup sheet to come up')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    walk = AccountWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in a.steps.split(','):
        row = new_user_walk.run_step(walk, step)
        ok = ok and row['outcome'] == 'PASS'
        report['steps'].append(row)
        a.out.mkdir(parents=True, exist_ok=True)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    walk.pull()
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
