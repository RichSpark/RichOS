#!/usr/bin/env bash
#
# workspace-spec-fourteen.test.sh — THE FOURTEEN CHECKS OF THE ROUND-6 BRIEF,
# FROZEN, ONE PER SENTENCE OF THE CEO'S PAGE, MEASURED THROUGH THE REAL HOOKS.
#
# The page:  docs/plans/worktree-spec-2026-09-11.md (richos-hq; mirrored here).
# The list:  richos-hq docs/plans/round6-brief-fourteen-points-2026-09-12.md §4,
#            frozen before the engineer started. A check may be ADDED here. None
#            of the fourteen is removed or reworded; where one is believed wrong
#            it is implemented as written and the report says why.
#            Point 5 spawn assertions were superseded by the CEO on 2026-09-30:
#            pending integration no longer blocks unrelated development.
#
# Every step goes through the hook scripts exactly as hooks/hooks.json registers
# them — the spawn guard, the Bash worktree guard, the lifecycle hook, the
# lock-out, the catch-all PostToolUse, the Stop gate — with the payload shapes
# the platform sends, in temporary repositories, with HOME and CLAUDE_CONFIG_DIR
# redirected. The operator's real files are never read or written. The only
# simulated party is the platform itself.
#
# OUTPUT. One line per sub-assertion — `      ok   C4.2 ...` or
# `      FAIL  C4.2 ...` — one verdict line per check — `  PASS  C4  ...` /
# `  FAIL  C4  ...`, TWO spaces after the id so a harness grepping `FAIL  C1 `
# cannot match C10..C14 and `FAIL  C4 ` cannot match `FAIL  C4.2` — and the
# last two lines are the round's own headline, the CEO's fourteen and the
# harness's added self-check (C0) counted APART, since C0 tests no sentence
# of his page (round 7, brief §7):
#
#     CHECKS RUN: <N>  RED: <K>
#     FOURTEEN: <N> green, <K> red · self-check: <green|red>
#
# A check is RED when any of its sub-assertions is red. Exit 0 only when K = 0.
#
# THE MUTATION HARNESS NAMES SUB-ASSERTIONS, NEVER CHECKS. One check (C14) was
# red on the base round 6 was written against, so a mutant wanting `FAIL  C14 `
# would have been "proven" by a suite that was red before the mutant touched
# anything — absence reading as success, which is failure type A of the
# 2026-09-10 record. workspace-spec-fourteen.mutation.sh therefore wants
# `FAIL  C14.3 `, a line that is green until the mutation makes it red.
#
# ROUND 7 (2026-09-12) ADDED SUB-ASSERTIONS UNDER THE FROZEN HEADINGS — none
# removed, none reworded — for every clause both round-6 reviewers found the
# fourteen did not ask, and for what a census of every sentence then found:
# C2.8, C3.8–C3.9, C4.4 (asks git's registry and ref list, not two directory
# names), C5.10–C5.13, C7.6–C7.9, C8.6–C8.7, C10.5–C10.6, C11.7, C12.7–C12.11,
# C14.8–C14.12. Each has its mutant in the harness.
#
# ROUND 8 (2026-09-13) ADDED MORE, still under the frozen headings, for the eight
# items of its brief: C5.13 now asserts BOTH sentences of point 5's last
# paragraph (the page settles it — "New work stays blocked either way" names
# the CEO's word in its own parenthesis, so nothing was his to decide), C5.14
# (a second item lands while the first waits on him), C5.15–C5.16 (a person's
# turn is decided on the fields the platform STAMPS, measured on this machine's
# transcripts, never on the shape of the text), C8.8 (same name, same SIZE,
# different bytes), C9.6 (a process that ignores TERM), C11.8 (handed in and
# still running), C10.7 (a ref created after the agent's last PostToolUse),
# C2.9–C2.12 (every deleter and mover of a codex/ ref, a write inside a codex/
# workspace, and a codex/ ref moved by an unnamed verb is restored) and
# C14.13–C14.16 (a RECORDED branch moved by any verb, named or unnamed, from
# either checkout, is restored and reported; the lead's move is not).
#
# Usage: workspace-spec-fourteen.test.sh [--keep]   (--keep leaves the sandbox)

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/../.." && pwd)"
HOOKS="$ENGINE/scripts/hooks"
KEEP=0; [ "${1:-}" = "--keep" ] && KEEP=1

# --- the tally --------------------------------------------------------------
CHECKS_RUN=0; CHECKS_RED=0
FOURTEEN_RUN=0; FOURTEEN_RED=0; SELF_RED=0      # the CEO's fourteen, and C0, apart
CUR=""; CUR_RED=0; CUR_TITLE=""
begin() { # <id> <title>
    CUR="$1"; CUR_TITLE="$2"; CUR_RED=0
    echo "=== $1  $2 ==="
}
sub() { # <label> <cond> [detail]   — one sub-assertion of the current check
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
    [ "$CUR" != C0 ] && FOURTEEN_RUN=$((FOURTEEN_RUN + 1))
    if [ "$CUR_RED" -eq 0 ]; then
        printf '  PASS  %s  %s\n' "$CUR" "$CUR_TITLE"
    else
        printf '  FAIL  %s  %s  (%d sub-assertion(s) red)\n' "$CUR" "$CUR_TITLE" "$CUR_RED"
        CHECKS_RED=$((CHECKS_RED + 1))
        if [ "$CUR" = C0 ]; then SELF_RED=$((SELF_RED + 1)); else FOURTEEN_RED=$((FOURTEEN_RED + 1)); fi
    fi
    echo ""
}

# --- the sandbox ------------------------------------------------------------
T="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/ws-fourteen.XXXXXX")" && pwd -P)"
SESS_PIDS=()
FIXTURE_PIDS=()
cleanup() {
    for p in "${SESS_PIDS[@]+"${SESS_PIDS[@]}"}"; do kill "$p" 2>/dev/null || true; done
    for p in "${FIXTURE_PIDS[@]+"${FIXTURE_PIDS[@]}"}"; do kill -9 "$p" 2>/dev/null || true; done
    [ -n "${HOLD_PID:-}" ] && kill -9 "$HOLD_PID" 2>/dev/null
    if [ "$KEEP" -eq 1 ]; then echo "sandbox kept: $T"; else
        chflags -R nouchg "$T" 2>/dev/null || true
        chmod -R u+w "$T" 2>/dev/null || true
        rm -rf "$T"
    fi
}
trap cleanup EXIT

export HOME="$T/home" CLAUDE_CONFIG_DIR="$T/home/.claude"
mkdir -p "$CLAUDE_CONFIG_DIR"
export GIT_CONFIG_GLOBAL="$T/home/.gitconfig"
printf '[user]\n\tname = fourteen\n\temail = fourteen@example.invalid\n[init]\n\tdefaultBranch = main\n' > "$GIT_CONFIG_GLOBAL"
unset RICHOS_WORKSPACES_DIR RICHOS_SESSION_ID CLAUDE_PROJECT_DIR RICHOS_ENGINE_ROOT CLAUDE_PLUGIN_ROOT RICHOS_SESSION_PID 2>/dev/null || true
export SEAL_WAIT_SECONDS=0 RICHOS_WORKSPACES_SPAWN_WINDOW=0 RICHOS_WORKSPACES_STOP_GRACE=1 RICHOS_WORKSPACES_RETRY_BASE=0
STORE="$CLAUDE_CONFIG_DIR/state/workspaces"

# --- process fixtures: read facts, never sleep (audit R8, 2026-09-29) --------
# C9 used to sleep 0.3 s twice and then assert, and a loaded Mac lost both bets.
#
# DEAD MEANS GONE OR A ZOMBIE. A process the land stopped has usually lost its
# parent already, so launchd reaps it whenever it gets round to it, and until then
# `kill -0` still succeeds. The product counts a zombie as stopped (workspaces.py
# _alive), and a land that finds a survivor fails, so C9 reads this ONCE, right
# after the land returns, with no wait: a mutant that leaves the holder running
# still reads alive at once.
is_dead() { # <pid>
    local stat
    kill -0 "$1" 2>/dev/null || return 0
    stat="$(ps -o stat= -p "$1" 2>/dev/null)" || return 0
    case "$stat" in ''|*Z*) return 0 ;; esac
    return 1
}
# A HOLDER THAT IGNORES TERM, SENT TERM ONLY ONCE IT DOES. Python needs time to
# start before it can install the handler, and under load that is longer than any
# sleep: a TERM that arrived first killed the holder and the positive control went
# red. The holder ignores TERM, sends itself one (delivered before its own kill()
# returns), and only then writes <ready>, so the file says it has survived a TERM.
# The wait has no clock: it ends when the file appears or the holder dies, and a
# hang is caught by the unit's own deadline in ci-shard.sh.
start_term_ignorer() { # <cwd> <ready-file>   -> prints the holder's pid once it is ready
    local pid
    pid="$(cd "$1" && sh -c 'python3 -c "import os,signal,sys,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); os.kill(os.getpid(), signal.SIGTERM); open(sys.argv[1], \"w\").close(); time.sleep(3600)" "$1" >/dev/null 2>&1 & echo $!' sh "$2")"
    while [ ! -e "$2" ] && kill -0 "$pid" 2>/dev/null; do sleep 0.05; done
    echo "$pid"
}

new_repo() { # <path> [adopt]
    mkdir -p "$1" && git init -q -b main "$1"
    printf 'x\n' > "$1/README"
    [ "${2:-}" = adopt ] && cp "$ENGINE/orchestration.config" "$1/orchestration.config"
    printf '.claude/\n.env\n' > "$1/.gitignore"
    git -C "$1" add -A && git -C "$1" commit -q -m init
}
ENT="$T/entity";   new_repo "$ENT" adopt     # the session's repository
OTHER="$T/other";  new_repo "$OTHER"         # a repository a cross-repo agent edits (integrates on main)
DEV="$T/devrepo";  new_repo "$DEV"           # a repository whose work integrates on a dev branch
export RICHOS_ENTITY_ROOT="$ENT"

start_session() { # <session-id> [cwd]
    local pid
    pid="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
    SESS_PIDS+=("$pid")
    export RICHOS_SESSION_PID="$pid"
    CUR_SID="$1"
    payload SessionStart "{\"session_id\":\"$1\",\"cwd\":\"${2:-$ENT}\"}" | bash "$HOOKS/workspace-lifecycle.sh" > "$T/last-start.json" 2>"$T/last-start.err"
}
payload() { # <event> <json-object>
    python3 -c 'import json,sys; d=json.loads(sys.argv[2]); d["hook_event_name"]=sys.argv[1]; print(json.dumps(d))' "$1" "$2"
}
spawn_payload() { # <name> <prompt>
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","cwd":sys.argv[4],"tool_input":{"name":sys.argv[2],"subagent_type":"zach","isolation":"worktree","prompt":sys.argv[3]}}))' "$CUR_SID" "$1" "$2" "$ENT"
}
spawn() { # <name> <prompt> -> rc of the spawn guard; stderr in $T/spawn.err
    spawn_payload "$1" "$2" | bash "$HOOKS/guard-worktree-isolation.sh" >/dev/null 2>"$T/spawn.err"
}
platform_spawn() { # <name> <agent-id>  — the platform: native workspace, SubagentStart, Agent result
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
send_message() { # <to> <text>  — PostToolUse[SendMessage] through the lifecycle hook
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"tool_name":"SendMessage","cwd":sys.argv[4],"tool_input":{"to":sys.argv[2],"message":sys.argv[3]},"tool_response":{}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
}
barrier() { # <agent-id> <tool> -> rc of the lock-out (empty agent id = the lead)
    python3 -c 'import json,sys; d={"hook_event_name":"PreToolUse","session_id":sys.argv[1],"agent_type":"zach","tool_name":sys.argv[3],"cwd":sys.argv[4],"tool_input":{}}
if sys.argv[2]: d["agent_id"]=sys.argv[2]
print(json.dumps(d))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/guard-sealed-worktree.sh" >/dev/null 2>"$T/barrier.err"
}
bash_guard() { # <command> -> rc of the Bash worktree guard; stderr in $T/bash.err
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"tool_name":"Bash","cwd":sys.argv[3],"tool_input":{"command":sys.argv[2]}}))' "$CUR_SID" "$1" "$ENT" \
        | bash "$HOOKS/guard-worktree-removal.sh" >/dev/null 2>"$T/bash.err"
}
agent_bash_guard() { # <agent-id> <command> -> the same guard, in an AGENT's call (the payload carries its id)
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"tool_name":"Bash","cwd":sys.argv[4],"tool_input":{"command":sys.argv[3]}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/guard-worktree-removal.sh" >/dev/null 2>"$T/bash.err"
}
barrier_path() { # <agent-id|""> <tool> <file_path> -> the lock-out on a writing tool aimed at a path (an empty agent id = the lead)
    python3 -c 'import json,sys; d={"hook_event_name":"PreToolUse","session_id":sys.argv[1],"agent_type":"zach","tool_name":sys.argv[3],"cwd":sys.argv[5],"tool_use_id":"tu-path-"+sys.argv[3],"tool_input":{"file_path":sys.argv[4],"old_string":"x","new_string":"y"}}
if sys.argv[2]: d["agent_id"]=sys.argv[2]
print(json.dumps(d))' "$CUR_SID" "$1" "$2" "$3" "$ENT" \
        | bash "$HOOKS/guard-sealed-worktree.sh" >/dev/null 2>"$T/barrier.err"
}
agent_call_pre() { # <agent-id> <tool_use_id> [tool] -> the catch-all PreToolUse (the lock-out) carrying this call's id: it opens the creation window (point 3)
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PreToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"tool_use_id":sys.argv[3],"tool_name":sys.argv[4],"cwd":sys.argv[5],"tool_input":{}}))' "$CUR_SID" "$1" "$2" "${3:-Bash}" "$ENT" \
        | bash "$HOOKS/guard-sealed-worktree.sh" >/dev/null 2>"$T/barrier.err"
}
agent_call_post() { # <agent-id> <tool_use_id> [tool] -> the catch-all PostToolUse (observe-created-refs.sh): the other half of the pair
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"tool_use_id":sys.argv[3],"tool_name":sys.argv[4],"cwd":sys.argv[5],"tool_input":{},"tool_response":{}}))' "$CUR_SID" "$1" "$2" "${3:-Bash}" "$ENT" \
        | bash "$HOOKS/observe-created-refs.sh" >/dev/null 2>>"$T/observe.err"
}
agent_call_post_bg() { # <agent-id> <tool_use_id> -> the PostToolUse of a Bash call the platform stamped run_in_background: its process outlives the call
    python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"agent_id":sys.argv[2],"tool_use_id":sys.argv[3],"tool_name":"Bash","cwd":sys.argv[4],"tool_input":{"command":"sleep 3 && git branch side/bg","run_in_background":True},"tool_response":{"status":"running"}}))' "$CUR_SID" "$1" "$2" "$ENT" \
        | bash "$HOOKS/observe-created-refs.sh" >/dev/null 2>>"$T/observe.err"
}
hold_shell_start() { # <agent-id> <tool_use_id> -> agent_hold's own record of that call's shell, made as its PreToolUse rewrite and the shell's own `mark` make it; the shell runs until hold_shell_end
    local lib="$ENGINE/scripts/lib" stem state pid
    stem="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import agent_hold as ah
r = ah._record({"session_id": sys.argv[2], "agent_id": sys.argv[3], "tool_use_id": sys.argv[4]}, "bg", "sleep 3 && git branch side/bg")
print(r[0] if r else "")' "$lib" "$CUR_SID" "$1" "$2")"
    state="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import agent_hold as ah; print(ah.state_dir())' "$lib")"
    [ -n "$stem" ] || return 1
    bash -c 'python3 "$1" mark --state "$2" "$3" && exec sleep 600' _ "$lib/agent_hold.py" "$state" "$stem" &
    pid=$!
    FIXTURE_PIDS+=("$pid")
    printf '%s\n' "$pid" > "$T/hold-shell-$2.pid"
    while [ ! -e "$stem.pid" ] && kill -0 "$pid" 2>/dev/null; do sleep 0.05; done   # a fact, not a clock
    [ -e "$stem.pid" ]
}
hold_shell_end() { # <tool_use_id> -> that call's process finishes: agent_hold's record then shows its shell gone
    local pid
    pid="$(cat "$T/hold-shell-$1.pid")"
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null || true                          # reaped, so no zombie stands in for it
}
task_completed() { # <agent-id> <name>  — TaskCompleted through the lifecycle hook: the agent handed in its work (point 11)
    payload TaskCompleted "{\"session_id\":\"$CUR_SID\",\"agent_id\":\"$1\",\"teammate_name\":\"$2\",\"cwd\":\"$ENT\"}" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
}
session_end() { # <session-id>  — SessionEnd through the lifecycle hook: the session records its end (point 12)
    payload SessionEnd "{\"session_id\":\"$1\",\"reason\":\"exit\",\"cwd\":\"$ENT\"}" \
        | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
}
refs_and_worktrees() { # <repo> -> one line per ref NAME and per registered worktree path: what git itself lists
    git -C "$1" for-each-ref --format='ref %(refname)'
    git -C "$1" worktree list --porcelain | grep '^worktree '
}
new_lines() { # <before> <after> -> the lines in <after> that were not in <before>
    comm -13 <(printf '%s\n' "$1" | sort) <(printf '%s\n' "$2" | sort)
}
stop_gate() { # [last_assistant_message] [transcript_path] -> rc; message in $T/stop.err, stdout in $T/stop.out
    python3 -c 'import json,sys; d={"hook_event_name":"Stop","session_id":sys.argv[1],"cwd":sys.argv[2],"stop_hook_active":False,"last_assistant_message":sys.argv[3] or "done"}
if sys.argv[4]: d["transcript_path"]=sys.argv[4]
print(json.dumps(d))' "$CUR_SID" "$ENT" "${1:-}" "${2:-}" \
        | bash "$HOOKS/guard-workspace-gate.sh" >"$T/stop.out" 2>"$T/stop.err"
}
commit_in() { # <path> <file>
    printf 'work %s\n' "$2" > "$1/$2" && git -C "$1" add "$2" && git -C "$1" commit -q -m "work $2"
}
has_branch() { git -C "$1" rev-parse --verify --quiet "refs/heads/$2" >/dev/null; }
listed() { local l; l="$(git -C "$1" worktree list --porcelain)"; printf "%s\n" "$l" | grep -xF "worktree $2" >/dev/null; }
agent_rec() { ls "$STORE/agents" 2>/dev/null | grep -- "--$1.json" | head -1 | sed "s|^|$STORE/agents/|"; }
done_rec()  { ls "$STORE/done" 2>/dev/null | grep -- "--$1.json" | head -1 | sed "s|^|$STORE/done/|"; }
jget() { python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); cur=d
for k in sys.argv[2].split("."):
    cur = cur.get(k) if isinstance(cur, dict) else None
