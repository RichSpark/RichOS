#!/usr/bin/env bash
#
# waiver-repetition.test.sh — the suite for notice-waiver-repetition.{sh,py}
#                             and scripts/waiver-repetition-lint.sh.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# The mechanism claims two things that pull in opposite directions:
#
#   A HATCH USED REPEATEDLY FOR THE SAME REASON IS SURFACED.
#   A SINGLE CONSIDERED WAIVER IS NOT.
#
# Either one alone is easy and useless. A checker that reports everything gets
# muted in a week; a checker that reports nothing passes every test it has. So
# every case here is TWO-SIDED — the positive case has a negative twin one
# variable away, and both are asserted.
#
# Every case in this file was run RED before it was run green, by breaking the
# shipped source rather than the fixture: the mutations are enumerated in
# scripts/hooks/waiver-repetition.mutation.sh, which re-derives that proof on
# demand instead of asking anyone to take this paragraph's word for it.
#
# Usage:  scripts/hooks/waiver-repetition.test.sh [-v]

set -uo pipefail
# ERREXIT DELIBERATELY OFF. Nearly every command in this suite is EXPECTED to
# exit non-zero — a flagged verdict is exit 1 and an unreadable target is exit
# 2 — so a stray `set -e` would end the run at the first case that works, which
# is precisely how a suite reports green over the half it never reached.

VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t waiver-repetition.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

# ---------------------------------------------------------------------------
# SANDBOX THE OPERATOR'S CONFIG DIR, for the whole suite, before anything runs.
#
# WHY: this suite drives the hook TWO WAYS, and only one of them was sealed.
#
#   run_py   calls the analyzer directly and passes --teams-root explicitly, at
#            a path inside $SANDBOX that does not exist. Sealed.
#   run_hook calls notice-waiver-repetition.sh, the shipped wrapper, which does
#            NOT pass --teams-root — correctly, because in production there is
#            no better value to pass. The analyzer then falls back to
#            "${CLAUDE_CONFIG_DIR:-~/.claude}/teams" (notice-waiver-
#            repetition.py, main()) and enumerates EVERY session team directory
#            on the operator's real machine as a ledger source.
#
# So cases 15, 15b and 15c were reading the operator's live sessions as part of
# their own fixture. That is not a theory. Case 15c truncates the ledger to one
# entry and asserts the recovery line; on 2026-09-06 it went red on this machine
# reporting "fixture-hatch-guard.sh (14x one reason, 14 subjects over 2 days)" —
# a count the case did not write and could not have written. It was green four
# times that evening and red five times after, with no commit in between: the
# variable that moved was the operator's ~/.claude/teams, not this repository.
# Reproduced deterministically by planting 14 same-reason entries in a fake
# teams dir and changing nothing else.
#
# THE HOOK IS RIGHT AND THE SUITE WAS WRONG. Reading the team directories is
# real production behavior — hatch ledgers genuinely live there — and case 20a
# is the standing promise that this watcher has no key to stand it down with.
# What was wrong was a sandbox that sealed one entry point and inherited the
# machine through the other. A suite whose verdict depends on what the operator
# did in an unrelated session is not measuring the code.
#
# Sandboxing CLAUDE_CONFIG_DIR is the discipline every other suite in this
# engine already follows (scripts/lib/global-state-witness.test.sh case c5
# names it as the correct one). Here it seals a READ rather than a write, which
# is the same defect one direction over.
#
# The teams dir is created EMPTY rather than left absent, so that case 15d can
# plant a decoy in it and prove this seal did not work by breaking the feature.
# ---------------------------------------------------------------------------
CLAUDE_CONFIG_DIR="$SANDBOX/config"
export CLAUDE_CONFIG_DIR
mkdir -p "$CLAUDE_CONFIG_DIR/teams"
# The governed repository comes from each payload's cwd (and, for case 17, the
# directory the hook runs from), never from the launching session's.
unset RICHOS_ENTITY_ROOT CLAUDE_PROJECT_DIR

REPO="$SANDBOX/repo"
mkdir -p "$REPO/scripts/hooks" "$REPO/scripts/lib" "$REPO/.claude/state"

cp "$SRC_DIR/notice-waiver-repetition.sh" "$REPO/scripts/hooks/"
cp "$SRC_DIR/notice-waiver-repetition.py" "$REPO/scripts/hooks/"
chmod +x "$REPO/scripts/hooks/notice-waiver-repetition.sh"
for l in resolve-roots.sh resolve-main-checkout.sh seat-jurisdiction.sh \
         stop-hook-notice.sh; do
    cp "$SRC_DIR/../lib/$l" "$REPO/scripts/lib/$l" 2>/dev/null || true
done
cp "$ENGINE_ROOT/scripts/waiver-repetition-lint.sh" "$REPO/scripts/"
chmod +x "$REPO/scripts/waiver-repetition-lint.sh"

HOOK="$REPO/scripts/hooks/notice-waiver-repetition.sh"
PY="$REPO/scripts/hooks/notice-waiver-repetition.py"
LINT="$REPO/scripts/waiver-repetition-lint.sh"
STATE="$REPO/.claude/state"

printf 'PROTECTED_PATHS="app"\n' > "$REPO/orchestration.config"

# ---------------------------------------------------------------------------
# THE SANDBOX'S OWN GUARDS. Discovery reads these, not a list — so the fixture
# has to contain real append sites for the scan to find, exactly as the engine
# does. One declares an escape hatch; one writes a plain event record and
# declares nothing. The pair is what proves the classifier is doing work
# rather than accepting everything.
# ---------------------------------------------------------------------------
cat > "$REPO/scripts/hooks/fixture-hatch-guard.sh" <<'GUARD'
#!/usr/bin/env bash
# fixture-hatch-guard.sh — refuses a thing, with one way through.
#
# The escape hatch is a live prompt line 'fixture-ack: <reason>'. Allowed and
# logged, never silent.
LOG_DIR="$ENTITY_ROOT/.claude/state"
mkdir -p "$LOG_DIR" 2>/dev/null || true
printf '%s\tto=%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$TARGET" "$FIXTURE_ACK" \
    >>"$LOG_DIR/fixture-acks.log" 2>/dev/null || true
