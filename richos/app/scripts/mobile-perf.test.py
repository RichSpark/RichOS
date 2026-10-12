#!/usr/bin/env python3
"""mobile-perf.test.py — the RichConnect measurement tool (richos/mobile/perf) answers from what the
platform printed, and refuses rather than measure the wrong build or the wrong device.

Parsers run against output captured from the API 34 emulator on 2026-09-24 (fixtures/android/),
the whole Android run against a scripted adb and iOS parsers against both scripted output and
the field/reference layout captured on an iPhone SE with iOS 26.3.1. Nothing boots,
builds or opens a window; no adb, simulator or network is touched.
"""
import base64
import json
import os
import re
import subprocess
import sys
import tempfile
import types

HERE = os.path.dirname(os.path.abspath(__file__))
PERF = os.path.abspath(os.path.join(HERE, "..", "..", "mobile", "perf"))
FIX = os.path.join(PERF, "fixtures", "android")
MOBILE = os.path.abspath(os.path.join(PERF, ".."))
sys.path.insert(0, PERF)
# What a run records before it changes a phone (phone_changes.py) lives in this suite's own directory, never ~.
os.environ["RICHOS_PHONE_CHANGES_DIR"] = tempfile.mkdtemp(prefix="richos-phone-changes-test-")
import atexit  # noqa: E402
import shutil  # noqa: E402
atexit.register(shutil.rmtree, os.environ["RICHOS_PHONE_CHANGES_DIR"], True)

import android  # noqa: E402
import ios  # noqa: E402
import perf  # noqa: E402
import perfcore  # noqa: E402

# A physical phone is measured only when `randroid device` / `rios device` started the run (they set
# RICHOS_DEVICE_VERB). Every case below runs as the verb would run it; DV1-DV2 call the unwrapped
# functions to prove the refusal without it.
VERB = "RICHOS_DEVICE_VERB"
BARE_RUN_ANDROID, BARE_RUN_IOS = perf.run_android, ios.run_ios


def as_verb(cli, fn):
    def wrapped(*a, **kw):
        before = os.environ.get(VERB)
        os.environ[VERB] = cli
        try:
            return fn(*a, **kw)
        finally:
            if before is None:
                os.environ.pop(VERB, None)
            else:
                os.environ[VERB] = before
    return wrapped


perf.run_android = as_verb("randroid", BARE_RUN_ANDROID)
ios.run_ios = as_verb("rios", BARE_RUN_IOS)

failures = []
# Library/Application Support: the state directory's parent, through which the state is written.
STATE_PARENT = os.path.dirname(ios.STATE_DIR)


def case(name):
    def wrap(fn):
        try:
            fn()
            print(f"  ok    {name}")
        except Exception as e:  # noqa: BLE001 — every failure is reported by name
            failures.append(name)
            print(f"  FAIL  {name}: {type(e).__name__}: {e}")
        return fn
    return wrap


def fixture(name):
    with open(os.path.join(FIX, name)) as f:
        return f.read()


def raises(exc, fn, *a, **kw):
    try:
        fn(*a, **kw)
    except exc as e:
        return str(e)
    raise AssertionError(f"expected {exc.__name__}")


# ---------------------------------------------------------------------------------------------
# statistics and the record
# ---------------------------------------------------------------------------------------------

@case("S1 nearest-rank percentiles; p95 only from 20 samples, p99 only from 100")
def _():
    s = perfcore.stats(list(range(1, 21)))
    assert (s["n"], s["p50"], s["p95"], s["p99"], s["max"]) == (20, 10, 19, None, 20), s
    small = perfcore.stats([5, 1, 9])
    assert small["p95"] is None and small["p50"] == 5 and "pilot" in small["why"], small
    big = perfcore.stats(list(range(1, 101)))
    assert big["p99"] == 99 and "why" not in big, big


@case("S2 a budget comparison without a p95 says why instead of passing")
def _():
    c = perfcore.compare("coldLaunch", perfcore.stats([400, 500]))
    assert c["within"] is None and "p95" in c["why"] and c["budget"] == 1000, c
    ok = perfcore.compare("coldLaunch", perfcore.stats([900] * 20))
    assert ok["within"] is True and "proposed" in ok["source"], ok
    over = perfcore.compare("warmResume", perfcore.stats([150] * 18 + [900, 900]))  # rank 19 of 20
    assert over["within"] is False, over


@case("S3 acceptance is NOT VERIFIED on an emulator, a debug build or under 100 trials; never PASS")
def _():
    a = perfcore.acceptance("emulator", False, 20)
    assert a["verdict"] == "NOT VERIFIED" and len(a["why"]) == 3, a
    b = perfcore.acceptance("physical", True, 100)
    assert b["verdict"] != "PASS" and not b["why"], b
    problems = perfcore.check_record({"schema": perfcore.SCHEMA, "acceptance": {"verdict": "PASS"}})
    assert any("never says PASS" in p for p in problems), problems


@case("S4 a record whose installed bytes are not the stamped bytes, or with no commit, is unsound")
def _():
    rec = {"schema": perfcore.SCHEMA, "platform": "android", "build": {"installedSha256": "a", "builtSha256": "b"},
           "device": {"kind": "emulator"}, "route": {}, "metrics": {"x": {"samplesMs": [1, 2], "stats": {"n": 3}}},
           "acceptance": {}, "notMeasured": [{"what": "y"}], "startedAt": "t"}
    problems = " | ".join(perfcore.check_record(rec))
    for needle in ("names no commit", "not the stamped build", "states no method", "stats.n 3 but 2", "lacks what or why"):
        assert needle in problems, (needle, problems)


@case("S5 stamp names the checkout's commit and whether the build's paths were uncommitted")
def _():
    with tempfile.TemporaryDirectory() as repo:
        # A throwaway repository: no user-global hooks, a placeholder identity (as battery-check.test.py).
        git = ["git", "-C", repo, "-c", "core.hooksPath=/dev/null", "-c", "user.name=Perf Test",
               "-c", "user.email=perf-test@example.invalid", "-c", "commit.gpgsign=false"]
        subprocess.run(git + ["init", "-q"], check=True)
        subprocess.run(git + ["commit", "-q", "--allow-empty", "-m", "x"], check=True)
        os.makedirs(os.path.join(repo, "src"))
        artifact = os.path.join(repo, "a.apk")
        with open(artifact, "wb") as f:
            f.write(b"bytes")
        ident = perfcore.source_identity(repo, ["src"])
        assert len(ident["commit"]) == 40 and ident["dirty"] is False, ident
        with open(os.path.join(repo, "src", "x.kt"), "w") as f:
            f.write("changed")
        assert perfcore.source_identity(repo, ["src"])["dirty"] is True
        d = os.path.join(repo, "App.app")
        os.makedirs(d)
        with open(os.path.join(d, "Info.plist"), "w") as f:
            f.write("one")
        first = perfcore.tree_sha256(d)
        with open(os.path.join(d, "Info.plist"), "w") as f:
            f.write("two")
        assert perfcore.tree_sha256(d) != first


@case("S6 a stamp from another commit, or from uncommitted changes, is refused as a freshness mismatch")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "s.json")
        with open(path, "w") as f:
            json.dump({"commit": "abcdef1234", "dirty": False, "sha256": "x"}, f)
        assert perf.load_stamp(path, "abcdef1")["commit"] == "abcdef1234"
        assert "freshness mismatch" in raises(perfcore.Refused, perf.load_stamp, path, "1234567")
        with open(path, "w") as f:
            json.dump({"commit": "abcdef1234", "dirty": True, "sha256": "x"}, f)
        assert "uncommitted" in raises(perfcore.Refused, perf.load_stamp, path, "abcdef1")
        assert "no build stamp" in raises(perfcore.Refused, perf.load_stamp, None, None)


# ---------------------------------------------------------------------------------------------
# Android parsers, on captured output
# ---------------------------------------------------------------------------------------------

AM_COLD = """Starting: Intent { cmp=dev.richos.connect/dev.richos.android.app.MainActivity }
Status: ok
LaunchState: COLD
Activity: dev.richos.connect/dev.richos.android.app.MainActivity
TotalTime: 1239
WaitTime: 1251
Complete
"""
AM_HOT = AM_COLD.replace("COLD", "HOT").replace("1239", "185").replace("1251", "190")
LOGCAT = """09-24 03:37:11.573   514   533 I ActivityTaskManager: Displayed dev.richos.connect/dev.richos.android.app.MainActivity for user 0: +1s239ms
09-24 03:37:12.197   514   533 I ActivityTaskManager: Fully drawn dev.richos.connect/dev.richos.android.app.MainActivity: +1s863ms
"""


@case("A1 am start -W: launch state and TotalTime; a failed start is unmeasurable, not zero")
def _():
    assert android.parse_am_start(AM_COLD) == {"status": "ok", "launchState": "COLD", "totalMs": 1239, "waitMs": 1251}
    assert android.parse_am_start(AM_HOT)["launchState"] == "HOT"
    raises(perfcore.Unmeasurable, android.parse_am_start, "Error: Activity class does not exist.")


@case("A2 'Fully drawn' in every duration form; none is None, never 0")
def _():
    assert android.parse_fully_drawn(LOGCAT) == 1863
    assert android.parse_plus_duration("+863ms") == 863
    assert android.parse_plus_duration("+1m2s3ms") == 62003
    assert android.parse_plus_duration("+2s") == 2000
    assert android.parse_fully_drawn(LOGCAT.splitlines()[0]) is None
    raises(perfcore.Unmeasurable, android.parse_plus_duration, "+")


@case("A3 framestats: the activity's window only, 19 rows, 18 timed frames")
def _():
    windows = android.parse_framestats(fixture("gfxinfo-framestats-tap.txt"))
    w = android.app_window(windows)
    assert list(windows) == ["dev.richos.connect/dev.richos.android.app.MainActivity"], list(windows)
    assert w["totalFrames"] == 18 and len(w["rows"]) == 19, (w["totalFrames"], len(w["rows"]))
    assert sum(1 for r in w["rows"] if android.frame_ok(r)) == 18
    raises(perfcore.Unmeasurable, android.app_window, {})
    # The test copy runs under its own application ID: the window lookup must follow use_package().
    saved = (android.PACKAGE, android.ACTIVITY, android.RECEIVER, android.MANIFEST)
    try:
        android.use_package("dev.richos.connect.perf")
        found = android.app_window({"dev.richos.connect.perf/dev.richos.android.app.MainActivity": {"rows": []}})
        assert found == {"rows": []}, found
    finally:
        android.PACKAGE, android.ACTIVITY, android.RECEIVER, android.MANIFEST = saved


@case("A4 tap: the app's own input event to the frame carrying its id (79.6 ms), settled at 752.6 ms")
def _():
    rows = android.app_window(android.parse_framestats(fixture("gfxinfo-framestats-tap.txt")))["rows"]
    events = android.parse_input_events(fixture("atrace-tap.txt"), 5911)
    assert [e["id"] for e in events] == [android.signed32(0xe55a39f2), android.signed32(0xc4bbd379)], events
    assert not android.parse_input_events(fixture("atrace-tap.txt"), 843) == events  # SystemUI's event is not the app's
    r = android.tap_latency(events, rows)
    # By hand: FrameCompleted 264288579957 - eventTimeNano 264209000000 = 79.58 ms; the last frame
    # of the burst completes at 264961640166 → 752.64 ms; the Flags=8 row is not a frame.
    assert (r["inputToFrameMs"], r["inputToSettledMs"], r["burstFrames"]) == (79.6, 752.6, 18), r
    assert "no deliverInputEvent" in raises(perfcore.Unmeasurable, android.tap_latency, [], rows)
    assert "InputEventId" in raises(perfcore.Unmeasurable, android.tap_latency, [{"id": 1, "eventTimeNs": 0}], rows)


@case("A12 launch joins different trace and frame clock origins through the draw counter")
def _():
    trace = "\n".join([
        " system-1 (1) [000] .... 10.000000: tracing_mark_write: S|1|launchingActivity#7|0",
        " system-1 (1) [000] .... 10.500000: tracing_mark_write: I|1|launchingActivity#7:completed-cold:dev.richos.connect",
        " app-42 (42) [000] .... 10.999999: tracing_mark_write: B|42|richconnect:foreground-useful",
        " worker-43 (42) [001] .... 10.999999: tracing_mark_write: E|42",
        " worker-43 (42) [001] .... 10.999999: tracing_mark_write: C|42|richconnect:monotonic-ns|9000000000",
        " app-42 (42) [000] .... 11.000000: tracing_mark_write: C|42|richconnect:monotonic-ns|3000000000",
        " app-42 (42) [000] .... 11.000001: tracing_mark_write: E|42",
    ])
    frame = {"Flags": 0, "DrawStart": 2_900_000_000, "SyncQueued": 3_050_000_000,
             "FrameCompleted": 3_060_000_000, "DisplayPresentTime": 3_100_000_000, "FrameTimelineVsyncId": 55}
    result = android.useful_launch_frame(trace, [frame], 42)
    assert result["usefulMs"] == 1100 and result["frameTimelineId"] == 55, result
    assert android.useful_launch_frame(trace, [{**frame, "Flags": 1}], 42) == result
    raises(perfcore.Unmeasurable, android.useful_launch_frame, trace, [{**frame, "Flags": 8}], 42)
    raises(perfcore.Unmeasurable, android.useful_launch_frame, trace, [frame, frame], 42)
    raises(perfcore.Unmeasurable, android.useful_launch_frame, trace, [{**frame, "DisplayPresentTime": 0}], 42)
    raises(perfcore.Unmeasurable, android.useful_launch_frame, trace.replace("monotonic-ns", "missing"), [frame], 42)


@case("A13 a rejected launch retains its raw report and trace")
def _():
    class Device:
        def sh(self, command, **kw):
            if command.startswith("am start"):
                return "Status: ok\nLaunchState: UNKNOWN (0)\n"
            return ""
        def run(self, *args, **kw): return "captured trace"
        def begin_change(self, *a): pass
        def end_change(self, *a): pass
        def sleep(self, seconds): pass
        def pid(self): return 42
    with tempfile.TemporaryDirectory(prefix="perf-launch-evidence-") as directory:
        measure = android.Measure(Device(), evidence_dir=directory)
        measure.gfx = lambda: {"rows": []}
        raises(perfcore.Unmeasurable, measure.traced_launch)
        with open(os.path.join(directory, "launch-0001.json")) as file: saved = json.load(file)
        assert "UNKNOWN" in saved["rawLaunch"] and saved["error"], saved
        with android.gzip.open(os.path.join(directory, "launch-0001.trace.gz"), "rt") as file:
            assert file.read() == "captured trace"


@case("A14 a launch whose useful frame left the phone's short framestats ring by the final read is still timed from the earlier read")
def _():
    trace = "\n".join([
        " system-1 (1) [000] .... 10.000000: tracing_mark_write: S|1|launchingActivity#7|0",
        " system-1 (1) [000] .... 10.500000: tracing_mark_write: I|1|launchingActivity#7:completed-cold:dev.richos.connect",
        " app-42 (42) [000] .... 10.999999: tracing_mark_write: B|42|richconnect:foreground-useful",
        " app-42 (42) [000] .... 11.000000: tracing_mark_write: C|42|richconnect:monotonic-ns|3000000000",
        " app-42 (42) [000] .... 11.000001: tracing_mark_write: E|42",
    ])
    useful = {"Flags": 0, "IntendedVsync": 2_890_000_000, "DrawStart": 2_900_000_000, "SyncQueued": 3_050_000_000,
              "FrameCompleted": 3_060_000_000, "DisplayPresentTime": 3_100_000_000}
    late = [{"Flags": 0, "IntendedVsync": 8_000_000_000 + i, "DrawStart": 8_000_000_000 + i, "SyncQueued": 8_000_000_100 + i,
             "FrameCompleted": 8_000_000_200 + i, "DisplayPresentTime": 8_000_000_300 + i} for i in range(10)]
    class Device:
        clock = 0.0
        def sh(self, command, **kw):
            return "Status: ok\nLaunchState: COLD\nTotalTime: 411\nWaitTime: 413\n" if command.startswith("am start") else ""
        def run(self, *args, **kw): return trace
        def begin_change(self, *a): pass
        def end_change(self, *a): pass
        def sleep(self, seconds): self.clock += seconds
        def pid(self): return 42
    device = Device()
    measure = android.Measure(device)
    # the ring holds the useful frame only in the first second after the launch; after that, ten later frames
    measure.gfx = lambda: {"rows": [useful] + late[:3] if device.clock < 1.0 else late}
    launch, detail = measure.traced_launch()
    assert detail["usefulMs"] == 1100 and launch["launchState"] == "COLD", detail


@case("A15 a start the phone's report could not time is replaced by one more start, so the series still holds every timed start")
def _():
    class Device:
        def sh(self, command, **kw): return ""
        def run(self, *args, **kw): return ""
        def sleep(self, seconds): pass
        def pid(self): return 42
        def uptime_epoch(self): return "0"
    measure = android.Measure(Device())
    measure.foreground = lambda: {"launchState": "HOT", "totalMs": 100}
    measure.home = lambda: None
    calls = []
    def traced():
        calls.append(1)
        if len(calls) in (2, 4): raise perfcore.Unmeasurable("OEM did not report a usable presentation timestamp")
        return {"launchState": "HOT", "totalMs": 100}, {"usefulMs": 100.0 + len(calls)}
    measure.traced_launch = traced
    got = measure.warm(5, physical=True)
    assert len(got["samples"]) == 5 and len(got["rejected"]) == 2 and len(calls) == 7, (got, len(calls))
    calls.clear()
    measure.traced_launch = lambda: (calls.append(1), (_ for _ in ()).throw(perfcore.Unmeasurable("nothing is ever timed")))[1]
    stuck = measure.warm(5, physical=True)
    assert stuck["samples"] == [] and len(calls) == 2, (stuck, len(calls))  # two untimed starts before any timed one end the series


@case("A5 frame timing counts deadline misses and stalls of 100 ms or more")
def _():
    rows = android.app_window(android.parse_framestats(fixture("gfxinfo-framestats-tap.txt")))["rows"]
    t = android.frame_timing(rows)
    assert t["frames"] == 18 and t["skippedRows"] == 1 and 0 <= t["onTimePercent"] <= 100, t
    assert t["maxFrameMs"] >= 60 and t["stallsOver100Ms"] >= 0, t
    assert android.frame_timing([]) == {"frames": 0, "skippedRows": 0}


@case("A6 batterystats checkin: the uid's rows only (captured), and every kind the background check reads")
def _():
    got = android.parse_checkin(fixture("batterystats-checkin.txt"), 10192)
    assert got["cpuMs"] == {"user": 23, "system": 40} and got["uidSeen"], got
    assert got["networkBytes"]["total"] == 0 and got["processes"][0]["name"] == "dev.richos.connect", got
    synthetic = "\n".join([  # SYNTHETIC rows in the checkin grammar, for the kinds the capture had none of
        '9,10192,l,nt,100,20,3000,400,1,1,2,2,0,0,0,0',
        '9,10192,l,wua,"*walarm*:dev.richos.connect/.Retry",3',
        '9,10192,l,wl,"*job*/dev.richos.connect/x",0,f,0,1200,p,2,0,bp,0',
        '9,10192,l,jb,"dev.richos.connect/x",1500,2',
        '9,10193,l,cpu,999,999,0',
        '9,10192,l,awl,1200,1200'])
    s = android.parse_checkin(synthetic, 10192)
    assert s["networkBytes"]["total"] == 3520 and s["wakeupAlarms"] == [{"name": "*walarm*:dev.richos.connect/.Retry", "count": 3}], s
    assert s["jobs"][0]["count"] == 2 and s["partialWakelockMs"] == 1200 and s["cpuMs"] is None, s
    assert android.parse_checkin(synthetic, 424242)["uidSeen"] is False


@case("A7 thread wakeups are per thread id, attribute new and ended threads, and sum")
def _():
    a = android.parse_threads("5911|.richos.connect|voluntary_ctxt_switches: 386 nonvoluntary_ctxt_switches: 931\n"
                              "5920|HeapTaskDaemon|voluntary_ctxt_switches: 15 nonvoluntary_ctxt_switches: 5\n"
                              "5930|DefaultDispatch|voluntary_ctxt_switches: 1 nonvoluntary_ctxt_switches: 1\n")
    b = android.parse_threads("5911|.richos.connect|voluntary_ctxt_switches: 388 nonvoluntary_ctxt_switches: 931\n"
                              "5920|HeapTaskDaemon|voluntary_ctxt_switches: 15 nonvoluntary_ctxt_switches: 5\n"
                              "5940|OkHttp Dispatch|voluntary_ctxt_switches: 4 nonvoluntary_ctxt_switches: 0\n")
    w = android.thread_wakeups(a, b)
    assert w["total"] == 6 and w["threadsEnded"] == [5930], w
    assert w["threads"][0] == {"tid": 5940, "thread": "OkHttp Dispatch", "switches": 4, "new": True}, w
    assert android.thread_wakeups(b, b)["total"] == 0


@case("A8 /proc stat, io, fsync trace and socket tables")
def _():
    assert android.parse_proc_stat_ticks("5911 (.richos connect) S 1 2 3 4 5 6 7 8 9 10 142 86 0 0 0 0 1 0 123 0") == 228
    io = android.parse_proc_io("rchar: 10\nwchar: 7020735\nsyscr: 3\nsyscw: 28108\nread_bytes: 0\nwrite_bytes: 180224\n")
    assert android.io_delta(io, dict(io, wchar=io["wchar"] + 500, syscw=io["syscw"] + 2))["wchar"] == 500
    raises(perfcore.Unmeasurable, android.parse_proc_io, "Permission denied")
    trace = ("  DefaultDispatch-5930  ( 5911) [001] ..... 300.1: ext4_sync_file_enter: dev 253,39 ino 1 parent 2 datasync 0\n"
             "  kworker-77  (   77) [000] ..... 300.2: ext4_sync_file_enter: dev 253,39 ino 9 parent 2 datasync 1\n"
             "  .richos.connect-5911  ( 5911) [001] ..... 300.3: f2fs_sync_file_enter: dev 253,39 ino 3\n")
    assert android.count_fsyncs(trace, 5911) == 2
    tcp = ("  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n"
           "   0: 0100007F:1F90 0100007F:D2F0 01 00000000:00000000 00:00000000 00000000 10192        0 1\n"
           "   1: 0100007F:1F91 0100007F:D2F1 0A 00000000:00000000 00:00000000 00000000  1000        0 2\n")
    assert android.parse_sockets(tcp, 10192) == {"ESTABLISHED": 1}


@case("A9 the UI dump: nodes by description, text or substring, and their centers")
def _():
    xml = ('<?xml version="1.0"?><hierarchy><node text="perf probe" content-desc="" bounds="[21,2179][1059,2316]" clickable="true"/>'
           '<node text="" content-desc="Send message" bounds="[928,2185][1054,2311]" clickable="true"/></hierarchy>')
    nodes = android.ui_nodes(xml)
    assert android.center(android.find_node(nodes, desc="Send message")) == (991, 2248)
    assert android.find_node(nodes, contains="probe")["text"] == "perf probe"
    assert android.find_node(nodes, desc="Settings") is None
    raises(perfcore.Unmeasurable, android.ui_nodes, "null root node")


@case("A10 seeded history and streamed replies are synthetic frames of the phone protocol")
def _():
    frame = android.history_frame(3)
    data = json.loads(frame["wire"].split("data: ", 1)[1])
    assert frame["type"] == "receive" and frame["wire"].startswith("id: 1\nevent: hello\n")
    assert [m["role"] for m in data["messages"]] == ["ceo", "rich", "ceo"] and data["latest_cursor"] == 3
    opening, middle, final, full = android.stream_frames(50, 3)
    assert len(middle) == 3 and full == "word1 word2 word3 " and "event: delta" in middle[0]["wire"]
    assert json.loads(final["wire"].split("data: ", 1)[1])["complete"] is True


# ---------------------------------------------------------------------------------------------
# the whole Android run, against a scripted adb
# ---------------------------------------------------------------------------------------------

def quiet(_line):
    pass


import hashlib  # noqa: E402

APK_BYTES = b"richos-connect-test-apk"
SHA = hashlib.sha256(APK_BYTES).hexdigest()  # the installed APK's bytes, so a pulled copy hashes to what the phone reports


