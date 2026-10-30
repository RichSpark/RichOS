#!/usr/bin/env bash
#
# agent-state-claims.test.sh — the suite for the liveness resolver
# (scripts/lib/agent-liveness.{py,sh}), its operator CLI
# (scripts/agent-liveness.sh) and the Stop-time claim guard
# (scripts/hooks/guard-agent-state-claims.{sh,py}).
#
# EVERY CASE HERE WAS RUN RED FIRST. Where "red first" means something other
# than "I deleted the assertion", the mutation is performed BY THE SUITE and is
# named in the case: sections R and M mutate the shipped source in a mirror and
# assert the corresponding case flips. A green suite that has never been shown
# to go red is a suite that is measuring nothing.
#
# WHAT IS COVERED
#   A. the resolver's five verdicts, on real git worktrees with real pids
#   B. INDETERMINATE is a real outcome and is never collapsed
#   C. (removed 2026-09-11: it proved remove-agent-worktree.sh agreed with the
#      library; that remover is deleted, docs/plans/worktree-spec-2026-09-11.md)
#   D. name -> agent id joins exactly, from a transcript, and refuses to guess
#   E. the extractor fires on the three measured constructions and on nothing else
#   F. the guard NOTICES a claim contradicted by a held lock
#   G. the guard is SILENT when the claim is true, when the name is unmappable,
#      when the construction is conditional/negated/reported, and in an
#      unadopted directory
#   H. the guard NEVER blocks (exit 0 on every path)
#   R. the resolver mutations — each flips a case, by name
#   M. the extractor mutations — each flips a case, by name
#
# Run directly: scripts/hooks/agent-state-claims.test.sh
# Exit 0 = all pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-agent-state-claims.sh"
ANALYZER="$SCRIPT_DIR/guard-agent-state-claims.py"
LIB_PY="$ENGINE_ROOT/scripts/lib/agent-liveness.py"
LIB_SH="$ENGINE_ROOT/scripts/lib/agent-liveness.sh"
CLI="$ENGINE_ROOT/scripts/agent-liveness.sh"
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n     -> %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

SANDBOX="$(mktemp -d -t agent-state-claims.XXXXXX)"
trap 'rm -rf "$SANDBOX"; [ -n "${LIVE_PID:-}" ] && kill "$LIVE_PID" 2>/dev/null' EXIT
SANDBOX="$(cd "$SANDBOX" && pwd -P)"

SESSION_ID="feedface-0000-4000-8000-000000000000"

# --- the governed entity ---------------------------------------------------
ENTITY="$SANDBOX/entity"
mkdir -p "$ENTITY"
git -C "$ENTITY" init -q -b main . >/dev/null 2>&1
git -C "$ENTITY" config user.email t@t.t >/dev/null 2>&1
git -C "$ENTITY" config user.name t >/dev/null 2>&1
mkdir -p "$SANDBOX/nohooks"
git -C "$ENTITY" config core.hooksPath "$SANDBOX/nohooks" >/dev/null 2>&1
: > "$ENTITY/orchestration.config"
echo seed > "$ENTITY/seed.txt"
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm seed >/dev/null 2>&1

UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"

# Four agents, four states. The ids are shaped like real ones so the CLI's
# `agent-<id>` matching is exercised rather than bypassed.
ID_LIVE="a1111111111111111"
ID_STALE="a2222222222222222"
ID_UNLOCKED="a3333333333333333"
ID_ABSENT="a4444444444444444"

WT_LIVE="$ENTITY/.claude/worktrees/agent-$ID_LIVE"
WT_STALE="$ENTITY/.claude/worktrees/agent-$ID_STALE"
WT_UNLK="$ENTITY/.claude/worktrees/agent-$ID_UNLOCKED"

git -C "$ENTITY" worktree add -q -b wt-live "$WT_LIVE" >/dev/null 2>&1
git -C "$ENTITY" worktree add -q -b wt-stale "$WT_STALE" >/dev/null 2>&1
git -C "$ENTITY" worktree add -q -b wt-unlk "$WT_UNLK" >/dev/null 2>&1

# A REAL running process, so "pid is alive" is a fact about this machine rather
# than a fixture. 999999 is the dead pid, for the same reason in reverse.
sleep 300 & LIVE_PID=$!
git -C "$ENTITY" worktree lock \
    --reason "claude agent agent-$ID_LIVE (pid $LIVE_PID start now)" "$WT_LIVE" >/dev/null 2>&1
