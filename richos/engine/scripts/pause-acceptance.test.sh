#!/usr/bin/env bash
#
# pause-acceptance.test.sh — the one-agent acceptance harness, driven with a FAKE agent.
#
# No real agent runs. A fake agent (fake-agent.py below) stands where Claude Code's
# subagent would: each of its Bash calls is rewritten by the real shell-evidence
# hook and run as the harness runs one (a shell leading its own process group), it
# starts the real pause-acceptance-worker.py counter in the background and then
# runs its steps, and after each call it appends a line to its own transcript.
# The generated pause and RESUME reach the real registry through the real
# PostToolUse[SendMessage] lifecycle hook, which holds and releases the work.
#
#   A1-A4  the whole run PASSES: frozen within seconds, held, resumed, receipt written
#   N1     the agent's run ends during the hold (SubagentStop)       -> FAIL, named
#   N2     a handwritten pause that freezes nothing                 -> FAIL, named
#   N3     one of its commands runs during the hold                 -> FAIL, named
#   N4     the counter is restarted instead of continued            -> FAIL, named
#
# Everything lives in a sandbox (HOME, CLAUDE_CONFIG_DIR, the registry, the hold
# state, the platform's projects directory). Only processes this test started are
# signaled. Usage: pause-acceptance.test.sh [--keep]

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/.." && pwd)"
HOOKS="$ENGINE/scripts/hooks"
LIB="$ENGINE/scripts/lib"
KEEP=0; [ "${1:-}" = "--keep" ] && KEEP=1

RUN=0; RED=0
sub() { # <label> <cond> [detail]
    RUN=$((RUN + 1))
    if eval "$2"; then printf '  ok    %s\n' "$1"; else
        printf '  FAIL  %s\n' "$1"; RED=$((RED + 1))
        [ -n "${3:-}" ] && printf '        %s\n' "$(printf '%s' "$3" | head -c 1200 | tr '\n' ' ')"
    fi
}

