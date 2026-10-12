#!/usr/bin/env bash
#
# run-all-tests.test.sh — THE RUNNER'S LEAK CANARY, PROVEN WHERE IT LIVES.
#
# scripts/lib/leak-canary.test.sh proves the DETECTOR. This proves the WIRING,
# and they are different claims: a correct detector that is never consulted, or
# consulted and then not allowed to change the verdict, is a green tick over
# the defect with extra steps. That distinction is not academic here — on
# 2026-09-02 five separate checks in this engine were found green over code
# that never ran.
#
# So every case below drives the REAL scripts/run-all-tests.sh against a
# throwaway engine containing throwaway suites, and reads its EXIT CODE and its
# OUTPUT — never the library's return value.
#
# BOTH DIRECTIONS, because a canary that reports everything is as useless as
# one that reports nothing: a suite that writes only in its own sandbox must
# leave the runner GREEN (case 1), and one that writes into the checkout must
# turn it RED and be NAMED (cases 2 and 3).
#
# NOT COVERED, by name: the cost of the canary on a real 68-suite run, and
# whether any SHIPPED suite trips it. That is a measurement over a run of about
# an hour, not a unit test, and it belongs in a measurement record rather than
# here.

set -uo pipefail
case "${1:-}" in
    "") ;;
    --record-only) echo "Focused record-canary cases only; other cases are not run." ;;
    *) echo "usage: run-all-tests.test.sh [--record-only]" >&2; exit 2 ;;
esac
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d "${TMPDIR:?}/run-all-tests-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '          %s\n' "$2"; FAIL=$((FAIL + 1)); }

echo "=== run-all-tests: the leak canary is wired, and it decides the verdict ==="

# The runner's RECORD canary (round 15) watches ${CLAUDE_CONFIG_DIR:-$HOME/.claude}.
# Every runner invocation below points it at a throwaway config directory, so
# this suite neither depends on the operator's real record nor touches it.
export CLAUDE_CONFIG_DIR="$SANDBOX/cfg"
export RICHOS_MACHINE_WORKERS="$SANDBOX/machine" RICHOS_ENGINE_PASS_DIR="$SANDBOX/slot"
# Deliberate leaks belong to the generated engine's domain. The surrounding
# shard still witnesses this entire suite and reports any actual outer escape.
export RICHOS_VERIFICATION_CONTAMINATION="$SANDBOX/expected-contamination"
unset RICHOS_WORKER_TOKENS RICHOS_WORKER_SLOT_HELD RICHOS_WORKER_BORROW_LOCK
mkdir -p "$CLAUDE_CONFIG_DIR/state"
# Every throwaway engine below has no host shield of its own, and the runner refuses to start a
# suite without one: each run gets this checkout's shield, so every fixture suite runs behind
# the stand-ins (7c proves the refusal with it withheld).
export RICHOS_HOST_SHIELD_TOOL="$ENGINE_ROOT/scripts/lib/host_shield.py"

NO_HOOKS="$SANDBOX/empty-hooks"; mkdir -p "$NO_HOOKS"
git_q() { git -C "$1" -c core.hooksPath="$NO_HOOKS" -c user.email=t@t -c user.name=t "${@:2}"; }