git -C "$ENTITY" worktree lock \
    --reason "claude agent agent-$ID_STALE (pid 999999 start old)" "$WT_STALE" >/dev/null 2>&1

# --- a transcript that carries the name -> id join -------------------------
# Exactly the two-record shape verified on a real 2026-08-31 transcript: an
# assistant tool_use named Agent carrying `name`, then a tool_result under the
# same tool_use_id whose toolUseResult carries `agentId`.
TRANSCRIPT="$SANDBOX/transcript.jsonl"
emit_spawn() { # <name> <agent-id> <tool-use-id>
    python3 - "$1" "$2" "$3" "$TRANSCRIPT" <<'PY'
import json, sys
name, aid, tuid, path = sys.argv[1:5]
a = {"type": "assistant", "message": {"role": "assistant", "content": [
     {"type": "tool_use", "id": tuid, "name": "Agent",
      "input": {"name": name, "subagent_type": name.split("-")[0]}}]}}
u = {"type": "user", "message": {"role": "user", "content": [
     {"type": "tool_result", "tool_use_id": tuid, "content": "launched"}]},
     "toolUseResult": {"isAsync": True, "status": "async_launched", "agentId": aid}}
with open(path, "a", encoding="utf-8") as f:
    f.write(json.dumps(a) + "\n")
    f.write(json.dumps(u) + "\n")
PY
}
: > "$TRANSCRIPT"
emit_spawn "zach-opus-g1" "$ID_LIVE"     "toolu_live"
emit_spawn "clark-opus-d1" "$ID_ABSENT"  "toolu_absent"
emit_spawn "mark-sonnet-p3" "$ID_STALE"  "toolu_stale"
emit_spawn "ace-sonnet-u9" "$ID_UNLOCKED" "toolu_unlk"

# --- payload builder -------------------------------------------------------
payload() { # <last_assistant_message> [stop_hook_active]
    python3 - "$1" "${2:-false}" "$TRANSCRIPT" "$SESSION_ID" <<'PY'
import json, sys
msg, active, tr, sid = sys.argv[1:5]
print(json.dumps({
    "hook_event_name": "Stop",
    "session_id": sid,
    "prompt_id": "11111111-2222-3333-4444-555555555555",
    "transcript_path": tr,
    "cwd": "/nonexistent",
    "stop_hook_active": active == "true",
    "last_assistant_message": msg,
}))
PY
}

run_hook() { # <message> -> stdout of the hook; sets RC
    local p
    p="$(payload "$1")"
    OUT="$(printf '%s' "$p" | RICHOS_ENTITY_ROOT="$ENTITY" bash "$HOOK" 2>/dev/null)"
    RC=$?
    # Every case runs in a fresh notice ledger, because the helper de-duplicates
    # on state change and a suite that shares one ledger measures the second
    # assertion against the first one's memory.
    rm -rf "$ENTITY/.claude/state/stop-hook-notices"
    printf '%s' "$OUT"
}

fired_on() { # <message> -> 0 if a systemMessage was emitted naming the guard
    local out
    out="$(run_hook "$1")"
    printf '%s' "$out" | grep -q '"systemMessage"' \
        && printf '%s' "$out" | grep -q 'AGENT-STATE CLAIM CONTRADICTED'
}

echo "=== agent-liveness + guard-agent-state-claims ==="
echo ""
echo "--- A. the resolver's verdicts, on real worktrees and real pids ---"

verdict() { python3 "$LIB_PY" --entity "$ENTITY" --owner "$1" --format triple | cut -f1; }

[ "$(verdict "$ID_LIVE")" = "ALIVE" ] \
    && ok "A1 locked worktree + LIVE pid -> ALIVE" \
    || bad "A1 locked + live pid -> ALIVE" "got $(verdict "$ID_LIVE")"
[ "$(verdict "$ID_STALE")" = "NOT-ALIVE" ] \
    && ok "A2 locked worktree + DEAD pid (stale lock) -> NOT-ALIVE" \
    || bad "A2 stale lock -> NOT-ALIVE" "got $(verdict "$ID_STALE")"
[ "$(verdict "$ID_UNLOCKED")" = "NOT-ALIVE" ] \
    && ok "A3 registered but UNLOCKED -> NOT-ALIVE" \
    || bad "A3 unlocked -> NOT-ALIVE" "got $(verdict "$ID_UNLOCKED")"
