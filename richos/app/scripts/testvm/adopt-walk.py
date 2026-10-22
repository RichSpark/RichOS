#!/usr/bin/env python3
"""adopt-walk.py — a task given from the phone starts without a typed message (CEO ruling §88).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      adopt-walk.py --out DIR --expect-sha SHA [--task TEXT] [--within SECONDS]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset, still
as one run under run-walk.py, for finding a step that changed.

THE QUESTION. CEO ruling §88: "The user should be able to answer on their phone just as well as
they can answer on the desktop app." Work Rich writes down during a turn (`richos_assignments`
registers it) starts at that turn's boundary. Until cc/echo-opus-adopt1 only the typed send
adopted it; a task given from the phone sat Registered until he next typed
(esc-20260926T083231Z-23865924). This walk gives the task from the phone, on a fresh install,
and never types into the Mac's composer.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity    the running app says it was built from --expect-sha
  first-run   memory setup declined, a company registered with a folder that is a repository,
              the business questions declined; the first conversation is the app's own
  watch       adopt-watch.py started in the guest: every assignment state, prompt and intake
              record on the guest's clock (no prompt text)
  pair        "Use Rich from your phone" -> Use Tailscale -> Set my phone up -> the pairing page
              in the guest's Safari (the phone) -> They match, on both sides
  phone-task  the task typed into the PHONE's composer and sent from there
  observe     within --within seconds: an assignment on that conversation leaves Registered, and
              every prompt on it after pairing came through the phone's intake channel — no desk
              record, no typed prompt. Evidence is pulled to --out.

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import json
import os
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

STEPS = ['identity', 'first-run', 'watch', 'pair', 'phone-task', 'observe']
TASK = ('Please add a file named hello.txt containing the single word hello to the Acme repository '
        'and land it.')


class StepFailed(Exception):
    pass


def command(argv, timeout=30):
    r = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=timeout)
    if r.returncode:
        raise StepFailed('command failed (%d): %s\n%s%s' % (r.returncode, ' '.join(map(str, argv)), r.stdout, r.stderr))
    return r.stdout


class Walk:
    def __init__(self, a):
        self.a = a
        self.vm = a.vm
        self.out = a.out
        self.out.mkdir(parents=True, exist_ok=True)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.payload = (state / 'payload').read_text().strip()
        self.home = self.payload + '/home'
        self.data = self.home + '/Library/Application Support/com.richos.app'
        self.company = self.home + '/Acme'
        self.watch_out = self.payload + '/adopt-watch.jsonl'
        self.facts = {}
        facts = self.out / 'facts.json'
        if facts.exists():
            self.facts = json.loads(facts.read_text())

    def save(self):
        (self.out / 'facts.json').write_text(json.dumps(self.facts, indent=2) + '\n')

    # --- the guest's screen -----------------------------------------------------------------
    def ax(self, mode, *args, app=None, timeout=40):
        argv = [HERE / 'ax.sh', self.vm, mode, *args, '--json']
        if app:
            argv += ['--app', app]
        return [json.loads(s) for s in command(argv, timeout).splitlines() if s.startswith('{')]

    def press(self, title, role='AXButton', app=None, contains=True):
        args = ['--title', title, '--role', role, '--first']
        if contains:
            args.append('--contains')
        return self.ax('click', *args, app=app)

    def present(self, title, role='AXButton', app=None):
        """Whether it is on screen. A find the app did not answer is "not yet" (axwait.py), so
        the caller's own wait asks again; its last such answer is kept for that wait's failure."""
        try:
            return bool(self.ax('find', '--title', title, '--role', role, '--contains', '--first', app=app))
        except StepFailed as exc:
            if not axwait.not_yet(exc, 'find'):
                raise
            if axwait.no_answer(exc):
                self.unanswered = str(exc).strip()[-300:]
            return False

    def wait_for(self, title, role='AXButton', app=None, seconds=30):
        end = time.monotonic() + seconds
        self.unanswered = None
        while time.monotonic() < end:
            if self.present(title, role, app):
                return True
            time.sleep(1)
        why = f'; the last read had no answer: {self.unanswered}' if self.unanswered else ''
        raise StepFailed(f'{role} "{title}" never appeared within {seconds} s{why}')

    def type_into(self, text, *selector, app=None):
        # ax.sh type pastes through the guest clipboard and refuses one that holds no text.
        command([HERE / 'ax.sh', self.vm, 'set the clipboard to "seed"'])
        return self.ax('type', text, *selector, '--contains', '--first', '--replace', app=app)

    def shot(self, name):
        command([HERE / 'shot.sh', self.vm, self.out / name], 60)

    def clock(self):
        return float(guest(self.vm, "python3 -c 'import time; print(time.time()*1000)'"))

    # --- steps --------------------------------------------------------------------------------
    def identity(self):
        line = guest(self.vm, 'grep -m1 "this app: built from" ' + shlex.quote(self.payload + '/app.log'))
        built = line.rsplit(' ', 1)[-1]
        if not built.startswith(self.a.expect_sha):
            raise StepFailed(f'the running app was built from {built}, not {self.a.expect_sha}')
        self.facts['built_from'] = built
        return {'built_from': built}

    # --- connecting a folder: the ONE copy every walk uses -----------------------------------
    def keys(self, applescript):
        command([HERE / 'ax.sh', self.vm, applescript], 90)

    def chooser_open(self):
        """The system folder chooser is up: an AXSheet, or its own Open button (folders-walk.py).
        None when a read had no answer: neither up nor gone, so wait_chooser() asks again."""
        unknown = False
        for args in (('--role', 'AXSheet'), ('--title', 'Open', '--role', 'AXButton')):
            try:
                if [n for n in self.ax('find', *args) if 'x' in n and not n.get('meta')]:
                    return True
            except StepFailed as exc:
                if not axwait.not_yet(exc, 'find'):
                    raise
                unknown = unknown or not axwait.absent(exc)
        return None if unknown else False

    def wait_chooser(self, want, seconds=15):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if self.chooser_open() == want:
                return True
            time.sleep(1)
        return False

    def open_connected_folders(self, settings_open=False):
        """The Connected folders sheet, open: Settings (unless the caller already has it open), then
        the row pressed as the plain button it is since round 27, then the sheet's Company menu."""
        if not settings_open:
            self.press('Settings', role='AXPopUpButton')
        self.press('Connected folders')
        self.wait_for('Company', role='AXPopUpButton')
        time.sleep(2)

    def wait_open_enabled(self, seconds=15):
        """Wait for the chooser's Open button to read as enabled, then return so the caller clicks
        it once. Typing the path and Return makes the chooser load the folder; on a busy host
        Open stays disabled for seconds (walk-9696d0b3a4f1: "matched element is disabled"). A read
        the app did not answer is "not yet" (axwait.py); the click is never sent here."""
        end = time.monotonic() + seconds
        state = 'not found'
        while True:
            try:
                nodes = [n for n in self.ax('find', '--title', 'Open', '--role', 'AXButton', '--in', 'dialog')
                         if 'x' in n and not n.get('meta')]
                if nodes:
                    enabled = nodes[0].get('enabled')
                    state = 'enabled=%s' % enabled
                    if str(enabled).lower() in ('true', '1'):
                        return
                else:
                    state = 'not found'
            except StepFailed as exc:
                if not axwait.not_yet(exc, 'find'):
                    raise
                state = 'no answer' if axwait.no_answer(exc) else 'not found'
            if time.monotonic() >= end:
                raise StepFailed('connect folder: the chooser\'s Open button was not enabled within %s s (%s)'
                                 % (seconds, state))
            time.sleep(1)

    def connect_folder(self, path=None):
        """Settings -> "Connected folders (repositories)" -> the company folder (or `path`, a
        further folder: then the wait is for that folder itself, by its real path), the way the app
        asks for it since round 27: the row is a plain button (settings-button.js
        buildDisclosureRow: "no `menuitem` role"), so pressing it as an AXMenuItem waits out the
        guest's 20 s accessibility deadline (exit 124, guest_deadline). The sheet pre-selects the
        only company and its folder field opens the system folder chooser on a click, so a typed
        path never reaches it: Go to folder, the path, Return, Open, then Connect folder. Waits up
        to 30 s for the registry to name the folder, closes the sheet, returns the connected paths."""
        self.open_connected_folders()
        fields = [n for n in self.ax('find', '--title', 'Project folder location', '--role', 'AXTextField',
                                     '--contains') if 'x' in n and not n.get('meta')]
        if not fields:
            raise StepFailed('no "Project folder location" field on the Connected folders sheet')
        f = fields[0]
        command([HERE / 'ax.sh', self.vm, 'click', '--at', '%d,%d' % (f['x'] + f['w'] / 2, f['y'] + f['h'] / 2)], 90)
        if not self.wait_chooser(True):
            self.ax('click', '--title', 'Project folder location', '--role', 'AXTextField', '--contains', '--first')
            if not self.wait_chooser(True):
                raise StepFailed('the folder field did not open the folder chooser')
        self.keys('tell application "System Events" to keystroke "g" using {command down, shift down}')
        time.sleep(2)
        self.keys('tell application "System Events" to keystroke ' + json.dumps(path or self.company))
        time.sleep(1)
        self.keys('tell application "System Events" to key code 36')
        self.wait_open_enabled()
        self.ax('click', '--title', 'Open', '--role', 'AXButton', '--in', 'dialog', '--first')
        if not self.wait_chooser(False):
            raise StepFailed('the folder chooser did not close after Open')
        time.sleep(1)
        self.ax('click', '--title', 'Connect folder', '--role', 'AXButton', '--in', 'dialog', '--first')
        registry = self.data + '/entities.json'
        real = path and guest(self.vm, 'python3 -c "import os,sys; print(os.path.realpath(sys.argv[1]))" '
                              + shlex.quote(path))
        end, value = time.monotonic() + 30, {}
        while time.monotonic() < end:
            value = json.loads(guest(self.vm, 'cat ' + shlex.quote(registry)))
            connected = [r for e in value.get('entities', []) for r in e.get('connected_repositories', [])]
            if (real in connected) if path else connected:
                self.press('Close')
                return connected
            time.sleep(1)
        raise StepFailed('the repository was not connected within 30 s: ' + json.dumps(value)[:400])

    def decline_first_run_sheets(self, tries=3, seconds=45):
        """Press Not now on each sheet a first run puts up before the company question (main.js,
        WHAT A FIRST RUN ASKS: the setup sheet, then the memory question), one at a time, until
        the company question is up. Since the setup essentials (e0a346d76) a guest without the
        video tools gets the setup sheet too, so one press left the memory question over the
        window (walk of candidate 43: "Add this company" never appeared). A sheet that is not up
        yet is waited for, `seconds` in all. Returns the presses."""
        presses = 0
        end = time.monotonic() + seconds
        while presses < tries and time.monotonic() < end:
            if self.present('Add this company'):
                break
            if self.present('Not now'):
                self.press('Not now')
                presses += 1
                time.sleep(3)
            else:
                time.sleep(1)
        return presses

    def first_run(self):
        self.decline_first_run_sheets()
        # Idempotent, so --steps can re-run it against a guest that already has the repository.
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        self.wait_for('Add this company')
        self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        if self.present('Start the questions'):
            self.press('Not now')
        end = time.monotonic() + 60
        while time.monotonic() < end:
            rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true')
            created = [json.loads(r) for r in rows.splitlines() if '"ThreadCreated"' in r]
            if created:
                self.facts['thread'] = created[0]['thread_id']
                self.facts['thread_title'] = created[0].get('title')
                self.save()
                return {'thread': self.facts['thread'], 'title': self.facts['thread_title']}
            time.sleep(1)
        raise StepFailed('no conversation was created after the company was added')

    def watch(self):
        remote = self.payload + '/adopt-watch.py'
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'adopt-watch.py', remote], 60)
        guest(self.vm, 'nohup python3 {s} {d} {o} --seconds 3000 >/dev/null 2>&1 &'.format(
            s=shlex.quote(remote), d=shlex.quote(self.data), o=shlex.quote(self.watch_out)))
        time.sleep(2)
        started = guest(self.vm, 'head -1 ' + shlex.quote(self.watch_out))
        if 'watch-start' not in started:
            raise StepFailed('the watcher did not start in the guest')
        return {'watching': self.data}

    def pair(self):
        self.press('Settings', role='AXPopUpButton')
        self.press('Use Rich from your phone', role='AXMenuItem')
        # The route chooser shows while RichOS Connect is offered; this walk is the Tailscale path.
        # Its two choices are toggles (AXCheckBox), not buttons.
        if self.present('Use Tailscale', role='AXCheckBox'):
            self.ax('click', '--title', 'Use Tailscale', '--role', 'AXCheckBox', '--in', 'dialog', '--first')
        self.wait_for('Set my phone up')
        self.press('Set my phone up')
        url = None
        end = time.monotonic() + 30
        while time.monotonic() < end and not url:
            try:
                text = command([HERE / 'ax.sh', self.vm, 'tree', '--in', 'dialog', '--max', '150'], 60)
            except StepFailed as exc:
                if not axwait.ask_again(exc, 'tree'):  # a read with no answer: not drawn yet
                    raise
                text = ''
            m = re.search(r'https://[^\s\'"<>]+#pair=[A-Z0-9]+', text)
            url = m.group(0) if m else None
            if not url:
                time.sleep(1)
        if not url:
            raise StepFailed('no pairing address was drawn within 30 s')
        guest(self.vm, shlex.join(['open', '-a', 'Safari', url]))
        self.wait_for('They match', app='Safari', seconds=40)
        self.press('They match', app='Safari')
        # Sage F1: the Mac, not the phone, activates a paired device — its own "They match".
        end = time.monotonic() + 40
        while True:
            try:
                self.ax('click', '--title', 'They match', '--role', 'AXButton', '--in', 'dialog', '--first')
                break
            except StepFailed:
                if time.monotonic() >= end:
                    raise StepFailed('the Mac never asked for its own "They match"')
                time.sleep(1)
        self.wait_for('Write to Rich', role='AXTextArea', app='Safari', seconds=40)
        if self.present('Not now', app='Safari'):
            self.press('Not now', app='Safari')
        return {'paired': True}

    def phone_task(self):
        before = self.clock()
        self.type_into(self.a.task, '--role', 'AXTextArea', '--title', 'Write to Rich', app='Safari')
        self.press('Send', app='Safari')
        after = self.clock()
        self.facts['sent_ms'] = [before, after]
        self.save()
        return {'sent_between_guest_ms': [round(before), round(after)]}

    def watched(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.watch_out), 60)
        return [json.loads(r) for r in rows.splitlines() if r.startswith('{')]

    def observe(self):
        thread = self.facts.get('thread')
        sent = self.facts.get('sent_ms')
        if not thread or not sent:
            raise StepFailed('first-run and phone-task must have run (no thread or no send time on record)')
        end = time.monotonic() + self.a.within
        while True:
            rows = self.watched()
            moved = [r for r in rows if r['kind'] == 'assignment' and r.get('thread') == thread
                     and r.get('state') != 'registered']
            if moved or time.monotonic() >= end:
                break
            time.sleep(2)
        (self.out / 'adopt-watch.jsonl').write_text(''.join(json.dumps(r, sort_keys=True) + '\n' for r in rows))
        evidence, failure = analyze(rows, thread, sent)
        (self.out / 'observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if failure:
            raise StepFailed(failure)
        return evidence


def analyze(rows, thread, sent):
    """The verdict, from adopt-watch.py's rows alone: (evidence, None) or (evidence, why it failed).

    Separate from the guest so the rules can be tested on rows (test/adopt-walk.test.py)."""
    intake = {r['id']: r for r in rows if r['kind'] == 'intake' and r.get('id') is not None}
    prompts = []
    for r in rows:
        if (r['kind'] == 'ledger' and r.get('event') == 'PromptReceived' and r.get('thread') == thread
                and r.get('source') != 'internal'):
            record = intake.get(r.get('intake_id'))
            if r.get('intake_id') is None:
                road = 'typed, straight to the spine'
            elif record and record.get('record') == 'channel':
                road = 'phone'
            elif record and record.get('record') == 'desk':
                road = 'typed, through the intake log'
            else:
                road = 'unknown'
            prompts.append({'turn': r.get('turn'), 'at': r.get('at'), 't_ms': r['t_ms'], 'road': road,
                            'channel': record.get('channel') if record else None})
    states = {}
    for r in rows:
        if r['kind'] == 'assignment' and r.get('thread') == thread:
            states.setdefault(r['id'], []).append({
                'state': r['state'], 't_ms': r['t_ms'], 'registered_at_ms': r.get('registered_at_ms'),
                'updated_at_ms': r.get('updated_at_ms'), 'detail': r.get('detail')})
    # `TurnCompleted` carries a turn id and no thread, so it is matched through the prompts.
    ours = {p['turn'] for p in prompts}
    completed = {r.get('turn'): r.get('at') for r in rows
                 if r['kind'] == 'ledger' and r.get('event') == 'TurnCompleted' and r.get('turn') in ours}
    for p in prompts:
        p['completed_at'] = completed.get(p['turn'])
    evidence = {'thread': thread, 'sent_between_guest_ms': sent, 'prompts': prompts, 'assignments': states}
    if not states:
        return evidence, 'no assignment was registered on the conversation'
    moved = [row for rows_ in states.values() for row in rows_ if row['state'] != 'registered']
    if not moved:
        return evidence, 'the assignment was registered and never left Registered: ' + json.dumps(states)
    typed = [p for p in prompts if p['road'] != 'phone']
    if typed:
        return evidence, 'a prompt on this conversation did not come from the phone: ' + json.dumps(typed)
    phone = [p for p in prompts if p['road'] == 'phone' and p['t_ms'] >= sent[0]]
    if not phone:
        return evidence, 'no prompt from the phone was seen after the send'
    # AT the boundary, never inside the turn (background-work spec §7.1): the first state after
    # Registered is written no earlier than the phone turn's own completion.
    left = min(row['updated_at_ms'] for row in moved if row.get('updated_at_ms'))
    ended = phone[0].get('completed_at')
    if ended is None:
        return evidence, 'the phone turn has no TurnCompleted on the ledger'
    if left < ended:
        return evidence, f'the assignment left Registered at {left}, before its turn ended at {ended}'
    evidence['left_registered_ms_after_turn_end'] = left - ended
    return evidence, None


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=TASK)
    p.add_argument('--within', type=float, default=120, help='seconds for the assignment to leave Registered')
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