# build_engine <dir> — a throwaway engine checkout carrying the real runner and
# the real libraries, in a real git repository (the canary's primary branch is
# the git one, so a fixture that is not a repository would exercise the wrong
# half of it).
build_engine() {
    local d="$1"
    mkdir -p "$d/engine/scripts/lib" "$d/engine/scripts/hooks" "$d/engine/hooks"
    cp "$ENGINE_ROOT/scripts/run-all-tests.sh" "$d/engine/scripts/"
    # stopwatch.sh joined the runner's required libraries when per-suite timing
    # landed: the runner refuses to report a fraction with a missing library, so
    # a fixture that omits one gets rc=2 and every case below fails for that
    # reason instead of the one it is about. Nine cases went red exactly that
    # way. The list here must track the runner's own `for lib in ...` loop.
    cp "$ENGINE_ROOT/scripts/lib/tree-witness.sh" \
       "$ENGINE_ROOT/scripts/lib/leak-canary.sh" \
       "$ENGINE_ROOT/scripts/lib/record-canary.sh" \
       "$ENGINE_ROOT/scripts/lib/worker_tokens.py" \
       "$ENGINE_ROOT/scripts/lib/operator_fences.py" "$ENGINE_ROOT/scripts/lib/proc_tree.py" \
       "$ENGINE_ROOT/scripts/lib/engine_pass.py" "$ENGINE_ROOT/scripts/lib/machine_settings.py" \
       "$ENGINE_ROOT/scripts/lib/stopwatch.sh" "$d/engine/scripts/lib/"
    chmod +x "$d/engine/scripts/run-all-tests.sh"
    printf 'PROTECTED_PATHS="app"\n' > "$d/engine/orchestration.config"
    git init -q -b main "$d"
    git_q "$d" add -A
    git_q "$d" commit -q -m seed
}

# a suite that stays inside its own sandbox
clean_suite() {
    cat > "$1" <<'EOF'
#!/usr/bin/env bash
S="$(mktemp -d)"; trap 'rm -rf "$S"' EXIT
printf 'fixture\n' > "$S/fixture.txt"
echo "  PASS  wrote only inside its sandbox"
exit 0
EOF
    chmod +x "$1"
}

# ===========================================================================
# 1. GREEN: well-behaved suites leave the runner green, and it SAYS it checked.
# ===========================================================================
if [ "${1:-}" != --record-only ]; then
E1="$SANDBOX/e1"
build_engine "$E1"
clean_suite "$E1/engine/scripts/hooks/alpha.test.sh"
clean_suite "$E1/engine/scripts/hooks/beta.test.sh"
git_q "$E1" add -A; git_q "$E1" commit -q -m suites
OUT1="$( cd "$E1/engine" && bash scripts/run-all-tests.sh 2>&1 )"; RC1=$?
if [ "$RC1" -eq 0 ]; then
    ok "1a  GREEN: two suites that write only in their sandboxes leave the runner at rc 0 — the canary does not report activity"
else
    bad "1a  a clean run stays green" "rc=$RC1; the canary is firing on suites that leaked nothing:
$OUT1"
fi
case "$OUT1" in
    *"leak canary: watching"*) ok "1b  and it announces how many roots it is watching and what its witness is — a canary watching zero roots would pass forever in silence" ;;
    *)                         bad "1b  the canary announces itself" "no canary line in the output: $OUT1" ;;
esac
case "$OUT1" in
    *"none wrote outside its sandbox"*) ok "1c  and the green banner claims the sandbox check specifically, so a reader can tell this run from one where the canary was absent" ;;
    *)                                  bad "1c  the banner names the check" "got: $OUT1" ;;
esac

# ===========================================================================
# 2. RED on residue: a suite that writes an untracked file into the checkout.
#    This is the 2026-09-05 escalations defect.
# ===========================================================================
E2="$SANDBOX/e2"
build_engine "$E2"
clean_suite "$E2/engine/scripts/hooks/alpha.test.sh"
cat > "$E2/engine/scripts/hooks/leaky.test.sh" <<EOF
#!/usr/bin/env bash
printf 'id: looks-like-a-real-record\n' > "$E2/docs-escaped-record.md"
echo "  PASS  every assertion passed"
exit 0
EOF
chmod +x "$E2/engine/scripts/hooks/leaky.test.sh"
git_q "$E2" add -A; git_q "$E2" commit -q -m suites
OUT2="$( cd "$E2/engine" && bash scripts/run-all-tests.sh 2>&1 )"; RC2=$?
if [ "$RC2" -ne 0 ]; then
    ok "2a  RED: a suite whose own assertions all pass still fails the run because it wrote outside its sandbox"
