#!/usr/bin/env python3
"""engine-update-walk.py — after an update, the app installs its own build's engine by itself,
and the setup sheet never comes up (the CEO, 2026-11-14).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone. The bundle run.sh launches is the build being updated FROM, with NO engine handed in, so
its own first setup installs the engine its pin names:

  run-walk.py --bundle FROM_ZIP --home EMPTY_DIR --engine '' --report REPORT --wait 600 -- \\
      engine-update-walk.py --out DIR --next-bundle NEXT_ZIP --expect-sha SHA \\
          --from-sha256 DIGEST --next-sha256 DIGEST

HIS WORDS. "Is it normal and expected that I always get to see the setup screen ... after updating
to a new version?" It is not: each build pins its own engine (setup.rs), and until 2026-11-14 the
engine a newer build pins was installed only from his press of "Set it up".

STEPS, in the guest, never on the host's screen (CEO ruling §65):
  setup      the FROM build's first setup as a user does it (new-user-walk.py's setup step): the
             setup sheet's "Set it up", the memory question "Not now", the company added, the
             first conversation; then the engine RichOS installed carries --from-sha256 in its
             INSTALLED-FROM stamp
  update     the FROM app quit by its pid; NEXT_ZIP copied into the guest and its app started with
             the same home and harness environment and no RICHOS_ENGINE_DIR (an operator's engine
             outranks the pin, which is not the case under test), by its pid
  after      the next app says it was built from --expect-sha; its boot says the installed engine
             is an earlier build's and that it installs its own by itself. A relaunch opens on
             the home screen and its questions, the sheet among them, wait until he goes in, so
             the walk presses the home screen's "Talk to Rich" (home-enter) as soon as it is up.
             From launch until the install is done and 15 s past it, the setup sheet's title
             ("There's a bit of setting up to do first.") is never on screen, read by the
             accessibility tree every second and by OCR on a frame every few seconds. 15 s after
             going in, while the install still runs, a message is sent with real keys and must
             be answered (TurnCompleted with Rich's text) BEFORE the install ends: he never waits
             on it. After the install the window must ask voice again (a new `[richos] voice:`
             line within 15 s), so Talk to Rich works with no relaunch. The stamp then carries
             --next-sha256 (photographed)

A CONTROL: run against a build without the 2026-11-14 change (same pin, same command), the walk
fails at `after` with "the setup sheet came up", which is the CEO's screenshot.
  quit       the next app quit by its pid and gone from the guest's process table

Evidence: <out>/report.json, the frames, <out>/from.log and <out>/next.log. Exit 0 only when every
step passes. run-walk.py's stop.sh quits whatever instance is recorded and removes the guest.
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
from relaunch import guest, quit_app, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('new_user_walk', HERE / 'new-user-walk.py')
new_user = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(new_user)
StepFailed = new_user.StepFailed
voice_lines = new_user.setup_walk.voice_lines

STEPS = ['setup', 'update', 'after', 'quit']
SHEET_TITLE = "There's a bit of setting up to do first."
# OCR splits lines differently from the sheet; this fragment sits on one line of the title.
SHEET_OCR = 'setting up to do first'
STARTED = '[richos] engine update: the installed engine is an earlier build'
DONE = "[richos] engine update: this build's engine is installed and in use"
FAILED = '[richos] engine update FAILED'
REPLY_WORDS = 'Reply with one word: received.'
SWAPPED = '[richos] setup: engine '


def stamp_digest(text):
    """The `sha256` line of an INSTALLED-FROM stamp, lowercase, or ''."""
    m = re.search(r'^sha256\s+([0-9a-fA-F]{64})\s*$', text, re.M)
    return m.group(1).lower() if m else ''


class EngineUpdateWalk(new_user.NewUserWalk):
    def __init__(self, a):
        super().__init__(a)
        self.stamp = self.home + '/Library/Application Support/RichOS/engine/INSTALLED-FROM'
        self.next_log = None
        self.next_pid = None

    def installed_digest(self):
        return stamp_digest(guest(self.vm, 'cat ' + shlex.quote(self.stamp) + ' 2>/dev/null || true'))

    # --- steps --------------------------------------------------------------------------------
    def setup(self):
        evidence = super().setup()
        digest = self.installed_digest()
        if digest != self.a.from_sha256:
            raise StepFailed('the engine the first build installed carries %r, not %s' % (digest, self.a.from_sha256))
        evidence['installed_sha256'] = digest
        (self.out / 'from.log').write_text(self.read_log(self.log))
        return evidence

    def update(self):
        guest_zip = self.payload + '/next.zip'
        guest_dir = self.payload + '/next'
        new_user.command([HERE / 'guest.sh', self.vm, '--push', self.a.next_bundle, guest_zip], 600)
        guest(self.vm, 'rm -rf {d} && mkdir -p {d} && ditto -x -k {z} {d}'.format(
            d=shlex.quote(guest_dir), z=shlex.quote(guest_zip)), 300)
        app = guest(self.vm, 'find ' + shlex.quote(guest_dir) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()
        if not app:
            raise StepFailed('the next bundle holds no .app')
        # relaunch() quits the recorded FROM app by its pid first (quit_app), then starts this one.
        # RICHOS_ENGINE_DIR empty is unset to the app (engine.rs and setup.rs drop a blank one).
        started = relaunch(self.vm, app[0], {'RICHOS_ENGINE_DIR': ''})
        self.next_log, self.next_pid = started['log'], started['pid']
        return {'app': app[0], 'pid': self.next_pid, 'log': self.next_log}

    def send(self, words, name, seconds):
        """new-user-walk.py's send (real keys, Return), and the Send button pressed once when the
        Return was not taken: walk5 (2026-11-14) left the words in the composer for 250 s after
        the Return, with the app answering nothing because nothing had been sent."""
        self.type_into(words, '--role', 'AXTextArea', '--title', 'Message to Rich')
        new_user.command([HERE / 'ax.sh', self.vm, '--key', new_user.RETURN_KEY])
        end = time.monotonic() + 10
        while time.monotonic() < end and not self.received(words):
            time.sleep(1)
        if not self.received(words):
            self.ax('click', '--id', 'send', '--first')
            end = time.monotonic() + 10
            while time.monotonic() < end and not self.received(words):
                time.sleep(1)
        if not self.received(words):
            # Still not taken: the screen and the app's log as they are now, then the words typed
            # and sent once more, as a person would.
            self.frame(name + '-not-taken')
            (self.out / (name + '-not-taken.log')).write_text(self.read_log(self.next_log))
            self.type_into(words, '--role', 'AXTextArea', '--title', 'Message to Rich')
            new_user.command([HERE / 'ax.sh', self.vm, '--key', new_user.RETURN_KEY])
        end = time.monotonic() + seconds
        outcome = None
        while time.monotonic() < end:
            outcome = new_user.turn_outcome(self.ledger_rows(), words)
            if outcome:
                break
            time.sleep(2)
        time.sleep(3)  # the card is drawn after the ledger row
        return outcome, self.frame(name)

    def received(self, words):
        return any(r.get('event') == 'PromptReceived' and (r.get('text') or '').strip() == words
                   for r in self.ledger_rows())

    def sheet_seen(self, name):
        """Whether the setup sheet's title is on screen: the accessibility tree, and OCR on a frame."""
        if self.shows_text(SHEET_TITLE):
            return 'accessibility tree'
        if name and SHEET_OCR in self.frame(name):
            return 'ocr ' + name + '.png'
        return ''

    def after(self):
        text = ''
        end = time.monotonic() + 60
        while time.monotonic() < end and 'this app: built from' not in text:
            text = self.read_log(self.next_log)
            time.sleep(1)
        built = re.search(r'this app: built from (\S+)', text)
        built = built.group(1) if built else ''
        # A branch bundle reports the commit it was built from (branch-bundle.sh RICHOS_SOURCE_SHA).
        if not built or not (built.startswith(self.a.expect_sha) or self.a.expect_sha.startswith(built)):
            raise StepFailed('the next app was built from %r, not %s' % (built, self.a.expect_sha))
        seen, done_at, n, entered, reply = [], None, 0, None, None
        began = time.monotonic()
        end = began + self.a.within
        while time.monotonic() < end:
            text = self.read_log(self.next_log)
            if FAILED in text:
                raise StepFailed('the app could not install its engine: '
                                 + next(l for l in text.splitlines() if l.startswith(FAILED)))
            if done_at is None and DONE in text:
                done_at = time.monotonic()
            # A RELAUNCH OPENS ON THE HOME SCREEN, and the start's questions, the setup sheet
            # among them, wait for him to go in (ui/main.js askStartupQuestionsOffHome). So the
            # walk goes in as he does, by "Talk to Rich", the moment the home screen is up:
            # watching the home screen alone would pass a build that shows the sheet. By its id
            # (home.js `home-enter`): the voice toggle in the message box has the same label.
            if entered is None and self.by_id('home-enter'):
                self.frame('home-before-entering')
                self.ax('click', '--id', 'home-enter', '--first')
                entered = round(time.monotonic() - began, 1)
            # HE NEVER WAITS ON THE INSTALL: a message sent while it runs is answered while it
            # still runs, by the engine the earlier build installed. Sent 15 s after going in, so
            # a sheet that comes up on going in (a build without this change: 12.6 s) is seen
            # first.
            elif entered is not None and reply is None and time.monotonic() - began >= entered + 15:
                if DONE in text:
                    raise StepFailed('the install finished before the walk could send a message during it')
                sent = time.monotonic()
                outcome, _ = self.send(REPLY_WORDS, 'reply-during-update', self.a.turn_within)
                after = self.read_log(self.next_log)
                reply = {'sent_at_s': round(sent - began, 1), 'seconds': round(time.monotonic() - sent, 1),
                         'outcome': outcome, 'install_done_when_answered': DONE in after}
                (self.out / 'next.log').write_text(after)
                if not outcome or outcome.get('state') != 'completed' or not outcome.get('rich_wrote'):
                    raise StepFailed('the message sent during the install was not answered: ' + json.dumps(reply))
                if reply['install_done_when_answered']:
                    raise StepFailed('the message was answered only after the install ended: ' + json.dumps(reply))
                continue
            n += 1
            # A frame every fourth read (OCR costs seconds); the tree every read.
            where = self.sheet_seen('after-%02d' % n if n % 4 == 1 else None)
            if where:
                seen.append({'at_s': round(time.monotonic() - began, 1), 'by': where})
                self.frame('sheet-came-up')
                break
            if done_at is not None and reply is not None and time.monotonic() - done_at >= 15:
                break
            time.sleep(1)
        (self.out / 'next.log').write_text(text)
        if seen:
            raise StepFailed('the setup sheet came up: ' + json.dumps(seen))
        if entered is None:
            raise StepFailed('the home screen\'s "Talk to Rich" never came up, so the walk never went in')
        if STARTED not in text:
            raise StepFailed('the next app never said it installs its own engine; its log is next.log')
        if done_at is None:
            raise StepFailed('the next app did not finish installing its engine within %g s' % self.a.within)
        digest = self.installed_digest()
        if digest != self.a.next_sha256:
            raise StepFailed('after the update the stamp carries %r, not %s' % (digest, self.a.next_sha256))
        # VOICE IS ASKED AGAIN once the new engine is in use, with no relaunch: the window's ask
        # prints a `[richos] voice:` line every time (main.rs voice_readiness).
        # Counted from the swap, where the app tells the window (setup_view::swap_in_engine prints
        # the `setup: engine` line, then the window is told): the done line comes after the rest
        # of the wiring, so counting from it missed the ask (walk4, 2026-11-14).
        lines = text.splitlines()
        swapped = next((i for i, l in enumerate(lines) if l.startswith(SWAPPED)), None)
        asked = len(voice_lines('\n'.join(lines[swapped + 1:]))) if swapped is not None else 0
        if asked < 1:
            raise StepFailed("the window did not ask voice again after this build's engine came into use")
        final = self.frame('after-update')
        line = next(l for l in text.splitlines() if l.startswith(DONE))
        return {'built_from': built, 'installed_sha256': digest, 'done': line,
                'went_in_at_s': entered, 'install_done_at_s': round(done_at - began, 1),
                'reply_during_update': reply, 'voice_asked_after_install': asked,
                'talk_toggle_on_screen': self.by_id('talk-toggle') is not None,
                'sheet_reads': n, 'frames': (n + 3) // 4, 'company_on_screen': new_user.COMPANY in final}

    def quit(self):
        quit_app(self.vm)
        alive = guest(self.vm, 'ps -p %d -o pid= 2>/dev/null || true' % int(self.next_pid))
        if alive.strip():
            raise StepFailed('the next app (pid %s) is still running' % self.next_pid)
        return {'quit_pid': self.next_pid, 'gone': True}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--next-bundle', required=True, help='the zip of the build updated TO')
    p.add_argument('--expect-sha', required=True, help='the commit the next bundle was built from')
    p.add_argument('--from-sha256', required=True, help="the FROM build's engine pin digest")
    p.add_argument('--next-sha256', required=True, help="the next build's engine pin digest")
    p.add_argument('--within', type=float, default=1200, help='seconds for first setup, and for the update')
    p.add_argument('--turn-within', type=float, default=240, help='seconds for the message sent during the install')
    a = p.parse_args()
    a.from_sha256, a.next_sha256 = a.from_sha256.lower(), a.next_sha256.lower()
    if a.from_sha256 == a.next_sha256:
        p.error('the two builds pin the same engine, so there is no update to walk')
    walk = EngineUpdateWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in STEPS:
        began = time.monotonic()
        row = new_user.run_step(walk, step)
        ok = ok and row['outcome'] == 'PASS'
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    report['complete'] = [row['step'] for row in report['steps']] == STEPS
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
