"""ios — RichConnect measurement on an iOS simulator or iPhone.

Physical capture was first exercised on iPhone SE (2nd generation), iOS 26.3.1,
with Xcode 26.3 on 2026-09-24. App Launch alone does not collect the app's
signposts: add the os_signpost instrument explicitly. Retain each raw trace.

Cold launch and warm return (PRD §7) end where the PRD puts them: the saved
viewport PRESENTED and the composer accepting input. The app marks when its
transcript applied its saved position and its editable field exists, then
`input-ready` once the main run loop committed that turn and went idle. The tool
joins those marks, on the trace's one clock and by process ID, to the Core
Animation commit that carried them, to the first frame whose server render
began after that commit (Frame Lifetimes) and to the display swap that showed
it. A useful draw alone is not the endpoint: on the first physical trace it came
one commit before the transcript's final scroll position.

Simulator frame records are not a display pipeline (a swap can precede its own
render), so a simulator trace proves the capture, marks and join, never a phone
number. Simulator timing without --xctrace uses the host launch command and the
device log on the Mac's clock. Simulator background observations use the
simulated process's CPU and network use, not physical iPhone energy.
"""
import contextlib
import datetime
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import machine_storage as M
import re
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

import condition
import perfcore
import test_copy
from perfcore import Refused, Unmeasurable

# Simulators run the app under its own ID. Every PHYSICAL iPhone run (the speed watch, `rios device perf`) works only on
# the TEST COPY installed beside the CEO's own RichConnect, under its own ID and with its own data (test_copy.py);
# the CEO's app (dev.richos.connect) is never installed, launched, stopped, backed up or restored by it.
BUNDLE = "dev.richos.connect"
TEST_BUNDLE = test_copy.TEST_BUNDLE_IOS
SUBSYSTEM = "dev.richos.connect"
USEFUL = "useful-content"
FOREGROUND = "foreground-useful"
# The app's PRD §7 end marks (native-ios/App/Platform/PerformanceMarks.swift, ReadinessMarks).
VIEWPORT = "viewport-ready"
COMPOSER = "composer-ready"
READY = "input-ready"
CORE_ANIMATION = "com.apple.coreanimation"
PROCESS_CREATION = "Initializing - Process Creation"
# Every table a launch or return sample reads; each export must carry its own schema.
TABLES = ("life-cycle-period", "os-signpost", "coreanimation-lifetime-interval", "display-surface-swap")
# Xcode 26.3's trace loader crashes on about one load in seven (SIGSEGV or a Swift trap in
# InstrumentsPlugIn FileStatus, during ProcessLoader.load) before any table is read. An export is
# a pure read of an immutable trace, so a signal death is re-read; every attempt is recorded.
EXPORT_ATTEMPTS = 5
EXPORT_PAUSE_S = 2.0
# Frame Lifetimes' end and the display's swap timestamp for the same swap agree to tens of ns.
SWAP_TOLERANCE_NS = 100_000
# Simulator.swift `developmentMarkers`: every Debug bundle carries all of them, Release none.
DEVELOPMENT_MARKERS = ["rios-commands", "rios-fixture", "rios-interactive-fixture", "rios-appearance",
                       "rios-notifications", "rios-cards", "compose-draft", "Henderson proposal"]


def run(cmd, runner=subprocess.run, timeout=120):
    p = runner(cmd, capture_output=True, text=True, timeout=timeout)
    if p.returncode:
        raise Unmeasurable(f"{' '.join(cmd[:4])} exited {p.returncode}: {(p.stderr or p.stdout).strip()[:200]}")
    return p.stdout


def simulator(listing, udid):
    """`xcrun simctl list devices -j` → the named simulator, which must be booted."""
    if udid.lower() == "booted":
        raise Refused("a simulator is named by its UDID, never 'booted' (another test's simulator may be booted)")
    data = json.loads(listing)
    for runtime, devices in data.get("devices", {}).items():
        for d in devices:
            if d.get("udid") == udid:
                if d.get("state") != "Booted":
                    raise Refused(f"simulator {udid} is {d.get('state')}, not Booted")
                return {"kind": "simulator", "udid": udid, "model": d.get("name"),
                        "os": runtime.rsplit(".", 1)[-1].replace("iOS-", "iOS ").replace("-", ".")}
    raise Refused(f"no simulator {udid}")


def physical(listing, udid):
    """`xcrun devicectl list devices --json-output` → the named iPhone."""
    data = json.loads(listing)
    for d in (data.get("result") or {}).get("devices", []):
        hw = d.get("hardwareProperties") or {}
        if udid in (d.get("identifier"), hw.get("udid")):
            props = d.get("deviceProperties") or {}
            return {"kind": "physical", "udid": udid, "model": hw.get("marketingName") or hw.get("productType"),
                    "os": f"iOS {props.get('osVersionNumber')}"}
    raise Refused(f"no iPhone {udid} in devicectl's list")


def parse_log_marks(ndjson, name):
    """`log show --style ndjson --signpost` lines → timestamps (UTC epoch seconds) of mark `name`."""
    marks = []
    for line in ndjson.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        if entry.get("subsystem") != SUBSYSTEM:
            continue
        if entry.get("signpostName") != name and entry.get("eventMessage") != name:
            continue
        stamp = entry.get("timestamp", "")
        try:
            when = datetime.datetime.strptime(stamp, "%Y-%m-%d %H:%M:%S.%f%z")
        except ValueError:
            continue
        marks.append(when.timestamp())
    return marks


def parse_top(text):
    """`top -l 1 -pid P -stats pid,idlew,csw,time` → {"idleWakeups", "switches", "cpuSeconds"}."""
    lines = [l for l in text.splitlines() if l.strip()]
    for i, line in enumerate(lines):
        if line.split()[:4] == ["PID", "IDLEW", "CSW", "TIME"] and i + 1 < len(lines):
            cols = lines[i + 1].split()
            parts = [float(x) for x in cols[3].split(":")]
            seconds = 0.0
            for part in parts:
                seconds = seconds * 60 + part
            return {"idleWakeups": int(re.sub(r"\D", "", cols[1])), "switches": int(re.sub(r"\D", "", cols[2])),
                    "cpuSeconds": seconds}
    raise Unmeasurable("top printed no PID IDLEW CSW TIME row")


def parse_nettop(text, pid):
    """`nettop -P -L 1 -p PID -J bytes_in,bytes_out -x` CSV → total bytes for the process."""
    total = 0
    found = False
    for line in text.splitlines():
        cols = line.split(",")
        if len(cols) >= 3 and cols[0].endswith(f".{pid}"):
            total += int(cols[1] or 0) + int(cols[2] or 0)
            found = True
    return total if found else 0


def parse_launchctl_pid(text):
    """`simctl spawn <udid> launchctl list` → the app's pid (label UIKitApplication:dev.richos.connect[...])."""
    for line in text.splitlines():
        cols = line.split("\t") if "\t" in line else line.split()
        if len(cols) >= 3 and f"UIKitApplication:{BUNDLE}[" in cols[2] and cols[0].isdigit():
            return int(cols[0])
    return None


def parse_xctrace_signposts(xml_text, name):
    """An exported os-signpost table → event times (ns from trace start) of signpost `name`. The
    export dedupes repeated values by id/ref; both forms are resolved."""
    root = ET.fromstring(xml_text)
    refs = {}
    for el in root.iter():
        if el.get("id") is not None:
            refs[el.get("id")] = el
    times = []
    for row in root.iter("row"):
        def val(tag):
            el = row.find(tag)
            if el is None:
                return None
            if el.get("ref") is not None:
                el = refs.get(el.get("ref"), el)
            return el.text
        if val("name") == name or val("signpost-name") == name:
            t = val("event-time") or val("start-time")
            if t is not None:
                times.append(int(t))
    return times


class Sim:
    def __init__(self, udid, runner=subprocess.run, sleep=time.sleep, clock=time.time):
        self.udid, self.runner, self.sleep, self.clock = udid, runner, sleep, clock

    def simctl(self, *args, timeout=120):
        return run(["xcrun", "simctl", *args], self.runner, timeout)

    def since(self, t):
        return datetime.datetime.fromtimestamp(t - 1).strftime("%Y-%m-%d %H:%M:%S")

    def marks(self, name, since):
        out = self.simctl("spawn", self.udid, "log", "show", "--style", "ndjson", "--signpost", "--start", self.since(since),
                          "--predicate", f'subsystem == "{SUBSYSTEM}"')
        return [m for m in parse_log_marks(out, name) if m >= since]

    def cold(self, trials):
        samples, rejected = [], []
        for i in range(trials):
            self.runner(["xcrun", "simctl", "terminate", self.udid, BUNDLE], capture_output=True, text=True)
            self.sleep(1.0)
            t0 = self.clock()
            self.simctl("launch", self.udid, BUNDLE)
            mark = None
            for _ in range(40):
                found = self.marks(USEFUL, t0)
                if found:
                    mark = found[0]
                    break
                self.sleep(0.25)
            if mark is None:
                rejected.append({"trial": i + 1, "why": f"no '{USEFUL}' signpost within 10 s (the app does not emit it yet?)"})
                continue
            samples.append(round((mark - t0) * 1000))
            self.sleep(2.0)
        return samples, rejected

    def pid(self):
        return parse_launchctl_pid(self.simctl("spawn", self.udid, "launchctl", "list"))

    def background(self, seconds, settle_s):
        pid = self.pid()
        if pid is None:
            raise Unmeasurable("the app is not running on the simulator")
        self.simctl("launch", self.udid, "com.apple.Preferences")  # the app goes to the background
        self.sleep(settle_s)

        def sample():
            top = parse_top(run(["top", "-l", "1", "-pid", str(pid), "-stats", "pid,idlew,csw,time"], self.runner))
            net = parse_nettop(run(["nettop", "-P", "-L", "1", "-p", str(pid), "-J", "bytes_in,bytes_out", "-x"], self.runner), pid)
            return top, net
        (a, na), _ = sample(), self.sleep(seconds)
        b, nb = sample()
        return {"seconds": seconds, "settleSeconds": settle_s, "pid": pid,
                "idleWakeups": b["idleWakeups"] - a["idleWakeups"], "contextSwitches": b["switches"] - a["switches"],
                "cpuMs": round((b["cpuSeconds"] - a["cpuSeconds"]) * 1000), "networkBytes": nb - na}


def trace_useful_draw(lifecycle_xml, signposts_xml):
    """Join two real xctrace tables by target PID, never assume trace zero is launch."""
    def rows(xml):
        root = ET.fromstring(xml)
        refs = {e.get("id"): e for e in root.iter() if e.get("id") is not None}
        def resolve(e):
            return refs.get(e.get("ref"), e) if e is not None else None
        for row in root.iter("row"):
            values = {e.tag: resolve(e) for e in row}
            process = values.get("process")
            pid = resolve(process.find("pid")) if process is not None else None
            yield values, int(pid.text) if pid is not None else None
    starts = []
    for values, pid in rows(lifecycle_xml):
        if values.get("app-period") is not None and values["app-period"].text == "Initializing - Process Creation":
            starts.append((pid, int(values["start-time"].text)))
    if len(starts) != 1 or starts[0][0] is None:
        raise Unmeasurable("expected exactly one process creation in the target lifecycle table")
    pid, start = starts[0]
    ends = []
    for values, row_pid in rows(signposts_xml):
        if (row_pid == pid and values.get("subsystem") is not None and values["subsystem"].text == SUBSYSTEM
                and values.get("signpost-name") is not None and values["signpost-name"].text == USEFUL
                and values.get("event-type") is not None and values["event-type"].text == "Event"):
            ends.append(int(values["event-time"].text))
    if len(ends) != 1 or ends[0] <= start:
        raise Unmeasurable("expected one useful-content event after creation in the same process")
    return {"pid": pid, "creationNs": start, "usefulDrawNs": ends[0], "durationMs": (ends[0] - start) / 1e6}


# ---------------------------------------------------------------------------------------------
# PRD §7 end marks: the frame that presented the ready content, and the main thread idle after it
# ---------------------------------------------------------------------------------------------

def _table(xml_text, schema):
    """One exported table as rows of {column mnemonic: element}, id/ref resolved across the export.
    Refuses an export without its schema: Frame Lifetimes and display swaps repeat engineering
    types, so reading their columns by position would be a guess. (An export of several tables
    carries the schema on the first table only, which is why each table is exported alone.)"""
    root = ET.fromstring(xml_text)
    refs = {e.get("id"): e for e in root.iter() if e.get("id") is not None}

    def resolve(e):
        return refs.get(e.get("ref"), e) if e is not None and e.get("ref") is not None else e
    nodes = root.findall("node")
    if len(nodes) != 1:
        raise Unmeasurable(f"expected one exported {schema} table, found {len(nodes)} (was it recorded?)")
    found = nodes[0].find("schema")
    if found is None or found.get("name") != schema:
        raise Unmeasurable(f"the {schema} export carries no {schema} schema")
    cols = [c.findtext("mnemonic") for c in found.findall("col")]
    rows = []
    for row in nodes[0].findall("row"):
        cells = list(row)
        if len(cells) != len(cols):
            raise Unmeasurable(f"a {schema} row has {len(cells)} cells for {len(cols)} columns")
        rows.append({c: resolve(e) for c, e in zip(cols, cells)})
    return rows, resolve


def _text(e):
    return None if e is None or e.tag == "sentinel" else e.text


def _int(e):
    t = _text(e)
    return int(t) if t not in (None, "") else None


def _pid(resolve, e):
    e = resolve(e)
    if e is not None and e.tag == "thread":
        e = resolve(e.find("process"))
    if e is None or e.tag == "sentinel":
        return None
    p = resolve(e.find("pid"))
    return int(p.text) if p is not None and p.text else None


def signposts(xml_text):
    """os-signpost → [{ns, pid, main, subsystem, name, type, message}]; `main` is the main thread."""
    rows, resolve = _table(xml_text, "os-signpost")

    def message(e):
        return None if e is None or e.tag == "sentinel" else (e.get("fmt") or e.text)
    return [{"ns": _int(r.get("time")), "pid": _pid(resolve, r.get("process")),
             "main": r.get("thread") is not None and (r["thread"].get("fmt") or "").startswith("Main Thread"),
             "subsystem": _text(r.get("subsystem")), "name": _text(r.get("name")), "type": _text(r.get("event-type")),
             "message": message(r.get("message"))}
            for r in rows]


