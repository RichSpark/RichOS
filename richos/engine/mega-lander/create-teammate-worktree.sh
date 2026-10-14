#!/usr/bin/env bash
#
# create-teammate-worktree.sh — THE ONLY WAY TO GIVE A TEAMMATE A WORKSPACE IN
# ANOTHER REPOSITORY. The page: docs/plans/worktree-spec-2026-09-11.md.
#
# NORMALLY YOU DO NOT RUN THIS DIRECTLY. `scripts/spawn.sh` runs it as one step
# of one command, having already resolved the repository by name, recorded or
# repaired the branch this work integrates on, and evaluated every
# PreToolUse[Agent] guard against the payload — so the refusals below are found
# and reported TOGETHER before anything is created, rather than one per round
# trip. This file stays the only creator, and stays runnable on its own.
#
#   1. "Every non-native Claude workspace is named cc/."            (point 1)
#   3. "If registration fails, the spawn does not happen. Creating a
#       non-native workspace not named cc/ is refused."             (point 3)
#
# WHAT IT DOES, in order — and it stops at the first refusal:
#   1. Refuses a name that is not <role>-<model>-<identifier> (the spawn
#      contract), a path that already exists, a branch that already exists,
#      and a base that is a codex/ branch (point 2: an agent works from a copy
#      in its own cc/ workspace, never inside a codex/ one).
#      A SECOND REPOSITORY UNDER THE SAME NAME IS NOT ONE OF THOSE REFUSALS.
#      Run it again with a different <repo> and the same <teammate-name>: the
#      teammate gets a second registered cc/ workspace, and all of an agent's
#      workspaces go together when its work is landed or discarded (point 10).
#      What stays refused is a second workspace in the SAME repository under
#      that name - it would want the same cc/<name> branch, which already
#      exists there (point 3, one registration per repository). Normally you do
#      not run this twice either: `spawn.sh --repo A --repo B` does both.
#   2. REGISTERS the workspace — session, teammate, repository, path, cc/
#      branch — in the workspace registry (mega-lander/workspaces.py) BEFORE
#      anything exists on disk. If the registration cannot be written, nothing
#      is created, so no spawn can name the workspace.
#   3. `git worktree add <dir> -b cc/<name> <base>` in the repository's MAIN
#      checkout (resolved through `git worktree list`, never guessed).
#   4. Seeds every gitignored file matching a `.worktreeinclude` pattern from
#      the main checkout — the same contract native isolation honors.
#  4b. Runs the repository's own `.worktree-setup`, if it has one, inside the
#      new workspace. That is where a repository puts the things every one of
#      its worktrees needs and none of them should build for itself — richos
#      points its cargo target/ at the one shared build cache there. NEVER
#      FATAL: a workspace whose setup failed is a workspace that works and
#      rebuilds more, which is not a reason to refuse a teammate a place to
#      stand. It is reported, loudly, and it is bounded — a setup script that
#      hangs would otherwise hang every spawn on the machine.
#   5. Confirms the registration: created. If creation failed, the
#      registration records that instead; the agent was never spawned, so it
#      counts as finished, and whatever git left behind (a branch) is deleted
#      by the page's own land — an agent that produced nothing counts as landed
#      (point 7). This script deletes nothing.
#   6. Prints the path and the spawn contract that carries it.
#
# USAGE
#   ~/.claude/richos-engine/mega-lander/create-teammate-worktree.sh <repo> <teammate-name> [--dir <path>]
#       [--base <ref>] [--session <id>] [--integration <branch>]
#
#   <repo>            any path inside the target repository
#   <teammate-name>   the name the spawn will carry, e.g. echo-opus-bt2
#   --dir <path>      where to put it; default <main>-wt/<name> beside the
#                     main checkout (the convention this machine already uses)
#   --base <ref>      branch point; default the main checkout's HEAD. To
#                     continue a finished agent's work (point 7), its branch:
#                     --base cc/<old-name>, and spawn with `continues: <old-name>`.
#   --session <id>    the session the spawn will run in; default the session
#                     this command runs in (its recorded process is an ancestor)
#   --integration <b> the branch THIS teammate lands on when it is not the
#                     repository's recorded one (a release branch such as
#                     v1.3.0); only this teammate is bound to it, and --base
#                     defaults to it
#
# Environment (test affordances): RICHOS_WORKSPACES_DIR, RICHOS_SESSION_PID,
# RICHOS_SESSION_ID.
#
# Exit codes: 0 created + registered; 2 usage; 3 refused (name / exists /
#             registration); 4 git failed (the registration records it).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WS_PY="$SCRIPT_DIR/workspaces.py"

