#!/usr/bin/env bash
#
# create-teammate-worktree.test.sh — behavioral tests for the one command that
# creates a cc/ workspace (docs/plans/worktree-spec-2026-09-11.md, points 1-3).
#
# The helper REGISTERS the workspace in the workspace registry before anything
# exists on disk, creates it on a cc/ branch, seeds .worktreeinclude, confirms
# the registration, and deletes nothing. Every case is two-sided where a verdict
# is involved. HOME, CLAUDE_CONFIG_DIR and the registry are pinned into the
# sandbox and the session is a process of the suite's own, so no case can touch
# the operator's record.
#
# Run directly: mega-lander/tests/create-teammate-worktree.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HELPER="$SCRIPT_DIR/../create-teammate-worktree.sh"
WS_PY="$SCRIPT_DIR/../workspaces.py"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t create-teammate-wt.XXXXXX)" && pwd -P)"
SESS_PID="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
trap 'kill "$SESS_PID" 2>/dev/null; rm -rf "$SANDBOX"' EXIT
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -x "$HELPER" ] || { echo "FATAL: helper missing/non-executable: $HELPER" >&2; exit 1; }

export HOME="$SANDBOX/home" CLAUDE_CONFIG_DIR="$SANDBOX/home/.claude"
export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces"
export RICHOS_SESSION_PID="$SESS_PID" RICHOS_SESSION_ID="feedface-0000-4000-8000-000000000001"
export GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid \
       GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
mkdir -p "$CLAUDE_CONFIG_DIR"
REC() { printf '%s/agents/%s--%s.json' "$RICHOS_WORKSPACES_DIR" "${2:-$RICHOS_SESSION_ID}" "$1"; }

# A repository with a .worktreeinclude and the gitignored files it names, in
# the same shape femcboost carries (.envrc at the root, .env.local nested).
REPO="$SANDBOX/repo"
mkdir -p "$REPO/app/deep"
git -C "$REPO" init -q -b main
printf '.envrc\n**/.env.local\n' >"$REPO/.worktreeinclude"
printf '.envrc\n.env.local\n' >"$REPO/.gitignore"
printf 'seed\n' >"$REPO/seed.txt"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m seed
printf 'export X=1\n' >"$REPO/.envrc"
printf 'A=1\n' >"$REPO/app/.env.local"
printf 'B=2\n' >"$REPO/app/deep/.env.local"
printf 'not-seeded\n' >"$REPO/app/other.local"
# Point 14: "The branch a body of work integrates on is RECORDED when that work
# starts, before its first agent is spawned. Nothing infers it and nothing
# guesses it." Since eedfbc7d (2026-09-12) register_cc REFUSES a repository with
# no current body of work, and this helper registers BEFORE it creates — so
# without this line every creation below is refused and the suite tests the
# operator step it skipped, not the helper. One command, exactly as Rich runs it.
python3 "$WS_PY" --entity "$REPO" --session "$RICHOS_SESSION_ID" integration \
    --repo "$REPO" --branch main --why "the create-teammate-worktree suite's body of work" >/dev/null

echo "=== create-teammate-worktree tests ==="

# 1. THE HAPPY PATH: created at <repo>-wt/<name>, on branch cc/<name> from HEAD,
#    seeded, and REGISTERED with path/repo/branch/session.
OUT="$("$HELPER" "$REPO/app" echo-opus-high-ct1 2>&1)"; rc=$?
WT="$SANDBOX/repo-wt/echo-opus-high-ct1"
if [ "$rc" -eq 0 ] && [ -d "$WT" ] \
   && [ "$(git -C "$WT" symbolic-ref -q --short HEAD)" = "cc/echo-opus-high-ct1" ] \
   && [ "$(git -C "$WT" rev-parse HEAD)" = "$(git -C "$REPO" rev-parse HEAD)" ]; then
    ok "C01  creates <repo>-wt/<name> on branch cc/<name> at the main checkout's HEAD (point 1)"
else
    bad "C01  create (rc=$rc): $OUT"
fi
if [ -f "$WT/.envrc" ] && [ -f "$WT/app/.env.local" ] && [ -f "$WT/app/deep/.env.local" ] \
   && [ ! -f "$WT/app/other.local" ] && printf '%s' "$OUT" | grep -q 'seeded:     3 file(s)'; then
    ok "C02  seeds every gitignored file matching .worktreeinclude (root and nested), nothing else"
