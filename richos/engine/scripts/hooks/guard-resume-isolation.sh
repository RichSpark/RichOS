#!/usr/bin/env bash
#
# guard-resume-isolation.sh — BLOCKING PreToolUse guard on the SendMessage
# tool. Closes the "resume bypasses worktree isolation" hole.
#
# THE FAILURE MODE:
#   A SendMessage to a *completed* teammate RESUMES it from its transcript.
#   The spawn-side guard (guard-worktree-isolation.sh) only fires on
#   PreToolUse[Agent] SPAWNS — it never sees a resume. So if the resumed
#   teammate's worktree was already landed and REMOVED (the single-writer land
#   sequence removes a teammate's worktree after shutdown), the agent wakes with
#   NO isolated workspace and improvises: today a hand-rolled worktree
#   (tolerable), tomorrow main-checkout writes or lost work. This guard makes
#   that IMPOSSIBLE by default: a resume that cannot be proven to land in a live
#   workspace is blocked, with the two sanctioned paths spelled out inline.
#
# THE CONTRACT (default-deny, fail closed, one auditable escape hatch):
#   1. Recipient is a currently-ACTIVE teammate — positive liveness: present in
#      the session team roster (config.json members[]) with a non-terminal
#      status, OR owning an existing (present) native worktree (its cwd under
#      .claude/worktrees/agent-* still on disk) -> ALLOW silently. An active
#      agent trips NEITHER block path because one of those two signals always
#      holds for it (see resolver below).
#   2. Recipient would RESUME a non-active/completed teammate (roster status
#      terminal AND/OR its worktree already removed) -> BLOCK, unless the
#      message carries an explicit  resume-ack: <where writes land + why safe>
#      live line (the deliberate escape hatch for pure-question follow-ups or
#      serialized external-repo writers). resume-ack: -> ALLOW + append the ack
#      line + recipient + timestamp to .claude/state/resume-acks.log.
#   3. Recipient resolution failure or ambiguity -> BLOCK (fail closed).
#   4. NEVER blocked: the lead / reply channel (to == main / team-lead / lead /
#      user / empty) and PROTOCOL messages (shutdown / plan-approval / permission
#      control traffic — detected by a non-string `message` structured body or an
#      explicit protocol type/token).
#
# SendMessage PreToolUse payload:
#   { "tool_name": "SendMessage",
#     "tool_input": { "to": "<name-or-agentId>",
#                     "message": "<text>" | {<structured protocol object>},
#                     "summary": "<preview>", "messageType"?: "<type>" },
#     "session_id": "<uuid>" }
# `to` is matched against the same roster the tool resolves against, and we
# handle BOTH the plain-name and raw-agentId addressing forms.
#
# HARNESS-VERSION NOTE: the roster/member schema and the "cwd under
# .claude/worktrees/" liveness signal are harness-version-coupled. This guard
# assumes the schema documented in orchestration.config (SESSION_TEAMS_DIR). If
# a future harness changes it, update the resolver below — the guard fails
# CLOSED, so a schema drift blocks (with resume-ack: as the override) rather
# than silently waving resumes through.
#
# FAIL-CLOSED: python3 unavailable, an unparseable payload, or an
# unreadable/absent team config for a specific-teammate recipient are all
# blocked (never waved through) — with resume-ack: available as the operator
# override where a deliberate resume is genuinely safe.
#
# NOTE: hooks are snapshotted per session; this guard takes effect from the
# NEXT session. It assumes nothing about being live in the session that adds it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail
# Payloads can exceed a pipe buffer. Matching pipelines must consume EOF:
# grep -q or head can give the producer SIGPIPE and reverse the verdict.

# Fail-closed, not fail-open: every payload check below depends on python3 to
# parse the tool_input JSON. If python3 is missing, refuse outright rather than
# wave the resume through unchecked (which would contradict the default-deny
# contract). resume-ack: remains the operator override for a genuinely safe
# resume.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-resume-isolation.sh: python3 is required for payload parsing — refusing (fail-closed). If this is a deliberate, safe resume, add a 'resume-ack: <where writes land + why safe>' message line." >&2; exit 2; }

# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/guard-resume-isolation.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

