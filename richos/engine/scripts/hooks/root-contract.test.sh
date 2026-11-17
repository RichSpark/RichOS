#!/usr/bin/env bash
#
# root-contract.test.sh — END-TO-END proof that every wired hook resolves the
# SESSION's repository, not the engine's, and fails loud when it cannot.
#
# scripts/lib/resolve-roots.test.sh tests the resolver in isolation. This file
# tests the thing that actually matters: each HOOK, invoked exactly as the host
# invokes it, against the real topology that broke them —
#
#     <hostrepo>/                       a repository that merely CONTAINS the engine
#       engine/                         ENGINE_ROOT (the plugin)
#     <sessionrepo>/                    the repository the session is in  <- must win
#       orchestration.config            PROTECTED_PATHS="src"
#       .claude/agents/                 its own roster
#     <plainrepo>/                      adopted by nobody
#
# Every case is a PAIR. The positive arm proves the hook does the right thing
# for the session's repo; the negative arm proves it does NOT do it for the
# engine's repo — asserted by naming the wrong root explicitly, because
# "didn't block" and "blocked the wrong path" are different failures and only
# one of them is the regression.
#
# Run directly: scripts/hooks/root-contract.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ENGINE="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t root-contract.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

# The launching session's own project dir must not leak in as a candidate.
unset CLAUDE_PROJECT_DIR RICHOS_ENTITY_ROOT RICHOS_ENGINE_ROOT CLAUDE_PLUGIN_ROOT

# ===========================================================================
# THE OPERATOR'S RECORD IS OUT OF REACH, AND BOTH ARMS SAY SO (round 15,
# 2026-09-11 — Frank D2). Section 6 runs the SHIPPED SessionStart hooks; a
# suite carved out of this one wrote a false `terminated` witness into the
# operator's real ledger on 2026-09-11 for a locked, running agent, because
# HOME was never moved. The real record's witness is taken first, then HOME
# moves, then every path a hook resolves under it is named explicitly too --
# the workspace registry included, and the registration's session identity is
# a process of this suite's own. 6c proves the redirect is honored; 6d proves
# the real record did not move.
# ===========================================================================
# shellcheck source=../lib/record-canary.sh
. "$SRC_ENGINE/scripts/lib/record-canary.sh"
rc_baseline "$SANDBOX/real-record-baseline.txt"
REAL_HOME="$HOME"
export HOME="$SANDBOX/home"
mkdir -p "$HOME/.claude/state" "$HOME/.claude/teams" "$HOME/.claude/projects" "$HOME/.claude/sessions"
if [ -f "$REAL_HOME/.gitconfig" ]; then
    cp "$REAL_HOME/.gitconfig" "$HOME/.gitconfig"
else
    export GIT_AUTHOR_NAME="fixture" GIT_AUTHOR_EMAIL="fixture@example.invalid" \
           GIT_COMMITTER_NAME="fixture" GIT_COMMITTER_EMAIL="fixture@example.invalid"
fi
export CLAUDE_CONFIG_DIR="$HOME/.claude"
export REAP_WORKTREE_LEDGER="$HOME/.claude/state/worktree-ledger.jsonl"
export RICHOS_WORKTREE_LEDGER="$REAP_WORKTREE_LEDGER"
export REAP_TEAM_DIR="$HOME/.claude/teams" RICHOS_TEAMS_DIR="$HOME/.claude/teams" \
       WORKER_EVENTS_TEAMS_DIR="$HOME/.claude/teams" RICHOS_LIVENESS_TEAMS_DIR="$HOME/.claude/teams"
export REAP_LEDGER="$HOME/.claude/state/reap-known-repos.txt"
export REAP_PROJECTS_DIR="$HOME/.claude/projects" RICHOS_PROJECTS_DIR="$HOME/.claude/projects"
export RICHOS_SESSIONS_DIR="$HOME/.claude/sessions"
export RICHOS_WORKSPACES_DIR="$HOME/.claude/state/workspaces"
RICHOS_SESSION_PID="$(sh -c 'sleep 600 >/dev/null 2>&1 & echo $!')"
export RICHOS_SESSION_PID
trap 'kill "$RICHOS_SESSION_PID" 2>/dev/null; rm -rf "$SANDBOX"' EXIT
export RICHOS_WORKTREE_CAPTURE_DIR="$HOME/.claude/state/worktree-captures"

