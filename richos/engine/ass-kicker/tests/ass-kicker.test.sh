#!/usr/bin/env bash
#
# ass-kicker.test.sh — regression tests for the Stop-time gate that
# refuses a turn whose REPORT does not match its ACTIONS
# (scripts/hooks/ass-kicker.sh + ass-kicker/ass-kicker.py).
#
# THE REAL FAILURES ARE REPLAYED HERE BY SHAPE, WITH THE REAL SENTENCES.
#   The two ARM 1 sentences are the lead's own words from 2026-09-02 and are
#   short enough to carry; everything around them is invented. The measurement
#   that decided which arms block was run against the real transcripts on disk
#   and only its NUMBERS are recorded, in the hook header and the corpus file.
#
# Covers:
#   ARM 1 — STATED, NOT TAKEN
#     (a)  "Zach builds it tomorrow." + Bash only               -> exit 2, names the clause
#     (a2) "Frank breaks it first, and I want him attacking
#           whether ... still ... now that ..." + Bash only     -> exit 2
#          (the clause split keeps the subject clause; the `whether`, `still`
#           and `now that` of the SECOND clause do not exempt the first)
#     (b)  same sentence + Agent(frank) this turn                -> exit 0
#     (b2) role act + Agent(sage) — any dispatch discharges 1a   -> exit 0
#     (b3) role act + SendMessage to a teammate                  -> exit 0
#     (c)  "I'm dispatching Frank now." + no Agent               -> exit 2  (1b positive probe)
#     (c2) 1b + Agent(sage) — 1b is role-matched                 -> exit 2
#     (c3) 1b + Agent(frank-opus-x1)                              -> exit 0
#     (d)  "Say the word and I'll dispatch Frank."               -> exit 0  (proposal)
#     (e)  the sentence inside double quotes                     -> exit 0
#     (e2) the sentence inside a fenced ```ecs``` block           -> exit 0
#     (e3) "Frank breaks it first, I said, and ..." (reported)   -> exit 0
#     (f)  "- Zach builds it tomorrow" as a list item             -> exit 0
#     (f2) "Zach builds it — in flight now." (progress)          -> exit 0
#     (f3) "**1. Zach builds it tomorrow.**" (emphasized number) -> exit 0
#     (g)  "Frank recommends the hybrid." (report verb)          -> exit 0
#     (h)  "Art designs Bootstrap components." (bare object)     -> exit 0
#     (i)  role act, then "Want me to post the brief first?"     -> exit 0  (ends on CEO)
#     (j)  "Sage will fold it in tonight." (1c, report-only)     -> exit 0 + observation
#     (k)  role act + AskUserQuestion this turn                  -> exit 0
#     (l)  Agent only in an EARLIER prompt, role act now         -> exit 2  (turn scoping)
#     (m)  no roles on disk                                      -> exit 0  (inert, recorded)
#     (o)  "Frank breaks it the moment Sage returns."            -> exit 0  (conditional)
#     (p)  "Frank breaks it, but not tonight."                   -> exit 0  (negation)
#   ARM 2 — THE TURN THAT STOPS
#     (n)  Agent "X" finished, nothing started, no declaration   -> exit 2, names X + the form
#     (n2) same + Agent call                                      -> exit 0
#     (n3) same + backgrounded Bash                               -> exit 0
#     (n4) same + AskUserQuestion                                 -> exit 0
#     (n5) same + he typed "hold everything"                      -> exit 2
#          (INVERTED 2026-09-20: his words are not read here any more)
#     (n6) same + valid stop-declared: line                       -> exit 0, SHOWN as declared, not verified
#     (n7) declaration too short                                  -> exit 2 + REJECTED + why
#     (n8) declaration inside a code span                         -> exit 2
#     (n9) Agent "X" was stopped by user                          -> exit 0
#     (n10) Background command "..." completed                    -> exit 0
#     (n11) finished but <status> not completed                   -> exit 0
#     (n12) app: new teammate saved, reload to come, reply waits   -> exit 0, nothing added
#     (n13) app: job closed this turn, plain Done report           -> exit 0, nothing added
#     (n14) app: Done report, the turn landed but never closed     -> exit 2
#     (n15) app: Done report, the close was refused                -> exit 2
#     (n16) app: wait reply, the save replaced (no reload comes)   -> exit 2
#     (n17) app: two jobs finish, one is closed, report on both    -> exit 2
#     (n18) app: two jobs finish, both are closed                  -> exit 0
#     (n19) app: n17's turn with no work receipt to read           -> exit 0
#     (n20) app: n17 with both teammates under one title           -> exit 2
#     (n21) app: Done report, a save and then a refused close      -> exit 2
#   ARM 3 — APP RECORD TOOLS
#     (R56) app team saves answered with paths, "I saved Norm"     -> no alert
#     (R57) saved paths named + job report recorded: true          -> no alert
#     (R58) the team save was refused, "I saved Norm"              -> alert
#   SAFETY — never strand a session
#     (r)  stop_hook_active=true, violation present               -> exit 0
#     (s)  repository has not adopted the engine                  -> exit 0, silent
#     (t)  CHECK_STATED_ACTIONS=0                                  -> exit 0 + STOOD DOWN notice
#     (u)  unparseable payload                                     -> exit 0
#     (v)  no prompt_id                                            -> exit 0, silent
#     (w)  transcript missing                                      -> exit 0
#     (x)  broken install (resolve-roots.sh absent)                -> exit 0 + banner
#     (y)  STATED_ACTIONS_ENFORCE=0                                -> exit 0 + report on stderr
#     (y2) guard-idle-land.py absent                               -> exit 0 + NOT RUNNING notice
#   RECORD
#     (z)  an observation line is appended
#
# Run directly: ass-kicker/tests/ass-kicker.test.sh
# Exit 0 = all pass; exit 1 = at least one failure.

set -uo pipefail

TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$TEST_DIR/../.." && pwd)"
SCRIPT_DIR="$ENGINE_ROOT/scripts/hooks"
HOOK="$SCRIPT_DIR/ass-kicker.sh"
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "$2"; FAIL=$((FAIL + 1)); }

SANDBOX="$(mktemp -d -t ass-kicker.XXXXXX)"
trap 'rm -rf "$SANDBOX"' EXIT
SANDBOX="$(cd "$SANDBOX" && pwd -P)"
# The record lookup reads the workspace registry's repository list: a sandbox one, never the
# operator's live registry (a live richos-hq once resolved a fixture SHA here).
export RICHOS_WORKSPACES_DIR="$SANDBOX/registry-none"

command -v python3 >/dev/null 2>&1 || { echo "ERROR: python3 required" >&2; exit 1; }

PROMPT_ID="11111111-2222-3333-4444-555555555555"
OLD_PROMPT="00000000-0000-4000-8000-000000000000"

# --- the governed repository ----------------------------------------------
ENTITY="$SANDBOX/entity"
mkdir -p "$ENTITY/.claude/agents"
git -C "$ENTITY" init -q -b main . >/dev/null 2>&1
git -C "$ENTITY" config user.email t@t.t >/dev/null 2>&1
git -C "$ENTITY" config user.name t >/dev/null 2>&1
mkdir -p "$SANDBOX/nohooks"
git -C "$ENTITY" config core.hooksPath "$SANDBOX/nohooks" >/dev/null 2>&1
: > "$ENTITY/orchestration.config"
for r in frank zach sage mark art; do printf -- '---\nname: %s\n---\n' "$r" > "$ENTITY/.claude/agents/$r.md"; done
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm seed >/dev/null 2>&1

# A repository that never adopted the engine.
UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"
git -C "$UNADOPTED" init -q . >/dev/null 2>&1

# An entity with NO roles at all.
NOROLES="$SANDBOX/noroles"
mkdir -p "$NOROLES"
git -C "$NOROLES" init -q -b main . >/dev/null 2>&1
git -C "$NOROLES" config user.email t@t.t >/dev/null 2>&1
git -C "$NOROLES" config user.name t >/dev/null 2>&1
git -C "$NOROLES" config core.hooksPath "$SANDBOX/nohooks" >/dev/null 2>&1
: > "$NOROLES/orchestration.config"
git -C "$NOROLES" add -A >/dev/null 2>&1
git -C "$NOROLES" commit -qm seed >/dev/null 2>&1

# --- transcript fixtures ---------------------------------------------------
# At Stop time the transcript holds the turn's tool_use records but NOT the
# final assistant text. The DSL is a JSON list:
#   {"prompt": "..."}            a real operator prompt (carries promptSource)
#   {"machine": "..."}           a host-written prompt (task notification)
#   {"bash": "..."}              an assistant Bash tool_use
#   {"bgbash": "..."}            the same, sent to the background
#   {"agent": "<name>"}          an Agent tool_use with name + subagent_type
#   {"send": "<to>"}             a SendMessage tool_use
#   {"tool": "..."}              any other assistant tool_use, by name
#   {"turn": "<prompt-id>"}      switch the promptId from here on
#   {"write": "<path>"}          a Write tool_use (ARM 3)
#   {"edit": "<path>"}           an Edit tool_use (ARM 3)
#   {"bashout": [cmd, out]}      a Bash tool_use whose result is <out> (ARM 3:
#                                the commit line a SHA is checked against)
#   {"app": [name, input, answer, refused?]}
#                                an app work tool_use (mcp__richos_work__*) whose
#                                result is <answer> as the app returns it (ARM 2)
mk_tr() { # <outfile> <json-spec>
    python3 - "$1" "$2" "$PROMPT_ID" "$ENTITY" <<'PY'
import json, sys
out, spec, pid, cwd = sys.argv[1:5]
cur = pid
rows = []
n = 0
def user(text, machine):
    rows.append({"type": "user", "promptId": cur, "cwd": cwd,
                 "promptSource": "user" if not machine else "hook",
                 "message": {"content": text}})
def call(name, inp, result="ok"):
    global n
    n += 1
    tid = "toolu_%03d" % n
    rows.append({"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": tid, "name": name, "input": inp}]}})
    rows.append({"type": "user", "promptId": cur, "cwd": cwd,
                 "message": {"content": [
                     {"type": "tool_result", "tool_use_id": tid, "content": result}]}})