INPUT="$(cat)"

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine, so there is no enforcement to
    # lose here. Stand down. NOT a silent skip: engine-status.sh announces the
    # stand-down into the orchestrator's own context at every session start.
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/guard-resume-isolation.sh" >&2
    exit 2
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
# Default to the platform team-state location when unset/blank in config.
: "${SESSION_TEAMS_DIR:=$HOME/.claude/teams}"

HOOK_TAG="(hook: scripts/hooks/guard-resume-isolation.sh)"

# Single combined parse: tool-name gate + full field extraction in ONE python
# pass, so an unparseable payload can NEVER slip past a swallowed error on a
# separate earlier extraction (the old two-step form failed OPEN here: a garbage
# top-level payload defaulted tool_name to "" and exited 0 before this
# fail-closed block was ever reached). Now every outcome is explicit:
#   - top-level JSON unparseable / not an object / tool_input not an object
#       -> PARSEFAIL  (fail CLOSED, block — matches the header's fail-closed claim)
#   - valid payload for a DIFFERENT tool (tool_name != SendMessage)
#       -> NOTSENDMSG (pass through — the matcher semantics are preserved)
#   - valid SendMessage payload
#       -> OK + fields
# Message text has newlines preserved via a \001 placeholder so the resume-ack:
# marker can be matched with a line-start anchor (same discipline
# guard-worktree-isolation.sh uses for main-checkout-run:). MSG_KIND is "string"
# when message is plain text and "object" when it is a structured protocol body
# ("none" when absent).
PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    print("PARSEFAIL\t\t\t\t\t\t")
    sys.exit(0)
if not isinstance(d, dict):
    print("PARSEFAIL\t\t\t\t\t\t")
    sys.exit(0)
tool_name = str(d.get("tool_name", "") or "")
if tool_name != "SendMessage":
    # A well-formed payload for a different tool is not our concern.
    print("NOTSENDMSG\t\t\t\t\t\t")
    sys.exit(0)
ti = d.get("tool_input", {})
if not isinstance(ti, dict):
    print("PARSEFAIL\t\t\t\t\t\t")
    sys.exit(0)
to = str(ti.get("to", "") or "")
msg = ti.get("message", None)
if isinstance(msg, str):
    kind = "string"
    text = msg
elif msg is None:
    kind = "none"
    text = ""
else:
    # structured / non-string message body = protocol/control traffic.
    kind = "object"
    try:
        text = json.dumps(msg)
    except Exception:
        text = ""
mt = ti.get("messageType") or ti.get("message_type") or ti.get("type") or ""
mt = str(mt or "")
sid = str(d.get("session_id", "") or "")
# The lead transcript, carried on every hook payload. It holds the ONLY complete
# name -> agent id join (Agent tool_use -> toolUseResult.agentId), which is how a
# recipient NAME reaches the authoritative liveness signal below. NO APOSTROPHES
# IN THIS BLOCK: it is a single-quoted shell string, and one would end it.
tpath = str(d.get("transcript_path", "") or "").replace("\t", " ").replace("\n", " ")
text = text.replace("\t", " ").replace("\x01", " ").replace("\n", "\x01")
print("OK\t%s\t%s\t%s\t%s\t%s\t%s" % (to, kind, mt, sid, tpath, text))
' 2>/dev/null || printf 'PARSEFAIL\t\t\t\t\t\t')"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
TO="$(printf '%s' "$PARSED" | cut -f2)"
MSG_KIND="$(printf '%s' "$PARSED" | cut -f3)"
MSG_TYPE="$(printf '%s' "$PARSED" | cut -f4)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f5)"
TRANSCRIPT="$(printf '%s' "$PARSED" | cut -f6)"
MESSAGE="$(printf '%s' "$PARSED" | cut -f7- | tr '\001' '\n')"

# Not a SendMessage payload — a well-formed event for a different tool passes
# through untouched (preserving the PreToolUse[SendMessage] matcher semantics).
if [ "$STATUS" = "NOTSENDMSG" ]; then
  exit 0
fi

# Fail-closed on an unparseable SendMessage payload.
if [ "$STATUS" = "PARSEFAIL" ]; then
  {
    echo "=== Resume-isolation guard: BLOCKED ==="
    echo "  - Could not parse the SendMessage payload to verify the recipient's"
    echo "    liveness. Failing closed. Re-issue a well-formed SendMessage; if this"
    echo "    is a deliberate, safe resume, add a 'resume-ack: <where writes land +"
    echo "    why safe>' line to the message body."
    echo "$HOOK_TAG"
  } >&2
  exit 2
