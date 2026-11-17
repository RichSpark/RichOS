#!/usr/bin/env bash
#
# worker-lifecycle.test.sh — one suite for the four worker-lifecycle emitters
# (worker-created / worker-started / worker-updated / worker-ended), because
# they are one stream and their most important properties are properties OF the
# stream, not of any single hook:
#
#   * a BLOCKED spawn must produce NO event at all — no phantom active worker;
#   * a SYNCHRONOUS Agent run must produce no "created", because its
#     PostToolUse fires when the work is already over;
#   * an unattributable event must produce no line rather than an anonymous one;
#   * created / started / run_ended must land in one file, in order, joinable
#     on agent_id — which is the only way a consumer can compute an active
#     count without guessing;
#   * `run_ended` must never be dressed up as completed / failed / interrupted;
#   * a message BODY must never reach the log;
#   * and none of it may ever fail a tool call.
#
# Every negative case here has a positive control beside it. A test that
# asserts "nothing was written" passes just as well when the hook is broken and
# writes nothing ever, so each one is paired with the almost-identical input
# that MUST write, and the pair is what carries the meaning.
#
#   scripts/hooks/worker-lifecycle.test.sh
#
# Exit 0 = all pass, 1 = a failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CREATED_HOOK="$SCRIPT_DIR/worker-created-handoff.sh"
STARTED_HOOK="$SCRIPT_DIR/worker-started-handoff.sh"
UPDATED_HOOK="$SCRIPT_DIR/worker-updated-handoff.sh"
ENDED_HOOK="$SCRIPT_DIR/worker-ended-handoff.sh"
ISOLATION_GUARD="$SCRIPT_DIR/guard-worktree-isolation.sh"

SANDBOX="$(mktemp -d -t worker-lifecycle-test.XXXXXX)"
# The cleanup trap is fenced to the TOP-LEVEL shell on purpose. Bash runs an
# inherited EXIT trap when a PIPELINE SUBSHELL exits too, so a plain
# `trap 'rm -rf "$SANDBOX"' EXIT` deletes the sandbox mid-suite the first time
# anything inside a pipeline exits early (a `set -u` slip is enough). Every
# case after that then "fails" against a directory that is simply gone, which
# is a spectacularly misleading way to be told about a one-line typo.
TOP_SHELL="${BASHPID:-$$}"
# The isolation guard's clause 7 registers every allowed file-capable spawn
# (docs/plans/worktree-spec-2026-09-11.md, point 3). The registry is pinned into
# the sandbox so this suite never writes the operator's record, and the
# registration's session identity is a process of this suite's own.
export RICHOS_WORKSPACES_DIR="$SANDBOX/ws"
RICHOS_SESSION_PID="$(sh -c 'sleep 600 >/dev/null 2>&1 & echo $!')"
export RICHOS_SESSION_PID
trap '[ "${BASHPID:-$$}" = "$TOP_SHELL" ] && { kill "$RICHOS_SESSION_PID" 2>/dev/null; rm -rf "$SANDBOX"; }' EXIT
TEAMS_DIR="$SANDBOX/teams"
SESSION_ID="abcd1234-0000-0000-0000-00000000beef"
SESSION_DIR="$TEAMS_DIR/session-abcd1234"
mkdir -p "$SESSION_DIR"
LOG="$SESSION_DIR/worker-events.jsonl"

FAIL=0
PASS=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
fail() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; }

reset_log() { : >"$LOG"; }
lines() { grep -c . "$LOG" 2>/dev/null | tr -d ' '; }

# run <hook> <payload> -> echoes the hook's exit code
WL="$SANDBOX/wt-ledger.jsonl"
export RICHOS_WORKTREE_LEDGER="$WL"
run() {
    printf '%s' "$2" | WORKER_EVENTS_TEAMS_DIR="$TEAMS_DIR" RICHOS_WORKTREE_LEDGER="$WL" "$1" >/dev/null 2>&1
    echo $?
}

