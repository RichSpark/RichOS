#!/usr/bin/env bash
#
# worker-created-handoff.sh — PostToolUse[Agent] hook. LOG-ONLY; never blocks.
#
# THE GAP THIS CLOSES. Until this hook existed the engine emitted exactly two
# worker signals — a task COMPLETED (task-completed-handoff.sh) and a teammate
# went IDLE (teammate-idle-handoff.sh). A worker's CREATION was observed by
# guard-worktree-isolation.sh's partner (detect-nonnative-worktree.sh, which
# appends the plain-text spawned-names.log) and then thrown away as far as any
# typed consumer was concerned. That is precisely why the desktop app's
# worker_status.rs reports `active: 0` structurally: with no authoritative
# start, "currently active" could only ever have been guessed at, and it
# refuses to guess. This hook supplies the missing signal.
#
#   -> <team dir>/worker-events.jsonl   (fallback: ~/.claude/worker-events.jsonl)
#
# Full stream contract, and the per-state observability table (which of the
# seven UX states have a real source and which do not):
#   docs/worker-lifecycle-events.md
#
# WHY PostToolUse AND NOT PreToolUse. PreToolUse[Agent] fires on the INTENT to
# spawn. It runs FIRST in a four-hook chain and any later hook — or this one's
# own sibling guards — can still veto the same call, so a PreToolUse-sourced
# "created" event would invent workers that were never created. That is not a
# hypothetical: the identical mistake, made with the spawned-names ledger,
# burned names for teammates that never existed and is written up under
# "BLOCKED-SPAWN NAME BURN" in guard-worktree-isolation.sh. PostToolUse fires
# only for a call that actually ran, which makes "logged" and "created" the
# same event, and makes a BLOCKED spawn produce silence rather than a phantom
# active worker. (Claude Code routes a FAILED tool call to PostToolUseFailure,
# a different event this hook is not registered on, so even an errored spawn
# cannot reach here.)
#
# WHY THE ASYNC-LAUNCH ACKNOWLEDGEMENT IS REQUIRED. The Agent tool has two
# shapes. A backgrounded team spawn returns immediately with an acknowledgement
# naming the new agent ("Async agent launched successfully ... agentId: <id>")
# while the worker runs on; that return IS the creation. A SYNCHRONOUS subagent
# run, by contrast, returns its finished result — its PostToolUse fires when
# the work is already OVER. Emitting "created" for the second shape would
# announce a live worker at the moment it stopped existing. So this hook emits
# ONLY when the async acknowledgement is present and an agent_id can be
# extracted from it, and is silent otherwise. Silence is the honest answer: a
# synchronous Agent call is not a background worker, and §7 of the UX design is
# about background workers.
#
# WHAT THIS EVENT DOES AND DOES NOT CLAIM. It claims: the harness accepted this
# spawn and returned an agent id for it. It does NOT claim the worker has begun
# executing — that is a separate observation with its own hook
# (worker-started-handoff.sh, on SubagentStart). Nor does it claim the worker
# is STILL running: liveness is only ever "a created/started with no later
# terminal event", and the terminal events have their own hooks too
# (worker-ended-handoff.sh on SubagentStop; plus the pre-existing TeammateIdle
# and TaskCompleted logs). A consumer that wants an active COUNT composes those
# events; it must never infer one from this event alone.
#
# NEVER BLOCKS, ALWAYS EXIT 0. Same contract as the two sibling handoff hooks:
# any parse or IO error exits 0. A tool call must never fail because logging
# failed. Env override for tests: WORKER_EVENTS_TEAMS_DIR.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

# Keep event JSON out of argv/environment; descriptor 3 carries its bytes.
PAYLOAD="$(cat)"

python3 - 3<<< "$PAYLOAD" <<'PY'
import json
import os
import re
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

# Registered on PostToolUse; tolerate a payload that omits the event name (the
# sibling hooks do the same) but never act on a different event.
if payload.get("hook_event_name") not in ("", None, "PostToolUse"):
    finish()

if payload.get("tool_name") != "Agent":
    finish()

tool_input = payload.get("tool_input")
if not isinstance(tool_input, dict):
    tool_input = {}


def as_text(value):
    """Flatten an arbitrary tool_response (string, list of content blocks, or
    dict) into one searchable string. The harness has used more than one shape
    for this field, and a regex over the flattened form survives all of them."""
    if isinstance(value, str):
        return value
    try:
        return json.dumps(value)
    except Exception:
        return str(value)


response_text = as_text(payload.get("tool_response"))

# THE CREATION WITNESS. Both halves are required:
#   1. the async-launch acknowledgement — this was a backgrounded spawn, not a
#      synchronous run that has already finished;
#   2. an extractable agent id — the join key every later lifecycle event for
#      this worker carries (SubagentStart/SubagentStop payloads name agent_id;
#      the native isolation worktree is literally .claude/worktrees/agent-<id>).
# Missing either one means we cannot honestly say "a background worker now
# exists, and here is which one". In that case we write nothing at all.
if "Async agent launched successfully" not in response_text:
    finish()

m = re.search(r"agentId:\s*([A-Za-z0-9_-]+)", response_text)
if not m:
    finish()
agent_id = m.group(1)

session_id = payload.get("session_id") or ""
home = os.path.expanduser("~")
teams_dir = os.environ.get("WORKER_EVENTS_TEAMS_DIR") or os.path.join(home, ".claude", "teams")


def resolve_team_dir():
    """Same resolution the sibling handoff hooks use: exact session match
    first, then a single-session-dir fallback, else None."""
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
    "event": "WorkerCreated",
    # The UX design's worker state vocabulary (§7.1). Only ever a state this
    # hook actually witnessed — see the header.
    "lifecycle_state": "created",
    "source_hook": "PostToolUse[Agent]",
    "agent_id": agent_id,
    "worker_name": s(tool_input.get("name")),
    "agent_type": s(tool_input.get("subagent_type")),
    "isolation": s(tool_input.get("isolation")),
    "model_override": s(tool_input.get("model")),
    "session_id": session_id,
    "decision": "logged",
}

# The host CLI's pid, when the harness exports it. A consumer reading this log
# after the CLI died can check whether that process still exists rather than
# treating a stale "created" as a live worker — a real liveness test instead of
# a timeout heuristic. Omitted entirely when unavailable.
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
