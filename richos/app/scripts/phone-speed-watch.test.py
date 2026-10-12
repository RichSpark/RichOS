#!/usr/bin/env python3
"""phone-speed-watch.test.py — the phone speed check that runs by itself after a mobile land
(richos/mobile/perf/watch.py) and the guard in front of the phones (phone_guard.py).

Fake phones, a fake land: a throwaway Git repository stands for richos, a stand-in adb and xcrun
list one Android phone and one wired iPhone, stand-in `randroid`/`rios` command lines answer the
`device install` and `device perf` verbs, a stand-in perf.py answers `compare`, and escalations go
to a scratch file. No phone, emulator, simulator, build or window; nothing touches this repository.

  W1-W3   the trigger: a land touching richos/mobile/ starts a runner, one that does not starts
          nothing, and a clone that is not the operator's checkout never does
  W4      a missed run (requested, never finished, no runner alive) is escalated once and retried
  W5-W8   the run: a good result is recorded; a slower one raises an escalation naming the range
          since the last good run; a verb that does not exist yet and a verb that tries to
          uninstall are reported (COULD NOT MEASURE, REFUSED) and nothing reaches the phone
  W9-W10  a busy phone: the run waits and never interrupts the user; still busy at the bound, it
          reports and leaves the user running
  W11     a phone whose app cannot have changed is not measured again
  W12     the land through git itself: autocheck's post-merge hook starts the run
  W13     old rounds are pruned to the newest 20; the good run's round is always kept
  G1-G3   the guard: uninstall, pm clear and install without -r are refused and stop the run; (device-cli-exempt: describes the guard test)
          install -r and reads pass; xcrun devicectl uninstall is refused
  B1      the busy test: another process naming the phone is busy, this run's own tree is not,
          the adb server is not
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
PERF = HERE.parents[1] / "mobile/perf"
WATCH = PERF / "watch.py"
GUARD = PERF / "phone_guard.py"
AUTOCHECK = HERE / "autocheck"
SERIAL = "FAKESERIAL0042"
UDID = "00008030-FAKEUDID0042"
IDENT = "AAAAAAAA-BBBB-CCCC-DDDD-FAKEIDENT042"

sys.path.insert(0, str(PERF))
import watch  # noqa: E402

FAKE_ADB = f"""#!/usr/bin/env python3
import os, sys
open(os.environ["FAKE_LOG"], "a").write("real-adb " + " ".join(sys.argv[1:]) + "\\n")
if sys.argv[1:] == ["devices"]:
    print("List of devices attached")
    if os.environ.get("FAKE_NO_ANDROID") != "1":
        print("{SERIAL}\\tdevice")
"""
FAKE_XCRUN = f"""#!/usr/bin/env python3
import json, os, sys
open(os.environ["FAKE_LOG"], "a").write("real-xcrun " + " ".join(sys.argv[1:]) + "\\n")
if sys.argv[1:4] == ["devicectl", "list", "devices"]:
    out = sys.argv[sys.argv.index("--json-output") + 1]
    json.dump({{"result": {{"devices": [{{"identifier": "{IDENT}", "connectionProperties": {{"transportType": "wired"}},
        "hardwareProperties": {{"udid": "{UDID}", "reality": "physical", "deviceType": "iPhone"}}}}]}}}}, open(out, "w"))
"""
# randroid / rios stand-in: `<platform> device install|perf --serial|--device ID ...`.
FAKE_CLI = """#!/usr/bin/env python3
import json, os, subprocess, sys
platform, args = sys.argv[1], sys.argv[2:]
open(os.environ["FAKE_LOG"], "a").write(f"cli-{platform} " + " ".join(args) + "\\n")
mode = os.environ.get("FAKE_MODE_" + platform.upper(), "good")
if mode == "noverb":
    print("{\\"ok\\": false, \\"error\\": \\"unknown mode 'device'; see randroid --help\\"}", file=sys.stderr)
    sys.exit(1)
if mode == "uninstall" and args[:2] == ["device", "install"]:
    # A tool that ignores a failed uninstall and goes on to install: both must be stopped.
    subprocess.run(["adb", "uninstall", "dev.richos.connect"])  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
    subprocess.run(["adb", "install", "twin.apk"])  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
    sys.exit(1)
