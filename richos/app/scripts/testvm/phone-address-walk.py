#!/usr/bin/env python3
"""phone-address-walk.py — what a browser gets at the phone address now that the phone web page is
retired (CEO, 2026-10-30: "yes, the phone web page needs to be retired now").

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --tailnet --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      phone-address-walk.py --out DIR --expect-sha SHA [--steps a,b,c]

THE QUESTION. The Mac used to serve a web app to a phone's browser on 8443. That page is retired:
the native RichConnect apps replaced it, and a browser that opens the address (a phone camera that
scans the pairing QR code opens `<origin>/#pair=<code>`) must get a plain answer, never a broken
page. Asked on the real app in the guest, with the guest's own Safari and curl as the browser,
never on the host's screen (CEO ruling §65). It replaces phone-file-walk.py, which drove the page.

WHAT IT DOES, reusing adopt-walk.py's steps for everything before the address:
  identity    the running app says it was built from --expect-sha
  first-run   memory setup declined, the company "Acme" with a folder, the first conversation
  address     "Use Rich from your phone" -> Use Tailscale -> Set my phone up, and the pairing
              address the Mac draws is read. In the guest, over the tailnet name: `/` and
              `/index.html` must be 200 text/html saying "Use the RichConnect app" with no script;
              `/sw.js` must be the worker that unregisters the old page; every file the web app was
              made of must be a 404. Then the pairing address is opened in the guest's Safari and
              the screen is photographed and read (OCR): PASS when it says "RichConnect".

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed = adopt.StepFailed
command = adopt.command

STEPS = ['identity', 'first-run', 'address']
PLAIN = 'Use the RichConnect app'
# Every file the retired web app was made of, and two it never had: each must be a 404.
GONE = ['/app.js', '/styles.css', '/manifest.webmanifest', '/icons/icon-192.png',
        '/icons/apple-touch-icon.png', '/lib/api.js', '/lib/oxford-comma.js', '/README.md']


class Walk(adopt.Walk):
    def pairing_address(self):
        self.press('Settings', role='AXPopUpButton')
        self.press('Use Rich from your phone', role='AXMenuItem')
        # The route chooser shows while RichOS Connect is offered; this walk is the Tailscale path.
        if self.present('Use Tailscale', role='AXCheckBox'):
            self.ax('click', '--title', 'Use Tailscale', '--role', 'AXCheckBox', '--in', 'dialog', '--first')
        self.wait_for('Set my phone up')
        self.press('Set my phone up')
        end = time.monotonic() + 30
        while time.monotonic() < end:
            try:
                text = command([HERE / 'ax.sh', self.vm, 'tree', '--max', '400'], 60)
            except StepFailed as exc:
                if not axwait.ask_again(exc, 'tree'):  # a read with no answer: not drawn yet
                    raise
                text = ''
            m = re.search(r'https://[^\s\'"<>]+#pair=[A-Z0-9]+', text)
            if m:
                return m.group(0)
            time.sleep(1)
        raise StepFailed('no pairing address was drawn within 30 s\n' + self.picture_screen('Set my phone up'))

    def fetch(self, url):
        """status, content type and body of one GET from inside the guest, as a browser there."""
        body_path = self.payload + '/phone-address-body'
        meta = guest(self.vm, 'curl -sk --max-time 20 -o {b} -w "%{{http_code}} %{{content_type}}" {u}'.format(
            b=shlex.quote(body_path), u=shlex.quote(url)), 60)
        body = guest(self.vm, 'cat ' + shlex.quote(body_path) + ' 2>/dev/null || true', 60)
        status, _, ctype = meta.partition(' ')
        return status, ctype, body

    def address(self):
        url = self.pairing_address()
        origin = url.split('#', 1)[0].rstrip('/')
        seen = {}
        problems = []
        for path in ['/', '/index.html']:
            status, ctype, body = self.fetch(origin + path)
            seen[path] = [status, ctype]
            if status != '200' or not ctype.startswith('text/html') or PLAIN not in body or '<script' in body:
                problems.append(f'{path}: {status} {ctype} {body[:200]!r}')
        status, ctype, body = self.fetch(origin + '/sw.js')
        seen['/sw.js'] = [status, ctype]
        if status != '200' or 'javascript' not in ctype or 'registration.unregister()' not in body:
            problems.append(f'/sw.js: {status} {ctype} {body[:200]!r}')
        for path in GONE:
            status, ctype, _ = self.fetch(origin + path)
            seen[path] = [status, ctype]
            if status != '404':
                problems.append(f'{path} was served: {status} {ctype}')
        (self.out / 'address.json').write_text(json.dumps({'origin': origin, 'seen': seen}, indent=2) + '\n')
        if problems:
            raise StepFailed('the phone address does not give the plain answer:\n' + '\n'.join(problems))
        # And as a person meets it: the pairing address opened in a browser.
        guest(self.vm, shlex.join(['open', '-a', 'Safari', url]))
        shot = self.out / 'safari-at-the-phone-address.png'
        said = ''
        for _ in range(10):
            time.sleep(2)
            r = subprocess.run([str(HERE / 'shot.sh'), self.vm, str(shot), '--ocr'],
                               capture_output=True, text=True, timeout=120)
            said = r.stdout
            if r.returncode == 0 and 'RichConnect' in said:
                break
        (self.out / 'safari-at-the-phone-address.ocr.txt').write_text(said)
        if 'RichConnect' not in said:
            raise StepFailed(f'Safari at {origin} did not show the plain answer; screen read: {said[-400:]!r}')
        return {'origin': origin, 'seen': seen, 'photo': str(shot)}


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
