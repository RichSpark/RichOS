#!/usr/bin/env python3
"""richfiles-open-walk.py — how long RichFiles takes to appear after a click on its rail icon, in the guest.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT --wait 600 -- \\
      richfiles-open-walk.py --out DIR --expect-sha SHA [--steps a,b,c] [--limit-ms N] [--diagnose sample|disk]

THE QUESTION, the CEO's words (2026-11-15): "The very first time when I clicked on the RichFiles icon in
the nav rail ... it took quite a long time (probably at least 2 or 3 seconds) where absolutely nothing
happened after my click ... Ever since then (even after multiple app restarts) RichFiles comes up near
instantly ... whatever is done to make RichFiles appearance near instant in all those subsequent clicks,
should also be done for that initial click."

A guest is a fresh clone, so RichFiles has never opened in it: the first click here is his first click.

STEPS (in the guest, never on the host's screen, CEO section 65; the guest has no sound device):
  identity, first-run   as folders-walk.py: a new user's first run up to the conversation screen
  first-open     the real pointer clicks the RichFiles rail icon; the window is photographed on the same
                 clock as the click (qa/timeline.py, in the guest) and the first frame that shows RichFiles'
                 own words is found by reading the frames in order
  again          back to Chats, then the same click: an open in a process that has already shown RichFiles
  after-restart  the app quit and started again (relaunch.py), then the same click: his "even after
                 multiple app restarts"
  after-purge    (optional, a diagnosis) the app quit, the guest's file cache emptied (`sudo purge`), the app
                 started again, then the same click: an open whose files all come from the disk again
  typed-open     (optional) the first click after typing that never pauses, a user's ordinary minute: the
                 app started again, and then, in the guest and on one clock, the real pointer presses Talk
                 to Rich on the home screen, a key is typed every --key-ms for --typed-seconds with the
                 window photographed throughout, and the RichFiles icon is clicked right after the last
                 key. Reported like the other timed steps, with what the typing cost: the longest time the
                 window did not change while keys were arriving. Run it with identity and first-run only
                 and the click is also the first one ever
  early-open     (optional) a click that comes before RichFiles is ready (the CEO, 2026-11-15: "at least
                 show a basic FUCKING LOADING SCREEN!!!"): the app started again and, seconds in, before it
                 has said it is usable, the guest presses Talk to Rich and clicks the RichFiles icon 0.4 s
                 later. Reported: the click to the loading line on screen, and the click to RichFiles on
                 screen. It fails when no frame shows the loading line
  home-still     (optional) what getting RichFiles ready costs the start: the app quit from its own menu and
                 started again, and the home screen's moving picture watched from outside for --still-seconds
                 (frame-rate.swift, in the guest: the picture's frames in each second and the longest pause
                 between two of them). The seconds are listed by when they began after the process started,
                 beside the app's own line saying when its RichFiles library load ended. Run it on the
                 build before the change too: the pauses are the result, not a pass mark.

Each timed step reports the click-to-visible interval as two bounds in milliseconds: RichFiles was not on
screen in the frame that began `low` ms after the click and was on screen in the frame that ended `high`
ms after it. `high` is the number to quote. `--limit-ms N` fails a timed step whose `high` is over N.
Each also reads the app's own line about getting RichFiles ready ahead of a click (how long the library load took and when it ended) and says whether that was before the click.
`--diagnose sample` also samples the app's process across the first click (`sample`, saved as
first-open.sample.txt); `--diagnose disk` records every read from the guest's disk across it (`fs_usage`,
first-open.disk.txt). Both keep the guest's system log for those seconds. A sampled click is slower, so its
time is not a result and `--limit-ms` does not judge it.
Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO section 54).
"""
import argparse
import importlib.util
import json
import os
import re
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
QA = HERE.parent / 'qa'
sys.path.insert(0, str(QA / 'lib'))

