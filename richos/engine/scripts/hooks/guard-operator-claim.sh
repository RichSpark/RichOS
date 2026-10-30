#!/usr/bin/env bash
#
# guard-operator-claim.sh: PreToolUse. While the RichOS app runs his team, his
# terminal does not (richos-hq spec r3 (e) "The claim", items 6 and 7).
#
# Registered on Agent and SendMessage, and a module of the Bash and
# Write chains of dispatch-pretooluse.sh. In a TERMINAL session of his entity
# (entrypoint not `sdk-*`, seated at the entity root, not in .claude/worktrees/;
# Frank G11), while <claude dir>/state/operator-lead.json holds a LIVE app claim,
# it refuses, by tool name:
#   * Agent and SendMessage (starting or messaging his team). NEVER TaskStop: a
#     stop only removes, reaches only this terminal's own tasks, and must never
#     wait on a claim file (ceo-decisions §67);
#   * a Bash command that takes the land lease (`land-lease.sh acquire|takeover`);
#   * a Write, Edit, MultiEdit or NotebookEdit into his memory directory or into a
#     main checkout of a fenced repository (the record).
# An UNREADABLE claim is treated as held, and the refusal names the way through
# (item 6). The app's own leads (RICHOS_OPERATOR_LEAD matching the live app claim
# that lists them) and every non-terminal session pass untouched.
#
# WITH THE SWITCH OFF (the entity's launcher is off or absent) it never starts
# its check and never refuses. An error inside the check passes the call and
# says so: a defect here must never be what stops his terminal.

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
  hook: scripts/hooks/guard-operator-claim.sh
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
    unevaluated_or_continue "guard-operator-claim.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this terminal may act on your team while the RichOS app runs it"
fi

if [ -z "$ENTITY_ROOT" ]; then
    # Not adopted, or not resolvable: no entity, so no switch to be on.
    exit 0
fi

_GOC_MODE="$SCRIPT_DIR/../lib/operator-mode.sh"
[ -f "$_GOC_MODE" ] || exit 0
# shellcheck source=../lib/operator-mode.sh
. "$_GOC_MODE"
operator_mode_on "$ENTITY_ROOT" || exit 0

_goc_py=python3
_goc_err="$(printf '%s' "$INPUT" | OPERATOR_ENTITY_ROOT="$ENTITY_ROOT" "$_goc_py" "$SCRIPT_DIR/../lib/operator_leads.py" claim-guard 2>&1 >/dev/null)"
_goc_rc=$?
if [ "$_goc_rc" = 2 ]; then
    printf '%s\n' "$_goc_err" >&2
    exit 2
fi
if [ "$_goc_rc" != 0 ]; then
    _goc_m="OPERATOR CLAIM CHECK DID NOT RUN (exit $_goc_rc): this call was NOT checked against the RichOS app's claim on your team, and it was let through. ${_goc_err}"
    _goc_j="${_goc_m//\\/\\\\}"; _goc_j="${_goc_j//\"/\\\"}"; _goc_j="${_goc_j//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_goc_j"
fi
exit 0
