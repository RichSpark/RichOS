#!/usr/bin/env python3
"""mobile-ship-gate.test.py — no RichConnect build reaches users without the §104 speed PASS
(richos/mobile/perf/shipgate.py; CEO 2026-10-03, richos-hq/wiki/ceo-decisions.md §106).

A throwaway Git repository stands for richos and a scratch directory for the phone speed watch's
state (measured.json and the records it names). No phone, build, store or network.

  S1  no verdict: a missing verdict file refuses, and so does an unreadable one (fails closed)
  S2  a failing verdict refuses: the watch said "slower", or the record no longer passes the current
      limits when judged again
  S3  a verdict for different app code refuses
  S4  the passing case: a "good" verdict whose record passes cold and warm, for the same app code
      (a later commit that changed only tests or tooling is the same app code)
  S5  a checkout with uncommitted app code refuses; a missing record refuses
  S6  the command line: a refusal exits 1 with one REFUSED line naming the platform; there is no
      skip flag
  R1-R3  `randroid bundle` asks the gate before it builds: a refusal stops it with the gate's line
      and Gradle never starts, a pass lets the build start, and no argument skips it
  R4  a bundle signed with ANOTHER key is gated the same way (Google ties the app to whichever key
      signs its first upload)
  T1  `stamp`: the archived iPhone app's Info.plist gets the source commit and whether app code was
      uncommitted (read back by testflight.ts upload, which gates that commit)
  F1  the stand-in verdict a test writes (lib/ship_gate_fixture.py, named by RICHOS_SHIP_GATE_VERDICTS)
      passes the real gate for the same app code, says it is a stand-in, and still refuses other app code
  B1-B4  `randroid bundle` also refuses without a PASSING review walk of its exact commit (CEO 2026-10-04,
      §107; richos/mobile/perf/reviewwalk.py, written by `randroid review-walk`), before it builds: B1 no
      record, B2 a failed walk (naming the step), B3 a record for different code; B4 a passing record lets
      the build start and the pass line is printed; B5 the walk itself takes only an emulator and a full commit
"""
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import unittest.mock

HERE = Path(__file__).resolve().parent
PERF = HERE.parents[1] / "mobile/perf"
sys.path.insert(0, str(PERF))
sys.path.insert(0, str(HERE / "lib"))
import shipgate  # noqa: E402
import reviewwalk  # noqa: E402
import ship_gate_fixture  # noqa: E402

APP_FILE = {"android": "richos/mobile/native-android/app/src/main/kotlin/Main.kt",
            "ios": "richos/mobile/native-ios/App/Main.swift"}
TEST_FILE = {"android": "richos/mobile/native-android/app/src/test/kotlin/MainTest.kt",
             "ios": "richos/mobile/native-ios/UnitTests/MainTests.swift"}
GOOD = {"android": {"cold": [900] + [803] * 19, "warm": [100] * 20},
        "ios": {"cold": [470] * 19, "warm": [None] + [600] * 19}}
SLOW_COLD = {"android": [900] * 20, "ios": [700] * 19}


EMPTY_CONFIG = Path(tempfile.mkdtemp(prefix="ship-gate-config-")) / "gitconfig"
EMPTY_CONFIG.write_text("")


def git(repo, *args):
    # The operator's global configuration (hooks, signing) never reaches the throwaway repository.
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(GIT_CONFIG_GLOBAL=str(EMPTY_CONFIG), GIT_CONFIG_NOSYSTEM="1",
GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.com", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@example.com")
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True,
                          env=env).stdout.strip()


