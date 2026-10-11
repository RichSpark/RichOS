#!/usr/bin/env bash
#
# guard-stop-live-work.test.sh — the acceptance suite for the PreToolUse
# [TaskStop] gate.
#
# THE HEADLINE CASES ARE THE TWO REAL KILLS OF 2026-09-10, reconstructed from
# the session transcript. If either of them ever passes again, this suite is
# red.
#
# THE SECOND HEADLINE IS 2026-09-20: this guard must not read anybody's
# sentences at all. The deleted "authority B" allowed a kill when a regex found
# a stop imperative in the last user message, and it answered AUTHORIZES to
# "how is the stop command coming along". The section HIS SENTENCES ARE NOT AN
# INPUT holds that ground structurally — by the absence of the code, by the CLI
# refusing the arguments that fed it, and end to end against a transcript in
# which the user plainly orders the stop.
#
# Everything else here exists to keep the guard NARROW: a gate that also
# refuses self-stops, protocol traffic or the cleanup of a finished teammate
# would be waived within a day, and a habitually waived guard is a dead guard
# that still looks alive.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GUARD="$SCRIPT_DIR/guard-stop-live-work.sh"
LIB="$ENGINE_ROOT/scripts/lib/stop-live-work.py"
ACK="$ENGINE_ROOT/scripts/stop-work-ack.sh"

PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL %s\n     %s\n' "$1" "${2:-}"; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A hook writes its runtime record under <entity>/.claude/state/. Every guard
# invocation below is seated on a sandbox entity, never on the engine checkout:
# an unseated run resolved the checkout and wrote unevaluated-payloads.log into
# it (ci-shard.sh fails that as STATE-WRITTEN). The witness at the end asserts it.
# shellcheck source=../lib/entity-sandbox.sh
. "$ENGINE_ROOT/scripts/lib/entity-sandbox.sh"
SANDBOX_ENT="$(entity_sandbox "$ENGINE_ROOT")"
trap 'rm -rf "$TMP" "$SANDBOX_ENT"' EXIT
ENGINE_STATE_DIR="$ENGINE_ROOT/.claude/state"
STATE_BEFORE="$(ls -la "$ENGINE_STATE_DIR" 2>&1; cat "$ENGINE_STATE_DIR"/* 2>/dev/null | cksum)"
guard_sandboxed() { RICHOS_ENTITY_ROOT="$SANDBOX_ENT" CLAUDE_PROJECT_DIR="$SANDBOX_ENT" bash "$GUARD"; }

decide() {  # task liveness entity [self-agent-id] -> verdict
    python3 "$LIB" --task-id "$1" --liveness "$2" \
        --entity-root "$3" ${4:+--self-agent-id "$4"} 2>/dev/null \
        | python3 -c 'import json,sys
try: print(json.load(sys.stdin)["verdict"])
except Exception: print("ERROR")'
}

echo "== THE TWO KILLS OF 2026-09-10 =="
# His sentence that morning — "the currently running agents MIGHT need to be
# paused IF we get close to hitting the quota" — is no longer an input to
# anything here, and these two are refused for the reason that survives:
# a provably live target with no reason written down.
for target in echo-opus-hw2 zach-opus-dor1; do
    v="$(decide "$target" ALIVE "$TMP")"
    [ "$v" = "refuse" ] && ok "$target: a live teammate with nothing written down is refused" \
        || bad "$target: expected refuse, got '$v'"
done

# The refusal must say WHAT IS MISSING, in the terms the one way through uses.
# "he said nothing about stopping" was the old wording and it pointed at a
# predicate that no longer exists; the honest answer is that no ack is on disk.
reason="$(python3 "$LIB" --task-id echo-opus-hw2 --liveness ALIVE \
    --entity-root "$TMP" 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["reason"])')"
case "$reason" in
    *"no live stop-work-ack for echo-opus-hw2"*) ok "the refusal names what is missing, by target" ;;
    *) bad "the refusal does not name the missing ack" "$reason" ;;
esac

echo
echo "== IT NEVER TOUCHES WHAT IT MUST NOT =="
v="$(decide zach-opus-dor1 NOT-ALIVE "$TMP")"
[ "$v" = "allow-not-live" ] && ok "cleanup of a finished teammate is silent" \
    || bad "finished-teammate cleanup: expected allow-not-live, got '$v'"

