#!/usr/bin/env bash
#
# unstarted-rows.mutation.sh — PROVES THE UNSTARTED-ROW SUITE CAN FAIL.
#
# 32 green ticks are evidence of nothing until somebody shows them turning red
# for the right reason. That is not a general anxiety here, it is a defect this
# suite ALREADY COMMITTED once: the first draft of the claim matcher tokenized
# branch names on punctuation, so "agent/row-11" became "agent", "row", "11"
# and NO branch claim could ever match. The case asserting that `row-1` must
# not claim row 11 passed anyway — perfectly green, over machinery that was
# not working at all — and it kept passing until a positive control was put
# beside it in the same run.
#
# So: take the shipped source, remove ONE property at a time, and assert that
#   1. unstarted-rows.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement that matched nothing gives
#      a green run that looks like a green run, which is the same trap again).
#
# Every mutant is a throwaway copy of the engine subtree. Nothing here touches
# the real tree.
#
# Run directly: scripts/hooks/unstarted-rows.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

# EXPORTED, not set per invocation, and that is deliberate. This harness runs
# the suite it mutates -- sometimes the sandboxed copy, sometimes the one in
# the real tree -- and that suite now invokes this harness at its end. Exported
# once here, the flag reaches every child however many invocation sites this
# file grows; set per-call, one missed site is an infinite regress.
export RICHOS_MUTATION_INNER=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t unstarted-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
# The needles arrive from a shell single-quoted string, so a multi-line target
# is written `a\nb`. Decoded here rather than in bash, where the quoting to
# carry a literal newline through three levels is its own bug.
old = old.replace("\\n", "\n")
new = new.replace("\\n", "\n")
with open(path, encoding="utf-8") as fh:
    src = fh.read()
if old not in src:
    sys.stderr.write("MUTATION TARGET ABSENT — the source has drifted:\n  %s\n" % old)
    sys.exit(3)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(src.replace(old, new, 1))
PYEOF

# mutant <name> <expected-failing-case> <rel-file> <old> <new> <why>
# --- concurrency ------------------------------------------------------------
# Each mutant builds its own sandbox and runs its named setup group, first
# unmodified and then mutated, so no two mutants share a path and none of them is ordered
# against another. They ran one at a time only because a `for` loop is what this
# harness was first written as. mutation-pool.sh bounds the fan-out, keeps the
# report in declaration order, and counts a KILLED mutant as a failure rather
# than letting it vanish from the tally. RICHOS_MUTANT_JOBS overrides the degree.
# shellcheck source=../lib/stopwatch.sh
. "$ENGINE_ROOT/scripts/lib/stopwatch.sh"
# shellcheck source=../lib/mutation-pool.sh
. "$ENGINE_ROOT/scripts/lib/mutation-pool.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

