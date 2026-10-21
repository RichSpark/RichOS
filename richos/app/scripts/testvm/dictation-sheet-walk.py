#!/usr/bin/env python3
"""dictation-sheet-walk.py — the Dictation sheet from no grants to a dictation in Try it here (dictation plan slice 2).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      dictation-sheet-walk.py --out DIR --expect-sha SHA --wav SPOKEN.wav --model GGML.bin [--model-id small.en]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION. Slice 2 of richos-hq docs/plans/2026-10-08-dictation-anywhere.md (revision 2,
section 9): "VM walk from no grants: turn the switch on; the microphone prompt appears and OCR
(scripts/qa/ocr-find.sh) reads Iris's sentence in it; Allow is pressed through the accessibility
tree; the Accessibility prompt appears; the guest grant is written; within 2 s the sheet says On
with no relaunch, and Try it here receives a dictation. Then the refused path: the sheet shows the
drawn denied line and Open System Settings opens the Accessibility pane. Then a second copy with
dictation on shows 'On even when RichOS is closed'. If the guest's prompt cannot be clicked through
the accessibility tree, the walk proves the prompt by OCR, grants the row, and says so in its
report." Measured against the CEO's words: "yes, also keep the sentence Iris wrote for when macOS
shows when RichOS asks for the microphone" and his "On even when RichOS is closed" (2026-10-08).

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). The app is started with
`open -n -a` (relaunch.py) and the tool is the app's child, never a binary run from ssh (minor 8).
No sound is played anywhere (CEO §53): the spoken sample is a file that RICHOS_VOICE_INPUT_WAV puts
through the identical capture path. The ways in are shown through RICHOS_DICTATION_PREVIEW=1, the
walks' preview, because DICTATION_READY stays false until slice 5.
  identity      the running app says it was built from --expect-sha
  stage         every TCC row for com.richos.app removed (no grants at all); the sample and the
                speech model copied in
  relaunch      the app relaunched on that guest with the preview and the sample as its microphone
  switch-on     Settings, Dictation, the switch pressed
  mic-prompt    macOS's microphone prompt is on screen, OCR reads Iris's sentence in it, and its
                Allow is pressed through the accessibility tree (or, when no process in the tree
                offers it, the Microphone row is written and the report says so)
  ax-prompt     macOS's Accessibility prompt is on screen (OCR)
  granted       the guest's Accessibility row is written; the tool makes its key tap and the sheet
                says On, with the app's pid unchanged (no relaunch); the time from the write to the
                tool's line is measured (UNPROVEN in run 22: a row written behind tccd's back
                never reaches a running process)
  granted-settings THE WAY A PERSON DOES IT (slice 5's brief): the Accessibility row removed, the
                app relaunched and the switch pressed to the Accessibility prompt; then System
                Settings, Privacy & Security, Accessibility is driven through the accessibility
                tree: RichOS's switch turned on, the guest's admin password entered where macOS
                asks; the running app's sheet must say On with no relaunch (the app's pid
                unchanged), and the tool must make its key tap
  try-it        one dictation of the sample into Try it here; its text is the guest's own decode
  refused       the Accessibility row removed, the app relaunched: the sheet says the drawn denied
                line with Open System Settings beside it
  open-settings Open System Settings brings System Settings to the front on Accessibility
  second-copy   the Accessibility row back; a second copy of the bundle, on its own home with
                dictation on: its tool finds the key taken (exit 3), and its Settings row and sheet
                say "On even when RichOS is closed"; that copy is then quit by its pid

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54); the
second copy is quit here, by its own pid, before the step ends.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
QA = HERE.parent / 'qa'
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('dictation_walk', HERE / 'dictation-walk.py')
dictation_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(dictation_walk)
StepFailed = dictation_walk.StepFailed


class StepUnproven(Exception):
    """A step whose claim the guest cannot settle: its outcome is UNPROVEN, with the reason and
    the evidence gathered, and the walk goes on to the steps after it."""

    def __init__(self, why, evidence):
        super().__init__(why)
        self.evidence = evidence
command = dictation_walk.command

STEPS = ['identity', 'stage', 'relaunch', 'switch-on', 'mic-prompt', 'ax-prompt', 'granted', 'granted-settings', 'try-it',
         'refused', 'open-settings', 'second-copy']
# System Settings, Privacy & Security, Accessibility: the pane the sheet's Open System Settings
# opens, and the one a person turns RichOS on in (granted-settings).
AX_PANE = 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility'
SYSTEM_SETTINGS = 'System Settings'
# The guest's admin password, which macOS asks for before a privacy switch is changed (lib.sh's
# TESTVM_GUEST_PASS; the image's stock account). A disposable clone's, never a host's.
GUEST_PASS = os.environ.get('TESTVM_GUEST_PASS', 'admin')

# macOS's own words in its two prompts, and Iris's sentence (Info.plist), as OCR fragments that sit on
# one line of a prompt: a wrapped sentence is never one OCR line (setup-walk.py, walk-d33d482c9645).
# Measured on walk-63fc2c726051 (mic-prompt-6.png): the prompt wraps after "would like to access"
# and after "press the talk", and OCR reads "F1" in its small type as "Ft", so no fragment holds the key.
# The sentence now names no key (product hunt 2026-10-15, finding 20): the old one wrapped after
# "I listen", so the second fragment is the new sentence's start of that line. Unverified on a
# walk until the next one reads it.
# walk-8f45d498831b (ax-prompt-failed.png): OCR joins the sheet's line under the prompt with its
# title and drops the "w" ("Dictate in any app ould like to control this computer using"), so
# the titles are matched from their middle.
MIC_PROMPT = 'like to access'
IRIS_FRAGMENTS = ('talk instead of typing', 'only when you ask')
AX_PROMPT = 'control this computer'
# The sheet's drawn words (round 19 and its more lines), read from the accessibility tree. A line
# with the key's cap in it is read by the text run after the cap.
ON_RUN = 'in any app, talk and tap it again.'
# Round 19 state 10's line under the card, drawn by the turn-on once the tool holds the key (D18,
# the candidate 46 walk: it never showed, because the window decided it before the tool's tap).
FEEDBACK_LINE = 'Dictation is on. Try it in the box on the right or in any app.'
DENIED_LINE = 'Not working yet: macOS has not let me type into other apps.'
# The second copy's row and sheet say round 19's On line, through the first copy's tool (the CEO,
# 2026-10-08: dictation runs only while RichOS runs, so "On even when RichOS is closed" is drawn
# nowhere); the sheet's line is the row's line completed with ON_RUN.
OTHER_ROW = 'On. Tap F1 to talk'
OTHER_SHEET = 'On. Tap F1 in any app, talk and tap it again.'
# The second copy's executable, renamed so its process is told apart from the first's.
SECOND_EXE = 'richos-second'
# The processes macOS has put its privacy prompts in, across releases; the first that answers wins.
PROMPT_PROCESSES = ('UserNotificationCenter', 'universalAccessAuthWarn', 'CoreServicesUIAgent', 'tccd')
# The processes that can host the "Privacy & Security is trying to modify your system settings"
# password sheet beside System Settings itself (walk-15d68ef48fc5).
PASSWORD_PROCESSES = ('SecurityAgent', 'UserNotificationCenter', 'CoreServicesUIAgent', 'loginwindow')
USER_DB = '"$HOME/Library/Application Support/com.apple.TCC/TCC.db"'
SYS_DB = '"/Library/Application Support/com.apple.TCC/TCC.db"'
# tccd writes these databases too; a read or write that meets its lock waits up to 10 s instead
# of failing at once (walk-2d04da6fc5a4, stage: "database is locked (5)").
SQLITE = "sqlite3 -cmd '.timeout 10000'"
GRANT = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, "
         "indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) "
         "VALUES ('{svc}', 'com.richos.app', 0, 2, 2, 1, 0, 'UNUSED', 0, strftime('%s','now'));")
GRANTED_WITHIN = 2.0


def center_of(node):
    """A node's center from ax.sh's x, y, w, h."""
    x, y = float(node.get('x') or 0), float(node.get('y') or 0)
    return x + float(node.get('w') or 0) / 2, y + float(node.get('h') or 0) / 2


# Half a row of System Settings' app list, in points: a switch on the same row as a name sits
# within this of the name's vertical center.
HALF_ROW = 20.0


def names_richos(node):
    """A static text that says RichOS: by its value (a static text's string) or its title."""
    return 'richos' in (str(node.get('value') or '') + ' ' + str(node.get('title') or '')).lower()


def the_only_switch_off(boxes):
    """The one checkbox whose value is off when every other is on; nothing otherwise."""
    off = [b for b in boxes if str(b.get('value') or '0') in ('0', 'false', 'False', '')]
    return off if len(off) == 1 and len(boxes) > 1 else []


def switch_on_the_row(names, boxes):
    """The checkbox on the same row as one of `names` (static texts): nearest vertical centers,
    within HALF_ROW. The switches in the Accessibility list carry no title (walk-943b91d5720b)."""
    out = []
    for name in names:
        _, ny = center_of(name)
        near = [(abs(center_of(b)[1] - ny), b) for b in boxes]
        near = [(d, b) for d, b in near if d <= HALF_ROW]
        if near:
            out.append(min(near, key=lambda p: p[0])[1])
    return out


def grant_seconds_ok(seconds):
    """The plan's "within 2 s", read as the walk measures it: from the row's write to the tool's
    key-tap line, each read one ssh round trip, so a reading is an upper bound."""
    return seconds is not None and seconds <= GRANTED_WITHIN + 1.0


class SheetWalk(dictation_walk.DictationWalk):
    def env(self):
        return {'RICHOS_VOICE_INPUT_WAV': self.wav, 'RICHOS_DICTATION_PREVIEW': '1'}

    # --- the guest's screen -----------------------------------------------------------------
    def by_id(self, dom_id):
        """None while the node is not there yet. `nowindow` is one such answer: a relaunched app
        whose process is up and whose window is not (walk-9b00963b522b, 13.9 s after the launch,
        the frame blank, the app fetching its voice tools); the launch loop waits 180 s for that,
        and this read must not end it early."""
        try:
            nodes = self.ax('find', '--id', dom_id, '--first')
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find') or 'nowindow' in str(exc):
                return None
            raise
        nodes = [n for n in nodes if not n.get('meta')]
        return nodes[0] if nodes else None

    def shows_text(self, text):
        try:
            return bool(self.ax('find', '--value', text, '--contains', '--first'))
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return False
            raise

    # present() is adopt-walk's, whose find with no answer is "not yet" (axwait.py): a relaunched
    # app still building its tree answers nothing in 20 s (walk-918256bab6a4, this walk's first run).

    def until(self, check, seconds, what):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if check():
                return True
            time.sleep(1)
        raise StepFailed(f'{what} within {seconds} s')

    def frame_says(self, name, text):
        r = subprocess.run([str(QA / 'ocr-find.sh'), text, str(self.out / name), '--first', '--quiet'],
                           capture_output=True, text=True, timeout=120)
        if r.returncode not in (0, 1):
            raise StepFailed('ocr-find could not read the frame: ' + r.stderr)
        return r.returncode == 0

    def screen_says(self, name, text, seconds):
        """Frames of the guest's screen until OCR reads `text` in one (at most `seconds`)."""
        end, n = time.monotonic() + seconds, 0
        while time.monotonic() < end:
            frame = f'{name}-{n}.png'
            self.shot(frame)
            if self.frame_says(frame, text):
                return frame
            n += 1
            time.sleep(1)
        raise StepFailed(f'OCR did not read "{text}" on the guest screen within {seconds} s (frames {name}-*.png)')

    def click(self, title, role='AXButton', app=None):
        """A button by its title: adopt-walk's press(). DictationWalk's press(key) shadows it (it
        posts the dictation key), so self.press('Not now') failed with "invalid literal for
        int()" (walks 490c9ae819b3 and 8e2679058844); a button goes through here."""
        return dictation_walk.adopt_walk.Walk.press(self, title, role, app)

    def decline_first_run_sheets(self, tries=3, seconds=45):
        """adopt-walk's, pressing Not now through click(), and the company question too: this
        walk needs no company, and that question over the window blocked the Settings button
        (walk-8c18b701c0d8, granted: "the Dictation row did not appear"). Each
        first-run sheet is declined, one at a time, `seconds` in all. Returns the presses."""
        presses = 0
        end = time.monotonic() + seconds
        while presses < tries and time.monotonic() < end:
            if self.present('Not now'):
                self.click('Not now')
                presses += 1
                time.sleep(3)
            else:
                time.sleep(1)
        return presses

    def declined_late_sheet(self):
        """A first-run sheet a slow launch puts up late hides the window's tree from ax.sh
        ("blocked ... modal=Where should I keep what you tell me?": walk-513e77dc426d after
        decline_first_run_sheets' 30 s, walk-e0f1629e8f0b's refused relaunch). Declines one
        that is up; True when it did."""
        if self.present('Not now'):
            self.click('Not now')
            time.sleep(3)
            return True
        return False

    def open_settings_panel(self):
        """The Settings button, until its Dictation row is drawn; a late first-run sheet is
        declined and the opening starts over, at most three times."""
        for attempt in range(3):
            try:
                self.ax('click', '--id', 'set-btn', '--first')
                self.until(lambda: self.by_id('set-dictation-open') is not None, 30, 'the Dictation row did not appear')
                return
            except StepFailed as exc:
                if attempt == 2 or not self.declined_late_sheet():
                    raise

    def open_sheet(self):
        """Settings, then its Dictation row, with open_settings_panel's tolerance for a late sheet."""
        for attempt in range(3):
            try:
                self.open_settings_panel()
                self.ax('click', '--id', 'set-dictation-open', '--first')
                self.until(lambda: self.by_id('dict-switch') is not None, 30, 'the Dictation sheet did not open')
                return
            except StepFailed as exc:
                if attempt == 2 or not self.declined_late_sheet():
                    raise

    def bring_front(self, pid):
        guest(self.vm, 'osascript -e ' + shlex.quote(
            f'tell application "System Events" to set frontmost of (first process whose unix id is {int(pid)}) to true'))
        time.sleep(1)

    def press_in_prompt(self, title):
        """Press `title` in macOS's prompt through the accessibility tree. The process that owns
        the prompt, or None when no process offered the button."""
        for process in PROMPT_PROCESSES:
            try:
                self.ax('click', '--title', title, '--role', 'AXButton', '--first', app=process)
                return process
            except StepFailed:
                continue
        return None

    def tcc_rows(self):
        # Each row with its auth_value (0 denied, 2 allowed): macOS itself records an app it has
        # checked as a denied row, which is no grant (walk of ee92df2c4, stage).
        query = "SELECT service || '=' || auth_value FROM access WHERE client='com.richos.app' ORDER BY service;"
        return {'user': guest(self.vm, f'{SQLITE} {USER_DB} ' + shlex.quote(query)).splitlines(),
                'system': guest(self.vm, f'sudo -n {SQLITE} {SYS_DB} ' + shlex.quote(query)).splitlines()}

    def grant(self, service, system):
        db = SYS_DB if system else USER_DB
        guest(self.vm, ('sudo -n ' if system else '') + f'{SQLITE} {db} ' + shlex.quote(GRANT.format(svc=service)))

    def revoke(self, service, system):
        db = SYS_DB if system else USER_DB
        sql = f"DELETE FROM access WHERE client='com.richos.app' AND service='{service}';"
        guest(self.vm, ('sudo -n ' if system else '') + f'{SQLITE} {db} ' + shlex.quote(sql))

    def post_ax_notice(self):
        """What a change through tccd announces and a row written with sqlite3 does not: the
        Darwin notice "com.apple.tcc.access.changed" (the name sits in the guest's system
        libraries beside libTCC's answer cache) and the distributed notice
        "com.apple.accessibility.api" that System Settings posts after an Accessibility switch.
        Both are posted; neither alone moved a running RichOS in walks 46bb2eac452a to
        8b3ec04cad73 (the distributed one only). The distributed one goes out in the guest's GUI
        session, as the user."""
        darwin = guest(self.vm, 'notifyutil -p com.apple.tcc.access.changed 2>&1 && echo posted || echo refused')
        script = ("ObjC.import('Foundation'); $.NSDistributedNotificationCenter.defaultCenter"
                  ".postNotificationNameObjectUserInfoDeliverImmediately('com.apple.accessibility.api', null, null, true);"
                  " 'posted'")
        distributed = guest(self.vm, 'sudo -n launchctl asuser "$(id -u)" sudo -n -u "$(id -un)" osascript -l JavaScript -e '
                            + shlex.quote(script) + ' 2>&1 || echo refused')
        return {'com.apple.tcc.access.changed': darwin, 'com.apple.accessibility.api': distributed}

    def tell_tool(self, kind):
        """One line to the tool's socket, as an app connection says it (ipc.rs AppMessage)."""
        code = ("import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); "
                "s.sendall(('{\"type\":\"%s\"}\\n' % sys.argv[2]).encode()); time.sleep(0.5); s.close(); print('told')")
        return guest(self.vm, 'python3 -c ' + shlex.quote(code) + ' "/private/tmp/richos-$(id -u)/dictation.sock" '
                     + shlex.quote(kind) + ' 2>&1 || echo refused')

    def save_dlog(self):
        (self.out / 'dictation.log').write_text('\n'.join(self.dlog_lines()) + '\n')

    def app_log(self):
        return guest(self.vm, 'cat ' + shlex.quote(self.log) + ' 2>/dev/null || true', 30)

    def dlog_lines(self):
        return guest(self.vm, 'cat ' + shlex.quote(self.dlog) + ' 2>/dev/null || true', 30).splitlines()

    def launch(self):
        launched = relaunch(self.vm, environment=self.env())
        self.log = launched['log']
        self.facts.update({'log': self.log, 'app_pid': launched['pid']})
        self.save()
        # The window's tree first: the Settings button is on every screen; a first-run sheet up
        # meanwhile hides it, and is declined.
        declined = []

        def drawn():
            if self.by_id('set-btn') is not None:
                return True
            if self.declined_late_sheet():
                declined.append('Not now')
            return False

        self.until(drawn, 180, 'the relaunched app drew no Settings button')
        presses = self.decline_first_run_sheets(seconds=30) + len(declined)
        return launched, presses

    # --- steps --------------------------------------------------------------------------------
    def stage(self):
        for service, system in (('kTCCServiceMicrophone', False), ('kTCCServiceAccessibility', True),
                                ('kTCCServiceListenEvent', True), ('kTCCServicePostEvent', True)):
            self.revoke(service, system)
        rows = self.tcc_rows()
        allowed = [r for r in rows['user'] + rows['system'] if r.endswith('=2')]
        if allowed:
            raise StepFailed(f'com.richos.app is still allowed something: {rows}')
        guest(self.vm, 'rm -f ' + shlex.quote(self.data + '/dictation.json'))
        command([HERE / 'guest.sh', self.vm, '--push', self.a.wav, self.wav], 120)
        models = self.home + '/.config/richos/models'
        guest(self.vm, 'mkdir -p ' + shlex.quote(models))
        target = f'{models}/ggml-{self.a.model_id}.bin'
        command([HERE / 'guest.sh', self.vm, '--push', self.a.model, target], 900)
        return {'tcc': rows, 'no_grants': True, 'dictation_json': 'absent', 'wav': self.wav, 'model': target}

    def relaunch(self):
        launched, presses = self.launch()
        return {'app_pid': launched['pid'], 'first_run_sheets_declined': presses}

    def switch_on(self):
        self.open_sheet()
        self.shot('sheet-off.png')
        off = self.shows_text('Off. Turn it on to type with your voice')
        self.ax('click', '--id', 'dict-switch', '--first')
        return {'sheet_said_off_first': off}

    def mic_prompt(self):
        frame = self.screen_says('mic-prompt', MIC_PROMPT, 30)
        iris = {fragment: self.frame_says(frame, fragment) for fragment in IRIS_FRAGMENTS}
        if not all(iris.values()):
            raise StepFailed(f'OCR did not read Iris\'s sentence in the microphone prompt ({frame}): {iris}')
        pressed_in = self.press_in_prompt('Allow')
        fallback = None
        if pressed_in is None:
            # Minor 8's fallback, said in the report: no process offered Allow, so the row is written.
            self.grant('kTCCServiceMicrophone', False)
            fallback = 'no process in the accessibility tree offered Allow; the Microphone row was written instead'
        self.facts['mic_frame'] = frame
        self.save()
        return {'frame': frame, 'iris_sentence_ocr': iris, 'allow_pressed_in': pressed_in, 'fallback': fallback}

    def ax_prompt(self):
        frame = self.screen_says('ax-prompt', AX_PROMPT, 30)
        mic_line = [x for x in self.app_log().splitlines() if 'dictation: the microphone was' in x]
        return {'frame': frame, 'mic_answer_in_log': mic_line[-1:] or None, 'tcc': self.tcc_rows()}

    def granted(self):
        """Accessibility allowed while the app runs: the tool's key tap and the sheet's On, with
        no relaunch, within 2 s. The walk's grant is a row written into the guest's TCC.db; a
        person's switch in System Settings goes through tccd, which tells running processes.
        A running process does not read a row written behind tccd's back (walks 368bb9a01972,
        46bb2eac452a, b257f16e0001, 72c807eaccab: row allowed, tccd restarted, System Settings'
        notice posted, the tool told directly, and the running tool still read no; tccd logged no
        Accessibility question from the app for 30 s, so the answer comes from inside the
        process). So: the live answer is tried first and timed (the row; then tccutil's reset,
        which tccd announces, with the row straight after); if it does not come, the step
        says UNPROVEN and why, relaunches with the grant in place, and proves the sheet's On
        and the key tap from there, so the steps after it still run."""
        pid_before = self.facts['app_pid']
        taps_before = sum('key tap created for F1' in x for x in self.dlog_lines())
        self.grant('kTCCServiceAccessibility', True)
        noticed = self.post_ax_notice()
        written = time.monotonic()
        row_after_write = self.tcc_rows()['system']

        def tap_within(since, seconds):
            while time.monotonic() - since < seconds:
                if sum('key tap created for F1' in x for x in self.dlog_lines()) > taps_before:
                    return round(time.monotonic() - since, 2)
                time.sleep(0.2)
            return None

        seen, clock_from = tap_within(written, 5), 'row write'
        if seen is None:
            # A change tccd itself announces: tccutil's reset goes through tccd, which publishes it
            # to running processes as System Settings' switch does, and the allowed row is written
            # straight after it, so the process's next question reads it.
            reset_sql = shlex.quote(GRANT.format(svc='kTCCServiceAccessibility'))
            guest(self.vm, f'sudo -n tccutil reset Accessibility com.richos.app >/dev/null 2>&1; sudo -n {SQLITE} {SYS_DB} {reset_sql}')
            noticed = [noticed, self.post_ax_notice()]
            row_after_write = self.tcc_rows()['system']
            seen, clock_from = tap_within(time.monotonic(), 5), 'tccutil reset, then the row'
        if seen is not None:
            on = self.until(lambda: self.shows_text(ON_RUN), 15, 'the sheet did not say On')
            line = self.shows_text(FEEDBACK_LINE)
            dismissed_in = self.press_in_prompt('Deny')  # the prompt macOS left up
            self.grant('kTCCServiceAccessibility', True)  # whatever the prompt's button wrote
            pid_now = guest(self.vm, f'kill -0 {int(pid_before)} 2>/dev/null && echo alive || true')
            if pid_now != 'alive':
                raise StepFailed(f'the app (pid {pid_before}) is gone: the sheet must say On with no relaunch')
            self.shot('sheet-on.png')
            if not grant_seconds_ok(seen):
                raise StepFailed(f'the key tap came {seen} s after the Accessibility row, not within {GRANTED_WITHIN} s')
            if not line:
                raise StepFailed(f'the sheet says On but not round 19\'s line under the card: {FEEDBACK_LINE!r} (D18)')
            return {'key_tap_after_row_seconds': seen, 'clock_from': clock_from, 'sheet_says_on': on, 'sheet_says_feedback': line, 'same_app_pid': pid_before,
                    'notice': noticed, 'row_after_write': row_after_write, 'prompt_dismissed_in': dismissed_in,
                    'tcc': self.tcc_rows()}

        # Not seen live. Evidence first, then the relaunch.
        self.shot('granted-unseen.png')
        lines_before = len(self.dlog_lines())
        told = self.tell_tool('permissions-changed')
        time.sleep(3)
        probe = self.dlog_lines()[lines_before:]
        tcc_log = guest(self.vm, "/usr/bin/log show --last 2m --style compact --predicate "
                                 + shlex.quote('subsystem == "com.apple.TCC" AND eventMessage CONTAINS[c] "richos"')
                                 + " 2>&1 | tail -300 || true", 120)
        (self.out / 'tcc-answers.log').write_text(tcc_log)
        dismissed_in = self.press_in_prompt('Deny')  # the prompt macOS left up
        self.grant('kTCCServiceAccessibility', True)  # whatever the prompt's button wrote
        launched, presses = self.launch()
        self.open_sheet()
        relaunched_tap = tap_within(time.monotonic(), 60)
        if relaunched_tap is None:
            raise StepFailed('with the grant in place and a relaunch, the tool still made no key tap')
        on = self.until(lambda: self.shows_text(ON_RUN), 30, 'after the relaunch the sheet did not say On')
        self.shot('sheet-on-after-relaunch.png')
        raise StepUnproven(
            'the sheet\'s On with no relaunch is not proven in the guest: the walk\'s grant (a row written into '
            'TCC.db) never reaches a running RichOS; after a relaunch the tool made its key tap and the sheet says On',
            {'live_wait_seconds': 5, 'row_after_write': row_after_write, 'notice': noticed, 'told_tool': told,
             'tool_answered': probe, 'prompt_dismissed_in': dismissed_in, 'relaunched_pid': launched['pid'],
             'first_run_sheets_declined': presses, 'sheet_says_on_after_relaunch': on,
             'tcc': self.tcc_rows()})

    # --- System Settings, driven as a person drives it ------------------------------------------
    def settings_find(self, role, title=None):
        """System Settings' nodes of one role (and a title fragment), by ax.sh find: a role search
        answers in a second where a whole-tree read of System Settings runs past ax.sh's 20 s
        deadline (walk-b69c9d706854)."""
        args = ['--role', role]
        if title:
            args += ['--title', title, '--contains']
        try:
            return [n for n in self.ax('find', *args, app=SYSTEM_SETTINGS, timeout=60) if not n.get('meta')]
        except StepFailed as exc:
            if axwait.not_yet(exc, 'find'):
                return []
            raise

    def password_sheet_process(self):
        """The process whose tree holds the password sheet's secure field, or None. The sheet
        "Privacy & Security is trying to modify your system settings" is not in System Settings'
        own tree (walk-15d68ef48fc5: the press landed, the sheet was up, and a search of System
        Settings saw no secure field), so every process that can host it is asked."""
        # A secure field's role is AXTextField with the subrole AXSecureTextField (walk-8dcad10d445b's
        # sheet run: the sheet was up, and a search by the role AXSecureTextField found nothing in
        # any process); the subrole is what names it.
        for process in (SYSTEM_SETTINGS,) + PASSWORD_PROCESSES:
            try:
                nodes = [n for n in self.ax('find', '--subrole', 'AXSecureTextField', app=process, timeout=30) if not n.get('meta')]
            except StepFailed:
                continue
            if nodes:
                return process
        return None

    def password_sheet_up(self):
        return self.password_sheet_process() is not None

    def enter_password(self):
        """The admin password into the secure field of the sheet macOS put up, then its button.
        Typed with System Events (sshd-session's own grant), to the process that holds the sheet."""
        process = self.password_sheet_process() or SYSTEM_SETTINGS
        self.bring_front_app(process)
        try:
            self.ax('click', '--subrole', 'AXSecureTextField', '--first', app=process)
        except StepFailed:
            pass
        self.osa(f'tell application "System Events" to keystroke "{GUEST_PASS}"')
        time.sleep(0.5)
        for title in ('Modify Settings', 'Unlock', 'OK'):
            try:
                self.ax('click', '--title', title, '--role', 'AXButton', '--first', app=process)
                return f'{title} in {process}'
            except StepFailed:
                continue
        self.osa('tell application "System Events" to key code 36')  # return
        return f'return in {process}'

    def bring_front_app(self, name):
        guest(self.vm, 'osascript -e ' + shlex.quote(f'tell application "System Events" to set frontmost of process "{name}" to true'))
        time.sleep(1)

    def richos_switch(self):
        """RichOS's switch in the Accessibility list. The switches carry no title or description
        of their own (walk-943b91d5720b saw five untitled AXCheckBox nodes), so the switch is the
        checkbox on the same row as the static text "RichOS": the one whose vertical center is
        nearest that text's, within half a row."""
        rows = self.settings_find('AXCheckBox', 'RichOS')
        if rows:
            return rows
        # The pane answers no AXStaticText at all (walk-ba7a4420692c: texts [], five untitled
        # boxes): any node of any role whose title says RichOS names the row; and when nothing
        # names it, the pane after a denied prompt has exactly one switch off (RichOS's) among
        # the harness's own, all on, which is the row too, said so in the evidence.
        boxes = self.settings_find('AXCheckBox')
        try:
            names = [n for n in self.ax('find', '--title', 'RichOS', '--contains', app=SYSTEM_SETTINGS, timeout=60) if not n.get('meta')]
        except StepFailed:
            names = []
        self.pane_seen = {'names': [(n.get('role'), str(n.get('title') or ''), n.get('y')) for n in names][:20],
                          'boxes': [(n.get('x'), n.get('y'), n.get('w'), n.get('h'), n.get('value'), str(n.get('title') or ''),
                                     str(n.get('desc') or '')) for n in boxes]}
        rows = switch_on_the_row(names, boxes)
        self.switch_matched_by = 'the row named RichOS'
        if not rows:
            rows = the_only_switch_off(boxes)
            self.switch_matched_by = 'the only switch off in the list'
        return rows

    def turn_richos_on_in_accessibility(self):
        """System Settings on the Accessibility pane; RichOS's switch found by its row and pressed;
        the password entered where asked. Returns what was seen, for the report."""
        guest(self.vm, 'open ' + shlex.quote(AX_PANE))
        seen = {'pane_opened': False, 'row': None, 'switch_before': None, 'password_asked': False, 'password_button': None, 'switch_after': None}
        end = time.monotonic() + 60
        rows = []
        while time.monotonic() < end:
            if self.front_bundle() == 'com.apple.systempreferences':
                seen['pane_opened'] = True
                rows = self.richos_switch()
                if rows:
                    break
            time.sleep(1)
        if not seen['pane_opened']:
            raise StepFailed('System Settings did not come to the front')
        self.shot('granted-settings-pane.png')
        if not rows:
            seen['pane'] = getattr(self, 'pane_seen', None)
            raise StepFailed(f'no RichOS switch in the Accessibility pane (texts and boxes seen: {seen["pane"]})')
        seen['row'] = rows[0]
        seen['matched_by'] = getattr(self, 'switch_matched_by', None)
        seen['switch_before'] = rows[0].get('value')
        x, y = center_of(rows[0])
        self.ax('click', '--at', f'{x:.0f},{y:.0f}', app=SYSTEM_SETTINGS)
        time.sleep(2)
        if self.password_sheet_up():
            seen['password_asked'] = True
            self.shot('granted-settings-password.png')
            seen['password_button'] = self.enter_password()
            time.sleep(3)
        after = self.richos_switch()
        seen['switch_after'] = after[0].get('value') if after else None
        self.shot('granted-settings-on.png')
        return seen

    def granted_settings(self):
        """Accessibility allowed the way a person allows it, with the app running: no relaunch.
        From the denied state the ax-prompt step leaves (macOS was asked once), the person's way
        is the sheet's own Open System Settings beside the Accessibility row (round 19 state 12);
        when macOS asks again instead, the prompt's own Open System Settings. Then System
        Settings, where RichOS's switch is turned on and the password entered."""
        self.revoke('kTCCServiceAccessibility', True)
        self.post_ax_notice()
        launched, presses = self.launch()
        pid_before = launched['pid']
        if self.by_id('dict-switch') is None:
            self.open_sheet()
        if self.shows_text('Off. Turn it on to type with your voice'):
            self.ax('click', '--id', 'dict-switch', '--first')
        # Either the prompt, or the sheet's denied line with its button.
        frame, dismissed, way = None, None, None
        end = time.monotonic() + 30
        while time.monotonic() < end and way is None:
            self.shot('granted-settings-before.png')
            if self.frame_says('granted-settings-before.png', AX_PROMPT):
                frame = 'granted-settings-before.png'
                dismissed = self.press_in_prompt('Open System Settings') or self.press_in_prompt('Deny')
                way = 'the prompt'
            elif self.shows_text(DENIED_LINE) and self.present('Open System Settings'):
                self.click('Open System Settings')
                way = "the sheet's Open System Settings"
            else:
                time.sleep(2)
        if way is None:
            raise StepFailed('neither the Accessibility prompt nor the sheet\'s denied line with Open System Settings appeared within 30 s')
        taps_before = sum('key tap created for F1' in x for x in self.dlog_lines())
        seen = self.turn_richos_on_in_accessibility()
        seen['way_in'] = way
        switched = time.monotonic()
        tap = None
        while time.monotonic() - switched < 15:
            if sum('key tap created for F1' in x for x in self.dlog_lines()) > taps_before:
                tap = round(time.monotonic() - switched, 2)
                break
            time.sleep(0.25)
        guest(self.vm, 'osascript -e ' + shlex.quote('tell application "System Settings" to quit'))
        self.bring_front(pid_before)
        on = False
        try:
            on = self.until(lambda: self.shows_text(ON_RUN), 20, 'the sheet did not say On')
        except StepFailed:
            pass
        # The line under the card is in the same paint as On (D18), so it is read right after.
        line = on and self.shows_text(FEEDBACK_LINE)
        self.shot('granted-settings-sheet.png')
        alive = guest(self.vm, f'kill -0 {int(pid_before)} 2>/dev/null && echo alive || true') == 'alive'
        rows = self.tcc_rows()
        if not alive:
            raise StepFailed(f'the app (pid {pid_before}) is gone: the sheet must say On with no relaunch')
        if tap is None:
            raise StepFailed(f'the tool made no key tap within 15 s of the switch in System Settings ({seen}; TCC {rows})')
        if not on:
            raise StepFailed(f'the sheet did not say On after the switch in System Settings, with the app still running ({seen})')
        if not line:
            raise StepFailed(f'the sheet says On but not round 19\'s line under the card: {FEEDBACK_LINE!r} (D18)')
        return {'app_pid': pid_before, 'first_run_sheets_declined': presses, 'prompt_frame': frame, 'prompt_dismissed_in': dismissed,
                'system_settings': seen, 'key_tap_after_switch_seconds': tap, 'sheet_says_on': on, 'sheet_says_feedback': line,
                'same_app_pid': alive, 'tcc': rows}

    def try_it(self):
        pid = self.facts['app_pid']
        self.bring_front(pid)
        self.ax('click', '--id', 'dict-try', '--first')
        row = self.dictate('122', 'com.richos.app', 'clipboard-before-try-it')
        node = self.by_id('dict-try') or {}
        text = node.get('value') or node.get('title') or ''
        self.expect_words(text, row['expected'], 'Try it here')
        self.shot('try-it.png')
        return {'dictation': row, 'try_it_text': text}

    def refused(self):
        self.revoke('kTCCServiceAccessibility', True)
        self.post_ax_notice()
        launched, presses = self.launch()
        self.open_sheet()
        self.until(lambda: self.shows_text(DENIED_LINE), 30, 'the sheet did not say the denied line')
        button = self.by_id('dict-switch') is not None and self.present('Open System Settings')
        if not button:
            raise StepFailed('Open System Settings is not beside the denied line')
        self.shot('denied.png')
        return {'app_pid': launched['pid'], 'denied_line': DENIED_LINE, 'open_system_settings': True, 'tcc': self.tcc_rows()}

    def open_settings(self):
        self.click('Open System Settings')
        end = time.monotonic() + 30
        front = None
        while time.monotonic() < end:
            front = self.front_bundle()
            if front == 'com.apple.systempreferences':
                break
            time.sleep(1)
        if front != 'com.apple.systempreferences':
            raise StepFailed(f'System Settings did not come to the front: {front}')
        time.sleep(2)
        windows = guest(self.vm, 'osascript -e ' + shlex.quote(
            'tell application "System Events" to get name of windows of process "System Settings"'))
        self.shot('system-settings.png')
        if 'Accessibility' not in windows:
            raise StepFailed(f'System Settings is not on Accessibility: {windows!r}')
        guest(self.vm, 'osascript -e ' + shlex.quote('tell application "System Settings" to quit'))
        return {'front': front, 'windows': windows}

    def second_copy(self):
        self.grant('kTCCServiceAccessibility', True)
        self.post_ax_notice()
        first_pid = self.facts['app_pid']
        app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
        second_dir, home2 = self.payload + '/second', self.payload + '/home2'
        second = second_dir + '/' + Path(app).name
        guest(self.vm, f'rm -rf {shlex.quote(second_dir)} {shlex.quote(home2)} && mkdir -p {shlex.quote(second_dir)} '
                       f'&& cp -Rc {shlex.quote(app)} {shlex.quote(second)} && cp -Rc {shlex.quote(self.home)} {shlex.quote(home2)}', 300)
        # Its own process name: ax.sh reached the first copy's window for the second copy's pid
        # while both ran as "richos-tauri" (walk-2301826d94c2: the two trees were the same, the
        # frame showed the second copy's company question). The executable is renamed and the
        # copy signed ad hoc; it needs no permission of its own, only the key's owner.
        macos = second + '/Contents/MacOS'
        guest(self.vm, f'mv {shlex.quote(macos)}/richos-tauri {shlex.quote(macos)}/{SECOND_EXE} '
                       f'&& /usr/libexec/PlistBuddy -c "Set :CFBundleExecutable {SECOND_EXE}" {shlex.quote(second)}/Contents/Info.plist '
                       f'&& codesign --force --deep -s - {shlex.quote(second)} 2>&1', 300)
        env = {'HOME': home2, 'CLAUDE_CONFIG_DIR': home2 + '/.claude', 'RICHOS_ACTIVATION': 'regular',
               'RICHOS_CLAUDE_BIN': '/Users/admin/.local/bin/claude', 'DISABLE_AUTOUPDATER': '1',
               'RICHOS_ENGINE_DIR': self.payload + '/engine', 'RICHOS_DICTATION_TEST_ON': '1', **self.env()}
        log2 = self.payload + f'/second-{time.time_ns()}.log'
        argv = ['open', '-n', '-a', second]
        for key, value in env.items():
            argv += ['--env', f'{key}={value}']
        argv += ['--stdout', log2, '--stderr', log2]
        guest(self.vm, shlex.join(argv))
        pid2 = None
        for _ in range(100):
            for line in guest(self.vm, 'ps -axo pid=,ppid=,comm=').splitlines():
                parts = line.split(None, 2)
                if len(parts) == 3 and parts[1] == '1' and parts[2].startswith(second_dir) and parts[2].endswith('/' + SECOND_EXE):
                    pid2 = int(parts[0])
            if pid2:
                break
            time.sleep(0.2)
        if not pid2:
            raise StepFailed('the second copy did not start')
        state = Path(self.state_file('app.pid'))
        try:
            text2 = ''
            end = time.monotonic() + 60
            while time.monotonic() < end:
                text2 = guest(self.vm, 'cat ' + shlex.quote(log2) + ' 2>/dev/null || true', 30)
                if 'ended: exit status: 3' in text2 and 'connected to the tool' in text2:
                    break
                time.sleep(1)
            if 'ended: exit status: 3' not in text2:
                raise StepFailed('the second copy\'s tool did not find the key taken (exit 3); its log:\n' + text2[-3000:])
            # ax.sh drives the app whose pid run-walk recorded: the second copy, for this read only.
            state.write_text(f'{pid2}\n')
            def drawn():
                if self.by_id('set-btn') is not None:
                    return True
                self.declined_late_sheet()
                return False

            # Its window first, in front: walk-9f62f33077fc, this step's first run, waited
            # 180 s with the first copy's window over it and kept no frame of its own.
            self.bring_front(pid2)
            try:
                self.until(drawn, 180, 'the second copy drew no Settings button')
            except StepFailed:
                # Whose window is whose (walk-bdcaf2edd24e: the frame showed a company question and
                # ax.sh's tree an open Dictation sheet): every RichOS process, and each copy's tree.
                self.shot('second-copy-unseen.png')
                procs = guest(self.vm, 'ps -axo pid=,ppid=,lstart=,command= | grep -E "richos-(tauri|second)" | grep -v -F "grep -E" || true')
                trees = [f'first copy {first_pid}, second copy {pid2}, app.pid now {state.read_text().strip()}', procs]
                for who, pid in (('first', first_pid), ('second', pid2)):
                    state.write_text(f'{pid}\n')
                    r = subprocess.run([str(HERE / 'ax.sh'), self.vm, 'tree', '--depth', '40'],
                                       capture_output=True, text=True, timeout=120)
                    trees.append(f'--- {who} copy, pid {pid} ---\n' + r.stdout + r.stderr)
                state.write_text(f'{pid2}\n')
                (self.out / 'second-copy-tree.txt').write_text('\n'.join(trees))
                (self.out / 'second-copy.log').write_text(guest(self.vm, 'cat ' + shlex.quote(log2) + ' 2>/dev/null || true', 30))
                raise
            self.decline_first_run_sheets(seconds=20)
            self.bring_front(pid2)
            self.open_settings_panel()
            # The row's state line is a span inside the row's button, so the accessibility tree
            # carries it in the button's own title (walk-8631ebd515c6: no node for set-dictation-state); the
            # frame is read too, by OCR, and kept whatever it says.
            self.shot('second-copy-row.png')
            row = self.by_id('set-dictation-open') or {}
            row_text = ' '.join(str(row.get(k) or '') for k in ('title', 'desc', 'value'))
            row_ocr = self.frame_says('second-copy-row.png', OTHER_ROW)
            if OTHER_ROW not in row_text and not self.shows_text(OTHER_ROW) and not row_ocr:
                raise StepFailed(f'the second copy\'s row does not say "{OTHER_ROW}": {row_text!r} (second-copy-row.png)')
            self.ax('click', '--id', 'set-dictation-open', '--first')
            self.until(lambda: self.by_id('dict-switch') is not None, 30, 'the second copy\'s sheet did not open')
            time.sleep(1)
            self.shot('second-copy-sheet.png')
            sheet_ax = self.shows_text(OTHER_SHEET)
            sheet_ocr = self.frame_says('second-copy-sheet.png', 'talk and tap it again')
            if not sheet_ax and not sheet_ocr:
                raise StepFailed(f'the second copy\'s sheet does not say "{OTHER_SHEET}" (second-copy-sheet.png)')
            seen = {'row_text': row_text.strip(), 'row_ocr': row_ocr, 'sheet_ax': sheet_ax, 'sheet_ocr': sheet_ocr}
        finally:
            state.write_text(f'{first_pid}\n')
            guest(self.vm, f'kill -TERM {int(pid2)} 2>/dev/null || true')
        end = time.monotonic() + 10
        while time.monotonic() < end and guest(self.vm, f'kill -0 {int(pid2)} 2>/dev/null && echo alive || true') == 'alive':
            time.sleep(0.2)
        gone = guest(self.vm, f'kill -0 {int(pid2)} 2>/dev/null && echo alive || true') != 'alive'
        if not gone:
            raise StepFailed(f'the second copy (pid {pid2}) did not quit')
        return {'second_pid': pid2, 'second_tool_exit': 3, 'row': OTHER_ROW, 'sheet': OTHER_SHEET, 'second_quit': True,
                'first_pid': first_pid, **seen}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--wav', type=Path, required=True, help='a short spoken sample, 16 kHz mono WAV (say -o, then afconvert)')
    p.add_argument('--model', type=Path, required=True, help='the GGML weights to copy in, matching --model-id')
    p.add_argument('--model-id', default='small.en')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    # DictationWalk's helpers read these; this walk posts no top-row event and runs no probe.
    a.post_key = a.key_probe = None
    a.idle = 0
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = SheetWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    unproven = False
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except StepUnproven as exc:
            row['outcome'] = 'UNPROVEN'
            row['detail'] = str(exc)
            row['evidence'] = exc.evidence
            unproven = True
        except (StepFailed, RuntimeError, ValueError, KeyError, IndexError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            try:
                walk.shot(f'{step}-failed.png')
                (a.out / 'app.log.tail').write_text(guest(a.vm, 'tail -80 ' + shlex.quote(walk.log) + ' || true', 30))
                (a.out / 'dictation.log').write_text(guest(a.vm, 'cat ' + shlex.quote(walk.dlog) + ' 2>/dev/null || true', 30))
            except Exception as exc:  # evidence only; the step already failed
                (a.out / 'evidence-error.txt').write_text(str(exc))
            break
    # 2: every step that could be settled passed, and at least one could not be.
    return 1 if not ok else 2 if unproven else 0


if __name__ == '__main__':
    sys.exit(main())
