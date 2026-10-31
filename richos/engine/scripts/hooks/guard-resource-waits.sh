#!/usr/bin/env bash
#
# guard-resource-waits.sh — BLOCKING Stop hook. THE LEAD'S TURN DOES NOT END
#                           WHILE ANY JOB HAS WAITED MORE THAN TEN MINUTES ON A
#                           SHARED RESOURCE.
#
# THE CEO, 2026-09-25: "A free Mac is used whole; nothing waits in a line."
# THE CEO, 2026-09-27, after jobs waited hours that day for the single test VM:
#   "do I want the work to be BLOCKED AND PISSED AWAY like this because the
#   current worker might need the VM for a 5-second-long fart? Do I want HOURS
#   OF DEVELOPMENT TIME TO BE PISSED AWAY FOR EVERY 5-SECOND FART???" and
#   "how many more times will this exact fuckshit keep repeating itself?"
#
# The rule existed for two days with no mechanism, and three 90-minute wait
# escalations were each answered "keep waiting". This refuses the turn end and
# names the waiter, what it waits for, who holds the resource and whether the
# holder is actually using it right now.
#
# IT CLEARS ONLY WHEN THE WAIT ENDS. No acknowledgement, disposition or marker
# line clears it. The sources, what ends each wait, how the holder is read and
# what the gate cannot see are in scripts/lib/resource_waits.py; read that.
# This file is the wiring.
#
# IT DOES NOT STAND DOWN ON THE RE-FIRE (`stop_hook_active`), for the reason
#   guard-failure-type-answer.sh gives: the lead can act inside the same turn
#   (create the go-file, reclaim an idle slot, stop the holder, tell the waiter
#   to stop waiting). The bound is the host's own consecutive-block cap. When
#   the host ends a turn over it, the wait is still there and the next turn end
#   is refused for it in turn.
#
# FAIL-OPEN, LOUDLY, LIKE ITS SIBLINGS. A broken install, an unreadable
#   payload, a missing python3 or analyzer: the turn ends and the operator is
#   TOLD the check did not run. A wait source that cannot be read does not
#   block by itself; the refusal stands only on waits that WERE read.
#
# A TEAMMATE IS NEVER REFUSED. A payload carrying agent_id is not the lead.
#
# CONFIGURATION (orchestration.config of the governed repository):
#   CHECK_RESOURCE_WAITS=0     stands the gate down, announced, never silent
#   RESOURCE_WAIT_MINUTES=10   the threshold (the CEO's ten minutes)
#
# WHEN IT TAKES EFFECT
#   Hooks snapshot at session start. Installing this changes nothing in the
#   session that installs it; it begins enforcing in the NEXT session.
#
# Exit codes (Claude Code Stop convention):
#   0  no wait over the threshold, not evaluable (announced), stood down
#      (announced), or anything went wrong
#   2  BLOCKED: at least one wait is over the threshold
#
# Self-test:  scripts/hooks/guard-resource-waits.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-resource-waits.sh)"

# --- self-test dispatch ---------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/resource-waits.test.sh"
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
  hook: scripts/hooks/guard-resource-waits.sh
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

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    stop_notice_init "guard-resource-waits.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "RESOURCE-WAIT GATE — NOT RUNNING: could not resolve which repository it governs, so a job can wait in line for the VM or for CPU admission for hours tonight and no turn end will say so. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-resource-waits.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_RESOURCE_WAITS:=1}"
: "${RESOURCE_WAIT_MINUTES:=10}"

stop_notice_init "guard-resource-waits.sh" "$ENTITY_ROOT" "$INPUT"
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself: stop-hook-visibility.test.sh case 3a.
        _UE_MSG="$(unevaluated_sentence "guard-resource-waits.sh" \
            "whether any job has waited past the resource-wait threshold on a shared resource" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_RESOURCE_WAITS" = "0" ]; then
    # Never a silent permission.
    stop_notice_abnormal "stood-down" \
        "RESOURCE-WAIT GATE — STOOD DOWN by CHECK_RESOURCE_WAITS=0 in $CONFIG. A job can wait in line for the VM or for CPU admission for hours and no turn end will be refused for it this session. $HOOK_TAG"
    exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "RESOURCE-WAIT GATE — NOT RUNNING: python3 is not on PATH, so turns are ending without checking for jobs waiting in line. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/../lib/resource_waits.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "RESOURCE-WAIT GATE — NOT RUNNING: the analyzer is missing at $ANALYZER, so turns are ending without checking for jobs waiting in line. $HOOK_TAG"
    exit 0
fi

# THE ANALYZER'S STDOUT IS CAPTURED: one line, `RW<TAB>kind<TAB>detail`, kind
# one of none | blocked | cannot. Its stderr is NOT captured: on exit 2 that is
# the refusal, and it is what reaches the model.
set +e
ANALYZER_OUT="$(printf '%s' "$INPUT" | RICHOS_RESOURCE_WAIT_MINUTES="$RESOURCE_WAIT_MINUTES" \
    python3 "$ANALYZER" stop)"
RC=$?
set -e

LINE="$(printf '%s\n' "$ANALYZER_OUT" | grep -m1 "^RW	" || true)"
KIND="$(printf '%s' "$LINE" | cut -f2)"
DETAIL="$(printf '%s' "$LINE" | cut -f3- | tr -d '\000-\010\013\014\016-\037' | tr '"' "'")"

# Failing open must not turn a failed analyzer into a healthy observation.
if [ -z "$KIND" ] || { [ "$RC" != "0" ] && [ "$RC" != "2" ]; }; then
    stop_notice_abnormal "analyzer-exit:$RC" \
        "RESOURCE-WAIT GATE: NOT RUNNING. The analyzer exited $RC without a verdict, so this turn was NOT CHECKED for jobs waiting in line; it is allowed to end. $HOOK_TAG"
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
        # Announced when what could not be read changes, and again every hour
        # while it stays unread: a wait source nobody can see is news.
        stop_notice_abnormal_recurring "cannot:$(printf '%s' "$DETAIL" | sed 's/[0-9]* wait(s).*//' | cksum | tr -d ' ')" \
            "RESOURCE-WAIT GATE: $DETAIL. The turn is allowed to end on what was read. $HOOK_TAG" 3600
        exit 0 ;;
    *)
        stop_notice_normal \
            "RESOURCE-WAIT GATE — RUNNING AGAIN. Turn ends are being refused again while any job waits more than ${RESOURCE_WAIT_MINUTES} minutes on a shared resource. $HOOK_TAG"
        exit 0 ;;
esac
