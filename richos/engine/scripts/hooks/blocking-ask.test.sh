#!/usr/bin/env bash
#
# blocking-ask.test.sh — the lead must not go deaf while it waits for the CEO.
#
# Item B of richos-hq docs/operations/2026-10-01-escalation-wakes-the-lead.md
# and Sage's review of it (richos-hq 8ba32b71, items 8, 9 and 10): the
# deaf-lead check in scripts/hooks/guard-ceo-ruled-ask.sh (predicate:
# scripts/lib/blocking_ask.py, declaration: scripts/blocking-ask-exempt.sh) and
# the Stop notice scripts/hooks/notice-unanswered-question.sh.
#
# Every case that expects the call ALLOWED is paired, in the same case, with
# the same fixture REFUSED, so the only difference is the one thing the case is
# about. None of them passes on a checkout where the check does not exist.
#
#   B01  AskUserQuestion with a live teammate of this session: refused (exit 2),
#        naming the teammate, the `QUESTION FOR YOU:` lead-in and the
#        declaration command
#   B02  the same call in a session with no live teammate: allowed
#   B03  after blocking-ask-exempt.sh for this session: allowed; a reason under
#        20 characters is refused by the script and declares nothing
#   B04  this turn was already refused at Stop (the exit-2 feedback row, and the
#        hook_blocking_error row): allowed; the same refusal in an EARLIER turn:
#        still refused
#   B05  a teammate's own AskUserQuestion (agent_id): allowed
#   B06  the teammate's run has ended: allowed
#   Q01  a `QUESTION FOR YOU:` final message from an earlier turn that no human
#        prompt has answered is repeated in the Stop systemMessage
#   Q02  the question's own turn end says nothing (it is already on screen)
#   Q03  his next prompt answers it: nothing more; a notification turn does not
#   Q04  hooks.json registers notice-unanswered-question.sh once, on Stop
#
# Every transcript row is a HOST row in shape (top-level and attachment keys),
# copied from real lead transcripts with free text replaced: the turn-start
# user row (459554d9 line 33), the exit-2 Stop feedback row (02c7d12c line 743),
# the hook_blocking_error row (212ab083 line 7605), an assistant text row.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$ENGINE/scripts/hooks/guard-ceo-ruled-ask.sh"
NOTICE="$ENGINE/scripts/hooks/notice-unanswered-question.sh"
EXEMPT="$ENGINE/scripts/blocking-ask-exempt.sh"

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

# ALLOCATED, NOT NAMED (scripts/lib/scratch.sh): a run stopped by a signal
# leaves a directory the sweeper finds without being told its name.
# shellcheck source=../lib/scratch.sh
. "$ENGINE/scripts/lib/scratch.sh"
SB="$(scratch_new blocking-ask-test)" || { echo "FATAL: no scratch" >&2; exit 1; }
SB="$(cd "$SB" && pwd -P)"
PIDS=()
cleanup() {
    local p
    for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
    rm -rf "$SB"
}
trap cleanup EXIT

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

# --- isolation: a fixture entity, registry, sessions and projects -----------
unset CLAUDE_PROJECT_DIR CLAUDE_PLUGIN_ROOT RICHOS_ENGINE_ROOT RICHOS_AGENT_OWNER
export RICHOS_ENTITY_ROOT="$SB/entity"
export RICHOS_WORKSPACES_DIR="$SB/ws"
export RICHOS_SESSIONS_DIR="$SB/platform-sessions"
export RICHOS_PROJECTS_DIR="$SB/projects"
mkdir -p "$RICHOS_ENTITY_ROOT" "$RICHOS_WORKSPACES_DIR/sessions" "$RICHOS_WORKSPACES_DIR/agents" \
         "$RICHOS_SESSIONS_DIR" "$RICHOS_PROJECTS_DIR"
