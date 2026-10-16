#!/usr/bin/env bash
#
# stall-watch.sh: wakes the lead the moment teammate work stalls.
#
# The CEO, 2026-09-28, after three teammates sat 70 to 85 minutes without a
# commit, queued behind the Mac's single proof-run slot while it ran at 26 to
# 38% CPU, and nothing woke the lead: "WHERE THE FUCK WAS THAT FUCKING WATCHER
# BEFORE???" His build point: "He never has to ask; every step and failure
# reaches him unprompted."
#
#   stall-watch.sh --monitor   THE WATCHER. Claude Code starts it with every
#                              interactive session: the engine's plugin monitor
#                              (monitors/monitors.json), as quota-watch is. It
#                              looks every 60 s and prints each stall once;
#                              every printed block reaches the lead as a
#                              notification, and it keeps going.
#   stall-watch.sh --alive     one line: is this session watched, and when did
#                              it last look (0 watched, 1 not, 2 no session)
#   stall-watch.sh --watch     the fallback where plugin monitors do not run:
#                              a BACKGROUND command that exits at the next
#                              notification
#   stall-watch.sh --once      every stall right now (0 none, 1 some, 2 could
#                              not look)
#   stall-watch.sh --notice    the SessionStart notice body
#
# A stall: a resource wait over 10 minutes (proof-run slot, CPU admission, the
# test VM, a device, any recorded wait); a live teammate with no commit and no
# transcript write for 20 minutes; a teammate waiting on a slot its own other
# process holds. It reports these stalls. Its existing idle-end exception
# ends only owned idle commands deferring an already ended turn. The logic
# and every threshold's reason: scripts/lib/stall_watch.py.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib/stall_watch.py"
RR="$SCRIPT_DIR/lib/resolve-roots.sh"

command -v python3 >/dev/null 2>&1 || { echo "stall-watch.sh: python3 is required" >&2; exit 2; }
[ -f "$LIB" ] || { echo "stall-watch.sh: $LIB is missing" >&2; exit 2; }
[ -f "$RR" ] || { echo "stall-watch.sh: $RR is missing" >&2; exit 2; }

# shellcheck source=lib/resolve-roots.sh
. "$RR"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR/lib")"

# The governed repository, the way every rooted engine script finds it. Only
# whether it adopted the engine (an orchestration.config) is read: a
# repository that never did gets no watcher and no notice.
CONFIG=""
if resolve_entity_root ""; then
    if [ -f "$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config" ]; then
        CONFIG="$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config"
    fi
fi

exec python3 "$LIB" "$@" --config "$CONFIG" --engine-root "$ENGINE_ROOT" \
    --command "$SCRIPT_DIR/stall-watch.sh"
