#!/usr/bin/env python3
"""browser-page-walk.py — Rich reads a page as the user's own signed-in browser shows it.

The CEO, 2026-11-10: "Basically, we need a way for Rich to overcome the LinkedIn barrier i.e. the
problem of not being able to browse Linkedin without being signed-in." and "the user is already
expected to be signed-in in all of their social media profiles (including Linkedin) in their default
browser."

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes it:

  run-walk.py --no-app --home EMPTY_DIR --engine "" --report REPORT -- \\
      browser-page-walk.py --out DIR

It MEASURES; it does not exercise any RichOS tool. The CEO widened the job on 2026-11-10 (a way for
Rich to use the user's default browser, options compared before anything is built), and how much
text each browser exposes through Accessibility is one input to that comparison.

run-walk.py passes the owned VM name as the first argument. Nothing here touches the host's screen
(CEO ruling §65): every browser runs in the guest.

  site            browser-page-guest.py serving the local test site in the guest; a plain fetch of
                  /profile gets the signed-out sentence
  chrome          Google Chrome installed in the guest when missing, started once with its first-run
                  screens off (so a later `open` reaches a running Chrome)
  measure-safari  THE EXPOSURE MEASUREMENT, Safari: the 400-sentence article opened, how many of the
                  sentences Safari's AXWebArea exposes as AXStaticText, and whether the
                  display:none sentence is (it must not be)
  measure-chrome  the same in Chrome: as it comes, after AXManualAccessibility, after
                  AXEnhancedUserInterface
  measure-public  one public page (Wikipedia) in both browsers: how many of its paragraphs, fetched
                  separately with curl, appear in what Accessibility exposes
  signed-in       in each browser: /sign-in sets the site's cookie, then /profile is opened; what
                  Accessibility exposes must be the signed-in sentence, while curl in the same guest
                  still gets the signed-out one (the browser's own session is what is read)
  cleanup         the server stopped; the guest and every browser in it end with run-walk.py's stop.sh

Exit 0 when every step passes. The report is DIR/report.json.
"""
import argparse
import html.parser
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

PORT = 18765
CHROME_DMG = 'https://dl.google.com/chrome/mac/universal/stable/GGRO/googlechrome.dmg'
CHROME_APP = '/Applications/Google Chrome.app'
CHROME_FLAGS = ['--no-first-run', '--no-default-browser-check', '--use-mock-keychain', '--disable-sync']
PUBLIC = 'https://en.wikipedia.org/wiki/Accessibility'
SIGNED_IN = 'Ada Walker, signed-in view: works on accessible browsers at Example Works.'
SIGNED_OUT = 'Sign in to see this profile.'
STEPS = ['site', 'chrome', 'measure-safari', 'measure-chrome', 'measure-public', 'signed-in', 'cleanup']


class StepFailed(Exception):
    pass


def command(argv, timeout=60):
    r = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=timeout)
    if r.returncode:
        raise StepFailed('command failed (%d): %s\n%s%s' % (r.returncode, ' '.join(map(str, argv)), r.stdout, r.stderr))
    return r.stdout


class Paragraphs(html.parser.HTMLParser):
    """The text of every <p> of a page, as a fetch without a browser sees it."""

    def __init__(self):
        super().__init__()
        self.depth, self.skip, self.rows, self.cur = 0, 0, [], []

    def handle_starttag(self, tag, attrs):
        if tag == 'p':
            self.depth += 1
        if tag in ('sup', 'style', 'script'):
            self.skip += 1

    def handle_endtag(self, tag):
        if tag in ('sup', 'style', 'script') and self.skip:
            self.skip -= 1
        if tag == 'p' and self.depth:
            self.depth -= 1
            text = ' '.join(''.join(self.cur).split())
            if len(text) > 60:
                self.rows.append(text)
            self.cur = []

    def handle_data(self, data):
        if self.depth and not self.skip:
            self.cur.append(data)


def probe_phrases(paragraphs):
    """One 40-character run from the middle of each paragraph: inside one text run, so a browser
    splitting a paragraph into several AXStaticText nodes at its links does not hide it."""
    out = []
    for p in paragraphs:
        words = p.split()
        mid = len(words) // 2
        phrase = ' '.join(words[mid:mid + 6])
        if len(phrase) >= 20 and '[' not in phrase:
            out.append(phrase)
    return out


