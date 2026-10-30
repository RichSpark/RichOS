#!/usr/bin/env bash
#
# scripts/lib/mutation-pool.sh — RUN INDEPENDENT MUTANTS AT THE SAME TIME,
# AND REPORT THEM IN THE ORDER THEY WERE DECLARED.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# A mutation harness runs a guard's entire behavioral suite once per mutant, so
# it costs N times that suite. Measured 2026-09-04: 19 such cases were 2347.7s,
# 70.0% of contract-integrity.test.sh, at TWELVE harnesses. There are now 40 of
# them declaring 526 mutants. The suite's cost tracks the number of MUTANTS.
#
# Every one of those mutants is independent by construction — it builds its own
# throwaway copy of the engine, edits one file in the copy, and runs a suite
# against the copy. Nothing is shared and nothing is ordered. They ran one at a
# time anyway, on a ten-core machine, because a `for` loop is what a harness was
# first written as.
#
# THE BINDING CONSTRAINT IS NOT TOTAL WALL CLOCK. IT IS THE PER-CHECK CEILING.
# An agent's Bash call is capped at 600 seconds. A real `--only IL,SA` call in
# the record hit that cap, and 239 polling calls hit it — the polling loops
# exist BECAUSE of it. So a section that cannot finish inside the cap does not
# merely take longer; it converts into a background run plus a poll loop plus a
# model turn per re-issue. Section sharding cannot split these: the six mutation
# sections are each a SINGLE case. Concurrency inside the harness is the only
# thing that lowers the largest indivisible unit.
#
# ===========================================================================
# WHAT MUST NOT CHANGE, AND IS THE REASON THIS IS A POOL AND NOT A `&`
# ===========================================================================
# 1. THE ORDER OF THE OUTPUT. A harness's report is read by humans and grepped
#    by contract-integrity.test.sh (`grep -E '^  FAIL'`). Completion order under
#    concurrency is a function of machine load, so a run whose lines arrive in
#    finishing order is a run that cannot be diffed against another run. Every
#    worker writes to its own file and the drain prints them in SUBMISSION
#    order, so the report is byte-identical to the serial one but for durations.
#
# 2. THE EXIT STATUS AND THE NAMED CASES. contract-integrity.test.sh reads only
#    the harness's exit code for its verdict (`emit_case "WTI1..." 0 "$rc"`) and
#    greps the log for diagnosis. The tally therefore comes from the workers'
#    exit codes, and a worker that left NO exit code counts as a FAILURE, never
#    as absent. A killed worker that silently vanished from the tally would be
#    this engine's founding defect rebuilt inside the fix: a green fraction over
#    an inventory that never ran.
#
# 3. NO NESTED EXPLOSION. Each mutant runs a whole suite; if that suite also
#    used a pool, the process count would be JOBS squared. Every worker carries
#    `RICHOS_MUTANT_POOL_DEPTH`, incremented by this file, and a pool that starts
#    at a non-zero depth runs at degree 1. The bound is JOBS, always.
#
#    IT IS NOT KEYED ON `RICHOS_MUTATION_INNER`, and that is a correction worth
#    the space: eight harnesses `export RICHOS_MUTATION_INNER=1` at the top of
#    the file for their own recursion guard, so keying on it pinned those eight
#    to degree 1 — 110 mutants — while the rest went wide. A nesting bound must
#    be counted by the thing that nests, not inferred from somebody else's flag.
#
# ===========================================================================
# WHY IT POLLS A MARKER FILE INSTEAD OF USING `wait -n`
# ===========================================================================
# `wait -n` — return when the NEXT job finishes — is exactly the primitive a
# bounded pool wants, and it does not exist here:
#
#   /bin/bash 3.2.57 on darwin 24.6.0:  `help wait` has no -n
#
# That is the operator's default shell and Apple will not ship newer (bash 4+
# is GPLv3). CI's `ubuntu-latest` has bash 5 and would be fine, so this is the
# usual trap: the primitive works everywhere the author tested and nowhere the
# operator runs.
#
# The rejected alternatives, said rather than implied:
#   - `kill -0 $pid` liveness polling. A finished-but-unreaped child answers
#     `kill -0` differently depending on when the shell got round to reaping it,
#     so the running count is racy in a way that shows up as an occasional
#     over-subscription rather than an error.
#   - Fixed batches of N with a `wait` between them. Simple and wrong for this
#     workload: a batch costs its SLOWEST member, and WTI's mutants are not
#     uniform, so nine cores idle while one 600-second mutant finishes.
#
# So each worker's LAST action is to create `<seq>.done`, after its output, its
# duration and its exit code are all on disk. A marker's existence therefore
# means "this slot is complete and safe to read", which is the drain's
# precondition.
#
# THE THROTTLE NEEDS ONE MORE FACT THAN THE MARKER, and finding out why cost a
# deadlock. Counting markers alone makes a SIGKILLed worker occupy its slot for
# ever, because a killed worker never writes one — see _mut_pool_occupied. The
# throttle therefore asks "no marker AND the process is still alive", which is
# the only formulation that is correct for both a worker that finishes and a
# worker that is killed.
#
# Usage, from a harness:
#
#   . "$ENGINE_ROOT/scripts/lib/mutation-pool.sh"
#   mut_pool_init                       # degree from RICHOS_MUTANT_JOBS or cores
#   mut_pool_submit "<label>" <fn> [args...]     # fn returns 0=PASS, non-0=FAIL
#   ...
#   mut_pool_drain                      # waits, prints in order, sets the tally
#   # -> MUT_POOL_PASS, MUT_POOL_FAIL, MUT_POOL_N, MUT_POOL_SLOWEST
#
# The submitted function must print its own report and must not depend on any
# variable it sets being visible afterwards: it runs in a subshell. Anything it
# needs to say comes back as stdout or as an exit code.