# The async-launch acknowledgement, WORDED EXACTLY as the shipping harness
# words it (Claude Code 2.1.251), captured from a real Agent tool_result. Only
# the agent id is substituted — the harness's own result text asks that real
# ids not be reproduced, and the id's VALUE is irrelevant to every assertion
# here, only its presence and extraction.
LAUNCH_ACK='Async agent launched successfully. (This tool result is internal metadata — never quote or paste any part of it, including the agentId below, into a user-facing reply.)\nagentId: aTESTWORKER00001 (internal ID - do not mention to user. Use SendMessage with to: '"'"'aTESTWORKER00001'"'"', summary: '"'"'<5-10 word recap>'"'"' to continue this agent.)\nThe agent is working in the background. You will be notified automatically when it completes.'

# A real backgrounded spawn's PostToolUse payload.
SPAWN_OK='{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","cwd":"/repo","tool_name":"Agent","tool_use_id":"toolu_1","tool_input":{"name":"dev-sonnet-high-1", "effort": "high","subagent_type":"dev","isolation":"worktree","model":"sonnet","prompt":"do the thing"},"tool_response":[{"type":"text","text":"'"$LAUNCH_ACK"'"}]}'

echo "=== worker-lifecycle emitters: one stream, four hooks ==="
echo ""

# ---------------------------------------------------------------------------
# 1. CREATED — the signal that did not exist before this slice.
# ---------------------------------------------------------------------------
reset_log
rc="$(run "$CREATED_HOOK" "$SPAWN_OK")"
[ "$rc" = "0" ] && pass "1a  created: backgrounded spawn exits 0" \
                || fail "1a  created: exit $rc != 0"
[ "$(lines)" = "1" ] && pass "1b  created: writes exactly one line" \
                     || fail "1b  created: wrote $(lines) lines, want 1"

if python3 - "$LOG" <<'PY'
import json, sys
r = [json.loads(l) for l in open(sys.argv[1]) if l.strip()][-1]
assert r["event"] == "WorkerCreated", r
assert r["lifecycle_state"] == "created", r
assert r["source_hook"] == "PostToolUse[Agent]", r
assert r["agent_id"] == "aTESTWORKER00001", r
assert r["worker_name"] == "dev-sonnet-high-1", r
assert r["agent_type"] == "dev", r
assert r["isolation"] == "worktree", r
assert r["session_id"].startswith("abcd1234"), r
assert r["timestamp"], r
# The spawn PROMPT must never be logged: it is arbitrary operator text.
assert "prompt" not in r, r
assert "do the thing" not in json.dumps(r), r
PY
then pass "1c  created: fields correct, and the spawn prompt is not logged"
else fail "1c  created: field assertions failed"
fi

# NEGATIVE + POSITIVE CONTROL PAIR. A synchronous Agent call's PostToolUse
# fires when the subagent has ALREADY FINISHED. Treating that as a creation
# would announce a live worker at the moment it stopped existing.
reset_log
SYNC_RESULT='{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","tool_name":"Agent","tool_input":{"name":"dev-sonnet-high-2", "effort": "high","subagent_type":"Explore"},"tool_response":[{"type":"text","text":"Here is my final report: I read 4 files and found nothing."}]}'
rc="$(run "$CREATED_HOOK" "$SYNC_RESULT")"
[ "$rc" = "0" ] && pass "1d  created: synchronous Agent result still exits 0" \
                || fail "1d  created: exit $rc != 0"
[ "$(lines)" = "0" ] && pass "1e  created: synchronous Agent result writes NOTHING (no already-finished 'live' worker)" \
                     || fail "1e  created: wrote $(lines) lines for a synchronous result, want 0"

