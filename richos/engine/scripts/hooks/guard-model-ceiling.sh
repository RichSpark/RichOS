#!/usr/bin/env bash
#
# guard-model-ceiling.sh — BLOCKING PreToolUse guard on the Agent tool.
#
# REFUSES A SPAWN ON A MODEL TIER ABOVE THE DECLARED COST CEILING, AND CARRIES
# THE WHOLE RULE IN THE REFUSAL.
#
# ===========================================================================
# THE FAILURE THIS EXISTS TO PREVENT
# ===========================================================================
# The top-tier model is the most capable AND roughly twice the price of the one
# below it, and it burns the subscription quota much faster. Those are two
# different orders, and until 2026-09-03 this engine declared only one of them.
# MODEL_TIERS says which model is STRONGER. It has never said which to SPEND,
# and an orchestrator reading a capability order as a spending order picks the
# top of the list every time something feels important.
#
# The founder's ruling, 2026-09-03: the normal ceiling for CRITICAL work is the
# tier below the top; the top tier is for SUPER-CRITICAL work and extreme
# ONE-OFF cases. The cost of the omission is measured rather than feared — the
# build of one worktree rebuild consumed roughly a third of a week's top-tier
# allowance, and "the founder allowed the top tier for the critical thing"
# quietly became "the top tier is what critical work gets".
#
# HIS OWN DESIGN FOR THIS GUARD, in his words: "Whenever Fable pick is attempted
# it should simply get the orchestrator to reach that note I've just written and
# get him to acknowledge (if he continues with the Fable pick) or reconsider (and
# pick Opus instead)." Both halves — acknowledge, or reconsider — are actionable
# from the refusal text below WITHOUT opening a file, because a refusal that
# says "see the ruling" is a refusal that gets read past.
#
# ===========================================================================
# WHO THIS BLOCKS — deliberately, and it is the point
# ===========================================================================
# THE ORCHESTRATOR, not the founder. The founder greenlighting the top tier in
# conversation is a perfectly good reason: the orchestrator writes THAT as the
# reason on the ack line, and the log preserves that he said it. What is refused
# is the reflex — reaching for the strongest alias because the work feels
# important — which is exactly what an unwritten ceiling cannot stop.
#
# EVERY Agent spawn is in scope, including read-only and generic types. Spend is
# spend: a top-tier `Explore` costs the same as a top-tier engineer.
# guard-worktree-isolation.sh exempts those types from ISOLATION because they
# write no files; that has nothing to do with what they cost.
#
# ===========================================================================
# WHY PreToolUse[Agent], AND NOTHING ELSE
# ===========================================================================
# The model is chosen at exactly one moment — the spawn — and this is the only
# event that sees it before it is paid for. The alternatives, argued rather than
# left to be re-derived:
#
#   SubagentStart — REJECTED. The instance has already booted; a refusal there
#     is a bill with a lecture attached.
#   Stop, as a notice — REJECTED. The founder asked for acknowledge-or-
#     reconsider AT THE PICK. A turn-end notice reports a spend that already
#     happened, which is the thing he is trying not to pay for.
#   A lint over the transcript — REJECTED for the reason every honor-system
#     check in this engine has been retired for: a rule enforced by attention
#     is a rule that holds until the session is busy.
#
# CHAIN POSITION: LAST, after verify-agent-prompt.sh. The four
# structural guards decide whether the SPAWN is well formed; a dispatch that is
# both malformed and over the ceiling should hear about the malformed half
# first, because that is the half the operator can fix without leaving the
# keyboard. Between this and the CEO-ask gate the order is not load-bearing —
# both are policy — and appending rather than inserting was also the merge-safe
# choice while another engineer held hooks.json open.
#
# ===========================================================================
# THE CEILING IS DATA, READ FROM ONE LINE
# ===========================================================================
#     orchestration.config:  MODEL_CEILING="opus"
#
# Parsed ONLY through scripts/lib/model-tiers.sh, the same contract MODEL_TIERS
# has. This guard REFUSES A TIER, never an alias by name: it computes the rank
# of the declared ceiling and the rank of the resolved model and compares the
# two integers. Nothing here contains the word "fable" as a decision. When a
# model above the current top ships, the declaration is re-derived and this file
# needs no edit — which is the whole reason the 2026-09-02 incident's fix was
# data rather than a guard with a name baked into it.
#
# THE MODEL IS RESOLVED BY scripts/lib/resolve-model.sh — the SAME function
# guard-worktree-isolation.sh uses for its truthful-name and capability-tier
# clauses. Not a copy of it. Two resolvers that disagree about which model a
# spawn boots on would produce a refusal naming a model the other guard does not
# see, and this engine has shipped that shape (one question, two answers) often
# enough to know how it ends.
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY — every time
# ===========================================================================
# There is no state in which this guard refuses because it could not decide:
#
#   NOT ADOPTED (no orchestration.config)      -> STAND DOWN, silent. The engine
#     loads at user scope in every directory on the machine; a repository that
#     never adopted it has no ceiling to enforce and a notice in each would be
#     noise.
#   MODEL_CEILING NOT DECLARED                 -> ALLOW, ANNOUNCE. This is the
#     normal state of a fresh adopter, and it is announced on EVERY spawn rather
#     than once, because the alternative is a repository quietly running with no
#     ceiling while a guard sits in the chain looking like protection. The
#     announcement stops the moment the key is declared.
#   CEILING OR MODEL UNRANKABLE / CONFIG OR
#   LIBRARY UNREADABLE / ROOTS UNRESOLVABLE    -> ALLOW, ANNOUNCE. Same
#     argument: "I cannot tell" is never "forbidden", and a guard that wedges
#     every dispatch over its own plumbing is a guard that gets switched off.
#   MODEL UNDETERMINABLE (no override and no
#   definition default, or model:"inherit")    -> ALLOW, SILENT. The instance
#     boots on the SESSION's model; there is no pick to judge, and every spawn
#     of a host built-in is in this state. Announcing here would be a nag on the
#     common path, and a nag is how a guard becomes something to disable.
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing
# entirely. That is the shared bootstrap's contract (a hook that cannot tell
# which repository it governs must not guess), it is byte-identical in every
# rooted hook, and probe Layer R asserts that it stays so.
#
# NOTE ON THE LOUD CHANNEL. On a PASSING exit this hook announces on stderr AND
# as a `systemMessage`. The systemMessage channel is PROVEN for Stop hooks and
# only for Stop hooks (scripts/lib/stop-hook-notice.sh carries the measurement);
# whether it reaches the operator from PreToolUse is UNVERIFIED. Both are used
# because a condition announced on nothing is the defect this file is about.
#
# ===========================================================================
# THE ESCAPE HATCH — a live prompt line, with a reason, logged
# ===========================================================================
#     model-ceiling-ack: <reason>
#
# anywhere on its own line in the Agent spawn prompt. It permits that ONE spawn
# and appends to <entity root>/.claude/state/model-ceiling-acks.log. Same idiom
# as `model-downgrade-ack:`, `main-checkout-run:`, `resume-ack:` and
# `ceo-todos-deferred:` — and named for the CEILING rather than for a model,
# because the guard refuses a TIER and the name has to stay true when the top
# alias is renamed.
#
# A BARE TOKEN EXEMPTS NOTHING, and neither does a reflex. Beyond the length
# floor this engine's staffing gate already uses, a reason whose entire content
# is an assertion of merit — "most capable", "better", "faster", "why not",
# "quality", "important", "critical" — is refused BY NAME, because those words
# say nothing about one-offness and are exactly what gets typed by reflex.
#
# WHAT IS NOT REFUSED: a written sentence that happens to contain one of those
# words. The check fires only when the reason is DOMINATED by the assertion —
# strip the matched words and fewer than three substantive words remain. "This
# is important" is refused; "super-critical one-off: the design system every
# screen inherits" is not. Judging a REAL reason's merit is not a machine's job,
# and a guard that tried would be refusing the founder's own greenlight.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the next
# session — it assumes nothing about being live in the session that adds it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
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
  hook: scripts/hooks/guard-model-ceiling.sh
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
    announce_off "MODEL COST CEILING IS OFF: python3 is not on PATH, so this spawn's model could not be read. The pick is UNCHECKED."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "MODEL COST CEILING IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether a spawn is above the declared cost ceiling."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
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
    unevaluated_or_continue "guard-model-ceiling.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this spawn resolves to a model above the declared cost ceiling"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${ALLOWED_MODELS:=fable opus sonnet haiku}"

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
[ "$TOOL_NAME" = "Agent" ] || exit 0