fi

# Pause is an orchestrator control message. Validate it before the liveness
# fast path, protocol exemption or resume-ack can allow delivery.
if ! printf '%s' "$INPUT" | python3 "$ENGINE_ROOT/scripts/lib/pause_protocol.py" --check; then
  exit 2
fi

# --- (4a) lead / reply channel — NEVER blocked ---------------------------
# A message to the lead/reply channel cannot resume a specific completed
# teammate, so it is never the failure mode. Also covers the background-agent
# -> lead reply channel.
TO_LC="$(printf '%s' "$TO" | tr '[:upper:]' '[:lower:]')"
case "$TO_LC" in
  main|team-lead|lead|leader|user|"") exit 0 ;;
esac

# --- (0) A FINISHED AGENT IS REFUSED BEFORE EVERY ESCAPE HATCH ------------
#
# docs/plans/worktree-spec-2026-09-11.md, points 9 and 11: "a finished agent
# never writes again"; "An agent Rich pauses ... is not finished: it keeps its
# workspaces, is not locked out, and resumes." Both come from ONE record — the
# workspace registry (mega-lander/workspaces.py), keyed by this session and the
# teammate's name (names are unique within a session by clause 3 of the spawn
# guard). So:
#
#   finished      -> REFUSED here, before the protocol exemption and before
#                    resume-ack:. The lock-out (guard-sealed-worktree.sh) would
#                    refuse its every tool anyway; a message would only restart
#                    an agent that can do nothing.
#   paused/active -> ALLOWED here: a registered agent that is not finished may
#                    be messaged, and a message to a paused one resumes it
#                    (workspace-lifecycle.sh records that on PostToolUse).
#                    Nothing further is asked — no liveness question (the page:
#                    "No question of whether an agent is still alive").
#   unregistered  -> not a workspace-owning teammate of this session; the checks
#                    below decide, unchanged.
_WS_PY="$SCRIPT_DIR/../../mega-lander/workspaces.py"
if [ -f "$_WS_PY" ] && [ -n "$SESSION_ID" ]; then
  WS_VERDICT="$(python3 "$_WS_PY" --session "$SESSION_ID" recipient --name "$TO" 2>/dev/null || printf 'unknown\tthe registry could not be read')"
  WKIND="$(printf '%s' "$WS_VERDICT" | sed -n '1p' | cut -f1)"
  WDETAIL="$(printf '%s' "$WS_VERDICT" | sed -n '1p' | cut -f2-)"
  case "$WKIND" in
    finished)
      {
        echo "=== Resume-isolation guard: REFUSED (finished agent) ==="
        echo "  SendMessage to '${TO}': it is finished — ${WDETAIL}."
        echo ""
        echo "  A finished agent never writes again (point 9); there is NO escape hatch —"
        echo "  not resume-ack:, not a protocol message. Its work is landed or discarded"
        echo "  (workspaces.sh land|discard ${TO}). To continue unfinished work, spawn a"
        echo "  FRESH teammate with a 'continues: ${TO}' prompt line (point 7)."
        echo "$HOOK_TAG"
      } >&2
      exit 2 ;;
    paused|active)
      exit 0 ;;
    unknown)
      echo "NOTICE: guard-resume-isolation.sh: the workspace registry could not be read ($WDETAIL); continuing to the checks below." >&2 ;;
  esac
fi

# --- (4b) protocol / control messages — NEVER blocked --------------------
# Three independent tells, any of which marks control traffic:
#   - a structured (non-string) message body,
#   - an explicit messageType/type on one of the protocol control types,
#   - a JSON body carrying a quoted protocol type token (quoted so ordinary
#     prose that merely mentions "shutdown" never false-matches).
PROTO_TYPES_RE='shutdown_request|shutdown_response|shutdown_approved|plan_approval_request|plan_approval_response|permission_request|permission_response|sandbox_permission_request|sandbox_permission_response|team_permission_update|mode_set_request'
if [ "$MSG_KIND" = "object" ]; then
  exit 0