STEPS = ['identity', 'first-run', 'first-open', 'again', 'after-restart']
OPTIONAL = ['after-purge', 'typed-open', 'early-open', 'home-still']
TIMED = ('first-open', 'again', 'after-restart', 'after-purge', 'typed-open', 'early-open')
# The load outcome from src-tauri/src/richfiles_pane.rs. Older builds write none.
LIBRARY = re.compile(r'RichFiles library: (loaded|unavailable) in (\d+) ms at (\d+) ms after process start')
# RichFiles' own words, which no other screen of the app shows: its sidebar's headings and first
# row, its Workspaces button and the Details view's date column
# (tools/richfiles/Sources/RichFiles/BrowserView.swift). Several, and "This Mac" with or without its
# space: the reader took "ThisMac" off seven frames in a row that plainly showed RichFiles, and the
# walk dated RichFiles 0.4 s late by it.
WORDS = r'This ?Mac|Quick Access|Pin Current Folder|Workspaces|Modified'
# How the frames are read: as sparse text. Read as a page, the reader finds nothing at all on a
# screen that holds one short line (the loading line), and joins words on a busy one.
SPARSE = 11
# The loading line a click shows when RichFiles is not ready yet (ui/index.html, #richfiles-loading).
LOADING = r'Opening RichFiles'


def visible_bounds(record, readings, words=WORDS):
    """Click-to-visible, from one capture's bounds.json and what each frame reads.

    `readings` maps a frame number to the text read off it. Returns low and high in milliseconds
    with the two frames they come from, or raises ValueError saying why there is no answer: the
    words were on screen before the click (nothing to time), or never appeared."""
    action = record['action']
    frames = sorted(record['frames'], key=lambda f: f['number'])
    shows = lambda f: re.search(words, readings.get(f['number'], ''), re.I) is not None  # noqa: E731
    before = [f for f in frames if f['after_ns'] <= action['before_ns']]
    if not before:
        raise ValueError('no frame was complete before the click: nothing shows what was on screen then')
    if shows(before[-1]):
        raise ValueError('RichFiles was already on screen before the click')
    later = [f for f in frames if f['number'] > before[-1]['number']]
    present = next((f for f in later if shows(f)), None)
    if present is None:
        raise ValueError('RichFiles was on screen in none of the %d frames after the click' % len(later))
    absent = next(f for f in frames if f['number'] == present['number'] - 1)
    return {'low_ms': round(max(0, absent['before_ns'] - action['after_ns']) / 1e6, 1),
            'high_ms': round((present['after_ns'] - action['before_ns']) / 1e6, 1),
            'last_absent_frame': absent['number'], 'first_present_frame': present['number'],
            'frames': len(frames), 'click_window_ms': round((action['after_ns'] - action['before_ns']) / 1e6, 1)}


def library_line(log):
    """The app's own library load outcome and duration, or None."""
    found = LIBRARY.search(log or '')
    if found is None:
        return None
    outcome, duration, finished = found.groups()
    return {'outcome': outcome, 'duration_ms': int(duration), 'finished_ms_after_start': int(finished)}


def longest_still(frames, since, until):
    """The longest time the window did not change between two moments, from a capture's frames in
    order: (when the frame was begun, in ms on one clock; its digest). A stretch runs from the first
    frame that showed a picture to the last that still showed it, so it is the least the window
    stood still for. Returns its length and when it began, or None when the window changed fewer
    than five times in all (nothing was being drawn that a stall could be seen in)."""
    inside = [f for f in frames if since <= f[0] <= until]
    if len({digest for _, digest in inside}) < 5:
        return None
    best, start = (0, inside[0][0]), 0
    for i in range(1, len(inside) + 1):
        if i == len(inside) or inside[i][1] != inside[start][1]:
            if inside[i - 1][0] - inside[start][0] > best[0]:
                best = (inside[i - 1][0] - inside[start][0], inside[start][0])
            start = i
    return {'ms': round(best[0]), 'began_ms': round(best[1])}


