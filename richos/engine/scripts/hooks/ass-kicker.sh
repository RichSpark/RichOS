#!/usr/bin/env bash
#
# ass-kicker.sh — BLOCKING Stop hook. Refuses to let a turn end when
# its REPORT does not match its ACTIONS, in three arms:
#
#   ARM 1  STATED, NOT TAKEN    the final text states an action ("Frank breaks
#                               it first", "I'm dispatching Zach") and the
#                               turn's tool calls do not contain it.
#   ARM 2  THE TURN THAT STOPS  a teammate's completion arrived in this turn,
#                               and the turn ends having started nothing and
#                               declared nothing.
#   ARM 3  RECORDED, NOT        the final text claims something was recorded,
#          WRITTEN              saved, written down or made a rule, and the
#                               turn wrote it nowhere durable, or the reply
#                               does not say where (path, or record and
#                               section, and the commit for a repository).
#                               The CEO, 2026-10-02: "Get a hook added that
#                               HITS YOU WITH A HAMMER every time you claim to
#                               have 'recorded' something without actually
#                               doing it." ARM 3 runs on the re-fire as well,
#                               and has no declaration line.
#
# THE FAILURE, PRECISELY
#   2026-09-02. Seven times in one session the lead wrote a sentence describing
#   an action and treated having written it as having done it. "Zach builds it
#   tomorrow." — no Agent call. "Frank breaks it first" — no Agent call. The
#   CEO: asked in blunt terms where Frank was this time, and then, on reading
#   the apology, said this class of failure would never end.
#
#   The same day, six times, a teammate returned, the lead answered it with a
#   report, and the turn ended having started nothing — and he had to send
#   "No Frank this time?" / "Next Sage." / a blunter repeat of the same
#   to restart work that should never have paused. guard-idle-land.sh SAW all
#   three of the evening's finishes and stood down on its backlog term (rows
#   41, free 0): the next step after a returned design is not a backlog row.
#
#   The measured lesson of that day: every rule left as PROSE was broken, and
#   every rule with a GUARD caught the lead — six refusals from five guards in
#   one evening. So this is a guard, and both arms BLOCK. A notice that names
#   the failure and lets the turn end is the same defect as a reaper printing
#   CLEAN over 14 unlanded commits.
#
# THE PREDICATES, THE MEASUREMENT AND WHAT WAS REFUSED are in the module
#   docstring of ass-kicker.py, the analysis half. READ THAT FILE;
#   this one is the wiring. The numbers in one paragraph, from a replay of
#   1,275 real turns (19 sessions, 2026-07-27 to 2026-09-02) through the
#   shipped analyzer — method and adjudication in stated-actions.corpus.md:
#
#     ARM 1a  role act      "Zach builds it tomorrow"     2 fires, 2 genuine, 0 false  BLOCKS
#     ARM 1b  I'm dispatching <who>                        0 fires (10 raw, all CEO proposals)  BLOCKS
#     ARM 1c  role future   "Sage will fold it in"         2 fires, 0 genuine  REPORTS
#     ARM 2   undeclared stop after a completion           263 of 366 completion turns  BLOCKS
#             independent reader agrees on all 263; the stops divide into the
#             three declared cases and the defect, which is what the
#             declaration exists to tell apart
#
# THE ESCAPE IS A DECLARATION, AND IT IS guard-idle-land.sh's DECLARATION
#   One vocabulary, by import. ARM 2 passes on the line that gate documents:
#
#       stop-declared: <case> — <why, in a full sentence>
#
#       nothing-unblocked     everything unblocked is genuinely done
#       ceo-owns-it           he stopped this, or his answer IS the deliverable
#       waiting-on-teammate   a teammate is running and the next step needs it
#
#   Six words and thirty characters of reason, minimum; a BARE MARKER EXEMPTS
#   NOTHING; and this wrapper puts every declaration in front of the CEO
#   through `systemMessage`, unverified and labeled as such. ARMS 1 and 3
#   have no escape and need none: the honest routes are to make the call or
#   write the record, or to write what is true.
#
# FAIL-OPEN, LIKE ITS SIBLINGS AND FOR THEIR REASON
#   A PreToolUse guard that fails closed refuses one tool call; a Stop guard
#   that fails closed refuses to let the SESSION END. So a broken install, an
#   unparseable payload, a missing python3, a missing analyzer, an unreadable
#   transcript — all let the turn end, and the ones worth knowing about SAY SO
#   on the operator channel rather than dying quietly.
#
# WHEN IT TAKES EFFECT
#   Hooks snapshot at session start. Installing this changes nothing in the
#   session that installs it; it begins enforcing in the NEXT session.
#
# Exit codes (Claude Code Stop convention):
#   0  report matches the turn, declared, exempt, not evaluable, stood down,
#      or anything went wrong
#   2  BLOCKED — the report states an action the turn did not take, or a
#      teammate finished and the turn ends undeclared having started nothing,
#      or the report claims a record the turn did not write or does not locate
#
# Self-test:  scripts/hooks/ass-kicker.sh --self-test

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/ass-kicker.sh)"

