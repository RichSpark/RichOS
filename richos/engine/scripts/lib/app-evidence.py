#!/usr/bin/env python3
"""Project native hook callbacks into the transcript shape engine guards read.

The app owns the raw conversation ledger. These private files are a disposable
guard-evidence projection, not conversation history or organizational memory.
No callback is inferred from an attempted action. In particular, PreToolUse does
not establish execution and SubagentStop does not establish successful work.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess
import sys
import time


def project(payload, instruction=None):
    """Preserve actual prompt/tool identities and separate worker sidechains."""
    record = {
        "promptId": payload.get("prompt_id"),
        "cwd": payload.get("cwd"),
        "isSidechain": bool(payload.get("agent_id")),
        "agentId": payload.get("agent_id"),
        "evidenceSource": "richos-native-hook-v1",
    }
    event = payload.get("hook_event_name")
    if event == "UserPromptSubmit":
        # Runtime input can also be hidden priming or quoted context. Only the
        # app ledger can attest that a given message is a user instruction.
        record.update(type="user", promptSource="runtime",
                      message={"content": payload.get("prompt", "")})
        if (not payload.get("agent_id") and isinstance(instruction, dict)
                and isinstance(payload.get("prompt"), str)
                and instruction.get("sha256") == hashlib.sha256(payload["prompt"].encode()).hexdigest()
                and str(instruction.get("ledger_ref", "")).startswith("ledger:")):
            record.update(promptSource="sdk", origin={"kind":"human"},
                          ledgerReference=instruction["ledger_ref"],
                          evidenceSource="richos-ledger-attested-hook-v1")
    elif event == "PreToolUse":
        if not payload.get("tool_use_id") or not payload.get("tool_name"):
            raise ValueError("Tool callback has no tool identity")
        record.update(type="assistant", message={"content": [{
            "type": "tool_use", "id": payload["tool_use_id"],
            "name": payload["tool_name"], "input": payload.get("tool_input", {}),
        }]})
    elif event in ("PostToolUse", "PostToolUseFailure"):
        if not payload.get("tool_use_id"):
            raise ValueError("Tool result has no tool identity")
        response = payload.get("tool_response", payload.get("error", ""))
        # Native transcript tool_result content is a string or content blocks,
        # whereas hook tool_response may be an object. Preserve its structure.
        if not isinstance(response, (str, list)):
            response = json.dumps(response, ensure_ascii=False)
        record.update(type="user", message={"content": [{
            "type": "tool_result", "tool_use_id": payload["tool_use_id"],
            "content": response, "is_error": event == "PostToolUseFailure",
        }]})
    else:
        return None
    if not record["promptId"]:
        raise ValueError("Callback has no prompt identity; refusing session-wide evidence")
    return record


def append(path, value):
    fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "a") as stream:
        stream.write(json.dumps(value, ensure_ascii=False) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


# ---- The output record's witnesses (b) and (c) --------------------------------------
#
# Output side panel PRD, richos-hq docs/prds/2026-10-05-output-side-panel.md, §4.1 (b)/(c)
# and §4.8. Two kinds of row go to evidence/<session>/writes.jsonl, PATHS ONLY (no file
# content, no command text), and the app joins them to a thread by session:
#   (b) a PostToolUse for Write/Edit/MultiEdit/NotebookEdit, the tool's own path;
#   (c) a file a Bash command made, seen on the DISK: a regular file in a directory the
#       command ran in or named, or a file it named, whose mtime is at or after the command
#       started. A second pass at Stop / SubagentStop catches what finished in the background.
# A pass that fails never fails the tool call: it is reported and appends nothing.

WRITE_TOOLS = ("Write", "Edit", "MultiEdit", "NotebookEdit")
WRITES_FILE = "writes.jsonl"
DIRECTORY_ENTRY_CAP = 2000
NEVER_INSIDE = frozenset((".git", "node_modules", "target"))
SAFE_NAME = re.compile(r"[A-Za-z0-9_-]{1,128}")


def now_ms():
    return time.time_ns() // 1_000_000


def _absolute(raw, cwd):
    if not isinstance(raw, str) or not raw or "\0" in raw:
        return None
    path = os.path.expanduser(raw)
    if not os.path.isabs(path):
        if not isinstance(cwd, str) or not os.path.isabs(cwd):
            return None
        path = os.path.join(cwd, path)
    return os.path.normpath(path)


def _under(path, root):
    return path == root or path.startswith(root.rstrip("/") + "/")


def _excluded(path, excluded, allowed=()):
    """Never inside .git, node_modules or target; never anything under the app's own data,
    EXCEPT under an `allowed` root (§4.1 (c)'s carve-out, slice S2b): every back-end worker's
    workspace is `<app-data>/engine-state/target-worktrees/<scope>/<name>`, so excluding the data
    directory wholesale dropped every file a worker's command made (a `pandoc` PDF built in its
    worktree, 2026-10-05). The NEVER_INSIDE names still apply inside an allowed root: a
    worktree's `.git` is a file, and its build output is not a deliverable."""
    if any(part in NEVER_INSIDE for part in Path(path).parts):
        return True
    if any(_under(path, root) for root in allowed):
        return False
    return any(_under(path, root) for root in excluded)


