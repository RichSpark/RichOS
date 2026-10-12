"""android — RichConnect measurement on an Android emulator or phone, over adb.

Every number comes from the platform's own instruments, read back over adb; nothing here asks the
app to time itself except the one launch marker the app emits for the platform (`ReportDrawnWhen`
in MainActivity, which Android logs as "Fully drawn"):

  cold launch    `am start -W` after `am force-stop` (LaunchState must read COLD). First frame is
                 the platform's TotalTime ("Displayed"); USEFUL CONTENT is the platform's
                 "Fully drawn" time, reported by the app on the first frame drawn after the saved
                 state is read (the transcript at its newest row and the composer).
  warm resume    HOME, then the launcher intent with `am start -W`: TotalTime of a HOT start with
                 the SAME process (a new pid is a cold start and is never counted as warm).
  tap feedback   `input tap` on "Send message": the app's own `deliverInputEvent ... eventTimeNano`
                 trace slice (atrace, CLOCK_MONOTONIC) to the FrameCompleted of the frame whose
                 InputEventId is that event (gfxinfo framestats, CLOCK_MONOTONIC): one clock.
  idle frames    `dumpsys gfxinfo <pkg> reset`, N seconds untouched, "Total frames rendered" for the
                 activity's window.
  background     HOME, a settle period, then over N seconds: context switches of every app thread
                 (/proc/<pid>/task/*/status), CPU ticks (/proc/<pid>/stat), and batterystats reset
                 at the window's start and read at its end (--checkin: wakeup alarms, wake locks,
                 jobs, syncs, network bytes, CPU), with pending alarms, jobs, held wake locks and the
                 app's open sockets at the end.
  typing cost    K keystrokes through the real field (`input text`, one character each): the app's
                 write syscalls and bytes (/proc/<pid>/io, root) and fsync calls (ftrace
                 ext4/f2fs_sync_file_enter in a private tracefs instance, root) per keystroke, and
                 the frames drawn while typing.
  stream cost    K reply deltas fed through the debug bridge's `receive`: the same write, fsync and
                 frame counts per delta.

Device addressing: an emulator only through `randroid emu perf` (its recorded serial, `--owned-by
randroid`); a physical phone only by a serial the caller names with `--kind physical`, checked to be
a real device. Never a bare `adb`.
"""
import base64
import csv
import gzip
import io
import json
import re
import subprocess
import time
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import machine_storage as M
import xml.etree.ElementTree as ET

import perfcore
import phone_changes
from perfcore import Refused, Unmeasurable

BATTERY_UNDO = ["dumpsys battery reset"]
ATRACE_UNDO = ["atrace --async_stop > /dev/null 2>&1"]
PACKAGE = "dev.richos.connect"
ACTIVITY = f"{PACKAGE}/dev.richos.android.app.MainActivity"
RECEIVER = f"{PACKAGE}/dev.richos.android.app.debug.DevBridgeReceiver"


def use_package(package):
    """Point every measurement in this module at `package`. A PHYSICAL phone is only ever measured through the TEST
    COPY (test_copy.py, dev.richos.connect.perf), installed beside the CEO's own RichConnect; perf.py run_android
    calls this for it. The class names stay the app's own (the Kotlin namespace is not the application ID)."""
    global PACKAGE, ACTIVITY, RECEIVER, MANIFEST
    PACKAGE = package
    ACTIVITY = f"{PACKAGE}/dev.richos.android.app.MainActivity"
    RECEIVER = f"{PACKAGE}/dev.richos.android.app.debug.DevBridgeReceiver"
    MANIFEST = f"run-as {PACKAGE} sh -c 'find . -type f -exec sha256sum {{}} +'"
TRACE_FILE = "/data/local/tmp/richos-perf-trace.txt"
TRACEFS = "/sys/kernel/tracing"
FSYNC_INSTANCE = f"{TRACEFS}/instances/richos-perf-fsync"
FSYNC_EVENTS = ("ext4/ext4_sync_file_enter", "f2fs/f2fs_sync_file_enter")
# Seeded history: synthetic, contains nothing about any person (PRD §8: synthetic conversations).
CHALLENGE = "X4zZvQZS4kl8eriGLhoxvxVwcFz5Tx40"  # Fixtures.CHALLENGE, the development Mac's


# ---------------------------------------------------------------------------------------------
# Parsers: pure functions of text the device printed. Each raises Unmeasurable with a sentence.
# ---------------------------------------------------------------------------------------------

def parse_am_start(text):
    """`am start -W` → {"status", "launchState", "totalMs", "waitMs"}."""
    fields = dict(re.findall(r"^(\w+): (.*?)\s*$", text, re.M))
    if fields.get("Status") != "ok" or "TotalTime" not in fields:
        raise Unmeasurable("am start -W did not report a completed launch: " + " ".join(text.split())[:200])
    return {"status": "ok", "launchState": fields.get("LaunchState"), "totalMs": int(fields["TotalTime"]),
            "waitMs": int(fields["WaitTime"]) if "WaitTime" in fields else None}


_DURATION = re.compile(r"^\+(?:(\d+)m)?(?:(\d+)s)?(?:(\d+)ms)?$")


def parse_plus_duration(token):
    """ActivityTaskManager's "+1s863ms" / "+863ms" / "+1m2s3ms" → milliseconds."""
    m = _DURATION.match(token.strip())
    if not m or not any(m.groups()):
        raise Unmeasurable(f"unreadable launch duration {token!r}")
    minutes, seconds, ms = (int(g) if g else 0 for g in m.groups())
    return minutes * 60000 + seconds * 1000 + ms


def parse_fully_drawn(logcat, component=None):
    """The last "Fully drawn <component>: +Ns" in a logcat read → milliseconds, or None."""
    component = component or ACTIVITY  # read now: use_package() moves it, a default bound at import would not follow
    hits = re.findall(r"Fully drawn " + re.escape(component) + r"(?: for user \d+)?: (\+[0-9ms]+)", logcat)
    return parse_plus_duration(hits[-1]) if hits else None


def parse_framestats(text):
    """`dumpsys gfxinfo <pkg> framestats` → {window: {"totalFrames": n, "rows": [ {column: int} ]}}.

    Only the per-window sections ("Window: ..."); the process-wide summary above them repeats the
    same frames and is skipped."""
    windows = {}
    current = None
    header = None
    in_profile = False
    for line in text.splitlines():
        line = line.rstrip()
        if line.startswith("Window: "):
            current = line[len("Window: "):].strip()
            windows[current] = {"totalFrames": None, "rows": []}
            in_profile = False
            continue
        if current is None:
            continue
        m = re.match(r"^Total frames rendered: (\d+)", line)
        if m and windows[current]["totalFrames"] is None:
            windows[current]["totalFrames"] = int(m.group(1))
            continue
        if line == "---PROFILEDATA---":
            in_profile = not in_profile
            header = None
            continue
        if in_profile:
            cells = [c for c in line.split(",") if c != ""]
            if header is None:
                header = cells
                continue
            try:
                windows[current]["rows"].append(dict(zip(header, (int(c) for c in cells))))
            except ValueError:
                continue
    return windows


def app_window(windows, component=None):
    component = component or ACTIVITY  # read now, so use_package() (the test copy) is followed
    for name, window in windows.items():
        if name.startswith(component):
            return window
    raise Unmeasurable(f"gfxinfo has no window for {component} (windows: {sorted(windows)})")


def signed32(value):
    value &= 0xFFFFFFFF
    return value - (1 << 32) if value >= 1 << 31 else value


def parse_input_events(trace, pid):
    """The app's `deliverInputEvent src=... eventTimeNano=T id=0x...` slices (Android 12+)."""
    events = []
    pattern = re.compile(r"B\|" + str(pid) + r"\|deliverInputEvent src=0x([0-9a-f]+) eventTimeNano=(\d+) id=0x([0-9a-f]+)")
    for m in pattern.finditer(trace):
        events.append({"source": int(m.group(1), 16), "eventTimeNs": int(m.group(2)), "id": signed32(int(m.group(3), 16))})
    return events


# A start the phone's own report could not time (an OEM that leaves a frame out of framestats or never fills in its
# present time) is rejected and one more start is run in its place, up to this many in a series; every judged
# start is still timed, and a series that still falls short of its timed starts gets no verdict (§104).
UNTIMED_RETRIES = 10


def merge_frame_rows(*reads):
    """Framestats rows from several reads of one launch, each frame once (a later read of the same
    frame wins: its present time may have been filled in), in draw order."""
    merged = {}
    for rows in reads:
        for row in rows:
            merged[(row.get("IntendedVsync"), row.get("DrawStart"))] = row
    return sorted(merged.values(), key=lambda r: r.get("DrawStart") or 0)


