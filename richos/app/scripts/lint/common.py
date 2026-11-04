"""Shared lint inventory and subprocess handling. No third-party dependencies."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

APP = "richos/app/"
# Vendored third-party source (`.richos/vendored-material`): upstream's code, linted upstream.
THIRD_PARTY = APP + "third-party/"


class Refusal(RuntimeError):
    pass


def finish_group(process):
    """Bounded cleanup even when a successful wrapper leaves children behind."""
    def alive():
        process.poll()
        try:
            os.killpg(process.pid, 0)
            return True
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
    for sig in (signal.SIGTERM, signal.SIGKILL):
        if not alive():
            break
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            break
        except PermissionError:
            # macOS rejects the signal while the group's exiting orphans are reaped (the same
            # EPERM alive() counts as present; nightly-local.py stop_group). The bounded wait
            # and the final presence check below still decide.
            pass
        end = time.monotonic() + 2
        while alive() and time.monotonic() < end:
            time.sleep(.02)
    process.wait(timeout=2)
    if alive():
        raise Refusal(f"cleanup failed for owned process group {process.pid}")


def run(args, *, cwd, timeout=180, env=None, input=None):
    """Own the process group, including Cargo children and cache-lock waiters."""
    started = time.monotonic()
    process = subprocess.Popen(
        [str(a) for a in args], cwd=cwd, env=env, text=True, errors="surrogateescape",
        stdin=subprocess.PIPE if input is not None else subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        out, err = process.communicate(input, timeout=max(.001, timeout))
    finally:
        try:
            finish_group(process)
        finally:
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None:
                    stream.close()
    return subprocess.CompletedProcess(args, process.returncode, out, err), time.monotonic() - started


def checked(args, root, **kwargs):
    result, _ = run(args, cwd=root, **kwargs)
    if result.returncode:
        raise Refusal(f"{args[0]} failed ({result.returncode}): {result.stderr[-2000:]}")
    return result.stdout


def tracked(root):
    paths = checked(["git", "ls-files", "-z"], root).split("\0")
    return sorted(p for p in paths if p)


def classify(path):
    """The inventory row for one repository-relative path, or None when it is not scanned."""
    if not path.startswith(APP) or path.startswith(THIRD_PARTY):
        return None
    p = Path(path)
    if p.suffix not in {".sh", ".rs", ".js"}:
        return None
    if p.suffix == ".sh" and not path.startswith(APP + "scripts/"):
        return None
    if p.suffix == ".js" and not path.startswith(APP + "ui/"):
        return None
    role = "production"
    if "fixtures" in p.parts or "fixture" in p.parts:
        role = "fixture"
    elif p.name.endswith(".test.sh") or "tests" in p.parts or "test" in p.parts:
        role = "suite"
    return {"language": {".sh": "shell", ".rs": "rust", ".js": "javascript"}[p.suffix], "role": role}


def inventory(root):
    """One tracked inventory; fixtures are accounted for but never linted as products."""
    rows = {}
    for path in tracked(root):
        row = classify(path)
        if row is not None:
            rows[path] = row
    for language in ("shell", "rust", "javascript"):
        if not any(r["language"] == language for r in rows.values()):
            raise Refusal(f"zero-file {language} inventory")
    return rows


def select(rows, language):
    return [p for p, r in rows.items() if r["language"] == language and r["role"] != "fixture"]


def shellcheck(root, rows):
    paths = select(rows, "shell")
    if not paths:
        raise Refusal("zero-file ShellCheck scan")
    command = ["shellcheck", "--format=json", "--rcfile=" + APP + ".shellcheckrc", *paths]
    result, _ = run(command, cwd=root)
    if result.returncode not in (0, 1):
        raise Refusal(f"ShellCheck failed: {result.stderr}")
    try:
        diagnostics = json.loads(result.stdout)
        if not isinstance(diagnostics, list):
            raise ValueError("expected diagnostic array")
        counts = {}
        for d in diagnostics:
            if not isinstance(d["code"], int) or d["file"] not in paths:
                raise ValueError("invalid ShellCheck diagnostic")
            rule = "SC" + str(d["code"])
            counts[rule] = counts.get(rule, 0) + 1
        if result.returncode == 1 and not diagnostics:
            raise ValueError("failed scan without diagnostics")
    except (ValueError, KeyError, TypeError) as exc:
        raise Refusal(f"malformed ShellCheck output: {exc}") from exc
    return counts, diagnostics
