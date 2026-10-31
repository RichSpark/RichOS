#!/usr/bin/env bash
#
# turn-manifest.sh — NON-BLOCKING Stop hook. It prints, at the end of every
# turn, the tool calls that turn made and THE STATUS EACH ONE ACTUALLY
# RETURNED — generated from the results, never written by the assistant.
#
# WHY IT EXISTS
#   The orchestrator's recurring failure is reporting the strongest reading of
#   a narrow result: "Message queued for delivery at its next tool round"
#   reported as "I've told him"; no tool call at all reported as "dispatching
#   it rather than queuing it"; one glob matching 18 files reported as "18/18
#   suites". Every one of those was caught by the CEO reading the reply, which
#   makes him the detection mechanism for the orchestrator's own claims. That
#   is the last unmechanized failure class he has actually been hit by, and it
#   has to fire without him.
#
# WHY IT IS NOT A DETECTOR — the design is the point
#   Nothing here reads the assistant's prose, scores it, or decides whether a
#   sentence overstates. There is no threshold and no interpretation, so the
#   false-positive rate is ZERO BY CONSTRUCTION. It does not CATCH an
#   overstatement; it makes one UNSUSTAINABLE, because the true status is
#   printed directly beneath the claim. Unrepresentable rather than
#   detectable.
#
#   Judging the prose is a real job with a real owner: guard-unresolved-
#   claims.sh, which measured its own precision (17% for the prose signal —
#   report-only; 0 false positives on 262 SHA citations — blocking) before it
#   was allowed near a verdict. If a future edit to THIS file starts asking
#   whether a claim was fair, the edit belongs in that one instead.
#
# IT NEVER BLOCKS. Every path exits 0. A manifest that can refuse a turn needs
#   an opinion about which turns deserve refusing — which is the prose
#   judgment this hook exists to avoid.
#
# WHERE THE TEXT GOES — established by running it, not by assuming
#   A Stop hook reaches the OPERATOR by writing {"systemMessage": "..."} to
#   stdout and exiting 0. Verified against the shipping binary 2.1.251 both
#   statically (the command-hook runner lifts `systemMessage` out of the
#   hook's parsed stdout and the consumer yields a `hook_system_message`
#   tagged with the event; the binary's own bundled docs name the pattern
#   under "Stop hook that displays message to user") and LIVE (a headless run
#   with one such Stop hook produced
#   {"type":"system","subtype":"informational","content":"Stop says: …"} after
#   the assistant's final text). Multi-line survives; the host prefixes EVERY
#   line with "Stop says: ", which is why the rows are kept narrow.
#   engine-status.sh already reaches the operator this same way.
#
# FAIL OPEN, like every Stop hook here, for the reason in the sibling's
#   header: a Stop guard that fails CLOSED refuses to let the SESSION END and
#   re-fires until the binary's block cap gives up while the operator watches
#   a wedged session. A missing resolver, an unadopted repo, an absent
#   python3, an unreadable transcript — all end the turn.
#
#   The one thing it will not do quietly is DEGRADE. "Could not build the
#   manifest" is rendered as an explicit UNAVAILABLE notice, because silence
#   is indistinguishable from "this turn ran no tools", and those two facts
#   must never look alike.
#
# WHEN IT TAKES EFFECT
#   Hooks snapshot at session start. Installing this changes nothing in the
#   session that installs it; it begins rendering in the NEXT session.
#
# Exit code: 0, always.
#
# Self-test:  scripts/hooks/turn-manifest.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/turn-manifest.sh)"

# --- self-test dispatch ---------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/turn-manifest.test.sh"
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
  hook: scripts/hooks/turn-manifest.sh
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

# --- NOTICE CHANNEL --------------------------------------------------------
# A Stop hook's stand-down and cannot-run notices go to the OPERATOR, never to
# stderr. The measurement behind that, and the argument for announcing on state
# change rather than every turn, are in scripts/lib/stop-hook-notice.sh. This
# block is byte-identical in every Stop hook and stop-hook-visibility.test.sh
# asserts it, for the reason Layer R asserts the same of the root bootstrap: a
# divergent copy is one hook disagreeing with its siblings about how it tells
# you it has stopped working.
_SHN_LIB="$SCRIPT_DIR/../lib/stop-hook-notice.sh"
if [ -f "$_SHN_LIB" ]; then
    # shellcheck source=../lib/stop-hook-notice.sh
    . "$_SHN_LIB"
else
    # The helper is the thing that makes these notices visible, so its absence
    # must not make them invisible. The hook then announces EVERY turn,
    # undeduplicated, and says why. Degrading toward noise is recoverable by an
    # operator who can read it; degrading toward silence rebuilds the defect.
    stop_notice_init() { :; }
    stop_notice_normal() { :; }
    stop_notice_abnormal() {
        printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"NOTICE HELPER MISSING at $_SHN_LIB, so this is unconditional and undeduplicated: ${2:-}\"}"
        return 0
    }
    # The recurring form degrades to the same unconditional announcement. An
    # interval it cannot honor is announced MORE often, never less: a notice
    # channel's degraded mode leans toward noise, because noise is recoverable
    # by an operator who can read it and silence rebuilds the defect.
    stop_notice_abnormal_recurring() {
        printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"NOTICE HELPER MISSING at $_SHN_LIB, so this is unconditional and undeduplicated: ${2:-}\"}"
        return 0
    }
fi

INPUT="$(cat)"

