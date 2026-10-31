#!/usr/bin/env bash
#
# guard-interactive-prompt.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# At 02:01 on 2026-09-01 a macOS password window appeared on the CEO's screen:
# "Enter the password for D.p12". An agent had run
#
#     security import D.p12 -k <scratch>/t3.keychain-db -T /usr/bin/codesign
#
# with no -P. A MISSING passphrase is not an empty passphrase — macOS escalates
# to SecurityAgent, which draws a window on the logged-in user's screen, and the
# calling process waits on it forever. PID 70803, killed by hand.
#
# His question was, bluntly, when that would STOP — and this hook is the answer.
#
# THE ROOT CAUSE IS A HOLE IN THE GUARD SET, NOT A CARELESS ENGINEER. Forty-one
# guards were registered that night. Every one of them inspects text and state —
# files, commit contents, agent names, spawn prompts, worktree locks, dialect,
# secrets. NOT ONE ASKED WHETHER A COMMAND CAN WAIT ON A HUMAN. Combined with
# shell commands executing without per-call approval, nothing at all sat between
# that command and his screen. A rule in CLAUDE.md would not have: the engineer
# who ran it was mid-repro on a signing problem and had every reason to believe
# the command was inert.
#
# ===========================================================================
# WHAT IT DOES
# ===========================================================================
# Refuses a Bash command that can stop and wait for a person, AND NAMES THE FLAG
# that makes it fail instead of wait. The judgment is entirely in
# scripts/lib/interactive-prompt.py, which carries the shape table, the two-tier
# policy and the measured false-positive record. This file is the wiring: root
# resolution, the refusal text, and the exit code.
#
#   verdict block   -> exit 2, refusal on stderr, the fix named per finding
#   verdict report  -> exit 0, one line on stderr, the command runs
#   verdict clean   -> exit 0, silent
#
# ===========================================================================
# NO ESCAPE HATCH ON THE BLOCKING TIER, AND THAT IS THE DESIGN
# ===========================================================================
# Several guards in this engine carry a live opt-out line, because they can be
# wrong in a way the author cannot route around. This one cannot: every blocking
# refusal names a one-token fix that keeps the command working and is what the
# author meant anyway. `-P ''`. `-n`. `-o BatchMode=yes`. So there is no waiver
# marker, no ack file and no config switch that turns the blocking tier off.
#
# That decision is only defensible because it was MEASURED rather than asserted.
# Against 65,781 unique Bash commands drawn from every Claude Code transcript on
# this machine, the blocking tier fires 4 times — 0.006% — and all four are
# `security import` with no -P, the exact incident shape. The full record,
# including the shapes that were CUT for measuring badly, is in
# scripts/hooks/interactive-prompt.corpus.md.
#
# ===========================================================================
# FAIL-CLOSED, AND WHAT THAT MEANS HERE
# ===========================================================================
# Missing python3, or a missing scripts/lib/interactive-prompt.py, is exit 2 with
# the shared BROKEN INSTALL banner. A guard that cannot judge must not wave
# things through: the whole failure being fixed is a defense that was not there.
# The refusal names the file it went looking for, which is what
# scripts/lib/sandbox-completeness.sh reads to prove this hook can start in every
# sandbox that claims to model the engine.
#
# A repository that never adopted the engine stands down, like every other rooted
# guard — see scripts/lib/resolve-roots.sh §"not-adopted". `broken` blocks.
#
# ===========================================================================
# WHAT THIS DOES NOT COVER — the app-facing half is somebody else's row
# ===========================================================================
# RichOS MUST raise real microphone and accessibility dialogs or it cannot work.
# This guard governs COMMANDS AGENTS RUN. It says nothing about shipped code
# paths, and it must never be extended to, because the guarantee there is a
# different one: every prompt follows a user-initiated action and none blocks a
# background operation. RICH-TODOs.md row g4 names that half separately.
#
# It also does not look inside scripts. `bash app/scripts/install-signing-cert.sh`
# is one token here, and that script contains `security import`. Stated in
# scripts/lib/interactive-prompt.py under "WHAT THIS CANNOT SEE", and repeated
# here so nobody has to find it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || {
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-interactive-prompt.sh"
        echo "  python3 is required for this guard and is not on PATH."
        echo "  Refusing rather than passing every command through unjudged."
    } >&2
    exit 2
}

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
  hook: scripts/hooks/guard-interactive-prompt.sh
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

