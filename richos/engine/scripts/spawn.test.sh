#!/usr/bin/env bash
#
# spawn.test.sh — behavioral tests for scripts/spawn.sh, the one command that
# starts one teammate (docs/plans/worktree-spec-2026-09-11.md; the reason it
# exists is in scripts/lib/spawn.py's header).
#
# WHAT IS PINNED, AND WHY. HOME, CLAUDE_CONFIG_DIR, the workspace registry and
# the session identity all live in the sandbox, and the session is a process of
# this suite's own, so no case can touch the operator's record. The hook
# surfaces are pinned too (RICHOS_SPAWN_HOOK_SOURCES): the point of these cases
# is what spawn.sh does with a verdict, not which verdicts the machine's live
# CEO-todo state happens to produce today. The REAL guard-worktree-isolation.sh
# is in the pinned set, because clauses 4c and 7 are half of what is under test.
#
# Three cases are deliberately NOT hermetic (REAL SURFACES): they assert that
# the collector finds a guard from the engine's matcherless group AND one from a
# governed repository's settings.local.json, and that femcboost's brief-scope
# guard reports both of its rules at once. Reading only the engine's Agent group
# is the exact hole that cost two refusals on 2026-09-13, and a hermetic fixture
# cannot prove it is closed on this machine.
#
# TWO OF THOSE THREE NEED A WORKSTATION THAT NO CI RUNNER CAN BE, so each judges
# its OWN surface and SKIPS with its reason printed when that surface is absent
# — see the block itself for what that cost before it did. The engine-side one
# needs nothing but this repository and runs everywhere.
#
# Run directly: scripts/spawn.test.sh
# Exit 0 = every case that could run passed; exit 1 = at least one failure.
# A SKIP is neither: it is counted and listed separately, and never as a pass.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPAWN="$SCRIPT_DIR/spawn.sh"
WS_PY="$SCRIPT_DIR/../mega-lander/workspaces.py"

PASS=0
FAIL=0
SKIP=0
SKIP_LINES=()
SANDBOX="$(cd "$(mktemp -d -t spawn-test.XXXXXX)" && pwd -P)"
SESS_PID="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
# RICHOS_SPAWN_TEST_KEEP=1 leaves the sandbox on disk for a post-mortem.
if [ -n "${RICHOS_SPAWN_TEST_KEEP:-}" ]; then
    trap 'kill "$SESS_PID" 2>/dev/null; echo "  sandbox kept: $SANDBOX"' EXIT
else
    trap 'kill "$SESS_PID" 2>/dev/null; rm -rf "$SANDBOX"' EXIT
fi
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); shift; [ "$#" -gt 0 ] && printf '        %s\n' "$*"; }
# A CASE THAT CANNOT RUN HERE SAYS SO AND IS COUNTED SEPARATELY. It is never a
# PASS — a skip that reads as a pass is how a suite stops meaning anything —
# and the reason travels with it, on the line below and again in the summary.
skip() { printf '  SKIP  %s\n' "$1"; printf '        NOT RUN HERE: %s\n' "${2:-no reason given}"; \
         SKIP=$((SKIP + 1)); SKIP_LINES+=("$1 — ${2:-no reason given}"); return 0; }

[ -x "$SPAWN" ] || { echo "FATAL: $SPAWN missing or not executable" >&2; exit 1; }

export HOME="$SANDBOX/home" CLAUDE_CONFIG_DIR="$SANDBOX/home/.claude"
export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces"
export RICHOS_SESSION_PID="$SESS_PID" RICHOS_SESSION_ID="feedface-0000-4000-8000-0000000000a1"
export GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid \
       GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
mkdir -p "$CLAUDE_CONFIG_DIR"

mkrepo() {   # <path> [branch-to-end-on]
    local r="$1"
    mkdir -p "$r"
    git -C "$r" init -q -b main
    printf 'seed\n' >"$r/seed.txt"
    git -C "$r" add -A
    git -C "$r" commit -q -m seed
    [ "$#" -ge 2 ] && git -C "$r" checkout -q -b "$2"
    return 0
}

# The session's own repository. It carries the agent definition the model
# resolver reads and the settings.local.json that stands in for femcboost's.
PROJECT="$SANDBOX/session-repo"
mkrepo "$PROJECT"
mkdir -p "$PROJECT/.claude/agents"
cat >"$PROJECT/.claude/agents/zach.md" <<'DEF'
---
name: zach
description: infrastructure
model: sonnet
tools: Read, Write, Edit, Bash
---
Infrastructure engineer.
DEF
# THE SANDBOX REPOSITORY MUST ACTUALLY ADOPT THE ENGINE. resolve-roots.sh
# answers `not-adopted` for a repository with no orchestration.config at its
# root, and every guard then STANDS DOWN and exits 0 — correctly, because
# nothing has asked it to enforce anything there. Without this line the suite
# would run every case against guards that were all silently switched off, and
# would have reported them passing. (It did, on 2026-09-13, and the case that
# caught it was the one that checked what a FAILURE leaves behind.)
cp "$SCRIPT_DIR/../orchestration.config" "$PROJECT/orchestration.config"
git -C "$PROJECT" add orchestration.config
git -C "$PROJECT" commit -q -m "adopt the engine"
TARGET="$SANDBOX/widgets"
mkrepo "$TARGET"
python3 "$WS_PY" integration --repo "$PROJECT" --branch main --why "the session repo" >/dev/null

# --- the pinned hook surfaces ---------------------------------------------
# An engine file with a MATCHERLESS group (every tool) and an Agent group, and a
# repository-local file with one more Agent guard: the two surfaces, in the two
# shapes that matter.
GUARDS="$SANDBOX/guards"
mkdir -p "$GUARDS"
cat >"$GUARDS/always-ok.sh" <<'G'
#!/usr/bin/env bash
cat >/dev/null
exit 0
G
cat >"$GUARDS/refuse-a.sh" <<'G'
#!/usr/bin/env bash
payload="$(cat)"
if [ -n "${REFUSE_A:-}" ]; then echo "GUARD A REFUSES: reason alpha" >&2; exit 2; fi
exit 0
G
cat >"$GUARDS/refuse-b.sh" <<'G'
#!/usr/bin/env bash
payload="$(cat)"
if [ -n "${REFUSE_B:-}" ]; then echo "GUARD B REFUSES: reason beta" >&2; exit 2; fi
exit 0
G
# Fault injection for the rollback case: passes while the workspace is still
# only PLANNED, refuses once it exists. Nothing natural does that; the rollback
# path has to be reachable to be tested.
cat >"$GUARDS/refuse-after-create.sh" <<'G'
#!/usr/bin/env bash
payload="$(cat)"
[ -n "${REFUSE_AFTER:-}" ] || exit 0
if printf '%s' "$payload" | grep -q '"planned": \[\]'; then
  echo "GUARD REFUSES AFTER CREATION: injected fault" >&2
  exit 2
fi
exit 0
G
# Fault injection for the interrupt case: once the workspace exists, say so by
# touching a marker and hold still, so the test can interrupt a spawn that has
# created and registered its workspace but has not finished.
cat >"$GUARDS/stall-after-create.sh" <<'G'
#!/usr/bin/env bash
payload="$(cat)"
[ -n "${STALL_MARKER:-}" ] || exit 0
if printf '%s' "$payload" | grep -q '"planned": \[\]'; then
  : >"$STALL_MARKER"
  exec sleep 30
