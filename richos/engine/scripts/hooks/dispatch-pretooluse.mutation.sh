#!/usr/bin/env bash
#
# dispatch-pretooluse.mutation.sh — EVERY PROPERTY OF THE RUNNER, PROVEN BY
# REMOVING IT.
#
# dispatch-pretooluse.test.sh is 25 green cases, and 25 green cases are evidence
# of nothing until somebody has watched each one go red for the RIGHT reason.
# This harness takes the shipped dispatcher, removes one property at a time in a
# throwaway copy of the engine, and asserts the suite fails AT THE NAMED CASE.
#
# THE MUTANTS THAT MATTER MOST ARE THE SILENCES.
# `missing-module-silent`, `oddrc-silent` and `first-block-wins` do not break a
# refusal — they make the dispatcher report a rule that DID NOT RUN exactly as
# it reports a rule that ran and found nothing. That is the failure this shape
# newly makes possible, it leaves no other symptom, and it is the reason twelve
# guards in one process was a risk worth this much apparatus. A harness that
# only proved the refusals would prove the half that cannot kill us.
#
# `memo-always-on` is here for a different reason: it is the only mutant that
# touches a shared library every other hook in the engine uses. The memo's whole
# safety argument is that it is inert unless one caller asks for it, and an
# argument nobody has watched fail is a hope.
#
# Invoked from dispatch-pretooluse.test.sh, so the runner that discovers
# *.test.sh runs this too. A harness nobody runs proves nothing about anything.
# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

mutation_begin "THE PreToolUse RUNNER" \
               "scripts/hooks/dispatch-pretooluse.test.sh"
# Each mutant stops at its named FAIL line (see mutation_focus): in this suite a
# printed FAIL line always ends the run red, so the rest of the run buys nothing.
mutation_focus stop-at-want

D="scripts/hooks/dispatch-pretooluse.sh"
R="scripts/lib/resolve-roots.sh"
U="scripts/lib/unevaluated-notice.sh"

# --- 1. A BLOCK IS A BLOCK -------------------------------------------------
mutant block-not-propagated "D2" "$D" \
    '        2) _DSP_RC=2 ;;' \
    '        2) : ;;' \
    "every refusal from every rule in both chains would be printed and then waved through — twelve guards reduced to twelve warnings, with the refusal text still on screen to make it look like they worked."

# --- 2. EVERY RULE REPORTS, NOT JUST THE FIRST TO REFUSE -------------------
# The host ran all twelve registrations whatever the first decided. Stopping at
# the first refusal sends the operator round the loop once per guard.
mutant first-block-wins "D3" "$D" \
    '        2) _DSP_RC=2 ;;' \
    '        2) _DSP_RC=2; break ;;' \
    "a call refused by two rules would report one, so fixing the named problem would produce a second refusal nobody was warned about."

# --- 3. A RULE THAT DID NOT RUN IS NOT A RULE THAT PASSED ------------------
# THE CENTRAL MUTANT. A module absent from disk is enforcement that is simply
# off, and the only symptom available is the announcement this removes.
mutant missing-module-silent "D6" "$D" \
    'printf '"'"'%s\n'"'"' "ERROR: dispatch-pretooluse.sh: rule module '"'"'$_dsp_m'"'"' is NOT PRESENT' \
    ': ignored "ERROR: dispatch-pretooluse.sh: rule module '"'"'$_dsp_m'"'"' is NOT PRESENT' \
    "deleting a rule from the manifest, or the rule itself from disk, would turn that guard off in total silence — the dispatcher would report the chain as clean."

# An absent rule has NO verdict. Giving it one in either direction is a guess:
# `0` is the silent hole above; `2` refuses every call in the chain until
# somebody notices, which is the failure that gets a guard waived and then
# switched off.
mutant missing-module-blocks "D6b" "$D" \
    '        cat "$_dsp_slot.err" >&2
        continue' \
    '        cat "$_dsp_slot.err" >&2
        _DSP_RC=2
        continue' \
    "one rule missing from disk would refuse every Bash call and every Write in the session, turning a broken install into a locked session rather than a reported one."

# --- 4. A RULE THAT CRASHED IS NOT A RULE THAT PASSED ----------------------
mutant oddrc-silent "D7" "$D" \
    'printf '"'"'%s\n'"'"' "ERROR: dispatch-pretooluse.sh: rule module '"'"'$_dsp_m'"'"' exited $_dsp_rc' \
    ': ignored "ERROR: dispatch-pretooluse.sh: rule module '"'"'$_dsp_m'"'"' exited $_dsp_rc' \
    "a rule that died of a syntax error, a missing python3 or an OOM kill would be indistinguishable from a rule that ran and found nothing."

# --- 5. ...AND IT IS NOT A BLOCK EITHER ------------------------------------
# A guard that blocks on its own internal error is a guard that gets waived, and
# habitual waiving is how a guard dies. This engine recorded three instances of
# exactly that in one day.
mutant oddrc-blocks "D7" "$D" \
    '        *)
            printf' \
    '        *)
            _DSP_RC=2
            printf' \
    "one rule crashing would refuse the tool call, so a bug in any of seventeen rules would stop the whole session rather than being reported."

# Visible diagnostics and preservation of a sibling verdict are separate contracts.
mutant failures-stderr-only "D7b" "$D" \
    'if [ -s "$_DSP_WORK/failures" ]; then' \
    'if false; then' \
    "failed rules would again be invisible to the operator because exit-zero stderr is hidden."