# --- build the topology ----------------------------------------------------
mk_repo() {
    local r="$SANDBOX/$1"
    mkdir -p "$r"
    git -C "$r" init -q -b main
    printf 'seed\n' >"$r/seed.txt"
    git -C "$r" add -A && git -C "$r" commit -q -m seed
    printf '%s\n' "$r"
}

HOSTREPO="$(mk_repo hostrepo)"
ENGINE="$HOSTREPO/engine"
mkdir -p "$ENGINE"
# A real engine, copied wholesale — no stubs. A stubbed engine could pass these
# tests while the shipped one fails, which is the same wrong-reason pass this
# suite exists to prevent.
cp -R "$SRC_ENGINE/scripts" "$ENGINE/scripts"
cp -R "$SRC_ENGINE/mega-lander" "$ENGINE/mega-lander"
cp -R "$SRC_ENGINE/ass-kicker" "$ENGINE/ass-kicker"
cp -R "$SRC_ENGINE/.claude" "$ENGINE/.claude"
cp -R "$SRC_ENGINE/.claude-plugin" "$ENGINE/.claude-plugin"
cp "$SRC_ENGINE/orchestration.config" "$ENGINE/orchestration.config"
cp "$SRC_ENGINE/VERSION" "$ENGINE/VERSION" 2>/dev/null || echo "0.0.0-test" >"$ENGINE/VERSION"
# The engine's own config protects "app packages" — deliberately DIFFERENT from
# the session repo's "src", so a hook that loaded the wrong config guards the
# wrong directory names and the tests can tell which one it read.
git -C "$HOSTREPO" add -A >/dev/null 2>&1
git -C "$HOSTREPO" commit -q -m engine >/dev/null 2>&1

SESSREPO="$(mk_repo sessionrepo)"
printf 'PROTECTED_PATHS="src"\nREADONLY_ALLOWLIST="Explore Plan"\nALLOWED_MODELS="opus sonnet haiku"\n' >"$SESSREPO/orchestration.config"
mkdir -p "$SESSREPO/.claude/agents" "$SESSREPO/src" "$SESSREPO/app"
printf -- '---\nname: mark\nmodel: opus\n---\nbody\n' >"$SESSREPO/.claude/agents/mark.md"
git -C "$SESSREPO" add -A && git -C "$SESSREPO" commit -q -m adopt

PLAINREPO="$(mk_repo plainrepo)"
mkdir -p "$PLAINREPO/src"

HOOKS="$ENGINE/scripts/hooks"

# run <hook> <payload> [env...] -> sets RC, OUT (stdout+stderr merged)
run() {
    local hook="$1" payload="$2"; shift 2
    # cd into the marker-free, non-git sandbox first. $PWD is the resolver's
    # last-resort candidate and this suite is normally run FROM the engine
    # directory, which is itself adopted — left alone, every "unadopted repo"
    # case below would resolve the engine through $PWD and report enforcement
    # ON, i.e. pass while asserting the opposite of what it claims.
    OUT="$(cd "$SANDBOX" && printf '%s' "$payload" | env "$@" bash "$HOOKS/$hook" 2>&1)"
    RC=$?
}

echo "=== root contract, end to end: engine in one repo, session in another ==="
echo ""

# ===========================================================================
# 1. guard-main-checkout-writes.sh — protects the SESSION repo's trees.
# ===========================================================================
W_SESS='{"tool_name":"Write","cwd":"'"$SESSREPO"'","tool_input":{"file_path":"'"$SESSREPO"'/src/x.js"}}'
run guard-main-checkout-writes.sh "$W_SESS" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "$SESSREPO"; then
    ok "1a POSITIVE  blocks a main-checkout write to the SESSION repo's protected tree"
