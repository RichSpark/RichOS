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
              now", any done/account sheet closed, the company "RV Park" added with its folder,
              the business questions "Not now"; the first conversation exists
  signed-out  Claude Code is on this Mac and nobody is signed in to it. A message typed and sent
              with real keys. PASS only when the turn ends with the not-signed-in cause and the
              screen names the way out (sign in), never the transient "usually fixes itself"
              sentence and never machinery rows
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

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('setup_walk', HERE / 'setup-walk.py')
setup_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(setup_walk)
StepFailed = setup_walk.StepFailed
command = setup_walk.command

STEPS = ['identity', 'setup', 'signed-out', 'update', 'signed-in']
RETURN_KEY = '36'
COMPANY = 'RV Park'

# THE DEAD ENDS, as the screen says them. Each is a sentence or token a new user must never be
# left looking at; see docs/development/first-run-dead-ends.md for the class each belongs to.
DEAD_ENDS = {
    'raw OS error': r'os error [0-9]+|Resource temporarily unavailable',
    'retry loop on a permanent cause': r'usually fixes itself',
    'machinery in the user view': r'control_request|system:init|outcome not recorded|Between turns',
    'account state unknown': r'No reading yet',
}


def dead_ends(text):
    """The dead-end names whose pattern matches TEXT (OCR or accessibility text)."""
    return sorted(name for name, pattern in DEAD_ENDS.items() if re.search(pattern, text or '', re.I))


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
    if screen_hits:
        return 'dead end on screen: ' + ', '.join(screen_hits)
    if not re.search(r'sign in', screen_text or '', re.I):
        return 'the screen does not name the way out (sign in)'
    return None


def update_verdict(line, screen_hits):
    """None when the update row reached a verdict a user can act on, else why not."""
    if screen_hits:
        return 'dead end on screen: ' + ', '.join(screen_hits)
    if not line:
        return 'the update row says nothing'
    if re.search(r'could not', line, re.I):
        return 'the update check failed: ' + line
    return None


class NewUserWalk(setup_walk.SetupWalk):
    def __init__(self, a):
        super().__init__(a)
        self.company = self.home + '/' + COMPANY.replace(' ', '-')
        self.report = {}

    # --- evidence -----------------------------------------------------------------------------
    def frame(self, name):
        """A frame of the guest's screen, and the dead ends OCR finds on it."""
        self.shot(name + '.png')
        found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), '|'.join(DEAD_ENDS.values()) + '|sign in',
                                str(self.out / (name + '.png')), '--show'], capture_output=True, text=True, timeout=90)
        if found.returncode not in (0, 1):
            raise StepFailed('ocr-find could not read %s.png: %s' % (name, found.stderr.strip()))
        return found.stdout

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
        self.ax('click', '--title', 'Message to Rich', '--role', 'AXTextArea', '--contains', '--first')
        literal = words.replace('\\', '\\\\').replace('"', '\\"')
        command([HERE / 'guest.sh', self.vm, 'osascript', '-e',
                 'tell application "System Events" to keystroke "%s"' % literal], 120)
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
        words = 'what do I do now?'
        outcome, text = self.send(words, 'signed-out', self.a.turn_within)
        hits = dead_ends(text)
        self.report['signed-out'] = {'outcome': outcome, 'screen': text.strip()[-1500:], 'dead_ends': hits}
        self.pull()
        why = signed_out_verdict(outcome, hits, text)
        if why:
            raise StepFailed(why)
        return self.report['signed-out']

    def update(self):
        self.ax('click', '--id', 'set-btn', '--first')
        time.sleep(2)
        before = (self.by_id('update-line') or {}).get('value')
        if self.by_id('update-check'):
            self.ax('click', '--id', 'update-check', '--first')
        line = before
        end = time.monotonic() + 90
        while time.monotonic() < end:
            time.sleep(3)
            line = (self.by_id('update-line') or {}).get('value')
            if line and not re.search(r'checking|looking', line, re.I):
                break
        text = self.frame('update')
        if self.by_id('update-why'):
            self.ax('click', '--id', 'update-why', '--first')
            time.sleep(1)
            text += self.frame('update-reason')
        hits = [h for h in dead_ends(text) if h == 'raw OS error']
        self.report['update'] = {'before': before, 'line': line, 'dead_ends': hits}
        command([HERE / 'ax.sh', self.vm, '--key', '53'])  # Escape closes Settings
        why = update_verdict(line, hits)
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
        self.report['signed-in'] = {'login': line, 'outcome': outcome, 'dead_ends': hits}
        self.pull()
        if not outcome or outcome['state'] != 'completed' or not outcome.get('rich_wrote'):
            raise StepFailed('Rich did not answer once signed in: %s' % json.dumps(outcome))
        if hits:
            raise StepFailed('dead end on screen once signed in: ' + ', '.join(hits))
        return self.report['signed-in']


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', default='', help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=1200, help='seconds for first setup (it fetches Claude Code and the engine)')
    p.add_argument('--turn-within', type=float, default=240, help='seconds for one message to Rich to end')
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
        a.out.mkdir(parents=True, exist_ok=True)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok and not a.keep_going:
            break
    walk.pull()
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