def useful_launch_frame(trace, rows, pid):
    """Join the system launch interval to the app's useful draw and its presented frame.

    Never subtract ftrace timestamps from FrameMetrics: OEMs can use different clock origins.
    The app emits both clock coordinates at the draw; only durations cross that join.
    """
    events = []
    for line in trace.splitlines():
        match = re.search(r"-(\d+)\s+\([^)]*\).*? ([0-9]+\.[0-9]+): tracing_mark_write: (.*)$", line)
        if match:
            seconds, fraction = match[2].split(".")
            events.append((int(seconds) * 1_000_000_000 + int(fraction.ljust(9, "0")), match[3], int(match[1])))
    completed = [(at, payload) for at, payload, _ in events if re.search(r"\|launchingActivity#\d+:completed-(?:cold|warm|hot):" + re.escape(PACKAGE) + r"$", payload)]
    if not completed:
        raise Unmeasurable("no system launch interval for RichConnect in this OEM trace")
    _, completion = completed[-1]
    owner, name = completion.split("|")[1:3]
    name = name.split(":")[0]
    starts = [at for at, payload, _ in events if payload.startswith(f"S|{owner}|{name}|")]
    if len(starts) != 1:
        raise Unmeasurable("missing or ambiguous system launch start")
    start = starts[0]
    draw = None
    for index, (at, payload, tid) in enumerate(events):
        if at >= start and payload == f"B|{pid}|richconnect:foreground-useful":
            for counter_at, counter, counter_tid in events[index + 1:]:
                if counter_tid != tid: continue
                if counter == f"E|{pid}": break
                prefix = f"C|{pid}|richconnect:monotonic-ns|"
                if counter.startswith(prefix):
                    draw = (counter_at, int(counter[len(prefix):])); break
            break
    if draw is None:
        raise Unmeasurable("useful draw has no monotonic clock counter")
    trace_at, monotonic = draw
    # WindowVisibilityChanged (1) is expected on resume. It has valid draw/presentation
    # coordinates even though it is excluded from steady-state frame-deadline statistics.
    frames = [r for r in rows if r.get("Flags") in (0, 1) and r.get("FrameCompleted", 0) > 0
              and 0 < r.get("DrawStart", 0) <= monotonic <= r.get("SyncQueued", 0)]
    if len(frames) != 1:
        raise Unmeasurable("useful draw does not identify exactly one frame")
    frame = frames[0]
    present = frame.get("DisplayPresentTime", 0)
    if not monotonic <= present <= monotonic + 2_000_000_000:
        raise Unmeasurable("OEM did not report a usable presentation timestamp")
    return {"usefulMs": round((trace_at - start + present - monotonic) / 1_000_000, 2),
            "launchStartTraceNs": start, "drawTraceNs": trace_at, "drawMonotonicNs": monotonic,
            "presentedMonotonicNs": present, "frameTimelineId": frame.get("FrameTimelineVsyncId")}


def frame_ok(row):
    """A framestats row that timed a real frame (Flags 0; a nonzero Flags row is not a timed frame)."""
    return row.get("Flags", 0) == 0 and row.get("FrameCompleted", 0) > 0


def tap_latency(events, rows):
    """Input event time → FrameCompleted of the frame that consumed it; and → the last frame of the
    burst it started (the response settling). Both on CLOCK_MONOTONIC."""
    if not events:
        raise Unmeasurable("the trace holds no deliverInputEvent slice from the app (atrace 'input' category, Android 12+)")
    by_id = {r.get("InputEventId"): r for r in rows if frame_ok(r)}
    consumed = [(e, by_id[e["id"]]) for e in events if e["id"] in by_id]
    if not consumed:
        raise Unmeasurable("no framestats frame carries the tap's InputEventId")
    event, frame = consumed[-1]  # the last event of the tap (UP): the one that sends
    start = event["eventTimeNs"]
    after = sorted((r for r in rows if frame_ok(r) and r["FrameCompleted"] >= frame["FrameCompleted"]), key=lambda r: r["FrameCompleted"])
    settled = after[0]["FrameCompleted"]
    for r in after[1:]:
        if r["IntendedVsync"] - settled > 100_000_000:  # a gap of more than 100 ms ends the burst
            break
        settled = r["FrameCompleted"]
    return {"inputToFrameMs": round((frame["FrameCompleted"] - start) / 1e6, 1),
            "inputToSettledMs": round((settled - start) / 1e6, 1),
            "burstFrames": sum(1 for r in after if r["FrameCompleted"] <= settled),
            "eventId": event["id"]}


def frame_timing(rows):
    """Frames drawn and how many met their deadline (FrameCompleted <= FrameDeadline)."""
    timed = [r for r in rows if frame_ok(r)]
    if not timed:
        return {"frames": 0, "skippedRows": len(rows)}
    durations = [(r["FrameCompleted"] - r["IntendedVsync"]) / 1e6 for r in timed]
    on_time = [r for r in timed if r.get("FrameDeadline") and r["FrameCompleted"] <= r["FrameDeadline"]]
    return {"frames": len(timed), "skippedRows": len(rows) - len(timed),
            "onTimePercent": round(100.0 * len(on_time) / len(timed), 1),
            "maxFrameMs": round(max(durations), 1),
            "stallsOver100Ms": sum(1 for d in durations if d >= 100)}


def parse_threads(text):
    """Lines `tid|comm|voluntary nonvoluntary` → {tid: {"comm", "switches"}}."""
    threads = {}
    for line in text.splitlines():
        parts = line.strip().split("|")
        if len(parts) != 3 or not parts[0].isdigit():
            continue
        nums = [int(n) for n in re.findall(r"\d+", parts[2])]
        if len(nums) != 2:
            continue
        threads[parts[0]] = {"comm": parts[1], "switches": nums[0] + nums[1]}
    return threads


def thread_wakeups(before, after):
    """Context switches per thread between two samples: the app's wakeups, attributed by thread."""
    rows = []
    for tid, now in after.items():
        was = before.get(tid)
        delta = now["switches"] - (was["switches"] if was else 0)
        if delta:
            rows.append({"tid": int(tid), "thread": now["comm"], "switches": delta, "new": was is None})
    ended = sorted(int(t) for t in before if t not in after)
    rows.sort(key=lambda r: -r["switches"])
    return {"total": sum(r["switches"] for r in rows), "threads": rows, "threadsEnded": ended}


def parse_proc_stat_ticks(text):
    """/proc/<pid>/stat → utime + stime, in clock ticks (the comm field may contain spaces)."""
    rest = text[text.rfind(")") + 2:].split()
    if len(rest) < 13:
        raise Unmeasurable("unreadable /proc/<pid>/stat")
    return int(rest[11]) + int(rest[12])


def parse_proc_io(text):
    out = dict((k, int(v)) for k, v in re.findall(r"^(\w+):\s*(\d+)", text, re.M))
    if "wchar" not in out or "syscw" not in out:
        raise Unmeasurable("unreadable /proc/<pid>/io")
    return out


def io_delta(before, after):
    return {k: after[k] - before[k] for k in ("wchar", "syscw", "write_bytes", "rchar", "syscr") if k in before and k in after}


def count_fsyncs(trace, tgid):
    """Lines of the fsync instance's trace whose thread-group is the app's."""
    count = 0
    for line in trace.splitlines():
        if "_sync_file_enter" not in line:
            continue
        m = re.search(r"\(\s*(\d+)\)\s*\[", line)
        if m and int(m.group(1)) == tgid:
            count += 1
    return count


CHECKIN_KINDS = {"cpu": "cpu", "nt": "network", "wua": "wakeupAlarms", "wl": "wakelocks", "jb": "jobs",
                 "sy": "syncs", "awl": "aggregatedWakelock", "pr": "processes"}


def parse_checkin(text, uid):
    """`dumpsys batterystats --checkin` rows for one uid, since the last reset (section `l`)."""
    out = {"cpuMs": None, "networkBytes": None, "wakeupAlarms": [], "wakelocks": [], "jobs": [], "syncs": [],
           "partialWakelockMs": None, "processes": []}
    seen = False
    for row in csv.reader(io.StringIO(text)):
        if len(row) < 4 or row[0] != "9" or row[1] != str(uid) or row[2] != "l":
            continue
        seen = True
        kind, rest = row[3], row[4:]
        if kind == "cpu" and len(rest) >= 2:
            out["cpuMs"] = {"user": int(rest[0]), "system": int(rest[1])}
        elif kind == "nt" and len(rest) >= 4:
            nums = [int(x) for x in rest[:8]]
            out["networkBytes"] = {"mobileRx": nums[0], "mobileTx": nums[1], "wifiRx": nums[2], "wifiTx": nums[3],
                                   "total": sum(nums[:4])}
        elif kind == "wua" and len(rest) >= 2:
            out["wakeupAlarms"].append({"name": rest[0], "count": int(rest[1])})
        elif kind == "wl" and len(rest) >= 1:
            out["wakelocks"].append({"name": rest[0], "fields": rest[1:]})
        elif kind == "jb" and len(rest) >= 3:
            out["jobs"].append({"name": rest[0], "ms": int(rest[1]), "count": int(rest[2])})
        elif kind == "sy" and len(rest) >= 3:
            out["syncs"].append({"name": rest[0], "ms": int(rest[1]), "count": int(rest[2])})
        elif kind == "awl" and len(rest) >= 1:
            out["partialWakelockMs"] = int(rest[0])
        elif kind == "pr" and len(rest) >= 6:
            out["processes"].append({"name": rest[0], "userMs": int(rest[1]), "systemMs": int(rest[2]),
                                     "starts": int(rest[3]), "anrs": int(rest[4]), "crashes": int(rest[5])})
    if out["networkBytes"] is None and seen:
        out["networkBytes"] = {"total": 0, "note": "no network row for the uid since the reset"}
    out["uidSeen"] = seen
    return out


