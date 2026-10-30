#!/usr/bin/env bash
#
# left-off-report.sh — WHEN HE COMES BACK, THE THING HE LEFT ON IS ALREADY
#                      IN FRONT OF THE ASSISTANT.
#
# Registered SessionStart AND UserPromptSubmit. The predicate, every
# measurement behind it, and the reasoning are in scripts/lib/left-off.py —
# read that first. This file is the wiring, the channel, and the two decisions
# that belong to a hook rather than to a library.
#
# ===========================================================================
# WHAT THIS IS FOR
# ===========================================================================
# 2026-09-15. Nine-hour break. He came back and asked "TLDR, plain English",
# and was answered about a guard-deletion step, because that is what the lead
# was holding. SIXTEEN MESSAGES AND THIRTY-NINE MINUTES later he had the one
# line he wanted, after re-pasting his own messages from the night before:
#
#     "HOW MANY MORE FUCKING MESSAGES FROM LAST NIGHT DO I NEED TO COPY AND
#      PASTE HERE???? HOW MANY IDENTICAL FUCKING QUESTIONS FROM LAST NIGHT DO
#      I FUCKING NEED TO ASK AGAIN HERE"
#
# Lifecycle failure types 60 and 61. The answer to "how many" is zero, and it
# is zero because a mechanism puts it there rather than because somebody
# remembers to look.
#
# ===========================================================================
# IT IS NOT A GUARD — IT REFUSES NOTHING
# ===========================================================================
# The standing rule since 2026-09-14 is that a new guard costs a deleted one,
# or a written reason the error cannot be designed out. This does not qualify
# and does not need to: it has no refusal, no exit code anybody clears, no
# acknowledgement line and no way to block a turn. Every path exits 0. It is an
# ANSWER delivered before the question is asked.
#
# WHY A NEW FILE RATHER THAN A LINE IN AN EXISTING ONE, since the brief asked
# for that preference and it is the right preference. Seven hooks run at
# SessionStart and ONE runs at UserPromptSubmit. The requirement is that this
# fires on a GAP, and the session this exists for ran 33 hours across three of
# his sleep cycles — a SessionStart-only report would have fired once, on the
# 13th, and been silent on the morning it was needed. So it has to be on
# UserPromptSubmit, where the only resident is commit-ceo-inputs.sh: an INGRESS
# that commits files he hands over, whose whole body is git side effects and
# refusal gates, and which is registered on one event only. Extending it would
# have meant a second unrelated predicate inside a hook that must never block,
# registered on an event it was never written for. What IS reused, and it is
# the substantive half: repository discovery and the integration-branch
# question come from scripts/lib/unlanded-branches.py and workspaces.py
# unchanged, so there is no second answer to "has this landed" anywhere in the
# engine.
#
# ===========================================================================
# THE CHANNEL — BOTH AUDIENCES, AND additionalContext IS THE LOAD-BEARING ONE
# ===========================================================================
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back —
# the table carried verbatim in every hook of this engine that resolves a root:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# THE MODEL IS THE AUDIENCE THAT HAS TO ACT, so the report goes on
# `additionalContext`, which is measured to reach it on both of this hook's
# events. `systemMessage` reaches the PERSON and nothing else — and that is
# exactly how a staleness notice named a dead guard all day while nobody acted
# on it. So the person gets ONE LINE and the model gets the report; a hook that
# put the report on `systemMessage` would be rebuilding the defect.
#
# hookEventName is spelled from --event and must match the event the host
# fired, because a mismatch makes the host DROP the whole object. That is also
# why the event is an ARGUMENT rather than something read from the payload:
# see the stdin note below.
#
# ===========================================================================
# STDIN — READ ON ONE EVENT, NEVER ON THE OTHER, AND THE ARGUMENT IS WHY
# ===========================================================================
# engine-status.sh measured the hazard: a SessionStart hook is also runnable by
# hand, and in that case stdin is an inherited pipe nobody closes, so an
# unconditional `cat` hangs — 92 seconds and counting inside the contract
# probe before it was reverted, and `[ ! -t 0 ]` does not help because an
# inherited pipe is not a TTY.
#
# So the event comes from the REGISTRATION, as `--event`, and:
#   SessionStart      never touches stdin. It does not need to: the transcript
#                     is found from CLAUDE_PROJECT_DIR, which the probe
#                     measured present and correct in a plugin-loaded hook at
#                     SessionStart, and there is no arriving prompt to exclude.
#   UserPromptSubmit  reads the payload with `cat`, exactly as
#                     commit-ceo-inputs.sh has on this event since it shipped.
#                     It needs `prompt` — see left-off.py on why the arriving
#                     message is excluded by identity rather than by timing.
#
# ===========================================================================
# IT SPEAKS ONCE PER GAP, AND THEN ONCE MORE, TWICE
# ===========================================================================
# The full report is emitted on the first message after a gap. The next two
# messages get ONE LINE repeating his pre-gap question verbatim, and after that
# it is silent until the next gap.
#
# The two are not decoration. Type 60's aggravating detail is that retrieval
# ALREADY WORKED — the lead printed his 21:39 message on screen and answered
# something else anyway — and he then sent fifteen more messages. A single
# emission that gets reasoned past has no second chance; two one-line anchors
# cost 240 characters between them and are the cheapest possible insurance
# against exactly the failure that is recorded.
#
# ===========================================================================
# COST
# ===========================================================================
# Measured on the 18 MB / 10031-line transcript of the session it was written
# in, with three git repositories in scope (femcboost, richos, richos-hq):
# 1.86 s wall for one whole run of THIS FILE producing a full report, of which
# 1.45 s is the analyzer (three runs: 1.45 / 1.48 / 1.44). When there is no gap
# the analyzer takes 0.18 s and stops before any git call at all, and that is
# the common case -- it is what almost every message pays. The numbers and the
# commands that produce them are in docs/verification/left-off-2026-09-15.md.
#
# Stand down with CHECK_LEFT_OFF=0 in orchestration.config — announced, never
# silent.
#
# Verify a live install:  scripts/hooks/left-off-report.sh --self-test
# Read it by hand:        scripts/hooks/left-off-report.sh --report
#
# Exit codes: always 0. This hook never refuses anything.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

