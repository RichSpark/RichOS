#!/usr/bin/env bash
#
# guard-sealed-worktree.sh — THE LOCK-OUT. Matcherless PreToolUse guard,
# registered FIRST, before every tool-specific hook.
#
# ===========================================================================
# WHAT IT DECIDES — docs/plans/worktree-spec-2026-09-11.md, points 9, 11, 3
# ===========================================================================
#   "a finished agent never writes again. The platform restarts finished
#    agents (14 times observed). A restarted agent is refused every tool, so
#    it cannot write anywhere, including after its workspace is gone. This
#    lock-out already exists and stays."                          (point 9)
#
# FINISHED is the page's definition and nothing else (points 11 and 12), read
# from mega-lander/workspaces.py: the platform's own end-of-run signal is
# recorded and Rich had not paused the agent before it — or it ended after
# handing in its work — or its session has ended (it recorded its end, or its
# process no longer exists, read from the operating system). A PAUSED agent is
# not finished: it keeps its workspaces, is not locked out, and resumes.
#
#   FINISHED worker            -> every tool refused, Read included, whatever
#                                 its agent_type (a type exemption says it owns
#                                 no workspace, not that it may return)
#   registered, not finished   -> passes (paused included)
#   UNREGISTERED worker        -> "If registration fails, the spawn does not
#                                 happen" (point 3). A worker's registration is
#                                 written by PreToolUse[Agent], SubagentStart
#                                 and PostToolUse[Agent]; this call waits
#                                 SEAL_WAIT_SECONDS for it, then refuses every
#                                 potentially writing tool. Read-only tools pass
#                                 so it can say why it stopped.
#   READ-ONLY AGENT TYPES      -> own no workspace; pass unless finished
#   the lead of a claude --worktree session
#                              -> "no one is allowed to do that" (point 3):
#                                 every tool but reading and STOPPING ITS
#                                 AGENTS (TaskStop, the engine's stop.sh) is
#                                 refused. Which directory the session was
#                                 launched in is the platform's own record,
#                                 never a later SessionStart: a SUBAGENT's
#                                 compaction fires one with the lead's
#                                 session_id (2026-09-22, 33 minutes locked out)
#   the lead, otherwise        -> passes
#
# How it tells the lead from a worker: the hooks reference documents that tool
# events fired inside a subagent carry `agent_id`, "present only when the hook
# fires inside a subagent call". A payload that PARSES and carries no agent_id
# is the lead's own call.
#
# FAIL CLOSED for a worker, on an unregistered worker and on its own error
# alike: when the barrier cannot evaluate a worker's call (python3 missing, the
# library missing, the root unresolvable, the payload unparseable, the resolver
# raising), a potentially writing or unknown tool is DENIED (exit 2) and a
# read-only tool passes. WITHOUT python3 the lead is proven only by the
# ABSENCE of an unescaped "agent_id" key in the raw JSON.
#
# NOTE: hooks are snapshotted at session start. This guard is inert until the
# next session and assumes nothing about being live in the session that adds it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail
# Payloads can exceed a pipe buffer. Matching pipelines must consume EOF:
# grep -q or head can give the producer SIGPIPE and reverse the verdict.

INPUT="$(cat)"
HOOK_TAG="(hook: scripts/hooks/guard-sealed-worktree.sh)"
: "${SEAL_READONLY_TOOLS_FALLBACK:=Read Glob Grep LS WebFetch WebSearch ListAgents TaskList TaskGet TodoRead ToolSearch}"

deny_cannot_evaluate() { # <reason> [tool] [agent]
    {
        echo "=== Lock-out: REFUSED (the barrier cannot evaluate this call) ==="
        echo "  tool: ${2:-<unknown>}    agent: ${3:-<unknown>}"
        echo "  reason: $1"
        echo ""
        echo "  This call comes from a worker (or from a payload the barrier cannot prove"
        echo "  is the lead's), and the barrier could not read the worker's registration."
        echo "  A potentially writing or unknown tool is refused rather than allowed on a"
        echo "  guess. Read-only tools (${SEAL_READONLY_TOOLS:-$SEAL_READONLY_TOOLS_FALLBACK}) still pass."
        echo "  FIX THE ENGINE: restore the named dependency (scripts/hooks/install.sh)."
        echo "$HOOK_TAG"
    } >&2
    exit 2
}

