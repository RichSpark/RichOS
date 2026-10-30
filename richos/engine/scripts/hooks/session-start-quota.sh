#!/usr/bin/env bash
#
# session-start-quota.sh — SessionStart notice: the CEO's 93% quota rule
#                          applies to this session, and here is the command.
#
# HIS WORDS (ruling §87, richos-hq/wiki/ceo-decisions.md):
#   "quota polling: every 5 minutes from now. And once it crosses the 93%
#    threshold: PAUSE subagents. Then resume after quota rest."
#
# HIS UPDATE, 2026-09-25 (ruling §87), quoted in the notice as the rule now
# stands: at or above the threshold, no pause when the reset is less than 20
# minutes away (exactly 20 still pauses), and a hold in place releases inside
# that window. Polling stays every 5 minutes at every usage level.
#
# On 2026-09-25 he asked when the rule would be "burned in everywhere where it
# should be" and when there would be "a re-usable script for that". The script
# is scripts/quota-watch.sh. This notice is the burning-in: every lead, in
# every session of a repository that adopted the engine, is told the rule in
# his words, the threshold as declared, the reading now, and the exact command
# that starts the watcher.
#
# WHO HEARS IT: THE LEAD, NOT THE CEO. The text goes out on additionalContext
# only, which reaches the model. It deliberately does NOT use systemMessage,
# which reaches the person: the design notes this rule rests on
# (richos-hq/docs/plans/budget-self-management-2026-09-10.md, R4) put "no
# meter, no percentage, no countdown in his ordinary view". The lead gets the
# number; he does not.
#
# SILENT where the rule cannot apply: a repository that never adopted the
# engine has no orchestration.config, and engine-status.sh already announces
# that stand-down. An adopted repository with no QUOTA_PAUSE_PERCENT is NOT
# silent: the notice says the watcher cannot run and which line to declare.
#
# NEVER BLOCKS AND NEVER FAILS THE SESSION: every path exits 0. It reads one
# small JSON file and one config line; it does not load the workspace registry.
#
# It resolves no entity root itself: quota-watch.sh does, from the session's
# CLAUDE_PROJECT_DIR and working directory, like every rooted engine script.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WATCH="$HOOK_DIR/../quota-watch.sh"

# The payload is not needed, so stdin is NEVER read: a SessionStart hook that
# reads a stdin the host may never close hangs the session start
# (session-start-stdin.test.sh, case 9q).

[ -f "$WATCH" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

BODY="$(bash "$WATCH" --notice 2>/dev/null || true)"
[ -n "$BODY" ] || exit 0

BODY="$BODY" python3 -c '
import json, os
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": os.environ.get("BODY", ""),
    },
}))
' 2>/dev/null || true
exit 0