class FakeAdb:
    """Answers the adb commands the run issues, as the API 34 emulator did; records them all."""

    def __init__(self, installed=SHA, qemu="1", pid_changes_on_warm=False, debuggable=True, apks=None):
        self.calls, self.installed, self.qemu = [], installed, qemu
        self.cold_next, self.pid, self.pid_changes = True, 5911, pid_changes_on_warm
        self.state = {"draft": "", "messages": [], "paired": True}
        self.debuggable, self.apks = debuggable, apks or {}  # apks: local path -> (sha256, debuggable)
        self.pushed, self.core = {}, {}  # device path -> bytes; files/core name -> bytes (the app's data)
        self.contents = {SHA: APK_BYTES}  # apk sha256 -> bytes (what `adb pull` returns)
        self.flags = {installed: debuggable}  # apk sha256 -> debuggable
        self.fail = set()  # verbs that fail: "pull", "install", "tar-x", "tar-c"
        self.bad_signature = set()  # local APK paths the phone refuses to install over the app
        self.ui_text = "Synthetic message 4 for the launch"  # the newest row a UI dump shows

    def tar_bytes(self):
        import io
        import tarfile
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w") as t:
            for name, data in sorted(self.core.items()):
                info = tarfile.TarInfo(f"./files/core/{name}")
                info.size = len(data)
                t.addfile(info, io.BytesIO(data))
        return buf.getvalue()

    def file_command(self, args):
        """push, pull, install, uninstall and run-as against an app whose data is `self.core`."""
        import io
        import tarfile
        cmd = " ".join(args[1:]) if args[0] == "shell" else None
        if args[0] == "push":
            with open(args[1], "rb") as f:
                self.pushed[args[2]] = f.read()
        elif args[0] == "pull":
            if "pull" in self.fail:
                return types.SimpleNamespace(returncode=1, stdout="", stderr="pull failed")
            with open(args[2], "wb") as f:
                f.write(self.contents.get(self.installed, b""))
        elif args[0] == "uninstall":
            self.installed, self.core = None, {}
        elif args[0] == "install":
            if "install" in self.fail:
                return types.SimpleNamespace(returncode=1, stdout="", stderr="install failed")
            if args[-1] in self.bad_signature:
                return types.SimpleNamespace(returncode=1, stdout="",
                                             stderr="Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]")
            assert "-r" in args, f"an install without -r would replace the app the hard way: {args}"
            if args[-1] in self.apks:
                self.installed, self.debuggable = self.apks[args[-1]]
            else:  # an APK this tool pulled earlier: its bytes say which build it is
                with open(args[-1], "rb") as f:
                    data = f.read()
                self.installed = hashlib.sha256(data).hexdigest()
                self.contents[self.installed] = data
                self.debuggable = self.flags[self.installed]
            self.flags[self.installed] = self.debuggable
            self.contents.setdefault(self.installed, b"")
        elif args[0] == "exec-out" and args[1:3] == ["run-as", android.PACKAGE] and args[3] == "tar":
            if "tar-c" in self.fail:
                return types.SimpleNamespace(returncode=1, stdout=b"", stderr=b"tar failed")
            return self.tar_bytes()
        elif cmd and cmd.startswith(f"run-as {android.PACKAGE}"):
            if not self.debuggable:
                return f"run-as: package not debuggable: {android.PACKAGE}"
            rest = cmd.split(" ", 2)[2]
            if rest.startswith("cp "):
                _, src, dst = rest.split()
                self.core[dst.split("/")[-1]] = self.pushed[src]
            elif rest.startswith("sha256sum "):
                name = rest.split("/")[-1]
                return f"{hashlib.sha256(self.core[name]).hexdigest()}  files/core/{name}" if name in self.core else ""
            elif rest.startswith("sh -c"):
                return "".join(f"{hashlib.sha256(d).hexdigest()}  ./files/core/{n}\n" for n, d in sorted(self.core.items()))
            elif rest.startswith("rm -f"):
                self.core.pop(rest.split("/")[-1].strip("'"), None)
            elif rest.startswith("tar -xf"):
                if "tar-x" in self.fail:
                    return "tar: extract failed"
                with tarfile.open(fileobj=io.BytesIO(self.pushed[rest.split()[-1]])) as t:
                    for m in t.getmembers():
                        self.core[m.name.split("/")[-1]] = t.extractfile(m).read()
        else:
            return None
        return ""

    def bridge(self, cmd):
        command = re.search(r"--es command (\S+)", cmd).group(1)
        arg = re.search(r"--es arg64 (\S+)", cmd)
        arg = base64.b64decode(arg.group(1)).decode() if arg else None
        if command == "action":
            obj = json.loads(arg)
            if obj.get("type") == "receive" and "event: hello" in obj["wire"]:
                self.state["messages"] = json.loads(obj["wire"].split("data: ", 1)[1])["messages"]
        body = {"ok": True, "result": {"state": self.state}}
        return f'Broadcast completed: result=0, data="{base64.b64encode(json.dumps(body).encode()).decode()}"'

    def __call__(self, argv, capture_output=True, text=True, timeout=None):
        assert argv[:3] == ["/fake/adb", "-s", "emulator-5580"], argv  # never a bare adb
        args = argv[3:]
        cmd = " ".join(args[1:]) if args[0] == "shell" else " ".join(args)
        self.calls.append(cmd)
        out = self.file_command(args)
        if isinstance(out, types.SimpleNamespace):
            return out
        if out is not None:
            if not text:
                return types.SimpleNamespace(returncode=0, stdout=out, stderr=b"")
            return types.SimpleNamespace(returncode=0, stdout=out, stderr="")
        out = ""
        if cmd.startswith("getprop ro.kernel.qemu"):
            out = self.qemu
        elif cmd.startswith("getprop"):
            out = "x"
        elif cmd.startswith("pm path"):
            out = f"package:/data/app/~~x/{android.PACKAGE}/base.apk" if self.installed else ""
        elif cmd.startswith("sha256sum"):
            out = f"{self.installed}  /data/app/base.apk"
        elif cmd.startswith("dumpsys package"):
            flags = "DEBUGGABLE HAS_CODE" if self.debuggable else "HAS_CODE"
            out = f"versionName=0.1.0-dev\n    versionCode=1 minSdk=29\n    pkgFlags=[ {flags} ]\n    appId=10192"
        elif cmd.startswith("am broadcast"):
            out = self.bridge(cmd)
        elif cmd.startswith("am force-stop"):
            self.cold_next = True
        elif cmd.startswith("am start"):
            out = AM_COLD if self.cold_next else AM_HOT
            self.cold_next = False
        elif cmd.startswith("input keyevent KEYCODE_HOME") and self.pid_changes:
            self.pid += 1
        elif cmd.startswith("date"):
            out = "1790217517.123456789"
        elif cmd.startswith("logcat"):
            out = LOGCAT
        elif cmd.startswith("pidof"):
            out = str(self.pid)
        elif cmd.startswith("exec-out cat /sdcard"):
            out = (f'<?xml version="1.0"?><hierarchy><node text="{self.ui_text}" content-desc="" bounds="[0,0][1,1]"/>'
                   '<node text="" content-desc="Message Rich" bounds="[21,2179][1059,2316]"/></hierarchy>')
        elif "gfxinfo" in cmd and "framestats" in cmd:
            out = "Window: dev.richos.connect/dev.richos.android.app.MainActivity\nTotal frames rendered: 0\n"
        elif cmd.startswith("for t in"):
            out = f"{self.pid}|.richos.connect|voluntary_ctxt_switches: 386 nonvoluntary_ctxt_switches: 931\n"
        elif cmd.startswith("cat /proc/") and cmd.endswith("/stat"):
            out = f"{self.pid} (.richos.connect) S 1 2 3 4 5 6 7 8 9 10 142 86 0 0 0 0 1 0 123 0"
        elif cmd.startswith("dumpsys batterystats --checkin"):
            out = fixture("batterystats-checkin.txt")
        elif cmd.startswith("cmd uimode night") and cmd.strip() == "cmd uimode night":
            out = "Night mode: no"
        return types.SimpleNamespace(returncode=0, stdout=out, stderr="")


def android_args(tmp, **over):
    stamp = os.path.join(tmp, "stamp.json")
    with open(stamp, "w") as f:
        json.dump({"commit": "b4b1b517011837108979b5e690cb84105fef6a53", "dirty": False, "sha256": SHA,
                   "paths": ["richos/mobile/native-android"]}, f)
    base = perf.parse_args(["android", "--adb", "/fake/adb", "--serial", "emulator-5580", "--kind", "emulator",
                            "--owned-by", "randroid", "--stamp", stamp, "--cold", "2", "--warm", "2",
                            "--history", "4", "--idle-seconds", "1", "--background-seconds", "1",
                            "--only", "seed,cold,idle-conversation,warm,background"])
    base.keep_dir = os.path.join(tmp, "keep")
    for k, v in over.items():
        setattr(base, k, v)
    return base


@case("R1 a scripted run yields a sound record: build, device, route, every metric with its method")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, touched = FakeAdb(), []
        record, failures_ = perf.run_android(android_args(tmp), runner=fake, sleep=lambda s: None, log=quiet,
                                            host=lambda: {"cpuBusyPercent": 1.0}, touch=lambda: touched.append(1))
        assert failures_ == 0 and not perfcore.check_record(record), (failures_, perfcore.check_record(record), record["phases"])
        m = record["metrics"]
        assert m["coldLaunch"]["samplesMs"] == [1863, 1863] and m["coldLaunch"]["firstFrameMs"] == [1239, 1239], m["coldLaunch"]
        assert m["coldLaunch"]["screenCheck"] == {"composerOnScreen": True, "newestMessageOnScreen": True}
        assert m["warmResume"]["samplesMs"] == [185, 185] and m["idleFrames"]["zeroFrames"] is True
        assert m["backgroundQuiet"]["zeroWork"] is True and m["backgroundQuiet"]["threadWakeups"]["total"] == 0
        assert record["build"]["commit"].startswith("b4b1b517") and record["build"]["configuration"] == "debug"
        assert record["acceptance"]["verdict"] == "NOT VERIFIED" and record["device"]["kind"] == "emulator"
        assert record["conditions"]["transport"] == "unreachable"  # launch never waits on the network (Sage T6)
        cond = record["condition"]  # the default: a seeded conversation of a stated size, the Mac's state stated
        assert (cond["conversation"]["fixture"], cond["conversation"]["rows"], cond["mac"], cond["build"]) == \
            ("devbridge-hello/1", 4, "unreachable", "debug"), cond
        assert cond["verified"] == {"row": "Synthetic message 4 ", "onScreen": True}, cond
        assert touched, "the emulator lease was never renewed"
        assert "dumpsys battery reset" in fake.calls  # the unplugged state is always put back
        assert any("sendToQueued" in g["what"] for g in record["notMeasured"])


@case("R8 production launch and resume never invoke the Debug bridge")
def _():
    class ProductionAdb(FakeAdb):
        def bridge(self, cmd):
            raise AssertionError("production run invoked the development bridge")
    with tempfile.TemporaryDirectory() as tmp:
        record, failures_ = perf.run_android(
            android_args(tmp, production=True, route="tailnet", only="cold,warm", conversation="as-installed"),
            runner=ProductionAdb(), sleep=lambda s: None, log=quiet, host=lambda: {})
        assert failures_ == 0, record["phases"]
        assert record["route"]["name"] == "tailnet"
        assert record["route"]["persistence"] == "production"
        assert record["conditions"]["productionControls"] is True
        assert "seed" not in record["phases"]
        assert record["condition"]["conversation"]["fixture"] == "as-installed", record["condition"]


@case("R9 production Send timing records its method without invoking the bridge")
def _():
    from unittest.mock import patch
    with tempfile.TemporaryDirectory() as tmp:
        with patch.object(android.Measure, "tap", return_value={"samples": [20], "settled": [30], "burstFrames": [], "rejected": []}):
            record, failures_ = perf.run_android(
                android_args(tmp, production=True, exercise_sends=True, route="managed", only="tap", conversation="as-installed"),
                runner=FakeAdb(), sleep=lambda s: None, log=quiet, host=lambda: {})
        assert failures_ == 0, record["phases"]
        assert "real controls" in record["metrics"]["tapToFeedback"]["method"]


@case("R10 physical background observation preserves battery history and reports unknown work")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeAdb(qemu="0", debuggable=False)
        record, failures_ = perf.run_android(
            android_args(tmp, kind="physical", production=True, route="managed", only="background", conversation="as-installed"),
            runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert failures_ == 0, record["phases"]
        assert record["metrics"]["backgroundQuiet"]["zeroWork"] is None
        assert record["metrics"]["backgroundQuiet"]["cpuTicks"] == 0
        assert not any("batterystats --reset" in c or "battery unplug" in c or "battery reset" in c for c in fake.calls)


@case("R11 native composer text is read from the EditText sharing its label bounds")
def _():
    nodes = android.ui_nodes('<hierarchy><node class="android.widget.EditText" text="keep existing work" bounds="[0,0][100,50]"/>'
                             '<node class="android.view.View" content-desc="Message Rich" text="" bounds="[0,0][100,50]"/></hierarchy>')
    assert android.composer_node(nodes)["text"] == "keep existing work"
    from unittest.mock import patch
    fake = FakeAdb()
    measure = android.Measure(android.Device("/fake/adb", "phone", runner=fake, sleep=lambda s: None), log=quiet)
    with patch.object(measure, "dump_ui", return_value=nodes):
        raises(perfcore.Unmeasurable, measure.enter_empty_composer, "probe")
    assert not any("input text" in c for c in fake.calls)


@case("R11b probe preparation delivers each ASCII character through a separate input invocation")
def _():
    from unittest.mock import patch
    nodes = android.ui_nodes('<hierarchy><node class="android.widget.EditText" text="" bounds="[0,0][100,50]"/>'
                             '<node class="android.view.View" content-desc="Message Rich" text="" bounds="[0,0][100,50]"/></hierarchy>')
    measure = android.Measure(android.Device("/fake/adb", "phone", runner=FakeAdb(), sleep=lambda s: None), log=quiet)
    commands = []
    with patch.object(measure, "dump_ui", return_value=nodes), patch.object(measure.d, "sh", side_effect=commands.append):
        measure.enter_empty_composer("Probe 190007")
    # Execute the generated device shell loop with a recording input command. No
    # Android device is touched. This catches grouping and whitespace mistakes.
    record_input = 'input() { [ "$1" = text ] || exit 9; printf "%s\\n" "$2"; }; '
    calls = subprocess.check_output(["/bin/sh", "-c", record_input + commands[-1]], text=True).splitlines()
    assert calls == list("Probe") + ["%s"] + list("190007"), calls
    for unsafe in ("probe;echo", "$(probe)", "é"):
        commands.clear()
        with patch.object(measure, "dump_ui", return_value=nodes), patch.object(measure.d, "sh", side_effect=commands.append):
            raises(perfcore.Unmeasurable, measure.enter_empty_composer, unsafe)
        assert not commands, "invalid probe must not tap or inject anything"


@case("R12 stopped Send setup retains completed trials and their raw evidence")
def _():
    from unittest.mock import patch
    with tempfile.TemporaryDirectory() as tmp:
        measure = android.Measure(android.Device("/fake/adb", "emulator-5580", runner=FakeAdb(), sleep=lambda s: None),
                                  log=quiet, evidence_dir=tmp)
        measure.probe_prefix = "unique probe"
        nodes = [{"text": "unique probe 1", "desc": "Send message", "bounds": [0, 0, 100, 50]},
                 {"text": "", "desc": "Message Rich", "bounds": [0, 50, 100, 100]}]
        before = [nodes[0], dict(nodes[1], text="unique probe 1")]
        with patch.object(measure, "enter_empty_composer", side_effect=[None, perfcore.Unmeasurable("occupied")]), \
             patch.object(measure, "dump_ui", side_effect=[before, nodes]), \
             patch.object(measure, "atrace", return_value="raw test trace"), \
             patch.object(measure, "gfx", return_value={"rows": []}), \
             patch.object(android, "tap_latency", return_value={"inputToFrameMs": 20, "inputToSettledMs": 30, "burstFrames": 1}):
            result = measure.tap(3, production=True)
        assert result["samples"] == [20]
        assert result["rejected"] == [{"trial": 2, "why": "occupied", "stoppedSeries": True}]
        assert os.path.exists(os.path.join(tmp, "tap-0001.trace.gz"))
        assert os.path.exists(os.path.join(tmp, "tap-0001.json"))


@case("R13 altered synthetic input is retained and never sent")
def _():
    from unittest.mock import patch
    fake = FakeAdb()
    measure = android.Measure(android.Device("/fake/adb", "emulator-5580", runner=fake, sleep=lambda s: None), log=quiet)
    measure.probe_prefix = "Perf probe 123456"
    nodes = [{"text": "Perf probe 12346 1", "desc": "Message Rich", "bounds": [0, 0, 100, 50]},
             {"text": "", "desc": "Send message", "bounds": [100, 0, 150, 50]}]
    with patch.object(measure, "enter_empty_composer"), patch.object(measure, "dump_ui", return_value=nodes):
        result = measure.tap(2, production=True)
    assert result["samples"] == [] and result["rejected"][0]["stoppedSeries"] is True
    assert not any("input tap" in c for c in fake.calls)


@case("R2 an installed APK that is not the stamped one is REFUSED before any measurement")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeAdb(installed="0" * 64)
        msg = raises(perfcore.Refused, perf.run_android, android_args(tmp), runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert "freshness mismatch" in msg and not any(c.startswith("am start") for c in fake.calls), (msg, fake.calls)


@case("R3 an emulator named without randroid, or a phone that is really an emulator, is REFUSED")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        msg = raises(perfcore.Refused, perf.run_android, android_args(tmp, owned_by=None), runner=FakeAdb(), sleep=lambda s: None, log=quiet)
        assert "randroid emu perf" in msg, msg
        msg = raises(perfcore.Refused, perf.run_android, android_args(tmp, kind="physical"), runner=FakeAdb(), sleep=lambda s: None, log=quiet, host=lambda: {})
        assert "is an emulator" in msg, msg
        msg = raises(perfcore.Refused, perf.run_android, android_args(tmp), runner=FakeAdb(qemu="0"), sleep=lambda s: None, log=quiet, host=lambda: {})
        assert "physical device" in msg, msg
        raises(perfcore.Refused, android.Device, "/fake/adb", "")


@case("R4 a warm trial whose process changed is rejected as cold, never counted as warm")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        record, _ = perf.run_android(android_args(tmp, only="seed,warm"), runner=FakeAdb(pid_changes_on_warm=True),
                                     sleep=lambda s: None, log=quiet, host=lambda: {})
        warm = record["metrics"]["warmResume"]
        assert warm["samplesMs"] == [] and len(warm["rejected"]) == 2 and "cold start" in warm["rejected"][0]["why"], warm


class VanishingAdb(FakeAdb):
    """The emulator is stopped (as the Mac's CPU breaker does) at the Nth launch: every later adb
    call fails as adb does, and one hangs past its timeout."""

    def __init__(self, launches):
        super().__init__()
        self.launches, self.gone = launches, False

    def __call__(self, argv, capture_output=True, text=True, timeout=None):
        cmd = " ".join(argv[3:])
        if "am start" in cmd:
            self.launches -= 1
            if self.launches < 0:
                self.gone = True
        if self.gone:
            if "wait-for-device" in cmd:
                raise subprocess.TimeoutExpired(argv, timeout)
            return types.SimpleNamespace(returncode=1, stdout="", stderr="adb: device 'emulator-5580' not found")
        return super().__call__(argv, capture_output, text, timeout)


@case("R6 a device that goes away mid-run still yields a record: what was measured, and why the rest was not")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        record, failures_ = perf.run_android(android_args(tmp), runner=VanishingAdb(launches=1), sleep=lambda s: None,
                                             host=lambda: {}, log=quiet)
        assert failures_ >= 1 and record["phases"]["cold"].startswith("NOT MEASURED"), record["phases"]
        assert record["phases"]["warm"].startswith("NOT RUN") and record["phases"]["background"].startswith("NOT RUN")
        assert any("went away" in g["why"] for g in record["notMeasured"]), record["notMeasured"]
        assert not perfcore.check_record(record), perfcore.check_record(record)
    dev = android.Device("/fake/adb", "emulator-5580", runner=VanishingAdb(launches=-1))
    dev.runner.gone = True
    assert "did not answer" in raises(perfcore.Unmeasurable, dev.run, "wait-for-device", timeout=1)


class RecreatingAdb(FakeAdb):
    """The process stays, the activity is recreated on every return (Android's WARM start)."""

    def __call__(self, argv, capture_output=True, text=True, timeout=None):
        out = super().__call__(argv, capture_output, text, timeout)
        cmd = " ".join(argv[3:])
        if "am start" in cmd and "HOT" in out.stdout:
            out.stdout = out.stdout.replace("HOT", "WARM").replace("185", "420")
        if "logcat -b events" in cmd:
            out.stdout = "1790220400.1 1000 1000 I wm_destroy_activity: [0,1,2,dev.richos.connect/dev.richos.android.app.MainActivity,trim]\n"
        return out


@case("R7 a resume that recreates the activity in the same process is warm (PRD J2), kept apart, with its reason")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        record, _ = perf.run_android(android_args(tmp, only="seed,warm"), runner=RecreatingAdb(), sleep=lambda s: None,
                                     host=lambda: {}, log=quiet)
        warm = record["metrics"]["warmResume"]
        assert warm["samplesMs"] == [420, 420] and warm["sampleStates"] == ["WARM", "WARM"], warm
        assert warm["recreatedStats"]["n"] == 2 and warm["hotStats"]["n"] == 0, warm
        assert warm["firstRecreation"] and warm["firstRecreation"][0].endswith("trim]"), warm["firstRecreation"]


@case("R5 perf.py main exits 3 with a sentence on a refusal and writes no record")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "r.json")
        code = perf.main(["android", "--adb", "/fake/adb", "--serial", "emulator-5580", "--kind", "emulator", "--out", out])
        assert code == 3 and not os.path.exists(out), code


# ---------------------------------------------------------------------------------------------
# iOS: parsers, the PRD §7 presentation join, the export re-read and the trace series
# ---------------------------------------------------------------------------------------------

@case("I1 a simulator is named by UDID and must be booted; 'booted' is refused")
def _():
    listing = json.dumps({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-18-0": [
        {"udid": "AAA", "name": "iPhone 16", "state": "Booted"}, {"udid": "BBB", "name": "iPhone SE", "state": "Shutdown"}]}})
    assert ios.simulator(listing, "AAA") == {"kind": "simulator", "udid": "AAA", "model": "iPhone 16", "os": "iOS 18.0"}
    assert "never 'booted'" in raises(perfcore.Refused, ios.simulator, listing, "booted")
    assert "Shutdown" in raises(perfcore.Refused, ios.simulator, listing, "BBB")
    raises(perfcore.Refused, ios.simulator, listing, "CCC")


@case("I2 the useful-content signpost's time comes from the log; other subsystems and names are ignored")
def _():
    lines = "\n".join([
        json.dumps({"subsystem": "dev.richos.connect", "signpostName": "useful-content", "timestamp": "2026-09-25 10:00:01.250000+0000"}),
        json.dumps({"subsystem": "com.apple.UIKit", "signpostName": "useful-content", "timestamp": "2026-09-25 10:00:00.100000+0000"}),
        json.dumps({"subsystem": "dev.richos.connect", "signpostName": "foreground-useful", "timestamp": "2026-09-25 10:00:02.000000+0000"}),
        "Filtering the log data using ..."])
    marks = ios.parse_log_marks(lines, "useful-content")
    assert len(marks) == 1 and abs(marks[0] - 1790330401.25) < 1e-3, marks


@case("I3 simulator background: top's cumulative wakeups, switches and CPU; nettop bytes; launchctl pid")
def _():
    top = "Processes: 1 total\nPID  IDLEW CSW      TIME     \n4242 17    21494302+ 1:02:03.50\n"
    assert ios.parse_top(top) == {"idleWakeups": 17, "switches": 21494302, "cpuSeconds": 3723.5}
    raises(perfcore.Unmeasurable, ios.parse_top, "nothing")
    assert ios.parse_nettop(",bytes_in,bytes_out,\nRichOS.4242,120,30,\nother.1,5,5,\n", 4242) == 150
    assert ios.parse_launchctl_pid("PID\tStatus\tLabel\n4242\t0\tUIKitApplication:dev.richos.connect[1a2b][rb-legacy]\n") == 4242
    assert ios.parse_launchctl_pid("-\t0\tcom.apple.x\n") is None


@case("I4 an exported xctrace os-signpost table resolves id/ref values to the signpost's time")
def _():
    xml = ('<trace-query-result><node><schema name="os-signpost"/>'
           '<row><event-time id="1" fmt="00:00.812">812000000</event-time><name id="2">useful-content</name></row>'
           '<row><event-time id="3">900000000</event-time><name ref="2"/></row>'
           '<row><event-time id="4">50000000</event-time><name id="5">other</name></row>'
           '</node></trace-query-result>')
    assert ios.parse_xctrace_signposts(xml, "useful-content") == [812000000, 900000000]


