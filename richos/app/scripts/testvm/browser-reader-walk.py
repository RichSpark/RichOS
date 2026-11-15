#!/usr/bin/env python3
"""browser-reader-walk.py — the team's browser reader on REAL websites, in several browsers, in a guest.

The CEO, 2026-11-13: "Getting useful information from ANY website in the user's default browser
where the user is already logged in to all their relevant social media accounts etc. And NOTE: I
don't know what the user's default browser is or what extensions the user might or might not have
installed."

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes it:

  run-walk.py --no-app --home EMPTY_DIR --engine "" --report REPORT -- \\
      browser-reader-walk.py --out DIR --exe RICHOS_TAURI [--expect-sha SHA] [--steps ...] [--pages REGEX]

The tool is the app's own executable, `--browser-mcp`, driven over stdio exactly as claude drives
it, one call at a time, by browser-reader-guest.py in the guest: open a page, read the answer,
press an item chosen by what it is called, read again. Nothing runs on the host's screen (CEO
ruling §65); the browsers are the guest's. The guest has no account anywhere and nobody is asked
for a sign-in: the signed-in case is the local site whose /profile shows a different sentence to a
browser holding its cookie than to a plain fetch (browser-page-guest.py).

The pages are real public websites, one per layout that matters (PAGES below): a long article, two
accordions (one with a code sample, whose lines and indents must be kept), a tab list, a table with
a "More" link, a page that loads more as it scrolls, a web app, a page that sends a signed-out
visitor to its sign-in page, and a very long page in German at an address with a letter outside
ASCII in it (Chrome offers to translate it). Each has a question whose answer is on
the page, some only after a press or a scroll. Three more are the local site's: the page only a
signed-in browser is shown, a code sample whose spacing is part of the program (it reads
character for character as written), and a link that opens in a new tab (the reader follows it
there, and closes both tabs at the end). They are other people's sites and can change: a FAIL
names the page and what was missing, and the tool's whole answer is kept beside the report.

  site           browser-page-guest.py's local site in the guest (the signed-in case)
  chrome         Google Chrome installed in the guest (browser-page-walk.py's step)
  firefox        Firefox installed and started
  brave          Brave installed and started (a second Chromium browser)
  edge           Microsoft Edge installed and started (a third)
  tool           the executable pushed (with its whole app bundle when it is in one, as a release is);
                 it lists exactly the five tools
  read-safari    every page of PAGES with Safari the default browser
  read-chrome    the same with Chrome
  read-firefox   the same with Firefox
  read-brave     the same with Brave
  read-edge      the same with Edge
  states-safari  one real page read with Safari in each state a user's desktop leaves a browser
                 in: every window minimized, the app hidden, every window closed, the app quit
  states-chrome, states-firefox, states-brave, states-edge
                 the same with each
  cleanup        the server stopped; the guest and every browser in it end with run-walk.py's stop.sh

Each read-* step is judged page by page and FAILS when any page fails; the others still run, so one
report shows every browser. A states-* step FAILS when the page was not read in a state, on the
first open or on the second the tool's own refusal asks for; whether the browser could be put in
the state at all (`reached`) and what it was left with (its windows, whether it runs) are in the
report.
--pages REGEX walks only the pages whose name matches, to walk again only what changed. Exit 0 when every step passes. DIR/report.json is the report;
DIR/<browser>-rows.json every answer of the tool, whole.
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

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

PORT = 18765
TOOLS = ['open_page', 'press', 'scroll_down', 'next_part', 'close_page']
SIGNED_IN = 'Ada Walker, signed-in view: works on accessible browsers at Example Works.'
SIGNED_OUT = 'Sign in to see this profile.'
# The local site's code sample (browser-page-guest.py's CODE): spacing that is part of the program.
CODE = ' if ready:\n  say("a  b ")\n done()\nmessage = """first \n\n\nlast"""'
BROWSERS = {
    'safari': {'bundle': 'com.apple.Safari', 'app': '/Applications/Safari.app'},
    # `first_run`: the file a Chromium browser writes once its first-run screens are done. The
    # guest's browsers were started with --no-first-run, which skips the screens and the file,
    # so a browser the reader starts again (states-*, not running) would show its welcome
    # window, which a user's browser never does.
    'chrome': {'bundle': 'com.google.Chrome', 'app': '/Applications/Google Chrome.app',
               'first_run': 'Library/Application Support/Google/Chrome/First Run'},
    # A new Firefox profile opens a "Welcome to Firefox" dialog window over its browser window
    # (measured 2026-11-13: the only window Accessibility then lists is that dialog's
    # chrome://browser/content/spotlight.html, so no page can be read). A user's Firefox has long
    # dismissed it; the guest's is told not to show it, through Firefox's own policies.
    'firefox': {'bundle': 'org.mozilla.firefox', 'app': '/Applications/Firefox.app',
                'dmg': 'https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=en-US', 'flags': [],
                'before': ['defaults write org.mozilla.firefox EnterprisePoliciesEnabled -bool TRUE',
                           'defaults write org.mozilla.firefox OverrideFirstRunPage -string ""',
                           'defaults write org.mozilla.firefox DontCheckDefaultBrowser -bool TRUE',
                           'defaults write org.mozilla.firefox SkipTermsOfUse -bool TRUE',
                           'defaults write org.mozilla.firefox UserMessaging -dict SkipOnboarding -bool TRUE '
                           'WhatsNew -bool FALSE FeatureRecommendations -bool FALSE ExtensionRecommendations -bool FALSE']},
    'brave': {'bundle': 'com.brave.Browser', 'app': '/Applications/Brave Browser.app',
              'dmg': 'https://laptop-updates.brave.com/latest/osx',
              'first_run': 'Library/Application Support/BraveSoftware/Brave-Browser/First Run',
              'flags': ['--no-first-run', '--no-default-browser-check', '--use-mock-keychain', '--disable-sync']},
    # Microsoft ships Edge for the Mac as an installer package; its app is taken out of it.
    'edge': {'bundle': 'com.microsoft.edgemac', 'app': '/Applications/Microsoft Edge.app',
             'pkg': 'https://go.microsoft.com/fwlink/?linkid=2093504',
             'first_run': 'Library/Application Support/Microsoft Edge/First Run',
             'flags': ['--no-first-run', '--no-default-browser-check', '--use-mock-keychain', '--disable-sync']},
}
STEPS = ['site', 'chrome', 'firefox', 'brave', 'edge', 'tool',
         'read-safari', 'states-safari', 'read-chrome', 'states-chrome', 'read-firefox', 'states-firefox',
         'read-brave', 'states-brave', 'read-edge', 'states-edge', 'cleanup']
# The states a user's desktop leaves a browser in, and the real page read in each.
STATES = ['minimized', 'hidden', 'no-windows', 'not-running']
STATE_PAGE = {'do': 'open', 'url': 'https://github.com/rust-lang/rust', 'expect': ['rust-lang', 'Issues']}

# One real page per layout. A step's checks: `expect` (in the page as read), `absent`, `gains` (it
# appears more often than in the page as read before this step), `address` (in the address the
# browser ended at), `notes` (in the tool's notes).
PAGES = [
    {'name': 'long article, and a link followed', 'steps': [
        {'do': 'open', 'url': 'https://en.wikipedia.org/wiki/Accessibility',
         'expect': ['Accessibility', 'assistive technology']},
        {'do': 'press', 'kind': 'link', 'name': '^Assistive technology$', 'address': 'Assistive_technology',
         'expect': ['Assistive technology']}]},
    {'name': 'accordion (W3C pattern): a closed panel opened', 'steps': [
        # A closed panel is said to be closed, an open one open, and a plain button neither.
        {'do': 'open', 'url': 'https://www.w3.org/WAI/ARIA/apg/patterns/accordion/examples/accordion/',
         'expect': ['button "Personal Information" expanded', 'button "Billing Address" collapsed'],
         'absent': ['"Open In CodePen" collapsed']},
        {'do': 'press', 'name': '^Billing Address', 'gains': ['Zip Code']}]},
    {'name': 'accordion (Bootstrap): a closed panel opened', 'steps': [
        # Its code sample keeps its lines and their indents (the button's label is on a line of
        # its own, eight spaces in) and reads as the page shows it, no space added inside a
        # tag, in every browser.
        {'do': 'open', 'url': 'https://getbootstrap.com/docs/5.3/components/accordion/',
         'expect': ['Accordion Item #2', '\n        Accordion Item #1\n', '<div class="accordion" id="accordionExample">\n'],
         'absent': ['< div', 'class= "']},
        {'do': 'press', 'name': '^Accordion Item #2$', 'gains': ['second item’s accordion body']}]},
    {'name': 'tab list (W3C pattern): another tab shown', 'steps': [
        {'do': 'open', 'url': 'https://www.w3.org/WAI/ARIA/apg/patterns/tabs/examples/tabs-automatic/',
         'expect': ['Carl Andersen']},
        {'do': 'press', 'kind': 'tab', 'name': '^Carl Andersen', 'gains': ['Carl Joachim Andersen']}]},
    {'name': 'table of links, and its "More"', 'steps': [
        {'do': 'open', 'url': 'https://news.ycombinator.com/', 'expect': ['Hacker News']},
        {'do': 'press', 'kind': 'link', 'name': '^More$', 'address': 'p=2'}]},
    {'name': 'a page that loads more as it scrolls', 'steps': [
        {'do': 'open', 'url': 'https://infinite-scroll.com/demo/full-page/', 'expect': ['1a - Infinite Scroll full page demo']},
        {'do': 'scroll', 'gains': ['2a - RGB Schemes logo']}]},
    {'name': 'web app (a GitHub repository), and a link followed', 'steps': [
        {'do': 'open', 'url': 'https://github.com/rust-lang/rust', 'expect': ['rust-lang']},
        {'do': 'press', 'kind': 'link', 'name': '^Issues', 'address': '/issues'}]},
    {'name': 'a page that sends a signed-out visitor to its sign-in page', 'steps': [
        {'do': 'open', 'url': 'https://github.com/settings/profile', 'address': '/login',
         'notes': ['rather than the address asked for']}]},
    # The browser spells this address /wiki/M%C3%BCnchen: the reader still knows its tab and
    # does not say the site sent it elsewhere. And the page is German: Chrome and Brave lay
    # "Translate this page?" over it, whose two language buttons are tab buttons to Accessibility.
    {'name': 'an address with a letter outside ASCII, on a page the browser offers to translate', 'steps': [
        {'do': 'open', 'url': 'https://de.wikipedia.org/wiki/München', 'expect': ['Landeshauptstadt'],
         'address': 'de.wikipedia.org/wiki/M', 'quiet': ['rather than the address asked for']}]},
]


class StepFailed(Exception):
    pass


def command(argv, timeout=120):
    r = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=timeout)
    if r.returncode:
        raise StepFailed('command failed (%d): %s\n%s%s' % (r.returncode, ' '.join(map(str, argv)), r.stdout, r.stderr))
    return r.stdout


def browser_page_walk():
    spec = importlib.util.spec_from_file_location('browser_page_walk', HERE / 'browser-page-walk.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def whole(answer):
    """The page of an answer: the driver joins a long page's parts as `whole`."""
    if not isinstance(answer, dict):
        return ''
    return answer.get('whole') or answer.get('page') or ''


