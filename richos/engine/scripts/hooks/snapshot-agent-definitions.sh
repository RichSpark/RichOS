#!/usr/bin/env bash
#
# snapshot-agent-definitions.sh — SessionStart hook. Records the sha256 of
# every `.claude/agents/*.md` teammate definition AS OF SESSION START, so the
# PreToolUse[Agent] partner guard (guard-definition-drift.sh) can prove, at
# spawn time, whether a definition has been installed/updated since the
# harness took its own snapshot.
#
# THE FAILURE MODE THIS PAIR EXISTS TO CLOSE (proven upstream, 2026-08-06):
#   Subagent definitions in `.claude/agents/*.md` are loaded ONCE, at SESSION
#   START — exactly like hooks. A definition installed or updated MID-SESSION
#   therefore NEVER reaches a newly spawned agent's BOOTED system prompt: the
#   harness boots that agent on the definition it read at session start, while
#   the file on disk (and everyone's mental model) says otherwise. In the
#   upstream production project this engine was extracted from, that cost 25
#   rejected deliverables on 2026-08-06: a teammate's definition was upgraded
#   (v2.0 -> v2.1) and the work was dispatched in the SAME session, so three of
#   four batches silently drafted under the stale v2.0 contract and every one
#   had to be thrown away. That project's incident write-up does not ship with
#   this engine — the lesson does, and this pair is the lesson made structural.
#   Doctrine (the "How to Delegate" section of your CLAUDE.md) is advisory and
#   degrades silently. This hook pair makes the drift STRUCTURALLY visible at
#   the spawn boundary instead.
#
# WHAT THIS HOOK WRITES
#   .claude/state/agent-definitions-<session8>.snapshot   (session-scoped)
#   .claude/state/agent-definitions-latest.snapshot       (symlink -> newest)
#
#   Format — one record per definition file, `shasum -a 256`-shaped, plus two
#   leading `#` comment lines that any consumer skips:
#     # agent-definition snapshot — written by scripts/hooks/snapshot-agent-definitions.sh
#     # session=<id|unknown> generated=<iso8601Z> root=<abs> count=<n>
#     <sha256>  .claude/agents/ace.md
#     <sha256>  .claude/agents/andy.md
#     ...
#   Paths are REPO-RELATIVE so a snapshot is comparable across checkouts.
#
# WHERE IT READS FROM: the TRUE main checkout, resolved via
# scripts/lib/resolve-main-checkout.sh (the same helper the sibling hooks
# use), so a copy of this script running from a linked worktree
# still snapshots — and the partner guard still compares against — the ONE
# shared `.claude/agents/` the harness actually boots teammates from.
#
# SESSION ID RESOLUTION (first hit wins):
#   1. --session <id>            (explicit, used by tests)
#   2. $CLAUDE_SESSION_ID        (if the harness exports it)
#   3. session_id in the SessionStart JSON payload on stdin
#   4. none -> a timestamped filename; the `latest` symlink is the only handle
#   Only the first 8 characters are used, matching the session-<first8>
#   convention the team dir and the sibling guards already use.
#
# LOG-ONLY / NEVER BLOCKS: like every other SessionStart hook. Every
# failure mode (no `.claude/agents/` dir, unwritable state dir, missing
# python3, absent stdin) is swallowed — this hook ALWAYS exits 0 and NEVER
# holds up a session start. A missing snapshot is handled by the partner
# guard as "cannot prove drift -> allow + warn", never as a block.
#
# RETENTION: the newest SNAPSHOT_RETAIN (20) snapshot files are kept; older
# ones are pruned so .claude/state/ doesn't grow one file per session forever.
# The `latest` symlink target is never pruned.
#
# TEST OVERRIDE: DEFINITION_DRIFT_ROOT forces the root directory (test-only;
# never set in a real session). The partner guard honours the same variable.
#
# NOTE: hooks are snapshotted per session, so this hook takes effect from the
# NEXT session. It assumes nothing about being live in the session that adds it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

SNAPSHOT_RETAIN=20

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
  hook: scripts/hooks/snapshot-agent-definitions.sh
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

# --- explicit session override (tests) -----------------------------------
SESSION_ID=""
if [ "${1:-}" = "--session" ] && [ -n "${2:-}" ]; then
    SESSION_ID="$2"
fi

# --- root resolution -----------------------------------------------------
# This hook resolved to the enclosing repository under a plugin load, found no
# .claude/agents there, and emitted "skipped — ... (definition-drift guard will
# fail open)". The partner guard then DID fail open, exactly as advertised, and
# nothing about that chain was visible as a failure. It is now.
if [ -n "${DEFINITION_DRIFT_ROOT:-}" ]; then
    RICHOS_ENTITY_ROOT="$DEFINITION_DRIFT_ROOT"
fi

