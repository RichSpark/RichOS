#!/usr/bin/env bash
#
# run-all-tests.sh — run EVERY test suite in this engine, and derive the count.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# "All 18 suites green" was the sentence quoted at every land, and 18 was the
# size of one glob — `scripts/hooks/*.test.sh`. There were 23 suites. The five
# outside that glob (scripts/demo.test.sh, scripts/locate-engine.test.sh,
# scripts/provision-claude-md.test.sh and the two under scripts/lib/) were not
# failing quietly; they were not being RUN.
#
# On 2026-08-29 two of those five were red on main, and had been for a day:
#
#   scripts/demo.test.sh          scripts/demo.sh — the buyer-facing 60-second
#                                 proof — exited 2 during setup, printing no
#                                 reason, on any machine that ran it.
#   scripts/locate-engine.test.sh case 6a, "install.sh mints the pointer",
#                                 FAILED.
#
# Both had the SAME cause: install.sh had started deriving its hook inventory
# from hooks/hooks.json, and two consumers did not provision it. Both were
# caught by a suite that already existed and already asserted the right thing.
# Nothing ran either suite, so the land reported 18/18 and was, on its own
# terms, telling the truth.
#
# This is the engine's own founding defect one level out. scripts/lib/
# registered-hooks.sh exists because a TYPED list of guards drifted twice in two
# days while the banner reported a full, reassuring fraction over it. A typed
# list of test suites is the same object. So this runner does not have one:
#
#   THE SUITE INVENTORY IS DISCOVERED FROM DISK, NEVER TYPED, NEVER GLOBBED
#   AT ONE DIRECTORY. A suite added anywhere under the engine is run by the
#   next invocation with no edit here, and cannot be silently excluded from
#   the fraction it is counted in.
#
# NO SILENT DEGRADATION: finding zero suites is a hard failure, not "0/0 green".
#
# ===========================================================================
# WHY THIS ONE STAYS IN CI, AND WHAT THAT COSTS — measured 2026-08-30
# ===========================================================================
# Every other honour-system check in this engine has now been moved to a
# PreToolUse chokepoint, because a check that runs when somebody remembers is a
# rule enforced by attention. The obvious next question is whether this runner
# should join them. THE ANSWER IS NO, AND IT IS ARITHMETIC RATHER THAN TASTE.
#
#   28 suites, one full sequential run, this machine:   439s  (7m19s)
#   the two that dominate it:
#     scripts/hooks/contract-integrity.test.sh          313s   (98 cases)
#     scripts/hooks/by-reference.test.sh                106s   (48 cases, each
#                                                              building TWO)
#   everything else, all 26 suites together:            ~40s
#
# RE-MEASURED 2026-09-04, and the arithmetic got worse while the ATTRIBUTION
# above turned out to be wrong. contract-integrity.test.sh is now 165 cases and
# 2978s -- 49.6 minutes on its own. The case count grew 1.7x; the wall clock
# grew 10x. The parenthetical that used to sit on this line said each case
# builds a whole sandbox repo, as though that were the cost. It is not:
#
#   sandbox construction, all 85 builds                  3.8s   (0.1%)
#   19 cases that are each a WHOLE OTHER SUITE          2348s   (70%)
#     of which 10 *.mutation.sh harnesses               2168s   (65%)
#   the probe itself, ~10s x ~80 invocations             ~800s   (~24%)
#
# A mutation harness runs a guard's entire behavioral suite once per mutant, so
# it costs N times that suite. WTI1 alone is 599s. That is not waste -- it is
# the work that makes a green tick load-bearing -- but it means the suite's cost
# tracks the number of MUTANTS, not the number of sandboxes.
# Full account: docs/measurements/integrity-suite-cost-2026-09-04/.
#
# A seven-minute pause before every `git commit` is not a guard, it is an
# outage. The engineer would remove the hook within the hour, and then NOTHING
# would run the suites — strictly worse than the honour system it replaced. A
# guard people disable protects nothing.
#
# THE 40-SECOND SUBSET IS ALSO REJECTED, and this is the less obvious half. It
# is affordable, and it is exactly the wrong 26: the two suites it would drop
# are the two that verify the guard REGISTRATION SURFACE — that the hooks are
# wired, once each, on the right event, present, executable and hash-matched.
# Those are the checks that answer "is the enforcement actually on?", which is
# the only question a commit-time gate has any business asking. A subset that
# skips them is the "18/18 suites" defect rebuilt on purpose: a fast, green,
# reassuring fraction over the set that does not include the thing most worth
# checking.
#
# SO THE COST OF LEAVING IT IN CI, STATED RATHER THAN GLOSSED: a red suite can
# reach `main` and sit there until CI runs. That risk is bounded by what the
# suites are FOR — they verify the engine's own machinery, which changes only
# when somebody is deliberately editing the engine, and that person is running
# the suite they are editing. It is NOT the risk that bit us on 2026-08-30:
# publication-completeness went red on a DOCS merge, by an author who had no
# reason to think any check applied to them. That is the class a chokepoint
# fixes, and it is now fixed — see guard-completeness-commits.sh.
#
# WHAT WOULD CHANGE THIS ANSWER, so it can be re-decided on evidence rather
# than re-argued: get contract-integrity.test.sh and by-reference.test.sh under
# ~10s combined, and the whole runner becomes chokepoint-affordable. Until then
# it stays in ci-verify.sh step 3, which is where the numbers say it belongs.
#
# This sentence used to end "(their cost is sandbox construction, which is
# cacheable)". Sandbox construction HAS now been cached -- built once, cloned
# per case -- and it bought 92 seconds of 3070, because it was 2.9% of the run
# rather than the whole of it. The distance between ~10s and 2978s is the
# mutation harnesses, and closing it means running independent mutants
# concurrently, not copying fewer files. Nobody should re-derive that from the
# suite's shape: it is measured, and the measurement is on disk.
#
# WHAT DID CHANGE, and it is the part a developer feels: the suite now takes
# --only <section>. A one-line change to one guard costs that guard's section --
# 21s to 181s, typically about 50s -- instead of 49.6 minutes. A scoped run
# exits 3, never 0, so nothing in this runner or in any gate can mistake one for
# a full pass. This runner invokes every suite with no arguments, so what it
# gets is still, always, the full pass.
#
# ===========================================================================
# THE LEAK CANARY, PER SUITE — added 2026-09-05
# ===========================================================================
# A suite is supposed to write only inside its own sandbox. Two findings on one
# day showed that "supposed to" was carrying the whole guarantee:
#
#   - escalations.test.sh wrote a fixture into whatever directory it was
#     started from. It was found as an untracked file in a working engineer's
#     worktree, reading exactly like a live escalation. He could not tell it
#     from a real one. The suite reported 59 passed, 0 failed on every leaking
#     run. The cause was one argument away from a fix already in the file.
#   - two mutation harnesses mutated the SHIPPED guards in place — including
#     guard-worktree-isolation.sh, the spawn gate — and restored them with
#     `trap ... EXIT`, which does not survive `kill -9`. Both are invoked by
#     contract-integrity.test.sh, which this runner runs.
#
# THE SECOND ONE IS WHY THIS LIVES HERE AND NOT ONLY IN EACH SUITE. If a
# tracked engine file changes DURING this run, the suites that ran before the
# change and the suites that ran after it tested DIFFERENT CODE, and the
# fraction printed at the bottom is a green tick over an inventory that never
# existed in one state. That is this engine's founding defect, one level out
# again — so it is a failure here, not a warning.
#
# IT IS TAKEN PER SUITE, NOT PER RUN, and that is the whole usability of it: a
# once-per-run check says "something in these 68 leaked" and leaves somebody a
# bisection; per-suite NAMES THE CULPRIT. The cost is two `git status` calls
# per suite, about 40 seconds across a 50-minute run.
#
# WHAT IT CANNOT SEE, so nobody reads more into a green run than is there:
# gitignored paths (deliberate — build output and state dirs churning under
# another agent must not turn this red), and anything outside the watched
# checkouts. Global state OUTSIDE every repository — the `richos-engine`
# pointer, user-scope settings — is a different problem with a different
# answer: scripts/lib/global-state-witness.sh, which watches named paths rather
# than trees. The two do not overlap and neither subsumes the other.
#
# ===========================================================================
# THE RECORD CANARY, PER SUITE — added 2026-09-11 (round 15)
# ===========================================================================
# The third problem, found three times in two days after the paragraph above
# was written: a suite writing into the OPERATOR'S RECORD under $HOME — the
# ownership ledger, the platform's fallback event log, the team directories.
# The last instance was the shipped SessionStart reaper, run by a green suite
# with a sandbox entity and an unredirected HOME, appending a `terminated`
# witness for a REAL agent whose shell was locked by a running pid. Each
# instance was fixed in its suite with a both-arms control; this runner now
# baselines those three paths before every suite and fails the suite that
# changes them, so the class is closed at the runner rather than re-found per
# suite. scripts/lib/record-canary.sh says exactly what it witnesses and the
# one thing it cannot (the platform's own per-turn `finished` rows).
#
# AMENDED 2026-09-27: each suite now runs with HOME in a throwaway home and
# every variable pointing into the operator's record removed, and the canary
# watches the record there, because a diff of the LIVE record charged every spawn and land
# elsewhere on the machine to whichever suite was running. The library's
# "AMENDED 2026-09-27" section says what that changes and what it cannot see.
#
# ITS FALSE-POSITIVE VECTOR, MEASURED RATHER THAN GUESSED. 2026-09-05, 49
# minutes of 10-second sampling with 68 agents live on the machine:
#
#   the shared main checkout   288 one-minute windows, 0 red
#                              252 seven-minute windows, 0 red
#   an engineer's OWN worktree  17% red at one minute, 50% at seven,
#   while he was editing it     because he was saving files in it
#
# So the noise is self-inflicted and self-explaining — the report names the
# file and the engineer recognizes his own save — and it is NOT the "another
# agent broke my run" flakiness a runner-level canary was feared for. Agents
# work in worktrees; only the lander writes to the shared checkout. It is also,
# in that case, TRUE: a suite that ran before the save and one that ran after
# it did test different code.
#
# WHAT THAT WINDOW DID NOT CONTAIN, said rather than glossed: a land. The last
# merge to the observed checkout was six minutes before sampling began, so the
# 0% figure is NOT evidence about a merge arriving mid-run. That property is
# proven mechanically instead, in leak-canary.test.sh case 5a, which lands a
# real merge commit under a live canary and requires silence.
#
# ===========================================================================
# PER-SUITE TIMING — added 2026-09-10
# ===========================================================================
# This runner printed PASS and FAIL and nothing else, so every wall-clock claim
# ever made about it came from OUTSIDE it: an agent watching a terminal, or a CI
# run log read days later. The cost of that is in the record rather than
# hypothetical — the "2977.9s" figure quoted into three plans was stale, the
# attribution above was wrong once already ("their cost is sandbox
# construction"), and two adversarial reviews on 2026-09-09 had to re-derive the
# distribution from transcripts because the runner would not say.
#
# So it says. Every suite line carries its duration, and every run ends with a
# slowest-N tail and the summed suite time. `--timing-tsv PATH` writes
# suite/ms/verdict/clock for anything that wants to diff two runs.
#
# THE NUMBER NAMES ITS OWN CLOCK. `date +%s%3N` is a GNU extension that BSD
# `date` answers with the literal string `17889976023N`, and the operator's
# /bin/bash is 3.2.57 with no `$EPOCHREALTIME`. scripts/lib/stopwatch.sh probes
# for a clock that returns thirteen digits and prints which one it got, because
# a duration table with no stated precision is how a one-second granularity gets
# quoted to three decimal places.
#
# Usage:
#   scripts/run-all-tests.sh            run everything, quiet on success
#   scripts/run-all-tests.sh --verbose  stream every suite's full output
#   scripts/run-all-tests.sh --list     print the discovered inventory, run none
#   scripts/run-all-tests.sh --timing-tsv PATH   also write a timing TSV
#
# Environment:
#   RICHOS_TIMING_TSV    same as --timing-tsv
#   RICHOS_TIMING_TAIL   how many rows in the slowest-N tail (default 10, 0 off)
#
# Exit codes:
#   0  every discovered suite passed and none wrote outside its sandbox
#   1  at least one suite failed, or leaked (each is named, with its output)
#   2  no suites discovered, or the engine root is unreadable — refusing to
#      report a green fraction over an inventory of nothing

