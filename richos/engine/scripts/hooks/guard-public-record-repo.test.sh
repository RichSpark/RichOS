#!/usr/bin/env bash
#
# guard-public-record-repo.test.sh — regression tests for
# scripts/hooks/guard-public-record-repo.sh.
#
# THE FIXTURE IS THE REAL SPAWN PROMPT OF 2026-09-19 — the one that sent a
# research read into the PUBLIC repository and put its ledger in published
# history. Every quoted block below is copied from
# .../scratchpad/spawn-r1.prompt.md and from the brief it carries,
# richos-hq/docs/briefs/reed-brief-t3code-tooling-adoption-read-2026-09-19.md.
#
# Covered here:
#
#   (a) REFUSES that dispatch as issued: its workspace resolves to a
#       repository carrying a publication declaration and its deliverable is
#       `docs/research/...`;
#   (b) PASSES THE SAME PROMPT aimed at the private record repository — the
#       only thing that changes is the cross-repo-worktree: line;
#   (c) the PRIOR-ART citations in that same prompt are not deliverables:
#       `richos-hq/docs/research/...` is prefixed and
#       `wiki/ceo-decisions.md` carries no delivery verb;
#   (d) the COMMIT surface: a commit ADDING docs/research/x.md in a
#       publication-bound fixture is REFUSED; the same commit ADDING
#       docs/verification/x.md PASSES, because the CEO's ruling was "remove
#       the two ledger commits and nothing else";
#   (e) a MODIFIED record file is not refused — it is already in history;
#   (f) THE HATCH: `public-record-ack: <30+ characters>` permits and is
#       logged; a bare or short marker exempts nothing and is refused by name;
#   (g) FAIL OPEN: a repository with NO publication declaration, a call for
#       another tool, an unparseable payload, and a declared off switch all
#       allow in silence; a workspace path that resolves to no repository at
#       all ALLOWS and ANNOUNCES;
#   (h) the one hard refusal: a missing scripts/lib/resolve-roots.sh.
#
# ===========================================================================
# MEASURED BEFORE IT WAS WIRED
# ===========================================================================
# 302 real documents — every spawn prompt in the ordering session's scratchpad
# and every brief in richos-hq/docs/briefs — driven through the guard as Agent
# payloads. ONE refused: spawn-r1.prompt.md, on
# docs/research/t3code-tooling-what-to-adopt-2026-09-19.md. Nothing else fired
# and nothing was announced.
#
# ONE NARROWING, measured. The delivery-verb list first carried land, lands,
# output, save and saves, and the same prompt's PRIOR-ART line matched on
# "lands" — "8 of 19 lands on richos main were our own tooling", one sentence
# after a bare docs/research/ citation. A verb here must name the production
# of THIS dispatch's own artifact.
#
# Run directly: scripts/hooks/guard-public-record-repo.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-public-record-repo.sh"

unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0

