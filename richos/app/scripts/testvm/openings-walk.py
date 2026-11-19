#!/usr/bin/env python3
"""openings-walk.py: round 74's openings on the real app, in the guest, with no sound device.

The CEO's order of 2026-11-14: the "Your openings" gallery with Play and "Make it my opening", the
materials ladder, the sound on by default with a switch in Settings, "One of mine, a different one
each day" as a second way to start; and, the same day, no switch that turns the splash screen off.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone. ALWAYS with --no-audio: the guest is then booted with no sound device passed through, so
nothing it plays can reach this Mac's speakers (CEO ruling 53), and the walk refuses a guest
booted any other way:

  run-walk.py --no-audio --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      openings-walk.py --out DIR [--steps first,history,...]

run-walk.py passes the owned VM name as the first argument.

THE STEPS, in order (each is a real press on the app's own window or a read of its own files):
  first     a new user's first start: the splash played, the two openings are his, the sound is on
  history   the app quit from its own menu; its launch record made 430 days old and its settings
            given the old off switch (`"splash_enabled": false`), as a user of an older build who
            had switched the splash off; started again: the splash plays, the time with Rich has
            earned the materials and the year's edition, and nothing earned what needs money or jobs
  gallery   Settings, "Your openings": a card for each opening that is his, one sealed tile where a
            ladder has more, and no name of an opening that is not his
  play      Play on a material and on the year's edition: the opening covers the window and leaves
            by itself; the gallery is there again
  missing_pack  An older profile's saved picks with no downloaded videos play their own opening.
            The app downloads the video after playback and uses it on the next launch.
  pick      "Make it my opening" downloads Bridle Leather; after quitting from the menu, the next
            start plays the verified native pack
  daily     "One of mine, a different one each day" plays today's native pack on the next start.
            Advancing the guest's clock a day plays the next opening, and another start that day
            plays the same one again.
  sound     the Settings switch "Play its sound" is on; pressed, it is off in the settings file,
            and still off, in Settings and in the gallery, after the app is started again
  save      Save image on a card: a picture of that opening in the guest's Downloads folder,
            copied out as evidence
  spend     Token spend opened from Settings, then Settings, "Your openings" over it: the gallery
            is on the screen and Token spend is gone from under it
  sheet     with the gallery up, Settings, "Connected folders (repositories)": the gallery gives
            way to the sheet that row opens

Evidence: --out/report.json (every step's result), frames of the guest's screen, the app's own log
lines quoted in the report. Exit 0 when every step passed. It quits the app only from the app's
own menu, between its own starts; run-walk.py quits the last instance by its pid, checks it gone
and deletes the clone (CEO 54).
"""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
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

_spec = importlib.util.spec_from_file_location('home_standard_walk', HERE / 'home-standard-walk.py')
standard = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(standard)
StepFailed = standard.StepFailed
run = standard.run

STEPS = ['first', 'history', 'gallery', 'play', 'pick', 'daily', 'sound', 'save', 'spend', 'sheet']
OPTIONAL_STEPS = ['missing_pack']
DAY = 86400
# What 430 days with Rich earns by the catalog's own rules (ui/openings-catalog.js): 3, 6 and 12
# months, and the year's edition; with the two every user starts with, in the catalog's order.
HIS = ['ruled-standard', 'midnight-suede', 'edition-year', 'material-leather', 'material-slate', 'material-steel']
NAMES = {'ruled-standard': 'The Ruled Standard', 'midnight-suede': 'Midnight Suede', 'edition-year': 'One Year with Rich',
         'material-leather': 'Bridle Leather', 'material-slate': 'Honed Slate', 'material-steel': 'Blued Steel'}
NOT_HIS = ['Gold Leaf', 'Guilloch', 'First Light', 'The First Job', 'Half a Day', 'Fresh from the Update', 'The Thousandth Start']