# --- self-test dispatch ---------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/../../ass-kicker/tests/ass-kicker.test.sh"
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
  hook: scripts/hooks/ass-kicker.sh
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
    stop_notice_init "ass-kicker.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "STATED-ACTIONS GATE — NOT RUNNING: could not resolve which repository it governs, so no report this session was checked against the turn's own tool calls. $HOOK_TAG"
    root_failure_banner "scripts/hooks/ass-kicker.sh" >&2
    exit 0
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_STATED_ACTIONS:=1}"
# 1 = block the turn, 0 = report on stderr and let it end. Report-only exists
# so an adopter can run the gate and read its own numbers before arming it.
: "${STATED_ACTIONS_ENFORCE:=1}"

stop_notice_init "ass-kicker.sh" "$ENTITY_ROOT" "$INPUT"
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
        _UE_MSG="$(unevaluated_sentence "ass-kicker.sh" \
            "whether this turn stated an action it did not take" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

if [ "$CHECK_STATED_ACTIONS" = "0" ]; then
    # Never a silent permission: an opt-out that cannot be seen is a defense
    # that decays into a rumor.
    stop_notice_abnormal "stood-down" \
        "STATED-ACTIONS GATE — STOOD DOWN by CHECK_STATED_ACTIONS=0 in $CONFIG. A report stating an action the turn did not take, or a turn that stops after a teammate returns, is NOT being refused this session. $HOOK_TAG"
    exit 0
fi

# python3 is the analysis half's only dependency. Absent, the turn ends — this
# guard never turns a missing interpreter into an unendable session.
if ! command -v python3 >/dev/null 2>&1; then
    stop_notice_abnormal "no-python3" \
        "STATED-ACTIONS GATE — NOT RUNNING: python3 is not on PATH, so turns are ending unchecked this session. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

ANALYZER="$SCRIPT_DIR/../../ass-kicker/ass-kicker.py"
if [ ! -f "$ANALYZER" ]; then
    stop_notice_abnormal "no-analyzer" \
        "STATED-ACTIONS GATE — NOT RUNNING: the analyzer is missing at $ANALYZER, so turns are ending unchecked this session. An unchecked turn is not a clean one. $HOOK_TAG"
    exit 0
fi

# The analyzer imports two siblings rather than copying them: turn-manifest.py
# for THIS TURN's call list (the authority on the turn boundary) and
# guard-idle-land.py for the completion signal, the hold, and the declaration
# vocabulary. Either missing is this gate half-blind, and half-blind is
# announced, not absorbed.
for _DEP in turn-manifest.py guard-idle-land.py; do
    if [ ! -f "$SCRIPT_DIR/$_DEP" ]; then
        stop_notice_abnormal "no-$_DEP" \
            "STATED-ACTIONS GATE — NOT RUNNING: $SCRIPT_DIR/$_DEP is missing, and this gate reads the turn through it rather than through a second copy. Turns are ending unchecked this session. $HOOK_TAG"
        exit 0
    fi
done

