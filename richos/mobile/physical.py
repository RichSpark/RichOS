#!/usr/bin/env python3
"""physical.py — what may touch a physical phone, and how (CEO, 2026-10-02).

His rule: only the build users get (release) ever goes on his wired phones, and every test on them
tests that build. His standing rule (2026-10-01): never wipe or reinstall the app; install over the
existing build only, keeping its data. On 2026-10-02 the wired Android phone ran a debuggable build
from 00:40Z to about 08:30Z; its screen sat blank 2.1-2.6 s per cold start against 0.25-0.34 s on the
release build, and he judged the app by it.

So a physical phone is touched ONLY through the two command lines, `randroid device ...` and
`rios device ...`, and this file holds their rules (both call it; nothing else should):

    physical.py android-build   --adb ADB --serial S        the installed build; never refused
    physical.py android-gate    --adb ADB --serial S        exit 0 (the installed build as JSON) or 3
    physical.py android-install --adb ADB --serial S --apk APK --aapt2 AAPT2
    physical.py android-record  --adb ADB --serial S --out FILE.mp4 [--seconds N]
    physical.py android-list    --adb ADB                   the wired phones (adb's list; none contacted)
    physical.py android-net     --adb ADB --serial S        READ ONLY: what the phone itself reaches;
                                                            exit 0 online, 1 not
    physical.py android-condition --adb ADB --serial S [--seconds N] [--out DIR]
                                                            READ ONLY: what automation left running or
                                                            changed on the phone, and its log flood
    physical.py ios-app APP                                 exit 0 for a Release bundle, 3 otherwise
    physical.py ios-automation-end --device ID              end iOS's AutomationModeUI and the stuck
                                                            input clients it keeps from every UI-test
                                                            session (see "the input client" below)
    physical.py hold --platform android|ios --phone ID [--wait S] [--holder NAME] -- <command>
                                                            run <command> holding that phone's lock
    physical.py status --platform android|ios --phone ID    free or held, and by whom (JSON)
    physical.py scan [--root DIR]                           the commit check (autocheck.py runs it)

One user per phone (CEO 2026-10-02): see "one user per phone" below. The lock records live in
resolved phone-locks state/<android-<sha256(serial)[:16]> | ios-iphone>.json.

The gate refuses: an emulator (it goes through `randroid emu`), and a phone whose installed
RichConnect is debuggable (`dumpsys package` pkgFlags DEBUGGABLE), naming the build it found. The
install refuses an APK whose manifest is debuggable (aapt2's own reading), installs with
`adb install -r` (the app's data kept), and never uninstalls: when the phone refuses (a build signed
by another key is installed), it says so and leaves the phone as it was, because replacing that
build means an uninstall, which wipes the app's data, and that is the CEO's decision. An emulator or
a simulator is never refused here; it has its own commands.

Exit codes: 0 done, 2 cannot answer (bad arguments, a tool missing), 3 refused, with the sentence.
"""
import argparse
import fcntl
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[0]))
import machine_storage as M

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "perf"))
import phone_changes  # noqa: E402 — what a killed run left changed on a phone
import test_copy  # noqa: E402 — the test copy's identity and the refusal of the CEO's own app

# Every command here that touches an Android phone works on the TEST COPY (dev.richos.connect.perf), installed beside
# the CEO's own RichConnect; his own app is never installed, read, opened or replaced by the tools (test_copy.py).
PACKAGE = test_copy.TEST_PACKAGE_ANDROID
REPO = HERE.parents[1]
# Set by the command line that owns physical phones (randroid / rios) for the tools it runs.
VERB_ENV = "RICHOS_DEVICE_VERB"
RULE = ("only the build users get (release) ever goes on a physical phone, and every test on one tests "
        "that build (CEO, 2026-10-02)")


class Refused(Exception):
    pass


class CannotAnswer(Exception):
    pass


def require_verb(cli):
    """The tools behind the two command lines refuse to touch a physical phone on their own."""
    if os.environ.get(VERB_ENV) != cli:
        raise Refused(f"a physical phone is touched only through `{cli} device ...`, which checks that the "
                      f"build on it is the release build: {RULE}")


# -- Android ----------------------------------------------------------------------------------

def adb(adb_path, serial, *args, binary=False, timeout=120):
    if not serial:
        raise CannotAnswer("no serial: a phone is addressed only by an explicit serial, never a bare adb")
    try:
        return subprocess.run([adb_path, "-s", serial, *args], capture_output=True, text=not binary, timeout=timeout)
    except FileNotFoundError:
        raise CannotAnswer(f"no adb at {adb_path}")
    except subprocess.TimeoutExpired:
        raise CannotAnswer(f"adb {' '.join(args)[:80]} did not answer in {timeout} s")


def put_back_leftovers(adb_path, serial):
    """Every `randroid device` command first undoes what a killed perf run left on this phone (night mode, a
    simulated unplug, system tracing; phone_changes.py). A phone that does not answer keeps its record."""
    def one(command):
        try:
            return adb(adb_path, serial, "shell", command, timeout=30).returncode == 0
        except CannotAnswer:
            return False
    return phone_changes.restore(serial, one, lambda s: print(s, file=sys.stderr, flush=True)) if serial else []


PROBE = (f"getprop ro.kernel.qemu; getprop ro.boot.qemu; echo '--richos--'; "
         f"dumpsys package {PACKAGE} | grep -E 'versionName=|versionCode=|pkgFlags=|lastUpdateTime='; true")


def probe(adb_path, serial):
    """One adb round trip: is it an emulator, and what RichConnect build is installed."""
    p = adb(adb_path, serial, "shell", PROBE, timeout=30)
    if p.returncode != 0 or "--richos--" not in p.stdout:
        raise CannotAnswer(f"{serial} did not answer (is it attached and authorized in `adb devices`?): "
                           f"{(p.stderr or p.stdout).strip()[:200]}")
    head, _, tail = p.stdout.replace("\r", "").partition("--richos--")
    emulator = "1" in head.split()
    flags = re.search(r"pkgFlags=\[([^\]]*)\]", tail)
    version = re.search(r"versionName=(\S+)", tail)
    code = re.search(r"versionCode=(\d+)", tail)
    updated = re.search(r"lastUpdateTime=(.+)", tail)
    installed = bool(flags or version)
    debuggable = bool(flags and "DEBUGGABLE" in flags.group(1).split())
    return {"serial": serial, "emulator": emulator, "package": PACKAGE, "installed": installed,
            "versionName": version.group(1) if version else None, "versionCode": int(code.group(1)) if code else None,
            "lastUpdateTime": updated.group(1).strip() if updated else None,
            "debuggable": debuggable if installed else None,
            "configuration": ("debug" if debuggable else "release") if installed else None}


def android_phones(adb_path):
    """The Android phones wired to this Mac, from adb's own list (no phone is contacted). A phone that
    is listed but not usable (unauthorized, offline) is listed with that state, never left out."""
    try:
        p = subprocess.run([adb_path, "devices"], capture_output=True, text=True, timeout=15)
    except FileNotFoundError:
        raise CannotAnswer(f"no adb at {adb_path}")
    except subprocess.TimeoutExpired:
        raise CannotAnswer("adb devices did not answer in 15 s")
    if p.returncode != 0:
        raise CannotAnswer(f"adb devices exited {p.returncode}: {(p.stderr or p.stdout).strip()[:200]}")
    rows = [line.split() for line in p.stdout.splitlines()[1:]]
    return [{"serial": r[0], "state": r[1]} for r in rows if len(r) >= 2 and not r[0].startswith("emulator-")]


