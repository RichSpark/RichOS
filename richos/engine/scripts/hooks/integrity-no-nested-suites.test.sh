#!/usr/bin/env bash
#
# integrity-no-nested-suites.test.sh — the integrity suite does not run other
# suites inside itself.
#
# The finding (hunt 2026-09-29 part 3, number 10): contract-integrity.test.sh ran
# nine complete suites as cases of its own (definition drift, workspace registry,
# workspace end-to-end, interactive prompt, idle land, claims, resume isolation,
# stated actions, scratch reaper). Each had been added when nothing else ran that
# suite. Now scripts/run-all-tests.sh and scripts/ci-units.sh discover every
# *.test.sh and run each as its own unit, so the nested run repeated work a unit
# already does, inside the suite that is the longest unit of a full pass. What
# the integrity suite can still say that the runner cannot is that the suite IS
# on the runner's list, and that is what remains (suite_discovered).
#
#   N1  none of the nine suites is executed from contract-integrity.test.sh
#   N2  each of the nine is still asserted present on the runner's list
#   N3  by execution: a scoped run of the scratch-reaper section passes its
#       discovery case (the section is cheap; it used to run a whole suite)
#
# Run directly: scripts/hooks/integrity-no-nested-suites.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="$SCRIPT_DIR/contract-integrity.test.sh"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s%s\n' "$1" "${2:+ ($2)}"; FAIL=$((FAIL + 1)); }

echo "=== the integrity suite runs no other suite inside itself ==="

NINE=(
    "scripts/hooks/guard-definition-drift.test.sh"
    "mega-lander/tests/workspaces.test.sh"
    "mega-lander/tests/workspaces-e2e.test.sh"
    "scripts/hooks/guard-interactive-prompt.test.sh"
    "scripts/hooks/guard-idle-land.test.sh"
    "scripts/hooks/guard-unresolved-claims.test.sh"
    "scripts/hooks/guard-resume-isolation.test.sh"
    "ass-kicker/tests/ass-kicker.test.sh"
    "scripts/scratch-reaper.test.sh"
)

for rel in "${NINE[@]}"; do
    base="$(basename "$rel")"
    # An EXECUTION names the suite in a quoted path and then redirects its output:
    #   set +e; "$SCRIPT_DIR/.../x.test.sh" >"$LOG" 2>&1; rc=$?; set -e
    if grep -E "\"\\\$SCRIPT_DIR/[^\"]*${base//./\\.}\"[[:space:]]*>" "$TARGET" >/dev/null; then
        bad "N1  $base is not executed from contract-integrity.test.sh" "an execution of it is still there"
    else
        ok "N1  $base is not executed from contract-integrity.test.sh"
    fi
    if grep -F "suite_discovered " "$TARGET" | grep -F "\"$rel\"" >/dev/null; then
        ok "N2  $base is still asserted to be on the runner's list"
    else
        bad "N2  $base is still asserted to be on the runner's list" "no suite_discovered case names $rel"
    fi
done

# N3 — by execution. A scoped run exits 3 when everything in scope is green (that
# suite's own deliberate design), so exit 3 plus the PASS line is the pass.
OUT="$(bash "$TARGET" --only SCR 2>&1)"
RC=$?
if [ "$RC" = 3 ] && printf '%s' "$OUT" | grep -q 'PASS  SCR1\.scratch-reaper-suite-is-discovered'; then
    ok "N3  the scoped SCR section passes its discovery case (exit 3, scoped green)"
else
    bad "N3  the scoped SCR section" "rc=$RC out=$(printf '%s' "$OUT" | grep -E 'SCR|FAIL' | head -4 | tr '\n' '|')"
fi

# N4/N5 — a SKIPPED mutation harness is not a PASS (hunt part 3 v3, finding
# 14). At the merge gate (RICHOS_MUTATION_PASSES=0) every harness exits 0 after
# printing NOT RUN; the suite used to read that exit alone and label eleven of
# them PASS "...-all-load-bearing".
if grep -E '^emit_case "[^"]*-all-load-bearing" 0 "\$rc"' "$TARGET" >/dev/null; then
    bad "N4  no mutation harness is judged by its exit code alone" \
        "$(grep -cE '^emit_case "[^"]*-all-load-bearing" 0 "\$rc"' "$TARGET") site(s) still are"
else
    ok "N4  no mutation harness is judged by its exit code alone"
fi
N5_FNS="$(python3 - "$TARGET" <<'PY'
import sys
t = open(sys.argv[1]).read()
out = []
for name in ("emit_mutation_case() {", "emit_case() {"):
    if name in t:
        i = t.index(name)
        out.append(t[i:t.index("\n}\n", i) + 3])
print("".join(out))
PY
)"
N5_LOG="$(mktemp -t n5-log.XXXXXX)"
RICHOS_MUTATION_PASSES=0 bash "$SCRIPT_DIR/idle-land.mutation.sh" >"$N5_LOG" 2>&1
N5_RC=$?
N5_OUT="$(bash -c "PASS=0; FAIL=0; NOT_RUN=0; FAIL_NAMES=(); NOT_RUN_NAMES=()
_t(){ echo 0; }; _trec(){ :; }
$N5_FNS
emit_mutation_case IL7.idle-land-mutations-all-load-bearing $N5_RC '$N5_LOG'" 2>&1)"
rm -f "$N5_LOG"
if printf '%s' "$N5_OUT" | grep -q 'NOT RUN  IL7' && ! printf '%s' "$N5_OUT" | grep -q 'PASS  IL7'; then
    ok "N5  a harness that did not run is reported NOT RUN, not PASS"
else
    bad "N5  a harness that did not run is reported NOT RUN, not PASS" "$(printf '%s' "$N5_OUT" | tr '\n' '|' | cut -c1-200)"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== integrity-no-nested-suites: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== integrity-no-nested-suites: all $PASS passed ==="
exit 0
