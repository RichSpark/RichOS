#!/usr/bin/env python3
"""richbrief-walk.py — RichBrief with a BROWSER call, in the packaged app, in the guest. No real call,
calendar or person: the "call" is a fixture page served from the guest itself.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --no-audio --report REPORT --wait 600 -- \\
      richbrief-walk.py --out DIR --expect-sha SHA --model WHISPER.bin

WHY IT EXISTS. meeting-walk.py stands in for a native call app (a Swift mic user). The browser route has
its own rules (richbrief-browser-mute and richbrief-chrome-join-window, 2026-11-06/07): a Chrome call
whose input resets while its sound goes on is still the call, and a hang-up ends the recording even while
another tab plays sound. Real Google Chrome (installed in the guest from the call lab's dmg) opens
fixture pages from files in the guest (--allow-file-access-from-files); a page polls cmd.txt, so the walk
sends "mute", "unmute", "reset" (stop the microphone tracks, open them again 1.5 s later, the sound
playing throughout) and "hangup" without a debugger. Chrome's microphone is BlackHole 2ch and its sound
goes to BlackHole 16ch (the guest boots with --no-audio: nothing is audible anywhere, CEO section 53, 65).

STEPS (in the guest, never on the host's screen):
  identity, companion, audio, first-run   as meeting-walk.py
  chrome         Chrome installed, its microphone grant written, the fixture pages in place, Chrome opened on
                 a tab that only plays a (silent) sound: no prompt for 20 s
  browser-prompt a second tab joins the fixture call (microphone + sound): the panel offers Take Notes
                 and nothing records until it is pressed
  browser-record Take Notes pressed; speech played into the microphone; the page mutes, unmutes, and
                 resets its microphone while the sound goes on: the recording is still running
  browser-hangup the page hangs up (no other sound): StopCapture within 40 s and Rich says the notes
  saved-retry    a second call; the transcription model is taken away before the recording ends, the
                 failure message is in the conversation with Try again; the model is put back and
                 Try again writes the notes from the saved recording
Exit 0 when every step passes.
"""
import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('meeting_walk', HERE / 'meeting-walk.py')
meeting = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(meeting)

LAB = meeting.LAB
LABDIR = meeting.LABDIR
WEB = LABDIR + '/web'
STEPS = ['identity', 'companion', 'audio', 'first-run', 'chrome', 'browser-prompt', 'browser-record',
         'browser-hangup', 'saved-retry']

CALL_HTML = """<!doctype html><meta charset="utf-8"><title>fixture call</title><body>fixture call page
<script>
let stream = null, ctx = null, osc = null;
async function mic() { stream = await navigator.mediaDevices.getUserMedia({audio: true}); }
async function join() {
  if (ctx) return;
  // Playback opens before the microphone, as measured on a real Zoom join in Chrome (meeting.rs END_GRACE_MS).
  ctx = new AudioContext();
  const g = ctx.createGain(); g.gain.value = 0.0001;
  osc = ctx.createOscillator(); osc.connect(g).connect(ctx.destination); osc.start();
  await new Promise(r => setTimeout(r, 1500));
  await mic();
  // A live Zoom join in Chrome reopened its microphone six seconds after first opening it, the sound
  // playing throughout (meeting.rs, MicActivity.swift): the same, 5 s after the first microphone.
  setTimeout(async () => {
    if (!stream) return;
    stream.getTracks().forEach(t => t.stop());
    await new Promise(r => setTimeout(r, 1500));
    if (ctx) await mic();
  }, 5000);
  document.title = 'fixture call: in call';
}
function ack(n, c) {
  // The page says what it did and what its tracks look like now, to the walk's own server in the guest.
  const live = stream ? stream.getTracks().filter(t => t.readyState === 'live').length : 0;
  const en = stream ? stream.getAudioTracks().filter(t => t.enabled).length : 0;
  new Image().src = 'http://127.0.0.1:8765/ack?n=' + n + '&cmd=' + c + '&live=' + live + '&en=' + en + '&ctx=' + (ctx ? ctx.state : 'none') + '&t=' + Date.now();
}
async function act(c, n) {
  if (c === 'join') await join();
  else if (c === 'mute') stream.getAudioTracks().forEach(t => t.enabled = false);
  else if (c === 'unmute') stream.getAudioTracks().forEach(t => t.enabled = true);
  else if (c === 'hangup') { stream.getTracks().forEach(t => t.stop()); osc.stop(); await ctx.close(); ctx = null; stream = null; document.title = 'fixture call: left'; }
  ack(n, c);
}
let seen = '';
setInterval(async () => {
  try {
    const t = (await (await fetch('cmd.txt?' + Date.now(), {cache: 'no-store'})).text()).trim();
    if (t && t !== seen) { seen = t; const [n, c] = t.split(':'); await act(c, n); }
  } catch (e) {}
}, 500);
join().then(() => ack(0, 'join')).catch(e => { document.title = 'join failed ' + e; });
</script>"""

