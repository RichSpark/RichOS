#!/usr/bin/env bash
#
# workspace-stopped-ending.test.sh — POINT 11's FOURTH ENDING: "OR WAS STOPPED".
#
# The page: docs/plans/worktree-spec-2026-09-11.md, point 11 — "'Finished'
# means the agent's run has ended and Rich did not pause it. That covers every
# ending: it handed in its work, crashed, was cut off by a limit, OR WAS
# STOPPED. … An agent becomes finished when the platform's own end-of-run
# signal for it is recorded, AUTOMATICALLY AND NEVER BY RICH NOTICING." Point 5
# then guarantees the work is landed or discarded "whether or not Rich
# remembers", and points 4 and 7 delete both workspace and branch.
#
# WHY THIS SUITE EXISTS. A stopped run never reaches the end of a sub-run, so
# the platform delivers NO SubagentStop for it — measured on this machine on
# 2026-09-13: of 24 runs flagged `stoppedByUser` in 14 days, 23 had no terminal
# SubagentStop, and the two the CEO stopped that day have zero rows in
# worktree-ledger.jsonl (written by worker-ended-handoff.sh on every
# SubagentStop and on nothing else). Their workspaces sat for four and a half
# hours until somebody marked them finished BY HAND. The platform does record
# the stop — in its own per-agent record beside the agent's transcript,
# `subagents/agent-<id>.meta.json`, within 0-5 s — and observe_platform_end
# reads it.
#
# THE ONLY SIMULATED PARTY IS THE PLATFORM: the sandbox writes the hook
# payloads the platform sends and the per-agent record the platform writes, and
# every other step goes through the real hooks and the real workspaces.sh. HOME
# and CLAUDE_CONFIG_DIR are redirected; the operator's files are never touched.
#
# NOTHING IN THIS SUITE MARKS AN AGENT FINISHED. It never runs `workspaces.sh
# stop`, never sends a SubagentStop or a TaskStop for the stopped agent, and
# never writes to the state store. If a workspace goes, the system decided it.
#
# OUTPUT: `      ok   S1.2 …` / `      FAIL  S1.2 …` per sub-assertion, one
# `  PASS  S1  …` / `  FAIL  S1  …` per check, then:
#     CHECKS RUN: <N>  RED: <K>
# Exit 0 only when K = 0.
#
# Usage: workspace-stopped-ending.test.sh [--keep]

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/../.." && pwd)"
HOOKS="$ENGINE/scripts/hooks"
KEEP=0; [ "${1:-}" = "--keep" ] && KEEP=1

CHECKS_RUN=0; CHECKS_RED=0; CUR=""; CUR_RED=0; CUR_TITLE=""
begin() { CUR="$1"; CUR_TITLE="$2"; CUR_RED=0; echo "=== $1  $2 ==="; }
sub() { # <label> <cond> [detail]
    if eval "$2"; then
        printf '      ok   %s\n' "$1"
    else
        printf '      FAIL  %s\n' "$1"
        [ -n "${3:-}" ] && printf '           %s\n' "$(printf '%s' "$3" | head -c 600 | tr '\n' ' ')"
        CUR_RED=$((CUR_RED + 1))
    fi
}
verdict() {
    CHECKS_RUN=$((CHECKS_RUN + 1))
    if [ "$CUR_RED" -eq 0 ]; then printf '  PASS  %s  %s\n' "$CUR" "$CUR_TITLE"
    else printf '  FAIL  %s  %s  (%d sub-assertion(s) red)\n' "$CUR" "$CUR_TITLE" "$CUR_RED"
         CHECKS_RED=$((CHECKS_RED + 1)); fi
    echo ""
}

# --- the sandbox ------------------------------------------------------------
T="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/ws-stopped.XXXXXX")" && pwd -P)"
SESS_PIDS=()
cleanup() {
    for p in "${SESS_PIDS[@]+"${SESS_PIDS[@]}"}"; do kill "$p" 2>/dev/null || true; done
    if [ "$KEEP" -eq 1 ]; then echo "sandbox kept: $T"; else chmod -R u+w "$T" 2>/dev/null || true; rm -rf "$T"; fi
}
trap cleanup EXIT

