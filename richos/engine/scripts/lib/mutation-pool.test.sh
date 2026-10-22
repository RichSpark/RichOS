#!/usr/bin/env bash
#
# mutation-pool.test.sh — PROVE THE POOL BEFORE 526 MUTANTS RUN THROUGH IT.
#
# The pool's failure modes are all quiet. It does not crash; it reports a
# plausible tally in a plausible order and is wrong about which mutant said
# what, or about how many ran at all. Every property below is one whose
# violation produces a green-looking report:
#
#   P1   Order is SUBMISSION order, not completion order. Proven with workers
#        whose durations are deliberately reversed, so a pool that printed in
#        finishing order would print exactly backwards. Without this, two runs
#        of the same harness cannot be diffed.
#   P2   The tally comes from exit codes and matches the bodies.
#   P3   A worker that leaves NO exit code is a FAILURE, named. This is the
#        `kill -9` case, and the one that would otherwise shrink the
#        denominator and print green over a mutant that never ran.
#   P4   Concurrency actually happens — total work / wall clock > 1. A pool that
#        silently serialized would pass P1, P2 and P3 and buy nothing, which is
#        the whole point of the change.
#   P5   The degree is RESPECTED as an upper bound. Proven by having the workers
#        themselves record concurrent occupancy, so the assertion is on observed
#        overlap and not on the variable the pool set.
#   P6   A worker's pool DEPTH forces degree 1, the depth propagates into the
#        worker, and `RICHOS_MUTATION_INNER` — which eight harnesses export for
#        an unrelated reason — does NOT serialize. That last one (P6b) is a
#        regression case: keying the bound on that flag silently left 110 mutants
#        serial while claiming to have parallelized them.
#   P7   RICHOS_MUTANT_JOBS is honored, and junk in it is refused loudly rather
#        than silently becoming 0 (a degree of 0 would hang the throttle for
#        ever).
#   P8   Body stdout is reproduced VERBATIM, so contract-integrity.test.sh's
#        `grep -E '^  FAIL'` keeps matching byte for byte.
#   P9   A body that prints nothing and a zero-mutant pool are both handled
#        without a false tally.
#
# Everything runs in the pool's own temp dirs. Nothing is written to any tree.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '          %s\n' "$2"; FAIL=$((FAIL + 1)); }

# shellcheck source=stopwatch.sh
. "$SCRIPT_DIR/stopwatch.sh"
# shellcheck source=mutation-pool.sh
. "$SCRIPT_DIR/mutation-pool.sh"

