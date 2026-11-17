#!/usr/bin/env bash
#
# guard-unresolved-claims.test.sh — regression tests for the Stop-time claim
# gate (scripts/hooks/guard-unresolved-claims.sh + .py).
#
# THE TWO REAL FAILURES ARE REPLAYED HERE, STRUCTURALLY, NOT VERBATIM.
#   richos is a public repository, so no operator speech and no transcript text
#   is committed. Every fixture below reproduces the SHAPE of a real failure —
#   an agent name with no spawn, a SHA that names no object, a nameless action
#   claim in a turn with zero Agent calls — with invented content. The
#   measurement that justified each threshold was run against the real
#   transcripts on disk and only the NUMBERS are recorded, in the hook header.
#
# Covers:
#   BLOCKING — agent names (monotonic ground truth: the ledger only grows)
#     (a)  name never spawned, session team dir present     -> exit 2
#     (b)  name present in spawned-names.log                -> exit 0
#     (c)  name present in the roster only                  -> exit 0
#     (d)  name present in an event log only                -> exit 0
#     (e)  name never spawned, session team dir ABSENT      -> exit 0 (inert)
#     (f)  name never spawned but grounded in tool output   -> exit 0
#   BLOCKING — commit SHAs
#     (g)  SHA that resolves in the entity repo             -> exit 0
#     (h)  SHA that names no object anywhere                -> exit 2
#     (i)  dead SHA grounded in this session's tool output  -> exit 0
#   MUST NOT FIRE — the precision cases
#     (j)  UUID fragments, tool-use ids, hex-looking prose  -> exit 0
#     (k)  ratios and labels that look like paths (13/13)   -> exit 0
#     (l)  a path that does not resolve                     -> exit 0 + report
#     (m)  quoting the operator; describing another agent   -> exit 0
#     (n)  a legitimate turn end: a hold, a question, an
#          answer, a named next step awaiting a decision    -> exit 0
#   FAILURE REPLAYS
#     (o)  FAILURE 2, as written: nameless action claim,
#          zero Agent calls                                 -> exit 0 + report
#     (o2) FAILURE 2, named: the same claim naming an agent
#          that was never spawned                           -> exit 2
#     (p)  FAILURE 1: a turn naming the next step without
#          taking it                                        -> exit 0 + report
#     (p2) Agent called INSIDE this turn                    -> exit 0, silent
#     (p3) Agent called only in an EARLIER turn             -> exit 0 + report
#          (p2/p3 are the turn-scoping regression: a session-wide tool view
#           silently disarmed the reporting layer after the first spawn)
#   SAFETY — this gate must never strand a session
#     (q)  stop_hook_active=true, violation present         -> exit 0
#     (r)  repository has not adopted the engine            -> exit 0
#     (s)  CHECK_UNRESOLVED_CLAIMS=0                        -> exit 0 + notice
#     (t)  unparseable payload                              -> exit 0
#     (u)  empty last_assistant_message                     -> exit 0
#     (v)  transcript_path missing / unreadable             -> exit 2 still
#          works from last_assistant_message alone
#     (w)  broken install (resolve-roots.sh absent)         -> exit 0 + banner
#   RECORD
#     (x)  an observation line is appended for every turn
#
# Run directly: scripts/hooks/guard-unresolved-claims.test.sh
# Exit 0 = all pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/guard-unresolved-claims.sh"
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0

SANDBOX="$(mktemp -d -t guard-unresolved-claims.XXXXXX)"
# THE WORKSPACE REGISTRY IS A SANDBOX ONE. The gate asks
# mega-lander/workspaces.py which branch this work integrates on (point 14),
# and this suite RECORDS one for its fixture repository -- which must never
# reach the operator's real ~/.claude/state registry.
export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces-registry"
trap 'rm -rf "$SANDBOX"' EXIT

SESSION_ID="feedface-0000-4000-8000-000000000000"
TEAMS="$SANDBOX/teams"
TEAM_DIR="$TEAMS/session-feedface"
mkdir -p "$TEAM_DIR"

printf 'norm-opus-a1\n' > "$TEAM_DIR/spawned-names.log"
# agentType and status are what liveness is read from. mark is RUNNING; norm
# was spawned (it is in the ledger above) and is NOT in the roster, which is
# the "ran earlier, finished" shape.
cat >"$TEAM_DIR/config.json" <<'JSON'
{ "name": "session-feedface",
  "members": [ { "name": "team-lead" },
               { "name": "mark-sonnet-r7", "agentType": "mark", "status": "running" },
               { "name": "norm-opus-a1", "agentType": "norm", "status": "completed" } ] }
JSON
printf '{"teammate":"echo-opus-k3"}\n' > "$TEAM_DIR/idle-events.jsonl"

# Short IDs must contain both letters and digits to exercise the claim gate.
# Random seven-character IDs can be entirely decimal (or alphabetic). Extend
# the real object prefix in that case instead of silently testing no claim.
claim_fixture_sha() {
    local full="$1" width token
    [[ "$full" =~ ^[0-9a-f]{40}$ ]] || return 1
    for ((width=7; width<=40; width++)); do
        token="${full:0:width}"
        if [[ "$token" == *[0-9]* && "$token" == *[a-f]* ]]; then
            printf '%s\n' "$token"
            return 0
        fi
    done
    return 1
}
# Deterministic controls cover both formerly random failure shapes.
if [ "$(claim_fixture_sha 1234567abcdef0123456789abcdef0123456789a)" != 1234567a ] ||
   [ "$(claim_fixture_sha abcdefa123456789abcdef0123456789abcdef01)" != abcdefa1 ]; then
    echo 'FATAL: claim fixture prefix selection failed' >&2
    exit 1
fi