if args[:2] == ["device", "perf"]:
    # mode "VERDICT[:EXIT[:BLANK]]": perf.py writes its record first, then exits 1 (a phase failed) or
    # 4 (slower). BLANK is the cold-start blank check's verdict in it (metrics.coldBlank, as
    # blankstart.metric writes it): pass, fail or none; Android runs it by default, the iPhone not yet.
    verdict, code, blank = (mode.split(":") + ["", ""])[:3]
    slow = os.environ.get("FAKE_SLOW_COMMIT")  # a phone in good condition: only this commit's build is slow
    if slow:
        verdict = "slower" if args[args.index("--expect-commit") + 1] == slow else "good"
    blank = blank or ("pass" if platform == "android" else "none")
    record = {"fake": verdict, "metrics": {}, "phases": {}}
    # The speed standard (CEO 2026-10-03, §104) judges the cold series: "good" is 20 starts well under the
    # limits (start 1 slow, never judged); "slower" has a start 3 over them; the iPhone's record has no start 1.
    cold = [500] * 20 if verdict != "slower" else [500, 500, 1500] + [500] * 17
    cold[0] = 9000 if platform == "android" else cold[0]
    record["metrics"]["coldLaunch"] = {"samplesMs": cold if platform == "android" else cold[1:]}
    warm = os.environ.get("FAKE_WARM", "pass")
    record["metrics"]["warmStandard"] = (
        {"verdict": "LIMIT NOT SET", "why": "the warm-start limits are not set"} if warm == "unset" else
        {"verdict": warm.upper(), "why": "fixture warm", "startsMs": [100, 100]})
    if blank == "none":
        record["phases"]["cold-blank"] = "NOT MEASURED: the RichConnect icon is not on the launcher's first page"
    else:
        record["metrics"]["coldBlank"] = {
            "verdict": blank.upper(), "statistic": "median", "valueMs": 120.0 if blank == "pass" else 900.0,
            "limitMs": 400, "samplesMs": [120.0, 900.0, 900.0], "starts": [{}, {}, {}],
            "why": [] if blank == "pass" else ["the median of 3 starts' longest blank stretch is 900 ms, over the 400 ms limit"],
            "overLimit": [] if blank == "pass" else [{"recording": "start-002.mp4", "worstMs": 900.0}, {"recording": "start-003.mp4", "worstMs": 950.0}]}
    out = args[args.index("--out") + 1]
    json.dump(record, open(out, "w"))
    sys.exit(int(code or 0))
"""
# with-android-signing.py stand-in: records that the command ran inside it, then runs the command.
FAKE_SIGNING = """#!/usr/bin/env python3
import os, sys
open(os.environ["FAKE_LOG"], "a").write("signing-wrapper " + " ".join(sys.argv[1:]) + "\\n")
os.execvp(sys.argv[1], sys.argv[1:])
"""
FAKE_PERF = """#!/usr/bin/env python3
import json, sys
assert sys.argv[1:3] == ["compare", "--json"], sys.argv
verdict = json.load(open(sys.argv[3]))["fake"]
status = {"good": "WITHIN NOISE", "slower": "SLOWER"}.get(verdict, "NOT COMPARED")
p95 = {"good": 900, "slower": 1400}.get(verdict)
print(json.dumps({"verdict": {"good": "NOT SLOWER", "slower": "SLOWER THAN THE ESTABLISHED BENCHMARK"}.get(verdict, "NOT COMPARED"),
                  "metrics": {m: {"status": status, "p95Ms": p95, "limitMs": 1000, "why": "fixture"} for m in ("coldLaunch", "warmResume")}}))
