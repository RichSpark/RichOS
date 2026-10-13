#!/usr/bin/env python3
"""voice-walk.py — on a Mac with no whisper-cli of its own, the app hears with the engine's (CEO 2026-10-07).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      voice-walk.py --out DIR --expect-sha SHA --wav SPOKEN.wav --model GGML.bin [--model-id ID]

run-walk.py passes the owned VM name as the first argument.

THE QUESTION. The CEO, 2026-10-07: "I thought Whisper was a key part of the whole RichOS app ...
And now, you're telling me it's not guaranteed to be on the user's Mac??" The engine's runtime
now carries whisper-cli (build-runtimes.py), and the app resolves it before PATH and Homebrew
(richos-voice stt.rs). This walk asks a clean guest, which has no Homebrew and no whisper-cli,
to turn one spoken sample into words through the app.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity    the running app says it was built from --expect-sha
  no-decoder  `command -v whisper-cli` prints nothing in a login shell and in /bin/sh, and no
              Homebrew or /usr/local copy exists
  runtime     the engine's runtime/bin/whisper-cli exists, is listed in its delivery.json with
              that sha256, and says its version
  first-run   adopt-walk.py's: memory setup declined, company "Acme" registered, questions declined
  stage       the spoken sample and the speech model are copied into the guest home. The model
              is copied, not downloaded: fetching weights is provision.rs's own, separate proof.
              The guest's microphone permission for com.richos.app is granted in its TCC.db,
              the "Allow" a person gives once (see grant_microphone)
  relaunch    the app is relaunched with RICHOS_VOICE_INPUT_WAV (capture.rs: the WAV stands in
              for the microphone through the identical capture path; no audio device is opened
              and nothing is played) and RICHOS_VOICE_WHISPER_MODEL_ID
  ready       the app's own voice-readiness line says ready, and its decoder sha256 is the
              runtime's whisper-cli
  heard       the guest's own sound output is muted (see mute_guest_output), the home screen's
              door is passed, the talk control (#talk-toggle) is pressed and turns into "Stop
              talking"; the transcript the app stored (PromptReceived) is read from the guest's
              ledger. With --one-turn, the walk then waits --settle seconds and requires that
              the whole sample became exactly ONE spoken turn holding every --expect-words word
              (2026-10-13: a sentence with a pause in the middle of it used to become two turns,
              and Rich answered the first half while the CEO was saying the second).

CEO §53: no sound is played anywhere. The sample is made with `say -o`, which writes a file.
Rich's spoken answer is played into the guest's MUTED output: tart passes guest audio through to
the host unless it is run with --no-audio, and the guest needs an output device for voice to start.
Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed
command = adopt_walk.command

STEPS = ['identity', 'no-decoder', 'runtime', 'first-run', 'stage', 'relaunch', 'ready', 'heard']
READY = '[richos] voice: ready on this machine'
NOT_READY = '[richos] voice: not ready on this machine'


def spoken_prompts(rows):
    """Every stored prompt that came from voice: a PromptReceived with source "jam" (ledger.rs
    `Source::Jam`) and words in it. A re-prime or any other internal turn is never one (the first
    run of this walk read a "[re-prime]" turn as the transcript, which is why this is a function)."""
    return [row for row in rows
            if row.get('event') == 'PromptReceived' and row.get('source') == 'jam' and (row.get('text') or '').strip()]


def spoken_prompt(rows):
    """The first of spoken_prompts(rows), or None."""
    found = spoken_prompts(rows)
    return found[0] if found else None


def one_turn_problem(texts, words):
    """Why the stored spoken turns are NOT one turn holding every expected word, or None when they
    are. `texts` is every spoken prompt's text in ledger order; `words` the words the sample says
    across its pause, which must all be in that one turn (case-insensitive)."""
    if len(texts) != 1:
        return f'the sample became {len(texts)} spoken turns, not one: {texts}'
    missing = [w for w in words if w.lower() not in texts[0].lower()]
    if missing:
        return f'the one turn is missing {missing}: {texts[0]!r}'
    return None


def decoder_sha12(line):
    """The `bin:<sha12>` the readiness line carries (toolchain.rs `provenance`), or None."""
    for word in line.split():
        if word.startswith('bin:'):
            return word[4:16]
    return None


class VoiceWalk(adopt_walk.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.engine = self.payload + '/engine'
        self.wav = self.home + '/voice-sample.wav'
        self.log = self.payload + '/app.log'

    def no_decoder(self):
        login = guest(self.vm, "zsh -lc 'command -v whisper-cli || true'")
        plain = guest(self.vm, "/bin/sh -c 'command -v whisper-cli || true'")
        copies = guest(self.vm, 'ls /opt/homebrew/bin/whisper-cli /usr/local/bin/whisper-cli /usr/bin/whisper-cli '
                                '2>/dev/null || true')
        if login or plain or copies:
            raise StepFailed(f'this guest has its own whisper-cli: login={login!r} sh={plain!r} files={copies!r}')
        return {'command_v_login_shell': login, 'command_v_sh': plain, 'homebrew_or_usr_copies': copies}

    def runtime(self):
        path = self.engine + '/runtime/bin/whisper-cli'
        sha = guest(self.vm, 'shasum -a 256 ' + shlex.quote(path)).split()[0]
        listed = json.loads(guest(self.vm, 'cat ' + shlex.quote(self.engine + '/runtime/delivery.json')))
        if listed['files'].get('bin/whisper-cli') != sha:
            raise StepFailed(f'runtime whisper-cli {sha} is not the delivery.json entry {listed["files"].get("bin/whisper-cli")}')
        version = guest(self.vm, shlex.quote(path) + ' --version 2>/dev/null')
        self.facts['runtime_whisper_sha256'] = sha
        self.save()
        return {'path': path, 'sha256': sha, 'version': version, 'delivery_version': listed['versions'].get('whisper-cli')}

    def grant_microphone(self):
        """The "Allow" a person gives the first time voice opens. walk-e5e9903a36b4 (2026-10-07)
        pressed the talk control and macOS put up '"RichOS" would like to access the microphone.'
        over the window (talk-refused.png); start_voice_capture waited on it and the control never
        turned into "Stop talking". No walk in this harness answers a system privacy prompt by
        script; every grant the guest needs is written instead, as provision-guest.sh writes the
        screen-capture and accessibility grants. This one follows it: a row in the guest user's TCC.db
        (the guest image ships with SIP off; never on a host). The row is the same one tccd writes
        when a person presses Allow: service kTCCServiceMicrophone, client com.richos.app (the
        bundle identifier, tauri.conf.json), client_type 0, auth_value 2 (allowed)."""
        db = '"$HOME/Library/Application Support/com.apple.TCC/TCC.db"'
        sql = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, "
               "auth_version, indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) "
               "VALUES ('kTCCServiceMicrophone', 'com.richos.app', 0, 2, 2, 1, 0, 'UNUSED', 0, strftime('%s','now'));")
        guest(self.vm, f'sqlite3 {db} {shlex.quote(sql)}')
        row = guest(self.vm, f'sqlite3 {db} ' + shlex.quote(
            "SELECT service, client, auth_value FROM access WHERE service='kTCCServiceMicrophone' "
            "AND client='com.richos.app';"))
        if row != 'kTCCServiceMicrophone|com.richos.app|2':
            raise StepFailed(f'the guest microphone grant for com.richos.app did not take: {row!r}')
        return row

    def stage(self):
        microphone = self.grant_microphone()
        command([HERE / 'guest.sh', self.vm, '--push', self.a.wav, self.wav], 120)
        models = self.home + '/.config/richos/models'
        guest(self.vm, 'mkdir -p ' + shlex.quote(models))
        target = f'{models}/ggml-{self.a.model_id}.bin'
        command([HERE / 'guest.sh', self.vm, '--push', self.a.model, target], 900)
        return {'wav': self.wav, 'model': target, 'microphone_grant': microphone,
                'model_sha256': guest(self.vm, 'shasum -a 256 ' + shlex.quote(target), 120).split()[0]}

    def relaunch(self):
        launched = relaunch(self.vm, environment={'RICHOS_VOICE_INPUT_WAV': self.wav,
                                                  'RICHOS_VOICE_WHISPER_MODEL_ID': self.a.model_id})
        self.log = launched['log']
        self.facts['relaunch'] = launched
        self.facts['relaunched_at_ms'] = self.clock()
        self.save()
        self.window_up(seconds=60)
        return launched

    def window_up(self, seconds):
        """The relaunched window shows "Talk to Rich" (the home door or the talk control). A find
        is only a read, so a read that hit ax.sh's guest deadline is asked again inside the same
        budget: walk-3fa7da94531b failed here on one 19 s AX read (exit 124, guest_deadline) of
        a window that walk-e5e9903a36b4, the same bundle, had read at once."""
        end = time.monotonic() + seconds
        last = None
        declined = False
        while time.monotonic() < end:
            try:
                # The home screen's door is an AXButton; with no home screen the window opens on
                # the conversation, whose talk control (#talk-toggle, aria-pressed) is an
                # AXCheckBox/AXToggle (candidate 43, voice run 4: relaunch-missing.tree).
                if self.present('Talk to Rich') or self.present('Talk to Rich', role='AXCheckBox'):
                    return True
                # This walk copies one speech model in and declines setup, so the video tools are
                # still missing and the relaunched app puts the setup sheet up again (setup
                # essentials, e0a346d76); it holds the window and the find above sees nothing
                # (candidate 43, voice run 2: relaunch-missing.png). Declined once, as at first run.
                if not declined and self.present('Set it up'):
                    self.press('Not now')
                    declined = True
                    self.facts['relaunch_setup_sheet_declined'] = True
                    self.save()
            except StepFailed as exc:
                if 'guest_deadline' not in str(exc):
                    raise
                last = exc
            time.sleep(1)
        self.shot('relaunch-missing.png')
        # Why the find saw nothing, kept: present() reads a refusal ("blocked: ... modal=...") as
        # not found, and candidate 43's voice run 3 showed the talk control on screen while the
        # find kept missing it. The raw answer and the window's tree say which.
        why = ''
        try:
            self.ax('find', '--title', 'Talk to Rich', '--role', 'AXButton', '--contains', '--first')
        except StepFailed as exc:
            why = str(exc)[-400:]
        try:
            (self.out / 'relaunch-missing.tree').write_text(command([HERE / 'ax.sh', self.vm, 'tree'], 60))
        except Exception as exc:  # the tree is evidence only; the step already failed
            why += f'; tree: {exc}'
        raise StepFailed(f'"Talk to Rich" never appeared within {seconds} s of the relaunch' +
                         (f'; last AX read: {last}' if last else '') + (f'; raw find: {why}' if why else ''))

    def ready(self):
        end = time.monotonic() + 60
        while time.monotonic() < end:
            text = guest(self.vm, 'cat ' + shlex.quote(self.log) + ' 2>/dev/null || true')
            lines = [line for line in text.splitlines() if READY in line or NOT_READY in line]
            if lines:
                line = lines[-1]
                if NOT_READY in line:
                    raise StepFailed(line)
                want = self.facts['runtime_whisper_sha256'][:12]
                if decoder_sha12(line) != want:
                    raise StepFailed(f'voice is ready on a decoder that is not the runtime\'s ({want}): {line}')
                return {'line': line}
            time.sleep(1)
        raise StepFailed('the app printed no voice-readiness line within 60 s')

    def ax(self, mode, *args, app=None, timeout=40):
        """adopt-walk's ax(), with a READ that hit ax.sh's guest deadline asked again, up to three
        times. Two runs on 2026-10-07 each lost a step to one such read on a busy host:
        walk-3fa7da94531b in relaunch, walk-ea458ffa8bca in first-run (adopt-walk's "Start the
        questions" find), both exit 124, guest_deadline. A find changes nothing in the guest, so a
        second ask is the same question. A click is never asked again: its effect is unknown."""
        for attempt in range(3):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if mode != 'find' or 'guest_deadline' not in str(exc) or attempt == 2:
                    raise

    def by_id(self, dom_id):
        """The node whose DOM id is dom_id, or None when it is not in the accessibility tree."""
        try:
            nodes = self.ax('find', '--id', dom_id, '--first')
        except StepFailed as exc:
            # A read that hit ax.sh's guest deadline is a read with no answer: the callers loop.
            if 'notfound' in str(exc) or 'nothing matched' in str(exc) or 'guest_deadline' in str(exc):
                return None
            raise
        nodes = [n for n in nodes if not n.get('meta')]
        return nodes[0] if nodes else None

    def press_by_id(self, dom_id):
        """One press. A press whose ax.sh call hit the guest deadline may or may not have landed
        (walk-977f9c8d80b5 lost its heard step to exactly that on the home door), so it is not
        a failure here and it is not pressed again: the caller reads what is on screen next."""
        try:
            return self.ax('click', '--id', dom_id)
        except StepFailed as exc:
            if 'guest_deadline' not in str(exc):
                raise
            return None

    def talk(self):
        """Open the microphone the way a person does: through the home screen's door, then the talk
        control. TWO BUTTONS ARE NAMED "Talk to Rich". The home screen's door (#home-enter,
        home.js DOOR_LABEL) opens the conversation and nothing else, and while the home screen is
        up #app is inert, so the door is the ONLY "Talk to Rich" in the tree. The 2026-10-07 run
        (walk-1fede03eadc0) pressed it by title and waited 120 s for a capture that nothing had
        started. The talk control is #talk-toggle (index.html); its label turns into "Stop talking"
        only after start_voice_capture resolves (main.js enterVoiceMode), so that label is the
        proof the press opened capture."""
        end = time.monotonic() + 30
        toggle = None
        door = False
        while time.monotonic() < end:
            toggle = self.by_id('talk-toggle')
            if toggle:
                break
            if self.by_id('home-enter'):
                # Pressed again only if a later pass still SEES the door: state, not a blind retry.
                self.press_by_id('home-enter')
                door = True
            time.sleep(1)
        if not toggle:
            self.shot('talk-missing.png')
            raise StepFailed('the talk control (#talk-toggle) is not on screen within 30 s of the home door')
        self.press_by_id('talk-toggle')  # once; what it did is read below
        end = time.monotonic() + 30
        while time.monotonic() < end:
            toggle = self.by_id('talk-toggle') or {}
            if toggle.get('desc') == 'Stop talking':
                return {'home_door_pressed': bool(door), 'talk_control': toggle}
            time.sleep(1)
        self.shot('talk-refused.png')
        log = guest(self.vm, 'tail -40 ' + shlex.quote(self.log) + ' || true')
        raise StepFailed(f'the talk control did not turn into "Stop talking" within 30 s: {toggle}\nlog tail:\n{log}')

    def mute_guest_output(self):
        """Mute the GUEST's sound output before Rich can answer aloud. run.sh starts tart without
        --no-audio, and tart then passes guest audio through to the host ("Disable audio
        pass-through to host" is the opt-in flag), so a spoken answer in the guest would come out
        of this Mac's speakers: a live playback on the CEO's Mac (CEO §53). The guest keeps its
        output device, which voice needs to start (playout opens it), and plays into silence.
        Only the guest is touched; the host's volume is never read or set (CEO §65)."""
        guest(self.vm, "osascript -e 'set volume with output muted'")
        muted = guest(self.vm, "osascript -e 'output muted of (get volume settings)'")
        if muted.strip() != 'true':
            raise StepFailed(f'the guest output did not mute (output muted = {muted!r}); not talking with sound on')
        return muted.strip()

    def spoken_rows(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        return spoken_prompts(json.loads(r) for r in rows.splitlines() if '"PromptReceived"' in r)

    def one_turn(self):
        """The whole sample, a sentence with a pause in the middle of it, is ONE spoken turn holding
        every expected word. Waits --settle seconds after the first turn so a second one has time
        to arrive if the pause split the sentence."""
        time.sleep(self.a.settle)
        rows = self.spoken_rows()
        log = guest(self.vm, 'grep -E "\\[richos-voice\\]" ' + shlex.quote(self.log) + ' || true')
        timeline = [line for line in log.splitlines()
                    if any(k in line for k in ('utterance START', 'utterance END', 'end-of-turn quiet',
                                               'TURN to Rich', 'ANSWER AUDIBLE', 'answer HELD',
                                               'answer SUPERSEDED', 'fragment recognized'))]
        texts = [row['text'] for row in rows]
        problem = one_turn_problem(texts, [w for w in self.a.expect_words.split(',') if w])
        if problem:
            raise StepFailed(problem + '\n' + '\n'.join(timeline))
        return {'turns': len(rows), 'transcript': texts[0], 'timeline': timeline}

    def heard(self):
        muted = self.mute_guest_output()
        talk = self.talk()
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            rows = self.spoken_rows()
            row = rows[0] if rows else None
            if row:
                log = guest(self.vm, 'grep -iE "richos-voice|\\[richos\\] voice|wav" ' + shlex.quote(self.log) + ' || true')
                self.shot('heard.png')
                result = {'transcript': row['text'], 'row': row, 'talk': talk, 'guest_output_muted': muted,
                          'voice_log': log.splitlines()[-20:]}
                if self.a.one_turn:
                    result["one_turn"] = self.one_turn()
                return result
            time.sleep(2)
        log = guest(self.vm, 'tail -40 ' + shlex.quote(self.log) + ' || true')
        # What the window showed when nothing was heard: the 2026-10-07 run pressed "Talk to Rich"
        # and the app logged no capture at all, and only the screen can say why.
        self.shot('heard-failed.png')
        raise StepFailed(f'no transcript was stored within {self.a.within} s; log tail:\n{log}')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--wav', type=Path, required=True, help='a short spoken sample, 16 kHz mono WAV (say -o, then afconvert)')
    p.add_argument('--model', type=Path, required=True, help='the GGML weights to copy in, matching --model-id')
    p.add_argument('--model-id', default='small.en')
    p.add_argument('--within', type=float, default=120, help='seconds for the transcript to be stored')
    p.add_argument('--one-turn', action='store_true',
                   help='require the whole sample to become exactly one spoken turn (a pause inside a sentence)')
    p.add_argument('--settle', type=float, default=15,
                   help='with --one-turn: seconds to wait after the first turn for a second one to appear')
    p.add_argument('--expect-words', default='',
                   help='with --one-turn: comma-separated words that must all be in that one turn')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = VoiceWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
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
