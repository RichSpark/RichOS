#!/usr/bin/env python3
"""Drive a PHYSICAL iPhone through its real controls, and read what an app leaves running.

    phone-ios.py check STEPS.json                  validate a step list; touches no device. Also prints
                                                   its expected duration at the phone's measured median
                                                   step times and the --allowance it needs; `run` refuses
                                                   a list its allowance cannot hold (XCUITest stops there
                                                   and every later step, shot and tree is lost)
    phone-ios.py run STEPS.json --out DIR [--allowance S] [--prebuilt --stamp STAMP.json] [--as-installed]
                 [--approval-announced] [--screen-recording]
                                                   run the steps through XCUITest on the phone
                                                   (`rios device verify script`), then write
                                                   DIR/steps.jsonl (one line per step, phone clock)
                                                   and DIR/attachments/ (the steps' shots and trees;
                                                   with --screen-recording also XCTest's recording
                                                   of the whole session, made on the phone)
                                                   Committed, unchanged app source is never rebuilt:
                                                   the one signed build of that source tree is reused
                                                   from the shared store automatically (see
                                                   physical-device.mjs), so a walk starts in seconds;
                                                   --prebuilt reuses the products the stamp names
                                                   (this checkout's earlier build, or a shared-store
                                                   entry's stamp.json) and refuses unless the app
                                                   still hashes to STAMP.json (perf.py stamp).
                                                   A list whose every step acts in Safari, Settings,
                                                   SpringBoard or Tailscale (or sleeps, marks, opens a link,
                                                   presses Home) runs with the test runner alone: RichConnect
                                                   is not handed to the phone (summary `runnerOnly`).
                                                   --as-installed does the same for a list that acts on the
                                                   test copy: the one already on the phone is used as it is
                                                   (a `rios device install --push production` build)
    phone-ios.py approval --device UDID            will the next run ask the phone's owner to approve
                                                   UI automation? Asks nothing of the phone's screen.
    phone-ios.py wifi-restore --out DIR            read the phone's Wi-Fi switch in Settings; turn it on if
                                                   it is off (`run` does this itself after any list that
                                                   acted on Wi-Fi, however the list ended)
    phone-ios.py wifi-on --out DIR                 the same as wifi-restore, named as the pair of wifi-off
    phone-ios.py wifi-off --out DIR [--device ID]  read the phone's Wi-Fi switch in Settings; turn it off if
                                                   it is on. REFUSED unless this Mac reaches the phone over
                                                   the USB cable (devicectl transportType "wired"): over
                                                   Wi-Fi the Mac would lose the phone with no way to switch
                                                   it back on. `run` refuses a list that presses the Wi-Fi
                                                   switch for the same reason. Put it back with wifi-on and
                                                   check with `rios device net`
    phone-ios.py reboot --device ID                restart the phone (devicectl) and wait until it is back and
                                                   its Wi-Fi carries traffic; it opens NO app, ever (CEO
                                                   2026-10-04). The remedy when `trust` fails (no passcode,
                                                   CEO 2026-10-02); opening an app is `launch`'s one procedure
    phone-ios.py close BUNDLE --device ID          end that app's running process: the Home Screen is in front
    phone-ios.py net --device ID --out FILE [--seconds S] [--url URL] [--raw]
                                                   what the phone itself reaches: this Mac reaching its
                                                   Wi-Fi address, and Safari's internet connections read
                                                   from the phone's log (only Safari's own: devicectl's
                                                   tunnel to this Mac is listed apart), the interface iOS
                                                   elected for its internet (Wi-Fi, or the cable to this
                                                   Mac when Internet Sharing leased it), with its Wi-Fi
                                                   link report; one verdict. Run it before any claim
                                                   about the Wi-Fi. --raw keeps every logged line
    phone-ios.py launch BUNDLE [--device ID] [-- app args]
                                                   open an installed app by the one procedure: the phone must
                                                   be connected and on Wi-Fi (else nothing happens), open once,
                                                   refused -> restart the phone once, wait, open once more,
                                                   refused again -> the exact error. Never a third launch.
    phone-ios.py parse-log TEST.log                the PHONE_STEP lines of a finished run
    phone-ios.py summary DIR                       a finished `run` DIR read back: one row per step
                                                   (outcome, seconds, what it waited for, the audit's
                                                   issues) and each named shot or tree mapped to its
                                                   exported file, so nobody reads the xcresult manifest
    phone-ios.py vocabulary [RUNNER.swift]         exit 1 when a step this tool validates has no case in
                                                   the phone runner's dispatcher (or the reverse), before
                                                   any build or install finds it as "unknown step"
    phone-ios.py pair-steps MAC-TEST-CONFIG.json [--v2-hold S]
                                                   the real-control pairing steps for a lab's current
                                                   link, with all six words checked before They match;
                                                   --v2-hold (pairing v2) waits for each word by label
                                                   and holds S seconds before the press, for
                                                   `pair-words.py check` (see that tool)
    phone-ios.py procs [--name NAME]               the phone's processes whose executable ends in
                                                   NAME (default RichOSNative): pid and path only
    phone-ios.py apps                              which RichConnect builds and test runners are installed
    phone-ios.py lock                              the phone's lock state
    phone-ios.py battery                           level, charging and external power
    phone-ios.py crashes --out DIR [--name NAME] [--since EPOCH]
                                                   the phone's own crash reports (devicectl's systemCrashLogs
                                                   domain) whose file name holds NAME (default RichOSNative)
                                                   plus every JetsamEvent (iOS ending an app for memory),
                                                   copied to DIR, each with its exception type and the
                                                   first frames of the crashed thread. Why an app that was
                                                   in front is suddenly "notRunning" in a run. Reads only
    phone-ios.py syslog --seconds S --out FILE [--keep TEXT ...]
                                                   the phone's log for S seconds, keeping ONLY lines that
                                                   name RichOSNative or dev.richos.connect, plus lines
                                                   containing each --keep TEXT (6+ characters), e.g.
                                                   --keep IOHIDEventSystem: backboardd's HID clients
    (`syslog` and `battery` take --network for an unplugged phone: libimobiledevice's -n, same pairing)
    phone-ios.py trust --device ID --out FILE [--seconds S]
                                                   why iOS will or will not open RichConnect: Developer
                                                   Mode, the installed provisioning profiles and their
                                                   expiry, and one launch of the app with the trust-check,
                                                   DNS and VPN log lines during it (to FILE), read into
                                                   one "cause". Exit 1 when the launch is refused.
                                                   Reads only: nothing installed, removed or switched
    phone-ios.py syslog-rate FILE [--process P] [--bucket S] [--from HH:MM:SS] [--to HH:MM:SS]
                                                   entries ONE process (exact name) wrote per S seconds of
                                                   a `syslog` capture: what the app did while hidden

ONLY THROUGH `rios device` (CEO, 2026-10-02: only the build users get goes on his phones and every
test on them tests it). `run`, `approval`, `procs`, `apps`, `lock`, `battery`, `crashes` and `syslog` touch the
phone, so they run as `rios device <command> ...` and refuse (exit 3) when started any other way;
`rios device verify` runs only the Release build and refuses a bundle carrying Debug's development
markers. `check`, `parse-log`, `summary`, `vocabulary`, `pair-steps` and `syslog-rate` touch no
phone and run directly.

The iPhone counterpart of phone-android.py. iOS 26 offers no shell on the phone, so every tap,
type, Home, lock and screenshot goes through one XCUITest check that executes a list of steps
(`PhysicalDeviceTests.testScript`). Each step prints one `PHONE_STEP` JSON line with its start and
end on the phone's clock, which this tool collects. A failed step stops the list; nothing is
retried. Validation happens here, before anything is built, so a typo costs a second, not a build.

THE APPROVAL PROMPT, AND WHY IT IS NEVER A SURPRISE. When the phone has a passcode, iOS asks
its owner (Touch ID or passcode, at the phone) to allow UI automation when a test session starts
after the phone has been idle; the session waits about 60 s and then fails "Timed out while
enabling automation mode". Apple: the prompt exists only when a passcode is set, comes back
"roughly once per day", and cannot be automated (developer.apple.com/forums/thread/693273).
Measured on this Mac's 108 sessions (2026-09-27 to 10-01): never asked within 12.5 min of the
previous session, always asked after 9.3 h or more, and NOT triggered by reinstalling the runner
(about 100 back-to-back sessions each re-installed it without asking; a 9.7 h idle asked with the
very same products). A phone dedicated to testing is kept without a passcode, so nothing asks;
`run` still forecasts before it builds anything (`approval` prints the same forecast), and
REFUSES a run that is expected to ask unless --approval-announced says the CEO was told first.

First launch of a newly signed development app can require Apple's online verification.
On the physical SE on 2026-09-27, Xcode reported an untrusted certificate but the icon showed
"Unable to Verify App" and Settings had no Developer App profile. One tap on the app icon while
the phone was online resolved it. On 2026-10-02 the same refusal ("Profile Needs Network
Validation") came from the phone's Wi-Fi DNS server no longer answering it, and a restart of the
phone cleared it; the phone has no passcode, so it comes back usable on its own (CEO 2026-10-02).
So `rios device verify|run|perf|hold` restart the phone ONCE on their own when the launch is
refused this way (`rios device reboot`, phone_net.py) and go on, or say why not. Do not send the
person searching for a trust profile that is absent or repeatedly retry the unchanged refusal.

Steps are JSON objects with "do" and, where needed, "id" (accessibility identifier), "label"
(substring of the accessibility label) or "kind" ("switch" or "button": the first one), "in":
"springboard" for system UI, "tailscale" for the route's own app, "settings" for iOS Settings (this
app's permissions under Settings > Apps > RichConnect, and Wi-Fi: set back what you change) or
"safari" for the one share-sheet source, "timeout" (s) and "optional": true (a failure is logged,
the list continues). `tree` and `shot` keep the "in" app's window; `launch` and `terminate` with
"in" start that app fresh on its first page or close it (never SpringBoard); "kind" with "label" is
that control itself (a Settings permission row's inner toggle, not the row: a tap at the row's center
does not toggle it). Settings builds its rows lazily, so a row below the fold is not found until
scrolled; reach this app's page through the app's own "iPhone permissions" row. A list that touches Tailscale may
not take a shot, tree or audit: that screen is the person's own account. Settings' first page names
the phone's Apple Account, so `ocr-gate.sh` every Settings frame before it enters a record.

    launch{textSize} activate terminate home lock{person: true} sleep{seconds} mark{label} state audit
    waitState{state: background|suspended|foreground|notRunning}
    wait tap exists gone value{equals} type{text, delete, focus} press{seconds, drag:[dx,dy]}
    swipe{direction} count{label, equals} alert{button} shot{name, screen} tree{name}
    appearance{set: light|dark}   the phone's own light/dark setting, read (and set); the app follows
                                  the phone. Record the starting value and set it back before the end.
    orientation{set: portrait|landscapeLeft|landscapeRight|portraitUpsideDown}
                                  turn the phone; the step reports the app window's size, so a
                                  portrait-only app is seen to stay upright. End on portrait.
    open{url}                     an https link, opened as a person's tap opens one: in Safari
                                  (test content only, e.g. an isolated lab's own image).
    tapThen{after, at: [x, y]}    tap the target and, `after` seconds later (0-2), touch screen point
                                  `at`, as one synthesized event record: a touch at an exact moment
                                  of an app's return (`perf.py ios --tap-returns`). A `tap` step's
                                  detail also carries `tapAt`, the phone-clock moment it was asked for.

An "id" names the FIRST element carrying that identifier (SwiftUI repeats a container's identifier
on its children).

`launch` may carry Apple's own text-size override (`textSize`, a UIContentSizeCategory name) and
nothing else: the app under test stays the Release app. `audit` records XCTest's accessibility
audit of the screen as it is.

`lock` presses XCTest's lock button, and NOTHING in a script can open the phone again: measured on
an iPhone SE, iOS 26.3.1, with no passcode set (2026-09-24), XCTest's Home press wakes the lock
screen but does not open it, and XCTest cannot start its runner on a locked phone at all ("Device
test exceeded its time limit"). So a `lock` step must say `"person": true` (a person is at the
phone to press Home), and there is no `unlock` step. `shot` keeps only the app unless "screen": true, so nothing else on a person's phone lands
in a record by accident; run `ocr-gate.sh` on any frame before it enters a record.

Environment for `run`: RICHOS_IOS_DEVICE (the hardware UDID xcodebuild accepts) and
RICHOS_APPLE_TEAM. `procs`, `apps`, `lock` and `battery` take --device (devicectl identifier or
UDID). Prints one JSON document; exits 1 when the phone answered "no" (a step failed) and 2 when
it cannot answer at all, with the sentence in `error`.
"""
import argparse
import json
import math
import os
import re
import signal
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'lib'))
import machine_storage as M


ROOT = Path(__file__).resolve().parents[4]  # the richos repository root
RIOS = Path(os.environ.get("RICHOS_IOS_RIOS") or ROOT / "richos/mobile/native-ios/bin/rios")
OURS = ("dev.richos.connect", "dev.richos.native.ios", "dev.richos.mobile.integration")