# What the phone itself reaches (CLAUDE.md: before any claim about a phone's network, check its internet
# through the CLI and show the output). READ ONLY: the default network as the phone's own connectivity
# service holds it (its transport and whether Android VALIDATED it reaches the internet), one ping by
# address and one by name (the name needs the phone's DNS). Nothing is switched, reset or opened.
NET_READ = ("dumpsys connectivity | grep -E 'Active default network|NetworkAgentInfo'; echo '--richos--'; "
            "ping -c 2 -W 3 1.1.1.1 >/dev/null 2>&1; echo $?; ping -c 1 -W 5 www.google.com >/dev/null 2>&1; echo $?")


def net_reading(text):
    """(transport, validated, address reached, name reached) from NET_READ's output."""
    head, _, tail = text.replace("\r", "").partition("--richos--")
    default = re.search(r"Active default network:\s*(\d+)", head)
    transport = validated = None
    if default:
        for line in head.splitlines():
            if re.search(r"network\{%s\}" % default.group(1), line):
                found = re.search(r"Transports:\s*([A-Z_|]+)", line)
                transport = found.group(1) if found else None
                validated = "VALIDATED" in line
                break
    codes = tail.split()
    reached = [c == "0" for c in codes[:2]] + [None] * (2 - len(codes[:2]))
    return transport, validated, reached[0], reached[1]


def net(adb_path, serial):
    info = probe(adb_path, serial)
    physical(info)
    p = adb(adb_path, serial, "shell", NET_READ, timeout=60)
    if p.returncode != 0 or "--richos--" not in p.stdout:
        raise CannotAnswer(f"{serial} did not answer the network read: {(p.stderr or p.stdout).strip()[:200]}")
    transport, validated, by_address, by_name = net_reading(p.stdout)
    internet = bool(by_address and by_name)
    if internet:
        verdict = f"the phone reaches the internet over {transport or 'its default network'}"
    elif transport is None:
        verdict = "the phone has no default network: it reaches nothing"
    elif by_address and not by_name:
        verdict = f"the phone reaches an internet address over {transport} but cannot resolve a name (its DNS)"
    else:
        verdict = f"the phone is on {transport} but reaches no internet address"
    return {"serial": serial, "defaultTransport": transport, "validatedByAndroid": validated,
            "pingAddress": by_address, "pingName": by_name, "internet": internet, "verdict": verdict}


def physical(info):
    if info["emulator"]:
        raise Refused(f"{info['serial']} is an emulator: emulators go through `randroid emu ...`, never `randroid device`")


def gate(adb_path, serial):
    """The check every command on a physical phone passes first. Returns the installed build."""
    info = probe(adb_path, serial)
    physical(info)
    if info["debuggable"]:
        raise Refused(f"{serial} has a DEBUGGABLE RichConnect installed (versionName {info['versionName']}, "
                      f"pkgFlags DEBUGGABLE), so nothing on it would test the build users get: {RULE}. "
                      "Put the release build over it, data kept: `randroid build release`, then "
                      f"`randroid device --serial {serial} install`. If the phone refuses that install (the "
                      "debuggable build is signed by another key), replacing it means an uninstall, which wipes "
                      "the app's data: that is the CEO's decision, never a tool's")
    return info


def apk_badging(aapt2, apk):
    """(package name, debuggable) from aapt2's own reading of the APK's manifest."""
    if not os.path.isfile(apk):
        raise CannotAnswer(f"no APK at {apk}")
    try:
        p = subprocess.run([aapt2, "dump", "badging", apk], capture_output=True, text=True, timeout=120)
    except FileNotFoundError:
        raise CannotAnswer(f"no aapt2 at {aapt2}: whether {apk} is debuggable cannot be read, so it is not installed")
    if p.returncode != 0:
        raise CannotAnswer(f"aapt2 could not read {apk}: {(p.stderr or p.stdout).strip()[:200]}")
    name = re.search(r"^package: name='([^']*)'", p.stdout, re.M)
    lines = [line.strip() for line in p.stdout.splitlines()]
    return (name.group(1) if name else None), "application-debuggable" in lines


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def installed_sha(adb_path, serial):
    p = adb(adb_path, serial, "shell", f"pm path {PACKAGE}", timeout=30)
    paths = [l.split(":", 1)[1].strip() for l in p.stdout.split() if l.startswith("package:")]
    if len(paths) != 1:
        return None
    q = adb(adb_path, serial, "shell", f"sha256sum {paths[0]}", timeout=60)
    return (q.stdout.split() or [None])[0] if q.returncode == 0 else None


def install(adb_path, serial, apk, aapt2):
    """Release only, over the installed app, data kept, never an uninstall."""
    name, debuggable = apk_badging(aapt2, apk)
    if name in test_copy.CEO_APP_IDS:
        raise Refused(f"{apk} is {name}, the CEO's own RichConnect: he handles it himself. The tools install only the "
                      f"test copy ({PACKAGE}; `randroid device install` builds it)")
    if name != PACKAGE:
        raise Refused(f"{apk} is {name!r}, not the RichConnect test copy ({PACKAGE}); only its release build goes on the phone")
    if debuggable:
        raise Refused(f"{apk} is a DEBUGGABLE build (its manifest says so); {RULE}. Build the release APK "
                      "(`randroid build release`) and install that")
    info = probe(adb_path, serial)
    physical(info)
    want = sha256_file(apk)
    if info["installed"] and not info["debuggable"] and installed_sha(adb_path, serial) == want:
        return {"install": "skipped", "why": "these exact bytes are already installed", "sha256": want, "build": info}
    p = adb(adb_path, serial, "install", "-r", apk, timeout=600)
    if p.returncode != 0 or "Success" not in (p.stdout + p.stderr):
        raise Refused(f"the phone refused the install ({(p.stderr or p.stdout).strip()[-300:]}); NOTHING was "
                      "uninstalled and the app's data is untouched. If a build signed by another key is installed, "
                      "replacing it means an uninstall, which wipes the app's data: that is the CEO's decision")
    after = probe(adb_path, serial)
    got = installed_sha(adb_path, serial)
    if after["debuggable"] or got != want:
        raise Refused(f"after the install the phone holds {after['configuration']} bytes {str(got)[:12]}…, not the "
                      f"release APK {want[:12]}…")
    return {"install": "installed", "sha256": want, "build": after}