def judge(step, row, before):
    """What a step's row lacks, as sentences; empty when it passed."""
    if 'failed' in row:
        return [row['failed']]
    answer = row.get('answer')
    if row.get('error') or not isinstance(answer, dict):
        return ['the tool refused: %s' % str(answer)[:600]]
    page, problems = whole(answer), []
    for phrase in step.get('expect', []):
        if phrase not in page:
            problems.append('"%s" is not in the page as read' % phrase)
    for phrase in step.get('absent', []):
        if phrase in page:
            problems.append('"%s" is in the page as read' % phrase)
    for phrase in step.get('gains', []):
        if page.count(phrase) <= before.count(phrase):
            problems.append('"%s" did not appear (%d before, %d after)' % (phrase, before.count(phrase), page.count(phrase)))
    if step.get('address') and step['address'] not in answer.get('address', ''):
        problems.append('the browser ended at %s, not at an address with "%s"' % (answer.get('address'), step['address']))
    said = ' '.join(answer.get('notes', []))
    for phrase in step.get('notes', []):
        if phrase not in said:
            problems.append('the tool\'s notes do not say "%s": %s' % (phrase, said[:300]))
    for phrase in step.get('quiet', []):
        if phrase in said:
            problems.append('the tool\'s notes say "%s": %s' % (phrase, said[:300]))
    return problems


