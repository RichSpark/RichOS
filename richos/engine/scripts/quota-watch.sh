#!/usr/bin/env bash
#
# quota-watch.sh: five-hour 93% and overall weekly 99%, every five minutes.
# Prints every reported window. Weekly reset approvals are shared with the desktop
# via quota-reset.sh; only its user-authenticated approve command can create one.
# Without a usable approved reset, weekly 99% emits the standard pause message.
# No five-hour release may release an active weekly hold.
#
# HIS WORDS, the whole behavior (ruling §87, richos-hq/wiki/ceo-decisions.md):
#   "quota polling: every 5 minutes from now. And once it crosses the 93%
#    threshold: PAUSE subagents. Then resume after quota rest."
#
# HIS UPDATE, 2026-09-25 (ruling §87): at or above the threshold, do not pause
# when the reset is LESS than 20 minutes away (exactly 20 still pauses), and a
# hold already in place releases inside that window, keeping the same agent.
# Polling stays every 5 minutes at every usage level: "Well, I've changed my
# mind. Let's drop the 30-minute check nonsense. Keep it consistent at 5
# minutes."
#
#   quota-watch.sh --once
#       one line: the reading, its age and the reset time.
#       exit 0 below the threshold, 1 at or above it (pause), 2 unknown,
#       3 at or above it with the reset less than 20 minutes away (no pause)
#   quota-watch.sh --status
#       the same reading, human-readable, with this session's live workers
#   quota-watch.sh --monitor
#       THE WATCHER, started by Claude Code itself with every interactive
#       session: the engine's plugin monitor (monitors/monitors.json) runs it
#       for the whole session. A poller polls every 300 s and NEVER stops on an
#       event; each event below is printed once and reaches the lead as a
#       notification. (2026-09-28: the watcher used to EXIT to wake the lead,
#       and nothing polled until it was started again by hand.)
#   quota-watch.sh --alive
#       what polls for this session and what delivers its wake-ups
#   quota-watch.sh --watch [--until-reset]
#       the fallback, where plugin monitors do not run: run it as a BACKGROUND
#       command (Bash with run_in_background: true) so its exit wakes the
#       lead. The same poller polls every 300 s and keeps polling after it
#       exits; a wake-up that comes before the next --watch waits for it. It
#       exits after printing the next event:
#         QUOTA-THRESHOLD  at or above the threshold with a worker running and
#                          20 minutes or more to the reset: the exact pause
#                          message, the names to send it to, the minutes to
#                          reset and the live-worker count
#         QUOTA-RELEASE    the reset is less than 20 minutes away and agents
#                          are paused for the quota: the exact resume message
#                          and their names (the hold releases there)
#         WINDOW-RESET     the window turned over: the exact resume message and
#                          the names of the agents paused for the quota
#         QUOTA-STALE      quota is still unknown after automatic retries,
#                          and the most recent usable reading is stale
#         QUOTA-UNKNOWN    no usable reading after one full poll interval
#       get_usage failures are retried automatically every 60 s. Valid quota
#       events still fire from the fallback when it proves the crossing.
#       --until-reset never fires the threshold: for a window in which the
#       lead has already decided (paused, or chose to keep working). It wakes
#       at the hold's release when agents are paused for the quota, and at
#       the reset.
#
# HIGH SPEED, ruling §108 (2026-10-04): "when the token consumption velocity
# increases a lot, we need to start checking the remaining quota every 2
# minutes (instead of 5) and then also re-calculate the estimate for when the
# workers must be paused". From 5.8 points per 5 minutes (the 7 points above
# 93% used inside one check plus a minute for the hold) the watcher wakes the
# lead once with QUOTA-FAST (the speed and when the window runs out, for the
# CEO), polls every 60 s, and sends QUOTA-THRESHOLD early, when the measured
# speed (a measured jump at once) would carry the window to 100% before the
# next check and the hold. 10 or more teammates started within 5 minutes also
# poll every 60 s, before any rise is measured. Back at normal speed, all of
# it returns to every 5 minutes and 93%. Derivation and readings: quota_watch.py.
#
# THE THRESHOLD is QUOTA_PAUSE_PERCENT in the governed repository's
# orchestration.config, beside MODEL_CEILING, read here and nowhere else.
# Undeclared is UNKNOWN (exit 2), never a built-in 93.
#
# THE READING, at every poll: Claude Code's own get_usage control request,
# from a bounded control-only `claude` process (no prompt, no tools, no
# settings, no MCP, no session persistence) that it starts in its own process
# group and ends on every read. The desktop app's design, copied (Codex,
# codex/claude-quota-settings, quota/probe.rs). When that fails it says why and
# falls back to ~/.claude/statusline-payload.json (QUOTA_PAYLOAD overrides, for
# tests). --notice reads only the file: a SessionStart hook does not wait
# seconds on a process. It also reads the workspace registry. It never
# messages, pauses, stops or spawns an agent, and it never reads a credential
# itself. The lead acts.
#
# The logic, and every choice in it: scripts/lib/quota_watch.py.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib/quota_watch.py"
RR="$SCRIPT_DIR/lib/resolve-roots.sh"

command -v python3 >/dev/null 2>&1 || { echo "quota-watch.sh: python3 is required" >&2; exit 2; }
[ -f "$LIB" ] || { echo "quota-watch.sh: $LIB is missing" >&2; exit 2; }
[ -f "$RR" ] || { echo "quota-watch.sh: $RR is missing" >&2; exit 2; }

# shellcheck source=lib/resolve-roots.sh
. "$RR"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR/lib")"

# The governed repository, the same way every rooted engine script finds it:
# RICHOS_ENTITY_ROOT, then CLAUDE_PROJECT_DIR, then the working directory.
CONFIG=""
RAW=""
if resolve_entity_root ""; then
    CONFIG="$RICHOS_ENTITY_ROOT_RESOLVED/orchestration.config"
    if [ -f "$CONFIG" ]; then
        # Sourced in a subshell, exactly as the guards read MODEL_CEILING: the
        # declaration is shell, and nothing it sets leaks into this process.
        RAW="$( ( . "$CONFIG" >/dev/null 2>&1; printf '%s' "${QUOTA_PAUSE_PERCENT:-}" ) 2>/dev/null || true )"
    fi
else
    CONFIG=""
fi

# The command a lead is told to run, spelled the way it can be copied.
SELF="$SCRIPT_DIR/quota-watch.sh"

exec python3 "$LIB" "$@" --threshold-raw "$RAW" --config "$CONFIG" \
    --engine-root "$ENGINE_ROOT" --command "$SELF"
