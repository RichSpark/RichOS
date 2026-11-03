#!/usr/bin/env bash
#
# guard-ci-turn-gate.test.sh — THE TURN GATE BITES, AND IT LETS GO.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# A blocking Stop hook is the most dangerous object this engine ships: get it
# wrong in one direction and a red workflow ends turns for another sixteen
# days; get it wrong in the other and the founder's session cannot be ended at
# all. So both directions are proven here, end to end, through the REAL hook —
# not the analyzer in isolation — because what the founder is promised is that
# he can WATCH it refuse.
#
# The five directions the brief names, each with its own case:
#
#   1. a session that pushed to a repository whose tip run FAILED  -> REFUSED,
#      and the refusal names the repository, the workflow and the URL
#   2. the same with a GREEN tip                                   -> silent, 0
#   3. GitHub unreachable                                          -> allowed,
#      and it says why
#   4. a live `ci-red-ack:` with a real reason                     -> allowed
#      and logged; a BARE marker                                   -> refused
#   5. an endpoint that never answers                              -> control
#      returns INSIDE the budget and the turn is allowed
#
# ===========================================================================
# THE FIXTURE IS A REAL REPOSITORY AND A REAL TRANSCRIPT
# ===========================================================================
# Nothing here is stubbed except GitHub. The sandbox holds an actual git
# repository with an actual `origin` and an actual pushed commit, and the
# session's pushes are read from an actual JSONL transcript of the shape Claude
# Code writes — including the `cd <other-repo> && git push` form, measured from
# this project's own transcripts, where the record's `cwd` is a DIFFERENT
# repository from the one being pushed. A fixture that used the easy form would
# prove the gate works on a command shape this project does not use.
#
# Usage: scripts/hooks/guard-ci-turn-gate.test.sh
# Exit:  0 all cases passed, 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-ci-turn-gate.sh"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "$2"; FAIL=$((FAIL + 1)); }

command -v python3 >/dev/null 2>&1 || { echo "ERROR: needs python3" >&2; exit 1; }
command -v git >/dev/null 2>&1 || { echo "ERROR: needs git" >&2; exit 1; }

SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/ci-turn-gate.XXXXXX")"
trap 'rm -rf "$SANDBOX"' EXIT

echo "=== the CI turn gate: it bites, and it lets go ==="
echo ""

# ---------------------------------------------------------------------------
# THE FIXTURE
# ---------------------------------------------------------------------------
REPO="$SANDBOX/richos-hq"
ORIGIN="$SANDBOX/origin.git"
BIN="$SANDBOX/bin"
mkdir -p "$BIN"

git init --quiet --bare "$ORIGIN" >/dev/null 2>&1
git -c init.defaultBranch=main init --quiet "$REPO" >/dev/null 2>&1
# THE SANDBOX INHERITS NOTHING FROM THE OPERATOR'S MACHINE. This one was found
# by the suite rather than reasoned about: a global `core.hooksPath` on the
# machine this was written on installs an identity pre-commit hook, which
# rejected the fixture's commit, which left the branch unborn, which left the
# push with nothing to send — and every case downstream then "passed" or
# "failed" for a reason that had nothing to do with the gate. A fixture that
# reads the operator's `~/.gitconfig` is a fixture whose result is the
# operator's, not the code's.
mkdir -p "$SANDBOX/nohooks"
git -C "$REPO" config core.hooksPath "$SANDBOX/nohooks"
git -C "$REPO" config user.name "Fixture"
git -C "$REPO" config user.email "fixture@example.invalid"
git -C "$REPO" config commit.gpgsign false
# The adoption marker: a repository governs itself only if it declares so.
printf 'MODEL_CEILING="opus"\n' > "$REPO/orchestration.config"
git -C "$REPO" add -A >/dev/null 2>&1
git -C "$REPO" commit -qm init >/dev/null 2>&1
git -C "$REPO" branch -M main >/dev/null 2>&1
git -C "$REPO" remote add origin "$ORIGIN" >/dev/null 2>&1
PUSH_STARTED="$(date +%s)"
git -C "$REPO" push -q origin main >/dev/null 2>&1
# Now it looks like GitHub, which is what the slug is parsed from. The
# remote-tracking ref the push just wrote is untouched by this.
git -C "$REPO" remote set-url origin https://github.com/WebDevBooster/richos-hq.git
PUSHED_SHA="$(git -C "$REPO" rev-parse refs/remotes/origin/main 2>/dev/null)"

