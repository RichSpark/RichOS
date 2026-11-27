"""The terminal stall monitor's prompt cache renewal."""

import datetime
import html
import json
import os
import re
import sys

STATE_NAME = "keep-alive.json"
PENDING_NAME = "keep-alive-pending.json"
STATUSES = frozenset({
    "busy", "queued", "unreadable", "unmeasured", "no_one_hour_cache",
    "waiting", "outside_window", "other_notice", "awaiting_request", "emitted", "state_unavailable",
})

KEEP_ALIVE_TAG = "<richos-keep-alive>"
KEEP_ALIVE_TEXT = (
    KEEP_ALIVE_TAG + "RichOS sent this automatically to keep this chat ready. "
    "If another request is in progress, ignore this event. Otherwise reply with the single word ok "
    "and do nothing else.</richos-keep-alive>"
)
# The terminal can start idle compaction at 54 minutes. Allow for the
# monitor's one-minute poll, delivery and model startup before that mark.
DUE_AFTER = 50 * 60
WINDOW_END = 54 * 60
RETRY_AFTER = 2 * 60
TAIL_BYTES = 256 * 1024


def transcript_tail(path):
    """Read complete rows only. An incomplete write is a reason to wait."""
    with open(path, "rb") as fh:
        end = fh.seek(0, os.SEEK_END)
        start = max(0, end - TAIL_BYTES)
        fh.seek(start)
        data = fh.read(TAIL_BYTES)
    if not data.endswith(b"\n"):
        return
    for raw in reversed(data.splitlines()[bool(start):]):
        row = json.loads(raw)
        if not isinstance(row, dict):
            raise ValueError("invalid transcript row")
        yield row


def request_after_turn(path):
    """(status, last main request time). Use the provider request's own
    timestamp when recorded, with the matching assistant response's usage."""
    ended = False
    dequeued = 0
    latest = None
    try:
        for row in transcript_tail(path):
            if row.get("isSidechain") or row.get("agentId"):
                continue
            kind = row.get("type")
            if kind == "queue-operation":
                operation = row.get("operation")
                if operation in ("dequeue", "remove"):
                    dequeued += 1
                if operation == "enqueue":
                    if not dequeued:
                        return "queued", None
                    dequeued -= 1
                continue
            if not ended:
                if kind in ("user", "assistant", "api-request") or (
                    row.get("attachment") or {}
                ).get("type") == "queued_command":
                    return "busy", None
                if kind == "system" and row.get("subtype") == "turn_duration":
                    ended = True
                continue
            if kind == "assistant" and row.get("requestId"):
                if latest is not None and row["requestId"] != latest["requestId"]:
                    break
                latest = row            # first streamed block for this request
                continue
            if kind == "api-request" and latest is not None and row.get("id") == latest.get("requestRef"):
                return cache_reading(latest, row["timestamp"])
            if kind == "api-request" or (kind == "system" and row.get("subtype") == "compact_boundary"):
                break
            if latest is not None and kind == "user":
                break
        if latest is not None:
            return cache_reading(latest, latest["timestamp"])
    except (OSError, ValueError, TypeError, KeyError, AttributeError):
        return "unreadable", None
    return "unmeasured", None


def cache_reading(assistant, at):
    message = assistant.get("message") or {}
    usage = message.get("usage") or {}
    created = usage.get("cache_creation") or {}
    requested = assistant.get("promptCacheRequested")
    one_hour = created.get("ephemeral_1h_input_tokens", 0)
    confirmed_read = (requested == "1h" and created.get("ephemeral_5m_input_tokens", 0) == 0
                      and usage.get("cache_read_input_tokens", 0) > 0)
    if requested not in (None, "1h") or not (one_hour > 0 or confirmed_read):
        return "no_one_hour_cache", None
    return "idle", timestamp(at)


def timestamp(at):
    return datetime.datetime.fromisoformat(at.replace("Z", "+00:00")).timestamp()