class Walk:
    def __init__(self, a):
        import os
        self.a, self.vm, self.out = a, a.vm, a.out
        self.out.mkdir(parents=True, exist_ok=True)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.state = state
        self.payload = (state / 'payload').read_text().strip()
        self.home = self.payload + '/home'
        self.site_dir = self.payload + '/site'
        self.helper = self.payload + '/browser-page-guest.py'
        self.base = f'http://127.0.0.1:{PORT}'
        self.facts = {}

    def g(self, cmd, timeout=60):
        return guest(self.vm, cmd, timeout)

    def measure(self, bundle, url_contains, expect=None, manual=False, timeout=25, enhanced=False):
        argv = ['python3', self.helper, 'measure', '--bundle', bundle, '--url-contains', url_contains,
                '--timeout', str(timeout)]
        if expect:
            argv += ['--expect', expect]
        if manual:
            argv.append('--manual')
        if enhanced:
            argv.append('--enhanced')
        return json.loads(self.g(shlex.join(argv), timeout + 60).splitlines()[-1])

    def open_in(self, app, url):
        # -g: in the background, the way the tool opens a link, so nothing is raised over the app.
        self.g(f'open -g -a {shlex.quote(app)} {shlex.quote(url)}')

    # --- steps --------------------------------------------------------------------------------
    def site(self):
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'browser-page-guest.py', self.helper], 60)
        self.g(f'nohup python3 {shlex.quote(self.helper)} serve --port {PORT} --dir {shlex.quote(self.site_dir)} '
               f'>{shlex.quote(self.site_dir)}.log 2>&1 &')
        for _ in range(30):
            body = self.g(f'curl -fsS {self.base}/profile || true')
            if 'Profile' in body:
                break
            time.sleep(0.5)
        else:
            raise StepFailed('the test site did not answer in the guest')
        if SIGNED_OUT not in body or SIGNED_IN in body:
            raise StepFailed(f'a plain fetch of /profile is not the signed-out page: {body!r}')
        return {'url': self.base, 'plain_fetch_profile': body}

    def chrome(self):
        installed = False
        if self.g(f'test -d {shlex.quote(CHROME_APP)} && echo yes || true') != 'yes':
            dmg = self.payload + '/googlechrome.dmg'
            self.g(f'curl -fsSL -o {shlex.quote(dmg)} {CHROME_DMG}', 600)
            mount = self.g(f'hdiutil attach -nobrowse -readonly {shlex.quote(dmg)} | tail -1', 120).split('\t')[-1].strip()
            self.g(f'cp -R {shlex.quote(mount)}/"Google Chrome.app" /Applications/ && hdiutil detach {shlex.quote(mount)}', 300)
            installed = True
        # By path: a Chrome copied in seconds ago is not registered with LaunchServices by name yet.
        self.g(f'open -g -a {shlex.quote(CHROME_APP)} --args ' + ' '.join(CHROME_FLAGS))
        for _ in range(40):
            if self.g('lsappinfo info -only pid -app com.google.Chrome || true').split('=')[-1].strip().isdigit():
                break
            time.sleep(0.5)
        else:
            raise StepFailed('Chrome did not start in the guest')
        time.sleep(3)
        return {'installed_chrome': installed}

    def measure_article(self, app, bundle, manual):
        self.open_in(app, f'{self.base}/article')
        time.sleep(4)
        row = self.measure(bundle, '/article', self.site_dir + '/expected.txt', manual)
        if not row.get('trusted'):
            raise StepFailed(f'the measuring process has no Accessibility grant in the guest: {row}')
        return row

    def measure_safari(self):
        row = self.measure_article('Safari', 'com.apple.Safari', False)
        self.facts['safari'] = row
        return row

    def measure_chrome(self):
        plain = self.measure_article(CHROME_APP, 'com.google.Chrome', False)
        manual = self.measure('com.google.Chrome', '/article', self.site_dir + '/expected.txt', True)
        enhanced = self.measure('com.google.Chrome', '/article', self.site_dir + '/expected.txt', False, enhanced=True)
        return {'as_it_comes': plain, 'after_manual_accessibility': manual,
                'after_enhanced_user_interface': enhanced}

    def measure_public(self):
        html_text = self.g(f'curl -fsSL --max-time 60 {shlex.quote(PUBLIC)}', 120)
        parser = Paragraphs()
        parser.feed(html_text)
        phrases = probe_phrases(parser.rows)
        if len(phrases) < 10:
            raise StepFailed(f'the public page gave too few paragraphs to measure by ({len(phrases)})')
        expect = self.payload + '/public-expected.txt'
        local = self.out / 'public-expected.txt'
        local.write_text('\n'.join(phrases) + '\n')
        command([HERE / 'guest.sh', self.vm, '--push', local, expect], 60)
        rows = {}
        for app, bundle in (('Safari', 'com.apple.Safari'), (CHROME_APP, 'com.google.Chrome')):
            self.open_in(app, PUBLIC)
            time.sleep(6)
            chrome = bundle == 'com.google.Chrome'
            rows[bundle] = self.measure(bundle, '/wiki/Accessibility', expect, chrome, 40, enhanced=chrome)
        return {'url': PUBLIC, 'paragraph_phrases': len(phrases), 'browsers': rows}

    def signed_in(self):
        rows = {}
        for app, bundle in (('Safari', 'com.apple.Safari'), (CHROME_APP, 'com.google.Chrome')):
            self.open_in(app, f'{self.base}/sign-in')
            time.sleep(3)
            self.open_in(app, f'{self.base}/profile')
            time.sleep(3)
            chrome = bundle == 'com.google.Chrome'
            row = self.measure(bundle, '/profile', None, chrome, 20, enhanced=chrome)
            if not row.get('signed_in_text') or row.get('signed_out_text'):
                raise StepFailed(f'{bundle}: Accessibility did not expose the signed-in profile: {row}')
            rows[bundle] = row
        plain = self.g(f'curl -fsS {self.base}/profile')
        if SIGNED_OUT not in plain:
            raise StepFailed(f'a plain fetch is no longer the signed-out page: {plain!r}')
        return {'browsers': rows, 'plain_fetch_profile': plain}

    def cleanup(self):
        pid = self.g(f'cat {shlex.quote(self.site_dir)}/server.pid 2>/dev/null || true')
        if pid.isdigit():
            self.g(f'kill {pid} 2>/dev/null || true')
        return {'server_pid_stopped': pid}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', default='')
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
        except (StepFailed, RuntimeError, ValueError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok and step != 'cleanup':
            if 'cleanup' in steps and steps.index('cleanup') > steps.index(step):
                walk.cleanup()
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