if [ -n "$PUSHED_SHA" ]; then
    ok "0a. fixture: a real repository, pushed, with origin/main at ${PUSHED_SHA:0:12}"
else
    bad "0a. fixture" "no refs/remotes/origin/main — every case below would prove nothing"
fi

# The stubbed GitHub. It answers with whatever is in $SANDBOX/gh.json, exits
# with whatever is in $SANDBOX/gh.rc, and sleeps for $SANDBOX/gh.sleep seconds
# first — which is how case 5 builds an endpoint that never answers.
cat > "$BIN/gh" <<'STUB'
#!/usr/bin/env bash
D="$(dirname "$(dirname "$0")")"
[ -f "$D/gh.sleep" ] && sleep "$(cat "$D/gh.sleep")"
[ -f "$D/gh.calls" ] && printf '%s\n' "$*" >> "$D/gh.calls"
RC=0
[ -f "$D/gh.rc" ] && RC="$(cat "$D/gh.rc")"
if [ "$RC" != "0" ]; then
    echo "gh: HTTP 000 could not resolve host: api.github.com" >&2
    exit "$RC"
fi
cat "$D/gh.json"
STUB
chmod 755 "$BIN/gh"
: > "$SANDBOX/gh.calls"
printf '0' > "$SANDBOX/gh.rc"

# runs_json <state> — the GitHub answer for the pushed commit.
runs_json() {
    python3 - "$1" > "$SANDBOX/gh.json" <<'PY'
import json, sys
state = sys.argv[1]
if state == "red":
    runs = [{"path": ".github/workflows/ui-suite-ci.yml", "name": "UI suite",
             "status": "completed", "conclusion": "failure", "run_number": 412,
             "id": 17705551234,
             "html_url": "https://github.com/WebDevBooster/richos-hq/actions/runs/17705551234"}]
elif state == "green":
    runs = [{"path": ".github/workflows/ui-suite-ci.yml", "name": "UI suite",
             "status": "completed", "conclusion": "success", "run_number": 413,
             "id": 17705559999,
             "html_url": "https://github.com/WebDevBooster/richos-hq/actions/runs/17705559999"}]
elif state == "running":
    runs = [{"path": ".github/workflows/ui-suite-ci.yml", "name": "UI suite",
             "status": "in_progress", "conclusion": None, "run_number": 414,
             "id": 17705558888,
             "html_url": "https://github.com/WebDevBooster/richos-hq/actions/runs/17705558888"}]
else:
    runs = []
print(json.dumps({"total_count": len(runs), "workflow_runs": runs}))
PY
}

# transcript <path> — one assistant turn that pushed richos-hq, in the shape
# this project's own lands actually take.
write_transcript() {
    python3 - "$1" "$REPO" "$SANDBOX" "$PUSH_STARTED" <<'PY'
import json, sys, time
path, repo, cwd = sys.argv[1], sys.argv[2], sys.argv[3]
rec = {
    "type": "assistant",
    "cwd": cwd,
    "sessionId": "feedface-0000-4000-8000-000000000001",
    "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(int(sys.argv[4]))),
    "message": {"role": "assistant", "content": [
        {"type": "text", "text": "landing"},
        {"type": "tool_use", "id": "toolu_01", "name": "Bash",
         "input": {"command": "cd %s && git merge --no-ff -q worktree-x && git push -q origin main && echo landed" % repo}},
    ]},
}
with open(path, "w", encoding="utf-8") as fh:
    fh.write(json.dumps(rec) + "\n")
    fh.write(json.dumps({"timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "message": {"content": [
        {"type": "tool_result", "tool_use_id": "toolu_01", "content": "ok"}]}}) + "\n")
PY
}

TRANSCRIPT="$SANDBOX/transcript.jsonl"
write_transcript "$TRANSCRIPT"