usage() {
    sed -n '/^# USAGE/,/^# Environment/p' "$0" | sed 's/^# \{0,1\}//' >&2
}

REPO_ARG=""; NAME=""; DIR=""; BASE=""; SESSION=""; INTEGRATION=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        # No errexit here (set -uo only): a `shift 2` short of a value shifts nothing
        # and the loop never ends, so a missing value is refused (hunt P5-27 pattern).
        --dir)     DIR="${2:-}"; shift 2 || { echo "create-teammate-worktree.sh: $1 needs a value" >&2; exit 2; } ;;
        --base)    BASE="${2:-}"; shift 2 || { echo "create-teammate-worktree.sh: $1 needs a value" >&2; exit 2; } ;;
        --session) SESSION="${2:-}"; shift 2 || { echo "create-teammate-worktree.sh: $1 needs a value" >&2; exit 2; } ;;
        --integration) INTEGRATION="${2:-}"; shift 2 || { echo "create-teammate-worktree.sh: $1 needs a value" >&2; exit 2; } ;;
        -h|--help) usage; exit 2 ;;
        -*)        echo "create-teammate-worktree.sh: unknown option '$1'" >&2; usage; exit 2 ;;
        *)
            if [ -z "$REPO_ARG" ]; then REPO_ARG="$1"
            elif [ -z "$NAME" ]; then NAME="$1"
            else echo "create-teammate-worktree.sh: unexpected argument '$1'" >&2; usage; exit 2; fi
            shift ;;
    esac
done
[ -n "$REPO_ARG" ] && [ -n "$NAME" ] || { usage; exit 2; }
command -v git >/dev/null 2>&1 || { echo "create-teammate-worktree.sh: git is required" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "create-teammate-worktree.sh: python3 is required" >&2; exit 2; }
[ -f "$WS_PY" ] || { echo "create-teammate-worktree.sh: the workspace registry is missing at $WS_PY — refusing to create a workspace that could not be registered" >&2; exit 2; }

refuse() { echo "create-teammate-worktree.sh: REFUSED — $*" >&2; exit 3; }

# The setup's own processes (step 4b): the PID this script started, every
# process in the group it made for itself, and every descendant of either.
_setup_tree() { # <setup pid>
    ps -axo pid=,ppid=,pgid= 2>/dev/null | awk -v root="$1" '
        { pid[NR] = $1; ppid[NR] = $2; pgid[NR] = $3; n = NR }
        END {
            keep[root] = 1
            grew = 1
            while (grew) {
                grew = 0
                for (i = 1; i <= n; i++)
                    if (!(pid[i] in keep) && ((ppid[i] in keep) || pgid[i] == root)) { keep[pid[i]] = 1; grew = 1 }
            }
            for (p in keep) print p
        }'
}
# Which of these PIDs are still running (a zombie has already stopped).
_setup_alive() { # <pid>...
    local _q
    for _q in "$@"; do
        kill -0 "$_q" 2>/dev/null || continue
        case "$(ps -o stat= -p "$_q" 2>/dev/null)" in *Z*) ;; *) printf '%s\n' "$_q" ;; esac
    done
}