set -uo pipefail
export PYTHONDONTWRITEBYTECODE=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ORIG_ARGS=("$@")

VERBOSE=0
LIST_ONLY=0
TIMING_TSV="${RICHOS_TIMING_TSV:-}"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --verbose|-v) VERBOSE=1 ;;
        --list)       LIST_ONLY=1 ;;
        --timing-tsv)
            shift
            [ "$#" -gt 0 ] || { echo "ERROR: run-all-tests.sh: --timing-tsv needs a path" >&2; exit 2; }
            TIMING_TSV="$1" ;;
        --timing-tsv=*) TIMING_TSV="${1#--timing-tsv=}" ;;
        *)
            echo "ERROR: run-all-tests.sh: unrecognized argument '$1'. Usage: run-all-tests.sh [--verbose] [--list] [--timing-tsv PATH]" >&2
            exit 2 ;;
    esac
    shift
done

[ -d "$ENGINE_ROOT" ] || { echo "ERROR: run-all-tests.sh: engine root unreadable: $ENGINE_ROOT" >&2; exit 2; }

# --- Discovery -------------------------------------------------------------
# Everything named *.test.sh, anywhere under the engine, sorted for a stable
# run order. LC_ALL=C so the order does not depend on the operator's locale —
# a suite list that reorders between machines makes two runs hard to diff.
SUITES=()
while IFS= read -r t; do
    [ -n "$t" ] || continue
    SUITES+=("$t")