raw_has_agent_id() { # true when the raw payload carries an unescaped "agent_id" key
    printf '%s' "$INPUT" | grep -E >/dev/null '(^|[^\\])"agent_id"'
}
raw_tool_name() {
    printf '%s' "$INPUT" | sed -n -E 's/.*"tool_name"[[:space:]]*:[[:space:]]*"([^"]*)".*/\1/p' | sed -n '1p'
}
is_readonly_tool() { # <tool> <list>
    local t; for t in $2; do [ "$t" = "$1" ] && return 0; done; return 1
}

if ! command -v python3 >/dev/null 2>&1; then
    if ! raw_has_agent_id; then
        echo "NOTICE: guard-sealed-worktree.sh: python3 is unavailable; this payload carries no agent_id and is treated as the lead's own call (allowed). Every WORKER write is refused until python3 is restored." >&2
        exit 0
    fi
    RAW_TOOL="$(raw_tool_name)"
    if [ -n "$RAW_TOOL" ] && is_readonly_tool "$RAW_TOOL" "$SEAL_READONLY_TOOLS_FALLBACK"; then
        echo "NOTICE: guard-sealed-worktree.sh: python3 is unavailable; $RAW_TOOL is on the read-only allowlist and is allowed under the read-only policy. Every potentially writing tool is refused until python3 is restored." >&2
        exit 0
    fi
    deny_cannot_evaluate "python3 is unavailable, so the worker's registration cannot be read" "${RAW_TOOL:-<unknown>}" "<unparsed>"
fi

# PARSED: LEAD / WORKER:<agent_id> / UNPARSEABLE. Only a parsed payload with
# no agent_id is the lead; an unparseable one is NOT.
PARSED="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d = json.load(sys.stdin)
except Exception as e:
    print("UNPARSEABLE\t%s" % str(e).replace("\t"," ")); raise SystemExit(0)
if not isinstance(d, dict):
    print("UNPARSEABLE\tpayload is not a JSON object"); raise SystemExit(0)
aid = str(d.get("agent_id") or "")
tool = str(d.get("tool_name") or "")
atype = str(d.get("agent_type") or "")
print(("WORKER\t%s\t%s\t%s" % (aid, tool, atype)) if aid else "LEAD\t\t%s\t" % tool)' 2>/dev/null || true)"
PKIND="$(printf '%s' "$PARSED" | sed -n '1p' | cut -f1)"
AGENT_ID="$(printf '%s' "$PARSED" | sed -n '1p' | cut -f2)"
TOOL_NAME="$(printf '%s' "$PARSED" | sed -n '1p' | cut -f3)"
AGENT_TYPE="$(printf '%s' "$PARSED" | sed -n '1p' | cut -f4)"
case "$PKIND" in
  LEAD|WORKER) : ;;
  *)
    RAW_TOOL="$(raw_tool_name)"
    if [ -n "$RAW_TOOL" ] && is_readonly_tool "$RAW_TOOL" "$SEAL_READONLY_TOOLS_FALLBACK"; then
        exit 0
    fi
    deny_cannot_evaluate "the hook payload is unparseable (${PARSED#*	}); a payload that cannot be proven the lead's is treated as a worker's" "${RAW_TOOL:-<unknown>}" "<unparsed>" ;;
esac