def parse_sockets(text, uid):
    """/proc/net/{tcp,tcp6,udp,udp6} rows owned by uid → counts by protocol and TCP state."""
    states = {"01": "ESTABLISHED", "02": "SYN_SENT", "06": "TIME_WAIT", "08": "CLOSE_WAIT", "0A": "LISTEN"}
    counts = {}
    for line in text.splitlines():
        cols = line.split()
        if len(cols) < 8 or not cols[0].endswith(":") or cols[7] != str(uid):
            continue
        state = states.get(cols[3], cols[3])
        counts[state] = counts.get(state, 0) + 1
    return counts


def ui_nodes(xml_text):
    """uiautomator dump → [{"text", "desc", "bounds": (x1,y1,x2,y2), "clickable", "focused"}]."""
    try:
        root = ET.fromstring(xml_text)
    except ET.ParseError as e:
        raise Unmeasurable(f"unreadable UI dump: {e}")
    nodes = []
    for n in root.iter("node"):
        m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", n.get("bounds") or "")
        nodes.append({"text": n.get("text") or "", "desc": n.get("content-desc") or "", "class": n.get("class") or "",
                      "bounds": tuple(int(g) for g in m.groups()) if m else None,
                      "clickable": n.get("clickable") == "true", "focused": n.get("focused") == "true"})
    return nodes


def find_node(nodes, desc=None, text=None, contains=None):
    for n in nodes:
        if desc is not None and n["desc"] == desc:
            return n
        if text is not None and n["text"] == text:
            return n
        if contains is not None and contains in n["text"]:
            return n
    return None


def composer_node(nodes):
    label = find_node(nodes, desc="Message Rich")
    if label is None:
        return None
    # Accessibility exposes editable text and Compose's label as separate nodes.
    matches = [node for node in nodes if node.get("class") == "android.widget.EditText" and node.get("bounds") == label.get("bounds")]
    return matches[0] if len(matches) == 1 else label


def center(node):
    x1, y1, x2, y2 = node["bounds"]
    return (x1 + x2) // 2, (y1 + y2) // 2


def history_frame(count, thread="general"):
    """A `hello` frame carrying `count` synthetic messages (alternating CEO and Rich)."""
    rows = []
    for i in range(1, count + 1):
        role = "ceo" if i % 2 else "rich"
        rows.append({"id": f"perf-{i}", "thread_id": thread, "cursor": i, "role": role, "kind": "text",
                     "text": f"Synthetic message {i} for the launch measurement: three open items, one short summary.",
                     "created_at": "2023-11-14T22:13:20.000Z", "client_id": None, "has_audio": False,
                     "from_microphone": False, "state": "sent" if role == "ceo" else "complete", "complete": True})
    data = {"challenge": CHALLENGE, "thread_id": thread, "latest_cursor": count,
            "threads": [{"id": thread, "title": "General"}], "capabilities": ["text", "voice"], "build": "perf",
            "messages": rows}
    return {"type": "receive", "wire": "id: 1\nevent: hello\ndata: " + json.dumps(data) + "\n\n"}


def stream_frames(cursor, deltas, thread="general", message_id="perf-stream"):
    """A streamed reply: the opening row, `deltas` text deltas, the final row. Synthetic text."""
    def row(text, state):
        return {"id": message_id, "thread_id": thread, "cursor": cursor, "role": "rich", "kind": "text", "text": text,
                "created_at": "2023-11-14T22:13:20.000Z", "client_id": None, "has_audio": False,
                "from_microphone": False, "state": state, "complete": state != "streaming"}

    def frame(event, data):
        return {"type": "receive", "wire": f"id: {cursor}\nevent: {event}\ndata: {json.dumps(data)}\n\n"}
    words = [f"word{i} " for i in range(1, deltas + 1)]
    opening = frame("message", row("", "streaming"))
    middle = [frame("delta", {"message_id": message_id, "cursor": cursor, "text": w}) for w in words]
    final = frame("message", row("".join(words), "complete"))
    return opening, middle, final, "".join(words)


# ---------------------------------------------------------------------------------------------
# The device
# ---------------------------------------------------------------------------------------------