SANDBOX="$(cd "$(mktemp -d -t mutation-pool-test.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"; mut_pool_cleanup' EXIT

# This suite measures the pool with sleeping fixture workers, not host capacity.
# An outer proof run may legitimately lend only one slot. Give this concurrency
# fixture its own known capacity; cross-run admission is tested separately.
export RICHOS_WORKER_TOKENS="$SANDBOX/fixture-workers"
export RICHOS_MACHINE_WORKERS="$RICHOS_WORKER_TOKENS"
export RICHOS_WORKER_TOKENS_TOOL="$SCRIPT_DIR/worker_tokens.py"
export RICHOS_WORKER_TOKENS_RESERVED=0
unset RICHOS_WORKER_SLOT_HELD RICHOS_WORKER_BORROW_LOCK
python3 "$RICHOS_WORKER_TOKENS_TOOL" init "$RICHOS_WORKER_TOKENS" 4 || exit 1

echo "=== mutation-pool: order, tally, bound, and the killed worker ==="

# ---------------------------------------------------------------------------
# P1 + P2 + P8: order is submission order even when completion order is inverted
# ---------------------------------------------------------------------------
# Worker `first` sleeps LONGEST and is submitted FIRST. Under any
# completion-ordered implementation the report comes out backwards, so this is a
# discriminating test rather than a decorative one.
body_slow()  { sleep 0.9; printf '  PASS  alpha — the slow one, submitted first\n'; return 0; }
body_mid()   { sleep 0.5; printf '  PASS  beta — the middle one\n'; return 0; }
body_fast()  { sleep 0.1; printf '  FAIL  gamma — the fast one, and deliberately red\n'; return 1; }

mut_pool_init 4
mut_pool_submit alpha body_slow
mut_pool_submit beta  body_mid
mut_pool_submit gamma body_fast
P1_OUT="$SANDBOX/p1.out"
mut_pool_drain >"$P1_OUT" 2>&1

P1_SEEN="$(grep -oE '(alpha|beta|gamma)' "$P1_OUT" | tr '\n' ' ' | sed 's/ *$//')"
if [ "$P1_SEEN" = "alpha beta gamma" ]; then
    ok "P1. output-is-submission-order-not-completion-order"
else
    bad "P1. output-is-submission-order-not-completion-order" \
        "expected 'alpha beta gamma', got '$P1_SEEN' (completion order would be 'gamma beta alpha')"
fi

if [ "$MUT_POOL_PASS" -eq 2 ] && [ "$MUT_POOL_FAIL" -eq 1 ] && [ "$MUT_POOL_N" -eq 3 ]; then
    ok "P2. tally-comes-from-exit-codes (2 pass / 1 fail / 3 total)"
else
    bad "P2. tally-comes-from-exit-codes" \
        "got pass=$MUT_POOL_PASS fail=$MUT_POOL_FAIL n=$MUT_POOL_N, wanted 2/1/3"
fi

# The exact line contract-integrity.test.sh greps for.
if grep -qE '^  FAIL  gamma — the fast one, and deliberately red$' "$P1_OUT"; then
    ok "P8. body-stdout-is-verbatim (grep -E '^  FAIL' still matches)"
else
    bad "P8. body-stdout-is-verbatim" "the body's own FAIL line was altered or lost:
$(sed 's/^/            /' "$P1_OUT")"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P3: a worker that leaves no exit code is a named FAILURE
# ---------------------------------------------------------------------------
# A REAL SIGKILL, delivered to the real worker process from outside it. The
# first version of this case had the body `kill -9 $$` itself, which in bash 3.2
# is the PID OF THIS TEST SCRIPT, not of the subshell — so it killed the suite
# (rc=137) after three cases. That is why the pool records `<seq>.pid` from the
# parent, and why the kill here comes from the test rather than from the body.
body_long() { sleep 30; printf '  PASS  should-never-be-reached\n'; return 0; }
body_ok()   { printf '  PASS  survivor\n'; return 0; }

mut_pool_init 2
mut_pool_submit doomed body_long
DOOMED_PID="$(cat "$MUT_POOL_DIR/0001.pid")"
# Wait for it to be genuinely running before killing it, so the case cannot pass
# by killing a process that had not started.
_p3_waited=0
while [ "$_p3_waited" -lt 50 ]; do
    kill -0 "$DOOMED_PID" 2>/dev/null && break
    sleep 0.05
    _p3_waited=$(( _p3_waited + 1 ))
done
kill -9 "$DOOMED_PID" 2>/dev/null
mut_pool_submit survivor body_ok
P3_OUT="$SANDBOX/p3.out"
mut_pool_drain >"$P3_OUT" 2>&1

if [ "$MUT_POOL_FAIL" -ge 1 ] && grep -q 'NO RESULT' "$P3_OUT" && grep -q 'doomed' "$P3_OUT"; then
    ok "P3. killed-worker-is-a-named-failure-not-an-absence"
else
    bad "P3. killed-worker-is-a-named-failure-not-an-absence" \
        "fail=$MUT_POOL_FAIL pass=$MUT_POOL_PASS; output:
$(sed 's/^/            /' "$P3_OUT")"
fi
if [ "$MUT_POOL_N" -eq 2 ]; then
    ok "P3b. the-denominator-still-counts-the-killed-worker"
else
    bad "P3b. the-denominator-still-counts-the-killed-worker" "MUT_POOL_N=$MUT_POOL_N, wanted 2"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P3c: A KILLED WORKER MUST FREE ITS SLOT. This case exists because the pool
# deadlocked, and a mutation of the pool is what found it rather than a review.
# The first throttle counted `*.done` markers and treated everything else as
# occupied; a SIGKILLed worker never writes a marker, so it held its slot for
# ever. At degree 1 the next submit blocked immediately and permanently.
#
# Run at DEGREE 1 deliberately: that is the configuration in which the defect is
# an instant hang rather than a slow leak, and degree 1 is a real configuration
# (RICHOS_MUTATION_INNER forces it).
#
# THE ASSERTION IS ON `_mut_pool_occupied` DIRECTLY, NOT ON A TIMED `submit`,
# and that is a correction to this very case. The first version called
# mut_pool_submit behind a 15s alarm that killed the suite. It did detect the
# hang — but by SIGTERMing the whole run, so the case could never print its own
# verdict and the mutation reported a MISFIRE at rc=143 rather than a red P3c. A
# case that can only fail by killing its suite is a case that reports nothing.
# Polling the predicate is bounded by construction and cannot hang.
# ---------------------------------------------------------------------------
mut_pool_init 1
mut_pool_submit doomed2 body_long
DOOMED2_PID="$(cat "$MUT_POOL_DIR/0001.pid")"
_p3c_waited=0
while [ "$_p3c_waited" -lt 50 ]; do
    kill -0 "$DOOMED2_PID" 2>/dev/null && break
    sleep 0.05
    _p3c_waited=$(( _p3c_waited + 1 ))
done
kill -9 "$DOOMED2_PID" 2>/dev/null

# Up to 5 seconds — 100x the 50ms reap time measured on this bash.
_p3c_i=0
while [ "$_p3c_i" -lt 100 ]; do
    [ "$(_mut_pool_occupied)" -eq 0 ] && break
    sleep 0.05
    _p3c_i=$(( _p3c_i + 1 ))
done
P3C_OCC="$(_mut_pool_occupied)"
if [ "$P3C_OCC" -eq 0 ]; then
    ok "P3c. a-killed-worker-frees-its-slot (occupancy fell to 0 in $(( _p3c_i * 50 ))ms at degree 1)"
    # And end to end: the next submit must actually return. GUARDED BY THE
    # ASSERTION ABOVE, and that guard is the point. Run unconditionally, this
    # submit blocks for ever whenever P3c has just failed — so the suite printed
    # its red verdict and then hung, and a hung suite delivers no verdict at all.
    # A test must not perform the operation it has just proven unsafe.
    mut_pool_submit survivor2 body_ok
    mut_pool_drain >"$SANDBOX/p3c.out" 2>&1
else
    bad "P3c. a-killed-worker-frees-its-slot" \
        "5s after SIGKILL the throttle still reports $P3C_OCC occupied slot(s) at degree 1 — the next submit would block for ever"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P4 + P5: concurrency happens, and the degree is an observed upper bound
# ---------------------------------------------------------------------------
# Each worker appends its own start/stop to a shared log through `>>`, which is
# atomic for short writes, and the peak overlap is computed afterwards. The
# assertion is therefore on what the machine DID, not on MUT_POOL_JOBS.
OCC="$SANDBOX/occupancy.log"
: >"$OCC"
body_occupy() { # <n>
    printf 'START\n' >>"$OCC"
    # First-wave rendezvous measures actual overlap without a machine-speed assertion.
    if [ "$1" -le 3 ]; then
        local tries=0
        while [ "$(grep -c START "$OCC")" -lt 3 ] && [ "$tries" -lt 200 ]; do
            sleep 0.05; tries=$((tries + 1))
        done
    fi
    sleep 0.6
    printf 'STOP\n' >>"$OCC"
    printf '  PASS  worker-%s\n' "$1"
    return 0
}

mut_pool_init 3
P4_T0="$(sw_now_ms)"
n=1
while [ "$n" -le 9 ]; do
    mut_pool_submit "worker-$n" body_occupy "$n"
    n=$(( n + 1 ))
done
mut_pool_drain >"$SANDBOX/p4.out" 2>&1
P4_WALL=$(( $(sw_now_ms) - P4_T0 ))

# Check the overlap itself. Wall time includes admission and supervision and is
# reported, but a busy host must not turn correct throttling into a false failure.
PEAK="$(awk '/START/{c++; if (c>m) m=c} /STOP/{c--} END{print m+0}' "$OCC")"
if [ "$PEAK" -ge 2 ]; then
    ok "P4. concurrency-actually-happens (peak=$PEAK, wall $(sw_fmt "$P4_WALL"))"
else
    bad "P4. concurrency-actually-happens" "the fixture workers never overlapped (peak=$PEAK)"
fi
if [ "$PEAK" -ge 1 ] && [ "$PEAK" -le 3 ]; then
    ok "P5. observed-peak-occupancy-respects-the-degree (peak=$PEAK, limit=3)"
else
    bad "P5. observed-peak-occupancy-respects-the-degree" "peak $PEAK exceeded the declared limit of 3"
fi
if [ "$MUT_POOL_PASS" -eq 9 ]; then
    ok "P5b. every-submitted-worker-ran (9/9)"
else
    bad "P5b. every-submitted-worker-ran" "pass=$MUT_POOL_PASS fail=$MUT_POOL_FAIL, wanted 9 pass"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P6: inside a pool worker, the degree is forced to 1
# ---------------------------------------------------------------------------
RICHOS_MUTANT_POOL_DEPTH=1
export RICHOS_MUTANT_POOL_DEPTH
mut_pool_init 8
if [ "$MUT_POOL_JOBS" -eq 1 ]; then
    ok "P6. pool-depth-forces-degree-1 (no JOBS-squared explosion)"
else
    bad "P6. pool-depth-forces-degree-1" \
        "asked for 8 at depth 1 and got $MUT_POOL_JOBS: nested pools would multiply"
fi
mut_pool_cleanup
unset RICHOS_MUTANT_POOL_DEPTH

# P6b: THE SERIALIZER MUST NOT KEY ON SOMEBODY ELSE'S FLAG.
#
# This case exists because the first version tested `RICHOS_MUTATION_INNER`,
# which is set — with `export`, at the top of the file — by EIGHT harnesses for
# their own unrelated recursion guard. Keying on it pinned those eight to degree
# 1 while every other harness went wide: 110 mutants, including unstarted-rows'
# 37, silently left serial by the very change that was supposed to speed them up.
# It was caught by an A/B run printing `1 at a time` for a harness asked for 8.
# The bound must be counted by the thing that nests.
RICHOS_MUTATION_INNER=1
export RICHOS_MUTATION_INNER
mut_pool_init 6
if [ "$MUT_POOL_JOBS" -eq 6 ]; then
    ok "P6b. RICHOS_MUTATION_INNER-alone-does-NOT-serialize"
else
    bad "P6b. RICHOS_MUTATION_INNER-alone-does-NOT-serialize" \
        "asked for 6 with only RICHOS_MUTATION_INNER set and got $MUT_POOL_JOBS — the 8 harnesses that export it would stay serial"
fi
mut_pool_cleanup
unset RICHOS_MUTATION_INNER

# P6c: the depth actually propagates INTO a worker, so a nested pool sees it.
# Without this, P6 is a test of a variable nobody sets.
body_depth() { printf '  PASS  depth=%s\n' "${RICHOS_MUTANT_POOL_DEPTH:-unset}"; return 0; }
mut_pool_init 2
mut_pool_submit depth body_depth
mut_pool_drain >"$SANDBOX/p6c.out" 2>&1
if grep -q 'depth=1' "$SANDBOX/p6c.out"; then
    ok "P6c. the-worker-carries-depth-1 (so a nested pool serializes)"
else
    bad "P6c. the-worker-carries-depth-1" \
        "worker saw: $(cat "$SANDBOX/p6c.out")"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P7: the environment override, and junk in it
# ---------------------------------------------------------------------------
RICHOS_MUTANT_JOBS=5
export RICHOS_MUTANT_JOBS
if [ "$(mut_pool_jobs_default)" = "5" ]; then
    ok "P7. RICHOS_MUTANT_JOBS-is-honored"
else
    bad "P7. RICHOS_MUTANT_JOBS-is-honored" "got $(mut_pool_jobs_default), wanted 5"
fi

P7_JUNK_OK=1
P7_WHY=""
for junk in 0 -3 abc "" "3x"; do
    RICHOS_MUTANT_JOBS="$junk"
    got="$(mut_pool_jobs_default 2>/dev/null)"
    # Whatever it decides, it must never be 0 or empty: the throttle's
    # `>= MUT_POOL_JOBS` comparison against 0 blocks the first submit for ever.
    case "$got" in
        ''|*[!0-9]*) P7_JUNK_OK=0; P7_WHY="$P7_WHY '$junk'->'$got' (not a number);" ;;
        *) [ "$got" -lt 1 ] && { P7_JUNK_OK=0; P7_WHY="$P7_WHY '$junk'->$got (would hang);"; } ;;
    esac
