#!/usr/bin/env bash
#
# guard-ceo-ruled-ask.sh — the PreToolUse gate on the AskUserQuestion tool.
#
# THREE CHECKS, ONE TOOL, ONE REGISTRATION — the shape verify-agent-prompt.sh
# already carries for Agent spawns:
#
#   0. THE DEAF-LEAD CHECK (2026-10-01, blocking) — while a teammate of this
#      session is live, the question goes as the turn's final message under
#      `QUESTION FOR YOU:` instead, so the lead can still be woken. FIRST at
#      runtime. Predicate and reasons: scripts/lib/blocking_ask.py.
#
#   1. THE PREMISE CHECK (2026-09-10, not blocking) — before a question reaches
#      him, ONE interruption per episode asking what it changes for him, with
#      what the question does and does not carry printed beside it. Re-issuing
#      passes. Argument and corpus: scripts/lib/premise-ask.sh and
#      scripts/hooks/premise-ask.corpus.md. It is second in this file and
#      FIRST at runtime, deliberately — see the block itself.
#   2. THE ALREADY-RULED CHECK (2026-09-02, blocking) — refuses to put a
#      question to the CEO whose subject the record has already ruled, and
#      names the ruling. Everything below documents this one.
#
# REFUSES TO PUT A QUESTION TO THE CEO WHOSE SUBJECT THE RECORD HAS ALREADY
# RULED — AND NAMES THE RULING.
#
# The predicate, the corpus measurement, the fail-open argument and the escape
# hatch all live in scripts/lib/ceo-ruled.sh and NOWHERE ELSE. Read that first;
# this file is the wiring and the refusal.
#
# ===========================================================================
# THE FAILURE
# ===========================================================================
# 2026-09-01. Three times in one evening the orchestrator asked the CEO
# something the record already answered, and each answer had been written down
# by the orchestrator itself hours or days earlier: what a customer installs
# (his standing instruction, open-items.md row 3.14, "automatically download
# and install whatever the user needs"); whether the logo carries one tone or
# two (approved, §21); and the splash screens, where a palette approval had
# been laundered into "seven approved splash screens" and repeated back to him
# as fact.
#
# His answer to the first was, in substance, that he had already discussed and
# answered the identical question more than once — put in the sharpest terms he
# had used all week. The wording is in the private record; the count is what
# matters here, and it was three in one evening.
#
# THE MECHANISM: the orchestrator WRITES to the record constantly and READS it
# almost never. Nothing stood between "this looks like a decision" and "ask
# him". This hook is that step, and it is the only one.
#
# ===========================================================================
# THIS IS THE SIBLING OF guard-ceo-ask-first.sh, ASKING THE OPPOSITE QUESTION
# ===========================================================================
# That guard refuses to DISPATCH work while a prepared decision has never been
# put to him. This one refuses to PUT a question to him he has already
# answered. Same record, same tokenizer (ceo-ruled.py loads ceo-asks.py's), one
# declaration of where his record lives (ca_resolve). Two failures, one surface.
#
# ===========================================================================
# WHY PreToolUse ON AskUserQuestion, AND WHAT THAT DOES NOT COVER
# ===========================================================================
# PreToolUse is the only event that fires BEFORE the CEO sees anything, which
# is the only place a question can be stopped rather than regretted.
#
# BUT — AND THIS IS THE HONEST HALF — TWO OF THE THREE FAILURES ABOVE WERE
# PROSE, NOT AskUserQuestion CALLS. Verified against the session transcripts on
# this machine: the Option D question and the logo question were sentences in a
# reply. There is no PreToolUse event for a sentence, and no hook can block
# one. scripts/hooks/notice-ceo-ruled-prose.sh reports those at the END of the
# turn, which is after he has read them, and that is stated rather than papered
# over: this gate hardens the structured ask, and the notice is the only reach
# the prose ask has at all.
#
# ===========================================================================
# THE ATTRIBUTION GATE
# ===========================================================================
# A call carrying an `agent_id` came from a WORKER. A teammate asking its own
# clarifying question is not the orchestrator re-litigating a ruling, and
# refusing it would spend this gate's whole false-positive budget on the case
# it was not built for. Worker calls pass, silently — the same line
# notice-ceo-asks.sh draws on the same tool.
#
# ===========================================================================
# FAIL OPEN, ALWAYS
# ===========================================================================
# A gate that can wedge the orchestrator's ability to ask the CEO ANYTHING is
# worse than the failure it prevents. Every plumbing failure lets the ask
# through and says so on both channels available. The only refusal is the one
# where the predicate ran, read a real record, and can name and quote a ruling.
#
# NOTE ON THE LOUD CHANNEL. The `systemMessage` field is proven for Stop hooks
# and only for Stop hooks (scripts/lib/stop-hook-notice.sh carries the
# measurement); whether it reaches the operator from PreToolUse is UNVERIFIED.
# So a broken state is announced on stderr AND as a systemMessage here, and
# notice-ceo-ruled-prose.sh repeats it at the end of every turn through the
# channel that IS measured.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds it.

