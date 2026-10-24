#!/usr/bin/env bash
#
# notice-ceo-unasked.sh — NON-BLOCKING Stop hook. A TURN DOES NOT END QUIETLY
#                         WHILE A PREPARED CEO DECISION HAS NEVER BEEN PUT TO
#                         HIM THIS SESSION.
#
# The predicate, the failure it exists for and the fail-open/fail-closed
# argument are in scripts/lib/ceo-asks.sh. Read that first. This file is the
# wiring and the one sentence.
#
# ===========================================================================
# WHY IT DOES NOT BLOCK, WHEN ITS SIBLING DOES
# ===========================================================================
# guard-ceo-ask-first.sh BLOCKS a teammate dispatch, on the CEO's explicit
# ruling. This hook deliberately does not block the TURN, and the two are not in
# tension: they are aimed at different things.
#
# Dispatching a teammate is MOTION — the specific act that beat the ask on
# 2026-08-31 — and refusing it costs the operator one question. Ending a turn is
# not motion. A turn that ends BECAUSE he interrupted, or in order to answer
# him, or because the work is done, is a turn ending correctly with the same
# items outstanding. Refusing those would make the whole mechanism something to
# switch off, and a switched-off guard protects nothing forever. The same trade
# notice-unstarted-rows.sh and notice-inflight-acks.sh make, one and two events
# over, for the same reason.
#
# So: the gate is the floor (ONE item), this notice is the completeness (ALL the
# items he has not seen), and neither pretends to be the other.
#
# ===========================================================================
# NAMED, NOT COUNTED — and this is the actual point
# ===========================================================================
# "13 items waiting" is a count. It is also, almost verbatim, the sentence that
# got demoted on the morning this was ordered: the orchestrator had a backlog
# and reported it, and the CEO's own questions stayed inside it. A specific
# answerable question is what got answered in seconds when it finally arrived.
#
# So this notice names ONE item and renders it AS A QUESTION, with the count
# behind it rather than in front of it.
#
# ONE LINE, STATE-CHANGE DE-DUPLICATED, through scripts/lib/stop-hook-notice.sh
# — the only channel measured to reach the operator (that file carries the
# measurement table; on exit 0 the host shows him neither stdout nor stderr).
# The state key is the SET of unasked items, so the notice speaks again the
# moment that set changes — which is exactly the moment one of them is finally
# put to him, and the moment a new one is prepared.
#
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — the predicate is the CEO's own list. It reads session_id from the
# payload, but only as a notice de-duplication key: losing it makes the notice
# less well de-duplicated, it does not make the check not run.
#
# WHY THIS ONE NEEDS A DECLARATION WHERE THE OTHER EXEMPT HOOKS DO NOT.
# scripts/hooks/unevaluated-payload.test.sh derives every registered PreToolUse
# and Stop hook from hooks/hooks.json and drives each with an empty, a truncated
# and a non-JSON payload. A hook that REFUSES them is proven fail-closed by that
# alone; a hook that ANNOUNCES on them is proven audible by that alone; neither
# needs a word in its source. But a hook that is SILENT on all four looks
# identical whether its predicate never needed the payload or its predicate was
# silently lost — which is the whole defect, and it is the one thing driving a
# hook cannot tell you. So payload-independence is the single class that must be
# CLAIMED by a person, and the suite then holds the claim to its consequence:
# the output must be identical on all four payloads. An undeclared silent hook
# fails that suite; a falsely declared one fails it too.

set -eo pipefail

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
  hook: scripts/hooks/notice-ceo-unasked.sh
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

_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
[ -f "$_SHN_LIB" ] || exit 0
# shellcheck source=../lib/stop-hook-notice.sh
. "$_SHN_LIB"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    # A Stop hook that blocks on a broken install re-fires to the block cap and
    # strands the session. It says so instead, on the one channel that reaches
    # the operator, and stops.
    root_failure_banner "scripts/hooks/notice-ceo-unasked.sh" >&2
    stop_notice_init "notice-ceo-unasked.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "CEO-ASK WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether a prepared CEO decision has been put to him — run scripts/ceo-asks-status.sh by hand."
    exit 0
fi

stop_notice_init "notice-ceo-unasked.sh" "$ENTITY_ROOT" "$INPUT"

_CA_LIB="$SCRIPT_DIR/../lib/ceo-asks.sh"
if [ ! -f "$_CA_LIB" ]; then
    stop_notice_abnormal "no-lib" \
        "CEO-ASK WATCH IS OFF: scripts/lib/ceo-asks.sh is missing, so no prepared CEO decision was checked. A clean session and an absent checker must never look the same."
    exit 0
fi
# shellcheck source=../lib/ceo-asks.sh
. "$_CA_LIB"

if ! ca_require; then
    stop_notice_abnormal "cannot-run:$(printf '%s' "$CA_BROKEN" | cksum | tr -d ' ')" \
        "CEO-ASK WATCH IS BROKEN, so nothing was checked: ${CA_BROKEN}. Do not read this silence as an empty list."
    exit 0
fi

