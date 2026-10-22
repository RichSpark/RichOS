#!/usr/bin/env python3
"""mobile-device.test.py — a physical phone is touched only through `randroid device` / `rios device`,
only the release build goes on it, and nothing uninstalls the app or clears its data (CEO 2026-10-02,
2026-10-01). The real `randroid` runs against a scripted adb and a scripted aapt2 that keep a fake
phone's state in a JSON file; the commit check (`physical.py scan`) runs over planted files in a
scratch git repository. No phone, emulator, build or window.
"""
import hashlib
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
RANDROID = REPO / "richos/mobile/native-android/bin/randroid"
PHYSICAL = REPO / "richos/mobile/physical.py"
sys.path.insert(0, str(REPO / "richos/mobile"))
import physical  # noqa: E402

failures = []
names = []


def case(name):
    def wrap(fn):
        names.append(name)
        try:
            fn()
            print(f"  ok    {name}")
        except Exception as e:  # noqa: BLE001 — every failure is reported by name
            failures.append(name)
            print(f"  FAIL  {name}: {type(e).__name__}: {e}")
        return fn
    return wrap


# A scripted adb: the fake phone is a JSON file (emulator or not, the installed APK's bytes and
# whether it is debuggable, whether the phone refuses an install). Every call is logged.
FAKE_ADB = r'''#!/usr/bin/env python3
import hashlib, json, os, sys
state_path = os.environ["FAKE_PHONE"]
state = json.load(open(state_path))
args = sys.argv[1:]
with open(state_path + ".log", "a") as log:
    log.write(" ".join(args) + "\n")
if args[:1] == ["devices"]:
    print("List of devices attached\n" + state["serial"] + "\tdevice"); sys.exit(0)
if args[:2] != ["-s", state["serial"]]:
    print("error: device '%s' not found" % (args[1] if len(args) > 1 else ""), file=sys.stderr); sys.exit(1)
verb, rest = args[2], args[3:]
def save():
    json.dump(state, open(state_path, "w"))
if verb == "shell":
    cmd = " ".join(rest)
    if cmd.startswith("getprop ro.kernel.qemu"):
        flags = "DEBUGGABLE HAS_CODE" if state.get("debuggable") else "HAS_CODE ALLOW_CLEAR_USER_DATA"
        print("1" if state.get("emulator") else "")
        print("")
        print("--richos--")
        if state.get("sha"):
            print("    versionName=%s" % state.get("version", "1.0.0"))
            print("    versionCode=7 minSdk=26 targetSdk=36")
            print("    pkgFlags=[ %s ]" % flags)
        sys.exit(0)
    if cmd.startswith("pm path "):
        if state.get("sha"):
            print("package:/data/app/~~x/dev.richos.connect-1/base.apk")
        sys.exit(0)
    if cmd.startswith("sha256sum /data/app"):
        print("%s  /data/app/~~x/dev.richos.connect-1/base.apk" % state.get("sha")); sys.exit(0)
    if cmd.startswith("screenrecord"):
        state["recorded"] = cmd; save(); sys.exit(0)
    if cmd.startswith("rm -f"):
        sys.exit(0)
    for prefix, text in state.get("replies", {}).items():
        if cmd.startswith(prefix):
            print(text); sys.exit(0)
    sys.exit(0)
if verb == "install":
    if state.get("refuse"):
        print("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]", file=sys.stderr); sys.exit(1)
    data = open(rest[-1], "rb").read()
    state["sha"] = hashlib.sha256(data).hexdigest()
    state["debuggable"] = b"DEBUGGABLE" in data
    save(); print("Performing Streamed Install\nSuccess"); sys.exit(0)
if verb == "pull":
    open(rest[1], "wb").write(b"\x00\x00\x00\x18ftypmp42 fake video"); sys.exit(0)
print("unexpected adb call: " + " ".join(args), file=sys.stderr)
sys.exit(1)
'''

# aapt2's own reading of the manifest, scripted: an APK whose bytes say DEBUGGABLE is debuggable.
FAKE_AAPT2 = r'''#!/usr/bin/env python3
import sys
data = open(sys.argv[-1], "rb").read()
name = "dev.richos.other" if b"OTHER" in data else "dev.richos.connect" if b"CEOAPP" in data else "dev.richos.connect.perf"
print("package: name='%s' versionCode='7' versionName='1.0.0'" % name)
print("application-label:'RichConnect'")
if b"DEBUGGABLE" in data:
    print("application-debuggable")
'''


class Phone:
    def __init__(self, tmp, **state):
        self.tmp = Path(tmp)
        self.bin = self.tmp / "bin"
        self.bin.mkdir(exist_ok=True)
        for name, body in (("adb", FAKE_ADB), ("aapt2", FAKE_AAPT2)):
            (self.bin / name).write_text(body)
            (self.bin / name).chmod(0o755)
        self.file = self.tmp / "phone.json"
        self.file.write_text(json.dumps({"serial": "PHONE1", **state}))
        self.release = self.apk("release.apk", b"release bytes")
        self.debug = self.apk("app-debug.apk", b"debug bytes DEBUGGABLE")

    def apk(self, name, data):
        path = self.tmp / name
        path.write_bytes(data)
        return str(path)

    @property
    def state(self):
        return json.loads(self.file.read_text())

    @property
    def calls(self):
        log = Path(str(self.file) + ".log")
        return log.read_text().splitlines() if log.exists() else []

    def randroid(self, *args, env=None):
        e = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", "FAKE_PHONE": str(self.file),
             "RANDROID_CACHE": str(self.tmp / "cache"), "ANDROID_HOME": str(self.tmp / "sdk"),
             "RANDROID_AAPT2": str(self.bin / "aapt2"), "PYTHONDONTWRITEBYTECODE": "1",
             "RICHOS_PHONE_LOCK_DIR": str(self.tmp / "locks"), "RICHOS_DEVICE_HOLDER": "test-runner"}
        e.pop("RICHOS_DEVICE_VERB", None)
        e.update(env or {})
        p = subprocess.run(["bash", str(RANDROID), *args], capture_output=True, text=True, env=e, timeout=120)
        return p.returncode, p.stdout + p.stderr


def touched(calls):
    """The calls that change the phone: an install, an uninstall, a data clear."""
    return [c for c in calls if " install" in f" {c.split(' ', 2)[-1]}" or "uninstall" in c or "pm clear" in c]  # device-cli-exempt: reads the scripted adb's log for the calls that must not happen


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


# -- install: release only, over the app, never an uninstall ------------------------------------

@case("D1 a debuggable APK is refused on a physical phone, with the sentence; nothing is installed")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64)
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", ph.debug)
        assert code == 3, (code, out)
        assert "DEBUGGABLE build" in out and "only the build users get (release)" in out, out
        assert touched(ph.calls) == [] and ph.state["sha"] == "a" * 64, ph.calls


@case("D2 the release APK goes over the installed app with install -r (data kept), is read back, and is stamped for device perf")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64)
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", ph.release)
        assert code == 0, (code, out)
        installs = touched(ph.calls)
        assert installs == [f"-s PHONE1 install -r {ph.release}"], installs
        assert ph.state["sha"] == sha(ph.release) and not ph.state["debuggable"], ph.state
        assert '"configuration": "release"' in out, out
        stamp = json.loads(Path(ph.release + ".stamp.json").read_text())
        assert stamp["sha256"] == sha(ph.release), stamp
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", ph.release)
        assert code == 0 and '"install": "skipped"' in out and len(touched(ph.calls)) == 1, (code, out)


@case("D3 a phone that refuses the install (another signer) is reported, and NOTHING is uninstalled")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="b" * 64, debuggable=True, refuse=True)
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", ph.release)
        assert code == 3 and "NOTHING was uninstalled" in out and "CEO's decision" in out, (code, out)
        assert not any("uninstall" in c or "pm clear" in c for c in ph.calls), ph.calls
        assert ph.state["sha"] == "b" * 64


@case("D4 an APK that is not RichConnect is refused before the phone is touched")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp)
        other = ph.apk("other.apk", b"OTHER release")
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", other)
        assert code == 3 and "not the RichConnect test copy" in out and ph.calls == [], (code, out, ph.calls)
        # CEO 2026-10-03: his own app's APK is refused by name, before the phone is touched
        own = ph.apk("ceo.apk", b"CEOAPP release")
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", own)
        assert code == 3 and "CEO's own RichConnect" in out and "dev.richos.connect.perf" in out and ph.calls == [], (code, out, ph.calls)