class Walk:
    def __init__(self, a):
        import os
        self.a, self.vm = a, a.vm
        a.out.mkdir(parents=True, exist_ok=True)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.payload = (state / 'payload').read_text().strip()
        self.helper = self.payload + '/browser-page-guest.py'
        self.driver = self.payload + '/browser-reader-guest.py'
        self.exe = self.payload + '/richos-tauri'
        self.base = f'http://127.0.0.1:{PORT}'
        self.page_walk = browser_page_walk().Walk(argparse.Namespace(vm=self.vm, out=a.out))

    def g(self, cmd, timeout=60):
        return guest(self.vm, cmd, timeout)

    def drive(self, steps, timeout=1500):
        """The driver's rows for these steps, in one life of the tool server."""
        plan = self.payload + '/reader-plan.json'
        local = self.a.out / 'reader-plan.json'
        local.write_text(json.dumps({'steps': steps}))
        command([HERE / 'guest.sh', self.vm, '--push', local, plan])
        local.unlink()
        out = self.g(f'python3 {shlex.quote(self.driver)} --exe {shlex.quote(self.exe)} < {shlex.quote(plan)}', timeout)
        return json.loads(out.splitlines()[-1])

    def running(self, bundle):
        return self.g(f'lsappinfo info -only pid -app {bundle} || true').split('=')[-1].strip().isdigit()

    def install(self, name):
        b = BROWSERS[name]
        app, installed = b['app'], False
        if self.g(f'test -d {shlex.quote(app)} && echo yes || true') != 'yes' and 'pkg' in b:
            pkg, parts = f'{self.payload}/{name}.pkg', f'{self.payload}/{name}-pkg'
            self.g(f'curl -fsSL -o {shlex.quote(pkg)} {shlex.quote(b["pkg"])}', 1800)
            self.g(f'rm -rf {shlex.quote(parts)} && pkgutil --expand-full {shlex.quote(pkg)} {shlex.quote(parts)}', 900)
            found = self.g(f'find {shlex.quote(parts)} -maxdepth 4 -type d -name {shlex.quote(Path(app).name)} | head -1')
            if not found:
                raise StepFailed(f'{name}: no {Path(app).name} in its installer package')
            self.g(f'cp -R {shlex.quote(found)} /Applications/ && rm -rf {shlex.quote(parts)} {shlex.quote(pkg)}', 600)
            installed = True
        elif self.g(f'test -d {shlex.quote(app)} && echo yes || true') != 'yes':
            dmg = f'{self.payload}/{name}.dmg'
            self.g(f'curl -fsSL -o {shlex.quote(dmg)} {shlex.quote(b["dmg"])}', 900)
            mount = self.g(f'hdiutil attach -nobrowse -readonly {shlex.quote(dmg)} | tail -1', 180).split('\t')[-1].strip()
            self.g(f'cp -R {shlex.quote(mount + "/" + Path(app).name)} /Applications/ && hdiutil detach {shlex.quote(mount)}', 600)
            installed = True
        for cmd in b.get('before', []):
            self.g(cmd)
        self.g(f'open -g -a {shlex.quote(app)}' + (' --args ' + ' '.join(b['flags']) if b['flags'] else ''))
        for _ in range(60):
            if self.running(b['bundle']):
                break
            time.sleep(0.5)
        else:
            raise StepFailed(f'{name} did not start in the guest')
        time.sleep(5)
        version = self.g(f'defaults read {shlex.quote(app + "/Contents/Info")} CFBundleShortVersionString || true')
        return {'installed': installed, 'version': version}

    # --- steps --------------------------------------------------------------------------------
    def site(self):
        return self.page_walk.site()

    def chrome(self):
        return self.page_walk.chrome()

    def firefox(self):
        return self.install('firefox')

    def brave(self):
        return self.install('brave')

    def edge(self):
        return self.install('edge')

    def push_exe(self):
        """The executable into the guest. One inside an app bundle (a candidate's RichOS.app) goes
        with its whole bundle, packed with ditto and unpacked in the payload: taken out of its
        bundle, a signed release executable no longer matches its signature ("invalid Info.plist")
        and a lone copy ended in the guest before it answered `initialize`. A bare built
        executable is pushed as it is."""
        exe = self.a.exe.resolve()
        bundle = next((d for d in exe.parents if d.suffix == '.app'), None)
        if not bundle:
            command([HERE / 'guest.sh', self.vm, '--push', exe, self.exe], 900)
            self.g(f'chmod +x {shlex.quote(self.exe)}')
            return {'pushed': 'executable'}
        packed = self.a.out / (bundle.stem + '-under-test.zip')
        subprocess.run(['/usr/bin/ditto', '-c', '-k', '--keepParent', str(bundle), str(packed)], check=True)
        try:
            command([HERE / 'guest.sh', self.vm, '--push', packed, self.payload + '/bundle.zip'], 900)
        finally:
            packed.unlink(missing_ok=True)
        self.g(f'mkdir -p {shlex.quote(self.payload)}/bundle && /usr/bin/ditto -x -k {shlex.quote(self.payload)}/bundle.zip '
               f'{shlex.quote(self.payload)}/bundle && rm -f {shlex.quote(self.payload)}/bundle.zip', 300)
        self.exe = f'{self.payload}/bundle/{bundle.name}/{exe.relative_to(bundle)}'
        verify = self.g(f'codesign -v {shlex.quote(self.payload + "/bundle/" + bundle.name)} 2>&1 && echo signature-valid || true', 120)
        return {'pushed': 'bundle', 'bundle': bundle.name, 'exe': self.exe, 'codesign': verify.strip()[-200:]}

    def tool(self):
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'browser-reader-guest.py', self.driver])
        pushed = self.push_exe()
        result = self.drive([{'do': 'tools'}], 120)
        tools = result['rows'][0].get('answer')
        if tools != TOOLS:
            raise StepFailed(f'the tool server listed {tools}: {result}')
        return {'tools': tools, 'server_exit': result['server_exit'], 'exe': pushed}

    def make_default(self, name):
        """The browser made the guest's default and running. The default-browser helper's row."""
        b = BROWSERS[name]
        row = json.loads(self.g(f'python3 {shlex.quote(self.helper)} default-browser --bundle {b["bundle"]} || true', 90).splitlines()[-1])
        if row.get('after') != b['bundle'].lower():
            raise StepFailed(f'{name} could not be made the default browser in the guest: {row}')
        if not self.running(b['bundle']):
            self.g(f'open -g -a {shlex.quote(b["app"])}')
            time.sleep(6)
        return row

    def window_state(self, name, state):
        out = self.g(f'python3 {shlex.quote(self.helper)} window-state --bundle {BROWSERS[name]["bundle"]} --state {state} || true', 90)
        return json.loads(out.splitlines()[-1])

    def states(self, name):
        """One real page read with the browser in each state a user's desktop leaves it in."""
        b = BROWSERS[name]
        self.make_default(name)
        ask = {k: v for k, v in STATE_PAGE.items() if k in ('do', 'url')}
        cases, failed = [], []
        if b.get('first_run'):
            self.g('touch "$HOME"/' + shlex.quote(b['first_run']))
        for state in STATES:
            # The user's browser as they left it: running, showing a page of their own.
            self.g(f'open -g -b {b["bundle"]} {self.base}/article')
            time.sleep(5)
            self.window_state(name, 'normal')
            put = self.window_state(name, state)
            after = put.get('after') or {}
            reached = {'minimized': bool(after.get('windows')) and all(w.get('minimized') for w in after.get('windows', [])),
                       'hidden': after.get('hidden') is True,
                       'no-windows': after.get('running') is True and not after.get('windows'),
                       'not-running': after.get('running') is False}[state]
            case = {'state': state, 'reached': reached, 'put': after, 'errors': put.get('errors')}
            for attempt in ('first open', 'second open'):
                result = self.drive([ask, {'do': 'close'}], 400)
                rows = result['rows']
                problems = judge(STATE_PAGE, rows[0], '') if rows else ['the tool server gave no answer']
                answer = rows[0].get('answer') if rows else None
                case[attempt] = {'outcome': 'not read' if problems else 'read', 'seconds': rows[0].get('seconds') if rows else None,
                                 'characters': len(whole(answer)), 'closed': (rows[1].get('answer') if len(rows) > 1 else None),
                                 'notes': answer.get('notes') if isinstance(answer, dict) else None,
                                 **({'problems': problems, 'answer': str(answer)[:700]} if problems else {})}
                if not problems:
                    break
            case['left'] = self.window_state(name, 'observe').get('after')
            cases.append(case)
            if all(case.get(k, {}).get('outcome') != 'read' for k in ('first open', 'second open')):
                failed.append(state)
        # Back to a running browser with a window, for whatever step follows.
        self.g(f'open -g -b {b["bundle"]} {self.base}/article')
        time.sleep(4)
        self.window_state(name, 'normal')
        summary = {'browser': name, 'page': STATE_PAGE['url'], 'states': cases}
        (self.a.out / f'{name}-states.json').write_text(json.dumps(summary, indent=2) + '\n')
        if failed:
            raise StepFailed(f'{name}: FAILED with the browser ' + ', '.join(failed) + ' — ' + json.dumps(summary))
        return summary

    def read(self, name):
        b = BROWSERS[name]
        default = self.make_default(name)
        # The browser's own session for the local site: its /sign-in sets the cookie.
        self.g(f'open -g -b {b["bundle"]} {self.base}/sign-in')
        time.sleep(4)
        pages = [p for p in PAGES if self.a.pages.search(p['name'])] + [
            {'name': 'a page only a signed-in browser is shown', 'steps': [
                {'do': 'open', 'url': f'{self.base}/profile', 'expect': [SIGNED_IN], 'absent': [SIGNED_OUT]}]},
            # A code sample reads character for character as written, between the lines around it.
            {'name': 'a code sample whose spacing is part of the program', 'steps': [
                {'do': 'open', 'url': f'{self.base}/code', 'expect': ['# Code\n' + CODE + '\nEnd.']}]},
            # Last, so the closing that follows has two tabs of the reader's own to close.
            {'name': 'a link that opens in a new tab', 'steps': [
                {'do': 'open', 'url': f'{self.base}/links', 'expect': ['Profile in a new tab']},
                {'do': 'press', 'kind': 'link', 'name': '^Profile in a new tab$', 'address': '/profile',
                 'expect': [SIGNED_IN]}]}]
        steps = [s for p in pages for s in p['steps']] + [{'do': 'close'}]
        result = self.drive([{k: v for k, v in s.items() if k in ('do', 'url', 'name', 'kind')} for s in steps])
        # How many lines of each reading begin with spacing: an indent is kept for text the page
        # shows as written, so a page of plain prose should have next to none.
        for r in result['rows']:
            if isinstance(r.get('answer'), dict) and 'page' in r['answer']:
                lines = whole(r['answer']).split('\n')
                r['lines'], r['indented_lines'] = len(lines), sum(1 for line in lines if line[:1] in (' ', '\t'))
        (self.a.out / f'{name}-rows.json').write_text(json.dumps(result, indent=2) + '\n')
        rows = result['rows']
        plain = self.g(f'curl -fsS {self.base}/profile || true')
        judged, failed, n, before = [], [], 0, ''
        for p in pages:
            problems, seconds = [], 0.0
            for s in p['steps']:
                row = rows[n] if n < len(rows) else {'failed': 'the tool server had ended before this step'}
                n += 1
                problems += judge(s, row, before)
                seconds += row.get('seconds', 0)
                before = whole(row.get('answer'))
                last = row.get('answer') if isinstance(row.get('answer'), dict) else {}
            judged.append({'page': p['name'], 'outcome': 'FAIL' if problems else 'PASS', 'seconds': round(seconds, 1),
                           'address': last.get('address'), 'characters': len(before), 'notes': last.get('notes'),
                           **({'problems': problems} if problems else {})})
            if problems:
                failed.append(p['name'])
        closed = rows[n].get('answer') if n < len(rows) else None
        closed = closed.get('closed') if isinstance(closed, dict) else str(closed)
        if 'is closed' not in str(closed):
            failed.append('closing the tab')
        if SIGNED_OUT not in plain:
            failed.append('the local site (a plain fetch is not signed out)')
        tabs = self.g(f'python3 {shlex.quote(self.helper)} tab-bar --bundle {b["bundle"]} --depth 6 --max 400 || true', 120)
        (self.a.out / f'{name}-tab-bar-after.txt').write_text(tabs + '\n')
        summary = {'browser': name, 'default': default, 'pages': judged, 'closed': closed, 'server_exit': result.get('server_exit'),
                   'passed': len(pages) - len([f for f in failed if f in [p['name'] for p in pages]]), 'of': len(pages)}
        if failed:
            raise StepFailed(f'{name}: FAILED on ' + '; '.join(failed) + ' — ' + json.dumps(summary))
        return summary

    def read_safari(self):
        return self.read('safari')

    def read_chrome(self):
        return self.read('chrome')

    def read_firefox(self):
        return self.read('firefox')

    def read_brave(self):
        return self.read('brave')

    def read_edge(self):
        return self.read('edge')

    def states_safari(self):
        return self.states('safari')

    def states_chrome(self):
        return self.states('chrome')

    def states_firefox(self):
        return self.states('firefox')

    def states_brave(self):
        return self.states('brave')

    def states_edge(self):
        return self.states('edge')

    def cleanup(self):
        pid = self.g(f'cat {self.page_walk.site_dir}/server.pid 2>/dev/null || true')
        if pid.isdigit():
            self.g(f'kill {pid} 2>/dev/null || true')
        return {'server_pid_stopped': pid}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--exe', type=Path, required=True,
                   help='the richos-tauri executable under test; one inside an app bundle is pushed with its bundle')
    p.add_argument('--expect-sha', default='')
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--pages', default='', type=re.compile, help='walk only the pages of PAGES whose name matches (the local pages are always walked)')
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
        except (StepFailed, RuntimeError, ValueError, KeyError, IndexError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        # A browser that fails is one result of several: the others are still walked. Without the
        # site or the tool there is nothing to walk.
        if row['outcome'] != 'PASS' and step in ('site', 'tool'):
            if 'cleanup' in steps:
                walk.cleanup()
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
