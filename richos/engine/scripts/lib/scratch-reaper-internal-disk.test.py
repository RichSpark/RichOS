#!/usr/bin/env python3
"""The internal-disk classes (2026-11-10): Cargo output inside a checkout, and what keeps it.

The 6-hourly sweep freed 75 GB while the internal disk fell to 45 GB free, because 71 GB of Cargo
output sat inside the nightly's source checkout, which walls 2 and 3 refuse. The rule under test:
a `target` folder Cargo made is regenerable wherever it sits, so it is planned DELETE while no
build is using it, and the checkout's own files are never planned. State is a temporary directory;
the reaper only PLANS here, nothing is deleted.
"""
import contextlib
import fcntl
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
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

    def plan(self, lsof_stdout='p4242\nn/unrelated\n', lsof_status=0):
        r = object.__new__(reaper.Reaper)
        r.cfg = {"nightly_dir": self.nightly, "age_floor_minutes": 30}
        r.floor = 1800
        r.now = time.time()
        r.decisions = []
        r.add = lambda path, klass, size, action, why, **_k: r.decisions.append((path, klass, action, str(why)))
        r.command_lines = lambda: []
        with patch.object(reaper, "registered_workspaces", return_value=set()), \
                patch.object(reaper, "check_deadline", return_value=None), \
                patch.object(reaper.subprocess, "run", return_value=subprocess.CompletedProcess(
                    ["lsof"], lsof_status, stdout=lsof_stdout, stderr="")):
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

    def test_unreadable_lock_keeps_output(self):
        real_open = open
        for lock in (os.path.join(self.nightly, "release.lock"),
                     os.path.join(self.target, "debug", ".cargo-lock")):
            with self.subTest(lock=lock):
                Path(lock).touch()
                # Keep the age guard from hiding an incorrect lock decision.
                old = time.time() - 7200
                os.utime(lock, (old, old))
                os.utime(os.path.dirname(lock), (old, old))

                def deny_lock(path, *args, **kwargs):
                    if path == lock:
                        raise PermissionError(path)
                    return real_open(path, *args, **kwargs)

                with patch("builtins.open", side_effect=deny_lock):
                    (row,) = self.plan()
                self.assertEqual(row[2], reaper.INDETERMINATE, row)
                self.assertIn(lock, row[3])
                os.unlink(lock)

    def test_partial_open_file_table_cannot_establish_idleness(self):
        for stdout, action in (("", reaper.INDETERMINATE),
                               ("p4242\nn/unrelated\n", reaper.INDETERMINATE),
                               ("p4242\nn" + self.target + "/debug/big.rlib\n", reaper.KEEP)):
            with self.subTest(stdout=stdout):
                (row,) = self.plan(lsof_stdout=stdout, lsof_status=1)
                self.assertEqual(row[2], action, row)

    def test_unreadable_profile_keeps_output(self):
        scandir = os.scandir
        profile = os.path.join(self.target, "debug")

        def deny_profile(path):
            if path == profile:
                raise PermissionError(path)
            return scandir(path)

        with patch.object(reaper.os, "scandir", side_effect=deny_profile):
            (row,) = self.plan()
        self.assertEqual(row[2], reaper.INDETERMINATE, row)
        self.assertIn("could not be fully inspected", row[3])

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
            r.machine_open_paths = lambda opened=opened: (opened, True)
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
        r.machine_open_paths = lambda: ([], False)
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
        r.machine_open_paths = lambda: ([], True)
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

    def test_a_commit_only_a_local_tag_holds_is_kept(self):
        repo = self.clone()
        self.git(repo, "checkout", "--detach")
        Path(repo, "b.txt").write_text("b\n")
        self.git(repo, "add", "b.txt")
        self.git(repo, "commit", "-m", "tagged work")
        self.git(repo, "tag", "save-work")
        self.git(repo, "checkout", "main")
        self.assertIn("local reference", self.proof(repo))

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
    """The cleanup resolves the selected Xcode on disk before starting simulator tools."""

    def setUp(self):
        root = tempfile.TemporaryDirectory(prefix="reaper-toolchain-test.")
        self.addCleanup(root.cleanup)
        self.root = Path(root.name)
        env = patch.dict(os.environ, {"RICHOS_TOOLCHAIN_ROOT": str(self.root)}, clear=True)
        env.start()
        self.addCleanup(env.stop)
        access = os.access
        fixture_only = patch.object(os, "access", side_effect=lambda path, mode:
                                    os.fspath(path).startswith(str(self.root) + os.sep) and access(path, mode))
        fixture_only.start()
        self.addCleanup(fixture_only.stop)
        self.reaper = object.__new__(reaper.Reaper)
        self.reaper.skip_build_caches = False

    def install_xcode(self, rel):
        developer = self.root / rel / "Contents" / "Developer"
        simctl = developer / "usr" / "bin" / "simctl"
        simctl.parent.mkdir(parents=True)
        simctl.touch(mode=0o755)
        return developer

    def select(self, developer):
        link = self.root / "var" / "db" / "xcode_select_link"
        link.parent.mkdir(parents=True)
        link.symlink_to(developer)

    def test_lists_simulators_from_custom_selected_xcode(self):
        developer = self.install_xcode("Volumes/Tools/Xcode Beta.app")
        self.select(developer)
        devices = '{"devices":{"runtime":[{"name":"iPhone","udid":"id","state":"Shutdown",'
        devices += '"deviceTypeIdentifier":"type","dataPath":"/fixture/data"}]}}'
        types = '{"devicetypes":[{"identifier":"type","name":"iPhone"}]}'
        with patch.object(reaper.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 0, stdout=devices),
            subprocess.CompletedProcess([], 0, stdout=types),
        ]) as run:
            self.assertEqual(self.reaper.sim_devices(),
                             [("iPhone", "id", "Shutdown", "iPhone", "runtime", "/fixture/data")])
        self.assertEqual([c.args[0] for c in run.call_args_list],
                         [["xcrun", "simctl", "list", kind, "-j"] for kind in ("devices", "devicetypes")])
        for call in run.call_args_list:
            self.assertEqual(call.kwargs["env"]["DEVELOPER_DIR"], str(developer))

    def test_explicit_xcode_overrides_the_selected_directory(self):
        self.select(self.install_xcode("Applications/Xcode.app"))
        developer = self.install_xcode("Volumes/Tools/Xcode Beta.app")
        os.environ["DEVELOPER_DIR"] = str(developer)
        with patch.object(reaper.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 0, stdout='{"devices":{}}'),
            subprocess.CompletedProcess([], 0, stdout='{"devicetypes":[]}'),
        ]) as run:
            self.assertEqual(self.reaper.sim_devices(), [])
        self.assertEqual(run.call_args.kwargs["env"]["DEVELOPER_DIR"], str(developer))

    def test_selected_command_line_tools_do_not_start_simctl_even_with_default_xcode(self):
        self.install_xcode("Applications/Xcode.app")
        selected = self.root / "Library" / "Developer" / "CommandLineTools"
        (selected / "usr" / "bin").mkdir(parents=True)
        self.select(selected)
        with patch.object(reaper.subprocess, "run") as run:
            self.assertIsNone(self.reaper.sim_devices())
        run.assert_not_called()

    def test_no_xcode_no_process_and_no_simulator_work(self):
        with patch.object(reaper.subprocess, "run") as run:
            self.assertIsNone(self.reaper.sim_devices())
        run.assert_not_called()

    def test_a_notice_never_runs_developer_tools_even_where_they_exist(self):
        os.environ["DEVELOPER_DIR"] = str(self.install_xcode("Applications/Xcode.app"))
        self.reaper.skip_build_caches = True
        with patch.object(reaper.subprocess, "run") as run:
            self.assertIsNone(self.reaper.sim_devices())
        run.assert_not_called()


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


