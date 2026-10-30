#!/usr/bin/env bash
#
# notice-unanswered-question.sh — Stop hook. NEVER blocks.
#
# A QUESTION TO THE CEO THAT HAS NOT BEEN ANSWERED STAYS IN FRONT OF HIM.
#
# guard-ceo-ruled-ask.sh's deaf-lead check (2026-10-01) refuses an
# AskUserQuestion while a teammate of this session is live: that tool held the
# lead's turn open for 37 minutes and everything that could have woken it
# queued behind it. The question goes as the turn's FINAL MESSAGE instead,
# under the fixed lead-in `QUESTION FOR YOU:`. That route has one cost the tool
# did not: every later turn the lead is woken for (a teammate finishing, the
# stall watcher) prints below the question, and he comes back to the newest
# notification, not to the question (Sage's review, richos-hq 8ba32b71, item 9).
#
# So while a `QUESTION FOR YOU:` final message from an earlier turn has no later
# human prompt, this repeats it in the Stop systemMessage at every turn end.
# That channel is shown to the PERSON and never to the model (measured: the
# table below, and escalations.py's DELIVERY section), so it re-surfaces the
# question without waking anything and without telling the model anything new.
# Not de-duplicated, deliberately: the point is that it is under every turn.
# Its own turn is skipped (the question is already the last thing on screen),
# and so is a Stop re-fire after a block (stop_hook_active).
#
# The predicate, and why the lead-in is matched mechanically and never by a
# prose classifier: scripts/lib/blocking_ask.py, `unanswered`.

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
  hook: scripts/hooks/notice-unanswered-question.sh
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

if ! resolve_entity_root "$INPUT"; then
    [ "$RICHOS_ROOT_STATUS" = "not-adopted" ] && exit 0
    root_failure_banner "scripts/hooks/notice-unanswered-question.sh" >&2
    _SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
    [ -f "$_SHN_LIB" ] || exit 0
    # shellcheck source=../lib/stop-hook-notice.sh
    . "$_SHN_LIB"
    stop_notice_init "notice-unanswered-question.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "UNANSWERED-QUESTION WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). A question put to him as a final message is not repeated under later turns."
    exit 0
fi

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# A turn that ends without this check having run must not look like a turn that
# ran it and found no unanswered question: the predicate reads the transcript
# the payload names, so an empty or unreadable payload means nothing was read.
# The sentence comes from scripts/lib/unevaluated-notice.sh so every hook says
# it the same way, through the notice channel measured for the Stop event.
# NO VERDICT CHANGES — this hook never blocks, on these payloads or any other.
# The block below is the shared Stop form, byte for byte apart from the hook
# name and the clause naming what was not checked (unevaluated-payload.test.sh
# case 5b compares the mechanism across every hook that carries it).
_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
[ -f "$_SHN_LIB" ] || exit 0
# shellcheck source=../lib/stop-hook-notice.sh
. "$_SHN_LIB"
stop_notice_init "notice-unanswered-question.sh" "$RICHOS_ENTITY_ROOT_RESOLVED" "$INPUT"
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself, deliberately: stop-hook-visibility.test.sh
        # proves its case 3a by sed-ing out every such call, and a multi-line
        # continuation would leave the argument lines behind as orphaned commands.
        _UE_MSG="$(unevaluated_sentence "notice-unanswered-question.sh" \
            "whether a question put to him as an earlier final message is still unanswered" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

_BA_LIB="$SCRIPT_DIR/../lib/blocking_ask.py"
if [ ! -f "$_BA_LIB" ]; then
    printf '{"suppressOutput":true,"systemMessage":"UNANSWERED-QUESTION WATCH IS OFF: scripts/lib/blocking_ask.py is missing, so a question put to him as a final message is not repeated under later turns."}\n'
    exit 0
fi

OUT="$(printf '%s' "$INPUT" | python3 "$_BA_LIB" unanswered 2>/dev/null)"
RC=$?
# A crashed checker is not "no unanswered question" (hunt part 3 v3, finding 28).
if [ "$RC" != "0" ]; then
    printf '{"suppressOutput":true,"systemMessage":"UNANSWERED-QUESTION WATCH DID NOT RUN: scripts/lib/blocking_ask.py exited %s with no verdict, so an unanswered question is not repeated this turn."}\n' "$RC"
    exit 0
fi
[ -n "$OUT" ] && printf '%s\n' "$OUT"
exit 0
