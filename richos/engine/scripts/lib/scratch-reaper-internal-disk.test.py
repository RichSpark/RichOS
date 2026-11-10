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

    def test_a_registered_workspace_inside_the_candidate_is_refused(self):
        r = object.__new__(reaper.Reaper)
        r.cfg = {"nightly_dir": self.nightly, "age_floor_minutes": 30}
        r.floor, r.now, r.decisions = 1800, time.time(), []
        r.add = lambda path, klass, size, action, why, **_k: r.decisions.append((path, klass, action, str(why)))
        walls = FakeWalls()
        walls.registered = {os.path.join(self.target, "debug")}
        with patch.object(reaper, "registered_workspaces", return_value=set()), \
                patch.object(reaper, "check_deadline", return_value=None):
            r.scan_cargo_output(walls)
        self.assertEqual([a for _p, _k, a, _w in r.decisions], [reaper.INDETERMINATE], r.decisions)


if __name__ == "__main__":
    unittest.main()
