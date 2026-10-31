#!/usr/bin/env bash
#
# notice-escalations.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END QUIETLY
#                          WHILE A TEAMMATE'S ESCALATION IS SITTING UNREAD.
#
# The predicate, and the argument for every constant, is in
# scripts/lib/escalations.py. Read that first. This file is the wiring and the
# one sentence.
#
# ===========================================================================
# THE FAILURE
# ===========================================================================
# On 2026-09-02 two teammates finished their work and each recorded, correctly,
# that a premise in its brief was contradicted by evidence. Both wrote
# `BLOCKED.md` on their own branch because that was the protocol. They were
# found on 2026-09-04 by a worktree cleanup, because somebody was counting
# directories. NEITHER WAS A STALL and both were right to be written; what
# failed was the channel.
#
# A file on a teammate's branch is visible only to whoever merges that branch,
# and the teammate cannot see whether its branch was ever merged. So the
# escalation now goes to a ledger outside every repository
# (scripts/lib/escalations.py) and THIS is the half that makes it arrive: every
# turn ends here, and a turn that ends with an unacknowledged escalation says so.
#
# ===========================================================================
# TWO AUDIENCES, AND UNTIL 2026-09-28 ONLY ONE OF THEM WAS REACHED
# ===========================================================================
# The standing bar in this engine is that a deliverable which only reports has
# failed unless the report IS the mechanism. Here it is. Nothing was ever
# missing except the reading: both escalations existed, in git, in full, for two
# days. The artifact that changes the outcome is the ARRIVAL, in the turn after
# it is raised — to the operator AND to the lead's model.
#
# For its first three weeks this hook spoke only `systemMessage`, and a Stop
# hook's systemMessage reaches the PERSON and never the model (the channel
# table below; the hooks reference says "Warning message shown to the user").
# Measured 2026-09-28 in the lead's own session: a teammate's escalation was in
# the ledger at 13:49Z, this hook fired four times after it, and every firing
# sat in the transcript as `hook_success` with `content: ""`. The lead learned
# of it only at the next session start. So the operator's line is now joined,
# in the same JSON object, by `hookSpecificOutput.additionalContext` carrying
# every NEW escalation in the teammate's own words — the Stop field that does
# reach the model. What counts as new, and why that memory is per session and
# not in the ledger, is argued under "DELIVERY" in scripts/lib/escalations.py.
#
# additionalContext ON Stop CONTINUES THE CONVERSATION, and that is not the
# blocking refused below. The hooks reference: "Non-error feedback for Claude.
# The conversation continues so Claude can act on it, but unlike
# decision: block it is shown in the transcript as hook feedback rather than a
# hook error." Nothing has to be answered or acknowledged for the turn to end;
# each escalation is delivered ONCE per session, so the next turn end has
# nothing new and the lead stops. Raising an escalation costs the lead one
# extra model call, once — which is the arrival this file exists for.
#
# Blocking would be the wrong instrument, for the reason guard-inflight-notify.sh
# gives one event over: it would refuse the LEAD's turn until a third party's
# question has been answered, which is how a guard wedges a session. Worse, it
# would make raising an escalation an act that stops the lead working — and a
# teammate who learns that stops raising them. The escalation channel dies the
# moment it becomes expensive to use.
#
# NO ESCAPE HATCH AND NO CONFIG KEY, for the reason notice-waiver-repetition.sh
# states: an opt-out on the thing that watches for unread escalations is absurd,
# and its absence is why this notice cannot decay.
#
# ===========================================================================
# IT SAYS "THIS IS NOT A STALL" EVERY TIME, AND THAT IS DELIBERATE
# ===========================================================================
# The predicate carries each escalation's `state`, and the sentence names it.
# When nothing outstanding is `stopped`, the line says so in words: the work is
# done or continuing. A notice that read every escalation as a failure would
# teach teammates that raising one gets them treated as stalled, and the next
# correct escalation would not be written at all.
#
# ===========================================================================
# ONE LINE, STATE-CHANGE DE-DUPLICATED — AND IT GETS LOUDER
# ===========================================================================
# systemMessage via scripts/lib/stop-hook-notice.sh, the only channel measured
# to reach the operator (see that file's table). A condition repeated under
# every turn is a condition the eye is trained to skip.
#
# But pure de-duplication has a failure mode this hook cannot afford: a
# condition that never changes is announced once and then never again, and THE
# ORIGINALS SAT FOR TWO DAYS. So the state key the predicate returns carries an
# AGE BUCKET per escalation (1h / 24h / 72h), and crossing a boundary is a state
# change. The ledger stop-hook-notice.sh de-duplicates against is keyed per
# SESSION, so a new session re-announces everything outstanding from scratch.
# Silence about an escalation only ever means "still what I told you this
# session" — never "nobody has one".
#
# NOT SCOPED TO A REPOSITORY, deliberately. A session seated in one repository
# with teammates in worktrees of another is the NORMAL shape of this operation;
# a notice that reported only escalations raised in the seat's own repository
# would hide the common case. It stands down only where the engine is not
# adopted at all, because the plugin loads in every directory on the machine.
#
# NOTE: hook REGISTRATIONS are snapshotted at session start; the script a
# registration names is read from disk at every firing. The model delivery
# added on 2026-09-28 changed no registration (still Stop), so it is live in a
# running session as soon as the installed copy is updated.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the escalation ledger, and this hook already
# announces when that predicate is unavailable — 'ESCALATION WATCH IS OFF' and
# 'ESCALATION WATCH PRODUCED NOTHING'. That is this same property implemented
# for a different input. The payload's session id and transcript path decide
# only WHICH escalations this session has already been told about; with none,
# every outstanding escalation is new once, and the text is the same text.
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
  hook: scripts/hooks/notice-escalations.sh
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