export HOME="$T/home" CLAUDE_CONFIG_DIR="$T/home/.claude"
mkdir -p "$CLAUDE_CONFIG_DIR"
# The land's scratch sweep counts every allocation under $TMPDIR/richos-scratch
# and names the landed agent when that count is not zero, so the host's own
# allocations made S1.4 depend on what else had run on this Mac (2026-10-14).
export TMPDIR="$T/tmp"
mkdir -p "$TMPDIR"
export GIT_CONFIG_GLOBAL="$T/home/.gitconfig"
printf '[user]\n\tname = stopped\n\temail = stopped@example.invalid\n[init]\n\tdefaultBranch = main\n' > "$GIT_CONFIG_GLOBAL"
# The bulk root a proof run exports points the sweep at the host's real scratch
# (S1.4 then counted every agent's directory there), so it goes too (2026-10-23).
unset RICHOS_WORKSPACES_DIR RICHOS_SESSION_ID CLAUDE_PROJECT_DIR RICHOS_ENGINE_ROOT CLAUDE_PLUGIN_ROOT RICHOS_SESSION_PID RICHOS_PROJECTS_DIR \
      RICHOS_BULK_ROOT RICHOS_MACHINE_SETTINGS_RESOLVED 2>/dev/null || true
export SEAL_WAIT_SECONDS=0 RICHOS_WORKSPACES_SPAWN_WINDOW=0 RICHOS_WORKSPACES_STOP_GRACE=1 RICHOS_WORKSPACES_RETRY_BASE=0
STORE="$CLAUDE_CONFIG_DIR/state/workspaces"
WS="$ENGINE/mega-lander/workspaces.sh"
CREATE="$ENGINE/mega-lander/create-teammate-worktree.sh"

new_repo() {
    mkdir -p "$1" && git init -q -b main "$1"
    printf 'x\n' > "$1/README"
    [ "${2:-}" = adopt ] && cp "$ENGINE/orchestration.config" "$1/orchestration.config"
    printf '.claude/\n.env\n' > "$1/.gitignore"
    git -C "$1" add -A && git -C "$1" commit -q -m init
}
ENT="$T/entity";  new_repo "$ENT" adopt
OTHER="$T/other"; new_repo "$OTHER"
export RICHOS_ENTITY_ROOT="$ENT"

payload() { python3 -c 'import json,sys; d=json.loads(sys.argv[2]); d["hook_event_name"]=sys.argv[1]; print(json.dumps(d))' "$1" "$2"; }
ws() { RICHOS_SESSION_ID="$CUR_SID" "$WS" "$@"; }
start_session() {
    local pid; pid="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
    SESS_PIDS+=("$pid"); export RICHOS_SESSION_PID="$pid"; CUR_SID="$1"
    payload SessionStart "{\"session_id\":\"$1\",\"cwd\":\"$ENT\"}" | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>&1
}
spawn() { # <name> <prompt> -> rc of the spawn guard
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[4],"tool_input":{"name":sys.argv[2],"subagent_type":"zach","isolation":"worktree","prompt":sys.argv[3]}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/guard-worktree-isolation.sh" >/dev/null 2>"$T/spawn.err"
}
platform_spawn() { # <name> <agent-id>: the native workspace, SubagentStart, and the Agent result
    local name="$1" aid="$2" np="$ENT/.claude/worktrees/agent-$2"
    git -C "$ENT" worktree add -q "$np" -b "worktree-agent-$aid"
    payload SubagentStart "{\"session_id\":\"$CUR_SID\",\"agent_id\":\"$aid\",\"agent_type\":\"zach\",\"cwd\":\"$np\"}" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[4],"tool_input":{"name":sys.argv[2],"isolation":"worktree"},"tool_response":{"agentId":sys.argv[3],"status":"async_launched"}}))' "$CUR_SID" "$name" "$aid" "$ENT" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
}
# THE KILL, exactly as the platform leaves it: the per-agent record beside the
# agent's transcript gains `stoppedByUser`, and NO hook is delivered.
platform_stops() { # <name> <agent-id>
    local d="$CLAUDE_CONFIG_DIR/projects/-tmp-entity/$CUR_SID/subagents"
    mkdir -p "$d"
    python3 -c 'import json,sys; json.dump({"agentType":"zach","description":"d","name":sys.argv[2],"spawnDepth":1,"model":"opus","stoppedByUser":True}, open(sys.argv[1],"w"))' \
        "$d/agent-$2.meta.json" "$1"
}
barrier() { # <agent-id> <tool> -> rc of the point-9 lock-out; message in $T/barrier.err
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"agent_type":"zach","tool_name":sys.argv[3],"cwd":sys.argv[4],"tool_input":{}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/guard-sealed-worktree.sh" >/dev/null 2>"$T/barrier.err"
}
resume_guard() { # <to> <text> -> rc of guard-resume-isolation.sh; message in $T/resume.err
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"tool_name":"SendMessage","cwd":sys.argv[4],"tool_input":{"to":sys.argv[2],"message":sys.argv[3]}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/guard-resume-isolation.sh" >/dev/null 2>"$T/resume.err"
}
stop_gate() { # -> rc of the turn-end gate; stdout $T/stop.out, stderr $T/stop.err
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"Stop","session_id":sys.argv[1],"cwd":sys.argv[2],"stop_hook_active":False,"last_assistant_message":"done"}))' "$CUR_SID" "$ENT" \
        | bash "$HOOKS/guard-workspace-gate.sh" >"$T/stop.out" 2>"$T/stop.err"
}
commit_in() { printf 'work %s\n' "$2" > "$1/$2" && git -C "$1" add "$2" && git -C "$1" commit -q -m "work $2"; }
has_branch() { git -C "$1" rev-parse --verify --quiet "refs/heads/$2" >/dev/null; }
listed() { git -C "$1" worktree list --porcelain | grep -xF "worktree $2" >/dev/null; }
agent_rec() { ls "$STORE/agents" 2>/dev/null | grep -- "--$1.json" | head -1 | sed "s|^|$STORE/agents/|"; }
done_rec()  { ls "$STORE/done" 2>/dev/null | grep -- "--$1.json" | head -1 | sed "s|^|$STORE/done/|"; }
jget() { python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); cur=d
for k in sys.argv[2].split("."):
    cur = cur.get(k) if isinstance(cur, dict) else None