def lifecycle(xml_text):
    """life-cycle-period → [{start, duration, pid, period}] (trace clock, ns)."""
    rows, resolve = _table(xml_text, "life-cycle-period")
    return [{"start": _int(r.get("start")), "duration": _int(r.get("duration")), "pid": _pid(resolve, r.get("process")),
             "period": _text(r.get("period"))} for r in rows]


def frame_lifetimes(xml_text):
    """coreanimation-lifetime-interval → [{start, duration, swapId, renderStart}], complete rows only."""
    rows, _ = _table(xml_text, "coreanimation-lifetime-interval")
    out = []
    for r in rows:
        f = {"start": _int(r.get("start")), "duration": _int(r.get("duration")), "swapId": _int(r.get("swap-id")),
             "renderStart": _int(r.get("render-start"))}
        if None not in f.values():
            out.append(f)
    return out


def display_swaps(xml_text):
    """display-surface-swap → [{ns, swapId}]: the moment the display showed each surface."""
    rows, _ = _table(xml_text, "display-surface-swap")
    return [{"ns": _int(r.get("timestamp")), "swapId": _int(r.get("swap-id"))} for r in rows]


def presented_ready(marks, frames, swaps, pid, after_ns, content):
    """PRD §7's end on one trace clock. `input-ready` must occur exactly once in process `pid` after
    `after_ns`, preceded by every `content` mark. The carrying commit is the first main-thread Core
    Animation commit to END at or after the latest content mark (Core Animation sends a transaction
    when its commit ends), and it must end no later than `input-ready`, which the app emits after
    that commit. The presented frame is the first frame whose server render STARTED at or after
    that commit's end: a render that began earlier cannot contain it. Its presentation is the
    display's swap time, which must agree with that frame lifetime's end. The end is the later of
    presentation and `input-ready`. Any missing or ambiguous step rejects the sample."""
    mine = [m for m in marks if m["pid"] == pid and m["ns"] is not None and m["ns"] >= after_ns]

    def ours(name):
        return sorted(m["ns"] for m in mine if m["subsystem"] == SUBSYSTEM and m["name"] == name and m["type"] == "Event")
    ready = ours(READY)
    if len(ready) != 1:
        raise Unmeasurable(f"expected exactly one '{READY}' in process {pid} after the start, found {len(ready)}")
    r = ready[0]
    content_ns = {}
    for name in content:
        before = [t for t in ours(name) if t <= r]
        if not before:
            raise Unmeasurable(f"no '{name}' before '{READY}' in process {pid}")
        content_ns[name] = before[-1]
    commit = carrying_commit(marks, pid, max(content_ns.values(), default=after_ns), until_ns=r)
    frame = presented_frame(frames, swaps, commit)
    return {"pid": pid, "contentNs": content_ns, "commitEndNs": commit, **frame, "inputReadyNs": r,
            "endNs": max(frame["presentedNs"], r)}


def carrying_commit(marks, pid, content_ns, until_ns=None):
    """The end of the first main-thread Core Animation commit in `pid` ending at or after
    `content_ns` (and, if given, no later than `until_ns`)."""
    ends = sorted(m["ns"] for m in marks if m["pid"] == pid and m["main"] and m["subsystem"] == CORE_ANIMATION
                  and m["name"] == "Commit" and m["type"] == "End" and m["ns"] is not None and m["ns"] >= content_ns)
    if not ends or (until_ns is not None and ends[0] > until_ns):
        raise Unmeasurable(f"no main-thread Core Animation commit ended between the ready content and '{READY}'")
    return ends[0]


def presented_frame(frames, swaps, commit_end_ns):
    """The first frame whose server render started at or after `commit_end_ns`, and the display
    swap that showed it (which must agree with that frame lifetime's end)."""
    later = sorted((f for f in frames if f["renderStart"] >= commit_end_ns), key=lambda f: f["renderStart"])
    if not later:
        raise Unmeasurable("no frame began rendering after the carrying commit before the trace ended "
                           "(no Frame Lifetimes data, or the trace is too short)")
    if len(later) > 1 and later[1]["renderStart"] == later[0]["renderStart"]:
        raise Unmeasurable("two frames began rendering at the same instant; the presented frame is ambiguous")
    frame = later[0]
    swap = [s for s in swaps if s["swapId"] == frame["swapId"]]
    if len(swap) != 1:
        raise Unmeasurable(f"frame swap {frame['swapId']} has {len(swap)} display swaps, not one")
    lifetime_end = frame["start"] + frame["duration"]
    if abs(swap[0]["ns"] - lifetime_end) > SWAP_TOLERANCE_NS:
        raise Unmeasurable(f"swap {frame['swapId']}: the display shows it at {swap[0]['ns']} ns but its frame "
                           f"lifetime ends at {lifetime_end} ns")
    if swap[0]["ns"] < frame["renderStart"]:
        # Seen on the Simulator (Xcode 26.3): its host-rendered frame records are not a display pipeline.
        raise Unmeasurable(f"swap {frame['swapId']} was shown before its render began; this is not physical "
                           f"display presentation data")
    return {"renderStartNs": frame["renderStart"], "swapId": frame["swapId"], "presentedNs": swap[0]["ns"],
            "lifetimeEndNs": lifetime_end}


def _first_mark(marks, pid, name, after_ns):
    found = sorted(m["ns"] for m in marks if m["pid"] == pid and m["subsystem"] == SUBSYSTEM and m["name"] == name
                   and m["type"] == "Event" and m["ns"] is not None and m["ns"] >= after_ns)
    return found[0] if found else None


def launch_sample(tables):
    """Cold launch: the target's process creation to the presented, input-ready saved viewport."""
    creations = [p for p in lifecycle(tables["life-cycle-period"]) if p["period"] == PROCESS_CREATION]
    if len(creations) != 1 or creations[0]["pid"] is None or creations[0]["start"] is None:
        raise Unmeasurable(f"expected exactly one '{PROCESS_CREATION}' in the lifecycle table, found {len(creations)}")
    pid, start = creations[0]["pid"], creations[0]["start"]
    marks = signposts(tables["os-signpost"])
    end = presented_ready(marks, frame_lifetimes(tables["coreanimation-lifetime-interval"]),
                          display_swaps(tables["display-surface-swap"]), pid, start, (VIEWPORT, COMPOSER))
    draw = _first_mark(marks, pid, USEFUL, start)
    return {**end, "class": "cold", "startBoundary": PROCESS_CREATION, "startNs": start,
            "durationMs": (end["endNs"] - start) / 1e6,
            "phasesMs": {"usefulDraw": None if draw is None else (draw - start) / 1e6,
                         "commitEnd": (end["commitEndNs"] - start) / 1e6,
                         "presented": (end["presentedNs"] - start) / 1e6,
                         "inputReady": (end["inputReadyNs"] - start) / 1e6}}


RESUME = ("com.apple.UIKit", "AppResume")


def return_sample(tables, pid):
    """Warm return in retained process `pid`: UIKit's own `AppResume` interval begin (AppLifecycle,
    IsForeground 1, emitted in the app's process when the foreground transition reaches it) to the
    presented frame of the first draw after activation and the idle main thread after it. A new
    process is a cold start, never a warm sample."""
    life = lifecycle(tables["life-cycle-period"])
    if any(p["period"] == PROCESS_CREATION for p in life):
        raise Unmeasurable("the trace contains a process creation: the app was relaunched, which is a cold start")
    marks = signposts(tables["os-signpost"])
    pids = {m["pid"] for m in marks if m["subsystem"] == SUBSYSTEM and m["pid"] is not None}
    if pids - {pid}:
        raise Unmeasurable(f"app marks came from process {sorted(pids - {pid})}, not the retained process {pid}")
    resumes = sorted(m["ns"] for m in marks if m["pid"] == pid and (m["subsystem"], m["name"]) == RESUME
                     and m["type"] == "Begin" and "IsForeground= 1" in (m["message"] or ""))
    if len(resumes) != 1:
        raise Unmeasurable(f"expected exactly one UIKit AppResume (IsForeground 1) in process {pid}, found {len(resumes)}")
    start = resumes[0]
    end = presented_ready(marks, frame_lifetimes(tables["coreanimation-lifetime-interval"]),
                          display_swaps(tables["display-surface-swap"]), pid, start, (FOREGROUND,))
    return {**end, "class": "warm", "startBoundary": "UIKit AppResume begin", "startNs": start,
            "durationMs": (end["endNs"] - start) / 1e6,
            "phasesMs": {"foregroundDraw": (end["contentNs"][FOREGROUND] - start) / 1e6,
                         "commitEnd": (end["commitEndNs"] - start) / 1e6,
                         "presented": (end["presentedNs"] - start) / 1e6,
                         "inputReady": (end["inputReadyNs"] - start) / 1e6}}


def export_tables(trace, prefix, runner=subprocess.run, pause=time.sleep, tables=TABLES):
    """Export each table alone from a saved trace, keeping every attempt's output and exit. A
    signal death (the loader crash above) is re-read up to EXPORT_ATTEMPTS times; any other failure
    stops at once. The trace is retained either way: `perf.py ios --reparse` exports it later."""
    out, attempts = {}, []
    try:
        for table in tables:
            for attempt in range(1, EXPORT_ATTEMPTS + 1):
                result = runner(["xcrun", "xctrace", "export", "--input", trace, "--xpath",
                                 f'/trace-toc/run[@number="1"]/data/table[@schema="{table}"]'],
                                capture_output=True, text=True, timeout=120)
                path = f"{prefix}.{table}.xml"
                with open(path, "w") as f:
                    f.write(result.stdout or "")
                if result.stderr:
                    with open(f"{path}.{attempt}.stderr", "w") as f:
                        f.write(result.stderr)
                attempts.append({"table": table, "attempt": attempt, "exit": result.returncode,
                                 "bytes": len(result.stdout or "")})
                if result.returncode == 0 and (result.stdout or "").strip():
                    out[table] = result.stdout
                    break
                if result.returncode >= 0:
                    raise Unmeasurable(f"xctrace export of {table} failed (exit {result.returncode}); see {path}")
                if attempt == EXPORT_ATTEMPTS:
                    raise Unmeasurable(f"xctrace export of {table} was killed by signal {-result.returncode} "
                                       f"{EXPORT_ATTEMPTS} times; the trace is retained for --reparse")
                pause(EXPORT_PAUSE_S)
    finally:
        with open(prefix + ".exports.json", "w") as f:
            json.dump(attempts, f, indent=2)
    return out, attempts


def build_configuration(artifact):
    """'release', 'debug' or None, from the stamped bundle's bytes by the same development-marker
    test as `rios sim check-release`: a Debug bundle carries every marker, a Release bundle none."""
    if not artifact or not os.path.isdir(artifact):
        return None
    present = set()
    for root, _, files in os.walk(artifact):
        for name in files:
            path = os.path.join(root, name)
            if os.path.islink(path) or not os.path.isfile(path):
                continue
            with open(path, "rb") as f:
                data = f.read()
            present.update(m for m in DEVELOPMENT_MARKERS if m.encode() in data)
    if len(present) == len(DEVELOPMENT_MARKERS):
        return "debug"
    return "release" if not present else None


APP_EXECUTABLE = "RichOSNative"  # the test copy keeps the product name; its install path tells it from the CEO's app
AWAY_APP = "com.apple.Preferences"  # brought to the front to background the app (no Home press over USB)
RECORD = ["xcrun", "xctrace", "record", "--template", "App Launch", "--instrument", "os_signpost",
          "--instrument", "Frame Lifetimes"]


class Devicectl:
    """A physical iPhone through Xcode's devicectl. `process launch` without --terminate-existing
    activates an app that is already running (devicectl's documented default, --activate)."""
    kind = "physical"
    bundle = TEST_BUNDLE

    def __init__(self, udid, runner=subprocess.run):
        self.target, self.runner = udid, runner

    def install_path(self):
        """Where the TEST COPY is installed on the phone (its install URL, by bundle ID). The CEO's own app has the same
        executable name, so a process is the test copy's only by its path, never by its name."""
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "apps.json")
            run(["xcrun", "devicectl", "device", "info", "apps", "--device", self.target, "--bundle-id", self.bundle,
                 "--json-output", out], self.runner, timeout=60)
            with open(out) as f:
                data = json.load(f)
        apps = (data.get("result") or {}).get("apps")
        if not isinstance(apps, list):
            raise Unmeasurable("devicectl's app list has no result.apps")
        path = str((apps[0] if apps else {}).get("url") or "").replace("file://", "").rstrip("/")
        if not path:
            raise Unmeasurable(f"the test copy {self.bundle} is not installed on the phone (`rios device install` installs it)")
        return path

    def pid(self):
        path = self.install_path()
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "processes.json")
            run(["xcrun", "devicectl", "device", "info", "processes", "--device", self.target, "--json-output", out],
                self.runner, timeout=60)
            with open(out) as f:
                data = json.load(f)
        procs = (data.get("result") or {}).get("runningProcesses")
        if not isinstance(procs, list):
            raise Unmeasurable("devicectl's process list has no result.runningProcesses")
        found = [p.get("processIdentifier") for p in procs
                 if str(p.get("executable") or "").replace("file://", "").startswith(path + "/")]
        if len(found) > 1:
            raise Unmeasurable(f"{len(found)} test-copy processes are running")
        return int(found[0]) if found else None

    def launch(self, bundle, *args):
        test_copy.refuse_ceo_app(bundle, "a speed run")  # the CEO's own app is never opened by a run
        run(["xcrun", "devicectl", "device", "process", "launch", "--device", self.target, bundle, *args],
            self.runner, timeout=60)


class Simctl:
    """A leased, booted simulator (`--xctrace`): the same capture path, for dry runs of the tool.
    Simulator frame records are refused as presentation data (see presented_frame)."""
    kind = "simulator"
    bundle = BUNDLE

    def __init__(self, udid, runner=subprocess.run):
        self.target, self.runner = udid, runner

    def pid(self):
        return parse_launchctl_pid(run(["xcrun", "simctl", "spawn", self.target, "launchctl", "list"], self.runner))

    def launch(self, bundle, *args):
        run(["xcrun", "simctl", "launch", self.target, bundle, *args], self.runner)


def evidence_root_ok(path):
    if not path:
        return False
    try:
        M.validate(path)
        return True
    except (ValueError, OSError):
        return False


def _capture(command, prefix, runner, timeout):
    result = runner(command, capture_output=True, text=True, timeout=timeout)
    with open(prefix + ".log", "w") as f:
        f.write(result.stdout or "")
    with open(prefix + ".log.stderr", "w") as f:
        f.write(result.stderr or "")
    if result.returncode:
        raise Unmeasurable(f"capture failed ({result.returncode}); see {prefix}.log")


