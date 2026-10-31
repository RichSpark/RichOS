#!/usr/bin/env bash
#
# guard-idle-land.sh — BLOCKING Stop hook. Refuses to let a turn end when work
# was COMPLETED, nothing further was STARTED, and nothing is OWED TO THE CEO.
#
# THE FAILURE, PRECISELY
#   The orchestrator's working record opens by stating its own rule: a land
#   ends by STARTING the top unblocked item, then reports — not the other way
#   round; the only permitted stop is an item whose next action needs a
#   decision only the CEO can make. That rule was written by the orchestrator.
#   The orchestrator then landed four branches across two repositories, wrote a
#   long report, and ended the turn with an empty dispatch queue and seven
#   unblocked rows still in the file. The operator had to ask "why has
#   everything stopped again" — for the seventh time in two days.
#
#   Every previous answer to that question was a document. This engine has
#   cataloged the same defect a dozen times in a week under one sentence:
#
#       A RULE ENFORCED BY ATTENTION LASTS EXACTLY AS LONG AS THE ATTENTION.
#
#   So the answer here is the answer everywhere else in this engine — a
#   chokepoint that fires whether or not anybody remembers it. Stop is the
#   chokepoint, and guard-unresolved-claims.sh already established, against the
#   shipping binary rather than against documentation, that Stop can block, that
#   the payload carries `transcript_path` / `prompt_id` / `stop_hook_active` /
#   `background_tasks`, and that the turn's tool traffic is readable there.
#   This hook is built on that finding and follows all of its conventions.
#
# WHY THE FIRST VERSION OF THIS FILE DID NOT HOLD — read this before editing
#   It shipped on 2026-08-30, blocking, measured. On 2026-09-01 the operator
#   reported the same failure twice in one day. Its own observation record says
#   why, over 107 landing turns on this machine:
#
#       dispatched (correctly silent)     60
#       background-running (STOOD DOWN)   44     <- 41% of every landing turn
#       backlog-empty (correctly silent)   2
#       block                              1
#
#   Old term 4 stood the gate down whenever `background_tasks` held anything
#   running — and that field is the host's whole task registry: teammates,
#   subagents, shells, monitors, workflows, scans. This orchestrator keeps ten
#   to fifteen teammates alive at all times, so the gate was off almost whenever
#   it mattered. And old term 1 read only one of the two ways work completes
#   here, missing every turn that answers a FINISHED TEAMMATE with a list.
#
# THE PREDICATE — four terms, all four required, none of them read from prose
#   1. this turn COMPLETED something  a `git merge`/`git push` in the turn's own
#                                     tool traffic whose EFFECT is confirmed
#                                     against the repository by identity, OR a
#                                     host-written `<task-notification>` in this
#                                     turn saying an agent finished
#   2. nothing was STARTED            no `Agent` call and no BACKGROUNDED tool
#                                     call this turn, scoped to promptId
#   3. nothing is OWED TO THE CEO     no `AskUserQuestion` this turn — a TOOL
#                                     CALL, never prose. (The second half of
#                                     this term read his own typed messages for
#                                     a hold; deleted 2026-09-20 by CEO ruling
#                                     §68 — no hook decides what he meant. A
#                                     turn he stopped is ended by the
#                                     orchestrator's own `stop-declared:` line.)
#   4. there IS something to start    an unblocked row DERIVED from the record's
#                                     `## Next` table — never a typed count.
#                                     This is also what answers "a teammate is
#                                     still running": the legitimate stop is
#                                     "running AND the next step depends on it",
#                                     and dependency is what `Blocked by` says.
#
#   The derivation, the conservatism rules and the honest list of what none of
#   this can see are in the module docstring of guard-idle-land.py, which is the
#   analysis half. READ THAT FILE; this one is the wiring.
#
# MEASURED BEFORE IT WAS TRUSTED — 1,082 real orchestrator turns, six sessions
#   Replayed by rebuilding each turn's Stop payload against the transcript AS IT
#   STOOD at that turn, and running this hook against the real repositories.
#
#     305  turns ran a `git merge` or `git push` at all
#     276  of those were CONFIRMED by identity (the merged tip is an ancestor of
#          HEAD, or HEAD equals the remote-tracking ref)
#      29  ran the command and were NOT confirmed. That is term 1's identity
#          check earning its place on its own: 29 turns where the message could
#          have said "landed" and the repository did not agree.
#
#   Of the 276 confirmed landings:
#      95  dispatched an Agent in the same turn        -> silent, correct
#     101  still had an agent running                  -> silent, correct
#       0  were held by the operator                   -> THE SUPPRESSOR THAT
#                                                         READ HIS WORDS NEVER
#                                                         FIRED ONCE in 1,082
#                                                         turns. Deleted
#                                                         2026-09-20; the
#                                                         number is kept
#                                                         because it says what
#                                                         the deletion cost.
#      80  landed, started nothing, nothing running    -> BLOCKED  (29%)
#
#   TERM 4 IS DECISIVE AND IS NOT RECONSTRUCTABLE. `background_tasks` comes
#   from the payload — ground truth at run time — and a transcript does not
#   record it. So the replay BRACKETED it, and the bracket is wide enough to be
#   worth stating rather than hiding:
#     presume every agent whose completion notice is missing is still running
#       (over-suppresses; notices are lost, agents get shut down)   ->  10 blocks
#     presume nothing is ever running (never suppresses)            -> 174 blocks
#     count an agent as running only if it demonstrably notified
#       later in the session — the tightest retrospective answer    ->  80 blocks
#   80 is the figure above and the one to argue with.
#
#   READ BY HAND: 14 of the 80. THE MECHANICAL TERMS WERE CORRECT IN ALL 14 —
#   every one really landed, really called no Agent, really had a derivable
#   unblocked row. ZERO cases where the gate misread ground truth.
#     11  the target failure exactly: land, report, stop, and the operator's
#         next message opens a new topic or asks what is next. One of them is
#         literally "yes, clean up the stale richos worktrees. what's next?"
#      3  ended by putting a decision or a click to the operator. Correct BY THE
#         RULE — those rows belong in the CEO's record — but they are the ones
#         that will feel like friction until the deferral route is used.
#
# WHY IT SHIPS BLOCKING AT A 29% FIRE RATE, WHICH IS THE OBVIOUS OBJECTION
#   Because the cost is bounded at ONE EXTRA TURN, structurally. `stop_hook_active`
#   is true on the re-fire, and this gate stands itself down on it — so it can
#   refuse a given turn AT MOST ONCE. The worst case is not a wedged session or a
#   retry storm; it is one more assistant turn, in which the answer is either to
#   dispatch the row or to move it. Both are things that should have happened.
#   And 29% is not the gate being noisy: it is the failure rate the operator has
#   been reporting out loud for two days, now with a number on it.
#
#   IDLE_LAND_ENFORCE=0 in orchestration.config runs it report-only, for an
#   adopter whose record has a different shape and who wants their own numbers
#   before arming it. That key is committed and diffable, like every other
#   stand-down in this engine.
#
# THE ESCAPE IS A DECLARATION, AND A DECLARATION IS NOT A TOKEN
#   This header used to read "NO LIVE OVERRIDE TOKEN", on the argument that an
#   in-the-moment override gets reached for at exactly the moment the gate is
#   working. That argument is RIGHT ABOUT TOKENS and it is why the escape is not
#   one. A flag is free, so it gets typed reflexively; a declaration names WHICH
#   of the three legitimate stops applies and WHY, in a sentence, in the reply
#   the CEO reads — and this wrapper puts that sentence in front of him through
#   `systemMessage` every single time one is used:
#
#       stop-declared: <case> — <why, in a full sentence>
#
#       nothing-unblocked     everything unblocked is genuinely done
#       ceo-owns-it           he stopped this, or his answer IS the deliverable
#       waiting-on-teammate   a teammate is running and the next step needs it
#
#   Six words and thirty characters of reason, minimum. A BARE MARKER EXEMPTS
#   NOTHING — the same discipline `dialect-exempt:` and `main-checkout-run:`
#   already carry in this engine. Moving the row into the CEO's record remains
#   the better escape and is still what the refusal recommends first, because it
#   is committed and diffable; the standing-down key below is in
#   `orchestration.config`, which is also committed and also diffable.
#
# FAIL-OPEN, LIKE ITS SIBLING AND FOR ITS REASON
#   Every other blocking guard in this engine fails CLOSED. The two Stop guards
#   do not. A PreToolUse guard that fails closed refuses one tool call; a Stop
#   guard that fails closed refuses to let the SESSION END, re-firing on every
#   retry until the binary's block cap gives up while the operator watches a
#   wedged session. So a broken install, an unparseable payload, a missing
#   python3, an unreadable or unparsable record — all let the turn end, and the
#   ones worth knowing about SAY SO on stderr rather than dying quietly.
#
# WHEN IT TAKES EFFECT
#   Hooks snapshot at session start. Installing this changes nothing in the
#   session that installs it; it begins enforcing in the NEXT session.
#
# Exit codes (Claude Code Stop convention):
#   0  completed nothing, started something, owed the CEO an answer, declared
#      the stop, nothing to start, not evaluable, or anything went wrong
#   2  BLOCKED — work completed, nothing started, nothing owed to the CEO, and
#      the record has an unblocked row
#
# Self-test:  scripts/hooks/guard-idle-land.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/guard-idle-land.sh)"

