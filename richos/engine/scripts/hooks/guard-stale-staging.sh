#!/usr/bin/env bash
#
# guard-stale-staging.sh — BLOCKING PreToolUse guard on the Agent tool.
#
# REFUSES A DISPATCH THAT WILL WORK ON, OR TEST AGAINST, A PRODUCT TREE WHOSE
# LANDED COMMITS HAVE NOT REACHED STAGING.
#
# ===========================================================================
# THE FAILURE THIS EXISTS TO PREVENT
# ===========================================================================
# The adopting repository's own rule, in its words:
#
#     "Staging MUST be brought current BEFORE the next `avelor/` or `fitapp/`
#      work or QA run, whatever landed in between. That is where the protection
#      actually lives, and it is a precondition of resuming product work rather
#      than a per-land chore."
#
# That is a paragraph, and a paragraph is a promise. Nothing enforced it. The
# first product QA run after a gap meets a staging that is weeks stale, and the
# only thing between that and a false QA verdict is somebody remembering a
# sentence. A rule enforced by attention lasts exactly as long as the attention
# — the same finding this engine's row-currency contract was built from.
#
# ===========================================================================
# THE HALF THAT MATTERS MORE: WHAT THIS MUST *NOT* DO
# ===========================================================================
# The same ruling that created the obligation also bounded it, on 2026-09-02,
# after a failed deploy was reported to the founder FIVE TIMES in a day during
# which 35 commits landed and ZERO touched product code:
#
#     "A land touching only `scripts/`, `docs/`, `.claude/`, `skills/` or the
#      record still deploys when deploy is working. But a deploy failure on
#      such a land is RECORDED, NEVER REPORTED — nothing shipped and nothing is
#      at risk."
#
# A guard that cannot tell those two cases apart fires on every docs land, gets
# waived on the day, and habitual waiving is how a guard dies. This engine has
# three recorded instances of that shape in a single day.
#
# SO THE REFUSAL IS A CONJUNCTION OF THREE FACTS, AND ALL THREE MUST HOLD:
#
#   1. There is a RECORD of what staging has, and it is behind main.
#   2. The commits between the record and main TOUCH A DECLARED PRODUCT TREE.
#      This is the whole of the 2026-09-02 amendment, mechanized. A staging 35
#      commits behind on docs is SILENT — not tolerated, not warned about,
#      silent — because nothing shipped and nothing is at risk.
#   3. THIS dispatch is product work or a run against staging.
#
# Fact 2 is computed from git and is the one that keeps the guard alive: in the
# adopting repository at the time this was written, `avelor/` had not changed
# since 2026-07-28 and `fitapp/` since 2026-05-05, so the guard is structurally
# incapable of firing until product work resumes — which is the moment the rule
# exists for. Fact 3's prose classifier is deliberately allowed to be a little
# broad BECAUSE fact 2 gates it: an over-broad match can only produce a refusal
# inside the window between a product land and its deploy, and inside that
# window "you mentioned avelor/ and testing" is the RIGHT time to be stopped.
#
# ===========================================================================
# WHY PreToolUse[Agent], AND NOT THE LAND
# ===========================================================================
# The row that asked for this offered two chokepoints — the land, or the
# dispatch. The ruling picks for us: the obligation is "a precondition of
# resuming product work rather than a per-land chore". Refusing the LAND would
# also be backwards, because the land is both what creates the debt and what
# discharges it (deploy is the land's last step); a guard there would wedge the
# very command that fixes it.
#
# Rejected alternatives, argued rather than left to be re-derived:
#
#   PreToolUse[Bash] on the test command — REJECTED. It would have to match
#     every shape of "run something against staging", a large and open class,
#     and it fires after the dispatch that already committed the work.
#   A Stop-hook notice — REJECTED. It reports a QA run that already happened
#     against a stale environment, which is the verdict this exists to prevent.
#   A checklist in the land skill — REJECTED for the reason the paragraph it
#     replaces failed: enforcement by attention.
#
# ===========================================================================
# THE RECORD — WHY A LOCAL FILE, AND WHAT ITS ABSENCE MEANS
# ===========================================================================
# "What does staging have?" has exactly one trustworthy answer and it is not a
# network call. The founder's standing ruling (11.4) is that the Railway login
# is restored when product work resumes and NOT before, so a guard that phoned
# the platform would fail at precisely the moment it matters. A PreToolUse hook
# also must not spend seconds on HTTP.
#
# So the guard reads a LOCAL, DURABLE record that the deploy writes:
#
#     <entity root>/.claude/state/staging-deployed     (STAGING_DEPLOY_RECORD)
#
#         sha=8f3a1c2b9d40
#         outcome=success
#         tree=avelor
#         at=2026-09-06T10:11:12Z
#
# New records are stored separately at <record>.trees/<tree> (nested tree
# slashes are encoded as %2F). A legacy record with tree= names only that tree;
# an unscoped legacy record is usable only with one declared product tree.
# Deploying one product never certifies another product.
#
# Minted by scripts/staging-record.sh, which the deploy script calls once its
# own freshness gate has passed — never before, so a record can never claim a
# deploy that did not finish. `outcome=` anything but `success` is read as NO
# RECORD, deliberately: "the deploy ran and failed" and "staging is current"
# must never produce the same verdict.
#
# ADOPTION COSTS NOTHING ON THE DAY IT HAPPENS, and that is a design decision
# taken from measurement rather than taste. On 2026-09-06 the sibling headline
# contract was measured on the real record: declaring its jurisdiction produced
# ZERO refusals and 26 rows named, and its teeth were a SECOND declared key.
# The same two-key shape is used here. Until a deploy has ever written a
# record, this guard ALLOWS and ANNOUNCES — and it announces only at the moment
# of a dispatch that would have been in scope, never on the 99% of dispatches
# that have nothing to do with the product. Declare STAGING_RECORD_REQUIRED=1
# when the deploy is wired and the absence of a record should itself refuse.
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY — but never on the quiet path
# ===========================================================================
#   NOT ADOPTED (no orchestration.config)   -> STAND DOWN, silent.
#   STAGING_TREES NOT DECLARED              -> STAND DOWN, SILENT. This is the
#     adoption switch and its absence is silent, unlike the model ceiling's:
#     most repositories have no staging at all, and a per-spawn announcement in
#     every one of them is noise rather than protection.
#   NO DEBT (record current, or nothing
#   product-shaped between it and main)     -> SILENT. Always. See fact 2.
#   NOT AN IN-SCOPE DISPATCH                -> SILENT.
#   RECORD MISSING / MALFORMED / FAILED,
#   on an IN-SCOPE dispatch                 -> ALLOW + ANNOUNCE (or REFUSE if
#     STAGING_RECORD_REQUIRED=1).
#   GIT, PYTHON3, CONFIG OR ROOTS UNREADABLE-> ALLOW + ANNOUNCE. "I cannot
#     tell" is never "forbidden".
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing
# entirely. That is the shared bootstrap's contract and probe Layer R asserts
# every rooted hook carries it byte-identically.
#
# ===========================================================================
# THE UNEVALUATED-PAYLOAD DECLARATION, AND WHY IT LOOKS WRONG UNTIL YOU READ IT
# ===========================================================================
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — in a repository that has
# not declared STAGING_TREES this hook stands down at the adoption switch
# BEFORE it parses anything, so its verdict on an empty, truncated or non-JSON
# payload is the same exit 0 in the same silence as its verdict on a perfect
# one. Most repositories on a machine are in that state and none of them
# adopted this contract, so announcing there would be a nag about a gate they
# never asked for — and stale-staging.test.sh case 1d asserts the stronger
# thing, that an unadopted repository is not merely quiet but accumulates NO
# STATE, which a notice would violate by writing
# .claude/state/unevaluated-payloads.log into it.
#
# THE CLAIM IS CONDITIONAL AND THE CONDITION IS THE DECLARATION. Where
# STAGING_TREES IS declared this hook is emphatically payload-DEPENDENT, and it
# wires scripts/lib/unevaluated-notice.sh a few lines below exactly like its
# siblings — an adopted repository hears about a call this guard could not
# read. So read the marker above as "payload-independent while stood down",
# which is the only state unevaluated-payload.test.sh can observe from a
# repository that has not adopted the contract, and which that suite verifies
# empirically rather than on trust: it drives all four payloads and requires
# the output to be identical.
#
# ===========================================================================
# THE ESCAPE HATCH — a live prompt line, with a reason, logged
# ===========================================================================
#     stale-staging-ack: <reason>
#
# anywhere on its own line in the Agent spawn prompt. It permits that ONE
# dispatch and appends to <entity root>/.claude/state/stale-staging-acks.log.
# Same idiom as `model-ceiling-ack:`, `main-checkout-run:`, `resume-ack:` and
# `data-contract-bypass:`. A bare marker exempts nothing.
#
# There IS an escape hatch here, unlike the row-currency contract's deliberate
# refusal to have one, and the difference is real: row currency's judgment ("I
# will fix the row after the deploy") fails at the moment of the land, so an
# override would have been used every time. This guard's legitimate exception
# is a different thing entirely — a dispatch that names a product path while
# genuinely not testing against staging (reading the code, auditing a diff,
# writing a plan). Refusing those with no way through would make the guard a
# nuisance on exactly the tasks that are safe, which is how a guard gets
# routed around instead of used.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds it.

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
  hook: scripts/hooks/guard-stale-staging.sh
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
    # SILENT on the tool-missing path is wrong, but so is announcing in every
    # repository on the machine. Only repositories that ADOPTED this have
    # anything to lose, and adoption cannot be read without python3 — so this
    # says so once, on stderr, and allows.
    announce_off "STALE-STAGING GUARD IS OFF: python3 is not on PATH, so this dispatch was not checked against the staging deploy record."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "STALE-STAGING GUARD IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether a product dispatch is about to run against a stale staging."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"

