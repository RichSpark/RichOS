#!/usr/bin/env bash
#
# observe-created-refs.sh — THE OTHER HALF OF THE PAIR. Matcherless PostToolUse.
#
# ===========================================================================
# WHAT IT RECORDS — docs/plans/worktree-spec-2026-09-11.md, points 3 and 10
# ===========================================================================
#   "A cc/ or native workspace with no registration, and ANY BRANCH AN AGENT
#    CREATED, counts as finished work of an ended session"          (point 3)
#   "every workspace and branch it has is deleted, as one. None is left
#    behind."                                                       (point 10)
#
# A ref an agent created is the agent's; a ref that already existed never is.
# CREATION is therefore the fact to record, and a tool call is the only moment
# the platform vouches for with an agent's id. It has two halves:
#
#   PreToolUse (matcherless)   guard-sealed-worktree.sh -> barrier() ->
#                              snapshot_refs(): what the agent's repositories
#                              held when this call STARTED.
#   PostToolUse (matcherless)  THIS HOOK -> observe() -> observe_created_refs():
#                              what is there now. A ref that is new AND carries
#                              this agent's own unlanded work was created by it.
#
# BOTH HALVES CARRY `tool_use_id`, AND IT IS THE SAME STRING FOR ONE CALL. That
# is what keys the window, and it is why this hook passes the WHOLE payload
# through rather than the agent id alone: an agent's own calls overlap, so one
# slot per agent lost a window every time two were open at once, and a ref
# created in the second was attributed to nobody. See mega-lander/workspaces.py,
# "ONE WINDOW PER TOOL CALL, KEYED BY THE CALL".
#
# THE PRE HALF ALREADY EXISTED AND THE POST HALF DID NOT, which is the whole
# reason attribution had to be read from POSSESSION (a ref checked out at the
# agent's own workspace path) until 2026-09-12 — and possession left the stray
# (`git branch spare` checks nothing out), the side branch (committed to and
# switched away from) and the borrowed branch (pre-existing, merely checked out,
# then deleted by a discard). mega-lander/workspaces.py carries the four filters
# that keep co-occurrence in time from being mistaken for authorship.
#
# IT RECORDS A FACT AND NEVER REFUSES ANYTHING. Exit 0 always, on every path:
# this hook fires after the work is already done, so a fact it could not record
# must never be allowed to look like a failed tool call. A missed observation
# costs one window of attribution, which under-attributes, which loses nothing.
#
# THE LEAD'S OWN CALLS COST ALMOST NOTHING. A payload with no `agent_id` is the
# lead's, and it is answered by one grep before python3 is started at all — this
# hook is registered against EVERY tool, so the lead's path has to be cheap.
#
# NOTE: hooks are snapshotted at session start. This hook is inert until the next
# session and assumes nothing about being live in the session that adds it.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

# A BOUNDED read, never `cat`: the payload arrives on stdin and the platform
# closes it within milliseconds; 3 s is ample for a 225 KB payload. `-d ''`
# returns nonzero on a complete read, so the VALUE is judged, never `$?`.
INPUT=""
IFS= read -r -t "${RICHOS_HOOK_STDIN_TIMEOUT:-3}" -d '' INPUT || true

# The lead's own call: no agent created anything, so there is nothing to compare.
if ! printf '%s' "$INPUT" | grep -E >/dev/null '(^|[^\\])"agent_id"[[:space:]]*:[[:space:]]*"[^"]'; then
    exit 0
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
  hook: scripts/hooks/observe-created-refs.sh
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

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # No engine here, no workspace contract: stand down. A RESOLVED state.
    exit 0
else
    root_failure_banner "scripts/hooks/observe-created-refs.sh" >&2
    exit 0
fi

LIB="$SCRIPT_DIR/../../mega-lander/workspaces.py"
if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$LIB" ]; then
    echo "NOTICE: observe-created-refs.sh: python3 or $LIB is unavailable — a branch this agent created was NOT recorded against it (docs/plans/worktree-spec-2026-09-11.md, points 3, 10). Restore the engine." >&2
    exit 0
fi

