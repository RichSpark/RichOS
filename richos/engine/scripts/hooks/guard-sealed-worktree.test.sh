#!/usr/bin/env bash
#
# guard-sealed-worktree.test.sh — behavioral tests for THE LOCK-OUT,
# scripts/hooks/guard-sealed-worktree.sh (docs/plans/worktree-spec-2026-09-11.md,
# points 3, 9, 11, 12).
#
# What is proven, each refusal beside its pass: the lead's own calls pass; a
# registered worker passes; a FINISHED worker — by its end-of-run signal, by its
# session's recorded end, by its session's process no longer existing — is
# refused EVERY tool, Read included, whatever its declared type, and still after
# its workspace is gone; a PAUSED worker is not locked out; a worker with no
# registration may use only the read-only allowlist; a read-only agent type
# with no registration is exempt; the lead of a claude --worktree session may
# only read; and the guard FAILS CLOSED on its own error (the registry missing,
# python3 missing, an unparseable payload), passing only a proven lead call or a
# proven read-only tool.
#
# Everything runs in a sandbox: HOME and CLAUDE_CONFIG_DIR redirected, a real
# `sleep` process as the session. The mutation harness proving each assertion
# load-bearing is scripts/hooks/guard-sealed-worktree.mutation.sh, run at the end.
#
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/guard-sealed-worktree.sh"
WS_PY="$SCRIPT_DIR/../../mega-lander/workspaces.py"
unset CLAUDE_PROJECT_DIR RICHOS_WORKSPACES_DIR RICHOS_SESSION_ID

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t guard-lockout-test.XXXXXX)" && pwd -P)"
SESS_PIDS=""
cleanup() { for p in $SESS_PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$SANDBOX"; }
trap cleanup EXIT
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -x "$HOOK" ] || { echo "FATAL: $HOOK missing/non-executable" >&2; exit 1; }

export HOME="$SANDBOX/home" CLAUDE_CONFIG_DIR="$SANDBOX/home/.claude"
mkdir -p "$CLAUDE_CONFIG_DIR"
export GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid \
       GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
export SEAL_WAIT_SECONDS=0 RICHOS_WORKSPACES_SPAWN_WINDOW=0
SID="deadbeef-0000-4000-8000-000000000000"

ENTITY="$SANDBOX/entity"
mkdir -p "$ENTITY"
git -C "$ENTITY" init -q -b main
printf 'seed\n' >"$ENTITY/seed.txt"; printf '.claude/\n' >"$ENTITY/.gitignore"
sed 's/^SEAL_WAIT_SECONDS=.*/: "${SEAL_WAIT_SECONDS:=0}"/' \
    "$SCRIPT_DIR/../../orchestration.config" >"$ENTITY/orchestration.config"
git -C "$ENTITY" add -A; git -C "$ENTITY" commit -q -m seed
export RICHOS_ENTITY_ROOT="$ENTITY"