@case("I5 physical draw timing joins lifecycle and signpost PID, rejects ambiguous or foreign marks")
def _():
    # Field tags and reference layout captured on iOS 26.3.1 with Xcode 26.3.
    # Nonzero origin deliberately prevents treating trace time as elapsed launch time.
    life = ('<trace-query-result><node><row><start-time>400000000</start-time>'
            '<process id="5"><pid id="6">717</pid></process>'
            '<app-period>Initializing - Process Creation</app-period></row></node></trace-query-result>')
    marks = ('<trace-query-result><node><row><event-time>500000000</event-time>'
             '<process id="4"><pid>717</pid></process><event-type id="7">Event</event-type>'
             '<signpost-name id="10">other</signpost-name><subsystem id="12">dev.richos.connect</subsystem></row>'
             '<row><event-time>1101743958</event-time><process ref="4"/><event-type ref="7"/>'
             '<signpost-name>useful-content</signpost-name><subsystem ref="12"/></row></node></trace-query-result>')
    result = ios.trace_useful_draw(life, marks)
    assert result == {"pid": 717, "creationNs": 400000000, "usefulDrawNs": 1101743958, "durationMs": 701.743958}
    raises(perfcore.Unmeasurable, ios.trace_useful_draw, life, marks.replace('<pid>717</pid>', '<pid>718</pid>'))
    raises(perfcore.Unmeasurable, ios.trace_useful_draw, life, marks.replace('dev.richos.connect', 'other.app'))
    raises(perfcore.Unmeasurable, ios.trace_useful_draw, life, marks.replace('1101743958', '300000000'))
    raises(perfcore.Unmeasurable, ios.trace_useful_draw, life, marks.replace('>other<', '>useful-content<'))
    raises(perfcore.Unmeasurable, ios.trace_useful_draw, '<root/>', marks)


@case("I6 a failed physical capture stops before trial two and retains the error")
def _():
    from unittest.mock import patch
    calls = []
    def failed(cmd, **kwargs):
        calls.append(cmd)
        return subprocess.CompletedProcess(cmd, 1, "partial capture", "device disconnected")
    with tempfile.TemporaryDirectory() as evidence, patch.object(ios, "evidence_root_ok", return_value=True):
        samples, rejected, retained = ios.device_cold("test-device", 100, evidence, failed)
        assert not samples and len(rejected) == 1 and len(calls) == 1
        assert 'os_signpost' in calls[0] and 'Frame Lifetimes' in calls[0] and '--launch' in calls[0]
        assert os.path.exists(os.path.join(retained, 'launch-0001.rejected.json'))
        assert open(os.path.join(retained, 'launch-0001.log.stderr')).read() == 'device disconnected'


@case("I6a xctrace temp files land in the run's own folder and are removed, even when the capture fails")
def _():
    from unittest.mock import patch
    seen = []
    def stub(cmd, **kwargs):
        tmp = os.environ["TMPDIR"]
        seen.append(tmp)
        open(os.path.join(tmp, "instruments-1.ktrace"), "w").write("x")
        return subprocess.CompletedProcess(cmd, 1, "", "boom")
    with tempfile.TemporaryDirectory() as evidence, tempfile.TemporaryDirectory() as usertmp, \
            patch.dict(os.environ, {"TMPDIR": usertmp}), patch.object(ios, "evidence_root_ok", return_value=True):
        ios.device_cold("test-device", 1, evidence, stub)
        assert seen and seen[0].startswith(evidence) and not os.path.exists(seen[0])
        assert os.environ["TMPDIR"] == usertmp and os.listdir(usertmp) == []


@case("I6b traces refuse a missing selected volume before capture")
def _():
    calls = []
    def never(cmd, **kwargs):
        calls.append(cmd)
        raise AssertionError("nothing may run")
    elsewhere = "/Volumes/RichOSMissingTraceFixture/cache" # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    assert "--evidence-dir" in raises(perfcore.Refused, ios.trace_series, "cold", ios.Devicectl("d", never), 1, elsewhere, never)
    assert "--evidence-dir" in raises(perfcore.Refused, ios.trace_series, "cold", ios.Devicectl("d", never), 1, None, never)
    assert not calls


# Synthetic exports shaped exactly like Xcode 26.3's (columns, engineering-type tags, id/ref reuse),
# checked field by field against a physical iPhone SE capture whose data stays private (richos-hq).
LIFE_COLS = ["start", "group", "lane", "duration", "process", "period", "narrative"]
SIGN_COLS = ["time", "thread", "process", "event-type", "scope", "identifier", "name", "format-string", "backtrace",
             "subsystem", "category", "message", "emit-location"]
FRAME_COLS = ["start", "duration", "display-id", "lifetime-id", "swap-id", "frame-seed", "hitch-duration",
              "acceptable-latency", "hid-latency", "render-start", "render-duration", "layout-qualifier", "type-label",
              "narrative", "severity", "color"]
SWAP_COLS = ["timestamp", "delay", "display-name", "surface-id", "framebuffer-index", "swap-id", "color", "pixel-format",
             "hid-time", "generation-time", "min-quanta", "desired-presentation-time", "layer1-surface-id",
             "layer2-surface-id", "layer1-pixel-format", "layer2-pixel-format"]


def export(schema, cols, rows, schema_tag=True):
    head = "<schema name=\"%s\">%s</schema>" % (schema, "".join(f"<col><mnemonic>{c}</mnemonic></col>" for c in cols)) if schema_tag else ""
    return f"<?xml version=\"1.0\"?><trace-query-result><node xpath='//x'>{head}{''.join(rows)}</node></trace-query-result>"


def life_row(start, period, pid=717):
    return (f"<row><start-time>{start}</start-time><string>States</string><layout-id>0</layout-id><duration>1</duration>"
            f"<process><pid>{pid}</pid></process><app-period>{period}</app-period><narrative>n</narrative></row>")


def sign_row(ns, name, sub="dev.richos.connect", kind="Event", pid=717, main=True, message=None):
    thread = f"{'Main Thread 0x78d2' if main else 'Thread 0x9a1'} (RichOSNative, pid: {pid})"
    msg = f'<os-log-metadata fmt="{message}"/>' if message else "<sentinel/>"
    return (f"<row><event-time>{ns}</event-time><thread fmt=\"{thread}\"><tid>1</tid><process><pid>{pid}</pid></process>"
            f"</thread><process><pid>{pid}</pid></process><event-type>{kind}</event-type><string>Process</string>"
            f"<os-signpost-identifier>1</os-signpost-identifier><signpost-name>{name}</signpost-name><sentinel/><sentinel/>"
            f"<subsystem>{sub}</subsystem><category>c</category>{msg}<sentinel/></row>")


def frame_row(start, duration, swap, render):
    return (f"<row><start-time>{start}</start-time><duration>{duration}</duration><uint32>4294967295</uint32><uint32>1</uint32>"
            f"<uint32>{swap}</uint32><uint32>4294967295</uint32><sentinel/><sentinel/><sentinel/><start-time>{render}</start-time>"
            f"<duration>5000000</duration><layout-id>0</layout-id><sentinel/><formatted-label fmt=\"Frame\"/>"
            f"<event-concept>Info</event-concept><sentinel/></row>")


def swap_row(ns, swap):
    return (f"<row><start-time>{ns}</start-time><duration>1</duration><string>Built-In Display</string><uint32>160</uint32>"
            f"<uint32>0</uint32><uint32>{swap}</uint32><uint32>0</uint32><sentinel/><start-time>0</start-time>"
            f"<start-time>0</start-time><uint32>1</uint32><start-time>0</start-time><sentinel/><sentinel/><sentinel/><sentinel/></row>")


CA = "com.apple.coreanimation"


def launch_tables(**over):
    """A cold launch: creation at 400 ms; composer 900, viewport 1010 (after a commit ending at 1000
    that only carried the first draw), carrying commit 1011-1015, input-ready 1015.05; a frame whose
    render began at 1012 (before the commit ended) and the first frame rendered after it at 1020."""
    marks = over.get("marks") or [
        sign_row(900_000_000, "composer-ready"), sign_row(990_000_000, "Commit", CA, "Begin"),
        sign_row(995_000_000, "useful-content"), sign_row(1_000_000_000, "Commit", CA, "End"),
        sign_row(1_010_000_000, "viewport-ready"), sign_row(1_011_000_000, "Commit", CA, "Begin"),
        sign_row(1_013_000_000, "Commit", CA, "End", main=False),  # another thread's commit never carries it
        sign_row(1_015_000_000, "Commit", CA, "End"), sign_row(1_015_050_000, "input-ready")]
    frames = over.get("frames") or [frame_row(996_000_000, 40_000_000, 10, 1_012_000_000),
                                     frame_row(1_003_000_000, 50_000_000, 11, 1_020_000_000),
                                     frame_row(1_019_000_000, 50_000_000, 12, 1_036_000_000)]
    swaps = over.get("swaps") or [swap_row(1_036_000_000, 10), swap_row(1_053_000_000, 11), swap_row(1_069_000_000, 12)]
    life = over.get("life") or [life_row(400_000_000, "Initializing - Process Creation"),
                                life_row(1_300_000_000, "Launching - UIKit Scene Creation")]
    return {"life-cycle-period": export("life-cycle-period", LIFE_COLS, life),
            "os-signpost": export("os-signpost", SIGN_COLS, marks),
            "coreanimation-lifetime-interval": export("coreanimation-lifetime-interval", FRAME_COLS, frames),
            "display-surface-swap": export("display-surface-swap", SWAP_COLS, swaps)}


@case("I7 a cold launch ends at the swap presenting the commit that carried viewport and composer, not at the draw")
def _():
    s = ios.launch_sample(launch_tables())
    assert s["pid"] == 717 and s["commitEndNs"] == 1_015_000_000 and s["swapId"] == 11, s
    assert s["presentedNs"] == 1_053_000_000 and s["inputReadyNs"] == 1_015_050_000 and s["endNs"] == 1_053_000_000
    assert s["durationMs"] == 653.0 and s["startBoundary"] == "Initializing - Process Creation"
    assert s["phasesMs"]["usefulDraw"] == 595.0 and s["phasesMs"]["presented"] == 653.0, s["phasesMs"]


@case("I8 the join rejects foreign, ambiguous, missing and out-of-order marks rather than guess")
def _():
    base = launch_tables()
    def with_marks(change):
        rows = [sign_row(900_000_000, "composer-ready"), sign_row(995_000_000, "useful-content"),
                sign_row(1_000_000_000, "Commit", CA, "End"), sign_row(1_010_000_000, "viewport-ready"),
                sign_row(1_015_000_000, "Commit", CA, "End"), sign_row(1_015_050_000, "input-ready")]
        return {**base, "os-signpost": export("os-signpost", SIGN_COLS, change(rows))}
    cases = {
        # the app's marks from another process while the target's own commits stay: only the pid check rejects it
        "foreign process": lambda r: [x.replace("pid>717<", "pid>718<").replace("pid: 717", "pid: 718")
                                      if "dev.richos.connect" in x else x for x in r],
        "two input-ready": lambda r: r + [sign_row(1_016_000_000, "input-ready")],
        "no input-ready (composer disabled)": lambda r: r[:-1],
        "no composer-ready": lambda r: r[1:],
        "no viewport-ready": lambda r: r[:3] + r[4:],
        "input-ready before the carrying commit": lambda r: r[:4] + [sign_row(1_012_000_000, "input-ready"),
                                                                    sign_row(1_015_000_000, "Commit", CA, "End")],
        "other subsystem": lambda r: [x.replace("dev.richos.connect", "com.example") for x in r],
    }
    for name, change in cases.items():
        try:
            ios.launch_sample(with_marks(change))
        except perfcore.Unmeasurable:
            continue
        raise AssertionError(f"accepted: {name}")
    assert "exactly one" in raises(perfcore.Unmeasurable, ios.launch_sample,
                                   {**base, "life-cycle-period": export("life-cycle-period", LIFE_COLS, [])})
    assert ios.launch_sample(with_marks(lambda r: r))["swapId"] == 11  # the unchanged control is accepted


@case("I9 presentation is refused when the frame data cannot be a display pipeline or is ambiguous")
def _():
    frames = lambda rows: {**launch_tables(), "coreanimation-lifetime-interval": export("coreanimation-lifetime-interval", FRAME_COLS, rows)}
    swaps = lambda rows: {**launch_tables(), "display-surface-swap": export("display-surface-swap", SWAP_COLS, rows)}
    assert "no frame" in raises(perfcore.Unmeasurable, ios.launch_sample, frames([frame_row(996_000_000, 40_000_000, 10, 1_012_000_000)]))
    assert "same instant" in raises(perfcore.Unmeasurable, ios.launch_sample, frames(
        [frame_row(1_003_000_000, 50_000_000, 11, 1_020_000_000), frame_row(1_004_000_000, 60_000_000, 12, 1_020_000_000)]))
    assert "lifetime ends" in raises(perfcore.Unmeasurable, ios.launch_sample, swaps([swap_row(1_070_000_000, 11)]))
    assert "display swaps" in raises(perfcore.Unmeasurable, ios.launch_sample, swaps([swap_row(1_069_000_000, 12)]))
    # the Simulator (Xcode 26.3): a swap shown before its own render began
    sim = {**launch_tables(), "coreanimation-lifetime-interval": export("coreanimation-lifetime-interval", FRAME_COLS,
                                                                        [frame_row(0, 1_030_000_000, 11, 1_040_000_000)]),
           "display-surface-swap": export("display-surface-swap", SWAP_COLS, [swap_row(1_030_000_000, 11)])}
    assert "not physical" in raises(perfcore.Unmeasurable, ios.launch_sample, sim)


@case("I10 an export is read only with its own schema: no schema, several tables or a short row is refused")
def _():
    t = launch_tables()
    rows = ios.lifecycle(t["life-cycle-period"])
    assert rows[0] == {"start": 400_000_000, "duration": 1, "pid": 717, "period": "Initializing - Process Creation"}
    refd = ('<trace-query-result><node><schema name="os-signpost">' + "".join(f"<col><mnemonic>{c}</mnemonic></col>" for c in SIGN_COLS)
            + '</schema>' + sign_row(5, "input-ready").replace("<process><pid>717</pid></process><event-type>",
                                                                  '<process id="9"><pid id="10">717</pid></process><event-type>')
            + sign_row(6, "input-ready").replace('<process><pid>717</pid></process><event-type>', '<process ref="9"/><event-type>')
            + '</node></trace-query-result>')
    assert [m["pid"] for m in ios.signposts(refd)] == [717, 717]  # a ref resolves to the first row's process
    assert "schema" in raises(perfcore.Unmeasurable, ios.lifecycle, export("life-cycle-period", LIFE_COLS, [], schema_tag=False))
    two = t["life-cycle-period"].replace("</trace-query-result>", "<node xpath='//y'><row/></node></trace-query-result>")
    assert "found 2" in raises(perfcore.Unmeasurable, ios.lifecycle, two)
    short = export("life-cycle-period", LIFE_COLS, ["<row><start-time>1</start-time></row>"])
    assert "cells" in raises(perfcore.Unmeasurable, ios.lifecycle, short)
    assert "found 0" in raises(perfcore.Unmeasurable, ios.frame_lifetimes, "<trace-query-result/>")


def return_tables(**over):
    """A warm return in retained process 717: AppResume at 2496 ms, the foreground draw at 2940.9
    inside a commit ending 2940.92, input-ready 2941.04, the next render 2950 shown at 2975."""
    marks = over.get("marks") or [
        sign_row(2_496_000_000, "AppResume", "com.apple.UIKit", "Begin", message=" enableTelemetry=YES WasFrozen= 0  IsForeground= 1"),
        sign_row(2_503_000_000, "AppResume", "com.apple.UIKit", "End"),
        sign_row(2_940_900_000, "foreground-useful"), sign_row(2_940_920_000, "Commit", CA, "End"),
        sign_row(2_941_040_000, "input-ready")]
    return {"life-cycle-period": export("life-cycle-period", LIFE_COLS, over.get("life") or []),
            "os-signpost": export("os-signpost", SIGN_COLS, marks),
            "coreanimation-lifetime-interval": export("coreanimation-lifetime-interval", FRAME_COLS,
                                                      [frame_row(2_930_000_000, 45_000_000, 40, 2_950_000_000)]),
            "display-surface-swap": export("display-surface-swap", SWAP_COLS, [swap_row(2_975_000_000, 40)])}


@case("I11 a warm return runs from UIKit's AppResume to the presented foreground draw in the same process; a relaunch is cold")
def _():
    s = ios.return_sample(return_tables(), 717)
    assert s["class"] == "warm" and s["startNs"] == 2_496_000_000 and s["presentedNs"] == 2_975_000_000
    assert s["durationMs"] == 479.0 and s["phasesMs"]["inputReady"] == 445.04, s
    assert "cold start" in raises(perfcore.Unmeasurable, ios.return_sample,
                                  return_tables(life=[life_row(2_400_000_000, "Initializing - Process Creation")]), 717)
    assert "not the retained process" in raises(perfcore.Unmeasurable, ios.return_sample, return_tables(), 718)
    background = [sign_row(2_496_000_000, "AppResume", "com.apple.UIKit", "Begin", message="IsForeground= 0"),
                  sign_row(2_940_900_000, "foreground-useful"), sign_row(2_940_920_000, "Commit", CA, "End"),
                  sign_row(2_941_040_000, "input-ready")]
    assert "AppResume" in raises(perfcore.Unmeasurable, ios.return_sample, return_tables(marks=background), 717)


@case("I12 an exporter killed by a signal is re-read and every attempt is kept; other failures stop at once")
def _():
    good = launch_tables()
    def exporter(script):
        calls = []
        def run(cmd, **kwargs):
            table = cmd[cmd.index("--xpath") + 1].split('"')[-2]
            calls.append(table)
            code = script.pop(0) if script else 0
            return subprocess.CompletedProcess(cmd, code, good[table] if code == 0 else "", "")
        return run, calls
    with tempfile.TemporaryDirectory() as d:
        prefix = os.path.join(d, "launch-0001")
        run, calls = exporter([-11, -9, 0])
        tables, attempts = ios.export_tables("t.trace", prefix, run, pause=lambda s: None)
        assert set(tables) == set(ios.TABLES) and calls[:3] == ["life-cycle-period"] * 3
        assert [a["exit"] for a in attempts][:3] == [-11, -9, 0]
        assert json.load(open(prefix + ".exports.json")) == attempts
        run, calls = exporter([1])
        assert "exit 1" in raises(perfcore.Unmeasurable, ios.export_tables, "t.trace", prefix, run, pause=lambda s: None)
        assert calls == ["life-cycle-period"]
        run, calls = exporter([-11] * ios.EXPORT_ATTEMPTS)
        assert "--reparse" in raises(perfcore.Unmeasurable, ios.export_tables, "t.trace", prefix, run, pause=lambda s: None)
        assert len(calls) == ios.EXPORT_ATTEMPTS and len(json.load(open(prefix + ".exports.json"))) == ios.EXPORT_ATTEMPTS


@case("I13 the build configuration comes from the stamped bundle's bytes, by check-release's own markers")
def _():
    with open(os.path.join(MOBILE, "native-ios", "Core", "Sources", "RichOSCLI", "Simulator.swift")) as f:
        swift = f.read()
    listed = re.search(r"static let developmentMarkers = \[(.*?)\]", swift).group(1)
    assert [m.strip().strip('"') for m in listed.split(",")] == ios.DEVELOPMENT_MARKERS
    with tempfile.TemporaryDirectory() as d:
        app = os.path.join(d, "RichOSNative.app")
        os.makedirs(app)
        with open(os.path.join(app, "RichOSNative"), "wb") as f:
            f.write(b"\0".join(m.encode() for m in ios.DEVELOPMENT_MARKERS))
        assert ios.build_configuration(app) == "debug"
        with open(os.path.join(app, "RichOSNative"), "wb") as f:
            f.write(b"release code only")
        assert ios.build_configuration(app) == "release"
        with open(os.path.join(app, "RichOSNative"), "wb") as f:
            f.write(b"rios-fixture")
        assert ios.build_configuration(app) is None  # partial: neither claim
    assert ios.build_configuration(None) is None and ios.build_configuration("/no/such.app") is None


@case("I14 the tool's mark names are the app's: every one is emitted by PerformanceMarks.swift")
def _():
    with open(os.path.join(MOBILE, "native-ios", "App", "Platform", "PerformanceMarks.swift")) as f:
        swift = f.read()
    for name in (ios.VIEWPORT, ios.COMPOSER, ios.READY, ios.USEFUL, ios.FOREGROUND):
        assert f'name: "{name}")' in swift, name
    assert "CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, false" in swift  # one-shot
    assert "Timer" not in swift and "DispatchSourceTimer" not in swift and "asyncAfter" not in swift


class FakeProc:
    def __init__(self, code=0):
        self.code, self.returncode = code, None
    def wait(self, timeout=None):
        if self.returncode is None:
            self.returncode = self.code
        return self.returncode
    def poll(self):
        return self.returncode
    def terminate(self):
        self.returncode = -15


class FakeDriver:
    target = "test-device"
    def __init__(self, pids):
        self.pids, self.events = list(pids), []
    def pid(self):
        self.events.append("pid")
        return self.pids.pop(0)
    def launch(self, bundle, *args):
        self.events.append(("launch", bundle) + args)


@case("I15 a warm trial backgrounds, attaches, returns only after tracing started, and rejects a changed process")
def _():
    from unittest.mock import patch
    started = []
    def popen(cmd, **kwargs):
        started.append(cmd)
        return FakeProc(0)
    good = return_tables()
    def exporter(cmd, **kwargs):
        table = cmd[cmd.index("--xpath") + 1].split('"')[-2]
        return subprocess.CompletedProcess(cmd, 0, good[table], "")
    with tempfile.TemporaryDirectory() as d, patch.object(ios, "evidence_root_ok", return_value=True):
        driver = FakeDriver([717, 717])
        samples, rejected, ev = ios.trace_series("warm", driver, 1, d, exporter, popen, lambda s: None)
        assert not rejected and samples[0]["durationMs"] == 479.0, rejected
        assert driver.events == ["pid", ("launch", ios.AWAY_APP), ("launch", ios.BUNDLE), "pid"], driver.events
        notify, record = started
        assert notify[:2] == ["notifyutil", "-1"] and record[record.index("--notify-tracing-started") + 1] == notify[2]
        assert record[record.index("--attach") + 1] == "717" and "Frame Lifetimes" in record
        assert json.load(open(os.path.join(ev, "return-0001.trial.json"))) == {"pid": 717}
        driver = FakeDriver([717, 902])
        samples, rejected, _ = ios.trace_series("warm", driver, 5, d, exporter, popen, lambda s: None)
        assert not samples and len(rejected) == 1 and "cold start" in rejected[0]["why"]
        failing = lambda cmd, **kw: FakeProc(0) if cmd[0] == "notifyutil" else FakeProc(3)
        samples, rejected, _ = ios.trace_series("warm", FakeDriver([717, 717]), 5, d, exporter, failing, lambda s: None)
        assert not samples and "capture failed (3)" in rejected[0]["why"]


class SilentWaiter(FakeProc):
    """notifyutil that never hears the notification (a recorder that could not attach)."""
    def wait(self, timeout=None):
        if timeout is not None and self.returncode is None:
            raise subprocess.TimeoutExpired("notifyutil", timeout)
        return super().wait(timeout)


class EndedRecorder(FakeProc):
    def __init__(self):
        super().__init__(70)
        self.returncode = 70  # already exited


@case("I15b a recorder that ends before tracing starts fails the trial at once; the app is never backgrounded")
def _():
    from unittest.mock import patch
    procs = []
    def popen(cmd, **kwargs):
        procs.append(SilentWaiter() if cmd[0] == "notifyutil" else EndedRecorder())
        return procs[-1]
    with tempfile.TemporaryDirectory() as d, patch.object(ios, "evidence_root_ok", return_value=True):
        driver = FakeDriver([717])
        samples, rejected, _ = ios.trace_series("warm", driver, 3, d, None, popen, lambda s: None)
        assert not samples and "ended (70) before tracing started" in rejected[0]["why"], rejected
        assert driver.events == ["pid"], driver.events  # never sent to the background
        assert procs[0].returncode == -15  # the waiter it started was stopped
        before = set(os.listdir(d))
        assert "--trace-seconds" in raises(perfcore.Refused, ios.trace_series, "warm", FakeDriver([717]), 1, d,
                                           None, popen, lambda s: None, seconds=6, away=2.0)
        assert set(os.listdir(d)) == before  # refused before any evidence directory exists


