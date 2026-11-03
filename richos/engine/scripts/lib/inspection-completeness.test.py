#!/usr/bin/env python3
"""Incomplete inspections never authorize collection, replacement or a CI pass."""
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, patch

import build_caches as B
import recovery_controls as C

HERE = Path(B.__file__).resolve().parent


class InspectionCompleteness(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='inspection-completeness-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_cache_refuses_unreadable_tree(self):
        tree = self.root / 'cache'
        tree.mkdir()
        with patch.object(B.os, 'scandir', side_effect=PermissionError('listing refused')):
            with self.assertRaises(OSError):
                B.walk(tree)
        self.assertEqual(B.walk(tree)[2], False)

    def test_cache_refuses_missing_root_and_unreadable_entry_metadata(self):
        with self.assertRaises(OSError):
            B.walk(self.root / 'gone')
        entry = types.SimpleNamespace(name='artifact', stat=Mock(side_effect=PermissionError('stat refused')))
        with patch.object(B.os, 'scandir', return_value=[entry]):
            with self.assertRaises(OSError):
                B.walk(self.root)

    def test_cache_units_refuse_incomplete_profile_listing(self):
        real = B.os.scandir
        def scan(path):
            if Path(path).name == '.fingerprint':
                raise PermissionError('listing refused')
            return real(path)
        with patch.object(B.os, 'scandir', side_effect=scan):
            with self.assertRaises(OSError):
                B.idle_units(str(self.root), 100, now=1000)
        # Optional profile subdirectories really can be absent.
        self.assertEqual(B.idle_units(str(self.root), 100, now=1000), [])

    def test_recovery_refuses_a_workspace_that_disappeared(self):
        workspace = self.root / 'workspace'
        workspace.mkdir()
        ws = types.SimpleNamespace(git=Mock(side_effect=[(0, 'patch', ''), (0, '', ''), (0, 'tip', '')]))
        rec = dict(key='fixture', workspaces=[dict(path=str(workspace)), dict(path=str(self.root / 'gone'))])
        with patch.object(C.R, 'storage_root', return_value=self.root / 'wip'):
            with self.assertRaises(C.R.Unavailable):
                C.snapshot(ws, rec, 'job')
        self.assertEqual(list((self.root / 'wip').rglob('manifest.json')), [])

    def test_complete_recovery_snapshot_is_preserved(self):
        workspace = self.root / 'workspace'
        workspace.mkdir()
        ws = types.SimpleNamespace(git=Mock(side_effect=[(0, 'patch', ''), (0, '', ''), (0, 'tip', '')]))
        with patch.object(C.R, 'storage_root', return_value=self.root / 'wip'):
            reference = C.snapshot(ws, dict(key='fixture', workspaces=[dict(path=str(workspace))]), 'job')
        self.assertTrue(C.preserved(reference))
        self.assertEqual(len(json.loads(Path(reference).read_text())), 1)

    def witness(self, function, before, unreadable=False):
        script = (HERE.parent / 'ci-shard.sh').read_text()
        body = script.split(function + '() {', 1)[1].split("<<'PY'\n", 1)[1].split('\nPY\n', 1)[0]
        real = os.scandir
        def scan(path):
            if unreadable and Path(path) == self.root:
                raise PermissionError('listing refused')
            return real(path)
        with patch.object(sys, 'argv', ['witness', str(self.root), str(before)]), \
                patch.object(os, 'scandir', side_effect=scan):
            exec(compile(body, str(HERE.parent / 'ci-shard.sh'), 'exec'), {})

    def test_ci_before_witness_refuses_unreadable_directory(self):
        before = self.root.parent / (self.root.name + '.json')
        self.addCleanup(before.unlink, missing_ok=True)
        with self.assertRaises((OSError, SystemExit)):
            self.witness('state_witness', before, unreadable=True)

    def test_ci_after_witness_reports_unreadable_directory(self):
        before = self.root / 'before.json'
        before.write_text('{}')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            try:
                self.witness('state_written', before, unreadable=True)
            except SystemExit:
                pass
        self.assertIn('could not', output.getvalue())

    def test_ci_readable_directory_reports_created_file(self):
        # Keep the witness outside the tree it measures.
        before = self.root.parent / (self.root.name + '.json')
        self.addCleanup(before.unlink, missing_ok=True)
        self.witness('state_witness', before)
        (self.root / 'notice.log').write_text('before\nafter\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.witness('state_written', before)
        self.assertIn('notice.log  created', output.getvalue())


if __name__ == '__main__':
    unittest.main()
