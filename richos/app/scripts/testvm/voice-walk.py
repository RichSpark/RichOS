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
  new-conversation  (ask for it with --steps) once this launch's spare desk is primed, a new
              conversation is started the only way the app starts one, a typed first message
              (voice rides the conversation already open: main.rs start_voice_capture), so it
              adopts that spare; its reply timing lines must say the session already holds the
              context brief. The spoken turn after it is then in the new conversation
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
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed
command = adopt_walk.command

STEPS = ['identity', 'no-decoder', 'runtime', 'first-run', 'stage', 'relaunch', 'ready', 'rich-voice',
         'new-conversation', 'heard', 'spoken', 'split']
# Rich's voice (CEO 2026-10-14, Pocket TTS). Not in the default run: the two steps
# download 166 MB into the guest and wait for a spoken answer. Ask for them with --steps.
DEFAULT_STEPS = ['identity', 'no-decoder', 'runtime', 'first-run', 'stage', 'relaunch', 'ready', 'heard']
VOICE_NOT_INSTALLED = '[richos] speech synthesis install failed this time'
# The nine pinned files of Rich's voice (pocket.rs FILES). The app installs them in silence, each
# renamed into place only after its size and sha256 match its pin, so all nine present is the
# install's receipt.
VOICE_FOLDER = '/.config/richos/models/pocket-tts-english_2026-04'
VOICE_FILES = ('bos_before_voice.npy', 'bundle.json', 'flow_lm_flow_int8.onnx', 'flow_lm_main_int8.onnx',
               'mimi_decoder_int8.onnx', 'mimi_encoder.onnx', 'peter_yearsley.wav', 'text_conditioner.onnx',
               'tokenizer.model')
# CEO 2026-10-15: Rich speaks only in his own voice, and no log line names or describes it.
# No app log line may name, describe or report loading a voice; only the `tts:` timing figures
# (which name none) remain.
VOICE_LINE_MARKERS = ('macOS say', 'with say', 'Daniel', 'Samantha', 'Reed (English', 'Yearsley', 'peter_yearsley', 'Pocket',
                      "Rich's voice", 'rich voice:', 'tts=', 'first PCM', 'loaded and verified',
                      'installed and verified')


def voice_lines(lines):
    """Every app log line about the voice itself (its name, its state, its loading)."""
    return [l for l in lines if any(marker in l for marker in VOICE_LINE_MARKERS)]
SPARE_READY = '[richos] next conversation ready for '
READY = '[richos] voice: ready on this machine'
NOT_READY = '[richos] voice: not ready on this machine'


def decoder_warm_before_capture(lines):
    for line in lines:
        if 'speech decoder initialized before capture in ' in line:
            return True
        if 'voice session OPEN' in line:
            return False
    return False


def tts_timing(lines, completed):
    """Whole-sentence synthesis and streaming first audio are different measurements; the
    `before synthesis` figure is how long the sentence waited for the voice to be ready."""
    match = re.search(r' (synthesized|streamed) in (\d+) ms', completed)
    if not match:
        raise StepFailed('unrecognized TTS measurement: ' + completed)
    total = int(match.group(2))
    wait = re.search(r'(\d+) ms before synthesis', completed)
    timing = {'complete_synthesis_ms': total, 'wait_before_synthesis_ms': int(wait.group(1)) if wait else None}
    if match.group(1) == 'synthesized':
        return {'first_audio_ms': total, **timing}
    first = re.search(r'first audio in (\d+) ms', completed)
    if not first:
        raise StepFailed('streamed speech has no first-audio measurement: ' + completed)
    return {'first_audio_ms': int(first.group(1)), **timing}


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


_STAMP = re.compile(r'\] (\d\d):(\d\d):(\d\d)\.(\d{3})Z ')


def _stamp_ms(line):
    """The app's own HH:MM:SS.mmmZ stamp on a [richos-voice] line, in ms of the day, or None."""
    m = _STAMP.search(line)
    if not m:
        return None
    h, mi, s, ms = (int(g) for g in m.groups())
    return ((h * 60 + mi) * 60 + s) * 1000 + ms