def _bundle(driver):
    """The app a driver opens: a phone's test copy, a simulator's own app."""
    return getattr(driver, "bundle", BUNDLE)


def _return_capture(driver, trace, prefix, seconds, away, app_args, runner, popen, sleep):
    """Attach a recording to the app's process while it is in front (a simulator dry run once
    failed to find a just-backgrounded process by pid), and once the recording reports that
    tracing started, put Settings in front, wait `away` seconds and bring the app back. The same
    process must be running afterwards."""
    pid = driver.pid()
    if pid is None:
        driver.launch(_bundle(driver), *app_args)
        sleep(3.0)
        pid = driver.pid()
        if pid is None:
            raise Unmeasurable("the app is not running after launching it")
    with open(prefix + ".trial.json", "w") as f:
        json.dump({"pid": pid}, f)
    note = f"dev.richos.perf.{os.getpid()}.{os.path.basename(prefix)}"
    waiter = popen(["notifyutil", "-1", note], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with open(prefix + ".log", "w") as out, open(prefix + ".log.stderr", "w") as err:
        recorder = popen(RECORD + ["--device", driver.target, "--time-limit", f"{seconds}s", "--output", trace,
                                   "--notify-tracing-started", note, "--attach", str(pid)],
                         stdout=out, stderr=err, text=True)
        try:
            for _ in range(120):  # at most 60 s for the recording to start; fail at once if it ends
                try:
                    waiter.wait(timeout=0.5)
                    break
                except subprocess.TimeoutExpired:
                    if recorder.poll() is not None:
                        with open(prefix + ".log.stderr") as f:
                            detail = f.read().strip()[-300:]
                        raise Unmeasurable(f"the recording ended ({recorder.returncode}) before tracing started: {detail}")
            else:
                raise Unmeasurable("the recording never reported that tracing started")
            driver.launch(AWAY_APP)
            sleep(away)
            driver.launch(_bundle(driver))
            code = recorder.wait(timeout=seconds + 120)
        finally:
            for owned in (waiter, recorder):  # only the processes this function started
                if owned.poll() is None:
                    owned.terminate()
                    owned.wait(timeout=30)
    if code:
        raise Unmeasurable(f"capture failed ({code}); see {prefix}.log")
    after = driver.pid()
    if after != pid:
        raise Unmeasurable(f"the app's process changed from {pid} to {after}: a relaunch is a cold start")
    return pid


@contextlib.contextmanager
def _own_tmp(evidence):
    """xctrace spills multi-GB `instruments*.ktrace` files into $TMPDIR. Point TMPDIR at a folder
    inside this run's evidence directory for the run, and remove it however the run ends."""
    mine = tempfile.mkdtemp(prefix="tmp-", dir=evidence)
    before = os.environ.get("TMPDIR")
    os.environ["TMPDIR"] = mine
    try:
        yield mine
    finally:
        if before is None:
            os.environ.pop("TMPDIR", None)
        else:
            os.environ["TMPDIR"] = before
        shutil.rmtree(mine, ignore_errors=True)


def trace_series(cls, driver, trials, evidence, runner=subprocess.run, popen=subprocess.Popen, sleep=time.sleep,
                 seconds=10, away=2.0, app_args=(), stop=None):
    """`cold` or `warm` trials with Instruments, each trace retained under a unique directory.
    The first rejected trial (capture, export or join) stops the series; nothing is retried
    except the re-read of a trace whose exporter died (export_tables)."""
    if not 1 <= trials <= 1000:
        raise Refused("trace trials must be between 1 and 1000")
    if cls == "warm" and seconds < away + 5:
        raise Refused(f"--trace-seconds {seconds} cannot hold {away} s away plus the return; use at least {away + 5:g}")
    if not evidence_root_ok(evidence):
        raise Refused("traces require --evidence-dir on a validated storage path")
    os.makedirs(evidence, exist_ok=True)
    evidence = tempfile.mkdtemp(prefix=f"ios-{cls}-", dir=evidence)
    samples, rejected = [], []
    with _own_tmp(evidence):
        for i in range(trials):
            prefix = os.path.join(evidence, f"{'launch' if cls == 'cold' else 'return'}-{i + 1:04}")
            trace = prefix + ".trace"
            try:
                if cls == "cold":
                    _capture(RECORD + ["--device", driver.target, "--time-limit", f"{seconds}s", "--output", trace,
                                       "--launch", "--", _bundle(driver), *app_args], prefix, runner, seconds + 120)
                    tables, attempts = export_tables(trace, prefix, runner, sleep)
                    sample = launch_sample(tables)
                else:
                    pid = _return_capture(driver, trace, prefix, seconds, away, app_args, runner, popen, sleep)
                    tables, attempts = export_tables(trace, prefix, runner, sleep)
                    sample = return_sample(tables, pid)
                sample.update(trial=i + 1, trace=trace, exportAttempts=len(attempts))
                with open(prefix + ".json", "w") as f:
                    json.dump(sample, f, indent=2)
                samples.append(sample)
                if stop is not None and stop([s["durationMs"] for s in samples]):
                    break  # the standard's early limit: no further start is made
            except (Unmeasurable, ET.ParseError, ValueError, KeyError, subprocess.TimeoutExpired) as e:
                rejection = {"trial": i + 1, "why": str(e), "evidence": prefix}
                rejected.append(rejection)
                with open(prefix + ".rejected.json", "w") as f:
                    json.dump(rejection, f, indent=2)
                break
    return samples, rejected, evidence


def device_cold(udid, trials, evidence, runner=subprocess.run):
    """Physical cold launches (kept for callers of the first tool): see trace_series."""
    return trace_series("cold", Devicectl(udid, runner), trials, evidence, runner)


def reparse(evidence, cls, runner=subprocess.run, sleep=time.sleep):
    """Re-export and re-join every retained trace of one series, e.g. after the exporter died on
    all its attempts. Each trace is read again; nothing is captured."""
    stem = "launch" if cls == "cold" else "return"
    traces = sorted(n for n in os.listdir(evidence) if n.startswith(stem + "-") and n.endswith(".trace"))
    if not traces:
        raise Refused(f"no {stem}-NNNN.trace in {evidence}")
    samples, rejected = [], []
    for name in traces:
        prefix = os.path.join(evidence, name[:-len(".trace")])
        try:
            tables, attempts = export_tables(prefix + ".trace", prefix, runner, sleep)
            if cls == "cold":
                sample = launch_sample(tables)
            else:
                with open(prefix + ".trial.json") as f:
                    sample = return_sample(tables, int(json.load(f)["pid"]))
            sample.update(trial=int(name[len(stem) + 1:len(stem) + 5]), trace=prefix + ".trace",
                          exportAttempts=len(attempts), reparsed=True)
            samples.append(sample)
        except (Unmeasurable, ET.ParseError, ValueError, KeyError, OSError, subprocess.TimeoutExpired) as e:
            rejected.append({"trial": name, "why": str(e), "evidence": prefix})
    return samples, rejected


KEYS = {"cold": "coldLaunch", "warm": "warmResume"}
END_BOUNDARY = ("the later of: the display swap that presented the first frame rendered after the main-thread "
                "Core Animation commit carrying the ready content, and the app's input-ready mark (main run loop "
                "idle after that commit)")
METHODS = {
    "cold": "Instruments App Launch + os_signpost + Frame Lifetimes (xctrace record --launch). Start: the target "
            "process's 'Initializing - Process Creation'. Ready content: the app's viewport-ready (the transcript "
            "applied its saved position) and composer-ready (the editable field exists). End: " + END_BOUNDARY + ". "
            "One trace clock, joined by process ID. Profiler overhead is included; pixels are not inspected.",
    "warm": "Instruments App Launch + os_signpost + Frame Lifetimes (xctrace record --attach to the retained "
            "process). Settings is brought to the front, then the app is re-activated without termination "
            "(devicectl/simctl launch). Start: UIKit's AppResume begin (IsForeground 1) in that process. Ready "
            "content: the first draw after activation (foreground-useful). End: " + END_BOUNDARY + ". A changed "
            "process is rejected as a cold start. Profiler overhead is included; pixels are not inspected.",
}


def trace_metric(cls, samples, rejected, evidence):
    ms = [round(s["durationMs"], 3) for s in samples]
    summary = perfcore.stats(ms)
    p95 = summary.get("p95")
    phases = {}
    for name in sorted({k for s in samples for k, v in s["phasesMs"].items() if v is not None}):
        values = [round(s["phasesMs"][name], 3) for s in samples if s["phasesMs"].get(name) is not None]
        phases[name] = perfcore.stats(values)
    return {"method": METHODS[cls], "startBoundary": samples[0]["startBoundary"], "endBoundary": END_BOUNDARY,
            "samplesMs": ms, "stats": summary, "budget": perfcore.compare(KEYS[cls], summary), "phaseStatsMs": phases,
            "outliers": [{"trial": s["trial"], "ms": round(s["durationMs"], 3), "trace": s["trace"]}
                         for s in samples if p95 is not None and s["durationMs"] > p95],
            "exportAttempts": sum(s["exportAttempts"] for s in samples), "rejected": rejected, "evidence": evidence}


# On an iPhone the cold start that is judged is the one with nothing attached (`--launches N`): it is
# `coldLaunch`. The Instruments cold series adds the profiler's own launch (§102), so it is evidence only.
TRACED_COLD = "coldLaunchTraced"
TRACED_WARM = "warmResumeTraced"


def metric_key(cls, device_kind):
    """On an iPhone the Instruments series is a DIAGNOSTIC (`--trace-diagnostic`): its own keys, which nothing
    compares and no standard judges (§104; a traced launch adds ~530 ms, §102). The judged series are the
    untraced launches and returns (`--launches`, `--returns`; `--cold`/`--warm` on a phone mean those)."""
    if device_kind == "physical":
        return TRACED_COLD if cls == "cold" else TRACED_WARM
    return KEYS[cls]


def untraced_for_phone(args):
    """On a physical iPhone `--cold N` and `--warm N` are the UNTRACED launch and return series; the Instruments
    trace runs only with the separately named `--trace-diagnostic` and never produces a pass or fail."""
    if not getattr(args, "device", None) or getattr(args, "trace_diagnostic", False):
        return
    args.launches = getattr(args, "launches", 0) or args.cold
    args.returns = getattr(args, "returns", 0) or (getattr(args, "warm", 0) or 0)
    args.cold, args.warm = 0, 0


def _trace_classes(record, driver, args, runner, popen, sleep):
    """Each requested class in turn; the first rejected trial stops everything after it."""
    failures = 0
    for cls, trials in (("cold", args.cold), ("warm", getattr(args, "warm", 0) or 0)):
        if trials <= 0:
            continue
        key = metric_key(cls, record["device"].get("kind"))
        samples, rejected, evidence = trace_series(cls, driver, trials, args.evidence_dir, runner, popen, sleep,
                                                   seconds=getattr(args, "trace_seconds", 10),
                                                   away=getattr(args, "away", 2.0),
                                                   app_args=tuple(getattr(args, "app_arg", None) or ()))
        with open(os.path.join(evidence, "series.json"), "w") as f:
            json.dump({"class": cls, "device": record["device"], "build": record["build"],
                       "condition": record.get("condition")}, f, indent=2)
        record.setdefault("evidence", {})[cls] = evidence
        if samples:
            record["metrics"][key] = trace_metric(cls, samples, rejected, evidence)
        else:
            record["notMeasured"].append({"what": key, "why": rejected[0]["why"] if rejected else "no trial"})
        record["phases"][cls] = (f"stopped at trial {rejected[0]['trial']}: {rejected[0]['why']}" if rejected
                                 else f"{len(samples)} trials")
        if rejected:
            failures += 1
            break
    return failures


def _reparsed(args, record, runner):
    with open(os.path.join(args.reparse, "series.json")) as f:
        series = json.load(f)
    record["device"], record["build"] = series["device"], series["build"]
    record["condition"] = series.get("condition")  # a series retained before conditions were written has none
    cond = record["condition"] or {}
    if cond and (cond.get("conversation") or {}).get("fixture") != condition.AS_INSTALLED and "verified" not in cond:
        cond["verified"] = {"row": None, "onScreen": None, "why": "the retained series names no on-screen check"}
    record["ranOnHardware"] = series["device"].get("kind") == "physical"
    record["evidence"] = {series["class"]: args.reparse}
    samples, rejected = reparse(args.reparse, series["class"], runner)
    if samples:
        record["metrics"][metric_key(series["class"], series["device"].get("kind"))] = trace_metric(series["class"], samples, rejected, args.reparse)
    else:
        record["notMeasured"].append({"what": metric_key(series["class"], series["device"].get("kind")), "why": rejected[0]["why"] if rejected else "no trace"})
    record["phases"][series["class"]] = f"reparsed {len(samples)} of {len(samples) + len(rejected)} retained traces"
    return int(bool(rejected) or not samples)




# ---------------------------------------------------------------------------------------------
# The seeded condition on a simulator (condition.py FILE_FIXTURE, as the iPhone app's saved state)
# ---------------------------------------------------------------------------------------------

RIOS = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "native-ios", "bin", "rios"))
# LocalOnlyStorage.appRoot (native-ios/App/Platform/Shared/PlatformIdentity.swift) inside the data container.
STATE_DIR = os.path.join("Library", "Application Support", "RichOS")
STATE_PARENT = os.path.dirname(STATE_DIR)
# EffectRunner.stateKey and historyKey: the files the seeding replaces, and restores afterwards.
STATE_FILES = ("history.json", "state.json")
SCREEN_SETTLE_S = 3.0


def _sha256(data):
    import hashlib
    return hashlib.sha256(data).hexdigest()


def _rios_json(cmd, runner):
    """A `bin/rios` command's result: its JSON envelope on stdout, or its JSON error refused."""
    p = runner(cmd, capture_output=True, text=True, timeout=900)
    if p.returncode:
        detail = (p.stderr or p.stdout).strip()
        try:
            detail = json.loads(detail.splitlines()[-1]).get("error", detail)
        except (ValueError, IndexError, AttributeError):
            pass
        raise Unmeasurable(f"rios {cmd[1]} exited {p.returncode}: {str(detail)[-400:]}")
    try:
        return json.loads(p.stdout)["result"]
    except (ValueError, KeyError) as e:
        raise Unmeasurable(f"rios {cmd[1]} printed no result: {e}")


