#!/usr/bin/env bash
#
# workspaces-e2e.test.sh — THE CEO'S WORKSPACE SPEC, END TO END, THROUGH THE REAL HOOKS.
#
# docs/plans/worktree-spec-2026-09-11.md. Every step below goes through the
# hook scripts exactly as hooks/hooks.json registers them — the spawn guard,
# the lifecycle hook, the lock-out, the Stop gate — with the payload shapes the
# platform sends, in temporary repositories, with HOME and CLAUDE_CONFIG_DIR
# redirected. The operator's real files are never read or written.
#
# The only simulated party is the platform itself: this script creates the
# native workspace where Claude Code would (`.claude/worktrees/agent-<id>` on
# `worktree-agent-<id>`) and sends the events it would send. A session is a real
# process (`sleep`) whose number and start time the registry records.
#
#   E1  spawn -> register -> commit -> finish -> land -> every workspace and branch gone
#   E2  the same, discarded
#   E3  a cross-repository agent with two workspaces, landed: both gone, as one
#   E7  ONE agent with workspaces in TWO other repositories: all three of its
#       workspaces and branches go together (point 10's "every workspace it has")
#   E4  a session that ends mid-work: the next session is told first, starts new
#       work, is told at each turn end (never refused it), and lands it
#   E6  the pair of events: a branch the agent creates inside one tool call is
#       recorded against it by the catch-all PreToolUse + PostToolUse hooks, and
#       is deleted with its workspace when the work lands
#
# Usage: workspaces-e2e.test.sh [--keep]   (--keep leaves the sandbox for inspection)

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/../.." && pwd)"
HOOKS="$ENGINE/scripts/hooks"
KEEP=0; [ "${1:-}" = "--keep" ] && KEEP=1

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; FAIL=$((FAIL + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1" "${3:-}"; fi; }

T="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/ws-e2e.XXXXXX")" && pwd -P)"
SESS_PIDS=()
cleanup() {
    for p in "${SESS_PIDS[@]+"${SESS_PIDS[@]}"}"; do kill "$p" 2>/dev/null || true; done
    if [ "$KEEP" -eq 1 ]; then echo "sandbox kept: $T"; else rm -rf "$T"; fi
}
trap cleanup EXIT

export HOME="$T/home" CLAUDE_CONFIG_DIR="$T/home/.claude"
mkdir -p "$CLAUDE_CONFIG_DIR"
export GIT_CONFIG_GLOBAL="$T/home/.gitconfig"
printf '[user]\n\tname = e2e\n\temail = e2e@example.invalid\n[init]\n\tdefaultBranch = main\n' > "$GIT_CONFIG_GLOBAL"
unset RICHOS_WORKSPACES_DIR RICHOS_SESSION_ID CLAUDE_PROJECT_DIR RICHOS_ENGINE_ROOT CLAUDE_PLUGIN_ROOT 2>/dev/null || true
export SEAL_WAIT_SECONDS=0 RICHOS_WORKSPACES_SPAWN_WINDOW=0 RICHOS_WORKSPACES_STOP_GRACE=1

new_repo() { # <path> [adopt]
    mkdir -p "$1" && git init -q -b main "$1"
    printf 'x\n' > "$1/README"
    [ "${2:-}" = adopt ] && cp "$ENGINE/orchestration.config" "$1/orchestration.config"
    printf '.claude/\n' > "$1/.gitignore"
    git -C "$1" add -A && git -C "$1" commit -q -m init
}
ENT="$T/entity";   new_repo "$ENT" adopt     # the session's repository
OTHER="$T/other";  new_repo "$OTHER"         # the repository a cross-repo agent edits
export RICHOS_ENTITY_ROOT="$ENT"

start_session() { # <session-id> -> starts a real session process, records SessionStart
    local pid
    pid="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"   # not this shell's job: no job-control noise
    SESS_PIDS+=("$pid")
    export RICHOS_SESSION_PID="$pid"
    CUR_SID="$1"
    payload SessionStart "{\"session_id\":\"$1\",\"cwd\":\"$ENT\"}" | bash "$HOOKS/workspace-lifecycle.sh" > "$T/last-start.json" 2>"$T/last-start.err"
}
payload() { # <event> <json-object> -> the object with hook_event_name
    python3 -c 'import json,sys; d=json.loads(sys.argv[2]); d["hook_event_name"]=sys.argv[1]; print(json.dumps(d))' "$1" "$2"
}
spawn_payload() { # <name> <prompt>
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[4],"tool_input":{"name":sys.argv[2],"subagent_type":"zach","isolation":"worktree","prompt":sys.argv[3]}}))' "$CUR_SID" "$1" "$2" "$ENT"
}
# The platform: a native workspace, its SubagentStart, and the Agent result.
platform_spawn() { # <name> <agent-id>
    local name="$1" aid="$2" np="$ENT/.claude/worktrees/agent-$2"
    git -C "$ENT" worktree add -q "$np" -b "worktree-agent-$aid"
    payload SubagentStart "{\"session_id\":\"$CUR_SID\",\"agent_id\":\"$aid\",\"agent_type\":\"zach\",\"cwd\":\"$np\"}" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[4],"tool_input":{"name":sys.argv[2],"isolation":"worktree"},"tool_response":{"agentId":sys.argv[3],"status":"async_launched"}}))' "$CUR_SID" "$name" "$aid" "$ENT" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
}
subagent_stop() { # <agent-id>
    payload SubagentStop "{\"session_id\":\"$CUR_SID\",\"agent_id\":\"$1\",\"cwd\":\"$ENT\"}" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
}
barrier() { # <agent-id> <tool> -> exit code of the lock-out
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"agent_type":"zach","tool_name":sys.argv[3],"cwd":sys.argv[4],"tool_input":{}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/guard-sealed-worktree.sh" >/dev/null 2>"$T/barrier.err"
}
post_tool() { # <agent-id> <tool> -> the catch-all PostToolUse: the other half of the pair
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"agent_type":"zach","tool_name":sys.argv[3],"cwd":sys.argv[4],"tool_input":{},"tool_response":{}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/observe-created-refs.sh" >/dev/null 2>"$T/observe.err"
}
stop_gate() { # -> exit code of the Stop gate; the lead's notice in $T/stop.out, errors in $T/stop.err
    payload Stop "{\"session_id\":\"$CUR_SID\",\"cwd\":\"$ENT\",\"stop_hook_active\":false,\"last_assistant_message\":\"done\"}" \
        | bash "$HOOKS/guard-workspace-gate.sh" >"$T/stop.out" 2>"$T/stop.err"
}
spawn() { # <name> <prompt> -> exit code of the spawn guard
    spawn_payload "$1" "$2" | bash "$HOOKS/guard-worktree-isolation.sh" >/dev/null 2>"$T/spawn.err"
}
commit_in() { # <path> <file>
    printf 'work %s\n' "$2" > "$1/$2" && git -C "$1" add "$2" && git -C "$1" commit -q -m "work $2"
}
has_branch() { git -C "$1" rev-parse --verify --quiet "refs/heads/$2" >/dev/null; }
listed() { local l; l="$(git -C "$1" worktree list --porcelain)"; printf "%s\n" "$l" | grep -xF "worktree $2" >/dev/null; }
WS="$ENGINE/mega-lander/workspaces.sh"

