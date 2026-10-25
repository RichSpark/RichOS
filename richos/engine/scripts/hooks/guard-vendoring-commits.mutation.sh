#!/usr/bin/env bash
#
# guard-vendoring-commits.mutation.sh — PROVES THE VENDORING SUITE CAN FAIL.
#
# Fifty-nine green ticks are evidence of nothing until somebody has watched
# them turn red for the right reason, and this project has been burned by
# exactly that: a secrets scanner whose canary passed green while the scanner
# itself never ran. This guard has the same shape of hiding place. A registry
# lookup that quietly matches everything, an `exit 2` that became `exit 0`, a
# command classifier that stops recognizing `git commit` — every one of those
# leaves the hook wired, registered, executable and PASSING, over zero
# enforcement.
#
# So: take the SHIPPED source, remove ONE property at a time in a throwaway
# copy of the engine, and assert that the suite fails AT THE NAMED CASE.
#
# The harness is scripts/lib/mutation-harness.sh — one loop, shared, because
# seven private copies of it is the defect this engine keeps finding in itself.
#
# Run directly, or let guard-vendoring-commits.test.sh run it (which it does,
# because a harness nobody runs proves nothing about anything).

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/mutation-harness.sh
. "$ENGINE_ROOT/scripts/lib/mutation-harness.sh"

mutation_begin "the vendoring guard" "scripts/hooks/guard-vendoring-commits.test.sh"
# Each mutant stops at its named FAIL line (see mutation_focus): in this suite a
# printed FAIL line always ends the run red, so the rest of the run buys nothing.
mutation_focus stop-at-want

G="scripts/hooks/guard-vendoring-commits.sh"
L="scripts/lib/vendored-material.sh"

# --- 1. IT REFUSES AT ALL --------------------------------------------------
mutant refuses-to-refuse "A1. " "$G" \
    '    echo "(hook: scripts/hooks/guard-vendoring-commits.sh)"{NL}} >&2{NL}exit 2' \
    '    echo "(hook: scripts/hooks/guard-vendoring-commits.sh)"{NL}} >&2{NL}exit 0' \
    "the guard would find every unrecorded vendoring, print it in full, and let the commit through — a warning wearing a guard's clothes, which is the 2026-08-30 sweep's failure mode exactly."

# --- 2. THE REGISTRY IS WHAT DECIDES ---------------------------------------
mutant everything-looks-recorded "A1. " "$L" \
    '    [ -n "$best" ] || return 1' \
    '    [ -n "$best" ] || return 0' \
    "vm_covering would report every path as already recorded, so the guard would run, look, and find nothing, forever."

mutant nothing-is-governed "A1. " "$L" \
    '        if [ "$rel" = "$pre" ] || [ "${rel#"$pre"/}" != "$rel" ]; then' \
    '        if false; then' \
    "no path would be governed, so no addition would ever be checked — the exact state this repository was in until 2026-09-04."

# --- 3. THE COMMAND CLASSIFIER ---------------------------------------------
# THE HEADLINE MUTANT. Its `new` text is the classifier this guard's sibling
# guard-row-currency-commits.sh carries today, and restoring it here is not a
# hypothetical regression: it is the bug this guard was found to have on its
# first smoke run, where three negative cases returned 0 because a multi-line
# commit message was cut at the blank line and no `git commit` was recognized
# at all.
mutant naive-segment-split "A7. " "$G" \
    'segments = top_level_segments(cmd)' \
    'segments = re.split(r"(?:\\|\\||&&|[;\\n|])", cmd)' \
    "a multi-line commit message — the house style — would be cut mid-quote, both halves would fail to parse, and EVERY such commit would pass unexamined."

mutant commit-verb-ignored "A1. " "$G" \
    '    if sub != "commit":{NL}        continue' \
    '    if sub != "no-such-verb":{NL}        continue' \
    "no git subcommand would ever match, so the guard would classify every command as PASS and never run its check."

# --- 4. THE ESCAPE HATCH MUST CARRY A REASON -------------------------------
# WHY THIS MUTANT IS COARSE, AND WHY THAT IS THE FINDING RATHER THAN A
# SHORTCUT. The first attempt removed only the extraction pattern's
# "at least one non-blank character" requirement, and the suite stayed GREEN —
# correctly. A bare marker is caught THREE times over: by the pattern, by the
# `if not r` branch, and by the 30-character floor. That redundancy is a good
# property, and a mutant that does not know about it reports a false negative
# dressed as a finding. So this one removes the whole validation instead, which
# is the property the case actually names: a marker's REASON is judged at all.
mutant reason-never-validated "A8. " "$G" \
    'ACK_WHY="$(_vendoring_reason_problem "$ACK_REASON" 2>/dev/null || printf '"'"'the justification could not be evaluated.'"'"')"' \
    'ACK_WHY=""' \
    "any 'vendoring-ack:' line at all would exempt anything, so the escape hatch becomes an off switch anyone can type."

mutant no-length-floor "A9. " "$G" \
    'MIN_CHARS = 30{NL}MIN_WORDS = 5{NL}MIN_CONTENT = 3' \
    'MIN_CHARS = 0{NL}MIN_WORDS = 0{NL}MIN_CONTENT = 0' \
    "'it is fine' would be accepted as a justification, which is a marker with three words of decoration rather than a reason."

mutant no-deferral-detection "A10. " "$G" \
    '    elif hit:' \
    '    elif False:' \
    "'I will add the entry later' would be accepted as a justification — which is not a reason, it is the behavior the registry replaces, and it is what fourteen skills did for six months."