@case("D5 an emulator is refused by randroid device (emulators go through randroid emu); no serial is refused")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, emulator=True)
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", ph.release)
        assert code == 3 and "randroid emu" in out and touched(ph.calls) == [], (code, out)
        code, out = ph.randroid("device", "build")
        assert code == 1 and "name the phone" in out, (code, out)


@case("D14 the grammar richos/mobile/perf/watch.py calls: --serial after the command; --expect-commit refuses another commit before the phone is touched")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64)
        code, out = ph.randroid("device", "install", "--serial", "PHONE1", "--expect-commit", "0000000", ph.release)
        assert code == 3 and "freshness mismatch" in out and "nothing was installed" in out, (code, out)
        assert touched(ph.calls) == [], ph.calls
        code, out = ph.randroid("device", "install", "--serial", "PHONE1", ph.release)
        assert code == 0 and ph.state["sha"] == sha(ph.release), (code, out)
        sys.path.insert(0, str(REPO / "richos/mobile/perf"))
        import perf  # noqa: E402
        args = perf.parse_args(["android", "--adb", "adb", "--serial", "S", "--kind", "physical", "--evidence-dir", "/x/e",
                                "--cold", "3", "--warm", "2", "--expect-commit", "abcdef1", "--out", "/x/r.json"])
        assert (args.evidence_dir, args.cold, args.warm, args.expect_commit) == ("/x/e", 3, 2, "abcdef1"), args


@case("D15 an unsigned release APK is refused by name before the phone is touched (a phone installs only a signed APK)")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64)
        unsigned = ph.apk("app-release-unsigned.apk", b"release bytes")
        code, out = ph.randroid("device", "--serial", "PHONE1", "install", unsigned)
        assert code == 3 and "unsigned" in out and "with-android-signing.py" in out and ph.calls == [], (code, out, ph.calls)


# -- the phone's condition: read only (CEO 2026-10-04) --------------------------------------------

CONDITION_REPLIES = {
    "settings get global window_animation_scale": "0.0",
    "settings get global stay_on_while_plugged_in": "7",
    "settings get system show_touches": "0",
    "date +%s": "1759576000\n3600.50 3500.00\n2.10 1.90 1.80 1/900 4242",
    "ps -A -o": "PID   PPID USER ELAPSED TIME NAME ARGS\n"
                "1       0 root 01:00:00 00:00:05 init /init\n"
                "300     1 shell 01:00:00 00:00:01 emdlogger emdlogger\n"
                "700     1 shell 00:50:00 00:00:09 adbd adbd --root_seclabel=u:r:su:s0\n"
                "900     1 shell 03:59:12 00:01:40 uiautomator app_process /system/bin com.android.commands.uiautomator.Launcher dump\n"
                "901   700 shell 00:40:00 00:00:00 sleep sleep 9999\n"
                "4242    1 system 01:00:00 00:10:00 system_server system_server\n"
                "5000   700 shell 00:00:00 00:00:00 ps ps -A -o PID,PPID,USER,ETIME,TIME,NAME,ARGS\n",
    "wm size": "Physical size: 1080x2412\nOverride size: 540x1200",
    "dumpsys battery": "Current Battery Service state:\n  (UPDATES STOPPED -- use 'reset' to restart)\n  AC powered: false",
    "dumpsys input": "Input Dispatcher State:\n  Connections:\n"
                     "    0: channelName='abc123 StatusBar (server)', windowName='StatusBar', status=NORMAL, monitor=false, responsive=true\n"
                     "    1: channelName='def456 dev.example/Main (server)', windowName='Main', status=NORMAL, monitor=false, responsive=false\n"
                     "    2: channelName='UiAutomation (server)', windowName='', status=NORMAL, monitor=true, responsive=true\n"
                     "    3: channelName='UiAutomation (server)', windowName='', status=NORMAL, monitor=true, responsive=true\n"
                     "\nInput Dispatcher State at time of last ANR:\n  ANR:\n    Time: 2026-09-10 10:32:48\n"
                     "    Reason: x is not responding.\n  Connections:\n"
                     "    9: channelName='aaa111 Old (server)', windowName='Old', status=NORMAL, monitor=false, responsive=false\n",
    "t=$(date +%s); echo since": "\n".join(["since 1759576000"] +
        [f" 1759576000.{i:03d}  4242  4300 E InputDispatcher: channel 'abc{i} Window' ~ Consumer closed input channel {i}"
         for i in range(60)] + [" 1759576001.000   900   901 I Other: once"]),
}


@case("D34 condition: READ ONLY (every call a read, nothing put, reset, unplugged or killed); finds an adb-started test tool, "
      "changed settings, a forced battery state, a stuck input connection and a repeated log line with its writer")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64, replies=CONDITION_REPLIES)
        out_dir = str(Path(tmp) / "raw")
        code, out = ph.randroid("device", "--serial", "PHONE1", "condition", "--seconds", "5", "--out", out_dir)
        assert code == 0, (code, out)
        r = json.loads(out)["result"]
        shells = [c.split(" ", 3)[3] for c in ph.calls if c.startswith("-s PHONE1 shell ")]
        reads = [c for c in shells if not c.startswith("getprop ro.kernel.qemu")]  # the gate's own probe
        assert reads and all(physical.read_only(c) for c in reads), [c for c in reads if not physical.read_only(c)]
        assert not any(w in c for c in shells for w in ("settings put", "--reset", "unplug", "logcat -c", "kill", "force-stop",
                                                          "input keyevent", "svc ", "wm size 5")), shells
        assert all(c.split()[1] == "PHONE1" for c in ph.calls), ph.calls  # never a bare adb
        # a test tool wherever it came from, and what adbd started and left; never adbd, this read's own ps,
        # or a shell-user daemon init started at boot (listed apart)
        assert [p["pid"] for p in r["leftoverProcesses"]] == [900, 901], r["leftoverProcesses"]
        assert "uiautomator" in r["leftoverProcesses"][0]["why"] and r["leftoverProcesses"][0]["elapsed"] == "03:59:12"
        assert "started over adb" in r["leftoverProcesses"][1]["why"], r["leftoverProcesses"][1]
        assert [p["pid"] for p in r["shellUserNotFromAdb"]] == [300], r["shellUserNotFromAdb"]
        assert set(r["settingsChanged"]) == {"global window_animation_scale=0.0", "global stay_on_while_plugged_in=7"}, r["settingsChanged"]
        assert r["batteryOverridden"] is True and "wm-size" in r["displayOverrides"], r
        assert len(r["input"]["notNormalOrUnresponsive"]) == 1, r["input"]
        assert r["input"]["duplicateChannels"] == {"UiAutomation (server)": 2}, r["input"]  # the last-ANR snapshot is not now
        assert r["input"]["lastAnr"][0] == "Time: 2026-09-10 10:32:48" and r["input"]["connections"] == 4, r["input"]
        top = r["log"]["top"][0]
        assert top["count"] == 60 and top["perSecond"] == 12.0 and top["tag"] == "InputDispatcher", top
        assert top["processes"] == ["system_server (system)"] and r["log"]["inputWindowLines"] == 60, r["log"]
        assert r["log"]["since"] == 1759576000 and r["log"]["bufferTurnedOver"] is False, r["log"]
        assert r["phoneClock"]["uptimeHours"] == 1.0 and r["ceoAppRunning"] is False, r["phoneClock"]
        assert Path(out_dir, "condition.json").is_file() and not Path(out_dir, "ceo-app-runs.txt").exists()
        # an emulator is refused before any read
        (Path(tmp) / "emu").mkdir()
        emu = Phone(Path(tmp) / "emu", emulator=True)
        code, out = emu.randroid("device", "--serial", "PHONE1", "condition", "--seconds", "5")
        assert code == 3 and "randroid emu" in out and len(emu.calls) == 1, (code, out, emu.calls)
    # the whole buffer's read: the rate is over the span its lines cover
    flood = physical.log_flood("\n".join(f" {150 + i}.000  1  1 W T: m {i}" for i in range(11)), None, {})
    assert flood["windowSeconds"] == 10.0 and flood["perSecond"] == 1.1 and flood["first"] == 150.0, flood
    for write in ("settings put global window_animation_scale 0", "dumpsys battery unplug", "dumpsys batterystats --reset",
                  "logcat -c", "wm size 540x1200", "settings get global x; svc power stayon true", "grep -E 'a|b'; am kill x"):
        assert not physical.read_only(write), write


