#!/usr/bin/env python3
"""The delta walk presses a button the moment it exists instead of sleeping 2 s first
(hunt part 2, finding 41), and the import walk's after-relaunch reading looks before it sleeps.

No guest: Walk.press is replaced by a stub that fails with 'notfound' a set number of times."""
import importlib.util
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('delta_walk', HERE / 'delta-walk.py')
delta = importlib.util.module_from_spec(spec)
spec.loader.exec_module(delta)
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


class Stub(delta.Walk):
    def __init__(self, misses):
        self.misses, self.calls = misses, 0

    def press(self, title, app=None, role='AXButton'):
        self.calls += 1
        if self.calls <= self.misses:
            raise delta.Failure('harness failure', 'notfound: ' + title)
        return 'pressed'


began = time.monotonic()
w = Stub(0)
check('a button that is already there is pressed on the first look', w.press_when_present('They match') == 'pressed' and w.calls == 1)
check('...with no fixed sleep before it', time.monotonic() - began < 1.0)

w = Stub(2)
check('a button that appears late is pressed once it does', w.press_when_present('They match') == 'pressed' and w.calls == 3, w.calls)

w = Stub(10 ** 6)
try:
    w.press_when_present('They match', budget=0)
    raised = False
except delta.Failure:
    raised = True
check('a button that never appears still fails, at the budget', raised)

src = (HERE / 'import-walk.py').read_text()
i = src.index("name_on_screen(vm, a.out, 'w6-after-%d' % attempt)")
check('import walk W6 looks before it sleeps', 'time.sleep(8)' not in src[src.rindex('while', 0, i):i])

# R41: the margins that stay (past the splash; the door and the theme settling) stop charging for
# time that already passed. The cap is the old margin, so a slow run waits exactly as before.
import margin  # noqa: E402

check('ps etime text is read in seconds', [margin.parse_etime(t) for t in ('00:07', '01:02:03', '2-01:00:00', 'junk', '')]
      == [7, 3723, 176400, None, None])
slept = []
r = margin.wait_since_start('vm', 1, 10, age_of=lambda vm, pid: 7, sleep=slept.append)
check('a margin counted from the app start sleeps only what is left', slept == [3.0] and r['slept_s'] == 3.0 and r['app_age_s'] == 7, (slept, r))
slept = []
margin.wait_since_start('vm', 1, 10, age_of=lambda vm, pid: 12, sleep=slept.append)
check('an app already older than the margin costs no sleep', slept == [], slept)
slept = []
margin.wait_since_start('vm', 1, 10, age_of=lambda vm, pid: None, sleep=slept.append)
check('an age that cannot be read leaves the whole margin in force', slept == [10.0], slept)


class Clock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now

    def sleep(self, n):
        self.now += n


def poll(frames, cap=4, interval=1.0):
    clock, seq = Clock(), iter(frames)
    return margin.wait_change_then_still(lambda: next(seq, frames[-1]), b'before', cap, interval, clock, clock.sleep)


r = poll([b'A', b'A'])
check('a frame that changed and then repeats is settled after two looks, not at the cap', r['outcome'] == 'settled' and r['waited_s'] == 1.0, r)
r = poll([b'before', b'before', b'A', b'B', b'B'])
check('a frame still equal to the one before the action is not settled', r['outcome'] == 'settled' and r['frames'] == 5, r)
r = poll([b'before'])
check('a screen that never changes runs to the cap', r['outcome'] == 'cap' and r['waited_s'] == 4.0, r)
r = poll([b'1', b'2', b'3', b'4', b'5', b'6'])
check('a screen that never holds still runs to the cap', r['outcome'] == 'cap' and r['waited_s'] == 4.0, r)
r = poll([None])
check('frames that cannot be taken never shorten the wait', r['outcome'] == 'cap', r)

src = (HERE / 'import-walk.py').read_text()
i = src.index('# 10 s since the app started')
check('import walk W1 counts its margin from the app start', 'time.sleep(10)' not in src[i:i + 900] and 'margin.wait_since_start' in src[i:i + 900])
src = (HERE / 'speckle-walk.py').read_text()
check('speckle walk no longer sleeps its 8 s, 4 s and 4 s unconditionally',
      'time.sleep(' not in src and src.count('margin.wait_since_start') == 1 and src.count('margin.wait_change_then_still') == 2)
sys.exit(1 if failures else 0)