echo "=== E1 spawn -> register -> commit -> finish -> land -> gone ==="
start_session "sess-e2e-one-1111"
# Point 14: "The branch a body of work integrates on is RECORDED when that work
# starts, before its first agent is spawned. Nothing infers it and nothing
# guesses it." Nothing derives it any more -- no floor, no reading of whatever
# the main checkout happens to be on -- so Rich records it, once per repository,
# and that is the only fact a land is proved against.
RICHOS_SESSION_ID="$CUR_SID" "$WS" integration --repo "$ENT" --branch main \
    --why "the end-to-end run's body of work" >"$T/integration.out" 2>&1
check "E0.1 the branch this work integrates on is recorded (point 14)" \
    "grep -q 'main' '$T/integration.out'" "$(cat "$T/integration.out")"
RICHOS_SESSION_ID="$CUR_SID" "$WS" integration --repo "$OTHER" --branch main \
    --why "the end-to-end run's body of work" >>"$T/integration.out" 2>&1
check "E1.1 SessionStart recorded the session with its process identity" \
    "grep -q '\"pid_start\"' '$CLAUDE_CONFIG_DIR/state/workspaces/sessions/sess-e2e-one-1111.json'"
spawn "zach-opus-e1" "build it"; rc=$?
check "E1.2 the spawn guard registered the spawn (exit 0)" "[ $rc -eq 0 ]" "$(cat "$T/spawn.err")"
platform_spawn "zach-opus-e1" "ae1e1e1e1e1e1e1e1"
NP1="$ENT/.claude/worktrees/agent-ae1e1e1e1e1e1e1e1"
barrier "ae1e1e1e1e1e1e1e1" Edit; rc=$?
check "E1.3 the registered worker may write (lock-out passes)" "[ $rc -eq 0 ]" "$(cat "$T/barrier.err")"
commit_in "$NP1" e1.txt
subagent_stop "ae1e1e1e1e1e1e1e1"
barrier "ae1e1e1e1e1e1e1e1" Read; rc=$?
check "E1.4 finished: the lock-out refuses every tool, Read included (point 9)" "[ $rc -eq 2 ]"
stop_gate; rc=$?
check "E1.5 while it is pending the turn ends and the lead is told, by name (R2)" \
    "[ $rc -eq 0 ] && grep -q 'zach-opus-e1' '$T/stop.out'" "rc=$rc $(cat "$T/stop.out" "$T/stop.err")"