GUARD

cat > "$REPO/scripts/hooks/fixture-record-writer.sh" <<'REC'
#!/usr/bin/env bash
# fixture-record-writer.sh — writes down what happened.
#
# This file permits nothing and refuses nothing. Every spawn's name is
# appended so a later sweep can enumerate them; there is no way through it
# because there is nothing to get through.
printf '%s\n' "$NAME" >>"$TEAM_DIR/fixture-names.log" 2>/dev/null || true
REC

# ---------------------------------------------------------------------------
# fixtures
# ---------------------------------------------------------------------------
LEDGER="$STATE/fixture-acks.log"
TODAY="2026-09-02"

write_ledger() { : > "$LEDGER"; }
add() { # <YYYY-MM-DD> <subject> <reason>
    printf '%sT12:00:00Z\tto=%s\tfixture-ack: %s\n' "$1" "$2" "$3" >> "$LEDGER"
}

run_py() { # extra args...
    python3 "$PY" --engine-root "$REPO" --entity-root "$REPO" \
        --teams-root "$SANDBOX/no-teams" --today "$TODAY" "$@" 2>&1
}
run_hook() { # <session-id>
    printf '{"session_id":"%s","cwd":"%s","hook_event_name":"Stop","transcript_path":"/dev/null"}' \
        "$1" "$REPO" | "$HOOK" 2>&1
}

echo "=== waiver-repetition.test.sh ==="
echo ""

# ===========================================================================
# 1 — THE QUIET SIDE. A single considered waiver says nothing.
# ===========================================================================
write_ledger
add 2026-09-02 alpha "one-off: the recipient is a live agent I verified by hand"
OUT="$(run_py --one-liner)"; RC=$?
say "1" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "1. ONE considered waiver is silent"
else
    bad "1. ONE considered waiver is silent" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 2 — still quiet at two. Two is that exception recurring, not a class.
# ===========================================================================
add 2026-09-01 beta "one-off: the recipient is a live agent I verified by hand"
OUT="$(run_py --one-liner)"; RC=$?
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "2. TWO waivers of the same reason are still silent"
else
    bad "2. TWO waivers of the same reason are still silent" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 3 — THE LOUD SIDE. The third use of the same reason is a false-positive
#     class, and it is named with its guard and its count.
# ===========================================================================
add 2026-08-31 gamma "one-off: the recipient is a live agent I verified by hand"
OUT="$(run_py --one-liner)"; RC=$?
say "3" "rc=$RC out=$OUT"
if [ "$RC" -eq 1 ] \
   && printf '%s' "$OUT" | grep -q "fixture-hatch-guard.sh" \
   && printf '%s' "$OUT" | grep -q "3x one reason"; then
    ok "3. THREE waivers of one reason surface, naming the guard and the count"
else
    bad "3. THREE waivers of one reason surface, naming the guard and the count" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 4 — REWORDED EVERY TIME, and still one class. This is the case exact-match
#     grouping fails: the real ledger holds 226 distinct strings for what is
#     substantially two reasons.
# ===========================================================================
# The three lines below are the shape of the real 88-entry class, and no two
# of them are the same string — so exact-text grouping finds three groups of
# one and says nothing at all. That is the property under test.
write_repeated_class() {
    write_ledger
    add 2026-09-02 alpha "recipient is the currently-running teammate spawned this session with native isolation:worktree; background agents never appear in the session roster, so every write still lands in its own existing isolated worktree"
    add 2026-09-01 beta  "the recipient is a live background teammate spawned minutes ago with native isolation:worktree and absent from the session roster by design; every write still lands in its own existing isolated worktree"
    add 2026-08-30 gamma "recipient is a currently-running background agent holding native isolation:worktree - background agents are absent from the session roster, and every write lands in its own existing isolated worktree as before"
}

# ---------------------------------------------------------------------------
# THE SAME CLASS, DATED AGAINST THE REAL CLOCK — for the run_hook cases only.
#
# run_py is told what day it is (--today "$TODAY"), so absolute fixture dates
# mean the same thing in it forever. THE WRAPPER CANNOT BE TOLD: it invokes the
# analyzer with no --today, deliberately, because in production the day is the
# day. So every run_hook case is evaluated against datetime.date.today(), and
# an absolutely-dated fixture is worth less every morning.
#
# It expires on a date that can be named. ACTIVE_DAYS is 14, measured from the
# NEWEST entry of a class, and the newest above is 2026-09-02 — so this fixture
# stops being a repeated class on 2026-09-17, and cases 15 and 15b go red that
# morning with nobody having changed anything. Measured on the shipped analyzer
# rather than reasoned about:
#
#   --today 2026-09-16  ->  "fixture-hatch-guard.sh (3x one reason, 3 subjects over 3 days)"
#   --today 2026-09-17  ->  no notice at all
#
# That is the identical defect the sandboxed config dir above fixes, one input
# over: the wrapper path inherits the operator's machine, and the machine is
# both its team directories AND its clock. Fixing half of it would have left a
# dated bomb behind a green suite.
#
# The relative offsets are 0/1/2 days rather than the original 0/1/3, so the
# class stays inside ACTIVE_DAYS on any day, and >= 2 distinct days keeps the
# independence test (is_repeated) satisfied exactly as the absolute fixture did.
# ---------------------------------------------------------------------------
days_ago() { # <n> -> YYYY-MM-DD, n days before the REAL today
    python3 -c 'import datetime,sys; print(datetime.date.today() - datetime.timedelta(days=int(sys.argv[1])))' "$1"
}
# THE OTHER HALF, for the run_py cases: n days before "$TODAY", the day run_py
# tells the analyzer it is. A run_py fixture dated with days_ago is dated
# against one clock and judged against another, so its entries sit weeks in the
# analyzer's FUTURE and their idle_days go negative. The verdict mostly survives
# that (only "idle > ACTIVE_DAYS" excludes), but a case about ORDER does not:
# the recency bucket is idle_days // RECENCY_BUCKET, and floor division of
# negative ages puts "today" and "yesterday" in different buckets on some real
# days and not others. That is how 21b went red on 2026-10-01 with nobody
# having changed anything: idle -29 and -28 landed in buckets -5 and -4.
days_before_today() { # <n> -> YYYY-MM-DD, n days before $TODAY
    python3 -c 'import datetime,sys; y,m,d=(int(x) for x in sys.argv[1].split("-")); print(datetime.date(y,m,d) - datetime.timedelta(days=int(sys.argv[2])))' "$TODAY" "$1"
}
write_repeated_class_now() {
    write_ledger
    add "$(days_ago 0)" alpha "recipient is the currently-running teammate spawned this session with native isolation:worktree; background agents never appear in the session roster, so every write still lands in its own existing isolated worktree"
    add "$(days_ago 1)" beta  "the recipient is a live background teammate spawned minutes ago with native isolation:worktree and absent from the session roster by design; every write still lands in its own existing isolated worktree"
    add "$(days_ago 2)" gamma "recipient is a currently-running background agent holding native isolation:worktree - background agents are absent from the session roster, and every write lands in its own existing isolated worktree as before"
}
write_repeated_class
if [ "$(sort -u "$LEDGER" | wc -l | tr -d ' ')" -eq 3 ]; then
    ok "4a. the fixture is three DISTINCT strings — exact-text grouping would see nothing"
