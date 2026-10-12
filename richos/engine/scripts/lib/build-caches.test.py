#!/usr/bin/env python3
"""The build caches on the external drive clean themselves up (2026-10-08).

CEO: "That space needs to be always freed up automatically without me needing to ever worry
about that again." The drive was 98% full, 824 GB of it build caches nothing ever removed.

Each rule is proven with its control beside it, because a deleter that deletes nothing passes
every "it kept X" test:

  B1   a cache folder keyed to a checkout that is gone is deleted by the scheduled sweep;
  B1c  the same folder keyed to a checkout that exists is kept (control).
  B2   an orphan something wrote or read within the declared idle time is kept.
  B3   an orphan whose Cargo lock a running build holds is kept.
  B4   a Cargo unit no build has read for the declared time is deleted, a unit read
       recently in the same directory is kept (control), and the directory itself stays.
  B5   while a running build holds the profile's Cargo lock, no unit is deleted.
  B6   what the lander moved aside and its own deletion left is deleted after the floor.
  B7   the lander's half: a checkout's own folders are found from its path, a neighbor's
       are not, and discard() moves them out of place at once.
  B8   with a repository whose worktree list cannot be read, no orphan is deleted.

Everything lives in a temporary directory; nothing outside it is read or written except
`git`, run against checkouts created here.
"""
import ast
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

HERE = Path(__file__).resolve().parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


bc = load("build_caches_under_test", "build_caches.py")
reaper = load("scratch_reaper_under_test", "scratch-reaper.py")

DAY = 86400


class NoWalls(object):
    registered = set()

    def check(self, *_a, **_k):
        return ""


def write(path, data=b"x" * 4096, age=0.0):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    if age:
        t = time.time() - age
        os.utime(path, (t, t))


def age_tree(path, age):
    t = time.time() - age
    for d, dirs, files in os.walk(path):
        for n in files + dirs:
            os.utime(os.path.join(d, n), (t, t), follow_symlinks=False)
    os.utime(path, (t, t))


def hold_lock(path):
    """A child process holding flock(2) on path, the way Cargo holds .cargo-lock."""
    code = ("import fcntl,os,sys,time\n"
            "fd=os.open(sys.argv[1],os.O_RDONLY)\n"
            "fcntl.flock(fd,fcntl.LOCK_EX)\n"
            "print('held',flush=True)\n"
            "time.sleep(60)\n")
    p = subprocess.Popen([sys.executable, "-c", code, path], stdout=subprocess.PIPE, text=True)
    assert p.stdout.readline().strip() == "held"
    return p


