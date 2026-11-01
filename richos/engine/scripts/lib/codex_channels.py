#!/usr/bin/env python3
"""Local chat channels. The source journal is the outbox; acceptors own positions.

Shared by the CLI, the app's scoped tools, codex-watch and review-watch. No model,
native session ID, current-chat lookup or background service participates in routing.
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
import sys
import uuid

_diagnostics = {}  # changed failures only, no persistent routing state


def diagnostic(path, error):
    message = str(error)
    if _diagnostics.get(str(path)) != message:
        print("Channel held at %s: %s" % (path, message), file=sys.stderr)
        _diagnostics[str(path)] = message


VERSION = 1
TEXT_LIMIT = 128 * 1024
FRAME_LIMIT = 256 * 1024
SETUP_TEXT = ("Codex coordination is waiting. In the chat you coordinate from, "
              "tell Rich to take over the Codex channel.")


def root():
    return Path(os.environ.get("CODEX_COORDINATION_ROOT") or
                Path.home() / ".richos-coordination").resolve()


def component(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", value):
        raise ValueError("Invalid channel identity")
    return value


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def mkdir(path):
    path = Path(path)
    if path.exists():
        if path.is_symlink() or not path.is_dir():
            raise ValueError("Channel directory is not an ordinary directory: %s" % path)
        return
    mkdir(path.parent)
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        if path.is_symlink() or not path.is_dir():
            raise ValueError("Channel directory changed during creation") from None
    sync_dir(path.parent)


def save(path, value):
    """Atomic, synced derived state or endpoint, never a payload queue."""
    path = Path(path)
    mkdir(path.parent)
    temporary = path.with_name(path.name + ".incoming-" + uuid.uuid4().hex)
    try:
        with temporary.open("x", encoding="utf-8") as f:
            os.chmod(temporary, 0o600)
            f.write(json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n")
            f.flush()
            os.fsync(f.fileno())
        os.replace(temporary, path)
        sync_dir(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def load(path):
    with Path(path).open(encoding="utf-8") as f:
        value = json.load(f)
    if not isinstance(value, dict):
        raise ValueError("Expected an object: %s" % path)
    return value


@contextlib.contextmanager
def locked(path):
    mkdir(Path(path).parent)
    fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield fd
    finally:
        os.close(fd)


def endpoint(entity, thread, connection):
    return root() / "connections" / component(entity) / component(thread) / component(connection)


def connection(locator, scope=None):
    path = Path(locator).absolute()
    if path.name == "connection.json":
        path = path.parent
    record = load(path / "connection.json")
    if record.get("version") != VERSION:
        raise ValueError("Unsupported connection version")
    want = endpoint(record.get("entity_id"), record.get("thread_id"), record.get("connection_id"))
    if path != want or path.resolve() != want:
        raise ValueError("Connection is outside its registered location")
    if scope and (record["entity_id"], record["thread_id"]) != (scope["entity_id"], scope["thread_id"]):
        raise ValueError("Connection belongs to another company or conversation")
    return path, record


def registered():
    for p in sorted((root() / "connections").glob("*/*/*/connection.json")):
        try:
            value = connection(p)
            _diagnostics.pop(str(p), None)
            yield value
        except (OSError, ValueError) as e:
            diagnostic(p, e)


def scope_read(path):
    s = load(path)
    if s.get("version") != VERSION or not Path(s.get("state_root", "")).is_absolute():
        raise ValueError("No valid host channel scope")
    component(s.get("entity_id"))
    component(s.get("thread_id"))
    return s


def handoff(path):
    helper = shlex.join([sys.executable, "-B", str(Path(__file__).resolve())])
    return ("Use this dedicated RichOS connection: %s\n"
            "Helper: %s. Use send/read with this locator; connect creates a NEW pair. "
            "Keep the locator in your working notes. After compaction recover it from "
            "the handoff/transcript, never silently replace it. Send requires --id: "
            "record it in your command; after an uncertain response read --direction "
            "to-rich and retry that SAME ID/text/reply-to. A fresh ID is a new message. "
            "Read to-codex during work and while awaiting replies. Saved is not completed."
            % (path, helper))


def connect(text, provenance, locator=None, scope=None):
    if not text.strip() or not provenance.strip():
        raise ValueError("Supply the original task text and its provenance")
    if locator:
        _, target = connection(locator, scope)
    elif scope:
        target = scope
    else:
        try:
            target = load(root() / "coordinator.json")
        except FileNotFoundError:
            raise ValueError(SETUP_TEXT) from None
    ident = uuid.uuid4().hex
    p = endpoint(target["entity_id"], target["thread_id"], ident)
    mkdir(p)
    # Publish the endpoint last, so a scanner never observes half-created setup.
    for direction in ("to-rich", "to-codex"):
        with (p / (direction + ".jsonl")).open("xb") as f:
            os.chmod(f.name, 0o600)
            f.flush()
            os.fsync(f.fileno())
    record = dict(version=VERSION, entity_id=target["entity_id"], thread_id=target["thread_id"], connection_id=ident)
    direction = "to-codex" if scope else "to-rich"
    append(p / (direction + ".jsonl"), dict(version=VERSION, message_id="task-context", kind="task_context",
           text=text, provenance=provenance))
    save(p / "connection.json", record)
    return dict(locator=str(p), connection=record, instructions=handoff(p))


def records(path, offset=0):
    """Only newline-terminated frames. Invalid complete input holds its connection."""
    with Path(path).open("rb") as f:
        f.seek(offset)
        while True:
            start = f.tell()
            line = f.readline(FRAME_LIMIT + 1)
            if not line:
                return
            if len(line) > FRAME_LIMIT:
                raise ValueError("Channel record exceeds frame limit: %s" % path)
            if not line.endswith(b"\n"):
                return
            record = json.loads(line)
            validate(record, Path(path).parent)
            yield start, f.tell(), record


def validate(record, locator=None):
    if not isinstance(record, dict) or record.get("version") != VERSION:
        raise ValueError("Unsupported channel record")
    if set(record) - {"version", "message_id", "kind", "text", "provenance", "reply_to"}:
        raise ValueError("Unknown channel record fields")
    if record.get("kind", "message") not in ("message", "task_context", "review_result"):
        raise ValueError("Unknown channel record kind")
    component(record.get("message_id"))
    text = record.get("text")
    if not isinstance(text, str) or not text.strip() or len(text.encode()) > TEXT_LIMIT:
        raise ValueError("Message must contain 1..131072 bytes of text")
    if record.get("reply_to") is not None:
        component(record["reply_to"])
    # Account for the native observation wrapper BEFORE acknowledging a saved send.
    reference = dict(connection_id=Path(locator).name if locator else "x" * 128,
                     message_id=record["message_id"], payload_digest="0" * 64,
                     locator=str(locator) if locator else "x" * 4096)
    if record.get("reply_to") is not None:
        reference["reply_to"] = record["reply_to"]
    envelope = dict(bridge=reference, text=text, instruction=None)
    envelope["from"] = "review-watch"  # longest supported observation source
    if len(json.dumps(envelope, ensure_ascii=False, separators=(",", ":")).encode()) + 1 > FRAME_LIMIT:
        raise ValueError("Encoded message exceeds the native inbox frame limit")


def append(path, record):
    validate(record, Path(path).parent)
    encoded = (json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n").encode()
    if len(encoded) > FRAME_LIMIT:
        raise ValueError("Encoded channel record exceeds frame limit")
    with locked(path) as fd:
        with os.fdopen(os.dup(fd), "r+b", buffering=0) as f:
            # An unacknowledged incomplete tail is not a message. Reuse inbox recovery.
            length = os.fstat(fd).st_size
            start = max(0, length - FRAME_LIMIT)
            f.seek(start)
            tail = f.read(FRAME_LIMIT)
            if tail and not tail.endswith(b"\n"):
                end = start + tail.rfind(b"\n") + 1
                if end == start and start:
                    raise ValueError("Incomplete channel tail exceeds the frame limit")
                f.truncate(end)
                print("Discarded incomplete channel tail: %s" % path, file=sys.stderr)
            for _, through, old in records(path):
                if old["message_id"] == record["message_id"]:
                    if old != record:
                        raise ValueError("Message ID already exists with different contents")
                    os.fsync(fd)
                    return dict(saved=True, already_saved=True, message_id=old["message_id"], through=through)
            f.seek(0, os.SEEK_END)
            f.write(encoded)
            os.fsync(fd)
            return dict(saved=True, already_saved=False, message_id=record["message_id"], through=f.tell())


def send(locator, message_id, text, reply_to=None, scope=None):
    p, _ = connection(locator, scope)
    direction = "to-codex" if scope else "to-rich"
    rec = dict(version=VERSION, message_id=message_id, text=text)
    if reply_to is not None:
        rec["reply_to"] = reply_to
    return append(p / (direction + ".jsonl"), rec)


def read(locator, direction="to-codex", offset=0, scope=None):
    p, _ = connection(locator, scope)
    if direction not in ("to-rich", "to-codex"):
        raise ValueError("Unknown channel direction")
    out, through, size = [], offset, 2
    for _, end, record in records(p / (direction + ".jsonl"), offset):
        encoded_size = len(json.dumps(record, ensure_ascii=False).encode()) + (2 if out else 0)
        if out and size + encoded_size > FRAME_LIMIT:
            break
        out.append(record)
        size += encoded_size
        through = end
    return dict(records=out, through=through, locator=str(p), instructions=handoff(p))


def designate(scope, legacy_folders):
    """An explicit host-scoped choice. Existing endpoint records never change."""
    target = dict(version=VERSION, entity_id=component(scope["entity_id"]), thread_id=component(scope["thread_id"]))
    with locked(root() / "coordinator.lock"):
        # The old spool had no per-channel address. Its first explicit frozen
        # association owns it permanently. Later folders/defaults cannot move it.
        # An unavailable existing legacy directory also prevents reassignment.
        has_legacy = any((root() / "connections").glob("*/*/legacy-*/connection.json"))
        for folder in legacy_folders:
            folder = Path(folder).absolute()
            if not (folder / "to-rich.md").exists():
                continue
            ident = "legacy-" + hashlib.sha256(str(folder).encode()).hexdigest()[:24]
            found = list((root() / "connections").glob("*/*/%s/connection.json" % ident))
            if found:
                p, _ = connection(found[0])
            else:
                if legacy_associated(folder):
                    raise ValueError("Legacy association is preserved but its endpoint is unavailable; restore the original endpoint, never retarget it")
                p = endpoint(target["entity_id"], target["thread_id"], ident)
                mkdir(p)
                try:
                    old = load(Path(scope["state_root"]) / "codex-watch/watch/last-told.json")
                    previous = old.get(str(folder / "to-rich.md"), {})
                    raw = (folder / "to-rich.md").stat()
                    boundary = previous.get("offset", 0) if isinstance(previous, dict) and previous.get("inode") == raw.st_ino else 0
                    if type(boundary) is not int or not 0 <= boundary <= raw.st_size:
                        boundary = 0
                except (FileNotFoundError, ValueError):
                    boundary = 0
                save(p / "connection.json", dict(target, connection_id=ident, legacy_folder=str(folder),
                                                 legacy_boundary=boundary, imports_legacy_spool=not has_legacy))
                has_legacy = True
            instruction = ("\n## RICHOS CHANNEL MIGRATION %s\n"
                           "Create your own dedicated pair with the helper connect --locator %s "
                           "--text <original-task> --provenance <source-reference>. "
                           "Re-send EVERY unanswered request through the new pair with an explicit --id. "
                           "Read replies only on your own pair. Old Markdown appends are preserved but "
                           "are no longer delivered as complete requests.\n%s\n" % (ident, p, handoff(p)))
            reply = folder / "to-codex.md"
            with locked(reply) as fd:
                with os.fdopen(os.dup(fd), "r+b", buffering=0) as f:
                    if instruction.encode() not in f.read():
                        f.seek(0, os.SEEK_END)
                        f.write(instruction.encode())
                    os.fsync(fd)
            sync_dir(folder)
        save(root() / "coordinator.json", target)
    return dict(coordinator=target)


def legacy_associated(folder):
    """A damaged/unavailable endpoint never reactivates the old live transport.

    Endpoint publication and the synced old reply handoff are existing evidence
    of association. An unpublished setup directory alone is not an endpoint.
    """
    folder = Path(folder).absolute()
    ident = "legacy-" + hashlib.sha256(str(folder).encode()).hexdigest()[:24]
    if any((root() / "connections").glob("*/*/%s/connection.json" % ident)):
        return True
    try:
        marker = ("\n## RICHOS CHANNEL MIGRATION %s\n" % ident).encode()
        associated = marker in (folder / "to-codex.md").read_bytes()
        _diagnostics.pop(str(folder), None)
        return associated
    except FileNotFoundError:
        return False
    except OSError as error:
        diagnostic(folder, error)
        return True  # unknown association holds old transport; never fall back


def legacy_owner(folder):
    folder = str(Path(folder).absolute())
    return next(((p, r) for p, r in registered() if r.get("legacy_folder") == folder), None)


def identity(path):
    st = Path(path).stat()
    return "%s:%s" % (st.st_dev, st.st_ino)


def event(path, record, source, start, end):
    return dict(type="channel", locator=str(path), connection=record, source=str(source),
                source_identity=identity(source), start=start, through=end)


def position(source, saved):
    """A lost, stale or malformed advisory position means retained-source replay."""
    offset = saved.get("through") if isinstance(saved, dict) and saved.get("source_identity") == identity(source) else 0
    if type(offset) is not int or not 0 <= offset <= Path(source).stat().st_size:
        return 0
    if offset:
        with Path(source).open("rb") as boundary:
            boundary.seek(offset - 1)
            if boundary.read(1) != b"\n":
                return 0
    return offset


def scan(state):
    """One next record per connection. No position is written by the scanner."""
    state = Path(state)
    events, errors = [], []
    try:
        channels = list(registered())
    except (OSError, ValueError) as e:
        return [], [str(e)]
    for p, c in channels:
        try:
            source = p / "to-rich.jsonl"
            legacy = c.get("legacy_folder")
            if legacy:
                raw = Path(legacy) / "to-rich.md"
                boundary = c.get("legacy_boundary", 0)
                if raw.exists() and raw.stat().st_size > boundary:
                    e = event(p, c, raw, boundary, boundary)
                    e["record"] = dict(version=VERSION, message_id="legacy-tail", kind="legacy_notice",
                        text="Unframed legacy input remains preserved at %s after byte %s. Use the migration handoff and re-send unanswered requests on dedicated pairs; do not judge or execute fragments." % (raw, boundary))
                    events.append(e)
                if not c.get("imports_legacy_spool"):
                    continue
                source = state / "codex-watch" / "pending.jsonl"
                if not source.exists():
                    continue
            pos = state / "codex-watch" / "accepted" / (c["connection_id"] + ".json")
            try:
                saved = load(pos)
                offset = position(source, saved)
            except (FileNotFoundError, ValueError):
                # Positions are advisory. Corruption replays retained source records.
                offset = 0
            if legacy:
                with source.open("rb") as f:
                    f.seek(offset)
                    line = f.readline(FRAME_LIMIT + 1)
                    if not line or not line.endswith(b"\n"):
                        continue
                    text = json.loads(line)
                    if not isinstance(text, str):
                        raise ValueError("Invalid preserved legacy spool record")
                    e = event(p, c, source, offset, f.tell())
                    e["record"] = dict(version=VERSION, message_id="legacy-" + hashlib.sha256(
                        (identity(source) + ":" + str(offset)).encode()).hexdigest(), text=text, kind="legacy_import")
                    events.append(e)
            else:
                for start, end, row in records(source, offset):
                    e = event(p, c, source, start, end)
                    e["record"] = row
                    events.append(e)
                    break
        except (OSError, ValueError, KeyError) as e:
            errors.append("%s: %s" % (p, e))
    return events, errors


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("action", choices=("connect", "send", "read", "designate"))
    ap.add_argument("--locator")
    ap.add_argument("--id", dest="message_id")
    ap.add_argument("--text")
    ap.add_argument("--provenance")
    ap.add_argument("--reply-to")
    ap.add_argument("--direction", default="to-codex", choices=("to-rich", "to-codex"))
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--scope")
    ap.add_argument("--request", action="store_true")
    ap.add_argument("--legacy", action="append", default=[])
    a = ap.parse_args(argv)
    try:
        if a.request:
            fields = json.load(sys.stdin)
            allowed = {"locator", "message_id", "text", "provenance", "reply_to", "direction", "offset", "legacy"}
            if not isinstance(fields, dict) or set(fields) - allowed:
                raise ValueError("Unknown channel request fields")
            for key, value in fields.items():
                if key == "offset":
                    if type(value) is not int or value < 0:
                        raise ValueError("offset must be a nonnegative integer")
                elif key == "legacy":
                    if not isinstance(value, list) or not all(isinstance(x, str) for x in value):
                        raise ValueError("legacy paths must be a list of strings")
                elif value is not None and not isinstance(value, str):
                    raise ValueError(key + " must be text")
                setattr(a, key, value)
        s = scope_read(a.scope) if a.scope else None
        if a.action == "connect":
            result = connect(a.text or "", a.provenance or "", a.locator, s)
        elif a.action == "designate":
            if not s:
                raise ValueError("Coordinator designation requires a host scope")
            result = designate(s, a.legacy)
        elif a.action == "send":
            if not a.message_id or not a.locator:
                raise ValueError("send requires --locator and --id; reuse the ID for an uncertain retry")
            result = send(a.locator, a.message_id, a.text or "", a.reply_to, s)
        else:
            if not a.locator or a.offset < 0:
                raise ValueError("read requires --locator and a nonnegative offset")
            result = read(a.locator, a.direction, a.offset, s)
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except (OSError, ValueError, KeyError) as e:
        print(json.dumps(dict(error=str(e))), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
