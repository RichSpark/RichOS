#!/usr/bin/env python3
"""dictation-walk.py — tap F1, speak, tap again: the words appear at the cursor in any app (dictation plan slice 1).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      dictation-walk.py --out DIR --expect-sha SHA --wav SPOKEN.wav --model GGML.bin \\
          --post-key POST_KEY --key-probe KEY_PROBE [--model-id small.en] [--idle 600]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION. The CEO, 2026-10-07, on the dictate-anywhere he uses today through open-wispr:
"That needs to become part of RichOS after this upcoming nightly gets published." Slice 1 of
richos-hq/docs/plans/2026-10-08-dictation-anywhere.md (revision 2, section 9) is the dictation
tool and engine with no new screens, turned on through a test switch.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). The app is started with
`open -n -a` (relaunch.py) and the tool is the app's child, never a binary run from ssh, so no
grant is inherited from sshd-session (Frank's minor 8). No sound is played anywhere (CEO §53):
the spoken sample is made with `say -o`, which writes a file, and RICHOS_VOICE_INPUT_WAV puts it
through the identical capture path (capture.rs).
  identity     the running app says it was built from --expect-sha
  apps         the guest's applications listed; Google Chrome installed when no Chromium or
               Electron app is there
  stage        the guest's TCC rows for com.richos.app: Microphone and Accessibility only, and no
               Input Monitoring row; the sample and the speech model copied in
  relaunch     the app relaunched with the sample as its microphone and the test switch on; the
               tool starts as its child, holds the key tap for F1 and the app connects
  settle       the relaunched app's voice readiness has finished timing its decodes, and the
               sample is decoded once with each speech model on the guest (the expected words,
               which also leaves each model warm): walk-770929407d2d's first dictation ran beside
               that calibration on a cold model and passed its 64 s bound
  probe-on     key_probe (a listen-only tap downstream of every session tap, standing in for
               open-wispr's passive observer) started in the guest
  textedit     three dictations into one empty TextEdit document, by key code 122 (F1), key code
               105 (F13) and the Apple top row's brightness-down event (post_key): each is pasted,
               the spacing rule joins them, the clipboard holds what it held before, and
               `lsappinfo front` sampled every 250 ms names TextEdit throughout
  terminal     one dictation into an empty Terminal prompt
  safari       one dictation into an empty text box in Safari
  chromium     one dictation into an empty text box in the Chromium or Electron app
  probe-check  key_probe saw no F1, F13 or brightness-down while dictation was on
  second-tool  a second tool started by hand exits at once (exit 3), and the first still owns F1
  idle         the tool left idle with no window for --idle seconds, then one dictation, and
               dictation.log has no timeout-disable line
  scratch      the tool's scratch folder is empty; the tool's memory (ps -o rss) is reported
  ends-with-app the app quit by its own pid: the tool is gone within a second
  probe-off    the app relaunched with the test switch off: no tool runs, and key_probe sees each
               of F1, F13 and brightness-down

The words a dictation should produce are this guest's own decode of the sample with the model
that dictation's log line names (the engine runtime's whisper-cli, the same flags as stt.rs
decode_args(None), annotations removed). Per dictation and not once: the app's background
setup download can finish fetching More accurate mid-walk, and from then on the tool uses it
instead of the Faster model this walk copies in (dictation.rs pick_model).

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
import wave

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed
command = adopt_walk.command

STEPS = ['identity', 'apps', 'stage', 'relaunch', 'settle', 'probe-on', 'textedit', 'terminal', 'safari', 'chromium',
         'probe-check', 'second-tool', 'idle', 'scratch', 'ends-with-app', 'probe-off']

# stt.rs decode_args(None), word for word: the expected words come from the same settings.
DECODE_ARGS = ['-l', 'en', '-t', '4', '-fa', '-np', '-nt', '-mc', '0']
CHROMIUM_APPS = ['Google Chrome.app', 'Chromium.app', 'Microsoft Edge.app', 'Brave Browser.app', 'Slack.app',
                 'Visual Studio Code.app']
CHROME_DMG = 'https://dl.google.com/chrome/mac/universal/stable/GGRO/googlechrome.dmg'
TOOL_ARG = '--richos-dictation'
# Every line dictation.log writes when a dictation ends, whatever the outcome (tool.rs write).
OUTCOMES = ('dictation: model ', 'nothing written', 'did not write the words', 'could not be put in place',
            'could not be resolved', 'words copied')
NO_TAP_EVENTS = ('"type":"key"', '"type":"system"')
# capture.rs's line when an injected WAV's last frame has gone downstream.
SAMPLE_ENDED = 'INJECTED INPUT ended'
# How long one dictation may take to write, from the second tap to its log line: More accurate
# took 46.2 s for 3.04 s of audio on a guest whose host was busy (walk-15a88cf29d54).
DICTATION_WITHIN = 240
# Terminal's text, read through System Events (sshd-session's own grant), never by asking
# Terminal itself for it: that needs an automation grant the guest does not give, and its prompt
# would sit on the screen.
TERMINAL_TEXT = '''tell application "System Events" to tell process "Terminal"
  repeat with e in (entire contents of front window)
    try
      if role of e is "AXTextArea" then return value of e
    end try
  end repeat
end tell
return ""'''
CHROME_FLAGS = ['--no-first-run', '--no-default-browser-check', '--use-mock-keychain', '--disable-sync']


def strip_annotations(text):
    """stt.rs strip_annotations: bracketed and parenthesized spans removed, then whitespace."""
    out, sq, par = [], 0, 0
    for c in text:
        if c == '[':
            sq += 1
        elif c == ']':
            sq = max(0, sq - 1)
        elif c == '(':
            par += 1
        elif c == ')':
            par = max(0, par - 1)
        elif sq == 0 and par == 0:
            out.append(c)
    return ' '.join(''.join(out).split())


def words_of(text):
    return re.findall(r"[a-z0-9']+", text.lower())


def answer_of(output):
    """ax.sh's by-hand answer without the phase lines it frames it with."""
    return '\n'.join(line for line in output.splitlines() if not line.startswith('{"ax_')).strip()