# --- the governed repository ----------------------------------------------
# A real git repo, because the SHA resolver runs `git cat-file` against it.
ENTITY="$SANDBOX/entity"
mkdir -p "$ENTITY"
git -C "$ENTITY" init -q . >/dev/null 2>&1
git -C "$ENTITY" config user.email t@t.t >/dev/null 2>&1
git -C "$ENTITY" config user.name t >/dev/null 2>&1
# The operator may have a global core.hooksPath (an identity guard, a linter).
# A suite that inherits it fails for reasons that have nothing to do with the
# thing under test, so the sandbox repo gets an empty hooks dir of its own.
mkdir -p "$SANDBOX/nohooks"
git -C "$ENTITY" config core.hooksPath "$SANDBOX/nohooks" >/dev/null 2>&1
: > "$ENTITY/orchestration.config"
# The role vocabulary is DERIVED from agent definitions on disk (unioned with
# the roster's own agentType values), never typed into the guard.
mkdir -p "$ENTITY/.claude/agents"
for r in zach norm mark art isaac; do printf -- '---\nname: %s\n---\n' "$r" > "$ENTITY/.claude/agents/$r.md"; done
mkdir -p "$ENTITY/docs"
printf 'real\n' > "$ENTITY/docs/real-file.md"
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm seed >/dev/null 2>&1
LIVE_SHA="$(claim_fixture_sha "$(git -C "$ENTITY" rev-parse HEAD)")"
# Hex, 7 chars, has a digit and a letter, and names no object anywhere.
DEAD_SHA="d0d0d0d"

DEFAULT_BRANCH="$(git -C "$ENTITY" rev-parse --abbrev-ref HEAD)"

# --- the three ref-graph states a state claim can be in --------------------
# The gate's whole precision argument lives in the difference between these,
# so the sandbox has to contain all three or the suite is testing one branch of
# a three-way decision.
#
#   BRANCH_SHA   alive on a branch, not on main   -> the 2026-09-01 failure
#   LIVE_SHA     on main                          -> the positive control
#   DANGLING_SHA in the object DB, on no ref       -> a rewrite casualty, SILENT
git -C "$ENTITY" checkout -q -b unlanded-branch
printf 'not landed\n' > "$ENTITY/docs/unlanded.md"
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm "on a branch, never merged" >/dev/null 2>&1
BRANCH_SHA="$(claim_fixture_sha "$(git -C "$ENTITY" rev-parse HEAD)")"
git -C "$ENTITY" checkout -q "$DEFAULT_BRANCH"

git -C "$ENTITY" checkout -q -b doomed
printf 'rewritten away\n' > "$ENTITY/docs/doomed.md"
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm "the shape a history rewrite leaves behind" >/dev/null 2>&1
DANGLING_SHA="$(claim_fixture_sha "$(git -C "$ENTITY" rev-parse HEAD)")"
git -C "$ENTITY" checkout -q "$DEFAULT_BRANCH"
git -C "$ENTITY" branch -qD doomed >/dev/null 2>&1

# A remote-tracking ref that is BEHIND the local branch, so the tip is
# committed and unpushed while the seed commit is published.
git -C "$ENTITY" update-ref "refs/remotes/origin/$DEFAULT_BRANCH" "$LIVE_SHA"
printf 'not pushed\n' > "$ENTITY/docs/unpushed.md"
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm "committed, not pushed" >/dev/null 2>&1
UNPUSHED_SHA="$(claim_fixture_sha "$(git -C "$ENTITY" rev-parse HEAD)")"

# POINT 14: THE BRANCH THIS BODY OF WORK INTEGRATES ON, RECORDED.
# The gate used to carry its own INTEGRATION_REFS tuple -- main, master,
# origin/main, origin/master, HEAD -- which is an answer of its own, and on a
# repository whose work integrates on a dev branch it refused truthful
# "integrated" claims. It now asks mega-lander/workspaces.py, so this fixture
# records the branch exactly as Rich does. With nothing recorded the gate
# ABSTAINS into "unknown", which is its silent verdict, so the recording is
# what makes every "landed" case below test the gate rather than the
# abstention. Case z7 is the abstention itself.
"$SCRIPT_DIR/../../mega-lander/workspaces.sh" integration \
    --repo "$ENTITY" --branch "$DEFAULT_BRANCH" \
    --why "the guard-unresolved-claims fixture" >"$SANDBOX/integration.log" 2>&1 || {
    echo "  FAIL  fixture.integration-branch-could-not-be-recorded"
    cat "$SANDBOX/integration.log"
    exit 1
}

# A value present in TWO spellings, one of which a claim will not name.
printf -- '--mark: #9C7C34;\n' > "$ENTITY/docs/style.css"
printf -- 'expect(mark).toBe("rgb(143, 112, 48)");\n' > "$ENTITY/docs/appearance.js"
git -C "$ENTITY" add -A >/dev/null 2>&1
git -C "$ENTITY" commit -qm "two spellings of one value" >/dev/null 2>&1

# --- THE FIXTURE IS ASSERTED BEFORE ANY CASE READS IT ----------------------
#
# Every git command above is `>/dev/null 2>&1` and none of their exit codes is
# checked. That is deliberate for the noisy ones, and it has a consequence
# nobody had written down: if one of those commits does not happen — an
# identity guard, a stray global hook, a lock contended under load — then
# `rev-parse HEAD` prints nothing and the SHA is the EMPTY STRING.
#
# An empty SHA does not make a case fail. It makes the case test something
# else. `z7.` becomes "The branch tip is `` and I am waiting on your call",
# a sentence with no hex token in it at all, which the gate is right to ignore
# and which proves nothing about the property z7 exists for. It reports PASS.
#
# THAT IS THE MECHANISM BEHIND A FLAKE THIS SUITE'S HARNESS HAS BEEN CARRYING.
# claim-roles.mutation.sh records `state-claim-needs-no-verb` going red "but
# NOT at z7." on some runs and not others, and its header names the cause in
# general terms — "its reachability cases need to stop depending on state that
# moves under them". This is that dependence, at its root: not the cases, the
# fixture they are written against. Observed here on 2026-09-06 inside a full
# contract-integrity pass at load average 10-11, while the same harness run
# alone passed 20/20 immediately before and after.
#
# So the fixture is now ASSERTED, loudly, before the first case runs. This does
# not stop the underlying git operation from failing — it makes a failure say
# so instead of quietly rewriting what 65 cases mean.
FIXTURE_BAD=""
for _pair in "LIVE_SHA:$LIVE_SHA" "BRANCH_SHA:$BRANCH_SHA" "DANGLING_SHA:$DANGLING_SHA" "UNPUSHED_SHA:$UNPUSHED_SHA"; do
    _name="${_pair%%:*}"; _val="${_pair#*:}"
    if ! [[ "$_val" =~ ^[0-9a-f]{7,40}$ && "$_val" == *[0-9]* && "$_val" == *[a-f]* ]]; then
        FIXTURE_BAD="$FIXTURE_BAD $_name='$_val'"
    fi