set -eo pipefail

command -v python3 >/dev/null 2>&1 || exit 0

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
  hook: scripts/hooks/guard-ceo-ruled-ask.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

INPUT="$(cat)"

announce_broken() {
    printf '%s\n' "$1" >&2
    SYSMSG="$1" python3 -c '
import json, os
print(json.dumps({"systemMessage": os.environ.get("SYSMSG", "")}))
' 2>/dev/null || true
}

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_broken "CEO-RULED GATE IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether this question has already been answered on the record."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"

# ===========================================================================
# CHECK 0 — THE DEAF-LEAD CHECK (2026-10-01, blocking)
# ===========================================================================
# 2026-10-01: an AskUserQuestion held the lead's turn open for 37 minutes while
# the CEO was away. Everything that could have woken the lead (a merge result,
# a teammate's final report, the stall watcher, the 93% quota rule) waited in
# the queue behind it. While a teammate of this session is live, the question
# goes as the turn's FINAL MESSAGE under the fixed lead-in `QUESTION FOR YOU:`
# instead; notice-unanswered-question.sh repeats it under every later turn
# until he answers, so it never scrolls away. Allowed with no live teammate,
# when this turn was already refused at Stop, on unknown liveness (fail open),
# and after a declaration (blocking-ask-exempt.sh). The predicate and every
# reason: scripts/lib/blocking_ask.py. It runs FIRST: the premise and ruling
# checks below would examine a question that is about to be refused, and the
# same question then goes out as prose, where notice-ceo-ruled-prose.sh reads it.
_BA_LIB="$SCRIPT_DIR/../lib/blocking_ask.py"
if [ ! -f "$_BA_LIB" ]; then
    announce_broken "DEAF-LEAD CHECK IS OFF: scripts/lib/blocking_ask.py is missing at $_BA_LIB. An AskUserQuestion can hold the lead's turn open while teammates' results queue behind it."
