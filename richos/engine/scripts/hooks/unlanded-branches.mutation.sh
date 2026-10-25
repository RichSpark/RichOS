#!/usr/bin/env bash
#
# unlanded-branches.mutation.sh — PROVES THE UNLANDED-WORK SUITE CAN FAIL.
#
# Twenty-nine green ticks are evidence of nothing until somebody has watched
# them go red for the right reason. This suite is especially exposed to that,
# because MOST OF WHAT IT ASSERTS IS SILENCE — eight of its cases pass by
# observing that a hook said nothing, and a hook that is broken says nothing
# too. A suite made largely of silent cases will pass over a predicate that
# never runs at all.
#
# So: take the shipped source, remove ONE property at a time in a throwaway copy
# of the engine, and assert that
#   1. unlanded-branches.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied.
#
# The loop is scripts/lib/mutation-harness.sh. Run directly, or let
# unlanded-branches.test.sh run it, which it does.
#
# ===========================================================================
# ONE PROPERTY HAS NO SINGLE-MUTANT PROOF, AND IT IS NAMED RATHER THAN GLOSSED
# ===========================================================================
# Case U03 -- "a branch main already contains is not a finding" -- is protected
# TWICE over, independently: `for-each-ref --no-merged=<trunk>` never lists it,
# and `rev-list --count <b> --not <trunk>` returns 0 for it if it ever were
# listed. Remove either one alone and U03 still passes, correctly, because the
# other still holds. There is therefore no single-property mutation that turns
# U2 red, and writing one that removes both at once would be testing an edit
# rather than a property. The redundancy is deliberate and the gap in this
# harness is its consequence; U06 covers the count filter on its own, since only
# the count filter can see a branch that landed upstream.
#
# Run directly: scripts/hooks/unlanded-branches.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

mutation_begin "unlanded work" "scripts/hooks/unlanded-branches.test.sh"
# Each mutant stops at its named FAIL line (see mutation_focus): in this suite a
# printed FAIL line always ends the run red, so the rest of the run buys nothing.
mutation_focus stop-at-want

S="scripts/lib/unlanded-branches.py"
H="scripts/hooks/notice-unlanded-branches.sh"
C="scripts/hooks/guard-unresolved-claims.py"
L="scripts/unlanded-branches-lint.sh"

# --- 1. LIVENESS FROM THE WORKTREE LOCK -----------------------------------
# Without it, every running teammate's branch is a finding, and the notice
# names six or seven branches through the middle of every session. That is not
# a louder guard, it is a muted one a day later.
mutant no-worktree-lock "U11" "$S" \
    '            if wt_locked:' \
    '            if False:' \
    "a branch a live agent is working on would be reported as stranded, which is the noise that gets a notice ignored."

# --- 2. LIVENESS ACROSS REPOSITORIES --------------------------------------
# A hand-rolled cross-repository worktree takes NO git lock of its own, so its
# owner's liveness is only readable from the native isolation worktree over in
# the session's repository. This is the common case: 48 of 53 worktrees on this
# machine were hand-rolled when it was last measured.
mutant no-native-lock "U08" "$S" \
    '                    if native_locks.get((sid, teammate)):' \
    '                    if False:' \
    "a live teammate working cross-repository would be reported as stranded on every turn, because the tree it stands in carries no evidence about its own owner."

# --- 3. THE REMOTE TRUNK IS EXCLUDED TOO ----------------------------------
# The shape a stale local main produces: work landed and was pushed, local main
# is behind its upstream, and `main..branch` still shows commits.
mutant no-remote-exclusion "U06" "$S" \
    '    exclude = [trunk] + (["origin/" + trunk] if has_remote else [])' \
    '    exclude = [trunk]' \
    "a branch that landed and was pushed would be reported as unlanded whenever the local trunk sat behind its upstream."

# --- 4. THE REPOSITORY SET COMES FROM THE SESSION -------------------------
# Measured on this machine: the seat is femcboost and every teammate works in
# richos. A seat-only sweep is a sweep of the repository where nothing happens.
mutant no-ledger-discovery "U01" "$S" \
    '    if session_id:' \
    '    if False:' \
    "only the seat repository would be swept, and the cross-repository branches -- which is where the work actually is -- would be invisible."

# --- 5. THE NOTICE ACTUALLY SAYS IT ---------------------------------------
mutant notice-says-nothing "U10" "$H" \
    'stop_notice_abnormal "unlanded:$KEY" "$SUMMARY $HOOK_TAG"' \
    'stop_notice_normal "$SUMMARY $HOOK_TAG"' \
    "the whole point: finished work outside main must end the turn with something on screen."

