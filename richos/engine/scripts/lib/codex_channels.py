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
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid

import codex_quota

_diagnostics = {}  # changed failures only, no persistent routing state


def diagnostic(path, error):
    message = str(error)
    if _diagnostics.get(str(path)) != message:
        print("Channel held at %s: %s" % (path, message), file=sys.stderr)
        _diagnostics[str(path)] = message


VERSION = 1
TEXT_LIMIT = 128 * 1024
FRAME_LIMIT = 256 * 1024
AUTOMATIC = "_automatic"
TERMINAL = "_terminal"
DISPATCH_LOCATOR = "RICHOS_CODEX_DISPATCH_LOCATOR"
DISPATCH_ATTEMPT = "RICHOS_CODEX_DISPATCH_ATTEMPT"
QUOTA_POLL_SECONDS = 5


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


def open_regular(path, flags):
    fd = os.open(path, flags | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError("Channel path is not an ordinary file: %s" % path)
        return fd
    except BaseException:
        os.close(fd)
        raise


def load(path):
    with os.fdopen(open_regular(path, os.O_RDONLY), "rb") as f:
        data = f.read(FRAME_LIMIT + 1)
    if len(data) > FRAME_LIMIT:
        raise ValueError("Channel metadata exceeds the frame limit: %s" % path)
    value = json.loads(data)
    if not isinstance(value, dict):
        raise ValueError("Expected an object: %s" % path)
    return value


@contextlib.contextmanager
def locked(path, create=True, wait=True):
    mkdir(Path(path).parent)
    fd = open_regular(path, os.O_RDWR | os.O_APPEND | (os.O_CREAT if create else 0))
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
        yield fd
    finally:
        os.close(fd)


def write_all(fd, data):
    remaining = memoryview(data)
    while remaining:
        written = os.write(fd, remaining)
        if written <= 0:
            raise OSError("Channel write made no progress")
        remaining = remaining[written:]


def contains(file, needle):
    tail = b""
    while chunk := file.read(64 * 1024):
        chunk = tail + chunk
        if needle in chunk:
            return True
        tail = chunk[-(len(needle) - 1):] if len(needle) > 1 else b""
    return False


def endpoint(entity, thread, connection):
    return root() / "connections" / component(entity) / component(thread) / component(connection)


def connection(locator, scope=None):
    path = Path(locator).absolute()
    if path.name == "connection.json":
        path = path.parent
    record = load(path / "connection.json")
    if type(record.get("version")) is not int or record["version"] != VERSION:
        raise ValueError("Unsupported connection version")
    if record.get("legacy_folder") is not None:
        if not isinstance(record["legacy_folder"], str) or not Path(record["legacy_folder"]).is_absolute():
            raise ValueError("Invalid legacy channel folder")
        if type(record.get("legacy_boundary")) is not int or record["legacy_boundary"] < 0:
            raise ValueError("Invalid legacy preservation boundary")
        if type(record.get("imports_legacy_spool")) is not bool:
            raise ValueError("Invalid frozen legacy spool owner")
    want = endpoint(record.get("entity_id"), record.get("thread_id"), record.get("connection_id"))
    if path != want or path.resolve() != want:
        raise ValueError("Connection is outside its registered location")
    if record["entity_id"] == AUTOMATIC:
        if record["thread_id"] != "codex" or record.get("legacy_folder"):
            raise ValueError("Invalid automatic channel endpoint")
        if "binding" in record:
            target = record["binding"]
            if not isinstance(target, dict) or component(target.get("entity_id")) in (AUTOMATIC, TERMINAL):
                raise ValueError("Invalid automatic channel binding")
            record = dict(record, entity_id=target["entity_id"], thread_id=component(target.get("thread_id")))
    if scope and (record["entity_id"], record["thread_id"]) != (scope["entity_id"], scope["thread_id"]):
        raise ValueError("Connection belongs to another company or conversation")
    return path, record


def registered(entity="*", thread="*", ident="*"):
    for p in sorted((root() / "connections").glob(f"{entity}/{thread}/{ident}/connection.json")):
        try:
            value = connection(p)
            _diagnostics.pop(str(p), None)
            yield value
        except (OSError, ValueError) as e:
            diagnostic(p, e)


def scope_read(path):
    s = load(path)
    if type(s.get("version")) is not int or s["version"] != VERSION or not Path(s.get("state_root", "")).is_absolute():
        raise ValueError("No valid host channel scope")
    component(s.get("entity_id"))
    component(s.get("thread_id"))
    return s


def helper_command():
    return ["env", "CODEX_COORDINATION_ROOT=" + str(root()), sys.executable, "-B", str(Path(__file__).resolve())]


def install_codex_instructions(home):
    """Publish the RichOS guide and append its reference to Codex's global instructions."""
    guide = root() / "codex-instructions.md"
    template = Path(__file__).resolve().parents[2] / "app/agents/codex.md"
    text = template.read_text().replace("{helper}", shlex.join(helper_command()))
    encoded = text.encode()
    mkdir(guide.parent)
    try:
        with os.fdopen(open_regular(guide, os.O_RDONLY), "rb") as installed:
            current = installed.read(len(encoded) + 1)
    except FileNotFoundError:
        current = None
    if current != encoded:
        with tempfile.NamedTemporaryFile(mode="w", dir=guide.parent, delete=False, encoding="utf-8") as f:
            temporary = Path(f.name)
            try:
                f.write(text)
                f.flush()
                os.fsync(f.fileno())
                os.replace(temporary, guide)
                sync_dir(guide.parent)
            finally:
                temporary.unlink(missing_ok=True)
    path = Path(home).expanduser().resolve() / "AGENTS.md"
    marker = b"<!-- RichOS Codex coordination -->"
    addition = ("\n\n" + marker.decode() + "\n"
                "When coordinating with RichOS, read the local RichOS Codex instructions at `" + str(guide) +
                "`. Use the same dedicated connection for the entire task.\n").encode()
    with locked(path, wait=False) as fd, os.fdopen(os.dup(fd), "rb") as existing:
        if not contains(existing, marker):
            write_all(fd, addition)
            os.fsync(fd)
            sync_dir(path.parent)


def handoff(path):
    helper = shlex.join(helper_command())
    return ("Use this dedicated RichOS connection: %s\n"
            "Helper: %s. Use send/read with this locator; connect creates a NEW pair. "
            "Keep the locator in your working notes. After compaction recover it from "
            "the handoff/transcript, never silently replace it. Send requires --id: "
            "record it in your command; after an uncertain response read --direction "
            "to-rich and retry that SAME ID/text/reply-to. A fresh ID is a new message. "
            "Read to-codex during work and while awaiting replies. Saved is not completed. "
            "Start a plain review handover's first line with uppercase READY TO LAND: "
            "followed by your actual codex/ branch. Existing ## headings also work."
            % (path, helper))


def automatic_target():
    try:
        target = load(root() / "automatic.json")
    except FileNotFoundError:
        return None
    if type(target.get("version")) is not int or target["version"] != VERSION:
        raise ValueError("Invalid automatic Codex destination version")
    if component(target.get("entity_id")) in (AUTOMATIC, TERMINAL):
        raise ValueError("Invalid automatic Codex company")
    component(target.get("thread_id"))
    return target


def bind_automatic(target):
    # Only the app receiver calls this. Its receiver lock fences other hosts.
    for path, record in registered(AUTOMATIC, "codex"):
        if record["entity_id"] == AUTOMATIC:
            save(path / "connection.json", dict(record, binding={
                "entity_id": target["entity_id"], "thread_id": target["thread_id"]}))


def automatic_waiting():
    return any(record["entity_id"] == AUTOMATIC for _, record in registered(AUTOMATIC, "codex"))


def terminal_scope(project, session=""):
    if project:
        path = str(Path(project).resolve())
        return "project-" + hashlib.sha256(path.encode()).hexdigest()[:32]
    return session


def terminal_scopes(project, session):
    # A terminal can also have pairs from before it had a project identity.
    if session:
        if project:
            yield terminal_scope(project)
        yield session


def terminal_project(directory):
    directory = directory or os.environ.get("CLAUDE_PROJECT_DIR") or os.environ.get("RICHOS_ENTITY_ROOT")
    if not directory or not Path(directory).is_dir():
        return None
    resolver = str(Path(__file__).with_name("resolve-roots.sh"))
    result = subprocess.run(
        ["bash", "-c", '. "$1" && resolve_entity_root "" && '
         '[ -f "$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config" ] && '
         'printf "%s" "$RICHOS_ENTITY_ROOT_RESOLVED"', "terminal-project", resolver],
        cwd=directory, env=dict(os.environ, CLAUDE_PROJECT_DIR=directory),
        capture_output=True, text=True,
    )
    return result.stdout if result.returncode == 0 else None


def terminal_connections(session):
    if session:
        yield from registered(TERMINAL, component(session))


def connect(text, provenance, locator=None, scope=None, terminal_session=None):
    if not text.strip() or not provenance.strip():
        raise ValueError("Supply the original task text and its provenance")
    if locator:
        if terminal_session is not None:
            raise ValueError("A terminal handoff uses its own session as destination")
        _, target = connection(locator, scope)
    elif scope:
        target = scope
    elif terminal_session is not None:
        target = dict(entity_id=TERMINAL, thread_id=component(terminal_session))
    else:
        target = automatic_target() or dict(entity_id=AUTOMATIC, thread_id="codex")
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
    direction = "to-codex" if scope or terminal_session is not None else "to-rich"
    append(p / (direction + ".jsonl"), dict(version=VERSION, message_id="task-context", kind="task_context",
           text=text, provenance=provenance))
    save(p / "connection.json", record)
    result = dict(locator=str(p), connection=record, instructions=handoff(p))
    if direction == "to-codex":
        result["dispatch_command"] = shlex.join(helper_command() + ["dispatch", "--locator", str(p)])
    return result


def rich_task(locator):
    path, _ = connection(locator)
    with contextlib.closing(records(path / "to-codex.jsonl")) as journal:
        _, _, context = next(journal, (None, None, {}))
    if context.get("kind") != "task_context":
        raise ValueError("This connection has no Rich task handoff")
    return path, context


def codex_binary():
    search_path = os.pathsep.join(filter(None, (os.environ.get("PATH"), "/opt/homebrew/bin",
        "/usr/local/bin", str(Path.home() / ".local/bin"))))
    binary = shutil.which("codex", path=search_path) or shutil.which(
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex")
    if not binary:
        raise ValueError("Codex CLI is unavailable")
    return binary


def quota_status():
    try:
        with codex_quota.client(codex_binary()) as rpc:
            return codex_quota.read(rpc).status()
    except (OSError, ValueError):
        return dict(status="unavailable", limits=[], available_resets=None, ordinary_usage_allowed=None)


def check_codex_quota(binary, expected_account=None, *, rpc=None, reading=None, active=None):
    """Read the active account and serialize redemption across RichOS jobs."""
    outcome = "unavailable"
    try:
        with (codex_quota.client(binary) if rpc is None else contextlib.nullcontext(rpc)) as rpc:
            if reading is None:
                reading = codex_quota.read(rpc)
            if expected_account is not None and reading.account != expected_account:
                return None, "accountChanged"
            if not reading.exhausted_windows() or not reading.resets:
                return reading, "none"
            if active is not None and not active():
                return reading, "none"
            if not reading.account:
                return reading, "accountUnavailable"
            config = rpc.request("config/read", {"cwd": os.getcwd(), "includeLayers": False}).get("config")
            if not isinstance(config, dict):
                return reading, "unavailable"
            if config.get("model_provider") not in (None, "openai") or config.get("openai_base_url"):
                return reading, "otherProvider"
            if active is not None and not active():
                return reading, "none"

            auth = rpc.request("account/read", {"refreshToken": False})
            if codex_quota.obj(auth.get("account")).get("type") != "chatgpt":
                return reading, "otherProvider"
            user = auth.get("userId") or ""
            # A live monitor can retry if another job owns the lock. Preflight
            # must wait for that job's result before it starts Codex.
            with locked(root() / "quota" / "reset.lock", wait=active is None):
                fresh = codex_quota.read(rpc)
                if fresh.account != reading.account:
                    return None, "accountChanged"
                reading = fresh
                windows = reading.exhausted_windows()
                if not windows or not reading.resets:
                    return reading, "none"
                account = hashlib.sha256(json.dumps([reading.account, user]).encode()).hexdigest()
                marker = root() / "quota" / (account + ".json")
                try:
                    attempt = load(marker)
                except FileNotFoundError:
                    attempt = {}
                prior = attempt.get("outcome")
                if prior in ("reset", "alreadyRedeemed") and set(windows).intersection(attempt["windows"]):
                    return reading, "previousAttempt"
                if active is not None and not active():
                    return reading, "none"
                # Uncertain requests reuse their UUID even if the reading has
                # changed. Known refusals can be tried again with fresh inventory.
                if not attempt or prior is not None:
                    attempt = dict(windows=windows, key=str(uuid.uuid4()))
                    if (credit_id := reading.next_reset_id()) is not None:
                        attempt["credit_id"] = credit_id
                    save(marker, attempt)
                params = {"idempotencyKey": attempt["key"]}
                if "credit_id" in attempt:
                    params["creditId"] = attempt["credit_id"]
                outcome = "unconfirmed"
                result = rpc.request("account/rateLimitResetCredit/consume", params)
                if result.get("outcome") not in codex_quota.RESET_OUTCOMES:
                    return reading, outcome
                outcome = result["outcome"]
                save(marker, dict(attempt, outcome=outcome))
                refreshed = codex_quota.read(rpc)
                return (refreshed if refreshed.account == reading.account else None), outcome
    except BlockingIOError:
        return reading, "resetInProgress"
    except (OSError, ValueError, KeyError, TypeError):
        return (None if outcome in codex_quota.RESET_OUTCOMES else reading), outcome


def dispatch_notice(path, text):
    print(text, file=sys.stderr, flush=True)
    try:
        send(path, "dispatch-" + uuid.uuid4().hex, text, reply_to="task-context")
    except (OSError, ValueError) as error:
        diagnostic(path, error)


def quota_notice(path, reading, outcome):
    if outcome in ("reset", "alreadyRedeemed"):
        dispatch_notice(path, "Codex used a banked reset. " +
                        (reading.summary() if reading else "Updated usage is unavailable."))
    elif reading and reading.exhausted_windows():
        status = {
            "none": ("Banked reset availability is unknown." if reading.resets is None else
                     "No banked reset was reported as available."),
            "noCredit": "Codex reported no banked reset available.",
            "nothingToReset": "Codex reported no eligible window for a banked reset.",
            "accountUnavailable": "Codex did not identify the account for reset redemption.",
            "otherProvider": "This CLI run has a provider override.",
            "previousAttempt": "RichOS already attempted a reset for this window.",
            "unconfirmed": "The banked reset result is unconfirmed.",
            "unavailable": "Banked reset redemption is unavailable.",
        }.get(outcome, "Banked reset status is unknown.")
        dispatch_notice(path, f"Codex included quota is at its limit. {status} "
                        f"Codex can draw from existing credits for eligible work. {reading.summary()}")


class DispatchQuotaWatch:
    def __init__(self, path, reading, outcome):
        self.path = path
        self.account = reading.account if reading else None
        self.wake = threading.Event()
        self.source = self.outcome = None
        self.notice = (outcome, reading.exhausted_windows()) if reading else None

    @property
    def redeemed(self):
        return self.outcome in ("reset", "alreadyRedeemed")

    @contextlib.contextmanager
    def during(self, binary, child):
        self.wake.clear()
        stopped = threading.Event()

        def active():
            return not stopped.is_set() and child.poll() is None

        worker = threading.Thread(target=self.watch, args=(binary, active))
        worker.start()
        try:
            yield
        finally:
            stopped.set()
            self.wake.set()
            worker.join()

    def watch(self, binary, active):
        # Keep one App Server connection for periodic reads. Short turns never
        # start it, and the exec stdout reader stays responsive during RPCs.
        with contextlib.ExitStack() as connection:
            rpc = None
            while active():
                source = "event" if self.wake.wait(QUOTA_POLL_SECONDS) else "poll"
                self.wake.clear()
                if not active():
                    break
                try:
                    if rpc is None:
                        rpc = connection.enter_context(codex_quota.client(binary))
                    reading = codex_quota.read(rpc)
                    if self.account is None:
                        self.account = reading.account
                    reading, outcome = check_codex_quota(binary, self.account, rpc=rpc, reading=reading, active=active)
                except (OSError, ValueError):
                    reading, outcome = None, "unavailable"
                if (reading is None and outcome == "unavailable") or outcome == "unconfirmed":
                    connection.close()
                    rpc = None
                if not self.redeemed or outcome in ("reset", "alreadyRedeemed"):
                    self.source, self.outcome = source, outcome
                windows = reading.exhausted_windows() if reading else []
                notice = (outcome, windows)
                if ((windows and active() and outcome != "resetInProgress") or
                        outcome in ("reset", "alreadyRedeemed")) and notice != self.notice:
                    self.notice = notice
                    quota_notice(self.path, reading, outcome)
                if outcome == "accountChanged":
                    break


def run_codex(binary, path, marker, state, prompt, watch):
    command = [binary, "exec", "--json", "--skip-git-repo-check",
               "--sandbox", "danger-full-access"]
    if session := state.get("session_id"):
        command += ["resume", component(session)]
    # A file avoids a pipe deadlock if a large prompt and early JSON events
    # fill both pipes. The CLI reads from stdin and retains its own transcript.
    with tempfile.TemporaryFile(mode="w+b") as source:
        source.write(prompt.encode())
        source.seek(0)
        env = dict(os.environ, **{DISPATCH_LOCATOR: str(path), DISPATCH_ATTEMPT: state["attempt_id"]})
        quota_failed = False
        with (subprocess.Popen([*command, "-"], stdin=source, stdout=subprocess.PIPE, text=True, env=env) as child,
              watch.during(binary, child)):
            for line in child.stdout:
                try:
                    event = json.loads(line)
                except ValueError:
                    print(line, end="", flush=True)
                    continue
                event = codex_quota.obj(event)
                quota_failed |= codex_quota.usage_failed(event)
                if codex_quota.usage_exhausted(event):
                    watch.wake.set()
                if event.get("type") == "thread.started":
                    session_id = component(event.get("thread_id"))
                    if session and session_id != session:
                        child.terminate()
                        raise ValueError("Codex resumed a different session")
                    if session_id != state.get("session_id"):
                        state["session_id"] = session_id
                        save(marker, state)
                print(line, end="", flush=True)
            return child.wait(), quota_failed


def run_dispatch_attempt(binary, path, marker, state, prompt, recovering, watch):
    state["attempt_id"] = uuid.uuid4().hex
    state.pop("exit_code", None)
    save(marker, state)
    if recovering and state.get("session_id"):
        prompt = ("Continue the pending RichOS task in this chat. "
                  "Check prior progress before repeating any effects. Pending request follows.\n\n" + prompt)
    return run_codex(binary, path, marker, state, prompt, watch)


def dispatch(locator, resume=False):
    """Start one Codex chat or deliver queued Rich messages to that exact chat."""
    path, context = rich_task(locator)
    marker = path / "dispatch.json"
    receipt = path / "dispatch-read.json"
    journal = path / "to-codex.jsonl"
    with locked(path / "dispatch.lock"):
        if resume:
            state = load(marker)
            if not state.get("session_id") and not state.get("exit_code"):
                raise ValueError("The initial Codex launch did not report a session ID")
            offset = position(journal, state)
            if state.get("exit_code") == 0 and offset and receipt.exists():
                acknowledged = load(receipt)
                if acknowledged.get("attempt_id") == state.get("attempt_id"):
                    offset = max(offset, position(journal, acknowledged))
        else:
            if marker.exists():
                raise ValueError("This handoff was already dispatched; resume its existing Codex chat")
            state = {"source_identity": identity(journal), "through": 0}
            offset = 0

        pending = []
        through = offset
        size = 0
        for _, end, record in records(journal, offset):
            if record.get("kind") == "task_context":
                text = (context["text"] + "\n\nOriginal source: " + context["provenance"] +
                        "\n\n" + handoff(path) + "\nSend updates and results to Rich via this connection.")
            else:
                text = "Message %s from Rich:\n%s" % (record["message_id"], record["text"])
            if pending and size + len(text.encode()) > TEXT_LIMIT:
                break
            pending.append(text)
            size += len(text.encode())
            through = end
            if not resume:
                break
        if not pending:
            return 0
        state["through"] = offset
        recovering = resume and state.get("exit_code") != 0
        if not resume:
            save(marker, state)
        prompt = (handoff(path) + "\n\n" if offset else "") + "\n\n".join(pending)

        started = time.monotonic()
        reading, outcome = None, "unavailable"
        preflight_ms = None
        quota_failed = False
        retry_outcome = None
        watch = None
        try:
            binary = codex_binary()
            reading, outcome = check_codex_quota(binary)
            preflight_ms = round((time.monotonic() - started) * 1000)
            quota_notice(path, reading, outcome)
            watch = DispatchQuotaWatch(path, reading, outcome)
            code, quota_failed = run_dispatch_attempt(binary, path, marker, state, prompt, recovering, watch)
            if (code and quota_failed and watch.account and
                    (outcome in ("none", "unavailable", "accountUnavailable") or watch.redeemed)):
                updated, retry_outcome = check_codex_quota(binary, expected_account=watch.account)
                if updated is not None:
                    reading = updated
                reset_for_retry = retry_outcome in ("reset", "alreadyRedeemed")
                if reset_for_retry:
                    quota_notice(path, updated, retry_outcome)
                if reset_for_retry or (watch.redeemed and retry_outcome == "none" and updated and updated.allowed):
                    code, quota_failed = run_dispatch_attempt(binary, path, marker, state, prompt, True, watch)
        except (OSError, ValueError) as error:
            diagnostic(path, error)
            code = 1

        if code == 0 and not state.get("session_id"):
            code = 1
        if code == 0:
            state["through"] = through
        state["exit_code"] = code
        state["quota_diagnostics"] = dict(
            at_ms=time.time_ns() // 1_000_000, elapsed_ms=round((time.monotonic() - started) * 1000),
            preflight_ms=preflight_ms, reading="available" if reading else "unavailable", reset=outcome,
            in_flight_source=watch.source if watch else None, in_flight_reset=watch.outcome if watch else None,
            retry_reset=retry_outcome, quota_failed=quota_failed, exit_code=code)
        save(marker, state)
        try:
            save(root() / "quota" / "dispatch-health.json", state["quota_diagnostics"])
        except (OSError, ValueError):
            pass
        if code:
            reason = "at a quota limit" if quota_failed else "before completing the handoff"
            summary = reading.summary() if reading else "Codex usage is unavailable"
            resume_command = shlex.join(helper_command() + ["resume", "--locator", str(path)])
            dispatch_notice(path, f"Codex stopped {reason} (exit {code}). {summary}. "
                            f"The work is still pending. To resume, run `{resume_command}` from the work directory.")
        return code


def records(path, offset=0, writer_locked=False):
    """Only complete immutable frames. Release the read lock before yielding.

    Bounded pread avoids retaining an incomplete read-ahead fragment across a
    writer's tail repair. A busy writer holds only its connection, never a scan.
    """
    fd = open_regular(path, os.O_RDONLY)
    try:
        while True:
            if not writer_locked:
                try:
                    fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
                except BlockingIOError:
                    raise ValueError("Channel writer is busy; retry on the next scan") from None
            try:
                encoded = bytearray()
                while len(encoded) <= FRAME_LIMIT:
                    part = os.pread(fd, min(4096, FRAME_LIMIT + 1 - len(encoded)), offset + len(encoded))
                    if not part:
                        return
                    newline = part.find(b"\n")
                    encoded.extend(part if newline < 0 else part[:newline + 1])
                    if newline >= 0:
                        break
                if len(encoded) > FRAME_LIMIT:
                    raise ValueError("Channel record exceeds frame limit: %s" % path)
                if not encoded.endswith(b"\n"):
                    return
                record = json.loads(encoded)
                validate(record, Path(path).parent)
                end = offset + len(encoded)
            finally:
                if not writer_locked:
                    fcntl.flock(fd, fcntl.LOCK_UN)
            yield offset, end, record
            offset = end
    finally:
        os.close(fd)


def validate(record, locator=None):
    if not isinstance(record, dict) or type(record.get("version")) is not int or record["version"] != VERSION:
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
    with locked(path, create=False) as fd:
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
            for _, through, old in records(path, writer_locked=True):
                if old["message_id"] == record["message_id"]:
                    if old != record:
                        raise ValueError("Message ID already exists with different contents")
                    os.fsync(fd)
                    return dict(saved=True, already_saved=True, message_id=old["message_id"], through=through)
            f.seek(0, os.SEEK_END)
            write_all(fd, encoded)
            os.fsync(fd)
            return dict(saved=True, already_saved=False, message_id=record["message_id"], through=f.tell())


def send(locator, message_id, text, reply_to=None, scope=None):
    p, _ = connection(locator, scope)
    direction = "to-codex" if scope else "to-rich"
    rec = dict(version=VERSION, message_id=message_id, text=text)
    if reply_to is not None:
        rec["reply_to"] = reply_to
    result = append(p / (direction + ".jsonl"), rec)
    if scope and (p / "dispatch.json").is_file():
        result["resume_command"] = shlex.join(helper_command() + ["resume", "--locator", str(p)])
    return result


def read(locator, direction="to-codex", offset=0, scope=None):
    p, _ = connection(locator, scope)
    if direction not in ("to-rich", "to-codex"):
        raise ValueError("Unknown channel direction")
    source = p / (direction + ".jsonl")
    out, through, size = [], offset, 2
    for _, end, record in records(source, offset):
        encoded_size = len(json.dumps(record, ensure_ascii=False).encode()) + (2 if out else 0)
        if out and size + encoded_size > FRAME_LIMIT:
            break
        out.append(record)
        size += encoded_size
        through = end
    return dict(records=out, through=through, locator=str(p), instructions=handoff(p))


def acknowledge_dispatch_read(path, offset, through):
    # Only the running attempt can acknowledge reads. A failed attempt's
    # receipt cannot advance the next dispatch, even after a successful retry.
    if os.environ.get(DISPATCH_LOCATOR) != path or through <= offset:
        return
    p = Path(path)
    source = p / "to-codex.jsonl"
    with locked(p / "dispatch-read.lock"):
        state = load(p / "dispatch.json")
        attempt = state.get("attempt_id")
        if state.get("exit_code") is not None or os.environ.get(DISPATCH_ATTEMPT) != attempt:
            return
        receipt = p / "dispatch-read.json"
        previous = load(receipt) if receipt.exists() else {}
        seen = position(source, previous) if previous.get("attempt_id") == attempt else 0
        delivered = position(source, state)
        if offset <= max(seen, delivered) and through > seen:
            save(receipt, dict(source_identity=identity(source), through=through, attempt_id=attempt))


def associate_legacy(scope, legacy_folders, *, designate=False):
    """Freeze each old source once. Existing endpoint records never change."""
    target = dict(version=VERSION, entity_id=component(scope["entity_id"]), thread_id=component(scope["thread_id"]))
    with locked(root() / "coordinator.lock"):
        # The old spool had no per-channel address. Its first explicit frozen
        # association owns it permanently. Later folders/defaults cannot move it.
        # An unavailable existing legacy directory also prevents reassignment.
        has_legacy = any((root() / "connections").glob("*/*/legacy-*/connection.json"))
        for folder in legacy_folders:
            folder = Path(folder).absolute()
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
                outbox = folder / "to-codex.md"
                has_outbox = not designate and outbox.is_file() and outbox.stat().st_size > 0
                save(p / "connection.json", dict(target, connection_id=ident, legacy_folder=str(folder),
                                                 legacy_boundary=boundary, imports_legacy_spool=not has_legacy,
                                                 preserves_legacy_outbox=has_outbox, legacy_live=not designate))
                has_legacy = True
            input_note = ("Existing Markdown replies continue to arrive. Use dedicated pairs for new work."
                          if not designate else "Old Markdown appends are preserved but are no longer delivered as complete requests.")
            instruction = ("\n## RICHOS CHANNEL MIGRATION %s\n"
                           "Create your own dedicated pair with the helper connect --locator %s "
                           "--text <original-task> --provenance <source-reference>. "
                           "Re-send EVERY unanswered request through the new pair with an explicit --id. "
                           "Read replies on your own pair. %s\n%s\n" % (ident, p, input_note, handoff(p)))
            reply = folder / "to-codex.md"
            with locked(reply) as fd:
                with os.fdopen(os.dup(fd), "r+b", buffering=0) as f:
                    if not contains(f, instruction.encode()):
                        f.seek(0, os.SEEK_END)
                        write_all(fd, instruction.encode())
                    os.fsync(fd)
            sync_dir(folder)
        if designate:
            save(root() / "coordinator.json", target)
    return dict(coordinator=target)


def designate(scope, legacy_folders):
    return associate_legacy(scope, (p for p in legacy_folders if (Path(p) / "to-rich.md").exists()),
                            designate=True)


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
        with os.fdopen(open_regular(folder / "to-codex.md", os.O_RDONLY), "rb") as file:
            associated = contains(file, marker)
        _diagnostics.pop(str(folder), None)
        return associated
    except FileNotFoundError:
        return False
    except (OSError, ValueError) as error:
        diagnostic(folder, error)
        return True  # unknown association holds old transport; never fall back


def legacy_live(folder):
    folder = str(Path(folder).absolute())
    ident = "legacy-" + hashlib.sha256(folder.encode()).hexdigest()[:24]
    for _, record in registered("*", "*", ident):
        if record.get("legacy_folder") == folder:
            return record.get("legacy_live") is True
    return False


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
        with os.fdopen(open_regular(source, os.O_RDONLY), "rb") as boundary:
            boundary.seek(offset - 1)
            if boundary.read(1) != b"\n":
                return 0
    return offset


LIVE_LEGACY_SPOOL = Path("codex-watch/live.jsonl")


def scan(state):
    """One next record per connection. No position is written by the scanner."""
    state = Path(state)
    events, errors = [], []
    try:
        channels = list(registered())
    except (OSError, ValueError) as e:
        return [], [str(e)]
    for p, c in channels:
        if c["entity_id"] in (TERMINAL, AUTOMATIC):
            continue
        try:
            source = p / "to-rich.jsonl"
            legacy = c.get("legacy_folder")
            if legacy:
                if not c.get("imports_legacy_spool"):
                    continue
                # The pre-cutover spool and archive notices stay on disk outside delivery.
                source = state / LIVE_LEGACY_SPOOL
                if not source.exists():
                    continue
            suffix = ".live.json" if legacy else ".json"
            pos = state / "codex-watch" / "accepted" / (c["connection_id"] + suffix)
            try:
                saved = load(pos)
                offset = position(source, saved)
            except (FileNotFoundError, ValueError):
                # Positions are advisory. Corruption replays retained source records.
                offset = 0
            if legacy:
                with os.fdopen(open_regular(source, os.O_RDONLY), "rb") as f:
                    f.seek(offset)
                    line = f.readline(FRAME_LIMIT + 1)
                    if not line or not line.endswith(b"\n"):
                        continue
                    text = json.loads(line)
                    if not isinstance(text, str):
                        raise ValueError("Invalid preserved legacy spool record")
                    e = event(p, c, source, offset, f.tell())
                    e["record"] = dict(version=VERSION, message_id="legacy-live-" + hashlib.sha256(
                        (e["source_identity"] + ":" + str(offset)).encode()).hexdigest(),
                        text=text, kind="legacy_live")
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
    ap.add_argument("action", choices=("connect", "send", "read", "dispatch", "resume", "quota", "designate"))
    ap.add_argument("--locator")
    ap.add_argument("--id", dest="message_id")
    ap.add_argument("--text")
    ap.add_argument("--provenance")
    ap.add_argument("--reply-to")
    ap.add_argument("--direction", default="to-codex", choices=("to-rich", "to-codex"))
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--scope")
    ap.add_argument("--terminal", action="store_true")
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
        terminal_session = None
        if a.terminal:
            if s or a.action not in ("connect", "send", "read"):
                raise ValueError("--terminal applies to terminal connect, send or read")
            from codex_watch import session
            sid, directory = session(str(Path(__file__).resolve().parents[2]), with_project=True)
            if not sid:
                raise ValueError("No owning Rich terminal session")
            terminal_session = terminal_scope(terminal_project(directory), sid)
            if a.action != "connect":
                s = dict(entity_id=TERMINAL, thread_id=terminal_session)
                if a.locator:
                    _, target = connection(a.locator)
                    if target["entity_id"] == TERMINAL and target["thread_id"] == sid:
                        s["thread_id"] = sid
        if a.action == "connect":
            result = connect(a.text or "", a.provenance or "", a.locator, s, terminal_session)
        elif a.action in ("dispatch", "resume"):
            if not a.locator or s or a.terminal:
                raise ValueError(a.action + " requires an existing Rich handoff locator")
            return dispatch(a.locator, resume=a.action == "resume")
        elif a.action == "quota":
            if a.locator or a.terminal:
                raise ValueError("quota reads the active Codex CLI account")
            result = quota_status()
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
        print(json.dumps(result, ensure_ascii=False), flush=True)
        if a.action == "read" and not s and a.direction == "to-codex":
            acknowledge_dispatch_read(result["locator"], a.offset, result["through"])
        return 0
    except (OSError, ValueError, KeyError) as e:
        print(json.dumps(dict(error=str(e))), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