def app_state_files(rows, scratch, runner=subprocess.run, rios=RIOS):
    """FILE_FIXTURE with `rows`, as the iPhone app's saved state: condition.py writes Android's files,
    `rios perf-seed` translates them through the app's own core (refusing unless their manifest is the
    condition's SHA-256, and unless the core loads every row back), and each written file is checked
    against the SHA-256 the core reported. Returns (report, {name: bytes})."""
    fixture_dir, out_dir = os.path.join(scratch, "fixture"), os.path.join(scratch, "app-state")
    condition.write_fixture(rows, fixture_dir)
    expected = condition.for_files(rows, "release", "-")["conversation"]["sha256"]
    report = _rios_json([rios, "perf-seed", fixture_dir, expected, out_dir], runner)
    if report.get("fixtureSha256") != expected or report.get("rows") != rows:
        raise Unmeasurable(f"rios perf-seed wrote {report.get('rows')} rows of {report.get('fixtureSha256')}, "
                           f"not {rows} of {expected}")
    files = {}
    for name in STATE_FILES:
        with open(os.path.join(out_dir, name), "rb") as f:
            files[name] = f.read()
        if _sha256(files[name]) != (report.get("written") or {}).get(name):
            raise Unmeasurable(f"the written {name} is not the file rios perf-seed reported")
    if set(report.get("written") or {}) != set(STATE_FILES):
        raise Unmeasurable(f"rios perf-seed wrote {sorted(report.get('written') or {})}, not {list(STATE_FILES)}")
    return report, files


class SimState:
    """The installed app's saved state on a simulator, in its data container on this Mac. The app is
    terminated before every change, so it never writes over what is put there."""

    def __init__(self, udid, runner=subprocess.run):
        self.udid, self.runner = udid, runner

    def directory(self):
        data = run(["xcrun", "simctl", "get_app_container", self.udid, BUNDLE, "data"], self.runner).strip()
        if not data or not os.path.isdir(data):
            raise Unmeasurable(f"the app's data container {data!r} is not on this Mac")
        return os.path.join(data, STATE_DIR)

    def stop(self):
        self.runner(["xcrun", "simctl", "terminate", self.udid, BUNDLE], capture_output=True, text=True)

    def read(self):
        """{name: bytes or None} of STATE_FILES as they are now."""
        here = self.directory()
        out = {}
        for name in STATE_FILES:
            path = os.path.join(here, name)
            out[name] = open(path, "rb").read() if os.path.isfile(path) else None
        return out

    def put(self, files):
        """Write `files` ({name: bytes, or None to remove}) and read each back."""
        self.stop()
        here = self.directory()
        os.makedirs(here, exist_ok=True)
        for name, data in sorted(files.items()):
            path = os.path.join(here, name)
            if data is None:
                if os.path.exists(path):
                    os.remove(path)
                continue
            staging = path + ".perf-seed"
            with open(staging, "wb") as f:
                f.write(data)
            os.replace(staging, path)
        now = self.read()
        for name, data in files.items():
            if now.get(name) != data:
                raise Unmeasurable(f"{name} in the app's data container does not read back as written")


def normalized(text):
    return " ".join(text.split()).lower()


def screen_check(udid, marker, keep_dir, runner=subprocess.run, sleep=time.sleep, rios=RIOS):
    """After the launches: bring the app to the front (a launch if it is not running, which reads the
    saved state again), take a screenshot and read its text with the Mac's Vision framework (`rios
    screen-text`). The seeded newest CEO row must be among it. The screenshot shows only the made-up
    conversation; it is kept in `keep_dir` when given, otherwise deleted."""
    result = {"row": marker, "onScreen": None,
              "how": "simctl launch (front), a screenshot, its text read by Vision (rios screen-text); the "
                     "newest seeded CEO row must be in it"}
    with tempfile.TemporaryDirectory() as tmp:
        shot = os.path.join(keep_dir or tmp, "screen-check.png")
        try:
            run(["xcrun", "simctl", "launch", udid, BUNDLE], runner)
            sleep(SCREEN_SETTLE_S)
            run(["xcrun", "simctl", "io", udid, "screenshot", shot], runner)
            lines = _rios_json([rios, "screen-text", shot], runner).get("lines") or []
        except (Unmeasurable, subprocess.TimeoutExpired, OSError) as e:
            result["why"] = f"the screen could not be read: {e}"
            return result
        result["onScreen"] = normalized(marker) in normalized(" ".join(lines))
        result["linesRead"] = len(lines)
        if keep_dir:
            result["screenshot"] = shot
    return result


# ---------------------------------------------------------------------------------------------
# The seeded condition on an iPhone: devicectl into the app's data container, the UI-test runner
# ---------------------------------------------------------------------------------------------

PHONE_IOS = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "app", "scripts",
                                         "qa", "phone-ios.py"))
RUNNER_APP = "RichOSNativeUITests-Runner.app"
STOP_TRIES = 10
# The UI-test session starts xcodebuild through the engine's native-work.py, which holds it until the
# Mac's CPU is under the admission line (up to 1800 s), and the device runner's limit (allowance + 60 s)
# counts that wait. With 120 s, the check after the first 100+100 series (2026-10-02) was stopped
# before xcodebuild began: the Mac was 93-95% busy. The three steps take seconds; the allowance is
# the room to be admitted, phone-ios.py's maximum.
SCREEN_ALLOWANCE_S = 1800
# devicectl's error when the source of `copy from` does not exist (measured on the iPhone, 2026-10-02).
NO_FILE_NODE = "Failed to retrieve the file node"


class Interrupted(BaseException):
    """SIGTERM or SIGHUP while the phone holds the seeded state: raised so the restore runs first."""


def tree_manifest(root):
    """{relative path: "dir", or the sha256 of the file's bytes} of a local directory tree."""
    out = {}
    for here, dirs, files in os.walk(root):
        dirs.sort()
        for name in dirs:
            out[os.path.relpath(os.path.join(here, name), root)] = "dir"
        for name in sorted(files):
            path = os.path.join(here, name)
            with open(path, "rb") as f:
                out[os.path.relpath(path, root)] = _sha256(f.read())
    return out


def tree_differences(want, got):
    """Sentences naming how `got` differs from `want` (both tree_manifest), at most five."""
    out = [f"{p} is missing" for p in sorted(set(want) - set(got))]
    out += [f"{p} should not be there" for p in sorted(set(got) - set(want))]
    out += [f"{p} has other bytes" for p in sorted(set(want) & set(got)) if want[p] != got[p]]
    return out[:5]


def manifest_sha256(manifest):
    return _sha256(json.dumps(manifest, sort_keys=True).encode())


def same_bytes(a, b):
    """Every file and directory under `a` and `b` is the same, compared byte for byte."""
    import filecmp
    if tree_manifest(a).keys() != tree_manifest(b).keys():
        return False
    for here, _, files in os.walk(a):
        for name in files:
            left = os.path.join(here, name)
            if not filecmp.cmp(left, os.path.join(b, os.path.relpath(left, a)), shallow=False):
                return False
    return True


class DeviceState:
    """The installed app's saved state on an iPhone: `Library/Application Support/RichOS` in its data
    container, read and written with devicectl. Measured on the test iPhone in a private temporary
    domain (2026-10-02): `copy to` a directory with --remove-existing-content true leaves exactly the
    source there, empty directories and modification times included; `copy from` a directory gives
    it back the same way; an absent source fails with NO_FILE_NODE. The app is terminated before every
    read and every change, so it never writes over what is put there.

    Ownership (measured on the test iPhone, 2026-10-02): the directory a `copy to` names as its
    destination is left owned by root, mode 0755, while every file and directory created INSIDE the
    copied tree belongs to the app's user. Written as the destination, RichOS became root's: the app
    (uid 501) could not create a file in its own state directory, every save failed, and the app showed
    "This iPhone could not save your latest changes" through the whole 2026-10-02 benchmark and on the
    phone afterwards, while the byte-for-byte read-back passed. So the state is written into its parent
    (`Library/Application Support`, which must hold nothing else), RichOS is created inside the copied
    tree, and every write is checked for owner and permissions as well as bytes."""

    def __init__(self, driver, runner=subprocess.run, sleep=time.sleep):
        self.driver, self.target, self.runner, self.sleep = driver, driver.target, runner, sleep

    def _devicectl(self, *args, timeout=300):
        return self.runner(["xcrun", "devicectl", "device", *args, "--device", self.target, "-q"],
                           capture_output=True, text=True, timeout=timeout)

    def owners(self):
        """{relative path: (owner uid, mode)} of the app's data container, read with devicectl."""
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "files.json")
            p = self._devicectl("info", "files", "--domain-type", "appDataContainer", "--domain-identifier", TEST_BUNDLE,
                                "--json-output", out)
            try:
                with open(out) as f:
                    files = (json.load(f).get("result") or {}).get("files") or []
            except (OSError, ValueError):
                raise Unmeasurable(f"devicectl could not list the app's files: {(p.stderr or p.stdout or '').strip()[-300:]}")
        return {f.get("relativePath"): ((f.get("metadata") or {}).get("ownerUid"), (f.get("metadata") or {}).get("permissions"))
                for f in files if f.get("relativePath")}

    def check_owner(self, listing=None):
        """Every entry of the state directory belongs to the app's user (the owner of the container's
        own Library) and its owner can write it, so the app can save. Returns the app's uid."""
        listing = self.owners() if listing is None else listing
        app_uid = (listing.get("Library") or (None, None))[0]
        if app_uid in (None, 0):
            raise Unmeasurable(f"the app's container Library has no app owner ({listing.get('Library')}), so the "
                               "state directory's owner cannot be checked")
        wrong = sorted(f"{path} (uid {uid}, mode {oct(mode or 0)})" for path, (uid, mode) in listing.items()
                       if (path == STATE_DIR or path.startswith(STATE_DIR + "/"))
                       and (uid != app_uid or not (mode or 0) & 0o200))
        if STATE_DIR not in listing:
            wrong.insert(0, f"{STATE_DIR} is absent")
        if wrong:
            raise Unmeasurable(f"the app (uid {app_uid}) could not save into its state directory: "
                               + "; ".join(wrong[:5]) + (f"; and {len(wrong) - 5} more" if len(wrong) > 5 else ""))
        return app_uid

    def stop(self):
        """Terminate the app and confirm it is gone."""
        for _ in range(STOP_TRIES):
            pid = self.driver.pid()
            if pid is None:
                return
            # A process that exits between the list and the request makes this fail; the next list says.
            self._devicectl("process", "terminate", "--pid", str(pid), timeout=60)
            self.sleep(1.0)
        raise Unmeasurable(f"the app was still running after {STOP_TRIES} terminate requests")

    def read(self, dest, stop=True):
        """Copy the state directory to `dest` (absent beforehand) and return its tree_manifest, or None
        when the app has no state directory on the phone. `stop=False` leaves the app running: a warm
        series reads the timing file between returns and a terminated app would make the next return a
        relaunch (the copy is a read; the app is only stopped first so it cannot write over a change)."""
        if stop:
            self.stop()
        p = self._devicectl("copy", "from", "--domain-type", "appDataContainer", "--domain-identifier", TEST_BUNDLE,
                            "--source", STATE_DIR, "--destination", dest)
        if p.returncode:
            said = (p.stderr or p.stdout or "").strip()
            if NO_FILE_NODE in said:
                return None
            raise Unmeasurable(f"devicectl could not read the app's saved state: {said[-300:]}")
        if not os.path.isdir(dest):
            raise Unmeasurable("devicectl reported a copy of the app's saved state but wrote no directory")
        return tree_manifest(dest)

    def put(self, source, scratch):
        """Make the state directory exactly `source`, owned by the app, then read it back and compare
        every file and directory byte for byte, and check every entry's owner and mode. Returns the
        manifest read back."""
        self.stop()
        others = sorted(path for path in self.owners() if path.startswith(STATE_PARENT + "/")
                        and path != STATE_DIR and not path.startswith(STATE_DIR + "/"))
        if others:
            raise Unmeasurable(f"{STATE_PARENT} holds more than RichOS ({', '.join(others[:3])}); writing the state "
                               "through it would remove that, so nothing was written")
        wrapper = tempfile.mkdtemp(prefix="put-", dir=scratch)
        tree = os.path.join(wrapper, os.path.basename(STATE_PARENT))
        try:
            shutil.copytree(source, os.path.join(tree, os.path.basename(STATE_DIR)))
            p = self._devicectl("copy", "to", "--domain-type", "appDataContainer", "--domain-identifier", TEST_BUNDLE,
                                "--source", tree, "--destination", STATE_PARENT, "--remove-existing-content", "true")
        finally:
            shutil.rmtree(wrapper, ignore_errors=True)
        if p.returncode:
            raise Unmeasurable(f"devicectl could not write the app's saved state: {(p.stderr or p.stdout).strip()[-300:]}")
        back = tempfile.mkdtemp(prefix="readback-", dir=scratch)
        try:
            got = self.read(os.path.join(back, "RichOS"))
            if got is None:
                raise Unmeasurable("the app's saved state is absent after it was written")
            if not same_bytes(source, os.path.join(back, "RichOS")):
                raise Unmeasurable("the app's saved state does not read back as written: "
                                   + "; ".join(tree_differences(tree_manifest(source), got) or ["a file differs"]))
            self.check_owner()
            return got
        finally:
            shutil.rmtree(back, ignore_errors=True)


def hardware_udid(listing, udid):
    """The hardware UDID xcodebuild's destination needs, for the iPhone named by `udid` in devicectl's list."""
    for d in ((json.loads(listing).get("result") or {}).get("devices") or []):
        hw = d.get("hardwareProperties") or {}
        if udid in (d.get("identifier"), hw.get("udid")):
            return hw.get("udid")
    return None


def approval_forecast(hw, runner):
    """phone-ios.py's forecast: will a UI-test session ask the phone's owner to allow automation?"""
    p = runner([sys.executable, PHONE_IOS, "approval", "--device", hw], capture_output=True, text=True, timeout=120)
    try:
        return json.loads(p.stdout)
    except ValueError:
        return {"approvalExpected": None, "why": f"phone-ios.py approval printed no forecast (exit {p.returncode})"}


