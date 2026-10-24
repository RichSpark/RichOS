#!/usr/bin/env python3
"""work-lease-walk.py's verdict, with no guest: its steps are stubbed on the walk class.

(1) Nightly 74 walk: a failed final `keep` (the transcripts and rows NOT carried off the guest before
it is deleted) left the exit code 0. A keep failure is a failed walk."""
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

spec = importlib.util.spec_from_file_location('work_lease_walk', HERE / 'work-lease-walk.py')
wlw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wlw)
Walk = wlw.WorkLeaseWalk


def run_main(out, steps, **stubs):
    argv = ['work-lease-walk.py', 'walk-x', '--out', str(out), '--expect-sha', 'abc', '--steps', steps]
    with mock.patch.object(Walk, '__init__', lambda self, a: None), mock.patch.object(sys, 'argv', argv):
        patches = [mock.patch.object(Walk, k, v) for k, v in stubs.items()]
        for p in patches:
            p.start()
        try:
            return wlw.main()
        finally:
            for p in patches:
                p.stop()


class KeepIsPartOfTheVerdict(unittest.TestCase):
    def test_a_failed_keep_fails_the_walk(self):
        with tempfile.TemporaryDirectory() as out:
            def keep(self):
                raise wlw.StepFailed('guest.sh --pull failed')
            self.assertEqual(run_main(out, 'identity', identity=lambda self: {}, keep=keep), 1)
            rows = json.loads((Path(out) / 'report.json').read_text())['steps']
            self.assertEqual([(r['step'], r['outcome']) for r in rows], [('identity', 'PASS'), ('keep', 'FAIL')])

    def test_every_step_and_keep_passing_exits_0(self):
        with tempfile.TemporaryDirectory() as out:
            self.assertEqual(run_main(out, 'identity', identity=lambda self: {}, keep=lambda self: {'kept': 'x'}), 0)


if __name__ == '__main__':
    unittest.main()