# Ack present but no id: we know something launched, we do not know WHICH. An
# unjoinable worker is worse than a missing one.
reset_log
NO_ID='{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","tool_name":"Agent","tool_input":{"name":"dev-sonnet-high-3", "effort": "high"},"tool_response":"Async agent launched successfully. (no id in this one)"}'
rc="$(run "$CREATED_HOOK" "$NO_ID")"
[ "$rc" = "0" ] && pass "1f  created: launch ack without an agentId exits 0" \
                || fail "1f  created: exit $rc != 0"
[ "$(lines)" = "0" ] && pass "1g  created: launch ack without an agentId writes NOTHING (unjoinable)" \
                     || fail "1g  created: wrote $(lines) lines, want 0"

# Not the Agent tool at all.
reset_log
rc="$(run "$CREATED_HOOK" '{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","tool_name":"Bash","tool_input":{"command":"ls"},"tool_response":"Async agent launched successfully. agentId: aFAKE"}')"
[ "$rc" = "0" ] && [ "$(lines)" = "0" ] \
    && pass "1h  created: a non-Agent tool is ignored even if its output mentions a launch" \
    || fail "1h  created: non-Agent tool wrote $(lines) lines / exit $rc"

# ---------------------------------------------------------------------------
# 2. THE NEGATIVE CONTROL THAT MATTERS MOST — A BLOCKED SPAWN IS SILENT.
#
# Modelled the way the harness actually behaves: PreToolUse runs first, and if
# it blocks (exit 2) the tool never executes, so PostToolUse never fires. The
# pair below is the whole proof — same spawn, one isolated and one not.
# ---------------------------------------------------------------------------
GUARD_REPO="$SANDBOX/guardrepo"
mkdir -p "$GUARD_REPO/.claude/agents"
: >"$GUARD_REPO/orchestration.config"
printf -- '---\nname: dev\nmodel: sonnet\n---\nA dev.\n' >"$GUARD_REPO/.claude/agents/dev.md"

# simulate_spawn <isolation> <name> -> runs PreToolUse then, ONLY if it allowed
# the call, PostToolUse. Echoes "<preRc>:<postFired>".
simulate_spawn() {
    local iso="$1" name="$2"
    local payload
    payload='{"session_id":"'"$SESSION_ID"'","cwd":"'"$GUARD_REPO"'","hook_event_name":"PreToolUse","tool_name":"Agent","tool_use_id":"toolu_x","tool_input":{"name":"'"$name"'","effort":"high","subagent_type":"dev","isolation":"'"$iso"'","prompt":"work"}}'
    local pre_rc
    printf '%s' "$payload" \
        | CLAUDE_PROJECT_DIR="$GUARD_REPO" GUARD_ISOLATION_TEAMS_DIR="$TEAMS_DIR" \
          "$ISOLATION_GUARD" >/dev/null 2>&1
    pre_rc=$?
    if [ "$pre_rc" -ne 0 ]; then
        echo "$pre_rc:no"
        return
    fi
    local post
    post='{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","cwd":"'"$GUARD_REPO"'","tool_name":"Agent","tool_input":{"name":"'"$name"'","subagent_type":"dev","isolation":"'"$iso"'"},"tool_response":[{"type":"text","text":"'"$LAUNCH_ACK"'"}]}'
    run "$CREATED_HOOK" "$post" >/dev/null
    echo "$pre_rc:yes"
}

reset_log
res="$(simulate_spawn "" "dev-sonnet-high-blocked")"
if [ "$res" = "2:no" ]; then
    pass "2a  blocked spawn: the isolation guard refuses it (exit 2), so PostToolUse never runs"
else
    fail "2a  blocked spawn: expected '2:no', got '$res'"
fi
if [ "$(lines)" = "0" ]; then
    pass "2b  blocked spawn: ZERO lifecycle events — no phantom active worker"
else
    fail "2b  blocked spawn: wrote $(lines) event(s) for a spawn that never happened"
fi