def ios_args(**over):
    base = dict(simulator=None, device="test-device", reparse=None, stamp=None, expect_commit=None, cold=1, warm=0,
                evidence_dir="unused", away=2.0, trace_seconds=10, app_arg=None, xctrace=False,
                conversation="as-installed", mac="reachable", trace_diagnostic=True)  # the traced path is the diagnostic one
    base.update(over)
    return types.SimpleNamespace(**base)


def devicectl_listing(cmd, **kwargs):
    with open(cmd[cmd.index('--json-output') + 1], 'w') as f:
        json.dump({"result": {"devices": [{"identifier": "test-device", "hardwareProperties": {"marketingName": "iPhone"},
                    "deviceProperties": {"osVersionNumber": "26.3.1"}}]}}, f)
    return subprocess.CompletedProcess(cmd, 0, '', '')


@case("I16 a physical record carries the PRD boundary, a budget comparison and why acceptance is still NOT VERIFIED")
def _():
    from unittest.mock import patch
    sample = {**ios.launch_sample(launch_tables()), "trial": 1, "trace": "/t/launch-0001.trace", "exportAttempts": 5}
    with tempfile.TemporaryDirectory() as d:
        stamp = os.path.join(d, "stamp.json")
        with open(stamp, "w") as f:
            json.dump({"commit": "c" * 40, "dirty": False, "sha256": "f" * 64, "artifact": "/no/longer/here.app"}, f)
        with patch.object(ios, 'trace_series', return_value=([sample], [], d)):
            record, failed = ios.run_ios(ios_args(stamp=stamp, expect_commit="ccc"), runner=devicectl_listing)
        assert not failed and record['ranOnHardware'] is True and 'coldUsefulDraw' not in record['metrics']
        assert 'coldLaunch' not in record['metrics'], "a traced run is a diagnostic: never the judged key"
        metric = record['metrics']['coldLaunchTraced']
        assert metric['samplesMs'] == [653.0] and metric['budget']['budget'] == 1000 and metric['exportAttempts'] == 5
        assert metric['startBoundary'] == "Initializing - Process Creation" and "presented" in metric['endBoundary']
        assert record['acceptance']['verdict'] == 'NOT VERIFIED'
        why = " ".join(record['acceptance']['why'])
        assert "configuration is unknown" in why and "no warm return distribution" in why and "below the PRD" in why
        assert json.load(open(os.path.join(d, "series.json")))["class"] == "cold"
        assert not perfcore.check_record(record), perfcore.check_record(record)


@case("I17 --reparse re-reads a retained series with its recorded device and build, capturing nothing")
def _():
    good = launch_tables()
    def exporter(cmd, **kwargs):
        assert cmd[:3] == ["xcrun", "xctrace", "export"], cmd  # nothing records, lists or launches
        table = cmd[cmd.index("--xpath") + 1].split('"')[-2]
        return subprocess.CompletedProcess(cmd, 0, good[table], "")
    with tempfile.TemporaryDirectory() as d:
        for n in (1, 2):
            os.makedirs(os.path.join(d, f"launch-000{n}.trace"))
        with open(os.path.join(d, "series.json"), "w") as f:
            json.dump({"class": "cold", "device": {"kind": "physical", "udid": "u", "model": "iPhone SE", "os": "iOS 26.3.1"},
                       "build": {"bundle": ios.BUNDLE, "commit": "c" * 40, "dirty": False, "configuration": "release"}}, f)
        record, failed = ios.run_ios(ios_args(device=None, reparse=d), runner=exporter)
        assert not failed and record['metrics']['coldLaunchTraced']['samplesMs'] == [653.0, 653.0]
        assert record['device']['model'] == "iPhone SE" and record['ranOnHardware'] is True
        assert "not a release" not in " ".join(record['acceptance']['why'])


@case("P1 pacing waits for four quiet seconds under 1.5 cores on the emulator's host process, gives up at 30 s, says so")
def _():
    with tempfile.TemporaryDirectory() as cache:
        assert perf.emulator_pacer(cache) == (None, [])  # no emulator record: no pacing, no guess
        with open(os.path.join(cache, "emulator.json"), "w") as f:
            json.dump({"pid": 4242, "serial": "emulator-5580"}, f)
        now = [0.0]

        def sleep(s):
            now[0] += s
        cpu = iter([0.0, 6.5, 9.0, 9.4, 9.8, 10.2, 10.6])  # 6.5 cores, 2.5, then 0.4 for four seconds
        pace, waits = perf.emulator_pacer(cache, sleep=sleep, clock=lambda: now[0], cpu_seconds=lambda: next(cpu))
        pace()
        assert waits == [{"waitedSeconds": 6.0, "cores": 0.4, "quiet": True}], waits
        cpu = iter([0.0, 0.4, 0.8, 3.8, 4.2, 4.6, 5.0, 5.4])  # quiet twice, busy, then quiet four times
        pace, waits = perf.emulator_pacer(cache, sleep=sleep, clock=lambda: now[0], cpu_seconds=lambda: next(cpu))
        pace()
        assert waits[0]["waitedSeconds"] == 7.0 and waits[0]["quiet"], waits  # a busy second restarts the count
        busy = iter([float(i * 5) for i in range(100)])  # always 5 cores
        pace, waits = perf.emulator_pacer(cache, sleep=sleep, clock=lambda: now[0], cpu_seconds=lambda: next(busy))
        pace()
        assert waits[0]["quiet"] is False and waits[0]["waitedSeconds"] >= 30, waits


@case("M1 merge: parts of one build (same bytes) on one device become one record; twice-measured, other bytes or dirty refused")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        first, _ = perf.run_android(android_args(tmp, only="seed,cold"), runner=FakeAdb(), sleep=lambda s: None, host=lambda: {}, log=quiet)
        second, _ = perf.run_android(android_args(tmp, only="seed,warm"), runner=FakeAdb(), sleep=lambda s: None, host=lambda: {}, log=quiet)
        first["phases"]["warm"] = "NOT RUN: the device went away"
        first["notMeasured"].append({"what": "warm", "why": "not run: the device went away"})
        merged = perf.merge_records([first, second])
        assert set(merged["metrics"]) == {"coldLaunch", "warmResume"} and merged["metrics"]["warmResume"]["part"] == 2
        assert merged["phases"]["warm"] == "measured (part 2)" and not any(g["what"] == "warm" for g in merged["notMeasured"])
        assert len(merged["parts"]) == 2 and not perfcore.check_record(merged), perfcore.check_record(merged)
        assert "measured in two parts" in raises(perfcore.Refused, perf.merge_records, [first, first])
        other = json.loads(json.dumps(second))
        other["build"]["installedSha256"] = "f" * 64
        assert "build installedSha256 differs" in raises(perfcore.Refused, perf.merge_records, [first, other])
        later = json.loads(json.dumps(second))
        later["build"]["commit"] = "c" * 40  # same bytes, stamped at a later commit: one build
        assert perf.merge_records([first, later])["build"]["commits"] == sorted({first["build"]["commit"], "c" * 40})
        later["build"]["dirty"] = True
        assert "uncommitted" in raises(perfcore.Refused, perf.merge_records, [first, later])
        # idle frames merge per screen; the same screen twice is refused
        a, b = json.loads(json.dumps(first)), json.loads(json.dumps(second))
        a["metrics"]["idleFrames"] = {"method": "m", "screens": {"conversation": {"framesRendered": 0}}, "zeroFrames": True}
        b["metrics"]["idleFrames"] = {"method": "m", "screens": {"pairing": {"framesRendered": 3}}, "zeroFrames": False}
        idle = perf.merge_records([a, b])["metrics"]["idleFrames"]
        assert set(idle["screens"]) == {"conversation", "pairing"} and idle["zeroFrames"] is False, idle
        assert idle["screens"]["pairing"]["part"] == 2
        b["metrics"]["idleFrames"]["screens"] = {"conversation": {"framesRendered": 0}}
        assert "measured in two parts" in raises(perfcore.Refused, perf.merge_records, [a, b])


BASELINES = os.path.abspath(os.path.join(HERE, "..", "..", "..", "docs", "verification"))


@case("B1 every committed baseline record is sound, and its record.json is exactly the merge of its parts")
def _():
    found = 0
    for name in sorted(os.listdir(BASELINES)):
        if not name.endswith("-perf-baseline"):
            continue
        folder = os.path.join(BASELINES, name)
        parts_dir = os.path.join(folder, "parts")
        parts = [os.path.join(parts_dir, p) for p in sorted(os.listdir(parts_dir))] if os.path.isdir(parts_dir) else []
        for path in [os.path.join(folder, "record.json")] + parts:
            with open(path) as f:
                problems = perfcore.check_record(json.load(f))
            assert not problems, (path, problems)
            found += 1
        if parts:
            loaded = []
            for p in parts:
                with open(p) as f:
                    loaded.append(json.load(f))
            with open(os.path.join(folder, "record.json")) as f:
                committed = json.load(f)
            again = perf.merge_records(loaded)
            assert again["metrics"] == committed["metrics"] and again["build"] == committed["build"], name
            assert committed["acceptance"]["verdict"] == "NOT VERIFIED" or committed["device"]["kind"] == "physical", name
    assert found, "no committed baseline found under docs/verification"


# ---------------------------------------------------------------------------------------------
# the mobile CLIs hand the tool what makes a measurement trustworthy
# ---------------------------------------------------------------------------------------------

MOBILE = os.path.abspath(os.path.join(PERF, ".."))


@case("W1 randroid stamps the APK it installs and hands emu perf its recorded serial, lease and that stamp")
def _():
    with open(os.path.join(MOBILE, "native-android", "bin", "randroid")) as f:
        script = f.read()
    install = script[script.index("emu_install() {"):script.index("emu_command() {")]
    assert install.index("perf.py\" stamp") < install.index("install_apk_if_changed"), "the stamp must be written before the install"
    assert '> "$apk.stamp.json"' in install
    perf_verb = script[script.index("    perf)"):script.index(";;", script.index("    perf)"))]
    for needle in ('s="$(serial)"', '--serial "$s"', "--kind emulator", "--owned-by randroid", '--lease "$CACHE"',
                   '--stamp "$OUT/app/outputs/apk/debug/app-debug.apk.stamp.json"'):
        assert needle in perf_verb, needle
    adb_verb = script[script.index("    adb)"):script.index(";;", script.index("    adb)"))]
    assert 'adb_s "$@"' in adb_verb, adb_verb  # the recorded serial only, never a bare adb


@case("W3 the Android install step skips unchanged bytes and never uninstalls or clears data")
def _():
    import hashlib, subprocess, tempfile
    lib = os.path.join(MOBILE, "native-android", "bin", "apk-install.sh")
    with tempfile.TemporaryDirectory() as d:
        apk = os.path.join(d, "app.apk")
        with open(apk, "wb") as f:
            f.write(b"apk-bytes")
        log = os.path.join(d, "calls.log")
        adb = os.path.join(d, "adb")
        with open(adb, "w") as f:
            f.write("#!/usr/bin/env bash\n"
                    "echo \"$*\" >> \"$FAKE_LOG\"\n"
                    "case \"$*\" in\n"
                    "  *'pm path'*) echo 'package:/data/app/x/base.apk';;\n"
                    "  *sha256sum*) echo \"$FAKE_SHA  /data/app/x/base.apk\";;\n"
                    "esac\n")
        os.chmod(adb, 0o755)
        want = hashlib.sha256(b"apk-bytes").hexdigest()

        def run(sha):
            open(log, "w").close()
            out = subprocess.run(["bash", "-c", f'. "{lib}"; install_apk_if_changed "{adb}" S dev.x "{apk}"'],
                                 env={**os.environ, "FAKE_LOG": log, "FAKE_SHA": sha}, capture_output=True, text=True)
            assert out.returncode == 0, out.stderr
            return out.stdout.strip(), open(log).read()

        out, calls = run(want)
        assert out == "skipped" and " install" not in calls, (out, calls)
        out, calls = run("0" * 64)
        assert out == "installed" and "install -r -t" in calls, (out, calls)
        assert "uninstall" not in calls and "pm clear" not in calls, calls
    with open(os.path.join(MOBILE, "native-android", "bin", "randroid")) as f:
        script = f.read()
    assert "install_apk_if_changed" in script and "adb_s install" not in script
    # HERE is native-android/ (the parent of bin/): the sourced path must exist from it.
    assert '. "$HERE/bin/apk-install.sh"' in script, "randroid must source the helper from $HERE/bin"
    for rel in (("native-android", "bin", "randroid"), ("native-android", "bin", "apk-install.sh")):
        with open(os.path.join(MOBILE, *rel)) as f:
            code = [l for l in f if not l.lstrip().startswith("#")]
        assert not any("uninstall" in l or "pm clear" in l for l in code), rel


@case("W2 rios perf goes to perf.py ios before any Swift build")
def _():
    with open(os.path.join(MOBILE, "native-ios", "bin", "rios")) as f:
        script = f.read()
    assert script.index('= "perf" ]') < script.index("swift build"), "perf must not wait on the core's build"
    assert 'perf.py" ios "$@"' in script


# ---------------------------------------------------------------------------------------------
# the start-time benchmark: a build slower than what was already achieved is refused
# ---------------------------------------------------------------------------------------------

import benchmark  # noqa: E402
import condition  # noqa: E402

PERF_PY = os.path.join(PERF, "perf.py")
# The fixture class's condition: the made-up conversation of 100 rows, the Mac unreachable, release.
COND = condition.for_files(100, "release", "fixture")
KEEP = object()


def bench_record(cold=None, warm=None, model="Fixture Phone", kind="physical", built="a" * 64, dirty=False, cond=KEEP):
    """A sound record of the fixture class; `cold` and `warm` are sample lists (None leaves the metric out).
    `cond` is its condition (default the fixture class's; None writes none)."""
    r = {"schema": perfcore.SCHEMA, "platform": "android", "startedAt": "2026-10-01T12:00:00Z",
         "build": {"commit": "b" * 40, "dirty": dirty, "builtSha256": built, "installedSha256": built, "configuration": "release"},
         "device": {"kind": kind, "model": model}, "route": {"name": "managed"},
         "conditions": {"networkCondition": "mac-unreachable"}, "metrics": {}, "notMeasured": [],
         "acceptance": {"verdict": "NOT VERIFIED", "why": ["fixture"]}}
    if cond is not None:
        r["condition"] = json.loads(json.dumps(COND if cond is KEEP else cond))
    for name, samples in (("coldLaunch", cold), ("warmResume", warm)):
        if samples is not None:
            r["metrics"][name] = {"method": "fixture", "samplesMs": samples, "stats": perfcore.stats(samples)}
    return r


def p95_of(value, n=100):
    """n samples whose nearest-rank p95 is exactly `value` (the 95th of 100 sorted)."""
    return [value - 50.0] * 94 + [value] * 6


def bench_file(tmp, cold=800.0, warm=100.0, cold_allow=5.0, warm_allow=10.0):
    source = {"record": "fixture.json", "recordSha256": "0" * 64, "commit": "c" * 40, "date": "2026-09-24"}
    def entry(p95, allow):
        return {"p95Ms": p95, "allowancePercent": allow, "source": source,
                "noiseSeries": [dict(source, n=100, p95Ms=p95, bootstrapSeMs=1.0, twoRunBoundMs=1.0, twoRunBoundPercent=allow)]}
    bench = {"schema": benchmark.SCHEMA, "classes": [
        {"name": "fixture-phone", "match": {"platform": "android", "device.kind": "physical", "device.model": "Fixture Phone",
                                             "build.configuration": "release"}, "condition": COND,
         "metrics": {"coldLaunch": entry(cold, cold_allow), "warmResume": entry(warm, warm_allow)}},
        {"name": "fixture-never", "match": {"platform": "ios", "device.kind": "physical"}, "condition": COND, "metrics": {},
         "neverEstablished": "never measured as a distribution: fixture"}]}
    path = os.path.join(tmp, "benchmarks.json")
    with open(path, "w") as f:
        json.dump(bench, f)
    return path


def write(tmp, name, obj):
    path = os.path.join(tmp, name)
    with open(path, "w") as f:
        json.dump(obj, f)
    return path


def run_perf(*argv):
    return subprocess.run([sys.executable, PERF_PY, *argv], capture_output=True, text=True,
                          env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})


@case("G1 a record slower than the benchmark by more than its noise allowance is refused (exit 4), every metric reported")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)  # cold 800 ms + 5% -> limit 840; warm 100 ms + 10% -> limit 110
        result = benchmark.compare([bench_record(cold=p95_of(841.0), warm=p95_of(100.0))], benchmark.load(path))
        assert result["verdict"] == benchmark.VERDICT_SLOWER, result
        cold, warm = result["metrics"]["coldLaunch"], result["metrics"]["warmResume"]
        assert cold["status"] == "SLOWER" and cold["limitMs"] == 840.0 and cold["deltaMs"] == 41.0, cold
        assert warm["status"] == "WITHIN NOISE", warm
        assert cold["benchmarkSource"] == {"record": "fixture.json", "commit": "c" * 40, "date": "2026-09-24"}
        out = run_perf("compare", write(tmp, "slow.json", bench_record(cold=p95_of(841.0), warm=p95_of(100.0))), "--benchmark", path)
        assert out.returncode == 4, (out.returncode, out.stdout, out.stderr)
        assert re.search(r"^coldLaunch\s+SLOWER", out.stdout, re.M) and re.search(r"^warmResume\s+WITHIN NOISE", out.stdout, re.M), out.stdout
        assert "verdict: SLOWER THAN THE ESTABLISHED BENCHMARK" in out.stdout, out.stdout


@case("G2 an equal or faster record passes (exit 0); faster says so and never changes the benchmark file")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)
        before = open(path).read()
        for cold, want in ((800.0, "WITHIN NOISE"), (840.0, "WITHIN NOISE"), (700.0, "FASTER")):
            result = benchmark.compare([bench_record(cold=p95_of(cold), warm=p95_of(100.0))], benchmark.load(path))
            assert result["verdict"] == benchmark.VERDICT_OK and result["metrics"]["coldLaunch"]["status"] == want, (cold, result)
        assert "benchmark-update" in result["metrics"]["coldLaunch"]["note"], result
        out = run_perf("compare", write(tmp, "fast.json", bench_record(cold=p95_of(700.0), warm=p95_of(90.0))), "--benchmark", path)
        assert out.returncode == 0 and out.stdout.count("FASTER") == 2, (out.returncode, out.stdout, out.stderr)
        assert open(path).read() == before, "a compare must never write the benchmark"
        js = run_perf("compare", os.path.join(tmp, "fast.json"), "--benchmark", path, "--json")
        assert json.loads(js.stdout)["verdict"] == "NOT SLOWER", js.stdout


@case("G3 a missing metric, a series too short for a p95, an unmatched device or a never-measured class is NOT COMPARED, said why")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)
        bench = benchmark.load(path)
        cold_only = benchmark.compare([bench_record(cold=p95_of(800.0))], bench)
        assert cold_only["verdict"] == benchmark.VERDICT_OK, cold_only
        assert cold_only["metrics"]["warmResume"] == {"status": "NOT COMPARED", "why": "the record has no warmResume"}, cold_only
        short = benchmark.compare([bench_record(cold=[500.0] * 5, warm=[90.0] * 5)], bench)
        assert short["verdict"] == benchmark.VERDICT_NONE and "cannot place a p95" in short["metrics"]["coldLaunch"]["why"], short
        other = benchmark.compare([bench_record(cold=p95_of(800.0), model="Another Phone")], bench)
        assert other["verdict"] == benchmark.VERDICT_NONE and other["class"] is None, other
        assert "device.model is 'Another Phone'" in other["metrics"]["coldLaunch"]["why"], other
        never = bench_record(cold=p95_of(800.0))
        never["platform"] = "ios"
        never_result = benchmark.compare([never], bench)
        assert never_result["metrics"]["coldLaunch"]["why"] == "never measured as a distribution: fixture", never_result
        out = run_perf("compare", write(tmp, "short.json", bench_record(cold=[500.0] * 5)), "--benchmark", path)
        assert out.returncode == 5 and "NOT COMPARED" in out.stdout and "verdict: NOT COMPARED" in out.stdout, (out.returncode, out.stdout)


@case("G4 an iOS cold series and its warm series are judged as one build; other bytes or a metric twice are refused")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)
        cold, warm = bench_record(cold=p95_of(800.0)), bench_record(warm=p95_of(130.0))
        pair = benchmark.compare([cold, warm], benchmark.load(path))
        assert pair["metrics"]["coldLaunch"]["status"] == "WITHIN NOISE" and pair["metrics"]["warmResume"]["status"] == "SLOWER", pair
        other = bench_record(warm=p95_of(100.0), built="f" * 64)
        assert "different build bytes" in raises(perfcore.Refused, benchmark.compare, [cold, other], benchmark.load(path))
        assert "appears in two records" in raises(perfcore.Refused, benchmark.compare, [cold, cold], benchmark.load(path))
        out = run_perf("compare", write(tmp, "c.json", cold), write(tmp, "c2.json", cold), "--benchmark", path)
        assert out.returncode == 3 and "appears in two records" in out.stderr, (out.returncode, out.stderr)


@case("G5 the noise allowance is reproducible from the samples; no benchmark file is committed in this public tree")
def _():
    samples = [float(800 + (i * 37) % 120) for i in range(98)]
    a, b = benchmark.p95_noise(samples), benchmark.p95_noise(list(samples))
    assert a == b and a["n"] == 98 and a["twoRunBoundPercent"] > 0, (a, b)
    assert abs(a["twoRunBoundMs"] - 1.96 * 2 ** 0.5 * a["bootstrapSeMs"]) < 0.02, a
    assert benchmark.p95_noise(samples[:19]) is None
    assert benchmark.allowance([{"twoRunBoundPercent": x} for x in (3.53, 2.9, 7.37)]) == 3.53
    # phone-measured numbers are private (richos-hq): the default is outside this repository and no
    # benchmark file is tracked in the public perf directory
    assert not os.path.realpath(benchmark.DEFAULT).startswith(os.path.realpath(benchmark.REPO) + os.sep), benchmark.DEFAULT
    assert not os.path.exists(os.path.join(PERF, "benchmarks.json")), "a benchmark file in the public perf directory"


@case("G6 benchmark-update establishes, raises when faster, keeps when slower unless a reason is given, refuses a dirty build")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)
        bench = benchmark.load(path)
        bench["classes"][0]["metrics"] = {}
        bench["classes"][0]["neverEstablished"] = "fixture"
        first = write(tmp, "first.json", bench_record(cold=p95_of(800.0), warm=p95_of(100.0)))
        changes = benchmark.update(bench, [first])
        cls = bench["classes"][0]
        assert any("established at 800.0" in c for c in changes) and "neverEstablished" not in cls, changes
        slower = write(tmp, "slower.json", bench_record(cold=p95_of(820.0), warm=p95_of(100.0)))
        changes = benchmark.update(bench, [slower])
        assert cls["metrics"]["coldLaunch"]["p95Ms"] == 800.0 and len(cls["metrics"]["coldLaunch"]["noiseSeries"]) == 2, changes
        assert any("kept 800.0" in c for c in changes), changes
        faster = write(tmp, "faster.json", bench_record(cold=p95_of(760.0), warm=p95_of(100.0)))
        changes = benchmark.update(bench, [faster])
        assert cls["metrics"]["coldLaunch"]["p95Ms"] == 760.0 and cls["metrics"]["coldLaunch"]["source"]["record"].endswith("faster.json")
        assert any("raised 800.0 -> 760.0" in c for c in changes), changes
        changes = benchmark.update(bench, [slower], allow_slower="the phone's OS update")
        assert cls["metrics"]["coldLaunch"]["p95Ms"] == 820.0 and cls["metrics"]["coldLaunch"]["loweredBecause"] == "the phone's OS update"
        assert len(cls["metrics"]["coldLaunch"]["noiseSeries"]) == 3, "a record already in the series is replaced, not added twice"
        dirty = write(tmp, "dirty.json", bench_record(cold=p95_of(700.0), dirty=True))
        assert "uncommitted" in raises(perfcore.Refused, benchmark.update, bench, [dirty])
        stranger = write(tmp, "stranger.json", bench_record(cold=p95_of(700.0), model="Another Phone"))
        assert "add the class" in raises(perfcore.Refused, benchmark.update, bench, [stranger])
        out = run_perf("benchmark-update", faster, "--benchmark", path)
        assert out.returncode == 0 and "commit it" in out.stdout, (out.returncode, out.stdout, out.stderr)
        assert benchmark.load(path)["classes"][0]["metrics"]["coldLaunch"]["p95Ms"] == 760.0


