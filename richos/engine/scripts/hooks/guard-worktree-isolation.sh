#!/usr/bin/env bash
#
# guard-worktree-isolation.sh — PreToolUse guard (Agent), FIRST in the chain.
#
# HARD, PROMPT-INDEPENDENT enforcement of the teammate-spawn contract. Every
# teammate that can edit repo files MUST be spawned correctly, by STRUCTURE,
# not by the lead remembering to. This guard reads the spawn payload and BLOCKS
# the call unless the spawn is well-formed, so a correct spawn is the only spawn
# that survives.
#
# THE CONTRACT — enforced for every file-capable agent (i.e. every
# subagent_type NOT on the read-only allowlist, which is read from
# orchestration.config: READONLY_ALLOWLIST):
#   1. Native isolation:  isolation == "worktree" (or the isolated "remote")
#      -- OR -- a `cwd` inside a REGISTERED cross-repository worktree (clause
#      4, below)
#      -- OR -- an explicit "main-checkout-run:" marker line in the prompt (the
#      fallback escape hatch, see below).
#   2. A unique, well-formed, TRUTHFUL name:  "<role>-<model>-<identifier>"
#      (e.g. dev-sonnet-1, qa-opus-r3) — THREE dash-joined parts. Never bare
#      ("dev"), 2-part ("dev-1"), or run-together ("devsonnet1"). Required EVEN
#      WHEN the main-checkout-run marker is present — the marker does not relax
#      clause 2.
#        2a. The <model> token must be one of the harness's model aliases
#            (orchestration.config: ALLOWED_MODELS; default fable/opus/sonnet/haiku).
#        2b. TRUTHFULNESS: the <model> token must reflect the model the instance
#            ACTUALLY boots on. Expected model is resolved with this precedence:
#              (i)  an explicit tool_input.model override wins; else
#              (ii) the model: line in the YAML frontmatter of the LIVE agent
#                   definition .claude/agents/<subagent_type>.md.
#            LIVE agents only resolve — a non-live template under
#            .claude/agents/templates/ is never a spawnable type and is never
#            read here. If the expected model is determinable and the token
#            disagrees -> BLOCKED, naming BOTH values. If it is genuinely
#            undeterminable (no override AND no live-def default -- the
#            inherit-from-session case, or model:"inherit"), we cannot know the
#            boot model, so we require only that the token be a valid alias (2a)
#            and accept it -- a spawn is never failed on unknowable information.
#
# The "main-checkout-run:" marker is the FALLBACK escape hatch for a genuine
# one-off main-checkout run of ANY type. A prompt line beginning with
# "main-checkout-run:" followed by a short reason permits that one spawn to run
# without isolation. Every use is best-effort logged to
# .claude/state/main-checkout-runs.log so the opt-out stays auditable, never
# silent.
#
#   Without isolation, AND without the marker, AND not a read-only type
#   -> blocked, with the exact remediation spelled out inline.
#
# CLAUSE 3 — NAME REUSE IS STRUCTURALLY IMPOSSIBLE. A name that has EVER been
# used in this session's team — ACTIVE or COMPLETED — must never be reused.
# Latest-wins name shadowing and resumed-vs-respawned confusion both stem from
# names being reusable; this clause removes the ambiguity at the spawn boundary.
# There is NO escape hatch: identifiers are free (dev-1 -> dev-2 / dev-b /
# dev-0714), so a fresh name is always available.
#
#   Name-history sources (a UNION, so a past name can't slip through — see the
#   resolver for why it can't miss):
#     1. spawned-names.log — a session-scoped ledger. CHECKED here (this guard
#        reads it below), but APPENDED by the PostToolUse[Agent] partner hook,
#        scripts/hooks/detect-nonnative-worktree.sh, NOT by this PreToolUse
#        guard — see "BLOCKED-SPAWN NAME BURN" below. From the first spawn of a
#        session every name that actually EXECUTED is recorded (the can't-miss
#        primary source for real spawns); a spawn this guard itself blocks is
#        never recorded, because it never runs.
#     2. config.json members[] — every roster name, any status (the harness
#        keeps completed members with a terminal status rather than deleting).
#     3. idle-events.jsonl / task-events.jsonl teammate fields — durable
#        completion records that survive roster pruning.
#   Resolvable ONLY by EXACT session match (never a single-dir fallback). The
#   session teams dir is read from orchestration.config (SESSION_TEAMS_DIR, blank
#   -> platform default $HOME/.claude/teams); GUARD_ISOLATION_TEAMS_DIR overrides
#   it for tests. When no session team dir resolves (e.g. a bare unit-test
#   payload), the reuse clause is inert — a real session always has its team dir
#   (it auto-exists at startup), so production spawns are always covered.
#
#   HARNESS-VERSION NOTE: the members[]/status/cwd schema this clause reads is
#   harness-version-coupled (see orchestration.config). The clause fails OPEN on
#   any resolver error (never invents a collision from an IO glitch), so a schema
#   drift degrades to "no reuse check", never to a spurious block.
#
# BLOCKED-SPAWN NAME BURN.
#   This guard used to append NAME to spawned-names.log itself, inline, the
#   moment its own checks passed. But it runs FIRST in a multi-hook
#   PreToolUse[Agent] chain (this guard, then guard-definition-drift.sh,
#   verify-agent-prompt.sh, and the policy tail — see hooks/hooks.json),
#   and any LATER hook in that chain can still veto the same call. Result: a
#   spawn this guard approved but a later hook blocked burned its name anyway
#   — no teammate was ever created, yet the corrected retry under the same
#   name was refused as "reuse". Observed in production 2026-08-01, and
#   reproduced against this engine on 2026-08-28: hook 1 rc=0 and the ledger
#   gained the name; hook 4 rc=2; the corrected retry rc=2 "name reuse".
#
#   A name whose spawn never executed carries ZERO resume/shadowing risk, so
#   blocking it was pure friction, not safety.
#
#   Fix: the append now happens ONLY in the PostToolUse[Agent] partner hook
#   (scripts/hooks/detect-nonnative-worktree.sh), which — by construction —
#   never fires for a tool call a PreToolUse hook blocked. This guard still
#   performs the full CLAUSE 3 CHECK below, unchanged; it just no longer
#   performs the WRITE.
#
#   ACCEPTED RACE: two Agent calls issued in the very same in-flight turn with
#   an identical name are not caught against each other — neither hook sees
#   sibling calls still pending in the same batch, and there is no extension
#   point that inspects a whole in-flight tool-call batch. Narrow (distinct
#   identifiers are cheap) and self-healing (the second collides normally on
#   any subsequent attempt, once the first append has landed).
#
# CLAUSE 4 — CROSS-REPOSITORY WORK RUNS IN A WORKTREE RICHOS REGISTERED (2026-09-02).
#   Native isolation roots at the SESSION's repository; the Agent tool's own
#   escape hatch is `cwd`, "mutually exclusive with isolation: worktree". Until
#   now a cross-repository teammate improvised its worktree from a sentence in
#   its prompt ("git -C <repo> worktree add ..."), and nothing on disk said who
#   owned it — so the moment its native worktree was landed, the improvised
#   tree was permanently undecidable to the reaper. The rule is inverted:
#     4a. A `cwd` path must be the top level of a LINKED git worktree on a cc/
#         branch (docs/plans/worktree-spec-2026-09-11.md, point 1), never a
#         codex/ one (point 2). Anything else -> BLOCKED, naming the helper.
#         (A cwd-only spawn is then refused by clause 7f.)
#     4b. `cwd` together with isolation:"worktree" -> BLOCKED (the harness
#         refuses the pair; saying so here is cheaper than a failed spawn).
#     4c. Every `cross-repo-worktree: <path>` line in the prompt (the shape
#         that keeps native isolation in the session repo AND names the
#         cross-repo tree) must pass the same check -> else BLOCKED; whether it
#         was registered for THIS session and THIS teammate is clause 7's
#         question (the workspace registry, mega-lander/workspaces.py).
#     4d. A prompt that INSTRUCTS the teammate to run `git worktree add` is
#         BLOCKED: that is the improvisation this clause ends. The audited
#         escape hatch is a `hand-roll-ack: <reason>` prompt line, logged to
#         .claude/state/hand-roll-acks.log like main-checkout-run: is.
#   Clauses 1-3 are NOT relaxed by any of this: the name contract holds, and a
#   `cwd` spawn still needs a truthful <role>-<model>-<identifier> name.
#
# CLAUSE 5 — A GENERIC AGENT IS NOT A TEAMMATE (STAFFING, 2026-09-02).
#   READONLY_ALLOWLIST answers ONE question: does this agent need an isolated
#   worktree? It has never answered a second one: may delegated work be STAFFED
#   to this type at all? On 2026-09-02 those two were read as the same
#   permission — an engine-wide audit was dispatched to `Explore`, a generic
#   built-in, because a roster teammate would have needed a worktree created
#   first and the built-in could be dispatched immediately. Momentum over
#   correctness. Nothing refused it, correctly by the old contract. A comment
#   was added to orchestration.config saying the exemption is not a staffing
#   permission, and a comment is prose; prose does not hold.
#
#   So the staffing question now has its own gate, in front of the isolation
#   exemption and independent of it:
#     5a. The gate fires for a subagent_type that is on READONLY_ALLOWLIST or on
#         GENERIC_AGENT_TYPES (orchestration.config; default "general-purpose",
#         which closes the obvious detour — it is file-capable, so it passes the
#         whole contract today), MINUS anything on HARNESS_UTILITY_TYPES.
#     5b. HARNESS_UTILITY_TYPES (default: claude-code-guide statusline-setup
#         output-style-setup) is the declared exemption. Those types configure
#         or explain the harness itself; they never carry delegated project
#         work, so there is no roster teammate who should have had the job, and
#         demanding a justification for a statusline change is how a defense
#         becomes a nuisance and then a formality typed by reflex. The list is
#         CONFIG, not code, and the default is deny: a type newly added to
#         READONLY_ALLOWLIST needs the hatch until someone declares it a
#         utility.
#     5c. The escape hatch is one live prompt line, in the established shape:
#           generic-agent: <why no roster teammate fits this work>
#         A BARE MARKER EXEMPTS NOTHING. The reason must be >= 30 characters,
#         >= 5 words and carry >= 3 distinct substantive words, so "because" and
#         "n/a" are refused. It must also not be a SPEED or CONVENIENCE argument
#         ("faster", "saves time", "momentum", "convenient") — that was the
#         rationale in the incident, and it is the one reason this gate exists
#         to refuse. Every accepted use is logged to
#         .claude/state/generic-agent-dispatches.log, so a habit of waiving is
#         visible rather than invisible.
#     5d. The refusal NAMES THE ALTERNATIVE: pick the roster teammate whose
#         domain this is (the live roster is listed inline), and says why it
#         matters — a generic agent is invisible in the CEO's team display and
#         leaves no committed artifact, so work sent to one disappears from his
#         view and from the record.
#   CLAUSE 5 DOES NOT TOUCH THE ISOLATION EXEMPTION. An allowlisted type that
#   passes the staffing gate still exits with NO isolation and NO name required,
#   exactly as before. Both properties hold independently, and the suite proves
#   each without the other.
#
# CLAUSE 6 — A MOVE TO A LOWER CAPABILITY TIER IS STATED, NEVER SILENT (2026-09-02).
#   The doctrine said "don't downgrade" for weeks and never said which way down
#   was. On 2026-09-02 the orchestrator decided, out of nothing, that a spawn
#   of a Sonnet-default teammate on Fable was a downgrade, killed the correctly
#   configured teammate, told the CEO he was fixing an error that did not
#   exist, and commissioned a guard whose fixtures would have refused that
#   correct shape forever. It is an upgrade. A guard built on an invented
#   ordering does not stop the error; it makes it permanent and gives it
#   authority. So the ordering is DATA, and this clause reads nothing else:
#     6a. orchestration.config MODEL_TIERS declares the order ("fable opus >
#         sonnet > haiku": `>` separates tiers, leftmost is most capable,
#         aliases inside one tier are EQUAL). scripts/lib/model-tiers.sh is
#         the only parser. No consumer infers capability from an alias name.
#     6b. The clause fires ONLY when the spawn carries an explicit `model:`
#         override that is not "inherit" AND the definition's own model:
#         frontmatter default resolves. It compares the two RANKS.
#           lower tier than the default  -> REFUSED unless the prompt carries
#                                           a live  model-downgrade-ack: <reason>
#                                           line (a bare marker exempts
#                                           nothing); accepted uses are logged
#                                           to .claude/state/model-downgrade-acks.log
#           same or higher tier          -> SILENT, always. Upgrades were
#                                           never the defect.
#     6c. FAIL OPEN on its own error. A missing library, a blank or malformed
#         MODEL_TIERS, or an alias the declaration does not rank all SKIP the
#         clause for that spawn, with a notice on stderr and a systemMessage
#         naming what is broken — never a refusal. A crashing guard that
#         blocks every spawn is worse than the defect it was built for.
#     6d. The refusal names the teammate, the definition default, the
#         requested model, both ranks, the declaration, and the exact line to
#         add.
#   Clauses 1-5 are NOT relaxed by an accepted ack, and clause 6 never touches
#   a spawn with no override or with model:"inherit" (the boot model is then
#   the definition's own default, which is by definition not a downgrade).
#
# ALLOWED (exit 0):
#   - any non-Agent tool (passthrough)
#   - spawns of READ-ONLY agent types (they never write repo files) — the
#     allowlist below — with no ISOLATION or NAME requirement, once the
#     clause-5 staffing gate is satisfied.
#   - a file-capable spawn that satisfies BOTH contract clauses.
# BLOCKED (exit 2): every other Agent spawn — with the exact clause(s) it
# failed AND the exact fix, inline.
#
# FAIL-CLOSED: default-deny. Anything not on the read-only allowlist is treated
# as a file-capable teammate and must satisfy the full contract; an unparseable
# Agent payload is blocked, not waved through.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail
# Payloads can exceed a pipe buffer. Matching pipelines must consume EOF:
# grep -q or head can give the producer SIGPIPE and reverse the verdict.