done <<EOF
$(find "$ENGINE_ROOT" -type f -name '*.test.sh' 2>/dev/null | LC_ALL=C sort)
EOF

TOTAL="${#SUITES[@]}"
if [ "$TOTAL" -eq 0 ]; then
    echo "ERROR: run-all-tests.sh: found NO *.test.sh suites under $ENGINE_ROOT." >&2
    echo "       That is not a passing run with nothing to do — it means discovery is broken" >&2
    echo "       or this is not an engine checkout. Refusing to report a green fraction." >&2
    exit 2
fi

if [ "$LIST_ONLY" -eq 1 ]; then
    for t in "${SUITES[@]}"; do
        printf '%s\n' "${t#"$ENGINE_ROOT"/}"
    done
    printf '%s suite(s) discovered under %s\n' "$TOTAL" "$ENGINE_ROOT" >&2
    exit 0
fi

WORKER_TOOL="$ENGINE_ROOT/scripts/lib/worker_tokens.py"
PASS_TOOL="$ENGINE_ROOT/scripts/lib/engine_pass.py"
# The host shield (lib/host_shield.py, ceo-decisions §65): each suite's source is checked first,
# then it runs with stand-ins for this Mac's power, sleep, lock and display commands first on
# PATH, and a suite that called one fails with the stand-in's logged line. A fixture engine
# without it uses the one its caller exports (RICHOS_HOST_SHIELD_TOOL); with neither it REFUSES
# and starts no suite (`--list`, above, still lists), as run-tests.sh refuses
# (zach-opus-hostshim3, d645b9a77).
SHIELD="$ENGINE_ROOT/scripts/lib/host_shield.py"
[ -f "$SHIELD" ] || SHIELD="${RICHOS_HOST_SHIELD_TOOL:-}"
SHIELDED=()
if [ -n "$SHIELD" ] && [ -f "$SHIELD" ]; then
    SHIELDED=(python3 -S "$SHIELD" run --)
