#!/usr/bin/env python3
"""screen-wait-walk.py's locked and unlock steps, offline: the timeline line is proved, never
assumed. The live walk is its own proof.

Handover review rv-20261025T031926Z-a924a2a6-8dbf#1: at a924a2a6e `locked()` saved
`window_shows_waiting` without asserting it, so an accessibility read that found nothing at all
still passed every check. Here the window's answer is stubbed; the step must fail when the
turn's "Waiting for the screen to unlock" label is absent, and pass only when a static text
reads exactly that.
"""
import importlib.util
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('screen_wait_walk', HERE / 'screen-wait-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


class Clock:
    """time.monotonic and time.sleep, with sleep advancing the clock so every wait ends."""
    def __init__(self):
        self.now = 0.0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


def node(value):
    return {'role': 'AXStaticText', 'value': value, 'x': 1, 'y': 1, 'w': 1, 'h': 1}


class Walk(walk.ScreenWalk):
    """The real check/window/locked/unlock; the guest replaced by fixed answers."""
    def __init__(self, out, screen):
        self.out, self.facts, self.vm = Path(out), {}, 'NO-VM-IS-CONTACTED'
        self.screen = screen  # label -> nodes the guest's window answers with
        self.given = False
        self.state = 'waiting-for-screen'

    def save(self):
        pass

    def ax(self, mode, *args, app=None, timeout=40):
        label = args[args.index('--value') + 1]
        hits = self.screen.get(label, [])
        if not hits:
            raise walk.StepFailed('command failed (1): ax.sh find\n{"error": "notfound", "detail": "nothing matched"}')
        return [{'meta': True}] + hits

    def job(self, name):
        detail = walk.LINE if self.state == 'waiting-for-screen' else 'The screen is back, so this is carrying on.'
        return {'state': self.state, 'detail': detail, 'notices': [], 'entity_id': 'e', 'thread_id': 't', 'id': 'a'}

    def look(self, name, record):
        rows = [{'detail': walk.LINE}] if self.state == 'waiting-for-screen' else []
        return {'session': 'gui', 'answer': {'screen': {'state': 'locked'}, 'waiting_for_the_screen': rows}}

    def delivery(self, name, record):
        return {'handoff_offset': 100, 'given_to_stu': self.given}

    def keys(self, *lines):
        self.state, self.given = 'registered', True

    def clock(self):
        return 0.0


class Steps(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        clock = Clock()
        self.saved = walk.time
        walk.time = clock
        self.addCleanup(setattr, walk, 'time', self.saved)
        self.run_cmd = walk.subprocess.run
        walk.subprocess.run = lambda *a, **k: None  # shot.sh: no picture is taken offline
        self.addCleanup(setattr, walk.subprocess, 'run', self.run_cmd)

    def test_locked_fails_when_the_timeline_shows_no_waiting_line(self):
        w = Walk(self.tmp.name, {})
        with self.assertRaises(walk.StepFailed) as caught:
            w.locked()
        self.assertIn(walk.WAITING_LABEL, str(caught.exception))
        self.assertIs(w.facts['window_shows_waiting'], False)

    def test_locked_fails_on_a_node_that_only_contains_the_words(self):
        # A longer sentence (the record's detail in Under the hood) is not the timeline label.
        w = Walk(self.tmp.name, {walk.WAITING_LABEL: [node(walk.WAITING_LABEL + '. I will carry on.')]})
        with self.assertRaises(walk.StepFailed):
            w.locked()

    def test_locked_passes_when_the_turn_line_reads_the_label(self):
        w = Walk(self.tmp.name, {walk.WAITING_LABEL: [node(walk.WAITING_LABEL)]})
        w.locked()
        self.assertTrue(w.facts['window_shows_waiting'])
        self.assertTrue(all(c.startswith('ok') for c in w.facts['checks']), w.facts['checks'])

    def test_unlock_fails_when_the_carrying_on_line_never_appears(self):
        w = Walk(self.tmp.name, {})
        with self.assertRaises(walk.StepFailed) as caught:
            w.unlock()
        self.assertIn(walk.BACK_LABEL, str(caught.exception))

    def test_unlock_fails_when_the_waiting_line_is_still_there(self):
        w = Walk(self.tmp.name, {walk.BACK_LABEL: [node(walk.BACK_LABEL)], walk.WAITING_LABEL: [node(walk.WAITING_LABEL)]})
        with self.assertRaises(walk.StepFailed):
            w.unlock()

    def test_unlock_passes_when_stu_has_it_and_the_line_carries_on(self):
        w = Walk(self.tmp.name, {walk.BACK_LABEL: [node(walk.BACK_LABEL)]})
        evidence = w.unlock()
        self.assertTrue(evidence['window_carrying_on'])
        self.assertFalse(evidence['window_still_waiting'])


class Records(unittest.TestCase):
    """records() as the guest runs it: ssh hands the command to the guest user's login shell,
    zsh, where an unmatched glob is an error. The 2026-10-25 walk's first read after Send, before
    Rich had written anything, failed exactly so ("no matches found")."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.saved = walk.guest
        walk.guest = self.zsh
        self.addCleanup(setattr, walk, 'guest', self.saved)
        w = Walk(self.tmp.name, {})
        w.data = str(Path(self.tmp.name) / 'Application Support' / 'com.richos.app')
        self.walk = w

    def zsh(self, vm, command, timeout=30):
        r = walk.subprocess.run(['/bin/zsh', '-f', '-c', command], capture_output=True, text=True, timeout=timeout)
        if r.returncode:
            raise RuntimeError(r.stderr or r.stdout or 'guest command failed')
        return r.stdout.strip()

    @unittest.skipUnless(Path('/bin/zsh').exists(), 'the guest shell is zsh')
    def test_no_register_yet_is_no_records(self):
        self.assertEqual(self.walk.records('records.txt'), [])
        (Path(self.walk.data) / 'engine-state' / 'assignments' / 'p').mkdir(parents=True)
        self.assertEqual(self.walk.records('records.txt'), [])

    @unittest.skipUnless(Path('/bin/zsh').exists(), 'the guest shell is zsh')
    def test_every_record_is_one_row_and_an_incoming_file_is_not(self):
        part = Path(self.walk.data) / 'engine-state' / 'assignments' / 'p'
        part.mkdir(parents=True)
        (part / 'a.json').write_text('{"id": "a", "registered_at_ms": 1}')
        (part / 'b.json').write_text('{"id": "b", "registered_at_ms": 2}')
        (part / '.assignment-x.incoming').write_text('{"id": "half"')
        rows = self.walk.records('records.txt')
        self.assertEqual(sorted(r['id'] for r in rows), ['a', 'b'])


if __name__ == '__main__':
    unittest.main()