else
    bad "C02  seeding: $(ls -la "$WT" "$WT/app" 2>/dev/null | tr '\n' ' ')"
fi
if python3 - "$(REC echo-opus-high-ct1)" "$WT" "$REPO" <<'PY' 2>/dev/null
import json, os, sys
r = json.load(open(sys.argv[1]))
assert r["name"] == "echo-opus-high-ct1" and r["session_id"] == "feedface-0000-4000-8000-000000000001", r
w = r["workspaces"][0]
assert w["kind"] == "cc" and w["branch"] == "cc/echo-opus-high-ct1" and w["created"] is True, w
assert os.path.realpath(w["path"]) == os.path.realpath(sys.argv[2]), w
assert os.path.realpath(w["repo"]) == os.path.realpath(sys.argv[3]), w
assert r["session_identity"]["pid"] and r["session_identity"]["pid_start"], r
PY
then ok "C03  the workspace is REGISTERED: name, session, repo, exact path, cc/ branch, created, and the session's process identity (point 3)"
else bad "C03  registration: $(cat "$(REC echo-opus-high-ct1)" 2>/dev/null | tr '\n' ' ' | cut -c1-300)"; fi
if printf '%s' "$OUT" | grep -q 'isolation: "worktree"' \
   && printf '%s' "$OUT" | grep -q "cross-repo-worktree: $WT" \
   && printf '%s' "$OUT" | grep -q 'prepare-agent-spawn.py' \
   && ! printf '%s' "$OUT" | grep -q 'cwd: "'; then
    ok "C05  prints native+external and acknowledgement preparation; never recommends cwd-only"
else
    bad "C05  spawn contract: $OUT"
fi

# 2. REFUSALS, each named. Nothing is created and nothing is registered.
N0="$(ls "$RICHOS_WORKSPACES_DIR/agents" | wc -l | tr -d ' ')"
"$HELPER" "$REPO" echo-opus-high-ct1 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && ok "C06  refuses a name whose branch already exists (exit 3)" || bad "C06  duplicate name rc=$rc"
"$HELPER" "$REPO" echo-ct2 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && [ ! -d "$SANDBOX/repo-wt/echo-ct2" ] && ok "C07  refuses a name that is not <role>-<model>-<effort>-<identifier> (exit 3)" || bad "C07  bad name rc=$rc"
# The effort word (CEO, 2026-11-17): the three parts every name had before are refused for a
# NEW workspace, in the shared rule's own words, and a word that is not a level is refused.
OUT="$("$HELPER" "$REPO" echo-opus-ct7 2>&1)"; rc=$?
[ "$rc" -eq 3 ] && [ ! -d "$SANDBOX/repo-wt/echo-opus-ct7" ] \
    && printf '%s' "$OUT" | grep -qF "is not a teammate name of the form <role>-<model>-<effort>-<identifier>" \
    && ok "C07b refuses a name with no effort word, naming the four-part form (exit 3)" || bad "C07b no effort word rc=$rc: $OUT"
"$HELPER" "$REPO" echo-opus-ultra-ct7 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && [ ! -d "$SANDBOX/repo-wt/echo-opus-ultra-ct7" ] \
    && ok "C07c refuses an effort word that is not a level Claude Code accepts (exit 3)" || bad "C07c bad effort word rc=$rc"
"$HELPER" "$REPO" echo-gpt-ct3 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && ok "C08  refuses a model token outside the allowed set (exit 3)" || bad "C08  bad model rc=$rc"
mkdir -p "$SANDBOX/repo-wt/echo-opus-high-ct4"
"$HELPER" "$REPO" echo-opus-high-ct4 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && ok "C09  refuses a target path that already exists (exit 3)" || bad "C09  existing path rc=$rc"
"$HELPER" "$REPO" echo-opus-high-ct5 --base no-such-ref >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && ok "C10  refuses a base ref that does not resolve (exit 3)" || bad "C10  bad base rc=$rc"
"$HELPER" "$SANDBOX" echo-opus-high-ct6 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && ok "C11  refuses a path that is not inside a repository (exit 3)" || bad "C11  non-repo rc=$rc"
"$HELPER" "$REPO" >/dev/null 2>&1; rc=$?
[ "$rc" -eq 2 ] && ok "C12  missing arguments is a usage error (exit 2)" || bad "C12  usage rc=$rc"
# An option with no value: the helper has no errexit, so an unguarded `shift 2` looped
# forever (hunt P5-27 pattern). Bounded by python's timeout, which kills only its own child.
for opt in --dir --base --session; do
    rc="$(python3 - "$HELPER" "$REPO" echo-opus-high-ct7 "$opt" <<'PYB'
