#!/usr/bin/env bash
#
# commit-ceo-inputs.sh — NON-BLOCKING UserPromptSubmit hook. THE INGRESS.
#                        A FILE HE HANDS OVER ENTERS THE RECORD BY ITSELF.
#
# The predicate, the gates, the git plumbing, the refusals and the reasoning
# behind every one of them are in commit-ceo-inputs.py. Read that first — it is
# the analysis half. This file is the wiring, the channel and the one sentence.
#
# ===========================================================================
# THE RULE
# ===========================================================================
# A path this message names, that exists on disk, that sits inside a git
# repository, and that git is NOT holding, is COMMITTED — unmodified, now, with
# a message saying it is his input and where it came from.
#
# Unless a safety gate refuses. Then nothing is committed and the refusal is
# stated with its reason and where the file should live instead.
#
# ===========================================================================
# WHY IT COMMITS INSTEAD OF REMINDING
# ===========================================================================
# The first version of this hook only noticed. The CEO rejected it in one line:
#
#     "'it does not commit for you': Then who commits for me? Santa Claus?"
#
# His complaint was that a document went uncommitted because the orchestrator
# forgot. A mechanism whose last link is the orchestrator remembering is the
# same failure with a reminder bolted on. So the judgment is made MECHANICAL
# rather than reserved for a human step, out of gates this engine already ships
# — scan-secrets.sh and guard-publication-writes.sh — invoked, never rewritten.
#
# ===========================================================================
# WHY IT NEVER BLOCKS — MEASURED
# ===========================================================================
# Claude Code 2.1.261, from the shipped binary's own hook-event table:
#
#     UserPromptSubmit
#       Exit code 0      - stdout shown to Claude
#       Exit code 2      - block processing, ERASE ORIGINAL PROMPT, show stderr
#       Other exit codes - show stderr to user only
#
# A blocking ingress hook does not hold his message pending a fix. IT DELETES
# WHAT HE TYPED. So the only exit code in this file is 0, on every path
# including catastrophe, and the same binary's `suppressOriginalPrompt` — the
# same destruction wearing a JSON key — is never emitted.
#
# Default timeout for this event is 30000 ms and the registration sets its own.
# A local git commit is milliseconds; the budget is not close. Hooks are
# ordinary spawned processes with no restriction on side effects, and this
# engine already performs git side effects from hooks — terminalize-agent-
# worktrees.sh runs `git update-ref` from SubagentStop. Nothing here is a new
# capability; it is an established one pointed at the ingress.
#
# ===========================================================================
# THE CHANNEL — BOTH AUDIENCES, ONE JSON OBJECT
# ===========================================================================
# hookSpecificOutput.additionalContext reaches the ORCHESTRATOR (measured cap:
# 8000 characters, 200 lines; a hookEventName that does not match the event
# causes the host to DROP the whole object, which is why it is spelled out
# literally below). systemMessage reaches the OPERATOR (cap 4000 characters, 20
# lines) and is kept to one line.
#
# ONE object, because the host parses stdout as a single JSON document — two
# would be a parse failure, and a parse failure here degrades to the raw text
# being injected, which is ugly but still carries the fact. Degrading toward
# noise is recoverable by someone who can read it; degrading toward silence
# rebuilds the defect.
#
# ===========================================================================
# THE ABSENCE OF A FINDING IS NOT THE ABSENCE OF A CHECK
# ===========================================================================
# Every run appends one line to <entity>/.claude/state/ceo-inputs.jsonl — a run
# that committed nothing writes a line saying so. That ledger is the positive
# trace, and it is what notice-ceo-inputs-unheld.sh re-reads at every turn end,
# so a REFUSAL cannot be announced once and then forgotten. A hook fires on one
# message and cannot tell whether anyone acted; the Stop-side partner is how
# that loop closes.
#
# Every way this hook can fail to do its job — stood down, no python3, no
# analyzer, unresolvable root, unreadable payload — is ANNOUNCED. Silence from
# this hook means "it ran and there was nothing to hold", and nothing else.
#
# ===========================================================================
# WHEN IT TAKES EFFECT
# ===========================================================================
# Hooks snapshot at session start. Installing this changes NOTHING in the
# session that installs it; it begins working in the NEXT session. Verify it
# with a direct invocation and a constructed payload:
#
#     scripts/hooks/commit-ceo-inputs.sh --self-test
#
# Exit codes: always 0. This hook never refuses a message.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