@case("D34b condition lists a change a run recorded and never put back, by name and when, and restores nothing")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64, replies=CONDITION_REPLIES)
        rec = str(Path(tmp) / "records")
        env = {"RICHOS_PHONE_CHANGES_DIR": rec}
        os.environ["RICHOS_PHONE_CHANGES_DIR"] = rec
        try:
            sys.path.insert(0, str(Path(physical.__file__).parent / "perf"))
            import phone_changes
            phone_changes.begin("PHONE1", "battery", ["dumpsys battery reset"])
        finally:
            os.environ.pop("RICHOS_PHONE_CHANGES_DIR", None)
        code, out = ph.randroid("device", "--serial", "PHONE1", "condition", "--seconds", "5", env=env)
        assert code == 0, (code, out)
        found = json.loads(out[out.index("{"):])["result"]["pendingPhoneChanges"]
        assert len(found) == 1 and found[0].startswith("battery: recorded 20"), found
        assert not any("battery reset" in c for c in ph.calls), ph.calls  # listed, never undone
        assert len(list(Path(rec).glob("*.json"))) == 1


# -- every other command tests the release build only --------------------------------------------

@case("D6 build shows the installed build, debuggable or not: the way to see the phone's state is never refused")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="c" * 64, debuggable=True, version="1.0.0-dev")
        code, out = ph.randroid("device", "--serial", "PHONE1", "build")
        result = json.loads(out)["result"]
        assert code == 0 and result["debuggable"] is True and result["versionName"] == "1.0.0-dev", out
        assert result["configuration"] == "debug"


@case("D7 a walk step on a phone holding a debuggable build is refused before the step, naming the build and the way back")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="c" * 64, debuggable=True, version="1.0.0-dev")
        for step in (["tap", "Send message"], ["texts"], ["state", "--package", "dev.richos.connect"]):
            code, out = ph.randroid("device", "--serial", "PHONE1", *step)
            assert code == 3, (step, code, out)
            assert "DEBUGGABLE RichConnect" in out and "1.0.0-dev" in out and "device --serial PHONE1 install" in out, out
        assert not any(" input " in f" {c} " or "uiautomator" in c for c in ph.calls), ph.calls


@case("D8 a screen recording on the release build: screenrecord, pulled, removed from the phone; a debuggable build is refused first")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="d" * 64)
        out_file = str(Path(tmp) / "rec" / "cold.mp4")
        code, out = ph.randroid("device", "--serial", "PHONE1", "record", out_file, "--seconds", "5")
        assert code == 0 and Path(out_file).is_file(), (code, out)
        assert ph.state["recorded"].startswith("screenrecord --time-limit 5 "), ph.state
        assert any(c.endswith("rm -f /sdcard/richos-device-record.mp4") for c in ph.calls), ph.calls
        (Path(tmp) / "two").mkdir()
        ph2 = Phone(Path(tmp) / "two", sha="d" * 64, debuggable=True)
        code, out = ph2.randroid("device", "--serial", "PHONE1", "record", out_file)
        assert code == 3 and "recorded" not in ph2.state, (code, out)


@case("D9 perf and seed on a physical phone go through the verb; started any other way perf.py refuses")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="e" * 64)
        env = {**os.environ, "PATH": f"{ph.bin}:{os.environ['PATH']}", "FAKE_PHONE": str(ph.file)}
        env.pop("RICHOS_DEVICE_VERB", None)
        p = subprocess.run([sys.executable, str(REPO / "richos/mobile/perf/perf.py"), "android", "--adb", "adb",
                            "--serial", "PHONE1", "--kind", "physical", "--conversation", "as-installed"],
                           capture_output=True, text=True, env=env)
        assert p.returncode == 3 and "randroid device perf" in p.stderr and ph.calls == [], (p.returncode, p.stderr, ph.calls)
        # seed builds the release build's twin itself; without the upload key it cannot carry the
        # release signature, so it refuses (exit 3) with the sentence before the phone is touched
        from unittest.mock import patch
        with patch.dict(os.environ):
            for key in [k for k in os.environ if k.startswith("ORG_GRADLE_PROJECT_richos.upload.")]:
                del os.environ[key]
            code, out = ph.randroid("device", "--serial", "PHONE1", "seed")
        assert code == 3 and "debuggable twin must carry the upload key" in out and ph.calls == [], (code, out, ph.calls)


# -- one user per phone (CEO 2026-10-02) ---------------------------------------------------------
# Every holder below is a process this test started; its PID is the one recorded at spawn (the
# hold process) or the one its own command wrote (the held command), and only those are signaled.

def hold_cmd(phone, *cmd, wait=0, holder="agent-one", platform="android"):
    return [sys.executable, str(PHYSICAL), "hold", "--platform", platform, "--phone", phone, "--wait", str(wait),
            "--holder", holder, "--", *cmd]


class Holder:
    """A first user of a phone: `hold` running a command that writes its own PID, then sleeps."""

    def __init__(self, tmp, phone="P1", seconds=30, holder="agent-one", platform="android"):
        self.env = {**os.environ, "RICHOS_PHONE_LOCK_DIR": str(Path(tmp) / "locks"), "PYTHONDONTWRITEBYTECODE": "1"}
        self.flag = Path(tmp) / f"held-{holder}-{phone}"
        body = f"import os, pathlib, time; pathlib.Path({str(self.flag)!r}).write_text(str(os.getpid())); time.sleep({seconds})"
        self.proc = subprocess.Popen(hold_cmd(phone, sys.executable, "-c", body, holder=holder, platform=platform),
                                     env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        # The fact waited for: the held command wrote its PID, or the holder ended without doing so.
        while not (self.flag.exists() and self.flag.read_text()):
            if self.proc.poll() is not None:
                raise AssertionError(f"the first holder ended before its command ran: {self.proc.stderr.read()}")
            time.sleep(0.05)
        self.child = int(self.flag.read_text())

    def stop(self):
        for pid in (self.proc.pid, self.child):
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        self.proc.wait()  # SIGKILL always ends it; this only reaps it


def second(tmp, phone="P1", wait=0, cmd=("true",), holder="agent-two", platform="android"):
    env = {**os.environ, "RICHOS_PHONE_LOCK_DIR": str(Path(tmp) / "locks"), "PYTHONDONTWRITEBYTECODE": "1"}
    return subprocess.run(hold_cmd(phone, *cmd, wait=wait, holder=holder, platform=platform), env=env,
                          capture_output=True, text=True, timeout=900)  # load-bound: a hang guard only; no verdict rests on it


def lock_status(tmp, phone="P1", platform="android"):
    env = {**os.environ, "RICHOS_PHONE_LOCK_DIR": str(Path(tmp) / "locks")}
    p = subprocess.run([sys.executable, str(PHYSICAL), "status", "--platform", platform, "--phone", phone],
                       env=env, capture_output=True, text=True)
    return json.loads(p.stdout)["result"]


@case("D16 two users of one phone: the second is refused at once, naming the holder; status says held and by whom")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        first = Holder(tmp)
        try:
            p = second(tmp)
            assert p.returncode == 3, (p.returncode, p.stderr)
            assert "in use by agent-one" in p.stderr and f"process {first.proc.pid}" in p.stderr, p.stderr
            st = lock_status(tmp)
            assert st["state"] == "held" and st["holder"] == "agent-one" and st["pid"] == first.proc.pid, st
        finally:
            first.stop()


@case("D17 a second user given --wait waits for the first, says so, and goes on when the phone is free")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        first = Holder(tmp, seconds=2)
        try:
            p = second(tmp, wait=600)  # load-bound: the first ends after its 2 s sleep; 600 s only bounds a hang
            assert p.returncode == 0, (p.returncode, p.stderr)
            assert "waiting up to 600 s" in p.stderr and "held by agent-one" in p.stderr and "is free after" in p.stderr, p.stderr
            st = lock_status(tmp)
            assert st["state"] == "free" and st["previous"]["holder"] == "agent-two" and st["previous"]["endedAt"], st
        finally:
            first.stop()


@case("D18 a holder whose process is gone: its lock is free, status says so, and the next user takes it and records whose it was")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        first = Holder(tmp)
        first.stop()  # killed outright: no handler, no release of its own
        st = lock_status(tmp)
        assert st["state"] == "free" and "process is gone" in st["previous"]["note"], st
        p = second(tmp)
        assert p.returncode == 0 and "ended without releasing" in p.stderr and "agent-one" in p.stderr, (p.returncode, p.stderr)
        record = json.loads((Path(tmp) / "locks" / Path(lock_status(tmp)["record"]).name).read_text())
        assert record["reclaimedFrom"]["holder"] == "agent-one" and record["holder"] == "agent-two", record


@case("D19 one session, many commands: a device command inside `hold` of the same phone runs without waiting")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        inner = hold_cmd("P1", "true", holder="inner")
        p = second(tmp, cmd=inner, holder="walker")
        assert p.returncode == 0, (p.returncode, p.stderr)


@case("D20 two phones are two locks; the iPhone's spellings (hardware UDID, CoreDevice UUID) are one")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        first = Holder(tmp, phone="P1")
        try:
            assert second(tmp, phone="P2").returncode == 0
        finally:
            first.stop()
        iphone = Holder(tmp, phone="00008030-AAAA", platform="ios")
        try:
            p = second(tmp, phone="AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE", platform="ios")
            assert p.returncode == 3 and "iPhone is in use by agent-one" in p.stderr, p.stderr
        finally:
            iphone.stop()


@case("D21 randroid device refuses a walk step while another user holds the phone (nothing reaches it), and status names the holder")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="f" * 64)
        first = Holder(tmp, phone="PHONE1")
        try:
            code, out = ph.randroid("device", "--serial", "PHONE1", "tap", "Send message")
            assert code == 3 and "in use by agent-one" in out, (code, out)
            assert ph.calls == [], ph.calls
            code, out = ph.randroid("device", "--serial", "PHONE1", "status")
            result = json.loads(out)["result"]
            assert code == 0 and result["state"] == "held" and result["holder"] == "agent-one", out
        finally:
            first.stop()
        code, out = ph.randroid("device", "--serial", "PHONE1", "build")
        assert code == 0 and '"configuration": "release"' in out, (code, out)


