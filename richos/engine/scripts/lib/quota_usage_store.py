"""The desktop's per-account quota record and flock protocol (quota/shared_usage.rs)."""

import contextlib
from datetime import datetime
import fcntl
import functools
import hashlib
import json
import os
from pathlib import Path
import pwd
import subprocess
import sys
import tempfile
import time


INTERVAL_MS = 300_000
FAST_INTERVAL_MS = 60_000
FAST_ABOVE_PERCENT = 80
FAST_POINTS_PER_CHECK = 5


def fast_until(answer, previous):
    at = answer["checkedAt"]
    gap = at - previous.get("checkedAt", at) if previous else 0
    if gap < 30_000:
        return None
    older = {w["id"]: w for w in previous.get("windows", [])}
    until = None
    for window in answer["windows"]:
        end = window.get("resetsAt") or at + INTERVAL_MS
        used = window["usedPercent"]
        if end <= at or used <= FAST_ABOVE_PERCENT:
            continue
        name = window["id"]
        if name != "five_hour" and not name.startswith(("seven_day", "model:")):
            continue
        old = older.get(name)
        if not old:
            continue
        a, b = window.get("resetsAt"), old.get("resetsAt")
        same = (a is None and b is None) or (a is not None and b is not None and abs(a - b) < FAST_INTERVAL_MS)
        rise = (used - old["usedPercent"]) * INTERVAL_MS / gap
        if same and rise >= FAST_POINTS_PER_CHECK:
            stamp = min(end, at + INTERVAL_MS)
            until = max(until or 0, stamp)
    return until


def answer(checked_at, windows):
    return {"checkedAt": checked_at, "windows": [
        {"id": window["id"], "label": window["label"], "usedPercent": window["used"],
         "resetsAt": window["resets_at"] * 1000 if window["resets_at"] else None,
         "durationMs": 18_000_000 if window["id"] == "five_hour" else 604_800_000}
        for window in windows
    ]}


def control_result(checked_at, payload):
    """Normalize a raw get_usage payload for diagnostics that keep their own wire."""
    import quota_weekly

    def epoch(value):
        try:
            return int(datetime.fromisoformat(value).timestamp())
        except (ValueError, TypeError):
            return None

    if isinstance(payload, dict) and payload.get("rate_limits_available") is True:
        if "rate_limits" in payload and payload["rate_limits"] is None:
            return {"refusal": {"at": checked_at, "error": "noReading"}}
        windows = quota_weekly.windows(payload.get("rate_limits"), epoch)
        if windows:
            return {"answer": answer(checked_at, windows)}
    return {"refusal": {"at": checked_at, "error": "failed"}}


def data_dir():
    if sys.platform == "darwin":
        return Path(pwd.getpwuid(os.geteuid()).pw_dir) / "Library/Application Support/com.richos.app"
    root = os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share"
    return Path(root) / "com.richos.app"


@functools.lru_cache(maxsize=16)
def _account_identity(binary, _folder, _period):
    if binary:
        env = {key: value for key, value in os.environ.items() if key != "CLAUDECODE"}
        try:
            result = subprocess.run([binary, "auth", "status", "--json"], env=env, stdin=subprocess.DEVNULL,
                                    capture_output=True, timeout=15, check=True)
            status = json.loads(result.stdout)
            email = status.get("email", "").strip().lower()
            org = status.get("orgId") or ""
            if not isinstance(org, str):
                org = ""
            if email:
                return hashlib.sha256(f"account\0{email}\0{org}".encode()).hexdigest()
        except (OSError, ValueError, AttributeError, subprocess.SubprocessError):
            pass
    return None


class Store:
    def __init__(self, root, key):
        self.folder = Path(root) / "claude-usage"
        self.key = key
        self.verified = True
        self.record = self.folder / (key + ".json")
        self.lock = self.folder / (key + ".lock")

    @classmethod
    def for_account(cls, root, binary, folder):
        folder = str(Path(folder).resolve())
        folder_key = hashlib.sha256(f"folder\0{folder}".encode()).hexdigest()
        identity = _account_identity(binary, folder, int(time.monotonic()) // 60)
        binding = Path(root) / "claude-usage" / (folder_key + ".identity")
        previous = cls.read(binding)
        if identity is not None:
            if identity != previous:
                try:
                    binding.parent.mkdir(parents=True, exist_ok=True)
                    cls.write(binding, identity)
                except OSError:
                    pass
            return cls(root, identity)
        valid = isinstance(previous, str) and len(previous) == 64 and all(c in "0123456789abcdef" for c in previous)
        store = cls(root, previous if valid else folder_key)
        # Auth status can be temporarily unavailable. A cached answer remains
        # readable without granting an unverified reader a second budget.
        store.verified = False
        return store

    @staticmethod
    def read(path):
        try:
            with path.open("rb") as stream:
                return json.loads(stream.read(262_145))
        except (OSError, ValueError):
            return None

    def saved(self):
        record = self.read(self.record)
        return record if isinstance(record, dict) else {}

    @contextlib.contextmanager
    def locked(self, nonblocking):
        self.folder.mkdir(parents=True, exist_ok=True)
        fd = os.open(self.lock, os.O_CREAT | os.O_WRONLY, 0o600)
        try:
            flags = fcntl.LOCK_EX | (fcntl.LOCK_NB if nonblocking else 0)
            try:
                fcntl.flock(fd, flags)
            except BlockingIOError:
                yield False
                return
            yield True
        finally:
            os.close(fd)

    @staticmethod
    def write(target, value):
        fd, path = tempfile.mkstemp(dir=target.parent, suffix=".tmp")
        try:
            with os.fdopen(fd, "w") as stream:
                json.dump(value, stream, separators=(",", ":"))
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(path, target)
        finally:
            Path(path).unlink(missing_ok=True)

    @staticmethod
    def interval(record, now):
        fast_until = record.get("fastUntil")
        return FAST_INTERVAL_MS if isinstance(fast_until, int) and now < fast_until and not record.get("refusal") else INTERVAL_MS

    @classmethod
    def next_request_at(cls, record, now):
        last = max((at for name in ("requestedAt", "finishedAt") if isinstance(at := record.get(name), int)), default=None)
        refusal = record.get("refusal")
        retry = refusal.get("retryAt") if isinstance(refusal, dict) else None
        refused = refusal.get("at") if isinstance(refusal, dict) else None
        return max(last + cls.interval(record, now) if last is not None else 0,
                   retry if isinstance(retry, int) else 0,
                   refused + INTERVAL_MS if isinstance(refused, int) else 0)

    def claim(self, now, source="terminal"):
        if not self.verified:
            return False
        if now < self.next_request_at(self.saved(), now):
            return False
        with self.locked(nonblocking=True) as locked:
            if not locked:
                return False
            saved = self.saved()
            if now < self.next_request_at(saved, now):
                return False
            cadence = self.interval(saved, now)
            saved["requestedAt"] = now
            self.write(self.record, saved)
        print(f"quota request: account={self.key} at_ms={now} source={source} interval_ms={cadence}", file=sys.stderr)
        return True

    def finish(self, requested_at, *, answer=None, refusal=None):
        with self.locked(nonblocking=False):
            saved = self.saved()
            if saved.get("requestedAt") != requested_at:
                return
            outcome_at = answer["checkedAt"] if answer is not None else (refusal or {}).get("at", requested_at)
            saved["finishedAt"] = max(requested_at, outcome_at)
            if answer is not None:
                saved["fastUntil"] = fast_until(answer, saved.get("answer"))
                saved["answer"] = answer
            saved["refusal"] = refusal
            self.write(self.record, saved)
