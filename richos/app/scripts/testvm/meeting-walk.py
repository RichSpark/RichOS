#!/usr/bin/env python3
"""meeting-walk.py — meeting notes in the packaged app, in the guest, with a stand-in for "another app
starts using the microphone" and no real call, calendar or person.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --no-audio --report REPORT --wait 600 -- \\
      meeting-walk.py --out DIR --expect-sha SHA --model WHISPER.bin

WHY IT EXISTS. The Granola walk (richos-hq tools-private/windows-call-lab/mac-guest/meeting-walk.sh)
needs the Windows call lab and a real Zoom meeting. This walk needs neither: `mic-user.swift`
(compiled in the guest) opens the guest's default input, which is BlackHole 2ch, exactly as a call app
does, and the capture companion's `watch` sees another process running input. The near end's speech is
a WAV played into BlackHole 2ch (mac-audio-lab play); the guest boots with no sound device, so nothing is
played aloud anywhere (CEO §53, §65). Synthetic speech only.

STEPS (steps in the guest, never on the host's screen):
  identity     the running app says it was built from --expect-sha
  companion    Contents/MacOS/richos-companion exists beside the app and the app log says
               "meeting notes: companion ...", never "no capture companion"
  audio        BlackHole installed, default input set, microphone/system-audio grants written the way
               tccd records a person's Allow (the guest image has SIP off; never on a host)
  first-run    a company is added (folders-walk.py's own first run)
  prompt       another process starts using the microphone: the app offers Take Notes at the top right;
               nothing records until it is pressed (no capture process, no session folder)
  record       Take Notes pressed: "Recording meeting"; speech played into the microphone
  stop         Stop pressed on the panel: the recording ends; Rich posts the notes in the conversation
               and the transcript is in the Output panel
  settings     Settings carries "Start meeting notes" and "Pause meeting prompts"; Pause on: another
               mic user raises no prompt; Start meeting notes from Settings records; its row reads
               "Stop meeting notes" and stops it
  retry        the capture process killed while recording: the problem card offers Try again, and
               pressing it records again
Exit 0 when every step passes.
"""
import argparse
import importlib.util
import json
import os
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
LAB = Path('/Volumes/E1TB/caches/richos-mac-call-lab')
STEPS = ['identity', 'companion', 'audio', 'first-run', 'prompt', 'record', 'stop', 'settings', 'retry']
LABDIR = '/Users/admin/lab'


def host_modules():
    sys.path.insert(0, str(HERE))
    from relaunch import guest  # noqa: E402
    spec = importlib.util.spec_from_file_location('folders_walk', HERE / 'folders-walk.py')
    folders = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(folders)
    return guest, folders