MUT_POOL_DIR=""
_MUT_WORKER_TOOL_DEFAULT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/worker_tokens.py"
MUT_POOL_N=0
# Slots include notes; MUT_POOL_SUBMITTED counts only real mutants, which is what
# the report line and the zero-mutant refusal must be about.
MUT_POOL_SUBMITTED=0
MUT_POOL_JOBS=1
MUT_POOL_PASS=0
MUT_POOL_FAIL=0
MUT_POOL_SLOWEST=""
MUT_POOL_TOTAL_MS=0
MUT_POOL_BUSY=0
MUT_POOL_WAKE=0      # 1 while fd 8 is this pool's wake-up FIFO (_mut_pool_wait_for_a_worker)

# mut_pool_jobs_default — the degree, derived rather than typed.
#
# MEASURED, not chosen by taste. docs/measurements/mutant-concurrency-2026-09-10/
# swept guard-model-ceiling.mutation.sh (17 mutants) on a 10-core machine:
#
#   jobs   1      2      4      6      8     10     12
#   wall  143s    72s    46s    39s    41s    36s    37s
#   sp    1.0x   1.9x   3.0x   3.7x   3.4x   3.9x   3.8x
#
# THE KNEE IS AT 4-6 AND EVERYTHING PAST 6 IS INSIDE THE NOISE — 8 measuring
# slower than 6, and 12 slower than 10, are variance rather than signal. A mutant
# is a whole shell suite: process spawning, git, python3, and a recursive copy of
# the mechanical layer. It never saturates a core, so the ceiling arrives well
# BELOW the core count and adding workers past it buys nothing.
#
# So: the core count, capped at 8. One step past the knee, so a harness whose
# mutants are not uniform in cost (guard-worktree-isolation's are not) still has
# somewhere to put a long one — and no further. The cap is what stops a 64-core
# CI runner starting 64 sandbox builds at once and going IO-bound, which is not
# hypothetical there: the sandbox is a recursive copy of the whole mechanical
# layer, and `clonefile` copy-on-write is NOT available under /tmp on the Linux
# runner (the run log says so in plain text), so on CI every sandbox is a full
# recursive copy rather than a CoW clone.
mut_pool_jobs_default() {
    local n=""
    if [ -n "${RICHOS_MUTANT_JOBS:-}" ]; then
        case "$RICHOS_MUTANT_JOBS" in
            ''|*[!0-9]*) n="" ;;
            *) [ "$RICHOS_MUTANT_JOBS" -ge 1 ] && n="$RICHOS_MUTANT_JOBS" ;;
        esac
        if [ -n "$n" ]; then printf '%s' "$n"; return 0; fi
        echo "WARNING: RICHOS_MUTANT_JOBS='$RICHOS_MUTANT_JOBS' is not a positive integer — deriving instead" >&2
    fi
    n="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
    case "$n" in ''|*[!0-9]*) n="" ;; esac
    [ -z "$n" ] && n="$(sysctl -n hw.ncpu 2>/dev/null || true)"
    case "$n" in ''|*[!0-9]*) n="" ;; esac
    [ -z "$n" ] && n=2
    [ "$n" -lt 1 ] && n=2
    [ "$n" -gt 8 ] && n=8
    printf '%s' "$n"
}