@case("G7 every record perf.py writes is judged: merge and a measurement run exit 4 when slower and the record reads REFUSED")
def _():
    parts_dir = os.path.join(BASELINES, "2026-09-24-richconnect-android-perf-baseline", "parts")
    with tempfile.TemporaryDirectory() as tmp:
        # the baseline's own condition (40 bridge rows, the scripted Mac unreachable, Debug), written into copies
        cond = condition.for_bridge(40, android.history_frame(40), "unreachable", "debug")
        parts = []
        for p in sorted(os.listdir(parts_dir)):
            with open(os.path.join(parts_dir, p)) as f:
                part = json.load(f)
            part["condition"] = cond
            parts.append(write(tmp, "part-" + p, part))
        # a fixture benchmark of the baseline's own class, established from the baseline itself
        absent = os.path.join(tmp, "absent.json")
        out = run_perf("merge", *parts, "--out", os.path.join(tmp, "first.json"), "--benchmark", absent)
        assert out.returncode == 0 and "no private benchmark file" in out.stderr, (out.returncode, out.stderr)
        with open(os.path.join(tmp, "first.json")) as f:
            first = json.load(f)
        match = {k: benchmark.lookup(first, k) for k in ("platform", "device.kind", "device.model", "build.configuration")}
        fixture = {"schema": benchmark.SCHEMA, "classes": [{"name": "fixture-emulator", "match": match, "condition": cond,
                                                              "metrics": {}, "neverEstablished": "fixture"}]}
        benchmark.update(fixture, [os.path.join(tmp, "first.json")])
        same_bench = write(tmp, "same-bench.json", fixture)
        out = run_perf("merge", *parts, "--out", os.path.join(tmp, "same.json"), "--benchmark", same_bench)
        assert out.returncode == 0, (out.returncode, out.stderr)
        with open(os.path.join(tmp, "same.json")) as f:
            same = json.load(f)
        assert same["benchmark"]["class"] == "fixture-emulator", same["benchmark"]
        assert {r["status"] for r in same["benchmark"]["metrics"].values()} == {"WITHIN NOISE"}, same["benchmark"]
        assert "benchmark: coldLaunch" in out.stderr and "benchmark: warmResume" in out.stderr, out.stderr
        # against a benchmark this emulator has to beat
        stricter = benchmark.load(same_bench)
        emu = stricter["classes"][0]
        emu["metrics"]["coldLaunch"]["p95Ms"] = 1000
        strict = write(tmp, "strict.json", stricter)
        out = run_perf("merge", *parts, "--out", os.path.join(tmp, "slow.json"), "--benchmark", strict)
        assert out.returncode == 4, (out.returncode, out.stderr)
        with open(os.path.join(tmp, "slow.json")) as f:
            slow = json.load(f)
        assert slow["acceptance"]["verdict"].startswith("REFUSED") and "coldLaunch p95 2105" in slow["acceptance"]["why"][0], slow["acceptance"]
        assert not perfcore.check_record(slow), perfcore.check_record(slow)
        # a measurement run (android or ios) goes through the same judgment before its record is written
        saved = perf.run_android
        try:
            perf.run_android = lambda args: (bench_record(cold=p95_of(841.0), warm=p95_of(100.0)), 0)
            path = bench_file(tmp)
            code = perf.main(["android", "--adb", "adb", "--serial", "S", "--kind", "physical",
                              "--out", os.path.join(tmp, "run.json"), "--benchmark", path])
        finally:
            perf.run_android = saved
        with open(os.path.join(tmp, "run.json")) as f:
            run = json.load(f)
        assert code == 4 and run["benchmark"]["metrics"]["coldLaunch"]["status"] == "SLOWER", (code, run.get("benchmark"))
        assert run["acceptance"]["verdict"].startswith("REFUSED"), run["acceptance"]


@case("G9 no private benchmark file (a public clone): every metric NOT COMPARED with the reason, exit 5, never a pass")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        rec = write(tmp, "r.json", bench_record(cold=p95_of(800.0), warm=p95_of(100.0)))
        absent = os.path.join(tmp, "nope.json")
        out = run_perf("compare", rec, "--benchmark", absent)
        assert out.returncode == 5, (out.returncode, out.stdout, out.stderr)
        assert out.stdout.count("NOT COMPARED") >= 3 and out.stdout.count("no private benchmark file") >= 2, out.stdout
        js = json.loads(run_perf("compare", rec, "--benchmark", absent, "--json").stdout)
        assert js["verdict"] == benchmark.VERDICT_NONE and all(
            r["status"] == "NOT COMPARED" and r["why"] == "no private benchmark file" for r in js["metrics"].values()), js
        # the environment variable is the documented setting
        good = bench_file(tmp)
        env = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1", benchmark.ENV_VAR: good}
        out = subprocess.run([sys.executable, PERF_PY, "compare", rec], capture_output=True, text=True, env=env)
        assert out.returncode == 0 and "WITHIN NOISE" in out.stdout, (out.returncode, out.stdout)
        env[benchmark.ENV_VAR] = absent
        out = subprocess.run([sys.executable, PERF_PY, "compare", rec], capture_output=True, text=True, env=env)
        assert out.returncode == 5 and "no private benchmark file" in out.stdout, (out.returncode, out.stdout)
        # a record the tool writes is stamped NOT COMPARED too, and is not refused
        got = {}
        slower = perf.judge(bench_record(cold=p95_of(800.0)), absent, log=lambda line: got.setdefault("log", line))
        assert slower is False and "no private benchmark file" in got["log"], got


@case("G8 a record judged slower whose acceptance is not REFUSED is unsound")
def _():
    r = bench_record(cold=p95_of(900.0))
    r["benchmark"] = {"verdict": benchmark.VERDICT_SLOWER}
    assert any("not REFUSED" in p for p in perfcore.check_record(r)), perfcore.check_record(r)
    r["acceptance"]["verdict"] = "REFUSED: slower than the established benchmark"
    assert not perfcore.check_record(r), perfcore.check_record(r)


# ---------------------------------------------------------------------------------------------
# conditions: a record is compared only with benchmarks taken under the same condition
# ---------------------------------------------------------------------------------------------

def other_condition(**change):
    """COND with fields changed: rows=40, mac="reachable", build="debug", fixture=..."""
    c = json.loads(json.dumps(COND))
    for key, value in change.items():
        (c["conversation"] if key in ("rows", "fixture", "sha256") else c)[key] = value
    return c


@case("C1 same condition and slower is REFUSED (exit 4); a different condition or none is NOT COMPARED (exit 5), never a pass or a fail")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)  # cold 800 ms + 5% -> limit 840, taken under COND
        slow = write(tmp, "slow.json", bench_record(cold=p95_of(900.0), warm=p95_of(100.0)))
        out = run_perf("compare", slow, "--benchmark", path)
        assert out.returncode == 4 and re.search(r"^coldLaunch\s+SLOWER", out.stdout, re.M), (out.returncode, out.stdout)
        assert "condition synthetic-conversation/1 x100, Mac unreachable, release build" in out.stdout, out.stdout
        for change, said in (({"rows": 40}, "conversation.rows is 40 here, 100 in the benchmark"),
                             ({"mac": "reachable"}, "mac is 'reachable' here, 'unreachable' in the benchmark"),
                             ({"sha256": "f" * 64}, "conversation.sha256"),
                             ({"fixture": "devbridge-hello/1"}, "conversation.fixture")):
            for cold in (900.0, 700.0):  # slower and faster: neither fails nor passes across conditions
                rec = write(tmp, "other.json", bench_record(cold=p95_of(cold), warm=p95_of(100.0), cond=other_condition(**change)))
                out = run_perf("compare", rec, "--benchmark", path)
                assert out.returncode == 5 and "verdict: NOT COMPARED" in out.stdout, (change, cold, out.returncode, out.stdout)
                assert out.stdout.count("different conditions") == 2 and said in out.stdout, (change, out.stdout)
                assert "SLOWER" not in out.stdout and "FASTER" not in out.stdout and "WITHIN" not in out.stdout, out.stdout
        none = write(tmp, "none.json", bench_record(cold=p95_of(900.0), warm=p95_of(100.0), cond=None))
        out = run_perf("compare", none, "--benchmark", path)
        assert out.returncode == 5 and "the record names no condition" in out.stdout, (out.returncode, out.stdout)
        loose = write(tmp, "loose.json", bench_record(cold=p95_of(900.0), cond=condition.as_installed("reachable", "release", "x")))
        out = run_perf("compare", loose, "--benchmark", path)
        assert out.returncode == 5 and "as-installed" in out.stdout and "different conditions" in out.stdout, out.stdout
        unseen = json.loads(json.dumps(COND))
        unseen["verified"] = {"row": "Perf probe 50", "onScreen": False}
        rec = write(tmp, "unseen.json", bench_record(cold=p95_of(900.0), cond=unseen))
        out = run_perf("compare", rec, "--benchmark", path)
        assert out.returncode == 5 and "was not on screen" in out.stdout, out.stdout
        # a measurement run's own judgment: a record under another condition is never REFUSED
        rec = bench_record(cold=p95_of(900.0), cond=other_condition(rows=40))
        assert perf.judge(rec, path, log=quiet) is False and rec["benchmark"]["verdict"] == benchmark.VERDICT_NONE
        assert not str(rec["acceptance"]["verdict"]).startswith("REFUSED"), rec["acceptance"]


@case("C2 the seeded conversation is the 2026-10-02 fixture byte for byte; the default sizes are the benchmarks'")
def _():
    files = condition.file_fixture(100)
    import hashlib
    # andy-opus-coldstart1's genstate.py 100 produced these bytes (sizes 413 and 38016 in his seed logs)
    assert {n: (len(b), hashlib.sha256(b).hexdigest()) for n, b in files.items()} == {
        "session.json": (413, "02d1ed15a951637d5e4732a27af47f9ba7dd86a763c6672827899a0dcf72529f"),
        "history.json": (38016, "b17f4c2b62260ecd699a3e639c8f0b4b908e34dc18ba62bc87635b19a269fa0b")}, files.keys()
    history = json.loads(files["history.json"])
    assert len(history["rows"]["perf-thread"]) == 100 and ".invalid" in history["identity"]  # never resolves: Mac unreachable
    assert condition.for_files(100, "release", "a") ["conversation"]["sha256"] == COND["conversation"]["sha256"]
    assert condition.file_fixture(40) != condition.file_fixture(100)
    assert condition.file_fixture_marker(100) == "Perf probe 50: what is on my plate this afternoon?"
    assert (condition.FILE_DEFAULT_ROWS, condition.BRIDGE_DEFAULT_ROWS) == (100, 40)
    assert "Synthetic message" in json.dumps(android.history_frame(2))  # synthetic text only
    raises(condition.ConditionError, condition.file_fixture, 0)


SIGNER_OF = {}  # local APK path -> certificate digest; anything else reads as the one upload key


def fake_signer(apk, apksigner):
    return SIGNER_OF.get(apk, "a" * 64)


android.signer_sha256 = fake_signer  # apksigner reads real APKs; the fake phone's are bytes


def release_setup(tmp):
    """A stamped release APK and its debuggable twin, as local files the fake adb installs."""
    import hashlib
    SIGNER_OF.clear()
    release, twin = os.path.join(tmp, "release.apk"), os.path.join(tmp, "twin.apk")
    for path, data in ((release, b"release-bytes"), (twin, b"twin-bytes")):
        with open(path, "wb") as f:
            f.write(data)
    rsha, tsha = hashlib.sha256(b"release-bytes").hexdigest(), hashlib.sha256(b"twin-bytes").hexdigest()
    stamp = write(tmp, "release-stamp.json", {"commit": "b4b1b517011837108979b5e690cb84105fef6a53", "dirty": False,
                                               "sha256": rsha, "artifact": release, "paths": ["richos/mobile/native-android"]})
    fake = FakeAdb(installed=rsha, qemu="0", debuggable=False, apks={release: (rsha, False), twin: (tsha, True)})
    fake.contents = {rsha: b"release-bytes"}
    fake.flags = {rsha: False, tsha: True}
    return fake, stamp, twin


def fake_release(stamp):
    return json.load(open(stamp))["artifact"]


@case("C3 Android: a release build is seeded through its twin, read back, and the record states the condition; without a twin it is refused untouched")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, stamp, twin = release_setup(tmp)
        args = android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None)
        msg = raises(perfcore.Refused, perf.run_android, args, runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert "--seed-twin" in msg and "as-installed" in msg, msg
        assert not any(c.startswith(("uninstall", "install", "am start", "run-as")) for c in fake.calls), fake.calls
        assert "reachable" in raises(perfcore.Refused, perf.run_android, android_args(
            tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None, seed_twin=twin, mac="reachable"),
            runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert not any(c.startswith(("uninstall", "install")) for c in fake.calls), fake.calls
        fake.ui_text = "Perf probe 50: what is on my plate this afternoon?"
        args = android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None, seed_twin=twin)
        from unittest.mock import patch
        cold = {"first": [500], "useful": [700], "rejected": [], "presentationSamples": []}
        def fake_cold(self, trials, marker=None, physical=False):
            nodes = self.dump_ui()
            return dict(cold, screenCheck={"composerOnScreen": True,
                                           "newestMessageOnScreen": android.find_node(nodes, contains=marker) is not None})
        with patch.object(android.Measure, "cold", fake_cold):
            record, failures_ = perf.run_android(args, runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        installs = [c for c in fake.calls if c.startswith(("uninstall", "install"))]
        assert all(c.startswith("install -r ") for c in installs), installs  # never an uninstall, data kept every time
        assert installs[0].endswith("twin.apk") and installs[2].endswith(fake_release(stamp)), installs
        assert installs[-1].endswith("base.apk"), installs  # the build found on the phone, put back from the saved copy
        assert record["savedState"]["restored"] is True and record["savedState"]["dataLostBy"] is None, record
        assert failures_ == 0 and record["build"]["configuration"] == "release", (failures_, record["phases"])
        # CEO 2026-10-03: a physical phone is measured only through the TEST COPY; no command names his own app
        import re as _re
        assert record["build"]["package"] == "dev.richos.connect.perf", record["build"]
        own_app = [c for c in fake.calls if _re.search(r"dev\.richos\.connect(?!\.perf)", c)]
        assert not own_app, own_app
        cond = record["condition"]
        assert (cond["conversation"]["fixture"], cond["conversation"]["rows"], cond["mac"], cond["build"]) == \
            ("synthetic-conversation/1", 100, "unreachable", "release"), cond
        assert condition.same(cond, COND) and cond["verified"]["onScreen"] is True, cond
        assert "debuggable twin" in cond["conversation"]["seededBy"] and record["route"]["name"] == "seeded fixture"
        assert record["conditions"]["networkCondition"] == "mac-unreachable" and not perfcore.check_record(record)
        # the app read something else: the condition stands but is never compared
        fake.ui_text = "Pair with your Mac"
        with patch.object(android.Measure, "cold", fake_cold):
            record, _ = perf.run_android(args, runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert record["condition"]["verified"]["onScreen"] is False
        assert "not on screen" in condition.why_not_comparable(record["condition"], COND)
        # --conversation as-installed is said in the record and never compared
        loose, _ = perf.run_android(android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp,
                                                 conversation="as-installed", network_condition="live"),
                                    runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert loose["condition"]["conversation"]["fixture"] == "as-installed" and loose["condition"]["mac"] == "reachable"


@case("C4 a debuggable build without its bridge is seeded through run-as; --only never skips the seeding a condition depends on")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeAdb()
        record, failures_ = perf.run_android(android_args(tmp, production=True, route="managed", only="cold", rows=None),
                                             runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert record["phases"].get("seed") == "measured" and fake.core == {}, record["phases"]  # the app's own data is back
        assert any(c.startswith("push") and c.endswith("richos-perf-session.json") for c in fake.calls)  # the fixture was written
        assert record["condition"]["conversation"]["fixture"] == "synthetic-conversation/1" and record["condition"]["build"] == "debug"
        assert record["savedState"]["restored"] is True, record["savedState"]
        bridge, _ = perf.run_android(android_args(tmp, only="cold"), runner=FakeAdb(), sleep=lambda s: None, log=quiet, host=lambda: {})
        assert bridge["phases"].get("seed") == "measured" and bridge["condition"]["conversation"]["rows"] == 4, bridge["phases"]


@case("C5 a condition declaration states the condition of exactly the records it names, never contradicts or replaces one")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        import hashlib
        bench = benchmark.load(bench_file(tmp))
        bench["classes"][0]["metrics"], bench["classes"][0]["neverEstablished"] = {}, "fixture"
        old = write(tmp, "old.json", bench_record(cold=p95_of(800.0), cond=None))
        digest = hashlib.sha256(open(old, "rb").read()).hexdigest()
        assert "names no condition" in raises(perfcore.Refused, benchmark.update, bench, [old])
        decl = write(tmp, "decl.json", {"schema": condition.DECLARATION_SCHEMA, "condition": COND,
                                        "records": [{"record": "old.json", "sha256": digest}],
                                        "evidence": "fixture", "declaredBy": "test", "date": "2026-10-02"})
        decls = [condition.load_declaration(decl)]
        changes = benchmark.update(bench, [old], declarations=decls)
        src = bench["classes"][0]["metrics"]["coldLaunch"]["source"]
        assert any("established at 800.0" in c for c in changes) and src["conditionDeclaration"]["sha256"], src
        other = write(tmp, "other.json", bench_record(cold=p95_of(700.0), cond=None, built="e" * 64))
        assert "names no condition" in raises(perfcore.Refused, benchmark.update, bench, [other], declarations=decls)
        out = run_perf("compare", old, "--benchmark", bench_file(tmp), "--condition-declaration", decl)
        assert out.returncode == 0 and "WITHIN NOISE" in out.stdout, (out.returncode, out.stdout, out.stderr)
        out = run_perf("compare", old, "--benchmark", bench_file(tmp))
        assert out.returncode == 5 and "names no condition" in out.stdout, out.stdout
        own = write(tmp, "own.json", bench_record(cold=p95_of(800.0)))
        listed = write(tmp, "decl2.json", dict(json.load(open(decl)), records=[
            {"record": "own.json", "sha256": hashlib.sha256(open(own, "rb").read()).hexdigest()}]))
        assert "never replaces" in raises(condition.ConditionError, condition.apply, json.load(open(own)), own,
                                          [condition.load_declaration(listed)])
        debug = json.loads(json.dumps(COND))
        debug["build"] = "debug"
        wrong = write(tmp, "decl3.json", dict(json.load(open(decl)), condition=debug))
        assert "contradicts" in raises(perfcore.Refused, benchmark.update, bench, [old],
                                       declarations=[condition.load_declaration(wrong)])
        assert "names no evidence" in raises(condition.ConditionError, condition.load_declaration,
                                             write(tmp, "decl4.json", dict(json.load(open(decl)), evidence="")))


@case("C6 a benchmark file whose class names no condition, or of the old schema, is refused; retired classes are never compared")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)
        bench = json.load(open(path))
        no_cond = dict(bench, classes=[{k: v for k, v in bench["classes"][0].items() if k != "condition"}])
        assert "class fixture-phone names no condition" in raises(perfcore.Refused, benchmark.load, write(tmp, "a.json", no_cond))
        assert "richos-mobile-perf-benchmarks/2" in raises(perfcore.Refused, benchmark.load,
                                                           write(tmp, "b.json", dict(bench, schema="richos-mobile-perf-benchmarks/1")))
        loose = dict(bench, classes=[dict(bench["classes"][0], condition=condition.as_installed("reachable", "release", "x"))])
        assert "never comparable" in raises(perfcore.Refused, benchmark.load, write(tmp, "c.json", loose))
        retired = dict(bench, classes=[], retired=[dict(bench["classes"][0], retiredBecause="different conditions")])
        result = benchmark.compare([bench_record(cold=p95_of(900.0))], benchmark.load(write(tmp, "d.json", retired)))
        assert result["verdict"] == benchmark.VERDICT_NONE and result["class"] is None, result
        assert "retiredBecause" in raises(perfcore.Refused, benchmark.load,
                                          write(tmp, "e.json", dict(retired, retired=[{"name": "x"}])))


@case("C7 iOS: an iPhone's default without a stamp is refused untouched; as-installed states the Mac; merge and a pair refuse mixed conditions")
def _():
    touched = []
    msg = raises(perfcore.Refused, ios.run_ios, ios_args(conversation="fixture"), runner=lambda cmd, **kw: touched.append(cmd))
    assert "seeded only for a stamped build" in msg and not touched, (msg, touched)
    assert "--mac" in raises(perfcore.Refused, ios.run_ios, ios_args(mac=None), runner=devicectl_listing)
    from unittest.mock import patch
    sample = {**ios.launch_sample(launch_tables()), "trial": 1, "trace": "/t/launch-0001.trace", "exportAttempts": 5}
    with tempfile.TemporaryDirectory() as d:
        with patch.object(ios, 'trace_series', return_value=([sample], [], d)):
            record, _ = ios.run_ios(ios_args(mac="unreachable"), runner=devicectl_listing)
        assert record["condition"]["conversation"]["fixture"] == "as-installed" and record["condition"]["mac"] == "unreachable"
        assert json.load(open(os.path.join(d, "series.json")))["condition"] == record["condition"]
    cold, warm = bench_record(cold=p95_of(800.0)), bench_record(warm=p95_of(100.0), cond=other_condition(rows=40))
    with tempfile.TemporaryDirectory() as tmp:
        assert "different conditions" in raises(perfcore.Refused, benchmark.compare, [cold, warm], benchmark.load(bench_file(tmp)))
        first, _ = perf.run_android(android_args(tmp, only="seed,cold"), runner=FakeAdb(), sleep=lambda s: None, host=lambda: {}, log=quiet)
        second, _ = perf.run_android(android_args(tmp, only="seed,warm", rows=6), runner=FakeAdb(), sleep=lambda s: None, host=lambda: {}, log=quiet)
        assert "different conditions" in raises(perfcore.Refused, perf.merge_records, [first, second])


@case("C8 declare-condition computes the condition from the fixture, checks every record, and refuses a contradiction")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        old = write(tmp, "old.json", bench_record(cold=p95_of(800.0), cond=None))
        base = ["declare-condition", "--fixture", "synthetic-conversation/1", "--rows", "100", "--mac", "unreachable",
                "--seeded-by", "fixture", "--evidence", "fixture", "--declared-by", "test", "--date", "2026-10-02"]
        out = run_perf(*base, "--build", "release", old, "--out", os.path.join(tmp, "decl.json"))
        assert out.returncode == 0, (out.returncode, out.stderr)
        decl = condition.load_declaration(os.path.join(tmp, "decl.json"))
        assert condition.same(decl["condition"], COND) and decl["records"][0]["commit"] == "b" * 40, decl
        assert benchmark.record_condition(json.load(open(old)), old, [decl])[0]["name"] == COND["name"]
        out = run_perf(*base, "--build", "debug", old)
        assert out.returncode == 3 and "the record's build is release" in out.stderr, (out.returncode, out.stderr)
        own = write(tmp, "own.json", bench_record(cold=p95_of(800.0)))
        out = run_perf(*base, "--build", "release", own)
        assert out.returncode == 3 and "names its own condition" in out.stderr, (out.returncode, out.stderr)
        live = bench_record(cold=p95_of(800.0), cond=None)
        live["conditions"]["networkCondition"] = "live"
        out = run_perf(*base, "--build", "release", write(tmp, "live.json", live))
        assert out.returncode == 3 and "the Mac was reachable" in out.stderr, (out.returncode, out.stderr)
        # a declaration kept in a richos-hq worktree is named as it will be in richos-hq, not by the worktree's path
        wt = os.path.join(tmp, "richos-hq-wt", "someone")
        os.makedirs(os.path.join(wt, "docs", "mobile-perf", "conditions"))
        with open(os.path.join(wt, ".git"), "w") as f:
            f.write(f"gitdir: {os.path.join(tmp, 'richos-hq', '.git', 'worktrees', 'someone')}\n")
        inside = os.path.join(wt, "docs", "mobile-perf", "conditions", "d.json")
        assert benchmark._display(inside) == os.path.join("richos-hq", "docs", "mobile-perf", "conditions", "d.json")
        with open(os.path.join(wt, ".git"), "w") as f:
            f.write(f"gitdir: {os.path.join(tmp, 'elsewhere', '.git', 'worktrees', 'someone')}\n")
        assert benchmark._display(inside) == inside


# ---------------------------------------------------------------------------------------------
# the iPhone app under the same condition: Android's conversation as its saved state (simulator)
# ---------------------------------------------------------------------------------------------

UDID = "11111111-2222-3333-4444-555555555555"


class FakeSimulator:
    """simctl, `rios perf-seed` and `rios screen-text` for one booted simulator: the app's data
    container is a real directory, so what a launch would read is what is on disk at that moment."""

    def __init__(self, tmp, screen=None, seed_fails=False, read_fails=False):
        self.app = os.path.join(tmp, "RichOSNative.app")
        self.data = os.path.join(tmp, "data")
        os.makedirs(self.app, exist_ok=True)
        with open(os.path.join(self.app, "RichOSNative"), "wb") as f:
            f.write(b"release bytes")  # no development marker: a Release bundle
        self.state = os.path.join(self.data, ios.STATE_DIR)
        os.makedirs(self.state, exist_ok=True)
        with open(os.path.join(self.state, "state.json"), "wb") as f:
            f.write(b'{"the person\'s own":"state"}')  # history.json absent before the run
        self.screen, self.seed_fails, self.read_fails = screen, seed_fails, read_fails
        self.calls, self.fixture_seen, self.written = [], None, None

    def now(self):
        return {n: open(os.path.join(self.state, n), "rb").read() for n in sorted(os.listdir(self.state))}

    def __call__(self, cmd, **kw):
        self.calls.append(cmd)
        ok = lambda out="": subprocess.CompletedProcess(cmd, 0, out, "")
        if cmd[:4] == ["xcrun", "simctl", "list", "devices"]:
            return ok(json.dumps({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-26-3": [
                {"udid": UDID, "state": "Booted", "name": "iPhone 16 Pro"}]}}))
        if cmd[:3] == ["xcrun", "simctl", "get_app_container"]:
            return ok(self.app if cmd[-1] == "app" else self.data)
        if cmd[0] == ios.RIOS and cmd[1] == "perf-seed":
            fixture, expected, out = cmd[2], cmd[3], cmd[4]
            files = {n: open(os.path.join(fixture, n), "rb").read() for n in sorted(os.listdir(fixture))}
            self.fixture_seen = (files, expected)
            if self.seed_fails or condition._manifest_sha256(files) != expected:
                return subprocess.CompletedProcess(cmd, 1, "", json.dumps({"ok": False, "error": "refused"}))
            os.makedirs(out)
            self.written = {"state.json": b'{"schema":2,"seeded":true}', "history.json": b'{"messages":"100 rows"}'}
            for name, data in self.written.items():
                with open(os.path.join(out, name), "wb") as f:
                    f.write(data)
            import hashlib
            sha = lambda b: hashlib.sha256(b).hexdigest()
            return ok(json.dumps({"ok": True, "result": {
                "fixture": "synthetic-conversation/1", "fixtureSha256": expected, "rows": json.loads(files["history.json"])["cursor"] // 2,
                "fixtureFiles": {n: sha(b) for n, b in files.items()}, "written": {n: sha(b) for n, b in self.written.items()}}}))
        if cmd[:4] == ["xcrun", "simctl", "io", UDID]:
            with open(cmd[-1], "wb") as f:
                f.write(b"png")
            return ok()
        if cmd[0] == ios.RIOS and cmd[1] == "screen-text":
            if self.read_fails:
                return subprocess.CompletedProcess(cmd, 1, "", json.dumps({"ok": False, "error": "Vision failed"}))
            return ok(json.dumps({"ok": True, "result": {"lines": self.screen or []}}))
        return ok()  # terminate, launch


def seeded_args(**over):
    return ios_args(**dict(dict(simulator=UDID, device=None, conversation="fixture", mac=None, rows=None,
                                evidence_dir=None, background_seconds=1.0, background_settle=0.0), **over))


def run_seeded(sim, **over):
    """run_ios against `sim`; the cold series and background window are replaced, and the cold series
    records what the app would have read at launch."""
    from unittest.mock import patch
    seen = {}
    def cold(self, trials):
        seen["atLaunch"] = sim.now()
        return [500, 520], []
    with patch.object(ios.Sim, "cold", cold), \
         patch.object(ios.Sim, "background", lambda self, s, t: {"seconds": s, "idleWakeups": 0}):
        record, failed = ios.run_ios(seeded_args(**over), runner=sim, sleep=lambda s: None)
    return record, failed, seen


NEWEST_CEO = "Perf probe 50: what is on my plate this afternoon?"


@case("C9 iOS seeding gives the app exactly Android's fixture, states the same condition, checks the screen and puts the app's own state back")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        sim = FakeSimulator(tmp, screen=["Rich", "Perf probe 50: what is on my plate this", "afternoon?", "Message Rich"])
        before = sim.now()
        stamp = write(tmp, "stamp.json", {"commit": "c" * 40, "dirty": False, "sha256": perfcore.tree_sha256(sim.app)})
        record, failed, seen = run_seeded(sim, stamp=stamp, expect_commit="ccccccc")
        files, expected = sim.fixture_seen
        # what perf-seed was given is Android's fixture byte for byte, named by the condition's SHA-256
        assert files == condition.file_fixture(100) and expected == COND["conversation"]["sha256"], sorted(files)
        # what the app read at launch is exactly what perf-seed wrote: nothing more, nothing else
        assert seen["atLaunch"] == sim.written, seen
        cond = record["condition"]
        assert condition.same(cond, COND) and cond["build"] == "release" and cond["mac"] == "unreachable", cond
        assert cond["conversation"]["fixtureFiles"] == condition.for_files(100, "release", "x")["conversation"]["files"]
        assert set(cond["conversation"]["files"]) == {"history.json", "state.json"} and "rios perf-seed" in cond["conversation"]["seededBy"]
        assert cond["verified"]["onScreen"] is True and cond["verified"]["row"] == NEWEST_CEO, cond["verified"]
        assert condition.why_not_comparable(cond, COND) is None
        assert record["conditions"] == {"fixture": "synthetic-conversation/1", "history": 100,
                                        "networkCondition": "mac-unreachable", "savedStateRestored": True}, record["conditions"]
        assert record["route"]["name"] == "seeded fixture" and not failed, (record["route"], failed)
        assert sim.now() == before, sim.now()  # the person's own state is back; the seeded history is gone
        # the app was terminated before its files changed, and launched for the screen check
        first_write = next(i for i, c in enumerate(sim.calls) if c[:3] == ["xcrun", "simctl", "terminate"])
        assert first_write < next(i for i, c in enumerate(sim.calls) if c[:4] == ["xcrun", "simctl", "io", UDID])
        assert not perfcore.check_record(record), perfcore.check_record(record)


@case("C10 iOS: a seeded record whose conversation was not seen on screen, or whose screen could not be read, is never compared")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        path = bench_file(tmp)
        for sim, said in ((FakeSimulator(tmp + "/a", screen=["Pair with your Mac"]), "was not on screen"),
                          (FakeSimulator(tmp + "/b", read_fails=True), "is unknown")):
            before = sim.now()
            record, failed, _ = run_seeded(sim)
            cond = record["condition"]
            assert failed and cond["verified"]["onScreen"] is not True, cond["verified"]
            assert said in condition.why_not_comparable(cond, COND)
            assert any(g["what"] == "the seeded condition" for g in record["notMeasured"]), record["notMeasured"]
            assert sim.now() == before  # restored either way
            for cold in (900.0, 700.0):  # neither a pass nor a fail: NOT COMPARED, exit 5
                out = run_perf("compare", write(tmp, "r.json", bench_record(cold=p95_of(cold), cond=cond)), "--benchmark", path)
                assert out.returncode == 5 and said in out.stdout and "SLOWER" not in out.stdout, (out.returncode, out.stdout)
        # a verified one is compared under the same benchmark
        record, _, _ = run_seeded(FakeSimulator(tmp + "/c", screen=[NEWEST_CEO]))
        out = run_perf("compare", write(tmp, "r.json", bench_record(cold=p95_of(900.0), cond=record["condition"])), "--benchmark", path)
        assert out.returncode == 4 and re.search(r"^coldLaunch\s+SLOWER", out.stdout, re.M), out.stdout
        # a retained trace series reparsed without its check is unknown too, never a pass
        series = {"class": "cold", "device": {"kind": "simulator"}, "build": {}, "condition": condition.for_files(100, "release", "x")}
        d = os.path.join(tmp, "series")
        os.makedirs(d)
        write(d, "series.json", series)
        from unittest.mock import patch
        with patch.object(ios, "reparse", return_value=([], [{"why": "fixture"}])):
            record, _ = ios.run_ios(ios_args(device=None, reparse=d), runner=lambda *a, **k: None)
        assert "is unknown" in condition.why_not_comparable(record["condition"], COND)


@case("C11 iOS seeding refuses before the simulator changes: a fixture perf-seed rejects, a Debug launch argument, a reachable Mac")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        sim = FakeSimulator(tmp, seed_fails=True)
        before = sim.now()
        assert "rios perf-seed exited 1: refused" in raises(perfcore.Unmeasurable, run_seeded, sim)
        assert sim.now() == before and not any(c[:3] == ["xcrun", "simctl", "terminate"] for c in sim.calls), sim.calls
        for over, said in (({"app_arg": ["-rios-fixture", "conv-long"]}, "launch argument"), ({"mac": "reachable"}, "unreachable")):
            sim = FakeSimulator(tmp + "/" + said.replace(" ", "-"))
            assert said in raises(perfcore.Refused, run_seeded, sim, **over)
            assert sim.calls == [], sim.calls


IOS = os.path.join(MOBILE, "native-ios")


@case("C12 no app build has a seeding path: the app links only the core's products, the seeder is no product, and check-release searches both bundles for its marker")
def _():
    with open(os.path.join(IOS, "Core", "Package.swift")) as f:
        package = f.read()
    products = package[package.index("products: ["):package.index("targets: [")]
    assert "RichOSPerfSeed" not in products, products  # only products can be linked by the app's project
    assert '.target(name: "RichOSPerfSeed", dependencies: ["RichOSCore"]' in package
    with open(os.path.join(IOS, "project.yml")) as f:
        project = f.read()
    assert "PerfSeed" not in project, "the app's project names the seeder"
    for top in ("App", "DevBridge", "NotificationService", "ShareExtension"):
        for root, _, names in os.walk(os.path.join(IOS, top)):
            for name in names:
                if name.endswith(".swift"):
                    with open(os.path.join(root, name)) as f:
                        text = f.read()
                    assert "PerfSeed" not in text and "rios-perf-seed-fixture-writer" not in text, os.path.join(root, name)
    with open(os.path.join(IOS, "Core", "Sources", "RichOSPerfSeed", "PerfSeed.swift")) as f:
        assert 'public static let marker = "rios-perf-seed-fixture-writer"' in f.read()
    with open(os.path.join(IOS, "Core", "Sources", "RichOSCLI", "Simulator.swift")) as f:
        sim = f.read()
    check = sim[sim.index("func checkRelease()"):sim.index("func stop()")]
    assert "seedingMarkers" in check and "rios-cli" in check, "check-release must search both bundles and probe the CLI"
    assert "static let seedingMarkers = [PerfSeed.marker]" in sim
    # longer than Swift's 15-byte small strings, so the literal is stored whole and a byte search can find it
    assert len("rios-perf-seed-fixture-writer") > 15


def keep_setup(tmp, own=None, installed_debuggable=True):
    """A phone holding the CEO's build and saved app data, and the args to seed it by run-as."""
    fake = FakeAdb()
    fake.debuggable = installed_debuggable
    fake.flags = {SHA: installed_debuggable}
    fake.core = dict(own if own is not None else {"session.json": b"the CEO's pairing", "history.json": b"the CEO's conversation"})
    return fake, android_args(tmp, production=True, route="managed", only="cold", rows=None)


def keep_run(args, fake):
    return perf.run_android(args, runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})


