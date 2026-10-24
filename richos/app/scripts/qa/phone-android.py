#!/usr/bin/env python3
"""Drive a PHYSICAL Android phone the way a person does, and read what an app leaves running.

    phone-android.py --serial SERIAL texts                         every text and description on screen
    phone-android.py --serial SERIAL wait TEXT [--timeout S]       until a node shows TEXT (exit 1 on timeout)
    phone-android.py --serial SERIAL gone TEXT [--timeout S]       until no node shows TEXT (exit 1 on timeout)
    phone-android.py --serial SERIAL tap TEXT [--timeout S]        tap the center of the node showing TEXT
    phone-android.py --serial SERIAL node TEXT                     the matching nodes: class, bounds, focus, text
    phone-android.py --serial SERIAL swipe TEXT --dx DX --dy DY    swipe from that node's center (a Recents card)
    phone-android.py --serial SERIAL type-file FILE                type FILE's text, one `input text` per character
    phone-android.py --serial SERIAL keys FILE --keymap MAP.json   type FILE's text by TAPPING the on-screen
                                                                   keyboard's keys, as a thumb does (MAP: {"char": [x, y]})
    phone-android.py --serial SERIAL field                         the text of every editable field on screen
    phone-android.py --serial SERIAL unlabeled --package PKG       PKG's clickable nodes that carry neither text nor a
                                                                   content description: what a screen reader would
                                                                   announce as an unnamed button (exit 1 if any)
    phone-android.py --serial SERIAL shot FILE.png                 the screen, as a PNG on this Mac
    phone-android.py --serial SERIAL burst DIR --seconds S [--tap-desc TEXT]
                                                                   frames for S seconds into DIR, each named by
                                                                   its offset in ms; --tap-desc taps that control
                                                                   first and the offsets start at the tap
    phone-android.py --serial SERIAL idle-frames --package PKG --seconds S [--out F]
                                                                   untouched for S seconds: frames the app's window
                                                                   rendered and the spacing between them (a steady
                                                                   rhythm is an animation or a timer that never ends)
    phone-android.py --serial SERIAL state --package PKG [--out F] what PKG leaves running: see below
    phone-android.py --serial SERIAL launch                        open the TEST copy from its launcher activity (am start -W),
                                                                   cold or warm as it stands; its launch state and time
    phone-android.py --serial SERIAL close                         end the TEST copy (am force-stop), as the phone ends it
                                                                   for memory; the next launch is a cold start
    phone-android.py compare BEFORE.json AFTER.json                what changed between two `state` records
    phone-android.py --serial SERIAL observe --package PKG --label L --out-dir DIR --settle S --seconds N
                     [--action home|kill|force-stop|none]
                                                                   one closure-matrix cell: do the action
                                                                   (none = the walker already did it through real
                                                                   controls), settle S s, read `state`, wait N s,
                                                                   read it again, write L-start/L-end/L-compare
                                                                   .json into DIR and print the comparison

A node "shows" TEXT when its text or its content description equals TEXT, or starts with it when
TEXT ends in "…"; `--contains` matches a substring instead. The screen is read with
`uiautomator dump`, as `native-android/bin/emu-ui.py` reads an emulator. That tool refuses
anything but an emulator on purpose; this one is its physical-phone counterpart and refuses
anything but a serial that `adb devices` lists as an attached device. Never a bare adb: a second
device may be attached.

`state` is the closure matrix's reading (PRD 2026-09-24 section 8: "record the action and observed
process/job state"). It is READ-ONLY: no battery reset, no simulated unplug, no setting change.
It records the package's stopped flag, standby bucket, processes (pid, birth, CPU ticks, thread
context switches), whether its activity is resumed, its running services, held wake locks
(`dumpsys power`), microphone use (`cmd appops get PKG RECORD_AUDIO` and the audio service's
recording clients for the app's uid), scheduled jobs, pending alarms, the app's open sockets, its
own notifications, screen/keyguard state and the charger. The microphone is "running" only when
AppOps marks its RECORD_AUDIO access "(running)"; the audio service's start/stop lines are kept
as `recentRecordingEvents`, which is history. A counter the phone does not expose is
recorded as null with the reason, never as zero. `compare` subtracts CPU ticks and context
switches only when the SAME process (pid and birth time) is present at both ends.

Only the named package's lines are kept, and notification records only for that package: the
phone is a person's own phone, and nothing else on it belongs in a record.

ONLY THROUGH `randroid device` (CEO, 2026-10-02: only the release build goes on his phones and every
test on them tests it). Run as `randroid device --serial SERIAL <command> ...`; started any other way
it refuses before touching the phone. Every command except `compare` first reads the installed
RichConnect build in one adb round trip and refuses (exit 3, the sentence in `refused`) an emulator
or a debuggable build (richos/mobile/physical.py).

Prints one JSON document; exits 1 when the answer is "not found in time" and 2 when it cannot
answer at all (no such device, unreadable screen, bad arguments), with the sentence in `error`.
"""
import argparse
import datetime
import json
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "mobile"))
import physical  # noqa: E402 — richos/mobile/physical.py: the physical-phone rules

