#!/usr/bin/env bash
#
# guard-workflow-ban.sh — BLOCKING PreToolUse guard on the `Workflow` tool.
#
# WHAT IT BLOCKS
#   Every invocation of the `Workflow` orchestration tool, in a repository that
#   has adopted the engine, unconditionally. There is no allow-list, no live
#   escape line, and no ack mechanism: if the tool call reaches this hook in an
#   adopted repository it is refused, period.
#
# WHY
#   The `/workflow` orchestration tool has failed in production sessions and
#   wasted an operator's time. The framing that put this guard here: memory
#   cannot be relied on — a hook is the only thing an operator can rely on. A
#   doctrine line in CLAUDE.md or a memory entry is advisory and degrades
#   silently; a blocking PreToolUse hook is structural and cannot be forgotten,
#   re-read wrong, or summarized away. Orchestration happens exclusively via
#   individual `Agent` dispatches.
#
# DEFAULT-ON, OPT-OUT EXPLICIT
#   BAN_WORKFLOW_TOOL in the governed repository's orchestration.config:
#     unset / 1  -> BAN ACTIVE   (the default, so the ban can never be lost by
#                                 an adopter forgetting to opt in)
#     0          -> stood down, and it SAYS SO on stderr the moment a Workflow
#                   call arrives — never a silent permission
#   A ban that defaults off is a ban that disappears the first time somebody
#   adopts the engine and does not read this file. So it defaults on, and the
#   only way to turn it off is to write the word down in the entity's config,
#   where a reviewer sees it.
#
# WHERE IT DOES NOT APPLY
#   A repository that never adopted the engine. The plugin is enabled at USER
#   scope and therefore loads in every project on the machine; the root contract
#   (scripts/lib/resolve-roots.sh §"not-adopted") says a guard stands down where
#   it was never invited, and this guard is not a special case. `broken`, by
#   contrast, blocks — a guard that believes it is governing something and
#   cannot resolve it must never guess.
#
# DESIGN — DELIBERATELY MINIMAL
#   Past the root resolution every rooted hook shares, the block depends on
#   NOTHING: no state files, no registry or roster reads, no network, no git, no
#   python3. The only payload field it reads is the tool name, and only so it
#   can prove it never over-blocks a different tool (self-test case b). Fewer
#   moving parts = fewer ways for the ban to fail open.
#
# FAIL-CLOSED: if the payload's tool name cannot be determined, the call is
# BLOCKED. This hook is wired under the `Workflow` matcher only, so an
# unparseable payload arriving here is a Workflow call by construction — and a
# ban that guesses "probably fine" is not a ban.
#
# Exit codes (Claude Code PreToolUse convention):
#   0  not a Workflow call, or this repository has not adopted the engine, or
#      the entity opted out explicitly — passes through untouched
#   2  BLOCKED
#
# Self-test:  scripts/hooks/guard-workflow-ban.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-workflow-ban.sh)"

emit_block() {
  {
    echo "=== Workflow tool: BANNED — BLOCKED ==="
    echo "  The \`Workflow\` orchestration tool is banned in this repository."
    echo "  It has failed in prior sessions and wasted an operator's time. This"
    echo "  block is unconditional: there is no escape line, no ack, and no"
    echo "  condition under which a Workflow call is permitted here."
    echo ""
    echo "  Do this instead: orchestrate with individual \`Agent\` dispatches —"
    echo "  one teammate per task, each spawned with isolation: \"worktree\"."
    echo "  Sequence them yourself; do not look for a workflow/plan-runner"
    echo "  substitute."
    echo ""
    echo "  Do not unwire, weaken, shim, or condition this hook to get past it."
    echo "  If this repository genuinely wants the tool, that is a deliberate,"
    echo "  reviewable decision: set BAN_WORKFLOW_TOOL=0 in orchestration.config."
    echo "$HOOK_TAG"
  } >&2
  exit 2
}

