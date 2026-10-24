#!/usr/bin/env python3
"""The open-file table must be readable under the scheduled job's PATH.

The scheduled cleanup runs from launchd with the PATH its installer writes into
the plist (scratch-reaper.sh). On macOS that PATH has no /usr/sbin, where lsof
lives, so `lsof` was never found: every scheduled run reported "THE OPEN-FILE
TABLE COULD NOT BE READ" and decided nothing under $TMPDIR or /private/tmp,
while the same command run from a session (whose PATH has /usr/sbin) decided
everything.

This test runs the real whole-machine read with PATH set to exactly the
installer's PATH, and checks that a file this process holds open under $TMPDIR
shows up in the table. Nothing is deleted.
"""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("scratch_reaper", HERE / "scratch-reaper.py")
reaper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reaper)

INSTALLER = HERE.parent / "scratch-reaper.sh"


def launchd_path():
    """The PATH the installer writes into the scheduled job's plist."""
    m = re.search(r"<key>PATH</key><string>([^<]*)</string>", INSTALLER.read_text())
    if not m:
        raise AssertionError("no PATH in the plist that %s writes" % INSTALLER)
    return m.group(1)


def bare_reaper():
    r = object.__new__(reaper.Reaper)
    r.cfg = {"scratch_root_name": "richos-scratch", "shared_tmp_roots": []}
    r._open_tmp = None
    r._open_alloc = None
    r._open_shared = None
    return r


class OpenFileTableUnderLaunchdPath(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="reaper-launchd-path-test."))
        self.saved = {k: os.environ.get(k) for k in ("TMPDIR", "PATH")}
        os.environ["TMPDIR"] = self.tmp
        held = os.path.join(self.tmp, "held-child")
        os.makedirs(held)
        self.handle = open(os.path.join(held, "open.txt"), "w")

    def tearDown(self):
        self.handle.close()
        for k, v in self.saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_the_table_is_read_under_the_scheduled_jobs_path(self):
        path = launchd_path()
        if shutil.which("lsof", path=path) is not None:
            self.skipTest("lsof is on the scheduled job's PATH on this machine, "
                          "so the launchd condition does not reproduce here")
        if shutil.which("lsof", path="/usr/sbin:/sbin") is None:
            self.skipTest("no lsof in the system tool directories on this machine")
        os.environ["PATH"] = path
        table = bare_reaper().open_under_tmp()
        self.assertIsNotNone(table, "the open-file table could not be read under "
                             "the scheduled job's PATH %r" % path)
        self.assertIn("held-child", table)

    def test_control_a_table_that_cannot_be_read_still_decides_nothing(self):
        with patch.object(reaper.subprocess, "run", side_effect=FileNotFoundError("lsof")):
            r = bare_reaper()
            self.assertIsNone(r.open_under_tmp())
            self.assertIsNone(r.open_under_alloc())
            self.assertIsNone(r.open_under_shared("/private/tmp"))

    def test_the_callers_path_still_wins(self):
        os.environ["PATH"] = "/first:/usr/bin"
        self.assertTrue(reaper._ps_env()["PATH"].startswith("/first:/usr/bin:"))


if __name__ == "__main__":
    unittest.main()
