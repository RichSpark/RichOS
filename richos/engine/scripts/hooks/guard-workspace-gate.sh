#!/usr/bin/env bash
#
# guard-workspace-gate.sh — Stop hook. POINT 5 OF THE CEO'S WORKSPACE SPEC, as
# amended by the orchestration reliability repair (richos-hq docs/prds/2026-10-23-
# orchestration-reliability.md, R2/R3/R5/R6, 2026-10-24).
#
# IT NEVER REFUSES A TURN END. Finished work stays owned in the registry; at each
# turn end this hook lands on its own what is already landed (point 4) and, only
# while no healthy stall watcher evaluates the session, tells the LEAD the due
# work (workspaces.py fallback_notice: the one status calculation, due_set) on the
# model channel, once per change. With a healthy watcher it says nothing: the
# watcher tells due work at its next idle look. Nothing routine reaches the person;
# a deletion that keeps failing is still told to him.
#
# What it replaced: a refusal to end the turn while any finished work was
# pending, the "answer allowance" that rationed it, and a pending/hold wall at
# every turn end (7,849 of the 10,302 characters of the 2026-10-23 screenshot
# turn). The exit 2 path below remains only for a refusal the library reports.
#
# FAILS OPEN ON ITS OWN ERROR, LOUDLY: a broken engine that could never let a
# turn end would trap the CEO's session. The failure is announced every turn.

set -o pipefail

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
  hook: scripts/hooks/guard-workspace-gate.sh
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
HOOK_TAG="(hook: scripts/hooks/guard-workspace-gate.sh)"
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    stop_notice_init "guard-workspace-gate.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "WORKSPACE GATE IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Point 5 of the workspace spec — finished work landed or discarded before the turn ends — is not enforced. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-workspace-gate.sh" >&2
    exit 0
fi
stop_notice_init "guard-workspace-gate.sh" "$ENTITY_ROOT" "$INPUT"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# A turn that ends without this check having run must not look like a turn that
# ran it and found nothing. The sentence comes from
# scripts/lib/unevaluated-notice.sh so every hook says it the same way, and it
# goes out through the notice channel measured for the Stop event. The turn is
# not held: a payload that cannot be read names no session to hold it for.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself, deliberately: stop-hook-visibility.test.sh
        # proves its case 3a by sed-ing out every such call, and a multi-line
        # continuation would leave the argument lines behind as orphaned commands.
        _UE_MSG="$(unevaluated_sentence "guard-workspace-gate.sh" \
            "whether this turn ended with finished work neither landed nor discarded (point 5)" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

LIB="$SCRIPT_DIR/../../mega-lander/workspaces.py"
if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$LIB" ]; then
    stop_notice_abnormal "cannot-run" \
        "WORKSPACE GATE IS OFF: python3 or mega-lander/workspaces.py is unavailable. Point 5 of the workspace spec is not enforced until the engine is restored. $HOOK_TAG"
    exit 0
fi

ERRF="$(mktemp "${TMPDIR:-/tmp}/workspace-gate.XXXXXX")" || ERRF=""
if [ -z "$ERRF" ]; then
    stop_notice_abnormal "cannot-run" "WORKSPACE GATE IS OFF this turn: no temporary file could be made. $HOOK_TAG"
    exit 0
fi
OUT="$(printf '%s' "$INPUT" | python3 "$LIB" --entity "$ENTITY_ROOT" gate-stop 2>"$ERRF")"
RC=$?
ERR="$(cat "$ERRF")"
rm -f "$ERRF"
case "$RC" in
    0)
        [ -n "$OUT" ] && printf '%s\n' "$OUT"
        [ -n "$ERR" ] && printf '%s\n' "$ERR" >&2
        exit 0 ;;
    2)
        printf '%s\n' "$ERR" >&2
        echo "$HOOK_TAG" >&2
        exit 2 ;;
    *)
        stop_notice_abnormal "evaluation-failed" \
            "WORKSPACE GATE FAILED TO EVALUATE (exit $RC) — the turn is allowed to end and point 5 is NOT enforced: $(printf '%s' "$ERR" | tail -3 | tr '\n' ' ' | tr '"' "'" | cut -c1-400) $HOOK_TAG"
        exit 0 ;;
esac
