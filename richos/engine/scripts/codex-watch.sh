#!/usr/bin/env bash
#
# codex-watch.sh: Codex's messages to the lead wake the lead.
#
# On 2026-10-05 Codex's "READY TO LAND" entry in to-rich.md sat unlanded for
# about five hours because the watcher on it had to be started by hand and was
# not. This one is started by Claude Code itself with every interactive
# session: the engine's plugin monitor (monitors/monitors.json), as
# quota-watch and stall-watch are.
#
#   codex-watch.sh --monitor   THE WATCHER. Looks at to-rich.md in each channel
#                              folder (the CODEX_CHANNELS setting in
#                              orchestration.config; unset, the conventional
#                              ~/.richos-coordination/rich-codex and
#                              rich-codex-questions) every 15 s and
#                              prints each new entry ONCE, as one block with
#                              every one of its lines; each block reaches the
#                              lead as a notification. Ends with its session.
#   codex-watch.sh --tick      one look, printing what the monitor would print
#
# It only reads and prints. The logic: scripts/lib/codex_watch.py.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib/codex_watch.py"
RR="$SCRIPT_DIR/lib/resolve-roots.sh"

command -v python3 >/dev/null 2>&1 || { echo "codex-watch.sh: python3 is required" >&2; exit 2; }
[ -f "$LIB" ] || { echo "codex-watch.sh: $LIB is missing" >&2; exit 2; }
[ -f "$RR" ] || { echo "codex-watch.sh: $RR is missing" >&2; exit 2; }

# shellcheck source=lib/resolve-roots.sh
. "$RR"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR/lib")"

# Only whether the governed repository adopted the engine (an
# orchestration.config) is read: one that never did gets no watcher.
CONFIG=""
if resolve_entity_root ""; then
    if [ -f "$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config" ]; then
        CONFIG="$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config"
    fi
fi

exec python3 "$LIB" "$@" --config "$CONFIG" --engine-root "$ENGINE_ROOT"