print("" if cur is None else (json.dumps(cur) if isinstance(cur,(dict,list)) else cur))' "$1" "$2" 2>/dev/null; }

start_session "sess-stopped-0001"
ws integration --repo "$ENT" --branch main --why "the stopped-ending suite, entity" >/dev/null 2>&1
ws integration --repo "$OTHER" --branch main --why "the stopped-ending suite, other" >/dev/null 2>&1

# ===========================================================================
begin S1 "An agent the platform STOPPED is finished automatically, and its workspaces and branches are deleted with nobody marking anything by hand (point 11 -> points 5, 4, 7, 10)"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-s1 >/dev/null 2>&1
CC1="$T/other-wt/zach-opus-s1"
spawn "zach-opus-s1" "$(printf 'read a page\ncross-repo-worktree: %s\n' "$CC1")"
platform_spawn "zach-opus-s1" "as1s1s1s1s1s1s1s1"
NAT1="$ENT/.claude/worktrees/agent-as1s1s1s1s1s1s1s1"
sub "S1.1 both of its workspaces exist and are registered before the stop (point 10)" \
    "[ -d '$CC1' ] && [ -d '$NAT1' ] && [ -n \"\$(agent_rec zach-opus-s1)\" ]" \
    "cc=$CC1 native=$NAT1 rec=$(agent_rec zach-opus-s1)"
sub "S1.2 while it is running it is not finished: nothing is pending and the turn may end" \
    "stop_gate" "$(cat "$T/stop.err" 2>/dev/null)"

# The CEO stops it. No SubagentStop, no TaskStop, no `workspaces.sh stop`.
platform_stops "zach-opus-s1" "as1s1s1s1s1s1s1s1"
STOP_MARKED="$(ls "$STORE"/agents/*zach-opus-s1.json 2>/dev/null | head -1)"
sub "S1.3 the kill itself writes NOTHING to the state store: the record still has no end" \
    "[ -z \"\$(jget '$STOP_MARKED' end.signal)\" ]" "end=$(jget "$STOP_MARKED" end.signal)"

stop_gate; GATE_RC=$?
sub "S1.4 the very next automatic moment (the turn-end gate) takes it: rc=0, nothing left pending" \
    "[ $GATE_RC -eq 0 ] && ! grep -q 'zach-opus-s1' '$T/stop.err'" "rc=$GATE_RC err=$(cat "$T/stop.err")"
