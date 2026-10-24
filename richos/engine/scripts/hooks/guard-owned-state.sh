#!/usr/bin/env bash
#
# guard-owned-state.sh — PreToolUse hook on the Agent tool. SINCE 2026-10-24 IT
# REFUSES NOTHING (orchestration reliability R3; the end of this file): a system the
# orchestrator owns the health of that is known-bad, with nothing done about it this
# session, is TOLD to the lead once per session and system, and the dispatch goes on.
# The history below is the record of why the system is watched at all.
#
# The inventory, and the argument for every row in it, is in
# `owned-systems.declaration` at the engine root. The predicate — what healthy
# means, how a verdict is taken, how age is derived, what the cache is for —
# is in scripts/lib/owned-systems.py and NOWHERE ELSE. Read those first; this
# file is the wiring and the refusal.
#
# ===========================================================================
# THE FAILURE
# ===========================================================================
# 2026-09-10. The founder found, himself, in one day: a workflow red on main
# for thirteen days, 30 GB of worktrees that nothing would ever reclaim, and
# the engine's own hook text printing on his screen. He was then told the
# owned-outcome PRD would fix the general case, and he answered:
#
#     "even if that PRD were to get magically implemented within the next 5
#      seconds, that still doesn't fully answer the questions I previously
#      raised, does it?"
#
# He was right. That PRD is triggered by an ASSIGNMENT and carries that
# assignment's outcome to completion. EVERY DEFECT HE FOUND THAT DAY WAS
# UNASSIGNED. Nothing pointed at CI. Nothing pointed at the worktrees.
#
# AND THE PART THAT DECIDED THE SHAPE OF THIS FILE: THE WORST OF IT WAS
# ALREADY ON THE SCREEN. That same session's SessionStart notice said "worktree
# reconciler: BLOCKED (46 member(s) cannot proceed by waiting) ... live workers
# positively present=0" and the session read past it and got on with something
# else. Surfacing had already been tried and had already failed, in front of
# him, that morning.
#
# So this does not surface anything. It makes a known-bad state cost the only
# thing that has ever changed behavior here: THE ABILITY TO GET ON WITH
# SOMETHING ELSE.
#
# ===========================================================================
# ONE SYSTEM PER SESSION — THE WHOLE DESIGN IS IN THIS SECTION
# ===========================================================================
# This project has killed three guards (g11/g12/g13) in a single day by making
# them broad enough that waiving became the daily habit. A gate demanding a
# disposition for all six systems would be that guard: six waivers to type at
# the first dispatch of every session, every one of them typed without reading.
# Today's real state — six systems in the declaration, five of them standing —
# would have produced exactly that on day one.
#
# So the requirement is ONE. Precisely:
#
#   * The gate demands a disposition for the SINGLE oldest standing system.
#   * ANY disposition recorded this session satisfies the gate for the whole
#     session. Not one per system. One, total, ever, per session.
#   * MAKING THE DEMANDED SYSTEM SMALLER is itself a disposition, so the
#     natural reply to being refused — doing something about the problem — is
#     what clears it. This gate wants to be answered with work, not with a
#     marker. (Until 2026-09-25 "a prompt that mentions it" counted as work.
#     It was not; see the next section but one.)
#
# COST, STATED HONESTLY AND MEASURED RATHER THAN ASSERTED: at most ONE
# deliberate act per session, at the first teammate dispatch, and complete
# silence for the rest of that session. That is the test this engine applies to
# its own gates — "if it fires on ordinary work several times a day it is
# wrong" — and one-per-session passes it by a wide margin.
#
# ONE PER SESSION IS ALSO WHY THE OLDEST IS THE ONE DEMANDED. If the gate named
# whichever system were cheapest to clear, an operator would clear the cheap one
# every session forever and the thirteen-day red would never come up. The
# demanded item is the one that has been standing longest, its age is printed in
# the refusal, and it comes back tomorrow with a bigger number on it.
#
# ===========================================================================
# WHY THE Agent EVENT, AND NOTHING ELSE
# ===========================================================================
# Getting on with something else IS dispatching a teammate: this orchestrator
# does no work directly, so a spawn is the observable form of "moving on". The
# alternatives, argued rather than left to be re-derived:
#
#   Stop, blocking the turn — REJECTED for the reason guard-ceo-ask-first.sh
#     gives on the same choice: a turn that ends because the founder
#     interrupted is a turn ending correctly, and refusing those makes this
#     something to switch off. The end of a turn is a NOTICE's event.
#   Every Bash call — REJECTED. Reading a file is not "getting on with
#     something else", and a gate that fires on everything is a gate nobody can
#     work behind. That exact design was tried in this project's history (the
#     every-Bash-call data-contract gate) and was retired for this reason.
#   The land — REJECTED. Landing a teammate's finished work is the opposite of
#     ignoring an owned system; blocking it strands committed work, which is
#     the `unlanded` row's own failure mode.
#
# ===========================================================================
# WHAT COUNTS AS ADDRESSING IT: THE PROBLEM GOT SMALLER, NOT A PROMPT NAMED IT
# ===========================================================================
# THE FIRST VERSION GOT THIS WRONG, AND IT IS WORTH READING WHY. Each row
# declared a `match:` keyword pattern, and a dispatch whose prompt matched any
# standing row counted as working on it and cleared the gate for the whole
# session. The argument was that a false ADDRESS was cheap. The ledger says it
# was not. On 2026-09-25 the founder asked whether the escalation backlog (143
# outstanding, the oldest 19 days, 13 for him) had been "purely just discovered
# by accident". The dispositions log showed the escalations system "addressed"
# ONCE in fifteen days, by an unrelated dispatch whose text contained the word,
# while `staging|deploy` matched nearly every product dispatch and cleared every
# session for free. A keyword is not work. It is a waiver nobody had to type.
#
# So the predicate (scripts/lib/owned-systems.py, "WHAT ADDRESSED MEANS")
# decides, and it counts exactly two things:
#   progress  the system THIS SESSION WAS REFUSED OVER got smaller: its check
#             passes, its `measure:` fell below the fresh baseline taken at that
#             refusal, or its `backlog:` past the tolerated age reached zero.
#             Measured by the predicate. This hook sees it as exit 0.
#   triage    the prompt matches the DEMANDED row's `triage:` pattern, which
#             names the system's own record (for escalations, the ledger file).
# Both are silent when they clear a dispatch. A gate that lectured the
# operator at the moment he did the right thing would teach the wrong lesson.
#
# (Until 2026-10-24 an `owned-state-ack:` prompt line unlocked the refusal. With the
# refusal gone it unlocked nothing, so it was removed; an old line is ignored.)
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY — the deliberate choice, state by state
# ===========================================================================
#   NOT ADOPTED (no orchestration.config anywhere)  -> STAND DOWN, SILENT.
#     The engine loads at USER scope in every directory on this machine.
#   OWNED_STATE_GATE="0"                            -> STAND DOWN, SILENT.
#     The declared opt-out. Absent means ENABLED, which matches every other
#     guard here: enforcement that has to be switched on is enforcement a fresh
#     clone does not have.
#   DECLARATION MISSING OR MALFORMED                -> ALLOW + ANNOUNCE LOUD.
#     "Declared and unreadable" is a defense reporting 'on' while protecting
#     nothing, and it must never look like a clean run.
#   A CHECK CANNOT BE RUN                           -> that SYSTEM is UNKNOWN,
#     which stands like a red one but sorts below it. An absent check is never
#     a passing check.
#   PAYLOAD UNPARSEABLE                             -> ALLOW + ANNOUNCE. The
#     ack line cannot be read from such a payload either, so refusing would
#     block a dispatch the operator has no way to permit. Same choice, for the
#     same reason, as guard-model-ceiling.sh and guard-stale-staging.sh.
#   python3 MISSING                                 -> ALLOW + ANNOUNCE.
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing
# entirely. That is the shared bootstrap's contract and probe Layer R asserts
# every rooted hook carries it byte-identically.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds
# it.

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
  hook: scripts/hooks/guard-owned-state.sh
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