spawn "zach-opus-e1b" "unrelated new work"; rc=$?
check "E1.6 unrelated work starts while finished work stays pending (point 5, amended 2026-09-30)" \
    "[ $rc -eq 0 ] && [ -e '$NP1' ]" "rc=$rc $(cat "$T/spawn.err")"
git -C "$ENT" merge -q --no-edit "worktree-agent-ae1e1e1e1e1e1e1e1"
stop_gate; rc=$?
check "E1.7 merged: the Stop gate lands it on its own and lets the turn end (point 4)" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"
check "E1.8 its workspace is gone from disk and from git" "[ ! -e '$NP1' ] && ! listed '$ENT' '$NP1'"
check "E1.9 its branch is gone" "! has_branch '$ENT' worktree-agent-ae1e1e1e1e1e1e1e1"
barrier "ae1e1e1e1e1e1e1e1" Write; rc=$?
check "E1.10 restarted after its workspace is gone: still refused (point 9)" "[ $rc -eq 2 ]"

echo "=== E2 the same, discarded ==="
spawn "zach-opus-e2" "try something"; rc=$?
check "E2.1 spawn registered" "[ $rc -eq 0 ]" "$(cat "$T/spawn.err")"
platform_spawn "zach-opus-e2" "ae2e2e2e2e2e2e2e2"
NP2="$ENT/.claude/worktrees/agent-ae2e2e2e2e2e2e2e2"
commit_in "$NP2" e2.txt
subagent_stop "ae2e2e2e2e2e2e2e2"
stop_gate; rc=$?
check "E2.2 pending: the turn ends and the lead is told" "[ $rc -eq 0 ] && grep -q 'zach-opus-e2' '$T/stop.out'" "rc=$rc $(cat "$T/stop.out")"
RICHOS_SESSION_ID="$CUR_SID" "$WS" discard zach-opus-e2 --reason "the reviewer rejected the approach" \
    --not-ceo-ordered "an internal experiment, not the CEO's order" >"$T/discard.out" 2>&1; rc=$?
check "E2.3 discarded with its reason recorded" "[ $rc -eq 0 ]" "$(cat "$T/discard.out")"
check "E2.4 its workspace and branch are gone" "[ ! -e '$NP2' ] && ! has_branch '$ENT' worktree-agent-ae2e2e2e2e2e2e2e2"
MAINLOG="$(git -C "$ENT" log --format=%s)"
check "E2.5 the discarded commit was never merged" "! printf '%s\\n' \"\$MAINLOG\" | grep -q 'work e2.txt'"
check "E2.6 the reason is in the record" "grep -rq 'the reviewer rejected the approach' '$CLAUDE_CONFIG_DIR/state/workspaces/done'"
stop_gate; rc=$?
check "E2.7 nothing pending: the turn may end" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"

echo "=== E3 a cross-repository agent with two workspaces ==="
RICHOS_SESSION_ID="$CUR_SID" bash "$ENGINE/mega-lander/create-teammate-worktree.sh" "$OTHER" zach-opus-e3 >"$T/create.out" 2>&1; rc=$?
CC3="$T/other-wt/zach-opus-e3"
check "E3.1 create-teammate-worktree.sh registered and created a cc/ workspace (point 1)" \
    "[ $rc -eq 0 ] && has_branch '$OTHER' cc/zach-opus-e3 && [ -d '$CC3' ]" "$(cat "$T/create.out")"