sys.exit({"good": 0, "slower": 4}.get(verdict, 5))
"""
FAKE_ESCALATE = """#!/usr/bin/env python3
import json, os, sys
fields = json.load(open(sys.argv[sys.argv.index("--fields") + 1]))
open(os.environ["FAKE_ESCALATIONS"], "a").write(json.dumps(fields) + "\\n")
"""
FAKE_RUNNER = """#!/usr/bin/env python3
import os, sys
open(os.environ["FAKE_RUNNER_LOG"], "a").write(" ".join(sys.argv[1:]) + "\\n")
"""


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="phone-watch-")
        self.base = Path(self.tmp.name).resolve()
        self.repo = self.base / "repo"
        self.bin = self.base / "bin"
        self.bin.mkdir()
        for name, text in (("adb", FAKE_ADB), ("xcrun", FAKE_XCRUN), ("cli.py", FAKE_CLI), ("perf.py", FAKE_PERF),
                           ("escalate.py", FAKE_ESCALATE), ("signing.py", FAKE_SIGNING), ("runner.py", FAKE_RUNNER)):
            (self.bin / name).write_text(text)
            (self.bin / name).chmod(0o755)
        (self.base / "gitconfig").write_text("[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n"
                                              "[init]\n\tdefaultBranch = main\n")
        self.log = self.base / "tools.log"
        self.escalations = self.base / "escalations.jsonl"
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(("GIT_", "RICHOS_PHONE_WATCH", "RICHOS_AUTOCHECK", "FAKE_"))}
        self.env.update(
            GIT_CONFIG_GLOBAL=str(self.base / "gitconfig"), GIT_CONFIG_NOSYSTEM="1",
            FAKE_LOG=str(self.log), FAKE_ESCALATIONS=str(self.escalations),
            FAKE_RUNNER_LOG=str(self.base / "runner.log"),
            RICHOS_PHONE_WATCH_HOME=str(self.base / "home"), RICHOS_PHONE_WATCH_REPO=str(self.repo),
            RICHOS_PHONE_WATCH_ESCALATE=f"{sys.executable} {self.bin / 'escalate.py'}",
            RICHOS_PHONE_WATCH_ADB=str(self.bin / "adb"), RICHOS_PHONE_WATCH_XCRUN=str(self.bin / "xcrun"),
            RICHOS_PHONE_WATCH_ANDROID_CLI=f"{sys.executable} {self.bin / 'cli.py'} android",
            RICHOS_PHONE_WATCH_IOS_CLI=f"{sys.executable} {self.bin / 'cli.py'} ios",
            RICHOS_PHONE_WATCH_SIGNING=f"{sys.executable} {self.bin / 'signing.py'}",
            RICHOS_PHONE_WATCH_PERF=str(self.bin / "perf.py"),
            RICHOS_PHONE_WATCH_POLL_S="0", RICHOS_PHONE_WATCH_TRIALS="3",
            # Only processes naming the made-up phones count: the real phones on this Mac may be in use.
            RICHOS_PHONE_WATCH_MARKERS="ident", RICHOS_PHONE_WATCH_MAX_WAIT_S="60",
            RICHOS_APPLE_TEAM="FAKETEAM42")
        self.repo.mkdir()
        self.git("init", "-q")
        self.write("richos/app/src/thing.txt", "fine\n")
        self.write("richos/mobile/native-android/App.kt", "fast\n")
        self.write("richos/mobile/native-ios/App.swift", "fast\n")
        self.commit("base")

    def tearDown(self):
        self.tmp.cleanup()

    def run_(self, argv, env=None, cwd=None, timeout=120):
        return subprocess.run(argv, cwd=cwd or self.repo, env=env or self.env, capture_output=True, text=True,
                              timeout=timeout)

    def git(self, *args):
        p = self.run_(["git", *args])
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout.strip()

    def write(self, rel, text):
        path = self.repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)
        return self.git("rev-parse", "HEAD")

    def watch(self, *args, env=None):
        return self.run_([sys.executable, str(WATCH), *args], env=env)

    def ledger(self):
        path = self.base / "home/ledger.jsonl"
        return [json.loads(l) for l in path.read_text().splitlines()] if path.exists() else []

    def escalated(self):
        return [json.loads(l) for l in self.escalations.read_text().splitlines()] if self.escalations.exists() else []

    def tools(self):
        return self.log.read_text() if self.log.exists() else ""

    def request(self, main=None):
        """A request in the ledger, as the trigger writes it."""
        main = main or self.git("rev-parse", "HEAD")
        (self.base / "home").mkdir(exist_ok=True)
        with open(self.base / "home/ledger.jsonl", "a") as f:
            f.write(json.dumps({"at": watch.now(), "event": "requested", "id": f"r{time.time_ns()}", "main": main}) + "\n")

    def run_round(self, env=None):
        p = self.watch("run", "--repo", str(self.repo), env=env)
        self.assertEqual(p.returncode, 0, p.stderr)
        return [r for r in self.ledger() if r["event"] == "outcome"]


class Trigger(Base):
    def setUp(self):
        super().setUp()
        self.env["RICHOS_PHONE_WATCH_RUNNER"] = f"{sys.executable} {self.bin / 'runner.py'}"

    def runner_calls(self):
        path = self.base / "runner.log"
        for _ in range(50):  # the runner is detached; give it a moment to write its line
            if path.exists():
                return path.read_text().splitlines()
            time.sleep(0.1)
        return []

    def test_W1_a_land_touching_mobile_starts_a_run(self):
        old = self.git("rev-parse", "HEAD")
        self.write("richos/mobile/native-android/App.kt", "slower?\n")
        new = self.commit("android change")
        p = self.watch("trigger", "--repo", str(self.repo), "--from", old, "--to", new)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("phone run requested", p.stderr)
        requests = [r for r in self.ledger() if r["event"] == "requested"]
        self.assertEqual([r["main"] for r in requests], [new])
        self.assertEqual(self.runner_calls(), [f"run --repo {self.repo}"])

    def test_W2_a_land_not_touching_mobile_starts_nothing(self):
        old = self.git("rev-parse", "HEAD")
        self.write("richos/app/src/thing.txt", "other\n")
        new = self.commit("app change")
        p = self.watch("trigger", "--repo", str(self.repo), "--from", old, "--to", new)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("changes nothing under richos/mobile/", p.stderr)
        # The trigger writes `spawned` before it returns whenever it starts a runner: none was written.
        self.assertEqual(self.ledger(), [])
        self.assertFalse((self.base / "runner.log").exists())

    def test_W3_a_clone_that_is_not_the_operator_checkout_never_reaches_a_phone(self):
        old = self.git("rev-parse", "HEAD")
        self.write("richos/mobile/native-ios/App.swift", "slower?\n")
        new = self.commit("ios change")
        env = dict(self.env, RICHOS_PHONE_WATCH_REPO=str(self.base / "elsewhere"))
        p = self.watch("trigger", "--repo", str(self.repo), "--from", old, "--to", new, env=env)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("not the operator's main checkout", p.stderr)
        self.assertEqual(self.ledger(), [])

    def test_W4_a_missed_run_is_escalated_once_and_retried(self):
        head = self.git("rev-parse", "HEAD")
        (self.base / "home").mkdir()
        stale = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 3600))
        (self.base / "home/ledger.jsonl").write_text(json.dumps(
            {"at": stale, "event": "requested", "id": "lost1", "main": head}) + "\n")
        self.write("richos/app/src/thing.txt", "other\n")
        new = self.commit("app change, not mobile")
        p = self.watch("trigger", "--repo", str(self.repo), "--from", head, "--to", new)
        self.assertEqual(p.returncode, 0, p.stderr)
        titles = [e["title"] for e in self.escalated()]
        self.assertEqual(len(titles), 1, titles)
        self.assertIn("MISSED", titles[0])
        self.assertEqual(self.runner_calls(), [f"run --repo {self.repo}"], "the missed request is retried")
        self.watch("trigger", "--repo", str(self.repo), "--from", head, "--to", new)
        self.assertEqual(len(self.escalated()), 1, "reported once, not at every land")


class Run(Base):
    def test_W5_a_good_run_is_recorded_and_raises_nothing(self):
        head = self.git("rev-parse", "HEAD")
        self.request()
        outcomes = self.run_round()
        self.assertEqual(sorted((o["platform"], o["verdict"]) for o in outcomes), [("android", "good"), ("ios", "good")])
        blank = {o["platform"]: o["blankScreen"] for o in outcomes}
        self.assertTrue(blank["android"].startswith("PASS: median 120 ms of 3 starts"), blank)
        self.assertEqual(self.escalated(), [])
        good = json.loads((self.base / "home/good.json").read_text())
        self.assertEqual(good["android"]["commit"], head)
        self.assertEqual(good["ios"]["commit"], head)
        calls = self.tools()
        self.assertIn(f"cli-android device install --serial {SERIAL} --expect-commit {head}", calls)
        self.assertIn(f"cli-ios device perf --device {IDENT} --expect-commit {head} --cold 0 --launches 20 --warm 20", calls)
        self.assertIn(f"cli-android device perf --serial {SERIAL} --expect-commit {head} --cold 20 --warm 20", calls)
        self.assertIn("cold PASS", [o for o in outcomes if o["platform"] == "ios"][0]["standard"])
        self.assertNotIn("real-adb install", calls, "the phone is reached only through the platform verbs")
        finished = [r for r in self.ledger() if r["event"] == "finished"]
        self.assertEqual(len(finished), 1)
        self.assertEqual(watch.uncovered.__name__, "uncovered")
        p = self.watch("run", "--repo", str(self.repo))
        self.assertIn("nothing pending", p.stderr)

    def test_W19_a_land_that_changes_the_watch_is_run_by_its_own_watch_code(self):
        # The 2026-10-03 defect: a runner started before the standard landed judged later lands with old code.
        self.write("richos/mobile/perf/watch.py",
                   "import os, sys\nopen(os.environ['FAKE_LOG'], 'a').write('land-watch ' + ' '.join(sys.argv[1:]) + '\\n')\n")
        self.commit("the watch changes")
        self.request()
        p = self.watch("run", "--repo", str(self.repo))
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"land-watch run --repo {self.repo}", self.tools())
        self.assertNotIn("cli-android device perf", self.tools(), "the old code measured nothing")

    def test_W17_the_android_install_and_perf_run_inside_the_signing_wrapper_and_the_iphone_does_not(self):
        # The 2026-10-02 run: randroid refused the unsigned Release APK, so the Android was never measured.
        self.request()
        self.run_round()
        # The two platforms run side by side into one log, so no line is promised to sit beside another:
        # the wrapper records the command it wraps on its own line.
        wrapped = [l for l in self.tools().splitlines() if l.startswith("signing-wrapper")]
        for verb in ("install", "perf"):  # perf builds the debuggable twin, which must carry the upload key too
            self.assertEqual(len([l for l in wrapped if f"android device {verb}" in l]), 1, (verb, wrapped))
        self.assertEqual(len(wrapped), 2, ("the iPhone verbs never go through the Android key", wrapped))

    def test_W18_a_missing_signing_wrapper_is_a_run_that_could_not_measure(self):
        self.request()
        outcomes = self.run_round(env=dict(self.env, RICHOS_PHONE_WATCH_SIGNING=f"{sys.executable} {self.base}/nope.py"))
        android = [o for o in outcomes if o["platform"] == "android"][0]
        self.assertEqual(android["verdict"], "unmeasured")
        self.assertIn("signing wrapper", android["why"])
        self.assertNotIn("cli-android device install", self.tools())

    def test_W6_a_slower_run_escalates_with_the_range_since_the_last_good_run(self):
        good_sha = self.git("rev-parse", "HEAD")
        self.request()
        self.run_round()
        self.write("richos/mobile/native-android/App.kt", "slow\n")
        slow_sha = self.commit("android: the change that slowed it")
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_MODE_ANDROID="slower"))
        android = [o for o in outcomes if o["platform"] == "android"][-1]
        self.assertEqual(android["verdict"], "slower")
        [esc] = self.escalated()
        self.assertIn("Android start FAILS the speed standard", esc["title"])
        self.assertIn(f"{good_sha[:12]}..{slow_sha[:12]}", esc["question"])
        self.assertIn("the change that slowed it", esc["question"])
        self.assertIn("cold start 3 took 1500 ms, not under 1000 ms", esc["question"])
        self.assertIn("coldLaunch p95 1400 ms (SLOWER, limit 1000 ms)", json.dumps(esc),
                      "the old p95 comparison is information only")
        self.assertEqual(json.loads((self.base / "home/good.json").read_text())["android"]["commit"], good_sha,
                         "a slower run never becomes the good run")

    def slow_round(self, phone_is_slow):
        """A good round, then an Android app change whose round fails the standard. With `phone_is_slow` every
        build fails on the phone; otherwise only the new commit's build does. Returns (good sha, slow sha, escalation)."""
        good_sha = self.git("rev-parse", "HEAD")
        self.request()
        self.run_round()
        self.write("richos/mobile/native-android/App.kt", "slow\n")
        slow_sha = self.commit("android: the change under test")
        self.request()
        self.log.unlink()
        env = dict(self.env, FAKE_MODE_ANDROID="slower") if phone_is_slow else dict(self.env, FAKE_SLOW_COMMIT=slow_sha)
        self.run_round(env=env)
        [esc] = self.escalated()
        return good_sha, slow_sha, esc

    def test_W22_a_failure_the_last_good_build_does_not_share_is_a_code_regression(self):
        good_sha, slow_sha, esc = self.slow_round(phone_is_slow=False)
        self.assertIn("(a code regression)", esc["title"])
        self.assertTrue(esc["question"].startswith("A CODE REGRESSION in "), esc["question"])
        self.assertIn(f"{good_sha[:12]}..{slow_sha[:12]}", esc["question"])
        self.assertIn("still passes", esc["question"])

    def test_W23_a_failure_the_last_good_build_shares_is_the_phones_condition(self):
        # CEO 2026-10-03: the watch named the code as the suspect; the last good build failed too (the phone).
        good_sha, slow_sha, esc = self.slow_round(phone_is_slow=True)
        self.assertIn("(the phone's condition, not the code)", esc["title"])
        self.assertTrue(esc["question"].startswith(f"THE PHONE'S CONDITION, NOT THE CODE: the last good build ({good_sha[:12]})"))
        self.assertIn(f"cli-android device perf --serial {SERIAL} --expect-commit {good_sha}", self.tools(),
                      "the last good build was measured on the same phone")
        self.assertIn(f"cli-android device install --serial {SERIAL} --expect-commit {slow_sha}", self.tools()[
            self.tools().index(f"--expect-commit {good_sha} --cold"):], "the build under test went back on the phone")

    def test_W6b_the_warm_test_fails_and_its_unset_limits_are_loud(self):
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_WARM="fail"))
        self.assertEqual({o["platform"]: o["verdict"] for o in outcomes}, {"android": "slower", "ios": "slower"})
        self.assertIn("warm FAIL", self.escalated()[0]["question"])
        self.escalations.unlink()
        self.write("richos/mobile/native-android/App.kt", "changed\n")  # app code changed: the phones run again
        self.write("richos/mobile/native-ios/App.swift", "changed\n")
        self.commit("both apps change")
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_WARM="unset"))
        self.assertEqual({o["platform"]: o["verdict"] for o in outcomes[-2:]}, {"android": "unmeasured", "ios": "unmeasured"})
        self.assertIn("warm-start limits are not set", outcomes[-1]["why"])

    def test_W7_a_verb_that_does_not_exist_yet_is_a_run_that_could_not_measure(self):
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_MODE_ANDROID="noverb"))
        android = [o for o in outcomes if o["platform"] == "android"][0]
        self.assertEqual(android["verdict"], "unmeasured")
        self.assertIn("does not exist in main yet", android["why"])
        titles = [e["title"] for e in self.escalated()]
        self.assertTrue(any("Android run COULD NOT MEASURE" in t for t in titles), titles)
        for text in (self.escalations.read_text(), (self.base / "home/ledger.jsonl").read_text()):
            self.assertNotIn(SERIAL, text, "the owner's device identifiers never reach the ledger or an escalation")
            self.assertNotIn(IDENT, text)

    def test_W8_a_verb_that_would_uninstall_is_refused_and_the_phone_is_untouched(self):
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_MODE_ANDROID="uninstall"))
        android = [o for o in outcomes if o["platform"] == "android"][0]
        self.assertEqual(android["verdict"], "refused")
        self.assertIn("adb uninstall", android["why"])  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        calls = self.tools()
        self.assertNotIn("real-adb uninstall", calls)
        self.assertNotIn("real-adb install", calls, "the install after the refused uninstall was stopped too")
        titles = [e["title"] for e in self.escalated()]
        self.assertTrue(any("Android run REFUSED" in t for t in titles), titles)

    def test_W14_a_perf_verb_exiting_slower_with_its_record_is_judged_not_lost(self):
        # perf.py's measurement run (what `device perf` execs) writes the record, then exits 4 when it
        # is slower than the benchmark and 1 when a phase failed: the record is a measurement. Any
        # other exit (6: the phone's own state NOT restored) is a run that could not measure.
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_MODE_ANDROID="slower:4", FAKE_MODE_IOS="good:6"))
        android = [o for o in outcomes if o["platform"] == "android"][0]
        ios = [o for o in outcomes if o["platform"] == "ios"][0]
        self.assertEqual(android["verdict"], "slower", android)
        self.assertEqual(ios["verdict"], "unmeasured", ios)
        self.assertIn("exited 6", ios["why"])
        titles = [e["title"] for e in self.escalated()]
        self.assertTrue(any("Android start FAILS the speed standard" in t for t in titles), titles)
        self.assertTrue(any("iPhone run COULD NOT MEASURE" in t for t in titles), titles)

    def test_W15_a_blank_screen_on_the_android_start_is_a_regression_even_at_good_speed(self):
        # quint-opus-blank1's check runs inside the perf verb (perf.py cold-blank phase) and its FAIL
        # makes perf.py exit 1 with the record written; the run reads the verdict from the record.
        good_sha = self.git("rev-parse", "HEAD")
        self.request()
        first = self.run_round()
        ios = [o for o in first if o["platform"] == "ios"][0]
        self.write("richos/mobile/native-android/App.kt", "white flash\n")
        sha = self.commit("android: the change that put a blank screen on the start")
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_MODE_ANDROID="good:1:fail"))
        android = [o for o in outcomes if o["platform"] == "android"][-1]
        self.assertEqual(android["verdict"], "slower", android)
        self.assertIn("Android cold start BLANK SCREEN", android["blankScreen"][0])
        self.assertIn("2 of 3 starts over the limit, the worst 950 ms", android["blankScreen"][1])
        self.assertIn(f"--blank-starts 10", self.tools(), "the verb is asked for the blank check explicitly")
        self.assertIn("not checked on the iPhone yet", ios["blankScreen"])
        [esc] = self.escalated()
        self.assertIn("Android start shows a BLANK SCREEN", esc["title"])
        self.assertIn(f"{good_sha[:12]}..{sha[:12]}", esc["question"])
        self.assertIn("put a blank screen on the start", esc["question"])
        self.assertEqual(json.loads((self.base / "home/good.json").read_text())["android"]["commit"], good_sha,
                         "a blank start never becomes the good run")

    def test_W16_an_android_record_without_the_blank_verdict_could_not_measure(self):
        self.request()
        outcomes = self.run_round(env=dict(self.env, FAKE_MODE_ANDROID="good:1:none"))
        android = [o for o in outcomes if o["platform"] == "android"][0]
        self.assertEqual(android["verdict"], "unmeasured", android)
        self.assertIn("blank-screen check gave no verdict", android["why"])
        self.assertIn("icon is not on the launcher's first page", android["why"])
        self.assertIn("cold PASS", android["why"], "the speed it did measure is still said")
        titles = [e["title"] for e in self.escalated()]
        self.assertTrue(any("Android run COULD NOT MEASURE" in t for t in titles), titles)

    def test_W9_a_busy_phone_waits_and_is_not_interrupted(self):
        user = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(3)", SERIAL])
        try:
            self.request()
            env = dict(self.env, RICHOS_PHONE_WATCH_PLATFORMS="android", RICHOS_PHONE_WATCH_POLL_S="1",
                       RICHOS_PHONE_WATCH_MAX_WAIT_S="60")
            started = time.monotonic()
            outcomes = self.run_round(env=env)
            self.assertEqual(outcomes[0]["verdict"], "good")
            self.assertEqual(user.wait(timeout=30), 0, "the phone's user ran to its own end")
            self.assertGreaterEqual(time.monotonic() - started, 2.5, "the run waited for the user to finish")
            free = [r for r in self.ledger() if r["event"] == "phone-free"]
            self.assertGreaterEqual(free[0]["waited"], 2)
        finally:
            user.kill()

    def test_W10_still_busy_at_the_bound_is_reported_and_the_user_left_running(self):
        user = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", SERIAL])
        try:
            self.request()
            env = dict(self.env, RICHOS_PHONE_WATCH_PLATFORMS="android", RICHOS_PHONE_WATCH_POLL_S="1",
                       RICHOS_PHONE_WATCH_MAX_WAIT_S="2")
            outcomes = self.run_round(env=env)
            self.assertEqual(outcomes[0]["verdict"], "unmeasured")
            self.assertIn("stayed busy", outcomes[0]["why"])
            self.assertIn("time.sleep(60) <phone>", outcomes[0]["why"], "the busy process is named, the serial is not")
            self.assertNotIn(SERIAL[:6], self.escalations.read_text(), "not even a shortened serial")
            self.assertIsNone(user.poll(), "the user's process is still running: nothing interrupted it")
            self.assertNotIn("cli-android", self.tools(), "nothing was installed or measured")
        finally:
            user.kill()
            user.wait()

    def test_W11_a_phone_whose_app_cannot_have_changed_is_not_measured_again(self):
        self.request()
        self.run_round()
        self.write("richos/mobile/native-ios/App.swift", "faster\n")
        ios_sha = self.commit("ios only")
        self.log.unlink()
        self.request()
        outcomes = self.run_round()[-2:]
        verdicts = {o["platform"]: o["verdict"] for o in outcomes}
        self.assertEqual(verdicts, {"android": "unchanged", "ios": "good"})
        self.assertNotIn("cli-android", self.tools())
        good = json.loads((self.base / "home/good.json").read_text())
        self.assertEqual(good["android"]["commit"], ios_sha, "the good run moves forward, so the next range is short")


    def test_W20_a_land_changing_only_limits_tests_or_tooling_judges_the_old_samples_and_runs_no_phone(self):
        # CEO 2026-10-03: "why does it need to be all re-tested again? Everything was just tested minutes ago?"
        self.request()
        self.run_round()
        self.write("richos/mobile/native-ios/UITests/Start.swift", "a test\n")
        self.write("richos/mobile/native-ios/Tools/physical-device.mjs", "tooling\n")
        self.write("richos/mobile/native-android/app/src/test/StartTest.kt", "a test\n")
        self.write("richos/mobile/native-android/bin/randroid", "tooling\n")
        self.write("richos/mobile/perf/perfcore.py", "# limits\n")
        sha = self.commit("tests, tooling and limits only")
        self.log.unlink()
        self.request()
        outcomes = self.run_round()[-2:]
        self.assertEqual({o["platform"]: o["verdict"] for o in outcomes}, {"android": "unchanged", "ios": "unchanged"})
        for o in outcomes:
            self.assertIn("judged again against the current limits", o["why"])
        self.assertNotIn("cli-", self.tools(), "no phone was touched")
        good = json.loads((self.base / "home/good.json").read_text())
        self.assertEqual((good["android"]["commit"], good["ios"]["commit"]), (sha, sha))

    def test_W21_which_paths_are_app_code(self):
        a, i = "richos/mobile/native-android/", "richos/mobile/native-ios/"
        for platform, root, app, other in (
                ("android", a, ("app/src/main/Start.kt", "core/src/main/State.kt", "build.gradle.kts", "gradle/libs.versions.toml",
                                "app/src/debug/Dev.kt"),
                 ("bin/randroid", "cli/src/main/Cli.kt", "app/src/test/T.kt", "core/src/testDebug/T.kt", "README.md")),
                ("ios", i, ("App/Features/Home.swift", "Core/Sources/X.swift", "project.yml", "Release/platform.yml",
                            "ShareExtension/S.swift", "NotificationService/N.swift"),
                 ("bin/rios", "Tools/x.mjs", "UITests/U.swift", "UnitTests/U.swift", "Core/Tests/T.swift", "docs/x.md"))):
            for p in app:
                self.assertTrue(watch.is_app_code(platform, root + p), p)
            for p in other:
                self.assertFalse(watch.is_app_code(platform, root + p), p)
        self.assertFalse(watch.is_app_code("android", i + "App/Home.swift"), "the other phone's app")
        self.assertFalse(watch.is_app_code("ios", "richos/mobile/perf/perfcore.py"))

    def test_W22_only_what_the_iphone_build_uses_in_release_is_app_code(self):
        # A change to the TestFlight tool alone must not make the phone measure again (and the ship gate,
        # which uses the same rule, must not call it different app code).
        r = "richos/mobile/native-ios/Release/"
        for p in ("platform.yml", "App-Info.plist", "RichOSNative.entitlements", "RichOSNative.testcopy.entitlements",
                  "AnythingNew.plist"):
            self.assertTrue(watch.is_app_code("ios", r + p), p)
        for p in ("testflight.ts", "testflight.test.ts", "README.md", "ExportOptions.plist", "check-release.sh",
                  "platform-tests.sh", "simulator-tests.sh", "generate.sh", "make-app-icon.cjs",
                  "third-party/T3-Code-LICENSE.txt"):
            self.assertFalse(watch.is_app_code("ios", r + p), p)