_mutant_body() {
    local name="$1" want="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib"
    cp "$ENGINE_ROOT/scripts/hooks/notice-unstarted-rows.sh" \
       "$ENGINE_ROOT/scripts/hooks/unstarted-rows.test.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/unstarted-rows.sh" "$ENGINE_ROOT/scripts/lib/unstarted-rows.py" \
       "$ENGINE_ROOT/scripts/lib/row-currency.sh" "$ENGINE_ROOT/scripts/lib/row-currency.py" \
       "$ENGINE_ROOT/scripts/lib/ceo-todos.sh" "$ENGINE_ROOT/scripts/lib/ceo-todos.py" \
       "$ENGINE_ROOT/scripts/lib/declaration-path.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" "$dir/scripts/lib/"
    cp "$ENGINE_ROOT/scripts/unstarted-rows-lint.sh" "$dir/scripts/"
    chmod +x "$dir/scripts/hooks/"*.sh "$dir/scripts/"*.sh

    bash "$dir/scripts/hooks/unstarted-rows.test.sh" --mutation-case "$want" >"$dir/control.txt" 2>&1
    local control_rc=$?
    if [ "$control_rc" -ne 3 ] || ! grep -qF "PASS  $want" "$dir/control.txt"; then
        printf '  FAIL  %s: unmutated named witness was not green (rc=%s)\n' "$name" "$control_rc"
        tail -15 "$dir/control.txt"
        return 1
    fi

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    bash "$dir/scripts/hooks/unstarted-rows.test.sh" --mutation-case "$want" >"$dir/out.txt" 2>&1
    local rc=$?
    if [ "$rc" -eq 0 ] || [ "$rc" -eq 3 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    # THE CASE MATCH IS A LITERAL WITH A BOUNDARY, NOT A BARE REGEX, and that
    # is a correction rather than a style. It used to be `grep -q "FAIL  $want"`,
    # in which an unescaped `.` matches ANY character: `g.` matched `FAIL  g1.`,
    # `p1.` matched `FAIL  p10.`, `C1.` matched `FAIL  C10.`. Demonstrated on
    # 2026-09-06 rather than suspected -- case `g.` of turn-manifest.test.sh was
    # deliberately weakened so it could not fail, and the harness still reported
    # `removing it turns g. red`, having matched a sibling case. The mutant was
    # genuinely killed every time; what was false was the claim about WHICH case
    # killed it, and that claim is the entire second clause of this harness's
    # contract. A wrong witness is exactly what root-contract's M1 turned out to
    # be. Metacharacters are escaped and the token must end at a non-alphanumeric
    # or end of line. The token is TRIMMED first, and that is not tidiness:
    # ceo-asks writes its cases as "C3. " with a trailing space, so the
    # boundary landed one character late and demanded a non-alphanumeric where
    # the label's first word begins. Five mutants went red for that reason on
    # the first run of this very change -- a red that looked exactly like a
    # finding and was not one.
    want_re="$(printf '%s' "$want" | sed 's/[[:space:]]*$//' | sed 's/[][\.*^$(){}?+|\/]/\\&/g')"
    # NO TRAILING BOUNDARY, and that is a correction to this very change made an
    # hour after it: the first version required the token to end at a
    # non-alphanumeric, which assumed every suite writes `FAIL  z1. some words`.
    # claim-roles' suite writes `FAIL  z1.landed-claim-about-a-dangling-commit`,
    # so the boundary demanded a space where a slug begins and turned ALL TWENTY
    # of its mutants into misfires -- twenty reds that looked exactly like
    # findings. The boundary was never what fixed anything: ESCAPING is. With
    # `.` taken literally, `z1.` cannot match `z1b.` and `C1.` cannot match
    # `C10.`, because the character after the prefix is a digit and not a dot.
    if ! grep -q "FAIL  ${want_re}" "$dir/out.txt"; then
        printf '  FAIL  %s — the suite went red, but NOT at %s (so the red is unrelated).\n' "$name" "$want"
        grep '  FAIL' "$dir/out.txt" | sed 's/^/          /'
        return 1
    fi
    printf '  PASS  %s — removing it turns %s red\n' "$name" "$want"
    return 0
}
# mutant <name> ... — a SUBMISSION, not an execution. The body above is unchanged
# except that it returns its verdict instead of incrementing a counter: it runs
# in a pool worker, which is a subshell, so an increment in there would mutate a
# copy and be discarded. The pool prints the bodies in DECLARATION order, so this
# harness's report is byte-identical to the serial one but for the durations.
mutant() {
    mut_pool_submit "$1" _mutant_body "$@"
}

echo "=== unstarted rows: every property, proven load-bearing by removing it ==="

H="scripts/hooks/notice-unstarted-rows.sh"
L="scripts/lib/unstarted-rows.sh"
P="scripts/lib/unstarted-rows.py"
T="scripts/unstarted-rows-lint.sh"

# --- 1. THE NOTICE ITSELF --------------------------------------------------
mutant no-notice "1a " "$H" \
    'stop_notice_abnormal "unstarted:$UR_UNSTARTED" \' \
    'stop_notice_normal "" \' \
    "the whole point: an unstarted row must end the turn with something on screen."

mutant unnamed-rows "1b " "$H" \
    'NAMES="$(printf '"'"'%s'"'"' "$UR_UNSTARTED"' \
    'NAMES="$(printf '"'"'%s'"'"' "" ' \
    "a count is a number to get used to; a name is a thing to go and start."

mutant vague-notice "1c " "$H" \
    'OPEN ROWS WITH NO RUNNING WORK RECORDED' \
    'FYI' \
    "the notice has to say what it knows, not merely that something is."

mutant overclaims "1e " "$H" \
    'OPEN ROWS WITH NO RUNNING WORK RECORDED —' \
    'OPEN ROWS WITH NO RUNNING WORK RECORDED, NOTHING RUNNING FOR THEM — Start one.' \
    "missing branch metadata proves neither that nothing runs nor that a row may start now."

mutant blocking "1d " "$H" \
    'stop_notice_abnormal "unstarted:$UR_UNSTARTED" \' \
    'exit 2; stop_notice_abnormal "unstarted:$UR_UNSTARTED" \' \
    "a Stop hook that refuses turns on a judgment it does not have gets switched off."

# --- 2. CLAIMS -------------------------------------------------------------
mutant no-branch-claim "2a " "$P" \
    '        if pat.search(wt.get("hay") or ""):' \
    '        if False and pat.search(wt.get("hay") or ""):' \
    "a live worktree working the row is the whole reason the notice can be quiet."

mutant no-claim-file "2c " "$P" \
    '        if rid in (wt.get("ids") or []):' \
    '        if False and rid in (wt.get("ids") or []):' \
    "the claim file is the escape hatch for a branch named before anybody knew the row."

mutant unbounded-claim "6b " "$P" \
    '(?![a-z0-9.]|[-_][0-9])' \
    '' \
    "row-11 would claim row 1: silence bought with a prefix match."

mutant wildcard-id "6a " "$P" \
    'r"(?<![a-z0-9])row[-_]?(?:%s)(?![a-z0-9.]|[-_][0-9])" % "|".join(alts)' \
    'r"(?<![a-z0-9])row[-_]?[0-9.]+(?![a-z0-9.])"' \
    "a claim that matches ANY id claims every row the moment one worktree exists."

# --- 3. DECLARED BLOCKERS --------------------------------------------------
mutant queue-blocker-ignored "3a " "$P" \
    '        elif names_nobody(blocker):' \
    '        elif True or names_nobody(blocker):' \
    "the Blocked by cell IS the declaration; ignoring it shouts about the CEO's own rows every turn."

mutant section-blocker-ignored "3e " "$P" \
    '        bm = BLOCKED_RE.search(body.split(WARRANT_MARK)[0])' \
    '        bm = None' \
    "a section row has no other way to say it is waiting on somebody."

# --- 4. THE POSITIVE PROBE -------------------------------------------------
mutant no-receipt "3b " "$L" \
    '    dir="$(dirname "$path")"' \
    '    return 0; dir="$(dirname "$path")"' \
    "silence with a receipt is a result; silence without one is a check that never ran."

mutant lying-receipt "3c " "$L" \
    'echo "rows-swept:    ${UR_N_ROWS:-0}"' \
    'echo "rows-swept:    0"' \
    "a receipt that cannot report the corpus size cannot prove the sweep was over anything."

# --- 5. LOUD, NEVER GREEN OVER NOTHING -------------------------------------
mutant header-guessed "4a " "$P" \
    '    if not headers:' \
    '    if False:' \
    "a table found by guessing is a table swept by guessing."

mutant declared-queue-missing "4d " "$L" \
    '            UR_BROKEN_REASON="$UNSTARTED_ROWS_DECLARATION in $UR_RECORD_REPO declares' \
    '            UR_STANDDOWN_REASON="$UNSTARTED_ROWS_DECLARATION in $UR_RECORD_REPO declares' \
    "somebody wrote down where the queue lives; finding nothing there is the loudest fact available."

mutant record-missing-quiet "4e " "$L" \
    '        UR_BROKEN_REASON="the working record declared by .ceo-todos' \
    '        UR_STANDDOWN_REASON="the working record declared by .ceo-todos' \
    "half the corpus gone is not a clean sweep of the other half."

mutant half-corpus-allowed "4f " "$P" \
    '    if qtext is None:' \
    '    if False:' \
    "THE QUEUE IS TWO FILES — the record's own corrected rule, and the failure it was written after."

mutant unknown-token-ok "4k " "$P" \
    '        if tok not in status_tokens:' \
    '        if False:' \
    "an unrecognized state classified as 'fine' by default is the whole defect."

mutant duplicate-ids-ok "4l " "$P" \
    '        if r["id"] in seen_ids:' \
    '        if False:' \
    "an ambiguous claim resolved in the quiet direction is a row nobody is working on, reported as covered."

mutant empty-queue-ok "4m " "$P" \
    '    if not rows:\n        fail("%s has a `Blocked by` header and NOT ONE ROW under it.' \
    '    if False:\n        fail("%s has a `Blocked by` header and NOT ONE ROW under it.' \
    "an emptied table reported as an empty queue is failure (1) of 2026-08-31, exactly."

mutant empty-section-ok "4n " "$P" \
    '    if not rows:\n        fail("%s section %s parsed to ZERO rows.' \
    '    if False:\n        fail("%s section %s parsed to ZERO rows.' \
    "section 3 is the half the lead did not read; a zero-row sweep of it must never be green."

mutant missing-predicate-ok "4o " "$L" \
    '        UR_BROKEN_REASON="scripts/lib/unstarted-rows.py is missing at $_UR_PY.' \
    '        UR_STANDDOWN_REASON="scripts/lib/unstarted-rows.py is missing at $_UR_PY.' \
    "an absent checker and a clean queue must never look the same."

mutant no-adoption-memory "4p " "$H" \
    'if [ -s "$ADOPTED" ]; then' \
    'if false; then' \
    "a queue renamed after it was swept would go quiet, which is the same lie by another route."

mutant section-missing-ok "4c " "$P" \
    '        if s not in seen:' \
    '        if False:' \
    "a section that moved takes every row in it out of sight."

# --- 6. DE-DUPLICATION -----------------------------------------------------
mutant constant-state-key "5b " "$H" \
    'stop_notice_abnormal "unstarted:$UR_UNSTARTED" \' \
    'stop_notice_abnormal "unstarted" \' \
    "if the key never changes, the notice never speaks again — including when a row is created."

# --- 7. THE LINT'S THREE ANSWERS -------------------------------------------
mutant lint-exit-clean "7a " "$T" \
    '    echo "  or \`**Blocked:** <who>\` in a $UR_RECORD_LABEL row."\n    exit 1' \
    '    echo "  or \`**Blocked:** <who>\` in a $UR_RECORD_LABEL row."\n    exit 0' \
    "a lint that exits 0 over unstarted work cannot be wired into anything."

mutant lint-broken-is-clean "7c " "$T" \
    '[ -n "$RECEIPT" ] && { UR_VERDICT="BROKEN"; ur_receipt "$RECEIPT"; }\n        exit 2 ;;' \
    '[ -n "$RECEIPT" ] && { UR_VERDICT="BROKEN"; ur_receipt "$RECEIPT"; }\n        exit 0 ;;' \
    "'nothing is unstarted' and 'nothing was read' are the two answers this exists to keep apart."

# --- 8. WHAT CLOSES A ROW ---------------------------------------------------
mutant residual-reopens-row "8a " "$P" \
    '        if is_done_cell and not struck:' \
    '        if is_done_cell != struck:' \
    "a struck row naming who owns its residual is FINISHED; refusing it shouts about closed work forever."

mutant done-cell-unstruck-ok "8b " "$P" \
    '        if is_done_cell and not struck:' \
    '        if False:' \
    "a row finished in one place and open in the other must not be picked between silently."

# --- 8. THE REST OF THE NAMED CASES, each proven load-bearing --------------
mutant lint-hides-rows "2b " "$T" \
    'if [ "$QUIET" -eq 0 ]; then' \
    'if false; then' \
    "a sweep nobody can inspect is a sweep nobody can believe."

mutant receipt-hides-claims "2d " "$L" \
    'echo "claimed:       ${UR_N_CLAIMED:-0}"' \
    'echo "claimed:       0"' \
    "the receipt has to say what the silence was bought with."

mutant anonymous-claim "2e " "$P" \
    'return "%s (row-claims.txt)" % wt.get("where", "a worktree")' \
    'return "a worktree"' \
    "'claimed' without a source is a claim nobody can check."

mutant receipt-hides-blockers "3d " "$L" \
    'echo "blocker-named: ${UR_N_DECLARED:-0}"' \
    'echo "blocker-named: 0"' \
    "a silence over declared blockers has to be distinguishable from a silence over nothing."

mutant broken-reads-clear "4b " "$H" \
    'UNSTARTED-ROW WATCH SWEPT NOTHING — the records did not parse:' \
    'UNSTARTED-ROW WATCH: clear again — the records did not parse:' \
    "a failure that reads like a clean sweep is the whole failure class, restated."

mutant announces-every-turn "5a " "$H" \
    'stop_notice_abnormal "unstarted:$UR_UNSTARTED" \' \
    'stop_notice_abnormal "unstarted:$UR_UNSTARTED:$RANDOM" \' \
    "a line under every turn is a line the eye is trained to skip, and then muted."

mutant thirty-minute-rung "5c " "$H" \
    'stop_notice_abnormal "unstarted:$UR_UNSTARTED" \' \
    '_rung() { stop_notice_abnormal_recurring "$1" "$2" 1800; }; _rung "unstarted:$UR_UNSTARTED" \' \
    "an unchanged set repeated on a timer is the administrative noise removed on 2026-10-24."

mutant lint-never-clean "7b " "$T" \
    '\nfi\nexit 0' \
    '\nfi\nexit 3' \
    "a lint that cannot say 'all accounted for' can never be wired into a gate."

# --- 8. A DECLARATION THAT NAMES NOBODY ------------------------------------
mutant blocked-nothing-silences "3f " "$P" \
    '        elif bm and names_ceo(bm.group("who")):' \
    '        elif bm:' \
    "a row saying in so many words that nothing blocks it must be the loudest row on the page, not a silent one (rows 3.19/3.20, 2026-09-02)."

mutant rich-blocker-silences "3i " "$P" \
    '        elif names_ceo(blocker):' \
    '        elif True:' \
    "a blocker naming Rich or earlier work is a hold on his work; only the CEO quiets a row (2026-10-22, no silent holds)."

# --- the verdict ------------------------------------------------------------
# Drained rather than accumulated: PASS/FAIL below come from the workers' exit
# codes, and a worker that left no exit code is counted as a FAILURE. The tally
# that follows is unchanged and still decides this harness's exit status.
# ADDITIVE, NOT ASSIGNMENT, and that is a correction found by measurement rather
# than by reading: mechanical-findings.mutation.sh scores one case BEFORE its
# mutants, and `PASS="$MUT_POOL_PASS"` silently discarded it. A tally that
# overwrites an earlier verdict is a green fraction over a case that ran and was
# thrown away. Harnesses that score nothing beforehand are unaffected, because
# adding to zero is assignment.
mut_pool_drain
# A harness that declared mutants and ran NONE must not exit 0 -- see
# mut_pool_require_submissions for the run where exactly that happened.
mut_pool_require_submissions "$(basename "$0")"
PASS=$(( PASS + MUT_POOL_PASS ))
FAIL=$(( FAIL + MUT_POOL_FAIL ))
mut_pool_report_line "$(( $(sw_now_ms) - MUT_WALL_T0 ))"
mut_pool_cleanup

echo ""
echo "  $PASS mutant(s) killed, $FAIL survived or misfired"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