def talk_timing(lines):
    """From the voice session's own log lines (controller.rs), for the first answer: when his last
    word ended (`utterance END`), when his turn was declared over (`end-of-turn quiet`), when Rich
    became audible (`ANSWER AUDIBLE`), the two gaps, and whether Rich was audible before his last
    word ended (`spoke_before_done`: an utterance line saying `Rich audible=true`, or any utterance
    line after the first ANSWER AUDIBLE; the walk feeds a file as the microphone and mutes the
    guest's output, so speech after that line can only be the rest of his sample). Missing moments
    are None."""
    last_end = turn_end = audible = None
    spoke_before_done = False
    for line in lines:
        if 'utterance' in line and ('Rich audible=true' in line or audible is not None):
            spoke_before_done = True
        if audible is not None:
            continue
        if 'ANSWER AUDIBLE' in line:
            audible = _stamp_ms(line)
        elif 'utterance END' in line:
            last_end = _stamp_ms(line)
        elif 'end-of-turn quiet' in line:
            turn_end = _stamp_ms(line)
    gap = lambda a, b: (b - a) if a is not None and b is not None else None
    return {'last_word_end_ms': last_end, 'turn_end_ms': turn_end, 'audible_ms': audible,
            'turn_end_to_audible_ms': gap(turn_end, audible), 'last_word_to_audible_ms': gap(last_end, audible),
            'spoke_before_done': spoke_before_done}


DAY_MS = 86_400_000