else
    bad "1a session-repo write not blocked (rc=$RC out=${OUT:0:200})"
fi

# 1b NEGATIVE — the session repo also has an `app/` directory, which is
# protected in the ENGINE's config and NOT in the session's. A hook reading the
# wrong config would block it. This is the discriminator: it can only pass if
# the config came from the right root.
W_APP='{"tool_name":"Write","cwd":"'"$SESSREPO"'","tool_input":{"file_path":"'"$SESSREPO"'/app/x.js"}}'
run guard-main-checkout-writes.sh "$W_APP" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 0 ]; then
    ok "1b NEGATIVE  does NOT block app/ — that tree is protected in the ENGINE's config, not the session's"
else
    bad "1b read the ENGINE's PROTECTED_PATHS (blocked app/ with rc=$RC)"
fi

# 1c NEGATIVE — a write into the ENGINE's own protected tree, from a session in
# the session repo, is not this session's business.
W_ENG='{"tool_name":"Write","cwd":"'"$SESSREPO"'","tool_input":{"file_path":"'"$ENGINE"'/app/x.js"}}'
run guard-main-checkout-writes.sh "$W_ENG" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 0 ]; then
    ok "1c NEGATIVE  does not guard the ENGINE's own tree from the session's repo"
else
    bad "1c guarded the engine's tree (rc=$RC)"
fi

# 1d — an unadopted repository: stand down, never block.
W_PLAIN='{"tool_name":"Write","cwd":"'"$PLAINREPO"'","tool_input":{"file_path":"'"$PLAINREPO"'/src/x.js"}}'
run guard-main-checkout-writes.sh "$W_PLAIN" "CLAUDE_PROJECT_DIR=$PLAINREPO"
if [ "$RC" -eq 0 ]; then
    ok "1d POSITIVE  stands down in an unadopted repository (never bricks an unrelated project)"
else
    bad "1d blocked in an unadopted repo (rc=$RC out=${OUT:0:200})"
fi

# 1e — a DECLARED root that is not an engine root: BLOCK, loudly.
run guard-main-checkout-writes.sh "$W_SESS" "CLAUDE_PROJECT_DIR=$SESSREPO" "RICHOS_ENTITY_ROOT=$PLAINREPO"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'ROOT RESOLUTION FAILURE'; then
    ok "1e POSITIVE  a broken declared root BLOCKS with the loud banner"
else
    bad "1e broken root did not block loudly (rc=$RC out=${OUT:0:200})"
fi

# ===========================================================================
# 2. guard-bash-main-writes.sh
# ===========================================================================
B_SESS='{"tool_name":"Bash","cwd":"'"$SESSREPO"'","tool_input":{"command":"echo hi > '"$SESSREPO"'/src/x.js"}}'
run guard-bash-main-writes.sh "$B_SESS" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 2 ]; then
    ok "2a POSITIVE  blocks a Bash write into the SESSION repo's protected tree"
else
    bad "2a session-repo bash write not blocked (rc=$RC out=${OUT:0:200})"
fi
B_APP='{"tool_name":"Bash","cwd":"'"$SESSREPO"'","tool_input":{"command":"echo hi > '"$SESSREPO"'/app/x.js"}}'
run guard-bash-main-writes.sh "$B_APP" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 0 ]; then
    ok "2b NEGATIVE  does not block app/ (the ENGINE's protected tree, not the session's)"
else
    bad "2b read the ENGINE's config (rc=$RC)"
fi

# ===========================================================================
# 3. guard-worktree-isolation.sh — the model-truthfulness clause, and the
#    namespaced subagent_type that used to defeat it silently.
# ===========================================================================
# Clause 7 registers every allowed file-capable spawn; pin the workspace
# registry into the sandbox so this suite never touches the record. Point 14:
# since eedfbc7d (2026-09-12) that registration REFUSES a repository with no
# recorded body of work, so the recording comes first — otherwise 3a, 3d and 3f
# read clause 7's refusal and report it as the model clause blocking a truthful
# spawn.
export RICHOS_WORKSPACES_DIR="$SANDBOX/ws"
python3 "$SRC_ENGINE/mega-lander/workspaces.py" --entity "$SESSREPO" --session deadbeef-0000 \
    integration --repo "$SESSREPO" --branch main --why "the root-contract suite's body of work" >/dev/null
