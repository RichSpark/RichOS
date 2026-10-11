#!/usr/bin/env bash
#
# resource-waits.test.sh — NO JOB WAITS ON A SHARED RESOURCE FOR MORE THAN TEN
#                          MINUTES WITHOUT THE LEAD'S TURN END BEING REFUSED.
#
# The hook and its analyzer:
#   scripts/hooks/guard-resource-waits.sh   Stop, BLOCKING
#   scripts/lib/resource_waits.py           the three wait sources, the holder
#                                           facts, the writer (`waiting()`), and
#                                           the `list`, `wait`, `wait-over` CLI
#
# THE BRIEF'S THREE CONTROLS, FIRST: an 11-minute waiter blocks (RW10); a
# 9-minute one does not (RW11); a lock held by a running walk with no waiter
# does not (RW12). Every silence case is paired with a speech case on the same
# path, so a switched-off predicate cannot pass this suite.
#
#   RW01..RW03  the escalation classifier, calibrated on the real ledger
#   RW10..RW19  wait records and the slot queue, and who holds the VM
#   RW20..RW27  escalations: acks never clear, what does clear
#   RW30..RW36  the wrapper: teammates, re-fire, config, cannot-read, cost
#   RW40..RW42  the writers: `wait`, the waits directory, prune
#   RW45..RW48  a teammate the lead holds (agent_hold.py) is not waiting in line
#
# NOTHING HERE TOUCHES THE OPERATOR'S STATE: the waits directory, the test VM
# root, the escalation ledger and the workspace registry are all sandboxed, and
# every process this suite starts is captured at spawn and stopped by that pid.
# A synthetic "eleven-minute" wait is made by moving the gate's clock
# (RICHOS_RESOURCE_WAITS_NOW), never by waiting eleven minutes; process start
# times are always read against the real clock.
#
# THE CASE IDS ARE FOUR CHARACTERS AND NO ID IS A PREFIX OF ANOTHER, because the
# mutation harness greps `FAIL  <id>`.
#
# Usage: scripts/hooks/resource-waits.test.sh
# Exit:  0 all cases passed, 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LIB="$ENGINE_ROOT/scripts/lib/resource_waits.py"
HOOK="$SCRIPT_DIR/guard-resource-waits.sh"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; FAIL=$((FAIL + 1)); return 0; }

command -v python3 >/dev/null 2>&1 || { echo "ERROR: needs python3" >&2; exit 1; }
command -v git     >/dev/null 2>&1 || { echo "ERROR: needs git" >&2; exit 1; }

# ALLOCATED, NOT NAMED (scripts/lib/scratch.sh): a run stopped by a signal, or
# by the mutation harness's stop-at-want, leaves a directory the sweeper finds.
# shellcheck source=../lib/scratch.sh
. "$ENGINE_ROOT/scripts/lib/scratch.sh"
SANDBOX="$(scratch_new resource-waits-test)" || { echo "FATAL: no scratch" >&2; exit 1; }
SANDBOX="$(cd "$SANDBOX" && pwd -P)"
OWNED_PIDS=""
cleanup() {
    # Only pids this suite captured at spawn. No `wait`: bash 3.2's `wait PID`
    # on a pid it had already reaped was measured hanging here.
    for p in $OWNED_PIDS; do kill "$p" 2>/dev/null || true; done
    chmod -R u+rwx "$SANDBOX" 2>/dev/null || true
    rm -rf "$SANDBOX"
}
trap cleanup EXIT
trap 'exit 143' TERM INT HUP

export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces"
export RICHOS_WAITS_DIR="$SANDBOX/waits"
export TESTVM_ROOT="$SANDBOX/testvm"
export RICHOS_ESCALATION_LEDGER="$SANDBOX/escalations.jsonl"
export RICHOS_AGENT_HOLD_DIR="$SANDBOX/agent-hold"
unset RICHOS_RESOURCE_WAITS_NOW RICHOS_RESOURCE_WAIT_MINUTES RICHOS_WAITER
mkdir -p "$RICHOS_WAITS_DIR" "$TESTVM_ROOT/run" "$SANDBOX/bin" "$RICHOS_AGENT_HOLD_DIR"
: > "$RICHOS_ESCALATION_LEDGER"

echo "=== resource waits: the lead's turn does not end while a job waits in line ==="
echo ""

SEAT="$SANDBOX/seat"
mk_repo() { # <path>
    mkdir -p "$1"
    git -C "$1" init -q -b main
    git -C "$1" config user.email "tester@example.invalid"
    git -C "$1" config user.name  "tester"
    mkdir -p "$SANDBOX/nohooks"
    git -C "$1" config core.hooksPath "$SANDBOX/nohooks"
    printf 'seed\n' > "$1/README.md"
    git -C "$1" add -A >/dev/null 2>&1
    git -C "$1" commit -qm seed >/dev/null 2>&1
}
mk_repo "$SEAT"
mkdir -p "$SEAT/.claude/state"
: > "$SEAT/orchestration.config"

# --- fakes ---------------------------------------------------------------
# slots.py: `run --wait N -- WAIT` sleeps (a caller still in the queue);
# `run --wait N -- CMD...` runs CMD as its child (a caller holding a slot).
# A holder takes its child down with it, as slots.py does, so no fake outlives
# the suite; every fake also ends by itself after two minutes.
# (The handler kills and leaves at once: calling child.wait() inside it while the
# main thread sits in child.wait() was measured deadlocking the fake forever.)
cat > "$SANDBOX/bin/slots.py" <<'PY'
import os, signal, subprocess, sys, time
cmd = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
if cmd == ["WAIT"] or not cmd:
    time.sleep(120)
    sys.exit(0)
