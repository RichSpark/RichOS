#!/usr/bin/env bash
#
# worker-updated-handoff.sh — PostToolUse[SendMessage] hook. LOG-ONLY; never blocks.
#
# The "{name} updated" signal (UX design §7.1's timeline verb, §7.2's "latest
# authored update"). Appends one JSON line per update a WORKER authored, to the
# same durable stream the other three lifecycle emitters write:
#
#   -> <team dir>/worker-events.jsonl   (fallback: ~/.claude/worker-events.jsonl)
#
# Full stream contract + the per-state observability table:
#   docs/worker-lifecycle-events.md
#
# WHY THIS IS A REAL SIGNAL AND NOT A MAILBOX READ. The orchestration doctrine
# is explicit that the teammate->lead mailbox is lossy and that no load-bearing
# signal may depend on it. This hook does not read the mailbox. It observes the
# SEND ITSELF, in the sending worker's own execution, at the moment the tool
# call resolves — so it witnesses the update even in the cases where delivery
# is dropped. The event is "a worker authored an update", which is true
# independently of whether anyone received it.
#
# WHY PostToolUse AND NOT PreToolUse. Same reasoning as the creation hook, and
# it matters twice over here: PreToolUse[SendMessage] is where the BLOCKING
# resume-isolation guard lives, and a message that guard refuses is a message
# that was never sent. Sourcing this from PreToolUse would log updates that
# never happened, and would put a logger inside a blocking chain for no reason.
# PostToolUse fires only for a send that actually went through.
#
# ATTRIBUTION IS THE WHOLE DESIGN. Every hook payload carries agent_id ONLY
# when the hook fired from inside a subagent. That single field is what
# separates the two kinds of SendMessage:
#
#   * agent_id PRESENT  -> a worker sent this. That is an update, and it is
#                          attributable to exactly that worker. Logged.
#   * agent_id ABSENT   -> the lead sent this (the main thread). "Messaged
#                          {name}" is an orchestrator action, not a worker
#                          state. NOT logged.
#
# There is no fallback, no cwd-sniffing, no name matching. An update that
# cannot be attributed by identity is not written, because a misattributed
# update is a lie about which worker said what, and that is worse than a
# missing row.
#
# CONTENT IS NEVER LOGGED. Only the SendMessage tool's own short `summary`
# field (already a designed-for-display recap) is recorded, truncated, plus the
# message's character count. The message body never enters this log: it can
# carry anything, including credentials, and an evidence log that quietly
# accumulates model output is a privacy defect waiting to happen. A consumer
# that needs the text reads the transcript under its own rules.
#
# NEVER BLOCKS, ALWAYS EXIT 0. Env override for tests: WORKER_EVENTS_TEAMS_DIR.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

# Keep event JSON out of argv/environment; descriptor 3 carries its bytes.
PAYLOAD="$(cat)"

python3 - 3<<< "$PAYLOAD" <<'PY'
import json
import os
import sys
from datetime import datetime, timezone


def finish():
    sys.exit(0)


try:
    payload = json.load(os.fdopen(3))
except Exception:
    finish()

if not isinstance(payload, dict):
    finish()

if payload.get("hook_event_name") not in ("", None, "PostToolUse"):
    finish()

if payload.get("tool_name") != "SendMessage":
    finish()

# THE ATTRIBUTION GATE — see the header. Absent agent_id means the lead sent
# it, which is not a worker update.
agent_id = payload.get("agent_id") or ""
if not isinstance(agent_id, str) or not agent_id:
    finish()

tool_input = payload.get("tool_input")
if not isinstance(tool_input, dict):
    tool_input = {}

session_id = payload.get("session_id") or ""
home = os.path.expanduser("~")
teams_dir = os.environ.get("WORKER_EVENTS_TEAMS_DIR") or os.path.join(home, ".claude", "teams")


def resolve_team_dir():
    if session_id:
        candidate = os.path.join(teams_dir, "session-%s" % session_id[:8])
        if os.path.isdir(candidate):
            return candidate
        # ROUND 15 (2026-09-11, Frank D4): A KNOWN SESSION WHOSE OWN DIRECTORY
        # IS ABSENT WRITES TO THE FALLBACK FILE, NEVER TO SOMEBODY ELSE'S
        # DIRECTORY. A reader (the transaction store, removed 2026-09-11)
        # opens this session's directory and the fallback file, keyed by the
        # full session id this row carries; it never opens a foreign session's
        # log. The single-directory guess below filed this row into whatever
        # one directory happened to exist, where only THAT session's reader
        # looks and this session's never does -- reproduced in a sandbox by
        # Frank (round four, D4): one foreign directory present, the row landed
        # there, the reader returned nothing. The guess stays for the case it
        # was written for: a payload that carries NO session id at all.
        return None
    try:
        sessions = [
            os.path.join(teams_dir, name)
            for name in os.listdir(teams_dir)
            if name.startswith("session-") and os.path.isdir(os.path.join(teams_dir, name))
        ]
    except Exception:
        sessions = []
    return sessions[0] if len(sessions) == 1 else None


team_dir = resolve_team_dir()
log_path = (
    os.path.join(team_dir, "worker-events.jsonl")
    if team_dir
    else os.path.join(home, ".claude", "worker-events.jsonl")
)


def s(value, limit=200):
    if value in (None, ""):
        return ""
    return str(value)[:limit]


# The message body is measured, never stored. A structured (protocol) body has
# no meaningful character count, so it is reported as such rather than as a
# number derived from its JSON encoding.
message = tool_input.get("message")
if isinstance(message, str):
    message_chars = len(message)
    message_kind = "text"
else:
    message_chars = None
    message_kind = "structured"

record = {
    "timestamp": datetime.now(timezone.utc).isoformat(),
    "event": "WorkerUpdated",
    "lifecycle_state": "updated",
    "source_hook": "PostToolUse[SendMessage]",
    "agent_id": agent_id,
    "agent_type": s(payload.get("agent_type")),
    "to": s(tool_input.get("to")),
    "summary": s(tool_input.get("summary")),
    "message_kind": message_kind,
    "session_id": session_id,
    "decision": "logged",
}
if message_chars is not None:
    record["message_chars"] = message_chars

host_pid = os.environ.get("CLAUDE_PID", "")
if host_pid.isdigit():
    record["host_pid"] = int(host_pid)

try:
    os.makedirs(os.path.dirname(log_path), exist_ok=True)
    with open(log_path, "a", encoding="utf-8") as f:
        f.write(json.dumps(record) + "\n")
except Exception:
    pass

finish()
PY