print("" if cur is None else (json.dumps(cur) if isinstance(cur,(dict,list)) else cur))' "$1" "$2" 2>/dev/null; }
quarantine_dirs() { find "$T" -maxdepth 4 \( -iname '*retired*' -o -iname '*quarantin*' \) 2>/dev/null; }
WS="$ENGINE/mega-lander/workspaces.sh"
CREATE="$ENGINE/mega-lander/create-teammate-worktree.sh"
ws() { RICHOS_SESSION_ID="$CUR_SID" "$WS" "$@"; }
# Orchestration reliability R2/R3 (richos-hq docs/prds/2026-10-23-orchestration-reliability.md):
# the Stop gate no longer refuses a turn end; finished work stays owned and is DUE in the one
# status calculation, which `workspaces.sh holds` prints. A case that used to read a refusal
# naming the work now reads an allowed turn end and the work due, by name.
due_has() { ws holds >"$T/holds.out" 2>&1; grep -qE "^DUE .*$1" "$T/holds.out"; }

start_session "sess-fourteen-1111"

# ===========================================================================
begin C14 "The integration branch is recorded before the first spawn, or the spawn is refused naming what to record; every consumer asks the recorded branch and none asks main"
# The page: "The branch a body of work integrates on is RECORDED when that work
# starts, before its first agent is spawned. Nothing infers it and nothing
# guesses it." Nothing is recorded yet. The check's first clause is a guarantee
# only if the SYSTEM refuses the spawn; a habit of recording is not a guarantee.
spawn "zach-opus-n0" "work with no integration branch recorded"; rc=$?
sub "C14.1 with NO branch recorded, the first spawn is refused, naming the recording command" \
    "[ $rc -ne 0 ] && grep -q 'workspaces.sh integration' '$T/spawn.err'" "spawn rc=$rc stderr=$(cat "$T/spawn.err")"
ws integration --repo "$ENT" --branch main --why "the fourteen checks, entity" >"$T/integration.out" 2>&1; rc1=$?
ws integration --repo "$OTHER" --branch main --why "the fourteen checks, other" >>"$T/integration.out" 2>&1; rc2=$?
git -C "$DEV" branch dev/work
ws integration --repo "$DEV" --branch dev/work --why "the fourteen checks, a body of work on a dev branch" >>"$T/integration.out" 2>&1; rc3=$?
sub "C14.2 Rich records the branch, once per repository, and the record names it" \
    "[ $rc1 -eq 0 ] && [ $rc2 -eq 0 ] && [ $rc3 -eq 0 ] && grep -q 'dev/work' '$T/integration.out'" "$(cat "$T/integration.out")"
# Work merged onto its RECORDED dev branch, never onto main, counts as landed.
git -C "$DEV" worktree add -q "$T/devrepo-wt/zach-opus-d1" -b cc/zach-opus-d1 2>/dev/null || true
git -C "$DEV" worktree remove --force "$T/devrepo-wt/zach-opus-d1" 2>/dev/null; git -C "$DEV" branch -D cc/zach-opus-d1 >/dev/null 2>&1 || true
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$DEV" zach-opus-d1 >"$T/create.out" 2>&1; rc=$?
CCD="$T/devrepo-wt/zach-opus-d1"
spawn "zach-opus-d1" "$(printf 'edit the dev repository\ncross-repo-worktree: %s\n' "$CCD")"; rc=$?
platform_spawn "zach-opus-d1" "ad1d1d1d1d1d1d1d1"
commit_in "$CCD" dev.txt
DEV_TIP="$(git -C "$CCD" rev-parse HEAD)"
subagent_stop "ad1d1d1d1d1d1d1d1"
git -C "$DEV" branch -f dev/work "$DEV_TIP"          # merged onto the dev branch (a fast-forward), main untouched
stop_gate; rc=$?
sub "C14.3 work merged onto its recorded dev branch (not main) is landed on its own: workspace and branch gone" \
    "[ $rc -eq 0 ] && [ ! -e '$CCD' ] && ! has_branch '$DEV' cc/zach-opus-d1 && ! git -C '$DEV' merge-base --is-ancestor $DEV_TIP main" "$(cat "$T/stop.err")"
# Consumers: unlanded-branches.py and guard-unresolved-claims.py ask the record.
: > "$T/ledger.jsonl"
git -C "$DEV" branch side/ahead "$DEV_TIP"; commit_in "$DEV" ahead-marker.txt 2>/dev/null || true
git -C "$DEV" checkout -q -b tmp-ahead "$DEV_TIP" 2>/dev/null; printf 'ahead\n' > "$DEV/ahead.txt"; git -C "$DEV" add ahead.txt; git -C "$DEV" commit -q -m "ahead of dev/work"; git -C "$DEV" checkout -q main; git -C "$DEV" branch -f side/ahead tmp-ahead; git -C "$DEV" branch -D tmp-ahead >/dev/null
RICHOS_SESSION_ID="$CUR_SID" bash "$ENGINE/scripts/unlanded-branches-lint.sh" "$DEV" --json --ledger "$T/ledger.jsonl" >"$T/unlanded.json" 2>"$T/unlanded.err"; rc_u=$?
sub "C14.4 unlanded-branches.py compares against the RECORDED branch dev/work (a branch merged onto dev/work but not main is not ahead; one ahead of dev/work is)" \
    "[ $rc_u -eq 1 ] && grep -q '\"trunk\": *\"dev/work\"' '$T/unlanded.json' && grep -q 'side/ahead' '$T/unlanded.json'" "rc=$rc_u $(head -c 400 "$T/unlanded.json") $(cat "$T/unlanded.err")"
git -C "$DEV" branch -D side/ahead >/dev/null
NOREC="$T/norecord"; new_repo "$NOREC"; git -C "$NOREC" branch stray; git -C "$NOREC" checkout -q stray; printf 's\n' > "$NOREC/s.txt"; git -C "$NOREC" add s.txt; git -C "$NOREC" commit -q -m s; git -C "$NOREC" checkout -q main
RICHOS_SESSION_ID="$CUR_SID" bash "$ENGINE/scripts/unlanded-branches-lint.sh" "$NOREC" --json --ledger "$T/ledger.jsonl" >"$T/unlanded2.json" 2>"$T/unlanded2.err"; rc_n=$?
sub "C14.5 with no branch recorded, unlanded-branches.py ABSTAINS (exit 2) rather than assuming main" \
    "[ $rc_n -eq 2 ]" "rc=$rc_n $(head -c 300 "$T/unlanded2.json") $(cat "$T/unlanded2.err")"
python3 - "$HOOKS/guard-unresolved-claims.py" "$DEV" "$DEV_TIP" "$NOREC" > "$T/claims.out" 2>&1 <<'PY'
import importlib.util, sys, time
spec = importlib.util.spec_from_file_location("guc", sys.argv[1]); m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
dev, tip, norec = sys.argv[2], sys.argv[3], sys.argv[4]
print("dev:", m.state_verdict("integrated", tip, [dev], time.monotonic() + 8))
import subprocess
sha = subprocess.run(["git", "-C", norec, "rev-parse", "stray"], capture_output=True, text=True).stdout.strip()
print("norecord:", m.state_verdict("integrated", sha, [norec], time.monotonic() + 8))
PY
sub "C14.6 guard-unresolved-claims.py: a commit on the recorded dev branch (not main) is 'integrated'; with no record it abstains ('unknown')" \
    "grep -q \"dev: ('ok',)\" '$T/claims.out' && grep -q \"norecord: ('unknown',)\" '$T/claims.out'" "$(cat "$T/claims.out")"
MAINREFS="$(grep -c 'INTEGRATION_REFS\b' "$HOOKS/guard-unresolved-claims.py")"
MAINUSE="$(grep -n 'INTEGRATION_REFS' "$HOOKS/guard-unresolved-claims.py" | grep -v -E '^[0-9]+:INTEGRATION_REFS =|^[0-9]+:#' | wc -l | tr -d ' ')"
sub "C14.7 guard-unresolved-claims.py keeps no live fallback to main: its INTEGRATION_REFS tuple is referenced by no code" \
    "[ \"$MAINUSE\" = 0 ]" "INTEGRATION_REFS mentions=$MAINREFS, code uses=$MAINUSE: $(grep -n 'INTEGRATION_REFS' "$HOOKS/guard-unresolved-claims.py" | tr '\n' ' ')"
# "Finished work never waits on the agent's own branch": an agent's branch is never the target.
ws integration --repo "$DEV" --branch cc/zach-opus-d1 --why "an agent's own branch as the target" >"$T/integration2.out" 2>&1; rc=$?
sub "C14.8 an agent's own workspace branch (cc/) is refused as an integration branch (exit $rc), and the record still names dev/work" \
    "[ $rc -eq 2 ] && grep -q 'own workspace branch' '$T/integration2.out' && [ \"\$(ws integration --repo '$DEV' 2>/dev/null | cut -f2)\" = dev/work ]" "$(cat "$T/integration2.out")"
# WHO RECORDS IT (both round-6 reviewers): nothing checked, so the party under test could
# record `wip` in the store the runner reads, or `git branch -f dev/work HEAD` — and either
# move retargets where every in-flight agent in that repository lands. In an AGENT's call
# (the payload carries its id) the Bash guard refuses both, the way it refuses `claude -w`.
AGENT_ANY="ad1d1d1d1d1d1d1d1"
agent_bash_guard "$AGENT_ANY" "$WS integration --repo $DEV --branch wip --why 'a new body of work, says the engineer'"; r1=$?
agent_bash_guard "$AGENT_ANY" "python3 $ENGINE/mega-lander/workspaces.py integration --repo $DEV --branch dev/work --correct --why 'moved by the engineer'"; r2=$?
sub "C14.9 an AGENT's call recording or correcting the integration branch is refused by the Bash guard (exits $r1 $r2), naming point 14" \
    "[ $r1 -eq 2 ] && [ $r2 -eq 2 ] && grep -q 'point 14' '$T/bash.err'" "$(cat "$T/bash.err")"
bash_guard "$WS integration --repo $DEV --branch dev/work --why 're-stated by Rich'"; r3=$?
sub "C14.10 POSITIVE CONTROL: the lead's own call recording it passes the guard ($r3)" "[ $r3 -eq 0 ]" "$(cat "$T/bash.err")"
agent_bash_guard "$AGENT_ANY" "git branch -f dev/work HEAD"; r4=$?
agent_bash_guard "$AGENT_ANY" "git update-ref refs/heads/dev/work HEAD"; r5=$?
agent_bash_guard "$AGENT_ANY" "git push . HEAD:dev/work"; r6=$?
agent_bash_guard "$AGENT_ANY" "git branch -D dev/work"; r7=$?
sub "C14.11 an AGENT's call moving, pushing into or deleting a RECORDED integration branch is refused (exits $r4 $r5 $r6 $r7)" \
    "[ $r4 -eq 2 ] && [ $r5 -eq 2 ] && [ $r6 -eq 2 ] && [ $r7 -eq 2 ] && grep -q 'RECORDED integration branch' '$T/bash.err'" "$(cat "$T/bash.err")"
agent_bash_guard "$AGENT_ANY" "git branch -f side/scratch HEAD"; r8=$?
bash_guard "git branch -f dev/work HEAD"; r9=$?
sub "C14.12 PRECISION: an agent moving a branch nobody recorded passes ($r8), and the lead moving the recorded one passes ($r9) — landing is his" \
    "[ $r8 -eq 0 ] && [ $r9 -eq 0 ]" "$(cat "$T/bash.err")"
# ROUND 8, ITEM 2 — a RECORDED branch moved by an agent by ANY means. Both reviewers measured
# eight verbs that name the branch passing the guard and moving it (branch -C, push
# HEAD:heads/<it>, fetch, pull, checkout -B, switch -C, symbolic-ref, send-pack), and the
# DOORWAY: a plain `checkout <it>`, after which commit/reset/merge/rebase move it naming
# nothing, from the agent's worktree or the main checkout. "A longer verb list cannot close
# this" — so the snapshot/observe pair records the recorded refs' TIPS at an agent's
# PreToolUse and REPORTS a move at its PostToolUse (C14.13), re-creates a DELETED one
# (C14.14), and passes over the lead's legitimate move in silence (C14.15); the guard refuses
# the eight verbs and the doorway by name (C14.16).
#
# AMENDED 2026-09-14: A MOVE IS REPORTED AND THE REF IS LEFT ALONE. Until then C14.13 asserted
# the ref was moved BACK, and that write moved refs/heads/main in richos three times in one
# night — twice in twelve seconds in opposite directions, while Rich was landing, each write
# manufacturing the condition the next agent's check fired on (docs/verification/
# ref-write-forensics-2026-09-14.md; reproduced at docs/verification/
# protected-ref-oscillation-2026-09-14-logs/). A check that infers the writer from the SHAPE
# of the result cannot tell Rich's ordinary land from the doorway it hunts, so it reports and
# a human decides. A DELETION is still put back: it is unambiguous, point 2 forbids it
# outright, and re-creating a ref at the tip it held loses nothing.
#
# The agent below has a cc/ workspace in the dev repository.
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$DEV" zach-opus-d2 >/dev/null 2>&1
CCD2="$T/devrepo-wt/zach-opus-d2"
spawn "zach-opus-d2" "$(printf 'edit the dev repository\ncross-repo-worktree: %s\n' "$CCD2")"
platform_spawn "zach-opus-d2" "ad2d2d2d2d2d2d2d2"
commit_in "$CCD2" d2.txt
DW_TIP="$(git -C "$DEV" rev-parse dev/work)"
EV_BEFORE="$(wc -l < "$STORE/events.jsonl")"
agent_call_pre "ad2d2d2d2d2d2d2d2" "tu-d2-1"
git -C "$CCD2" checkout -q dev/work && git -C "$CCD2" commit -q --allow-empty -m "moved unnamed"    # THE DOORWAY, from the agent's own worktree
agent_call_post "ad2d2d2d2d2d2d2d2" "tu-d2-1"
git -C "$CCD2" checkout -q cc/zach-opus-d2
DW_MOVED="$(git -C "$DEV" rev-parse dev/work)"
sub "C14.13 the RECORDED branch moved by the UNNAMED doorway (checkout dev/work, then commit — no verb names it) from the agent's own worktree is REPORTED at the call's PostToolUse (event log + the hook's notice) and the ref is LEFT WHERE IT IS — the engine never moves a protected ref back" \
    "[ \"\$(git -C '$DEV' rev-parse dev/work)\" = '$DW_MOVED' ] && [ '$DW_MOVED' != '$DW_TIP' ] && tail -n +$((EV_BEFORE + 1)) '$STORE/events.jsonl' | grep '\"event\": \"protected-ref-moved\"' | grep -q '\"branch\": \"dev/work\"' && grep -q 'PROTECTED REF MOVED: dev/work' '$T/observe.err' && grep -q 'NOTHING WAS CHANGED' '$T/observe.err'" "tip=$(git -C "$DEV" rev-parse dev/work) snapshot=$DW_TIP $(tail -2 "$T/observe.err")"
agent_call_pre "ad2d2d2d2d2d2d2d2" "tu-d2-2"
git -C "$DEV" update-ref -d refs/heads/dev/work                   # DELETED, from the main checkout, in the agent's call
agent_call_post "ad2d2d2d2d2d2d2d2" "tu-d2-2"
sub "C14.14 the RECORDED branch DELETED from the main checkout during an agent's call is re-created at the call's PostToolUse at the snapshot tip, by a create-only write that carries a reflog message (an anonymous ref write is what cost a day of forensics)" \
    "has_branch '$DEV' dev/work && [ \"\$(git -C '$DEV' rev-parse dev/work)\" = '$DW_MOVED' ] && git -C '$DEV' reflog show dev/work | grep -q 'richos engine: protected ref restored'" "$(git -C "$DEV" for-each-ref refs/heads/dev) $(git -C "$DEV" reflog show dev/work | head -2)"
