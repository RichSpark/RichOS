#!/usr/bin/env bash
#
# session-start-escalations.sh — SessionStart hook. AN ESCALATION RAISED IN A
#                                 SESSION THAT HAS ENDED IS THE FIRST THING THE
#                                 NEXT SESSION IS TOLD.
#
# The predicate is scripts/lib/escalations.py. This file is the wiring and one
# design decision, and the decision is the whole file.
#
# ===========================================================================
# WHY A SessionStart HALF AT ALL — the two days
# ===========================================================================
# The turn-end notice (notice-escalations.sh) covers the session the escalation
# is raised in. It cannot cover the case that actually happened: on 2026-09-02
# two teammates raised escalations, THE SESSION ENDED, and they were found on
# 2026-09-04 by a worktree cleanup. Everything that could have surfaced them
# died with the session.
#
# So the ledger lives outside every session (see escalations.py on why not the
# session team directory), and this hook reads it before anything else happens.
# An escalation raised on Wednesday is announced on Thursday, on Friday, and
# every session after that, until somebody acknowledges it. Nothing decays,
# nothing expires, and no cleanup has to be the thing that finds it.
#
# ===========================================================================
# TWO CHANNELS, BOTH, ALWAYS
# ===========================================================================
# engine-status.sh's rule and its measurement: `hookSpecificOutput.
# additionalContext` reaches the MODEL and nothing else; `systemMessage`
# reaches the OPERATOR. An escalation announced only to the model is one the
# orchestrator may summarize away — which is the class of thing that already
# happened to the CEO's own prepared questions. One announced only to the
# operator is one the orchestrator does not know it owes an answer to.
#
# THE MODEL'S HALF CARRIES THE WHOLE ESCALATION — title, teammate, state, age,
# audience, the question, what was tried, what is proceeding meanwhile. Not a
# count and not a pointer. A count is something to acknowledge and get past;
# that is exactly how "13 items waiting" demoted the CEO's list on 2026-08-31.
# The lead gets the words the teammate wrote, without opening anything and
# without merging anything.
#
# IT ANNOUNCES NOTHING when there is no engine here, when the ledger is empty,
# or when every escalation has been acknowledged. Silence is the healthy state
# and is never spent on saying so.
#
# NEVER BLOCKS (SessionStart hooks must not), always exits 0.

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
  hook: scripts/hooks/session-start-escalations.sh
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

# NOTE ON STDIN — this hook does NOT read the payload, for the reason
# engine-status.sh documents at length: a SessionStart hook is also runnable as
# a CLI tool, and in that case stdin is an inherited pipe nobody closes, so an
# unconditional `cat` hangs forever. Nothing here needs the payload: the ledger
# is session-independent by design, which is the entire point of this hook.

emit() { # <model-block> <operator-line>
    SUMMARY="$1" SYSMSG="$2" python3 -c '
import json, os
print(json.dumps({
    "systemMessage": os.environ.get("SYSMSG", ""),
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": os.environ.get("SUMMARY", ""),
    },
}))
' 2>/dev/null || true
}

# A SessionStart hook cannot announce that python3 is missing without python3 to
# build the JSON with. Stated rather than hidden: on a machine with no python3
# NOTHING in this engine runs, and engine-status.sh is the hook that says so.
command -v python3 >/dev/null 2>&1 || exit 0

RICHOS_ENTITY_ROOT_RESOLVED=""
resolve_entity_root "" || true
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
[ -n "$ENTITY_ROOT" ] || exit 0

_ESC_LIB="$SCRIPT_DIR/../lib/escalations.sh"
[ -f "$_ESC_LIB" ] || exit 0
# shellcheck source=../lib/escalations.sh
. "$_ESC_LIB"

if ! escalations_require; then
    emit "THE ESCALATION WATCH IS OFF this session: ${ESCALATIONS_BROKEN}. Teammate escalations are written to $(escalations_ledger) and NOTHING IS READING IT — treat the absence of an escalation notice as unknown, not as none, and read that file by hand before trusting silence." \
         "ESCALATION WATCH IS OFF: ${ESCALATIONS_BROKEN} — $(escalations_ledger) is unread."
    exit 0
fi

# ERREXIT IS NEVER TURNED ON IN THIS FILE, and that is deliberate rather than an
# omission: `escalations_list` exits 1 when something IS outstanding, which is
# the case this hook exists for. A SessionStart hook that died on its own
# success condition would open every session in silence.
BLOCK="$(escalations_list session-context 2>/dev/null)"
RC=$?

if [ "$RC" -ge 2 ]; then
    emit "THE ESCALATION LEDGER COULD NOT BE READ ($(escalations_ledger)). Teammate escalations may be outstanding and unseen; this is an UNREAD ledger, not an empty one. Run escalate.sh list." \
         "ESCALATION LEDGER UNREADABLE: $(escalations_ledger) — run escalate.sh list."
    exit 0
fi

# Empty means nothing outstanding. Silence is correct and is not spent on saying so.
[ -n "$BLOCK" ] || exit 0

# The predicate separates the two halves with a form feed — one character that
# cannot occur in either, so this splits without a parser.
MODEL="$(printf '%s' "$BLOCK" | awk 'BEGIN{RS="\f"} NR==1')"
OPERATOR="$(printf '%s' "$BLOCK" | awk 'BEGIN{RS="\f"} NR==2')"
[ -n "$OPERATOR" ] || OPERATOR="A teammate escalation is outstanding — escalate.sh list"

emit "$MODEL" "$OPERATOR"
exit 0
