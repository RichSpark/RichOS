#!/usr/bin/env bash
#
# guard-unresolved-claims.sh — BLOCKING Stop hook. The first enforcement point
# in this engine that observes the ORCHESTRATOR'S BEHAVIOR rather than the
# repository's state.
#
# WHY A Stop HOOK, AND WHAT WAS VERIFIED BEFORE BUILDING ON IT
#   Every other guard here is PreToolUse/PostToolUse/SessionStart — they watch
#   the repo. Two recurring failures happen inside the turn itself, where no
#   repository hook can see them:
#     1. ending a turn by NAMING the next step instead of taking it
#     2. writing an action claim -- "dispatching it now" -- with no
#        corresponding tool call
#   Both happen at exactly the boundary a Stop hook fires on.
#
#   Verified against the SHIPPING BINARY (2.1.251), not documentation:
#     * `Stop` is in the binary's own hook-event list, alongside `StopFailure`
#       and `SubagentStop`.
#     * A Stop hook CAN BLOCK. The binary carries the string "A hook blocked the
#       turn from ending", the telemetry counter `tengu_stop_hook_block_count`,
#       and `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP`. Confirmed live: a Stop hook
#       exiting 2 with stderr stopped the turn, the stderr reached the model,
#       and the model acted on it.
#     * The payload carries `transcript_path`, `last_assistant_message`,
#       `stop_hook_active`, `prompt_id`, `background_tasks`, `session_crons`.
#     * The transcript IS readable at Stop time and already holds this turn's
#       tool_use and tool_result records — but NOT the final assistant text,
#       which has not been flushed yet. That is why the message is read from
#       `last_assistant_message` and only the tool traffic from the transcript.
#     * `stop_hook_active` is true on the re-fire after a block, so the gate can
#       stand itself down and never strand a session.
#
# WHAT BLOCKS AND WHAT ONLY REPORTS
#   Blocking: unresolved AGENT NAMES, unresolved COMMIT SHAs, a BARE-ROLE
#     in-flight claim about a role never dispatched in this session, and a
#     POSITIVE STATE CLAIM -- "landed", "merged", "on main", "pushed" -- whose
#     commit is alive on a branch and is not where the sentence says it is.
#     POSITIVE is load-bearing: a sentence that says a commit has NOT landed is
#     making the opposite claim, and on 2026-09-03 and 2026-09-05 this gate
#     blocked exactly that -- a true statement, by the guard whose job is
#     catching false ones. Polarity is now read from the CLAUSE the state word
#     sits in, before git is asked.
#   Reporting: unresolved FILE PATHS, a prose in-flight-dispatch signal, that
#     signal narrowed to messages naming no agent, a bare-role claim about
#     a role that ran EARLIER and is not running now, a VALUE claimed gone
#     that survives in a spelling the claim did not name, and a NEGATED state
#     claim the repository contradicts (understating what landed has never been
#     the failure this gate protects against) or one whose polarity could not
#     be read at all.
#   The full reasoning — monotonic vs shrinking ground truth, the grounding
#   relaxation, and the measured numbers behind each choice — is in the module
#   docstring of guard-unresolved-claims.py, which is the analysis half.
#
#   Measured on 468 real orchestrator turns across six sessions:
#     agent names   5 tokens    0 false positives
#     commit SHAs   262 tokens  0 false positives
#     file paths    572 tokens  15 false positives (2.6%) -> REPORT ONLY
#     prose signal  418 turns   1 true / 5 false (17% precision) -> REPORT ONLY
#
#   Re-measured 2026-08-31 on 4,134 turns / 3,532 final messages, for the
#   bare-role work. THE NUMBERS DECIDED WHICH HALF BLOCKS:
#     bare-role extraction   103 hits, 103 genuine teammate references (103/103)
#     ...never-dispatched      0 fires -> 0 FP, and 0 TP: it guards a case this
#                              corpus does not contain. BLOCKS, costing nothing.
#     ...spawned-but-not-live  the half that catches the 2026-08-31 failure, and
#                              its FP rate CANNOT be measured — past rosters are
#                              not retained. REPORTS until it can be.
#     prose + names-nobody   29 of the 30 prose hits name nobody, so the filter
#                            removes ONE IN THIRTY: 3 true / 29 = 10.3%, against
#                            the 17% it was proposed to replace. REPORT ONLY.
#
#   Re-measured 2026-09-01 on 2,276 turns / 2,206 final messages across all 79
#   non-scratchpad orchestrator transcripts on this machine, for the STATE-CLAIM
#   work. Method and adjudication: scripts/hooks/state-claims.corpus.md.
#     landed/pushed + SHA    573 sentences, 658 citations
#     ...against today      0 fires
#     ...replayed at the    1 fire (0.15% of citations, 0.045% of turns) -- a
#        turn's own clock     claim made 19 minutes before its commit reached
#                             master, counted as a false positive on purpose
#     value-absence, general 95 fires in 109 literals (87%). REPORT ONLY.
#     value-absence, sharp   fires on 22 of 41 value citations once the absence
#                            word stops being the only thing holding it back.
#                            REPORT ONLY, and the reason is in the analyzer.
#
#   Re-measured 2026-09-05 over 960 state claims in 770 messages, for the
#   POLARITY work (docs/verification/claim-polarity-2026-09-05/):
#     positive               949 -- unchanged, still BLOCKS on a contradiction
#     negated                 11 -- report-only; all 11 read by hand and every
#                                   one is a genuine statement of absence
#     unreadable               0
#   The first draft of the rule looked for a negation anywhere in the SENTENCE
#   and demoted 113 positive claims to report-only. It was the count, not the
#   reasoning, that caught it.
#
# FAIL-OPEN, DELIBERATELY, AND ONLY HERE
#   Every other blocking guard in this engine fails CLOSED. This one does not,
#   and the asymmetry is intentional. A PreToolUse guard that fails closed
#   refuses one tool call. A Stop guard that fails closed refuses to let the
#   session END — it re-fires on every retry until the binary's block cap gives
#   up, and the operator watches a wedged session. The thing being protected
#   here is report integrity, not containment of a destructive act. So a broken
#   install, an unparseable payload, a resolver error or an unreadable
#   transcript all pass the turn through.
#
#   FAILING OPEN IS ONLY DEFENSIBLE IF IT IS SEEN, and for a while it was not.
#   All four ways this guard stops enforcing — stood down by config, no
#   python3, no analyzer, no resolvable root — announced themselves on STDERR
#   and exited 0. Measured against 2.1.251: a Stop hook's stderr AND its plain
#   stdout are both filed into the transcript as a `hook_success` attachment
#   and shown to the operator NOWHERE. So this guard could be switched off and
#   the operator's only evidence that it was protecting him was that it was
#   quiet — which is also exactly what being disabled looks like. Its own
#   header called that "a defense that decays into a rumour" and then built
#   one. The notices now go through scripts/lib/stop-hook-notice.sh, on the one
#   channel proven to reach him. engine-status.sh announces BROKEN at session
#   start as well, but it speaks once, at startup, about a different question.
#
# WHEN IT TAKES EFFECT
#   Hooks snapshot at session start. Installing this changes nothing in the
#   session that installs it; it begins enforcing in the NEXT session.
#
# Exit codes (Claude Code Stop convention):
#   0  nothing unresolved, not adopted, not evaluable, or anything went wrong
#   2  BLOCKED — an identifier in the final message resolves against nothing, or
#      a state claim in it contradicts the repository
#
# Self-test:  scripts/hooks/guard-unresolved-claims.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-unresolved-claims.sh)"