def observe(path, now, previous, other_notice):
    """Return a bounded observation and the time of the latest attempted wake."""
    status, request = request_after_turn(path)
    last_attempt = previous.get("attempt")
    attempt = last_attempt if isinstance(last_attempt, (int, float)) else 0
    idle = max(0, now - request) if request is not None else None
    if idle is not None:
        if idle < DUE_AFTER:
            status = "waiting"
        elif idle >= WINDOW_END:
            status = "outside_window"
        elif other_notice:
            status = "other_notice"
        elif now - attempt < RETRY_AFTER:
            status = "awaiting_request"
        else:
            status = "emitted"
            attempt = now
    return {"at": now, "status": status, "last_request": request,
            "idle_seconds": int(idle) if idle is not None else None, "attempt": attempt}


_MONITOR_EVENT = re.compile(
    r'<task-notification>\s*<task-id>[^<>]+</task-id>\s*'
    r'<summary>Monitor event: [^<>]*</summary>\s*<event>([^<>]*)</event>\s*</task-notification>'
)


def heartbeat_content(content):
    if not isinstance(content, str):
        return False
    event = _MONITOR_EVENT.fullmatch(content.strip())
    return bool(event and html.unescape(event.group(1)) == KEEP_ALIVE_TEXT)


def owned_turn(payload, state, now):
    """Match a standalone native task notification to a recent monitor write.
    A copied tag, a queued attachment and a normal prompt carry no authority."""
    attempt = state.get("attempt")
    if type(attempt) not in (int, float) or not 0 <= now - attempt < RETRY_AFTER:
        return False
    event = payload.get("hook_event_name")
    if event not in ("UserPromptSubmit", "PreToolUse", "Stop") or payload.get("agent_id"):
        return False
    incoming = event == "UserPromptSubmit"
    prompt_id = payload.get("prompt_id")
    if not prompt_id or (incoming and not heartbeat_content(payload.get("prompt"))):
        return False
    session_id = payload.get("session_id")
    matched = False
    for row in transcript_tail(payload.get("transcript_path")):
        if row.get("sessionId") != session_id or row.get("isSidechain"):
            continue
        kind = row.get("type")
        if kind == "user" and not row.get("isMeta"):
            message = row.get("message") or {}
            # Tool results can follow the original turn-opening user row.
            if isinstance(message.get("content"), list) and any(
                block.get("type") == "tool_result" for block in message["content"]
            ):
                continue
            if (matched or row.get("promptId") != prompt_id or row.get("turnOrigin") != "task_notification"
                    or timestamp(row["timestamp"]) < attempt or not heartbeat_content(message.get("content"))):
                return False
            matched = True
        if incoming and not matched and kind == "queue-operation" and row.get("operation") == "enqueue":
            return timestamp(row["timestamp"]) >= attempt and row.get("content") == payload["prompt"]
        if kind == "system" and row.get("subtype") == "turn_duration":
            return matched
    return False


def classify_hook(state_path, session_id):
    """A hook may skip only when the recent emission and native prompt agree."""
    try:
        with open(state_path, encoding="utf-8") as fh:
            state = json.load(fh)
        payload = json.load(sys.stdin)
        if payload.get("session_id") == session_id and owned_turn(payload, state, datetime.datetime.now().timestamp()):
            return 2 if payload["hook_event_name"] == "PreToolUse" else 0
    except (OSError, ValueError, TypeError, KeyError, AttributeError):
        pass
    return 1


def diagnostic(directory):
    """Bounded support output for stall-watch --alive."""
    try:
        with open(os.path.join(directory, STATE_NAME), encoding="utf-8") as fh:
            state = json.loads(fh.read(2048))
        status = state["status"]
        if status not in STATUSES:
            return None
        idle = state.get("idle_seconds")
        attempt = state.get("attempt")
        elapsed = f"{idle}s" if type(idle) is int else "unknown"
        last_attempt = datetime.datetime.fromtimestamp(attempt, datetime.timezone.utc).isoformat(
            timespec="seconds") if type(attempt) in (int, float) and attempt > 0 else "none"
        return f"stall-watch: terminal keep-alive {status}; request age {elapsed}; last emission {last_attempt}"
    except (OSError, ValueError, TypeError, KeyError, OverflowError):
        return None


if __name__ == "__main__":
    sys.exit(classify_hook(*sys.argv[1:]))
