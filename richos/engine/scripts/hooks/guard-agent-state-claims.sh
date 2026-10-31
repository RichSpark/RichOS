#!/usr/bin/env bash
#
# guard-agent-state-claims.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END
#                               QUIETLY WHILE IT CLAIMS A NAMED AGENT IS
#                               FINISHED AND THAT AGENT'S LOCK IS HELD.
#
# The incident, the corpus, the measured constructions, the constructions
# REFUSED and the reason this reports rather than blocks are all in
# guard-agent-state-claims.py, which is the analysis half. Read that first.
# This file is the wiring: resolve the repository, run the check, say the one
# sentence on the one channel that reaches the operator.
#
# ===========================================================================
# WHAT IT ACTUALLY CHECKS
# ===========================================================================
# The final message names a specific agent — `zach-opus-g1`, not "Zach" — and
# says it is completed / done / finished / idle / terminated. The name is joined
# to an agent id through this session's transcript, and that id is put to
# scripts/lib/agent-liveness.py, the SAME resolver remove-agent-worktree.sh
# uses. If the authoritative answer is ALIVE, the turn does not end quietly.
#
# ===========================================================================
# IT REPORTS. IT DOES NOT BLOCK. SAID HERE AS WELL AS IN THE HEADER OF THE
# ANALYZER, BECAUSE A READER OF EITHER FILE IS ENTITLED TO KNOW.
# ===========================================================================
# Two reasons. The harm is a sentence already written, so blocking only delays
# the same correction by a round trip. And the liveness half's false-positive
# rate is UNPROVEN, in those words: lock history is not retained, so it cannot
# be measured on the archive, and there is one known false-positive mode — an
# agent that genuinely finished whose worktree has not yet been reaped still
# reads ALIVE. A guard that blocked on an unmeasured signal would be switched
# off, and being ignored is the failure mode that produced this defect.
#
# The EXTRACTOR half is measured: 5 hits over 4,230 real final messages, 5 of 5
# genuine claims about a named agent's terminal state. The numbers and the
# adjudication are in the analyzer's docstring.
#
# ===========================================================================
# WHEN IT TAKES EFFECT
# ===========================================================================
# Hooks snapshot at session start. Installing this changes nothing in the
# session that installs it; it begins reporting in the NEXT session.
#
# Exit code: 0, always. This hook never refuses a turn.
#
# Self-test:  scripts/hooks/guard-agent-state-claims.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-agent-state-claims.sh)"

# --- self-test dispatch ---------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/agent-state-claims.test.sh"
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
  hook: scripts/hooks/guard-agent-state-claims.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 0
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# --- NOTICE CHANNEL --------------------------------------------------------
# A Stop hook's stand-down and cannot-run notices go to the OPERATOR, never to
# stderr. The measurement behind that, and the argument for announcing on state
# change rather than every turn, are in scripts/lib/stop-hook-notice.sh. This
# block is byte-identical in every Stop hook and stop-hook-visibility.test.sh
# asserts it, for the reason Layer R asserts the same of the root bootstrap: a
# divergent copy is one hook disagreeing with its siblings about how it tells
# you it has stopped working.
_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
if [ -f "$_SHN_LIB" ]; then
    # shellcheck source=../lib/stop-hook-notice.sh
    . "$_SHN_LIB"
else
    # The helper is the thing that makes these notices visible, so its absence
    # must not make them invisible. The hook then announces EVERY turn,
    # undeduplicated, and says why. Degrading toward noise is recoverable by an
    # operator who can read it; degrading toward silence rebuilds the defect.
    stop_notice_init() { :; }
    stop_notice_normal() { :; }
    stop_notice_abnormal() {
        printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"NOTICE HELPER MISSING at $_SHN_LIB, so this is unconditional and undeduplicated: ${2:-}\"}"
        return 0
    }
    # The recurring form degrades to the same unconditional announcement. An
    # interval it cannot honor is announced MORE often, never less: a notice
    # channel's degraded mode leans toward noise, because noise is recoverable
    # by an operator who can read it and silence rebuilds the defect.
    stop_notice_abnormal_recurring() {
        printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"NOTICE HELPER MISSING at $_SHN_LIB, so this is unconditional and undeduplicated: ${2:-}\"}"
        return 0
    }
fi