fi
if printf '%s' "$MSG_TYPE" | grep -iE >/dev/null "^(${PROTO_TYPES_RE})$"; then
  exit 0
fi
if printf '%s' "$MESSAGE" | grep -E >/dev/null "\"(${PROTO_TYPES_RE})\""; then
  exit 0
fi

# --- (2b) resume-ack: deliberate-safe-resume escape hatch ----------------
# A live line beginning with resume-ack: sanctions the resume. Best-effort by
# design (this is an auditable opt-out, not a security boundary). Logged to
# .claude/state/resume-acks.log, mirroring main-checkout-runs.log.
RESUME_ACK=""
if printf '%s' "$MESSAGE" | grep -E >/dev/null '^[[:space:]]*resume-ack:[[:space:]]*.+'; then
  RESUME_ACK="$(printf '%s' "$MESSAGE" | grep -oE '^[[:space:]]*resume-ack:[[:space:]]*.+' | sed -n '1p' | sed -E 's/^[[:space:]]*//')"
fi
if [ -n "$RESUME_ACK" ]; then
  LOG_DIR="$ENTITY_ROOT/.claude/state"
  LOG_FILE="$LOG_DIR/resume-acks.log"
  mkdir -p "$LOG_DIR" 2>/dev/null || true
  # De-dup guard: Claude Code merges hooks from BOTH .claude/settings.json
  # (generated by scripts/hooks/install.sh) AND .claude/settings.local.json (the
  # committed source) — both register this PreToolUse[SendMessage] hook, so it
  # fires TWICE per delivery and would append two byte-identical lines (the
  # sibling guard-worktree-isolation.sh doubles its own append logs the same
  # way). Collapse the second write by comparing the RECIPIENT + ACK-CONTENT
  # ONLY against the immediately-preceding logged line — the timestamp is NEVER
  # part of the equality key, it is written for display only.
  #
  # WHY THE TIMESTAMP IS EXCLUDED (the fix): the earlier form embedded a
  # SECOND-granularity UTC timestamp in the equality key and relied on both
  # double-fire invocations landing in the same wall-clock second. On a colder /
  # loaded runner the two sequential subprocess spawns can straddle a UTC-second
  # boundary, the timestamps differ, the string-equality misses, and BOTH lines
  # get appended — a real, reproducible timing race (the automation QA's live-fire CI autopsy).
  # Keying on recipient+ack alone removes the clock-boundary race entirely: a
  # same-delivery double-fire always has identical recipient+ack and collapses;
  # two GENUINELY separate resumes differ in recipient and/or ack text and are
  # both kept. Hooks for one tool call run sequentially, so no lock is needed.
  DEDUP_KEY="$(printf 'to=%s\t%s' "${TO:-<unset>}" "$RESUME_ACK")"
  NEW_LINE="$(printf '%s\t%s' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$DEDUP_KEY")"
  LAST_KEY=""
  if [ -f "$LOG_FILE" ]; then
    # Strip the leading timestamp field (field 1) from the last logged line to
    # recover its dedup key (fields 2+: "to=<recipient>\t<ack>").
    LAST_KEY="$(tail -n 1 "$LOG_FILE" 2>/dev/null | cut -f2- || true)"
  fi
  if [ "$DEDUP_KEY" != "$LAST_KEY" ]; then
    printf '%s\n' "$NEW_LINE" >>"$LOG_FILE" 2>/dev/null || true
  fi
  exit 0
fi

