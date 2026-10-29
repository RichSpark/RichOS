#!/usr/bin/env python3
"""A second review's empty Claude session folder is the reaper's own to delete once its scratch is gone.

Follow-up 2026-10-28: each second review run by Claude leaves a 96 byte folder under the Claude
scratch root named after its export ("<scratch root>-<pid>-second-review-<id>-export"). The reaper
could not decide it: any folder written after the earliest running claude process started may belong
to a running session, which on a Mac that keeps claude running for days is every one of them. So it
reported each as undecided and raised an escalation after each run. State is a temporary directory;
nothing is deleted (the reaper only PLANS here; `add` records the decision).
"""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("scratch_reaper", HERE / "scratch-reaper.py")
reaper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reaper)


class NoWalls(object):
    def check(self, *_a, **_k):
        return None


class RefusingWalls(object):
    def check(self, *_a, **_k):
        return "wall 4: refused for the test"


class Live(object):
    """What the orphan rule asks: the start of the earliest running claude process."""

    def __init__(self, started):
        self.started = started
        self.running = {}

    def oldest_start(self):
        return self.started


class ReviewExport(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="reaper-export-test."))
        self.before = os.environ.get("TMPDIR")
        os.environ["TMPDIR"] = self.tmp
        self.allocator = os.path.join(self.tmp, "richos-scratch")
        self.claude = os.path.join(self.tmp, "claude-root")
        os.makedirs(self.allocator)
        os.makedirs(self.claude)
        self.now = time.time()

    def tearDown(self):
        if self.before is None:
            os.environ.pop("TMPDIR", None)
        else:
            os.environ["TMPDIR"] = self.before
        shutil.rmtree(self.tmp, ignore_errors=True)

    def slug_for(self, scratch_name, root=None):
        return re.sub(r"[^A-Za-z0-9]", "-", os.path.join(root or self.allocator, scratch_name, "export"))

    def folder(self, name, age_minutes=600, children=()):
        path = os.path.join(self.claude, name)
        os.makedirs(path)
        for child in children:
            Path(path, child).write_text("x")
        stamp = self.now - age_minutes * 60
        os.utime(path, (stamp, stamp))
        return path

    def plan(self, walls=None, running_since=None):
        """What scan_slug decides for each folder under the Claude root."""
        r = object.__new__(reaper.Reaper)
        r.cfg = {"scratch_root_name": "richos-scratch", "age_floor_minutes": 60, "shared_tmp_roots": []}
        r.now = self.now
        r.floor = 60 * 60
        # A claude process that has been running for days: every folder here is newer than its start.
        r.live = Live(running_since if running_since is not None else self.now - 5 * 86400)
        r.decisions = []
        r.add = lambda path, klass, size, action, why, **_k: r.decisions.append((os.path.basename(path), klass, action, why))
        for name in sorted(os.listdir(self.claude)):
            r.scan_slug(os.path.join(self.claude, name), walls or NoWalls())
        return {row[0]: row for row in r.decisions}

    def test_an_empty_export_folder_whose_scratch_is_gone_is_deleted(self):
        slug = self.slug_for("4242-second-review-ab12")
        self.folder(slug)
        row = self.plan()[slug]
        self.assertEqual((row[1], row[2]), ("claude-export", reaper.DELETE), row)
        self.assertIn("no longer exists", row[3])

    def test_the_same_folder_is_kept_while_its_scratch_exists(self):
        os.makedirs(os.path.join(self.allocator, "4242-second-review-ab12", "export"))
        slug = self.slug_for("4242-second-review-ab12")
        self.folder(slug)
        row = self.plan()[slug]
        self.assertEqual((row[1], row[2]), ("claude-export", reaper.KEEP), row)

    def test_a_young_folder_waits_for_the_age_floor(self):
        slug = self.slug_for("4242-second-review-ab12")
        self.folder(slug, age_minutes=5)
        row = self.plan()[slug]
        self.assertEqual((row[1], row[2]), ("claude-export", reaper.KEEP), row)

    def test_a_wall_makes_it_undecided_never_a_silent_skip(self):
        slug = self.slug_for("4242-second-review-ab12")
        self.folder(slug)
        row = self.plan(walls=RefusingWalls())[slug]
        self.assertEqual((row[1], row[2]), ("claude-export", reaper.INDETERMINATE), row)

    def test_folders_that_are_not_review_exports_are_not_taken(self):
        other_root = os.path.join(self.tmp, "elsewhere", "richos-scratch")
        cases = {
            "a project folder": "-Users-someone-ab-project",
            "an export of another scratch": self.slug_for("4242-nightly-ab12"),
            "a review export of a folder this reaper does not allocate in": self.slug_for("4242-second-review-ab12", other_root),
            "the allocator root alone": re.sub(r"[^A-Za-z0-9]", "-", self.allocator) + "-export",
        }
        for label, name in cases.items():
            self.folder(name)
        decided = self.plan()
        for label, name in cases.items():
            with self.subTest(label):
                self.assertNotEqual(decided[name][1], "claude-export", decided[name])
                self.assertNotEqual(decided[name][2], reaper.DELETE, decided[name])

    def test_a_folder_with_anything_in_it_is_left_to_the_general_rules(self):
        slug = self.slug_for("4242-second-review-ab12")
        self.folder(slug, children=("notes.txt",))
        row = self.plan()[slug]
        self.assertNotEqual(row[1], "claude-export", row)


if __name__ == "__main__":
    unittest.main()