HOOK_TAG="(hook: scripts/hooks/commit-ceo-inputs.sh)"

if [ "${1:-}" = "--self-test" ]; then
    _SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    exec bash "$_SELF_DIR/ceo-inputs.test.sh"
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
  hook: scripts/hooks/commit-ceo-inputs.sh
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

# --- OUTPUT ----------------------------------------------------------------
# Builtins only. One of the states this file must announce is "python3 is not
# on PATH", so an emitter that needed python3 could not report the very
# condition it exists to report. Backslash first, or the escapes introduced
# afterwards get escaped again.
_esc() {
    local s="${1:-}"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\r'/ }"
    s="${s//$'\t'/ }"
    printf '%s' "$s"
}

# _say <system-message-one-line> <orchestrator-context>
#
# One JSON object, then exit 0. hookEventName is spelled literally because the
# host drops the entire hookSpecificOutput when it does not match the event.
_say() {
    printf '{"systemMessage":"%s","hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"%s"}}\n' \
        "$(_esc "${1:-}")" "$(_esc "${2:-}")"
    exit 0
}

# _say_once <state-key> <system-message> <orchestrator-context>
#
# The orchestrator hears an abnormal state on EVERY message, because its
# context is rebuilt each turn and a condition it was told about yesterday is a
# condition it does not know today. The operator hears it on STATE CHANGE only:
# an unchanging line under every message is a line the eye is trained to skip,
# and the rational response to that is to mute it — after which the protection
# is off and there is a muted line reporting it. Same trade, and the same
# reasoning, as scripts/lib/stop-hook-notice.sh.
_say_once() {
    local key="${1:-}" msg="${2:-}" ctx="${3:-}" prior="" ledger=""
    if [ -n "${_STATE_DIR:-}" ] && [ -n "${_SESSION_SHORT:-}" ]; then
        ledger="$_STATE_DIR/ceo-inputs-notice.${_SESSION_SHORT}.state"
        mkdir -p "$_STATE_DIR" 2>/dev/null || true
        [ -f "$ledger" ] && prior="$(cat "$ledger" 2>/dev/null || true)"
        printf '%s\n' "$key" > "$ledger" 2>/dev/null || true
    fi
    if [ "$prior" = "$key" ]; then
        _say "" "$ctx"
    fi
    _say "$msg" "$ctx"
}

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine. There is no ledger to write and
    # no configuration to read, so the ingress stands down — silently, and only
    # here, because engine-status.sh announces the whole stand-down at every
    # session start and repeating it on every message would be the noise this
    # file refuses everywhere else.
    exit 0
else
    _STATE_DIR=""
    _SESSION_SHORT=""
    _say "INGRESS IS OFF: the hook that commits files you hand over cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing you hand over is being captured. $HOOK_TAG" \
         "INGRESS CAPTURE IS NOT RUNNING. commit-ceo-inputs.sh could not resolve the repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}), so NO file named in this message was checked or committed. Do not read the absence of a finding as the absence of a handed-over file. $HOOK_TAG"
fi

_STATE_DIR="$ENTITY_ROOT/.claude/state"
_SESSION_SHORT="$(printf '%s' "$INPUT" | tr '\n' ' ' \
    | grep -o '"session_id"[[:space:]]*:[[:space:]]*"[^"]*"' \
    | head -1 | sed 's/.*"\([^"]*\)"$/\1/' 2>/dev/null || true)"
_SESSION_SHORT="$(printf '%s' "$_SESSION_SHORT" | tr -cd '[:alnum:]-' | cut -c1-8)"
[ -n "$_SESSION_SHORT" ] || _SESSION_SHORT="nosession"

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CHECK_CEO_INPUTS:=1}"

if [ "$CHECK_CEO_INPUTS" = "0" ]; then
    # Never a silent permission. An opt-out nobody can see is a defense that
    # decays into a rumour — and this hook exists because a document went
    # unheld quietly.
    _say_once "stood-down" \
        "INGRESS — STOOD DOWN by CHECK_CEO_INPUTS=0 in $CONFIG. Files you hand over are NOT being committed, so an untracked document will stay untracked. $HOOK_TAG" \
        "INGRESS CAPTURE IS STOOD DOWN (CHECK_CEO_INPUTS=0 in $CONFIG). No file named in this message was checked or committed. $HOOK_TAG"
