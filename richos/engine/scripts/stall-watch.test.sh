#!/usr/bin/env bash
#
# stall-watch.test.sh: THE STALL WATCHER (scripts/stall-watch.sh running
# scripts/lib/stall_watch.py), PROVEN ON REAL SLOT HOLDERS, REAL
# WAITERS AND A FIXTURE TEAMMATE REGISTRY. Never the operator's
# ~/.richos-nightly slots, ~/.richos-waits, registry, transcripts or ledger.
#
# The CEO, 2026-09-28: "WHERE THE FUCK WAS THAT FUCKING WATCHER BEFORE???"
# Every case below fails on a checkout without scripts/stall-watch.sh.
#
#   S01  a proof-run slot wait over 10 min by one teammate behind another is
#        told ONCE: the waiter, "proof-run slot", its place in line, the
#        holder's name and pid, since when, the Mac's CPU, and `set 2`
#   S02  the next look says nothing (not repeated)
#   S03  after the repeat interval it is told once more, marked as still going
#   S04  the look after that says nothing again
#   S05  at 95% CPU the action is to reorder, not `set 2`
#   S06  the waiter ends: one miss clears nothing, the second look prints ONE
#        STALL-CLEARED line, and the third look says nothing
#   X01  a teammate waiting on a proof-run slot its OWN other process holds is
#        a SELF-WAIT at 3 minutes (never a plain WAIT), naming both pids
#   X02  not repeated
#   X03  cleared once when it ends
#   X04  two DIFFERENT teammates at 3 minutes: nothing is said at all
#   X05  ownership read from the inherited owner tag (agent_hold.py's
#        RICHOS_AGENT_OWNER), through the registry to the teammate's name,
#        though the two records carry different waiter names
#   X06  a holder that is the waiter's own ancestor is a DEADLOCK
#   W01  a test VM wait and a device wait over 10 min: one block, two stalls,
#        who holds each ("no holder recorded" / "not recorded by the waiter")
#   T01  a live teammate with no commit and no transcript write for 25 min is
#        SILENT: its name, last commit, last transcript write, what to do
#   T02  not repeated
#   T03  it writes its transcript: cleared after two looks, once
#   L01-L03  slow steps: three agent_hold.py wait steps 4.5 min apart are told once (name,
#        median, slowest command, since); 10-min run-tests.sh steps are exempt; clears once
#   T05  a silent teammate whose end the registry deferred because its own endless
#        loop still runs: the notice says so and names the one command that ends
#        its own commands (agent_hold.py end), not a pid to look up
#   T04  a long honest run (transcript written 5 min ago, no commit for an
#        hour), a paused teammate and a finished one: none is SILENT
#   E01  THE 2026-10-01 INCIDENT: a teammate of this session raises a proceeding,
#        for=lead escalation after the session started; the next look prints
#        ONE ESCALATION-WATCH block, FIRST, with its key, sender and question
#   E02  told once (paired with E01's print)   E03  a restarted --monitor is silent
#   E04  one in the SessionStart block, one raised after: only the second
#   E04b a SessionStart backlog over the host cap does not wake; a new one does
#   E05  an acked one is never told; an ack ends it with no cleared line
#   E06  stopped: again at 30 min, not at 29
#   E07  needs=ceo-hands: first under NEEDS THE CEO AT A DEVICE, again at 10 min
#   E08  another live session's teammate: not told; owner unknown: told;
#        E08b that session ends: told here
#   E09  proceeding: no 30-min clock, once more at the 1h bucket
#   E10  END TO END: --monitor tells it once over several looks
#   E11  an ack whose --until passes: told again under its reopened key
#   E12  an unreadable ledger: NOT READ, and nothing re-announced after
#   E13  one the Stop hook already delivered this session is not told
#   E14  the lead's transcript is found by session id (the real locator)
#   M01  END TO END: --monitor (the plugin monitor's command) on the fixture
#        prints the stall once over several looks, --alive says watching,
#        a second --monitor for the same session stands down, nothing it
#        watched was touched, and it ends with its session
#   M02  --alive with nothing watching: NOT WATCHED, exit 1
#   M03  --monitor in a repository that never adopted the engine: silent, 0
#   N01  the SessionStart hook: additionalContext only (no systemMessage),
#        names --monitor and the one-line --alive check, says it only reports
#   N02  the hook is silent where the engine was never adopted
#   N03  monitors.json starts `stall-watch.sh --monitor` always, and hooks.json
#        registers session-start-stall.sh once, under SessionStart
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$SCRIPT_DIR/.." && pwd)"
SW="$SCRIPT_DIR/stall-watch.sh"
HOOK="$SCRIPT_DIR/hooks/session-start-stall.sh"
APP_LIB="$ENGINE/../app/scripts/lib"

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }
command -v git >/dev/null 2>&1 || { echo "FATAL: git required" >&2; exit 1; }

# shellcheck source=lib/scratch.sh
. "$SCRIPT_DIR/lib/scratch.sh"
SB="$(scratch_new stall-watch-test)" || { echo "FATAL: no scratch" >&2; exit 1; }