# -- the test iPhone's network (CEO 2026-10-02: "Unable to Verify App" again) ---------------------
# A fake xcrun keeps a fake iPhone in a JSON file: while its network is off, launching RichConnect
# fails the way iOS does when it cannot verify the developer certificate online. Every call is logged.
FAKE_XCRUN = r'''#!/usr/bin/env python3
import json, os, sys
state_path = os.environ["FAKE_IPHONE"]
state = json.load(open(state_path))
args = sys.argv[1:]
with open(state_path + ".log", "a") as log:
    log.write(" ".join(args) + "\n")
if args[:4] == ["devicectl", "device", "process", "launch"]:
    if state.get("launch_error"):
        print(state["launch_error"], file=sys.stderr); sys.exit(1)
    if state["network"] == "off":
        print("ERROR: The application could not be launched because the Developer App Certificate is not trusted. (com.apple.dt.CoreDeviceError error 10002)", file=sys.stderr)
        sys.exit(1)
    json.dump({"result": {"process": {"processIdentifier": 4242}}}, open(args[args.index("--json-output") + 1], "w"))
    sys.exit(0)
if args[:4] == ["devicectl", "device", "process", "terminate"]:
    # The fake phone's processes ("procs": [[pid, path]]); only SIGKILL (--kill) ends AutomationModeUI, as measured.
    pid = int(args[args.index("--pid") + 1])
    if "--kill" in args and not state.get("ignores_kill"):
        state["procs"] = [p for p in state.get("procs", []) if p[0] != pid]
        json.dump(state, open(state_path, "w"))
    sys.exit(0)
if args[:4] == ["devicectl", "device", "info", "processes"]:
    if state.get("procs_error"):
        print(state["procs_error"], file=sys.stderr); sys.exit(1)
    rows = [{"processIdentifier": p[0], "executable": "file://" + p[1]} for p in state.get("procs", [])]
    json.dump({"result": {"runningProcesses": rows}}, open(args[args.index("--json-output") + 1], "w"))
    sys.exit(0)
if args[:4] == ["devicectl", "device", "info", "details"]:
    json.dump({"result": {"connectionProperties": {"tunnelState": "connected", "transportType": "wired",
               "localHostnames": [state.get("hostname", "")]}}}, open(args[args.index("--json-output") + 1], "w"))
    sys.exit(0)
if args[:3] == ["devicectl", "device", "reboot"]:
    # A restart puts the network back only when the fake phone says it would (2026-10-02 it did).
    state["reboots"] = state.get("reboots", 0) + 1
    if state.get("reboot_fixes"):
        state["network"] = "on"
    if state.get("reboot_fixes_wifi"):
        state["wifi"] = "on"
    json.dump(state, open(state_path, "w"))
    sys.exit(0)
print("unexpected xcrun call: " + " ".join(args), file=sys.stderr)
sys.exit(1)
'''


# The fake phone's Wi-Fi path, as this Mac sees it (phone_net.wifi_path): its address lookup answers with a
# loopback address (the sync port refuses at once) and `ping` answers unless the fake phone's "wifi" is "dead".
FAKE_DSCACHEUTIL = "#!/bin/sh\nprintf 'ip_address: 127.0.0.1\\n'\n"
FAKE_PING = r'''#!/usr/bin/env python3
import json, os, sys
if json.load(open(os.environ["FAKE_IPHONE"])).get("wifi") == "dead":
    print("3 packets transmitted, 0 packets received, 100.0% packet loss"); sys.exit(2)
print("3 packets transmitted, 3 packets received, 0.0% packet loss")
'''


class IPhone:
    def __init__(self, tmp, network="on", launch_error=None):
        self.tmp = Path(tmp)
        (self.tmp / "bin").mkdir(exist_ok=True)
        for name, body in (("xcrun", FAKE_XCRUN), ("dscacheutil", FAKE_DSCACHEUTIL), ("ping", FAKE_PING)):
            (self.tmp / "bin" / name).write_text(body)
            (self.tmp / "bin" / name).chmod(0o755)
        self.file = self.tmp / "iphone.json"
        import phone_net
        self.set(network=network, launch_error=launch_error, hostname="TESTPHONE" + phone_net.COREDEVICE_SUFFIX)
        self.env = {**os.environ, "PATH": f"{self.tmp / 'bin'}:{os.environ['PATH']}", "FAKE_IPHONE": str(self.file),
                    "RICHOS_PHONE_LOCK_DIR": str(self.tmp / "locks"), "PYTHONDONTWRITEBYTECODE": "1",
                    "RICHOS_IOS_DEVICE": "00000000-0000000000000000", "RICHOS_DEVICE_HOLDER": "net-test",
                    "RICHOS_PHONE_REBOOT_SETTLE": "0", "RICHOS_PHONE_REBOOT_STEP": "0",
                    "RICHOS_PHONE_NET_PROBE": "echo {}"}
        self.env.pop("RICHOS_PHONE_NET_RESTORE", None)
        self.env.pop("RICHOS_PHONE_CHECK_APP", None)

    @property
    def reboots(self):
        return json.loads(self.file.read_text()).get("reboots", 0)

    def set(self, **state):
        old = json.loads(self.file.read_text()) if self.file.exists() else {}
        self.file.write_text(json.dumps({**old, **state}))

    @property
    def network(self):
        return json.loads(self.file.read_text())["network"]

    def launches(self):
        log = Path(str(self.file) + ".log")
        return [c for c in (log.read_text().splitlines() if log.exists() else []) if " process launch " in f" {c} "]

    def hold(self, body, restore=None):
        """`physical.py hold --net-check` running python `body` (which may change the fake phone's network)."""
        env = dict(self.env)
        if restore:
            env["RICHOS_PHONE_NET_RESTORE"] = restore
        return subprocess.run([sys.executable, str(PHYSICAL), "hold", "--platform", "ios", "--phone", "iphone", "--net-check",
                               "--", sys.executable, "-c", body], env=env, capture_output=True, text=True, timeout=300)


def phone_sets(file, network):
    return f"import json; p={str(file)!r}; s=json.load(open(p)); s['network']={network!r}; json.dump(s, open(p, 'w'))"


@case("D22 the phone cannot reach Apple even after one restart: rios device refuses up front with one sentence, runs nothing, and releases the phone")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp, network="off")
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 3 and not flag.exists(), (p.returncode, p.stderr)
        for word in ("cannot reach Apple", "rios device reboot", "rios device trust", "no Verify App", "Wi-Fi", "Tailscale"):
            assert word in p.stderr, (word, p.stderr)
        assert ph.reboots == 1, ph.reboots          # once, never a loop
        assert len(ph.launches()) == 2, ph.launches()   # before the restart, and once after it
        env = {**ph.env}
        st = json.loads(subprocess.run([sys.executable, str(PHYSICAL), "status", "--platform", "ios", "--phone", "iphone"],
                                       env=env, capture_output=True, text=True).stdout)["result"]
        assert st["state"] == "free", st