def record(adb_path, serial, out, seconds):
    """A screen recording of what the phone shows, through the gate first. Android's own
    `screenrecord` (at most 180 s), pulled to `out`, removed from the phone."""
    if not (1 <= seconds <= 180):
        raise CannotAnswer("--seconds must be 1 to 180 (Android's screenrecord limit)")
    if not out.endswith(".mp4"):
        raise CannotAnswer("--out must name an .mp4 file")
    info = gate(adb_path, serial)
    remote = "/sdcard/richos-device-record.mp4"
    try:
        p = adb(adb_path, serial, "shell", f"screenrecord --time-limit {int(seconds)} {remote}", timeout=seconds + 30)
        if p.returncode != 0:
            raise CannotAnswer(f"screenrecord failed: {(p.stderr or p.stdout).strip()[:200]}")
        Path(out).parent.mkdir(parents=True, exist_ok=True)
        q = adb(adb_path, serial, "pull", remote, out, timeout=120)
        if q.returncode != 0 or not os.path.isfile(out):
            raise CannotAnswer(f"could not copy the recording off the phone: {(q.stderr or q.stdout).strip()[:200]}")
    finally:
        adb(adb_path, serial, "shell", f"rm -f {remote}", timeout=30)
    return {"recording": out, "seconds": seconds, "bytes": os.path.getsize(out), "build": info}


# -- the phone's condition: what automation left behind (CEO 2026-10-04) --------------------------
# On the iPhone, every automated session left a stuck input client behind until a reboot, and the
# phone slowed down. This is the Android read of the same question: what our tools left running or
# changed that a person would not, and the phone's log flood. READ ONLY: every command below is a
# read (`settings get`, never put; `wm size` with no argument; `logcat` without -c; `dumpsys` with no
# reset flag); `read_only` checks each command before it runs, and mobile-device.test.py (D34) checks every call. Nothing is fixed,
# reset, killed or restarted here; the report says what it found, and a person decides. The CEO's own
# app is only asked whether it runs (`pidof`), never read beyond that. A debuggable test copy is
# REPORTED, not refused: seeing the phone's state is never refused (as `device build`).

CONDITION_SETTINGS = [  # (namespace, key, the value a phone has when nobody changed it)
    ("global", "window_animation_scale", "1.0"), ("global", "transition_animation_scale", "1.0"),
    ("global", "animator_duration_scale", "1.0"), ("global", "stay_on_while_plugged_in", "0"),
    ("system", "show_touches", "0"), ("system", "pointer_location", "0"),
    ("global", "always_finish_activities", "0"), ("global", "debug_view_attributes", "0"),
    ("global", "debug_app", None), ("global", "wait_for_debugger", "0"),
    ("secure", "enabled_accessibility_services", None), ("secure", "accessibility_enabled", None),
    ("secure", "touch_exploration_enabled", "0"), ("secure", "default_input_method", None),
    ("secure", "enabled_input_methods", None), ("system", "screen_off_timeout", None),
    ("global", "development_settings_enabled", None), ("global", "adb_enabled", None),
    ("secure", "ui_night_mode", None),
]
CONDITION_PROPS = ["debug.hwui.profile", "debug.hwui.overdraw", "debug.layout", "debug.hwui.show_dirty_regions",
                   "persist.sys.ui.hw", "debug.atrace.tags.enableflags", "debug.atrace.app_cmdlines",
                   "debug.atrace.app_number", "log.tag", "persist.log.tag"]
# Files our tools put on the phone and remove when they end normally (a UI dump, a recording, a trace,
# the benchmark's binaries): one left behind says a run ended early.
LEFTOVER_FILES = "richos|qa-phone|randroid|review-walk|emu-ui"
CONDITION_DUMP_KEYS = [k for _, k, _ in CONDITION_SETTINGS if k not in ("default_input_method", "enabled_input_methods")]
INPUT_WINDOW = re.compile(r"(?i)input|window|viewroot|surface|choreographer|anr|blast|focus")
SHELL_USER = "shell"


def condition_reads(seconds):
    """name -> one read-only shell command (each is checked by `read_only` before it runs)."""
    reads = {f"setting:{ns}:{key}": f"settings get {ns} {key}" for ns, key, _ in CONDITION_SETTINGS}
    reads.update({f"prop:{p}": f"getprop {p}" for p in CONDITION_PROPS})
    reads.update({
        "clock": "date +%s; cat /proc/uptime; cat /proc/loadavg",
        "processes": "ps -A -o PID,PPID,USER,ETIME,TIME,NAME,ARGS",
        "packages": f"cmd package list packages -U {PACKAGE}",
        "bench-package": "cmd package list packages dev.richos.bench",
        "files": f"ls -la /sdcard /data/local/tmp | grep -E '{LEFTOVER_FILES}'",
        "tracing": "cat /sys/kernel/tracing/tracing_on; cat /sys/kernel/debug/tracing/tracing_on",
        "wm-size": "wm size", "wm-density": "wm density", "night": "cmd uimode night",
        "battery": "dumpsys battery",
        "batterystats-reset": "dumpsys batterystats | grep -m 3 -E 'RESET:TIME|Time on battery'",
        "settings-history": "dumpsys settings | grep -E '" + "|".join(CONDITION_DUMP_KEYS) + "'",
        "ime": "ime list -s",
        "accessibility": "dumpsys accessibility | grep -i -E 'uiautomation|bound services|enabled services'",
        "instrumentation": "dumpsys activity processes | grep -i -E 'instrumentation|instr='",
        "input": "dumpsys input",
        "power": "dumpsys power | grep -i -E 'wake_lock|wake locks|stay_on|mStayOn'",
        "test-copy": f"echo pid:; pidof {PACKAGE}; echo services:; dumpsys activity services {PACKAGE} | "
                     f"grep -E 'ServiceRecord|app=|isForeground'; echo jobs:; dumpsys jobscheduler {PACKAGE} | grep -E 'JOB #'; "
                     f"echo alarms:; dumpsys alarm | grep -F '{PACKAGE}'",
        "ceo-app-runs": f"pidof {test_copy.CEO_APP_ANDROID}",
        "front": "dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'",
        # Every line written in the next `seconds`: the phone's own clock marks the start, the phone
        # sleeps, then one dump (`logcat -d`, which ends on its own: no reader is left behind) of the
        # lines since that mark. (A streamed logcat ended by `timeout` lost its buffered output on the
        # Honor: 0 lines in 30 s, measured 2026-10-04.)
        "log": f"t=$(date +%s); echo since $t; sleep {int(seconds)}; logcat -d -v epoch -b main,system,crash -T \"$t.000\"",
        # Then the whole buffer as it stands (its sizes first): the same count over the hours it holds,
        # and whether it still reaches back to the window's start (if not, the window lost lines).
        "log-buffer": "logcat -g -b main,system,crash; logcat -d -v epoch -b main,system,crash",
    })
    return reads


# Each piece of a read, split at ; | and &&, must be one of these, and nothing in a dumpsys may name
# a flag that changes the phone (`dumpsys battery unplug`, `dumpsys batterystats --reset`).
READ_SEGMENT = re.compile(
    r"^(settings get (global|secure|system) \w+|getprop [\w.]+|date \+%s|cat /proc/(uptime|loadavg)"
    r"|ps -A -o [A-Z,]+|cmd package list packages( -U)? [\w.]+|wm (size|density)|cmd uimode night|ime list -s"
    r"|ls -la( /[\w/]+)+|cat /sys/kernel(/debug)?/tracing/tracing_on"
    r"|pidof [\w.]+|grep( -[a-zA-Z]+| \d+)* '[^']*'|t=\$\(date \+%s\)|echo [\w:]+|echo since \$t|sleep \d+"
    r"|logcat -d -v epoch -b [a-z,]+ -T \"\$t\.000\"|logcat -d -v epoch -b [a-z,]+|logcat -g -b [a-z,]+"
    r"|dumpsys (battery|batterystats|settings|accessibility|activity processes|activity services [\w.]+"
    r"|jobscheduler [\w.]+|alarm|input|power|window))$")