SPAWN() { # <subagent_type> <name> [model] [prompt]
    local st="$1" nm="$2" md="${3:-}" pr="${4:-}"
    printf '{"tool_name":"Agent","cwd":"%s","session_id":"deadbeef-0000","tool_use_id":"toolu_root_contract","tool_input":{"subagent_type":"%s","name":"%s","effort":"high","isolation":"worktree"%s%s}}' \
        "$SESSREPO" "$st" "$nm" \
        "$([ -n "$md" ] && printf ',"model":"%s"' "$md")" \
        "$([ -n "$pr" ] && printf ',"prompt":"%s"' "$pr")"
}
# mark.md in the SESSION repo declares model: opus.
run guard-worktree-isolation.sh "$(SPAWN mark mark-opus-high-1)" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 0 ]; then
    ok "3a POSITIVE  a truthful spawn against the SESSION roster is allowed"
else
    bad "3a truthful spawn blocked (rc=$RC out=${OUT:0:300})"
fi
# 3b — the untruthful one must be caught, which is only possible if the guard
# found the session repo's mark.md at all.
run guard-worktree-isolation.sh "$(SPAWN mark mark-sonnet-high-1)" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'untruthful model token'; then
    ok "3b POSITIVE  an untruthful model token is caught using the SESSION repo's roster"
else
    bad "3b untruthful token not caught (rc=$RC out=${OUT:0:300})"
fi
# 3c — THE NAMESPACED FAIL-OPEN. `richos-engine:clark` resolves to the ENGINE's
# clark.md (model: sonnet), so an "opus" token must now be caught. Before the
# fix the lookup stat'd a path containing a colon, found nothing, and accepted.
CLARK_MODEL="$(awk -F': *' '/^model:/{print $2; exit}' "$ENGINE/.claude/agents/clark.md" 2>/dev/null | tr -d '"'"'"' ')"
if [ -n "$CLARK_MODEL" ]; then
    WRONG=opus; [ "$CLARK_MODEL" = "opus" ] && WRONG=haiku
    run guard-worktree-isolation.sh "$(SPAWN richos-engine:clark "clark-$WRONG-high-1")" "CLAUDE_PROJECT_DIR=$SESSREPO"
    if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'untruthful model token'; then
        ok "3c POSITIVE  a NAMESPACED type is checked against the ENGINE roster (clark is $CLARK_MODEL, '$WRONG' rejected)"
    else
        bad "3c namespaced type not checked (rc=$RC out=${OUT:0:300})"
    fi
    run guard-worktree-isolation.sh "$(SPAWN richos-engine:clark "clark-$CLARK_MODEL-high-1")" "CLAUDE_PROJECT_DIR=$SESSREPO"
    if [ "$RC" -eq 0 ]; then
        ok "3d NEGATIVE  the TRUTHFUL namespaced spawn is still allowed (3c is a real check, not a blanket block)"
    else
        bad "3d truthful namespaced spawn blocked (rc=$RC out=${OUT:0:300})"
    fi
else
    bad "3c/3d could not read a model: line from the engine's clark.md"
fi
# 3e — an unresolvable NAMESPACE fails loud rather than degrading to accept.
run guard-worktree-isolation.sh "$(SPAWN ghost-plugin:nobody nobody-opus-high-1)" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'cannot verify the model token'; then
    ok "3e POSITIVE  an unresolvable namespaced type is REFUSED, not waved through"
else
    bad "3e unresolvable namespace not refused (rc=$RC out=${OUT:0:300})"