@case("K1 run-as seeding saves the app's data and APK first and puts both back, read back by hash, keeping nothing private afterwards")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, args = keep_setup(tmp)
        own, apk = dict(fake.core), fake.contents[SHA]
        record, _ = keep_run(args, fake)
        saved = record["savedState"]
        assert saved["restored"] is True and saved["apkSha256"] == SHA and saved["dataFiles"] == 2, saved
        assert fake.core == own and fake.installed == SHA and fake.contents[SHA] == apk, (sorted(fake.core), fake.installed)
        pulled = [i for i, c in enumerate(fake.calls) if c.startswith("pull ")]
        tar_out = [i for i, c in enumerate(fake.calls) if c.startswith(f"exec-out run-as {android.PACKAGE} tar")]
        written = [i for i, c in enumerate(fake.calls) if c.startswith("push ") and "richos-perf-session" in c]
        assert pulled and tar_out and pulled[0] < written[0] and tar_out[0] < written[0], fake.calls  # saved before the first write
        assert not os.path.isdir(os.path.join(tmp, "keep")) or not os.listdir(os.path.join(tmp, "keep")), "private copy deleted after a verified restore"
        assert not any(c.startswith(("uninstall", "install", "pm clear")) or "pm clear" in c for c in fake.calls), \
            "the app is never uninstalled or its data cleared on a default phone"


@case("K2 a run that fails or is interrupted midway still puts the phone back")
def _():
    from unittest.mock import patch
    with tempfile.TemporaryDirectory() as tmp:
        for boom in (RuntimeError("the measurement blew up"), KeyboardInterrupt()):
            fake, args = keep_setup(tmp)
            own = dict(fake.core)

            def cold(self, trials, marker=None, physical=False):
                raise boom
            with patch.object(android.Measure, "cold", cold):
                try:
                    if isinstance(boom, KeyboardInterrupt):
                        keep_run(args, fake)
                    else:  # phases swallow ordinary failures; make the run itself raise
                        with patch.object(perf, "android_gaps", side_effect=boom):
                            keep_run(args, fake)
                    raise AssertionError("expected the failure to propagate")
                except (RuntimeError, KeyboardInterrupt) as e:
                    assert e is boom
            assert fake.core == own and fake.installed == SHA, (type(boom).__name__, sorted(fake.core))


@case("K3 a release build with no twin to read it through is REFUSED before the phone is touched, unless someone agreed to lose its data")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, stamp, twin = release_setup(tmp)
        fake.core = {"session.json": b"the CEO's pairing"}
        dev = android.Device("/fake/adb", "emulator-5580", runner=fake, sleep=lambda s: None)
        msg = raises(perfcore.Refused, android.StateKeeper(dev, root=os.path.join(tmp, "keep"), log=quiet).save)
        assert "release build" in msg and "--accept-state-loss" in msg, msg
        assert not any(c.startswith(("uninstall", "install", "push", "pull", "run-as", "exec-out", "am force-stop")) for c in fake.calls), fake.calls
        assert not os.path.exists(os.path.join(tmp, "keep")), "nothing was saved either"
        keeper = android.StateKeeper(dev, root=os.path.join(tmp, "keep"), log=quiet, accept_loss="the CEO, in the test")
        keeper.save()
        assert keeper.lost == "the CEO, in the test" and keeper.tar is None
        assert keeper.restore() == [] and fake.core == {"session.json": b"the CEO's pairing"}


@case("K4 an app that is not installed has nothing to lose: seeding goes ahead and nothing is saved")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, stamp, twin = release_setup(tmp)
        keeper = android.StateKeeper(android.Device("/fake/adb", "emulator-5580", runner=fake, sleep=lambda s: None),
                                     root=os.path.join(tmp, "keep"), log=quiet)
        fake.installed = None
        fake.calls.clear()
        keeper.save()
        assert keeper.restore() == [] and not any(c.startswith(("pull", "uninstall", "install")) for c in fake.calls), fake.calls


@case("K5 a restore that cannot put it back says so loudly as the last line, exits non-zero, and keeps the private copy")
def _():
    import io
    from contextlib import redirect_stderr
    with tempfile.TemporaryDirectory() as tmp:
        fake, args = keep_setup(tmp)
        own = dict(fake.core)
        fake.fail = {"tar-x"}  # the saved data will not extract
        record, _ = keep_run(args, fake)
        saved = record["savedState"]
        assert saved["restored"] is False and any("KEPT at" in p for p in saved["problems"]), saved
        kept = os.path.join(tmp, "keep")
        assert os.listdir(kept), "the copy stays when the phone is not whole"
        keep_dir = os.path.join(kept, os.listdir(kept)[0])
        assert os.path.isfile(os.path.join(keep_dir, "data.tar")) and os.path.isfile(os.path.join(keep_dir, "base.apk"))
        assert fake.core != own
        # perf.main: the last stderr line is the loud one, and the exit code is non-zero
        out = os.path.join(tmp, "rec.json")
        from unittest.mock import patch
        err = io.StringIO()
        with patch.object(perf, "run_android", lambda a: (record, 0)), patch.object(perf, "judge", lambda r, b: False), \
                patch.object(perfcore, "check_record", lambda r: []), redirect_stderr(err):
            code = perf.main(["android", "--adb", "/fake/adb", "--serial", "emulator-5580", "--kind", "emulator",
                              "--owned-by", "randroid", "--out", out])
        lines = [l for l in err.getvalue().splitlines() if l.strip()]
        assert code == perf.EXIT_NOT_RESTORED != 0 and lines[-1].startswith("SAVED STATE NOT RESTORED"), (code, lines[-1:])
        # a failure to install the saved APK is the same


@case("K8 a twin signed with another key is refused and reported: nothing uninstalled, the app and its data untouched")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, stamp, twin = release_setup(tmp)
        own = {"session.json": b"the CEO's pairing", "history.json": b"the CEO's conversation"}
        fake.core = dict(own)
        fake.bad_signature = {twin}
        args = android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None, seed_twin=twin)
        msg = raises(perfcore.Refused, keep_run, args, fake)
        assert "NOT uninstalled" in msg and "INSTALL_FAILED_UPDATE_INCOMPATIBLE" in msg, msg
        assert not any(c.startswith("uninstall") for c in fake.calls), fake.calls
        assert fake.installed == json.load(open(stamp))["sha256"] and fake.core == own, "the app and its data are as found"


@case("K9 --seed-twin on a release phone holding data never uninstalls: install -r the twin, write, install -r back, data restored")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        from unittest.mock import patch
        fake, stamp, twin = release_setup(tmp)
        own = {"session.json": b"the CEO's pairing", "history.json": b"the CEO's conversation"}
        fake.core = dict(own)
        fake.ui_text = "Perf probe 50: what is on my plate this afternoon?"
        args = android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None, seed_twin=twin)
        cold = {"first": [500], "useful": [700], "rejected": [], "presentationSamples": []}

        def fake_cold(self, trials, marker=None, physical=False):
            seeded = dict(fake.core)  # what the app would have read while measured
            assert seeded == condition.file_fixture(100), sorted(seeded)
            return dict(cold, screenCheck={"composerOnScreen": True, "newestMessageOnScreen": True})
        with patch.object(android.Measure, "cold", fake_cold):
            record, failures_ = keep_run(args, fake)
        assert not any(c.startswith("uninstall") or "pm clear" in c for c in fake.calls), fake.calls
        assert record["savedState"]["restored"] is True and fake.core == own, (record["savedState"], sorted(fake.core))
        assert fake.installed == json.load(open(stamp))["sha256"] and not fake.debuggable, "the release build is back"


@case("T1 a twin signed with a different key than the installed app is refused with a plain sentence BEFORE any install; nothing uninstalled, data untouched")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake, stamp, twin = release_setup(tmp)
        own = {"session.json": b"the CEO's pairing", "history.json": b"the CEO's conversation"}
        fake.core = dict(own)
        SIGNER_OF[twin] = "b" * 64
        args = android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None, seed_twin=twin)
        msg = raises(perfcore.Refused, keep_run, args, fake)
        assert "signed with a different key" in msg and "CEO's decision" in msg, msg
        assert not any(c.startswith(("uninstall", "install")) for c in fake.calls), fake.calls
        assert fake.installed == json.load(open(stamp))["sha256"] and not fake.debuggable and fake.core == own


@case("T2 the twin is never left installed: a seeding that fails after the twin went on puts the release build back over it (install -r, data kept)")
def _():
    from unittest.mock import patch
    with tempfile.TemporaryDirectory() as tmp:
        fake, stamp, twin = release_setup(tmp)
        own = {"session.json": b"the CEO's pairing", "history.json": b"the CEO's conversation"}
        fake.core = dict(own)
        args = android_args(tmp, kind="physical", production=True, route="managed", only="cold", stamp=stamp, rows=None, seed_twin=twin)
        with patch.object(android, "write_core_files", side_effect=perfcore.Unmeasurable("the write failed")):
            raises(perfcore.Unmeasurable, keep_run, args, fake)
        installs = [c for c in fake.calls if c.startswith(("uninstall", "install"))]
        assert all(c.startswith("install -r ") for c in installs) and installs[0].endswith("twin.apk"), installs
        assert fake.installed == json.load(open(stamp))["sha256"] and not fake.debuggable, "the release build is back, not the twin"
        assert fake.core == own, "the app's data is as found"


@case("T3 randroid: the twin is a build type with the release code, debuggable, the upload key; the bundle and release checks refuse a debuggable build; the scan refuses the twin's Gradle install task")
def _():
    gradle = open(os.path.join(MOBILE, "native-android/app/build.gradle.kts")).read()
    twin = gradle[gradle.index('create("seedTwin")'):]
    twin = twin[:twin.index("\n        }")]
    assert 'initWith(getByName("release"))' in twin and "isDebuggable = true" in twin, twin
    assert 'signingConfigs.findByName("upload")' in twin and 'applicationIdSuffix = ".perf"' in twin, twin  # the twin of the TEST COPY
    cli = open(os.path.join(MOBILE, "native-android/bin/randroid")).read()
    assert '"notDebuggable": scan["debuggable"] is False' in cli and "ok = clean(r) and clean(b) and probe and ids and notdebug" in cli
    assert "REFUSED: the debuggable twin must carry the upload key" in cli and ":app:assembleSeedTwin" in cli
    sys.path.insert(0, MOBILE)
    import physical
    planted = "./gradlew :app:install" + "SeedTwin"  # the rule must name it, and the line is not itself one
    assert "EVERY attached device" in physical.rule_for(planted, True)


@case("K6 an exception that ends the run AND a failed restore raise RestoreFailed (exit non-zero, loud last line), the cause kept")
def _():
    from unittest.mock import patch
    with tempfile.TemporaryDirectory() as tmp:
        fake, args = keep_setup(tmp)
        fake.fail = {"tar-x"}
        with patch.object(perf, "android_gaps", side_effect=RuntimeError("boom")):
            try:
                keep_run(args, fake)
                raise AssertionError("expected RestoreFailed")
            except perf.RestoreFailed as e:
                assert isinstance(e.__cause__, RuntimeError) and "KEPT at" in str(e), e


@case("K7 the seeding docstrings and the README no longer say the saved state is lost")
def _():
    for path in (os.path.join(PERF, "android.py"), os.path.join(PERF, "README.md")):
        text = open(path).read()
        assert "previous saved\n    state for this app is gone" not in text, path
        assert "replaces the app's saved state on the phone" not in text, path
    readme = open(os.path.join(PERF, "README.md")).read()
    assert "--accept-state-loss" in readme and "--disposable-phone" not in readme and "install -r" in readme


