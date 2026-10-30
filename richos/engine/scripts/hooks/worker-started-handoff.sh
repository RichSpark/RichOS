#!/usr/bin/env bash
#
# worker-started-handoff.sh — SubagentStart hook. LOG-ONLY; never blocks.
#
# The second of the four worker-lifecycle emitters. Appends one JSON line per
# observed worker START to the same durable stream the creation hook writes:
#
#   -> <team dir>/worker-events.jsonl   (fallback: ~/.claude/worker-events.jsonl)
#
# Full stream contract + the per-state observability table:
#   docs/worker-lifecycle-events.md
#
# WHY THIS IS A SEPARATE EVENT FROM "created". Creation and start are two
# genuinely different observations and the harness reports them separately.
# PostToolUse[Agent] fires when the spawn call returns — for a backgrounded
# teammate that is an acknowledgement ("launched"), and the worker's own turn
# has not necessarily begun. SubagentStart fires from inside the worker's own
# execution, and carries the worker's agent_id and agent_type in the payload.
# Collapsing the two would have meant asserting one from the other, which is
# the inference this whole slice exists to avoid.
#
# WHAT IT CLAIMS. Exactly: "the harness began running the subagent with this
# agent_id". Nothing about progress, nothing about success, and — critically —
# nothing about the worker still being live at read time. A worker is live only
# as "started, with no later terminal event", and the terminal events have
# their own emitters.
#
# NOT ONLY TEAMMATES. SubagentStart fires for every subagent the host runs,
# including short read-only helpers that are not delegated "AI workers" in the
# UX-design sense. This hook does not editorialize about which is which — it
# records agent_type verbatim and leaves the classification to the consumer,
# because a filter applied here would be an opinion baked into an evidence log.
#
# ATTRIBUTION IS REQUIRED, NEVER GUESSED. The event is written ONLY when the
# payload carries an agent_id. An unattributable start cannot be joined to the
# worker it belongs to, and an unjoined "some worker started" would inflate any
# active count with an anonymous phantom. No agent_id -> no line.
#
# NEVER BLOCKS, ALWAYS EXIT 0 — same contract as every sibling handoff hook: a
# session must never stall because logging failed. SubagentStart is a blockable
# event for hooks that want to be; this one never is. Env override for tests:
# WORKER_EVENTS_TEAMS_DIR.

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

if payload.get("hook_event_name") not in ("", None, "SubagentStart"):
    finish()

agent_id = payload.get("agent_id") or ""
if not isinstance(agent_id, str) or not agent_id:
    # Unattributable start — see the header. Silence beats an anonymous worker.
    finish()

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


record = {
    "timestamp": datetime.now(timezone.utc).isoformat(),
    "event": "WorkerStarted",
    "lifecycle_state": "started",
    "source_hook": "SubagentStart",
    "agent_id": agent_id,
    # The spawn-time display name is NOT in this payload. It is in the
    # WorkerCreated record for the same agent_id; the consumer joins on
    # agent_id rather than this hook inventing a name it never saw.
    "agent_type": s(payload.get("agent_type")),
    "session_id": session_id,
    "cwd": s(payload.get("cwd"), 512),
    "decision": "logged",
}

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
