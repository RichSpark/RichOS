#!/usr/bin/env bash
#
# review-watch.sh: the second review starts by itself, with nobody asking.
#
# On 2026-10-08 the CEO fetched a Codex review of an agent's work by hand four
# times, and each one found defects the author's own tests had passed. His
# words (ruling §113): "A regular RichOS user can never be expected anything
# even remotely close to that. So, this all must be completely automated."
# second-review.sh does one review (slice 1); this watcher starts it (slice 2
# of richos-hq docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md):
# at every teammate handover, every hour of a long job, when a teammate goes
# quiet, and at each Codex READY entry. It is started by Claude Code itself
# with every interactive session: the engine's plugin monitor
# (monitors/monitors.json), as stall-watch and codex-watch are.
#
#   review-watch.sh --monitor   THE WATCHER. Looks every 60 s, starts what is
#                               due, and prints each verdict once; every block
#                               it prints reaches the lead as a notification.
#                               Ends with its session (the reviews it started
#                               run to their own end).
#   review-watch.sh --tick      one look, printing what the monitor would print
#   review-watch.sh --status    is this session watched, and which reviews run
#
# It reviews only repositories listed in SECOND_REVIEW_REPOS in the governed
# repository's orchestration.config, and never work that changes only release
# notes. The logic: scripts/lib/review_watch.py.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib/review_watch.py"
RR="$SCRIPT_DIR/lib/resolve-roots.sh"

command -v python3 >/dev/null 2>&1 || { echo "review-watch.sh: python3 is required" >&2; exit 2; }
command -v git >/dev/null 2>&1 || { echo "review-watch.sh: git is required" >&2; exit 2; }
[ -f "$LIB" ] || { echo "review-watch.sh: $LIB is missing" >&2; exit 2; }
[ -f "$RR" ] || { echo "review-watch.sh: $RR is missing" >&2; exit 2; }

# shellcheck source=lib/resolve-roots.sh
. "$RR"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR/lib")"

# The governed repository's orchestration.config: whether it adopted the engine
# at all, and which of its repositories are reviewed.
CONFIG=""
if resolve_entity_root ""; then
    if [ -f "$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config" ]; then
        CONFIG="$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config"
    fi
fi

exec python3 "$LIB" "$@" --config "$CONFIG" --engine-root "$ENGINE_ROOT"