import subprocess, sys
try:
    print(subprocess.run(sys.argv[1:], capture_output=True, timeout=10).returncode)
except subprocess.TimeoutExpired:
    print("timeout")
PYB
)"
    [ "$rc" = 2 ] && ok "C12c $opt with no value is a usage error (exit 2), never a loop" \
        || bad "C12c $opt with no value: rc=$rc"
done
git -C "$REPO" branch codex/their-fix HEAD
"$HELPER" "$REPO" echo-opus-high-cx1 --base codex/their-fix >/dev/null 2>&1; rc=$?
[ "$rc" -eq 3 ] && [ ! -d "$SANDBOX/repo-wt/echo-opus-high-cx1" ] && ok "C12b refuses a codex/ base: an agent works from a copy, never inside codex/ (point 2)" || bad "C12b codex base rc=$rc"
[ "$(ls "$RICHOS_WORKSPACES_DIR/agents" | wc -l | tr -d ' ')" -eq "$N0" ] && ok "C13  no refusal wrote a registration" || bad "C13  a refusal wrote a registration"

# 3. --dir, --base and --session are honored.
git -C "$REPO" branch older-base HEAD
OUT="$("$HELPER" "$REPO" mark-sonnet-high-ct7 --dir "$SANDBOX/elsewhere/mark-sonnet-high-ct7" --base older-base --session cafebabe-0000-4000-8000-000000000002 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && [ -d "$SANDBOX/elsewhere/mark-sonnet-high-ct7" ] && [ -f "$(REC mark-sonnet-high-ct7 cafebabe-0000-4000-8000-000000000002)" ]; then
    ok "C14  --dir, --base and --session are honored (registered under the named session)"
else
    bad "C14  options (rc=$rc): $OUT"
fi

# 4. REGISTRATION FIRST: when it cannot be written, NOTHING is created (point 3).
OUT="$(RICHOS_WORKSPACES_DIR=/nonexistent-dir/registry "$HELPER" "$REPO" norm-opus-high-ct8 2>&1)"; rc=$?
if [ "$rc" -eq 3 ] && [ ! -e "$SANDBOX/repo-wt/norm-opus-high-ct8" ] \
   && ! git -C "$REPO" rev-parse --verify -q refs/heads/cc/norm-opus-high-ct8 >/dev/null \
   && printf '%s' "$OUT" | grep -q 'could not be registered, so it was not created'; then
    ok "C17  an unregistrable workspace is NEVER CREATED: no directory, no branch (point 3)"
else
    bad "C17  unregistrable (rc=$rc): $OUT"
fi

# 5. NO SESSION PROCESS -> refused before anything exists: a registration that
#    could never tell its session ended is not a registration (point 12).
OUT="$(RICHOS_SESSION_PID=999999 "$HELPER" "$REPO" norm-opus-high-ct9 2>&1)"; rc=$?
if [ "$rc" -eq 3 ] && [ ! -e "$SANDBOX/repo-wt/norm-opus-high-ct9" ] && printf '%s' "$OUT" | grep -q 'process identity'; then
    ok "C19  no readable session process -> refused, nothing created"
else
    bad "C19  no-session (rc=$rc): $OUT"
fi
"$HELPER" "$REPO" norm-opus-high-ct9 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 0 ] && [ -d "$SANDBOX/repo-wt/norm-opus-high-ct9" ] && ok "C20  the same call with a readable session succeeds (positive control)" || bad "C20  positive control rc=$rc"

# 6. CREATION FAILS AFTER REGISTRATION: recorded, NOTHING DELETED by the helper.
#    A `git` shim fails `worktree add` only.
SHIM="$SANDBOX/shim"
mkdir -p "$SHIM"
REAL_GIT="$(command -v git)"
cat >"$SHIM/git" <<SHIMEOF
#!/usr/bin/env bash
# Forwards to the real git; fails only 'worktree add'.
args=("\$@"); sub=""; rest=""
i=0
while [ \$i -lt \${#args[@]} ]; do
    case "\${args[\$i]}" in
        -C|-c) i=\$((i + 2)); continue ;;
        -*)    i=\$((i + 1)); continue ;;
        *)     sub="\${args[\$i]}"; rest="\${args[\$((i + 1))]:-}"; break ;;
    esac