fi
# 3f NEGATIVE — a bare host built-in still passes CLAUSE 2b; 3e is a
# discrimination, not a blanket rule.
# The clause-5 staffing hatch is supplied here on purpose. general-purpose is a
# GENERIC type, so since 2026-09-02 it needs a staffing justification — a
# different clause, and one this case is not about. Carrying the hatch keeps 3f
# asserting exactly one thing: an unresolvable NAMESPACE is refused by the
# model-truthfulness clause while a bare built-in with no definition is not.
run guard-worktree-isolation.sh \
    "$(SPAWN general-purpose gp-sonnet-high-1 '' 'generic-agent: a bare host built-in with no roster definition, exercised here for the model-truthfulness clause')" \
    "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 0 ]; then
    ok "3f NEGATIVE  a bare built-in type with no definition is still allowed"
else
    bad "3f built-in blocked (rc=$RC out=${OUT:0:300})"
fi

# ===========================================================================
# 4. guard-definition-drift.sh — the guard that wrote state into the wrong repo.
# ===========================================================================
rm -rf "$HOSTREPO/.claude/state" "$ENGINE/.claude/state" "$SESSREPO/.claude/state"
run guard-definition-drift.sh "$(SPAWN mark mark-opus-high-1)" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ -d "$SESSREPO/.claude/state" ]; then
    ok "4a POSITIVE  drift state is written into the SESSION repo"
else
    bad "4a no state written to the session repo (rc=$RC out=${OUT:0:200})"
fi
# 4b NEGATIVE — the exact step-1 defect, asserted by name.
if [ ! -d "$HOSTREPO/.claude/state" ] && [ ! -d "$ENGINE/.claude/state" ]; then
    ok "4b NEGATIVE  NOTHING was written into the engine's repo or the engine itself"
else
    bad "4b state leaked into the engine's repository (the step-1 defect)"
fi

# ===========================================================================
# 5. snapshot-agent-definitions.sh — the silent skip.
# ===========================================================================
SS='{"session_id":"cafe1234-0000","cwd":"'"$SESSREPO"'","hook_event_name":"SessionStart"}'
run snapshot-agent-definitions.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ -f "$SESSREPO/.claude/state/agent-definitions-cafe1234.snapshot" ] \
   && grep -q '\.claude/agents/mark\.md' "$SESSREPO/.claude/state/agent-definitions-cafe1234.snapshot"; then
    ok "5a POSITIVE  snapshots the SESSION repo's roster, session-scoped"
else
    bad "5a no session-scoped snapshot of the session roster (out=${OUT:0:300})"
fi
# 5b — the not-adopted case says what it is, in words, instead of "skipped".
SSP='{"session_id":"cafe1234-0000","cwd":"'"$PLAINREPO"'","hook_event_name":"SessionStart"}'
run snapshot-agent-definitions.sh "$SSP" "CLAUDE_PROJECT_DIR=$PLAINREPO"
if printf '%s' "$OUT" | grep -q 'has not adopted the engine'; then
    ok "5b POSITIVE  an unadopted repo is reported as a stand-down, not as 'skipped'"
else
    bad "5b stand-down wording (out=${OUT:0:300})"
fi
# 5c NEGATIVE — the old wording is gone. It is the word that made a real failure
# read as routine, so its absence is worth asserting directly.
if ! printf '%s' "$OUT" | grep -q 'snapshot: skipped'; then
    ok "5c NEGATIVE  the ambiguous 'snapshot: skipped' wording is gone"
else
    bad "5c still emits 'snapshot: skipped'"
fi
# 5d — a BROKEN root screams and says the partner guard will fail open.
run snapshot-agent-definitions.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO" "RICHOS_ENTITY_ROOT=$PLAINREPO"
if printf '%s' "$OUT" | grep -q 'ROOT RESOLUTION FAILURE' \
   && printf '%s' "$OUT" | grep -q 'fail OPEN'; then
    ok "5d POSITIVE  a broken root screams AND names the downstream consequence"
else
    bad "5d broken-root output (out=${OUT:0:400})"
fi