# --- (1/2/3) LIVENESS RESOLUTION — TWO SOURCES, ONE OF THEM AUTHORITATIVE
#
# THE DEFECT THIS SECTION EXISTS TO CLOSE — measured 2026-09-02
# ---------------------------------------------------------------------------
# The lead sent in-flight notices to three agents that scripts/agent-liveness.sh
# had confirmed ALIVE seconds earlier by the authoritative signal (isolation
# worktree LOCKED, locking pid running). This guard refused all three — by
# teammate NAME and by raw agentId — with "could not be confirmed as an ACTIVE
# teammate (not in session roster)".
#
# It was not a bug in the matching. It was the SOURCE. This guard decided
# "active" from the session team roster, and CLAUDE.md documents that background
# native-isolation agents NEVER appear in that roster: "Background
# native-isolation agents don't appear in the SendMessage roster — absence !=
# terminated." So the roster cannot distinguish "completed and cleaned up" from
# "running but invisible", and this guard failed closed on both. Every notice to
# a live background agent had to be waived — not occasionally, STRUCTURALLY,
# every single time. A guard that must be overridden every time it fires is a
# guard nobody reads, and the override becomes reflex.
#
# THE PROTECTION IS UNCHANGED AND MUST STAY THAT WAY. A SendMessage to a
# genuinely completed teammate resumes it from its transcript; if its worktree
# was already landed and REMOVED it wakes with no workspace and improvises —
# main-checkout writes, lost work. That is still blocked, and the two-sided
# canaries in guard-resume-isolation.test.sh prove both directions at once.
#
# THE FIX: consult the AUTHORITATIVE liveness signal before refusing.
#   roster  — cheap, advisory, checked FIRST because a hit is free and covers
#             every in-process teammate. An ACTIVE roster answer allows, exactly
#             as before. It is never trusted to REFUSE on its own.
#   lock    — scripts/lib/agent-liveness.py, THE ONE implementation of "is this
#             agent alive?", consulted on the refusal path only. It already
#             returns ALIVE / NOT-ALIVE / INDETERMINATE with its evidence, and a
#             second implementation here is how one of the two silently becomes
#             the stale one. Read its docstring: the lock pid is the HOST
#             SESSION's pid, shared across every agent of that session, so the
#             lock's PRESENCE is the per-agent signal and the pid check is the
#             stale-lock filter.
#
# INDETERMINATE IS NOT "ALLOW". It stays a real outcome and it keeps requiring
# the ack — a resolver that collapses "I could not tell" into "fine, go ahead"
# is the failure this whole engine keeps finding in itself.
#
# WHY A LOCKED, PRESENT WORKTREE IS NOT A WEAKENING: this guard has ALWAYS
# treated a present worktree as positive liveness regardless of a stale roster
# status (see wt_present() below — "A present worktree is positive liveness").
# The lock is a strictly STRONGER signal than mere directory presence. The only
# thing that changes is that an agent the roster never listed can now produce
# that evidence too.
#
# Resolve the team config the SAME way the SendMessage tool resolves `to`: the
# session team dir under SESSION_TEAMS_DIR/session-<first8>. Tests override the
# teams dir via RESUME_GUARD_TEAMS_DIR. EXACT session match only (never a
# single-dir fallback) so a mismatched/absent session never resolves to an
# unrelated team.
TEAMS_DIR="${RESUME_GUARD_TEAMS_DIR:-$SESSION_TEAMS_DIR}"
TEAM_CONFIG=""
TEAM_DIR=""
if [ -n "$SESSION_ID" ]; then
  TEAM_DIR="$TEAMS_DIR/session-$(printf '%s' "$SESSION_ID" | cut -c1-8)"
  TEAM_CONFIG="$TEAM_DIR/config.json"
fi

# The agent worktrees this guard reasons about are registered in the ENTITY'S
# MAIN CHECKOUT, never in whatever worktree a caller happens to stand in. One
# resolver, shared (scripts/lib/resolve-main-checkout.sh, sourced by
# resolve-roots.sh) — a local re-derivation here is the copy that goes stale.
MAIN_CHECKOUT="$ENTITY_ROOT"
if command -v resolve_main_checkout >/dev/null 2>&1; then
  MAIN_CHECKOUT="$(resolve_main_checkout "$ENTITY_ROOT" "$ENTITY_ROOT" 2>/dev/null || printf '%s' "$ENTITY_ROOT")"
fi

emit_block_completed() { # <detail>
  local detail="$1"
  {
    echo "=== Resume-isolation guard: BLOCKED ==="
    echo "  SendMessage to '${TO}' would RESUME a completed / non-active teammate"
    echo "  (${detail})."
    echo ""
    echo "  A resumed teammate wakes from its transcript, and the spawn-side guard"
    echo "  (guard-worktree-isolation.sh) NEVER fires on a resume — so if this"
    echo "  agent's worktree was already landed and removed, it will improvise in"
    echo "  the wrong place (main-checkout writes / lost work). Default-deny."
    echo ""
    echo "  Two sanctioned paths:"
    echo "    (a) FILE-BEARING follow-up -> do NOT resume. Spawn a FRESH teammate"
    echo "        with the Agent tool and isolation:\"worktree\" (a new, unique"
    echo "        <role>-<identifier> name)."
    echo "    (b) DELIBERATE safe resume (a pure-question follow-up, or a"
    echo "        serialized external-repo writer) -> add a live message line:"
    echo "          resume-ack: <where any writes will land + why this resume is safe>"
    echo "        (allowed + logged to .claude/state/resume-acks.log)."
  } >&2
}