new_session() { # <session-id> [cwd] -> exports RICHOS_SESSION_PID; records SessionStart
    local pid
    pid="$(sh -c 'sleep 600 >/dev/null 2>&1 & echo $!')"
    SESS_PIDS="$SESS_PIDS $pid"
    export RICHOS_SESSION_PID="$pid"
    printf '{"hook_event_name":"SessionStart","session_id":"%s","cwd":"%s"}' "$1" "${2:-$ENTITY}" \
        | python3 "$WS_PY" --entity "$ENTITY" hook >/dev/null
}
spawn_agent() { # <agent-id> <name> [session-id] -> registered, started, bound
    local aid="$1" name="$2" sid="${3:-$SID}" np="$ENTITY/.claude/worktrees/agent-$1"
    python3 -c 'import json,sys; print(json.dumps({"session_id":sys.argv[1],"tool_use_id":"tu-"+sys.argv[2],"tool_name":"Agent","tool_input":{"name":sys.argv[2],"subagent_type":"dev","isolation":"worktree","prompt":"x"}}))' "$sid" "$name" \
        | python3 "$WS_PY" --entity "$ENTITY" register-spawn >/dev/null
    git -C "$ENTITY" worktree add -q "$np" -b "worktree-agent-$aid"
    printf '{"hook_event_name":"SubagentStart","session_id":"%s","agent_id":"%s","agent_type":"dev","cwd":"%s"}' "$sid" "$aid" "$np" \
        | python3 "$WS_PY" --entity "$ENTITY" hook >/dev/null
    printf '{"hook_event_name":"PostToolUse","session_id":"%s","tool_use_id":"tu-%s","tool_name":"Agent","tool_input":{"name":"%s"},"tool_response":{"agentId":"%s"}}' "$sid" "$name" "$name" "$aid" \
        | python3 "$WS_PY" --entity "$ENTITY" hook >/dev/null
}
stop_agent() { # <agent-id> [session-id]
    printf '{"hook_event_name":"SubagentStop","session_id":"%s","agent_id":"%s"}' "${2:-$SID}" "$1" \
        | python3 "$WS_PY" --entity "$ENTITY" hook >/dev/null
}
payload() { # <tool_name> <agent_id|""> [agent_type] [session]
    python3 -c '
import json, sys
tool, aid, at, sid = sys.argv[1:5]
d = {"session_id": sid, "hook_event_name": "PreToolUse", "tool_name": tool,
     "tool_input": {"file_path": "/tmp/x", "command": "ls"}, "tool_use_id": "toolu_x"}
if aid:
    d["agent_id"] = aid
    d["agent_type"] = at or "dev"
print(json.dumps(d))' "$1" "$2" "${3:-dev}" "${4:-$SID}"
}
run() { OUT="$(printf '%s' "$1" | "$HOOK" 2>&1)"; RC=$?; }

echo "=== guard-sealed-worktree (the lock-out) tests ==="
new_session "$SID"
# Point 14: the branch this body of work integrates on is RECORDED before the
# first spawn. Nothing infers it any more, and without it nothing here can land,
# so the point-5 gate would refuse the second spawn and the lock-out cases would
# never be reached. One command, exactly as Rich runs it.
python3 "$WS_PY" --entity "$ENTITY" --session "$SID" integration --repo "$ENTITY" \
    --branch main --why "the lock-out suite's body of work" >/dev/null

# G01 the lead
run "$(payload Write "")"
[ "$RC" -eq 0 ] && [ -z "$OUT" ] && ok "G01  the lead's own call (no agent_id) passes silently" || bad "G01  lead rc=$RC: $OUT"

# G02 a registered worker
spawn_agent "a00000000000reg1" dev-opus-g2
run "$(payload Write a00000000000reg1)"
[ "$RC" -eq 0 ] && [ -z "$OUT" ] && ok "G02  a REGISTERED worker's Write passes silently" || bad "G02  registered rc=$RC: $OUT"

