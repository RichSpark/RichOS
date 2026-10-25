#!/usr/bin/env python3
"""new-user-walk.py — a new user's first run, from opening the app to Rich's first answer and an
update check, in a fresh guest, failing on every dead end a real new user hit (2026-10-21).

Run by run-walk.py with --new-user, which holds one guest slot for this run only, boots the guest,
launches the app with HOME and nothing else of the harness's (no RICHOS_CLAUDE_BIN, no
CLAUDE_CONFIG_DIR, no engine payload, no Claude login), and removes the clone:

  run-walk.py --new-user --bundle ZIP --home EMPTY_DIR --engine none --report REPORT --wait 600 -- \\
      new-user-walk.py --out DIR [--expect-sha SHA]

WHY IT EXISTS. Every other walk launches the app with the harness's claude path, the host's Claude
login and an engine handed in (run.sh), so not one of them was ever a first run as a customer has
it. A real user's first run on 1.2.1 hit four dead ends no walk could see: every message to Rich
ended "RichOS lost its connection to Claude Code ... This usually fixes itself" in a loop, the
Claude account said "No reading yet" with no verdict, the update check failed with
"Resource temporarily unavailable (os error 35)", and machinery rows (`control_request`,
`system:init`, "outcome not recorded") reached his conversation. This walk is the one that
can see them. The class and its checks: docs/development/first-run-dead-ends.md.

STEPS, in the guest, never on the host's screen (CEO ruling 65):
  identity    the running app says it was built from --expect-sha (skipped without one)
  setup       first setup as a new user does it: the setup sheet's "Set it up" (Claude Code, the
              engine and the video tools fetched by the app itself), the memory question "Not
              now", any done/account sheet closed, the fictional company "First Run Workshop" added with its folder,
              the business questions "Not now"; the first conversation exists
  signed-out  Claude Code is on this Mac and nobody is signed in to it. A message typed and sent
              with real keys. PASS only when the turn ends with the not-signed-in cause and the
              screen names the way out (sign in), never the transient "usually fixes itself"
              sentence and never machinery rows. Two messages, as a new user sends them
  accounts    Settings, Claude accounts, still signed out: PASS when the card says the account
              is not signed in and offers Sign in, never "No reading yet"; then press Sign in,
              start the real provider login, verify its browser page, cancel and retry
  update      Settings opened, the update check pressed once: PASS when the row reaches a
              verdict (up to date, available, ready) and never a raw OS error
  signed-in   the host's login pushed into this home's keychain (claude-login.sh push), as a
              person's `claude /login` leaves it; a message sent: PASS when Rich answers (a
              TurnCompleted row with his text) and no dead end is on screen
After every step the frame is read for the dead-end sentences (DEAD_ENDS below); any hit fails
the step that showed it.

Evidence: <out>/report.json, a frame per moment, <out>/app.log and <out>/ledger.jsonl pulled from
the guest. Exit 0 only when every step passes. The app is quit and the guest removed by
run-walk.py's stop.sh (CEO 54); its log's "clean:" line says so.
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
from urllib.parse import urlsplit

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('setup_walk', HERE / 'setup-walk.py')
setup_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(setup_walk)
StepFailed = setup_walk.StepFailed
# The one subprocess helper every walk uses lives in adopt-walk.py, which setup-walk.py loads.
command = setup_walk.adopt_walk.command

STEPS = ['identity', 'setup', 'signed-out', 'accounts', 'update', 'signed-in']
RETURN_KEY = '36'
COMPANY = 'First Run Workshop'

# THE DEAD ENDS, as the screen says them. Each is a sentence or token a new user must never be
# left looking at; see docs/development/first-run-dead-ends.md for the class each belongs to.
DEAD_ENDS = {
    'raw OS error': r'os error [0-9]+|Resource temporarily unavailable',
    'retry loop on a permanent cause': r'usually fixes itself',
    'machinery in the user view': r'control_request|system:init|outcome not recorded|Between turns',
    'account state unknown': r'No reading yet',
    # Claude Code's own terminal instruction, shown as Rich's reply on 1.2.1 (guest walk,
    # 2026-10-21): a person using the app has no terminal to run it in.
    "machine words as Rich's words": r'Please run /login',
}


def dead_ends(text):
    """The dead-end names whose pattern matches TEXT (OCR or accessibility text). Whitespace is
    folded first: OCR keeps the screen's line breaks, so "No reading\\nyet" (a sentence wrapped
    where the card wraps it) slipped past every pattern (review rv-20261021T193611Z-6df3f1f3-b313,
    finding 1)."""
    flat = re.sub(r'\s+', ' ', text or '')
    return sorted(name for name, pattern in DEAD_ENDS.items() if re.search(pattern, flat, re.I))


def turn_outcome(rows, words):
    """What happened to the turn that carried WORDS: None while it runs, else a dict with
    `state` (completed|interrupted), the interruption `cause` tag and sentence when there is one,
    and whether Rich wrote any text."""
    turn = next((r.get('turn_id') for r in rows if r.get('event') == 'PromptReceived'
                 and (r.get('text') or '').strip() == words), None)
    if not turn:
        return None
    out = {'turn_id': turn}
    for r in rows:
        if r.get('turn_id') != turn:
            continue
        if r.get('event') == 'TurnCompleted':
            out['state'] = 'completed'
        elif r.get('event') == 'TurnInterrupted':
            out['state'] = 'interrupted'
            out['reason'] = r.get('reason')
        elif r.get('event') == 'TurnInterruptionCause':
            rec = r.get('record') or {}
            out['cause'] = rec.get('cause')
            out['ceo_message'] = rec.get('ceo_message')
        elif r.get('event') == 'AssistantDelta' and (r.get('text') or '').strip():
            out['rich_wrote'] = True
    return out if 'state' in out else None


def signed_out_verdict(outcome, screen_hits, screen_text):
    """None when a signed-out first message ends where a new user can act, else why not."""
    if outcome is None:
        return 'the turn never ended'
    if outcome['state'] == 'completed':
        return 'the turn completed although nobody is signed in to Claude Code (harness login leaked in?)'
    if outcome.get('cause') != 'not-signed-in':
        return ('the turn ended as %r (%s), not as "not signed in": the user is told the wrong thing'
                % (outcome.get('cause'), outcome.get('reason')))
    if outcome.get('rich_wrote'):
        return "the turn failed and still recorded text as Rich's answer (the vendor's line as his words)"
    if screen_hits:
        return 'dead end on screen: ' + ', '.join(screen_hits)
    if not re.search(r'sign in', screen_text or '', re.I):
        return 'the screen does not name the way out (sign in)'
    return None


def accounts_verdict(text, sheet=None, sign=None):
    """None when Settings, Claude accounts tells a signed-out new user so, with the way to sign
    in, else why not (the dead end of 2026-10-21: "In use now" over "No reading yet").

    SHEET is the Claude accounts sheet's own close button as the accessibility tree reports it
    (present only while the sheet is open) and SIGN the card's own Sign in button
    (#acc-sign-one). Text alone is not enough: Settings' row says "Not signed in on this Mac"
    and the interruption before it says "sign in", so a frame could pass with no account card
    and no button at all (review rv-20261021T213648Z-b3f0990a-85f9, finding 2)."""
    hits = dead_ends(text)
    if hits:
        return 'dead end on screen: ' + ', '.join(hits)
    if not sheet:
        return 'the Claude accounts sheet did not open'
    if not sign:
        return "the account card has no Sign in button"
    if sign.get('enabled') is False:
        return 'the account card\'s Sign in button cannot be pressed'
    if not re.search(r'sign in', ' '.join(str(sign.get(k) or '') for k in ('title', 'desc', 'value')), re.I):
        return 'the account card\'s Sign in button does not say Sign in'
    if not re.search(r'not signed in', text or '', re.I):
        return 'the account card does not say the account is not signed in'
    if not re.search(r'sign in', re.sub(r'not signed in', '', text or '', flags=re.I), re.I):
        return 'the account card does not offer the sign-in'
    return None


def provider_page(url):
    """Only the provider host/path may enter evidence, never OAuth query, state or code."""
    try:
        parsed = urlsplit(url if '://' in url else 'https://' + url)
    except ValueError:
        return None
    if parsed.scheme != 'https' or parsed.hostname not in {
        'claude.ai', 'console.anthropic.com', 'platform.claude.com', 'auth.anthropic.com',
    }:
        return None
    return {'host': parsed.hostname, 'path': parsed.path}


# The update row's headlines once a check has an answer (ui/updates.js `words`): up to date,
# available, ready, going back, or a failure's headline. While it checks, the headline is the
# bare "RichOS <version>" with "Checking for updates…" under it, so the headline alone is no
# verdict (review rv-20261021T153457Z-fb99f171-52f1, finding 2).
UPDATE_VERDICT = r'is up to date\.|is available\.|is ready|will go back|could not|did not|cannot'


def update_settled(line, sub):
    """True once the update row (headline LINE, subtitle SUB) says how the check ended."""
    return bool(line) and bool(re.search(UPDATE_VERDICT, line, re.I)) \
        and not re.search(r'checking|looking|downloading|preparing', (sub or '') + ' ' + line, re.I)


def update_row(text):
    """(headline, subtitle) of Settings' Updates row, read from a frame's OCR TEXT: the two lines
    after the "Updates" heading. The accessibility tree's `value` of #update-line came back empty
    on the guest walk of 2026-10-21 (WebKit keeps a paragraph's words in its child text) while
    the frame said "RichOS 1.2.2 is up to date.", so the row is read the way a person reads it.
    A headline wrapped onto more lines ("... is up to" / "date.", nightly 71's walk of 2026-10-23)
    is one headline: a line starting in lower case continues the sentence above it."""
    lines = [l.strip() for l in (text or '').splitlines() if l.strip()]
    for i, line in enumerate(lines):
        if line == 'Updates':
            rest = lines[i + 1:]
            while len(rest) > 1 and rest[1][0].islower():
                rest[0:2] = [rest[0] + ' ' + rest[1]]
            rest = rest[:2] + [None, None]
            return rest[0], rest[1]
    return None, None


def settings_open(text):
    """Whether a frame's OCR TEXT shows the Settings panel (its first column's heading). Any case:
    the heading is 14px caps drawn by CSS, and OCR read it as "HOW IT Looks" on nightly 75's walk
    (2026-10-25) with the panel plainly open, so a case-sensitive match called an open panel shut."""
    return bool(re.search(r'^\s*HOW IT LOOKS\s*$', text or '', re.M | re.I))


# The open panel measured 1070x764 in the guest (nightly 75, 2026-10-25). After six presses in a
# row the same node once read 40x10 with nothing drawn, so a node is the open panel only at a size
# a person could read.
PANEL_MIN_PX = 200
# Seconds after the one press for the panel to be there (or gone). It was there within 0.3 s on
# every press of nightly 75's probe walk; ten is room for a slow guest, never a second press.
SETTINGS_WITHIN = 10


def panel_shown(node):
    """Whether #set-menu's accessibility node is the open panel: there, and of a readable size.
    The panel is `display: none` while closed, so it is absent from the tree then."""
    return bool(node) and (node.get('w') or 0) >= PANEL_MIN_PX and (node.get('h') or 0) >= PANEL_MIN_PX


def update_verdict(line, sub, screen_hits):
    """None when the update row reached a verdict a user can act on, else why not."""
    if screen_hits:
        return 'dead end on screen: ' + ', '.join(screen_hits)
    if not line:
        return 'the update row says nothing'
    if not update_settled(line, sub):
        return 'the update check never reached a verdict: %s / %s' % (line, sub)
    if re.search(r'could not|did not|cannot', line, re.I):
        return 'the update check failed: ' + line
    return None


class NewUserWalk(setup_walk.SetupWalk):
    def __init__(self, a):
        super().__init__(a)
        self.company = self.home + '/' + COMPANY.replace(' ', '-')
        self.report = {}
        # Every frame taken, with the dead ends on it (run_step judges each step by its frames).
        self.seen = []

    # --- evidence -----------------------------------------------------------------------------
    def frame(self, name):
        """A frame of the guest's screen and ALL of its text, read through the shared OCR cache
        (qa/ocr-read.py). Never ocr-find's `--show`, which prints at most three matching lines
        per frame, so three harmless lines could hide a dead end (review
        rv-20261021T153457Z-fb99f171-52f1, finding 1)."""
        self.shot(name + '.png')
        read = subprocess.run([sys.executable, str(HERE.parent / 'qa' / 'ocr-read.py'), str(self.out / (name + '.png'))],
                              capture_output=True, text=True, timeout=90)
        if read.returncode != 0:
            raise StepFailed('ocr-read could not read %s.png: %s' % (name, read.stderr.strip()))
        (self.out / (name + '.txt')).write_text(read.stdout)
        self.seen.append((name, dead_ends(read.stdout)))
        return read.stdout

    def ledger_rows(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        out = []
        for line in rows.splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
        return out

    def pull(self):
        for remote, local in ((self.log, 'app.log'), (self.data + '/conversation-ledger.jsonl', 'ledger.jsonl')):
            try:
                (self.out / local).write_text(guest(self.vm, 'cat ' + shlex.quote(remote) + ' 2>/dev/null || true', 60))
            except (RuntimeError, subprocess.TimeoutExpired):
                pass

    # --- steps --------------------------------------------------------------------------------
    def identity(self):
        if not self.a.expect_sha:
            line = guest(self.vm, 'grep -m1 "this app: built from" ' + shlex.quote(self.log) + ' || true')
            return {'built_from': line.rsplit(' ', 1)[-1] if line else None, 'expected': None}
        return super().identity()

    def setup(self):
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "RV Park notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        presses, n = [], 0
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            if self.present('Add this company'):
                break
            if self.present('Set it up') and self.shows_text('Where should I keep what you tell me?'):
                self.press('Not now')
                presses.append('memory question: Not now')
            elif self.present('Set it up'):
                n += 1
                self.frame('setup-sheet-%d' % n)
                self.press('Set it up')
                presses.append('setup sheet: Set it up')
            elif self.present('Close'):
                n += 1
                self.frame('setup-close-%d' % n)
                self.press('Close')
                presses.append('Close')
            elif self.present('Start', contains=False):
                self.press('Start', contains=False)
                presses.append('Start')
            else:
                time.sleep(2)
                continue
            time.sleep(2)
        else:
            self.frame('setup-stuck')
            raise StepFailed('the company question never came up within %g s; presses: %s' % (self.a.within, presses))
        self.type_into(COMPANY, '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        end = time.monotonic() + 60
        created = None
        while time.monotonic() < end and not created:
            created = next((r for r in self.ledger_rows() if r.get('event') == 'ThreadCreated'), None)
            if not created:
                time.sleep(1)
        if not created:
            raise StepFailed('no conversation was created after the company was added')
        time.sleep(2)
        if self.present('Start the questions'):
            self.press('Not now')
            presses.append('business questions: Not now')
        self.frame('first-conversation')
        self.report['setup'] = {'presses': presses, 'thread': created.get('thread_id')}
        return self.report['setup']

    def present(self, title, role='AXButton', app=None, contains=True):
        try:
            args = ['--title', title, '--role', role, '--first'] + (['--contains'] if contains else [])
            return bool([n for n in self.ax('find', *args, app=app) if not n.get('meta')])
        except StepFailed as exc:
            if axwait.absent(exc):
                return False
            raise

    def send(self, words, name, seconds):
        """Type WORDS into the composer with real keys, send with Return, and wait for its turn to end."""
        # ax.sh `type` sets the composer's focus and VERIFIES it before a word is sent
        # (`focusfailed` otherwise). A click on a text area does not focus it: on the guest walk
        # of 2026-10-21 the third message was typed into nothing once Settings had been opened
        # and closed, and the composer stayed empty.
        self.type_into(words, '--role', 'AXTextArea', '--title', 'Message to Rich')
        command([HERE / 'ax.sh', self.vm, '--key', RETURN_KEY])
        end = time.monotonic() + seconds
        outcome = None
        while time.monotonic() < end:
            outcome = turn_outcome(self.ledger_rows(), words)
            if outcome:
                break
            time.sleep(2)
        time.sleep(3)  # the card is drawn after the ledger row
        text = self.frame(name)
        return outcome, text

    def signed_out(self):
        """Two messages, as he sent them: the second is where a passing-hiccup sentence becomes a
        loop. Each must end not-signed-in, with the way out on screen and no dead end."""
        rows = []
        for n, words in enumerate(('what do I do now?', 'Test to see if something is working.'), 1):
            outcome, text = self.send(words, 'signed-out-%d' % n, self.a.turn_within)
            hits = dead_ends(text)
            rows.append({'words': words, 'outcome': outcome, 'screen': text.strip()[-1500:], 'dead_ends': hits,
                         'verdict': signed_out_verdict(outcome, hits, text)})
        self.report['signed-out'] = rows
        self.pull()
        why = '; '.join('message %d: %s' % (n, r['verdict']) for n, r in enumerate(rows, 1) if r['verdict'])
        if why:
            raise StepFailed(why)
        return self.report['signed-out']

    def accounts(self):
        """Settings, Claude accounts, signed out: the card says so and offers Sign in."""
        self.settings(True, 'accounts-settings')
        self.ax('click', '--id', 'set-accounts-open', '--first')
        end = time.monotonic() + 120
        text, sheet, sign = '', None, None
        while time.monotonic() < end:
            time.sleep(5)
            text = self.frame('accounts')
            sheet, sign = self.by_id('acc-close'), self.by_id('acc-sign-one')
            if sheet and (sign or 'No reading yet' in text):
                break
        why = accounts_verdict(text, sheet, sign)
        browser = None
        try:
            if why:
                raise StepFailed(why)
            browser = self.sign_in_browser()
            self.report['accounts'] = {'screen': text.strip()[-1500:], 'dead_ends': dead_ends(text),
                                       'sheet_open': bool(sheet),
                                       'sign_in': sign and {k: sign.get(k) for k in ('role', 'title', 'desc', 'enabled')},
                                       'browser_sign_in': browser}
            return self.report['accounts']
        finally:
            original = sys.exc_info()[1]
            try:
                guest(self.vm, "osascript -e 'tell application id \"com.richos.app\" to activate'")
                for control in ('provider-cancel', 'setup-close', 'acc-close'):
                    if self.by_id(control):
                        self.ax('click', '--id', control, '--first')
                self.settings(False, 'accounts-closed')
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as cleanup:
                if original is not None:
                    raise StepFailed('%s; account cleanup also failed: %s' % (original, cleanup)) from original
                raise

    def login_running(self):
        # Do not return process arguments: provider output and OAuth parameters are private.
        return guest(self.vm, "python3 - <<'PY'\nimport subprocess\nrows = subprocess.check_output(['ps', '-axo', 'command'], text=True).splitlines()\nprint(any(r.split()[-3:] == ['auth', 'login', '--claudeai'] for r in rows))\nPY").strip() == 'True'

    def browser_page(self):
        # Safari's AX address fields can retain the first tab's URL after a new login opens
        # another tab. Ask the browser for its current page, keeping the query only in memory.
        raw = guest(self.vm, "osascript -e 'tell application \"Safari\" to if (count windows) > 0 then get URL of current tab of front window'")
        safe = provider_page(raw)
        if not safe:
            return None, None
        try:
            nodes = self.ax('find', '--role', 'AXTextField', app='Safari')
            buttons = self.ax('find', '--role', 'AXButton', app='Safari')
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return None, None
            raise
        # A provider URL alone also appears on Safari's network-error page. Require actual
        # sign-in controls as well, never merely the address handed to the browser.
        labels = ' '.join(str(node.get(k) or '') for node in nodes + buttons
                          for k in ('title', 'desc'))
        if re.search(r'\b(email|log in|sign in|continue with (google|apple|email))\b', labels, re.I):
            return raw, safe
        return None, None

    def sign_in_browser(self):
        """Real app action, real provider login/browser, cancel and retry. Completing external
        authentication is separate: signed_in() projects a login and must never claim OAuth proof."""
        self.ax('click', '--id', 'acc-sign-one', '--first')
        end = time.monotonic() + 60
        while not self.by_id('provider-connect'):
            if time.monotonic() >= end:
                raise StepFailed('Sign in did not open the account connection sheet')
            time.sleep(1)
        attempts, previous_url = [], None
        for n in (1, 2):
            if self.login_running():
                raise StepFailed('a previous provider login is still running before attempt %d' % n)
            self.ax('click', '--id', 'provider-connect', '--first')
            end = time.monotonic() + 90
            process_seen, page_seen, new_page_seen, safe = False, False, False, None
            while time.monotonic() < end:
                process_seen = self.login_running() or process_seen
                raw, page = self.browser_page()
                page_seen = bool(page) or page_seen
                new_page_seen = bool(raw and raw != previous_url) or new_page_seen
                if process_seen and raw and raw != previous_url:
                    safe, previous_url = page, raw
                    break
                time.sleep(1)
            if not safe:
                raise StepFailed('Sign in attempt %d did not start provider login and open a new provider page '
                                 '(process observed: %s; provider page observed: %s; new URL observed: %s)'
                                 % (n, process_seen, page_seen, new_page_seen))
            self.ax('click', '--id', 'provider-cancel', '--first')
            guest(self.vm, "osascript -e 'tell application id \"com.richos.app\" to activate'")
            end = time.monotonic() + 30
            while self.login_running() or not self.by_id('provider-connect') or not self.shows_text('Sign-in was canceled'):
                if time.monotonic() >= end:
                    raise StepFailed('sign-in cancellation did not stop its process, restore Connect account and explain the cancellation')
                time.sleep(1)
            # The live frame is still retained and scanned for dead ends. Its OCR lost the
            # initial S of this modal's sentence, so the actual accessible text decides it.
            self.frame('sign-in-cancelled-%d' % n)
            attempts.append({'attempt': n, 'provider_process_seen': process_seen,
                             'browser_page': safe, 'cancelled': True})
        self.ax('click', '--id', 'setup-close', '--first')
        return {'attempts': attempts, 'external_authentication_completed': False}

    def settings(self, want_open, name):
        """Open or close Settings the way a person does, with ONE press of its own button, and check
        the frame: Escape left it open on the guest walk of 2026-10-21, and the next message was
        typed into nothing. Returns the frame's text with Settings in the state asked for.

        The button TOGGLES, so it is never pressed a second time on a reading that only might be
        wrong. Nightly 75's walk (2026-10-25) pressed it again whenever the frame's OCR missed the
        heading: the first press opened the panel, OCR read its heading "HOW IT Looks", and the
        second press closed it, so "Settings did not open" was reported with the panel never
        failing to open. The state is now the panel's own node (`panel_shown`) AND the frame
        (`settings_open`), both read again for up to SETTINGS_WITHIN seconds after the one press."""
        guest(self.vm, "osascript -e 'tell application id \"com.richos.app\" to activate'")
        node = self.by_id('set-menu')
        if panel_shown(node) != want_open:
            self.ax('click', '--id', 'set-btn', '--first')
        end = time.monotonic() + SETTINGS_WITHIN
        while True:
            if panel_shown(node) == want_open:
                text = self.frame(name)
                if settings_open(text) == want_open:
                    return text
            if time.monotonic() >= end:
                break
            time.sleep(1)
            node = self.by_id('set-menu')
        self.frame(name)
        raise StepFailed('Settings did not %s within %g s of one press of its button (panel node: %s)'
                         % ('open' if want_open else 'close', SETTINGS_WITHIN,
                            node and {k: node.get(k) for k in ('x', 'y', 'w', 'h')}))

    def update(self):
        text = self.settings(True, 'update-before')
        before = update_row(text)
        if self.by_id('update-check'):
            self.ax('click', '--id', 'update-check', '--first')
        line, sub = before
        # Until the row says how the check ended (update_settled), never on an in-between state,
        # read off the frame as a person reads it (update_row).
        end = time.monotonic() + self.a.update_within
        while time.monotonic() < end:
            time.sleep(5)
            text = self.frame('update')
            line, sub = update_row(text)
            if update_settled(line, sub):
                break
        if self.by_id('update-why'):
            self.ax('click', '--id', 'update-why', '--first')
            time.sleep(1)
            text += self.frame('update-reason')
        hits = [h for h in dead_ends(text) if h == 'raw OS error']
        self.report['update'] = {'before': before, 'line': line, 'sub': sub, 'dead_ends': hits}
        self.settings(False, 'update-closed')
        why = update_verdict(line, sub, hits)
        if why:
            raise StepFailed(why)
        return self.report['update']

    def signed_in(self):
        line = command([HERE / 'claude-login.sh', 'push', self.vm, self.home], 120).strip()
        if line != 'claude login: guest logged in':
            raise StepFailed('could not sign the guest in: ' + line)
        words = 'Reply with one word: received.'
        outcome, text = self.send(words, 'signed-in', self.a.turn_within)
        hits = dead_ends(text)
        self.report['signed-in'] = {'login': line, 'outcome': outcome, 'dead_ends': hits,
                                    'credential_source': 'host projection',
                                    'external_authentication_completed': False}
        self.pull()
        if not outcome or outcome['state'] != 'completed' or not outcome.get('rich_wrote'):
            raise StepFailed('Rich did not answer once signed in: %s' % json.dumps(outcome))
        if hits:
            raise StepFailed('dead end on screen once signed in: ' + ', '.join(hits))
        return self.report['signed-in']


def frames_with_dead_ends(seen):
    """'frame: dead end, ...; ...' for every captured (frame, hits) pair that has one, else ''."""
    return '; '.join('%s: %s' % (name, ', '.join(hits)) for name, hits in seen if hits)


def run_step(walk, step):
    """Walk STEP and judge it, AND every frame it captured against every known dead end. A step's
    own verdict looks at the part of the screen it is about; a dead end anywhere else on a frame it
    took still fails it (review rv-20261021T183543Z-ad01d021-62a5: the update step kept only
    'raw OS error' and passed a frame showing 'Between turns')."""
    start = len(walk.seen)
    row = {'step': step}
    try:
        row['evidence'] = getattr(walk, step.replace('-', '_'))()
        row['outcome'] = 'PASS'
    except (StepFailed, RuntimeError, ValueError, KeyError, IndexError, subprocess.TimeoutExpired) as exc:
        row['outcome'] = 'FAIL'
        row['detail'] = str(exc)
    hits = frames_with_dead_ends(walk.seen[start:])
    if hits:
        row['frame_dead_ends'] = hits
        if row['outcome'] == 'PASS':
            row['outcome'] = 'FAIL'
            row['detail'] = 'dead end on screen: ' + hits
    return row


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', default='', help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=1200, help='seconds for first setup (it fetches Claude Code and the engine)')
    p.add_argument('--turn-within', type=float, default=240, help='seconds for one message to Rich to end')
    p.add_argument('--update-within', type=float, default=180,
                   help='seconds for the update check to reach a verdict; past it the step fails, never passes')
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--keep-going', action='store_true',
                   help='walk every step even after one fails (to collect evidence; the exit status still fails)')
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = NewUserWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = run_step(walk, step)
        ok = ok and row['outcome'] == 'PASS'
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        a.out.mkdir(parents=True, exist_ok=True)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok and not a.keep_going:
            break
    # THE WALK'S OWN END, written only here: report.json is rewritten after every step, so a walk
    # that stopped after `identity` left a report of passes the gate would have read as a pass
    # (review rv-20261021T163510Z-b612361c-0e62, finding 3). `complete` says every step asked
    # for was walked; nightly-local.py require_new_user_walk refuses a report without it.
    report['complete'] = [row['step'] for row in report['steps']] == steps
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    walk.pull()
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