fi

if ! command -v python3 >/dev/null 2>&1; then
    _say_once "no-python3" \
        "INGRESS — NOT RUNNING: python3 is not on PATH, so nothing you hand over is being captured. $HOOK_TAG" \
        "INGRESS CAPTURE IS NOT RUNNING: python3 is not on PATH. NO file named in this message was checked or committed. $HOOK_TAG"
fi

if ! command -v git >/dev/null 2>&1; then
    _say_once "no-git" \
        "INGRESS — NOT RUNNING: git is not on PATH, so nothing you hand over is being captured. $HOOK_TAG" \
        "INGRESS CAPTURE IS NOT RUNNING: git is not on PATH. NO file named in this message was checked or committed. $HOOK_TAG"
fi

ANALYZER="$SCRIPT_DIR/commit-ceo-inputs.py"
if [ ! -f "$ANALYZER" ]; then
    _say_once "no-analyzer" \
        "INGRESS — NOT RUNNING: the analyzer is missing at $ANALYZER, so nothing you hand over is being captured. $HOOK_TAG" \
        "INGRESS CAPTURE IS NOT RUNNING: the analyzer is missing at $ANALYZER. NO file named in this message was checked or committed. $HOOK_TAG"
fi

set +e
RESULT="$(printf '%s' "$INPUT" \
    | RICHOS_ENGINE_ROOT_FOR_GATES="$ENGINE_ROOT" \
      RICHOS_INGRESS_STATE_DIR="$_STATE_DIR" \
      RICHOS_INGRESS_SEAT_ROOT="$ENTITY_ROOT" \
      python3 "$ANALYZER" 2>/dev/null)"
RC=$?
set -e

if [ "$RC" = "1" ] || { [ "$RC" != "0" ] && [ "$RC" != "3" ] && [ "$RC" != "4" ]; }; then
    _say_once "analyzer-rc-$RC" \
        "INGRESS — DID NOT COMPLETE (exit $RC). This message was not checked for files you handed over. $HOOK_TAG" \
        "INGRESS CAPTURE DID NOT COMPLETE: the analyzer exited $RC. NO file named in this message was checked or committed — this is the absence of a CHECK, not the absence of a finding. $HOOK_TAG"
fi

# Healthy from here. Record the healthy state so a recovery from any of the
# announcements above is itself announced, and stay quiet when there is
# nothing to say.
if [ -n "$_STATE_DIR" ]; then
    mkdir -p "$_STATE_DIR" 2>/dev/null || true
    _PRIOR=""
    _LEDGER="$_STATE_DIR/ceo-inputs-notice.${_SESSION_SHORT}.state"
    [ -f "$_LEDGER" ] && _PRIOR="$(cat "$_LEDGER" 2>/dev/null || true)"
    printf 'ok\n' > "$_LEDGER" 2>/dev/null || true
    if [ -n "$_PRIOR" ] && [ "$_PRIOR" != "ok" ]; then
        _RECOVERED="INGRESS — RUNNING AGAIN. Files you hand over are being captured into the record. $HOOK_TAG"
    else
        _RECOVERED=""
    fi
else
    _RECOVERED=""
fi

if [ "$RC" = "0" ]; then
    # Ran, and nothing this message named needed holding. SILENT by design:
    # this is the ordinary case for almost every message, and a line under
    # every one of them is the muting failure. The positive trace that the
    # check RAN is the ledger line the analyzer just wrote.
    [ -n "$_RECOVERED" ] && _say "$_RECOVERED" ""
    exit 0
fi

# A finding. Render it — paths, repositories and outcomes only, never content.
REPORT="$(printf '%s' "$RESULT" | python3 -c '
import json, sys

try:
    d = json.load(sys.stdin)
except Exception:
    print("SUMMARY\tINGRESS produced output that could not be read — treat this message as UNCHECKED.")
    print("CONTEXT\tINGRESS CAPTURE produced unreadable output. Assume NO file named in this message was captured.")
    sys.exit(0)