# POSITIVE CONTROL — same guard, same log, an ALLOWED spawn. Without this,
# 2b would pass equally well if the hook were simply broken.
res="$(simulate_spawn "worktree" "dev-sonnet-high-allowed")"
if [ "$res" = "0:yes" ]; then
    pass "2c  POSITIVE CONTROL: the identical spawn WITH isolation is allowed and does run PostToolUse"
else
    fail "2c  POSITIVE CONTROL: expected '0:yes', got '$res'"
fi
if [ "$(lines)" = "1" ]; then
    pass "2d  POSITIVE CONTROL: exactly one created event — so 2b's silence is the guard's doing, not a dead hook"
else
    fail "2d  POSITIVE CONTROL: wrote $(lines) lines, want 1"
fi

# ---------------------------------------------------------------------------
# 3. NAME-REUSE BLOCKING IS UNCHANGED.
#
# The lifecycle emitters deliberately do not touch guard-worktree-isolation.sh
# or spawned-names.log. This case re-proves the clause end to end against the
# same sandbox the cases above use, so a future edit that "helpfully" moved
# ledger writing into a lifecycle hook would be caught here.
# ---------------------------------------------------------------------------
printf 'dev-sonnet-high-taken\n' >>"$SESSION_DIR/spawned-names.log"
res="$(simulate_spawn "worktree" "dev-sonnet-high-taken")"
if [ "$res" = "2:no" ]; then
    pass "3a  name reuse is still BLOCKED (a name already in spawned-names.log)"
else
    fail "3a  name reuse: expected '2:no', got '$res'"
fi
res="$(simulate_spawn "worktree" "dev-sonnet-high-fresh")"
if [ "$res" = "0:yes" ]; then
    pass "3b  POSITIVE CONTROL: a fresh name is still allowed"
else
    fail "3b  fresh name: expected '0:yes', got '$res'"
fi
if [ "$(grep -c . "$SESSION_DIR/spawned-names.log" | tr -d ' ')" = "1" ]; then
    pass "3c  no lifecycle hook writes to spawned-names.log (ledger untouched: still 1 line)"
else
    fail "3c  spawned-names.log was modified by a lifecycle hook"
fi

# ---------------------------------------------------------------------------
# 4. STARTED — SubagentStart.
# ---------------------------------------------------------------------------
reset_log
STARTED='{"hook_event_name":"SubagentStart","session_id":"'"$SESSION_ID"'","cwd":"/repo/.claude/worktrees/agent-aTESTWORKER00001","transcript_path":"/t.jsonl","agent_id":"aTESTWORKER00001","agent_type":"dev"}'
rc="$(run "$STARTED_HOOK" "$STARTED")"
[ "$rc" = "0" ] && [ "$(lines)" = "1" ] \
    && pass "4a  started: SubagentStart writes one line, exit 0" \
    || fail "4a  started: exit $rc, $(lines) lines"

if python3 - "$LOG" <<'PY'
import json, sys
r = [json.loads(l) for l in open(sys.argv[1]) if l.strip()][-1]
assert r["event"] == "WorkerStarted", r
assert r["lifecycle_state"] == "started", r
assert r["source_hook"] == "SubagentStart", r
assert r["agent_id"] == "aTESTWORKER00001", r
assert r["agent_type"] == "dev", r
# The display name is NOT in this payload and must not be invented here.
assert "worker_name" not in r, r
PY
then pass "4b  started: fields correct, and no display name is invented"
else fail "4b  started: field assertions failed"
fi

reset_log
rc="$(run "$STARTED_HOOK" '{"hook_event_name":"SubagentStart","session_id":"'"$SESSION_ID"'","agent_type":"dev"}')"
[ "$rc" = "0" ] && [ "$(lines)" = "0" ] \
    && pass "4c  started: no agent_id -> no line (an anonymous start would inflate any active count)" \
    || fail "4c  started: exit $rc, wrote $(lines) lines"

