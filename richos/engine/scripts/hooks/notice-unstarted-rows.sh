#!/usr/bin/env bash
#
# notice-unstarted-rows.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END
#                            QUIETLY WHILE AN UNBLOCKED ROW HAS NOTHING
#                            RUNNING FOR IT.
#
# The argument, the two failure directions it is designed against, and what it
# cannot see are all in scripts/lib/unstarted-rows.sh. Read that first. This
# file is the wiring: resolve the repository, sweep, and say the one sentence.
#
# ===========================================================================
# WHY IT DOES NOT BLOCK
# ===========================================================================
# `Stop` CAN block — that is measured, and guard-idle-land.sh uses it. This one
# deliberately does not, and the reason is that its subject is a judgment the
# hook does not have. "There is an unblocked row and nothing is running for it"
# is a FACT; "and therefore you should not have stopped" is not. A turn that
# ends to ask the CEO a question, or ends because he interrupted, is a turn
# ending correctly with the same set of rows outstanding. Refusing those would
# make the mechanism something to be switched off, and a guard people disable
# protects nothing.
#
# So it states the fact, names the rows, and leaves the decision where it
# belongs — which is the same trade notice-inflight-acks.sh makes, for the same
# reason, one event over.
#
# ===========================================================================
# ONE LINE, STATE-CHANGE DE-DUPLICATED
# ===========================================================================
# systemMessage via scripts/lib/stop-hook-notice.sh, because that is the only
# channel measured to reach the operator (see that file's table). The notice
# speaks when the SET changes, which is exactly the moment a row is created
# without being started, and the moment one is finally picked up, and AGAIN
# EVERY 30 MINUTES while the set persists (2026-10-22, richos-hq docs/plans/
# 2026-10-22-no-silent-holds.md): announced once is how these rows went quiet
# while the CEO asked "WHEN WILL [THIS] STOP ONCE AND FOR ALL???". Only a
# blocker naming the CEO quiets a row (unstarted-rows.py names_ceo).
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the row table in the record. Byte-identical
# output on all four payload shapes.
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

set -eo pipefail

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
  hook: scripts/hooks/notice-unstarted-rows.sh
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

INPUT="$(cat)"

_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
[ -f "$_SHN_LIB" ] || exit 0
# shellcheck source=../lib/stop-hook-notice.sh
. "$_SHN_LIB"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # Nothing is governed here, so no row of this repository can be unstarted.
    # The plugin loads in every directory on the machine; a notice in each would
    # be noise, and engine-status.sh already announces the stand-down at session
    # start.
    exit 0
else
    # This hook believes it governs something and cannot tell what. It does NOT
    # exit 2 — a Stop hook that blocks on a broken install re-fires to the block
    # cap and strands the session. It says so instead, on the one channel that
    # reaches the operator, and stops.
    root_failure_banner "scripts/hooks/notice-unstarted-rows.sh" >&2
    stop_notice_init "notice-unstarted-rows.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "UNSTARTED-ROW WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nobody is checking whether an unblocked row has anything running for it — run scripts/unstarted-rows-lint.sh by hand."
    exit 0
fi

stop_notice_init "notice-unstarted-rows.sh" "$ENTITY_ROOT" "$INPUT"

STATE_DIR="$ENTITY_ROOT/.claude/state/unstarted-rows"
RECEIPT="$STATE_DIR/last-sweep.txt"
ADOPTED="$STATE_DIR/adopted.txt"

_UR_LIB="$SCRIPT_DIR/../lib/unstarted-rows.sh"
if [ ! -f "$_UR_LIB" ]; then
    stop_notice_abnormal "no-lib" \
        "UNSTARTED-ROW WATCH IS OFF: scripts/lib/unstarted-rows.sh is missing, so no row was swept. A clean queue and an absent checker must never look the same."
    exit 0
fi
# shellcheck source=../lib/unstarted-rows.sh
. "$_UR_LIB"