# mut_pool_init [<jobs>] — make the slot directory and fix the degree.
mut_pool_init() {
    MUT_POOL_DIR="$(cd "$(mktemp -d -t mutation-pool.XXXXXX)" && pwd -P)" || {
        echo "FATAL: mut_pool_init: could not create a pool directory" >&2; exit 2; }
    MUT_POOL_BORROW="${RICHOS_WORKER_SLOT_HELD:-0}"
    if [ -z "${RICHOS_WORKER_TOKENS:-}" ] && [ -f "$_MUT_WORKER_TOOL_DEFAULT" ]; then
        RICHOS_WORKER_TOKENS_TOOL="$_MUT_WORKER_TOOL_DEFAULT"
        RICHOS_WORKER_TOKENS="$(python3 "$RICHOS_WORKER_TOKENS_TOOL" directory)" || return 1
        RICHOS_MACHINE_WORKERS="$RICHOS_WORKER_TOKENS"
        RICHOS_WORKER_TOKENS_RESERVED=1
        export RICHOS_WORKER_TOKENS RICHOS_WORKER_TOKENS_TOOL RICHOS_MACHINE_WORKERS RICHOS_WORKER_TOKENS_RESERVED
        MUT_POOL_BORROW=0  # standalone pool: no caller holds a token to lend
    fi
    MUT_POOL_N=0
    MUT_POOL_SUBMITTED=0
    MUT_POOL_PASS=0
    MUT_POOL_FAIL=0
    MUT_POOL_SLOWEST=""
    MUT_POOL_TOTAL_MS=0
    MUT_POOL_WAKE=0
    if mkfifo "$MUT_POOL_DIR/wake" 2>/dev/null && exec 8<>"$MUT_POOL_DIR/wake"; then
        MUT_POOL_WAKE=1
    fi
    if [ -n "${1:-}" ]; then
        MUT_POOL_JOBS="$1"
    else
        MUT_POOL_JOBS="$(mut_pool_jobs_default)"
    fi
    # THE HARD SERIALIZER. Inside a pool worker, the degree is 1 — see property 3
    # in the header. Without this the process count is the product of the nesting
    # levels rather than the pool's bound.
    #
    # IT KEYS ON A VARIABLE THIS FILE OWNS, AND THAT IS A CORRECTION. The first
    # version tested `RICHOS_MUTATION_INNER`, which looked like exactly the right
    # signal and is not: EIGHT harnesses set `export RICHOS_MUTATION_INNER=1` at
    # the TOP OF THE FILE, for their own unrelated purpose of stopping the suite
    # they invoke from recursing. Keying on it silently pinned those eight to
    # degree 1 — 110 mutants, including unstarted-rows' 37 — while every other
    # harness went concurrent. The A/B run reported `1 at a time` for a harness
    # asked for 8, which is the ONLY reason it was noticed; the change would
    # otherwise have shipped as a speedup that skipped a fifth of the corpus.
    #
    # A nesting bound must be counted by the thing that does the nesting.
    case "${RICHOS_MUTANT_POOL_DEPTH:-0}" in
        ''|0) : ;;
        *) MUT_POOL_JOBS=1 ;;
    esac
    if [ -n "${RICHOS_MUTANT_ONLY:-}" ] && [ "${RICHOS_MUTANT_POOL_DEPTH:-0}" = 0 ]; then
        echo "  (RICHOS_MUTANT_ONLY=$RICHOS_MUTANT_ONLY: only the named mutant(s) run; this is not a whole pass)"
    fi
    # The clock. sw_init is idempotent, so sourcing order between this file and
    # stopwatch.sh does not matter, but the library must BE there: a pool that
    # silently reported every mutant as taking `?` would remove the only number
    # this whole change is judged on.
    if ! command -v sw_init >/dev/null 2>&1; then
        echo "FATAL: mut_pool_init: scripts/lib/stopwatch.sh must be sourced before mutation-pool.sh" >&2
        exit 2
    fi
    sw_init
    return 0
}