[ "$(verdict "$ID_ABSENT")" = "NOT-ALIVE" ] \
    && ok "A4 no registered worktree -> NOT-ALIVE" \
    || bad "A4 absent -> NOT-ALIVE" "got $(verdict "$ID_ABSENT")"

# A locked worktree whose lock line carries no pid at all.
# INVERTED 2026-09-10. This asserted NOT-ALIVE until three RUNNING agents on
# this machine were answered NOT-ALIVE through exactly this branch: their lock
# reasons had been emptied by an unlock/re-lock (`git worktree lock` with no
# --reason writes an empty file), and an empty lock names nobody. A HELD lock
# is the platform saying the workspace may still be in use; a resolver that
# reads it as death is permissive in the one direction that destroys work.
# INDETERMINATE is this module's own honest outcome for "I cannot tell", and
# the remover refuses on it.
WT_NOPID="$ENTITY/.claude/worktrees/agent-a5555555555555555"
git -C "$ENTITY" worktree add -q -b wt-nopid "$WT_NOPID" >/dev/null 2>&1
git -C "$ENTITY" worktree lock --reason "claude agent agent-a5555555555555555 (no pid here)" "$WT_NOPID" >/dev/null 2>&1
[ "$(verdict a5555555555555555)" = "INDETERMINATE" ] \
    && ok "A5 locked but the lock line carries NO pid -> INDETERMINATE (a held lock this cannot attribute is never read as death)" \
    || bad "A5 lock without pid -> INDETERMINATE" "got $(verdict a5555555555555555)"

# A5b the same worktree with an EMPTY lock reason, which is the exact artifact
# measured on this machine at 11:25 on 2026-09-10 for three live agents.
WT_EMPTY="$ENTITY/.claude/worktrees/agent-a7777777777777777"
git -C "$ENTITY" worktree add -q -b wt-empty "$WT_EMPTY" >/dev/null 2>&1
git -C "$ENTITY" worktree lock "$WT_EMPTY" >/dev/null 2>&1
[ "$(verdict a7777777777777777)" = "INDETERMINATE" ] \
    && ok "A5b locked with an EMPTY reason (the measured artifact) -> INDETERMINATE, not NOT-ALIVE" \
    || bad "A5b empty lock reason -> INDETERMINATE" "got $(verdict a7777777777777777)"

# A5c the positive control: the SAME shape with a live pid on it still reads
# ALIVE, so A5/A5b are about attribution and not about locks in general.
[ "$(verdict "$ID_LIVE")" = "ALIVE" ] \
    && ok "A5c a lock naming a RUNNING pid still reads ALIVE (the inversion above is about attribution, not about locks)" \
    || bad "A5c live lock -> ALIVE" "got $(verdict "$ID_LIVE")"

# Accepts a worktree PATH as well as an id.
[ "$(python3 "$LIB_PY" --entity "$ENTITY" --owner "$WT_LIVE" --format triple | cut -f1)" = "ALIVE" ] \
    && ok "A6 a worktree PATH resolves the same as its id" \
    || bad "A6 path form" "path form disagreed with id form"

# --- A7. THE LOCK PROVES THE SESSION, THE REGISTRY PROVES "FINISHED" ---------
# Round 8, item 6 (2026-09-13). The lock's pid is the SESSION's, shared by every
# agent of that session, so a terminal agent inside a living session read ALIVE
# from its session's lock (round 7 §10). The workspace registry
# (mega-lander/workspaces.py, finished_state — point 11 of the CEO's page) is
# what decides "finished", and a finished record decides NOT-ALIVE whatever the
# lock says. The registry here is a sandbox store; the record is built with the
# library's own helpers and carries the platform's end-of-run signal.
ID_FIN="a8888888888888888"
WT_FIN="$ENTITY/.claude/worktrees/agent-$ID_FIN"
git -C "$ENTITY" worktree add -q -b wt-fin "$WT_FIN" >/dev/null 2>&1
git -C "$ENTITY" worktree lock \
    --reason "claude agent agent-$ID_FIN (pid $LIVE_PID start now)" "$WT_FIN" >/dev/null 2>&1