EV_BEFORE_LEAD="$(wc -l < "$STORE/events.jsonl")"
agent_call_pre "ad2d2d2d2d2d2d2d2" "tu-d2-3"
LEAD_LAND="$(git -C "$DEV" commit-tree "$(git -C "$DEV" rev-parse "$DW_MOVED^{tree}")" -p "$DW_MOVED" -m "the lead lands a FINISHED agent's work onto dev/work")"
git -C "$DEV" branch -f dev/work "$LEAD_LAND"                     # the lead's land: a descendant carrying none of THIS agent's work
agent_call_post "ad2d2d2d2d2d2d2d2" "tu-d2-3"
sub "C14.15 PRECISION: the lead's own move of the recorded branch during the agent's call — a descendant of the snapshot tip carrying none of this agent's work (his land of a finished agent) — stands, and is not even reported" \
    "[ \"\$(git -C '$DEV' rev-parse dev/work)\" = '$LEAD_LAND' ] && ! tail -n +$((EV_BEFORE_LEAD + 1)) '$STORE/events.jsonl' | grep -q 'protected-ref-'" "tip=$(git -C "$DEV" rev-parse dev/work) expected=$LEAD_LAND $(tail -2 "$T/observe.err")"
AG_D2="ad2d2d2d2d2d2d2d2"
agent_bash_guard "$AG_D2" "git -C $DEV branch -C side dev/work"; v1=$?
agent_bash_guard "$AG_D2" "git -C $DEV push . HEAD:heads/dev/work"; v2=$?
agent_bash_guard "$AG_D2" "git -C $DEV fetch . +HEAD:dev/work"; v3=$?
agent_bash_guard "$AG_D2" "git -C $DEV pull . +side:dev/work"; v4=$?
agent_bash_guard "$AG_D2" "git -C $DEV checkout -B dev/work"; v5=$?
agent_bash_guard "$AG_D2" "git -C $DEV switch -C dev/work"; v6=$?
agent_bash_guard "$AG_D2" "git -C $DEV symbolic-ref refs/heads/dev/work refs/heads/main"; v7=$?
agent_bash_guard "$AG_D2" "git -C $DEV send-pack . HEAD:refs/heads/dev/work"; v8=$?
agent_bash_guard "$AG_D2" "git -C $DEV checkout dev/work"; v9=$?
cp "$T/bash.err" "$T/bash-doorway.err"
agent_bash_guard "$AG_D2" "git -C $CCD2 switch dev/work"; v10=$?
bash_guard "git -C $DEV checkout dev/work"; v11=$?
agent_bash_guard "$AG_D2" "git -C $DEV checkout -B side/scratch"; v12=$?
sub "C14.16 the Bash guard refuses the eight verbs that NAME the recorded branch from an agent's call (branch -C, push HEAD:heads/, fetch, pull, checkout -B, switch -C, symbolic-ref, send-pack: exits $v1 $v2 $v3 $v4 $v5 $v6 $v7 $v8) and the plain checkout/switch doorway ($v9 $v10), naming point 14; the lead's checkout passes ($v11) and an agent's -B of an unrecorded branch passes ($v12)" \
    "[ $v1 -eq 2 ] && [ $v2 -eq 2 ] && [ $v3 -eq 2 ] && [ $v4 -eq 2 ] && [ $v5 -eq 2 ] && [ $v6 -eq 2 ] && [ $v7 -eq 2 ] && [ $v8 -eq 2 ] && [ $v9 -eq 2 ] && [ $v10 -eq 2 ] && [ $v11 -eq 0 ] && [ $v12 -eq 0 ] && grep -q 'point 14' '$T/bash-doorway.err'" "$(cat "$T/bash-doorway.err")"
# THE BOUND OUTSIDE A WINDOW (measured on Sage's runner-round case R8 on 2026-09-13, which my
# first restore rule broke): with NO call open, the lead merges the agent's OWN branch onto the
# recorded branch — after the agent's last call, before its end signal. The end-of-run
# observation compares against a snapshot that may be minutes old; a descendant carrying the
# agent's own work is its doorway only INSIDE a window, so here it is the lead's land and stays.
MERGE="$(git -C "$DEV" commit-tree "$(git -C "$CCD2" rev-parse 'HEAD^{tree}')" -p "$(git -C "$DEV" rev-parse dev/work)" -p "$(git -C "$CCD2" rev-parse HEAD)" -m "the lead merges the agent's branch onto dev/work")"
git -C "$DEV" branch -f dev/work "$MERGE"
EV_BEFORE_END="$(wc -l < "$STORE/events.jsonl")"
subagent_stop "ad2d2d2d2d2d2d2d2"                                 # the end signal: the last observation, no window open
ws land zach-opus-d2 >"$T/land-d2.out" 2>&1; rl=$?
sub "C14.15b PRECISION, outside a window: the lead's merge of the agent's OWN branch onto the recorded branch after its last call and before its end signal is neither undone nor reported at the end signal, and the land proceeds ($rl)" \
    "[ \"\$(git -C '$DEV' rev-parse dev/work)\" = '$MERGE' ] && [ $rl -eq 0 ] && [ ! -e '$CCD2' ] && ! has_branch '$DEV' cc/zach-opus-d2 && ! tail -n +$((EV_BEFORE_END + 1)) '$STORE/events.jsonl' | grep -q 'protected-ref-'" "tip=$(git -C "$DEV" rev-parse dev/work) expected=$MERGE $(cat "$T/land-d2.out")"
verdict

# ===========================================================================
begin C1 "Creating a non-native workspace not named cc/ is refused — non-zero exit, nothing created"
ws register-cc --name zach-opus-x1 --repo "$OTHER" --path "$T/other-wt/zach-opus-x1" --branch feature/x >"$T/reg.out" 2>&1; rc=$?
sub "C1.1 the registry refuses a non-native workspace on a branch not named cc/ (exit $rc)" \
    "[ $rc -ne 0 ] && grep -q 'cc/' '$T/reg.out' && [ -z \"\$(agent_rec zach-opus-x1)\" ] && [ ! -e '$T/other-wt/zach-opus-x1' ]" "$(cat "$T/reg.out")"
bash_guard "git worktree add $T/raw-feature -b feature/y"; rc=$?
sub "C1.2 a raw 'git worktree add' in Bash is refused by the worktree guard (exit $rc), and nothing is created" \
    "[ $rc -eq 2 ] && grep -q 'REFUSED' '$T/bash.err' && [ ! -e '$T/raw-feature' ]" "$(cat "$T/bash.err")"
bash_guard "git worktree add $T/raw-cc -b cc/zach-opus-raw"; rc=$?
sub "C1.3 even a raw add NAMED cc/ is refused: a workspace is created by create-teammate-worktree.sh, which registers it first (point 3)" \
    "[ $rc -eq 2 ] && [ ! -e '$T/raw-cc' ]" "$(cat "$T/bash.err")"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-c1 >"$T/create.out" 2>&1; rc=$?
CC1="$T/other-wt/zach-opus-c1"
sub "C1.4 POSITIVE CONTROL: the sanctioned path creates a cc/ workspace, registered (exit $rc)" \
    "[ $rc -eq 0 ] && has_branch '$OTHER' cc/zach-opus-c1 && [ -d '$CC1' ] && [ -n \"\$(agent_rec zach-opus-c1)\" ]" "$(cat "$T/create.out")"
# It is registered but never spawned into. Such a registration has no run to end
# (workspaces.py never_ran, 2026-10-14: a refused spawn's workspace needed a stop
# before its discard), so the page's own discard takes it directly, with no stop.
ws discard zach-opus-c1 --reason "the positive control of check 1 is over" --not-ceo-ordered "a fixture of this suite" >"$T/discard.out" 2>&1; rc=$?
sub "C1.5 the control workspace, registered but never spawned into, is deleted by the page's own discard with no stop first (discard: $rc)" \
    "[ $rc -eq 0 ] && [ ! -e '$CC1' ] && ! has_branch '$OTHER' cc/zach-opus-c1" "$(cat "$T/discard.out")"
verdict

# ===========================================================================
begin C2 "No deletion path reaches a codex/ path or ref: with codex/ present in the fixture, every deleter runs and codex/ is byte-identical after; an agent briefed against codex/ work gets a cc/ copy"
CX="$T/codex-wt"
git -C "$OTHER" worktree add -q "$CX" -b codex/fix
commit_in "$CX" codex-work.txt
git -C "$OTHER" branch codex/other
codex_state() { { git -C "$OTHER" rev-parse codex/fix codex/other; git -C "$OTHER" worktree list --porcelain | grep -A3 "^worktree $CX\$"; (cd "$CX" && find . -path ./.git -prune -o -type f -print0 | sort -z | xargs -0 shasum -a 256); } | shasum -a 256 | cut -d' ' -f1; }
BEFORE="$(codex_state)"
# every deleter: an automatic land at the Stop gate, a discard, a retry, a
# SessionStart scan, the Bash guard on direct deletion, and a spawn aimed at it.
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-c2 >/dev/null 2>&1
CC2="$T/other-wt/zach-opus-c2"
spawn "zach-opus-c2" "$(printf 'x\ncross-repo-worktree: %s\n' "$CC2")"
platform_spawn "zach-opus-c2" "ac2c2c2c2c2c2c2c2"
commit_in "$CC2" c2.txt; subagent_stop "ac2c2c2c2c2c2c2c2"
git -C "$OTHER" merge -q --no-edit cc/zach-opus-c2
stop_gate; rc_land=$?
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-c2b >/dev/null 2>&1
CC2B="$T/other-wt/zach-opus-c2b"
spawn "zach-opus-c2b" "$(printf 'x\ncross-repo-worktree: %s\n' "$CC2B")"
platform_spawn "zach-opus-c2b" "ac2bc2bc2bc2bc2bc"
commit_in "$CC2B" c2b.txt; subagent_stop "ac2bc2bc2bc2bc2bc"
ws discard zach-opus-c2b --reason "the reviewer rejected it, check 2 fixture" --not-ceo-ordered "a fixture of this suite" >"$T/discard.out" 2>&1; rc_disc=$?
ws retry >/dev/null 2>&1
payload SessionStart "{\"session_id\":\"$CUR_SID\",\"cwd\":\"$ENT\"}" | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
sub "C2.1 a land (exit $rc_land) and a discard (exit $rc_disc) in the repository ran, the retry ran, the SessionStart scan ran" "[ $rc_land -eq 0 ] && [ $rc_disc -eq 0 ]" "$(cat "$T/stop.err" "$T/discard.out")"
bash_guard "git branch -D codex/other"; r1=$?
bash_guard "git worktree remove $CX"; r2=$?
bash_guard "rm -rf $CX"; r3=$?
bash_guard "git worktree remove $CX  # worktree-remove-ack: I really want to"; r4=$?
sub "C2.2 direct deletion of the codex/ branch or workspace is refused in Bash, with no override (exits $r1 $r2 $r3 $r4)" \
    "[ $r1 -eq 2 ] && [ $r2 -eq 2 ] && [ $r3 -eq 2 ] && [ $r4 -eq 2 ]" "$(cat "$T/bash.err")"
spawn "zach-opus-cx" "$(printf 'fix it in codex\ncross-repo-worktree: %s\n' "$CX")"; rc=$?
sub "C2.3 a spawn aimed INSIDE the codex/ workspace is refused (exit $rc)" "[ $rc -eq 2 ] && grep -q 'codex/' '$T/spawn.err'" "$(cat "$T/spawn.err")"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-cb --base codex/fix >"$T/create.out" 2>&1; rc=$?
sub "C2.4 a cc/ workspace based ON the codex/ branch is refused, and the refusal names the copy to take instead (exit $rc)" \
    "[ $rc -eq 3 ] && grep -q 'branch from the commit instead' '$T/create.out' && ! has_branch '$OTHER' cc/zach-opus-cb" "$(cat "$T/create.out")"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-cc --base "$(git -C "$OTHER" rev-parse codex/fix)" >"$T/create.out" 2>&1; rc=$?
CCC="$T/other-wt/zach-opus-cc"
sub "C2.5 an agent briefed against codex/ work gets a cc/ COPY: cc/zach-opus-cc at the codex/fix commit (exit $rc)" \
    "[ $rc -eq 0 ] && [ \"\$(git -C '$CCC' rev-parse HEAD)\" = \"\$(git -C '$OTHER' rev-parse codex/fix)\" ] && [ \"\$(git -C '$CCC' rev-parse --abbrev-ref HEAD)\" = cc/zach-opus-cc ]" "$(cat "$T/create.out")"
ws stop zach-opus-cc --why "the codex/ copy fixture is done with" >/dev/null 2>&1        # never spawned into: no end signal of its own
ws discard zach-opus-cc --reason "the codex/ copy fixture is done with" --not-ceo-ordered "a fixture of this suite" >/dev/null 2>&1
# A DELETER AIMED AT A codex/ REF ON AN AGENT'S RECORD (round 6, Frank §2.3: byte-identity was
# proved by never being asked). The only way a codex/ ref reaches a record is by hand or by a
# defect — observe_created_refs never attributes one — so the fixture writes it there, and the
# discard's branch deleter is then pointed straight at it.
spawn "zach-opus-cz" "a record that will carry a codex/ ref"
platform_spawn "zach-opus-cz" "aczczczczczczczcz"
commit_in "$ENT/.claude/worktrees/agent-aczczczczczczczcz" cz.txt
subagent_stop "aczczczczczczczcz"
RECZ="$(agent_rec zach-opus-cz)"
python3 - "$RECZ" "$OTHER" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); d["created_branches"] = [[sys.argv[2], "codex/other"]]; json.dump(d, open(p, "w"))
PY
CODEX_OTHER_TIP="$(git -C "$OTHER" rev-parse codex/other)"
ws discard zach-opus-cz --reason "check 2: a deleter aimed at a codex/ ref on a record" --not-ceo-ordered "a fixture of this suite" >"$T/dz.out" 2>&1; rcz=$?
DZR="$(done_rec zach-opus-cz)"
sub "C2.8 a deleter AIMED at a codex/ ref on an agent's record refuses it: the discard completes ($rcz), codex/other is untouched, and the record and the store say so" \
    "[ $rcz -eq 0 ] && [ \"\$(git -C '$OTHER' rev-parse codex/other 2>/dev/null)\" = '$CODEX_OTHER_TIP' ] && [ -n \"$DZR\" ] && grep -q 'codex/ untouched' '$DZR' && grep -q '\"event\": \"codex-untouched\"' '$STORE/events.jsonl'" "$(cat "$T/dz.out") done=$DZR"
# ROUND 8, ITEM 3 — "A codex/ workspace or branch is never deleted without the CEO's express
# word. An agent never works inside a codex/ workspace." Measured on the base by both
# reviewers (brief-audit-frank-round8 §3, executed; brief-audit-sage-round8 §4): five deleters
# passed the Bash guard and DELETED a codex/ branch at rc=0, nine movers rewrote one, a commit
# inside a codex/ workspace landed on its branch, and a registered agent's Edit inside one
# passed the lock-out. Two mechanisms, both asked here: the Bash guard refuses every deleter
# and mover BY NAME (C2.13, C2.14), and the snapshot/observe pair sees what an agent's call did
# to a codex/ ref by ANY means — a verb the guard missed, the unnamed doorway, a non-git write.
# A DELETION it re-creates (C2.10); a MOVE it reports and leaves alone (C2.9, C2.11), because
# from 2026-09-14 the engine never moves a protected ref back: its own "restores" moved
# refs/heads/main in richos three times in one night, each write manufacturing the condition
# the next agent's check fired on (docs/verification/ref-write-forensics-2026-09-14.md).
# The lock-out refuses a writing tool aimed inside a codex/ workspace (C2.12). The agent below
# has a cc/ workspace in this repository.
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-cd >/dev/null 2>&1
CCD_="$T/other-wt/zach-opus-cd"
spawn "zach-opus-cd" "$(printf 'x\ncross-repo-worktree: %s\n' "$CCD_")"
platform_spawn "zach-opus-cd" "acdcdcdcdcdcdcdcd"
commit_in "$CCD_" cd.txt
CD_TIP="$(git -C "$CCD_" rev-parse HEAD)"
EV_BEFORE="$(wc -l < "$STORE/events.jsonl")"
agent_call_pre "acdcdcdcdcdcdcdcd" "tu-cd-1"
git -C "$OTHER" branch -f codex/other "$CD_TIP"                 # a MOVER, executed in the agent's call (as if the guard had missed it)
agent_call_post "acdcdcdcdcdcdcdcd" "tu-cd-1"
sub "C2.9 a codex/ ref MOVED during an agent's call (branch -f to the agent's tip) is REPORTED at the call's PostToolUse — the store's event log and the hook's notice say so — and the ref is left where it is: the engine does not move a protected ref back" \
    "[ \"\$(git -C '$OTHER' rev-parse codex/other)\" = '$CD_TIP' ] && tail -n +$((EV_BEFORE + 1)) '$STORE/events.jsonl' | grep '\"event\": \"protected-ref-moved\"' | grep -q '\"branch\": \"codex/other\"' && grep -q 'PROTECTED REF MOVED: codex/other' '$T/observe.err'" "tip=$(git -C "$OTHER" rev-parse codex/other) $(tail -3 "$T/observe.err")"
