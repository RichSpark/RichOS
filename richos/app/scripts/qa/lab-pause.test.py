#!/usr/bin/env python3
"""No lab needed: lab-pause.py continues only a process it stopped (hunt part 2 v3, R35).

The listener's identity changes while the tool waits for the stop mark and the wait times out: the
pid now belongs to something else and was never stopped, so nothing may be sent to it. The control:
a stop that is reached is continued exactly once. os.kill is a recorder; no process is signaled."""
import contextlib
import importlib.util
import io
import json
import signal
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("lab_pause", Path(__file__).with_name("lab-pause.py"))
lab_pause = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lab_pause)


class ContinuesOnlyWhatItStopped(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.lab = Path(self.tmp.name)
        self.data = self.lab / "manual-data"
        self.data.mkdir()
        (self.data / "lab-owner").write_text(lab_pause.OWNER)
        (self.lab / "mac.json").write_text(json.dumps({"data": str(self.data), "pid": 12345}))
        self.owned = True
        self.signals = []
        self.handlers = {s: signal.getsignal(s) for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}

    def tearDown(self):
        for sig, handler in self.handlers.items():
            signal.signal(sig, handler)
        self.tmp.cleanup()

    def ps(self, *_a, **_k):
        where = self.data if self.owned else self.lab / "foreign-lab"
        return SimpleNamespace(returncode=0, stdout=f"Tue Nov 3 10:00:00 2026 {lab_pause.LISTENER} RICHOS_MOBILE_MAC_DIR={where}")

    def main(self, wait_line):
        with patch.object(lab_pause.subprocess, "run", self.ps), patch.object(lab_pause, "wait_line", wait_line), \
                patch.object(lab_pause.os, "kill", lambda pid, sig: self.signals.append((pid, sig.name))), \
                contextlib.redirect_stdout(io.StringIO()):
            return lab_pause.main(["--lab", str(self.lab), "--log", str(self.lab / "steps.log"),
                                   "--stop-at", "STOP", "--cont-at", "CONT", "--timeout", "1"])

    def test_a_pid_never_stopped_is_never_continued(self):
        def wait_line(*_a):
            self.owned = False  # the listener went away and its pid was reused while we waited
            return None, 0
        code = self.main(wait_line)
        self.assertEqual(code, 1)
        self.assertEqual(self.signals, [], "SIGCONT went to a pid this tool never stopped")

    def test_a_stopped_listener_is_continued_exactly_once(self):
        marks = iter([("STOP", 5), ("CONT", 10)])
        code = self.main(lambda *_a: next(marks))
        self.assertEqual(code, 0)
        self.assertEqual(self.signals, [(12345, "SIGSTOP"), (12345, "SIGCONT")])

    def test_a_continue_mark_timeout_still_continues_the_stopped_listener(self):
        marks = iter([("STOP", 5), (None, 5)])
        code = self.main(lambda *_a: next(marks))
        self.assertEqual(code, 1)
        self.assertEqual(self.signals, [(12345, "SIGSTOP"), (12345, "SIGCONT")])


if __name__ == "__main__":
    result = unittest.main(exit=False, verbosity=1).result
    print("OK" if result.wasSuccessful() else "FAILED")
    raise SystemExit(0 if result.wasSuccessful() else 1)