def device_screen_check(hw, team, stamp_path, marker, out_dir, runner=subprocess.run):
    """After the launches: through the phone's UI-test runner (`phone-ios.py run --prebuilt --stamp`, the
    same stamped app and its runner, so nothing new is installed), bring the app to the front and wait
    for the seeded newest CEO row by its accessibility label. iOS has no devicectl screenshot; the
    runner's `wait` is the on-screen check, and its app-only shot is kept with the evidence."""
    result = {"row": marker, "onScreen": None,
              "how": "phone-ios.py run (the stamped app's own UI-test runner): activate, then wait up to 15 s for an "
                     "element whose accessibility label contains the newest seeded CEO row"}
    os.makedirs(out_dir, exist_ok=True)
    steps = os.path.join(out_dir, "steps.json")
    with open(steps, "w") as f:
        json.dump([{"do": "activate"}, {"do": "wait", "label": marker, "timeout": 15},
                   {"do": "shot", "name": "perf-seeded-condition"}], f)
    env = {**os.environ, "RICHOS_IOS_DEVICE": hw, "RICHOS_APPLE_TEAM": team}
    try:
        p = runner([sys.executable, PHONE_IOS, "run", steps, "--out", os.path.join(out_dir, "run"), "--prebuilt",
                    "--stamp", stamp_path, "--allowance", str(SCREEN_ALLOWANCE_S)],
                   capture_output=True, text=True, env=env, timeout=SCREEN_ALLOWANCE_S + 600)
    except (subprocess.TimeoutExpired, OSError) as e:
        result["why"] = f"the UI-test runner did not answer: {e}"
        return result
    try:
        summary = json.loads(p.stdout)
    except ValueError:
        result["why"] = f"phone-ios.py run printed no summary (exit {p.returncode}): {(p.stderr or '').strip()[-300:]}"
        return result
    result["evidence"] = summary.get("out") or os.path.join(out_dir, "run")
    rows = []
    try:
        with open(os.path.join(result["evidence"], "steps.jsonl")) as f:
            rows = [json.loads(line) for line in f if line.strip()]
    except (OSError, ValueError):
        pass
    waited = next((r for r in rows if r.get("do") == "wait"), None)
    if waited is None:
        result["why"] = f"the runner logged no wait step: {summary.get('error') or (summary.get('failed') or ['no step ran'])[0]}"
        return result
    result["onScreen"] = waited.get("ok") is True
    if waited.get("ok"):
        result["label"] = (waited.get("detail") or {}).get("label")
        result["waitedMs"] = (waited.get("detail") or {}).get("waitedMs")
    else:
        result["why"] = f"the row was not on screen: {waited.get('error')}"
    return result


# ---------------------------------------------------------------------------------------------
# Taps with no profiler attached: the app's own clocks (native-ios/App/Platform/LaunchTiming.swift)
# ---------------------------------------------------------------------------------------------

# The app writes its launch and return lines only while this file is in its saved-state directory.
TIMING_MARKER = "perf-launch-timing.on"
TIMING_FILE = "perf-launch-timing.jsonl"
ICON_LABEL = test_copy.TEST_DISPLAY_NAME  # the test copy's name on the Home Screen, beside the CEO's RichConnect
TAP_SETTLE_S = 2.0   # after a terminate, before the tap
TAP_DWELL_S = 4.0    # after a tap: the launch or return finishes and the app has written its lines
# Where a return's probe touch lands: the middle of the transcript on an iPhone SE in portrait, where
# no control is. Its offsets after the tap that brings the app back, cycled over the returns.
PROBE_POINT = (187.0, 300.0)
# 0.9 s is the positive control: past the scene's activation (about 0.47 s after the tap on the test
# iPhone), a touch must reach the app, or the probe itself is not working.
PROBE_OFFSETS_S = (0.1, 0.2, 0.3, 0.45, 0.6, 0.9)
TAP_ALLOWANCE_MAX_S = 1800
UNPROFILED_METHOD = (
    "No profiler: the phone's UI-test runner taps the RichConnect icon on the Home Screen (a cold launch after "
    "`terminate`, a return after Home), and the app writes its own times (LaunchTiming.swift): the kernel's start "
    "time of the process (sysctl KERN_PROC_PID p_starttime, the wall clock at spawn), its first line of code, scene "
    "activation, the useful draw and input-ready (the main run loop idle after the commit that made the viewport and "
    "composer ready). The runner's `tapAt` is the phone-clock moment the tap was asked for, before XCTest synthesized "
    "it; one wall clock on one phone. Presentation of the input-ready commit is not visible in the app: the profiled "
    "series measured it at 39-48 ms after input-ready")


# The test iPhone keeps RichConnect on its Home Screen's page 2 of 2 (read from SpringBoard's tree,
# 2026-10-02). Home from the Home Screen shows page 1, so Home then one swipe left shows page 2 whichever
# page was showing. Every series starts on that page and the tap series ends on it (the launch animation from
# page 1 runs 100-130 ms longer than from the icon's page). Nothing on the Home Screen is moved.
HOME_ICONS = "Home screen icons"
ICON_PAGE_SWIPES = 1
ICON_PAGE = ICON_PAGE_SWIPES + 1  # Home shows page 1; each swipe left is one page on


def _to_icon_page():
    return [{"do": "home"}, {"do": "sleep", "seconds": 1}] + [
        step for _ in range(ICON_PAGE_SWIPES)
        for step in ({"do": "swipe", "in": "springboard", "id": HOME_ICONS, "direction": "left"},
                     {"do": "sleep", "seconds": 1})]


def tap_steps(launches, returns, away, offsets=PROBE_OFFSETS_S, point=PROBE_POINT):
    """The runner's list: `launches` cold launches by a tap on the icon, then `returns` returns by a tap
    on the icon after `away` s at Home, each with a touch at `point` `offsets[i]` s after that tap."""
    steps = [{"do": "home"}, {"do": "sleep", "seconds": TAP_SETTLE_S}]
    for n in range(1, launches + 1):
        steps += ([{"do": "terminate"}, {"do": "sleep", "seconds": TAP_SETTLE_S}] + _to_icon_page()
                  + [{"do": "mark", "label": f"launch {n}"},
                     {"do": "tap", "in": "springboard", "label": ICON_LABEL, "timeout": 10},
                     {"do": "sleep", "seconds": TAP_DWELL_S}])
    for n in range(1, returns + 1):
        steps += ([{"do": "home"}, {"do": "sleep", "seconds": away}] + _to_icon_page()
                  + [{"do": "mark", "label": f"return {n}"},
                     {"do": "tapThen", "in": "springboard", "label": ICON_LABEL, "timeout": 10,
                      "after": offsets[(n - 1) % len(offsets)], "at": list(point)},
                     {"do": "sleep", "seconds": TAP_DWELL_S}])
    return steps + _to_icon_page()  # the phone is left on the icon's page, not page 1


def parse_timing(text):
    """The app's timing lines; a line that is not one JSON object (a torn last write) is skipped."""
    out = []
    for line in text.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if isinstance(event, dict) and isinstance(event.get("e"), str) and isinstance(event.get("wallUs"), int):
            out.append(event)
    return sorted(out, key=lambda e: e["wallUs"])


def _first_event(events, name, pid=None, after_us=None, before_us=None):
    for e in events:
        if (e["e"] == name and (pid is None or e.get("pid") == pid) and (after_us is None or e["wallUs"] >= after_us)
                and (before_us is None or e["wallUs"] < before_us)):
            return e
    return None


def _ms(a_us, b_us):
    return None if a_us is None or b_us is None else round((b_us - a_us) / 1000.0, 3)


def tap_launch_samples(rows, events):
    """One sample per logged tap launch: the process that started after the tap (exactly one), and its
    marks from the kernel's start time. Returns (samples, rejected)."""
    taps = [r for r in rows if r.get("do") == "tap" and r.get("ok") and (r.get("detail") or {}).get("tapAt")]
    samples, rejected = [], []
    for n, tap in enumerate(taps, 1):
        tap_us = tap["detail"]["tapAt"] * 1e6
        # The launch belongs to this tap: it starts after it, before the next tap, and within the dwell.
        next_us = min(taps[n]["detail"]["tapAt"] * 1e6 if n < len(taps) else float("inf"),
                      tap_us + (TAP_DWELL_S + 6.0) * 1e6)
        procs = [e for e in events if e["e"] == "process" and e.get("startUs", -1) > 0
                 and tap_us <= e["startUs"] < next_us]
        if len(procs) != 1:
            rejected.append({"trial": n, "why": f"{len(procs)} processes started between this tap and the next"})
            continue
        p = procs[0]
        start, pid = p["startUs"], p["pid"]
        marks = {name: _first_event(events, name, pid=pid, after_us=start, before_us=next_us)
                 for name in ("did-activate", "composer-ready", "useful-content", "viewport-ready", "input-ready")}
        if marks["input-ready"] is None:
            rejected.append({"trial": n, "pid": pid, "why": "no input-ready line from the launched process"})
            continue
        wall = {k: (v["wallUs"] if v else None) for k, v in marks.items()}
        samples.append({"trial": n, "pid": pid,
                        "tapToProcessStartMs": _ms(tap_us, start),
                        "processStartToMainMs": _ms(start, p["wallUs"]),
                        "processStartToActiveMs": _ms(start, wall["did-activate"]),
                        "processStartToComposerReadyMs": _ms(start, wall["composer-ready"]),
                        "processStartToUsefulMs": _ms(start, wall["useful-content"]),
                        "processStartToViewportReadyMs": _ms(start, wall["viewport-ready"]),
                        "processStartToInputReadyMs": _ms(start, wall["input-ready"]),
                        "tapToInputReadyMs": _ms(tap_us, wall["input-ready"])})
    return samples, rejected


def tap_return_samples(rows, events):
    """One sample per logged return (a `tapThen` on the icon): the scene's transitions in the retained
    process, and what became of the probe touch: received or not, when, and in which scene state."""
    probes = [r for r in rows if r.get("do") == "tapThen" and r.get("ok") and (r.get("detail") or {}).get("synthesizedAt")]
    samples, rejected = [], []
    for n, probe in enumerate(probes, 1):
        start = probe["detail"]["synthesizedAt"] * 1e6
        # The return belongs to this tap: before the next one, and within the dwell (the last one's window
        # would otherwise reach the on-screen check's later launch and read it as a relaunch).
        end = min(probes[n]["detail"]["synthesizedAt"] * 1e6 if n < len(probes) else float("inf"),
                  start + (TAP_DWELL_S + 6.0) * 1e6)
        window = [e for e in events if e["wallUs"] >= start and (end is None or e["wallUs"] < end)]
        if any(e["e"] == "process" for e in window):
            rejected.append({"trial": n, "why": "a new process started: a relaunch, not a return"})
            continue
        marks = {name: _first_event(window, name) for name in
                 ("will-enter-foreground", "did-activate", "foreground-useful", "input-ready")}
        if marks["did-activate"] is None:
            rejected.append({"trial": n, "why": "the app logged no activation after the tap"})
            continue
        wall = {k: (v["wallUs"] if v else None) for k, v in marks.items()}
        touch = next((e for e in window if e["e"] == "touch"), None)
        sample = {"trial": n, "probeAfterMs": round(float(probe.get("after", 0)) * 1000, 1),
                  "enterForegroundMs": _ms(start, wall["will-enter-foreground"]),
                  "activeMs": _ms(start, wall["did-activate"]),
                  "foregroundUsefulMs": _ms(start, wall["foreground-useful"]),
                  "inputReadyMs": _ms(start, wall["input-ready"]),
                  "touch": {"received": touch is not None}}
        if touch is not None:
            event_us = touch["wallUs"] - (touch["uptime"] - touch["eventUptime"]) * 1e6
            sample["touch"].update({"eventMs": _ms(start, event_us), "receivedMs": _ms(start, touch["wallUs"]),
                                    "scene": touch.get("scene"), "view": touch.get("view"),
                                    "beforeActive": touch["wallUs"] < wall["did-activate"],
                                    "receivedToActiveMs": _ms(touch["wallUs"], wall["did-activate"])})
        samples.append(sample)
    return samples, rejected


def unprofiled_summary(launches, returns):
    def stats(key, samples):
        values = [s[key] for s in samples if s.get(key) is not None]
        return perfcore.stats(values) if values else None
    cold = {"n": len(launches)}
    for key in ("tapToProcessStartMs", "processStartToMainMs", "processStartToActiveMs", "processStartToUsefulMs",
                "processStartToInputReadyMs", "tapToInputReadyMs"):
        cold[key] = stats(key, launches)
    by_offset = {}
    for s in returns:
        slot = by_offset.setdefault(str(s["probeAfterMs"]), {"returns": 0, "received": 0, "receivedBeforeActive": 0,
                                                            "scenes": {}, "receivedMs": []})
        slot["returns"] += 1
        t = s["touch"]
        if t["received"]:
            slot["received"] += 1
            slot["receivedBeforeActive"] += 1 if t["beforeActive"] else 0
            slot["scenes"][t["scene"]] = slot["scenes"].get(t["scene"], 0) + 1
            slot["receivedMs"].append(t["receivedMs"])
    warm = {"n": len(returns), "activeMs": stats("activeMs", returns), "inputReadyMs": stats("inputReadyMs", returns),
            "enterForegroundMs": stats("enterForegroundMs", returns), "probeTouches": by_offset}
    return {"cold": cold, "warm": warm}


def device_tap_series(hw, team, stamp_path, launches, returns, away, out_dir, runner=subprocess.run, screen_recording=False):
    """The tap launches and returns through the stamped build's runner. Returns (rows, summary-or-error)."""
    os.makedirs(out_dir, exist_ok=True)
    steps = tap_steps(launches, returns, away)
    # phone-ios.py's measured medians: terminate 1.08, home 0.46, swipe 2.69, tap 2.0 s (tapThen 4.0, unmeasured).
    to_page = 0.46 + 1 + ICON_PAGE_SWIPES * (2.69 + 1)
    expected = (launches * (1.08 + TAP_SETTLE_S + to_page + 2.0 + TAP_DWELL_S)
                + returns * (0.46 + away + to_page + 4.0 + TAP_DWELL_S))
    if expected * 1.25 > TAP_ALLOWANCE_MAX_S:
        return [], (f"{launches} launches and {returns} returns need about {int(expected)} s, more than one runner "
                    f"session's {TAP_ALLOWANCE_MAX_S} s; run fewer")
    return device_run_steps(hw, team, stamp_path, steps, out_dir, runner, screen_recording)