def command_tokens(command):
    """The command's words: split on whitespace and ; && || | > >> ( ), quotes stripped,
    --flag=value split on '='. Bare flags are not paths."""
    if not isinstance(command, str):
        return []
    try:
        lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|<>()")
        lexer.whitespace_split = True
        lexer.commenters = ""
        words = list(lexer)
    except ValueError:  # unbalanced quotes: fall back to the separators alone
        words = re.split(r"[\s;&|<>()]+", command)
    out = []
    for word in words:
        word = word.strip("'\"")
        if not word or set(word) <= set(";&|<>()"):
            continue
        if word.startswith("-"):
            if "=" not in word:
                continue
            word = word.split("=", 1)[1]
        out.append(word)
    return out


def candidates(cwd, command, excluded, allowed=()):
    """The call's candidate set (§4.1 (c)): the working directories (the callback's cwd and
    every word that is an existing directory) and the explicit files (every word that is an
    existing regular file)."""
    dirs, files = set(), set()
    start = _absolute(cwd, None)
    if start and os.path.isdir(start):
        dirs.add(os.path.realpath(start))
    for word in command_tokens(command):
        path = _absolute(word, cwd)
        if not path:
            continue
        try:
            st = os.lstat(path)
        except (OSError, ValueError):
            continue
        if stat.S_ISDIR(st.st_mode):
            dirs.add(os.path.realpath(path))
        elif stat.S_ISREG(st.st_mode):
            files.add(os.path.realpath(path))
    return ({d for d in dirs if not _excluded(d, excluded, allowed)},
            {f for f in files if not _excluded(f, excluded, allowed)})


def scan(dirs, files, start_ns, excluded, allowed=()):
    """Every regular file (lstat, no follow) among the explicit files and one level inside
    each directory whose mtime is at or after the start. A directory over the entry cap is
    not listed; dot-entries are skipped; names and lstat only, never contents.

    The start is floored to its whole second: a file system that stamps whole seconds would
    otherwise put a file written in the same second as the start before it. The cost is a
    file changed earlier in that same second, inside the declared false-positive class."""
    floor = start_ns - start_ns % 1_000_000_000
    found = {}

    def consider(path):
        try:
            st = os.lstat(path)
        except OSError:
            return
        if stat.S_ISREG(st.st_mode) and st.st_mtime_ns >= floor and not _excluded(path, excluded, allowed):
            found[path] = st.st_mtime_ns

    for path in files:
        consider(path)
    for folder in dirs:
        try:
            entries = list(os.scandir(folder))
        except OSError:
            continue
        if len(entries) > DIRECTORY_ENTRY_CAP:
            continue
        for entry in entries:
            if not entry.name.startswith("."):
                consider(os.path.join(folder, entry.name))
    return found