# THE ANALYZER'S STDOUT IS CAPTURED, NOT PASSED THROUGH. It carries at most one
# line, and only when ARM 2 was let through BY A DECLARATION:
#
#     RICHOS_STOP_DECLARED<TAB>case<TAB>why<TAB>finished-agent-title
#
# Its stderr is NOT captured — that is the refusal, and on exit 2 it is what
# reaches the model.
set +e
ANALYZER_OUT="$(printf '%s' "$INPUT" | RICHOS_SA_ENTITY_ROOT="$ENTITY_ROOT" \
    RICHOS_SA_ENFORCE="$STATED_ACTIONS_ENFORCE" \
    python3 "$ANALYZER")"
RC=$?
set -e

# Failing open must not turn a failed analyzer into a healthy observation.
if [ "$RC" != "0" ] && [ "$RC" != "2" ]; then
    stop_notice_abnormal "analyzer-exit:$RC" \
        "STATED-ACTIONS GATE: NOT RUNNING. The analyzer exited $RC without a verdict. This turn was not checked; it is allowed to end. Inspect the hook error output. $HOOK_TAG"
    exit 0
fi

# EXACTLY ONE NOTICE PER TURN — the ledger holds one state per (session, hook),
# so the recovery line is an `else` of the declaration line, never before it.
NOTICE_OUT=""
DECLARED_LINE="$(printf '%s\n' "$ANALYZER_OUT" | grep -m1 "^RICHOS_STOP_DECLARED	" || true)"
if [ -n "$DECLARED_LINE" ]; then
    # A DECLARED STOP IS SHOWN TO HIM, ALWAYS. Keyed on the case plus a hash of
    # the reason, so a DIFFERENT declaration speaks again and the same one
    # restated does not.
    D_CASE="$(printf '%s' "$DECLARED_LINE" | cut -f2)"
    D_WHY="$(printf '%s' "$DECLARED_LINE" | cut -f3 | tr -d '\000-\010\013\014\016-\037')"
    D_TOP="$(printf '%s' "$DECLARED_LINE" | cut -f4)"
    NOTICE_OUT="$(stop_notice_abnormal "declared:${D_CASE}:$(printf '%s' "$D_WHY" | cksum | tr -d ' ')" \
        "STOP DECLARED — a teammate (\"${D_TOP}\") finished this turn, nothing was started, and the turn was let through on a declared exception (${D_CASE}), not on a check. The reason given, which is DECLARED AND NOT VERIFIED: \"${D_WHY}\". $HOOK_TAG")"
else
    NOTICE_OUT="$(stop_notice_normal \
        "STATED-ACTIONS GATE — RUNNING AGAIN. Reports are being checked against the turn's own tool calls once more. $HOOK_TAG")"
fi

# THE RECORD ALERT (ARM 3). A reply that claims something was recorded, in a
# turn that wrote nothing to a durable record, tells the LEAD, and only the
# lead, to write it. Never a refusal, never a block, never shown to the CEO:
# additionalContext named for Stop reaches the model (the envelope
# notice-hook-staleness.sh uses), and systemMessage is deliberately absent.
# Both outputs leave in ONE JSON line.
ALERT_LINE="$(printf '%s\n' "$ANALYZER_OUT" | grep -m1 "^RICHOS_RECORD_ALERT	" | cut -f2- || true)"
if [ -n "$ALERT_LINE" ]; then
    ALERT_LINE="$ALERT_LINE" NOTICE_OUT="$NOTICE_OUT" python3 -c '
import json, os
out = {}
try:
    out = json.loads(os.environ.get("NOTICE_OUT", "").strip().splitlines()[-1])
except Exception:
    out = {}
out["hookSpecificOutput"] = {"hookEventName": "Stop", "additionalContext": os.environ["ALERT_LINE"]}
print(json.dumps(out))
' || printf '%s\n' "$NOTICE_OUT"
elif [ -n "$NOTICE_OUT" ]; then
    printf '%s\n' "$NOTICE_OUT"
fi

# Only 2 is a block. Anything else — including a crash in the analyzer — lets
# the turn end.
[ "$RC" = "2" ] && exit 2
exit 0
