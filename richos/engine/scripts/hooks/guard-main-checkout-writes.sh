#!/usr/bin/env bash
#
# guard-main-checkout-writes.sh — PreToolUse guard (Write|Edit|MultiEdit|NotebookEdit).
#
# Blocks edits to SOURCE in the MAIN checkout. All implementation work must
# happen in an isolated git worktree (native Agent `isolation: "worktree"`, or
# a manual worktree under .claude/worktrees/); the orchestrator only ever
# MERGES source into main via git and edits docs/config there. So a Write/Edit
# targeting one of the protected source trees in the main checkout is almost
# always an agent that drifted into the shared checkout by mistake — the #1
# isolation failure class this hook exists to catch AT THE MOMENT of the write.
#
# The protected trees are project-specific and read from orchestration.config
# (PROTECTED_PATHS). ALLOWED (exit 0): anything under a worktree
# (…/.claude/worktrees/…), plus docs/config in main (CLAUDE.md, .claude/**,
# scripts/**, docs/**, team/**, skills/** …). BLOCKED (exit 2): writes to
# "<main>/<protected-prefix>/**".
#
# REPO_ROOT is resolved from THIS script's own location, and settings.json
# wires the hook by absolute path to the MAIN checkout's copy — so REPO_ROOT is
# always the main checkout, regardless of which session triggered the write.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

# Fail-closed, not fail-open: TOOL_NAME and FILE_PATH below are both extracted
# via `python3 ... || true`. If python3 is missing, that swallowed failure
# yields an empty TOOL_NAME/FILE_PATH, which every downstream check below reads
# as "not a guardable write" — i.e. this guard would let EVERY write through,
# including one targeting a protected source tree. Refuse outright instead.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-main-checkout-writes.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-main-checkout-writes.sh
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

# --- JURISDICTION ----------------------------------------------------------
# Deliberately BELOW the root-resolution bootstrap, never inside it: Layer R of
# contract-integrity-probe.sh extracts that block verbatim and asserts it is
# byte-identical across every rooted hook, so anything added inside it would
# read as divergence.
#
# The seat resolved above answers "am I governed?". It does NOT answer "does
# the artifact I was just handed belong to the repository I govern?" — and
# until 2026-08-30 nothing asked. See scripts/lib/seat-jurisdiction.sh.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-main-checkout-writes.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs, and a guard that"
        echo "  cannot tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

INPUT="$(cat)"

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # LOUD, once per repository per session. engine-status.sh announces the
    # stand-down at SessionStart, which fires before any work happens and names
    # no action; this fires at the MOMENT this guard declines, which is the only
    # moment the absence costs anything.
    richos_announce_stand_down "scripts/hooks/guard-main-checkout-writes.sh"
    # This repository never adopted the engine, so there is no enforcement to
    # lose here. Stand down. NOT a silent skip: engine-status.sh announces the
    # stand-down into the orchestrator's own context at every session start.
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/guard-main-checkout-writes.sh" >&2
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
    unevaluated_or_continue "guard-main-checkout-writes.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this write lands in the main checkout of a governed repository instead of a worktree"
fi

# Load the GOVERNED repository's config — its protected source trees, not the
# engine's. Under the old resolution this loaded the engine's own config and
# then guarded the engine's own directories, which is why a plugin-loaded
# engine protected nothing at all in the repository it was watching.
CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${PROTECTED_PATHS:=}"

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
  Write|Edit|MultiEdit|NotebookEdit) ;;
  *) exit 0 ;;
esac

FILE_PATH="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"
[ -n "$FILE_PATH" ] || exit 0