# --- THE ADOPTION SWITCH ---------------------------------------------------
# Absent or blank -> this repository has no staging this guard knows about.
# SILENT, and before anything else is read: a repository that never adopted
# must cost nothing, not even a payload parse.
STAGING_TREES="${STAGING_TREES:-}"
[ -n "$(printf '%s' "$STAGING_TREES" | tr -d '[:space:]')" ] || exit 0

: "${STAGING_DEPLOY_RECORD:=.claude/state/staging-deployed}"
: "${STAGING_DEPLOY_COMMAND:=}"
: "${STAGING_RECORD_REQUIRED:=0}"
: "${STAGING_MAIN_REF:=refs/heads/main}"
# The two halves of "is THIS dispatch in scope". Overridable, because an
# adopter's vocabulary is theirs; defaulted, because a key nobody sets is a
# check nobody gets.
: "${STAGING_TRIGGER_RE:=(\btests?\b|\btesting\b|\baudit\b|\bQA\b|\bverif(y|ies|ication)\b|\brender\b|\bscreenshot\b|\bstaging\b|\be2e\b|playwright|install-fresh|\bregression\b|\bimplement\b|\bfix\b|\bbuild\b)}"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes, which is why 17 of 25
# PreToolUse guards were measured passing a call in complete silence on
# 2026-09-05. This separates the two. NO VERDICT CHANGES — only the silence.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-stale-staging.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this dispatch works on or tests against a product tree whose landed commits have not reached staging"
fi

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
[ "$TOOL_NAME" = "Agent" ] || exit 0

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
        str(ti.get("subagent_type", "") or ""),
        str(ti.get("name", "") or ""),
        str(d.get("session_id", "") or ""),
        pr,
    ))