else
    BA_OUT="$(printf '%s' "$INPUT" | python3 "$_BA_LIB" check --entity "$ENTITY_ROOT" --engine-root "$ENGINE_ROOT" 2>/dev/null || true)"
    BA_VERDICT="$(printf '%s' "$BA_OUT" | cut -f1)"
    # No verdict at all is a crashed checker, not an ALLOW: still fail open, but
    # say so instead of passing for a check that ran (hunt part 3 v3, finding 28).
    if [ "$BA_VERDICT" != "REFUSE" ] && [ "$BA_VERDICT" != "ALLOW" ]; then
        announce_broken "DEAF-LEAD CHECK DID NOT RUN: scripts/lib/blocking_ask.py check gave no verdict, so this AskUserQuestion was not checked against live teammates."
    fi
    if [ "$BA_VERDICT" = "REFUSE" ]; then
        BA_LIVE="$(printf '%s' "$BA_OUT" | cut -f2)"
        BA_SESSION="$(printf '%s' "$BA_OUT" | cut -f3)"
        {
            echo "=== NOT AS AskUserQuestion WHILE TEAMMATES WORK: MAKE IT YOUR TURN'S FINAL MESSAGE ==="
            echo ""
            echo "  2026-10-01: an AskUserQuestion held the lead's turn open for 37 minutes while"
            echo "  the CEO was away. A merge result and a teammate's final report waited behind"
            echo "  it, and nothing could wake the lead: not a teammate finishing, not the stall"
            echo "  watcher, not the 93% quota rule."
            echo ""
            echo "  Live teammates of this session: $BA_LIVE"
            echo ""
            echo "  INSTEAD: end this turn with the question as your FINAL MESSAGE, starting with"
            echo "  this lead-in on its own line, then the question and its options:"
            echo ""
            echo "      QUESTION FOR YOU:"
            echo ""
            echo "  He reads the same words in the terminal and his answer arrives as his next"
            echo "  prompt. Meanwhile finishing teammates and the stall watcher can still wake"
            echo "  you, and notice-unanswered-question.sh repeats the question under every later"
            echo "  turn until he answers, so it does not scroll away."
            echo ""
            echo "  Allowed with nothing to declare once this turn has been refused at Stop (a"
            echo "  blocking Stop gate would keep the question from being your final message)."
            echo "  IF IT MUST BLOCK, declare why, for this session only:"
            printf '    %s/scripts/blocking-ask-exempt.sh %s "<why this question has to block>"\n' \
                "$ENGINE_ROOT" "${BA_SESSION:-<session-id>}"
            echo "(hook: scripts/hooks/guard-ceo-ruled-ask.sh, deaf-lead check)"
        } >&2
        exit 2
    fi
fi

# ===========================================================================
# CHECK 2 OF 2 — THE PREMISE CHECK (2026-09-10)
# ===========================================================================
# WHY IT LIVES IN THIS FILE. It is a second, different check on the same tool,
# which is the shape verify-agent-prompt.sh already carries for Agent spawns —
# seven checks, one gate, one registration. It is also the only shape that is
# LIVE THE MOMENT THIS BRANCH MERGES: what a session snapshots is WHICH hooks
# are registered, and this one already is, so its body is read at every
# invocation. A new hook file would have needed a new registration, an entry in
# contract-integrity-probe.sh's inventory, and a fresh session before it
# protected anybody.
#
# WHAT IT REFUSES: nothing, ever, permanently. It interrupts a question at most
# once per episode and prints what that question does and does not carry. The
# corpus behind that decision — 85 real questions, four measured predicates,
# and the two of 2026-09-10 that sit in the TOP DECILE for premise richness —
# is scripts/hooks/premise-ask.corpus.md, and the argument is
# scripts/lib/premise-ask.sh. Read either before changing this.
#
# IT RUNS BEFORE THE ALREADY-RULED CHECK, deliberately: that check needs the
# CEO's rulings record to resolve and stands down when it cannot, and the
# premise question — why is this worth his time — is worth asking even in a
# repository whose rulings file is missing or broken.
_PA_LIB="$SCRIPT_DIR/../lib/premise-ask.sh"
if [ ! -f "$_PA_LIB" ]; then
    announce_broken "PREMISE CHECK IS OFF: scripts/lib/premise-ask.sh is missing at $_PA_LIB. Questions to the CEO are going through unexamined — a clean run and an absent check must never look the same."
else
    # shellcheck source=../lib/premise-ask.sh
    . "$_PA_LIB"

    PA_META="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    if d.get("tool_name") not in (None, "", "AskUserQuestion", "mcp__richos_questions__ask"):
        raise ValueError("not an AskUserQuestion call")
    print("OK\t%s\t%s" % (str(d.get("session_id", "") or ""),
                          str(d.get("agent_id", "") or "")))
