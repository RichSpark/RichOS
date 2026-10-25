#!/usr/bin/env bash
#
# mutation-focus-declared.test.sh — a mutant's suite run stops at the mutant's
# named FAIL line, in every harness that can honestly claim it.
#
# The finding (hunt 2026-09-29 part 3, number 9): a mutation harness proves a
# property is load-bearing by removing it and watching ONE named case go red, yet
# most harnesses ran the suite's WHOLE body for every mutant, so the dialect
# suite ran 22 times to be read at one line each. The shared harness already had
# the answer (`mutation_focus stop-at-want`, scripts/lib/stop-at-line.py); these
# did not use it. This suite proves:
#
#   S1  every harness on the shared loop that can claim it declares the focus
#   S2  every harness with its own mutant loop that can claim it runs its suite
#       through mutant_suite_run, and no longer runs the whole suite unfocused
#   H1  mutant_suite_run stops a suite at the named line, in seconds, and keeps
#       the output written up to it
#   H2  ... and when the named line never appears, the run is judged exactly as
#       it was before (the suite's own exit status, nothing stopped)
#   H3  ... and RICHOS_MUTATION_FOCUS=off turns the stop off
#   H4  --boundary never stops at a longer case (`D1` at `FAIL  D14`), as a
#       witness test ending in `\b` would not, and does stop at `FAIL  D1 `
#
# THE NINE THAT WERE LEFT OUT (finding 9, partial in the v2 re-check) are in
# CUSTOM now. They were held back on the claim that a substring stop would
# loosen their witness test; read, only one test carries a boundary
# (claim-capability-delivery's `\b`, which --boundary reproduces). ceo-asks,
# ceo-ruled, claim-roles, turn-manifest, unasked-deferral and waiver-repetition
# escape a TRIMMED want with no boundary, which a fixed-string stop on the
# trimmed want equals; idle-land and mechanical-findings grep the want
# unescaped, which a fixed-string stop is stricter than.
# NOT COVERED, STATED: guard-row-currency-commits, guard-worktree-isolation and
# guard-worktree-removal were not in the finding's inventory.
#
# Run directly: scripts/hooks/mutation-focus-declared.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s%s\n' "$1" "${2:+ ($2)}"; FAIL=$((FAIL + 1)); }

echo "=== mutation harnesses stop each mutant at its named line ==="

SHARED=(
    dispatch-pretooluse failure-type foreign-app-data guard-model-ceiling
    guard-sealed-worktree guard-unresolved-claims guard-vendoring-commits
    land-lease-commands left-off operator-leads premise-ask
    release-land-leases stale-staging unlanded-branches
)
for h in "${SHARED[@]}"; do
    f="$SCRIPT_DIR/$h.mutation.sh"
    if [ -f "$f" ] && grep -q '^mutation_focus stop-at-want$' "$f"; then
        ok "S1  $h.mutation.sh declares mutation_focus stop-at-want"
    else
        bad "S1  $h.mutation.sh declares mutation_focus stop-at-want" "not declared"
    fi
done

# One name is spelled in two halves: this suite is not about a network design,
# and a literal of it would read as one to the engine's phone-surface guard.
PHONE_SURFACE_HARNESS="home-net""work-phone"
CUSTOM=(
    dialect-guard guard-resume-isolation "$PHONE_SURFACE_HARNESS" host-display-power
    inflight-notify interactive-prompt public-record-repo reference-ledger
    ceo-asks ceo-ruled claim-capability-delivery claim-roles idle-land
    mechanical-findings turn-manifest unasked-deferral waiver-repetition
)
for h in "${CUSTOM[@]}"; do
    f="$SCRIPT_DIR/$h.mutation.sh"
    if [ -f "$f" ] && grep -q '^ *mutant_suite_run ' "$f" \
       && ! grep -qE '^ *(RICHOS_MUTATION_INNER=1 )?bash "\$dir/scripts/hooks/[a-z-]+\.test\.sh"' "$f" \
       && ! grep -q '^ *( cd "\$dir" && bash ' "$f"; then
        ok "S2  $h.mutation.sh runs each mutant through mutant_suite_run"
    else
        bad "S2  $h.mutation.sh runs each mutant through mutant_suite_run" "still runs the whole suite unfocused, or never calls the helper"
    fi
