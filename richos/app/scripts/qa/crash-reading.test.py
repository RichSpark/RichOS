#!/usr/bin/env python3
"""No phone needed: `rios device crashes` reads an .ips crash report into its exception, its
termination and the crashed thread's first frames.

The report is written here in the shape iOS 26 writes one (one JSON header line, then the JSON
body), hand-made: no phone's report is copied."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("phone_ios", Path(__file__).with_name("phone-ios.py"))
phone_ios = importlib.util.module_from_spec(spec)
spec.loader.exec_module(phone_ios)

HEADER = {"bug_type": "309", "timestamp": "2026-10-21 14:48:02.00 +0100", "app_name": "RichOSNative",
          "app_version": "1.0.0", "build_version": "1"}
BODY = {
    "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
    "termination": {"namespace": "SIGNAL", "indicator": "Abort trap: 6"},
    "usedImages": [{"name": "libsystem_kernel.dylib"}, {"name": "RichOSNative"}],
    "threads": [
        {"frames": [{"imageIndex": 1, "symbol": "idle()"}]},
        {"triggered": True, "frames": [{"imageIndex": 0, "symbol": "__pthread_kill"},
                                       {"imageIndex": 1, "symbol": "Coordinator.apply", "sourceFile": "T.swift",
                                        "sourceLine": 12},
                                       {"imageIndex": 9}]},
    ],
}


def report(body):
    tmp = tempfile.NamedTemporaryFile("w", suffix=".ips", delete=False)
    tmp.write(json.dumps(HEADER) + "\n" + (body if isinstance(body, str) else json.dumps(body)))
    tmp.close()
    return Path(tmp.name)


class CrashReading(unittest.TestCase):
    def test_the_crashed_thread_is_read_and_no_other(self):
        path = report(BODY)
        try:
            r = phone_ios.crash_summary(path)
        finally:
            path.unlink()
        self.assertEqual(r["bugType"], "309")
        self.assertEqual(r["app"], "RichOSNative")
        self.assertEqual(r["exception"], "EXC_CRASH SIGABRT")
        self.assertEqual(r["termination"], "SIGNAL Abort trap: 6")
        self.assertEqual(r["crashedThread"], ["libsystem_kernel.dylib __pthread_kill",
                                              "RichOSNative Coordinator.apply T.swift 12", "(no image or symbol)"])

    def test_an_unreadable_body_says_so_rather_than_guessing(self):
        path = report("not json")
        try:
            r = phone_ios.crash_summary(path)
        finally:
            path.unlink()
        self.assertEqual(r, {"bugType": "309", "time": HEADER["timestamp"], "readable": False})


if __name__ == "__main__":
    unittest.main()