v="$(decide a1b2c3 ALIVE "$TMP" a1b2c3)"
[ "$v" = "allow-self" ] && ok "a teammate stopping ITSELF is never blocked" \
    || bad "self-stop: expected allow-self, got '$v'"

v="$(decide zach-opus-dor1 INDETERMINATE "$TMP")"
[ "$v" = "allow-undecidable" ] && ok "INDETERMINATE allows, and is not collapsed into either verdict" \
    || bad "indeterminate: expected allow-undecidable, got '$v'"

echo
echo "== HIS SENTENCES ARE NOT AN INPUT (2026-09-20) =="
# The deleted authority B read the last user message and allowed the kill when
# a regex found a stop imperative. Measured that morning, it said AUTHORIZES to
# "how is the stop command coming along". These four cases hold the removal
# down from four directions: the code is gone, the arguments that fed it are
# refused, the decision function cannot be handed text, and a transcript in
# which the user plainly orders the stop changes nothing end to end (the last
# one lives in the live-fixture section, which is the only place a genuinely
# ALIVE target exists).
if grep -q "authorizes_stop\|last_user_message" "$LIB"; then
    bad "the sentence predicate is still in $LIB"
else
    ok "the predicate that read his words is gone from the library"
fi

# Not just the entry points: the machinery. A stop-word or hedge regex left
# behind is the next person's invitation to wire it back up.
if grep -qE "_STOP_VERB|_CONDITIONAL|_NEGATED|_STOP_ANYFORM|split_sentences" "$LIB"; then
    bad "stop-imperative/hedge matching machinery is still present in $LIB"
else
    ok "no stop-word or hedge matching machinery remains"
fi

# THE POSITIVE PROBE FIRST. "The CLI rejected it" is also what a CLI that is
# broken for every input reports, and a negative test that passes for the wrong
# reason is worse than none. So prove the SAME invocation works without the
# argument before believing the rejection means anything.
probe="$(decide x-opus-1 ALIVE "$TMP")"
[ "$probe" = "refuse" ] && ok "the CLI answers that same invocation when the argument is absent" \
    || bad "the CLI is broken for valid input, so the rejections below prove nothing" "$probe"

for arg in --user-text --transcript; do
    if python3 "$LIB" --task-id x-opus-1 --liveness ALIVE --entity-root "$TMP" \
            "$arg" "Stop everything now." >/dev/null 2>&1; then
        bad "the CLI still accepts $arg"
    else
        ok "the CLI refuses $arg — there is no door left in the wall"
    fi
done

got="$(python3 -c '
import importlib.util,inspect,sys
spec=importlib.util.spec_from_file_location("slw",sys.argv[1])
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
print(",".join(inspect.signature(m.decide).parameters))' "$LIB" 2>/dev/null)"
case "$got" in
    *user_text*|*text*) bad "decide() still takes the user's text" "$got" ;;
    "") bad "decide() could not be introspected" ;;
    *) ok "decide() takes liveness and an entity root — no text parameter" ;;
esac

echo
echo "== THE WRITTEN ACK =="
E="$TMP/entity"; mkdir -p "$E"
bash "$ACK" --entity "$E" --task echo-opus-hw2 \
    --destroying "an uncommitted three-hour session with a clean worktree" \
    --why "its brief was superseded and it is building the wrong thing" >/dev/null 2>&1
v="$(decide echo-opus-hw2 ALIVE "$E")"
[ "$v" = "allow-acked" ] && ok "a well-formed ack allows the stop" \
    || bad "well-formed ack: expected allow-acked, got '$v'"

# ONE ACK, ONE TARGET. The two kills were 2.7 seconds apart.
v="$(decide zach-opus-dor1 ALIVE "$E")"
[ "$v" = "refuse" ] && ok "the ack covers ONE target, not the sweep" \
    || bad "ack leaked to another target: got '$v'"

# ONE ACK, ONE DESTRUCTION.
python3 "$LIB" --task-id echo-opus-hw2 --liveness ALIVE \
    --entity-root "$E" --consume >/dev/null 2>&1
v="$(decide echo-opus-hw2 ALIVE "$E")"
[ "$v" = "refuse" ] && ok "the ack is spent on first use" \
    || bad "a spent ack was reused: got '$v'"

# A BARE MARKER EXEMPTS NOTHING.
if bash "$ACK" --entity "$E" --task x-opus-1 --destroying "n/a" --why "n/a" >/dev/null 2>&1; then
    bad "the recorder accepted a bare marker"
