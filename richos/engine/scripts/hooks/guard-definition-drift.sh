#!/usr/bin/env bash
#
# guard-definition-drift.sh — BLOCKING PreToolUse guard on the `Agent` tool.
# Closes the "a mid-session definition install never reaches the spawned
# agent's booted prompt" hole.
#
# THE FAILURE MODE (proven upstream 2026-08-06, 25 rejected deliverables):
#   `.claude/agents/*.md` teammate definitions are loaded ONCE, at SESSION
#   START — exactly like hooks. Install or update a definition mid-session and
#   the file on disk changes, the roster line changes, everyone's mental model
#   changes — but a teammate spawned later in that SAME session still boots on
#   the STALE definition the harness read at session start. Nothing in the
#   spawn payload, the tool result, or the agent's own transcript says so. In
#   the upstream production project this engine was extracted from, a teammate's
#   definition was upgraded (v2.0 -> v2.1) and the work dispatched in the very
#   same session: batch 1 happened to notice its booted prompt was v2.0-shaped
#   and self-corrected; batches 2-4 did not, drafted 25 outputs under the stale
#   v2.0 contract, and every one had to be thrown away. That project's incident
#   write-up does not ship with this engine — the lesson does.
#   The "How to Delegate" section of your CLAUDE.md states the rule; this guard
#   is the structure that makes forgetting it impossible.
#
# THE CONTRACT
#   Partner hook: scripts/hooks/snapshot-agent-definitions.sh (SessionStart)
#   records `sha256  <path>` for every definition at session start into
#   .claude/state/agent-definitions-<session8>.snapshot. At every Agent spawn
#   this guard compares that recorded hash against the CURRENT on-disk hash of
#   the definition being spawned:
#
#     MODIFIED (hashes differ)   -> BLOCK. The booted prompt is provably the
#                                   pre-edit definition. Message names both
#                                   hashes and the two sanctioned paths.
#     ACKED    (definition-drift-ack: <sha256> in the prompt, matching the
#               CURRENT on-disk hash)
#                                -> ALLOW + append to
#                                   .claude/state/definition-drift-acks.log
#                                   (mirrors the resume-acks.log pattern).
#     CREATED  (no snapshot entry — the definition did not exist at session
#               start; the creator-teammate-hires-then-orchestrator-spawns
#               flow, which has historically worked same-session)
#                                -> ALLOW + warn + log. See "OPEN QUESTION".
#     UNCHANGED                  -> ALLOW silently.
#     NO DEFINITION FILE (built-in types: Explore/Plan/general-purpose/claude/
#               claude-code-guide/statusline-setup, or any type with no
#               `.claude/agents/<type>.md`)
#                                -> ALLOW silently.
#     NO SNAPSHOT / UNPARSEABLE PAYLOAD / NO python3 / NO sha256 tool
#                                -> ALLOW + warn. Never block.
#
# FAIL-OPEN BY DESIGN — AND WHY THAT IS NOT A WEAKNESS HERE:
#   The sibling guards (guard-worktree-isolation.sh, guard-resume-isolation.sh)
#   fail CLOSED, because each of those can evaluate its contract from the
#   payload alone: an unparseable spawn is by definition not a proven-correct
#   spawn. This guard is different in kind — it can only ever
#   PROVE DRIFT (a hash mismatch against a recorded baseline); it can never
#   prove freshness. With no snapshot to compare against there is no evidence of
#   anything, and blocking every spawn on "no evidence" would halt the whole
#   team the first session after this hook lands (when no snapshot exists yet)
#   and on every machine where the state dir is unwritable. So: block ONLY on
#   positive proof of drift; warn on everything indeterminate. The blocking
#   sibling guard runs FIRST in the same PreToolUse[Agent] chain and already
#   fails closed on an unparseable payload, so nothing is lost.
#
# OPEN QUESTION — NEW definitions fail OPEN, deliberately:
#   We know MODIFIED definitions do not reach a newly spawned agent (proven).
#   We do NOT have equally hard evidence for a definition file CREATED
#   mid-session: the creator teammate (CREATOR_TEAMMATE in
#   orchestration.config) hires and the orchestrator spawns the new hire in
#   the same session routinely, which suggests the harness resolves an
#   unknown subagent_type from disk on demand. Rather than block a flow with a
#   working track record on an untested theory, a created-since-snapshot
#   definition is ALLOWED with a warning line (logged to
#   .claude/state/definition-drift.log). If a fresh-hire spawn is ever observed
#   booting empty/stale, flip CREATED to BLOCK — the branch is isolated below.
#
# PreToolUse[Agent] payload (same shape guard-worktree-isolation.sh parses):
#   { "tool_name": "Agent",
#     "tool_input": { "subagent_type": "dev", "name": "dev-sonnet-c1",
#                     "prompt": "...", "isolation": "worktree", ... },
#     "session_id": "<uuid>" }
#
# Exit codes (Claude Code PreToolUse convention):
#   0  allowed (silently, or with a warning on stderr)
#   2  BLOCKED — provable definition drift with no matching ack
#
# TEST OVERRIDE: DEFINITION_DRIFT_ROOT forces the root directory (test-only;
# never set in a real session). The partner snapshot hook honours the same
# variable.
#
# NOTE: hooks are snapshotted per session; this guard takes effect from the
# NEXT session. It assumes nothing about being live in the session that adds it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail
# Payloads can exceed a pipe buffer. Matching pipelines must consume EOF:
# grep -q or head can give the producer SIGPIPE and reverse the verdict.

