#!/usr/bin/env python3
"""The internal-disk classes (2026-11-10): Cargo output inside a checkout, and what keeps it.

The 6-hourly sweep freed 75 GB while the internal disk fell to 45 GB free, because 71 GB of Cargo
output sat inside the nightly's source checkout, which walls 2 and 3 refuse. The rule under test:
a `target` folder Cargo made is regenerable wherever it sits, so it is planned DELETE while no
build is using it, and the checkout's own files are never planned. State is a temporary directory;
the reaper only PLANS here, nothing is deleted.
"""
import fcntl
import importlib.util
import os
from pathlib import Path
import shutil
import tempfile
import time
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("scratch_reaper", HERE / "scratch-reaper.py")
reaper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reaper)

CARGO_TAG = "Signature: 8a477f597d28d172789f06886806bc55\n# made by cargo\n"


class FakeWalls(object):
    never = set()
    registered = set()


class CargoOutput(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="reaper-internal-test."))
        self.nightly = os.path.join(self.tmp, "nightly")
        self.checkout = os.path.join(self.nightly, "source")
        self.target = os.path.join(self.checkout, "app", "target")
        os.makedirs(os.path.join(self.target, "debug"))
        Path(self.target, "CACHEDIR.TAG").write_text(CARGO_TAG)
        Path(self.target, "debug", "big.rlib").write_text("x" * 100)
        Path(self.checkout, "app", "main.rs").write_text("fn main() {}")
        old = time.time() - 7200
        for root, _dirs, files in os.walk(self.target):
            for f in files:
                os.utime(os.path.join(root, f), (old, old))
            os.utime(root, (old, old))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def plan(self):
        r = object.__new__(reaper.Reaper)
        r.cfg = {"nightly_dir": self.nightly, "age_floor_minutes": 30}
        r.floor = 1800
        r.now = time.time()
        r.decisions = []
        r.add = lambda path, klass, size, action, why, **_k: r.decisions.append((path, klass, action, str(why)))
        r.command_lines = lambda: []
        r.machine_open_paths = lambda: []
        with patch.object(reaper, "registered_workspaces", return_value=set()), \
                patch.object(reaper, "check_deadline", return_value=None):
            r.scan_cargo_output(FakeWalls())
        return r.decisions

    def test_cargo_output_inside_a_checkout_is_planned_and_the_checkout_files_are_not(self):
        rows = self.plan()
        self.assertEqual([(p, k, a) for p, k, a, _w in rows], [(self.target, "cargo-output", reaper.DELETE)], rows)

    def test_a_folder_named_target_without_cargos_signature_is_not_build_output(self):
        Path(self.target, "CACHEDIR.TAG").write_text("Signature: not-cargo\n")
        self.assertEqual(self.plan(), [])

    def test_a_nightly_holding_its_lock_keeps_everything(self):
        lock = os.path.join(self.nightly, "release.lock")
        Path(lock).write_text("")
        with open(lock, "rb") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            rows = self.plan()
        self.assertEqual([a for _p, _k, a, _w in rows], [reaper.KEEP], rows)

    def test_a_build_holding_cargos_own_lock_is_kept(self):
        lock = os.path.join(self.target, "debug", ".cargo-lock")
        Path(lock).write_text("")
        with open(lock, "rb") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            rows = self.plan()
        self.assertEqual([a for _p, _k, a, _w in rows], [reaper.KEEP], rows)

    def test_output_changed_inside_the_floor_is_kept(self):
        Path(self.target, "debug", "fresh.o").write_text("y")
        rows = self.plan()
        self.assertEqual([a for _p, _k, a, _w in rows], [reaper.KEEP], rows)

    def test_output_a_running_process_uses_is_kept(self):
        for who, lines, opened in (
                ("command line", [self.target + "/release/service"], []),
                ("open file or working directory", [], [("4242", self.target + "/release")])):
            r = object.__new__(reaper.Reaper)
            r.cfg = {"nightly_dir": self.nightly, "age_floor_minutes": 30}
            r.floor, r.now, r.entries = 1800, time.time(), []
            r.command_lines = lambda lines=lines: lines
            r.machine_open_paths = lambda opened=opened: opened
            r.add = lambda path, klass, size, action, why, **_k: r.entries.append((action, str(why)))
            with patch.object(reaper, "registered_workspaces", return_value=set()), \
                    patch.object(reaper, "check_deadline", return_value=None):
                r.scan_cargo_output(FakeWalls())
            self.assertEqual([a for a, _w in r.entries], [reaper.KEEP], (who, r.entries))

    def test_output_is_not_deleted_when_the_tables_cannot_be_read(self):
        r = object.__new__(reaper.Reaper)
        r.cfg = {"nightly_dir": self.nightly, "age_floor_minutes": 30}
        r.floor, r.now, r.entries = 1800, time.time(), []
        r.command_lines = lambda: []
        r.machine_open_paths = lambda: None
        r.add = lambda path, klass, size, action, why, **_k: r.entries.append((action, str(why)))
        with patch.object(reaper, "registered_workspaces", return_value=set()), \
                patch.object(reaper, "check_deadline", return_value=None):
            r.scan_cargo_output(FakeWalls())
        self.assertEqual([a for a, _w in r.entries], [reaper.INDETERMINATE], r.entries)

    def test_a_registered_workspace_inside_the_candidate_is_refused(self):
        r = object.__new__(reaper.Reaper)
        r.cfg = {"nightly_dir": self.nightly, "age_floor_minutes": 30}
        r.floor, r.now, r.decisions = 1800, time.time(), []
        r.add = lambda path, klass, size, action, why, **_k: r.decisions.append((path, klass, action, str(why)))
        walls = FakeWalls()
        walls.registered = {os.path.join(self.target, "debug")}
        r.command_lines = lambda: []
        r.machine_open_paths = lambda: []
        with patch.object(reaper, "registered_workspaces", return_value=set()), \
                patch.object(reaper, "check_deadline", return_value=None):
            r.scan_cargo_output(walls)
        self.assertEqual([a for _p, _k, a, _w in r.decisions], [reaper.INDETERMINATE], r.decisions)