spawn "zach-opus-e3" "$(printf 'edit the other repository\ncross-repo-worktree: %s\n' "$CC3")"; rc=$?
check "E3.2 the spawn naming its cc/ workspace is registered" "[ $rc -eq 0 ]" "$(cat "$T/spawn.err")"
platform_spawn "zach-opus-e3" "ae3e3e3e3e3e3e3e3"
NP3="$ENT/.claude/worktrees/agent-ae3e3e3e3e3e3e3e3"
commit_in "$CC3" e3.txt
subagent_stop "ae3e3e3e3e3e3e3e3"
git -C "$OTHER" merge -q --no-edit cc/zach-opus-e3
stop_gate; rc=$?
check "E3.3 merged: landed on its own" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"
check "E3.4 BOTH workspaces are gone (point 10)" "[ ! -e '$CC3' ] && [ ! -e '$NP3' ] && ! listed '$OTHER' '$CC3' && ! listed '$ENT' '$NP3'"
check "E3.5 BOTH branches are gone (point 10)" "! has_branch '$OTHER' cc/zach-opus-e3 && ! has_branch '$ENT' worktree-agent-ae3e3e3e3e3e3e3e3"
git -C "$OTHER" worktree add -q "$T/rogue" -b cc/rogue      # made raw: registered by nothing
commit_in "$T/rogue" rogue.txt                                 # ...with work in it, so it is not landed by itself
spawn "zach-opus-e3x" "$(printf 'x\ncross-repo-worktree: %s/rogue\n' "$T")"; rc=$?
check "E3.6 a spawn naming an unregistered workspace does not happen (point 3)" \
    "[ $rc -eq 2 ] && grep -q 'could not be registered' '$T/spawn.err'" "$(cat "$T/spawn.err")"
stop_gate; rc=$?
check "E3.7 an unregistered cc/ workspace is finished work the lead is told about (point 3)" \
    "[ $rc -eq 0 ] && grep -q 'orphan-rogue' '$T/stop.out'" "rc=$rc $(cat "$T/stop.out" "$T/stop.err")"
RICHOS_SESSION_ID="$CUR_SID" "$WS" discard orphan-rogue --reason "made by hand, never registered" \
    --not-ceo-ordered "a test fixture made by this demonstration" >"$T/discard.out" 2>&1; rc=$?
check "E3.8 discarded: workspace and branch gone" \
    "[ $rc -eq 0 ] && [ ! -e '$T/rogue' ] && ! has_branch '$OTHER' cc/rogue" "$(cat "$T/discard.out")"

echo "=== E5 ONE agent with workspaces in TWO other repositories ==="
# Point 10 is "every workspace and branch it has", and its sentence's "two" is
# the example, not the rule. Before 2026-09-17 a teammate needing a second
# repository was given a SECOND NAME — a second agent, landed separately, and
# the CEO found one of the strays himself.
THIRD="$T/third"; new_repo "$THIRD"
RICHOS_SESSION_ID="$CUR_SID" "$WS" integration --repo "$THIRD" --branch main \
    --why "the end-to-end run's body of work" >>"$T/integration.out" 2>&1
RICHOS_SESSION_ID="$CUR_SID" bash "$ENGINE/mega-lander/create-teammate-worktree.sh" "$OTHER" zach-opus-e7 >"$T/create7a.out" 2>&1; rc1=$?
RICHOS_SESSION_ID="$CUR_SID" bash "$ENGINE/mega-lander/create-teammate-worktree.sh" "$THIRD" zach-opus-e7 >"$T/create7b.out" 2>&1; rc2=$?
CC7A="$T/other-wt/zach-opus-e7"
CC7B="$T/third-wt/zach-opus-e7"
check "E7.1 two repositories, ONE name: both cc/ workspaces are created and registered (points 1, 3)" \
    "[ $rc1 -eq 0 ] && [ $rc2 -eq 0 ] && [ -d '$CC7A' ] && [ -d '$CC7B' ] && has_branch '$OTHER' cc/zach-opus-e7 && has_branch '$THIRD' cc/zach-opus-e7" \
    "$(cat "$T/create7a.out" "$T/create7b.out")"
REC7="$CLAUDE_CONFIG_DIR/state/workspaces/agents/$CUR_SID--zach-opus-e7.json"
check "E7.2 and both are on the ONE agent's record" \
    "[ \"\$(python3 -c 'import json,sys; print(len([w for w in json.load(open(sys.argv[1]))[\"workspaces\"] if not w.get(\"deleted_at\")]))' '$REC7')\" = 2 ]" \
    "$(cat "$REC7" 2>/dev/null | tr '\n' ' ' | cut -c1-300)"