# announce_off <one-line> — the best-effort loud channel for a fail-open. BOTH
# stderr and systemMessage, because neither is proven for this event and a
# condition announced on nothing is the defect this whole file is about.
announce_off() {
    printf '%s\n' "$1" >&2
    if command -v python3 >/dev/null 2>&1; then
        SYSMSG="$1" python3 -c '
import json, os
print(json.dumps({"systemMessage": os.environ.get("SYSMSG", "")}))
' 2>/dev/null || true
    fi
}

if ! command -v python3 >/dev/null 2>&1; then
    announce_off "OWNED-STATE GATE IS OFF: python3 is not on PATH, so no owned system was checked and this dispatch is UNGATED."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "OWNED-STATE GATE IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether an owned system is standing unaddressed."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"

# --- THE OPT-OUT SWITCH ----------------------------------------------------
# ABSENT MEANS ENABLED. Deliberate and opposite to guard-stale-staging.sh's
# adoption switch: staging is a thing most repositories genuinely do not have,
# while "some system whose health you own is bad and you did nothing" is true
# of every repository an orchestrator runs in. A gate that has to be switched
# on is a gate a fresh clone does not have.
case "${OWNED_STATE_GATE:-1}" in
    0|no|off|false) exit 0 ;;
esac

LIB="$SCRIPT_DIR/../lib/owned-systems.py"
if [ ! -f "$LIB" ]; then
    announce_off "OWNED-STATE GATE IS OFF: scripts/lib/owned-systems.py is missing at $LIB, so its entire predicate is absent. Teammate dispatches are UNGATED — a clean run and an absent gate must never look the same."
    exit 0
