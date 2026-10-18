#!/usr/bin/env bash
#
# second-review.sh: a different model reviews a teammate's exact commits
# against the words it was given, with nobody fetching anything by hand.
#
# On 2026-10-08 the CEO fetched a Codex review of an agent's work by hand four
# times, and each one found defects the author's own tests had passed. His
# words (ruling §113): "A regular RichOS user can never be expected anything
# even remotely close to that. So, this all must be completely automated."
# This is the command that does one such review (slice 1 of richos-hq
# docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md, with Sage's
# check beside it); the watcher that starts it by itself is slice 2.
#
#   second-review.sh --name <teammate> [--repo <path>] [--trigger handover|long-job|quiet|manual]
#   second-review.sh --repo <path> (--tip <sha> | --branch <b>) --base <sha> --words-file <file> [...]
#   second-review.sh --follow-up <review-id>#<n> --why '<who rules, and why>'   (the lead only)
#   second-review.sh --reopen <review-id>#<n>                                    (undoes --follow-up)
#
# Exit: 0 passed, 1 changes-requested, 2 no verdict (the row says why),
# 64 refused before any reviewer ran (bad arguments, no original words).
# The logic: scripts/lib/second_review.py.
#
# THE SCRATCH IS ALLOCATED HERE, NOT IN PYTHON, ON PURPOSE: scratch.sh records
# the allocating shell's pid as the owner, and this shell lives exactly as long
# as the review does. So a review killed hard leaves a folder whose owner is
# dead, which the scratch reaper deletes; the release below is the fast path.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib/second_review.py"

command -v python3 >/dev/null 2>&1 || { echo "second-review.sh: python3 is required" >&2; exit 64; }
command -v git >/dev/null 2>&1 || { echo "second-review.sh: git is required" >&2; exit 64; }
[ -f "$LIB" ] || { echo "second-review.sh: $LIB is missing" >&2; exit 64; }

# shellcheck source=lib/scratch.sh
. "$SCRIPT_DIR/lib/scratch.sh"
# 150 minutes: the 60-minute review limit, a 30-minute admission wait and a
# Claude fallback after a failed Codex start, with room to spare.
SCRATCH="$(scratch_new second-review --ttl 150)" || { echo "second-review.sh: no scratch folder" >&2; exit 2; }
# Stopped from outside (review-watch signals this script's process group, which
# holds the Python side and its reviewer alike): this releases the scratch once
# the Python side has exited.
trap 'scratch_release "$SCRATCH" >/dev/null 2>&1; exit 143' TERM HUP

python3 "$LIB" --scratch "$SCRATCH" --engine-root "$(cd "$SCRIPT_DIR/.." && pwd)" "$@"
RC=$?
scratch_release "$SCRATCH" >/dev/null 2>&1 || true
exit "$RC"