def build_walk(folders, guest):
    StepFailed = folders.StepFailed
    command = folders.command

    class Walk(folders.Walk):
        def __init__(self, a):
            super().__init__(a)
            self.applog = self.payload + '/app.log'
            self.mic_started = None

        # --- the guest -----------------------------------------------------------------------
        def g(self, cmd, timeout=120):
            return guest(self.vm, cmd, timeout)

        def log_has(self, text):
            r = subprocess.run([str(HERE / 'guest.sh'), self.vm, 'grep -c -- ' + shlex.quote(text) + ' ' + shlex.quote(self.applog) + ' || true'],
                               capture_output=True, text=True, timeout=60)
            try:
                return int((r.stdout.strip() or '0').splitlines()[-1])
            except ValueError:
                return 0

        def wait_log(self, text, seconds, more_than=0):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if self.log_has(text) > more_than:
                    return True
                time.sleep(2)
            return False

        def sessions(self):
            out = self.g("find " + shlex.quote(self.home + '/RichOS/corpus') + " -maxdepth 6 -name session.json 2>/dev/null || true")
            return [l for l in out.splitlines() if l.strip()]

        def capture_pids(self):
            out = self.g("pgrep -f 'richos-companion capture' || true")
            return [l.strip() for l in out.splitlines() if l.strip().isdigit()]

        def start_mic(self):
            """Another app starts using the microphone."""
            self.g(f'nohup {LABDIR}/mic-user > {LABDIR}/mic-user.log 2>&1 & echo $! > {LABDIR}/mic-user.pid')
            self.mic_started = time.monotonic()

        def stop_mic(self):
            """The other app stops using the microphone. A background job of a non-interactive shell
            ignores SIGINT, so SIGTERM first, then SIGKILL; the walk checks the process is gone,
            because a mic user that is still there is still the same call to the app (measured:
            meet5, where the old user kept the call going and the next one rightly raised no prompt)."""
            gone = self.g(f'p=$(cat {LABDIR}/mic-user.pid 2>/dev/null); if [ -n "$p" ]; then kill -TERM $p 2>/dev/null; '
                          f'for i in 1 2 3 4 5; do kill -0 $p 2>/dev/null || break; sleep 1; done; '
                          f'kill -0 $p 2>/dev/null && kill -KILL $p 2>/dev/null; rm -f {LABDIR}/mic-user.pid; fi; '
                          f'pgrep -x mic-user || echo none')
            if gone.strip() != 'none':
                raise StepFailed('mic-user is still running after it was stopped: ' + gone)

        def panel_press(self, title, seconds=30):
            end = time.monotonic() + seconds
            last = None
            while time.monotonic() < end:
                try:
                    self.ax('click', '--title', title, '--role', 'AXButton', '--first', '--in', 'window', 'RichOS meeting notes')
                    return
                except StepFailed as exc:
                    last = exc
                    time.sleep(2)
            raise StepFailed(f'no "{title}" button in the meeting panel: {last}')

        def panel_says(self, words, seconds=30):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if self.text_on_screen(words):
                    return True
                time.sleep(2)
            return False

        def open_settings(self):
            for _ in range(3):
                try:
                    self.ax('click', '--id', 'set-btn', '--first')
                    end = time.monotonic() + 20
                    while time.monotonic() < end:
                        if self.nodes('--id', 'mn-start'):
                            return
                        time.sleep(1)
                except StepFailed:
                    time.sleep(2)
            raise StepFailed('the Settings panel did not open or has no Start meeting notes row')

        def close_settings(self):
            command([HERE / 'ax.sh', self.vm, 'tell application "System Events" to key code 53'], 60)
            time.sleep(1)

        # --- steps --------------------------------------------------------------------------
        def companion(self):
            said0 = self.g("grep -m1 'meeting notes: companion' " + shlex.quote(self.applog) + " || true").strip()
            app = said0.split('meeting notes: companion', 1)[-1].strip().rsplit('/Contents/MacOS/', 1)[0] if said0 else ''
            exists = self.g(f"ls -l {shlex.quote(app)}/Contents/MacOS/richos-companion 2>&1 || true").strip() if app else ''
            said = self.g("grep -m3 'meeting notes' " + shlex.quote(self.applog) + " || true").strip()
            ev = {'app': app, 'companion_file': exists, 'log': said}
            self.facts['companion'] = ev
            self.save()
            if 'No such file' in exists or not exists:
                raise StepFailed('no richos-companion beside the app: ' + exists)
            if 'meeting notes: companion' not in said:
                raise StepFailed('the app log has no "meeting notes: companion" line: ' + said)
            if 'no capture companion' in said:
                raise StepFailed('the app log says "no capture companion": ' + said)
            return ev

        def audio(self):
            g = self.g
            g(f'mkdir -p {LABDIR}')
            for f in ('BlackHole2ch-0.6.1.pkg', 'BlackHole16ch-0.6.1.pkg', 'bin/mac-audio-lab', 'near-track.wav'):
                command([HERE / 'guest.sh', self.vm, '--push', LAB / f, LABDIR + '/'], 180)
            command([HERE / 'guest.sh', self.vm, '--push', HERE / 'mic-user.swift', LABDIR + '/'], 60)
            res = {}
            for p in ('BlackHole2ch-0.6.1.pkg', 'BlackHole16ch-0.6.1.pkg'):
                res[p] = g(f'sudo -n installer -pkg {LABDIR}/{p} -target / 2>&1 | tail -1')
            g('sudo -n killall coreaudiod; sleep 4; true')
            res['default_input'] = g(f'chmod +x {LABDIR}/mac-audio-lab; {LABDIR}/mac-audio-lab default-input "BlackHole 2ch" 2>&1')
            res['default_output'] = g(f'{LABDIR}/mac-audio-lab default-output "BlackHole 16ch" 2>&1')
            res['compile'] = g(f'cd {LABDIR} && swiftc -O mic-user.swift -o mic-user 2>&1 | tail -5; ls -l mic-user 2>&1', 300)
            userdb = '/Users/admin/Library/Application Support/com.apple.TCC/TCC.db'
            sysdb = '/Library/Application Support/com.apple.TCC/TCC.db'

            def tcc(db, service, client, ctype):
                sql = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, "
                       "indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) VALUES "
                       f"('{service}', '{client}', {ctype}, 2, 2, 1, 0, 'UNUSED', 0, strftime('%s','now'));")
                return g(f'sudo -n sqlite3 "{db}" "{sql}" 2>&1 || true')
            res['tcc'] = [
                tcc(userdb, 'kTCCServiceMicrophone', 'com.richos.app', 0),
                tcc(userdb, 'kTCCServiceAudioCapture', 'com.richos.app', 0),
                tcc(sysdb, 'kTCCServiceAudioCapture', 'com.richos.app', 0),
                tcc(userdb, 'kTCCServiceMicrophone', f'{LABDIR}/mic-user', 1),
                tcc(userdb, 'kTCCServiceMicrophone', '/usr/libexec/sshd-keygen-wrapper', 1),
            ]
            models = self.home + '/.config/richos/models'
            g(f'mkdir -p {shlex.quote(models)}')
            command([HERE / 'guest.sh', self.vm, '--push', Path(self.a.model), models + '/' + Path(self.a.model).name], 300)
            self.facts['audio'] = res
            self.save()
            if 'BlackHole 2ch' not in res['default_input']:
                raise StepFailed("the guest's default input is not BlackHole 2ch: " + res['default_input'])
            if 'mic-user' not in res['compile'] or 'No such file' in res['compile']:
                raise StepFailed('mic-user did not compile: ' + res['compile'])
            return res

        def prompt(self):
            before_sessions = self.sessions()
            self.start_mic()
            if not self.wait_log('ShowPrompt', 60):
                self.shot('prompt-missing.png')
                raise StepFailed('no prompt after another app took the microphone; mic-user: ' + self.g(f'cat {LABDIR}/mic-user.log'))
            seen = self.panel_says('Take Notes', 20)
            self.shot('prompt.png')
            time.sleep(8)
            recording = self.capture_pids()
            ev = {'prompt_text_on_screen': seen, 'capture_processes_while_prompted': recording,
                  'sessions_before': before_sessions, 'sessions_while_prompted': self.sessions(),
                  'start_capture_logged': self.log_has('StartCapture')}
            self.facts['prompt'] = ev
            self.save()
            if not seen:
                raise StepFailed('Take Notes is not on screen')
            if recording or ev['start_capture_logged'] or ev['sessions_while_prompted'] != before_sessions:
                raise StepFailed('something recorded before Take Notes was pressed: ' + json.dumps(ev))
            return ev

        def record(self):
            self.panel_press('Take Notes')
            if not self.wait_log('StartCapture', 40):
                self.shot('no-recording.png')
                raise StepFailed('Take Notes pressed, no StartCapture in the log')
            shown = self.panel_says('Recording meeting', 40)
            self.shot('recording.png')
            self.g(f'nohup {LABDIR}/mac-audio-lab play {LABDIR}/near-track.wav --device "BlackHole 2ch" > {LABDIR}/near.log 2>&1 &')
            time.sleep(45)
            ev = {'recording_text_on_screen': shown, 'sessions': self.sessions(), 'capture_processes': self.capture_pids()}
            self.facts['record'] = ev
            self.save()
            if not shown:
                raise StepFailed('"Recording meeting" never showed on screen')
            if not ev['sessions']:
                raise StepFailed('no session folder while recording')
            return ev

        def stop(self):
            self.panel_press('Stop')
            if not self.wait_log('StopCapture', 40):
                raise StepFailed('Stop pressed, no StopCapture in the log')
            self.stop_mic()
            self.shot('after-stop.png')
            if not self.wait_log('Rich said the notes', 900):
                self.shot('no-notes.png')
                raise StepFailed('Rich never posted the notes (no "Rich said the notes" in the log)')
            # the conversation: the notes' headings
            time.sleep(5)
            self.shot('notes.png')
            heads = {w: self.text_on_screen(w) for w in ('Decisions', 'Your actions', 'Other commitments', 'Open questions')}
            session = self.sessions()[0].rsplit('/', 1)[0]
            files = self.g(f'ls -l {shlex.quote(session)}')
            # the transcript in the Output panel
            self.ax('click', '--id', 'out-bottom', '--role', 'AXCheckBox', '--first')
            time.sleep(4)
            in_output = self.panel_says('transcript.md', 20)
            self.shot('output-panel.png')
            self.ax('click', '--id', 'out-bottom', '--role', 'AXCheckBox', '--first')
            time.sleep(1)
            ev = {'headings_on_screen': heads, 'session_files': files, 'transcript_in_output_panel': in_output}
            self.facts['stop'] = ev
            self.save()
            if 'transcript.md' not in files:
                raise StepFailed('no transcript.md beside the recording: ' + files)
            if not all(heads.values()) or not in_output:
                raise StepFailed('the notes or the Output panel lack something: ' + json.dumps(ev))
            return ev

        def settings(self):
            self.open_settings()
            self.shot('settings.png')
            ev = {'start_row': bool(self.nodes('--id', 'mn-start')), 'pause_row': bool(self.nodes('--id', 'mn-pause'))}
            if not (ev['start_row'] and ev['pause_row']):
                raise StepFailed('Settings lacks a meeting row: ' + json.dumps(ev))
            # Pause on, another mic user: no prompt
            self.ax('click', '--id', 'mn-pause', '--first')
            time.sleep(2)
            ev['pause_checked'] = [n.get('value') for n in self.nodes('--id', 'mn-pause')]
            self.shot('settings-paused.png')
            self.close_settings()
            prompts = self.log_has('ShowPrompt')
            self.start_mic()
            time.sleep(25)
            ev['prompts_before'] = prompts
            ev['prompts_while_paused'] = self.log_has('ShowPrompt')
            self.shot('paused-no-prompt.png')
            self.stop_mic()
            time.sleep(12)
            # Start meeting notes from Settings records
            self.open_settings()
            starts = self.log_has('StartCapture')
            self.ax('click', '--id', 'mn-start', '--first')
            ev['manual_start'] = self.wait_log('StartCapture', 40, more_than=starts - 1 if starts else 0) and self.log_has('StartCapture') > starts
            time.sleep(6)
            self.shot('settings-recording.png')
            stops = self.log_has('StopCapture')
            self.open_settings()
            ev['row_text'] = [n.get('title') or n.get('value') for n in self.nodes('--id', 'mn-start-name')]
            self.shot('settings-row-while-recording.png')
            self.ax('click', '--id', 'mn-start', '--first')
            ev['manual_stop'] = self.wait_log('StopCapture', 40, more_than=stops)
            self.shot('settings-stopped.png')
            # prompts switched back on (the row closes Settings when pressed: open it again if so)
            if not self.nodes('--id', 'mn-pause'):
                self.open_settings()
            self.ax('click', '--id', 'mn-pause', '--first')
            time.sleep(2)
            ev['pause_after_resume'] = [n.get('value') for n in self.nodes('--id', 'mn-pause')]
            self.close_settings()
            self.facts['settings'] = ev
            self.save()
            if ev['prompts_while_paused'] > prompts:
                raise StepFailed('a prompt appeared while Pause meeting prompts was on: ' + json.dumps(ev))
            if not ev['manual_start']:
                raise StepFailed('Start meeting notes did not start a capture: ' + json.dumps(ev))
            if not ev['manual_stop']:
                raise StepFailed('the row did not stop the recording: ' + json.dumps(ev))
            return ev

        def meeting_log(self, name):
            text = self.g("grep -n -i 'meeting\\|ShowPrompt\\|StartCapture\\|StopCapture\\|microphone' " + shlex.quote(self.applog) + " | tail -80 || true")
            (self.out / name).write_text(text + '\n')

        def retry(self):
            before = self.log_has('ShowPrompt')
            self.start_mic()
            got = self.wait_log('ShowPrompt', 60, more_than=before)
            self.meeting_log('retry-meeting-log.txt')
            self.shot('retry-prompt.png')
            self.facts['retry_prompt'] = {'prompt_logged': got, 'mic_user_log': self.g(f'cat {LABDIR}/mic-user.log')}
            self.save()
            if not got:
                raise StepFailed('no prompt when another app took the microphone after the settings step (see retry-meeting-log.txt)')
            # A real failure the app reports itself: the company's meetings folder cannot be written
            # (a full or read-only disk), so the recorder cannot start. SIGKILL of the recorder is no
            # such case: it leaves no card (the process is gone with no word; measured on this build).
            zone = self.home + '/RichOS/corpus/companies/acme/evidence/meetings'
            self.g(f'chmod a-w {shlex.quote(zone)}')
            self.panel_press('Take Notes')
            shown = self.panel_says('Try again', 40)
            self.shot('problem.png')
            self.meeting_log('retry-failed-log.txt')
            self.g(f'chmod u+w {shlex.quote(zone)}')
            if not shown:
                raise StepFailed('no Try again card after the recorder could not start (see retry-failed-log.txt)')
            starts = self.log_has('StartCapture')
            self.panel_press('Try again')
            again = self.wait_log('StartCapture', 40, more_than=starts)
            recording = self.panel_says('Recording meeting', 30)
            self.shot('retried.png')
            self.stop_mic()
            time.sleep(15)
            ev = {'try_again_on_screen': shown, 'recording_again': again, 'recording_text_on_screen': recording}
            self.facts['retry'] = ev
            self.save()
            if not again or not recording:
                raise StepFailed('Try again did not start the capture again: ' + json.dumps(ev))
            return ev

    return Walk, StepFailed


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--model', type=Path, required=True, help='ggml-large-v3-turbo-q5_0.bin')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    bad = [s for s in steps if s not in STEPS and s != 'first-run']
    if bad:
        p.error('unknown step(s): ' + ', '.join(bad))
    guest, folders = host_modules()
    Walk, StepFailed = build_walk(folders, guest)
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)[:3000]
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        walk.stop_mic()
    except Exception as exc:  # the guest is removed by run-walk.py either way
        print('stop_mic at the end: ' + str(exc)[:200], flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