# Each character goes to the phone's shell single-quoted, so everything here is literal there;
# `'` cannot be quoted that way and `%` is `input text`'s own escape, so both are refused.
TYPEABLE = re.compile(r"[A-Za-z0-9 .,:!?/#=_+@-]+")


class CannotAnswer(Exception):
    pass


def emit(obj, code=0):
    print(json.dumps(obj, indent=2))
    return code


class Phone:
    def __init__(self, adb, serial):
        self.adb, self.serial = adb, serial

    def _completed(self, args, timeout):
        try:
            return subprocess.run([self.adb, "-s", self.serial, *args], capture_output=True, timeout=timeout)
        except FileNotFoundError:
            raise CannotAnswer(f"no adb at {self.adb}")
        except subprocess.TimeoutExpired:
            raise CannotAnswer(f"adb {' '.join(args)[:80]} did not answer in {timeout} s")

    def run(self, *args, check=True, timeout=60, binary=False):
        p = self._completed(args, timeout)
        if check and p.returncode != 0:
            raise CannotAnswer(f"adb {' '.join(args)[:80]} exited {p.returncode}: {p.stderr.decode(errors='replace').strip()[:160]}")
        return p.stdout if binary else p.stdout.decode(errors="replace")

    def sh(self, command, **kw):
        return self.run("shell", command, **kw)

    def read(self, command, timeout=60):
        """(output, None) when the command succeeded; (None, reason) when it failed. A failed read
        is not an empty answer: empty output from a command that exited 0 is a real empty."""
        p = self._completed(("shell", command), timeout)
        if p.returncode != 0:
            why = p.stderr.decode(errors="replace").strip()[:160] or p.stdout.decode(errors="replace").strip()[:160]
            return None, f"exited {p.returncode}" + (f": {why}" if why else "")
        return p.stdout.decode(errors="replace"), None

    def attached(self):
        try:
            p = subprocess.run([self.adb, "devices"], capture_output=True, text=True, timeout=15)
        except (FileNotFoundError, subprocess.TimeoutExpired) as e:
            raise CannotAnswer(f"adb devices failed: {e}")
        return any(line.split("\t") == [self.serial, "device"] for line in p.stdout.splitlines())

    def nodes(self):
        for _ in range(5):
            # The file name is fixed, so an interrupted earlier read or a failed cleanup can leave
            # an old layout behind. It is removed first, and a dump that did not exit 0 is not
            # read at all: the coordinates would be those of a screen that is no longer there.
            cleared = self._completed(("shell", "rm -f /sdcard/qa-phone-ui.xml"), 60).returncode == 0
            dumped = self._completed(("shell", "uiautomator dump /sdcard/qa-phone-ui.xml"), 60).returncode == 0
            raw = self.run("exec-out", "cat", "/sdcard/qa-phone-ui.xml", check=False) if cleared and dumped else ""
            self.sh("rm -f /sdcard/qa-phone-ui.xml", check=False)
            if raw.startswith("<?xml"):
                try:
                    root = ET.fromstring(raw)
                except ET.ParseError:
                    root = None
                if root is not None:
                    out = []
                    for n in root.iter("node"):
                        m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", n.get("bounds") or "")
                        out.append({"text": n.get("text") or "", "desc": n.get("content-desc") or "",
                                    "class": n.get("class") or "", "package": n.get("package") or "",
                                    "bounds": [int(g) for g in m.groups()] if m else None,
                                    "focused": n.get("focused") == "true", "clickable": n.get("clickable") == "true"})
                    return out
            time.sleep(0.5)
        raise CannotAnswer("the screen could not be read: five uiautomator dumps returned no layout")