else
    echo "run-all-tests.sh: REFUSED: NO HOST SHIELD beside this copy and none exported (RICHOS_HOST_SHIELD_TOOL), so no suite can run behind the power, sleep, lock and display stand-ins (ceo-decisions §65). Nothing was run." >&2
    exit 2
fi
for tool in "$WORKER_TOOL" "$PASS_TOOL"; do
    [ -f "$tool" ] || { echo "ERROR: missing verification admission helper: $tool" >&2; exit 2; }
done
export RICHOS_VERIFICATION_CHECKOUT="${RICHOS_VERIFICATION_CHECKOUT:-$ENGINE_ROOT}"
python3 "$PASS_TOOL" needed "$TOTAL"; PASS_RC=$?
PASS_ARGS=()
case "$PASS_RC" in
    0) ;;
    10) if [ "${RICHOS_WORKER_SLOT_HELD:-}" = 1 ]; then
            echo "ERROR: large engine plan must acquire its slot before a worker; use proof-run's unit dispatch" >&2
            exit 75
        fi
        PASS_ARGS=(python3 "$PASS_TOOL" hold --count "$TOTAL" --label "run-all-tests.sh"
                   --checkout "$ENGINE_ROOT" --) ;;
    *) echo "ERROR: could not establish large-plan admission" >&2; exit 2 ;;