done
[ "\$sub" = "worktree" ] && [ "\$rest" = "add" ] && { echo "shim: simulated worktree add failure" >&2; exit 1; }
exec "$REAL_GIT" "\$@"
SHIMEOF
chmod +x "$SHIM/git"
OUT="$(PATH="$SHIM:$PATH" "$HELPER" "$REPO" norm-opus-high-ct21 2>&1)"; rc=$?
if [ "$rc" -eq 4 ] && python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); assert r.get("creation_failed"), r' "$(REC norm-opus-high-ct21)" 2>/dev/null \
   && printf '%s' "$OUT" | grep -q "page's own land"; then
    ok "C22  creation failing AFTER registration is RECORDED (creation_failed), exit 4; the helper deletes nothing"
else
    bad "C22  failed creation (rc=$rc): $OUT"
fi
# 7. SEVERAL REPOSITORIES UNDER ONE NAME (point 10: "All of an agent's
#    workspaces go together... every workspace and branch it has"). A teammate
#    working in two repositories gets two cc/ workspaces under the ONE name —
#    it is one agent, not two. Until 2026-09-17 the second one had to be given a
#    second NAME, which made it a second agent landed separately. What stays
#    refused is a second workspace in the SAME repository under that name
#    (point 3, one registration per repository).
REPO2="$SANDBOX/repo2"
mkdir -p "$REPO2"
git -C "$REPO2" init -q -b main
printf 'seed\n' >"$REPO2/seed.txt"
git -C "$REPO2" add -A
git -C "$REPO2" commit -q -m seed
python3 "$WS_PY" --entity "$REPO2" --session "$RICHOS_SESSION_ID" integration \
    --repo "$REPO2" --branch main --why "the second repository of one teammate" >/dev/null
"$HELPER" "$REPO" norm-opus-high-ct30 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 0 ] && ok "C24a POSITIVE CONTROL: the first workspace of norm-opus-high-ct30 is created (exit 0)" \
                || bad "C24a first workspace rc=$rc"
OUT="$("$HELPER" "$REPO2" norm-opus-high-ct30 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && [ -d "$SANDBOX/repo2-wt/norm-opus-high-ct30" ] \
   && [ "$(git -C "$SANDBOX/repo2-wt/norm-opus-high-ct30" symbolic-ref -q --short HEAD)" = "cc/norm-opus-high-ct30" ]; then
    ok "C24  a SECOND repository under the SAME name is created on its own cc/<name> branch (point 10)"
else
    bad "C24  second repository (rc=$rc): $OUT"
fi
if python3 - "$(REC norm-opus-high-ct30)" "$REPO" "$REPO2" <<'PY' 2>/dev/null
import json, os, sys
r = json.load(open(sys.argv[1]))
live = [w for w in r["workspaces"] if not w.get("deleted_at")]
assert len(live) == 2, live
repos = sorted(os.path.realpath(w["repo"]) for w in live)
assert repos == sorted(os.path.realpath(p) for p in sys.argv[2:4]), repos
assert all(w["kind"] == "cc" and w["created"] is True and w["branch"] == "cc/norm-opus-high-ct30"
           for w in live), live
PY
then ok "C25  BOTH are on the ONE agent's record — one name, one agent, two workspaces (point 10)"
else bad "C25  one record: $(cat "$(REC norm-opus-high-ct30)" 2>/dev/null | tr '\n' ' ' | cut -c1-400)"; fi
OUT="$("$HELPER" "$REPO" norm-opus-high-ct30 --dir "$SANDBOX/elsewhere/norm-opus-high-ct30" 2>&1)"; rc=$?
if [ "$rc" -eq 3 ] && [ ! -e "$SANDBOX/elsewhere/norm-opus-high-ct30" ]; then
    ok "C26  NEGATIVE: a second workspace in the SAME repository under that name is still refused (exit 3)"
else
    bad "C26  same-repository second workspace (rc=$rc): $OUT"
fi
# The same refusal AT THE REGISTRY, which is where the rule lives: the helper's
# own branch check answers it first, so the registration is asked directly.
OUT="$(python3 "$WS_PY" register-cc --name norm-opus-high-ct30 --repo "$REPO" \
        --path "$SANDBOX/elsewhere/norm-opus-high-ct30" --branch cc/norm-opus-high-ct30 2>&1)"; rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$OUT" | grep -q "already has a workspace in"; then
    ok "C27  and the REGISTRY refuses it by the question it asks — one workspace per repository per name (point 3)"
