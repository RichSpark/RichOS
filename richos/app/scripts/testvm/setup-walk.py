#!/usr/bin/env python3
"""setup-walk.py — the video tools download in the background from launch, and first setup does
not wait for them (CEO 2026-10-07).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      setup-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument.

THE QUESTION. The CEO, 2026-10-07: "the download of the essential files starts right away and
independently of the other stuff. So that by the time the user is done with their intial setup all
those essential files will already have downloaded or nearly finished downloading in the
background." And: "Important is that the user can do all the other stuff for the initial setup
(like typing their company name, selecting a folder that will be associated with that etc) while
the audio tools are seamlessly downloading and installing in the background." The video tools are
yt-dlp and both pinned speech models (media-tools plan §2): small.en for voice and
large-v3-turbo-q5_0 for transcription, 1,061,655,396 B together (`model-pins.json`).

The guest has Claude Code and the engine (run.sh's RICHOS_ENGINE_DIR) and no video tools, so the
first-run questions it asks are the memory question and the company question.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). Every step writes its
moments to ONE timeline on the guest's clock, each with how much of the two models is on disk then:
  identity    the running app says it was built from --expect-sha
  meanwhile   the models start arriving with nothing pressed; then first setup is done as a new
              user does it: the memory question answered "Set it up" and closed, the company name
              typed, its folder given, "Add this company" pressed, the first conversation created.
              PASS when the download started before any press and every one of those moments came
              while it was still running.
              (On a build where the setup sheet lists the video tools, the walk presses its "Set it
              up", which is the only way that build fetches them, and goes on as soon as it can.)
  arrived     the download finishes by itself: the app's own line says the video tools are
              installed; tools/yt-dlp verifies (launcher record = file hash) and answers --version
              on the runtime's Python; BOTH models are on disk with their pinned sha256
  voice       voice is ready with NO relaunch (a4facb643: "the window asks voice again when a model
              arrives ... so voice switches on without a relaunch"): within 60 s the app's own
              "[richos] voice: ready on this machine" line is printed (main.rs voice_readiness
              prints a line on EVERY ask), and the talk control (#talk-toggle), pressed once as a
              person does, turns into "Stop talking" rather than putting up the offer to download
              a model that is already installed (#voice-model-get). Then pressed again to stop.
              The press opens capture, so the app must be launched with a stand-in for the
              microphone: TESTVM_APP_ENV=RICHOS_VOICE_INPUT_WAV=/Users/admin/voice-silence.wav in [machine-path-exempt: isolated guest fixture example; no customer default]
              run-walk.py's environment; the step writes two seconds of silence there first, and
              the microphone grant a person gives with Allow is written as voice-walk.py writes it
              (grant_microphone), so no privacy prompt comes up.
  relaunch    the app is relaunched; the boot line says "nothing missing." and no sheet comes up

ROUND 19, STATES 1 TO 4 (dictation plan revision 2, slice 4), three more steps, chosen with
--steps identity,r19-sheet,r19-offer,r19-again. The ways in are off until slice 5
(richos_core::DICTATION_READY), so these relaunch the app with the walks' preview,
RICHOS_DICTATION_PREVIEW=1, and with RICHOS_ENGINE_DIR naming the folder setup installs the
engine into, which does not exist yet: the guest is then a Mac without the engine, so the sheet
asks for it, and the press installs it there from the build's engine pin (the bundle must be
built with one: a nightly's engine-pin.env).
  r19-sheet   state 1: the sheet up, his line for the voice and video tools read by OCR and by
              the accessibility tree; "Set it up" pressed; state 2: his download line read by
              OCR, two counter readings ("490 MB of 1.06 GB") that increase; Start on the sheet
              before "You're all set." when the engine is in first (recorded either way); state 3:
              "You're all set." with both models on disk
  r19-offer   Start pressed, first setup answered (the memory question "Not now", the company
              added); Rich's offer appears once, its "Not now" says the drawn line, and
              dictation.json says offered
  r19-set-up  instead of r19-offer, the CEO's 2026-10-13 test of nightly 54: Start pressed, the
              company added as --company-name, "Start the questions" pressed; the rail paints the
              name as typed (OCR, case kept), the first conversation is "Onboarding", the offer says
              his copy; Turn on dictation with both grants written as a person gives them, F2 picked
              in the sheet and the sheet closed: a "Dictation feature" conversation naming F2,
              recorded in dictation.json, the dictation text gone from the onboarding conversation,
              no dictation line under the message box, and Rich's reply naming the company exactly
  r19-again   the app relaunched: no setup sheet and no offer

CEO §53: no sound is played. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
Exit 0 when every step passes.
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

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed

STEPS = ['identity', 'meanwhile', 'arrived', 'voice', 'relaunch']
# Round 19's states 1 to 4 (dictation plan slice 4); never in the default run.
R19_STEPS = ['r19-sheet', 'r19-offer', 'r19-set-up', 'r19-again']
QA = HERE.parent / 'qa'
# What OCR must read in each state: fragments of his two lines short enough to sit on one line
# of the sheet (setup.rs MEDIA_TOOLS_WHY, setup_view.rs SETUP_DOWNLOAD_LINE).
R19_STATE1_OCR = ('Wispr Flow', 'voice and video tools')
# Measured on the guest (walk of 76167734c): the sheet breaks his download line after "local voice",
# so "local voice AI" is never one OCR line; "Sit tight" and "Wispr Flow" each are.
R19_STATE2_OCR = ('Sit tight', 'Wispr Flow')
# The accessibility tree carries the exact words.
R19_STATE1_AX = 'a free & private/local replacement for Wispr Flow'
# The CEO's copy in the offer, 2026-10-13 (main.js DICTATION_OFFER_WHAT and _HOW_BEFORE).
R19_OFFER_AX = 'a free & private/local replacement for Wispr Flow. (saves you $140+/year)'
R19_OFFER_COPY_AX = (R19_OFFER_AX, 'Just click into any other app (Mail, Slack, Chrome/Safari, anything), press')
R19_NOT_NOW_AX = "Okay. It's in Settings, under Dictation, whenever you want it."
# The sheet's On line (dictation.js) and the "Dictation feature" message with F2 (dictation_offer.rs).
R19_ON_AX = 'On. Tap F'
R19_FEATURE_AX = 'Click into where you want your words to appear, tap F2, say what you want written'
# The microphone's stand-in for the voice step (capture.rs RICHOS_VOICE_INPUT_WAV), in the guest.
SILENCE_WAV = '/Users/admin/voice-silence.wav' # machine-path-exempt: isolated test VM account/path; never a customer default
# Two seconds of 16 kHz mono silence, written in the guest: nothing is played and nothing is said.
SILENCE = ("import sys,wave\n"
           "w=wave.open(sys.argv[1],'wb')\n"
           "w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000)\n"
           "w.writeframes(b'\\x00\\x00'*32000)\n"
           "w.close()\n")
PINS = HERE.parents[2] / 'engine' / 'voice' / 'models' / 'model-pins.json'
# The model voice fetches on a Mac with none (the cost table's safe rung) and the transcription
# model (stt.rs TRANSCRIPTION_MODEL_ID).
VOICE = 'ggml-small.en.bin'
TRANSCRIPTION = 'ggml-large-v3-turbo-q5_0.bin'
MODELS = (VOICE, TRANSCRIPTION)
# The setup step's name on screen, in every build: "my video tools" and, once dictation is there,
# "my voice and video tools" (setup.rs MEDIA_TOOLS_NAME; the sheet capitalizes it).
TOOLS_ON_SHEET = 'video tools'
# The first-setup moments a user acts in; each must come while the download is still running.
USER_MOMENTS = ('company name typed', 'folder given', '"Add this company" pressed', 'first conversation created')

# One guest call: the guest's clock and every file's size in the models folder, together.
SAMPLE = ("import json,os,sys,time\n"
          "d=sys.argv[1]\n"
          "try:\n    names=os.listdir(d)\nexcept OSError:\n    names=[]\n"
          "f={}\n"
          "for n in names:\n"
          "    try:\n        f[n]=os.path.getsize(os.path.join(d,n))\n    except OSError:\n        pass\n"
          "print(json.dumps({'t':time.time()*1000,'files':f}))\n")


def pinned_bytes():
    pins = {m['file']: m['bytes'] for m in json.loads(PINS.read_text())['models']}
    return {name: pins[name] for name in MODELS}


def download_progress(files, want):
    """How much of the two models is on disk, from one listing of the models folder: finished
    files plus their `.part` partials. `done` is both finished at their pinned size, no partial."""
    got = 0
    for name, size in want.items():
        got += min(files.get(name, 0) or files.get(name + '.part', 0), size)
    done = all(files.get(name) == size for name, size in want.items()) and \
        not any(name + '.part' in files for name in want)
    return {'bytes': got, 'of': sum(want.values()), 'done': done}


def meanwhile_verdict(timeline):
    """PASS (None) when the download was running before anything was pressed and every user moment
    came while it was still running; otherwise the reason."""
    first_press = next((i for i, row in enumerate(timeline) if row.get('pressed')), None)
    started = next((i for i, row in enumerate(timeline) if row['downloaded_bytes'] > 0), None)
    if started is None:
        return 'the models never started arriving during first setup'
    if first_press is not None and started > first_press:
        return 'the models started arriving only after a press: ' + timeline[first_press]['event']
    for name in USER_MOMENTS:
        row = next((r for r in timeline if r['event'] == name), None)
        if row is None:
            return f'first setup never reached "{name}"'
        if row['download_done']:
            return f'"{name}" came only after the download had finished ({row["since_launch_s"]} s)'
    return None


def boot_setup(text):
    """The first-run setup verdict a boot log states (main.rs's boot block): `None` if it has not
    printed one yet, else {'nothing_missing': bool, 'missing': [names]}. The names are the machine
    names the boot line prints (`Component::as_str`: claude-code, engine, media-tools); a build
    before the dictation plan's M8 printed the on-screen names instead."""
    missing = re.findall(r'^\[richos\] first-run setup: (.+?) is NOT installed', text, re.M)
    nothing = bool(re.search(r'^\[richos\] first-run setup: nothing missing\.$', text, re.M))
    if not missing and not nothing:
        return None
    return {'nothing_missing': nothing, 'missing': missing}


def voice_lines(text):
    """The app's voice readiness lines, in order (main.rs voice_readiness prints one per ask)."""
    return re.findall(r'^\[richos\] voice: .*$', text, re.M)


def voice_ready(lines):
    """The ready lines among them."""
    return [line for line in lines if line.startswith('[richos] voice: ready on this machine')]


def tools_outcome(text):
    """How the video tools' download ended, from the app's own lines (setup_view.rs): 'done',
    'failed' or None while it runs. Either source: the background download, or a setup press."""
    if re.search(r'^\[richos\] (?:setup|video tools in the background) \d+/\d+ done: My (?:voice and )?video tools are installed\.$', text, re.M):
        return 'done'
    if re.search(r'^\[richos\] (?:setup|video tools in the background) \d+/\d+ FAILED', text, re.M):
        return 'failed'
    return None


def counter_reading(text):
    """The voice row's counter as round 19 draws it, "490 MB of 1.06 GB" (setup_view.rs
    counter_line): (megabytes in hand, the whole as written), or None for anything else."""
    m = re.fullmatch(r'\s*(\d+) MB of (\d+\.\d\d GB|\d+ MB)\s*', text or '')
    return (int(m.group(1)), m.group(2)) if m else None


def readings_increase(readings):
    """At least two readings of the same whole, each more than the one before."""
    if len(readings) < 2 or len({whole for _, whole in readings}) != 1:
        return False
    return all(b[0] > a[0] for a, b in zip(readings, readings[1:]))


def r19_verdict(facts):
    """Round 19's states 1 to 3 from what r19-sheet recorded: None (PASS) or the reasons."""
    why = []
    if not facts.get('state1_ocr'):
        why.append('OCR did not read his line for the voice and video tools in state 1')
    if not facts.get('state1_ax'):
        why.append('the accessibility tree does not carry his line word for word in state 1')
    if not facts.get('state2_ocr'):
        why.append('OCR did not read his download line in state 2')
    if not readings_increase([tuple(r) for r in facts.get('readings', [])]):
        why.append(f"no two counter readings that increase: {facts.get('readings')}")
    if not facts.get('start'):
        why.append('Start never came up')
    if not facts.get('all_set'):
        why.append('"You\'re all set." never came up')
    elif not facts['all_set'].get('download_done'):
        why.append('"You\'re all set." came up before both models were on disk')
    return '; '.join(why) or None


class SetupWalk(adopt_walk.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.engine = self.payload + '/engine'
        self.log = self.payload + '/app.log'
        self.tools = self.home + '/Library/Application Support/RichOS/tools'
        self.models = self.home + '/.config/richos/models'
        self.want = pinned_bytes()
        self.timeline = self.facts.get('timeline', [])

    def ax(self, mode, *args, app=None, timeout=40):
        """A find that hit ax.sh's guest deadline is asked again, at most twice (voice-walk.py's
        rule). The walk of candidate 44 lost its relaunch step to one such read on a host at
        97% CPU, after every product check had passed."""
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
            if 'notfound' in str(exc) or 'nothing matched' in str(exc) or 'guest_deadline' in str(exc):
                return None
            raise
        nodes = [n for n in nodes if not n.get('meta')]
        return nodes[0] if nodes else None

    def shows_text(self, text):
        """Static text on the guest's screen: WebKit puts a text run's words in AXValue."""
        try:
            return bool(self.ax('find', '--value', text, '--contains', '--first'))
        except StepFailed as exc:
            if 'notfound' in str(exc) or 'nothing matched' in str(exc):
                return False
            raise

    def read_log(self, path=None):
        return guest(self.vm, 'cat ' + shlex.quote(path or self.log) + ' 2>/dev/null || true', 60)

    def wait_boot_setup(self, path, seconds=90):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            verdict = boot_setup(self.read_log(path))
            if verdict:
                return verdict
            time.sleep(1)
        raise StepFailed(f'the app printed no first-run setup verdict within {seconds} s ({path})')

    # --- the one clock ------------------------------------------------------------------------
    def launched_ms(self):
        if 'launched_ms' not in self.facts:
            # run.sh starts the app with its output redirected into app.log, so the log's birth is
            # the launch, on the guest's clock.
            out = guest(self.vm, "python3 -c 'import os,sys; print(os.stat(sys.argv[1]).st_birthtime*1000)' "
                        + shlex.quote(self.log))
            self.facts['launched_ms'] = float(out)
        return self.facts['launched_ms']

    def sample(self):
        s = json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(SAMPLE) + ' ' + shlex.quote(self.models)))
        return s['t'], download_progress(s['files'], self.want)

    def mark(self, event, pressed=False):
        t, progress = self.sample()
        row = {'event': event, 'since_launch_s': round((t - self.launched_ms()) / 1000, 1),
               'downloaded_bytes': progress['bytes'],
               'downloaded_pct': round(100 * progress['bytes'] / progress['of'], 1),
               'download_done': progress['done'], 'pressed': pressed}
        self.timeline.append(row)
        self.facts['timeline'] = self.timeline
        self.save()
        print(f"  {row['since_launch_s']:>7} s  {row['downloaded_pct']:>5}%  "
              f"{'done' if row['download_done'] else 'running':7}  {event}", flush=True)
        return row

    def click(self, title, event):
        self.press(title)
        return self.mark(event, pressed=True)

    # --- steps --------------------------------------------------------------------------------
    def meanwhile(self):
        verdict = self.wait_boot_setup(self.log)
        _, before = self.sample()
        self.mark('walk begins')
        # 1. THE DOWNLOAD STARTS BY ITSELF: model bytes on disk with nothing pressed.
        end = time.monotonic() + 60
        while time.monotonic() < end:
            _, progress = self.sample()
            if progress['bytes'] > 0:
                self.mark('models arriving, nothing pressed')
                break
            if self.present('Set it up') and self.shows_text(TOOLS_ON_SHEET):
                self.mark('the setup sheet lists the video tools, nothing arriving')
                break
            time.sleep(1)
        else:
            self.mark('nothing arriving 60 s into the walk, nothing pressed')
        self.shot('first-screen.png')

        # 2. FIRST SETUP, AS A NEW USER DOES IT, whatever comes up, until the company question.
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            if self.present('Add this company'):
                break
            if self.present('Set it up') and self.shows_text(TOOLS_ON_SHEET):
                self.click('Set it up', 'setup sheet: "Set it up" pressed (it lists the video tools)')
            elif self.present('Set it up') and self.shows_text('Where should I keep what you tell me?'):
                if self.a.memory == 'set-up':
                    self.click('Set it up', 'memory question: "Set it up" pressed')
                else:
                    self.click('Not now', 'memory question: "Not now" pressed')
            elif self.present('Close') and (self.shows_text('Setup is done.') or self.shows_text("That's set up.")
                                            or self.shows_text('Your memory folder.')
                                            or self.shows_text('Your Anthropic account')):
                self.click('Close', '"Close" pressed')
            else:
                time.sleep(1)
                continue
            time.sleep(2)
        else:
            raise StepFailed(f'the company question never came up within {self.a.within} s: '
                             + json.dumps(self.timeline)[-1500:])
        self.mark('company question up')
        self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
        self.mark('company name typed', pressed=True)
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.mark('folder given', pressed=True)
        self.click('Add this company', '"Add this company" pressed')
        end = time.monotonic() + 60
        created = None
        while time.monotonic() < end and not created:
            rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true')
            created = next((json.loads(r) for r in rows.splitlines() if '"ThreadCreated"' in r), None)
            if not created:
                time.sleep(1)
        if not created:
            raise StepFailed('no conversation was created after the company was added')
        self.mark('first conversation created')
        if self.present('Start the questions'):
            self.click('Not now', 'business questions: "Not now"')
        self.shot('first-conversation.png')
        why = meanwhile_verdict(self.timeline)
        if why:
            raise StepFailed(why)
        return {'boot': verdict, 'models_before': before, 'thread': created.get('thread_id'),
                'timeline': self.timeline}

    def arrived(self):
        end = time.monotonic() + self.a.within
        outcome = None
        while time.monotonic() < end:
            outcome = tools_outcome(self.read_log())
            if outcome:
                break
            time.sleep(5)
        lines = [line for line in self.read_log().splitlines()
                 if (line.startswith('[richos] setup') or line.startswith('[richos] video tools')
                     or 'yt-dlp' in line or 'voice model' in line) and '%' not in line]
        self.facts['tools_lines'] = lines[-40:]
        self.save()
        if outcome != 'done':
            raise StepFailed(f'the video tools did not finish within {self.a.within} s ({outcome}): '
                             + json.dumps(lines[-20:])[:2000])
        self.mark('the app says the video tools are installed')
        launcher = guest(self.vm, 'cat ' + shlex.quote(self.tools + '/yt-dlp'))
        record = re.search(r'^# yt-dlp nightly (\S+) sha256 ([0-9a-f]{64})$', launcher, re.M)
        if not record:
            raise StepFailed(f'tools/yt-dlp is not the launcher RichOS writes: {launcher!r}')
        tag, sha = record.groups()
        version_file = f'{self.tools}/yt-dlp-{sha[:12]}'
        got = guest(self.vm, 'shasum -a 256 ' + shlex.quote(version_file), 60).split()[0]
        if got != sha:
            raise StepFailed(f'yt-dlp {version_file} hashes to {got}, its record says {sha}')
        path = f'{self.engine}/runtime/bin:/usr/bin:/bin:/usr/sbin:/sbin'
        version = guest(self.vm, f'env -i HOME={shlex.quote(self.home)} PATH={shlex.quote(path)} '
                                 + shlex.quote(self.tools + '/yt-dlp') + ' --version', 60)
        pins = {m['file']: m for m in json.loads(PINS.read_text())['models']}
        models = guest(self.vm, 'ls ' + shlex.quote(self.models)).split()
        found = [name for name in models if name in pins]
        if sorted(found) != sorted(MODELS):
            raise StepFailed(f'expected {MODELS} in {self.models}, found {models}')
        verified = {}
        for name in found:
            model_sha = guest(self.vm, 'shasum -a 256 ' + shlex.quote(f'{self.models}/{name}'), 300).split()[0]
            if model_sha != pins[name]['sha256']:
                raise StepFailed(f'{name} hashes to {model_sha}, the pin says {pins[name]["sha256"]}')
            verified[name] = {'sha256': model_sha, 'bytes': pins[name]['bytes']}
        self.mark('video tools installed and verified (yt-dlp and both models against their pins)')
        voice = [line for line in self.read_log().splitlines() if line.startswith('[richos] voice: ')]
        return {'yt_dlp': {'tag': tag, 'sha256': sha, 'version': version}, 'models': verified,
                'voice_lines': voice[-3:], 'timeline': self.timeline}

    def voice(self):
        ps = guest(self.vm, 'ps -axwwE -o command= 2>/dev/null || true', 60)
        app = [line for line in ps.splitlines() if '/Contents/MacOS/richos-tauri' in line]
        if not any('RICHOS_VOICE_INPUT_WAV=' + SILENCE_WAV in line for line in app):
            raise StepFailed('the app was not launched with the microphone stand-in, so pressing the talk '
                             'control would open a real device: run with TESTVM_APP_ENV='
                             f'RICHOS_VOICE_INPUT_WAV={SILENCE_WAV}')
        guest(self.vm, 'python3 -c ' + shlex.quote(SILENCE) + ' ' + shlex.quote(SILENCE_WAV))
        _vspec = importlib.util.spec_from_file_location('voice_walk', HERE / 'voice-walk.py')
        voice_walk = importlib.util.module_from_spec(_vspec)
        _vspec.loader.exec_module(voice_walk)
        voice_walk.VoiceWalk.grant_microphone(self)
        # 1. THE WINDOW ASKED VOICE AGAIN when the model arrived: every ask prints a line.
        end = time.monotonic() + 60
        lines = []
        while time.monotonic() < end:
            lines = voice_lines(self.read_log())
            if voice_ready(lines):
                break
            time.sleep(3)
        ready = voice_ready(lines)
        self.mark('voice ready line printed' if ready else 'no voice ready line 60 s after the step began')
        # 2. WHAT A PERSON SEES: the talk control, pressed once.
        toggle = self.by_id('talk-toggle')
        if not toggle:
            self.shot('voice-no-talk-control.png')
            raise StepFailed('the talk control (#talk-toggle) is not on screen; voice lines: ' + json.dumps(lines))
        self.ax('click', '--id', 'talk-toggle')
        self.mark('talk control pressed', pressed=True)
        listening, offer = False, None
        # 90 s: the control turns only after start_voice_capture resolves, and the walk of candidate
        # 44 saw nothing within 30 s with both guest slots busy on a host at 98% CPU.
        end = time.monotonic() + 90
        while time.monotonic() < end:
            after = self.by_id('talk-toggle') or {}
            if after.get('desc') == 'Stop talking':
                listening = True
                break
            offer = self.by_id('voice-model-get')
            if offer:
                break
            time.sleep(1)
        self.shot('voice-pressed.png')
        offer_text = None
        if offer:
            try:
                found = self.ax('find', '--id', 'voice-model-offer-label', '--first')
                offer_text = next((n.get('value') or n.get('title') for n in found if not n.get('meta')), None)
            except StepFailed:
                pass
        self.mark('listening ("Stop talking")' if listening else
                  ('the download offer is up' if offer else 'neither listening nor the offer within 90 s'))
        capture_log = [line for line in self.read_log().splitlines()
                       if re.search(r'voice|capture|wav|microphone|whisper', line, re.I)][-25:]
        # Back to the composer: the same control again.
        self.ax('click', '--id', 'talk-toggle')
        evidence = {'voice_lines': lines, 'ready_lines': ready, 'talk_before': toggle, 'listening': listening,
                    'offer': offer, 'offer_text': offer_text, 'capture_log': capture_log}
        self.facts['voice'] = evidence
        self.save()
        failures = []
        if not ready:
            failures.append('the app never printed "voice: ready" after the models arrived, so the window '
                            'did not ask voice again')
        if not listening:
            failures.append('pressing the talk control put up the offer to download the speech model '
                            f'({offer_text!r}) although it is installed and verified' if offer
                            else 'pressing the talk control neither started listening nor offered anything')
        if failures:
            raise StepFailed('; '.join(failures) + ' — ' + json.dumps(evidence)[:1500])
        return evidence

    def relaunch(self):
        launched = relaunch(self.vm)
        self.facts['relaunch'] = launched
        self.save()
        verdict = self.wait_boot_setup(launched['log'])
        if not verdict['nothing_missing'] or verdict['missing']:
            raise StepFailed(f'after setup the next launch still finds something missing: {verdict}')
        # The window settles before it would ask; then the sheet must not be there.
        time.sleep(15)
        asked = self.present('Set it up') or self.shows_text(TOOLS_ON_SHEET)
        self.shot('relaunch.png')
        if asked:
            raise StepFailed('the relaunched app put the setup sheet up again')
        return {'boot': verdict, 'sheet_shown': False, 'log': launched['log']}

    # --- round 19, states 1 to 4 (dictation plan slice 4) --------------------------------------
    def r19_env(self):
        """The relaunch environment: the walks' preview of the dictation ways in, and the engine
        named at the folder setup installs it into (`setup::engine_install_dir`), so the guest is
        a Mac without one until the press puts it there."""
        return {'RICHOS_DICTATION_PREVIEW': '1',
                'RICHOS_ENGINE_DIR': self.home + '/Library/Application Support/RichOS/engine'}

    def frame_says(self, name, text):
        """True when OCR reads `text` in the frame `name` (qa/ocr-find.sh: 0 hit, 1 none)."""
        r = subprocess.run([str(QA / 'ocr-find.sh'), text, str(self.out / name), '--first', '--quiet'],
                           capture_output=True, text=True, timeout=120)
        if r.returncode not in (0, 1):
            raise StepFailed('ocr-find could not read the frame: ' + r.stderr)
        return r.returncode == 0

    def shown(self, dom_id, title):
        """The control `dom_id` is on screen as `title`. The DOM id alone is not enough: on the
        guest (2026-10-08) a find for the hidden #setup-start answered with a titleless 10 px
        button, so the walk recorded a Start that was not drawn."""
        node = self.by_id(dom_id)
        return bool(node) and title in (node.get('title') or node.get('desc') or '')

    def counter(self):
        """The voice row's counter now, read from the accessibility tree, or None."""
        try:
            nodes = self.ax('find', '--value', ' MB of ', '--contains', '--first')
        except StepFailed as exc:
            if 'notfound' in str(exc) or 'nothing matched' in str(exc):
                return None
            raise
        for n in nodes:
            reading = counter_reading(n.get('value') or n.get('title') or '')
            if reading:
                return reading
        return None

    def r19_sheet(self):
        facts = {}
        launched = relaunch(self.vm, environment=self.r19_env())
        self.log = launched['log']
        self.facts['r19_launch'] = launched
        self.save()
        verdict = self.wait_boot_setup(self.log)
        facts['boot'] = verdict
        end = time.monotonic() + 120
        while time.monotonic() < end and not (self.present('Set it up')
                                             and self.shows_text("There's a bit of setting up to do first.")):
            time.sleep(1)
        self.mark('round 19, state 1: the setup sheet is up')
        self.shot('r19-state1.png')
        facts['state1_ocr'] = all(self.frame_says('r19-state1.png', t) for t in R19_STATE1_OCR)
        facts['state1_ax'] = self.shows_text(R19_STATE1_AX)
        self.click('Set it up', 'round 19: "Set it up" pressed')
        readings, end = [], time.monotonic() + self.a.within
        while time.monotonic() < end:
            # A press that failed says so at once rather than after the whole wait (the first walk,
            # 2026-10-08, waited out its window on an engine download refused in the first second).
            if self.present('Try again'):
                self.shot('r19-press-failed.png')
                failed = [ln for ln in self.read_log().splitlines() if 'FAILED' in ln][-3:]
                raise StepFailed('the press failed: ' + json.dumps(failed))
            reading = self.counter()
            if reading and (not readings or reading != readings[-1]):
                readings.append(reading)
                self.mark(f'counter reads {reading[0]} MB of {reading[1]}')
                if len(readings) == 1:
                    self.shot('r19-state2.png')
                    facts['state2_ocr'] = all(self.frame_says('r19-state2.png', t) for t in R19_STATE2_OCR)
            if 'start' not in facts and self.shown('setup-start', 'Start'):
                all_set = self.shows_text("You're all set.")
                row = self.mark('Start is on the sheet' + (' with "You\'re all set."' if all_set
                                                           else ' while the voice row still counts'))
                facts['start'] = {'with_all_set': all_set, 'reading': list(readings[-1]) if readings else None,
                                  'since_launch_s': row['since_launch_s']}
                self.shot('r19-start.png')
            if self.shows_text("You're all set."):
                row = self.mark('round 19, state 3: "You\'re all set."')
                facts['all_set'] = {'download_done': row['download_done'], 'since_launch_s': row['since_launch_s']}
                self.shot('r19-state3.png')
                break
            time.sleep(2)
        facts['readings'] = [list(r) for r in readings]
        # Whether the engine was in first, so Start came up while the voice row still counted.
        facts['start_before_all_set'] = bool(facts.get('start')) and not facts['start']['with_all_set']
        self.facts['r19_sheet'] = facts
        self.save()
        why = r19_verdict(facts)
        if why:
            raise StepFailed(why + ' — ' + json.dumps(facts)[:1500])
        return facts

    def r19_offer(self):
        self.ax('click', '--id', 'setup-start')
        self.mark('round 19: "Start" pressed', pressed=True)
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Acme notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        # FIRST SETUP'S OTHER QUESTIONS, one at a time and by their own controls, before the offer
        # is looked for: it is drawn in the conversation, which they cover.
        end = time.monotonic() + 240
        added = False
        while time.monotonic() < end:
            if self.shown('memory-setup-later', 'Not now'):
                self.ax('click', '--id', 'memory-setup-later')
                self.mark('memory question: "Not now" pressed', pressed=True)
            elif not added and self.present('Add this company'):
                self.type_into('Acme', '--role', 'AXTextField', '--title', "What's the company called?")
                self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
                self.click('Add this company', '"Add this company" pressed')
                added = True
            elif self.present('Start the questions'):
                self.click('Not now', 'business questions: "Not now"')
            elif added and self.shown('dictation-offer-yes', 'Turn on dictation'):
                break
            time.sleep(2)
        if not self.shown('dictation-offer-yes', 'Turn on dictation'):
            self.shot('r19-no-offer.png')
            raise StepFailed('Rich never offered dictation after Start: '
                             + json.dumps([ln for ln in self.read_log().splitlines() if 'voice' in ln][-5:]))
        self.mark('round 19, state 4: Rich offers dictation')
        self.shot('r19-offer.png')
        offers = [n for n in self.ax('find', '--title', 'Turn on dictation', '--role', 'AXButton', '--contains')
                  if not n.get('meta')]
        said = self.shows_text(R19_OFFER_AX)
        self.ax('click', '--id', 'dictation-offer-no')
        time.sleep(2)
        after = self.shows_text(R19_NOT_NOW_AX)
        self.shot('r19-offer-not-now.png')
        record = guest(self.vm, 'cat ' + shlex.quote(self.data + '/dictation.json') + ' 2>/dev/null || true')
        try:
            offered = json.loads(record).get('offered') is True
        except ValueError:
            offered = False
        facts = {'offers': len(offers), 'offer_words': said, 'not_now_line': after,
                 'dictation_json': record.strip(), 'offered': offered}
        self.facts['r19_offer'] = facts
        self.save()
        why = [w for ok, w in ((len(offers) == 1, f'{len(offers)} offers on screen'),
                               (said, 'the offer does not say the drawn words'),
                               (after, '"Not now" did not say the drawn line'),
                               (offered, 'dictation.json does not record the offer')) if not ok]
        if why:
            raise StepFailed('; '.join(why) + ' — ' + json.dumps(facts))
        return facts

    def r19_set_up(self):
        """The CEO's test of nightly 54 (richos-hq docs/ceo-input/2026-10-13_1), on this fresh
        install: the company he adds keeps its capitals in the rail and in Rich's words, the first
        conversation is "Onboarding", the offer says his copy, and once dictation is set up with
        the key he picks (F2) its text is in a new "Dictation feature" conversation naming F2, with
        no dictation line under the message box."""
        name = self.a.company_name
        self.ax('click', '--id', 'setup-start')
        self.mark('round 19: "Start" pressed', pressed=True)
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.company)))
        end = time.monotonic() + 240
        added = asked = False
        while time.monotonic() < end:
            if self.shown('memory-setup-later', 'Not now'):
                self.ax('click', '--id', 'memory-setup-later')
            elif not added and self.present('Add this company'):
                self.type_into(name, '--role', 'AXTextField', '--title', "What's the company called?")
                self.type_into(self.company, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
                self.click('Add this company', '"Add this company" pressed')
                added = True
            elif not asked and self.present('Start the questions'):
                self.click('Start the questions', 'business questions: "Start the questions"')
                asked = True
            elif added and asked and self.shown('dictation-offer-yes', 'Turn on dictation'):
                break
            elif self.present('Allow action'):
                # Rich reading his company notes asks first in the guest (seen 2026-10-13).
                self.click('Allow action', 'Rich asked to read the company notes: "Allow action"')
            time.sleep(2)
        if not self.shown('dictation-offer-yes', 'Turn on dictation'):
            self.shot('set-up-no-offer.png')
            raise StepFailed('Rich never offered dictation after Start')
        facts = {'company_name': name}
        facts['onboarding_title'] = self.present('Onboarding', role='AXButton')
        facts['offer_copy'] = all(self.shows_text(t) for t in R19_OFFER_COPY_AX)
        self.shot('set-up-offer.png')
        # The two grants a person gives when he turns dictation on (dictation-walk.py `stage`).
        user_db = '"$HOME/Library/Application Support/com.apple.TCC/TCC.db"'
        sys_db = '"/Library/Application Support/com.apple.TCC/TCC.db"'
        row = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, "
               "indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) "
               "VALUES ('{svc}', 'com.richos.app', 0, 2, 2, 1, 0, 'UNUSED', 0, strftime('%s','now'));")
        guest(self.vm, f'sqlite3 {user_db} ' + shlex.quote(row.format(svc='kTCCServiceMicrophone')))
        guest(self.vm, f'sudo -n sqlite3 {sys_db} ' + shlex.quote(row.format(svc='kTCCServiceAccessibility')))
        self.ax('click', '--id', 'dictation-offer-yes')
        self.mark('"Turn on dictation" pressed', pressed=True)
        end = time.monotonic() + 120
        while time.monotonic() < end and not self.shows_text(R19_ON_AX):
            time.sleep(2)
        facts['sheet_on'] = self.shows_text(R19_ON_AX)
        self.shot('set-up-sheet-on.png')
        self.facts['r19_set_up'] = facts
        self.save()
        # Scoped to the sheet: a whole-window search for the key ran past ax.sh's 20 s deadline
        # in the guest (walk of 2047c2a73, 18:05Z).
        try:
            self.ax('click', '--title', 'F2', '--role', 'AXRadioButton', '--in', 'dialog')
            facts['key_picked_by'] = 'the F2 button in the key strip'
        except StepFailed:
            # The other way a person picks it: "Press a different key", then F2 (key code 120).
            self.ax('click', '--id', 'dict-capture')
            time.sleep(1)
            guest(self.vm, "osascript -e 'tell application \"System Events\" to key code 120'")
            facts['key_picked_by'] = '"Press a different key", then F2'
        time.sleep(2)
        record = guest(self.vm, 'cat ' + shlex.quote(self.data + '/dictation.json') + ' 2>/dev/null || true')
        try:
            facts['key_saved'] = json.loads(record).get('key')
        except ValueError:
            facts['key_saved'] = None
        time.sleep(2)
        self.ax('click', '--id', 'dict-close')
        self.mark('key F2 picked, the sheet closed', pressed=True)
        end = time.monotonic() + 60
        while time.monotonic() < end and not self.present('Dictation feature'):
            time.sleep(2)
        facts['feature_listed'] = self.present('Dictation feature')
        facts['moved_out'] = not any(self.shows_text(t) for t in R19_OFFER_COPY_AX)
        facts['no_composer_line'] = not self.shows_text('here or in any app')
        self.shot('set-up-onboarding.png')
        ocr = adopt_walk.command([HERE / 'shot.sh', self.vm, self.out / 'set-up-rail.png', '--ocr'], 120)
        facts['rail_ocr_exact'] = name in ocr
        facts['rail_ocr_upper'] = name.upper() in ocr
        if facts['feature_listed']:
            self.press('Dictation feature')
            time.sleep(3)
        facts['feature_text'] = self.shows_text(R19_FEATURE_AX)
        facts['feature_wrong_key'] = self.shows_text('tap F1,')
        self.shot('set-up-feature.png')
        record = guest(self.vm, 'cat ' + shlex.quote(self.data + '/dictation.json') + ' 2>/dev/null || true')
        try:
            facts['feature_thread_recorded'] = bool(json.loads(record).get('featureThread'))
        except ValueError:
            facts['feature_thread_recorded'] = False
        # Rich's words in the onboarding conversation: the name as typed, never re-capitalized.
        end = time.monotonic() + 180
        said = ''
        while time.monotonic() < end:
            rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true')
            said = ''.join(json.loads(r).get('text', '') for r in rows.splitlines() if '"AssistantDelta"' in r)
            if name in said or name.title() in said or name.upper() in said:
                break
            if self.present('Allow action'):
                self.click('Allow action', 'Rich asked for an action: "Allow action"')
            time.sleep(5)
        facts['rich_exact'] = said.count(name)
        facts['rich_recapitalized'] = said.count(name.title()) + (said.count(name.upper()) if name.upper() != name else 0)
        (self.out / 'rich-said.txt').write_text(said)
        self.facts['r19_set_up'] = facts
        self.save()
        why = [k for k in ('onboarding_title', 'offer_copy', 'sheet_on', 'feature_listed', 'moved_out',
                           'no_composer_line', 'rail_ocr_exact', 'feature_text', 'feature_thread_recorded')
               if not facts[k]]
        why += [k for k in ('rail_ocr_upper', 'feature_wrong_key', 'rich_recapitalized') if facts[k]]
        if facts['key_saved'] != 2:
            why.append(f"key_saved ({facts['key_saved']}, not F2)")
        if not facts['rich_exact']:
            why.append('rich_exact (Rich never wrote the name)')
        if why:
            raise StepFailed('failed: ' + ', '.join(why) + ' — ' + json.dumps(facts))
        return facts

    def r19_again(self):
        launched = relaunch(self.vm, environment=self.r19_env())
        self.log = launched['log']
        verdict = self.wait_boot_setup(self.log)
        # The window settles; then neither the sheet nor the offer may be there.
        time.sleep(20)
        offered = self.present('Turn on dictation') or self.shows_text(R19_OFFER_AX)
        sheet = self.present('Set it up')
        self.shot('r19-again.png')
        facts = {'boot': verdict, 'offer_shown': offered, 'sheet_shown': sheet, 'log': launched['log']}
        self.facts['r19_again'] = facts
        self.save()
        if offered or sheet or not verdict['nothing_missing']:
            raise StepFailed('after the relaunch: ' + json.dumps(facts))
        return facts


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=2700, help='seconds for first setup, and for the download (two models, about 1 GB)')
    p.add_argument('--memory', choices=('set-up', 'not-now'), default='set-up',
                   help='the answer to the memory question. Setting it up takes about 30 s in the guest, and the '
                        "guest's network fetches the 1 GB in about 63 s (run of 2026-10-07 18:38Z), so not-now "
                        'reaches the company question while the models are still arriving')
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--company-name', default='Brightwave LABS',
                   help='the name r19-set-up types for the company, with the capitals it must keep')
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS + R19_STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = SetupWalk(a)
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
        report['timeline'] = walk.timeline
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