WRITES = re.compile(r"logcat\b.*\s-[cG]\b|--reset|\bunplug\b|\breset\b|\bset\b|\bput\b|--enable|--disable|\blogcat\b.*\s-c\b|\bkill\b")


def read_only(cmd):
    """True when every piece of `cmd` is a known read and nothing in it names a write."""
    masked = re.sub(r"'[^']*'", "'Q'", cmd)  # a grep pattern's own | and ; are not separators
    pieces = [x.strip() for x in re.split(r";|&&|\|", masked) if x.strip()]
    return bool(pieces) and all(READ_SEGMENT.match(x) for x in pieces) and not WRITES.search(masked)


def parse_ps(text):
    rows = []
    for line in text.replace("\r", "").splitlines()[1:]:
        parts = line.split(None, 6)
        if len(parts) < 6 or not parts[0].isdigit():
            continue
        rows.append({"pid": int(parts[0]), "ppid": int(parts[1]) if parts[1].isdigit() else None, "user": parts[2],
                     "elapsed": parts[3], "cpu": parts[4], "name": parts[5], "args": parts[6] if len(parts) > 6 else ""})
    return rows


def leftovers(rows):
    """Processes an adb shell started and left: adbd is among their ancestors (what `adb shell` runs
    is adbd's child), or a test tool started through app_process (uiautomator, monkey, am
    instrument's runner) whoever started it. adbd itself and this read's own `ps` and its shell are
    not counted; the phone's own daemons that run as the shell user but were started by init at boot
    are listed apart (`shellUserNotFromAdb`), never as ours."""
    by_pid = {r["pid"]: r for r in rows}
    own = {r["pid"] for r in rows if r["args"].startswith("ps -A ")}
    own |= {r["ppid"] for r in rows if r["pid"] in own and by_pid.get(r["ppid"], {}).get("name") != "adbd"}

    def from_adb(r):
        seen, pid = set(), r["ppid"]
        while pid and pid not in seen and pid in by_pid:
            seen.add(pid)
            if by_pid[pid]["name"] == "adbd":
                return True
            pid = by_pid[pid]["ppid"]
        return False

    found, other = [], []
    for r in rows:
        if r["pid"] in own or r["name"] == "adbd":
            continue
        tool = re.search(r"uiautomator|com\.android\.commands\.\S+|\bmonkey\b|screenrecord", r["args"] + " " + r["name"])
        parent = by_pid.get(r["ppid"], {}).get("name")
        if from_adb(r) or tool:
            found.append({**r, "parent": parent, "why": ("started over adb (adbd is an ancestor)" if from_adb(r) else "")
                          + (f"; a test tool ({tool.group(0)})" if tool else "")})
        elif r["user"] == SHELL_USER:
            other.append({**r, "parent": parent})
    return found, other


LOG_LINE = re.compile(r"^\s*(\d+\.\d+)\s+(\d+)\s+(\d+)\s+([VDIWEFA])\s+(.*?)\s*:\s(.*)$")


def normalize(message):
    m = re.sub(r"0x[0-9a-fA-F]+", "0x#", message)
    m = re.sub(r"\b[0-9a-fA-F]{6,}\b", "#", m)
    return re.sub(r"\d+", "#", m).strip()[:200]


def log_flood(text, seconds, names, top=15):
    """A log read: total lines and rate, the most repeated lines (digits and ids folded, so one
    message repeated with different numbers is one row), each with its rate and the processes that
    wrote it, and the share from the input and window system. `seconds` None: the rate is over the
    span the lines cover (the whole buffer's read)."""
    from collections import Counter, defaultdict
    counts, pids, sample = Counter(), defaultdict(set), {}
    total = errors = input_window = 0
    since, first, last = None, None, None
    for line in text.replace("\r", "").splitlines():
        mark = re.match(r"^since (\d+)$", line.strip())
        if mark:
            since = int(mark.group(1))
            continue
        m = LOG_LINE.match(line)
        if not m:
            continue
        total += 1
        ts, pid, _, prio, tag, msg = m.groups()
        first = float(ts) if first is None else min(first, float(ts))
        last = float(ts) if last is None else max(last, float(ts))
        errors += prio in "EF"
        input_window += bool(INPUT_WINDOW.search(tag))
        key = (prio, tag, normalize(msg))
        counts[key] += 1
        pids[key].add(int(pid))
        sample.setdefault(key, msg[:200])
    window = seconds if seconds is not None else (round(last - first, 1) if first is not None else None)
    rate = lambda n: round(n / window, 3) if window else None
    who = lambda key: sorted({names.get(pid, f"pid {pid} (ended)") for pid in pids[key]})[:5]
    rows = [{"count": n, "perSecond": rate(n), "priority": k[0], "tag": k[1], "line": sample[k], "folded": k[2],
             "processes": who(k)} for k, n in counts.most_common(top)]
    top_ew = [{"count": n, "perSecond": rate(n), "priority": k[0], "tag": k[1], "line": sample[k], "processes": who(k)}
              for k, n in counts.most_common() if k[0] in "WEF"][:top]
    return {"windowSeconds": window, "since": since, "first": first, "last": last,
            "lines": total, "perSecond": rate(total), "errorLines": errors,
            "inputWindowLines": input_window, "inputWindowPerSecond": rate(input_window), "top": rows,
            "topWarningsAndErrors": top_ew}


def input_state(text):
    """`dumpsys input`: the dispatcher's connections and monitors. A connection that is not NORMAL
    or not responsive, and the same channel registered more than once, are the Android form of the
    iPhone's stuck input clients."""
    from collections import Counter
    conns, monitors, anr, section = [], [], [], None
    now, _, last_anr = text.replace("\r", "").partition("Input Dispatcher State at time of last ANR:")
    for line in now.splitlines():
        s = line.strip()
        if re.match(r"(Global|Gesture) monitors", s) or s.startswith("Monitors"):
            section = "monitors"
        elif s.startswith("Connections:"):
            section = "connections"
        elif s and not s[0].isdigit() and line[:1] not in (" ", "\t"):
            section = None
        if "channelName=" in s and section == "connections":
            conns.append(s)
        elif section == "monitors" and re.match(r"\d+:", s):
            monitors.append(s)
        if re.search(r"\bANR\b|not responding|unresponsive", s, re.I):
            anr.append(s[:300])
    name = lambda c: re.sub(r"\b[0-9a-f]{6,}\b", "#", (re.search(r"channelName='([^']*)'", c) or [None, c])[1])
    dup = {k: n for k, n in Counter(name(c) for c in conns).items() if n > 1}
    bad = [c[:300] for c in conns if "responsive=false" in c or not re.search(r"status=NORMAL", c)]
    last = [x.strip()[:300] for x in last_anr.splitlines() if re.match(r"\s*(Time|Reason):", x)][:2]
    return {"connections": len(conns), "notNormalOrUnresponsive": bad, "duplicateChannels": dup,
            "monitors": [m[:300] for m in monitors], "anrLines": anr[:20], "lastAnr": last}