# Only absolute paths are guardable (the file tools require absolute paths anyway).
case "$FILE_PATH" in
  /*) ;;
  *) exit 0 ;;
esac

# A worktree edit is always fine (manual worktrees live under .claude/worktrees/;
# native isolation worktrees are a separate checkout path entirely). Allow explicitly.
# P3-32: judged on the PHYSICAL path. A directory under .claude/worktrees that
# is a symlink back into protected main is not a worktree.
REAL_FILE_PATH="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$FILE_PATH" 2>/dev/null || printf '%s' "$FILE_PATH")"
case "$FILE_PATH" in
  */.claude/worktrees/*)
    case "$REAL_FILE_PATH" in
      */.claude/worktrees/*) exit 0 ;;
    esac
    # Not a worktree after all: judge the physical target from here on.
    FILE_PATH="$REAL_FILE_PATH"
    ;;
esac

# --- GOVERNANCE: resolved from the FILE, not from the seat -----------------
# "Am I governed?" and "what am I inspecting?" are now the SAME question about
# the SAME repository. The file's own repository governs it if it has adopted;
# failing that the seat governs it, but only if the file is inside the seat.
#
# THIS IS THE LINE THAT WAS WRONG. PROTECTED_PATHS was joined onto whatever the
# session happened to be seated in, so in richos the guard defended
# richos/engine/app — WHICH DOES NOT EXIST — and allowed every write to the real
# richos/app. Reloading the config from the governing root makes the protected
# trees belong to the same repository as the path they are matched against.
if ! GOVERNING_ROOT="$(richos_governing_root "$FILE_PATH" "${ENTITY_ROOT}")"; then
    # TWO DIFFERENT PROBLEMS, TWO DIFFERENT MESSAGES, because they need
    # different actions from the reader and a banner that names the wrong
    # repository is barely better than no banner:
    #
    #   out of jurisdiction  the file lives in ANOTHER repository. The notice
    #                        must name BOTH, or the reader cannot tell which
    #                        tree is the unguarded one.
    #   not adopted          the file is inside the seat, and the seat has
    #                        adopted nothing. Naming the one repository is
    #                        right, and the fix is to adopt it.
    #
    # richos_assert_jurisdiction returns 0 when the file IS inside the seat, so
    # this reads as "if it was mine and I still cannot govern it, that is a
    # non-adoption problem".
    if richos_assert_jurisdiction "scripts/hooks/guard-main-checkout-writes.sh" \
           "${ENTITY_ROOT}" "$FILE_PATH" "file" "declines"; then
        richos_announce_stand_down "scripts/hooks/guard-main-checkout-writes.sh" \
            "this repository has adopted nothing, so no PROTECTED_PATHS govern this write"
    fi
    exit 0
fi

# BOTH SIDES PHYSICALIZED BEFORE THE PREFIX TEST BELOW. On macOS /var is a
# symlink to /private/var, so the seat arrives logical (from the payload) while
# the governing root arrives physical (it has been through `cd && pwd`). Compare
# those two strings and a repository is not itself: the guard silently reloads
# a config it already had, and then matches PROTECTED_PATHS against a path with
# the other spelling and never fires. Caught by case 2a of
# scripts/lib/seat-jurisdiction.test.sh — the POSITIVE control, which is exactly
# the case that exists to notice a guard that has gone inert.
SEAT_PHYS="$(richos_physical "${ENTITY_ROOT:-}")"
FILE_PATH="$(richos_physical "$FILE_PATH")"

if [ "$GOVERNING_ROOT" != "$SEAT_PHYS" ]; then
    # A different repository governs this file than the one this session sits
    # in. Say so once, then judge it by ITS rules rather than the seat's.
    richos_assert_jurisdiction "scripts/hooks/guard-main-checkout-writes.sh" \
        "$ENTITY_ROOT" "$FILE_PATH" "file" "proceeds" || true
    PROTECTED_PATHS=""
    CONFIG="$GOVERNING_ROOT/orchestration.config"
    # shellcheck disable=SC1090
    [ -f "$CONFIG" ] && . "$CONFIG"
    : "${PROTECTED_PATHS:=}"
fi
ENTITY_ROOT="$GOVERNING_ROOT"

# Sensible failure: with no protected paths configured, the guard cannot know
# what to protect. Surface it loudly (never silently) and allow the write.
if [ -z "${PROTECTED_PATHS// /}" ]; then
  echo "(hook: guard-main-checkout-writes.sh) NOTE: PROTECTED_PATHS is unset in orchestration.config — write-guard is INACTIVE. Fill PROTECTED_PATHS to enable main-checkout write protection." >&2
  exit 0
fi

# Block source writes to any protected tree in the MAIN checkout.
for p in $PROTECTED_PATHS; do
  case "$FILE_PATH" in
    "$ENTITY_ROOT/$p"/*)
      echo "=== Main-checkout write BLOCKED ===" >&2
      echo "  Refusing to write '$FILE_PATH'." >&2
      echo "  This is the SHARED main checkout ($ENTITY_ROOT). All source (protected trees:" >&2
      echo "  $PROTECTED_PATHS) must be edited in your own git worktree, never in the main" >&2
      echo "  checkout — concurrent edits here corrupt other agents' work and block landing." >&2
      echo "  Re-issue the edit against your worktree path (…/.claude/worktrees/<your-worktree>/…)." >&2
      echo "  (If you are the orchestrator: land source via git merge, or make source changes in a worktree.)" >&2
      echo "(hook: scripts/hooks/guard-main-checkout-writes.sh)" >&2
      exit 2
      ;;
  esac
done

exit 0