# _mut_pool_occupied — how many slots are STILL RUNNING.
#
# THE OBVIOUS VERSION OF THIS FUNCTION DEADLOCKS, and it was a mutation of this
# very file that proved it rather than a review. The first version counted
# `*.done` markers and called the rest occupied. A worker killed by SIGKILL
# never writes its marker, so it stayed "occupied" for ever: at degree 1 the
# next submit blocked immediately and permanently, and at degree 8 the pool lost
# one slot per killed worker and deadlocked on the eighth. The `silently-serial`
# mutant hung the suite instead of failing it — the shape of bug that, in a
# harness invoked from contract-integrity.test.sh, is a run that never returns
# and no output saying why.
#
# So a slot is occupied when it has NO completion marker AND its process is
# still alive. The reap behavior this leans on is measured, not assumed:
#
#   /bin/bash 3.2.57, a SIGKILLed background child, no blocking `wait`:
#   within 50ms `kill -0` answers GONE and `jobs -pr` is empty.
#
# A slot with no marker and no PID FILE YET counts as occupied: `.pid` is
# written by the parent immediately after `&`, so the window is microseconds,
# but treating it as free would over-subscribe the pool by one.
#
# The throttle asks _mut_pool_count, which leaves the count in MUT_POOL_BUSY and uses builtins
# only (printf -v, read): it is asked every time the pool wakes, and a `$(cat ...)` per slot
# started one process per slot per wake (2026-10-22, below). _mut_pool_occupied prints the
# same count, for a caller that wants it as text.
_mut_pool_occupied() {
    _mut_pool_count
    printf '%s' "$MUT_POOL_BUSY"
}

_mut_pool_count() {
    local i seq n=0 pid
    i=1
    while [ "$i" -le "$MUT_POOL_N" ]; do
        printf -v seq '%04d' "$i"
        if [ ! -e "$MUT_POOL_DIR/$seq.done" ]; then
            if [ ! -e "$MUT_POOL_DIR/$seq.pid" ]; then
                n=$(( n + 1 ))
            else
                pid=""
                read -r pid <"$MUT_POOL_DIR/$seq.pid" 2>/dev/null || :
                if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
                    n=$(( n + 1 ))
                fi
            fi
        fi
        i=$(( i + 1 ))
    done
    MUT_POOL_BUSY="$n"
}

# THE THROTTLE WAITS FOR A WORKER TO FINISH, NOT FOR A CLOCK (2026-10-22). It used to sleep
# 0.05 s and count again: a `sleep` and a process per slot twenty times a second for as long
# as the pool was full, which Hank's after-measurement caught 313 times in one once-a-second
# sample (richos-hq docs/research/2026-10-22-mac-system-cpu-after.md). Now every worker writes
# one byte to the pool's FIFO, on fd 8, right after its marker, and the throttle blocks reading
# it. A killed worker writes nothing, so the read gives up after one second and the count above,
# which is the only thing that decides, is asked again: the deadlock it was written against
# stays impossible. With no FIFO (mkfifo failed) it sleeps as before.
_mut_pool_wait_for_a_worker() {
    if [ "$MUT_POOL_WAKE" = 1 ]; then
        read -r -n 1 -t 1 -u 8 _mut_pool_token 2>/dev/null || :
    else
        sleep 0.05
    fi
}