def _write_row(payload, path, source, tool_use_id=None, mtime_ns=None):
    row = {"schema": 1, "session_id": payload["session_id"], "agent_id": payload.get("agent_id") or None,
           "tool_use_id": tool_use_id, "path": path, "at": now_ms(), "source": source}
    if mtime_ns is not None:
        row["mtime_ns"] = mtime_ns
    return row


def _read_json(path):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    except OSError:
        return None
    with os.fdopen(fd) as stream:
        try:
            return json.load(stream)
        except ValueError:
            return None


def _write_json(path, value):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(value, stream)


def _actor_file(commands, agent_id):
    """The turn-end ledger of one actor: the lead, or one worker."""
    if agent_id:
        return commands / f"turn-agent-{agent_id}.json" if SAFE_NAME.fullmatch(agent_id) else None
    return commands / "turn-lead.json"


def witness_writes(payload, folder, excluded, allowed=()):
    """The rows witnesses (b) and (c) owe this callback, and the bookkeeping (c) needs.
    Runs under the evidence folder's lock, after the callback itself is kept."""
    event = payload.get("hook_event_name")
    tool = payload.get("tool_name")
    tool_use_id = payload.get("tool_use_id")
    agent_id = payload.get("agent_id") or None
    commands = folder / "commands"
    rows = []
    if event == "PostToolUse" and tool in WRITE_TOOLS:
        ti = payload.get("tool_input") or {}
        raw = ti.get("file_path") or ti.get("notebook_path") or ti.get("path")
        path = _absolute(raw, payload.get("cwd"))
        # No exclusion here. The data-directory and .git rules are the command witness's (c):
        # (b) records the path the tool itself names, and a back-end worker's target worktree
        # lives under the app's data directory (engine-state/target-worktrees), so excluding it
        # dropped every back-end worker's write (seen on the test VM, 2026-10-05).
        if path:
            rows.append(_write_row(payload, path, "hook", tool_use_id=tool_use_id))
    elif tool == "Bash" and isinstance(tool_use_id, str) and SAFE_NAME.fullmatch(tool_use_id):
        start = commands / f"{tool_use_id}.json"
        if event == "PreToolUse":
            # Taken BEFORE the command runs, which is what makes "mtime >= start" a write.
            commands.mkdir(mode=0o700, exist_ok=True)
            _write_json(start, {"at_ns": time.time_ns(), "cwd": payload.get("cwd"), "agent_id": agent_id})
        elif event in ("PostToolUse", "PostToolUseFailure"):
            began = _read_json(start)
            if isinstance(began, dict) and isinstance(began.get("at_ns"), int):
                command = (payload.get("tool_input") or {}).get("command")
                dirs, files = candidates(began.get("cwd") or payload.get("cwd"), command, excluded, allowed)
                for path, mtime in sorted(scan(dirs, files, began["at_ns"], excluded, allowed).items()):
                    rows.append(_write_row(payload, path, "command", tool_use_id=tool_use_id, mtime_ns=mtime))
                # Kept for this actor's turn-end pass: its directories, and its earliest start.
                ledger = _actor_file(commands, agent_id)
                if ledger is not None:
                    held = _read_json(ledger) or {}
                    _write_json(ledger, {
                        "at_ns": min(held.get("at_ns", began["at_ns"]), began["at_ns"]),
                        "dirs": sorted(set(held.get("dirs", [])) | dirs),
                        "files": sorted(set(held.get("files", [])) | files),
                    })
            try:
                os.unlink(start)
            except FileNotFoundError:
                pass
    elif event in ("Stop", "SubagentStop"):
        # Stop is the lead's turn end, SubagentStop a worker's run end.
        ledger = _actor_file(commands, agent_id if event == "SubagentStop" else None)
        held = _read_json(ledger) if ledger is not None else None
        if isinstance(held, dict) and isinstance(held.get("at_ns"), int):
            dirs = {d for d in held.get("dirs", []) if isinstance(d, str)}
            files = {f for f in held.get("files", []) if isinstance(f, str)}
            for path, mtime in sorted(scan(dirs, files, held["at_ns"], excluded, allowed).items()):
                rows.append(_write_row(payload, path, "command", mtime_ns=mtime))
            os.unlink(ledger)
    return rows


