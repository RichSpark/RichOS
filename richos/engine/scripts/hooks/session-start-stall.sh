#!/usr/bin/env bash
#
# session-start-stall.sh: SessionStart notice: the stall watcher runs with this
#                         session, what it reports, and the one-line check.
#
# The CEO, 2026-09-28: "WHERE THE FUCK WAS THAT FUCKING WATCHER BEFORE???"
# The watcher itself is the engine's plugin monitor `stall-watch`
# (monitors/monitors.json), which Claude Code starts with every interactive
# session; this notice tells the lead it is there, what it reports, the one
# thing it ends (a teammate's own idle commands that keep its end waiting),
# and how to check it (stall-watch.sh --alive) and start the fallback if not.
#
# WHO HEARS IT: the lead only (additionalContext), never the CEO
# (systemMessage), as session-start-quota.sh.
#
# SILENT where the engine was never adopted (no orchestration.config).
# NEVER BLOCKS AND NEVER FAILS THE SESSION: every path exits 0, and stdin is
# never read (session-start-stdin.test.sh, case 9q: a SessionStart hook that
# reads a stdin the host may never close hangs the session start).

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WATCH="$HOOK_DIR/../stall-watch.sh"

[ -f "$WATCH" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

BODY="$(bash "$WATCH" --notice </dev/null 2>/dev/null || true)"
[ -n "$BODY" ] || exit 0

BODY="$BODY" python3 -c '
import json, os
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": os.environ.get("BODY", ""),
    },
}))
' 2>/dev/null </dev/null || true
exit 0