except Exception:
    print("PARSEFAIL\t\t\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t\t\t')"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
SUBAGENT_TYPE="$(printf '%s' "$PARSED" | cut -f2)"
NAME="$(printf '%s' "$PARSED" | cut -f3)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f4)"
PROMPT="$(printf '%s' "$PARSED" | cut -f5- | tr '\001' '\n')"

# FAIL OPEN on an unparseable payload, and say so. Same choice as
# guard-model-ceiling.sh and for the same reason: this guard cannot read the
# prompt OR the ack line from such a payload, so refusing would block a
# dispatch the operator has no way to permit.
if [ "$STATUS" = "PARSEFAIL" ]; then
    announce_off "STALE-STAGING GUARD: this Agent spawn could not be parsed, so it was not checked against the staging deploy record and the 'stale-staging-ack:' line could not be read either. This ONE dispatch is UNCHECKED."
    exit 0
fi

# --- IS THIS DISPATCH IN SCOPE? --------------------------------------------
# BOTH halves, never one: a product path AND something that works on or runs
# against it. `avelor/` appears in a great deal of this project's prose — a
# row of the working record, a brief quoting that row — and prose is not a
# dispatch. The path half is built from the DECLARATION so an adopter's trees
# are the adopter's.
SCOPE_ALT="$(python3 -c 'import re,sys; print("|".join(re.escape(t) for t in sys.argv[1].split()))' "$STAGING_TREES")"
SCOPE_RE="(^|[^A-Za-z0-9_.-])(${SCOPE_ALT})/"

in_scope() {
    grep -qE "$SCOPE_RE" <<<"$PROMPT" || return 1
    grep -qiE "$STAGING_TRIGGER_RE" <<<"$PROMPT" || return 1
    return 0
}

# --- THE DEBT --------------------------------------------------------------
# Computed against the MAIN CHECKOUT, from any invocation location: an agent's
# session cwd is usually a linked worktree, and a worktree's HEAD is a proposal
# rather than what landed.
MAIN_ROOT="$ENTITY_ROOT"
_MC_LIB="$SCRIPT_DIR/../lib/resolve-main-checkout.sh"
if [ -f "$_MC_LIB" ]; then
    # shellcheck source=../lib/resolve-main-checkout.sh
    . "$_MC_LIB"
    MAIN_ROOT="$(resolve_main_checkout "$ENTITY_ROOT" "$ENTITY_ROOT" 2>/dev/null || printf '%s' "$ENTITY_ROOT")"
fi

RECORD_PATH="$STAGING_DEPLOY_RECORD"
case "$RECORD_PATH" in
    /*) : ;;
    *)  RECORD_PATH="$ENTITY_ROOT/$RECORD_PATH" ;;
esac

# read_record — sets DEPLOYED and RECORD_PROBLEM. NOT a function that echoes
# its answer: `X="$(read_record)"` runs it in a subshell, so the REASON a record
# was rejected would be discarded and every rejection would print the same
# generic "no record exists". A guard that cannot say WHY it is unsure is the
# shape this engine keeps finding in itself — 'worked' and 'never ran' reported
# identically.
DEPLOYED=""
RECORD_PROBLEM=""
read_record() {
    if [ ! -f "$RECORD_PATH" ]; then
        RECORD_PROBLEM="no staging deploy record exists at ${RECORD_PATH}"
        return 1
    fi
    local sha outcome recorded_tree
    sha="$(grep -E '^[[:space:]]*sha[[:space:]]*=' "$RECORD_PATH" 2>/dev/null | head -1 \
           | sed -E 's/^[[:space:]]*sha[[:space:]]*=[[:space:]]*//; s/[[:space:]]*$//' || true)"
    outcome="$(grep -E '^[[:space:]]*outcome[[:space:]]*=' "$RECORD_PATH" 2>/dev/null | head -1 \
           | sed -E 's/^[[:space:]]*outcome[[:space:]]*=[[:space:]]*//; s/[[:space:]]*$//' || true)"
    recorded_tree="$(sed -n 's/^[[:space:]]*tree[[:space:]]*=[[:space:]]*//p' "$RECORD_PATH" | sed -n '1p')"
    if [ "$recorded_tree" != "$CHECK_TREE" ] && { [ -n "$recorded_tree" ] || [ "$TREE_COUNT" -ne 1 ]; }; then
        RECORD_PROBLEM="the record at ${RECORD_PATH} does not identify a deployment of ${CHECK_TREE}"
        return 1
    fi
    if ! printf '%s' "$sha" | grep -qE '^[0-9a-f]{7,40}$'; then
        RECORD_PROBLEM="the record at ${RECORD_PATH} carries no readable 'sha=' line, so what staging is running cannot be read from it"
        return 1
    fi
    # A record with no outcome at all is read as a success record: the field is
    # an ESCALATION of precision, and an adopter whose writer predates it must
    # not be told its staging is unknown. A record that states a NON-success
    # outcome is read as no record — "the deploy ran and failed" and "staging
    # is current" must never produce the same verdict.
    if [ -n "$outcome" ] && [ "$outcome" != "success" ]; then
        RECORD_PROBLEM="the record at ${RECORD_PATH} states outcome='${outcome}', so the last deploy did not succeed and what staging carries is unknown"
        return 1
    fi
    DEPLOYED="$sha"
    return 0
}

