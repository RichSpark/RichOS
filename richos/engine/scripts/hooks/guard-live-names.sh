#!/usr/bin/env bash
#
# guard-live-names.sh: PreToolUse[Agent]. Live names are unique across sessions
# (richos-hq spec r3 e5).
#
# guard-worktree-isolation.sh's clause 3 already refuses a name this SESSION has
# ever used. With several operator leads on one Mac, a second session could start
# an agent under a name a first session's agent is still running under, and a
# stop, a message or a land naming it would then reach two agents. So a spawn is
# refused when the machine-wide workspace registry (mega-lander/workspaces.py)
# holds an agent of that name from ANOTHER session that has not finished
# (finished_state: running or paused). A finished agent's name is free to other
# sessions, exactly as before.
#
# WITH THE SWITCH OFF (the entity's launcher is off or absent) it never starts
# its check and never refuses. An error inside the check passes the spawn and
# says so.

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
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/guard-live-names.sh
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

INPUT="$(cat)"

ENTITY_ROOT=""
resolve_entity_root "$INPUT" && ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"

_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-live-names.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether a live agent of another session already holds this spawn's name"
fi

if [ -z "$ENTITY_ROOT" ]; then
    exit 0
fi

_GLN_MODE="$SCRIPT_DIR/../lib/operator-mode.sh"
[ -f "$_GLN_MODE" ] || exit 0
# shellcheck source=../lib/operator-mode.sh
. "$_GLN_MODE"
operator_mode_on "$ENTITY_ROOT" || exit 0

_gln_py=python3
_gln_err="$(printf '%s' "$INPUT" | OPERATOR_ENTITY_ROOT="$ENTITY_ROOT" "$_gln_py" "$SCRIPT_DIR/../lib/operator_leads.py" live-names 2>&1 >/dev/null)"
_gln_rc=$?
if [ "$_gln_rc" = 2 ]; then
    printf '%s\n' "$_gln_err" >&2
    exit 2
fi
if [ "$_gln_rc" != 0 ]; then
    _gln_m="OPERATOR LIVE NAMES CHECK DID NOT RUN (exit $_gln_rc): this spawn's name was NOT checked against the live agents of other sessions, and it was let through. ${_gln_err}"
    _gln_j="${_gln_m//\\/\\\\}"; _gln_j="${_gln_j//\"/\\\"}"; _gln_j="${_gln_j//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_gln_j"
fi
exit 0