done
unset RICHOS_MUTANT_JOBS
if [ "$P7_JUNK_OK" -eq 1 ]; then
    ok "P7b. junk-degree-never-becomes-zero-or-empty"
else
    bad "P7b. junk-degree-never-becomes-zero-or-empty" "$P7_WHY"
fi

# ---------------------------------------------------------------------------
# P10: a note keeps its PLACE and is not counted as a mutant
# ---------------------------------------------------------------------------
# Deferred output moves anything printed between declarations. claim-roles prints
# a section heading between its two groups of mutants; as a plain `echo` it
# floated to the top and introduced every mutant instead of the eleven below it.
# Nothing about the verdicts changed, which is why it would have survived review.
body_p10() { printf '  PASS  %s\n' "$1"; return 0; }
mut_pool_init 4
mut_pool_submit one  body_p10 one
mut_pool_note "--- HEADING ---"
mut_pool_submit two  body_p10 two
mut_pool_drain >"$SANDBOX/p10.out" 2>&1
P10_SEEN="$(grep -oE 'one|--- HEADING ---|two' "$SANDBOX/p10.out" | tr '\n' '|')"
if [ "$P10_SEEN" = "one|--- HEADING ---|two|" ]; then
    ok "P10. a-note-keeps-its-place-between-mutants"