git -C "$OTHER" branch -f codex/other "$CODEX_OTHER_TIP"          # the human's own decision, made from the report
agent_call_pre "acdcdcdcdcdcdcdcd" "tu-cd-2"
git -C "$OTHER" update-ref -d refs/heads/codex/other              # a DELETER, executed in the agent's call
agent_call_post "acdcdcdcdcdcdcdcd" "tu-cd-2"
sub "C2.10 a codex/ ref DELETED during an agent's call (update-ref -d) is re-created at the call's PostToolUse at the snapshot tip — a deletion is unambiguous where a move is not, and point 2 forbids it outright" \
    "has_branch '$OTHER' codex/other && [ \"\$(git -C '$OTHER' rev-parse codex/other)\" = '$CODEX_OTHER_TIP' ]" "$(git -C "$OTHER" for-each-ref refs/heads/codex)"
agent_call_pre "acdcdcdcdcdcdcdcd" "tu-cd-3"
git -C "$CCD_" checkout -q codex/other && git -C "$CCD_" commit -q --allow-empty -m "moved unnamed"   # THE DOORWAY: checkout, then a commit that names nothing
DOORWAY_TIP="$(git -C "$CCD_" rev-parse HEAD)"
agent_call_post "acdcdcdcdcdcdcdcd" "tu-cd-3"
git -C "$CCD_" checkout -q cc/zach-opus-cd
sub "C2.11 a codex/ ref moved by the UNNAMED doorway (checkout it, then commit — no verb names it) from the agent's own worktree is REPORTED at the call's PostToolUse and left alone" \
    "[ \"\$(git -C '$OTHER' rev-parse codex/other)\" = '$DOORWAY_TIP' ] && tail -n +$((EV_BEFORE + 1)) '$STORE/events.jsonl' | grep '\"event\": \"protected-ref-moved\"' | grep -q '\"branch\": \"codex/other\"'" "tip=$(git -C "$OTHER" rev-parse codex/other) doorway=$DOORWAY_TIP $(tail -2 "$T/observe.err")"
git -C "$OTHER" branch -f codex/other "$CODEX_OTHER_TIP"          # the human's decision again, so the checks below start from the real tip
barrier_path "acdcdcdcdcdcdcdcd" Edit "$CX/codex-work.txt"; re1=$?
cp "$T/barrier.err" "$T/barrier-codex.err"
barrier_path "acdcdcdcdcdcdcdcd" Write "$CX/new-file.txt"; re2=$?
barrier_path "acdcdcdcdcdcdcdcd" Edit "$CCD_/cd.txt"; re3=$?
barrier_path "" Edit "$CX/codex-work.txt"; re4=$?
sub "C2.12 the lock-out refuses a registered agent's Edit ($re1) and Write ($re2) aimed INSIDE the codex/ workspace, naming point 2; the same agent's Edit in its own cc/ workspace passes ($re3); the lead's Edit inside codex/ passes ($re4)" \
    "[ $re1 -eq 2 ] && [ $re2 -eq 2 ] && [ $re3 -eq 0 ] && [ $re4 -eq 0 ] && grep -q 'point 2' '$T/barrier-codex.err'" "$(cat "$T/barrier-codex.err")"
AG_CD="acdcdcdcdcdcdcdcd"
agent_bash_guard "$AG_CD" "git -C $OTHER update-ref -d refs/heads/codex/other"; g1=$?
agent_bash_guard "$AG_CD" "git -C $OTHER push --delete . codex/other"; g2=$?
agent_bash_guard "$AG_CD" "git -C $OTHER push . :codex/other"; g3=$?
agent_bash_guard "$AG_CD" "git -C $OTHER push . :refs/heads/codex/other"; g4=$?
agent_bash_guard "$AG_CD" "git -C $OTHER branch -M codex/other not-codex"; g5=$?
sub "C2.13 the Bash guard refuses every DELETER of a codex/ ref from an agent's call by name (exits $g1 $g2 $g3 $g4 $g5): update-ref -d, push --delete, push :codex/x, push :refs/heads/codex/x, branch -M away" \
    "[ $g1 -eq 2 ] && [ $g2 -eq 2 ] && [ $g3 -eq 2 ] && [ $g4 -eq 2 ] && [ $g5 -eq 2 ] && grep -q 'point 2' '$T/bash.err'" "$(cat "$T/bash.err")"
agent_bash_guard "$AG_CD" "git -C $OTHER branch -f codex/other HEAD"; m1=$?
agent_bash_guard "$AG_CD" "git -C $OTHER branch -C side codex/other"; m2=$?
agent_bash_guard "$AG_CD" "git -C $OTHER update-ref refs/heads/codex/other HEAD"; m3=$?
agent_bash_guard "$AG_CD" "git -C $OTHER push . +HEAD:codex/other"; m4=$?
agent_bash_guard "$AG_CD" "git -C $OTHER fetch . +HEAD:codex/other"; m5=$?
agent_bash_guard "$AG_CD" "git -C $OTHER checkout -B codex/other"; m6=$?
agent_bash_guard "$AG_CD" "git -C $OTHER switch -C codex/other"; m7=$?
agent_bash_guard "$AG_CD" "git -C $OTHER symbolic-ref refs/heads/codex/other refs/heads/main"; m8=$?
agent_bash_guard "$AG_CD" "git -C $OTHER checkout codex/other"; m9=$?
agent_bash_guard "$AG_CD" "git -C $CX commit --allow-empty -m x"; m10=$?
agent_bash_guard "$AG_CD" "cd $CX && printf x >> README"; m11=$?
bash_guard "git -C $OTHER branch -f codex/other HEAD"; l1=$?
bash_guard "git -C $OTHER update-ref -d refs/heads/codex/other"; l2=$?
sub "C2.14 ...and every MOVER (branch -f, branch -C onto, update-ref, push +, fetch +, checkout -B, switch -C, symbolic-ref, the plain checkout doorway: exits $m1 $m2 $m3 $m4 $m5 $m6 $m7 $m8 $m9) and every command INSIDE the codex/ workspace ($m10 $m11); the lead's mover passes ($l1) and the lead's deleter is refused too ($l2)" \
    "[ $m1 -eq 2 ] && [ $m2 -eq 2 ] && [ $m3 -eq 2 ] && [ $m4 -eq 2 ] && [ $m5 -eq 2 ] && [ $m6 -eq 2 ] && [ $m7 -eq 2 ] && [ $m8 -eq 2 ] && [ $m9 -eq 2 ] && [ $m10 -eq 2 ] && [ $m11 -eq 2 ] && [ $l1 -eq 0 ] && [ $l2 -eq 2 ]" "$(cat "$T/bash.err")"
subagent_stop "acdcdcdcdcdcdcdcd"
ws discard zach-opus-cd --reason "the item-3 fixture is done with, check 2" --not-ceo-ordered "a fixture of this suite" >/dev/null 2>&1
AFTER="$(codex_state)"
sub "C2.6 codex/ is byte-identical after every deleter ran: refs, worktree listing and every file" \
    "[ \"$BEFORE\" = \"$AFTER\" ] && [ -d '$CX' ] && has_branch '$OTHER' codex/fix && has_branch '$OTHER' codex/other" "before=$BEFORE after=$AFTER"
stop_gate; rc=$?
sub "C2.7 nothing pending after check 2 (the codex/ workspace is not the system's concern)" "[ $rc -eq 0 ]" "$(cat "$T/stop.err")"
verdict

# ===========================================================================
begin C3 "A registration failure means the spawn does not happen; claude -w is refused; an unregistered cc/ or native workspace appears in point 5's pending list"
GOOD_PID="$RICHOS_SESSION_PID"
export RICHOS_SESSION_PID=999999            # a process that does not exist: the session's identity cannot be read
spawn "zach-opus-r1" "work"; rc=$?
export RICHOS_SESSION_PID="$GOOD_PID"
sub "C3.1 a registration that fails (no session identity) refuses the spawn (exit $rc) and leaves no record" \
    "[ $rc -eq 2 ] && grep -q -i 'regist' '$T/spawn.err' && [ -z \"\$(agent_rec zach-opus-r1)\" ]" "$(cat "$T/spawn.err")"
bash_guard "claude --worktree"; r1=$?
bash_guard "claude -w"; r2=$?
bash_guard "cd /somewhere && claude -w --model opus"; r3=$?
sub "C3.2 'claude --worktree' and 'claude -w' are refused in Bash (exits $r1 $r2 $r3)" "[ $r1 -eq 2 ] && [ $r2 -eq 2 ] && [ $r3 -eq 2 ]" "$(cat "$T/bash.err")"
git -C "$ENT" worktree add -q "$ENT/.claude/worktrees/lead-here" -b lead-here
WT_SID="sess-in-a-worktree-9"
SAVE_SID="$CUR_SID"
start_session "$WT_SID" "$ENT/.claude/worktrees/lead-here"
barrier "" Edit; re=$?
barrier "" Read; rr=$?
FORB="$(jget "$STORE/sessions/$WT_SID.json" forbidden)"
sub "C3.3 a session started inside its own workspace is recorded as not allowed; its lead is refused Edit ($re) and allowed Read ($rr)" \
    "[ -n \"$FORB\" ] && [ $re -eq 2 ] && [ $rr -eq 0 ]" "forbidden=$FORB $(cat "$T/barrier.err")"
payload SessionEnd "{\"session_id\":\"$WT_SID\",\"reason\":\"exit\"}" | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>&1
git -C "$ENT" worktree remove --force "$ENT/.claude/worktrees/lead-here"; git -C "$ENT" branch -D lead-here >/dev/null 2>&1
CUR_SID="$SAVE_SID"; export RICHOS_SESSION_PID="$GOOD_PID"
git -C "$OTHER" worktree add -q "$T/rogue-cc" -b cc/rogue-hand      # made raw: registered by nothing
commit_in "$T/rogue-cc" rogue.txt
git -C "$ENT" worktree add -q "$ENT/.claude/worktrees/agent-deadbeefdeadbeef0" -b worktree-agent-deadbeefdeadbeef0
commit_in "$ENT/.claude/worktrees/agent-deadbeefdeadbeef0" native-rogue.txt
stop_gate; rc=$?
sub "C3.4 an unregistered cc/ workspace is in point 5's list: the turn cannot end and it is named" \
    "[ $rc -eq 0 ] && due_has 'orphan-rogue-cc'" "$(cat "$T/stop.err")"
sub "C3.5 an unregistered NATIVE workspace is in point 5's list too, by name" \
    "[ $rc -eq 0 ] && due_has 'orphan-agent-deadbeefdeadbeef0'" "$(cat "$T/stop.err")"
spawn "zach-opus-r2" "new work"; rc=$?
sub "C3.6 unrelated new work starts while both orphans remain pending" "[ $rc -eq 0 ] && [ -e '$T/rogue-cc' ]" "$(cat "$T/spawn.err")"
ws discard orphan-rogue-cc --reason "made by hand, never registered" --not-ceo-ordered "a fixture of this suite" >"$T/d1.out" 2>&1; r1=$?
ws discard orphan-agent-deadbeefdeadbeef0 --reason "made by hand, never registered" --not-ceo-ordered "a fixture of this suite" >"$T/d2.out" 2>&1; r2=$?
sub "C3.7 both are handled under point 5 (discarded: $r1 $r2) and gone" \
    "[ $r1 -eq 0 ] && [ $r2 -eq 0 ] && [ ! -e '$T/rogue-cc' ] && ! has_branch '$OTHER' cc/rogue-hand && [ ! -e '$ENT/.claude/worktrees/agent-deadbeefdeadbeef0' ] && ! has_branch '$ENT' worktree-agent-deadbeefdeadbeef0" "$(cat "$T/d1.out" "$T/d2.out")"
# "...and any branch an agent created, counts as finished work of an ended session": a cc/
# BRANCH with no workspace at all, carrying a commit that is on no integration branch.
STRAY_TIP="$(git -C "$OTHER" commit-tree "$(git -C "$OTHER" rev-parse 'main^{tree}')" -p "$(git -C "$OTHER" rev-parse main)" -m "a stray commit")"
git -C "$OTHER" branch cc/stray-hand "$STRAY_TIP"
stop_gate; rc=$?
sub "C3.8 an unregistered cc/ BRANCH with no workspace is in point 5's list too, by name" \
    "[ $rc -eq 0 ] && due_has 'orphan-cc-stray-hand'" "$(cat "$T/stop.err")"
ws discard orphan-cc-stray-hand --reason "made by hand, never registered" --not-ceo-ordered "a fixture of this suite" >"$T/d3.out" 2>&1; r3=$?
sub "C3.9 and it is handled under point 5 (discarded: $r3) and gone" "[ $r3 -eq 0 ] && ! has_branch '$OTHER' cc/stray-hand" "$(cat "$T/d3.out")"
verdict

# ===========================================================================
begin C6 "Points 3 and 4 again, against a native agent-<id> / worktree-agent-<id> pair"
spawn "zach-opus-n1" "native work"; rc=$?
platform_spawn "zach-opus-n1" "an1n1n1n1n1n1n1n1"
NP1="$ENT/.claude/worktrees/agent-an1n1n1n1n1n1n1n1"
REC="$(agent_rec zach-opus-n1)"
sub "C6.1 the spawn is registered (exit $rc) and the native workspace joins its record by path and branch" \
    "[ $rc -eq 0 ] && [ -n \"$REC\" ] && grep -q '\"kind\": \"native\"' '$REC' && grep -q 'agent-an1n1n1n1n1n1n1n1' '$REC' && grep -q 'worktree-agent-an1n1n1n1n1n1n1n1' '$REC'" "$(head -30 "$REC" 2>/dev/null)"
python3 -c 'import json,sys; print(json.dumps({"hook_event_name":"PostToolUse","session_id":sys.argv[1],"tool_use_id":"tu-never-registered","tool_name":"Agent","cwd":sys.argv[2],"tool_input":{"name":"zach-opus-ghost","isolation":"worktree"},"tool_response":{"agentId":"aghostghostghost1","status":"async_launched"}}))' "$CUR_SID" "$ENT" \
    | bash "$HOOKS/workspace-lifecycle.sh" >/dev/null 2>>"$T/hooks.err"
barrier "aghostghostghost1" Edit; rc=$?
sub "C6.2 a native agent whose spawn was never registered gets no record and is refused every writing tool (exit $rc): the spawn did not happen" \
    "[ $rc -eq 2 ] && [ ! -e '$STORE/ids/aghostghostghost1' ]" "$(cat "$T/barrier.err")"
commit_in "$NP1" n1.txt
subagent_stop "an1n1n1n1n1n1n1n1"
git -C "$ENT" merge -q --no-edit "worktree-agent-an1n1n1n1n1n1n1n1"
stop_gate; rc=$?
sub "C6.3 after the land (automatic, exit $rc): workspace absent, branch absent, no prompt, no quarantine directory, no registry entry" \
    "[ $rc -eq 0 ] && [ ! -e '$NP1' ] && ! listed '$ENT' '$NP1' && ! has_branch '$ENT' worktree-agent-an1n1n1n1n1n1n1n1 && [ -z \"\$(quarantine_dirs)\" ] && [ -z \"\$(agent_rec zach-opus-n1)\" ] && [ -n \"\$(done_rec zach-opus-n1)\" ]" "$(cat "$T/stop.err") quarantine=$(quarantine_dirs)"
verdict

# ===========================================================================
begin C4 "After a land: workspace absent, branch absent, no prompt, no quarantine directory, no registry entry"
# What git itself lists BEFORE the spawn: after the land it must list nothing more. A workspace
# moved to `.parked-<name>` on a `parked/` branch passed the old C4.4, which grepped two directory
# names (round 6, Frank §2.2); the registry and the ref list cannot be fooled by a name.
BEFORE_OTHER="$(refs_and_worktrees "$OTHER")"; BEFORE_ENT="$(refs_and_worktrees "$ENT")"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-l1 >/dev/null 2>&1
CCL="$T/other-wt/zach-opus-l1"
spawn "zach-opus-l1" "$(printf 'x\ncross-repo-worktree: %s\n' "$CCL")"
platform_spawn "zach-opus-l1" "al1l1l1l1l1l1l1l1"
NPL="$ENT/.claude/worktrees/agent-al1l1l1l1l1l1l1l1"
commit_in "$CCL" l1.txt
subagent_stop "al1l1l1l1l1l1l1l1"
git -C "$OTHER" merge -q --no-edit cc/zach-opus-l1
BEFORE_WT="$(git -C "$OTHER" worktree list --porcelain | grep -c '^worktree ')"
stop_gate; rc=$?
sub "C4.1 no prompt: the merged work is landed by the Stop gate itself, with no command (exit $rc)" "[ $rc -eq 0 ] && ! grep -q 'land' '$T/stop.err'" "$(cat "$T/stop.err")"
sub "C4.2 workspace absent — from disk and from git's list (both workspaces)" \
    "[ ! -e '$CCL' ] && ! listed '$OTHER' '$CCL' && [ ! -e '$NPL' ] && ! listed '$ENT' '$NPL'"