@case("DV1 a physical phone is measured only through randroid device / rios device: without it, refused before any adb or devicectl call")
def _():
    saved = os.environ.pop(VERB, None)
    try:
        with tempfile.TemporaryDirectory() as tmp:
            fake = FakeAdb(qemu="0", debuggable=False)
            msg = raises(perfcore.Refused, BARE_RUN_ANDROID, android_args(tmp, kind="physical", production=True, route="managed",
                         only="cold", conversation="as-installed"), runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
            assert "randroid device perf" in msg and fake.calls == [], (msg, fake.calls)
            calls = []
            msg = raises(perfcore.Refused, BARE_RUN_IOS, types.SimpleNamespace(device="u", stamp=None, expect_commit=None),
                         runner=lambda *a, **k: calls.append(a))
            assert "rios device perf" in msg and calls == [], (msg, calls)
    finally:
        if saved is not None:
            os.environ[VERB] = saved


@case("DV2 a DEBUGGABLE build on a physical phone is refused before anything is saved, seeded, installed or measured")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeAdb(qemu="0", debuggable=True)
        msg = raises(perfcore.Refused, perf.run_android, android_args(tmp, kind="physical", production=True, route="managed",
                     only="cold", conversation="as-installed"), runner=fake, sleep=lambda s: None, log=quiet, host=lambda: {})
        assert "DEBUGGABLE" in msg and "randroid device" in msg, msg
        touched = [c for c in fake.calls if c.startswith(("install", "uninstall", "pull", "push", "am ", "run-as"))]
        assert touched == [], touched


# ---------------------------------------------------------------------------------------------
# the same condition on an iPhone: devicectl into the app's data container, the UI-test runner
# ---------------------------------------------------------------------------------------------

HW = "00000000-FAKEFAKEFAKEFAKE"


class FakePhone:
    """devicectl, `rios perf-seed` and phone-ios.py for one iPhone. The app's data container is a real
    directory, with the copy semantics measured on the test iPhone (a directory copied to it with
    --remove-existing-content true becomes exactly the source); the app runs or not, and a write to its
    state while it runs is refused, as a running app would write over it."""

    def __init__(self, tmp, screen_ok=True, corrupt_seed=False, fail_restore=False, approval=False, no_state=False,
                 root_owned_state=False, all_root=False, parent_extra=False):
        self.container = os.path.join(tmp, "phone-container")
        self.state = os.path.join(self.container, ios.STATE_DIR)
        # Ownership as measured on the test iPhone (2026-10-02): the directory a `copy to` names as its
        # destination is root's (uid 0, 0755); everything created inside the copied tree is the app's
        # (uid 501). `root_owned_state`: the phone as the old seeding left it; `all_root`: a devicectl
        # that leaves everything root's, which the owner check must refuse.
        self.root_owned = {STATE_PARENT}
        if root_owned_state:
            self.root_owned.add(ios.STATE_DIR)
        self.all_root = all_root
        os.makedirs(os.path.join(self.container, "Library"), exist_ok=True)
        if parent_extra:
            os.makedirs(os.path.join(self.container, STATE_PARENT, "SomethingElse"))
        if not no_state:
            os.makedirs(os.path.join(self.state, "Attachments"))  # empty: it must come back too
            os.makedirs(os.path.join(self.state, "Recordings"))
            for name, data in (("state.json", b'{"the person\'s own":"pairing"}'), ("history.json", b'{"their":"history"}'),
                               ("completion-budget.json", b"[]"), ("Recordings/kept.wav", b"RIFF voice")):
                with open(os.path.join(self.state, name), "wb") as f:
                    f.write(data)
        self.screen_ok, self.corrupt_seed, self.fail_restore, self.approval = screen_ok, corrupt_seed, fail_restore, approval
        self.running, self.puts, self.calls, self.env = True, 0, [], None
        self.fixture_seen = self.written = None

    def now(self):
        return ios.tree_manifest(self.state) if os.path.isdir(self.state) else None

    def copies_to(self):
        return [c for c in self.calls if c[:5] == ["xcrun", "devicectl", "device", "copy", "to"]]

    def __call__(self, cmd, **kw):
        self.calls.append(cmd)
        ok = lambda out="": subprocess.CompletedProcess(cmd, 0, out, "")
        arg = lambda name: cmd[cmd.index(name) + 1]
        if cmd[:4] == ["xcrun", "devicectl", "list", "devices"]:
            with open(arg("--json-output"), "w") as f:
                json.dump({"result": {"devices": [{"identifier": "test-device", "hardwareProperties": {
                    "marketingName": "Fixture iPhone", "udid": HW}, "deviceProperties": {"osVersionNumber": "26.0"}}]}}, f)
            return ok()
        if cmd[:5] == ["xcrun", "devicectl", "device", "info", "apps"]:
            assert arg("--bundle-id") == ios.TEST_BUNDLE, cmd  # only ever the test copy, never the CEO's app
            with open(arg("--json-output"), "w") as f:
                json.dump({"result": {"apps": [{"bundleIdentifier": ios.TEST_BUNDLE, "url": "file:///private/var/containers/"
                                                "Bundle/Application/X/RichOSNative.app/"}]}}, f)
            return ok()
        if cmd[:5] == ["xcrun", "devicectl", "device", "info", "processes"]:
            procs = [{"processIdentifier": 717, "executable": "file:///private/var/containers/Bundle/Application/X/"
                                                              "RichOSNative.app/RichOSNative"}] if self.running else []
            with open(arg("--json-output"), "w") as f:
                json.dump({"result": {"runningProcesses": procs}}, f)
            return ok()
        if cmd[:5] == ["xcrun", "devicectl", "device", "process", "terminate"]:
            self.running = False
            return ok()
        if cmd[:5] == ["xcrun", "devicectl", "device", "info", "files"]:
            assert arg("--domain-type") == "appDataContainer" and arg("--domain-identifier") == ios.TEST_BUNDLE, cmd
            files = []
            for here, dirs, names in os.walk(self.container):
                for name in dirs + names:
                    rel = os.path.relpath(os.path.join(here, name), self.container)
                    uid = 0 if (self.all_root and rel.startswith(STATE_PARENT)) or rel in self.root_owned else 501
                    files.append({"relativePath": rel, "metadata": {"ownerUid": uid,
                                                                    "permissions": 0o755 if name in dirs else 0o644}})
            with open(arg("--json-output"), "w") as f:
                json.dump({"result": {"files": files}}, f)
            return ok()
        if cmd[:4] == ["xcrun", "devicectl", "device", "copy"]:
            assert arg("--domain-type") == "appDataContainer" and arg("--domain-identifier") == ios.TEST_BUNDLE, cmd
            if cmd[4] == "from":
                assert arg("--source") == ios.STATE_DIR, cmd
                if not os.path.isdir(self.state):
                    return subprocess.CompletedProcess(cmd, 1, "", f"ERROR: {ios.NO_FILE_NODE} for {ios.STATE_DIR}")
                import shutil
                shutil.copytree(self.state, arg("--destination"))
                return ok()
            dest = arg("--destination")
            assert dest in (STATE_PARENT, ios.STATE_DIR) and arg("--remove-existing-content") == "true", cmd
            assert not self.running, "the app's state was written while the app was running"
            self.puts += 1
            if self.fail_restore and self.puts >= 2:
                return subprocess.CompletedProcess(cmd, 1, "", "ERROR: the device went away")
            import shutil
            target = os.path.join(self.container, dest)
            shutil.rmtree(target, ignore_errors=True)
            shutil.copytree(arg("--source"), target)
            # the destination is root's, everything inside the copied tree the app's
            self.root_owned = {p for p in self.root_owned if not p.startswith(dest + "/")} | {dest}
            if self.corrupt_seed and self.puts == 1:
                with open(os.path.join(self.state, "history.json"), "ab") as f:
                    f.write(b" ")
            return ok()
        if cmd[0] == ios.RIOS and cmd[1] == "perf-seed":
            fixture, expected, out = cmd[2], cmd[3], cmd[4]
            files = {n: open(os.path.join(fixture, n), "rb").read() for n in sorted(os.listdir(fixture))}
            self.fixture_seen = (files, expected)
            os.makedirs(out)
            self.written = {"state.json": b'{"schema":2,"seeded":true}', "history.json": b'{"messages":"100 rows"}'}
            for name, data in self.written.items():
                with open(os.path.join(out, name), "wb") as f:
                    f.write(data)
            import hashlib
            sha = lambda b: hashlib.sha256(b).hexdigest()
            return ok(json.dumps({"ok": True, "result": {
                "fixture": "synthetic-conversation/1", "fixtureSha256": expected, "rows": json.loads(files["history.json"])["cursor"] // 2,
                "fixtureFiles": {n: sha(b) for n, b in files.items()}, "written": {n: sha(b) for n, b in self.written.items()}}}))
        if cmd[:2] == [sys.executable, ios.PHONE_IOS] and cmd[2] == "approval":
            assert arg("--device") == HW, cmd
            return ok(json.dumps({"approvalExpected": self.approval, "why": "fixture forecast"}))
        if cmd[:2] == [sys.executable, ios.PHONE_IOS] and cmd[2] == "run":
            self.env = kw.get("env") or {}
            steps = json.load(open(cmd[3]))
            assert "--prebuilt" in cmd and arg("--stamp"), cmd  # the stamped app's own runner: nothing new installed
            # room for native-work's CPU admission (up to 1800 s), which the device runner's limit counts
            assert arg("--allowance") == "1800", cmd
            out = arg("--out")
            os.makedirs(out, exist_ok=True)
            label = steps[1]["label"]
            seeded = self.now() == ios.tree_manifest(self._seed_dir()) if self.written else False
            seen = self.screen_ok and seeded
            rows = [{"i": 0, "do": "activate", "ok": True, "detail": {}},
                    {"i": 1, "do": "wait", "ok": seen, "error": None if seen else "not on screen within 15 s",
                     "detail": {"label": f"You, 8:00 AM: {label}", "waitedMs": 900} if seen else {}}]
            with open(os.path.join(out, "steps.jsonl"), "w") as f:
                f.write("\n".join(json.dumps(r) for r in rows) + "\n")
            return subprocess.CompletedProcess(cmd, 0 if seen else 1, json.dumps({"passed": seen, "out": out}), "")
        raise AssertionError(f"unexpected command {cmd}")

    def _seed_dir(self):
        """The seeded files as a directory, to compare the phone's state with (what the app read)."""
        d = os.path.join(os.path.dirname(self.container), "expected-seed")
        if not os.path.isdir(d):
            os.makedirs(d)
            for name, data in self.written.items():
                with open(os.path.join(d, name), "wb") as f:
                    f.write(data)
        return d


def phone_stamp(tmp, runner_app=True):
    products = os.path.join(tmp, "Products", "Release-iphoneos")
    app = os.path.join(products, "RichOSNative.app")
    os.makedirs(app)
    with open(os.path.join(app, "RichOSNative"), "wb") as f:
        f.write(b"release bytes")  # no development marker: a Release bundle
    if runner_app:
        os.makedirs(os.path.join(products, ios.RUNNER_APP))
    return write(tmp, "stamp.json", {"commit": "c" * 40, "dirty": False, "sha256": perfcore.tree_sha256(app), "artifact": app})


def run_phone(phone, tmp, trace=None, **over):
    """run_ios against `phone`. The trace series is replaced: it records what the app's state was when
    the series launched it (and the app is running afterwards, as after a launch)."""
    from unittest.mock import patch
    seen = []
    def series(cls, driver, trials, evidence, runner, popen, sleep, seconds=10, away=2.0, app_args=(), stop=None):
        seen.append((cls, phone.now()))
        if trace:
            trace(cls)
        phone.running = True
        d = os.path.join(evidence, f"ios-{cls}")
        os.makedirs(d, exist_ok=True)
        sample = ({**ios.launch_sample(launch_tables())} if cls == "cold" else {**ios.return_sample(return_tables(), 717)})
        sample.update(trial=1, trace=os.path.join(d, "t.trace"), exportAttempts=1)
        return [sample], [], d
    args = dict(device="test-device", conversation="fixture", mac=None, rows=None, cold=1, warm=1,
                evidence_dir=os.path.join(tmp, "evidence"), stamp=over.pop("stamp", None) or phone_stamp(tmp))
    args.update(over)
    with patch.object(ios, "trace_series", series), patch.object(ios, "evidence_root_ok", return_value=True), \
         patch.dict(os.environ, {"RICHOS_APPLE_TEAM": "ABCDEFGHIJ"}):
        record, failed = ios.run_ios(ios_args(**args), runner=phone, sleep=lambda s: None)
    return record, failed, seen


@case("C13 iPhone: the app's own state is copied off, the app gets exactly Android's fixture, both series run on it, the row is checked on screen, and the phone's state comes back byte for byte")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        phone = FakePhone(tmp)
        before = phone.now()
        record, failed, seen = run_phone(phone, tmp)
        files, expected = phone.fixture_seen
        assert files == condition.file_fixture(100) and expected == COND["conversation"]["sha256"], sorted(files)
        # both series launched into exactly what perf-seed wrote, and nothing of the phone's own state
        want = ios.tree_manifest(phone._seed_dir())
        assert [c for c, _ in seen] == ["cold", "warm"] and all(state == want for _, state in seen), seen
        cond = record["condition"]
        assert condition.same(cond, COND) and cond["build"] == "release" and condition.why_not_comparable(cond, COND) is None
        assert "devicectl" in cond["conversation"]["seededBy"] and set(cond["conversation"]["files"]) == {"history.json", "state.json"}
        assert cond["verified"]["onScreen"] is True and cond["verified"]["row"] == NEWEST_CEO, cond["verified"]
        assert phone.env["RICHOS_IOS_DEVICE"] == HW and phone.env["RICHOS_APPLE_TEAM"] == "ABCDEFGHIJ"
        c = record["conditions"]
        assert c["savedStateRestored"] is True and c["ownStateBackup"]["entries"] == len(before) and c["seededState"]["entries"] == 2, c
        assert record["route"]["name"] == "seeded fixture" and record["ranOnHardware"] and not failed, failed
        assert set(record["metrics"]) == {"coldLaunchTraced", "warmResumeTraced"}  # the diagnostic's keys: never judged
        assert phone.now() == before, phone.now()  # the phone's own state, empty directory and recording included
        assert len(phone.copies_to()) == 2  # the seed, then the restore: nothing else was written
        # CEO 2026-10-03: his own RichConnect is never touched; every command on the phone names only the test copy
        assert not [c for c in phone.calls if ios.BUNDLE in c], [c for c in phone.calls if ios.BUNDLE in c]
        assert record["build"]["bundle"] == ios.TEST_BUNDLE, record["build"]
        # both written through the parent, so the state directory is the app's own and it can save
        assert all(c[c.index("--destination") + 1] == STATE_PARENT for c in phone.copies_to()), phone.copies_to()
        assert ios.STATE_DIR not in phone.root_owned, phone.root_owned
        # the phone's own conversation does not stay on the Mac once it is back; its manifest does
        (seed_dir,) = [d for d in os.listdir(os.path.join(tmp, "evidence")) if d.startswith("ios-seed-")]
        backup = os.path.join(tmp, "evidence", seed_dir, "backup")
        assert os.listdir(backup) == ["manifest.json"] and json.load(open(os.path.join(backup, "manifest.json")))["manifest"] == before
        # the retained series carry the checked condition, so a reparse is compared like the record
        assert json.load(open(os.path.join(tmp, "evidence", "ios-cold", "series.json")))["condition"]["verified"]["onScreen"] is True
        assert not perfcore.check_record(record), perfcore.check_record(record)


@case("C14 iPhone: every failure puts the phone's own state back first — a row not on screen, a seed that reads back wrong, SIGTERM mid-series; a failed restore keeps the copy and ios-restore puts it back")
def _():
    import signal
    with tempfile.TemporaryDirectory() as tmp:
        for name, phone, said in (("unseen", FakePhone(tmp + "/unseen", screen_ok=False), "not seen on screen"),
                                  ("corrupt", FakePhone(tmp + "/corrupt", corrupt_seed=True), "could not be written")):
            before = phone.now()
            record, failed, seen = run_phone(phone, tmp + "/" + name)
            assert failed and phone.now() == before and record["conditions"]["savedStateRestored"] is True, name
            assert any(said in g["why"] for g in record["notMeasured"]), (name, record["notMeasured"])
            if name == "corrupt":
                assert not seen and record.get("condition") is None, "nothing is measured on a seed that did not read back"
            else:
                assert "was not on screen" in condition.why_not_comparable(record["condition"], COND)
        # a real SIGTERM in the middle of the series: the restore runs before the run ends
        phone = FakePhone(tmp + "/term")
        before = phone.now()
        handler = signal.getsignal(signal.SIGTERM)
        assert "signal" in raises(ios.Interrupted, run_phone, phone, tmp + "/term",
                                  trace=lambda cls: os.kill(os.getpid(), signal.SIGTERM))
        assert phone.now() == before and signal.getsignal(signal.SIGTERM) is handler, "restored, and the handler put back"
        # the restore itself fails: the record says so, the copy stays, and ios-restore puts it back verified
        phone = FakePhone(tmp + "/lost", fail_restore=True)
        before = phone.now()
        record, failed, _ = run_phone(phone, tmp + "/lost")
        assert record["conditions"]["savedStateRestored"] is False and phone.now() != before
        why = next(g["why"] for g in record["notMeasured"] if g["what"] == "the app's own saved state after the run")
        assert "perf.py ios-restore --device test-device --backup" in why, why
        backup = why.split("--backup ")[1].strip()
        assert ios.tree_manifest(os.path.join(backup, "RichOS")) == before
        phone.fail_restore = False
        assert "was taken from test-device" in raises(perfcore.Refused, ios.restore_device, "another-phone", backup, phone, lambda s: None)
        out = ios.restore_device("test-device", backup, phone, lambda s: None)
        assert out["restored"] and phone.now() == before and not os.path.exists(os.path.join(backup, "RichOS")), out


@case("C15 iPhone refusals change nothing on the phone: no stamp, no runner beside the app, no team, no SSD, a reachable Mac, a launch argument, an expected approval prompt, no saved state to put back")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        for name, over, env, said in (
                ("nostamp", {"stamp": None}, None, "stamped build"),
                ("mac", {"mac": "reachable"}, None, "unreachable by construction"),
                ("arg", {"app_arg": ["-x"]}, None, "launch argument"),
                ("team", {}, {"RICHOS_APPLE_TEAM": ""}, "RICHOS_APPLE_TEAM"),
        ):
            phone = FakePhone(tmp + "/" + name)
            from unittest.mock import patch
            args = dict(device="test-device", conversation="fixture", mac=None, rows=None, cold=1, warm=1,
                        evidence_dir=os.path.join(tmp, name, "evidence"), stamp=phone_stamp(tmp + "/" + name))
            args.update(over)
            with patch.object(ios, "evidence_root_ok", return_value=True), \
                 patch.dict(os.environ, env or {"RICHOS_APPLE_TEAM": "ABCDEFGHIJ"}):
                assert said in raises(perfcore.Refused, ios.run_ios, ios_args(**args), runner=phone), name
            assert phone.calls == [], (name, phone.calls)  # refused before the phone was asked anything
        phone = FakePhone(tmp + "/ssd")
        with patch.dict(os.environ, {"RICHOS_APPLE_TEAM": "ABCDEFGHIJ"}):
            assert "/Volumes/E1TB" in raises(perfcore.Refused, ios.run_ios, ios_args( # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
                device="test-device", conversation="fixture", mac=None, rows=None, stamp=phone_stamp(tmp + "/ssd"),
                evidence_dir=os.path.join(tmp, "ssd", "evidence")), runner=phone)
        assert phone.calls == []
        noruner = tmp + "/norunner"
        os.makedirs(noruner)
        phone = FakePhone(noruner)
        assert ios.RUNNER_APP in raises(perfcore.Refused, run_phone, phone, noruner, stamp=phone_stamp(noruner, runner_app=False))
        assert phone.calls == []
        for name, phone, said in (("approval", FakePhone(tmp + "/approval", approval=True), "allow UI automation"),
                                  ("empty", FakePhone(tmp + "/empty", no_state=True), "no saved state on this iPhone")):
            before = phone.now()
            assert said in raises(perfcore.Refused, run_phone, phone, tmp + "/" + name), name
            assert not phone.copies_to() and phone.now() == before, (name, phone.calls)
            leftovers = [d for d in os.listdir(os.path.join(tmp, name, "evidence"))] if os.path.isdir(os.path.join(tmp, name, "evidence")) else []
            assert leftovers == [], (name, leftovers)  # no partial copy of anything stays on the Mac


@case("C17 iPhone ownership: the seed and the restore leave the state directory the app's own, and give it back to "
      "a phone the old seeding left root-owned; a write that leaves it root's is refused even though its bytes read "
      "back right (the 2026-10-02 benchmark's save-failure banner); a parent holding anything else is never written")
def _():
    with tempfile.TemporaryDirectory() as tmp:
        phone = FakePhone(tmp + "/broken", root_owned_state=True)  # as the 2026-10-02 runs left the test iPhone
        before = phone.now()
        record, failed, _ = run_phone(phone, tmp + "/broken")
        assert not failed and record["conditions"]["savedStateRestored"] is True, record["notMeasured"]
        assert phone.now() == before and ios.STATE_DIR not in phone.root_owned, phone.root_owned
    with tempfile.TemporaryDirectory() as tmp:
        phone = FakePhone(tmp, all_root=True)  # bytes right, owner wrong
        before = phone.now()
        record, failed, seen = run_phone(phone, tmp)
        assert failed and "could not save into its state directory" in record["phases"]["seed"], record["phases"]
        assert not seen and "coldLaunch" not in record["metrics"]  # nothing measured on a state the app cannot save
        assert record["conditions"]["savedStateRestored"] is False and phone.now() == before
        assert any("ios-restore" in n["why"] for n in record["notMeasured"]), record["notMeasured"]
    with tempfile.TemporaryDirectory() as tmp:
        phone = FakePhone(tmp, parent_extra=True)
        before = phone.now()
        record, failed, seen = run_phone(phone, tmp)
        assert failed and "holds more than RichOS" in record["phases"]["seed"] and not phone.copies_to(), record["phases"]
        assert phone.now() == before and not seen
    state = ios.DeviceState(types.SimpleNamespace(target="x", pid=lambda: None))
    listing = {"Library": (501, 0o755), STATE_PARENT: (0, 0o755), ios.STATE_DIR: (0, 0o755),
               ios.STATE_DIR + "/state.json": (501, 0o644)}
    said = raises(perfcore.Unmeasurable, state.check_owner, listing)
    assert "uid 501" in said and ios.STATE_DIR + " (uid 0" in said, said
    listing[ios.STATE_DIR] = (501, 0o555)  # the app's, but not writable by it
    assert "mode 0o555" in raises(perfcore.Unmeasurable, state.check_owner, listing)
    listing[ios.STATE_DIR] = (501, 0o755)
    assert state.check_owner(listing) == 501


def timing_line(e, pid, wall_s, **extra):
    return json.dumps({"e": e, "pid": pid, "wallUs": int(round(wall_s * 1e6)), "uptime": wall_s - 1000.0, **extra})


@case("C15b Home page: the tap series ends on the icon's page, and a launch series on a phone first brings the Home "
      "Screen to that page, records it in the conditions, and does not run when the page cannot be set")
def _():
    from unittest.mock import patch
    # the tap list used to end with Home twice (page 1: the slow launch animation for whatever ran next)
    steps = ios.tap_steps(1, 1, 2.0)
    assert steps[-len(ios._to_icon_page()):] == ios._to_icon_page(), steps[-6:]
    assert steps[-1]["do"] == "sleep" and steps[-2]["do"] == "swipe" and steps[-2]["direction"] == "left", steps[-4:]
    order = []
    def fake_set(hw, team, stamp, out, runner=None):
        order.append("page")
        return {"page": ios.ICON_PAGE, "setBy": "x", "stepsOk": True, "evidence": out}, None
    def fake_launches(*a, **k):
        order.append("launches")
        return 0
    with tempfile.TemporaryDirectory() as tmp:
        with patch.object(ios, "set_icon_page", fake_set), patch.object(ios, "_launch_series", fake_launches):
            record, failed, _ = run_phone(FakePhone(tmp), tmp, cold=0, warm=0, launches=3)
        assert order == ["page", "launches"], order
        assert record["conditions"]["homePage"]["launches"]["page"] == ios.ICON_PAGE, record["conditions"]
    order.clear()
    with tempfile.TemporaryDirectory() as tmp:
        def refuse(hw, team, stamp, out, runner=None):
            return {"page": ios.ICON_PAGE, "stepsOk": False, "evidence": out}, "no runner"
        with patch.object(ios, "set_icon_page", refuse), patch.object(ios, "_launch_series", fake_launches):
            record, failed, _ = run_phone(FakePhone(tmp), tmp, cold=0, warm=0, launches=3)
        assert order == [] and failed, (order, failed)
        assert any("icon's page" in g["why"] for g in record["notMeasured"]), record["notMeasured"]


@case("C16 unprofiled taps: the step list passes phone-ios.py's own validation; the runner's tap times and the app's "
      "own lines join into launch and return samples; a launch with no input-ready, two processes after one tap and a "
      "relaunch during a return are rejected; a probe touch before activation is reported so; refused off a seeded iPhone")
def _():
    import importlib.util
    spec = importlib.util.spec_from_file_location("phone_ios", os.path.join(HERE, "qa", "phone-ios.py"))
    phone_ios = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(phone_ios)
    steps = ios.tap_steps(2, 3, 2.0)
    assert phone_ios.validate(steps) == steps
    assert [s["after"] for s in steps if s["do"] == "tapThen"] == [0.1, 0.2, 0.3]
    assert sum(1 for s in steps if s["do"] == "tap") == 2 and sum(1 for s in steps if s["do"] == "terminate") == 2
    assert "tapThen" in phone_ios.ACTIONS and phone_ios.vocabulary(phone_ios.RUNNER)["same"]
    rows = [{"i": 0, "do": "tap", "ok": True, "detail": {"tapAt": 100.0}},
            {"i": 1, "do": "tap", "ok": True, "detail": {"tapAt": 110.0}},
            {"i": 2, "do": "tap", "ok": True, "detail": {"tapAt": 120.0}},
            {"i": 3, "do": "tapThen", "ok": True, "after": 0.1, "detail": {"synthesizedAt": 130.0}},
            {"i": 4, "do": "tapThen", "ok": True, "after": 0.45, "detail": {"synthesizedAt": 140.0}},
            {"i": 5, "do": "tapThen", "ok": True, "after": 0.2, "detail": {"synthesizedAt": 150.0}},
            {"i": 6, "do": "tapThen", "ok": True, "after": 0.9, "detail": {"synthesizedAt": 160.0}}]
    lines = [
        timing_line("process", 7, 100.20, startUs=int(100.05e6)),  # launch 1: spawn 50 ms after the tap
        timing_line("did-activate", 7, 100.30), timing_line("useful-content", 7, 100.31),
        timing_line("input-ready", 7, 100.32),
        timing_line("process", 8, 110.20, startUs=int(110.06e6)),  # launch 2: no input-ready
        timing_line("process", 9, 120.20, startUs=int(120.04e6)), timing_line("process", 10, 120.30, startUs=int(120.25e6)),
        timing_line("input-ready", 9, 120.40),
        # return 1: the probe touch arrives at +120 ms, before activation at +447 ms
        timing_line("will-enter-foreground", 10, 130.02), timing_line("touch", 10, 130.12, eventUptime=130.11 - 1000.0,
                                                                      scene="foregroundInactive", view="UICollectionView"),
        timing_line("did-activate", 10, 130.447), timing_line("input-ready", 10, 130.48),
        # return 2: no touch reached the app
        timing_line("will-enter-foreground", 10, 140.02), timing_line("did-activate", 10, 140.45),
        timing_line("input-ready", 10, 140.49),
        # return 3: a relaunch
        timing_line("process", 11, 150.2, startUs=int(150.05e6)), timing_line("did-activate", 11, 150.5),
        # return 4, the last: the touch at +900 ms arrives after activation; the on-screen check's launch
        # 15 s later is not this return's
        timing_line("will-enter-foreground", 11, 160.02), timing_line("did-activate", 11, 160.46),
        timing_line("touch", 11, 160.91, eventUptime=160.90 - 1000.0, scene="foregroundActive", view="UICollectionView"),
        timing_line("input-ready", 11, 160.5), timing_line("process", 12, 175.2, startUs=int(175.05e6)),
        "{torn",
    ]
    events = ios.parse_timing("\n".join(lines))
    launches, rejected = ios.tap_launch_samples(rows, events)
    assert [s["trial"] for s in launches] == [1], launches
    assert launches[0]["tapToProcessStartMs"] == 50.0 and launches[0]["processStartToInputReadyMs"] == 270.0
    assert launches[0]["processStartToMainMs"] == 150.0 and launches[0]["tapToInputReadyMs"] == 320.0
    assert [(r["trial"], r["why"]) for r in rejected] == [
        (2, "no input-ready line from the launched process"), (3, "2 processes started between this tap and the next")]
    returns, rejected = ios.tap_return_samples(rows, events)
    assert [s["trial"] for s in returns] == [1, 2, 4] and rejected[0]["trial"] == 3 and "relaunch" in rejected[0]["why"]
    one, two, four = returns
    assert four["touch"]["received"] and not four["touch"]["beforeActive"] and four["touch"]["scene"] == "foregroundActive"
    assert one["touch"]["received"] and one["touch"]["beforeActive"] and one["touch"]["scene"] == "foregroundInactive"
    assert one["touch"]["eventMs"] == 110.0 and one["activeMs"] == 447.0 and one["probeAfterMs"] == 100.0
    assert two["touch"] == {"received": False} and two["inputReadyMs"] == 490.0
    summary = ios.unprofiled_summary(launches, returns)
    assert summary["warm"]["probeTouches"]["100.0"]["receivedBeforeActive"] == 1
    assert summary["warm"]["probeTouches"]["450.0"]["received"] == 0
    for over in (dict(simulator="sim", device=None, conversation="fixture", mac=None),
                 dict(conversation="as-installed", mac="reachable")):
        args = ios_args(tap_launches=30, tap_returns=0, **over)
        assert "seeded conversation" in raises(perfcore.Refused, ios.run_ios, args, runner=lambda *a, **k: 1 / 0)


def _series(first, rest):
    return [first] + list(rest)


@case("K1 cold standard (CEO 2026-10-03, §104): a slow start 3 fails AT start 3 and stops the series; start 1 is ignored; both phones' limits")
def _():
    for plat, early, avg in (("ios", 800, 600), ("android", 1000, 900)):
        assert perfcore.COLD_STANDARD[plat]["earlyMs"] == early and perfcore.COLD_STANDARD[plat]["avgMs"] == avg
        v =perfcore.cold_verdict(plat, [early * 5, early - 1, early])
        assert v["verdict"] == "FAIL" and v["failedStart"] == 3 and v["failedMs"] == early and v["stop"], v
        assert "start 3" in v["why"] and f"{early} ms" in v["why"], v
        assert perfcore.cold_should_stop(plat, [100, 100, early])
        assert not perfcore.cold_should_stop(plat, [early * 9, early - 1, early - 1]), "start 1 is never judged"
        assert perfcore.cold_verdict(plat, [100, 100, 100])["verdict"] == "INCOMPLETE"
        # a slow start 6 is past the per-start window: only the average sees it
        assert not perfcore.cold_should_stop(plat, [100] * 5 + [early * 3])


@case("K2 cold standard: starts 2-5 under the limit but a 2-20 average over it fails; a good series passes (iPhone and Android)")
def _():
    for plat, early, avg in (("ios", 800, 600), ("android", 1000, 900)):
        slow_tail = _series(100, [avg - 100] * 4 + [early + 2000] * 15)  # 2-5 fine, 6-20 slow
        v = perfcore.cold_verdict(plat, slow_tail)
        assert v["verdict"] == "FAIL" and not v["stop"] and "average" in v["why"] and v["averageMs"] >= avg, v
        good = perfcore.cold_verdict(plat, _series(5000, [avg - 150] * 19))
        assert good["verdict"] == "PASS" and good["averageMs"] == avg - 150, good
        edge = perfcore.cold_verdict(plat, _series(0, [avg] * 19))
        assert edge["verdict"] == "FAIL", "an average equal to the limit is not under it"


@case("K3 the iPhone's untraced series: start 1 is the unjudged first launch, a slow start 3 stops it after 3 launches")
def _():
    log = []
    ms = {2: 500, 3: 900, 4: 400}
    launched = []
    state = {"n": 0}

    def launch():
        state["n"] += 1
        launched.append(state["n"])

    def read():
        return [{"requestToInputReadyMs": ms[k]} for k in range(2, state["n"] + 1)]
    verdict, error = ios.run_launch_series(20, launch, lambda: None, read, lambda s: None, log.append)
    assert error is None and verdict["verdict"] == "FAIL" and verdict["failedStart"] == 3, (verdict, error)
    assert launched == [1, 2, 3], f"no start after the failing one: {launched}"
    state["n"] = 0
    launched.clear()
    ms = {k: 300 for k in range(2, 21)}
    verdict, error = ios.run_launch_series(20, launch, lambda: None, read, lambda s: None)
    assert verdict is None and error is None and launched == list(range(1, 21)), (verdict, error, launched)


@case("K4 warm standard: the CEO's final limits (§104), same judge as the cold one")
def _():
    assert (perfcore.WARM_STANDARD["ios"]["earlyMs"], perfcore.WARM_STANDARD["ios"]["avgMs"]) == (800, 700)
    assert (perfcore.WARM_STANDARD["android"]["earlyMs"], perfcore.WARM_STANDARD["android"]["avgMs"]) == (200, 150)
    assert perfcore.warm_verdict("android", [0, 100, 210])["failedStart"] == 3
    assert perfcore.warm_verdict("ios", [0, 100, 820])["failedStart"] == 3
    assert perfcore.warm_verdict("ios", [0, 100, 799])["verdict"] == "INCOMPLETE"
    saved = {p: dict(v) for p, v in perfcore.WARM_STANDARD.items()}
    try:
        perfcore.WARM_STANDARD["ios"].update(earlyMs=None, avgMs=None)
        raises(perfcore.LimitNotSet, perfcore.warm_verdict, "ios", [100] * 20)
        raises(perfcore.LimitNotSet, perfcore.warm_should_stop, "ios", [100] * 3)
        perfcore.WARM_STANDARD["ios"].update(earlyMs=800, avgMs=700)
        perfcore.WARM_STANDARD["android"].update(earlyMs=1000, avgMs=900)
        v = perfcore.warm_verdict("ios", [100, 100, 850])
        assert v["verdict"] == "FAIL" and v["failedStart"] == 3 and v["stop"] and v["kind"] == "warm", v
        assert perfcore.warm_verdict("android", _series(9000, [500] * 19))["verdict"] == "PASS"
        assert perfcore.warm_verdict("android", _series(0, [100] * 4 + [1500] * 15))["verdict"] == "FAIL"
    finally:
        for p, v in saved.items():
            perfcore.WARM_STANDARD[p].update(v)


@case("K5 the judged series never comes from a traced run: a phone's --cold/--warm are the untraced series, the trace is --trace-diagnostic and carries only *Traced keys, no standard verdict")
def _():
    from unittest.mock import patch
    plain = ios_args(trace_diagnostic=False, cold=20, warm=20)
    ios.untraced_for_phone(plain)
    assert (plain.cold, plain.warm, plain.launches, plain.returns) == (0, 0, 20, 20), plain
    traced = ios_args(trace_diagnostic=True, cold=20, warm=20)
    ios.untraced_for_phone(traced)
    assert (traced.cold, traced.warm) == (20, 20) and not getattr(traced, "launches", 0), "the diagnostic keeps the trace"
    assert ios.metric_key("cold", "physical") == "coldLaunchTraced" and ios.metric_key("warm", "physical") == "warmResumeTraced"
    # no traced series is ever started without --trace-diagnostic, even when asked for cold and warm
    with patch.object(ios, "trace_series", side_effect=AssertionError("a traced series was started")):
        raises(perfcore.Refused, ios.run_ios, ios_args(trace_diagnostic=False, cold=20, warm=20), runner=devicectl_listing)
    # and the speed check refuses a record whose only cold series is a traced one
    import watch
    traced_record = {"metrics": {"coldLaunchTraced": {"samplesMs": [1100] * 20}}}
    raises(watch.Unmeasured, watch.cold_standard, "ios", traced_record)
    raises(watch.Unmeasured, watch.warm_standard, {"metrics": {"warmResumeTraced": {"samplesMs": [500] * 20}}})


@case("K6 the iPhone's untraced warm series: a slow return stops it at once under a set limit; unset limits stop nothing and say so; a relaunch is rejected")
def _():
    year = 2026
    def stamp(sec):
        return f"Oct  3 10:00:{sec:06.3f}"
    request = lambda sec: f'{stamp(sec)} SpringBoard(FrontBoard)[1] <Notice>: [FBSystemService][0xabc] Received request to open "{ios.TEST_BUNDLE}"'
    log = "\n".join(request(s) for s in (1, 10, 20, 30))
    t = lambda sec: ios.syslog_wall_us(stamp(sec), year)
    events = [{"e": "input-ready", "wallUs": t(11.4), "pid": 7}, {"e": "input-ready", "wallUs": t(21.0), "pid": 7},
              {"e": "input-ready", "wallUs": t(30.6), "pid": 7}]
    samples, rejected = ios.return_request_samples(events, log, year)  # returns 1..3; return 1 is unjudged
    assert [round(s["requestToInputReadyMs"]) for s in samples] == [1000, 600] and not rejected, (samples, rejected)
    relaunch = events + [{"e": "process", "wallUs": t(20.2), "startUs": t(20.1), "pid": 9}]
    assert ios.return_request_samples(relaunch, log, year)[1][0]["why"].startswith("a new process started")
    saved = dict(perfcore.WARM_STANDARD["ios"])
    try:
        # unset: nothing stops the series, and its verdict is the loud placeholder
        perfcore.WARM_STANDARD["ios"].update(earlyMs=None, avgMs=None)
        ms = {2: 900, 3: 900, 4: 900}
        state = {"n": 0, "launches": []}
        def launch():
            state["n"] += 1
        def read():
            return [{"requestToInputReadyMs": 900} for k in range(2, state["n"])]  # launch count = return number + 1
        # n counts launches: the setup launch plus one per return
        verdict, error = ios.run_return_series(6, lambda: None, launch, read, lambda s: None)
        assert verdict is None and error is None and state["n"] == 7, state
        perfcore.WARM_STANDARD["ios"].update(earlyMs=500.0, avgMs=400.0)
        state["n"] = 0
        verdict, error = ios.run_return_series(20, lambda: None, launch, read, lambda s: None)
        assert verdict["failedStart"] == 2 and state["n"] == 3, (verdict, state)  # setup + returns 1 and 2, nothing after
    finally:
        perfcore.WARM_STANDARD["ios"].update(saved)


@case("K7 the warm series' mid-series timing read never terminates the app: devicectl sees no `process terminate` between returns (a stop would make the next return a relaunch)")
def _():
    calls = []

    def fake_devicectl(cmd, **kw):
        calls.append(list(cmd))
        if "copy" in cmd and "from" in cmd:
            dest = cmd[cmd.index("--destination") + 1]
            os.makedirs(dest)
            with open(os.path.join(dest, ios.TIMING_FILE), "w") as f:
                f.write("")
        return types.SimpleNamespace(returncode=0, stdout="", stderr="")
    driver = types.SimpleNamespace(target="x", pid=lambda: 4242)  # the app is running throughout
    state = ios.DeviceState(driver, runner=fake_devicectl, sleep=lambda s: None)
    with tempfile.TemporaryDirectory() as tmp:
        got = os.path.join(tmp, "got")
        log_path = os.path.join(tmp, "log")
        open(log_path, "w").close()
        ios._read_launch_evidence(state, got, log_path, stop=False)
        assert any("copy" in c and "from" in c for c in calls), calls
        assert not any("terminate" in c for c in calls), f"the mid-series read stopped the app: {calls}"
        calls.clear()
        try:
            ios._read_launch_evidence(state, got, log_path)  # the default (final) read still stops first
        except ios.Unmeasurable:
            pass  # the fake app never exits, so the stop gives up; what matters is that it asked
        assert any("terminate" in c for c in calls), calls


@case("TC1 the test copy's identity is one value everywhere it is written: test_copy.py, the iPhone build settings, the device tool, the UI-test runner, the Android build types")
def _():
    import test_copy
    assert (test_copy.TEST_BUNDLE_IOS, test_copy.TEST_PACKAGE_ANDROID) == ("dev.richos.connect.perf",) * 2
    assert test_copy.CEO_APP_IOS == "dev.richos.connect" == ios.BUNDLE and test_copy.TEST_DISPLAY_NAME == "RichConnect Perf"
    mjs = open(os.path.join(MOBILE, "native-ios/Tools/physical-device.mjs")).read()
    assert f"TEST_BUNDLE = '{test_copy.TEST_BUNDLE_IOS}'" in mjs and f"TEST_NAME = '{test_copy.TEST_DISPLAY_NAME}'" in mjs
    assert f"CEO_BUNDLE = '{test_copy.CEO_APP_IOS}'" in mjs
    assert "RICHOS_BUNDLE_ID=${TEST_BUNDLE}" in mjs and "RICHOS_APP_DISPLAY_NAME=${TEST_NAME}" in mjs
    swift = open(os.path.join(MOBILE, "native-ios/UITests/PhysicalDeviceTests.swift")).read()
    assert f'XCUIApplication(bundleIdentifier: "{test_copy.TEST_BUNDLE_IOS}")' in swift
    project = open(os.path.join(MOBILE, "native-ios/project.yml")).read()
    assert "PRODUCT_BUNDLE_IDENTIFIER: $(RICHOS_BUNDLE_ID)" in project and "$(RICHOS_APP_DISPLAY_NAME)" in project
    platform = open(os.path.join(MOBILE, "native-ios/Release/platform.yml")).read()
    assert f"RICHOS_BUNDLE_ID: {test_copy.CEO_APP_IOS}\n" in platform and "RICHOS_APP_DISPLAY_NAME: RichConnect\n" in platform
    gradle = open(os.path.join(MOBILE, "native-android/app/build.gradle.kts")).read()
    assert f'applicationId = "{test_copy.CEO_APP_ANDROID}"' in gradle
    copy = gradle[gradle.index('create("perfCopy")'):]
    copy = copy[:copy.index("\n        }")]
    assert f'applicationIdSuffix = "{test_copy.TEST_SUFFIX}"' in copy and 'initWith(getByName("release"))' in copy, copy
    assert "isDebuggable" not in copy and 'signingConfigs.findByName("upload")' in copy and test_copy.TEST_DISPLAY_NAME in copy
    assert 'android:label="${appLabel}"' in open(os.path.join(MOBILE, "native-android/app/src/main/AndroidManifest.xml")).read()
    rd = open(os.path.join(MOBILE, "native-android/bin/randroid")).read()
    assert ":app:assemblePerfCopy" in rd and "built_apk perfCopy" in rd and ":app:assembleRelease >&2 || fail \"the release APK did not build\"" not in rd.split("device() {")[1].split("# --- the release check")[0]
    import physical
    assert physical.PACKAGE == test_copy.TEST_PACKAGE_ANDROID
    import phone_net
    assert phone_net.PACKAGE == test_copy.TEST_BUNDLE_IOS


@case("TC2 every automatic path refuses the CEO's own app: a run's launch, `rios device launch|close`, the Android install, the device tool's bundle check")
def _():
    import test_copy
    own = test_copy.CEO_APP_IOS
    assert "CEO's own RichConnect" in raises(test_copy.CeoAppRefused, test_copy.refuse_ceo_app, own)
    assert test_copy.refuse_ceo_app(test_copy.TEST_BUNDLE_IOS) == test_copy.TEST_BUNDLE_IOS
    asked = []
    drv = ios.Devicectl("test-device", runner=lambda cmd, **kw: asked.append(cmd))
    raises(test_copy.CeoAppRefused, drv.launch, own)
    assert asked == [], "refused before the phone was asked anything"
    sys.path.insert(0, MOBILE)
    import phone_net
    raises(test_copy.CeoAppRefused, phone_net.launch_app, "test-device", own, [], lambda s: None)
    import physical
    from unittest.mock import patch
    with patch.object(physical, "apk_badging", return_value=(own, False)), patch.object(physical, "adb", side_effect=AssertionError("adb was used")):
        said = raises(physical.Refused, physical.install, "adb", "SER", "/x.apk", "aapt2")
    assert "CEO's own RichConnect" in said and "dev.richos.connect.perf" in said, said
    import importlib.util
    spec = importlib.util.spec_from_file_location("phone_ios_tc2", os.path.join(PERF, "..", "..", "app", "scripts", "qa", "phone-ios.py"))
    phone_ios = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(phone_ios)
    import contextlib, io as _io
    for fn, args in ((phone_ios.close_app, types.SimpleNamespace(bundle=own, device="test-device")),
                     (phone_ios.launch_app, types.SimpleNamespace(bundle=own, device="test-device", app_args=[], detach=False))):
        out = _io.StringIO()
        with contextlib.redirect_stdout(out), patch.object(phone_ios, "devicectl", side_effect=AssertionError("devicectl was used")):
            code = fn(args)
        assert code == 2 and "CEO's own RichConnect" in out.getvalue(), (fn.__name__, code, out.getvalue())


class Killed(BaseException):
    """A hard kill: nothing after it in the run executes, and the phone is not reachable afterwards."""


def killing_runner(trigger):
    """Answers everything with empty success until a command containing `trigger`, then the run is 'killed'."""
    calls, state = [], {"dead": False}

    def run(cmd, **kw):
        if state["dead"] or trigger in " ".join(cmd):
            state["dead"] = True
            raise Killed()
        calls.append(" ".join(cmd))
        return types.SimpleNamespace(returncode=0, stdout="5911\n" if "pidof" in " ".join(cmd) else "", stderr="")
    return run, calls


def next_command_runner():
    calls = []
    return (lambda cmd, **kw: (calls.append(" ".join(cmd)), types.SimpleNamespace(returncode=0, stdout="", stderr=""))[1]), calls


def restores_on_next_command(serial, kind, expect):
    import phone_changes
    assert [r["kind"] for r in phone_changes.pending(serial)] == [kind], phone_changes.pending(serial)
    run, calls = next_command_runner()
    android.Device("/fake/adb", serial, runner=run, sleep=lambda s: None).restore_leftovers()
    assert any(expect in c for c in calls), calls
    assert phone_changes.pending(serial) == []


@case("C1 a run killed while the phone is 'unplugged' leaves a record; the next command resets the battery")
def _():
    import phone_changes
    run, _calls = killing_runner("batterystats --reset")
    measure = android.Measure(android.Device("/fake/adb", "PHONE-C1", runner=run, sleep=lambda s: None), log=quiet)
    try:
        measure.background(1, 0, 10192)
    except Killed:
        pass
    assert [r["kind"] for r in phone_changes.pending("PHONE-C1")] == ["battery"]
    restores_on_next_command("PHONE-C1", "battery", "dumpsys battery reset")


@case("C2 a run killed with system tracing on leaves a record; the next command stops the trace")
def _():
    import phone_changes
    run, _calls = killing_runner("atrace --async_start")
    measure = android.Measure(android.Device("/fake/adb", "PHONE-C2", runner=run, sleep=lambda s: None), log=quiet)
    try:
        measure.atrace(["view"], lambda: None, 0)
    except Killed:
        pass
    restores_on_next_command("PHONE-C2", "atrace", "atrace --async_stop")


@case("C3 a run that forced night mode and lost the phone leaves a record; the next command puts the theme back")
def _():
    import phone_changes
    with tempfile.TemporaryDirectory() as tmp:
        fake = FakeAdb()
        state = {"forced": False}

        def runner(cmd, **kw):
            text = " ".join(cmd)
            if state["forced"]:
                raise Killed()  # the phone is gone as soon as the theme was forced: nothing after it reaches it
            if "cmd uimode night yes" in text:
                state["forced"] = True
            return fake(cmd, **kw)
        try:
            perf.run_android(android_args(tmp, theme="dark"), runner=runner, sleep=lambda s: None, log=quiet,
                             host=lambda: {})
        except Killed:
            pass
    left = [r for r in phone_changes.pending("emulator-5580") if r["kind"] == "night"]
    assert left, phone_changes.pending()
    run, calls = next_command_runner()
    android.Device("/fake/adb", "emulator-5580", runner=run, sleep=lambda s: None).restore_leftovers()
    assert any("cmd uimode night" in c for c in calls), calls
    assert not [r for r in phone_changes.pending("emulator-5580") if r["kind"] == "night"]


@case("C4 `observe --action sleep` is gone: a phone is never put to sleep")
def _():
    import importlib.util
    from unittest.mock import patch
    spec = importlib.util.spec_from_file_location("phone_android_c4", os.path.join(HERE, "qa", "phone-android.py"))
    pa = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(pa)
    sent = []

    class P:
        def __init__(self, *a, **k): pass
        def attached(self): return True
        def sh(self, cmd, *a, **k): sent.append(cmd)

    import contextlib, io as _io
    out = _io.StringIO()
    with patch.object(pa, "Phone", P), patch.object(pa.physical, "require_verb"), patch.object(pa.physical, "gate"), \
            contextlib.redirect_stdout(out), tempfile.TemporaryDirectory() as tmp:
        code = pa.main(["--serial", "S", "observe", "--package", "p", "--label", "l", "--out-dir", tmp,
                        "--settle", "0", "--seconds", "0", "--action", "sleep"])
    assert code != 0 and "never locked or put to sleep" in out.getvalue(), (code, out.getvalue())
    assert not any("SLEEP" in c for c in sent), sent
    assert "KEYCODE_SLEEP" not in open(os.path.join(HERE, "qa", "phone-android.py")).read().replace("KEYCODE_SLEEP\" is gone", "")


if __name__ == "__main__":
    total = sum(1 for line in open(__file__) if line.startswith("@case("))
    if failures:
        print(f"mobile-perf: {len(failures)} of {total} FAILED: {', '.join(failures)}")
        sys.exit(1)
    print(f"mobile-perf: {total} cases passed")
