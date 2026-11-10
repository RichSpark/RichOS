#!/usr/bin/env python3
"""The disk alert names folders on the disk in alert, found by walking it (2026-11-10).

The internal disk's alert listed a folder of the external drive (363 GB) first, and named
~/.richos-nightly only as one total. volume_consumers walks one volume and names the subfolder;
is_ours counts our own Cargo output as ours. Fixtures only; nothing real is read.
"""
import importlib.util
import contextlib
import io
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("disk_watchdog", HERE / "disk-watchdog.py")
wd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wd)


class Consumers(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="wd-volume-test."))
        self.root = os.path.join(self.tmp, "home")
        self.deep = os.path.join(self.root, ".richos-nightly", "source", "app", "target")
        os.makedirs(self.deep)
        Path(self.deep, "CACHEDIR.TAG").write_text("Signature: 8a477f597d28d172789f06886806bc55\n")
        os.makedirs(os.path.join(self.root, "small"))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def sizes(self, path):
        return 10 * 1024 * 1024 if path.endswith("target") or path.count(os.sep) < self.deep.count(os.sep) \
            and os.path.basename(path) in (".richos-nightly", "source", "app") else 1

    def test_the_subfolder_is_named_not_only_the_total(self):
        with patch.object(wd, "du_kb", side_effect=lambda p: self.sizes(p) // 1024):
            found = wd.volume_consumers(self.tmp, [self.root], 1024 * 1024)
        self.assertEqual([p for _b, p in found], [self.deep], found)

    def test_a_folder_found_below_another_is_counted_once(self):
        """10 GB in a nested target and 90 GB directly in its parent is two rows of 10 and 90, never 80."""
        parent = os.path.join(self.root, "big")
        nested = os.path.join(parent, "target")
        os.makedirs(nested)
        sizes = {parent: 100, nested: 10}
        with patch.object(wd, "du_kb", side_effect=lambda p: sizes.get(p, 0) * 1024 * 1024):
            found = wd.volume_consumers(self.tmp, [self.root], 1024 * 1024 * 1024 * 3 // 1024)
        self.assertEqual(sorted(b for b, _p in found), [10 * 1024 ** 3, 90 * 1024 ** 3], found)

    def test_another_device_is_never_listed(self):
        with patch.object(wd, "du_kb", return_value=10 * 1024), patch.object(wd, "on_volume", return_value=False):
            self.assertEqual(wd.volume_consumers(self.tmp, [self.root], 1024), [])

    def test_a_refused_path_is_not_walked(self):
        with patch.object(wd, "du_kb", return_value=None):
            self.assertEqual(wd.volume_consumers(self.tmp, [self.root], 1024), [])

    def test_our_cargo_output_counts_as_ours(self):
        self.assertTrue(wd.is_ours(self.deep))
        self.assertFalse(wd.is_ours(os.path.join(self.root, "small")))


class Notifications(unittest.TestCase):
    def test_each_volumes_notification_names_only_its_own_folders(self):
        primary, external = "/System/Volumes/Data", "/Volumes/Working"
        folders = {primary: "/Users/x/.richos-nightly/source/richos/app/target",
                   external: external + "/caches"}
        sent, saved = [], []
        env = {"MODE": "check", "HOME": "/Users/x", "DISK_PRIMARY_VOLUME": primary,
               "DISK_SCALE_EXTRA_VOLUMES": "0", "DISK_RECLAIM_ENABLE": "0"}
        patches = {
            "collect_devices_for_scheduled_check": lambda: None,
            "storage_candidates": lambda: ([], [external]),
            "read_volume": lambda mount: (460 * wd.GB, 20 * wd.GB),
            "read_state": lambda path: {},
            "cleanup_health": lambda *a: [],
            "volume_consumers": lambda mount, *a: [(10 * wd.GB, folders[mount])],
            "richos_garbage_bytes": lambda: 0,
            "is_ours": lambda path: True,
            "notify_ceo": lambda title, message: sent.append(message) or True,
            "write_state": lambda path, payload: saved.append(payload),
            "log": lambda *a: None,
        }
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.dict(os.environ, env, clear=True))
            for name, value in patches.items():
                stack.enter_context(patch.object(wd, name, value))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            self.assertEqual(wd.main(), 1)
        self.assertEqual(len(sent), 2, sent)
        internal = [m for m in sent if "/System/Volumes/Data" in m][0]
        outside = [m for m in sent if "/Volumes/Working" in m][0]
        self.assertIn("target", internal)
        self.assertNotIn("caches", internal)
        self.assertIn("caches", outside)
        self.assertNotIn("target", outside)


if __name__ == "__main__":
    unittest.main()