def condition(adb_path, serial, seconds=30, out_dir=None):
    if not (5 <= seconds <= 120):
        raise CannotAnswer("--seconds must be 5 to 120 (the log is read for that long)")
    info = probe(adb_path, serial)
    physical(info)
    raw = {}
    reads = condition_reads(seconds)
    unsafe = [cmd for cmd in reads.values() if not read_only(cmd)]
    if unsafe:
        raise CannotAnswer(f"not a read, so not run: {unsafe[0][:160]}")
    for name, cmd in reads.items():
        p = adb(adb_path, serial, "shell", cmd, timeout=seconds + 60 if name == "log" else 60)
        raw[name] = (p.stdout or "").replace("\r", "")
        if (p.stderr or "").strip():
            raw[name + ".stderr"] = p.stderr
    get = lambda k: raw.get(k, "").strip()
    settings = []
    for ns, key, default in CONDITION_SETTINGS:
        value = get(f"setting:{ns}:{key}")
        value = None if value in ("", "null") else value
        changed = default is not None and value is not None and _num(value) != _num(default)
        settings.append({"namespace": ns, "key": key, "value": value, "untouched": default, "changed": changed})
    props = {p: get(f"prop:{p}") or None for p in CONDITION_PROPS}
    rows = parse_ps(raw["processes"])
    names = {r["pid"]: f"{r['name']} ({r['user']})" for r in rows}
    left, other = leftovers(rows)
    window = {**log_flood(raw["log"], seconds, names), "error": raw.get("log.stderr", "").strip()[:300] or None}
    buffer = log_flood(raw.get("log-buffer", ""), None, names)
    # The buffer no longer reaching back to the window's start means lines written in the window were
    # pushed out before the dump read them: the window's count is then a floor.
    window["bufferTurnedOver"] = bool(window["since"] and buffer["first"] and buffer["first"] > window["since"])
    clock = get("clock").split()
    battery = get("battery")
    overrides = {k: get(k) for k in ("wm-size", "wm-density")}
    result = {
        "build": info,
        "phoneClock": {"epoch": int(clock[0]) if clock and clock[0].isdigit() else None,
                       "uptimeHours": round(float(clock[1]) / 3600, 2) if len(clock) > 1 else None,
                       "loadavg": " ".join(clock[3:6]) if len(clock) > 5 else None},
        "leftoverProcesses": left,
        "shellUserNotFromAdb": other,
        "adbd": [{k: r[k] for k in ("pid", "elapsed", "args")} for r in rows if r["name"] == "adbd"],
        "processCount": len(rows),
        "settings": settings,
        "settingsChanged": [f"{s['namespace']} {s['key']}={s['value']}" for s in settings if s["changed"]],
        "settingsHistory": get("settings-history").splitlines()[:80],
        "debugProperties": {k: v for k, v in props.items() if v},
        "displayOverrides": {k: v for k, v in overrides.items() if "Override" in v},
        "display": overrides,
        "nightMode": get("night"),
        "batteryOverridden": "UPDATES STOPPED" in battery,
        "battery": battery.splitlines()[:12],
        "batterystatsReset": get("batterystats-reset").splitlines(),
        "inputMethodsEnabled": get("ime").splitlines(),
        "accessibility": get("accessibility").splitlines()[:30],
        "instrumentation": get("instrumentation").splitlines()[:20],
        "input": input_state(raw["input"]),
        "wakeLocks": get("power").splitlines()[:60],
        "leftoverFiles": get("files").splitlines()[:40],
        "benchmarkAppInstalled": get("bench-package") or None,
        "kernelTracingOn": get("tracing").splitlines(),
        "testCopy": {"package": PACKAGE, "uid": get("packages") or None, "detail": get("test-copy").splitlines()[:40]},
        "ceoAppRunning": bool(get("ceo-app-runs")),
        "front": get("front").splitlines(),
        "log": window,
        "logBuffer": {**{k: v for k, v in buffer.items() if k != "since"},
                      "sizes": [x.strip() for x in raw.get("log-buffer", "").splitlines() if "ring buffer" in x]},
        "logLevel": {k: props.get(k) for k in ("log.tag", "persist.log.tag")},
        # what a run recorded on the Mac as changed on this phone and never put back (phone_changes.py); listed, not undone
        "pendingPhoneChanges": [
            f"{r.get('kind')}: recorded {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(r['at']))}"
            if isinstance(r.get("at"), (int, float)) else f"{r.get('kind')}: recorded at an unknown time"
            for r in phone_changes.pending(serial)],
    }
    if out_dir:
        Path(out_dir).mkdir(parents=True, exist_ok=True)
        for name, text in raw.items():
            if name == "ceo-app-runs":
                continue
            (Path(out_dir) / (re.sub(r"[^\w.-]+", "_", name) + ".txt")).write_text(text)
        (Path(out_dir) / "condition.json").write_text(json.dumps(result, indent=2))
        result["rawDir"] = out_dir
    return result


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return v


# -- one user per phone -------------------------------------------------------------------------
# CEO, 2026-10-02: two agents used the iPhone at once and broke each other's runs twice. Every
# command of the two verbs runs inside `hold`: an exclusive lock per physical phone for the whole
# use (a perf series, a recording, a UI-test session: the command's full length, never per adb
# call; `randroid device hold -- <script>` keeps it across a walker's many commands). The lock is
# the kernel's (flock), so it is released however the holder ends, a crash or a kill included; the
# record beside it says who holds it (teammate, process, start, command). A second caller is refused
# at once with the holder's name, or waits up to --wait seconds, saying so. A holder whose process
# is gone left its lock released; the next caller says so and takes it. Queried by `status`.
# The iPhone lock is one for every spelling of the phone (hardware UDID, CoreDevice UUID): the Mac
# has one wired iPhone, and two spellings must never be two locks.

LOCK_DIR = "phone-locks"
HOLD_ENV = "RICHOS_DEVICE_HOLD"


def lock_dir():
    try:
        return str(M.state(LOCK_DIR, "RICHOS_PHONE_LOCK_DIR"))
    except (ValueError, OSError) as error:
        raise CannotAnswer(str(error)) from error


def lock_paths(platform, phone):
    if platform not in ("android", "ios"):
        raise CannotAnswer("--platform is android or ios")
    if platform == "android" and not phone:
        raise CannotAnswer("an Android phone's lock is named by its serial")
    name = "ios-iphone" if platform == "ios" else "android-" + hashlib.sha256(phone.encode()).hexdigest()[:16]
    base = os.path.join(lock_dir(), name)
    return name, base + ".lock", base + ".json"


def phone_label(platform, phone):
    return "iPhone" if platform == "ios" else f"Android phone {phone}"


def holder_name():
    if os.environ.get("RICHOS_DEVICE_HOLDER"):
        return os.environ["RICHOS_DEVICE_HOLDER"]
    branch = subprocess.run(["git", "rev-parse", "--abbrev-ref", "HEAD"], capture_output=True, text=True).stdout.strip()
    return f"{branch or 'unknown'} in {os.getcwd()}"


