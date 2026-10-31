#!/usr/bin/env bash
#
# snapshot-enforcing-hooks.sh — SessionStart hook. Records WHICH HOOKS THIS
# SESSION ACTUALLY BOOTED WITH, so the partner Stop hook
# (notice-hook-staleness.sh) can tell the operator, mid-session, that a guard
# landed since then is sitting on disk enforcing nothing.
#
# ===========================================================================
# THE FAILURE THIS PAIR EXISTS TO CLOSE (2026-08-30, this operation)
# ===========================================================================
# Six guards were landed in one day. The lander knew they were inert until the
# session restarted, and said so — in the form "they arm at next session start".
# That sentence describes a DATE. It does not describe an ACTION, and it does
# not name an ACTOR. The operator read it as a thing that would happen to him
# rather than a thing he could do, in five seconds, at any moment. Nobody told
# him that restarting was available RIGHT NOW. He then hit a failure that three
# of those six guards would have caught.
#
# Nothing was wrong with the guards. Nothing was even factually wrong with the
# sentence. What was missing was that a deferred activation had been reported as
# a forecast instead of a request, so no one acted on it.
#
#   A DEFERRED ACTIVATION MUST NAME THE ACTOR AND THE ACTION.
#   Never "this arms at the next session" — always "restart the session to arm
#   this; that is the operator's to do." A state change that requires a human
#   action is not a date, it is a request.
#
# ===========================================================================
# WHAT IS AND IS NOT FROZEN AT SESSION START — MEASURED, NOT ASSUMED
# ===========================================================================
# The comparison this pair makes is only honest if it compares the ONE surface
# that is genuinely frozen. Both surfaces were probed against the shipping
# binary (2.1.251) on 2026-08-30, each with a negative control that had to fire
# before the result counted:
#
#   PLUGIN hooks/hooks.json ............ FROZEN AT SESSION START.
#     A plugin was installed from a directory marketplace and loaded; a Bash
#     PostToolUse hook from it fired (negative control, three times). Its
#     hooks.json was then rewritten mid-session to add a second hook. The new
#     hook NEVER fired. The binary agrees: plugin hooks re-register only when
#     PLUGIN-AFFECTING SETTINGS change (enabledPlugins, pluginConfigs, trust,
#     marketplaces, the disable switches) — a subscription on `policySettings`
#     that compares a snapshot of exactly those keys. The plugin's OWN hook
#     table is not in that snapshot, so editing it triggers nothing. The
#     binary's operator-facing string for this case says: "edits there take
#     effect after /reload-plugins".
#
#   .claude/settings.local.json ........ HOT-RELOADS MID-SESSION.
#     Same three-call shape, same session: a second hook appended to a project
#     settings file DID fire on the very next tool call. The binary's own
#     hook-authoring guidance explains the boundary — the settings watcher
#     "only watches directories that had a settings file when this session
#     started", and an adopted repo's .claude/ always did.
#
# THIS IS WHY ONLY THE PLUGIN SURFACE IS SNAPSHOTTED. Including the settings
# surface would have produced a confident, well-formatted false positive on
# every settings edit — reporting as inert a hook that was already enforcing.
# The engine registers the same guards on both surfaces, so the temptation to
# "check both for completeness" was real and would have been wrong.
#
# HOOK SCRIPT BODIES ARE DELIBERATELY NOT SNAPSHOTTED either, for the same
# reason and with the same evidence: a registration names `bash <path>`, and
# that path is executed afresh on every event. Edit a registered guard's body
# mid-session and the next fire runs the new body. Reporting a body edit as
# "inert" would be a false positive. Only the REGISTRATION is frozen, so only
# the registration is compared.
#
# ===========================================================================
# WHAT THIS HOOK WRITES
# ===========================================================================
#   .claude/state/enforcing-hooks-<session8>.snapshot   (session-scoped)
#   .claude/state/enforcing-hooks-latest.snapshot       (symlink -> newest)
#
#   Two `#` header lines any consumer skips, then one TAB-separated row per
#   registered hook script, sorted:
#     # enforcing-hook snapshot — written by scripts/hooks/snapshot-enforcing-hooks.sh
#     # session=<id|unknown> generated=<iso8601Z> engine=<abs> surface=hooks/hooks.json rows=<n>
#     PreToolUse<TAB>Agent<TAB>guard-worktree-isolation.sh
#     SessionStart<TAB>-<TAB>engine-status.sh
#     ...
#
#   `rows=<n>` in the header is the partner's NEGATIVE CONTROL anchor. The
#   comparer re-counts the rows it parsed and refuses to report "no drift"
#   unless the count agrees and is non-zero — so a green run can never mean
#   "clean because nothing was read". This engine has already shipped one
#   scanner that reported CLEAN over an empty corpus; the count is there so
#   there is not a second.
#
#   NEITHER SIDE IS EVER TYPED. Both the baseline written here and the current
#   set read by the partner come from registered_hook_rows() over the same
#   hooks/hooks.json. A typed list of 14 where the registration held 15 is the
#   drift that started this whole sequence, and it is not repeated here.
#
# WHERE IT READS FROM: the ENGINE root (the plugin root under a plugin load),
# because hooks/hooks.json is the engine's own registration surface. WHERE IT
# WRITES: the ENTITY root, the governed repository, alongside every other
# guard's state.
#
# SESSION ID RESOLUTION (first hit wins) — identical to
# snapshot-agent-definitions.sh, deliberately:
#   1. --session <id>        (explicit, used by tests)
#   2. $CLAUDE_SESSION_ID
#   3. session_id in the SessionStart JSON payload on stdin
#   4. none -> a timestamped filename; the `latest` symlink is the only handle
#
# LOG-ONLY / NEVER BLOCKS: always exits 0. Every failure mode (unresolvable
# root, unwritable state dir, missing python3, absent stdin) is survivable —
# but none of them is SILENT: each one is announced through the SessionStart
# context so "no baseline was written" can never look like "nothing drifted".
#
# RETENTION: the newest SNAPSHOT_RETAIN (20) snapshots are kept.
#
# TEST OVERRIDE: HOOK_STALENESS_ROOT forces the governed root and
# HOOK_STALENESS_SURFACE forces the hooks.json path (test-only; never set in a
# real session). The partner hook honours both.
#
# WHEN THIS PAIR ITSELF ARMS — the same rule it exists to enforce, applied to
# itself and stated as a request rather than a date: this hook is registered in
# hooks/hooks.json, which the host reads once at session start. In the session
# that MERGES it, it is inert and writes no baseline; the partner will say so
# rather than pretend. TO ARM IT: re-run scripts/hooks/install.sh, then RESTART
# THE SESSION. That is the operator's to do — no hook, and no model, can do it.

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
  hook: scripts/hooks/snapshot-enforcing-hooks.sh
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

