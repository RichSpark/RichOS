#!/usr/bin/env bash
#
# guard-shared-writes.sh: PreToolUse, a module of the Write chain of
# dispatch-pretooluse.sh (Write, Edit, MultiEdit, NotebookEdit). Several leads
# share one Mac's memory and records (richos-hq spec r3 e3).
#
#   * HIS MEMORY DIRECTORY (<claude dir>/projects/<entity>/memory/): the write
#     takes a short lease first, one file created with O_EXCL holding the
#     session's pid and start time and the tool_use_id. It waits up to 10 s for
#     another session's lease, then refuses naming the holder. A lease is stale
#     when its session has ended or it is older than 5 s. release-shared-writes.sh
#     removes it on PostToolUse AND PostToolUseFailure, so a write a sibling guard
#     refused does not hold everyone else.
#   * A MAIN CHECKOUT OF A FENCED REPOSITORY (the record included): refused unless
#     this session holds that repository's land lease. The path's OWN checkout
#     decides (`git rev-parse` from its directory), never a path prefix, so a
#     native worktree under .claude/worktrees/ is exempt for the right reason.
#     Gitignored paths in the main checkout stay exempt.
#
# THE LIMIT, STATED: a write made through Bash is not seen here. For the record,
# the Git fence refuses the commit without the lease; for memory, this is the
# only fence (r3 e3).
#
# WITH THE SWITCH OFF (the entity's launcher is off or absent) it never starts
# its check, takes no lease and never refuses. An error inside the check passes
# the call and says so.

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
  hook: scripts/hooks/guard-shared-writes.sh
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
    unevaluated_or_continue "guard-shared-writes.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this write into his memory or a fenced main checkout is serialized with the other leads"
fi

if [ -z "$ENTITY_ROOT" ]; then
    exit 0
fi

_GSW_MODE="$SCRIPT_DIR/../lib/operator-mode.sh"
[ -f "$_GSW_MODE" ] || exit 0
# shellcheck source=../lib/operator-mode.sh
. "$_GSW_MODE"
operator_mode_on "$ENTITY_ROOT" || exit 0

_gsw_py=python3
_gsw_err="$(printf '%s' "$INPUT" | OPERATOR_ENTITY_ROOT="$ENTITY_ROOT" "$_gsw_py" "$SCRIPT_DIR/../lib/operator_leads.py" shared-writes-pre 2>&1 >/dev/null)"
_gsw_rc=$?
if [ "$_gsw_rc" = 2 ]; then
    printf '%s\n' "$_gsw_err" >&2
    exit 2
fi
if [ "$_gsw_rc" != 0 ]; then
    _gsw_m="OPERATOR SHARED WRITES CHECK DID NOT RUN (exit $_gsw_rc): this write was NOT serialized with the other leads, and it was let through. ${_gsw_err}"
    _gsw_j="${_gsw_m//\\/\\\\}"; _gsw_j="${_gsw_j//\"/\\\"}"; _gsw_j="${_gsw_j//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_gsw_j"
fi
exit 0
