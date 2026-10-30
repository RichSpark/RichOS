#!/usr/bin/env bash
#
# failure-type-lookup.sh — WHEN HE PUTS "TYPE" AND "FAILURE" TOGETHER, THE
#                          FAILURE REGISTER IS IN FRONT OF THE LEAD BEFORE IT
#                          WRITES A WORD.
#
# Registered UserPromptSubmit. Its partner is guard-failure-type-answer.sh on
# Stop, which refuses the turn end until the register was read, a type from it
# was named by number with its heading, and the record was committed. The
# predicates, the measurement and the honest list of what neither hook can
# guarantee are in scripts/lib/failure-type.py; read that first. This file is
# the wiring and the channel.
#
# ===========================================================================
# WHAT THIS IS FOR
# ===========================================================================
# 2026-09-26. The operator asked what type of failure something was, and was
# answered from recall. It had happened before: that morning he had to ask the
# same question three times, and the lead promised that a question about a type
# of failure would be answered from the register on the first reply. Nothing
# held the promise but attention, and it broke six hours later. His question
# after that was what exactly must ALWAYS happen when he mentions those two
# words close together, and when it would happen with guaranteed reliability.
# In the register this is Type 61, the lookup that takes seconds answered from
# memory instead, recorded again at section 5.2.
#
# ===========================================================================
# WHAT IT DOES
# ===========================================================================
# On a message of HIS whose words include an inflection of "type" and of
# "failure" within six words of each other:
#   * the MODEL gets the rule, the register's path, both next free numbers from
#     the register's box (and any disagreement between the box and the
#     headings), and every `Type <N>: <heading>` line, READ FROM THE FILE ON
#     THIS MESSAGE, never from a copy;
#   * the PERSON gets one line saying the lookup ran and how many types went in;
#   * an obligation is written to the entity's state, which the Stop gate
#     honors even if it cannot find the message in the transcript.
# On every other message it prints nothing.
#
# IT REFUSES NOTHING. Every path exits 0. The refusal lives at Stop, where the
# lead's actions can be read; this hook only makes sure the answer never has
# to come from memory.
#
# ===========================================================================
# THE CHANNEL
# ===========================================================================
# additionalContext reaches the model at UserPromptSubmit and systemMessage
# reaches the person; the measurement is in the table below and in
# left-off-report.sh. The host keeps a hook's additionalContext inline up to
# 10,000 characters (read from the 2.1.283 binary: `CLo=1e4`) and over that
# puts only its head and a file path in front of the model; an older
# measurement recorded in commit-ceo-inputs.sh says 8000 and a dropped object.
# So the analyzer budgets the list under 10,000 and NAMES any type it could not
# fit, rather than letting the host cut it where nobody can see.
#
# Stand down with CHECK_FAILURE_TYPE=0 in orchestration.config: announced when
# his message carries the words, never silent.
#
# Exit codes: always 0.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

HOOK_TAG="(hook: scripts/hooks/failure-type-lookup.sh)"

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
  hook: scripts/hooks/failure-type-lookup.sh
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

# --- OUTPUT (builtins only) -------------------------------------------------
# One of the states this file must be able to announce is "python3 is not on
# PATH", and an emitter that needed python3 could not report it.
_esc() {
    local s="${1:-}"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\r'/ }"
    s="${s//$'\t'/    }"
    printf '%s' "$s"
}
_say() { # <person-line> <model-context>
    printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"%s"}}\n' \
        "$(_esc "${1:-}")" "$(_esc "${2:-}")"
    exit 0
}

INPUT="$(cat)"

# A cheap test for "might carry the words", used ONLY to decide whether a
# cannot-run state is worth announcing on this message. The real predicate is
# the analyzer's; this one only keeps a broken install from speaking under
# every message he sends.
_might_trigger() {
    printf '%s' "$INPUT" | grep -Eiq '(^|[^a-z])typ(e|es|ed|ing)([^a-z]|$)' \
        && printf '%s' "$INPUT" | grep -Eiq '(^|[^a-z])failures?([^a-z]|$)'
}

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # engine-status.sh announces the whole stand-down at session start.
    exit 0
else
    _might_trigger || exit 0
    _say "FAILURE-TYPE LOOKUP IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). $HOOK_TAG" \
         "THE FAILURE-TYPE LOOKUP COULD NOT RUN: failure-type-lookup.sh could not resolve its repository (${RICHOS_ROOT_REASON:-root resolution failed}). If his message puts \"type\" and \"failure\" together, read the failure register yourself and answer with \`Type <N>: <heading>\` from it. $HOOK_TAG"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_FAILURE_TYPE:=1}"
: "${FAILURE_TYPE_REGISTER:=}"

if ! command -v python3 >/dev/null 2>&1; then
    _might_trigger || exit 0
    _say "FAILURE-TYPE LOOKUP NOT RUNNING: python3 is not on PATH. $HOOK_TAG" \
         "THE FAILURE-TYPE LOOKUP IS NOT RUNNING (python3 is not on PATH). If his message puts \"type\" and \"failure\" together, read the failure register yourself and answer with \`Type <N>: <heading>\` from it. $HOOK_TAG"
fi

ANALYZER="$ENGINE_ROOT/scripts/lib/failure-type.py"
if [ ! -f "$ANALYZER" ]; then
    _might_trigger || exit 0
    _say "FAILURE-TYPE LOOKUP NOT RUNNING: the analyzer is missing at $ANALYZER. $HOOK_TAG" \
         "THE FAILURE-TYPE LOOKUP IS NOT RUNNING: its analyzer is missing at $ANALYZER. If his message puts \"type\" and \"failure\" together, read the failure register yourself and answer with \`Type <N>: <heading>\` from it. $HOOK_TAG"
fi

STOOD_DOWN=0
[ "$CHECK_FAILURE_TYPE" = "0" ] && STOOD_DOWN=1

# The analyzer prints the host's JSON object itself (json.dumps), or nothing.
OUT="$(printf '%s' "$INPUT" | RICHOS_FT_ENTITY_ROOT="$ENTITY_ROOT" \
    RICHOS_FT_REGISTER="$FAILURE_TYPE_REGISTER" \
    RICHOS_FT_STOOD_DOWN="$STOOD_DOWN" \
    python3 "$ANALYZER" inject 2>/dev/null)"
RC=$?

if [ "$RC" != "0" ]; then
    _might_trigger || exit 0
    _say "FAILURE-TYPE LOOKUP FAILED: the analyzer exited $RC. $HOOK_TAG" \
         "THE FAILURE-TYPE LOOKUP FAILED (analyzer exit $RC), so nothing was checked. If his message puts \"type\" and \"failure\" together, read the failure register yourself and answer with \`Type <N>: <heading>\` from it. $HOOK_TAG"
fi
[ -n "$OUT" ] && printf '%s\n' "$OUT"
exit 0
