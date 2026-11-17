#!/usr/bin/env bash
#
# guard-model-ceiling.test.sh — regression tests for
# scripts/hooks/guard-model-ceiling.sh.
#
# THE COST CEILING (orchestration.config MODEL_CEILING, ranked only through
# scripts/lib/model-tiers.sh, with the spawn's model resolved only through
# scripts/lib/resolve-model.sh). Covered here:
#
#   (a) a spawn AT or BELOW the ceiling is allowed IN SILENCE — no output on
#       either stream, because a ceiling that comments on the normal case is a
#       nag and a nag is how a guard becomes something to disable;
#   (b) a spawn ABOVE the ceiling is BLOCKED, by explicit `model:` override AND
#       by the agent definition's own model: frontmatter (so the rule is about
#       the RESOLVED model, not about a string in the payload), including a
#       verbose model id that normalizes down to an alias;
#   (c) the refusal CARRIES the rule rather than pointing at it: the ceiling in
#       one line, the three worked examples, the one-off-that-everything-
#       inherits test, the exact ack line, and the reconsider path (drop to the
#       ceiling AND rename the teammate, because the <model> token must stay
#       truthful);
#   (d) the escape hatch `model-ceiling-ack: <reason>` — a reasoned line permits
#       ONE spawn and is logged with both the model and the ceiling; a bare
#       marker, a too-short reason, and a reason that is nothing but an
#       ASSERTION OF MERIT ("most capable", "better", "why not", "important")
#       exempt nothing; a written sentence that merely CONTAINS such a word is
#       allowed through, because judging a real reason's merit is not a
#       machine's job; a refused spawn is never logged;
#   (e) THE DATA IS THE RULE: a ceiling declared at the TOP tier allows the top
#       tier silently, and a ceiling declared LOWER refuses the tier that was
#       fine a moment ago — proving nothing in the guard knows an alias by name;
#   (f) FAIL OPEN, LOUDLY, EVERY TIME: no MODEL_CEILING, an unrankable ceiling,
#       an unrankable resolved model, a blank/malformed MODEL_TIERS, a missing
#       model-tiers.sh, a missing resolve-model.sh, an unresolvable namespaced
#       type and an unparseable payload all ALLOW the spawn and announce that
#       nothing is checking;
#   (g) FAIL OPEN, SILENTLY, for the two states that are not faults: a
#       repository that never adopted the engine, and a spawn whose model is
#       genuinely undeterminable (model:"inherit", or a type with no live
#       definition — the inherit-from-session case);
#   (h) the one hard refusal: a missing scripts/lib/resolve-roots.sh prints the
#       shared BROKEN INSTALL banner and exits 2, because a hook that cannot
#       tell which repository it governs must not guess;
#   (i) SCOPE: a read-only agent type over the ceiling is refused like any
#       other — spend is spend, and the isolation exemption is about writing
#       files, not about cost.
#
# Run directly: scripts/hooks/guard-model-ceiling.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-model-ceiling.sh"

# The governed repository is DECLARED, never inherited from the launching
# session: run from a session seated elsewhere, every case below would pass by
# standing down. Same reason guard-worktree-isolation.test.sh declares it.
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0