def read_record(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def write_record(path, record):
    tmp = f"{path}.{os.getpid()}.tmp"
    with open(tmp, "w") as f:
        json.dump(record, f, indent=1)
    os.replace(tmp, path)


def alive(pid):
    try:
        os.kill(int(pid), 0)
        return True
    except ProcessLookupError:
        return False
    except (PermissionError, ValueError, TypeError):
        return True


def ancestors():
    pids, pid = set(), os.getppid()
    for _ in range(64):
        if pid <= 1 or pid in pids:
            break
        pids.add(pid)
        out = subprocess.run(["ps", "-o", "ppid=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
        pid = int(out) if out.isdigit() else 0
    return pids


def inside_hold(name):
    """True when a `hold` of this phone is an ancestor of this process: one session, many commands."""
    held, _, pid = os.environ.get(HOLD_ENV, "").rpartition(":")
    return held == name and pid.isdigit() and int(pid) in ancestors()


def now_iso():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def status(platform, phone):
    """free or held, and by whom. Reads the record; tests the kernel lock only when the record says
    held and its process lives (a reused process id), and then only for an instant."""
    name, lock, record_path = lock_paths(platform, phone)
    rec = read_record(record_path) or {}
    out = {"phone": phone_label(platform, phone), "lock": lock, "record": record_path}
    if not rec or rec.get("endedAt") or not alive(rec.get("pid")):
        if rec and not rec.get("endedAt"):
            out["previous"] = {**rec, "note": "its process is gone; the kernel released its lock"}
        elif rec:
            out["previous"] = rec
        return {"state": "free", **out}
    fd = os.open(lock, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        return {"state": "free", **out, "previous": {**rec, "note": "its process id lives on, but it holds no lock"}}
    except BlockingIOError:
        return {"state": "held", **out, **{k: rec.get(k) for k in ("holder", "pid", "startedAt", "command")}}
    finally:
        os.close(fd)


def hold(platform, phone, cmd, wait_s=0, holder=None, say=lambda s: print(s, file=sys.stderr, flush=True),
         net_check=False):
    """Run `cmd` holding the phone's lock for its whole length; returns its exit code.

    `net_check` (iOS, from `rios device verify|run|perf|hold`): the phone must be able to reach Apple
    before the command starts and is checked again after it, however it ended (phone_net.py: CEO,
    2026-10-02, "Unable to Verify App" again). Refused up front with one sentence; a run that left the
    phone's network off gets it put back, and exits 4 if it cannot be."""
    if not cmd:
        raise CannotAnswer("hold runs a command: hold ... -- <command>")
    name, lock, record_path = lock_paths(platform, phone)
    if inside_hold(name):
        return subprocess.call(cmd)
    os.makedirs(os.path.dirname(lock), mode=0o700, exist_ok=True)
    fd = os.open(lock, os.O_RDWR | os.O_CREAT, 0o600)
    started, said = time.monotonic(), False
    while True:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            break
        except BlockingIOError:
            who = read_record(record_path) or {}
            waited = time.monotonic() - started
            line = (f"{who.get('holder', 'an unknown holder')} (process {who.get('pid')}, since {who.get('startedAt')}: "
                    f"{who.get('command')})")
            if waited >= wait_s:
                os.close(fd)
                raise Refused(f"the {phone_label(platform, phone)} is in use by {line}; two users of one phone break each "
                              f"other's runs (CEO 2026-10-02). " + (f"Waited {waited:.0f} s. " if wait_s else "") +
                              "Run it again when it is free (`device status` says), or pass --wait SECONDS")
            if not said:
                say(f"waiting up to {wait_s:.0f} s for the {phone_label(platform, phone)}, held by {line}")
                said = True
            time.sleep(min(1.0, max(0.05, wait_s - waited)))
    waited = round(time.monotonic() - started, 1)
    previous = read_record(record_path)
    record = {"holder": holder or holder_name(), "pid": os.getpid(), "startedAt": now_iso(), "command": " ".join(cmd)[:300],
              "platform": platform, "waitedSeconds": waited}
    if previous and not previous.get("endedAt"):
        record["reclaimedFrom"] = {k: previous.get(k) for k in ("holder", "pid", "startedAt", "command")}
        say(f"the previous holder of the {phone_label(platform, phone)}, {previous.get('holder')} (process "
            f"{previous.get('pid')}), ended without releasing it; the kernel released its lock and it is taken now")
    if said:
        say(f"the {phone_label(platform, phone)} is free after {waited} s; taken")
    write_record(record_path, record)
    net_check = net_check and platform == "ios"
    # The phone the command names (`--device ID`, as perf and the speed watch pass it), else
    # RICHOS_IOS_DEVICE: on 2026-10-03 every speed-watch run skipped this check ("names no phone").
    named = [cmd[i + 1] for i in range(len(cmd) - 1) if cmd[i] == "--device"]
    device = os.environ.get("RICHOS_IOS_DEVICE") or (named[0] if named else None)
    if net_check:
        import phone_net
        try:
            phone_net.preflight(say, device)
        except phone_net.Unreachable as error:
            record.update(endedAt=now_iso(), exit=3, refused=str(error)[:300])
            write_record(record_path, record)
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)
            raise Refused(str(error))
        # A run killed outright (its own session end never ran) leaves AutomationModeUI's stuck input
        # clients behind: they go before this command touches the phone, so nothing measures a clogged
        # phone. Refused when that cannot be done, like the network above.
        try:
            automation_ui_ended(say, device, "before the run")
        except CannotAnswer as error:
            record.update(endedAt=now_iso(), exit=3, refused=str(error)[:300])
            write_record(record_path, record)
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)
            raise Refused(f"{error}; nothing was run (the phone may be carrying stuck input clients from earlier "
                          "UI-test sessions, which make any timing on it wrong)")
    child = subprocess.Popen(cmd, env={**os.environ, HOLD_ENV: f"{name}:{os.getpid()}"})
    forward = lambda signum, _frame: child.send_signal(signum)
    previous_handlers = {s: signal.signal(s, forward) for s in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT)}
    code = None
    left = None
    try:
        code = child.wait()
    finally:
        for s, h in previous_handlers.items():
            signal.signal(s, h)
        if net_check:
            try:
                automation_ui_ended(say, device, "after the run")
            except CannotAnswer as error:
                say(f"{error}; the next `rios device` run ends it before it starts")
            left = phone_net.postflight(say, device)
        record.update(endedAt=now_iso(), exit=code)
        write_record(record_path, record)
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)
    if left:
        say(left)
        return code if code else 4
    return code


# -- iOS --------------------------------------------------------------------------------------

def ios_configuration(app):
    """'release', 'debug' or None, by the development markers `rios sim check-release` proves: every
    Debug bundle carries all of them, a Release bundle none (perf/ios.py build_configuration)."""
    sys.path.insert(0, str(HERE / "perf"))
    import ios  # noqa: E402 — perf/ios.py, the one definition of the markers
    return ios.build_configuration(app)


