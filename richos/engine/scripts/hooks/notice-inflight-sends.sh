#!/usr/bin/env bash
#
# notice-inflight-sends.sh — PostToolUse[SendMessage] hook. LOG-ONLY; never blocks.
#
# THE WITNESS THAT MAKES THE SEND HALF ENFORCEABLE.
#
# rich-lander/SKILL.md §8b says: when a land moves main under a live teammate,
# MESSAGE THEM. Twice on 2026-08-30 that was not done, and the only record of
# the omission was the damage — an eight-hunk conflict across five files, and a
# library that shipped at 7 of 19. A written step cannot be checked. A send can.
#
# So this hook writes one JSON line per message the LEAD authored:
#
#   -> <team dir>/inflight-notices.jsonl   (fallback: ~/.claude/inflight-notices.jsonl)
#
# guard-inflight-notify.sh reads that ledger and REFUSES a push that leaves a
# live teammate behind with no line in it.
#
# ===========================================================================
# THE ATTRIBUTION GATE IS THE EXACT MIRROR OF ITS SIBLING
# ===========================================================================
# worker-updated-handoff.sh logs sends where agent_id is PRESENT — a worker
# reporting. This hook logs sends where agent_id is ABSENT — the lead
# messaging a teammate. Same field, opposite branch, and between them every
# SendMessage that actually resolves is witnessed exactly once by exactly one
# of them. Neither reads a mailbox: both observe the SEND, in the sender's own
# execution, at the moment the tool call resolves. That is what makes the
# record true even when delivery drops, which is the whole point given a
# channel measured at ~50% loss.
#
# WHY PostToolUse. PreToolUse[SendMessage] fires on the INTENT, and the
# blocking guard-resume-isolation.sh sits there and can veto the call. A
# PreToolUse-sourced record would credit the lander for notices that were
# refused and never sent. PostToolUse fires only for a send that went through.
#
# ===========================================================================
# WHAT IS RECORDED, AND WHY THE SHAs ARE THE EXCEPTION
# ===========================================================================
# The sibling hook's rule stands: THE MESSAGE BODY IS NEVER LOGGED. It can
# carry anything, and an evidence log that quietly accumulates model output is
# a privacy defect waiting to happen. Recorded instead: the recipient, the
# tool's own short `summary`, the body's length and sha256 — and the one
# addition this guard needs, every 7-40 character hex run in the body.
#
# A commit SHA is a public identifier, not a credential, and it is the only
# thing that can tie a notice to the land it is about. Without it the ledger
# could say "somebody was messaged" and never "somebody was told about THIS
# move", which is the difference between a log and a guarantee. The extraction
# is a plain hex regex; it cannot pull out prose, and anything it does pull out
# is by construction a bare hex token.
#
# ===========================================================================
# THE RECIPIENT IS RECORDED TWICE: AS ADDRESSED, AND AS RESOLVED
# ===========================================================================
# `to` is what SendMessage was handed — always the teammate's MANDATORY UNIQUE
# NAME (`zach-opus-s1`), because that is the only thing it can be addressed
# with. That is recorded verbatim and always.
#
# `to_agent_id` is that name resolved to an agent id, HERE, in the lead's own
# execution, by scripts/lib/teammate-identity.py — the same module the guard
# resolves worktrees with. It is added because of 2026-08-31: two notices were
# genuinely sent, witnessed and logged, and the guard still reported
# OWED-NO-NOTICE, because the debt side was resolving teammates as the ROLE
# (`zach`, from worker-events `agent_type`) and nothing could ever join `zach`
# to `zach-opus-s1`. Recording the resolved id makes the join exact and makes
# it independent of what the debt side can still resolve at push time.
#
# BEST EFFORT, NEVER LOAD-BEARING HERE: an unresolvable name records an empty
# `to_agent_id` and the guard falls back to matching on the name. This hook
# never fails, never blocks, and never delays a send over an identity lookup.
#
# ===========================================================================
# WHAT THIS LEDGER DOES NOT PROVE — named, not implied
# ===========================================================================
# It proves the lead SENT a message naming a SHA to a named recipient. It does
# NOT prove the message was delivered, read, or understood; those are the ack's
# job (scripts/lib/inflight.py -> ack_status). And it is not fraud-proof:
# anyone with Bash can append a line to a JSONL file. The failure being
# engineered out is FORGETTING. Fabrication is a different act, and no hook in
# this engine claims to stop it.
#
# NEVER BLOCKS, ALWAYS EXIT 0. Env override for tests: INFLIGHT_TEAMS_DIR
# (WORKER_EVENTS_TEAMS_DIR is honored too, so a test harness that already sets
# one does not have to set two).

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

