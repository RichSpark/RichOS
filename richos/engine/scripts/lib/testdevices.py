#!/usr/bin/env python3
"""testdevices.py — a simulator or emulator a test made is garbage when the test is over.

CEO, ceo-decisions §54, verbatim: "There now needs to be a ROCK-SOLID,
UNBREAKABLE, UNDEFEATABLE mechanisms that always guarantees that garbage like
this will be always cleaned up afterwards. Or if the clean-up fails or
impossible for some reason, then Rich must get a MASSIVE ALERT about it". And
addendum 4: "Test app windows must always close/quit when testing is finished.
Same hygiene as with any other garbage."

WHY THIS EXISTS. On 2026-09-22 the CEO stopped ten agents from his own screen.
Three iOS simulators their test runs had booted stayed booted after he exited:
one `rios-ui-<pid>-...` from richos/app/scripts/native-ios-ui.test.sh and two
`RichOS native-ios <key>-app` from native-ios-app.test.sh. Each script deletes
its simulator in an EXIT trap, and a killed run never reaches its trap.
appinstances.py, the engine's one definition of a test instance, looks only for
the desktop app's process, so nothing collected them; Rich deleted them by
hand the next session.

THE RULE: OWNERSHIP, NEVER A NAME. A name says which family of tool made a
device; it never says whether anyone is still using it. A device is collected
only when its owner is PROVEN gone, by one of:

  registered owner   the exact simulator UDID or emulator process generation
                     records every live user; all registered processes or agent
                     runs have ended. A cache path alone never proves ownership
  creator pid        `rios-ui-<pid>-...` names its creator; the pid no longer
                     exists, or it started AFTER the device was created (the
                     number was reused)
  checkout           the per-checkout families (`RichOS native-ios <key>`,
                     `RichOS mobile loop <key>`, `randroid-<key>`) are keyed by
                     a hash of the checkout's path; the device is collected when
                     that key belongs to a checkout the engine RECORDED and that
                     checkout no longer exists (or is being deleted by the land
                     running this). An existing checkout without a live owner is undecided

Anything in those families that none of the three can decide is UNDECIDED: it
is never deleted, and while it RUNS it is written to the failure record, which
the disk watchdog raises as the MASSIVE ALERT with the exact command Rich runs
(a shut-down one is reported by the collector and not alerted; see
record_failures). A device
that would not close when collected is recorded the same way. A row resolves
the moment its device no longer exists, however it went. Devices outside the
families (an engineer's own "iPhone 16") are never looked at.

Every simulator command addresses one UDID (never `booted`); an emulator gets a
signal only through the pid its own launcher recorded, and only after its
arguments are read back and still name that AVD.
"""

import argparse
import calendar
import contextlib
import contextvars
import fcntl
import importlib.util
import shlex
import tempfile
import hashlib
import json
import plistlib
import os
import re
import signal
import subprocess
import sys
import time

FAMILY_RIOS_UI = re.compile(r"^rios-ui-(\d+)-")
FAMILY_NATIVE = re.compile(r"^RichOS native-ios ([0-9a-f]{10})(-[a-z]+)?$")
FAMILY_LOOP = re.compile(r"^RichOS mobile loop ([0-9a-f]{10})$")
FAMILY_PLATFORM = re.compile(r"^RichOS native-ios platform-tests ")

COLLECT = "COLLECT"
LEAVE = "LEAVE"
UNDECIDED = "UNDECIDED"


_LOCKED = contextvars.ContextVar("device_registry_locked", default=False)
_DEADLINE = contextvars.ContextVar("device_collection_deadline", default=None)
# The alert writer of a collection that has marked itself running, so each bounded
# operation can move the mark's `due` (testdevice_alerts.note_operation).
_MARK = contextvars.ContextVar("device_collection_mark", default=None)


class BudgetExpired(TimeoutError):
    pass


class RegistryBusy(TimeoutError):
    """Another process held the registry lock for the whole wait."""


# How long a registry call waits for the lock before giving up. The collector keeps five
# seconds and skips a busy cycle (expire_leases); it runs every two seconds and must not block.
REGISTRY_LOCK_SECONDS = 5
# A CLI verb that a test run depends on (acquire, boot, release, run-active...) waits longer.
# The lock is held across simctl calls, which took more than five seconds on the pinned host
# of 2026-09-25, and a UI suite's second device then failed on its acquire.
CLI_REGISTRY_LOCK_SECONDS = 90
# The lease collector SKIPS a cycle when the registry is busy and retries on the next one
# (every two seconds). A lock held longer than this is no longer a busy moment: the longest
# legitimate hold is a simctl call under the lock (acquire, release, collect), bounded at
# 120 s by simctl(), so past 180 s the collector reports a failure and the watchdog alerts.
COLLECTOR_BUSY_ALERT_SECONDS = 180


import machine_settings as M

def _timeout(maximum):
    deadline = _DEADLINE.get()
    remaining = maximum if deadline is None else min(maximum, deadline - time.time())
    if remaining <= 0:
        raise BudgetExpired("test-device collection deadline reached")
    writer = _MARK.get()
    if writer is not None:
        try:
            writer.note_operation(remaining)
        except (OSError, ValueError):
            pass                 # a missed move only makes the alert come sooner
    return remaining


@contextlib.contextmanager
def registry_lock(wait_seconds=None):
    """Registration and collection cannot replace ownership under a deletion."""
    if _LOCKED.get():
        yield
        return
    os.makedirs(registry_dir(), exist_ok=True)
    with open(os.path.join(registry_dir(), ".lock"), "a") as handle:
        wait_seconds = REGISTRY_LOCK_SECONDS if wait_seconds is None else wait_seconds
        lock_deadline = time.monotonic() + wait_seconds
        while True:
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= lock_deadline:
                    raise RegistryBusy("test-device registry lock stayed busy for %g seconds"
                                       % wait_seconds)
                time.sleep(min(0.05, _timeout(5)))
        token = _LOCKED.set(True)
        try:
            yield
        finally:
            _LOCKED.reset(token)
            fcntl.flock(handle, fcntl.LOCK_UN)