printf 'PROTECTED_PATHS=""\n' >"$RICHOS_ENTITY_ROOT/orchestration.config"
SID="beadfeed-0000-4000-8000-0000000b0001"
SID2="beadfeed-0000-4000-8000-0000000b0002"
SLEEPER="$(sh -c 'sleep 900 >/dev/null 2>&1 & echo $!')"; PIDS+=("$SLEEPER")

python3 - "$ENGINE" "$RICHOS_WORKSPACES_DIR" "$SID" "$SLEEPER" <<'PY'
import json, os, sys, time
sys.path.insert(0, os.path.join(sys.argv[1], "mega-lander"))
import workspaces
engine, reg, sid, pid = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
st, start = workspaces.process_start(pid)
ident = {"pid": pid, "pid_start": start}
with open(os.path.join(reg, "sessions", sid + ".json"), "w") as fh:
    json.dump(dict(ident, session_id=sid), fh)
now = time.time()
def agent(name, aid, ended):
    rec = workspaces.new_record("%s--%s" % (sid, name), name=name, session_id=sid, agent_id=aid,
                                session_identity=ident, started_at=workspaces.iso(now - 600), workspaces=[])
    if ended:
        rec["end"] = {"at": now - 60, "signal": "SubagentStop", "detail": "fixture"}
        rec["handed_in"] = {"at": now - 61}
    with open(os.path.join(reg, "agents", rec["key"] + ".json"), "w") as fh:
        json.dump(rec, fh)
agent("romeo-opus-b1", "a00000000000b01", False)
PY

row() { # <file> <kind> [text]: append one host-shaped row
    python3 - "$@" <<'PY'
import json, sys
path, kind = sys.argv[1:3]
text = sys.argv[3] if len(sys.argv) > 3 else ""
base = {"isSidechain": False, "userType": "external", "entrypoint": "cli", "cwd": "/fixture",
        "sessionId": "fixture", "version": "2.1.286", "gitBranch": "main",
        "uuid": "00000000-0000-4000-8000-000000000000", "parentUuid": None,
        "timestamp": "2026-10-01T13:02:02.000Z"}
if kind == "human":
    r = dict(base, type="user", promptId="fixture", message={"role": "user", "content": text or "new session"},
             permissionMode="bypassPermissions", origin={"kind": "human"}, promptSource="typed",
             turnOrigin="human", turnPosition={"promptIndex": 1, "turnIndex": 1})
elif kind == "notification":
    r = dict(base, type="user", promptId="fixture",
             message={"role": "user", "content": "<task-notification>\n<summary>Agent \"x\" finished</summary>\n</task-notification>"},
             permissionMode="bypassPermissions", origin={"kind": "task-notification", "producer": "session-task"},
             promptSource="system", turnOrigin="task_notification", turnPosition={"promptIndex": 2, "turnIndex": 9})
elif kind == "stop-feedback":
    r = dict(base, type="user", promptId="fixture",
             message={"role": "user", "content": "Stop hook feedback:\n[bash /fixture/guard-resource-waits.sh]: === A TEAMMATE WAITS ==="})
elif kind == "stop-blocking-error":
    r = dict(base, type="attachment", attachment={"type": "hook_blocking_error", "hookName": "Stop", "toolUseID": "fixture",
             "hookEvent": "Stop", "blockingError": {"blockingError": "fixture block", "command": "/fixture/gate.sh"}})
elif kind == "assistant":
    r = dict(base, type="assistant", message={"role": "assistant", "model": "fixture", "type": "message",
             "content": [{"type": "text", "text": text}]})
else:
    raise SystemExit("unknown row kind " + kind)
with open(path, "a") as fh:
    fh.write(json.dumps(r) + "\n")
PY
}
ask() { # <session> <transcript> [agent-id]: run the gate; stderr in ERR, exit in RC
    local payload
    payload="$(python3 -c 'import json,sys
d={"session_id":sys.argv[1],"transcript_path":sys.argv[2],"hook_event_name":"PreToolUse","tool_name":sys.argv[5],
   "cwd":sys.argv[4],"tool_input":{"questions":[{"question":"Which phone run goes first?","header":"Order",
   "multiSelect":False,"options":[{"label":"D3","description":"the D3 reproduction"},{"label":"R1","description":"the R1 walk"}]}]}}
if sys.argv[3]: d["agent_id"]=sys.argv[3]
print(json.dumps(d))' "$1" "$2" "${3:-}" "$RICHOS_ENTITY_ROOT" "${4:-AskUserQuestion}")"
    ERR="$(printf '%s' "$payload" | bash "$GATE" 2>&1 >/dev/null)"; RC=$?
}
notice() { # <transcript>: run the Stop notice; stdout in OUT
    OUT="$(python3 -c 'import json,sys; print(json.dumps({"session_id":"fixture","transcript_path":sys.argv[1],"hook_event_name":"Stop","stop_hook_active":False,"cwd":sys.argv[2]}))' "$1" "$RICHOS_ENTITY_ROOT" \
          | bash "$NOTICE" 2>/dev/null)"; NRC=$?
}

