#!/usr/bin/env bash
#
# guard-failure-type-answer.sh — BLOCKING Stop hook. A turn that began with his
# message putting "type" and "failure" together does not end until:
#
#   (a) the failure register was READ in this turn,
#   (b) the reply NAMES a type from it, `Type <N>: <heading, verbatim>`, for an
#       N the register has (or says "new type" and the new heading is
#       committed), and
#   (c) the register has a COMMIT made during this turn.
#
# The refusal names exactly which of the three is missing, and the path, the
# repository and the next free numbers it needs to supply them.
#
# Its partner is failure-type-lookup.sh on UserPromptSubmit, which puts the
# rule and the register's live type list in front of the lead first. The
# predicates, the measurement and the list of what neither can guarantee are in
# scripts/lib/failure-type.py; read that. This file is the wiring.
#
# THE FAILURE, PRECISELY
#   2026-09-26. Asked what type of failure something was, the lead answered from
#   recall. That morning the operator had needed three attempts to get the same
#   question answered from the register, and the lead had promised it would
#   never again take more than one. The promise was a sentence, with nothing
#   behind it, and it broke the same day. A rule enforced by attention lasts
#   exactly as long as the attention.
#
# IT DOES NOT STAND DOWN ON THE RE-FIRE. Most Stop guards here return 0 when
#   `stop_hook_active` is true, so each refuses a turn at most once. That is a
#   pattern, not a rule (guard-workspace-gate.sh keeps refusing too), and this
#   gate breaks it on purpose: every one of its three steps is something the
#   lead can do inside the same turn in under a minute. The bound is the host's
#   own consecutive-block cap, CLAUDE_CODE_STOP_HOOK_BLOCK_CAP, default 8 in
#   2.1.283. When the host does end a turn over it, the obligation is carried in
#   the entity's state into the NEXT turn, which is refused for it in turn.
#
# FAIL-OPEN, LOUDLY, LIKE ITS SIBLINGS. A broken install, an unreadable
#   payload, a missing python3, an unreadable register or transcript: the turn
#   ends and the operator is TOLD this check did not run. A Stop guard that
#   fails closed refuses to let the session end at all. An obligation that could
#   not be checked stays owed.
#
# A TEAMMATE IS NEVER REFUSED. A payload carrying agent_id is not the lead.
#
# WHEN IT TAKES EFFECT
#   Hooks snapshot at session start. Installing this changes nothing in the
#   session that installs it; it begins enforcing in the NEXT session.
#
# Exit codes (Claude Code Stop convention):
#   0  nothing owed, owed and paid, not evaluable (announced), stood down
#      (announced), or anything went wrong
#   2  BLOCKED: this turn owes a failure-type answer and (a), (b) or (c) is
#      missing
#
# Self-test:  scripts/hooks/guard-failure-type-answer.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-failure-type-answer.sh)"

# --- self-test dispatch ---------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/failure-type.test.sh"
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
  hook: scripts/hooks/guard-failure-type-answer.sh
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
# A blocked lead cannot perform the repairs a Stop hook would demand. The
# obligation is not cleared: it stays in the entity's state for the next turn.
if printf '%s' "$INPUT" | python3 "$SCRIPT_DIR/../lib/stop-session-recovery.py"; then
    exit 0
fi

# Resolve the governed repository. Three outcomes, and ALL THREE let the turn
# end. See "FAIL-OPEN" above.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    stop_notice_init "guard-failure-type-answer.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "FAILURE-TYPE ANSWER GATE — NOT RUNNING: could not resolve which repository it governs, so no turn this session is being checked for a failure-type answer taken from the register. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-failure-type-answer.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_FAILURE_TYPE:=1}"
: "${FAILURE_TYPE_REGISTER:=}"

stop_notice_init "guard-failure-type-answer.sh" "$ENTITY_ROOT" "$INPUT"
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# A turn that ends without this check having run must not look like a turn that
# ran it and found nothing. NO VERDICT CHANGES: the turn ends either way.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself: stop-hook-visibility.test.sh case 3a.
        _UE_MSG="$(unevaluated_sentence "guard-failure-type-answer.sh" \
            "whether this turn answered a failure-type question from the register" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_FAILURE_TYPE" = "0" ]; then
    # Never a silent permission.
    stop_notice_abnormal "stood-down" \
        "FAILURE-TYPE ANSWER GATE — STOOD DOWN by CHECK_FAILURE_TYPE=0 in $CONFIG. A turn that answers his failure-type question from memory is NOT being refused this session. $HOOK_TAG"
    exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "FAILURE-TYPE ANSWER GATE — NOT RUNNING: python3 is not on PATH, so turns are ending unchecked this session. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/../lib/failure-type.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "FAILURE-TYPE ANSWER GATE — NOT RUNNING: the analyzer is missing at $ANALYZER, so turns are ending unchecked this session. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

# THE ANALYZER'S STDOUT IS CAPTURED: one line, `FT<TAB>kind<TAB>detail`, kind
# one of none | satisfied | blocked | cannot. Its stderr is NOT captured: on
# exit 2 that is the refusal, and it is what reaches the model.
set +e
ANALYZER_OUT="$(printf '%s' "$INPUT" | RICHOS_FT_ENTITY_ROOT="$ENTITY_ROOT" \
    RICHOS_FT_REGISTER="$FAILURE_TYPE_REGISTER" \
    python3 "$ANALYZER" stop)"
RC=$?
set -e

LINE="$(printf '%s\n' "$ANALYZER_OUT" | grep -m1 "^FT	" || true)"
KIND="$(printf '%s' "$LINE" | cut -f2)"
DETAIL="$(printf '%s' "$LINE" | cut -f3- | tr -d '\000-\010\013\014\016-\037')"

# Failing open must not turn a failed analyzer into a healthy observation.
if [ -z "$KIND" ] || { [ "$RC" != "0" ] && [ "$RC" != "2" ]; }; then
    stop_notice_abnormal "analyzer-exit:$RC" \
        "FAILURE-TYPE ANSWER GATE: NOT RUNNING. The analyzer exited $RC without a verdict, so this turn was NOT CHECKED for a failure-type answer; it is allowed to end. $HOOK_TAG"
    exit 0
fi

# EXACTLY ONE JSON OBJECT ON STDOUT PER TURN: the host parses stdout as one
# document. Each branch below prints at most one.
case "$KIND" in
    blocked)
        stop_notice_normal ""
        [ "$RC" = "2" ] && exit 2
        exit 0 ;;
    cannot)
        # Keyed per turn: an unchecked turn is news every time it happens.
        _PID="$(printf '%s' "$INPUT" | tr '\n' ' ' | grep -o '"prompt_id"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 || true)"
        stop_notice_abnormal "cannot:$(printf '%s' "$DETAIL$_PID" | cksum | tr -d ' ')" \
            "FAILURE-TYPE ANSWER GATE: $DETAIL. The turn is allowed to end. $HOOK_TAG"
        exit 0 ;;
    satisfied)
        # The ledger returns to normal quietly, and the person is told once,
        # in one line, which type was named and which commit holds the record.
        stop_notice_normal ""
        _J="${DETAIL//\\/\\\\}"; _J="${_J//\"/\\\"}"
        printf '{"suppressOutput":true,"systemMessage":"%s"}\n' "$_J"
        exit 0 ;;
    *)
        stop_notice_normal \
            "FAILURE-TYPE ANSWER GATE — RUNNING AGAIN. Turns that answer his failure-type question are being checked against the register once more. $HOOK_TAG"
        exit 0 ;;
esac