else
    bad "4a. the fixture is three DISTINCT strings — exact-text grouping would see nothing"
fi
OUT="$(run_py --one-liner)"; RC=$?
say "4" "rc=$RC out=$OUT"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "3x one reason"; then
    ok "4. the same reason RETYPED three ways is one class"
else
    bad "4. the same reason RETYPED three ways is one class" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 5 — its twin. Three waivers, three genuinely different reasons, silence.
#     Without this, case 4 passes for a checker that clusters everything.
# ===========================================================================
write_ledger
add 2026-09-02 alpha "recipient is an active background teammate holding an isolated worktree"
add 2026-09-01 beta  "cross-repository land notice required by the push gate before a merge"
add 2026-08-30 gamma "pure question with no file-bearing follow-up of any kind"
OUT="$(run_py --one-liner)"; RC=$?
say "5" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "5. three DIFFERENT reasons stay silent"
else
    bad "5. three DIFFERENT reasons stay silent" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 6 — INDEPENDENCE. Three uses of one reason on one day for ONE subject is one
#     act restated, not a class, and stays quiet.
# ===========================================================================
write_ledger
add 2026-09-02 alpha "the recipient is a live background agent holding its own worktree"
add 2026-09-02 alpha "the recipient is a live background agent holding its own worktree today"
add 2026-09-02 alpha "the recipient is a live background agent still holding its own worktree"
OUT="$(run_py --one-liner)"; RC=$?
say "6" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "6. one day, ONE subject: an act restated is not a class"
else
    bad "6. one day, ONE subject: an act restated is not a class" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 7 — its twin, one variable away: same day, same reason, THREE subjects.
#     Three independent acts excused identically is the ceo-todos-defers shape.
# ===========================================================================
write_ledger
add 2026-09-02 alpha "the recipient is a live background agent holding its own worktree"
add 2026-09-02 beta  "the recipient is a live background agent holding its own worktree today"
add 2026-09-02 gamma "the recipient is a live background agent still holding its own worktree"
OUT="$(run_py --one-liner)"; RC=$?
say "7" "rc=$RC out=$OUT"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "3 subjects"; then
    ok "7. one day, THREE subjects: three independent acts, surfaced"
else
    bad "7. one day, THREE subjects: three independent acts, surfaced" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 8 — THE FIX MUST BUY SILENCE. A class whose last use is outside the activity
#     window is history, not a live defect. Without this the notice becomes the
#     permanent line the operator learns to skip.
# ===========================================================================
write_ledger
add 2026-08-01 alpha "the recipient is a live background agent holding its own worktree"
add 2026-08-02 beta  "the recipient is a live background agent holding its own worktree today"
add 2026-08-03 gamma "the recipient is a live background agent still holding its own worktree"
OUT="$(run_py --one-liner)"; RC=$?
say "8" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "8. a class last used 30 days ago has aged out"
else
    bad "8. a class last used 30 days ago has aged out" "rc=$RC out=$OUT"
fi

# 8b — its twin: the identical class, one day inside the window, speaks.
write_ledger
add 2026-08-20 alpha "the recipient is a live background agent holding its own worktree"
add 2026-08-21 beta  "the recipient is a live background agent holding its own worktree today"
add 2026-08-22 gamma "the recipient is a live background agent still holding its own worktree"
OUT="$(run_py --one-liner)"; RC=$?
if [ "$RC" -eq 1 ]; then
    ok "8b. the same class 11 days old still speaks"
else
    bad "8b. the same class 11 days old still speaks" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 9 — THE DOUBLE-FIRE ARTIFACT. install.sh documents a host that fires a hook
#     twice, producing two byte-identical rows for ONE act. Counting them would
#     turn every pair into a class.
# ===========================================================================
write_ledger
for _i in 1 2; do
    printf '2026-09-02T12:00:00Z\tto=alpha\tfixture-ack: %s\n' \
        "the recipient is a live background agent holding its own worktree" >> "$LEDGER"
    printf '2026-09-01T12:00:00Z\tto=beta\tfixture-ack: %s\n' \
        "the recipient is a live background agent holding its own worktree today" >> "$LEDGER"
done
OUT="$(run_py --one-liner)"; RC=$?
say "9" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "9. four lines that are two acts double-fired stay silent"
else
    bad "9. four lines that are two acts double-fired stay silent" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 10 — DISCOVERY IS DERIVED FROM THE GUARDS. The hatch ledger is found because
