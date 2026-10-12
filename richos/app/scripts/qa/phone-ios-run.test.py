#!/usr/bin/env python3
"""No phone needed: phone-ios.py's run verdicts, with every tool boundary (rios, xcresulttool, devicectl)
replaced by a stand-in. Nothing here reaches a device.

N07 (hunt part 2 v3): a run whose list keeps a shot is not passed when the shots cannot be exported."""
import contextlib
import importlib.util
import io
import json
import os
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("phone_ios", Path(__file__).with_name("phone-ios.py"))
phone_ios = importlib.util.module_from_spec(spec)
spec.loader.exec_module(phone_ios)

# The tool refuses an --out anywhere else, so its fixtures live there too.
SCRATCH = tempfile.gettempdir()
ENV = {"RICHOS_IOS_DEVICE": "fixture-phone", "RICHOS_APPLE_TEAM": "fixture-team"}


class Fixture(unittest.TestCase):
    def setUp(self):
        self.assertTrue(os.path.isdir(SCRATCH), f"{SCRATCH} is not a scratch directory")
        self.tmp = tempfile.TemporaryDirectory(dir=SCRATCH)
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def run_list(self, steps, export_rc, **extra):
        """_run over STEPS with a runner that logs every step ok and an export that exits EXPORT_RC."""
        physical = self.root / "physical"
        bundle = physical / "script-1.xcresult"
        bundle.mkdir(parents=True, exist_ok=True)
        (physical / "verify-script-1-test.log").write_text("".join(
            "PHONE_STEP " + json.dumps({"i": i, "do": s["do"], "ok": True, "start": time.time()}) + "\n"
            for i, s in enumerate(steps)))
        listing = self.root / "steps.json"
        listing.write_text(json.dumps(steps))

        def run(argv, **_kw):
            if argv[0] == "xcrun":
                return SimpleNamespace(returncode=export_rc, stdout="", stderr="fixture: attachment export failed")
            return SimpleNamespace(returncode=0, stdout=str(bundle), stderr="")
        args = SimpleNamespace(steps=str(listing), out=str(self.root / "out"), allowance=240, prebuilt=False,
                               stamp=None, approval_announced=False, **extra)
        with patch.dict(os.environ, ENV), patch.object(phone_ios, "forecast", return_value={"approvalExpected": False}), \
                patch.object(phone_ios, "passcode_state", return_value=False), \
                patch.object(phone_ios, "record_session", return_value=None), \
                patch.object(phone_ios, "session_from_log", return_value=None), \
                patch.object(phone_ios.subprocess, "run", run), contextlib.redirect_stdout(io.StringIO()):
            return phone_ios._run(args)


class ExportedEvidence(Fixture):
    def test_a_failed_export_of_a_requested_shot_is_not_a_pass(self):
        summary = self.run_list([{"do": "shot", "name": "proof"}], export_rc=1)
        self.assertFalse(summary["passed"], summary)
        self.assertIsNone(summary["attachments"])
        self.assertIn("export", summary.get("error") or "")

    def test_a_shot_that_exports_passes(self):
        summary = self.run_list([{"do": "shot", "name": "proof"}], export_rc=0)
        self.assertTrue(summary["passed"], summary)

    def test_a_list_that_keeps_no_picture_is_not_failed_by_the_export(self):
        summary = self.run_list([{"do": "sleep", "seconds": 1}], export_rc=1)
        self.assertTrue(summary["passed"], summary)


class OnePhone(Fixture):
    def test_wifi_off_switches_the_phone_whose_cable_it_checked(self):
        """V05: --device PHONE-A with RICHOS_IOS_DEVICE=PHONE-B. The cable check and both runs (read, press)
        must all name PHONE-A; the environment is left as it was."""
        checked, used = [], []

        def run(args):
            used.append(os.environ["RICHOS_IOS_DEVICE"])
            folder = Path(args.out)
            folder.mkdir(parents=True, exist_ok=True)
            value = "1" if len(used) == 1 else "0"  # on when read, off after the press
            (folder / "steps.jsonl").write_text(json.dumps({"do": "value", "ok": True, "detail": {"value": value}}) + "\n")
            return {"passed": True}
        with patch.dict(os.environ, {"RICHOS_IOS_DEVICE": "PHONE-B", "RICHOS_APPLE_TEAM": "fixture"}), \
                patch.object(phone_ios, "require_cable", checked.append), patch.object(phone_ios, "_run", run), \
                contextlib.redirect_stdout(io.StringIO()):
            code = phone_ios.wifi_off(SimpleNamespace(out=str(self.root / "wifi"), device="PHONE-A"))
            after = os.environ["RICHOS_IOS_DEVICE"]
        self.assertEqual(code, 0)
        self.assertEqual(checked, ["PHONE-A"])
        self.assertEqual(used, ["PHONE-A", "PHONE-A"], "the Wi-Fi was read or switched on a phone whose cable was not checked")
        self.assertEqual(after, "PHONE-B")


if __name__ == "__main__":
    result = unittest.main(exit=False, verbosity=1).result
    print("OK" if result.wasSuccessful() else "FAILED")
    raise SystemExit(0 if result.wasSuccessful() else 1)