INPUT="$(cat)"
# A blocked lead cannot perform the repairs a Stop hook would demand.
if printf '%s' "$INPUT" | python3 "$SCRIPT_DIR/../lib/stop-session-recovery.py"; then
    exit 0
fi

# Resolve the governed repository. Three outcomes, and ALL THREE let the turn
# end — this hook never refuses one. See "IT REPORTS. IT DOES NOT BLOCK." above.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    stop_notice_init "guard-agent-state-claims.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "AGENT-STATE CLAIM CHECK — NOT RUNNING: could not resolve which repository it governs, so nothing this turn said about any agent was checked against that agent's worktree lock. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-agent-state-claims.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_AGENT_STATE_CLAIMS:=1}"

stop_notice_init "guard-agent-state-claims.sh" "$ENTITY_ROOT" "$INPUT"
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# A turn that ends without this check having run must not look like a turn that
# ran it and found nothing. The sentence comes from
# scripts/lib/unevaluated-notice.sh so every hook says it the same way, and it
# goes out through the same notice channel the stand-downs above use, which is
# the one measured for the Stop event. NO VERDICT CHANGES — this hook already
# ended the turn on exactly these payloads.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself, deliberately: stop-hook-visibility.test.sh
        # proves its case 3a by sed-ing out every such call, and a multi-line
        # continuation would leave the argument lines behind as orphaned commands.
        _UE_MSG="$(unevaluated_sentence "guard-agent-state-claims.sh" \
            "whether this turn stated an agent was done without running the liveness check" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_AGENT_STATE_CLAIMS" = "0" ]; then
    # Never a silent permission. An opt-out nobody can see is indistinguishable
    # from a guard that is working, which is the shape of the whole defect.
    stop_notice_abnormal "stood-down" \
        "AGENT-STATE CLAIM CHECK — STOOD DOWN by CHECK_AGENT_STATE_CLAIMS=0 in $CONFIG. Statements that a named agent has finished are NOT being checked against its worktree lock. $HOOK_TAG"
    exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "AGENT-STATE CLAIM CHECK — NOT RUNNING: python3 is not on PATH, so this turn's statements about agents went out unchecked. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/guard-agent-state-claims.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "AGENT-STATE CLAIM CHECK — NOT RUNNING: the analyzer is missing at $ANALYZER, so this turn's statements about agents went out unchecked. $HOOK_TAG"
    exit 0
fi

RESOLVER="$ENGINE_ROOT/scripts/lib/agent-liveness.py"
if [ ! -f "$RESOLVER" ]; then
    # This guard decides NOTHING itself. Without the shared resolver there is no
    # authoritative answer to compare a claim against, and comparing against a
    # guess is exactly what went wrong.
    stop_notice_abnormal "no-resolver" \
        "AGENT-STATE CLAIM CHECK — NOT RUNNING: the liveness resolver is missing at $RESOLVER, so nothing decides whether an agent is alive. $HOOK_TAG"
    exit 0
fi

set +e
OUT="$(printf '%s' "$INPUT" | RICHOS_CLAIMS_ENTITY_ROOT="$ENTITY_ROOT" \
    python3 "$ANALYZER" 2>/dev/null)"
RC=$?
set -e

if [ "$RC" -ne 0 ]; then
    # A crash in the analyzer is a guard that did not check, not a clean turn.
    stop_notice_abnormal "analyzer-error" \
        "AGENT-STATE CLAIM CHECK — FAILED (exit $RC): this turn's statements about agents went out unchecked. $HOOK_TAG"
    exit 0
fi

# Everything present, and it just ran. Silent in the ordinary case; one line if
# the previous state was one of the abnormal ones above, so the operator who was
# told it was off is told it is back.
stop_notice_normal \
    "AGENT-STATE CLAIM CHECK — RUNNING AGAIN. Statements that a named agent has finished are being checked against its worktree lock. $HOOK_TAG"

case "$OUT" in
    FIRE*)
        KEY="$(printf '%s' "$OUT" | cut -f2)"
        MSG="$(printf '%s' "$OUT" | cut -f3)"
        stop_notice_abnormal "$KEY" "$MSG $HOOK_TAG"
        exit 0
        ;;
esac

# Nothing contradicted. A claim check that agrees says nothing, ever — but the
# state must return to normal so a LATER contradiction is announced rather than
# deduplicated against a stale key.
stop_notice_normal ""
exit 0