# --- self-test dispatch ---------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/guard-idle-land.test.sh"
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
  hook: scripts/hooks/guard-idle-land.sh
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
# guard, ALL THREE let the turn end. See "FAIL-OPEN" above.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    exit 0
else
    # No entity root, so no ledger: this one announces every turn, and should.
    # A guard that cannot tell which repository it governs is not a guard, and
    # is not entitled to be quiet. The full banner still goes to stderr, where
    # the transcript keeps it for forensics; the operator gets the one line.
    stop_notice_init "guard-idle-land.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "IDLE-LAND GATE — NOT RUNNING: could not resolve which repository it governs, so nothing this session ends on was checked against the backlog. $HOOK_TAG"
    root_failure_banner "scripts/hooks/guard-idle-land.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_IDLE_LAND:=1}"
# The record and its section. Defaults name the file the orchestrator's own
# rule lives in; an adopter whose backlog is named something else says so here
# rather than being silently ungoverned.
: "${IDLE_LAND_RECORD:=RICH-TODOs.md}"
: "${IDLE_LAND_SECTION:=Next}"
# 1 = block the turn, 0 = report on stderr and let it end. Report-only exists so
# a repository whose backlog shape has not been measured yet can run the gate
# and read its own numbers before arming it.
: "${IDLE_LAND_ENFORCE:=1}"

