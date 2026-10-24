#!/usr/bin/env bash
#
# mutation-harness-guards.test.sh — THE TWO REFUSALS, AND A REAL kill -9.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# mutation-harness.sh gained two refusals on 2026-09-18 after one killed harness
# left 105.3 GB, and a refusal nobody has watched fire is a refusal nobody should
# trust. These are the cases that decide whether that night can happen again.
#
#   G1   THE DESTINATION IS INSIDE THE SOURCE -> refused. This is the exponential
#        case directly: mutant-2 copies the engine plus mutant-1, mutant-3 copies
#        the engine plus both, and 13.0 -> 26.2 -> 52.8 GB is what that looks
#        like.
#   G2   THE SOURCE IS ITSELF UNDER $TMPDIR -> refused. A harness mutates the
#        SHIPPED engine; a source that is already scratch means a mutant is
#        running mutants.
#   G3   THE SOURCE IS INSIDE THE ALLOCATOR ROOT -> refused, which holds even
#        when $TMPDIR has been reassigned (several suites here do that).
#   G4   CONTROL — a legitimate copy of a real engine still WORKS. Without this,
#        G1-G3 pass just as well against a function that refuses everything, and
#        every mutation harness in the engine would be silently dead.
#   G5   the refusal NAMES the paths and the 105 GB, so whoever hits it at 3 AM
#        does not have to go looking for why.
#   G6   A SCRATCH ROOT ALREADY OVER THE CEILING -> the next mutant refuses to
#        START. The ceiling is on the ROOT and not on one copy, which is the
#        opposite of the obvious design and the only version that would have
#        helped: no single copy that night was implausible (13, then 26, then 52
#        GB) and asking "is this copy too big" answered no, four times.
#   G7   CONTROL — under the ceiling it is silent.
#   G8   kill -9 MID-HARNESS. A real allocation, a real SIGKILL, and then the
#        real sweeper. The EXIT trap cannot run, so this is the exact condition
#        of 2026-09-17 — and the sandbox must still be reaped, by the sweeper,
#        with no session and no cooperation from the dead process. The sandbox
#        holds a fixture repository, as the engine's own suites' sandboxes do.
#   G9   CONTROL for G8 — while the owner is ALIVE the same sandbox is kept.
#        Without it G8 passes against a sweeper that deletes everything.
#   G10  CONTROL — an ended directory under the allocator root with NO ledger
#        row and a .git is kept: a name alone never makes a repository a fixture.
#   G11  CONTROL — an ended, ledger-recorded allocation holding a linked
#        checkout of a repository OUTSIDE it is kept.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$LIB_DIR/../.." && pwd)"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

OUTER_TMP="${TMPDIR:-/tmp}"
SANDBOX="$(cd "$(mktemp -d "$OUTER_TMP/mh-guards-test.XXXXXX")" && pwd -P)"
KILL_LIST=""
cleanup() {
    for p in $KILL_LIST; do
        kill -9 "$p" >/dev/null 2>&1 || true
        wait "$p" >/dev/null 2>&1 || true
    done
    rm -rf "$SANDBOX"
}
trap cleanup EXIT

echo "=== mutation-harness guard tests ==="

