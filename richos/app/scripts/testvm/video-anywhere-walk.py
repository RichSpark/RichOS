#!/usr/bin/env python3
"""video-anywhere-walk.py — "Make a video", anywhere in the app (round 79), on the real app: a
recording started in a conversation, carried through other screens, stopped on the home screen,
saved through the Mac's own save window, and the saved file read back.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      video-anywhere-walk.py --out DIR [--size 312] [--steps seed,button,record]

run-walk.py passes the owned VM name as the first argument.

THE ORDER (CEO, 2026-11-15): "Get someone on Fable to build round 79." His words for the feature:
"We let the user start a screen recording anywhere in the app. This will record whatever they
want to record/show within the app." And, the same day, on the round's mockup: "I would expect the
screen recording to record the text highlighting as well." The app's part is
`ui/video-anywhere.js`; the save window and the made-up microphone are `src-tauri/src/home_video.rs`.

IT PLAYS NO SOUND AND OPENS NO MICROPHONE, as home-video-walk.py (whose seed, save and read-back
this walk uses): the app is relaunched with RICHOS_VIDEO_INPUT_TONE=1, so the "Microphone" choice
records a tone made in the page that goes into the file only, and the sheet's player is muted for
it. The saved file is read on the host with ffprobe and ffmpeg, which decode it and play nothing.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  seed    home-video-walk.py's: a company's repository of about --size documents, a team of three,
          the app relaunched until the home screen says that many memories
  button  Talk to Rich (the conversation), Settings, "Make a video" in its heading at its right
          end (frame 1-button); pressed, it opens two choices, Microphone and Mouse pointer, and
          no choice for when the recording stops (frame 2-choices); Escape closes them and
          Settings stays
  record  Microphone On, pointer Shown, Start recording: Settings closes, the count (frame
          3-count), the red Stop beside the settings button (frame 4-recording); the RichPass
          page (frame 5-another-page), where the real mouse is dragged over a line of text,
          which highlights it (frame 6-highlighted); Settings, whose heading holds the same red
          Stop (frame 7-in-settings); Escape, which closes Settings and does not stop the
          recording; the home screen, where Stop stands in place of the home screen's own button
          (frame 8-home); Stop; the sheet (frame 9-sheet); Save video, the Mac's save window
          (frame 10-anywhere-save-window), Save, "Saved to …" (frame 10-anywhere-saved). The file
          is pulled out of the guest and read back.
  still   (not run unless named) a recording of 25 s in which nothing is pressed, saved and read
          back: how long a guest's file is when the walk leaves the app alone while it records

READ BACK is home-video-walk.py's `read_back`: ffprobe's streams and length (AAC, and that sound
is not silence), everything in the file decoded with no error, the name saying when it was made,
and, where the file has a picture, its length against what the sheet said and against how long
the walk recorded, and frames taken out of it into --out for a person to look at.

A GUEST'S FILE HAS NO PICTURE AND 10 s OF SOUND, and this walk asks for more only with
--expect-picture. The guest has no hardware H.264 encoder (home-video-walk.py's note), and the
webview's recorder then writes the sound and no picture, and closes that sound at 10.03 s
whatever is recorded: measured 2026-11-15 in two guests, a recording of 48 s that went through
four screens and one of 27 s in which nothing was pressed (the `still` step) both left a file of
10.03 s. On this Mac's own encoder the same code writes a recording of 30 s whole (30.1 s of
H.264 and of AAC, headless WebKit). The picture in the file (every screen he went to, the
pointer, the text he highlighted, every line of a text box) and its length are proved on the host
by ui/tests/video-anywhere.js, which records through the same code in headless WebKit and reads
the H.264 back. What the guest proves is the real app: the button, the choices, the count, Stop on
every screen, the sheet and the Mac's save window, and a real file with the recording's sound.

Evidence (frames, the file's facts, every step's result) is written to --out/report.json. Exit 0
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
from relaunch import guest  # noqa: E402


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


home_video = load('home_video_walk', 'home-video-walk.py')
standard = home_video.standard
StepFailed = home_video.StepFailed

STEPS = ['seed', 'button', 'record', 'still']
NAME = r'RichOS-\d{4}-\d\d-\d\d-at-\d\d\.\d\d\.\d\d\.mp4'


class Walk(home_video.Walk):
    def says(self, element_id):
        node = self.node('--id', element_id)
        return str(node.get('title') or node.get('desc') or '') if node else None

    def recording(self):
        return self.says('video-anywhere-stop') == 'Stop recording'

    def settings_open(self):
        # closed, the panel is still in the Mac's tree, as an empty group 10 high under the button
        menu = self.node('--id', 'set-menu')
        return bool(menu and (menu.get('h') or 0) > 100)

    def open_settings(self):
        if not self.settings_open():
            self.press('--id', 'set-btn')
        self.until(self.settings_open, 15, 'Settings did not open')
        time.sleep(1)  # it has risen into place

    def escape(self):
        self.key(53)
        time.sleep(1.5)

    # ---- the steps ------------------------------------------------------------------------
    def to_the_conversation(self):
        """Talk to Rich, from the home screen, and the conversation's own first questions declined."""
        self.to_home()
        self.press('--id', 'home-enter')
        self.until(lambda: not self.node('--id', 'home-enter') or None, 20, 'Talk to Rich did not open the conversation')
        time.sleep(2)
        # a new user's conversation asks its own questions, one after the other ("Where should I keep
        # what you tell me?", "Which company is this copy of Rich for?"), and each covers the app
        for _ in range(5):
            self.decline_sheets()
            time.sleep(4)
            if not self.button('Not now'):
                return
        raise StepFailed('a sheet still covers the conversation after five rounds of Not now')

    def button_step(self):
        self.to_the_conversation()
        result = {'nothing_of_the_recorder_before_settings': not self.node('--id', 'video-anywhere') and not self.node('--id', 'video-anywhere-stop')}
        self.open_settings()
        make = self.until(lambda: self.node('--id', 'video-anywhere'), 15, 'Make a video is not in Settings')
        menu = self.node('--id', 'set-menu')
        result.update({'make_a_video': self.rect(make), 'reads': self.says('video-anywhere'), 'settings': self.rect(menu), 'frame': self.shot('1-button')})
        result['frame_reads_make_a_video'] = self.reads(result['frame'], 'Make a video', self.rect(make))
        # in the heading (the top 60px of the panel), at its right end
        at_the_right = 0 <= (menu['x'] + menu['w']) - (make['x'] + make['w']) <= 40
        in_the_heading = menu['y'] <= make['y'] and make['y'] + make['h'] <= menu['y'] + 60
        if not (result['nothing_of_the_recorder_before_settings'] and result['reads'] == 'Make a video' and at_the_right and in_the_heading):
            raise StepFailed('Make a video is not in the heading of Settings at its right end: ' + json.dumps(result))
        self.press('--id', 'video-anywhere')
        self.until(lambda: self.button('Start recording'), 15, 'the choices did not open')
        time.sleep(1)
        result['choices_frame'] = self.shot('2-choices')
        result['rows'] = [bool(self.text(row)) for row in ('Microphone', 'Mouse pointer')]
        chosen = {c: self.choice(c) for c in ('Off', 'On', 'Shown', 'Hidden')}
        result['choices'] = [bool(n) for n in chosen.values()]
        result['as_first_opened'] = [c for c, n in chosen.items() if n and str(n.get('value')) == '1']
        # "there won't be an option for when recording stops"
        result['a_choice_for_when_it_stops'] = [t for t in ('Recording stops', 'By itself', 'When I stop it') if self.text(t) or self.choice(t)]
        if not all(result['rows'] + result['choices']) or result['as_first_opened'] != ['Off', 'Shown'] or result['a_choice_for_when_it_stops']:
            raise StepFailed('the two rows and their four choices are not as first opened: ' + json.dumps(result))
        self.escape()
        result['escape_closed_the_choices'] = not self.button('Start recording')
        result['settings_stayed'] = self.settings_open()
        if not (result['escape_closed_the_choices'] and result['settings_stayed']):
            raise StepFailed('Escape did not close the choices alone: ' + json.dumps(result))
        return result

    def choose(self, *titles):
        self.open_settings()
        if not self.button('Start recording'):
            self.press('--id', 'video-anywhere')
        self.until(lambda: self.button('Start recording'), 15, 'the choices did not open')
        for title in titles:
            self.press('--title', title, '--role', 'AXCheckBox')
            time.sleep(0.8)
        pressed = [t for t in titles if str((self.choice(t) or {}).get('value')) == '1']
        if pressed != list(titles):
            raise StepFailed('chose %s and the choices read %s' % (', '.join(titles), ', '.join(pressed) or 'none of them'))

    def still(self):
        """A recording of 25 s in which nothing is pressed and nothing moves, saved and read back:
        how long a guest's file is when the walk does nothing to the app while it records."""
        self.to_the_conversation()
        self.choose('On', 'Shown')
        time.sleep(2)
        self.press('--title', 'Start recording', '--role', 'AXButton')
        self.until(self.recording, 20, 'the button beside the settings button never became Stop')
        began = time.monotonic()
        time.sleep(25)
        self.press('--id', 'video-anywhere-stop')
        result = {'pressed_stop_after_s': round(time.monotonic() - began, 1)}
        if not self.to_the_sheet(30):
            raise StepFailed('the sheet has no video to save: ' + str(self.words('could not record')))
        result['sheet_says'] = self.words(' · with your voice · ')
        result.update(self.save('11-still'))
        result['seconds_in_the_file'] = result['read_back']['seconds']
        self.press('--title', 'Done', '--role', 'AXButton')
        time.sleep(1.5)
        return result

    def record(self):
        self.choose('On', 'Shown')
        time.sleep(2)
        self.press('--title', 'Start recording', '--role', 'AXButton')
        result = {'count_frame': self.shot('3-count'), 'settings_closed_for_the_count': not self.settings_open()}
        self.until(self.recording, 20, 'the button beside the settings button never became Stop')
        began = time.monotonic()
        stop, settings = self.node('--id', 'video-anywhere-stop'), self.node('--id', 'set-btn')
        result.update({'stop': self.rect(stop), 'settings_button': self.rect(settings)})
        if not (abs((stop['y'] + stop['h'] / 2) - (settings['y'] + settings['h'] / 2)) <= 3 and 0 < settings['x'] - (stop['x'] + stop['w']) <= 20):
            raise StepFailed('Stop is not beside the settings button: ' + json.dumps(result))
        result['recording_frame'] = self.shot('4-recording')
        # every screen he goes to: a tool's page, Settings, the home screen
        self.press('--id', 'nr-pass')
        time.sleep(3)
        result['another_page'] = {'still_recording': self.recording(), 'frame': self.shot('5-another-page')}
        # a line of that page highlighted with the real mouse, the way a hand selects text (a new
        # user's conversation has no line in it yet)
        line = self.text('Safer Than Any Password Manager') or self.text('unbreakable passwords')
        if not line or line.get('w', 0) < 80:
            raise StepFailed('the RichPass page has no line of text to highlight: ' + json.dumps(line))
        y = int(line['y'] + line['h'] / 2)
        self.pointer('%d,%d' % (line['x'] + 4, y), '%d,%d' % (line['x'] + min(int(line['w']) - 6, 420), y))
        time.sleep(1.5)
        result['highlighted'] = {'line': self.rect(line), 'still_recording': self.recording(), 'frame': self.shot('6-highlighted')}
        self.open_settings()
        result['in_settings'] = {'the_heading_button_reads': self.says('video-anywhere'), 'frame': self.shot('7-in-settings')}
        self.escape()
        result['escape'] = {'closed_settings': not self.settings_open(), 'still_recording': self.recording()}
        self.press('--id', 'nr-home')
        self.until(lambda: self.node('--id', 'home-enter'), 20, 'the home screen did not come up during the recording')
        time.sleep(3)
        result['home'] = {'still_recording': self.recording(), 'the_home_screens_own_button_gave_its_place': not self.node('--id', 'home-video'), 'frame': self.shot('8-home')}
        problems = []
        if not result['settings_closed_for_the_count']:
            problems.append('Settings stayed open for the count')
        if not result['another_page']['still_recording']:
            problems.append('the RichPass page ended the recording')
        if not result['highlighted']['still_recording']:
            problems.append('highlighting a line ended the recording')
        if result['in_settings']['the_heading_button_reads'] != 'Stop recording':
            problems.append('in Settings the button reads %s' % result['in_settings']['the_heading_button_reads'])
        if not (result['escape']['closed_settings'] and result['escape']['still_recording']):
            problems.append('Escape did not close Settings alone')
        if not (result['home']['still_recording'] and result['home']['the_home_screens_own_button_gave_its_place']):
            problems.append('on the home screen the recording ended or two buttons stand there')
        if problems:
            raise StepFailed('; '.join(problems) + ' ' + json.dumps(result))
        self.press('--id', 'video-anywhere-stop')
        result['pressed_stop_after_s'] = round(time.monotonic() - began, 1)
        if not self.to_the_sheet(30):
            raise StepFailed('the sheet has no video to save: ' + str(self.words('could not record')))
        result['sheet_frame'] = self.shot('9-sheet')
        result['still_on_the_home_screen'] = bool(self.node('--id', 'home-enter'))
        result['sheet_says'] = self.words(' · with your voice · ')
        if not result['sheet_says'] or 'with the mouse pointer' not in result['sheet_says']:
            raise StepFailed('the sheet does not say the video has his voice and the pointer: ' + str(result['sheet_says']))
        result.update(self.save('10-anywhere'))
        facts = result['read_back']
        said = re.match(r'(\d+):(\d\d)', result['sheet_says'])
        said_s = int(said.group(1)) * 60 + int(said.group(2))
        result['sheet_seconds'] = said_s
        result['picture_in_the_file'] = bool(facts['video'])
        if facts['video'] and facts['video']['codec'] != 'h264':
            problems.append('the picture is ' + facts['video']['codec'])
        if self.expect_picture and not facts['video']:
            problems.append('no picture in the file')
        if not facts['audio'] or facts['audio']['codec'] != 'aac':
            problems.append('no AAC sound in a video made with the microphone')
        elif facts['audio']['mean_volume_db'] is None or facts['audio']['mean_volume_db'] < -60:
            problems.append('its sound is silence (%s dB)' % facts['audio']['mean_volume_db'])
        # the whole way through the app is in it: as long as the sheet said and as the walk recorded,
        # give or take the presses. Asked of a file with a picture only: a guest's recorder, which
        # has no picture to write, closes its sound after 10 s whatever is recorded (the `still` step).
        result['whole'] = abs(facts['seconds'] - said_s) <= 1.5 and abs(facts['seconds'] - result['pressed_stop_after_s']) <= 6
        if facts['video'] and not result['whole']:
            problems.append('it is %.1f s long, the sheet said %d s and the walk recorded for %.1f s' % (facts['seconds'], said_s, result['pressed_stop_after_s']))
        if not facts['video'] and facts['seconds'] < min(9.5, said_s - 1.5):
            problems.append('it is %.1f s long, under the 10 s of sound a guest keeps' % facts['seconds'])
        if not re.fullmatch(NAME, result['name']):
            problems.append('its name is ' + result['name'])
        if not result['still_on_the_home_screen']:
            problems.append('the home screen went away under the sheet')
        if problems:
            raise StepFailed('; '.join(problems) + ' ' + json.dumps(facts))
        self.press('--title', 'Done', '--role', 'AXButton')
        time.sleep(1.5)
        result['done_closed_the_sheet'] = not self.button('Record again')
        result['stop_is_gone'] = not self.node('--id', 'video-anywhere-stop')
        result['the_home_screens_own_button_is_back'] = bool(self.node('--id', 'home-video'))
        if not (result['done_closed_the_sheet'] and result['stop_is_gone'] and result['the_home_screens_own_button_is_back']):
            raise StepFailed('Done did not put everything back: ' + json.dumps(result))
        return result


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--size', type=int, default=312, help='about how many memories his picture has')
    p.add_argument('--steps', default='seed,button,record')
    p.add_argument('--expect-picture', action='store_true', help='fail when the saved file has no picture (a machine with a hardware H.264 encoder; a guest has none)')
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