RRC=0
ur_resolve "$ENTITY_ROOT" || RRC=$?
if [ "$RRC" -eq 2 ]; then
    UR_VERDICT="BROKEN"
    ur_receipt "$RECEIPT"
    stop_notice_abnormal "broken:$(printf '%s' "${UR_BROKEN_REASON:-}" | cksum | tr -d ' ')" \
        "UNSTARTED-ROW WATCH IS BROKEN, so nothing was swept: ${UR_BROKEN_REASON:-unknown}. Fix it or run scripts/unstarted-rows-lint.sh by hand — do not read this silence as an empty queue."
    exit 0
fi

if [ "$RRC" -eq 1 ]; then
    # STAND-DOWN, WITH A MEMORY. Adoption is the queue record existing, so a
    # stand-down is the ordinary answer in every repository that has no such
    # backlog — and announcing it everywhere would be the noise this engine
    # already decided not to make. But a repository that WAS swept and now
    # stands down has had its queue renamed or deleted underneath it, and that
    # is the "green over an empty set" failure arriving by the back door.
    if [ -s "$ADOPTED" ]; then
        PRIOR="$(cat "$ADOPTED" 2>/dev/null || true)"
        UR_VERDICT="BROKEN"
        UR_BROKEN_REASON="this repository was swept before, against $PRIOR, and now stands down: ${UR_STANDDOWN_REASON:-no reason given}"
        ur_receipt "$RECEIPT"
        stop_notice_abnormal "lost-queue" \
            "UNSTARTED-ROW WATCH LOST ITS QUEUE: it swept $PRIOR before and cannot find it now (${UR_STANDDOWN_REASON:-no reason given}). Every row in it is currently invisible; this is silence caused by a missing file, not by an empty backlog."
        exit 0
    fi
    UR_VERDICT="STOOD-DOWN"
    ur_receipt "$RECEIPT"
    stop_notice_normal ""
    exit 0
fi

# Adopted. Remember it, so a later disappearance is loud rather than quiet.
mkdir -p "$STATE_DIR" 2>/dev/null || true
printf '%s\n' "$UR_QUEUE_FILE" > "$ADOPTED" 2>/dev/null || true

ur_collect_claims
ur_sweep
ur_receipt "$RECEIPT"

case "$UR_VERDICT" in
    BROKEN)
        stop_notice_abnormal "broken:$(printf '%s' "${UR_BROKEN_REASON:-}" | cksum | tr -d ' ')" \
            "UNSTARTED-ROW WATCH SWEPT NOTHING — the records did not parse: ${UR_BROKEN_REASON:-unknown}. This is not an empty queue; it is an unread one. scripts/unstarted-rows-lint.sh has the detail."
        exit 0 ;;
    SWEPT)
        stop_notice_normal \
            "UNSTARTED-ROW WATCH: clear again — all ${UR_N_ROWS:-0} rows are closed, claimed by a live worktree, or name what they are waiting on."
        exit 0 ;;
esac

# NAMED, NOT COUNTED. A count is a number to get used to; a name is a thing to
# go and start. Six is where one line stops being readable, and the rest are
# reachable in one command rather than lost.
NAMES="$(printf '%s' "$UR_UNSTARTED" | tr ' ' '\n' | head -6 | tr '\n' ' ' | sed 's/ $//' | sed 's/ /, /g')"
MORE=""
[ "${UR_N_UNSTARTED:-0}" -gt 6 ] && MORE=" (+$((UR_N_UNSTARTED - 6)) more)"

stop_notice_abnormal_recurring "unstarted:$UR_UNSTARTED" \
    "UNSTARTED WORK, NOTHING RUNNING FOR IT — ${UR_N_UNSTARTED} row(s): ${NAMES}${MORE}. Nothing but Rich or earlier work holds them and no live worktree claims them, so writing them down is all that has happened. Start one, or, if it is his decision, name the CEO as its blocker: a \`Blocked by\` cell in ${UR_QUEUE_LABEL}, or \`**Blocked:** the CEO — <the CEO-TODOs item>\` in a ${UR_RECORD_LABEL} row. Detail: scripts/unstarted-rows-lint.sh" 1800
exit 0
