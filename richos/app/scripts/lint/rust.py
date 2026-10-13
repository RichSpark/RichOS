"""Clippy collection and lint rule configuration."""
import json
from pathlib import Path
import subprocess
import threading
import time
import tomllib
from common import APP, THIRD_PARTY, Refusal, finish_group, tracked

# WAITING FOR CARGO'S LOCK IS QUEUEING, NEVER CLIPPY'S WORK (audit R12,
# docs/verification/2026-09-29-load-sensitive-checks-audit.md). Cargo prints this line on
# stderr when another cargo holds the build-directory or package-cache lock, and nothing
# more until it has the lock. Until 2026-09-29 the Tauri cap counted that wait, so a cold
# Clippy queued behind another build could spend its 180 s before it started. The wait is
# now measured and reported, and bounded only by this hang guard: the same 1800 s the
# machine worker admission gives a queued command (worker_tokens.py).
LOCK_WAIT = "Blocking waiting for file lock"
LOCK_WAIT_GUARD = 1800
POLL = 0.1

FAST = [
    ["cargo", "clippy", "--locked", "--all-targets", "--manifest-path", APP + "Cargo.toml"],
    ["cargo", "clippy", "--locked", "--all-targets", "--manifest-path", APP + "crates/richos-user-update/Cargo.toml"],
]
TAURI = [["cargo", "clippy", "--locked", "--all-targets", "--manifest-path", APP + "src-tauri/Cargo.toml"]]


def lint_rules(root):
    """Record actual lint IDs, levels and package inheritance, not just counts."""
    rules = {"toolchain-default-diagnostics": "blocking-count-ceiling"}
    for path in tracked(root):
        if not path.startswith(APP) or path.startswith(THIRD_PARTY) or Path(path).name != "Cargo.toml":
            continue
        data = tomllib.loads((root / path).read_text())
        tables = {"package": data.get("lints", {}), "workspace": data.get("workspace", {}).get("lints", {})}
        for scope, table in tables.items():
            if scope == "package" and "package" in data:
                rules[path + ":inherits-workspace"] = str(table.get("workspace", False)).lower()
            for group in ("rust", "clippy"):
                for name, setting in table.get(group, {}).items():
                    rules[f"{path}:{scope}:{group}::{name}"] = json.dumps(setting, sort_keys=True)
    return rules


def run_cargo(args, *, cwd, timeout, clock):
    """common.run() for Cargo, whose `timeout` counts Cargo's own work only.

    From Cargo's LOCK_WAIT line to its next line on either stream, it is waiting for a lock:
    that time goes to clock["lock_wait"] and not against `timeout`, and a wait longer than
    LOCK_WAIT_GUARD is refused as a hang (TimeoutError). Past `timeout` of work it raises
    subprocess.TimeoutExpired, as run() does. The process group is owned either way."""
    started = time.monotonic()
    state = {"waited": 0.0, "since": None}
    guard = threading.Lock()
    out, err = [], []
    process = subprocess.Popen(
        [str(a) for a in args], cwd=cwd, text=True, errors="surrogateescape",
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        start_new_session=True)

    def read(stream, into):
        for text_line in iter(stream.readline, ""):
            with guard:
                now = time.monotonic()
                if state["since"] is not None:
                    state["waited"] += now - state["since"]
                    state["since"] = None
                if into is err and LOCK_WAIT in text_line:
                    state["since"] = now
            into.append(text_line)

    readers = [threading.Thread(target=read, args=(process.stdout, out), daemon=True),
               threading.Thread(target=read, args=(process.stderr, err), daemon=True)]
    for reader in readers:
        reader.start()
    try:
        while True:
            try:
                process.wait(timeout=POLL)
                break
            except subprocess.TimeoutExpired:
                pass
            with guard:
                now = time.monotonic()
                waiting = now - state["since"] if state["since"] is not None else 0.0
                work = now - started - state["waited"] - waiting
            if waiting >= LOCK_WAIT_GUARD:
                raise TimeoutError(f"Cargo's lock was still held by another build after "
                                   f"{LOCK_WAIT_GUARD}s of waiting (hang guard)")
            if work >= timeout:
                raise subprocess.TimeoutExpired(args, timeout)
    finally:
        try:
            finish_group(process)
        finally:
            for reader in readers:
                reader.join(timeout=5)
            for stream in (process.stdout, process.stderr):
                stream.close()
            with guard:
                now = time.monotonic()
                if state["since"] is not None:
                    state["waited"] += now - state["since"]
                    state["since"] = None
                clock["lock_wait"] = clock.get("lock_wait", 0.0) + state["waited"]
    return (subprocess.CompletedProcess(args, process.returncode, "".join(out), "".join(err)),
            time.monotonic() - started)


def collect(root, commands, deadline=None, clock=None):
    """Clippy over `commands`. `deadline` (monotonic) bounds Clippy's own work: it moves
    later by every second spent waiting for Cargo's lock, which `clock["lock_wait"]` sums."""
    counts, diagnostics = {}, []
    clock = {} if clock is None else clock
    clock.setdefault("lock_wait", 0.0)
    for command in commands:
        remaining = deadline + clock["lock_wait"] - time.monotonic() if deadline else 900
        if remaining <= 0:
            raise TimeoutError("Tauri Clippy deadline expired")
        result, _ = run_cargo(command + ["--message-format=json"], cwd=root, timeout=remaining,
                              clock=clock)
        if result.returncode:
            errors = []
            for line in result.stdout.splitlines():
                try:
                    item = json.loads(line)
                except ValueError:
                    continue
                message = item.get("message", {})
                if item.get("reason") == "compiler-message" and message.get("level") == "error":
                    errors.append(message.get("rendered") or message.get("message", ""))
            detail = "\n".join(errors) if errors else result.stderr[-3000:]
            raise Refusal(f"Clippy compilation failed: {detail}")
        finished, artifacts = False, 0
        try:
            for line in result.stdout.splitlines():
                item = json.loads(line)
                reason = item["reason"]
                if reason == "build-finished":
                    finished = item["success"] is True
                elif reason == "compiler-artifact":
                    artifacts += 1
                elif reason == "compiler-message":
                    message = item["message"]
                    if message["level"] not in {"warning", "error"}:
                        continue
                    rule = (message.get("code") or {}).get("code", "compiler-warning")
                    counts[rule] = counts.get(rule, 0) + 1
                    diagnostics.append({"rule": rule, "message": message["message"]})
            if not finished or not artifacts:
                raise ValueError("no successful build-finished event or no compiled targets")
        except (ValueError, KeyError, TypeError) as exc:
            raise Refusal(f"malformed or empty Clippy output: {exc}") from exc
    return counts, diagnostics
