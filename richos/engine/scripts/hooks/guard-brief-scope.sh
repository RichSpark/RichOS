#!/usr/bin/env bash
#
# guard-brief-scope.sh — BLOCKING PreToolUse guard on the Agent tool.
#
# REFUSES A DISPATCH AGAINST A SPEC-GOVERNED BODY OF WORK WHEN EVERY SPEC POINT
# THE BRIEF NAMES IS ALREADY RECORDED GREEN BY A RUN — AND WHEN THE BRIEF NAMES
# NO SPEC POINT AT ALL.
#
# ===========================================================================
# WHY A GUARD AND NOT A REVIEWER
# ===========================================================================
# The CEO has already run the reviewer experiment, at his own expense, and
# recorded the result. He supervised every engineer brief himself; then he put
# TWO ADVERSARIAL REVIEWERS on every brief BEFORE every round; his verdict is
# that this is the only reason the job was completed at all. AND IT STILL
# FAILED: by round 7 the work had drifted off his fourteen points, and the thing
# that caught it was him noticing a round 10 was being proposed after being told
# round 7 or 8 would be the last.
#
# The reason is type K of the lifecycle failure record, and it is measured:
# his fourteen points contain ZERO adversarial words, and "every reviewer brief
# Rich wrote instructed them to attack the machinery." THE REVIEWERS WERE
# AUDITING AGAINST A PREMISE THE LEAD HAD ALREADY CORRUPTED. A reader is briefed
# by the lead. Adding readers adds people who are briefed by the lead.
#
# So this guard reads nothing the lead wrote except ONE declaration, and decides
# against two artifacts he did not author: THE CEO'S SPEC and A RUN'S OUTPUT.
#
# ===========================================================================
# WHAT IT REFUSES, AND WHY EACH ONE IS A FACT AND NOT A JUDGMENT
# ===========================================================================
#   NO-ANCHOR         the brief names no point of the spec that governs this
#                     work. Round 9's item 1 (a gutted probe reads GREEN) is in
#                     none of the CEO's fourteen sentences and could not have
#                     been anchored to one.
#   SPEC-SATISFIED    every point it names is recorded GREEN. A round exists to
#                     turn a red point green; this one has no spec-derived
#                     reason to exist. Round 9's own third line says
#                     `14 green, 0 red`.
#   VERDICT-STALE     the measurement predates the branch tip it judges.
#   SPEC-CHANGED      the CEO's page is not the text this work was recorded
#                     against, so nothing derived from it can be trusted.
#   NO-SUCH-POINT     the brief names a point the spec does not have.
#   POINT-UNMEASURED  the brief names a point the run produced no verdict for.
#                     An unmeasured point is not a red one.
#   WRONG-WORK        the brief declares a different body of work than the one
#                     recorded for the repository. Renaming the work is the
#                     clean way out of a spec's reach, so it is refused here and
#                     becomes the CEO's to start.
#
# AND, FOR EVERY DISPATCH WHETHER OR NOT A SPEC IS RECORDED (ass-kicker/brief-done.py,
# added 2026-09-28 after andy-opus-dfix1 spent 75 minutes and 389k tokens on three
# defects main had carried fixes for since 2026-09-24):
#   ALREADY-DONE      the brief names items (D02, I05, 3.41) or a teammate the
#                     work was "routed to", and the main branch already has
#                     commits naming them that change more than records.
#   CONTRADICTED      the brief says a quoted read-only command shows nothing,
#                     and run now it prints lines (or cannot run at all).
#   Escape: one `already-done-ack: <reason>` line, logged to
#   .claude/state/already-done-acks.log on a live spawn.
#
# ===========================================================================
# WHO THIS BLOCKS, AND WHAT IT COSTS EVERYONE ELSE
# ===========================================================================
# THE LEAD, on one kind of dispatch: work against a body of work whose spec has
# been RECORDED. Nothing else. A body of work with no recorded spec — which is
# every body of work at the moment this ships — takes the silent exit 0 below
# and pays a `read_json` of a file the spawn path already reads.
#
# ===========================================================================
# THE ESCAPE HATCH, AND WHY IT IS THE CEO'S AND NOT THE LEAD'S
# ===========================================================================
#     scope-ceo-word: <what he said, and when>
#
# on its own line in the spawn prompt. The refusal text hands the lead the exact
# question to put to him, because the question is one he demonstrably wants —
# it is the question he asked himself, unprompted, and it is the only thing in
# this whole body of work that ever caught the drift.
#
# THE HONEST RESIDUE, stated here rather than discovered later: NOTHING IN THIS
# HOOK CAN VERIFY THAT HE SPOKE. A lead who writes a sentence he was never told
# gets past it. What the hatch buys is that the evasion has to be a fabricated
# quotation of the CEO, written into a log he can read, rather than a silent
# dispatch that leaves no trace at all. Every use is logged to
# .claude/state/brief-scope-acks.log.
#
# ===========================================================================
# WHY PreToolUse[Agent]
# ===========================================================================
# The scope of a dispatch is decided at exactly one moment — the spawn — and
# this is the last event that sees it before an engineer starts building. The
# alternatives were considered and rejected for the reasons the census of
# 2026-09-14 measured: PostToolUse REFUSES NOTHING (verified in this repository,
# notice-claim-capability.sh lines 248-262), so a scope check there is failure
# type AB, a detector with no authority. A Stop-time notice reports a round that
# has already been built. A lint over the brief file is an honor-system check,
# and this engine has retired every one of those.
#
# Exit 2 with the reason on stderr is the only channel the host renders as a
# refusal reason. Exit 0 on anything it cannot evaluate: a guard that fails
# closed on its own inability protects nothing and teaches its reader to skip it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$HERE/../../ass-kicker/brief-scope.py"

