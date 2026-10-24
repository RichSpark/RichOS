#!/usr/bin/env bash
#
# notice-unasked-deferral.sh — NON-BLOCKING Stop hook. A REPLY THAT SEEMS TO
#                              DEFER WORK ALEX ASKED FOR IS PUT TO THE LEAD ONCE.
#
# The predicate, the corpus, the precision number, the constructions that were
# refused and WHY are in guard-unasked-deferral.py. Read that first — it is the
# analysis half. This file is the wiring and the one sentence.
#
# ===========================================================================
# THE RULE
# ===========================================================================
# A turn whose text DEFERS WORK must be paired with one of:
#   1. an AskUserQuestion in the same turn putting the deferral to him, or
#   2. an explicit CEO instruction to defer, already on the record, or
#   3. an Agent spawn in the same turn that actually starts the deferred thing.
# Otherwise the LEAD is told once, on the model channel, to check the original
# request: start the work, put the choice to him, record the real work, or drop
# the match. A match is an unverified diagnostic, never a work item: nothing is
# recorded and nothing reaches the person (orchestration reliability R1,
# richos-hq docs/prds/2026-10-23-orchestration-reliability.md).
#
# ===========================================================================
# WHY IT REPORTS AND DOES NOT BLOCK
# ===========================================================================
# By the time this fires the harm has already happened — the decision was taken
# and announced inside the text the operator is about to read. Blocking cannot
# unsay it. What blocking WOULD do is refuse to let a turn end over a regex on
# PROSE, re-firing to the binary's block cap while the operator watches a wedged
# session, and the first thing a wedged session earns is CHECK_UNASKED_DEFERRAL=0.
#
# That is the exact failure mode that produced this defect: a mechanism that is
# switched off protects nothing forever. The value here is that the CEO sees the
# deferral NAMED in the same turn, instead of having to ask a third time. Same
# trade as notice-ceo-unasked.sh, notice-unstarted-rows.sh and
# notice-inflight-acks.sh, for the same reason.
#
# ===========================================================================
# WHAT IT CANNOT DO, SAID HERE RATHER THAN ENGINEERED AROUND
# ===========================================================================
# THE ORCHESTRATOR CAN PHRASE AROUND ANY WORD LIST, and this one is six patterns
# wide. It catches the NATURAL PHRASING OF AN UNNOTICED HABIT — which is the
# actual failure mode; the specimen was written with no awareness that it was
# taking a decision that belonged to the CEO. It does not and cannot catch
# deliberate evasion. Claiming otherwise would make it a checkbox, and this
# engine's cold open is explicit about what a checkbox is worth.
#
# ===========================================================================
# WHEN IT TAKES EFFECT
# ===========================================================================
# Hooks snapshot at session start. Installing this changes NOTHING in the
# session that installs it — it begins reporting in the NEXT session. Verify it
# is live after a restart with:  scripts/hooks/notice-unasked-deferral.sh --self-test
# and by checking the engine-status banner's registered-hook count.
#
# ONE LINE, STATE-CHANGE DE-DUPLICATED, through scripts/lib/stop-hook-notice.sh
# (stop_notice_lead: Stop additionalContext, the model channel). The state key
# is the construction plus a hash of the quote, so a second DIFFERENT deferral
# speaks again and a re-report of the same one does not. The continued turn that
# answers it is not read again (guard-unasked-deferral.py, CONTINUED).
#
# Exit codes: always 0. This hook never refuses a turn.
#
# Self-test:  scripts/hooks/notice-unasked-deferral.sh --self-test

set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/notice-unasked-deferral.sh)"

if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/unasked-deferral.test.sh"
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
  hook: scripts/hooks/notice-unasked-deferral.sh
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
    stop_notice_lead() { stop_notice_abnormal "$@"; }
fi
INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    stop_notice_init "notice-unasked-deferral.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "DEFERRAL WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether this turn deferred something the CEO never chose to defer. $HOOK_TAG"
    root_failure_banner "scripts/hooks/notice-unasked-deferral.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_UNASKED_DEFERRAL:=1}"

stop_notice_init "notice-unasked-deferral.sh" "$ENTITY_ROOT" "$INPUT"

if [ "$CHECK_UNASKED_DEFERRAL" = "0" ]; then
    # Never a silent permission. An opt-out the operator cannot see is a defense
    # that decays into a rumour — and this particular guard exists precisely
    # because a decision was taken quietly.
    stop_notice_abnormal "stood-down" \
        "DEFERRAL WATCH — STOOD DOWN by CHECK_UNASKED_DEFERRAL=0 in $CONFIG. Turns that postpone work you asked for are NOT being flagged, so a deferral you never chose will pass silently. $HOOK_TAG"
    exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "DEFERRAL WATCH — NOT RUNNING: python3 is not on PATH, so this turn was not checked for a deferral you were never asked about. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/guard-unasked-deferral.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "DEFERRAL WATCH — NOT RUNNING: the analyzer is missing at $ANALYZER, so this turn was not checked for a deferral you were never asked about. $HOOK_TAG"
    exit 0
fi

# THE RECOVERY LINE IS EMITTED AFTER THE ANALYSIS, NOT BEFORE, and the reason
# is a bug this suite caught in its first run. The ledger holds ONE state per
# (session, hook). Recording "ok" on entry and then recording the finding
# afterwards makes the two calls alternate: every turn the finding key replaces
# "ok", every next turn "ok" replaces the finding key, and BOTH of them see a
# changed state and speak. The same deferral was announced on every single turn
# — de-duplication defeated by the hook that owns it.
#
# guard-unresolved-claims.sh announces up front and is right to: its finding
# goes out on exit 2, never through this ledger, so it only ever writes "ok".
# This hook writes both, so it must write exactly one of them per turn.