# --- self-test dispatch ---------------------------------------------------
# The suite lives in guard-unresolved-claims.test.sh; this flag exists so the
# hook answers the same question every sibling guard answers.
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/guard-unresolved-claims.test.sh"
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
  hook: scripts/hooks/guard-unresolved-claims.sh
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
# A blocked lead cannot perform the repairs a Stop hook would demand.
if printf '%s' "$INPUT" | python3 "$SCRIPT_DIR/../lib/stop-session-recovery.py"; then
    exit 0
fi

# Resolve the governed repository. Three outcomes — but unlike a PreToolUse
# guard, ALL THREE let the turn end. See "FAIL-OPEN, DELIBERATELY" above.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    # No entity root, so no ledger: this one announces every turn, and should.
    # A guard that cannot tell which repository it governs is not a guard, and
    # is not entitled to be quiet. The full banner still goes to stderr, where
    # the transcript keeps it for forensics; the operator gets the one line.
    stop_notice_init "guard-unresolved-claims.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "CLAIM CHECK — NOT RUNNING: could not resolve which repository it governs, so no agent name, commit SHA or file path in this turn's report was checked against anything. Candidates examined are in this hook's transcript entry. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-unresolved-claims.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_UNRESOLVED_CLAIMS:=1}"

stop_notice_init "guard-unresolved-claims.sh" "$ENTITY_ROOT" "$INPUT"
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
        _UE_MSG="$(unevaluated_sentence "guard-unresolved-claims.sh" \
            "whether this turn ended over a claim it never resolved" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_UNRESOLVED_CLAIMS" = "0" ]; then
    # Never a silent permission: an opt-out that cannot be seen is a defense
    # that decays into a rumour. This line used to write that sentence to
    # STDERR, which the operator cannot see — the guard could be switched off
    # and his only evidence it was protecting him was that it was quiet, which
    # is also exactly what being disabled looks like.
    stop_notice_abnormal "stood-down" \
        "CLAIM CHECK — STOOD DOWN by CHECK_UNRESOLVED_CLAIMS=0 in $CONFIG. Agent names, commit SHAs and file paths in this session's reports are NOT being checked against ground truth. $HOOK_TAG"
    exit 0