# NOTE ON STDIN — this hook reads the payload LATE and CONDITIONALLY.
#
# It is a SessionStart hook AND a plain CLI tool, and in the CLI case stdin is
# an inherited pipe that nobody closes, so an unconditional `cat` hangs forever
# (`[ ! -t 0 ]` does not help: an inherited pipe is not a TTY). Measured: 92
# seconds and counting, inside the contract-integrity probe, before this was
# reverted.
#
# It costs nothing, because the payload's `cwd` is a REDUNDANT resolution
# candidate here: CLAUDE_PROJECT_DIR is measured present and correct in a
# plugin-loaded hook's environment at SessionStart (probe, 2026-08-28), and it
# outranks the payload cwd anyway. Paying a hang risk for a candidate that
# never wins is a bad trade.
#
# It does still read stdin further down, for the session id — but only under
# its original condition (no --session was supplied). Root resolution happens
# first and needs nothing from stdin.
#
# CORRECTION, 2026-09-05 — that condition was described here as "what kept the
# hazard unreachable". It did not. The no---session case is EXACTLY the real
# SessionStart firing, and any script invoking this hook with an inherited,
# never-closed stdin reaches it too. Measured on unmodified main: run with an
# open, never-closed stdin and no --session, this hook never returned; the same
# run with stdin closed finished in under a second. The suite did not catch it
# because its only hang case passes `--session`, which is the one invocation
# that cannot reach the read — a test passing for the wrong reason.
#
# The read below is therefore BOUNDED, not merely conditional. A SessionStart
# hook that blocks holds the WHOLE session for as long as it blocks: measured
# against the shipped binary (claude 2.1.261), 602 seconds, with no rescue
# timeout on that path.

ROOT_FAILURE=""
if resolve_entity_root ""; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    ENTITY_ROOT=""
else
    ENTITY_ROOT=""
    ROOT_FAILURE="$(root_failure_banner "scripts/hooks/snapshot-agent-definitions.sh")"
    printf '%s\n' "$ROOT_FAILURE" >&2
fi

AGENTS_DIR="$ENTITY_ROOT/.claude/agents"
STATE_DIR="$ENTITY_ROOT/.claude/state"

# --- session id ----------------------------------------------------------
if [ -z "$SESSION_ID" ] && [ -n "${CLAUDE_SESSION_ID:-}" ]; then
    SESSION_ID="$CLAUDE_SESSION_ID"
fi
if [ -z "$SESSION_ID" ] && [ ! -t 0 ]; then
    # SessionStart payload on stdin: {"session_id":"...","hook_event_name":...}
    #
    # BOUNDED READ, NEVER `cat`. Three details are load-bearing, and all three
    # were measured on bash 3.2.57 — what `/usr/bin/env bash` resolves to on
    # macOS — rather than assumed from the manual:
    #   * `-d ''` reads to EOF instead of to the first newline, so the whole
    #     JSON payload arrives. It also returns NONZERO on a complete read
    #     (the NUL delimiter is never found), so the VALUE is judged below,
    #     never `$?`.
    #   * on timeout, bash 3.2 leaves the variable UNMODIFIED rather than
    #     clearing it, so it is initialized to empty first.
    #   * the real firing closes stdin in 3-6ms (measured against the shipped
    #     binary), so the ceiling costs nothing and 2s is ~400x the margin.
    # An empty STDIN_JSON degrades to a timestamped snapshot, which the code
    # below already handles — the hook loses precision, never the session.
    STDIN_JSON=""
    IFS= read -r -t "${RICHOS_HOOK_STDIN_TIMEOUT:-2}" -d '' STDIN_JSON || true
    if [ -n "$STDIN_JSON" ] && command -v python3 >/dev/null 2>&1; then
        SESSION_ID="$(printf '%s' "$STDIN_JSON" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    print(str(d.get("session_id", "") or "") if isinstance(d, dict) else "")
except Exception:
    print("")
' 2>/dev/null || true)"
    fi
    if [ -z "$SESSION_ID" ] && [ -n "$STDIN_JSON" ]; then
        # python3-free fallback: first "session_id":"..." occurrence.
        SESSION_ID="$(printf '%s' "$STDIN_JSON" | tr '\n' ' ' \
            | grep -o '"session_id"[[:space:]]*:[[:space:]]*"[^"]*"' \
            | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)"
    fi
fi

SESSION_SHORT="$(printf '%s' "$SESSION_ID" | tr -cd '[:alnum:]-' | cut -c1-8)"

# --- sha256 helper (same precedence as install.sh) -----------------------
sha256_of() {
    local p="$1"
    [ -f "$p" ] || { echo ""; return; }
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$p" 2>/dev/null | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$p" 2>/dev/null | awk '{print $1}'
    elif command -v python3 >/dev/null 2>&1; then
        python3 -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$p" 2>/dev/null
    else
        echo ""
    fi
}

emit_context() { # <summary>
    local summary="$1"
    if command -v python3 >/dev/null 2>&1; then
        SUMMARY="$summary" python3 - <<'PY' 2>/dev/null || true
import json, os
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": os.environ.get("SUMMARY", ""),
    }
}))
PY
    else
        local escaped="${summary//\\/\\\\}"
        escaped="${escaped//\"/\\\"}"
        printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$escaped"
    fi
}

