#!/usr/bin/env bash
#
# notice-disk-alert.sh — RICH IS TOLD, AT EVERY SESSION START AND EVERY TURN
#                        END, UNTIL IT CLEARS.
#
# ===========================================================================
# WHAT THIS IS
# ===========================================================================
# CEO, 2026-09-18 (ceo-decisions §54), verbatim: "...if the clean-up fails or
# impossible for some reason, then Rich must get a MASSIVE ALERT about it and get
# on with manually deleting the garbage if it fails to be deleted automatically."
#
# The launchd timer takes the readings and notifies the CEO. THIS is the other
# half — the half that reaches Rich inside the session, where the work happens.
# It reads `disk-watchdog.sh --alert` and says nothing at all when that says
# nothing, which on a healthy machine is always.
#
# Registered on BOTH SessionStart and Stop, and it renders differently on each,
# because the two channels are not the same shape:
#
#   SessionStart — systemMessage + additionalContext, which take a BLOCK. Rich
#                  gets the whole alert: every volume, every stuck path, the top
#                  consumers, the classification and what to do.
#   Stop         — ONE LINE. The host prefixes each line with "Stop says: ", so
#                  a block renders as noise; the turn end gets a summary and
#                  points at the full reading.
#
# ===========================================================================
# WHY THIS ONE DEFEATS THE DE-DUPLICATION EVERY OTHER Stop NOTICE HONORS
# ===========================================================================
# stop-hook-notice.sh exists because "a condition repeated under every turn is a
# condition the eye is trained to skip", and almost every caller is right to
# announce on entry and then go quiet.
#
# THIS CALLER IS THE EXCEPTION AND THE CEO NAMED IT: the alert is "repeated every
# turn until the condition clears". That is an explicit instruction to be
# wallpaper until somebody fixes it, and it is the correct trade for this one
# condition — a disk with 20 GB left does not become less urgent because it was
# mentioned last turn, and the failure mode of going quiet is that the machine
# stops working mid-task.
#
# It is done through the library's emission (stop_notice_abnormal_recurring)
# with no ledger, so every turn it is said again, unconditionally: the ledger
# below is for the kept set only, and an alarm never consults it.
#
# ONE THING IS NOT AN ALARM AND IS TOLD ONCE (CEO, 2026-10-24: "remove obsolete
# instructions and repeated administrative noise without hiding genuine
# failures"): leftovers the scheduled cleanup kept or left undecided by a known
# condition. The watchdog prints them under "KEPT OR UNDECIDED" with the set's
# identity (which entries, in which condition); this line tells a
# set once a session, and a new or changed set once more. Low or falling disk, a
# failed deletion, a failed, missing or stale run and a failed app or device
# cleanup are alarms and still repeat every turn.
#
# ===========================================================================
# IT NEVER BLOCKS AND IT NEVER COSTS A DF
# ===========================================================================
# `--alert` reads the state file the launchd job writes; it takes no reading of
# its own and, critically, never WRITES the state file. A turn end that moved the
# reading forward would destroy the baseline the 20 GB drop arithmetic subtracts
# from, silently turning a 15-minute rate alarm into a per-turn-end one that can
# never see a drop. disk-watchdog.test.sh W14 is that property.
#
# Every exit is 0. A notice that takes the session down with it is worse than no
# notice.

# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — reads machine watchdog state, never a tool decision.
set -uo pipefail

EVENT="SessionStart"
while [ $# -gt 0 ]; do
    case "$1" in
        --event) EVENT="${2:-SessionStart}"; shift ;;
        *)       : ;;
    esac
    shift
done

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
  hook: scripts/hooks/notice-disk-alert.sh
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

HOOK_DIR="$SCRIPT_DIR"
SCRIPTS="$ENGINE_ROOT/scripts"
WD="$SCRIPTS/disk-watchdog.sh"

[ -x "$WD" ] || exit 0

# stdin is drained on the Stop path because the host writes the payload there and
# a hook that leaves it unread can make the host block. Done unconditionally and
# with a timeout, the way the other notices do it.
# READ AS EVERY OTHER Stop HOOK READS IT. This was `timeout 2 cat`, and macOS has
# no `timeout`, so the payload was always empty here: no session id, so no
# ledger, so nothing this hook said was ever remembered. A terminal is never read.
PAYLOAD=""
if [ "$EVENT" = "Stop" ] && [ ! -t 0 ]; then
    PAYLOAD="$(cat 2>/dev/null || true)"
fi

BODY="$(bash "$WD" --alert 2>/dev/null || true)"