sub "C4.3 branch absent (both)" "! has_branch '$OTHER' cc/zach-opus-l1 && ! has_branch '$ENT' worktree-agent-al1l1l1l1l1l1l1l1"
NEW_OTHER="$(new_lines "$BEFORE_OTHER" "$(refs_and_worktrees "$OTHER")")"
NEW_ENT="$(new_lines "$BEFORE_ENT" "$(refs_and_worktrees "$ENT")")"
sub "C4.4 no quarantine anywhere: git's worktree registry and ref list of BOTH repositories hold nothing they did not hold before the spawn (a workspace moved under any name, or a parked/ branch, would be listed), no quarantine directory, no detached leftover" \
    "[ -z \"$NEW_OTHER\" ] && [ -z \"$NEW_ENT\" ] && [ -z \"\$(quarantine_dirs)\" ] && ! git -C '$OTHER' worktree list --porcelain | grep -q '^detached' && ! git -C '$ENT' worktree list --porcelain | grep -q '^detached'" "new in other: [$NEW_OTHER] new in entity: [$NEW_ENT] $(quarantine_dirs; git -C "$OTHER" worktree list)"
sub "C4.5 no registry entry: nothing in agents/ names it, status lists nothing for it; its ending is in done/" \
    "[ -z \"\$(agent_rec zach-opus-l1)\" ] && [ -n \"\$(done_rec zach-opus-l1)\" ] && ! ws status 2>/dev/null | grep -q 'zach-opus-l1'"
verdict

# ===========================================================================
begin C7 "Every ending reads landed or discarded in the store; a discard carries its reason; an agent that produced nothing reads landed; CEO-ordered work cannot be discarded without a recorded word"
D="$(done_rec zach-opus-l1)"
sub "C7.1 the landed agent's ending reads 'landed' in the store" "[ \"\$(jget '$D' disposition.kind)\" = landed ]" "$(jget "$D" disposition)"
D2="$(done_rec zach-opus-c2b)"
sub "C7.2 the discarded agent's ending reads 'discarded' with its reason and the tips it deleted" \
    "[ \"\$(jget '$D2' disposition.kind)\" = discarded ] && [ -n \"\$(jget '$D2' disposition.reason)\" ] && [ -n \"\$(jget '$D2' disposition.tips)\" ]" "$(jget "$D2" disposition)"
spawn "zach-opus-z0" "produce nothing"
platform_spawn "zach-opus-z0" "az0z0z0z0z0z0z0z0"
subagent_stop "az0z0z0z0z0z0z0z0"
stop_gate; rc=$?
DZ="$(done_rec zach-opus-z0)"
sub "C7.3 an agent that produced nothing counts as landed: deleted on its own (exit $rc) and its ending reads 'landed'" \
    "[ $rc -eq 0 ] && [ -n \"$DZ\" ] && [ \"\$(jget '$DZ' disposition.kind)\" = landed ] && [ ! -e '$ENT/.claude/worktrees/agent-az0z0z0z0z0z0z0z0' ]" "$(cat "$T/stop.err")"
spawn "zach-opus-ceo" "$(printf 'build the thing he asked for\nceo-ordered: build the thing, his words 2026-09-12\n')"
platform_spawn "zach-opus-ceo" "aceoceoceoceoceo1"
commit_in "$ENT/.claude/worktrees/agent-aceoceoceoceoceo1" ceo.txt
subagent_stop "aceoceoceoceoceo1"
ws discard zach-opus-ceo --reason "a reviewer rejected the approach" --not-ceo-ordered "it was not his order, I think" >"$T/d.out" 2>&1; r1=$?
sub "C7.4 work recorded as CEO-ordered cannot be discarded without his word (exit $r1), even with a --not-ceo-ordered claim" \
    "[ $r1 -eq 2 ] && grep -q 'his word' '$T/d.out' && [ -e '$ENT/.claude/worktrees/agent-aceoceoceoceoceo1' ]" "$(cat "$T/d.out")"
ws discard zach-opus-ceo --reason "a reviewer rejected the approach" --ceo-word "drop it, he said 2026-09-12" >"$T/d.out" 2>&1; r2=$?
DC="$(done_rec zach-opus-ceo)"
sub "C7.5 with his word it is discarded and the word is RECORDED (exit $r2)" \
    "[ $r2 -eq 0 ] && [ \"\$(jget '$DC' disposition.ceo_word)\" = 'drop it, he said 2026-09-12' ]" "$(cat "$T/d.out") $(jget "$DC" disposition)"
# "with the reason recorded" is asked of a discard that GIVES none (round 6, Frank F1/F2:
# C7.2 asserted the reason the harness gave was stored, never that one is required).
spawn "zach-opus-nr" "work to be discarded without a reason"
platform_spawn "zach-opus-nr" "anrnrnrnrnrnrnrnr"
NPNR="$ENT/.claude/worktrees/agent-anrnrnrnrnrnrnrnr"
commit_in "$NPNR" nr.txt
subagent_stop "anrnrnrnrnrnrnrnr"
ws discard zach-opus-nr --reason "" --not-ceo-ordered "a fixture of this suite" >"$T/d.out" 2>&1; r1=$?
sub "C7.6 a discard with NO reason is refused (exit $r1) and deletes nothing" \
    "[ $r1 -eq 2 ] && grep -q -i 'reason' '$T/d.out' && [ -e '$NPNR' ]" "$(cat "$T/d.out")"
ws discard zach-opus-nr --reason "the reviewer rejected it, check 7 fixture" >"$T/d.out" 2>&1; r2=$?
sub "C7.7 a discard that says NEITHER --ceo-word nor --not-ceo-ordered is refused (exit $r2): the attestation is how 'never discarded without his word' is asked of work the prompt did not mark" \
    "[ $r2 -eq 2 ] && grep -q 'not-ceo-ordered' '$T/d.out' && [ -e '$NPNR' ]" "$(cat "$T/d.out")"
ws discard zach-opus-nr --reason "the reviewer rejected it, check 7 fixture" --not-ceo-ordered "a fixture of this suite" >/dev/null 2>&1
# THE CONTINUATION CLAUSE: "a new agent continues from its branch, the old workspaces are
# deleted when the new agent starts, and its work counts as landed when the new agent's does."
# No check asked it in round 6; one reviewer reported no code path for it. The path is the
# `continues:` prompt line (register_spawn), `_on_start` at the new agent's start, and the
# chain a land walks (`_chain`). Measured here through the hooks.
spawn "zach-opus-old" "work that will be continued"
platform_spawn "zach-opus-old" "aoldoldoldoldold1"
NPOLD="$ENT/.claude/worktrees/agent-aoldoldoldoldold1"
commit_in "$NPOLD" old.txt
subagent_stop "aoldoldoldoldold1"                        # finished, unlanded: pending
spawn "zach-opus-new" "$(printf 'continue it\ncontinues: zach-opus-old\n')"; rcn=$?
platform_spawn "zach-opus-new" "anewnewnewnewnew1"
NPNEW="$ENT/.claude/worktrees/agent-anewnewnewnewnew1"
RECOLD="$(agent_rec zach-opus-old)"
sub "C7.8 CONTINUATION: the new agent's spawn is allowed while the old one is pending ($rcn); at its start the OLD workspace is deleted, the old branch is kept, and the old record reads 'continued'" \
    "[ $rcn -eq 0 ] && [ ! -e '$NPOLD' ] && has_branch '$ENT' worktree-agent-aoldoldoldoldold1 && [ \"\$(jget '$RECOLD' disposition.kind)\" = continued ]" "$(cat "$T/spawn.err") $(jget "$RECOLD" disposition)"
commit_in "$NPNEW" new.txt
subagent_stop "anewnewnewnewnew1"
git -C "$ENT" merge -q --no-edit worktree-agent-aoldoldoldoldold1
git -C "$ENT" merge -q --no-edit worktree-agent-anewnewnewnewnew1
stop_gate; rcl=$?
DOLD="$(done_rec zach-opus-old)"
sub "C7.9 and the old work counts as landed when the new agent's does ($rcl): both branches gone, both workspaces gone, the old ending reads 'landed'" \
    "[ $rcl -eq 0 ] && ! has_branch '$ENT' worktree-agent-aoldoldoldoldold1 && ! has_branch '$ENT' worktree-agent-anewnewnewnewnew1 && [ ! -e '$NPNEW' ] && [ -n \"$DOLD\" ] && [ \"\$(jget '$DOLD' disposition.kind)\" = landed ]" "$(cat "$T/stop.err") old=$DOLD"
verdict

# ===========================================================================
begin C8 "A land of a tree with uncommitted or needed-ignored files is refused; after the commit it proceeds; nothing under the tree is lost by the deletion"
spawn "zach-opus-u1" "work, then leave things"
platform_spawn "zach-opus-u1" "au1u1u1u1u1u1u1u1"
NPU="$ENT/.claude/worktrees/agent-au1u1u1u1u1u1u1u1"
commit_in "$NPU" u1.txt
printf 'half-written\n' > "$NPU/draft.txt"                 # uncommitted
printf 'SECRET=needed\n' > "$NPU/.env"                     # ignored, and the main checkout does not have it
subagent_stop "au1u1u1u1u1u1u1u1"
git -C "$ENT" merge -q --no-edit "worktree-agent-au1u1u1u1u1u1u1u1"
ws land zach-opus-u1 >"$T/land.out" 2>&1; r1=$?
sub "C8.1 the land is refused while draft.txt is uncommitted (exit $r1), naming the uncommitted entry, and the tree is untouched" \
    "[ $r1 -eq 2 ] && grep -q 'uncommitted entr' '$T/land.out' && grep -q 'draft.txt' '$T/land.out' && [ -e '$NPU/draft.txt' ] && [ -e '$NPU/.env' ]" "$(cat "$T/land.out")"
stop_gate; r2=$?
sub "C8.2 the Stop gate does not land it either: it is pending, by name" "[ $r2 -eq 0 ] && due_has 'zach-opus-u1'" "$(cat "$T/stop.err")"
git -C "$NPU" add draft.txt && git -C "$NPU" commit -q -m "what it left"
git -C "$ENT" merge -q --no-edit "worktree-agent-au1u1u1u1u1u1u1u1"
ws land zach-opus-u1 >"$T/land.out" 2>&1; r3=$?
sub "C8.3 committed and merged, the land is still refused for the ignored .env the main checkout lacks (exit $r3)" \
    "[ $r3 -eq 2 ] && grep -q 'ignored' '$T/land.out' && [ -e '$NPU/.env' ]" "$(cat "$T/land.out")"
# SAME NAME, SAME SIZE, DIFFERENT BYTES (round 8, item 5; brief-audit-frank-round8 §5 F-A and F-H).
# Until this sub-assertion the .env reached the main checkout only by `cp`, so the two files
# compared were always identical bytes: a `_same_file` that always said True, or one that
# stopped at the size test, survived every check. A rotated key of equal length is the
# realistic case, and it is the one that makes both mutants red at once.
printf 'SECRET=nEEded\n' > "$ENT/.env"                      # 14 bytes, like the workspace's, other bytes
ws land zach-opus-u1 >"$T/land.out" 2>&1; r8=$?
sub "C8.8 an ignored file the main checkout has under the SAME NAME and SAME SIZE with DIFFERENT BYTES still holds the land (exit $r8), named by path — compared by content, not by name or size" \
    "[ $r8 -eq 2 ] && grep -q '\\.env' '$T/land.out' && [ -e '$NPU/.env' ] && [ \"\$(wc -c < '$ENT/.env')\" = \"\$(wc -c < '$NPU/.env')\" ]" "$(cat "$T/land.out")"
cp "$NPU/.env" "$ENT/.env"                                  # Rich keeps what it needs
# THE RECORDED INCIDENT'S SHAPE (09-10 §3b.2; round 6, Frank §2.1): an ignored DIRECTORY the main
# checkout also has — `.claude/`, which every main checkout on this machine carries — used to be
# skipped by name with nothing inside it compared. A needed file and a nested repository with a
# commit nobody else has, both under it, must hold the land and be named.
mkdir -p "$NPU/.claude/notes" && printf 'needed\n' > "$NPU/.claude/notes/needed.txt"
mkdir -p "$NPU/.claude/vendor/lib" && git init -q -b main "$NPU/.claude/vendor/lib" \
    && printf 'lib\n' > "$NPU/.claude/vendor/lib/lib.txt" && git -C "$NPU/.claude/vendor/lib" add -A \
    && git -C "$NPU/.claude/vendor/lib" commit -q -m "a commit nobody else has"
NESTED_TIP="$(git -C "$NPU/.claude/vendor/lib" rev-parse HEAD)"
ws land zach-opus-u1 >"$T/land.out" 2>&1; r5=$?
sub "C8.6 an ignored DIRECTORY the main checkout also has (.claude/) is compared by content, not skipped by name: a needed file and a nested repository's commit inside it hold the land (exit $r5), named by path" \
    "[ $r5 -eq 2 ] && grep -q '\\.claude/notes/needed.txt' '$T/land.out' && grep -q '\\.claude/vendor/lib/' '$T/land.out' && [ -e '$NPU/.claude/notes/needed.txt' ] && [ -d '$NPU/.claude/vendor/lib/.git' ]" "$(cat "$T/land.out")"
mkdir -p "$ENT/.claude/notes" && cp "$NPU/.claude/notes/needed.txt" "$ENT/.claude/notes/needed.txt"
cp -R "$NPU/.claude/vendor" "$ENT/.claude/vendor"          # Rich keeps what it needs, the nested repository included
TIP="$(git -C "$NPU" rev-parse HEAD)"
ws land zach-opus-u1 >"$T/land.out" 2>&1; r4=$?
sub "C8.4 with the needed file kept, the land proceeds (exit $r4) and the workspace and branch are gone" \
    "[ $r4 -eq 0 ] && [ ! -e '$NPU' ] && ! has_branch '$ENT' worktree-agent-au1u1u1u1u1u1u1u1" "$(cat "$T/land.out")"
MISSING_FILES="$(git -C "$ENT" ls-tree -r --name-only "$TIP" | while read -r f; do [ -e "$ENT/$f" ] || echo "$f"; done)"
sub "C8.5 nothing under the tree is lost: every file of the agent's tip is in the main checkout, and so is .env" \
    "[ -z \"$MISSING_FILES\" ] && grep -q 'SECRET=needed' '$ENT/.env' && git -C '$ENT' merge-base --is-ancestor $TIP main" "missing: $MISSING_FILES"
sub "C8.7 and nothing under the ignored directory is lost either: the needed file and the nested repository's commit ($NESTED_TIP) are in the main checkout" \
    "[ -e '$ENT/.claude/notes/needed.txt' ] && git -C '$ENT/.claude/vendor/lib' cat-file -e '$NESTED_TIP^{commit}' && [ ! -e '$NPU' ]"
verdict

# ===========================================================================
begin C9 "A restarted finished agent gets no tool; a process started inside the workspace is dead before deletion (fixture: a sleeper holding an open file)"
spawn "zach-opus-p1" "start a process"
platform_spawn "zach-opus-p1" "ap1p1p1p1p1p1p1p1"
NPP="$ENT/.claude/worktrees/agent-ap1p1p1p1p1p1p1p1"
commit_in "$NPP" p1.txt
printf 'held\n' > "$NPP/held.txt"; git -C "$NPP" add held.txt; git -C "$NPP" commit -q -m held
HOLD_PID="$(cd "$NPP" && sh -c 'exec 3<held.txt; sleep 3600 >/dev/null 2>&1 & echo $!')"   # cwd inside, a file open
subagent_stop "ap1p1p1p1p1p1p1p1"
barrier "ap1p1p1p1p1p1p1p1" Read; r1=$?
barrier "ap1p1p1p1p1p1p1p1" Bash; r2=$?
sub "C9.1 finished: the restarted agent is refused every tool, Read ($r1) and Bash ($r2) alike" "[ $r1 -eq 2 ] && [ $r2 -eq 2 ]" "$(cat "$T/barrier.err")"
sub "C9.2 POSITIVE CONTROL: the sleeper is alive in the workspace before the land (pid $HOLD_PID)" "kill -0 $HOLD_PID 2>/dev/null"
git -C "$ENT" merge -q --no-edit "worktree-agent-ap1p1p1p1p1p1p1p1"
stop_gate; rc=$?
ALIVE=0; is_dead "$HOLD_PID" || ALIVE=1
sub "C9.3 landed (exit $rc): the process is dead and the workspace is gone" "[ $rc -eq 0 ] && [ $ALIVE -eq 0 ] && [ ! -e '$NPP' ]" "alive=$ALIVE $(cat "$T/stop.err")"
KEY="$(basename "$(done_rec zach-opus-p1)" .json)"
ORDER="$(grep -E '"event": "(processes-stopped|deleted)"' "$STORE/events.jsonl" | grep -E "processes-stopped.*$HOLD_PID|\"key\": \"$KEY\"" | grep -o -E '"event": "[a-z-]+"' | tr -d '"' | sed 's/event: //' | tr '\n' ' ')"
sub "C9.4 and it was stopped BEFORE the deletion: the store's history reads 'processes-stopped' then 'deleted'" \
    "[ \"$ORDER\" = 'processes-stopped deleted ' ]" "order=[$ORDER]"
