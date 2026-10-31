#!/usr/bin/env bash
#
# teammate-idle-handoff.sh — TeammateIdle hook. LOG-ONLY; never blocks.
#
# The teammate->lead idle-notification mailbox is lossy. This hook fires
# deterministically on EVERY teammate idle and appends one JSON line to a
# durable session-scoped log the orchestrator reads:
#
#   ~/.claude/teams/session-<first8-of-session-id>/idle-events.jsonl
#   (fallback: ~/.claude/teammate-idle-events.jsonl)
#
# It is one of TWO guaranteed completion signals (the companion
# task-completed-handoff.sh writes task-events.jsonl). Together with
# ground-truth reads (git log on the teammate's branch) they make the mailbox
# non-load-bearing for handoff detection.
#
# Extra (non-blocking) annotation: if the payload's cwd is a git checkout,
# the record carries `uncommitted_changes` (count of dirty paths) — the
# orchestrator can spot idle-with-uncommitted-work from the log without this
# hook ever blocking anyone.
#
# Fail-open everywhere: any parse/IO error exits 0. Env override for tests:
# TEAMMATE_IDLE_TEAMS_DIR.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

# Keep event JSON out of argv/environment: Linux imposes per-string exec limits.
# Descriptor 3 carries the data while stdin carries the inline Python source.
PAYLOAD="$(cat)"

# Cheap, best-effort dirty-count for the teammate's cwd. Computed in bash
# (not python) so the python block below stays dependency-free.
CWD="$(printf '%s' "$PAYLOAD" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("cwd",""))' 2>/dev/null || true)"
UNCOMMITTED=""
if [ -n "$CWD" ] && [ -d "$CWD" ]; then
    if git -C "$CWD" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        UNCOMMITTED="$(git -C "$CWD" status --porcelain 2>/dev/null | grep -c . || true)"
    fi
fi

# --- ownership ledger: an ADVISORY per-agent finish signal -------------------
# Retained in ~/.claude/state/worktree-ledger.jsonl (scripts/lib/
# worktree-ledger.py) against the agent id and the worktree path the payload
# carries. It is per-agent where the harness lock is per-session, which is
# why it is kept — and it is NOT a termination: an idle teammate can be
# resumed, a completed task is task-grain, and SubagentStop fires every turn.
# The reaper prints these beside its verdict and never decides on them.
# Best-effort; this hook's exit code is unchanged whatever happens here.
_LEDGER_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/worktree-ledger.py"
if [ -f "$_LEDGER_PY" ] && command -v python3 >/dev/null 2>&1; then
  LEDGER_PY="$_LEDGER_PY" SIGNAL="TeammateIdle" python3 - 3<<< "$PAYLOAD" <<'PY' 2>/dev/null || true
import importlib.util, json, os
spec = importlib.util.spec_from_file_location("wl", os.environ["LEDGER_PY"])
wl = importlib.util.module_from_spec(spec); spec.loader.exec_module(wl)
try:
    d = json.load(os.fdopen(3))
except Exception:
    d = {}
if not isinstance(d, dict) or d.get("hook_event_name") not in ("", None, os.environ["SIGNAL"]):
    raise SystemExit(0)
def first(*keys):
    for k in keys:
        v = d.get(k)
        if v not in (None, ""):
            return str(v)
    return ""
cwd = first("cwd")
agent_id = first("agent_id", "agentId")
if not agent_id and cwd:
    b = os.path.basename(cwd.rstrip("/"))
    if b.startswith("agent-"):
        agent_id = b[len("agent-"):]
rec = {"event": "finished", "signal": os.environ["SIGNAL"], "agent_id": agent_id,
       "teammate": first("teammate_name", "teammateName", "agent_name", "agentName", "name"),
       "session_id": first("session_id"), "worktree": cwd, "task_id": first("task_id", "taskId", "id"),
       "source": "teammate-idle-handoff.sh"}
if agent_id or cwd or rec["teammate"] or rec["task_id"]:
    wl.append(rec)
PY
fi

UNCOMMITTED="$UNCOMMITTED" python3 - 3<<< "$PAYLOAD" <<'PY'
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

if payload.get("hook_event_name") not in ("", None, "TeammateIdle"):
    finish()

def first(*keys):
    for k in keys:
        v = payload.get(k)
        if v:
            return v
    return ""

session_id = payload.get("session_id") or ""
home = os.path.expanduser("~")
teams_dir = os.environ.get("TEAMMATE_IDLE_TEAMS_DIR") or os.path.join(home, ".claude", "teams")

def resolve_team_dir():
    if session_id:
        candidate = os.path.join(teams_dir, "session-%s" % session_id[:8])
        if os.path.isdir(candidate):
            return candidate
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
log_path = os.path.join(team_dir, "idle-events.jsonl") if team_dir else os.path.join(home, ".claude", "teammate-idle-events.jsonl")

record = {
    "timestamp": datetime.now(timezone.utc).isoformat(),
    "event": "TeammateIdle",
    "session_id": session_id,
    # The payload historically carries no teammate name; extract defensively
    # in case the harness adds one under any spelling.
    "teammate": first("teammate_name", "teammateName", "agent_name", "agentName", "name"),
    "agent_id": first("agent_id", "agentId"),
    "transcript_path": payload.get("transcript_path") or "",
    "cwd": payload.get("cwd") or "",
    "decision": "logged",
    # MEASUREMENT FIXTURE (CEO specification 2026-09-03, worktree-terminal-
    # authority-fix-recommendation section 3). TeammateIdle is a candidate
    # terminal ingress whose payload has never been observed live on this
    # machine (every row of this log is a test fixture). The first live
    # firing must show whether any field joins to the spawn-side ownership
    # id, so the record carries the payload's top-level KEY NAMES — never
    # values: a prompt or a model message must not land in an evidence log —
    # plus its identity, type and task fields, which are the only values
    # the specification asks to keep.
    "payload_keys": sorted(k for k in payload.keys() if isinstance(k, str)),
    "agent_type": str(payload.get("agent_type") or ""),
    "task_id": first("task_id", "taskId"),
}

uncommitted = os.environ.get("UNCOMMITTED", "")
if uncommitted != "":
    try:
        record["uncommitted_changes"] = int(uncommitted)
    except ValueError:
        pass

try:
    os.makedirs(os.path.dirname(log_path), exist_ok=True)
    with open(log_path, "a", encoding="utf-8") as f:
        f.write(json.dumps(record) + "\n")
except Exception:
    pass

finish()
PY
