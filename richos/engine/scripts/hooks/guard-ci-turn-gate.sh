#!/usr/bin/env bash
#
# guard-ci-turn-gate.sh — BLOCKING Stop hook. THE TURN DOES NOT END WHILE THIS
#                          SESSION'S OWN PUSH IS FAILING CI.
#
# THE FOUNDER'S QUESTION, 2026-09-13, asked twice in one evening after three
# screenshots of red CI:
#
#     "How many more times will I need to post screenshots of that CI?"
#
# He was given a promise. He then asked the question a promise cannot answer —
# HOW WOULD HE KNOW the answer was zero — and the honest answer was that he
# could not. This is the mechanism that makes it knowable: he can watch it
# refuse, the way he can watch GitHub's `main-only` ruleset refuse a branch.
#
# THIS WRAPPER DECIDES NOTHING. It settles jurisdiction (which repository is
# this session's?), then hands the entire verdict to guard-ci-turn-gate.py —
# the predicate, the budget, the escape hatch and the words. That file's header
# carries the argument for every one of them, in particular the crux: a run
# still IN PROGRESS does not hold the turn, it holds the obligation, and every
# later turn-end re-reads it.
#
# THE BUDGET IS A NUMBER, NOT AN ADJECTIVE: BUDGET_SECONDS = 2.0 in the
# analyzer, enforced as a wall-clock deadline around every subprocess it
# starts. A check that cannot answer inside its budget ALLOWS the turn and says
# why. The `timeout` on the registration is the host's backstop, not the budget.
#
# FAILS OPEN ON ITS OWN ERROR, LOUDLY, for guard-workspace-gate.sh's reason: a
# broken engine that could never let a turn end would trap the founder's
# session, so every failure here is announced and none of them blocks.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
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
  hook: scripts/hooks/guard-ci-turn-gate.sh
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
HOOK_TAG="(hook: scripts/hooks/guard-ci-turn-gate.sh)"
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    stop_notice_init "guard-ci-turn-gate.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "THE CI TURN GATE IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). A turn can end tonight on a push whose CI is failing, and nothing will say so. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-ci-turn-gate.sh" >&2
    exit 0
fi
stop_notice_init "guard-ci-turn-gate.sh" "$ENTITY_ROOT" "$INPUT"

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
        _UE_MSG="$(unevaluated_sentence "guard-ci-turn-gate.sh" \
            "whether this turn is ending on a commit this session pushed whose CI is failing" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

ANALYZER="$SCRIPT_DIR/guard-ci-turn-gate.py"
if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "cannot-run" \
        "THE CI TURN GATE IS OFF: python3 or scripts/hooks/guard-ci-turn-gate.py is unavailable, so nothing stops this turn ending on a failing push. $HOOK_TAG"
    exit 0
fi

ERRF="$(mktemp "${TMPDIR:-/tmp}/ci-turn-gate.XXXXXX")" || ERRF=""
if [ -z "$ERRF" ]; then
    stop_notice_abnormal "cannot-run" "THE CI TURN GATE IS OFF this turn: no temporary file could be made. $HOOK_TAG"
    exit 0
fi
OUT="$(printf '%s' "$INPUT" | python3 "$ANALYZER" --entity "$ENTITY_ROOT" 2>"$ERRF")"
RC=$?
ERR="$(cat "$ERRF")"
rm -f "$ERRF"
case "$RC" in
    0)
        # stdout is at most one {"systemMessage": ...} line, which is the ONLY
        # channel a Stop hook has to the operator's screen (measured; see
        # scripts/lib/stop-hook-notice.sh). An in-flight run, an unreadable
        # repository and an accepted ack all arrive this way — the turn ends,
        # and it does not end in silence.
        [ -n "$OUT" ] && printf '%s\n' "$OUT"
        [ -n "$ERR" ] && printf '%s\n' "$ERR" >&2
        exit 0 ;;
    2)
        printf '%s\n' "$ERR" >&2
        echo "$HOOK_TAG" >&2
        exit 2 ;;
    *)
        stop_notice_abnormal "evaluation-failed" \
            "THE CI TURN GATE FAILED TO EVALUATE (exit $RC) — the turn is allowed to end and a failing push is NOT being checked for: $(printf '%s' "$ERR" | tail -3 | tr '\n' ' ' | tr '"' "'" | cut -c1-400) $HOOK_TAG"
        exit 0 ;;
esac