# ===========================================================================
# 6. workspace-lifecycle.sh on SessionStart — the OTHER silent skip. The
#    registry library is an ENGINE asset; the session's repository is the
#    ENTITY. One variable cannot be both. (Until 2026-09-11 this section proved
#    the same split for the SessionStart reaper, which the workspace spec
#    removed; docs/plans/worktree-spec-2026-09-11.md.)
# ===========================================================================
WS_SESSION="$RICHOS_WORKSPACES_DIR/sessions/cafe1234-0000.json"
run workspace-lifecycle.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ -f "$WS_SESSION" ] \
   && python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); sys.exit(0 if r.get("repo")==sys.argv[2] else 1)' "$WS_SESSION" "$SESSREPO" \
   && ! printf '%s' "$OUT" | grep -q 'is unavailable'; then
    ok "6a POSITIVE  finds the ENGINE's registry library and records the SESSION's repo"
else
    bad "6a lifecycle root split (session record: $(cat "$WS_SESSION" 2>/dev/null | head -c 300); out=${OUT:0:400})"
fi
# 6b — a broken root does not quietly do nothing.
run workspace-lifecycle.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO" "RICHOS_ENTITY_ROOT=$PLAINREPO"
if printf '%s' "$OUT" | grep -q 'ROOT RESOLUTION FAILURE'; then
    ok "6b POSITIVE  a broken root is reported as a failure, not as a skip"
else
    bad "6b broken-root lifecycle output (out=${OUT:0:400})"
fi
# 6c — THE REDIRECT IS HONORED: with no explicit registry, the hook resolves it
# under the SANDBOX config directory, never the operator's.
rm -f "$WS_SESSION"
run workspace-lifecycle.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO" "RICHOS_WORKSPACES_DIR="
if [ -f "$HOME/.claude/state/workspaces/sessions/cafe1234-0000.json" ]; then
    ok "6c POSITIVE  the registry resolved under the SANDBOX home's config directory — the redirect is honored, not assumed"
else
    bad "6c the session was not recorded under the sandbox config directory (out=${OUT:0:600})"
fi

# ===========================================================================
# 7. engine-status.sh — the answer to "is this defense on?"
# ===========================================================================
run engine-status.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO"
if printf '%s' "$OUT" | grep -q 'ACTIVE' && printf '%s' "$OUT" | grep -q "$SESSREPO"; then
    ok "7a POSITIVE  announces ACTIVE and names the governed repository"
else
    bad "7a status ACTIVE (out=${OUT:0:400})"
fi
run engine-status.sh "$SSP" "CLAUDE_PROJECT_DIR=$PLAINREPO"
if printf '%s' "$OUT" | grep -q 'STOOD DOWN' && printf '%s' "$OUT" | grep -q 'NOT adopted'; then
    ok "7b POSITIVE  announces the stand-down explicitly in an unadopted repo"
else
    bad "7b status STOOD DOWN (out=${OUT:0:400})"
fi
run engine-status.sh "$SS" "CLAUDE_PROJECT_DIR=$SESSREPO" "RICHOS_ENTITY_ROOT=$PLAINREPO"
if printf '%s' "$OUT" | grep -q 'ENFORCEMENT IS NOT ACTIVE'; then
    ok "7c POSITIVE  announces a broken root as ENFORCEMENT IS NOT ACTIVE"
else
    bad "7c status BROKEN (out=${OUT:0:400})"
fi
# 7d NEGATIVE — the stand-down must never be reported as ACTIVE. Asserted
# separately because "said something" and "said the right thing" differ.
run engine-status.sh "$SSP" "CLAUDE_PROJECT_DIR=$PLAINREPO"
if ! printf '%s' "$OUT" | grep -q 'Enforcement is ON'; then
    ok "7d NEGATIVE  a stand-down is never announced as enforcement being on"
else
    bad "7d stand-down announced as ON"
fi

