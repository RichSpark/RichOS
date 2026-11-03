#!/usr/bin/env python3
"""Incomplete inspections never authorize collection, replacement or a CI pass."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import time
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

    def assert_cache_protected(self, path):
        output = io.StringIO()
        with patch.object(B.time, 'time', return_value=1000), contextlib.redirect_stderr(output):
            self.assertEqual(B.walk(path), (0, 1000, True))
        self.assertIn('incomplete', output.getvalue())

    def test_cache_protects_unreadable_tree(self):
        tree = self.root / 'cache'
        tree.mkdir()
        with patch.object(B.os, 'scandir', side_effect=PermissionError('listing refused')):
            self.assert_cache_protected(tree)
        self.assertEqual(B.walk(tree)[2], False)

    def test_cache_protects_missing_root_and_unreadable_entry_metadata(self):
        self.assert_cache_protected(self.root / 'gone')
        entry = types.SimpleNamespace(name='artifact', stat=Mock(side_effect=PermissionError('stat refused')))
        with patch.object(B.os, 'scandir', return_value=[entry]):
            self.assert_cache_protected(self.root)

    def test_cache_units_refuse_incomplete_profile_listing(self):
        real = B.os.scandir
        def scan(path):
            if Path(path).name == '.fingerprint':
                raise PermissionError('listing refused')
            return real(path)
        output = io.StringIO()
        with patch.object(B.os, 'scandir', side_effect=scan), contextlib.redirect_stderr(output):
            self.assertEqual(B.idle_units(str(self.root), 100, now=1000), [])
        self.assertIn('incomplete', output.getvalue())
        # Optional profile subdirectories really can be absent.
        self.assertEqual(B.idle_units(str(self.root), 100, now=1000), [])

    def test_sweep_continues_after_a_trash_file_disappears(self):
        spec = importlib.util.spec_from_file_location('cache_sweep_fixture', HERE / 'scratch-reaper.py')
        reaper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reaper)
        now = 1700000000
        for fresh in (False, True):
            with self.subTest(fresh=fresh):
                cache = self.root / str(fresh)
                trash = Path(B.trash_dir(str(cache)))
                stamp = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime(now if fresh else now - 7200))
                bad = trash / (stamp + '-bad')
                good = trash / (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime(now - 7200)) + '-good')
                bad.mkdir(parents=True)
                good.mkdir()
                (bad / 'artifact').write_text('concurrent deletion')
                (good / 'artifact').write_text('idle output')
                real = os.scandir
                def scan(path):
                    entries = list(real(path))
                    if Path(path) == bad:
                        (bad / 'artifact').unlink()
                    return iter(entries)
                added = []
                me = types.SimpleNamespace(cfg=dict(build_cache_root=str(cache), cargo_target_roots=[],
                    age_floor_minutes=60, build_cache_orphan_hours=24, build_cache_idle_hours=72,
                    checkout_parents=[]), skip_build_caches=False, roots=[], now=now, floor=3600,
                    entries=[], add=lambda *args: added.append(args))
                walls = types.SimpleNamespace(registered=[], check=lambda path, protected:
                                              'tree is protected' if protected else '')
                with patch.object(reaper, 'build_caches', return_value=B), \
                        patch.object(B, 'live_checkouts', return_value=([], [])), \
                        patch.object(B.os, 'scandir', side_effect=scan), \
                        contextlib.redirect_stderr(io.StringIO()):
                    reaper.Reaper.scan_build_caches(me, walls)
                planned = {Path(row[0]).name: row[3] for row in added}
                self.assertNotEqual(planned[bad.name], reaper.DELETE)
                self.assertEqual(planned[good.name], reaper.DELETE)

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