T="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/pause-acceptance.XXXXXX")" && pwd -P)"
OURS=()   # every pid this test started, recorded at spawn
cleanup() {
    # Each fake agent's own record of what it started: its calls' process groups.
    for f in "$T"/*.groups; do
        [ -e "$f" ] || continue
        touch "${f%.groups}.stop"
        for g in $(cat "$f"); do kill -CONT -- "-$g" 2>/dev/null; kill -9 -- "-$g" 2>/dev/null; done
    done
    for p in "${OURS[@]+"${OURS[@]}"}"; do kill -CONT "$p" 2>/dev/null; kill -9 "$p" 2>/dev/null; done
    if [ "$KEEP" -eq 1 ]; then echo "sandbox kept: $T"; else rm -rf "$T"; fi
}
trap cleanup EXIT

export HOME="$T/home" CLAUDE_CONFIG_DIR="$T/home/.claude" RICHOS_PROJECTS_DIR="$T/projects"
mkdir -p "$CLAUDE_CONFIG_DIR"
export GIT_CONFIG_GLOBAL="$T/home/.gitconfig"
printf '[user]\n\tname = accept\n\temail = accept@example.invalid\n[init]\n\tdefaultBranch = main\n' > "$GIT_CONFIG_GLOBAL"
unset RICHOS_WORKSPACES_DIR RICHOS_SESSION_ID CLAUDE_PROJECT_DIR RICHOS_ENGINE_ROOT CLAUDE_PLUGIN_ROOT RICHOS_SESSION_PID RICHOS_AGENT_HOLD_DIR 2>/dev/null || true
export SEAL_WAIT_SECONDS=0 RICHOS_WORKSPACES_SPAWN_WINDOW=0 RICHOS_WORKSPACES_STOP_GRACE=1 RICHOS_WORKSPACES_RETRY_BASE=0
export TESTVM_ROOT="$T/testvm" RICHOS_CPU_GUARD_STATE="$T/cpu-guard" RICHOS_AGENT_HOLD_WATCH_SECONDS=0.5
mkdir -p "$TESTVM_ROOT"
HOLD_STATE="$CLAUDE_CONFIG_DIR/state/agent-hold"
ACCEPT="$ENGINE/scripts/pause-acceptance.py"
WORKER="$ENGINE/scripts/pause-acceptance-worker.py"

ENT="$T/entity"
mkdir -p "$ENT" && git init -q -b main "$ENT"
printf 'x\n' > "$ENT/README"
cp "$ENGINE/orchestration.config" "$ENT/orchestration.config"
printf '.claude/\n.env\n' > "$ENT/.gitignore"
git -C "$ENT" add -A && git -C "$ENT" commit -q -m init
export RICHOS_ENTITY_ROOT="$ENT"

SID="sess-accept-1111"
payload() { python3 -c 'import json,sys; d=json.loads(sys.argv[2]); d["hook_event_name"]=sys.argv[1]; print(json.dumps(d))' "$1" "$2"; }
lifecycle() { bash "$HOOKS/workspace-lifecycle.sh"; }
LEAD="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
OURS+=("$LEAD")   # started by this test, so cleaned up by it
export RICHOS_SESSION_PID="$LEAD"
payload SessionStart "{\"session_id\":\"$SID\",\"cwd\":\"$ENT\"}" | lifecycle >/dev/null 2>>"$T/hooks.err"
python3 "$ENGINE/mega-lander/workspaces.py" --entity "$ENT" --session "$SID" integration --repo "$ENT" --branch main \
    --why "the pause acceptance suite's body of work" >/dev/null 2>>"$T/hooks.err"

register_agent() { # <name> <agent-id>
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[3],"tool_input":{"name":sys.argv[2],"subagent_type":"zach","isolation":"worktree","prompt":"the pause acceptance test agent"}}))' "$SID" "$1" "$ENT" \
        | bash "$HOOKS/guard-worktree-isolation.sh" >/dev/null 2>>"$T/spawn.err"
    local np="$ENT/.claude/worktrees/agent-$2"
    git -C "$ENT" worktree add -q "$np" -b "worktree-agent-$2"
    payload SubagentStart "{\"session_id\":\"$SID\",\"agent_id\":\"$2\",\"agent_type\":\"zach\",\"cwd\":\"$np\"}" | lifecycle >/dev/null 2>>"$T/hooks.err"
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[4],"tool_input":{"name":sys.argv[2],"isolation":"worktree"},"tool_response":{"agentId":sys.argv[3],"status":"async_launched"}}))' "$SID" "$1" "$2" "$ENT" \
        | lifecycle >/dev/null 2>>"$T/hooks.err"
    mkdir -p "$T/projects/proj/$SID/subagents"
    printf '{"timestamp":"%s","type":"user","message":"the task"}\n' "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" \
        > "$T/projects/proj/$SID/subagents/agent-$2.jsonl"
}
send() { # <to> <message> <out-file>: the lead's PostToolUse[SendMessage]
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"tool_name":"SendMessage","cwd":sys.argv[4],"tool_input":{"to":sys.argv[2],"message":sys.argv[3]},"tool_response":{}}))' "$SID" "$1" "$2" "$ENT" \
        | lifecycle >"$3" 2>>"$T/hooks.err"
}
generated() { # <json-from-the-harness> -> the message field
    python3 -c 'import json,sys; print(json.loads(sys.argv[1])["message"])' "$1"
}

# The fake agent: the counter in the background, then steps, each a rewritten Bash call.
cat > "$T/fake-agent.py" <<'PY'
import json, os, shlex, shutil, subprocess, sys, time
sid, aid, hooks, worker, work, transcript, groups, stop = sys.argv[1:9]
q = shlex.quote  # the engine under test may live under "Application Support"
shell = shutil.which("zsh") or "/bin/sh"
def call(n, command, wait=True):
    hook = subprocess.run(["bash", os.path.join(hooks, "shell-evidence.sh")], capture_output=True, text=True,
                          input=json.dumps({"hook_event_name": "PreToolUse", "session_id": sid, "agent_id": aid,
                                            "tool_use_id": "toolu_%s" % n, "tool_name": "Bash", "cwd": work,
                                            "tool_input": {"command": command}}))
    response = json.loads(hook.stdout)["hookSpecificOutput"]
    if response.get("permissionDecision") == "deny":
        call("wait_" + n, "python3 %s wait" % q(os.path.join(hooks, "../lib/agent_hold.py")))
        return
    rewritten = response["updatedInput"]["command"]
    p = subprocess.Popen([shell, "-c", rewritten], process_group=0, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with open(groups, "a") as g:
        g.write("%d\n" % p.pid)
    if wait:
        p.wait()
    with open(transcript, "a") as t:
        t.write(json.dumps({"timestamp": time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime()),
                            "type": "tool_result", "call": n}) + "\n")
os.makedirs(work, exist_ok=True)
call("count", "python3 %s count --dir %s" % (q(worker), q(work)), wait=False)
i = 0
while not os.path.exists(stop):
    i += 1
    call("step%d" % i, "python3 %s step --dir %s --seconds 1.5" % (q(worker), q(work)))
PY
start_fake() { # <name> <agent-id> <work-dir>: its records are <work-dir>.groups and .stop
    python3 "$T/fake-agent.py" "$SID" "$2" "$HOOKS" "$WORKER" "$3" \
        "$T/projects/proj/$SID/subagents/agent-$2.jsonl" "$3.groups" "$3.stop" &
    FAKE_PID=$!; FAKE_WORK="$3"
    OURS+=("$FAKE_PID")
    local i=0; while [ ! -s "$3/progress.log" ] && [ $i -lt 200 ]; do sleep 0.05; i=$((i + 1)); done
    sleep 1
}
stop_fake() { # after a case: end the fake agent and everything it started
    touch "$FAKE_WORK.stop"
    for g in $(cat "$FAKE_WORK.groups" 2>/dev/null); do kill -CONT -- "-$g" 2>/dev/null; kill -9 -- "-$g" 2>/dev/null; done
    kill -9 "$FAKE_PID" 2>/dev/null
    wait "$FAKE_PID" 2>/dev/null
}
accept() { python3 "$ACCEPT" "$@"; }

echo "=== A the whole run, as Rich will run it: one agent, paused once, held, resumed ==="
register_agent "zach-opus-accept1" "aacceptoneoneone1"
W1="$T/work1"; start_fake "zach-opus-accept1" "aacceptoneoneone1" "$W1"
accept start --agent zach-opus-accept1 --work-dir "$W1" --session "$SID" --hold-seconds 6 --receipt-dir "$T/receipts" >"$T/a-start.out" 2>&1; rc=$?
sub "A1 start: the agent is doing measurable work, and the generated pause is printed" \
    "[ $rc -eq 0 ] && grep -q '^START zach-opus-accept1' '$T/a-start.out' && grep -q 'richos-pause-control' '$T/a-start.out'" "$(cat "$T/a-start.out")"
send "zach-opus-accept1" "$(generated "$(tail -1 "$T/a-start.out")")" "$T/a-pause.out"
accept next --work-dir "$W1" >"$T/a-paused.out" 2>&1; rc=$?
sub "A2 paused: frozen within seconds, CPU near zero, counter stopped" \
    "[ $rc -eq 0 ] && grep -q '^PAUSED zach-opus-accept1' '$T/a-paused.out'" "$(cat "$T/a-paused.out" "$T/a-pause.out")"
accept next --work-dir "$W1" >"$T/a-hold.out" 2>&1; rc=$?
sub "A3 hold: nothing moved for the hold, its run did not end, and the generated RESUME is printed" \
    "[ $rc -eq 0 ] && grep -q '^HELD zach-opus-accept1' '$T/a-hold.out' && grep -q 'richos-resume-control' '$T/a-hold.out'" "$(cat "$T/a-hold.out")"
send "zach-opus-accept1" "$(generated "$(tail -1 "$T/a-hold.out")")" "$T/a-resume.out"
accept next --work-dir "$W1" >"$T/a-resumed.out" 2>&1; rc=$?
RECEIPT="$(sed -n 's/^Receipt: //p' "$T/a-resumed.out")"
sub "A4 resumed: PASS, the same process continued from the frozen value, and the receipt is written" \
    "[ $rc -eq 0 ] && grep -q '^PASS zach-opus-accept1' '$T/a-resumed.out' && [ -s \"$RECEIPT\" ] && [ -s \"\${RECEIPT%.md}.json\" ] && grep -q 'PASS' \"$RECEIPT\"" \
    "$(cat "$T/a-resumed.out" "$T/a-resume.out")"
sub "A5 a finished run refuses another step" "! accept next --work-dir '$W1' >/dev/null 2>&1"
stop_fake

run_to_hold() { # <name> <agent-id> <work-dir> <tag>: start, pause, paused
    register_agent "$1" "$2"
    start_fake "$1" "$2" "$3"
    accept start --agent "$1" --work-dir "$3" --session "$SID" --hold-seconds 6 --receipt-dir "$T/receipts" >"$T/$4-start.out" 2>&1
    send "$1" "$(generated "$(tail -1 "$T/$4-start.out")")" "$T/$4-pause.out"
    accept next --work-dir "$3" >"$T/$4-paused.out" 2>&1
}

echo "=== N1 the agent's run ends during the hold -> FAIL ==="
run_to_hold "zach-opus-accept2" "aaccepttwotwotwo2" "$T/work2" n1
payload SubagentStop "{\"session_id\":\"$SID\",\"agent_id\":\"aaccepttwotwotwo2\"}" | lifecycle >/dev/null 2>>"$T/hooks.err"
accept next --work-dir "$T/work2" >"$T/n1-hold.out" 2>&1; rc=$?
sub "N1 the hold step fails, naming the ended run" "[ $rc -eq 1 ] && grep -q \"run ended during the hold\" '$T/n1-hold.out'" "$(cat "$T/n1-hold.out")"
send "zach-opus-accept2" "$(python3 "$LIB/pause_protocol.py" --resume --to zach-opus-accept2 | python3 -c 'import json,sys; print(json.load(sys.stdin)["message"])')" /dev/null
stop_fake

echo "=== N2 a handwritten pause that freezes nothing -> FAIL ==="
register_agent "zach-opus-accept3" "aacceptthreethre3"
start_fake "zach-opus-accept3" "aacceptthreethre3" "$T/work3"
accept start --agent zach-opus-accept3 --work-dir "$T/work3" --session "$SID" --hold-seconds 6 --receipt-dir "$T/receipts" >/dev/null 2>&1
send "zach-opus-accept3" "$(printf 'Please hold for five minutes.\npause-until: my word')" "$T/n2-pause.out"
accept next --work-dir "$T/work3" >"$T/n2-paused.out" 2>&1; rc=$?
sub "N2 the paused step fails: not the generated message" "[ $rc -eq 1 ] && grep -q 'not the unchanged generated message' '$T/n2-paused.out'" "$(cat "$T/n2-paused.out")"
send "zach-opus-accept3" "carry on" /dev/null      # a handwritten pause ends on any message
stop_fake

echo "=== N3 a command of the agent runs during the hold -> FAIL ==="
run_to_hold "zach-opus-accept4" "aacceptfourfourf4" "$T/work4" n3
printf '%s 1 start\n' "$(python3 -c 'import time; print("%.3f" % time.time())')" >> "$T/work4/commands.log"
accept next --work-dir "$T/work4" >"$T/n3-hold.out" 2>&1; rc=$?
sub "N3 the hold step fails, naming the command that ran" "[ $rc -eq 1 ] && grep -q 'a command of the agent ran during the hold' '$T/n3-hold.out'" "$(cat "$T/n3-hold.out")"
send "zach-opus-accept4" "$(python3 "$LIB/pause_protocol.py" --resume --to zach-opus-accept4 | python3 -c 'import json,sys; print(json.load(sys.stdin)["message"])')" /dev/null
stop_fake

echo "=== N4 the counter is restarted instead of continued -> FAIL ==="
run_to_hold "zach-opus-accept5" "aacceptfivefivef5" "$T/work5" n4
accept next --work-dir "$T/work5" >"$T/n4-hold.out" 2>&1
send "zach-opus-accept5" "$(generated "$(tail -1 "$T/n4-hold.out")")" "$T/n4-resume.out"
# What the incident did: the running counter is ended and a new one started in its place.
OLD="$(awk 'NR==1 {print $2}' "$T/work5/progress.log")"
kill -9 "$OLD" 2>/dev/null
python3 "$WORKER" count --dir "$T/work5" --ticks 40 &
NEWC=$!
accept next --work-dir "$T/work5" >"$T/n4-resumed.out" 2>&1; rc=$?
kill -9 "$NEWC" 2>/dev/null; wait "$NEWC" 2>/dev/null
sub "N4 the resumed step fails: the counter was restarted" "[ $rc -eq 1 ] && grep -q 'restarted' '$T/n4-resumed.out'" "$(cat "$T/n4-resumed.out")"
sub "N5 a failed run writes its receipt, naming the failure" "grep -l 'FAILED at resumed' '$T/receipts/'*accept5.md >/dev/null"
stop_fake

sub "Z  nothing is left frozen or held by this test" "[ -z \"\$(ls $HOLD_STATE/held 2>/dev/null)\" ]" "$(ls "$HOLD_STATE/held" 2>&1)"
echo ""
echo "CHECKS: $RUN  RED: $RED"
[ "$RED" -eq 0 ]