esac

C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'

printf '%s=== engine self-test: %s suite(s) discovered under %s ===%s\n' \
    "$C_BOLD" "$TOTAL" "$ENGINE_ROOT" "$C_RESET"

PASSED=0
FAILED_NAMES=()
LEAKED_NAMES=()
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/engine-tests.XXXXXX")"
trap 'rm -rf "$LOG_DIR"' EXIT

# --- the canary's roots ----------------------------------------------------
# A MISSING LIBRARY IS A REFUSAL, NOT A DEGRADATION. Sourcing a file that is
# not there under `set -u` without `-e` prints an error and carries on, and the
# run would then report a fraction with the canary silently absent — which is
# the shape of every defect this runner's header is about.
for lib in tree-witness leak-canary record-canary stopwatch; do
    if [ ! -f "$ENGINE_ROOT/scripts/lib/$lib.sh" ]; then
        echo "ERROR: run-all-tests.sh: scripts/lib/$lib.sh is missing." >&2
        echo "       The leak canary cannot run, and this runner will not report a green" >&2
        echo "       fraction with its own sandbox check absent. Reinstall the engine." >&2
        exit 2
    fi
done
# shellcheck source=lib/tree-witness.sh
. "$ENGINE_ROOT/scripts/lib/tree-witness.sh"
# shellcheck source=lib/leak-canary.sh
. "$ENGINE_ROOT/scripts/lib/leak-canary.sh"
# THE OPERATOR'S RECORD, PER SUITE (round 15, 2026-09-11). The leak canary
# watches checkouts and says so; three suites in two days wrote into $HOME
# under it — the last one a false `terminated` witness for a running agent,
# written by the shipped reaper from inside a green suite. This canary watches
# the ownership ledger, the fallback event log and the team directories, per
# suite, and a suite that changes them fails here whatever its own output said.
# Its paths are captured NOW, before any suite can move HOME. See the library.
# shellcheck source=lib/record-canary.sh
. "$ENGINE_ROOT/scripts/lib/record-canary.sh"
# shellcheck source=lib/stopwatch.sh
. "$ENGINE_ROOT/scripts/lib/stopwatch.sh"
sw_init
tw_pick_mtime "$LOG_DIR"
lc_add_root "$PWD"          # where this runner was started: where the 2026-09-05 fixture landed
lc_add_root "$ENGINE_ROOT"  # the engine's own checkout: where a mutated guard would show
CANARY_ROOTS_N="$(lc_count)"
if [ "$CANARY_ROOTS_N" -eq 0 ]; then
    # Not a warning. A canary watching nothing passes every run forever, which
    # is the shape this whole file exists to refuse.
    echo "ERROR: run-all-tests.sh: the leak canary resolved NO watched root." >&2
    echo "       It would report 'nothing leaked' on every run without looking at anything." >&2
    exit 2
fi
printf '  leak canary: watching %s root(s); witness is contents%s\n' \
    "$CANARY_ROOTS_N" \
    "$(tw_mtime_available && printf ' and a proven sub-second mtime' || printf ' ALONE (no sub-second mtime format proved itself here)')"
printf '  record canary: watching the record inside each suite'"'"'s own throwaway home (HOME moved there, every variable pointing into the operator'"'"'s record removed; all ledger rows, the fallback event log, the team directory entries); the operator'"'"'s own is %s\n' "$RC_LIVE_CFG"
printf '  timing: per suite, clock=python-monotonic; run clock=%s%s\n' \
    "$SW_METHOD" \
    "$([ -n "$TIMING_TSV" ] && printf ', TSV -> %s' "$TIMING_TSV")"

# The timing ledger. Parallel arrays rather than an associative array, because
# the operator's /bin/bash is 3.2.57 and has none — see stopwatch.sh's header.
TIMES_MS=()
TIMES_VERDICT=()
if [ -n "$TIMING_TSV" ]; then
    : >"$TIMING_TSV" || { echo "ERROR: run-all-tests.sh: cannot write TSV at $TIMING_TSV" >&2; exit 2; }
    printf 'suite\tms\tverdict\tclock\n' >>"$TIMING_TSV"
fi

