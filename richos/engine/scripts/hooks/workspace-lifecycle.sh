#!/usr/bin/env bash
#
# workspace-lifecycle.sh — RECORDS THE FACTS THE WORKSPACE SPEC NAMES. NEVER BLOCKS.
#
# The page: docs/plans/worktree-spec-2026-09-11.md. The mechanism:
# mega-lander/workspaces.py. Registered on:
#
#   SessionStart              the session records itself — process number AND
#                             process start time (point 12); a session that is a
#                             claude --worktree session is marked not allowed
#                             (point 3); finished work is put in front of Rich
#                             first (point 5); due deletion retries run (point 13)
#   SessionEnd                the session records its end (point 12)
#   SubagentStart             the worker's native workspace is registered by the
#                             exact path it started in (point 6); a continuing
#                             agent's predecessor loses its workspaces (point 7)
#   SubagentStop              the platform's own end-of-run signal (point 11)
#   TaskCompleted             the agent handed in its work (point 11, hole 7)
#   PostToolUse[Agent]        the platform's agent id joins the registration and
#                             the native workspace it created is registered (6)
#   PostToolUse[TaskStop]     a successful stop is an end of run (point 11)
#   PostToolUse[SendMessage]  `pause-until: <what ends it>` records a pause; a
#                             later message to a paused agent resumes it (11)
#
# It deletes nothing except what the page deletes: a continuing agent's start
# (point 7) and a due retry of a land or discard (point 13).
#
# Exit 0 always: a fact that could not be recorded is announced, never allowed
# to stop the platform's own event.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

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
  hook: scripts/hooks/workspace-lifecycle.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 0
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# A BOUNDED read, never `cat`: this hook fires on SessionStart, and no
# SessionStart hook may block on a stdin nobody closes (session-start-stdin.
# test.sh). The platform closes stdin within milliseconds; a 225 KB payload
# reads in under 0.1 s, so 3 s is ample. `-d ''` reads to EOF and returns
# nonzero on a complete read, so the value is judged, never `$?`.
INPUT=""
IFS= read -r -t "${RICHOS_HOOK_STDIN_TIMEOUT:-3}" -d '' INPUT || true
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    root_failure_banner "scripts/hooks/workspace-lifecycle.sh" >&2
    exit 0
fi

LIB="$SCRIPT_DIR/../../mega-lander/workspaces.py"
if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$LIB" ]; then
    echo "NOTICE: workspace-lifecycle.sh: python3 or $LIB is unavailable — this workspace fact was NOT recorded (docs/plans/worktree-spec-2026-09-11.md). Restore the engine." >&2
    exit 0
fi

ERRF="$(mktemp "${TMPDIR:-/tmp}/workspace-lifecycle.XXXXXX")" || ERRF=/dev/null
OUT="$(printf '%s' "$INPUT" | python3 "$LIB" --entity "$ENTITY_ROOT" hook 2>"$ERRF")" || true
[ -n "$OUT" ] && printf '%s\n' "$OUT"
if [ "$ERRF" != /dev/null ]; then
    [ -s "$ERRF" ] && sed 's/^/workspace-lifecycle.sh: /' "$ERRF" >&2
    rm -f "$ERRF"
fi
exit 0