# --- explicit source override (tests) -------------------------------------
# SessionStart carries a `source`, and it is the ONLY thing that distinguishes
# a new OS process from a `/clear` inside the one already running. See the
# WHY-THIS-IS-WRITE-ONCE block below for what turns on it.
SESSION_SOURCE="${HOOK_SNAPSHOT_SOURCE:-}"
if [ "${3:-}" = "--source" ] && [ -n "${4:-}" ]; then
    SESSION_SOURCE="$4"
fi

if [ -n "${HOOK_STALENESS_ROOT:-}" ]; then
    RICHOS_ENTITY_ROOT="$HOOK_STALENESS_ROOT"
fi

# NOTE ON STDIN — read LATE, CONDITIONALLY, and with a CEILING.
#
# This file is a SessionStart hook AND a plain CLI tool. Root resolution needs
# nothing from stdin, so it happens first, and the payload is read further down
# only when no --session was supplied.
#
# That condition was once believed to make an unbounded `cat` unreachable. It
# does not, and this is the correction of 2026-09-05: the no---session case is
# EXACTLY the real SessionStart firing, and any script invoking this hook with
# an inherited, never-closed stdin reaches it too. Measured on unmodified main:
# with an open, never-closed stdin this hook never returned, while the same run
# with stdin closed finished in under a second. `[ ! -t 0 ]` never helped — an
# inherited pipe is not a TTY, which is the whole reason the guard reads as if
# it were sufficient.
#
# So the read is BOUNDED (`read -t`, below) rather than merely conditional. A
# hook that needs no payload must not be stoppable by one, and a hook that does
# need it must not be able to wait forever for it.
#
# WHY THE CEILING MATTERS MORE HERE THAN ANYWHERE ELSE: a SessionStart hook
# that blocks holds the WHOLE session for as long as it blocks. Measured
# against the shipped binary (claude 2.1.261, 2026-09-05): a SessionStart hook
# blocking on an unclosed stdin held a headless session for 602 seconds, to the
# exact moment the writer let go. There is no rescue timeout on that path. The
# failure is also silent — with stdin already closed it exits 0 instantly,
# which is why this class survives unnoticed in a test suite.

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

ROOT_FAILURE=""
if resolve_entity_root ""; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    ENTITY_ROOT=""
else
    ENTITY_ROOT=""
    ROOT_FAILURE="$(root_failure_banner "scripts/hooks/snapshot-enforcing-hooks.sh")"
    printf '%s\n' "$ROOT_FAILURE" >&2
fi