emit_block_unresolvable() { # <detail>
  local detail="$1"
  {
    echo "=== Resume-isolation guard: BLOCKED ==="
    echo "  SendMessage recipient '${TO}' could not be confirmed as an ACTIVE"
    echo "  teammate (${detail}). Failing closed: a resume that can't be proven to"
    echo "  land in a live isolated workspace is denied."
    echo ""
    echo "  Fix one of:"
    echo "    - If you meant a live teammate, re-send with its EXACT current name"
    echo "      or agentId (disambiguate — do not rely on a partial/prefix name)."
    echo "    - For a FILE-BEARING follow-up, spawn a FRESH teammate with the Agent"
    echo "      tool and isolation:\"worktree\" (a new <role>-<identifier> name)."
    echo "    - For a DELIBERATE safe resume (pure question, or a serialized"
    echo "      external-repo writer), add a live message line:"
    echo "        resume-ack: <where any writes will land + why this resume is safe>"
    echo "      (allowed + logged to .claude/state/resume-acks.log)."
  } >&2
}

# emit_liveness_evidence — WHAT THE AUTHORITATIVE SOURCE SAID, on every refusal.
# Naming the disagreement is half the job (agent-liveness.py's own words): a
# refusal that only quotes the roster is the 2026-09-02 defect wearing a nicer
# error message.
emit_liveness_evidence() { # <kind> <detail>
  {
    echo ""
    echo "  Authoritative liveness (scripts/lib/agent-liveness.py — the isolation-"
    echo "  worktree lock, NOT the roster): ${1}"
    echo "    ${2}"
    echo "    main checkout swept: ${MAIN_CHECKOUT}"
    if [ "${1}" = "INDETERMINATE" ]; then
      echo "    INDETERMINATE is not 'alive'. The ack is still required."
    fi
  } >&2
}

# --- THE ROSTER READING (cheap, advisory, allows but never refuses alone) ---
# Emits three TAB-separated fields: KIND, detail, and the roster's own cwd for
# the matched member (handed to the authoritative resolver below, so a member
# the roster knows is checked against the worktree the roster itself names).
roster_verdict() {
  if [ -z "$TEAM_CONFIG" ] || [ ! -f "$TEAM_CONFIG" ]; then
    printf 'NOCONFIG\tno readable session team config for session %s\t\n' "'${SESSION_ID:-<unset>}'"
    return 0
  fi
  TO_ARG="$TO" python3 - "$TEAM_CONFIG" <<'PY'
import json, os, sys

recipient = os.environ.get("TO_ARG", "")

def out(kind, detail, cwd=""):
    print("%s\t%s\t%s" % (kind, detail.replace("\t", " "), cwd.replace("\t", " ")))
    raise SystemExit

try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    out("NOCONFIG", "unreadable")

members = data.get("members", []) or []
TERMINAL = {
    "shutdown", "shutdown_request", "shutdown_approved", "completed",
    "complete", "done", "terminated", "dead", "exited", "killed", "removed",
    "gone",
}

def wt_present(m):
    """True/False if the member owns a native worktree cwd (present/gone);
    None for an in-process / non-worktree member (cwd not under
    .claude/worktrees/)."""
    cwd = str(m.get("cwd") or "")
    if "/.claude/worktrees/" in cwd:
        # P3-24: a LEFTOVER directory (git removed the worktree, files remain)
        # has no .git entry; only a real worktree does.
        return os.path.isdir(cwd) and os.path.exists(os.path.join(cwd, ".git"))
    return None

# Exact match on name OR agentId (both addressing forms).
matches = [m for m in members
           if m.get("name") == recipient or m.get("agentId") == recipient]
if not matches:
    # Unambiguous name-prefix match only; >1 is ambiguous (fail closed).
    pfx = [m for m in members
           if isinstance(m.get("name"), str) and m.get("name").startswith(recipient)]
    if len(pfx) == 1:
        matches = pfx
    elif len(pfx) > 1:
        out("AMBIGUOUS", "%d roster names start with '%s'" % (len(pfx), recipient))

if not matches:
    out("NOTFOUND", "not in session roster")

m = matches[0]
status = str(m.get("status") or "").lower()
cwd = str(m.get("cwd") or "")
wt = wt_present(m)

# A present worktree is positive liveness over a stale NON-terminal status.
# It is not over a FINISHED one: a finished teammate's worktree can be
# retained, so it proves a worktree, not a live agent. That case goes on to
# the authoritative reading below (hunt part 3 v3, finding 24).
if wt is True:
    if status in TERMINAL:
        out("TERMINAL", "roster status is '%s' (its worktree %s is still on disk)" % (status, cwd), cwd)
    out("ACTIVE", "worktree present", cwd)
# The worktree once existed (cwd under .claude/worktrees/) and is now gone —
# THE failure state (landed + removed).
if wt is False:
    out("TERMINAL", "its worktree %s was landed + removed" % (cwd or ""), cwd)
# In-process / non-worktree member: liveness is the roster status.
if status in TERMINAL:
    out("TERMINAL", "roster status is '%s'" % status, cwd)
out("ACTIVE", "roster status '%s'" % (status or "present"), cwd)
PY
}

