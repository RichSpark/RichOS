#!/usr/bin/env bash
#
# notice-ceo-ruled-prose.sh — Stop hook. LOG-ONLY; never blocks.
#
# THE HALF OF THE FAILURE THAT NO GATE CAN BLOCK.
#
# The predicate, the corpus measurement and the fail-open argument live in
# scripts/lib/ceo-ruled.sh. Read that first; this file is the wiring.
#
# ===========================================================================
# WHY THIS EXISTS AT ALL, WHEN THERE IS ALREADY A BLOCKING GATE
# ===========================================================================
# guard-ceo-ruled-ask.sh refuses an AskUserQuestion whose subject the record
# has already ruled. It is the right gate on the right event, and on the night
# it was ordered IT WOULD HAVE CAUGHT NONE OF THE THREE FAILURES, because —
# verified against the session transcripts on this machine — ALL THREE WERE
# ASKED IN PROSE:
#
#   22:01  "3. What the customer installs themselves. ... Options: build your
#          Option D, ship the engine inside the app, or v1 is dogfood-only."
#   20:01  "is your mark one color, or does the swoosh get the gold?"
#   19:45  the seven splash screens, restated as approved.
#
# Not one of them was a tool call. There is no PreToolUse event for a sentence
# in a reply, and no hook can block one. A mechanism that shipped only the
# blocking gate would be green over exactly the failure it was built for —
# which is the shape this engine has now caught in itself several times, and
# the one the brief named: a check satisfied by a corpse.
#
# So the prose ask is covered HERE, at the only event that can see it, and the
# cost is stated rather than hidden: THE STOP EVENT FIRES AFTER HE HAS READ THE
# MESSAGE. This notice cannot prevent the question. What it can do is put the
# ruling in front of the orchestrator before the CEO answers, so the next thing
# he sees is "the record already says this, here it is" rather than a second
# question. That is worth having and it is not the same as prevention.
#
# ===========================================================================
# ONLY THE PARAGRAPHS THAT ASK ARE CHECKED — AND THE WINDOW WAS MEASURED
# ===========================================================================
# An assistant turn is long and cites the record constantly; running the
# predicate over a whole reply fires on 26% of asking turns, and a notice that
# fires on one turn in four is a notice nobody reads. Three windows were
# measured over 2,821 real assistant turns on this machine before one was
# chosen; the numbers are beside the extractor below.
#
# WHAT THIS DOES NOT CATCH, stated because a notice that overstated its reach
# would be worse than none: the logo question never used the word "logo" (it
# said "your mark"), so no title anchor could reach it; and the splash-screen
# failure was not a question at all — it was an assertion, and nothing that
# looks for an ask can look at an assertion. ONE of the three prose failures is
# caught here. That is the honest number.
#
# NEVER BLOCKS, ALWAYS EXIT 0. A Stop hook that can refuse a turn re-fires to
# the block cap and strands the session, and a notice people have to escape
# from is a notice people remove.

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
  hook: scripts/hooks/notice-ceo-ruled-prose.sh
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
    root_failure_banner "scripts/hooks/notice-ceo-ruled-prose.sh" >&2
    stop_notice_init "notice-ceo-ruled-prose.sh" "" "$INPUT"
    stop_notice_abnormal "root-failure" \
        "CEO-RULED WATCH IS OFF: this hook cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether a question you just asked in prose was already answered on his record."
    exit 0
fi

stop_notice_init "notice-ceo-ruled-prose.sh" "$ENTITY_ROOT" "$INPUT"
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
        _UE_MSG="$(unevaluated_sentence "notice-ceo-ruled-prose.sh" \
            "whether this turn re-asked him something his own record has already ruled on" \
            "$_UE_REASON" turn)"
        stop_notice_abnormal "payload-unreadable:$_UE_REASON" "$_UE_MSG"
        exit 0
    fi
fi

_CR_LIB="$SCRIPT_DIR/../lib/ceo-ruled.sh"
if [ ! -f "$_CR_LIB" ]; then
    stop_notice_abnormal "no-lib" \
        "CEO-RULED WATCH IS OFF: scripts/lib/ceo-ruled.sh is missing, so nothing was checked against the CEO's record. A clean turn and an absent checker must never look the same."
    exit 0
fi
# shellcheck source=../lib/ceo-ruled.sh
. "$_CR_LIB"

if ! cr_require; then
    stop_notice_abnormal "cannot-run:$(printf '%s' "$CR_BROKEN" | cksum | tr -d ' ')" \
        "CEO-RULED WATCH IS BROKEN, so nothing was checked: ${CR_BROKEN}. Do not read this silence as a record that rules nothing."
    exit 0