spawn "zach-opus-e7" "$(printf 'edit both repositories\ncross-repo-worktree: %s\ncross-repo-worktree: %s\n' "$CC7A" "$CC7B")"; rc=$?
check "E7.3 a spawn naming BOTH registered workspaces is registered (clause 4c reads every marker line)" \
    "[ $rc -eq 0 ]" "$(cat "$T/spawn.err")"
# NEGATIVE, with its first line REGISTERED so the refusal can only be about the
# second: one real workspace for this name, one made by hand in another
# repository. Nothing about a payload's FIRST marker line excuses its second.
RICHOS_SESSION_ID="$CUR_SID" bash "$ENGINE/mega-lander/create-teammate-worktree.sh" "$OTHER" zach-opus-e7x >"$T/create7x.out" 2>&1; rc=$?
CC7X="$T/other-wt/zach-opus-e7x"
check "E7.4a POSITIVE CONTROL: the first of the two workspaces IS registered and created" \
    "[ $rc -eq 0 ] && [ -d '$CC7X' ]" "$(cat "$T/create7x.out")"
git -C "$THIRD" worktree add -q "$T/rogue2" -b cc/rogue2      # made raw: registered by nothing
commit_in "$T/rogue2" rogue2.txt                              # ...with work in it, so it is not landed by itself
spawn "zach-opus-e7x" "$(printf 'x\ncross-repo-worktree: %s\ncross-repo-worktree: %s\n' "$CC7X" "$T/rogue2")"; rc=$?
check "E7.4 NEGATIVE: a payload whose SECOND marker line names an unregistered workspace does not happen, and the refusal names THAT path (point 3)" \
    "[ $rc -ne 0 ] && grep -q 'rogue2' '$T/spawn.err'" "$(cat "$T/spawn.err")"
RICHOS_SESSION_ID="$CUR_SID" "$WS" withdraw-cc --name zach-opus-e7x \
    --why "its spawn was refused for the workspace it did not register" >"$T/withdraw7.out" 2>&1; rc=$?
check "E7.4b the registered half of that refused spawn is withdrawn: nothing left behind (point 3)" \
    "[ $rc -eq 0 ] && [ ! -e '$CC7X' ] && ! has_branch '$OTHER' cc/zach-opus-e7x" "$(cat "$T/withdraw7.out")"
stop_gate; rc=$?
check "E7.4c the hand-made one is finished work of an ended session the lead is told about (point 3)" \
    "[ $rc -eq 0 ] && grep -q 'orphan-rogue2' '$T/stop.out'" "rc=$rc $(cat "$T/stop.out" "$T/stop.err")"
RICHOS_SESSION_ID="$CUR_SID" "$WS" discard orphan-rogue2 --reason "made by hand for the negative case, never registered" \
    --not-ceo-ordered "a test fixture made by this demonstration" >"$T/discard7.out" 2>&1; rc=$?
check "E7.4d and it is discarded: workspace and branch gone" \
    "[ $rc -eq 0 ] && [ ! -e '$T/rogue2' ] && ! has_branch '$THIRD' cc/rogue2" "$(cat "$T/discard7.out")"
platform_spawn "zach-opus-e7" "ae7e7e7e7e7e7e7e7"
NP7="$ENT/.claude/worktrees/agent-ae7e7e7e7e7e7e7e7"
commit_in "$CC7A" e7a.txt; commit_in "$CC7B" e7b.txt; commit_in "$NP7" e7n.txt
subagent_stop "ae7e7e7e7e7e7e7e7"
git -C "$OTHER" merge -q --no-edit cc/zach-opus-e7
git -C "$THIRD" merge -q --no-edit cc/zach-opus-e7
git -C "$ENT" merge -q --no-edit "worktree-agent-ae7e7e7e7e7e7e7e7"
stop_gate; rc=$?
check "E7.5 landed as ONE (exit $rc)" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"
check "E7.6 ALL THREE workspaces are gone — none is left behind (point 10)" \
    "[ ! -e '$CC7A' ] && [ ! -e '$CC7B' ] && [ ! -e '$NP7' ] && ! listed '$OTHER' '$CC7A' && ! listed '$THIRD' '$CC7B' && ! listed '$ENT' '$NP7'"