stop_notice_init "guard-idle-land.sh" "$ENTITY_ROOT" "$INPUT"
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
        _UE_MSG="$(unevaluated_sentence "guard-idle-land.sh" \
            "whether this turn went idle on a teammate's work that was ready to land" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_IDLE_LAND" = "0" ]; then
    # Never a silent permission: an opt-out that cannot be seen is a defense
    # that decays into a rumour. This line used to write that sentence to
    # STDERR, which the operator cannot see — so the gate could be switched off
    # and his only evidence it was protecting him was that it was quiet, which
    # is also exactly what being disabled looks like.
    stop_notice_abnormal "stood-down" \
        "IDLE-LAND GATE — STOOD DOWN by CHECK_IDLE_LAND=0 in $CONFIG. Turns ending on a named-but-untaken next step are NOT being refused this session. $HOOK_TAG"
    exit 0
fi

# python3 is the analysis half's only dependency. Absent, the turn ends — this
# guard never turns a missing interpreter into an unendable session.
if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "IDLE-LAND GATE — NOT RUNNING: python3 is not on PATH, so turns are ending unchecked this session. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/guard-idle-land.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "IDLE-LAND GATE — NOT RUNNING: the analyzer is missing at $ANALYZER, so turns are ending unchecked this session. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

# THE ANALYZER'S STDOUT IS CAPTURED, NOT PASSED THROUGH. It carries at most one
# line, and only when the turn was let through BY A DECLARATION:
#
#     RICHOS_STOP_DECLARED<TAB>case<TAB>why<TAB>top-row
#
# Its stderr is NOT captured — that is the refusal, and on exit 2 it is what
# reaches the model.
set +e
ANALYZER_OUT="$(printf '%s' "$INPUT" | RICHOS_IDLE_ENTITY_ROOT="$ENTITY_ROOT" \
    RICHOS_IDLE_RECORD="$IDLE_LAND_RECORD" \
    RICHOS_IDLE_SECTION="$IDLE_LAND_SECTION" \
    RICHOS_IDLE_ENFORCE="$IDLE_LAND_ENFORCE" \
    python3 "$ANALYZER")"
RC=$?
set -e

# EXACTLY ONE NOTICE PER TURN, and that is a correctness requirement rather than
# tidiness. The ledger holds ONE state per (session, hook): writing "ok" on the
# way in and a finding on the way out makes the two alternate, so both see a
# changed state and both speak on every turn — de-duplication defeated by the
# hook that owns it. notice-unasked-deferral.sh carries the same paragraph after
# the same bug. So the recovery line moved BELOW the analysis, and it is an
# `else`.
DECLARED_LINE="$(printf '%s\n' "$ANALYZER_OUT" | grep -m1 "^RICHOS_STOP_DECLARED	" || true)"
if [ -n "$DECLARED_LINE" ]; then
    # A DECLARED STOP IS SHOWN TO HIM, ALWAYS. The whole difference between a
    # declaration and a flag is that somebody reads it; a justification filed
    # where nobody looks is a flag with a longer spelling. Keyed on the case
    # plus a hash of the reason, so a DIFFERENT declaration speaks again and the
    # same one restated does not.
    D_CASE="$(printf '%s' "$DECLARED_LINE" | cut -f2)"
    D_WHY="$(printf '%s' "$DECLARED_LINE" | cut -f3 | tr -d '\000-\010\013\014\016-\037')"
    D_TOP="$(printf '%s' "$DECLARED_LINE" | cut -f4)"
    stop_notice_abnormal "declared:${D_CASE}:$(printf '%s' "$D_WHY" | cksum | tr -d ' ')" \
        "STOP DECLARED — this turn completed work, started nothing, and was let through on a declared exception (${D_CASE}), not on a check. The reason given, which is DECLARED AND NOT VERIFIED: \"${D_WHY}\" — the top row it was declared over is ${D_TOP}. $HOOK_TAG"
else
    # Silent in the ordinary case — a working guard says nothing — and one line
    # if the previous state was one of the four stand-downs above. Safe to emit
    # here: a Stop hook may write systemMessage to stdout AND exit 2, verified
    # live, and the turn is still refused.
    stop_notice_normal \
        "IDLE-LAND GATE — RUNNING AGAIN. Turns that complete work and start nothing are being refused once more. $HOOK_TAG"
fi

# Only 2 is a block. Anything else — including a crash in the analyzer — lets
# the turn end.
[ "$RC" = "2" ] && exit 2
exit 0