def type_in_guest(argv):
    """Runs INSIDE the guest (the walk copies this file in): press the point given (Talk to Rich) with
    the real pointer, then type the letter a every KEY_MS for SECONDS, and print when each happened
    on the guest's clock. One process, so the press and the keys are on one clock.
      richfiles-open-walk.py --type-in-guest X,Y SECONDS KEY_MS"""
    import ctypes

    class Point(ctypes.Structure):
        _fields_ = [('x', ctypes.c_double), ('y', ctypes.c_double)]
    x, y = map(float, argv[0].split(','))
    seconds, key_ms = float(argv[1]), float(argv[2])
    cg = ctypes.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
    cf = ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
    cg.CGPreflightPostEventAccess.restype = ctypes.c_bool
    if not cg.CGPreflightPostEventAccess():
        print('no permission to post events; nothing was pressed or typed', file=sys.stderr)
        return 2
    cg.CGEventCreateMouseEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint32, Point, ctypes.c_uint32]
    cg.CGEventCreateMouseEvent.restype = ctypes.c_void_p
    cg.CGEventCreateKeyboardEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint16, ctypes.c_bool]
    cg.CGEventCreateKeyboardEvent.restype = ctypes.c_void_p
    cg.CGEventPost.argtypes = [ctypes.c_uint32, ctypes.c_void_p]
    cf.CFRelease.argtypes = [ctypes.c_void_p]

    def post(event):
        cg.CGEventPost(0, event)
        cf.CFRelease(event)
    time.sleep(0.3)
    pressed = time.time()
    for kind in (1, 2):  # left button down, up
        post(cg.CGEventCreateMouseEvent(None, kind, Point(x, y), 0))
    # The first key comes before the app can have built anything after the press, so whatever it
    # builds from here on, it builds under typing: the hardest case for the typing.
    time.sleep(0.15)
    first, last, keys = time.time(), None, 0
    while time.time() - first < seconds:
        for down in (True, False):
            post(cg.CGEventCreateKeyboardEvent(None, 0, down))  # the letter a
        last, keys = time.time(), keys + 1
        time.sleep(key_ms / 1000)
    last = last or first  # no key was asked for: the press alone
    print(json.dumps({'pressed_ms': round(pressed * 1000), 'first_key_ms': round(first * 1000),
                      'last_key_ms': round(last * 1000), 'keys': keys}))
    return 0


