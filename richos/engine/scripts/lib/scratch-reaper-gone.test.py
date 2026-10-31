#!/usr/bin/env python3
"""A deletion whose target is already gone is a success, never a failure or an alarm.

Escalation esc-20261031T105739Z-738019a9: a probe directory its owner removed between the
plan and the delete raised ENOENT and was reported as a failed deletion. Any other error
(permission) stays a failure. Everything runs in a temporary directory.
"""
import importlib.util
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("scratch_reaper", HERE / "scratch-reaper.py")
reaper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reaper)


class AlreadyGone(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="reaper-gone-test.")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def apply(self, paths, rmtree=None, klass="scratch"):
        r = object.__new__(reaper.Reaper)
        r.scope = None
        r.entries = [reaper.Entry(p, klass, 10, reaper.DELETE, "test") for p in paths]
        recorded = []
        r.named_scratch = lambda p: False
        r.collect_test_instances = lambda paths, stamp: []
        r.collect_test_devices = lambda stamp: []
        r.prune_docker = lambda: []
        r.app_spent = lambda until: False
        r.counts = lambda: (0, 0)
        r._record_failures = lambda failures, stamp: recorded.append(list(failures))
        log = os.path.join(self.tmp, "log")
        with mock.patch.object(reaper, "write_log", lambda path, lines: recorded.append(lines)):
            if rmtree:
                with mock.patch("shutil.rmtree", rmtree):
                    out = r.apply(log)
            else:
                out = r.apply(log)
        return out, recorded

    def test_removed_between_plan_and_delete_is_gone_not_failed(self):
        victim = os.path.join(self.tmp, "fswatch-probe-x")
        os.mkdir(victim)

        def vanish(path, *a, **k):
            os.rmdir(path)               # the owner removed it itself ...
            raise FileNotFoundError(2, "No such file or directory", path)  # ... mid-delete
        (deleted, freed, failures), rec = self.apply([victim], rmtree=vanish)
        self.assertEqual(failures, [])
        self.assertEqual(rec[1], [])      # nothing recorded for the alarm
        self.assertTrue(any(" GONE " in l for l in rec[0]), rec[0])
        self.assertFalse(any("FAILED" in l for l in rec[0]))

    def test_permission_error_is_still_a_failure(self):
        victim = os.path.join(self.tmp, "locked")
        os.mkdir(victim)

        def deny(path, *a, **k):
            raise PermissionError(13, "Permission denied", path)
        (deleted, freed, failures), rec = self.apply([victim], rmtree=deny)
        self.assertEqual(len(failures), 1)
        self.assertIn("Permission denied", failures[0])

    def test_gone_during_the_permission_retry_is_gone_not_failed(self):
        victim = os.path.join(self.tmp, "locked-then-gone")
        os.mkdir(victim)
        calls = []

        def deny_then_vanish(path, *a, **k):
            calls.append(path)
            if len(calls) == 1:
                raise PermissionError(13, "Permission denied", path)
            os.rmdir(path)               # the owner removed it before the retry ...
            raise FileNotFoundError(2, "No such file or directory", path)
        (deleted, freed, failures), rec = self.apply([victim], rmtree=deny_then_vanish, klass="scratch-alloc")
        self.assertEqual(len(calls), 2)
        self.assertEqual(failures, [])
        self.assertEqual(rec[1], [])
        self.assertTrue(any(" GONE " in l for l in rec[0]), rec[0])
        self.assertFalse(any("FAILED" in l for l in rec[0]))

    def test_other_error_during_the_permission_retry_is_still_a_failure(self):
        victim = os.path.join(self.tmp, "locked-twice")
        os.mkdir(victim)

        def deny(path, *a, **k):
            raise PermissionError(13, "Permission denied", path)
        (deleted, freed, failures), rec = self.apply([victim], rmtree=deny, klass="scratch-alloc")
        self.assertEqual(len(failures), 1)


if __name__ == "__main__":
    unittest.main()