barrier "ap1p1p1p1p1p1p1p1" Write; r3=$?
sub "C9.5 restarted after its workspace is gone: still refused ($r3)" "[ $r3 -eq 2 ]"
# A PROCESS THAT IGNORES TERM (round 8, item 5; brief-audit-frank-round8 §5 F-G). C9.2–C9.4
# use a `sleep` that dies on SIGTERM, so removing the SIGKILL escalation survived them. "When
# an agent is finished, every process it started is stopped before its workspaces are deleted."
spawn "zach-opus-p2" "start a process that ignores TERM"
platform_spawn "zach-opus-p2" "ap2p2p2p2p2p2p2p2"
NPP2="$ENT/.claude/worktrees/agent-ap2p2p2p2p2p2p2p2"
commit_in "$NPP2" p2.txt
HOLD2_PID="$(start_term_ignorer "$NPP2" "$T/hold2.ready")"
HOLD_PID="$HOLD2_PID"   # the EXIT trap stops it if the land does not; C9.2-C9.5's holder is dead by now
subagent_stop "ap2p2p2p2p2p2p2p2"
kill -0 "$HOLD2_PID" 2>/dev/null && H2_BEFORE=1 || H2_BEFORE=0
kill -TERM "$HOLD2_PID" 2>/dev/null
H2_IGNORES=0; [ -e "$T/hold2.ready" ] && ! is_dead "$HOLD2_PID" && H2_IGNORES=1
sub "C9.6a POSITIVE CONTROL: the holder is alive (pid $HOLD2_PID, alive=$H2_BEFORE) and IGNORES TERM (it survived its own TERM before it said ready, and is alive after ours: $H2_IGNORES)" "[ $H2_BEFORE -eq 1 ] && [ $H2_IGNORES -eq 1 ]"
git -C "$ENT" merge -q --no-edit "worktree-agent-ap2p2p2p2p2p2p2p2"
stop_gate; rc6=$?
H2_ALIVE=0; is_dead "$HOLD2_PID" || H2_ALIVE=1
KEY2="$(basename "$(done_rec zach-opus-p2 2>/dev/null)" .json)"
SURV="$(grep '"event": "processes-stopped"' "$STORE/events.jsonl" | grep "$HOLD2_PID" | grep -o '"survivors": [^,}]*' | tail -1 | tr -d '"')"
sub "C9.6 a process that ignores TERM is dead after the land (exit $rc6, alive=$H2_ALIVE): the SIGKILL escalation, and the store records no survivor ($SURV)" \
    "[ $rc6 -eq 0 ] && [ $H2_ALIVE -eq 0 ] && [ ! -e '$NPP2' ] && [ -n '$KEY2' ] && [ '$SURV' = 'survivors: null' ]" "alive=$H2_ALIVE survivors=[$SURV] $(cat "$T/stop.err")"
HOLD_PID="$HOLD2_PID"   # cleanup kills it if the land did not
# THE HARNESS'S TWO FACTS, proven where they are used (audit R8, 2026-09-29). Both
# were a sleep until then; neither depends on how busy the Mac is now.
ZFILE="$T/zombie.pid"
python3 -c 'import os,signal,sys
child = os.fork()
if child == 0:
    os._exit(0)
open(sys.argv[1] + ".new", "w").write(str(child)); os.replace(sys.argv[1] + ".new", sys.argv[1])
while True:
    signal.pause()   # the child exit wakes pause() on macOS: hold in a loop, never return' "$ZFILE" </dev/null >/dev/null 2>&1 &
ZPARENT=$!
FIXTURE_PIDS+=("$ZPARENT")
while [ ! -s "$ZFILE" ] && kill -0 "$ZPARENT" 2>/dev/null; do sleep 0.05; done
ZCHILD="$(cat "$ZFILE" 2>/dev/null)"
# The child has exited and its parent never waits for it: wait for the kernel to say Z.
while [ -n "$ZCHILD" ] && kill -0 "$ZPARENT" 2>/dev/null \
        && ! ps -o stat= -p "$ZCHILD" 2>/dev/null | grep -q Z; do sleep 0.05; done
ZFOUND=0; [ -n "$ZCHILD" ] && kill -0 "$ZCHILD" 2>/dev/null && ZFOUND=1
ZDEAD=0; [ -n "$ZCHILD" ] && is_dead "$ZCHILD" && ZDEAD=1
kill -9 "$ZPARENT" 2>/dev/null; wait "$ZPARENT" 2>/dev/null
FIXTURE_PIDS=()   # reaped: never signal a released pid from the EXIT trap
sub "C9.7 HARNESS: an exited process nobody has reaped yet reads DEAD (pid $ZCHILD, dead=$ZDEAD) although kill -0 still finds it (found=$ZFOUND), which is how C9.3 and C9.6 read the holder the moment the land returns" \
    "[ $ZFOUND -eq 1 ] && [ $ZDEAD -eq 1 ]"
SLOWBIN="$T/slow-bin"; mkdir -p "$SLOWBIN"
printf '#!/bin/sh\nsleep 1\nexec "%s" "$@"\n' "$(command -v python3)" > "$SLOWBIN/python3"; chmod +x "$SLOWBIN/python3"
SLOW_PID="$(PATH="$SLOWBIN:$PATH" start_term_ignorer "$T" "$T/slow.ready")"
FIXTURE_PIDS+=("$SLOW_PID")
kill -TERM "$SLOW_PID" 2>/dev/null
SLOW_IGNORES=0; [ -e "$T/slow.ready" ] && ! is_dead "$SLOW_PID" && SLOW_IGNORES=1
kill -9 "$SLOW_PID" 2>/dev/null
while ! is_dead "$SLOW_PID"; do sleep 0.05; done   # SIGKILL is certain; wait for the kernel to finish it
FIXTURE_PIDS=()   # stopped: never signal a released pid from the EXIT trap
sub "C9.8 HARNESS: the holder that ignores TERM is sent one only after it has installed its handler, even through an interpreter that takes a second to start (survived: $SLOW_IGNORES), so C9.6a does not depend on how busy the Mac is" \
    "[ $SLOW_IGNORES -eq 1 ]"
verdict

# ===========================================================================
begin C10 "Landing a two-workspace agent removes both workspaces and both branches in one operation"
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-t1 >/dev/null 2>&1
CCT="$T/other-wt/zach-opus-t1"
spawn "zach-opus-t1" "$(printf 'two workspaces\ncross-repo-worktree: %s\n' "$CCT")"
platform_spawn "zach-opus-t1" "at1t1t1t1t1t1t1t1"
NPT="$ENT/.claude/worktrees/agent-at1t1t1t1t1t1t1t1"
commit_in "$CCT" t1.txt; commit_in "$NPT" t1-native.txt
subagent_stop "at1t1t1t1t1t1t1t1"
git -C "$OTHER" merge -q --no-edit cc/zach-opus-t1
git -C "$ENT" merge -q --no-edit "worktree-agent-at1t1t1t1t1t1t1t1"
REC="$(agent_rec zach-opus-t1)"
sub "C10.1 POSITIVE CONTROL: before the land the agent has both workspaces registered and both exist" \
    "[ \"\$(python3 -c 'import json,sys; print(len([w for w in json.load(open(sys.argv[1]))[\"workspaces\"] if not w.get(\"deleted_at\")]))' '$REC')\" = 2 ] && [ -d '$CCT' ] && [ -d '$NPT' ]"
BEFORE_EVENTS="$(wc -l < "$STORE/events.jsonl")"
ws land zach-opus-t1 >"$T/land.out" 2>&1; rc=$?
sub "C10.2 ONE operation (exit $rc): both workspaces gone" "[ $rc -eq 0 ] && [ ! -e '$CCT' ] && [ ! -e '$NPT' ] && ! listed '$OTHER' '$CCT' && ! listed '$ENT' '$NPT'" "$(cat "$T/land.out")"
sub "C10.3 and both branches gone" "! has_branch '$OTHER' cc/zach-opus-t1 && ! has_branch '$ENT' worktree-agent-at1t1t1t1t1t1t1t1"
NEW_LANDED="$(tail -n +"$((BEFORE_EVENTS + 1))" "$STORE/events.jsonl" | grep -c '"event": "landed"')"
NEW_DELETED="$(tail -n +"$((BEFORE_EVENTS + 1))" "$STORE/events.jsonl" | grep -c '"event": "deleted"')"
sub "C10.4 the store records one land and one deletion covering both (landed=$NEW_LANDED deleted=$NEW_DELETED)" "[ $NEW_LANDED -eq 1 ] && [ $NEW_DELETED -eq 1 ]"
# "...and ANY BRANCH AN AGENT CREATED" (point 3) "...none is left behind" (point 10): a side
# branch made during ONE of the agent's own tool calls — the pair of hooks the platform gives
# every call — is recorded against it and goes with its work. No agent in the round-6 fourteen
# created a side branch (Frank F19); a reviewer's probe found one left behind with the pending
# list empty (certification-frank-attribution-2026-09-12.md §3 B1).
spawn "zach-opus-t2" "create a side branch"
platform_spawn "zach-opus-t2" "at2t2t2t2t2t2t2t2"
NPT2="$ENT/.claude/worktrees/agent-at2t2t2t2t2t2t2t2"
commit_in "$NPT2" t2.txt
agent_call_pre "at2t2t2t2t2t2t2t2" "tu-t2-call-1"; rp=$?
git -C "$NPT2" branch side/t2                              # created INSIDE its own tool call, at its own unlanded tip
agent_call_post "at2t2t2t2t2t2t2t2" "tu-t2-call-1"
RECT2="$(agent_rec zach-opus-t2)"
sub "C10.5 a branch the agent created during one of its own tool calls (PreToolUse $rp, then PostToolUse) is RECORDED against it" \
    "[ $rp -eq 0 ] && grep -q 'side/t2' '$RECT2'" "$(cat "$T/observe.err" 2>/dev/null) $(jget "$RECT2" created_branches)"
subagent_stop "at2t2t2t2t2t2t2t2"
git -C "$ENT" merge -q --no-edit worktree-agent-at2t2t2t2t2t2t2t2
stop_gate; rt=$?
sub "C10.6 and it goes with the work: after the land ($rt) the side branch is gone too — none is left behind" \
    "[ $rt -eq 0 ] && ! has_branch '$ENT' side/t2 && ! has_branch '$ENT' worktree-agent-at2t2t2t2t2t2t2t2 && [ ! -e '$NPT2' ]" "$(cat "$T/stop.err") $(git -C "$ENT" for-each-ref refs/heads/side)"
# A REF CREATED AFTER THE AGENT'S LAST PostToolUse (round 8, item 8). A backgrounded process
# outlives its tool call, so a ref it creates appears after the call's window was consumed;
# the end-of-run signal is the last observation and compares once more against the last
# snapshot the agent took. Until round 8 it observed only windows still OPEN, so this ref
# was attributed to nobody and left behind by a land that reported success — the one RED
# probe on the real manifest (certification-frank-recorded-attribution, outside-stray /
# outside-side; esc-20260912T225456Z-d34bf4e6). Points 3 and 9.
spawn "zach-opus-t3" "create a side branch from a process that outlives its call"
platform_spawn "zach-opus-t3" "at3t3t3t3t3t3t3t3"
NPT3="$ENT/.claude/worktrees/agent-at3t3t3t3t3t3t3t3"
commit_in "$NPT3" t3.txt
agent_call_pre "at3t3t3t3t3t3t3t3" "tu-t3-call-1"; rp3=$?
agent_call_post "at3t3t3t3t3t3t3t3" "tu-t3-call-1"             # the call ends; its window is consumed
git -C "$NPT3" branch side/t3                                # created AFTER the last PostToolUse, at its own unlanded tip
RECT3="$(agent_rec zach-opus-t3)"
T3_BEFORE_END="$(jget "$RECT3" created_branches)"
subagent_stop "at3t3t3t3t3t3t3t3"                            # the end signal: the last observation
sub "C10.7 a branch created AFTER the agent's last PostToolUse (call $rp3 opened and closed; nothing recorded then: [$T3_BEFORE_END]) is attributed to it at its end-of-run signal" \
    "[ $rp3 -eq 0 ] && [ \"$T3_BEFORE_END\" = '[]' ] && grep -q 'side/t3' '$RECT3'" "$(jget "$RECT3" created_branches) $(cat "$T/hooks.err" 2>/dev/null | tail -3)"
git -C "$ENT" merge -q --no-edit worktree-agent-at3t3t3t3t3t3t3t3
stop_gate; rt3=$?
sub "C10.8 and it goes with the work: after the land ($rt3) that branch is gone too — none is left behind" \
    "[ $rt3 -eq 0 ] && ! has_branch '$ENT' side/t3 && ! has_branch '$ENT' worktree-agent-at3t3t3t3t3t3t3t3 && [ ! -e '$NPT3' ]" "$(cat "$T/stop.err") $(git -C "$ENT" for-each-ref refs/heads/side)"
# A BACKGROUNDED CALL WHOSE PROCESS OUTLIVES IT — the shape of the RED probe's own cases: the
# platform stamps the call `run_in_background`, its PostToolUse arrives while the process
# still runs, the process then creates the ref, and the agent's NEXT call opens with the ref
# already there. The background window stays open and is judged, against its OWN before-set,
# at the next observation and every one after it, until agent_hold's own record of the call's
# shell shows the process ended (hunt part 4, finding 6); C10.7c and C10.7d run after it has.
# The stamped field decides, never the text of the command.
spawn "zach-opus-t4" "create a side branch from a backgrounded call"
platform_spawn "zach-opus-t4" "at4t4t4t4t4t4t4t4"
NPT4="$ENT/.claude/worktrees/agent-at4t4t4t4t4t4t4t4"
commit_in "$NPT4" t4.txt
agent_call_pre "at4t4t4t4t4t4t4t4" "tu-t4-bg"; hold_shell_start "at4t4t4t4t4t4t4t4" "tu-t4-bg"; agent_call_post_bg "at4t4t4t4t4t4t4t4" "tu-t4-bg"   # a backgrounded call: Pre, its shell, then its Post while the process runs
git -C "$NPT4" branch side/t4                                # created by that process, after its call's Post
agent_call_pre "at4t4t4t4t4t4t4t4" "tu-t4-call-2"; rp4=$?   # the next call: its snapshot already holds side/t4
RECT4="$(agent_rec zach-opus-t4)"
T4_AT_PRE="$(jget "$RECT4" created_branches)"
agent_call_post "at4t4t4t4t4t4t4t4" "tu-t4-call-2"            # the next observation judges the background window (its shell still runs, so it stays open)
sub "C10.7b a branch created by a BACKGROUNDED call's process after that call's PostToolUse (nothing recorded at the next Pre: [$T4_AT_PRE]) is attributed at the next observation ($rp4), judged against the background window's own before-set" \
    "[ $rp4 -eq 0 ] && grep -q 'side/t4' '$RECT4'" "$(jget "$RECT4" created_branches) $(tail -3 "$T/observe.err" 2>/dev/null)"
hold_shell_end "tu-t4-bg"                                     # the process finishes; the next call's start closes its window
# PRECISION, the other direction (certification-sage-window-and-target case D): with a call
# still OPEN, a ref cut at the agent's tip between two PreToolUse calls is NOT the agent's —
# it may be Rich's, and the union rule under-attributes inside an overlap on purpose.
agent_call_pre "at4t4t4t4t4t4t4t4" "tu-t4-call-3"                 # call 3 opens and never closes
git -C "$ENT" branch rich/rescue-t4 "$(git -C "$NPT4" rev-parse HEAD)"   # Rich, in the main checkout, at the agent's tip
agent_call_pre "at4t4t4t4t4t4t4t4" "tu-t4-call-4"                 # call 4 opens while 3 is still in flight
agent_call_post "at4t4t4t4t4t4t4t4" "tu-t4-call-4"
sub "C10.7c PRECISION: with a call still in flight, a ref cut at the agent's tip between two PreToolUse calls is NOT attributed (it may be Rich's)" \
    "! grep -q 'rich/rescue-t4' '$RECT4' && grep -q 'side/t4' '$RECT4'" "$(jget "$RECT4" created_branches)"
git -C "$ENT" branch -D rich/rescue-t4 >/dev/null                 # Rich's own; out of the fixture
# PRECISION, the bound this round keeps (the engine's own point-8 test and the reviewer's
# control probe): between two ordinary, CLOSED calls with nothing backgrounded, a ref Rich cuts
# at the agent's tip stays Rich's. git cannot tell it from the agent's stray; the platform's
# `run_in_background` stamp is the fact that separates the two, and its absence decides for Rich.
agent_call_pre "at4t4t4t4t4t4t4t4" "tu-t4-call-5"; agent_call_post "at4t4t4t4t4t4t4t4" "tu-t4-call-5"
git -C "$ENT" branch rich/bookmark-t4 "$(git -C "$NPT4" rev-parse HEAD)"  # Rich, between two closed foreground calls
agent_call_pre "at4t4t4t4t4t4t4t4" "tu-t4-call-6"; agent_call_post "at4t4t4t4t4t4t4t4" "tu-t4-call-6"
subagent_stop "at4t4t4t4t4t4t4t4"
sub "C10.7d PRECISION: between two CLOSED foreground calls, a ref Rich cuts at the agent's tip is NOT attributed — nothing the platform stamped says a process of the agent's was still running" \
    "! grep -q 'rich/bookmark-t4' '$RECT4' && grep -q 'side/t4' '$RECT4'" "$(jget "$RECT4" created_branches)"
