#!/usr/bin/env bash
#
# task-completed-handoff.sh — TaskCompleted hook. Verify committed integration before recording completion.
#
# Appends one JSON line per task completion to the session team directory
# (task-events.jsonl) — durable coordination state for the orchestrator, one
# of the two guaranteed completion signals in the durable-handoff model
# (companion: teammate-idle-handoff.sh -> idle-events.jsonl). A completed task
# is pruned from the queryable task store, so this append-only log + the commit
# are the durable completion record.
#
# A refused proof exits 2 so the native task remains open with remediation.
# Idle and SubagentStop remain observations, not accepted delivery.
# This cooperative hook is not a same-user security or writer-cutoff boundary.
#
# "COULD NOT READ THE MESSAGE" AND "THE WORK IS NOT DELIVERED" ARE DIFFERENT
# FINDINGS, and until this was split the hook gave both the same answer: exit 2,
# task stays open. A lifecycle payload IS a message; the agent<->lead channel is
# measured at roughly 50% loss, and the standing rule is that no load-bearing
# signal may depend on it. An unparseable payload could therefore WEDGE a
# completion -- letting the lossy channel be decisive, which is the exact thing
# that rule forbids -- and it would do so without the gate ever looking at the
# evidence it actually judges, which is the COMMIT.
#
# So the two findings now get two answers:
#
#   unreadable payload        no advisory side effect, a refusal row in the
#                             durable event log so it stays visible, exit 0. It
#                             did not prove delivery; it also did not DISPROVE
#                             it, and only a proof may hold a task open.
#   readable payload whose
#   delivery evidence fails   exit 2. That is the gate doing its job.
#
# The malformed-event check's original intent is kept whole: garbage still
# triggers no ledger row, no receipt and no accepted completion.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

# Keep event JSON out of argv/environment: Linux imposes per-string exec limits.
# Descriptor 3 carries the data while stdin carries the inline Python source.
PAYLOAD="$(cat)"

_PROOF_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/completion-proof.py"

# The durable event record, parameterized by decision. Defined up here because
# the unreadable path below needs it too, and that path has to be able to say
# so before anything else runs.
_record_decision() {
    TASK_COMPLETED_DECISION="$1" python3 - 3<<< "$PAYLOAD" <<'PY'
import json
import os
import sys
from datetime import datetime, timezone

def finish():
    sys.exit(0)

decision = os.environ.get("TASK_COMPLETED_DECISION") or "verified"

try:
    payload = json.load(os.fdopen(3))
except Exception:
    payload = None

if not isinstance(payload, dict):
    # An accepted completion is never written from a payload nobody could read;
    # a refusal is exactly the case where there is nothing left to read.
    if decision == "verified":
        finish()
    payload = {}

if decision == "verified" and payload.get("hook_event_name") not in ("", None, "TaskCompleted"):
    finish()

session_id = payload.get("session_id") or ""
home = os.path.expanduser("~")
teams_dir = os.environ.get("TASK_COMPLETED_TEAMS_DIR") or os.path.join(home, ".claude", "teams")

def resolve_team_dir():
    team_name = payload.get("team_name") or ""
    if isinstance(team_name, str) and team_name.startswith("session-"):
        candidate = os.path.join(teams_dir, team_name)
        if os.path.isdir(candidate):
            return candidate
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

def first(*keys):
    for key in keys:
        value = payload.get(key)
        if value not in (None, ""):
            return value
    return ""

team_dir = resolve_team_dir()
log_path = os.path.join(team_dir, "task-events.jsonl") if team_dir else os.path.join(home, ".claude", "teammate-task-events.jsonl")

record = {
    "timestamp": datetime.now(timezone.utc).isoformat(),
    "event": "TaskCompleted",
    "task_id": first("task_id", "taskId", "id"),
    "task_subject": first("task_subject", "task_title", "subject", "title"),
    "teammate": first("teammate_name", "agent_type", "agentType", "owner", "agent_id", "agentId"),
    "session_id": session_id,
    "decision": decision
}

try:
    os.makedirs(os.path.dirname(log_path), exist_ok=True)
    with open(log_path, "a", encoding="utf-8") as f:
        f.write(json.dumps(record) + "\n")
except Exception:
    pass

finish()
PY
}

# An unreadable payload is recorded and waved through. An explicitly different
# event is readable and is simply not a completion, so it is not this hook's
# business and gets no row.
_EVENT="$(printf '%s' "$PAYLOAD" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert isinstance(d,dict); event=d.get("hook_event_name"); assert isinstance(event,str) and event.strip(); print(event)')" || _EVENT=""
if [ -z "$_EVENT" ]; then
    _record_decision unreadable
    exit 0
fi
[ "$_EVENT" = "TaskCompleted" ] || exit 0
# The plugin is global; the existing root contract limits enforcement to adopters.
_ROOT_LIB="$(dirname "$_PROOF_PY")/resolve-roots.sh"
if [ ! -f "$_ROOT_LIB" ]; then
    echo "Task remains unfinished: RichOS root resolver is missing." >&2
    exit 2
fi
. "$_ROOT_LIB" || exit 2
# STAND IN THE WORKSPACE THE EVENT NAMES BEFORE ASKING WHO GOVERNS IT.
#
# resolve_entity_root's last-resort candidate is $PWD — correct for a host that
# offers no other signal, and WRONG here, because $PWD is wherever this hook
# process happened to be launched and has nothing to do with the completion
# being judged. When that cwd sits anywhere inside the engine's own checkout,
# the engine root carries orchestration.config, so the pwd candidate ACCEPTS the
# engine as the governing entity for a task completed in a repository that never
# adopted the engine. The gate then refuses (exit 2) where the contract says it
# must stand down (exit 0), and a task in an unadopted repository can never be
# marked complete. Measured: CLAUDE_PROJECT_DIR and the payload cwd both naming
# an unadopted repo still resolved status=governed source=pwd
# root=<engine>, because the caller stood in <engine>.
#
# This is a hole straight through the engine-self clause, which refuses exactly
# this substitution one branch further down (it requires the engine root to lie
# under the session anchor). The pwd candidate reaches the same root with no
# such check.
#
# A lifecycle event CARRIES its workspace, so this hook does not have to guess:
# cd into the event's cwd and the last resort becomes the session's own
# repository, which is what "stand down for unadopted repositories" means. When
# the payload names no usable cwd, nothing changes — the existing precedence
# stands. Both surviving absolute paths (_PROOF_PY, _ROOT_LIB) are resolved
# above, and nothing below this line reads a relative path.
_EVENT_CWD="$(printf '%s' "$PAYLOAD" | python3 -c 'import json,sys
value=json.load(sys.stdin).get("cwd")
print(value if isinstance(value,str) else "")' 2>/dev/null || true)"
if [ -n "$_EVENT_CWD" ] && [ -d "$_EVENT_CWD" ]; then
    cd "$_EVENT_CWD" || exit 2
fi
if ! resolve_entity_root "$PAYLOAD"; then
    case "$RICHOS_ROOT_STATUS" in
        not-adopted) exit 0 ;;
        *) root_failure_banner "task-completed-handoff" >&2; exit 2 ;;
    esac
fi
if ! printf '%s' "$PAYLOAD" | python3 -B "$_PROOF_PY"; then
    exit 2
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
  LEDGER_PY="$_LEDGER_PY" SIGNAL="TaskCompleted" python3 - 3<<< "$PAYLOAD" <<'PY' 2>/dev/null || true
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
       "source": "task-completed-handoff.sh"}
if agent_id or cwd or rec["teammate"] or rec["task_id"]:
    wl.append(rec)
PY
fi

_record_decision verified