sub "S1.5 its cc/ workspace and its native workspace are both GONE (points 4, 10)" \
    "[ ! -d '$CC1' ] && [ ! -d '$NAT1' ] && ! listed '$OTHER' '$CC1' && ! listed '$ENT' '$NAT1'" \
    "cc=$( [ -d "$CC1" ] && echo present || echo gone) native=$( [ -d "$NAT1" ] && echo present || echo gone)"
sub "S1.6 both branches are GONE (point 4)" \
    "! has_branch '$OTHER' cc/zach-opus-s1 && ! has_branch '$ENT' worktree-agent-as1s1s1s1s1s1s1s1" \
    "$(git -C "$OTHER" branch --format='%(refname:short)' | tr '\n' ' ')"
D1="$(done_rec zach-opus-s1)"
sub "S1.7 the recorded ending names the platform's own signal, not a person" \
    "[ \"\$(jget '$D1' end.signal)\" = stopped ] && [ \"\$(jget '$D1' end.source)\" = stoppedByUser ]" \
    "signal=$(jget "$D1" end.signal) source=$(jget "$D1" end.source) detail=$(jget "$D1" end.detail)"
sub "S1.8 an agent that produced nothing counts as landed (point 7)" \
    "[ \"\$(jget '$D1' disposition.kind)\" = landed ]" "kind=$(jget "$D1" disposition.kind)"
verdict

# ===========================================================================
begin S2 "A stopped agent whose work is NOT in the branch it integrates on is pending, not deleted: point 5 blocks the turn until Rich lands or discards it"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-s2 >/dev/null 2>&1
CC2="$T/other-wt/zach-opus-s2"
spawn "zach-opus-s2" "$(printf 'write a page\ncross-repo-worktree: %s\n' "$CC2")"
platform_spawn "zach-opus-s2" "as2s2s2s2s2s2s2s2"
commit_in "$CC2" note.txt
platform_stops "zach-opus-s2" "as2s2s2s2s2s2s2s2"
stop_gate; GATE_RC=$?
sub "S2.1 the gate REFUSES the turn and names it as finished work (point 5)" \
    "[ $GATE_RC -ne 0 ] && grep -q 'zach-opus-s2' '$T/stop.err'" "rc=$GATE_RC err=$(cat "$T/stop.err" | head -c 400)"
sub "S2.2 the reason given is the stop, quoted from the platform's record (point 11)" \
    "grep -qi 'stopped' '$T/stop.err'" "$(grep -i stopped "$T/stop.err" | head -2)"
sub "S2.3 its committed work is still there: nothing finished is discarded on its own (point 7)" \
    "[ -d '$CC2' ] && [ -f '$CC2/note.txt' ]" "cc2=$CC2"
ws discard zach-opus-s2 --reason "the stopped-ending suite: this work was not wanted" \
    --not-ceo-ordered "the suite spawned it, not the CEO" >"$T/discard.out" 2>&1; DRC=$?
sub "S2.4 Rich's discard is accepted for a stopped agent (it is finished), and deletes both workspaces" \
    "[ $DRC -eq 0 ] && [ ! -d '$CC2' ] && [ ! -d '$ENT/.claude/worktrees/agent-as2s2s2s2s2s2s2s2' ]" \
    "rc=$DRC out=$(cat "$T/discard.out")"
sub "S2.5 with it discarded the turn may end again" "stop_gate" "$(cat "$T/stop.err")"
verdict

# ===========================================================================
begin S3 "The flag is the whole of it: a running agent is never finished by this path, and a PAUSED agent that is stopped is (point 11)"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-s3 >/dev/null 2>&1
CC3="$T/other-wt/zach-opus-s3"
spawn "zach-opus-s3" "$(printf 'keep working\ncross-repo-worktree: %s\n' "$CC3")"
platform_spawn "zach-opus-s3" "as3s3s3s3s3s3s3s3"
# A per-agent record exists for it, WITHOUT the flag: the ordinary running case.
mkdir -p "$CLAUDE_CONFIG_DIR/projects/-tmp-entity/$CUR_SID/subagents"
python3 -c 'import json,sys; json.dump({"agentType":"zach","name":"zach-opus-s3","spawnDepth":1}, open(sys.argv[1],"w"))' \
    "$CLAUDE_CONFIG_DIR/projects/-tmp-entity/$CUR_SID/subagents/agent-as3s3s3s3s3s3s3s3.meta.json"