if ! command -v git >/dev/null 2>&1; then
    in_scope || exit 0
    announce_off "STALE-STAGING GUARD IS OFF: git is not on PATH, so the commits between the deploy record (${DEPLOYED}) and main could not be listed. This dispatch is UNCHECKED."
    exit 0
fi

MAIN_TIP="$(git -C "$MAIN_ROOT" rev-parse --verify --quiet "$STAGING_MAIN_REF" 2>/dev/null || true)"
if [ -z "$MAIN_TIP" ]; then
    in_scope || exit 0
    announce_off "STALE-STAGING GUARD IS OFF: '${STAGING_MAIN_REF}' does not resolve in ${MAIN_ROOT}, so there is nothing to compare the deploy record against. This dispatch is UNCHECKED. (STAGING_MAIN_REF in ${CONFIG} names the branch that lands.)"
    exit 0
fi

# Evaluate each named product against its own deployment. Unknown evidence is
# retained while checking the other products, so it cannot hide known stale work.
RECORD_BASE="$RECORD_PATH"
TREE_COUNT="$(printf '%s\n' "$STAGING_TREES" | awk '{ n += NF } END { print n }')"
UNKNOWN_RECORD=""
UNDEPLOYED=0
NAMED_TREES=""
for tree in $STAGING_TREES; do
    TREE_RE="$(python3 -c 'import re,sys; print("(^|[^A-Za-z0-9_.-])" + re.escape(sys.argv[1]) + "/")' "$tree")"
    if grep -qE "$TREE_RE" <<<"$PROMPT"; then NAMED_TREES="$NAMED_TREES $tree"; fi