else
    bad "C27  registry-level same-repository refusal (rc=$rc): $OUT"
fi
OUT="$(python3 "$WS_PY" register-cc --name norm-opus-high-ct30 --repo "$REPO2" \
        --path "$SANDBOX/elsewhere/norm-opus-high-ct30b" --branch cc/norm-opus-high-ct30 2>&1)"; rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$OUT" | grep -q "already has a workspace in"; then
    ok "C27b POSITIVE CONTROL: the refusal is about the REPOSITORY, not the name — the same call for the second repository is refused too, and only because that one is taken"
else
    bad "C27b registry refusal names the repository (rc=$rc): $OUT"
fi
# A FINISHED agent gets no further workspace: it never writes again (point 9).
"$HELPER" "$REPO2" norm-opus-high-ct31 >/dev/null 2>&1; rc=$?
[ "$rc" -eq 0 ] && ok "C28a POSITIVE CONTROL: norm-opus-high-ct31's first workspace is created (exit 0)" \
                || bad "C28a first workspace rc=$rc"
python3 - "$(REC norm-opus-high-ct31)" <<'PY'
import json, sys, time
r = json.load(open(sys.argv[1]))
r["agent_id"] = "a0123456789abcde"; r["tool_use_id"] = "toolu_ct31"; r["spawned_at"] = "now"
r["end"] = {"signal": "SubagentStop", "at": time.time()}
json.dump(r, open(sys.argv[1], "w"))
PY
OUT="$("$HELPER" "$REPO" norm-opus-high-ct31 2>&1)"; rc=$?
if [ "$rc" -eq 3 ] && printf '%s' "$OUT" | grep -q "is finished"; then
    ok "C28  a FINISHED agent gets no further workspace, in any repository (points 5, 7, 9)"
else
    bad "C28  finished agent (rc=$rc): $OUT"
fi

# --- 4b, the repository's own per-worktree setup ----------------------------
# Three cases, because the interesting half is what happens when it goes wrong:
# a setup that fails, and one that hangs, must BOTH leave a usable workspace.
# A repository that shares a build cache this way cannot be allowed to cost a
# teammate its place to stand.
REPO3="$SANDBOX/repo3"
mkdir -p "$REPO3/.richos"
git -C "$REPO3" init -q -b main
printf 'seed\n' >"$REPO3/seed.txt"
printf 'echo "$PWD" > setup-ran.txt\n' >"$REPO3/.richos/worktree-setup"
git -C "$REPO3" add -A
git -C "$REPO3" commit -q -m seed
python3 "$WS_PY" --entity "$REPO3" --session "$RICHOS_SESSION_ID" integration \
    --repo "$REPO3" --branch main --why "the worktree-setup cases" >/dev/null

OUT="$("$HELPER" "$REPO3" zach-opus-high-ct40 2>&1)"; rc=$?
WT3="$SANDBOX/repo3-wt/zach-opus-high-ct40"
if [ "$rc" -eq 0 ] && [ -f "$WT3/setup-ran.txt" ] \
   && [ "$(cat "$WT3/setup-ran.txt" 2>/dev/null)" = "$WT3" ] \
   && printf '%s' "$OUT" | grep -q "^setup: *ok"; then
    ok "C40  .richos/worktree-setup RUNS, with the new worktree as its working directory, and is reported"
else
    bad "C40  setup did not run in $WT3 (rc=$rc): $OUT"
fi

# NEGATIVE HALF ONE: a setup that fails is loud and costs nothing.
git -C "$REPO3" checkout -q -b failing main
printf 'echo "the cache volume is not mounted" >&2\nexit 7\n' >"$REPO3/.richos/worktree-setup"
git -C "$REPO3" commit -q -am "a setup that fails"
OUT="$("$HELPER" "$REPO3" zach-opus-high-ct41 --base failing 2>&1)"; rc=$?
WT3B="$SANDBOX/repo3-wt/zach-opus-high-ct41"
if [ "$rc" -eq 0 ] && [ -d "$WT3B" ] \
   && printf '%s' "$OUT" | grep -q "FAILED (exit 7)" \
   && printf '%s' "$OUT" | grep -q "the cache volume is not mounted"; then
    ok "C41  a FAILING setup still leaves a usable workspace (exit 0), and its output is reported"