#      a guard appends to it and declares a way through; the event record is
#      found and deliberately NOT treated as a hatch.
# ===========================================================================
write_ledger
add 2026-09-02 alpha "a single considered waiver"
OUT="$(run_py)"
say "10" "$OUT"
if printf '%s' "$OUT" | grep -q "fixture-acks.log" \
   && printf '%s' "$OUT" | grep -A3 "NOT CLASSIFIED AS A HATCH" | grep -q "fixture-names.log"; then
    ok "10. the hatch is derived from its guard; the plain record is not a hatch"
else
    bad "10. the hatch is derived from its guard; the plain record is not a hatch" "$OUT"
fi

# ===========================================================================
# 11 — A LEDGER NOBODY CLAIMS IS NAMED, NOT DROPPED. This is where a broken
#      deriver stops being invisible: a shorter list would otherwise read as a
#      cleaner report.
# ===========================================================================
printf '2026-09-02T12:00:00Z\tsomething\n' > "$STATE/orphan-acks.log"
OUT="$(run_py)"
if printf '%s' "$OUT" | grep -A3 "CLAIMED BY NO GUARD" | grep -q "orphan-acks.log"; then
    ok "11. a ledger no guard claims is reported as unattributed"
else
    bad "11. a ledger no guard claims is reported as unattributed" "$OUT"
fi
rm -f "$STATE/orphan-acks.log"

# ===========================================================================
# 11b — THE NAME CROSSES A LANGUAGE BOUNDARY, AND THE SCAN FOLLOWS IT.
#
# Five guards in this engine embed a Python program in a shell heredoc and hand
# it the ledger path through the environment. The append site then reads
# `open(log_path, "a")` while the only place the name is spelled is a shell
# assignment hundreds of lines above. guard-ci-red-lands.sh is the one that
# mattered: 93 acks, 35 of them for a single workflow, and until 2026-09-14 the
# resolver followed a variable to a variable only through the SHELL spelling
# `$VAR`, so the chain broke at the one link that changes language and the
# ledger was never named by the scan at all.
#
# THE SECOND HALF IS THE TENSE. A guard writes its log line in the past — "was
# acked with a real reason" — and the vocabulary required the word to end at
# "ack", so even once the name resolved the ledger was filed as a plain record
# and never analyzed. Both halves are in this one fixture, which is why two
# mutants aim at this case.
#
# The guard below is READ, never run. It is the only thing the discovery scan
# has to go on, exactly as the real ones are.
# ===========================================================================
cat > "$REPO/scripts/hooks/fixture-envhop-guard.sh" <<'ENVHOP'
#!/usr/bin/env bash
# fixture-envhop-guard.sh — refuses a thing, with one way through.
#
# The ledger name is spelled ONCE, here, in shell. The write happens inside an
# embedded Python program that receives the path through the environment.
ENVHOP_LOG="$ENTITY_ROOT/.claude/state/fixture-envhop.log"
export ENVHOP_LOG
python3 <<'PY'
import os
log_path = os.environ["ENVHOP_LOG"]

if REFUSAL_WAS_OVERRIDDEN:
    # Every refusal that was acked with a real reason is recorded here, and the
    # count is what shows one becoming a habit.
    with open(log_path, "a", encoding="utf-8") as f:
        f.write(os.environ.get("ROW", ""))
PY
ENVHOP

envhop_add() { # <days-before-TODAY> <subject> <reason>; read by run_py below
    printf '%sT12:00:00Z\tto=%s\tfixture-ack: %s\n' "$(days_before_today "$1")" "$2" "$3" \
        >> "$STATE/fixture-envhop.log"
}
: > "$STATE/fixture-envhop.log"
envhop_add 0 nu  "the recipient is a running teammate holding an isolated worktree of its own, so every write still lands inside that worktree"
envhop_add 1 xi  "recipient is a currently-running teammate that holds its own isolated worktree; every write lands inside that worktree as before"
envhop_add 2 pi  "the recipient is a live teammate with an isolated worktree of its own and every write still lands within that worktree"

OUT="$(run_py)"
say "11b" "$OUT"
if printf '%s' "$OUT" | grep -q "fixture-envhop.log *3 entries" \
   && printf '%s' "$OUT" | grep -q "fixture-envhop-guard.sh"; then
    ok "11b. a ledger named across the shell/environment/Python hop is derived and analyzed"
else
    bad "11b. a ledger named across the shell/environment/Python hop is derived and analyzed" "$OUT"
fi

# 11c — THE NEGATIVE TWIN, and it guards the widened vocabulary rather than the
#       hop. Following one more kind of reference is only worth having if the
#       classifier still says no to something: this guard crosses the identical
#       env hop and permits nothing, so it must stay a plain record. Without
#       this, "follow the hop" and "call everything a hatch" pass the same test.
cat > "$REPO/scripts/hooks/fixture-envhop-record.sh" <<'ENVREC'
#!/usr/bin/env bash
# fixture-envhop-record.sh — writes down what happened.
#
# Same shape as its neighbor and none of its meaning: this file permits
# nothing and refuses nothing, so there is nothing to get through.
ENVREC_LOG="$ENTITY_ROOT/.claude/state/fixture-envrec.log"
export ENVREC_LOG
python3 <<'PY'
import os
log_path = os.environ["ENVREC_LOG"]

if SOMETHING_HAPPENED:
    # One line per event, so a later sweep can enumerate them in order.
    with open(log_path, "a", encoding="utf-8") as f:
        f.write(os.environ.get("ROW", ""))
PY
ENVREC

printf '2026-09-02T12:00:00Z\tsomething happened\n' > "$STATE/fixture-envrec.log"
OUT="$(run_py)"
if printf '%s' "$OUT" | grep -A3 "NOT CLASSIFIED AS A HATCH" | grep -q "fixture-envrec.log"; then
    ok "11c. the same hop with no hatch vocabulary stays a plain record"
else
    bad "11c. the same hop with no hatch vocabulary stays a plain record" "$OUT"
fi