reset_log
rc="$(run "$STARTED_HOOK" '{"hook_event_name":"SessionStart","session_id":"'"$SESSION_ID"'","agent_id":"aTESTWORKER00001"}')"
[ "$rc" = "0" ] && [ "$(lines)" = "0" ] \
    && pass "4d  started: a different hook event is ignored" \
    || fail "4d  started: exit $rc, wrote $(lines) lines"

# ---------------------------------------------------------------------------
# 5. RUN ENDED — SubagentStop. The state it must NOT claim is the point.
# ---------------------------------------------------------------------------
reset_log
ENDED='{"hook_event_name":"SubagentStop","session_id":"'"$SESSION_ID"'","agent_id":"aTESTWORKER00001","agent_type":"dev","stop_hook_active":false,"agent_transcript_path":"/t.jsonl","last_assistant_message":"I have finished and everything worked perfectly."}'
rc="$(run "$ENDED_HOOK" "$ENDED")"
[ "$rc" = "0" ] && [ "$(lines)" = "1" ] \
    && pass "5a  ended: SubagentStop writes one line, exit 0" \
    || fail "5a  ended: exit $rc, $(lines) lines"

if python3 - "$LOG" <<'PY'
import json, sys
r = [json.loads(l) for l in open(sys.argv[1]) if l.strip()][-1]
assert r["event"] == "WorkerRunEnded", r
# The payload carries no success/failure/interrupt information, so none of
# these three may EVER appear here — the whole reason this state is called
# run_ended and not something more satisfying.
assert r["lifecycle_state"] == "run_ended", r
assert r["lifecycle_state"] not in ("completed", "failed", "interrupted"), r
assert r["agent_id"] == "aTESTWORKER00001", r
assert r["agent_transcript_path"] == "/t.jsonl", r
# A cheerful last message must not be laundered into a success claim, and the
# model's words must not enter the log at all.
blob = json.dumps(r)
assert "everything worked perfectly" not in blob, r
assert "last_assistant_message" not in r, r
PY
then pass "5b  ended: recorded as run_ended only — never completed/failed/interrupted, and the last message is not logged"
else fail "5b  ended: field assertions failed"
fi

reset_log
# ownership ledger: the stop above is retained as an ADVISORY finish signal
# keyed to the agent id — and the id-less stop below writes nothing there.
if grep -q '"signal": "SubagentStop"' "$WL" 2>/dev/null && grep -q '"agent_id": "aTESTWORKER00001"' "$WL"; then
    pass "5a2 ended: retained in the ownership ledger as an advisory SubagentStop signal for aTESTWORKER00001"
else
    fail "5a2 ended: ownership ledger has no SubagentStop signal for aTESTWORKER00001"
fi
WL_BEFORE="$(grep -c . "$WL" 2>/dev/null || echo 0)"
rc="$(run "$ENDED_HOOK" '{"hook_event_name":"SubagentStop","session_id":"'"$SESSION_ID"'","agent_type":"dev"}')"
[ "$(grep -c . "$WL" 2>/dev/null || echo 0)" -eq "$WL_BEFORE" ] \
    && pass "5c2 ended: an id-less stop writes no ledger signal either" \
    || fail "5c2 ended: id-less stop wrote a ledger signal"
[ "$rc" = "0" ] && [ "$(lines)" = "0" ] \
    && pass "5c  ended: no agent_id -> no line (an unpaired end could close out the wrong worker)" \
    || fail "5c  ended: exit $rc, wrote $(lines) lines"

# ---------------------------------------------------------------------------
# 6. UPDATED — PostToolUse[SendMessage], attribution-gated.
# ---------------------------------------------------------------------------
reset_log
SECRET_BODY="here is the database password hunter2 and the whole plan"
FROM_WORKER='{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","tool_name":"SendMessage","agent_id":"aTESTWORKER00001","agent_type":"dev","tool_input":{"to":"team-lead","summary":"landed the parser fix","message":"'"$SECRET_BODY"'"}}'
rc="$(run "$UPDATED_HOOK" "$FROM_WORKER")"
[ "$rc" = "0" ] && [ "$(lines)" = "1" ] \
    && pass "6a  updated: a worker's SendMessage writes one line, exit 0" \
    || fail "6a  updated: exit $rc, $(lines) lines"