VERDICT="$(roster_verdict 2>/dev/null || printf 'NOCONFIG\tthe roster reader could not run\t\n')"
VKIND="$(printf '%s' "$VERDICT" | cut -f1)"
VDETAIL="$(printf '%s' "$VERDICT" | cut -f2)"
VCWD="$(printf '%s' "$VERDICT" | cut -f3)"

# The roster's ONE decisive power: a positive hit allows. It is never asked to
# refuse by itself again. Written as an `if` rather than an `&&` list because
# `set -e` is on and the two forms are only accidentally equivalent.
if [ "$VKIND" = "ACTIVE" ]; then
  exit 0
fi

# --- THE AUTHORITATIVE READING (the lock) ----------------------------------
# Consulted ONLY on the path that would otherwise refuse — so the common case
# pays nothing, and the expensive, correct answer is taken exactly where the
# wrong one used to be given.
authoritative_liveness() {
  RESUME_LIB_DIR="$SCRIPT_DIR/../lib" \
  RESUME_MAIN_CHECKOUT="$MAIN_CHECKOUT" \
  RESUME_TO="$TO" \
  RESUME_CWD_HINT="$VCWD" \
  RESUME_TEAM_DIR="$TEAM_DIR" \
  RESUME_TRANSCRIPT="$TRANSCRIPT" \
  RESUME_SESSION_ID="$SESSION_ID" \
  RICHOS_LIVENESS_TEAMS_DIR="$TEAMS_DIR" \
  INFLIGHT_TEAMS_DIR="$TEAMS_DIR" \
  python3 - <<'PY' 2>/dev/null || printf 'UNRESOLVED\tthe authoritative liveness resolver could not run\n'
import importlib.util, os, re, sys


def emit(kind, detail):
    sys.stdout.write("%s\t%s\n" % (kind, (detail or "").replace("\t", " ").replace("\n", " ")))
    sys.stdout.flush()
    raise SystemExit(0)


LIB = os.environ.get("RESUME_LIB_DIR", "")


def load(mod, filename):
    """Load a hyphenated module by path. Missing -> None, never a guess."""
    path = os.path.join(LIB, filename)
    if not os.path.isfile(path):
        return None
    spec = importlib.util.spec_from_file_location(mod, path)
    if spec is None or spec.loader is None:
        return None
    m = importlib.util.module_from_spec(spec)
    sys.modules[mod] = m
    spec.loader.exec_module(m)
    return m


try:
    al = load("richos_agent_liveness", "agent-liveness.py")
except Exception as e:
    emit("UNRESOLVED", "agent-liveness.py failed to load: %s" % e)
if al is None:
    emit("UNRESOLVED",
         "agent-liveness.py is missing from %s — that file is THE one "
         "implementation of 'is this agent alive?' and this guard will not "
         "write a second one" % (LIB or "<no lib dir>"))

to = (os.environ.get("RESUME_TO") or "").strip()
root = (os.environ.get("RESUME_MAIN_CHECKOUT") or "").strip()
if not root:
    emit("UNRESOLVED", "no main checkout resolved for the governed repository")

# EVERY EXACT WAY THIS RECIPIENT CAN NAME AN AGENT. No role prefixes, no fuzzy
# matching: `zach-opus-a1` is not `zach`, and three Zachs have run at once. A
# target that cannot be joined exactly is simply not a target.
targets = []
hint = (os.environ.get("RESUME_CWD_HINT") or "").strip()
if hint:
    targets.append((hint, "the roster's own cwd for this recipient"))
if re.match(r"^agent-[A-Za-z0-9_]+$", to):
    targets.append((to, "addressed by agent directory name"))

identity_note = "teammate-identity.py unavailable"
try:
    ti = load("richos_teammate_identity", "teammate-identity.py")
except Exception as e:
    ti = None
    identity_note = "teammate-identity.py failed to load: %s" % e
if ti is not None:
    try:
        index = ti.identity_index(os.environ.get("RESUME_TEAM_DIR", ""),
                                  os.environ.get("RESUME_TRANSCRIPT", ""),
                                  os.environ.get("RESUME_SESSION_ID", ""))
    except Exception as e:
        index = None
        identity_note = "the identity index raised: %s" % e
    if index is not None:
        identity_note = ("; ".join(index.get("found") or [])
                         or "nothing resolved from: %s"
                            % "; ".join(index.get("tried") or ["<none>"]))
        if to in (index.get("names") or {}):
            targets.append((to, "addressed by raw agent id"))
        try:
            aid, how = ti.agent_id_for_name(to, index)
        except Exception:
            aid, how = "", ""
        if aid:
            targets.append((aid, "exact name join via %s" % (how or "the identity index")))

seen = set()
uniq = []
for t, how in targets:
    if t in seen:
        continue
    seen.add(t)
    uniq.append((t, how))

if not uniq:
    emit("UNRESOLVED",
         "nothing joins '%s' to an agent worktree (identity sources: %s)"
         % (to, identity_note))

worst = None
for t, how in uniq:
    try:
        rec = al.resolve(root, t)
    except Exception as e:
        rec = {"verdict": al.INDETERMINATE, "reason": "resolve() raised: %s" % e}
    v = rec.get("verdict")
    line = "%s (%s) — %s" % (t, how, rec.get("reason") or "")
    if v == al.ALIVE:
        emit("ALIVE", line)
    # INDETERMINATE outranks NOT-ALIVE in the report: "I could not tell" is the
    # more honest headline, and both refuse.
    if worst is None or (worst[0] != al.INDETERMINATE and v == al.INDETERMINATE):
        worst = (v, line)

emit(worst[0] if worst[0] in (al.NOT_ALIVE, al.INDETERMINATE) else "UNRESOLVED",
     worst[1])
PY
}

LIVENESS="$(authoritative_liveness)"
LKIND="$(printf '%s' "$LIVENESS" | cut -f1)"
LDETAIL="$(printf '%s' "$LIVENESS" | cut -f2-)"

# THE ONE NEW ALLOW PATH. The roster could not see this teammate; the lock can.
# A live background isolation agent has a workspace by definition — it is
# holding the lock on it — so the failure mode this guard exists to prevent
# cannot occur, and no resume-ack: is owed.
if [ "$LKIND" = "ALIVE" ]; then
  exit 0
fi

case "$VKIND" in
  TERMINAL)
    emit_block_completed "$VDETAIL" ;;
  AMBIGUOUS)
    emit_block_unresolvable "ambiguous: $VDETAIL" ;;
  NOTFOUND)
    emit_block_unresolvable "$VDETAIL" ;;
  NOCONFIG)
    emit_block_unresolvable "team config unreadable: $VDETAIL" ;;
  *)
    # Any unexpected resolver output -> fail closed.
    emit_block_unresolvable "recipient liveness indeterminate" ;;
esac
emit_liveness_evidence "$LKIND" "$LDETAIL"
echo "$HOOK_TAG" >&2
exit 2