# Fail-closed, not fail-open: every payload check below depends on python3 to
# parse the tool_input JSON. If python3 is missing, EVERY `python3 ... ||
# true`-guarded call below silently returns empty, and an empty parse used to
# read as "not an Agent spawn" / "not file-capable" — i.e. this guard would
# wave every spawn through unchecked. That contradicts the "FAIL-CLOSED:
# default-deny" contract documented above, so refuse outright instead.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-worktree-isolation.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-worktree-isolation.sh
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

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine, so there is no enforcement to
    # lose here. Stand down. NOT a silent skip: engine-status.sh announces the
    # stand-down into the orchestrator's own context at every session start.
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/guard-worktree-isolation.sh" >&2
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
    unevaluated_or_continue "guard-worktree-isolation.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this spawn is isolated in its own worktree, whether its name has been used before, and whether that name tells the truth about the model"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
# Default to Claude Code's built-in read-only agent types if unset.
: "${READONLY_ALLOWLIST:=Explore Plan claude-code-guide statusline-setup}"
# Staffing gate (clause 5). HARNESS_UTILITY_TYPES is the DECLARED exemption from
# it; GENERIC_AGENT_TYPES extends it to generic built-ins that are not on the
# read-only allowlist at all. Deny-by-default: a type added to READONLY_ALLOWLIST
# needs the hatch until someone declares it a harness utility.
: "${HARNESS_UTILITY_TYPES:=claude-code-guide statusline-setup output-style-setup}"
: "${GENERIC_AGENT_TYPES:=general-purpose}"
# Default the session teams dir to the platform location when unset/blank.
: "${SESSION_TEAMS_DIR:=$HOME/.claude/teams}"
# Default the allowed <model> name-token set to Claude Code's aliases if unset.
: "${ALLOWED_MODELS:=fable opus sonnet haiku}"