else
    bad "C41  failing setup (rc=$rc), workspace present=$([ -d "$WT3B" ] && echo yes || echo no): $OUT"
fi

# NEGATIVE HALF TWO: a setup that hangs is killed, not waited on forever. One
# bad commit must not be able to hang every spawn on the machine.
git -C "$REPO3" checkout -q -b hanging main
# 20s, not 600: long enough that the 2s bound below must do the killing, short
# enough that the `setup-unbounded` mutant — which removes that bound — goes red
# in twenty seconds rather than holding the whole suite for ten minutes.
printf 'sleep 20\n' >"$REPO3/.richos/worktree-setup"
git -C "$REPO3" commit -q -am "a setup that hangs"
OUT="$(WORKTREE_SETUP_TIMEOUT=2 "$HELPER" "$REPO3" zach-opus-high-ct42 --base hanging 2>&1)"; rc=$?
WT3C="$SANDBOX/repo3-wt/zach-opus-high-ct42"
if [ "$rc" -eq 0 ] && [ -d "$WT3C" ] && printf '%s' "$OUT" | grep -q "TIMED OUT after 2s"; then
    ok "C42  a HANGING setup is killed at the bound and the workspace is still created"
else
    bad "C42  hanging setup (rc=$rc): $OUT"
fi

# And a repository with no setup file says so rather than inventing a status.
OUT="$("$HELPER" "$REPO" zach-opus-high-ct43 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$OUT" | grep -q "^setup: *none$"; then
    ok "C43  a repository with no .worktree-setup reports none, and nothing is run"
else
    bad "C43  no-setup repository (rc=$rc): $OUT"
fi

# HUNT PART 4, FINDING 16: a seed that FAILS is a failed creation, never an
# empty successful one. The file .worktreeinclude names is unreadable, so the
# copy fails; the helper used to hide that and print "seeded: 0 file(s)".
REPO4="$SANDBOX/repo4"
mkdir -p "$REPO4"
git -C "$REPO4" init -q -b main
printf '.env.secret\n' >"$REPO4/.worktreeinclude"
printf '.env.secret\n' >"$REPO4/.gitignore"
printf 'seed\n' >"$REPO4/seed.txt"
git -C "$REPO4" add -A
git -C "$REPO4" commit -q -m seed
python3 "$WS_PY" --entity "$REPO4" --session "$RICHOS_SESSION_ID" integration \
    --repo "$REPO4" --branch main --why "the seed-failure case" >/dev/null
printf 'TOKEN=1\n' >"$REPO4/.env.secret"
chmod 000 "$REPO4/.env.secret"
OUT="$("$HELPER" "$REPO4" zach-opus-high-ct44 2>&1)"; rc=$?
chmod 600 "$REPO4/.env.secret"
if [ "$rc" -eq 4 ] && printf '%s' "$OUT" | grep -q "seeding the .worktreeinclude files" \
   && printf '%s' "$OUT" | grep -q ".env.secret" \
   && ! printf '%s' "$OUT" | grep -q "^seeded:" \
   && python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); assert r.get("creation_failed"), r' "$(REC zach-opus-high-ct44)" 2>/dev/null; then
    ok "C44  a FAILED seed is a failed creation (exit 4, recorded, the unreadable file named), never \"seeded: 0\""
else
    bad "C44  seed failure (rc=$rc): $OUT"
fi
# and the positive control, same repository, once the file can be read
OUT="$("$HELPER" "$REPO4" zach-opus-high-ct44b 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$OUT" | grep -q 'seeded:     1 file(s)' \
   && [ -f "$SANDBOX/repo4-wt/zach-opus-high-ct44b/.env.secret" ]; then
    ok "C44b POSITIVE CONTROL: the readable file is seeded and counted"
else
    bad "C44b seed control (rc=$rc): $OUT"
fi