committed = d.get("committed") or []
refused = d.get("refused") or []
pending = d.get("pending") or []
reported = d.get("reported") or []
undecided = d.get("undecided") or []

lines = []
if committed:
    lines.append("A FILE YOU HANDED OVER IS NOW IN THE RECORD. Committed unmodified by the ingress hook:")
    for c in committed[:20]:
        lines.append("  COMMITTED  %s" % c.get("path", ""))
        lines.append("             %s in %s" % ((c.get("sha") or "")[:12], c.get("repo", "")))
if refused:
    lines.append("")
    lines.append("NOT COMMITTED — A GATE REFUSED. Nothing was written; these need a decision:")
    for r in refused[:20]:
        lines.append("  REFUSED    %s" % r.get("path", ""))
        lines.append("             %s" % r.get("why", ""))
if pending:
    lines.append("")
    lines.append("NOT COMMITTED YET: ANOTHER LAND HOLDS THE LAND LEASE. Kept pending and retried on the next message:")
    for r in pending[:20]:
        lines.append("  PENDING    %s" % r.get("path", ""))
        lines.append("             %s" % r.get("why", ""))
if d.get("lease_notes"):
    lines.append("")
    lines.append("THE LAND LEASE SAID, when the ingress took it for this commit (read it: a renewal is how a holder is told):")
    for n in d["lease_notes"][:10]:
        lines.append("  " + n)
if reported:
    lines.append("")
    lines.append("UNHELD, AND NO REPOSITORY OBVIOUSLY OWNS IT:")
    for r in reported[:20]:
        lines.append("  UNHELD     %s" % r.get("path", ""))
        lines.append("             %s" % r.get("why", ""))
if undecided:
    lines.append("")
    lines.append("UNDECIDED — git could not answer. This is the absence of a CHECK, not a clean result:")
    for u in undecided[:20]:
        lines.append("  UNDECIDED  %s  (%s)" % (u.get("path", ""), u.get("why", "")))
if d.get("ledger_error"):
    lines.append("")
    lines.append("THE LEDGER COULD NOT BE WRITTEN (%s), so the turn-end re-check has nothing to read and a refusal above will NOT be repeated." % d["ledger_error"])
if d.get("truncated"):
    lines.append("")
    lines.append("This message named more paths than the ingress will examine; the list above is not complete.")

lines.append("")
lines.append("THE HOOK DOES NOT MODIFY HIS FILE and never prints its content. A refusal is his exception, stated narrowly: unless the file is something that should not be committed to git for some reason. Act on the refusals; the commits are already done.")

nc, nr = len(committed), len(refused)
bits = []
if nc:
    bits.append("%d file%s you handed over committed" % (nc, "" if nc == 1 else "s"))
if nr:
    bits.append("%d REFUSED by a safety gate" % nr)
if pending:
    bits.append("%d not committed yet (another land holds the lease; retried on your next message)" % len(pending))
if d.get("lease_notes"):
    bits.append("the land lease named something unfinished in the checkout")
if reported:
    bits.append("%d unheld outside every repository" % len(reported))
if undecided:
    bits.append("%d undecided" % len(undecided))
summary = "INGRESS: " + "; ".join(bits) + "." if bits else ""

print("SUMMARY\t" + summary)
print("CONTEXT\t" + "\\n".join(lines))
' 2>/dev/null || true)"

if [ -z "$REPORT" ]; then
    _say "INGRESS — the outcome could not be rendered. Treat this message as UNCHECKED. $HOOK_TAG" \
         "INGRESS CAPTURE ran but its outcome could not be rendered, so assume NO file named in this message was captured. $HOOK_TAG"
fi

SUMMARY="$(printf '%s' "$REPORT" | grep '^SUMMARY' | head -1 | cut -f2-)"
CONTEXT="$(printf '%s' "$REPORT" | grep '^CONTEXT' | head -1 | cut -f2-)"
CONTEXT="${CONTEXT//\\n/$'\n'}"

[ -n "$_RECOVERED" ] && SUMMARY="$_RECOVERED $SUMMARY"

_say "$SUMMARY $HOOK_TAG" "$CONTEXT

$HOOK_TAG"