# --- 5. THE SUBJECT IS ADDITIONS, AGAINST THE RIGHT BASE -------------------
# Aimed at the AMEND BRANCH, not at the HEAD~1 lookup inside it. Removing only
# the lookup leaves BASE empty, which falls through to the empty-tree base —
# every tracked file then reads as added and the unrecorded one is still caught,
# so the suite stays green for an unrelated reason. The property is that an
# amend is recognized AS an amend.
mutant amend-not-widened "A14. " "$G" \
    'if [ "$AMEND" = "1" ]; then{NL}    if git -C "$REPO" rev-parse --verify -q HEAD~1' \
    'if false; then{NL}    if git -C "$REPO" rev-parse --verify -q HEAD~1' \
    "an --amend would be diffed against the commit it is rewriting, so its own additions would look like nothing at all and a vendoring could be laundered through an amend."

mutant modifications-refused "B6. " "$G" \
    '--diff-filter=A -z "$BASE"' \
    '--diff-filter=AM -z "$BASE"' \
    "every EDIT to a governed file would be refused as if it were a new vendoring — the cries-wolf failure that gets a guard switched off within a day."

# --- 6. PATH BOUNDARIES, NOT STRING PREFIXES -------------------------------
mutant governed-by-substring "B5. " "$L" \
    '        if [ "$rel" = "$pre" ] || [ "${rel#"$pre"/}" != "$rel" ]; then{NL}            VM_PREFIX="$pre"' \
    '        if [ "$rel" = "$pre" ] || [ "${rel#"$pre"}" != "$rel" ]; then{NL}            VM_PREFIX="$pre"' \
    "engine/skills-archive/ would be judged as if it were inside engine/skills/, and a guard that refuses commits in a directory it does not govern is a guard somebody removes."

mutant covered-by-substring "B17. " "$L" \
    '        if [ "$rel" = "$p" ] || [ "${rel#"$p"/}" != "$rel" ]; then' \
    '        if [ "$rel" = "$p" ] || [ "${rel#"$p"}" != "$rel" ]; then' \
    "an entry for engine/skills/known would silently cover engine/skills/known-extra — a NEW, unrecorded vendoring passing on the strength of a shared prefix."

# --- 7. A MALFORMED REGISTRY IS FAIL-CLOSED, NEVER IGNORED -----------------
mutant field-count-unchecked "C1. " "$L" \
    '        if [ "$fields" -ne 10 ]; then' \
    '        if [ "$fields" -eq -1 ]; then' \
    "a row that lost a tab still LOOKS like a row, and every field after the gap shifts by one — a license read out of the holder column is a wrong answer wearing the right shape."

mutant unknown-origin-allowed "C2. " "$L" \
    '        if [ "$known" -ne 1 ]; then' \
    '        if false; then' \
    "an entry whose origin nothing recognizes would be honored, and dialect-guard.sh would then ask that entry a question it cannot answer."

mutant no-governed-paths-ok "C3. " "$L" \
    '    if [ -z "${VM_REDISTRIBUTABLE_PATHS//[[:space:]]/}" ]; then' \
    '    if false; then' \
    "a registry governing nothing would report itself healthy — declared, present, and enforcing zero, which looks identical to a clean run."

mutant empty-registry-ok "C4. " "$L" \
    '    if [ "$VM_ENTRY_COUNT" -eq 0 ]; then' \
    '    if false; then' \
    "an emptied registry would silently govern nothing rather than refusing, and deleting every line would be an invisible way to switch the contract off."

mutant unknown-setting-ignored "C5. " "$L" \
    '            [A-Z]*=*)' \
    '            NEVER-MATCHES-THIS=*)' \
    "a mistyped setting name — REDISTRIBUTABLE_PATH for REDISTRIBUTABLE_PATHS — would be read as an entry line or skipped, and the real setting would silently be blank."

mutant broken-registry-fails-open "C1. " "$G" \
    '    *) vm_broken_banner "scripts/hooks/guard-vendoring-commits.sh" "$VM_BROKEN_REASON" >&2{NL}       echo "(hook: scripts/hooks/guard-vendoring-commits.sh)" >&2{NL}       exit 2 ;;' \
    '    *) vm_broken_banner "scripts/hooks/guard-vendoring-commits.sh" "$VM_BROKEN_REASON" >&2{NL}       echo "(hook: scripts/hooks/guard-vendoring-commits.sh)" >&2{NL}       exit 0 ;;' \
    "a declared-but-unreadable registry would print a banner and then wave the commit through — enforcement gone, with a reassuring message on top."

# --- 8. TWO DECLARATIONS ARE BROKEN, NEVER A CHOICE ------------------------
mutant two-copies-tolerated "C6. " "$L" \
    '        *) VM_BROKEN_REASON="$DECL_BROKEN_REASON"; return 2 ;;{NL}    esac{NL}    VM_REGISTRY="$DECL_PATH"' \
    '        *) VM_BROKEN_REASON="$DECL_BROKEN_REASON"; return 1 ;;{NL}    esac{NL}    VM_REGISTRY="$DECL_PATH"' \
    "a repository carrying two copies of its declaration would be treated as declaring nothing, so the contract switches itself off in exactly the state where somebody believes it is live."

# --- 9. AN UNDECLARED REPOSITORY IS SILENT, NOT MERELY PERMITTED ----------
mutant undeclared-announces "B16. " "$G" \
    '    1) exit 0 ;;   # this repository declares no vendoring contract' \
    '    1) echo "NOTE: no vendoring registry here" >&2; exit 0 ;;' \
    "every commit in every unrelated repository on the machine would carry a line about a contract that repository never adopted — noise with a serious face on, and the first thing an adopter switches off."

mutation_end