else
    bad "2a  a leaking suite fails the run" "rc=0. The suite exited 0 and the canary did not change the verdict, so this is a green tick over the defect:
$OUT2"
fi
case "$OUT2" in
    *leaky.test.sh*) ok "2b  and the LEAKING SUITE is named — per-suite baselines mean nobody has to bisect 68 suites to find it" ;;
    *)               bad "2b  the leaking suite is named" "got: $OUT2" ;;
esac
case "$OUT2" in
    *docs-escaped-record.md*) ok "2c  and so is the file it left behind, so the reader can see at a glance it is residue rather than a real record" ;;
    *)                        bad "2c  the escaped file is named" "got: $OUT2" ;;
esac
case "$OUT2" in
    *"WROTE OUTSIDE ITS SANDBOX"*) ok "2d  and it is explained as residue, not as a test failure — the suite's assertions did pass" ;;
    *)                             bad "2d  residue is explained as residue" "got: $OUT2" ;;
esac
# The other suite must NOT be blamed. Per-suite baselines exist for this.
# THE ASSERTION IS ON alpha's OWN LINE, not on the whole output. A `case`
# against the whole string matched "alpha.test.sh ... PASS ... leaky ... FAIL"
# and reported the runner as broken when the runner was right — this case
# failed for a reason unrelated to what it tests, which proves nothing about
# anything. Its first version is why the line is isolated here.
ALPHA_LINE="$(printf '%s\n' "$OUT2" | sed $'s/\033\\[[0-9;]*m//g' | grep 'alpha\.test\.sh')"
case "$ALPHA_LINE" in
    *PASS*) ok "2e  and the innocent suite beside it is NOT blamed — the baseline is re-taken per suite, so residue is charged once, to its author" ;;
    "")     bad "2e  the innocent suite is not blamed" "alpha.test.sh has no line in the output at all, so it was not run" ;;
    *)      bad "2e  the innocent suite is not blamed" "alpha's own line is not a PASS, so the attribution is not per-suite: $ALPHA_LINE" ;;
esac

# ===========================================================================
# 3. RED on a TRACKED change: the mutation-harness defect. Different finding,
#    different explanation — this one means the run tested two different trees.
# ===========================================================================
E3="$SANDBOX/e3"
build_engine "$E3"
printf 'THE_SHIPPED_GUARD=1\n' > "$E3/engine/scripts/hooks/pretend-guard.sh"
clean_suite "$E3/engine/scripts/hooks/alpha.test.sh"
cat > "$E3/engine/scripts/hooks/mutating.test.sh" <<EOF
#!/usr/bin/env bash
# the old harness shape: mutate the shipped file and (here) never restore it
printf 'THE_SHIPPED_GUARD=0\n' > "$E3/engine/scripts/hooks/pretend-guard.sh"
echo "  PASS  every assertion passed"
exit 0
EOF
chmod +x "$E3/engine/scripts/hooks/mutating.test.sh"
git_q "$E3" add -A; git_q "$E3" commit -q -m suites
OUT3="$( cd "$E3/engine" && bash scripts/run-all-tests.sh 2>&1 )"; RC3=$?
if [ "$RC3" -ne 0 ]; then
    ok "3a  RED: a suite that modifies a TRACKED engine file mid-run fails it, however green its own assertions were"
else
    bad "3a  a tracked mid-run change fails the run" "rc=0:
$OUT3"
fi
case "$OUT3" in
    *"TRACKED FILE CHANGED DURING THE RUN"*)
        ok "3b  and it is explained as 'every suite after this one tested different code' — not as residue, which is a different and lesser problem" ;;
    *)  bad "3b  a tracked change gets its own explanation" "got: $OUT3" ;;
esac
case "$OUT3" in
    *pretend-guard.sh*) ok "3c  and the modified file is named" ;;
    *)                  bad "3c  the modified file is named" "got: $OUT3" ;;