# --- 6. THE CLAIM CLASS READS POLARITY ------------------------------------
# The defect that produced g11/g12/g13 in miniature: a sentence saying the
# opposite is reported for agreeing with the repository.
mutant claim-ignores-polarity "C06" "$C" \
    '               if pol == "positive"]' \
    '               if pol or True]' \
    "a reply correctly saying nothing has landed yet would be reported for saying so -- the gate contradicting accuracy, which is the one thing it must never do."

# --- 7. QUOTATIONS ARE MASKED ---------------------------------------------
# Worth 13 points of measured precision on its own: the orchestrator quotes
# this very failure constantly, and an unmasked matcher reads every quotation
# as a fresh claim.
mutant claim-reads-quotations "C07" "$C" \
    '    masked = COMPLETE_QUOTED.sub(_blank, text)' \
    '    masked = text' \
    "quoting the sentence that caused the incident would be treated as making the claim again, which is 13 points of precision and the whole reason this signal is worth reading."

# --- 8. AN UNEVALUATED CHECK SAYS SO --------------------------------------
mutant unevaluated-is-silent "C08" "$C" \
    '            complete_unevaluated = why' \
    '            complete_unevaluated = ""' \
    "a completeness claim made while the check that could contradict it did not run would pass in the SAME silence a clean repository produces -- the absence of a finding made indistinguishable from the absence of a check."

# --- 9. THE ONE LINE THAT LEAVES THE TRANSCRIPT ---------------------------
# stderr is filed into the transcript and shown to the operator nowhere. A
# report he cannot see is not a report.
mutant no-operator-channel "C02" "$C" \
    '    sys.stdout.write(json.dumps({"suppressOutput": True,{NL}                                 "systemMessage": text}) + "\n")' \
    '    return None' \
    "the finding would go to stderr only, where stop-hook-notice.sh measured that the operator sees it nowhere -- the report exists and reaches nobody."

# --- 10. THE ABSTENTION IS CARRIED AS DATA --------------------------------
# A repository with no recorded integration branch cannot be answered for. That
# fact used to travel ONLY as the word `partial` in STATUS, every caller read N
# instead, and the notice printed "clear again" over an unlanded cc/ branch.
mutant abstention-not-counted "U18" "$S" \
    '    result["n_not_examined"] = len(result["not_examined"])' \
    '    result["n_not_examined"] = 0' \
    "the count every caller branches on would say nothing went unread, so an unexamined repository would be reported with the identical words as one read end to end."

# --- 11. AND THE NOTICE ANNOUNCES IT --------------------------------------
mutant abstention-is-a-no-op "U19" "$H" \
    'if [ "${NOTEXAMINED:-0}" != "0" ] && [ -n "${NOTEXAMINED:-}" ]; then' \
    'if false; then' \
    "this is the shipped defect exactly: treating the abstention as a no-op and falling through to the zero-findings branch, which says the word 'clear' about a repository nothing looked in."

# --- 12. THE LINT'S TWO CODES DO NOT COLLAPSE -----------------------------
mutant lint-unread-exits-zero "L05" "$L" \
    '[ "${NOTEX:-0}" = "0" ] || exit 2' \
    ': # mutated' \
    "'nothing is unlanded' and 'nothing was read' would share exit 0 again -- the collapse this script's own exit-code table forbids, and the one it was shipping."

# --- 13. A CONDITION IS NOT A REPORT ---------------------------------------
# "Nightly 75 builds once all the work you named has landed." told the operator
# YOU CALLED IT DONE every turn it was repeated (2026-10-25).
mutant claim-reads-conditions-as-claims "C09" "$C" \
    '                if pol == "positive" and conditional_completion(probe, mt.start()):' \
    '                if False:' \
    "a sentence about what happens once the work lands would be reported as a claim that it has, every turn it is repeated."

# --- 14. RETAINED STOPPED WORK IS NOT OWED WORK ----------------------------
mutant stops-not-honored "U23" "$C" \
    '            state = "retained"' \
    '            state = "owed"' \
    "work the CEO stopped and ordered kept would be reported as owed by every completeness claim, as if the report had lied."

# --- 15..16. THE SENTENCE'S OWN SCOPE (second review rv-...ee69ea88-3482) ---
# (A condition anywhere in the sentence is mutant 13's property, C09..C19.)
mutant repository-scope-ignored "C16" "$C" \
    '    repos = {lab for lab in labels if lab and _names(sentence, lab)}' \
    '    repos = set()' \
    "a true claim about one repository would be blamed for another repository's owed branch."
mutant named-branch-ignored "C18" "$C" \
    '    if named:{NL}        return' \
    '    if False:{NL}        return' \
    "a branch the report names as landed would be let off whenever its owner record is missing or still running."

mutation_end