# The payload travels on stdin, never in the environment: a large payload must
# not skip the observation. Anything it recorded is announced, because a branch
# joining an agent's record decides what a land deletes.
OUT="$(printf '%s' "$INPUT" | python3 "$LIB" --entity "$ENTITY_ROOT" observe-refs 2>/dev/null)" || true
if [ -n "$OUT" ]; then
    # A trailing newline, or `read` fails on the LAST line and its notice is
    # never printed: `$(...)` strips it, and until round 8 the notice for the
    # last row of every observation was silently lost.
    printf '%s\n' "$OUT" | while IFS=$'\t' read -r tag repo branch tip found why; do
        case "$tag" in
            CREATED)
                [ -n "$branch" ] && echo "NOTICE: $branch in $repo was created by this agent and is recorded against it: it is deleted with its workspaces when its work is landed or discarded (docs/plans/worktree-spec-2026-09-11.md, points 3, 10)." >&2 ;;
            RESTORED)
                # ITEMS 2 AND 3 OF ROUND 8, DELETION HALF: a RECORDED integration branch
                # or a codex/ ref that this agent's call DELETED — by any verb, named or
                # unnamed, from either checkout — has been re-created at the tip the
                # snapshot recorded. The write is create-only and carries a reflog
                # message, so it can neither clobber nor move anything.
                [ -n "$branch" ] && echo "=== PROTECTED REF RESTORED: $branch in $repo was $why during this agent's tool call and has been re-created at $tip. A RECORDED integration branch is moved only by Rich (point 14) and a codex/ ref is never deleted without the CEO's express word (point 2) — docs/plans/worktree-spec-2026-09-11.md. Recorded on the agent's record and in the store's event log (protected-ref-restored). ===" >&2 ;;
            LANDED)
                # ANOTHER CONVERSATION'S LAND, ANNOUNCED TO THE THREAD IT MOVED
                # UNDER (2026-09-17). The CEO runs two threads whose back ends
                # share a repository; their lands take one machine-wide lock per
                # repository, so they never collide — and the second one's land
                # still moves the recorded branch under the first one's running
                # agents. This is the one case where the AGENT is the right
                # audience for a protected-ref notice: it is not being accused
                # of anything, it is being told its base moved. Nothing was
                # lost (a fast-forward) and nothing was written, so the Stop
                # notice in notice-protected-ref-moves.sh deliberately never
                # speaks about it — its question is "were commits lost".
                [ -n "$branch" ] && echo "=== $branch MOVED UNDER YOU: in $repo it was $why. Nothing was lost and nothing was changed — this is a fast-forward by another conversation's land, taken in its turn under this repository's land lock. Your workspace was branched from $tip, so anything you measured, based or recorded against that tip is now behind: re-read before you rely on it, and expect your own land to fast-forward onto the newer tip. Inspect it with: git -C $repo log --oneline $tip..$found. Recorded on this agent's record and in the store's event log (protected-ref-landed-elsewhere). ===" >&2 ;;
            MOVED)
                # THE MOVE HALF, AND NOTHING WAS WRITTEN. Until 2026-09-14 this case put
                # the ref back, and that line moved refs/heads/main in richos three times
                # in one night — twice in twelve seconds, in opposite directions, while
                # Rich was landing (docs/verification/ref-write-forensics-2026-09-14.md).
                # A check that infers the writer from the SHAPE of the result cannot tell
                # an ordinary land from the abuse it hunts, so it reports and a human
                # decides. The reflog holds every tip either way.
                [ -n "$branch" ] && echo "=== PROTECTED REF MOVED: $branch in $repo was $why during this agent's tool call. NOTHING WAS CHANGED — the engine reports this and never moves a ref back (it did until 2026-09-14, and it undid Rich's own merges: docs/verification/ref-write-forensics-2026-09-14.md). $branch is at $found; this agent's call started with it at $tip. If that move was not intended, inspect it and decide: git -C $repo reflog show $branch --date=iso. A RECORDED integration branch is moved only by Rich (point 14) and a codex/ ref is never touched (point 2) — docs/plans/worktree-spec-2026-09-11.md. Recorded on the agent's record and in the store's event log (protected-ref-moved). ===" >&2 ;;
        esac
    done
fi
exit 0