echo "=== blocking-ask tests ==="
T1="$SB/t1.jsonl"; row "$T1" human
ask "$SID" "$T1"; B01_RC=$RC; B01_ERR="$ERR"
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -qF "QUESTION FOR YOU:" && printf '%s' "$ERR" | grep -qF "romeo-opus-b1" \
   && printf '%s' "$ERR" | grep -qF "blocking-ask-exempt.sh $SID"; then
    ok "B01  a live teammate: refused, naming it, the QUESTION FOR YOU: lead-in and the declaration"
else bad "B01  refused while a teammate is live" "rc=$RC err=$(printf '%s' "$ERR" | head -c 400)"; fi

ask "$SID" "$T1" "" "mcp__richos_questions__ask"
if [ "$B01_RC" -eq 2 ] && [ "$RC" -eq 0 ] && [ -z "$ERR" ]; then
    ok "B07  the nonblocking app question passes while the same terminal question is refused"
else bad "B07  app question is not a blocking terminal box" "rc=$RC err=$ERR"; fi

ask "$SID2" "$T1"
if [ "$B01_RC" -eq 2 ] && [ "$RC" -eq 0 ]; then ok "B02  the same call in a session with no live teammate: allowed"
else bad "B02  no live teammate" "b01=$B01_RC rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

T4="$SB/t4.jsonl"; row "$T4" human; row "$T4" stop-feedback
ask "$SID" "$T4"; B04A=$RC
T4B="$SB/t4b.jsonl"; row "$T4B" human; row "$T4B" stop-blocking-error
ask "$SID" "$T4B"; B04B=$RC
T4C="$SB/t4c.jsonl"; row "$T4C" human; row "$T4C" stop-feedback; row "$T4C" notification
ask "$SID" "$T4C"; B04C=$RC
if [ "$B04A" -eq 0 ] && [ "$B04B" -eq 0 ] && [ "$B04C" -eq 2 ]; then
    ok "B04  refused at Stop in this turn (feedback row, or blocking-error row): allowed; in an earlier turn: still refused"
else bad "B04  a Stop refusal this turn" "feedback=$B04A blocking-error=$B04B earlier-turn=$B04C"; fi

ask "$SID" "$T1" a1234567890abcdef
if [ "$B01_RC" -eq 2 ] && [ "$RC" -eq 0 ]; then ok "B05  a teammate's own AskUserQuestion: allowed"
else bad "B05  a worker's question" "rc=$RC"; fi