rm -f "$STATE/fixture-envhop.log" "$STATE/fixture-envrec.log"

# ===========================================================================
# 12 — FINDING NOTHING IS A FAILURE, NOT AN ALL-CLEAR. The engine's own
#      history is a typed sandbox list that fell behind and turned a dead
#      scanner into a passing layer. A discovery scan that matches nothing must
#      never be indistinguishable from a clean engine.
# ===========================================================================
EMPTY="$SANDBOX/empty-engine"
mkdir -p "$EMPTY/scripts/hooks"
printf '#!/usr/bin/env bash\necho nothing\n' > "$EMPTY/scripts/hooks/inert.sh"
OUT="$(python3 "$PY" --engine-root "$EMPTY" --entity-root "$REPO" \
        --teams-root "$SANDBOX/no-teams" --today "$TODAY" 2>&1)"; RC=$?
say "12" "rc=$RC out=$OUT"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "BROKEN"; then
    ok "12. an engine with no append site at all is BROKEN, never all-clear"
else
    bad "12. an engine with no append site at all is BROKEN, never all-clear" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 13 — THE GROUPING CONSTANT IS NOT LOAD-BEARING. The verdict is asserted to
#      be identical across the whole measured range of the Jaccard threshold,
#      so a later edit cannot quietly make one number decide the answer.
# ===========================================================================
sweep_jaccard() { # <expected-rc>
    local want="$1" j rc all_ok=1 seen=""
    for j in 0.25 0.35 0.45 0.55; do
        set +e
        python3 "$PY" --engine-root "$REPO" --entity-root "$REPO" \
            --teams-root "$SANDBOX/no-teams" --today "$TODAY" --jaccard "$j" \
            --one-liner >/dev/null 2>&1
        rc=$?
        set +e
        seen="$seen $j=$rc"
        [ "$rc" -eq "$want" ] || all_ok=0
    done
    J_SEEN="$seen"
    return $(( 1 - all_ok ))
}

write_repeated_class
if sweep_jaccard 1; then
    ok "13. a repeated class is flagged at Jaccard 0.25 / 0.35 / 0.45 / 0.55 alike"
else
    bad "13. a repeated class is flagged at Jaccard 0.25 / 0.35 / 0.45 / 0.55 alike" "$J_SEEN"
fi

# 13b — the other direction, which is the one a loose threshold breaks: three
#       genuinely different reasons must stay silent even at 0.25, where
#       single-linkage chains hardest.
write_ledger
add 2026-09-02 alpha "recipient is an active background teammate holding an isolated worktree of its own"
add 2026-09-01 beta  "cross-repository land notice required by the push gate before a merge can complete"
add 2026-08-30 gamma "a pure question with no file-bearing follow-up of any kind is being sent here"
if sweep_jaccard 0; then
    ok "13b. three different reasons stay silent at all four thresholds, 0.25 included"
else
    bad "13b. three different reasons stay silent at all four thresholds, 0.25 included" "$J_SEEN"
fi

# ===========================================================================
# 14 — THE STATE KEY. Stable while the situation is, different when a class
#      doubles. This is what keeps the notice from being repeated under every
#      turn, which is the failure mode that gets a notice muted.
# ===========================================================================
write_repeated_class
K1="$(run_py --state-key)"
K2="$(run_py --state-key)"
add 2026-08-29 delta "recipient is a currently-running teammate spawned this session with native isolation:worktree; background agents never appear in the session roster, so every write still lands in its own existing isolated worktree"
add 2026-08-28 epsilon "the recipient is a live background teammate spawned minutes ago with native isolation:worktree, absent from the session roster by design; every write still lands in its own existing isolated worktree"
add 2026-08-27 zeta "recipient is a currently-running background agent with native isolation:worktree — background agents are absent from the session roster, and every write lands in its own existing isolated worktree"
K3="$(run_py --state-key)"
if [ "$K1" = "$K2" ] && [ "$K1" != "$K3" ] && [ "${K1#repeated:}" != "$K1" ]; then
    ok "14. the state key is stable while the situation is, and moves when it doubles"
else
    bad "14. the state key is stable while the situation is, and moves when it doubles" "K1=$K1 K2=$K2 K3=$K3"
fi

# ===========================================================================
# 14b — WHAT DID NOT CHANGE IS NOT READ AGAIN (2026-10-22). The Stop hook spent
#       6-9 CPU-s a turn re-reading and re-clustering ledgers that had not
#       changed. An unchanged ledger is not opened at all: made unreadable
#       after one run, the next run still reports it as it was (before the
#       memo it read as empty). A ledger that grows is read again.
# ===========================================================================
write_repeated_class
R1="$(run_py --json)"
chmod 000 "$LEDGER"
R2="$(run_py --json)"
chmod 644 "$LEDGER"
add 2026-08-29 delta "recipient is a currently-running teammate spawned this session with native isolation:worktree; background agents never appear in the session roster, so every write still lands in its own existing isolated worktree"
R3="$(run_py --one-liner)"
if [ "$R1" = "$R2" ] && printf '%s' "$R1" | grep -q '"size": 3' \
   && printf '%s' "$R3" | grep -q "4x one reason" \
   && ls "$CLAUDE_CONFIG_DIR"/state/waiver-repetition-memo-*.json >/dev/null 2>&1; then
    ok "14b. an unchanged ledger is not read again, and a grown one is"
else
    bad "14b. an unchanged ledger is not read again, and a grown one is" "R3=$R3"
fi

# From here to the end of 15d the driver is run_hook, which cannot be told what
# day it is — so the fixture is dated against the real clock. See
# write_repeated_class_now.
write_repeated_class_now

# ===========================================================================
# 15 — THE HOOK, END TO END. It says the sentence on the one channel measured
#      to reach the operator, and it does NOT block.
# ===========================================================================
OUT="$(run_hook waivtst1)"; RC=$?
say "15" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] \
   && printf '%s' "$OUT" | grep -q '"systemMessage"' \
   && printf '%s' "$OUT" | grep -q "REPEATED WAIVERS"; then
    ok "15. the Stop hook emits systemMessage and exits 0 — it reports, never blocks"