INFLIGHT_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" 2>/dev/null && pwd)"
export INFLIGHT_LIB_DIR

# Keep event JSON out of argv/environment; descriptor 3 carries its bytes.
PAYLOAD="$(cat)"

python3 - 3<<< "$PAYLOAD" <<'PY'
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone


def identity_module():
    """scripts/lib/teammate-identity.py — the SAME module the guard resolves
    worktrees with. Absent or broken costs a field, never the record."""
    try:
        import importlib.util as ilu
        path = os.path.join(os.environ.get("INFLIGHT_LIB_DIR", ""),
                            "teammate-identity.py")
        if not os.path.isfile(path):
            return None
        spec = ilu.spec_from_file_location("teammate_identity_witness", path)
        mod = ilu.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod
    except Exception:
        return None


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

# THE ATTRIBUTION GATE — the mirror of worker-updated-handoff.sh. An agent_id
# means a WORKER sent this, which is that hook's business, not ours.
if payload.get("agent_id"):
    finish()

tool_input = payload.get("tool_input")
if not isinstance(tool_input, dict):
    tool_input = {}

to = str(tool_input.get("to", "") or "")
if not to:
    finish()

# The lead/reply channel is not a notice to a teammate.
if to.strip().lower() in ("main", "team-lead", "lead", "user"):
    finish()

message = tool_input.get("message")
if isinstance(message, str):
    body = message
    kind = "text"
else:
    # A structured protocol body (shutdown / plan-approval) is control traffic,
    # never an in-flight notice. Recorded with no SHAs so it can never satisfy
    # the guard by accident.
    body = ""
    kind = "structured"

shas = sorted({m.group(0) for m in re.finditer(r"\b[0-9a-f]{7,40}\b", body.lower())})

session_id = payload.get("session_id") or ""
home = os.path.expanduser("~")

# THE SAME LADDER THE GUARD USES. Written and read by one resolver, because a
# ledger written to one path and read from another is the identity defect one
# layer down.
IDENTITY = identity_module()
team_dir = ""
if IDENTITY is not None:
    try:
        team_dir, _how = IDENTITY.resolve_teams_dir(session_id)
    except Exception:
        team_dir = ""
if not team_dir:
    base = (os.environ.get("INFLIGHT_TEAMS_DIR")
            or os.environ.get("WORKER_EVENTS_TEAMS_DIR")
            or os.path.join(home, ".claude", "teams"))
    candidate = os.path.join(base, "session-%s" % session_id[:8]) if session_id else ""
    team_dir = candidate if candidate and os.path.isdir(candidate) else ""

log_path = (
    os.path.join(team_dir, "inflight-notices.jsonl")
    if team_dir
    else os.path.join(home, ".claude", "inflight-notices.jsonl")
)

# The recipient, resolved to an agent id at the moment the send happened.
to_agent_id = ""
to_identity_source = ""
if IDENTITY is not None:
    try:
        index = IDENTITY.identity_index(
            team_dir, str(payload.get("transcript_path", "") or ""), session_id)
        if to in (index.get("names") or {}):
            # SendMessage's other legal form: the bare agentId.
            to_agent_id = to
            to_identity_source = "addressed by agent id"
        else:
            to_agent_id, to_identity_source = IDENTITY.agent_id_for_name(to, index)
    except Exception:
        to_agent_id, to_identity_source = "", ""

record = {
    "timestamp": datetime.now(timezone.utc).isoformat(),
    "event": "InflightNotice",
    "source_hook": "PostToolUse[SendMessage]",
    "to": to[:200],
    "to_agent_id": to_agent_id[:64],
    "to_identity_source": to_identity_source[:120],
    "summary": str(tool_input.get("summary", "") or "")[:200],
    "message_kind": kind,
    "message_chars": len(body),
    "body_sha256": hashlib.sha256(body.encode("utf-8")).hexdigest() if body else "",
    "sha_tokens": shas,
    "session_id": session_id,
    "cwd": str(payload.get("cwd", "") or "")[:400],
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