done
# ...and that they are DISTINCT. Four names for one commit would silently
# collapse the three-way ref-graph distinction the cases are built on.
if [ -z "$FIXTURE_BAD" ]; then
    _uniq="$(printf '%s\n%s\n%s\n%s\n' "$LIVE_SHA" "$BRANCH_SHA" "$DANGLING_SHA" "$UNPUSHED_SHA" | sort -u | grep -c .)"
    [ "$_uniq" -eq 4 ] || FIXTURE_BAD="$FIXTURE_BAD not-distinct(live=$LIVE_SHA branch=$BRANCH_SHA dangling=$DANGLING_SHA unpushed=$UNPUSHED_SHA)"
fi
if [ -n "$FIXTURE_BAD" ]; then
    printf '  FATAL  the sandbox repository was not built as these cases assume:%s\n' "$FIXTURE_BAD" >&2
    printf '         A SHA that is empty or duplicated does not make a case fail — it makes it\n' >&2
    printf '         test a different sentence and report PASS. Refusing to run 65 cases over it.\n' >&2
    printf '         Most likely a git commit in the setup above did not happen; re-run with the\n' >&2
    printf '         `>/dev/null 2>&1` removed from the setup commands to see which.\n' >&2
    exit 1
fi

# A repository that never adopted the engine: no orchestration.config.
UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"
git -C "$UNADOPTED" init -q . >/dev/null 2>&1

# --- transcript fixtures ---------------------------------------------------
# At Stop time the transcript holds the turn's tool_use/tool_result records but
# NOT the final assistant text. These fixtures mirror that exactly.
EMPTY_TR="$SANDBOX/empty.jsonl"
: > "$EMPTY_TR"

BASH_TR="$SANDBOX/bash-turn.jsonl"
cat >"$BASH_TR" <<JSON
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"git log --oneline -3"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","content":"$DEAD_SHA an old commit\nzz-opus-zz9 was in the ledger once"}]}}
JSON

AGENT_TR="$SANDBOX/agent-turn.jsonl"
cat >"$AGENT_TR" <<'JSON'
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","input":{"name":"norm-opus-a1"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","content":"agent started"}]}}
JSON

# THE TURN-SCOPING FIXTURES. These exist because of a real bug: the tool-call
# names were collected session-wide, so once ANY Agent call had happened the
# reporting layer believed every later turn had dispatched something, and the
# prose signal silently never fired again. Grounding wants the whole session;
# "did THIS turn call Agent?" wants only this turn. Two scopes, one function,
# and the suite must hold them apart.
PROMPT_ID="11111111-2222-3333-4444-555555555555"