# Every process this suite signals is one it started, its pid captured at
# spawn and kept in PIDS. Nothing is ever chosen by name.
PIDS=()
spawn_sleeper() { sh -c 'sleep 900 >/dev/null 2>&1 & echo $!'; }
cleanup() {
    local p f
    for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
    # Every slot user writes its own pid at start (slotproc.py): this suite's.
    for f in "$SB"/*.pid; do [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null; done
    scratch_release "$SB" >/dev/null 2>&1 || true
}
trap cleanup EXIT

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }
check() { if [ "$2" -eq 0 ]; then ok "$1"; else bad "$1 -- $3"; fi; }
has()   { printf '%s' "$1" | grep -qF -- "$2"; }
count() { printf '%s' "$1" | grep -cF -- "$2" || true; }

# --- isolation -------------------------------------------------------------
unset CLAUDE_PROJECT_DIR CLAUDE_PLUGIN_ROOT RICHOS_ENGINE_ROOT RICHOS_PROOF_RUN_SLOT_HELD
# This suite may itself run under a teammate's owner tag; its fixture
# processes must not inherit it, or every fixture would share one owner.
unset RICHOS_AGENT_OWNER RICHOS_AGENT_SESSION RICHOS_WAITER
SID="beadfeed-0000-4000-8000-00000000s001"
SLEEPER="$(spawn_sleeper)"; PIDS+=("$SLEEPER")
export RICHOS_ENTITY_ROOT="$SB/entity"
export RICHOS_SESSIONS_DIR="$SB/platform-sessions"
export RICHOS_PROJECTS_DIR="$SB/projects"
export RICHOS_SESSION_PID="$SLEEPER"
export RICHOS_SESSION_ID="$SID"
export RICHOS_WAITS_DIR="$SB/waits"
export RICHOS_PROOF_RUN_SLOTS_DIR="$SB/slots"
export TESTVM_ROOT="$SB/testvm"
export RICHOS_ESCALATION_LEDGER="$SB/escalations.jsonl"
export RICHOS_AGENT_HOLD_DIR="$SB/agent-hold"
export RICHOS_TEST_DEVICES_DIR="$SB/test-devices"
export STALL_WATCH_CPU_BUSY=30
mkdir -p "$RICHOS_ENTITY_ROOT" "$RICHOS_SESSIONS_DIR" "$RICHOS_PROJECTS_DIR" "$RICHOS_WAITS_DIR" \
         "$RICHOS_PROOF_RUN_SLOTS_DIR" "$TESTVM_ROOT" "$RICHOS_AGENT_HOLD_DIR"
printf 'PROTECTED_PATHS=""\n' >"$RICHOS_ENTITY_ROOT/orchestration.config"
printf '1\n' >"$RICHOS_PROOF_RUN_SLOTS_DIR/limit"

new_registry() { # <name>: an empty registry holding this suite's session
    export RICHOS_WORKSPACES_DIR="$SB/ws-$1"
    mkdir -p "$RICHOS_WORKSPACES_DIR/sessions" "$RICHOS_WORKSPACES_DIR/agents"
    python3 - "$ENGINE" "$RICHOS_WORKSPACES_DIR" "$SID" "${RICHOS_SESSION_PID}" <<'PY'
import json, os, sys
sys.path.insert(0, os.path.join(sys.argv[1], "mega-lander"))
import workspaces
st, start = workspaces.process_start(int(sys.argv[4]))
with open(os.path.join(sys.argv[2], "sessions", sys.argv[3] + ".json"), "w") as fh:
    json.dump({"session_id": sys.argv[3], "pid": int(sys.argv[4]), "pid_start": start}, fh)
PY
}
new_state() { export STALL_WATCH_STATE_DIR="$SB/state-$1"; }

add_agent() { # <name> <agent-id> <session> <started-ago-s> <commit-ago-s> <transcript-ago-s> [pause|end]
    python3 - "$ENGINE" "$SB" "$@" <<'PY'
import json, os, subprocess, sys, time
engine, sb, name, aid, sid, started, commit, tr = sys.argv[1:9]
extra = sys.argv[9] if len(sys.argv) > 9 else ""
sys.path.insert(0, os.path.join(engine, "mega-lander"))
import workspaces
# Whole seconds, like tick's `date +%s`: a fractional fixture clock made an age of
# exactly N minutes read as N-1 whenever tick ran inside the same second.
now = float(int(time.time()))
repo = os.path.join(sb, "repos", name)
os.makedirs(repo, exist_ok=True)
# Only the commit's DATE is the fixture's. The identity is the operator's own
# configured one (a machine-wide identity guard refuses any other, rightly),
# with a placeholder only where none is configured at all.
env = dict(os.environ)
env["GIT_AUTHOR_DATE"] = env["GIT_COMMITTER_DATE"] = "@%d +0000" % int(now - float(commit))
ident = [] if subprocess.run(["git", "config", "--get", "user.email"], capture_output=True).stdout.strip() \
    else ["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid"]
subprocess.run(["git", "init", "-q", "-b", "cc/" + name, repo], check=True, env=env)
subprocess.run(["git"] + ident + ["-C", repo, "commit", "-q", "--allow-empty", "-m", "work"], check=True, env=env)
reg = os.environ["RICHOS_WORKSPACES_DIR"]
sess = json.load(open(os.path.join(reg, "sessions", os.environ["RICHOS_SESSION_ID"] + ".json")))
rec = workspaces.new_record("%s--%s" % (sid, name), name=name, session_id=sid, agent_id=aid,
                            session_identity={"pid": sess["pid"], "pid_start": sess["pid_start"]},
                            started_at=workspaces.iso(now - float(started)),
                            workspaces=[{"kind": "cc", "repo": repo, "path": os.path.realpath(repo),
                                         "branch": "cc/" + name, "deleted_at": None}])
if extra == "pause":
    rec["pause"] = {"at": now - 60, "until": "the fixture's resume", "control": True}
if extra == "deferred":
    # Its turn ended at its last transcript write, and the registry deferred its end.
    rec["history"] = [{"at": workspaces.iso(now - float(tr)), "calls": ["toolu_loop1"],
                       "fact": workspaces.END_DEFERRED_FACT}]
if extra == "end":
    rec["end"] = {"at": now - 60, "signal": "SubagentStop", "detail": "fixture"}
    rec["handed_in"] = {"at": now - 61}
with open(os.path.join(reg, "agents", rec["key"] + ".json"), "w") as fh:
    json.dump(rec, fh)
tdir = os.path.join(os.environ["RICHOS_PROJECTS_DIR"], "fixture-project", sid, "subagents")
os.makedirs(tdir, exist_ok=True)
t = os.path.join(tdir, "agent-%s.jsonl" % aid)
open(t, "a").close()
os.utime(t, (now - float(tr), now - float(tr)))
PY
}
touch_transcript() { # <agent-id>
    python3 -c 'import os,sys; p=sys.argv[1]; os.utime(p, None)' \
        "$RICHOS_PROJECTS_DIR/fixture-project/$SID/subagents/agent-$1.jsonl"
}

# A real proof-run slot user, through the real slot library: it takes a slot
# (or waits in the real line, recording its wait) and then sleeps.
cat >"$SB/slotproc.py" <<'PY'
import os, subprocess, sys, time
sys.path.insert(0, sys.argv[1])
import proof_slots
marker, sleep_s, child = sys.argv[2], float(sys.argv[3]), sys.argv[4] if len(sys.argv) > 4 else ""
open(marker + ".pid", "w").write("%d\n" % os.getpid())
slot, waited = proof_slots.acquire({"selection": "fixture"}, wait_seconds=900)
open(marker, "w").write("held\n")
if child:
    # A nested run that does NOT borrow this slot: it waits behind its own parent.
    env = {k: v for k, v in os.environ.items() if k != proof_slots.HELD_ENV}
    subprocess.Popen([sys.executable, __file__, sys.argv[1], child, "600"], env=env)
time.sleep(sleep_s)
PY
SPID=""
start_slot() { # <waiter-name> <marker> [nested-child-marker|-] [owner-tag]; its pid lands in SPID
    # Called in THIS shell, never inside $(...): a pid recorded in a command
    # substitution's subshell never reaches the cleanup.
    local child="${3:-}"; [ "$child" = "-" ] && child=""
    ( cd "$SB" && if [ -n "${4:-}" ]; then export RICHOS_AGENT_OWNER="$4"; fi \
        && exec env RICHOS_WAITER="$1" python3 "$SB/slotproc.py" "$APP_LIB" "$2" 600 ${child:+"$child"} \
        >/dev/null 2>&1 ) &
    SPID=$!
    PIDS+=("$SPID")
    disown "$SPID" 2>/dev/null   # its end is this suite's doing, not news for the log
}
wait_for() { # <seconds> <test command...>
    local n="$1"; shift
    local i=0
    while [ "$i" -lt $((n * 10)) ]; do "$@" && return 0; sleep 0.1; i=$((i + 1)); done
    return 1
}
has_waiter() { ls "$RICHOS_PROOF_RUN_SLOTS_DIR"/wait-*.json >/dev/null 2>&1 && ls "$RICHOS_WAITS_DIR"/*.json >/dev/null 2>&1; }
no_waiter() { ! ls "$RICHOS_PROOF_RUN_SLOTS_DIR"/wait-*.json >/dev/null 2>&1; }
stop_slots() { # every slot user this suite started: each wrote its own pid at start
    local f
    for f in "$SB"/*.pid; do
        [ -f "$f" ] || continue
        kill "$(cat "$f")" 2>/dev/null
        rm -f "$f"
    done
    wait_for 10 no_waiter
    sleep 0.3
    rm -f "$RICHOS_WAITS_DIR"/*.json
}

tick() { # <now-offset-seconds>
    OUT="$(STALL_WATCH_NOW=$(( $(date +%s) + $1 )) bash "$SW" --tick 2>&1)"; RC=$?
}

echo "=== stall-watch tests ==="
[ -f "$SW" ] || echo "  (scripts/stall-watch.sh is missing: every case below fails)"
[ -d "$APP_LIB" ] || echo "  (the app tree is not beside this engine: the S and X cases fail)"

# --- S: a proof-run slot wait over 10 minutes --------------------------------
new_registry s; new_state s
start_slot alpha-opus-h1 "$SB/s-held"; H="$SPID"
wait_for 10 test -f "$SB/s-held"
start_slot bravo-opus-w1 "$SB/s-w-held"; W="$SPID"
wait_for 10 has_waiter
tick 660
check "S01  a slot wait over 10 min is told once: waiter, slot, place, holder and pid, CPU, set 2" \
    "$( [ "$(count "$OUT" "[WAIT]")" -eq 1 ] && has "$OUT" "[WAIT] bravo-opus-w1: waits for a proof-run slot" \
        && has "$OUT" "#1 in line, this Mac runs 1 at once" && has "$OUT" "alpha-opus-h1 (pid $H" \
        && has "$OUT" "30% busy" && has "$OUT" "proof_slots.py set 2" && has "$OUT" "(since "; echo $?)" "out=$OUT"
tick 720
check "S02  the next look says nothing" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"
tick 2530
check "S03  after the repeat interval: told once more, as still going" \
    "$( [ "$(count "$OUT" "[WAIT]")" -eq 1 ] && has "$OUT" "still, notice 2"; echo $?)" "out=$OUT"
tick 2590
check "S04  and then nothing again" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"
OUT="$(STALL_WATCH_CPU_BUSY=95 STALL_WATCH_NOW=$(( $(date +%s) + 660 )) bash "$SW" --once 2>&1)"; RC=$?
check "S05  at 95% CPU the action is to reorder, not another slot (and --once exits 1)" \
    "$( [ "$RC" -eq 1 ] && has "$OUT" "another slot would not run faster" && has "$OUT" "reorder" \
        && ! has "$OUT" "set 2"; echo $?)" "rc=$RC out=$OUT"
kill "$W" 2>/dev/null; wait_for 10 no_waiter
tick 2650; first="$OUT"
tick 2710; second="$OUT"
tick 2770; third="$OUT"
check "S06  the wait ends: cleared on the second miss, in one line, then silence" \
    "$( [ -z "$first" ] && [ "$(count "$second" "STALL-CLEARED")" -eq 1 ] && has "$second" "bravo-opus-w1" \
        && [ -z "$third" ]; echo $?)" "first=$first second=$second third=$third"
stop_slots

# --- X: waiting on your own slot --------------------------------------------
new_registry x; new_state x
start_slot charlie-opus-s1 "$SB/x-held"; H="$SPID"
wait_for 10 test -f "$SB/x-held"
start_slot charlie-opus-s1 "$SB/x-w-held"; W="$SPID"
wait_for 10 has_waiter
tick 180
check "X01  a self-wait is told at 3 min as SELF-WAIT, naming both pids" \
    "$( [ "$(count "$OUT" "[SELF-WAIT]")" -eq 1 ] && ! has "$OUT" "[WAIT]" && has "$OUT" "charlie-opus-s1" \
        && has "$OUT" "waiting: pid $W" && has "$OUT" "held by: pid $H" && has "$OUT" "ALSO charlie-opus-s1's" \
        && ! has "$OUT" "DEADLOCK"; echo $?)" "out=$OUT"
tick 240
check "X02  not repeated" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"
kill "$W" 2>/dev/null; wait_for 10 no_waiter
tick 300; tick 360
check "X03  cleared once when it ends" "$( [ "$(count "$OUT" "STALL-CLEARED")" -eq 1 ]; echo $?)" "out=$OUT"
stop_slots

new_state x4
start_slot delta-opus-h2 "$SB/x4-held"; H="$SPID"
wait_for 10 test -f "$SB/x4-held"
start_slot echo-opus-w2 "$SB/x4-w-held"; W="$SPID"
wait_for 10 has_waiter
tick 180
check "X04  two different teammates at 3 min: nothing is said" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"
stop_slots

new_registry x5; new_state x5
add_agent hotel-opus-t1 a0000000000fix5 "other-session" 60 30 10
start_slot foxtrot-a "$SB/x5-held" - a0000000000fix5; H="$SPID"
wait_for 10 test -f "$SB/x5-held"
start_slot golf-b "$SB/x5-w-held" - a0000000000fix5; W="$SPID"
wait_for 10 has_waiter
tick 180
check "X05  the owner tag names the teammate through the registry, whatever the waiter names say" \
    "$( has "$OUT" "[SELF-WAIT] hotel-opus-t1" && has "$OUT" "known from its owner tag"; echo $?)" "out=$OUT"
stop_slots

new_registry x6; new_state x6
start_slot india-opus-n1 "$SB/x6-held" "$SB/x6-child-held"; H="$SPID"
wait_for 10 test -f "$SB/x6-held"
wait_for 10 has_waiter
tick 180
check "X06  a holder that is the waiter's own ancestor is a DEADLOCK" \
    "$( has "$OUT" "[SELF-WAIT] india-opus-n1" && has "$OUT" "DEADLOCK" && has "$OUT" "held by: pid $H"; echo $?)" "out=$OUT"
# The nested child is this suite's too: it wrote its own pid at start.
[ -f "$SB/x6-child-held.pid" ] && kill "$(cat "$SB/x6-child-held.pid")" 2>/dev/null
stop_slots

# --- W: other recorded waits ---------------------------------------------------
new_registry w; new_state w
VMW="$(spawn_sleeper)"; PIDS+=("$VMW")
DEVW="$(spawn_sleeper)"; PIDS+=("$DEVW")
sleep 1.1
python3 - "$RICHOS_WAITS_DIR" "$VMW" "$DEVW" <<'PY'
import json, os, sys, time
d, vm, dev = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
for pid, res, who in ((vm, "testvm-slot", "juliet-opus-v1"), (dev, "android-device", "kilo-opus-d1")):
    with open(os.path.join(d, "%d-fixture.json" % pid), "w") as fh:
        json.dump({"schema": "richos-wait/1", "pid": pid, "since": time.time(), "resource": res,
                   "waiter": who, "reason": "fixture", "holding": [], "cwd": d, "command": "fixture"}, fh)
PY
tick 660
check "W01  a VM wait and a device wait: one block, two stalls, who holds each" \
    "$( [ "$(count "$OUT" "STALL-WATCH")" -eq 1 ] && has "$OUT" "2 stalls" \
        && has "$OUT" "[WAIT] juliet-opus-v1: waits for the test VM" && has "$OUT" "NO HOLDER IS RECORDED" \
        && has "$OUT" "[WAIT] kilo-opus-d1: waits for android-device" && has "$OUT" "not recorded by the waiter"; \
        echo $?)" "out=$OUT"
rm -f "$RICHOS_WAITS_DIR"/*.json

# --- T: silent teammates ------------------------------------------------------
new_registry t; new_state t
add_agent lima-opus-q1 a0000000000fixq "$SID" 3600 3000 1500
add_agent mike-opus-r1 a0000000000fixr "$SID" 3600 3500 300
add_agent november-opus-p1 a0000000000fixp "$SID" 3600 3500 3000 pause
add_agent oscar-opus-e1 a0000000000fixe "$SID" 3600 3500 3000 end
tick 0
check "T01  no commit and no transcript write for 25 min: SILENT, with last commit, transcript, action" \
    "$( [ "$(count "$OUT" "[SILENT]")" -eq 1 ] && has "$OUT" "[SILENT] lima-opus-q1: no commit and no transcript activity, for 25 min" \
        && has "$OUT" "on cc/lima-opus-q1" && has "$OUT" "last transcript write" && has "$OUT" "SendMessage to lima-opus-q1"; \
        echo $?)" "out=$OUT"
T01OUT="$OUT"
tick 60
check "T02  not repeated" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"
touch_transcript a0000000000fixq
tick 120; tick 180
check "T03  it writes its transcript: cleared once, after two looks" \
    "$( [ "$(count "$OUT" "STALL-CLEARED")" -eq 1 ] && has "$OUT" "lima-opus-q1"; echo $?)" "out=$OUT"
check "T04  a long honest run, a paused teammate and a finished one are not SILENT" \
    "$( has "$T01OUT" "[SILENT] lima-opus-q1" && ! has "$T01OUT" "mike-opus-r1" && ! has "$T01OUT" "november-opus-p1" \
        && ! has "$T01OUT" "oscar-opus-e1"; echo $?)" \
    "out=$T01OUT"

# --- T05: an end deferred by its own endless loop (2026-10-11, echo-opus-walk49fix1) ----
new_registry t5; new_state t5
add_agent papa-opus-l1 a0000000000fixl "$SID" 3600 3000 1500 deferred
LOOP='until grep -q Finished /nonexistent-deleted-scratch/build.log 2>/dev/null; do sleep 1; done'
STEM="$(python3 - "$ENGINE" a0000000000fixl "$SID" "$LOOP" <<'PY'
import os, sys
sys.path.insert(0, os.path.join(sys.argv[1], "scripts", "lib"))
import agent_hold
stem, _held = agent_hold._record({"session_id": sys.argv[3], "agent_id": sys.argv[2],
                                  "tool_use_id": "toolu_loop1", "tool_name": "Bash"}, "native", sys.argv[4])
print(stem)
PY
)"
# The agent's call, as its shell runs it: the ownership mark, then its own loop.
printf '%s\n' "python3 '$ENGINE/scripts/lib/agent_hold.py' mark --state '$RICHOS_AGENT_HOLD_DIR' '$STEM'" \
    "$LOOP" >"$SB/t5-loop.sh"
bash "$SB/t5-loop.sh" >/dev/null 2>&1 &
LOOPPID=$!; PIDS+=("$LOOPPID")
wait_for 10 test -f "$STEM.pid"
tick 0
check "T05  a silent agent whose end waits on its own loop: said so, with the one command that ends its commands" \
    "$( has "$OUT" "[SILENT] papa-opus-l1: its turn ended; its end waits on its own still-running command(s)" \
        && has "$OUT" "its end waits on its own command(s) still running" && has "$OUT" "running: \`until grep -q Finished" \
        && has "$OUT" "agent_hold.py end --session $SID --agent a0000000000fixl --name papa-opus-l1" \
        && ! has "$OUT" "SendMessage to papa-opus-l1"; echo $?)" "out=$OUT"
kill "$LOOPPID" 2>/dev/null

# --- L: slow steps (2026-10-04, isaac-opus-logo1: every step cost ~4.5 minutes) ----
write_steps() { # <agent-id> <command> <result-text> <step-seconds> <count>: steps ending now-60, spaced 5 min
    python3 - "$RICHOS_PROJECTS_DIR/fixture-project/$SID/subagents/agent-$1.jsonl" "$2" "$3" "$4" "$5" <<'PY'
import datetime, json, sys, time
path, cmd, text, step, n = sys.argv[1], sys.argv[2], sys.argv[3], float(sys.argv[4]), int(sys.argv[5])
def iso(t):
    return datetime.datetime.fromtimestamp(t, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")
end = time.time() - 60
rows = []
for i in range(n):
    t1 = end - (n - 1 - i) * (step + 30)
    t0 = t1 - step
    uid = "toolu_%d" % i
    rows.append({"type": "assistant", "timestamp": iso(t0), "message": {"content": [
        {"type": "tool_use", "id": uid, "name": "Bash", "input": {"command": cmd}}]}})
    rows.append({"type": "user", "timestamp": iso(t1), "message": {"content": [
        {"type": "tool_result", "tool_use_id": uid, "content": text}]}})
with open(path, "w") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")
PY
}
new_registry l; new_state l
add_agent papa-opus-w1 a0000000000fixw "$SID" 3600 100 50
add_agent quebec-opus-t1 a0000000000fixt "$SID" 3600 100 50
write_steps a0000000000fixw "python3 ~/.claude/richos-engine/scripts/lib/agent_hold.py wait" \
    "STILL RUNNING at 12:00:00Z: repeat this wait to collect the native task's completion." 270 3
write_steps a0000000000fixt "scripts/run-tests.sh --suite stall-watch" "ok" 600 1
tick 0
check "L01  three agent_hold.py wait steps 4.5 min apart: SLOW-STEPS, named, median, slowest command, since; a 10-min run-tests.sh step is not" \
    "$( [ "$(count "$OUT" "[SLOW-STEPS]")" -eq 1 ] && has "$OUT" "[SLOW-STEPS] papa-opus-w1" && has "$OUT" "median step time 4.5 min" \
        && has "$OUT" "agent_hold.py wait" && has "$OUT" "(since " && ! has "$OUT" "quebec-opus-t1"; echo $?)" "out=$OUT"
tick 60
check "L02  not repeated at the next look" "$( [ -z "$OUT" ]; echo $?)" "out=$OUT"
write_steps a0000000000fixt "scripts/run-tests.sh --suite stall-watch" "ok" 600 3
write_steps a0000000000fixw "git status" "ok" 5 3
tick 120; tick 180
check "L03  three 10-min run-tests.sh steps are exempt; the fast teammate clears once" \
    "$( [ "$(count "$OUT" "STALL-CLEARED")" -eq 1 ] && has "$OUT" "papa-opus-w1" && ! has "$OUT" "[SLOW-STEPS]"; echo $?)" "out=$OUT"

# --- E: a teammate's escalation wakes an idle lead (2026-10-01) ---------------
# richos-hq docs/operations/2026-10-01-escalation-wakes-the-lead.md, built past
# Sage's review (richos-hq 8ba32b71). Every case below that asserts silence is
# paired, in the same case, with something that must be printed, so none of
# them passes on a checkout where the watcher prints no escalation at all.
ESC_TOOL="$ENGINE/scripts/lib/escalations.py"
esc_ledger() { export RICHOS_ESCALATION_LEDGER="$SB/esc-$1.jsonl"; : >"$RICHOS_ESCALATION_LEDGER"; }
esc_lead() { export STALL_WATCH_LEAD_TRANSCRIPT="$SB/lead-$1.jsonl"; : >"$STALL_WATCH_LEAD_TRANSCRIPT"; }
esc_raise() { # <id> <teammate> <state> <needs|-> <raised-ago-s> [question]
    python3 - "$RICHOS_ESCALATION_LEDGER" "$SB/repos" "$@" <<'PY'
import json, os, sys, time
ledger, repos, rid, who, state, needs, ago = sys.argv[1:8]
q = sys.argv[8] if len(sys.argv) > 8 else "a fixture question that is long enough to be one"
t = time.gmtime(int(time.time()) - int(ago))
row = {"event": "Escalation", "id": rid, "raised": time.strftime("%Y-%m-%dT%H:%M:%SZ", t),
       "teammate": who, "worktree": os.path.realpath(os.path.join(repos, who)), "branch": "cc/" + who,
       "repo": "fixture", "head": "", "state": state, "for": "lead", "title": "fixture title for " + rid,
       "question": q, "tried": "", "meanwhile": "", "record": "", "session_id": "", "actor": "fixture"}
if needs != "-":
    row["needs"] = needs
with open(ledger, "a") as fh:
    fh.write(json.dumps(row) + "\n")
PY
}
esc_ack() { # <id> [until-from-now-s]
    python3 - "$RICHOS_ESCALATION_LEDGER" "$@" <<'PY'
import json, sys, time
ledger, rid = sys.argv[1:3]
row = {"event": "EscalationAck", "id": rid, "acked": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
       "disposition": "fixture: decided and recorded, nothing left to do", "actor": "fixture", "session_id": ""}
if len(sys.argv) > 3:
    until = int(time.time()) + int(sys.argv[3])
    row["until"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(until))
    row["until_epoch"] = float(until)
with open(ledger, "a") as fh:
    fh.write(json.dumps(row) + "\n")
print(row.get("until", ""))
PY
}
esc_lead_row() { # <kind: sessionstart|stop> <text>: a HOST row (459554d9 lines 4 and 718), free text replaced
    python3 - "$STALL_WATCH_LEAD_TRANSCRIPT" "$@" <<'PY'
import json, sys
path, kind, text = sys.argv[1:4]
if kind == "sessionstart":
    row = {"parentUuid": None, "isSidechain": False,
           "attachment": {"type": "hook_success", "hookName": "SessionStart:compact", "toolUseID": "fixture",
                          "hookEvent": "SessionStart", "content": "",
                          "stdout": json.dumps({"hookSpecificOutput": {"hookEventName": "SessionStart",
                                                                        "additionalContext": text}}) + "\n",
                          "stderr": "", "exitCode": 0,
                          "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/session-start-escalations.sh",
                          "durationMs": 170},
           "type": "attachment", "uuid": "fixture-ss", "timestamp": "2026-10-01T11:29:34.356Z",
           "userType": "external", "entrypoint": "cli", "cwd": "/fixture", "sessionId": "fixture",
           "version": "2.1.286", "gitBranch": "main"}
else:
    row = {"parentUuid": "fixture", "isSidechain": False,
           "attachment": {"type": "hook_additional_context", "content": [text], "hookName": "Stop",
                          "toolUseID": "fixture", "hookEvent": "Stop"},
           "type": "attachment", "uuid": "fixture-stop", "timestamp": "2026-10-01T13:01:51.415Z",
           "userType": "external", "entrypoint": "cli", "cwd": "/fixture", "sessionId": "fixture",
           "version": "2.1.286", "gitBranch": "main"}
with open(path, "a") as fh:
    fh.write(json.dumps(row) + "\n")
PY
}
escs() { printf '%s\n' "$OUT" | sed -n '/^ESCALATION-WATCH/,/^STALL-WATCH/p' | grep -v '^STALL-WATCH'; }
nesc() { printf '%s' "$OUT" | grep -c '^ESCALATION-WATCH' || true; }

new_registry e
export STALL_WATCH_SESSION_START=$(( $(date +%s) - 120 ))
add_agent isaac-opus-d3cfix a0000000000fe01 "$SID" 3600 3000 10
add_agent zulu-opus-e2 a0000000000fe02 "$SID" 3600 3000 10

E01_ID="esc-20261001T113833Z-c9ac3ed0"
new_state e1; esc_ledger e1; esc_lead e1
esc_raise "$E01_ID" isaac-opus-d3cfix proceeding - 60 \
    "Can the CEO be at the test iPhone SE for the UI automation approval (Touch ID or passcode, within about 60 s of the run starting)?"
tick 0; E01OUT="$OUT"
check "E01  the incident: a teammate's proceeding, for=lead escalation is told at the next look, first, with its question" \
    "$( [ "$(nesc)" -eq 1 ] && [ "$(printf '%s\n' "$OUT" | head -1 | cut -c1-16)" = "ESCALATION-WATCH" ] \
        && escs | grep -qF "[$E01_ID]" && escs | grep -qF "Q: Can the CEO be at the test iPhone SE" \
        && escs | grep -qF "from isaac-opus-d3cfix, state=proceeding, for=lead"; echo $?)" "out=$OUT"
tick 60
check "E02  told once: the next look says nothing about it" \
    "$( printf '%s' "$E01OUT" | grep -qF "[$E01_ID]" && [ "$(nesc)" -eq 0 ]; echo $?)" "first=$E01OUT now=$OUT"
( STALL_WATCH_POLL_SECONDS=1 exec bash "$SW" --monitor >"$SB/e3.out" 2>&1 ) &
E3MON=$!; PIDS+=("$E3MON")
wait_for 10 test -s "$SB/state-e1/$SID/monitor.json"
sleep 2.5
kill "$E3MON" 2>/dev/null
check "E03  a restarted --monitor on the same state does not tell it again" \
    "$( printf '%s' "$E01OUT" | grep -qF "[$E01_ID]" && ! grep -q '^ESCALATION-WATCH' "$SB/e3.out"; echo $?)" \
    "first=$E01OUT restarted=$(cat "$SB/e3.out")"

new_state e4; esc_ledger e4; esc_lead e4
esc_raise esc-20261001T120000Z-0000e4a0 zulu-opus-e2 proceeding - 50
esc_raise esc-20261001T120001Z-0000e4b0 zulu-opus-e2 proceeding - 40
esc_lead_row sessionstart "1 ESCALATION(S) OUTSTANDING
  [esc-20261001T120000Z-0000e4a0] fixture title (1m old), from zulu-opus-e2, state=proceeding"
tick 0
check "E04  one already in the SessionStart block, one raised after it: exactly the second is told" \
    "$( escs | grep -qF "[esc-20261001T120001Z-0000e4b0]" && ! escs | grep -qF "[esc-20261001T120000Z-0000e4a0]"; echo $?)" \
    "out=$OUT"

new_state e4b; esc_ledger e4b; esc_lead e4b
BIG=""
for i in $(seq 10 39); do
    esc_raise "esc-20261001T1000${i}Z-00000b$i" zulu-opus-e2 proceeding - 3600 "$(printf 'Q%.0s' $(seq 1 300))"
    BIG="$BIG  [esc-20261001T1000${i}Z-00000b$i] $(printf 'x%.0s' $(seq 1 330))
"
done
esc_raise esc-20261001T120002Z-00004bc0 zulu-opus-e2 proceeding - 30
esc_lead_row sessionstart "30 ESCALATION(S) OUTSTANDING
$BIG"
tick 0
check "E04b a SessionStart backlog over the host's 10,000-character cap does not wake; one raised after the start does" \
    "$( [ "$(escs | grep -c '^  \[esc-' || true)" -eq 1 ] && escs | grep -qF "[esc-20261001T120002Z-00004bc0]"; echo $?)" \
    "out=$OUT"

new_state e5; esc_ledger e5; esc_lead e5
esc_raise esc-20261001T120003Z-0000e5a0 zulu-opus-e2 proceeding - 50
esc_raise esc-20261001T120004Z-0000e5b0 zulu-opus-e2 proceeding - 40
esc_ack esc-20261001T120003Z-0000e5a0 >/dev/null
tick 0; E05A="$OUT"
esc_ack esc-20261001T120004Z-0000e5b0 >/dev/null
tick 60; tick 120
check "E05  acknowledged is never told; the unacknowledged one is; its ack ends it with no cleared line" \
    "$( printf '%s' "$E05A" | grep -qF "[esc-20261001T120004Z-0000e5b0]" && ! printf '%s' "$E05A" | grep -qF "0000e5a0" \
        && [ -z "$OUT" ]; echo $?)" "first=$E05A later=$OUT"

new_state e6; esc_ledger e6; esc_lead e6
esc_raise esc-20261001T120005Z-0000e6a0 zulu-opus-e2 stopped - 30
tick 0; E06A="$OUT"; tick 1740; E06B="$OUT"; tick 1800; E06C="$OUT"
check "E06  stopped: told, not again at 29 min, again at 30 min" \
    "$( printf '%s' "$E06A" | grep -qF "[esc-20261001T120005Z-0000e6a0]" && ! printf '%s' "$E06B" | grep -q '^ESCALATION-WATCH' \
        && printf '%s' "$E06C" | grep -qF "[esc-20261001T120005Z-0000e6a0]" && printf '%s' "$E06C" | grep -qF "told again, notice 2"; echo $?)" \
    "0=$E06A 29=$E06B 30=$E06C"

new_state e7; esc_ledger e7; esc_lead e7
esc_raise esc-20261001T120006Z-0000e7b0 zulu-opus-e2 proceeding - 40
esc_raise esc-20261001T120007Z-0000e7a0 isaac-opus-d3cfix proceeding ceo-hands 30
tick 0; E07A="$OUT"; tick 540; E07B="$OUT"; tick 600; E07C="$OUT"
HANDS_LINE="$(printf '%s\n' "$E07A" | grep -n 'NEEDS THE CEO AT A DEVICE' | head -1 | cut -d: -f1)"
A_LINE="$(printf '%s\n' "$E07A" | grep -nF '[esc-20261001T120007Z-0000e7a0]' | head -1 | cut -d: -f1)"
B_LINE="$(printf '%s\n' "$E07A" | grep -nF '[esc-20261001T120006Z-0000e7b0]' | head -1 | cut -d: -f1)"
check "E07  needs=ceo-hands comes first under NEEDS THE CEO AT A DEVICE; told again at 10 min, not at 9; the other is not" \
    "$( [ -n "$HANDS_LINE" ] && [ -n "$A_LINE" ] && [ -n "$B_LINE" ] && [ "$HANDS_LINE" -lt "$A_LINE" ] && [ "$A_LINE" -lt "$B_LINE" ] \
        && ! printf '%s' "$E07B" | grep -q '^ESCALATION-WATCH' && printf '%s' "$E07C" | grep -qF '0000e7a0' \
        && ! printf '%s' "$E07C" | grep -qF '0000e7b0'; echo $?)" "0=$E07A 9=$E07B 10=$E07C"

# Another live Rich session: its own sleeper, its own session record, its own teammate.
SID2="beadfeed-0000-4000-8000-00000000s002"
SLEEPER2="$(spawn_sleeper)"; PIDS+=("$SLEEPER2")
python3 - "$ENGINE" "$RICHOS_WORKSPACES_DIR" "$SID2" "$SLEEPER2" <<'PY'
import json, os, sys
sys.path.insert(0, os.path.join(sys.argv[1], "mega-lander"))
import workspaces
st, start = workspaces.process_start(int(sys.argv[4]))
with open(os.path.join(sys.argv[2], "sessions", sys.argv[3] + ".json"), "w") as fh:
    json.dump({"session_id": sys.argv[3], "pid": int(sys.argv[4]), "pid_start": start}, fh)
PY
add_agent yankee-opus-o1 a0000000000fe08 "$SID2" 3600 3000 10
new_state e8; esc_ledger e8; esc_lead e8
esc_raise esc-20261001T120008Z-0000e8a0 yankee-opus-o1 proceeding - 40
esc_raise esc-20261001T120009Z-0000e8b0 xray-opus-unregistered proceeding - 30
tick 0
check "E08  owned by another live session: not told here; owner unknown: told, in the same look" \
    "$( escs | grep -qF '[esc-20261001T120009Z-0000e8b0]' && ! escs | grep -qF '0000e8a0'; echo $?)" "out=$OUT"
kill "$SLEEPER2" 2>/dev/null; sleep 0.3
new_state e8b
tick 0
check "E08b the other session ends: its teammate's escalation is told here (an owner that is gone is no owner)" \
    "$( escs | grep -qF '[esc-20261001T120008Z-0000e8a0]'; echo $?)" "out=$OUT"

new_state e9; esc_ledger e9; esc_lead e9
esc_raise esc-20261001T120010Z-0000e9a0 zulu-opus-e2 proceeding - 60
tick 0; E09A="$OUT"; tick 1800; E09B="$OUT"; tick 3600; E09C="$OUT"; tick 3660; E09D="$OUT"
check "E09  proceeding: told, not on a 30-min clock, once more when it crosses 1h, then quiet" \
    "$( printf '%s' "$E09A" | grep -qF '0000e9a0' && ! printf '%s' "$E09B" | grep -q '^ESCALATION-WATCH' \
        && printf '%s' "$E09C" | grep -qF '0000e9a0' && ! printf '%s' "$E09D" | grep -q '^ESCALATION-WATCH'; echo $?)" \
    "0=$E09A 30=$E09B 60=$E09C 61=$E09D"

new_state e11; esc_ledger e11; esc_lead e11
esc_raise esc-20261001T120011Z-000e11a0 zulu-opus-e2 proceeding - 60
tick 0; E11A="$OUT"
E11_UNTIL="$(esc_ack esc-20261001T120011Z-000e11a0 300)"
tick 60; E11B="$OUT"; tick 400; E11C="$OUT"
check "E11  an ack with --until that expires: quiet while it holds, told again under its reopened key after" \
    "$( printf '%s' "$E11A" | grep -qF '[esc-20261001T120011Z-000e11a0]' && ! printf '%s' "$E11B" | grep -q '^ESCALATION-WATCH' \
        && printf '%s' "$E11C" | grep -qF "[esc-20261001T120011Z-000e11a0@reopened:$E11_UNTIL]"; echo $?)" \
    "0=$E11A held=$E11B expired=$E11C"

new_state e12; esc_ledger e12; esc_lead e12
esc_raise esc-20261001T120012Z-000e12a0 zulu-opus-e2 proceeding - 60
tick 0; E12A="$OUT"
chmod 000 "$RICHOS_ESCALATION_LEDGER"
tick 60; E12B="$OUT"; tick 120; E12C="$OUT"
chmod 644 "$RICHOS_ESCALATION_LEDGER"
tick 180; E12D="$OUT"
check "E12  a ledger that cannot be read: NOT READ once, and the next good read re-announces nothing" \
    "$( printf '%s' "$E12A" | grep -qF '000e12a0' && printf '%s' "$E12B" | grep -q 'NOT READ' \
        && ! printf '%s%s' "$E12B" "$E12C" | grep -q '^ESCALATION-WATCH' && ! printf '%s' "$E12D" | grep -q '^ESCALATION-WATCH'; echo $?)" \
    "0=$E12A 1=$E12B 2=$E12C back=$E12D"

new_state e13; esc_ledger e13; esc_lead e13
esc_raise esc-20261001T120013Z-000e13a0 zulu-opus-e2 proceeding - 50
esc_raise esc-20261001T120014Z-000e13b0 zulu-opus-e2 proceeding - 40
esc_lead_row stop "TEAMMATE ESCALATION — 1 NEW since you were last told, raised by a teammate and not acknowledged.
  [esc-20261001T120013Z-000e13a0] fixture title (1m old), from zulu-opus-e2, state=proceeding"
tick 0
check "E13  one the Stop hook already delivered this session is not told; the other is" \
    "$( escs | grep -qF '[esc-20261001T120014Z-000e13b0]' && ! escs | grep -qF '000e13a0'; echo $?)" "out=$OUT"

new_state e10; esc_ledger e10; esc_lead e10
esc_raise esc-20261001T120015Z-000e10a0 isaac-opus-d3cfix proceeding - 30
( STALL_WATCH_POLL_SECONDS=1 exec bash "$SW" --monitor >"$SB/e10.out" 2>"$SB/e10.err" ) &
E10MON=$!; PIDS+=("$E10MON")
wait_for 15 grep -q '^ESCALATION-WATCH' "$SB/e10.out"
sleep 3.5
kill "$E10MON" 2>/dev/null
check "E10  END TO END: --monitor tells it once over several looks" \
    "$( [ "$(grep -c '^ESCALATION-WATCH' "$SB/e10.out" || true)" -eq 1 ] && grep -qF '[esc-20261001T120015Z-000e10a0]' "$SB/e10.out"; echo $?)" \
    "out=$(cat "$SB/e10.out") err=$(cat "$SB/e10.err")"
new_state e14; esc_ledger e14
# The real locator, not the override: <projects>/*/<session id>.jsonl.
export STALL_WATCH_LEAD_TRANSCRIPT="$RICHOS_PROJECTS_DIR/fixture-project/$SID.jsonl"
: >"$STALL_WATCH_LEAD_TRANSCRIPT"
esc_lead_row stop "TEAMMATE ESCALATION — 1 NEW since you were last told, raised by a teammate and not acknowledged.
  [esc-20261001T120016Z-000e14a0] fixture title (1m old), from zulu-opus-e2, state=proceeding"
unset STALL_WATCH_LEAD_TRANSCRIPT
esc_raise esc-20261001T120016Z-000e14a0 zulu-opus-e2 proceeding - 50
esc_raise esc-20261001T120017Z-000e14b0 zulu-opus-e2 proceeding - 40
tick 0
check "E14  the lead's transcript is found by session id under the projects directory (no override)" \
    "$( escs | grep -qF '[esc-20261001T120017Z-000e14b0]' && ! escs | grep -qF '000e14a0'; echo $?)" "out=$OUT"
unset STALL_WATCH_LEAD_TRANSCRIPT STALL_WATCH_SESSION_START
export RICHOS_ESCALATION_LEDGER="$SB/escalations.jsonl"

# --- M: the monitor end to end --------------------------------------------------
SESS2="$(spawn_sleeper)"; PIDS+=("$SESS2")
export RICHOS_SESSION_PID="$SESS2"
new_registry m; new_state m
add_agent papa-opus-m1 a0000000000fixm "$SID" 3600 3000 1500
( STALL_WATCH_POLL_SECONDS=1 exec bash "$SW" --monitor >"$SB/monitor.out" 2>"$SB/monitor.err" ) &
MON=$!; PIDS+=("$MON")
wait_for 15 grep -q "SILENT" "$SB/monitor.out"
sleep 4
MOUT="$(cat "$SB/monitor.out")"
ALIVE="$(bash "$SW" --alive 2>&1)"; ARC=$?
( STALL_WATCH_POLL_SECONDS=1 exec bash "$SW" --monitor >"$SB/monitor2.out" 2>&1 ) &
MON2=$!; PIDS+=("$MON2")
wait_for 10 sh -c "! kill -0 $MON2 2>/dev/null"
SECOND_GONE=$?
kill -0 "$SLEEPER" 2>/dev/null; UNTOUCHED=$?
kill "$SESS2" 2>/dev/null
wait_for 15 sh -c "! kill -0 $MON 2>/dev/null"; ENDED=$?
check "M01  --monitor tells the stall once over several looks, --alive says watching, one per session, touches nothing, ends with its session" \
    "$( [ "$(count "$MOUT" "[SILENT] papa-opus-m1")" -eq 1 ] && [ "$ARC" -eq 0 ] && has "$ALIVE" "watching" \
        && [ "$SECOND_GONE" -eq 0 ] && [ ! -s "$SB/monitor2.out" ] && [ "$UNTOUCHED" -eq 0 ] && [ "$ENDED" -eq 0 ]; echo $?)" \
    "out=$MOUT alive=$ALIVE arc=$ARC second_gone=$SECOND_GONE untouched=$UNTOUCHED ended=$ENDED err=$(cat "$SB/monitor.err")"
export RICHOS_SESSION_PID="$SLEEPER"

new_state m2
ALIVE="$(bash "$SW" --alive 2>&1)"; ARC=$?
check "M02  --alive with nothing watching: NOT WATCHED, exit 1, and the fallback command" \
    "$( [ "$ARC" -eq 1 ] && has "$ALIVE" "NOT WATCHED" && has "$ALIVE" "stall-watch.sh --watch"; echo $?)" "rc=$ARC out=$ALIVE"

mkdir -p "$SB/not-adopted"
OUT="$(RICHOS_ENTITY_ROOT="$SB/not-adopted" bash "$SW" --monitor 2>&1)"; RC=$?
check "M03  a repository that never adopted the engine: the monitor is silent and exits 0" \
    "$( [ "$RC" -eq 0 ] && [ -z "$OUT" ]; echo $?)" "rc=$RC out=$OUT"

# --- N: the SessionStart notice and the wiring ---------------------------------
OUT="$(bash "$HOOK" </dev/null 2>&1)"; RC=$?
check "N01  the hook: additionalContext only, names --monitor and the --alive check, only reports" \
    "$( [ "$RC" -eq 0 ] && printf '%s' "$OUT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
c = d["hookSpecificOutput"]["additionalContext"]
ok = ("systemMessage" not in d and d["hookSpecificOutput"]["hookEventName"] == "SessionStart"
      and "stall-watch.sh --monitor" in c and "stall-watch.sh --alive" in c
      and "It only reports" in c and "never pauses, stops or kills" in c)
sys.exit(0 if ok else 1)'; echo $?)" "rc=$RC out=$OUT"
OUT="$(RICHOS_ENTITY_ROOT="$SB/not-adopted" bash "$HOOK" </dev/null 2>&1)"; RC=$?
check "N02  the hook is silent where the engine was never adopted" "$( [ "$RC" -eq 0 ] && [ -z "$OUT" ]; echo $?)" "rc=$RC out=$OUT"
python3 - "$ENGINE" <<'PY'; RC=$?
import json, os, sys
e = sys.argv[1]
mons = json.load(open(os.path.join(e, "monitors", "monitors.json")))
m = [x for x in mons if x.get("name") == "stall-watch"]
hooks = json.load(open(os.path.join(e, "hooks", "hooks.json")))["hooks"]
cmds = [h.get("command", "") for event, groups in hooks.items() for g in groups for h in g.get("hooks", [])
        if "session-start-stall.sh" in h.get("command", "")]
start = [h.get("command", "") for g in hooks.get("SessionStart", []) for h in g.get("hooks", [])
         if "session-start-stall.sh" in h.get("command", "")]
ok = (len(m) == 1 and m[0].get("when") == "always" and m[0]["command"].endswith("/scripts/stall-watch.sh --monitor")
      and len(cmds) == 1 and len(start) == 1)
sys.exit(0 if ok else 1)
PY
check "N03  monitors.json starts stall-watch.sh --monitor always; hooks.json registers session-start-stall.sh once at SessionStart" \
    "$RC" "monitors.json or hooks.json wiring"

if [ "$FAIL" -gt 0 ]; then
    echo "=== stall-watch tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== stall-watch tests: all $PASS passed ==="
exit 0