done
# With no named tree, leave jurisdiction to in_scope rather than treating an
# unevaluated scope as evidence that staging is current.
for CHECK_TREE in ${NAMED_TREES:-$STAGING_TREES}; do
    # Do not require a legacy product's deployment for a dispatch about another
    # product. The outer scope test still requires the work/QA half as well.
    DEPLOYED=""
    RECORD_PROBLEM=""
    RECORD_PATH="$RECORD_BASE.trees/$(printf '%s' "$CHECK_TREE" | sed 's|/|%2F|g')"
    [ -f "$RECORD_PATH" ] || RECORD_PATH="$RECORD_BASE"
    read_record || true
    if [ -z "$DEPLOYED" ]; then
        UNKNOWN_RECORD="${UNKNOWN_RECORD}${UNKNOWN_RECORD:+; }${RECORD_PROBLEM}"
        continue
    fi
    if ! git -C "$MAIN_ROOT" cat-file -e "${DEPLOYED}^{commit}" 2>/dev/null; then
        UNKNOWN_RECORD="the deploy record names commit ${DEPLOYED}, which does not exist in ${MAIN_ROOT}"
        continue
    fi
    if ! git -C "$MAIN_ROOT" merge-base --is-ancestor "$DEPLOYED" "$MAIN_TIP" 2>/dev/null; then
        UNKNOWN_RECORD="the ${CHECK_TREE} deployment ${DEPLOYED} is not an ancestor of ${STAGING_MAIN_REF}; it cannot certify the landed product"
        continue
    fi
    UNDEPLOYED="$(git -C "$MAIN_ROOT" rev-list --count "${DEPLOYED}..${MAIN_TIP}" -- "$CHECK_TREE" 2>/dev/null || printf 'ERR')"
    if [ "$UNDEPLOYED" = "ERR" ]; then
        UNKNOWN_RECORD="could not list commits between ${DEPLOYED} and ${STAGING_MAIN_REF} in ${MAIN_ROOT}"
        UNDEPLOYED=0
        continue
    fi
    if [ "$UNDEPLOYED" -gt 0 ]; then
        # The first stale product is enough to refuse this dispatch. Keep its
        # exact identity and pathspec for the diagnostic and acknowledgement.
        STAGING_TREES="$CHECK_TREE"
        UNKNOWN_RECORD=""
        break
    fi