# THE LEDGER IS THE SESSION'S, so a kept leftover told once is remembered (below).
# A root that cannot be resolved leaves it unset, and then everything is told
# every turn: the library's safe direction.
NOTICE_ROOT=""
if [ "$EVENT" = "Stop" ] && resolve_entity_root "$PAYLOAD" >/dev/null 2>&1; then
    NOTICE_ROOT="${RICHOS_ENTITY_ROOT_RESOLVED:-}"
fi

if [ -z "$BODY" ]; then
    # Nothing wrong. On Stop, tell the ledger the condition has CLEARED, so the
    # next entry into it announces again instead of being de-duplicated against
    # a state that ended hours ago.
    if [ "$EVENT" = "Stop" ] && [ -f "$SCRIPTS/lib/stop-hook-notice.sh" ]; then
        # shellcheck source=../lib/stop-hook-notice.sh
        . "$SCRIPTS/lib/stop-hook-notice.sh" 2>/dev/null || exit 0
        stop_notice_init "notice-disk-alert.sh" "$NOTICE_ROOT" "$PAYLOAD" 2>/dev/null || exit 0
        stop_notice_normal 2>/dev/null || true
    fi
    exit 0
fi

if [ "$EVENT" = "Stop" ]; then
    [ -f "$SCRIPTS/lib/stop-hook-notice.sh" ] || exit 0
    # shellcheck source=../lib/stop-hook-notice.sh
    . "$SCRIPTS/lib/stop-hook-notice.sh" 2>/dev/null || exit 0
    stop_notice_init "notice-disk-alert.sh" "$NOTICE_ROOT" "$PAYLOAD" 2>/dev/null || exit 0

    # ONE LINE, assembled from the block. The interesting facts are the volume
    # lines, the stuck-path count and the classification; the "what to do"
    # section is left for SessionStart and for --status.
    # ===================================================================
    # THE LINE THE CEO ASKED ABOUT WAS WRITTEN HERE
    # ===================================================================
    # 2026-09-19, verbatim: "And what is this all about: MASSIVE ALERT — DISK:
    # 218 path(s) could not be deleted (delete BY HAND) | 13.9 GB in 4122
    # place(s) NOTHING WILL EVER COLLECT | 11.4 GB in 2 place(s) UNDECIDABLE —
    # no run will clear it".
    #
    # Three clauses, and each was a different thing wearing the same words:
    # 200 of the 218 were a defect in the sweeper's own delete loop and 18 were
    # directories the kernel owns; 12.8 of the 13.9 GB were our campaign roots,
    # each already printing the command that reclaims it, added to 1.15 GB of
    # other programs' temporary files; the 11.4 GB had been deleted by hand an
    # hour before the line was printed. He could act on none of it, and it was
    # repeated every turn.
    #
    # SO THE SUMMARY NOW SPLITS THE SAME WAY THE BLOCK DOES. Only what a person
    # must ACT on reaches this line, and only an ALARM wears the words MASSIVE
    # ALERT. Another program's temporary files never appear here at all: they
    # are in the SessionStart block, where they are information, and a turn-end
    # line about files nobody may touch is the definition of wallpaper.
    PARSED="$(printf '%s\n' "$BODY" | python3 -c '
import re, sys
text = sys.stdin.read()
vols, stuck, inst, klass, notinstalled, stopped = [], "", "", "", False, False
todo = []
cleanup = ""
kept, kept_set = "", ""
for line in text.splitlines():
    s = line.strip()
    if "NOT INSTALLED" in s:
        notinstalled = True
    if "STOPPED REPORTING" in s:
        stopped = True
    m = re.match(r"^(/\S+) — (.+?) FREE of (\S+ \S+) \(([\d.]+)%\): (.*)$", s)
    if m:
        vols.append("%s %s free (%s%%) — %s" % (m.group(1), m.group(2),
                                                m.group(4), m.group(5)))
    m = re.match(r"^(\d+) GARBAGE PATH\(S\) COULD NOT BE DELETED", s)
    if m:
        stuck = "%s path(s) could not be deleted (delete BY HAND)" % m.group(1)
    m = re.match(r"^(\d+) TEST APP INSTANCE\(S\) WOULD NOT CLOSE", s)
    if m:
        inst = "%s test app instance(s) would not close" % m.group(1)
    # The scheduled cleanup has not recovered (cleanup PRD R5): the watchdog line
    # already is one short summary with its owner action and detail path.
    m = re.match(r"^CLEANUP HAS NOT RECOVERED: (.*)$", s)
    if m:
        cleanup = "cleanup has not recovered: " + m.group(1)[:400]
    # OURS AND NEVER DELETED AUTOMATICALLY — a campaign root past its retention,
    # a running throwaway container. Not an alarm, but the one pile a person
    # gets space back from, so it keeps its place on this line.
    m = re.match(r"^(\S+ \S+) in (\d+) place\(s\) IS OURS AND IS NEVER DELETED", s)
    if m:
        todo.append("%s in %s place(s) is ours and waits for a person to remove it"
                    % (m.group(1), m.group(2)))
    if s.startswith("CLASSIFICATION:"):
        klass = s.split(":", 1)[1].strip()
    # KEPT OR UNDECIDED: not an alarm. Told once per set (the shell below), so
    # the set id travels apart from the words.
    m = re.match(r"^KEPT OR UNDECIDED \(not an alert\): (.*) \(set ([0-9a-f]+)\)$", s)
    if m:
        kept = ("Cleanup, kept or undecided: %s; told once per set. Which, and why: "
                "scripts/disk-watchdog.sh --status" % m.group(1))
        kept_set = m.group(2)