# (payload already read above, before root resolution)

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
[ "$TOOL_NAME" = "Agent" ] || exit 0

# A well-formed teammate name under the truthful-naming contract:
#   <role>-<model>-<identifier>   (THREE+ dash-joined parts)
# role: starts with a letter, alphanumeric, no dash. model/identifier:
# alphanumeric. Set-membership of the model token (clause 2a) and its
# truthfulness (clause 2b) are checked SEPARATELY so each failure gets a precise
# message; this regex only fixes the SHAPE (rejects bare "dev", 2-part "dev-1",
# run-together "devsonnet1").
NAME_SHAPE_RE='^[A-Za-z][A-Za-z0-9]*-[A-Za-z0-9]+-[A-Za-z0-9]+(-[A-Za-z0-9]+)*$'

# model_in_allowed_set <token> — is the token one of ALLOWED_MODELS?
model_in_allowed_set() {
  local tok="$1" m
  for m in $ALLOWED_MODELS; do
    [ "$tok" = "$m" ] && return 0
  done
  return 1
}

# THE MODEL RESOLVER — SOURCED, NOT CARRIED. resolve_expected_model() lived
# inline here until 2026-09-03, when guard-model-ceiling.sh became the SECOND
# consumer of the same fact ("which model does this spawn actually boot on?").
# Two copies of that answer is the defect this engine keeps re-discovering —
# the capability order guessed from prose, the root resolution hand-copied into
# ~35 hooks, the git jurisdiction copied five times and wrong in four. So the
# function moved to scripts/lib/resolve-model.sh byte-for-byte, and both guards
# now read one answer. It still uses ALLOWED_MODELS / ENTITY_ROOT / ENGINE_ROOT
# from this scope, and resolve_agent_def from the bootstrap above.
#
# FAIL-CLOSED, matching this guard's contract and NOT the ceiling guard's:
# clause 2b (is the name's <model> token truthful?) and clause 6 (is this a move
# down the capability order?) cannot be evaluated at all without it, and this
# guard already refuses rather than waves through when a model cannot be
# resolved. guard-model-ceiling.sh makes the opposite choice on the same missing
# file, on purpose, and says why in its own header.
_RM_LIB="$SCRIPT_DIR/../lib/resolve-model.sh"
if [ ! -f "$_RM_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-worktree-isolation.sh"
        echo "  scripts/lib/resolve-model.sh is missing at: $_RM_LIB"
        echo "  Without it this guard cannot tell WHICH MODEL a spawn boots on,"
        echo "  so neither the truthful-name clause nor the capability-tier"
        echo "  clause can be evaluated. It will not guess, and it will not"
        echo "  carry on quietly — a defense that reports 'on' while protecting"
        echo "  nothing is worse than none."
    } >&2
    exit 2
fi
# shellcheck source=../lib/resolve-model.sh
. "$_RM_LIB"

# THE SHARED TEAMMATE-NAME SHAPE RULE — see scripts/lib/teammate-name.sh's own
# header. This guard's own NAME_SHAPE_RE below is a loose PRE-check (any case,
# no length bound, extra segments allowed) that exists only to produce its own
# "missing/malformed name" message for the badly-broken cases (bare, 2-part,
# run-together); teammate_name_check applies the CREATOR's stricter rule on
# top of it (clause 2c, below), so a name the creator would refuse is refused
# here too, rather than one round trip later inside create-teammate-worktree.sh.
_TN_LIB="$SCRIPT_DIR/../lib/teammate-name.sh"
if [ ! -f "$_TN_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-worktree-isolation.sh"
        echo "  scripts/lib/teammate-name.sh is missing at: $_TN_LIB"
        echo "  Without it clause 2c (the creator's stricter name shape) cannot be"
        echo "  evaluated. It will not guess, and it will not carry on quietly."
    } >&2
    exit 2
fi
# shellcheck source=../lib/teammate-name.sh
. "$_TN_LIB"

# Parse subagent_type / isolation / name / prompt (with embedded newlines
# preserved via a \001 placeholder so the "main-checkout-run:" marker can be
# matched with a line-start anchor).
PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input", {})
    if not isinstance(ti, dict):
        raise ValueError("tool_input not an object")
    st = str(ti.get("subagent_type", "") or "")
    iso = str(ti.get("isolation", "") or "")
    nm = str(ti.get("name", "") or "")
    md = str(ti.get("model", "") or "")
    sid = str(d.get("session_id", "") or "")
    cwd = str(ti.get("cwd", "") or "")
    pr = str(ti.get("prompt", "") or "")
    pr = pr.replace("\t", " ").replace("\n", "\x01")
    print("OK\t%s\t%s\t%s\t%s\t%s\t%s\t%s" % (st, iso, nm, sid, md, cwd, pr))
except Exception:
    print("PARSEFAIL\t\t\t\t\t\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t\t\t\t\t\t')"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
SUBAGENT_TYPE="$(printf '%s' "$PARSED" | cut -f2)"
ISOLATION="$(printf '%s' "$PARSED" | cut -f3)"
NAME="$(printf '%s' "$PARSED" | cut -f4)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f5)"
MODEL_OVERRIDE="$(printf '%s' "$PARSED" | cut -f6)"
SPAWN_CWD="$(printf '%s' "$PARSED" | cut -f7)"
PROMPT="$(printf '%s' "$PARSED" | cut -f8- | tr '\001' '\n')"

# Fail-closed: an Agent spawn we cannot parse is blocked, never waved through.
if [ "$STATUS" = "PARSEFAIL" ]; then
  {
    echo "=== Teammate-spawn guard: BLOCKED ==="
    echo "  - Could not parse the Agent spawn payload to verify the spawn contract."
    echo "    Failing closed. Re-issue with isolation:\"worktree\" and a '<role>-<id>' name"
    echo "    (or, if this is a deliberate main-checkout run, add a"
    echo "    'main-checkout-run: <reason>' prompt line)."
    echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"
  } >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# CLAUSE 5 — STAFFING GATE. Runs BEFORE the isolation exemption and is entirely
# separate from it: this asks "may work be staffed to this type at all?", the
# allowlist below asks "does this type need a worktree?". See the header.
# ---------------------------------------------------------------------------
_type_in_set() {   # <type> <space-separated set>
  local _t="$1" _set="$2" _x
  for _x in $_set; do [ "$_t" = "$_x" ] && return 0; done
  return 1
}

# _staffing_reason_problem <reason> — echo NOTHING when the reason is a
# well-formed justification; echo the refusal reason otherwise. Kept as a
# function (not an inline $(...) heredoc) because bash will not parse a
# quoted heredoc containing an apostrophe inside a command substitution.
_staffing_reason_problem() {
  GA_REASON="$1" python3 - <<'PY'
import os, re
r = (os.environ.get("GA_REASON", "") or "").strip()
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
    "using","work","works","working","task","agent","one","two",
}
if not r:
    print("no 'generic-agent: <reason>' line is present in the prompt.")
else:
    low = r.lower()
    words = re.findall(r"[A-Za-z][A-Za-z'-]*", r)
    content = {w.lower() for w in words if len(w) >= 4 and w.lower() not in STOP}
    speed = re.search(
        r"(fastest|faster|quickest|quicker|save[sd]?\s+time|saving\s+time|"
        r"too\s+slow|no\s+time|momentum|convenien\w*|expedien\w*)", low)
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
    elif speed:
        print("the reason given is SPEED or CONVENIENCE (%r). That is the exact "
              "rationale this gate exists to refuse: on 2026-09-02 an engine-wide "
              "audit went to a generic built-in because a roster teammate would "
              "have needed a worktree created first. Creating one is a single call "
              "to mega-lander/create-teammate-worktree.sh, or one isolation:\"worktree\" "
              "parameter. Being quicker to dispatch is never a reason to staff work "
              "to a non-teammate." % speed.group(0))
    else:
        print("")
PY
}

