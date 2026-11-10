#!/usr/bin/env python3
"""The disk alert names folders on the disk in alert, found by walking it (2026-11-10).

The internal disk's alert listed /Volumes/E1TB (363 GB, on the external drive) first, and named
~/.richos-nightly only as one total. volume_consumers walks one volume and names the subfolder;
is_ours counts our own Cargo output as ours. Fixtures only; nothing real is read.
"""
import importlib.util
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


if __name__ == "__main__":
    unittest.main()