# --- nothing to snapshot, and WHY ----------------------------------------
# The old code collapsed three unrelated situations into the single word
# "skipped": the root could not be resolved, the repo never adopted the engine,
# or the repo genuinely has no roster. Only the first is a failure, and it was
# the one that actually happened — so the one real failure was wearing the
# wording of a routine no-op. Each now says what it is.
if [ -n "$ROOT_FAILURE" ]; then
    emit_context "ROOT RESOLUTION FAILURE — the agent-definition snapshotter could not resolve the repository it governs, so NO baseline was written and guard-definition-drift.sh will fail OPEN for every spawn this session. ${RICHOS_ROOT_REASON}"
    exit 0
fi
if [ -z "$ENTITY_ROOT" ]; then
    emit_context "agent-definition snapshot: not run — this repository has not adopted the engine (no orchestration.config at its root). No definitions are being tracked here."
    exit 0
fi
if [ ! -d "$AGENTS_DIR" ]; then
    # A GOVERNED repository with no roster. Legitimate for a repo that has not
    # written its agents yet, but it is stated as a live consequence rather
    # than as a skip, because the partner guard degrades silently on the back
    # of it.
    emit_context "agent-definition snapshot: NO BASELINE WRITTEN — $AGENTS_DIR does not exist in the governed repository ($ENTITY_ROOT). guard-definition-drift.sh will fail OPEN for every spawn this session."
    exit 0
fi

mkdir -p "$STATE_DIR" 2>/dev/null || true
if [ ! -d "$STATE_DIR" ]; then
    emit_context "agent-definition snapshot: NO BASELINE WRITTEN — could not create $STATE_DIR in the governed repository ($ENTITY_ROOT). guard-definition-drift.sh will fail OPEN for every spawn this session."
    exit 0
fi

# --- build the snapshot --------------------------------------------------
if [ -n "$SESSION_SHORT" ]; then
    SNAP_NAME="agent-definitions-${SESSION_SHORT}.snapshot"
else
    SNAP_NAME="agent-definitions-$(date -u +%Y%m%dT%H%M%SZ).snapshot"
fi
SNAP_PATH="$STATE_DIR/$SNAP_NAME"
TMP_PATH="$SNAP_PATH.tmp.$$"

COUNT=0
{
    printf '# agent-definition snapshot — written by scripts/hooks/snapshot-agent-definitions.sh\n'
    printf '# session=%s generated=%s root=%s\n' \
        "${SESSION_ID:-unknown}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$ENTITY_ROOT"
} >"$TMP_PATH" 2>/dev/null || true

if [ -f "$TMP_PATH" ]; then
    # Sorted, deterministic ordering so two snapshots of identical content are
    # byte-identical (makes an eyeball diff meaningful).
    while IFS= read -r def; do
        [ -n "$def" ] || continue
        h="$(sha256_of "$def")"
        [ -n "$h" ] || continue
        rel="${def#"$ENTITY_ROOT"/}"
        printf '%s  %s\n' "$h" "$rel" >>"$TMP_PATH" 2>/dev/null || true
        COUNT=$((COUNT + 1))
    done < <(find "$AGENTS_DIR" -maxdepth 1 -type f -name '*.md' 2>/dev/null | LC_ALL=C sort)

    mv -f "$TMP_PATH" "$SNAP_PATH" 2>/dev/null || rm -f "$TMP_PATH" 2>/dev/null || true
fi

if [ ! -f "$SNAP_PATH" ]; then
    emit_context "agent-definition snapshot: FAILED to write $SNAP_PATH (definition-drift guard will fail open)"
    exit 0
fi

# --- refresh the `latest` handle -----------------------------------------
# Relative symlink so the state dir stays relocatable. Falls back to a plain
# copy on a filesystem that refuses symlinks.
LATEST="$STATE_DIR/agent-definitions-latest.snapshot"
rm -f "$LATEST" 2>/dev/null || true
ln -s "$SNAP_NAME" "$LATEST" 2>/dev/null || cp -f "$SNAP_PATH" "$LATEST" 2>/dev/null || true

# --- retention -----------------------------------------------------------
# Keep the newest $SNAPSHOT_RETAIN real snapshot files; never touch the
# `latest` handle or the file it points at.
if command -v ls >/dev/null 2>&1; then
    OLD="$(ls -t "$STATE_DIR"/agent-definitions-*.snapshot 2>/dev/null \
        | grep -v '/agent-definitions-latest\.snapshot$' \
        | tail -n +$((SNAPSHOT_RETAIN + 1)) || true)"
    if [ -n "$OLD" ]; then
        while IFS= read -r stale; do
            [ -n "$stale" ] || continue
            [ "$stale" = "$SNAP_PATH" ] && continue
            rm -f "$stale" 2>/dev/null || true
        done <<<"$OLD"
    fi
fi

emit_context "agent-definition snapshot: recorded $COUNT definition(s) at session start -> .claude/state/$SNAP_NAME (guard-definition-drift.sh blocks a spawn whose definition changed since)"
exit 0