except Exception:
    print("PARSEFAIL\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t')"

    PA_STATUS="$(printf '%s' "$PA_META" | cut -f1)"
    PA_SESSION="$(printf '%s' "$PA_META" | cut -f2)"
    PA_AGENT="$(printf '%s' "$PA_META" | cut -f3)"

    # A worker's clarifying question is not the orchestrator putting a decision
    # to the CEO. An unreadable payload is not checked and says so — with no
    # text there is nothing to report on, and refusing on that would block a
    # question nobody could then permit.
    if [ "$PA_STATUS" = "PARSEFAIL" ]; then
        announce_broken "PREMISE CHECK: could not parse this AskUserQuestion call, so it was not examined. This ONE question is unchecked."
    elif [ -n "$PA_AGENT" ]; then
        :
    elif ! pa_require; then
        announce_broken "PREMISE CHECK IS OFF: $PA_BROKEN. Questions to the CEO are going through unexamined."
    else
        PA_GOV=0
        pa_governed "$ENTITY_ROOT" || PA_GOV=$?
        if [ "$PA_GOV" -eq 2 ]; then
            announce_broken "PREMISE CHECK IS OFF: ${PA_REASON}. A declared-but-unreadable CEO record is not an absent one."
        elif [ "$PA_GOV" -eq 0 ]; then
            PA_QLIST="$(pa_questions_of "$INPUT")"
            if [ -n "$PA_QLIST" ]; then
                PA_QF="$(mktemp -t premise-ask-q.XXXXXX)" || PA_QF=""
                PA_WF="$(mktemp -t premise-ask-w.XXXXXX)" || PA_WF=""
                PA_OUT="$(mktemp -t premise-ask-out.XXXXXX)" || PA_OUT=""
                PA_REPORT="$(mktemp -t premise-ask-report.XXXXXX)" || PA_REPORT=""
                if [ -n "$PA_QF" ] && [ -n "$PA_WF" ] && [ -n "$PA_OUT" ] && [ -n "$PA_REPORT" ]; then
                    trap 'rm -f "$PA_QF" "$PA_WF" "$PA_OUT" "$PA_REPORT"' EXIT
                    PA_ANY_CHECK=0
                    PA_CODES=""
                    PA_HEADER=""
                    PA_ALLTEXT=""
                    PA_DECLARED=""
                    # HIS PREPARED QUESTION GOES THROUGH (orchestration reliability R3: "Asking an
                    # actually needed question must not be intercepted by a contradictory premise
                    # check; both paths use the same current facts"). 2026-10-23 19:46Z: the
                    # CEO-TODO question the dispatch gate demanded was itself intercepted here. A
                    # question that matches a prepared item on his list (ca_match, the matcher
                    # that records the ask) was premise-checked when it was prepared.
                    PA_PREPARED_ITEMS=""
                    if [ -f "$SCRIPT_DIR/../lib/ceo-asks.sh" ]; then
                        . "$SCRIPT_DIR/../lib/ceo-asks.sh"
                        if ca_require && ca_resolve "$ENTITY_ROOT"; then
                            PA_PREPARED_ITEMS="$(mktemp -t premise-ask-items.XXXXXX)" \
                                && ca_items_json "$PA_PREPARED_ITEMS" || PA_PREPARED_ITEMS=""
                        fi
                    fi
                    while IFS=$'\t' read -r PA_IDX PA_QTEXT PA_WTEXT; do
                        [ -n "${PA_WTEXT:-}" ] || continue
                        printf '%s' "$PA_QTEXT" | tr '\001' '\n' > "$PA_QF"
                        printf '%s' "$PA_WTEXT" | tr '\001' '\n' > "$PA_WF"
                        [ -n "$PA_HEADER" ] || PA_HEADER="$(head -n1 "$PA_QF" 2>/dev/null || true)"
                        PA_ALLTEXT="$PA_ALLTEXT$(cat "$PA_WF")"
                        if [ -n "$PA_PREPARED_ITEMS" ] \
                                && [ "$(ca_match "$PA_WF" "$PA_PREPARED_ITEMS" 2>/dev/null | cut -f1)" = "MATCH" ]; then
                            continue
                        fi
                        if ! pa_check "$PA_QF" "$PA_WF" > "$PA_OUT" 2>/dev/null; then
                            announce_broken "PREMISE CHECK IS DEGRADED: ${PA_BROKEN:-the predicate could not run}. This question was not examined."
                            continue
                        fi
                        if grep -q '^MARKER	' "$PA_OUT"; then
                            PA_DECLARED="$(grep -m1 '^MARKER	' "$PA_OUT" | cut -f3)"
                            continue
                        fi
                        [ "$(grep -m1 '^VERDICT	' "$PA_OUT" | cut -f2)" = "CHECK" ] || continue
                        PA_ANY_CHECK=1
                        {
                            printf '  QUESTION %s\n' "$((PA_IDX + 1))"
                            while IFS=$'\t' read -r PA_K PA_F1 PA_F2; do
                                [ "$PA_K" = "FINDING" ] || continue
                                PA_CODES="$PA_CODES$PA_F1,"
                                printf '    %s\n      %s\n' "$PA_F1" "$PA_F2"
                            done < "$PA_OUT"
                            printf '\n'
                        } >> "$PA_REPORT"
                    done <<PA_QLIST_EOF
$PA_QLIST
PA_QLIST_EOF

                    if [ -n "$PA_DECLARED" ]; then
                        pa_record "$ENTITY_ROOT" "$PA_SESSION" "DECLARED" "$PA_HEADER" "premise-unverified" "$PA_ALLTEXT"
                    elif [ "$PA_ANY_CHECK" -eq 1 ]; then
                        if pa_standdown_active "$ENTITY_ROOT" "$PA_SESSION"; then
                            pa_record "$ENTITY_ROOT" "$PA_SESSION" "STOOD-DOWN" "$PA_HEADER" "$PA_CODES" "$PA_ALLTEXT"
                        else
                            pa_standdown_arm "$ENTITY_ROOT" "$PA_SESSION"
                            pa_record "$ENTITY_ROOT" "$PA_SESSION" "CHECKED" "$PA_HEADER" "$PA_CODES" "$PA_ALLTEXT"
                            {
                                echo "=== BEFORE HE READS THIS: WHAT DOES IT CHANGE FOR HIM? ==="
                                echo ""
                                echo "  2026-09-10. Two questions reached him that could not affect him."
                                echo "  One asked what to promise about a capability that turned out to be"
                                echo "  a safety net for my own misjudgment. The other rested on work"
                                echo "  surviving him QUITTING MID-FLIGHT — and when he asked how often he"
                                echo "  had ever done that, nobody had looked. The log said ZERO, across"
                                echo "  its whole history. A probe had constructed the scenario in a"
                                echo "  sandbox and I had put it to him as a blocker."
                                echo ""
                                echo "  THIS IS NOT A REFUSAL AND NOTHING IS BLOCKED. Re-issue the call —"
                                echo "  changed or unchanged — and it goes straight through, this time and"
                                echo "  for the next ${PA_STANDDOWN_SECONDS}s. The check exists because no"
                                echo "  predicate can answer the question below; it was measured against"
                                echo "  all 85 real questions on this machine and both of that day's"
                                echo "  failures read BETTER than average (premise-ask.corpus.md)."
                                echo ""
                                echo "  ANSWER THESE THREE, THEN SEND IT:"
                                echo "    1. What is TRUE that makes this worth his time — and is it in"
                                echo "       the question's own text, where he reads it?"
                                echo "    2. If that fact is about something HAPPENING, what measured that"
                                echo "       it happens? Run the command. A probe in a sandbox is not"
                                echo "       evidence that it occurs."
                                echo "    3. Does his answer change what gets BUILT, PROMISED or SPENT?"
                                echo "       If both options end in the same work, it is not his."
                                echo ""
                                if [ -s "$PA_REPORT" ]; then
                                    echo "  WHAT THIS PARTICULAR QUESTION CARRIES:"
                                    echo ""
                                    cat "$PA_REPORT"
                                fi
                                echo "  IF THE PREMISE IS GENUINELY UNMEASURED, SAY SO IN THE QUESTION"
                                echo "  ITSELF, on its own line — the marked form is the rule, not an"
                                echo "  evasion of it, and it skips this check entirely:"
                                echo ""
                                echo "    premise-unverified: <what is unknown and what would settle it>"
                                echo ""
                                echo "  A bare marker exempts nothing. Every question, checked or not, is"
                                echo "  logged to .claude/state/$PA_LEDGER_NAME with whether the re-issue"
                                echo "  changed anything — so if this check ever becomes a reflex, the"
                                echo "  numbers will say so."
                                echo "(hook: scripts/hooks/guard-ceo-ruled-ask.sh, premise check)"
                            } >&2
                            exit 2
                        fi
                    else
                        pa_record "$ENTITY_ROOT" "$PA_SESSION" "UNGATED" "$PA_HEADER" "" "$PA_ALLTEXT"
                    fi
                    rm -f "$PA_QF" "$PA_WF" "$PA_OUT" "$PA_REPORT"
                    trap - EXIT
                fi
            fi
        fi
    fi
fi

_CR_LIB="$SCRIPT_DIR/../lib/ceo-ruled.sh"
if [ ! -f "$_CR_LIB" ]; then
    announce_broken "CEO-RULED GATE IS OFF: scripts/lib/ceo-ruled.sh is missing at $_CR_LIB, so its entire predicate is absent. Questions to the CEO are UNCHECKED against his record — a clean run and an absent gate must never look the same."
    exit 0
fi
# shellcheck source=../lib/ceo-ruled.sh
. "$_CR_LIB"

if ! cr_require; then
    announce_broken "CEO-RULED GATE IS OFF: $CR_BROKEN. Questions to the CEO are UNCHECKED against his record."
    exit 0
fi

RRC=0
cr_resolve "$ENTITY_ROOT" || RRC=$?
case "$RRC" in
    0) ;;
    1) exit 0 ;;    # NOT-DECLARED: no CEO record here, stand down silently
    *)
        announce_broken "CEO-RULED GATE IS OFF: this repository declares a CEO record and it cannot be read — ${CR_REASON}. Questions are UNCHECKED, and a declared-but-unreadable record is not an empty one."
        exit 0 ;;