class Prune(unittest.TestCase):
    def test_W13_old_rounds_are_pruned_and_the_good_run_is_kept(self):
        with tempfile.TemporaryDirectory(prefix="phone-watch-prune-") as tmp:
            os.environ["RICHOS_PHONE_WATCH_HOME"] = tmp
            try:
                runs = Path(tmp) / "runs"
                names = [f"20261002T{i:06d}Z-abc" for i in range(25)]
                for name in names:
                    (runs / name / "android").mkdir(parents=True)
                watch.set_good("android", {"commit": "abc", "record": str(runs / names[0] / "android/android.json")})
                removed = watch.prune_runs()
                left = sorted(p.name for p in runs.iterdir())
            finally:
                del os.environ["RICHOS_PHONE_WATCH_HOME"]
        self.assertEqual(removed, list(reversed(names[1:5])))
        self.assertEqual(left, [names[0], *names[5:]], "the newest 20 and the good run's round are kept")


class Land(Base):
    """W12: git itself starts the run, through autocheck's post-merge hook on main."""

    def test_W12_a_merge_into_main_touching_mobile_starts_the_run_through_the_hook(self):
        for name in ("autocheck.py", "shim.sh", "install.sh"):
            dest = self.repo / "richos/app/scripts/autocheck" / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(AUTOCHECK / name, dest)
        (self.repo / "richos/mobile/perf").mkdir(parents=True, exist_ok=True)
        for path in (WATCH, GUARD):
            shutil.copy(path, self.repo / "richos/mobile/perf" / path.name)
        shutil.copy(WATCH.parent.parent / "machine_storage.py", self.repo / "richos/mobile/machine_storage.py")
        (self.repo / "richos/engine").symlink_to(HERE.parents[1] / "engine")
        (self.repo / ".git/info/exclude").write_text("richos/engine\n")
        self.commit("the check and the watch")
        p = self.run_(["bash", str(AUTOCHECK / "install.sh"), str(self.repo)])
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        env = dict(self.env, RICHOS_PHONE_WATCH_RUNNER=f"{sys.executable} {self.bin / 'runner.py'}",
                   RICHOS_ESCALATION_LEDGER=str(self.base / "engine-ledger.jsonl"))
        self.run_(["git", "checkout", "-q", "-b", "feature"], env=env)
        self.write("richos/mobile/native-android/App.kt", "changed\n")
        self.run_(["git", "add", "-A"], env=env)
        p = self.run_(["git", "commit", "-q", "-m", "android change", "--no-verify"], env=env)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.run_(["git", "checkout", "-q", "main"], env=env)
        p = self.run_(["git", "merge", "-q", "--ff-only", "feature"], env=env)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("phone run requested", p.stderr)
        new = self.git("rev-parse", "HEAD")
        self.assertEqual([r["main"] for r in self.ledger() if r["event"] == "requested"], [new])
        for _ in range(50):
            if (self.base / "runner.log").exists():
                break
            time.sleep(0.1)
        self.assertEqual((self.base / "runner.log").read_text().splitlines(), [f"run --repo {self.repo}"])