class WorkProof(unittest.TestCase):
    """Our own test-fixture checkouts are decided; real work is not (2026-11-10)."""

    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="reaper-workproof-test."))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def git(self, repo, *args, email="t@t"):
        import subprocess
        env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL=email, GIT_COMMITTER_NAME="t",
                   GIT_COMMITTER_EMAIL=email, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null")
        subprocess.run(["git", "-C", repo] + list(args), check=True, capture_output=True, env=env)

    def clone(self, name="work"):
        remote = os.path.join(self.tmp, "remote.git")
        if not os.path.isdir(remote):
            os.makedirs(remote)
            self.git(remote, "init", "--bare", "-b", "main")
        repo = os.path.join(self.tmp, name)
        os.makedirs(repo)
        self.git(repo, "init", "-b", "main")
        Path(repo, "a.txt").write_text("a\n")
        self.git(repo, "add", "a.txt")
        self.git(repo, "commit", "-m", "first")
        self.git(repo, "remote", "add", "origin", remote)
        self.git(repo, "push", "origin", "main")
        return repo

    def proof(self, path):
        r = object.__new__(reaper.Reaper)
        with patch.object(reaper, "check_deadline", return_value=None):
            return r.work_proof(path)

    def test_a_clean_clone_with_nothing_unpushed_is_disposable(self):
        self.assertEqual(self.proof(self.clone()), "")

    def test_uncommitted_work_is_kept(self):
        repo = self.clone()
        Path(repo, "a.txt").write_text("changed\n")
        self.assertIn("uncommitted work", self.proof(repo))

    def test_an_untracked_file_is_work_too(self):
        repo = self.clone()
        Path(repo, "notes.txt").write_text("mine\n")
        self.assertIn("uncommitted work", self.proof(repo))

    def test_a_commit_no_remote_has_is_kept(self):
        repo = self.clone()
        Path(repo, "b.txt").write_text("b\n")
        self.git(repo, "add", "b.txt")
        self.git(repo, "commit", "-m", "second")
        self.assertIn("no remote has", self.proof(repo))

    def test_stashed_work_is_kept(self):
        repo = self.clone()
        Path(repo, "a.txt").write_text("changed\n")
        self.git(repo, "stash")
        self.assertIn("stashed work", self.proof(repo))

    def test_a_commit_only_a_detached_head_holds_is_kept(self):
        repo = self.clone()
        self.git(repo, "checkout", "--detach")
        Path(repo, "b.txt").write_text("b\n")
        self.git(repo, "add", "b.txt")
        self.git(repo, "commit", "-m", "detached work")
        self.assertIn("no remote has", self.proof(repo))

    def test_a_repository_with_no_remote_is_kept(self):
        repo = os.path.join(self.tmp, "proto")
        os.makedirs(repo)
        self.git(repo, "init", "-b", "main")
        Path(repo, "a.txt").write_text("a\n")
        self.git(repo, "add", "a.txt")
        self.git(repo, "commit", "-m", "mine")
        self.assertIn("no remote", self.proof(repo))

    def unborn(self, name, files):
        repo = os.path.join(self.tmp, name)
        os.makedirs(repo)
        self.git(repo, "init", "-b", "main")
        for f in files:
            Path(repo, f).write_text("x\n")
        r = object.__new__(reaper.Reaper)
        r.cfg = {"fixture_files": ["worker-output.txt"]}
        with patch.object(reaper, "check_deadline", return_value=None):
            return r.work_proof(repo)

    def test_an_empty_repository_with_no_commit_is_disposable(self):
        self.assertEqual(self.unborn("empty", []), "")

    def test_a_repository_with_no_commit_holding_only_harness_files_is_disposable(self):
        self.assertEqual(self.unborn("harness", ["worker-output.txt"]), "")

    def test_a_new_project_with_no_commit_and_real_files_is_kept(self):
        self.assertIn("no commit yet", self.unborn("project", ["source.py"]))
        self.assertIn("no commit yet", self.unborn("mixed", ["worker-output.txt", "main.rs"]))

    def test_a_harness_made_repository_is_disposable_whatever_its_tree_holds(self):
        repo = os.path.join(self.tmp, "fx")
        os.makedirs(repo)
        self.git(repo, "init", "-b", "main")
        Path(repo, "a.txt").write_text("a\n")
        self.git(repo, "add", "a.txt")
        self.git(repo, "commit", "-m", "Initialize", email="richos@localhost")
        Path(repo, "loose.txt").write_text("x\n")
        r = object.__new__(reaper.Reaper)
        r.cfg = {"fixture_emails": ["richos@localhost"]}
        with patch.object(reaper, "check_deadline", return_value=None):
            self.assertEqual(r.work_proof(repo), "")
        # ...and the same repository without the declared identity is someone's work.
        r.cfg = {"fixture_emails": []}
        with patch.object(reaper, "check_deadline", return_value=None):
            self.assertIn("uncommitted work", r.work_proof(repo))

    def test_every_repository_inside_is_read(self):
        outer = os.path.join(self.tmp, "outer")
        os.makedirs(outer)
        inner = self.clone("outer/inner")
        Path(inner, "dirty.txt").write_text("x\n")
        self.assertIn("uncommitted work", self.proof(outer))