def set_icon_page(hw, team, stamp_path, out_dir, runner=subprocess.run):
    """Bring the Home Screen to the page that holds the test app's icon (the tap series' own steps), before a
    series that launches without a tap. Returns (what was done, error or None)."""
    rows, error = device_run_steps(hw, team, stamp_path, _to_icon_page(), out_dir, runner)
    ok = bool(rows) and all(r.get("ok") for r in rows)
    page = {"page": ICON_PAGE, "setBy": f"Home, then {ICON_PAGE_SWIPES} swipe left on '{HOME_ICONS}'",
            "stepsOk": ok, "evidence": out_dir}
    if not error and not ok:
        error = "the steps that bring the Home Screen to the icon's page did not all succeed"
    return page, error


def device_run_steps(hw, team, stamp_path, steps, out_dir, runner=subprocess.run, screen_recording=False):
    """The steps through the stamped build's UI-test runner. Returns (rows, error or None)."""
    os.makedirs(out_dir, exist_ok=True)
    steps_path = os.path.join(out_dir, "steps.json")
    with open(steps_path, "w") as f:
        json.dump(steps, f)
    # The largest allowance, as the on-screen check asks: the device runner's limit also counts the wait
    # for native-work's CPU admission before the session starts.
    allowance = TAP_ALLOWANCE_MAX_S
    env = {**os.environ, "RICHOS_IOS_DEVICE": hw, "RICHOS_APPLE_TEAM": team}
    try:
        p = runner([sys.executable, PHONE_IOS, "run", steps_path, "--out", os.path.join(out_dir, "run"), "--prebuilt",
                    "--stamp", stamp_path, "--allowance", str(allowance)]
                   + (["--screen-recording"] if screen_recording else []),
                   capture_output=True, text=True, env=env, timeout=allowance + 1800)
    except (subprocess.TimeoutExpired, OSError) as e:
        return [], f"the UI-test runner did not answer: {e}"
    try:
        summary = json.loads(p.stdout)
    except ValueError:
        return [], f"phone-ios.py run printed no summary (exit {p.returncode}): {(p.stderr or '').strip()[-300:]}"
    try:
        with open(os.path.join(summary.get("out") or os.path.join(out_dir, "run"), "steps.jsonl")) as f:
            rows = [json.loads(line) for line in f if line.strip()]
    except (OSError, ValueError) as e:
        return [], f"the runner's step log could not be read: {e}"
    by_index = {i: s for i, s in enumerate(steps)}
    for r in rows:  # the step's own definition beside its result (a probe's offset)
        if r.get("do") == "tapThen" and isinstance(r.get("i"), int):
            r["after"] = by_index.get(r["i"], {}).get("after")
    return rows, None


def _tap_series(args, record, state, seed, scratch, hw, work):
    """With the timing marker added to the seeded state, the tap launches and returns, then the app's own
    lines read off the phone. The seeded conversation is unchanged; the restore removes the marker and
    the lines with the rest. Returns the number of failures (0 or 1)."""
    launches, returns = getattr(args, "tap_launches", 0) or 0, getattr(args, "tap_returns", 0) or 0
    timed = os.path.join(scratch, "timed", "RichOS")
    shutil.copytree(seed, timed)
    open(os.path.join(timed, TIMING_MARKER), "w").close()
    out_dir = os.path.join(work, "taps")
    record["conditions"].setdefault("homePage", {})["taps"] = {
        "page": ICON_PAGE, "setBy": "every tap is preceded by Home and the swipe to the icon's page; the list ends on it"}
    try:
        state.put(timed, scratch)
    except (Unmeasurable, subprocess.TimeoutExpired) as e:
        record["phases"]["taps"] = f"failed: {e}"
        record["notMeasured"].append({"what": "unprofiled taps", "why": f"the timing marker could not be written: {e}"})
        return 1
    rows, error = device_tap_series(hw, os.environ["RICHOS_APPLE_TEAM"], args.stamp, launches, returns,
                                    getattr(args, "away", 2.0), out_dir,
                                    screen_recording=getattr(args, "screen_recording", False))
    got = os.path.join(out_dir, "after", "RichOS")
    read_back = None
    try:
        read_back = state.read(got)
        with open(os.path.join(got, TIMING_FILE)) as f:
            text = f.read()
    except (Unmeasurable, OSError, subprocess.TimeoutExpired) as e:
        text, error = "", error or f"the app's timing lines could not be read: {e}"
    finally:
        if os.path.isdir(got):  # keep only the app's timing lines with the evidence
            with open(os.path.join(out_dir, "after-manifest.json"), "w") as f:
                json.dump(read_back, f, indent=1)  # what the directory held, before the seeded files are dropped
            for name in os.listdir(got):
                if name != TIMING_FILE:
                    path = os.path.join(got, name)
                    shutil.rmtree(path, ignore_errors=True) if os.path.isdir(path) else os.remove(path)
    events = parse_timing(text)
    cold, cold_rejected = tap_launch_samples(rows, events)
    warm, warm_rejected = tap_return_samples(rows, events)
    record["unprofiled"] = {"method": UNPROFILED_METHOD, "evidence": out_dir, "requested": {"launches": launches, "returns": returns},
                            "launches": cold, "returns": warm, "rejected": {"launches": cold_rejected, "returns": warm_rejected},
                            "summary": unprofiled_summary(cold, warm)}
    if error:
        record["unprofiled"]["error"] = error
    short = len(cold) < launches or len(warm) < returns
    record["phases"]["taps"] = (f"{len(cold)} of {launches} launches, {len(warm)} of {returns} returns"
                                + (f"; {error}" if error else ""))
    if short or error:
        record["notMeasured"].append({"what": "unprofiled taps", "why": record["phases"]["taps"]})
        return 1
    return 0


# Launch requests with nothing attached (`--launches N`): the iPhone's `coldLaunch` (CEO 2026-10-02: the speed
# check judges what the user sees, never an Instruments-traced launch, which adds ~530 ms; §102). Its
# clock was measured on 2026-10-02 (median 582 ms over 20 starts); the standard that judges it is
# perfcore.COLD_STANDARD (§104).
LAUNCH_DWELL_S = 4.0  # after a launch request: the app has started and written its lines
HOME_SETTLE_S = 1.5   # after terminating the app in front: the Home Screen is back and still
LAUNCH_REQUEST = re.compile(r'^(?P<ts>[A-Z][a-z]{2}\s+\d+\s+\d\d:\d\d:\d\d\.\d+) SpringBoard\(FrontBoard\)\[\d+\] <\w+>: '
                            r'\[FBSystemService\]\[0x[0-9a-fA-F]+\] Received request to open "' + re.escape(TEST_BUNDLE) + '"')
APP_LOG_LINE = re.compile(r'^(?P<ts>[A-Z][a-z]{2}\s+\d+\s+\d\d:\d\d:\d\d\.\d+) ' + re.escape(APP_EXECUTABLE)
                          + r'(?:\([^)]*\))?\[(?P<pid>\d+)\]')
LAUNCH_END = ("the app's own input-ready mark (the main run loop idle after the commit that made the saved viewport "
              "and composer ready)")
LAUNCH_METHOD = (
    "No profiler and no UI test: the app in front is terminated, which leaves the Home Screen in front, then it "
    "is opened from there with an ordinary launch request (devicectl process launch); start 1 is the first launch "
    "and is not judged. Start: SpringBoard's 'Received request to open' for RichConnect, read from the phone's own "
    "log (idevicesyslog). End: " + LAUNCH_END + ", written by the app (LaunchTiming.swift) on the same wall clock; "
    "the join is checked per launch against the app's first log line and its kernel start time.")


def syslog_wall_us(stamp, year):
    """'Oct  2 23:04:02.637698' (the phone's log, its local time) -> epoch microseconds in this Mac's time zone."""
    dt = datetime.datetime.strptime(f"{year} {' '.join(stamp.split())}", "%Y %b %d %H:%M:%S.%f")
    return int(round(dt.timestamp() * 1e6))


def launch_request_samples(events, log_text, year, skip=0):
    """One sample per process the app logged after the first `skip` (the first, unjudged launch): the launch
    request SpringBoard logged for it (exactly one in the 3 s before the kernel started it), and its marks.
    Returns (samples, rejected)."""
    requests, first_line = [], {}
    for line in log_text.splitlines():
        m = LAUNCH_REQUEST.match(line)
        if m:
            requests.append(syslog_wall_us(m["ts"], year))
            continue
        m = APP_LOG_LINE.match(line)
        if m and int(m["pid"]) not in first_line:
            first_line[int(m["pid"])] = syslog_wall_us(m["ts"], year)
    samples, rejected = [], []
    procs = [e for e in events if e["e"] == "process" and e.get("startUs", -1) > 0][skip:]
    for n, p in enumerate(procs, 1):
        start, pid = p["startUs"], p["pid"]
        asked = [r for r in requests if start - 3_000_000 <= r <= start]
        if len(asked) != 1:
            rejected.append({"trial": n, "pid": pid, "why": f"{len(asked)} launch requests in the 3 s before the process started"})
            continue
        seen = first_line.get(pid)
        if seen is None or not 0 <= seen - start <= 3_000_000:
            rejected.append({"trial": n, "pid": pid, "why": "the phone's log and the app's clock do not join (its first "
                                                            "log line is not within 3 s after its kernel start)"})
            continue
        ready = _first_event(events, "input-ready", pid=pid, after_us=start)
        if ready is None:
            rejected.append({"trial": n, "pid": pid, "why": "no input-ready line from the launched process"})
            continue
        samples.append({"trial": n, "pid": pid, "requestToProcessStartMs": _ms(asked[0], start),
                        "processStartToMainMs": _ms(start, p["wallUs"]),
                        "processStartToInputReadyMs": _ms(start, ready["wallUs"]),
                        "requestToInputReadyMs": _ms(asked[0], ready["wallUs"])})
    return samples, rejected


def launch_metric(samples, rejected, evidence):
    """The launch-request series as the iPhone's `coldLaunch`: the request to input-ready, per launch (starts 2..)."""
    ms = [round(s["requestToInputReadyMs"], 3) for s in samples]
    summary = perfcore.stats(ms)
    phases = {name: perfcore.stats([round(s[name], 3) for s in samples])
              for name in ("requestToProcessStartMs", "processStartToMainMs", "processStartToInputReadyMs")}
    return {"method": LAUNCH_METHOD, "startBoundary": "SpringBoard's launch request", "endBoundary": LAUNCH_END,
            "samplesMs": ms, "stats": summary, "budget": perfcore.compare("coldLaunch", summary), "phaseStatsMs": phases,
            "rejected": rejected, "evidence": evidence}


def launch_starts_ms(samples):
    """The series for perfcore.cold_verdict: start 1 (the unjudged first launch) is None, then starts 2.."""
    return [None] + [s["requestToInputReadyMs"] for s in samples]


def _read_launch_evidence(state, got, log_path, stop=True):
    """The app's timing lines read off the phone and the phone's log so far: (events, log text)."""
    if os.path.isdir(got):
        shutil.rmtree(got, ignore_errors=True)
    state.read(got, stop=stop)
    with open(os.path.join(got, TIMING_FILE)) as f:
        text = f.read()
    for name in os.listdir(got):  # keep only the app's timing lines with the evidence
        if name != TIMING_FILE:
            path = os.path.join(got, name)
            shutil.rmtree(path, ignore_errors=True) if os.path.isdir(path) else os.remove(path)
    with open(log_path, errors="replace") as f:
        return parse_timing(text), f.read()


def run_launch_series(n, launch, stop_app, read, sleep, log=lambda text: None):
    """The cold-start standard's series: `n` starts (start 1 unjudged), stopping at the first of starts 2-5 over
    the iPhone limit (perfcore §104). `launch()` makes a launch request, `stop_app()` terminates the app,
    `read()` returns the judged samples so far. Returns (stopped_early_verdict or None, error or None)."""
    for start in range(1, n + 1):
        if start > 1:
            stop_app()
            sleep(HOME_SETTLE_S)
        launch()
        sleep(LAUNCH_DWELL_S)
        if 2 <= start <= perfcore.COLD_EARLY_LAST:
            stop_app()
            samples = read()
            starts = launch_starts_ms(samples)
            if len(starts) < start:
                return None, f"cold start {start} gave no timing (rejected join); the series stops"
            verdict = perfcore.cold_verdict("ios", starts, total=n)
            if verdict["stop"]:
                log(verdict["why"])
                return verdict, None
    return None, None


def _launch_series(args, record, state, seed, scratch, hw, work, driver, popen, sleep):
    """`--launches N` cold starts with nothing attached, the phone's log captured for their requests, then the
    app's own lines read off the phone. The restore removes the timing marker and lines with the rest.
    Returns the number of failures (0 or 1)."""
    n = args.launches
    timed = os.path.join(scratch, "launch-timed", "RichOS")
    shutil.copytree(seed, timed)
    open(os.path.join(timed, TIMING_MARKER), "w").close()
    out_dir = os.path.join(work, "launches")
    os.makedirs(out_dir)
    log_path = os.path.join(out_dir, "phone.log")
    got = os.path.join(out_dir, "after", "RichOS")
    try:
        state.put(timed, scratch)
    except (Unmeasurable, subprocess.TimeoutExpired) as e:
        record["phases"]["launches"] = f"failed: {e}"
        record["notMeasured"].append({"what": "coldLaunch", "why": f"the timing marker could not be written: {e}"})
        return 1
    error = None
    year = datetime.datetime.now().year

    def read():
        events, log_text = _read_launch_evidence(state, got, log_path)
        return launch_request_samples(events, log_text, year, skip=1)[0]

    with open(log_path, "w") as log:
        reader = popen(["idevicesyslog", "-u", hw], stdout=log, stderr=subprocess.STDOUT, text=True)
        try:
            sleep(2.0)  # the relay is connected before the first request
            _, error = run_launch_series(n, lambda: driver.launch(TEST_BUNDLE), state.stop, read, sleep)
            state.stop()  # the phone is left at its Home Screen
        except (Unmeasurable, subprocess.TimeoutExpired) as e:
            error = f"the launches stopped: {e}"
        finally:
            if reader.poll() is None:  # only the reader this function started
                reader.terminate()
                reader.wait(timeout=30)
    samples, rejected = [], []
    try:
        events, log_text = _read_launch_evidence(state, got, log_path)
        samples, rejected = launch_request_samples(events, log_text, year, skip=1)
    except (Unmeasurable, OSError, subprocess.TimeoutExpired) as e:
        error = error or f"the app's timing lines could not be read: {e}"
    if samples:
        record["metrics"]["coldLaunch"] = launch_metric(samples, rejected, out_dir)
        record["metrics"]["coldStandard"] = perfcore.cold_verdict("ios", launch_starts_ms(samples), total=n)
    record["unprofiledLaunches"] = {"method": LAUNCH_METHOD, "evidence": out_dir, "requested": n,
                                    "launches": samples, "rejected": rejected}
    stopped = (record["metrics"].get("coldStandard") or {}).get("stop")
    record["phases"]["launches"] = f"{len(samples) + 1} of {n} launches" + (f"; {error}" if error else "")
    if error or (len(samples) + 1 < n and not stopped):
        record["notMeasured"].append({"what": "coldLaunch", "why": record["phases"]["launches"]})
        return 1
    return 0 if not stopped else 1


