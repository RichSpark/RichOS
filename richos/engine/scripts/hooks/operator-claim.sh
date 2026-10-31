#!/usr/bin/env bash
#
# operator-claim.sh: SessionStart. His terminal claims his team (richos-hq spec
# r3 (e) "The claim", item 7; r4 §2.4; Frank G11).
#
# A session in his entity whose entrypoint does not start with `sdk-` (his
# terminal: `cli`, or an IDE or desktop entrypoint), seated at the entity root and
# not in .claude/worktrees/, writes itself into <claude dir>/state/
# operator-lead.json under the flock on operator-lead.lock. The app reads that
# file, and the session table itself, before it opens any operator lead, so the
# terminal and the app never run his team at once.
#
#   * a live APP claim: nothing is claimed, and one systemMessage tells him his
#     team is running in the app and what this terminal will refuse while it does
#     (guard-operator-claim.sh refuses it);
#   * an unreadable claim: treated as held, and the message names the way through;
#   * one of the app's own leads (RICHOS_OPERATOR_LEAD matching a live app claim
#     that lists its pid) is skipped: it is the app's, not the terminal's;
#   * IDEMPOTENT for every source: `compact` and `resume` re-run SessionStart
#     (r4 §2.4, P7), and a session already in the claim writes nothing.
#
# Nothing is ever written into ~/.claude/sessions/ (r3 F6).
#
# WITH THE SWITCH OFF (the entity's launcher says OPERATOR_FENCES_STATE="off", or
# there is none) this hook never starts its check, writes nothing and says nothing.
#
# STDIN IS READ WITH A TIMEOUT: a SessionStart hook is runnable as a CLI tool with
# an inherited stdin nobody closes (session-start-stdin.test.sh).

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
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
  hook: scripts/hooks/operator-claim.sh
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

INPUT=""
IFS= read -r -t "${RICHOS_HOOK_STDIN_TIMEOUT:-3}" -d '' INPUT || true

_ocl_say() { # one systemMessage, escaped in pure bash (python3 may be what is missing)
    local j="${1//\\/\\\\}"; j="${j//\"/\\\"}"; j="${j//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$j"
}

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    # Which entity this is cannot be told, so neither can its switch. Nothing is
    # claimed; with the switch on, the app's own read of the session table still
    # sees this terminal (r3 (e) item 1).
    exit 0
fi

_OCL_MODE="$SCRIPT_DIR/../lib/operator-mode.sh"
[ -f "$_OCL_MODE" ] || exit 0
# shellcheck source=../lib/operator-mode.sh
. "$_OCL_MODE"
operator_mode_on "$ENTITY_ROOT" || exit 0

_ocl_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
_ocl_out="$(printf '%s' "$INPUT" | OPERATOR_ENTITY_ROOT="$ENTITY_ROOT" "$_ocl_py" "$SCRIPT_DIR/../lib/operator_leads.py" claim-start 2>/dev/null)"
_ocl_rc=$?
if [ "$_ocl_rc" -ne 0 ]; then
    _ocl_say "OPERATOR CLAIM DID NOT RUN (exit $_ocl_rc): this terminal session was not checked against the RichOS app's claim on your team. operator_leads.py claim-status shows the claim."
    exit 0
fi
[ -n "$_ocl_out" ] && printf '%s\n' "$_ocl_out"
exit 0