# --- session id ----------------------------------------------------------
if [ -z "$SESSION_ID" ] && [ -n "${CLAUDE_SESSION_ID:-}" ]; then
    SESSION_ID="$CLAUDE_SESSION_ID"
fi
if [ -z "$SESSION_ID" ] && [ ! -t 0 ]; then
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
    # An empty STDIN_JSON degrades to a timestamped snapshot, which every
    # branch below already handles — the hook loses precision, never the
    # session.
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
        SESSION_ID="$(printf '%s' "$STDIN_JSON" | tr '\n' ' ' \
            | grep -o '"session_id"[[:space:]]*:[[:space:]]*"[^"]*"' \
            | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)"
    fi
    if [ -z "$SESSION_SOURCE" ] && [ -n "$STDIN_JSON" ]; then
        SESSION_SOURCE="$(printf '%s' "$STDIN_JSON" | tr '\n' ' ' \
            | grep -o '"source"[[:space:]]*:[[:space:]]*"[^"]*"' \
            | head -1 | sed 's/.*"\([^"]*\)"$/\1/' || true)"
    fi
fi
SESSION_SHORT="$(printf '%s' "$SESSION_ID" | tr -cd '[:alnum:]-' | cut -c1-8)"

# --- nothing to snapshot, and WHY ----------------------------------------
# Three unrelated situations that a single word like "skipped" would collapse.
# Only the first and last are failures, and each says which it is — because the
# partner hook degrades on the back of this one, and a degraded partner that
# looks like a clean partner is the entire failure class this engine exists to
# remove.
if [ -n "$ROOT_FAILURE" ]; then
    emit_context "ROOT RESOLUTION FAILURE — the enforcing-hook snapshotter could not resolve the repository it governs, so NO baseline was written and notice-hook-staleness.sh cannot tell you this session whether a landed guard is inert. ${RICHOS_ROOT_REASON}"
    exit 0
fi
if [ -z "$ENTITY_ROOT" ]; then
    emit_context "enforcing-hook snapshot: not run — this repository has not adopted the engine (no orchestration.config at its root). Nothing is being enforced here, so there is no enforcing set to track."
    exit 0
fi

SURFACE="${HOOK_STALENESS_SURFACE:-$ENGINE_ROOT/hooks/hooks.json}"
STATE_DIR="$ENTITY_ROOT/.claude/state"

# --- derive the enforcing set --------------------------------------------
# DERIVED, NEVER TYPED — see scripts/lib/registered-hooks.sh for the two
# incidents that made that non-negotiable.
_RH_LIB="$SCRIPT_DIR/../lib/registered-hooks.sh"
if [ ! -f "$_RH_LIB" ]; then
    emit_context "enforcing-hook snapshot: NO BASELINE WRITTEN — scripts/lib/registered-hooks.sh is missing at $_RH_LIB, so the enforcing set could not be derived. notice-hook-staleness.sh will report that it cannot compare, rather than reporting no drift."
    exit 0
fi
# shellcheck source=../lib/registered-hooks.sh
. "$_RH_LIB"

if ! ROWS="$(registered_hook_rows "$SURFACE")"; then
    emit_context "enforcing-hook snapshot: NO BASELINE WRITTEN — could not derive the enforcing set from $SURFACE (missing, unparseable, registering nothing, or no python3). notice-hook-staleness.sh will say it cannot compare this session. A landed-but-inert guard will NOT be reported until this is fixed and the session restarted."
    exit 0
fi

ROW_COUNT="$(printf '%s\n' "$ROWS" | grep -c . || true)"
if [ "${ROW_COUNT:-0}" -lt 1 ]; then
    emit_context "enforcing-hook snapshot: NO BASELINE WRITTEN — $SURFACE parsed but registered zero hook scripts. Refusing to record an empty baseline: every later comparison against it would report 'no drift' for the wrong reason."
    exit 0
fi

mkdir -p "$STATE_DIR" 2>/dev/null || true
if [ ! -d "$STATE_DIR" ]; then
    emit_context "enforcing-hook snapshot: NO BASELINE WRITTEN — could not create $STATE_DIR in the governed repository ($ENTITY_ROOT). notice-hook-staleness.sh will say it cannot compare this session."
    exit 0
fi

# --- write it ------------------------------------------------------------
if [ -n "$SESSION_SHORT" ]; then
    SNAP_NAME="enforcing-hooks-${SESSION_SHORT}.snapshot"
else
    SNAP_NAME="enforcing-hooks-$(date -u +%Y%m%dT%H%M%SZ).snapshot"
fi
SNAP_PATH="$STATE_DIR/$SNAP_NAME"
TMP_PATH="$SNAP_PATH.tmp.$$"