SB="$(cd "$(mktemp -d -t guard-public-record-repo.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SB"' EXIT

ENTITY="$SB/entity"
mkdir -p "$ENTITY/.claude/state"
printf 'PROTECTED_PATHS=""\n' >"$ENTITY/orchestration.config"

# --- TWO REPOSITORIES, ONE PUBLIC AND ONE NOT ------------------------------
# The public one carries a publication declaration in the same place and the
# same format richos does. The private one carries none, which is exactly how
# richos-hq declares itself private: by declaring nothing.
PUBLIC="$SB/public-repo"
PRIVATE="$SB/private-repo"
mkdir -p "$PUBLIC/.richos" "$PUBLIC/docs/research" "$PUBLIC/docs/verification" "$PRIVATE/docs/research"
git -C "$PUBLIC" init -q 2>/dev/null
git -C "$PRIVATE" init -q 2>/dev/null
git -C "$PUBLIC" config user.email t@example.com
git -C "$PUBLIC" config user.name Test
{
    printf 'PRIVATE_RECORD="private-repo — the private record repository"\n'
    printf 'PRIVATE_SOURCES="../private-repo"\n'
    printf 'MIN_SPEECH_LINES="8"\n'
    printf 'MIN_QUOTE_WORDS="10"\n'
    printf 'ALLOWLIST=""\n'
} >"$PUBLIC/.richos/publication-boundary"
printf 'seed\n' >"$PUBLIC/seed.txt"
git -C "$PUBLIC" add -A >/dev/null 2>&1
git -C "$PUBLIC" commit -q -m seed --no-verify >/dev/null 2>&1

# The workspaces the prompts name. They EXIST here; the guard's own
# <repo>-wt/<name> fallback for a not-yet-created workspace is case G5.
PUB_WS="$PUBLIC/.claude/worktrees/agent-1"
PRIV_WS="$PRIVATE/.claude/worktrees/agent-1"
mkdir -p "$PUB_WS" "$PRIV_WS"

# ===========================================================================
# THE FIXTURE — the real prompt of 2026-09-19
# ===========================================================================
# Its first line is the workspace; then the prior-art paragraph, whose
# citations must NOT read as deliverables; then the heading that names what
# this dispatch actually produces.
prompt_for() {
    cat <<FIX
cross-repo-worktree: $1

# Brief: read T3 Code's build, test, release, update and harness tooling in full, and say what RichOS can adopt instead of build

**CEO, verbatim, 2026-09-19 22:50Z:** Earlier (richos-hq \`wiki/ceo-decisions.md\` ~line 423): he wants t3code inside the RichOS app.

**What exists (search the record before you start, and cite it):** \`richos-hq/docs/research/t3code-mobile-vs-richos-phone-2026-09-18.md\` (Reed, pinned to \`pingdotgg/t3code\` @ \`8ebb6112\`, MIT) and \`docs/research/t3-relay-exact-details-and-our-cost-2026-09-18.md\` — both scoped to the phone channel and their relay. **Nobody has read their tooling.** Today, 2026-09-19, 8 of 19 lands on richos main were our own tooling.

## What to read, IN FULL, from a fresh clone pinned to a commit

T3 Code's: repository layout and package manager; how they build the desktop app, sign, notarize and publish.

## What to deliver — a committed brief in your worktree, \`docs/research/t3code-tooling-what-to-adopt-2026-09-19.md\`

1. **Per area, one of three verdicts with the file:line evidence:** ADOPT AS-IS, COPY THE APPROACH, NOT APPLICABLE.

Read in full; every claim carries a \`t3code:<path>:<line>\`. Commit, SHA in the handoff.
FIX
}

agent_payload() {
    TEXT="$1" python3 -c '
import json, os
print(json.dumps({"session_id": "s1", "tool_name": "Agent",
                  "tool_input": {"prompt": os.environ["TEXT"], "description": "spawn"}}))
'
}

bash_payload() {
    CMD="$1" CWD="$2" python3 -c '
import json, os
print(json.dumps({"session_id": "s1", "cwd": os.environ["CWD"], "tool_name": "Bash",
                  "tool_input": {"command": os.environ["CMD"]}}))
'
}

check() {
    local id="$1" want="$2" desc="$3" rc="$4" out="$5"
    if [ "$rc" -eq "$want" ]; then
        PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' "$id" "$desc"
    else
        FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s want=%s)\n' "$id" "$desc" "$rc" "$want"
        printf '%s\n' "$out" | sed 's/^/          /' | head -10
    fi
    LAST_OUT="$out"
}

saw() {
    local id="$1" needle="$2" desc="$3"
    if printf '%s' "$LAST_OUT" | grep -qF -- "$needle"; then
        PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' "$id" "$desc"
    else
        FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s — never said %s\n' "$id" "$desc" "$needle"
    fi
}

run_agent() {
    local id="$1" want="$2" desc="$3" text="$4"; shift 4
    local out rc
    out="$(agent_payload "$text" | env RICHOS_ENTITY_ROOT="$ENTITY" "$@" "$HOOK" 2>&1)"; rc=$?
    check "$id" "$want" "$desc" "$rc" "$out"
}

run_bash() {
    local id="$1" want="$2" desc="$3" cmd="$4" cwd="$5"; shift 5
    local out rc
    out="$(bash_payload "$cmd" "$cwd" | env RICHOS_ENTITY_ROOT="$ENTITY" "$@" "$HOOK" 2>&1)"; rc=$?
    check "$id" "$want" "$desc" "$rc" "$out"
}

echo "=== guard-public-record-repo.test.sh ==="
echo ""
echo "A. THE DISPATCH THAT PUT A RESEARCH READ IN PUBLIC HISTORY"

run_agent A1 2 "the real prompt, aimed at the PUBLIC repository, is REFUSED" "$(prompt_for "$PUB_WS")"
saw A1a "docs/research/t3code-tooling-what-to-adopt-2026-09-19.md" "the refusal names the deliverable"
saw A1b "private-repo" "the refusal names the private repository to use instead"
saw A1c "public repo????" "the refusal carries the CEO's own sentence"

# THE REFUSAL NAMES THE DELIVERABLE AND NOTHING ELSE. The same prompt carries a
# BARE prior-art path, docs/research/t3-relay-exact-details-..., on a line that
# also reads "8 of 19 lands on richos main were our own tooling" — and that
# sentence is why "lands" is not a delivery verb.
if printf '%s' "$LAST_OUT" | grep -q "t3-relay-exact-details"; then
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' A1d "a bare prior-art path on a lands line must not read as a deliverable"
else
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' A1d "the refusal names the deliverable only, not the bare prior-art path beside it"
fi

run_agent A2 0 "the SAME prompt aimed at the private repository passes" "$(prompt_for "$PRIV_WS")"

if printf '%s' "$LAST_OUT" | grep -q "t3code-mobile-vs-richos-phone"; then
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' A3 "a prefixed prior-art citation must not read as a deliverable"
else
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' A3 "richos-hq/docs/research/... is prior art, not a deliverable"
fi

# THE PREFIX IS WHAT DECIDES, NOT THE VERB. This one carries the strongest
# delivery verb in the list AND a record path, and it must still pass: the path
# is the private repository's, so it is somebody else's file.
read -r -d '' PRIOR_ART <<'FIX' || true
cross-repo-worktree: PUBWS

# Brief: make the nightly fail loudly when the notary rejects it

The read that settled this was delivered as `richos-hq/docs/research/t3code-tooling-what-to-adopt-2026-09-19.md`; write your fix against its section 2.1.

## Scope

`richos/app/scripts/nightly-local.py`.
FIX
run_agent A4 0 "a PREFIXED record path on a line with a delivery verb is still prior art" \
    "${PRIOR_ART/PUBWS/$PUB_WS}"

echo ""
echo "B. THE COMMIT SURFACE"

printf 'a read\n' >"$PUBLIC/docs/research/x.md"
git -C "$PUBLIC" add docs/research/x.md >/dev/null 2>&1
run_bash B1 2 "a commit ADDING docs/research/x.md to the public repo is REFUSED" \
    "git commit -m 'the read'" "$PUBLIC"
saw B1a "docs/research/x.md" "the refusal names the file being added"
run_bash B2 0 "the same commit with a 30+ character public-record-ack passes" \
    "git commit -m 'the read'   # public-record-ack: this note is published on purpose as project documentation" "$PUBLIC"
run_bash B3 2 "a BARE public-record-ack exempts nothing" \
    "git commit -m 'the read'   # public-record-ack: because" "$PUBLIC"
saw B3a "under 30 characters" "and the refusal says why"
git -C "$PUBLIC" reset -q

printf 'an audit\n' >"$PUBLIC/docs/verification/y.md"
git -C "$PUBLIC" add docs/verification/y.md >/dev/null 2>&1
run_bash B4 0 "a commit ADDING docs/verification/y.md PASSES — out of scope by ruling" \
    "git commit -m 'the audit'" "$PUBLIC"
git -C "$PUBLIC" reset -q

# An existing record file, MODIFIED. Already in history; refusing it is theater.
mkdir -p "$PUBLIC/docs/plans"
printf 'v1\n' >"$PUBLIC/docs/plans/p.md"
git -C "$PUBLIC" add docs/plans/p.md >/dev/null 2>&1
git -C "$PUBLIC" commit -q -m "plan v1" --no-verify >/dev/null 2>&1
printf 'v2\n' >"$PUBLIC/docs/plans/p.md"
git -C "$PUBLIC" add docs/plans/p.md >/dev/null 2>&1
run_bash B5 0 "MODIFYING a record already in history is not refused" \
    "git commit -m 'plan v2'" "$PUBLIC"
git -C "$PUBLIC" reset -q --hard >/dev/null 2>&1

# STAGING AND COMMITTING IN ONE COMMAND. The index alone answers "what is in
# this commit" only when the staging already happened in an earlier tool call,
# and a guard that reads only the index is defeated by one ampersand.
printf 'a plan\n' >"$PUBLIC/docs/plans/q.md"
run_bash B8 2 "git add docs/plans/q.md && git commit, in ONE command, is REFUSED" \
    "git add docs/plans/q.md && git commit -m 'the plan'" "$PUBLIC"
saw B8a "docs/plans/q.md" "the refusal names the path the same command would stage"
rm -f "$PUBLIC/docs/plans/q.md"

run_bash B6 0 "a commit in a repository with NO publication declaration passes" \
    "git commit -m anything" "$PRIVATE"

# THE REMEDY MUST WORK. Everything this guard refuses, it refuses by telling
# somebody to do it in the private repository instead — so the identical
# commit there has to pass, and it must pass because the declaration says so
# and not because the commit happened to stage nothing.
git -C "$PRIVATE" config user.email t@example.com
git -C "$PRIVATE" config user.name Test
printf 'the read\n' >"$PRIVATE/docs/research/z.md"
git -C "$PRIVATE" add docs/research/z.md >/dev/null 2>&1
run_bash B9 0 "the SAME record, committed in the private repository, passes" \
    "git commit -m 'the read'" "$PRIVATE"
run_bash B7 0 "a Bash command that is not a commit is silent" "ls -la" "$PUBLIC"

echo ""
echo "C. THE HATCH ON THE DISPATCH SURFACE"

run_agent C1 0 "a 30+ character public-record-ack on its own line permits the dispatch" \
    "$(printf '%s\n\n%s\n' "$(prompt_for "$PUB_WS")" 'public-record-ack: this research note ships with the open-source project on purpose')"
if grep -q "ships with the open-source project" "$ENTITY/.claude/state/public-record-acks.log" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' C2 "and it is logged to .claude/state/public-record-acks.log"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' C2 "nothing in public-record-acks.log"
fi
run_agent C3 2 "a short public-record-ack exempts nothing" \
    "$(printf '%s\n\n%s\n' "$(prompt_for "$PUB_WS")" 'public-record-ack: fine')"

echo ""
echo "D. FAIL OPEN"

D1_OUT="$(python3 -c '
import json
print(json.dumps({"tool_name": "Write", "tool_input": {"file_path": "/tmp/x.md", "content": "docs/research/x.md written here"}}))
' | env RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>&1)"; D1_RC=$?
check D1b 0 "a Write call is silent" "$D1_RC" "$D1_OUT"

D2_OUT="$(printf 'not json' | env RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>&1)"; D2_RC=$?
check D2 0 "an unparseable payload allows" "$D2_RC" "$D2_OUT"

mkdir -p "$SB/not-adopted"
run_agent D3 0 "a session in a repository that never adopted the engine stands down" \
    "$(prompt_for "$PUB_WS")" RICHOS_ENTITY_ROOT="$SB/not-adopted"

mkdir -p "$SB/off"
printf 'PROTECTED_PATHS=""\nPUBLIC_RECORD_REPO_GUARD="off"\n' >"$SB/off/orchestration.config"
run_agent D4 0 "a declared off switch stands down" "$(prompt_for "$PUB_WS")" \
    RICHOS_ENTITY_ROOT="$SB/off"

echo ""
echo "E. A WORKSPACE THAT DOES NOT EXIST YET"

# spawn.sh evaluates every PreToolUse[Agent] guard BEFORE it creates anything,
# so the common case is a path with no directory behind it. The <repo>-wt/<name>
# convention answers it; anything else is announced, never guessed.
CONV="$SB/public-repo-wt/reed-opus-x1"
mkdir -p "$(dirname "$SB/public-repo-wt")"
run_agent E1 2 "a not-yet-created <repo>-wt/<name> workspace still resolves to the public repo" \
    "$(prompt_for "$CONV")"

E2_OUT="$(agent_payload "$(prompt_for "/nowhere/at/all/agent-1")" \
    | env RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>&1)"; E2_RC=$?
if [ "$E2_RC" -eq 0 ] && printf '%s' "$E2_OUT" | grep -q "resolve to no repository"; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' E2 "an unresolvable workspace ALLOWS and ANNOUNCES"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s)\n' E2 "an unresolvable workspace must allow loudly" "$E2_RC"
    printf '%s\n' "$E2_OUT" | sed 's/^/          /' | head -6
fi

echo ""
echo "F. THE ONE HARD REFUSAL"

BROKEN="$SB/broken"
mkdir -p "$BROKEN/scripts/hooks"
cp "$HOOK" "$BROKEN/scripts/hooks/"
F1_OUT="$(agent_payload "$(prompt_for "$PUB_WS")" \
    | env RICHOS_ENTITY_ROOT="$ENTITY" "$BROKEN/scripts/hooks/guard-public-record-repo.sh" 2>&1)"; F1_RC=$?
if [ "$F1_RC" -eq 2 ] && printf '%s' "$F1_OUT" | grep -q "BROKEN INSTALL"; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' F1 "a missing scripts/lib/resolve-roots.sh prints the shared banner and exits 2"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s)\n' F1 "a broken install must refuse loudly" "$F1_RC"
fi

echo ""
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/public-record-repo.mutation.sh" ]; then
    # THE HARNESS RUNS FROM THE SUITE IT MUTATES, as dialect-guard.test.sh
    # does. Until 2026-09-22 nothing ran it (open items 3.42/3.43, and
    # mutation-inventory.test.sh named it an orphan). The harness sets
    # RICHOS_MUTATION_INNER for every copy of this suite it runs, so this
    # block does not recurse. A failing harness counts as one failure.
    echo "=== running the mutation harness ==="
    "$SCRIPT_DIR/public-record-repo.mutation.sh" || FAIL=$((FAIL + 1))
fi
echo ""
echo "guard-public-record-repo: $PASS/$((PASS + FAIL)) cases pass"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