def shows(node, want, contains=False):
    for value in (node["text"], node["desc"]):
        if contains and want in value:
            return True
        if value == want or (want.endswith("…") and value.startswith(want[:-1])):
            return True
    return False


def center(node):
    x1, y1, x2, y2 = node["bounds"]
    return (x1 + x2) // 2, (y1 + y2) // 2


def find(phone, want, timeout, contains=False):
    deadline = time.monotonic() + timeout
    while True:
        found = [n for n in phone.nodes() if n["bounds"] and shows(n, want, contains)]
        if found or time.monotonic() > deadline:
            return found
        time.sleep(0.4)


# -- the closure-state reading ------------------------------------------------------------------

def proc_stat(text):
    """/proc/<pid>/stat -> (utime+stime ticks, starttime); the comm field may hold spaces."""
    rest = text[text.rfind(")") + 2:].split()
    if len(rest) < 20:
        return None, None
    return int(rest[11]) + int(rest[12]), rest[19]


def thread_switches(text):
    out = {}
    for line in text.splitlines():
        parts = line.strip().split("|")
        if len(parts) == 3 and parts[0].isdigit():
            nums = [int(n) for n in re.findall(r"\d+", parts[2])]
            if len(nums) == 2:
                out[parts[0]] = {"comm": parts[1], "switches": sum(nums)}
    return out


def sockets(text, uid):
    names = {"01": "ESTABLISHED", "02": "SYN_SENT", "06": "TIME_WAIT", "08": "CLOSE_WAIT", "0A": "LISTEN"}
    counts, readable = {}, False
    for line in text.splitlines():
        cols = line.split()
        if cols and cols[0] == "sl":
            readable = True
        if len(cols) > 7 and cols[0].endswith(":") and cols[7] == str(uid):
            state = names.get(cols[3], cols[3])
            counts[state] = counts.get(state, 0) + 1
    return counts if readable else None