git -C "$ENT" branch -D rich/bookmark-t4 >/dev/null
git -C "$ENT" merge -q --no-edit worktree-agent-at4t4t4t4t4t4t4t4
stop_gate >/dev/null 2>&1
# "ALL of an agent's workspaces go together... EVERY workspace and branch it has"
# (point 10). The sentence's "two" is its example; the rule is every one. Until
# 2026-09-17 spawn.sh took one --repo, so a teammate needing a second repository
# was given a SECOND NAME — a second agent, landed separately, and one of those
# strays (`cc/norm-opus-wireguide1`) was found by the CEO himself. These four
# sub-assertions are the same check as C10.1-C10.3 with THREE workspaces, in
# THREE repositories, which is the shape nothing measured before.
OTHER2="$T/other2"; new_repo "$OTHER2"
ws integration --repo "$OTHER2" --branch main --why "the fourteen checks, a second other repository" >/dev/null 2>&1
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-t5 >/dev/null 2>&1
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER2" zach-opus-t5 >/dev/null 2>&1
CCT5A="$T/other-wt/zach-opus-t5"; CCT5B="$T/other2-wt/zach-opus-t5"
spawn "zach-opus-t5" "$(printf 'three workspaces\ncross-repo-worktree: %s\ncross-repo-worktree: %s\n' "$CCT5A" "$CCT5B")"; rc5=$?
platform_spawn "zach-opus-t5" "at5t5t5t5t5t5t5t5"
NPT5="$ENT/.claude/worktrees/agent-at5t5t5t5t5t5t5t5"
commit_in "$CCT5A" t5a.txt; commit_in "$CCT5B" t5b.txt; commit_in "$NPT5" t5-native.txt
subagent_stop "at5t5t5t5t5t5t5t5"
git -C "$OTHER" merge -q --no-edit cc/zach-opus-t5
git -C "$OTHER2" merge -q --no-edit cc/zach-opus-t5
git -C "$ENT" merge -q --no-edit "worktree-agent-at5t5t5t5t5t5t5t5"
REC5="$(agent_rec zach-opus-t5)"
sub "C10.9 POSITIVE CONTROL: ONE name, THREE registered workspaces in three repositories, all present and the spawn naming both marker lines registered ($rc5)" \
    "[ $rc5 -eq 0 ] && [ \"\$(python3 -c 'import json,sys; print(len([w for w in json.load(open(sys.argv[1]))[\"workspaces\"] if not w.get(\"deleted_at\")]))' '$REC5')\" = 3 ] && [ -d '$CCT5A' ] && [ -d '$CCT5B' ] && [ -d '$NPT5' ]" \
    "$(cat "$T/spawn.err") $(cat "$REC5" 2>/dev/null | tr '\n' ' ' | cut -c1-300)"
ws land zach-opus-t5 >"$T/land5.out" 2>&1; rc=$?
sub "C10.10 ONE operation (exit $rc): ALL THREE workspaces are gone, in all three repositories — none is left behind" \
    "[ $rc -eq 0 ] && [ ! -e '$CCT5A' ] && [ ! -e '$CCT5B' ] && [ ! -e '$NPT5' ] && ! listed '$OTHER' '$CCT5A' && ! listed '$OTHER2' '$CCT5B' && ! listed '$ENT' '$NPT5'" "$(cat "$T/land5.out")"
sub "C10.11 and all three branches are gone" \
    "! has_branch '$OTHER' cc/zach-opus-t5 && ! has_branch '$OTHER2' cc/zach-opus-t5 && ! has_branch '$ENT' worktree-agent-at5t5t5t5t5t5t5t5"
# The other of point 7's two endings, on the same shape: a DISCARD deletes every
# one of them too, and nothing it wrote reaches the branch this work integrates on.
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER" zach-opus-t6 >/dev/null 2>&1
RICHOS_SESSION_ID="$CUR_SID" bash "$CREATE" "$OTHER2" zach-opus-t6 >/dev/null 2>&1
CCT6A="$T/other-wt/zach-opus-t6"; CCT6B="$T/other2-wt/zach-opus-t6"
spawn "zach-opus-t6" "$(printf 'three workspaces, discarded\ncross-repo-worktree: %s\ncross-repo-worktree: %s\n' "$CCT6A" "$CCT6B")"
platform_spawn "zach-opus-t6" "at6t6t6t6t6t6t6t6"
NPT6="$ENT/.claude/worktrees/agent-at6t6t6t6t6t6t6t6"
commit_in "$CCT6A" t6a.txt; commit_in "$CCT6B" t6b.txt; commit_in "$NPT6" t6-native.txt
subagent_stop "at6t6t6t6t6t6t6t6"
ws discard zach-opus-t6 --reason "the reviewer rejected this approach" \
    --not-ceo-ordered "an internal experiment, not the CEO's order" >"$T/discard6.out" 2>&1; rc=$?
sub "C10.12 a DISCARD deletes every one of a three-workspace agent's workspaces and branches too (exit $rc), and nothing it wrote is in any integration branch (point 7)" \
    "[ $rc -eq 0 ] && [ ! -e '$CCT6A' ] && [ ! -e '$CCT6B' ] && [ ! -e '$NPT6' ] && ! has_branch '$OTHER' cc/zach-opus-t6 && ! has_branch '$OTHER2' cc/zach-opus-t6 && ! has_branch '$ENT' worktree-agent-at6t6t6t6t6t6t6t6 && ! git -C '$OTHER2' log --format=%s | grep -q 'work t6b.txt'" \
    "$(cat "$T/discard6.out")"
verdict

# ===========================================================================
begin C11 "A SubagentStop for a sub-run does not finish the teammate; a pause is recorded with what ends it; a pause naming nothing appears in point 5's list"
spawn "zach-opus-s1" "long work with sub-runs"
platform_spawn "zach-opus-s1" "as1s1s1s1s1s1s1s1"
NPS="$ENT/.claude/worktrees/agent-as1s1s1s1s1s1s1s1"
commit_in "$NPS" s1.txt
subagent_stop "asubrun00000000001"                          # a sub-run's own id, not the teammate's
subagent_stop "asubrun00000000002"
barrier "as1s1s1s1s1s1s1s1" Edit; r1=$?
stop_gate; r2=$?
sub "C11.1 two SubagentStops carrying sub-run ids do not finish the teammate: it may still write ($r1) and nothing is pending ($r2)" \
    "[ $r1 -eq 0 ] && [ $r2 -eq 0 ]" "$(cat "$T/barrier.err" "$T/stop.err")"
send_message "zach-opus-s1" "$(printf 'commit and hold\npause-until: the CEO'"'"'s answer\n')"
REC="$(agent_rec zach-opus-s1)"
sub "C11.2 a pause sent to it is RECORDED with what ends it" "[ \"\$(jget '$REC' pause.until)\" = \"the CEO's answer\" ]" "$(jget "$REC" pause)"
subagent_stop "as1s1s1s1s1s1s1s1"                           # its own run ends while paused
barrier "as1s1s1s1s1s1s1s1" Edit; r3=$?
stop_gate; r4=$?
sub "C11.3 its own end of run while PAUSED is not finished: not locked out ($r3), not pending ($r4)" "[ $r3 -eq 0 ] && [ $r4 -eq 0 ]" "$(cat "$T/barrier.err" "$T/stop.err")"
send_message "zach-opus-s1" "go on, here is his answer"
barrier "as1s1s1s1s1s1s1s1" Edit; r5=$?
sub "C11.4 a later message resumes it ($r5)" "[ $r5 -eq 0 ] && [ -z \"\$(jget '$REC' pause)\" ]" "$(jget "$REC" pause)"
ws pause zach-opus-s1 --until "" >"$T/pause.out" 2>&1
subagent_stop "as1s1s1s1s1s1s1s1"
stop_gate; r6=$?
sub "C11.5 a pause naming NOTHING is pending work under point 5: the turn cannot end and it is named" \
    "[ $r6 -eq 0 ] && due_has 'zach-opus-s1' && grep -q -i 'nothing named' '$T/holds.out'" "$(cat "$T/pause.out" "$T/stop.err")"
ws stop zach-opus-s1 --why "its work is no longer wanted" >/dev/null 2>&1
subagent_stop "as1s1s1s1s1s1s1s1"
barrier "as1s1s1s1s1s1s1s1" Edit; r7=$?
git -C "$ENT" merge -q --no-edit "worktree-agent-as1s1s1s1s1s1s1s1"
stop_gate; r8=$?
sub "C11.6 stopped: it is finished ($r7 = refused), and its own SubagentStop carrying ITS id is the end-of-run signal; landed once merged ($r8)" \
    "[ $r7 -eq 2 ] && [ $r8 -eq 0 ] && [ ! -e '$NPS' ]" "$(cat "$T/stop.err")"
# "An agent that ends after handing in its work is finished even if a pause was sent."
spawn "zach-opus-h2" "hand in, get paused, then end"
platform_spawn "zach-opus-h2" "ah2h2h2h2h2h2h2h2"
NPH2="$ENT/.claude/worktrees/agent-ah2h2h2h2h2h2h2h2"
commit_in "$NPH2" h2.txt
task_completed "ah2h2h2h2h2h2h2h2" "zach-opus-h2"          # TaskCompleted: it handed in its work
send_message "zach-opus-h2" "$(printf 'commit and hold\npause-until: the CEO'"'"'s answer\n')"
subagent_stop "ah2h2h2h2h2h2h2h2"                          # its own run ends after the hand-in, pause and all
barrier "ah2h2h2h2h2h2h2h2" Edit; rh1=$?
stop_gate; rh2=$?
sub "C11.7 an agent that ends AFTER HANDING IN its work is finished even though a pause was sent: locked out ($rh1) and due ($rh2), by name" \
    "[ $rh1 -eq 2 ] && [ $rh2 -eq 0 ] && due_has 'zach-opus-h2'" "$(cat "$T/barrier.err" "$T/stop.err")"
git -C "$ENT" merge -q --no-edit worktree-agent-ah2h2h2h2h2h2h2h2
stop_gate >/dev/null 2>&1
# HANDED IN AND STILL RUNNING (round 8, item 5; brief-audit-frank-round8 §5 F-F). "'Finished'
# means the agent's run has ended": between TaskCompleted and its own SubagentStop the agent
# is not finished — it may still write and nothing is pending. C11.7 called task_completed
# and subagent_stop back to back, so a finished_state that read handed_in BEFORE the end
# signal survived it.
spawn "zach-opus-h3" "hand in, then keep running"
platform_spawn "zach-opus-h3" "ah3h3h3h3h3h3h3h3"
NPH3="$ENT/.claude/worktrees/agent-ah3h3h3h3h3h3h3h3"
commit_in "$NPH3" h3.txt
task_completed "ah3h3h3h3h3h3h3h3" "zach-opus-h3"          # handed in; its run has NOT ended
barrier "ah3h3h3h3h3h3h3h3" Edit; rh3=$?
stop_gate; rh4=$?
sub "C11.8 handed in and STILL RUNNING: not finished — it may still write ($rh3) and nothing is pending ($rh4); only its own end of run finishes it" \
    "[ $rh3 -eq 0 ] && [ $rh4 -eq 0 ] && ! due_has 'zach-opus-h3'" "$(cat "$T/barrier.err" "$T/stop.err")"
subagent_stop "ah3h3h3h3h3h3h3h3"
barrier "ah3h3h3h3h3h3h3h3" Edit; rh5=$?
sub "C11.8b and after its own end of run it is finished ($rh5 = refused), as C11.7" "[ $rh5 -eq 2 ]" "$(cat "$T/barrier.err")"
git -C "$ENT" merge -q --no-edit worktree-agent-ah3h3h3h3h3h3h3h3
stop_gate >/dev/null 2>&1
verdict

# ===========================================================================
begin C5 "Pending integration allows unrelated spawns and turn ends; the work stays owned and due (amended 2026-10-24, orchestration reliability R2/R3)"
spawn "zach-opus-a1" "finish without being merged"
platform_spawn "zach-opus-a1" "aa1a1a1a1a1a1a1a1"
NPA="$ENT/.claude/worktrees/agent-aa1a1a1a1a1a1a1a1"
commit_in "$NPA" a1.txt
subagent_stop "aa1a1a1a1a1a1a1a1"
spawn "zach-opus-b1" "unrelated new work"; r1=$?
stop_gate; r2=$?
sub "C5.1 an unrelated spawn is allowed ($r1), leaving the finished workspace pending" \
    "[ $r1 -eq 0 ] && [ -e '$NPA' ]" "$(cat "$T/spawn.err")"
sub "C5.1b and the turn may end ($r2) with the agent due, by name" \
    "[ $r2 -eq 0 ] && due_has 'zach-opus-a1'" "$(cat "$T/stop.err")"
# The one answer allowance (C5.2-C5.5, C5.15, C5.16) is gone with the refusal it rationed
# (orchestration reliability, section 3): a turn end is never refused, so there is nothing to allow.
spawn "zach-opus-h1" "$(printf 'merge and land it\nlands-pending: zach-opus-a1\n')"; r7=$?
sub "C5.6 ALLOWANCE 2: work whose only purpose is landing it may start ($r7)" "[ $r7 -eq 0 ]" "$(cat "$T/spawn.err")"
stop_gate; r8=$?
sub "C5.7 and with that work started, the turn may end ($r8) while the item waits on it, quiet (not due)" "[ $r8 -eq 0 ] && ! due_has 'zach-opus-a1'" "$(cat "$T/stop.out" "$T/stop.err" "$T/holds.out")"
spawn "zach-opus-b2" "unrelated new work"; r9=$?
sub "C5.8 unrelated new work may start while the land helper runs ($r9)" "[ $r9 -eq 0 ]" "$(cat "$T/spawn.err")"
platform_spawn "zach-opus-h1" "ah1h1h1h1h1h1h1h1"
git -C "$ENT" merge -q --no-edit "worktree-agent-aa1a1a1a1a1a1a1a1"
subagent_stop "ah1h1h1h1h1h1h1h1"
stop_gate; r10=$?
sub "C5.9 merged: both are landed on their own and the turn may end ($r10)" "[ $r10 -eq 0 ] && [ ! -e '$NPA' ] && [ -n \"\$(done_rec zach-opus-a1)\" ] && [ -n \"\$(done_rec zach-opus-h1)\" ]" "$(cat "$T/stop.err")"
spawn "zach-opus-a2" "finish without being merged, again"
platform_spawn "zach-opus-a2" "aa2a2a2a2a2a2a2a2"
NPA2="$ENT/.claude/worktrees/agent-aa2a2a2a2a2a2a2a2"
commit_in "$NPA2" a2.txt
subagent_stop "aa2a2a2a2a2a2a2a2"
# "...or waiting on something outside his reach (the CEO's word, a service that is down); the
# latter goes on the CEO's TODO list." Independent development may continue.
ws wait zach-opus-a2 --outside "GitHub is down, the merge cannot be pushed" >"$T/wait.out" 2>&1; rw1=$?
sub "C5.11 an item waiting on something OUTSIDE his reach with no CEO-TODO reference is refused ($rw1): 'the latter goes on the CEO's TODO list'" \
    "[ $rw1 -eq 2 ] && grep -q 'TODO' '$T/wait.out'" "$(cat "$T/wait.out")"
ws wait zach-opus-a2 --outside "GitHub is down, the merge cannot be pushed" --todo "CEO-TODOs 7.1" >"$T/wait.out" 2>&1; rw2=$?
stop_gate; rw3=$?
spawn "zach-opus-b3" "unrelated new work"; rw4=$?
sub "C5.12 recorded with its TODO reference ($rw2): the turn may end ($rw3), and independent work may start ($rw4)" \
    "[ $rw2 -eq 0 ] && [ $rw3 -eq 0 ] && [ $rw4 -eq 0 ]" "$(cat "$T/wait.out" "$T/stop.err" "$T/spawn.err")"
git -C "$ENT" merge -q --no-edit worktree-agent-aa2a2a2a2a2a2a2a2
stop_gate >/dev/null 2>&1
# A discard waiting on the CEO remains recorded. Independent spawns and other
# pending lands continue under the 2026-09-30 amendment to point 5.
spawn "zach-opus-cw2" "a second body of work, running while nothing is pending"
platform_spawn "zach-opus-cw2" "acw2cw2cw2cw2cw2c"
NPCW2="$ENT/.claude/worktrees/agent-acw2cw2cw2cw2cw2c"
commit_in "$NPCW2" cw2.txt
spawn "zach-opus-cw" "$(printf 'work he ordered\nceo-ordered: build it, his words 2026-09-12\n')"
platform_spawn "zach-opus-cw" "acwcwcwcwcwcwcwcw"
NPCW="$ENT/.claude/worktrees/agent-acwcwcwcwcwcwcwcw"
commit_in "$NPCW" cw.txt
subagent_stop "acwcwcwcwcwcwcwcw"
ws wait zach-opus-cw --ceo "May I discard it? A reviewer rejected it." --todo "CEO-TODOs 7.2" >"$T/wait.out" 2>&1; rc1=$?
stop_gate; rc2=$?
spawn "zach-opus-b4" "unrelated new work while he is being asked"; rc3=$?
sub "C5.13 a discard waiting on the CEO stays named with its TODO ($rc1); the turn may end ($rc2) and independent work may start ($rc3)" \
    "[ $rc1 -eq 0 ] && [ $rc2 -eq 0 ] && due_has 'zach-opus-cw' && [ $rc3 -eq 0 ]" "$(cat "$T/wait.out" "$T/stop.err" "$T/stop.out" "$T/spawn.err")"
subagent_stop "acw2cw2cw2cw2cw2c"
git -C "$ENT" merge -q --no-edit worktree-agent-acw2cw2cw2cw2cw2c
stop_gate; rc4=$?
DCW2="$(done_rec zach-opus-cw2)"
sub "C5.14 'blocks nothing else': the second finished item lands on its own ($rc4) while the first still waits on him — its workspace gone, its ending 'landed', the first still pending by name" \
    "[ $rc4 -eq 0 ] && [ ! -e '$NPCW2' ] && [ -n \"$DCW2\" ] && [ \"\$(jget '$DCW2' disposition.kind)\" = landed ] && [ -e '$NPCW' ] && due_has 'zach-opus-cw'" "$(cat "$T/stop.err" "$T/stop.out") done=$DCW2"