@case("D22b the phone cannot verify the app and a restart clears it: rios device restarts it once on its own (no step asks anyone) and the run goes on")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp, network="off")
        ph.set(reboot_fixes=True)
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 0 and flag.exists(), (p.returncode, p.stderr)
        assert ph.reboots == 1 and "restarting it once" in p.stderr and "the run goes on" in p.stderr, (ph.reboots, p.stderr)
        calls = Path(str(ph.file) + ".log").read_text()
        assert "devicectl device reboot --device 00000000-0000000000000000 --wait-for-device" in calls, calls


@case("D22c rios device reboot: restarts the phone through devicectl and waits for it; it opens NO app, whatever the app check says (CEO 2026-10-04)")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp, network="off")
        ph.set(reboot_fixes=True)
        env = {**ph.env, "RICHOS_DEVICE_VERB": "rios"}
        script = REPO / "richos/app/scripts/qa/phone-ios.py"
        p = subprocess.run([sys.executable, str(script), "reboot", "--device", "00000000-0000000000000000"],
                           env=env, capture_output=True, text=True, timeout=120)
        result = json.loads(p.stdout)
        assert p.returncode == 0 and result["state"] == "ok" and result["rebooted"] and ph.reboots == 1, (p.returncode, p.stdout, p.stderr)
        assert ph.launches() == [] and result["launchTries"] == 0, (ph.launches(), result)
        # Even a phone that would refuse the app: reboot never asks it to open anything.
        ph.set(network="off", reboot_fixes=False)
        p = subprocess.run([sys.executable, str(script), "reboot", "--device", "00000000-0000000000000000"],
                           env=env, capture_output=True, text=True, timeout=120)
        assert ph.launches() == [], ph.launches()
        # Without the rios verb it touches nothing.
        env.pop("RICHOS_DEVICE_VERB")
        reboots_before = ph.reboots
        p = subprocess.run([sys.executable, str(script), "reboot", "--device", "00000000-0000000000000000"],
                           env=env, capture_output=True, text=True, timeout=120)
        assert p.returncode == 3 and ph.reboots == reboots_before, (p.returncode, ph.reboots)


@case("D23 the phone is online: the app is opened once and closed again before the run, and the run goes on")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 0 and flag.exists(), (p.returncode, p.stderr)
        calls = Path(str(ph.file) + ".log").read_text()
        assert "--terminate-existing" in calls and "dev.richos.connect" in calls and "process terminate" in calls and "--pid 4242" in calls, calls


@case("D23b RICHOS_PHONE_CHECK_APP=none: the checks around a run open no app at all (the CEO's own app is his, 2026-10-03)")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        ph.env["RICHOS_PHONE_CHECK_APP"] = "none"
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 0 and flag.exists(), (p.returncode, p.stderr)
        assert ph.launches() == [] and "no app opened" in p.stderr, (ph.launches(), p.stderr)
        env = {**ph.env, "RICHOS_DEVICE_VERB": "rios"}
        p = subprocess.run([sys.executable, str(REPO / "richos/app/scripts/qa/phone-ios.py"), "reboot", "--device",
                            "00000000-0000000000000000"], env=env, capture_output=True, text=True, timeout=120)
        result = json.loads(p.stdout)
        assert p.returncode == 0 and result["state"] == "ok" and result["rebooted"] and result["launchTries"] == 0, \
            (p.returncode, p.stdout, p.stderr)
        assert ph.reboots == 1 and ph.launches() == [], (ph.reboots, ph.launches())
        # Another bundle is opened instead of RichConnect when one is named.
        ph.env["RICHOS_PHONE_CHECK_APP"] = "dev.richos.test.app"
        p = ph.hold("pass")
        assert p.returncode == 0 and ph.launches() and all("dev.richos.test.app" in c and "dev.richos.connect" not in c
                                                              for c in ph.launches()), ph.launches()


@case("D24 a run that leaves the phone's network off: it is checked again after the run and put back, and the run's exit stands")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        put_back = f"{sys.executable} -c \"{phone_sets(ph.file, 'on')}\""
        p = ph.hold(phone_sets(ph.file, "off"), restore=put_back)
        assert p.returncode == 0 and ph.network == "on", (p.returncode, ph.network, p.stderr)
        assert "left its network off" in p.stderr and "network is back" in p.stderr, p.stderr
        assert len(ph.launches()) == 3, ph.launches()   # before, after, once more after the restore


@case("D25 a run that leaves the network off and cannot put it back: exit 4 with the sentence (the run's own failure code is kept)")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        p = ph.hold(phone_sets(ph.file, "off"), restore=f"{sys.executable} -c pass")
        assert p.returncode == 4 and "cannot reach Apple" in p.stderr and "After the run" in p.stderr, (p.returncode, p.stderr)
        ph.set(network="on")
        p = ph.hold(phone_sets(ph.file, "off") + "; raise SystemExit(7)", restore=f"{sys.executable} -c pass")
        assert p.returncode == 7, (p.returncode, p.stderr)


@case("D26 an answer that is not about the network (locked phone) is said and does not refuse the run")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp, launch_error="ERROR: The device is locked. Unlock the device and try again.")
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 0 and flag.exists() and "could not decide" in p.stderr, (p.returncode, p.stderr)


@case("D27 rios sends verify, run, perf and hold through the check, and the phone's read-only commands around it never launch the app")
def _():
    text = (REPO / "richos/mobile/native-ios/bin/rios").read_text()
    assert 'HOLDN=(python3 "$PHYS" hold --platform ios --phone iphone --net-check' in text
    for verb in ('verify) exec "${HOLDN[@]}"', 'run) exec "${HOLDN[@]}"', 'exec "${HOLDN[@]}" python3 "$HERE/../perf/perf.py"'):
        assert verb in text, verb
    # `trust` is the diagnosis of a refused check, so it runs without the check (which would refuse it).
    # `reboot` is the remedy the check itself applies, so it too runs without the check, holding the phone.
    # `launch` opens the app on purpose: it holds the phone but runs its own Wi-Fi-first, launch-once procedure
    # instead of the trust check, so it sits beside the read-only verbs on the HOLD line and is the only one there that launches.
    assert 'approval|procs|apps|lock|battery|syslog|trust|net|close|wifi-restore|wifi-on|wifi-off|reboot|launch) exec "${HOLD[@]}"' in text


@case("D27b a phone joined to Wi-Fi whose Wi-Fi carries no traffic is restarted once BEFORE anything is opened, so iOS never refuses the app for want of a network")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        ph.set(wifi="dead", reboot_fixes_wifi=True)
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 0 and flag.exists(), (p.returncode, p.stderr)
        assert ph.reboots == 1 and "carries no traffic" in p.stderr, (ph.reboots, p.stderr)
        calls = Path(str(ph.file) + ".log").read_text().splitlines()
        first_reboot = next(i for i, c in enumerate(calls) if " reboot " in f" {c} ")
        assert not any(" process launch " in f" {c} " for c in calls[:first_reboot]), calls
        ph.set(wifi="dead", reboot_fixes_wifi=False)
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).unlink()")
        assert p.returncode == 3 and flag.exists() and ph.reboots == 2, (p.returncode, p.stderr)


AUTOMATION_UI = "/System/Library/PrivateFrameworks/AutomationMode.framework/AutomationModeUI.app/AutomationModeUI"
OTHER_PROCS = [[455, "/private/var/containers/Bundle/Application/X/RichOSNative.app/RichOSNative"],
               [553, "/usr/libexec/testmanagerd"],
               [71, "/usr/libexec/backboardd"]]


def terminations(ph):
    log = Path(str(ph.file) + ".log")
    return [c for c in (log.read_text().splitlines() if log.exists() else []) if " process terminate " in f" {c} "]