# The suite gets its own $TMPDIR, so every allocation and every sweep below
# happens in a throwaway tree and never in the operator's.
export TMPDIR="$SANDBOX/tmp"
export CLAUDE_CONFIG_DIR="$SANDBOX/claude"
mkdir -p "$TMPDIR" "$CLAUDE_CONFIG_DIR/state"
# AND ITS OWN MACHINE SETTINGS. G8's sweep is the real reaper with --apply; with the
# operator's settings file it swept the REAL build caches and bulk-drive temp roots.
mkdir -p "$SANDBOX/bulk"
printf 'RICHOS_BULK_ROOT=%s\n' "$SANDBOX/bulk" >"$SANDBOX/machine.env"
export RICHOS_ENV_FILE="$SANDBOX/machine.env"
for _k in RICHOS_MACHINE_SETTINGS_RESOLVED $(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import machine_settings; print(" ".join(machine_settings.KEYS))' "$LIB_DIR"); do
    unset "$_k"
done
unset _k

# shellcheck source=mutation-harness.sh
. "$ENGINE/scripts/lib/mutation-harness.sh"
# shellcheck source=scratch.sh
. "$ENGINE/scripts/lib/scratch.sh"

# ---------------------------------------------------------------------------
# G1 — the destination inside the source
# ---------------------------------------------------------------------------
SRC="$SANDBOX/engine-like"
mkdir -p "$SRC/scripts/hooks"
: >"$SRC/orchestration.config"
ERR="$SANDBOX/g1.err"
if mut_refuse_recursive_copy "$SRC/inner/dest" "$SRC" 2>"$ERR"; then
    bad "G1  a destination INSIDE the source was allowed — the exponential case"
else
    ok "G1  a destination inside the source is refused"
fi
if grep -q '105.3 GB' "$ERR" 2>/dev/null && grep -q "$SRC" "$ERR" 2>/dev/null; then
    ok "G5  the refusal names the paths and what it cost last time"
else
    bad "G5  the refusal does not say enough to act on at 3 AM"
    sed 's/^/        /' "$ERR" 2>/dev/null | head -6
fi

# ---------------------------------------------------------------------------
# G2 — the source is itself scratch
# ---------------------------------------------------------------------------
TMPSRC="$TMPDIR/a-sandbox-pretending-to-be-an-engine"
mkdir -p "$TMPSRC/scripts/hooks"
: >"$TMPSRC/orchestration.config"
if mut_refuse_recursive_copy "$SANDBOX/dest2" "$TMPSRC" 2>/dev/null; then
    bad "G2  a source under \$TMPDIR was allowed — a mutant running mutants"
else
    ok "G2  a source that is itself scratch is refused"
fi

# ---------------------------------------------------------------------------
# G3 — the source inside the allocator root, with $TMPDIR moved
# ---------------------------------------------------------------------------
# Several suites in this engine reassign $TMPDIR, so the root check has to hold
# independently of the $TMPDIR check above rather than being a duplicate of it.
ALLOC="$(scratch_new guardtest)"
mkdir -p "$ALLOC/scripts/hooks"
: >"$ALLOC/orchestration.config"
if mut_refuse_recursive_copy "$SANDBOX/dest3" "$ALLOC" 2>/dev/null; then
    bad "G3  a source inside the allocator root was allowed"
else
    ok "G3  a source inside the allocator root is refused"
fi

# ---------------------------------------------------------------------------
# G4 — THE CONTROL. A legitimate copy must still work.
# ---------------------------------------------------------------------------
# Without this, every case above passes against a function that refuses
# everything — which would leave every mutation harness in the engine dead while
# reporting PROVEN.
if mut_refuse_recursive_copy "$SANDBOX/dest-ok" "$SRC" 2>/dev/null; then
    ok "G4  CONTROL: a legitimate source/destination pair is ALLOWED"
else
    bad "G4  CONTROL FAILED: the guard refuses everything, so G1-G3 prove nothing"
fi

# And the real thing, end to end, against the real engine: the copy must still
# build a usable mutant.
if mutation_copy_engine "$SANDBOX/real-mutant" "$ENGINE" 2>/dev/null \
   && [ -f "$SANDBOX/real-mutant/orchestration.config" ] \
   && [ -d "$SANDBOX/real-mutant/scripts/hooks" ]; then
    ok "G4b CONTROL: mutation_copy_engine still builds a real mutant"
else
    bad "G4b mutation_copy_engine no longer works against the shipped engine"
fi

# ---------------------------------------------------------------------------
# G6 / G7 — the ceiling on the ROOT
# ---------------------------------------------------------------------------
# A 12 MB file against a 0 GB ceiling: `du -sk` reports whole kilobytes, so the
# ceiling is exercised without writing gigabytes to somebody's disk.
FILLER="$(scratch_new filler)"
dd if=/dev/zero of="$FILLER/blob" bs=1024 count=12288 >/dev/null 2>&1 || true
ERR6="$SANDBOX/g6.err"
if RICHOS_SCRATCH_ROOT_CEILING_GB=0 mut_refuse_oversize_root 2>"$ERR6"; then
    bad "G6  a scratch root over the ceiling did not refuse the next mutant"
else
    ok "G6  a scratch root over the declared ceiling refuses to START a mutant"
fi
if grep -q 'CEILING ON THE ROOT' "$ERR6" 2>/dev/null \
   && grep -q 'scratch-sweep.sh' "$ERR6" 2>/dev/null; then
    ok "G6b the refusal explains the root-not-copy reasoning AND how to recover"
else
    bad "G6b the ceiling refusal does not say how to get unstuck"
    sed 's/^/        /' "$ERR6" 2>/dev/null | head -8
fi
if RICHOS_SCRATCH_ROOT_CEILING_GB=500 mut_refuse_oversize_root 2>/dev/null; then
    ok "G7  CONTROL: comfortably under the ceiling it is silent"
else
    bad "G7  CONTROL FAILED: it refuses at any size, so G6 proves nothing"
fi
rm -rf "$FILLER"

# ---------------------------------------------------------------------------
# G8 / G9 — kill -9 MID-HARNESS, then the real sweeper
# ---------------------------------------------------------------------------
# THE EXACT CONDITION OF 2026-09-17: the process dies by SIGKILL, so its EXIT
# trap cannot run and cannot be made to run. Nothing about the cleanup may depend
# on the dead process having cooperated.
#
# The child allocates through scratch.sh and then sleeps. `kill -9` is a real
# SIGKILL to a real process, and the sweeper afterwards is the shipped one.
VICTIM_OUT="$SANDBOX/victim-path.txt"
# A REAL SEPARATE PROCESS — `bash -c`, not `( ... ) &`.
#
# The first version of this case used a subshell and G8 FAILED, correctly: `$$` in
# a bash subshell is the SCRIPT's pid, so the ledger row named this test suite,
# the suite was still alive, and the sweeper kept the sandbox exactly as it
# should. The test was wrong, not the sweeper. A separate `bash -c` child has its
# own `$$`, which is what a real harness invocation has, and killing it really
# does orphan the allocation.
VICTIM_SCRIPT="$SANDBOX/victim.sh"
cat >"$VICTIM_SCRIPT" <<VICTIM_EOF
#!/usr/bin/env bash
. "$ENGINE/scripts/lib/scratch.sh"
D="\$(scratch_new killed-mid-harness)"
# A fixture repository, as operator-fences-suite, quota-watch-test and
# resource-waits-test build inside their allocations.
mkdir -p "\$D/fixture-repo/.git/objects"
printf '%s\n' "\$D" >"$VICTIM_OUT"
# A trap that WOULD clean up, deliberately present, so this proves the SWEEPER
# and not the trap: SIGKILL does not run it. This is the whole point.
trap 'rm -rf "\$D"' EXIT
dd if=/dev/zero of="\$D/payload" bs=1024 count=2048 >/dev/null 2>&1 || true
sleep 300
VICTIM_EOF
chmod +x "$VICTIM_SCRIPT"
TMPDIR="$TMPDIR" CLAUDE_CONFIG_DIR="$CLAUDE_CONFIG_DIR" \
    /bin/bash "$VICTIM_SCRIPT" >/dev/null 2>&1 &
VICTIM=$!
KILL_LIST="$KILL_LIST $VICTIM"
# Wait for the allocation to be recorded rather than sleeping a guessed interval.
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
    [ -s "$VICTIM_OUT" ] && break
    sleep 0.4
done
VICTIM_DIR="$(cat "$VICTIM_OUT" 2>/dev/null || true)"

if [ -z "$VICTIM_DIR" ] || [ ! -d "$VICTIM_DIR" ]; then
    bad "G8  the child never allocated a sandbox — the case could not be set up"
    bad "G9  not reached"
else
    # G9 FIRST: while the owner is ALIVE the sweeper must keep it. This is the
    # control, and running it before the kill is what makes it a control rather
    # than an afterthought.
    CFG="$SANDBOX/sweep.config"
    cat >"$CFG" <<CFG
SCRATCH_REAPER_ENABLE="1"
SCRATCH_SESSION_PROCESS_NAMES="mhg${$}claude"
SCRATCH_CLAUDE_ROOTS="$SANDBOX/claude-roots"
SCRATCH_TMP_PATTERNS="nothing-matches-this"
SCRATCH_AGE_FLOOR_MINUTES="0"
SCRATCH_NIGHTLY_DIR="$SANDBOX/nightly"
SCRATCH_NIGHTLY_KEEP="3"
SCRATCH_NOTICE_BYTES="1"
SCRATCH_REAPER_HOURS="4"
SCRATCH_REAPER_MINUTE="40"
SCRATCH_ROOT_NAME="richos-scratch"
SCRATCH_DEFAULT_TTL_MINUTES="360"
SCRATCH_LEGACY_TMP_PATTERNS=""
SCRATCH_LEGACY_AGE_HOURS="0"
SCRATCH_LEGACY_GIT_IS_FIXTURE="1"
SCRATCH_FIXTURE_REPO_LABELS="killed-mid-harness"
SCRATCH_DOCKER_PRUNE="0"
SCRATCH_DOCKER_KEEP_STORAGE="20GB"
SCRATCH_DOCKER_UNTIL="720h"
CFG
    mkdir -p "$SANDBOX/claude-roots"
    sweep() {
        SCRATCH_REAPER_CONFIG="$CFG" \
        CLAUDE_CONFIG_DIR="$CLAUDE_CONFIG_DIR" \
        TMPDIR="$TMPDIR" \
        bash "$ENGINE/scripts/scratch-reaper.sh" --apply 2>&1
    }
    sweep >/dev/null 2>&1 || true
    if [ -d "$VICTIM_DIR" ]; then
        ok "G9  CONTROL: while its owner is ALIVE the sandbox is kept"
    else
        bad "G9  CONTROL FAILED: the sweeper deleted a LIVE owner's sandbox"
    fi

    # Now the kill. -9, so no trap, no handler, no cooperation.
    kill -9 "$VICTIM" >/dev/null 2>&1 || true
    wait "$VICTIM" 2>/dev/null || true
    # The child's own EXIT trap would have removed it had it been able to run.
    if [ ! -d "$VICTIM_DIR" ]; then
        bad "G8  the sandbox vanished at kill time, so this did not test the sweeper"
    else
        sweep >/dev/null 2>&1 || true
        if [ ! -d "$VICTIM_DIR" ]; then
            ok "G8  kill -9 mid-harness: the sweeper reaps the sandbox with no"
            ok "     trap, no session and no cooperation from the dead process"
        else
            bad "G8  a SIGKILLed harness's sandbox SURVIVED the sweep. This is"
            bad "     2026-09-17 exactly: the EXIT trap cannot run and nothing else did."
        fi
    fi

    # G10 / G11: ended owners (a pid that cannot be running), age floor 0, nothing open.
    ROOT_DIR="$(dirname "$VICTIM_DIR")"
    NOROW="$ROOT_DIR/99999990-noledger-aaaaaaaa"
    mkdir -p "$NOROW/repo/.git/objects"
    OUTSIDE_REPO="$SANDBOX/outside-repo"
    mkdir -p "$OUTSIDE_REPO/.git/worktrees/linked"
    LINKED="$ROOT_DIR/99999991-linked-bbbbbbbb"
    mkdir -p "$LINKED/checkout"
    printf 'gitdir: %s/.git/worktrees/linked\n' "$OUTSIDE_REPO" >"$LINKED/checkout/.git"
    printf '{"path":"%s","label":"linked","pid":99999991,"ppid":1,"session":"","cwd":"","created":"2026-01-01T00:00:00Z","ttl_minutes":1,"event":"new"}\n' \
        "$LINKED" >>"$(scratch_ledger)"
    sweep >/dev/null 2>&1 || true
    if [ -d "$NOROW/repo/.git" ]; then
        ok "G10 CONTROL: a .git under the allocator root with no ledger row is kept"
    else
        bad "G10 a repository with no ledger row was deleted on its NAME alone"
    fi
    if [ -f "$LINKED/checkout/.git" ]; then
        ok "G11 CONTROL: an ended allocation holding a checkout of an outside repository is kept"
    else
        bad "G11 a linked checkout of a repository outside the allocation was deleted"
    fi
fi

echo ""
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
