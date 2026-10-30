#!/usr/bin/env bash
#
# notice-waiver-repetition.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END
#                               QUIETLY WHILE AN ESCAPE HATCH IS BEING USED
#                               OVER AND OVER FOR THE SAME REASON.
#
# The argument — why 251 waivers in one day is a broken-guard report and not a
# discipline problem, why every constant is what it is, and how the ledgers are
# discovered without a typed list — is in scripts/hooks/notice-waiver-
# repetition.py. Read that first. This file is the wiring: resolve the two
# roots, hand the whole verdict to the analyzer, and say the one sentence.
#
# ===========================================================================
# WHY IT REPORTS AND DOES NOT BLOCK — argued, not defaulted to
# ===========================================================================
# The standing bar is that a deliverable which only reports has failed unless
# the report IS the mechanism. Here it is, and the reason is that the evidence
# was never missing — it was never READ. resume-acks.log held 226 entries
# across 122 recipients and 27 days, in plain text, in the repository, while
# the lead wrote entry 227. The hatches write; nothing reads. So the artifact
# that changes the outcome is the arithmetic, done automatically, on the one
# channel measured to reach the operator, in the turn before he writes the
# next one.
#
# Blocking would be the wrong instrument for a precise reason. The act it would
# refuse is the WAIVER — and in every case measured, the waiver was CORRECT:
# the teammate really was alive, the message really had to go. Refusing it
# punishes the operator for a DIFFERENT guard's false positive and wedges the
# session, which is exactly guard-inflight-notify.sh's reasoning about acks one
# event over ("blocking a land until another party acts is how a guard wedges a
# session"). And a blocking waiver-watcher would need its own escape hatch,
# which by this hook's own thesis would be waived 228 times.
#
# THIS HOOK HAS NO ESCAPE HATCH AND NO CONFIG KEY. An opt-out on the thing that
# watches opt-outs is absurd, and its absence is the whole reason this notice
# cannot decay the way the 228 did.
#
# ===========================================================================
# ONE LINE, STATE-CHANGE DE-DUPLICATED
# ===========================================================================
# systemMessage via scripts/lib/stop-hook-notice.sh, the only channel measured
# to reach the operator (see that file's table). The state key is the SET of
# flagged hatches plus each one's largest class rounded down to a power of two,
# so a new hatch crossing the line speaks, a fixed one going quiet speaks, and
# a doubling speaks — while 88 ticking to 89 does not. A condition repeated
# under every turn is a condition the eye is trained to skip, and this hook of
# all hooks must not become the thing it reports.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds
# it.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the waiver log. Byte-identical output on all
# four payload shapes.
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
  hook: scripts/hooks/notice-waiver-repetition.sh
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
    # Nothing is governed here, so no hatch of this repository is being worn
    # out. The plugin loads in every directory on the machine; a notice in each
    # would be the noise this engine already refused to make.
    exit 0
else
    # This hook believes it governs something and cannot tell what. It does NOT
    # exit 2 — a Stop hook that blocks on a broken install re-fires to the block
    # cap and strands the session. It says so instead, on the one channel that
    # reaches the operator, and stops.
    root_failure_banner "scripts/hooks/notice-waiver-repetition.sh" >&2
    stop_notice_init "notice-waiver-repetition.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "WAIVER-REPETITION WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nobody is checking whether an escape hatch is being used over and over instead of the guard being fixed — run scripts/waiver-repetition-lint.sh by hand."
    exit 0
fi

stop_notice_init "notice-waiver-repetition.sh" "$ENTITY_ROOT" "$INPUT"

# NO python3, NO ANALYZER, NO SILENCE. The wrapper decides nothing; a wrapper
# that carried on quietly without its analyzer would be a hook that is wired,
# hashed, executable and reading nothing — the exact shape that left Layer K
# green over a scanner that never ran.
ANALYZER="$SCRIPT_DIR/notice-waiver-repetition.py"
if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python" \
        "WAIVER-REPETITION WATCH IS OFF: python3 is not on PATH, so no waiver ledger was read this session. A clean report and an absent reader must never look the same."
    exit 0
fi
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "WAIVER-REPETITION WATCH IS OFF: scripts/hooks/notice-waiver-repetition.py is missing, so no escape-hatch ledger was read. This hook decides nothing on its own — without the analyzer it is wiring around an empty space."
    exit 0
fi

# ONE INVOCATION, THREE LINES: the state key, the number of discovery
# failures, and the sentence. The analyzer owns all three, so this wrapper
# cannot end up telling the operator a different number from the one
# scripts/waiver-repetition-lint.sh prints.
set +e
SUMMARY="$(python3 "$ANALYZER" \
    --engine-root "$ENGINE_ROOT" \
    --entity-root "$ENTITY_ROOT" \
    --hook-summary 2>/dev/null)"
RC=$?
set -e

if [ -z "$SUMMARY" ]; then
    stop_notice_abnormal "analyzer-failed" \
        "WAIVER-REPETITION WATCH PRODUCED NOTHING (python3 exit $RC). The escape-hatch ledgers were not read this turn; do not read this silence as a clean report — scripts/waiver-repetition-lint.sh has the detail."
    exit 0
fi

STATE_KEY="$(printf '%s\n' "$SUMMARY" | sed -n '1p')"
BROKEN_N="$(printf '%s\n' "$SUMMARY" | sed -n '2p')"
LINE="$(printf '%s\n' "$SUMMARY" | sed -n '3,$p' | tr '\n' ' ' | sed 's/ *$//')"

if [ "${BROKEN_N:-0}" != "0" ]; then
    stop_notice_abnormal "discovery-broken" \
        "WAIVER-REPETITION WATCH COULD NOT ENUMERATE THE HATCHES: its scan of the guards found nothing to read. Every escape hatch in this engine is currently unwatched — scripts/waiver-repetition-lint.sh names the reason."
    exit 0
fi

if [ -z "$LINE" ]; then
    # CLEAR. Nothing is said unless the operator was previously told otherwise,
    # in which case he is owed the end of the story.
    stop_notice_normal \
        "WAIVER-REPETITION WATCH: clear again — no escape hatch is being used repeatedly for the same reason."
    exit 0
fi

stop_notice_abnormal "$STATE_KEY" "$LINE"
exit 0