# payload <session> <last-assistant-message>
payload() {
    python3 - "$1" "$2" "$TRANSCRIPT" "$REPO" <<'PY'
import json, sys
print(json.dumps({
    "hook_event_name": "Stop",
    "session_id": sys.argv[1],
    "transcript_path": sys.argv[3],
    "cwd": sys.argv[4],
    "prompt_id": "deadbeef-0000-4000-8000-000000000000",
    "permission_mode": "default",
    "stop_hook_active": False,
    "last_assistant_message": sys.argv[2],
    "background_tasks": [],
    "session_crons": [],
}))
PY
}

# fire <case-tag> <session> <message> -> RC / OUT / ERR, with a fresh state dir
# so no case can be decided by another case's cache.
fire() {
    local tag="$1" sid="$2" msg="$3"
    local state="$SANDBOX/state-$tag"
    mkdir -p "$state"
    OUT="$(payload "$sid" "$msg" \
        | PATH="$BIN:$PATH" \
          RICHOS_ENTITY_ROOT="$REPO" \
          CI_TURN_GATE_STATE_DIR="$state" \
          CI_TURN_GATE_ACK_LOG="$SANDBOX/acks.log" \
          bash "$HOOK" 2>"$SANDBOX/err-$tag")"
    RC=$?
    ERR="$(cat "$SANDBOX/err-$tag")"
    return 0
}

# ===========================================================================
# 1. RED TIP -> THE TURN IS REFUSED, AND THE REFUSAL NAMES WHAT IS RED
# ===========================================================================
runs_json red
printf '0' > "$SANDBOX/gh.rc"
rm -f "$SANDBOX/gh.sleep"
fire red s-red "All done."

if [ "$RC" -eq 2 ]; then
    ok "1a. a push whose tip run FAILED refuses the turn (exit 2)"
else
    bad "1a. red refuses the turn" "expected exit 2, got $RC. stderr: $(printf '%s' "$ERR" | head -c 300)"
fi

MISSING=""
for needle in \
    "CI IS RED ON WHAT THIS SESSION PUSHED" \
    "WebDevBooster/richos-hq" \
    "ui-suite-ci.yml" \
    "https://github.com/WebDevBooster/richos-hq/actions/runs/17705551234" \
    "${PUSHED_SHA:0:12}" \
    "ci-red-ack:"
do
    case "$ERR" in *"$needle"*) : ;; *) MISSING="$MISSING [$needle]" ;; esac
done
if [ -z "$MISSING" ]; then
    ok "1b. the refusal names the repository, the workflow, the run URL, the commit and the way out"
else
    bad "1b. the refusal is self-evident" "not in the refusal:$MISSING"
fi

# 1c. IT SAYS WHAT CLEARS IT, IN THE IMPERATIVE. A refusal the reader cannot
# act on is the session-start banner again, with a worse temper.
case "$ERR" in
    *"WHAT CLEARS IT"*) ok "1c. the refusal states what clears it" ;;
    *) bad "1c. what clears it" "the refusal never says how to get out of it" ;;
esac

# ===========================================================================
# 2. GREEN TIP -> SILENT
# ===========================================================================
# A guard that says something on a clean surface is a guard people stop
# reading. This is the case that keeps it silent 99 turns out of 100.
runs_json green
fire green s-green "All done."
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "2a. a GREEN tip is silent and allows the turn (exit 0, nothing on stdout)"
else
    bad "2a. green is silent" "rc=$RC stdout=$(printf '%s' "$OUT" | head -c 200) stderr=$(printf '%s' "$ERR" | head -c 200)"
fi

# 2b. AND IT COST ONE API CALL, NOT MORE. The second turn-end reads the cache:
# a finished green commit cannot become red on its own, so that reading is
# final. This is the whole basis of the claim that the steady state is free.
CALLS_BEFORE="$(wc -l < "$SANDBOX/gh.calls" | tr -d ' ')"
OUT2="$(payload s-green "All done." \
    | PATH="$BIN:$PATH" RICHOS_ENTITY_ROOT="$REPO" \
      CI_TURN_GATE_STATE_DIR="$SANDBOX/state-green" \
      CI_TURN_GATE_ACK_LOG="$SANDBOX/acks.log" bash "$HOOK" 2>/dev/null)"
RC2=$?
CALLS_AFTER="$(wc -l < "$SANDBOX/gh.calls" | tr -d ' ')"
if [ "$RC2" -eq 0 ] && [ "$CALLS_AFTER" -eq "$CALLS_BEFORE" ]; then
    ok "2b. a second turn-end on the same green commit costs ZERO API calls (still $CALLS_AFTER)"
