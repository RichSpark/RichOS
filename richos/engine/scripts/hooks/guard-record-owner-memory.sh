#!/usr/bin/env bash
#
# guard-record-owner-memory.sh: PreToolUse, a module of the Write chain of
# dispatch-pretooluse.sh. Once the RichOS app owns his record, his plain terminal
# does not write his memory directory (daily-driver plan step 8; two-installs
# spec points 25 and 26).
#
# THE SWITCH is the record's owner line, ROW_RECORD_OWNER="app <memory dir>" in
# the record's own .row-currency (scripts/lib/row-currency.sh, rc_record_owner).
# The record is found from the seat: the seat's .row-currency is the record
# (record form) or points at it (peer form, ROW_RECORD_REPO), read from the MAIN
# checkouts either way. With no owner line, which is the state until cut-over,
# this guard refuses nothing and says nothing. Deleting the line reverses it.
#
# With the line present it refuses a Write, Edit, MultiEdit or NotebookEdit whose
# target is inside the named memory directory, when the caller is his terminal
# or cannot be identified. The app's own lead passes. Who is calling is
# scripts/lib/record_owner.py's answer: the platform's own session record for the
# nearest Claude process in the call's ancestry (pid plus kernel start time,
# entrypoint `sdk-*` for the app), never a name, a path or a working directory.
#
# NOT COVERED, stated rather than implied: a Bash command that writes into the
# directory (`echo >> MEMORY.md`), exactly as the operator claim's memory rule;
# and a session whose seat declares no row-currency contract at all.
#
# A declaration that is BROKEN is announced and the write passes: the landing
# guard already refuses every landing over a broken declaration, and a typo in
# an unrelated key must not be what stops his memory.

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
  hook: scripts/hooks/guard-record-owner-memory.sh
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

SEAT_ROOT=""
resolve_entity_root "$INPUT" && SEAT_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"

_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-record-owner-memory.sh" "$INPUT" \
        "${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}" \
        "whether this write goes into his memory directory while the RichOS app owns it"
fi

# Cheap exits first, bash only: no seat, or a payload that cannot name a
# memory directory, never reaches a file read or an interpreter.
[ -n "$SEAT_ROOT" ] || exit 0
case "$INPUT" in
    *memory*) ;;
    *) exit 0 ;;
esac

_ROM_RC_LIB="$SCRIPT_DIR/../lib/row-currency.sh"
if [ ! -f "$_ROM_RC_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-record-owner-memory.sh"
        echo "  scripts/lib/row-currency.sh is missing at: $_ROM_RC_LIB"
    } >&2
    exit 0
fi
# shellcheck source=../lib/row-currency.sh
. "$_ROM_RC_LIB"

_ROM_RC=0
rc_record_owner "$SEAT_ROOT" || _ROM_RC=$?
case "$_ROM_RC" in
    0) ;;
    1) exit 0 ;;
    *)
        _rom_m="RECORD OWNER CHECK DID NOT RUN: the row-currency declaration is broken (${RC_BROKEN_REASON:-no reason given}), so this write was NOT checked against the record's owner line and it was let through."
        _rom_j="${_rom_m//\\/\\\\}"; _rom_j="${_rom_j//\"/\\\"}"; _rom_j="${_rom_j//$'\n'/\\n}"
        printf '{"systemMessage":"%s"}\n' "$_rom_j"
        exit 0 ;;
esac
[ "$RC_OWNER" = "app" ] || exit 0

_rom_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
_rom_err="$(printf '%s' "$INPUT" | "$_rom_py" "$SCRIPT_DIR/../lib/record_owner.py" memory-guard \
    "$RC_OWNER_MEMORY" "$RC_OWNER_DECL" 2>&1 >/dev/null)"
_rom_rc=$?
if [ "$_rom_rc" = 2 ]; then
    printf '%s\n' "$_rom_err" >&2
    exit 2
fi
if [ "$_rom_rc" != 0 ]; then
    # The owner line is set and the check itself failed: refuse, because "could
    # not tell" must not read as "the terminal may write his memory".
    printf '%s\n' "=== RECORD OWNER CHECK FAILED (exit $_rom_rc) — REFUSING ===" \
        "  The RichOS app owns his memory ($RC_OWNER_MEMORY), and this write could not be" \
        "  checked. ${_rom_err}" "(hook: scripts/hooks/guard-record-owner-memory.sh)" >&2
    exit 2
fi
exit 0