def _workspace_module():
    path = os.path.join(os.path.dirname(__file__), "../../mega-lander/workspaces.py")
    spec = importlib.util.spec_from_file_location("device_workspaces", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    # Reuse the authoritative lifecycle rules with this collection's budget.
    def bounded_process_start(pid):
        value = process_start(pid)
        return ("unknown", "process identity unreadable") if value is None else (("ok", value) if value else ("gone", ""))
    mod.process_start = bounded_process_start
    return mod


def choose_owner(owner_pid=None, checkout="", script=""):
    """An explicit test process, an exact registered agent, or a live session.

    A short CLI process is not the owner of a device used across CLI calls.
    Unknown ownership is recorded explicitly and alerts instead of guessing.
    """
    raw = owner_pid or os.environ.get("RICHOS_TEST_DEVICE_OWNER_PID")
    if raw:
        start = process_start(raw)
        if not start:
            raise ValueError("the owner pid %s is not a running process" % raw)
        return {"pid": int(raw), "start": start, "script": script}
    if checkout:
        ws = _workspace_module()
        matches = []
        for sub in ("agents", "done"):
            folder = os.path.join(workspaces_dir(), sub)
            for name in os.listdir(folder) if os.path.isdir(folder) else []:
                rec = _read_json(os.path.join(folder, name))
                if not isinstance(rec, dict):
                    continue
                if any(os.path.realpath(w.get("path") or "/") == os.path.realpath(checkout)
                       for w in rec.get("workspaces", []) if isinstance(w, dict)):
                    if not ws.finished_state(rec)[0]:
                        matches.append(rec)
        unique = {r["key"]: r for r in matches if r.get("key")}
        if len(unique) == 1:
            rec = next(iter(unique.values()))
            return {"workspace_key": rec["key"], "session_id": rec.get("session_id"),
                    "agent_id": rec.get("agent_id"), "script": script}
        if len(unique) > 1:
            return {"unknown": "multiple live workspace owners", "script": script}
    pid = os.getppid()
    for _ in range(20):
        r = subprocess.run(["ps", "-o", "ppid=,comm=", "-p", str(pid)],
                           capture_output=True, text=True, timeout=_timeout(5), env=_env())
        parts = r.stdout.strip().split(None, 1)
        if len(parts) != 2:
            break
        name = os.path.basename(parts[1])
        # A Claude/Codex process or interactive terminal shell outlives a CLI.
        if name in ("claude", "codex") or (name in ("zsh", "-zsh", "bash", "-bash", "fish")
                and (process_args(pid) or "").strip() in (name, name + " -l", name + " -il", name + " -i")):
            start = process_start(pid)
            if start:
                return {"pid": pid, "start": start, "script": script}
        pid = int(parts[0])
        if pid <= 1:
            break
    return {"unknown": "no durable owner could be proven", "script": script}


def _env():
    return dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")


def _state_base():
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state")


def registry_dir():
    return (os.environ.get("RICHOS_TEST_DEVICES_DIR") or "").strip() \
        or os.path.join(_state_base(), "test-devices")


def failures_path():
    """Declared in orchestration.config as TEST_DEVICE_FAILURES_STATE and
    resolved here the same way, so the writer and the watchdog cannot drift."""
    return (os.environ.get("TEST_DEVICE_FAILURES_STATE") or "").strip() \
        or os.path.join(_state_base(), "test-device-failures.json")


def workspaces_dir():
    return (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip() \
        or os.path.join(_state_base(), "workspaces")


def machine_devices_allowed():
    """THE MACHINE'S DEVICES ARE TOUCHED ONLY WITH THE ACCOUNT'S OWN STATE.
    Simulators and emulators belong to the machine, while the records that
    prove who owns one live in a state directory, and every engine suite runs
    with HOME or CLAUDE_CONFIG_DIR redirected into a sandbox. Deciding a REAL
    device against a SANDBOX's records would be deciding it against the wrong
    evidence, so with either redirected the machine's devices are not read at
    all. A suite that means to exercise this module names its own fakes with
    RICHOS_SIMCTL and RICHOS_ANDROID_CACHES_ROOT."""
    try:
        import pwd
        real = os.path.realpath(pwd.getpwuid(os.getuid()).pw_dir)
    except (ImportError, KeyError):
        return False
    if os.path.realpath(os.path.expanduser("~")) != real:
        return False
    for var, default in (("CLAUDE_CONFIG_DIR", os.path.join(real, ".claude")),
                         ("RICHOS_TEST_DEVICES_DIR", os.path.join(real, ".claude/state/test-devices")),
                         ("TEST_DEVICE_FAILURES_STATE", os.path.join(real, ".claude/state/test-device-failures.json")),
                         ("RICHOS_WORKSPACES_DIR", os.path.join(real, ".claude", "state", "workspaces"))):
        v = (os.environ.get(var) or "").strip()
        if v and os.path.realpath(v) != os.path.realpath(default):
            return False
    return True


def android_caches_root():
    v = (os.environ.get("RICHOS_ANDROID_CACHES_ROOT") or "").strip()
    if v:
        return v
    return str(M.cache("richos-native-android", "RICHOS_ANDROID_CACHES_ROOT")) if machine_devices_allowed() else ""


def sim_devices_dir():
    return (os.environ.get("RICHOS_SIM_DEVICES_DIR") or "").strip() or os.path.join(
        os.path.expanduser("~"), "Library", "Developer", "CoreSimulator", "Devices")


def _read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def _write_json(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=".record-", dir=os.path.dirname(path))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=1, sort_keys=True)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


# ---------------------------------------------------------------------------
# processes, read from the operating system
# ---------------------------------------------------------------------------

def process_start(pid):
    """The process's start time as `ps -o lstart=` prints it, "" when no such
    process exists, None when it cannot be told."""
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return None
    if pid <= 0:
        return None
    try:
        r = subprocess.run(["ps", "-o", "stat=,lstart=", "-p", str(pid)], capture_output=True,
                           text=True, timeout=_timeout(10), env=_env())
    except BudgetExpired:
        raise
    except (OSError, subprocess.TimeoutExpired):
        return None
    stat, _, text = r.stdout.strip().partition(" ")
    text = " ".join(text.split())
    if r.returncode == 0 and stat.startswith("Z"):
        return ""                        # a zombie has ended; only its parent's wait is missing
    if r.returncode == 0 and text:
        return text
    if r.returncode in (0, 1) and not text:
        return ""
    return None


def _epoch(lstart):
    """lstart is read with TZ=UTC0 (see _env), so it is UTC."""
    try:
        return calendar.timegm(time.strptime(lstart, "%a %b %d %H:%M:%S %Y"))
    except (TypeError, ValueError):
        return None


def process_args(pid):
    try:
        r = subprocess.run(["ps", "-ww", "-o", "args=", "-p", str(int(pid))], capture_output=True,
                           text=True, timeout=_timeout(10), env=_env())
    except BudgetExpired:
        raise
    except (OSError, subprocess.TimeoutExpired, TypeError, ValueError):
        return None
    return r.stdout.strip() if r.returncode == 0 else ("" if r.returncode == 1 else None)


def owner_state(owner):
    """('alive'|'gone'|'unknown', why) for a registered {pid, start}."""
    owner = owner or {}
    if owner.get("unknown"):
        return "unknown", owner["unknown"]
    if owner.get("workspace_key"):
        ws = _workspace_module()
        rec = ws.load_agent(owner["workspace_key"])
        if not rec or rec.get("session_id") != owner.get("session_id") or rec.get("agent_id") != owner.get("agent_id"):
            return "unknown", "the registered agent identity changed or is missing"
        ended, _paused, why = ws.finished_state(rec)
        if ended:
            return "gone", why
        state, detail = ws.session_state(rec.get("session_id"), rec.get("session_identity"))
        return ("alive" if state == "running" else "unknown"), detail
    pid = owner.get("pid")
    st = process_start(pid)
    if st is None:
        return "unknown", "the owner's process (%s) could not be read" % pid
    if st == "":
        return "gone", "its owner, pid %s, no longer exists" % pid
    if owner.get("start") and st != owner["start"]:
        return "gone", "pid %s is now another process (started %s, owner started %s)" % (
            pid, st, owner["start"])
    return "alive", "its owner, pid %s, still runs" % pid


# ---------------------------------------------------------------------------
# the registry: what a creating script says it made
# ---------------------------------------------------------------------------

KINDS = ("ios-simulator", "android-emulator", "ios-cache", "android-cache")

# A device lease has two independent limits (host-cpu-enforcement.md). The
# inactivity limit catches a device its CLI forgot; the lifetime is absolute
# unless an owned run is actively renewing the lease (LEASE_ACTIVE_CEILING_SECONDS).
LEASE_MAX_SECONDS = 900
LEASE_IDLE_SECONDS = 300
# How often run-active renews an owned run's activity: ten renewals fit inside
# one inactivity limit, so a renewer starved by a busy host still keeps up.
LEASE_RENEW_SECONDS = 30
# A DECLARED purpose may name a longer lifetime; a caller never names a number.
# The native iOS UI suite runs its whole selection serially on one device and
# took 787-884 s per device under load on 2026-09-24, against a 900 s lifetime
# that also covers the boot. 1800 s is twice that measurement. Owner death and
# lease-holder death still collect at once whatever the lifetime is.
LEASE_PURPOSES = {"ui-suite": 1800}
# The lifetime above applies to a lease nobody vouches for. A lease its owned run
# keeps renewing (run-active) outlives it, up to this absolute ceiling, so a run
# that hangs yet stays alive still releases the device. Four hours is many times
# the longest measured suite (884 s) and short enough to be found the same day.
LEASE_ACTIVE_CEILING_SECONDS = 4 * 3600


def _record_path(kind, ident):
    h = hashlib.sha1(("%s\0%s" % (kind, ident)).encode("utf-8")).hexdigest()[:16]
    return os.path.join(registry_dir(), "%s-%s.json" % (kind, h))


def _norm(kind, ident):
    return os.path.realpath(ident) if kind.endswith("-cache") or kind == "android-emulator" else ident.strip()


def register(kind, ident, owner_pid=None, script="", checkout="", device_set=""):
    if kind not in KINDS:
        raise ValueError("kind must be one of %s" % ", ".join(KINDS))
    ident = _norm(kind, ident)
    if not ident:
        raise ValueError("nothing to register")
    owner = choose_owner(owner_pid, checkout, script)
    if os.environ.get("RICHOS_TEST_DEVICE_RUN_ID"):
        owner["verification_run"] = os.environ["RICHOS_TEST_DEVICE_RUN_ID"]
    with registry_lock():
        generation = None
        if kind == "ios-simulator":
            generation = {"udid": ident, "device_set": os.path.realpath(device_set) if device_set else ""}
        elif kind == "android-emulator":
            dev = _read_json(os.path.join(ident, "emulator.json"))
            if not isinstance(dev, dict) or not dev.get("pid") or not dev.get("avd"):
                raise ValueError("no emulator identity in %s" % ident)
            start = process_start(dev["pid"])
            if dev.get("start") and dev["start"] != start:
                raise ValueError("the recorded emulator process generation changed")
            if not start or not _names_avd(dev["pid"], dev["avd"]):
                raise ValueError("the emulator identity is no longer alive")
            generation = {"pid": int(dev["pid"]), "start": start, "avd": dev["avd"]}
        path = _record_path(kind, ident)
        previous = _read_json(path) or {}
        if previous.get("prepared") and not same_owner(previous.get("owner", {}), owner):
            raise ValueError("prepared simulator already belongs to another run")
        owners = [owner]
        if generation and previous.get("generation") == generation:
            # A second live caller must not revoke the first caller's lease.
            for old in previous.get("owners", [previous.get("owner", {})]):
                if old != owner and owner_state(old)[0] != "gone":
                    owners.append(old)
        rec = {"kind": kind, "id": ident, "registered_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "owner": owner, "owners": owners, "generation": generation}
        if previous.get("prepared"):
            rec["prepared"] = previous["prepared"]
        if kind in ("ios-simulator", "android-emulator"):
            old_lease = previous.get("lease", {}) if previous.get("generation") == generation else {}
            # Re-registering never lengthens a lease: a declared purpose's
            # lifetime is set once, by acquire_ios, and this resets to the default.
            rec["lease"] = {**old_lease, "created": old_lease.get("created", time.time()),
                            "last_use": time.time(), "max_seconds": LEASE_MAX_SECONDS,
                            "idle_seconds": LEASE_IDLE_SECONDS}
            rec["lease"].pop("purpose", None)
        _write_json(path, rec)
    return rec


def touch_lease(kind, ident):
    """Renew activity without resetting the maximum lifetime or adding owners."""
    with registry_lock():
        path = _record_path(kind, _norm(kind, ident))
        rec = _read_json(path)
        if not rec:
            raise ValueError("device has no registered lease; prepare it again")
        if kind == "android-emulator":
            gen = rec.get("generation") or {}
            current = process_start(gen.get("pid", 0))
            if (current is not None and current != gen.get("start")) or not _names_avd(gen["pid"], gen["avd"]):
                raise ValueError("device process identity changed; prepare it again")
        if lease_expired(rec):
            raise ValueError("device lease expired; stop and prepare the device again")
        if rec.get("lease"):
            rec["lease"]["last_use"] = time.time()
            _write_json(path, rec)


def renew_activity(kind, ident, owner, created, child):
    """One renewal on behalf of an OWNED, LIVE run: (renewed, why not).

    Renews only the inactivity clock of the exact lease generation `created`,
    only while `child` (a process this renewer started) still runs, and only
    while the lease's registered owner is `owner` and is alive. The lifetime is
    never touched, and an expired lease is never renewed, so a runaway run ends
    at its lifetime like any other.
    """
    if child.poll() is not None:
        return False, "the owned run ended"
    # Read the owner's process before taking the registry lock: `ps` stalled
    # for seconds under the September 24 load, and the collector needs the lock.
    if owner_state(owner)[0] != "alive":
        return False, "the run's owner is not proven alive"
    with registry_lock():
        path = _record_path(kind, _norm(kind, ident))
        rec = _read_json(path)
        lease = (rec or {}).get("lease")
        if not lease:
            return False, "the device has no lease any more"
        if lease.get("created") != created:
            return False, "the lease was replaced by another generation"
        if not any(same_owner(o, owner) for o in rec.get("owners", [rec.get("owner", {})])):
            return False, "the lease belongs to another run"
        if lease_expired(rec):
            return False, "the lease reached its lifetime or inactivity limit"
        lease["last_use"] = time.time()
        lease["activity"] = {"pid": child.pid, "renewer": os.getpid(), "renewed": lease["last_use"]}
        _write_json(path, rec)
    return True, ""


LEASE_LOST_EXIT = 75


def lease_still_ours(kind, ident, owner, created):
    """(ours, why not): a lock-free read of the record. The record is replaced atomically
    (os.replace), so a read sees one whole version; the lock is not needed to ask."""
    rec = _read_json(_record_path(kind, _norm(kind, ident)))
    lease = (rec or {}).get("lease")
    if not lease:
        return False, "the device's lease was ended (by the collector, a release or a stop)"
    if lease.get("created") != created:
        return False, "the device was leased again, to a new lease generation"
    if not any(same_owner(o, owner) for o in rec.get("owners", [rec.get("owner", {})])):
        return False, "the device now belongs to another run"
    if lease_expired(rec):
        return False, "the lease reached its lifetime or inactivity limit"
    return True, ""


def _end_group(pgid, child, grace=10):
    """TERM, a bounded wait, then KILL, to the process group this renewer created for
    its own child (pgid = the child's pid, captured at spawn). Never a name match."""
    for sig, wait in ((signal.SIGTERM, grace), (signal.SIGKILL, 5)):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            break
        try:
            child.wait(timeout=wait)
            break
        except subprocess.TimeoutExpired:
            continue
    try:
        os.killpg(pgid, signal.SIGKILL)       # descendants that outlived the leader
    except ProcessLookupError:
        pass


def run_active(kind, ident, command, owner_pid=None, checkout="", interval=None, check=2.0,
               lost_file=""):
    """Run COMMAND as the owned activity of an existing lease; return its exit status.

    A lease's inactivity limit is written for a CLI that touches its device
    between calls. A test run is one long call: `xcodebuild test-without-building`
    ran 787-884 s per device on 2026-09-24 and nothing renewed the lease, so the
    collector shut the simulator down five minutes in (escalation
    esc-20260924T220236Z-52fae3ec). This renews activity from the process that
    started the run, for exactly as long as that process lives, and never
    extends the lifetime.

    A RUN WHOSE LEASE HAS ENDED NEVER KEEPS USING THE DEVICE (escalation
    esc-20260925T014934Z-0a4bf206). On 2026-09-25 a run's lease ended mid-suite,
    another run leased the same prepared simulator and installed its own build,
    and the first run's xcodebuild restarted on the device and executed the OTHER
    checkout's test bundle. So every `check` seconds this reads the lease: the
    moment it is gone, replaced, another run's or expired, the whole run (its own
    process group, created here) is ended, the reason is printed loudly and
    written to `lost_file`, and the exit status is LEASE_LOST_EXIT (75), which
    callers report as NOT RUN, never as a result.
    """
    owner = choose_owner(owner_pid, checkout, "run-active")
    if owner.get("unknown"):
        raise ValueError("an owned run needs a durable owner: " + owner["unknown"])
    with registry_lock():
        rec = _read_json(_record_path(kind, _norm(kind, ident)))
        if not rec or not rec.get("lease"):
            raise ValueError("device has no registered lease; prepare it again")
        if not any(same_owner(o, owner) for o in rec.get("owners", [rec.get("owner", {})])):
            raise ValueError("cannot run under another run's device lease")
        if lease_expired(rec):
            raise ValueError("device lease expired; stop and prepare the device again")
        created = rec["lease"]["created"]
    interval = LEASE_RENEW_SECONDS if interval is None else interval
    # The run gets a process group of its own, so ending it ends xcodebuild and everything
    # it started. Signals this renewer receives are passed on to that group.
    try:
        child = subprocess.Popen(command, start_new_session=True)
    except OSError as exc:
        sys.stderr.write("testdevices run-active: cannot start %s: %s\n" % (command[0], exc))
        return 127
    pgid = child.pid

    def forward(signum, _frame):
        try:
            os.killpg(pgid, signum)
        except ProcessLookupError:
            pass
    previous = {s: signal.signal(s, forward) for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
    try:
        renewing, next_renewal = True, time.monotonic() + interval
        while True:
            try:
                code = child.wait(timeout=max(0.05, min(check, next_renewal - time.monotonic())))
                break
            except subprocess.TimeoutExpired:
                pass
            ours, why = lease_still_ours(kind, ident, owner, created)
            if not ours and child.poll() is None:
                message = ("testdevices run-active: LEASE LOST on %s: %s. Stopping this run; its "
                           "results are NOT RUN, never a verdict." % (ident, why))
                sys.stderr.write(message + "\n")
                sys.stderr.flush()
                _end_group(pgid, child)
                if lost_file:
                    _write_json(lost_file, {"device": ident, "why": why, "at": time.time()})
                return LEASE_LOST_EXIT
            if renewing and time.monotonic() >= next_renewal:
                next_renewal = time.monotonic() + interval
                try:
                    renewed, why = renew_activity(kind, ident, owner, created, child)
                except (OSError, TimeoutError, ValueError) as exc:
                    # A registry lock timeout or a full disk is a failed ATTEMPT, not a
                    # lost lease: the next interval tries again. A positive loss (a False
                    # answer below) and the lease check above are what end renewal or the
                    # run, so a healthy test is not abandoned by one bad moment.
                    sys.stderr.write("testdevices run-active: renewal of %s failed (%s); "
                                     "trying again in %ss\n" % (ident, exc, interval))
                    continue
                if not renewed:
                    renewing = False
                    if child.poll() is None:
                        sys.stderr.write("testdevices run-active: stopped renewing %s: %s\n" % (ident, why))
    finally:
        for s, h in previous.items():
            signal.signal(s, h)
    return code if code >= 0 else 128 - code


def lease_expired(rec, now=None):
    lease = rec.get("lease")
    if not lease:
        return False
    now = time.time() if now is None else now
    if now - lease["last_use"] >= lease["idle_seconds"]:
        return True
    age = now - lease["created"]
    if age < lease["max_seconds"]:
        return False
    # The lifetime exists to end a device nobody is using or nobody can vouch for.
    # A run whose own renewer (renew_activity: live owner, live child) renewed it
    # within the last inactivity window is demonstrably working, so the lifetime
    # does not cut it off; LEASE_ACTIVE_CEILING_SECONDS still ends a run that
    # renews forever without finishing.
    activity = lease.get("activity") or {}
    renewed = activity.get("renewed")
    if isinstance(renewed, (int, float)) and now - renewed < lease["idle_seconds"]:
        return age >= max(LEASE_ACTIVE_CEILING_SECONDS, lease["max_seconds"])
    return True


def _registered_verdict(rec):
    holder = (rec.get("lease") or {}).get("holder")
    if holder and process_start(holder["pid"]) == "":
        return COLLECT, "device lease supervisor ended"
    if holder and process_start(holder["pid"]) not in (None, holder["start"]):
        return COLLECT, "device lease supervisor PID was reused"
    if lease_expired(rec):
        return COLLECT, "device lease reached its lifetime or inactivity limit"
    states = [owner_state(o) for o in rec.get("owners", [rec.get("owner", {})])]
    if any(st == "alive" for st, _ in states):
        return LEAVE, "a registered owner still runs"
    if not states or any(st == "unknown" for st, _ in states):
        return UNDECIDED, "registered ownership cannot be proven"
    return COLLECT, "all registered owners ended (%s): %s" % (
        rec.get("owner", {}).get("script") or "test", "; ".join(why for _, why in states))


def records():
    out = []
    try:
        names = sorted(os.listdir(registry_dir()))
    except FileNotFoundError:
        return out
    for n in names:
        if n.endswith(".json"):
            r = _read_json(os.path.join(registry_dir(), n))
            if not isinstance(r, dict):
                raise ValueError("unreadable device registration: %s" % n)
            if isinstance(r, dict) and r.get("kind") in KINDS and r.get("id"):
                r["_path"] = os.path.join(registry_dir(), n)
                out.append(r)
    return out


# ---------------------------------------------------------------------------
# checkouts the engine has recorded, and the keys their tools derive
# ---------------------------------------------------------------------------

def _sha256_10(s):
    return hashlib.sha256(s.encode("utf-8")).hexdigest()[:10]


def _sha1_10(s):
    return hashlib.sha1(s.encode("utf-8")).hexdigest()[:10]


def checkout_keys(path):
    """The keys each per-checkout tool derives from a checkout at `path`:
       suite     native-ios-*.test.sh: shasum -a 256 of the checkout root
       rios      native-ios/bin/rios: of <root>/richos/mobile/native-ios
       loop      mobile/cli/cache.mjs: of <root>/richos/mobile
       randroid  native-android/bin/randroid: SHA-1 of <root>/richos/mobile/native-android"""
    p = path.rstrip("/")
    return {
        "suite": _sha256_10(p),
        "rios": _sha256_10(p + "/richos/mobile/native-ios"),
        "loop": _sha256_10(p + "/richos/mobile"),
        "randroid": _sha1_10(p + "/richos/mobile/native-android"),
    }


def _git_worktrees(repo):
    try:
        r = subprocess.run(["git", "-C", repo, "worktree", "list", "--porcelain"], capture_output=True,
                           text=True, timeout=_timeout(20), env=_env())
    except BudgetExpired:
        raise
    except (OSError, subprocess.TimeoutExpired):
        return []
    if r.returncode != 0:
        return []
    return [line[len("worktree "):] for line in r.stdout.splitlines() if line.startswith("worktree ")]


def recorded_checkouts():
    """Every checkout path the engine has a record of: the repositories and their
    current worktrees, and every workspace any agent record ever named."""
    paths = set()
    wd = workspaces_dir()
    repos = _read_json(os.path.join(wd, "repos.json"))
    if isinstance(repos, dict):
        repos = list(repos.keys()) + [v for v in repos.values() if isinstance(v, str)]
    for repo in repos if isinstance(repos, list) else []:
        if isinstance(repo, str) and repo.startswith("/"):
            paths.add(repo)
            for w in _git_worktrees(repo) if os.path.isdir(repo) else []:
                paths.add(w)
    for sub in ("agents", "done"):
        try:
            names = os.listdir(os.path.join(wd, sub))
        except OSError:
            continue
        for n in names:
            rec = _read_json(os.path.join(wd, sub, n))
            for w in (rec or {}).get("workspaces") or []:
                if isinstance(w, dict):
                    for k in ("path", "repo"):
                        v = w.get(k)
                        if isinstance(v, str) and v.startswith("/"):
                            paths.add(v)
    return paths


class Checkouts(object):
    """(family, key) -> (path, exists) for every recorded checkout. Built on the
    first question, so a pass that finds no device in a per-checkout family
    reads no record and runs no git."""

    def __init__(self, departing=()):
        self.departing = set(os.path.realpath(p) for p in departing or () if p)
        self._by_key = None

    @property
    def by_key(self):
        if self._by_key is None:
            self._by_key = self._build()
        return self._by_key

    def _build(self):
        by_key = {}
        for p in recorded_checkouts() | self.departing:
            rp = os.path.realpath(p)
            exists = os.path.isdir(rp) and rp not in self.departing
            for fam, key in checkout_keys(rp).items():
                prev = by_key.get((fam, key))
                # An existing checkout always wins: one live owner is enough.
                if prev is None or (exists and not prev[1]):
                    by_key[(fam, key)] = (rp, exists)
        return by_key

    def decide(self, fam, key):
        hit = self.by_key.get((fam, key))
        if hit is None:
            return UNDECIDED, "no checkout the engine has recorded derives the key %s" % key
        path, exists = hit
        if exists:
            return UNDECIDED, "its checkout %s exists, but no live run owns this device" % path
        return COLLECT, "its checkout %s no longer exists" % path


# ---------------------------------------------------------------------------
# iOS simulators
# ---------------------------------------------------------------------------

def simctl(*args, timeout=120):
    base = (os.environ.get("RICHOS_SIMCTL") or "").strip()
    cmd = [base] if base else ["xcrun", "simctl"]
    try:
        r = subprocess.run(cmd + list(args), capture_output=True, text=True, timeout=_timeout(timeout), env=_env())
    except BudgetExpired:
        raise
    except FileNotFoundError:
        return 127, "", "simctl is not installed"
    except (OSError, subprocess.TimeoutExpired) as e:
        return 124, "", str(e)
    return r.returncode, r.stdout, r.stderr


def ios_devices(device_set=""):
    """[{udid, name, state}], [] where there is no simulator tooling, None when
    the tooling exists and could not be read."""
    if not (os.environ.get("RICHOS_SIMCTL") or "").strip() and (
            sys.platform != "darwin" or not machine_devices_allowed()):
        return []
    rc, out, _err = simctl(*((["--set", device_set] if device_set else []) + ["list", "devices", "--json"]))
    if rc == 127:
        return None
    if rc != 0:
        return None
    try:
        groups = (json.loads(out) or {}).get("devices") or {}
    except ValueError:
        return None
    devs = []
    for lst in groups.values():
        for d in lst or []:
            if isinstance(d, dict) and d.get("udid"):
                devs.append({"udid": d["udid"], "name": d.get("name") or "", "state": d.get("state") or "", "device_set": device_set})
    return devs


def device_created(udid):
    try:
        st = os.stat(os.path.join(sim_devices_dir(), udid))
    except OSError:
        return None
    return getattr(st, "st_birthtime", None) or st.st_mtime


def _cache_record_for(regs, basename, kind):
    for r in regs:
        if r["kind"] == kind and os.path.basename(r["id"].rstrip("/")) == basename:
            return r
    return None


def classify_ios(dev, regs, checkouts):
    """(verdict, why) for one simulator, or (None, '') when it is not ours."""
    name, udid = dev["name"], dev["udid"]
    for r in regs:
        if r["kind"] == "ios-simulator" and r["id"] == udid:
            if r.get("generation", {}).get("device_set", "") == dev.get("device_set", ""):
                return _registered_verdict(r)
            return UNDECIDED, "the registered simulator belongs to a different device set"
    m = FAMILY_NATIVE.match(name)
    if m:
        r = _cache_record_for(regs, m.group(1) + (m.group(2) or ""), "ios-cache")
        if r:
            return UNDECIDED, "legacy cache ownership does not identify this simulator; register its UDID"
        return checkouts.decide("suite" if m.group(2) else "rios", m.group(1))
    m = FAMILY_RIOS_UI.match(name)
    if m:
        pid = int(m.group(1))
        st = process_start(pid)
        if st is None:
            return UNDECIDED, "its creator, pid %d, could not be read" % pid
        if st == "":
            return COLLECT, "its creator, pid %d, no longer exists" % pid
        born, started = device_created(udid), _epoch(st)
        if born is None or started is None:
            return UNDECIDED, "pid %d runs, and whether it is the creator cannot be told" % pid
        if started > born + 1:
            return COLLECT, "pid %d started %s, after the device was made: the number was reused" % (pid, st)
        return LEAVE, "its creator, pid %d, still runs" % pid
    m = FAMILY_LOOP.match(name)
    if m:
        return checkouts.decide("loop", m.group(1))
    if FAMILY_PLATFORM.match(name):
        return UNDECIDED, "nothing records which run made it"
    return None, ""


def _ios_remove(dev):
    """(gone, how). Shut down if running, delete, then read the list back."""
    udid = dev["udid"]
    prefix = ["--set", dev["device_set"]] if dev.get("device_set") else []
    registered = _read_json(_record_path("ios-simulator", udid)) or {}
    if registered.get("prepared"):
        simctl(*(prefix + ["shutdown", udid]))
        after = ios_devices(dev.get("device_set", ""))
        if after is not None and all(d["state"] == "Shutdown" for d in after if d["udid"] == udid):
            return True, "shut down; prepared OS retained for reuse"
        return False, "prepared simulator did not shut down"
    if dev.get("state") and dev["state"] != "Shutdown":
        simctl(*(prefix + ["shutdown", udid]))
    rc, _o, err = simctl(*(prefix + ["delete", udid]))
    after = ios_devices(dev.get("device_set", ""))
    if after is not None and not any(d["udid"] == udid for d in after):
        return True, "shut down and deleted"
    return False, "simctl delete exited %d: %s" % (rc, (err or "").strip()[:200] or "it is still listed")


# ---------------------------------------------------------------------------
# Android emulators — only ones a launcher RECORDED, by the pid it recorded
# ---------------------------------------------------------------------------

def _names_avd(pid, avd):
    a = process_args(pid)
    try:
        args = shlex.split(a or "")
        return any(args[i:i+2] == ["-avd", avd] for i in range(len(args)))
    except ValueError:
        return False


def android_emulators(regs):
    """[{pid, avd, cache, key}] for every emulator.json whose recorded process
    still runs AND still has `-avd <avd>` in its arguments."""
    caches = set(r["id"] for r in regs if r["kind"] in ("android-cache", "android-emulator"))
    root = android_caches_root()
    try:
        for n in os.listdir(root) if root else []:
            caches.add(os.path.join(root, n))
    except FileNotFoundError:
        pass
    out = []
    for c in sorted(caches):
        rec = _read_json(os.path.join(c, "emulator.json"))
        if rec is None and os.path.exists(os.path.join(c, "emulator.json")):
            raise ValueError("unreadable emulator identity: %s" % c)
        if rec is None:
            continue
        if not isinstance(rec, dict) or not rec.get("pid") or not rec.get("avd"):
            raise ValueError("invalid emulator identity: %s" % c)
        start = process_start(rec["pid"])
        if start == "":
            registered = next((r for r in regs if r["kind"] == "android-emulator" and r["id"] == c), {})
            generation = registered.get("generation") or {}
            recorded_start = rec.get("start") or (generation.get("start") if generation.get("pid") == rec["pid"] and generation.get("avd") == rec["avd"] else None)
            out.append({"pid": int(rec["pid"]), "avd": rec["avd"], "cache": c, "key": os.path.basename(c),
                        "name": rec["avd"], "state": "Shutdown", "start": recorded_start})
            continue
        if start is None:
            raise ValueError("emulator process identity could not be read: %s" % rec["pid"])
        if process_args(rec["pid"]) is None:
            raise ValueError("emulator process identity could not be read: %s" % rec["pid"])
        if not _names_avd(rec["pid"], rec["avd"]):
            continue
        out.append({"pid": int(rec["pid"]), "avd": rec["avd"], "cache": c, "key": os.path.basename(c),
                    "name": rec["avd"], "state": "Running", "start": process_start(rec["pid"])})
    return out


def classify_android(emu, regs, checkouts):
    for r in regs:
        if r["kind"] == "android-emulator" and os.path.realpath(r["id"]) == os.path.realpath(emu["cache"]):
            expected = {"pid": emu["pid"], "start": emu.get("start"), "avd": emu["avd"]}
            if r.get("generation") != expected:
                return UNDECIDED, "emulator generation changed; old ownership cannot authorize deletion"
            return _registered_verdict(r)
    if _cache_record_for(regs, emu["key"], "android-cache"):
        return UNDECIDED, "legacy cache ownership does not identify this emulator process"
    return checkouts.decide("randroid", emu["key"])


def _remove_avd(path):
    # A large tree can take longer than the hook budget even after its process
    # has ended. A separate, waited-for process makes deletion interruptible.
    result = subprocess.run([sys.executable, "-c", "import shutil,sys; shutil.rmtree(sys.argv[1])", path],
                            capture_output=True, text=True, timeout=_timeout(120), env=_env())
    if result.returncode:
        raise OSError(result.stderr.strip()[:300] or "AVD deletion failed")


def _android_remove(emu, remove_avd=True):
    pid, avd = emu["pid"], emu["avd"]
    recorded = _read_json(os.path.join(emu["cache"], "emulator.json"))
    if not recorded or recorded.get("pid") != pid or recorded.get("avd") != avd:
        return False, "emulator record changed; no cleanup attempted"
    for sig, wait in ((signal.SIGTERM, 15.0), (signal.SIGKILL, 5.0)):
        current_start = process_start(pid)
        if current_start is None:
            return False, "emulator identity unreadable; no cleanup attempted"
        if current_start == "":
            break
        if current_start != emu.get("start") or not _names_avd(pid, avd):
            return False, "emulator process identity changed; no signal sent"
        recorded = _read_json(os.path.join(emu["cache"], "emulator.json"))
        if not recorded or recorded.get("pid") != pid or recorded.get("avd") != avd:
            return False, "emulator record changed; no signal sent"
        try:
            os.kill(pid, sig)
        except OSError:
            pass
        deadline = time.time() + _timeout(wait)
        while time.time() < deadline:
            state = process_start(pid)
            if state is None:
                return False, "emulator identity unreadable after signal; AVD preserved"
            if state == "":
                break
            if state != emu.get("start"):
                return False, "emulator PID was reused after signal; AVD preserved"
            time.sleep(min(0.2, _timeout(0.2)))
    state = process_start(pid)
    if state is None:
        return False, "emulator identity unreadable; AVD preserved"
    if state:
        return False, "pid %d survived SIGTERM and SIGKILL" % pid
    try:
        avd_dir = os.path.join(emu["cache"], "avd")
        if remove_avd and os.path.exists(avd_dir):
            _remove_avd(avd_dir)
        rec = os.path.join(emu["cache"], "emulator.json")
        if os.path.exists(rec):
            os.unlink(rec)
    except (OSError, subprocess.TimeoutExpired) as e:
        return False, "the emulator ended but its AVD could not be removed: %s" % e
    return True, "ended and its AVD removed" if remove_avd else "ended"


# ---------------------------------------------------------------------------
# the one entry point, and the durable failure record
# ---------------------------------------------------------------------------

def _command(kind, d):
    if kind == "ios":
        prefix = "xcrun simctl" + (" --set " + shlex.quote(d["device_set"]) if d.get("device_set") else "")
        return "%s shutdown %s; %s delete %s" % (prefix, shlex.quote(d["udid"]), prefix, shlex.quote(d["udid"]))
    return "kill %d   (after checking that ps -o args= -p %d still names -avd %s)" % (d["pid"], d["pid"], d["avd"])


def _collect(apply=False, departing=(), deadline=None):
    """Classify every device in our families; remove the proven-orphaned ones.

    {"collected", "survivors", "left", "undecided", "deferred", "notes"}.
    `deadline` (epoch seconds) bounds the removals: what is not reached stays
    for the next pass and is reported as deferred, never dropped."""
    res = {"collected": [], "survivors": [], "left": [], "undecided": [], "deferred": [], "notes": []}
    regs = records()
    checkouts = Checkouts(departing)
    seen = {"ios": None, "ios_names": set(), "android": set()}
    devs = ios_devices()
    if devs is None:
        res["notes"].append("the simulator list could not be read; iOS devices were not checked")
    else:
        seen["ios"] = set(d["udid"] for d in devs)
        seen["ios_names"] = set(d["name"] for d in devs)
    all_devs = list(devs or [])
    sets = sorted({r.get("generation", {}).get("device_set") for r in regs
                   if r.get("generation") and r.get("generation", {}).get("device_set")})
    for device_set in sets:
        extra = ios_devices(device_set)
        if extra is None:
            res["notes"].append("simulator set could not be read: %s" % device_set)
            seen["ios"] = None
        else:
            all_devs.extend(extra)
            if seen["ios"] is not None:
                seen["ios"].update(d["udid"] for d in extra)
            seen["ios_names"].update(d["name"] for d in extra)
    work = [("ios", d, classify_ios(d, regs, checkouts)) for d in all_devs]
    try:
        emulators = android_emulators(regs)
    except (OSError, ValueError) as exc:
        res["notes"].append("Android inventory could not be read: %s" % exc)
        seen["android"] = None
        emulators = []
    for e in emulators:
        seen["android"].add(e["pid"])
        work.append(("android", e, classify_android(e, regs, checkouts)))
    for kind, d, (verdict, why) in work:
        if verdict is None:
            continue
        row = {"kind": kind, "id": d.get("udid") or d.get("pid"), "name": d["name"], "state": d.get("state"),
               "why": why, "command": _command(kind, d)}
        if verdict == LEAVE:
            res["left"].append(row)
        elif verdict == UNDECIDED:
            res["undecided"].append(row)
        elif not apply:
            row["how"] = "would be removed (dry run)"
            res["collected"].append(row)
        elif deadline is not None and time.time() >= deadline:
            row["how"] = "not reached inside this pass's budget; the next pass collects it"
            res["deferred"].append(row)
        else:
            gone, how = (_ios_remove if kind == "ios" else _android_remove)(d)
            row["how"] = how
            (res["collected"] if gone else res["survivors"]).append(row)
            if gone and kind == "ios":
                if seen["ios"] is not None:
                    seen["ios"].discard(d["udid"])
                seen["ios_names"].discard(d["name"])
    if apply:
        _prune_registry(regs, seen)
        res["standing"] = record_failures(res["survivors"], res["undecided"], seen, notes=res["notes"], deferred=res["deferred"])
    return res


def _alert_writer():
    path = os.path.join(os.path.dirname(__file__), "testdevice_alerts.py")
    spec = importlib.util.spec_from_file_location("device_alert_writer", path)
    writer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(writer)
    return writer


def _collect_command():
    return "python3 %s collect --apply" % shlex.quote(os.path.abspath(__file__))


def record_collector_failure(message):
    return _alert_writer().record_failure(message, _collect_command())


def collect(apply=False, departing=(), deadline=None):
    token = _DEADLINE.set(deadline)
    mark = _MARK.set(None)
    try:
        with registry_lock():
            if apply:
                # A killed collector leaves an alert instead of a false clean bill. The mark
                # names this process, so while it still runs the mark reads "running", not
                # failed (testdevice_alerts.standing); a standing failure is left as it is.
                writer = _alert_writer()
                writer.record_running(_collect_command())
                _MARK.set(writer)
            return _collect(apply, departing, deadline)
    except Exception as exc:
        res = {k: [] for k in ("collected", "survivors", "left", "undecided", "deferred")}
        res["notes"] = ["test-device collector failed: %s" % exc]
        if apply:
            res["standing"] = record_collector_failure(res["notes"][0])
        return res
    except BaseException:
        # Interrupted while this process lives on: the run did not complete.
        if apply:
            record_collector_failure("cleanup started but has not completed")
        raise
    finally:
        _MARK.reset(mark)
        _DEADLINE.reset(token)


def cleanup_run_simulators(run_id, budget=15):
    """Finalize this run's registered simulators after its supervised processes end.

    CoreSimulator is a daemon: killing a process tree cannot stop its devices.
    Never infer ownership from names or delete another run's registered device.
    The regular collector/watchdog remains the fallback after runner SIGKILL.
    Lock admission uses the command wait; budget bounds cleanup after acquisition.
    """
    errors = []
    token = None
    try:
        # Owned processes have already ended. An absent registry cannot contain
        # this run's device, so cleanup must not create state for an empty run.
        # Other lookup errors still take the visible failure path below.
        try:
            os.stat(registry_dir())
        except FileNotFoundError:
            return []
        # Waits as long as the collector tolerates: a healthy holder can sit in simctl for 120 s.
        with registry_lock(COLLECTOR_BUSY_ALERT_SECONDS):
            token = _DEADLINE.set(time.time() + budget)
            regs = [r for r in records() if r["kind"] == "ios-simulator" and any(
                o.get("verification_run") == run_id for o in r.get("owners", [r.get("owner", {})]))]
            inventories = {}
            for r in regs:
                device_set = r.get("generation", {}).get("device_set", "")
                if device_set not in inventories:
                    inventories[device_set] = ios_devices(device_set)
                devices = inventories[device_set]
                if devices is None:
                    errors.append("simulator inventory unreadable: " + device_set)
                    continue
                device = next((d for d in devices if d["udid"] == r["id"]), None)
                if device is None:
                    continue
                verdict, why = _registered_verdict(r)
                if verdict != COLLECT:
                    ours = [o for o in r.get("owners", [r.get("owner", {})]) if o.get("verification_run") == run_id]
                    if verdict == LEAVE and all(owner_state(o)[0] == "gone" for o in ours):
                        continue  # another live owner still has its own lease
                    errors.append(r["id"] + ": " + why)
                    continue
                gone, why = _ios_remove(device)
                if not gone:
                    errors.append(r["id"] + ": " + why)
    except Exception as exc:
        errors.append("run simulator cleanup failed: " + str(exc))
    finally:
        if token is not None:
            _DEADLINE.reset(token)
    if errors:
        record_collector_failure("; ".join(errors))
    return errors


def _prune_registry(regs, seen):
    """A record whose device is gone and whose owner is gone has nothing left to
    say. One whose owner still runs stays: its test may not have booted yet."""
    for r in regs:
        if _registered_verdict(r)[0] != COLLECT:
            continue
        if r["kind"] == "ios-simulator":
            if seen["ios"] is None or r["id"] in seen["ios"]:
                continue
        elif r["kind"] == "ios-cache":
            if seen["ios"] is None or \
                    ("RichOS native-ios " + os.path.basename(r["id"].rstrip("/"))) in seen["ios_names"]:
                continue
        elif r["kind"] in ("android-cache", "android-emulator"):
            if seen["android"] is None:
                continue
            rec = _read_json(os.path.join(r["id"], "emulator.json"))
            if isinstance(rec, dict) and rec.get("pid") in seen["android"]:
                continue
        try:
            os.unlink(r["_path"])
        except OSError:
            pass


def read_failures(path=None):
    rows = _read_json(path or failures_path())
    return rows if isinstance(rows, dict) else {}


def record_failures(survivors, undecided, seen, path=None, notes=(), deferred=()):
    """Carry every device that could not be collected, or whose owner cannot be
    proven; drop every row whose device no longer exists, or that this pass
    decided after all. A listing that could not be read resolves nothing."""
    path = path or failures_path()
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    rows = read_failures(path)
    live = {}
    for verdict, lst in (("would not close", survivors), ("owner cannot be proven", undecided)):
        for d in lst or []:
            # AN UNPROVEN OWNER IS ALERTED ONLY WHILE THE DEVICE RUNS. A running
            # simulator or emulator consumes resources after its run ends;
            # a shut-down one whose checkout nothing recorded costs disk, which
            # the disk alert already watches, and nine of them sat on this
            # machine when this was written (`RichOS mobile loop <key>` from
            # checkouts no record names). Alerting on those would put a
            # permanent nine-row block in front of the one row that matters.
            # They stay in the collector's output and log as UNDECIDED.
            if verdict == "owner cannot be proven" and d.get("state") == "Shutdown":
                continue
            live["%s:%s" % (d["kind"], d["id"])] = (verdict, d)
    if notes or deferred:
        live["collector:incomplete"] = ("collection incomplete", {
            "kind": "collector", "id": "incomplete", "name": "test-device collector",
            "why": "; ".join(notes) or "%d device(s) deferred by the cleanup deadline" % len(deferred),
            "command": "python3 %s collect --apply" % shlex.quote(os.path.abspath(__file__))})
    for key in list(rows):
        if key in live:
            continue
        kind, _, ident = key.partition(":")
        if kind in ("ios", "android") and seen[kind] is None:
            continue                    # nothing was read, so nothing is resolved
        del rows[key]
    for key, (verdict, d) in live.items():
        prev = rows.get(key) or {}
        rows[key] = {"kind": d["kind"], "id": d["id"], "name": d["name"], "verdict": verdict,
                     "why": d.get("how") or d.get("why") or "?", "command": d["command"],
                     "first": prev.get("first") or stamp, "last": stamp,
                     "attempts": int(prev.get("attempts") or 0) + 1}
    if rows:
        _write_json(path, rows)
    elif os.path.exists(path):
        try:
            os.unlink(path)
        except OSError:
            _write_json(path, {})
    return rows


# How long a device launch waits, in total, for its machine worker token, a live slot and a
# boot slot. The admission rules are unchanged (the boot slot still samples CPU and memory every
# 30 s); only the wait is longer. At 60 s each, a busy host turned a test into a red result: on
# 2026-09-25 native-ios-ui's second device failed "worker admission timed out after 60s" and
# native-ios-app failed "simulator boot admission exceeded 60s", both while other runs held the
# machine. A launch still refuses when the bound is reached; it is never unbounded.
DEVICE_ADMISSION_SECONDS = 900


def _admitted_keeping_lease(kind, ident, every=None):
    """_device_admission() for a device whose lease already exists, renewing that lease's
    inactivity clock while the wait lasts. The wait is bounded (DEVICE_ADMISSION_SECONDS) and
    can outlast the 300 s inactivity limit; this call is the owner's own activity on the lease.
    touch_lease never renews an expired lease or touches the lifetime."""
    import threading
    every = LEASE_RENEW_SECONDS if every is None else every
    done = threading.Event()

    def keep():
        while not done.wait(every):
            try:
                touch_lease(kind, ident)
            except (ValueError, TimeoutError, OSError):
                return            # gone or expired: boot_ios refuses it after admission
    keeper = threading.Thread(target=keep, daemon=True)
    keeper.start()
    try:
        return _device_admission()
    finally:
        done.set()
        keeper.join(timeout=every + 10)


def _device_admission():
    """The same boot/live pool as iOS, and a machine token held until shutdown."""
    from pathlib import Path
    import cpu_guard
    cpu_guard.require_managed_ancestor()
    import worker_tokens
    if not cpu_guard.healthy():
        raise RuntimeError("CPU watchdog is unhealthy; device launch refused")
    sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "app/scripts/lib"))
    import simulator_budget
    held = []
    deadline = time.monotonic() + DEVICE_ADMISSION_SECONDS

    def left():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("device admission exceeded %gs" % DEVICE_ADMISSION_SECONDS)
        return remaining
    try:
        held.append(worker_tokens.Budget(worker_tokens.machine_directory(), runner=True).acquire(timeout=left()))
        held.append(simulator_budget.acquire("live", timeout=left()))
        held.append(simulator_budget.acquire("boot", timeout=left()))
        return held
    except BaseException:
        for token in held:
            token.release()
        raise


def _transfer_device_leases(tokens, kind, ident):
    if not tokens:
        return
    # The boot lease ends after the launch transaction. The live and worker
    # leases transfer by inherited FDs, without LOCK_UN on the shared description.
    tokens.pop().release()
    fds = tuple(fd for token in tokens for fd in token.fds)
    holder = subprocess.Popen([sys.executable, __file__, "hold-lease", "--kind", kind, "--id", ident],
                     pass_fds=fds, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)
    path = _record_path(kind, ident)
    rec = _read_json(path)
    rec["lease"]["holder"] = {"pid": holder.pid, "start": process_start(holder.pid)}
    _write_json(path, rec)
    for token in tokens:
        for fd in token.fds:
            os.close(fd)
        token.fd = None
    tokens.clear()


def hold_lease(kind, ident):
    generation = None
    while True:
        rec = _read_json(_record_path(kind, ident))
        if not rec:
            return 0
        created = (rec.get("lease") or {}).get("created")
        if generation is not None and created != generation:
            return 0
        generation = created
        holder = (rec.get("lease") or {}).get("holder")
        if holder and holder["pid"] != os.getpid():
            return 0
        if kind == "android-emulator":
            gen = rec.get("generation") or {}
            current = process_start(gen.get("pid", 0))
            if current is not None and current != gen.get("start"):
                return 0                 # None is "could not be told": ask again next turn
        else:
            devices = ios_devices((rec.get("generation") or {}).get("device_set", ""))
            if devices is not None and not any(d["udid"] == ident and d.get("state") != "Shutdown" for d in devices):
                return 0
        time.sleep(2)


def boot_ios(udid):
    import cpu_guard
    cpu_guard.require_managed_ancestor()
    cpu_guard.require_ios()
    if not _read_json(_record_path("ios-simulator", udid)):
        raise ValueError("register the exact simulator before booting it")
    tokens = _admitted_keeping_lease("ios-simulator", udid)
    deadline_token = None
    try:
        cpu_guard.require_ios()
        with registry_lock():
            rec = _read_json(_record_path("ios-simulator", udid))
            if not rec:
                raise RuntimeError("simulator lease ended during admission")
            if (rec.get("boot") or {}).get("phase") in ("starting", "ready"):
                raise RuntimeError("simulator already starting or ready; release its lease before another boot")
            manifest_path = os.path.join(prepared_dir(), rec["prepared"] + ".json") if rec.get("prepared") else None
            manifest = (_read_json(manifest_path) or {}) if manifest_path else {}
            seconds = (cpu_guard.IOS_WARM_BOOT_SECONDS if manifest.get("boot_completed_at")
                       else cpu_guard.IOS_FIRST_BOOT_SECONDS)
            started = time.time()
            boot = dict(phase="starting", started=started, deadline=started + seconds)
            rec["boot"] = boot
            _write_json(_record_path("ios-simulator", udid), rec)
        deadline_token = _DEADLINE.set(boot["deadline"])
        try:
            if rec.get("prepared"):
                # This lease is the pool's only one, so a prepared device found running is
                # an orphan: on 2026-09-24 the collector ended a run's expired lease and
                # shut the device down, the run's own xcodebuild booted it again, and the
                # next lease's boot failed "Unable to boot device in current state: Booted".
                # Shut the exact UDID down, and boot it clean as this lease promises.
                found = next((d for d in ios_devices() or [] if d["udid"] == udid), None)
                if found and found.get("state") not in ("", "Shutdown"):
                    checked_simctl("shutdown", udid)
                    cpu_guard.note("Prepared simulator was running without a lease; shut down before boot",
                                   device=udid, state=found["state"])
            checked_simctl("boot", udid)
            with registry_lock():
                _transfer_device_leases(tokens, "ios-simulator", udid)
            checked_simctl("bootstatus", udid, "-b", timeout=seconds)
            if rec.get("prepared"):
                # Reset app data, keychain and permissions, retaining the OS's
                # completed first boot. Never simctl erase a prepared device.
                apps = simulator_apps(checked_simctl("listapps", udid))
                for bundle, app in apps.items():
                    if app.get("ApplicationType") == "User":
                        checked_simctl("uninstall", udid, bundle)
                checked_simctl("keychain", udid, "reset")
                checked_simctl("privacy", udid, "reset", "all")
            with registry_lock():
                current = _read_json(_record_path("ios-simulator", udid)) or {}
                if current.get("boot") != boot:
                    raise RuntimeError("simulator lease ended during startup")
                current["boot"] = dict(boot, phase="ready", completed=time.time())
                _write_json(_record_path("ios-simulator", udid), current)
                if manifest_path:
                    _write_json(manifest_path, dict(manifest, boot_completed_at=time.time()))
        except BaseException:
            if time.time() >= boot["deadline"]:
                cpu_guard.block_ios("Simulator startup deadline exceeded: " + udid, automatic=True)
            cleanup_token = _DEADLINE.set(time.time() + 12)
            try:
                rc, _, err = simctl("shutdown", udid)
                if rc:
                    # A concurrent shutdown can finish before simctl answers. Confirm
                    # this exact device is stopped before escalating to a global incident.
                    stopped = next((d for d in ios_devices() or [] if d["udid"] == udid), None)
                    if stopped and stopped.get("state") == "Shutdown":
                        rc = 0
                if rc:
                    cpu_guard.block_ios("Failed startup cleanup for %s: %s" % (udid, err), automatic=True)
                else:
                    with registry_lock():
                        current = _read_json(_record_path("ios-simulator", udid)) or {}
                        if current.get("boot") == boot:
                            current["boot"] = dict(boot, phase="failed", completed=time.time())
                            _write_json(_record_path("ios-simulator", udid), current)
            finally:
                _DEADLINE.reset(cleanup_token)
            raise
    finally:
        if deadline_token is not None:
            _DEADLINE.reset(deadline_token)
        for token in tokens:
            token.release()


def checked_simctl(*args, timeout=120):
    rc, out, err = simctl(*args, timeout=timeout)
    if rc:
        raise RuntimeError("simctl %s: %s" % (args[0], err))
    return out


def simulator_apps(output):
    try:
        return plistlib.loads(output.encode())
    except plistlib.InvalidFileException:
        # simctl versions also emit OpenStep property lists, which plistlib
        # cannot parse. Apple's plutil accepts both formats.
        result = subprocess.run(['/usr/bin/plutil', '-convert', 'json', '-o', '-', '-'],
                                input=output, capture_output=True, text=True, timeout=_timeout(10), check=True)
        return json.loads(result.stdout)


def prepared_dir():
    # Tests put their registry under a sandbox; the real pool's manifest lives
    # on the external SSD. Device storage keeps the approved Apple default.
    if not machine_devices_allowed():
        return os.path.join(registry_dir(), "prepared")
    return str(M.cache("richos-ios-prepared"))


def same_owner(a, b):
    keys = ("pid", "start") if a.get("pid") else ("workspace_key", "session_id", "agent_id")
    return not a.get("unknown") and all(a.get(k) is not None and a.get(k) == b.get(k) for k in keys)


# A caller that names no deadline waits for as long as the holder can legitimately
# keep the one device: the longest declared lease lifetime, plus a minute for the
# holder's release. A shorter default made two ordinary callers fail an admission
# while a healthy test simply held the device (P5-24). "As long as the holder can
# legitimately keep it" includes a run that keeps renewing its lease: lease_expired()
# lets such a lease live to LEASE_ACTIVE_CEILING_SECONDS, so the default wait covers
# that ceiling too (P5-24, v2 re-check: a renewed lease at 1900 s outlived a 1860 s wait).
POOL_WAIT_SECONDS = max([LEASE_MAX_SECONDS, LEASE_ACTIVE_CEILING_SECONDS, *LEASE_PURPOSES.values()]) + 60
POOL_WAIT_ENV = "RICHOS_IOS_POOL_WAIT"


def pool_wait_seconds():
    """How long `acquire-ios` waits for the one machine-wide prepared-simulator lease.

    POOL_WAIT_SECONDS (the longest a lease can stay valid, a renewed one's ceiling included,
    plus a minute) unless RICHOS_IOS_POOL_WAIT
    names another positive number of seconds. A gate
    that runs several simulator suites side by side sets it to its own deadline: on
    2026-09-25 native-ios-app's A8 held the pool for about 26 minutes, so native-ios-ui
    and native-ios-app, started together by the nightly script-suites gate, could each
    only fail after 300 s waiting for the other (run 20260925T190759Z-2b4b0a7e: A8's
    lease acquire gave up at exactly 300 s). Waiting longer queues them; it never lets
    two runs share a device, and a lease's own lifetime is untouched. A value that is
    not a positive number is refused, never read as the default."""
    raw = (os.environ.get(POOL_WAIT_ENV) or "").strip()
    if not raw:
        return POOL_WAIT_SECONDS
    try:
        value = float(raw)
    except ValueError:
        raise ValueError("%s=%r is not a number of seconds" % (POOL_WAIT_ENV, raw)) from None
    if not value > 0 or value != value or value == float("inf"):
        raise ValueError("%s=%r must be a positive, finite number of seconds" % (POOL_WAIT_ENV, raw))
    return value


POOL_LEASES = 1
POOL_LEASES_ENV = "RICHOS_IOS_POOL_LEASES"


def pool_leases():
    """How many prepared-simulator leases the machine hands out at once.

    1 unless RICHOS_IOS_POOL_LEASES names another whole number of at least 1. The nightly
    sets it from its own `--simulated-phones` flag, which the operator must give on every
    build (nightly-local.py): that is where the decision is made, from what else is running
    on the Mac. Every other caller -- a land, an engineer's suite, `rios` by hand -- keeps
    the one lease at a time it has always had. A value that is not a whole number of at
    least 1 is refused, never read as the default.

    Raising it never lets two runs share a device: each device type and runtime still has
    exactly one prepared simulator, so a second run wanting the SAME type waits for it
    whatever this says. It admits runs on DIFFERENT types side by side (the nightly's
    iPhone SE, iPhone 16 Pro and iPhone 16 Pro Max). Booting is still admitted separately,
    by simulator_budget.py's machine-wide live and boot limits and the CPU/memory check."""
    raw = (os.environ.get(POOL_LEASES_ENV) or "").strip()
    if not raw:
        return POOL_LEASES
    if not raw.isdigit() or int(raw) < 1:
        raise ValueError("%s=%r must be a whole number of simulators, at least 1" % (POOL_LEASES_ENV, raw))
    return int(raw)


def acquire_ios(device_type, runtime, owner_pid=None, checkout="", timeout=POOL_WAIT_SECONDS, purpose=None,
                leases=None):
    """`leases` active pool leases per machine (pool_leases(): 1 unless the nightly raised it);
    one retained OS per type/runtime, which one run holds at a time.

    `purpose` names a declared lifetime from LEASE_PURPOSES for a NEW lease.
    An existing lease is never lengthened by acquiring it again."""
    import cpu_guard
    cpu_guard.require_managed_ancestor()
    if purpose is not None and purpose not in LEASE_PURPOSES:
        raise ValueError("unknown lease purpose %r (declared: %s)" % (purpose, ", ".join(sorted(LEASE_PURPOSES))))
    limit = pool_leases() if leases is None else int(leases)
    if limit < 1:
        raise ValueError("at least one prepared-simulator lease is needed, not %r" % leases)
    cpu_guard.require_ios()
    owner = choose_owner(owner_pid, checkout, "prepared-ios")
    if owner.get("unknown"):
        raise ValueError("prepared simulator needs a durable owner")
    key = hashlib.sha256((device_type + "\0" + runtime).encode()).hexdigest()[:20]
    path = os.path.join(prepared_dir(), key + ".json")
    deadline = time.monotonic() + timeout
    while True:
        cpu_guard.require_ios()
        with registry_lock():
            busy = [r for r in records() if r.get("prepared") and
                    not (r["prepared"] == key and same_owner(r.get("owner", {}), owner))]
            # Full, or THIS type's one prepared simulator is another run's: wait either way.
            # At one lease the second condition is implied by the first, which is the old rule.
            if len(busy) < limit and not any(r["prepared"] == key for r in busy):
                manifest = _read_json(path)
                inventory = ios_devices()
                if inventory is None:
                    raise RuntimeError("cannot read simulator inventory")
                if manifest and not any(d["udid"] == manifest["udid"] for d in inventory):
                    raise RuntimeError("prepared simulator was deleted; repair the pool explicitly instead of silently cold-booting")
                if not manifest:
                    udid = checked_simctl("create", "RichOS prepared " + key, device_type, runtime).strip()
                    manifest = dict(udid=udid, device_type=device_type, runtime=runtime)
                    _write_json(path, manifest)
                udid = manifest["udid"]
                previous = _read_json(_record_path("ios-simulator", udid))
                if previous:
                    if not same_owner(previous.get("owner", {}), owner):
                        raise RuntimeError("prepared simulator already belongs to another run")
                    touch_lease("ios-simulator", udid)
                else:
                    rec = register("ios-simulator", udid, owner_pid, "prepared-ios", checkout)
                    rec["prepared"] = key
                    if purpose is not None:
                        rec["lease"]["max_seconds"] = LEASE_PURPOSES[purpose]
                        rec["lease"]["purpose"] = purpose
                    _write_json(_record_path("ios-simulator", udid), rec)
                return udid
        if time.monotonic() >= deadline:
            raise TimeoutError("prepared simulator is leased by another run")
        time.sleep(.5)


def release_ios(udid, owner_pid=None, checkout="", if_ours=False):
    """Shut down and unlease the caller's own prepared simulator.

    Releasing another run's lease is refused. With `if_ours`, a cleanup path that may run
    after its own lease already ended (and the pool leased the device to someone else)
    leaves that other run's device alone and returns, instead of failing: on 2026-09-25 a
    UI suite's final release reached a device another run had leased a minute later."""
    with registry_lock():
        rec = _read_json(_record_path("ios-simulator", udid))
        if not rec:
            return
        owner = choose_owner(owner_pid, checkout, "prepared-ios")
        if not same_owner(rec.get("owner", {}), owner):
            if if_ours:
                sys.stderr.write("testdevices: %s is leased by another run now; left alone\n" % udid)
                return
            raise ValueError("cannot release another run's simulator")
        ok, why = _ios_remove(dict(udid=udid, state="Booted", device_set=""))
        if not ok:
            raise RuntimeError(why)
        os.unlink(_record_path("ios-simulator", udid))


def use_ios(udid, owner_pid=None, checkout=""):
    with registry_lock():
        rec = _read_json(_record_path("ios-simulator", udid)) or {}
        owner = choose_owner(owner_pid, checkout, "prepared-ios")
        if not rec.get("prepared") or not same_owner(rec.get("owner", {}), owner):
            raise ValueError("no prepared simulator lease for this run; prepare again")
        touch_lease("ios-simulator", udid)


def lease_report(now=None):
    """Read-only: every registered device lease, its age and its inactivity, for status."""
    now = time.time() if now is None else now
    rows = []
    for rec in records():
        lease = rec.get("lease")
        if not lease:
            continue
        owner = rec.get("owner") or {}
        rows.append({"kind": rec.get("kind"), "id": rec.get("id"), "script": owner.get("script"),
                     "owner_pid": owner.get("pid"), "purpose": lease.get("purpose"),
                     "max_seconds": lease.get("max_seconds"), "idle_seconds": lease.get("idle_seconds"),
                     "age": round(now - lease.get("created", now), 1),
                     "idle": round(now - lease.get("last_use", now), 1),
                     "activity": lease.get("activity"), "expired": lease_expired(rec, now)})
    return rows


def _busy_path():
    return os.path.join(registry_dir(), ".collector-busy.json")


def expire_leases(pressure=False):
    """Bounded collection of exact registered devices, without workspace discovery.

    A registry lock held by another caller is a SKIPPED cycle, never a collector failure:
    the watchdog runs this every two seconds, and a failure closes native admission. On
    2026-09-24 the collector died on "registry lock stayed busy for five seconds" under
    load 26-39. The first skip of a busy spell is noted in the guard's history, the next
    cycle retries, and a spell longer than COLLECTOR_BUSY_ALERT_SECONDS is a failure again,
    so a wedged lock is never tolerated silently.
    """
    import cpu_guard
    token = _DEADLINE.set(time.time() + 12)
    try:
        try:
            code = _expire_leases(pressure)
        except RegistryBusy as exc:
            busy = _read_json(_busy_path()) or {}
            now = time.time()
            since = busy.get("since", now)
            busy = {"since": since, "skipped": busy.get("skipped", 0) + 1, "last": now}
            _write_json(_busy_path(), busy)
            if busy["skipped"] == 1:
                cpu_guard.note("Device lease collection skipped: registry lock busy; retrying next cycle",
                               error=str(exc))
            if now - since >= COLLECTOR_BUSY_ALERT_SECONDS:
                message = ("CPU device lease cleanup: registry lock busy for %d s (%d cycles skipped)"
                           % (now - since, busy["skipped"]))
                record_collector_failure(message)
                print(message, file=sys.stderr)
                return 1
            print("CPU device lease cleanup: skipped, %s" % exc, file=sys.stderr)
            return 0
        busy = _read_json(_busy_path())
        if busy:
            cpu_guard.note("Device lease collection resumed after a busy registry",
                           skipped=busy.get("skipped"), seconds=round(time.time() - busy.get("since", time.time())))
            os.unlink(_busy_path())
        return code
    finally:
        _DEADLINE.reset(token)


def _expire_leases(pressure=False):
    errors = []
    with registry_lock():
        regs = records()
        if pressure:
            regs.sort(key=lambda rec: rec.get("kind") != "ios-simulator")
        for rec in regs:
            if rec.get("kind") not in ("android-emulator", "ios-simulator"):
                continue
            # Simulator services are started by launchd, outside a CLI's process
            # tree. Under sustained host pressure, stop only exact registered
            # simulator UDIDs. Do not guess ownership from service names.
            if pressure and rec["kind"] == "ios-simulator":
                verdict, why = COLLECT, "sustained host CPU pressure; registered simulator shed for headroom"
            else:
                verdict, why = _registered_verdict(rec)
            if verdict != COLLECT:
                continue
            if rec["kind"] == "android-emulator":
                gen = rec.get("generation") or {}
                if not gen.get("pid") or not gen.get("start") or not gen.get("avd"):
                    errors.append("unproven emulator generation: " + rec["id"])
                    continue
                # _android_remove rechecks the PID generation before each signal.
                ok = True
                if os.path.exists(os.path.join(rec["id"], "emulator.json")):
                    ok, detail = _android_remove(dict(gen, cache=rec["id"]))
                    if not ok: errors.append(detail)
                if ok:
                    os.unlink(_record_path(rec["kind"], rec["id"]))
            else:
                device_set = (rec.get("generation") or {}).get("device_set", "")
                # Shutdown the exact registered UDID first during an incident.
                # Neither owner ps nor a full CoreSimulator inventory is needed
                # to authorize this operation, and both stalled in the incident.
                inventory = ([dict(udid=rec["id"], state="Booted", device_set=device_set)]
                             if pressure else ios_devices(device_set))
                if inventory is None:
                    errors.append("cannot read simulator inventory")
                    continue
                ok = True
                for dev in inventory:
                    if dev["udid"] == rec["id"]:
                        ok, detail = _ios_remove(dev)
                        if not ok: errors.append(detail)
                if ok:
                    os.unlink(_record_path(rec["kind"], rec["id"]))
            if ok and rec.get("lease"):
                import cpu_guard
                cpu_guard.event("Device lease ended; owned device stopped", device=rec["id"], reason=why)
        if errors:
            record_collector_failure("CPU device lease cleanup: " + "; ".join(errors))
            print("CPU device lease cleanup: " + "; ".join(errors), file=sys.stderr)
    return 1 if errors else 0


def launch_android(cache, avd, port, command, checkout="", owner_pid=None, script="randroid"):
    """Publish process identity and ownership in one registry transaction."""
    cache = os.path.realpath(cache)
    os.makedirs(cache, exist_ok=True)
    tokens = _device_admission()
    try:
        return _launch_android_admitted(cache, avd, port, command, checkout, owner_pid, script, tokens)
    finally:
        for token in tokens:
            token.release()


def _launch_android_admitted(cache, avd, port, command, checkout, owner_pid, script, tokens):
    with registry_lock():
        path = os.path.join(cache, "emulator.json")
        old = _read_json(path)
        if isinstance(old, dict) and old.get("pid") and old.get("avd") and _names_avd(old["pid"], old["avd"]):
            register("android-emulator", cache, owner_pid, script, checkout)
            return old["pid"]
        with open(os.path.join(cache, "emulator.log"), "ab") as log:
            child = subprocess.Popen(command, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                                     start_new_session=True)
        try:
            until = time.monotonic() + 2
            while not _names_avd(child.pid, avd) and child.poll() is None and time.monotonic() < until:
                time.sleep(0.02)
            if child.poll() is not None or not _names_avd(child.pid, avd):
                raise ValueError("emulator did not start with the expected AVD")
            _write_json(path, {"pid": child.pid, "avd": avd, "port": port,
                               "serial": "emulator-%d" % port, "start": process_start(child.pid)})
            register("android-emulator", cache, owner_pid, script, checkout)
            _transfer_device_leases(tokens, "android-emulator", cache)
            return child.pid
        except BaseException:
            child.terminate()
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=2)
            raise