HOOK_TAG="(hook: scripts/hooks/left-off-report.sh)"

if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/left-off.test.sh"
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
  hook: scripts/hooks/left-off-report.sh
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

# --- ARGUMENTS -------------------------------------------------------------
EVENT="UserPromptSubmit"
REPORT_ONLY=0
ARG_TRANSCRIPT=""
ARG_NOW=""
ARG_SESSION=""
while [ $# -gt 0 ]; do
    case "$1" in
        --event)      EVENT="${2:-}"; shift 2 ;;
        --report)     REPORT_ONLY=1; shift ;;
        --transcript) ARG_TRANSCRIPT="${2:-}"; shift 2 ;;
        --now)        ARG_NOW="${2:-}"; shift 2 ;;
        --session)    ARG_SESSION="${2:-}"; shift 2 ;;
        *)            shift ;;
    esac
done

# --- OUTPUT ----------------------------------------------------------------
# Builtins only, for the reason commit-ceo-inputs.sh gives: one of the states
# this file must be able to announce is "python3 is not on PATH", and an
# emitter that needed python3 could not report it.
_esc() {
    local s="${1:-}"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\r'/ }"
    s="${s//$'\t'/    }"
    printf '%s' "$s"
}

# _say <operator-one-line> <model-context>
_say() {
    if [ "$REPORT_ONLY" = "1" ]; then
        [ -n "${2:-}" ] && printf '%s\n' "$2"
        exit 0
    fi
    printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"%s","additionalContext":"%s"}}\n' \
        "$(_esc "${1:-}")" "$EVENT" "$(_esc "${2:-}")"
    exit 0
}

_silent() {
    [ "$REPORT_ONLY" = "1" ] && exit 0
    exit 0
}

# --- PAYLOAD ---------------------------------------------------------------
# UserPromptSubmit only. See the stdin note in the header — SessionStart never
# touches stdin, and that is a measured hazard rather than a preference.
INPUT=""
if [ "$REPORT_ONLY" != "1" ] && [ "$EVENT" = "UserPromptSubmit" ]; then
    INPUT="$(cat)"
fi

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine. engine-status.sh announces the
    # whole stand-down at session start; repeating it here would be noise.
    _silent