child = subprocess.Popen(cmd)
def stop(signum, frame):
    child.kill()
    os._exit(0)
signal.signal(signal.SIGTERM, stop)
sys.exit(child.wait())
PY
for s in delta-walk.py hold-walk.py run-walk.py; do
    printf 'import time\ntime.sleep(120)\n' > "$SANDBOX/bin/$s"
done

spawn() { # <command...>  -> SPAWNED (pid, captured at spawn)
    # The subshell drops the EXIT trap and replaces itself with the command, so
    # the pid IS the command and no copy of `cleanup` can run in a child.
    ( trap - EXIT; exec "$@" ) >/dev/null 2>&1 &
    disown "$!" 2>/dev/null || true
    SPAWNED=$!
    OWNED_PIDS="$OWNED_PIDS $SPAWNED"
}
gone() { # <pid> — bounded poll until the pid is gone; never `wait`
    local i=0
    while kill -0 "$1" 2>/dev/null && [ "$i" -lt 50 ]; do
        python3 -c 'import time; time.sleep(0.1)'
        i=$((i + 1))
    done
    ! kill -0 "$1" 2>/dev/null
}
stop_pid() { # <pid> — only a pid this suite spawned
    kill "$1" 2>/dev/null || true
    gone "$1" || true
}
settle() { # give a spawned process time to exec (ps shows the new argv)
    python3 -c 'import time; time.sleep(0.4)'
}

now() { python3 -c 'import time; print("%.0f" % time.time())'; }

await_child() { # <pid> — bounded poll until that pid (a fake holder) has started its child
    local i=0
    while [ -z "$(ps -A -o ppid= | awk -v p="$1" '$1==p' | head -1)" ] && [ "$i" -lt 50 ]; do
        python3 -c 'import time; time.sleep(0.1)'
        i=$((i + 1))
    done
}

await_record() { # <pid> — bounded poll until that pid's wait record exists
    local i=0
    while ! ls "$RICHOS_WAITS_DIR"/"$1"-*.json >/dev/null 2>&1 && [ "$i" -lt 50 ]; do
        python3 -c 'import time; time.sleep(0.1)'
        i=$((i + 1))
    done
}

# write_record <pid> <resource> <since-epoch> [holding-path] -> RECORD (path)
write_record() {
    RECORD="$RICHOS_WAITS_DIR/$1-test$RANDOM.json"
    PID="$1" RES="$2" SINCE="$3" HOLD="${4:-}" OUT="$RECORD" python3 -c '
import json, os
rec = {"schema": "richos-wait/1", "pid": int(os.environ["PID"]), "since": float(os.environ["SINCE"]),
       "resource": os.environ["RES"], "waiter": "tester-waiter-%s" % os.environ["PID"],
       "reason": "total CPU 93% is at or above the 80% line",
       "holding": [os.environ["HOLD"]] if os.environ["HOLD"] else [],
       "cwd": "/tmp", "command": "reserve.py --wait 3600 -- make"}
open(os.environ["OUT"], "w").write(json.dumps(rec) + "\n")'
}

# stop_run [extra-json] -> SOUT, SERR, SRC
stop_run() {
    local payload
    payload="$(SEAT="$SEAT" EXTRA="${1:-}" python3 -c '
import json, os
p = {"hook_event_name": "Stop", "session_id": "aaaa0001-0000-4000-8000-000000000000",
     "cwd": os.environ["SEAT"], "prompt_id": "bbbb0001-0000-4000-8000-000000000000",
     "transcript_path": os.path.join(os.environ["SEAT"], "no-transcript.jsonl"),
     "stop_hook_active": False, "last_assistant_message": "Done."}
if os.environ.get("EXTRA"):
    p.update(json.loads(os.environ["EXTRA"]))
print(json.dumps(p))')"
    SOUT="$(printf '%s' "$payload" | (cd "$SEAT" && RICHOS_ENTITY_ROOT="$SEAT" bash "$HOOK" 2>"$SANDBOX/stop.err"))"
    SRC=$?
    SERR="$(cat "$SANDBOX/stop.err" 2>/dev/null)"
}

has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }

clear_all() {
    rm -f "$RICHOS_WAITS_DIR"/*.json "$TESTVM_ROOT"/guest*.lock
    rm -rf "$TESTVM_ROOT/run"/* "$RICHOS_AGENT_HOLD_DIR"/*
    : > "$RICHOS_ESCALATION_LEDGER"
    unset RICHOS_RESOURCE_WAITS_NOW
}

# ===========================================================================
# RW01..RW03 — the escalation classifier
# ===========================================================================
CLS="$(python3 - "$ENGINE_ROOT/scripts/lib" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
import resource_waits as rw
# Calibrated on the real ledger, 2026-09-27: every Escalation carrying a wait
# word was read by hand. These are the true waits ...
true_cases = [
    ("VM boot proof for the claude refresh is queued behind ray9 (40+ min and counting)", rw.VM),
    ("Android A1: CPU admission refuses every Gradle run; first core build has waited 10+ minutes", rw.CPU),
    ("tom-opus-conf1: CPU admission has refused the Rust conformance verifier for 34+ minutes", rw.CPU),
    ("Host CPU pinned 97-100% (about half system time) since ~09:40Z: my cargo proof runs and the P12 VM re-run cannot be admitted", rw.CPU),
    ("echo-opus-adopt1 final proof selection not admitted for 60+ min: host CPU 100 percent, about 65 percent system time", rw.CPU),
    ("echo-opus-bgdone2 has waited 90 min for the VM go-file; items 1, 2 and 4 are committed, item 3 needs the VM", rw.VM),
    ("echo-opus-bgdone2: another 90 min without the VM go-file (waiting since 12:23Z); build ready at 1.2.0-dev.0d3dadf8", rw.VM),
    ("echo-opus-dedup2: 90 min waiting for the VM go file; P18 and the crash matrix not yet run, all code and unit work committed", rw.VM),
]
# ... and these carry the word and are NOT a teammate waiting.
false_cases = [
    ("Two VM slots will not end the waiting while the vm-queue go-files still hand one agent the VM for a whole job",
     "the harness: two slots taken only per run, --wait waits for a slot, run.sh refuses to boot a guest outside a slot"),
    ("one-pass slot: two brief premises are false (narrowing is a no-op; the machine worker budget already exists)", ""),
    ("Reap walk: the product back end never runs a tool command in the test VM, so the Stop, Quit and crash-with-commands steps cannot be driven", ""),
    ("The waiting sentence: hands-free install is a separate design session", ""),
    ("echo-x: no longer waiting for the VM; the walk ran at 14:02Z", ""),
]
wrong = []
for title, want in true_cases:
    got = rw.classify(title, "")
    if got != want:
        wrong.append("TRUE %r -> %r, wanted %r" % (title[:60], got, want))
for title, body in false_cases:
    got = rw.classify(title, body)
    if got is not None:
        wrong.append("FALSE %r -> %r, wanted None" % (title[:60], got))
print("\n".join(wrong) if wrong else "OK %d %d" % (len(true_cases), len(false_cases)))
PY
)"
case "$CLS" in
    OK*) ok "RW01 classifier: every calibrated true wait is read as its resource, every false one as none ($CLS)" ;;
    *)   bad "RW01 classifier" "$CLS" ;;
esac

SINCE_OUT="$(python3 - "$ENGINE_ROOT/scripts/lib" <<'PY'
import sys, calendar, time
sys.path.insert(0, sys.argv[1])
import resource_waits as rw
raised = calendar.timegm(time.strptime("2026-09-27T16:10:22Z", "%Y-%m-%dT%H:%M:%SZ"))
a = rw._since_from_text("another 90 min without the VM go-file (waiting since 12:23Z)", raised)
want_a = calendar.timegm(time.strptime("2026-09-27T12:23:00Z", "%Y-%m-%dT%H:%M:%SZ"))
b = rw._stated_wait_seconds("echo-opus-bgdone2 has waited 90 min for the VM go-file")
c = rw._stated_wait_seconds("echo-opus-dedup2: 90 min waiting for the VM go file")
print("OK" if (a == want_a and b == 5400 and c == 5400) else "since=%r stated=%r,%r" % (a, b, c))
PY
)"
[ "$SINCE_OUT" = "OK" ] && ok "RW02 the wait began when the escalation says it did ('waiting since 12:23Z', 'has waited 90 min')" \
                       || bad "RW02 stated wait start" "$SINCE_OUT"

# ===========================================================================
# RW10..RW19 — wait records, the slot queue, and who holds the VM
# ===========================================================================
clear_all
spawn sleep 120; W1=$SPAWNED
T0="$(now)"
write_record "$W1" cpu-admission "$T0"
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 660))
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "tester-waiter-$W1" && has "$SERR" "11 min" && has "$SERR" "CPU admission" \
   && has "$SERR" "THIS CLEARS ONLY WHEN THE WAIT ENDS" && has "$SERR" "largest users right now"; then
    ok "RW10 POSITIVE CONTROL: an 11-minute waiter REFUSES the turn and names the waiter, the resource and who uses the CPU"
else
    bad "RW10 11-minute waiter blocks" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 600)"
fi

export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 540))
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW11 NEGATIVE CONTROL: a 9-minute waiter does not refuse the turn"
else
    bad "RW11 9-minute waiter does not block" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi
unset RICHOS_RESOURCE_WAITS_NOW

# RW12 — a slot held by a running walk, nobody waiting.
clear_all
spawn python3 "$SANDBOX/bin/slots.py" run --wait 600 -- python3 "$SANDBOX/bin/delta-walk.py"; HOLDER=$SPAWNED
spawn sleep 120; GUEST=$SPAWNED
await_child "$HOLDER"
T0="$(now)"
printf '{"pid": %s, "since": %s, "purpose": "delta-walk.py for tester", "slot": "guest.lock"}\n' \
    "$HOLDER" "$((T0 - 300))" > "$TESTVM_ROOT/guest.lock"
mkdir -p "$TESTVM_ROOT/run/walk-test1"
echo "$GUEST" > "$TESTVM_ROOT/run/walk-test1/vm.pid"
echo "$TESTVM_ROOT/guest.lock" > "$TESTVM_ROOT/run/walk-test1/slot"
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 3600))
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW12 NEGATIVE CONTROL: a lock held by a running walk, with no waiter, does not refuse the turn (even an hour on)"
else
    bad "RW12 held lock without a waiter" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 2000)"
fi

# RW13 — the same holder, and a caller waiting for a slot for 11 minutes. The
# fake waiter records its wait through the real writer (`resource_waits.py
# wait`, which is `waiting()`), exactly as the slot queue will: a wait is
# recorded, never inferred.
spawn python3 "$LIB" wait --resource vm --until-exists "$SANDBOX/never-13" --every 1 --waiter slot-waiter-13; QW=$SPAWNED
await_record "$QW"
export RICHOS_RESOURCE_WAITS_NOW=$(( $(now) + 660 ))
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "slot-waiter-13" && has "$SERR" "pid $QW" && has "$SERR" "the test VM" \
   && has "$SERR" "guest.lock held by pid $HOLDER" && has "$SERR" "delta-walk.py for tester" \
   && has "$SERR" "IN USE RIGHT NOW: YES" && has "$SERR" "walk-test1"; then
    ok "RW13 a recorded slot waiter of 11 minutes is refused on, naming the holder, its purpose and that a run IS executing"
else
    bad "RW13 queue waiter + busy holder" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 900)"
fi

# RW14 — the holder holds the slot and no guest is booted.
rm -rf "$TESTVM_ROOT/run/walk-test1"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "IN USE RIGHT NOW: NO. The slot is held but no guest is booted"; then
    ok "RW14 a slot held with no guest booted is reported as NOT in use"
else
    bad "RW14 held, no guest" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 700)"
fi

# RW44 — slots.py writes `"state": "admitting"` while the holder checks CPU and
# memory: the slot is held and nothing runs yet.
printf '{"pid": %s, "since": %s, "purpose": "run-walk walk-x delta-walk.py", "slot": "guest.lock", "state": "admitting"}\n' \
    "$HOLDER" "$(( $(now) - 60 ))" > "$TESTVM_ROOT/guest.lock"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "IN USE RIGHT NOW: NO. The holder is still being admitted"; then
    ok "RW44 a slot whose holder is still being admitted (slots.py's \"admitting\" state) is reported as NOT in use"
else
    bad "RW44 admitting holder" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 900)"
fi

# RW15 — the lock is held by a walk that is itself waiting for CPU (reserve.py's `holding`).
rm -f "$TESTVM_ROOT/guest.lock"
: > "$TESTVM_ROOT/guest.lock"
spawn sleep 120; LW=$SPAWNED
write_record "$LW" cpu-admission "$(now)" "$TESTVM_ROOT/guest.lock"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "guest.lock held by pid $LW" && has "$SERR" "the holder is itself waiting"; then
    ok "RW15 a walk holding guest.lock while it waits for CPU is named as the holder and as NOT using the VM"
else
    bad "RW15 holder that is itself waiting" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 900)"
fi
rm -f "$RICHOS_WAITS_DIR"/"$LW"-*.json
stop_pid "$LW"

# RW16 — a guest held open by hold-walk.py for hand-driven steps.
spawn python3 "$SANDBOX/bin/slots.py" run --wait 600 -- python3 "$SANDBOX/bin/hold-walk.py"; HH=$SPAWNED
spawn sleep 120; GUEST2=$SPAWNED
await_child "$HH"
printf '{"pid": %s, "since": %s, "purpose": "hold-walk.py for tester", "slot": "guest.lock"}\n' \
    "$HH" "$(( $(now) - 1200 ))" > "$TESTVM_ROOT/guest.lock"
mkdir -p "$TESTVM_ROOT/run/walk-test2"
echo "$GUEST2" > "$TESTVM_ROOT/run/walk-test2/vm.pid"
echo "$TESTVM_ROOT/guest.lock" > "$TESTVM_ROOT/run/walk-test2/slot"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "HELD OPEN, NOT RUNNING" && has "$SERR" "hold-walk.py keeps the guest up"; then
    ok "RW16 a guest held open by hold-walk.py is reported as held open, NOT running a scripted run"
else
    bad "RW16 hold-walk" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 900)"
fi

# RW17 — the wait ends: the queued caller exits. The turn may end.
stop_pid "$QW"
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW17 the moment the waiting process is gone, the turn is no longer refused"
else
    bad "RW17 wait over by exit" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 400)"
fi
unset RICHOS_RESOURCE_WAITS_NOW
stop_pid "$HH"; stop_pid "$GUEST2"; stop_pid "$HOLDER"; stop_pid "$GUEST"

# RW38 — A WAIT IS RECORDED, NEVER INFERRED (the lead's ruling on
# esc-20260927T192047Z-fb31d4e2). A process that merely LOOKS like a queue
# caller, by its script name and a --wait flag, is not counted...
clear_all
spawn python3 "$SANDBOX/bin/run-walk.py" --wait 1800 --bundle B.zip -- delta-walk.py; RWW=$SPAWNED
settle
export RICHOS_RESOURCE_WAITS_NOW=$(( $(now) + 660 ))
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW38 a process that only LOOKS like a slot caller (run-walk.py --wait, nothing recorded) is never counted as a wait"
else
    bad "RW38 no inference from process names" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 900)"
fi
# ... and the pair: the same process, once it records its wait, is.
write_record "$RWW" testvm-slot "$(( $(now) ))"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "tester-waiter-$RWW"; then
    ok "RW39 the pair to RW38: the same process with its wait RECORDED refuses the turn"
else
    bad "RW39 recorded wait counts" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 600)"
fi
stop_pid "$RWW"
unset RICHOS_RESOURCE_WAITS_NOW

# RW18 — a record whose process is dead is not a wait.
#
# THE SUBJECT IS DEAD AND REAPED BEFORE THE RECORD IS WRITTEN (2026-10-05). It was a spawn()ed,
# disowned pid stopped with stop_pid, which sends TERM, polls for at most 5 s and goes on
# whatever happened: under load the "dead" pid was still alive when the record was written,
# and the guard rightly refused (merge gate 2026-10-02 05:08, 09:10 and 17:57, the last after
# RW18z's zombie fix). Reproduced with a subject slow to die (20 s in its TERM handler, as a
# loaded Mac makes a plain sleep): the old setup wrote the record for a live pid and RW18 failed
# exactly as in the gate; this one passes with the same subject. So the subject is this shell's
# own child,
# killed with KILL, which cannot be caught, and reaped with `wait`, and the case refuses to run
# on a pid that still answers.
clear_all
RW18_SUBJECT=(sleep 120)
( trap - EXIT; exec "${RW18_SUBJECT[@]}" ) >/dev/null 2>&1 &
DEAD=$!
kill -KILL "$DEAD" 2>/dev/null
wait "$DEAD" 2>/dev/null
if kill -0 "$DEAD" 2>/dev/null; then
    bad "RW18 setup" "the dead-pid subject $DEAD still answers after KILL and wait; no record written"
fi
T0="$(now)"
write_record "$DEAD" cpu-admission "$T0"
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 3600))
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW18 a record left by a process that is gone does not refuse the turn"
else
    bad "RW18 dead-pid record" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi
# ... and the same record with a live pid DOES (the pair to RW18).
rm -f "$RICHOS_WAITS_DIR"/*.json
write_record "$W1" cpu-admission "$T0"
stop_run
[ "$SRC" = "2" ] && ok "RW19 the pair to RW18: the same record with a live pid refuses the turn" \
                 || bad "RW19 live-pid record" "rc=$SRC"

# RW18z — the deterministic form of RW18's load flake: a waiter that has exited
# but is not yet reaped (a zombie; kill -0 still succeeds) is not a wait.
rm -f "$RICHOS_WAITS_DIR"/*.json
python3 -c 'import subprocess,sys,time
p=subprocess.Popen(["true"]); print(p.pid, flush=True); time.sleep(60)' > "$SANDBOX/zpid" 2>/dev/null &
ZPAR=$!; OWNED_PIDS="$OWNED_PIDS $ZPAR"
for _ in $(seq 50); do [ -s "$SANDBOX/zpid" ] && break; python3 -c 'import time; time.sleep(0.1)'; done
ZPID="$(cat "$SANDBOX/zpid")"; settle
write_record "$ZPID" cpu-admission "$T0"
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW18z a record whose process exited but is unreaped (zombie) does not refuse the turn"
else
    bad "RW18z zombie-pid record" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi
stop_pid "$ZPAR"

# RW31 — a pid reused by a process that started after the wait began.
rm -f "$RICHOS_WAITS_DIR"/*.json
spawn sleep 120; REUSED=$SPAWNED
settle
write_record "$REUSED" cpu-admission "$(( $(now) - 3600 ))"
unset RICHOS_RESOURCE_WAITS_NOW
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW31 a record whose pid now belongs to a process younger than the wait is a dead wait, not a live one"
else
    bad "RW31 pid reuse" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi
stop_pid "$REUSED"

# ===========================================================================
# RW20..RW27 — escalations: an ack never clears; the wait ending does
# ===========================================================================
clear_all
WT="$SANDBOX/wt-echo-test"
mkdir -p "$WT"
GO="$SANDBOX/vm-queue/echo-test-go"
mkdir -p "$(dirname "$GO")"
T0="$(now)"
RAISED="$(python3 -c 'import sys,time; print(time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(int(sys.argv[1]) - 5)))' "$T0")"
esc_row() { # <id> <raised> <title> [question]
    ID="$1" RAISED="$2" TITLE="$3" Q="${4:-}" WT="$WT" python3 -c '
import json, os
print(json.dumps({"event": "Escalation", "id": os.environ["ID"], "raised": os.environ["RAISED"],
                  "teammate": "echo-test", "worktree": os.environ["WT"], "state": "proceeding", "for": "lead",
                  "title": os.environ["TITLE"], "question": os.environ["Q"], "meanwhile": "", "tried": ""}))' \
        >> "$RICHOS_ESCALATION_LEDGER"
}
esc_row esc-test-1 "$RAISED" "echo-test has waited 90 min for the VM go-file; item 3 needs the VM" \
    "When will $GO exist?"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "echo-test has waited 90 min for the test VM" && has "$SERR" "esc-test-1" \
   && has "$SERR" "$GO"; then
    ok "RW20 an escalation saying the teammate has waited 90 min for the VM refuses the turn at once, naming it and its go-file"
else
    bad "RW20 escalation wait" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 700)"
fi

printf '%s\n' "{\"event\": \"EscalationAck\", \"id\": \"esc-test-1\", \"acked\": \"$RAISED\", \"disposition\": \"Answered: keep waiting; the VM goes to echo-test after the current walk\"}" >> "$RICHOS_ESCALATION_LEDGER"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "esc-test-1"; then
    ok "RW21 an acknowledgement saying \"keep waiting\" does NOT clear it: the turn is still refused"
else
    bad "RW21 ack does not clear" "rc=$SRC"
fi

touch "$GO"
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW22 the go-file it waits for exists: the wait is over and the turn may end"
else
    bad "RW22 go-file ends the wait" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi
rm -f "$GO"

rmdir "$WT"
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW23 the waiting teammate's worktree is gone: its wait is over"
else
    bad "RW23 worktree gone ends the wait" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi
mkdir -p "$WT"

# RW24 — wait-over refuses a note that says the wait goes on, and it still blocks.
WO_OUT="$(python3 "$LIB" wait-over esc-test-1 --note "keep waiting, echo-test is next on the VM after the walk" 2>&1)"
WO_RC=$?
stop_run
if [ "$WO_RC" = "2" ] && has "$WO_OUT" "does not end it" && [ "$SRC" = "2" ]; then
    ok "RW24 wait-over REFUSES a note that says to keep waiting, and the turn is still refused"
else
    bad "RW24 keep-waiting note refused" "wait-over rc=$WO_RC out=$WO_OUT; stop rc=$SRC"
fi

WO_OUT="$(python3 "$LIB" wait-over esc-test-1 --note "the walk ran in slot 2 at 14:02Z; P18 passed, no VM needed any more" 2>&1)"
WO_RC=$?
stop_run
if [ "$WO_RC" = "0" ] && [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW25 wait-over with what actually ended the wait clears it"
else
    bad "RW25 wait-over clears" "wait-over rc=$WO_RC out=$WO_OUT; stop rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi

python3 -c 'import time; time.sleep(1.1)'
esc_row esc-test-2 "$(python3 -c 'import time; print(time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + 5)))')" \
    "echo-test: 40 min waiting for the VM again"
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "esc-test-2" && ! has "$SERR" "esc-test-1"; then
    ok "RW26 a later escalation that says it is waiting again is a NEW wait, and the one already over stays over"
else
    bad "RW26 new wait after wait-over" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 500)"
fi

# RW27 — the re-fire (stop_hook_active) with the wait still there is refused again.
stop_run '{"stop_hook_active": true}'
[ "$SRC" = "2" ] && ok "RW27 the re-fire (stop_hook_active) with the wait still on is REFUSED again, not waved through" \
                 || bad "RW27 re-fire" "rc=$SRC"

# ===========================================================================
# RW30..RW36 — the wrapper
# ===========================================================================
stop_run '{"agent_id": "a0000000-teammate"}'
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW30 a teammate's turn end is never refused (the lead is the one who can reorder)"
else
    bad "RW30 teammate" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 300)"
fi

printf 'RESOURCE_WAIT_MINUTES="60"\n' > "$SEAT/orchestration.config"
stop_run
S60=$SRC
printf 'RESOURCE_WAIT_MINUTES="30"\n' > "$SEAT/orchestration.config"
stop_run
S30=$SRC
if [ "$S60" = "0" ] && [ "$S30" = "2" ]; then
    ok "RW32 RESOURCE_WAIT_MINUTES is honored: a 40-minute wait passes a 60-minute line and is refused at 30"
else
    bad "RW32 threshold config" "60 -> rc=$S60, 30 -> rc=$S30"
fi

printf 'CHECK_RESOURCE_WAITS="0"\n' > "$SEAT/orchestration.config"
rm -rf "$SEAT/.claude/state/stop-hook-notices"
stop_run
if [ "$SRC" = "0" ] && has "$SOUT" "STOOD DOWN" && has "$SOUT" "systemMessage"; then
    ok "RW33 CHECK_RESOURCE_WAITS=0 stands the gate down, and says so to the operator"
else
    bad "RW33 stand-down announced" "rc=$SRC out=$SOUT"
fi
: > "$SEAT/orchestration.config"

# RW34 — a waits directory that cannot be read is announced; the turn is not
# refused for it, and the ledger's wait still refuses it.
chmod 000 "$RICHOS_WAITS_DIR"
rm -rf "$SEAT/.claude/state/stop-hook-notices"
stop_run
S_WITH=$SRC
if [ "$S_WITH" = "2" ] && has "$SERR" "could not be read"; then
    ok "RW34 an unreadable waits directory does not hide a wait read from the ledger, and the refusal says what was not read"
else
    bad "RW34 unreadable source" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 500)"
fi
: > "$RICHOS_ESCALATION_LEDGER"
rm -rf "$SEAT/.claude/state/stop-hook-notices"
stop_run
if [ "$SRC" = "0" ] && has "$SOUT" "could not be read" && has "$SOUT" "systemMessage"; then
    ok "RW35 with nothing else waiting, the unreadable directory is ANNOUNCED to the operator and the turn ends"
else
    bad "RW35 cannot-read announced" "rc=$SRC out=$SOUT err=$SERR"
fi
chmod 700 "$RICHOS_WAITS_DIR"

# RW36 — cost per turn end, measured: the quiet path and the refusing path with ps.
clear_all
COST_Q="$(python3 - "$HOOK" "$SEAT" <<'PY'
import json, os, subprocess, sys, time
hook, seat = sys.argv[1], sys.argv[2]
p = json.dumps({"hook_event_name": "Stop", "session_id": "aaaa0001-0000-4000-8000-000000000000", "cwd": seat,
                "prompt_id": "c", "transcript_path": os.path.join(seat, "x.jsonl"), "stop_hook_active": False,
                "last_assistant_message": "Done."})
env = dict(os.environ, RICHOS_ENTITY_ROOT=seat)
runs = []
for _ in range(5):
    t = time.time()
    subprocess.run(["bash", hook], input=p, capture_output=True, text=True, env=env, cwd=seat)
    runs.append(time.time() - t)
runs.sort()
print("%.0f" % (runs[2] * 1000))
PY
)"
spawn sleep 120; WC=$SPAWNED
# A pid younger than its wait reads as reused, so the refusing path is timed on the test clock.
write_record "$WC" cpu-admission "$(now)"
export RICHOS_RESOURCE_WAITS_NOW=$(( $(now) + 700 ))
COST_B="$(python3 - "$HOOK" "$SEAT" <<'PY'
import json, os, subprocess, sys, time
hook, seat = sys.argv[1], sys.argv[2]
p = json.dumps({"hook_event_name": "Stop", "session_id": "aaaa0001-0000-4000-8000-000000000000", "cwd": seat,
                "prompt_id": "c", "transcript_path": os.path.join(seat, "x.jsonl"), "stop_hook_active": False,
                "last_assistant_message": "Done."})
env = dict(os.environ, RICHOS_ENTITY_ROOT=seat)
runs, rcs = [], set()
for _ in range(5):
    t = time.time()
    r = subprocess.run(["bash", hook], input=p, capture_output=True, text=True, env=env, cwd=seat)
    runs.append(time.time() - t)
    rcs.add(r.returncode)
runs.sort()
print("%.0f %s" % (runs[2] * 1000, ",".join(str(c) for c in sorted(rcs))))
PY
)"
unset RICHOS_RESOURCE_WAITS_NOW
COST_BMS="${COST_B%% *}"
COST_BRC="${COST_B#* }"
if [ "${COST_Q:-99999}" -lt 1500 ] && [ "${COST_BMS:-99999}" -lt 2500 ] && [ "$COST_BRC" = "2" ]; then
    ok "RW36 cost per turn end, whole hook, median of 5: ${COST_Q} ms with nothing waiting; ${COST_BMS} ms refusing (with the ps read)"
else
    bad "RW36 cost" "quiet ${COST_Q} ms, refusing ${COST_BMS} ms (rcs $COST_BRC); ceilings 1500 / 2500 ms"
fi
stop_pid "$WC"

# ===========================================================================
# RW40..RW42 — the writers
# ===========================================================================
clear_all
FILE="$SANDBOX/go-file-40"
spawn python3 "$LIB" wait --resource vm --until-exists "$FILE" --every 1 --waiter tester-40
WPID=$SPAWNED
python3 -c 'import time; time.sleep(0.8)'
SEEN="$(cat "$RICHOS_WAITS_DIR"/"$WPID"-*.json 2>/dev/null || true)"
touch "$FILE"
if gone "$WPID"; then WRC=0; else WRC=still-running; fi
LEFT="$(ls "$RICHOS_WAITS_DIR" 2>/dev/null | grep -c . || true)"
if has "$SEEN" '"testvm-slot"' && has "$SEEN" '"tester-40"' && has "$SEEN" "$FILE" && [ "$WRC" = "0" ] && [ "$LEFT" = "0" ]; then
    ok "RW40 \`wait --until-exists\` is recorded as a VM wait while it polls, and removes its record when the file appears"
else
    bad "RW40 wait CLI" "seen=$SEEN rc=$WRC left=$LEFT"
fi

TO_OUT="$(python3 "$LIB" wait --resource cpu --until-exists "$SANDBOX/never" --every 1 --timeout 1 2>&1)"
TO_RC=$?
LEFT="$(ls "$RICHOS_WAITS_DIR" 2>/dev/null | grep -c . || true)"
if [ "$TO_RC" = "75" ] && [ "$LEFT" = "0" ]; then
    ok "RW41 a \`wait\` that times out exits 75 and leaves no record behind"
else
    bad "RW41 wait timeout" "rc=$TO_RC left=$LEFT out=$TO_OUT"
fi

PR="$(python3 - "$ENGINE_ROOT/scripts/lib" "$RICHOS_WAITS_DIR" <<'PY'
import os, subprocess, sys
sys.path.insert(0, sys.argv[1])
import resource_waits as rw
d = sys.argv[2]
p = subprocess.Popen(["sleep", "30"]); pid = p.pid; p.kill(); p.wait()
stale = os.path.join(d, "%d-deadbeef.json" % pid)
open(stale, "w").write("{}")
with rw.waiting(rw.CPU, "refused", holding=["/x/guest.lock"]) as w:
    inside = os.path.exists(w.path) and not os.path.exists(stale)
    rec = open(w.path).read()
after = os.path.exists(w.path)
print("OK" if inside and not after and '"/x/guest.lock"' in rec else "inside=%s after=%s rec=%s" % (inside, after, rec))
PY
)"
[ "$PR" = "OK" ] && ok "RW42 waiting() writes its record for exactly its body, with what it holds, and prunes a dead writer's leftover" \
                || bad "RW42 writer" "$PR"

# ===========================================================================
# RW45..RW48 — a teammate the lead HOLDS (agent_hold.py) is not waiting in line
# ===========================================================================
# 2026-10-11 04:26Z and 06:19Z: a frozen teammate's recorded wait kept its clock
# running, blocked the lead at ten minutes and forced an early resume. The hold
# records are written by hand in the agent_hold.py shape, in the sandbox; the
# release is the real `agent_hold.py release`.
HOLD_SID="aaaa0001-0000-4000-8000-000000000045"
write_hold() { # <agent-id> <name> <at-epoch> [pid] — a standing hold; [pid] frozen by it
    AID="$1" NAME="$2" AT="$3" HPID="${4:-}" python3 - "$ENGINE_ROOT/scripts/lib" <<'PY'
import json, os, sys
sys.path.insert(0, sys.argv[1])
import agent_hold
held = {}
if os.environ["HPID"]:
    held[os.environ["HPID"]] = agent_hold.snapshot()[int(os.environ["HPID"])]["birth"]
rec = {"session_id": os.environ["HOLD_SID"], "agent_id": os.environ["AID"], "name": os.environ["NAME"],
       "at": float(os.environ["AT"]), "held": held, "parents": {}, "detached": {}}
agent_hold._write_json(agent_hold._held_path(os.environ["HOLD_SID"], os.environ["AID"]), rec)
PY
}
export HOLD_SID

clear_all
spawn sleep 120; W45=$SPAWNED
settle
T0="$(now)"
write_record "$W45" cpu-admission "$T0"
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 900))
stop_run; BEFORE_RC=$SRC
# Held 11 minutes in: already over the line when the lead froze it.
write_hold a0000000000fix45 "tester-waiter-$W45" "$((T0 + 660))"
stop_run
LIST45="$(python3 "$LIB" list 2>&1)"; LRC45=$?
if [ "$BEFORE_RC" = "2" ] && [ "$SRC" = "0" ] && [ -z "$SERR" ] && [ "$LRC45" = "0" ] \
   && has "$LIST45" "HELD tester-waiter-$W45 is HELD by the lead" && has "$LIST45" "stopped at 11 min" \
   && ! has "$LIST45" "OVER"; then
    ok "RW45 a wait already 11 minutes old when its agent was HELD does not refuse the turn (the same wait refused before the hold); list shows it HELD, clock stopped at 11 min"
else
    bad "RW45 held wait does not block" "before=$BEFORE_RC rc=$SRC err=$(printf '%s' "$SERR" | head -c 400) list($LRC45)=$LIST45"
fi

stop_pid "$W45"

# RW46 — RESUME, through the real `agent_hold.py release`. An escalated wait
# (no pid, so its start can lie in the real past): waited 15 min, the last 12
# of them held. Five minutes after the release it has 3 + 5 = 8 counted
# minutes (20 if the held time counted): no refusal.
clear_all
WT45="$SANDBOX/wt-hold-test"; mkdir -p "$WT45"
T0="$(now)"
RAISED45="$(python3 -c 'import sys,time; print(time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(int(sys.argv[1]))))' "$T0")"
python3 -c '
import json, sys
print(json.dumps({"event": "Escalation", "id": "esc-test-45", "raised": sys.argv[1], "teammate": "hold-test",
                  "worktree": sys.argv[2], "state": "proceeding", "for": "lead",
                  "title": "hold-test has waited 15 min for CPU admission", "question": "", "meanwhile": "", "tried": ""}))' \
    "$RAISED45" "$WT45" >> "$RICHOS_ESCALATION_LEDGER"
write_hold a0000000000fix46 hold-test "$((T0 - 720))"
export RICHOS_RESOURCE_WAITS_NOW="$T0"
stop_run; HELD_RC=$SRC
REL46="$(python3 "$ENGINE_ROOT/scripts/lib/agent_hold.py" release --session "$HOLD_SID" --agent a0000000000fix46 2>&1)"
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 300))
stop_run
if [ "$HELD_RC" = "0" ] && has "$REL46" '"ok": true' && [ "$SRC" = "0" ] && [ -z "$SERR" ] \
   && grep -q '"name": "hold-test"' "$RICHOS_AGENT_HOLD_DIR/hold-history.jsonl"; then
    ok "RW46 after RESUME the wait counts again WITHOUT the held time: 8 counted minutes (20 raw) do not refuse"
else
    bad "RW46 resumed wait excludes held time" "held_rc=$HELD_RC release=$REL46 rc=$SRC err=$(printf '%s' "$SERR" | head -c 600)"
fi

# RW47 — ...and it does count again: two and a half minutes later it is over ten.
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 450))
stop_run
if [ "$SRC" = "2" ] && has "$SERR" "hold-test has waited 10 min" && has "$SERR" "held by the lead, not counted"; then
    ok "RW47 the resumed wait refuses again once its COUNTED time passes ten minutes, saying the held time was not counted"
else
    bad "RW47 resumed wait counts again" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 600)"
fi

# RW48 — the hold is matched by the frozen PROCESS too (pid and start time from
# the hold record), whatever name the waiter wrote.
clear_all
spawn sleep 120; W48=$SPAWNED
settle
T0="$(now)"
write_record "$W48" testvm-slot "$T0"
write_hold a0000000000fix48 some-other-name "$((T0 + 660))" "$W48"
export RICHOS_RESOURCE_WAITS_NOW=$((T0 + 900))
stop_run
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "RW48 a wait whose process the hold froze is HELD, matched by pid and start time, not by name"
else
    bad "RW48 held by pid" "rc=$SRC err=$(printf '%s' "$SERR" | head -c 400)"
fi
stop_pid "$W48"
clear_all

# RW43 — nothing this suite started outlives it (§54): every owned pid is
# stopped, and no process whose command line names the sandbox is left.
for p in $OWNED_PIDS; do stop_pid "$p"; done
LEFTOVER="$(SANDBOX="$SANDBOX" python3 -c '
import os, subprocess
out = subprocess.run(["ps", "-A", "-o", "pid=,command="], capture_output=True, text=True).stdout
me = os.getpid()
print(" ".join(l.split()[0] for l in out.splitlines()
               if os.environ["SANDBOX"] + "/bin/" in l and l.split()[0] != str(me)))')"
if [ -z "$LEFTOVER" ]; then
    ok "RW43 nothing this suite started is still running (checked by the sandbox path in every command line)"
else
    bad "RW43 leftover processes" "pids: $LEFTOVER"
fi

# ===========================================================================
echo ""
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$SCRIPT_DIR/resource-waits.mutation.sh" ]; then
    echo "=== running the mutation harness ==="
    if bash "$SCRIPT_DIR/resource-waits.mutation.sh"; then
        ok "RW90 the mutation harness: every property this suite claims was removed and watched going red"
    else
        bad "RW90 the mutation harness found a property this suite does not actually prove"
    fi
fi

echo ""
echo "resource waits: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