# ALARM and REPORT are assembled separately, and the prefix follows the ALARM.
alarm = []
if notinstalled:
    alarm.append("THE DISK WATCHDOG IS NOT INSTALLED — nothing is watching free space")
if stopped:
    alarm.append("THE DISK WATCHDOG HAS STOPPED REPORTING")
alarm.extend(vols)
if stuck:
    alarm.append(stuck)
if inst:
    alarm.append(inst)
if cleanup:
    alarm.append(cleanup)
if alarm and klass:
    alarm.append("classification: " + klass)
summary = ""
if alarm:
    summary = ("MASSIVE ALERT — DISK: " + " | ".join(alarm + todo)
               + ". Full reading: scripts/disk-watchdog.sh --status")
elif todo:
    # NO ALARM WORDS. Nothing is broken and nothing is at risk; there is simply
    # space a person can reclaim with a command the full reading prints.
    summary = ("Disk: " + " | ".join(todo)
               + ". Which, and the command: scripts/disk-watchdog.sh --status")
# AND NOTHING AT ALL when the block held only other programs temp. That is in
# the SessionStart block to be read, not on every turn end to be scrolled past.
# Three lines, always: the summary, the kept set id and the kept sentence.
print(summary)
print(kept_set)
print(kept)
' 2>/dev/null || echo "")"
    SUMMARY="$(printf '%s\n' "$PARSED" | sed -n 1p)"
    KEPT_SET="$(printf '%s\n' "$PARSED" | sed -n 2p)"
    KEPT="$(printf '%s\n' "$PARSED" | sed -n 3p)"

    # KEPT LEFTOVERS ARE TOLD ONCE PER SET, and only they are. Known, kept
    # leftovers are administrative: an unchanged set is not news, so it is told
    # once a session, and a new leftover or a changed set (its id is WHICH
    # entries, never their size or age) is told once more. Everything that is
    # an alarm keeps repeating every turn below, whatever was told before.
    if [ -n "$KEPT" ] && [ -n "$KEPT_SET" ] && stop_notice_seen "kept:$KEPT_SET"; then
        KEPT=""
    fi

    if [ -z "$SUMMARY" ]; then
        if [ -n "$KEPT" ]; then
            stop_notice_abnormal "kept:$KEPT_SET" "$KEPT" 2>/dev/null || true
            exit 0
        fi
        # The block said something, but nothing a person must act on. Treat it
        # exactly as the empty-block path above: tell the ledger the condition
        # has cleared so the next real one announces afresh.
        stop_notice_normal 2>/dev/null || true
        exit 0
    fi
    if [ -n "$KEPT" ]; then
        SUMMARY="$SUMMARY | $KEPT"
    fi
    # AN ALARM IS TOLD EVERY TURN, with no memory at all, exactly as before the
    # ledger above existed: its key is only how many MASSIVE ALERT banners there
    # are, so remembering it could hide a later, different failure.
    stop_notice_init "notice-disk-alert.sh" "" "$PAYLOAD" 2>/dev/null || exit 0

    # THE STATE KEY IS THE CONDITION, NOT THE NUMBER. Keying on free bytes would
    # make every reading a new state and defeat the ledger entirely; keying on
    # WHICH conditions hold means the recurrence below is what drives repetition,
    # which is the behavior that was actually asked for.
    KEY="$(printf '%s' "$BODY" | grep -c 'MASSIVE ALERT')"
    stop_notice_abnormal_recurring "disk-alert:$KEY" "$SUMMARY" \
        1 2>/dev/null || true
    exit 0
fi

# --- SessionStart: the whole block, on both channels ------------------------
# systemMessage for the operator and additionalContext for the model, which is
# the pair session-start-escalations.sh uses. NEVER stderr: measured 2026-09-14
# to reach no channel at all at SessionStart
# (docs/verification/check-census-2026-09-14.md).
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