else
    bad "2b. green is cached forever" "rc=$RC2, calls went $CALLS_BEFORE -> $CALLS_AFTER"
fi

# ===========================================================================
# 3. UNREACHABLE GITHUB -> ALLOWED, AND IT SAYS WHY
# ===========================================================================
# "Could not look" is not evidence of red. A guard that blocks because it could
# not check is a guard that gets ripped out — and the fix on the day would be
# to waive it, which is how g11/g12/g13 died.
runs_json red
printf '7' > "$SANDBOX/gh.rc"
fire net s-net "All done."
printf '0' > "$SANDBOX/gh.rc"

if [ "$RC" -eq 0 ]; then
    ok "3a. an unreachable GitHub does NOT hold the turn (exit 0)"
else
    bad "3a. network failure never traps the session" "exit $RC — the turn was held on a fact nobody established"
fi
case "$OUT" in
    *"COULD NOT BE READ"*)
        ok "3b. and it says so on the operator's channel rather than passing in silence" ;;
    *)
        bad "3b. it says why" "stdout carried no explanation: $(printf '%s' "$OUT" | head -c 300)" ;;
esac

# ===========================================================================
# 4. THE ESCAPE HATCH — A REAL REASON PASSES, A BARE MARKER DOES NOT
# ===========================================================================
runs_json red

# 4a. BARE MARKER EXEMPTS NOTHING.
fire ackbare s-ackbare "Done.

ci-red-ack:"
if [ "$RC" -eq 2 ]; then
    ok "4a. a BARE \`ci-red-ack:\` exempts nothing — the turn is still refused"
else
    bad "4a. bare marker exempts nothing" "exit $RC — a marker with no target and no reason was accepted"
fi

# 4b. NOR DOES A PURE ASSERTION THAT IT IS FINE.
fire ackfine s-ackfine "Done.

ci-red-ack: richos-hq — known"
if [ "$RC" -eq 2 ]; then
    ok "4b. \`ci-red-ack: richos-hq — known\` is refused: an assertion is not a reason"
else
    bad "4b. an assertion is not a reason" "exit $RC — \"known\" was accepted as a reason"
fi

# 4c. A REAL REASON, NAMING SOMETHING THAT IS ACTUALLY RED, PASSES.
fire ackreal s-ackreal "Done.

ci-red-ack: richos-hq — the runner image was withdrawn by the vendor this morning, so this workflow cannot pass until they republish it; nothing in this session touched it"
if [ "$RC" -eq 0 ]; then
    ok "4c. an ack that names a red repository and gives a real reason allows the turn"
else
    bad "4c. a real ack allows the turn" "exit $RC — stderr: $(printf '%s' "$ERR" | head -c 300)"
fi

# 4d. AND IT IS LOGGED, because the anti-habit mechanism is the count.
if grep -q "WebDevBooster/richos-hq" "$SANDBOX/acks.log" 2>/dev/null; then
    ok "4d. the accepted ack is appended to the ack log ($SANDBOX/acks.log)"
else
    bad "4d. accepted acks are logged" "nothing in the ack log — an unlogged waiver cannot be counted back"
fi

# 4e. THE NEXT REFUSAL COUNTS IT BACK AT YOU.
fire ackcount s-ackcount "Done."
case "$ERR" in
    *"ack #2"*) ok "4e. the next refusal prints the repetition count (\"ack #2\")" ;;
    *) bad "4e. repetition is counted back" "the refusal did not name the ack count: $(printf '%s' "$ERR" | grep -c ack) ack line(s)" ;;
esac

# 4f. AN ACK NAMING SOMETHING THAT IS NOT RED COVERS NOTHING.
fire ackwrong s-ackwrong "Done.

ci-red-ack: some-other-repo — the vendor withdrew the runner image and it cannot pass until they republish"
if [ "$RC" -eq 2 ]; then
    ok "4f. an ack naming a repository that is not red covers nothing"
else
    bad "4f. an ack must name something red" "exit $RC"
fi