ANALYZER="$ENGINE_ROOT/scripts/lib/interactive-prompt.py"
if [ ! -f "$ANALYZER" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-interactive-prompt.sh"
        echo "  scripts/lib/interactive-prompt.py is missing at: $ANALYZER"
        echo "  Every judgment this guard makes comes from that file. Without it"
        echo "  no command can be checked for the ability to wait on a human."
        echo "  Refusing rather than passing every command through unjudged."
    } >&2
    exit 2
fi

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    :
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # Never adopted, so there is no enforcement to lose here. engine-status.sh
    # announces the stand-down at session start; this is not a silent skip.
    exit 0
else
    root_failure_banner "scripts/hooks/guard-interactive-prompt.sh" >&2
    exit 2
fi
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes: the tool-name extraction ends
# in `|| true`, so "this call is not mine" and "I could not tell whose call this
# is" are one exit. That is why 17 of 25 PreToolUse guards were measured passing
# a call in complete silence on 2026-09-05. This separates the two. NO VERDICT
# CHANGES — the exit is the one already taken — only the silence does. The
# measurement, the channel and the argument: scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-interactive-prompt.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this command opens an interactive prompt with nobody at the keyboard"
fi

# FAIL-CLOSED, as the header promises (hunt part 3, finding 28): an analyzer
# that crashes, prints nothing or prints something unreadable produced NO
# verdict, and no verdict is not "clean".
_ANALYZER_FAILED=""
RESULT="$(printf '%s' "$INPUT" | python3 "$ANALYZER" 2>/dev/null)" || _ANALYZER_FAILED=1
[ -n "$RESULT" ] || _ANALYZER_FAILED=1

VERDICT="$(printf '%s' "$RESULT" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("verdict","clean"))
except Exception: print("unreadable")')"
[ "$VERDICT" != "unreadable" ] || _ANALYZER_FAILED=1
if [ -n "$_ANALYZER_FAILED" ]; then
    {
        echo "=== guard-interactive-prompt: BLOCKED (no verdict) ==="
        echo "  scripts/lib/interactive-prompt.py crashed or returned nothing readable, so"
        echo "  this command was NOT checked for an interactive prompt. A check that"
        echo "  produced no answer is not a clean answer."
        echo "(hook: scripts/hooks/guard-interactive-prompt.sh)"
    } >&2
    exit 2
fi

case "$VERDICT" in
  clean) exit 0 ;;

  report)
    printf '%s' "$RESULT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
for f in d.get("findings", []):
    if f.get("severity") != "report":
        continue
    sys.stderr.write("(hook: guard-interactive-prompt) NOTE: `%s` may wait on a human "
                     "(%s) — %s. FIX: %s\n" % (f["token"], f["shape"], f["why"], f["fix"]))
' >&2 || true
    exit 0 ;;

  block)
    {
        echo "=== guard-interactive-prompt: BLOCKED ==="
        echo "  This command can STOP AND WAIT FOR A HUMAN. Agents run without a"
        echo "  terminal, so waiting means one of two things: a window drawn on the"
        echo "  CEO's screen, or a process hung until somebody notices and kills it."
        echo "  Both have happened. 2026-09-01, 02:01, PID 70803."
        echo ""
        printf '%s' "$RESULT" | python3 -c '
import json, sys
for f in json.load(sys.stdin).get("findings", []):
    if f.get("severity") != "block":
        continue
    sys.stdout.write("  * %s (%s)\n      why: %s\n      FIX: %s\n"
                     % (f["token"], f["shape"], f["why"], f["fix"]))
' || echo "  (the analyzer produced a blocking verdict with no readable finding)"
        echo ""
        echo "  Apply the fix and re-run. There is no waiver line for this guard:"
        echo "  every refusal above names a change that keeps the command working."
        echo "(hook: scripts/hooks/guard-interactive-prompt.sh)"
    } >&2
    exit 2 ;;

  *) exit 0 ;;
esac