fi

# python3 is the analysis half's only dependency. Absent, the turn ends — this
# guard never turns a missing interpreter into an unendable session.
if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "CLAIM CHECK — NOT RUNNING: python3 is not on PATH, so this session's reports are going out unchecked. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/guard-unresolved-claims.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "CLAIM CHECK — NOT RUNNING: the analyzer is missing at $ANALYZER, so this session's reports are going out unchecked. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

set +e
printf '%s' "$INPUT" | RICHOS_CLAIMS_ENTITY_ROOT="$ENTITY_ROOT" \
    RICHOS_CLAIMS_EXTRA_REPOS="${CLAIM_CHECK_EXTRA_REPOS:-}" \
    python3 "$ANALYZER"
RC=$?
set -e

# Only 2 is a block. Anything else — including a crash in the analyzer — lets
# the turn end, but a crash is NOT a clean check: it says so on the notice
# channel instead of passing for one (hunt part 3 v3, finding 28).
if [ "$RC" != "0" ] && [ "$RC" != "2" ]; then
    _CRASH_MSG="CLAIM CHECK — DID NOT RUN THIS TURN: the analyzer exited $RC with no verdict, so this turn's report went out unchecked. An unchecked turn is not a clean one. $HOOK_TAG"
    stop_notice_abnormal "analyzer-crashed" "$_CRASH_MSG"
    exit 0
fi

# It checked this turn. Silent in the ordinary case — the whole point is that a
# working guard says nothing — and one line if the previous state was one of
# the ones above, so the operator who was told it was off is told it is back. A
# Stop hook may write systemMessage to stdout AND exit 2, verified live, and
# the turn is still refused.
stop_notice_normal \
    "CLAIM CHECK — RUNNING AGAIN. Agent names, commit SHAs and file paths in this turn's report were checked against ground truth. $HOOK_TAG"
[ "$RC" = "2" ] && exit 2
exit 0