_ESC_LIB="$SCRIPT_DIR/../lib/escalations.sh"
[ -f "$_ESC_LIB" ] || exit 0
# shellcheck source=../lib/escalations.sh
. "$_ESC_LIB"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # No engine here, so no teammate of this operation is raising anything into
    # it. The plugin loads in every directory on the machine; a notice in each
    # would be the noise this engine already refused to make.
    exit 0
else
    # This hook believes it governs something and cannot tell what. It does NOT
    # exit 2 — a Stop hook that blocks on a broken install re-fires to the block
    # cap and strands the session. It says so instead, on the one channel that
    # reaches the operator, and stops.
    root_failure_banner "scripts/hooks/notice-escalations.sh" >&2
    stop_notice_init "notice-escalations.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "ESCALATION WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nobody is checking whether a teammate raised something nobody has read — run escalate.sh list by hand."
    exit 0
fi

stop_notice_init "notice-escalations.sh" "$ENTITY_ROOT" "$INPUT"

# NO PREDICATE, NO SILENCE. A wrapper that carried on quietly without its
# predicate would be a hook that is wired, hashed, executable and reading
# nothing — the exact shape that left Layer K green over a scanner that never
# ran. An absent reader and a clean report must never look the same.
if ! escalations_require; then
    stop_notice_abnormal "broken" \
        "ESCALATION WATCH IS OFF: $ESCALATIONS_BROKEN. No teammate escalation was read this turn; do not take this silence for an empty ledger — $(escalations_ledger) is the file, read it by hand."
    exit 0
fi

# AUDIT REPORTS THAT CHANGED AFTER THEIR FIXES WERE BRIEFED. The sweep raises an
# ordinary escalation for each new or changed finding, and the read below then
# delivers it in this same turn end. Why here and not in a commit hook: see
# scripts/lib/audit-report-watch.py. A failed sweep is said aloud, never skipped.
set +e
AUDIT_WATCH_ERR="$(python3 "$SCRIPT_DIR/../lib/audit-report-watch.py" 2>&1 >/dev/null)"
AUDIT_WATCH_RC=$?
set -e
if [ "$AUDIT_WATCH_RC" -ne 0 ]; then
    stop_notice_abnormal "audit-watch-failed" \
        "AUDIT REPORT WATCH FAILED (exit $AUDIT_WATCH_RC): $AUDIT_WATCH_ERR. A report rewritten with new findings would not be announced this turn."
fi

# ONE INVOCATION, THREE LINES: the state key, the count, and the sentence. The
# predicate owns all three, so this wrapper cannot end up telling the operator a
# different number from the one `escalate.sh list` prints.
set +e
SUMMARY="$(escalations_list hook-summary 2>/dev/null)"
RC=$?
set -e

if [ -z "$SUMMARY" ] || [ "$RC" -ge 2 ]; then
    stop_notice_abnormal "predicate-failed" \
        "ESCALATION WATCH PRODUCED NOTHING (exit $RC) over $(escalations_ledger). A teammate escalation raised now would not be announced; this silence is not a clean report — escalate.sh list has the detail."
    exit 0
fi

KEY="$(printf '%s\n' "$SUMMARY" | sed -n '1p')"
LINE="$(printf '%s\n' "$SUMMARY" | sed -n '3,$p' | tr '\n' ' ' | sed 's/ *$//')"

if [ "$KEY" = "clear" ]; then
    # CLEAR. Nothing is said unless the operator was previously told otherwise,
    # in which case he is owed the end of the story.
    stop_notice_normal \
        "ESCALATION WATCH: clear again — every teammate escalation has been acknowledged."
    exit 0
fi

# TWO AUDIENCES, ONE JSON OBJECT. The operator's line comes from the notice
# ledger exactly as before (de-duplicated, louder with age). The MODEL's half —
# every NEW escalation, by id, teammate, title, state and question — is added by
# the predicate as hookSpecificOutput.additionalContext, the one Stop field that
# reaches the lead's context. See "DELIVERY" in scripts/lib/escalations.py.
OPERATOR_JSON="$(stop_notice_abnormal "$KEY" "$LINE")"

set +e
FINAL="$(printf '%s' "$INPUT" \
    | ESCALATIONS_OPERATOR_JSON="$OPERATOR_JSON" \
      escalations_deliver "$ENTITY_ROOT/.claude/state/stop-hook-notices" 2>/dev/null)"
DRC=$?
set -e

if [ "$DRC" -eq 0 ]; then
    [ -z "$FINAL" ] || printf '%s\n' "$FINAL"
    exit 0
fi

# DELIVERY BROKE. The operator still gets his line, and is told — every turn,
# because there is no memory to de-duplicate a failure against — that the lead
# was NOT told. Silence here would be the 2026-09-28 defect again.
_shn_emit "$LINE | ESCALATION DELIVERY TO THE LEAD FAILED (exit $DRC): the model was not told about any new escalation this turn — run escalate.sh list."
exit 0
