#!/usr/bin/env python3
"""richfiles-open-walk.py's arithmetic and refusals, without a guest: the click-to-visible bounds
come from the frame before RichFiles and the first frame that shows it, on the capture's own clock;
a capture that cannot answer says why instead of giving a number; an unknown step is refused."""
import importlib.util
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
WALK = HERE.parent / 'richfiles-open-walk.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


def refusal(record, readings):
    try:
        walk.visible_bounds(record, readings)
    except ValueError as exc:
        return str(exc)
    return ''


spec = importlib.util.spec_from_file_location('richfiles_open_walk', WALK)
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

MS = 1_000_000
# Frames every 100 ms, each taking 60 ms to photograph; the click is posted from 1,000 to 1,010 ms.
record = {'clock': 'monotonic_ns', 'action': {'kind': 'native-click', 'before_ns': 1000 * MS, 'after_ns': 1010 * MS},
          'frames': [{'number': n, 'before_ns': n * 100 * MS, 'after_ns': (n * 100 + 60) * MS} for n in range(40)]}
chat = 'Message to Rich\nChats'
files = 'Quick Access\nThis Mac\nHome\nDesktop'

# RichFiles first shows in frame 23 (begun 2,300 ms, complete 2,360 ms).
readings = {n: (files if n >= 23 else chat) for n in range(40)}
got = walk.visible_bounds(record, readings)
check('low is from the click posted to the START of the last frame without RichFiles (frame 22 began 2,200 ms)',
      got['low_ms'] == 1190.0 and got['last_absent_frame'] == 22, got)
check('high is from the click begun to the END of the first frame with RichFiles (frame 23 ended 2,360 ms)',
      got['high_ms'] == 1360.0 and got['first_present_frame'] == 23, got)
check('the click\'s own window is reported', got['click_window_ms'] == 10.0, got)

# The very next frame after the click already shows it: low is 0, never negative.
readings = {n: (files if n >= 10 else chat) for n in range(40)}
got = walk.visible_bounds(record, readings)
check('a frame begun before the click was posted gives a low of 0, not a negative time',
      got['low_ms'] == 0.0 and got['high_ms'] == 60.0 and got['first_present_frame'] == 10, got)

# The Details view's own column is RichFiles' words too.
readings = {n: ('Name  Size  Kind  Modified' if n >= 15 else chat) for n in range(40)}
check('the Details view\'s Modified column counts as RichFiles on screen',
      walk.visible_bounds(record, readings)['first_present_frame'] == 15)

check('RichFiles on screen before the click is refused: there is nothing to time',
      'already on screen' in refusal(record, {n: files for n in range(40)}))
check('RichFiles in no frame after the click is refused, with the count of frames looked at',
      'none of the 30 frames' in refusal(record, {n: chat for n in range(40)}))
late = dict(record, action={'kind': 'native-click', 'before_ns': 10 * MS, 'after_ns': 20 * MS})
check('a capture with no complete frame before the click is refused',
      'no frame was complete before the click' in refusal(late, {n: files for n in range(40)}))

# The app's own line about getting RichFiles ready ahead of a click.
line = ('[richos] launch: fresh\n[richos] RichFiles ready before its first click: library 412 ms on a background thread, '
        'pane 388 ms on the main thread, asked 5210 ms and ready 6015 ms after the process started\n')
check('the app\'s line is read into its four times',
      walk.ready_line(line) == {'library_ms': 412, 'pane_ms': 388, 'asked_ms_after_start': 5210, 'ready_ms_after_start': 6015},
      walk.ready_line(line))
check('a build that writes no such line reads as None, never as zeros', walk.ready_line('[richos] launch: fresh\n') is None)

r = subprocess.run([sys.executable, '-B', str(WALK), 'no-such-vm', '--out', '/nonexistent', '--expect-sha', 'x',
                    '--steps', 'identity,second-open'], capture_output=True, text=True)
check('an unknown step is refused before anything is driven', r.returncode == 2 and 'second-open' in r.stderr, r.stderr[-300:])

print('\n%d failed' % len(failures) if failures else '\nall passed')
sys.exit(1 if failures else 0)