esac

# --- Parse the call --------------------------------------------------------
META="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    if d.get("tool_name") not in (None, "", "AskUserQuestion", "mcp__richos_questions__ask"):
        raise ValueError("not an AskUserQuestion call")
    print("OK\t%s\t%s" % (str(d.get("session_id", "") or ""),
                          str(d.get("agent_id", "") or "")))
except Exception:
    print("PARSEFAIL\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t')"

STATUS="$(printf '%s' "$META" | cut -f1)"
SESSION_ID="$(printf '%s' "$META" | cut -f2)"
AGENT_ID="$(printf '%s' "$META" | cut -f3)"

# An unparseable payload is NOT blocked, and the choice is deliberate rather
# than left as an inconsistency to be discovered. guard-worktree-isolation.sh
# fails CLOSED on an unparseable spawn because an unparseable spawn could BE
# the violation it exists to catch. Here the subject of the check is the
# question TEXT: with no text there is nothing to compare against the record,
# and refusing on that would block a question nobody could then permit.
if [ "$STATUS" = "PARSEFAIL" ]; then
    announce_broken "CEO-RULED GATE: could not parse this AskUserQuestion call, so it was not checked against the CEO's record. This ONE question is ungated."
    exit 0
fi

# A worker's own clarifying question is not the orchestrator re-asking a ruling.
[ -z "$AGENT_ID" ] || exit 0

QLIST="$(cr_questions_of "$INPUT")"
[ -n "$QLIST" ] || exit 0

cr_exempts "$ENTITY_ROOT" "$SESSION_ID"

QFILE="$(mktemp -t ceo-ruled-q.XXXXXX)" || exit 0
OUTFILE="$(mktemp -t ceo-ruled-out.XXXXXX)" || { rm -f "$QFILE"; exit 0; }
REPORT="$(mktemp -t ceo-ruled-report.XXXXXX)" || { rm -f "$QFILE" "$OUTFILE"; exit 0; }
trap 'rm -f "$QFILE" "$OUTFILE" "$REPORT"' EXIT

BLOCKED=0
BROKEN_NOTE=""

while IFS=$'\t' read -r QIDX QTEXT; do
    [ -n "${QTEXT:-}" ] || continue
    printf '%s' "$QTEXT" | tr '\001' '\n' > "$QFILE"
    if ! cr_check "$QFILE" > "$OUTFILE" 2>/dev/null; then
        BROKEN_NOTE="${CR_BROKEN:-the predicate could not run}"
        continue
    fi
    if grep -q '^PROBLEM	' "$OUTFILE"; then
        BROKEN_NOTE="a declared record could not be parsed: $(grep -m1 '^PROBLEM	' "$OUTFILE" | cut -f2-)"
    fi
    case "$(grep -m1 '^VERDICT	' "$OUTFILE" | cut -f2)" in
        RULED)
            BLOCKED=1
            {
                printf '  QUESTION %s\n' "$((QIDX + 1))"
                # NAMED AND QUOTED, NEVER COUNTED. "already decided" sends the
                # reader hunting through 1,400 lines; a section number, a title
                # and the CEO's own sentence is something he can answer FROM.
                #
                # CAPPED AT MAX_CITED, and the cap is the same instinct as the
                # rest of this gate: a refusal that dumps a dozen rulings is a
                # wall, and a wall gets waived. The predicate emits all of them
                # — the ledger and the tests see everything — the REFUSAL shows
                # the most specific few. Three was the most any real question
                # produced in the measured corpus.
                MAX_CITED=4
                SHOWN=0
                while IFS=$'\t' read -r K F1 F2 F3 F4 F5 F6 F7 F8; do
                    case "$K" in
                        RULED)
                            SHOWN=$((SHOWN + 1))
                            [ "$SHOWN" -le "$MAX_CITED" ] || continue
                            printf '    %s  %s\n' "$F2" "$F3"
                            printf '      in %s, line %s   (matched: %s "%s")\n' \
                                "$F1" "$F8" "$F4" "$F5"
                            ;;
                        QUOTE)
                            [ "$SHOWN" -le "$MAX_CITED" ] || continue
                            printf '      > %s\n' "$F2"
                            ;;
                    esac
                done < "$OUTFILE"
                if [ "$SHOWN" -gt "$MAX_CITED" ]; then
                    printf '    (+%d more ruling(s) matched — the record is dense here)\n' \
                        "$((SHOWN - MAX_CITED))"
                fi
                printf '\n'
            } >> "$REPORT"
            ;;
    esac