MUSIC_HTML = """<!doctype html><meta charset="utf-8"><title>fixture music</title><body>fixture music tab
<script>
const ctx = new AudioContext();
const g = ctx.createGain(); g.gain.value = 0.0001;
const o = ctx.createOscillator(); o.connect(g).connect(ctx.destination); o.start();
</script>"""


def build():
    guest, folders = meeting.host_modules()
    Walk, StepFailed = meeting.build_walk(folders, guest)
    command = folders.command

    class BriefWalk(Walk):
        def __init__(self, a):
            super().__init__(a)
            self.cmd_n = 0

        # --- the fixture --------------------------------------------------------------------
        def send(self, cmd):
            """One command for the call page: the page runs each new line once. Returns its number."""
            self.cmd_n += 1
            self.g(f'printf "%s" {shlex.quote(str(self.cmd_n) + ":" + cmd)} > {WEB}/cmd.txt')
            return self.cmd_n

        def ack(self, n, cmd, want, seconds=30):
            """The page's own account of command n (its tracks and sound context after it ran), read from
            the guest server's log. Raises when it never came or the tracks are not what the command makes
            them. `want` is a dict of field -> predicate."""
            end = time.monotonic() + seconds
            fields = None
            while time.monotonic() < end and fields is None:
                line = self.g(f'grep -m1 "GET /ack?n={n}&cmd={cmd}&" {WEB}/server.log || true').strip()
                if line:
                    query = line.split('/ack?', 1)[1].split(' ', 1)[0]
                    fields = dict(kv.split('=', 1) for kv in query.split('&'))
                else:
                    time.sleep(1)
            if fields is None:
                raise StepFailed(f'the page never acknowledged command {n} ({cmd}): it was not applied')
            bad = {k: fields.get(k) for k, ok in want.items() if not ok(fields.get(k))}
            if bad:
                raise StepFailed(f'the page acknowledged {cmd} with the wrong state {bad}: {fields}')
            return fields

        def open_tab(self, page):
            self.g(f'open -a "/Applications/Google Chrome.app" file://{WEB}/{page}')

        def chrome_title(self):
            return ''  # Chrome answers no AppleScript here (Automation consent); the frames show the tabs

        def diag(self):
            """Keep the second watcher's trace and the app's meeting lines whatever happened."""
            self.meeting_log('meeting-log.txt')
            command([HERE / 'guest.sh', self.vm, '--pull', LABDIR + '/watch.jsonl', self.out / 'watch.jsonl'], 120)

        def count(self, text):
            return self.log_has(text)

        def front_app(self, name):
            self.script(f'tell application "{name}" to activate')
            time.sleep(2)

        # --- steps --------------------------------------------------------------------------
        def chrome(self):
            g = self.g
            with tempfile.TemporaryDirectory(prefix='richbrief-web-') as tmp:
                (Path(tmp) / 'call.html').write_text(CALL_HTML)
                (Path(tmp) / 'music.html').write_text(MUSIC_HTML)
                (Path(tmp) / 'cmd.txt').write_text('0:none')
                g(f'mkdir -p {WEB}')
                for f in ('call.html', 'music.html', 'cmd.txt'):
                    command([HERE / 'guest.sh', self.vm, '--push', Path(tmp) / f, WEB + '/'], 60)
            command([HERE / 'guest.sh', self.vm, '--push', LAB / 'googlechrome.dmg', LABDIR + '/'], 600)
            inst = g(f'hdiutil attach -nobrowse -quiet {LABDIR}/googlechrome.dmg && sudo -n ditto "/Volumes/Google Chrome/Google Chrome.app" "/Applications/Google Chrome.app" '
                     f'&& hdiutil detach -quiet "/Volumes/Google Chrome" && echo installed', 600)
            if 'installed' not in inst:
                raise StepFailed('Chrome did not install: ' + inst)
            userdb = '/Users/admin/Library/Application Support/com.apple.TCC/TCC.db'
            sql = ("INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, "
                   "indirect_object_identifier_type, indirect_object_identifier, flags, last_modified) VALUES "
                   "('kTCCServiceMicrophone', 'com.google.Chrome', 0, 2, 2, 1, 0, 'UNUSED', 0, strftime('%s','now'));")
            tcc = g(f'sudo -n sqlite3 "{userdb}" "{sql}" 2>&1 || true')
            g(f'nohup python3 -m http.server 8765 --bind 127.0.0.1 --directory {WEB} > {WEB}/server.log 2>&1 &')
            time.sleep(2)
            served = g(f'head -c 60 {WEB}/music.html')  # the music tab opens later, for the second call
            # A second watcher on the same machine, only to keep a record of which Chrome process runs
            # input and which runs output, and when (the evidence behind any stop the app logs).
            app = self.facts['companion']['app']
            g(f'nohup sh -c "sleep 3000 | {shlex.quote(app)}/Contents/MacOS/richos-companion watch --trace > {LABDIR}/watch.jsonl 2> {LABDIR}/watch.err" > /dev/null 2>&1 &')
            prompts = self.count('ShowPrompt')
            g(f'open -na "/Applications/Google Chrome.app" --args --user-data-dir={LABDIR}/chrome-profile --no-first-run --no-default-browser-check '
              f'--use-fake-ui-for-media-stream --allow-file-access-from-files --autoplay-policy=no-user-gesture-required about:blank')
            time.sleep(20)
            self.shot('chrome-idle.png')
            ev = {'chrome_install': inst.splitlines()[-1], 'tcc': tcc, 'served': served, 'tabs': self.chrome_title(),
                  'prompts_before': prompts, 'prompts_after_20s_idle': self.count('ShowPrompt')}
            self.facts['chrome'] = ev
            self.save()
            if 'fixture music' not in served:
                raise StepFailed('the fixture page is not in the guest: ' + served)
            if ev['prompts_after_20s_idle'] > prompts:
                raise StepFailed('an idle Chrome raised a prompt: ' + json.dumps(ev))
            return ev

        def browser_prompt(self):
            before_sessions = self.sessions()
            before = self.count('ShowPrompt')
            released = self.count('microphone released')
            self.open_tab('call.html')
            if not self.wait_log('ShowPrompt', 60, more_than=before):
                self.shot('prompt-missing.png')
                raise StepFailed('no prompt after a Chrome tab joined the fixture call; tabs: ' + self.chrome_title())
            seen = self.panel_says('Take Notes', 20)
            self.shot('prompt.png')
            time.sleep(14)  # the page reopens its microphone 5 s after joining, for 1.5 s
            ev = {'prompt_text_on_screen': seen, 'capture_processes_while_prompted': self.capture_pids(),
                  'sessions_before': before_sessions, 'sessions_while_prompted': self.sessions(),
                  'start_capture_logged': self.count('StartCapture'), 'tabs': self.chrome_title(),
                  'released_lines_before': released, 'released_lines_after': self.count('microphone released'),
                  'prompts_logged_for_this_call': self.count('ShowPrompt') - before,
                  'prompt_lines': self.g("grep -i 'ShowPrompt' " + shlex.quote(self.applog) + ' | tail -3')}
            self.facts['browser_prompt'] = ev
            self.save()
            if not seen:
                raise StepFailed('Take Notes is not on screen')
            if ev['released_lines_after'] > released or ev['prompts_logged_for_this_call'] != 1:
                raise StepFailed("the join's microphone reopening was taken for the end of the call and a new one: " + json.dumps(ev))
            if ev['capture_processes_while_prompted'] or ev['start_capture_logged'] or ev['sessions_while_prompted'] != before_sessions:
                raise StepFailed('something recorded before Take Notes was pressed: ' + json.dumps(ev))
            return ev

        def still_recording(self, label, seconds):
            stops = self.count('StopCapture')
            time.sleep(seconds)
            ev = {'label': label, 'stops_before': stops, 'stops_after': self.count('StopCapture'),
                  'capture_processes': self.capture_pids()}
            if ev['stops_after'] > stops or not ev['capture_processes']:
                raise StepFailed(f'the recording ended during "{label}": ' + json.dumps(ev))
            return ev

        def browser_record(self):
            self.panel_press('Take Notes')
            if not self.wait_log('StartCapture', 40):
                self.shot('no-recording.png')
                raise StepFailed('Take Notes pressed, no StartCapture in the log')
            shown = self.panel_says('Recording meeting', 40)
            self.shot('recording.png')
            self.g(f'nohup {LABDIR}/mac-audio-lab play {LABDIR}/near-track.wav --device "BlackHole 2ch" > {LABDIR}/near.log 2>&1 &')
            rows = [self.still_recording('speech playing', 20)]
            acks = {}
            n = self.send('mute')
            acks['mute'] = self.ack(n, 'mute', {'live': lambda v: bool(v) and int(v) >= 1, 'en': lambda v: v == '0', 'ctx': lambda v: v == 'running'})
            rows.append(self.still_recording('muted, 20 s', 20))
            self.shot('muted.png')
            n = self.send('unmute')
            acks['unmute'] = self.ack(n, 'unmute', {'live': lambda v: bool(v) and int(v) >= 1, 'en': lambda v: bool(v) and int(v) >= 1, 'ctx': lambda v: v == 'running'})
            rows.append(self.still_recording('unmuted', 10))
            rows.append(self.still_recording('unmuted and mic on, one more look', 10))
            ev = {'recording_text_on_screen': shown, 'sessions': self.sessions(), 'page_acks': acks, 'checks': rows}
            self.facts['browser_record'] = ev
            self.save()
            if not shown:
                raise StepFailed('"Recording meeting" never showed on screen')
            if not ev['sessions']:
                raise StepFailed('no session folder while recording: ' + json.dumps(ev))
            return ev

        def browser_hangup(self):
            stops = self.count('StopCapture')
            began = time.monotonic()
            n = self.send('hangup')
            ack = self.ack(n, 'hangup', {'live': lambda v: v == '0', 'ctx': lambda v: v == 'none'})
            saw = True
            ended = self.wait_log('StopCapture', 40, more_than=stops)
            took = round(time.monotonic() - began, 1)
            self.shot('after-hangup.png')
            tabs = self.chrome_title()
            ev = {'page_hung_up': saw, 'page_ack': ack, 'stop_capture_logged': ended, 'seconds_to_stop': took, 'tabs': tabs,
                  'stop_lines': self.g("grep -n 'StopCapture' " + shlex.quote(self.applog) + ' | tail -3')}
            self.facts['browser_hangup'] = ev
            self.save()
            if not (saw and ended):
                raise StepFailed('the recording did not end at hang-up: ' + json.dumps(ev))
            self.front_app('RichOS')
            if not self.wait_log('Rich said the notes', 900):
                self.shot('no-notes.png')
                raise StepFailed('Rich never posted the notes after the hang-up')
            time.sleep(5)
            self.shot('notes.png')
            ev['notes_in_conversation'] = {w: self.text_on_screen(w) for w in ('Decisions', 'Your actions', 'Open questions')}
            self.save()
            return ev

        def saved_retry(self):
            g = self.g
            model = Path(self.a.model).name
            models = self.home + '/.config/richos/models'
            keep = self.home + '/.config/richos/models-away'
            notes = self.count('Rich said the notes')
            starts = self.count('StartCapture')
            # another tab is already playing sound when this call joins (the case the join-window fix is for)
            self.open_tab('music.html')
            time.sleep(6)
            n = self.send('join')
            ev = {'join_ack': self.ack(n, 'join', {'live': lambda v: bool(v) and int(v) >= 1})}
            took = self.wait_log('StartCapture', 60, more_than=starts)
            ev['rejoin_recorded_without_asking'] = took
            if not took:
                self.shot('second-call-prompt.png')
                self.panel_press('Take Notes')
                if not self.wait_log('StartCapture', 40, more_than=starts):
                    raise StepFailed('the second call did not start recording')
            self.shot('second-recording.png')
            g(f'nohup {LABDIR}/mac-audio-lab play {LABDIR}/near-track.wav --device "BlackHole 2ch" > {LABDIR}/near2.log 2>&1 &')
            time.sleep(25)
            g(f'mkdir -p {shlex.quote(keep)} && mv {shlex.quote(models + "/" + model)} {shlex.quote(keep + "/")} && ls {shlex.quote(models)}')
            stops = self.count('StopCapture')
            n = self.send('hangup')
            ev['hangup_ack'] = self.ack(n, 'hangup', {'live': lambda v: v == '0', 'ctx': lambda v: v == 'none'})
            ev['stopped'] = self.wait_log('StopCapture', 60, more_than=stops)
            ev['music_tab_was_playing'] = True
            if not ev['stopped']:
                self.shot('second-not-stopped.png')
                raise StepFailed('the second call (another tab playing sound from before the join) did not end at hang-up: ' + json.dumps(ev))
            self.front_app('RichOS')
            ev['failure_in_log'] = self.wait_log('transcription failed', 240)
            time.sleep(5)
            self.shot('failed-card.png')
            ev['failure_text_on_screen'] = self.text_on_screen("couldn't prepare the transcript")
            ev['try_again_on_screen'] = bool(self.nodes('--title', 'Try again', '--role', 'AXButton', '--contains'))
            ev['log_tail'] = g("grep -n 'meeting notes' " + shlex.quote(self.applog) + ' | tail -8')
            g(f'mv {shlex.quote(keep + "/" + model)} {shlex.quote(models + "/")} && ls {shlex.quote(models)}')
            self.facts['saved_retry'] = ev
            self.save()
            if not (ev['failure_in_log'] and ev['failure_text_on_screen'] and ev['try_again_on_screen']):
                raise StepFailed('no failure message with Try again in the conversation: ' + json.dumps(ev))
            self.press('Try again')
            ev['retry_logged'] = self.wait_log('retrying a saved recording', 60)
            ev['notes_written'] = self.wait_log('Rich said the notes', 240, more_than=notes)
            zone = self.home + '/RichOS/corpus/companies/acme/evidence/meetings'
            ev['sessions_listing'] = g(f'ls -l {shlex.quote(zone)}/*/ 2>&1 | head -60')
            ev['pipeline_logs'] = g(f'for f in {shlex.quote(zone)}/*/pipeline.log; do echo "== $f"; tail -15 "$f"; done 2>&1 | head -80')
            time.sleep(5)
            self.shot('retried-notes.png')
            ev['try_again_gone'] = not self.nodes('--title', 'Try again', '--role', 'AXButton', '--contains')
            self.facts['saved_retry'] = ev
            self.save()
            if not (ev['retry_logged'] and ev['notes_written']):
                raise StepFailed('Try again did not write the notes from the saved recording: ' + json.dumps(ev))
            return ev

    return BriefWalk, StepFailed


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--model', type=Path, required=True, help='ggml-large-v3-turbo-q5_0.bin')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    bad = [s for s in steps if s not in STEPS]
    if bad:
        p.error('unknown step(s): ' + ', '.join(bad))
    Walk, StepFailed = build()
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
        walk.diag()
    except Exception as exc:  # the guest is removed by run-walk.py either way
        print('diag: ' + str(exc)[:200], flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