export RICHOS_WORKSPACES_DIR="$SANDBOX/ws"
mkdir -p "$RICHOS_WORKSPACES_DIR"
registry_record() { # <agent-id> <name> finished|running
    python3 - "$ENGINE_ROOT/mega-lander/workspaces.py" "$1" "$2" "$3" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("ws", sys.argv[1])
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)
aid, name, state = sys.argv[2:5]
key = "feedface--" + name
rec = ws.load_agent(key) or ws.new_record(key, name=name, agent_id=aid)
rec["end"] = {"at": ws.now(), "signal": "SubagentStop", "detail": ""} if state == "finished" else None
ws.save_agent(rec)
PY
}
registry_record "$ID_FIN" "zach-opus-fin" running
[ "$(verdict "$ID_FIN")" = "ALIVE" ] \
    && ok "A7a POSITIVE PROBE: a live-locked agent the registry records as RUNNING reads ALIVE" \
    || bad "A7a running + live lock -> ALIVE" "got $(verdict "$ID_FIN")"
registry_record "$ID_FIN" "zach-opus-fin" finished
[ "$(verdict "$ID_FIN")" = "NOT-ALIVE" ] \
    && ok "A7 a terminal agent inside a LIVING session (its lock held by a running pid) reads NOT-ALIVE: the registry decides finished (point 11), the lock only proves the session" \
    || bad "A7 registry-finished + live lock -> NOT-ALIVE" "got $(verdict "$ID_FIN")"
A7_JSON="$(python3 "$LIB_PY" --entity "$ENTITY" --owner "$ID_FIN" --format json)"
printf '%s' "$A7_JSON" | grep -q '"lock_verdict": "ALIVE"' \
    && printf '%s' "$A7_JSON" | grep -q '"workspace-registry"' \
    && ok "A7b and the lock's own answer (ALIVE) is kept in the evidence beside the registry's" \
    || bad "A7b evidence keeps both answers" "$A7_JSON"
emit_spawn "zach-opus-fin" "$ID_FIN" "toolu_fin"

echo ""
echo "--- B. INDETERMINATE is a real outcome, never collapsed ---"
B_OUT="$(python3 "$LIB_PY" --entity "$SANDBOX/not-a-repo" --owner "$ID_LIVE" --format triple | cut -f1)"
[ "$B_OUT" = "INDETERMINATE" ] \
    && ok "B1 git cannot be queried -> INDETERMINATE (not NOT-ALIVE)" \
    || bad "B1 unqueryable -> INDETERMINATE" "got $B_OUT"
B_SH="$(bash -c ". '$LIB_SH'; AGENT_LIVENESS_PY=/nonexistent agent_liveness_triple '$ENTITY' '$ID_LIVE'" | cut -f1)"
[ "$B_SH" = "INDETERMINATE" ] \
    && ok "B2 the shell lib with no resolver -> INDETERMINATE, never a guess" \
    || bad "B2 shell lib missing resolver" "got $B_SH"

echo ""
echo ""
echo "--- D. name -> agent id joins exactly, and refuses to guess ---"
D_MAP="$(python3 - "$LIB_PY" "$TRANSCRIPT" <<'PY'
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("al", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
print(json.dumps(m.names_to_ids(sys.argv[2]), sort_keys=True))
PY
)"
printf '%s' "$D_MAP" | grep -q "\"zach-opus-g1\": \"$ID_LIVE\"" \
    && ok "D1 the Agent tool_use name joins to toolUseResult.agentId" \
    || bad "D1 name->id join" "got $D_MAP"
printf '%s' "$D_MAP" | grep -q "zach-opus-neverspawned" \
    && bad "D2 a name never spawned is absent from the map" "it was present" \
    || ok "D2 a name never spawned is absent from the map — no guessing"

echo ""
echo "--- E. the extractor: the three measured constructions, and nothing else ---"
extract() { python3 - "$ANALYZER" "$1" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for name, span, tag in m.extract(sys.argv[2]):
    print("%s\t%s" % (name, tag))
PY
}
E1="$(extract '| Agent | State | What |
|---|---|---|
| `zach-opus-g1` | **completed** | the ask gate — landed and pushed already |')"
printf '%s' "$E1" | grep -q "^zach-opus-g1	table-row$" \
    && ok "E1 T1 TABLE ROW — the exact 2026-08-31 sentence" \
    || bad "E1 table row" "got: $E1"