def state(phone, package):
    unreadable = {}

    def read(name, command):
        """The command's output, or None (with the reason kept) when it FAILED. `state` is the
        closure matrix's evidence that nothing is left running, so a read that failed must never
        look like an empty one: null with the reason, as the module doc promises."""
        text, why = phone.read(command)
        if why is not None:
            unreadable[name] = why
        return text

    pkg_text, why = phone.read(f"dumpsys package {package}")
    if why is not None:
        raise CannotAnswer(f"cannot tell whether {package} is installed: dumpsys package {why}")
    pkg_dump = pkg_text
    # Android 14 names it appId; older releases userId.
    uid_m = re.search(r"\b(?:appId|userId)=(\d+)", pkg_dump)
    if not uid_m:
        return {"package": package, "installed": False,
                "at": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    uid = int(uid_m.group(1))
    stopped = re.search(r"\bstopped=(true|false)", pkg_dump)
    bucket = read("standbyBucket", f"am get-standby-bucket {package}")
    rec = {"package": package, "installed": True, "uid": uid,
           "at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
           "uptimeSeconds": float(phone.sh("cat /proc/uptime").split()[0]),
           "stoppedFlag": (stopped.group(1) == "true") if stopped else None,
           "standbyBucket": (bucket.strip() or None) if bucket is not None else None}
    power = read("power", "dumpsys power")
    wake = re.search(r"mWakefulness=(\w+)", power) if power is not None else None
    rec["screen"] = wake.group(1) if wake else None
    window = read("window", "dumpsys window")
    kg = (re.search(r"isKeyguardShowing=(true|false)", window) or re.search(r"mShowingLockscreen=(true|false)", window)) if window is not None else None
    rec["keyguardShowing"] = (kg.group(1) == "true") if kg else None
    focus = re.search(r"mCurrentFocus=(.*)", window) if window is not None else None
    rec["appHasFocus"] = bool(focus and package in focus.group(1)) if window is not None else None
    battery = read("battery", "dumpsys battery")
    rec["charger"] = ({k: v for k, v in re.findall(r"^\s*(AC powered|USB powered|Wireless powered|level|status): (\S+)", battery, re.M)}
                      if battery is not None else None)

    ps = read("processes", "ps -A -o PID,NAME")
    if ps is None:
        procs = None
    else:
        procs = []
        for line in ps.splitlines():
            cols = line.split()
            if len(cols) == 2 and cols[0].isdigit() and (cols[1] == package or cols[1].startswith(package + ":")):
                pid = int(cols[0])
                ticks, birth = proc_stat(read(f"process {pid} stat", f"cat /proc/{pid}/stat") or "")
                threads = thread_switches(read(f"process {pid} threads",
                    f"for t in /proc/{pid}/task/*; do echo \"${{t##*/}}|$(cat $t/comm)|$(grep -E '^(voluntary|nonvoluntary)_ctxt_switches' $t/status | tr -s ' \\t\\n' ' ')\"; done") or "")
                procs.append({"pid": pid, "name": cols[1], "birth": birth, "cpuTicks": ticks,
                              "threads": threads or None, "threadCount": len(threads) if threads else None})
    rec["processes"] = procs
    acts = read("resumedActivity", "dumpsys activity activities")
    rec["resumedActivity"] = (any(package in l for l in acts.splitlines() if "ResumedActivity" in l)
                              if acts is not None else None)

    services = read("services", f"dumpsys activity services {package}")
    rec["services"] = sorted(set(re.findall(r"ServiceRecord\{\w+ u\d+ ([^}\s]+)", services))) if services is not None else None
    rec["foregroundServices"] = len(re.findall(r"isForeground=true", services)) if services is not None else None
    rec["wakeLocks"] = ([l.strip() for l in power.splitlines()
                         if "WAKE_LOCK" in l and (f"uid={uid}" in l or package in l)]
                        if power is not None else None)
    appop = read("microphone", f"cmd appops get {package} RECORD_AUDIO")
    rec["microphoneAppOp"] = appop.strip() if appop is not None else None
    # AppOps marks an access that is still open "(running)": that, and only that, is live capture.
    rec["microphoneRunning"] = ("(running)" in rec["microphoneAppOp"]) if appop is not None else None
    audio = read("recentRecordingEvents", "dumpsys audio")
    # The audio service's recent start/stop/release history for this uid: history, not live state.
    rec["recentRecordingEvents"] = ([l.strip() for l in audio.splitlines()
                                     if re.search(r"(session|riid).*uid[:=]\s*%d\b" % uid, l) and "rec" in l.lower()]
                                    if audio is not None else None)
    jobs = read("jobs", f"dumpsys jobscheduler {package}")
    rec["jobs"] = ([l.strip() for l in jobs.splitlines() if re.search(r"JOB #\S*" + re.escape(package), l)]
                   if jobs is not None else None)
    alarms = read("pendingAlarms", "dumpsys alarm")
    rec["pendingAlarms"] = ([l.strip() for l in alarms.splitlines() if re.search(r"Alarm\{[^}]*" + re.escape(package), l)]
                            if alarms is not None else None)
    net = phone.sh("cat /proc/net/tcp /proc/net/tcp6 /proc/net/udp /proc/net/udp6", check=False)
    rec["openSockets"] = sockets(net, uid)
    if rec["openSockets"] is None:
        rec["openSocketsWhy"] = "/proc/net is not readable to the adb shell on this phone"
    notes = read("ownNotifications", "dumpsys notification")
    rec["ownNotifications"] = (len(re.findall(r"NotificationRecord\([^)]*pkg=" + re.escape(package) + r"\b", notes))
                               if notes is not None else None)
    if unreadable:
        rec["unreadable"] = unreadable
    return rec


def frame_rows(framestats):
    """IntendedVsync (ns) of every frame row in `dumpsys gfxinfo PKG framestats`, per window."""
    windows, name = [], None
    for block in re.split(r"^(Window: .*)$", framestats, flags=re.M):
        if block.startswith("Window: "):
            name = block[len("Window: "):].strip()
            continue
        for part in block.split("---PROFILEDATA---")[1::2]:
            lines = [l.split(",") for l in part.strip().splitlines()]
            if not lines or lines[0][0] != "Flags":
                continue
            col = lines[0].index("IntendedVsync")
            windows.append({"window": name, "vsync": [int(r[col]) for r in lines[1:] if len(r) > col and re.fullmatch(r"\d+", r[col])]})
    return windows


def idle_frames(phone, package, seconds):
    phone.sh(f"dumpsys gfxinfo {package} reset")
    time.sleep(seconds)
    raw = phone.sh(f"dumpsys gfxinfo {package} framestats")
    totals = [int(n) for n in re.findall(r"Total frames rendered: (\d+)", raw)]
    if not totals:
        raise CannotAnswer(f"gfxinfo has no frame counts for {package}: is its window on screen?")
    rows = []
    for w in frame_rows(raw):
        gaps = [round((b - a) / 1e6, 1) for a, b in zip(w["vsync"], w["vsync"][1:])]
        rows.append({"window": w["window"], "framesWithTimestamps": len(w["vsync"]), "gapsMs": gaps})
    return {"package": package, "seconds": seconds, "totalFramesRendered": totals[0],
            "framesPerSecond": round(totals[0] / seconds, 2), "windows": rows,
            "note": "gfxinfo keeps only the most recent frame rows; the total counts every frame in the window"}


def compare(a, b):
    out = {"from": a.get("at"), "to": b.get("at"), "package": b.get("package")}
    if a.get("uptimeSeconds") is not None and b.get("uptimeSeconds") is not None:
        out["seconds"] = round(b["uptimeSeconds"] - a["uptimeSeconds"], 3)
    before = {(p["pid"], p["birth"]): p for p in a.get("processes") or []}
    same = []
    for p in b.get("processes") or []:
        q = before.get((p["pid"], p["birth"]))
        if q is None:
            continue
        row = {"pid": p["pid"], "name": p["name"], "cpuTicks": None, "threadSwitches": None}
        if q["cpuTicks"] is not None and p["cpuTicks"] is not None:
            row["cpuTicks"] = p["cpuTicks"] - q["cpuTicks"]
        if q.get("threads") and p.get("threads"):
            row["threadSwitches"] = sum(t["switches"] - q["threads"].get(tid, {"switches": 0})["switches"]
                                        for tid, t in p["threads"].items())
        same.append(row)
    out["sameProcess"] = same
    out["processesAtStart"] = [p["pid"] for p in a["processes"]] if a.get("processes") is not None else None
    out["processesAtEnd"] = [p["pid"] for p in b["processes"]] if b.get("processes") is not None else None
    for key in ("services", "wakeLocks", "jobs", "pendingAlarms"):
        out[key] = {"start": a.get(key), "end": b.get(key)}
    for key in ("stoppedFlag", "standbyBucket", "screen", "keyguardShowing", "resumedActivity",
                "microphoneRunning", "openSockets", "ownNotifications", "foregroundServices"):
        out[key] = {"start": a.get(key), "end": b.get(key)}
    return out


def main(argv):
    p = argparse.ArgumentParser(prog="phone-android.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--serial")
    p.add_argument("--adb", default="adb", help="adb executable (tests substitute a scripted one)")
    p.add_argument("command", choices=["launch", "close", "texts", "wait", "gone", "tap", "node", "swipe", "type-file", "keys", "field", "unlabeled", "shot", "burst", "idle-frames", "observe", "state", "compare"])
    p.add_argument("value", nargs="?")
    p.add_argument("value2", nargs="?")
    p.add_argument("--timeout", type=float, default=15)
    p.add_argument("--contains", action="store_true")
    p.add_argument("--dx", type=int, default=0)
    p.add_argument("--dy", type=int, default=0)
    p.add_argument("--package")
    p.add_argument("--seconds", type=float)
    p.add_argument("--tap-desc")
    p.add_argument("--keymap")
    p.add_argument("--label")
    p.add_argument("--out-dir")
    p.add_argument("--settle", type=float)
    p.add_argument("--action", default="none")
    p.add_argument("--out")
    a = p.parse_args(argv)
    try:
        if a.command == "compare":
            if not a.value or not a.value2:
                raise CannotAnswer("compare takes two state records: BEFORE.json AFTER.json")
            try:
                before, after = (json.loads(Path(f).read_text()) for f in (a.value, a.value2))
            except (OSError, ValueError) as e:
                raise CannotAnswer(f"unreadable state record: {e}")
            return emit({"ok": True, "compare": compare(before, after)})
        if not a.serial:
            raise CannotAnswer("name the phone with --serial: a bare adb reaches whatever is attached")
        physical.require_verb("randroid")
        phone = Phone(a.adb, a.serial)
        if not phone.attached():
            raise CannotAnswer(f"{a.serial} is not an attached, authorized device in `adb devices`")
        physical.gate(a.adb, a.serial)
        # Only ever the TEST copy (physical.PACKAGE): neither step takes a package, so neither can reach the
        # CEO's own RichConnect (CEO 2026-10-03), whatever --package says.
        if a.command == "launch":
            out = phone.sh(f"am start -W -a android.intent.action.MAIN -c android.intent.category.LAUNCHER "
                           f"-n {physical.PACKAGE}/dev.richos.android.app.MainActivity", timeout=60)
            status, launch, total = (re.search(rf"{k}:\s*(\S+)", out) for k in ("Status", "LaunchState", "TotalTime"))
            ok = bool(status and status.group(1) == "ok")
            return emit({"ok": ok, "launched": physical.PACKAGE, "launchState": launch and launch.group(1),
                         "totalMs": int(total.group(1)) if total else None,
                         **({} if ok else {"said": out.strip()[:300]})}, 0 if ok else 1)
        if a.command == "close":
            phone.sh(f"am force-stop {physical.PACKAGE}")
            return emit({"ok": True, "closed": physical.PACKAGE})
        if a.command == "texts":
            seen = [v for n in phone.nodes() for v in (n["text"], n["desc"]) if v]
            return emit({"ok": True, "texts": seen})
        if a.command == "observe":
            if not (a.package and a.label and a.out_dir and a.seconds is not None and a.settle is not None):
                raise CannotAnswer("observe needs --package, --label, --out-dir, --settle and --seconds")
            actions = {"home": "input keyevent KEYCODE_HOME", "kill": f"am kill {a.package}",
                       "force-stop": f"am force-stop {a.package}", "none": None}
            if a.action == "sleep":
                raise CannotAnswer("--action sleep is removed: a phone is never locked or put to sleep (CEO 2026-09-24)")
            if a.action not in actions:
                raise CannotAnswer(f"--action must be one of {', '.join(actions)}")
            out = Path(a.out_dir)
            out.mkdir(parents=True, exist_ok=True)
            acted = datetime.datetime.now(datetime.timezone.utc).isoformat()
            if actions[a.action]:
                phone.sh(actions[a.action])
            time.sleep(a.settle)
            start = state(phone, a.package)
            time.sleep(a.seconds)
            end = state(phone, a.package)
            result = compare(start, end)
            result.update({"label": a.label, "action": a.action, "actedAt": acted, "settleSeconds": a.settle,
                           "requestedSeconds": a.seconds})
            for name, rec in (("start", start), ("end", end), ("compare", result)):
                (out / f"{a.label}-{name}.json").write_text(json.dumps(rec, indent=2))
            return emit({"ok": True, "observe": result})
        if a.command == "idle-frames":
            if not a.package or not a.seconds or a.seconds <= 0:
                raise CannotAnswer("idle-frames needs --package and --seconds S")
            rec = idle_frames(phone, a.package, a.seconds)
            if a.out:
                Path(a.out).write_text(json.dumps(rec, indent=2))
            return emit({"ok": True, "idleFrames": rec})
        if a.command == "state":
            if not a.package:
                raise CannotAnswer("state needs --package")
            rec = state(phone, a.package)
            if a.out:
                Path(a.out).write_text(json.dumps(rec, indent=2))
                rec = dict(rec, processes=[{k: v for k, v in pr.items() if k != "threads"}
                                           for pr in rec.get("processes") or []]) if rec.get("processes") is not None else rec
            return emit({"ok": True, "state": rec})
        if a.command == "shot":
            if not a.value or not a.value.endswith(".png"):
                raise CannotAnswer("shot takes the path of a .png to write")
            png = phone.run("exec-out", "screencap", "-p", binary=True)
            if not png.startswith(b"\x89PNG"):
                raise CannotAnswer("screencap returned no PNG (a secure window or a locked screen can refuse it)")
            Path(a.value).write_bytes(png)
            return emit({"ok": True, "shot": a.value, "bytes": len(png)})
        if a.command == "burst":
            if not a.value or not a.seconds or a.seconds <= 0:
                raise CannotAnswer("burst takes a directory and --seconds S")
            out = Path(a.value)
            out.mkdir(parents=True, exist_ok=True)
            tap = None
            if a.tap_desc:
                found = find(phone, a.tap_desc, a.timeout, a.contains)
                if not found:
                    return emit({"ok": False, "missing": a.tap_desc}, 1)
                tap = center(found[0])
            frames = []
            start = time.monotonic()
            if tap:
                phone.sh(f"input tap {tap[0]} {tap[1]}")
            while time.monotonic() - start < a.seconds:
                asked = time.monotonic() - start
                png = phone.run("exec-out", "screencap", "-p", binary=True)
                got = time.monotonic() - start
                if not png.startswith(b"\x89PNG"):
                    raise CannotAnswer("screencap returned no PNG mid-burst")
                name = out / f"{int(asked * 1000):06d}ms.png"
                name.write_bytes(png)
                # A frame is somewhere between the request and its arrival; both ends are kept.
                frames.append({"file": name.name, "requestedMs": round(asked * 1000), "receivedMs": round(got * 1000)})
            (out / "frames.json").write_text(json.dumps({"tapped": tap, "frames": frames}, indent=2))
            return emit({"ok": True, "dir": str(out), "frames": len(frames), "tapped": tap})
        if a.command == "unlabeled":
            if not a.package:
                raise CannotAnswer("unlabeled needs --package")
            nodes = [n for n in phone.nodes() if n["package"] == a.package]
            clickable = [n for n in nodes if n["clickable"]]
            # A clickable wrapper often has no label of its own but holds the labeled text inside
            # it; a screen reader reads that text, so only a control with no label anywhere inside
            # its bounds counts as unnamed.
            def inside(outer, inner):
                return (outer["bounds"] and inner["bounds"] and outer["bounds"][0] <= inner["bounds"][0]
                        and outer["bounds"][1] <= inner["bounds"][1] and inner["bounds"][2] <= outer["bounds"][2]
                        and inner["bounds"][3] <= outer["bounds"][3])
            unnamed = [n for n in clickable if not (n["text"] or n["desc"])
                       and not any((m["text"] or m["desc"]) and inside(n, m) for m in nodes)]
            return emit({"ok": not unnamed, "clickable": len(clickable), "unnamed": unnamed}, 0 if not unnamed else 1)
        if a.command == "field":
            fields = [{"text": n["text"], "bounds": n["bounds"], "focused": n["focused"]}
                      for n in phone.nodes() if n["class"] == "android.widget.EditText"]
            return emit({"ok": True, "fields": fields})
        if a.command == "keys":
            if not a.value or not a.keymap:
                raise CannotAnswer("keys takes a file and --keymap MAP.json")
            text = Path(a.value).read_text(encoding="utf-8").rstrip("\n")
            keymap = json.loads(Path(a.keymap).read_text())
            missing = sorted(set(ch for ch in text if ch not in keymap))
            if not text or missing:
                raise CannotAnswer(f"the key map has no key for {missing!r}; nothing was typed")
            for ch in text:
                x, y = keymap[ch]
                phone.sh(f"input tap {int(x)} {int(y)}")
            return emit({"ok": True, "tapped": len(text)})
        if a.command == "type-file":
            if not a.value:
                raise CannotAnswer("type-file takes a file")
            text = Path(a.value).read_text(encoding="utf-8").rstrip("\n")
            if not text or not TYPEABLE.fullmatch(text):
                raise CannotAnswer("type-file types letters, digits, spaces and .,:!?/#=_+@- only; anything else is not typed reliably by `input text`")
            for ch in text:
                phone.sh("input text " + ("%s" if ch == " " else f"'{ch}'"))
            return emit({"ok": True, "typed": len(text), "calls": len(text)})
        if not a.value:
            raise CannotAnswer(f"{a.command} takes the text to look for")
        if a.command == "gone":
            started = time.monotonic()
            while any(shows(n, a.value, a.contains) for n in phone.nodes()):
                if time.monotonic() - started > a.timeout:
                    return emit({"ok": False, "still": a.value, "waitedS": round(time.monotonic() - started, 1)}, 1)
                time.sleep(0.4)
            return emit({"ok": True, "gone": a.value, "waitedS": round(time.monotonic() - started, 1)})
        started = time.monotonic()
        found = find(phone, a.value, 0 if a.command == "node" else a.timeout, a.contains)
        if not found:
            return emit({"ok": False, "missing": a.value, "waitedS": round(time.monotonic() - started, 1)}, 1)
        node = found[0]
        if a.command == "node":
            return emit({"ok": True, "nodes": found})
        if a.command == "tap":
            x, y = center(node)
            phone.sh(f"input tap {x} {y}")
            return emit({"ok": True, "tapped": a.value, "at": [x, y], "waitedS": round(time.monotonic() - started, 1)})
        if a.command == "swipe":
            if not (a.dx or a.dy):
                raise CannotAnswer("swipe needs --dx or --dy")
            x, y = center(node)
            phone.sh(f"input swipe {x} {y} {x + a.dx} {y + a.dy} 250")
            return emit({"ok": True, "swiped": a.value, "from": [x, y], "to": [x + a.dx, y + a.dy]})
        return emit({"ok": True, "shown": a.value, "node": node, "waitedS": round(time.monotonic() - started, 1)})
    except (CannotAnswer, physical.CannotAnswer) as e:
        return emit({"ok": False, "error": str(e)}, 2)
    except physical.Refused as e:
        return emit({"ok": False, "refused": str(e)}, 3)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