NEEDS_STAFFING_HATCH=0
if ! _type_in_set "$SUBAGENT_TYPE" "$HARNESS_UTILITY_TYPES"; then
  if _type_in_set "$SUBAGENT_TYPE" "$READONLY_ALLOWLIST" \
     || _type_in_set "$SUBAGENT_TYPE" "$GENERIC_AGENT_TYPES"; then
    NEEDS_STAFFING_HATCH=1
  fi
fi

if [ "$NEEDS_STAFFING_HATCH" -eq 1 ]; then
  GENERIC_REASON=""
  if printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*generic-agent:[[:space:]]*.+'; then
    GENERIC_REASON="$(printf '%s' "$PROMPT" \
      | grep -E '^[[:space:]]*generic-agent:[[:space:]]*.+' \
      | sed -n '1p' \
      | sed -E 's/^[[:space:]]*generic-agent:[[:space:]]*//')"
  fi

  # Empty output = the hatch is well-formed. Non-empty = the refusal reason.
  STAFFING_WHY="$(_staffing_reason_problem "$GENERIC_REASON" 2>/dev/null || printf 'the justification could not be evaluated (fail-closed).')"

  if [ -n "$STAFFING_WHY" ]; then
    ROSTER_HINT=""
    if [ -d "$ENTITY_ROOT/.claude/agents" ]; then
      ROSTER_HINT="$(ls "$ENTITY_ROOT/.claude/agents"/*.md 2>/dev/null \
        | while IFS= read -r _f; do _b="${_f##*/}"; printf '%s ' "${_b%.md}"; done)"
    fi
    {
      echo "=== Teammate-spawn guard: BLOCKED (clause 5 — staffing) ==="
      echo "  subagent_type '${SUBAGENT_TYPE}' is a GENERIC agent type, not a roster teammate,"
      echo "  and ${STAFFING_WHY}"
      echo ""
      echo "  WHY THIS IS REFUSED, not merely discouraged:"
      echo "    - a generic agent never appears in the CEO's team display, so work sent"
      echo "      to one disappears from his view of what is running;"
      echo "    - it has no worktree and leaves no commit, so the work leaves no artifact"
      echo "      in the record — only this session's transcript, which does not survive."
      echo ""
      echo "  FIX (preferred): re-issue with the ROSTER TEAMMATE whose domain this work is,"
      echo "  with isolation: \"worktree\" and a truthful '<role>-<model>-<identifier>' name."
      if [ -n "$ROSTER_HINT" ]; then
        echo "    roster: ${ROSTER_HINT}"
      fi
      echo "  Needing a worktree first is NOT a reason to reach for a generic agent —"
      echo "  $ENGINE_ROOT/mega-lander/create-teammate-worktree.sh creates and registers one in a single call."
      echo ""
      echo "  FIX (only when no roster teammate genuinely fits): add ONE live prompt line"
      echo "      generic-agent: <why no roster teammate fits this work>"
      echo "  It must be a real reason — at least 30 characters, at least 5 words, at least"
      echo "  3 substantive words — and it must not be a speed or convenience argument."
      echo "  Every accepted use is logged to .claude/state/generic-agent-dispatches.log,"
      echo "  so a habit of waiving is visible rather than invisible."
      echo ""
      echo "  (READONLY_ALLOWLIST exempts this type from WORKTREE ISOLATION — a safety"
      echo "   question. It has never been a permission to STAFF work here. A harness"
      echo "   utility that carries no delegated work belongs in HARNESS_UTILITY_TYPES"
      echo "   in orchestration.config, not in a waiver typed on every dispatch.)"
      echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"
    } >&2
    exit 2
  fi

  # Hatch accepted. Best-effort log — never fail the spawn because logging
  # failed; the marker in the prompt is itself the audit trail.
  GA_LOG_DIR="$ENTITY_ROOT/.claude/state"
  mkdir -p "$GA_LOG_DIR" 2>/dev/null || true
  printf '%s\tagent=%s\tname=%s\tgeneric-agent: %s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    "${SUBAGENT_TYPE:-<unset>}" \
    "${NAME:-<unset>}" \
    "$GENERIC_REASON" \
    >>"$GA_LOG_DIR/generic-agent-dispatches.log" 2>/dev/null || true
fi

# Read-only agent types are exempt from the teammate contract (frictionless):
# no isolation, no name. They are NOT exempt from the point-9 lock-out — "a
# restarted agent is refused every tool" — and until 2026-09-12 they escaped it
# anyway, because this early return also skipped clause 7's REGISTRATION and
# the lock-out finds an agent through its registration. barrier() answered
# UNREGISTERED for a read-only agent forever, so it could never be found
# finished. `Explore` carries every tool except Edit/Write/NotebookEdit — it
# carries BASH — so a restarted finished Explore could write anywhere a shell
# can, including after its workspace was gone.
#
# So it is registered first, with no workspaces, and then exempted exactly as
# before. Point 3 governs the failure: "If registration fails, the spawn does
# not happen." It costs the spawn nothing afterwards: an agent that produced
# nothing counts as landed (point 7), so it is landed the moment it finishes
# and never becomes pending work in front of the CEO's turn ends.
for a in $READONLY_ALLOWLIST; do
  if [ "$SUBAGENT_TYPE" = "$a" ]; then
    RO_RC=0
    RO_ERR="$(printf '%s' "$INPUT" | python3 "$SCRIPT_DIR/../../mega-lander/workspaces.py" --entity "$ENTITY_ROOT" register-readonly 2>&1 >/dev/null)" || RO_RC=$?
    if [ "$RO_RC" -ne 0 ]; then
      {
        echo "=== Teammate-spawn guard: BLOCKED (clause 7 — registration at spawn) ==="
        echo "  '${SUBAGENT_TYPE}' needs no workspace, so it is exempt from isolation and from the"
        echo "  name contract. It is NOT exempt from the lock-out that refuses a restarted"
        echo "  finished agent every tool (point 9), and that lock-out finds an agent through"
        echo "  its registration — a read-only agent still carries Bash. The registration could"
        echo "  not be written, so the spawn does not happen (point 3):"
        echo "    - ${RO_ERR:-the registration could not be written (exit $RO_RC)}"
        echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"
      } >&2
      exit 2
    fi
    exit 0
  fi
done

# File-capable teammate: enforce the FULL correct-spawn contract.
PROBLEMS=()

# main-checkout-run: <reason> — a live prompt line (kept simple/best-effort by
# design; this is an auditable FALLBACK opt-out, not a security boundary — the
# boundary is guard-main-checkout-writes.sh blocking actual writes).
MAIN_CHECKOUT_MARKER=""
if printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*main-checkout-run:[[:space:]]*.+'; then
  MAIN_CHECKOUT_MARKER="$(printf '%s' "$PROMPT" | grep -oE '^[[:space:]]*main-checkout-run:[[:space:]]*.+' | sed -n '1p' | sed -E 's/^[[:space:]]*//')"
fi

# --- CLAUSE 4 helpers ------------------------------------------------------
# registered_teammate_worktree <path> -> 0 when <path> is the top level of a
# LINKED git worktree on a cc/ branch (docs/plans/worktree-spec-2026-09-11.md,
# point 1) and not a codex/ one (point 2); prints the reason for a refusal on
# stdout otherwise. Whether it is registered FOR THIS SPAWN — this session,
# this teammate — is clause 7's question, answered by the workspace registry
# (mega-lander/workspaces.py register-spawn).
registered_teammate_worktree() {
  local p="$1" top common gitdir br
  [ -n "$p" ] || { printf 'no path given'; return 1; }
  [ -d "$p" ] || { printf "'%s' does not exist" "$p"; return 1; }
  top="$(git -C "$p" rev-parse --show-toplevel 2>/dev/null || true)"
  [ -n "$top" ] || { printf "'%s' is not inside a git worktree" "$p"; return 1; }
  if [ "$(cd "$top" && pwd -P)" != "$(cd "$p" && pwd -P)" ]; then
    printf "'%s' is not the top level of a worktree (that is '%s')" "$p" "$top"; return 1
  fi
  common="$(git -C "$p" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
  gitdir="$(git -C "$p" rev-parse --path-format=absolute --git-dir 2>/dev/null || true)"
  if [ -z "$common" ] || [ "$common" = "$gitdir" ]; then
    printf "'%s' is a MAIN checkout, not a linked worktree — a teammate never works in the main checkout" "$p"; return 1
  fi
  br="$(git -C "$p" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
  case "$br" in
    codex/*) printf "'%s' is a codex/ workspace — codex/ is never touched, and an agent never works inside one; it works from a copy in its own cc/ workspace (point 2)" "$p"; return 1 ;;
    cc/*) : ;;
    *) printf "'%s' is on branch '%s', not a cc/ branch — every non-native workspace is named cc/ (point 1)" "$p" "${br:-<detached>}"; return 1 ;;
  esac
  return 0
}
# The hint every clause-4 refusal ends with. It named the four-step path
# first and spawn.sh as an afterthought until 2026-09-13 — the same ordering
# defect as the session banner, and the same cost: the reader of a refusal
# does the thing that produced the refusal. ONE command first; the manual
# route is still named because it is still accepted, and nothing about what
# this guard blocks changed with the wording.
HELPER_HINT="do all of it in ONE command —  <engine>/scripts/spawn.sh <teammate-name> --repo <repo> --type <subagent-type> --brief <file>  creates and REGISTERS the tree, assembles the payload, and evaluates every PreToolUse[Agent] guard from every surface BEFORE anything exists, reporting all failures together. By hand it is  <engine>/mega-lander/create-teammate-worktree.sh <repo> <teammate-name>  which creates, seeds .worktreeinclude, and REGISTERS the tree; then spawn with isolation:\"worktree\" and add the prompt line  cross-repo-worktree: <path>  (a cwd-only spawn is refused: it has no platform-owned lifecycle witness)."

# A DRY EVALUATION'S *PLANNED* WORKSPACES (see clause 7g). scripts/spawn.sh
# evaluates this guard BEFORE it creates anything, precisely so that a brief
# with three problems costs one run instead of three dispatches. Its workspace
# therefore does not exist yet, and "does it exist and is it on a cc/ branch?"
# has no answer for it — the same shape as clause 7a, and the same resolution:
# the answerable question one step earlier (does its repository resolve, is its
# branch free, is its path absent and creatable?) is asked instead, by
# `check-spawn` in mega-lander/workspaces.py, so there is ONE answer in ONE
# place. Nothing here is skipped for a workspace that is NOT planned, and a
# live call can never be dry (it carries a tool_use_id).
DRY_PLANNED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
if not (isinstance(d.get("richos_spawn_check"), dict) and not str(d.get("tool_use_id") or "")):
    sys.exit(0)
for p in (d["richos_spawn_check"].get("planned") or []):
    if isinstance(p, dict) and p.get("path"):
        print(str(p["path"]).rstrip("/"))
' 2>/dev/null || true)"

is_planned_path() {   # <path> -> 0 when this dry evaluation is about to create it
  local _q="${1%/}" _p
  [ -n "$DRY_PLANNED" ] || return 1
  while IFS= read -r _p; do
    [ -n "$_p" ] && [ "$_p" = "$_q" ] && return 0
  done <<PLANNED_EOF
$DRY_PLANNED
PLANNED_EOF
  return 1
}

case "$ISOLATION" in
  worktree|remote)
    if [ -n "$SPAWN_CWD" ]; then
      PROBLEMS+=("cwd and isolation are mutually exclusive — the Agent tool refuses the pair (got cwd='${SPAWN_CWD}' with isolation='${ISOLATION}'). For cross-repository work keep isolation and name the registered worktree on a 'cross-repo-worktree: <path>' prompt line (a cwd-only spawn is refused: it has no platform-owned lifecycle witness).")
    fi
    ;;
  *)
    if [ -n "$SPAWN_CWD" ]; then
      # CLAUSE 4a — a cwd spawn stands in for native isolation ONLY inside a
      # registered cross-repository worktree.
      CWD_WHY="$(registered_teammate_worktree "$SPAWN_CWD")" || \
        PROBLEMS+=("cwd spawn refused — ${CWD_WHY}. A cross-repository teammate works only in a worktree RichOS registered, so its owner can be judged after this session is gone: ${HELPER_HINT}")
    elif printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*cross-repo-worktree:[[:space:]]*[^[:space:]]'; then
      # The linked target worktrees provide isolation already. Clauses 4c
      # and 7 below still verify every path, owner and native call identity.
      :
    elif [ -n "$MAIN_CHECKOUT_MARKER" ]; then
      # Auditable fallback opt-out taken. Best-effort log — never fail the
      # spawn because logging failed; the marker itself is the audit trail.
      LOG_DIR="$ENTITY_ROOT/.claude/state"
      mkdir -p "$LOG_DIR" 2>/dev/null || true
      {
        printf '%s\tagent=%s\tname=%s\t%s\n' \
          "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
          "${SUBAGENT_TYPE:-<unset>}" \
          "${NAME:-<unset>}" \
          "$MAIN_CHECKOUT_MARKER"
      } >>"$LOG_DIR/main-checkout-runs.log" 2>/dev/null || true
    else
      PROBLEMS+=("missing native isolation — add  isolation: \"worktree\"  to run in an isolated worktree (got isolation='${ISOLATION:-unset}'); for cross-repository work ALSO add a 'cross-repo-worktree: <path>' prompt line naming a worktree registered by mega-lander/create-teammate-worktree.sh (a cwd-only spawn is refused); OR, if this is a deliberate main-checkout run, add a live prompt line starting with 'main-checkout-run: <reason>'.")
    fi
    ;;
esac

# CLAUSE 4c — every cross-repo-worktree: line names a REGISTERED worktree.
while IFS= read -r _marker_path; do
  [ -n "$_marker_path" ] || continue
  if is_planned_path "$_marker_path"; then continue; fi
  MK_WHY="$(registered_teammate_worktree "$_marker_path")" || \
    PROBLEMS+=("cross-repo-worktree: line refused — ${MK_WHY}. ${HELPER_HINT}")
done <<MARKERS_EOF
$(printf '%s' "$PROMPT" | sed -n -E 's/^[[:space:]]*cross-repo-worktree:[[:space:]]*(.*[^[:space:]])[[:space:]]*$/\1/p')
MARKERS_EOF

# CLAUSE 4d — a prompt that tells the teammate to hand-roll a worktree is the
# improvisation this clause ends. Audited escape hatch: hand-roll-ack: <reason>.
if printf '%s' "$PROMPT" | grep -E >/dev/null 'git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+worktree[[:space:]]+add'; then
  HAND_ROLL_ACK=""
  if printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*hand-roll-ack:[[:space:]]*.+'; then
    HAND_ROLL_ACK="$(printf '%s' "$PROMPT" | grep -oE '^[[:space:]]*hand-roll-ack:[[:space:]]*.+' | sed -n '1p' | sed -E 's/^[[:space:]]*//')"
  fi
  if [ -n "$HAND_ROLL_ACK" ]; then
    LOG_DIR="$ENTITY_ROOT/.claude/state"
    mkdir -p "$LOG_DIR" 2>/dev/null || true
    printf '%s\tagent=%s\tname=%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${SUBAGENT_TYPE:-<unset>}" "${NAME:-<unset>}" "$HAND_ROLL_ACK" >>"$LOG_DIR/hand-roll-acks.log" 2>/dev/null || true
  else
    PROBLEMS+=("the prompt instructs the teammate to run 'git worktree add' — an improvised workspace has no registration, so it is finished work the moment it exists (docs/plans/worktree-spec-2026-09-11.md, point 3). ${HELPER_HINT} If this instruction is genuinely intended (a task ABOUT worktree tooling), add a live prompt line 'hand-roll-ack: <reason>' — logged to .claude/state/hand-roll-acks.log.")
  fi
fi

if ! printf '%s' "$NAME" | grep -E >/dev/null "$NAME_SHAPE_RE"; then
  PROBLEMS+=("missing/malformed name — give a unique '<role>-<model>-<identifier>' name, e.g. dev-sonnet-1 / qa-opus-r3 (got name='${NAME:-unset}'). Needs THREE dash-joined parts (role, model, identifier). Never bare ('dev'), 2-part ('dev-1'), or run-together ('devsonnet1'). NOT relaxed by main-checkout-run:.")
else
  # Shape valid: split into role / <model> token / identifier.
  NAME_ROLE="${NAME%%-*}"
  NAME_REST="${NAME#*-}"
  NAME_MODEL_TOKEN="${NAME_REST%%-*}"
  NAME_ID="${NAME_REST#*-}"

  # Clause 2c — the CREATOR (mega-lander/create-teammate-worktree.sh) reads a
  # STRICTER shape than this guard's own NAME_SHAPE_RE ever did: lower-case
  # only, role <=16 chars, identifier <=12 chars, EXACTLY three dash-joined
  # parts (no fourth segment). The two rules now live in one place,
  # scripts/lib/teammate-name.sh, and this is that shared rule applied here
  # too — so a name the creator will refuse is refused HERE, before spawn.sh's
  # guard table ever says "passed" (2026-09-17: zach-sonnet-multirepodemo1, a
  # 14-character identifier, passed this guard's own loose check and was only
  # then refused by the creator, one round trip later).
  if ! _TN_MSG="$(teammate_name_check "$NAME" "$ALLOWED_MODELS")"; then
    PROBLEMS+=("$_TN_MSG")
  fi

  # Clause 2a — the <model> token must be a real alias.
  if ! model_in_allowed_set "$NAME_MODEL_TOKEN"; then
    PROBLEMS+=("model token '${NAME_MODEL_TOKEN}' in name '${NAME}' is not an allowed model — the <model> part of <role>-<model>-<identifier> must be one of: ${ALLOWED_MODELS// /, } (e.g. ${NAME_ROLE}-sonnet-${NAME_ID}). Extend ALLOWED_MODELS in orchestration.config if your harness accepts more.")
  else
    # Clause 2b — TRUTHFULNESS. Resolve the model this spawn boots on and
    # require the name token to match it.
    EXPECTED_MODEL="$(resolve_expected_model "$MODEL_OVERRIDE" "$SUBAGENT_TYPE")"
    if [ "$EXPECTED_MODEL" = "UNRESOLVABLE" ]; then
      # FAIL LOUD. A namespaced subagent_type is by construction supplied by a
      # plugin, so its definition MUST be locatable; if it is not, clause 2b
      # cannot be evaluated at all. The old code degraded to "accept" here
      # without a word.
      PROBLEMS+=("cannot verify the model token — subagent_type '${SUBAGENT_TYPE}' is namespaced, but no definition for it was found under the entity roster (${ENTITY_ROOT}/.claude/agents/), the engine's own roster (${ENGINE_ROOT}/.claude/agents/), or AGENT_NAMESPACE_ROOTS. The model-truthfulness clause cannot be evaluated, so this spawn is refused rather than waved through. Fix: add '<namespace>=<plugin root>' to AGENT_NAMESPACE_ROOTS in orchestration.config, or dispatch the un-namespaced type.")
    elif [ -n "$EXPECTED_MODEL" ] && [ "$NAME_MODEL_TOKEN" != "$EXPECTED_MODEL" ]; then
      if [ -n "$MODEL_OVERRIDE" ]; then
        MODEL_SRC="explicit model override '${MODEL_OVERRIDE}'"
      else
        MODEL_SRC="the '${SUBAGENT_TYPE}' agent definition's model: frontmatter"
      fi
      PROBLEMS+=("untruthful model token — the name claims '${NAME_MODEL_TOKEN}' but this spawn boots on '${EXPECTED_MODEL}' (from ${MODEL_SRC}). The <model> token MUST match the model the instance actually runs on. Fix: rename to '${NAME_ROLE}-${EXPECTED_MODEL}-${NAME_ID}', or change the model so it matches.")
    fi
    # else: expected model is undeterminable (no override AND no live-def
    # default, or model:"inherit" — the inherit-from-session case). We cannot
    # know the boot model, so we accept the (already-valid) alias token rather
    # than fail the spawn on unknowable information.
  fi
fi

# ---------------------------------------------------------------------------
# CLAUSE 6 — a move to a LOWER capability tier is stated, never silent.
# The order is DATA (orchestration.config MODEL_TIERS) read through
# scripts/lib/model-tiers.sh. Nothing here infers capability from an alias.
# See the header for the incident this exists to end.
# ---------------------------------------------------------------------------
_c6_announce_skip() {   # <reason> — fail-open notice: stderr + systemMessage
  local _m="NOTICE: guard-worktree-isolation.sh clause 6 (model tier) SKIPPED for spawn '${NAME:-<unset>}' of '${SUBAGENT_TYPE:-<unset>}' — $1. The spawn is ALLOWED (fail-open), but the tier check protected nothing here. Fix the declaration: MODEL_TIERS in orchestration.config must be well-formed and rank every alias in ALLOWED_MODELS."
  printf '%s\n' "$_m" >&2
  SYSMSG="$_m" python3 -c '
import json, os
print(json.dumps({"systemMessage": os.environ.get("SYSMSG", "")}))
' 2>/dev/null || true
}

C6_OVERRIDE_LC="$(printf '%s' "$MODEL_OVERRIDE" | tr '[:upper:]' '[:lower:]')"
if [ -n "$MODEL_OVERRIDE" ] && [ "$C6_OVERRIDE_LC" != "inherit" ]; then
  MODEL_TIERS_LIB="$SCRIPT_DIR/../lib/model-tiers.sh"
  C6_SKIP=""
  if [ -f "$MODEL_TIERS_LIB" ]; then
    # shellcheck source=../lib/model-tiers.sh
    . "$MODEL_TIERS_LIB"
    C6_SPEC_PROBLEM="$(model_tiers_problem "${MODEL_TIERS:-}")"
    [ -z "$C6_SPEC_PROBLEM" ] || C6_SKIP="MODEL_TIERS in orchestration.config is unusable (${C6_SPEC_PROBLEM})"
  else
    C6_SKIP="scripts/lib/model-tiers.sh is missing at ${MODEL_TIERS_LIB}"
  fi
  if [ -z "$C6_SKIP" ]; then
    C6_DEFAULT="$(resolve_expected_model "" "$SUBAGENT_TYPE")"
    C6_REQUESTED="$(resolve_expected_model "$MODEL_OVERRIDE" "$SUBAGENT_TYPE")"
    # No resolvable definition default (host built-in, or a def with no model:
    # line) means there is nothing to be lower THAN: the clause is inert, and
    # that is not a skip worth announcing. UNRESOLVABLE is clause 2b's refusal
    # and has already been added to PROBLEMS above.
    if [ -n "$C6_DEFAULT" ] && [ "$C6_DEFAULT" != "UNRESOLVABLE" ] && [ -n "$C6_REQUESTED" ]; then
      C6_RANK_DEFAULT="$(model_tier_rank "$C6_DEFAULT" "$MODEL_TIERS")"
      C6_RANK_REQUESTED="$(model_tier_rank "$C6_REQUESTED" "$MODEL_TIERS")"
      if [ -z "$C6_RANK_DEFAULT" ] || [ -z "$C6_RANK_REQUESTED" ]; then
        C6_UNRANKED=""
        [ -n "$C6_RANK_DEFAULT" ] || C6_UNRANKED="'${C6_DEFAULT}'"
        [ -n "$C6_RANK_REQUESTED" ] || C6_UNRANKED="${C6_UNRANKED:+$C6_UNRANKED and }'${C6_REQUESTED}'"
        C6_SKIP="MODEL_TIERS=\"${MODEL_TIERS}\" ranks no alias named ${C6_UNRANKED}"
      elif [ "$C6_RANK_REQUESTED" -gt "$C6_RANK_DEFAULT" ]; then
        # LOWER tier than the definition's default. Stated, or refused.
        C6_ACK=""
        if printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*model-downgrade-ack:[[:space:]]*[^[:space:]]'; then
          C6_ACK="$(printf '%s' "$PROMPT" | grep -E '^[[:space:]]*model-downgrade-ack:[[:space:]]*[^[:space:]]' | sed -n '1p' | sed -E 's/^[[:space:]]*//')"
        fi
        if [ -n "$C6_ACK" ]; then
          # Accepted. Best-effort log — never fail the spawn because logging
          # failed; the line in the prompt is itself the audit trail.
          C6_LOG_DIR="$ENTITY_ROOT/.claude/state"
          mkdir -p "$C6_LOG_DIR" 2>/dev/null || true
          printf '%s\tagent=%s\tname=%s\tdefault=%s\trequested=%s\t%s\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            "${SUBAGENT_TYPE:-<unset>}" \
            "${NAME:-<unset>}" \
            "$C6_DEFAULT" \
            "$C6_REQUESTED" \
            "$C6_ACK" \
            >>"$C6_LOG_DIR/model-downgrade-acks.log" 2>/dev/null || true
        else
          PROBLEMS+=("model tier — spawn '${NAME:-<unset>}' of '${SUBAGENT_TYPE}' requests model '${C6_REQUESTED}' (tier ${C6_RANK_REQUESTED}), but the '${SUBAGENT_TYPE}' definition defaults to '${C6_DEFAULT}' (tier ${C6_RANK_DEFAULT}). That is a LOWER capability tier under the declared order MODEL_TIERS=\"${MODEL_TIERS}\" (orchestration.config; tier 1 is the most capable, aliases inside one tier are equal, and nothing infers this order from a name). A move DOWN a tier must state its reason. Fix: add ONE live prompt line  model-downgrade-ack: <why this task needs less than the definition's default>  — a bare marker exempts nothing; accepted uses are logged to .claude/state/model-downgrade-acks.log. Equal or higher tier never needs it.")
        fi
      fi
      # Same or higher tier: silent, by design. Upgrades were never the defect.
    fi
  fi
  [ -z "$C6_SKIP" ] || _c6_announce_skip "$C6_SKIP"
fi

if [ "${#PROBLEMS[@]}" -gt 0 ]; then
  {
    echo "=== Teammate-spawn guard: BLOCKED ==="
    echo "  Agent '${SUBAGENT_TYPE:-<unset/general-purpose>}' can edit repo files, so it must satisfy the"
    echo "  teammate-spawn contract. It failed:"
    for p in "${PROBLEMS[@]}"; do
      echo "    - $p"
    done
    echo "  Re-issue the Agent call fixing the above."
    echo "  (If this agent is genuinely READ-ONLY, add its type to READONLY_ALLOWLIST"
    echo "   in orchestration.config.)"
    echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"
  } >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# CLAUSE 3 — name reuse is structurally impossible.
# ---------------------------------------------------------------------------
# Only reached once isolation + name-format already pass (so we never record a
# name for a spawn that is being blocked for a different reason). Inert unless a
# session team dir resolves by EXACT match (so bare unit-test payloads with a
# nonexistent session are unaffected — existing suites stay green). The teams
# dir is read from orchestration.config (SESSION_TEAMS_DIR); tests override via
# GUARD_ISOLATION_TEAMS_DIR.
GI_TEAMS_DIR="${GUARD_ISOLATION_TEAMS_DIR:-$SESSION_TEAMS_DIR}"
GI_TEAM_DIR=""
if [ -n "$SESSION_ID" ]; then
  CANDIDATE="$GI_TEAMS_DIR/session-$(printf '%s' "$SESSION_ID" | cut -c1-8)"
  [ -d "$CANDIDATE" ] && GI_TEAM_DIR="$CANDIDATE"
fi

if [ -n "$GI_TEAM_DIR" ]; then
  # Union the three durable name-history sources and test NAME against it. Fail
  # OPEN on a resolver error (never invent a collision from an IO glitch); the
  # primary source (spawned-names.log) is written by this same hook so, absent
  # IO failure, it cannot miss a name spawned this session.
  REUSE_VERDICT="$(NAME_ARG="$NAME" python3 - "$GI_TEAM_DIR" 2>/dev/null <<'PY'
import json, os, sys

name = os.environ.get("NAME_ARG", "")
team_dir = sys.argv[1]
history = set()

# 1. self-maintained ledger (one name per line)
ledger = os.path.join(team_dir, "spawned-names.log")
try:
    with open(ledger, "r", encoding="utf-8") as f:
        for line in f:
            v = line.strip()
            if v:
                history.add(v)
except Exception:
    pass

# 2. roster config.json member names (any status)
try:
    with open(os.path.join(team_dir, "config.json"), "r", encoding="utf-8") as f:
        cfg = json.load(f)
    for m in cfg.get("members", []) or []:
        nm = m.get("name")
        if isinstance(nm, str) and nm:
            history.add(nm)
except Exception:
    pass

# 3. durable event logs — completed teammates that may be roster-pruned
for logname in ("idle-events.jsonl", "task-events.jsonl"):
    try:
        with open(os.path.join(team_dir, logname), "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                t = rec.get("teammate")
                if isinstance(t, str) and t:
                    history.add(t)
    except Exception:
        pass

# The lead is not a spawnable teammate name; never treat it as a collision.
history.discard("team-lead")

print("REUSE" if name in history else "FRESH")
PY
)"
  # Fail-OPEN on a resolver error / empty output (never invent a collision).
  [ -n "$REUSE_VERDICT" ] || REUSE_VERDICT="FRESH"

  if [ "$REUSE_VERDICT" = "REUSE" ]; then
    {
      echo "=== Teammate-spawn guard: BLOCKED (name reuse) ==="
      echo "  The name '${NAME}' has ALREADY been used in this session's team"
      echo "  (active OR completed). Reusing a name causes latest-wins shadowing and"
      echo "  resumed-vs-respawned ambiguity, so it is forbidden — there is NO escape"
      echo "  hatch for this one."
      echo "  Fix: pick a FRESH '<role>-<identifier>' name. Identifiers are free —"
      echo "  bump the suffix (${NAME}-2 / ${NAME%%-*}-b / ${NAME%%-*}-0714) or use any"
      echo "  unused identifier. Never resume-vs-respawn by reusing a name."
      echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"
    } >&2
    exit 2
  fi

  # NOTE: this guard deliberately does NOT append NAME to spawned-names.log.
  # See "BLOCKED-SPAWN NAME BURN" in this file's header. The append lives in
  # the PostToolUse[Agent] partner hook (detect-nonnative-worktree.sh), which
  # only ever fires for a tool call that actually executed. The CHECK above is
  # unchanged and still runs first, at spawn time.
fi

# ---------------------------------------------------------------------------
# CLAUSE 7 — REGISTRATION AT SPAWN (docs/plans/worktree-spec-2026-09-11.md).
# Every clause above is satisfied; before this call may run it is REGISTERED
# in the workspace registry (mega-lander/workspaces.py register-spawn), and:
#   "If registration fails, the spawn does not happen."            (point 3)
# The registration also carries point 5:
#   "while any finished agent's work is neither landed nor discarded, Rich can
#    neither start new work nor end his turn"
# so a spawn is refused while finished work is pending — every finished agent
# whose work is already in main is landed first, automatically (point 4) —
# unless the spawn is the work that lands it: a `lands-pending: <name>` or a
# `continues: <name>` prompt line naming the pending agent (points 5, 7).
#   7a. every `cross-repo-worktree:` path must be a cc/ workspace registered for
#       THIS session and THIS teammate when it was created
#       (mega-lander/create-teammate-worktree.sh), still on its registered branch;
#   7b. a file-capable Agent call with run_in_background: false is refused;
#   7c. the lifecycle components registration depends on must be present — the
#       registry, the lifecycle hook that records the platform's facts, and the
#       lock-out — or the spawn is refused;
#   7d. the registration itself must be written (temp file, fsync, rename);
#   7f. a cwd-only spawn is refused: the platform creates no workspace of its
#       own for it, and the registration names the native workspace every
#       file-capable spawn is given.
#   7g. a DRY EVALUATION (scripts/spawn.sh, before the call is made) runs every
#       check above through `check-spawn` and writes nothing. See the block at
#       C7_VERB for why it cannot be used to skip a registration.
#
# THIS HOOK'S BUDGET IS PART OF CLAUSE 7, AND A KILL IS NOT A REFUSAL.
# The registration is a WRITE, and it is the last thing this guard does. On
# 2026-09-17 the platform killed this hook at 10.027s against a 10s budget
# (orchestrator transcript, `hook_cancelled … timedOut: true`, tool_use_id
# toolu_01C3dCBDwFksv5tBHramMDch) and then LAUNCHED THE AGENT ANYWAY two
# seconds later — so point 3's "if registration fails, the spawn does not
# happen" was not violated by any decision in this file; the decision was never
# reached. The registration's own longest wait is the registry lock, which
# waits up to 30s (mega-lander/workspaces.py, class Lock), and point 5's gate
# inside register_spawn budgets 4s of auto-landing on top of that, so a 10s
# hook could be killed mid-write while doing exactly what it should. The budget
# in hooks/hooks.json is therefore 60s — larger than the longest wait inside
# it, and the same budget guard-owned-state.sh already carries.
# A LARGER BUDGET IS NOT A FIX: the platform decides when to kill a hook, so
# the spawn-without-a-registration case is repaired rather than merely made
# rarer — `bind_agent` (PostToolUse[Agent]) and `observe_platform_binding`
# (every land/status afterwards) adopt the platform's own record of which agent
# the call became. Neither certifies the spawn: they record what happened, and
# the record says it was reconciled.
# ---------------------------------------------------------------------------
C7_PROBLEMS=()
for _c in "mega-lander/workspaces.py" "scripts/hooks/workspace-lifecycle.sh" "scripts/hooks/guard-sealed-worktree.sh"; do
  [ -f "$SCRIPT_DIR/../../$_c" ] || C7_PROBLEMS+=("lifecycle component MISSING: $_c — without it this spawn's workspaces could not be registered, finished or locked out. Restore the engine (scripts/hooks/install.sh) before spawning a file-capable teammate.")
done
for _c in "scripts/hooks/guard-sealed-worktree.sh" "scripts/hooks/workspace-lifecycle.sh"; do
  if [ -f "$SCRIPT_DIR/../../$_c" ] && [ ! -x "$SCRIPT_DIR/../../$_c" ]; then
    C7_PROBLEMS+=("lifecycle component NOT EXECUTABLE: $_c — the harness cannot run it. Restore the engine (scripts/hooks/install.sh) before spawning a file-capable teammate.")
  fi
done
RUN_IN_BG="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); v=(d.get("tool_input") or {}).get("run_in_background"); print("false" if v is False else ("true" if v is True else ""))' 2>/dev/null || true)"
if [ "$RUN_IN_BG" = "false" ]; then
  C7_PROBLEMS+=("run_in_background: false on a file-capable spawn — spawn it as a background teammate (omit run_in_background, or set it true), so the orchestrator can land it when it finishes.")
fi
if [ -n "$SPAWN_CWD" ] && [ "$ISOLATION" != "worktree" ] && [ "$ISOLATION" != "remote" ]; then
  C7_PROBLEMS+=("cwd-only spawn refused — spawn it with isolation:\"worktree\" and put the registered cc/ workspace on a 'cross-repo-worktree: ${SPAWN_CWD}' prompt line instead, so both of its workspaces are registered and go together (points 6, 10).")
fi

# 7g. A DRY EVALUATION IS NOT A LIVE ONE, AND THE DIFFERENCE IS THE WRITE.
#     `scripts/spawn.sh` evaluates this guard against a payload it has not sent
#     yet, so that a brief with three problems is fixed once instead of costing
#     three dispatches. Such a payload has no tool_use_id — only the platform
#     mints one — so `register-spawn` refused it every time, which is why the
#     pre-flight this replaces carried a KNOWN FALSE POSITIVE on the one check
#     that matters most. A check that always fails protects nothing and teaches
#     the reader to skip it.
#
#     So the dry evaluation runs the SAME code with the write removed:
#     `check-spawn` performs every refusal `register-spawn` performs and writes
#     no record, no binding and no event. It is reached only when the payload
#     carries `richos_spawn_check` AND carries NO `tool_use_id`; a live call
#     always carries one (this guard has refused every payload without one
#     since it was written, and spawns happen), so the marker can never turn a
#     live spawn into an unregistered one. `check-spawn` itself refuses a
#     payload that does carry a tool_use_id rather than evaluating it.
C7_VERB="register-spawn"
if printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(1)
sys.exit(0 if (isinstance(d.get("richos_spawn_check"), dict)
               and not str(d.get("tool_use_id") or "")) else 1)
' 2>/dev/null; then
  C7_VERB="check-spawn"
fi

if [ "${#C7_PROBLEMS[@]}" -eq 0 ]; then
  # `set -e` is on: the failing substitution must not end the script silently.
  C7_RC=0
  C7_ERR="$(printf '%s' "$INPUT" | python3 "$SCRIPT_DIR/../../mega-lander/workspaces.py" --entity "$ENTITY_ROOT" "$C7_VERB" 2>&1 >/dev/null)" || C7_RC=$?
  if [ "$C7_RC" -ne 0 ]; then
    C7_PROBLEMS+=("${C7_ERR:-the registration could not be written (exit $C7_RC)}")
  fi
fi

if [ "${#C7_PROBLEMS[@]}" -gt 0 ]; then
  {
    echo "=== Teammate-spawn guard: BLOCKED (clause 7 — registration at spawn) ==="
    echo "  The spawn is well-formed, but it could not be registered, so it does not"
    echo "  happen (docs/plans/worktree-spec-2026-09-11.md, point 3):"
    for p in "${C7_PROBLEMS[@]}"; do
      echo "    - $p"
    done
    echo "(hook: scripts/hooks/guard-worktree-isolation.sh)"
  } >&2
  exit 2
fi

exit 0