if SECRET="$SECRET_BODY" python3 - "$LOG" <<'PY'
import json, os, sys
r = [json.loads(l) for l in open(sys.argv[1]) if l.strip()][-1]
assert r["event"] == "WorkerUpdated", r
assert r["lifecycle_state"] == "updated", r
assert r["source_hook"] == "PostToolUse[SendMessage]", r
assert r["agent_id"] == "aTESTWORKER00001", r
assert r["to"] == "team-lead", r
assert r["summary"] == "landed the parser fix", r
assert r["message_kind"] == "text", r
assert r["message_chars"] == len(os.environ["SECRET"]), r
# THE BODY NEVER ENTERS THE LOG.
assert os.environ["SECRET"] not in json.dumps(r), r
assert "hunter2" not in json.dumps(r), r
PY
then pass "6b  updated: summary + length recorded, message BODY never written to the log"
else fail "6b  updated: field assertions failed"
fi

# The lead's own SendMessage carries no agent_id. "Messaged {name}" is an
# orchestrator action, not a worker state.
reset_log
FROM_LEAD='{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","tool_name":"SendMessage","tool_input":{"to":"dev-sonnet-high-1","summary":"go ahead","message":"proceed please"}}'
rc="$(run "$UPDATED_HOOK" "$FROM_LEAD")"
[ "$rc" = "0" ] && [ "$(lines)" = "0" ] \
    && pass "6c  updated: the LEAD's SendMessage writes NOTHING (no agent_id -> not a worker update)" \
    || fail "6c  updated: lead message wrote $(lines) lines, exit $rc"

reset_log
rc="$(run "$UPDATED_HOOK" "$FROM_WORKER")"
[ "$(lines)" = "1" ] \
    && pass "6d  POSITIVE CONTROL: the worker's identical-shaped message still writes — 6c is attribution, not a dead hook" \
    || fail "6d  POSITIVE CONTROL: wrote $(lines) lines, want 1"

# A structured protocol body has no character count to report.
reset_log
run "$UPDATED_HOOK" '{"hook_event_name":"PostToolUse","session_id":"'"$SESSION_ID"'","tool_name":"SendMessage","agent_id":"aTESTWORKER00001","tool_input":{"to":"team-lead","message":{"type":"shutdown_request"}}}' >/dev/null
if python3 - "$LOG" <<'PY'
import json, sys
r = [json.loads(l) for l in open(sys.argv[1]) if l.strip()][-1]
assert r["message_kind"] == "structured", r
assert "message_chars" not in r, r
assert "shutdown_request" not in json.dumps(r), r
PY
then pass "6e  updated: a structured protocol body is marked structured, never measured or copied"
else fail "6e  updated: structured-body assertions failed"
fi

# ---------------------------------------------------------------------------
# 7. THE STREAM — one file, ordered, joinable. This is what makes an active
#    count derivable instead of guessed.
# ---------------------------------------------------------------------------
reset_log
run "$CREATED_HOOK" "$SPAWN_OK" >/dev/null
run "$STARTED_HOOK" "$STARTED" >/dev/null
run "$UPDATED_HOOK" "$FROM_WORKER" >/dev/null
run "$ENDED_HOOK" "$ENDED" >/dev/null
if python3 - "$LOG" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
assert [r["lifecycle_state"] for r in rows] == \
    ["created", "started", "updated", "run_ended"], rows
