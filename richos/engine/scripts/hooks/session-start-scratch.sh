#!/usr/bin/env bash
#
# session-start-scratch.sh — SessionStart notice. Never blocks, and says
#                            nothing at all on a machine that is fine.
#
# ===========================================================================
# WHY A NOTICE AT ALL, WHEN A SCHEDULED JOB ALREADY DELETES
# ===========================================================================
# com.richos.scratch-reaper deletes dead scratch four times a day and writes
# every deletion to ~/.claude/state/scratch-reaper.log. Neither of those puts
# one word in front of a person, and the morning that started all of this
# (2026-09-17, 1.8 GB free of 460 GB) is the proof that a disk filling up is
# invisible until the operating system shouts about it.
#
# So this is the third leg, and it is narrow ON PURPOSE. It says something
# only when one of exactly three things is true:
#
#   1. MORE THAN THE DECLARED THRESHOLD IS RECLAIMABLE RIGHT NOW. Dead scratch
#      nothing running owns, measured live, in this run.
#   2. MORE THAN THE THRESHOLD CANNOT BE DECIDED. That pile is the one the
#      scheduled job will never clear on its own — a tripped wall is
#      permanent until a person looks at it — so if nobody is told it grows
#      forever, which is the exact failure the mechanism was ordered to end.
#   2b. MORE THAN ITS OWN THRESHOLD IS GARBAGE NOTHING WILL EVER COLLECT.
#      Added 2026-09-18: another program's temp directory, another user's, a
#      symlink, or entries a budgeted run did not reach. THIS IS THE GARBAGE
#      ALARM, and its absence was the whole of frank-opus-garbage1's verdict —
#      the mechanism had a disk-space alarm and a delete-failure alarm and
#      nothing that could say "there is garbage here and nobody is coming for
#      it". Its own threshold, because this pile never shrinks by itself.
#   3. (REMOVED 2026-10-23, cleanup PRD R5.) Whether the scheduled job is
#      installed, has run and is healthy is now the disk watchdog's cleanup
#      health, read from the job's own published result and delivered once,
#      with its owner action, by notice-disk-alert.sh at this same session
#      start. Saying it here too was a second summary of one failure.
#
# ===========================================================================
# WHAT IT COSTS, MEASURED, AND WHAT HAPPENS WHEN IT COSTS MORE
# ===========================================================================
# The scan is LIVE — it is the reaper's own --dry-run, so the number on the
# banner is the number the reaper would act on, not a cached recollection of
# one. MEASURED 2026-09-17 over 68 scratch roots and 5.2 GB: 1.3 s, of which
# nearly all is walking the dead trees to size them; the live session's own
# 4.27 GB scratchpad is never walked because a running session is not a
# candidate.
#
# A session start that gets slower every week is a notice somebody deletes, so
# the scan carries a BUDGET (--deadline). Past it, the scan stops and the
# notice says the size is UNKNOWN rather than holding the session open. 8
# seconds is six times the measured cost.
#
# CHANNELS: systemMessage for the operator, additionalContext for the model —
# the pair session-start-escalations.sh uses, for the reason it gives. Never
# stderr, which was measured on 2026-09-14 to reach nobody at all
# (docs/verification/check-census-2026-09-14.md).

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS="$(cd "$HOOK_DIR/.." && pwd)"
ENGINE_ROOT="$(cd "$SCRIPTS/.." && pwd)"
REAPER="$SCRIPTS/scratch-reaper.sh"
CONFIG="${SCRATCH_REAPER_CONFIG:-$ENGINE_ROOT/orchestration.config}"

# BUDGET, declared here rather than in the config: it is a property of being a
# SessionStart hook (how long a person will wait for a banner), not of this
# machine's scratch.
#
# IT WAS 8 — six times a measured 1.3 s — AND THE DENY-BY-DEFAULT ARM CHANGED
# THE COST IT WAS SIX TIMES OF. Measured 2026-09-18 on this machine, warm:
#
#   full pass, every arm, no budget            11.6 s   (60,942 + 764 children)
#   every arm EXCEPT the deny-by-default one     1.3 s
#   --notice as it now runs                      1.5 s
#
# A BIGGER BUDGET WAS THE WRONG ANSWER AND SO WAS A SMALLER ONE. With a budget of
# 5 s the arm got part of the way and the banner said "44,851 temp entries were
# NOT MEASURED within the 5 s budget" — at every session start, forever, because
# 60,942 temp entries is simply what this machine has. That line describes the
# budget rather than the machine's health, and a line that is always true is
# wallpaper. Wallpaper is how a real signal comes to be skipped.
#
# So --notice does not attempt the expensive arm at all: its garbage numbers come
# from the LAST FULL PASS, which the scheduled job publishes to
# scratch-reaper-state.json every six hours and which the banner quotes WITH ITS
# DATE. A pile nothing will ever collect does not change in six hours — that is
# what makes it that pile — and the disk watchdog's cleanup health alarms when
# no scheduled pass has completed within its declared twelve hours.
#
# The budget therefore stays as a SAFETY rather than as a design: 5 is more than
# three times the measured 1.5 s, and past it the banner says the size is unknown
# instead of holding the session open.
DEADLINE=5

[ -x "$REAPER" ] || exit 0
[ -f "$CONFIG" ] || exit 0
# shellcheck disable=SC1090
. "$CONFIG" >/dev/null 2>&1 || true
[ "${SCRATCH_REAPER_ENABLE:-1}" = "0" ] && exit 0
command -v python3 >/dev/null 2>&1 || exit 0

LINES="$(bash "$REAPER" --notice --deadline "$DEADLINE" 2>/dev/null || true)"

[ -n "$LINES" ] || exit 0

BODY="$(printf '=== SCRATCH ===\n%s\n  The whole plan, deleting nothing:  %s --verbose\n' \
    "$(printf '%s\n' "$LINES" | sed 's/^/  /')" "$REAPER")"

BODY="$BODY" python3 -c '
import json, os
body = os.environ.get("BODY", "")
print(json.dumps({
    "systemMessage": body,
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": body,
    },
}))
' 2>/dev/null || true
exit 0
