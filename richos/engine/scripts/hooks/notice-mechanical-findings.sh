#!/usr/bin/env bash
#
# notice-mechanical-findings.sh — NON-BLOCKING Stop hook. A DEFECT THE TREE
#                                 CAN SHOW YOU IS WRITTEN DOWN AS A ROW, BY
#                                 THE MACHINE THAT FOUND IT, AND THE TURN
#                                 DOES NOT END QUIETLY WHILE IT HAPPENED.
#
# The argument — why the trigger is the turn end, what is and is not a finding,
# how a finding keeps one identity across runs, and what this cannot see — is
# in scripts/lib/mechanical-findings.sh and scripts/lib/mechanical-findings.py.
# Read those first. This file is the wiring: resolve the repository, sweep,
# write the rows, and say the one sentence.
#
# ===========================================================================
# WHY IT WRITES AND DOES NOT ONLY REPORT
# ===========================================================================
# The standing bar in this engine is that a deliverable which only reports has
# failed unless the report IS the mechanism. Here the report is not the
# mechanism: a finding that is only announced is a finding the lead has to
# retype into the record, and that retyping is the link that was missing on
# 2026-09-02 — "Tracked separately" was a finding that had been announced and
# never written. So this hook APPENDS the row, in the record's own format with
# its own warrant, into the record's working tree. It is then a file on disk
# that the landing guards check, that the unstarted-row sweep names every turn,
# and that the lead lands or deletes on purpose. It is never edited by this
# hook again.
#
# ===========================================================================
# WHY IT DOES NOT BLOCK
# ===========================================================================
# `Stop` CAN block, and guard-idle-land.sh uses it. This one does not, for the
# reason every notice in this Stop chain gives: its subject is a fact ("a suite
# is skipped in CI"), and "therefore this turn may not end" is a judgment the
# hook does not have. A blocking coverage check is the check that gets waived,
# and the waiver ledgers on this machine held 251 entries on the day this was
# written. The row is the durable artifact; the notice is the doorbell.
#
# ===========================================================================
# ONE LINE, STATE-CHANGE DE-DUPLICATED
# ===========================================================================
# systemMessage via scripts/lib/stop-hook-notice.sh, the only channel measured
# to reach the operator (see that file's table). The state key is the set of
# rows written, rows whose finding is gone, and rows that say CLOSED over a
# finding that is still there. A stable set is announced once; the notice
# speaks again when the set changes. A finding that is KNOWN and open says
# nothing here — notice-unstarted-rows.sh already names it every turn until
# somebody starts it, and two hooks saying one thing is noise.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds
# it.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the repository, not the payload. The survey drove
# it with all four shapes and got byte-identical notices; a degraded payload
# costs it nothing and an announcement would be pure noise.
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

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
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
  hook: scripts/hooks/notice-mechanical-findings.sh
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
    # Nothing is governed here, so no record of this repository can take a
    # row. The plugin loads in every directory on the machine; a notice in
    # each would be noise, and engine-status.sh already announces the
    # stand-down at session start.
    exit 0
else
    # This hook believes it governs something and cannot tell what. It does NOT
    # exit 2 — a Stop hook that blocks on a broken install re-fires to the block
    # cap and strands the session. It says so instead, on the one channel that
    # reaches the operator, and stops.
    root_failure_banner "scripts/hooks/notice-mechanical-findings.sh" >&2
    stop_notice_init "notice-mechanical-findings.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "MECHANICAL SWEEP IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). No tree was read for skipped suites, unrun harnesses or untested hooks — run scripts/mechanical-findings-lint.sh by hand."
    exit 0
fi

stop_notice_init "notice-mechanical-findings.sh" "$ENTITY_ROOT" "$INPUT"

STATE_DIR="$ENTITY_ROOT/.claude/state/mechanical-findings"
RECEIPT="$STATE_DIR/last-sweep.txt"

_MF_LIB="$SCRIPT_DIR/../lib/mechanical-findings.sh"
if [ ! -f "$_MF_LIB" ]; then
    stop_notice_abnormal "no-lib" \
        "MECHANICAL SWEEP IS OFF: scripts/lib/mechanical-findings.sh is missing, so no tree was read. A clean tree and an absent checker must never look the same."
    exit 0
fi
# shellcheck source=../lib/mechanical-findings.sh
. "$_MF_LIB"

RRC=0
mf_resolve "$ENTITY_ROOT" || RRC=$?
if [ "$RRC" -eq 2 ]; then
    MF_VERDICT="BROKEN"
    mf_receipt "$RECEIPT"
    stop_notice_abnormal "broken:$(printf '%s' "${MF_BROKEN_REASON:-}" | cksum | tr -d ' ')" \
        "MECHANICAL SWEEP IS BROKEN, so nothing was swept: ${MF_BROKEN_REASON:-unknown}. Fix it or run scripts/mechanical-findings-lint.sh by hand — do not read this silence as a clean tree."
    exit 0