# FAIL CLOSED BEFORE THE SHARED BOOTSTRAP: the block below is byte-identical
# in every rooted hook (probe Layer R compares them), and its own answer to a
# missing resolver is a banner and an exit — right for a notice hook, wrong for
# a lock-out. So the barrier decides first: with the resolver missing, the
# lead passes, a worker's read-only tool passes and everything else is refused.
_PRE_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ ! -f "$_PRE_SCRIPT_DIR/../lib/resolve-roots.sh" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-sealed-worktree.sh"
        echo "  scripts/lib/resolve-roots.sh is missing at: $_PRE_SCRIPT_DIR/../lib/resolve-roots.sh"
    } >&2
    [ "$PKIND" = "LEAD" ] && exit 0
    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS_FALLBACK" && exit 0
    deny_cannot_evaluate "scripts/lib/resolve-roots.sh is missing at $_PRE_SCRIPT_DIR/../lib/resolve-roots.sh" "$TOOL_NAME" "$AGENT_ID"
fi

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
  hook: scripts/hooks/guard-sealed-worktree.sh
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

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # No engine here, no workspace contract: stand down. A RESOLVED state.
    exit 0
else
    root_failure_banner "scripts/hooks/guard-sealed-worktree.sh" >&2
    [ "$PKIND" = "LEAD" ] && exit 0
    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS_FALLBACK" && exit 0
    deny_cannot_evaluate "the governed root could not be resolved (${RICHOS_ROOT_REASON:-no reason given})" "$TOOL_NAME" "$AGENT_ID"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${READONLY_ALLOWLIST:=Explore Plan claude-code-guide statusline-setup}"
: "${HARNESS_UTILITY_TYPES:=claude-code-guide statusline-setup output-style-setup}"
: "${SEAL_READONLY_TOOLS:=$SEAL_READONLY_TOOLS_FALLBACK}"
: "${SEAL_WAIT_SECONDS:=5}"

WS_PY="$SCRIPT_DIR/../../mega-lander/workspaces.py"
if [ ! -f "$WS_PY" ]; then
    [ "$PKIND" = "LEAD" ] && exit 0
    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS" && exit 0
    deny_cannot_evaluate "mega-lander/workspaces.py is missing at $WS_PY" "$TOOL_NAME" "$AGENT_ID"
fi

# The payload travels on stdin, never in the environment: a large payload
# must not skip the finished check.
_verdict() {
    printf '%s' "$INPUT" | python3 "$WS_PY" --entity "$ENTITY_ROOT" barrier 2>/dev/null \
        || printf 'ERROR\tthe barrier resolver could not run\n'
}
VERDICT="$(_verdict)"
KIND="$(printf '%s' "$VERDICT" | sed -n '1p' | cut -f1)"
DETAIL="$(printf '%s' "$VERDICT" | sed -n '1p' | cut -f2-)"

if [ "$KIND" = "UNREGISTERED" ]; then
    # A plugin-namespaced type ("richos-engine:clark") is the same type.
    BARE_TYPE="${AGENT_TYPE##*:}"
    for t in $READONLY_ALLOWLIST $HARNESS_UTILITY_TYPES; do
        [ -n "$BARE_TYPE" ] && [ "$BARE_TYPE" = "$t" ] && exit 0
    done
    # The registration may still be being written by the spawn's own hooks.
    _deadline=$(( $(date +%s) + ${SEAL_WAIT_SECONDS%.*} ))
    while [ "$KIND" = "UNREGISTERED" ] && [ "$(date +%s)" -lt "$_deadline" ]; do
        sleep 0.25
        VERDICT="$(_verdict)"
        KIND="$(printf '%s' "$VERDICT" | sed -n '1p' | cut -f1)"
        DETAIL="$(printf '%s' "$VERDICT" | sed -n '1p' | cut -f2-)"
    done
fi