@case("D35 ios-automation-end: iOS's AutomationModeUI (which keeps one stuck input client per UI-test session, 2026-10-04) is ended with SIGKILL and nothing else on the phone is touched; still running afterwards is an error")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        ph.set(procs=OTHER_PROCS + [[555, AUTOMATION_UI]])
        run = lambda env: subprocess.run([sys.executable, str(PHYSICAL), "ios-automation-end", "--device",
                                          "00000000-0000000000000000"], env=env, capture_output=True, text=True, timeout=60)
        p = run({**ph.env, "RICHOS_DEVICE_VERB": "rios"})
        assert p.returncode == 0 and json.loads(p.stdout)["result"] == {"ended": [555]}, (p.returncode, p.stdout, p.stderr)
        assert terminations(ph) == ["devicectl device process terminate --pid 555 --kill --device 00000000-0000000000000000 "
                                    "--json-output " + terminations(ph)[0].split("--json-output ")[1]], terminations(ph)
        assert sorted(p[0] for p in json.loads(ph.file.read_text())["procs"]) == [71, 455, 553]
        # Not running: nothing is ended, and that is fine.
        p = run({**ph.env, "RICHOS_DEVICE_VERB": "rios"})
        assert p.returncode == 0 and json.loads(p.stdout)["result"] == {"ended": []} and len(terminations(ph)) == 1
        # It survives SIGKILL: an error naming it, never a silent pass.
        ph.set(procs=OTHER_PROCS + [[556, AUTOMATION_UI]], ignores_kill=True)
        p = run({**ph.env, "RICHOS_DEVICE_VERB": "rios"})
        assert p.returncode == 2 and "pid 556" in p.stderr and "still running" in p.stderr, (p.returncode, p.stderr)
        # Only through `rios device`.
        p = run(ph.env)
        assert p.returncode == 3 and "rios device" in p.stderr, (p.returncode, p.stderr)


@case("D35b every hold of the iPhone ends AutomationModeUI before its command (a run killed outright left it) and after it, however the command ended, and keeps the command's exit")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        ph.set(procs=OTHER_PROCS + [[555, AUTOMATION_UI]])   # left by an earlier session nobody ended
        seen = Path(tmp) / "seen.json"
        # The command records what it found on the phone, starts a UI-test session (AutomationModeUI comes back) and fails.
        body = (f"import json; p={str(ph.file)!r}; s=json.load(open(p)); open({str(seen)!r}, 'w').write(json.dumps(s['procs'])); "
                f"s['procs'].append([600, {AUTOMATION_UI!r}]); json.dump(s, open(p, 'w')); raise SystemExit(7)")
        p = ph.hold(body)
        assert p.returncode == 7, (p.returncode, p.stderr)
        assert [555, AUTOMATION_UI] not in json.loads(seen.read_text()), "the command ran on a phone still holding the stuck clients"
        assert [t.split("--pid ")[1].split()[0] for t in terminations(ph) if "--kill" in t] == ["555", "600"], terminations(ph)
        assert all(p[1] != AUTOMATION_UI for p in json.loads(ph.file.read_text())["procs"])
        assert "before the run: ended iOS's AutomationModeUI (pid 555)" in p.stderr and "after the run: ended" in p.stderr, p.stderr


@case("D35c the hold cannot read the phone's processes: the run is refused before it starts (it could be timing a clogged phone), and the phone is released")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = IPhone(tmp)
        ph.set(procs_error="ERROR: The operation couldn't be completed.")
        flag = Path(tmp) / "ran"
        p = ph.hold(f"import pathlib; pathlib.Path({str(flag)!r}).write_text('x')")
        assert p.returncode == 3 and not flag.exists() and "stuck input clients" in p.stderr, (p.returncode, p.stderr)
        st = json.loads(subprocess.run([sys.executable, str(PHYSICAL), "status", "--platform", "ios", "--phone", "iphone"],
                                       env=ph.env, capture_output=True, text=True).stdout)["result"]
        assert st["state"] == "free", st