fi
if [ "$RRC" -eq 1 ]; then
    # STAND-DOWN: this repository declares no record to write into. The
    # ordinary answer in every repository that is not part of the operation,
    # and announcing it everywhere would be the noise this engine already
    # decided not to make. The receipt says so, for anyone who looks.
    MF_VERDICT="STOOD-DOWN"
    mf_receipt "$RECEIPT"
    stop_notice_normal ""
    exit 0
fi

MF_HOOK_NAME="notice-mechanical-findings.sh"
mf_sweep write
mf_receipt "$RECEIPT"

case "$MF_VERDICT" in
    BROKEN)
        stop_notice_abnormal "broken:$(printf '%s' "${MF_BROKEN_REASON:-}" | cksum | tr -d ' ')" \
            "MECHANICAL SWEEP SWEPT NOTHING: ${MF_BROKEN_REASON:-unknown}. This is not a clean tree; it is an unread one. scripts/mechanical-findings-lint.sh has the detail."
        exit 0 ;;
esac

if [ "${MF_N_SUBJECTS:-0}" -eq 0 ]; then
    stop_notice_abnormal "swept-nothing" \
        "MECHANICAL SWEEP CHECKED ZERO SUBJECTS across the record's roots — no suite, no harness, no registry was found. That is an enumerator that found nothing, not a tree with nothing in it. scripts/mechanical-findings-lint.sh has the detail."
    exit 0
fi

# WHAT IS SAID, AND WHAT IS NOT. Rows WRITTEN this turn are said, by id and
# key, because they are new work nobody is running and they are uncommitted in
# somebody else's repository. Rows whose finding is GONE are said, because a
# row describing a defect that is not there is the staleness this record's
# contract exists to remove. CLOSED rows over a live finding are said, because
# two statements disagree. KNOWN open rows are NOT said here.
PIECES=""
STATE=""
if [ -n "${MF_WRITTEN_IDS:-}" ]; then
    DETAIL="$(printf '%s\n' "$MF_WRITTEN" | awk -F'\t' 'NF>=2 && n<4 {printf "%s%s (%s)", (n?", ":""), $1, $2; n++} END{if (n>=4 && NR>4) printf " (+%d more)", NR-4}')"
    PIECES="WROTE ${MF_N_WRITTEN} ROW(S) into ${MF_RECORD_LABEL} §${MF_SECTIONS} — ${DETAIL}. They are UNCOMMITTED in ${MF_RECORD_REPO} and nothing is running for them: read each one, then land it or delete it — do not retype it."
    STATE="wrote:${MF_WRITTEN_IDS}"
fi
if [ -n "${MF_GONE_IDS:-}" ]; then
    PIECES="${PIECES:+$PIECES }ROW(S) ${MF_GONE_IDS} describe a finding the sweep no longer produces — the defect is fixed, moved or gone; close the row in the same land as the fix (it is not re-stamped for you)."
    STATE="${STATE:+$STATE|}gone:${MF_GONE_IDS}"
fi
if [ -n "${MF_CONTRA_IDS:-}" ]; then
    PIECES="${PIECES:+$PIECES }ROW(S) ${MF_CONTRA_IDS} say CLOSED and the sweep still produces their finding — one of the two is wrong."
    STATE="${STATE:+$STATE|}closed-but-present:${MF_CONTRA_IDS}"
fi
REFUSED="$(printf '%s\n' "${MF_LINES:-}" | awk -F'\t' '!found && $1=="NOTE" && $2 ~ /^WRITE REFUSED/ {print $2; found=1}')"
if [ -n "$REFUSED" ]; then
    PIECES="${PIECES:+$PIECES }${REFUSED}"
    STATE="${STATE:+$STATE|}refused"
fi

if [ -z "$PIECES" ]; then
    # A WRITE is an event, not a condition: it is announced once and then the
    # rows speak for themselves through the unstarted-row sweep, so no "clear
    # again" line follows it. A GONE row, a contradiction, a refused write or a
    # broken sweep IS a condition, and the operator who was told about it is
    # owed the end of the story.
    case "$(_shn_prior 2>/dev/null || true)" in
        *gone:*|*closed-but-present:*|*refused*|broken*|swept-nothing|root-failure|no-lib)
            stop_notice_normal \
                "MECHANICAL SWEEP: clear again — every finding in the tree has its row, and no row describes a finding that is gone." ;;
        *)
            stop_notice_normal "" ;;
    esac
    exit 0
fi

stop_notice_abnormal "$STATE" "MECHANICAL SWEEP: ${PIECES} Detail: scripts/mechanical-findings-lint.sh"
exit 0