SB="$(cd "$(mktemp -d -t guard-model-ceiling.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SB"' EXIT

ENTITY="$SB/entity"
mkdir -p "$ENTITY/.claude/agents"

# Fixture roster. The model: frontmatter is the whole point of two of them: the
# ceiling must fire on the RESOLVED model, which for a spawn with no override IS
# the definition's default.
printf -- '---\nname: mcjudge\nmodel: opus\n---\nA sandbox judgment role, at the ceiling.\n'  >"$ENTITY/.claude/agents/mcjudge.md"
printf -- '---\nname: mctop\nmodel: fable\n---\nA sandbox role whose OWN default is above the ceiling.\n' >"$ENTITY/.claude/agents/mctop.md"
printf -- '---\nname: mcverbose\nmodel: claude-fable-5-1\n---\nA sandbox role whose default is a VERBOSE id above the ceiling.\n' >"$ENTITY/.claude/agents/mcverbose.md"
printf -- '---\nname: mcverboseopus\nmodel: "claude-opus-4-8"\n---\nA sandbox role whose default is a VERBOSE id at the ceiling.\n' >"$ENTITY/.claude/agents/mcverboseopus.md"
printf -- '---\nname: mcplain\n---\nA sandbox role with no model: line at all.\n'             >"$ENTITY/.claude/agents/mcplain.md"

# write_config <ceiling-line> [tiers]
write_config() {
    {
        printf 'PROTECTED_PATHS=""\n'
        printf 'READONLY_ALLOWLIST="Explore Plan"\n'
        printf 'ALLOWED_MODELS="fable opus sonnet haiku"\n'
        printf 'MODEL_TIERS="%s"\n' "${2-fable > opus > sonnet > haiku}"
        if [ -n "${1-}" ]; then printf '%s\n' "$1"; fi
    } >"$ENTITY/orchestration.config"
}
write_config 'MODEL_CEILING="opus"'

# payload <subagent> <name> <model-or-empty> <prompt>
payload() {
    python3 - "$1" "$2" "$3" "$4" <<'PY'
import json, sys
subagent, name, model, prompt = sys.argv[1:5]
ti = {"prompt": prompt}
if subagent: ti["subagent_type"] = subagent
if name:     ti["name"] = name
if model:    ti["model"] = model
ti["isolation"] = "worktree"
print(json.dumps({"tool_name": "Agent", "tool_input": ti,
                  "session_id": "mc000000-0000-4000-8000-000000000000",
                  "tool_use_id": "toolu_mc_test"}))
PY
}

run() {   # <json> -> stdout+stderr, sets RC
    set +e
    OUT="$(printf '%s' "$1" | env RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>&1)"
    RC=$?
}

ok()  { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  FAIL  %s%s\n' "$1" "${2:+ ($2)}"; }

case_rc() {   # <name> <expected-rc> <json>
    run "$3"
    [ "$RC" -eq "$2" ] && ok "$1" || bad "$1" "expected exit $2, got $RC: ${OUT:0:200}"
}

case_silent() {   # <name> <json> — exit 0 AND not one byte of output
    run "$2"
    if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then ok "$1"
    else bad "$1" "expected exit 0 with NO output, got exit $RC: ${OUT:0:200}"; fi
}

case_msg() {   # <name> <needle> <json>
    run "$3"
    printf '%s' "$OUT" | grep -qF "$2" && ok "$1" || bad "$1" "output did not mention \"$2\""
}

case_open_loud() {   # <name> <needle> <json> — allowed AND announced
    run "$3"
    if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -qF "$2"; then ok "$1"
    else bad "$1" "expected exit 0 announcing \"$2\", got exit $RC: ${OUT:0:200}"; fi
}

echo "=== guard-model-ceiling tests ==="

# --- (a) at or below the ceiling: allowed, in silence ----------------------
case_silent "at the ceiling (opus override on an opus-default role) -> silent" \
    "$(payload mcjudge mcjudge-opus-1 opus 'Judge this.')"
case_silent "below the ceiling (sonnet override) -> silent" \
    "$(payload mcjudge mcjudge-sonnet-1 sonnet 'Judge this.')"
case_silent "below the ceiling (haiku override) -> silent" \
    "$(payload mcjudge mcjudge-haiku-1 haiku 'Judge this.')"
case_silent "no override, definition default AT the ceiling -> silent" \
    "$(payload mcjudge mcjudge-opus-2 '' 'Judge this.')"
case_silent "a non-Agent tool is none of this guard's business" \
    '{"tool_name":"Bash","tool_input":{"command":"echo hi"}}'

# --- (b) above the ceiling: blocked, however the model got there -----------
case_rc "above the ceiling by explicit override -> BLOCKED" 2 \
    "$(payload mcjudge mcjudge-fable-1 fable 'Judge this.')"
case_rc "above the ceiling by the DEFINITION's own model: default -> BLOCKED" 2 \
    "$(payload mctop mctop-fable-1 '' 'Judge this.')"
case_rc "a verbose model id normalizes to its alias and is still BLOCKED" 2 \
    "$(payload mcjudge mcjudge-fable-2 claude-fable-5-1 'Judge this.')"
case_rc "a VERBOSE model id in the definition's own model: default is BLOCKED like the alias (P5-63)" 2 \
    "$(payload mcverbose mcverbose-fable-1 '' 'Judge this.')"
case_silent "a verbose opus id in the definition's default is at the ceiling -> silent (P5-63)" \
    "$(payload mcverboseopus mcverboseopus-opus-1 '' 'Judge this.')"
case_rc "a READ-ONLY agent type over the ceiling is refused too (spend is spend)" 2 \
    "$(payload Explore explore-fable-1 fable 'Look around.')"

# --- (c) the refusal CARRIES the rule --------------------------------------
OVER="$(payload mcjudge mcjudge-fable-3 fable 'Judge this.')"
case_msg "the refusal names the spawn"                    "mcjudge-fable-3 of 'mcjudge'" "$OVER"
case_msg "the refusal names the requested model and tier" "requested  : fable (tier 1)"  "$OVER"
case_msg "the refusal names the ceiling and its tier"     "the ceiling: opus (tier 2)"   "$OVER"
case_msg "the refusal names the key it read"              "MODEL_CEILING in orchestration.config" "$OVER"
case_msg "the refusal states the ceiling in one line"     "the normal ceiling for CRITICAL work is" "$OVER"
case_msg "the refusal carries the one-off test"           "EVERYTHING DOWNSTREAM INHERITS" "$OVER"
case_msg "the refusal carries worked example 1"           "worktree-lifecycle work and its fixes" "$OVER"
case_msg "the refusal carries worked example 2"           "super-premium front-end design" "$OVER"
case_msg "the refusal carries worked example 3"           "creating a design system"      "$OVER"
case_msg "the refusal names the RECONSIDER path"          'model: "opus"'                 "$OVER"
case_msg "the refusal names the rename the drop forces"   "mcjudge-opus-3"                "$OVER"
case_msg "the rename keeps the name's effort word (2026-11-17)" "rename the teammate 'mcjudge-opus-medium-3'" \
    "$(payload mcjudge mcjudge-fable-medium-3 fable 'Judge this.')"
case_msg "the refusal names the exact ack line to add"    "model-ceiling-ack: <why this is a one-off" "$OVER"
case_msg "the refusal says the founder's greenlight is a good reason" \
    "greenlighting this tier in conversation IS a good reason" "$OVER"

# --- (d) the escape hatch ---------------------------------------------------
case_rc "a reasoned model-ceiling-ack: permits the spawn" 0 \
    "$(payload mcjudge mcjudge-fable-4 fable 'Build it.
model-ceiling-ack: creating the design system every later screen inherits, once.')"
case_rc "model-ceiling-ack: tolerates leading whitespace" 0 \
    "$(payload mcjudge mcjudge-fable-5 fable 'Build it.
   model-ceiling-ack: the founder greenlit this seat for the lifecycle rebuild today.')"
case_rc "a BARE model-ceiling-ack: exempts nothing -> BLOCKED" 2 \
    "$(payload mcjudge mcjudge-fable-6 fable 'Build it.
model-ceiling-ack:')"
case_rc "a too-short reason exempts nothing -> BLOCKED" 2 \
    "$(payload mcjudge mcjudge-fable-7 fable 'Build it.
model-ceiling-ack: needed')"
case_rc "the marker mid-line (not its own line) exempts nothing -> BLOCKED" 2 \
    "$(payload mcjudge mcjudge-fable-8 fable 'Build it and model-ceiling-ack: this is a one-off whose output everything inherits.')"

# The reflex list — an assertion of merit is not a reason.
for reflex in \
    'it is the most capable model available to us right now' \
    'a better model gives a better answer on this one' \
    'faster and quicker than the alternative would be' \
    'why not, this is the quality option here' \
    'this work is really important and quite critical' ; do
    case_rc "reflex reason refused: '${reflex:0:28}...'" 2 \
        "$(payload mcjudge mcjudge-fable-9 fable "Build it.
model-ceiling-ack: $reflex")"
done
case_msg "a REJECTED ack is quoted back with the reason it failed" "THE LINE YOU GAVE WAS NOT ACCEPTED" \
    "$(payload mcjudge mcjudge-fable-9 fable 'Build it.
model-ceiling-ack: needed')"
# ... and the no-ack refusal does NOT carry that prefix: telling an operator
# "no line is present" directly under the instruction to add one is noise.
run "$(payload mcjudge mcjudge-fable-9 fable 'Build it.')"
if printf '%s' "$OUT" | grep -qF "THE LINE YOU GAVE WAS NOT ACCEPTED"; then
    bad "a refusal with NO ack attempted does not diagnose one"
else
    ok "a refusal with NO ack attempted does not diagnose one"
fi
case_msg "the reflex refusal names the assertion it read" "ASSERTION OF MERIT" \
    "$(payload mcjudge mcjudge-fable-9 fable 'Build it.
model-ceiling-ack: it is simply the most capable model we have')"
case_msg "the reflex refusal re-asks the one-off question" "Say what makes it one." \
    "$(payload mcjudge mcjudge-fable-9 fable 'Build it.
model-ceiling-ack: it is the most capable model available to us')"
# ... and a written sentence that merely CONTAINS a listed word survives.
case_rc "a real reason that happens to contain a listed word -> allowed" 0 \
    "$(payload mcjudge mcjudge-fable-10 fable 'Build it.
model-ceiling-ack: super-premium start-screen treatment; the quality bar here is copied by every later screen.')"
case_rc "a real reason containing 'critical' survives when it says more" 0 \
    "$(payload mcjudge mcjudge-fable-11 fable 'Build it.
model-ceiling-ack: super-critical one-off, the worktree lifecycle rebuild whose fix every future agent inherits.')"

# The log — both models, the ceiling, and the reason.
MC_MARK="mc-log-canary-$$"
run "$(payload mcjudge mcjudge-fable-12 fable "Build it.
model-ceiling-ack: design-system creation, logging fixture $MC_MARK, inherited downstream.")"
MC_LOG="$ENTITY/.claude/state/model-ceiling-acks.log"
if [ -f "$MC_LOG" ] && grep -q "$MC_MARK" "$MC_LOG" \
   && grep -q "model=fable" "$MC_LOG" && grep -q "ceiling=opus" "$MC_LOG"; then
    ok "an accepted ack is logged with the model, the ceiling and the reason"
else
    bad "an accepted ack is logged with the model, the ceiling and the reason" \
        "$([ -f "$MC_LOG" ] && tail -1 "$MC_LOG" || echo 'no log file')"
fi
MC_LINES_BEFORE="$(wc -l <"$MC_LOG" 2>/dev/null || echo 0)"
run "$(payload mcjudge mcjudge-fable-13 fable 'Build it.
model-ceiling-ack: most capable')"
MC_LINES_AFTER="$(wc -l <"$MC_LOG" 2>/dev/null || echo 0)"
if [ "$MC_LINES_BEFORE" = "$MC_LINES_AFTER" ]; then
    ok "a REFUSED spawn is not logged (a refusal is not a waiver)"
else
    bad "a REFUSED spawn is not logged (a refusal is not a waiver)"
fi

# --- (e) the DATA is the rule, not an alias name ---------------------------
write_config 'MODEL_CEILING="fable"'
case_silent "ceiling declared at the TOP tier -> the top tier is silent" \
    "$(payload mcjudge mcjudge-fable-14 fable 'Judge this.')"
write_config 'MODEL_CEILING="sonnet"'
case_rc "ceiling declared LOWER -> opus is now above it and BLOCKED" 2 \
    "$(payload mcjudge mcjudge-opus-4 opus 'Judge this.')"
case_msg "the lower-ceiling refusal names sonnet, never a hardcoded alias" \
    "the ceiling: sonnet (tier 3)" "$(payload mcjudge mcjudge-opus-5 opus 'Judge this.')"
# A declaration that contradicts the alias NAMES is obeyed: nothing infers an
# order from a name (the 2026-09-02 defect, restated for the ceiling).
write_config 'MODEL_CEILING="fable"' "haiku > sonnet > opus > fable"
case_rc "an INVERTED declaration is obeyed: opus is above a fable ceiling -> BLOCKED" 2 \
    "$(payload mcjudge mcjudge-opus-6 opus 'Judge this.')"
case_silent "an INVERTED declaration is obeyed: fable is at the ceiling -> silent" \
    "$(payload mcjudge mcjudge-fable-15 fable 'Judge this.')"
write_config 'MODEL_CEILING="opus"'

# --- (f) fail open, LOUDLY -------------------------------------------------
write_config ''
case_open_loud "no MODEL_CEILING declared -> allowed, announced" \
    "MODEL COST CEILING IS NOT DECLARED" "$(payload mcjudge mcjudge-fable-16 fable 'Judge.')"
write_config 'MODEL_CEILING="mythic"'
case_open_loud "an unrankable ceiling -> allowed, announced" \
    "ranks nowhere" "$(payload mcjudge mcjudge-fable-17 fable 'Judge.')"
write_config 'MODEL_CEILING="opus"' ""
case_open_loud "a blank MODEL_TIERS -> allowed, announced" \
    "MODEL COST CEILING IS OFF" "$(payload mcjudge mcjudge-fable-18 fable 'Judge.')"
write_config 'MODEL_CEILING="opus"' "fable > > sonnet"
case_open_loud "a malformed MODEL_TIERS -> allowed, announced" \
    "is unusable" "$(payload mcjudge mcjudge-fable-19 fable 'Judge.')"
write_config 'MODEL_CEILING="opus"'
case_open_loud "an unrankable RESOLVED model -> allowed, announced" \
    "ranks nowhere" "$(payload mcjudge mcjudge-opus-7 zebra 'Judge.')"
case_open_loud "a namespaced type with no findable definition -> allowed, announced" \
    "MODEL COST CEILING IS OFF for this spawn" "$(payload someplugin:ghost ghost-fable-1 '' 'Judge.')"
set +e
PF_OUT="$(printf '{"tool_name":"Agent","tool_input":"not-an-object"}' | env RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>&1)"
PF_RC=$?
if [ "$PF_RC" -eq 0 ] && printf '%s' "$PF_OUT" | grep -qF "could not be parsed"; then
    ok "an unparseable Agent payload -> allowed, announced (fail-open, unlike the isolation guard)"
else
    bad "an unparseable Agent payload -> allowed, announced" "exit $PF_RC: ${PF_OUT:0:160}"
fi

# --- (g) fail open, SILENTLY, for the two states that are not faults -------
case_silent "model:\"inherit\" is not a pick -> silent" \
    "$(payload mcjudge mcjudge-opus-8 inherit 'Judge this.')"
case_silent "no override and no live definition (host built-in) -> silent" \
    "$(payload general-purpose gp-opus-1 '' 'Do a thing.')"
case_silent "a definition with no model: line and no override -> silent" \
    "$(payload mcplain mcplain-opus-1 '' 'Do a thing.')"

NEUTRAL="$SB/never-adopted"
mkdir -p "$NEUTRAL"
NA_PAYLOAD="$(python3 - "$NEUTRAL" <<'PY'
import json, sys
print(json.dumps({"tool_name": "Agent", "cwd": sys.argv[1],
                  "tool_input": {"subagent_type": "mcjudge", "name": "mcjudge-fable-20",
                                 "model": "fable", "isolation": "worktree", "prompt": "x"}}))
PY
)"
set +e
NA_OUT="$(cd "$NEUTRAL" && printf '%s' "$NA_PAYLOAD" | env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR "$HOOK" 2>&1)"
NA_RC=$?
if [ "$NA_RC" -eq 0 ] && [ -z "$NA_OUT" ]; then
    ok "a repository that never adopted the engine -> stand down, SILENT"
else
    bad "a repository that never adopted the engine -> stand down, SILENT" "exit $NA_RC: ${NA_OUT:0:160}"
fi

# --- (f cont.) / (h) the library cases, in a hand-built mini-engine --------
# mini_engine <dir> <lib...> — an engine carrying ONLY the named libs.
mini_engine() {
    local dir="$1"; shift
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib"
    cp "$HOOK" "$dir/scripts/hooks/"
    chmod +x "$dir/scripts/hooks/guard-model-ceiling.sh"
    local l
    for l in "$@"; do cp "$ENGINE_ROOT/scripts/lib/$l" "$dir/scripts/lib/"; done
}
mini_run() {   # <dir> <json>
    set +e
    OUT="$(printf '%s' "$2" | env RICHOS_ENTITY_ROOT="$ENTITY" "$1/scripts/hooks/guard-model-ceiling.sh" 2>&1)"
    RC=$?
}
OVER_JSON="$(payload mcjudge mcjudge-fable-21 fable 'Judge.')"

mini_engine "$SB/no-tiers" resolve-roots.sh resolve-main-checkout.sh resolve-model.sh
mini_run "$SB/no-tiers" "$OVER_JSON"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -qF "model-tiers.sh is missing"; then
    ok "model-tiers.sh missing -> allowed, announced"
else
    bad "model-tiers.sh missing -> allowed, announced" "exit $RC: ${OUT:0:160}"
fi

mini_engine "$SB/no-resolver" resolve-roots.sh resolve-main-checkout.sh model-tiers.sh
mini_run "$SB/no-resolver" "$OVER_JSON"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -qF "resolve-model.sh is missing"; then
    ok "resolve-model.sh missing -> allowed, announced"
else
    bad "resolve-model.sh missing -> allowed, announced" "exit $RC: ${OUT:0:160}"
fi

mini_engine "$SB/no-roots" model-tiers.sh resolve-model.sh
mini_run "$SB/no-roots" "$OVER_JSON"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -qF "BROKEN INSTALL"; then
    ok "resolve-roots.sh missing -> the shared BROKEN INSTALL banner, exit 2 (the one hard refusal)"
else
    bad "resolve-roots.sh missing -> BROKEN INSTALL banner, exit 2" "exit $RC: ${OUT:0:160}"
fi

# The whole engine present, so the mini-engine harness itself is proven honest:
# without this, every case above would pass for a sandbox that simply cannot run.
mini_engine "$SB/complete" resolve-roots.sh resolve-main-checkout.sh model-tiers.sh resolve-model.sh
mini_run "$SB/complete" "$OVER_JSON"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -qF "MODEL COST CEILING"; then
    ok "harness honesty: a COMPLETE mini-engine still refuses the over-ceiling spawn"
else
    bad "harness honesty: a COMPLETE mini-engine still refuses the over-ceiling spawn" "exit $RC: ${OUT:0:160}"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== guard-model-ceiling tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== guard-model-ceiling tests: all $PASS passed ==="

# The mutation harness runs LAST, from inside the suite it mutates, so the
# runner that discovers *.test.sh runs it too — a harness nobody runs proves
# nothing. RICHOS_MUTATION_INNER stops a mutant recursing into its own harness.
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/guard-model-ceiling.mutation.sh" ]; then
    echo ""
    if ! "$SCRIPT_DIR/guard-model-ceiling.mutation.sh"; then
        echo "=== guard-model-ceiling: the MUTATION harness failed — a property this suite claims to test is not load-bearing ==="
        exit 1
    fi
fi
exit 0