PAYLOAD="$(cat 2>/dev/null)"

# A PAYLOAD THIS GUARD NEVER READ IS NOT A DISPATCH IT APPROVED, and the bare
# `[ -n "$PAYLOAD" ] || exit 0` that stood here could not tell the two apart. It
# took the SAME silent exit 0 on an empty payload, on a truncated one and on one
# that is not JSON, as it takes on a spawn it read in full and found in scope —
# which is the absence of a CHECK wearing the costume of the absence of a
# FINDING, the property scripts/lib/unevaluated-notice.sh exists for.
#
# This guard is emphatically NOT payload-independent: every word of its verdict
# comes out of the prompt in that payload, so the `# UNEVALUATED-PAYLOAD-EXEMPT:`
# declaration would be a false claim about this file. It announces instead. The
# exit stays 0 either way — failing closed on its own inability protects nothing,
# which is the same rule the python3/library branches below already follow.
_UE_LIB="$HERE/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-brief-scope.sh" "$PAYLOAD" \
        "${CLAUDE_PROJECT_DIR:-}" \
        "whether this dispatch's brief stays inside the scope the CEO actually set"
fi
[ -n "$PAYLOAD" ] || exit 0

# A GUARD THAT CANNOT RUN SAYS SO. It still exits 0 — refusing a spawn because
# this file is broken protects nothing — but it does not do it SILENTLY, because
# a check that passes for a reason unrelated to what it checks is failure type Z,
# and this guard shipped with exactly that defect: `$ENGINE/scripts/brief-scope.py`
# resolved one directory too deep, the `[ -f ]` test took the quiet exit, and
# every case in the suite that drove the library directly still passed. Only the
# two cases that drive the HOOK caught it.
if ! command -v python3 >/dev/null 2>&1; then
    echo "guard-brief-scope: python3 is not on PATH; the scope check did NOT run." >&2
    exit 0
fi
if [ ! -f "$LIB" ]; then
    echo "guard-brief-scope: $LIB is missing; the scope check did NOT run." >&2
    exit 0
fi

TMP="$(mktemp "${TMPDIR:-/tmp}/brief-scope.XXXXXX")" || exit 0
printf '%s' "$PAYLOAD" >"$TMP"
trap 'rm -f "$TMP"' EXIT

OUT="$(python3 "$LIB" check "$TMP" 2>/dev/null)"; RC=$?

# ---- the acknowledgement log ----------------------------------------------
# Written whenever the dispatch went through on the CEO's word, so a claim that
# he said something survives the turn that claimed it.
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "on the CEO's word:"; then
    LOGDIR="${CLAUDE_PROJECT_DIR:-$PWD}/.claude/state"
    mkdir -p "$LOGDIR" 2>/dev/null && \
        printf '%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(printf '%s' "$OUT" | tr '\n' ' ')" \
        >>"$LOGDIR/brief-scope-acks.log" 2>/dev/null
fi

[ "$RC" -eq 2 ] || exit 0

{
    echo ""
    printf '%s\n' "$OUT"
    echo ""
} >&2
exit 2