def load_phone_ios():
    import importlib.util
    spec = importlib.util.spec_from_file_location("phone_ios_net", REPO / "richos/app/scripts/qa/phone-ios.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FakeSettings:
    """A fake iPhone's Settings behind phone-ios.py's `_run`: runs a step list the way the phone would."""

    def __init__(self, module, wifi="1", raise_after_first=None, stuck=False):
        self.module, self.wifi, self.runs, self.raise_after_first, self.stuck = module, wifi, 0, raise_after_first, stuck

    def __call__(self, args):
        args.phone_touched = True
        self.runs += 1
        steps = json.loads(Path(args.steps).read_text())
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        rows, failed = [], None
        for i, step in enumerate(steps):
            if step["do"] == "mark" and step.get("label") == "STOP":
                rows.append({"i": i, "do": "mark", "ok": False, "error": "the list stopped here"})
                failed = i
                break
            # iOS 26.3.1's Wi-Fi page (measured 2026-10-03): a StaticText labeled "Wi‑Fi" sits above the switch
            # of the same label, so only a step naming the switch's kind reaches the switch; the label alone
            # reaches the text, whose value is "" and whose tap toggles nothing.
            on_switch = step.get("label") == self.module.WIFI_SWITCH and step.get("kind") == "switch"
            if step["do"] == "tap" and on_switch and not self.stuck:
                self.wifi = "0" if self.wifi == "1" else "1"
            detail = {"value": self.wifi if on_switch else ""} if step["do"] == "value" else {}
            rows.append({"i": i, "do": step["do"], "ok": True, "detail": detail})
        (out / "steps.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
        if self.raise_after_first and self.runs == 1:
            raise self.module.CannotAnswer(self.raise_after_first)
        return {"passed": failed is None, "error": None if failed is None else "step failed"}


def wifi_list(tmp, *tail):
    sw = "Wi‑Fi"
    steps = [{"do": "launch", "in": "settings"}, {"do": "tap", "label": "Wi-Fi", "in": "settings"},
             {"do": "tap", "kind": "switch", "label": sw, "in": "settings"}, *tail]
    path = Path(tmp) / "list.json"
    path.write_text(json.dumps(steps))
    return path


def phone_run(module, fake, path, tmp):
    from types import SimpleNamespace
    import contextlib
    import io
    args = SimpleNamespace(steps=str(path), out=str(Path(tmp) / "out"), allowance=240, prebuilt=False, stamp=None,
                           approval_announced=False)
    out, err = io.StringIO(), io.StringIO()
    from unittest.mock import patch
    # On the cable unless the test says otherwise (a list that presses the Wi-Fi switch is refused off it).
    cabled = getattr(fake, "props", {"tunnelState": "connected", "transportType": "wired"})
    with patch.object(module, "_run", fake), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err), \
            patch.object(module, "devicectl", lambda a, d: {"connectionProperties": cabled}), \
            patch.dict(os.environ, {"RICHOS_IOS_DEVICE": "00000000-0000000000000000"}):
        try:
            code = module.run(args)
            return code, (json.loads(out.getvalue()) if out.getvalue() else None), None
        except Exception as error:  # noqa: BLE001: the test reads which error left the run
            return None, None, error


@case("D27b battery and syslog give libimobiledevice the hardware UDID, not the CoreDevice UUID rios passes")
def _():
    module = load_phone_ios()
    core, hw = "AAAAAAAA-1111-2222-3333-444455556666", "0000FFFF-0123456789ABCDEF"
    seen = []

    def fake_run(cmd, **kw):
        seen.append(cmd)
        out = "BatteryCurrentCapacity: 80\nBatteryIsCharging: true\n" if cmd[0] == "ideviceinfo" else ""
        return subprocess.CompletedProcess(cmd, 0 if cmd[2] == hw else 1, out, "")

    old_ids, old_run = module.device_ids, module.subprocess.run
    module.device_ids = lambda d: {d.lower(), hw.lower()}
    module.subprocess.run = fake_run
    try:
        args = type("A", (), {"device": core, "network": False})()
        try:
            module.battery(args)
        except Exception:
            pass
    finally:
        module.device_ids, module.subprocess.run = old_ids, old_run
    assert seen and seen[0][:3] == ["ideviceinfo", "-u", hw], seen


@case("D28 a list that switches Wi-Fi off and stops before switching it back: the run reads the switch and turns it on")
def _():
    module = load_phone_ios()
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeSettings(module)
        code, summary, error = phone_run(module, fake, wifi_list(tmp, {"do": "mark", "label": "STOP"}), tmp)
        assert fake.wifi == "1", "the phone's Wi-Fi was left off"
        wifi = summary["wifiRestore"]
        assert wifi["before"] == "0" and wifi["turnedOn"] is True and wifi["error"] is None, wifi
        assert fake.runs == 3, fake.runs   # the list, the read, the turn-on
        assert code == 1 and summary["passed"] is False, (code, summary)


@case("D29 a list that fails outright (the run raises) still gets the Wi-Fi put back, and the error goes on")
def _():
    module = load_phone_ios()
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeSettings(module, raise_after_first="the check produced no result bundle")
        code, summary, error = phone_run(module, fake, wifi_list(tmp), tmp)
        assert isinstance(error, module.CannotAnswer) and "no result bundle" in str(error), error
        assert fake.wifi == "1", "the phone's Wi-Fi was left off"


@case("D30 a list that put Wi-Fi back itself costs one read and no tap; a list that never touched Wi-Fi costs nothing")
def _():
    module = load_phone_ios()
    with tempfile.TemporaryDirectory() as tmp:
        sw = "Wi‑Fi"
        fake = FakeSettings(module)
        code, summary, error = phone_run(module, fake, wifi_list(tmp, {"do": "tap", "kind": "switch", "label": sw, "in": "settings"}), tmp)
        assert fake.wifi == "1" and summary["wifiRestore"]["turnedOn"] is False and summary["wifiRestore"]["error"] is None, summary
        assert fake.runs == 2 and code == 0, (fake.runs, code)   # the list and the read
        fake = FakeSettings(module)
        path = Path(tmp) / "plain.json"
        path.write_text(json.dumps([{"do": "launch"}, {"do": "tap", "id": "composer.send"}]))
        code, summary, error = phone_run(module, fake, path, tmp)
        assert fake.runs == 1 and summary["wifiRestore"] is None, (fake.runs, summary)


@case("D31 a Wi-Fi switch that will not turn on is said in the run's own result, never hidden")
def _():
    module = load_phone_ios()
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeSettings(module, wifi="0", stuck=True)
        code, summary, error = phone_run(module, fake, wifi_list(tmp), tmp)
        assert summary["wifiRestore"]["error"] and "still off" in summary["wifiRestore"]["error"], summary
        assert code == 1 and summary["passed"] is False and "Wi-Fi could not be confirmed on" in summary["error"], (code, summary)


@case("D31b the restore reads and taps the Wi-Fi SWITCH, not the text labeled Wi-Fi above it, and a reading that is not 0 or 1 is said, never taken as on")
def _():
    module = load_phone_ios()
    assert all(s.get("kind") == "switch" for s in module.wifi_steps(True) if s.get("label") == module.WIFI_SWITCH), \
        module.wifi_steps(True)
    from unittest.mock import patch
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeSettings(module, wifi="0")
        with patch.object(module, "_run", fake):
            result = module.ensure_wifi_on(Path(tmp) / "restore")
        assert fake.wifi == "1" and result["before"] == "0" and result["turnedOn"] is True, (fake.wifi, result)
        (Path(tmp) / "read").mkdir()
        (Path(tmp) / "read" / "steps.jsonl").write_text(json.dumps({"do": "value", "ok": True, "detail": {"value": ""}}) + "\n")
        assert module.wifi_value(Path(tmp) / "read") is None


@case("D31c a list that acts only in Safari, Settings or SpringBoard runs with the runner alone; one step on the app hands the app over as before")
def _():
    module = load_phone_ios()
    safari = [{"do": "open", "url": "https://www.apple.com"}, {"do": "sleep", "seconds": 2},
              {"do": "shot", "name": "s", "in": "safari", "screen": True}, {"do": "terminate", "in": "safari"},
              {"do": "launch", "in": "settings"}, {"do": "mark", "label": "x"}, {"do": "home"}]
    assert module.addresses_app(safari) is False
    for app_step in ({"do": "launch"}, {"do": "tap", "id": "composer.send"}, {"do": "shot", "name": "app"},
                     {"do": "state"}):
        assert module.addresses_app(safari + [app_step]) is True, app_step
    text = (REPO / "richos/mobile/native-ios/Tools/physical-device.mjs").read_text()
    assert "env.RICHOS_PHYSICAL_RUNNER_ONLY === '1'" in text and "t['UITargetAppPath']=t['TestHostPath']" in text


@case("D31d --as-installed hands the runner alone even for a list that acts on the app, so the installed build stays")
def _():
    module = load_phone_ios()
    app = [{"do": "launch"}, {"do": "tap", "id": "composer.send"}]
    assert module.runner_only_for(app) is False
    assert module.runner_only_for(app, as_installed=True) is True
    assert module.runner_only_for([{"do": "home"}]) is True
    text = (REPO / "richos/app/scripts/qa/phone-ios.py").read_text()
    assert "--as-installed hands the phone no app, so --prebuilt has nothing to reuse" in text


def wifi_off_call(module, fake, props, tmp):
    """`wifi-off`'s work (switch_wifi_off) against the fake Settings, with devicectl answering `props`.
    (code, result or refusal sentence, devicectl calls); code None when refused."""
    from unittest.mock import patch
    seen = []
    with patch.object(module, "_run", fake), \
            patch.object(module, "devicectl", lambda a, d: seen.append(a) or {"connectionProperties": props}):
        try:
            result = module.switch_wifi_off(Path(tmp) / f"off-{fake.runs}-{len(seen)}", "00000000-0000000000000000")
        except module.CannotAnswer as refused:
            return None, str(refused), seen
    return (0 if not result["error"] else 1), result, seen


@case("D32 wifi-off: refused over Wi-Fi (the Mac would lose the phone) with Settings never opened; over the cable it turns the switch off, and an off switch costs one read and no tap")
def _():
    module = load_phone_ios()
    wired = {"tunnelState": "connected", "transportType": "wired"}
    with tempfile.TemporaryDirectory() as tmp:
        for props in ({"tunnelState": "connected", "transportType": "localNetwork"}, {"tunnelState": "disconnected"}, {}):
            fake = FakeSettings(module)
            code, said, seen = wifi_off_call(module, fake, props, tmp)
            assert code is None and "not touched" in said and fake.runs == 0 and fake.wifi == "1", (props, said, fake.runs)
        assert "USB cable" in wifi_off_call(module, FakeSettings(module), {"tunnelState": "connected", "transportType": "localNetwork"}, tmp)[1]
        fake = FakeSettings(module)
        code, result, seen = wifi_off_call(module, fake, wired, tmp)
        assert code == 0 and fake.wifi == "0" and result["before"] == "1" and result["turnedOff"] is True and fake.runs == 2, (code, result)
        assert seen == [["device", "info", "details"]], seen
        code, result, _ = wifi_off_call(module, fake, wired, tmp)
        assert code == 0 and fake.wifi == "0" and result["turnedOff"] is False and fake.runs == 3, (code, result, fake.runs)
        stuck = FakeSettings(module, stuck=True)
        code, result, _ = wifi_off_call(module, stuck, wired, tmp)
        assert code == 1 and "still on" in result["error"], result
    text = (REPO / "richos/mobile/native-ios/bin/rios").read_text()
    assert "wifi-on|wifi-off|reboot|launch) exec \"${HOLD[@]}\"" in text


@case("D33 a `run` list that presses the Wi-Fi switch is refused off the cable before anything touches the phone; on the cable it runs, and a list that never presses it asks nothing")
def _():
    module = load_phone_ios()
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeSettings(module)
        fake.props = {"tunnelState": "connected", "transportType": "localNetwork"}
        code, summary, error = phone_run(module, fake, wifi_list(tmp), tmp)
        assert isinstance(error, module.CannotAnswer) and "USB cable" in str(error) and fake.runs == 0 and fake.wifi == "1", (error, fake.runs)
        fake = FakeSettings(module)
        code, summary, error = phone_run(module, fake, wifi_list(tmp, {"do": "tap", "kind": "switch", "label": "Wi‑Fi", "in": "settings"}), tmp)
        assert error is None and code == 0 and fake.wifi == "1", (error, code)
    assert module.switches_wifi([{"do": "tap", "kind": "switch", "label": "Wi‑Fi", "in": "settings"}])
    assert not module.switches_wifi([{"do": "tap", "label": "Wi-Fi", "in": "settings"}, {"do": "value", "kind": "switch", "label": "Wi‑Fi", "in": "settings"},
                                     {"do": "tap", "kind": "switch", "label": "Camera", "in": "settings"}])


# -- the commit check ---------------------------------------------------------------------------
# Each planted line is what a change might add; the scan must name it. The literals carry the
# exemption marker as a comment on THIS file's line, never inside the planted text.
PLANTED = {
    "richos/app/scripts/qa/new-tool.sh": 'adb -s "$SERIAL" install -r out/app-debug.apk\n',  # device-cli-exempt: planted, the scan must refuse it
    "richos/app/scripts/qa/wipe.py": 'subprocess.run(["adb", "-s", serial, "uninstall", "dev.richos.connect"])\n',  # device-cli-exempt: planted, the scan must refuse it
    "richos/app/scripts/qa/clear.sh": 'adb -s "$S" shell pm clear dev.richos.connect\n',  # device-cli-exempt: planted, the scan must refuse it
    "richos/mobile/native-android/bin/helper.sh": './gradlew :app:installDebug\n',  # device-cli-exempt: planted, the scan must refuse it
    "richos/app/scripts/qa/ios.sh": 'xcodebuild test -scheme X -destination "platform=iOS,id=$UDID"\n',  # device-cli-exempt: planted, the scan must refuse it
    "richos/app/scripts/qa/ios2.sh": 'xcrun devicectl device install app --device "$UDID" "$APP"\n',  # device-cli-exempt: planted, the scan must refuse it
    "richos/app/scripts/qa/ios3.mjs": "const app = 'build/Debug-iphoneos/RichOSNative.app';\n",  # device-cli-exempt: planted, the scan must refuse it
    "richos/mobile/native-android/bin/randroid": 'adb -s "$S" uninstall dev.richos.connect\n',  # device-cli-exempt: planted: the command line itself may never remove the app
    "docs/howto.md": "Prose about adb install is fine.\n\n```sh\nadb -s SERIAL install app.apk\n```\n",  # device-cli-exempt: planted, the fenced command must be refused
}
CLEAN = {
    "richos/mobile/native-android/bin/apk-install.sh": 'echo never an uninstall; "$adb" -s "$serial" install -r "$apk"\n',  # device-cli-exempt: a fixture the check must pass
    "richos/app/scripts/qa/t.test.py": 'assert "uninstall" not in calls and "pm clear" not in calls\n',  # device-cli-exempt: a fixture the check must pass
    "richos/app/scripts/qa/sim.sh": 'xcrun simctl install "$UDID" "$APP"\nmake install\n',
    "richos/mobile/perf/android.py": 'dev.run("install", "-r", apk)\n',  # device-cli-exempt: a fixture the check must pass
    "richos/app/scripts/qa/x.sh": 'adb -s "$S" install x.apk  # device-cli-exempt: a reviewer-visible reason here\n',
    "docs/prose.md": "Run `adb install -r` only through randroid device.\n",  # device-cli-exempt: a fixture the check must pass
    "richos/engine/reference/advanced-tier/old.sh": 'adb -s "$S" uninstall example\n',  # device-cli-exempt: a fixture the check must pass
}


def scratch_repo(tmp, files):
    root = Path(tmp)
    for path, text in files.items():
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(text)
    subprocess.run(["git", "init", "-q", str(root)], check=True)
    subprocess.run(["git", "-C", str(root), "add", "-A"], check=True)
    return root


@case("D10 the commit check names every planted phone install, uninstall, data clear, Gradle device task, device xcodebuild and Debug iPhone build")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        root = scratch_repo(tmp, PLANTED)
        found = {path for path, *_ in physical.scan(root)}
        assert found == set(PLANTED), sorted(set(PLANTED) - found)
        p = subprocess.run([sys.executable, str(PHYSICAL), "scan", "--root", str(root)], capture_output=True, text=True)
        assert p.returncode == 1 and "REFUSED" in p.stderr and "randroid device" in p.stderr, p.stderr


@case("D11 the check passes the command lines' own installs, simulators, tests of absence, prose, declared exemptions and reference examples")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        root = scratch_repo(tmp, CLEAN)
        assert physical.scan(root) == [], physical.scan(root)


@case("D12 this repository's tree passes its own check")
def _():
    p = subprocess.run([sys.executable, str(PHYSICAL), "scan", "--root", str(REPO)], capture_output=True, text=True)
    assert p.returncode == 0, p.stderr[-1500:]


@case("D13 the commit check is wired into the commit and the land (autocheck.py)")
def _():
    text = (REPO / "richos/app/scripts/autocheck/autocheck.py").read_text()
    assert 'PHYSICAL_CHECK = "richos/mobile/physical.py"' in text
    commit = text[text.index("def commit_check("):text.index("def policy_inputs(")]
    assert commit.count("physical_check(repo, what)") == 2, "both commit paths (inside and outside richos/app) run it"
    land = text[text.index("def land_check("):text.index("def left_out(")]
    # proof-run.py reads only `cd <dir> && <command>` lines, so the land writes the scan in that form
    assert 'f"cd {scanner.parent} && python3 {scanner.name} scan"' in land, "every land runs it, as a line proof-run can read"
    assert "python3 {PHYSICAL_CHECK} scan" not in land, "not the bare form proof-run refuses"


NET_ONLINE = ("Active default network: 100\n"
              "  NetworkAgentInfo{network{100}  handle{430} ni{WIFI CONNECTED} nc{[ Transports: WIFI Capabilities: "
              "INTERNET&NOT_RESTRICTED&TRUSTED&NOT_VPN&VALIDATED&NOT_ROAMING ]}}\n"
              "  NetworkAgentInfo{network{101}  handle{431} ni{MOBILE CONNECTED} nc{[ Transports: CELLULAR Capabilities: INTERNET ]}}\n"
              "--richos--\n0\n0")


@case("D36 device list names the wired phones and their adb state without a serial, and contacts no phone")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64)
        code, out = ph.randroid("device", "list")
        assert code == 0, (code, out)
        assert json.loads(out)["result"] == [{"serial": "PHONE1", "state": "device"}], out
        assert ph.calls == ["devices"], ph.calls