check "E7.7 and all three branches are gone, in all three repositories (point 10)" \
    "! has_branch '$OTHER' cc/zach-opus-e7 && ! has_branch '$THIRD' cc/zach-opus-e7 && ! has_branch '$ENT' worktree-agent-ae7e7e7e7e7e7e7e7"

echo "=== E4 a session that ends mid-work ==="
spawn "zach-opus-e4" "long work"; rc=$?
check "E4.1 spawn registered" "[ $rc -eq 0 ]" "$(cat "$T/spawn.err")"
platform_spawn "zach-opus-e4" "ae4e4e4e4e4e4e4e4"
NP4="$ENT/.claude/worktrees/agent-ae4e4e4e4e4e4e4e4"
commit_in "$NP4" e4.txt
printf 'half-written\n' > "$NP4/draft.txt"
OLD_PID="$RICHOS_SESSION_PID"
kill "$OLD_PID"
while kill -0 "$OLD_PID" 2>/dev/null; do sleep 0.1; done      # the process no longer exists
start_session "sess-e2e-two-2222"
check "E4.2 the next session is told first (point 5)" \
    "grep -q 'zach-opus-e4' '$T/last-start.json' && grep -q 'DUE WORK' '$T/last-start.json'" "$(cat "$T/last-start.json" "$T/last-start.err")"
barrier "ae4e4e4e4e4e4e4e4" Bash; rc=$?
check "E4.3 its agent cannot outlive the session: locked out (points 9, 12)" "[ $rc -eq 2 ]" "$(cat "$T/barrier.err")"
spawn "zach-opus-e4b" "unrelated"; rc=$?
check "E4.4 unrelated work starts while the previous session stays unresolved" "[ $rc -eq 0 ] && [ -e '$NP4/draft.txt' ]"
stop_gate; rc=$?
check "E4.5 its turn ends and the lead is told again" "[ $rc -eq 0 ] && grep -q 'zach-opus-e4' '$T/stop.out'" "rc=$rc $(cat "$T/stop.out")"
RICHOS_SESSION_ID="$CUR_SID" "$WS" land zach-opus-e4 >"$T/land.out" 2>&1; rc=$?
# Unmerged AND uncommitted: refused, and the file is kept. Which reason speaks
# first is not point 8's business: since 25b2b68ff (hunt part 4, finding 13)
# work that plainly is not merged is refused before anything is stopped, so the
# refusal names the merge, not the file.
check "E4.6a unmerged work with an uncommitted file is refused and the file is kept (points 7, 8)" \
    "[ $rc -eq 2 ] && [ -e '$NP4/draft.txt' ]" \
    "rc=$rc draft=$([ -e "$NP4/draft.txt" ] && echo kept || echo GONE) $(cat "$T/land.out")"
# POINT 8 ALONE: everything it committed is merged, so the uncommitted file is
# the ONLY thing between this work and its deletion. A land that ignored it
# would succeed here and delete draft.txt with the workspace.
git -C "$ENT" merge -q --no-edit "worktree-agent-ae4e4e4e4e4e4e4e4"
RICHOS_SESSION_ID="$CUR_SID" "$WS" land zach-opus-e4 >"$T/land.out" 2>&1; rc=$?
check "E4.6 an uncommitted file refuses the land (point 8)" "[ $rc -eq 2 ] && grep -q 'uncommitted' '$T/land.out' && [ -e '$NP4/draft.txt' ]" \
    "rc=$rc draft=$([ -e "$NP4/draft.txt" ] && echo kept || echo GONE) $(cat "$T/land.out")"
git -C "$NP4" add draft.txt && git -C "$NP4" commit -q -m "what it left"
git -C "$ENT" merge -q --no-edit "worktree-agent-ae4e4e4e4e4e4e4e4"
RICHOS_SESSION_ID="$CUR_SID" "$WS" land zach-opus-e4 >"$T/land.out" 2>&1; rc=$?
check "E4.7 committed and merged: landed by the next session" "[ $rc -eq 0 ]" "$(cat "$T/land.out")"
check "E4.8 its workspace and branch are gone" "[ ! -e '$NP4' ] && ! has_branch '$ENT' worktree-agent-ae4e4e4e4e4e4e4e4"
MAINLOG="$(git -C "$ENT" log --format=%s)"
check "E4.9 nothing it produced was lost: both commits are in main" \
    "case \"\$MAINLOG\" in *'work e4.txt'*) case \"\$MAINLOG\" in *'what it left'*) true ;; *) false ;; esac ;; *) false ;; esac"