RRC=0
ca_resolve "$ENTITY_ROOT" || RRC=$?
case "$RRC" in
    0) ;;
    1)
        # NOT-DECLARED is the ordinary answer in every repository with no CEO
        # list, and announcing it everywhere would be the noise this engine
        # already decided not to make. stop_notice_normal is still called so a
        # repository that RECOVERS from a broken state gets the end of its story.
        stop_notice_normal ""
        exit 0 ;;
    *)
        # THE LOUD FAIL-OPEN. guard-ceo-ask-first.sh also announces this, but on
        # a channel that is unproven for PreToolUse. This is the one that is
        # measured, so this is the guarantee.
        stop_notice_abnormal "broken:$(printf '%s' "$CA_REASON" | cksum | tr -d ' ')" \
            "CEO-ASK WATCH IS BROKEN: this repository DECLARES CEO TODOs and they cannot be read — ${CA_REASON}. Teammate dispatches are UNGATED and no prepared decision is being surfaced. A declared-but-unreadable list is not an empty one."
        exit 0 ;;
esac

# Read here rather than through stop-hook-notice.sh's own extractor: that one is
# private to the de-duplication ledger, and a notice reaching into a sibling's
# internals is a coupling that breaks silently. Same shape as
# notice-inflight-acks.sh's.
SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(str(d.get("session_id","") or "") if isinstance(d,dict) else "")
except Exception: print("")' 2>/dev/null || true)"

# A QUESTION IN THE REPLY IS AN ASK (CLAUDE.md §125: "A question never holds up work:
# it goes in the reply with the current default stated"). The reply's text is matched
# against his prepared items exactly as an AskUserQuestion is (ca_match), and a match
# is recorded the same way, so it is not asked again.
REPLY="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(str(d.get("last_assistant_message","") or "") if isinstance(d,dict) else "")
except Exception: print("")' 2>/dev/null || true)"
if [ -n "$REPLY" ] && [ -n "$SESSION_ID" ]; then
    _CU_ITEMS="$(mktemp -t ceo-unasked-items.XXXXXX)" || _CU_ITEMS=""
    _CU_Q="$(mktemp -t ceo-unasked-q.XXXXXX)" || _CU_Q=""
    if [ -n "$_CU_ITEMS" ] && [ -n "$_CU_Q" ] && ca_items_json "$_CU_ITEMS"; then
        printf '%s' "$REPLY" > "$_CU_Q"
        _CU_LINE="$(ca_match "$_CU_Q" "$_CU_ITEMS" 2>/dev/null || true)"
        if [ "$(printf '%s' "$_CU_LINE" | cut -f1)" = "MATCH" ]; then
            CA_LINE="$_CU_LINE" CA_SID="$SESSION_ID" CA_LEDGER="$(ca_ledger_path "$ENTITY_ROOT")" python3 -c '
import json, os
from datetime import datetime, timezone
f = (os.environ["CA_LINE"].split("\t") + [""] * 7)[:7]
rec = {"timestamp": datetime.now(timezone.utc).isoformat(), "event": "CeoAsk", "source_hook": "Stop[reply]",
       "session_id": os.environ["CA_SID"], "matched_item": f[2], "match": f[0], "repo": f[1], "score": f[3],
       "hits": f[4], "needed": f[5], "how": f[6], "discharges": True}
os.makedirs(os.path.dirname(os.environ["CA_LEDGER"]), exist_ok=True)
with open(os.environ["CA_LEDGER"], "a", encoding="utf-8") as fh:
    fh.write(json.dumps(rec) + "\n")
' 2>/dev/null || true
        fi
    fi
    rm -f "$_CU_ITEMS" "$_CU_Q"
fi
ARC=0
ca_assess "$ENTITY_ROOT" "$SESSION_ID" || ARC=$?
if [ "$ARC" -ge 2 ]; then
    stop_notice_abnormal "assess-broken:$(printf '%s' "${CA_BROKEN:-}" | cksum | tr -d ' ')" \
        "CEO-ASK WATCH SWEPT NOTHING — ${CA_BROKEN:-the predicate could not run}. This is not an empty list; it is an unread one. Detail: ${ENGINE_ROOT}/scripts/ceo-asks-status.sh"
    exit 0
fi

if [ "${CA_UNASKED:-0}" -eq 0 ]; then
    stop_notice_normal ""            # asked: nothing more to say to anyone
    exit 0
fi

TOP="$(printf '%s\n' "$CA_ASK_LINES" | head -1)"
TOP_ID="$(printf '%s' "$TOP" | cut -f2)"
TOP_ASK="$(printf '%s' "$TOP" | cut -f4)"
IDS="$(printf '%s\n' "$CA_ASK_LINES" | awk -F'\t' '{printf "%s ", $2}')"

MORE=""
if [ "${CA_UNASKED:-0}" -gt 1 ]; then
    MORE=" ${CA_UNASKED} of his prepared items have not been put to him in this session."
fi

# TO THE LEAD (Stop additionalContext), once per set of unasked items: the lead asks,
# so the lead must read it; a systemMessage reaches only the person.
_CU_MSG="CEO TODO ${TOP_ID} has not been put to him this session: ${TOP_ASK}${MORE} First check it is still current (already done or answered: close it on the record instead). If it is, ask it in your next reply with the current default stated, and keep independent work moving (CLAUDE.md §125); a blocking AskUserQuestion only if the next action truly waits on it. Full list: ${ENGINE_ROOT}/scripts/ceo-asks-status.sh"
if [ "$(_shn_prior)" != "unasked:$IDS" ]; then
    _shn_record "unasked:$IDS" "$(date +%s 2>/dev/null || echo '')"
    printf '%s\n' "{\"hookSpecificOutput\":{\"hookEventName\":\"Stop\",\"additionalContext\":\"$(_shn_escape "$_CU_MSG")\"}}"
else
    _shn_record "unasked:$IDS" "$(_shn_prior_emit_epoch)"
fi
exit 0