class Gate(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="ship-gate-"))
        self.addCleanup(lambda: subprocess.run(["rm", "-rf", str(self.tmp)], check=False))
        self.repo = self.tmp / "repo"
        self.home = self.tmp / "watch"
        self.home.mkdir()
        git(self.tmp, "init", "-q", "-b", "main", str(self.repo))
        for platform in ("android", "ios"):
            self.write(APP_FILE[platform], "one\n")
            self.write(TEST_FILE[platform], "test one\n")
        self.measured = self.commit("measured")

    def write(self, rel, text):
        path = self.repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self, message):
        git(self.repo, "add", "-A")
        git(self.repo, "commit", "-q", "-m", message)
        return git(self.repo, "rev-parse", "HEAD")

    def verdict(self, platform, verdict="good", cold=None, warm=None, commit=None, record=True):
        cold = GOOD[platform]["cold"] if cold is None else cold
        warm = GOOD[platform]["warm"] if warm is None else warm
        path = self.home / f"{platform}.json"
        if record:
            path.write_text(json.dumps({
                "build": {"commit": commit or self.measured, "dirty": False},
                "metrics": {"coldLaunch": {"samplesMs": cold},
                            "warmStandard": {"verdict": "PASS", "startsMs": warm}}}))
        entries = {}
        with contextlib.suppress(OSError, ValueError):
            entries = json.loads((self.home / "measured.json").read_text())
        entries[platform] = {"commit": commit or self.measured, "record": str(path), "verdict": verdict}
        (self.home / "measured.json").write_text(json.dumps(entries))

    def refused(self, platform, *words, **kwargs):
        kwargs.setdefault("checkout", True)
        with self.assertRaises(shipgate.Refused) as caught:
            shipgate.check(platform, self.repo, home=self.home, **kwargs)
        text = str(caught.exception)
        label = shipgate.LABEL[platform]
        head = git(self.repo, "rev-parse", "HEAD")[:12]
        self.assertIn(f"{label} build of {head}", text)  # names the platform and the commit
        for word in words:
            self.assertIn(word, text)
        return text

    def test_s1_no_verdict_refuses(self):
        for platform in ("android", "ios"):
            self.refused(platform, "does not exist")
        (self.home / "measured.json").write_text("{not json")
        for platform in ("android", "ios"):
            self.refused(platform, "unreadable")
        (self.home / "measured.json").write_text(json.dumps({"ios": {}}))
        self.refused("android", "no Android speed verdict")

    def test_s2_failing_verdict_refuses(self):
        for platform in ("android", "ios"):
            self.verdict(platform, verdict="slower")
            self.refused(platform, "'slower', not a pass")
            # The watch said good, but the record fails the limits as they are now.
            self.verdict(platform, cold=SLOW_COLD[platform])
            self.refused(platform, "does not pass §104", "cold FAIL")
            self.verdict(platform, warm=[None if platform == "ios" else 900] + [900] * 19)
            self.refused(platform, "does not pass §104", "warm FAIL")

    def test_s3_verdict_for_different_app_code_refuses(self):
        for platform in ("android", "ios"):
            self.verdict(platform)
        self.write(APP_FILE["android"], "two\n")
        self.write(APP_FILE["ios"], "two\n")
        self.commit("app code changed")
        for platform in ("android", "ios"):
            self.refused(platform, "different app code", self.measured[:12], APP_FILE[platform])
            self.refused(platform, "different app code", checkout=False, commit="HEAD")

    def test_s4_pass_for_the_same_app_code(self):
        for platform in ("android", "ios"):
            self.verdict(platform)
            result = shipgate.check(platform, self.repo, checkout=True, home=self.home)
            self.assertEqual((result["ok"], result["commit"], result["measured"]), (True, self.measured, self.measured))
            for words in ("§104 PASS", "cold PASS", "warm PASS"):
                self.assertIn(words, result["line"])
        # Only tests and tooling changed since the measurement: the same app code still passes.
        self.write(TEST_FILE["android"], "test two\n")
        self.write(TEST_FILE["ios"], "test two\n")
        self.write("richos/mobile/native-ios/bin/rios", "tool\n")
        self.write("richos/mobile/native-ios/Release/testflight.ts", "release tool\n")
        later = self.commit("tests only")
        for platform in ("android", "ios"):
            self.assertEqual(shipgate.check(platform, self.repo, checkout=True, home=self.home)["commit"], later)
            self.assertIn(later[:12], shipgate.check(platform, self.repo, commit=later, home=self.home)["line"])
        # Run from a subdirectory (randroid and testflight.ts pass their own folder): same answer.
        sub = self.repo / "richos/mobile/native-android"
        self.assertTrue(shipgate.check("android", sub, checkout=True, home=self.home)["ok"])

    def test_s5_uncommitted_app_code_or_missing_record_refuses(self):
        for platform in ("android", "ios"):
            self.verdict(platform)
        self.write(APP_FILE["android"], "edited, not committed\n")
        self.refused("android", "uncommitted app code")
        self.write("richos/mobile/native-ios/App/New.swift", "untracked\n")
        self.refused("ios", "uncommitted app code", "New.swift")
        git(self.repo, "checkout", "-q", "--", APP_FILE["android"])
        (self.repo / "richos/mobile/native-ios/App/New.swift").unlink()
        (self.home / "android.json").unlink()
        self.refused("android", "missing or unreadable")
        self.verdict("ios", commit="0" * 40)
        self.refused("ios", "does not have")

    def test_s6_command_line_refuses_without_a_skip(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            code = shipgate.main(["check", "--platform", "ios", "--repo", str(self.repo), "--commit", "nope"])
        self.assertEqual(code, 1)
        self.assertTrue(err.getvalue().startswith("REFUSED by the speed gate"), err.getvalue())
        self.assertIn("iPhone build: nope is not a commit", err.getvalue())
        for flag in ("--skip", "--force", "--allow-slow"):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                shipgate.main(["check", "--platform", "android", "--checkout", str(self.repo), flag])

    def test_t1_stamp_writes_the_source_commit_and_uncommitted_app_code_into_the_built_app(self):
        import plistlib
        for fmt in (plistlib.FMT_BINARY, plistlib.FMT_XML):
            plist = self.tmp / "Info.plist"
            plist.write_bytes(plistlib.dumps({"CFBundleIdentifier": "dev.richos.connect"}, fmt=fmt))
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(shipgate.main(["stamp", "--platform", "ios", "--checkout", str(self.repo),
                                                "--plist", str(plist)]), 0)
            info = plistlib.loads(plist.read_bytes())
            self.assertEqual((info["RichOSSourceCommit"], info["RichOSSourceDirty"], info["CFBundleIdentifier"]),
                             (self.measured, False, "dev.richos.connect"))
            self.assertEqual(plist.read_bytes().startswith(b"bplist"), fmt == plistlib.FMT_BINARY, "format kept")
        self.write(TEST_FILE["ios"], "a test, not app code\n")
        shipgate.stamp("ios", self.repo, plist)
        self.assertIs(plistlib.loads(plist.read_bytes())["RichOSSourceDirty"], False)
        self.write(APP_FILE["ios"], "edited, not committed\n")
        self.assertTrue(shipgate.stamp("ios", self.repo, plist)["dirty"])
        self.assertIs(plistlib.loads(plist.read_bytes())["RichOSSourceDirty"], True)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(shipgate.main(["stamp", "--platform", "ios", "--checkout", str(self.tmp / "nowhere"),
                                            "--plist", str(plist)]), 1)

    def test_f1_a_stand_in_verdict_passes_the_real_gate_only_for_the_same_app_code(self):
        stand_in = self.tmp / "stand-in"
        for platform in ("android", "ios"):
            ship_gate_fixture.write(stand_in, platform, self.measured)

        def gate(platform):
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err), \
                    unittest.mock.patch.dict(os.environ, {"RICHOS_SHIP_GATE_VERDICTS": str(stand_in)}):
                code = shipgate.main(["check", "--platform", platform, "--checkout", str(self.repo)])
            return code, out.getvalue(), err.getvalue()

        for platform in ("android", "ios"):
            code, out, err = gate(platform)
            self.assertEqual(code, 0, err)
            result = json.loads(out)
            self.assertEqual((result["commit"], result["verdicts"]), (self.measured, str(stand_in)))
            self.assertIn("STAND-IN verdicts", result["line"])
        self.write(APP_FILE["android"], "two\n")
        self.commit("app code changed")
        code, _, err = gate("android")
        self.assertEqual(code, 1)
        self.assertIn("different app code", err)
        with self.assertRaises(SystemExit):
            ship_gate_fixture.write(ship_gate_fixture.WATCH_STATE, "android", self.measured)

    def test_f2_a_stand_in_covers_only_the_exact_uncommitted_tree_and_the_real_gate_never_does(self):
        stand_in = self.tmp / "stand-in"
        self.verdict("android")
        self.write(APP_FILE["android"], "merge tree\n")
        ship_gate_fixture.write(stand_in, "android", self.measured, repo=self.repo)
        self.assertTrue(shipgate.check("android", self.repo, checkout=True, home=stand_in)["ok"])
        # The real gate (no stand-in): the same uncommitted code is refused.
        self.refused("android", "uncommitted app code")
        # Any further edit is no longer the covered tree.
        self.write(APP_FILE["android"], "edited again\n")
        with self.assertRaises(shipgate.Refused):
            shipgate.check("android", self.repo, checkout=True, home=stand_in)
        # The watch's own directory never honors a coversTree claim.
        entries = json.loads((self.home / "measured.json").read_text())
        entries["android"]["coversTree"] = shipgate.tree_fingerprint("android", self.repo)
        (self.home / "measured.json").write_text(json.dumps(entries))
        with unittest.mock.patch.object(shipgate.watch, "DEFAULT_HOME", str(self.home)):
            with self.assertRaises(shipgate.Refused):
                shipgate.check("android", self.repo, checkout=True, home=str(self.home))


