#!/usr/bin/env python3
"""garbage-walk.py's listings, with no guest: the guest's answers are stubbed.

(1) A listing groups what is left by place, with sizes, and leaves out the control and the walk's
own files. (2) A listing whose control the search did not find fails: its silence proves nothing.
(3) The last listing fails naming every leftover a --gone pattern matches, and passes without one.
(4) `reported`: a failed start sweep must be said as FAILED, with every leftover in the deletion log."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('garbage_walk', HERE / 'garbage-walk.py')
gw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gw)

HOME = '/Users/admin/testvm/walk-x/home'
ROOTS = [HOME, '/private/tmp', '/private/var/folders/9m/x', '/Users/admin/Library', '/Users/Shared']


def scan(files, missing=()):
    return json.dumps({'files': files, 'controls_missing': list(missing), 'unreadable_roots': [],
                       'unreadable_dirs': []})


class Listing(unittest.TestCase):
    def walk(self, out, gone=()):
        w = gw.GarbageWalk.__new__(gw.GarbageWalk)
        w.vm, w.out, w.home, w.payload = 'walk-x', Path(out), HOME, '/Users/admin/testvm/walk-x'
        w.a = mock.Mock(gone=list(gone))
        w.facts = {'garbage': {'mark': '/m', 'roots': ROOTS, 'harness': []}}
        return w

    def answer(self, listing):
        def guest(vm, command, timeout=30):
            if 'files-since.py' in command:
                return listing
            if 'ps' in command and 'python3 -c' in command:
                return '[]'
            return ''
        return mock.patch.object(gw, 'guest', guest)

    def test_what_is_left_is_grouped_by_place_with_sizes(self):
        files = [{'path': '/private/tmp/claude-501/slug/s1/tasks/a.output', 'kind': 'file', 'bytes': 7, 'class': 'other'},
                 {'path': '/private/tmp/claude-501/slug/s1/tasks/b.output', 'kind': 'file', 'bytes': 5, 'class': 'other'},
                 {'path': HOME + '/garbage-walk-control-x', 'kind': 'file', 'bytes': 8, 'class': 'control'}]
        with tempfile.TemporaryDirectory() as out, self.answer(scan(files)):
            got = self.walk(out).listing('x')
        self.assertEqual(got['entries'], 2)
        self.assertEqual(got['places'], {'/private/tmp/claude-501/slug/s1': {'files': 2, 'dirs': 0, 'bytes': 12}})

    def test_a_listing_that_did_not_find_its_control_fails(self):
        with tempfile.TemporaryDirectory() as out, self.answer(scan([], missing=[HOME + '/c'])):
            with self.assertRaises(gw.StepFailed):
                self.walk(out).listing('x')

    def test_a_leftover_is_what_was_there_before_and_is_still_there(self):
        killed = '/private/tmp/claude-501/slug/s-killed'
        new = '/private/tmp/claude-501/slug/s-new'
        kept = HOME + '/Library/Caches/kept.db'
        row = lambda p: {'path': p, 'kind': 'dir', 'bytes': 0, 'class': 'other'}
        gone = [r'^/private/tmp/claude-501/[^/]+/s-[a-z]+$']
        with tempfile.TemporaryDirectory() as out:
            (Path(out) / 'listing-kill.json').write_text(json.dumps({'files': [row(killed), row(kept)]}))
            with self.answer(scan([row(killed), row(new), row(kept)])):
                with self.assertRaises(gw.StepFailed) as failed:
                    self.walk(out, gone=gone).listing('final')
            self.assertIn(killed, str(failed.exception))
            self.assertNotIn(new, str(failed.exception), 'what the run now running made is not a leftover')
            with self.answer(scan([row(new), row(kept)])):
                got = self.walk(out, gone=gone).listing('final')
            self.assertEqual((got['were_there'], got['still_there']), ([killed], []))
            with self.answer(scan([row(killed)])):
                self.assertNotIn('still_there', self.walk(out, gone=gone).listing('restart'),
                                 'only the listings after the Quit and after the kill judge --gone')


class Reported(unittest.TestCase):
    """(4) The start sweep after the kill must say FAILED for the base it could not delete, and
    the engine's deletion log must carry each leftover; a clean line for that sweep fails."""
    PLANTED = {'split_standing': HOME + '/Library/Application Support/com.richos.app/engine-profiles/a-standing',
               'undeletable_base': HOME + '/Library/Application Support/com.richos.app/engine-profiles/b',
               'undeletable_standing': HOME + '/Library/Application Support/com.richos.app/engine-profiles/b-standing'}
    WAV = HOME + '/Library/Application Support/com.richos.app/voice-scratch/utt-1103.wav'

    def walk(self, out):
        w = gw.GarbageWalk.__new__(gw.GarbageWalk)
        w.vm, w.out, w.home, w.payload = 'walk-x', Path(out), HOME, '/Users/admin/testvm/walk-x'
        w.facts = {'planted': dict(self.PLANTED), 'abandoned_recording': self.WAV,
                   'logs': {'restart-after-kill': '/Users/admin/testvm/walk-x/relaunch-1.log'}}
        return w

    def answer(self, line, deletions, there='there'):
        def guest(vm, command, timeout=30):
            if 'relaunch-1.log' in command:
                return 'other\n' + line + '\n'
            if 'scratch-reaper.log' in command:
                return deletions
            if command.startswith('test -d'):
                return there
            return ''
        return mock.patch.object(gw, 'guest', guest)

    def log(self):
        return '\n'.join(['t DELETED %s bytes=1 class=app-profile why=x' % self.PLANTED['split_standing'],
                          't DELETED %s bytes=1 class=app-profile why=x' % self.PLANTED['undeletable_standing'],
                          't FAILED %s class=app-profile error=denied' % self.PLANTED['undeletable_base'],
                          't DELETED %s bytes=32044 class=app-voice why=x' % self.WAV])

    def test_a_failed_sweep_said_as_failed_with_every_deletion_logged_passes(self):
        line = '[richos] cleanup at start: FAILED, deleted 9, undecidable 0, failures 1 (verdict: x)'
        with tempfile.TemporaryDirectory() as out, self.answer(line, self.log()):
            got = self.walk(out).reported()
        self.assertEqual(got['failures'], 1)

    def test_a_clean_line_for_a_sweep_that_could_not_delete_fails(self):
        line = '[richos] cleanup at start: deleted 9, undecidable 0, failures 0 (verdict: x)'
        with tempfile.TemporaryDirectory() as out, self.answer(line, self.log()):
            with self.assertRaises(gw.StepFailed):
                self.walk(out).reported()

    def test_a_leftover_missing_from_the_deletion_log_fails(self):
        line = '[richos] cleanup at start: FAILED, deleted 9, undecidable 0, failures 1 (verdict: x)'
        without_wav = '\n'.join(l for l in self.log().splitlines() if 'utt-1103' not in l)
        with tempfile.TemporaryDirectory() as out, self.answer(line, without_wav):
            with self.assertRaises(gw.StepFailed) as failed:
                self.walk(out).reported()
        self.assertIn('utt-1103.wav', str(failed.exception))


if __name__ == '__main__':
    unittest.main(verbosity=2)
