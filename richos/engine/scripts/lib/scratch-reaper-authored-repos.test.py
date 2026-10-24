#!/usr/bin/env python3
"""An ended allocation's own repository, or one in a producer temp root, is deleted only
on positive evidence that it is a disposable test fixture (cleanup PRD R1, A2).

Adapted from the mid-job review's fixture (rv-20261023T212629Z-3dc330ca-a2b9): a real
allocator row and a repository holding one unpushed commit, a tracked edit and untracked
notes, plus a copy of it in a declared producer temp root. A ledger row and repository
containment prove ownership, not disposability, so both must survive. The controls: an
allocation whose label is a declared fixture producer, and a declared harness family in
a producer root, are still collected.

Only this test's synthetic world is read or touched; the open-file table is not scanned.
"""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location("authored_reaper", HERE / "scratch-reaper.py")
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)


def git(repo, *args):
    return subprocess.check_output(
        ["git", "-C", str(repo), "-c", "core.hooksPath=/dev/null", "-c", "init.defaultBranch=main",
         "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", *args],
        text=True, stderr=subprocess.STDOUT, env=dict(os.environ, GIT_CONFIG_NOSYSTEM="1",
                                                       GIT_CONFIG_GLOBAL=os.devnull)).strip()


def authored_repo(path):
    path.mkdir(parents=True)
    git(path, "init", "-q")
    (path / "source.py").write_text("print('base')\n")
    git(path, "add", ".")
    git(path, "commit", "-qm", "base")
    git(path, "checkout", "-qb", "unfinished-feature")
    (path / "source.py").write_text("print('unique authored implementation')\n")
    git(path, "commit", "-qam", "unfinished feature exists only here")
    (path / "source.py").write_text("print('uncommitted authored work')\n")
    (path / "notes.txt").write_text("Untracked handoff notes not preserved elsewhere.\n")


def backdate(tree, seconds):
    old = time.time() - seconds
    for root, dirs, files in os.walk(tree, topdown=False):
        for name in files + dirs:
            os.utime(Path(root) / name, (old, old), follow_symlinks=False)
        os.utime(root, (old, old))


class AuthoredRepositories(unittest.TestCase):
    def setUp(self):
        self.world = Path(tempfile.mkdtemp(prefix="authored-repos-test.")).resolve()
        self.tmp = self.world / "tmp"
        self.tmp.mkdir()
        self.env = dict(os.environ, TMPDIR=str(self.tmp), CLAUDE_CONFIG_DIR=str(self.world / "claude"))
        self.env.pop("SCRATCH_LEDGER", None)
        self.bulk_tmp = self.world / "bulk/tmp/codex"
        self.bulk_tmp.mkdir(parents=True)

    def tearDown(self):
        shutil.rmtree(self.world, ignore_errors=True)

    def allocate(self, label):
        return Path(subprocess.check_output(["bash", str(HERE / "scratch.sh"), "new", label],
                                            env=self.env, text=True).strip())

    def reaper(self):
        r = object.__new__(R.Reaper)
        r.cfg = {"scratch_root_name": "richos-scratch", "shared_tmp_roots": [str(self.bulk_tmp)],
                 "producer_tmp_roots": [str(self.bulk_tmp)], "default_ttl_minutes": 360,
                 "age_floor_minutes": 60, "min_owner_pid": 100, "agent_roots": [],
                 "claude_roots": [], "deny_by_default": True, "unknown_age_hours": 72,
                 "unknown_git_is_fixture": True, "foreign_patterns": [],
                 "legacy_tmp_patterns": ["ws-spec-*"],
                 "fixture_repo_labels": ["operator-fences-suite"]}
        r.now = time.time()
        r.floor = 3600
        r.roots, r.entries, r._standing, r._unknown = [], [], set(), []
        r.live = SimpleNamespace(oldest_start=lambda: time.time())
        r.scope, r.unattributed = {"name": "isolated-test"}, 0
        r.open_under_alloc = lambda *_a: set()
        r.collect_test_instances = lambda *_a: []
        r._record_failures = lambda *_a: None
        walls = object.__new__(R.Walls)
        walls.roots, walls.never, walls.registered = [str(self.tmp), str(self.bulk_tmp)], set(), set()
        return r, walls

    def sweep(self):
        r, walls = self.reaper()
        with patch.dict(os.environ, self.env, clear=True):
            r.scan_scratch_root(walls)
            r.scan_shared_tmp(walls)
            for path, root in r._unknown:
                r.scan_unknown(path, walls, root, set())
            r.apply(str(self.world / "apply.log"))
        return {e.path: e for e in r.entries}

    def test_authored_repositories_survive(self):
        allocation = self.allocate("prototype")
        authored_repo(allocation / "prototype")
        shutil.copytree(allocation / "prototype", self.bulk_tmp / "prototype")
        backdate(allocation, 4 * 86400)
        backdate(self.bulk_tmp / "prototype", 4 * 86400)
        entries = self.sweep()
        self.assertEqual(entries[str(allocation)].action, R.INDETERMINATE, entries[str(allocation)].why)
        self.assertEqual(entries[str(self.bulk_tmp / "prototype")].action, R.INDETERMINATE)
        self.assertTrue((allocation / "prototype" / "notes.txt").exists())
        self.assertTrue((self.bulk_tmp / "prototype" / "notes.txt").exists())

    def test_declared_fixture_producers_are_still_collected(self):
        allocation = self.allocate("operator-fences-suite")
        (allocation / "fixture-repo" / ".git" / "objects").mkdir(parents=True)
        family = self.bulk_tmp / "ws-spec-abc123"
        (family / "repo" / ".git" / "objects").mkdir(parents=True)
        backdate(allocation, 4 * 86400)
        backdate(family, 4 * 86400)
        entries = self.sweep()
        self.assertEqual(entries[str(allocation)].action, R.DELETE, entries[str(allocation)].why)
        self.assertEqual(entries[str(family)].action, R.DELETE, entries[str(family)].why)
        self.assertFalse(allocation.exists())
        self.assertFalse(family.exists())


if __name__ == "__main__":
    unittest.main()