E2="$(extract 'mark-sonnet-p3 completed and replied exactly P3-RAN as instructed.')"
printf '%s' "$E2" | grep -q "^mark-sonnet-p3	adjacent$" \
    && ok "E2 T2 ADJACENT predicate" || bad "E2 adjacent" "got: $E2"
E3="$(extract 'A fifth, `clark-opus-d1` (the licensing research), finished and has dropped off.')"
printf '%s' "$E3" | grep -q "^clark-opus-d1	appositive$" \
    && ok "E3 T2b APPOSITIVE" || bad "E3 appositive" "got: $E3"

# --- and the refusals, each of which is a measured decision ---
none() { # <label> <text>
    if [ -z "$(extract "$2")" ]; then ok "E- $1"; else bad "E- $1" "fired: $(extract "$2")"; fi
}
none "a BARE ROLE is not an agent (three Zachs ran at once)" \
     "Zach is done with the guard and Mark is finished too."
none "a CONDITIONAL is not a claim" \
     "As soon as zach-opus-g1 is done I will land it."
none "a NEGATION is not a claim" \
     "zach-opus-g1 is not finished yet."
none "REPORTED SPEECH about a past claim is the correction, not the claim" \
     "zach-opus-g1 is plainly alive on your screen and I told you it was completed."
none "a FUTURE is not a claim" \
     "zach-opus-g1 will be done in a minute."
none "LANDED is about the work, not the agent — the row that was RIGHT" \
     "| \`zach-opus-g1\` | ALIVE | landed already; still running |"
none "prose with no identifier at all" \
     "The dispatch completed and everything is done."

echo ""
echo "--- F. the guard NOTICES a claim contradicted by a held lock ---"
DEFECT='| Agent | State | What |
|---|---|---|
| `zach-opus-g1` | **completed** | the ask gate — landed and pushed already |'
if fired_on "$DEFECT"; then
    ok "F1 the 2026-08-31 message, against a held lock -> NOTICE"
else
    bad "F1 the defect message notices" "no systemMessage: $(run_hook "$DEFECT")"
fi
F_OUT="$(run_hook "$DEFECT")"
printf '%s' "$F_OUT" | grep -q 'zach-opus-g1' \
    && ok "F2 the notice NAMES the agent" || bad "F2 names the agent" "$F_OUT"
printf '%s' "$F_OUT" | grep -q 'LOCKED by running pid' \
    && ok "F3 the notice states what the authoritative check returned" \
    || bad "F3 states the authoritative result" "$F_OUT"
printf '%s' "$F_OUT" | grep -q 'agent-liveness.sh' \
    && ok "F4 the notice names the command that settles it" \
    || bad "F4 names the command" "$F_OUT"
printf '%s' "$F_OUT" | grep -q '"suppressOutput":true' \
    && ok "F5 it speaks on the ONE channel measured to reach the operator" \
    || bad "F5 systemMessage channel" "$F_OUT"

echo ""
echo "--- G. the guard is SILENT when it should be ---"
silent_on() { # <label> <message>
    local out
    out="$(run_hook "$2")"
    if printf '%s' "$out" | grep -q 'AGENT-STATE CLAIM CONTRADICTED'; then
        bad "G- $1" "it spoke: $out"
    else
        ok "G- $1"
    fi
}
silent_on "a TRUE claim: the agent really is not alive (stale lock)" \
          "mark-sonnet-p3 completed and replied exactly P3-RAN."
silent_on "a TRUE claim: the agent's worktree is gone entirely" \
          "A fifth, \`clark-opus-d1\` (the licensing research), finished and dropped off."
silent_on "a TRUE claim about a terminal agent INSIDE A LIVING SESSION: the registry records it finished (point 11), so its session's held lock contradicts nothing" \
          "\`zach-opus-fin\` is finished and its work is on its branch."
silent_on "a name that cannot be joined to any agent id" \
          "\`nobody-opus-x9\` is completed."
silent_on "a turn that says nothing about any agent" \
          "Landed and pushed. The tests are green."
G_UN="$( (cd "$UNADOPTED" && payload "$DEFECT" | env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR bash "$HOOK" 2>/dev/null) )"
[ -z "$G_UN" ] && ok "G- an UNADOPTED directory: silent (the noise control)" \
              || bad "G- unadopted silence" "spoke: $G_UN"