HOOK_TAG="(hook: scripts/hooks/guard-definition-drift.sh)"

# Subagent types the harness provides itself — they have no
# `.claude/agents/<type>.md`, so there is nothing to drift. Listed explicitly
# (rather than relying on the file-existence check alone) so the common case
# short-circuits before any filesystem or state work.
BUILTIN_TYPES="Explore Plan general-purpose claude claude-code-guide statusline-setup"

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
  hook: scripts/hooks/guard-definition-drift.sh
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

INPUT="$(cat 2>/dev/null || true)"

# --- warn: stderr + a durable, de-duplicated log line --------------------
# Warnings never block (exit stays 0). The log makes them survivable evidence
# rather than transcript-only noise, mirroring resume-acks.log / main-checkout-
# runs.log. Dedup keys on CONTENT ONLY (never the timestamp), because a
# double-registered hook fires twice and two sequential subprocess spawns can
# straddle a UTC-second boundary — the same race already fixed in
# guard-resume-isolation.sh's resume-acks.log append path.
append_log() { # <logfile> <content-key>
    local logfile="$1" key="$2" last new
    mkdir -p "$(dirname "$logfile")" 2>/dev/null || true
    new="$(printf '%s\t%s' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$key")"
    last=""
    [ -f "$logfile" ] && last="$(tail -n 1 "$logfile" 2>/dev/null | cut -f2- || true)"
    if [ "$key" != "$last" ]; then
        printf '%s\n' "$new" >>"$logfile" 2>/dev/null || true
    fi
}

# THIS WARNING WAS NEVER HEARD, AND THAT WAS MEASURED RATHER THAN SUSPECTED.
# Until 2026-09-05 it wrote to stderr alone. A PreToolUse hook exiting 0 has its
# stderr filed into the transcript and rendered to NOBODY — the same fact
# scripts/lib/stop-hook-notice.sh established for Stop hooks, extended to this
# event by a live probe recorded in
# docs/verification/unevaluated-payload-notice-2026-09-05.md: of two markers
# emitted by one PreToolUse[Bash] hook, only the stdout {"systemMessage":...}
# reached the operator stream and the stderr one appeared nowhere in it.
#
# So this guard was cited as the template for making an unevaluated call
# audible while being, on the only channel that carries, inaudible itself. The
# banner is unchanged and still goes to stderr; the same text now also goes out
# as a systemMessage, which carries no `permissionDecision` and therefore
# changes no verdict.
warn_allow() { # <one-line reason> [<log-key or empty>]
    local _wa_msg
    _wa_msg="Definition-drift guard: WARNING (allowed) — $1 $HOOK_TAG"
    {
        echo "=== Definition-drift guard: WARNING (allowed) ==="
        echo "  $1"
        echo "$HOOK_TAG"
    } >&2
    if command -v python3 >/dev/null 2>&1; then
        WA_MSG="$_wa_msg" python3 -c '
import json, os
print(json.dumps({"systemMessage": os.environ.get("WA_MSG", "")}))
' 2>/dev/null || true
    fi
    if [ -n "${2:-}" ] && [ -n "${STATE_DIR:-}" ]; then
        append_log "$STATE_DIR/definition-drift.log" "$2"
    fi
    exit 0
}