# Warm returns with nothing attached (`--returns N`, and `--warm N` on a phone): another app is put in front with
# devicectl, then RichConnect is opened with `devicectl process launch` without terminating it (the process is
# retained). The clock is the cold clock: SpringBoard's launch request (the phone's own log) to the app's own
# input-ready mark. No test runner, no Instruments. (A will-enter-foreground start was retracted by the CEO
# 2026-10-03: it is not what a person sees.) Start 1 is the first return and is not judged.
OTHER_APP_BUNDLE = "com.apple.Preferences"
RETURN_AWAY_S = 2.0
RETURN_DWELL_S = 3.0
WARM_METHOD = (
    "No profiler and no UI test: another app (Settings) is put in front with devicectl, then RichConnect is opened "
    "with `devicectl process launch` without terminating it; the process is the same one (a new process is "
    "rejected). Start: SpringBoard's 'Received request to open' for RichConnect, from the phone's own log "
    "(idevicesyslog). End: the app's own input-ready mark after that request (LaunchTiming.swift). Start 1 is the "
    "first return and is not judged.")


def return_request_samples(events, log_text, year, skip=1):
    """One sample per launch request after the first (the setup launch) and after `skip` returns (the unjudged
    first return): the app's input-ready after it, in the same process. Returns (samples, rejected)."""
    requests = sorted(syslog_wall_us(m["ts"], year) for m in (LAUNCH_REQUEST.match(l) for l in log_text.splitlines()) if m)
    returns = requests[1:]
    samples, rejected = [], []
    for n, req in enumerate(returns, 1):
        end = returns[n] if n < len(returns) else float("inf")
        if any(e["e"] == "process" and req <= e["wallUs"] < end for e in events):
            rejected.append({"trial": n, "why": "a new process started: a relaunch, not a return"})
            continue
        ready = next((e for e in events if e["e"] == "input-ready" and req <= e["wallUs"] < end), None)
        if ready is None:
            rejected.append({"trial": n, "why": "no input-ready line from the retained process after the launch request"})
            continue
        ms = _ms(req, ready["wallUs"])
        if not 0 < ms < 10_000:
            rejected.append({"trial": n, "why": f"the phone's log and the app's clock do not join ({ms} ms)"})
            continue
        samples.append({"trial": n, "requestToInputReadyMs": ms})
    return [s for s in samples if s["trial"] > skip], rejected


def return_starts_ms(samples):
    return [None] + [s["requestToInputReadyMs"] for s in samples]


def run_return_series(n, background, launch, read, sleep, log=lambda text: None):
    """`n` warm returns (return 1 unjudged), stopping at the first of returns 2-5 over the warm limit (§104; an
    unset limit stops nothing). Returns (verdict that stopped it or None, error or None)."""
    launch()  # the setup launch: the app is running
    sleep(LAUNCH_DWELL_S)
    for start in range(1, n + 1):
        background()
        sleep(RETURN_AWAY_S)
        launch()
        sleep(RETURN_DWELL_S)
        if 2 <= start <= perfcore.COLD_EARLY_LAST:
            starts = return_starts_ms(read())
            if len(starts) < start:
                return None, f"warm start {start} gave no timing (rejected); the series stops"
            try:
                verdict = perfcore.warm_verdict("ios", starts, total=n)
            except perfcore.LimitNotSet:
                continue
            if verdict["stop"]:
                log(verdict["why"])
                return verdict, None
    return None, None


def _return_series(args, record, state, seed, scratch, hw, work, driver, popen, sleep):
    """`--returns N` warm returns with nothing attached; the iPhone's warmResume and its standard verdict."""
    n = args.returns
    timed = os.path.join(scratch, "return-timed", "RichOS")
    shutil.copytree(seed, timed)
    open(os.path.join(timed, TIMING_MARKER), "w").close()
    out_dir = os.path.join(work, "returns")
    os.makedirs(out_dir)
    log_path = os.path.join(out_dir, "phone.log")
    got = os.path.join(out_dir, "after", "RichOS")
    try:
        state.put(timed, scratch)
    except (Unmeasurable, subprocess.TimeoutExpired) as e:
        record["phases"]["returns"] = f"failed: {e}"
        record["notMeasured"].append({"what": "warmResume", "why": f"the timing marker could not be written: {e}"})
        return 1
    year = datetime.datetime.now().year
    error = None

    def read():
        events, log_text = _read_launch_evidence(state, got, log_path, stop=False)  # the process must survive
        return return_request_samples(events, log_text, year)[0]

    with open(log_path, "w") as log:
        reader = popen(["idevicesyslog", "-u", hw], stdout=log, stderr=subprocess.STDOUT, text=True)
        try:
            sleep(2.0)
            _, error = run_return_series(n, lambda: driver.launch(OTHER_APP_BUNDLE), lambda: driver.launch(TEST_BUNDLE),
                                         read, sleep)
        except (Unmeasurable, subprocess.TimeoutExpired) as e:
            error = f"the returns stopped: {e}"
        finally:
            if reader.poll() is None:
                reader.terminate()
                reader.wait(timeout=30)
    samples, rejected = [], []
    try:
        events, log_text = _read_launch_evidence(state, got, log_path)
        samples, rejected = return_request_samples(events, log_text, year)
    except (Unmeasurable, OSError, subprocess.TimeoutExpired) as e:
        error = error or f"the app's timing lines could not be read: {e}"
    if samples:
        ms = [round(s["requestToInputReadyMs"], 3) for s in samples]
        summary = perfcore.stats(ms)
        record["metrics"]["warmResume"] = {"method": WARM_METHOD, "samplesMs": ms, "stats": summary,
                                           "budget": perfcore.compare("warmResume", summary), "rejected": rejected,
                                           "evidence": out_dir}
        record["metrics"]["warmStandard"] = _warm_standard(return_starts_ms(samples), n)
    stopped = (record["metrics"].get("warmStandard") or {}).get("stop")
    record["phases"]["returns"] = f"{len(samples) + 1} of {n} returns" + (f"; {error}" if error else "")
    if error or (len(samples) + 1 < n and not stopped):
        record["notMeasured"].append({"what": "warmResume", "why": record["phases"]["returns"]})
        return 1
    return 1 if stopped else 0


def _warm_standard(starts_ms, total):
    try:
        return perfcore.warm_verdict("ios", starts_ms, total=total)
    except perfcore.LimitNotSet as exc:
        return {"verdict": "LIMIT NOT SET", "why": str(exc), "method": "the warm-start standard has placeholder limits"}


SEEDED_BY_DEVICE = ("condition.py wrote Android's fixture files; `rios perf-seed` translated them through the app's own "
                    "core (EffectRunner .persist) into its saved state and loaded every row back; the app was terminated, "
                    "its saved-state directory on the iPhone (Library/Application Support/RichOS in its data container) "
                    "was replaced by exactly those files with devicectl and read back byte for byte. The pairing names a "
                    "host under .invalid and no device id, so the app reads no key and opens no connection: the Mac is "
                    "unreachable")


def _device_refusals(args, stamp):
    """Everything that can refuse a seeded iPhone run without asking the phone anything."""
    if not args.stamp or not stamp or not stamp.get("artifact"):
        raise Refused("an iPhone is seeded only for a stamped build (--stamp, perf.py stamp or a store entry's "
                      "stamp.json): the stamp names the installed app, its configuration, and the UI-test runner "
                      "beside it that checks the seeded conversation on screen")
    if getattr(args, "mac", None) not in (None, "unreachable"):
        raise Refused("the seeded conversation's pairing names a host that never resolves: the Mac is unreachable "
                      "by construction (--mac unreachable, or leave --mac out)")
    if getattr(args, "app_arg", None):
        raise Refused("a launch argument would replace the seeded conversation; the seeded condition launches the "
                      "app with none")
    if not os.environ.get("RICHOS_APPLE_TEAM"):
        raise Refused("set RICHOS_APPLE_TEAM: the on-screen check runs the phone's UI-test runner, which needs the "
                      "signing team")
    if not evidence_root_ok(getattr(args, "evidence_dir", None)):
        raise Refused("an iPhone run needs --evidence-dir on a validated storage path: the traces, and the phone's own "
                      "saved state until it is back, are kept there")
    runner_app = os.path.join(os.path.dirname(stamp["artifact"]), RUNNER_APP)
    if not os.path.isdir(runner_app):
        raise Refused(f"the stamped build has no {RUNNER_APP} beside {stamp['artifact']}: the on-screen check needs the "
                      "build's own runner (a `rios device build` or store entry has one)")


def _seeded_device(args, record, driver, hw, stamp, runner, popen, sleep, rios):
    """Back up the app's own saved state on the iPhone, write the seeded state and read it back, run the
    cold and warm series, check the seeded conversation on screen, then put the phone's own state back
    and verify it byte for byte. Whatever fails, the restore runs first; SIGTERM and SIGHUP are turned
    into an exception for the same reason. Nothing on the phone changes before the fixture has been
    written and checked on the Mac and the phone's own state has been copied off it."""
    import signal
    configuration = record["build"]["configuration"]
    if configuration not in condition.BUILDS:
        raise Refused("the stamped bundle is neither a Debug nor a Release build (development markers: some, not all), "
                      "so the condition cannot name its build")
    if not hw:
        raise Refused(f"devicectl's list names no hardware UDID for {args.device}; the UI-test runner needs it")
    ahead = approval_forecast(hw, runner)
    if ahead.get("approvalExpected") is not False:
        raise Refused("the on-screen check's UI-test session may ask the phone's owner to allow UI automation "
                      f"({ahead.get('why')}); nothing on the phone was changed. `phone-ios.py approval --device {hw}` "
                      "says why")
    rows = getattr(args, "rows", None) or condition.FILE_DEFAULT_ROWS
    os.makedirs(args.evidence_dir, exist_ok=True)
    work = tempfile.mkdtemp(prefix="ios-seed-", dir=args.evidence_dir)
    scratch = tempfile.mkdtemp(prefix="rios-perf-seed-")
    try:
        report, files = app_state_files(rows, scratch, runner, rios)
        cond = condition.for_app_state(rows, configuration, SEEDED_BY_DEVICE, report["written"], report["fixtureFiles"])
    except condition.ConditionError as e:
        shutil.rmtree(scratch, ignore_errors=True)
        raise Refused(str(e))
    except BaseException:
        shutil.rmtree(scratch, ignore_errors=True)
        raise
    marker = condition.file_fixture_marker(rows)
    cond["verified"] = {"row": marker, "onScreen": None, "why": "not checked yet"}
    seed = os.path.join(scratch, "seed", "RichOS")
    os.makedirs(seed)
    for name, data in files.items():
        with open(os.path.join(seed, name), "wb") as f:
            f.write(data)
    state = DeviceState(driver, runner, sleep)
    backup = os.path.join(work, "backup", "RichOS")
    restore_hint = (f"python3 richos/mobile/perf/perf.py ios-restore --device {args.device} --backup "
                    f"{os.path.dirname(backup)}")
    previous = {}

    def interrupted(signum, frame):
        raise Interrupted(f"signal {signum}")
    try:
        try:
            mine = state.read(backup)
            if mine is None:
                raise Refused("the app has no saved state on this iPhone (Library/Application Support/RichOS is absent); "
                              "devicectl cannot remove a directory, so the phone could not be put back exactly. Nothing "
                              "was changed")
        except BaseException:
            shutil.rmtree(work, ignore_errors=True)  # nothing was changed; no partial copy stays on the Mac
            raise
        with open(os.path.join(work, "backup", "manifest.json"), "w") as f:
            json.dump({"device": args.device, "bundle": TEST_BUNDLE, "directory": STATE_DIR, "manifest": mine,
                       "takenAt": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                       "restore": restore_hint}, f, indent=2)
        for sig in (signal.SIGTERM, signal.SIGHUP):
            try:
                previous[sig] = signal.signal(sig, interrupted)
            except ValueError:  # not the main thread (a test): nothing to catch
                pass
        record["conditions"] = {"fixture": condition.FILE_FIXTURE, "history": rows, "networkCondition": "mac-unreachable",
                                "ownStateBackup": {"entries": len(mine), "manifestSha256": manifest_sha256(mine)}}
        try:
            try:
                seeded = state.put(seed, scratch)
            except (Unmeasurable, subprocess.TimeoutExpired) as e:
                record["phases"]["seed"] = f"failed: {e}"
                record["notMeasured"].append({"what": "the seeded condition",
                                              "why": f"the seeded state could not be written ({e}); nothing was measured"})
                return 1
            record["condition"] = cond
            record["conditions"]["seededState"] = {"entries": len(seeded), "manifestSha256": manifest_sha256(seeded)}
            record["route"] = {"name": "seeded fixture",
                               "detail": "the seeded pairing names a host under .invalid and no device id: the app opens no "
                                         "connection, so no Mac is reachable",
                               "persistence": "production (the app's own Application Support/RichOS files)"}
            record["phases"]["seed"] = "measured"
            failures = _trace_classes(record, driver, args, runner, popen, sleep)
            if getattr(args, "returns", 0):
                if _home_page(record, hw, work, args, runner, "returns"):
                    failures += _return_series(args, record, state, seed, scratch, hw, work, driver, popen, sleep)
                else:
                    failures += 1
            if getattr(args, "launches", 0):
                if _home_page(record, hw, work, args, runner, "launches"):
                    failures += _launch_series(args, record, state, seed, scratch, hw, work, driver, popen, sleep)
                else:
                    failures += 1
            if getattr(args, "tap_launches", 0) or getattr(args, "tap_returns", 0):
                failures += _tap_series(args, record, state, seed, scratch, hw, work)
            verified = device_screen_check(hw, os.environ["RICHOS_APPLE_TEAM"], args.stamp, marker,
                                           os.path.join(work, "screen-check"), runner)
            failures += _record_verified(record, verified)
            return failures
        finally:
            for sig in previous:  # the restore itself is never cut short by a second signal
                signal.signal(sig, signal.SIG_IGN)
            try:
                state.put(backup, scratch)
                record["conditions"]["savedStateRestored"] = True
                shutil.rmtree(backup, ignore_errors=True)  # the phone's own conversation leaves the Mac again
            except (Unmeasurable, OSError, subprocess.TimeoutExpired) as e:
                record["conditions"]["savedStateRestored"] = False
                record["notMeasured"].append({"what": "the app's own saved state after the run",
                                              "why": f"not put back: {e}. Its copy is kept at {backup}; put it back "
                                                     f"with: {restore_hint}"})
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        shutil.rmtree(scratch, ignore_errors=True)