def reply_split(voice_lines, ledger_rows, machinery_rows):
    """Where the time between his turn being handed over and Rich being heard went, for the first
    spoken turn (CEO 2026-10-14: "From barge in to a 10-second delay now?").

    Each moment is in ms of the UTC day, the clock the [richos-voice] lines print; the ledger's and
    the machinery journal's epoch-ms `at` are folded onto it. A moment the sources do not hold is
    None, never guessed.

      turn_handed_over   controller.rs `TURN to Rich`
      prompt_received    the ledger's PromptReceived (source jam): his words are durable
      turn_started       the ledger's TurnStarted: the spine is delivering
      hidden_first/last  the turn's INTERNAL machinery (a context-only priming turn sent before
                         his words: native.rs supply_context_brief), first and last record
      words_sent         `[richos] ... reply timing: a visible turn sent to the model` (builds
                         from 2026-10-14 on; None before)
      model_first_frame  the first non-internal machinery record of the turn: his words reached
                         the model (`system/init` arrives as the prompt is taken)
      first_text         the native client's first-text timing line, before app word buffering;
                         the first AssistantDelta is a fallback for builds without that line
      first_sentence     `ANSWER AUDIBLE`: the first complete sentence went to the voice
      speech_started     the first `playout chunk START` after it: the first sample played"""
    def voice(marker, after=None):
        for line in voice_lines:
            if marker in line:
                at = _stamp_ms(line)
                if at is not None and (after is None or at >= after):
                    return at
        return None

    fold = lambda at: at % DAY_MS if isinstance(at, int) else None
    prompt = next((r for r in ledger_rows if r.get('event') == 'PromptReceived' and r.get('source') == 'jam'
                   and (r.get('text') or '').strip()), None)
    turn = prompt.get('turn_id') if prompt else None
    started = next((r for r in ledger_rows if r.get('event') == 'TurnStarted' and r.get('turn_id') == turn), None)
    first_delta = next((r for r in ledger_rows if r.get('event') == 'AssistantDelta' and r.get('turn_id') == turn), None)
    mine = [r for r in machinery_rows if r.get('turnId') == turn and isinstance(r.get('at'), int)]
    hidden = sorted(r['at'] for r in mine if r.get('internal'))
    shown = sorted(r['at'] for r in mine if not r.get('internal'))
    sentence = voice('ANSWER AUDIBLE')
    words_sent = voice('reply timing: a visible turn sent to the model', after=voice('TURN to Rich'))
    # Context-only turns log their first text too. Only the text after the visible
    # send belongs to this answer; without that marker use the ledger's scoped delta.
    first_text = voice('reply timing: first text from the model', after=words_sent) if words_sent is not None else None
    moments = {
        'turn_handed_over': voice('TURN to Rich'),
        'prompt_received': fold(prompt.get('at')) if prompt else None,
        'turn_started': fold(started.get('at')) if started else None,
        'hidden_first': fold(hidden[0]) if hidden else None,
        'hidden_last': fold(hidden[-1]) if hidden else None,
        'words_sent': words_sent,
        'model_first_frame': fold(shown[0]) if shown else None,
        'first_text': first_text if first_text is not None else fold(first_delta.get('at')) if first_delta else None,
        'first_sentence': sentence,
        'speech_started': voice('playout chunk START', after=sentence) if sentence is not None else None,
    }
    order = list(moments)
    gaps = {}
    for a, b in zip(order, order[1:]):
        gaps[f'{a}->{b}'] = (moments[b] - moments[a]) if moments[a] is not None and moments[b] is not None else None
    # A cached context brief omits hidden_first/last. Account for the time between
    # the surrounding recorded moments, while retaining absent moments as None.
    available = [name for name in order if moments[name] is not None]
    for a, b in zip(available, available[1:]):
        gaps.setdefault(f'{a}->{b}', moments[b] - moments[a])
    start, end = moments['turn_handed_over'], moments['speech_started']
    return {'turn_id': turn, 'moments_ms_of_day': moments, 'gaps_ms': gaps, 'hidden_records': len(hidden),
            'total_ms': (end - start) if start is not None and end is not None else None}


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
                # `nowindow`: the relaunched process exists and has not opened its window yet.
                # relaunch() returns at the pid, not at a window, so the first read can land in
                # that instant (walk-a75c409405c1, 2026-10-14: windows=0 at 7.7 s; the same
                # bundle's first launch had shown its window a minute earlier).
                if not axwait.no_answer(exc) and '"nowindow"' not in str(exc):
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

    def rich_voice(self):
        """Rich's voice reaches this guest the way it reaches a user: the app, finding voice ready,
        downloads the nine pinned Pocket TTS files in the background (voice_provision.rs
        fetch_rich_voice_in_background), in silence; all nine in place is the receipt. Nothing is
        copied in."""
        end = time.monotonic() + self.a.voice_within
        while time.monotonic() < end:
            # First-run may finish the download before relaunch. That relaunch
            # verifies the installed files and skips downloading them again, so
            # its log contains no new installation receipt. Retain the original
            # receipt from this same run; a later fetch or failure supersedes it.
            logs = dict.fromkeys((self.payload + '/app.log', self.log))
            text = guest(self.vm, 'cat ' + ' '.join(shlex.quote(p) for p in logs) + ' 2>/dev/null || true')
            failed = [l for l in text.splitlines() if VOICE_NOT_INSTALLED in l]
            if failed:
                raise StepFailed(failed[-1])
            folder = self.home + VOICE_FOLDER
            names = guest(self.vm, 'ls ' + shlex.quote(folder) + ' 2>/dev/null || true').split()
            if all(name in names for name in VOICE_FILES):
                listing = guest(self.vm, 'ls -l ' + shlex.quote(folder))
                voice = guest(self.vm, 'shasum -a 256 ' + shlex.quote(folder + '/peter_yearsley.wav')).split()[0]
                return {'files': listing.splitlines(), 'peter_yearsley_sha256': voice}
            time.sleep(2)
        raise StepFailed(f'the app did not install Rich\'s voice within {self.a.voice_within} s')

    def spoken(self):
        """Rich answers aloud in his own voice, with no wait for it and no log line about it: the
        first sentence's `tts:` timing line (printed only by RichVoice, tts.rs, which speaks only
        with Rich's voice) shows 0 ms before synthesis, and no line of the app's log names,
        describes or reports loading a voice (VOICE_LINE_MARKERS). Output is muted in the guest."""
        end = time.monotonic() + self.a.within
        lines = []
        while time.monotonic() < end:
            text = guest(self.vm, 'grep -E "\\[richos-voice\\] tts:" ' + shlex.quote(self.log) + ' || true')
            lines = text.splitlines()
            spoken = [l for l in lines if '] tts: ' in l]
            if spoken:
                first = spoken[0]
                synthesis = tts_timing(lines, first)
                time.sleep(5)
                # The whole voice session, kept: every utterance, the end of his turn and the
                # moment Rich became audible, on the app's own clock.
                session = guest(self.vm, 'grep -F "[richos-voice]" ' + shlex.quote(self.log) + ' || true')
                (self.out / 'voice-log.txt').write_text(session)
                lines = session.splitlines()
                said = [l for l in lines if '] tts: ' in l]
                foreign = [l for l in said if 'ms before synthesis' not in l]
                if foreign:
                    raise StepFailed('a sentence was spoken by something other than Rich\'s voice: ' + foreign[0])
                about = voice_lines(guest(self.vm, 'cat ' + shlex.quote(self.log)).splitlines())
                if about:
                    raise StepFailed('the app log has a line about the voice itself: ' + about[0])
                if synthesis['wait_before_synthesis_ms'] != 0:
                    raise StepFailed('Rich\'s first sentence waited for his voice to be ready: ' + first)
                if self.a.expect_warmed_decoder and not decoder_warm_before_capture(lines):
                    raise StepFailed('the first speech decoder initialization was not completed before capture opened')
                timing = talk_timing(lines)
                if timing['spoke_before_done']:
                    raise StepFailed('Rich was audible before his turn was over: ' + json.dumps(timing))
                if timing['audible_ms'] is None:
                    raise StepFailed('Rich synthesized speech but the session never logged ANSWER AUDIBLE: '
                                     + json.dumps(timing))
                return {'first_sentence': first, **synthesis, 'sentences': len(said),
                        'session_open': [l for l in lines if 'voice session OPEN' in l],
                        'lines_about_the_voice': about, 'timing': timing}
            time.sleep(2)
        raise StepFailed(f'Rich spoke no sentence within {self.a.within} s; voice log:\n' + '\n'.join(lines))

    def split(self):
        """After `spoken`: the app log, the ledger and the machinery journal are copied out of the
        guest into OUT/guest/, and reply_split() says where the seconds between his turn and Rich's
        first sample went (reply-split.json)."""
        kept = self.out / 'guest'
        kept.mkdir(parents=True, exist_ok=True)
        for source in (self.log, self.data + '/conversation-ledger.jsonl', self.data + '/machinery'):
            command([HERE / 'guest.sh', self.vm, '--pull', source, str(kept)], 300)
        voice_lines = [l for l in (kept / Path(self.log).name).read_text(errors='replace').splitlines()
                       if '[richos-voice]' in l or 'reply timing:' in l]
        ledger = [json.loads(l) for l in (kept / 'conversation-ledger.jsonl').read_text().splitlines() if l.strip()]
        machinery = []
        for shard in sorted((kept / 'machinery').glob('*/*.jsonl')):
            if not shard.name.endswith('.raw.jsonl'):
                machinery += [json.loads(l) for l in shard.read_text().splitlines() if l.strip()]
        result = reply_split(voice_lines, ledger, machinery)
        (self.out / 'reply-split.json').write_text(json.dumps(result, indent=2) + '\n')
        if result['total_ms'] is None:
            raise StepFailed('the split is missing a moment: ' + json.dumps(result['moments_ms_of_day']))
        return result

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
                if not axwait.ask_again(exc, mode) or attempt == 2:
                    raise

    def by_id(self, dom_id):
        """The node whose DOM id is dom_id, or None when it is not in the accessibility tree."""
        try:
            nodes = self.ax('find', '--id', dom_id, '--first')
        except StepFailed as exc:
            # A read with no answer (axwait.py) is not there yet: the callers loop.
            if axwait.not_yet(exc, 'find'):
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
            if not axwait.no_answer(exc):
                raise
            return None

    def press_to_listening_ms(self, pressed):
        """Milliseconds from `pressed` (guest epoch seconds) to the first `voice session OPEN`
        after it, on the app's own UTC clock. None when no such line arrives within 30 s."""
        end = time.monotonic() + 30
        while time.monotonic() < end:
            text = guest(self.vm, 'grep -F "voice session OPEN" ' + shlex.quote(self.log) + ' || true')
            for line in text.splitlines():
                match = re.search(r'(\d\d):(\d\d):(\d\d)\.(\d\d\d)Z voice session OPEN', line)
                if not match:
                    continue
                h, m, sec, ms = (int(x) for x in match.groups())
                opened = h * 3600000 + m * 60000 + sec * 1000 + ms
                pressed_ms = round((pressed % 86400) * 1000)
                gap = (opened - pressed_ms) % 86400000
                if gap < 600000:
                    return gap
            time.sleep(0.5)
        return None

    def mark_press(self):
        """CEO 2026-10-15: what Talk needs is ready when the app is up, so pressing it listens at
        once. Taken just before the press: whether the speech decoder has already initialized,
        and the guest's clock, against which the app's own `voice session OPEN` stamp (printed
        once capture is running) gives an upper bound on the press-to-listening gap."""
        before = guest(self.vm, 'cat ' + shlex.quote(self.log)).splitlines()
        return {'pressed': self.guest_clock(),
                'decoder_ready': any('speech decoder initialized before capture' in l for l in before)}

    def guest_clock(self):
        """The guest's epoch seconds, to the millisecond (perl ships with macOS)."""
        return float(guest(self.vm, "perl -MTime::HiRes -e 'printf \"%.3f\\n\", Time::HiRes::time()'").strip())

    def talk(self, before_press=None):
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
        marked = before_press() if before_press else None
        self.press_by_id('talk-toggle')  # once; what it did is read below
        clicked = self.guest_clock() if marked else None
        end = time.monotonic() + 30
        while time.monotonic() < end:
            toggle = self.by_id('talk-toggle') or {}
            if toggle.get('desc') == 'Stop talking':
                result = {'home_door_pressed': bool(door), 'talk_control': toggle}
                if marked:
                    # The click is an accessibility round trip into the guest, so the press lands
                    # somewhere inside it. Both bounds: from before the click was sent (upper),
                    # and from when the click call returned (negative: listening began before
                    # the click call had even returned).
                    upper = self.press_to_listening_ms(marked['pressed'])
                    click_ms = round((clicked - marked['pressed']) * 1000)
                    result.update(decoder_ready_before_press=marked['decoder_ready'],
                                  click_round_trip_ms=click_ms,
                                  press_to_listening_ms=upper,
                                  listening_after_click_returned_ms=None if upper is None else upper - click_ms)
                return result
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

    def new_conversation(self):
        """A new conversation adopts the desk primed at this launch, and that desk holds his context
        brief already: between his first message and his words reaching the model, the reply timing
        lines say "already holds this context brief" and never "hidden context ... sent". Waits for
        that first turn to end, so the spoken turn after it is the conversation's second."""
        end = time.monotonic() + 300
        while not any(line.startswith(SPARE_READY) for line in
                      guest(self.vm, 'cat ' + shlex.quote(self.log) + ' 2>/dev/null || true').splitlines()):
            if time.monotonic() > end:
                raise StepFailed('no spare desk was primed within 300 s of the relaunch')
            time.sleep(2)
        end = time.monotonic() + 30
        while not self.by_id('rail-new-thread'):
            if time.monotonic() > end:
                raise StepFailed('the New conversation control (#rail-new-thread) is not on screen within 30 s')
            if self.by_id('home-enter'):
                self.press_by_id('home-enter')
            time.sleep(1)
        self.press_by_id('rail-new-thread')
        self.type_into('What is the capital of France? One word.', '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        ledger = self.data + '/conversation-ledger.jsonl'
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            rows = [json.loads(r) for r in guest(self.vm, 'cat ' + shlex.quote(ledger), 60).splitlines() if r.strip()]
            created = [r for r in rows if r.get('event') == 'ThreadCreated' and r.get('thread_id') != self.facts.get('thread')]
            thread = created[0]['thread_id'] if created else None
            prompt = next((r for r in rows if r.get('event') == 'PromptReceived' and r.get('thread_id') == thread
                           and r.get('source') == 'text'), None)
            turn = prompt and prompt.get('turn_id')
            done = next((r for r in rows if r.get('turn_id') == turn and r.get('event') in
                         ('TurnCompleted', 'TurnInterrupted', 'TurnStopped')), None)
            if done:
                break
            time.sleep(2)
        else:
            raise StepFailed('the first message of a new conversation got no finished turn within %s s' % self.a.within)
        sent, ended = prompt['at'] % DAY_MS, done['at'] % DAY_MS
        log = guest(self.vm, 'grep -F "reply timing:" ' + shlex.quote(self.log) + ' || true').splitlines()
        window = [line for line in log if (_stamp_ms(line) or -1) >= sent and (_stamp_ms(line) or -1) <= ended]
        # Decided by the "already holds" line alone: his turn prints it or runs the hidden turn,
        # once. A "hidden context" line in this window can be the NEXT spare's, primed meanwhile.
        result = {'thread': thread, 'turn_id': turn, 'ended': done['event'], 'reply_timing': window,
                  'brief_already_held': any('already holds this context brief' in l for l in window)}
        (self.out / 'new-conversation.json').write_text(json.dumps(result, indent=2) + '\n')
        if not result['brief_already_held']:
            raise StepFailed('the adopted desk did not hold his context brief: ' + json.dumps(window))
        return result

    def heard(self):
        muted = self.mute_guest_output()
        talk = self.talk(self.mark_press)
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
    p.add_argument('--expect-warmed-decoder', action='store_true', help='require real decoder initialization before voice capture, not on the first user utterance')
    p.add_argument('--within', type=float, default=120, help='seconds for the transcript to be stored')
    p.add_argument('--one-turn', action='store_true',
                   help='require the whole sample to become exactly one spoken turn (a pause inside a sentence)')
    p.add_argument('--settle', type=float, default=15,
                   help='with --one-turn: seconds to wait after the first turn for a second one to appear')
    p.add_argument('--expect-words', default='',
                   help='with --one-turn: comma-separated words that must all be in that one turn')
    p.add_argument('--voice-within', type=float, default=600,
                   help="seconds for the app to download and verify Rich's voice (step rich-voice)")
    p.add_argument('--steps', default=','.join(DEFAULT_STEPS))
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