set +e
FINDING="$(printf '%s' "$INPUT" | python3 "$ANALYZER" 2>/dev/null)"
RC=$?
set -e

# ===========================================================================
# THE THREE OUTCOMES, AND WHY THERE USED TO BE TWO
# ===========================================================================
# 3 means a deferral fired. 0 means the turn's text was READ and is clean. 4
# means the predicate was NOT EVALUATED, with a one-word reason on stdout.
#
# 4 was added on 2026-09-05 because this wrapper made an affirmatively FALSE
# statement, and it is the only one found anywhere in the forty-hook payload
# survey. Given a payload carrying no `last_assistant_message`, the analyzer
# returned 0 — its code for "nothing fired" — and this wrapper announced:
#
#     "DEFERRAL WATCH — RUNNING AGAIN. This turn's text was checked for work
#      postponed without putting the choice to you."
#
# It was not checked. There was no turn text to check. The reassurance was
# decoupled from the check, which is a green tick over nothing, in the guard
# whose entire subject is decisions taken quietly on the CEO's behalf.
#
# A crash in the analyzer is treated the same way and for the same reason: this
# hook's premise is that missing one deferral is cheaper than being switched
# off, so an analyzer that died still ends the turn — but it ends it SAYING SO,
# not claiming to have looked.
#
# The healthy-and-clean path stays silent. stop_notice_normal speaks only when
# the previous state was one of the stand-downs above, so an operator who was
# told this guard was off is told when it comes back.
notice_clean() {
    # After a finding the clean turn says nothing: "running again" is for a watch
    # that was off, never for one that just spoke.
    case "$(_shn_prior 2>/dev/null || true)" in
        deferral:*) stop_notice_normal "" ;;
        *) stop_notice_normal \
            "DEFERRAL WATCH — RUNNING AGAIN. This turn's text was checked for work postponed without putting the choice to you. $HOOK_TAG" ;;
    esac
    exit 0
}

# NOT the same sentence with a caveat bolted on. A separate state key, so the
# operator hears it once per distinct reason, and wording that never uses the
# word "checked" about something nothing looked at.
notice_unevaluated() { # <reason word>
    local why
    case "${1:-}" in
        no-turn-text)          why="this turn arrived with no text for it to read" ;;
        payload-unreadable)    why="the turn-end payload could not be read" ;;
        transcript-unreadable) why="the transcript naming this turn's tool use could not be read, and two of the three discharges are decided from it" ;;
        turn-beyond-budget)    why="this turn is longer than the check reads (its opening prompt is more than 4 MB back in the transcript), so it was not checked" ;;
        analyzer-failed)       why="the analyzer exited $RC instead of deciding" ;;
        analyzer-empty)        why="the analyzer reported a finding and then handed over nothing to report" ;;
        *)                     why="its predicate could not be evaluated" ;;
    esac
    stop_notice_abnormal "unevaluated:${1:-unknown}" \
        "DEFERRAL WATCH — NOT RUN this turn: $why, so nothing looked for work postponed without putting the choice to you. Do not read this as a clean turn; it is an unexamined one. $HOOK_TAG"
    exit 0
}

case "$RC" in
    3) : ;;
    4) notice_unevaluated "$(printf '%s' "$FINDING" | head -1 | tr -d '[:space:]')" ;;
    0) notice_clean ;;
    *) notice_unevaluated "analyzer-failed" ;;
esac
[ -n "$FINDING" ] || notice_unevaluated "analyzer-empty"

CONSTRUCTION="$(printf '%s' "$FINDING" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(str(d.get("construction","") or ""))
except Exception: print("")' 2>/dev/null || true)"
QUOTE="$(printf '%s' "$FINDING" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(str(d.get("quote","") or ""))
except Exception: print("")' 2>/dev/null || true)"

# Same rule one level down: a finding that carries no quote is a broken
# analyzer contract, not a clean turn, and it must not borrow the clean
# turn's sentence.
[ -n "$QUOTE" ] || notice_unevaluated "analyzer-empty"

# The state key is construction + quote hash: a second, DIFFERENT deferral
# speaks again; the same one restated does not.
KEY="deferral:${CONSTRUCTION}:$(printf '%s' "$QUOTE" | cksum | tr -d ' ')"
# Once per quote per SESSION: a later status reply that repeats a held item's sentence is not
# a new deferral (Frank D2). The seen-set keeps every key told; the one-slot ledger still
# carries the on/off states.
if command -v stop_notice_seen >/dev/null 2>&1 && stop_notice_seen "$KEY"; then
    exit 0
fi

# REPORT ONLY, TO THE LEAD (orchestration reliability R1, richos-hq docs/prds/
# 2026-10-23-orchestration-reliability.md): nothing is recorded, nothing blocks,
# and nothing is said to the person. The lead reads it once in a continued turn,
# checks the original request and records the real work or drops the match.
stop_notice_lead "$KEY" \
    "DEFERRAL CHECK (unverified, report only): this reply says \"${QUOTE}\". If it postpones work Alex asked for and he did not choose that, check his original request, then start it, put the choice to him in your reply with the current default, or record the real work with workspaces.sh hold <name> --what '...' --until <agent:|review:|at:|ceo:end>. If it does not postpone his work, ignore this. It is not a work item and nothing was recorded. $HOOK_TAG"
exit 0