SHORT="$(bash "$EXEMPT" "$SID" "too short" 2>&1)"; SRC=$?
ask "$SID" "$T1"; BEFORE=$RC
DECL="$(bash "$EXEMPT" "$SID" "The CEO asked to be asked this one in the terminal picker, by name." 2>&1)"; DRC=$?
ask "$SID" "$T1"; AFTER=$RC
if [ "$SRC" -eq 2 ] && [ "$BEFORE" -eq 2 ] && [ "$DRC" -eq 0 ] && [ "$AFTER" -eq 0 ] \
   && grep -qF "session=$SID" "$RICHOS_ENTITY_ROOT/.claude/state/blocking-ask-exempts.log"; then
    ok "B03  a short reason declares nothing; a declaration for this session lets the ask block"
else bad "B03  the declaration" "short=$SRC before=$BEFORE declare=$DRC after=$AFTER out=$SHORT $DECL"; fi
rm -f "$RICHOS_ENTITY_ROOT/.claude/state/blocking-ask-exempts.log"

python3 - "$RICHOS_WORKSPACES_DIR" <<'PY'
import glob, json, sys, time
for p in glob.glob(sys.argv[1] + "/agents/*.json"):
    r = json.load(open(p))
    r["end"] = {"at": time.time() - 30, "signal": "SubagentStop", "detail": "fixture"}
    r["handed_in"] = {"at": time.time() - 31}
    json.dump(r, open(p, "w"))
PY
ask "$SID" "$T1"
if [ "$B01_RC" -eq 2 ] && [ "$RC" -eq 0 ]; then ok "B06  the teammate's run has ended: allowed"
else bad "B06  ended teammate" "rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

# --- Q: the question is repeated until he answers it ---------------------------
QTEXT="QUESTION FOR YOU:
Which phone run goes first, D3 or R1?"
TQ="$SB/tq.jsonl"; row "$TQ" human; row "$TQ" assistant "Both are ready.

$QTEXT"
notice "$TQ"; Q02="$OUT"
row "$TQ" notification; row "$TQ" assistant "R1 landed."
notice "$TQ"; Q01="$OUT"; Q01RC=$NRC
if [ "$Q01RC" -eq 0 ] && printf '%s' "$Q01" | python3 -c 'import json,sys
d=json.loads(sys.stdin.read()); m=d["systemMessage"]
sys.exit(0 if "STILL WAITING FOR YOUR ANSWER" in m and "Which phone run goes first, D3 or R1?" in m else 1)'; then
    ok "Q01  an unanswered QUESTION FOR YOU: from an earlier turn is repeated in the Stop systemMessage"
else bad "Q01  re-surfaced" "rc=$Q01RC out=$Q01"; fi
if [ -z "$Q02" ] && [ -n "$Q01" ]; then ok "Q02  its own turn end says nothing: it is already the last thing on screen"
else bad "Q02  not at its own turn" "own-turn=$Q02"; fi
row "$TQ" human "R1 first."
notice "$TQ"; Q03="$OUT"
if [ -n "$Q01" ] && [ -z "$Q03" ]; then ok "Q03  his next prompt answers it: nothing more (a notification turn did not)"
else bad "Q03  answered" "after-answer=$Q03"; fi

python3 - "$ENGINE" <<'PY'; RC=$?
import json, os, sys
hooks = json.load(open(os.path.join(sys.argv[1], "hooks", "hooks.json")))["hooks"]
all_ = [h.get("command", "") for ev, gs in hooks.items() for g in gs for h in g.get("hooks", [])
        if "notice-unanswered-question.sh" in h.get("command", "")]
stop = [h.get("command", "") for g in hooks.get("Stop", []) for h in g.get("hooks", [])
        if "notice-unanswered-question.sh" in h.get("command", "")]
sys.exit(0 if len(all_) == 1 and len(stop) == 1 else 1)
PY
if [ "$RC" -eq 0 ]; then ok "Q04  hooks.json registers notice-unanswered-question.sh once, on Stop"
else bad "Q04  registration" "hooks/hooks.json"; fi

if [ "$FAIL" -gt 0 ]; then
    echo "=== blocking-ask tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== blocking-ask tests: all $PASS passed ==="
exit 0