class Toolchain(unittest.TestCase):
    """The cleanup never starts developer tools to find out whether they exist."""

    def test_no_xcode_no_process_and_no_simulator_work(self):
        r = object.__new__(reaper.Reaper)
        r.skip_build_caches = False
        with patch.object(reaper.Reaper, "simulator_toolchain", return_value=False), \
                patch.object(reaper.subprocess, "run", side_effect=AssertionError("a process was started")):
            self.assertIsNone(r.sim_devices())

    def test_a_notice_never_runs_developer_tools_even_where_they_exist(self):
        r = object.__new__(reaper.Reaper)
        r.skip_build_caches = True
        with patch.object(reaper.Reaper, "simulator_toolchain", return_value=True), \
                patch.object(reaper.subprocess, "run", side_effect=AssertionError("a process was started")):
            self.assertIsNone(r.sim_devices())

    def test_the_toolchain_is_looked_for_on_disk_only(self):
        with patch.object(reaper.subprocess, "run", side_effect=AssertionError("a process was started")), \
                patch.dict(os.environ, {"DEVELOPER_DIR": "/nonexistent-developer-dir"}):
            self.assertIsInstance(reaper.Reaper.simulator_toolchain(), bool)


class NamedScratch(unittest.TestCase):
    """Scratch of an agent that is not running goes after the declared retention."""

    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="reaper-named-test."))
        self.root = os.path.join(self.tmp, "agents")
        os.makedirs(self.root)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def plan(self, name, age_days, holder="", with_repo=False):
        path = os.path.join(self.root, name)
        os.makedirs(os.path.join(path, ".git" if with_repo else "x"))
        Path(path, "f.txt").write_text("scratch")
        then = time.time() - age_days * 86400
        for root, _d, files in os.walk(path):
            for f in files:
                os.utime(os.path.join(root, f), (then, then))
            os.utime(root, (then, then))
        r = object.__new__(reaper.Reaper)
        r.cfg = {"agent_roots": [self.root], "age_floor_minutes": 30, "named_retention_days": 3}
        r.floor, r.now, r.roots, r.entries = 1800, time.time(), [], []
        r.skip_named = False
        r.live_agent_names = lambda: (set(), {})
        r.holder = lambda p: holder
        with patch.object(reaper, "check_deadline", return_value=None):
            r.scan_named_scratch(FakeWalls())
        return r.entries

    def test_scratch_of_an_agent_not_running_goes_after_the_retention(self):
        (e,) = self.plan("zach-old", 5)
        self.assertEqual((e.action, getattr(e, "named_ok", False)), (reaper.DELETE, True), e.why)

    def test_it_is_kept_inside_the_retention(self):
        (e,) = self.plan("zach-new", 1)
        self.assertEqual(e.action, reaper.KEEP, e.why)

    def test_a_directory_holding_a_repository_is_never_taken(self):
        (e,) = self.plan("zach-repo", 9, with_repo=True)
        self.assertEqual(e.action, reaper.INDETERMINATE, e.why)

    def test_a_directory_a_process_holds_open_is_never_taken(self):
        (e,) = self.plan("zach-held", 9, holder="4242")
        self.assertEqual(e.action, reaper.INDETERMINATE, e.why)


if __name__ == "__main__":
    unittest.main()