ACTIONS = {
    "launch": {"textSize"}, "audit": set(), "activate": set(), "terminate": set(), "home": set(), "lock": {"person"},
    "sleep": {"seconds"}, "mark": {"label"}, "state": set(),
    "waitState": {"state"}, "wait": set(), "tap": set(), "exists": set(), "gone": set(),
    "value": {"equals"}, "type": {"text", "delete", "focus"}, "press": {"seconds", "drag"},
    "swipe": {"direction"}, "count": {"label", "equals"}, "alert": {"button"},
    "shot": {"name", "screen"}, "tree": {"name"},
    "appearance": {"set"}, "orientation": {"set"}, "open": {"url"}, "tapThen": {"after", "at"},
}
APPEARANCES = ("light", "dark")
ORIENTATIONS = ("portrait", "landscapeLeft", "landscapeRight", "portraitUpsideDown")
NEEDS_TARGET = {"wait", "tap", "exists", "gone", "value", "type", "press", "swipe", "tapThen"}
COMMON = {"do", "id", "label", "kind", "in", "timeout", "optional"}
PLACES = ("springboard", "tailscale", "settings", "safari")
# The Tailscale app shows the person's own account and devices: a list that touches it may press
# and read its switch, and may not keep a picture or a tree of anything.
KEEPS_SCREEN = {"shot", "tree", "audit"}


class CannotAnswer(Exception):
    pass


def emit(obj, code=0):
    print(json.dumps(obj, indent=2))
    return code


def validate(steps):
    """The whole list, or the first reason it cannot run. Nothing partial reaches the phone."""
    if not isinstance(steps, list) or not steps:
        raise CannotAnswer("steps must be a non-empty JSON list")
    for i, step in enumerate(steps):
        if not isinstance(step, dict):
            raise CannotAnswer(f"step {i} is not an object")
        action = step.get("do")
        if action not in ACTIONS:
            raise CannotAnswer(f"step {i}: unknown step {action!r}")
        extra = set(step) - COMMON - ACTIONS[action]
        if extra:
            raise CannotAnswer(f"step {i} ({action}): unexpected keys {sorted(extra)}")
        if action in NEEDS_TARGET and not (isinstance(step.get("id"), str) or isinstance(step.get("label"), str)
                                           or step.get("kind") in ("switch", "button")):
            raise CannotAnswer(f"step {i} ({action}) names no id or label")
        if "kind" in step and step["kind"] not in ("switch", "button"):
            raise CannotAnswer(f"step {i}: 'kind' may only be 'switch' or 'button'")
        if action == "count" and not isinstance(step.get("label"), str):
            raise CannotAnswer(f"step {i} (count) needs a label")
        if action == "type" and not isinstance(step.get("text"), str):
            raise CannotAnswer(f"step {i} (type) needs text")
        if action == "swipe" and step.get("direction") not in ("up", "down", "left", "right"):
            raise CannotAnswer(f"step {i} (swipe) needs direction up, down, left or right")
        if action == "waitState" and step.get("state") not in ("background", "suspended", "foreground", "notRunning"):
            raise CannotAnswer(f"step {i} (waitState) needs a state")
        if action == "lock" and step.get("person") is not True:
            raise CannotAnswer(f"step {i} (lock) needs \"person\": true: only a person can open the phone again")
        if action == "sleep" and not (isinstance(step.get("seconds"), (int, float)) and 0 < step["seconds"] <= 1800):
            raise CannotAnswer(f"step {i} (sleep) needs seconds between 0 and 1800")
        if action == "appearance" and "set" in step and step["set"] not in APPEARANCES:
            raise CannotAnswer(f"step {i} (appearance) may only set light or dark")
        if action == "orientation" and "set" in step and step["set"] not in ORIENTATIONS:
            raise CannotAnswer(f"step {i} (orientation) may only set {', '.join(ORIENTATIONS)}")
        if action == "open" and not (isinstance(step.get("url"), str) and step["url"].startswith("https://")):
            raise CannotAnswer(f"step {i} (open) needs an https url: it opens Safari, never another app's own scheme")
        if action == "tapThen" and not (
                isinstance(step.get("after"), (int, float)) and 0 <= step["after"] <= 2
                and isinstance(step.get("at"), list) and len(step["at"]) == 2
                and all(isinstance(v, (int, float)) and 0 <= v <= 2000 for v in step["at"])):
            raise CannotAnswer(f"step {i} (tapThen) needs after (0 to 2 s) and at [x, y] in screen points")
        if "in" in step and step["in"] not in PLACES:
            raise CannotAnswer(f"step {i}: 'in' may only be one of {', '.join(PLACES)}")
        if action in ("launch", "terminate") and step.get("in") == "springboard":
            raise CannotAnswer(f"step {i} ({action}): the system UI is never launched or terminated")
        if action == "launch" and "in" in step and "textSize" in step:
            raise CannotAnswer(f"step {i} (launch): textSize is for this app only; another app starts with no arguments")
    if any(s.get("in") == "tailscale" for s in steps) and any(s["do"] in KEEPS_SCREEN for s in steps):
        raise CannotAnswer("a list that touches Tailscale may not keep a shot, tree or audit: its screen is the person's account")
    return steps


def parse_log(text):
    rows = []
    for line in text.splitlines():
        at = line.find("PHONE_STEP ")
        if at < 0:
            continue
        try:
            rows.append(json.loads(line[at + len("PHONE_STEP "):]))
        except json.JSONDecodeError:
            continue  # xcodebuild echoes the print once more inside a quoted line; the clean one counts
    seen, unique = set(), []
    for row in rows:
        if row.get("i") in seen:
            continue
        seen.add(row.get("i"))
        unique.append(row)
    return unique


def summary(directory):
    """A finished `run` directory read back for a person: every step on one row, and the named
    shots and trees mapped to the files xcresulttool exported. Refuses a directory that is not a
    finished run rather than printing an empty table."""
    directory = Path(directory)
    try:
        rows = [json.loads(line) for line in (directory / "steps.jsonl").read_text().splitlines() if line.strip()]
    except FileNotFoundError:
        raise CannotAnswer(f"{directory} has no steps.jsonl: it is not a finished phone-ios.py run")
    except json.JSONDecodeError as error:
        raise CannotAnswer(f"{directory}/steps.jsonl is not one JSON object per line: {error}")
    if not rows:
        raise CannotAnswer(f"{directory}/steps.jsonl is empty: the phone logged no step")
    steps = []
    for row in rows:
        detail = row.get("detail") or {}
        out = {"i": row.get("i"), "do": row.get("do"), "ok": row.get("ok"),
               "seconds": round(float(row.get("end", 0)) - float(row.get("start", 0)), 2), "app": row.get("appState")}
        for key in ("error", ):
            if row.get(key):
                out[key] = row[key]
        for key in ("label", "value", "count", "exists", "waitedMs", "text", "issues", "appearance", "orientation", "frame"):
            if key in detail:
                out[key] = detail[key]
        steps.append(out)
    named = {}
    manifest = directory / "attachments" / "manifest.json"
    if manifest.exists():
        try:
            tests = json.loads(manifest.read_text())
        except json.JSONDecodeError as error:
            raise CannotAnswer(f"{manifest} is not JSON: {error}")
        for test in tests:
            for item in test.get("attachments", []):
                # xcresulttool names an attachment "<name>_<n>_<UUID>.<ext>".
                human = str(item.get("suggestedHumanReadableName", ""))
                found = re.match(r"^(.*)_\d+_[0-9A-F-]{36}\.(\w+)$", human)
                key = f"{found.group(1)}.{found.group(2)}" if found else human
                named[key] = str(directory / "attachments" / item.get("exportedFileName", ""))
    failed = [s for s in steps if not s["ok"]]
    return {"steps": steps, "logged": len(steps), "failed": len(failed), "files": dict(sorted(named.items()))}


RUNNER = ROOT / "richos/mobile/native-ios/UITests/PhysicalDeviceTests.swift"
# Steps the phone runner knows that this tool refuses on purpose: there is no scripted unlock
# (XCTest's Home press does not open an iOS 26 lock screen; see the module docstring).
REFUSED_HERE = {"unlock"}


def vocabulary(runner_path):
    """Does the phone runner (`PhysicalDeviceTests.perform`) have a case for every step this tool
    validates? A step the host accepts and the phone does not know fails on the phone, after a
    build and an install, as "unknown step"; this finds it before either."""
    try:
        source = Path(runner_path).read_text()
    except OSError as error:
        raise CannotAnswer(f"cannot read the phone runner {runner_path}: {error}")
    start = source.find("private func perform(")
    if start < 0:
        raise CannotAnswer(f"{runner_path} has no perform(...) step dispatcher")
    # The dispatcher's own cases sit at eight spaces; nested switches (swipe directions) deeper.
    phone = set()
    for group in re.findall(r'^ {8}case ((?:"[A-Za-z]+"(?:, )?)+):', source[start:], re.M):
        phone.update(re.findall(r'"([A-Za-z]+)"', group))
    if not phone:
        raise CannotAnswer(f"found no step cases in {runner_path}'s perform(...)")
    host = set(ACTIONS)
    missing = sorted(host - phone)
    unknown = sorted(phone - host - REFUSED_HERE)
    return {"same": not missing and not unknown, "host": len(host), "phone": len(phone),
            "missingOnPhone": missing, "unknownToHost": unknown}


def pair_steps(config_path, v2_hold=None):
    """The real-control pairing a person does, from the lab's own mac-test-config.json: open the
    link field, enter the link, check all six words BEFORE pressing They match, then the consent.

    Pairing v2 (`v2_hold` seconds): the Mac's words depend on the key the phone registers, so the
    config's words (v1) are not the ones to count. Each "Word n:" is waited for by label instead,
    which puts the phone's six words in its PHONE_STEP log, and the press waits `v2_hold` seconds
    while the walker runs `pair-words.py check` and stops the run if it does not exit 0."""
    try:
        config = json.loads(Path(config_path).read_text())
    except (FileNotFoundError, json.JSONDecodeError) as error:
        raise CannotAnswer(f"cannot read the lab configuration {config_path}: {error}")
    link, words = config.get("pairLink"), str(config.get("words", "")).split()
    if not (isinstance(link, str) and link.startswith("https://") and "#pair=" in link):
        raise CannotAnswer("the lab configuration has no current HTTPS pairing link")
    if v2_hold is None and len(words) != 6:
        raise CannotAnswer("the lab configuration has no current HTTPS pairing link and six words")
    if v2_hold is not None and not 20 <= v2_hold <= 300:
        raise CannotAnswer("--v2-hold must be 20 to 300 seconds: long enough to check, short enough to stay inside the code's five minutes")
    steps = [{"do": "launch"}, {"do": "tap", "id": "pair.link", "timeout": 10},
             {"do": "type", "id": "pairlink.field", "text": link}, {"do": "tap", "id": "pairlink.submit"},
             {"do": "wait", "id": "pair.match", "timeout": 20}]
    if v2_hold is None:
        steps += [{"do": "count", "label": f"Word {n}: {word}", "equals": 1} for n, word in enumerate(words, 1)]
    else:
        steps += [{"do": "wait", "label": f"Word {n}: ", "timeout": 5} for n in range(1, 7)]
        steps += [{"do": "mark", "label": "compare the words now: pair-words.py check"}, {"do": "sleep", "seconds": v2_hold}]
    steps += [{"do": "tap", "id": "pair.match"}, {"do": "tap", "id": "consent.continue", "timeout": 4, "optional": True},
              {"do": "wait", "id": "composer.field", "timeout": 20}]
    return validate(steps)


# Median seconds per step kind on the physical iPhone SE (iOS 26.3.1), measured 2026-10-01
# from 230 passed steps of two step lists (quint-opus-rewalk3). A `sleep` takes its own seconds;
# a kind not listed here (mark, waitState, ...) measured ~0 and counts as 0.
STEP_MEDIAN_S = {"tap": 2.0, "wait": 1.2, "type": 2.9, "press": 2.64, "swipe": 2.69, "launch": 2.64,
                 "value": 1.48, "appearance": 1.17, "terminate": 1.08, "home": 0.46, "activate": 0.4,
                 "tree": 0.52, "shot": 0.15, "exists": 0.13,
                 # Not yet measured on the phone: a tap's median, plus its `after` (at most 2 s).
                 "tapThen": 4.0}
# XCUITest stops a test at its execution-time allowance: the steps after it never run and the
# shots and trees they took are never exported (lost: a 158-step list measured 245 s against the
# default 240 s, 2026-10-01). Ask for this much headroom over the measured-median estimate.
ALLOWANCE_HEADROOM = 1.25


def expected_seconds(steps):
    """The list's duration at this phone's measured median step times (sleeps at face value)."""
    return round(sum(float(s.get("seconds", 0)) if s["do"] == "sleep" else STEP_MEDIAN_S.get(s["do"], 0.0)
                     for s in steps), 1)


def allowance_refusal(steps, allowance):
    """A sentence when the allowance cannot hold the list, else None."""
    expected = expected_seconds(steps)
    needed = math.ceil(expected * ALLOWANCE_HEADROOM)
    if needed <= allowance:
        return None
    return (f"this list of {len(steps)} steps is expected to take about {expected} s at the phone's measured "
            f"median step times, and the allowance is {allowance} s: XCUITest stops at the allowance and every "
            f"later step, shot and tree is lost. Pass --allowance {min(max(needed, 60), 1800)}"
            + (" or split the list" if needed > 1800 else ""))