class AppBudget(unittest.TestCase):
    """The app's bounded sweep: the Cargo arm never takes the time already planned cleanup needs."""

    def run_sweep(self, scan_seconds, budget):
        clock = [100_000.0]
        r = object.__new__(reaper.Reaper)
        r.cfg = {"nightly_dir": "/n", "age_floor_minutes": 60}
        r.floor, r.now = 3600, clock[0]
        r.scope, r.unattributed = {"kind": "app"}, 0
        r.entries = [reaper.Entry("/fixture/ended-app-session", "app-session", 4096,
                                  reaper.DELETE, "already proved ended and unheld")]
        r.app_until = clock[0] + budget / 2
        r.app_apply_until = clock[0] + budget
        r.app_deferred = 0
        r.find_cargo_targets = lambda checkout, until=None: ["/p/target"]
        r.lock_free = lambda path: True
        r.in_use_under = lambda path: ""
        r.collect_test_instances = lambda *a: []
        r._record_failures = lambda *a: None

        def measure(path, until=None, *, strict=False):
            # A real bounded walk stops as soon as it is past `until`.
            if until is not None and clock[0] + scan_seconds > until:
                clock[0] = max(clock[0], until) + 0.01
                return None
            clock[0] += scan_seconds
            return 10 * 1024 ** 3, 1.0, False

        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(reaper.time, "time", lambda: clock[0]))
            stack.enter_context(patch.object(reaper, "measure", measure))
            stack.enter_context(patch.object(reaper, "write_log", lambda *a: None))
            stack.enter_context(patch.object(reaper.os.path, "lexists", return_value=True))
            stack.enter_context(patch.object(reaper.os.path, "isdir", return_value=True))
            stack.enter_context(patch.object(reaper.os.path, "islink", return_value=False))
            stack.enter_context(patch.object(reaper.os, "scandir", return_value=contextlib.nullcontext([])))
            stack.enter_context(patch.object(shutil, "rmtree"))
            r.scan_cargo_output(FakeWalls(), ["/p"])
            result = r.apply("/unused.log")
        return result[0], r

    def test_a_quick_scan_deletes_both(self):
        self.assertEqual(self.run_sweep(0, 8)[0], 2)

    def test_a_slow_scan_still_applies_the_ordinary_entry_and_defers_cargo(self):
        deleted, r = self.run_sweep(9, 8)
        self.assertEqual(deleted, 1)
        self.assertGreaterEqual(r.app_deferred, 1)
        self.assertEqual([e.path for e in r.entries if e.klass == "cargo-output"], [])

    def test_the_wide_budget_deletes_both(self):
        self.assertEqual(self.run_sweep(9, 540)[0], 2)


if __name__ == "__main__":
    unittest.main()