# mut_pool_submit <label> <fn> [args...]
mut_pool_submit() {
    local label="$1"
    shift
    [ -n "$MUT_POOL_DIR" ] || { echo "FATAL: mut_pool_submit before mut_pool_init" >&2; exit 2; }
    # RICHOS_MUTANT_ONLY=<name>[,<name>...]: run only the named mutants of this harness, to
    # retry one unresolved mutant alone (docs/development/verification-retries.md) instead of
    # the whole pass. Only the outermost pool filters (a suite under test keeps its own pools),
    # and a name that selects nothing leaves zero submissions, which mut_pool_require_submissions
    # refuses. Announced in mut_pool_init: a filtered run is never a whole pass.
    if [ -n "${RICHOS_MUTANT_ONLY:-}" ] && [ "${RICHOS_MUTANT_POOL_DEPTH:-0}" = 0 ]; then
        case ",$RICHOS_MUTANT_ONLY," in
            *",$label,"*) : ;;
            *) return 0 ;;
        esac
    fi
    # Block until a slot frees: woken by the worker that finished (_mut_pool_wait_for_a_worker),
    # so a freed core is taken at once and a full pool costs nothing while it waits.
    _mut_pool_count
    while [ "$MUT_POOL_BUSY" -ge "$MUT_POOL_JOBS" ]; do
        _mut_pool_wait_for_a_worker
        _mut_pool_count
    done
    # A RUN-WIDE WORKER BUDGET, when the caller gives one (RICHOS_WORKER_TOKENS, from
    # proof-run.py; scripts/lib/worker_tokens.py): mutation-harness.sh `_mut_run` runs each
    # suite either on this pool's ONE free slot ($MUT_POOL_DIR/free.lock, the caller's own
    # token) or on a token of the budget. So eight workers inside one check count as eight
    # against the run, and a check can always progress one worker at a time.
    MUT_POOL_N=$(( MUT_POOL_N + 1 ))
    MUT_POOL_SUBMITTED=$(( MUT_POOL_SUBMITTED + 1 ))
    local seq
    seq="$(printf '%04d' "$MUT_POOL_N")"
    printf '%s' "$label" >"$MUT_POOL_DIR/$seq.label"
    (
        # The depth this pool's workers run at. Read by mut_pool_init in any
        # nested pool, so the process bound stays JOBS rather than JOBS squared.
        RICHOS_MUTANT_POOL_DEPTH=$(( ${RICHOS_MUTANT_POOL_DEPTH:-0} + 1 ))
        export RICHOS_MUTANT_POOL_DEPTH
        _lease=""
        if [ -n "${RICHOS_WORKER_TOKENS:-}" ]; then
            # The parent publishes this subshell's real PID; $$ is the wrong PID on bash 3.2.
            while [ ! -s "$MUT_POOL_DIR/$seq.pid" ]; do sleep 0.02; done
            _free="-"
            [ "$MUT_POOL_BORROW" = 0 ] || _free="$MUT_POOL_DIR/free.lock"
            python3 "${RICHOS_WORKER_TOKENS_TOOL:-$_MUT_WORKER_TOOL_DEFAULT}" lease "$RICHOS_WORKER_TOKENS" \
                "$(cat "$MUT_POOL_DIR/$seq.pid")" "$MUT_POOL_DIR/$seq.ready" "$MUT_POOL_DIR/$seq.release" "$_free" 8>&- &
            _lease=$!
            # 2026-10-28: the lease can wait minutes for a permit, and this looked every 0.02 s
            # (a `sleep` process 50 times a second). The first half second still looks that
            # often (a free permit is ready within it); after that it blocks in quiet_wait.py
            # until the ready file appears or the lease exits, at about 0% CPU.
            _ready_looks=0
            while [ ! -s "$MUT_POOL_DIR/$seq.ready" ]; do
                if ! kill -0 "$_lease" 2>/dev/null; then exit 125; fi
                if [ "$_ready_looks" -lt 25 ]; then
                    sleep 0.02
                    _ready_looks=$((_ready_looks + 1))
                else
                    python3 "$(dirname "${RICHOS_WORKER_TOKENS_TOOL:-$_MUT_WORKER_TOOL_DEFAULT}")/quiet_wait.py" \
                        --until-exists "$MUT_POOL_DIR/$seq.ready" --pid "$_lease" --seconds 60 8>&- 2>/dev/null || sleep 1
                fi
            done
            RICHOS_PROCESS_SCOPE="${RICHOS_PROCESS_SCOPE:+$RICHOS_PROCESS_SCOPE:}$(cat "$MUT_POOL_DIR/$seq.ready")"
            RICHOS_WORKER_SLOT_HELD=1
            RICHOS_WORKER_BORROW_LOCK="$(cat "$MUT_POOL_DIR/$seq.ready.borrow")"
            export RICHOS_PROCESS_SCOPE RICHOS_WORKER_SLOT_HELD RICHOS_WORKER_BORROW_LOCK
        fi
        _t0="$(sw_now_ms)"
        "$@" >"$MUT_POOL_DIR/$seq.out" 2>&1 8>&-
        _rc=$?
        if [ -n "$_lease" ]; then
            : > "$MUT_POOL_DIR/$seq.release"
            wait "$_lease" || _rc=125
        fi
        printf '%s' "$(( $(sw_now_ms) - _t0 ))" >"$MUT_POOL_DIR/$seq.ms"
        printf '%s' "$_rc" >"$MUT_POOL_DIR/$seq.rc"
        # LAST, and deliberately so: the marker means the three files above are
        # complete on disk. The throttle and the drain then share one fact.
        : >"$MUT_POOL_DIR/$seq.done"
        # Then wake the throttle. The FIFO is open read-write here, so this never blocks.
        [ "$MUT_POOL_WAKE" != 1 ] || printf 'x' >&8 2>/dev/null || :
    ) &
    # The worker's PID, recorded by the PARENT because a bash 3.2 subshell
    # cannot portably learn its own: `$$` inside `( )` is the SCRIPT's pid, not
    # the subshell's, and `$BASHPID` is bash 4+. That is not a footnote — the
    # first version of mutation-pool.test.sh had a worker `kill -9 $$` itself to
    # simulate a killed mutant, and it killed the TEST SUITE instead (rc=137).
    # Written for two consumers: a stuck run can be attributed to a process, and
    # the kill-proofing evidence needs a real signal delivered to a real worker.
    printf '%s' "$!" >"$MUT_POOL_DIR/$seq.pid"
    return 0
}