for step in json.loads(spec):
    if "turn" in step:
        cur = step["turn"]; continue
    if "prompt" in step:
        user(step["prompt"], False); continue
    if "machine" in step:
        user(step["machine"], True); continue
    if "bash" in step:
        call("Bash", {"command": step["bash"]}); continue
    if "bgbash" in step:
        call("Bash", {"command": step["bgbash"], "run_in_background": True}); continue
    if "agent" in step:
        name = step["agent"]
        call("Agent", {"name": name, "subagent_type": name.split("-")[0], "prompt": "go"}); continue
    if "send" in step:
        call("SendMessage", {"to": step["send"], "message": "hi"}); continue
    if "tool" in step:
        call(step["tool"], {}); continue
    if "write" in step:
        call("Write", {"file_path": step["write"], "content": "x"}); continue
    if "edit" in step:
        call("Edit", {"file_path": step["edit"], "old_string": "a", "new_string": "b"}); continue
    if "bashout" in step:
        call("Bash", {"command": step["bashout"][0]}, step["bashout"][1]); continue
    if "app" in step:
        # An app work tool exactly as the provider records it: the result is a list of one
        # text block holding the tool's JSON answer, and a refusal is marked is_error.
        name, inp, answer = step["app"][:3]
        refused = len(step["app"]) > 3 and step["app"][3]
        n += 1
        tid = "toolu_%03d" % n
        rows.append({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": tid, "name": name, "input": inp}]}})
        block = {"type": "tool_result", "tool_use_id": tid,
                 "content": [{"type": "text", "text": answer if isinstance(answer, str) else json.dumps(answer)}]}
        if refused:
            block["is_error"] = True
        rows.append({"type": "user", "promptId": cur, "cwd": cwd, "message": {"content": [block]}})
        continue
with open(out, "w", encoding="utf-8") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")
PY
}

FINISHED='<task-notification><task-id>a1</task-id><status>completed</status><summary>Agent "Break the elimination design" finished</summary><result>done</result></task-notification>'
KILLED='<task-notification><task-id>a1</task-id><status>completed</status><summary>Agent "Break the elimination design" was stopped by user</summary></task-notification>'
SHELL_DONE='<task-notification><task-id>b1</task-id><status>completed</status><summary>Background command "npm test" completed</summary></task-notification>'
NOT_COMPLETED='<task-notification><task-id>a1</task-id><status>failed</status><summary>Agent "Break the elimination design" finished</summary></task-notification>'

TR_BASH="$SANDBOX/bash.jsonl";        mk_tr "$TR_BASH"    '[{"prompt":"go"},{"bash":"git status"},{"bash":"ls"}]'
TR_FRANK="$SANDBOX/frank.jsonl";      mk_tr "$TR_FRANK"   '[{"prompt":"go"},{"bash":"git status"},{"agent":"frank-opus-x1"}]'
TR_SAGE="$SANDBOX/sage.jsonl";        mk_tr "$TR_SAGE"    '[{"prompt":"go"},{"agent":"sage-opus-x1"}]'
TR_SEND="$SANDBOX/send.jsonl";        mk_tr "$TR_SEND"    '[{"prompt":"go"},{"send":"frank-opus-x1"}]'
TR_ASK="$SANDBOX/ask.jsonl";          mk_tr "$TR_ASK"     '[{"prompt":"go"},{"tool":"AskUserQuestion"}]'
TR_EARLIER="$SANDBOX/earlier.jsonl";  mk_tr "$TR_EARLIER" "[{\"turn\":\"$OLD_PROMPT\"},{\"prompt\":\"before\"},{\"agent\":\"frank-opus-x1\"},{\"turn\":\"$PROMPT_ID\"},{\"prompt\":\"now\"},{\"bash\":\"ls\"}]"
TR_FIN="$SANDBOX/fin.jsonl";          mk_tr "$TR_FIN"     "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$FINISHED")},{\"bash\":\"git merge --no-ff x\"}]"
TR_FIN_AGENT="$SANDBOX/fin-agent.jsonl"; mk_tr "$TR_FIN_AGENT" "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$FINISHED")},{\"agent\":\"sage-opus-x2\"}]"
TR_FIN_BG="$SANDBOX/fin-bg.jsonl";    mk_tr "$TR_FIN_BG"  "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$FINISHED")},{\"bgbash\":\"npm test\"}]"
TR_FIN_ASK="$SANDBOX/fin-ask.jsonl";  mk_tr "$TR_FIN_ASK" "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$FINISHED")},{\"tool\":\"AskUserQuestion\"}]"
TR_FIN_HOLD="$SANDBOX/fin-hold.jsonl"; mk_tr "$TR_FIN_HOLD" "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$FINISHED")},{\"prompt\":\"hold everything until I am back\"},{\"bash\":\"ls\"}]"
TR_KILLED="$SANDBOX/killed.jsonl";    mk_tr "$TR_KILLED"  "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$KILLED")},{\"bash\":\"ls\"}]"
TR_SHELL="$SANDBOX/shell.jsonl";      mk_tr "$TR_SHELL"   "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$SHELL_DONE")},{\"bash\":\"ls\"}]"
TR_NOTDONE="$SANDBOX/notdone.jsonl";  mk_tr "$TR_NOTDONE" "[{\"machine\":$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$NOT_COMPLETED")},{\"bash\":\"ls\"}]"

# --- running the hook ------------------------------------------------------
CASE_N=0
OUT=""; ERR=""; RC=0
# run_hook <transcript|-> <message> [stop_hook_active] [entity] [hook] [prompt_id]
run_hook() {
    local tr="$1" msg="$2" active="${3:-false}" ent="${4:-$ENTITY}" hook="${5:-$HOOK}" pid="${6:-$PROMPT_ID}"
    CASE_N=$((CASE_N + 1))
    local sid; sid="$(printf 'feedface-%04d-4000-8000-000000000000' "$CASE_N")"
    local payload="$SANDBOX/payload.json"
    python3 - "$tr" "$msg" "$active" "$ent" "$sid" "$pid" > "$payload" <<'PY'
import json, sys
tr, msg, active, ent, sid, pid = sys.argv[1:7]
p = {"hook_event_name": "Stop", "session_id": sid,
     "transcript_path": (tr if tr != "-" else "/nonexistent/none.jsonl"),
     "cwd": ent, "permission_mode": "default",
     "stop_hook_active": active == "true", "last_assistant_message": msg,
     "background_tasks": [], "session_crons": []}
# "none" means the payload carries NO prompt_id at all (the unscopable turn);
# anything else is the id. The first version of this block deleted a key it
# had not set, crashed, wrote an empty payload, and case (v) passed for the
# wrong reason — an empty stdin exits 0 too.
if pid != "none":
    p["prompt_id"] = pid
print(json.dumps(p))
PY
    local o e
    o="$(mktemp "$SANDBOX/o.XXXXXX")"; e="$(mktemp "$SANDBOX/e.XXXXXX")"
    RICHOS_ENTITY_ROOT="$ent" bash "$hook" < "$payload" > "$o" 2> "$e"
    RC=$?
    OUT="$(cat "$o")"; ERR="$(cat "$e")"
    rm -f "$o" "$e"
}
# run_unrooted <transcript> <message> <cwd> — no explicit root; resolution is
# from the payload's cwd, which is how the not-adopted case is reached.
run_unrooted() {
    local tr="$1" msg="$2" ent="$3"
    CASE_N=$((CASE_N + 1))
    local payload="$SANDBOX/payload.json"
    python3 - "$tr" "$msg" "$ent" "$PROMPT_ID" > "$payload" <<'PY'
import json, sys
tr, msg, ent, pid = sys.argv[1:5]
print(json.dumps({"hook_event_name": "Stop", "session_id": "feedface-9999-4000-8000-000000000000",
                  "transcript_path": tr, "cwd": ent, "prompt_id": pid,
                  "stop_hook_active": False, "last_assistant_message": msg}))
PY
    local o e
    o="$(mktemp "$SANDBOX/o.XXXXXX")"; e="$(mktemp "$SANDBOX/e.XXXXXX")"
    ( cd "$ent" && env -u RICHOS_ENTITY_ROOT bash "$HOOK" < "$payload" > "$o" 2> "$e" )
    RC=$?
    OUT="$(cat "$o")"; ERR="$(cat "$e")"
    rm -f "$o" "$e"
}
has_sysmsg() { # <needle>
    python3 -c 'import json,sys; d=json.loads(sys.stdin.readline() or "{}"); m=d.get("systemMessage",""); sys.exit(0 if isinstance(m,str) and sys.argv[1] in m else 1)' "$1" <<<"$OUT"
}

ZACH="Landed at abc1234. Sixty-five lines get added and none removes anything.

Zach builds it tomorrow."
FRANK="Design 4 landed, and it found something that reframes the whole eleven days.

I'm not accepting it on its summary. Frank breaks it first, and I want him attacking whether −66% is still too generous now that the platform's own record turns out to exist."
FIN_REPORT="Both arms are in, and they answer the question that has consumed eleven days. The design holds."

echo "=== ass-kicker: the report must match the turn ==="
echo ""

# ---------------------------------------------------------------------------
# ARM 1
# ---------------------------------------------------------------------------
run_hook "$TR_BASH" "$ZACH"
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'Zach builds it tomorrow' && printf '%s' "$ERR" | grep -q 'no Agent call for zach'; then
    ok "a.  a role act with no Agent call this turn is REFUSED, and the refusal quotes the clause"
else bad "a.  role act refused" "rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$TR_BASH" "$FRANK"
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'Frank breaks it first'; then
    ok "a2. the second real sentence is refused — the clause split keeps 'Frank breaks it first' and the second clause's 'whether/still/now that' do not exempt it"
else bad "a2. Frank breaks it first" "rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$TR_FRANK" "$FRANK"
[ "$RC" -eq 0 ] && ok "b.  the same sentence with an Agent call for frank this turn is let through" || bad "b.  same-turn dispatch" "rc=$RC err=$(printf '%s' "$ERR" | head -c 200)"

run_hook "$TR_SAGE" "$ZACH"
[ "$RC" -eq 0 ] && ok "b2. a role act is discharged by ANY Agent call this turn (a plan in motion is not the defect)" || bad "b2. any Agent discharges 1a" "rc=$RC"

run_hook "$TR_SEND" "$ZACH"
[ "$RC" -eq 0 ] && ok "b3. a SendMessage to a teammate this turn discharges a role act" || bad "b3. SendMessage discharges" "rc=$RC"

run_hook "$TR_BASH" "I'm dispatching Frank now, with the full brief."
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'in the first person'; then
    ok "c.  POSITIVE PROBE for 1b: 'I'm dispatching Frank now' with no Agent call is refused"
else bad "c.  1b positive probe" "rc=$RC err=$(printf '%s' "$ERR" | head -c 200)"; fi

run_hook "$TR_SAGE" "I'm dispatching Frank now, with the full brief."
[ "$RC" -eq 2 ] && ok "c2. 1b is ROLE-MATCHED: an Agent call for sage does not discharge 'I'm dispatching Frank'" || bad "c2. 1b role-matched" "rc=$RC"

run_hook "$TR_FRANK" "I'm dispatching Frank now, with the full brief."
[ "$RC" -eq 0 ] && ok "c3. ...and an Agent call for frank does" || bad "c3. 1b discharged by frank" "rc=$RC"

# ONE DEFENSE PER FIXTURE. The mutation run found three fixtures here carrying
# two defenses at once (a quoted sentence inside a fence; a proposal that was
# also the reply's last sentence), so removing either property left the suite
# green over a defense it was not testing. Each of (d), (e), (e2) now reaches
# exactly one property.
run_hook "$TR_BASH" "Say the word and I'll dispatch Frank. The rest stays exactly as it is."
[ "$RC" -eq 0 ] && ok "d.  a dispatch conditioned on the CEO's word is a proposal, not a claim (and it is not the last sentence, so only the clause rule protects it)" || bad "d.  proposal" "rc=$RC err=$(printf '%s' "$ERR" | head -c 200)"

run_hook "$TR_BASH" "The sentence \"I'm dispatching Frank now\" was the failure, quoted here as evidence."
[ "$RC" -eq 0 ] && ok "e.  a dispatch claim inside double quotes is quoted speech, not a claim" || bad "e.  quoted" "rc=$RC"

run_hook "$TR_BASH" "The transcript line, verbatim:

\`\`\`
Zach builds it tomorrow.
\`\`\`

That is the sentence under discussion."
[ "$RC" -eq 0 ] && ok "e2. the sentence inside a fenced block is not scanned" || bad "e2. fenced" "rc=$RC"

run_hook "$TR_BASH" "Frank breaks it first, I said, and then I wrote a status report."
[ "$RC" -eq 0 ] && ok "e3. reported speech ('I said') in the sentence is not a claim" || bad "e3. reported" "rc=$RC"

run_hook "$TR_BASH" "The sequence:

- Sage designs the lease
- Zach builds it tomorrow
- I land it"
[ "$RC" -eq 0 ] && ok "f.  a list item is a plan, not a statement, and is not scanned" || bad "f.  list item" "rc=$RC"

run_hook "$TR_BASH" "Zach builds it — in flight now. Worktree cut, no commits yet."
[ "$RC" -eq 0 ] && ok "f2. a present-progress marker after a dash stays in the clause and marks it as state, not announcement" || bad "f2. progress" "rc=$RC"

run_hook "$TR_BASH" "**1. Zach builds it tomorrow.** Then I land it."
[ "$RC" -eq 0 ] && ok "f3. an emphasized numbered line is still a list item" || bad "f3. emphasized list" "rc=$RC"

run_hook "$TR_BASH" "Frank recommends the hybrid, and I agree with him."
[ "$RC" -eq 0 ] && ok "g.  a report verb ('recommends') describes an agent's output, not an act to take" || bad "g.  report verb" "rc=$RC"

run_hook "$TR_BASH" "Art designs Bootstrap components for a mobile coaching app."
[ "$RC" -eq 0 ] && ok "h.  a bare-noun object is a description of a craft, not an announced act" || bad "h.  bare object" "rc=$RC"

run_hook "$TR_BASH" "$ZACH

Want me to post the brief first?"
[ "$RC" -eq 0 ] && ok "i.  a reply that ENDS on a question to the CEO is a proposal; the turn may end on his answer" || bad "i.  ends on CEO" "rc=$RC"

run_hook "$TR_BASH" "Sage will fold it in tonight, with the count."
if [ "$RC" -eq 0 ] && printf '%s' "$ERR" | grep -q 'not blocking'; then
    ok "j.  the role-future shape (0/2 on the corpus) REPORTS and never blocks"
else bad "j.  role-future reports" "rc=$RC err=$(printf '%s' "$ERR" | head -c 200)"; fi

run_hook "$TR_ASK" "$ZACH"
[ "$RC" -eq 0 ] && ok "k.  an AskUserQuestion this turn exempts the turn — the CEO is being asked" || bad "k.  AskUserQuestion" "rc=$RC"

run_hook "$TR_EARLIER" "$ZACH"
[ "$RC" -eq 2 ] && ok "l.  TURN SCOPING: an Agent call in an EARLIER prompt does not discharge this turn's claim" || bad "l.  turn scoping" "rc=$RC"

run_hook "$TR_BASH" "$ZACH" false "$NOROLES"
if [ "$RC" -eq 0 ] && grep -q '"roles": 0' "$NOROLES/.claude/state/stated-actions.jsonl" 2>/dev/null; then
    ok "m.  with no roles on disk ARM 1 is inert, and the record says roles: 0"
else bad "m.  no roles" "rc=$RC record=$(cat "$NOROLES/.claude/state/stated-actions.jsonl" 2>/dev/null | tail -1)"; fi

run_hook "$TR_BASH" "Frank breaks it the moment Sage returns."
[ "$RC" -eq 0 ] && ok "o.  a conditional ('the moment ...') in the clause is not an announcement" || bad "o.  conditional" "rc=$RC"

run_hook "$TR_BASH" "Frank breaks it, but not tonight."
[ "$RC" -eq 0 ] && ok "p.  a negation anywhere in the sentence is not an announcement (it is a deferral, owned elsewhere)" || bad "p.  negation" "rc=$RC"

# ---------------------------------------------------------------------------
# ARM 2
# ---------------------------------------------------------------------------
run_hook "$TR_FIN" "$FIN_REPORT"
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'Break the elimination design' \
   && printf '%s' "$ERR" | grep -q 'stop-declared: <case>' \
   && printf '%s' "$ERR" | grep -q 'waiting-on-teammate'; then
    ok "n.  a teammate finished, nothing started, nothing declared -> REFUSED, naming the agent and the exact declaration line"
else bad "n.  undeclared stop refused" "rc=$RC err=$(printf '%s' "$ERR" | head -c 400)"; fi

run_hook "$TR_FIN_AGENT" "$FIN_REPORT"
[ "$RC" -eq 0 ] && ok "n2. ...and the same turn with an Agent call is let through" || bad "n2. started" "rc=$RC"

run_hook "$TR_FIN_BG" "$FIN_REPORT"
[ "$RC" -eq 0 ] && ok "n3. a BACKGROUNDED tool call counts as started" || bad "n3. backgrounded" "rc=$RC"

run_hook "$TR_FIN_ASK" "$FIN_REPORT"
[ "$RC" -eq 0 ] && ok "n4. a question put to the CEO this turn is owed an answer; the turn may end" || bad "n4. ask" "rc=$RC"

# n5 INVERTED, 2026-09-20 (CEO ruling §68). The arm used to import
# guard-idle-land.py's hold_signal() and excuse the turn when his own prompt
# carried "hold everything". The predicate is deleted there and the call is
# deleted here: a hook does not decide what he meant. The SAME fixture is now
# refused, and n6 below is the route that ends such a turn — the orchestrator
# declaring the stop in its own words.
run_hook "$TR_FIN_HOLD" "$FIN_REPORT"
[ "$RC" -eq 2 ] && ok "n5. a hold IN HIS OWN WORDS no longer stands the arm down" || bad "n5. hold-in-his-words" "rc=$RC"

DECL="$FIN_REPORT

stop-declared: waiting-on-teammate — the other two designers are still running and the review needs both before anything moves."
run_hook "$TR_FIN" "$DECL"
if [ "$RC" -eq 0 ] && has_sysmsg "STOP DECLARED" && has_sysmsg "NOT VERIFIED" && has_sysmsg "waiting-on-teammate"; then
    ok "n6. a valid declaration lets the turn through AND is shown to the operator as declared, not verified"
else bad "n6. declaration shown" "rc=$RC out=$(printf '%s' "$OUT" | head -c 300)"; fi

run_hook "$TR_FIN" "$FIN_REPORT

stop-declared: waiting-on-teammate — later."
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'REJECTED'; then
    ok "n7. a declaration short on words is REJECTED and the refusal says why"
else bad "n7. short declaration" "rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$TR_FIN" "$FIN_REPORT

The form is \`stop-declared: waiting-on-teammate — the other two designers are still running and the review needs both.\` and I am not using it."
[ "$RC" -eq 2 ] && ok "n8. a declaration inside a code span exempts nothing" || bad "n8. code span" "rc=$RC"

run_hook "$TR_KILLED" "Frank left nothing — killed before it wrote a line."
[ "$RC" -eq 0 ] && ok "n9. an agent the operator STOPPED is not a delivery" || bad "n9. killed" "rc=$RC"

run_hook "$TR_SHELL" "The suite is green."
[ "$RC" -eq 0 ] && ok "n10. a finished BACKGROUND COMMAND is not a teammate" || bad "n10. shell" "rc=$RC"

run_hook "$TR_NOTDONE" "$FIN_REPORT"
[ "$RC" -eq 0 ] && ok "n11. a notification without <status>completed</status> is not a delivery" || bad "n11. not completed" "rc=$RC"

# n12-n16: STU'S TURNS IN THE APP, from the nightly 74 walk (check 5, Stu's transcript
# 9cadeff2…jsonl, kept.tgz in richos-hq docs/audits/2026-10-15-nightly-root/evidence/
# nightly-74-walk/check5-work-lease/kept/). The notices, the app tools' answers and the replies
# are those rows; ids and paths are shortened. Both truthful replies were refused there
# (rows 146 and 327), and nightly 73 refused the same pair (4beb2145…jsonl rows 160 and 308).
APP_SPECS="$SANDBOX/app-specs"
mkdir -p "$APP_SPECS"
APP_STATE="$SANDBOX/app-state"
APP_PART="2a2183bda91a03d6c8458d75216155d64922946bc022736669853e991dfa21ec"
python3 - "$APP_SPECS" "$APP_STATE/work-receipts/$APP_PART" <<'PY'
import json, os, sys
out, receipts = sys.argv[1:3]
def notice(title, task="a191b4bf3c4ab6b88"):
    return ("<task-notification>\n<task-id>%s</task-id>\n<status>completed</status>\n"
            "<summary>Agent \"%s\" finished</summary>\n<result>done</result>\n</task-notification>" % (task, title))
save = ["mcp__richos_work__team", {"action": "save", "name": "norm", "text": "---\nname: norm\n---\n"}]
saved = {"saved": "/home/Library/Application Support/com.richos.app/team/norm.md", "name": "norm",
         "replaced": False, "registered": False, "registration_boundary": "next_provider_connection"}
integrate = ["mcp__richos_work__integrate", {"worker_id": "2b46", "reviewer_id": "cf86"},
             {"work_integrated": True, "commit": "64f2ad7", "cleanup_pending": [],
              "obligation_closed": False, "published": False}]
complete = ["mcp__richos_work__complete", {"obligation_id": "work-94e48f43", "worker_ids": ["2b46"]}]
closed = {"obligation_closed": True, "obligation_id": "work-94e48f43",
          "evidence_ref": "app-completion:5860d42f", "published": False}
send = ["mcp__richos_status__send", {"text": "Done: notes.md is committed and landed."}, {"saved": True, "to": "rich"}]
review = {"machine": notice("Review notes.md commit for Acme")}
# The second review's fixture: notes.md's reviewer and more.md's worker (another obligation,
# ordinary concurrent use) finish in the same turn, and only notes.md is closed. Their work
# receipts are the app's own (nightly 74's cf86... and c2ad..., shortened): the agent id each
# notice carries, and the obligation the receipt was prepared for.
os.makedirs(receipts)
for rid, agent, obligation, role in (("cf86", "aaff1d866a945cba3", "work-94e48f43", "reviewer"),
                                     ("c2ad", "a02f76eded281b9d6", "work-9953e2af", "worker")):
    with open(os.path.join(receipts, rid + ".json"), "w", encoding="utf-8") as fh:
        json.dump({"schema": 1, "id": rid, "agent_id": agent, "status": "run-ended",
                   "request": {"obligation_id": obligation, "role": role}}, fh)
two_jobs = [{"machine": notice("Review notes.md commit for Acme", "aaff1d866a945cba3")},
            {"machine": notice("Add more.md to Acme", "a02f76eded281b9d6")}, {"app": integrate}]
closed_more = {"app": ["mcp__richos_work__complete", {"obligation_id": "work-9953e2af", "worker_ids": ["c2ad"]},
                       dict(closed, obligation_id="work-9953e2af")]}
specs = {
    # rows 129-146: Dean's consult returns, Stu saves Norm, the app reloads the team after the turn
    "wait": [{"machine": notice("Activate Norm for Acme")}, {"app": save + [saved]}],
    # rows 261-327: the review returns, Stu lands, checks, closes the job and tells Rich
    "done": [review, {"app": integrate}, {"bash": "git -C /home/Acme log --oneline -3"},
             {"tool": "Read"}, {"app": complete + [closed]}, {"app": send}],
    "two-jobs-one-closed": two_jobs + [{"app": complete + [closed]}],
    "two-jobs-both-closed": two_jobs + [{"app": complete + [closed]}, closed_more],
    # review rv-20261025T001232Z-00aed5ab-1fe4: the two jobs' teammates share one title
    "same-title-one-closed": [{"machine": notice("Add notes to Acme", "aaff1d866a945cba3")},
                              {"machine": notice("Add notes to Acme", "a02f76eded281b9d6")},
                              {"app": integrate}, {"app": complete + [closed]}],
    # the same reply over a turn that landed and never closed the job
    "landed-open": [review, {"app": integrate}, {"bash": "git -C /home/Acme log --oneline -3"},
                    {"app": send}],
    # the same reply over a turn whose close the app refused
    "close-refused": [review, {"app": integrate},
                      {"app": complete + ["this assignment still has unresolved execution; "
                                          "reconcile it before completion", True]}, {"app": send}],
    # the wait reply over a save that replaced an existing teammate: no reload follows that
    "replaced": [{"machine": notice("Activate Norm for Acme")},
                 {"app": save + [dict(saved, replaced=True)]}],
    # review rv-20261024T230343Z-5ff59ca2-e966#1 (fixtures/review_arm2.py of
    # rv-20261025T011324Z-5002285a-2e67): a new save, then the refused close, then the Done reply
    "save-close-refused": [review, {"app": save + [saved]}, {"app": integrate},
                           {"app": complete + ["this assignment still has unresolved execution; "
                                               "reconcile it before completion", True]}, {"app": send}],
}
for name, spec in specs.items():
    with open(os.path.join(out, name + ".json"), "w", encoding="utf-8") as fh:
        json.dump(spec, fh)
PY
for s in wait done landed-open close-refused replaced two-jobs-one-closed two-jobs-both-closed same-title-one-closed save-close-refused; do
    mk_tr "$SANDBOX/app-$s.jsonl" "$(cat "$APP_SPECS/$s.json")"
done
# Row 142, verbatim.
STU_WAIT="Norm's definition is saved, and he becomes available after the team reload. When the reload notice arrives I'll brief him to add notes.md."
# Row 326, verbatim.
STU_DONE='notes.md is now in the Acme repository, committed and landed on its main branch. Its only content is the line "Notes for the walk test.", and I read the file back to check the exact characters.

- **The commit:** "Add notes.md" (64f2ad7) changes nothing but the new file. A separate review passed it, and it was merged into main as b12b2a5. The repository is clean, and the temporary work copies have been removed.
- **No tests:** Acme has no automated tests, so reading the file back was the only check.
- **Not pushed:** Acme has no online copy set up (`git remote -v` lists none), so "landed" here means the main branch on this Mac.
- **New teammate:** None of the active teammates was an engineer, so I had Dean activate Norm, a full-stack engineer, to make the change. Norm stays on the team for future Acme work.

The job record is closed.'

run_hook "$SANDBOX/app-wait.jsonl" "$STU_WAIT"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "n12. a teammate saved in the app, its reload still to come: the truthful wait passes, and nothing is added to the reply"
else bad "n12. truthful wait" "rc=$RC out=$(printf '%s' "$OUT" | head -c 200) err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$SANDBOX/app-done.jsonl" "$STU_DONE"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "n13. the job closed this turn: the complete Done report passes without a stop-declared line, and nothing is added"
else bad "n13. truthful Done" "rc=$RC out=$(printf '%s' "$OUT" | head -c 200) err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$SANDBOX/app-landed-open.jsonl" "$STU_DONE"
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'Review notes.md commit for Acme'; then
    ok "n14. a Done report over a job this turn never closed is still REFUSED (a land is not a close)"
else bad "n14. unclosed job" "rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$SANDBOX/app-close-refused.jsonl" "$STU_DONE"
[ "$RC" -eq 2 ] && ok "n15. a Done report over a close the app REFUSED is still refused" || bad "n15. refused close" "rc=$RC"

run_hook "$SANDBOX/app-replaced.jsonl" "$STU_WAIT"
[ "$RC" -eq 2 ] && ok "n16. a wait on a reload that is not coming (an existing teammate replaced) is still refused" || bad "n16. replaced teammate" "rc=$RC"

run_hook "$SANDBOX/app-save-close-refused.jsonl" "$STU_DONE"
[ "$RC" -eq 2 ] && ok "n21. a new teammate save does not excuse a Done report over a close the app refused in the same turn" || bad "n21. save then refused close" "rc=$RC"

TWO_DONE="Done. notes.md and more.md are committed and landed on main. Both jobs are closed."
# n17/n18 run as the app runs its hooks: the work receipts beside the thread's workspace registry.
SAVED_WORKSPACES_DIR="$RICHOS_WORKSPACES_DIR"
export RICHOS_APP_STATE="$APP_STATE" RICHOS_WORKSPACES_DIR="$APP_STATE/workspaces/$APP_PART"
run_hook "$SANDBOX/app-two-jobs-one-closed.jsonl" "$TWO_DONE"
if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -q 'Add more.md to Acme' \
        && ! printf '%s' "$ERR" | grep -q 'Review notes.md commit for Acme'; then
    ok "n17. two jobs finish in one turn and only notes.md is closed: still REFUSED, naming only more.md's teammate"
else bad "n17. other job pending" "rc=$RC err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$SANDBOX/app-two-jobs-both-closed.jsonl" "$TWO_DONE"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "n18. two jobs finish in one turn and both are closed: the report on both passes"
else bad "n18. both closed" "rc=$RC out=$(printf '%s' "$OUT" | head -c 200) err=$(printf '%s' "$ERR" | head -c 300)"; fi

run_hook "$SANDBOX/app-same-title-one-closed.jsonl" "$TWO_DONE"
[ "$RC" -eq 2 ] && ok "n20. two jobs' teammates share one title and only the first job is closed: still REFUSED (agents are told apart by id)" || bad "n20. same title" "rc=$RC"
unset RICHOS_APP_STATE
export RICHOS_WORKSPACES_DIR="$SAVED_WORKSPACES_DIR"

run_hook "$SANDBOX/app-two-jobs-one-closed.jsonl" "$TWO_DONE"
N19_DETAIL="$(tail -1 "$ENTITY/.claude/state/stated-actions.jsonl" 2>/dev/null | python3 -c 'import json,sys
print((json.loads(sys.stdin.read()).get("stop") or {}).get("detail", ""))' 2>/dev/null)"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] && printf '%s' "$N19_DETAIL" | grep -q 'owner not read for: Review notes.md commit for Acme, Add more.md to Acme'; then
    ok "n19. the same turn with no receipt to read: the owner is not guessed, the close is not refused on a guess, and the kept record says whose owner was not read"
else bad "n19. no receipts" "rc=$RC detail=$N19_DETAIL err=$(printf '%s' "$ERR" | head -c 300)"; fi

# R56-R58: ARM 3 over the app's own record tools, from the walk of f7db19c03 (Stu's transcript
# 49b30d21…jsonl, richos-hq docs/audits/2026-10-15-nightly-root/evidence/
# m1-report-guard-walk-f7db19c/kept/kept.tgz). Two team saves answered with their saved paths
# (rows 141, 146), the job report answered recorded: true (row 163), and both truthful replies
# (rows 155, 167) still got "this turn wrote NOTHING" (rows 156-157, 168-169). Run as the re-fire,
# where ARM 3 alone speaks.
python3 - "$APP_SPECS" <<'PY'
import json, os, sys
out = sys.argv[1]
team = "/Users/admin/testvm/walk-9b09c00b8b30/home/Library/Application Support/com.richos.app/team/"
notice = ("<task-notification>\n<task-id>a014432beb72259d3</task-id>\n<status>completed</status>\n"
          "<summary>Agent \"Activate an engineer and a reviewer for Acme\" finished</summary>\n"
          "<result>Both are fitted.</result>\n</task-notification>")
def save(name):
    return {"app": ["mcp__richos_work__team", {"action": "save", "name": name, "text": "---\nname: %s\n---\n" % name},
                    {"saved": team + name + ".md", "name": name, "replaced": False, "registered": False,
                     "registration_boundary": "next_provider_connection"}]}
report = {"app": ["mcp__richos_work__report",
                  {"obligation_id": "work-ffd9dd7010af48b194f07e8037136584", "state": "running",
                   "detail": "Dean fitted shelf teammates norm and tom to Acme, and I saved both."},
                  {"recorded": True, "state": "running"}]}
refused = {"app": ["mcp__richos_work__team", {"action": "save", "name": "norm", "text": "---\nname: norm\n---\n"},
                   "norm is already the user's teammate, and that copy is the user's: it is kept.", True]}
specs = {"saved": [{"machine": notice}, save("norm"), save("tom")],
         "saved-reported": [{"machine": notice}, save("norm"), save("tom"), report],
         "save-refused": [{"machine": notice}, refused]}
for name, spec in specs.items():
    with open(os.path.join(out, name + ".json"), "w", encoding="utf-8") as fh:
        json.dump(spec, fh)
PY
for s in saved saved-reported save-refused; do
    mk_tr "$SANDBOX/app-$s.jsonl" "$(cat "$APP_SPECS/$s.json")"
done
# Row 155, verbatim.
STU_SAVED="I saved Norm and Tom as teammates. Once RichOS reloads the team, Norm will add and commit the file and Tom will review it."
# Row 167, verbatim.
IFS= read -r -d '' STU_SAVED_WHERE <<'EOF' || true
Nothing is in the Acme repository yet. `notes.md` has not been created, committed or landed.

No one on the team could make or review a repository change, so I added two teammates, Norm (engineer) and Tom (reviewer). Their definitions are saved in the app's team folder at `team/norm.md` and `team/tom.md`, and I logged this step on the job's record. They become available when RichOS next reloads the team.

When they're available, Norm will add `notes.md` with the single line "Notes for the walk test." and commit it. Tom will review that commit independently, and then I'll land it on Acme's `main`.
EOF

run_hook "$SANDBOX/app-saved.jsonl" "$STU_SAVED" true
if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q 'RECORD ALERT'; then
    ok "R56. two app team saves that answered with their paths back 'I saved Norm and Tom': no alert"
else bad "R56. app team save" "rc=$RC out=$(printf '%s' "$OUT" | head -c 300)"; fi

run_hook "$SANDBOX/app-saved-reported.jsonl" "$STU_SAVED_WHERE" true
if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q 'RECORD ALERT'; then
    ok "R57. the saved paths named and a job report answered recorded: true back the explanation: no alert"
else bad "R57. app saves + job report" "rc=$RC out=$(printf '%s' "$OUT" | head -c 300)"; fi

run_hook "$SANDBOX/app-save-refused.jsonl" "$STU_SAVED" true
if printf '%s' "$OUT" | grep -q 'RECORD ALERT'; then
    ok "R58. a team save the app REFUSED backs nothing: 'I saved Norm' still alerts"
else bad "R58. refused app save" "rc=$RC out=$(printf '%s' "$OUT" | head -c 300)"; fi

# ---------------------------------------------------------------------------
# ARM 3 — RECORDED, NOT WRITTEN
# The R cases are the real claims the CEO answered with "recorded WHERE?" (or
# its equivalent), each with the shape of what its turn actually wrote. The
# full excerpts, his questions and the replay that adjudicated every fire are
# in the private record; only the lead's own claim sentences are carried here.
# ---------------------------------------------------------------------------
MEM="$SANDBOX/home/.claude/projects/-Users-x-proj/memory"
mkdir -p "$MEM" "$ENTITY/wiki"
printf '# decisions\n' > "$ENTITY/wiki/ceo-decisions.md"
git -C "$ENTITY" add -A >/dev/null 2>&1; git -C "$ENTITY" commit -qm decisions >/dev/null 2>&1
js() { python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$1"; }

TR_MEM_BASH="$SANDBOX/mem-bash.jsonl"
mk_tr "$TR_MEM_BASH" "[{\"prompt\":\"go\"},{\"bash\":$(js "cd $MEM && cat > feedback_mobile_testing_through_the_cli.md <<'EOF'
All mobile testing goes through the command line.
EOF")}]"
TR_MEM_WRITE="$SANDBOX/mem-write.jsonl"
mk_tr "$TR_MEM_WRITE" "[{\"prompt\":\"go\"},{\"write\":$(js "$MEM/project_mobile_version_and_builds.md")}]"
TR_REPO_COMMIT="$SANDBOX/repo-commit.jsonl"
mk_tr "$TR_REPO_COMMIT" "[{\"prompt\":\"go\"},{\"bash\":$(js "cd $ENTITY && cat >> wiki/ceo-decisions.md <<'EOF'
## 100. Phones
EOF")},{\"bashout\":[$(js "cd $ENTITY && git add wiki/ceo-decisions.md && git commit -m 'decisions 100'"),\"[main cf7e16b7] decisions 100\"]}]"
TR_REPO_EDIT="$SANDBOX/repo-edit.jsonl"
mk_tr "$TR_REPO_EDIT" "[{\"prompt\":\"go\"},{\"edit\":$(js "$ENTITY/wiki/ceo-decisions.md")}]"
TR_ESC="$SANDBOX/esc.jsonl"
mk_tr "$TR_ESC" "[{\"prompt\":\"go\"},{\"bashout\":[\"/x/engine/scripts/escalate.sh ack esc-20261002T085838Z-029b1749 --disposition 'judged on the median'\",\"acknowledged esc-20261002T085838Z-029b1749\"]}]"

# ARM 3 is an ALERT, never a refusal (CEO 2026-10-02: "check whether or not a
# 'recorded' claim was followed by the appropriate action and then issue an
# alert if it wasn't"). An alert is exit 0 and one JSON line whose
# additionalContext, named for Stop, reaches the lead only; it carries no
# systemMessage, so nothing is shown to the CEO.
alerted() { # <needle in the alert>
    [ "$RC" -eq 0 ] && python3 -c 'import json,sys
d = json.loads(sys.stdin.readline() or "{}")
h = d.get("hookSpecificOutput") or {}
c = h.get("additionalContext", "")
ok = h.get("hookEventName") == "Stop" and c.startswith("RECORD ALERT") and sys.argv[1] in c and "systemMessage" not in d
sys.exit(0 if ok else 1)' "$1" <<<"$OUT"
}
silent() { # exit 0 and no alert (the alert has no stderr channel; stderr is not read)
    if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q 'RECORD ALERT'; then return 0; fi
    return 1
}

R1="Three changes are in.

- **Recorded as a standing rule:** every phone-testing instruction names the command to use.
- The old scripts become thin wrappers around that command."
run_hook "$TR_MEM_BASH" "$R1"
if alerted 'Recorded as a standing rule: every phone-testing instruction' && alerted 'only a private memory note'; then
    ok "R1. 2026-10-02: a bulleted 'Recorded as a standing rule' whose turn wrote only memory is ALERTED: the claim is named, the turn is not blocked, nothing goes to the CEO"
else bad "R1. recorded as a standing rule" "rc=$RC out=$(printf '%s' "$OUT" | head -c 500) err=$ERR"; fi

run_hook "$TR_MEM_WRITE" "Understood. The first phone version is 1.0.0. I've also saved your ruling so it isn't lost."
alerted 'saved your ruling' && ok "R2. 2026-09-26: 'I've also saved your ruling' with a memory-only Write is ALERTED" || bad "R2. saved your ruling" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "The results are in. Recorded as the first job for the next session."
alerted 'first job' && ok "R3. 2026-09-28: 'Recorded as the first job for the next session' with a memory-only Write is ALERTED" || bad "R3. first job" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "**First question:** it is written in the three places I read: the rules file, your decisions page, and my memory."
silent && ok "R4. a citation of an existing record ('it is written in the three places I read') is not an act of this turn: SILENT" || bad "R4. written in three places" "rc=$RC out=$OUT err=$ERR"

run_hook "$TR_MEM_WRITE" "I've saved this as a rule in my notes."
alerted 'saved this as a rule in my notes' && ok "R5. 2026-09-26: 'saved this as a rule in my notes' (memory only, not 'private notes') is ALERTED" || bad "R5. in my notes" "rc=$RC out=$OUT"

TR_MIXED="$SANDBOX/mixed.jsonl"
mk_tr "$TR_MIXED" "[{\"prompt\":\"go\"},{\"write\":$(js "$MEM/feedback_interrupted_work_is_shelved_by_hand.md")},{\"bash\":$(js "cd $ENTITY && cat >> wiki/ceo-decisions.md <<'EOF'
row closed
EOF")},{\"bashout\":[$(js "cd $ENTITY && git commit -am 'close row'"),\"[main cf7e16b7] close row\"]}]"
run_hook "$TR_MIXED" "The row is closed in \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`.

I've saved this as a rule in my notes."
alerted 'saved this as a rule in my notes' && ok "R5b. the real 2026-09-26 turn: a commit cited for something else does not back 'saved this as a rule in my notes'" || bad "R5b. notes with an unrelated commit" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "Recorded."
alerted 'wrote NOTHING' && ok "R6. a bare 'Recorded.' in a turn that wrote nothing is ALERTED, and the alert says nothing was written" || bad "R6. bare recorded" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "Recorded."
alerted 'only a private memory note' && ok "R7. a memory-only write with an unqualified 'Recorded.' is ALERTED" || bad "R7. memory unqualified" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "Saved in my private notes: phone testing goes through the command line."
alerted 'follow-ups.md' && ok "R8. (CEO 2026-10-21, was SILENT) a memory-only write described as 'in my private notes' is ALERTED" || bad "R8. private notes" "rc=$RC out=$OUT"

# 2026-10-02 ~23:13Z: "private notes" turned the alert off for a RULE. A rule is
# a line in CLAUDE.md or the decisions record, never a memory note.
run_hook "$TR_MEM_WRITE" "I've saved the rule in my private notes."
alerted 'CLAUDE.md' && ok "R8b. 2026-10-02: 'saved the rule in my private notes' with a memory-only write is ALERTED (a private note is not a rule)" || bad "R8b. rule in private notes" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "Recorded as a standing rule in \`wiki/ceo-decisions.md\` §100, commit \`cf7e16b7\`."
silent && ok "R9. TRUE REPLY: written to a repository file, committed, path and the turn's own SHA named: SILENT" || bad "R9. true positive" "rc=$RC out=$OUT"

run_hook "$TR_REPO_EDIT" "Recorded in \`wiki/ceo-decisions.md\` §100."
silent && ok "R10. a repository file written and named but not committed: a write exists, so SILENT (no SHA recital)" || bad "R10. no commit" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "Recorded as a standing rule."
silent && ok "R11. written and committed, the reply names no place: a write exists, so SILENT" || bad "R11. no where" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "Recorded in \`wiki/ceo-decisions.md\`, commit \`abc1234\`."
silent && ok "R12. a typed SHA the turn never produced does not matter: a write exists, SILENT" || bad "R12. typed sha" "rc=$RC out=$OUT"

run_hook "$TR_MEM_BASH" "$R1" true
alerted 'Recorded as a standing rule' && ok "R13. the re-fire (stop_hook_active) carries the final reply and is ALERTED too, exit 0" || bad "R13. refire" "rc=$RC out=$OUT"

run_hook "$TR_MEM_BASH" "That claim was wrong. It has not been recorded anywhere yet; I will write it to the rules file next." true
silent && ok "R14. ...and the re-fire with the claim removed and the truth said is SILENT" || bad "R14. fixed refire" "rc=$RC out=$OUT"

run_hook "$TR_MEM_BASH" "$R1

stop-declared: nothing-unblocked — everything else this turn is genuinely finished and nothing remains to start."
alerted 'Recorded as a standing rule' && ok "R15. NO ESCAPE LINE: a stop declaration does not excuse a false record claim" || bad "R15. no escape" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "Nothing was recorded before you asked."
silent && ok "R16. a denial ('Nothing was recorded') is not a claim" || bad "R16. denial" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "When I wrote \"recorded as a standing rule\", that was wrong."
silent && ok "R17. the claim inside quotes, reported, is not a claim" || bad "R17. quoted" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "Andy recorded the cold start on the phone, and the video is in his report."
silent && ok "R18. a teammate's screen recording is not a record claim" || bad "R18. media" "rc=$RC out=$OUT"

run_hook "$TR_ESC" "Recorded in the escalation ledger as esc-20261002T085838Z-029b1749."
silent && ok "R19. a ledger written by its engine command, named with its entry id: SILENT" || bad "R19. ledger" "rc=$RC out=$OUT"

TR_MEM_VAR="$SANDBOX/mem-var.jsonl"
mk_tr "$TR_MEM_VAR" "[{\"prompt\":\"go\"},{\"bash\":$(js "M=$MEM
cat > \"\$M/feedback_split_failures_obsolete_vs_broken.md\" <<'EOF'
Report failures split obsolete versus broken.
EOF")}]"
run_hook "$TR_MEM_VAR" "Recorded so it doesn't need saying twice."
alerted 'only a private memory note' && ok "R21. 2026-09-08: a memory write through a shell variable is seen as a private note, and 'Recorded so it doesn't need saying twice' is ALERTED" || bad "R21. variable target" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "No, and you found the hole in the very thing I recorded an hour ago."
silent && ok "R22. a reference to a record from an earlier turn ('I recorded an hour ago') is not judged" || bad "R22. past reference" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "He nearly wrote five notes off bad captures, caught it, and recorded the anomaly as unresolved."
silent && ok "R23. a teammate's act ('He ..., and recorded the anomaly') is not the lead's claim" || bad "R23. third person" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "Recorded numbers: the best cold start so far, per phone type."
silent && ok "R24. 'Recorded numbers:' is a noun phrase, not a claim" || bad "R24. noun" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "Noted that the path implies rounds; I will number the next batches the same way."
silent && ok "R25. 'Noted that ...' is an acknowledgment, not a record claim" || bad "R25. noted that" "rc=$RC out=$OUT"

TR_WS="$SANDBOX/ws.jsonl"
mk_tr "$TR_WS" "[{\"prompt\":\"go\"},{\"bash\":\"/x/engine/scripts/workspaces.sh wait quint-opus-rewalk4 --started 'merge running'\"}]"
run_hook "$TR_WS" "I've recorded Quint's workspace (quint-opus-rewalk4) as merging: his record is already merged."
silent && ok "R26. a workspace-registry entry written by its engine command: SILENT" || bad "R26. registry" "rc=$RC out=$OUT"

TR_ECS="$SANDBOX/ecs.jsonl"
mk_tr "$TR_ECS" "[{\"prompt\":\"go\"},{\"bash\":\"scripts/ecs/ecs checkpoint-template --session s1 --write\"}]"
run_hook "$TR_ECS" "Recorded as a standing commitment, not a note."
alerted 'only a private memory note' && ok "R27. a claim backed only by an ECS checkpoint (private shadow state) is ALERTED" || bad "R27. ecs only" "rc=$RC out=$OUT"

run_hook "$TR_ECS" "I've saved the missing end-of-turn record."
silent && ok "R28. an ECS checkpoint named as what it is ('end-of-turn record'): SILENT" || bad "R28. ecs named" "rc=$RC out=$OUT"

# CEO 2026-10-21: "noted" as a promise and "private (follow-up) notes" are record
# claims too; only a committed repository file the reply names backs them.
run_hook "$TR_ECS" "Noted. The numbering goes in right after the next nightly."
alerted 'follow-ups.md' && ok "R34. 'Noted.' promising later work, only an ECS write: ALERTED, names the follow-up list" || bad "R34. noted ecs" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "It's in my private notes."
alerted 'follow-ups.md' && ok "R35. 'it's in my private notes', memory-directory write only: ALERTED" || bad "R35. private notes memory" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "It's in my private follow-up notes, not filed in a repository."
alerted 'follow-ups.md' && ok "R36. the 2026-10-21 slip 'my private follow-up notes', memory write only: ALERTED" || bad "R36. private follow-up notes" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "Noted. The numbering goes in after the next nightly: it is in \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`."
silent && ok "R37. 'Noted.' promise with a committed repository file named and its commit: SILENT" || bad "R37. noted backed" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "It's in my private follow-up notes: \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`."
silent && ok "R38. 'in my private follow-up notes' with the committed file and commit named: SILENT" || bad "R38. notes backed" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "I've saved it in my private notes."
alerted 'follow-ups.md' && ok "R40. review 1: 'I've saved it in my private notes', memory write only: ALERTED" || bad "R40. saved private" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "Saved in my private follow-up notes: number the next nightly."
alerted 'follow-ups.md' && ok "R41. review 1: 'private follow-up notes' with a 'Saved' lead, memory write only: ALERTED" || bad "R41. saved private followup" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "Noted in my private notes for the next nightly."
alerted 'follow-ups.md' && ok "R42. review 1: 'Noted in my private notes', memory write only: ALERTED" || bad "R42. noted private" "rc=$RC out=$OUT"

run_hook "$TR_ECS" "Noted: I'll add the numbering after the next nightly."
alerted 'follow-ups.md' && ok "R43. review 2: 'Noted: I'll ...' promise, ECS write only: ALERTED" || bad "R43. noted colon" "rc=$RC out=$OUT"

run_hook "$TR_ECS" "Noted for the next nightly."
alerted 'follow-ups.md' && ok "R44. review 2: 'Noted for the next nightly.', ECS write only: ALERTED" || bad "R44. noted for" "rc=$RC out=$OUT"

TR_EDIT_OLDSHA="$SANDBOX/edit-oldsha.jsonl"
mk_tr "$TR_EDIT_OLDSHA" "[{\"prompt\":\"go\"},{\"edit\":$(js "$ENTITY/wiki/ceo-decisions.md")},{\"bashout\":[$(js "git log -1 --oneline"),\"cf7e16b7 an older commit\"]}]"
run_hook "$TR_EDIT_OLDSHA" "Noted. The numbering goes in after the next nightly: \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`."
alerted 'follow-ups.md' && ok "R45. review 3: the named file was edited, never committed, and an OLDER commit is cited: ALERTED" || bad "R45. uncommitted edit" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "The ruling is in \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`.

Noted. The numbering goes in after the next nightly."
alerted 'follow-ups.md' && ok "R46. review 4: an unrelated paragraph's file and commit do not back the 'Noted.' promise: ALERTED" || bad "R46. unrelated commit" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "Noted.
The numbering goes in after the next nightly: \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`."
silent && ok "R47. the file and commit sit in the claim's own paragraph (next line): SILENT" || bad "R47. same paragraph" "rc=$RC out=$OUT"

TR_ADD_U="$SANDBOX/add-u.jsonl"
mk_tr "$TR_ADD_U" "[{\"prompt\":\"go\"},{\"bash\":$(js "cd $ENTITY && echo x >> wiki/ceo-decisions.md")},{\"bashout\":[$(js "cd $ENTITY && git add -u && git commit -m 'decisions 101'"),\"[main cf7e16b7] decisions 101\"]}]"
run_hook "$TR_ADD_U" "Noted. The numbering goes in after the next nightly: \`wiki/ceo-decisions.md\`, commit \`cf7e16b7\`."
silent && ok "R48. review 5: a tracked note file committed after 'git add -u': SILENT" || bad "R48. add -u" "rc=$RC out=$OUT"

# CEO 2026-10-23: "So, it wasn't on your fucking follow-up list?" A reply that
# puts an item on the follow-up list is backed only by a committed change to it.
FUP_REPO="$SANDBOX/richos-hq"
mkdir -p "$FUP_REPO/docs/plans"
git -C "$FUP_REPO" init -q -b main . >/dev/null 2>&1
git -C "$FUP_REPO" config user.email t@t.t >/dev/null 2>&1
git -C "$FUP_REPO" config user.name t >/dev/null 2>&1
git -C "$FUP_REPO" config core.hooksPath "$SANDBOX/nohooks" >/dev/null 2>&1
printf '# follow-ups\n' > "$FUP_REPO/docs/plans/follow-ups.md"
git -C "$FUP_REPO" add -A >/dev/null 2>&1; git -C "$FUP_REPO" commit -qm seed >/dev/null 2>&1
TR_FUP_COMMIT="$SANDBOX/fup-commit.jsonl"
mk_tr "$TR_FUP_COMMIT" "[{\"prompt\":\"go\"},{\"bash\":$(js "cd $FUP_REPO && cat >> docs/plans/follow-ups.md <<'EOF'
- number the next nightly
EOF")},{\"bashout\":[$(js "cd $FUP_REPO && git add docs/plans/follow-ups.md && git commit -m 'follow-up'"),\"[main 4b1d9e2a] follow-up\"]}]"

run_hook "$TR_MEM_WRITE" "That's on the follow-up list for after the nightly"
alerted 'follow-ups.md' && ok "R49. 2026-10-23: 'That's on the follow-up list for after the nightly', memory write only: ALERTED" || bad "R49. on the follow-up list" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "I've added it to the follow-ups."
alerted 'follow-ups.md' && ok "R50. 'added to the follow-ups' in a turn that wrote nothing: ALERTED" || bad "R50. added to the follow-ups" "rc=$RC out=$OUT"

run_hook "$TR_REPO_COMMIT" "I've added it to the follow-ups."
alerted 'follow-ups.md' && ok "R51. 'added to the follow-ups' backed only by a commit to ANOTHER file: ALERTED" || bad "R51. other file" "rc=$RC out=$OUT"

run_hook "$TR_FUP_COMMIT" "That's on the follow-up list for after the nightly"
silent && ok "R52. 'on the follow-up list' with a committed change to the follow-up list in this turn: SILENT" || bad "R52. list committed" "rc=$RC out=$OUT"

# AN EXISTING RECORD IS LOOKED UP, NOT RE-DEMANDED (orchestration reliability R6/A7). On
# 2026-10-23 a reply citing a follow-up committed in an EARLIER turn (path and SHA in code
# spans) was alerted twice as "in no committed repository file". The shapes below are those
# replies' shapes (the verbatim replies are replayed in the private evidence): a paragraph, a
# list item, and the clarification. The cited commit is resolved in the repositories the
# workspace registry knows (repos.json), never in this turn's traffic. Observable facts only:
# no wording is compared, so a cited row is never called verified (A7d states that limit).
printf -- '- After the release: a land refuses a branch whose author run was made in another workspace on the exact same commit, so the same check re-ran. A passed run of the identical commit should count wherever it was made.\n' >> "$FUP_REPO/docs/plans/follow-ups.md"
git -C "$FUP_REPO" commit -qam "Follow-ups: a passed author run of the identical commit should count from any workspace" >/dev/null 2>&1
FUP_SHA="$(git -C "$FUP_REPO" rev-parse --short=9 HEAD)"
printf 'other\n' > "$FUP_REPO/other.md"; git -C "$FUP_REPO" add other.md >/dev/null 2>&1
git -C "$FUP_REPO" commit -qm "an unrelated file" >/dev/null 2>&1
OTHER_SHA="$(git -C "$FUP_REPO" rev-parse --short=9 HEAD)"
printf -- '- Investigate release speed measurements on Windows.\n' >> "$FUP_REPO/docs/plans/follow-ups.md"
git -C "$FUP_REPO" commit -qam "Follow-ups: investigate the Windows release speed measurements" >/dev/null 2>&1
INVESTIGATE_SHA="$(git -C "$FUP_REPO" rev-parse --short=9 HEAD)"
printf -- '- A row that was later removed.\n' >> "$FUP_REPO/docs/plans/follow-ups.md"
git -C "$FUP_REPO" commit -qam "Follow-ups: a row" >/dev/null 2>&1
GONE_SHA="$(git -C "$FUP_REPO" rev-parse --short=9 HEAD)"
grep -v 'A row that was later removed' "$FUP_REPO/docs/plans/follow-ups.md" > "$SANDBOX/fup.tmp"
cp "$SANDBOX/fup.tmp" "$FUP_REPO/docs/plans/follow-ups.md"
git -C "$FUP_REPO" commit -qam "Follow-ups: done" >/dev/null 2>&1
mkdir -p "$SANDBOX/registry"
printf '{"repos": ["%s"]}\n' "$FUP_REPO" > "$SANDBOX/registry/repos.json"
export RICHOS_WORKSPACES_DIR="$SANDBOX/registry"
no_demand() { ! printf '%s' "$OUT" | grep -q 'write it to the record now'; }

run_hook "$TR_BASH" "**Fix for the notes refusal:** a passed test run of the exact same commit should count no matter which folder it ran in. That's on the follow-up list in the private repo (\`docs/plans/follow-ups.md\`, commit \`$FUP_SHA\`), to brief once the release is out."
silent && ok "A7a. a follow-up committed in an EARLIER turn, cited with its path and SHA in code spans: SILENT (looked up)" || bad "A7a. earlier record cited" "rc=$RC out=$OUT"
run_hook "$TR_BASH" "Two fixes:
- **Same-commit test result counts from any folder:** this removes that re-run entirely. It's on the follow-up list (\`docs/plans/follow-ups.md\`, commit \`$FUP_SHA\`), to brief once the release is out.
- **Release notes land without a review:** finishing its tests now."
silent && ok "A7b. the same record cited from a list item: SILENT" || bad "A7b. list item" "rc=$RC out=$OUT"
run_hook "$TR_BASH" "That follow-up is committed in the private repo at docs/plans/follow-ups.md, commit $FUP_SHA, pushed in an earlier turn. Nothing new needed recording."
silent && ok "A7c. the truthful clarification passes with no further write, commit or wording" || bad "A7c. clarification" "rc=$RC out=$OUT"
# THE STATED LIMIT (Codex 2026-10-24): the cited commit added "Investigate ..." and the reply says
# "Delete ...". Only observable facts are checked, so this is CITED like A7a: no alert, no write
# demand, and nothing calls it verified. Catching it would take comparing wording, which is
# deliberately not done here.
run_hook "$TR_BASH" "Delete all release speed measurements on Windows. It is on the follow-up list (\`docs/plans/follow-ups.md\`, commit \`$INVESTIGATE_SHA\`)."
A7D_LOGGED="$(tail -1 "$ENTITY/.claude/state/stated-actions.jsonl" 2>/dev/null | python3 -c 'import json,sys
a = json.loads(sys.stdin.readline() or "{}").get("arm3") or {}
print("%s|%s" % (a.get("verdict"), ",".join(a.get("cited") or [])))' 2>/dev/null)"
{ silent && no_demand && [ "$A7D_LOGGED" = "cited, content not checked|$INVESTIGATE_SHA" ]; } \
    && ok "A7d. LIMIT: a cited row whose words say the opposite is logged 'cited, content not checked' with its commit, never verified; no alert, no write demand" \
    || bad "A7d. opposite action" "rc=$RC out=$OUT logged=$A7D_LOGGED"
run_hook "$TR_BASH" "It's on the follow-up list (\`docs/plans/follow-ups.md\`, commit \`$OTHER_SHA\`), to brief later."
{ alerted 'NOT VERIFIED' && alerted 'not a finding that no record exists' && no_demand; } \
    && ok "A7e. a cited commit that does not change the follow-up list is NOT VERIFIED, never 'no record', with no write demand" \
    || bad "A7e. cited commit elsewhere" "rc=$RC out=$OUT"
run_hook "$TR_BASH" "It's on the follow-up list (\`docs/plans/follow-ups.md\`, commit \`$GONE_SHA\`)."
{ alerted 'NOT VERIFIED' && alerted 'no line it added is still there' && no_demand; } \
    && ok "A7f. a cited commit whose added row is gone from the list is NOT VERIFIED" || bad "A7f. row gone" "rc=$RC out=$OUT"
run_hook "$TR_BASH" "That's on the follow-up list for after the release."
alerted 'write it to the record now' && ok "A7g. a genuinely missing record, nothing cited and nothing written: the alert as before" \
    || bad "A7g. missing record" "rc=$RC out=$OUT"
export RICHOS_WORKSPACES_DIR="$SANDBOX/registry-none"

# A negation, modal or subordinate word in an EARLIER clause does not silence
# the claim: only the claim's own clause is checked.
run_hook "$TR_BASH" "It isn't in tonight's release, but it's on the follow-up list for after the nightly."
alerted 'follow-ups.md' && ok "R53. 'isn't' in an earlier clause, then 'on the follow-up list': ALERTED" || bad "R53. earlier negation" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "The fix will ship later; I've added it to the follow-ups."
alerted 'follow-ups.md' && ok "R54. 'will' in an earlier clause, then 'added to the follow-ups': ALERTED" || bad "R54. earlier modal" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "After checking the release, I've added it to the follow-ups."
alerted 'follow-ups.md' && ok "R55. a completed 'After checking X,' preface, then 'added to the follow-ups': ALERTED" || bad "R55. completed preface" "rc=$RC out=$OUT"

run_hook "$TR_MEM_WRITE" "Noted that the build is green."
silent && ok "R39. 'Noted that the build is green.' stays an acknowledgment: SILENT" || bad "R39. noted that" "rc=$RC out=$OUT"

# The misread classes the corpus replay found: none is a claim of THIS turn.
run_hook "$TR_BASH" "I recorded it wrong. The version is 1.0.0, not 0.1.0."
silent && ok "R30. a past CONFESSION ('I recorded it wrong.') is not a claim" || bad "R30. confession" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "I saved their work before the reset; the branch is committed."
silent && ok "R31. a TEAMMATE's version-control 'saved' ('saved their work') is not a claim" || bad "R31. saved their work" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "Documented, never fixed. The tool still has the bug."
silent && ok "R32. a DESCRIPTION of an existing document ('Documented, never fixed.') is not a claim" || bad "R32. documented never fixed" "rc=$RC out=$OUT"

run_hook "$TR_BASH" "It was recorded, but never enforced."
silent && ok "R33. 'It was recorded, but never enforced' describes an old record and is not a claim" || bad "R33. recorded but never" "rc=$RC out=$OUT"

# An alert is never a refusal, whatever the switch says.
printf 'STATED_ACTIONS_ENFORCE=0\n' > "$ENTITY/orchestration.config"
run_hook "$TR_MEM_BASH" "$R1"
alerted 'Recorded as a standing rule' && ok "R20. STATED_ACTIONS_ENFORCE=0: the alert is still issued and the turn ends" || bad "R20. report-only" "rc=$RC out=$OUT"
: > "$ENTITY/orchestration.config"

# The corpus replay tool (ass-kicker/tests/record-claims.replay.py), run on a
# one-session fixture: a memory-only write, a claim, then a turn that commits
# a named record. Counts on stdout, verdicts in the private --out file.
REPLAY_DIR="$SANDBOX/replay-corpus"; mkdir -p "$REPLAY_DIR"
python3 - "$REPLAY_DIR/s1.jsonl" "$MEM" "$ENTITY" <<'PY'
import json, sys
out, mem, ent = sys.argv[1:4]
rows = []
def turn(pid, cmd, result, text):
    rows.append({"type": "user", "promptId": pid, "cwd": ent, "message": {"content": "go"}})
    rows.append({"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": "t-" + pid, "name": "Bash", "input": {"command": cmd}}]}})
    rows.append({"type": "user", "promptId": pid, "cwd": ent, "message": {"content": [
        {"type": "tool_result", "tool_use_id": "t-" + pid, "content": result}]}})
    rows.append({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}})
turn("p1", "cd %s && cat > rule.md <<'EOF'\nx\nEOF" % mem, "ok", "Recorded as a standing rule.")
turn("p2", "cd %s && echo y >> wiki/ceo-decisions.md && git commit -am r" % ent, "[main 9a8b7c6] r",
     "Recorded in `wiki/ceo-decisions.md`, commit `9a8b7c6`.")
open(out, "w").write("".join(json.dumps(r) + "\n" for r in rows))
PY
REPLAY_SUM="$(python3 "$ENGINE_ROOT/ass-kicker/tests/record-claims.replay.py" "$REPLAY_DIR" --out "$SANDBOX/replay-out.jsonl" 2>/dev/null)"
if printf '%s' "$REPLAY_SUM" | grep -q '"claim_turns": 2' && printf '%s' "$REPLAY_SUM" | grep -q '"alerted": 1' \
   && printf '%s' "$REPLAY_SUM" | grep -q '"backed": 1' && grep -q '"private"' "$SANDBOX/replay-out.jsonl"; then
    ok "R29. the corpus replay tool counts a refused memory-only claim and a backed committed one, verdicts to --out only"
else bad "R29. replay tool" "sum=$REPLAY_SUM"; fi

# ---------------------------------------------------------------------------
# SAFETY
# ---------------------------------------------------------------------------
run_hook "$TR_BASH" "$ZACH" true
[ "$RC" -eq 0 ] && ok "r.  stop_hook_active=true stands the gate down — it refuses a turn at most once" || bad "r.  re-fire" "rc=$RC"

run_unrooted "$TR_BASH" "$ZACH" "$UNADOPTED"
[ "$RC" -eq 0 ] && [ -z "$OUT" ] && ok "s.  a repository that never adopted the engine: exit 0, silent" || bad "s.  not adopted" "rc=$RC out=$OUT"

printf 'CHECK_STATED_ACTIONS=0\n' > "$ENTITY/orchestration.config"
run_hook "$TR_BASH" "$ZACH"
if [ "$RC" -eq 0 ] && has_sysmsg "STOOD DOWN"; then ok "t.  CHECK_STATED_ACTIONS=0: exit 0, and the operator is TOLD"
else bad "t.  stood down" "rc=$RC out=$(printf '%s' "$OUT" | head -c 200)"; fi
: > "$ENTITY/orchestration.config"

CASE_N=$((CASE_N + 1))
printf 'this is not json' | RICHOS_ENTITY_ROOT="$ENTITY" bash "$HOOK" >/dev/null 2>&1; RC=$?
[ "$RC" -eq 0 ] && ok "u.  an unparseable payload never wedges turn-end" || bad "u.  garbage" "rc=$RC"

run_hook "$TR_BASH" "$ZACH" false "$ENTITY" "$HOOK" none
[ "$RC" -eq 0 ] && ok "v.  no prompt_id: the turn cannot be scoped, so nothing is refused" || bad "v.  unscoped" "rc=$RC"

run_hook "-" "$ZACH"
[ "$RC" -eq 0 ] && ok "w.  a missing transcript: exit 0" || bad "w.  missing transcript" "rc=$RC"

# A mirror of the engine subtree, so the broken-install paths are reached for
# the right reason (see stop-hook-visibility.test.sh on why a bare temp dir
# proves nothing).
MIRROR="$SANDBOX/mirror"
mkdir -p "$MIRROR/scripts/hooks" "$MIRROR/scripts/lib" "$MIRROR/ass-kicker"
cp "$ENGINE_ROOT/ass-kicker/ass-kicker.py" "$MIRROR/ass-kicker/"
cp "$SCRIPT_DIR/ass-kicker.sh" \
   "$SCRIPT_DIR/turn-manifest.py" "$SCRIPT_DIR/guard-idle-land.py" "$MIRROR/scripts/hooks/"
cp "$SCRIPT_DIR/../lib/resolve-roots.sh" "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" \
   "$SCRIPT_DIR/../lib/stop-hook-notice.sh" "$MIRROR/scripts/lib/"
BROKEN="$SANDBOX/broken"
cp -R "$MIRROR" "$BROKEN"
rm -f "$BROKEN/scripts/lib/resolve-roots.sh"
run_hook "$TR_BASH" "$ZACH" false "$ENTITY" "$BROKEN/scripts/hooks/ass-kicker.sh"
if [ "$RC" -eq 0 ] && printf '%s' "$ERR" | grep -q 'BROKEN INSTALL'; then ok "x.  broken install (resolve-roots.sh absent): exit 0 + banner"
else bad "x.  broken install" "rc=$RC err=$(printf '%s' "$ERR" | head -c 200)"; fi

printf 'STATED_ACTIONS_ENFORCE=0\n' > "$ENTITY/orchestration.config"
run_hook "$TR_BASH" "$ZACH"
if [ "$RC" -eq 0 ] && printf '%s' "$ERR" | grep -q 'TURN BLOCKED'; then ok "y.  STATED_ACTIONS_ENFORCE=0: the refusal is printed and the turn ends"
else bad "y.  report-only" "rc=$RC err=$(printf '%s' "$ERR" | head -c 200)"; fi
: > "$ENTITY/orchestration.config"

NODEP="$SANDBOX/nodep"
cp -R "$MIRROR" "$NODEP"
rm -f "$NODEP/scripts/hooks/guard-idle-land.py"
run_hook "$TR_FIN" "$FIN_REPORT" false "$ENTITY" "$NODEP/scripts/hooks/ass-kicker.sh"
if [ "$RC" -eq 0 ] && has_sysmsg "NOT RUNNING"; then ok "y2. guard-idle-land.py absent: the gate does not guess, exits 0, and the operator is TOLD"
else bad "y2. dependency absent" "rc=$RC out=$(printf '%s' "$OUT" | head -c 200)"; fi

# One session through failed, still failed, recovered and still healthy states.
notice_transition() {
    local message="$1"
    python3 - "$ENTITY" "$TR_BASH" "$PROMPT_ID" "$message" > "$SANDBOX/notice-payload.json" <<'PYNOTICE'
import json, sys
entity, transcript, prompt, message = sys.argv[1:]
print(json.dumps({"cwd": entity, "session_id": "feedface-8888-4000-8000-000000000000",
    "prompt_id": prompt, "transcript_path": transcript, "hook_event_name": "Stop",
    "last_assistant_message": json.loads(message)}))
PYNOTICE
    RICHOS_ENTITY_ROOT="$ENTITY" bash "$HOOK" < "$SANDBOX/notice-payload.json" > "$SANDBOX/notice-out" 2> "$SANDBOX/notice-err"
    RC=$?; OUT="$(cat "$SANDBOX/notice-out")"; ERR="$(cat "$SANDBOX/notice-err")"
}
notice_transition '123'
if [ "$RC" -eq 0 ] && has_sysmsg "NOT RUNNING" && has_sysmsg "exited 1" && ! has_sysmsg "RUNNING AGAIN"; then
    ok "y3. analyzer crash fails open and tells the operator that no verdict was obtained"
else bad "y3. analyzer crash notice" "rc=$RC out=$OUT err=$ERR"; fi
notice_transition '123'
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "y4. repeated analyzer failure stays abnormal without repeating the same notice"
else bad "y4. repeated failure" "rc=$RC out=$OUT"; fi
notice_transition '"Done."'
if [ "$RC" -eq 0 ] && has_sysmsg "RUNNING AGAIN"; then
    ok "y5. successful analysis after a crash reports actual recovery"
else bad "y5. recovery" "rc=$RC out=$OUT err=$ERR"; fi
notice_transition '"Done."'
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "y6. another successful analysis does not repeat recovery"
else bad "y6. repeated recovery" "rc=$RC out=$OUT"; fi

# ---------------------------------------------------------------------------
# RECORD
# ---------------------------------------------------------------------------
REC="$ENTITY/.claude/state/stated-actions.jsonl"
if [ -f "$REC" ] && grep -q '"verdict": "block"' "$REC" && grep -q '"undeclared-stop"' "$REC"; then
    ok "z.  an observation record is appended per evaluated turn, with the verdict and the arm"
else bad "z.  record" "missing or incomplete: $REC"; fi

echo ""
echo "=== summary: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