done

# --- the helper, by execution ------------------------------------------------
LIB="$ENGINE_ROOT/scripts/lib/mutant-suite-run.sh"
if [ ! -f "$LIB" ]; then
    bad "H1  scripts/lib/mutant-suite-run.sh exists" "missing"
    echo ""
    echo "=== mutation-focus-declared: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
# shellcheck source=../lib/mutant-suite-run.sh
. "$LIB"

SCRATCH="$(cd "$(mktemp -d -t mutfocus.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SCRATCH"' EXIT

# A fake suite: one case passes, the named one fails, then a slow tail that a
# stopped run must never reach. It exits 1, as a suite with a FAIL line does.
cat >"$SCRATCH/suite.sh" <<'SUITE'
#!/usr/bin/env bash
echo "  PASS  a1. first"
echo "  FAIL  b1. the named case"
sleep 20
echo "  FAIL  z9. the slow tail"
exit 1
SUITE
chmod +x "$SCRATCH/suite.sh"

T0=$SECONDS
mutant_suite_run "$SCRATCH/out1.txt" "b1." bash "$SCRATCH/suite.sh"
EL=$((SECONDS - T0))
if [ "$MUTANT_STOPPED" = 1 ] && [ "$EL" -lt 15 ] && grep -q 'FAIL  b1\.' "$SCRATCH/out1.txt" \
   && ! grep -q 'z9\.' "$SCRATCH/out1.txt"; then
    ok "H1  the run stops at the named line (${EL}s, tail never reached, output kept)"
else
    bad "H1  the run stops at the named line" "stopped=$MUTANT_STOPPED elapsed=${EL}s out=$(tr '\n' '|' <"$SCRATCH/out1.txt")"
fi

# H2 — a want that never appears: the suite runs to its end and its own exit
# status comes back, nothing stopped. (A short suite, so this costs no 20 s.)
cat >"$SCRATCH/short.sh" <<'SUITE'
#!/usr/bin/env bash
echo "  FAIL  other. something else"
exit 1
SUITE
mutant_suite_run "$SCRATCH/out2.txt" "b1." bash "$SCRATCH/short.sh"
if [ "$MUTANT_STOPPED" = 0 ] && [ "$MUTANT_RC" = 1 ]; then
    ok "H2  a want that never appears is judged as before (rc=1, nothing stopped)"
else
    bad "H2  unmatched want" "stopped=$MUTANT_STOPPED rc=$MUTANT_RC"
fi

RICHOS_MUTATION_FOCUS=off mutant_suite_run "$SCRATCH/out3.txt" "other." bash "$SCRATCH/short.sh"
if [ "$MUTANT_STOPPED" = 0 ] && [ "$MUTANT_RC" = 1 ] && grep -q 'FAIL  other\.' "$SCRATCH/out3.txt"; then
    ok "H3  RICHOS_MUTATION_FOCUS=off runs the whole suite and stops nothing"
else
    bad "H3  focus off" "stopped=$MUTANT_STOPPED rc=$MUTANT_RC"
fi

# H4 — a witness test that ends in `\b` (claim-capability-delivery's) must not be
# loosened by the stop: `D1` must not stop at `FAIL  D14`, and must stop at
# `FAIL  D1 ` once that line comes.
cat >"$SCRATCH/bounded.sh" <<'SUITE'
#!/usr/bin/env bash
echo "  FAIL  D14 a longer case"
echo "  FAIL  D1 the named case"
sleep 20
echo "  FAIL  z9. the slow tail"
exit 1
SUITE
T0=$SECONDS
mutant_suite_run --boundary "$SCRATCH/out4.txt" "D1" bash "$SCRATCH/bounded.sh"
EL=$((SECONDS - T0))
if [ "$MUTANT_STOPPED" = 1 ] && [ "$EL" -lt 15 ] && grep -q 'FAIL  D1 the named' "$SCRATCH/out4.txt" \
   && ! grep -q 'z9\.' "$SCRATCH/out4.txt"; then
    ok "H4a --boundary stops at FAIL  D1, after FAIL  D14 went by (${EL}s)"
else
    bad "H4a --boundary stops at the bounded case" "stopped=$MUTANT_STOPPED elapsed=${EL}s out=$(tr '\n' '|' <"$SCRATCH/out4.txt")"
fi
cat >"$SCRATCH/longer.sh" <<'SUITE'
#!/usr/bin/env bash
echo "  FAIL  D14 a longer case"
exit 1
SUITE
mutant_suite_run --boundary "$SCRATCH/out5.txt" "D1" bash "$SCRATCH/longer.sh"
if [ "$MUTANT_STOPPED" = 0 ] && [ "$MUTANT_RC" = 1 ]; then
    ok "H4b --boundary does not take FAIL  D14 for D1 (rc=1, nothing stopped)"
else
    bad "H4b --boundary must not stop at a longer case" "stopped=$MUTANT_STOPPED rc=$MUTANT_RC"
fi

# W1/W2 — THE WITNESS IS READ LITERALLY (hunt part 3 v3, finding 27). When the
# named line never appears the suite runs to its end and the harness reads the
# want from the output. Read as a regex, `W1.` matches `FAIL  W10.unrelated`:
# a different red was taken as the witness and the mutant called load-bearing.
W1_BAD=""
for f in "$SCRIPT_DIR"/*.mutation.sh "$ENGINE_ROOT"/ass-kicker/tests/*.mutation.sh; do
    [ -f "$f" ] || continue
    if grep -E '^[^#]*grep -q "FAIL  \$want"' "$f" >/dev/null; then
        W1_BAD="$W1_BAD $(basename "$f")"
    fi
done
if [ -z "$W1_BAD" ]; then
    ok "W1  no mutation harness reads an unescaped want as a regex"
else
    bad "W1  no mutation harness reads an unescaped want as a regex" "regex witness in:$W1_BAD"
fi

# W2 — by execution: the actual interactive-prompt mutant body, its suite run
# stubbed to fail at W10.unrelated (never at the wanted W1.), must not PASS.
mkdir -p "$SCRATCH/w2"
printf 'pass\n' >"$SCRATCH/w2/mutate.py"
W2_BODY="$(python3 - "$SCRIPT_DIR/interactive-prompt.mutation.sh" <<'PY'
import sys
t = open(sys.argv[1]).read()
print("_mutant_body() {" + t.split("_mutant_body() {", 1)[1].split("\n# mutant <name>", 1)[0])
PY
)"
W2_OUT="$(bash -c "SANDBOX='$SCRATCH/w2'; ENGINE_ROOT='$ENGINE_ROOT'
mutant_suite_run(){ printf '  FAIL  W10.unrelated\n' > \"\$1\"; MUTANT_STOPPED=0; MUTANT_RC=1; }
$W2_BODY
_mutant_body fixture W1. scripts/hooks/guard-interactive-prompt.sh fixture" </dev/null 2>&1)"
W2_RC=$?
if [ "$W2_RC" -ne 0 ] && ! printf '%s\n' "$W2_OUT" | grep -q '^ *PASS'; then
    ok "W2  a red at W10.unrelated is not taken as the witness for W1. (rc=$W2_RC)"
else
    bad "W2  a red at W10.unrelated is not taken as the witness for W1." "rc=$W2_RC out=$(printf '%s' "$W2_OUT" | tr '\n' '|' | cut -c1-200)"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== mutation-focus-declared: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== mutation-focus-declared: all $PASS passed ==="
exit 0