stop_gate; GATE_RC=$?
R3="$(agent_rec zach-opus-s3)"
sub "S3.1 a record with no stop flag leaves the agent running: no end, workspaces untouched" \
    "[ $GATE_RC -eq 0 ] && [ -z \"\$(jget '$R3' end.signal)\" ] && [ -d '$CC3' ]" \
    "rc=$GATE_RC end=$(jget "$R3" end.signal)"
ws pause zach-opus-s3 --until "the CEO's answer" >/dev/null 2>&1
sub "S3.2 Rich's pause holds while it is paused: still not finished, still not deleted" \
    "stop_gate && [ -d '$CC3' ]" "$(cat "$T/stop.err")"
platform_stops "zach-opus-s3" "as3s3s3s3s3s3s3s3"
stop_gate >/dev/null 2>&1
D3="$(done_rec zach-opus-s3)"
sub "S3.3 a pause ends when the agent is stopped: it is finished and its workspaces go (point 11)" \
    "[ -n '$D3' ] && [ \"\$(jget '$D3' end.signal)\" = stopped ] && [ ! -d '$CC3' ]" \
    "done=$D3 signal=$(jget "$D3" end.signal) cc3=$( [ -d "$CC3" ] && echo present || echo gone)"
verdict

# ===========================================================================
begin S4 "A stopped agent the platform RESTARTS is refused every tool before anything has looked at it (point 11 -> point 9)"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-s4 >/dev/null 2>&1
CC4="$T/other-wt/zach-opus-s4"
spawn "zach-opus-s4" "$(printf 'work then be stopped\ncross-repo-worktree: %s\n' "$CC4")"
platform_spawn "zach-opus-s4" "as4s4s4s4s4s4s4s4"
barrier "as4s4s4s4s4s4s4s4" Edit; R1=$?
sub "S4.1 before the stop it writes like any registered worker" "[ $R1 -eq 0 ]" "rc=$R1 $(cat "$T/barrier.err")"
# Stopped, and the FIRST thing that happens next is the platform restarting it:
# no gate, no command, nobody noticing.
platform_stops "zach-opus-s4" "as4s4s4s4s4s4s4s4"
barrier "as4s4s4s4s4s4s4s4" Edit; R2=$?
sub "S4.2 restarted after the stop, its very next tool call is REFUSED (point 9)" \
    "[ $R2 -ne 0 ] && grep -qi 'finished' '$T/barrier.err'" "rc=$R2 $(cat "$T/barrier.err" | head -c 300)"
sub "S4.3 the refusal names the stop as the reason" "grep -qi 'stopped' '$T/barrier.err'" \
    "$(cat "$T/barrier.err" | head -c 300)"
stop_gate >/dev/null 2>&1
sub "S4.4 and the ordinary path then deletes its workspaces" "[ ! -d '$CC4' ]" \
    "cc4=$( [ -d "$CC4" ] && echo present || echo gone)"
verdict

# ===========================================================================
begin S5 "A message to a stopped agent is refused rather than restarting it (point 11 -> point 9)"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-s5 >/dev/null 2>&1
CC5="$T/other-wt/zach-opus-s5"
spawn "zach-opus-s5" "$(printf 'be messaged\ncross-repo-worktree: %s\n' "$CC5")"
platform_spawn "zach-opus-s5" "as5s5s5s5s5s5s5s5"
resume_guard "zach-opus-s5" "carry on"; M1=$?
sub "S5.1 while it runs, a message to it is allowed" "[ $M1 -eq 0 ]" "rc=$M1 $(cat "$T/resume.err" | head -c 200)"
platform_stops "zach-opus-s5" "as5s5s5s5s5s5s5s5"
resume_guard "zach-opus-s5" "carry on"; M2=$?
sub "S5.2 once stopped, the same message is REFUSED as finished work" \
    "[ $M2 -ne 0 ] && grep -qi 'finished' '$T/resume.err'" "rc=$M2 $(cat "$T/resume.err" | head -c 300)"
verdict

echo "CHECKS RUN: $CHECKS_RUN  RED: $CHECKS_RED"
[ "$CHECKS_RED" -eq 0 ]