def ios_app(app):
    if not os.path.isdir(app):
        raise CannotAnswer(f"no app bundle at {app}")
    found = ios_configuration(app)
    if found != "release":
        raise Refused(f"{app} is {'a Debug' if found == 'debug' else 'not a Release'} bundle (its development "
                      f"markers say so); {RULE}")
    return {"app": app, "configuration": found}


# -- iOS: the input client every UI-test session leaves behind (2026-10-04) --------------------
# Each XCUITest session on the iPhone turns on iOS's Automation Mode, and its process AutomationModeUI
# opens an input (HID) connection to backboardd that it never reads again once the session is over.
# backboardd then flags that client unresponsive on every motion event; they pile up one per session
# until a reboot (23 on the morning of 2026-10-04: about 8,000 log lines a second and a stuttering
# phone that failed the speed check). Measured on the test iPhone the same day: a session that ends
# normally (xcodebuild exit 0) still leaves one, 0 -> 1 about a minute later; AutomationModeUI ignores
# SIGTERM; ending it with SIGKILL closes every connection it holds (4 -> 0 and 1 -> 0, still 0 80 s
# later), and iOS starts it again for the next session, which passes. So every UI-test session on the
# phone ends with AutomationModeUI ended (physical-device.mjs), and so does every hold of the phone,
# before and after its command, whatever that command did. Nothing else on the phone is touched.
AUTOMATION_UI = "/System/Library/PrivateFrameworks/AutomationMode.framework/AutomationModeUI.app/AutomationModeUI"


def _devicectl(args, device, runner, timeout=120):
    """devicectl's JSON `result` for one call on the phone; CannotAnswer when it gives none."""
    with tempfile.TemporaryDirectory(prefix="richos-devicectl-") as tmp:
        out = os.path.join(tmp, "out.json")
        try:
            p = runner(["xcrun", "devicectl", *args, "--device", device, "--json-output", out],
                       capture_output=True, text=True, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as e:
            raise CannotAnswer(f"devicectl {' '.join(args)} did not answer: {e}")
        if p.returncode != 0 or not os.path.exists(out):
            raise CannotAnswer(f"devicectl {' '.join(args)} failed: {((p.stderr or '') + (p.stdout or '')).strip()[-200:]}")
        with open(out) as f:
            return json.load(f).get("result") or {}


def automation_ui_pids(rows):
    """The pids of iOS's AutomationModeUI in devicectl's `runningProcesses`, matched by its exact path."""
    return sorted(int(r["processIdentifier"]) for r in rows
                  if str(r.get("executable", "")).endswith(AUTOMATION_UI) and r.get("processIdentifier") is not None)


def end_automation_ui(device, runner=subprocess.run):
    """End AutomationModeUI on the iPhone (SIGKILL through devicectl; it ignores SIGTERM) and check it is
    gone. Returns {"ended": [pids]} ([] when it was not running). CannotAnswer when it cannot be listed or
    is still running afterwards: the phone would keep its stuck input clients."""
    if not device:
        raise CannotAnswer("no phone named (--device or RICHOS_IOS_DEVICE), so AutomationModeUI could not be ended")
    listing = lambda: automation_ui_pids(_devicectl(["device", "info", "processes"], device, runner).get("runningProcesses") or [])
    pids = listing()
    for pid in pids:
        try:
            _devicectl(["device", "process", "terminate", "--pid", str(pid), "--kill"], device, runner)
        except CannotAnswer:
            pass  # gone already, or refused: the listing below decides
    left = listing() if pids else []
    if set(left) & set(pids):
        raise CannotAnswer(f"AutomationModeUI (pid {', '.join(map(str, sorted(set(left) & set(pids))))}) is still "
                           "running after SIGKILL; its stuck input clients stay on the phone until it restarts")
    return {"ended": pids}


def automation_ui_ended(say, device, when):
    """The hold's step `when` ("before the run" / "after the run"): end AutomationModeUI and say so when it
    was running. Without a named phone it is skipped and said, as the network check is."""
    if not device:
        say(f"AutomationModeUI not checked {when}: RICHOS_IOS_DEVICE names no phone")
        return None
    ended = end_automation_ui(device)
    if ended["ended"]:
        say(f"{when}: ended iOS's AutomationModeUI (pid {', '.join(map(str, ended['ended']))}) and with it the stuck "
            "input clients it keeps from UI-test sessions")
    return ended


# -- the commit check ---------------------------------------------------------------------------
# A file outside the two command lines must not install on, uninstall from or wipe a phone. The
# command lines may install (release only, checked at run time above) and may never uninstall or
# clear the app's data either. A line can be excused only by `device-cli-exempt: <reason>` on it,
# where a reviewer sees it (a test that plants a forbidden command to prove a refusal).

CLI_FILES = {
    "richos/mobile/native-android/bin/randroid": "the Android command line",
    "richos/mobile/native-android/bin/apk-install.sh": "randroid emu's installer, for its recorded emulator only",
    "richos/mobile/physical.py": "randroid device install: refuses a debuggable APK, installs with install -r",
    "richos/mobile/perf/android.py": "randroid device seed/perf: the debuggable twin installed with install -r and "
                                     "replaced by the release build in the same call",
    "richos/mobile/perf/bench/bench.py": "randroid device bench: installs and removes only its own empty benchmark "
                                         "app (dev.richos.bench.empty); RichConnect is never touched",
    "richos/mobile/perf/perf.py":"randroid device perf/seed and rios device perf; refuses a phone without them",
    "richos/mobile/native-ios/bin/rios": "the iPhone command line",
    "richos/mobile/native-ios/Tools/physical-device.mjs": "rios device: Release products only, never removed",
}
NOT_SCANNED = {
    "richos/mobile/physical.py": "this check's own patterns; its own install is the gated one above",
    "richos/engine/reference/": "reference examples from another product, marked not wired and not runnable as-is",
    "docs/verification/": "evidence of past runs: a record, not a path anything runs",
    "richos/engine/docs/verification/": "evidence of past runs: a record, not a path anything runs",
}
TEXT = {".py", ".sh", ".mjs", ".js", ".cjs", ".ts", ".swift", ".kt", ".kts", ".gradle", ".md", ".yml", ".yaml",
        ".toml", ".bash", ".zsh", ""}
EXEMPT = re.compile(r"device-cli-exempt:\s*\S.{8,}")
# A line that asserts the thing does NOT happen (a test of a refusal) is not a path that does it.
ABSENCE = re.compile(r"^\s*assert\b.*\bnot\b|assert\.doesNotMatch|assert\.ok\(\s*!")
CHEAP = re.compile(r"install|Debug-iphoneos|connected|ios-deploy|platform=iOS,|pm\s+clear")
ADB = r"(?<![\w-])adb[\"']?(?:\s+-s\s+\S+)?\s+(?:shell\s+)?(?:pm\s+)?"
# An argument list ("install", apk) counts only on a line that is about the phone's adb.
ADB_CONTEXT = re.compile(r"(?<![\w-])adb(?![\w-])|apk|PACKAGE|dev\.richos|\b(?:dev|phone|device)\.(?:run|sh)\(")
QUOTED = re.compile(r"[\"'](uninstall|install|install-multiple)[\"']")
REMOVES = "removes the app or its data"
INSTALLS = "installs on a phone outside randroid device / rios device"
# (name, pattern, applies inside the command lines too)
RULES = [
    (REMOVES, re.compile(
        ADB + r"(uninstall|clear)\b"
        r"|\bpm\s+(uninstall|clear)\b"
        r"|devicectl\s+device\s+uninstall\b"
        r"|ideviceinstaller[^\n]*?(\s-U\b|--uninstall)"), True),
    (INSTALLS, re.compile(
        ADB + r"install(-multiple)?\b"
        r"|\bpm\s+install\b"
        r"|devicectl\s+device\s+install\b|[\"']devicectl[\"'],\s*[\"']device[\"'],\s*[\"']install[\"']"
        r"|ideviceinstaller[^\n]*?(\s-i\b|--install)"
        r"|\bios-deploy\b"
        r"|platform=iOS,\s*(?:id|name)="), False),
    ("a Gradle task that installs on EVERY attached device, a phone included", re.compile(
        r"(?<![\w-]):?\b(?:install|uninstall)(?:Debug|Release|SeedTwin|All)\w*\b|\bconnected\w*(?:AndroidTest|Check)\b"), True),
    ("a Debug build for a physical iPhone", re.compile(r"Debug-iphoneos"), True),
]


def rule_for(line, inside):
    for name, pattern, everywhere in RULES:
        if (everywhere or not inside) and pattern.search(line):
            return name
    quoted = QUOTED.search(line)
    if quoted and ADB_CONTEXT.search(line):
        if quoted.group(1) == "uninstall":
            return REMOVES
        if not inside:
            return INSTALLS
    return None


def tracked(root):
    p = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], capture_output=True)
    if p.returncode != 0:
        raise CannotAnswer(f"git ls-files failed in {root}")
    return [x for x in p.stdout.decode().split("\0") if x]