ids = {r["agent_id"] for r in rows}
assert ids == {"aTESTWORKER00001"}, ids
# The display name appears exactly once, on creation — every later event joins
# to it by agent_id rather than repeating (or re-deriving) it.
named = [r for r in rows if r.get("worker_name")]
assert len(named) == 1 and named[0]["lifecycle_state"] == "created", named
# A consumer's honest active computation over this stream: one worker started,
# one terminal event -> zero active. Not a heuristic, an arithmetic.
started = sum(1 for r in rows if r["lifecycle_state"] in ("created", "started"))
ended = sum(1 for r in rows if r["lifecycle_state"] == "run_ended")
assert started == 2 and ended == 1, (started, ended)
PY
then pass "7a  stream: created -> started -> updated -> run_ended, in order, joined on agent_id"
else fail "7a  stream: sequence/join assertions failed"
fi

# ---------------------------------------------------------------------------
# 8. NEVER FAIL A TOOL CALL. Every hook, every hostile input, always exit 0.
# ---------------------------------------------------------------------------
for hook in "$CREATED_HOOK" "$STARTED_HOOK" "$UPDATED_HOOK" "$ENDED_HOOK"; do
    b="$(basename "$hook")"
    bad=0
    for payload in '' 'not json at all }{' '[]' 'null' '{"tool_input":"not-an-object","tool_name":"Agent","hook_event_name":"PostToolUse"}'; do
        rc="$(run "$hook" "$payload")"
        [ "$rc" = "0" ] || bad=1
    done
    [ "$bad" -eq 0 ] && pass "8   $b: garbage/empty/typed-wrong input always exits 0" \
                     || fail "8   $b: a malformed payload produced a non-zero exit"
done

# An unwritable log directory must not fail the tool call either.
mkdir -p "$SANDBOX/home/.claude"
rc="$(printf '%s' "$SPAWN_OK" | WORKER_EVENTS_TEAMS_DIR="$SANDBOX/nope" HOME="$SANDBOX/home" "$CREATED_HOOK" >/dev/null 2>&1; echo $?)"
[ "$rc" = "0" ] && pass "8e  unresolvable team dir falls back to \$HOME and still exits 0" \
                || fail "8e  unresolvable team dir exit $rc != 0"
if [ -s "$SANDBOX/home/.claude/worker-events.jsonl" ]; then
    pass "8f  the \$HOME fallback actually receives the event (the fallback is not a silent drop)"
else
    fail "8f  the \$HOME fallback wrote nothing"
fi

# ---------------------------------------------------------------------------
# 9. THE WRITER AND THE READER AGREE ON WHERE A SESSION WITH NO TEAM DIRECTORY
#    WRITES (round 15, 2026-09-11 — Frank D4). resolve_team_dir() used to file
#    a KNOWN session's row into the single `session-*` directory that happened
#    to exist — another session's log, which only that session's reader opens
#    and this session's reader never does. Reproduced by Frank in a sandbox:
#    one foreign directory, the row landed there, the reader returned []. Now
#    a known session whose own directory is absent writes to the fallback file.
#    Both arms: 9a the row reaches the fallback; 9b the single-directory guess
#    still serves the case it was written for, a payload with NO session id.
#    (The reader this used to be checked against was the transaction store,
#    removed with the workspace spec of 2026-09-11; 9a' went with it.)
# ---------------------------------------------------------------------------
# The topology is the real one: the teams directory under the home's .claude,
# so the writer's fallback ($HOME/.claude/worker-events.jsonl) and the reader's
# (the sibling of the teams directory) are the SAME file, as they are in
# production. A fixture that put them in different places would test the
# fixture.
HOME2="$SANDBOX/home2"; TEAMS2="$HOME2/.claude/teams"
mkdir -p "$TEAMS2/session-ffffffff"                             # exactly ONE directory, and it is FOREIGN
rc="$(printf '%s' "$STARTED" | WORKER_EVENTS_TEAMS_DIR="$TEAMS2" HOME="$HOME2" RICHOS_WORKTREE_LEDGER="$WL" "$STARTED_HOOK" >/dev/null 2>&1; echo $?)"
if [ "$rc" = "0" ] && [ ! -e "$TEAMS2/session-ffffffff/worker-events.jsonl" ] \
   && grep -q '"agent_id": "aTESTWORKER00001"' "$HOME2/.claude/worker-events.jsonl" 2>/dev/null; then
    pass "9a  writer: a KNOWN session whose own directory is absent writes to the FALLBACK file, never into the one foreign directory that happens to exist"