# --- payload parse -------------------------------------------------------
# No python3 -> cannot parse reliably -> allow (see FAIL-OPEN above). The
# sibling guard-worktree-isolation.sh in this same chain already refuses the
# spawn outright when python3 is missing.
if ! command -v python3 >/dev/null 2>&1; then
    warn_allow "python3 unavailable — cannot verify agent-definition freshness for this spawn. Definition drift is NOT being checked."
fi

PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    if not isinstance(d, dict):
        raise ValueError("payload not an object")
    if str(d.get("tool_name", "") or "") != "Agent":
        print("NOTAGENT\t\t\t\t")
        sys.exit(0)
    ti = d.get("tool_input", {})
    if not isinstance(ti, dict):
        raise ValueError("tool_input not an object")
    st = str(ti.get("subagent_type", "") or "")
    nm = str(ti.get("name", "") or "")
    sid = str(d.get("session_id", "") or "")
    pr = str(ti.get("prompt", "") or "")
    pr = pr.replace("\t", " ").replace("\n", "\x01")
    print("OK\t%s\t%s\t%s\t%s" % (st, nm, sid, pr))
except Exception:
    print("PARSEFAIL\t\t\t\t")
' 2>/dev/null || printf 'PARSEFAIL\t\t\t\t')"

STATUS="$(printf '%s' "$PARSED" | cut -f1)"
SUBAGENT_TYPE="$(printf '%s' "$PARSED" | cut -f2)"
NAME="$(printf '%s' "$PARSED" | cut -f3)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f4)"
PROMPT="$(printf '%s' "$PARSED" | cut -f5- | tr '\001' '\n')"

# A well-formed payload for a different tool passes through untouched.
[ "$STATUS" = "NOTAGENT" ] && exit 0

# --- root + state resolution --------------------------------------------
# THIS is the guard the step-1 audit caught writing .claude/state/ into a
# repository that merely happened to CONTAIN the engine. It resolved from its
# own on-disk location, so as a plugin it deposited state wherever it landed.
# DEFINITION_DRIFT_ROOT is still honoured (the partner snapshotter and the
# existing suites use it) by feeding the contract's own override.
if [ -n "${DEFINITION_DRIFT_ROOT:-}" ]; then
    RICHOS_ENTITY_ROOT="$DEFINITION_DRIFT_ROOT"
fi

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
    root_failure_banner "scripts/hooks/guard-definition-drift.sh" >&2
    exit 2
fi

STATE_DIR="$ENTITY_ROOT/.claude/state"

# An Agent payload we cannot parse: the sibling preventer blocks it; we warn.
if [ "$STATUS" = "PARSEFAIL" ]; then
    warn_allow "could not parse the Agent spawn payload — agent-definition freshness NOT verified for this spawn." \
        "$(printf 'parsefail\tunparseable Agent payload — definition freshness not verified')"
fi

# --- built-in / definition-less types pass silently ----------------------
[ -n "$SUBAGENT_TYPE" ] || exit 0
for b in $BUILTIN_TYPES; do
    [ "$SUBAGENT_TYPE" = "$b" ] && exit 0
done

# Strip any plugin namespace before the lookup: `richos-engine:clark` can
# never match `.claude/agents/richos-engine:clark.md`, so the drift check
# silently compared nothing at all for every plugin-supplied teammate.
SUBAGENT_BARE="$(strip_agent_namespace "$SUBAGENT_TYPE")"
DEF_PATH="$(resolve_agent_def "$ENTITY_ROOT" "$ENGINE_ROOT" "$SUBAGENT_TYPE")"
DEF_RC=$?
if [ "$DEF_RC" -eq 2 ]; then
    warn_allow "subagent_type '${SUBAGENT_TYPE}' is namespaced but no definition for it was found under the entity roster, the engine's own roster, or AGENT_NAMESPACE_ROOTS — definition freshness is NOT being checked for this spawn." \
        "$(printf 'unresolvable-namespace\tagent=%s' "$SUBAGENT_TYPE")"
