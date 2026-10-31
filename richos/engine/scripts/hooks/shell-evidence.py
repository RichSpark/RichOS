#!/usr/bin/env python3
"""Preserve ordinary Bash command failures before output filters can hide them.

PreToolUse preserves failure propagation and records subagent ownership. Subagent
CLI commands opt into a six-second foreground grace and native handoff; in the app
(every other entrypoint) an ordinary subagent command runs exactly as the host runs
it, with no added background flag, timeout or wait instruction. Wait calls remain
foreground and explicit background calls remain background. Held calls are denied. No shell parsing, command execution or auto-approval.
The notice about these rules is said once per context (say_once), not on every call.
Expected failures belong in explicit if/else or || branches. This covers the
calling shell; scripts and explicit failure handling retain their own semantics.
"""
import hashlib
import json
import os
import sys
import tempfile
import time

PREFIX = "set -e -o pipefail\n"
# The rewrite's own bound. hooks.json gives this hook 86400 s so it can hold a held
# agent's wait until its release (agent_hold.gate); the rewrite keeps its old 5 s.
REWRITE_SECONDS = 5
NOTICE = ("Shell commands use errexit and pipefail: a failed command or pipeline stops this call. "
          "Handle expected nonzero results explicitly with if/else. Keep JSON stdout separate from stderr. "
          "Read saved logs in a separate call; do not turn a failed check into success with a trailing echo or filter.")
# The host's own line when it compacts a conversation (a quoted copy inside a tool result is escaped).
COMPACTED = b'"subtype":"compact_boundary"'
NOTICE_STATE_DAYS = 7


def _notice_state(payload):
    """<config>/state/shell-evidence/<session>__<agent or lead>.json, or None without a session."""
    sid = payload.get("session_id")
    agent = payload.get("agent_id") or "lead"
    if not isinstance(sid, str) or not sid or not isinstance(agent, str):
        return None
    safe = lambda v: "".join(c if c.isalnum() or c in "-_." else "_" for c in v)[:120]
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state", "shell-evidence", safe(sid) + "__" + safe(agent) + ".json")


def _transcript(payload):
    """The transcript holding this conversation's context: a subagent's own file, as agent_hold.py finds it."""
    path, sid, agent = payload.get("transcript_path"), payload.get("session_id"), payload.get("agent_id")
    if not isinstance(path, str) or not os.path.isabs(path):
        return None
    if isinstance(agent, str) and agent and isinstance(sid, str) and os.path.basename(path) == sid + ".jsonl":
        path = os.path.join(os.path.dirname(path), sid, "subagents", "agent-" + agent + ".jsonl")
    return path


def _compacted_since(path, offset):
    """True when the host wrote a compaction line in `path` after byte `offset`."""
    with open(path, "rb") as fh:
        fh.seek(max(0, offset - len(COMPACTED) + 1))
        tail = b""
        while True:
            chunk = fh.read(1 << 20)
            if not chunk:
                return False
            if COMPACTED in tail + chunk:
                return True
            tail = chunk[-(len(COMPACTED) - 1):]


def say_once(payload, text):
    """`text` the first time this conversation's context would get it, "" after that.

    Once per session and per subagent; said again after the host compacts that context
    (its transcript gains a compaction line) and whenever the text differs. Without a
    session, or on any failure here, the text is said, as it always was."""
    try:
        path = _notice_state(payload)
        if path is None:
            return text
        try:
            with open(path, encoding="utf-8") as fh:
                rec = json.load(fh)
        except (OSError, ValueError):
            rec = None
        rec = rec if isinstance(rec, dict) else {}
        said = rec.get("said") if isinstance(rec.get("said"), list) else []
        transcript = _transcript(payload)
        size = None
        if transcript:
            try:
                size = os.path.getsize(transcript)
            except OSError:
                pass
        if size is not None and said:
            offset = rec.get("offset")
            if (rec.get("transcript") != transcript or not isinstance(offset, int) or size < offset
                    or _compacted_since(transcript, offset)):
                said = []
        digest = hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]
        out = "" if digest in said else text
        if out:
            said = said + [digest]
        directory = os.path.dirname(path)
        if not rec:
            _prune(directory)
        os.makedirs(directory, mode=0o700, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=directory, prefix=".notice.")
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump({"said": said, "transcript": transcript, "offset": size}, fh)
        os.replace(tmp, path)
        return out
    except Exception:
        return text


def _prune(directory):
    """Records untouched for NOTICE_STATE_DAYS go when a new conversation starts."""
    try:
        cutoff = time.time() - NOTICE_STATE_DAYS * 86400
        for name in os.listdir(directory):
            p = os.path.join(directory, name)
            if os.path.getmtime(p) < cutoff:
                os.unlink(p)
    except OSError:
        pass


def hold_gate(payload):
    """A held agent's wait call starts only after its release, so no model call is made
    while it waits (agent_hold.gate). Every other call passes at once."""
    try:
        sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"))
        import agent_hold
        agent_hold.gate(payload)
    except Exception:
        pass


def ownership(payload):
    """A subagent's call also records who owns it, so a pause can hold it (scripts/lib/agent_hold.py).

    This hook is the only Bash rewriter, so the capture lives here: two rewriting
    hooks would race for the one updated command. Returns {"command", "input"} or
    None. Any failure leaves the command exactly as it was without capture; the
    pause then cannot hold this call.
    """
    try:
        sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"))
        import agent_hold
        got = agent_hold.rewrite(payload)
        if isinstance(got, dict) and isinstance(got.get("command"), str) and isinstance(got.get("input"), dict):
            return got
    except Exception:
        pass
    return None


def rewrite(payload):
    if not isinstance(payload, dict) or not isinstance(payload.get("tool_name"), str):
        return {"systemMessage": "Shell evidence hook could not read this call (tool identity missing); failure propagation is unverified."}
    if payload["tool_name"] != "Bash":
        return {}
    original = payload.get("tool_input")
    if not isinstance(original, dict) or not isinstance(original.get("command"), str):
        return {"systemMessage": "Shell evidence hook could not read this call (Bash command missing); failure propagation is unverified."}
    command = original["command"]
    if command.startswith(PREFIX) and not payload.get("agent_id"):
        return {}
    owned = ownership(payload)
    if owned and owned.get("deny"):
        return {"hookSpecificOutput": {"hookEventName": "PreToolUse",
                "permissionDecision": "deny", "permissionDecisionReason": owned["deny"]}}
    updated = dict(original, command=PREFIX + command)
    if owned:
        updated.update(owned["input"])
        updated["command"] = PREFIX + owned["command"]
    out = {"hookEventName": "PreToolUse", "updatedInput": updated}
    # Said once per context, never on every call (152 copies, about 11k tokens, in one session).
    notice = say_once(payload, NOTICE + (" " + owned["context"] if owned and owned.get("context") else ""))
    if notice:
        out["additionalContext"] = notice
    return {"hookSpecificOutput": out}


class _Late(Exception):
    pass


def _late(_signum, _frame):
    raise _Late()


if __name__ == "__main__":
    import signal
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        payload = None
    if payload is None:
        result = {"systemMessage": "Shell evidence hook could not read this call (invalid payload); failure propagation is unverified."}
    else:
        hold_gate(payload)
        signal.signal(signal.SIGALRM, _late)
        signal.alarm(REWRITE_SECONDS)
        try:
            result = rewrite(payload)
        except _Late:
            # What the harness did at the old 5 s hook timeout: the call runs as typed.
            result = {"systemMessage": "Shell evidence hook ran out of time; failure propagation is unverified."}
        finally:
            signal.alarm(0)
    if result:
        print(json.dumps(result))