fi
exit 0
G
chmod +x "$GUARDS"/*.sh
# The engine under test may live under a folder with a space ("Application
# Support"), so its guard's path is quoted in the command, as a registration
# naming an absolute path must be; unquoted it is two words and never runs.
cat >"$GUARDS/engine-hooks.json" <<J
{"hooks": {"PreToolUse": [
  {"hooks": [{"type": "command", "command": "bash $GUARDS/always-ok.sh"}]},
  {"matcher": "Agent", "hooks": [
    {"type": "command", "command": "bash '$SCRIPT_DIR/hooks/guard-worktree-isolation.sh'"},
    {"type": "command", "command": "bash $GUARDS/refuse-a.sh"},
    {"type": "command", "command": "bash $GUARDS/refuse-after-create.sh"},
    {"type": "command", "command": "bash $GUARDS/stall-after-create.sh"}]},
  {"matcher": "Bash", "hooks": [{"type": "command", "command": "bash $GUARDS/never-runs.sh"}]}
]}}
J
mkdir -p "$PROJECT/.claude"
cat >"$PROJECT/.claude/settings.local.json" <<J
{"hooks": {"PreToolUse": [
  {"matcher": "Agent", "hooks": [{"type": "command", "command": "bash $GUARDS/refuse-b.sh"}]}
]}}
J
export RICHOS_SPAWN_HOOK_SOURCES="engine=$GUARDS/engine-hooks.json:local=$PROJECT/.claude/settings.local.json"

BRIEF="$SANDBOX/brief.md"
cat >"$BRIEF" <<'B'
ceo-todos-deferred: the suite's own fixture; no CEO item is being skipped by a test.

Fix the widget health check. Scope: health.js only.
Verify with `scripts/hooks/contract-integrity.test.sh --only base`.
B

run() {   # <spawn args...> -> stdout+stderr in OUT, status in RC
    OUT="$("$SPAWN" "$@" --project-dir "$PROJECT" 2>&1)"
    RC=$?
    return 0
}

echo "spawn.sh"

# ---------------------------------------------------------------------------
echo "  repository resolution"
# ---------------------------------------------------------------------------
run "zach-sonnet-r1" --repo widgets --type zach --brief "$BRIEF" --dry-run
case "$OUT" in
    *"$TARGET"*) ok "a bare repository name resolves (the old helper answered \"'richos' is not a directory\")" ;;
    *) bad "a bare repository name resolves" "$OUT" ;;
esac

run "zach-sonnet-r2" --repo nosuchrepo --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "no repository of that name is known here" \
   && printf '%s' "$OUT" | grep -q "widgets"; then
    ok "an unknown bare name is refused WITH the candidates"
else
    bad "an unknown bare name is refused with the candidates" "rc=$RC $OUT"
fi

mkrepo "$SANDBOX/other/widgets"
python3 "$WS_PY" integration --repo "$SANDBOX/other/widgets" --branch main --why "the twin" >/dev/null
run "zach-sonnet-r3" --repo widgets --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "is ambiguous"; then
    ok "an ambiguous bare name is refused and both candidates are named"
else
    bad "an ambiguous bare name is refused" "rc=$RC $OUT"
fi
rm -rf "$SANDBOX/other"

run "zach-sonnet-r4" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
[ "$RC" -eq 0 ] && ok "an absolute path resolves" || bad "an absolute path resolves" "rc=$RC $OUT"

# ---------------------------------------------------------------------------
echo "  the shared teammate-name shape (scripts/lib/teammate-name.sh, 2026-09-17)"
# ---------------------------------------------------------------------------
# BEFORE the fix: guard-worktree-isolation.sh's own shape check had no length
# bound, so 'zach-sonnet-multirepodemo1' (a 14-character identifier) passed
# every guard in this table and was refused only later, inside
# create-teammate-worktree.sh — one round trip after spawn.sh's own guard
# table said "passed". Both readers now source the same rule
# (scripts/lib/teammate-name.sh), so the guard refuses it HERE, in this table,
# before anything is registered — proving spawn.sh's own claim that every
# refusal is found before anything is created, for THIS rule specifically.
run "zach-sonnet-multirepodemo1" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -qE "refused by 1 of [0-9]+ guard\(s\) - NOTHING WAS CREATED" \
   && printf '%s' "$OUT" | grep -qF "'zach-sonnet-multirepodemo1' is not a teammate name of the form <role>-<model>-<identifier>"; then
    ok "an over-length identifier (14 chars) is refused BEFORE creation, in the guard table, in the creator's exact wording"
else
    bad "the creator's stricter name shape is refused up front" "rc=$RC $OUT"
fi
[ ! -d "$SANDBOX/widgets-wt/zach-sonnet-multirepodemo1" ] \
    && ok "and nothing was created for it" \
    || bad "nothing was created for the refused name" "the workspace exists"
run "zach-sonnet-shapeok1" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
[ "$RC" -eq 0 ] && ok "POSITIVE CONTROL: a name within the shared shape (12-char identifier) still passes" \
                || bad "the positive control name passes" "rc=$RC $OUT"

# ---------------------------------------------------------------------------
echo "  the branch this work integrates on (point 14)"
# ---------------------------------------------------------------------------
# A repository nothing has recorded yet. (By now $TARGET HAS a record, written
# by the first case above — which is itself the behavior under test, so it needs
# a repository that has not been through it.)
FRESH="$SANDBOX/sprockets"
mkrepo "$FRESH"
run "zach-sonnet-r4b" --repo "$FRESH" --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "RECORDED main for $FRESH"; then
    ok "an ABSENT record is RECORDED, not refused with a second command to run"
else
    bad "an absent record is recorded" "rc=$RC $OUT"
fi
if printf '%s' "$OUT" | grep -q "If this work must not reach main yet"; then
    ok "and the recording SAYS SO, with the one line that would change it"
else
    bad "the recording is announced" "$OUT"
fi
WORK_ID="$(python3 "$WS_PY" integration --repo "$TARGET" | awk '{print $5}')"
[ -n "$WORK_ID" ] && ok "the recording names a body of work ($WORK_ID)" \
                  || bad "the recording names a body of work" "none"

# STALE: the recorded branch is deleted under it, as dev/workspace-spec was.
git -C "$TARGET" branch -f dev/old main
python3 "$WS_PY" integration --repo "$TARGET" --branch dev/old --correct --why "kept off main" >/dev/null
git -C "$TARGET" branch -D dev/old >/dev/null 2>&1
if python3 "$WS_PY" integration-branch --repo "$TARGET" >/dev/null 2>&1; then
    bad "NEGATIVE CONTROL: a stale record should abstain before the repair" ""
else
    ok "NEGATIVE CONTROL: with the stale record every consumer abstains"
fi
run "zach-sonnet-r5" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "CORRECTED $TARGET from dev/old (no such branch) to main"; then
    ok "a STALE record is CORRECTED in place"
else
    bad "a stale record is corrected" "rc=$RC $OUT"
fi
AFTER_ID="$(python3 "$WS_PY" integration --repo "$TARGET" | awk '{print $5}')"
[ "$AFTER_ID" = "$WORK_ID" ] \
    && ok "the correction KEEPS the body of work's id ($AFTER_ID), so agents bound to it move with it" \
    || bad "the correction keeps the work id" "was $WORK_ID now $AFTER_ID"

# THE HUMAN DECISION: no record, and the checkout is not on the default branch.
DEV="$SANDBOX/gadgets"
mkrepo "$DEV" dev/rework
run "zach-sonnet-r6" --repo "$DEV" --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "DECISION:" \
   && printf '%s' "$OUT" | grep -q "not on its default branch"; then
    ok "where the answer is a human's, it says WHICH decision and stops (it never guesses one)"
else
    bad "the human decision is named" "rc=$RC $OUT"
fi
run "zach-sonnet-r7" --repo "$DEV" --type zach --brief "$BRIEF" \
    --integration dev/rework --integration-why "the gadget rework" --dry-run
[ "$RC" -eq 0 ] && ok "--integration answers it and the run proceeds" \
                || bad "--integration answers it" "rc=$RC $OUT"

# ---------------------------------------------------------------------------
echo "  every guard, from every surface, all failures together"
# ---------------------------------------------------------------------------
run "zach-sonnet-g1" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
if printf '%s' "$OUT" | grep -q "always-ok.sh"; then
    ok "a MATCHERLESS PreToolUse group is collected (it matches every tool, Agent included)"
else
    bad "a matcherless group is collected" "$OUT"
fi
if printf '%s' "$OUT" | grep -q "refuse-b.sh"; then
    ok "the repository's OWN settings.local.json is collected (femcboost's surface)"
else
    bad "the repository's settings.local.json is collected" "$OUT"
fi
if printf '%s' "$OUT" | grep -q "never-runs.sh"; then
    bad "a Bash-matched group must NOT be collected for an Agent call" "$OUT"
else
    ok "a group matched to another tool is NOT collected"
fi

REFUSE_A=1 REFUSE_B=1 run "zach-sonnet-g2" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] \
   && printf '%s' "$OUT" | grep -q "reason alpha" && printf '%s' "$OUT" | grep -q "reason beta"; then
    ok "TWO guards refusing are BOTH named in ONE run (never one refusal per attempt)"
else
    bad "both refusals are reported together" "rc=$RC $OUT"
fi

# ---------------------------------------------------------------------------
echo "  create, and leave nothing behind on failure"
# ---------------------------------------------------------------------------
REFUSE_A=1 run "zach-sonnet-c1" --repo "$TARGET" --type zach --brief "$BRIEF"
WT="$SANDBOX/widgets-wt/zach-sonnet-c1"
if [ "$RC" -eq 1 ] && [ ! -e "$WT" ] && [ ! -e "$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--zach-sonnet-c1.json" ] \
   && ! git -C "$TARGET" rev-parse --verify --quiet refs/heads/cc/zach-sonnet-c1 >/dev/null; then
    ok "a refusal BEFORE creation leaves no workspace, no branch and no registration"
else
    bad "a pre-create refusal leaves nothing" "rc=$RC wt=$( [ -e "$WT" ] && echo present || echo absent)"
fi

run "zach-sonnet-c2" --repo "$TARGET" --type zach --brief "$BRIEF"
WT2="$SANDBOX/widgets-wt/zach-sonnet-c2"
if [ "$RC" -eq 0 ] && [ -d "$WT2" ]; then
    ok "a clean run creates the workspace and exits 0"
else
    bad "a clean run creates the workspace" "rc=$RC $OUT"
fi
N="$(git -C "$TARGET" worktree list --porcelain | grep -c "^worktree $WT2\$")"
[ "$N" = "1" ] && ok "the workspace exists EXACTLY once" || bad "the workspace exists exactly once" "found $N"
if printf '%s' "$OUT" | grep -q '"isolation": "worktree"' \
   && printf '%s' "$OUT" | grep -q "cross-repo-worktree: $WT2" \
   && printf '%s' "$OUT" | grep -q "git merge main"; then
    ok "the printed payload carries isolation, the registered workspace and the merge-main handover line"
else
    bad "the printed payload is complete" "$OUT"
fi

# 2026-10-14: --integration <repo>=v1.3.0 for one teammate CORRECTED the whole
# repository's body of work and moved every running main-bound teammate with
# it, and the appended last step still said to merge main into the release.
git -C "$TARGET" branch rel "$(git -C "$TARGET" commit-tree -p main -m "rel only" "main^{tree}")"
run "zach-sonnet-rel1" --repo "$TARGET" --type zach --brief "$BRIEF" --integration "$TARGET=rel"
RELWT="$SANDBOX/widgets-wt/zach-sonnet-rel1"
RELCHECK="$(python3 - "$RICHOS_WORKSPACES_DIR" "$TARGET" <<'PY'
import glob, json, os, sys
d, target = sys.argv[1], os.path.realpath(sys.argv[2])
integ = json.load(open(os.path.join(d, "integration.json")))
cur = integ["works"][integ["current"][target]]["branch"]
rec = [json.load(open(p)) for p in glob.glob(os.path.join(d, "agents", "*zach-sonnet-rel1.json"))][0]
mine = integ["works"][rec["integration_work"][target]]["branch"]
print("%s %s" % (cur, mine))
PY
)"
if [ "$RC" -eq 0 ] && [ "$RELCHECK" = "main rel" ]; then
    ok "--integration for ONE teammate binds that teammate alone; the repository stays on main"
else
    bad "--integration binds the teammate alone" "rc=$RC check='$RELCHECK' $OUT"
fi
[ "$(git -C "$RELWT" rev-parse HEAD 2>/dev/null)" = "$(git -C "$TARGET" rev-parse rel)" ] \
    && ok "and its workspace starts from that branch, not from main" \
    || bad "the workspace starts from its own branch" "$(git -C "$RELWT" log --oneline -1 2>&1)"
if printf '%s' "$OUT" | grep -qF "\`git merge rel\` in $TARGET" \
   && ! printf '%s' "$OUT" | grep -qF "\`git merge main\` in $TARGET"; then
    ok "and its last step merges rel in that repository, never main"
else
    bad "the last step names the teammate's own branch" "$OUT"
fi

# Review rv-20261014T105153Z-f22eff3f-da5a, P1: a repository whose RECORDED
# branch is already the release one, its main checkout on main. spawn.sh passed
# no --base, so the workspace started at HEAD (main), and landing it into the
# release would have pulled main in.
GADGETS="$SANDBOX/gadgets"
mkrepo "$GADGETS"
git -C "$GADGETS" branch rel "$(git -C "$GADGETS" commit-tree -p main -m "rel only" "main^{tree}")"
python3 "$WS_PY" integration --repo "$GADGETS" --branch rel --why "the release" >/dev/null
for case in "zach-sonnet-relrec1:" "zach-sonnet-relrec2:$GADGETS=rel"; do
    name="${case%%:*}"; integ="${case#*:}"
    if [ -n "$integ" ]; then
        run "$name" --repo "$GADGETS" --type zach --brief "$BRIEF" --integration "$integ"
    else
        run "$name" --repo "$GADGETS" --type zach --brief "$BRIEF"
    fi
    got="$(git -C "$SANDBOX/gadgets-wt/$name" rev-parse HEAD 2>/dev/null)"
    if [ "$RC" -eq 0 ] && [ "$got" = "$(git -C "$GADGETS" rev-parse rel)" ]; then
        ok "a workspace in a repository recorded on rel starts from rel (${integ:-no --integration})"
    else
        bad "the workspace starts from the recorded branch, not from the main checkout's HEAD" \
            "rc=$RC head=$got rel=$(git -C "$GADGETS" rev-parse rel) $OUT"
    fi
done

# --integration naming another branch for the session's OWN repository (a native
# workspace only, which cannot start from or carry a branch of its own) is
# refused before anything is created, and the refusal says to start that
# teammate from a session in another repository, never to move everybody.
git -C "$PROJECT" branch rel "$(git -C "$PROJECT" commit-tree -p main -m "rel only" "main^{tree}")"
run "zach-sonnet-relown1" --repo "$PROJECT" --type zach --brief "$BRIEF" --integration "$PROJECT=rel"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -qF "from a session in another repository" \
   && ! printf '%s' "$OUT" | grep -qF -- "--correct" \
   && ! ls "$RICHOS_WORKSPACES_DIR"/agents/*zach-sonnet-relown1.json >/dev/null 2>&1; then
    ok "--integration for the session's own repository is refused: start that teammate from another repository's session"
else
    bad "a release target in the session's own repository is refused with the other-session remedy" "rc=$RC $OUT"
fi

# Review rv-20261014T120915Z-3bf392e2-75cb, P1: the session's OWN repository
# already RECORDED on the release branch, its main checkout on main. The early
# "recorded and live" returns accepted it, so a native teammate started from
# main and its land pulled main into the release. Refused the same way, with
# and without --integration naming that recorded branch.
python3 "$WS_PY" integration --repo "$PROJECT" --branch rel --correct --why "the release" >/dev/null
for case in "zach-sonnet-relown2:" "zach-sonnet-relown3:$PROJECT=rel"; do
    name="${case%%:*}"; integ="${case#*:}"
    if [ -n "$integ" ]; then
        run "$name" --repo "$PROJECT" --type zach --brief "$BRIEF" --integration "$integ"
    else
        run "$name" --repo "$PROJECT" --type zach --brief "$BRIEF"
    fi
    if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -qF "from a session in another repository" \
       && ! ls "$RICHOS_WORKSPACES_DIR"/agents/*"$name".json >/dev/null 2>&1; then
        ok "the session's own repository recorded on rel is refused for a native teammate (${integ:-no --integration})"
    else
        bad "a recorded release target in the session's own repository is refused" "rc=$RC $OUT"
    fi
done
python3 "$WS_PY" integration --repo "$PROJECT" --branch main --correct --why "the session repo" >/dev/null

run "zach-sonnet-c2" --repo "$TARGET" --type zach --brief "$BRIEF"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -qi "already"; then
    ok "a REUSED name is refused before anything is created"
else
    bad "a reused name is refused" "rc=$RC $OUT"
fi
[ -d "$WT2" ] && ok "and the refusal did not touch the workspace that already owned the name" \
              || bad "the refused reuse left the existing workspace alone" "$OUT"

REFUSE_AFTER=1 run "zach-sonnet-c3" --repo "$TARGET" --type zach --brief "$BRIEF"
WT3="$SANDBOX/widgets-wt/zach-sonnet-c3"
if [ "$RC" -eq 4 ] && printf '%s' "$OUT" | grep -q "rolled back"; then
    ok "FAULT INJECTION: a refusal AFTER creation rolls the workspace back and says so"
else
    bad "a post-create refusal rolls back" "rc=$RC $OUT"
fi
if [ ! -e "$WT3" ] && ! git -C "$TARGET" rev-parse --verify --quiet refs/heads/cc/zach-sonnet-c3 >/dev/null \
   && [ ! -e "$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--zach-sonnet-c3.json" ]; then
    ok "the rollback leaves no workspace, no branch and no registration"
else
    bad "the rollback leaves nothing" "wt=$( [ -e "$WT3" ] && echo present || echo absent)"
fi
run "zach-sonnet-c3" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
[ "$RC" -eq 0 ] && ok "and the name is FREE again, so the retry is the same command" \
                || bad "the name is free after a rollback" "rc=$RC $OUT"

# AN INTERRUPT AFTER CREATION is the same failure as a refusal after creation
# (Rich stopped a running spawn on 2026-10-06 and it left a WORKING row and a branch).
for SIG in TERM INT HUP; do
    IDX=$(( ${IDX:-0} + 1 ))
    N="zach-sonnet-i$IDX"
    MARK="$SANDBOX/stalled-$SIG"
    rm -f "$MARK"
    STALL_MARKER="$MARK" "$SPAWN" "$N" --repo "$TARGET" --type zach --brief "$BRIEF" \
        --project-dir "$PROJECT" >"$SANDBOX/int-$SIG.out" 2>&1 &
    SPID=$!
    for _ in $(seq 1 100); do [ -e "$MARK" ] && break; sleep 0.2; done
    if [ ! -e "$MARK" ]; then
        kill -KILL "$SPID" 2>/dev/null; wait "$SPID" 2>/dev/null
        bad "SIG$SIG: the spawn reached the point after creation" "$(cat "$SANDBOX/int-$SIG.out")"
        continue
    fi
    REG="$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--$N.json"
    PRE_REG=absent; [ -e "$REG" ] && PRE_REG=present
    kill -"$SIG" "$SPID"
    wait "$SPID"; IRC=$?
    if [ "$PRE_REG" = present ] && [ "$IRC" -eq 4 ] && [ ! -e "$SANDBOX/widgets-wt/$N" ] \
       && ! git -C "$TARGET" rev-parse --verify --quiet "refs/heads/cc/$N" >/dev/null \
       && [ ! -e "$REG" ]; then
        ok "SIG$SIG after creation rolls back: no workspace, no branch, no registration (exit 4)"
    else
        bad "SIG$SIG after creation rolls back" "rc=$IRC before=$PRE_REG $(cat "$SANDBOX/int-$SIG.out")"
    fi
done

# ---------------------------------------------------------------------------
echo "  several repositories, ONE teammate (point 10)"
# ---------------------------------------------------------------------------
# "All of an agent's workspaces go together. An agent working in another
# repository has two... every workspace and branch it has is deleted, as one."
# The rule is "every workspace it has"; two is the example. Until 2026-09-17
# --repo was taken once, so a teammate needing a second repository was given a
# SECOND NAME — which made it a second agent, landed separately.
SECOND="$SANDBOX/gizmos"
mkrepo "$SECOND"
run "zach-sonnet-m1" --repo "$TARGET" --repo "$SECOND" --type zach --brief "$BRIEF" --dry-run
M1A="$SANDBOX/widgets-wt/zach-sonnet-m1"
M1B="$SANDBOX/gizmos-wt/zach-sonnet-m1"
if [ "$RC" -eq 0 ] \
   && printf '%s' "$OUT" | grep -q "cross-repo-worktree: $M1A" \
   && printf '%s' "$OUT" | grep -q "cross-repo-worktree: $M1B"; then
    ok "two --repo produce TWO cross-repo-worktree: lines in one payload, under the one name"
else
    bad "two --repo produce two marker lines" "rc=$RC $OUT"
fi
if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q "REFUSED"; then
    ok "and EVERY guard passes that payload — clause 4c reads every marker line, and always has"
else
    bad "every guard passes a two-workspace payload" "rc=$RC $OUT"
fi

run "zach-sonnet-m2" --repo "$TARGET" --repo "$SECOND" --type zach --brief "$BRIEF"
M2A="$SANDBOX/widgets-wt/zach-sonnet-m2"
M2B="$SANDBOX/gizmos-wt/zach-sonnet-m2"
if [ "$RC" -eq 0 ] && [ -d "$M2A" ] && [ -d "$M2B" ] \
   && [ "$(git -C "$M2A" symbolic-ref -q --short HEAD)" = "cc/zach-sonnet-m2" ] \
   && [ "$(git -C "$M2B" symbolic-ref -q --short HEAD)" = "cc/zach-sonnet-m2" ]; then
    ok "a clean run CREATES both workspaces, each on cc/<name> in its own repository"
else
    bad "both workspaces are created" "rc=$RC $OUT"
fi
if python3 - "$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--zach-sonnet-m2.json" "$M2A" "$M2B" <<'PY' 2>/dev/null
import json, os, sys
r = json.load(open(sys.argv[1]))
live = [w for w in r["workspaces"] if not w.get("deleted_at")]
assert sorted(os.path.realpath(w["path"]) for w in live) == sorted(sys.argv[2:4]), live
assert all(w["kind"] == "cc" and w["created"] is True for w in live), live
PY
then ok "and BOTH are registered on the ONE agent's record, so they go together at the land"
else bad "both are on one record" "$(cat "$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--zach-sonnet-m2.json" 2>/dev/null | tr '\n' ' ' | cut -c1-400)"; fi

run "zach-sonnet-m3" --repo "$TARGET" --repo widgets --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "already this teammate's repository"; then
    ok "NEGATIVE: the SAME repository twice under one name is refused (one workspace per repository)"
else
    bad "the same repository twice is refused" "rc=$RC $OUT"
fi

run "zach-sonnet-m4" --repo "$TARGET" --repo "$SECOND" --type zach --brief "$BRIEF" \
    --dir "$SANDBOX/anywhere/m4" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q -- "--dir <repo>=<value>"; then
    ok "NEGATIVE: an UNSCOPED --dir with several repositories is refused, naming the form that answers it"
else
    bad "an unscoped --dir is refused" "rc=$RC $OUT"
fi
run "zach-sonnet-m5" --repo "$TARGET" --repo "$SECOND" --type zach --brief "$BRIEF" \
    --dir "$SECOND=$SANDBOX/anywhere/m5" --dry-run
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "cross-repo-worktree: $SANDBOX/anywhere/m5" \
   && printf '%s' "$OUT" | grep -q "cross-repo-worktree: $SANDBOX/widgets-wt/zach-sonnet-m5"; then
    ok "POSITIVE CONTROL: the SCOPED form --dir <repo>=<path> is honored, and the other repository keeps its default"
else
    bad "the scoped --dir is honored" "rc=$RC $OUT"
fi

REFUSE_AFTER=1 run "zach-sonnet-m6" --repo "$TARGET" --repo "$SECOND" --type zach --brief "$BRIEF"
M6A="$SANDBOX/widgets-wt/zach-sonnet-m6"
M6B="$SANDBOX/gizmos-wt/zach-sonnet-m6"
if [ "$RC" -eq 4 ] && [ ! -e "$M6A" ] && [ ! -e "$M6B" ] \
   && ! git -C "$TARGET" rev-parse --verify --quiet refs/heads/cc/zach-sonnet-m6 >/dev/null \
   && ! git -C "$SECOND" rev-parse --verify --quiet refs/heads/cc/zach-sonnet-m6 >/dev/null \
   && [ ! -e "$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--zach-sonnet-m6.json" ]; then
    ok "FAULT INJECTION: a refusal after creation rolls back BOTH workspaces and both branches — nothing is left behind in either repository"
else
    bad "the rollback covers every workspace" "rc=$RC a=$( [ -e "$M6A" ] && echo present || echo absent) b=$( [ -e "$M6B" ] && echo present || echo absent)"
fi

# ---------------------------------------------------------------------------
echo "  the dry evaluation is the same code with the write removed"
# ---------------------------------------------------------------------------
ENV_JSON="$SANDBOX/env.json"
python3 - "$ENV_JSON" "$RICHOS_SESSION_ID" "$SANDBOX/widgets-wt/zach-sonnet-d1" "$TARGET" "$PROJECT" <<'PY'
import json, sys
out, sid, path, repo, cwd = sys.argv[1:6]
json.dump({"session_id": sid, "cwd": cwd, "hook_event_name": "PreToolUse", "tool_name": "Agent",
           "tool_input": {"name": "zach-sonnet-d1", "subagent_type": "zach", "isolation": "worktree",
                          "prompt": "cross-repo-worktree: %s\n\ndo the thing" % path},
           "richos_spawn_check": {"planned": [{"path": path, "repo": repo,
                                               "branch": "cc/zach-sonnet-d1"}]}},
          open(out, "w"))
PY
if python3 "$WS_PY" check-spawn <"$ENV_JSON" >/dev/null 2>&1; then
    ok "check-spawn evaluates a payload the platform has not minted a tool_use_id for"
else
    bad "check-spawn evaluates an unsent payload" "$(python3 "$WS_PY" check-spawn <"$ENV_JSON" 2>&1)"
fi
if python3 "$WS_PY" register-spawn <"$ENV_JSON" >/dev/null 2>&1; then
    bad "NEGATIVE CONTROL: register-spawn must still refuse the same payload" ""
else
    ok "NEGATIVE CONTROL: register-spawn still refuses it — that was the pre-flight's false positive"
fi
if [ -e "$RICHOS_WORKSPACES_DIR/agents/$RICHOS_SESSION_ID--zach-sonnet-d1.json" ]; then
    bad "check-spawn wrote a registration" "it must write nothing"
else
    ok "check-spawn wrote NOTHING: no record, no binding, no event"
fi
python3 - "$ENV_JSON" "$SANDBOX/live.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["tool_use_id"] = "toolu_live_0001"
json.dump(d, open(sys.argv[2], "w"))
PY
if python3 "$WS_PY" check-spawn <"$SANDBOX/live.json" >/dev/null 2>&1; then
    bad "check-spawn accepted a payload carrying a tool_use_id" "a live call must go to register-spawn"
else
    ok "a payload WITH a tool_use_id is refused by check-spawn: the marker cannot dry-run a live call"
fi

# ---------------------------------------------------------------------------
echo "  --audience app: the RichOS app's own dispatch"
# ---------------------------------------------------------------------------
# WHAT THESE PROVE, and the order matters: the operator case is asserted FIRST
# against the same refusing guard, so a green app case cannot be a green for
# some unrelated reason. On 2026-09-18 an app dispatch on this machine was
# refused by guard-owned-state.sh over the DEVELOPMENT session's paused CI, in
# the development session's words, with the front desk holding neither of that
# guard's two ways out. A user cannot dispatch somebody at a paused CI and
# cannot add an acknowledgement line to a brief he never sees.
AUD="$SANDBOX/audience.declaration"
cat >"$AUD" <<'D'
id: always-ok.sh
audience: user-work
reason: fixture
user_message: The fixture's always-ok guard refused. Nothing was created.

id: guard-worktree-isolation.sh
audience: user-work
reason: fixture
user_message: I could not give this work its own separate copy of your project. Nothing was created.

id: refuse-after-create.sh
audience: user-work
reason: fixture
user_message: The fixture's after-create guard refused. Nothing was created.

id: refuse-a.sh
audience: operator-session
reason: fixture, and its reason is the development session's

id: refuse-b.sh
audience: operator-session
reason: fixture, and its reason is the development session's
D

# 1. THE CASE THIS WHOLE SLICE IS: an operator-session guard refusing does NOT
#    refuse the app. Proven in both audiences on ONE refusing guard.
REFUSE_A=1 run "zach-sonnet-aud1" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "GUARD A REFUSES"; then
    ok "operator audience: a refusing guard refuses the dispatch (the control for the next case)"
else
    bad "operator audience: a refusing guard refuses" "rc=$RC $OUT"
fi

REFUSE_A=1 RICHOS_SPAWN_GUARD_AUDIENCE="$AUD" run "zach-sonnet-aud2" --repo "$TARGET" \
    --type zach --brief "$BRIEF" --dry-run --audience app
if [ "$RC" -eq 0 ]; then
    ok "app audience: an operator-session guard's refusal does NOT refuse the app"
else
    bad "app audience: an operator-session guard's refusal does not refuse the app" "rc=$RC $OUT"
fi

# 2. AND IT IS NOT RUN AT ALL, not merely ignored. A guard that runs and is
#    overruled still costs the user its runtime and still writes whatever it
#    writes.
if printf '%s' "$OUT" | grep -q "GUARD A REFUSES"; then
    bad "app audience: the operator-session guard was not even run" "its output is in the report"
else
    ok "app audience: the operator-session guard is not run at all, not run-and-ignored"
fi
if printf '%s' "$OUT" | grep -q "not run (its reason is the development session's"; then
    ok "app audience: what was left out is NAMED, never dropped quietly"
else
    bad "app audience: what was left out is named" "$OUT"
fi

# 3. A REFUSAL A USER CAN STILL SEE IS IN THE APP'S OWN WORDS. The user-work
#    guard here is the real guard-worktree-isolation.sh, refusing on a real
#    defect (a bare name), so this is not a fixture talking to itself.
RICHOS_SPAWN_GUARD_AUDIENCE="$AUD" run "bare" --repo "$TARGET" --type zach \
    --brief "$BRIEF" --dry-run --audience app
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" \
       | grep -q "I could not give this work its own separate copy of your project"; then
    ok "app audience: a user-work guard's refusal reaches the caller in the app's own words"
else
    bad "app audience: a user-work refusal is in the app's own words" "rc=$RC $OUT"
fi
APP_OUT="$OUT"
# THE POSITIVE PROBE FOR THE FIVE CASES BELOW. A list of "this string is absent"
# checks passes on an empty string, so the SAME refusal is taken in operator
# audience first and every one of those strings must be PRESENT in it. Without
# this, five green cases would prove only that something went wrong.
#
# IT EARNED ITS KEEP THE FIRST TIME IT RAN. The list began with six strings,
# and "cross-repo-worktree" is not in THIS refusal's operator text at all, so
# that case was asserting the absence of something that was never there. It is
# gone; the five that remain are all present in the operator report.
run "bare" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run
OPERATOR_OUT="$OUT"
MISSING=""
for LEAK in "isolation" "REFUSES this spawn" "guards evaluated" \
            "NOTHING WAS CREATED" "problem(s)"; do
    printf '%s' "$OPERATOR_OUT" | grep -qF "$LEAK" || MISSING="$MISSING $LEAK"
done
if [ -z "$MISSING" ]; then
    ok "the five absent-string cases below can go red: the operator report contains all five"
else
    bad "the five absent-string cases below can go red" "operator report is missing:$MISSING"
fi

for LEAK in "isolation" "REFUSES this spawn" "guards evaluated" \
            "NOTHING WAS CREATED" "problem(s)"; do
    if printf '%s' "$APP_OUT" | grep -qF "$LEAK"; then
        bad "app audience: the refusal carries no operator text ($LEAK)" "$APP_OUT"
    else
        ok "app audience: the refusal carries no operator text ($LEAK)"
    fi
done

# 4. AN UNDECLARED GUARD IS NOT RUN FOR THE APP, and is named. refuse-b.sh is
#    removed from the declaration for this one case only.
cat >"$AUD.partial" <<'D'
id: always-ok.sh
audience: user-work
reason: fixture
user_message: The fixture's always-ok guard refused. Nothing was created.

id: guard-worktree-isolation.sh
audience: user-work
reason: fixture
user_message: I could not give this work its own separate copy of your project. Nothing was created.
D
REFUSE_B=1 RICHOS_SPAWN_GUARD_AUDIENCE="$AUD.partial" run "zach-sonnet-aud4" --repo "$TARGET" \
    --type zach --brief "$BRIEF" --dry-run --audience app
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "not run (nobody has classified it): refuse-b.sh"; then
    ok "app audience: an undeclared guard is not run, and is named as not run"
else
    bad "app audience: an undeclared guard is not run and is named" "rc=$RC $OUT"
fi

# 5. AN EMPTY USER-WORK LIST IS A REFUSAL, NEVER A SILENT PASS. "An unguarded
#    spawn is not a verified one" holds whoever the dispatch is for.
cat >"$AUD.none" <<'D'
id: always-ok.sh
audience: operator-session
reason: fixture

id: guard-worktree-isolation.sh
audience: operator-session
reason: fixture

id: refuse-a.sh
audience: operator-session
reason: fixture

id: refuse-b.sh
audience: operator-session
reason: fixture

id: refuse-after-create.sh
audience: operator-session
reason: fixture
D
RICHOS_SPAWN_GUARD_AUDIENCE="$AUD.none" run "zach-sonnet-aud5" --repo "$TARGET" \
    --type zach --brief "$BRIEF" --dry-run --audience app
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "no guard of this audience"; then
    ok "app audience: an empty user-work list refuses, rather than passing unguarded"
else
    bad "app audience: an empty user-work list refuses" "rc=$RC $OUT"
fi

# 6. AN UNREADABLE DECLARATION IS A REFUSAL, NEVER AN EMPTY ALLOWLIST.
RICHOS_SPAWN_GUARD_AUDIENCE="$SANDBOX/no-such.declaration" run "zach-sonnet-aud6" \
    --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run --audience app
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "classification could not be read"; then
    ok "app audience: an unreadable classification refuses, rather than allowing everything"
else
    bad "app audience: an unreadable classification refuses" "rc=$RC $OUT"
fi

# 7. THE DEFAULT IS UNCHANGED. Every case above this section ran without
#    --audience and is the proof; this one states it.
run "zach-sonnet-aud7" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run --audience nonsense
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "audience must be"; then
    ok "an unknown --audience is refused by name"
else
    bad "an unknown --audience is refused by name" "rc=$RC $OUT"
fi

# ---------------------------------------------------------------------------
echo "  REAL SURFACES (not hermetic, on purpose)"
# ---------------------------------------------------------------------------
# THREE ASSERTIONS, THREE DIFFERENT SURFACES, AND EACH ONE ANSWERS FOR ITSELF.
#
# Until 2026-09-14 all three sat inside ONE `if` on
# $REAL_PROJECT/.claude/settings.local.json, so on a GitHub runner — where that
# governed checkout cannot exist — the whole block collapsed to a single line,
# "FAIL REAL-SURFACES could not run" (run 34787626046, shard 5/12, ubuntu-24.04,
# 29 passed 1 failed; the same commit is 32/32 on the operator's machine, and
# `RICHOS_SPAWN_TEST_REAL_PROJECT=/tmp/definitely-not-here` reproduces the
# runner's output here byte for byte). Nothing about spawn.sh was wrong. A red
# that only means "this runner is not that workstation" is a red nobody can act
# on, and it teaches everyone to read past the next one.
#
# The fix is NOT to drop the assertions. One of the three needs no workstation
# at all, and the two that do are exactly the ones that must print WHY they did
# not run:
#
#   1. the engine's own hooks/hooks.json — ships IN this repository, so it runs
#      everywhere, runner included, and stays load-bearing in CI. Reading only
#      the Agent-matched group and missing the matcherless one is the hole that
#      cost two refusals on 2026-09-13; this is the case that keeps it closed.
#   2. a governed repository's .claude/settings.local.json — a property of a
#      workstation. Synthesizing one proves nothing the hermetic refuse-b.sh
#      case above has not already proved against a fixture.
#   3. femcboost's guard-brief-verification-scope.sh — the same.
#
# A case that cannot run here SKIPs, with its reason on the line under it and
# repeated in the summary, and is never counted as a pass. Same discipline as
# the workflow's `ci-skip:` declaration and the probe's BY-REFERENCE layers:
# an undeclared skip is a finding.
REAL_PROJECT="${RICHOS_SPAWN_TEST_REAL_PROJECT:-/Users/alex/ab/femcboost}"
ENGINE_HOOKS="$SCRIPT_DIR/../hooks/hooks.json"

collected() {   # <project-dir> -> "<label>\t<guard-file>" per collected guard
    env -u RICHOS_SPAWN_HOOK_SOURCES python3 - "$SCRIPT_DIR/lib/spawn.py" "$1" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("spawnlib", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for label, _src, cmd in m.collect_guards(sys.argv[2]):
    print("%s\t%s" % (label, m.guard_name(cmd)))
PY
}

# 1. THE ENGINE'S MATCHERLESS GROUP, from the real hooks.json that ships here.
#    The project directory is irrelevant to this one — the engine source does
#    not depend on it — so any real directory will do, and the engine's own is
#    the one that always exists.
PROBE_PROJECT="$REAL_PROJECT"
[ -d "$PROBE_PROJECT" ] || PROBE_PROJECT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
if [ -f "$ENGINE_HOOKS" ]; then
    FOUND="$(collected "$PROBE_PROJECT")"
    if printf '%s' "$FOUND" | grep -q '^engine.*guard-sealed-worktree.sh$'; then
        ok "the engine's MATCHERLESS guard is collected from the real hooks.json"
    else
        bad "the engine's matcherless guard is collected" "$FOUND"
    fi
else
    # NOT a skip: hooks.json is part of this repository, so its absence is a
    # broken engine, not a machine that lacks a surface.
    bad "the engine's matcherless guard is collected" "no $ENGINE_HOOKS — the engine is incomplete"
fi

# 2. A GOVERNED REPOSITORY'S OWN settings.local.json (femcboost's surface).
#    The precondition is read STRAIGHT OUT OF THE FILE, never through
#    collect_guards — asking the code under test whether there is anything to
#    find would make the assertion vacuous.
LOCAL_SETTINGS="$REAL_PROJECT/.claude/settings.local.json"
if [ -f "$LOCAL_SETTINGS" ] && python3 - "$LOCAL_SETTINGS" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(1)
pre = (d.get("hooks") or {}).get("PreToolUse")
if not isinstance(pre, list):
    pre = d.get("PreToolUse") if isinstance(d.get("PreToolUse"), list) else []
def agentish(g):
    m = g.get("matcher")
    if m is None:
        return True
    m = str(m).strip()
    return m in ("", "*") or "Agent" in m
sys.exit(0 if any(isinstance(g, dict) and agentish(g) and (g.get("hooks") or []) for g in pre) else 1)
PY
then
    FOUND_LOCAL="$(collected "$REAL_PROJECT")"
    if printf '%s' "$FOUND_LOCAL" | grep -q '^local'; then
        ok "a guard registered only in the repository's settings.local.json is collected (the hole that cost two refusals)"
    else
        bad "the repository's own guard is collected" "$FOUND_LOCAL"
    fi
else
    skip "a guard registered only in the repository's settings.local.json is collected" \
         "no governed repository at $REAL_PROJECT registering a PreToolUse[Agent] hook in .claude/settings.local.json — that file is a workstation's, and no runner has one. Point RICHOS_SPAWN_TEST_REAL_PROJECT at one to run it. The fixture case above proves the same collector hermetically; this is the one that proves it against a real surface, and it DID NOT RUN."
fi

# 3. A PREMISE THAT TURNED OUT TO BE FALSE, pinned so it stops being repeated.
# The brief for this work said guard-brief-verification-scope.sh "names only
# one of the two rules it enforces", offered as the reason one brief cost two
# refusals on 2026-09-13. It does not: it accumulates every finding and
# prints them as a list. The real cost was that it only ran at dispatch
# time, so every refusal from any guard arrived one per round trip.
# Escalation esc-20260913T221928Z-42f5cb1d.
SCOPE_GUARD="$REAL_PROJECT/scripts/hooks/guard-brief-verification-scope.sh"
if [ -f "$SCOPE_GUARD" ]; then
    TWO_RULE_MSG="$(python3 - <<'PY' | (cd "$REAL_PROJECT" && bash "$SCOPE_GUARD" 2>&1 >/dev/null)
import json
print(json.dumps({"tool_name": "Agent", "tool_input": {
    "name": "zach-sonnet-two1", "subagent_type": "zach", "isolation": "worktree",
    "prompt": "Verify with scripts/run-all-tests.sh and confirm a complete verification pass is green."}}))
PY
)"
    if printf '%s' "$TWO_RULE_MSG" | grep -q 'run-all-tests.sh' \
       && printf '%s' "$TWO_RULE_MSG" | grep -q 'full suite/pass in prose'; then
        ok "the brief-scope guard names BOTH broken rules in ONE message (the brief's premise that it names one was false)"
    else
        bad "the brief-scope guard names both rules at once" "$TWO_RULE_MSG"
    fi
else
    skip "the brief-scope guard names BOTH broken rules in ONE message" \
         "no $SCOPE_GUARD on this machine — that guard lives in the governed repository, not in this one, so no runner can execute it. It DID NOT RUN."
fi

# ---------------------------------------------------------------------------
echo "  the QA toolkit's index rides with a QA dispatch (scripts/lib/qa-toolkit.py)"
# ---------------------------------------------------------------------------
# The CEO, 2026-09-20: "what else must be done to ensure the QA toolkit
# actually gets used?" A QA-type teammate's payload carries the toolkit
# README's OWN table, read from the governed repository at spawn time. The
# TYPE decides — never the brief's prose, never his words.
#
# HERMETIC ON PURPOSE: the sandbox grows its own toolkit at the declared path
# inside the target repository, which `locate()` prefers over the engine's real
# sibling tree. A case that asserted the nine real tools would fail the day a
# tenth landed, which is the opposite of what this mechanism is for.
mkdir -p "$PROJECT/.claude/agents"
cat >"$PROJECT/.claude/agents/ray.md" <<'DEF'
---
name: ray
description: functional QA
model: sonnet
tools: Read, Write, Edit, Bash
---
Functional QA.
DEF
QA_KIT="$TARGET/richos/app/scripts/qa"
mkdir -p "$QA_KIT"
cat >"$QA_KIT/README.md" <<'KIT'
# `scripts/qa/` — the walk toolkit

## The rule

**A QA brief names the tool from this directory. A walker who needs a helper
that is not here ADDS it here and commits it.**

## The tools

| Tool | The job |
|---|---|
| `contrast.py` | WCAG ratio of two colors, or of a region of a frame. |
| `wait-for.sh` | Wait for a ref, a log line, a file. Exit 1 on timeout. |

## Adding a tool

One job per file.
KIT
git -C "$TARGET" add -A
git -C "$TARGET" commit -q -m "the walk toolkit"
KIT_SHA="$(git -C "$TARGET" log -1 --format=%h -- richos/app/scripts/qa)"

PAYLOAD_QA="$SANDBOX/payload-qa.json"
run "ray-sonnet-kit1" --repo "$TARGET" --type ray --brief "$BRIEF" --dry-run \
    --payload-out "$PAYLOAD_QA"
QA_PROMPT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["prompt"])' "$PAYLOAD_QA" 2>/dev/null)"
if [ "$RC" -eq 0 ] && printf '%s' "$QA_PROMPT" | grep -q '^## Your committed tools' \
   && printf '%s' "$QA_PROMPT" | grep -q '| `contrast.py` |' \
   && printf '%s' "$QA_PROMPT" | grep -q '| `wait-for.sh` |'; then
    ok "a QA type's payload carries the toolkit table"
else
    bad "a QA type's payload carries the toolkit table" "rc=$RC prompt=$QA_PROMPT"
fi

# The README's rule sentence, in the README's words. The table alone says what
# the tools are and not what the walker is being asked to do with them.
if printf '%s' "$QA_PROMPT" | grep -q 'ADDS it here and commits it'; then
    ok "the README's own rule sentence travels with the table"
else
    bad "the README's own rule sentence travels with the table" "$QA_PROMPT"
fi

# IDENTITY, NOT A CLAIM OF FRESHNESS (docs/freshness-contract.md): the commit
# the table was read at is in the payload, so a stale attachment is detectable
# rather than merely unlikely.
if [ -n "$KIT_SHA" ] && printf '%s' "$QA_PROMPT" | grep -q "$KIT_SHA"; then
    ok "the toolkit's commit is named in the payload"
else
    bad "the toolkit's commit is named in the payload" "sha=$KIT_SHA prompt=$QA_PROMPT"
fi

# A TENTH TOOL APPEARS WITHOUT ANYBODY EDITING THE ENGINE. This is the whole
# reason the table is read at spawn time instead of copied into a brief.
python3 - "$QA_KIT/README.md" <<'PY'
import sys
p = sys.argv[1]
t = open(p, encoding="utf-8").read()
t = t.replace("| `wait-for.sh` | Wait for a ref, a log line, a file. Exit 1 on timeout. |",
              "| `wait-for.sh` | Wait for a ref, a log line, a file. Exit 1 on timeout. |\n"
              "| `settle.py` | The tenth tool, added after the first dispatch. |")
open(p, "w", encoding="utf-8").write(t)
PY
git -C "$TARGET" add -A
git -C "$TARGET" commit -q -m "a tenth tool"
run "ray-sonnet-kit2" --repo "$TARGET" --type ray --brief "$BRIEF" --dry-run \
    --payload-out "$SANDBOX/payload-qa2.json"
QA_PROMPT2="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["prompt"])' "$SANDBOX/payload-qa2.json" 2>/dev/null)"
if printf '%s' "$QA_PROMPT2" | grep -q '`settle.py`'; then
    ok "a tool added to the README is in the next dispatch, with no engine edit"
else
    bad "a tool added to the README is in the next dispatch" "$QA_PROMPT2"
fi

# NON-QA TYPES GET NOTHING, byte for byte. An engineer, an architect and a
# researcher are not walkers, and a section in every payload is noise that
# teaches everyone to skip the end of the brief.
PAYLOAD_NONQA="$SANDBOX/payload-nonqa.json"
run "zach-sonnet-kit3" --repo "$TARGET" --type zach --brief "$BRIEF" --dry-run \
    --payload-out "$PAYLOAD_NONQA"
NONQA_PROMPT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["prompt"])' "$PAYLOAD_NONQA" 2>/dev/null)"
if [ "$RC" -eq 0 ] && ! printf '%s' "$NONQA_PROMPT" | grep -q 'Your committed tools' \
   && ! printf '%s' "$NONQA_PROMPT" | grep -q 'contrast.py'; then
    ok "a non-QA type's payload carries none of it"
else
    bad "a non-QA type's payload carries none of it" "rc=$RC prompt=$NONQA_PROMPT"
fi

# IT CANNOT FLIP A GATE THAT THE BRIEF DID NOT ALREADY TRIP. The data-contract
# gate's LOCAL_APP_CONTEXT_RE half is the one thing a QA-type dispatch has no
# other protection from (QA_ROLE_AGENTS satisfies its evidence half by role
# alone), so appended text carrying one of those tokens would refuse briefs
# that were fine before. The regex is READ from the declaration, never retyped.
LAC_RE="$(sed -n "s/^LOCAL_APP_CONTEXT_RE='\(.*\)'$/\1/p" "$SCRIPT_DIR/../orchestration.config" | head -1)"
QA_ONLY="$(python3 - "$PAYLOAD_QA" "$BRIEF" <<'PY'
import json, sys
prompt = json.load(open(sys.argv[1]))["prompt"]
brief = open(sys.argv[2], encoding="utf-8").read()
i = prompt.find("## Your committed tools")
sys.stdout.write(prompt[i:] if i >= 0 else "")
PY
)"
if [ -z "$LAC_RE" ]; then
    bad "the appended section introduces no data-contract trigger" "could not read LOCAL_APP_CONTEXT_RE"
elif printf '%s' "$QA_ONLY" | grep -qE "$LAC_RE"; then
    bad "the appended section introduces no data-contract trigger" \
        "the section matches $LAC_RE and would refuse briefs that passed before"
else
    ok "the appended section introduces no data-contract trigger"
fi

# A BRIEF THAT ALREADY CARRIES THE SECTION IS LEFT ALONE — no second copy.
BRIEF_OWN="$SANDBOX/brief-own-tools.md"
{ cat "$BRIEF"; printf '\n## Your committed tools\n\nthe brief wrote its own.\n'; } >"$BRIEF_OWN"
run "ray-sonnet-kit4" --repo "$TARGET" --type ray --brief "$BRIEF_OWN" --dry-run \
    --payload-out "$SANDBOX/payload-own.json"
OWN_PROMPT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["prompt"])' "$SANDBOX/payload-own.json" 2>/dev/null)"
OWN_N="$(printf '%s\n' "$OWN_PROMPT" | grep -c 'Your committed tools' || true)"
if [ "$OWN_N" = "1" ]; then
    ok "a brief that already has the section is not given a second one"
else
    bad "a brief that already has the section is not given a second one" "n=$OWN_N"
fi

# A TOOLKIT IT CANNOT FIND IS A NOTE, NEVER A DEAD SPAWN. Nothing about
# starting a walker should depend on a README being where it was last week.
QA_TOOLKIT_DIR="nowhere/at/all" run "ray-sonnet-kit5" --repo "$TARGET" --type ray \
    --brief "$BRIEF" --dry-run --payload-out "$SANDBOX/payload-gone.json"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "NOT ATTACHED"; then
    ok "a toolkit that cannot be found is reported and the spawn still goes"
else
    bad "a toolkit that cannot be found is reported and the spawn still goes" "rc=$RC $OUT"
fi

# THE ENGINE IS LOADED THROUGH A SYMLINK (`~/.claude/richos-engine` ->
# <repo>/richos/engine), and os.path.abspath does not follow one. Measured
# before the fix: through a symlinked engine the sibling-tree fallback
# resolved to the HOME directory and found no toolkit at all, so a QA dispatch
# into a repository that is not the toolkit's got an empty section and a note.
# The repositories-first path hid it, which is why this case loads the module
# the way the plugin does rather than the way the suite does.
SYMLINKED="$SANDBOX/engine-link"
ln -sfn "$SCRIPT_DIR/.." "$SYMLINKED"
LOCATED="$(python3 - "$SYMLINKED/scripts/lib/qa-toolkit.py" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("qt", sys.argv[1])
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
print(m.locate(["/nonexistent-repository"])[0] or "")
PY
)"
if [ -n "$LOCATED" ] && [ -f "$LOCATED" ]; then
    ok "the toolkit is found through a SYMLINKED engine (the plugin's own layout)"
else
    bad "the toolkit is found through a symlinked engine" "located='$LOCATED'"
fi

echo ""
if [ "$SKIP" -gt 0 ]; then
    echo "  NOT RUN HERE — these are SKIPS, not passes:"
    for line in "${SKIP_LINES[@]}"; do printf '    - %s\n' "$line"; done
fi
echo "  $PASS passed, $FAIL failed, $SKIP skipped"
[ "$FAIL" -eq 0 ] || exit 1