def load_steps(path):
    try:
        return validate(json.loads(Path(path).read_text()))
    except FileNotFoundError:
        raise CannotAnswer(f"no step file at {path}")
    except json.JSONDecodeError as error:
        raise CannotAnswer(f"{path} is not JSON: {error}")


def stamped_identity(stamp_path):
    """Identity or refuse: the prebuilt app must still be the stamped bytes (perf.py stamp)."""
    if not stamp_path:
        raise CannotAnswer("--prebuilt needs --stamp: reused products are only trusted against their stamp")
    try:
        stamp = json.loads(Path(stamp_path).read_text())
    except (FileNotFoundError, json.JSONDecodeError) as error:
        raise CannotAnswer(f"cannot read the stamp {stamp_path}: {error}")
    sys.path.insert(0, str(ROOT / "richos/mobile/perf"))
    import perfcore  # the same tree hash perf.py stamp wrote
    artifact = stamp.get("artifact", "")
    if not os.path.isdir(artifact):
        raise CannotAnswer(f"the stamped app {artifact} is gone")
    actual = perfcore.tree_sha256(artifact)
    if actual != stamp.get("sha256") or stamp.get("dirty"):
        raise CannotAnswer(f"freshness mismatch: {artifact} hashes {actual[:12]}…, the stamp says "
                           f"{str(stamp.get('sha256'))[:12]}… (dirty={stamp.get('dirty')})")
    # The Build/Products directory holding the stamped app (.../Products/Release-iphoneos/RichOSNative.app):
    # the run hands the phone these products and no others (physical-device.mjs prebuiltProducts),
    # whether they are this checkout's earlier build or a shared-store entry a measured build came from.
    products = os.path.dirname(os.path.dirname(os.path.realpath(artifact)))
    return {"artifact": artifact, "sha256": actual, "commit": stamp.get("commit"), "products": products}


def storage_path(path):
    try:
        return M.validate(path)
    except ValueError as error:
        raise CannotAnswer(str(error)) from error


SESSION_LOGS = str(M.cache("richos-native-ios")) + "/*/physical/*-test.log"
APPROVAL_TIMEOUT = "Timed out while enabling automation mode"
ASKS_AFTER_IDLE_S = 9.3 * 3600   # measured: a phone with a passcode always asked after this much idle
LEDGER = str(M.cache("richos-native-ios/phone-sessions.jsonl"))
HARDWARE_UDID = re.compile(r"^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{16}$")


def device_ids(device):
    """Every spelling of one phone, lowercase. devicectl and `approval --device` use the CoreDevice
    UUID (691DB4F7-...); xcodebuild logs and libimobiledevice use the hardware UDID (00008030-...).
    Matching only the given spelling is why `approval` read 0 sessions on 2026-10-01 for a phone that
    had run all day. RICHOS_IOS_DEVICE_ALIASES (comma-separated) adds spellings, for fixtures."""
    ids = {device.lower()}
    ids.update(a.strip().lower() for a in os.environ.get("RICHOS_IOS_DEVICE_ALIASES", "").split(",") if a.strip())
    if not HARDWARE_UDID.match(device) and not os.environ.get("RICHOS_IOS_DEVICE_ALIASES"):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "d.json"
            try:
                subprocess.run(["xcrun", "devicectl", "list", "devices", "--json-output", str(out)],
                               capture_output=True, timeout=30)
                for d in json.loads(out.read_text())["result"]["devices"]:
                    if d.get("identifier", "").lower() == device.lower():
                        udid = d.get("hardwareProperties", {}).get("udid")
                        if udid:
                            ids.add(udid.lower())
            except (OSError, subprocess.TimeoutExpired, ValueError, KeyError):
                pass
    return ids


def hardware_udid(device):
    """The hardware UDID (00008030-...) libimobiledevice and `log collect` need, from any spelling."""
    for spelling in sorted(device_ids(device)):
        if HARDWARE_UDID.match(spelling):
            return spelling.upper()
    raise CannotAnswer(f"no hardware UDID for {device} in `xcrun devicectl list devices`")


def record_session(device, started, ended, log=None, session=None, error=None):
    """Append this run to the ledger `approval` reads: when it started and ended, how long enabling
    automation took, whether it needed approval (it did when the wait was 3 s or more, or timed out)."""
    s = session or {}
    wait = s.get("enableWaitSeconds")
    timed_out = bool(s.get("timedOut"))
    row = {"device": device, "started": started, "ended": ended, "log": str(log) if log else None,
           "enableWaitSeconds": wait, "timedOut": timed_out, "passcodeConfigured": s.get("passcodeConfigured"),
           "approvalNeeded": True if timed_out or (wait is not None and wait >= ASKED_WAIT_S) else (False if wait is not None else None),
           "error": error}
    path = Path(os.environ.get("RICHOS_IOS_SESSION_LEDGER", LEDGER))
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "a") as f:
            f.write(json.dumps(row) + "\n")
    except OSError:
        return None
    return row
QUIET_WINDOW_S = 600      # measured: no session within 12.5 min of the previous one ever asked
COLD_GAP_S = 3600         # a session after this much idle is the one that would ask
ASKED_WAIT_S = 3.0        # runner start to first suite: 0.5-1.2 s unasked, 9.9-24.7 s when he approved
_PHONE_TS = r"(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3})"


def session_from_log(path):
    """One automation session as its xcodebuild log left it: which phone, when it ran (this Mac's
    clock: the log's creation and last write) and how long the runner waited for UI automation to
    be enabled (the phone's clock: runner start to first suite). None for a log naming no phone."""
    text = Path(path).read_text(errors="replace")
    device = re.search(r"-destination id=([A-Fa-f0-9-]+)", text)
    if not device:
        return None
    st = os.stat(path)
    runner = (re.search(_PHONE_TS + r"\d*[+-]\d{4} RichOSNativeUITests-Runner\[\d+:\d+\] \[Default\] Running tests", text)
              or re.search(_PHONE_TS + r"\d*[+-]\d{4} RichOSNativeUITests-Runner\[", text))
    suite = re.search(r"Test Suite '[^']+' started at " + _PHONE_TS, text)
    wait = None
    if runner and suite:
        fmt = "%Y-%m-%d %H:%M:%S.%f"
        from datetime import datetime
        wait = round((datetime.strptime(suite.group(1), fmt) - datetime.strptime(runner.group(1), fmt)).total_seconds(), 1)
    passcode = None
    said = re.search(r"PHONE_PASSCODE (\{[^\n]*?\})\s*$", text, re.M)
    if said:
        try:
            value = json.loads(said.group(1)).get("configured")
            passcode = value if isinstance(value, bool) else None
        except json.JSONDecodeError:
            pass
    return {"log": str(path), "device": device.group(1), "started": getattr(st, "st_birthtime", st.st_mtime),
            "ended": st.st_mtime, "enableWaitSeconds": wait, "timedOut": APPROVAL_TIMEOUT in text,
            "passcodeConfigured": passcode}


def sessions(device, pattern=None):
    import glob
    week = time.time() - 7 * 86400
    ids = device_ids(device)
    found = []
    for path in glob.glob(pattern or os.environ.get("RICHOS_IOS_SESSION_LOGS", SESSION_LOGS)):
        try:
            if os.stat(path).st_mtime < week:
                continue
            s = session_from_log(path)
        except OSError:
            continue
        if s and s["device"].lower() in ids:
            found.append(s)
    seen = {s["log"] for s in found}
    ledger = Path(os.environ.get("RICHOS_IOS_SESSION_LEDGER", LEDGER))
    try:
        rows = ledger.read_text().splitlines()
    except OSError:
        rows = []
    for line in rows:
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if not isinstance(r, dict) or str(r.get("device", "")).lower() not in ids or r.get("log") in seen \
                or not isinstance(r.get("ended"), (int, float)) or r["ended"] < week:
            continue
        found.append({"log": r.get("log") or "(run ledger)", "device": r["device"], "started": r.get("started", r["ended"]),
                      "ended": r["ended"], "enableWaitSeconds": r.get("enableWaitSeconds"),
                      "timedOut": bool(r.get("timedOut")), "passcodeConfigured": r.get("passcodeConfigured")})
    return sorted(found, key=lambda s: s["started"])


def passcode_state(device):
    """True or False only when the phone itself answered whether a passcode is CONFIGURED; None
    otherwise. Lockdown's PasswordProtected and devicectl's passcodeRequired are NOT this: both
    say whether the phone is locked right now (both read false on 2026-10-01 while it had one).
    Measured on iOS 26.3.1 (2026-10-01): MobileGestalt answers "MobileGestaltDeprecated", so this
    is None there and the forecast uses the phone's own PHONE_PASSCODE line from its last session."""
    try:
        udid = next((i for i in device_ids(device) if HARDWARE_UDID.match(i)), device)
        p = subprocess.run(["idevicediagnostics", "-u", udid.upper(), "mobilegestalt", "PasswordConfigured"],
                           capture_output=True, timeout=20)
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if p.returncode != 0:
        return None
    import plistlib
    try:
        gestalt = plistlib.loads(p.stdout).get("MobileGestalt", {})
    except Exception:
        return None
    value = gestalt.get("PasswordConfigured")
    return value if gestalt.get("Status") == "Success" and isinstance(value, bool) else None


def forecast(device, now=None, passcode=None, pattern=None):
    """Will the next session ask the phone's owner to approve UI automation? Decided from the phone's
    passcode when it answers, else from this phone's own session history. Never touches its screen."""
    now = time.time() if now is None else now
    history = sessions(device, pattern)
    # No Mac-side tool reads the passcode on iOS 26, so the phone's own answer at its latest
    # session (PhysicalDeviceTests.passcodeReading) stands in, with its age.
    said = next((s for s in reversed(history) if s.get("passcodeConfigured") is not None), None)
    source = "the phone, now"
    if passcode is None and said:
        passcode = said["passcodeConfigured"]
        source = f"the phone itself at a session {round((now - said['ended']) / 60)} min ago"
    out = {"device": device, "passcodeConfigured": passcode, "passcodeReadBy": source if passcode is not None else None,
           "sessionsOnRecord": len(history)}
    last = history[-1] if history else None
    if last:
        out["lastSession"] = {"endedSecondsAgo": round(now - last["ended"]), "enableWaitSeconds": last["enableWaitSeconds"],
                              "timedOut": last["timedOut"], "log": last["log"]}
    if passcode is False:
        return {**out, "approvalExpected": False,
                "why": f"the phone has no passcode ({source}), and iOS asks to allow UI automation only when one is set"}
    if last and last["timedOut"]:
        return {**out, "approvalExpected": True,
                "why": f"the last session on this phone ({last['log']}) failed '{APPROVAL_TIMEOUT}': nobody approved it"}
    if last and now - last["ended"] <= QUIET_WINDOW_S:
        return {**out, "approvalExpected": False,
                "why": f"the last session ended {round(now - last['ended'])} s ago; within {QUIET_WINDOW_S} s none has ever asked"}
    if passcode is None and not history:
        return {**out, "approvalExpected": "unknown",
                "why": "the passcode could not be read and this phone has no session on record: nobody knows yet whether it will ask",
                "settledBy": "one run (phone-ios.py run records its session; a wait under 3 s means no prompt and no passcode), "
                             "or Settings > Face ID/Touch ID & Passcode on the phone"}
    if passcode is None:
        cold = [s for prev, s in zip(history, history[1:]) if s["started"] - prev["ended"] >= COLD_GAP_S]
        if cold and not cold[-1]["timedOut"] and cold[-1]["enableWaitSeconds"] is not None \
                and cold[-1]["enableWaitSeconds"] < ASKED_WAIT_S:
            return {**out, "approvalExpected": False, "coldSession": cold[-1]["log"],
                    "why": f"the passcode could not be read, and the last session after an idle hour "
                           f"started in {cold[-1]['enableWaitSeconds']} s without asking (no passcode since then)"}
    if passcode is None and last and not last["timedOut"] and last["enableWaitSeconds"] is not None \
            and last["enableWaitSeconds"] < ASKED_WAIT_S and now - last["ended"] < ASKS_AFTER_IDLE_S:
        # A prompt-free session that began after the phone had idled past the quiet window proves no passcode
        # at that moment; a phone with one always asked after 9.3 h of idle.
        prev = history[-2] if len(history) > 1 else None
        if prev and last["started"] - prev["ended"] > QUIET_WINDOW_S:
            return {**out, "approvalExpected": False, "promptFreeSession": last["log"],
                    "why": f"the passcode could not be read, but the last session started in {last['enableWaitSeconds']} s "
                           f"without asking after {round((last['started'] - prev['ended']) / 60)} min idle; "
                           f"none is expected until {round(ASKS_AFTER_IDLE_S / 3600, 1)} h after it"}
    idle = "no session on record for this phone" if not last else f"the last session ended {round((now - last['ended']) / 60)} min ago"
    held = "the phone has a passcode" if passcode else "the passcode could not be read"
    return {**out, "approvalExpected": True,
            "why": f"{held} and {idle}: iOS asks again after idle (never within 12.5 min, always after 9.3 h, measured)"}


class Refusal(CannotAnswer):
    """A refusal that also carries fields for the JSON it is printed as."""

    def __init__(self, message, **extra):
        super().__init__(message)
        self.extra = extra


