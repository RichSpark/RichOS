#!/usr/bin/env python3
"""phone_net.py — the test iPhone must be able to reach Apple, or nothing runs on it (CEO, 2026-10-02).

Why this exists. iOS verifies the developer certificate of an app signed for development ONLINE, each
time its cached verification has lapsed. A phone that cannot reach Apple at that moment refuses to open
the app: "Unable to Verify App. An internet connection is required to verify trust of the developer",
and every XCUITest session on it fails "The application could not be launched because the Developer
App Certificate is not trusted" (the test iPhone, 2026-09-24 and 2026-10-02 from 12:27Z: the refusal
came and went between passing runs, so the phone's route to Apple was flapping, not the app). The run
that follows such a refusal tests nothing, and it said so only after a build, an install and minutes.

So `rios device` (verify, run, perf, hold) calls this module around the phone's one use:

    preflight   one cheap launch of RichConnect through devicectl, then it is closed again. A refusal
                that names the missing verification restarts the PHONE once (the function below,
                CEO 2026-10-02: no passcode, so it comes back usable without a hand) and opens the app
                again; only when that still fails does it raise `Unreachable` with ONE plain sentence,
                before anything is built, installed or run. A locked phone, a missing tool or any other
                answer is not a verdict on the network: it is said and the run goes on.
    postflight  the same launch after the run, however the run ended. When the phone now refuses,
                the run left its network off (a step list that switches Wi-Fi off in Settings and did not
                get to switch it back): `RICHOS_PHONE_NET_RESTORE` (default `phone-ios.py wifi-restore`)
                reads the Wi-Fi switch in Settings and turns it on, and the launch is tried once more.
                What is still wrong is said in a sentence; it is never hidden behind the run's own exit.

The phone is named by RICHOS_IOS_DEVICE (what xcodebuild and devicectl both accept). Nothing here
installs, uninstalls or erases anything, and it never touches the phone's screen beyond opening and
closing the app and, when the trust check fails, one restart of the phone.
"""
import json
import os
import re
import shlex
import shutil
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
import test_copy  # noqa: E402 — the test copy's identity and the refusal of the CEO's own app

# The net check opens the TEST COPY once, never the CEO's own RichConnect (he handles that himself).
PACKAGE = test_copy.TEST_BUNDLE_IOS
PHONE_IOS = HERE.parent / "app/scripts/qa/phone-ios.py"
# What devicectl (and the test log) say when iOS will not open a developer-signed app for want of its
# online verification.
UNTRUSTED = re.compile(
    r"not trusted|Unable to Verify|verify trust|has not been (?:explicitly )?trusted|"
    r"Developer App Certificate|internet connection is required", re.I)
LOCKED = re.compile(r"\blocked\b|unlock", re.I)

# Settings shows no Verify App button for this team-provisioned app (2026-09-27, and the CEO 2026-10-02);
# sending a person to look for one wastes his time. `rios device trust` names the actual cause.
FIX = ("The test iPhone cannot reach Apple to verify RichConnect's developer certificate, so iOS will not open "
       "the app, and restarting the phone once (`rios device reboot`) did not clear it; `rios device trust` names "
       "why (the phone's DNS, VPN or profiles). Settings has no Verify App button for this app, so do not look "
       "for one: the phone's Wi-Fi has to reach Apple (Tailscale and any VPN off).")

# When the trust check fails, the automation restarts the phone itself (CEO 2026-10-02: the test iPhone has
# no passcode, so it comes back usable on its own). On 2026-10-02 the phone's Wi-Fi DNS server stopped
# answering it (a capture on the phone: its queries to the router went out and nothing came back, while
# the router answered the Mac) and a restart of the phone cleared it. After the phone is back, RichConnect
# is opened every SETTLE_STEP seconds until iOS verifies it or SETTLE_SECONDS pass (Wi-Fi rejoining and
# Apple's check take a moment after boot).
SETTLE_SECONDS = float(os.environ.get("RICHOS_PHONE_REBOOT_SETTLE", "120"))
SETTLE_STEP = float(os.environ.get("RICHOS_PHONE_REBOOT_STEP", "10"))