G_RE="$(printf '%s' "$(payload "$DEFECT" true)" | RICHOS_ENTITY_ROOT="$ENTITY" bash "$HOOK" 2>/dev/null)"
[ -z "$G_RE" ] && ok "G- stop_hook_active: stands itself down" \
              || bad "G- stop_hook_active" "spoke: $G_RE"

echo ""
echo "--- H. it never blocks ---"
for m in "$DEFECT" "nothing to see here" "\`nobody-opus-x9\` is completed."; do
    run_hook "$m" >/dev/null
    [ "$RC" -eq 0 ] || bad "H1 exit 0 on every path" "exit $RC on: ${m:0:40}"
done
[ "$FAIL" -eq 0 ] && ok "H1 exit 0 on every path (report-only, by construction)"

echo ""
echo "--- R. resolver mutations: each must flip a case, BY NAME ---"
# A mirror of the engine whose lib is a WRITABLE copy, so the shipped source is
# never touched by the suite.
MIRROR="$SANDBOX/mirror"
mkdir -p "$MIRROR/scripts/hooks" "$MIRROR/scripts/lib"
cp "$ENGINE_ROOT/scripts/lib/"*.py "$ENGINE_ROOT/scripts/lib/"*.sh "$MIRROR/scripts/lib/" 2>/dev/null
cp "$ENGINE_ROOT/scripts/lib/cold-open-prompt.md" "$MIRROR/scripts/lib/" 2>/dev/null
cp "$ENGINE_ROOT/scripts/hooks/"*.sh "$ENGINE_ROOT/scripts/hooks/"*.py "$MIRROR/scripts/hooks/" 2>/dev/null

mutate_lib() { # <python-replacement-expr>
    cp "$LIB_PY" "$MIRROR/scripts/lib/agent-liveness.py"
    python3 - "$MIRROR/scripts/lib/agent-liveness.py" "$1" "$2" <<'PY'
import sys
p, old, new = sys.argv[1:4]
s = open(p, encoding="utf-8").read()
# Exactly once: a target that also matches a line added later mutates whichever comes first,
# and the case then "proves nothing" for a reason that has nothing to do with it (2026-10-30:
# the registry branch's own `rec["verdict"] = ALIVE` took R1's mutation from the held lock).
assert s.count(old) == 1, "mutation target must occur exactly once, found %d: %r" % (s.count(old), old)
open(p, "w", encoding="utf-8").write(s.replace(old, new, 1))
PY
}
mirror_verdict() { python3 "$MIRROR/scripts/lib/agent-liveness.py" --entity "$ENTITY" --owner "$1" --format triple | cut -f1; }

mutate_lib '    elif alive:
        rec["verdict"] = ALIVE' '    elif alive:
        rec["verdict"] = NOT_ALIVE'
if [ "$(mirror_verdict "$ID_LIVE")" != "ALIVE" ]; then
    ok "R1 MUTANT 'a held lock is not alive' -> flips A1 (survivor: none)"
else
    bad "R1 mutant flips A1" "the mutant still reported ALIVE — A1 proves nothing"
fi

# A SECOND stale-locked worktree, minted here and consumed by nothing else.
# The first attempt reused $ID_STALE — which section C had already REMOVED, so
# the mutant read NOT-ALIVE for the absent-worktree reason and the mutation
# "survived" for a reason that had nothing to do with the pid check. A negative
# result passing for the wrong reason is the exact trap this section exists to
# catch, and it caught itself.
ID_STALE2="a6666666666666666"
WT_STALE2="$ENTITY/.claude/worktrees/agent-$ID_STALE2"
git -C "$ENTITY" worktree add -q -b wt-stale2 "$WT_STALE2" >/dev/null 2>&1
git -C "$ENTITY" worktree lock \
    --reason "claude agent agent-$ID_STALE2 (pid 999999 start old)" "$WT_STALE2" >/dev/null 2>&1
[ "$(verdict "$ID_STALE2")" = "NOT-ALIVE" ] \
    && ok "R2a the second stale fixture reads NOT-ALIVE before mutation (positive probe)" \
    || bad "R2a stale fixture baseline" "got $(verdict "$ID_STALE2")"

mutate_lib '    alive = _pid_alive(pid)' '    alive = True'
if [ "$(mirror_verdict "$ID_STALE2")" != "NOT-ALIVE" ]; then
    ok "R2 MUTANT 'never check the pid' -> flips A2 (the stale-lock case)"