fi

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard would otherwise take the SAME silent
# exit 0 that a well-formed payload for a different tool takes. This separates
# the two. NO VERDICT CHANGES — only the silence.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-owned-state.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether a system whose health this orchestrator owns is standing unhealthy with nothing done about it this session"
fi

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
# NOT `[ ... ] && [ ... ] && exit 0`. Under `set -e` a trailing false test in an
# && chain is a non-zero return from the last command, which exits the hook
# with 1 — a code the host reads as a hook error rather than a pass.
if [ -n "$TOOL_NAME" ] && [ "$TOOL_NAME" != "Agent" ]; then
    exit 0
fi

# --- Parse the spawn -------------------------------------------------------
# Newlines survive through a \001 placeholder so the ack marker can be matched
# with a line-start anchor, exactly as the other Agent guards do it.
PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    if not isinstance(ti, dict):
        raise ValueError("tool_input not an object")
    pr = str(ti.get("prompt", "") or "").replace("\t", " ").replace("\n", "\x01")
    print("OK\t%s\t%s\t%s\t%s" % (
        str(d.get("session_id", "") or ""),
        str(ti.get("subagent_type", "") or ""),
        str(ti.get("name", "") or ""),
        pr,
    ))
except Exception:
    print("PARSEFAIL\t\t\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t\t\t')"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f2)"
SUBAGENT_TYPE="$(printf '%s' "$PARSED" | cut -f3)"
NAME="$(printf '%s' "$PARSED" | cut -f4)"
PROMPT="$(printf '%s' "$PARSED" | cut -f5- | tr '\001' '\n')"

if [ "$STATUS" = "PARSEFAIL" ]; then
    announce_off "OWNED-STATE GATE: this Agent spawn could not be parsed, so this ONE dispatch was not checked against the owned systems (UNGATED)."
    exit 0
fi

# --- IS ANYTHING DEMANDED OF THIS SESSION? ---------------------------------
# The predicate answers three things at once: is anything standing, has this
# session already disposed of something, and if not WHICH system is demanded.
# Cached inside the library, so the checks are run at most once per session.
DRC=0
DEMAND="$(python3 "$LIB" demanded --entity "$ENTITY_ROOT" --engine "$ENGINE_ROOT" \
            --session "$SESSION_ID" --agent "${NAME:-${SUBAGENT_TYPE:-<unset>}}" 2>/dev/null)" || DRC=$?

case "$DRC" in
    0) exit 0 ;;          # nothing demanded, this session already disposed, or
                          # the demanded system got smaller (recorded as progress)
    1) ;;                 # a system is demanded
    *)
        announce_off "OWNED-STATE GATE IS OFF: the owned-systems declaration could not be read, so nothing is being watched and this dispatch is UNGATED. Run scripts/owned-state.sh to see the error in full."
        exit 0 ;;