RUN_T0="$(sw_now_ms)"
i=0
for t in "${SUITES[@]}"; do
    i=$((i + 1))
    REL="${t#"$ENGINE_ROOT"/}"
    LOG="$LOG_DIR/$i.log"
    printf '  [%2s/%2s] %-58s ' "$i" "$TOTAL" "$REL"
    # The baseline is re-taken before EVERY suite, so a leak is attributed to
    # the suite that made it and residue from an earlier one is not charged
    # twice. LC_HEALTHY is reset with it: a root that became unreadable is a
    # failure for this suite, never a quiet pass.
    CANARY_DIR="$LOG_DIR/canary.$i"
    LC_HEALTHY=1
    lc_baseline "$CANARY_DIR"
    CANARY_BASE_HEALTHY="$LC_HEALTHY"
    # A record of the suite's own (lib/record-canary.sh, "AMENDED 2026-09-27"):
    # the suite runs with HOME in a throwaway home, every variable pointing into
    # the operator's record removed, and the canary watches the record there, so activity
    # elsewhere on the machine is never charged to the suite, and a suite that
    # would have written the operator's record is still red.
    SUITE_HOME="$LOG_DIR/home.$i"
    if rc_sandbox "$SUITE_HOME"; then
        rc_baseline "$CANARY_DIR/record.txt"
        RECORD_BASE_HEALTHY="$RC_HEALTHY"
    else
        RECORD_BASE_HEALTHY=0
    fi
    # Each suite is self-contained and sandboxes its own state; none of them
    # takes arguments. Output is captured so a green run stays readable and a
    # red one can print EVERYTHING the failing suite said — a truncated failure
    # is a failure somebody has to reproduce by hand.
    # Execution includes owned supervision/cleanup but excludes admission and
    # the canary baseline/diff. Queueing must not inflate execution weights.
    TIMING="$LOG_DIR/$i.timing.json"
    # This serial wrapper imposes no suite execution deadline.
    export RICHOS_UNIT_DEADLINE_MARKER=""
    if [ "$RECORD_BASE_HEALTHY" -eq 1 ]; then
        ${PASS_ARGS[@]+"${PASS_ARGS[@]}"} python3 "$WORKER_TOOL" machine --timing "$TIMING" -- env "${RC_SANDBOX_ENV[@]}" ${SHIELDED[@]+"${SHIELDED[@]}"} bash "$t" >"$LOG" 2>&1
        RC=$?
    else
        printf "run-all-tests.sh: NOT RUN: private home unavailable\n" >"$LOG"
        RC=1
    fi
    SLOT_REFUSED=0
    [ "$RC" -ne 75 ] || [ -f "$TIMING" ] || SLOT_REFUSED=1
    read -r EXECUTION SUITE_MS QUEUED < <(python3 - "$TIMING" "$RC" <<'STATUS'
import json, math, sys
try:
    row = json.load(open(sys.argv[1]))
    elapsed, queue = row["execution_seconds"], row["admission_seconds"]
    assert all(type(v) in (int, float) and math.isfinite(v) and v >= 0 for v in (elapsed, queue))
    assert type(row["admitted"]) is bool and row["exit"] == int(sys.argv[2])
    outcome = row["outcome"]
    assert outcome in ("completed", "not-admitted", "infrastructure-error", "timed-out")
    assert row["admitted"] or outcome in ("not-admitted", "infrastructure-error")
    print(outcome, round(elapsed * 1000), round(queue, 1))
except (OSError, ValueError, KeyError, TypeError, AssertionError):
    print("infrastructure-error 0 unknown")
STATUS
)
    if [ "$SLOT_REFUSED" -eq 1 ]; then EXECUTION="not-admitted"; fi
    [ -n "$SUITE_MS" ] || SUITE_MS=0
    printf '(admission %ss) ' "${QUEUED:-unknown}"
    TIMES_MS+=("$SUITE_MS")
    ESCAPED="$(lc_escaped "$CANARY_DIR" "$LOG_DIR")"
    TOUCHED=""
    [ "$RECORD_BASE_HEALTHY" -eq 1 ] && TOUCHED="$(rc_escaped "$CANARY_DIR/record.txt")"
    rm -rf "$SUITE_HOME" 2>/dev/null || true
    if [ "$EXECUTION" != "completed" ]; then
        VERDICT="$(printf '%s' "${EXECUTION:-infrastructure-error}" | tr '[:lower:]' '[:upper:]')"
        printf '%s%s%s (rc=%s) %s\n' "$C_RED" "$VERDICT" "$C_RESET" "$RC" "$(sw_fmt "$SUITE_MS")"
        FAILED_NAMES+=("$REL ($VERDICT, rc=$RC)")
        TIMES_VERDICT+=("$VERDICT")
        sed 's/^/        /' "$LOG"
    elif [ "$RC" -ne 0 ]; then
        printf '%sFAIL%s (rc=%s) %s\n' "$C_RED" "$C_RESET" "$RC" "$(sw_fmt "$SUITE_MS")"
        FAILED_NAMES+=("$REL (rc=$RC)")
        TIMES_VERDICT+=("FAIL")
        sed 's/^/        /' "$LOG"
    elif [ "$CANARY_BASE_HEALTHY" -ne 1 ] || [ "$RECORD_BASE_HEALTHY" -ne 1 ]; then
        printf '%sFAIL%s (canary blind) %s\n' "$C_RED" "$C_RESET" "$(sw_fmt "$SUITE_MS")"
        LEAKED_NAMES+=("$REL — a canary could not witness one of its roots (leak: $CANARY_BASE_HEALTHY, record: $RECORD_BASE_HEALTHY), so it is NOT reporting a pass")
        TIMES_VERDICT+=("CANARY-BLIND")
    elif [ -n "$TOUCHED" ]; then
        printf '%sFAIL%s (touched the operator'"'"'s record: wrote the one in its own home) %s\n' "$C_RED" "$C_RESET" "$(sw_fmt "$SUITE_MS")"
        LEAKED_NAMES+=("$REL — RECORD-TOUCHED: wrote to its HOME/CLAUDE_CONFIG_DIR record (run by hand, that is $RC_LIVE_CFG)")
        TIMES_VERDICT+=("RECORD-TOUCHED")
        printf '        WROTE TO THE RECORD UNDER ITS OWN HOME — run by hand it would have written the operator'"'"'s, the class that wrote a false termination for a running agent on 2026-09-11:\n'
        printf '%s\n' "$TOUCHED" | sed 's/^/          /'
    elif [ -n "$ESCAPED" ]; then
        printf '%sFAIL%s (wrote outside its sandbox) %s\n' "$C_RED" "$C_RESET" "$(sw_fmt "$SUITE_MS")"
        LEAKED_NAMES+=("$REL")
        TIMES_VERDICT+=("LEAKED")
        printf '%s\n' "$ESCAPED" | while IFS="$(printf '\t')" read -r croot centry; do
            if lc_is_tracked_change "$centry"; then
                printf '        TRACKED FILE CHANGED DURING THE RUN — every suite after this one tested different code:\n'
            else
                printf '        WROTE OUTSIDE ITS SANDBOX — residue a stranger will have to explain:\n'
            fi
            printf '          %s  (under %s)\n' "$centry" "$croot"
        done
    else
        printf '%sPASS%s %s\n' "$C_GREEN" "$C_RESET" "$(sw_fmt "$SUITE_MS")"
        PASSED=$((PASSED + 1))
        TIMES_VERDICT+=("PASS")
        [ "$VERBOSE" -eq 1 ] && sed 's/^/        /' "$LOG"
    fi
    if [ -n "$TIMING_TSV" ]; then
        printf '%s\t%s\t%s\t%s\n' "$REL" "$SUITE_MS" "${TIMES_VERDICT[$((i - 1))]}" "python-monotonic" >>"$TIMING_TSV"
    fi
    if [ "$SLOT_REFUSED" -eq 1 ]; then
        printf '  Slot admission refused; %s remaining suite(s) were not admitted and remain unrun.\n' "$((TOTAL - i))"
        break
    fi
    # Do not reset the baseline and earn later passes on contaminated inputs.
    if [ "$CANARY_BASE_HEALTHY" -ne 1 ] || [ "$RECORD_BASE_HEALTHY" -ne 1 ] \
       || [ -n "$TOUCHED" ] || [ -n "$ESCAPED" ]; then
        if [ -n "${RICHOS_VERIFICATION_CONTAMINATION:-}" ]; then
            mkdir -p "$RICHOS_VERIFICATION_CONTAMINATION"
            python3 - "$RICHOS_VERIFICATION_CONTAMINATION" "$REL" <<'PY'
