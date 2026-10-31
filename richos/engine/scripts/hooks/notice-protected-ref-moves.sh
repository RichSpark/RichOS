#!/usr/bin/env bash
#
# notice-protected-ref-moves.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END
#                                  QUIETLY WHILE A PROTECTED BRANCH IS MISSING
#                                  COMMITS IT HAD.
#
# The predicate, and the argument for every rule in it, is in
# scripts/lib/protected-ref-moves.py. Read that first. This file is the wiring
# and the one sentence.
#
# ===========================================================================
# THE GAP THIS CLOSES
# ===========================================================================
# On 2026-09-14 the engine's protected-ref check stopped WRITING. It used to
# move `refs/heads/main` back to a tip one agent's window happened to hold, with
# no compare-and-swap and no reflog message; on 2026-09-13/14 that line moved
# main in /Users/alex/ab/richos three times in one night, the second and third
# twelve seconds apart in opposite directions, while the lead was landing. It
# now reports a move and leaves the ref alone.
#
# THE ENGINEER WHO MADE THAT TRADE RAISED IT HIMSELF rather than hiding it:
# *"who reads that notice is not mine to decide."* He was right to. Where the
# notice went, measured rather than assumed:
#
#   * a `protected-ref-moved` row in the store's event log — read by no hook,
#     no script and no gate on this machine (grepped, 2026-09-14);
#   * a row on the agent's own record — surfaced by nothing;
#   * a `=== PROTECTED REF MOVED ... ===` line on the STDERR of
#     observe-created-refs.sh, a PostToolUse hook that exits 0. The measured
#     channel table in scripts/lib/stop-hook-notice.sh is explicit: stderr at
#     exit 0 reaches the transcript and NOT the operator's stream. Worse, that
#     stderr belongs to the session of the agent that made the move — the one
#     party who cannot decide whether the move was allowed.
#
# So an automatic action had been replaced by a notice with no reader, which is
# how a safety mechanism dies quietly. This hook is the reader, and it is the
# lead's: the branch is one only the lead may move (worktree spec point 14), so
# the lead is the person the finding belongs to.
#
# ===========================================================================
# WHY IT REPORTS AND MUST NOT BLOCK
# ===========================================================================
# The check stopped writing because its inference was wrong: it cannot tell the
# lead's ordinary land from the abuse it hunts, and the land is the most common
# write that branch ever receives. A BLOCKING version of that same inference
# would be the identical defect with a heavier instrument — it would refuse the
# lead's turn on the strength of a guess that has already been measured wrong,
# and the fix on the day would be to waive it, which is how a guard dies.
#
# What this hook announces is NOT that inference. It is one mechanical fact
# asked later, at rest: the tip the branch held when the agent's call started is
# no longer on the branch. Commits are missing. That statement has no threshold
# and no false-positive class, and it still is not grounds to refuse a turn —
# losing a commit on purpose is a thing the lead does (a rewind to drop a bad
# land), so the answer belongs to him and not to a hook.
#
# NO CONFIG KEY AND NO ESCAPE HATCH, for the reason notice-escalations.sh gives:
# an opt-out on the watch over a ref only the lead may move is absurd, and its
# absence is why this notice cannot decay into a rumour. The per-finding
# settlement (`protected-ref-moves.py review --why ...`) is the only way to
# silence one, it is durable, and it carries a reason.
#
# ===========================================================================
# ONE LINE, STATE-CHANGE DE-DUPLICATED — AND IT GETS LOUDER
# ===========================================================================
# systemMessage via scripts/lib/stop-hook-notice.sh, the only channel measured
# to reach the operator. The state key the predicate returns carries the set of
# findings AND an age bucket per finding (1 h / 24 h / 72 h), so a set that
# never changes still crosses rungs; on top of that the announcement recurs
# hourly while it stands, because a control that goes QUIETER as the failure
# persists is the failure shape recorded on 2026-09-10 as type F. The ledger is
# keyed per SESSION, so a new session re-announces everything outstanding — a
# move made while the lead was away is on his screen at the end of his first
# turn, without a second registration on SessionStart.
#
# A HEALED FINDING SAYS NOTHING. The commits coming back — by a merge, a reset,
# anything — closes it with nobody told to close it, and that is the ordinary
# ending. Silence here means "the branches hold what they held", never "nobody
# looked".
#
# NOT SCOPED TO A REPOSITORY, deliberately, and for notice-escalations.sh's
# reason: a session seated in one repository with teammates in worktrees of
# another is the NORMAL shape of this operation, and the real incident was a
# femcboost session's agents moving richos main. It stands down only where the
# engine is not adopted at all.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds it.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the workspace store's event log joined
# against the repositories it names; the payload contributes nothing but the
# session id used to de-duplicate, and a payload it cannot read degrades toward
# announcing MORE often rather than less. This hook already announces when the
# predicate is unavailable — 'PROTECTED REF WATCH IS OFF' and 'PROTECTED REF
# WATCH PRODUCED NOTHING' — which is that same property implemented for a
# different input.
#
# WHY THIS ONE NEEDS A DECLARATION WHERE THE OTHER EXEMPT HOOKS DO NOT.
# scripts/hooks/unevaluated-payload.test.sh derives every registered PreToolUse
# and Stop hook from hooks/hooks.json and drives each with an empty, a truncated
# and a non-JSON payload. A hook that REFUSES them is proven fail-closed by that
# alone; a hook that ANNOUNCES on them is proven audible by that alone; neither
# needs a word in its source. But a hook that is SILENT on all four looks
# identical whether its predicate never needed the payload or its predicate was
# silently lost — which is the whole defect, and it is the one thing driving a
# hook cannot tell you. So payload-independence is the single class that must be
# CLAIMED by a person, and the suite then holds the claim to its consequence:
# the output must be identical on all four payloads. An undeclared silent hook
# fails that suite; a falsely declared one fails it too.
#
# Exit codes: always 0. This hook never refuses a turn.
#
# Self-test:  scripts/hooks/notice-protected-ref-moves.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/protected-ref-moves.test.sh"
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
  hook: scripts/hooks/notice-protected-ref-moves.sh
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

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # Nothing is governed here. The plugin loads in every directory on the
    # machine and a notice in each would be the noise this engine already
    # decided not to make.
    exit 0