# Resolve the governed repository. All three outcomes end the turn.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    # No entity root, so no ledger: this one announces every turn, and should.
    # A renderer that cannot tell which repository it is describing has nothing
    # to describe, and a missing manifest looks exactly like a turn that ran no
    # tools. The full banner still goes to stderr, where the transcript keeps
    # it for forensics; the operator gets the one line.
    stop_notice_init "turn-manifest.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "TURN MANIFEST — UNAVAILABLE: could not resolve which repository it governs, so this turn's tool statuses were not rendered. This is a GAP IN THE RECORD, not a turn that ran nothing. $HOOK_TAG"
    root_failure_banner "scripts/hooks/turn-manifest.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${SHOW_TURN_MANIFEST:=1}"

stop_notice_init "turn-manifest.sh" "$ENTITY_ROOT" "$INPUT"
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# A turn that ends without this check having run must not look like a turn that
# ran it and found nothing. The sentence comes from
# scripts/lib/unevaluated-notice.sh so every hook says it the same way, and it
# goes out through the same notice channel the stand-downs above use, which is
# the one measured for the Stop event. NO VERDICT CHANGES — this hook already
# ended the turn on exactly these payloads.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    if _UE_REASON="$(richos_payload_unreadable "$INPUT")"; then
        # ONE LINE for the call itself, deliberately: stop-hook-visibility.test.sh
        # proves its case 3a by sed-ing out every such call, and a multi-line
        # continuation would leave the argument lines behind as orphaned commands.
        _UE_MSG="$(unevaluated_sentence "turn-manifest.sh" \
            "the manifest of what this turn actually did" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

# A stand-down is ANNOUNCED THROUGH THE SAME CHANNEL the manifest uses, never
# on stderr. MEASURED, not assumed: a Stop hook exiting 0 that wrote a unique
# marker to stderr AND a second one to plain stdout produced neither in the
# session stream. Both were filed into the transcript as a `hook_success`
# attachment — readable in transcript view, invisible in the scroll the
# operator is actually looking at. Only `systemMessage` reaches that scroll.
# So a stderr notice here would be an opt-out nobody can see: a defense that
# decays into a rumour, which is the failure the sibling guards warn about in
# words and this one would have committed in silence.
#
# The channel was already right; the CADENCE was not. This notice fired under
# every turn, and identical text under every turn is text the eye stops reading
# — at which point the operator has a line on screen reporting the stand-down
# and no longer reads it, which buys exactly what the stderr notice bought. It
# now announces on STATE CHANGE, including the change back to normal, so his
# last-seen notice is always the current state. The argument in full is in
# scripts/lib/stop-hook-notice.sh.
if [ "$SHOW_TURN_MANIFEST" = "0" ]; then
    stop_notice_abnormal "stood-down" \
        "TURN MANIFEST — STOOD DOWN by SHOW_TURN_MANIFEST=0 in $CONFIG. This session's tool statuses are not being rendered. $HOOK_TAG"
    exit 0
fi

# python3 is the renderer's only dependency. Absent, the turn ends — but the
# operator is told, because a manifest that is silently absent looks exactly
# like a turn that ran no tools.
if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "TURN MANIFEST — UNAVAILABLE: python3 is not on PATH, so this session's tool statuses cannot be rendered. This is a GAP IN THE RECORD, not turns that ran nothing. $HOOK_TAG"
    exit 0
fi

RENDERER="$SCRIPT_DIR/turn-manifest.py"
if [ ! -f "$RENDERER" ]; then
    stop_notice_abnormal "no-renderer" \
        "TURN MANIFEST — UNAVAILABLE: the renderer is missing at $RENDERER, so this session's tool statuses cannot be rendered. This is a GAP IN THE RECORD, not turns that ran nothing. $HOOK_TAG"
    exit 0
fi

# Never let a crash in the renderer wedge turn-end, and never let a partial
# write reach the host as malformed JSON: the output is buffered, and emitted
# only if python exited cleanly.
set +e
OUT="$(printf '%s' "$INPUT" | RICHOS_MANIFEST_ENTITY_ROOT="$ENTITY_ROOT" \
    python3 "$RENDERER" 2>/dev/null)"
RC=$?
set -e

if [ "$RC" = "0" ]; then
    # RC 0 AND NO OUTPUT IS A DESIGNED PATH, NOT A FAILURE. The renderer
    # deliberately prints nothing on a `stop_hook_active` re-fire, so that a
    # blocked turn does not re-render its manifest on every retry. An earlier
    # draft of this stanza treated empty-with-rc-0 as a crash and announced
    # "the renderer produced nothing" on exactly that path; the suite's case g1
    # caught it. Only a NON-ZERO rc is a failure here.
    #
    # Silent on purpose, and with NO recovery message. Every other Stop hook
    # gets one, because its healthy state produces no output and the operator
    # who was told it was off has no other way to learn it came back. This one
    # is different: the manifest itself is per-turn evidence that it ran, so
    # the line below IS the recovery notice. Emitting a second systemMessage
    # alongside it would also mean two JSON documents on one hook's stdout,
    # which is not a shape the host is known to accept.
    stop_notice_normal
    # An explicit `if`, not `[ -n "$OUT" ] && printf`: under `set -e` the
    # short-circuiting form returns 1 on the empty branch and would end the
    # hook with rc 1 instead of 0.
    if [ -n "$OUT" ]; then
        printf '%s\n' "$OUT"
    fi
    exit 0
fi

# The renderer CRASHED. This used to end the turn in silence, which contradicts
# this file's own contract at the top — "the one thing it will not do quietly
# is DEGRADE" — because a missing manifest is indistinguishable from a turn
# that ran no tools. It is a gap in the record and it is now named as one.
stop_notice_abnormal "renderer-failed" \
    "TURN MANIFEST — UNAVAILABLE: the renderer exited $RC, so this session's tool statuses are not being rendered. This is a GAP IN THE RECORD, not turns that ran nothing. $HOOK_TAG"
exit 0