else
    fail "9a  writer: foreign-dir=$([ -e "$TEAMS2/session-ffffffff/worker-events.jsonl" ] && echo written || echo untouched) fallback=$([ -e "$HOME2/.claude/worker-events.jsonl" ] && echo written || echo missing) rc=$rc"
fi
# 9b CONTROL — no session id at all: the single-directory guess is the only
# key there is, and it still applies (the pre-existing behavior, kept for the
# case it was written for).
rm -f "$HOME2/.claude/worker-events.jsonl"
NOSID='{"hook_event_name":"SubagentStart","cwd":"/repo","agent_id":"aTESTWORKER00002","agent_type":"dev"}'
rc="$(printf '%s' "$NOSID" | WORKER_EVENTS_TEAMS_DIR="$TEAMS2" HOME="$HOME2" RICHOS_WORKTREE_LEDGER="$WL" "$STARTED_HOOK" >/dev/null 2>&1; echo $?)"
if [ "$rc" = "0" ] && grep -q '"agent_id": "aTESTWORKER00002"' "$TEAMS2/session-ffffffff/worker-events.jsonl" 2>/dev/null \
   && [ ! -e "$HOME2/.claude/worker-events.jsonl" ]; then
    pass "9b  control: a payload with NO session id still goes to the single directory that exists — the guess serves the case it was written for and no other"
else
    fail "9b  control: no-session-id row: single-dir=$(grep -c aTESTWORKER00002 "$TEAMS2/session-ffffffff/worker-events.jsonl" 2>/dev/null || echo 0) fallback=$([ -e "$HOME2/.claude/worker-events.jsonl" ] && echo written || echo missing) rc=$rc"
fi
# the same rule in the other three writers, by the same two files
for hook in "$CREATED_HOOK" "$UPDATED_HOOK" "$ENDED_HOOK"; do
    b="$(basename "$hook")"
    rm -f "$HOME2/.claude/worker-events.jsonl" "$TEAMS2/session-ffffffff/worker-events.jsonl"
    case "$b" in
        worker-created-handoff.sh) P="$SPAWN_OK" ;;
        worker-updated-handoff.sh) P='{"hook_event_name":"SubagentStop","session_id":"'"$SESSION_ID"'","agent_id":"aTESTWORKER00001","agent_type":"dev","tool_name":"TaskUpdate","tool_input":{"status":"in_progress"}}' ;;
        *) P="$ENDED" ;;
    esac
    printf '%s' "$P" | WORKER_EVENTS_TEAMS_DIR="$TEAMS2" HOME="$HOME2" RICHOS_WORKTREE_LEDGER="$WL" "$hook" >/dev/null 2>&1
    if [ -e "$TEAMS2/session-ffffffff/worker-events.jsonl" ]; then
        fail "9c  $b: filed a known session's row into the one foreign directory"
    elif [ "$b" = "worker-updated-handoff.sh" ] && [ ! -e "$HOME2/.claude/worker-events.jsonl" ]; then
        pass "9c  $b: wrote nothing into the foreign directory (and nothing at all for this payload, which is its own rule)"
    else
        pass "9c  $b: a known session's row never lands in the one foreign directory"
    fi
done

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "ALL $PASS WORKER-LIFECYCLE TESTS PASSED"
    exit 0
fi
echo "$FAIL WORKER-LIFECYCLE TEST(S) FAILED ($PASS passed)"
exit 1
