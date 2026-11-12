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
  B9   THE FLOOR (2026-11-12): with every unit read within 72 h and the volume under its
       target, units go oldest first until the shortfall is covered; the warm unit stays.
  B9c  the same units on a volume at its target are all kept (control).
  B9m  a unit read within SCRATCH_BUILD_CACHE_MIN_IDLE_HOURS stays even when the volume is
       still short, and a profile a running build holds is neither counted nor touched.
  B9f  the floor is the disk watchdog's: scaled by capacity on a volume other than the primary.
  B9h  a unit whose binary Cargo also hard-linked outside it frees almost nothing; the volume
       is read again after the planned rows and the next oldest unit goes until it is on target
       (review rv-20261112T102721Z-317c4e7a-1985 #2), never one read within the minimum.
  B9a  the app's own sweep (--app, the only sweep a user's app runs) applies the floor to the
       shared Cargo trees too (the same review, #1); on a volume at its target it walks nothing.
  B9s  the reviewer's three cases (review rv-20261112T122930Z-beaaa3c2-fb0a,
       fixtures/floor_accounting.py), each on a volume one unit short: the app's sweep with the
       oldest unit at 50 h and at 100 h (past the usual 72 h), and the scheduled sweep with a
       hard-linked 100 h unit. Each ends on target, never by taking the unit read 1 h ago.
  B9e  the reviewer's two edges (review rv-20261112T133007Z-45bf4e10-69b6, fixtures/floor_edges.py):
       other cleanup in the same run already put the volume on target, so no floor unit goes;
       and 64 hard-linked units take more than 50 rounds, and the re-read goes on to the target.
  B9l  a build takes the oldest profile's lock after the plan (review
       rv-20261112T143113Z-f972fff8-cdd8, fixtures/floor_review.py): the re-read leaves that
       profile out and takes the next oldest unit elsewhere, and reaches the target.
  B9w  --floor-check, which the app's disk watcher asks every check, says exit 1 and names the
       drive when a Cargo tree's volume is below its target, exit 0 and nothing when it is not
       (review rv-20261112T112825Z-7f381427-61d2: the cache drive filled, the app's disk was fine).

Everything lives in a temporary directory; nothing outside it is read or written except
`git`, run against checkouts created here.
"""
import ast
import contextlib
import importlib.util
import io
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
                 "build_cache_orphan_hours": 6, "build_cache_idle_hours": 72,
                 # The floor (rule 3) is off unless a test sets it: the machine running this
                 # suite has whatever free space it has.
                 "build_cache_free_target_percent": 0, "build_cache_min_idle_hours": 2,
                 "disk_floor_gb": 1, "disk_primary_volume": "", "disk_scale_extra_volumes": True}
        r.cfg.update(getattr(self, "cfg_extra", {}))
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

    def test_selected_cargo_root_is_inside_the_real_sweep_boundary(self):
        for cargo in (self.cargo, os.path.join(self.tmp, "separate-cargo")):
            with self.subTest(cargo=cargo):
                orphan = os.path.join(cargo, "workspaces", "a" * 24)
                self.make([orphan], 2 * DAY)
                r = object.__new__(reaper.Reaper)
                r.cfg = {"claude_roots": [], "nightly_dir": self.tmp + "/nightly",
                         "shared_tmp_roots": [], "kept_dir": "",
                         "build_cache_root": self.cache, "cargo_target_roots": [cargo],
                         "checkout_parents": [self.parents], "age_floor_minutes": 60,
                         "build_cache_orphan_hours": 6, "build_cache_idle_hours": 72}
                r.entries, r.roots, r.now, r.floor = [], [], time.time(), 3600
                r.skip_build_caches = False
                for name in ("scan_claude_roots", "scan_scratch_root", "scan_tmp",
                             "scan_shared_tmp", "scan_nightly", "scan_kept",
                             "scan_campaign_roots", "scan_docker_containers",
                             "scan_deferred_unknown"):
                    setattr(r, name, lambda *_: None)
                walls = []
                r.scan_standing_failures = walls.append
                with patch.dict(os.environ, {"TMPDIR": self.tmp + "/process-tmp"}), \
                        patch.object(reaper, "registered_workspaces", return_value=[]), \
                        patch.object(reaper, "never_touch", return_value=[]):
                    entries = {e.path: e for e in r.scan()}
                self.assertEqual(entries[orphan].action, reaper.DELETE, entries[orphan].why)
                self.assertIn("wall 1:", walls[0].check(self.tmp + "/unselected/artifact", False))

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

    # -- rule 3: the volume's floor ---------------------------------------------------------
    def recent_profile(self, name, ages_h):
        """A profile whose units were all read within 72 h, one unit per age, oldest first."""
        p = os.path.join(self.cargo, name, "debug")
        hashes = []
        for i, age_h in enumerate(ages_h):
            h = "%016x" % (0xa0 + i)
            for rel in (os.path.join(".fingerprint", "dep%d-%s" % (i, h), "lib-dep"),
                        os.path.join("deps", "libdep%d-%s.rlib" % (i, h))):
                write(os.path.join(p, rel), b"x" * 65536, age=age_h * 3600)
            age_tree(os.path.join(p, ".fingerprint", "dep%d-%s" % (i, h)), age_h * 3600)
            hashes.append(h)
        write(os.path.join(p, ".cargo-lock"), b"")
        return p, hashes

    def under_target(self, short_bytes):
        """The volume is `short_bytes` below its target: the floor is 1 GiB, the target 100% of it."""
        gib = 1024 ** 3
        self.cfg_extra = {"build_cache_free_target_percent": 100, "disk_floor_gb": 1}
        mod = reaper.build_caches()
        # The volume gains exactly what the deletions really release (blocks, each inode once).
        start = self.blocks(self.cargo)
        return patch.object(mod, "volume_space", new=lambda _p: (
            1, 100 * gib, gib - short_bytes + (start - self.blocks(self.cargo))))

    def left(self, p):
        return sorted(n for n in os.listdir(os.path.join(p, "deps")))

    def test_B9_under_the_floor_recent_units_go_oldest_first_and_the_warm_one_stays(self):
        p, (h50, h30, h3) = self.recent_profile("shared-build-v1", (50, 30, 3))
        oldest = bc.units(p)[("unit", h50)]["bytes"]
        with self.under_target(oldest):
            plan, r = self.plan()
            self.assertEqual(plan[p].action, reaper.DELETE, plan[p].why)
            self.assertEqual(plan[p].unit_keys, [("unit", h50)])
            self.assertIn("under its target", plan[p].why)
            _d, freed, failures = self.apply(r)
        self.assertEqual(failures, [])
        self.assertEqual(freed, oldest)
        self.assertEqual(self.left(p), ["libdep1-%s.rlib" % h30, "libdep2-%s.rlib" % h3])

    def test_B9c_the_same_units_on_a_volume_at_its_target_are_kept(self):
        p, _h = self.recent_profile("shared-build-v1", (50, 30, 3))
        with self.under_target(0):
            plan, _r = self.plan()
        self.assertNotIn(p, plan)
        self.assertEqual(len(self.left(p)), 3)

    def test_B9m_the_minimum_idle_and_a_held_profile_are_never_taken(self):
        p, (h50, h30, h1) = self.recent_profile("shared-build-v1", (50, 30, 1))
        busy, _hb = self.recent_profile("codex/shared-build-v1", (60,))
        self.children.append(hold_lock(os.path.join(busy, ".cargo-lock")))
        with self.under_target(1024 ** 3):              # far more than everything here
            plan, r = self.plan()
            self.assertEqual(plan[p].unit_keys, [("unit", h50), ("unit", h30)])
            self.assertEqual(plan[busy].action, reaper.KEEP, plan[busy].why)
            self.apply(r)
        self.assertEqual(self.left(p), ["libdep2-%s.rlib" % h1])
        self.assertEqual(len(self.left(busy)), 1)

    def test_B9f_the_floor_is_scaled_like_the_disk_watchdog(self):
        gib = 1024 ** 3
        space = {"/primary": (1, 460 * gib, 100 * gib), "/external/cargo": (2, 954 * gib, 20 * gib)}
        with patch.object(bc, "volume_space", lambda path: space[path]):
            target, floor, free = bc.free_target("/external/cargo", 60, "/primary", True, 150)
            self.assertEqual(floor, int(60 * gib * 954 / 460.0))      # about 124 GiB
            self.assertEqual(target, int(floor * 1.5))
            self.assertEqual(free, 20 * gib)
            self.assertEqual(bc.free_target("/external/cargo", 60, "/primary", False, 100)[1], 60 * gib)
            self.assertEqual(bc.free_target("/primary", 60, "/primary", True, 100)[1], 60 * gib)

    def blocks(self, root):
        """Bytes the files under root hold, each inode once: what deleting them really frees."""
        seen = {}
        for d, _dirs, files in os.walk(root):
            for n in files:
                st = os.lstat(os.path.join(d, n))
                seen[(st.st_dev, st.st_ino)] = st.st_blocks * 512
        return sum(seen.values())

    def test_B9h_the_volume_is_read_again_and_a_hard_linked_unit_does_not_end_the_reclaim(self):
        p, (h50, h30, h1) = self.recent_profile("shared-build-v1", (50, 30, 1))
        # Cargo's top-level copy of the oldest unit's library: one inode, two names.
        os.link(os.path.join(p, "deps", "libdep0-%s.rlib" % h50), os.path.join(p, "libdep0.rlib"))
        oldest = bc.units(p)[("unit", h50)]["bytes"]
        start = self.blocks(self.cargo)
        gib = 1024 ** 3
        self.cfg_extra = {"build_cache_free_target_percent": 100, "disk_floor_gb": 1}
        # The volume's free space is what the deletions really released, nothing more.
        space = lambda _p: (1, 100 * gib, gib - oldest + (start - self.blocks(self.cargo)))
        with patch.object(reaper.build_caches(), "volume_space", space):
            plan, r = self.plan()
            self.assertEqual(plan[p].unit_keys, [("unit", h50)])
            log = os.path.join(self.tmp, "log")
            _d, _freed, failures = self.apply(r)
        self.assertEqual(failures, [])
        self.assertEqual(self.left(p), ["libdep2-%s.rlib" % h1])     # 30 h went too, 1 h stayed
        self.assertTrue(os.path.exists(os.path.join(p, "libdep0.rlib")))
        self.assertIn("FLOOR volume of", Path(log).read_text())

    def test_B9a_the_apps_own_sweep_applies_the_floor_to_the_shared_cargo_trees(self):
        p, (h50, h30, h3) = self.recent_profile("shared-build-v1", (50, 30, 3))
        oldest = bc.units(p)[("unit", h50)]["bytes"]
        data = os.path.join(self.tmp, "app-data")
        os.makedirs(data)
        for short, expect in ((oldest, [("unit", h50)]), (0, None)):
            with self.subTest(short=short), self.under_target(short):
                r = object.__new__(reaper.Reaper)
                r.cfg = {"claude_roots": [], "build_cache_root": self.cache,
                         "cargo_target_roots": [self.cargo], "build_cache_idle_hours": 72,
                         "build_cache_min_idle_hours": 2, "disk_primary_volume": "",
                         "disk_scale_extra_volumes": True}
                r.cfg.update(self.cfg_extra)
                r.entries, r.roots, r.now, r.floor = [], [], time.time(), 3600
                r.ws = Mock()
                r.app_sessions = Mock(return_value=set())
                r.app_account_session_dirs = Mock(return_value=[])
                r.app_repositories = Mock(return_value=[])
                for name in ("scan_app_profiles", "scan_app_voice", "scan_app_dictation",
                             "scan_app_mcp_logs", "scan_cargo_output"):
                    setattr(r, name, Mock())
                mod = reaper.build_caches()
                walked = Mock(side_effect=mod.profile_dirs)
                with patch.object(reaper, "Liveness"), \
                        patch.object(mod, "profile_dirs", walked), \
                        patch.object(reaper, "registered_workspaces", return_value=[]), \
                        patch.object(reaper, "never_touch", return_value=[]):
                    plan = {e.path: e for e in r.scan_app({"data": data})}
                if expect is None:
                    self.assertNotIn(p, plan)
                    self.assertEqual(walked.call_count, 0)
                else:
                    self.assertEqual(plan[p].action, reaper.DELETE, plan[p].why)
                    self.assertEqual(plan[p].unit_keys, expect)

    def app_sweep(self):
        """The app's own sweep (scan_app) with its other arms answering nothing, as in B9a."""
        data = os.path.join(self.tmp, "app-data")
        os.makedirs(data, exist_ok=True)
        r = object.__new__(reaper.Reaper)
        r.cfg = {"claude_roots": [], "build_cache_root": self.cache,
                 "cargo_target_roots": [self.cargo], "build_cache_idle_hours": 72,
                 "build_cache_min_idle_hours": 2, "disk_primary_volume": "",
                 "disk_scale_extra_volumes": True}
        r.cfg.update(self.cfg_extra)
        r.entries, r.roots, r.now, r.floor = [], [], time.time(), 3600
        r.ws = Mock()
        r.app_sessions = Mock(return_value=set())
        r.app_account_session_dirs = Mock(return_value=[])
        r.app_repositories = Mock(return_value=[])
        for name in ("scan_app_profiles", "scan_app_voice", "scan_app_dictation",
                     "scan_app_mcp_logs", "scan_cargo_output"):
            setattr(r, name, Mock())
        with patch.object(reaper, "Liveness"), \
                patch.object(reaper, "registered_workspaces", return_value=[]), \
                patch.object(reaper, "never_touch", return_value=[]):
            r.scan_app({"data": data})
        return r

    def test_B9s_the_reviewers_three_cases_each_end_on_target(self):
        cases = (("the app's sweep, oldest unit 50 h", 50, True, False),
                 ("the app's sweep, oldest unit 100 h", 100, True, False),
                 ("the scheduled sweep, hard-linked oldest unit 100 h", 100, False, True))
        for label, oldest_h, app, hardlink in cases:
            with self.subTest(label):
                shutil.rmtree(self.cargo, ignore_errors=True)
                os.makedirs(self.cargo)
                p, (old, _younger, warm) = self.recent_profile("shared-build-v1", (oldest_h, 30, 1))
                if hardlink:
                    os.link(os.path.join(p, "deps", "libdep0-%s.rlib" % old),
                            os.path.join(p, "libdep0.rlib"))
                short = bc.units(p)[("unit", old)]["bytes"]
                with self.under_target(short) as space:
                    r = self.app_sweep() if app else self.plan()[1]
                    with contextlib.redirect_stdout(io.StringIO()):
                        _d, _f, failures = self.apply(r)
                    left = space(p)[2] - 1024 ** 3        # free minus target: >= 0 is on target
                self.assertEqual(failures, [])
                self.assertGreaterEqual(left, 0, "still %d bytes short of the target" % -left)
                self.assertNotIn(("unit", old), bc.units(p))
                self.assertIn(("unit", warm), bc.units(p))

    def test_B9e_other_cleanup_on_the_volume_is_counted_before_any_floor_unit_goes(self):
        p, (h3, h25, h1) = self.recent_profile("shared-build-v1", (3, 2.5, 1))
        # On the same simulated volume (under_target counts the blocks under the Cargo root).
        output = os.path.join(self.cargo, "connected-project", "target")
        write(os.path.join(output, "unused"), b"x" * 1048576)
        short = sum(u["bytes"] for k, u in bc.units(p).items() if k != ("unit", h1))
        with self.under_target(short):
            plan, r = self.plan()
            self.assertEqual(plan[p].unit_keys, [("unit", h3), ("unit", h25)])
            r.entries.append(reaper.Entry(output, "cargo-output", self.blocks(output), reaper.DELETE,
                                          "unused Cargo output in a connected checkout"))
            with contextlib.redirect_stdout(io.StringIO()):
                _d, _f, failures = self.apply(r)
        self.assertEqual(failures, [])
        self.assertFalse(os.path.exists(output))
        self.assertEqual(len(self.left(p)), 3, "the output's 1 MB covered the shortfall")

    def test_B9e_the_re_read_goes_past_fifty_rounds_to_the_target(self):
        # The reviewer's shape: 64 units whose library Cargo also linked at the top level (deleting
        # one frees only its tiny fingerprint), one releasable 30 h unit, one warm 1 h unit.
        p = os.path.join(self.cargo, "shared-build-v1", "debug")
        keys = []
        for i, age_h in enumerate([70 - i / 10.0 for i in range(64)] + [30, 1]):
            h = "%016x" % (0x100 + i)
            fp = os.path.join(p, ".fingerprint", "dep%d-%s" % (i, h), "lib-dep")
            lib = os.path.join(p, "deps", "libdep%d-%s.rlib" % (i, h))
            write(fp, b"f", age=age_h * 3600)
            write(lib, b"x" * 262144, age=age_h * 3600)
            age_tree(os.path.dirname(fp), age_h * 3600)
            if i < 64:
                os.link(lib, os.path.join(p, "libdep%d.rlib" % i))
            keys.append(("unit", h))
        write(os.path.join(p, ".cargo-lock"), b"")
        with self.under_target(262144):
            results, _before, after = reaper.build_caches().reclaim_to_target([p], 1024 ** 3, 2 * 3600)
        self.assertGreaterEqual(after, 1024 ** 3, "still short after the re-read stopped")
        self.assertGreater(sum(n for _p, n, _b, _f in results), 50, "it took more than 50 rounds")
        self.assertIn(keys[65], bc.units(p))

    def test_B9l_a_build_that_takes_a_lock_after_the_plan_does_not_end_the_reclaim(self):
        oldest, (h50,) = self.recent_profile("a-shared", (50,))
        available, (h30, h1) = self.recent_profile("b-codex", (30, 1))
        short = bc.units(oldest)[("unit", h50)]["bytes"]
        with self.under_target(short) as space:
            _plan, r = self.plan()
            self.children.append(hold_lock(os.path.join(oldest, ".cargo-lock")))
            with contextlib.redirect_stdout(io.StringIO()):
                _d, _f, failures = self.apply(r)
            left = space(available)[2] - 1024 ** 3
        self.assertEqual(failures, [])
        self.assertGreaterEqual(left, 0, "still %d bytes short of the target" % -left)
        self.assertIn(("unit", h50), bc.units(oldest), "the held profile was touched")
        self.assertNotIn(("unit", h30), bc.units(available))
        self.assertIn(("unit", h1), bc.units(available))

    def test_B9w_the_floor_check_names_a_short_cache_drive_and_scans_nothing(self):
        cfg = {"cargo_target_roots": [self.cargo], "disk_primary_volume": "",
               "disk_scale_extra_volumes": True, "disk_floor_gb": 1,
               "build_cache_free_target_percent": 100}
        gib = 1024 ** 3
        mod = reaper.build_caches()
        for free, code in ((gib - 1, 1), (gib, 0)):
            with self.subTest(free=free), \
                    patch.object(mod, "volume_space", lambda _p, f=free: (1, 100 * gib, f)), \
                    patch.object(mod, "profile_dirs") as walked, \
                    contextlib.redirect_stdout(io.StringIO()) as out:
                self.assertEqual(reaper.floor_check(cfg), code)
                walked.assert_not_called()
            self.assertEqual(self.cargo in out.getvalue(), code == 1, out.getvalue())

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

    def test_unmounted_cargo_settings_refuse_before_any_cleanup(self):
        missing = Path('/Volumes') / ('unmounted-' + Path(self.tmp).name)
        self.assertFalse(missing.exists())
        private = Path(self.tmp) / 'machine.env'
        for key in ('CARGO_TARGET_DIR', 'RICHOS_CARGO_CACHE_ROOT'):
            with self.subTest(key=key):
                private.write_text(f'RICHOS_BULK_ROOT={self.tmp}\n{key}={missing}/cargo\n')
                error = io.StringIO()
                with patch.dict(os.environ, {'HOME': self.tmp, 'TMPDIR': self.tmp,
                                             'RICHOS_ENV_FILE': str(private)}, clear=True), \
                        patch.object(reaper, 'load_workspaces') as load, \
                        patch.object(reaper, 'Reaper') as sweep, \
                        contextlib.redirect_stderr(error):
                    self.assertEqual(reaper.main([]), 2)
                    load.assert_not_called()
                    sweep.assert_not_called()
                self.assertIn('storage volume is not mounted', error.getvalue())
                self.assertNotIn('Traceback', error.getvalue())
                self.assertFalse(missing.exists())

    def test_custom_build_cache_is_selected_by_lander_and_scheduled_cleanup(self):
        custom = str(Path(self.tmp) / 'custom-builds')
        private = Path(self.tmp) / 'machine.env'
        private.write_text(f'RICHOS_BULK_ROOT={self.tmp}\nRICHOS_BUILD_CACHE={custom}\n')
        original = self.cargo
        self.cargo = custom
        dead = self.folders_for(str(Path(self.parents) / 'gone'))[0]
        alive = self.folders_for(self.live)[0]
        unselected = str(Path(self.tmp) / 'unselected' / 'workspaces' / ('a' * 24))
        self.make([dead, alive, unselected], 2 * DAY)
        env = {'HOME': self.tmp, 'TMPDIR': self.tmp, 'RICHOS_ENV_FILE': str(private),
               'SCRATCH_CLAUDE_ROOTS': str(Path(self.tmp) / 'sessions'),
               'SCRATCH_TMP_PATTERNS': 'fixture-*', 'SCRATCH_AGE_FLOOR_MINUTES': '60',
               'SCRATCH_NIGHTLY_DIR': str(Path(self.tmp) / 'nightly'),
               'SCRATCH_NIGHTLY_KEEP': '1', 'SCRATCH_NOTICE_BYTES': '1',
               'SCRATCH_SESSION_PROCESS_NAMES': 'fixture-agent'}
        with patch.dict(os.environ, env, clear=True):
            self.assertEqual(bc.roots(), (self.cache, [original, custom]))
            self.assertEqual(bc.checkout_caches(self.live), [alive])
            cfg = reaper.config_from_env()
            self.assertEqual(cfg['cargo_target_roots'], [original, custom])
            cfg['checkout_parents'] = [self.parents]
            walls = reaper.Walls([cfg['build_cache_root'], *cfg['cargo_target_roots']])
            self.assertIn('wall 1:', walls.check(unselected, False))
            r = object.__new__(reaper.Reaper)
            r.cfg = cfg
            r.entries, r.roots, r.now, r.floor = [], [], time.time(), 3600
            r.skip_build_caches = False
            r.scan_build_caches(walls)
        plan = {e.path: e for e in r.entries}
        self.assertEqual(plan[dead].action, reaper.DELETE, plan[dead].why)
        self.assertEqual(plan[alive].action, reaper.KEEP)
        self.assertNotIn(unselected, plan)
        _, _, failures = self.apply(r)
        self.assertEqual(failures, [])
        self.assertFalse(Path(dead).exists())
        self.assertTrue(Path(alive).exists())
        self.assertTrue(Path(unselected).exists())

    def test_overlapping_build_roots_do_not_double_count(self):
        private = Path(self.tmp) / 'machine.env'
        private.write_text(f'RICHOS_BULK_ROOT={self.tmp}\nRICHOS_BUILD_CACHE={self.cargo}\n')
        with patch.dict(os.environ, {'HOME': self.tmp, 'RICHOS_ENV_FILE': str(private)}, clear=True):
            self.assertEqual(bc.roots(), (self.cache, [self.cargo]))
        nested = str(Path(self.cargo) / 'custom')
        workspaces = Path(nested) / 'workspaces'
        profile = workspaces / ('a' * 24) / 'debug'
        (profile / '.fingerprint').mkdir(parents=True)
        self.assertEqual(bc.cargo_workspace_dirs([self.cargo, nested]), [str(workspaces)])
        self.assertEqual(bc.profile_dirs([self.cargo, nested]), [str(profile)])

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