def main():
    if sys.argv[1:2] == ['--type-in-guest']:
        return type_in_guest(sys.argv[2:])
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--steps', default=','.join(STEPS))
    p.add_argument('--seconds', type=float, default=8.0, help='how long each click is photographed (default 8)')
    p.add_argument('--limit-ms', type=float, help='fail a timed step whose click-to-visible upper bound is over this')
    p.add_argument('--typed-seconds', type=float, default=7.0, help='typed-open: how long keys are typed before the click (default 7)')
    p.add_argument('--key-ms', type=float, default=120.0, help='typed-open: one key every this many ms (default 120)')
    p.add_argument('--still-seconds', type=int, default=16, help='home-still: how many seconds of the start are watched (default 16, at most 20)')
    p.add_argument('--diagnose', choices=('sample', 'disk'),
                   help="across the first click: sample the app's process, or record every read from the guest's disk")
    a = p.parse_args()
    steps = a.steps.split(',')
    bad = [s for s in steps if s not in STEPS + OPTIONAL]
    if bad:
        p.error('unknown step(s): ' + ', '.join(bad))

    import qaocr  # noqa: E402 - the host's reader; refused with a sentence when there is none
    spec = importlib.util.spec_from_file_location('rail_walk', HERE / 'rail-walk.py')
    rail = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(rail)
    StepFailed, command, guest = rail.StepFailed, rail.command, rail.folders.guest
    import relaunch  # noqa: E402 - beside this file; folders-walk.py put this directory on the path

    class Walk(rail.Walk):
        remote = None
        log = None  # the running app's own log in the guest

        def app_log(self):
            return guest(self.vm, 'cat ' + shlex.quote(self.log or self.payload + '/app.log') + ' 2>/dev/null || true', 60)

        def tools(self):
            if self.remote:
                return self.remote
            remote = self.payload + '/qa'
            guest(self.vm, 'mkdir -p ' + shlex.quote(remote + '/lib'), 60)
            command([HERE / 'guest.sh', self.vm, '--push', QA / 'timeline.py', remote + '/timeline.py'], 120)
            for lib in ('qaimg.py', 'qaocr.py'):
                command([HERE / 'guest.sh', self.vm, '--push', QA / 'lib' / lib, remote + '/lib/' + lib], 120)
            self.remote = remote
            return remote

        def app_pid(self):
            state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
            return (state / 'app.pid').read_text().strip()

        def to_conversation(self, seconds=90):
            """The conversation screen, where the rail is: after a restart the app opens on Home.
            A read the starting app does not answer yet is asked again."""
            end = time.monotonic() + seconds
            last = ''
            while time.monotonic() < end:
                try:
                    if self.geometry('nr-richfiles'):
                        return
                    if self.present('Talk to Rich'):
                        self.press('Talk to Rich', contains=True)
                except StepFailed as exc:
                    last = str(exc)[-300:]
                time.sleep(1)
            raise StepFailed('the RichFiles rail icon never came on screen within %d s %s' % (seconds, last))

        def timed_open(self, label, icon=None, talk=None, typed=None):
            """One click on the RichFiles icon, timed. With `talk` (typed-open, early-open) the app is on
            the home screen: the guest first presses that point and types for `typed` seconds, and the
            icon is where `icon` says."""
            typed = a.typed_seconds if typed is None else typed
            if not talk:
                self.script('tell application "RichOS" to activate')
                time.sleep(1)
                self.to_conversation()
                icon = self.geometry('nr-richfiles')
            window = next((n for n in self.ax('find', '--role', 'AXWindow', '--first') if n.get('role') == 'AXWindow'), None)
            if not icon or not window:
                raise StepFailed('no RichFiles rail icon or no window to photograph: %r %r' % (icon, window))
            x, y = round(icon['x'] + icon['w'] / 2), round(icon['y'] + icon['h'] / 2)
            region = '%d,%d,%d,%d' % (window['x'], window['y'], window['w'], window['h'])
            remote = self.tools()
            frames = remote + '/' + label
            pid = self.app_pid()
            ev = {'click': [x, y], 'region': region, 'app_pid': pid,
                  'app_running_for': guest(self.vm, 'ps -o etime= -p %s' % pid).strip(),
                  'app_memory_mb': round(int(guest(self.vm, 'ps -o rss= -p %s' % pid).strip() or 0) / 1024)}
            kept = {'sample': 'first-open.sample.txt', 'disk': 'first-open.disk.txt'}.get(a.diagnose)
            if kept and label == 'first-open':
                watch = ('sample {p} {s} 1 -file {f}' if a.diagnose == 'sample' else
                         'sudo fs_usage -w -t {s} -f diskio > {f} 2>&1').format(
                             p=pid, s=int(a.seconds) + 4, f=shlex.quote(remote + '/' + kept))
                guest(self.vm, 'nohup sh -c {w} >/dev/null 2>&1 &'.format(w=shlex.quote(watch)), 30)
                time.sleep(1.5)
            capture = shlex.join(
                ['python3', remote + '/timeline.py', 'capture', frames, str(a.seconds), '--region', region,
                 '--native-click', '%d,%d' % (x, y), '--baseline', str(typed + 0.7 if talk else 1.5)])
            if talk:
                # The typist and the camera start together; the click comes right after the last key.
                command([HERE / 'guest.sh', self.vm, '--push', Path(__file__).resolve(), remote + '/richfiles-open-walk.py'], 120)
                capture = '(python3 {w} --type-in-guest {t} {s} {k} > {o} 2>&1 &); {c}'.format(
                    w=shlex.quote(remote + '/richfiles-open-walk.py'), t='%d,%d' % talk, s=typed, k=a.key_ms,
                    o=shlex.quote(frames + '.typing.json'), c=capture)
            ev['capture'] = guest(self.vm, capture, int(a.seconds + typed) + 90).strip()
            if kept and label == 'first-open':
                time.sleep(4)
                guest(self.vm, '/usr/bin/log show --last 25s --style compact > {f} 2>&1 || true'.format(
                    f=shlex.quote(remote + '/first-open.syslog.txt')), 120)
                for name in (kept, 'first-open.syslog.txt'):
                    try:
                        command([HERE / 'guest.sh', self.vm, '--pull', remote + '/' + name, self.out / name], 120)
                    except StepFailed as exc:
                        ev[name] = 'not kept: ' + str(exc)[-200:]
            guest(self.vm, 'cd {r} && tar -czf {l}.tgz {l}'.format(r=shlex.quote(remote), l=label), 120)
            command([HERE / 'guest.sh', self.vm, '--pull', frames + '.tgz', self.out / (label + '.tgz')], 180)
            command(['tar', '-xzf', self.out / (label + '.tgz'), '-C', self.out], 120)
            local = self.out / label
            record = json.loads((local / 'bounds.json').read_text())
            ordered = sorted(record['frames'], key=lambda f: f['number'])
            before = [f for f in ordered if f['after_ns'] <= record['action']['before_ns']]
            # The last frame before the click says what was on screen then; the frames after it are
            # read in order, stopping at the first that shows RichFiles.
            readings, answer, why = {}, None, 'no frame was complete before the click'
            for frame in ordered[max(0, len(before) - 1):]:
                readings[frame['number']] = qaocr.text(str(local / ('%04d.png' % frame['number'])), psm=SPARSE)
                try:
                    answer = visible_bounds(record, readings)
                    break
                except ValueError as exc:
                    why = str(exc)
                    if 'already on screen' in why:
                        break
            if answer is None:
                raise StepFailed('%s (frames kept in %s)' % (why, local))
            ev.update(answer)
            # The loading line, when the click showed one before RichFiles: from the same frames.
            try:
                line = visible_bounds(record, readings, words=LOADING)
                ev['loading_line'] = {'low_ms': line['low_ms'], 'high_ms': line['high_ms']}
            except ValueError:
                ev['loading_line'] = None
            # Whether the library was loaded when the click came, by its own line and the
            # click's moment on the same clock (the guest's), both counted from the process's start.
            log = self.app_log()
            born = re.search(r'process started at (\d+)', log)
            clicked = re.search(r'^t_action_before\t([0-9.]+)', (local / 'meta.tsv').read_text(), re.M)
            ev['library_load'] = library_line(log)
            if born and clicked:
                ev['click_ms_after_start'] = round(float(clicked.group(1)) - int(born.group(1)))
            ready_at = (ev['library_load'] or {}).get('finished_ms_after_start')
            ev['library_loaded_before_click'] = (ev['library_load']['outcome'] == 'loaded' and ready_at < ev['click_ms_after_start']
                                            if ready_at is not None and 'click_ms_after_start' in ev else None)
            if talk:
                # What was typed, on the process's own clock, and what the typing cost: the longest
                # time the window did not change while keys were arriving.
                typed = json.loads(guest(self.vm, 'cat ' + shlex.quote(frames + '.typing.json'), 60))
                if not (born and clicked):
                    raise StepFailed('the app wrote no launch line with its start time: ' + log[-400:])
                start = int(born.group(1))
                digest = {f['number']: f['sha256'] for f in record['frames']}
                begun = [l.split('\t') for l in (local / 'meta.tsv').read_text().splitlines() if l.startswith('frame\t')]
                shots = [(float(t), digest[int(n)]) for _, n, t in begun if int(n) in digest]
                still = longest_still(shots, typed['first_key_ms'], typed['last_key_ms'])
                ev['typed'] = {'keys': typed['keys'], 'left_home_ms_after_start': typed['pressed_ms'] - start,
                               'from_ms_after_start': typed['first_key_ms'] - start, 'to_ms_after_start': typed['last_key_ms'] - start,
                               'click_ms_after_last_key': round(float(clicked.group(1)) - typed['last_key_ms']),
                               'longest_still_while_typing': still and {'ms': still['ms'], 'began_ms_after_start': still['began_ms'] - start}}
            self.facts[label] = ev
            self.save()
            print('%s: RichFiles on screen between %.0f and %.0f ms after the click, %s ms after the app started; library: %s%s'
                  % (label, ev['low_ms'], ev['high_ms'], ev.get('click_ms_after_start'), json.dumps(ev['library_load']),
                     '; typed: ' + json.dumps(ev['typed']) if talk else ''), flush=True)
            if ev['loading_line']:
                print('%s: the loading line on screen between %.0f and %.0f ms after the click'
                      % (label, ev['loading_line']['low_ms'], ev['loading_line']['high_ms']), flush=True)
            if a.limit_ms is not None and ev['high_ms'] > a.limit_ms and not (a.diagnose == 'sample' and label == 'first-open'):
                raise StepFailed('RichFiles took up to %.0f ms to appear, over the %.0f ms limit: %s'
                                 % (ev['high_ms'], a.limit_ms, json.dumps(ev)))
            return ev

        def first_open(self):
            return self.timed_open('first-open')

        def again(self):
            self.click_id('nr-chats')
            time.sleep(2)
            return self.timed_open('again')

        def after_restart(self):
            started = relaunch.relaunch(self.vm)
            self.log = started.get('log')
            ev = self.timed_open('after-restart')
            ev['relaunched_pid'] = started.get('pid')
            return ev

        def early_open(self):
            self.script('tell application "RichOS" to activate')
            time.sleep(1)
            self.to_conversation()
            icon = self.geometry('nr-richfiles')
            if not icon:
                raise StepFailed('the RichFiles rail icon was not on the conversation screen')
            # Where Talk to Rich is, read on one start; pressed on the next, too early for a read.
            relaunch.relaunch(self.vm)
            self.wait_for('Talk to Rich', seconds=90)
            talk = self.node('Talk to Rich', 'AXButton')
            started = relaunch.relaunch(self.vm)
            self.log = started.get('log')
            # The home screen is up; the app does not say it is usable for some seconds more.
            time.sleep(2.5)
            ev = self.timed_open('early-open', icon=icon, typed=0,
                                 talk=(round(talk['x'] + talk['w'] / 2), round(talk['y'] + talk['h'] / 2)))
            ev['relaunched_pid'] = started.get('pid')
            if not ev['loading_line']:
                raise StepFailed('no frame shows the loading line between the click and RichFiles: ' + json.dumps(ev))
            return ev

        def typed_open(self):
            # Where the icon is, read on the conversation screen: after the restart the app is on the
            # home screen, where the rail is covered, and it stays there until the guest presses.
            self.script('tell application "RichOS" to activate')
            time.sleep(1)
            self.to_conversation()
            icon = self.geometry('nr-richfiles')
            started = relaunch.relaunch(self.vm)
            self.log = started.get('log')
            up = time.monotonic()
            self.wait_for('Talk to Rich', seconds=90)
            talk = self.node('Talk to Rich', 'AXButton')
            # The app says it is usable some seconds after its window is up; a user is on the home
            # screen at least that long.
            time.sleep(max(0, 15 - (time.monotonic() - up)))
            if not icon:
                raise StepFailed('the RichFiles rail icon was not on the conversation screen')
            ev = self.timed_open('typed-open', icon=icon,
                                 talk=(round(talk['x'] + talk['w'] / 2), round(talk['y'] + talk['h'] / 2)))
            ev['relaunched_pid'] = started.get('pid')
            return ev

        def after_purge(self):
            relaunch.quit_app(self.vm)
            guest(self.vm, 'sudo purge', 120)
            started = relaunch.relaunch(self.vm)
            self.log = started.get('log')
            ev = self.timed_open('after-purge')
            ev['relaunched_pid'] = started.get('pid')
            return ev

        def home_still(self):
            binary = self.out / 'frame-rate'
            built = subprocess.run(['swiftc', '-O', '-target', 'arm64-apple-macos14.0', str(HERE / 'frame-rate.swift'),
                                    '-o', str(binary)], capture_output=True, text=True, timeout=600)
            if built.returncode:
                raise StepFailed('frame-rate.swift did not compile: ' + (built.stdout + built.stderr)[-600:])
            meter = self.tools() + '/frame-rate'
            command([HERE / 'guest.sh', self.vm, '--push', binary, meter], 120)
            # The app's own Quit, so the next start is an ordinary one and plays its opening screen.
            old = self.app_pid()
            try:
                self.script('tell application "System Events" to tell (first process whose unix id is %s) to click '
                            'menu item "Quit RichOS" of menu 1 of menu bar item 2 of menu bar 1' % old)
            except StepFailed:
                pass  # relaunch.py ends it instead; the evidence says which kind of start followed
            end = time.monotonic() + 20
            while time.monotonic() < end and guest(self.vm, 'kill -0 %s 2>/dev/null && echo alive || true' % old) == 'alive':
                time.sleep(0.5)
            started = relaunch.relaunch(self.vm)
            self.log = started.get('log')
            report, why = None, ''
            for _ in range(8):  # the window is on screen a fraction of a second after the start
                time.sleep(0.5)
                try:
                    report = json.loads(guest(self.vm, '%s %s 1 %d' % (shlex.quote(meter), started['pid'], a.still_seconds),
                                              a.still_seconds + 60))
                    break
                except (RuntimeError, ValueError) as exc:
                    why = str(exc)[-300:]
            if report is None:
                raise StepFailed('the window could not be watched: ' + why)
            log = self.app_log()
            (self.out / 'home-still.app.log').write_text(log)
            born = re.search(r'process started at (\d+)', log)
            kind = re.search(r'\[richos\] launch: (\S+)', log)
            if not born:
                raise StepFailed('the app wrote no launch line with its start time: ' + log[-400:])
            born = int(born.group(1))
            seconds = [{'from_ms': round(r['began_ms'] - born), 'picture_frames': r['picture'], 'window_frames': r['complete'],
                        'longest_pause_ms': round(r['longest_gap_ms']),
                        'pause_began_ms': round(r['longest_gap_began_ms'] - born) if r['longest_gap_began_ms'] else None}
                       for r in report['samples']]
            ev = {'launch': kind.group(1) if kind else None, 'relaunched_pid': started.get('pid'),
                  'made_ready': library_line(log), 'seconds': seconds,
                  'longest_pause': max(seconds, key=lambda r: r['longest_pause_ms'])}
            self.facts['home-still'] = ev
            self.save()
            print('home-still: a %s start; made ready: %s; the longest pause of the picture: %d ms, begun %s ms after the start; '
                  'by second (from ms: frames, longest pause): %s' % (
                      ev['launch'], json.dumps(ev['library_load']), ev['longest_pause']['longest_pause_ms'],
                      ev['longest_pause']['pause_began_ms'],
                      ', '.join('%d: %d, %d' % (r['from_ms'], r['picture_frames'], r['longest_pause_ms']) for r in seconds)),
                  flush=True)
            return ev

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
        # The timed steps each carry on, so one slow open does not hide the next number.
        if not ok and step not in TIMED:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