# --- WHY THIS IS WRITE-ONCE PER SESSION ----------------------------------
# THIS FILE IS THE ONLY RECORD OF WHAT THE SESSION ACTUALLY BOOTED WITH, and
# until 2026-09-14 it was overwritten by every SessionStart — which is not one
# event. The host fires SessionStart again on `/clear` and on every compaction,
# IN THE SAME OS PROCESS, and the host reads the plugin hook table exactly once,
# at process start. So a compaction rewrote the "session start" baseline from
# the CURRENT contents of hooks.json while the running session's loaded table
# had not moved at all.
#
# The consequence is the worst kind: the overwrite FORGIVES. Every guard that
# landed mid-session and is inert becomes part of the new baseline, so
# notice-hook-staleness.sh compares equal and reports no drift, for the rest of
# a session in which those guards still enforce nothing.
#
# MEASURED, on this machine, on the day this was written:
#   claude pid 75476 started        Sun 13 Sep 23:59:44 2026  (ps -eo lstart)
#   guard-hook-registration-commits.sh registered  2026-09-14 08:10 (git log)
#   enforcing-hooks-b7d89f44.snapshot header says  generated=2026-09-14T19:10:17Z
# A "session start" baseline stamped twenty hours after the session started.
# That file was then quoted as proof the guard was live in the session, and it
# was not: the identical commit the guard exists to refuse went through it in
# silence at 20:25, and repairing the red `main` that followed cost a
# six-figure-token agent.
#
# THE DISCRIMINATOR IS `source`, AND IT IS THE ONLY HONEST ONE. `startup` and
# `resume` are new OS processes, so the host really did re-read the table and
# the baseline MUST be refreshed. `clear` and `compact` are the same process,
# so the boot table is unchanged and the baseline must NOT move. Anything else
# — an unknown source, a payload that did not parse, a direct CLI call — keeps
# the previous behavior and writes, because refusing to record a baseline is
# how the partner hook ends up unable to compare at all, and "I could not
# check" must never be produced by a fix for "I checked the wrong thing".
case "$SESSION_SOURCE" in
    clear|compact)
        if [ -f "$SNAP_PATH" ]; then
            emit_context "enforcing-hook snapshot: baseline PRESERVED (SessionStart source=$SESSION_SOURCE). This is the same OS process, so the hook table this session booted with has NOT changed; rewriting the baseline now would silently forgive every guard that landed mid-session and leave notice-hook-staleness.sh reporting no drift while those guards enforce nothing."
            exit 0
        fi
        ;;
esac

{
    printf '# enforcing-hook snapshot — written by scripts/hooks/snapshot-enforcing-hooks.sh\n'
    printf '# session=%s generated=%s engine=%s surface=%s rows=%s\n' \
        "${SESSION_ID:-unknown}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        "$ENGINE_ROOT" "$SURFACE" "$ROW_COUNT"
    printf '%s\n' "$ROWS"
} >"$TMP_PATH" 2>/dev/null || true

if [ -f "$TMP_PATH" ]; then
    mv -f "$TMP_PATH" "$SNAP_PATH" 2>/dev/null || rm -f "$TMP_PATH" 2>/dev/null || true
fi

if [ ! -f "$SNAP_PATH" ]; then
    emit_context "enforcing-hook snapshot: FAILED to write $SNAP_PATH. notice-hook-staleness.sh will say it cannot compare this session rather than reporting no drift."
    exit 0
fi

# --- refresh the `latest` handle -----------------------------------------
LATEST="$STATE_DIR/enforcing-hooks-latest.snapshot"
rm -f "$LATEST" 2>/dev/null || true
ln -s "$SNAP_NAME" "$LATEST" 2>/dev/null || cp -f "$SNAP_PATH" "$LATEST" 2>/dev/null || true

# --- retention -----------------------------------------------------------
if command -v ls >/dev/null 2>&1; then
    OLD="$(ls -t "$STATE_DIR"/enforcing-hooks-*.snapshot 2>/dev/null \
        | grep -v '/enforcing-hooks-latest\.snapshot$' \
        | tail -n +$((SNAPSHOT_RETAIN + 1)) || true)"
    if [ -n "$OLD" ]; then
        while IFS= read -r stale; do
            [ -n "$stale" ] || continue
            [ "$stale" = "$SNAP_PATH" ] && continue
            rm -f "$stale" 2>/dev/null || true
        done <<<"$OLD"
    fi
fi

emit_context "enforcing-hook snapshot: recorded the ${ROW_COUNT} hook registrations this session actually booted with -> .claude/state/$SNAP_NAME. If a guard is landed into the engine's hooks/hooks.json later in this session, notice-hook-staleness.sh will name it at the end of a turn and tell you it is inert until you restart."
exit 0