mutant failure-discards-sibling "D7c" "$D" \
    '    result = json.loads(raw) if raw else {}' \
    '    result = {}' \
    "a failure notice would erase a sibling rule permission decision and additional context."

# --- 6. THE SUBSHELL IS THE ISOLATION --------------------------------------
mutant options-leak "D9" "$D" \
    '        set +e +u +o pipefail' \
    '        set -u' \
    "a rule written against bash defaults would die on its first unset variable because a SIBLING rule wanted stricter options — twelve rules sharing one set of shell settings is exactly what running them in one process must not mean."

mutant no-subshell "D11" "$D" \
    '    (
        set +e +u +o pipefail
        . "$_DSP_DIR/$_dsp_m"
    ) < "$_DSP_WORK/payload" > "$_dsp_slot.out" 2> "$_dsp_slot.err" &' \
    '    . "$_DSP_DIR/$_dsp_m" < "$_DSP_WORK/payload" > "$_dsp_slot.out" 2> "$_dsp_slot.err"' \
    "the first rule to call exit would end the DISPATCHER, so every rule after it would be skipped in silence and the chain would report whatever that one rule decided."

# --- 7. EVERY RULE SEES THE WHOLE PAYLOAD ----------------------------------
mutant payload-truncated "D12" "$D" \
    'printf '"'"'%s'"'"' "$_DSP_INPUT" > "$_DSP_WORK/payload"' \
    'printf '"'"'%s'"'"' "" > "$_DSP_WORK/payload"' \
    "every rule would receive an empty payload, decide it is not its call, and exit 0 — seventeen guards passing everything, loudly reported as clean."

# --- 8. TWO ENVELOPES ARE NOT ONE ENVELOPE ---------------------------------
mutant stdout-concatenated "D14" "$D" \
    '    if [ "$_DSP_STDOUT_EMITTERS" -gt 1 ]; then' \
    '    if [ "$_DSP_STDOUT_EMITTERS" -gt 99 ]; then' \
    "two rules writing JSON would produce text that is not JSON; the host drops it without a word, so BOTH rules' results would vanish and nothing would say so."

# --- 9. THE DISPATCHER REFUSES RATHER THAN GUESSES -------------------------
mutant no-key-passes "D15" "$D" \
    'NO RULE IN THIS CHAIN EVALUATED THIS CALL. $_DSP_TAG" >&2
    exit 2' \
    'NO RULE IN THIS CHAIN EVALUATED THIS CALL. $_DSP_TAG" >&2
    exit 0' \
    "a registration that lost its chain-key argument would run no rule at all and report success — the whole PreToolUse layer off, with a green tick."

mutant empty-chain-passes "D16" "$D" \
    'This call was NOT checked by any rule in that chain. Manifest: $_DSP_MANIFEST $_DSP_TAG" >&2
    exit 2' \
    'This call was NOT checked by any rule in that chain. Manifest: $_DSP_MANIFEST $_DSP_TAG" >&2
    exit 0' \
    "a manifest that no longer names any rule for a chain would pass every call in that chain, silently."

mutant no-manifest-passes "D17" "$D" \
    '  It will not guess: every rule in the ${_DSP_CHAIN_KEY} chain is UNEVALUATED." >&2
    exit 2' \
    '  It will not guess: every rule in the ${_DSP_CHAIN_KEY} chain is UNEVALUATED." >&2
    exit 0' \
    "a broken install that lost the manifest would leave every PreToolUse rule off and every tool call reported clean."

# --- 9b. A CALLER'S SELECTION IS WHAT RUNS ----------------------------------
# The desktop app names its rules; ignoring the names would run the terminal
# operator's whole chain, his claim hooks included, in every user's app.
mutant selection-ignored "D17a" "$D" \
    '    _DSP_MODULES="${_dsp_kept# }"' \
    '    :' \
    "the app's lease would run every rule of the chain, the terminal operator's own included."

mutant unknown-rule-silent "D17c" "$D" \
    '        | tee -a "$_DSP_WORK/failures" >&2' \
    '        >/dev/null' \
    "a rule the app names and the manifest no longer lists would stop checking with nobody told."

# --- 10. THE MEMO IS INERT UNLESS ASKED FOR --------------------------------
# The only mutant here that reaches outside this change: resolve-roots.sh is
# sourced by 59 hooks, and the memo's entire safety argument is that none of
# them can be affected by it.
mutant memo-always-on "D26" "$R" \
    '    if [ "${_RR_MEMO_ENABLE:-0}" != "1" ]; then' \
    '    if [ "${_RR_MEMO_ENABLE:-0}" = "1" ]; then' \
    "every caller in the engine would silently cache its first root resolution — including the test suites that build one fixture tree, ask, rebuild it differently and ask again, which would then be answered about a repository that no longer exists."

mutant memo-unkeyed "D24" "$R" \
    '    _rr_key="${1:-}"$'"'"'\x1f'"'"'"$PWD"' \
    '    _rr_key="CONSTANT"$'"'"'\x1f'"'"'"$PWD"' \
    "a second payload naming a DIFFERENT repository would be served the first payload's answer, so a guard would enforce one repository's rules against another's tree."

mutant ue-memo-loses-reason "D25" "$U" \
    '        [ -n "$_UE_MEMO_REASON" ] && printf '"'"'%s'"'"' "$_UE_MEMO_REASON"
        return "$_UE_MEMO_RC"' \
    '        return "$_UE_MEMO_RC"' \
    "the second and later rules would be told a payload is unreadable without being told WHY, so the unevaluated-payload notice would lose the one word that says what went wrong."

mutation_end