else
    bad "15. the Stop hook emits systemMessage and exits 0 — it reports, never blocks" "rc=$RC out=$OUT"
fi

# 15b — the second identical turn is silent. A line under every turn is a line
#       the eye skips, and this hook of all hooks must not become that.
OUT="$(run_hook waivtst1)"; RC=$?
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "15b. the same condition on the next turn is silent"
else
    bad "15b. the same condition on the next turn is silent" "rc=$RC out=$OUT"
fi

# 15c — and when the hatches go quiet, the operator is told the story ended.
write_ledger
add "$(days_ago 0)" alpha "a single considered waiver, and nothing else"
OUT="$(run_hook waivtst1)"; RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "clear again"; then
    ok "15c. a recovery is announced, so silence means no-change and not hope"
else
    bad "15c. a recovery is announced, so silence means no-change and not hope" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 15d — THE SEAL IS A SEAL, NOT AN AMPUTATION.
#
# 15c is green above because CLAUDE_CONFIG_DIR points at an EMPTY teams
# directory. It would be just as green if the analyzer had stopped reading team
# directories altogether — and that is the entire failure mode this suite's
# header is about: a checker that reports nothing passes every test it has.
# Sealing an input and deleting an input look identical from the sealed side.
#
# So the seal is asserted from the other side. The team directories are a real
# production ledger location — a teammate's acks land in the session team dir,
# not in the repository — and the wrapper's fallback to
# "${CLAUDE_CONFIG_DIR:-~/.claude}/teams" is the shipped behavior that finds
# them. This case plants a repeated class in a team directory INSIDE the
# sandbox and requires the hook to count it, which is the same code path that
# was reading the operator's live sessions before the seal, now aimed somewhere
# the suite owns.
#
# Two-sided, like everything else here: the decoy is then removed and the
# recovery line required, so a hook that simply shouted at every turn would
# fail the second half.
# ===========================================================================
DECOY="$CLAUDE_CONFIG_DIR/teams/session-decoy"
mkdir -p "$DECOY"
decoy_add() { # <days-ago> <subject> <reason>
    printf '%sT12:00:00Z\tto=%s\tfixture-ack: %s\n' "$(days_ago "$1")" "$2" "$3" \
        >> "$DECOY/fixture-acks.log"
}
: > "$DECOY/fixture-acks.log"
decoy_add 0 eta   "the recipient is a running teammate holding an isolated worktree of its own, so every write still lands inside that worktree"
decoy_add 1 theta "recipient is a currently-running teammate that holds its own isolated worktree; every write lands inside that worktree as before"
decoy_add 2 iota  "the recipient is a live teammate with an isolated worktree of its own and every write still lands within that worktree"

OUT="$(run_hook waivtst3)"; RC=$?
say "15d" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] \
   && printf '%s' "$OUT" | grep -q "REPEATED WAIVERS" \
   && printf '%s' "$OUT" | grep -q "3x one reason"; then
    ok "15d. a ledger in a session team directory is read, so the sandbox seals rather than blinds"
else
    bad "15d. a ledger in a session team directory is read, so the sandbox seals rather than blinds" "rc=$RC out=$OUT"
fi

# 15e — the negative twin. Take the decoy away and the same session is told the
#       story ended; a hook that shouted unconditionally would pass 15d alone.
rm -rf "$DECOY"
OUT="$(run_hook waivtst3)"; RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "clear again"; then
    ok "15e. removing that ledger returns the session to clear"
else
    bad "15e. removing that ledger returns the session to clear" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 15f — THE MACHINE-WIDE STATE DIRECTORY IS READ.
#
# A hatch ledger lives in one of THREE places, and until 2026-09-14 this
# watcher looked in two. The third is the operator's own state directory,
# outside every repository, and it is where the most-used hatch in the engine
# writes: guard-ci-red-lands.sh appends to
# "${CI_RED_ACK_LOG:-$HOME/.claude/state/ci-red-acks.log}". That location is
# correct and deliberate — one operator lands into several repositories and the
# habit is his, not any single repository's — but it meant 93 acks, 35 of them
# for one workflow, were invisible here. Not flagged, not listed as unused, not
# even named on the unattributed line.
#
# THE FAILURE MODE THIS PINS IS THE QUIET ONE. A directory nobody looks in
# produces a shorter, cleaner report, and a ledger nobody looks for reads
# exactly like a ledger nobody writes — the two answers this file exists to
# keep apart.
#
# Aimed at CLAUDE_CONFIG_DIR rather than $HOME, which is what makes it safe to
# assert: the suite sandboxes that variable at the top, for the reason case 15d
# documents. Reaching for $HOME in the shipped code would have re-opened the
# very hole the seal closed, one directory over.
# ===========================================================================
MACHINE_STATE="$CLAUDE_CONFIG_DIR/state"
mkdir -p "$MACHINE_STATE"
machine_add() { # <days-ago> <subject> <reason>
    printf '%sT12:00:00Z\tto=%s\tfixture-ack: %s\n' "$(days_ago "$1")" "$2" "$3" \
        >> "$MACHINE_STATE/fixture-acks.log"
}
: > "$MACHINE_STATE/fixture-acks.log"
# THE SAME THREE REASONS CASE 15d USES, deliberately. The variable under test
# is the DIRECTORY and nothing else, so the wording is held fixed at a class
# already proven to group — a fresh set of sentences would have made a failure
# here ambiguous between "the directory is not read" and "these three did not
# cluster", and the first draft of this case failed for the second reason.
machine_add 0 kappa  "the recipient is a running teammate holding an isolated worktree of its own, so every write still lands inside that worktree"
machine_add 1 lambda "recipient is a currently-running teammate that holds its own isolated worktree; every write lands inside that worktree as before"
machine_add 2 mu     "the recipient is a live teammate with an isolated worktree of its own and every write still lands within that worktree"