# ===========================================================================
# 5. THE BUDGET IS ENFORCED, NOT DESCRIBED
# ===========================================================================
# The founder asked how he could know this never costs him more than about a
# second per turn. The answer has to be a timeout with a test behind it, not a
# comment. GitHub here takes ten seconds to answer; the gate must return inside
# its budget and ALLOW.
BUDGET="$(grep -m1 '^BUDGET_SECONDS' "$SCRIPT_DIR/guard-ci-turn-gate.py" | sed 's/[^0-9.]//g')"
if [ -n "$BUDGET" ]; then
    ok "5a. the budget is a named constant a reader can find: BUDGET_SECONDS = $BUDGET"
else
    bad "5a. the budget is a named constant" "no BUDGET_SECONDS in guard-ci-turn-gate.py"
fi

runs_json red
printf '10' > "$SANDBOX/gh.sleep"
START="$(python3 -c 'import time; print(time.time())')"
fire budget s-budget "All done."
ELAPSED="$(python3 -c "import time; print('%.2f' % (time.time() - $START))")"
rm -f "$SANDBOX/gh.sleep"

if [ "$RC" -eq 0 ]; then
    ok "5b. an endpoint that never answers does NOT hold the turn (exit 0)"
else
    bad "5b. the budget allows on expiry" "exit $RC after ${ELAPSED}s — a check that cannot answer inside its budget held the turn"
fi

# The ceiling is the budget plus the fixed cost of starting bash, python3 and a
# handful of local `git` calls. 4s is deliberately loose: this asserts that the
# TIMEOUT FIRED, not that the machine is fast. Against a gate with no timeout
# the same case takes over ten seconds and fails.
WITHIN="$(python3 -c "print(1 if $ELAPSED < 4.0 else 0)")"
if [ "$WITHIN" = "1" ]; then
    ok "5c. it returned in ${ELAPSED}s against a 10s endpoint — the ${BUDGET}s budget is an enforced timeout"
else
    bad "5c. the budget is enforced" "took ${ELAPSED}s against a 10s endpoint; the timeout did not fire"
fi

case "$OUT" in
    *"budget"*) ok "5d. and it says the budget was the reason, rather than passing in silence" ;;
    *) bad "5d. the reason is stated" "stdout: $(printf '%s' "$OUT" | head -c 300)" ;;
esac

# ===========================================================================
# 6. THE CRUX — A RUN STILL IN PROGRESS HOLDS THE OBLIGATION, NOT THE TURN
# ===========================================================================
runs_json running
fire running s-running "All done."
if [ "$RC" -eq 0 ]; then
    ok "6a. a run still IN PROGRESS does not hold the turn (exit 0)"
else
    bad "6a. in-progress never traps the session" "exit $RC — the session would be held for the length of the run"
fi
case "$OUT" in
    *"STILL RUNNING"*"NOT HELD"*)
        ok "6b. and the allowance is announced, not silent — the absence of a finding is distinguishable from the absence of a check" ;;
    *)
        bad "6b. the allowance is announced" "stdout: $(printf '%s' "$OUT" | head -c 300)" ;;
esac

# 6c. THE OBLIGATION SURVIVES THE TURN. The same session, a later turn-end, the
# run having since failed: the turn is refused. This is the whole answer to
# "ignoring in-progress lets the turn end on a red that arrives two minutes
# later" — it costs ONE turn, not sixteen days.
runs_json red
OUT3="$(payload s-running "All done." \
    | PATH="$BIN:$PATH" RICHOS_ENTITY_ROOT="$REPO" \
      CI_TURN_GATE_STATE_DIR="$SANDBOX/state-running" \
      CI_TURN_GATE_ACK_LOG="$SANDBOX/acks.log" bash "$HOOK" 2>"$SANDBOX/err-running2")"
RC3=$?
if [ "$RC3" -eq 2 ] && grep -q "CI IS RED" "$SANDBOX/err-running2"; then
    ok "6c. when that run later FAILS, the next turn-end of the same session refuses"
else
    bad "6c. the obligation outlives the turn" "rc=$RC3 — the verdict that arrived late was never read"
fi

# ===========================================================================
# 7. THE PREDICATE IS NARROW — A SESSION THAT PUSHED NOTHING IS NEVER HELD
# ===========================================================================
# This is the clause that keeps the false-positive rate at zero for every
# session that is not landing anything, which is most of them.
printf '{"type":"assistant","cwd":"%s","message":{"role":"assistant","content":[{"type":"text","text":"no tools here"}]}}\n' \
    "$SANDBOX" > "$SANDBOX/empty.jsonl"