fi
[ -n "$DEF_PATH" ] || DEF_PATH="$ENTITY_ROOT/.claude/agents/${SUBAGENT_BARE}.md"
# Snapshot paths are recorded repo-relative, so the comparison key has to be
# repo-relative too — and relative to whichever root the definition came from.
case "$DEF_PATH" in
    "$ENTITY_ROOT"/*) DEF_REL="${DEF_PATH#"$ENTITY_ROOT"/}" ;;
    "$ENGINE_ROOT"/*) DEF_REL="${DEF_PATH#"$ENGINE_ROOT"/}" ;;
    *)                DEF_REL=".claude/agents/${SUBAGENT_BARE}.md" ;;
esac

# --- snapshot resolution -------------------------------------------------
# Session-scoped ONLY when a session id is present: falling back to `latest`
# there could compare this session's spawn against a DIFFERENT session's
# baseline (two concurrent sessions), inventing drift that never happened.
# The `latest` symlink is used only when the payload carries no session id at
# all — the best available handle, and still only ever able to prove drift.
SNAP_PATH=""
SESSION_SHORT="$(printf '%s' "$SESSION_ID" | tr -cd '[:alnum:]-' | cut -c1-8)"
if [ -n "$SESSION_SHORT" ]; then
    CAND="$STATE_DIR/agent-definitions-${SESSION_SHORT}.snapshot"
    [ -f "$CAND" ] && SNAP_PATH="$CAND"
else
    CAND="$STATE_DIR/agent-definitions-latest.snapshot"
    [ -f "$CAND" ] && SNAP_PATH="$CAND"
fi

if [ -z "$SNAP_PATH" ]; then
    warn_allow "no session-start definition snapshot found for session '${SESSION_ID:-<unset>}' (expected .claude/state/agent-definitions-${SESSION_SHORT:-latest}.snapshot). snapshot-agent-definitions.sh was probably not live at the last session start — hooks snapshot per session, so it activates from the NEXT one. Definition drift is NOT being checked for this spawn." \
        "$(printf 'nosnapshot\tsession=%s\tagent=%s' "${SESSION_ID:-<unset>}" "$SUBAGENT_TYPE")"
fi

# --- hashes --------------------------------------------------------------
sha256_of() {
    local p="$1"
    [ -f "$p" ] || { echo ""; return; }
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$p" 2>/dev/null | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$p" 2>/dev/null | awk '{print $1}'
    else
        python3 -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$p" 2>/dev/null
    fi
}

# Snapshot lookup: first field of the line whose second field is DEF_REL.
# `#`-prefixed header lines are skipped by the field match itself.
SNAP_HASH="$(awk -v want="$DEF_REL" '
    $1 ~ /^#/ { next }
    $2 == want { print $1; exit }
' "$SNAP_PATH" 2>/dev/null || true)"

CUR_HASH="$(sha256_of "$DEF_PATH")"

# Definition file gone. If it was in the snapshot, that is worth surfacing;
# either way there is no current content to compare, so never block.
if [ -z "$CUR_HASH" ]; then
    if [ -n "$SNAP_HASH" ]; then
        warn_allow "definition $DEF_REL was present at session start but is MISSING on disk now — the spawn will boot on the session-start copy. Verify this is intended." \
            "$(printf 'missing\tagent=%s\t%s deleted since session start' "$SUBAGENT_TYPE" "$DEF_REL")"
    fi
    exit 0
fi

# --- CREATED since session start: allow + warn (see OPEN QUESTION) -------
if [ -z "$SNAP_HASH" ]; then
    {
        echo "=== Definition-drift guard: NEW DEFINITION (allowed) ==="
        echo "  '$SUBAGENT_TYPE' has no entry in this session's start-of-session"
        echo "  snapshot — $DEF_REL was CREATED mid-session"
        echo "  (current sha256 ${CUR_HASH})."
        echo ""
        echo "  Allowed deliberately: the hire-then-spawn-in-the-same-session"
        echo "  flow has a working track record, which suggests an UNKNOWN"
        echo "  subagent_type is resolved from disk on demand. That is not proven"
        echo "  the way the MODIFIED case is, so if this fresh hire boots empty or"
        echo "  stale, say so — the created-vs-modified load semantics is an open"
        echo "  question we chose to fail OPEN on."
        echo "$HOOK_TAG"
    } >&2
    append_log "$STATE_DIR/definition-drift.log" \
        "$(printf 'created\tagent=%s\tname=%s\tsha256=%s' \
            "$SUBAGENT_TYPE" "${NAME:-<unset>}" "$CUR_HASH")"
    exit 0
fi

# --- UNCHANGED: the common case, silent ----------------------------------
[ "$SNAP_HASH" = "$CUR_HASH" ] && exit 0

# --- MODIFIED: ack or block ----------------------------------------------
# definition-drift-ack: <sha256> — a live prompt line whose hash token matches
# the CURRENT on-disk definition. Matching a >=16-char prefix counts (the full
# 64 obviously does), so an operator can paste the short hash a report quotes;
# anything shorter is too weak to prove the ack refers to THIS content.
ACK_LINE=""
ACK_TOKEN=""
if printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*definition-drift-ack:[[:space:]]*[0-9a-fA-F]+'; then
    ACK_LINE="$(printf '%s' "$PROMPT" \
        | grep -oE '^[[:space:]]*definition-drift-ack:[[:space:]]*.+' | sed -n '1p' \
        | sed -E 's/^[[:space:]]*//')"
    ACK_TOKEN="$(printf '%s' "$ACK_LINE" \
        | sed -E 's/^definition-drift-ack:[[:space:]]*//' \
        | grep -oE '^[0-9a-fA-F]+' | sed -n '1p' | tr '[:upper:]' '[:lower:]' || true)"