OUT="$(run_hook waivtst3)"; RC=$?
say "15f" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] \
   && printf '%s' "$OUT" | grep -q "REPEATED WAIVERS" \
   && printf '%s' "$OUT" | grep -q "3x one reason"; then
    ok "15f. a ledger in the machine-wide state directory is read, not skipped"
else
    bad "15f. a ledger in the machine-wide state directory is read, not skipped" "rc=$RC out=$OUT"
fi

# 15g — the negative twin. A watcher that had simply started shouting would
#       pass 15f on its own; taking the ledger away has to end the story.
rm -f "$MACHINE_STATE/fixture-acks.log"
OUT="$(run_hook waivtst3)"; RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "clear again"; then
    ok "15g. removing that ledger returns the session to clear"
else
    bad "15g. removing that ledger returns the session to clear" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 16 — THE WRAPPER DECIDES NOTHING, AND SAYS SO. Five Stop wrappers in this
#      engine hand their entire verdict to a sibling .py; a wrapper that went
#      quiet without its analyzer would be wired, hashed, executable and
#      reading nothing.
# ===========================================================================
write_repeated_class
mv "$PY" "$PY.hidden"
OUT="$(run_hook waivtst2)"; RC=$?
mv "$PY.hidden" "$PY"
say "16" "rc=$RC out=$OUT"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "WATCH IS OFF"; then
    ok "16. no analyzer -> the hook announces it is off, never quiet"
else
    bad "16. no analyzer -> the hook announces it is off, never quiet" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 17 — NOT ADOPTED, NOT ANNOUNCED. The plugin loads in every directory on the
#      machine; a notice in each is the noise this engine already refused.
# ===========================================================================
UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"
# The hook runs FROM the unadopted directory too. Entity resolution falls back
# from the payload's cwd to $PWD, and run from the engine (as ci-shard runs
# every unit) an unadopted cwd resolved to the ENGINE, which carries the
# adoption marker: this case then passed because that repository was clear,
# and wrote waivtst3's notice state into the checkout under test, which the
# proof gate binds as the input of every engine check beside it (2026-10-01).
OUT="$(cd "$UNADOPTED" && printf '{"session_id":"waivtst3","cwd":"%s","hook_event_name":"Stop","transcript_path":"/dev/null"}' \
        "$UNADOPTED" | "$HOOK" 2>&1)"; RC=$?
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "17. an unadopted repository gets no notice at all"
else
    bad "17. an unadopted repository gets no notice at all" "rc=$RC out=$OUT"
fi

# ===========================================================================
# 18 — THE LINT IS THE POSITIVE PROBE. Same code, same verdict, and it prints
#      what it looked at — so a silence with nothing behind it is visible.
# ===========================================================================
set +e
OUT="$("$LINT" "$REPO" --today "$TODAY" 2>&1)"; RC=$?
set +e
say "18" "rc=$RC out=$OUT"
if [ "$RC" -eq 1 ] \
   && printf '%s' "$OUT" | grep -q "ESCAPE-HATCH LEDGERS DERIVED FROM THE GUARDS" \
   && printf '%s' "$OUT" | grep -q "SUSPECTED BROKEN GUARD: fixture-hatch-guard.sh"; then
    ok "18. the lint shows its work and agrees with the hook"
else
    bad "18. the lint shows its work and agrees with the hook" "rc=$RC out=$OUT"
fi

# 18b — exit 2 is reserved for "nothing was read", never shared with "clean".
set +e
"$LINT" "$SANDBOX/does-not-exist" >/dev/null 2>&1
RC=$?
set +e
if [ "$RC" -eq 2 ]; then
    ok "18b. an unreadable target exits 2, never 0"
else
    bad "18b. an unreadable target exits 2, never 0" "rc=$RC"
fi

# ===========================================================================
# 19 — REGISTERED. A guard on disk and in no hook table is a guard that never
#      runs, which is the failure this engine has shipped before.
# ===========================================================================
HOOKS_JSON="$ENGINE_ROOT/hooks/hooks.json"
if [ -f "$HOOKS_JSON" ] && grep -q "notice-waiver-repetition.sh" "$HOOKS_JSON"; then
    ok "19. notice-waiver-repetition.sh is registered on the Stop event in hooks/hooks.json"
else
    bad "19. notice-waiver-repetition.sh is registered on the Stop event in hooks/hooks.json" \
        "not found in $HOOKS_JSON"
fi

# ===========================================================================
# 20 — IT HAS NO ESCAPE HATCH. The one property that stops this mechanism
#      going the way of the 228: there is nothing to waive it with.
# ===========================================================================
# 20a — STRUCTURAL. It reads no configuration, so there is no key to set to
#       zero. Every other guard in this engine can be stood down; this one is
#       the exception, deliberately.
if ! grep -qE 'orchestration\.config|CHECK_[A-Z_]+' \
        "$REPO/scripts/hooks/notice-waiver-repetition.sh" \
        "$REPO/scripts/hooks/notice-waiver-repetition.py"; then
    ok "20a. the waiver watcher reads no config — there is no key to stand it down with"
else
    bad "20a. the waiver watcher reads no config — there is no key to stand it down with"
fi

# 20b — BEHAVIORAL, which is the half a grep cannot give. A marker written into
#       the waivers themselves changes nothing: the count is the count.
write_repeated_class
: > "$LEDGER"
add 2026-09-02 alpha "waiver-repetition-ack: known and accepted — recipient is the currently-running teammate spawned this session with native isolation:worktree; background agents never appear in the session roster, so every write still lands in its own existing isolated worktree"
add 2026-09-01 beta  "waiver-repetition-ack: known and accepted — the recipient is a live background teammate spawned minutes ago with native isolation:worktree and absent from the session roster by design; every write still lands in its own existing isolated worktree"
add 2026-08-30 gamma "waiver-repetition-ack: known and accepted — recipient is a currently-running background agent holding native isolation:worktree, absent from the session roster, and every write lands in its own existing isolated worktree as before"
OUT="$(run_py --one-liner)"; RC=$?
say "20b" "rc=$RC out=$OUT"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "3x one reason"; then
    ok "20b. a marker inside the waivers waives nothing — the count is still the count"