def stop_android(cache, delete=False):
    """An explicit CLI stop still verifies identity and serializes with launch."""
    cache = os.path.realpath(cache)
    with registry_lock():
        path = os.path.join(cache, "emulator.json")
        rec = _read_json(path)
        if rec is None:
            if os.path.exists(path):
                raise ValueError("emulator identity is unreadable; no cleanup attempted")
            if delete and os.path.isdir(os.path.join(cache, "avd")):
                _remove_avd(os.path.join(cache, "avd"))
            return
        if not isinstance(rec, dict) or not rec.get("pid") or not rec.get("avd"):
            raise ValueError("invalid emulator identity; no cleanup attempted")
        current = process_start(rec["pid"])
        stored = rec.get("start")
        if not stored:
            registered = _read_json(_record_path("android-emulator", cache)) or {}
            gen = registered.get("generation") or {}
            if gen.get("pid") == rec["pid"] and gen.get("avd") == rec["avd"]:
                stored = gen.get("start")
        if current is None or (current and (not stored or current != stored)):
            raise ValueError("emulator process generation is unproven; no signal sent")
        gone, why = _android_remove(dict(rec, cache=cache, start=stored), remove_avd=delete)
        if not gone:
            raise ValueError(why)


def _main(argv):
    ap = argparse.ArgumentParser(description="Test simulators and emulators: register, find, collect.")
    sub = ap.add_subparsers(dest="cmd")
    pool = sub.add_parser("acquire-ios")
    pool.add_argument("--type", required=True)
    pool.add_argument("--runtime", required=True)
    pool.add_argument("--owner-pid", type=int)
    pool.add_argument("--checkout", default="")
    pool.add_argument("--purpose", choices=sorted(LEASE_PURPOSES),
                      help="a declared purpose whose lease lifetime is longer (never a number)")
    active = sub.add_parser("run-active", help="run COMMAND as the owned activity of an existing lease")
    active.add_argument("--kind", required=True, choices=["android-emulator", "ios-simulator"])
    active.add_argument("--id", required=True)
    active.add_argument("--owner-pid", type=int)
    active.add_argument("--checkout", default="")
    active.add_argument("--interval", type=float, default=None, help=argparse.SUPPRESS)
    active.add_argument("--check", type=float, default=2.0, help=argparse.SUPPRESS)
    active.add_argument("--lost-file", default="",
                        help="where to record why the lease was lost, if it is (exit 75 = NOT RUN)")
    active.add_argument("command", nargs=argparse.REMAINDER)
    release = sub.add_parser("release-ios")
    release.add_argument("--id", required=True)
    release.add_argument("--owner-pid", type=int)
    release.add_argument("--checkout", default="")
    release.add_argument("--if-ours", action="store_true",
                         help="for cleanup paths: a device another run now leases is left alone, not an error")
    use = sub.add_parser("use-ios")
    use.add_argument("--id", required=True)
    use.add_argument("--owner-pid", type=int)
    use.add_argument("--checkout", default="")
    expire = sub.add_parser("expire-leases")
    expire.add_argument("--pressure", action="store_true")
    sub.add_parser("leases", help="print every registered device lease as JSON (read-only)")
    touch = sub.add_parser("touch-lease")
    touch.add_argument("--kind", required=True, choices=["android-emulator", "ios-simulator"])
    touch.add_argument("--id", required=True)
    hold = sub.add_parser("hold-lease")
    hold.add_argument("--kind", required=True)
    hold.add_argument("--id", required=True)
    boot = sub.add_parser("boot-ios")
    boot.add_argument("--id", required=True)
    r = sub.add_parser("register", help="record that the calling test owns a device, or the cache naming it")
    r.add_argument("--kind", required=True, choices=KINDS)
    r.add_argument("--id", required=True, help="a simulator UDID, or a cache directory")
    r.add_argument("--owner-pid", type=int)
    r.add_argument("--checkout", default="")
    r.add_argument("--device-set", default="")
    r.add_argument("--script", default="")
    launch = sub.add_parser("launch-android", help="launch and register an emulator atomically")
    launch.add_argument("--cache", required=True)
    launch.add_argument("--avd", required=True)
    launch.add_argument("--port", required=True, type=int)
    launch.add_argument("--checkout", default="")
    launch.add_argument("--owner-pid", type=int)
    launch.add_argument("command", nargs=argparse.REMAINDER)
    stop = sub.add_parser("stop-android", help="stop one recorded emulator after verifying its generation")
    stop.add_argument("--cache", required=True)
    stop.add_argument("--delete", action="store_true")
    c = sub.add_parser("collect", help="report (default) or remove (--apply) orphaned test devices")
    c.add_argument("--apply", action="store_true")
    c.add_argument("--json", action="store_true")
    c.add_argument("--departing", action="append", default=[],
                   help="a checkout being deleted right now: treat it as gone")
    c.add_argument("--budget", type=float, default=0.0, help="seconds for removals (0 = unbounded)")
    a = ap.parse_args(argv)
    if a.cmd not in ("expire-leases", "collect", "leases"):
        global REGISTRY_LOCK_SECONDS
        REGISTRY_LOCK_SECONDS = CLI_REGISTRY_LOCK_SECONDS
    if a.cmd == "acquire-ios":
        print(acquire_ios(a.type, a.runtime, a.owner_pid, a.checkout, purpose=a.purpose,
                          timeout=pool_wait_seconds(), leases=pool_leases()))
        return 0
    if a.cmd == "run-active":
        command = a.command[1:] if a.command[:1] == ["--"] else a.command
        if not command:
            ap.error("run-active requires a command after --")
        try:
            return run_active(a.kind, a.id, command, a.owner_pid, a.checkout, a.interval, a.check, a.lost_file)
        except ValueError as e:
            sys.stderr.write("testdevices run-active: %s\n" % e)
            return 2
    if a.cmd == "release-ios":
        release_ios(a.id, a.owner_pid, a.checkout, a.if_ours)
        return 0
    if a.cmd == "use-ios":
        use_ios(a.id, a.owner_pid, a.checkout)
        return 0
    if a.cmd == "leases":
        print(json.dumps({"at": time.time(), "leases": lease_report()}, sort_keys=True))
        return 0
    if a.cmd == "touch-lease":
        touch_lease(a.kind, a.id)
        return 0
    if a.cmd == "expire-leases":
        return expire_leases(a.pressure)
    if a.cmd == "hold-lease":
        return hold_lease(a.kind, a.id)
    if a.cmd == "boot-ios":
        boot_ios(a.id)
        return 0
    if a.cmd == "stop-android":
        stop_android(a.cache, a.delete)
        return 0
    if a.cmd == "launch-android":
        command = a.command[1:] if a.command[:1] == ["--"] else a.command
        if not command:
            ap.error("launch-android requires an emulator command")
        print(launch_android(a.cache, a.avd, a.port, command, a.checkout, a.owner_pid))
        return 0
    if a.cmd == "register":
        try:
            register(a.kind, a.id, a.owner_pid, a.script, a.checkout, a.device_set)
        except ValueError as e:
            sys.stderr.write("testdevices: %s\n" % e)
            return 2
        return 0
    if a.cmd != "collect":
        ap.print_help()
        return 2
    res = collect(apply=a.apply, departing=a.departing,
                  deadline=(time.time() + a.budget) if a.budget > 0 else None)
    if a.json:
        print(json.dumps(res, indent=2, sort_keys=True, default=str))
    else:
        for n in res["notes"]:
            print("NOTE         %s" % n)
        for key in ("collected", "survivors", "undecided", "deferred", "left"):
            for d in res[key]:
                print("%-12s %s %s (%s): %s" % (key.upper(), d["kind"], d["id"], d["name"],
                                                d.get("how") or d["why"]))
        print("verdict: collected=%d survivors=%d undecided=%d deferred=%d left=%d"
              % tuple(len(res[k]) for k in ("collected", "survivors", "undecided", "deferred", "left")))
    if res["survivors"] or res["notes"] or res["deferred"]:
        return 1
    if res["undecided"]:
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(_main(sys.argv[1:]))