fi

if [ -n "$ACK_TOKEN" ] && [ "${#ACK_TOKEN}" -ge 16 ] && [ "${CUR_HASH#"$ACK_TOKEN"}" != "$CUR_HASH" ]; then
    append_log "$STATE_DIR/definition-drift-acks.log" \
        "$(printf 'agent=%s\tname=%s\tsnapshot=%s\tcurrent=%s\t%s' \
            "$SUBAGENT_TYPE" "${NAME:-<unset>}" "$SNAP_HASH" "$CUR_HASH" "$ACK_LINE")"
    {
        echo "=== Definition-drift guard: ACK accepted (allowed) ==="
        echo "  '$SUBAGENT_TYPE' definition changed since session start; the spawn"
        echo "  carries a matching definition-drift-ack. The booted system prompt is"
        echo "  STILL the session-start copy — the spawn prompt must order this agent"
        echo "  to read $DEF_REL on disk and follow it as authoritative."
        echo "  Logged to .claude/state/definition-drift-acks.log"
        echo "$HOOK_TAG"
    } >&2
    exit 0
fi

{
    echo "=== Definition-drift guard: BLOCKED ==="
    echo "  The '$SUBAGENT_TYPE' definition has been MODIFIED since this session"
    echo "  started, so this spawn would boot on the STALE session-start copy —"
    echo "  not the definition now on disk."
    echo ""
    echo "    file:            $DEF_REL"
    echo "    session-start:   $SNAP_HASH   <- what the agent would ACTUALLY boot on"
    echo "    current on disk: $CUR_HASH   <- what you think you are spawning"
    if [ -n "$ACK_TOKEN" ]; then
        echo ""
        echo "  A definition-drift-ack was present but does NOT match the current"
        echo "  hash (acked: ${ACK_TOKEN}). Acks must quote the CURRENT sha256 —"
        echo "  a stale ack proves nothing."
    fi
    echo ""
    echo "  Subagent definitions load ONCE, at SESSION START, exactly like hooks."
    echo "  This is the 2026-08-06 upstream incident that cost 25 rejected"
    echo "  deliverables: a definition upgraded mid-session, the work dispatched"
    echo "  in that same session, three of four batches drafted under the stale"
    echo "  contract."
    echo ""
    echo "  Two sanctioned paths:"
    echo "    (a) PREFERRED — restart into a FRESH session, then spawn. The new"
    echo "        session snapshots the current definitions and this agent boots"
    echo "        on the real thing, with no workaround in the prompt."
    echo "    (b) SAME-SESSION, eyes open — add a live spawn-prompt line:"
    echo "          definition-drift-ack: $CUR_HASH"
    echo "        AND an explicit order in the prompt to read $DEF_REL"
    echo "        on disk IN FULL and follow it as the governing definition"
    echo "        (its booted prompt is the stale copy). Acceptance must then"
    echo "        verify a fingerprint unique to the NEW definition — style"
    echo "        checks alone do not prove contract compliance."
    echo "$HOOK_TAG"
} >&2
exit 2