class BuildCaches(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="build-caches-test."))
        self.parents = os.path.join(self.tmp, "ab")
        self.cache = os.path.join(self.tmp, "caches")
        self.cargo = os.path.join(self.cache, "cargo-target")
        os.makedirs(self.parents)
        os.makedirs(self.cargo)
        self.live = self.checkout("live")
        self.children = []

    def tearDown(self):
        for p in self.children:
            p.kill()
            p.wait()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def checkout(self, name):
        root = os.path.join(self.parents, name)
        os.makedirs(os.path.join(root, "richos", "app"))
        with open(os.path.join(root, "richos", "app", "Cargo.toml"), "w") as f:
            f.write("[workspace]\n")
        subprocess.run(["git", "init", "-q", root], check=True)
        subprocess.run(["git", "-C", root, "add", "-A"], check=True)
        return root

    # -- the folders each tool makes for a checkout ----------------------------------------
    def folders_for(self, checkout):
        import hashlib

        def h(alg, p, n):
            return getattr(hashlib, alg)(p.encode()).hexdigest()[:n]
        ios = os.path.join(checkout, "richos", "mobile", "native-ios")
        android = os.path.join(checkout, "richos", "mobile", "native-android")
        app = os.path.join(checkout, "richos", "app")
        return [
            os.path.join(self.cargo, "workspaces", h("sha256", app, 24)),
            os.path.join(self.cargo, "codex", "workspaces", h("sha256", app, 24)),
            os.path.join(self.cache, "richos-native-ios", h("sha256", ios, 10)),
            os.path.join(self.cache, "richos-native-ios", "lab-phone-" + h("sha1", checkout, 10)),
            os.path.join(self.cache, "richos-native-ios-proof", h("sha256", checkout, 10) + "-app"),
            os.path.join(self.cache, "richos-native-ios-ui", h("sha256", ios, 10)),
            os.path.join(self.cache, "richos-native-android", h("sha1", android, 10)),
            os.path.join(self.cache, "richos-native-android", "ui-shots", h("sha1", android, 10)),
        ]

    def make(self, folders, age):
        for f in folders:
            write(os.path.join(f, "out", "artifact.bin"))
            age_tree(f, age)

    def plan(self):
        r = object.__new__(reaper.Reaper)
        r.cfg = {"build_cache_root": self.cache, "cargo_target_roots": [self.cargo],
                 "checkout_parents": [self.parents], "age_floor_minutes": 60,
                 "build_cache_orphan_hours": 6, "build_cache_idle_hours": 72}
        r.entries, r.roots, r.now, r.floor = [], [], time.time(), 3600
        r.skip_build_caches = False
        r.scan_build_caches(NoWalls())
        return {e.path: e for e in r.entries}, r

    def apply(self, r):
        """The reaper's own apply(), with the arms this suite does not exercise (test
        devices, app instances, Docker, the failure ledger) answering nothing."""
        r.scope = None
        r.collect_test_instances = lambda paths, stamp: []
        r.collect_test_devices = lambda stamp: []
        r.prune_docker = lambda: []
        r._record_failures = lambda failures, stamp: None
        r.counts = lambda: (0, 0, 0, 0)
        return r.apply(os.path.join(self.tmp, "log"))

    def test_B1_an_orphan_of_a_deleted_checkout_is_deleted_and_B1c_a_live_one_kept(self):
        gone = os.path.join(self.parents, "landed-and-deleted")
        dead = self.folders_for(gone)
        alive = self.folders_for(self.live)
        self.make(dead + alive, 2 * DAY)
        plan, _r = self.plan()
        for f in dead:
            self.assertEqual(plan[f].action, reaper.DELETE, (f, plan[f].why))
            self.assertEqual(plan[f].klass, "build-cache")
        for f in alive:
            self.assertEqual(plan[f].action, reaper.KEEP, (f, plan[f].why))

    def test_B1_the_sweep_really_deletes_it(self):
        dead = self.folders_for(os.path.join(self.parents, "gone"))
        self.make(dead, 2 * DAY)
        _plan, r = self.plan()
        _deleted, freed, failures = self.apply(r)
        self.assertEqual(failures, [])
        self.assertGreater(freed, 0)
        for f in dead:
            self.assertFalse(os.path.exists(f), f)
        self.assertTrue(os.path.isdir(self.cargo))

    def test_B2_an_orphan_used_recently_is_kept(self):
        dead = self.folders_for(os.path.join(self.parents, "copy-outside-git"))
        self.make(dead, 2 * DAY)
        write(os.path.join(dead[0], "debug", "just-built"))
        plan, _r = self.plan()
        self.assertEqual(plan[dead[0]].action, reaper.KEEP, plan[dead[0]].why)
        self.assertEqual(plan[dead[2]].action, reaper.DELETE)

    def test_B3_an_orphan_a_running_build_holds_is_kept(self):
        dead = self.folders_for(os.path.join(self.parents, "gone"))[0]
        lock = os.path.join(dead, "debug", ".cargo-lock")
        write(lock, b"")
        os.makedirs(os.path.join(dead, "debug", ".fingerprint"))
        age_tree(dead, 2 * DAY)
        self.children.append(hold_lock(lock))
        plan, _r = self.plan()
        self.assertEqual(plan[dead].action, reaper.KEEP, plan[dead].why)
        self.assertIn("running Cargo build", plan[dead].why)

    def profile(self):
        p = os.path.join(self.cargo, "shared-build-v1", "debug")
        old, new = "0123456789abcdef", "fedcba9876543210"
        for h, age in ((old, 5 * DAY), (new, 0)):
            write(os.path.join(p, ".fingerprint", "serde-" + h, "lib-serde"), age=age)
            write(os.path.join(p, "deps", "libserde-%s.rlib" % h), age=age)
            write(os.path.join(p, "deps", "serde-%s.d" % h), age=age)
            write(os.path.join(p, "build", "serde-" + h, "out", "x"), age=age)
        for sub in (".fingerprint", "build"):
            age_tree(os.path.join(p, sub, "serde-" + old), 5 * DAY)
        write(os.path.join(p, ".cargo-lock"), b"")
        write(os.path.join(p, "richos-app"), age=5 * DAY)       # an uplifted binary: never a unit
        return p, old, new

    def test_B4_an_idle_cargo_unit_is_deleted_and_a_used_one_kept(self):
        p, old, new = self.profile()
        plan, r = self.plan()
        self.assertEqual(plan[p].action, reaper.DELETE, plan[p].why)
        self.assertEqual(plan[p].unit_keys, [("unit", old)])
        _d, freed, failures = self.apply(r)
        self.assertEqual(failures, [])
        self.assertGreater(freed, 0)
        left = sorted(os.listdir(os.path.join(p, "deps")))
        self.assertEqual(left, ["libserde-%s.rlib" % new, "serde-%s.d" % new])
        self.assertFalse(os.path.exists(os.path.join(p, ".fingerprint", "serde-" + old)))
        self.assertTrue(os.path.exists(os.path.join(p, ".fingerprint", "serde-" + new)))
        self.assertTrue(os.path.exists(os.path.join(p, "richos-app")))

    def test_B5_no_unit_is_deleted_while_a_build_holds_the_lock(self):
        p, old, _new = self.profile()
        self.children.append(hold_lock(os.path.join(p, ".cargo-lock")))
        n, freed, failures, why_not = bc.delete_units(p, [("unit", old)], 72 * 3600)
        self.assertEqual((n, freed, failures), (0, 0, []))
        self.assertIn("running Cargo build", why_not)
        self.assertTrue(os.path.exists(os.path.join(p, "deps", "libserde-%s.rlib" % old)))
        plan, _r = self.plan()
        self.assertEqual(plan[p].action, reaper.KEEP, plan[p].why)

    def test_B6_what_the_lander_moved_aside_is_deleted_after_the_floor(self):
        trash = bc.trash_dir(self.cache)
        stale = os.path.join(trash, time.strftime("%Y%m%dT%H%M%SZ", time.gmtime(time.time() - 7200))
                             + "-1-0-abc")
        fresh = os.path.join(trash, time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-1-0-def")
        write(os.path.join(stale, "f"))
        write(os.path.join(fresh, "f"))
        plan, _r = self.plan()
        self.assertEqual(plan[stale].action, reaper.DELETE)
        self.assertEqual(plan[fresh].action, reaper.KEEP)

    def test_B7_the_lander_finds_its_own_folders_and_moves_them_aside(self):
        leaving = self.checkout("leaving")
        mine = self.folders_for(leaving)
        theirs = self.folders_for(self.live)
        self.make(mine + theirs, 0)
        found = sorted(p for _f, p in bc.owned_paths(leaving, self.cache, [self.cargo]))
        self.assertEqual(found, sorted(mine))
        moved, failures = bc.discard(found, self.cache)
        self.assertEqual(failures, [])
        self.assertEqual(len(moved), len(mine))
        for f in mine:
            self.assertFalse(os.path.exists(f), f)
        for f in theirs:
            self.assertTrue(os.path.exists(f), f)

    def test_B8_an_unreadable_worktree_list_deletes_no_orphan(self):
        broken = os.path.join(self.parents, "broken")
        os.makedirs(os.path.join(broken, ".git"))           # a .git that git cannot read
        dead = self.folders_for(os.path.join(self.parents, "gone"))
        self.make(dead, 2 * DAY)
        plan, _r = self.plan()
        for f in dead:
            self.assertEqual(plan[f].action, reaper.INDETERMINATE, (f, plan[f].why))

    def test_installed_engine_has_no_git_but_registered_owners_remain_checked(self):
        library = Path(self.tmp) / 'installed-engine/scripts/lib'
        library.mkdir(parents=True)
        with patch.object(bc, '__file__', str(library / 'build_caches.py')):
            found, unreadable = bc.live_checkouts([], extra=[self.live])
            self.assertIn(self.live, found)
            self.assertEqual(unreadable, [])
            broken = Path(self.parents) / 'broken-owner'
            (broken / '.git').mkdir(parents=True)
            _, unreadable = bc.live_checkouts([], extra=[str(broken)])
            self.assertTrue(unreadable)

    def test_explicit_cargo_is_found_and_reaped_without_a_general_cache(self):
        checkout = self.checkout('leaving')
        cargo = self.folders_for(checkout)[0]
        self.make([cargo], 2 * DAY)
        missing = str(Path(self.tmp) / 'missing-general-cache')
        with patch.object(bc, 'roots', return_value=(missing, [self.cargo])):
            self.assertEqual(bc.checkout_caches(checkout), [cargo])
        self.cache = missing
        shutil.rmtree(checkout)
        plan, _ = self.plan()
        self.assertEqual(plan[cargo].action, reaper.DELETE)

    def test_lander_calls_the_current_discard_api(self):
        source = HERE.parents[1] / 'mega-lander/workspaces.py'
        node = next(n for n in ast.parse(source.read_text()).body
                    if isinstance(n, ast.FunctionDef) and n.name == 'discard_build_caches')
        event = Mock()
        namespace = {'_build_caches': lambda: bc, 'event': event}
        exec(compile(ast.Module(body=[node], type_ignores=[]), str(source), 'exec'), namespace)
        paths = [str(Path(self.cargo) / 'owned-cache')]
        with patch.object(bc, 'roots', return_value=(self.cache, [self.cargo])), \
                patch.object(bc, 'discard', return_value=(paths, [])) as discard:
            namespace['discard_build_caches']('worker', self.live, paths)
            discard.assert_called_once_with(paths, self.cache)
        self.assertIsNone(event.call_args.kwargs['failures'])


if __name__ == "__main__":
    unittest.main()