done <<QLIST_EOF
$QLIST
QLIST_EOF

if [ -n "$BROKEN_NOTE" ] && [ "$BLOCKED" -eq 0 ]; then
    announce_broken "CEO-RULED GATE IS DEGRADED: $BROKEN_NOTE. This question was not fully checked against the CEO's record."
fi

[ "$BLOCKED" -eq 1 ] || exit 0

# --- REFUSE ----------------------------------------------------------------
{
    echo "=== THE RECORD HAS ALREADY RULED ON THIS — REFUSING TO ASK HIM AGAIN ==="
    echo ""
    echo "  On 2026-09-01 three questions went to the CEO that the record"
    echo "  already answered, each one written down by this session's own"
    echo "  orchestrator hours or days earlier. He had answered every one of"
    echo "  them before, and said so — the third time was not a question, it"
    echo "  was the same question."
    echo ""
    echo "  WHAT THE RECORD SAYS:"
    echo ""
    cat "$REPORT"
    echo "  ANSWER HIM FROM THIS, do not ask. Open the file at the line above"
    echo "  and read the ruling in full before deciding it does not apply."
    echo ""
    echo "  IF A RULING GENUINELY DOES NOT COVER THIS QUESTION, say which one"
    echo "  and why — on the record, where a reviewer sees it:"
    echo ""
    printf '    %s/scripts/ceo-ruled-exempt.sh %s "<cite>" "<why it does not cover this>"\n' \
        "$ENGINE_ROOT" "${SESSION_ID:-<session-id>}"
    echo ""
    echo "  A BARE MARKER EXEMPTS NOTHING — the reason is required and is"
    echo "  length-checked. Half the point is that you looked at the ruling."
    echo "  It is logged to .claude/state/$CR_EXEMPT_LOG_NAME, per session,"
    echo "  per citation, and it does not carry into the next session."
    echo "(hook: scripts/hooks/guard-ceo-ruled-ask.sh)"
} >&2
exit 2