class Device:
    """One adb device, addressed only by the serial it was given. `runner` is replaceable (tests)."""

    def __init__(self, adb, serial, runner=subprocess.run, sleep=time.sleep, touch=None, clock=time.monotonic):
        if not serial:
            raise Refused("no serial: a device is addressed only by an explicit serial, never a bare adb")
        self.adb, self.serial, self.runner, self.sleep = adb, serial, runner, sleep
        self.touch, self.clock, self.touched = touch, clock, None

    def run(self, *args, timeout=120, check=True):
        if self.touch and (self.touched is None or self.clock() - self.touched > 30):
            self.touch()
            self.touched = self.clock()
        try:
            p = self.runner([self.adb, "-s", self.serial, *args], capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            raise Unmeasurable(f"adb {' '.join(args)[:120]} did not answer in {timeout} s")
        if check and p.returncode != 0:
            raise Unmeasurable(f"adb {' '.join(args)[:120]} exited {p.returncode}: {(p.stderr or p.stdout).strip()[:200]}")
        return p.stdout

    def run_bytes(self, *args, timeout=600):
        """adb's stdout as raw bytes (a tar streamed back from run-as); always checked."""
        if self.touch and (self.touched is None or self.clock() - self.touched > 30):
            self.touch()
            self.touched = self.clock()
        try:
            p = self.runner([self.adb, "-s", self.serial, *args], capture_output=True, text=False, timeout=timeout)
        except subprocess.TimeoutExpired:
            raise Unmeasurable(f"adb {' '.join(args)[:120]} did not answer in {timeout} s")
        if p.returncode != 0:
            err = p.stderr.decode(errors="replace") if isinstance(p.stderr, bytes) else (p.stderr or "")
            raise Unmeasurable(f"adb {' '.join(args)[:120]} exited {p.returncode}: {err.strip()[:200]}")
        return p.stdout

    def sh(self, command, **kw):
        return self.run("shell", command, **kw)

    def prop(self, name):
        return self.sh(f"getprop {name}").strip()

    def pid(self):
        out = self.sh(f"pidof {PACKAGE}", check=False).strip()
        return int(out.split()[0]) if out else None

    def present(self):
        """Is the device still there (an emulator the Mac's CPU breaker stopped is not)?"""
        try:
            return self.run("get-state", timeout=10, check=False).strip() == "device"
        except Unmeasurable:
            return False

    def begin_change(self, kind, undo):
        """Record on the Mac what is about to change on the phone, so a killed run is still undone (phone_changes.py)."""
        phone_changes.begin(self.serial, kind, undo)

    def end_change(self, kind, undo):
        """Undo it now; the record is erased only when the undo reached the phone."""
        try:
            for c in undo:
                self.sh(c)
        except Unmeasurable:
            return False
        phone_changes.forget(self.serial, kind)
        return True

    def restore_leftovers(self, log=lambda s: None):
        """First thing in every run: undo what a previous run recorded and never undid."""
        def one(c):
            self.sh(c)
            return True
        return phone_changes.restore(self.serial, one, log)

    def uptime_epoch(self):
        return self.sh("date +%s.%N").strip()


class Bridge:
    """The debug build's development bridge (randroid's `emu <command>`, the same broadcast)."""

    def __init__(self, device):
        self.device = device

    def call(self, command, arg=None):
        extra = ""
        if arg is not None:
            extra = " --es arg64 " + base64.b64encode(arg.encode()).decode()
        out = self.device.sh(f"am broadcast -n {RECEIVER} --es command {command}{extra}", check=False)
        m = re.search(r'data="([^"]*)"', out)
        if not m:
            raise Unmeasurable(f"the debug bridge did not answer {command} (a release build has none): {' '.join(out.split())[:160]}")
        body = json.loads(base64.b64decode(m.group(1)))
        if not body.get("ok"):
            raise Unmeasurable(f"the debug bridge refused {command}: {body.get('error')}")
        return body.get("result")

    def action(self, obj):
        return self.call("action", json.dumps(obj))

    def state(self):
        return self.call("state")["state"]


# ---------------------------------------------------------------------------------------------
# Seeding the app's saved state (the measured condition, condition.py)
# ---------------------------------------------------------------------------------------------

CORE_DIR = "files/core"  # LocalSessionStore: session.json and history.json


def debuggable(dev):
    flags = re.search(r"pkgFlags=\[([^\]]*)\]", dev.sh(f"dumpsys package {PACKAGE}", check=False))
    return bool(flags and "DEBUGGABLE" in flags.group(1))


def write_core_files(dev, files, scratch):
    """Write `files` ({name: bytes}) into the installed DEBUGGABLE build's files/core with run-as,
    the app stopped, and read each back by sha256. Returns {name: sha256}. This OVERWRITES the app's
    files/core: the caller keeps a StateKeeper (below) saved first and restores it afterwards."""
    import hashlib
    import os
    if not debuggable(dev):
        raise Refused("run-as needs a debuggable build installed; this one is not (a release build is seeded "
                      "through its debuggable twin, --seed-twin)")
    dev.sh(f"am force-stop {PACKAGE}")
    dev.sh(f"run-as {PACKAGE} mkdir -p {CORE_DIR}")
    written = {}
    for name, data in sorted(files.items()):
        local = os.path.join(scratch, name)
        with open(local, "wb") as f:
            f.write(data)
        remote = f"/data/local/tmp/richos-perf-{name}"
        dev.run("push", local, remote)
        dev.sh(f"run-as {PACKAGE} cp {remote} {CORE_DIR}/{name}")
        dev.sh(f"rm -f {remote}", check=False)
        want = hashlib.sha256(data).hexdigest()
        got = (dev.sh(f"run-as {PACKAGE} sha256sum {CORE_DIR}/{name}", check=False).split() or [""])[0]
        if got != want:
            raise Unmeasurable(f"seeding {name}: the app's copy reads {got[:12] or 'nothing'}…, the fixture is {want[:12]}…")
        written[name] = want
    return written


def signer_sha256(apk, apksigner):
    """The SHA-256 of the certificate that signed `apk`, from apksigner's own reading; Unmeasurable when
    it cannot be read (no apksigner, an unsigned APK, more than one signer)."""
    import os
    import subprocess
    if not apksigner or not os.path.isfile(apksigner):
        raise Unmeasurable(f"no apksigner at {apksigner!r}: who signed {apk} cannot be read, so the twin is not installed")
    p = subprocess.run([apksigner, "verify", "--print-certs", apk], capture_output=True, text=True, timeout=120)
    digests = sorted(set(re.findall(r"certificate SHA-256 digest: ([0-9a-fA-F]{64})", p.stdout)))
    if p.returncode != 0 or len(digests) != 1:
        raise Unmeasurable(f"{apk} does not verify with exactly one signer (apksigner exit {p.returncode}, "
                           f"{len(digests)} certificates): {(p.stderr or p.stdout).strip()[-200:]}")
    return digests[0].lower()


def require_same_signer(installed_apk, twin_apk, apksigner):
    """Before the twin goes anywhere near the phone: the installed app's signer and the twin's must be
    the same certificate, or `install -r` is refused and the only way forward is an uninstall (the app's
    data wiped, the CEO's decision). Plain sentence, nothing touched."""
    have, want = signer_sha256(installed_apk, apksigner), signer_sha256(twin_apk, apksigner)
    if have != want:
        raise Refused(f"the debuggable twin is signed with a different key than the RichConnect on the phone (the phone's "
                      f"app: {have[:12]}…, the twin: {want[:12]}…), so Android would refuse to install it over the app. "
                      "Nothing was touched and nothing was uninstalled: replacing a differently signed build means an "
                      "uninstall, which wipes the app's data, and that is the CEO's decision. Build the twin through "
                      "richos-hq scripts/with-android-signing.py so it carries the upload key (`randroid build twin`)")
    return have


def install_over(dev, apk, what):
    """`adb install -r`: replaces the installed build and KEEPS its data. Never an uninstall. When the
    phone refuses (a signature that differs from the installed build's, a lower version code) it is
    a refusal with the phone's own words: the installed app is untouched, because Android replaces
    a package atomically or not at all."""
    try:
        dev.run("install", "-r", apk)
    except Unmeasurable as e:
        raise Refused(f"installing {what} over the installed app was refused ({e}); the app was NOT uninstalled and its "
                      "data is untouched. The debuggable twin must be signed with the same key as the installed build "
                      "and not be a lower version") from e


def seed_release(dev, files, twin_apk, release_apk, release_sha256, scratch, log=lambda s: None):
    """Seed a release build without any uninstall: `adb install -r` the release build's debuggable twin
    over the installed app (its data kept; the twin must be signed with the same key), write the
    fixture into its files/core with run-as (the twin is never launched), then `install -r` the stamped
    release APK back over it, data kept. If the phone refuses either install (another signature, a
    lower version) this raises Refused and reports; it never uninstalls and never clears data.
    The app's own data is overwritten: the caller saves it first with a StateKeeper(twin_apk=...)
    and puts it back after the run (perf.run_android does both). Refuses before touching the device
    when an APK is missing or the release APK is not the stamped bytes."""
    import hashlib
    import os
    for label, path in (("the debuggable twin", twin_apk), ("the stamped release APK", release_apk)):
        if not path or not os.path.isfile(path):
            raise Refused(f"{label} is not at {path!r}; seeding a release build needs both APKs")
    with open(release_apk, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    if digest != release_sha256:
        raise Refused(f"freshness mismatch: {release_apk} is {digest[:12]}…, the stamped build is {str(release_sha256)[:12]}…")
    with open(twin_apk, "rb") as f:
        twin_sha = hashlib.sha256(f.read()).hexdigest()
    log("seed: install -r the debuggable twin over the app, write the fixture, install -r the release build back (no uninstall)")
    install_over(dev, twin_apk, "the debuggable twin")
    try:
        written = write_core_files(dev, files, scratch)
    except BaseException:
        # The twin never stays on the phone: the release build goes back over it (data kept) whatever
        # went wrong; the original failure is the one that is raised.
        try:
            install_over(dev, release_apk, "the stamped release APK (after a failed seeding)")
        except Exception as e:  # noqa: BLE001
            log(f"seed: the release build could not be put back over the twin ({e}); the keeper puts the saved build back")
        raise
    install_over(dev, release_apk, "the stamped release APK")
    if debuggable(dev):
        raise Refused("after installing the release APK over the twin the app is still debuggable; not the release build")
    return {"files": written, "twinSha256": twin_sha}


KEEP_ROOT = str(M.agent_root() / "perf-keep")  # private, outside every repository
KEEP_TAR = "/data/local/tmp/richos-perf-keep.tar"
MANIFEST = f"run-as {PACKAGE} sh -c 'find . -type f -exec sha256sum {{}} +'"


class StateKeeper:
    """Gives the phone back as it found it: the installed build and the app's private data. It never
    uninstalls the app and never clears its data; every install is `adb install -r`, which keeps data.

    save() copies the installed APK (adb pull) and the app's whole private data directory (a tar
    streamed back through run-as) to a private directory under KEEP_ROOT, and records a sha256
    manifest of both. A release build has no run-as, so with `twin_apk` (its debuggable twin, signed
    with the same key) the twin is installed over it for the copy; without one the data cannot be
    saved and save() REFUSES, before anything on the device changes, unless `accept_loss` names who
    agreed to lose that saved state.

    restore() puts the data back in place (run-as: the files the run added are removed, the saved tar
    is extracted over the rest), reads the data hashes back, then installs the saved APK over whatever
    is installed and reads its hash back. It returns the problems it found, [] when everything
    matches; the private copy is deleted only then. Nothing installed means nothing to put back."""

    def __init__(self, dev, root=None, log=lambda s: None, accept_loss=None, twin_apk=None, clock=time.time_ns,
                 apksigner=None):
        self.dev, self.log, self.accept_loss, self.twin_apk = dev, log, accept_loss, twin_apk
        self.apksigner = apksigner
        self.root = root or KEEP_ROOT
        self.clock = clock
        self.saved = False
        self.apk = self.apk_sha = self.tar = self.manifest = None
        self.apk_debuggable = None
        self.dir = None
        self.lost = None  # who agreed to lose the data, once saved without it

    @classmethod
    def from_kept(cls, dev, directory, log=lambda s: None, twin_apk=None, apksigner=None):
        """The keeper of a copy a killed run left behind (base.apk and data.tar in `directory`), ready to restore()."""
        import hashlib
        import os
        import tarfile
        directory = os.path.abspath(directory)
        keeper = cls(dev, root=os.path.dirname(directory), log=log, twin_apk=twin_apk, apksigner=apksigner)
        keeper.dir = directory
        keeper.apk = os.path.join(directory, "base.apk")
        with open(keeper.apk, "rb") as f:
            keeper.apk_sha = hashlib.sha256(f.read()).hexdigest()
        tar = os.path.join(directory, "data.tar")
        if os.path.isfile(tar):
            keeper.tar = tar
            keeper.manifest = {}
            with tarfile.open(tar) as t:
                for member in t.getmembers():
                    if member.isfile():
                        name = member.name if member.name.startswith("./") else "./" + member.name
                        keeper.manifest[name] = hashlib.sha256(t.extractfile(member).read()).hexdigest()
        keeper.saved = True
        return keeper

    # -- reading the device -------------------------------------------------------------------
    def installed(self):
        """(path of the base APK on the phone, its sha256) or (None, None) when not installed."""
        paths = [l.split(":", 1)[1] for l in self.dev.sh(f"pm path {PACKAGE}", check=False).split() if l.startswith("package:")]
        if not paths:
            return None, None
        if len(paths) > 1:
            raise Unmeasurable(f"{PACKAGE} is installed as {len(paths)} split APKs; the saved state cannot be put back "
                               "by this tool (it restores a single APK)")
        sha = (self.dev.sh(f"sha256sum {paths[0]}", check=False).split() or [""])[0]
        return paths[0], sha

    def read_manifest(self):
        out = self.dev.sh(MANIFEST, check=False)
        rows = {}
        for line in out.splitlines():
            parts = line.split(None, 1)
            if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
                rows[parts[1].strip()] = parts[0]
        return rows

    # -- save ---------------------------------------------------------------------------------
    def save(self):
        import hashlib
        import os
        import tarfile
        if self.saved:
            return
        path, sha = self.installed()
        if path is None:
            self.log("keep: the app is not installed on the phone; there is no saved state to keep")
            self.saved = True
            return
        self.apk_debuggable = debuggable(self.dev)
        can_read = self.apk_debuggable or (self.twin_apk and os.path.isfile(self.twin_apk))
        if not can_read and not self.accept_loss:
            raise Refused(f"the installed {PACKAGE} is a release build with no debuggable twin to read it through: it has no "
                          "run-as, so its saved state (pairing and conversation) cannot be copied off the phone. Nothing was "
                          "touched. Give the debuggable twin of this build (--seed-twin), or pass --accept-state-loss <who "
                          "agreed> only when its saved state is already empty or that person agreed to lose it")
        base = os.path.join(self.root, f"{self.dev.serial}-{self.clock()}")
        os.makedirs(base, mode=0o700, exist_ok=False)
        os.chmod(base, 0o700)
        self.dir = base
        self.dev.sh(f"am force-stop {PACKAGE}")
        local = os.path.join(base, "base.apk")
        self.dev.run("pull", path, local)
        with open(local, "rb") as f:
            pulled = hashlib.sha256(f.read()).hexdigest()
        if pulled != sha:
            raise Unmeasurable(f"saving the installed APK: the copy reads {pulled[:12]}…, the phone's reads {sha[:12]}…; "
                               f"nothing changed on the phone (copy kept at {base})")
        self.apk, self.apk_sha = local, sha
        if can_read:
            if not self.apk_debuggable:
                # the twin and the app on the phone must carry one signature, checked on the copy just
                # pulled, before anything is installed
                require_same_signer(local, self.twin_apk, self.apksigner)
                install_over(self.dev, self.twin_apk, "the debuggable twin (to read the app's data)")
            data = self.dev.run_bytes("exec-out", "run-as", PACKAGE, "tar", "-cf", "-", ".")
            self.tar = os.path.join(base, "data.tar")
            with open(self.tar, "wb") as f:
                f.write(data)
            os.chmod(self.tar, 0o600)
            try:
                with tarfile.open(self.tar) as t:
                    t.getmembers()
            except tarfile.TarError as e:
                raise Unmeasurable(f"the app's saved data did not stream back as a readable tar ({e}); "
                                   f"copy kept at {base}")
            self.manifest = self.read_manifest()
            self.log(f"keep: saved the installed APK ({sha[:12]}…) and {len(self.manifest)} data files to {base}")
        else:
            self.lost = self.accept_loss
            self.log(f"keep: saved the installed APK ({sha[:12]}…); its data cannot be saved, {self.accept_loss} agreed to lose it")
        self.saved = True

    # -- restore ------------------------------------------------------------------------------
    def apk_problem(self):
        _, sha = self.installed()
        if sha != self.apk_sha:
            return f"the installed APK reads {str(sha)[:12]}…, it was {self.apk_sha[:12]}…"
        return None

    def data_problem(self):
        now = self.read_manifest()
        if now == self.manifest:
            return None
        gone = sorted(set(self.manifest) - set(now))
        changed = sorted(k for k in set(self.manifest) & set(now) if self.manifest[k] != now[k])
        extra = sorted(set(now) - set(self.manifest))
        return (f"the app's data differs: {len(gone)} missing {gone[:3]}, {len(changed)} changed {changed[:3]}, "
                f"{len(extra)} extra {extra[:3]}")

    def put_data_back(self):
        """Remove the files the run added and extract the saved tar over the rest (run-as, app stopped,
        a debuggable build installed). Nothing is uninstalled or cleared."""
        now = self.read_manifest()
        for extra in sorted(set(now) - set(self.manifest)):
            self.dev.sh(f"run-as {PACKAGE} rm -f '{extra}'")
        self.dev.run("push", self.tar, KEEP_TAR)
        try:
            self.dev.sh(f"run-as {PACKAGE} tar -xf {KEEP_TAR}")
        finally:
            self.dev.sh(f"rm -f {KEEP_TAR}", check=False)

    def restore(self):
        """Put the saved data and build back and read the hashes back. Returns the problems; [] means
        restored and verified, and the private copy is deleted. A problem keeps the copy and names it."""
        import shutil
        if not self.saved or self.apk is None:
            return []
        problems = []
        try:
            self.dev.sh(f"am force-stop {PACKAGE}", check=False)
            if self.tar is not None:
                if not debuggable(self.dev):
                    if not self.twin_apk:
                        raise Refused("the saved data can only be put back through a debuggable build and none is installed or given")
                    install_over(self.dev, self.twin_apk, "the debuggable twin (to put the app's data back)")
                if self.data_problem():
                    self.put_data_back()
                problem = self.data_problem()  # read back while a debuggable build is installed
                if problem:
                    problems.append(problem)
            if self.apk_problem():
                install_over(self.dev, self.apk, "the saved build")
            problem = self.apk_problem()
            if problem:
                problems.append(problem)
        except Exception as e:  # noqa: BLE001 — a restore that cannot run is a problem, said loudly
            problems.append(f"{type(e).__name__}: {e}")
        if problems:
            problems.append(f"the saved copy is KEPT at {self.dir}")
            return problems
        self.log("keep: the app's data and the installed build are back as they were, hashes read back")
        shutil.rmtree(self.dir, ignore_errors=True)
        self.saved = False
        return []


# ---------------------------------------------------------------------------------------------
# The measurements
# ---------------------------------------------------------------------------------------------

class Measure:
    """One run's measurements against one device. Each method returns what it measured, with
    rejected trials and their reasons, or raises Unmeasurable with its sentence."""

    def __init__(self, device, log=lambda s: None, settle_s=2.0, pace=None, evidence_dir=None):
        self.d = device
        self.bridge = Bridge(device)
        self.log = log
        self.settle_s = settle_s
        # Before each trial and window: wait until the device is quiet (an emulator's host process
        # below the pacing limit). Also what keeps back-to-back launches under the Mac's CPU circuit
        # breaker, which stops an emulator above 3 cores for 10 s (engine cpu_guard.py).
        self.pace = pace or (lambda: None)
        self.rooted = False
        self.rooted_by_us = False
        self.evidence_dir = Path(evidence_dir) if evidence_dir else None
        self.launch_number = 0
        # Start with a capital so sentence capitalization cannot change the probe on commit.
        self.probe_prefix = f"Perf probe {time.time_ns()}"

    # -- helpers -------------------------------------------------------------------------------
    def dump_ui(self):
        path = "/sdcard/richos-perf-ui.xml"
        for _ in range(5):
            self.d.sh(f"uiautomator dump {path}", check=False)
            text = self.d.run("exec-out", "cat", path, check=False)
            if text.startswith("<?xml"):
                self.d.sh(f"rm -f {path}", check=False)
                return ui_nodes(text)
            self.d.sleep(1)
        raise Unmeasurable("no UI dump after 5 attempts")

    def foreground(self):
        out = self.d.sh(f"am start -W -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -n {ACTIVITY}")
        self.last_launch_output = out
        return parse_am_start(out)

    def home(self):
        self.d.sh("input keyevent KEYCODE_HOME")

    def gfx_reset(self):
        self.d.sh(f"dumpsys gfxinfo {PACKAGE} reset")

    def gfx(self):
        return app_window(parse_framestats(self.d.sh(f"dumpsys gfxinfo {PACKAGE} framestats")))

    def ensure_root(self):
        """Root for /proc/<pid>/io and tracefs; only an emulator's google_apis image (or a
        userdebug phone) grants it. Returns False, never raises, when it is not available."""
        if self.rooted or self.d.sh("id -u", check=False).strip() == "0":
            self.rooted = True
            return True
        try:
            self.d.run("root", check=False)
            self.d.run("wait-for-device", timeout=60, check=False)
        except Unmeasurable:
            return False
        for _ in range(20):
            if self.d.sh("id -u", check=False).strip() == "0":
                self.rooted = self.rooted_by_us = True
                return True
            self.d.sleep(0.5)
        return False

    def restore_root(self):
        """Put adbd back as it was, if this run made it root."""
        if self.rooted_by_us:
            try:
                self.d.run("unroot", check=False)
                self.d.run("wait-for-device", timeout=60, check=False)
            except Unmeasurable:
                pass
            self.rooted = self.rooted_by_us = False

    def proc_io(self, pid):
        return parse_proc_io(self.d.sh(f"cat /proc/{pid}/io"))

    def fsync_start(self):
        self.d.sh(f"mkdir -p {FSYNC_INSTANCE} && echo 0 > {FSYNC_INSTANCE}/tracing_on && echo > {FSYNC_INSTANCE}/trace "
                  f"&& echo 1 > {FSYNC_INSTANCE}/options/record-tgid")
        enabled = 0
        for ev in FSYNC_EVENTS:
            answer = self.d.sh(f"[ -e {FSYNC_INSTANCE}/events/{ev}/enable ] && echo 1 > {FSYNC_INSTANCE}/events/{ev}/enable && echo on",
                               check=False)
            if answer.strip() == "on":
                enabled += 1
        if not enabled:
            self.d.sh(f"rmdir {FSYNC_INSTANCE}", check=False)
            raise Unmeasurable("no ext4/f2fs sync_file_enter trace event on this kernel")
        self.d.sh(f"echo 1 > {FSYNC_INSTANCE}/tracing_on")

    def fsync_stop(self, pid):
        self.d.sh(f"echo 0 > {FSYNC_INSTANCE}/tracing_on", check=False)
        trace = self.d.sh(f"cat {FSYNC_INSTANCE}/trace", check=False)
        for ev in FSYNC_EVENTS:
            self.d.sh(f"[ -e {FSYNC_INSTANCE}/events/{ev}/enable ] && echo 0 > {FSYNC_INSTANCE}/events/{ev}/enable", check=False)
        self.d.sh(f"rmdir {FSYNC_INSTANCE}", check=False)
        return count_fsyncs(trace, pid)

    def atrace(self, categories, action, settle_s):
        """Run `action()` inside an atrace capture of the app; returns the trace text."""
        self.d.begin_change("atrace", ATRACE_UNDO)
        self.d.sh(f"atrace --async_start -b 16384 -a {PACKAGE} {' '.join(categories)}")
        error = None
        try:
            self.d.sleep(0.3)
            action()
            self.d.sleep(settle_s)
        except Exception as failure:
            error = failure
        finally:
            self.d.sh(f"atrace --async_stop -o {TRACE_FILE}", check=False)
            self.d.end_change("atrace", ATRACE_UNDO)
        text = self.d.run("exec-out", "cat", TRACE_FILE, check=False)
        self.d.sh(f"rm -f {TRACE_FILE}", check=False)
        self.last_trace = text
        if error: raise error
        return text

    # -- setup -----------------------------------------------------------------------------------
    def seed(self, history, transport):
        self.bridge.call("fixture", "online")
        if history:
            state = self.bridge.action(history_frame(history))["state"]
            if len(state["messages"]) != history:
                raise Unmeasurable(f"seeding {history} messages left {len(state['messages'])}")
        self.bridge.call("transport", transport)
        return {"fixture": "online", "history": history, "transport": transport}

    def transport(self, mode):
        self.bridge.call("transport", mode)

    # -- cold launch -----------------------------------------------------------------------------
    def traced_launch(self):
        self.gfx_reset()
        launch = {}
        snapshots = []
        def snapshot():
            # The phone's framestats ring is short (about 10 rows on the Honor): a launch followed by a
            # burst of frames pushes the useful draw's own frame out before one late read. Read it while
            # the frame is still there, and again as the app settles; the reads are merged.
            try: snapshots.append(self.gfx()["rows"])
            except Exception: pass
        def start():
            launch.update(self.foreground())
            snapshot()
            for _ in range(3):
                self.d.sleep(0.4)
                snapshot()
        error = None
        self.last_trace = ""
        self.last_launch_output = ""
        try: trace = self.atrace(["am", "view", "gfx"], start, 2.0)
        except Exception as failure:
            error, trace = failure, self.last_trace
        try:
            window = self.gfx()
            window = {**window, "rows": merge_frame_rows(*snapshots, window["rows"])}
        except Exception as failure:
            window = {"rows": merge_frame_rows(*snapshots), "error": str(failure)}
            error = error or failure
        pid = self.d.pid()
        self.launch_number += 1
        if self.evidence_dir:
            self.evidence_dir.mkdir(parents=True, exist_ok=True)
            stem = self.evidence_dir / f"launch-{self.launch_number:04d}"
            with gzip.open(str(stem) + ".trace.gz", "wt") as output: output.write(trace)
            Path(str(stem) + ".json").write_text(json.dumps({"launch": launch, "rawLaunch": self.last_launch_output,
                "error": str(error) if error else None, "pid": pid, "window": window}))
        if error: raise error
        detail = useful_launch_frame(trace, window["rows"], pid)
        return launch, detail

    def cold(self, trials, newest_text=None, physical=False):
        first, useful, rejected, details = [], [], [], []
        budget, i = trials, -1
        while i + 1 < budget:
            i += 1
            self.pace()
            self.d.sh(f"am force-stop {PACKAGE}")
            self.d.sleep(1.0)
            if physical:
                try:
                    launch, detail = self.traced_launch()
                    if launch["launchState"] != "COLD": raise Unmeasurable("not a cold launch")
                    first.append(launch["totalMs"]); useful.append(detail["usefulMs"]); details.append(detail)
                    self.log(f"cold {i + 1}/{trials}: first frame {launch['totalMs']} ms, useful frame presented {detail['usefulMs']} ms")
                    if perfcore.cold_should_stop("android", useful):  # §104: starts 2-5 over the limit end the run at once
                        self.log(f"cold start {len(useful)} over the {perfcore.COLD_STANDARD['android']['earlyMs']} ms limit: stopping")
                        break
                except Unmeasurable as error:
                    rejected.append({"trial": i + 1, "why": str(error)})
                    self.log(f"cold {i + 1}/{trials}: REJECTED, {error}")
                    if len(rejected) >= 2 and not useful: break
                    if budget < trials + UNTIMED_RETRIES: budget += 1  # the start was not timed: one more start takes its place
                self.d.sleep(self.settle_s)
                continue
            since = self.d.uptime_epoch()
            launch = self.foreground()
            if launch["launchState"] != "COLD":
                rejected.append({"trial": i + 1, "why": f"LaunchState {launch['launchState']}, not COLD"})
                self.log(f"cold {i + 1}/{trials}: REJECTED, {rejected[-1]['why']}")
                continue
            drawn = None
            for _ in range(40):
                log = self.d.run("logcat", "-d", "-v", "epoch", "-T", since, "-s", "ActivityTaskManager:I", check=False)
                drawn = parse_fully_drawn(log)
                if drawn is not None:
                    break
                self.d.sleep(0.25)
            if drawn is None:
                rejected.append({"trial": i + 1, "why": "no Fully drawn report within 10 s"})
                continue
            first.append(launch["totalMs"])
            useful.append(drawn)
            self.log(f"cold {i + 1}/{trials}: first frame {launch['totalMs']} ms, useful content {drawn} ms")
            self.d.sleep(self.settle_s)
        check = None
        if newest_text:
            nodes = self.dump_ui()
            check = {"composerOnScreen": find_node(nodes, desc="Message Rich") is not None,
                     "newestMessageOnScreen": find_node(nodes, contains=newest_text) is not None}
        return {"first": first, "useful": useful, "rejected": rejected, "screenCheck": check, "presentationSamples": details}

    # -- warm resume -----------------------------------------------------------------------------
    def warm(self, trials, away_s=2.0, physical=False):
        """PRD J2: a resume with the PROCESS retained is warm. Android reports HOT (the activity kept)
        or WARM (the process kept, the activity recreated); both count, each sample keeps its state,
        and the first recreation's reason is read from the system's event log. A new pid is a cold
        start and is rejected."""
        samples, states_seen, rejected, states = [], [], [], {}
        destroyed = None
        details = []
        self.foreground()
        self.d.sleep(self.settle_s)
        budget, i = trials, -1
        while i + 1 < budget:
            i += 1
            self.pace()
            pid = self.d.pid()
            since = self.d.uptime_epoch()
            self.home()
            self.d.sleep(away_s)
            self.pace()  # the launcher settles before the resume is timed
            detail = None
            if physical:
                try: launch, detail = self.traced_launch()
                except Unmeasurable as error:
                    rejected.append({"trial": i + 1, "why": str(error)})
                    self.log(f"warm {i + 1}/{trials}: REJECTED, {error}")
                    if len(rejected) >= 2 and not samples: break
                    if budget < trials + UNTIMED_RETRIES: budget += 1  # the start was not timed: one more start takes its place
                    continue
            else: launch = self.foreground()
            after = self.d.pid()
            state = launch["launchState"]
            states[state] = states.get(state, 0) + 1
            if pid is None or after != pid:
                rejected.append({"trial": i + 1, "why": f"process changed ({pid} -> {after}): a cold start, never counted as warm"})
                self.log(f"warm {i + 1}/{trials}: REJECTED, {rejected[-1]['why']}")
            elif state not in ("HOT", "WARM"):
                rejected.append({"trial": i + 1, "why": f"LaunchState {state}"})
                self.log(f"warm {i + 1}/{trials}: REJECTED, {rejected[-1]['why']}")
            else:
                samples.append(detail["usefulMs"] if detail else launch["totalMs"])
                if detail: details.append(detail)
                states_seen.append(state)
                self.log(f"warm {i + 1}/{trials}: {samples[-1]} ms ({state})")
                if physical:  # the warm standard: starts 2-5 over the limit end the run at once; unset limits judge later, loudly
                    try:
                        if perfcore.warm_should_stop("android", samples):
                            self.log(f"warm start {len(samples)} over the limit: stopping")
                            break
                    except perfcore.LimitNotSet:
                        pass
                if state == "WARM" and destroyed is None:
                    events = self.d.run("logcat", "-b", "events", "-d", "-v", "epoch", "-T", since, check=False)
                    destroyed = [l.split("wm_destroy_activity: ", 1)[1].strip() for l in events.splitlines()
                                 if "wm_destroy_activity" in l and PACKAGE in l][:3]
            self.d.sleep(self.settle_s)
        return {"presentationSamples": details, "samples": samples, "states": states_seen, "rejected": rejected, "launchStates": states,
                "firstRecreation": destroyed}

    # -- tap to feedback -------------------------------------------------------------------------
    def tap(self, trials, production=False):
        samples, settled, rejected, frames = [], [], [], []
        pid = self.d.pid()
        for i in range(trials):
            self.pace()
            probe = f"{self.probe_prefix} {i + 1}"
            if production:
                try:
                    self.enter_empty_composer(probe)
                except Unmeasurable as error:
                    rejected.append({"trial": i + 1, "why": str(error), "stoppedSeries": True})
                    self.log(f"tap {i + 1}/{trials}: STOPPED, {error}")
                    break
            else:
                self.bridge.action({"type": "compose", "text": probe})
            self.d.sleep(1.0)
            before = self.dump_ui()
            field = composer_node(before)
            if production and (field is None or field.get("text") != probe):
                if self.evidence_dir:
                    self.evidence_dir.mkdir(parents=True, exist_ok=True)
                    observed = field.get("text", "") if field else ""
                    (self.evidence_dir / f"tap-{i + 1:04d}.input.json").write_text(json.dumps({
                        "requested": probe, "composerFound": field is not None, "observedLength": len(observed),
                        "observedProbe": observed if observed.lower().startswith("perf probe") else None,
                    }))
                rejected.append({"trial": i + 1, "why": "synthetic input changed before Send; draft retained", "stoppedSeries": True})
                self.log(f"tap {i + 1}/{trials}: STOPPED, synthetic input changed before Send; draft retained")
                break
            send = find_node(before, desc="Send message")
            if send is None:
                rejected.append({"trial": i + 1, "why": "no 'Send message' control on screen"})
                self.log(f"tap {i + 1}/{trials}: REJECTED, {rejected[-1]['why']}")
                continue
            x, y = center(send)
            self.gfx_reset()
            trace = self.atrace(["input", "view"], lambda: self.d.sh(f"input tap {x} {y}"), 1.5)
            window = self.gfx()
            if self.evidence_dir:
                self.evidence_dir.mkdir(parents=True, exist_ok=True)
                stem = self.evidence_dir / f"tap-{i + 1:04d}"
                with gzip.open(str(stem) + ".trace.gz", "wt") as output: output.write(trace)
                Path(str(stem) + ".json").write_text(json.dumps({"pid": pid, "window": window}))
            try:
                r = tap_latency(parse_input_events(trace, pid), window["rows"])
            except Unmeasurable as e:
                rejected.append({"trial": i + 1, "why": str(e)})
                self.log(f"tap {i + 1}/{trials}: REJECTED, {e}")
                continue
            after = self.dump_ui()
            composer = composer_node(after)
            bubble = any(n.get("text") == probe and n.get("class") != "android.widget.EditText" for n in after)
            if self.evidence_dir:
                # Keep only this tool's synthetic probe and composer metadata, not other messages.
                Path(str(stem) + ".validation.json").write_text(json.dumps({
                    "probe": probe, "probeNodes": [n for n in after if probe in n.get("text", "")],
                    "composerFound": composer is not None,
                    "composerTextLength": len(composer.get("text", "")) if composer else None,
                    "composerEmpty": composer is not None and composer.get("text", "") in ("", "Message Rich"),
                }))
            if not bubble or (production and (composer is None or composer.get("text", "") not in ("", "Message Rich"))):
                rejected.append({"trial": i + 1, "why": "the unique probe is not a conversation row with the composer cleared"})
                self.log(f"tap {i + 1}/{trials}: REJECTED, {rejected[-1]['why']}")
                continue
            samples.append(r["inputToFrameMs"])
            settled.append(r["inputToSettledMs"])
            frames.append(r["burstFrames"])
            self.log(f"tap {i + 1}/{trials}: first frame {r['inputToFrameMs']} ms, settled {r['inputToSettledMs']} ms")
        return {"samples": samples, "settled": settled, "burstFrames": frames, "rejected": rejected}

    # -- idle frames -------------------------------------------------------------------------------
    def idle(self, seconds):
        self.pace()
        self.gfx_reset()
        self.d.sleep(seconds)
        window = self.gfx()
        return {"seconds": seconds, "framesRendered": window["totalFrames"],
                "framesPerSecond": round((window["totalFrames"] or 0) / seconds, 2), "timing": frame_timing(window["rows"])}

    def open_settings(self):
        button = find_node(self.dump_ui(), desc="Settings")
        if button is None:
            raise Unmeasurable("no Settings control on screen")
        x, y = center(button)
        self.d.sh(f"input tap {x} {y}")
        self.d.sleep(self.settle_s)
        if find_node(self.dump_ui(), text="Show reply previews") is None:
            raise Unmeasurable("the Settings sheet did not open")

    def close_sheet(self):
        self.bridge.action({"type": "close-sheet"})
        self.d.sleep(self.settle_s)

    # -- active frames: scrolling the transcript ----------------------------------------------------
    def scroll(self, swipes):
        """Half the swipes drag the transcript toward older rows, half back. framestats keeps only
        the last 120 frames, so each swipe is read on its own and the rows are pooled."""
        nodes = self.dump_ui()
        composer = find_node(nodes, desc="Message Rich")
        if composer is None:
            raise Unmeasurable("no conversation on screen to scroll")
        width = composer["bounds"][2]
        top = composer["bounds"][1]
        x, y_high, y_low = width // 2, int(top * 0.35), int(top * 0.85)
        rows, totals = [], 0
        for i in range(swipes):
            older = i < (swipes + 1) // 2
            y1, y2 = (y_high, y_low) if older else (y_low, y_high)
            self.pace()
            self.gfx_reset()
            self.d.sh(f"input swipe {x} {y1} {x} {y2} 250")
            self.d.sleep(1.2)
            window = self.gfx()
            rows.extend(window["rows"])
            totals += window["totalFrames"] or 0
        timing = frame_timing(rows)
        return {"swipes": swipes, "framesRendered": totals, "framesTimed": timing.get("frames", 0), "timing": timing}

    # -- typing and streaming cost ----------------------------------------------------------------
    def _cost(self, pid, action, units, with_io):
        io0 = self.proc_io(pid) if with_io else None
        fsync_on = False
        if with_io:
            try:
                self.fsync_start()
                fsync_on = True
            except Unmeasurable:
                fsync_on = False
        self.gfx_reset()
        action()
        self.d.sleep(1.0)
        window = self.gfx()
        fsyncs = self.fsync_stop(pid) if fsync_on else None
        io1 = self.proc_io(pid) if with_io else None
        out = {"units": units, "framesRendered": window["totalFrames"], "frames": frame_timing(window["rows"])}
        if with_io:
            delta = io_delta(io0, io1)
            out["io"] = delta
            out["fsyncs"] = fsyncs
            out["perUnit"] = {"writeSyscalls": round(delta["syscw"] / units, 2),
                              "bytesWritten": round(delta["wchar"] / units),
                              "storageBytes": round(delta.get("write_bytes", 0) / units),
                              "fsyncs": round(fsyncs / units, 2) if fsyncs is not None else None,
                              "frames": round((window["totalFrames"] or 0) / units, 2)}
        return out

    def enter_empty_composer(self, text):
        field = composer_node(self.dump_ui())
        if field is None:
            raise Unmeasurable("the production composer is absent from the UI dump; no text was entered")
        if field.get("text", "") not in ("", "Message Rich"):
            raise Unmeasurable("production probes require an empty composer; existing work was left untouched")
        if not re.fullmatch(r"[A-Za-z0-9 ]+", text):
            raise Unmeasurable("probe text must contain only letters, digits and spaces")
        x, y = center(field)
        self.d.sh(f"input tap {x} {y}")
        # A whole `input text` burst can give its generated keys one event timestamp.
        # Prepare ASCII probes through separate input calls instead. This is outside
        # the measured Send interval; the exact pre-Send readback still decides
        # whether the probe may be sent. Never repair a mismatched draft silently.
        characters = " ".join("%s" if character == " " else character for character in text)
        self.d.sh('for richconnect_probe_character in ' + characters
                  + '; do input text "$richconnect_probe_character"; done')

    def typing(self, text, production=False):
        pid = self.d.pid()
        with_io = self.ensure_root()
        if production:
            self.enter_empty_composer("a")
            self.d.sh("input keyevent KEYCODE_DEL")
        else:
            self.bridge.action({"type": "compose", "text": ""})
        self.d.sleep(0.5)
        field = composer_node(self.dump_ui())
        if field is None:
            raise Unmeasurable("no composer field on screen")
        x, y = center(field)
        self.d.sh(f"input tap {x} {y}")
        self.d.sleep(1.0)

        def type_all():
            for ch in text:
                self.pace()
                self.d.sh(f"input text {ch}")
        out = self._cost(pid, type_all, len(text), with_io)
        draft = (composer_node(self.dump_ui()) or {}).get("text") if production else self.bridge.state()["draft"]
        out["draftMatches"] = draft == text
        self.d.sh("input keyevent KEYCODE_BACK", check=False)
        if production:
            # This command inserted this exact draft into a verified empty test composer.
            if draft != text: raise Unmeasurable("typed draft differs; leave it for review")
            for _ in text: self.d.sh("input keyevent KEYCODE_DEL")
        else:
            self.bridge.action({"type": "compose", "text": ""})
        if not with_io:
            out["ioWhy"] = "no root: /proc/<pid>/io and the fsync trace need root (an emulator's google_apis image grants it)"
        return out

    def streaming(self, deltas, cursor):
        pid = self.d.pid()
        with_io = self.ensure_root()
        opening, middle, final, full = stream_frames(cursor, deltas)
        # One unbroken reply: the opening row, the deltas back to back, the final row, all inside
        # the measured window. A streaming row animates its "replying" mark for as long as it
        # streams, and on a software-GPU emulator that alone holds the host above the Mac's CPU
        # breaker limit, so nothing waits between deltas; the quiet check comes before the reply.
        self.pace()
        states = []

        def feed():
            self.bridge.action(opening)
            for frame in middle:
                self.bridge.action(frame)
            states.append(self.bridge.action(final)["state"])
        out = self._cost(pid, feed, deltas, with_io)
        out["includes"] = "the opening and final rows as well as the deltas; per-delta figures divide the whole reply"
        out["replyMatches"] = any(m.get("text") == full for m in states[0]["messages"])
        if not with_io:
            out["ioWhy"] = "no root: /proc/<pid>/io and the fsync trace need root"
        return out

    # -- background --------------------------------------------------------------------------------
    def background_physical(self, seconds, settle_s, uid):
        """Read-only accounting on a user's phone. Never reset their battery history or fake unplugging."""
        pid = self.d.pid()
        if pid is None:
            raise Unmeasurable("the app is not running")
        self.home()
        self.d.sleep(settle_s)
        def process_sample():
            try:
                raw = self.d.sh(f"cat /proc/{pid}/stat", check=False)
                fields = raw[raw.rfind(")") + 2:].split()
                return {"ticks": parse_proc_stat_ticks(raw), "started": fields[19]}
            except (Unmeasurable, ValueError, IndexError): return None
        process_before = process_sample()
        power_before = self.d.sh("dumpsys battery", check=False)
        before = parse_checkin(self.d.sh("dumpsys batterystats --checkin"), uid)
        self.d.sleep(seconds)
        alive = self.d.pid() == pid
        process_after = process_sample() if alive else None
        ticks = None
        if process_before and process_after and process_before["started"] == process_after["started"]:
            value = process_after["ticks"] - process_before["ticks"]
            if value >= 0: ticks = value
        after = parse_checkin(self.d.sh("dumpsys batterystats --checkin"), uid)
        delta = {"uidSeen": before["uidSeen"] and after["uidSeen"], "cpuMs": None, "networkBytes": None,
                 "wakeupAlarms": [], "wakelocks": [], "jobs": [], "syncs": [], "partialWakelockMs": None, "processes": []}
        for field in ("cpuMs", "networkBytes"):
            if before[field] is not None and after[field] is not None:
                shared = before[field].keys() & after[field].keys()
                values = {k: after[field][k] - before[field][k] for k in shared
                          if isinstance(before[field][k], (int, float)) and isinstance(after[field][k], (int, float))}
                if values and all(v >= 0 for v in values.values()): delta[field] = values
        if before["partialWakelockMs"] is not None and after["partialWakelockMs"] is not None:
            value = after["partialWakelockMs"] - before["partialWakelockMs"]
            if value >= 0: delta["partialWakelockMs"] = value
        locks = [line.strip() for line in self.d.sh("dumpsys power", check=False).splitlines()
                 if "WAKE_LOCK" in line and (f"uid={uid}" in line or PACKAGE in line)]
        return {"seconds": seconds, "settleSeconds": settle_s, "processAliveAtEnd": alive,
                "settleWindowWakeups": None, "threadWakeups": None, "cpuTicks": ticks,
                "processBefore": process_before, "processAfter": process_after,
                "batterystats": delta, "accountingBefore": before, "accountingAfter": after,
                "heldWakeLocks": locks, "pendingAlarms": None, "scheduledJobs": None, "openSockets": None,
                "readOnlyPhysicalObservation": True, "powerBefore": power_before.strip(),
                "measurementLimits": "Read-only UID accounting and available process CPU ticks. Charging can pause battery accounting; inaccessible thread/socket counters are unknown. "
                                     "No zero-work or no-warning verdict is inferred, even if the process exited. Battery history and power state were preserved."}

    def background(self, seconds, settle_s, uid, physical=False):
        if physical:
            return self.background_physical(seconds, settle_s, uid)
        pid = self.d.pid()
        if pid is None:
            raise Unmeasurable("the app is not running")
        thread_cmd = (f"for t in /proc/{pid}/task/*; do echo \"${{t##*/}}|$(cat $t/comm)|"
                      f"$(grep -E '^(voluntary|nonvoluntary)_ctxt_switches' $t/status | tr -s ' \\t\\n' ' ')\"; done")
        t_front = parse_threads(self.d.sh(thread_cmd))
        self.home()
        self.d.sleep(settle_s)
        self.d.begin_change("battery", BATTERY_UNDO)
        self.d.sh("dumpsys battery unplug", check=False)
        try:
            self.d.sh("dumpsys batterystats --reset", check=False)
            t0 = parse_threads(self.d.sh(thread_cmd))
            settling = thread_wakeups(t_front, t0)
            s0 = parse_proc_stat_ticks(self.d.sh(f"cat /proc/{pid}/stat"))
            self.d.sleep(seconds)
            alive = self.d.pid() == pid
            t1 = parse_threads(self.d.sh(thread_cmd)) if alive else {}
            s1 = parse_proc_stat_ticks(self.d.sh(f"cat /proc/{pid}/stat")) if alive else None
            checkin = parse_checkin(self.d.sh("dumpsys batterystats --checkin"), uid)
        finally:
            self.d.end_change("battery", BATTERY_UNDO)
        alarms = [l.strip() for l in self.d.sh("dumpsys alarm", check=False).splitlines()
                  if PACKAGE in l and "Active uids" not in l]
        jobs = [l.strip() for l in self.d.sh(f"dumpsys jobscheduler {PACKAGE}", check=False).splitlines()
                if re.search(r"JOB #\S*" + re.escape(PACKAGE), l)]
        locks = [l.strip() for l in self.d.sh("dumpsys power", check=False).splitlines()
                 if "WAKE_LOCK" in l and (f"uid={uid}" in l or PACKAGE in l)]
        sockets = parse_sockets(self.d.sh("cat /proc/net/tcp /proc/net/tcp6 /proc/net/udp /proc/net/udp6", check=False), uid)
        return {"seconds": seconds, "settleSeconds": settle_s, "processAliveAtEnd": alive,
                "settleWindowWakeups": settling,
                "threadWakeups": thread_wakeups(t0, t1) if alive else None,
                "cpuTicks": (s1 - s0) if alive else None,
                "batterystats": checkin, "pendingAlarms": alarms, "scheduledJobs": jobs,
                "heldWakeLocks": locks, "openSockets": sockets}