else
    ok "the recorder refuses an ack that says nothing"
fi
if bash "$ACK" --entity "$E" --destroying "aaaaaaaaaaaaaaaaaaaaaaaa" --why "bbbbbbbbbbbbbbbbbbbbbbbb" >/dev/null 2>&1; then
    bad "the recorder accepted an ack naming no target"
else
    ok "the recorder refuses an ack that names no target"
fi

# AN ACK EXPIRES. A decision banked yesterday is not a decision taken today.
python3 - "$LIB" "$E" <<'PY'
import importlib.util, json, os, sys, time
spec = importlib.util.spec_from_file_location("slw", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
E = sys.argv[2]
p = m.ack_path(E)
with open(p, "a") as fh:
    fh.write(json.dumps({"id": "oldone", "task_id": "stale-opus-1",
                         "destroying": "an uncommitted afternoon of work here",
                         "why": "a reason written a very long time ago now",
                         "epoch": time.time() - 86400, "consumed": False}) + "\n")
ack, why = m.find_live_ack(E, "stale-opus-1")
sys.exit(0 if ack is None and "older than" in why else 1)
PY
[ $? -eq 0 ] && ok "a day-old ack does not authorize today's kill" \
    || bad "a stale ack was honored"

echo
echo "== THE HOOK WIRING =="
payload() { printf '{"tool_name":"%s","tool_input":%s,"session_id":"s1","cwd":"%s","transcript_path":"%s"}' "$1" "$2" "${3:-$PWD}" "${4:-}"; }

out="$(printf '%s' "$(payload Bash '{"command":"ls"}')" | guard_sandboxed 2>&1)"; rc=$?
[ $rc -eq 0 ] && [ -z "$out" ] && ok "a payload for another tool exits 0 in silence" \
    || bad "non-TaskStop payload: rc=$rc out=$out"

out="$(printf 'not json at all' | guard_sandboxed 2>&1)"; rc=$?
[ $rc -eq 0 ] && ok "an unreadable payload fails OPEN (never blocks a cleanup)" \
    || bad "unreadable payload blocked: rc=$rc"

out="$(printf '%s' "$(payload TaskStop '{"task_id":""}')" | guard_sandboxed 2>&1)"; rc=$?
[ $rc -eq 0 ] && ok "a TaskStop with no task_id fails OPEN and says so" \
    || bad "empty task_id blocked: rc=$rc"

# A target that resolves to nothing at all -> INDETERMINATE -> allowed, noisily.
out="$(printf '%s' "$(payload TaskStop '{"task_id":"nobody-opus-zz9"}')" | guard_sandboxed 2>&1)"; rc=$?
[ $rc -eq 0 ] && ok "an unresolvable target is allowed, not blocked" \
    || bad "unresolvable target blocked: rc=$rc out=$out"

echo
echo "== THE END-TO-END REFUSAL, AGAINST A GENUINELY LIVE AGENT =="
# Build a real locked isolation worktree so agent-liveness.py returns ALIVE
# from its authoritative source rather than from a stub.
REPO="$TMP/repo"
mkdir -p "$REPO"
(
  cd "$REPO"
  git init -q .
  git config user.email t@t; git config user.name t
  # An adopted engine root: the resolver refuses to govern a repository that
  # never adopted the engine, and refusing to guess is correct of it.
  printf 'MODEL_CEILING="opus"\nMODEL_TIERS="fable > opus > sonnet > haiku"\n' \
      > orchestration.config
  echo x > f; git add f orchestration.config; git commit -qm init
  mkdir -p .claude/worktrees
  git worktree add -q .claude/worktrees/agent-deadbeefcafe1234 -b worktree-deadbeefcafe1234 >/dev/null 2>&1
  git worktree lock --reason "claude agent agent-deadbeefcafe1234 (pid $$ start now)" \
      .claude/worktrees/agent-deadbeefcafe1234 >/dev/null 2>&1
) >/dev/null 2>&1
LIVEV="$(bash "$ENGINE_ROOT/scripts/agent-liveness.sh" --entity "$REPO" \
    deadbeefcafe1234 2>/dev/null | tail -1 || true)"
if bash "$ENGINE_ROOT/scripts/agent-liveness.sh" --entity "$REPO" deadbeefcafe1234 >/dev/null 2>&1; then
    :
fi
rc_l=$?
# The resolver's own verdict decides whether this arm can run at all; if the
# environment cannot produce a locked worktree, say so rather than pass by
# accident. A negative test that passes for the wrong reason is worse than none.
if bash "$ENGINE_ROOT/scripts/agent-liveness.sh" --entity "$REPO" deadbeefcafe1234 >/dev/null 2>&1; then
    bad "could not build a LIVE fixture: the resolver says not-alive" "$LIVEV"
else
    lrc=$?
    if [ "$lrc" -eq 10 ]; then
        ok "fixture is genuinely ALIVE by the authoritative resolver (locked worktree)"
        out="$(printf '%s' "$(payload TaskStop '{"task_id":"deadbeefcafe1234"}' "$REPO/.claude/worktrees/other")" \
              | RICHOS_ENTITY_ROOT="$REPO" CLAUDE_PROJECT_DIR="$REPO" bash "$GUARD" 2>&1)"; rc=$?
        if [ $rc -eq 2 ]; then
            ok "a provably live agent is BLOCKED end to end through the hook"
            case "$out" in
                *"stop-work-ack.sh"*) ok "the refusal names the one way through" ;;
                *) bad "the refusal does not name stop-work-ack.sh" ;;
            esac
            case "$out" in
                *"stop DISPATCHING"*) ok "the refusal states the cheaper truth" ;;
                *) bad "the refusal omits the stop-dispatching lesson" ;;
            esac
        else
            bad "live agent NOT blocked end to end: rc=$rc" "$out"
        fi
        # THE 2026-09-20 CASE, END TO END. A transcript whose last genuine user
        # message is as plain a stop order as exists — no hedge, no question,
        # naming this very target — and the live agent is STILL blocked,
        # because no ack is on disk. Under authority B this passed; it is the
        # same door "how is the stop command coming along" walked through.
        ORDER_T="$TMP/ceo-order.jsonl"
        cat >"$ORDER_T" <<'JSONL'
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Stop deadbeefcafe1234 now."}]}}
JSONL
        out="$(printf '%s' "$(payload TaskStop '{"task_id":"deadbeefcafe1234"}' "$REPO/.claude/worktrees/other" "$ORDER_T")" \
              | RICHOS_ENTITY_ROOT="$REPO" CLAUDE_PROJECT_DIR="$REPO" bash "$GUARD" 2>&1)"; rc=$?
        [ $rc -eq 2 ] && ok "a plain stop order in the transcript does NOT authorize the kill — the ack does" \
            || bad "transcript text authorized a live kill: rc=$rc" "$out"
        case "$out" in
            *"stop.sh"*) ok "the refusal names stop.sh, the way his order becomes an ack in seconds" ;;
            *) bad "the refusal does not name stop.sh" "$out" ;;
        esac

        # And the self-stop escape, through the same hook, same fixture.
        out="$(printf '%s' "$(payload TaskStop '{"task_id":"deadbeefcafe1234"}' "$REPO/.claude/worktrees/agent-deadbeefcafe1234")" \
              | RICHOS_ENTITY_ROOT="$REPO" CLAUDE_PROJECT_DIR="$REPO" bash "$GUARD" 2>&1)"; rc=$?
        [ $rc -eq 0 ] && ok "the same live agent stopping ITSELF passes" \
            || bad "self-stop blocked end to end: rc=$rc" "$out"
    else
        bad "could not build a LIVE fixture (resolver exit $lrc)" "$LIVEV"
    fi
fi

echo
echo "== THE SUITE WRITES NOTHING INTO THE ENGINE CHECKOUT =="
STATE_AFTER="$(ls -la "$ENGINE_STATE_DIR" 2>&1; cat "$ENGINE_STATE_DIR"/* 2>/dev/null | cksum)"
[ "$STATE_BEFORE" = "$STATE_AFTER" ] && ok "the engine's .claude/state is unchanged by this suite" \
    || bad "the suite wrote into $ENGINE_STATE_DIR" "$STATE_AFTER"
[ -s "$SANDBOX_ENT/.claude/state/unevaluated-payloads.log" ] \
    && ok "the unreadable-payload record landed in the sandbox entity instead" \
    || bad "no unreadable-payload record in the sandbox: the seat is not in effect"

echo
printf 'passed %d, failed %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