# HUNT PART 4, FINDING 17: a timed-out setup is stopped WITH what it started.
# The setup starts a child that outlives it, records the child's PID, and hangs;
# the kill used to reach the setup's own PID only.
git -C "$REPO3" checkout -q -b spawning main
cat >"$REPO3/.richos/worktree-setup" <<'SETUPEOF'
sleep 30 &
echo "$!" > "$PWD/child.pid"
sleep 20
SETUPEOF
git -C "$REPO3" commit -q -am "a setup that starts a child and hangs"
OUT="$(WORKTREE_SETUP_TIMEOUT=2 "$HELPER" "$REPO3" zach-opus-high-ct45 --base spawning 2>&1)"; rc=$?
WT3D="$SANDBOX/repo3-wt/zach-opus-high-ct45"
CHILD="$(cat "$WT3D/child.pid" 2>/dev/null || true)"
ALIVE=no
if [ -n "$CHILD" ] && kill -0 "$CHILD" 2>/dev/null; then
    case "$(ps -o stat= -p "$CHILD" 2>/dev/null)" in *Z*) ;; *) ALIVE=yes ;; esac
fi
if [ "$rc" -eq 0 ] && [ -n "$CHILD" ] && [ "$ALIVE" = no ] \
   && printf '%s' "$OUT" | grep -q "TIMED OUT after 2s and was killed, with every process it started"; then
    ok "C45  a timed-out setup is killed WITH the child it started, and the summary says so"
else
    bad "C45  setup child (rc=$rc, child=${CHILD:-none}, alive=$ALIVE): $OUT"
fi
# the suite's own leftover, by the PID the setup it ran recorded, never by name
[ "$ALIVE" = yes ] && kill -9 "$CHILD" 2>/dev/null

# HUNT PART 4, FINDING 18: the setup bound fits inside the CALLER's deadline.
# The app gives the whole preparation 120 s and the setup alone was allowed
# 120 s; a caller now says when it gives up (RICHOS_OPERATION_DEADLINE, epoch
# seconds) and the setup is bounded by what is left of that, less the time
# the confirmation needs. Default setup bound (120 s), a 20 s hanging setup,
# and a deadline 14 s out: the creation must finish well inside 14 s.
T0="$(date +%s)"
OUT="$(RICHOS_OPERATION_DEADLINE="$((T0 + 14))" "$HELPER" "$REPO3" zach-opus-high-ct46 --base hanging 2>&1)"; rc=$?
T1="$(date +%s)"
if [ "$rc" -eq 0 ] && [ "$((T1 - T0))" -lt 14 ] && [ -d "$SANDBOX/repo3-wt/zach-opus-high-ct46" ] \
   && printf '%s' "$OUT" | grep -q "the caller's deadline"; then
    ok "C46  the setup is bounded by the caller's deadline, and the creation answers inside it ($((T1 - T0))s < 14s)"
else
    bad "C46  caller deadline (rc=$rc, took $((T1 - T0))s): $OUT"
fi
OUT="$(RICHOS_OPERATION_DEADLINE="$(($(date +%s) - 1))" "$HELPER" "$REPO3" zach-opus-high-ct47 2>&1)"; rc=$?
if [ "$rc" -eq 3 ] && [ ! -e "$SANDBOX/repo3-wt/zach-opus-high-ct47" ] && printf '%s' "$OUT" | grep -q "deadline"; then
    ok "C47  a caller whose deadline has already passed gets a refusal and nothing is created"
else
    bad "C47  passed deadline (rc=$rc): $OUT"
fi

if ! grep -qE 'worktree (remove|prune)|branch -D|rm -rf' "$HELPER"; then
    ok "C23  the helper contains no deletion: land and discard are the only deleters (points 4, 7)"
else
    bad "C23  the helper deletes something: $(grep -nE 'worktree (remove|prune)|branch -D|rm -rf' "$HELPER" | head -3)"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== create-teammate-worktree tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== create-teammate-worktree tests: all $PASS passed ==="

# The mutation harness is part of this suite's definition of green: a suite
# nobody has watched go red proves nothing (open-items rows 3.22-3.29). It is watched
# ONLY BEFORE NIGHTLIES (hunt part 4 finding 19, 2026-09-30; the fourteen-point suite's
# ruling of 2026-09-23): the nightly's gates/workspace-mutants runs this unit with
# RICHOS_MUTATION_PASSES=1. Anywhere else the pass says NOT RUN and why.
if [ -f "$SCRIPT_DIR/create-teammate-worktree.mutation.sh" ]; then
    if [ "${RICHOS_MUTATION_PASSES:-}" = 1 ]; then
        bash "$SCRIPT_DIR/create-teammate-worktree.mutation.sh" || exit 1
    else
        echo "  NOT RUN  the mutation harness: it runs before each nightly (hunt part 4 finding 19); RICHOS_MUTATION_PASSES=1 runs it here"
    fi
fi
exit 0