# mut_pool_note <text> — a line that keeps its PLACE in the report without being
# a mutant.
#
# Deferred output moves anything printed between declarations. claim-roles prints
#     === the state-claim arms, proven load-bearing by removing them ===
# between its two groups of mutants; with the results collected at the drain, a
# plain `echo` there floats to the top of the report and ends up introducing
# every mutant instead of the eleven it belongs to. Nothing about the verdicts
# changes, which is exactly why it would have survived review — a heading over
# the wrong group is wrong in a way no exit code can express.
#
# A note takes a slot so ordering is the same mechanism as everything else, and
# carries rc=note so the tally skips it. It is NOT counted as a mutant.
mut_pool_note() {
    [ -n "$MUT_POOL_DIR" ] || { echo "FATAL: mut_pool_note before mut_pool_init" >&2; exit 2; }
    MUT_POOL_N=$(( MUT_POOL_N + 1 ))
    local seq
    seq="$(printf '%04d' "$MUT_POOL_N")"
    printf '%s' "note" >"$MUT_POOL_DIR/$seq.label"
    printf '%s\n' "$1" >"$MUT_POOL_DIR/$seq.out"
    printf '%s' "note" >"$MUT_POOL_DIR/$seq.rc"
    : >"$MUT_POOL_DIR/$seq.pid"
    : >"$MUT_POOL_DIR/$seq.done"
    return 0
}

# mut_pool_drain — wait for every worker, print the slots in SUBMISSION order,
# and set the tally. Also sets MUT_POOL_SLOWEST to "<ms> <label>" of the worst
# slot, which is the number that decides whether a section fits under the tool
# ceiling.
mut_pool_drain() {
    [ -n "$MUT_POOL_DIR" ] || { echo "FATAL: mut_pool_drain before mut_pool_init" >&2; exit 2; }
    wait
    MUT_POOL_PASS=0
    MUT_POOL_FAIL=0
    MUT_POOL_TOTAL_MS=0
    local i seq rc ms label worst_ms=-1 worst_label=""
    i=1
    while [ "$i" -le "$MUT_POOL_N" ]; do
        seq="$(printf '%04d' "$i")"
        label="$(cat "$MUT_POOL_DIR/$seq.label" 2>/dev/null || printf 'slot-%s' "$seq")"
        if [ -f "$MUT_POOL_DIR/$seq.done" ] && [ -f "$MUT_POOL_DIR/$seq.rc" ]; then
            rc="$(cat "$MUT_POOL_DIR/$seq.rc" 2>/dev/null)"
            ms="$(cat "$MUT_POOL_DIR/$seq.ms" 2>/dev/null)"
        else
            # NO RESULT IS A FAILURE, NAMED. A worker killed by a signal, an OOM
            # or a full disk leaves no exit code; counting it as absent would
            # shrink the denominator and print a green tally over a mutant that
            # never ran.
            rc=""
            ms=""
        fi
        case "$ms" in ''|*[!0-9]*) ms="" ;; esac
        if [ -n "$ms" ]; then
            MUT_POOL_TOTAL_MS=$(( MUT_POOL_TOTAL_MS + ms ))
            if [ "$ms" -gt "$worst_ms" ]; then worst_ms="$ms"; worst_label="$label"; fi
        fi
        if [ -f "$MUT_POOL_DIR/$seq.out" ]; then
            # The body's own report, verbatim, so every existing grep over
            # `^  PASS` / `^  FAIL  <name>` keeps matching byte for byte.
            cat "$MUT_POOL_DIR/$seq.out"
        fi
        if [ "$rc" = "note" ]; then
            # A heading, not a mutant: printed in place, counted in neither column.
            :
        elif [ -z "$rc" ]; then
            printf '  FAIL  %s — NO RESULT: the worker left no exit code (killed, OOM, or out of disk).\n' "$label"
            printf '          This is counted as a failure. A mutant that did not run has proven nothing,\n'
            printf '          and a tally that omitted it would be green over an inventory that never ran.\n'
            MUT_POOL_FAIL=$(( MUT_POOL_FAIL + 1 ))
        elif [ "$rc" -eq 0 ]; then
            MUT_POOL_PASS=$(( MUT_POOL_PASS + 1 ))
        else
            MUT_POOL_FAIL=$(( MUT_POOL_FAIL + 1 ))
        fi
        i=$(( i + 1 ))
    done
    if [ "$worst_ms" -ge 0 ]; then
        MUT_POOL_SLOWEST="$worst_ms $worst_label"
    fi
    return 0
}