def wav_seconds(path):
    with wave.open(str(path), 'rb') as w:
        return w.getnframes() / float(w.getframerate())


class DictationWalk(adopt_walk.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.engine = self.payload + '/engine'
        self.wav = self.home + '/dictation-sample.wav'
        self.log = self.facts.get('log', self.payload + '/app.log')
        self.dlog = self.data + '/dictation.log'
        self.seconds = wav_seconds(a.wav)
        self.page = self.home + '/dictation-box.html'
        self.post_key = self.payload + '/post_key'
        self.key_probe = self.payload + '/key_probe'

    # --- helpers ------------------------------------------------------------------------------
    def osa(self, script, timeout=40):
        """AppleScript through System Events in the guest (ax.sh's by-hand mode). Every grant it
        needs is sshd-session's (provision-guest.sh): it never asks another app for automation.
        ax.sh frames its answer with its own phase lines ({"ax_phase": ...}); they are not the
        answer (walk-15a88cf29d54 read them as the clipboard)."""
        return answer_of(command([HERE / 'ax.sh', self.vm, script], timeout))

    def samples_delivered(self):
        """How many times the injected sample has been delivered to the end (capture.rs says so
        once per capture, on the app's log, which the tool shares as the app's child)."""
        out = guest(self.vm, 'grep -c ' + shlex.quote(SAMPLE_ENDED) + ' ' + shlex.quote(self.log) + ' || true')
        return int(out.strip() or 0)

    def front_bundle(self):
        return guest(self.vm, 'lsappinfo info -only bundleid "$(lsappinfo front)"').split('=')[-1].strip().strip('"')

    def dictated_lines(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.dlog) + ' 2>/dev/null || true', 30)
        return [line for line in text.splitlines() if any(marker in line for marker in OUTCOMES)]

    def tool_pids(self):
        rows = guest(self.vm, 'ps -axo pid=,ppid=,command=')
        out = []
        for line in rows.splitlines():
            parts = line.split(None, 2)
            if len(parts) == 3 and TOOL_ARG in parts[2] and self.payload in parts[2]:
                out.append({'pid': int(parts[0]), 'ppid': int(parts[1]), 'command': parts[2]})
        return out

    def app_pid(self):
        return int(Path(self.state_file('app.pid')).read_text().strip())

    def state_file(self, name):
        import os
        return str(Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm / name)

    def set_clipboard(self, text):
        self.osa('set the clipboard to ' + json.dumps(text))

    def clipboard(self):
        return self.osa('get the clipboard as text')

    def press(self, key):
        """One press of the dictation key in one of its three shapes, by one ssh round trip: System
        Events posts a key code (sshd-session's own grant), post_key the top row's event. Not
        through ax.sh, whose preflight costs seconds a press cannot spend (minor 9: the second
        tap comes within 1 s of the sample's end). Returns when the event has been posted."""
        if key == 'brightness':
            guest(self.vm, shlex.quote(self.post_key) + ' --brightness-down')
        else:
            guest(self.vm, 'osascript -e ' + shlex.quote(f'tell application "System Events" to key code {int(key)}'))

    def sample_front(self):
        """`lsappinfo front` every 250 ms in the guest (activation.rs's method), to a file, until
        stop_front() is called (at most 10 minutes). Returns the file. Sampled from before the
        first tap until after the paste: a dictation with More accurate on a loaded guest takes
        46 s to write (walk-15a88cf29d54), far longer than the sample itself."""
        out = self.payload + f'/front-{time.time_ns()}.txt'
        script = ('end=$(( $(date +%s) + 600 )); while [ ! -e {o}.stop ] && [ $(date +%s) -lt $end ]; do '
                  'lsappinfo info -only bundleid "$(lsappinfo front)" >> {o}; sleep 0.25; done').format(o=shlex.quote(out))
        guest(self.vm, 'nohup /bin/bash -c ' + shlex.quote(script) + ' >/dev/null 2>&1 &')
        return out

    def stop_front(self, path):
        guest(self.vm, 'touch ' + shlex.quote(path + '.stop'))

    def front_samples(self, path):
        rows = guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true', 30).splitlines()
        return [r.split('=')[-1].strip().strip('"') for r in rows if r.strip()]

    def dictate(self, key, bundle, sentinel):
        """One dictation by `key` into the app `bundle` in front: tap, the sample plays through the
        capture path, tap again within 1 s of its end (minor 9), and the tool's log line. Returns
        the evidence. The clipboard holds `sentinel` before and must hold it after."""
        self.set_clipboard(sentinel)
        before = len(self.dictated_lines())
        if self.front_bundle() != bundle:
            raise StepFailed(f'{bundle} is not in front before the dictation: {self.front_bundle()}')
        sampler = self.sample_front()
        delivered_before = self.samples_delivered()
        began = time.monotonic()
        self.press(key)
        posted = time.monotonic()
        # The second tap comes when the sample has been DELIVERED, not when a wall clock says it
        # should have been: the injected source runs slow on a loaded host and never catches up
        # (capture.rs), and walk-be1a5b4bc16d's wall-clock tap cut the sentence after 1.52 s of a
        # 2.74 s sample. The source says when its file ends; each read of the log is one ssh
        # round trip, so the tap lands well inside minor 9's 1 s of the sample's end.
        while self.samples_delivered() <= delivered_before:
            if time.monotonic() - posted > 60:
                raise StepFailed('the sample was not delivered within 60 s of the first tap')
            time.sleep(0.2)
        delivered = time.monotonic()
        self.press(key)
        second = time.monotonic()
        end = time.monotonic() + DICTATION_WITHIN
        line = None
        while time.monotonic() < end:
            lines = self.dictated_lines()
            if len(lines) > before:
                line = lines[-1]
                break
            time.sleep(1)
        if line is None:
            tail = guest(self.vm, 'tail -30 ' + shlex.quote(self.dlog) + ' 2>/dev/null || true')
            raise StepFailed(f'no dictation finished within {DICTATION_WITHIN} s of the second tap; dictation.log tail:\n{tail}')
        if 'pasted' not in line:
            raise StepFailed(f'the dictation did not paste: {line}')
        model = re.search(r'dictation: model (\S+),', line).group(1)
        time.sleep(1.6)  # the clipboard is put back 1.0 s after the paste
        clip = self.clipboard()
        if clip != sentinel:
            raise StepFailed(f'the clipboard holds {clip!r}, not what it held before ({sentinel!r})')
        self.stop_front(sampler)
        time.sleep(0.5)
        fronts = self.front_samples(sampler)
        others = sorted(set(f for f in fronts if f != bundle))
        if not fronts or others:
            raise StepFailed(f'lsappinfo front was not {bundle} throughout: {len(fronts)} samples, others {others}')
        return {'key': key, 'log': line, 'model': model, 'expected': self.expected(model), 'clipboard_restored': True,
                'front_samples': len(fronts), 'front': bundle, 'press_seconds': round(posted - began, 3),
                'sample_delivered_after_seconds': round(delivered - posted, 3),
                'second_tap_after_delivery_seconds': round(second - delivered, 3)}

    def expected(self, model_id):
        """This guest's decode of the sample with `model_id`, as the tool decodes it."""
        cache = self.facts.setdefault('expected', {})
        if model_id not in cache:
            whisper = self.engine + '/runtime/bin/whisper-cli'
            model = f'{self.home}/.config/richos/models/ggml-{model_id}.bin'
            raw = guest(self.vm, ' '.join([shlex.quote(whisper), '-m', shlex.quote(model), '-f', shlex.quote(self.wav),
                                           *DECODE_ARGS, '2>/dev/null']), 300)
            text = strip_annotations(' '.join(line.strip() for line in raw.splitlines()))
            if len(words_of(text)) < 3:
                raise StepFailed(f'the guest decoded too little from the sample with {model_id} to judge by: {raw!r}')
            cache[model_id] = text
            self.save()
        return cache[model_id]

    def expect_words(self, text, want_text, label):
        want, got = words_of(want_text), words_of(text)
        if got != want:
            raise StepFailed(f'{label}: the words are {got}, not the expected {want} (text {text!r})')

    # --- steps --------------------------------------------------------------------------------
    def apps(self):
        listed = guest(self.vm, 'ls /Applications /System/Applications').splitlines()
        found = [a for a in CHROMIUM_APPS if a in listed]
        electron = guest(self.vm, 'ls -d /Applications/*.app/Contents/Frameworks/"Electron Framework.framework" '
                                  '2>/dev/null || true').splitlines()
        installed = False
        if not found and not electron:
            dmg = self.payload + '/googlechrome.dmg'
            guest(self.vm, f'curl -fsSL -o {shlex.quote(dmg)} {CHROME_DMG}', 600)
            mount = guest(self.vm, f'hdiutil attach -nobrowse -readonly {shlex.quote(dmg)} | tail -1', 120).split('\t')[-1].strip()
            guest(self.vm, f'cp -R {shlex.quote(mount)}/"Google Chrome.app" /Applications/ && hdiutil detach {shlex.quote(mount)}', 300)
            found = ['Google Chrome.app']
            installed = True
        chromium = (found or [Path(electron[0]).parents[2].name])[0]
        self.facts['chromium_app'] = chromium
        self.save()
        return {'applications': listed, 'chromium_or_electron': chromium, 'installed_chrome': installed}

    def stage(self):
        user_db = '"$HOME/Library/Application Support/com.apple.TCC/TCC.db"'
        sys_db = '"/Library/Application Support/com.apple.TCC/TCC.db"'
        row = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, "
               "indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) "
               "VALUES ('{svc}', 'com.richos.app', 0, 2, 2, 1, 0, 'UNUSED', 0, strftime('%s','now'));")
        # The two a person allows when he turns dictation on, and nothing else (plan section 4).
        guest(self.vm, f'sqlite3 {user_db} ' + shlex.quote(row.format(svc='kTCCServiceMicrophone')))
        guest(self.vm, f'sudo -n sqlite3 {sys_db} ' + shlex.quote(row.format(svc='kTCCServiceAccessibility')))
        query = "SELECT service FROM access WHERE client='com.richos.app' ORDER BY service;"
        grants = {'user': guest(self.vm, f'sqlite3 {user_db} ' + shlex.quote(query)).splitlines(),
                  'system': guest(self.vm, f'sudo -n sqlite3 {sys_db} ' + shlex.quote(query)).splitlines()}
        every = grants['user'] + grants['system']
        if sorted(every) != ['kTCCServiceAccessibility', 'kTCCServiceMicrophone']:
            raise StepFailed(f'com.richos.app must have exactly Microphone and Accessibility: {grants}')
        command([HERE / 'guest.sh', self.vm, '--push', self.a.wav, self.wav], 120)
        models = self.home + '/.config/richos/models'
        guest(self.vm, 'mkdir -p ' + shlex.quote(models))
        target = f'{models}/ggml-{self.a.model_id}.bin'
        command([HERE / 'guest.sh', self.vm, '--push', self.a.model, target], 900)
        for local, remote in ((self.a.post_key, self.post_key), (self.a.key_probe, self.key_probe)):
            command([HERE / 'guest.sh', self.vm, '--push', local, remote], 120)
            guest(self.vm, 'chmod 755 ' + shlex.quote(remote))
        guest(self.vm, 'cat > ' + shlex.quote(self.page) + " <<'HTML'\n"
              '<!doctype html><title>dictation box</title><textarea id=t autofocus style="width:90%;height:200px">'
              '</textarea><script>const t=document.getElementById("t");'
              't.addEventListener("input",()=>{document.title="box:"+t.value})</script>\nHTML')
        expected = self.expected(self.a.model_id)
        return {'tcc': grants, 'input_monitoring_row': False, 'wav': self.wav, 'wav_seconds': round(self.seconds, 3),
                'model': target, 'expected_text': {self.a.model_id: expected}}

    def relaunch(self):
        launched = relaunch(self.vm, environment={'RICHOS_VOICE_INPUT_WAV': self.wav, 'RICHOS_DICTATION_TEST_ON': '1'})
        self.log = launched['log']
        self.facts['log'] = self.log
        self.facts['app_pid'] = launched['pid']
        self.save()
        end = time.monotonic() + 60
        while time.monotonic() < end:
            text = guest(self.vm, 'cat ' + shlex.quote(self.log) + ' 2>/dev/null || true', 30)
            if 'key tap created for F1' in text and 'connected to the tool' in text:
                tools = self.tool_pids()
                if len(tools) != 1 or tools[0]['ppid'] != launched['pid']:
                    raise StepFailed(f'the tool is not exactly one child of the app {launched["pid"]}: {tools}')
                self.facts['tool_pid'] = tools[0]['pid']
                self.save()
                lines = [line for line in text.splitlines() if 'dictation' in line]
                return {'app_pid': launched['pid'], 'tool': tools[0], 'log': lines[-12:]}
            time.sleep(1)
        tail = guest(self.vm, 'tail -40 ' + shlex.quote(self.log) + ' || true')
        raise StepFailed('the tool did not start, take the key and connect within 60 s; log tail:\n' + tail)

    def settle(self):
        ready = '[richos] voice: ready on this machine'
        not_ready = '[richos] voice: not ready on this machine'
        end = time.monotonic() + 600
        line = None
        while time.monotonic() < end and line is None:
            text = guest(self.vm, 'cat ' + shlex.quote(self.log) + ' 2>/dev/null || true', 30)
            line = next((x for x in text.splitlines() if ready in x or not_ready in x), None)
            if line is None:
                time.sleep(2)
        if line is None:
            raise StepFailed('the app printed no voice-readiness line within 600 s')
        models = self.home + '/.config/richos/models'
        present = guest(self.vm, 'ls ' + shlex.quote(models)).split()
        decoded = {}
        for model_id in ('large-v3-turbo-q5_0', 'small.en'):
            if f'ggml-{model_id}.bin' in present:
                began = time.monotonic()
                decoded[model_id] = {'text': self.expected(model_id), 'seconds': round(time.monotonic() - began, 1)}
        return {'voice_readiness': line, 'models': present, 'decoded': decoded}

    def probe_on(self):
        out = self.payload + '/probe-on.jsonl'
        guest(self.vm, f'nohup {shlex.quote(self.key_probe)} --seconds 900 > {shlex.quote(out)} 2>&1 & echo $! > {shlex.quote(out)}.pid')
        time.sleep(2)
        first = guest(self.vm, 'head -1 ' + shlex.quote(out))
        if first != '{"ready":true}':
            raise StepFailed(f'key_probe did not create its listen-only tap: {first!r}')
        self.facts['probe_on'] = out
        self.save()
        return {'probe': out, 'ready': True}

    def textedit(self):
        guest(self.vm, ': > /tmp/dictation-textedit.txt && open -a TextEdit /tmp/dictation-textedit.txt')
        time.sleep(3)
        rows = []
        texts = []
        for n, key in enumerate(['122', '105', 'brightness']):
            rows.append(self.dictate(key, 'com.apple.TextEdit', f'clipboard-before-{n}'))
            nodes = [n for n in self.ax('find', '--role', 'AXTextArea', '--first', app='TextEdit')
                     if n.get('role') == 'AXTextArea']
            texts.append(nodes[0].get('value', '') if nodes else '')
        # The spacing rule: nothing before the first (an empty box), one space before each later one
        # (the character before the cursor is not whitespace), nothing after (end of the text).
        want_exact = [' '.join(r['expected'] for r in rows[:n + 1]) for n in range(len(rows))]
        for n, text in enumerate(texts):
            self.expect_words(text, want_exact[n], f'TextEdit after dictation {n + 1}')
        exact = [t == e for t, e in zip(texts, want_exact)]
        if not all(exact):
            raise StepFailed(f'TextEdit text is not the words joined by the spacing rule: {texts!r}')
        self.shot('textedit.png')
        return {'dictations': rows, 'texts': texts, 'spacing_rule_exact': exact}

    def terminal(self):
        guest(self.vm, 'open -a Terminal')
        time.sleep(4)
        row = self.dictate('122', 'com.apple.Terminal', 'clipboard-before-terminal')
        # Terminal's text area through ax.py (a fresh window's buffer is one login line and the
        # prompt, well inside the 203 characters ax.py keeps of a value). walk-157817a0a60e read
        # an empty string through System Events' entire contents while the words sat at the
        # prompt (terminal-failed.png); that read stays only as the fallback.
        nodes = [n for n in self.ax('find', '--role', 'AXTextArea', '--first', app='Terminal')
                 if n.get('role') == 'AXTextArea']
        text = nodes[0].get('value', '') if nodes else ''
        if not text.strip():
            text = self.osa(TERMINAL_TEXT)
        want = words_of(row['expected'])
        if words_of(text)[-len(want):] != want:
            raise StepFailed(f'Terminal does not show the words at its prompt: {text[-200:]!r}')
        self.shot('terminal.png')
        return {'dictation': row, 'terminal_text_tail': text[-200:]}

    def browser_box(self, app, bundle, label):
        guest(self.vm, f'open -a {shlex.quote(app)} {shlex.quote("file://" + self.page)}')
        time.sleep(6)
        row = self.dictate('122', bundle, f'clipboard-before-{label}')
        title = self.osa(f'tell application "System Events" to get name of front window of (first process whose bundle identifier is "{bundle}")')
        if not title.startswith('box:'):
            raise StepFailed(f'{label}: the text box received nothing (window title {title!r})')
        text = title[len('box:'):]
        self.expect_words(text, row['expected'], label)
        if text != row['expected']:
            raise StepFailed(f'{label}: {text!r} is not exactly {row["expected"]!r}')
        self.shot(f'{label}.png')
        return {'dictation': row, 'text': text}

    def safari(self):
        return self.browser_box('Safari', 'com.apple.Safari', 'safari')

    def chromium(self):
        app = self.facts['chromium_app']
        bundle = guest(self.vm, f'defaults read /Applications/{shlex.quote(app)}/Contents/Info CFBundleIdentifier')
        extra = ''
        if app == 'Google Chrome.app':
            extra = '--args ' + ' '.join(CHROME_FLAGS) + ' --user-data-dir=' + shlex.quote(self.payload + '/chrome')
            # `open -na App --args ... URL`: the page is an argument too. By path, not by name: a
            # Chrome the apps step copied into /Applications seconds earlier is not yet registered
            # with LaunchServices, and `open -na "Google Chrome"` answered "Unable to find
            # application named 'Google Chrome'" (walk-447858a8c733).
            guest(self.vm, f'open -na {shlex.quote("/Applications/" + app)} {extra} {shlex.quote("file://" + self.page)}')
            time.sleep(8)
            row = self.dictate('122', bundle, 'clipboard-before-chromium')
            title = self.osa(f'tell application "System Events" to get name of front window of (first process whose bundle identifier is "{bundle}")')
            text = title[len('box:'):].rsplit(' - Google Chrome', 1)[0] if title.startswith('box:') else ''
            if not text:
                raise StepFailed(f'Chrome: the text box received nothing (window title {title!r})')
            self.expect_words(text, row['expected'], 'Chrome')
            self.shot('chromium.png')
            return {'app': app, 'bundle': bundle, 'dictation': row, 'text': text}
        return self.browser_box(app.removesuffix('.app'), bundle, 'chromium')

    def probe_check(self):
        out = self.facts['probe_on']
        lines = guest(self.vm, 'cat ' + shlex.quote(out)).splitlines()
        pid = guest(self.vm, 'cat ' + shlex.quote(out + '.pid')).strip()
        guest(self.vm, f'kill {int(pid)} 2>/dev/null || true')
        seen = [line for line in lines if any(k in line for k in NO_TAP_EVENTS)]
        if seen:
            raise StepFailed(f'the passive listener saw the dictation key while dictation was on: {seen}')
        return {'lines': lines, 'key_events_seen': 0, 'probe_pid_stopped': int(pid)}

    def second_tool(self):
        app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
        exe = app + '/Contents/MacOS/richos-tauri'
        started = time.monotonic()
        out = guest(self.vm, f'HOME={shlex.quote(self.home)} {shlex.quote(exe)} {TOOL_ARG} --data-dir {shlex.quote(self.data)} '
                             f'>/dev/null 2>&1; echo $?', 60)
        took = round(time.monotonic() - started, 2)
        if out.strip() != '3':
            raise StepFailed(f'a second tool exited {out!r}, not 3 (another copy holds the key)')
        tools = self.tool_pids()
        if [t['pid'] for t in tools] != [self.facts['tool_pid']]:
            raise StepFailed(f'the first tool is not the only one left: {tools}')
        guest(self.vm, 'open -a TextEdit /tmp/dictation-textedit.txt')
        time.sleep(2)
        row = self.dictate('122', 'com.apple.TextEdit', 'clipboard-before-second-tool')
        return {'second_tool_exit': 3, 'seconds': took, 'first_still_owns_f1': row}

    def idle(self):
        time.sleep(self.a.idle)
        guest(self.vm, 'open -a TextEdit /tmp/dictation-textedit.txt')
        time.sleep(2)
        row = self.dictate('122', 'com.apple.TextEdit', 'clipboard-after-idle')
        log = guest(self.vm, 'cat ' + shlex.quote(self.dlog))
        disables = [line for line in log.splitlines() if 'disabled by' in line]
        if disables:
            raise StepFailed(f'the key tap was disabled during the walk: {disables}')
        return {'idle_seconds': self.a.idle, 'dictation': row, 'timeout_disable_lines': 0}

    def scratch(self):
        left = guest(self.vm, 'ls -A ' + shlex.quote(self.data + '/dictation-scratch') + ' 2>/dev/null || true')
        if left:
            raise StepFailed(f'the scratch folder is not empty: {left!r}')
        rss = guest(self.vm, f'ps -o rss= -p {self.facts["tool_pid"]}').strip()
        log = guest(self.vm, 'cat ' + shlex.quote(self.dlog))
        (self.out / 'dictation.log').write_text(log)
        return {'scratch_entries': 0, 'tool_rss_kib': int(rss), 'dictation_log_lines': len(log.splitlines())}

    def ends_with_app(self):
        app_pid = self.facts['app_pid']
        tool_pid = self.facts['tool_pid']
        guest(self.vm, f'kill -TERM {int(app_pid)}')
        started = time.monotonic()
        while time.monotonic() - started < 10:
            alive = guest(self.vm, f'kill -0 {int(tool_pid)} 2>/dev/null && echo alive || true')
            if alive != 'alive':
                break
            time.sleep(0.1)
        took = round(time.monotonic() - started, 2)
        if self.tool_pids():
            raise StepFailed(f'the tool outlived its app: {self.tool_pids()}')
        if took > 3:
            raise StepFailed(f'the tool took {took} s to end after its app')
        return {'app_pid_terminated': app_pid, 'tool_gone_after_seconds': took}

    def probe_off(self):
        launched = relaunch(self.vm, environment={'RICHOS_VOICE_INPUT_WAV': self.wav, 'RICHOS_DICTATION_TEST_ON': '0'})
        time.sleep(15)
        if self.tool_pids():
            raise StepFailed(f'a tool runs with dictation off: {self.tool_pids()}')
        guest(self.vm, 'open -a TextEdit /tmp/dictation-textedit.txt')
        time.sleep(2)
        out = self.payload + '/probe-off.jsonl'
        guest(self.vm, f'nohup {shlex.quote(self.key_probe)} --seconds 20 > {shlex.quote(out)} 2>&1 &')
        time.sleep(2)
        for key in ['122', '105', 'brightness']:
            self.press(key)
            time.sleep(1)
        time.sleep(18)
        lines = guest(self.vm, 'cat ' + shlex.quote(out)).splitlines()
        f1 = [x for x in lines if '"code":122' in x]
        f13 = [x for x in lines if '"code":105' in x]
        bright = [x for x in lines if '"keyType":3' in x]
        if not (f1 and f13 and bright):
            raise StepFailed(f'with dictation off the passive listener must see all three: {lines}')
        return {'app_pid': launched['pid'], 'tool': None, 'probe': lines}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--wav', type=Path, required=True, help='a short spoken sample, 16 kHz mono WAV (say -o, then afconvert)')
    p.add_argument('--model', type=Path, required=True, help='the GGML weights to copy in, matching --model-id')
    p.add_argument('--model-id', default='small.en')
    p.add_argument('--post-key', type=Path, required=True, help='the built post_key example')
    p.add_argument('--key-probe', type=Path, required=True, help='the built key_probe example')
    p.add_argument('--idle', type=float, default=600, help='seconds the tool is left idle before one more dictation')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = DictationWalk(a)
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
        if not ok:
            try:
                walk.shot(f'{step}-failed.png')
                (a.out / 'app.log.tail').write_text(guest(a.vm, 'tail -80 ' + shlex.quote(walk.log) + ' || true', 30))
                (a.out / 'dictation.log').write_text(guest(a.vm, 'cat ' + shlex.quote(walk.dlog) + ' 2>/dev/null || true', 30))
            except Exception as exc:  # evidence only; the step already failed
                (a.out / 'evidence-error.txt').write_text(str(exc))
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
