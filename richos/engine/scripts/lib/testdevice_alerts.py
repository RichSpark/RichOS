"""Persist collector failures even when the collector itself cannot import."""
import json
import os
import subprocess
import tempfile
import time

KEY = "collector:incomplete"
# A COLLECTION STILL RUNNING IS NOT A FAILURE. The collector marks its start so a
# collector that is killed leaves an alert behind, but while its own process is
# alive and inside this bound the mark only says "running". The bound is the
# collector's longest legitimate registry hold (testdevices.COLLECTOR_BUSY_ALERT_SECONDS);
# past it, or once its process is gone, the same mark is a failure again.
RUNNING = "collection running"
RUNNING_ALERT_SECONDS = 180


def _path():
    base = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
    return os.environ.get("TEST_DEVICE_FAILURES_STATE") or os.path.join(base, "state/test-device-failures.json")


def _load(path):
    """The rows, and a note when the previous state could not be read."""
    try:
        with open(path) as handle:
            rows = json.load(handle)
        if not isinstance(rows, dict):
            raise ValueError("failure state is not an object")
        return rows, ""
    except FileNotFoundError:
        return {}, ""
    except (OSError, ValueError) as exc:
        return {}, "previous failure state unreadable: %s" % exc


def _store(path, rows):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".device-alert-", dir=os.path.dirname(path))
    try:
        with os.fdopen(fd, "w") as handle:
            json.dump(rows, handle, indent=1, sort_keys=True)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def _row(rows, message, command, verdict):
    previous = rows.get(KEY) or {}
    if not isinstance(previous, dict):
        previous = {}
    try:
        attempts = int(previous.get("attempts", 0)) + 1
    except (TypeError, ValueError):
        attempts = 1
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    return {"kind": "collector", "id": "incomplete", "name": "test-device collector",
            "verdict": verdict, "why": str(message), "command": command,
            "first": previous.get("first", stamp), "last": stamp,
            "attempts": attempts}


def record_failure(message, command="Restore the test-device collector and rerun scratch-reaper.sh --apply"):
    path = _path()
    rows, note = _load(path)
    if note:
        message = "%s; %s" % (message, note)
    rows[KEY] = _row(rows, message, command, "collection incomplete")
    _store(path, rows)
    return rows


def process_start(pid):
    """`ps -o lstart=` for pid (UTC, C locale), "" when it has ended, None when unknown."""
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return None
    if pid <= 0:
        return None
    try:
        r = subprocess.run(["ps", "-o", "stat=,lstart=", "-p", str(pid)], capture_output=True,
                           text=True, timeout=10,
                           env=dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0"))
    except (OSError, subprocess.TimeoutExpired):
        return None
    stat, _, text = r.stdout.strip().partition(" ")
    text = " ".join(text.split())
    if r.returncode == 0 and stat.startswith("Z"):
        return ""
    if r.returncode == 0 and text:
        return text
    if r.returncode in (0, 1) and not text:
        return ""
    return None


def record_running(command):
    """Mark a collection as started by this process. A standing collector failure
    is left exactly as it is: it stays a failure until a collection succeeds."""
    path = _path()
    rows, note = _load(path)
    if note:
        rows[KEY] = _row(rows, "cleanup started but has not completed; " + note,
                         command, "collection incomplete")
        _store(path, rows)
        return rows
    # The same rule the readers use: an earlier mark whose process died, was
    # replaced or ran past the bound is a failure, and it stays one, written
    # down with its verdict, until a collection succeeds (record_failures).
    previous = rows.get(KEY)
    judged = standing({KEY: previous}).get(KEY) if previous is not None else None
    if judged is not None:
        if judged is not previous:
            rows[KEY] = judged
            _store(path, rows)
        return rows
    row = _row(rows, "cleanup started but has not completed", command, RUNNING)
    row.update({"pid": os.getpid(), "pid_start": process_start(os.getpid()) or "",
                "started": time.time()})
    rows[KEY] = row
    _store(path, rows)
    return rows


def standing(rows, now=None):
    """The rows that are failures. A collection whose process is still running,
    inside RUNNING_ALERT_SECONDS, is left out; one whose process is gone, or
    replaced, or cannot be checked, or that has run past the bound, is in, with
    its verdict saying which."""
    now = time.time() if now is None else now
    out = {}
    for key, row in (rows or {}).items():
        if key != KEY or not isinstance(row, dict) or row.get("verdict") != RUNNING:
            out[key] = row
            continue
        started = row.get("started")
        if not isinstance(started, (int, float)) or now - started >= RUNNING_ALERT_SECONDS:
            verdict = "collection timed out"
        else:
            current = process_start(row.get("pid"))
            if row.get("pid_start") and current == row["pid_start"]:
                continue
            verdict = ("collection state unknown" if current is None or not row.get("pid_start")
                       else "collection abandoned")
        out[key] = dict(row, verdict=verdict)
    return out