else
    _say "WHERE-HE-LEFT-OFF IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). $HOOK_TAG" \
         "THE RETURN REPORT IS NOT RUNNING: left-off-report.sh could not resolve its repository (${RICHOS_ROOT_REASON:-root resolution failed}). If he has just come back from a break, NOTHING has told you what he left on — read his last message out of the transcript yourself before answering. $HOOK_TAG"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_LEFT_OFF:=1}"
: "${LEFT_OFF_GAP_MINUTES:=120}"

STATE_DIR="$ENTITY_ROOT/.claude/state"

# --- SESSION AND TRANSCRIPT ------------------------------------------------
SESSION_ID="$ARG_SESSION"
TRANSCRIPT="$ARG_TRANSCRIPT"
PROMPT_FILE=""
TMPDIR_SELF=""
if [ -n "$INPUT" ]; then
    SESSION_ID="$(printf '%s' "$INPUT" | tr '\n' ' ' \
        | grep -o '"session_id"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -1 | sed 's/.*"\([^"]*\)"$/\1/' 2>/dev/null || true)"
    TRANSCRIPT="$(printf '%s' "$INPUT" | tr '\n' ' ' \
        | grep -o '"transcript_path"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -1 | sed 's/.*"\([^"]*\)"$/\1/' 2>/dev/null || true)"
fi
SESSION_SHORT="$(printf '%s' "$SESSION_ID" | tr -cd '[:alnum:]-' | cut -c1-8)"
[ -n "$SESSION_SHORT" ] || SESSION_SHORT="nosession"

if [ "$CHECK_LEFT_OFF" = "0" ]; then
    # Never a silent permission — an opt-out nobody can see decays into a
    # rumour, and this hook exists because a lookup nobody ran was assumed to
    # have been run.
    _say "WHERE-HE-LEFT-OFF — STOOD DOWN by CHECK_LEFT_OFF=0 in $CONFIG. $HOOK_TAG" \
         "THE RETURN REPORT IS STOOD DOWN (CHECK_LEFT_OFF=0 in $CONFIG). If he has just come back from a break, nothing is telling you what he left on. $HOOK_TAG"
fi

if ! command -v python3 >/dev/null 2>&1; then
    _say "WHERE-HE-LEFT-OFF — NOT RUNNING: python3 is not on PATH. $HOOK_TAG" \
         "THE RETURN REPORT IS NOT RUNNING: python3 is not on PATH, so nothing has read his pre-gap message. $HOOK_TAG"
fi

ANALYZER="$ENGINE_ROOT/scripts/lib/left-off.py"
if [ ! -f "$ANALYZER" ]; then
    _say "WHERE-HE-LEFT-OFF — NOT RUNNING: the analyzer is missing at $ANALYZER. $HOOK_TAG" \
         "THE RETURN REPORT IS NOT RUNNING: its analyzer is missing at $ANALYZER. $HOOK_TAG"
fi

# A transcript the payload NAMED and that is not there is the absence of a
# CHECK, and it is announced. Silently falling through to discovery would make
# a broken install look exactly like a session with no gap in it, which is the
# one thing every notice in this engine is forbidden to do.
if [ -n "$TRANSCRIPT" ] && [ ! -f "$TRANSCRIPT" ]; then
    _say "WHERE-HE-LEFT-OFF — the transcript named in the payload is not there: $TRANSCRIPT. $HOOK_TAG" \
         "THE RETURN REPORT COULD NOT RUN: the payload named a transcript that does not exist ($TRANSCRIPT), so NOTHING has read his pre-gap message. This is the absence of a check, not the absence of a gap. If he has been away, read his last message out of the transcript yourself before answering. $HOOK_TAG"
fi

# The transcript, when the payload did not name one: the newest .jsonl in this
# project's own directory. At UserPromptSubmit that is this session by
# construction; at SessionStart it is the session he was last in, which is the
# one he left off in and exactly what is wanted.
if [ -z "$TRANSCRIPT" ]; then
    PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$ENTITY_ROOT}"
    SLUG="$(printf '%s' "$PROJECT_DIR" | sed 's|[/.]|-|g')"
    CANDIDATE_DIR="$HOME/.claude/projects/$SLUG"
    if [ -d "$CANDIDATE_DIR" ]; then
        TRANSCRIPT="$(ls -t "$CANDIDATE_DIR"/*.jsonl 2>/dev/null | head -1 || true)"
    fi
fi
[ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ] || _silent

# The arriving prompt, written to a file rather than passed as an argument: it
# is his text, it can be any length and contain anything, and an argv round
# trip through two shells is where quoting bugs live.
if [ -n "$INPUT" ]; then
    TMPDIR_SELF="$(mktemp -d -t left-off.XXXXXX 2>/dev/null || true)"
    if [ -n "$TMPDIR_SELF" ]; then
        PROMPT_FILE="$TMPDIR_SELF/prompt.txt"
        printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
sys.stdout.write(str(d.get("prompt", "") or ""))
' > "$PROMPT_FILE" 2>/dev/null || : > "$PROMPT_FILE"
    fi
fi
# `return 0` is not tidiness. A bash EXIT trap whose last command fails takes
# the SHELL's exit status with it, so the bare `[ -n "$X" ] && rm` returned 1
# from a hook whose whole contract is "always 0" -- measured, on the path where
# there was no temp directory to remove.
_cleanup() { [ -n "${TMPDIR_SELF:-}" ] && rm -rf "$TMPDIR_SELF"; return 0; }
trap _cleanup EXIT

# --- RUN -------------------------------------------------------------------
set +e
OUT="$(python3 "$ANALYZER" \
    --transcript "$TRANSCRIPT" \
    ${ARG_NOW:+--now "$ARG_NOW"} \
    ${PROMPT_FILE:+--prompt-file "$PROMPT_FILE"} \
    --event "$EVENT" \
    --gap-minutes "$LEFT_OFF_GAP_MINUTES" \
    --entity-root "$ENTITY_ROOT" \
    --session "$SESSION_ID" \
    --format hook 2>/dev/null)"
RC=$?
set -e
KEY="$(printf '%s\n' "$OUT" | sed -n '1p')"
ANCHOR="$(printf '%s\n' "$OUT" | sed -n '2p')"
REPORT="$(printf '%s\n' "$OUT" | tail -n +3)"

# rc 1 is "he has not been away" — the ordinary case, and silence is correct.
# rc 2 is "the transcript could not be read", which is the absence of a CHECK
# and is announced, because a report that cannot run must never look like a
# report that found nothing.
if [ "$RC" = "2" ]; then
    _say "WHERE-HE-LEFT-OFF — could not read the transcript at $TRANSCRIPT. $HOOK_TAG" \
         "THE RETURN REPORT COULD NOT READ THE TRANSCRIPT ($TRANSCRIPT). This is the absence of a check, not the absence of a gap. If he has been away, read his last message yourself. $HOOK_TAG"
fi
[ "$RC" = "0" ] || _silent
[ -n "$REPORT" ] || _silent

# --- ONCE PER GAP, THEN TWO ANCHORS ----------------------------------------
# The key is the ANCHOR TIMESTAMP, so the same gap is reported once however
# many messages he sends into it, and a NEW gap always reports afresh.
[ -n "$KEY" ] || KEY="unkeyed"
LEDGER="$STATE_DIR/left-off.${SESSION_SHORT}.state"
PRIOR=""
PRIOR_N=0
if [ -f "$LEDGER" ]; then
    PRIOR="$(sed -n '1p' "$LEDGER" 2>/dev/null || true)"
    PRIOR_N="$(sed -n '2p' "$LEDGER" 2>/dev/null || true)"
fi
case "$PRIOR_N" in (*[!0-9]*|"") PRIOR_N=0 ;; esac

if [ "$PRIOR" = "$KEY" ]; then
    NEXT=$((PRIOR_N + 1))
    mkdir -p "$STATE_DIR" 2>/dev/null || true
    printf '%s\n%s\n' "$KEY" "$NEXT" > "$LEDGER" 2>/dev/null || true
    if [ "$NEXT" -le 2 ]; then
        _say "" "$ANCHOR  [you have already been given the full return report for this gap; $HOOK_TAG]"
    fi
    _silent
fi

mkdir -p "$STATE_DIR" 2>/dev/null || true
printf '%s\n1\n' "$KEY" > "$LEDGER" 2>/dev/null || true
_say "$ANCHOR" "$REPORT"