@contextlib.contextmanager
def _session_folder(state_root, session):
    """`<state_root>/<session>`, created and held under its `.lock` for the duration: the one
    lock every writer of a session's evidence takes (the hook's `capture`, and the land
    witness's `append_land_rows`)."""
    if not isinstance(session, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", session):
        raise ValueError("Invalid native session identity")
    root = Path(state_root)
    if not root.is_absolute():
        raise ValueError("Evidence root must be explicit and absolute")
    folder = root / session
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    if folder.is_symlink():
        raise ValueError("Session evidence directory cannot be a symlink")
    lock_fd = os.open(folder / ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(lock_fd, "r+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield folder


COMMIT = re.compile(r"[0-9a-f]{40,64}")


def append_land_rows(state_root, session_id, rows):
    """Witness (d), the land (Output side panel PRD §4.1 (d), slice S2b): one row per file a
    land wrote into the connected repository, appended to the back-end session's own
    `writes.jsonl` under the folder lock `capture` takes, so the app joins it to the thread by
    the same session as every other row of that job.

    `rows` are `{path, from, commit, worker: {name, agent_id}}`: `path` is the landed copy,
    `from` the worker's worktree copy it retires, `commit` the commit the ref moved to. Paths
    only, like every other row. A row that is not that shape refuses the whole call before
    anything is written; the caller's posture is that a refusal never fails the land."""
    out = []
    for row in rows:
        worker = row.get("worker") if isinstance(row, dict) else None
        if not (isinstance(worker, dict) and isinstance(worker.get("name"), str) and worker["name"]
                and all(isinstance(row.get(k), str) and os.path.isabs(row[k]) and "\0" not in row[k]
                        for k in ("path", "from"))
                and isinstance(row.get("commit"), str) and COMMIT.fullmatch(row["commit"])):
            raise ValueError("a land row needs absolute path and from, a commit and the worker's name")
        out.append({"schema": 1, "session_id": session_id, "agent_id": None, "tool_use_id": None,
                    "path": row["path"], "from": row["from"], "commit": row["commit"],
                    "worker": {"name": worker["name"], "agent_id": worker.get("agent_id") or None},
                    "at": now_ms(), "source": "land"})
    if not out:
        return 0
    with _session_folder(state_root, session_id) as folder:
        for row in out:
            append(folder / WRITES_FILE, row)
    return len(out)


def capture(payload, state_root, instruction=None, app_data=None, worktrees=None):
    session = payload.get("session_id", "")
    with _session_folder(state_root, session) as folder:
        root = folder.parent
        transcript = folder / "guard-transcript.jsonl"
        record = project(payload, instruction)
        append(folder / "callbacks.jsonl", {"schema": 1, "callback": payload})
        if record is not None:
            append(transcript, record)
        # The output record's witnesses. Never the app's own data, never this evidence, except
        # the worker worktrees under the data directory (§4.1 (c)'s carve-out). The evidence
        # root (`engine-state/evidence`) is not under `engine-state/target-worktrees`, so the
        # carve-out never reaches it.
        excluded = [os.path.realpath(root)] + ([os.path.realpath(app_data)] if app_data else [])
        allowed = [os.path.realpath(worktrees)] if worktrees else []
        try:
            for row in witness_writes(payload, folder, excluded, allowed):
                append(folder / WRITES_FILE, row)
        except (OSError, ValueError, TypeError) as error:
            print(f"RichOS output witness skipped this callback: {error}", file=sys.stderr)
        # SessionStart/Stop may arrive without a tool event. An empty projection
        # is valid; a missing or unwritable projection is an explicit failure.
        fd = os.open(transcript, os.O_CREAT | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
        os.close(fd)
    return {**payload, "transcript_path": str(transcript)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state-root", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        payload = capture(json.load(sys.stdin), args.state_root)
        command = args.command
        if command[:1] == ["--"]:
            command = command[1:]
        if command:
            return subprocess.run(command, input=json.dumps(payload), text=True).returncode
        print(json.dumps(payload))
        return 0
    except (OSError, ValueError) as error:
        print(f"RichOS guard evidence unavailable: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