esac

# ===========================================================================
# 4. A MISSING LIBRARY IS A REFUSAL, NOT A QUIET DEGRADATION. Under `set -u`
#    without `-e`, sourcing a file that is not there prints and carries on —
#    so without an explicit check the runner would report a fraction with its
#    own sandbox check silently absent.
# ===========================================================================
E4="$SANDBOX/e4"
build_engine "$E4"
clean_suite "$E4/engine/scripts/hooks/alpha.test.sh"
rm -f "$E4/engine/scripts/lib/leak-canary.sh"
OUT4="$( cd "$E4/engine" && bash scripts/run-all-tests.sh 2>&1 )"; RC4=$?
if [ "$RC4" -eq 2 ]; then
    ok "4a  the runner REFUSES (rc 2) when the canary's library is missing, rather than reporting a green fraction without it"
else
    bad "4a  a missing canary library is refused" "rc=$RC4, so a partial install would report a reassuring fraction with no sandbox check running at all:
$OUT4"
fi
case "$OUT4" in
    *leak-canary.sh*) ok "4b  and it names the file that is missing" ;;
    *)                bad "4b  the missing library is named" "got: $OUT4" ;;
esac

# ===========================================================================
# 5. RED ON THE OPERATOR'S RECORD (round 15, 2026-09-11). A suite whose own
#    assertions pass and which leaves the checkout clean appends a `terminated`
#    row to the ownership ledger under the config directory — the exact shape
#    session-start-stdin.test.sh 9b produced through the shipped reaper. The
#    runner must fail it, name it, and print the row. Beside it (5d, amended
#    2026-09-27), a clean suite during which ANOTHER process writes the live
#    record stays green: each suite runs in a throwaway home and the canary
#    watches the record there, so activity elsewhere is never charged to it.
#
#    The suites resolve the record at RUN time, from their own environment, as
#    the shipped reaper did. A path baked in before the suite started is the
#    one road the library's header names as out of its sight.
# ===========================================================================
fi
E5="$SANDBOX/e5"
FLAGS5="$SANDBOX/flags5"; mkdir -p "$FLAGS5"
build_engine "$E5"
clean_suite "$E5/engine/scripts/hooks/alpha.test.sh"
cat > "$E5/engine/scripts/hooks/toucher.test.sh" <<'EOF'
#!/usr/bin/env bash
cfg="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
mkdir -p "$cfg/state"
printf '{"event": "terminated", "agent_id": "ae904aac1949e5696", "teammate": "sage-fable-cert3", "witness": "platform-terminal-record", "ts": "2026-09-11T00:02:34.984941+00:00"}\n' >> "$cfg/state/worktree-ledger.jsonl"
echo "  PASS  every assertion passed"
exit 0
EOF
# platform-turn.test.sh: clean, and it waits (bounded) until the concurrent writer below
# has written the live record, so that write lands WHILE the suite runs.
cat > "$E5/engine/scripts/hooks/platform-turn.test.sh" <<EOF
#!/usr/bin/env bash
touch "$FLAGS5/started"
n=0
while [ ! -e "$FLAGS5/written" ] && [ "\$n" -lt 300 ]; do sleep 0.1; n=\$((n + 1)); done
[ -e "$FLAGS5/written" ] || { echo "  FAIL  the concurrent writer never wrote; this case proves nothing"; exit 7; }
echo "  PASS  every assertion passed"
exit 0
EOF
chmod +x "$E5/engine/scripts/hooks/toucher.test.sh" "$E5/engine/scripts/hooks/platform-turn.test.sh"
git_q "$E5" add -A; git_q "$E5" commit -q -m suites
(
    n=0
    while [ ! -e "$FLAGS5/started" ] && [ "$n" -lt 1200 ]; do sleep 0.1; n=$((n + 1)); done
    [ -e "$FLAGS5/started" ] || exit 0
    printf '{"event": "registered", "agent_id": "live1", "teammate": "echo-opus-live1", "source": "detect-nonnative-worktree.sh", "ts": "t"}\n' >> "$CLAUDE_CONFIG_DIR/state/worktree-ledger.jsonl"
    mkdir -p "$CLAUDE_CONFIG_DIR/teams/session-cafebabe"
    touch "$FLAGS5/written"
) &
WRITER5=$!
OUT5="$( cd "$E5/engine" && bash scripts/run-all-tests.sh 2>&1 )"; RC5=$?
wait "$WRITER5" 2>/dev/null
if [ "$RC5" -ne 0 ]; then
    ok "5a  RED: a suite whose assertions pass and whose checkout stays clean still fails the run because it appended a row to the operator's ledger"