else
    bad "20b. a marker inside the waivers waives nothing — the count is still the count" "rc=$RC out=$OUT"
fi


# ===========================================================================
# 21 — THE LINE NAMES WHAT IS STILL HAPPENING, NOT WHAT WAS BIGGEST.
#
# The one-liner names three hatches and counts the rest, so the ORDER decides
# what the operator is told. Ordering by size alone told him about the past:
# measured on 2026-09-14 against ten flagged hatches, the three he was shown
# were 86x idle 12 days, 46x idle 1 day and 42x idle 14 days, while the hatch
# he had personally used 21 times THAT EVENING was 40x idle 1 day and sat
# unnamed inside "+7 more". One of the three he was shown was a day away from
# ageing out of the window altogether.
#
# This is ACTIVE_DAYS' own argument one step further in. That constant exists
# so a guard which HAS BEEN FIXED stops being reported; announcing a fortnight-
# old class ahead of one used yesterday is the same defect at the granularity
# of ordering rather than inclusion.
#
# Both ledgers are written from scratch here so no earlier case can decide this
# one, and every reason is the SAME reason within a ledger and a DIFFERENT one
# across the two, so the two classes cannot merge.
# ===========================================================================
BIG="$STATE/fixture-acks.log"
SMALL="$STATE/fixture-envhop.log"

# Dated against $TODAY, never the real clock: run_py judges them on $TODAY, and
# these two cases are about recency ORDER (see days_before_today above).
big_add() {   # <days-before-TODAY> <subject>
    printf '%sT12:00:00Z\tto=%s\tfixture-ack: the recipient is a running teammate holding an isolated worktree of its own, so every write still lands inside that worktree %s\n' \
        "$(days_before_today "$1")" "$2" "$2" >> "$BIG"
}
small_add() { # <days-before-TODAY> <subject>
    printf '%sT12:00:00Z\tto=%s\tfixture-ack: continuous integration reads red on a workflow no part of this change touches, and its own repair is already under way elsewhere %s\n' \
        "$(days_before_today "$1")" "$2" "$2" >> "$SMALL"
}

# The BIG class is larger and dormant; the SMALL class is smaller and current.
: > "$BIG"
for s in a b c d e; do big_add 10 "$s"; done
big_add 12 f
: > "$SMALL"
small_add 0 g
small_add 1 h
small_add 2 i

OUT="$(run_py --one-liner)"; RC=$?
say "21" "rc=$RC out=$OUT"
LIVE_POS="$(printf '%s' "$OUT" | awk '{print index($0, "fixture-envhop-guard.sh")}')"
DORMANT_POS="$(printf '%s' "$OUT" | awk '{print index($0, "fixture-hatch-guard.sh")}')"
if [ "$RC" -eq 1 ] && [ "$LIVE_POS" -gt 0 ] && [ "$DORMANT_POS" -gt 0 ] \
   && [ "$LIVE_POS" -lt "$DORMANT_POS" ]; then
    ok "21. a live class is named before a larger dormant one"
else
    bad "21. a live class is named before a larger dormant one" "rc=$RC live=$LIVE_POS dormant=$DORMANT_POS out=$OUT"
fi

# 21b — THE TWIN, and it is what stops this becoming "newest wins". Bring the
#       big class into the same bucket and nothing else: size must decide
#       again. Without this, inverting the sort key outright would pass 21.
: > "$BIG"
for s in a b c d e; do big_add 1 "$s"; done
big_add 2 f

OUT="$(run_py --one-liner)"; RC=$?
say "21b" "rc=$RC out=$OUT"
LIVE_POS="$(printf '%s' "$OUT" | awk '{print index($0, "fixture-envhop-guard.sh")}')"
DORMANT_POS="$(printf '%s' "$OUT" | awk '{print index($0, "fixture-hatch-guard.sh")}')"
if [ "$RC" -eq 1 ] && [ "$LIVE_POS" -gt 0 ] && [ "$DORMANT_POS" -gt 0 ] \
   && [ "$DORMANT_POS" -lt "$LIVE_POS" ]; then
    ok "21b. within one bucket the larger class is named first — recency orders, it does not override"
else
    bad "21b. within one bucket the larger class is named first — recency orders, it does not override" "rc=$RC live=$LIVE_POS dormant=$DORMANT_POS out=$OUT"
fi
rm -f "$SMALL"

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# Until 2026-09-05 it ran from NOTHING. run-all-tests.sh discovers *.test.sh;
# contract-integrity.test.sh names a hand-typed set of harnesses that never
# included this one; and the only other mention of the file anywhere in the
# repository was a comment. So every property it proves load-bearing was
# proven exactly when somebody remembered to type its path. Recorded as row
# 3.29 of wiki/open-items.md.
#
# ITS FAILURE IS THIS SUITE'S FAILURE, not a warning. A harness whose result is
# advisory is a harness nobody has to fix.
#
# RICHOS_MUTATION_INNER is the only thing standing between this and an infinite
# regress: the harness EXPORTS it before running any copy of this suite, so the
# inner run skips this block. Never remove one half without the other.
# A MISSING HARNESS IS A FAILURE, NOT A SKIP, and that sentence was earned
# forty minutes ago rather than reasoned. The first draft of this block wrote
# the directory variable with one dollar sign too many, so the path expanded to
# the shell PID followed by the variable name, the test was false, the harness was
# skipped in silence and the suite printed a full green tally in 3.7 seconds.
# That is the exact defect this whole row is about, rebuilt inside the fix for
# it, and only the WALL CLOCK gave it away. So the guard now says so out loud.
if [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
    echo ""
    echo "=== running the mutation harness: waiver-repetition.mutation.sh ==="
    if [ -x "$SRC_DIR/waiver-repetition.mutation.sh" ]; then
        "$SRC_DIR/waiver-repetition.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. waiver-repetition.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