# An Agent call INSIDE the turn -> the claim is backed, nothing to report.
AGENT_IN_TURN="$SANDBOX/agent-in-turn.jsonl"
cat >"$AGENT_IN_TURN" <<JSON
{"type":"user","promptId":"$PROMPT_ID","message":{"content":"go"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","input":{"name":"norm-opus-a1"}}]}}
JSON

# An Agent call in an EARLIER turn only -> this turn dispatched nothing, and the
# session-wide view that used to hide that must not come back.
AGENT_EARLIER="$SANDBOX/agent-earlier.jsonl"
cat >"$AGENT_EARLIER" <<JSON
{"type":"user","promptId":"00000000-0000-4000-8000-000000000000","message":{"content":"earlier"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","input":{"name":"norm-opus-a1"}}]}}
{"type":"user","promptId":"$PROMPT_ID","message":{"content":"now"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}
JSON

# An Agent call in THIS turn that the host REFUSED (its result is an error)
# dispatched nobody (hunt part 3 v3, finding 19).
AGENT_REFUSED="$SANDBOX/agent-refused.jsonl"
cat >"$AGENT_REFUSED" <<JSON
{"type":"user","promptId":"$PROMPT_ID","message":{"content":"go"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"refused1","name":"Agent","input":{"name":"norm-opus-a1"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"refused1","is_error":true,"content":"Refused before dispatch"}]}}
JSON

# --- payload builder -------------------------------------------------------
payload() { # <message> [transcript] [session_id] [stop_hook_active] [cwd]
    python3 - "$1" "${2:-$EMPTY_TR}" "${3:-$SESSION_ID}" "${4:-false}" "${5:-$ENTITY}" <<'PY'
import json, sys
msg, tr, sid, active, cwd = sys.argv[1:6]
print(json.dumps({
    "hook_event_name": "Stop",
    "session_id": sid,
    "transcript_path": tr,
    "cwd": cwd,
    "prompt_id": "11111111-2222-3333-4444-555555555555",
    "permission_mode": "bypassPermissions",
    "stop_hook_active": active == "true",
    "last_assistant_message": msg,
    "background_tasks": [],
    "session_crons": [],
}))
PY
}

LAST_ERR=""
run_case() { # <name> <expected-exit> <payload-json> [needle-in-stderr] [FORBIDDEN-needle-in-stderr]
    #
    # THE FIFTH ARGUMENT EXISTS BECAUSE AN EXIT CODE IS NOT ALWAYS A WITNESS.
    # This hook fails OPEN: guard-unresolved-claims.sh treats anything but 2 as
    # "let the turn end", including a crash in the analyzer. So a case that
    # asserts only `exit 0` cannot tell "the analyzer ran and found nothing"
    # from "the analyzer raised and the wrapper swallowed it" — they are the
    # same exit code. A forbidden needle lets a case say which one it saw.
    local name="$1" want="$2" json="$3" needle="${4:-}" forbidden="${5:-}" got err
    err="$(mktemp "$SANDBOX/err.XXXXXX")"
    printf '%s' "$json" | RICHOS_ENTITY_ROOT="$ENTITY" \
        RICHOS_CLAIMS_TEAMS_DIR="$TEAMS" "$HOOK" >/dev/null 2>"$err"
    got=$?
    LAST_ERR="$(cat "$err")"
    if [ "$got" -ne "$want" ]; then
        printf '  FAIL  %s (expected exit %s, got %s)\n' "$name" "$want" "$got"
        [ -s "$err" ] && sed 's/^/          /' "$err"
        FAIL=$((FAIL + 1))
    elif [ -n "$needle" ] && ! grep -qF "$needle" "$err"; then
        printf '  FAIL  %s (stderr missing %q)\n' "$name" "$needle"
        [ -s "$err" ] && sed 's/^/          /' "$err"
        FAIL=$((FAIL + 1))
    elif [ -n "$forbidden" ] && grep -qF "$forbidden" "$err"; then
        printf '  FAIL  %s (stderr carries %q, which this case forbids)\n' "$name" "$forbidden"
        [ -s "$err" ] && sed 's/^/          /' "$err"
        FAIL=$((FAIL + 1))
    else
        printf '  PASS  %s\n' "$name"
        PASS=$((PASS + 1))
    fi
    rm -f "$err"
}

echo "=== guard-unresolved-claims.sh ==="

# --- BLOCKING: agent names -------------------------------------------------
run_case "a.name-never-spawned-blocks" 2 \
    "$(payload 'Dispatched sage-opus-q4 on the migration.')" \
    "never spawned"

# The name with its effort word (2026-11-17) is read as one name, like the three parts before it.
run_case "a2.four-part-name-never-spawned-blocks" 2 \
    "$(payload 'Dispatched sage-opus-high-q4 on the migration.')" \
    "never spawned"

run_case "b.name-in-ledger-passes" 0 \
    "$(payload 'norm-opus-a1 landed the fix.')"

run_case "c.name-in-roster-passes" 0 \
    "$(payload 'mark-sonnet-r7 is still running.')"

run_case "d.name-in-event-log-passes" 0 \
    "$(payload 'echo-opus-k3 finished and reported.')"

run_case "e.no-session-team-dir-is-inert" 0 \
    "$(payload 'Dispatched sage-opus-q4 on the migration.' "$EMPTY_TR" \
       'aaaaaaaa-0000-4000-8000-000000000000')"

run_case "f.name-grounded-in-tool-output-passes" 0 \
    "$(payload 'The ledger still lists zz-opus-zz9 from an earlier session.' "$BASH_TR")"

# --- BLOCKING: commit SHAs -------------------------------------------------
run_case "g.live-sha-passes" 0 \
    "$(payload "Landed at \`$LIVE_SHA\`, tree clean.")"

run_case "h.dead-sha-blocks" 2 \
    "$(payload "Landed at \`$DEAD_SHA\`, tree clean." "$EMPTY_TR")" \
    "name no object"

run_case "i.dead-sha-grounded-passes" 0 \
    "$(payload "The rewrite dropped \`$DEAD_SHA\`, so that citation is now dead." "$BASH_TR")"

# --- MUST NOT FIRE: precision ----------------------------------------------
run_case "j.uuid-and-toolid-fragments-ignored" 0 \
    "$(payload 'Prompt 49fb795d-c3cd-4fe5-92f1-1014c3f81bf1 and toolu_01LXJkEdyh9Rh68N2W8u3zVD, plus deadbeef and 1234567 in prose.')"

run_case "k.ratios-are-not-paths" 0 \
    "$(payload 'Suites green: `18/18`, guards `13/13`, criteria `44/46`.')"

run_case "l.unresolved-path-reports-never-blocks" 0 \
    "$(payload 'I deleted `docs/gone-forever.md` in the cleanup.')" \
    "not blocking"

run_case "m.describing-others-and-the-past-passes" 0 \
    "$(payload 'He said "just ship it". norm-opus-a1 already merged that yesterday, and I am not spawning anything further tonight.')"

run_case "n.legitimate-turn-end-passes" 0 \
    "$(payload 'Two options and I need your call before either: rename the field, or add a second one. Nothing is running while you decide.')"

# --- FAILURE REPLAYS -------------------------------------------------------
# FAILURE 2 as it actually occurred: an action claim with NO identifier in it.
# The identifier gate cannot see this and must not pretend to; the reporting
# layer records it. Asserting exit 0 here is the honest limit, written down.
run_case "o.failure2-nameless-claim-reports-only" 0 \
    "$(payload 'Separate trees, no collision — dispatching it now instead of holding it.' "$BASH_TR")" \
    "in-flight dispatch claim"

# The same claim in the form the engine's naming doctrine already requires.
# Naming the agent is what moves the failure into the blocking layer.
run_case "o2.failure2-named-claim-blocks" 2 \
    "$(payload 'Separate trees, no collision — dispatching mark-opus-v2 now instead of holding it.' "$BASH_TR")" \
    "never spawned"

# FAILURE 1: naming the next step without taking it. Sometimes correct (a hold,
# a decision point), so it is REPORTED, never blocked.
run_case "p.failure1-next-step-named-not-taken" 0 \
    "$(payload 'That leaves the scanner hole. I am spawning someone on it right after this lands.' "$BASH_TR")" \
    "in-flight dispatch claim"

# Turn scoping (the regression above). Same message both times; only the
# position of the Agent call moves.
run_case "p2.agent-called-this-turn-nothing-to-report" 0 \
    "$(payload 'Separate trees, no collision — dispatching it now instead of holding it.' "$AGENT_IN_TURN")"
if printf '%s' "$LAST_ERR" | grep -qF "in-flight dispatch claim"; then
    printf '  FAIL  p2.agent-called-this-turn-nothing-to-report (reported anyway)\n'
    FAIL=$((FAIL + 1)); PASS=$((PASS - 1))
fi

run_case "p3.agent-called-only-in-an-earlier-turn-still-reports" 0 \
    "$(payload 'Separate trees, no collision — dispatching it now instead of holding it.' "$AGENT_EARLIER")" \
    "in-flight dispatch claim"

run_case "p4.refused-agent-call-this-turn-still-reports (P3-19)" 0 \
    "$(payload 'Separate trees, no collision — dispatching it now instead of holding it.' "$AGENT_REFUSED")" \
    "in-flight dispatch claim"

# --- SAFETY: never strand a session ----------------------------------------
run_case "q.stop-hook-active-stands-down" 0 \
    "$(payload 'Dispatched sage-opus-q4 on the migration.' "$EMPTY_TR" "$SESSION_ID" true)"

printf '%s' "$(payload 'Dispatched sage-opus-q4.' "$EMPTY_TR" "$SESSION_ID" false "$UNADOPTED")" \
    | RICHOS_ENTITY_ROOT="$UNADOPTED" RICHOS_CLAIMS_TEAMS_DIR="$TEAMS" "$HOOK" >/dev/null 2>&1
if [ $? -eq 0 ]; then
    printf '  PASS  r.unadopted-repository-passes\n'; PASS=$((PASS + 1))
else
    printf '  FAIL  r.unadopted-repository-passes\n'; FAIL=$((FAIL + 1))
fi

# --- s. THE OPT-OUT IS SEEN BY THE OPERATOR, NOT JUST BY THE TRANSCRIPT -----
#
# This case used to assert the notice appeared on STDERR, and passed. It was
# asserting the defect. Measured against 2.1.251, a Stop hook's stderr and its
# plain stdout are both filed into the transcript as a `hook_success`
# attachment and rendered to the operator NOWHERE; only a stdout
# {"systemMessage": ...} reaches his scroll. So the old assertion confirmed
# that the guard could be switched off and say so where nobody would ever read
# it — which is indistinguishable from not saying so at all.
#
# Both halves are asserted, because "it is on the right channel" and "it is no
# longer only on the wrong one" are different claims and only the pair rules
# out a notice that was duplicated rather than moved.
printf 'CHECK_UNRESOLVED_CLAIMS=0\n' > "$ENTITY/orchestration.config"
rm -rf "$ENTITY/.claude/state/stop-hook-notices"
S_OUT="$(mktemp "$SANDBOX/sout.XXXXXX")"; S_ERR="$(mktemp "$SANDBOX/serr.XXXXXX")"
printf '%s' "$(payload 'Dispatched sage-opus-q4 on the migration.')" \
    | RICHOS_ENTITY_ROOT="$ENTITY" RICHOS_CLAIMS_TEAMS_DIR="$TEAMS" \
      "$HOOK" >"$S_OUT" 2>"$S_ERR"
S_RC=$?
if [ "$S_RC" -ne 0 ]; then
    printf '  FAIL  s.explicit-opt-out-says-so (expected exit 0, got %s)\n' "$S_RC"; FAIL=$((FAIL + 1))
elif ! grep -qF 'STOOD DOWN' "$S_OUT"; then
    printf '  FAIL  s.explicit-opt-out-says-so (stdout carries no stand-down notice)\n'; FAIL=$((FAIL + 1))
elif ! grep -qF '"systemMessage"' "$S_OUT"; then
    printf '  FAIL  s.explicit-opt-out-says-so (stand-down is on stdout but not as systemMessage — the operator still cannot see it)\n'; FAIL=$((FAIL + 1))
elif ! python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if isinstance(d.get("systemMessage"),str) else 1)' "$S_OUT" 2>/dev/null; then
    printf '  FAIL  s.explicit-opt-out-says-so (stdout is not valid JSON with a string systemMessage)\n'; FAIL=$((FAIL + 1))
elif grep -qF 'STOOD DOWN' "$S_ERR"; then
    printf '  FAIL  s.explicit-opt-out-says-so (still announced on stderr — the invisible channel was duplicated, not abandoned)\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  s.explicit-opt-out-says-so\n'; PASS=$((PASS + 1))
fi

# s2. AND IT DOES NOT REPEAT. The condition has not changed, so the second turn
# of the same session says nothing — the state-change rule in
# scripts/lib/stop-hook-notice.sh. A notice printed under every turn is one the
# operator learns to skip, and a notice nobody reads is worth what the stderr
# one was worth.
printf '%s' "$(payload 'Dispatched sage-opus-q4 on the migration.')" \
    | RICHOS_ENTITY_ROOT="$ENTITY" RICHOS_CLAIMS_TEAMS_DIR="$TEAMS" \
      "$HOOK" >"$S_OUT" 2>/dev/null
if [ -s "$S_OUT" ]; then
    printf '  FAIL  s2.opt-out-does-not-repeat-while-unchanged (announced again: %s)\n' "$(head -c 120 "$S_OUT")"; FAIL=$((FAIL + 1))
else
    printf '  PASS  s2.opt-out-does-not-repeat-while-unchanged\n'; PASS=$((PASS + 1))
fi

# s3. THE NEGATIVE CONTROL for s2 — silence there must mean "unchanged", not
# "this hook never prints". Turn the guard back ON in the same session: the
# state changed, so the recovery IS announced.
: > "$ENTITY/orchestration.config"
printf '%s' "$(payload 'norm-opus-a1 landed the fix.')" \
    | RICHOS_ENTITY_ROOT="$ENTITY" RICHOS_CLAIMS_TEAMS_DIR="$TEAMS" \
      "$HOOK" >"$S_OUT" 2>/dev/null
if grep -qF 'RUNNING AGAIN' "$S_OUT"; then
    printf '  PASS  s3.recovery-is-announced-so-silence-means-unchanged\n'; PASS=$((PASS + 1))
else
    printf '  FAIL  s3.recovery-is-announced-so-silence-means-unchanged (nothing said when the guard came back: %s)\n' "$(head -c 120 "$S_OUT")"; FAIL=$((FAIL + 1))
fi

# s4. A HEALTHY GUARD IS SILENT. Having announced the recovery, the next
# ordinary turn says nothing at all. The requirement is that NOT running is
# visible, never that running is.
printf '%s' "$(payload 'norm-opus-a1 landed the fix.')" \
    | RICHOS_ENTITY_ROOT="$ENTITY" RICHOS_CLAIMS_TEAMS_DIR="$TEAMS" \
      "$HOOK" >"$S_OUT" 2>/dev/null
if [ -s "$S_OUT" ]; then
    printf '  FAIL  s4.healthy-guard-stays-quiet (printed: %s)\n' "$(head -c 120 "$S_OUT")"; FAIL=$((FAIL + 1))
else
    printf '  PASS  s4.healthy-guard-stays-quiet\n'; PASS=$((PASS + 1))
fi
rm -f "$S_OUT" "$S_ERR"
rm -rf "$ENTITY/.claude/state/stop-hook-notices"
: > "$ENTITY/orchestration.config"

run_case "t.unparseable-payload-passes" 0 'not json at all {{{'

run_case "u.empty-final-message-passes" 0 "$(payload '')"

run_case "v.missing-transcript-still-blocks" 2 \
    "$(payload 'Dispatched sage-opus-q4.' "$SANDBOX/no-such-transcript.jsonl")" \
    "never spawned"

# Broken install: the root contract is gone. A PreToolUse guard blocks here.
# This one must NOT, or a broken install makes every turn unendable.
BROKEN="$SANDBOX/broken-engine/scripts/hooks"
mkdir -p "$BROKEN"
cp "$HOOK" "$BROKEN/guard-unresolved-claims.sh"
cp "$SCRIPT_DIR/guard-unresolved-claims.py" "$BROKEN/guard-unresolved-claims.py"
err="$(mktemp "$SANDBOX/err.XXXXXX")"
# Fed from a FILE, not a pipe. This path bails out before it reads stdin, so a
# pipe would leave `printf` writing into a closed descriptor; under `pipefail`
# that EPIPE becomes the pipeline's status and the case fails for a reason that
# has nothing to do with the hook. (Every other case reaches `INPUT="$(cat)"`,
# drains stdin, and is unaffected.)
pay="$(mktemp "$SANDBOX/pay.XXXXXX")"
payload 'Dispatched sage-opus-q4.' > "$pay"
RICHOS_ENTITY_ROOT="$ENTITY" bash "$BROKEN/guard-unresolved-claims.sh" \
    <"$pay" >/dev/null 2>"$err"
rc=$?
rm -f "$pay"
if [ "$rc" -eq 0 ] && grep -qF "BROKEN INSTALL" "$err"; then
    printf '  PASS  w.broken-install-passes-loudly\n'; PASS=$((PASS + 1))
else
    printf '  FAIL  w.broken-install-passes-loudly (exit %s)\n' "$rc"; FAIL=$((FAIL + 1))
fi
rm -f "$err"

# --- RECORD ----------------------------------------------------------------# --- BLOCKING: a bare-role claim about a role never dispatched -------------
# The rule, and the reason it is the only half that blocks: the ledger only
# grows, so "no agent of this role has ever been spawned here" cannot become
# false later. Same ground truth, same monotonicity, same zero-FP argument as
# the agent-name check.
run_case "y1.bare-role-never-dispatched-blocks" 2 \
    "$(payload 'Zach is building it now, in the engine, alongside the three already running.')" \
    "named a ROLE rather than an agent"

# --- REPORTING: the role ran earlier and is not running now ----------------
# This is the 2026-08-31 failure's real shape and it does NOT block, because
# liveness shrinks and a guard whose ground truth shrinks cannot be trusted to
# refuse. Asserted here so the limit is a test rather than a paragraph.
run_case "y2.bare-role-spawned-earlier-but-not-live-REPORTS" 0 \
    "$(payload 'Norm is building it now.')" \
    "NOT currently running"

run_case "y3.bare-role-with-a-live-teammate-is-silent" 0 \
    "$(payload 'Mark is building it now.')"
if printf '%s' "$LAST_ERR" | grep -q 'NOT currently running'; then
    printf '  FAIL  y3b.a-live-teammate-produces-no-observation\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  y3b.a-live-teammate-produces-no-observation\n'; PASS=$((PASS + 1))
fi

# Naming the agent is what moves the claim into the blocking bucket that has
# always worked. The role check stands down; the name check takes over.
run_case "y4.naming-the-agent-hands-over-to-the-name-check" 2 \
    "$(payload 'zach-opus-uici1 is building it now.')" \
    "never spawned"

run_case "y5.an-Agent-call-this-turn-suppresses-the-role-check" 0 \
    "$(payload 'Zach is building it now.' "$AGENT_IN_TURN")"

# PRECISION: the role must be CAPITALIZED and the subject of a progressive
# verb, so ordinary English containing a role word never enters. Measured
# 103/103 correct extraction over 3,532 real messages.
run_case "y6.ordinary-nouns-that-happen-to-be-role-words-never-fire" 0 \
    "$(payload 'A watermark is showing and the art is drying on the wall.')"

# ...and the progressive verb is the other half of the same precision argument:
# naming a teammate is not claiming they are working right now.
run_case "y6b.naming-a-role-without-a-progressive-verb-is-not-a-claim" 0 \
    "$(payload 'Isaac owns the iOS port, and Zach owns the deploy scripts.')"

# An identifier in the message hands the sentence to the name check, ONCE.
# Two verdicts on one sentence is one too many, and the second would be the
# weaker one.
run_case "y4b.an-identifier-suppresses-the-role-check-entirely" 0 \
    "$(payload 'norm-opus-a1 is building it now — Norm is building it now.')"
if printf '%s' "$LAST_ERR" | grep -q 'NOT currently running'; then
    printf '  FAIL  y4c.the-role-check-really-did-stand-down\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  y4c.the-role-check-really-did-stand-down\n'; PASS=$((PASS + 1))
fi

# The rule proposed as a gate, kept as a report with its measured number.
run_case "y7.nameless-dispatch-is-reported-with-its-measured-number" 0 \
    "$(payload 'Dispatching it now.')" \
    "10.3% precision"

# --- BLOCKING: STATE CLAIMS -- "landed", "merged", "on main", "pushed" -----
#
# THE 2026-09-01 FAILURE, REPRODUCED. A merge chained behind another command in
# one Bash call; a PreToolUse guard refused the whole call; the merge never ran;
# the turn reported it as landed. Every identifier in that report resolved, so
# the existence check above passed it — and the CEO found out by opening the
# file and getting ERR_FILE_NOT_FOUND.
run_case "z1.landed-claim-about-a-branch-only-commit-blocks" 2 \
    "$(payload "round-11.2 is landed on main and pushed — \`$BRANCH_SHA\`.")" \
    "You said this LANDED"

# ...and the refusal has to NAME THE CONTRADICTING FACT, or it nags instead of
# teaching. The branch the commit is actually on is the whole content of the
# correction.
if printf '%s' "$LAST_ERR" | grep -qF "unlanded-branch"; then
    printf '  PASS  z1b.refusal-names-the-branch-it-is-actually-on\n'; PASS=$((PASS + 1))
else
    printf '  FAIL  z1b.refusal-names-the-branch-it-is-actually-on\n'; FAIL=$((FAIL + 1))
fi

# THE POSITIVE HALF OF THE CANARY, and it is not decoration. exit 2 is
# ambiguous — Layer K learned that the expensive way, over a scanner that
# refused everything because it never started. A gate that refuses a true
# landing claim as readily as a false one is satisfied by a corpse.
run_case "z2.a-TRUE-landing-claim-passes" 0 \
    "$(payload "The report is landed on main at \`$LIVE_SHA\`, tree clean.")"
if printf '%s' "$LAST_ERR" | grep -qF "You said this LANDED"; then
    printf '  FAIL  z2b.two-sided-canary-a-true-claim-was-refused-too\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  z2b.two-sided-canary-a-true-claim-was-refused-too\n'; PASS=$((PASS + 1))
fi

# THE RELAXATION THAT MAKES IT SHIPPABLE. A history rewrite strands the old
# commits with no ref pointing at them; citing one is honest and unprovable.
# Requiring reachability-from-a-ref took the corpus from 41 fires to 0, and
# every one of the 41 was a rewrite casualty.
run_case "z3.landed-claim-about-a-dangling-commit-is-SILENT" 0 \
    "$(payload "That work landed at \`$DANGLING_SHA\` before the rewrite.")"
if printf '%s' "$LAST_ERR" | grep -qF "You said this LANDED"; then
    printf '  FAIL  z3b.a-rewrite-casualty-must-not-fire\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  z3b.a-rewrite-casualty-must-not-fire\n'; PASS=$((PASS + 1))
fi

run_case "z4.pushed-claim-about-an-unpushed-commit-blocks" 2 \
    "$(payload "Everything is committed and pushed — \`$UNPUSHED_SHA\`.")" \
    "You said this was PUSHED"

run_case "z5.a-TRUE-push-claim-passes" 0 \
    "$(payload "Pushed at \`$LIVE_SHA\`.")"

# A bare hex token is admitted ONLY inside a sentence that has already asserted
# a landing — the context is what makes it a citation rather than a coincidence.
run_case "z6.a-bare-unbackticked-sha-in-a-landing-sentence-still-blocks" 2 \
    "$(payload "Reed's pass landed on main as $BRANCH_SHA and the worktree is gone.")" \
    "You said this LANDED"

# PRECISION: no landing word, no question asked of git. The same SHA that
# blocks above is silent here.
#
# AND IT MUST BE SILENT FOR THE RIGHT REASON (2026-09-06). This case is the
# named witness for the verb requirement in `state_claims()` — the check that
# makes a state claim a claim rather than any sentence containing a hex token —
# and until now it asserted nothing but `exit 0`. claim-roles.mutation.sh's
# `state-claim-needs-no-verb` mutant removes that requirement, and the failure
# mode is not a wider check: execution reaches
# `claim_polarity(s, (mi or mp).start())` with both matches None, the analyzer
# raises AttributeError, and guard-unresolved-claims.sh fails OPEN on an
# analyzer it cannot run. Exit 0 either way. The property had no witness, and
# the harness said so in its own header rather than claim a green tick.
#
# Measured on this payload, at 934f127:
#     clean analyzer    exit 0, stderr EMPTY
#     mutated analyzer  exit 1, "AttributeError: 'NoneType' object has no
#                       attribute 'start'" on stderr
# so the traceback is the stable, deterministic witness — unlike the
# reachability cases the harness header records as flaking 2 of 5 runs.
run_case "z7.the-same-sha-outside-a-state-claim-is-not-checked" 0 \
    "$(payload "The branch tip is \`$BRANCH_SHA\` and I am waiting on your call before touching it.")" \
    "" "Traceback (most recent call last)"

run_case "z8.a-landing-word-with-no-sha-is-not-a-state-claim" 0 \
    "$(payload 'Everything is landed and pushed; the worktrees are cleaned up.')"

# --- POLARITY: the gate blocked a TRUE statement, twice -------------------
#
# 2026-09-03: a reply said a commit was NOT on the remote, the repository
# agreed exactly, and the gate read the word `pushed`, ignored the negation and
# stopped the turn. 2026-09-05: the same shape again, on "committed on the
# branch, not landed, main is still at <sha>" -- a sentence whose entire content
# is that the commit had not landed.
#
# zp* and zq* ship as a PAIR, for the RO/RD reason. Teaching a gate to read
# "not" is one edit away from teaching it to ignore everything, so every case
# that must go quiet has a case beside it that must still refuse.
run_case "zp1.REGRESSION-a-negated-push-claim-about-an-unpushed-commit-is-NOT-blocked" 0 \
    "$(payload "Committed on the branch, not pushed — \`$UNPUSHED_SHA\`.")"
if printf '%s' "$LAST_ERR" | grep -qF "You said this was PUSHED"; then
    printf '  FAIL  zp1b.the-refusal-really-did-stand-down\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  zp1b.the-refusal-really-did-stand-down\n'; PASS=$((PASS + 1))
fi

run_case "zp2.REGRESSION-committed-on-the-branch-not-landed-is-NOT-blocked" 0 \
    "$(payload "Committed on the branch, not landed, main is untouched — \`$BRANCH_SHA\`.")"

run_case "zp3.have-not-landed-it-yet-is-NOT-blocked" 0 \
    "$(payload "The fix is committed at \`$BRANCH_SHA\`, but I haven't landed it yet.")"

run_case "zp4.nothing-landed-is-NOT-blocked" 0 \
    "$(payload "Nothing landed; main is untouched at \`$BRANCH_SHA\`.")"

# THE OTHER HALF. A negation sitting nearby must not silence a claim it does
# not govern -- that is how a polarity rule becomes an off switch.
run_case "zq1.a-negation-in-a-DIFFERENT-clause-still-blocks" 2 \
    "$(payload "No worktrees are left and no agents are running, and the work is landed on main at \`$BRANCH_SHA\`.")" \
    "You said this LANDED"

run_case "zq2.a-negation-AFTER-the-sentence-terminator-still-blocks" 2 \
    "$(payload "The catastrophe is gone, and it was never the model.** Landed \`$BRANCH_SHA\`.")" \
    "You said this LANDED"

run_case "zq3.not-only-is-an-emphatic-POSITIVE-and-still-blocks" 2 \
    "$(payload "It not only landed but was verified — \`$BRANCH_SHA\`.")" \
    "You said this LANDED"

run_case "zq4.an-ordinary-positive-landing-claim-still-blocks" 2 \
    "$(payload "Landed on main — \`$BRANCH_SHA\`.")" \
    "You said this LANDED"

# A negated claim the repository CONTRADICTS is still worth saying, and it is
# said as an observation. Understating what landed has never been the failure
# this gate exists for, so it never blocks.
run_case "zp5.a-negated-claim-the-repository-contradicts-is-REPORTED" 0 \
    "$(payload "Committed but not pushed — \`$LIVE_SHA\`.")" \
    "the repository says it HAS"

# --- REPORTING: a value claimed gone, alive in a spelling not named --------
# The third failure of 2026-09-01. It REPORTS and does not block, and the
# reason is a number: the general grep fires on 95 of 109 real literals, and
# the sharpened version is held back only by how rarely an absence word lands
# next to a value. See value_absence_claims() in the analyzer.
run_case "z9.a-value-gone-in-one-spelling-only-is-REPORTED" 0 \
    "$(payload 'Contrast fixed: `#8F7030` is gone from the entire app.')" \
    "survives in a spelling you did not name"

run_case "z10.a-value-with-no-surviving-spelling-is-silent" 0 \
    "$(payload 'Contrast fixed: `#123456` is gone from the entire app.')"
if printf '%s' "$LAST_ERR" | grep -qF "survives in a spelling"; then
    printf '  FAIL  z10b.a-genuinely-absent-value-must-not-be-reported\n'; FAIL=$((FAIL + 1))
else
    printf '  PASS  z10b.a-genuinely-absent-value-must-not-be-reported\n'; PASS=$((PASS + 1))
fi

LOG="$ENTITY/.claude/state/claim-checks.jsonl"
if [ -s "$LOG" ] && python3 - "$LOG" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
assert any(r["verdict"] == "block" for r in rows), "no block recorded"
assert any(r["verdict"] == "pass" for r in rows), "no pass recorded"
assert any(r.get("prose_signal") for r in rows), "prose signal never recorded"
assert any(r.get("undispatched_roles") for r in rows), "role block never recorded"
assert any(r.get("stale_roles") for r in rows), "stale role never recorded"
assert any(r["verdict"] == "block" and r.get("undispatched_roles") and
           not r["unresolved"]["name"] and not r["unresolved"]["sha"]
           for r in rows), "a role-only block was not recorded as a block"
assert any(r.get("bad_state_claims") for r in rows), "no state-claim block recorded"
assert any(r["verdict"] == "block" and r.get("bad_state_claims") and
           not r["unresolved"]["name"] and not r["unresolved"]["sha"]
           for r in rows), "a state-claim-only block was not recorded as a block"
assert any(r.get("value_absence") for r in rows), "value absence never recorded"
assert any(r.get("state_claims") and not r.get("bad_state_claims")
           for r in rows), "a passing state claim was never recorded"
PY
then
    printf '  PASS  x.observation-record-written\n'; PASS=$((PASS + 1))
else
    printf '  FAIL  x.observation-record-written\n'; FAIL=$((FAIL + 1))
fi


# --- THE MUTATION HARNESS -------------------------------------------------
# A reading rule has two failure directions that look identical from outside:
# it can stop understanding sentences, or it can understand every sentence as an
# excuse. Green ticks distinguish neither until somebody has watched them go red
# for each. Skipped inside a mutation sandbox so a mutant cannot run mutants.
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/guard-unresolved-claims.mutation.sh" ]; then
    echo
    echo "=== running the mutation harness ==="
    "$SCRIPT_DIR/guard-unresolved-claims.mutation.sh" || FAIL=$((FAIL + 1))
fi

echo
if [ "$FAIL" -eq 0 ]; then
    printf '✓ %s/%s passed\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '✗ %s/%s passed — %s FAILED\n' "$PASS" "$((PASS + FAIL))" "$FAIL"
exit 1