else
    bad "P10. a-note-keeps-its-place-between-mutants" "sequence was '$P10_SEEN'"
fi
if [ "$MUT_POOL_PASS" -eq 2 ] && [ "$MUT_POOL_FAIL" -eq 0 ] && [ "$MUT_POOL_SUBMITTED" -eq 2 ]; then
    ok "P10b. a-note-is-not-counted-as-a-mutant (2 submitted, 2 pass, 0 fail)"
else
    bad "P10b. a-note-is-not-counted-as-a-mutant" \
        "submitted=$MUT_POOL_SUBMITTED pass=$MUT_POOL_PASS fail=$MUT_POOL_FAIL, wanted 2/2/0"
fi
mut_pool_cleanup

# P10c: a pool of NOTHING BUT NOTES has submitted no mutants, and the zero-mutant
# refusal must still fire. Otherwise a harness whose mutants all failed to wire
# up could satisfy the refusal with a heading.
mut_pool_init 2
mut_pool_note "just a heading"
mut_pool_drain >/dev/null 2>&1
if [ "$MUT_POOL_SUBMITTED" -eq 0 ]; then
    ok "P10c. notes-alone-do-not-satisfy-the-zero-mutant-refusal"
else
    bad "P10c. notes-alone-do-not-satisfy-the-zero-mutant-refusal" \
        "MUT_POOL_SUBMITTED=$MUT_POOL_SUBMITTED after only a note"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P9: silent bodies and an empty pool
