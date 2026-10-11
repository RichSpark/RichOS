#!/usr/bin/env bash
#
# turn-manifest.mutation.sh — PROVES THE MANIFEST'S TESTS CAN FAIL.
#
# A suite of green ticks is evidence of nothing until somebody shows it turning
# red for the right reason. That is not a general worry here, it is this
# project's most-repeated defect, and the manifest is unusually exposed to it:
# every one of its properties is about NOT quietly rendering something
# plausible, and a test for "it did not quietly do the wrong thing" is exactly
# the kind that passes for free.
#
# The sharpest instance is the one this hook was written next to. The sibling
# Stop guard collected tool names SESSION-WIDE while believing it was scoped to
# the turn, and its suite stayed green for weeks — because no fixture carried a
# promptId at all, so the broken scope and the correct one produced identical
# output on every input it was ever given. Mutation 1 below is that exact bug,
# reintroduced on purpose, to show that this suite would have caught it.
#
# So: take the shipped hook, MUTATE ONE PROPERTY OUT OF IT AT A TIME, and
# assert that
#   1. turn-manifest.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement that matched nothing gives
#      a green run that looks like a green run, which is the same trap again).
#
# Every mutant is a throwaway copy. Nothing here touches the real tree.
#
# Run directly: scripts/hooks/turn-manifest.mutation.sh
# Exit 0 = every property is proven load-bearing; exit 1 = at least one could be
# removed without any test noticing.

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
SANDBOX="$(cd "$(mktemp -d -t turn-manifest-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

# --- the mutator -----------------------------------------------------------
# An exact string replacement, in python rather than sed, because these targets
# contain regex metacharacters and a sed that silently matched nothing is the
# failure mode this whole file exists to rule out. It exits non-zero when the
# target is absent, so a drifted source is a loud error and never a green run.
cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as fh:
    src = fh.read()
if old not in src:
    sys.stderr.write("MUTATION TARGET ABSENT — the source has drifted:\n  %s\n" % old)
    sys.exit(3)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(src.replace(old, new, 1))
PYEOF

# mutant <name> <expected-failing-case> <file> <old> <new> <why>
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
    local name="$1" want_case="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"

    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib"
    cp "$ENGINE_ROOT/scripts/hooks/turn-manifest.sh" \
       "$ENGINE_ROOT/scripts/hooks/turn-manifest.py" \
       "$ENGINE_ROOT/scripts/hooks/turn-manifest.test.sh" "$dir/scripts/hooks/"
    # stop-hook-notice.sh is copied DELIBERATELY, and its absence was a real
    # hole: turn-manifest.sh defines stub stop_notice_* functions when the
    # library is missing, so every mutant here used to run the DEGRADED notice
    # path rather than the shipped one. A harness that mutates a stub proves
    # something about the stub.
    cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh" "$dir/scripts/lib/"
    chmod +x "$dir/scripts/hooks/"*.sh

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # The run stops at the mutant's named FAIL line (scripts/lib/mutant-suite-run.sh,
    # hunt part 3 finding 9): in this suite a printed FAIL line always ends the run
    # red, and the stop matches the TRIMMED want as a fixed string, exactly what the
    # escaped witness test below matches, so it is never looser than that test.
    mutant_suite_run "$dir/out.txt" "$(printf '%s' "$want_case" | sed 's/[[:space:]]*$//')" bash "$dir/scripts/hooks/turn-manifest.test.sh"
    if [ "$MUTANT_STOPPED" = 1 ]; then
        printf '  PASS  %s — removing it turns %s red (stopped at that line)\n' "$name" "$want_case"
        return 0
    fi
    local rc=$MUTANT_RC

    if [ "$rc" -eq 0 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    # THE CASE MATCH IS A LITERAL WITH A BOUNDARY, NOT A BARE REGEX, and that
    # is a correction rather than a style. It used to be `grep -q "FAIL  $want_case"`,
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
    want_case_re="$(printf '%s' "$want_case" | sed 's/[[:space:]]*$//' | sed 's/[][\.*^$(){}?+|\/]/\\&/g')"
    # NO TRAILING BOUNDARY, and that is a correction to this very change made an
    # hour after it: the first version required the token to end at a
    # non-alphanumeric, which assumed every suite writes `FAIL  z1. some words`.
    # claim-roles' suite writes `FAIL  z1.landed-claim-about-a-dangling-commit`,
    # so the boundary demanded a space where a slug begins and turned ALL TWENTY
    # of its mutants into misfires -- twenty reds that looked exactly like
    # findings. The boundary was never what fixed anything: ESCAPING is. With
    # `.` taken literally, `z1.` cannot match `z1b.` and `C1.` cannot match
    # `C10.`, because the character after the prefix is a digit and not a dot.
    if ! grep -q "FAIL  ${want_case_re}" "$dir/out.txt"; then
        printf '  FAIL  %s — the suite went red, but NOT at %s (so the red is unrelated).\n' "$name" "$want_case"
        grep '  FAIL' "$dir/out.txt" | sed 's/^/          /'
        return 1
    fi
    printf '  PASS  %s — removing it turns %s red\n' "$name" "$want_case"
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

echo "=== turn-manifest: every property, proven load-bearing by removing it ==="

# ---------------------------------------------------------------------------
# 1. PER-TURN SCOPING — the sibling guard's real bug, rebuilt.
#    Opening the window at the first record makes every call in the session
#    "this turn's". The manifest does not degrade under this; it INVERTS,
#    printing the session's history under the heading "this turn".
# ---------------------------------------------------------------------------
mutant scoping-session-wide "c." scripts/hooks/turn-manifest.py \
    "    in_turn = False" \
    "    in_turn = True; offset = 0" \
    "A session-wide window reports every call the session ever made as this turn's."

# ---------------------------------------------------------------------------
# 2. THE TOOL'S OWN SENTENCE — the motivating failure itself.
#    Without this rule SendMessage renders as a byte count, and "Message queued
#    for delivery at its next tool round" never reaches the operator's eye —
#    which is the precise gap that let "I've told him" stand.
# ---------------------------------------------------------------------------
mutant no-tool-message "a." scripts/hooks/turn-manifest.py \
    '        if isinstance(obj, dict) and isinstance(obj.get("message"), str):' \
    '        if False and isinstance(obj, dict) and isinstance(obj.get("message"), str):' \
    "Without it, a queued SendMessage renders as a size and reads like success."

# ---------------------------------------------------------------------------
# 3. THE EXPLICIT EMPTY TURN.
#    The "dispatching it rather than queuing it" failure had ZERO tool calls
#    behind it. A manifest that renders nothing on an empty turn hides exactly
#    the turn it was built for.
# ---------------------------------------------------------------------------
mutant blank-on-empty-turn "b." scripts/hooks/turn-manifest.py \
    '    if total == 0:' \
    '    if total == 0 and False:' \
    "A blank render on a no-tool turn is indistinguishable from no manifest."

# ---------------------------------------------------------------------------
# 4. A GAP MUST NOT READ AS AN ABSENCE.
#    If an unreadable transcript falls through to the normal path it renders
#    "0 tool calls this turn" — asserting something false about the turn, which
#    is the defect this hook exists to remove, rebuilt inside it.
# ---------------------------------------------------------------------------
mutant gap-reads-as-empty "b2." scripts/hooks/turn-manifest.py \
    '        return [], {}, 0, "the transcript is not a readable file at %s" % path' \
    '        return [], {}, 0, None' \
    "An unreadable transcript would claim the turn ran nothing."

# ---------------------------------------------------------------------------
# 5. ANNOUNCED TRUNCATION.
#    A manifest that silently elides is the defect it exists to prevent,
#    rebuilt. Dropping the omission line leaves 25 tidy rows and no hint that
#    15 calls existed.
# ---------------------------------------------------------------------------
mutant silent-truncation "e2." scripts/hooks/turn-manifest.py \
    '    if dropped:' \
    '    if dropped and False:' \
    "Dropped rows would vanish with nothing said about them."

# ---------------------------------------------------------------------------
# 6. THE NEGATIVE CONTROL ITSELF.
#    If records_examined stops counting, the suite's proof that the manifest
#    read anything at all evaporates — and a manifest rendered from an empty
#    corpus still looks entirely convincing. This session found a scanner
#    reporting CLEAN over an empty corpus; this case is the guard against a
#    manifest joining it.
# ---------------------------------------------------------------------------
mutant records-uncounted "f." scripts/hooks/turn-manifest.py \
    '                examined += 1' \
    '                examined += 0' \
    "Without the counter, a manifest rendered from nothing passes its own tests."

# ---------------------------------------------------------------------------
# 7. IT MUST NEVER BLOCK.
#    A Stop hook that fails closed refuses to let the SESSION end and re-fires
#    to the block cap while the operator watches.
# ---------------------------------------------------------------------------
# THE NEEDLE THIS MUTANT SITS ON DRIFTED, AND THIS HARNESS NEVER RAN TO SAY SO.
# It used to edit `if [ "$RC" = "0" ] && [ -n "$OUT" ]; then ... exit 0`. That
# compound condition was split when the hook gained its stop-hook notice
# channel -- the `$RC = 0` test became the enclosing branch and the `-n "$OUT"`
# test the inner `if` -- so from that commit onward the mutation matched
# nothing. The harness reported `MUTATION TARGET ABSENT` rather than a false
# green, which is the one thing that went right here: it is the third clause of
# this file's own contract doing its job. But nothing ran it, so the refusal was
# printed to nobody and property `g.` -- the hook has no path that can block a
# turn -- was proven by NOTHING between that refactor and 2026-09-05.
# Repointed at the block as it now stands, and the mutation is the same one:
# turn the successful exit into a blocking one.
mutant can-block "g." scripts/hooks/turn-manifest.sh \
    '    if [ -n "$OUT" ]; then
        printf '"'"'%s\n'"'"' "$OUT"
    fi
    exit 0' \
    '    if [ -n "$OUT" ]; then
        printf '"'"'%s\n'"'"' "$OUT"
    fi
    exit 2' \
    "A manifest that can refuse a turn is a different, worse mechanism."

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
    printf '✓ %s/%s properties proven load-bearing.\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '✗ %s/%s proven, %s NOT load-bearing (removable with no test noticing).\n' \
    "$PASS" "$((PASS + FAIL))" "$FAIL" >&2
exit 1