else
    bad "R2 mutant flips A2" "the stale lock still read NOT-ALIVE — A2 proves nothing"
fi

mutate_lib '        rec["verdict"] = INDETERMINATE
        rec["reason"] = str(e)' '        rec["verdict"] = NOT_ALIVE
        rec["reason"] = str(e)'
if [ "$(python3 "$MIRROR/scripts/lib/agent-liveness.py" --entity "$SANDBOX/not-a-repo" --owner "$ID_LIVE" --format triple | cut -f1)" != "INDETERMINATE" ]; then
    ok "R3 MUTANT 'collapse INDETERMINATE into NOT-ALIVE' -> flips B1"
else
    bad "R3 mutant flips B1" "B1 proves nothing"
fi

# R4: the registry is never consulted — the round-7 resolver, restored. A7's
# finished agent goes back to reading ALIVE from its session's lock.
mutate_lib '    reg = _registry_says(rec.get("agent_id") or "")' '    reg = None'
if [ "$(mirror_verdict "$ID_FIN")" = "ALIVE" ]; then
    ok "R4 MUTANT 'never ask the workspace registry' -> flips A7 (the finished agent reads ALIVE from the session's lock again)"
else
    bad "R4 mutant flips A7" "the mutant still read $(mirror_verdict "$ID_FIN") — A7 proves nothing"
fi
cp "$LIB_PY" "$MIRROR/scripts/lib/agent-liveness.py"

echo ""
echo "--- M. extractor mutations: each must flip a case, BY NAME ---"
mutate_analyzer() { # <old> <new>
    cp "$ANALYZER" "$MIRROR/scripts/hooks/guard-agent-state-claims.py"
    python3 - "$MIRROR/scripts/hooks/guard-agent-state-claims.py" "$1" "$2" <<'PY'
import sys
p, old, new = sys.argv[1:4]
s = open(p, encoding="utf-8").read()
assert s.count(old) == 1, "mutation target must occur exactly once, found %d: %r" % (s.count(old), old)
open(p, "w", encoding="utf-8").write(s.replace(old, new, 1))
PY
}
mirror_extract() { python3 - "$MIRROR/scripts/hooks/guard-agent-state-claims.py" "$1" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for name, span, tag in m.extract(sys.argv[2]):
    print("%s\t%s" % (name, tag))
PY
}

mutate_analyzer 'if SUBORD.search(pre) or FUT.search(pre) or REPORTED.search(s):' \
                'if False:'
if [ -n "$(mirror_extract 'As soon as zach-opus-g1 is done I will land it.')" ]; then
    ok "M1 MUTANT 'drop the conditional/future/reported filters' -> flips E- conditional, E- future, E- reported"
else
    bad "M1 mutant flips the conditional refusals" "those three cases prove nothing"
fi

mutate_analyzer 'r"shut\s+down|wrapped\s+up|no\s+longer\s+running)")' \
                'r"shut\s+down|wrapped\s+up|no\s+longer\s+running|landed)")'
if [ -n "$(mirror_extract '| `zach-opus-g1` | ALIVE | landed already; still running |')" ]; then
    ok "M2 MUTANT 'treat LANDED as terminal' -> flips E- landed (the row that was RIGHT)"
else
    bad "M2 mutant flips E- landed" "that refusal proves nothing"
fi

mutate_analyzer 'NAME_CELL = re.compile(r"^[\s`*_]*(" + NAME + r")[\s`*_]*$", re.I)' \
                'NAME_CELL = re.compile(r"[\s`*_]*(" + NAME + r")", re.I)'
M3_BEFORE="$(extract '| Agent | Lock says | Commits |
|---|---|---|
| `zach-opus-g1` | ALIVE | landed already |')"
if [ -z "$M3_BEFORE" ]; then
    ok "M3 the shipped NAME_CELL anchors the whole cell (an ALIVE row is not a claim)"
else
    bad "M3 anchored name cell" "the shipped extractor fired on an ALIVE row: $M3_BEFORE"
fi
cp "$ANALYZER" "$MIRROR/scripts/hooks/guard-agent-state-claims.py"

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "=== agent-state-claims tests: all $PASS passed ==="
    exit 0
fi
echo "=== agent-state-claims tests: $FAIL FAILED, $PASS passed ==="
exit 1