# ---------------------------------------------------------------------------
body_silent() { return 0; }
mut_pool_init 2
mut_pool_submit quiet body_silent
mut_pool_drain >"$SANDBOX/p9.out" 2>&1
if [ "$MUT_POOL_PASS" -eq 1 ] && [ "$MUT_POOL_FAIL" -eq 0 ]; then
    ok "P9. a-silent-body-still-tallies"
else
    bad "P9. a-silent-body-still-tallies" "pass=$MUT_POOL_PASS fail=$MUT_POOL_FAIL"
fi
mut_pool_cleanup

mut_pool_init 2
mut_pool_drain >"$SANDBOX/p9b.out" 2>&1
if [ "$MUT_POOL_N" -eq 0 ] && [ "$MUT_POOL_PASS" -eq 0 ] && [ "$MUT_POOL_FAIL" -eq 0 ]; then
    ok "P9b. an-empty-pool-drains-to-zero-not-to-a-false-green"
else
    bad "P9b. an-empty-pool-drains-to-zero-not-to-a-false-green" \
        "n=$MUT_POOL_N pass=$MUT_POOL_PASS fail=$MUT_POOL_FAIL"
fi
mut_pool_cleanup

# ---------------------------------------------------------------------------
# P11: RICHOS_MUTANT_ONLY runs the named mutants alone, says so, and a name that
# selects nothing leaves zero submissions for the zero-mutant refusal
# ---------------------------------------------------------------------------
body_p11() { printf '  PASS  %s\n' "$1"; return 0; }
export RICHOS_MUTANT_ONLY="two,four"
mut_pool_init 2 >"$SANDBOX/p11-init.out" 2>&1
for n in one two three four; do mut_pool_submit "$n" body_p11 "$n"; done
mut_pool_drain >"$SANDBOX/p11.out" 2>&1
P11_SEEN="$(grep -oE 'one|two|three|four' "$SANDBOX/p11.out" | tr '\n' ' ' | sed 's/ *$//')"
if [ "$P11_SEEN" = "two four" ] && [ "$MUT_POOL_SUBMITTED" -eq 2 ] && grep -q 'not a whole pass' "$SANDBOX/p11-init.out"; then
    ok "P11. RICHOS_MUTANT_ONLY-runs-only-the-named-mutants-and-announces-it"