done

acknowledged() {
    # --- Acknowledged? ---------------------------------------------------------
    ACK_MARKER="stale-staging-ack"
    ACK_REASON="$(printf '%s' "$PROMPT" \
        | grep -E "^[[:space:]]*${ACK_MARKER}:[[:space:]]*[^[:space:]]" \
        | sed -n '1p' \
        | sed -E "s/^[[:space:]]*${ACK_MARKER}:[[:space:]]*//" || true)"

    # The length floors are the staffing gate's, deliberately: one engine, one idea
    # of what a reason looks like. A bare marker exempts nothing.
    ACK_WHY=""
    if [ -z "$ACK_REASON" ]; then
        ACK_WHY="no 'stale-staging-ack: <reason>' line is present in the prompt."
    elif [ "${#ACK_REASON}" -lt 30 ]; then
        ACK_WHY="the reason given is ${#ACK_REASON} character(s) long; a real justification needs at least 30. A bare or token marker exempts nothing."
    fi

    if [ -z "$ACK_WHY" ]; then
        LOG_DIR="$ENTITY_ROOT/.claude/state"
        mkdir -p "$LOG_DIR" 2>/dev/null || true
        {
            printf '%s\tsession=%s\tagent=%s\tname=%s\tdeployed=%s\tmain=%s\tundeployed=%s\t%s: %s\n' \
                "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
                "${SESSION_ID:-<unset>}" "${SUBAGENT_TYPE:-<unset>}" "${NAME:-<unset>}" \
                "$DEPLOYED" "$(printf '%s' "$MAIN_TIP" | cut -c1-12)" "$UNDEPLOYED" \
                "$ACK_MARKER" "$ACK_REASON"
        } >>"$LOG_DIR/stale-staging-acks.log" 2>/dev/null || true
        return 0
    fi
    return 1

}

if [ -n "$UNKNOWN_RECORD" ]; then
    # NO USABLE RECORD. Only speak if this dispatch would have been in scope —
    # a repository mid-adoption must not be nagged on every unrelated spawn.
    in_scope || exit 0
    if acknowledged; then exit 0; fi
    WHY="$UNKNOWN_RECORD"
    if [ "$STAGING_RECORD_REQUIRED" = "1" ]; then
        {
            echo "=== STALE STAGING — WHAT STAGING CARRIES IS UNKNOWN, AND THIS DISPATCH IS PRODUCT WORK ==="
            echo "  dispatch : ${NAME:-<unset>} of '${SUBAGENT_TYPE:-<unset>}'"
            echo "  problem  : ${WHY}"
            echo ""
            echo "  This repository declares STAGING_RECORD_REQUIRED=1, so an unknown"
            echo "  staging is refused rather than announced. Deploy, which writes the"
            echo "  record: ${STAGING_DEPLOY_COMMAND:-<your deploy script>}"
            echo ""
            echo "  OR, if this dispatch genuinely does not run against staging, add ONE"
            echo "  line to the spawn prompt:"
            echo ""
            echo "      stale-staging-ack: <why this dispatch cannot be fooled by a stale staging>"
            echo "(hook: scripts/hooks/guard-stale-staging.sh)"
        } >&2
        exit 2
    fi
    announce_off "STALE-STAGING GUARD CANNOT DECIDE: ${WHY}. This dispatch (${NAME:-<unset>}) names a product tree and a run against it, so a QA verdict from it could be about a stale staging. Have the deploy call scripts/staging-record.sh, then declare STAGING_RECORD_REQUIRED=1 in ${CONFIG} to make this a refusal."
    exit 0
