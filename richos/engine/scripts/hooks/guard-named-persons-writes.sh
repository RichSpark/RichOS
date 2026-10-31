#!/usr/bin/env bash
#
# guard-named-persons-writes.sh — PreToolUse guard (Write|Edit|MultiEdit|NotebookEdit).
#
# Same hook shape as scan-secrets.sh, same matcher, a THIRD predicate. That one
# asks "is this a credential?"; guard-publication-writes.sh asks "is this
# something a person said in private?"; this one asks "IS THIS SOMEBODY'S NAME?"
#
# A NAME IS NOT A SECRET AND NEVER TRIPS A SECRET SCANNER. It carries no vendor
# prefix and its entropy is the entropy of ordinary prose. That is why a third
# party's name sat in this repository through every write, every commit and a
# public launch, and was found by the person himself.
#
# TWO SURFACES, NOT ONE. This checks the new CONTENT and the DESTINATION PATH.
# The path is not thoroughness for its own sake: the actual leak WAS a filename
# — a source recording named inside a test fixture's script — and a filename is
# personal data even when the media it names never ships.
#
# The full rationale, the match rule, the false-positive trade and the honest
# statement of which surfaces a hook cannot see live in
# scripts/lib/named-persons.py — one place, because three callers run the same
# predicate and a predicate in three copies is the defect class this exists to
# end.
#
# WHY A WRITE HOOK IS NOT ENOUGH, AND WHAT COVERS THE REST: this sees content an
# agent authors in THIS session. guard-named-persons-commands.sh covers the
# commit message, the branch name and the PR/issue title — the three surfaces
# the original scrub missed, one of which it made WORSE by putting the full name
# and the company into the commit message that removed it from the file.
# named-persons.sh --tree covers everything that reached the tree by other
# means, and the release check runs it.
#
# ABSENT IS ANNOUNCED, NEVER PASSED SILENTLY. With no deny-list on this machine
# this guard says so — loudly, by name, once per repository per session — and
# lets the write through. It does NOT block: this repository is public, a
# stranger who clones it has no roster of the owner's clients and friends, and
# a guard that bricks a fresh clone is a guard that gets deleted. The refusal
# lives at the release chokepoint instead, which runs on the owner's machine.
#
# FAIL-CLOSED on a missing python3 and on a BROKEN list. FAILS OPEN (exit 0) on
# a malformed/unparseable hook payload, matching its three siblings on this same
# matcher (guard-main-checkout-writes.sh, scan-secrets.sh,
# guard-publication-writes.sh): a payload that cannot be understood is not
# itself a disclosure.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-named-persons-writes.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-named-persons-writes.sh
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
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-writes.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs, and a guard that"
        echo "  cannot tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

# The publication-boundary library, for TWO helpers and no predicate:
# pb_repo_root / pb_physical (so "which repository is this file in?" has ONE
# answer shared with the guards that already ask it) and pb_path_is_ignored
# (a gitignored destination is not publication-bound, so a private note about a
# real person belongs there and must not be refused). Sourcing it also brings
# in decl_find, which is how np_publication_bound reads the SAME declaration
# the publication guards read rather than inventing a second adoption signal.
_PB_LIB="$SCRIPT_DIR/../lib/publication-boundary.sh"
if [ ! -f "$_PB_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-writes.sh"
        echo "  scripts/lib/publication-boundary.sh is missing at: $_PB_LIB"
        echo "  It answers which repository a path belongs to and whether that"
        echo "  repository publishes. Without it this guard cannot tell whether"
        echo "  it is looking at a public tree, and it will not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/publication-boundary.sh
. "$_PB_LIB"

_NP_LIB="$SCRIPT_DIR/../lib/named-persons.sh"
if [ ! -f "$_NP_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-named-persons-writes.sh"
        echo "  scripts/lib/named-persons.sh is missing at: $_NP_LIB"
        echo "  It is the whole decision this guard makes. Without it the guard"
        echo "  checks nothing, and a guard that checks nothing while reporting"
        echo "  'on' is worse than no guard at all."
    } >&2
    exit 2
fi
# shellcheck source=../lib/named-persons.sh
. "$_NP_LIB"

INPUT="$(cat)"
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
    unevaluated_or_continue "guard-named-persons-writes.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this write puts a real person's name into publication-bound material"
fi

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
  Write|Edit|MultiEdit|NotebookEdit) ;;
  *) exit 0 ;;
esac

FILE_PATH="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"

[ -n "$FILE_PATH" ] || exit 0
case "$FILE_PATH" in
  /*) ;;
  *) exit 0 ;;   # a relative path has no unambiguous repository
esac

# --- WHICH REPOSITORY IS THIS ARTIFACT GOING INTO? -------------------------
# The FILE's repository, not the session's. A teammate seated in one checkout
# writes into another constantly, and "does this tree get published?" is a
# question about the DESTINATION. Same resolution as guard-publication-writes.sh
# uses, out of the same library, so the two can never disagree about which
# repository a write lands in.
FILE_PATH="$(pb_physical "$FILE_PATH")"
NP_REPO="$(pb_repo_root "$FILE_PATH" 2>/dev/null || true)"
[ -n "$NP_REPO" ] || exit 0

NP_BOUND_RC=0
np_publication_bound "$NP_REPO" || NP_BOUND_RC=$?
case "$NP_BOUND_RC" in
  0) ;;
  2)
    # The declaration is broken. That is guard-publication-writes.sh's contract
    # and it runs on this same matcher and refuses on it — so refusing again
    # here would double the noise over one fault, and standing down would be a
    # silent pass on a question this guard cannot answer. It says nothing and
    # leaves the answer to the guard whose file that is.
    exit 0 ;;
  *) exit 0 ;;   # this repository does not publish — the precision floor
esac

# A GITIGNORED DESTINATION IS NOT PUBLICATION-BOUND, so it is the CORRECT home
# for a note that names a real person, exactly as it is the correct home for the
# private audio. Allowing it is not a grudging exception; refusing it would push
# private material somewhere worse.
if pb_path_is_ignored "$NP_REPO" "$FILE_PATH"; then
    exit 0
fi

RESULT="$(printf '%s' "$INPUT" | python3 "$NP_PREDICATE" --scan-payload 2>/dev/null || printf 'PARSEFAIL\n')"
VERDICT="$(printf '%s' "$RESULT" | head -1 | cut -f1)"

case "$VERDICT" in
  CLEAN|PARSEFAIL)
    exit 0
    ;;
  ABSENT)
    np_announce_absent "scripts/hooks/guard-named-persons-writes.sh" "$NP_REPO"
    exit 0
    ;;
  BROKEN)
    np_broken_banner "scripts/hooks/guard-named-persons-writes.sh" \
        "$(printf '%s' "$RESULT" | head -1 | cut -f2-)"
    exit 2
    ;;
  FOUND)
    np_block_banner "scripts/hooks/guard-named-persons-writes.sh" \
        "the write to '${FILE_PATH:-<unknown file>}'" "$RESULT"
    exit 2
    ;;
  *)
    echo "ERROR: guard-named-persons-writes.sh: unexpected predicate output — refusing (fail-closed)" >&2
    exit 2
    ;;
esac