ws discard zach-opus-cw --reason "he said drop it, check 5 fixture" --ceo-word "drop it, he said 2026-09-12" >/dev/null 2>&1
verdict

# ===========================================================================
begin C13 "A deletion blocked by a held file is retried without involvement and succeeds when the hold clears; the CEO hears only after a stated number of failures"
spawn "zach-opus-k1" "leave a held file"
platform_spawn "zach-opus-k1" "ak1k1k1k1k1k1k1k1"
NPK="$ENT/.claude/worktrees/agent-ak1k1k1k1k1k1k1k1"
commit_in "$NPK" k1.txt
printf 'held by the OS\n' > "$NPK/pinned.txt"; git -C "$NPK" add pinned.txt; git -C "$NPK" commit -q -m pinned
if command -v chflags >/dev/null 2>&1; then chflags uchg "$NPK/pinned.txt"; HOLD=uchg; else chmod 555 "$(dirname "$NPK")"; HOLD=chmod; fi
subagent_stop "ak1k1k1k1k1k1k1k1"
git -C "$ENT" merge -q --no-edit "worktree-agent-ak1k1k1k1k1k1k1k1"
stop_gate; rc=$?
REC="$(agent_rec zach-opus-k1)"
A1="$(jget "$REC" deletion.attempts)"
sub "C13.1 the land is recorded ($rc, landed) and the deletion FAILED on the held file: attempt 1, workspace still there, retry scheduled" \
    "[ $rc -eq 0 ] && [ \"\$(jget '$REC' disposition.kind)\" = landed ] && [ \"$A1\" = 1 ] && [ -e '$NPK' ] && [ -n \"\$(jget '$REC' deletion.next_at)\" ]" "hold=$HOLD attempts=$A1 $(jget "$REC" deletion)"
sub "C13.2 the CEO does NOT hear of it yet (no 'TELL THE CEO' at attempt 1)" "! grep -q 'TELL THE CEO' '$T/stop.out' '$T/stop.err'" "$(cat "$T/stop.out")"
STATED="$(sed -n 's/^RETRY_TELL_CEO_AFTER = int(os.environ.get("RICHOS_WORKSPACES_RETRY_TELL_CEO", "\([0-9]*\)")).*/\1/p' "$ENGINE/mega-lander/workspaces.py")"
subagent_stop "anothersubrun0001"                          # hook events, no command: each one retries what is due
subagent_stop "anothersubrun0002"
# As in C13.6: under load a hook event may defer the retry for its budget,
# which counts no attempt, so further hook events (still no command) deliver
# the two retries. An event that neither attempts nor defers ends the wait.
C13_UNTIL=$((SECONDS + 30))
while [ "$(jget "$REC" deletion.attempts)" -lt 3 ] && [ "$SECONDS" -lt "$C13_UNTIL" ]; do
    [ -n "$(jget "$REC" deletion.deferred)" ] || break
    subagent_stop "anothersubrun0003"
done
A3="$(jget "$REC" deletion.attempts)"
sub "C13.3 retried WITHOUT involvement: two unrelated hook events later the attempt count is $A3 (no command was run)" "[ \"$A3\" -ge 3 ]"
stop_gate >/dev/null 2>&1
A4="$(jget "$REC" deletion.attempts)"
sub "C13.4 still below the stated number ($STATED): the CEO is not told at attempt $A4" "[ \"$A4\" -lt \"$STATED\" ] && ! grep -q 'TELL THE CEO' '$T/stop.out'" "$(cat "$T/stop.out")"
i=0; while [ "$(jget "$REC" deletion.attempts)" -lt "$STATED" ] && [ $i -lt 10 ]; do subagent_stop "anothersubrun00$i"; i=$((i + 1)); done
stop_gate; rc=$?
AN="$(jget "$REC" deletion.attempts)"
sub "C13.5 at the stated number of failures ($STATED, now $AN) the turn-end gate tells the CEO, by name, and still lets the turn end ($rc)" \
    "[ \"$AN\" -ge \"$STATED\" ] && grep -q 'TELL THE CEO' '$T/stop.out' && grep -q 'zach-opus-k1' '$T/stop.out' && [ $rc -eq 0 ]" "$(cat "$T/stop.out" "$T/stop.err")"
if [ "$HOLD" = uchg ]; then chflags nouchg "$NPK/pinned.txt"; else chmod 755 "$(dirname "$NPK")"; fi
C13_ATTEMPTS="$(jget "$REC" deletion.attempts)"
subagent_stop "anothersubrun0099"                           # the hold clears; the next hook event retries
# A SubagentStop gives cleanup two seconds. Under load that event may
# explicitly defer the retry before deletion can finish. A deferral is not
# an attempted deletion (workspaces.py _delete), so let subsequent real
# hook events deliver the next attempt. Never retry an actual failure here.
C13_UNTIL=$((SECONDS + 30))
while [ -n "$(agent_rec zach-opus-k1)" ] && [ "$SECONDS" -lt "$C13_UNTIL" ]; do
    [ "$(jget "$REC" deletion.attempts)" = "$C13_ATTEMPTS" ] && \
        [ -n "$(jget "$REC" deletion.deferred)" ] || break
    subagent_stop "anothersubrun0100"
done
sub "C13.6 when the hold clears, the next retry succeeds on its own: workspace and branch gone, nothing left retrying" \
    "[ ! -e '$NPK' ] && ! has_branch '$ENT' worktree-agent-ak1k1k1k1k1k1k1k1 && [ -z \"\$(agent_rec zach-opus-k1)\" ] && [ -n \"\$(done_rec zach-opus-k1)\" ]" "$(ls "$NPK" 2>&1 | head -3) $(jget "$REC" deletion)"
verdict

# ===========================================================================
begin C12 "The session is recorded at start; its end is read from the process start-time, never a pid alone; the next session reports previous finished work while allowing independent spawns"
S="$STORE/sessions/$CUR_SID.json"
sub "C12.1 SessionStart recorded the session with its process number AND its process start time" \
    "[ -n \"\$(jget '$S' pid)\" ] && [ -n \"\$(jget '$S' pid_start)\" ]" "$(cat "$S" 2>/dev/null | head -c 300)"
spawn "zach-opus-w1" "work that outlives nothing"
platform_spawn "zach-opus-w1" "aw1w1w1w1w1w1w1w1"
NPW="$ENT/.claude/worktrees/agent-aw1w1w1w1w1w1w1w1"
commit_in "$NPW" w1.txt
ORIG_START="$(jget "$S" pid_start)"
python3 - "$S" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); d["pid_start"] = "Thu Jan  1 00:00:00 1970"; json.dump(d, open(p, "w"))
PY
barrier "aw1w1w1w1w1w1w1w1" Edit; r1=$?
sub "C12.2 NEVER A PID ALONE: the same live pid with a different recorded start time is a different process — the session counts as ended and its agent is locked out ($r1)" \
    "[ $r1 -eq 2 ] && grep -q -i 'session' '$T/barrier.err'" "$(cat "$T/barrier.err")"
python3 - "$S" "$ORIG_START" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); d["pid_start"] = sys.argv[2]; json.dump(d, open(p, "w"))
PY
barrier "aw1w1w1w1w1w1w1w1" Edit; r2=$?
sub "C12.3 POSITIVE CONTROL: with the true start time restored the agent is live again ($r2)" "[ $r2 -eq 0 ]"
OLD_PID="$RICHOS_SESSION_PID"
kill "$OLD_PID"; while kill -0 "$OLD_PID" 2>/dev/null; do sleep 0.1; done
start_session "sess-fourteen-2222"
sub "C12.4 the process is gone, read from the OS: the NEXT session is told first, by name" \
    "grep -q 'zach-opus-w1' '$T/last-start.json' && grep -q 'DUE WORK' '$T/last-start.json'" "$(cat "$T/last-start.json" "$T/last-start.err")"
spawn "zach-opus-x2" "new work in the new session"; r3=$?
sub "C12.5 independent work starts while the previous session's finished workspace is preserved ($r3)" "[ $r3 -eq 0 ] && [ -e '$NPW' ]" "$(cat "$T/spawn.err")"
git -C "$ENT" merge -q --no-edit "worktree-agent-aw1w1w1w1w1w1w1w1"
ws land zach-opus-w1 >"$T/land.out" 2>&1; r4=$?
spawn "zach-opus-x2" "new work in the new session"; r5=$?
platform_spawn "zach-opus-x2" "ax2x2x2x2x2x2x2x2"; subagent_stop "ax2x2x2x2x2x2x2x2"; stop_gate >/dev/null 2>&1
sub "C12.6 landed by the next session ($r4): its workspace is gone and new work may start ($r5)" "[ $r4 -eq 0 ] && [ ! -e '$NPW' ] && [ $r5 -eq 0 ]" "$(cat "$T/land.out" "$T/spawn.err")"
# "While two sessions run at once, each handles only the agents it started." Round 6's check 12
# never had two live sessions (Frank F10).
PID_2222="$RICHOS_SESSION_PID"
start_session "sess-fourteen-3333"                        # a THIRD session, live at the same time as the second
PID_3333="$RICHOS_SESSION_PID"
spawn "zach-opus-y1" "work in the third session"
platform_spawn "zach-opus-y1" "ay1y1y1y1y1y1y1y1"
NPY1="$ENT/.claude/worktrees/agent-ay1y1y1y1y1y1y1y1"
commit_in "$NPY1" y1.txt
subagent_stop "ay1y1y1y1y1y1y1y1"                          # finished, unlanded, in the third session
CUR_SID="sess-fourteen-2222"; export RICHOS_SESSION_PID="$PID_2222"
stop_gate; ry1=$?
spawn "zach-opus-x3" "new work in the second session"; ry2=$?
sub "C12.7 TWO LIVE SESSIONS: the second neither lists nor lands the third's finished agent — its turn may end ($ry1) and it may start new work ($ry2); the workspace is untouched" \
    "[ $ry1 -eq 0 ] && ! due_has 'zach-opus-y1' && [ $ry2 -eq 0 ] && [ -e '$NPY1' ]" "$(cat "$T/stop.err" "$T/spawn.err")"
platform_spawn "zach-opus-x3" "ax3x3x3x3x3x3x3x3"; subagent_stop "ax3x3x3x3x3x3x3x3"; stop_gate >/dev/null 2>&1
CUR_SID="sess-fourteen-3333"; export RICHOS_SESSION_PID="$PID_3333"
stop_gate; ry3=$?
sub "C12.8 POSITIVE CONTROL: the third session, which started it, cannot end its turn while it is pending ($ry3)" \
    "[ $ry3 -eq 0 ] && due_has 'zach-opus-y1'" "$(cat "$T/stop.err")"
git -C "$ENT" merge -q --no-edit worktree-agent-ay1y1y1y1y1y1y1y1
stop_gate >/dev/null 2>&1
# "A session has ended when it RECORDED ITS END or when its process no longer exists." Round 6
# exercised only the process path (C12.4); the recorded end, with the process still alive:
spawn "zach-opus-y2" "work in a session that will record its end"
platform_spawn "zach-opus-y2" "ay2y2y2y2y2y2y2y2"
NPY2="$ENT/.claude/worktrees/agent-ay2y2y2y2y2y2y2y2"
commit_in "$NPY2" y2.txt
session_end "sess-fourteen-3333"                           # recorded; its sleeper is still alive
barrier "ay2y2y2y2y2y2y2y2" Edit; ry4=$?
ALIVE_3333=0; kill -0 "$PID_3333" 2>/dev/null && ALIVE_3333=1
sub "C12.9 a session that RECORDED its end has ended even though its process still exists (pid $PID_3333 alive=$ALIVE_3333): its agent is finished and locked out ($ry4)" \
    "[ $ry4 -eq 2 ] && [ $ALIVE_3333 -eq 1 ] && grep -q -i 'session' '$T/barrier.err'" "$(cat "$T/barrier.err")"
CUR_SID="sess-fourteen-2222"; export RICHOS_SESSION_PID="$PID_2222"
stop_gate; ry5=$?
sub "C12.10 and the running session takes its finished work over: pending by name ($ry5)" "[ $ry5 -eq 0 ] && due_has 'zach-opus-y2'" "$(cat "$T/stop.err")"
git -C "$ENT" merge -q --no-edit worktree-agent-ay2y2y2y2y2y2y2y2
ws land zach-opus-y2 >"$T/land.out" 2>&1; ry6=$?
sub "C12.11 landed by the running session ($ry6): workspace gone" "[ $ry6 -eq 0 ] && [ ! -e '$NPY2' ]" "$(cat "$T/land.out")"
verdict

# ===========================================================================
begin C0 "ADDED: the sandbox ends clean — every ending in the store reads landed or discarded, no agent workspace or branch survives in any repository, the hooks reported nothing unexpected"
KINDS="$(python3 -c 'import json,os,sys; d=sys.argv[1]; print(sorted(set(json.load(open(os.path.join(d,f)))["disposition"]["kind"] for f in os.listdir(d))))' "$STORE/done")"
sub "C0.1 every record in done/ ends 'landed' or 'discarded' (kinds: $KINDS)" "[ \"$KINDS\" = \"['discarded', 'landed']\" ]"
LIVE="$(python3 -c 'import json,os,sys; d=sys.argv[1]; print(sorted(f for f in os.listdir(d) if any(not w.get("deleted_at") for w in json.load(open(os.path.join(d,f))).get("workspaces") or [])))' "$STORE/agents")"
sub "C0.2 no agent still holds a workspace in agents/ (live: $LIVE)" "[ \"$LIVE\" = '[]' ]"
sub "C0.3 the entity repository has only its main checkout" "[ \"\$(git -C '$ENT' worktree list --porcelain | grep -c '^worktree ')\" = 1 ]"
sub "C0.4 the other repository has only its main checkout and the untouched codex/ workspace" "[ \"\$(git -C '$OTHER' worktree list --porcelain | grep -c '^worktree ')\" = 2 ] && listed '$OTHER' '$CX'"
sub "C0.5 no agent branch survives anywhere (codex/ excepted)" \
    "[ -z \"\$(git -C '$ENT' for-each-ref refs/heads/worktree-agent-* refs/heads/cc; git -C '$OTHER' for-each-ref refs/heads/cc refs/heads/worktree-agent-*; git -C '$DEV' for-each-ref refs/heads/cc)\" ]" "$(git -C "$ENT" for-each-ref refs/heads/worktree-agent-* refs/heads/cc; git -C "$OTHER" for-each-ref refs/heads/cc)"
if [ -s "$T/hooks.err" ]; then
    sub "C0.6 the lifecycle hook reported nothing unexpected" "false" "$(head -5 "$T/hooks.err")"
else
    sub "C0.6 the lifecycle hook reported nothing unexpected" "true"
fi
verdict

# --- THE MUTATION HARNESS -------------------------------------------------
# A green check is evidence of nothing until it has been watched going red for
# its own reason; the harness removes one rule at a time in a throwaway copy of
# the engine and demands the named SUB-ASSERTION go red. Its verdict is printed
# here and never folded into the headline below: the headline counts the CEO's
# checks, and the harness counts the checks' own honesty.
#
# ONLY BEFORE NIGHTLIES (CEO, 2026-09-23, "Only before nightlies", esc-20260923T123440Z-b5371531).
# The pass costs about twelve minutes of the whole Mac (792 s for this unit, measured that day)
# and it ran on every land that touched the workspace code; the fourteen checks above still do.
# The nightly's gates/workspace-mutants gate (richos/app/scripts/nightly-local.py) runs this unit
# with RICHOS_FOURTEEN_MUTANTS=1, where a failed or unproven mutant stops the nightly. Anywhere
# else the pass says NOT RUN and why; set the variable to run it by hand.
MUT_RC=""
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$HERE/workspace-spec-fourteen.mutation.sh" ]; then
    if [ "${RICHOS_FOURTEEN_MUTANTS:-}" = 1 ]; then
        echo "=== running the mutation harness ==="
        bash "$HERE/workspace-spec-fourteen.mutation.sh"; MUT_RC=$?
        echo ""
    else
        echo "  NOT RUN  the mutation harness (79 mutants): it runs before each nightly (CEO 2026-09-23, \"Only before nightlies\"); RICHOS_FOURTEEN_MUTANTS=1 runs it here"
        echo ""
    fi
fi

echo "CHECKS RUN: $CHECKS_RUN  RED: $CHECKS_RED"
[ -n "$MUT_RC" ] && echo "MUTATION HARNESS: exit $MUT_RC ($([ "$MUT_RC" -eq 0 ] && echo 'every property proven load-bearing' || echo 'a property is NOT proven — see above'))"
# The CEO-facing number and the harness's own self-check (C0), apart: C0 is real — it caught a
# quarantine C4 missed in round 6 — but it tests no sentence of his page and is not one of his
# fourteen (round 7, brief §7).
echo "FOURTEEN: $((FOURTEEN_RUN - FOURTEEN_RED)) green, $FOURTEEN_RED red · self-check: $([ "$SELF_RED" -eq 0 ] && echo green || echo red)"
[ "$CHECKS_RED" -eq 0 ] && [ "${MUT_RC:-0}" -eq 0 ]