def _home_page(record, hw, work, args, runner, series):
    """Before a launch series: the Home Screen is brought to the icon's page and the page is written into the
    run's conditions, so a change of page shows in the record. False (the series is not run) when it could not
    be set: a series on an unknown page measures a different launch animation."""
    page, error = set_icon_page(hw, os.environ["RICHOS_APPLE_TEAM"], args.stamp, os.path.join(work, f"home-page-{series}"),
                                runner)
    record["conditions"].setdefault("homePage", {})[series] = page
    if error:
        page["error"] = error
        record["phases"][series] = f"not run: the Home Screen could not be brought to the icon's page: {error}"
        record["notMeasured"].append({"what": series, "why": record["phases"][series]})
        return False
    return True


def restore_device(device, backup_dir, runner=subprocess.run, sleep=time.sleep):
    """perf.py ios-restore: put an iPhone's own saved state back from the copy a seeded run kept
    (`<evidence>/ios-seed-*/backup`) and verify it byte for byte. For a run that was killed before
    its own restore could run; a finished run restores by itself."""
    source = os.path.join(backup_dir, "RichOS")
    manifest_path = os.path.join(backup_dir, "manifest.json")
    if not os.path.isdir(source) or not os.path.isfile(manifest_path):
        raise Refused(f"{backup_dir} holds no RichOS copy and manifest.json from a seeded run")
    with open(manifest_path) as f:
        kept = json.load(f)
    if kept.get("device") != device:
        raise Refused(f"{backup_dir} was taken from {kept.get('device')}, not {device}")
    if tree_manifest(source) != kept.get("manifest"):
        raise Refused(f"{source} is not the copy its manifest names; it was changed after the run took it")
    scratch = tempfile.mkdtemp(prefix="ios-restore-")
    try:
        got = DeviceState(Devicectl(device, runner), runner, sleep).put(source, scratch)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    shutil.rmtree(source, ignore_errors=True)
    return {"restored": True, "entries": len(got), "manifestSha256": manifest_sha256(got)}


def _as_installed(args, build):
    return condition.as_installed(args.mac, build.get("configuration"),
                                  "perf.py ios seeds nothing: the app held whatever it held (--conversation as-installed); "
                                  "the Mac's state is the operator's --mac")


SEEDED_BY = ("condition.py wrote Android's fixture files; `rios perf-seed` translated them through the app's own core "
             "(EffectRunner .persist) into its saved state and loaded every row back; the files were copied into the "
             "simulator's data container with the app terminated and read back. The pairing names a host under .invalid "
             "and no device id, so the app reads no key and opens no connection: the Mac is unreachable")


def run_ios(args, runner=subprocess.run, popen=subprocess.Popen, sleep=time.sleep, rios=RIOS):
    """perf.py ios. Returns (record, failures)."""
    if getattr(args, "device", None) and os.environ.get("RICHOS_DEVICE_VERB") != "rios":
        raise Refused("a physical iPhone is measured only through `rios device perf`, the one command line that puts "
                      "only the Release build on it (CEO 2026-10-02)")
    stamp = None
    if args.stamp:
        with open(args.stamp) as f:
            stamp = json.load(f)
    if args.expect_commit and (not stamp or not str(stamp.get("commit", "")).startswith(args.expect_commit) or stamp.get("dirty")):
        raise Refused(f"freshness mismatch: the stamp is {stamp and stamp.get('commit')} (dirty {stamp and stamp.get('dirty')}), not {args.expect_commit}")
    untraced_for_phone(args)  # a phone's --cold/--warm are the untraced series; the trace is --trace-diagnostic only
    measuring = not getattr(args, "reparse", None)
    seeding =measuring and getattr(args, "conversation", "fixture") != condition.AS_INSTALLED
    if (getattr(args, "launches", 0) or getattr(args, "returns", 0) or getattr(args, "tap_launches", 0) or getattr(args, "tap_returns", 0)) and not (seeding and getattr(args, "device", None)):
        raise Refused("--tap-launches and --tap-returns run on an iPhone with the seeded conversation: the app writes "
                      "its own times only while the seeded state carries the timing marker, and the restore removes it")
    if seeding and not args.simulator:
        _device_refusals(args, stamp)
    elif seeding:
        if getattr(args, "mac", None) not in (None, "unreachable"):
            raise Refused("the seeded conversation's pairing names a host that never resolves: the Mac is unreachable "
                          "by construction (--mac unreachable, or leave --mac out)")
        if getattr(args, "app_arg", None):
            raise Refused("a launch argument (a Debug fixture) would replace the seeded conversation; the seeded "
                          "condition launches the app with none")
    elif measuring and getattr(args, "mac", None) not in condition.MAC_STATES:
        raise Refused("--conversation as-installed needs --mac reachable or --mac unreachable: the record states the "
                      "Mac's state even when it does not control the conversation")
    record = {"schema": perfcore.SCHEMA, "platform": "ios", "startedAt": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
              "ranOnHardware": False, "metrics": {}, "phases": {}, "notMeasured": [],
              "route": {"name": "as installed", "detail": "whatever state the installed app holds"}}
    failures = 0
    if getattr(args, "reparse", None):
        failures += _reparsed(args, record, runner)
    elif args.simulator:
        record["device"] = simulator(run(["xcrun", "simctl", "list", "devices", "-j"], runner), args.simulator)
        container = run(["xcrun", "simctl", "get_app_container", args.simulator, BUNDLE, "app"], runner).strip()
        installed = perfcore.tree_sha256(container)
        if stamp and installed != stamp.get("sha256"):
            raise Refused(f"freshness mismatch: the installed app is {installed[:12]}…, the stamped build {str(stamp.get('sha256'))[:12]}…")
        record["build"] = {"bundle": BUNDLE, "commit": stamp and stamp.get("commit"), "dirty": stamp and stamp.get("dirty"),
                           "installedSha256": installed, "builtSha256": stamp and stamp.get("sha256"),
                           "configuration": build_configuration(container)}
        if seeding:
            failures += _seeded_simulator(args, record, runner, popen, sleep, rios)
        else:
            record["condition"] = _as_installed(args, record["build"])
            failures += _measure_simulator(args, record, runner, popen, sleep)
    else:
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "devices.json")
            run(["xcrun", "devicectl", "list", "devices", "--json-output", out], runner)
            with open(out) as f:
                listing = f.read()
        record["device"] = physical(listing, args.device)
        record["build"] = {"bundle": TEST_BUNDLE, "commit": stamp and stamp.get("commit"), "dirty": stamp and stamp.get("dirty"),
                           "builtSha256": stamp and stamp.get("sha256"),
                           "configuration": build_configuration(stamp and stamp.get("artifact")),
                           "note": "an iPhone's installed bundle cannot be read back; identity is the stamp of what was installed"}
        record["ranOnHardware"] = True
        driver = Devicectl(args.device, runner)
        if seeding:
            failures += _seeded_device(args, record, driver, hardware_udid(listing, args.device), stamp, runner, popen,
                                       sleep, rios)
        else:
            record["condition"] = _as_installed(args, record["build"])
            failures += _trace_classes(record, driver, args, runner, popen, sleep)
    record["notMeasured"].extend(ios_gaps(record["metrics"]))
    record["acceptance"] = ios_acceptance(record)
    record["finishedAt"] = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return record, failures


def _seeded_simulator(args, record, runner, popen, sleep, rios):
    """Seed FILE_FIXTURE as the app's saved state, measure, check the screen, and put the app's own
    saved state back whatever happened. Nothing on the simulator changes before the fixture has been
    written and checked on the Mac."""
    configuration = record["build"]["configuration"]
    if configuration not in condition.BUILDS:
        raise Refused("the installed bundle is neither a Debug nor a Release build (development markers: some, not all), "
                      "so the condition cannot name its build")
    rows = getattr(args, "rows", None) or condition.FILE_DEFAULT_ROWS
    state = SimState(args.simulator, runner)
    scratch = tempfile.mkdtemp(prefix="rios-perf-seed-")
    try:
        report, files = app_state_files(rows, scratch, runner, rios)
        cond = condition.for_app_state(rows, configuration, SEEDED_BY, report["written"], report["fixtureFiles"])
        marker = condition.file_fixture_marker(rows)
        cond["verified"] = {"row": marker, "onScreen": None, "why": "not checked yet"}
        backup = state.read()
        state.put(files)
        record["condition"] = cond
        record["conditions"] = {"fixture": condition.FILE_FIXTURE, "history": rows, "networkCondition": "mac-unreachable"}
        record["route"] = {"name": "seeded fixture",
                           "detail": "the seeded pairing names a host under .invalid and no device id: the app opens no "
                                     "connection, so no Mac is reachable",
                           "persistence": "production (the app's own Application Support/RichOS files)"}
        record["phases"]["seed"] = "measured"
        keep = None
        if getattr(args, "evidence_dir", None) and evidence_root_ok(args.evidence_dir):
            os.makedirs(args.evidence_dir, exist_ok=True)
            keep = tempfile.mkdtemp(prefix="ios-screen-", dir=args.evidence_dir)
        try:
            return _measure_simulator(args, record, runner, popen, sleep,
                                      check=lambda: screen_check(args.simulator, marker, keep, runner, sleep, rios))
        finally:
            try:
                state.put(backup)
                record["conditions"]["savedStateRestored"] = True
            except (Unmeasurable, OSError) as e:
                record["conditions"]["savedStateRestored"] = False
                record["notMeasured"].append({"what": "the app's own saved state after the run",
                                              "why": f"not put back: {e}"})
    except condition.ConditionError as e:
        raise Refused(str(e))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def _record_verified(record, verified):
    """The on-screen check's result into the condition, and into every retained series (which reparses
    with its check). Returns 1 when the seeded row was not seen, else 0."""
    record["condition"]["verified"] = verified
    for evidence in (record.get("evidence") or {}).values():
        path = os.path.join(evidence, "series.json")
        if os.path.isfile(path):
            with open(path) as f:
                series = json.load(f)
            series["condition"] = record["condition"]
            with open(path, "w") as f:
                json.dump(series, f, indent=2)
    if verified.get("onScreen") is True:
        return 0
    record["notMeasured"].append({"what": "the seeded condition",
                                  "why": "the seeded row was not seen on screen after the launches "
                                         f"({verified.get('why') or 'the screen showed something else'}); "
                                         "the record is never compared"})
    return 1


def _measure_simulator(args, record, runner, popen, sleep, check=None):
    """The launch series, then `check` (the seeded conversation on screen) while the app is still
    the one the series launched, then the background window."""
    failures = 0
    xctrace = getattr(args, "xctrace", False)
    if xctrace:
        failures += _trace_classes(record, Simctl(args.simulator, runner), args, runner, popen, sleep)
    else:
        sim = Sim(args.simulator, runner)
        samples, rejected = sim.cold(args.cold)
        if samples:
            record["metrics"]["coldLaunch"] = {"method": "simctl terminate; host clock before simctl launch to the app's "
                                               "'useful-content' signpost in the simulator log (one clock: the Mac's). "
                                               "Includes simctl's own launch overhead",
                                               "samplesMs": samples, "stats": perfcore.stats(samples),
                                               "budget": perfcore.compare("coldLaunch", perfcore.stats(samples)), "rejected": rejected}
        else:
            failures += 1
            record["notMeasured"].append({"what": "coldLaunch", "why": "; ".join(r["why"] for r in rejected[:1]) or "no trial"})
    if check:
        failures += _record_verified(record, check())
    if not xctrace:
        try:
            record["metrics"]["backgroundQuiet"] = {"method": "the simulated app is a Mac process: top idle wakeups, "
                                                    "context switches and CPU time, nettop bytes, differenced over the window",
                                                    **sim.background(args.background_seconds, args.background_settle)}
        except Unmeasurable as e:
            failures += 1
            record["notMeasured"].append({"what": "backgroundQuiet", "why": str(e)})
    return failures


def ios_acceptance(record):
    """PRD §8 needs both launch classes: the smaller class's trial count is the protocol count."""
    metrics = record["metrics"]
    counts = [len((metrics.get(k) or {}).get("samplesMs") or []) for k in ("coldLaunch", "warmResume")]
    out = perfcore.acceptance(record["device"]["kind"], record["build"].get("configuration") == "release", min(counts))
    if record["build"].get("configuration") is None:
        out["why"] = [w if not w.startswith("not a release") else
                      "the build configuration is unknown (no stamped bundle to inspect)" for w in out["why"]]
    for key, cls in (("coldLaunch", "cold launch"), ("warmResume", "warm return")):
        if key not in metrics:
            out["why"].append(f"no {cls} distribution")
    out["verdict"] = "NOT VERIFIED" if out["why"] else out["verdict"]
    return out


def ios_gaps(metrics=None):
    metrics = metrics or {}
    gaps = []
    if "coldLaunch" not in metrics:
        gaps.append({"what": "coldLaunch", "why": "run the trace series (physical --device, or --xctrace on a simulator "
                                                  "as a dry run) with --cold N"})
    if "warmResume" not in metrics:
        gaps.append({"what": "warmResume", "why": "run the trace series with --warm N"})
    return gaps + [
        {"what": "tapToFeedback, sendToQueued, receiveToVisible, activeFrames",
         "why": "need an XCUITest harness in native-ios/UITests driving the real controls with XCTOSSignpostMetric "
                "around app-emitted signposts; not written"},
        {"what": "idleFrames", "why": "iOS has no gfxinfo; on an iPhone, Instruments' Animation Hitches template "
                                      "(xctrace) over an untouched window; not parsed here"},
        {"what": "background on an iPhone and energy",
         "why": "Instruments' Activity Monitor / Power Profiler templates (xctrace record --attach) and Settings > "
                "Battery over the PRD §8 windows; not parsed here"},
    ]