runs_json red
OUT4="$(python3 - s-nopush "$SANDBOX/empty.jsonl" "$REPO" <<'PY' | PATH="$BIN:$PATH" RICHOS_ENTITY_ROOT="$REPO" CI_TURN_GATE_STATE_DIR="$SANDBOX/state-nopush" CI_TURN_GATE_ACK_LOG="$SANDBOX/acks.log" bash "$HOOK" 2>"$SANDBOX/err-nopush"
import json, sys
print(json.dumps({"hook_event_name": "Stop", "session_id": sys.argv[1],
                  "transcript_path": sys.argv[2], "cwd": sys.argv[3],
                  "stop_hook_active": False, "last_assistant_message": "Done."}))
PY
)"
RC4=$?
if [ "$RC4" -eq 0 ]; then
    ok "7a. a session that pushed nothing is never held, however red the world is"
else
    bad "7a. no push, no hold" "exit $RC4 — stderr: $(head -c 300 "$SANDBOX/err-nopush")"
fi

# 7b. AND THE PUSH IS READ FROM THE TRANSCRIPT, NOT FROM A TYPED LIST.
PARSED="$(python3 - "$SCRIPT_DIR/guard-ci-turn-gate.py" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
cases = [
    ("cd /a/b && git push -q origin main", "/elsewhere", [("/a/b", "main")]),
    ("git -C /c/d push origin HEAD:main", "/elsewhere", [("/c/d", "main")]),
    ("git push -u origin cc/zach-opus-gate2", "/e/f", [("/e/f", "cc/zach-opus-gate2")]),
    ("git push origin --delete stale", "/g/h", []),
    ("git status && echo push", "/i/j", []),
    ("git commit -m 'push it'", "/k/l", []),
    # `git` only as an ARGUMENT is text, not a push
    ("printf '%s\\n' git push origin main", "/m/n", []),
    ("echo git push origin main", "/o/p", []),
    # ... while a wrapped or assigned command word still is one
    ("GIT_TRACE=1 git push origin main", "/q/r", [("/q/r", "main")]),
    ("env -i HOME=/x git push origin main", "/s/t", [("/s/t", "main")]),
    ("sudo -n git push origin HEAD:main", "/u/v", [("/u/v", "main")]),
]
bad = []
for cmd, cwd, want in cases:
    got = [(p["dir"], p["branch"]) for p in m.parse_pushes(cmd, cwd)]
    if got != want:
        bad.append("%r -> %r, wanted %r" % (cmd, got, want))
print("; ".join(bad))
PY
)"
if [ -z "$PARSED" ]; then
    ok "7b. the push parser reads \`cd\`, \`-C\`, refspecs and \`HEAD:main\`, and ignores deletions and the word \"push\" in prose"
else
    bad "7b. the push parser" "$PARSED"
fi

# ===========================================================================
# 8. AN ANSWER THAT CAN NEVER ARRIVE IS SAID ONCE, NOT EVERY TURN
# ===========================================================================
# This case is here because the real transcripts had it: one session pushed a
# shared branch from SIX scratchpad worktrees that were later removed. A gate
# that reported all six at every turn-end for the rest of the session teaches
# its reader to skip its output — which is the exact failure it exists to
# correct, rebuilt one level up.
GONE="$SANDBOX/gone-worktree"
mkdir -p "$GONE"
python3 - "$SANDBOX/gone.jsonl" "$GONE" "$SANDBOX" <<'PY'
import json, sys, time
print(json.dumps({"type": "assistant", "cwd": sys.argv[3],
                  "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                  "message": {"role": "assistant", "content": [
                      {"type": "tool_use", "id": "t1", "name": "Bash",
                       "input": {"command": "cd %s && git push -q origin dev/x" % sys.argv[2]}}]}}),
      file=open(sys.argv[1], "w"))