else
    # This hook believes it governs something and cannot tell what. It does NOT
    # exit 2 — a Stop hook that blocks on a broken install re-fires to the block
    # cap and strands the session. It says so instead, on the one channel that
    # reaches the operator, and stops.
    stop_notice_init "notice-protected-ref-moves.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "PROTECTED REF WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether a protected branch lost commits it held while an agent was running — scripts/lib/protected-ref-moves.py list is the command, run it by hand."
    root_failure_banner "scripts/hooks/notice-protected-ref-moves.sh" >&2
    exit 0
fi

stop_notice_init "notice-protected-ref-moves.sh" "$ENTITY_ROOT" "$INPUT"

# NO PREDICATE, NO SILENCE. A wrapper that carried on quietly without its
# predicate would be a hook that is wired, hashed, executable and reading
# nothing — the exact shape that left Layer K green over a scanner that never
# ran. An absent reader and a clean report must never look the same.
PRM_LIB="$SCRIPT_DIR/../lib/protected-ref-moves.py"
if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$PRM_LIB" ]; then
    stop_notice_abnormal "broken" \
        "PROTECTED REF WATCH IS OFF: python3 or $PRM_LIB is unavailable, so no protected-ref finding was read this turn. Do not take this silence for an empty store — the rows are in the workspace store's events.jsonl. Restore the engine."
    exit 0
fi

# ONE INVOCATION, THREE LINES: the state key, the count, and the sentence. The
# predicate owns all three, so this wrapper cannot end up telling the operator a
# different number from the one `protected-ref-moves.py list` prints.
set +e
SUMMARY="$(python3 "$PRM_LIB" hook-summary 2>/dev/null)"
RC=$?
set -e

if [ -z "$SUMMARY" ] || [ "$RC" -ge 2 ]; then
    stop_notice_abnormal "predicate-failed" \
        "PROTECTED REF WATCH PRODUCED NOTHING (exit $RC) over the workspace store. A protected branch losing commits now would not be announced; this silence is not a clean report — scripts/lib/protected-ref-moves.py list has the detail."
    exit 0
fi

KEY="$(printf '%s\n' "$SUMMARY" | sed -n '1p')"
LINE="$(printf '%s\n' "$SUMMARY" | sed -n '3,$p' | tr '\n' ' ' | sed 's/ *$//')"

if [ "$KEY" = "clear" ]; then
    # CLEAR. Nothing is said unless the operator was previously told otherwise,
    # in which case he is owed the end of the story.
    stop_notice_normal "$LINE"
    exit 0
fi

# RECURRING, not announce-once. A branch missing commits does not heal by being
# ignored, and the type-F failure — a control that goes quieter the longer the
# fault persists — is the one this must not have. One hour is the rung.
stop_notice_abnormal_recurring "$KEY" "$LINE" 3600
exit 0