FAKE_GATE = """#!/usr/bin/env python3
import os, sys
open(os.environ["FAKE_LOG"], "a").write("gate " + " ".join(sys.argv[1:]) + "\\n")
if os.environ.get("FAKE_GATE") != "pass":
    print("REFUSED by the speed gate (CEO §106): Android build of 0123456789ab: no speed verdict file", file=sys.stderr)
    sys.exit(1)
print('{"ok": true}')
"""
FAKE_GRADLE = """#!/usr/bin/env python3
import os, sys
open(os.environ["FAKE_LOG"], "a").write("gradle " + " ".join(sys.argv[1:]) + "\\n")
sys.exit(1)
"""


class RandroidBundle(unittest.TestCase):
    """R1-R3: `randroid bundle` asks the gate before it builds, and stops on a refusal. The real randroid
    runs in a throwaway copy of its folder beside a stand-in gate and a stand-in Gradle; nothing builds."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="ship-gate-randroid-"))
        self.addCleanup(lambda: subprocess.run(["rm", "-rf", str(self.tmp)], check=False))
        repo, self.log = self.tmp / "repo", self.tmp / "log"
        git(self.tmp, "init", "-q", "-b", "main", str(repo))
        here = repo / "richos/mobile/native-android"
        files = {
            "richos/mobile/native-android/bin/randroid": (PERF.parent / "native-android/bin/randroid").read_text(),
            "richos/mobile/native-android/bin/apk-install.sh":
                (PERF.parent / "native-android/bin/apk-install.sh").read_text(),  # randroid sources it
            "richos/mobile/native-android/release/make-app-icon.cjs": "process.exit(0);\n",
            "richos/mobile/native-android/release/upload-certificate.sha256":
                (PERF.parent / "native-android/release/upload-certificate.sha256").read_text(),
            "richos/mobile/perf/shipgate.py": FAKE_GATE,
            "richos/mobile/perf/reviewwalk.py": (PERF / "reviewwalk.py").read_text(),
            "richos/mobile/machine_storage.py": (PERF.parent / "machine_storage.py").read_text(),
            "richos/engine/scripts/lib/native-work.py": FAKE_GRADLE,
            "richos/engine/scripts/lib/machine-settings.sh": (PERF.parents[1] / "engine/scripts/lib/machine-settings.sh").read_text(),
            "richos/engine/scripts/lib/machine_settings.py": (PERF.parents[1] / "engine/scripts/lib/machine_settings.py").read_text(),
        }
        for rel, text in files.items():
            (repo / rel).parent.mkdir(parents=True, exist_ok=True)
            (repo / rel).write_text(text)
        (here / "bin/randroid").chmod(0o755)
        git(repo, "add", "-A")
        git(repo, "commit", "-q", "-m", "layout")
        jdk = self.tmp / "jdk"
        (jdk / "bin").mkdir(parents=True)
        (jdk / "bin/java").write_text("#!/bin/sh\n")
        (jdk / "bin/java").chmod(0o755)
        (jdk / "release").write_text('JAVA_VERSION="21.0.1"\n')
        self.randroid = here / "bin/randroid"
        self.repo, self.walks = repo, self.tmp / "walks"
        self.head = git(repo, "rev-parse", "HEAD")
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(("GIT_", "RANDROID_", "ORG_GRADLE", "RICHOS_REVIEW_WALK"))}
        self.env.update(GIT_CONFIG_GLOBAL=str(EMPTY_CONFIG), GIT_CONFIG_NOSYSTEM="1", FAKE_LOG=str(self.log),
                        RANDROID_CACHE=str(self.tmp / "cache"), RANDROID_JAVA_HOME=str(jdk),
                        RICHOS_REVIEW_WALK_RECORDS=str(self.walks))
        for key in ("storeFile", "storePassword", "keyAlias", "keyPassword"):
            self.env[f"ORG_GRADLE_PROJECT_richos.upload.{key}"] = "x"
        for key in ("projectId", "appId", "apiKey", "senderId"):
            self.env[f"ORG_GRADLE_PROJECT_richos.firebase.{key}"] = "x"

    def bundle(self, **env):
        p = subprocess.run([str(self.randroid), "bundle"], env={**self.env, **env}, capture_output=True, text=True,
                           stdin=subprocess.DEVNULL)
        return p, (self.log.read_text() if self.log.exists() else "")

    def test_r1_a_refusal_stops_the_bundle_before_it_builds(self):
        p, log = self.bundle()
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("REFUSED by the speed gate", p.stderr)
        self.assertIn("gate check --platform android --checkout", log)
        self.assertNotIn("gradle", log)

    def test_r2_a_pass_lets_the_build_start(self):
        ship_gate_fixture.walk(self.walks, "android", self.head)  # and the walk gate's (B4)
        p, log = self.bundle(FAKE_GATE="pass")
        self.assertIn("gate check --platform android", log)
        self.assertIn(":app:bundleRelease", log)  # the stand-in Gradle then fails: "did not build"
        self.assertIn("did not build", p.stderr)

    def test_r4_a_bundle_signed_with_another_key_is_gated_too(self):
        p, log = self.bundle(RANDROID_UPLOAD_CERT_SHA256="AB:" * 31 + "AB")
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("REFUSED by the speed gate", p.stderr)
        self.assertIn("gate check --platform android --checkout", log)
        self.assertNotIn("gradle", log)

    def test_r3_no_flag_skips_it(self):
        p, log = self.bundle_args("--skip-speed")
        self.assertNotEqual(p.returncode, 0)
        self.assertNotIn("gradle", log)

    # --- B1-B4: the review-walk gate (CEO §107). The speed gate passes in each, so only the walk decides.
    def walk(self, commit, failed_step=None):
        names = ["sign in on the access page", "get a pairing link", "open the pairing link in the app",
                 "the six words match; They match in the app and on the page",
                 "send a message and see the Demo reply", "reset the Google review host"]
        steps = [{"name": n, "ok": n != failed_step,
                  "detail": "no Demo reply within 60 s" if n == failed_step else "ok"} for n in names]
        return reviewwalk.write({"platform": "android", "commit": commit, "passed": failed_step is None,
                                 "at": "2026-10-04T10:00:00Z", "host": "google-review.example", "steps": steps},
                                root=self.walks)

    def walk_refused(self, *phrases, **env):
        p, log = self.bundle(FAKE_GATE="pass", **env)
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("REFUSED by the review-walk gate (CEO ", p.stderr)
        self.assertIn(f"Android build of {self.head[:12]}", p.stderr)
        for phrase in phrases:
            self.assertIn(phrase, p.stderr)
        self.assertIn("gate check --platform android", log, "the speed gate ran first and passed")
        self.assertNotIn("gradle", log, "nothing built")
        return p

    def test_b1_no_walk_record_refuses(self):
        self.walk_refused("no review walk has run on this commit", f"{self.head}.json does not exist")
        # The real record directory (no stand-in named) has no walk of this throwaway commit either.
        env = {k: v for k, v in self.env.items() if k != "RICHOS_REVIEW_WALK_RECORDS"}
        p = subprocess.run([str(self.randroid), "bundle"], env={**env, "FAKE_GATE": "pass"}, capture_output=True,
                           text=True, stdin=subprocess.DEVNULL)
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertIn("no review walk has run on this commit", p.stderr)
        self.assertIn(reviewwalk.ROOT, p.stderr)

    def test_b2_a_failed_walk_refuses_naming_the_step(self):
        self.walk(self.head, failed_step="send a message and see the Demo reply")
        self.walk_refused("FAILED at step 'send a message and see the Demo reply'", "no Demo reply within 60 s")

    def test_b3_a_walk_of_different_code_refuses(self):
        self.walk(self.head)
        walked = self.head
        app = self.repo / "richos/mobile/native-android/app/Main.kt"
        app.parent.mkdir(parents=True, exist_ok=True)
        app.write_text("changed after the walk\n")
        git(self.repo, "add", "-A")
        git(self.repo, "commit", "-q", "-m", "app code after the walk")
        self.head = git(self.repo, "rev-parse", "HEAD")
        self.walk_refused("no review walk has run on this commit")
        # A record filed under this commit that describes another commit is not this build's walk.
        record = json.loads((self.walks / "android" / f"{walked}.json").read_text())
        (self.walks / "android" / f"{self.head}.json").write_text(json.dumps(record))
        self.walk_refused(f"is for Android {walked[:12]}, not this build")

    def test_b4_a_passing_walk_of_this_commit_lets_the_build_start(self):
        self.walk(self.head)
        p, log = self.bundle(FAKE_GATE="pass")
        self.assertNotIn("REFUSED", p.stderr)
        self.assertIn(f"review walk: Android build of {self.head[:12]}: the review walk PASSED on google-review.example",
                      p.stderr)
        self.assertIn("STAND-IN walk records", p.stderr)
        self.assertIn(":app:bundleRelease", log)  # the stand-in Gradle then fails: "did not build"

    def test_b5_the_walk_never_reaches_a_phone_or_unnamed_code(self):
        """bin/review-walk.mjs (what `randroid review-walk` runs) takes only an emulator serial and a full
        commit, and refuses anything else before it touches a device, the review service or the record."""
        walk = PERF.parent / "native-android/bin/review-walk.mjs"
        base = ["--adb", "adb", "--package", "dev.richos.connect", "--activity", "dev.richos.connect/x.Main",
                "--apk", "/nonexistent.apk", "--writer", "/nonexistent.py"]
        cases = (([*base, "--serial", "PHONESERIAL0001", "--commit", "a" * 40], "emulators only"),
                 ([*base, "--serial", "emulator-5580", "--commit", "abc1234"], "--commit must be a full commit"),
                 (["--serial", "emulator-5580", "--commit", "a" * 40], "needs --adb"))
        for argv, why in cases:
            p = subprocess.run(["node", str(walk), *argv], capture_output=True, text=True, stdin=subprocess.DEVNULL,
                               env={**self.env, "PATH": os.environ.get("PATH", "")})
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
            self.assertIn(why, p.stdout)

    def bundle_args(self, *args):
        p = subprocess.run([str(self.randroid), "bundle", *args], env=self.env, capture_output=True, text=True,
                           stdin=subprocess.DEVNULL)
        return p, (self.log.read_text() if self.log.exists() else "")


if __name__ == "__main__":
    try:
        unittest.main(verbosity=2)
    finally:
        subprocess.run(["rm", "-rf", str(EMPTY_CONFIG.parent)], check=False)