class Unreachable(Exception):
    """The phone cannot open the app for want of Apple's online verification; the text is the sentence."""


def _xcrun(*args, timeout=60):
    return subprocess.run([os.environ.get("RICHOS_XCRUN") or "xcrun", *args],
                          capture_output=True, text=True, timeout=timeout)


# THE CEO'S OWN APP IS HIS (CEO, 2026-10-03: "I will handle everything regarding my app myself. You handle
# everything about the fucking TEST app."). The checks below open an app to learn whether iOS will open a
# development-signed one; RICHOS_PHONE_CHECK_APP names which, and `none` opens nothing at all: the check then
# reports `skipped` (never `ok`, which would claim a verification that did not happen), and a restart waits
# for the phone's internet without opening anything. Unset, it is the test copy; the CEO's own app is refused by name.
NO_APP = "none"


def check_app():
    app = os.environ.get("RICHOS_PHONE_CHECK_APP") or PACKAGE
    if app != NO_APP:
        test_copy.refuse_ceo_app(app, "the phone's app check")
    return app


def launch_check(device, timeout=60):
    """One launch of the app and its close: {'state': ok|untrusted|locked|unknown|skipped, 'detail': text}.
    `skipped` when RICHOS_PHONE_CHECK_APP=none: nothing is opened."""
    app = check_app()
    if app == NO_APP:
        return {"state": "skipped", "detail": "no app opened (RICHOS_PHONE_CHECK_APP=none)"}
    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / "launch.json"
        try:
            p = _xcrun("devicectl", "device", "process", "launch", "--device", device, "--terminate-existing",
                       "--json-output", str(target), app, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as error:
            return {"state": "unknown", "detail": f"devicectl did not answer ({type(error).__name__})"}
        text = ((p.stderr or "") + (p.stdout or "")).strip()
        try:
            doc = json.loads(target.read_text()) if target.exists() else {}
        except ValueError:
            doc = {}
        if p.returncode == 0:
            pid = ((doc.get("result") or {}).get("process") or {}).get("processIdentifier")
            if pid:
                try:
                    _xcrun("devicectl", "device", "process", "terminate", "--device", device, "--pid", str(pid),
                           timeout=30)
                except (OSError, subprocess.TimeoutExpired):
                    pass
            return {"state": "ok", "detail": ""}
        blob = text + " " + json.dumps(doc.get("error") or {})
        if UNTRUSTED.search(blob):
            return {"state": "untrusted", "detail": text[-300:]}
        if LOCKED.search(blob):
            return {"state": "locked", "detail": text[-300:]}
        return {"state": "unknown", "detail": text[-300:]}


def check_ready(device):
    """Is the phone connected to this Mac and on Wi-Fi? (ok, one sentence). Reads only: no launch.
    devicectl's tunnelState says connected; its transportType says how: 'localNetwork' is Wi-Fi. A cable
    ('wired') proves the connection but devicectl does not report the phone's Wi-Fi, so RICHOS_PHONE_WIFI_CHECK
    (a command, exit 0 = on Wi-Fi) decides there; without it a cable connection is accepted, and said so."""
    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / "details.json"
        try:
            _xcrun("devicectl", "device", "info", "details", "--device", device, "--json-output", str(target),
                   timeout=60)
            props = (json.loads(target.read_text()).get("result") or {}).get("connectionProperties") or {}
        except (OSError, subprocess.TimeoutExpired, ValueError):
            return False, "the iPhone is not connected (devicectl did not answer)"
    if props.get("tunnelState") != "connected":
        return False, "the iPhone is not connected / not on Wi-Fi (devicectl: tunnel " + str(props.get("tunnelState")) + ")"
    if props.get("transportType") == "localNetwork":
        return True, "connected over Wi-Fi"
    named = os.environ.get("RICHOS_PHONE_WIFI_CHECK")
    if named:
        if subprocess.run(shlex.split(named), capture_output=True).returncode == 0:
            return True, "connected by cable; Wi-Fi confirmed by RICHOS_PHONE_WIFI_CHECK"
        return False, "the iPhone is connected by cable but not on Wi-Fi"
    return True, "connected by cable; devicectl does not report Wi-Fi, so it is not confirmed"


# THE PHONE'S WI-FI CAN BE JOINED AND STILL CARRY NOTHING (measured 2026-10-03 02:30-02:45 local, the test
# iPhone, on the phone itself and from this Mac): iOS said the Wi-Fi path was "satisfied" with link quality
# "good" and the phone kept transmitting, yet the Mac's ARP for the phone's own Wi-Fi address stayed
# incomplete and pings went unanswered, the phone's DNS queries got no answer, and its TCP connects to
# internet addresses timed out (`rios device net`, `rios device trust`). devicectl still reached the phone
# over the cable, so nothing on the Mac's side noticed. iOS cannot complete Apple's online check of a
# development-signed app on such a phone, and that is the "Unable to Verify App" dialog. A restart rejoins
# the Wi-Fi and the path carries traffic again. So "on Wi-Fi" is measured here as "this Mac reaches the
# phone's own Wi-Fi address": the phone's address comes from its Bonjour name (answered over the cable too),
# and one ping from the Mac must come back.
COREDEVICE_SUFFIX = ".coredevice.local"   # devicectl's name for the phone
BONJOUR_SUFFIX = ".local"                  # the same phone's Bonjour name


def wifi_address(device):
    """The phone's IPv4 address on the local network (not its cable link's 169.254 address), or None."""
    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / "details.json"
        try:
            _xcrun("devicectl", "device", "info", "details", "--device", device, "--json-output", str(target),
                   timeout=60)
            props = (json.loads(target.read_text()).get("result") or {}).get("connectionProperties") or {}
        except (OSError, subprocess.TimeoutExpired, ValueError):
            return None
    names = [n for n in props.get("localHostnames") or [] if n.endswith(COREDEVICE_SUFFIX)]
    for name in names[:1]:
        local = name[: -len(COREDEVICE_SUFFIX)] + BONJOUR_SUFFIX
        try:
            p = subprocess.run(["dscacheutil", "-q", "host", "-a", "name", local], capture_output=True, text=True,
                               timeout=20)
        except (OSError, subprocess.TimeoutExpired):
            return None
        for addr in re.findall(r"^ip_address: (\S+)$", p.stdout, re.M):
            if not addr.startswith("169.254."):
                return addr
    return None


def wifi_path(device):
    """Does this Mac reach the phone's own Wi-Fi address? (ok, one sentence). Reads only: no launch."""
    addr = wifi_address(device)
    if not addr:
        return False, "the phone's Wi-Fi address could not be found (its Bonjour name gave no local address)"
    try:
        p = subprocess.run(["ping", "-c", "3", "-i", "0.5", "-t", "5", addr], capture_output=True, text=True,
                           timeout=15)
    except (OSError, subprocess.TimeoutExpired):
        return False, f"ping {addr} did not finish"
    got = re.search(r"(\d+) packets received", p.stdout)
    if p.returncode == 0 and got and int(got.group(1)) > 0:
        return True, f"the phone's Wi-Fi address {addr} answers this Mac ({got.group(1)} of 3 pings)"
    # A phone that ignores pings still answers its sync port (lockdownd, 62078) on its Wi-Fi address.
    import socket
    try:
        with socket.create_connection((addr, 62078), timeout=3):
            return True, f"the phone's Wi-Fi address {addr} answers this Mac (its sync port, no ping answer)"
    except OSError:
        pass
    return False, (f"the phone's Wi-Fi address {addr} does not answer this Mac (0 of 3 pings, no answer on its "
                   "sync port): its Wi-Fi carries no traffic")


def cable_internet(device):
    """Does the phone reach the internet over the cable to this Mac (Internet Sharing)? (ok, one sentence).
    Measured, never assumed: `phone-ios.py net` opens Safari once on the phone and reads its connections from
    the phone's own log; this is true only when the phone's elected route is NOT its Wi-Fi and Safari completed
    a connection. RICHOS_PHONE_NET_PROBE (a command printing that JSON on stdout) replaces it in tests."""
    named = os.environ.get("RICHOS_PHONE_NET_PROBE")
    scratch = None
    try:
        if named:
            command = shlex.split(named)
        else:
            root = M.agent_root()
            root.mkdir(parents=True, exist_ok=True)
            scratch = tempfile.mkdtemp(prefix="phone-cable-", dir=str(root))
            command = [sys.executable, str(PHONE_IOS), "net", "--device", device,
                       "--out", str(Path(scratch) / "net.log"), "--seconds", "12"]
        env = {**os.environ, "RICHOS_DEVICE_VERB": "rios"}
        p = subprocess.run(command, capture_output=True, text=True, timeout=180, env=env)
        internet = json.loads(p.stdout).get("internet") or {}
    except (OSError, subprocess.TimeoutExpired, ValueError):
        return False, "the phone's internet over the cable could not be measured"
    finally:
        if scratch:
            shutil.rmtree(scratch, ignore_errors=True)
    primary = internet.get("primary") or {}
    if internet.get("reached") and primary and not primary.get("wifi"):
        return True, (f"the phone reaches the internet over {primary.get('interface')} ({primary.get('address')}), "
                      "the cable to this Mac, not its Wi-Fi")
    return False, "the phone's internet over the cable was measured and it did not complete"


def internet_path(device):
    """The gate's measurement: the phone reaches the internet over Wi-Fi (wifi_path) or over the cable
    (cable_internet). (ok, one sentence). The cable is measured only when the Wi-Fi gave no answer."""
    ok, detail = wifi_path(device)
    if ok:
        return ok, detail
    cable_ok, cable_detail = cable_internet(device)
    if cable_ok:
        return True, cable_detail
    return False, detail + "; " + cable_detail


def restart_and_wait(device, say):
    """Restart the phone (devicectl, a full reboot, waiting until it is connected again), then wait, with
    NO launches, until it is connected and its Wi-Fi carries traffic (wifi_path) or SETTLE_SECONDS pass.
    (ok, detail, seconds)."""
    started = time.monotonic()
    try:
        p = _xcrun("devicectl", "device", "reboot", "--device", device, "--wait-for-device", timeout=600)
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, f"devicectl reboot did not answer ({type(error).__name__})", 0
    if p.returncode != 0:
        return False, "devicectl reboot failed: " + ((p.stderr or "") + (p.stdout or "")).strip()[-300:], 0
    deadline = time.monotonic() + SETTLE_SECONDS
    while True:
        ok, detail = check_ready(device)
        if ok:
            ok, detail = internet_path(device)
        if ok or time.monotonic() + SETTLE_STEP > deadline:
            break
        time.sleep(SETTLE_STEP)
    seconds = time.monotonic() - started
    say(f"the phone restarted; after {seconds:.0f} s: {detail}")
    return ok, detail, seconds


def ensure_path(device, say):
    """Before anything opens an app on the phone: its Wi-Fi must carry traffic, or iOS cannot verify a
    development-signed app and shows "Unable to Verify App". A dead path gets ONE restart of the phone (no app is
    opened, so no dialog and no notification), then a wait until the path answers. (ok, detail, restarted)."""
    ok, detail = internet_path(device)
    if ok:
        return True, detail, False
    say(detail + "; restarting the phone once before anything is opened on it")
    ok, detail, _ = restart_and_wait(device, say)
    return ok, detail, True


def launch_app(device, bundle, app_args, say, out=None, console=True):
    """`rios device launch`: the one procedure for opening an installed app (CEO 2026-10-02/03).
    0. phone not connected / not on Wi-Fi: nothing happens (no launch, no reboot).
    1. open the app once. 2. refused for want of verification: restart the phone once. 3. wait until it is
    back, connected and on Wi-Fi. 4. open the app once more. 5. refused again: say exactly why.
    Never more than two launches and one restart: every failed launch pops a notification on the phone.
    Returns (exit code, {'launches', 'reboots', 'state', 'detail'}); the app's console goes to `out` and the
    call returns when the app exits. console=False (`rios device launch --detach`) returns once iOS has opened
    the app, leaving it running in front."""
    test_copy.refuse_ceo_app(bundle, "`rios device launch`")  # before anything is asked of the phone
    out = out or sys.stdout
    result = {"launches": 0, "reboots": 0, "state": "unknown", "detail": ""}
    ok, detail = check_ready(device)
    if not ok:
        result.update(state="not-ready", detail=detail)
        return 2, result
    say(detail)
    # Its Wi-Fi must carry traffic before the first open, or iOS refuses a development-signed app; a dead
    # path is restarted here, with nothing opened, and that is this procedure's one restart.
    ok, detail, restarted = ensure_path(device, say)
    result["reboots"] += int(restarted)
    if not ok:
        result.update(state="not-ready", detail=("after the restart: " if restarted else "") + detail)
        return 2, result
    say(detail)

    def once():
        result["launches"] += 1
        with tempfile.TemporaryDirectory() as tmp:
            try:
                p = subprocess.Popen([os.environ.get("RICHOS_XCRUN") or "xcrun", "devicectl", "device", "process",
                                      "launch", "--device", device, "--terminate-existing",
                                      *(["--console"] if console else []), "--json-output", str(Path(tmp) / "launch.json"), bundle, *app_args],
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            except OSError as error:
                return {"state": "unknown", "detail": f"devicectl did not start ({error})"}
            lines = []
            for line in p.stdout:
                lines.append(line)
                out.write(line)
                out.flush()
            code = p.wait()
            text = "".join(lines).strip()
            if code == 0:
                return {"state": "ok", "detail": ""}
            doc = Path(tmp) / "launch.json"
            blob = text + " " + (doc.read_text() if doc.exists() else "")
            return {"state": "untrusted" if UNTRUSTED.search(blob) else "failed", "detail": text[-600:]}

    first = once()
    result.update(state=first["state"], detail=first["detail"])
    if first["state"] == "ok":
        return 0, result
    if first["state"] != "untrusted" or result["reboots"]:
        return 1, result
    say("iOS refused to open the app (" + first["detail"].replace("\n", " ")[-200:] + "); restarting the phone once")
    result["reboots"] += 1
    ok, detail, _ = restart_and_wait(device, say)
    if not ok:
        result.update(state="not-ready", detail="after the restart: " + detail)
        return 2, result
    second = once()
    result.update(state=second["state"], detail=second["detail"])
    return (0 if second["state"] == "ok" else 1), result


def reboot(device, say):
    """Restart the phone (devicectl, a full reboot) and wait until it is back and its Wi-Fi carries traffic
    (wifi_path). Opens NO app, ever: after a restart RichConnect shows nothing (CEO 2026-10-04). Returns
    state `ok` when the phone is back and reaches the internet, with 'rebooted' and the seconds it took.
    Nothing is installed, removed or erased."""
    started = time.monotonic()
    ok, detail, back = restart_and_wait(device, say)
    if not back:
        return {"state": "unknown", "detail": detail, "rebooted": False}
    if not ok:
        return {"state": "not-ready", "detail": "after the restart: " + detail, "rebooted": True,
                "launchTries": 0, "totalSeconds": round(time.monotonic() - started)}
    return {"state": "ok", "detail": detail, "rebooted": True, "backAfterSeconds": round(back),
            "launchTries": 0, "totalSeconds": round(time.monotonic() - started)}


def preflight(say, device=None):
    """Raises Unreachable (the sentence) when the phone will not open the app for want of verification,
    after one restart of the phone (reboot) has not cleared it."""
    device = device or os.environ.get("RICHOS_IOS_DEVICE")
    if not device:
        say("phone network check skipped: RICHOS_IOS_DEVICE names no phone")
        return
    ok, detail, restarted = ensure_path(device, say)
    if not ok:
        raise Unreachable(FIX + f" (the phone's Wi-Fi: {detail})")
    result = launch_check(device)
    if result["state"] == "untrusted" and restarted:
        raise Unreachable(FIX + f" (after the restart: {result['state']}: {result['detail']})")
    if result["state"] == "untrusted":
        say("the phone cannot verify RichConnect with Apple; restarting it once (rios device reboot): "
            + result["detail"].replace("\n", " ")[-200:])
        result = reboot(device, say)
        if result["state"] != "ok":
            raise Unreachable(FIX + f" (after the restart: {result['state']}: {result['detail']})")
        back_after = result["totalSeconds"]
        result = launch_check(device)  # this procedure's one open after the restart; `reboot` opens nothing
        if result["state"] != "ok" and result["state"] != "skipped":
            raise Unreachable(FIX + f" (after the restart: {result['state']}: {result['detail']})")
        result["totalSeconds"] = back_after
        say(f"after the restart the phone opens RichConnect ({result['totalSeconds']} s): the run goes on")
        return
    if result["state"] == "skipped":
        say(f"phone network check: the phone reaches the internet; {result['detail']}, so iOS's verification of an "
            "app was not tried; the run goes on")
    elif result["state"] != "ok":
        say(f"phone network check could not decide ({result['state']}: {result['detail']}); the run goes on")


def _restore_command():
    named = os.environ.get("RICHOS_PHONE_NET_RESTORE")
    if named:
        return shlex.split(named)
    return None


def restore(say):
    """Ask the phone to put its Wi-Fi back on. True when the command ran and exited 0."""
    named = _restore_command()
    scratch = None
    try:
        if named:
            command = named
        else:
            root = M.agent_root()
            root.mkdir(parents=True, exist_ok=True)
            scratch = tempfile.mkdtemp(prefix="phone-net-", dir=str(root))
            command = [sys.executable, str(PHONE_IOS), "wifi-restore", "--out", scratch]
        p = subprocess.run(command, capture_output=True, text=True, timeout=600)
        if p.returncode != 0:
            say(f"putting the phone's Wi-Fi back failed (exit {p.returncode}): {(p.stdout + p.stderr).strip()[-300:]}")
        return p.returncode == 0
    except (OSError, subprocess.TimeoutExpired) as error:
        say(f"putting the phone's Wi-Fi back failed: {error}")
        return False
    finally:
        if scratch:
            shutil.rmtree(scratch, ignore_errors=True)


def postflight(say, device=None):
    """After the run, however it ended. Never raises. Returns None when the phone is fine (or cannot be
    judged) and the sentence when the phone is still unable to open the app."""
    device = device or os.environ.get("RICHOS_IOS_DEVICE")
    if not device:
        return None
    try:
        result = launch_check(device)
        if result["state"] != "untrusted":
            return None
        say("the phone now refuses to open the app for want of Apple's verification: the run left its "
            "network off; putting the Wi-Fi back")
        restore(say)
        again = launch_check(device)
        if again["state"] == "untrusted":
            return "After the run " + FIX
        say("the phone's network is back: the app opens again")
        return None
    except Exception as error:  # noqa: BLE001: the after-check never changes the run's own outcome by crashing
        say(f"phone network check after the run failed: {error}")
        return None


def main(argv):
    """`phone_net.py ensure --device ID` (what `rios device install` runs after every install, so the first open
    can be verified): the phone's Wi-Fi carries traffic, after one restart of the phone if it did not.
    Exit 0 when it does, 2 when it still does not. Only `rios device` starts it (physical.py)."""
    if os.environ.get("RICHOS_DEVICE_VERB") != "rios":
        print(json.dumps({"refused": "only `rios device ...` touches the iPhone"}))
        return 3
    if len(argv) != 3 or argv[0] != "ensure" or argv[1] != "--device":
        print(json.dumps({"error": "phone_net.py ensure --device ID"}))
        return 2
    say = lambda line: print(line, file=sys.stderr, flush=True)
    ok, detail, restarted = ensure_path(argv[2], say)
    print(json.dumps({"wifiCarriesTraffic": ok, "restarted": restarted, "detail": detail}))
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