@case("D37 device net: what the phone itself reaches (transport, Android's validation, a ping by address and by name), "
      "read only; no internet exits 1 with the sentence; an emulator is refused")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64, replies={"dumpsys connectivity": NET_ONLINE})
        code, out = ph.randroid("device", "--serial", "PHONE1", "net")
        assert code == 0, (code, out)
        got = json.loads(out[out.index("{"):])["result"]
        assert got["defaultTransport"] == "WIFI" and got["validatedByAndroid"] is True, got
        assert got["internet"] is True and "reaches the internet over WIFI" in got["verdict"], got
        assert touched(ph.calls) == [] and not any(" put " in c or "svc " in c for c in ph.calls), ph.calls
        ph.file.write_text(json.dumps({**ph.state, "replies": {"dumpsys connectivity": NET_ONLINE.replace("0\n0", "0\n1")}}))
        code, out = ph.randroid("device", "--serial", "PHONE1", "net")
        assert code == 1 and "cannot resolve a name" in out, (code, out)
        ph.file.write_text(json.dumps({**ph.state, "replies": {"dumpsys connectivity": "--richos--\n1\n1"}}))
        code, out = ph.randroid("device", "--serial", "PHONE1", "net")
        assert code == 1 and "no default network" in out, (code, out)
        ph.file.write_text(json.dumps({**ph.state, "emulator": True}))
        code, out = ph.randroid("device", "--serial", "PHONE1", "net")
        assert code == 3 and "is an emulator" in out, (code, out)


@case("D14 the test copy's install builds through the Firebase wrapper, so the copy carries notification settings (CEO 2026-10-03)")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        ph = Phone(tmp, sha="a" * 64)
        seen = Path(tmp) / "wrapper.log"
        wrapper = Path(tmp) / "with-android-firebase.py"
        wrapper.write_text(f'import sys\nopen({str(seen)!r}, "a").write(" ".join(sys.argv[1:]) + "\\n")\n')
        env = {"RANDROID_FIREBASE_WRAPPER": str(wrapper), "RANDROID_JAVA_HOME": "/Applications/Android Studio.app/Contents/jbr/Contents/Home"}
        for key in ("projectId", "appId", "apiKey", "senderId"):
            env["ORG_GRADLE_PROJECT_richos.firebase." + key] = ""
        ph.randroid("device", "--serial", "PHONE1", "install", env=env)
        assert seen.exists() and ":app:assemblePerfCopy" in seen.read_text(), "the copy was built without the Firebase wrapper"
        assert touched(ph.calls) == [], ph.calls


if __name__ == "__main__":
    if failures:
        print(f"mobile-device: {len(failures)} of {len(names)} FAILED: {', '.join(failures)}")
        sys.exit(1)
    print(f"mobile-device: {len(names)} cases passed")