import json, os, sys, tempfile
fd, path = tempfile.mkstemp(prefix="suite-", suffix=".json", dir=sys.argv[1])
with os.fdopen(fd, "w") as out:
    json.dump({"unit": sys.argv[2], "reason": "contaminated or unwitnessed execution inputs"}, out)
PY
        fi
        printf '  QUARANTINED: %s remaining suite(s) were not run after contamination.\n' "$((TOTAL - i))"
        break
    fi
done
RUN_MS=$(( $(sw_now_ms) - RUN_T0 ))

# --- the slowest-N tail ----------------------------------------------------
# Printed on EVERY run, green or red, because the point of it is not
# diagnosing a failure — it is that nobody should ever again have to hand-time
# this runner to find out where its hour goes. Five wrong cost attributions in
# the record were all "which suites dominate", answered by guessing at the
# suite's shape instead of reading a number the run already knew.
echo ""
printf '%s--- timing: %s suite(s) in %s (clock=%s) ---%s\n' \
    "$C_BOLD" "$i" "$(sw_fmt "$RUN_MS")" "$SW_METHOD" "$C_RESET"
TAIL_N="${RICHOS_TIMING_TAIL:-10}"
case "$TAIL_N" in ''|*[!0-9]*) TAIL_N=10 ;; esac
if [ "$TAIL_N" -gt 0 ]; then
    SUM_MS=0
    TIMING_RAW="$LOG_DIR/timing.raw"
    : >"$TIMING_RAW"
    j=0
    while [ "$j" -lt "$i" ]; do
        SUM_MS=$(( SUM_MS + ${TIMES_MS[$j]} ))
        printf '%012d\t%s\t%s\n' "${TIMES_MS[$j]}" "${SUITES[$j]#"$ENGINE_ROOT"/}" "${TIMES_VERDICT[$j]}" >>"$TIMING_RAW"
        j=$(( j + 1 ))
    done
    printf '  slowest %s of %s (admitted execution; run total also includes queueing and canaries):\n' \
        "$TAIL_N" "$i"
    LC_ALL=C sort -rn "$TIMING_RAW" | head -n "$TAIL_N" | while IFS="$(printf '\t')" read -r ms rel verdict; do
        # Share of the summed SUITE time, not of the run: the two differ by the
        # canary, and a percentage that does not sum to 100 invites the reader
        # to think a suite is cheaper than it is.
        pct=0
        [ "$SUM_MS" -gt 0 ] && pct=$(( (10#$ms * 100) / SUM_MS ))
        printf '    %9s  %3s%%  %-58s %s\n' "$(sw_fmt "$((10#$ms))")" "$pct" "$rel" "$verdict"
    done
    printf '  summed execution time %s across %s suite(s); queueing and runner overhead %s\n' \
        "$(sw_fmt "$SUM_MS")" "$i" "$(sw_fmt "$(( RUN_MS - SUM_MS ))")"
fi

echo ""
if [ "${#FAILED_NAMES[@]}" -eq 0 ] && [ "${#LEAKED_NAMES[@]}" -eq 0 ]; then
    printf '%s✓ %s/%s engine test suites passed, and none wrote outside its sandbox or touched the operator'"'"'s record.%s\n' \
        "$C_GREEN" "$PASSED" "$TOTAL" "$C_RESET"
    exit 0
fi
printf '%s✗ %s/%s engine test suites passed.%s\n' "$C_RED" "$PASSED" "$TOTAL" "$C_RESET" >&2
if [ "${#FAILED_NAMES[@]}" -gt 0 ]; then
    printf '%s  %s FAILED:%s\n' "$C_RED" "${#FAILED_NAMES[@]}" "$C_RESET" >&2
    for n in "${FAILED_NAMES[@]}"; do
        printf '    - %s\n' "$n" >&2
    done
fi
if [ "${#LEAKED_NAMES[@]}" -gt 0 ]; then
    printf '%s  %s WROTE OUTSIDE ITS SANDBOX OR TOUCHED THE OPERATOR'"'"'S RECORD (green tests, but the run is not trustworthy):%s\n' \
        "$C_RED" "${#LEAKED_NAMES[@]}" "$C_RESET" >&2
    for n in "${LEAKED_NAMES[@]}"; do
        printf '    - %s\n' "$n" >&2
    done
    printf '    If one of these is YOUR OWN edit made while the run was in flight, it is still\n' >&2
    printf '    a true finding: the suites before it and the suites after it tested different code.\n' >&2
fi
exit 1