# --- Parse the spawn -------------------------------------------------------
# Newlines survive through a \001 placeholder so the ack marker can be matched
# with a line-start anchor, exactly as guard-worktree-isolation.sh does it.
PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    if not isinstance(ti, dict):
        raise ValueError("tool_input not an object")
    pr = str(ti.get("prompt", "") or "").replace("\t", " ").replace("\n", "\x01")
    print("OK\t%s\t%s\t%s\t%s\t%s" % (
        str(ti.get("subagent_type", "") or ""),
        str(ti.get("name", "") or ""),
        str(ti.get("model", "") or ""),
        str(d.get("session_id", "") or ""),
        pr,
    ))
except Exception:
    print("PARSEFAIL\t\t\t\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t\t\t\t')"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
SUBAGENT_TYPE="$(printf '%s' "$PARSED" | cut -f2)"
NAME="$(printf '%s' "$PARSED" | cut -f3)"
MODEL_OVERRIDE="$(printf '%s' "$PARSED" | cut -f4)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f5)"
PROMPT="$(printf '%s' "$PARSED" | cut -f6- | tr '\001' '\n')"

# FAIL OPEN on an unparseable payload, and say so. The opposite of
# guard-worktree-isolation.sh's choice on the same event, stated here rather
# than left as an inconsistency to be discovered: that guard fails closed
# because an unparseable spawn could BE the contract violation it exists to
# catch. This one cannot read the model OR the ack line from such a payload, so
# refusing would block a dispatch the operator has no way to permit.
if [ "$STATUS" = "PARSEFAIL" ]; then
    announce_off "MODEL COST CEILING: this Agent spawn could not be parsed, so its model was not checked and the 'model-ceiling-ack:' line could not be read either. This ONE spawn is UNCHECKED."
    exit 0
fi

# --- The ceiling, as data --------------------------------------------------
CEILING="${MODEL_CEILING:-}"
if [ -z "$(printf '%s' "$CEILING" | tr -d '[:space:]')" ]; then
    announce_off "MODEL COST CEILING IS NOT DECLARED in ${CONFIG}: no MODEL_CEILING key, so NOTHING is checking whether a spawn is on a more expensive tier than this repository intends to spend. Declare it beside MODEL_TIERS, e.g. MODEL_CEILING=\"opus\" — the normal ceiling for critical work, with the tier above it reserved for super-critical work and extreme one-off cases."
    exit 0
fi

MODEL_TIERS_LIB="$SCRIPT_DIR/../lib/model-tiers.sh"
if [ ! -f "$MODEL_TIERS_LIB" ]; then
    announce_off "MODEL COST CEILING IS OFF: scripts/lib/model-tiers.sh is missing at ${MODEL_TIERS_LIB}, so a declared ceiling of '${CEILING}' cannot be ranked against anything. Spawns are UNCHECKED."
    exit 0
fi
# shellcheck source=../lib/model-tiers.sh
. "$MODEL_TIERS_LIB"

SPEC_PROBLEM="$(model_tiers_problem "${MODEL_TIERS:-}")"
if [ -n "$SPEC_PROBLEM" ]; then
    announce_off "MODEL COST CEILING IS OFF: MODEL_TIERS in ${CONFIG} is unusable (${SPEC_PROBLEM}), so no model can be ranked and a ceiling of '${CEILING}' means nothing. Spawns are UNCHECKED."
    exit 0
fi

CEILING_RANK="$(model_tier_rank "$CEILING" "${MODEL_TIERS:-}")"
if [ -z "$CEILING_RANK" ]; then
    announce_off "MODEL COST CEILING IS OFF: MODEL_CEILING=\"${CEILING}\" names a model that MODEL_TIERS=\"${MODEL_TIERS}\" ranks nowhere (both live in ${CONFIG}). An unrankable ceiling is not a ceiling. Spawns are UNCHECKED."
    exit 0
fi

# --- The model this spawn boots on, from the ONE resolver ------------------
_RM_LIB="$SCRIPT_DIR/../lib/resolve-model.sh"
if [ ! -f "$_RM_LIB" ]; then
    announce_off "MODEL COST CEILING IS OFF: scripts/lib/resolve-model.sh is missing at ${_RM_LIB}, so the model this spawn boots on cannot be resolved. Spawns are UNCHECKED. (guard-worktree-isolation.sh refuses outright without the same file — a clean run and an absent gate must never look the same.)"
    exit 0
fi
# shellcheck source=../lib/resolve-model.sh
. "$_RM_LIB"

RESOLVED="$(resolve_expected_model "$MODEL_OVERRIDE" "$SUBAGENT_TYPE")"

if [ "$RESOLVED" = "UNRESOLVABLE" ]; then
    announce_off "MODEL COST CEILING IS OFF for this spawn: subagent_type '${SUBAGENT_TYPE}' is namespaced and no definition for it was found under ${ENTITY_ROOT}/.claude/agents/, the engine's own roster, or AGENT_NAMESPACE_ROOTS — so the model it boots on is unknown and the ceiling could not be applied. Fix: add '<namespace>=<plugin root>' to AGENT_NAMESPACE_ROOTS in orchestration.config."
    exit 0
fi

# UNDETERMINABLE: no override (or model:"inherit") AND no live definition
# default. The instance boots on the session's model — there is no pick to
# judge. SILENT by design; see the fail-open table in the header.
[ -n "$RESOLVED" ] || exit 0

RESOLVED_RANK="$(model_tier_rank "$RESOLVED" "${MODEL_TIERS:-}")"
if [ -z "$RESOLVED_RANK" ]; then
    announce_off "MODEL COST CEILING IS OFF for this spawn: '${NAME:-<unset>}' of '${SUBAGENT_TYPE:-<unset>}' resolves to model '${RESOLVED}', which MODEL_TIERS=\"${MODEL_TIERS}\" ranks nowhere. An unrankable model cannot be compared to the ceiling '${CEILING}', so this pick is UNCHECKED. Re-derive MODEL_TIERS in ${CONFIG} so it ranks every alias this harness offers."
    exit 0
fi

# AT OR BELOW THE CEILING -> SILENT, always. A ceiling that comments on the
# normal case is a nag, and a nag is how a guard becomes something to disable.
# (Rank 1 is the MOST capable tier, so a LOWER number is a HIGHER tier.)
[ "$RESOLVED_RANK" -lt "$CEILING_RANK" ] || exit 0

# --- Above the ceiling: acknowledged, or refused ---------------------------
ACK_MARKER="model-ceiling-ack"

# ONE extraction, not a test followed by an extraction — two greps with the
# same pattern are two chances to relax one of them and not the other. The
# REASON is required by the pattern itself: at least one non-blank character
# after the marker, so a bare `model-ceiling-ack:` extracts to nothing.
ACK_REASON="$(printf '%s' "$PROMPT" \
    | grep -E "^[[:space:]]*${ACK_MARKER}:[[:space:]]*[^[:space:]]" \
    | head -1 \
    | sed -E "s/^[[:space:]]*${ACK_MARKER}:[[:space:]]*//" || true)"

# _ceiling_reason_problem <reason> — echo NOTHING when the reason is a
# well-formed justification; echo the refusal reason otherwise. Kept as a
# function (not an inline $(...) heredoc) because bash will not parse a quoted
# heredoc containing an apostrophe inside a command substitution. The length
# floors are the staffing gate's, deliberately: one engine, one idea of what a
# reason looks like.
_ceiling_reason_problem() {
  MC_REASON="$1" python3 - <<'PY'
import os, re
r = (os.environ.get("MC_REASON", "") or "").strip()
MIN_CHARS = 30
MIN_WORDS = 5
MIN_CONTENT = 3
STOP = {
    "the","and","for","that","this","with","have","has","had","been","from",
    "just","only","need","needs","needed","want","wants","because","none",
    "null","reason","tbd","todo","fine","okay","yes","not","but","was","were",
    "are","its","here","there","thing","things","stuff","some","any","all",
    "does","doesnt","dont","cant","will","would","should","could","which",
    "them","they","their","when","what","also","into","over","than","then",
    "very","really","quite","sure","done","doing","make","made","use","used",
    "using","work","works","working","task","agent","one","two","model",
    "models","spawn","tier","pick",
}
# The reflex list. These assert MERIT and say nothing about one-offness, which
# is the only question the ceiling asks. Named here the way the staffing gate
# names speed and convenience.
REFLEX = re.compile(
    r"(most\s+capable|more\s+capable|top\s+model|best\s+model|best\b|"
    r"better\b|fastest|faster|quicker|why\s+not|quality|important\w*|"
    r"critical\w*|powerful|strongest|smartest|cleverest)", re.I)
if not r:
    print("no 'model-ceiling-ack: <reason>' line is present in the prompt.")
else:
    words = re.findall(r"[A-Za-z][A-Za-z'-]*", r)
    content = {w.lower() for w in words if len(w) >= 4 and w.lower() not in STOP}
    hits = REFLEX.search(r)
    # DOMINANCE, not mere presence: strip every word the reflex list matched and
    # ask what is left. A written sentence that happens to contain "quality"
    # survives; a reason that IS "the most capable model" does not.
    matched_words = set()
    for m in REFLEX.finditer(r):
        for w in re.findall(r"[A-Za-z][A-Za-z'-]*", m.group(0)):
            matched_words.add(w.lower())
    residue = {w for w in content if w not in matched_words}
    if len(r) < MIN_CHARS:
        print("the reason given is %d character(s) long; a real justification needs "
              "at least %d. A bare or token marker exempts nothing."
              % (len(r), MIN_CHARS))
    elif len(words) < MIN_WORDS:
        print("the reason given is %d word(s) long; a real justification needs at "
              "least %d. A bare or token marker exempts nothing."
              % (len(words), MIN_WORDS))
    elif len(content) < MIN_CONTENT:
        print("the reason given carries %d substantive word(s) (needs %d) — it "
              "reads as filler, not a justification." % (len(content), MIN_CONTENT))
    elif hits and len(residue) < MIN_CONTENT:
        print("the reason given is an ASSERTION OF MERIT (%r) and nothing else. "
              "That the top tier is stronger is not in dispute and is not the "
              "question: the ceiling asks whether this is a ONE-OFF whose output "
              "everything downstream inherits. Say what makes it one."
              % hits.group(0))
    else:
        print("")
PY
}

ACK_WHY="$(_ceiling_reason_problem "$ACK_REASON" 2>/dev/null || printf 'the justification could not be evaluated.')"

if [ -z "$ACK_WHY" ]; then
    # Acknowledged. Best-effort log — never fail a spawn because logging failed;
    # the line in the prompt is itself the audit trail.
    LOG_DIR="$ENTITY_ROOT/.claude/state"
    mkdir -p "$LOG_DIR" 2>/dev/null || true
    {
        printf '%s\tsession=%s\tagent=%s\tname=%s\tmodel=%s\ttier=%s\tceiling=%s\t%s: %s\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            "${SESSION_ID:-<unset>}" \
            "${SUBAGENT_TYPE:-<unset>}" \
            "${NAME:-<unset>}" \
            "$RESOLVED" \
            "$RESOLVED_RANK" \
            "$CEILING" \
            "$ACK_MARKER" \
            "$ACK_REASON"
    } >>"$LOG_DIR/model-ceiling-acks.log" 2>/dev/null || true
    exit 0
fi

# --- REFUSE — and CARRY the rule, do not point at it ------------------------
if [ -n "$MODEL_OVERRIDE" ]; then
    MODEL_SRC="the explicit model override '${MODEL_OVERRIDE}' on this spawn"
    RECONSIDER="change the spawn to  model: \"${CEILING}\""
else
    MODEL_SRC="the '${SUBAGENT_TYPE}' agent definition's own model: frontmatter"
    RECONSIDER="override this spawn with  model: \"${CEILING}\""
fi
NAME_ROLE="${NAME%%-*}"
NAME_ID="${NAME##*-}"
if [ -n "$NAME" ] && [ "$NAME_ROLE" != "$NAME" ]; then
    RENAME="${NAME_ROLE}-${CEILING}-${NAME_ID}"
else
    RENAME="<role>-${CEILING}-<identifier>"
fi

{
    echo "=== MODEL COST CEILING — THIS SPAWN IS ABOVE IT ==="
    echo "  spawn      : ${NAME:-<unset>} of '${SUBAGENT_TYPE:-<unset>}'"
    echo "  requested  : ${RESOLVED} (tier ${RESOLVED_RANK}) — from ${MODEL_SRC}"
    echo "  the ceiling: ${CEILING} (tier ${CEILING_RANK}) — MODEL_CEILING in orchestration.config"
    echo ""
    echo "  THE CEILING, IN ONE LINE: the normal ceiling for CRITICAL work is"
    echo "  '${CEILING}'. The tier above it is for SUPER-CRITICAL work and extreme"
    echo "  ONE-OFF cases — it is the most capable seat AND roughly twice the price,"
    echo "  and it burns the subscription quota much faster. Capability and cost are"
    echo "  two different orders; MODEL_TIERS declares only the first one."
    echo ""
    echo "  THE TEST — not 'is this important', but: IS THIS A ONE-OFF WHOSE OUTPUT"
    echo "  EVERYTHING DOWNSTREAM INHERITS? How far does a mistake propagate — one"
    echo "  failed task, or every task built on top of it?"
    echo ""
    echo "  THE FOUNDER'S OWN EXAMPLES, as calibration rather than a closed list:"
    echo "    - the worktree-lifecycle work and its fixes — a defect class that has"
    echo "      survived seven designs, where a wrong fix becomes permanent"
    echo "    - unique or super-premium front-end design"
    echo "    - creating a design system"
    echo "  'One-off' is load-bearing: the exception is granted once, per instance."
    echo "  A respawned teammate does not keep the seat because its predecessor had it."
    echo ""
    echo "  RECONSIDER (the usual answer): ${RECONSIDER}"
    echo "  and rename the teammate '${RENAME}' — the <model> token must stay"
    echo "  truthful to the model the instance actually boots on, so the spawn guard"
    echo "  will refuse '${NAME:-<name>}' on '${CEILING}'."
    echo ""
    echo "  OR ACKNOWLEDGE, if the pick stands. Add ONE line to this spawn prompt:"
    echo ""
    echo "      ${ACK_MARKER}: <why this is a one-off whose output everything downstream inherits>"
    echo ""
    # Only when a line was actually attempted. "No line is present" under an
    # instruction that just told you to add one is noise, and the reason a
    # REJECTED line was rejected is the most useful sentence in this whole
    # refusal — so it is printed where it cannot be mistaken for boilerplate.
    if [ -n "$ACK_REASON" ]; then
        echo "  THE LINE YOU GAVE WAS NOT ACCEPTED: ${ACK_WHY}"
        echo ""
    fi
    echo "  The founder greenlighting this tier in conversation IS a good reason —"
    echo "  write that as the reason and the log preserves that he said it. Accepted"
    echo "  uses are appended to .claude/state/model-ceiling-acks.log, so a habit of"
    echo "  waiving is visible rather than invisible."
    echo "(hook: scripts/hooks/guard-model-ceiling.sh)"
} >&2
exit 2