def scan_text(path, text):
    """[(line number, rule, line)] for one file's text. In Markdown only the fenced code is read
    (the commands a reader copies), never the prose that describes what a tool does."""
    found = []
    inside = path in CLI_FILES
    markdown = path.endswith(".md")
    fenced = False
    for number, line in enumerate(text.splitlines(), 1):
        if markdown:
            if line.lstrip().startswith("```"):
                fenced = not fenced
                continue
            if not fenced or line.lstrip().startswith("#"):
                continue
        if not CHEAP.search(line) or EXEMPT.search(line) or ABSENCE.search(line):
            continue
        name = rule_for(line, inside)
        if name:
            found.append((number, name, line.strip()[:160]))
    return found


def scan(root, paths=None):
    rows = []
    for path in paths if paths is not None else tracked(root):
        if any(path.startswith(prefix) for prefix in NOT_SCANNED) or os.path.splitext(path)[1] not in TEXT:
            continue
        try:
            data = (Path(root) / path).read_bytes()
        except OSError:
            continue
        if b"\0" in data[:8192]:
            continue
        text = data.decode("utf-8", "replace")
        if CHEAP.search(text):
            rows += [(path, *hit) for hit in scan_text(path, text)]
    return rows


# -- the command line ---------------------------------------------------------------------------

def main(argv):
    if argv[:1] == ["hold"]:
        if "--" not in argv:
            print(json.dumps({"ok": False, "error": "hold --platform P --phone ID [--wait S] -- <command>"}), file=sys.stderr)
            return 2
        cut = argv.index("--")
        q = argparse.ArgumentParser(prog="physical.py hold")
        q.add_argument("--platform", required=True)
        q.add_argument("--phone", default="")
        q.add_argument("--wait", type=float, default=float(os.environ.get("RICHOS_DEVICE_WAIT") or 0))
        q.add_argument("--holder")
        q.add_argument("--net-check", action="store_true")
        h = q.parse_args(argv[1:cut])
        try:
            return hold(h.platform, h.phone, argv[cut + 1:], h.wait, h.holder, net_check=h.net_check)
        except Refused as e:
            print(json.dumps({"ok": False, "refused": str(e)}), file=sys.stderr)
            return 3
        except CannotAnswer as e:
            print(json.dumps({"ok": False, "error": str(e)}), file=sys.stderr)
            return 2
    p = argparse.ArgumentParser(prog="physical.py", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("command", choices=["android-build", "android-condition", "android-gate", "android-install",
                                       "android-list", "android-net", "android-record", "ios-app",
                                       "ios-automation-end", "scan", "status"])
    p.add_argument("--platform")
    p.add_argument("--device")
    p.add_argument("--phone", default="")
    p.add_argument("value", nargs="?")
    p.add_argument("--adb", default="adb")
    p.add_argument("--serial")
    p.add_argument("--apk")
    p.add_argument("--aapt2")
    p.add_argument("--out")
    p.add_argument("--seconds", type=int, default=10)
    p.add_argument("--root", default=str(REPO))
    a = p.parse_args(argv)
    try:
        if a.command == "scan":
            rows = scan(a.root)
            for path, number, rule, line in rows:
                print(f"{path}:{number}: {rule}: {line}", file=sys.stderr)
            if rows:
                print(f"REFUSED: {len(rows)} line(s) touch a phone outside `randroid device` / `rios device`, or "
                      f"remove the app or its data. {RULE}; and the app is never uninstalled or wiped (CEO, "
                      "2026-10-01). Go through the command line; a test that plants such a line to prove a refusal "
                      "says `device-cli-exempt: <reason>` on it.", file=sys.stderr)
                return 1
            print("physical.py scan: no phone install, uninstall or wipe outside the command lines")
            return 0
        if a.command == "status":
            result = status(a.platform, a.phone)
        elif a.command == "ios-app":
            if not a.value:
                raise CannotAnswer("ios-app takes the app bundle's path")
            result = ios_app(a.value)
        elif a.command == "ios-automation-end":
            require_verb("rios")
            result = end_automation_ui(a.device)
        elif a.command == "android-list":
            result = android_phones(a.adb)
        elif a.command == "android-net":
            result = net(a.adb, a.serial)
            print(json.dumps({"ok": result["internet"], "result": result}, indent=2))
            return 0 if result["internet"] else 1
        else:
            if a.command != "android-condition":  # condition only reads: it lists what is pending and restores nothing
                put_back_leftovers(a.adb, a.serial)
            if a.command == "android-build":
                result = probe(a.adb, a.serial)
                physical(result)
            elif a.command == "android-gate":
                result = gate(a.adb, a.serial)
            elif a.command == "android-condition":
                result = condition(a.adb, a.serial, a.seconds, a.out)
            elif a.command == "android-install":
                if not (a.apk and a.aapt2):
                    raise CannotAnswer("android-install needs --apk and --aapt2")
                result = install(a.adb, a.serial, a.apk, a.aapt2)
            else:
                if not a.out:
                    raise CannotAnswer("android-record needs --out FILE.mp4")
                result = record(a.adb, a.serial, a.out, a.seconds)
        print(json.dumps({"ok": True, "result": result}, indent=2))
        return 0
    except Refused as e:
        print(json.dumps({"ok": False, "refused": str(e)}), file=sys.stderr)
        return 3
    except CannotAnswer as e:
        print(json.dumps({"ok": False, "error": str(e)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