fi

# NO PRODUCT DEBT -> SILENT, ALWAYS, however far behind staging is otherwise.
# This is the amendment: nothing shipped, nothing is at risk, and a guard that
# spoke here would be waived within the day.
[ "$UNDEPLOYED" -gt 0 ] 2>/dev/null || exit 0

# There IS product debt. Is this dispatch the moment the rule cares about?
in_scope || exit 0

if acknowledged; then exit 0; fi

# --- REFUSE — and CARRY the rule, do not point at it ------------------------
COMMITS="$(git -C "$MAIN_ROOT" log --oneline --no-decorate -5 "${DEPLOYED}..${MAIN_TIP}" -- $STAGING_TREES 2>/dev/null || true)"
TOUCHED="$(git -C "$MAIN_ROOT" diff --name-only "${DEPLOYED}..${MAIN_TIP}" -- $STAGING_TREES 2>/dev/null | head -5 || true)"

{
    echo "=== STALE STAGING — PRODUCT COMMITS HAVE LANDED AND HAVE NOT BEEN DEPLOYED ==="
    echo "  dispatch     : ${NAME:-<unset>} of '${SUBAGENT_TYPE:-<unset>}'"
    echo "  staging has  : ${DEPLOYED}   (${RECORD_PATH})"
    echo "  main is at   : $(printf '%s' "$MAIN_TIP" | cut -c1-12)   (${STAGING_MAIN_REF} in ${MAIN_ROOT})"
    echo "  undeployed   : ${UNDEPLOYED} commit(s) touching: ${STAGING_TREES}"
    echo ""
    if [ -n "$COMMITS" ]; then
        echo "  WHAT IS NOT ON STAGING (newest first, up to 5):"
        printf '%s\n' "$COMMITS" | sed 's/^/    /'
        echo ""
    fi
    if [ -n "$TOUCHED" ]; then
        echo "  FILES THAT DIFFER (up to 5):"
        printf '%s\n' "$TOUCHED" | sed 's/^/    /'
        echo ""
    fi
    echo "  THE RULE, IN ONE LINE: staging must be brought current BEFORE the next"
    echo "  product work or QA run, whatever landed in between. It is a precondition"
    echo "  of resuming product work, not a per-land chore — which is why you are"
    echo "  hearing it HERE, at the dispatch, and not at the land that created it."
    echo ""
    echo "  WHY THIS IS NOT NOISE: a land touching only scripts/, docs/, .claude/,"
    echo "  skills/ or the record is NEVER refused here, however far behind staging"
    echo "  is — nothing shipped and nothing is at risk. This dispatch is refused"
    echo "  because ${UNDEPLOYED} commit(s) DID touch ${STAGING_TREES}, and a QA verdict"
    echo "  taken against staging now would be a verdict about older code."
    echo ""
    echo "  DEPLOY (the usual answer): ${STAGING_DEPLOY_COMMAND:-<your staging deploy script>}"
    echo "  It writes the record this guard reads, so the refusal clears itself."
    echo ""
    echo "  OR ACKNOWLEDGE, if this dispatch genuinely cannot be fooled by a stale"
    echo "  staging — reading the code, auditing a diff, writing a plan. Add ONE line:"
    echo ""
    echo "      ${ACK_MARKER}: <why a stale staging cannot affect this dispatch's result>"
    echo ""
    if [ -n "$ACK_REASON" ]; then
        echo "  THE LINE YOU GAVE WAS NOT ACCEPTED: ${ACK_WHY}"
        echo ""
    fi
    echo "  Accepted uses are appended to .claude/state/stale-staging-acks.log, so a"
    echo "  habit of waiving is visible rather than invisible."
    echo "(hook: scripts/hooks/guard-stale-staging.sh)"
} >&2
exit 2
