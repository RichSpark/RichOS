#!/usr/bin/env python3
"""home-video-walk.py — "Make a video" on the home screen (round 77) on the real app: a video made
with each choice, saved through the Mac's own save window, and each saved file read back.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      home-video-walk.py --out DIR [--size 312] [--steps seed,button,voice,film]

run-walk.py passes the owned VM name as the first argument.

THE ORDER (CEO, 2026-11-14): "`richos-hq/design/mockups/rounds/round-77` is great. Get an engineer
on Fable to build it." The app's part is `ui/home-video.js` and `src-tauri/src/home_video.rs`.

IT PLAYS NO SOUND AND OPENS NO MICROPHONE. The app is relaunched with RICHOS_VIDEO_INPUT_TONE=1, so
the "Microphone" choice records a tone made in the page that goes into the file only
(`home_video.rs`), and the sheet's player is muted for it. The saved files are read on the host with
ffprobe and ffmpeg, which decode them and play nothing.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  seed    one fictional company's repository of about --size documents is written in the guest
          (home-standard-repos.sh), registered with a team of three (seed-team.sh), and the app is
          relaunched until the left column says that many memories
  button  "Make a video" stands in the top right corner beside the settings button (frame
          1-button); pressed, it opens the three choices (frame 2-choices)
  voice   Microphone On, "When I stop it", pointer Shown: Start recording, the count (frame
          4-count), the red button with the time while the real mouse moves over the picture (frame
          5-recording), Stop, the sheet with the video playing (frame 6-sheet), Save video, the
          Mac's save window with the suggested name (frame 7-save-window), Save, "Saved to …" (frame
          8-saved). The file is pulled out of the guest and read back.
  film    Microphone Off, "By itself", pointer Hidden: the film plays once with the numbers, the
          team and the buttons off the screen (frame 9-film), the recording ends by itself, and the
          sheet's video is saved under another name typed into the save window. Pulled and read back.

READ BACK means: ffprobe's streams and length (AAC only with the microphone, and that sound is not
silence), everything in the file decoded by ffmpeg with no error, the length against what the sheet
said, and, where the file has a picture, frames taken out of it into --out for a person to look at.

THE PICTURE IS NOT IN A GUEST'S FILE, and this walk does not ask for it unless --expect-picture is
given. The test VM guest has no hardware H.264 encoder (measured 2026-11-14 in a guest on macOS
15.7.7: VideoToolbox answers -12903 when hardware is required), and the webview's recorder then
writes the sound and no picture: with the microphone the saved file is AAC alone, without it there
is no file and the sheet says "This Mac could not record the window…". Every Mac a user runs has
the encoder. The picture in the file is proved on the host by ui/tests/home-video.js, which records
through the same code in headless WebKit and reads the H.264 back. What the guest proves is the
button, the three choices, the count, Stop, the film ending by itself, the sheet and the Mac's save
window, on the real app.

Evidence (frames, each file's facts, every step's result) is written to --out/report.json. Exit 0
when every step passes. It quits nothing itself: run-walk.py quits the app by its pid and deletes
the clone (CEO 54).
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

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stages = load('loro_stages_walk', 'loro-stages-walk.py')
standard = stages.standard  # the one copy of home-standard-walk.py: its StepFailed is the one raised and caught
StepFailed, DATA = standard.StepFailed, standard.DATA

STEPS = ['seed', 'button', 'voice', 'film']
TONE = {'RICHOS_VIDEO_INPUT_TONE': '1'}


def host(args, timeout=300):
    r = subprocess.run([str(a) for a in args], capture_output=True, text=True, timeout=timeout)
    return r.returncode, r.stdout, r.stderr


def read_back(file, out, tag):
    """What a saved video is: its streams, its length, whether every frame decodes, how loud its
    sound is, and three frames out of it (1 s in, the middle, 1 s before the end) as JPEGs in `out`."""
    code, text, err = host(['ffprobe', '-v', 'error', '-show_streams', '-show_format', '-of', 'json', file])
    if code:
        raise StepFailed('ffprobe could not read ' + file.name + ': ' + err[-300:])
    probe = json.loads(text)
    video = next((s for s in probe['streams'] if s['codec_type'] == 'video'), None)
    audio = next((s for s in probe['streams'] if s['codec_type'] == 'audio'), None)
    if not video and not audio:
        raise StepFailed(file.name + ' has neither a picture nor sound in it')
    seconds = float(probe['format']['duration'])
    code, text, err = host(['ffmpeg', '-v', 'error', '-i', file, '-f', 'null', '-'])
    if code or err.strip():
        raise StepFailed(file.name + ' does not decode cleanly: ' + err[-300:])
    facts = {'file': file.name, 'bytes': file.stat().st_size, 'seconds': round(seconds, 2),
             'video': {'codec': video['codec_name'], 'width': video['width'], 'height': video['height'], 'frames': int(video.get('nb_frames') or 0)} if video else None,
             'audio': None, 'decodes': True, 'frames': []}
    if audio:
        code, text, err = host(['ffmpeg', '-v', 'info', '-i', file, '-map', '0:a:0', '-af', 'volumedetect', '-f', 'null', '-'])
        level = re.search(r'mean_volume: (-?[\d.]+) dB', err)
        facts['audio'] = {'codec': audio['codec_name'], 'sample_rate': int(audio['sample_rate']), 'mean_volume_db': float(level.group(1)) if level else None}
    for name, at in (('start', 1.0), ('middle', seconds / 2), ('end', max(0.0, seconds - 1.0))) if video else ():
        frame = out / ('%s-file-%s.jpg' % (tag, name))
        code, text, err = host(['ffmpeg', '-v', 'error', '-y', '-ss', '%.2f' % at, '-i', file, '-frames:v', '1', '-q:v', '3', frame])
        if code or not frame.exists():
            raise StepFailed('no frame at %.1f s out of %s: %s' % (at, file.name, err[-300:]))
        facts['frames'].append(frame.name)
    return facts


class Walk(stages.Walk):
    def __init__(self, vm, out, size, expect_picture=False):
        super().__init__(vm, out, [size])
        self.size = size
        self.expect_picture = expect_picture

    def launch_with_tone(self, what):
        """loro-stages-walk.py's launch, with the made-up microphone in the app's environment."""
        how = []
        for attempt in (1, 2, 3):
            relaunch(self.vm, environment=TONE)
            try:
                self.until(self.window, 40, 'no window')
                how.append('window')
            except StepFailed:
                app = guest(self.vm, 'find ' + shlex.quote(self.payload) + ' -maxdepth 1 -name "*.app" -type d -print').splitlines()[0]
                guest(self.vm, 'open ' + shlex.quote(app) + ' --env RICHOS_VIDEO_INPUT_TONE=1')
                how.append('reopened')
                try:
                    self.until(self.window, 40, 'no window')
                except StepFailed:
                    continue
            try:
                self.to_home()
                return how
            except StepFailed:
                continue
        log = guest(self.vm, 'tail -n 15 ' + shlex.quote(self.payload + '/app.log') + ' 2>/dev/null || true')
        raise StepFailed('the home screen did not come up ' + what + ' in three launches (' + ', '.join(how) + '). app.log: ' + log[-900:])

    def press(self, *selector):
        code, text = standard.run([HERE / 'ax.sh', self.vm, 'click', *selector, '--first'], 60)
        if code:
            raise StepFailed('could not press ' + ' '.join(selector) + ': ' + text[-300:])

    def make(self):
        """The button itself (a pop-up button to the Mac: it opens the choices) and what it reads now:
        "Make a video", "Cancel" during the count, "Stop recording" while it records."""
        node = self.node('--id', 'home-video')
        return node, (str(node.get('title') or node.get('desc') or '') if node else '')

    def recording(self):
        return self.make()[1] == 'Stop recording'

    def button(self, title):
        return self.node('--title', title, '--role', 'AXButton')

    def choice(self, title):
        """One of a row's two choices: a pressed-or-not button, which the Mac calls a checkbox."""
        return self.node('--title', title, '--role', 'AXCheckBox')

    def words(self, fragment):
        node = self.text(fragment)
        return ' '.join(str(node.get(k) or '') for k in ('value', 'title')).strip() if node else None

    # ---- the steps ------------------------------------------------------------------------
    def seed(self):
        standard.run([HERE / 'guest.sh', self.vm, '--push', HERE / 'home-standard-repos.sh', self.payload + '/'], 120)
        standard.run([HERE / 'guest.sh', self.vm, '--push', HERE / 'seed-team.sh', self.payload + '/'], 120)
        made = self.write_repository(self.size)
        data = self.home + '/' + DATA
        registry = {'version': 1, 'entities': [{'id': 'alder', 'display_name': 'Alder Works', 'roots': [self.repos + '/alder']}]}
        guest(self.vm, 'mkdir -p ' + shlex.quote(data) + ' && printf %s ' + shlex.quote(json.dumps(registry)) + ' > ' + shlex.quote(data + '/entities.json'))
        team = guest(self.vm, 'sh ' + shlex.quote(self.payload + '/seed-team.sh') + ' ' + shlex.quote(data) + ' 3')
        want = self.documents(self.size)
        launches, count = [], None
        for _ in range(4):
            launches += self.launch_with_tone('with a company and a team')
            try:
                count = self.until(lambda: (self.memories() or 0) >= want and self.memories(), 50, 'not yet')
                break
            except StepFailed:
                continue
        if count is None:
            raise StepFailed('the left column never reached %d memories (%s)' % (want, ', '.join(launches)))
        time.sleep(25)  # the lights have arrived
        self.pointer('%d,%d' % (700, 760), '%d,%d' % (700, 760))  # a press on the picture: the first hire's sentence leaves
        time.sleep(4)
        return {'repository': made, 'teammates': team.strip(), 'launches': launches, 'memories': self.memories() or count}

    def button_step(self):
        make = self.until(lambda: self.node('--id', 'home-video'), 30, 'Make a video is not on the home screen')
        settings = self.node('--id', 'set-btn')
        frame = self.shot('1-button')
        result = {'make_a_video': self.rect(make), 'settings': self.rect(settings), 'frame': frame,
                  'frame_reads_make_a_video': self.reads(frame, 'Make a video', self.rect(make))}
        if settings and not (abs(make['y'] - settings['y']) <= 2 and 0 < settings['x'] - (make['x'] + make['w']) <= 20):
            raise StepFailed('Make a video is not beside the settings button: ' + json.dumps(result))
        self.press('--id', 'home-video')
        self.until(lambda: self.button('Start recording'), 15, 'the choices did not open')
        time.sleep(1)
        result['choices_frame'] = self.shot('2-choices')
        result['rows'] = [bool(self.text(row)) for row in ('Microphone', 'Recording stops', 'Mouse pointer')]
        chosen = {c: self.choice(c) for c in ('Off', 'On', 'By itself', 'When I stop it', 'Shown', 'Hidden')}
        result['choices'] = [bool(n) for n in chosen.values()]
        # as first opened: the microphone off, it stops when he stops it, the pointer shown
        result['as_first_opened'] = [c for c, n in chosen.items() if n and str(n.get('value')) == '1']
        if not all(result['rows'] + result['choices']) or result['as_first_opened'] != ['Off', 'When I stop it', 'Shown']:
            raise StepFailed('the three rows and their six choices are not as first opened: ' + json.dumps(result))
        guest(self.vm, 'osascript -e ' + shlex.quote('tell application "System Events" to key code 53'))  # Escape: one step back
        time.sleep(1.5)
        result['escape_closed_the_choices'] = not self.button('Start recording')
        result['still_on_the_home_screen'] = bool(self.node('--id', 'home-enter'))
        if not (result['escape_closed_the_choices'] and result['still_on_the_home_screen']):
            raise StepFailed('Escape did not close the choices alone: ' + json.dumps(result))
        return result

    def choose(self, *titles):
        self.press('--id', 'home-video')
        self.until(lambda: self.button('Start recording'), 15, 'the choices did not open')
        for title in titles:
            self.press('--title', title, '--role', 'AXCheckBox')
            time.sleep(0.8)
        pressed = [t for t in titles if str((self.choice(t) or {}).get('value')) == '1']
        if pressed != list(titles):
            raise StepFailed('chose %s and the choices read %s' % (', '.join(titles), ', '.join(pressed) or 'none of them'))

    def to_the_sheet(self, seconds):
        """The sheet is up (Record again is on it whatever it holds); answers whether it has a video to save."""
        self.until(lambda: self.button('Record again'), seconds, 'the sheet never opened')
        time.sleep(2.5)  # the sheet has faded in and the video is playing
        return bool(self.button('Save video'))

    def save(self, tag, typed=None):
        """Save video, the Mac's own save window, Save. `typed` replaces the suggested name's stem."""
        self.press('--title', 'Save video', '--role', 'AXButton')
        self.until(lambda: self.button('Save'), 20, "the Mac's save window did not open")
        time.sleep(1.5)
        result = {'save_window_frame': self.shot(tag + '-save-window')}
        if typed:
            guest(self.vm, 'osascript -e ' + shlex.quote('tell application "System Events" to keystroke "%s"' % typed))
            time.sleep(1)
        self.press('--title', 'Save', '--role', 'AXButton')
        said = self.until(lambda: self.words('Saved to '), 30, 'the sheet never said where the video went')
        time.sleep(1)
        result['said'] = said
        result['saved_frame'] = self.shot(tag + '-saved')
        match = re.search(r'Saved to (.+) as (\S+\.(?:mp4|webm))', said)
        if not match:
            raise StepFailed('what the sheet said is not "Saved to <folder> as <file>": ' + said)
        result['folder'], result['name'] = match.group(1), match.group(2)
        found = guest(self.vm, 'find /Users/admin ' + shlex.quote(self.home) + ' -name ' + shlex.quote(result['name'])
                      + ' -mmin -10 -not -path "*/Library/*" 2>/dev/null | head -1').strip()
        if not found:
            raise StepFailed(result['name'] + ' is not in the guest')
        result['guest_path'] = found
        result['app_log'] = guest(self.vm, 'grep -h "home video: saved" ' + shlex.quote(self.payload) + '/*.log 2>/dev/null | tail -1 || true').strip()
        if Path(found).parent.name != result['folder']:
            raise StepFailed('the sheet says %s and the file is in %s' % (result['folder'], found))
        local = self.out / (tag + '-' + result['name'])
        code, text = standard.run([HERE / 'guest.sh', self.vm, '--pull', found, local], 300)
        if code or not local.exists():
            raise StepFailed('could not pull ' + found + ': ' + text[-300:])
        result['read_back'] = read_back(local, self.out, tag)
        local.unlink()  # the frames and the facts are the evidence; the video is made again by a run
        return result

    def voice(self):
        self.choose('On', 'When I stop it', 'Shown')
        time.sleep(2)
        result = {'microphone_frame': self.shot('3-microphone-on')}
        self.press('--title', 'Start recording', '--role', 'AXButton')
        result['count_frame'] = self.shot('4-count')
        self.until(self.recording, 20, 'the button never became Stop')
        began = time.monotonic()
        make = self.node('--id', 'home-video')
        # the real mouse over the picture, the way a hand moves through it
        x, y = make['x'] - 420, make['y'] + 330
        self.pointer('%d,%d' % (x, y), '%d,%d' % (x + 160, y + 90), '%d,%d' % (x + 40, y + 180), hover=True)
        result['pointer_rests_at'] = [x + 40, y + 180]
        result['recording_frame'] = self.shot('5-recording')
        time.sleep(max(0, 9 - (time.monotonic() - began)))
        self.press('--id', 'home-video')
        result['pressed_stop_after_s'] = round(time.monotonic() - began, 1)
        if not self.to_the_sheet(30):
            raise StepFailed('the sheet has no video to save: ' + str(self.words('could not record')))
        result['sheet_frame'] = self.shot('6-sheet')
        result['sheet_says'] = self.words(' · with your voice · ')
        if not result['sheet_says'] or 'with the mouse pointer' not in result['sheet_says']:
            raise StepFailed('the sheet does not say the video has his voice and the pointer: ' + str(result['sheet_says']))
        result.update(self.save('7-voice'))
        facts = result['read_back']
        said = re.match(r'(\d+):(\d\d)', result['sheet_says'])
        said_s = int(said.group(1)) * 60 + int(said.group(2))
        result['sheet_seconds'] = said_s
        problems = []
        result['picture_in_the_file'] = bool(facts['video'])
        if facts['video'] and facts['video']['codec'] != 'h264':
            problems.append('the picture is ' + facts['video']['codec'])
        if self.expect_picture and not facts['video']:
            problems.append('no picture in the file')
        if not facts['audio'] or facts['audio']['codec'] != 'aac':
            problems.append('no AAC sound in a video made with the microphone')
        elif facts['audio']['mean_volume_db'] is None or facts['audio']['mean_volume_db'] < -60:
            problems.append('its sound is silence (%s dB)' % facts['audio']['mean_volume_db'])
        if abs(facts['seconds'] - said_s) > 1.5:
            problems.append('it is %.1f s long and the sheet said %d s' % (facts['seconds'], said_s))
        if not re.fullmatch(r'RichOS-(day-\d+-)?\d+-memor(y|ies)\.mp4', result['name']):
            problems.append('its name is ' + result['name'])
        if problems:
            raise StepFailed('; '.join(problems) + ' ' + json.dumps(facts))
        self.press('--title', 'Done', '--role', 'AXButton')
        time.sleep(1.5)
        result['done_closed_the_sheet'] = not self.button('Record again')
        return result

    def film(self):
        self.choose('Off', 'By itself', 'Hidden')
        time.sleep(1)
        result = {'choices_frame': self.shot('8-film-choices')}
        make = self.node('--id', 'home-video')
        self.press('--title', 'Start recording', '--role', 'AXButton')
        self.until(self.recording, 20, 'the button never became Stop')
        began = time.monotonic()
        x, y = make['x'] - 420, make['y'] + 330
        self.pointer('%d,%d' % (x, y), '%d,%d' % (x + 60, y + 60), hover=True)  # over the picture, and not in the file
        time.sleep(4)
        result['film_frame'] = self.shot('9-film')
        result['numbers_off_the_screen'] = not self.text('sources understood')
        result['watch_it_grow_off_the_screen'] = not self.node('--id', 'home-grow') and not self.node('--id', 'home-grow-solo')
        has_video = self.to_the_sheet(60)
        result['ended_by_itself_after_s'] = round(time.monotonic() - began, 1)
        result['sheet_frame'] = self.shot('10-film-sheet')
        result['sheet_says'] = self.words(' · no sound · ')
        if not result['sheet_says'] or 'no mouse pointer' not in result['sheet_says']:
            raise StepFailed('the sheet does not say the video has no sound and no pointer: ' + str(result['sheet_says']))
        result['numbers_back'] = bool(self.text('sources understood'))
        if not 13 <= result['ended_by_itself_after_s'] <= 40:
            raise StepFailed('the film ended after %.1f s' % result['ended_by_itself_after_s'])
        if not has_video:
            # no picture and no sound is no file: what the sheet says then, and its two ways on
            result['picture_in_the_file'] = False
            result['sheet_says_no_file'] = self.words('could not record the window')
            if self.expect_picture or not result['sheet_says_no_file']:
                raise StepFailed('the film left no video to save: ' + json.dumps(result))
            self.press('--title', 'Discard', '--role', 'AXButton')
            time.sleep(1.5)
            result['discard_closed_the_sheet'] = not self.button('Record again')
            return result
        result.update(self.save('11-film', typed='RichOS-the-film'))
        facts = result['read_back']
        problems = []
        result['picture_in_the_file'] = bool(facts['video'])
        if not facts['video'] or facts['video']['codec'] != 'h264':
            problems.append('the picture is ' + (facts['video']['codec'] if facts['video'] else 'not there'))
        if facts['audio']:
            problems.append('there is sound in a video made with the microphone off')
        if not 13 <= facts['seconds'] <= 24:
            problems.append('the film is %.1f s long' % facts['seconds'])
        if result['name'] != 'RichOS-the-film.mp4':
            problems.append('the name typed into the save window was not kept: ' + result['name'])
        if problems:
            raise StepFailed('; '.join(problems) + ' ' + json.dumps(facts))
        self.press('--title', 'Done', '--role', 'AXButton')
        return result


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--size', type=int, default=312, help='about how many memories his picture has')
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--expect-picture', action='store_true', help='fail when a saved file has no picture (a machine with a hardware H.264 encoder; a guest has none)')
    a = p.parse_args()
    steps = [s for s in a.steps.split(',') if s]
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out, a.size, a.expect_picture)
    report = {'vm': a.vm, 'steps': {}}
    failed = False
    for step in steps:
        began = time.monotonic()
        try:
            report['steps'][step] = {'pass': True, 'result': getattr(walk, 'button_step' if step == 'button' else step)()}
        except (StepFailed, subprocess.TimeoutExpired, RuntimeError, KeyError, ValueError, TypeError) as exc:
            report['steps'][step] = {'pass': False, 'why': str(exc)[-1500:]}
            failed = True
            try:
                walk.shot('failed-' + step)
                code, tree = standard.run([HERE / 'ax.sh', a.vm, 'tree'], 60)
                (a.out / ('failed-' + step + '.tree')).write_text(tree + '\n')
                log = guest(a.vm, 'tail -n 60 ' + shlex.quote(walk.payload + '/app.log') + ' 2>/dev/null || true')
                (a.out / ('failed-' + step + '.log')).write_text(log + '\n')
            except Exception:
                pass
        report['steps'][step]['seconds'] = round(time.monotonic() - began, 1)
        print(step, 'PASS' if report['steps'][step]['pass'] else 'FAIL: ' + report['steps'][step]['why'][:400], flush=True)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        if failed:
            break  # a later step needs the one before it
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