stop_gate; rc=$?
check "E4.10 nothing pending: the turn may end" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"

echo "=== E6 the pair of events: refs created inside one tool call ==="
spawn "zach-opus-e6" "make something"; rc=$?
check "E6.1 spawn registered" "[ $rc -eq 0 ]" "$(cat "$T/spawn.err")"
platform_spawn "zach-opus-e6" "ae6e6e6e6e6e6e6e6"
NP6="$ENT/.claude/worktrees/agent-ae6e6e6e6e6e6e6e6"
# ONE tool call: the lock-out opens it, the agent commits on a side branch,
# switches away and leaves a plain branch behind, and the catch-all PostToolUse
# closes it. Neither ref is checked out at the workspace when anything looks.
barrier "ae6e6e6e6e6e6e6e6" Bash
commit_in "$NP6" e6.txt
git -C "$NP6" checkout -q -b e6-side
commit_in "$NP6" e6-side.txt
git -C "$NP6" branch e6-spare
git -C "$NP6" checkout -q "worktree-agent-ae6e6e6e6e6e6e6e6"
post_tool "ae6e6e6e6e6e6e6e6" Bash
REC6="$CLAUDE_CONFIG_DIR/state/workspaces/agents/$(ls "$CLAUDE_CONFIG_DIR/state/workspaces/agents" | grep -- "-zach-opus-e6.json")"
check "E6.2 both refs it created are recorded against it (points 3, 10)" \
    "grep -q 'e6-side' '$REC6' && grep -q 'e6-spare' '$REC6'" "$(head -40 "$REC6" 2>/dev/null) $(cat "$T/observe.err")"
subagent_stop "ae6e6e6e6e6e6e6e6"
git -C "$ENT" merge -q --no-edit "worktree-agent-ae6e6e6e6e6e6e6e6"   # its OWN branch is in
RICHOS_SESSION_ID="$CUR_SID" "$WS" land zach-opus-e6 >"$T/land6.out" 2>&1; rc=$?
check "E6.3 the land is held by the work it left on a side branch, by name (points 5, 8)" \
    "[ $rc -eq 2 ] && grep -q 'e6-side' '$T/land6.out'" "$(cat "$T/land6.out")"
stop_gate; rc=$?
check "E6.4 so it stays due and the lead is told at the turn end (point 5)" \
    "[ $rc -eq 0 ] && grep -q 'zach-opus-e6' '$T/stop.out'" "rc=$rc $(cat "$T/stop.out" "$T/stop.err")"
git -C "$ENT" merge -q --no-edit e6-side
stop_gate; rc=$?
check "E6.5 merged: nothing it produced was lost, and it lands on its own" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"
MAINLOG="$(git -C "$ENT" log --format=%s)"
check "E6.6 the side-branch commit is in main" "case \"\$MAINLOG\" in *'work e6-side.txt'*) true ;; *) false ;; esac"
check "E6.7 every ref it created is gone with its workspace (point 10)" \
    "! has_branch '$ENT' e6-side && ! has_branch '$ENT' e6-spare && [ ! -e '$NP6' ]"

echo "=== every workspace and branch the agents had ==="
check "E5.1 the entity repository has only its main checkout" "[ \"\$(git -C '$ENT' worktree list --porcelain | grep -c '^worktree ')\" = 1 ]"
check "E5.2 the other repository has only its main checkout" "[ \"\$(git -C '$OTHER' worktree list --porcelain | grep -c '^worktree ')\" = 1 ]"
check "E5.2b the third repository has only its main checkout" "[ \"\$(git -C '$THIRD' worktree list --porcelain | grep -c '^worktree ')\" = 1 ]"
check "E5.3 no agent branch survives anywhere" \
    "[ -z \"\$(git -C '$ENT' for-each-ref refs/heads/worktree-agent-* refs/heads/cc; git -C '$OTHER' for-each-ref refs/heads/cc; git -C '$THIRD' for-each-ref refs/heads/cc)\" ]"
if [ -s "$T/hooks.err" ]; then
    bad "E5.4 the lifecycle hook reported nothing unexpected" "$(head -5 "$T/hooks.err")"
else
    ok "E5.4 the lifecycle hook reported nothing unexpected"
fi

echo ""
echo "workspaces-e2e: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