with open(sys.argv[1], "a") as fh:
    fh.write(json.dumps({"message": {"content": [
        {"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}}) + "\n")
PY
rm -rf "$GONE"          # the worktree is landed and removed, as they always are
gone_fire() {
    python3 - s-gone "$SANDBOX/gone.jsonl" "$REPO" <<'PY' | PATH="$BIN:$PATH" RICHOS_ENTITY_ROOT="$REPO" CI_TURN_GATE_STATE_DIR="$SANDBOX/state-gone" CI_TURN_GATE_ACK_LOG="$SANDBOX/acks.log" bash "$HOOK" 2>/dev/null
import json, sys
print(json.dumps({"hook_event_name": "Stop", "session_id": sys.argv[1],
                  "transcript_path": sys.argv[2], "cwd": sys.argv[3],
                  "stop_hook_active": False, "last_assistant_message": "Done."}))
PY
}
GONE1="$(gone_fire)"
GONE2="$(gone_fire)"
case "$GONE1" in
    *"no longer exists"*) ok "8a. a push from a worktree that has since been removed is announced once" ;;
    *) bad "8a. announced once" "first turn-end said nothing about it: $(printf '%s' "$GONE1" | head -c 200)" ;;
esac
if [ -z "$GONE2" ]; then
    ok "8b. and NOT again at the next turn-end — an answer that can never arrive is not repeated"
else
    bad "8b. not repeated" "it said it again: $(printf '%s' "$GONE2" | head -c 200)"
fi

# ===========================================================================
# 9. ZERO OBSERVED RUNS IS NEVER CALLED "RUNNING" — richos-hq, 2026-09-28
# ===========================================================================
# The founder hit this verbatim: a push to a repository with NO WORKFLOWS AT
# ALL (GitHub answers `workflow_runs: []` for every commit, forever, because
# there is nothing to run) used to be promoted to "CI IS STILL RUNNING ... (no
# runs have appeared yet)" for a grace window on the guess that GitHub just
# had not indexed the run yet. It never would have. The fix: zero runs is
# silent, exactly like a green commit — never "running" unless a run is
# actually observed in flight.
runs_json nocirepo
fire nocirepo s-nocirepo "All done."
if [ "$RC" -eq 0 ]; then
    ok "9a. a commit with ZERO observed runs does not hold the turn (exit 0)"
else
    bad "9a. zero runs never traps the session" "exit $RC — stderr: $(printf '%s' "$ERR" | head -c 300)"
fi
case "$OUT" in
    *"STILL RUNNING"*)
        bad "9b. zero runs is never announced as running" "stdout falsely claimed CI was running: $(printf '%s' "$OUT" | head -c 300)" ;;
    *)
        ok "9b. a repository with no CI at all (0 workflow runs, ever) prints no RUNNING notice" ;;
esac

# 9c. THE SAME MACHINERY STILL REPORTS A REAL IN-FLIGHT RUN. Case 6 above
# already proves this end to end; restated here as the paired negative/positive
# so the two cases cannot silently drift apart (a fix to 9b that also broke 6a
# would still show a regression right beside it).
runs_json running
fire nocirepo2 s-nocirepo2 "All done."
case "$OUT" in
    *"STILL RUNNING"*"NOT HELD"*)
        ok "9c. and a run that IS actually in flight is still announced as running" ;;
    *)
        bad "9c. a real in-flight run is still reported" "stdout: $(printf '%s' "$OUT" | head -c 300)" ;;
esac

# 9d. AND A FAILED RUN STILL HOLDS THE TURN, EXACTLY AS BEFORE — the fix
# touches only the "zero runs" branch, never the red branch case 1 covers.
runs_json red
fire nocirepo3 s-nocirepo3 "All done."
if [ "$RC" -eq 2 ]; then
    ok "9d. a push whose tip run FAILED still refuses the turn, unchanged by this fix"
else
    bad "9d. red still refuses" "expected exit 2, got $RC"
fi

# 10. THE HUNT ROWS P3-13, 15, 16, 17, 18, 34 — one unit test each, in
# guard-ci-turn-gate.rows.test.py (the parser, the transcript offset, the cache
# expiry, the target rotation and the timeout handling are functions, not
# something a transcript fixture reaches cheaply).
if python3 "$SCRIPT_DIR/guard-ci-turn-gate.rows.test.py" >"$SANDBOX/rows.out" 2>&1; then
    ok "10. the hunt-row unit tests (P3-13, 15, 16, 17, 18, 34) pass"
else
    bad "10. hunt-row unit tests" "$(tail -c 600 "$SANDBOX/rows.out")"
fi

echo ""
printf 'passed %d, failed %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