# ===========================================================================
# 8. scan-secrets.sh still works through the resolved root (it reads only
#    config, so the risk is a silent stand-down).
# ===========================================================================
SEC='{"tool_name":"Write","cwd":"'"$SESSREPO"'","tool_input":{"file_path":"'"$SESSREPO"'/notes.txt","content":"aws_key = \"AKIAIOSFODNN7EXAMPLE\"\nghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\n"}}'
run scan-secrets.sh "$SEC" "CLAUDE_PROJECT_DIR=$SESSREPO"
if [ "$RC" -eq 2 ]; then
    ok "8a POSITIVE  the secret scanner still fires in the governed repo"
else
    bad "8a secret scanner did not fire (rc=$RC out=${OUT:0:300})"
fi
run scan-secrets.sh "${SEC//$SESSREPO/$PLAINREPO}" "CLAUDE_PROJECT_DIR=$PLAINREPO"
if [ "$RC" -eq 0 ]; then
    ok "8b POSITIVE  and stands down in an unadopted repo"
else
    bad "8b secret scanner blocked in an unadopted repo (rc=$RC)"
fi

# ===========================================================================
# 9. NO SessionStart HOOK MAY BLOCK ON STDIN — MOVED (round 14, 2026-09-11)
#
# Cases 9a-9k live in scripts/hooks/session-start-stdin.test.sh now, with the
# same sandbox topology and the same two-armed hang check. They were 36 s of
# this suite's 61 s inner run, and the mutation harness below replays the
# inner run for every root-contract mutant, so this file took 713 s on a
# shared machine (Sage, round three) — over the 600 s ceiling of a single
# harness call, which is the door a barred reviewer was restarted through.
# Split, each half fits one call with room; nothing is skipped and nothing
# is checked less often: run-all-tests and CI discover *.test.sh, and M10
# (the 92 s stdin hang) is still proven load-bearing by root-contract.mutation.sh,
# now against the suite that owns case 9c.
# ===========================================================================

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# Until 2026-09-05 it ran from NOTHING. run-all-tests.sh discovers *.test.sh;
# contract-integrity.test.sh names a hand-typed set of harnesses that never
# included this one; and the only other mention of the file anywhere in the
# repository was a comment. So every property it proves load-bearing was
# proven exactly when somebody remembered to type its path. Recorded as row
# 3.25 of wiki/open-items.md.
#
# ITS FAILURE IS THIS SUITE'S FAILURE, not a warning. A harness whose result is
# advisory is a harness nobody has to fix.
#
# RICHOS_MUTATION_INNER is the only thing standing between this and an infinite
# regress: the harness EXPORTS it before running any copy of this suite, so the
# inner run skips this block. Never remove one half without the other.
# A MISSING HARNESS IS A FAILURE, NOT A SKIP, and that sentence was earned
# forty minutes ago rather than reasoned. The first draft of this block wrote
# the directory variable with one dollar sign too many, so the path expanded to
# the shell PID followed by the variable name, the test was false, the harness was
# skipped in silence and the suite printed a full green tally in 3.7 seconds.
# That is the exact defect this whole row is about, rebuilt inside the fix for
# it, and only the WALL CLOCK gave it away. So the guard now says so out loud.

# 6d — THE OPERATOR'S REAL RECORD DID NOT MOVE (round 15): the negative arm of
# the redirect, checked against the real paths captured before HOME moved.
TOUCHED="$(rc_escaped "$SANDBOX/real-record-baseline.txt")"
if [ "$RC_HEALTHY" -eq 1 ] && [ -z "$TOUCHED" ]; then
    ok "6d NEGATIVE  the operator's real record under $RC_CFG is untouched by this suite (ledger rows except finished, fallback event log, team directory entries)"
else
    bad "6d the operator's real record CHANGED during this suite (healthy=$RC_HEALTHY): $TOUCHED"
fi

if [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
    echo ""
    echo "=== running the mutation harness: root-contract.mutation.sh ==="
    if [ -x "$SCRIPT_DIR/root-contract.mutation.sh" ]; then
        "$SCRIPT_DIR/root-contract.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. root-contract.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== root-contract end-to-end: $FAIL FAILED, $PASS passed ==="
    exit 1
else
    echo "=== root-contract end-to-end: all $PASS passed ==="
    exit 0
fi