# --- 1. the name is the spawn contract's name --------------------------------
# THE RULE ITSELF LIVES IN scripts/lib/teammate-name.sh — shared with
# guard-worktree-isolation.sh, which used to carry a LOOSER shape check of its
# own (no length bound, any case, any number of extra segments). See that
# file's header for the 2026-09-17 defect this replaces: a name could pass the
# guard and be refused here, one round trip after spawn.sh's own guard table
# said "passed" — spawn.sh always runs guard-worktree-isolation.sh (from the
# engine's own hooks.json) before creating anything, so the guard now sharing
# this rule is what closes that round trip; nothing here needed to change to
# fix it.
# shellcheck source=../scripts/lib/teammate-name.sh
. "$SCRIPT_DIR/../scripts/lib/teammate-name.sh"
ALLOWED_MODELS="$(teammate_name_engine_allowed_models "$SCRIPT_DIR/..")"
if ! _name_msg="$(teammate_name_check "$NAME" "$ALLOWED_MODELS")"; then
    refuse "$_name_msg"
fi

# --- the caller's deadline (hunt part 4, finding 18) --------------------------
# A caller that stops waiting at a known moment says so: RICHOS_OPERATION_DEADLINE
# is that moment in epoch seconds. The app gives a whole preparation 120 s and
# this script alone allowed its setup 120 s, so the outer bound could expire
# first, leaving a creation the caller can only record as "unknown". Every
# bound below is now the smaller of its own and what is left of the caller's,
# less CALLER_RESERVE for the confirmation and report that follow. A deadline
# already passed is refused here, before anything exists. Unset: unchanged.
CALLER_RESERVE=10
_caller_left() { # seconds left before the caller's deadline, less the reserve; "" when there is none
    [ -n "${RICHOS_OPERATION_DEADLINE:-}" ] || return 0
    python3 -c 'import sys, time; print(int(float(sys.argv[1]) - time.time()) - int(sys.argv[2]))' \
        "$RICHOS_OPERATION_DEADLINE" "$CALLER_RESERVE" 2>/dev/null || echo "invalid"
}
case "$(_caller_left)" in
    invalid) refuse "RICHOS_OPERATION_DEADLINE='$RICHOS_OPERATION_DEADLINE' is not a time in epoch seconds; nothing was created" ;;
    -*)      refuse "the caller's deadline (RICHOS_OPERATION_DEADLINE=$RICHOS_OPERATION_DEADLINE) leaves no time to create a workspace; nothing was created" ;;
esac

# --- the repository's MAIN checkout, from git, never from the argument -------
[ -d "$REPO_ARG" ] || refuse "'$REPO_ARG' is not a directory"
MAIN="$(git -C "$REPO_ARG" worktree list --porcelain 2>/dev/null | sed -n '1s|^worktree ||p')"
[ -n "$MAIN" ] || refuse "'$REPO_ARG' is not inside a git repository"
MAIN="$(cd "$MAIN" && pwd -P)"