fi

RRC=0
cr_resolve "$ENTITY_ROOT" || RRC=$?
case "$RRC" in
    0) ;;
    1)
        # NOT-DECLARED is the ordinary answer in every repository with no CEO
        # record. stop_notice_normal is still called so a repository that
        # RECOVERS from a broken state gets the end of its story.
        stop_notice_normal ""
        exit 0 ;;
    *)
        stop_notice_abnormal "broken:$(printf '%s' "$CR_REASON" | cksum | tr -d ' ')" \
            "CEO-RULED WATCH IS BROKEN: this repository declares a CEO record and it cannot be read — ${CR_REASON}. Questions asked in prose are UNCHECKED, and a declared-but-unreadable record is not an empty one."
        exit 0 ;;
esac

# --- The paragraphs of this turn that ASK something -------------------------
QUESTIONS="$(python3 3<<< "$INPUT" -c '
import json, os, re, sys
try:
    d = json.load(os.fdopen(3))
except Exception:
    sys.exit(0)
if not isinstance(d, dict) or d.get("stop_hook_active"):
    sys.exit(0)
msg = str(d.get("last_assistant_message", "") or "")
# THE WINDOW IS THE PARAGRAPH THAT ASKS, and the three windows were MEASURED
# over 2,821 assistant turns on this machine before this one was chosen:
#
#   question SENTENCES only   4 fires / 193 asking turns (2.1%) — and it caught
#                             NONE of the three real failures. The Option D ask
#                             put its question in a heading and its evidence in
#                             the next sentence, so a sentence window read the
#                             question and not the thing that was already ruled.
#   the WHOLE message         51 / 193 (26.4%). One turn in four. A notice that
#                             fires on a quarter of the turns is a notice
#                             nobody reads, which is the same corpse in a
#                             louder coat.
#   the asking PARAGRAPH      9 / 193 (4.7%), and it catches the Option D
#                             failure. This one.
#
# An asking paragraph is one carrying a question mark, or one presenting
# **Options:** / **Decision:** — the two shapes this orchestrator actually uses
# to put a choice to the CEO in prose.
ASK = re.compile(r"\?|\*\*Options?:|\*\*Decision:", re.I)
paras = [p.strip() for p in re.split(r"\n\s*\n", msg) if p.strip()]
asking = [p for p in paras if ASK.search(p)]
if not asking:
    sys.exit(0)
sys.stdout.write("\n".join(asking)[:6000])
' 2>/dev/null || true)"

if [ -z "$QUESTIONS" ]; then
    stop_notice_normal ""
    exit 0
fi

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d = json.load(sys.stdin); print(str(d.get("session_id","") or ""))
except Exception:
    print("")' 2>/dev/null || true)"

cr_exempts "$ENTITY_ROOT" "$SESSION_ID"

QFILE="$(mktemp -t ceo-ruled-prose-q.XXXXXX)" || exit 0
OUTFILE="$(mktemp -t ceo-ruled-prose-out.XXXXXX)" || { rm -f "$QFILE"; exit 0; }
trap 'rm -f "$QFILE" "$OUTFILE"' EXIT
printf '%s\n' "$QUESTIONS" > "$QFILE"

if ! cr_check "$QFILE" > "$OUTFILE" 2>/dev/null; then
    stop_notice_abnormal "check-failed:$(printf '%s' "${CR_BROKEN:-?}" | cksum | tr -d ' ')" \
        "CEO-RULED WATCH COULD NOT RUN this turn: ${CR_BROKEN:-the predicate failed}. A question asked in prose went unchecked against his record."
    exit 0
fi

if [ "$(grep -m1 '^VERDICT	' "$OUTFILE" | cut -f2)" != "RULED" ]; then
    stop_notice_normal ""
    exit 0
fi

MSG="YOU ASKED HIM SOMETHING THE RECORD ALREADY RULES."
while IFS=$'\t' read -r K F1 F2 F3 F4 F5 F6 F7 F8; do
    case "$K" in
        RULED) MSG="$MSG  ${F2} — ${F3} (${F1}, line ${F8})." ;;
        QUOTE) MSG="$MSG  > ${F2}" ;;
    esac
done < "$OUTFILE"
MSG="$MSG  This was PROSE, so nothing could block it and he has already read it. Answer it yourself from the ruling above BEFORE he does, or declare which ruling does not cover it: $ENGINE_ROOT/scripts/ceo-ruled-exempt.sh ${SESSION_ID:-<session-id>} \"<cite>\" \"<why>\"."

stop_notice_abnormal "ruled:$(printf '%s' "$MSG" | cksum | tr -d ' ')" "$MSG"
exit 0