esac

DEMANDED_ID="$(printf '%s' "$DEMAND" | python3 -c 'import json,sys; print(json.load(sys.stdin)["demanded"])' 2>/dev/null || true)"
if [ -z "$DEMANDED_ID" ]; then
    announce_off "OWNED-STATE GATE IS OFF: the predicate reported a standing system but named none, which is a defect in scripts/lib/owned-systems.py. This dispatch is UNGATED."
    exit 0
fi

# --- IS THIS DISPATCH A NAMED TRIAGE OF THE DEMANDED SYSTEM? ---------------
# NOT "does the prompt mention it". Until 2026-09-25 a keyword did, and the
# dispositions ledger is the proof of what that bought: the escalations system
# was "addressed" once in fifteen days, by an unrelated dispatch whose text
# happened to contain the word, while its backlog grew to 143. The demanded
# row's `triage:` pattern names the system's own record explicitly (for
# escalations, the ledger FILE), and only the DEMANDED row's pattern is
# consulted: a triage of a cheaper system is the rotation this gate refuses.
TRIAGED="$(printf '%s' "$DEMAND" | PROMPT_TEXT="$PROMPT" python3 -c '
import json, os, re, sys
d = json.load(sys.stdin)
pattern = d.get("triage") or ""
if not pattern:
    sys.exit(0)
prompt = os.environ.get("PROMPT_TEXT", "")
try:
    m = re.search(pattern, prompt, re.IGNORECASE)
except re.error:
    m = None
if m:
    print(m.group(0))
' 2>/dev/null || true)"

if [ -n "$TRIAGED" ]; then
    python3 "$LIB" dispose --entity "$ENTITY_ROOT" --session "$SESSION_ID" \
        --system "$DEMANDED_ID" --how triage --agent "${NAME:-${SUBAGENT_TYPE:-<unset>}}" \
        --reason "dispatch names the system's own record: ${TRIAGED}" >/dev/null 2>&1 || true
    exit 0
fi

# --- TOLD, NEVER REFUSED (orchestration reliability R3) --------------------
# richos-hq docs/prds/2026-10-23-orchestration-reliability.md: "An unrelated ...
# old health incident remains owned in its current queue and existing watcher, but
# does not veto a repair, inspection, ordinary answer or independent dispatch."
# 2026-10-23 19:46Z: the repair of the review tools was refused over the ECS
# capture backlog (1,143 historical records, first seen 42.9 days before) until an
# acknowledgement line was written. The standing system is now told to the LEAD once
# per session and system, on the model channel (PreToolUse additionalContext), with
# its evidence and the actions that resolve it; the dispatch goes ahead.
TOLD="$ENTITY_ROOT/.claude/state/owned-state-told.log"
KEY="$(printf '%s\t%s' "${SESSION_ID:-<unset>}" "$DEMANDED_ID")"
if [ -n "$SESSION_ID" ] && grep -qxF -- "$KEY" "$TOLD" 2>/dev/null; then
    exit 0
fi
mkdir -p "$(dirname "$TOLD")" 2>/dev/null && printf '%s\n' "$KEY" >>"$TOLD" 2>/dev/null || true
printf '%s' "$DEMAND" | REPORT="${ENGINE_ROOT}/scripts/owned-state.sh ${ENTITY_ROOT}" python3 -c '
import json, os, sys
d = json.load(sys.stdin)
ev = " | ".join(line.strip() for line in (d.get("evidence") or [])[:4])
ways = "; ".join(w.strip() for w in (d.get("progress") or [])[:3])
text = ("OWNED SYSTEM STANDING (not a refusal; this dispatch goes ahead): %s (%s) is %s, standing %s%s. "
        "Evidence: %s. What makes it smaller: %s%s. It holds no unrelated work. Full report: %s"
        % (d["demanded"], d["title"], d["status"], d["age"], ("; %s" % d["size"]) if d.get("size") else "",
           ev or "(none recorded)", ways or "see the report",
           ("; or a dispatch naming its record (%s)" % d["triage"]) if d.get("triage") else "",
           os.environ["REPORT"]))
print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": text}}))
' 2>/dev/null || true
exit 0