else
    bad "5a  a record-touching suite fails the run" "rc=0 — the suite wrote a terminated row into the watched ledger and the runner stayed green:
$OUT5"
fi
TOUCHER_LINE="$(printf '%s\n' "$OUT5" | sed $'s/\033\\[[0-9;]*m//g' | grep 'toucher\.test\.sh')"
case "$TOUCHER_LINE" in
    *"touched the operator"*) ok "5b  and the suite is NAMED on its own line as having touched the operator's record — not as residue, which is a different finding" ;;
    "") bad "5b  the touching suite is named" "toucher.test.sh has no line in the output at all, so it was not run" ;;
    *)  bad "5b  the touching suite is named" "toucher's own line does not say so: $TOUCHER_LINE" ;;
esac
case "$OUT5" in
    *"event=terminated"*"witness=platform-terminal-record"*"teammate=sage-fable-cert3"*)
        ok "5c  and the row it wrote is printed (event, witness, teammate), so a reader can tell a false witness from a fixture at a glance" ;;
    *)  bad "5c  the row is printed" "got: $OUT5" ;;
esac
TURN_LINE="$(printf '%s\n' "$OUT5" | sed $'s/\033\\[[0-9;]*m//g' | grep 'platform-turn\.test\.sh')"
case "$TURN_LINE" in
    *PASS*) if [ -e "$FLAGS5/written" ]; then
                ok "5d  and a clean suite during which ANOTHER process wrote the live record is NOT blamed — the canary counts only the suite's own writes"
            else
                bad "5d  the concurrent write happened during the suite" "the writer never wrote; the case proves nothing"
            fi ;;
    "")     bad "5d  the suite beside a concurrent writer is not blamed" "turn.test.sh has no line in the output" ;;
    *)      bad "5d  the suite beside a concurrent writer is not blamed" "got: $TURN_LINE" ;;
esac
if [ ! -e "$CLAUDE_CONFIG_DIR/state/worktree-ledger.jsonl" ] || ! grep -q 'sage-fable-cert3' "$CLAUDE_CONFIG_DIR/state/worktree-ledger.jsonl"; then
    ok "5g  and the toucher's row landed in its own throwaway home, never in the operator's record"
else
    bad "5g  the toucher's row stayed out of the operator's record" "it is in $CLAUDE_CONFIG_DIR/state/worktree-ledger.jsonl"
fi
rm -rf "$CLAUDE_CONFIG_DIR/teams/session-cafebabe" "$CLAUDE_CONFIG_DIR/state/worktree-ledger.jsonl"
case "$OUT5" in
    *"record canary: watching"*) ok "5e  and the runner announces the record canary and what it watches, so a reader can tell this run from one where it was absent" ;;
    *) bad "5e  the record canary announces itself" "got: $OUT5" ;;