def daily_opening(seconds, zone, his):
    """The first day's opening by the app's rule (`openings::daily`, before any such day): the
    guest's LOCAL day number, modulo how many openings are his, in the catalog's order. A later
    day opens on the one after the last. `seconds` is the guest's Unix time, `zone` its
    `date +%z` (+0100, -0800). Returns (day, opening)."""
    offset = (1 if zone[0] == '+' else -1) * (int(zone[1:3]) * 3600 + int(zone[3:5]) * 60)
    day = (seconds + offset) // DAY
    return day, his[day % len(his)]


class Walk(standard.Walk):
    def __init__(self, vm, out):
        super().__init__(vm, out, 0, 0)
        self.state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / vm
        self.log = self.payload + '/app.log'  # the log of the instance that is up now
        self.data = None

    # ---- the guest's files and the app's own instance ------------------------------------------
    def pid(self):
        pid = (self.state / 'app.pid').read_text().strip()
        if not pid.isdigit():
            raise StepFailed('the recorded app pid is not numeric')
        return pid

    def data_dir(self):
        if not self.data:
            found = guest(self.vm, 'find ' + shlex.quote(self.home + '/Library') + ' -name launches.json -maxdepth 4 2>/dev/null | head -1')
            if not found:
                raise StepFailed('the app has written no launch record under its home')
            self.data = os.path.dirname(found)
        return self.data

    def read_json(self, name):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data_dir() + '/' + name) + ' 2>/dev/null || true')
        return json.loads(text) if text.strip() else {}

    def write_json(self, name, value):
        body = base64.b64encode(json.dumps(value, indent=2).encode()).decode()
        guest(self.vm, 'echo ' + body + ' | base64 -D > ' + shlex.quote(self.data_dir() + '/' + name))

    def log_lines(self, pattern):
        text = guest(self.vm, 'grep -E ' + shlex.quote(pattern) + ' ' + shlex.quote(self.log) + ' 2>/dev/null || true', 60)
        return [line for line in text.splitlines() if line.strip()]

    def launch_line(self):
        """The app's own account of this start: which splash and which player it used."""
        return self.until(lambda: (self.log_lines(r'launch timing:') or [None])[-1], 90, 'the app wrote no launch line')

    def earned(self):
        rewards = self.read_json('launches.json').get('rewards_fired', {})
        return sorted(k[len('opening/'):] for k in rewards if k.startswith('opening/'))

    def quit_from_menu(self):
        """Quit with the app's menu and wait for that instance to leave."""
        pid = self.pid()
        script = ('tell application "System Events" to tell (first process whose unix id is %s) '
                  'to click menu item "Quit RichOS" of menu 1 of menu bar item 2 of menu bar 1' % pid)
        guest(self.vm, 'osascript -e ' + shlex.quote(script), 60)
        for _ in range(100):
            if guest(self.vm, 'kill -0 ' + pid + ' 2>/dev/null && echo alive || true') != 'alive':
                break
            time.sleep(0.2)
        else:
            raise StepFailed('the app (pid ' + pid + ') was still running 20 s after its own Quit')

    def start(self):
        """Start the app again as LaunchServices starts it, with the same fixture home."""
        started = relaunch(self.vm)
        if not started.get('pid'):
            raise StepFailed('the app did not start again')
        self.log = started['log']
        return self.launch_line()

    # ---- the screen ----------------------------------------------------------------------------
    def to_home(self):
        """The home screen, with a new user's sheets declined WHENEVER they come up: on a busy
        host a sheet can arrive after the first look (2026-11-15: "Where should I keep what you
        tell me?" came up 30 s into a guest that shared the Mac with another), so the sheets are
        looked for again on every pass, for up to two minutes."""
        end = time.monotonic() + 120
        while time.monotonic() < end:
            self.decline_sheets()
            if self.node('--id', 'home-enter'):
                return True
            self.ax('click', '--id', 'nr-home', '--first')
            time.sleep(3)
        raise StepFailed('the home screen did not come up within 120 s')

    def settings(self):
        if not self.node('--id', 'splash-sound'):
            self.ax('click', '--id', 'set-btn', '--first')
        return self.until(lambda: self.node('--id', 'splash-sound'), 30, 'the Settings panel did not open with the splash screen row in it')

    def open_gallery(self):
        if self.node('--id', 'openings-page'):
            return
        self.to_home()
        self.settings()
        self.ax('click', '--id', 'openings-open', '--first')
        self.until(lambda: self.node('--id', 'openings-page'), 30, 'Your openings did not open')
        time.sleep(2)  # the wall is drawn from the shell's answer

    def has(self, words):
        return self.text(words) is not None

    def sound_mode(self):
        """How the guest was booted (run.sh records it): `off` is tart's --no-audio, with which
        nothing the guest plays is passed to this Mac's speakers."""
        try:
            return (self.state / 'audio').read_text().strip()
        except OSError:
            return 'unknown'

    # ---- the steps -----------------------------------------------------------------------------
    def first(self):
        line = self.launch_line()
        if 'splash shown' not in line:
            raise StepFailed('a new user\'s first start showed no splash: ' + line)
        self.to_home()
        mine = self.until(lambda: self.earned() or None, 30, 'nothing was recorded as his at the first start')
        if mine != sorted(['ruled-standard', 'midnight-suede']):
            raise StepFailed('a new user has ' + ', '.join(mine) + ', not the two openings')
        config = self.read_json('config.json')
        if config.get('opening_sound', True) is not True:
            raise StepFailed('the sound is not on for a new user: ' + json.dumps(config.get('opening_sound')))
        switch = self.settings()
        frame = self.shot('first-settings-row')
        self.close_settings()
        return {'launch_line': line, 'his': mine, 'sound_setting': config.get('opening_sound', 'absent (on)'),
                'settings_switch_value': switch.get('value'), 'guest_sound': self.sound_mode(), 'frame': frame}

    def history(self):
        said = self.quit_from_menu()
        record = self.read_json('launches.json')
        now_ms = int(guest(self.vm, 'date +%s')) * 1000
        record['installed_at'] = now_ms - 430 * DAY * 1000
        self.write_json('launches.json', record)
        config = self.read_json('config.json')
        config.update({'splash_enabled': False, 'splash_disabled_at': now_ms - 30 * DAY * 1000})
        self.write_json('config.json', config)
        line = self.start()
        window = (self.log_lines(r'window: launch kind') or [''])[-1]
        if 'splash shown' not in line or 'splash on' not in window:
            raise StepFailed('a user who had the splash switched off did not get it: ' + window + ' | ' + line)
        mine = self.until(lambda: (self.earned() if len(self.earned()) > 2 else None), 30, 'time with Rich earned nothing')
        if mine != sorted(HIS):
            raise StepFailed('430 days earned ' + ', '.join(mine) + ', not ' + ', '.join(sorted(HIS)))
        return {'quit': said, 'window_line': window, 'launch_line': line, 'his': mine}

    def gallery(self):
        self.open_gallery()
        missing = [NAMES[o] for o in HIS if not self.has(NAMES[o])]
        if missing:
            raise StepFailed('the gallery does not show: ' + ', '.join(missing))
        leaked = [n for n in NOT_HIS if self.has(n)]
        if leaked:
            raise StepFailed('the gallery names openings that are not his: ' + ', '.join(leaked))
        sealed = len(self.ax('find', '--value', 'Sealed')) or len(self.ax('find', '--title', 'Sealed'))
        count = self.text('6 so far')
        if not count:
            raise StepFailed('the gallery does not say "6 so far"')
        frame = self.shot('gallery')
        return {'cards': [NAMES[o] for o in HIS], 'sealed_tiles': sealed, 'count_line': (count.get('value') or count.get('title') or '')[:120], 'frame': frame}

    def play(self):
        self.open_gallery()
        out = {}
        # What each opening shows that the gallery under it does not (the mark's "RichOS" is also in
        # the menu bar, so it proves nothing): the line under the mark, or the edition's own line.
        for opening, seconds, words in (('material-leather', 3, 'Operating System|AI-Enabled'), ('edition-year', 5, 'to the day|1 year')):
            self.ax('click', '--title', 'Play ' + NAMES[opening], '--first')
            time.sleep(0.4)
            frame = self.shot('play-' + opening)
            covered = self.reads(frame, words)
            gallery_hidden = not self.reads(frame, 'Your openings')
            time.sleep(seconds + 1.5)
            after = self.shot('play-' + opening + '-after')
            back = self.reads(after, 'Your openings')
            out[opening] = {'frame': frame, 'the_opening_is_on_screen': covered, 'the_gallery_is_covered': gallery_hidden,
                            'after': after, 'the_gallery_is_back': back}
            if not (covered and gallery_hidden and back):
                raise StepFailed('Play ' + NAMES[opening] + ': ' + json.dumps(out[opening]))
        return out

    def pick(self):
        self.open_gallery()
        # The cards are in the catalog's order; the app's own rule marks The Ruled Standard, whose
        # button reads "Your opening", so the buttons titled "Make it my opening" are the other five.
        index = [o for o in HIS if o != 'ruled-standard'].index('material-leather')
        self.ax('click', '--title', 'Make it my opening', '--nth', str(index))
        self.until(lambda: self.read_json('config.json').get('opening_pick') == 'material-leather', 20, 'the pick was not saved as material-leather')
        self.until(lambda: self.has('Bridle Leather is your opening now'), 15, 'the page did not say the pick was made')
        frame = self.shot('picked-leather')
        self.quit_from_menu()
        line = self.start()
        start_frame = self.shot('start-leather')
        if 'opening/material-leather (native pack)' not in line:
            raise StepFailed('the next start did not use the native pack for his opening: ' + line)
        return {'frame': frame, 'launch_line': line, 'start_frame': start_frame}

    def missing_pack(self):
        manifest_path = HERE.parent.parent / 'native' / 'splash' / 'packs.json'
        manifest = json.loads(manifest_path.read_text())
        results = {}
        for opening in ('material-slate', 'edition-year'):
            self.to_home()
            self.quit_from_menu()
            video = shlex.quote(self.data_dir() + '/splash-packs-v2/' + opening + '.mp4')
            if guest(self.vm, 'test ! -e ' + video + ' && echo absent || echo present') != 'absent':
                raise StepFailed(opening + ' already has a video, so this is not a migration fixture')
            config = self.read_json('config.json')
            config['opening_pick'] = opening
            self.write_json('config.json', config)
            fallback = self.start()
            if 'opening/' + opening + ' (web animation)' not in fallback:
                raise StepFailed('the start substituted another opening for ' + opening + ': ' + fallback)
            digest = self.until(
                lambda: guest(self.vm, 'shasum -a 256 ' + video + ' 2>/dev/null | cut -d " " -f1') or None,
                45, 'no video was prepared after ' + opening,
            )
            if digest != manifest[opening]['sha256']:
                raise StepFailed(opening + ' downloaded a video with the wrong SHA-256')
            self.to_home()
            self.quit_from_menu()
            native = self.start()
            if 'opening/' + opening + ' (native pack)' not in native:
                raise StepFailed('the next start did not use the local video for ' + opening + ': ' + native)
            results[opening] = {'first_launch': fallback, 'sha256': digest, 'next_launch': native}
        return results

    def expected_daily(self):
        """Today's opening by the rule: the local day number, modulo how many he has, in catalog order."""
        return daily_opening(int(guest(self.vm, 'date +%s')), guest(self.vm, 'date +%z'), HIS)

    def daily(self):
        self.open_gallery()
        self.ax('click', '--title', 'One of mine, a different one each day', '--first')
        self.until(lambda: self.read_json('config.json').get('opening_pick') == 'daily', 20, 'the daily way to start was not saved')
        frame = self.shot('daily')
        day, today = self.expected_daily()
        self.quit_from_menu()
        line = self.start()
        if 'opening/' + today + ' (native pack)' not in line:
            raise StepFailed('day %d: the start did not show %s: %s' % (day, today, line))
        out = {'frame': frame, 'day': day, 'today': today, 'launch_line': line}
        self.to_home()
        self.quit_from_menu()
        guest(self.vm, 'sudo systemsetup -setusingnetworktime off >/dev/null 2>&1; sudo date "$(date -v+1d +%m%d%H%M%Y.%S)" >/dev/null', 60)
        day2, _ = self.expected_daily()
        tomorrow = HIS[(HIS.index(today) + 1) % len(HIS)]
        if day2 != day + 1:
            raise StepFailed('the guest clock did not move a day on')
        for retried in [False, True]:
            line = self.start()
            if 'opening/' + tomorrow + ' (native pack)' not in line:
                raise StepFailed('day %d: the start did not use the pack for %s: %s' % (day2, tomorrow, line))
            if not retried:
                frame = self.shot('daily-next-day')
                self.to_home()
                self.quit_from_menu()
        out['next_day'] = {'day': day2, 'opening': tomorrow, 'launch_line': line, 'frame': frame}
        return out

    def sound(self):
        self.to_home()
        before = self.settings()
        if str(before.get('value')) not in ('1', 'True', 'true'):
            raise StepFailed('the sound switch is not on before it is pressed: ' + json.dumps(before.get('value')))
        frame_on = self.shot('sound-on')
        self.ax('click', '--id', 'splash-sound', '--first')
        self.until(lambda: self.read_json('config.json').get('opening_sound') is False, 20, 'the settings file does not say the sound is off')
        frame_off = self.shot('sound-off')
        config = self.read_json('config.json')
        self.close_settings()
        self.quit_from_menu()
        line = self.start()
        self.to_home()
        after = self.settings()
        if str(after.get('value')) not in ('0', 'False', 'false', 'None', ''):
            raise StepFailed('after a restart the Settings switch is on again: ' + json.dumps(after.get('value')))
        self.ax('click', '--id', 'openings-open', '--first')
        self.until(lambda: self.node('--id', 'openings-page'), 30, 'Your openings did not open')
        in_gallery = self.until(lambda: self.node('--id', 'opg-sound'), 20, 'the gallery has no sound switch')
        if str(in_gallery.get('value')) not in ('0', 'False', 'false', 'None', ''):
            raise StepFailed('the gallery\'s sound switch says on while Settings says off')
        return {'before': before.get('value'), 'frame_on': frame_on, 'frame_off': frame_off,
                'settings_file': {k: config.get(k) for k in ('opening_sound', 'splash_enabled', 'splash_disabled_at')},
                'after_restart': {'settings_switch': after.get('value'), 'gallery_switch': in_gallery.get('value'), 'launch_line': line}}

    def save(self):
        self.open_gallery()
        target = self.home + '/Downloads/RichOS Bridle Leather.jpg'
        guest(self.vm, 'rm -f ' + shlex.quote(target))
        index = HIS.index('material-leather')
        self.ax('click', '--title', 'Save image', '--nth', str(index))

        def answered():
            if self.text('could not be saved'):
                why = (self.log_lines(r'openings: the image') or ['(the app logged no reason)'])[-1]
                raise StepFailed('the app said the image could not be saved: ' + why)
            return self.text('Saved to your Downloads folder')

        said = self.until(answered, 40, 'the page said nothing about the picture')
        self.until(lambda: guest(self.vm, 'test -s ' + shlex.quote(target) + ' && echo yes || true') == 'yes', 20, 'no picture arrived in Downloads')
        data = base64.b64decode(guest(self.vm, 'base64 -i ' + shlex.quote(target), 120))
        (self.out / 'saved-bridle-leather.jpg').write_bytes(data)
        if data[:3] != b'\xff\xd8\xff':
            raise StepFailed('the saved file is not a JPEG')
        size = guest(self.vm, 'sips -g pixelWidth -g pixelHeight ' + shlex.quote(target) + ' 2>/dev/null | tail -2 | tr -s " \\n" " "')
        # The share card's size, whatever the guest's screen: 1600 x 900 at this guest's 1x.
        if 'pixelWidth: 1600' not in size or 'pixelHeight: 900' not in size:
            raise StepFailed('the saved picture is not 1600 x 900: ' + size.strip())
        return {'file': 'RichOS Bridle Leather.jpg', 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'pixels': size.strip(),
                'said': (said.get('value') or said.get('title') or '')[:160], 'copy': 'saved-bridle-leather.jpg'}

    def spend(self):
        """Token spend is a screen above the gallery, and Settings is reachable over it: "Your
        openings" pressed there must show the gallery, with Token spend gone from under it."""
        self.quit_from_menu()
        self.start()
        # A new user's sheet can come up late and take Token spend's place, as any sheet does
        # (2026-11-15: "Which company is this copy of Rich for?" arrived after the press). It is
        # declined (`to_home`) and Token spend opened again, until it stays up.
        for _ in range(4):
            self.to_home()
            self.settings()
            self.ax('click', '--id', 'set-spend-open', '--first')
            time.sleep(4)
            if self.node('--id', 'spend-back'):
                break
        else:
            raise StepFailed('Token spend did not open and stay open')
        frame = self.shot('spend-open')
        self.settings()
        self.ax('click', '--id', 'openings-open', '--first')
        self.until(lambda: self.node('--id', 'openings-page'), 30, 'Your openings did not open over Token spend')
        time.sleep(1)
        if self.node('--id', 'spend-back'):
            raise StepFailed('Token spend is still up with the gallery open: the gallery is under it')
        return {'frame_before': frame, 'frame_after': self.shot('gallery-over-spend'), 'token_spend_gone': True}

    def sheet(self):
        """The other way round: with the gallery up, Settings, "Connected folders (repositories)".
        Its sheet lives under the gallery, so the gallery must give way to it."""
        self.open_gallery()
        self.ax('click', '--id', 'set-btn', '--first')
        self.until(lambda: self.node('--id', 'set-repositories-open'), 30, 'the Settings panel did not open over the gallery')
        self.ax('click', '--id', 'set-repositories-open', '--first')
        self.until(lambda: self.has('Connected project folders'), 30, 'the Connected folders sheet did not open')
        # By what is on the screen: WebKit keeps an empty group for the hidden page in the
        # accessibility tree (2026-11-15), so the page's own node says nothing; its Back button does.
        try:
            self.until(lambda: not self.has('Back to RichOS'), 20, 'the gallery is still up')
        except StepFailed:
            raise StepFailed('the gallery is still up after Connected folders was pressed: its sheet is under the gallery')
        return {'frame': self.shot('sheet-over-gallery'), 'gallery_gone': True}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = [s for s in a.steps.split(',') if s]
    if not steps:
        p.error('--steps must select at least one step')
    unknown = [s for s in steps if s not in STEPS and s not in OPTIONAL_STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(a.vm, a.out)
    if walk.sound_mode() != 'off':
        p.error('the guest was booted with its sound device (%s), which plays on this Mac\'s speakers, and the '
                'splash has a sound: run it with run-walk.py --no-audio' % walk.sound_mode())
    report = {'vm': a.vm, 'steps': {}}
    failed = False
    for step in steps:
        began = time.monotonic()
        try:
            report['steps'][step] = {'pass': True, 'result': getattr(walk, step)()}
        except (StepFailed, subprocess.TimeoutExpired, RuntimeError, KeyError, ValueError, IndexError) as exc:
            report['steps'][step] = {'pass': False, 'why': str(exc)[-1500:]}
            failed = True
            try:
                walk.shot('failed-' + step)
                code, tree = run([HERE / 'ax.sh', a.vm, 'tree'], 60)
                (a.out / ('failed-' + step + '.tree')).write_text(tree + '\n')
            except Exception:
                pass
        report['steps'][step]['seconds'] = round(time.monotonic() - began, 1)
        print(step, 'PASS' if report['steps'][step]['pass'] else 'FAIL: ' + report['steps'][step]['why'][:500], flush=True)
        # every later step stands on the first two
        if not report['steps'][step]['pass'] and step in ('first', 'history'):
            break
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