class Guard(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="phone-guard-")
        base = Path(self.tmp.name)
        self.log = base / "real.log"
        for tool in ("adb", "xcrun"):
            real = base / f"real-{tool}"
            real.write_text(f"#!/bin/sh\necho \"{tool} $*\" >> {self.log}\n")
            real.chmod(0o755)
        self.env = dict(os.environ, RICHOS_PHONE_GUARD_TRIP=str(base / "trip"),
                        RICHOS_PHONE_GUARD_REAL_ADB=str(base / "real-adb"),
                        RICHOS_PHONE_GUARD_REAL_XCRUN=str(base / "real-xcrun"))
        self.trip = base / "trip"

    def tearDown(self):
        self.tmp.cleanup()

    def guard(self, *args):
        return subprocess.run([sys.executable, str(GUARD), *args], env=self.env, capture_output=True, text=True)

    def real(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_G1_uninstall_is_refused_and_stops_the_run(self):
        self.assertEqual(self.guard("adb", "-s", SERIAL, "shell", "pm", "path", "dev.richos.connect").returncode, 0)
        self.assertEqual(self.guard("adb", "-s", SERIAL, "install", "-r", "app.apk").returncode, 0)  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        p = self.guard("adb", "-s", SERIAL, "uninstall", "dev.richos.connect")  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        self.assertEqual(p.returncode, 97)
        self.assertIn("PHONE GUARD REFUSED: adb uninstall", p.stderr)  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        p = self.guard("adb", "-s", SERIAL, "install", "-r", "app.apk")  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        self.assertEqual(p.returncode, 97, "after a refusal nothing else of the run reaches the phone")
        self.assertEqual(self.real(), [f"adb -s {SERIAL} shell pm path dev.richos.connect",
                                       f"adb -s {SERIAL} install -r app.apk"])  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal

    def test_G2_data_clears_and_installs_without_r_are_refused(self):
        for args in (("shell", "pm clear dev.richos.connect"), ("shell", "pm", "uninstall", "dev.richos.connect"),  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
                     ("exec-out", "cmd", "package", "clear", "dev.richos.connect"), ("install", "app.apk"),  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
                     ("install-multiple", "a.apk", "b.apk")):  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
            self.trip.unlink(missing_ok=True)
            p = self.guard("adb", "-s", SERIAL, *args)
            self.assertEqual(p.returncode, 97, args)
        self.assertEqual(self.real(), [])

    def test_G3_the_iphone_app_is_never_uninstalled(self):
        self.assertEqual(self.guard("xcrun", "devicectl", "device", "install", "app", "--device", IDENT, "A.app").returncode, 0)  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        p = self.guard("xcrun", "devicectl", "device", "uninstall", "app", "--device", IDENT, "dev.richos.connect")  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal
        self.assertEqual(p.returncode, 97)
        self.assertEqual(self.real(), [f"xcrun devicectl device install app --device {IDENT} A.app"])  # device-cli-exempt: a fake phone or the guard under test; plants the command to prove the refusal


class Busy(unittest.TestCase):
    def test_B1_who_counts_as_using_the_phone(self):
        procs = [(1, 0, "/sbin/launchd"), (10, 1, "claude"), (20, 10, "python3 watch.py run --repo /r"),
                 (21, 20, f"randroid device perf --serial {SERIAL}"), (22, 21, f"adb -s {SERIAL} shell am start"),
                 (30, 10, f"python3 phone-android.py tap --serial {SERIAL} Send"),
                 (40, 1, "adb -L tcp:5037 fork-server server --reply-fd 4"),
                 (50, 10, "adb -s emulator-5554 shell getprop"), (60, 10, "adb shell input keyevent 3"),
                 (70, 1, "/Library/Developer/PrivateFrameworks/CoreDevice.framework/CoreDeviceService")]
        users = watch.users("android", [SERIAL], procs=procs, me=20)
        self.assertEqual([pid for pid, _ in users], [30, 60], "another agent's phone tool and a serial-less adb "
                                                              "client; not this run, not the adb server, not an emulator")
        self.assertEqual(users[0][1], "python3 phone-android.py tap --serial <phone> Send")
        self.assertEqual(watch.users("ios", [IDENT], procs=procs, me=20), [])

    def test_B2_a_free_phone_is_measured_at_once_and_a_busy_one_is_waited_for(self):
        slept = []
        free, waited = watch.wait_until_free("android", [SERIAL], sample=lambda: [],
                                             clock=lambda: 0, sleep=slept.append,
                                             log=lambda s: None)
        self.assertTrue(free)
        self.assertEqual(waited, 0, "one sample with no user is enough")
        self.assertEqual(slept, [], "a free phone is not waited for")
        samples = iter([[(30, "x")], [(30, "x")], []])
        slept = []
        free, waited = watch.wait_until_free("android", [SERIAL], sample=lambda: next(samples),
                                             clock=iter(range(0, 100, 10)).__next__, sleep=slept.append,
                                             log=lambda s: None)
        self.assertTrue(free)
        self.assertEqual(waited, 30, "free at the first sample with no user")
        self.assertEqual(len(slept), 2, "polled while busy, interrupting nothing")


if __name__ == "__main__":
    unittest.main(verbosity=2)