# --- self-test ------------------------------------------------------------
# Every case is a pair, because "does it block?" and "does it block only what
# it should?" are different questions and a guard that fails the second one is
# useless in a different direction.
if [ "${1:-}" = "--self-test" ]; then
  SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  st_fail=0
  st_root=""
  st_mkroot() { # <ban-value-or-empty>
    local d
    d="$(mktemp -d "${TMPDIR:-/tmp}/guard-workflow-ban-selftest.XXXXXX")"
    ( cd "$d" && git init -q . >/dev/null 2>&1 ) || true
    if [ -n "${1:-}" ]; then
      printf 'BAN_WORKFLOW_TOOL=%s\n' "$1" > "$d/orchestration.config"
    else
      : > "$d/orchestration.config"
    fi
    printf '%s' "$d"
  }
  st_case() { # <name> <expected-exit> <root> <payload> [<must-contain-in-stderr>]
    local name="$1" want="$2" root="$3" payload="$4" needle="${5:-}" err rc
    err="$(mktemp "${TMPDIR:-/tmp}/guard-workflow-ban-selftest-err.XXXXXX")"
    set +e
    printf '%s' "$payload" | RICHOS_ENTITY_ROOT="$root" "$SELF" >/dev/null 2>"$err"
    rc=$?
    set -e
    if [ "$rc" != "$want" ]; then
      printf '  FAIL  %s  (expected exit=%s got=%s)\n' "$name" "$want" "$rc"
      st_fail=$((st_fail+1))
    elif [ -n "$needle" ] && ! grep -qF "$needle" "$err"; then
      printf '  FAIL  %s  (stderr missing %s)\n' "$name" "$needle"
      st_fail=$((st_fail+1))
    else
      printf '  PASS  %s  (exit=%s)\n' "$name" "$rc"
    fi
    rm -f "$err"
  }

  echo "=== guard-workflow-ban.sh self-test ==="

  ADOPTED="$(st_mkroot "")"
  OPTED_OUT="$(st_mkroot 0)"
  EXPLICIT_ON="$(st_mkroot 1)"
  UNADOPTED="$(mktemp -d "${TMPDIR:-/tmp}/guard-workflow-ban-unadopted.XXXXXX")"
  ( cd "$UNADOPTED" && git init -q . >/dev/null 2>&1 ) || true

  st_case "a.workflow-call-blocked-by-default" 2 "$ADOPTED" \
    '{"tool_name":"Workflow","tool_input":{"description":"run the thing"},"session_id":"selftest"}' \
    "BANNED"
  st_case "a2.block-names-the-opt-out" 2 "$ADOPTED" \
    '{"tool_name":"Workflow","tool_input":{}}' \
    "BAN_WORKFLOW_TOOL=0 in orchestration.config"
  st_case "a3.explicit-1-blocks" 2 "$EXPLICIT_ON" \
    '{"tool_name":"Workflow","tool_input":{}}' \
    "BANNED"
  st_case "b.other-tool-passes-through" 0 "$ADOPTED" \
    '{"tool_name":"Bash","tool_input":{"command":"ls -la"},"session_id":"selftest"}'
  st_case "b2.agent-spawn-passes-through" 0 "$ADOPTED" \
    '{"tool_name":"Agent","tool_input":{"name":"mark-sonnet-x1","prompt":"workflow of work"}}'
  st_case "c.unparseable-payload-fails-closed" 2 "$ADOPTED" \
    'not json at all' \
    "BANNED"
  st_case "d.explicit-opt-out-allows-and-says-so" 0 "$OPTED_OUT" \
    '{"tool_name":"Workflow","tool_input":{}}' \
    "BAN_WORKFLOW_TOOL=0"
  st_case "e.unadopted-repo-stands-down" 0 "" \
    '{"tool_name":"Workflow","tool_input":{},"cwd":"'"$UNADOPTED"'"}'
  st_case "f.declared-but-unadoptable-root-is-broken-and-blocks" 2 "$UNADOPTED" \
    '{"tool_name":"Workflow","tool_input":{}}' \
    "ENFORCEMENT"

  rm -rf "$ADOPTED" "$OPTED_OUT" "$EXPLICIT_ON" "$UNADOPTED" 2>/dev/null || true

  echo ""
  if [ "$st_fail" -gt 0 ]; then
    echo "guard-workflow-ban.sh self-test: $st_fail case(s) FAILED"
    exit 1
  fi
  echo "guard-workflow-ban.sh self-test: all cases passed"
  exit 0
fi

# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/guard-workflow-ban.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

INPUT="$(cat)"

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine, so it never asked for this ban.
    # Stand down. NOT a silent skip: engine-status.sh announces the stand-down
    # into the orchestrator's own context at every session start.
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/guard-workflow-ban.sh" >&2
    exit 2
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${BAN_WORKFLOW_TOOL:=1}"

# --- live hook ------------------------------------------------------------
# Extract the FIRST "tool_name" string value with plain grep/sed — no python3,
# no jq, no interpreter dependency that could turn this ban into a no-op.
TOOL_NAME="$(printf '%s' "$INPUT" \
  | tr '\n' ' ' \
  | grep -o '"tool_name"[[:space:]]*:[[:space:]]*"[^"]*"' \
  | head -1 \
  | sed 's/.*"\([^"]*\)"$/\1/' || true)"

TOOL_LOWER="$(printf '%s' "$TOOL_NAME" | tr '[:upper:]' '[:lower:]')"

case "$TOOL_LOWER" in
  # Unresolvable tool name under the Workflow matcher, or `Workflow` itself
  # (including a renamed/namespaced build of the same tool). Only calls routed
  # here by the `Workflow` matcher can reach these branches, so containment
  # cannot over-block an unrelated tool.
  ""|*workflow*)
    if [ "$BAN_WORKFLOW_TOOL" = "0" ]; then
      # Opted out — but never SILENTLY. The operator sees, at the moment of the
      # call, that a guard stood aside because this repository told it to.
      echo "(hook: guard-workflow-ban.sh) NOTE: the Workflow tool is permitted here because $CONFIG sets BAN_WORKFLOW_TOOL=0. Remove that line to restore the ban." >&2
      exit 0
    fi
    emit_block
    ;;
esac

exit 0