# G02b/G02c — ROUND 8, ITEM 3: "An agent never works inside a codex/ workspace."
# This is the only hook that sees a Write/Edit; until round 8 a registered
# agent's Edit with a file_path inside a codex/ workspace passed it
# (brief-audit-sage-round8 §4). Whether the path is inside one is read from
# git — the branch its worktree has checked out — never from the path's name.
CODEX_WT="$SANDBOX/codex-wt"
git -C "$ENTITY" worktree add -q "$CODEX_WT" -b codex/some-task
payload_path() { # <tool_name> <agent_id|""> <file_path>
    python3 -c '
import json, sys
tool, aid, fp, sid = sys.argv[1:5]
d = {"session_id": sid, "hook_event_name": "PreToolUse", "tool_name": tool,
     "tool_input": {"file_path": fp, "old_string": "a", "new_string": "b"}, "tool_use_id": "toolu_p"}
if aid:
    d["agent_id"] = aid
    d["agent_type"] = "dev"
print(json.dumps(d))' "$1" "$2" "$3" "$SID"
}
run "$(payload_path Edit a00000000000reg1 "$CODEX_WT/seed.txt")"
[ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "codex/" && ok "G02b a REGISTERED worker's Edit aimed INSIDE a codex/ workspace is refused, naming point 2" || bad "G02b codex Edit rc=$RC: $OUT"
run "$(payload_path Write a00000000000reg1 "$CODEX_WT/sub/new.txt")"
[ "$RC" -eq 2 ] && ok "G02c ...and its Write of a new file there too" || bad "G02c codex Write rc=$RC: $OUT"
run "$(payload_path Edit "" "$CODEX_WT/seed.txt")"
[ "$RC" -eq 0 ] && ok "G02d the lead's Edit inside the codex/ workspace passes (the lead's calls are unaffected)" || bad "G02d lead codex Edit rc=$RC: $OUT"
run "$(payload_path Edit a00000000000reg1 "$ENTITY/.claude/worktrees/agent-a00000000000reg1/seed.txt")"
[ "$RC" -eq 0 ] && ok "G02e CONTROL: the same worker's Edit inside its OWN workspace passes" || bad "G02e own workspace Edit rc=$RC: $OUT"

# G03-G05 a finished worker: every tool refused, Read included
stop_agent "a00000000000reg1"
for t in Write Bash Read; do
    run "$(payload "$t" a00000000000reg1)"
    case "$t" in Write) id=G03 ;; Bash) id=G04 ;; Read) id=G05 ;; esac
    if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "finished agent"; then
        ok "$id  a FINISHED worker's $t is refused (point 9)"
    else
        bad "$id  finished $t rc=$RC: $OUT"
    fi
done