[ -n "$DIR" ] || DIR="$(dirname "$MAIN")/$(basename "$MAIN")-wt/$NAME"
[ ! -e "$DIR" ] || refuse "'$DIR' already exists"
case "$DIR" in /*) : ;; *) DIR="$(pwd -P)/$DIR" ;; esac
# THE BRANCH CARRIES OUR OWN PREFIX (point 1). Git cannot hold a bare `cc`
# ref beside `cc/<name>`, so nothing may ever create one.
BRANCH="cc/$NAME"
if git -C "$MAIN" rev-parse --verify --quiet "refs/heads/$BRANCH" >/dev/null; then
    refuse "branch '$BRANCH' already exists in $MAIN — a teammate name is used once; pick a fresh identifier"
fi
[ -n "$BASE" ] || BASE="${INTEGRATION:-HEAD}"
git -C "$MAIN" rev-parse --verify --quiet "$BASE^{commit}" >/dev/null \
    || refuse "base ref '$BASE' does not resolve in $MAIN"
case "$BASE" in
    codex/*|refs/heads/codex/*)
        refuse "base '$BASE' is a codex/ branch — codex/ is never touched; an agent works from a copy in its own cc/ workspace, so branch from the commit instead: --base \$(git -C $MAIN rev-parse $BASE) (point 2)" ;;
esac

# --- 2. register BEFORE anything exists (point 3) ---------------------------
SESS_ARGS=()
[ -n "$SESSION" ] && SESS_ARGS=(--session "$SESSION")
INT_ARGS=()
[ -n "$INTEGRATION" ] && INT_ARGS=(--integration "$INTEGRATION")
if ! _reg_err="$(python3 "$WS_PY" ${SESS_ARGS[@]+"${SESS_ARGS[@]}"} register-cc --name "$NAME" --repo "$MAIN" \
        --path "$DIR" --branch "$BRANCH" ${INT_ARGS[@]+"${INT_ARGS[@]}"} 2>&1 >/dev/null)"; then
    refuse "the workspace could not be registered, so it was not created: ${_reg_err#workspaces: REFUSED — }"
fi

# --- 3. create --------------------------------------------------------------
_fail() { # <why>
    python3 "$WS_PY" ${SESS_ARGS[@]+"${SESS_ARGS[@]}"} confirm-cc --name "$NAME" --path "$DIR" --failed "$1" >/dev/null 2>&1 || true
    echo "create-teammate-worktree.sh: $1 — the registration records it; nothing is spawned into it, and whatever git left behind is deleted by the page's own land (docs/plans/worktree-spec-2026-09-11.md, point 7)." >&2
    exit 4
}
mkdir -p "$(dirname "$DIR")" || _fail "cannot create $(dirname "$DIR")"
git -C "$MAIN" worktree add -q "$DIR" -b "$BRANCH" "$BASE" || _fail "git worktree add failed for $DIR"
DIR="$(cd "$DIR" && pwd -P)"
git -C "$MAIN" worktree list --porcelain 2>/dev/null | sed -n 's|^worktree ||p' \
    | while IFS= read -r _p; do [ "$(cd "$_p" 2>/dev/null && pwd -P)" = "$DIR" ] && exit 0; done \
    || _fail "git does not list $DIR as a worktree of $MAIN"

# --- 4. seed .worktreeinclude ---------------------------------------------
# The same contract native isolation honors: gitignore-style patterns, matched
# against files that are IGNORED in the main checkout, copied with their
# relative paths. Done in python so `**/` means what .gitignore means by it.
#
# A FAILED SEED IS A FAILED CREATION, never "0 file(s)" (hunt part 4, finding
# 16). Its errors used to go to /dev/null and its failure became `echo 0`, so an
# unreadable .env, a copy that stopped half way or a git that could not list the
# ignored files read exactly like a repository with nothing to seed, and the
# teammate started without the files its repository needs. Nothing justified
# hiding it, unlike the setup below, whose failure leaves a workspace that
# merely rebuilds more. It now goes through _fail: recorded, exit 4, and
# spawn.sh rolls the workspace back.
SEEDED=0
if [ -f "$MAIN/.worktreeinclude" ]; then
    SEED_ERR="$(mktemp -t worktree-seed)"
    SEEDED="$(MAIN="$MAIN" DIR="$DIR" python3 - <<'PY' 2>"$SEED_ERR"
import os, re, shutil, subprocess, sys
main = os.environ["MAIN"]; dest = os.environ["DIR"]
pats = []
with open(os.path.join(main, ".worktreeinclude"), encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if line and not line.startswith("#"):
            pats.append(line)
if not pats:
    print(0); sys.exit(0)

def to_regex(pat):
    # .gitignore semantics, the subset that matters: a leading '**/' matches at
    # any depth (including none); a pattern with no '/' matches the basename at
    # any depth; otherwise it is anchored at the repository root.
    anchored = "/" in pat.strip("/") and not pat.startswith("**/")
    p = pat.lstrip("/")
    if p.startswith("**/"):
        p = p[3:]
        prefix = r"(?:.*/)?"
    elif not anchored:
        prefix = r"(?:.*/)?"
    else:
        prefix = ""
    body = re.escape(p).replace(r"\*\*/", r"(?:.*/)?").replace(r"\*", r"[^/]*").replace(r"\?", r"[^/]")
    return re.compile("^" + prefix + body + "$")

regs = [to_regex(p) for p in pats]
res = subprocess.run(["git", "-C", main, "ls-files", "--others", "--ignored", "--exclude-standard", "-z"],
                     capture_output=True, text=True)
if res.returncode != 0:
    sys.stderr.write("git could not list the main checkout's ignored files (exit %d): %s\n"
                     % (res.returncode, res.stderr.strip()[:300]))
    sys.exit(1)
n = 0
for rel in res.stdout.split("\0"):
    if not rel or not any(r.match(rel) for r in regs):
        continue
    src = os.path.join(main, rel)
    if not os.path.isfile(src):
        continue
    dst = os.path.join(dest, rel)
    try:
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(src, dst)
    except OSError as e:
        sys.stderr.write("%s could not be copied after %d file(s) were: %s\n" % (rel, n, e))
        sys.exit(1)
    n += 1
print(n)
PY
)"
    SEED_RC=$?
    if [ "$SEED_RC" -ne 0 ]; then
        _seed_why="$(tr '\n' ' ' <"$SEED_ERR" | cut -c1-600)"
        rm -f "$SEED_ERR"
        _fail "seeding the .worktreeinclude files into $DIR FAILED (exit $SEED_RC): ${_seed_why:-no error text}"
    fi
    rm -f "$SEED_ERR"
fi

# --- 4b. the repository's own per-worktree setup ----------------------------
# Resolved in `.richos/` first and at the repository root second, which is the
# order scripts/lib/declaration-path.sh established and the order every other
# declaration in this engine is found in. Read from the NEW WORKTREE, not from
# the main checkout: it is tracked content, so the worktree has the version its
# own base commit carries, and a setup script must match the tree it sets up.
SETUP_STATUS="none"
: "${WORKTREE_SETUP_DECLARATION:=.worktree-setup}"
SETUP="$DIR/.richos/${WORKTREE_SETUP_DECLARATION#.}"
[ -f "$SETUP" ] || SETUP="$DIR/$WORKTREE_SETUP_DECLARATION"
[ -f "$SETUP" ] || SETUP=""
# Bounded. 120s is many times what any setup here takes and still finite; an
# unbounded child would make one bad commit hang every spawn after it. The
# bound is a variable so the suite can prove the kill without waiting two
# minutes for it — never so a caller can switch it off. And it never runs past
# the caller's own deadline (finding 18, see the top of this file).
: "${WORKTREE_SETUP_TIMEOUT:=120}"
SETUP_BOUND="$WORKTREE_SETUP_TIMEOUT"
SETUP_BOUND_WHY=""
_left="$(_caller_left)"
if [ -n "$SETUP" ] && [ -n "$_left" ] && [ "$_left" != invalid ] && [ "$_left" -lt "$SETUP_BOUND" ]; then
    SETUP_BOUND=$(( _left > 0 ? _left : 0 ))
    SETUP_BOUND_WHY=" (bounded by the caller's deadline)"
fi
if [ -n "$SETUP" ] && [ "$SETUP_BOUND" -le 0 ]; then
    SETUP_STATUS="SKIPPED: no time was left before the caller's deadline"
    echo "create-teammate-worktree.sh: $SETUP $SETUP_STATUS. The workspace is fine and the teammate can work in it; whatever that script shares between worktrees is simply not shared here." >&2
    SETUP=""
fi
if [ -n "$SETUP" ]; then
    SETUP_LOG="$(mktemp -t worktree-setup)"
    # ITS OWN PROCESS GROUP, so the timeout below can stop everything it
    # started (hunt part 4, finding 17). The kill used to reach SETUP_PID
    # alone, while a child the setup had started kept running in the workspace
    # and the summary said "killed". The group is made by the process itself
    # (setpgid, then exec: the PID does not change) because macOS has no
    # setsid(1) and job control would print notices into this script's output.
    # Everything signaled below is this PID, its group, or a descendant of it:
    # processes this script started, never ones matched by a name or a path.
    ( cd "$DIR" && exec python3 -c 'import os, sys; os.setpgid(0, 0); os.execvp("bash", ["bash", sys.argv[1]])' "$SETUP" ) > "$SETUP_LOG" 2>&1 &
    SETUP_PID=$!
    SETUP_WAITED=0
    while kill -0 "$SETUP_PID" 2>/dev/null && [ "$SETUP_WAITED" -lt "$SETUP_BOUND" ]; do
        sleep 1
        SETUP_WAITED=$((SETUP_WAITED + 1))
    done
    if kill -0 "$SETUP_PID" 2>/dev/null; then
        # Read the tree BEFORE anything dies: a child whose parent is killed is
        # reparented and no longer looks like a descendant. The group catches
        # what the tree then misses; the tree catches a child that left the
        # group while its parent still lived.
        _SETUP_TREE="$(_setup_tree "$SETUP_PID")"
        kill -9 -- "-$SETUP_PID" 2>/dev/null
        for _p in $_SETUP_TREE; do kill -9 "$_p" 2>/dev/null; done
        wait "$SETUP_PID" 2>/dev/null
        _SETUP_LEFT=""
        for _i in 1 2 3 4 5 6 7 8 9 10; do
            _SETUP_LEFT="$(_setup_alive "$SETUP_PID" $_SETUP_TREE)"
            [ -z "$_SETUP_LEFT" ] && break
            sleep 0.5
        done
        _SETUP_N="$(printf '%s\n' $_SETUP_TREE | grep -c .)"
        if [ -z "$_SETUP_LEFT" ]; then
            SETUP_STATUS="TIMED OUT after ${SETUP_WAITED}s${SETUP_BOUND_WHY} and was killed, with every process it started ($_SETUP_N)"
        else
            SETUP_STATUS="TIMED OUT after ${SETUP_WAITED}s${SETUP_BOUND_WHY}; killed, but STILL RUNNING: $(printf '%s ' $_SETUP_LEFT)"
        fi
    elif wait "$SETUP_PID"; then
        SETUP_STATUS="ok"
    else
        SETUP_STATUS="FAILED (exit $?)"
    fi
    if [ "$SETUP_STATUS" != "ok" ]; then
        echo "create-teammate-worktree.sh: $SETUP $SETUP_STATUS. The workspace is fine and the teammate can work in it; whatever that script shares between worktrees is simply not shared here. Output:" >&2
        sed 's/^/    /' "$SETUP_LOG" >&2
    fi
    rm -f "$SETUP_LOG"
fi

# --- 5. confirm -------------------------------------------------------------
if ! python3 "$WS_PY" ${SESS_ARGS[@]+"${SESS_ARGS[@]}"} confirm-cc --name "$NAME" --path "$DIR" >/dev/null 2>&1; then
    echo "create-teammate-worktree.sh: created $DIR but its registration could not be confirmed — the spawn guard will refuse to spawn into it; it is finished work of this session (point 3) and the page's land deletes it." >&2
    exit 4
fi

# --- 6. report --------------------------------------------------------------
echo "created:    $DIR"
echo "branch:     $BRANCH  (from $BASE in $MAIN)"
echo "seeded:     $SEEDED file(s) from .worktreeinclude"
echo "setup:      $SETUP_STATUS${SETUP:+  ($SETUP)}"
echo "registered: teammate=$NAME ($(python3 -c 'import importlib.util,sys; s=importlib.util.spec_from_file_location("w",sys.argv[1]); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); print(m.state_dir())' "$WS_PY" 2>/dev/null))"
echo ""
echo "Spawn with isolation: \"worktree\" in the session repo; add this prompt line:"
echo "  cross-repo-worktree: $DIR"
echo "A cwd-only spawn is refused."
echo "Prepare the full Agent JSON, including its mandatory acknowledgement contract:"
echo "  python3 \"$SCRIPT_DIR/../scripts/prepare-agent-spawn.py\" --file <task-input.json>"
echo "...or do the whole thing, guards evaluated first, in ONE command next time:"
echo "  $SCRIPT_DIR/../scripts/spawn.sh <teammate-name> --repo <repo|name> [--repo <repo|name> ...] --type <subagent-type> --brief <file>"
echo "  (--repo once per repository this teammate works in; they go together at the land, point 10.)"
exit 0
