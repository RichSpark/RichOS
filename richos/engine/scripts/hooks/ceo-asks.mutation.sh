#!/usr/bin/env bash
#
# ceo-asks.mutation.sh — PROVES THE CEO-ASK SUITE CAN FAIL.
#
# 46 green ticks are evidence of nothing until somebody shows them turning red
# for the right reason. That is not a general anxiety: the whole mechanism this
# suite covers exists because a set of guards was GREEN over a session in which
# the CEO was never asked anything. A suite that passed over a gutted gate would
# be the same defect, one layer in.
#
# So: take the SHIPPED source, remove ONE property at a time, and assert that
#   1. ceo-asks.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement that matched nothing gives a
#      green run that looks like a green run, which is the same trap again).
#
# Every mutant is a throwaway copy of the engine subtree. Nothing here touches
# the real tree.
#
# Run directly: scripts/hooks/ceo-asks.mutation.sh
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
SANDBOX="$(cd "$(mktemp -d -t ceo-asks-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
# The needles arrive from a shell single-quoted string, so a multi-line target
# is written `a\nb`. Decoded here rather than in bash, where the quoting to
# carry a literal newline through three levels is its own bug. A CONSEQUENCE,
# stated because it bit once: a needle may not contain the two characters
# backslash-n as data.
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
# Each mutant builds its own sandbox under $SANDBOX/<name> and runs a whole
# suite against it, so no two mutants share a path and none of them is ordered
# against another. They ran one at a time only because a `for` loop is what this
# harness was first written as. mutation-pool.sh bounds the fan-out, keeps the
# report in declaration order, and counts a KILLED mutant as a failure rather
# than letting it vanish from the tally. RICHOS_MUTANT_JOBS overrides the degree.
# shellcheck source=../lib/stopwatch.sh
. "$ENGINE_ROOT/scripts/lib/stopwatch.sh"
# shellcheck source=../lib/mutation-pool.sh
. "$ENGINE_ROOT/scripts/lib/mutation-pool.sh"
# shellcheck source=../lib/mutant-suite-run.sh
. "$ENGINE_ROOT/scripts/lib/mutant-suite-run.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

_mutant_body() {
    local name="$1" want="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib" "$dir/hooks"
    cp "$ENGINE_ROOT/scripts/hooks/notice-ceo-asks.sh" \
       "$ENGINE_ROOT/scripts/hooks/guard-ceo-ask-first.sh" \
       "$ENGINE_ROOT/scripts/hooks/notice-ceo-unasked.sh" \
       "$ENGINE_ROOT/scripts/hooks/session-start-ceo-ask.sh" \
       "$ENGINE_ROOT/scripts/hooks/ceo-asks.test.sh" \
       "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" \
       "$ENGINE_ROOT/scripts/hooks/install.sh" \
       "$ENGINE_ROOT/scripts/hooks/notice-unstarted-rows.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/ceo-asks.sh" "$ENGINE_ROOT/scripts/lib/ceo-asks.py" \
       "$ENGINE_ROOT/scripts/lib/ceo-todos.sh" "$ENGINE_ROOT/scripts/lib/ceo-todos.py" \
       "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" \
       "$ENGINE_ROOT/scripts/lib/cold-open-prompt.md" "$dir/scripts/lib/"
    cp "$ENGINE_ROOT/scripts/ceo-asks-status.sh" "$dir/scripts/"
    cp "$ENGINE_ROOT/hooks/hooks.json" "$dir/hooks/"
    chmod +x "$dir/scripts/hooks/"*.sh "$dir/scripts/"*.sh

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # The run stops at the mutant's named FAIL line (scripts/lib/mutant-suite-run.sh,
    # hunt part 3 finding 9): in this suite a printed FAIL line always ends the run
    # red, and the stop matches the TRIMMED want as a fixed string, exactly what the
    # escaped witness test below matches, so it is never looser than that test.
    mutant_suite_run "$dir/out.txt" "$(printf '%s' "$want" | sed 's/[[:space:]]*$//')" bash "$dir/scripts/hooks/ceo-asks.test.sh"
    if [ "$MUTANT_STOPPED" = 1 ]; then
        printf '  PASS  %s — removing it turns %s red (stopped at that line)\n' "$name" "$want"
        return 0
    fi
    local rc=$MUTANT_RC
    if [ "$rc" -eq 0 ]; then
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

echo "=== the CEO-ask gate: every property, proven load-bearing by removing it ==="

PY="scripts/lib/ceo-asks.py"
LIB="scripts/lib/ceo-asks.sh"
W="scripts/hooks/notice-ceo-asks.sh"
G="scripts/hooks/guard-ceo-ask-first.sh"
N="scripts/hooks/notice-ceo-unasked.sh"
S="scripts/hooks/session-start-ceo-ask.sh"

# --- 1. THE ANTI-GAMING PROPERTY -------------------------------------------
# First, because it is the one the whole design rests on.
mutant everything-matches "C3. " "$PY" \
    'return hits >= needed, score, hits, needed, "title"' \
    'return True, score, hits, needed, "title"' \
    "if any question matches something, one junk question per session clears the gate forever."

mutant worker-discharges "C4. " "$W" \
    '"discharges": bool(verdict == "MATCH" and not agent_id),' \
    '"discharges": bool(verdict == "MATCH"),' \
    "any subagent asking any clarifying question would hand the session a free discharge."

mutant no-session-filter "C5. " "$LIB" \
    'if not sid or str(rec.get("session_id") or "") != sid:' \
    'if False:' \
    "a question put to him yesterday did not reach the person who opened a session today."

mutant any-app-run "C5d. " "$LIB" \
    'if not (run and str(rec.get("app_run") or "") == run):' \
    'if not run:' \
    "an ask from an earlier launch of the app did not reach the person looking at this one."

mutant app-run-not-witnessed "C5c. " "$W" \
    'record["app_run"] = os.environ["CA_RUN"]' \
    'pass' \
    "a run the witness never writes down is a run no other conversation's lead can be credited with."

mutant run-start-asks-again "C5g. " "$S" \
    'if [ "${CA_VERDICT:-}" = "SATISFIED" ]; then\n    exit 0' \
    'if false; then\n    exit 0' \
    "a new conversation's lead told to open with the next question makes him answer one per conversation."

mutant blocked-counts-as-prepared "A5. " "$PY" \
    'prepared = [i for i in items if (i.get("state") or "") == ready_state]' \
    'prepared = list(items)' \
    "demanding that an UNPREPARED item be put to him is the failure the CEO-TODOs contract exists to stop, arriving from the other side."

# --- 2. NO BLOCK, AND THE ASK THAT IS NOT ONE ------------------------------
# The block, its escape hatch and the bare-marker rule went with the refusal
# (orchestration reliability R3, 2026-10-24). What is load-bearing now: nothing is
# refused, and a reply that asks his question is recorded as the ask.
mutant gate-refuses-again "C1. " "$G" \
    '# default (CLAUDE.md §125); a decision that really blocks something holds only that.\nexit 0' \
    '# default (CLAUDE.md §125); a decision that really blocks something holds only that.\nexit 2' \
    "an unrelated prepared decision would veto a repair or an independent dispatch again (R3; 2026-10-23, the review tools' repair held for a Windows question)."

mutant reply-ask-not-witnessed "D5. " "$N" \
    '        _CU_LINE="$(ca_match "$_CU_Q" "$_CU_ITEMS" 2>/dev/null || true)"' \
    '        _CU_LINE=""' \
    "a question put to him in the reply (§125) would not count, so he would be asked the same thing again."

mutant broken-fails-closed "C10. " "$G" \
    'UNGATED, and a declared-but-unreadable list is not an empty one."\n        exit 0 ;;' \
    'UNGATED, and a declared-but-unreadable list is not an empty one."\n        exit 2 ;;' \
    "a guard that wedges every dispatch over its own plumbing is a guard that gets switched off."

# --- 3. THE WITNESS --------------------------------------------------------
mutant no-ledger-write "B1. " "$W" \
    'with open(os.environ["CA_LEDGER"], "a+", encoding="utf-8") as fh:' \
    'if False:' \
    "the ledger IS the evidence; without it nothing can ever discharge and the gate becomes a wall."

# --- 4. THE NOTICES --------------------------------------------------------
mutant notice-counts-not-names "D1. " "$N" \
    '_CU_MSG="CEO TODO ${TOP_ID} has not been put to him this session: ${TOP_ASK}${MORE}' \
    '_CU_MSG="CEO TODO ${CA_UNASKED} items have not been put to him this session.${MORE}' \
    "a count is what got demoted on 2026-08-31; a specific question is what got answered."

mutant unasked-never-told "D1. " "$N" \
    'stop_notice_lead "unasked:$IDS" "$_CU_MSG"' \
    'true' \
    "a still-current question he was never asked would reach nobody: the lead asks him, so the lead must be told (R3/R6)."

mutant broken-list-is-quiet "D4. " "$N" \
    'are UNGATED and no prepared decision is being surfaced' \
    'are fine and no prepared decision is being surfaced' \
    "a declared-but-unreadable list must be loud on the one channel measured to reach the operator."

mutant session-start-silent "E2. " "$S" \
    '| head -1)"\nTOP_ID=' \
    '| head -0)"\nTOP_ID=' \
    "opening with a nameless announcement is opening with a count, which is the thing that failed."

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
if [ "$FAIL" -eq 0 ]; then
    echo "  $PASS/$PASS properties proven load-bearing"
    exit 0
fi
echo "  $PASS proven, $FAIL SURVIVED — a surviving mutant is a property nothing checks"
exit 1