# THE REFUSAL RAISES ITS OWN ESCALATION (2026-10-01). Both of that day's
# escalations that needed the CEO at this phone came out of the refusal below,
# and the first sat 82 minutes because nothing woke the idle lead (richos-hq
# docs/operations/2026-10-01-escalation-wakes-the-lead.md, item C). The step that
# KNOWS the CEO's hands are needed now says so itself: it appends one row to the
# engine's escalation ledger with needs=ceo-hands, which the stall watcher tells
# the lead within a minute and again every 10 minutes until acknowledged. One
# need, one id: an outstanding ceo-hands escalation for the same workspace is
# reused, and the refusal tells the teammate not to raise another. A placeholder
# device id (all zeros, the suite's own fake phone) raises nothing.
ENGINE_LIB = ROOT / "richos/engine/scripts/lib"
APPROVAL_TITLE = "UI automation approval needed at the test iPhone"
PLACEHOLDER_DEVICE = re.compile(r"^[0-]+$")


def _escalations():
    """The engine's escalation library in this checkout, or None."""
    import importlib.util
    path = ENGINE_LIB / "escalations.py"
    if not path.exists():
        return None
    try:
        spec = importlib.util.spec_from_file_location("richos_escalations", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    except Exception:  # noqa: BLE001: a library that cannot load is said, never guessed past
        return None


def _git_out(cwd, *args):
    try:
        p = subprocess.run(["git", "-C", str(cwd), *args], capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return ""
    return p.stdout.strip() if p.returncode == 0 else ""


def workspace_of(cwd):
    """The workspace a run starts in: its git top level, else the directory itself."""
    return os.path.realpath(_git_out(cwd, "rev-parse", "--show-toplevel") or str(cwd))


def raise_approval_escalation(device, f, cwd=None):
    """(id, raised now?, problem). Raises nothing for a placeholder device, and
    reuses an outstanding ceo-hands escalation for the same workspace."""
    if PLACEHOLDER_DEVICE.match(device or ""):
        return None, False, f"{device} is a placeholder device id (all zeros), not a phone, so nothing was raised"
    esc = _escalations()
    if esc is None:
        return None, False, f"the engine's escalation library ({ENGINE_LIB / 'escalations.py'}) could not be loaded"
    wt = workspace_of(cwd or os.getcwd())
    rows, _bad = esc.read_rows()
    if rows is None:
        return None, False, f"the escalation ledger ({esc.ledger_path()}) could not be read"
    for e in esc.outstanding(rows):
        if e.get("needs") == "ceo-hands" and e.get("worktree") and os.path.realpath(e["worktree"]) == wt:
            return e["id"], False, ""
    common = _git_out(wt, "rev-parse", "--path-format=absolute", "--git-common-dir")
    repo = os.path.basename(common[:-len("/.git")] if common.endswith("/.git") else common) or os.path.basename(wt)
    from types import SimpleNamespace
    args = SimpleNamespace(
        title=APPROVAL_TITLE, state="proceeding", audience="lead", needs="ceo-hands",
        question=(f"Can the CEO be at the test iPhone ({device}) to allow UI automation with Touch ID or the "
                  "passcode, within about 60 s of the next phone-ios.py run starting? Tell the teammate in "
                  f"{os.path.basename(wt)} when he is there; it then reruns with --approval-announced."),
        tried=f"phone-ios.py run refused before any build or install: {f['why']}.",
        meanwhile=("Nothing was built, installed or run on the phone. This was raised by phone-ios.py itself "
                   "at its approval refusal; the teammate raises no second escalation for it."),
        teammate=os.path.basename(wt), worktree=wt, branch=_git_out(wt, "symbolic-ref", "--short", "HEAD"),
        repo=repo, head=_git_out(wt, "rev-parse", "HEAD"), record="", session="")
    problems = esc.validate_raise(args)
    if problems:
        return None, False, "; ".join(problems)
    row = esc.build_row(args)
    try:
        esc.append_row(row)
    except Exception as error:  # noqa: BLE001: a raise that did not land says so
        return None, False, f"the escalation ledger could not be written ({error})"
    return row["id"], True, ""


def close_approval_escalations(cwd, log, when=None):
    """Acknowledge this workspace's outstanding approval escalations once a run
    with --approval-announced reached its first step: UI automation was allowed,
    so the 10-minute repeat stops on the fact, not on the lead remembering to
    ack (Sage's review, item 7). Returns the ids it closed."""
    esc = _escalations()
    if esc is None:
        return []
    rows, _bad = esc.read_rows()
    if rows is None:
        return []
    wt = workspace_of(cwd or os.getcwd())
    stamp = time.strftime("%H:%MZ", time.gmtime(when or time.time()))
    closed = []
    for e in esc.outstanding(rows):
        if e.get("needs") != "ceo-hands" or not str(e.get("title", "")).startswith(APPROVAL_TITLE):
            continue
        if not e.get("worktree") or os.path.realpath(e["worktree"]) != wt:
            continue
        row = {"event": "EscalationAck", "id": e["id"], "acked": esc.iso(esc.utcnow()),
               "disposition": (f"UI automation was allowed at the phone: the run at {stamp} reached its first "
                               f"step (log {log}). Closed by phone-ios.py itself."),
               "actor": "phone-ios.py", "session_id": ""}
        try:
            esc.append_row(row)
        except Exception:  # noqa: BLE001: an unwritten ack leaves it outstanding, which is the loud side
            continue
        closed.append(e["id"])
    return closed


def pairs_by_link(steps):
    """Does this list pair through a lab link (a `type` step carrying #pair=)?"""
    return any(s.get("do") == "type" and "#pair=" in str(s.get("text", "")) for s in steps)


# THE PAIRING WINDOW OUTLASTED BY THE APPROVAL (2026-10-01). The lab opens one
# pairing window when it starts (mobile/dev/mac-server.rs: open_pairing at
# startup, five minutes, app/src-tauri/src/phone/device.rs). That day the lab
# opened at 13:44Z, the approval took its time, and the link was typed at
# 13:49:55Z: the window had closed, the pairing failed, and the one approval was
# spent on it. The lab cannot know an approval is pending; this tool knows both
# the forecast and the step list, so the ordering is enforced here: spend the
# approval on a session with no deadline first, THEN start the lab. A session
# within 10 minutes of the previous one has never asked (QUIET_WINDOW_S).
PAIRING_ORDER = ("this list pairs through the lab's link, whose pairing window lasts five minutes from the "
                 "lab's start, and the approval wait can outlast it (2026-10-01: the window opened 13:44Z, the "
                 "link was typed 13:49:55Z after the approval, and the pairing failed). Spend the approval first: "
                 "run a list with no pairing, for example [{\"do\": \"state\"}], with --approval-announced; then "
                 "start the lab (that opens the window) and generate pair-steps, and run them within 10 minutes "
                 "of that session, when no approval is asked.")


def approval_refusal(f, raised=None, new=False, problem="", pairing=False):
    text = (f"this run is expected to ask the phone's owner to allow UI automation (Touch ID or passcode, "
            f"at the phone, within about 60 s, or the run fails '{APPROVAL_TIMEOUT}'): {f['why']}. "
            "Tell Rich so the CEO hears it BEFORE the run, then pass --approval-announced. ")
    if raised and new:
        text += (f"Raised {raised} with needs=ceo-hands, so the lead is woken now and every 10 minutes until "
                 "it is acknowledged; do not raise another for this. ")
    elif raised:
        text += (f"Already raised as {raised} (needs=ceo-hands, still unacknowledged); do not raise another "
                 "for this. ")
    elif problem:
        text += f"No escalation was raised: {problem}. "
    if pairing:
        text += "And " + PAIRING_ORDER + " "
    return text + "Removing the test phone's passcode ends the prompt for good."


def approval(args):
    return emit(forecast(args.device, passcode=passcode_state(args.device)))


# Steps that act on no app: a wait, a log mark, a link opened in Safari, the Home button.
APPLESS = {"sleep", "mark", "open", "home"}


def addresses_app(steps):
    """Does any step act on RichConnect (a step with no "in" place, other than APPLESS)? A list that does not is run
    with the test runner alone (RICHOS_PHYSICAL_RUNNER_ONLY, physical-device.mjs): xcodebuild is not handed
    RichConnect's bundle, so the CEO's installed app is left as it is (CEO 2026-10-03: his app is his)."""
    return any(s.get("do") not in APPLESS and s.get("in") not in PLACES for s in steps)


def runner_only_for(steps, as_installed=False):
    """Is the test runner alone handed to the phone? Yes for a list that acts on no app, and for any list run
    --as-installed (the test copy on the phone, e.g. a production-push build, is used as it is)."""
    return bool(as_installed) or not addresses_app(steps)


def touches_wifi(steps):
    """Does the list act on Wi-Fi in iOS Settings (a step in Settings naming Wi-Fi)?"""
    return any(s.get("in") == "settings" and "wi-fi" in str(s.get("label", "")).replace("‑", "-").lower()
               for s in steps)


# THE PHONE'S WI-FI IS PUT BACK, HOWEVER A RUN ENDS (CEO, 2026-10-02: "Unable to Verify App" again, and
# "this phone is ALWAYS on Wi-Fi here unless one of the workers has deliberately switched Wi-Fi off").
# A list that switches Wi-Fi off in Settings (the walk's network-drop pose, R2's floors) switches it back
# on in its own later steps, and XCUITest stops a list at its first failed step or its allowance, so a
# list that stops between the two leaves the phone with no network: iOS then cannot verify the
# developer certificate online and refuses to open the app ("Unable to Verify App"). So `run` itself
# reads the Wi-Fi switch after any list that touched it, however the list ended, and turns it on if it
# is off. Steps as measured on the SE (iOS 26.3.1): the row says "Wi-Fi, <network>", its switch is
# named "Wi‑Fi" with a non-breaking hyphen and reads "1" on, "0" off.
WIFI_SWITCH = "Wi‑Fi"
# The switch itself, by kind: on iOS 26.3.1 (measured on the SE 2026-10-03) the Wi-Fi page also carries a
# StaticText labeled "Wi‑Fi" (non-breaking hyphen) ABOVE the switch, so the label alone matched that text: its
# value read "" and a tap on it toggled nothing, and the restore reported success with the Wi-Fi untouched.
WIFI_CONTROL = {"kind": "switch", "label": WIFI_SWITCH, "in": "settings", "timeout": 10}


def wifi_steps(turn_on):
    steps = [{"do": "launch", "in": "settings"}, {"do": "tap", "label": "Wi-Fi", "in": "settings", "timeout": 10}]
    if turn_on:
        steps += [{"do": "tap", **WIFI_CONTROL}, {"do": "sleep", "seconds": 3}]
    return steps + [{"do": "value", **WIFI_CONTROL}, {"do": "terminate", "in": "settings"}]


def wifi_value(out):
    """The Wi-Fi switch's value ('1' on, '0' off) a wifi_steps list read, or None: anything else (an element that
    is not the switch reads "") is not a reading."""
    try:
        rows = [json.loads(line) for line in (Path(out) / "steps.jsonl").read_text().splitlines() if line.strip()]
    except (OSError, ValueError):
        return None
    for row in rows:
        if row.get("do") == "value" and row.get("ok"):
            value = str((row.get("detail") or {}).get("value"))
            return value if value in ("0", "1") else None
    return None


def ensure_wifi_on(out_root):
    """Read the phone's Wi-Fi switch and turn it on when it is off. Always returns a dict, never raises:
    {'before': '0'|'1'|None, 'after': ..., 'turnedOn': bool, 'error': sentence or None}."""
    return ensure_wifi(out_root, on=True)


def ensure_wifi(out_root, on):
    """Read the phone's Wi-Fi switch and press it when it is not where `on` wants it. Always returns a dict,
    never raises: {'before', 'after', 'turnedOn' (on) or 'turnedOff' (off), 'error'}."""
    from types import SimpleNamespace
    want, other = ("1", "0") if on else ("0", "1")
    moved = "turnedOn" if on else "turnedOff"
    result = {"before": None, "after": None, moved: False, "error": None}
    out_root = Path(out_root)
    try:
        for press, name in ((False, "wifi-read"), (True, "wifi-on" if on else "wifi-off")):
            if press and result["before"] != other:
                break
            folder = out_root / name
            folder.mkdir(parents=True, exist_ok=True)
            listing = folder / "steps-in.json"
            listing.write_text(json.dumps(wifi_steps(press)))
            summary = _run(SimpleNamespace(steps=str(listing), out=str(folder), allowance=120, prebuilt=False,
                                           stamp=None, approval_announced=False))
            value = wifi_value(folder)
            if press:
                result.update(after=value, **{moved: value == want})
            else:
                result.update(before=value, after=value)
            if value is None:
                result["error"] = ("the phone's Wi-Fi switch could not be read "
                                   f"({summary.get('error') or summary.get('failed') or 'no 0/1 reading from the switch'})")
                break
        if result["before"] == other and not result[moved] and not result["error"]:
            result["error"] = (f"the phone's Wi-Fi switch was {'off' if on else 'on'} and is still "
                               f"{'off' if on else 'on'} after the tap")
    except CannotAnswer as error:
        result["error"] = str(error)
    except Exception as error:  # noqa: BLE001: the epilogue says what failed, it never hides the run's own result
        result["error"] = f"{type(error).__name__}: {error}"
    return result


# WI-FI OFF ONLY OVER THE CABLE (CEO brief 2026-10-04). A phone this Mac reaches over Wi-Fi (devicectl's
# transportType "localNetwork") is cut off from it the moment its Wi-Fi switch goes off: the UI-test session
# that would switch it back on can no longer start, and nothing on the Mac can reach the phone again. Over the
# USB cable ("wired") the developer tunnel does not use the phone's Wi-Fi, so the Mac keeps the phone.
def switches_wifi(steps):
    """Does the list press the Wi-Fi SWITCH in Settings (the control itself, by kind; a tap on the text labeled
    Wi-Fi toggles nothing, measured 2026-10-03)?"""
    return any(s.get("in") == "settings" and s.get("do") in ("tap", "press", "tapThen") and s.get("kind") == "switch"
               and "wi-fi" in str(s.get("label", "")).replace("‑", "-").lower() for s in steps)


def cable_refusal(props):
    """None when devicectl's connectionProperties say the phone is reached over the USB cable, else the sentence.
    Pure (mobile-device.test.py)."""
    tunnel, transport = props.get("tunnelState"), props.get("transportType")
    if tunnel != "connected":
        return f"the iPhone is not connected to this Mac (devicectl: tunnel {tunnel}); its Wi-Fi was not touched"
    if transport != "wired":
        return (f"this Mac reaches the iPhone over {transport or 'an unreported transport'}, not the USB cable: switching "
                "its Wi-Fi off would cut the Mac off from it with no way to switch it back on. Its Wi-Fi was not "
                "touched; connect the cable and run it again")
    return None


def require_cable(device):
    """Raise CannotAnswer unless this Mac reaches the phone over the USB cable."""
    if not device:
        raise CannotAnswer("name the phone: --device ID or RICHOS_IOS_DEVICE")
    refused = cable_refusal(devicectl(["device", "info", "details"], device).get("connectionProperties") or {})
    if refused:
        raise CannotAnswer(refused)


def wifi_off(args):
    """`rios device wifi-off --out DIR`: over the cable only, read the phone's Wi-Fi switch, turn it off when on."""
    out = Path(args.out).resolve()
    out = storage_path(out)
    for name in ("RICHOS_IOS_DEVICE", "RICHOS_APPLE_TEAM"):
        if not os.environ.get(name):
            raise CannotAnswer(f"set {name}")
    result = switch_wifi_off(out, args.device or os.environ["RICHOS_IOS_DEVICE"])
    return emit(result, 0 if not result["error"] else 1)


def switch_wifi_off(out, device):
    """The cable check, then the switch read and turned off when on (ensure_wifi). Raises CannotAnswer off the cable,
    before Settings is opened."""
    require_cable(device)
    # ONE PHONE (hunt part 2 v3, V05): the step runner (_run, and the rios it starts) selects the phone from
    # RICHOS_IOS_DEVICE alone, so the phone whose cable was just checked is the one it is told to switch.
    # `--device A` with RICHOS_IOS_DEVICE=B used to check A's cable and turn B's Wi-Fi off.
    saved = os.environ.get("RICHOS_IOS_DEVICE")
    os.environ["RICHOS_IOS_DEVICE"] = device
    try:
        result = ensure_wifi(out, on=False)
    finally:
        if saved is None:
            os.environ.pop("RICHOS_IOS_DEVICE", None)
        else:
            os.environ["RICHOS_IOS_DEVICE"] = saved
    result["putBack"] = "rios device wifi-on --out DIR, then rios device net"
    return result


def run(args):
    """The list, then (when it touched Wi-Fi, however it ended) the Wi-Fi switch read and put back."""
    steps = load_steps(args.steps)
    args.phone_touched = False
    if switches_wifi(steps):
        require_cable(os.environ.get("RICHOS_IOS_DEVICE"))
    summary, failure = None, None
    try:
        summary = _run(args)
    except BaseException as error:  # noqa: BLE001: the Wi-Fi is put back first, then the error goes on
        failure = error
    wifi = None
    if touches_wifi(steps) and args.phone_touched:
        wifi = ensure_wifi_on(Path(args.out).resolve() / "wifi-restore")
        print(json.dumps({"wifiRestore": wifi}), file=sys.stderr)
    if failure is not None:
        raise failure
    summary["wifiRestore"] = wifi
    if wifi and wifi["error"]:
        summary["passed"] = False
        said = summary.get("error")
        summary["error"] = (said + " " if said else "") + \
            "The phone's Wi-Fi could not be confirmed on after this list: " + wifi["error"]
    return emit(summary, 0 if summary["passed"] else 1)


def wifi_restore(args):
    """`rios device wifi-restore --out DIR`: read the phone's Wi-Fi switch, turn it on when off."""
    out = Path(args.out).resolve()
    out = storage_path(out)
    for name in ("RICHOS_IOS_DEVICE", "RICHOS_APPLE_TEAM"):
        if not os.environ.get(name):
            raise CannotAnswer(f"set {name}")
    result = ensure_wifi_on(out)
    return emit(result, 0 if not result["error"] else 1)


def _run(args):
    started = time.time()
    steps = load_steps(args.steps)
    for name in ("RICHOS_IOS_DEVICE", "RICHOS_APPLE_TEAM"):
        if not os.environ.get(name):
            raise CannotAnswer(f"set {name}")
    out = Path(args.out).resolve()
    out = storage_path(out)
    if not 60 <= args.allowance <= 1800:
        raise CannotAnswer("--allowance must be 60 to 1800 seconds")
    too_short = allowance_refusal(steps, args.allowance)
    if too_short:
        raise CannotAnswer(too_short)
    identity =stamped_identity(args.stamp) if args.prebuilt else None
    device = os.environ["RICHOS_IOS_DEVICE"]
    ahead = forecast(device, passcode=passcode_state(device))
    pairing = pairs_by_link(steps)
    if ahead["approvalExpected"] is True and not args.approval_announced:
        raised, new, problem = raise_approval_escalation(device, ahead)
        raise Refusal(approval_refusal(ahead, raised, new, problem, pairing),
                      escalation=raised, escalationRaisedNow=new, escalationProblem=problem or None)
    if ahead["approvalExpected"] is True and pairing:
        raise Refusal("the CEO was told, but " + PAIRING_ORDER, approvalForecast=ahead)
    out.mkdir(parents=True, exist_ok=True)
    config = out / "script-config.json"
    config.unlink(missing_ok=True)
    fd = os.open(config, os.O_WRONLY | os.O_CREAT | os.O_EXCL, stat.S_IRUSR | stat.S_IWUSR)
    with os.fdopen(fd, "w") as f:
        json.dump({"isolatedLab": "true", "steps": json.dumps(steps), "allowanceSeconds": str(args.allowance),
                   **({"screenRecording": "true"} if getattr(args, "screen_recording", False) else {})}, f)
    env = {**os.environ, "RICHOS_MOBILE_TEST_CONFIG": str(config)}
    # --as-installed: the steps act on the test copy as it is installed (a production-push build, which the
    # development-signed products here would replace), so the runner alone goes to the phone, as for a list
    # that acts on no app. The runner finds the app by its bundle ID (PhysicalDeviceTests `app`).
    as_installed = bool(getattr(args, "as_installed", False))
    if as_installed and args.prebuilt:
        raise CannotAnswer("--as-installed hands the phone no app, so --prebuilt has nothing to reuse")
    runner_only = runner_only_for(steps, as_installed)
    if runner_only:
        env["RICHOS_PHYSICAL_RUNNER_ONLY"] = "1"
    else:
        env.pop("RICHOS_PHYSICAL_RUNNER_ONLY", None)
    if args.prebuilt:
        env["RICHOS_PHYSICAL_PREBUILT"] = "1"
        env["RICHOS_PHYSICAL_PRODUCTS"] = identity["products"]
        (out / "identity.json").write_text(json.dumps(identity, indent=1))
    args.phone_touched = True
    p = subprocess.run([str(RIOS), "device", "verify", "script"], capture_output=True, text=True, env=env)
    ran_to = time.time()
    config.unlink(missing_ok=True)
    (out / "rios.stdout").write_text(p.stdout)
    (out / "rios.stderr").write_text(p.stderr)
    text = p.stdout + p.stderr
    found = re.search(r'''(/[^\r\n"']+?/physical/script-(\d+)\.xcresult)''', text)
    if not found:
        record_session(device, started, ran_to, error=f"no result bundle (exit {p.returncode})")
        raise CannotAnswer(f"the check produced no result bundle (exit {p.returncode}); see {out}/rios.stderr")
    result, stamp = str(storage_path(found.group(1))), found.group(2)
    log = Path(result).parent / f"verify-script-{stamp}-test.log"
    rows = parse_log(log.read_text(errors="replace")) if log.exists() else []
    with open(out / "steps.jsonl", "w") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")
    attachments = out / "attachments"
    exported = subprocess.run(["xcrun", "xcresulttool", "export", "attachments", "--path", result,
                               "--output-path", str(attachments)], capture_output=True, text=True)
    failed = [r for r in rows if not r.get("ok") and not steps[r["i"]].get("optional")]
    session = session_from_log(log) if log.exists() else None
    record_session(device, started, ran_to, log if log.exists() else None, session)
    build = None
    for line in reversed(p.stdout.splitlines()):
        try:
            build = json.loads(line).get("result", {}).get("build")
            break
        except (json.JSONDecodeError, AttributeError):
            continue
    first = rows[0].get("start") if rows else None
    summary = {"passed": p.returncode == 0 and not failed and len(rows) == len(steps),
               "steps": len(steps), "logged": len(rows), "failed": failed[:1],
               "result": result, "testLog": str(log), "out": str(out),
               "attachments": str(attachments) if exported.returncode == 0 else None,
               "attachmentsError": None if exported.returncode == 0 else exported.stderr.strip()[-300:],
               "build": build, "approvalForecast": ahead, "runnerOnly": runner_only, "asInstalled": as_installed,
               # The command's start on this Mac's clock to the first step's start on the phone's
               # clock (both set from network time), and the runner's wait for automation to be allowed.
               "secondsToFirstStep": round(first - started, 1) if isinstance(first, (int, float)) else None,
               "automationEnableWaitSeconds": session and session["enableWaitSeconds"],
               "passcodeConfigured": session and session["passcodeConfigured"]}
    # A list that keeps a shot, tree or audit asked for those files; when their export fails they are not
    # in the output offered as evidence, so the run is not a pass (hunt part 2 v3, N07: the recording option
    # had this guard, an ordinary shot did not).
    if exported.returncode != 0 and any(s.get("do") in KEEPS_SCREEN for s in steps):
        summary["passed"] = False
        summary["error"] = ("the requested shots, trees or audits could not be exported from the result bundle: "
                            + (summary["attachmentsError"] or f"xcresulttool exit {exported.returncode}"))
    if getattr(args, "screen_recording", False):
        summary["screenRecordings"] = screen_recordings(attachments) if exported.returncode == 0 else []
        if not summary["screenRecordings"]:
            summary["passed"] = False
            summary["error"] = summary.get("error") or "--screen-recording: the result bundle holds no screen recording"
    if session and session["timedOut"]:
        summary["error"] = (f"'{APPROVAL_TIMEOUT}': the phone asked its owner to allow UI automation and "
                            "nobody approved it at the phone in about 60 s; no step ran")
    elif rows and args.approval_announced:
        # The phone ran a step, so UI automation was allowed: the approval
        # escalation this tool raised is answered by the fact, not by memory.
        summary["approvalEscalationsClosed"] = close_approval_escalations(os.getcwd(), str(log))
    return summary


VIDEO_SUFFIXES = (".mp4", ".mov", ".m4v")


def screen_recordings(attachments):
    """The screen recordings XCTest kept for a `--screen-recording` session (physical-device.mjs sets
    SystemAttachmentLifetime keepAlways), each with its manifest entry as exported."""
    try:
        manifest = json.loads((Path(attachments) / "manifest.json").read_text())
    except (OSError, ValueError):
        return []
    found = []
    for test in manifest if isinstance(manifest, list) else []:
        for item in test.get("attachments", []):
            name = str(item.get("exportedFileName", ""))
            if name.lower().endswith(VIDEO_SUFFIXES):
                found.append({**item, "file": str(Path(attachments) / name)})
    return found


def devicectl(args, device):
    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / "out.json"
        p = subprocess.run(["xcrun", "devicectl", *args, "--device", device, "--json-output", str(target)],
                           capture_output=True, text=True, timeout=120)
        if p.returncode != 0 or not target.exists():
            raise CannotAnswer(f"devicectl {' '.join(args)} failed: {(p.stderr or p.stdout).strip()[-200:]}")
        return json.loads(target.read_text()).get("result", {})


def procs(args):
    rows = devicectl(["device", "info", "processes"], args.device).get("runningProcesses", [])
    mine = [{"pid": r.get("processIdentifier"), "executable": r.get("executable")}
            for r in rows if str(r.get("executable", "")).rstrip("/").endswith("/" + args.name)]
    return emit({"name": args.name, "running": mine, "count": len(mine)})


def crash_summary(path):
    """An .ips crash report is one JSON header line, then the JSON body: the exception type, the
    termination reason and the crashed thread's first frames (image and symbol where present)."""
    text = Path(path).read_text(errors="replace")
    head, _, rest = text.partition("\n")
    try:
        header = json.loads(head)
    except ValueError:
        header = {}
    try:
        body = json.loads(rest)
    except ValueError:
        return {"bugType": header.get("bug_type"), "time": header.get("timestamp"), "readable": False}
    images = body.get("usedImages", [])
    frames = []
    for thread in body.get("threads", []):
        if not thread.get("triggered"):
            continue
        for f in thread.get("frames", [])[:12]:
            image = images[f["imageIndex"]].get("name") if isinstance(f.get("imageIndex"), int) and \
                f["imageIndex"] < len(images) else None
            frames.append(" ".join(str(x) for x in (image, f.get("symbol"), f.get("sourceFile"),
                                                    f.get("sourceLine")) if x is not None) or "(no image or symbol)")
    exception = body.get("exception", {})
    termination = body.get("termination", {})
    return {"bugType": header.get("bug_type"), "time": header.get("timestamp"),
            "app": header.get("app_name"), "version": header.get("app_version"),
            "build": header.get("build_version"),
            "exception": " ".join(str(exception.get(k)) for k in ("type", "signal", "subtype") if exception.get(k)),
            "termination": " ".join(str(termination.get(k)) for k in ("namespace", "indicator") if termination.get(k)),
            "crashedThread": frames,
            "largestProcess": body.get("largestProcess")}


def crashes(args):
    listed = devicectl(["device", "info", "files", "--domain-type", "systemCrashLogs"], args.device)
    rows = listed.get("files", [])
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    found = []
    for row in rows:
        name = str(row.get("relativePath") or row.get("name") or "")
        base = name.rsplit("/", 1)[-1]
        if not base.endswith(".ips") or not (args.name in base or base.startswith("JetsamEvent")):
            continue
        stamp = re.search(r"(\d{4}-\d{2}-\d{2}-\d{6})", base)
        when = time.mktime(time.strptime(stamp.group(1), "%Y-%m-%d-%H%M%S")) if stamp else None
        if args.since is not None and (when is None or when < args.since):
            continue
        target = out / base
        p = subprocess.run(["xcrun", "devicectl", "device", "copy", "from", "--device", args.device,
                            "--domain-type", "systemCrashLogs", "--source", name, "--destination", str(target)],
                           capture_output=True, text=True, timeout=120)
        if p.returncode != 0 or not target.exists():
            found.append({"file": base, "copied": False, "error": (p.stderr or p.stdout).strip()[-200:]})
            continue
        found.append({"file": str(target), "copied": True, "at": when, **crash_summary(target)})
    if not rows:
        raise CannotAnswer("devicectl listed no files in systemCrashLogs (unexpected shape: "
                           f"{sorted(listed)[:8]})")
    return emit({"name": args.name, "since": args.since, "listed": len(rows), "reports": found})


def apps(args):
    rows = devicectl(["device", "info", "apps"], args.device).get("apps", [])
    ours = [{"bundle": r.get("bundleIdentifier"), "version": r.get("version"), "build": r.get("bundleVersion")}
            for r in rows if str(r.get("bundleIdentifier", "")).startswith(OURS)]
    return emit({"installed": ours})


def lock(args):
    p = subprocess.run(["xcrun", "devicectl", "device", "info", "lockState", "--device", args.device],
                       capture_output=True, text=True, timeout=60)
    required = re.search(r"passcodeRequired:\s*(\w+)", p.stdout)
    if p.returncode != 0 or not required:
        raise CannotAnswer("devicectl did not report a lock state")
    # passcodeRequired is "locked right now", NOT "has a passcode": passcodeConfigured is that
    # (null when the phone would not say; it needs the hardware UDID).
    return emit({"passcodeRequired": required.group(1) == "true",
                 "unlockedSinceBoot": "unlockedSinceBoot: true" in p.stdout,
                 "passcodeConfigured": passcode_state(args.device)})


def libimobile_id(device):
    """libimobiledevice (ideviceinfo, idevicesyslog) knows the phone only by its hardware UDID; `rios device`
    hands every verb the CoreDevice UUID. A spelling with no hardware UDID is passed on as it is."""
    try:
        return hardware_udid(device)
    except CannotAnswer:
        return device


def battery(args):
    p = subprocess.run(["ideviceinfo", "-u", libimobile_id(args.device), *(["-n"] if args.network else []), "-q", "com.apple.mobile.battery"],
                       capture_output=True, text=True, timeout=60)
    values = dict(line.split(": ", 1) for line in p.stdout.splitlines() if ": " in line)
    if p.returncode != 0 or "BatteryCurrentCapacity" not in values:
        raise CannotAnswer("ideviceinfo did not report the battery (it needs the hardware UDID)")
    return emit({"percent": int(values["BatteryCurrentCapacity"]), "charging": values.get("BatteryIsCharging") == "true",
                 "externalPower": values.get("ExternalConnected") == "true", "full": values.get("FullyCharged") == "true"})


# The lines of the phone's log that say why iOS did or did not open the app: the developer-trust
# check (online-auth-agent asks ppq.apple.com; misagent holds the profiles), the VPN manager and the
# Tailscale extension, the app's own launch, and the DNS resolver's word on whether its server
# answers (names in those lines are hashed by iOS). Everything else on the phone is dropped unread.
TRUST_KEEP = re.compile(
    r"online-auth-agent|misagent|ppq\.apple|verify trust|Unable to Verify|not trusted|signature state|"
    r"nesessionmanager|NEVPN|IPNExtension|io\.tailscale|RichOSNative|dev\.richos\.connect|"
    r"mDNSResponder.*(?:Penalizing unresponsive server|Received acceptable|assigned DNS service --)", re.I)
DNS_SERVICE = re.compile(r"assigned DNS service -- id: \d+, type: (\w+), source: (\w+), scope: \w+, interface: ([\w/]+)")
VPN_STATUS = re.compile(r"NESMVPNSession\[(?:Primary Tunnel:)?([^:\]]+)[^\]]*\].*? status (\w+)")


def trust_reading(lines):
    """What the log lines of one launch say about iOS's developer-trust check, as facts and one cause.
    Pure: the lines in, a dict out (tested without a phone, qa/trust-reading.test.py)."""
    text = "\n".join(lines)
    state = re.search(r"signature state: ([^,\]]+)", text)
    errors = sorted({int(c) for c in re.findall(r"online-auth-agent.*finished with error \[(-?\d+)\]", text)})
    server = DNS_SERVICE.search(text)
    vpn = {}
    for name, status in VPN_STATUS.findall(text):
        vpn[name.strip()] = status
    reading = {
        "signatureState": state.group(1).strip() if state else None,
        "verifyRequests": len(re.findall(r"online-auth-agent.*Sending request for", text)),
        "verifyErrors": errors,
        "dnsStalls": len(re.findall(r"online-auth-agent.*reported DNS stall symptom", text)),
        "dns": {"type": server.group(1), "source": server.group(2), "interface": server.group(3)} if server else None,
        "dnsServerUnanswered": len(re.findall(r"Penalizing unresponsive server", text)),
        "dnsServerAnswered": len(re.findall(r"Received acceptable", text)),
        "vpn": vpn,
    }
    connected = [n for n, s in vpn.items() if s.lower() in ("connected", "connecting", "reasserting")]
    if connected:
        cause = (f"a VPN is up ({', '.join(connected)}) and the trust check could not reach Apple through it "
                 "(NSURLError " + ", ".join(map(str, errors)) + ")") if errors else f"a VPN is up ({', '.join(connected)})"
    elif -1009 in errors:
        cause = "the phone reports no internet connection (NSURLError -1009)"
    elif reading["dnsStalls"] or (-1001 in errors and reading["dnsServerUnanswered"]):
        iface = server.group(3).split("/")[0] if server else ""
        where = (f" (plain {server.group(1)} on {iface}" + (", the phone's Wi-Fi)" if iface == "en0" else ")")) if server else ""
        cause = ("the phone's DNS server" + where + " did not answer in time, so the trust check timed out before "
                 "it reached ppq.apple.com and iOS refused the app")
    elif errors:
        cause = "the trust check's request to Apple failed (NSURLError " + ", ".join(map(str, errors)) + ")"
    elif reading["signatureState"]:
        cause = f"iOS refused the launch: {reading['signatureState']}"
    else:
        cause = None
    reading["cause"] = cause
    return reading


def _profiles(udid):
    """The provisioning profiles installed on the phone (ideviceprovision copy: read, never changed)."""
    import plistlib
    with tempfile.TemporaryDirectory() as tmp:
        p = subprocess.run(["ideviceprovision", "-u", udid, "copy", tmp], capture_output=True, text=True, timeout=60)
        if p.returncode != 0:
            raise CannotAnswer(f"ideviceprovision did not list the profiles: {(p.stderr or p.stdout).strip()[-200:]}")
        rows = []
        for f in sorted(Path(tmp).glob("*.mobileprovision")):
            d = subprocess.run(["security", "cms", "-D", "-i", str(f)], capture_output=True, timeout=30)
            try:
                doc = plistlib.loads(d.stdout)
            except Exception:  # noqa: BLE001: an unreadable profile is reported, never fatal
                rows.append({"file": f.name, "unreadable": True})
                continue
            rows.append({"uuid": doc.get("UUID"), "name": doc.get("Name"), "team": (doc.get("TeamIdentifier") or [None])[0],
                         "app": (doc.get("Entitlements") or {}).get("application-identifier"),
                         "expires": doc.get("ExpirationDate").isoformat() + "Z" if doc.get("ExpirationDate") else None,
                         "expired": bool(doc.get("ExpirationDate")) and doc["ExpirationDate"].timestamp() < time.time(),
                         "listsThisPhone": udid.upper() in [str(x).upper() for x in doc.get("ProvisionedDevices") or []]})
        return rows


def trust(args):
    """Why iOS will or will not open RichConnect: Developer Mode, the phone's provisioning profiles,
    and one launch of the installed app with the trust, DNS and VPN log lines during it, read into
    one cause (trust_reading). Reads only: nothing is installed, removed or switched. The capture
    stays in --out in selected storage; it is the phone's own log and never enters a repository."""
    sys.path.insert(0, str(ROOT / "richos/mobile"))
    import phone_net
    out = Path(args.out)
    out = storage_path(out)
    with tempfile.TemporaryDirectory() as tmp:
        listed = Path(tmp) / "d.json"
        subprocess.run(["xcrun", "devicectl", "list", "devices", "--json-output", str(listed)], capture_output=True, timeout=60)
        devices = json.loads(listed.read_text())["result"]["devices"] if listed.exists() else []
    want = args.device.lower()
    phone = next((d for d in devices if want in (str(d.get("identifier")).lower(),
                                                 str(d.get("hardwareProperties", {}).get("udid")).lower())), None)
    if not phone:
        raise CannotAnswer("that iPhone is not in `xcrun devicectl list devices`")
    udid = phone["hardwareProperties"]["udid"]
    props = phone.get("deviceProperties", {})
    report = {"ios": props.get("osVersionNumber"), "developerMode": props.get("developerModeStatus"),
              "pairing": phone.get("connectionProperties", {}).get("pairingState")}
    try:
        report["profiles"] = _profiles(udid)
    except (CannotAnswer, OSError, subprocess.TimeoutExpired) as error:
        report["profiles"] = {"error": str(error)}
    installed = devicectl(["device", "info", "apps"], args.device).get("apps", [])
    app = next((a for a in installed if a.get("bundleIdentifier") == phone_net.PACKAGE), None)  # the test copy
    report["app"] = {"version": app.get("version"), "build": app.get("bundleVersion")} if app else None
    relay = spawn_owned(["idevicesyslog", "-u", udid, "--no-colors"], stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, text=True, errors="replace")
    kept = []
    try:
        import selectors
        import threading
        result = {}
        launcher = threading.Thread(target=lambda: result.update(phone_net.launch_check(args.device)))
        time.sleep(1.0)
        launcher.start()
        sel = selectors.DefaultSelector()
        sel.register(relay.stdout, selectors.EVENT_READ)
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline or launcher.is_alive():
            if not sel.select(timeout=0.5):
                if relay.poll() is not None:
                    break
                continue
            line = relay.stdout.readline()
            if not line:
                break
            if TRUST_KEEP.search(line):
                kept.append(line.rstrip("\n"))
        launcher.join()
    finally:
        relay.terminate()
        try:
            relay.wait(timeout=5)
        except subprocess.TimeoutExpired:
            relay.kill()
            relay.wait()
    out.write_text("\n".join(kept) + "\n")
    report["launch"] = result
    report["reading"] = trust_reading(kept)
    if result.get("state") == "ok":
        report["reading"]["cause"] = None
    elif result.get("state") == "untrusted":
        # The fix the automation applies on its own (CEO 2026-10-02): the phone has no passcode, so a
        # restart brings it back usable, and on 2026-10-02 a restart cleared the DNS that did not answer.
        report["remedy"] = (f"rios device reboot --device {args.device}: restarts the phone and waits for it "
                            "(it opens no app; `rios device launch` opens one once afterwards); rios device run, verify, perf and hold do this once on their own "
                            "before they give up")
    report["log"] = {"out": str(out), "lines": len(kept)}
    return emit(report, 0 if result.get("state") == "ok" else 1)


# `rios device net`: what the phone itself can reach, measured, before anyone says anything about its "Wi-Fi"
# (CEO 2026-10-03: check the internet connection before any claim about the Wi-Fi). Two readings:
#   wifiPath  does this Mac reach the phone's own Wi-Fi address (phone_net.wifi_path: its Bonjour address,
#             three pings from the Mac)? No answer means the phone's Wi-Fi carries no traffic at all.
#   internet  Safari (a system app: no developer trust check, so it opens while RichConnect is refused)
#             is opened once and closed again (the Home Screen is left in front); every TCP connection it
#             makes to the internet in that window is read from the phone's own network log: completed, or
#             timed out. Safari may load its open tab rather than NET_URL; any internet host serves.
NET_URL = "http://captive.apple.com/hotspot-detect.html"
NET_KEEP = re.compile(r" (?:MobileSafari|com\.apple\.WebKit\.Networking|mDNSResponder)[\[(]|"
                      r" wifid[\[(].*(?:LogStats|[Pp]ower|[Aa]ssociat|[Ll]ink (?:up|down)|AUTO-JOIN: Join |Joining network|4WayHS|4 ?-?way|EAPOL|"
                      r"[Dd]eauth|[Dd]isassoc|[Rr]eason|WPA|SAE|PMF|[Ll]inkDown|LinkChange)|"
                      r" kernel[\[(].*(?:[Dd]eauth|[Dd]isassoc|[Rr]eason|4 ?-?way|EAPOL|[Ll]ink [Dd]own|[Ll]ink [Uu]p)|"
                      r" configd\[.*(?:\d\. \w+ serviceID=\S+ addr=|primary IPv4|network changed)")
# The processes a Safari load runs in. Only THEIR completed connections say the phone reached the internet: in the
# same window devicectl's own services (mobile_storage_proxy, dtappserviced) connect to this Mac over the
# developer tunnel (a utun interface), and on 2026-10-03 those four were the "tcpConnected: 4" of a phone
# whose every internet connect timed out.
BROWSER = ("MobileSafari", "com.apple.WebKit.Networking")
CONNECT_EVENT = re.compile(r" ([^\s(\[]+)(?:\([^)]*\))?\[\d+\] <\w+>: \[C[\d.]+ (\S+) ([^\]]*)\] "
                           r"event: flow:(finish_connect|failed_connect)")
# configd's ranking of the phone's network services and its election: which interface carries the default route.
SERVICE_RANK = re.compile(r"\d+\. (\w+) serviceID=(\S+) addr=(\S+) rank=")
PRIMARY_V4 = re.compile(r"(\S+) is (?:still |the new )?primary IPv4")
WIFI_INTERFACE = "en0"  # the iPhone's Wi-Fi


def net_reading(lines, leases=""):
    """Safari's TCP connections in one window, and which interface the phone sends its internet over, read from
    the phone's log lines. `leases` is this Mac's DHCP lease file (Internet Sharing): when the phone's primary
    address is in it, the phone's internet goes over the cable to this Mac. Pure: text in, a dict out (tested
    without a phone, qa/trust-reading.test.py)."""
    text = "\n".join(lines)
    browser_lines = "\n".join(l for l in lines if re.search(r" (?:%s)[\[(]" % "|".join(map(re.escape, BROWSER)), l))
    # Safari's own word: its page's responses (CFNetwork's "received response" or WebKit's httpStatusCode) and
    # whether the main frame finished or failed loading. Other processes' responses (cloudd, a widget) are not
    # Safari loading a page, so a --raw capture does not count them.
    statuses = sorted({int(s) for s in re.findall(r"received response, status (\d{3})|httpStatusCode=(\d{3})",
                                                   browser_lines) for s in s if s})
    loaded = len(re.findall(r"didFinishLoadForFrame: [^\n]*isMainFrame=1", browser_lines))
    failed = len(re.findall(r"didFail(?:Provisional)?LoadForFrame[^\n]*isMainFrame=1", browser_lines))
    events = [(proc, peer, (re.search(r"interface: (\w+)", inside) or [None, None])[1], kind)
              for proc, peer, inside, kind in CONNECT_EVENT.findall(text)]
    connected = [e for e in events if e[0] in BROWSER and e[3] == "finish_connect"]
    others = {}
    for proc, _, iface, kind in events:
        if proc not in BROWSER and kind == "finish_connect":
            key = f"{proc} via {iface or '?'}"
            others[key] = others.get(key, 0) + 1
    timed_via = {}
    for line in re.findall(r"[^\n]*event: flow:failed_connect @[\d.]+s, error Operation timed out", text):
        # Only Safari's own timeouts are the phone's internet timing out, the same filter as the completed
        # connections above: a developer service's timeout over the tunnel is not (hunt part 2 v3, V06).
        event = CONNECT_EVENT.search(line)
        if not event or event.group(1) not in BROWSER:
            continue
        iface = re.search(r"interface: (\w+)", line)
        key = iface.group(1) if iface else "?"
        timed_via[key] = timed_via.get(key, 0) + 1
    return {"reached": bool(statuses) or loaded > 0 or len(connected) > 0, "tcpConnected": len(connected),
            "connectedVia": sorted({e[2] or "?" for e in connected}), "statuses": statuses,
            "pageLoaded": loaded, "pageFailed": failed,
            "notInternet": others,
            "tcpTimedOut": sum(timed_via.values()), "timedOutVia": timed_via,
            "dnsStalls": len(re.findall(r"reported DNS stall symptom", text)),
            "dnsServerUnanswered": len(re.findall(r"Penalizing unresponsive server", text)),
            "noNetworkRoute": len(re.findall(r"unsatisfied \(No network route\)", text)),
            "primary": primary_route(text, leases),
            "wifiLink": wifi_link(text)}


def primary_route(text, leases=""):
    """The interface iOS last elected for IPv4 (configd), its address, and whether this Mac's Internet Sharing
    leased that address (then the phone's internet runs over the cable to this Mac). None when configd said
    nothing in the window."""
    services = {sid: (iface, addr) for iface, sid, addr in SERVICE_RANK.findall(text)}
    elected = PRIMARY_V4.findall(text)
    if not elected or elected[-1] not in services:
        return None
    iface, addr = services[elected[-1]]
    lease = re.search(r"ip_address=" + re.escape(addr) + r"\s", leases or "")
    return {"interface": iface, "address": addr, "wifi": iface == WIFI_INTERFACE,
            "leasedByThisMac": bool(lease)}


def wifi_link(text):
    """The phone's own last Wi-Fi link report (wifid's LogStats line), or None when it wrote none."""
    stats = re.findall(r"InfraUptime:([\d.]+)secs Channel: (\d+).*?Rssi: (-?\d+).*?Snr: (-?\d+) BcnPer: ([\d.]+)%", text)
    if not stats:
        return None
    up, channel, rssi, snr, beacons = stats[-1]
    return {"joinedSeconds": float(up), "channel": int(channel), "rssi": int(rssi), "snr": int(snr),
            "beaconLossPercent": float(beacons)}


def wifi_carries(path_ok, internet):
    """The exit-0 claim of `net`: this Mac reaches the phone's Wi-Fi address, Safari reached the internet, and
    that internet did not go out over another interface. A primary route iOS names as not its Wi-Fi (the cable, a
    second interface) is never a Wi-Fi pass, whatever Safari reached that way (hunt part 2 v3, V06). Pure."""
    primary = internet.get("primary")
    return bool(path_ok and internet["reached"] and not (primary and not primary["wifi"]))


def net_verdict(path_ok, internet):
    """One sentence from the two readings, each claim only from what was measured: "reaches the internet" needs a
    completed connection (or a response), "time out" needs a recorded timeout. Pure (qa/trust-reading.test.py)."""
    timed_out = internet.get("tcpTimedOut")
    primary = internet.get("primary")
    if primary and not primary["wifi"]:
        where = ("the cable to this Mac (this Mac's Internet Sharing leased it that address)"
                 if primary["leasedByThisMac"] else "an interface that is not its Wi-Fi")
        outcome = ("and Safari reached the internet that way" if internet["reached"] else
                   "and Safari's connections that way timed out" if timed_out else
                   "and Safari's internet was not measured")
        return (f"the phone sends its internet over {primary['interface']} ({primary['address']}), {where}, not its "
                f"Wi-Fi, {outcome}")
    if internet["reached"]:
        if path_ok:
            return "the phone's Wi-Fi carries traffic: this Mac reaches it and it reaches the internet"
        return "the phone reaches the internet, but this Mac does not reach its Wi-Fi address"
    if internet.get("noNetworkRoute") and not timed_out:
        return ("the phone has no network at all: it is not joined to Wi-Fi (iOS: No network route), so this Mac "
                "gets no answer from it and it reaches nothing")
    mac = ("this Mac reaches the phone over Wi-Fi" if path_ok
           else "this Mac gets no answer from the phone's Wi-Fi address")
    if timed_out:
        return (f"{mac}, and the phone's own connections to the internet time out"
                " (the phone's Wi-Fi carries no traffic)" if not path_ok else
                f"{mac}, but the phone's connections to the internet did not complete (TCP timed out)")
    return f"{mac}; the phone's internet was not measured (no connection completed or timed out in the window)"


def _safari_load(device, udid, url, seconds, keep_all=False):
    """Open url in Safari on the phone once, keep its network log lines (every line with keep_all) for
    `seconds`, close Safari."""
    import selectors
    relay = spawn_owned(["idevicesyslog", "-u", udid, "--no-colors"], stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, text=True, errors="replace")
    kept, pid, opened = [], None, False
    try:
        time.sleep(1.0)
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "launch.json"
            p = subprocess.run(["xcrun", "devicectl", "device", "process", "launch", "--device", device,
                                "--terminate-existing", "--payload-url", url, "--json-output", str(target),
                                "com.apple.mobilesafari"], capture_output=True, text=True, timeout=60)
            opened = p.returncode == 0
            if opened and target.exists():
                pid = ((json.loads(target.read_text()).get("result") or {}).get("process") or {}).get(
                    "processIdentifier")
        sel = selectors.DefaultSelector()
        sel.register(relay.stdout, selectors.EVENT_READ)
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if not sel.select(timeout=0.5):
                if relay.poll() is not None:
                    break
                continue
            line = relay.stdout.readline()
            if not line:
                break
            if keep_all or NET_KEEP.search(line):
                kept.append(line.rstrip("\n"))
    finally:
        relay.terminate()
        try:
            relay.wait(timeout=5)
        except subprocess.TimeoutExpired:
            relay.kill()
            relay.wait()
        if pid:
            subprocess.run(["xcrun", "devicectl", "device", "process", "terminate", "--device", device,
                            "--pid", str(pid)], capture_output=True, timeout=30)
    return opened, kept


def net(args):
    """`rios device net --device ID --out FILE`: the two readings and one verdict. Exit 0 when the phone's
    Wi-Fi carries traffic both ways, 1 otherwise. The phone's kept log lines go to FILE (in selected storage).
    Nothing is installed, removed or switched."""
    sys.path.insert(0, str(ROOT / "richos/mobile"))
    import phone_net
    out = Path(args.out)
    out = storage_path(out)
    udid = hardware_udid(args.device)
    path_ok, path_detail = phone_net.wifi_path(args.device)
    opened, kept = _safari_load(args.device, udid, args.url, args.seconds, args.raw)
    try:
        leases = Path("/var/db/dhcpd_leases").read_text(errors="replace")
    except OSError:
        leases = ""
    internet = {"safariOpened": opened, **net_reading(kept, leases)}
    out.write_text("\n".join(kept) + "\n")
    ok = wifi_carries(path_ok, internet)
    return emit({"wifiPath": {"ok": path_ok, "detail": path_detail}, "internet": internet,
                 "verdict": net_verdict(path_ok, internet), "log": str(out)}, 0 if ok else 1)


def close_app(args):
    """`rios device close BUNDLE --device ID`: end the app's running process(es) so the phone is left at its
    Home Screen. Nothing is installed, removed or cleared; an app that is not running is said, exit 0."""
    sys.path.insert(0, str(ROOT / "richos/mobile/perf"))
    import test_copy
    try:
        test_copy.refuse_ceo_app(args.bundle, "`rios device close`")
    except test_copy.CeoAppRefused as refused:
        return emit({"error": str(refused)}, 2)
    rows = devicectl(["device", "info", "processes"], args.device).get("runningProcesses", [])
    apps = devicectl(["device", "info", "apps", "--bundle-id", args.bundle], args.device).get("apps", [])
    path = str((apps[0] if apps else {}).get("url", "")).replace("file://", "").rstrip("/")
    if not path:
        raise CannotAnswer(f"{args.bundle} is not installed on the phone")
    pids = [r.get("processIdentifier") for r in rows
            if str(r.get("executable", "")).replace("file://", "").startswith(path + "/")]
    for pid in pids:
        subprocess.run(["xcrun", "devicectl", "device", "process", "terminate", "--device", args.device,
                        "--pid", str(pid)], capture_output=True, timeout=30)
    return emit({"bundle": args.bundle, "closed": pids})


def launch_app(args):
    """`rios device launch BUNDLE [-- app args]`: the one procedure for opening an installed app
    (phone_net.launch_app): check connected and on Wi-Fi first, open once, on a trust refusal restart the
    phone once, wait, open once more, else print the exact error. The app's console goes to stdout."""
    sys.path.insert(0, str(ROOT / "richos/mobile"))
    import phone_net
    device = args.device or os.environ.get("RICHOS_IOS_DEVICE")
    if not device:
        return emit({"error": "name the phone: --device ID or RICHOS_IOS_DEVICE"}, 2)
    rest = args.app_args[1:] if args.app_args[:1] == ["--"] else args.app_args
    try:
        code, result = phone_net.launch_app(device, args.bundle, rest,
                                            lambda line: print(line, file=sys.stderr, flush=True),
                                            console=not args.detach)
    except phone_net.test_copy.CeoAppRefused as refused:  # the CEO's own app: nothing was asked of the phone
        return emit({"error": str(refused)}, 2)
    if result["state"] == "not-ready":
        print(result["detail"], file=sys.stderr)
    elif code:
        print("iOS would not open " + args.bundle + ": " + result["detail"], file=sys.stderr)
    print(json.dumps(result), file=sys.stderr)
    return code


def restart_phone(args):
    """`rios device reboot --device ID`: restart the phone through devicectl and wait until it is connected
    again and reaches the internet (phone_net.reboot). It opens NO app, ever (CEO 2026-10-04: RichConnect
    shows nothing after a reboot). Exit 0 when the phone is back and online. Opening an app that iOS will
    not verify is `rios device launch`'s one procedure, not this verb's.
    Nothing is installed, removed or erased; the phone has no passcode, so it comes back usable."""
    sys.path.insert(0, str(ROOT / "richos/mobile"))
    import phone_net
    result = phone_net.reboot(args.device, lambda line: print(line, file=sys.stderr, flush=True))
    return emit(result, 0 if result.get("state") in ("ok", "skipped") else 1)


SYSLOG_KEEP = ("RichOSNative", "dev.richos.connect")

# A child this tool starts must never outlive it. SIGTERM/SIGHUP are turned into a normal exit so
# the `finally:` runs; SIGKILL cannot be caught, so a watchdog (its own process, in its own session)
# polls this tool's pid and kills the child the moment the tool is gone.
WATCHDOG = (
    "import os,sys,time,signal\n"
    "parent,child=int(sys.argv[1]),int(sys.argv[2])\n"
    "def alive(p):\n"
    "    try: os.kill(p,0)\n"
    "    except ProcessLookupError: return False\n"
    "    except PermissionError: return True\n"
    "    return True\n"
    "while alive(parent) and alive(child): time.sleep(0.5)\n"
    "if alive(child):\n"
    "    try: os.kill(child,signal.SIGKILL)\n"
    "    except ProcessLookupError: pass\n"
)


def spawn_owned(argv, **kwargs):
    """Popen that ends with this process on exit, SIGTERM, SIGINT, SIGHUP or SIGKILL."""
    def leave(signum, frame):
        raise SystemExit(128 + signum)
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, leave)
    child = subprocess.Popen(argv, **kwargs)
    subprocess.Popen([sys.executable, "-c", WATCHDOG, str(os.getpid()), str(child.pid)],
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=True)
    return child


def syslog(args):
    """The phone's own log for RichConnect only, for a bounded interval. Every other line on a
    person's phone is dropped before it reaches disk; the relay process is owned and stopped here."""
    if not 1 <= args.seconds <= 3600:
        raise CannotAnswer("--seconds must be 1 to 3600")
    out = Path(args.out)
    out = storage_path(out)
    # --keep TEXT adds a key: a system line the phone's condition is read from (backboardd's HID
    # client lines), named explicitly, never the whole log. A key shorter than 6 characters would keep
    # nearly every line, so it is refused.
    extra = tuple(getattr(args, "keep", None) or ())
    short = [k for k in extra if len(k) < 6]
    if short:
        raise CannotAnswer(f"--keep needs at least 6 characters (got {short[0]!r}): a short key keeps the whole log")
    keys = SYSLOG_KEEP + extra
    relay = spawn_owned(["idevicesyslog", "-u", libimobile_id(args.device), *(["-n"] if args.network else []), "--no-colors"],
                             stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, text=True, errors="replace")
    kept = total = 0
    deadline = time.monotonic() + args.seconds
    try:
        import selectors
        sel = selectors.DefaultSelector()
        sel.register(relay.stdout, selectors.EVENT_READ)
        with open(out, "w") as f:
            while time.monotonic() < deadline:
                if not sel.select(timeout=max(0.0, min(1.0, deadline - time.monotonic()))):
                    if relay.poll() is not None:
                        break
                    continue
                line = relay.stdout.readline()
                if not line:
                    break
                total += 1
                if any(key in line for key in keys):
                    f.write(line)
                    kept += 1
    finally:
        relay.terminate()
        try:
            relay.wait(timeout=5)
        except subprocess.TimeoutExpired:
            relay.kill()
            relay.wait()
    if total == 0:
        raise CannotAnswer("the phone's log relay produced nothing (is the hardware UDID attached?)")
    return emit({"out": str(out), "kept": kept, "read": total, "keys": list(keys), "relayPid": relay.pid,
                 "relayExit": relay.returncode})


SYSLOG_ENTRY = re.compile(r"^\w{3} +\d+ (\d\d):(\d\d):(\d\d)(?:\.\d+)? (\S+)\[(\d+)\]")


def syslog_rate(path, process, bucket, start=None, end=None):
    """How many log entries one process wrote per `bucket` seconds of the phone's clock, from a
    `syslog` capture: the battery reading of a window (an app that does nothing while hidden writes
    nothing). Only the entry's first line counts; continuation lines carry no header. The process
    is matched exactly (`RichOSNative` is not `RichOSNativeUITests-Runner`)."""
    try:
        lines = Path(path).read_text(errors="replace").splitlines()
    except OSError as error:
        raise CannotAnswer(f"cannot read the log capture {path}: {error}")
    if not 1 <= bucket <= 3600:
        raise CannotAnswer("--bucket must be 1 to 3600 seconds")

    def seconds(clock):
        h, m, s = (int(x) for x in clock.split(":"))
        return h * 3600 + m * 60 + s

    lo, hi = (seconds(start) if start else None), (seconds(end) if end else None)
    counts, entries, matched = {}, 0, 0
    for line in lines:
        found = SYSLOG_ENTRY.match(line)
        if not found:
            continue
        entries += 1
        name = found.group(4).split("(")[0]
        if name != process:
            continue
        at = int(found.group(1)) * 3600 + int(found.group(2)) * 60 + int(found.group(3))
        if (lo is not None and at < lo) or (hi is not None and at >= hi):
            continue
        matched += 1
        key = at - at % bucket
        counts[key] = counts.get(key, 0) + 1
    if entries == 0:
        raise CannotAnswer(f"{path} holds no log entries in the idevicesyslog shape: is it a phone-ios.py syslog capture?")
    rows = [{"at": f"{k // 3600:02d}:{k % 3600 // 60:02d}:{k % 60:02d}", "entries": v} for k, v in sorted(counts.items())]
    return {"process": process, "bucketSeconds": bucket, "entries": matched, "ofAllEntries": entries, "buckets": rows}


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check").add_argument("steps")
    r = sub.add_parser("run")
    r.add_argument("steps")
    r.add_argument("--out", required=True)
    r.add_argument("--allowance", type=int, default=240)
    r.add_argument("--prebuilt", action="store_true")
    r.add_argument("--stamp")
    r.add_argument("--approval-announced", action="store_true",
                   help="the CEO was told before this run that the phone will ask him to allow UI automation")
    r.add_argument("--screen-recording", action="store_true",
                   help="keep XCTest's recording of the phone's screen for the whole session (recorded on the "
                        "phone); the summary names it under screenRecordings, and its absence fails the run")
    r.add_argument("--as-installed", action="store_true",
                   help="act on the test copy already installed (e.g. by `rios device install --push production`): "
                        "the test runner alone is handed to the phone, never an app, so that build stays as it is")
    sub.add_parser("wifi-restore").add_argument("--out", required=True)
    sub.add_parser("wifi-on").add_argument("--out", required=True)
    wo = sub.add_parser("wifi-off")
    wo.add_argument("--out", required=True)
    wo.add_argument("--device", help="the phone (default RICHOS_IOS_DEVICE)")
    sub.add_parser("parse-log").add_argument("log")
    sub.add_parser("approval").add_argument("--device", required=True)
    sub.add_parser("summary").add_argument("dir")
    sub.add_parser("vocabulary").add_argument("runner", nargs="?", default=str(RUNNER))
    sr = sub.add_parser("syslog-rate")
    sr.add_argument("file")
    sr.add_argument("--process", default="RichOSNative")
    sr.add_argument("--bucket", type=int, default=10)
    sr.add_argument("--from", dest="start", help="HH:MM:SS on the phone's clock, inclusive")
    sr.add_argument("--to", dest="end", help="HH:MM:SS on the phone's clock, exclusive")
    ps = sub.add_parser("pair-steps")
    ps.add_argument("config")
    ps.add_argument("--v2-hold", type=int, default=None)
    la = sub.add_parser("launch")
    la.add_argument("bundle")
    la.add_argument("--device")
    la.add_argument("--detach", action="store_true",
                    help="return once iOS has opened the app (no console); the app stays open in front")
    la.add_argument("app_args", nargs="*")
    cl = sub.add_parser("close")
    cl.add_argument("bundle")
    cl.add_argument("--device", required=True)
    cr = sub.add_parser("crashes")
    cr.add_argument("--device", required=True)
    cr.add_argument("--out", required=True)
    cr.add_argument("--name", default="RichOSNative")
    cr.add_argument("--since", type=float, default=None,
                    help="only reports whose file-name time (the phone's clock) is at or after this epoch")
    for name in ("procs", "apps", "lock", "battery", "syslog", "trust", "reboot", "net"):
        s = sub.add_parser(name)
        s.add_argument("--device", required=True)
        if name == "net":
            s.add_argument("--out", required=True)
            s.add_argument("--seconds", type=float, default=15.0)
            s.add_argument("--url", default=NET_URL)
            s.add_argument("--raw", action="store_true",
                           help="keep every line the phone logs in the window, not only the network ones")
        if name == "procs":
            s.add_argument("--name", default="RichOSNative")
        if name == "trust":
            s.add_argument("--out", required=True)
            s.add_argument("--seconds", type=float, default=12.0)
        if name == "syslog":
            s.add_argument("--seconds", type=float, required=True)
            s.add_argument("--out", required=True)
            s.add_argument("--keep", action="append", default=[],
                           help="also keep lines containing TEXT (at least 6 characters; repeatable), e.g. "
                                "IOHIDEventSystem for backboardd's HID client lines")
        if name in ("syslog", "battery"):
            # Unplugged (round 2 discharge windows): libimobiledevice's network mode, same pairing.
            s.add_argument("--network", action="store_true")
    # `launch BUNDLE [options] [-- app args]`: only what follows `--` is the app's. A REMAINDER positional
    # swallowed `--detach` written after the bundle, so the console stayed attached and the call never returned.
    tail = []
    if argv[:1] == ["launch"] and "--" in argv:
        cut = argv.index("--")
        argv, tail = argv[:cut], argv[cut:]
    args = parser.parse_args(argv)
    if tail:
        args.app_args = tail
    try:
        if args.command == "check":
            steps = load_steps(args.steps)
            return emit({"valid": True, "steps": len(steps), "expectedSeconds": expected_seconds(steps),
                         "allowanceNeeded": max(60, math.ceil(expected_seconds(steps) * ALLOWANCE_HEADROOM))})
        if args.command == "pair-steps":
            print(json.dumps(pair_steps(args.config, args.v2_hold), indent=1))
            return 0
        if args.command == "summary":
            return emit(summary(args.dir))
        if args.command == "syslog-rate":
            return emit(syslog_rate(args.file, args.process, args.bucket, args.start, args.end))
        if args.command == "vocabulary":
            result = vocabulary(args.runner)
            return emit(result, 0 if result["same"] else 1)
        if args.command == "parse-log":
            try:
                return emit({"steps": parse_log(Path(args.log).read_text(errors="replace"))})
            except FileNotFoundError:
                raise CannotAnswer(f"no log at {args.log}")
        # Everything below touches the phone: only through `rios device ...` (CEO 2026-10-02: only the
        # Release build goes on his phones and every test on them tests it).
        if os.environ.get("RICHOS_DEVICE_VERB") != "rios":
            return emit({"refused": f"`{args.command}` touches a physical iPhone, which is done only through "
                                    f"`rios device {args.command} ...`: the one command line that puts only the Release "
                                    "build on it (CEO 2026-10-02)"}, 3)
        return {"run": run, "procs": procs, "apps": apps, "lock": lock, "battery": battery,
                "syslog": syslog, "approval": approval, "wifi-restore": wifi_restore, "wifi-on": wifi_restore,
                "wifi-off": wifi_off, "trust": trust,
                "reboot": restart_phone, "launch": launch_app, "net": net, "close": close_app, "crashes": crashes}[args.command](args)
    except CannotAnswer as error:
        return emit({"error": str(error), **getattr(error, "extra", {})}, 2)
    except subprocess.TimeoutExpired as error:
        return emit({"error": f"{error.cmd[0]} did not answer in {error.timeout} s"}, 2)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