# G05b/G05c walk 6 (2026-10-05): a finished worker's report is not a write. Its
# SubagentHandback passes when its only ending is its own run's end; a STOPPED
# worker's does not, and neither does anything else the finished worker tries.
run "$(payload SubagentHandback a00000000000reg1)"
[ "$RC" -eq 0 ] && ok "G05b a worker whose run ended may still hand its report back (SubagentHandback)" || bad "G05b finished handback rc=$RC: $OUT"
run "$(payload Bash a00000000000reg1)"
[ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "finished agent" \
    && ok "G05c CONTROL: the same finished worker's Bash is still refused after its handback passed" || bad "G05c finished Bash rc=$RC: $OUT"
spawn_agent "a00000000000stp1" dev-opus-g5d
printf '{"hook_event_name":"PostToolUse","session_id":"%s","tool_name":"TaskStop","tool_input":{"task_id":"a00000000000stp1"},"tool_response":{"task_id":"a00000000000stp1"}}' "$SID" \
    | python3 "$WS_PY" --entity "$ENTITY" hook >/dev/null
run "$(payload SubagentHandback a00000000000stp1)"
[ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "finished agent" \
    && ok "G05d CONTROL: a STOPPED worker's SubagentHandback is refused (a stop still stops it)" || bad "G05d stopped handback rc=$RC: $OUT"

# G06 a finished worker of a read-only TYPE is still refused
spawn_agent "a00000000000exp1" explore-opus-g6
stop_agent "a00000000000exp1"
run "$(payload Read a00000000000exp1 Explore)"
[ "$RC" -eq 2 ] && ok "G06  a finished Explore-typed agent is refused (a type exemption is not permission to return)" || bad "G06  finished Explore rc=$RC: $OUT"

# G07 a paused worker is not locked out
spawn_agent "a00000000000pau1" dev-opus-g7
python3 "$WS_PY" --session "$SID" pause dev-opus-g7 --until "the quota reset" >/dev/null
stop_agent "a00000000000pau1"
run "$(payload Write a00000000000pau1)"
[ "$RC" -eq 0 ] && ok "G07  a PAUSED worker (pause recorded before its run ended) is not locked out (point 11)" || bad "G07  paused rc=$RC: $OUT"

# G08/G09 an unregistered worker: read-only only
run "$(payload Write a00000000000unr1)"
[ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "no registration" && ok "G08  an UNREGISTERED worker's Write is refused (point 3)" || bad "G08  unregistered Write rc=$RC: $OUT"
run "$(payload Read a00000000000unr1)"
[ "$RC" -eq 0 ] && ok "G09  an unregistered worker may Read (so it can report why it stopped)" || bad "G09  unregistered Read rc=$RC: $OUT"

# G10 a read-only agent type with no registration is exempt
run "$(payload Bash a00000000000exp2 Explore)"
[ "$RC" -eq 0 ] && ok "G10  an unregistered read-only TYPE (Explore) is exempt" || bad "G10  Explore rc=$RC: $OUT"
run "$(payload Bash a00000000000exp3 'richos-engine:Explore')"
[ "$RC" -eq 0 ] && ok "G11  a plugin-namespaced read-only type is the same type" || bad "G11  namespaced rc=$RC: $OUT"

# G12 the session's recorded end finishes its agents (point 12)
spawn_agent "a00000000000end1" dev-opus-g12
printf '{"hook_event_name":"SessionEnd","session_id":"%s","reason":"exit"}' "$SID" | python3 "$WS_PY" --entity "$ENTITY" hook >/dev/null
run "$(payload Write a00000000000end1)"
[ "$RC" -eq 2 ] && ok "G12  an agent of a session that RECORDED ITS END is refused (point 12)" || bad "G12  ended-session rc=$RC: $OUT"

# G13 a session whose process no longer exists finishes its agents (point 12)
SID2="deadbeef-0000-4000-8000-000000000002"
new_session "$SID2"
S2_PID="$RICHOS_SESSION_PID"
spawn_agent "a00000000000gon1" dev-opus-g13 "$SID2"
run "$(payload Write a00000000000gon1 dev "$SID2")"
G13_BEFORE="$RC"
kill "$S2_PID"; while kill -0 "$S2_PID" 2>/dev/null; do sleep 0.1; done
run "$(payload Write a00000000000gon1 dev "$SID2")"
[ "$G13_BEFORE" -eq 0 ] && [ "$RC" -eq 2 ] && ok "G13  an agent whose session's PROCESS no longer exists is refused — read from the operating system (point 12)" || bad "G13  gone-session before=$G13_BEFORE after=$RC: $OUT"

# G14 still refused after its workspace is gone (landed)
SID3="deadbeef-0000-4000-8000-000000000003"
new_session "$SID3"
spawn_agent "a00000000000lnd1" dev-opus-g14 "$SID3"
stop_agent "a00000000000lnd1" "$SID3"
printf '{"hook_event_name":"Stop","session_id":"%s","cwd":"%s"}' "$SID3" "$ENTITY" | python3 "$WS_PY" --entity "$ENTITY" gate-stop >/dev/null 2>&1
run "$(payload Write a00000000000lnd1 dev "$SID3")"
[ ! -e "$ENTITY/.claude/worktrees/agent-a00000000000lnd1" ] && [ "$RC" -eq 2 ] \
    && ok "G14  a finished agent whose workspace is already deleted is still refused (point 9)" || bad "G14  after-land rc=$RC: $OUT"

# G15 the lead of a claude --worktree session may only read (point 3)
WT="$ENTITY/.claude/worktrees/my-session"
git -C "$ENTITY" worktree add -q "$WT" -b worktree-my-session
SID4="deadbeef-0000-4000-8000-000000000004"
new_session "$SID4" "$WT"
run "$(payload Bash "" dev "$SID4")"
G15_BASH="$RC"
run "$(payload Read "" dev "$SID4")"
[ "$G15_BASH" -eq 2 ] && [ "$RC" -eq 0 ] && ok "G15  the lead of a claude --worktree session is refused Bash, allowed Read (point 3)" || bad "G15  worktree session bash=$G15_BASH read=$RC"

# G15b-G15e — 2026-09-22: a refused lead could not stop ten agents for 33 minutes.
# Under a (correct) refusal the lead may still STOP ITS AGENTS, and the refusal
# names that one thing so the next call is a stop, not a retry.
run "$(payload Bash "" dev "$SID4")"
printf '%s' "$OUT" | grep -q "TaskStop" && printf '%s' "$OUT" | grep -q "scripts/stop.sh" \
    && printf '%s' "$OUT" | grep -q "DO NOT RETRY" \
    && ok "G15b the refusal names the one thing left: TaskStop / stop.sh, and says not to retry" || bad "G15b refusal text: $OUT"
run "$(payload TaskStop "" dev "$SID4")"
[ "$RC" -eq 0 ] && ok "G15c ...and that session's TaskStop passes (a refused lead can always stop its agents)" || bad "G15c TaskStop rc=$RC: $OUT"
STOP_SH="$(cd "$SCRIPT_DIR/.." && pwd)/stop.sh"
payload_cmd() { # <command> <session>
    python3 -c 'import json,sys; print(json.dumps({"session_id": sys.argv[2], "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": sys.argv[1]}, "tool_use_id": "toolu_c"}))' "$1" "$2"
}
# The path is quoted as a lead would type it: the engine under test may live
# under a folder with a space ("Application Support"), and an unquoted path
# would be two words, not this engine's stop.sh.
run "$(payload_cmd "'$STOP_SH' dev-opus-x --ceo-word 'stop it'" "$SID4")"
G15D_STOP="$RC"
run "$(payload_cmd "'$STOP_SH' dev-opus-x --ceo-word 'stop it'; touch '$SANDBOX/rode-along'" "$SID4")"
[ "$G15D_STOP" -eq 0 ] && [ "$RC" -eq 2 ] && ok "G15d the engine's stop.sh passes as one plain command; nothing may ride along with it" || bad "G15d stop.sh=$G15D_STOP chained=$RC"
run "$(payload SendMessage "" dev "$SID4")"
[ "$RC" -eq 2 ] && ok "G15e CONTROL: that session's SendMessage is still refused (point 3 stands)" || bad "G15e SendMessage rc=$RC"

# G16-G19 fail closed on its own error
NOLIB="$SANDBOX/nolib"
mkdir -p "$NOLIB/scripts/hooks" "$NOLIB/scripts/lib"
cp "$HOOK" "$NOLIB/scripts/hooks/"; chmod +x "$NOLIB/scripts/hooks/guard-sealed-worktree.sh"
cp "$SCRIPT_DIR/../lib/resolve-roots.sh" "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" "$NOLIB/scripts/lib/"
OUT="$(printf '%s' "$(payload Write a00000000000reg1)" | bash "$NOLIB/scripts/hooks/guard-sealed-worktree.sh" 2>&1)"; RC=$?
[ "$RC" -eq 2 ] && ok "G16  the registry library MISSING: a worker's Write is refused" || bad "G16  nolib Write rc=$RC: $OUT"
OUT="$(printf '%s' "$(payload Read a00000000000reg1)" | bash "$NOLIB/scripts/hooks/guard-sealed-worktree.sh" 2>&1)"; RC=$?
[ "$RC" -eq 0 ] && ok "G17  registry missing: a worker's Read passes under the read-only policy" || bad "G17  nolib Read rc=$RC: $OUT"
OUT="$(printf '%s' "$(payload Write "")" | bash "$NOLIB/scripts/hooks/guard-sealed-worktree.sh" 2>&1)"; RC=$?
[ "$RC" -eq 0 ] && ok "G18  registry missing: the lead's Write passes" || bad "G18  nolib lead rc=$RC: $OUT"
run "not json"
[ "$RC" -eq 2 ] && ok "G19  an UNPARSEABLE payload is refused (it cannot be proven the lead's)" || bad "G19  unparseable rc=$RC: $OUT"

echo ""
echo "=== guard-sealed-worktree tests: $FAIL FAILED, $PASS passed ==="
[ "$FAIL" -eq 0 ] || exit 1
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$SCRIPT_DIR/guard-sealed-worktree.mutation.sh" ]; then
    bash "$SCRIPT_DIR/guard-sealed-worktree.mutation.sh" || exit 1
fi
exit 0