else
    bad "P11. RICHOS_MUTANT_ONLY-runs-only-the-named-mutants-and-announces-it" \
        "ran '$P11_SEEN', submitted=$MUT_POOL_SUBMITTED, init said: $(cat "$SANDBOX/p11-init.out")"
fi
mut_pool_cleanup
export RICHOS_MUTANT_ONLY="no-such-mutant"
mut_pool_init 2 >/dev/null 2>&1
mut_pool_submit one body_p11 one
mut_pool_drain >/dev/null 2>&1
P11B_RC=0
( mut_pool_require_submissions "p11b" ) >/dev/null 2>&1 || P11B_RC=$?
if [ "$MUT_POOL_SUBMITTED" -eq 0 ] && [ "$P11B_RC" -eq 2 ]; then
    ok "P11b. a-name-that-selects-nothing-is-refused-not-green (rc=$P11B_RC)"
else
    bad "P11b. a-name-that-selects-nothing-is-refused-not-green" \
        "submitted=$MUT_POOL_SUBMITTED require rc=$P11B_RC"
fi
mut_pool_cleanup
unset RICHOS_MUTANT_ONLY

# ---------------------------------------------------------------------------
# P12: A FULL POOL WAITS FOR ITS WORKER, NOT FOR A CLOCK (2026-10-22). The throttle
# slept 0.05 s and counted again, starting a `sleep` twenty times a second while the
# pool was full. A `sleep` on PATH that logs its parent shows whether the throttle (this
# shell, $$) started any; the bodies use /bin/sleep, and the workers' own waits are
# started by the workers, so neither is counted. The next mutant still starts at once.
# ---------------------------------------------------------------------------
mkdir -p "$SANDBOX/p12-shim"
printf '#!/bin/sh\necho "$PPID" >> "%s/p12-sleeps.log"\nexec /bin/sleep "$@"\n' "$SANDBOX" > "$SANDBOX/p12-shim/sleep"
chmod +x "$SANDBOX/p12-shim/sleep"
body_p12() { /bin/sleep 0.6; printf '  PASS  %s\n' "$1"; return 0; }
P12_PATH="$PATH"
PATH="$SANDBOX/p12-shim:$PATH"
hash -r
mut_pool_init 1
P12_T0="$(sw_now_ms)"
mut_pool_submit p12a body_p12 p12a
mut_pool_submit p12b body_p12 p12b
mut_pool_submit p12c body_p12 p12c
mut_pool_drain >"$SANDBOX/p12.out" 2>&1
P12_MS=$(( $(sw_now_ms) - P12_T0 ))
PATH="$P12_PATH"
hash -r
P12_OWN="$(grep -cx "$$" "$SANDBOX/p12-sleeps.log" 2>/dev/null || true)"
if [ "${P12_OWN:-0}" -eq 0 ] && [ "$MUT_POOL_PASS" -eq 3 ]; then
    ok "P12. a-full-pool-waits-for-its-worker-not-a-clock (0 throttle sleeps, 3 mutants at degree 1 in ${P12_MS}ms)"
else
    bad "P12. a-full-pool-waits-for-its-worker-not-a-clock" \
        "throttle sleeps=${P12_OWN:-?} pass=$MUT_POOL_PASS wall=${P12_MS}ms"
fi
mut_pool_cleanup

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== mutation-pool: $FAIL failed, $PASS passed ==="
    exit 1
fi
echo "=== mutation-pool: all $PASS properties hold ==="
exit 0