# mut_pool_report_line — one line naming the degree, the wall clock and the
# slowest mutant. PRINTED BY EVERY HARNESS, because the slowest single mutant is
# the number that decides whether the section fits under the 600s tool ceiling,
# and nobody should have to hand-time a harness to learn it again.
mut_pool_report_line() { # <wall-ms>
    local wall="${1:-}" slow_ms slow_label
    slow_ms="${MUT_POOL_SLOWEST%% *}"
    slow_label="${MUT_POOL_SLOWEST#* }"
    printf '  [%s mutant(s), %s at a time, wall %s' \
        "$MUT_POOL_SUBMITTED" "$MUT_POOL_JOBS" "$(sw_fmt "$wall")"
    if [ -n "$MUT_POOL_SLOWEST" ]; then
        printf ', slowest %s %s' "$(sw_fmt "$slow_ms")" "$slow_label"
    fi
    # Summed mutant time over wall clock is the speedup actually obtained, which
    # is not the degree: it is bounded by the slowest mutant and by whatever the
    # machine was already doing.
    if [ -n "$wall" ] && [ "$wall" -gt 0 ] && [ "$MUT_POOL_TOTAL_MS" -gt 0 ]; then
        printf ', %s.%sx serial' \
            "$(( MUT_POOL_TOTAL_MS / wall ))" "$(( (MUT_POOL_TOTAL_MS * 10 / wall) % 10 ))"
    fi
    printf ']\n'
}

mut_pool_cleanup() {
    if [ "$MUT_POOL_WAKE" = 1 ]; then
        exec 8>&-
        MUT_POOL_WAKE=0
    fi
    [ -n "$MUT_POOL_DIR" ] && rm -rf "$MUT_POOL_DIR"
    MUT_POOL_DIR=""
    return 0
}

# mut_pool_require_submissions <context> — a harness that DECLARED mutants and
# ran NONE must never exit 0.
#
# THIS EXISTS BECAUSE IT HAPPENED. interactive-prompt.mutation.sh passes each
# patch to `mutant` on STDIN via a heredoc. Converted naively, its thirteen
# mutants all failed to receive their patch, the pool drained zero submissions,
# and the harness printed
#
#     === interactive-prompt mutations: all 0 properties proven load-bearing ===
#
# and EXITED 0. Every layer above it agreed: `bash -n` passed, because a green
# run over nothing is syntactically perfect, and contract-integrity's IP7 case
# reads only the harness's exit code, so IP7 would have gone green over thirteen
# mutants that never ran. That is this engine's founding defect — a reassuring
# fraction over an inventory that was never there — reintroduced by the commit
# that was supposed to make the inventory run faster.
#
# The count is not compared against a declared total, because a harness that
# declares its own expected number can drift. Zero is the only number that is
# unambiguously wrong, and it is the one that actually occurred.
mut_pool_require_submissions() {
    local ctx="${1:-this harness}"
    if [ "$MUT_POOL_SUBMITTED" -eq 0 ]; then
        echo "ERROR: $ctx submitted NO mutants to the pool." >&2
        echo "       A harness that declares mutants and runs none must not report success:" >&2
        echo "       'all 0 properties proven load-bearing' is a green tick over an empty" >&2
        echo "       inventory, and every caller above reads only this exit code." >&2
        echo "       Look for a mutant loop that was not wired to mut_pool_submit." >&2
        exit 2
    fi
    return 0
}