esac
E6="$SANDBOX/e6"
build_engine "$E6"
clean_suite "$E6/engine/scripts/hooks/alpha.test.sh"
rm -f "$E6/engine/scripts/lib/record-canary.sh"
OUT6="$( cd "$E6/engine" && bash scripts/run-all-tests.sh 2>&1 )"; RC6=$?
if [ "$RC6" -eq 2 ] && printf '%s' "$OUT6" | grep -q 'record-canary.sh'; then
    ok "5f  the runner REFUSES (rc 2) and names the file when the record canary's library is missing — a partial install cannot report a green fraction without it"
else
    bad "5f  a missing record canary library is refused" "rc=$RC6: $OUT6"
fi

# ===========================================================================
# 7. THE HOST SHIELD (ceo-decisions §65; lib/host_shield.py). A suite that calls the host halt
#    command by bare name fails with the stand-in's logged line; one whose source names it by
#    path is refused before it runs. The throwaway suites and their walls are
#    lib/host-shield-fixture.sh's; PATH is the tripwire first and neither sbin. The fixture
#    engine has no shield of its own and uses the exported one.
# ===========================================================================
if [ "${1:-}" != --record-only ]; then
# shellcheck source=lib/host-shield-fixture.sh
. "$ENGINE_ROOT/scripts/lib/host-shield-fixture.sh"
for which in bare path; do
    E7="$SANDBOX/e7-$which"
    build_engine "$E7"
    hs_fixture "$SANDBOX/hstrip-$which" "$SANDBOX/hs-suites-$which"
    cp "$SANDBOX/hs-suites-$which/hs-$which.test.sh" "$E7/engine/scripts/hooks/"
    git_q "$E7" add -A; git_q "$E7" commit -q -m suites
    OUT7="$( cd "$E7/engine" && PATH="$(hs_path "$SANDBOX/hstrip-$which")" RICHOS_HOST_SHIELD_TOOL="$HS_TOOL" \
        bash scripts/run-all-tests.sh 2>&1 )"; RC7=$?
    if [ "$which" = bare ]; then
        if [ "$RC7" -ne 0 ] && printf '%s' "$OUT7" | grep -q "FAIL  host-shield: refused \`$HS_CMD -h +60\`" \
                && [ ! -s "$SANDBOX/hstrip-bare/trip.log" ]; then
            ok "7a  a suite calling $HS_CMD -h +60 by bare name fails the run with the stand-in's logged line; nothing reached the tripwire"
        else
            bad "7a  a bare-name host call is stood in for" "rc=$RC7: $OUT7"
        fi
    else
        if [ "$RC7" -ne 0 ] && printf '%s' "$OUT7" | grep -q "REFUSED before it ran" \
                && [ ! -e "$SANDBOX/hstrip-path/path-suite-ran" ]; then
            ok "7b  a suite whose source names /sbin/$HS_CMD is refused before it runs"
        else
            bad "7b  a by-path host call is refused before the suite runs" "rc=$RC7: $OUT7"
        fi
    fi
done
# 7c — with no shield beside it and none exported, the runner refuses (rc 2) and starts no suite.
E7C="$SANDBOX/e7-none"
build_engine "$E7C"
printf '%s\n' '#!/usr/bin/env bash' "touch \"$SANDBOX/e7c-started\"" 'echo "  PASS  started"' \
    > "$E7C/engine/scripts/hooks/started.test.sh"
git_q "$E7C" add -A; git_q "$E7C" commit -q -m suites
OUT7="$( cd "$E7C/engine" && PATH="$(hs_path "$SANDBOX/hstrip-bare")" env -u RICHOS_HOST_SHIELD_TOOL \
    bash scripts/run-all-tests.sh 2>&1 )"; RC7=$?
if [ "$RC7" -eq 2 ] && printf '%s' "$OUT7" | grep -q "REFUSED: NO HOST SHIELD" && [ ! -e "$SANDBOX/e7c-started" ]; then
    ok "7c  with no shield beside it and none exported, the runner refuses (rc 2) and no suite starts"
else
    bad "7c  a suite started without the host shield" "rc=$RC7: $OUT7"
fi
fi

printf '\n  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