case "$KIND" in
  LEAD)
    if [ -n "$DETAIL" ] && [ -z "${RICHOS_APP_STATE:-}" ]; then
        printf '%s' "$DETAIL" | python3 -c 'import json,sys; print(json.dumps({"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":sys.stdin.read()}}))'
    fi
    exit 0 ;;
  REGISTERED)
    exit 0 ;;
  RECOVERY)
    # A refused lead stopping its agents: TaskStop, or the engine's own stop.sh /
    # stop-work-ack.sh as one plain command (workspaces.py lead_recovery_call).
    # Never refused: a lead that cannot stop its agents is how 2026-09-22 went.
    exit 0 ;;
  HANDBACK)
    # A finished agent's SubagentHandback, when its only ending is its own run's
    # end (workspaces.py _hands_back_after_its_run). The report writes nothing
    # anywhere, which is point 9's whole reason; every other tool of that agent
    # is still refused. Walk 6, 2026-10-05: a reviewer's verdict refused 3 times.
    exit 0 ;;
  FINISHED)
    {
      echo "=== Lock-out: REFUSED (finished agent) ==="
      echo "  $DETAIL."
      echo "  A finished agent never writes again (docs/plans/worktree-spec-2026-09-11.md,"
      echo "  point 9). Every tool is refused, reading included. Nothing to do: the work"
      echo "  is over, and the orchestrator lands or discards it."
      echo "$HOOK_TAG"
    } >&2
    exit 2 ;;
  CONTROLLED)
    echo "=== Recovery control: REFUSED === $DETAIL" >&2
    exit 2 ;;
  READONLY)
    echo "=== Read-only assignment: REFUSED === $DETAIL $HOOK_TAG" >&2
    exit 2 ;;
  CODEX)
    {
      echo "=== A codex/ workspace: REFUSED ==="
      echo "  $DETAIL."
      echo "  An agent never works inside a codex/ workspace; it works from a copy in its"
      echo "  own cc/ workspace (docs/plans/worktree-spec-2026-09-11.md, point 2). Every"
      echo "  writing tool aimed inside one is refused; the lead's own calls are not."
      echo "$HOOK_TAG"
    } >&2
    exit 2 ;;
  FORBIDDEN)
    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS" && exit 0
    {
      echo "=== This session is not allowed: REFUSED ==="
      echo "  $DETAIL."
      echo "  Nobody starts a session in its own workspace (claude --worktree / claude -w)"
      echo "  in RichOS; it is not allowed (docs/plans/worktree-spec-2026-09-11.md, point 3)."
      echo "  DO NOT RETRY THIS CALL: it is refused every time, however it is worded."
      echo "  The one thing this session can still do is STOP ITS AGENTS (and read):"
      echo "    First prepare the acknowledgement, then make the printed TaskStop calls:"
      echo "    $ENGINE_ROOT/scripts/stop.sh <name> [<name> ...] --ceo-word '<his words>'"
      echo "  Then tell the CEO once that the session must be restarted from the"
      echo "  repository's main checkout, and end the turn."
      echo "$HOOK_TAG"
    } >&2
    exit 2 ;;
  UNREGISTERED)
    is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS" && exit 0
    {
      echo "=== Lock-out: REFUSED (no registration) ==="
      echo "  tool: ${TOOL_NAME:-<unknown>}    agent: $AGENT_ID"
      echo "  $DETAIL, after waiting ${SEAL_WAIT_SECONDS}s for its spawn to register it."
      echo "  If registration fails, the spawn does not happen (point 3): a worker with"
      echo "  no registration may read and report, and nothing else. Report it and stop."
      echo "$HOOK_TAG"
    } >&2
    exit 2 ;;
  *)
    if [ "$PKIND" = "LEAD" ]; then
        echo "NOTICE: guard-sealed-worktree.sh could not evaluate the lead's call (${DETAIL:-$KIND}); allowed. Fix the engine." >&2
        exit 0
    fi
    if is_readonly_tool "$TOOL_NAME" "$SEAL_READONLY_TOOLS"; then
        echo "NOTICE: guard-sealed-worktree.sh could not evaluate ($DETAIL); $TOOL_NAME is allowed under the read-only policy. Fix the engine." >&2
        exit 0
    fi
    deny_cannot_evaluate "${DETAIL:-unexpected verdict '$KIND'}" "$TOOL_NAME" "$AGENT_ID" ;;
esac
