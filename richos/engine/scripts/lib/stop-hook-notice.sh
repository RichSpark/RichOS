#!/usr/bin/env bash
#
# scripts/lib/stop-hook-notice.sh — THE CHANNEL A Stop HOOK USES TO TELL THE
#                                    OPERATOR IT IS NOT PROTECTING HIM.
#
# ===========================================================================
# THE DEFECT THIS FILE REMOVES
# ===========================================================================
# guard-unresolved-claims.sh shipped with three ways to stop enforcing:
#
#     CHECK_UNRESOLVED_CLAIMS=0 in orchestration.config   (stood down)
#     python3 absent from PATH                            (cannot run)
#     guard-unresolved-claims.py missing                  (cannot run)
#
# Each one announced itself on STDERR and exited 0. Its own header says, of the
# first: "Never a silent permission: an opt-out that cannot be seen is a
# defense that decays into a rumour." It then wrote that sentence to a channel
# the operator cannot see, which made it the rumour.
#
# MEASURED, NOT ASSUMED — Claude Code 2.1.251, macOS, 2026-08-30. Three Stop
# hooks were registered in one headless session, each emitting a unique marker
# on a different channel, and the session stream was searched for all three:
#
#   channel                             transcript   operator stream
#   ---------------------------------   ----------   ---------------
#   stderr,  exit 0   (ZACHMARK_STDERR)     yes            NO
#   stdout,  exit 0   (ZACHMARK_PLAINOUT)   yes            NO
#   {"systemMessage":...}, exit 0           yes            YES
#
# The two invisible hooks DID run: the transcript holds a `hook_success`
# attachment for each, with exitCode 0 and a measured durationMs, carrying the
# marker in its `stdout` / `stderr` field. So their absence from the operator's
# scroll is INVISIBILITY, not non-execution — the positive probe that stops
# this table passing for the wrong reason. The third arrived as
# {"type":"system","subtype":"informational","content":"Stop says: <marker>"}.
#
# The same session shows the consequence end to end. With BOTH Stop hooks stood
# down in one adopted sandbox, turn-manifest.sh (which already used
# systemMessage) was announced twice in the operator stream, and
# guard-unresolved-claims.sh (which used stderr) appeared ZERO times — while
# the transcript proves it ran, twice, and stood down, twice.
#
# ONE FURTHER FACT, because the alternative was caution rather than evidence: a
# Stop hook may write systemMessage to stdout AND exit 2. Verified live — the
# turn was still refused (the hook re-fired, which only happens after a block)
# and the systemMessage rendered on the blocking turn as well. So a hook never
# has to choose between announcing and blocking.
#
# ===========================================================================
# WHEN IT SPEAKS — the actual design decision
# ===========================================================================
# The requirement is NOT that a running guard is visible. It is that a guard
# which is NOT running cannot be mistaken for one that is. Three candidate
# frequencies, argued from what the operator experiences:
#
#   EVERY TURN — rejected. A stand-down is a persistent, unchanging condition,
#     not an event. Identical text under every turn is text the eye is trained
#     to skip within a dozen turns, and the operator's rational response is to
#     mute it. A muted notice is WORSE than none: the protection is still off
#     and now there is a line on screen that reports it, which nobody reads.
#     That is the CEO's stated failure mode, and it is the one this file must
#     not walk into while fixing the other one.
#
#   ONCE PER SESSION — nearly right, and the half that is right is kept. A
#     session is the unit at which hooks are snapshotted, so it is the unit at
#     which WHAT IS LOADED can change; re-announcing each session means a
#     stand-down cannot decay into a rumour across days.
#
#   ON STATE CHANGE, with "nothing recorded yet this session" counting as a
#     change — CHOSEN. It is the once-per-session rule plus one addition that
#     makes silence mean something precise: EVERY transition is announced,
#     including the transition back to normal. So the operator's last-seen
#     notice for a hook is always its current state, and silence means "still
#     what I last told you" instead of "probably fine". Silence stopped being
#     evidence of health and became evidence of no-change, which is a fact
#     rather than a hope.
#
# A GUARD RUNNING NORMALLY SAYS NOTHING, EVER. The first observation of the
# normal state in a session records itself and prints nothing; only a RECOVERY
# — abnormal, then normal — is worth a line, and it is worth one precisely
# because the operator was told about the abnormal state and is owed the end of
# the story.
#
# NO THRESHOLD ANYWHERE. Whether a guard ran, and down which branch, is read
# off the guard's own control flow. There is nothing to tune, nothing to score,
# and no input on which this can be wrong: the false-positive rate is zero by
# construction, not by measurement.
#
# ===========================================================================
# WHEN THE LEDGER CANNOT BE WRITTEN, IT ANNOUNCES EVERY TURN
# ===========================================================================
# De-duplication needs somewhere to remember what it already said. If the
# entity root is unknown or its state directory is unwritable, there is no
# memory — and the fallback is to REPEAT, not to assume it has already been
# said. Degrading toward noise is recoverable by an operator who can read it;
# degrading toward silence rebuilds the defect. This is also why the root-
# resolution failure path, which by definition has no entity root, is the one
# condition that does announce every turn: a guard that cannot tell which
# repository it governs is not a guard, and it is not entitled to be quiet.
#
# ===========================================================================
# ONE PATH IS STILL INVISIBLE, AND IT IS NAMED RATHER THAN GLOSSED
# ===========================================================================
# Every notice a Stop hook raises AFTER its root bootstrap now comes through
# here. One notice is raised BEFORE it: the "BROKEN INSTALL — ENFORCEMENT IS
# NOT ACTIVE" banner a hook prints when scripts/lib/resolve-roots.sh itself is
# missing. That one still goes to stderr, so it still reaches nobody.
#
# It was left alone deliberately, not overlooked. That banner lives inside the
# root-resolution bootstrap block, which contract-integrity-probe.sh Layer R
# asserts is BYTE-IDENTICAL across all 21 rooted hooks. Changing it in the two
# Stop hooks alone turns Layer R red; changing it correctly means one
# coordinated edit to 21 files on every event, several of which other engineers
# are in right now. That is a different change with a different blast radius,
# and smuggling it in beside this one would be the wrong trade.
#
# What the gap actually costs, stated honestly rather than minimized: if
# resolve-roots.sh is missing then EVERY guard on EVERY event is off, not just
# these two — so the correct fix is engine-wide by nature. It is also the case
# that this helper lives in the same directory as resolve-roots.sh, so an
# install broken enough to lose one has probably lost the other; that path
# needs a fully inline announcement, which is a further reason it belongs in
# the coordinated change rather than here. engine-status.sh announces at
# SessionStart, but it carries the identical bootstrap, so it would be mute in
# exactly this scenario too. NOT VERIFIED EITHER WAY: whether a SessionStart
# hook's stderr reaches the operator's scroll. That was measured for Stop and
# only for Stop; do not assume it transfers.
#
# ===========================================================================
# USAGE
# ===========================================================================
#     stop_notice_init "<hook-basename.sh>" "<entity-root-or-empty>" "<payload>"
#     stop_notice_abnormal "<state-key>" "<operator message>"
#     stop_notice_normal   ["<recovery message>"]
#
# stop_notice_init is safe to call with an empty entity root — that is the
# announce-every-turn mode above. Call it before the first notice, on every
# path that can produce one.
#
# Safe to source repeatedly. Never changes the caller's cwd. Every function
# returns 0, because these are called from hooks running under `set -e` where a
# non-zero return from a notice would end the turn on the notice rather than on
# the condition being reported.

if [ -n "${_STOP_HOOK_NOTICE_SH_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_STOP_HOOK_NOTICE_SH_SOURCED=1

_SHN_HOOK=""
_SHN_LEDGER=""

# _shn_escape <text> — emit <text> as the BODY of a JSON string.
#
# Builtins only, deliberately. One of the states this file has to announce is
# "python3 is not on PATH", so a JSON encoder that needs python3 could not
# report the very condition it exists to report. Backslash first, or the
# escapes introduced afterwards get escaped again.
_shn_escape() {
    local s="${1:-}"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/ }"
    s="${s//$'\r'/ }"
    s="${s//$'\t'/ }"
    printf '%s' "$s"
    return 0
}

# _shn_emit <text> — the one channel proven to reach the operator.
#
# suppressOutput:true keeps the raw JSON out of the transcript's rendered
# output; systemMessage is what the host lifts and shows. The host prefixes
# every line with "Stop says: ", so messages are kept to one line.
_shn_emit() {
    printf '%s\n' "{\"suppressOutput\":true,\"systemMessage\":\"$(_shn_escape "${1:-}")\"}"
    return 0
}

# _shn_session_id <payload> — the Stop payload's session_id, or "".
#
# Confirmed present on a real Stop payload captured from 2.1.251: session_id,
# transcript_path, cwd, prompt_id, permission_mode, hook_event_name,
# stop_hook_active, last_assistant_message, background_tasks, session_crons.
_shn_session_id() {
    local payload="${1:-}" sid=""
    [ -n "$payload" ] || { printf '%s' ""; return 0; }
    if command -v python3 >/dev/null 2>&1; then
        sid="$(printf '%s' "$payload" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    print(str(d.get("session_id", "") or "") if isinstance(d, dict) else "")
except Exception:
    print("")
' 2>/dev/null || true)"
    fi
    if [ -z "$sid" ]; then
        # python3-free fallback, same shape as snapshot-agent-definitions.sh.
        sid="$(printf '%s' "$payload" | tr '\n' ' ' \
            | grep -o '"session_id"[[:space:]]*:[[:space:]]*"[^"]*"' \
            | head -1 | sed 's/.*"\([^"]*\)"$/\1/' 2>/dev/null || true)"
    fi
    printf '%s' "$sid"
    return 0
}

# stop_notice_init <hook-basename.sh> <entity-root-or-empty> <payload>
stop_notice_init() {
    local hook="${1:-unknown-hook.sh}" root="${2:-}" payload="${3:-}"
    local sid short dir

    _SHN_HOOK="$hook"
    _SHN_LEDGER=""

    [ -n "$root" ] || return 0
    [ -d "$root" ] || return 0

    sid="$(_shn_session_id "$payload")"
    short="$(printf '%s' "$sid" | tr -cd '[:alnum:]-' | cut -c1-8)"
    # No session id means no way to tell this session's notices from the last
    # one's. Rather than share a bucket across sessions — which would let a
    # notice announced yesterday suppress today's — leave the ledger unset and
    # announce every turn.
    [ -n "$short" ] || return 0

    dir="$root/.claude/state/stop-hook-notices"
    mkdir -p "$dir" 2>/dev/null || return 0
    [ -d "$dir" ] || return 0

    _SHN_LEDGER="$dir/${short}.${hook%.sh}.state"
    return 0
}

# _shn_prior — the last state recorded for this (session, hook), or "".
#
# THE LEDGER LINE IS `<state><TAB><epoch-of-last-emission>` since 2026-09-10.
# A bare state with no tab is the older format and reads as "emitted at a time
# nobody recorded", which the recurring rung treats as due — the safe
# direction, and it means an upgrade in mid-session speaks once rather than
# going quiet.
_shn_prior() {
    if [ -n "$_SHN_LEDGER" ] && [ -f "$_SHN_LEDGER" ]; then
        cut -f1 "$_SHN_LEDGER" 2>/dev/null | head -1 || true
    fi
    return 0
}

# _shn_prior_emit_epoch — when this state was last ANNOUNCED, or "".
_shn_prior_emit_epoch() {
    if [ -n "$_SHN_LEDGER" ] && [ -f "$_SHN_LEDGER" ]; then
        awk -F'\t' 'NR==1 && NF>1 {print $2}' "$_SHN_LEDGER" 2>/dev/null || true
    fi
    return 0
}

# _shn_record <state-key> [last-emit-epoch] — best effort. A ledger that cannot
# be written is the announce-every-turn mode, which is the safe direction.
_shn_record() {
    [ -n "$_SHN_LEDGER" ] || return 0
    printf '%s\t%s\n' "${1:-}" "${2:-}" > "$_SHN_LEDGER" 2>/dev/null || true
    return 0
}

# stop_notice_lead <state-key> <message>
#
# TO THE LEAD, once per state: stop_notice_abnormal's state-change dedup on the MODEL channel
# (Stop hookSpecificOutput.additionalContext), which the lead reads and answers in a continued
# turn; a systemMessage reaches only the person (orchestration reliability R6). Measured: the
# frozen 2026-10-23 native request carried such a notice as model input and the lead answered it
# (richos-hq docs/verification/2026-10-23-orchestration-reliability-prd/author-v2-checks.json).
stop_notice_lead() {
    local state="${1:-lead}" msg="${2:-}"
    if [ "$(_shn_prior)" != "$state" ]; then
        _shn_record "$state" "$(date +%s 2>/dev/null || echo '')"
        printf '%s\n' "{\"hookSpecificOutput\":{\"hookEventName\":\"Stop\",\"additionalContext\":\"$(_shn_escape "$msg")\"}}"
    else
        _shn_record "$state" "$(_shn_prior_emit_epoch)"
    fi
    return 0
}

# stop_notice_seen <key> — 0 when <key> was already told in this session (a seen-set beside
# the ledger, one key per line); else records it and returns 1. Without a ledger nothing is
# remembered and every key is new (announce, the safe direction).
stop_notice_seen() {
    local seen
    [ -n "$_SHN_LEDGER" ] || return 1
    seen="${_SHN_LEDGER%.state}.seen"
    if grep -qxF -- "${1:-}" "$seen" 2>/dev/null; then
        return 0
    fi
    printf '%s\n' "${1:-}" >> "$seen" 2>/dev/null || true
    return 1
}

# stop_notice_abnormal <state-key> <message>
#
# The guard is not doing its job. Announce on entry to this state, and stay
# quiet while it persists.
stop_notice_abnormal() {
    local state="${1:-abnormal}" msg="${2:-}" prior now
    prior="$(_shn_prior)"
    now="$(date +%s 2>/dev/null || echo '')"
    if [ "$prior" != "$state" ]; then
        _shn_record "$state" "$now"
        _shn_emit "$msg"
    else
        _shn_record "$state" "$(_shn_prior_emit_epoch)"
    fi
    return 0
}

# stop_notice_abnormal_recurring <state-key> <message> [interval-seconds]
#
# THE SAME DE-DUPLICATION, WITH A RUNG SO IT CANNOT GO SILENT FOR A WORKING DAY.
#
# `stop_notice_abnormal` announces on ENTRY to a state and then stays quiet
# while it persists. That is right for most conditions and it has one failure
# mode, recorded on 2026-09-10 as type F: a control that goes QUIETER as the
# failure persists. The evidence pack's own 3.1 describes notice-unlanded-
# branches.sh failing in exactly this shape — "it spoke once, was right, and
# went silent for a working day" — and the land-disposition demand inherited it
# whole: with the escalation ladder re-announcing at 1 h, 24 h and 72 h, a
# demand raised at 3 h of standing is announced at ~3 h and ~4 h and then NOT
# AGAIN UNTIL ~27 h. A 23-hour silence that begins where a working day begins.
#
# So a caller that owns a PERSISTING condition uses this instead: announce on
# entry, and again every `interval` seconds while it persists. The interval is
# deliberately sparse — the reason for de-duplicating is real ("a condition
# repeated under every turn is a condition the eye is trained to skip"), and a
# rung between 1 h and 24 h answers the gap without becoming wallpaper.
#
# 0 or an unset interval is exactly the old behavior.
stop_notice_abnormal_recurring() {
    local state="${1:-abnormal}" msg="${2:-}" interval="${3:-0}" prior prior_ts now due=0
    prior="$(_shn_prior)"
    prior_ts="$(_shn_prior_emit_epoch)"
    now="$(date +%s 2>/dev/null || echo '')"
    if [ "$prior" != "$state" ]; then
        due=1
    elif [ -n "$now" ] && [ "${interval:-0}" -gt 0 ] 2>/dev/null; then
        if [ -z "$prior_ts" ]; then
            # The state persists but nothing recorded WHEN it was last said —
            # an older ledger line, or a write that failed. Speak: an unknown
            # last-spoken time is the same class as absence, and absence is
            # never evidence that the operator has heard this.
            due=1
        elif [ $((now - prior_ts)) -ge "$interval" ] 2>/dev/null; then
            due=1
        fi
    fi
    if [ "$due" -eq 1 ]; then
        _shn_record "$state" "$now"
        _shn_emit "$msg"
    else
        _shn_record "$state" "$prior_ts"
    fi
    return 0
}

# stop_notice_normal [recovery-message]
#
# The guard is doing its job. This prints NOTHING in the ordinary case — which
# is every turn of a healthy session. It speaks only to close a story it
# already started: the operator was told the guard was off, so he is told when
# it comes back.
stop_notice_normal() {
    local msg="${1:-}" prior now
    prior="$(_shn_prior)"
    now="$(date +%s 2>/dev/null || echo '')"
    _shn_record "ok" "$now"
    if [ -n "$prior" ] && [ "$prior" != "ok" ] && [ -n "$msg" ]; then
        _shn_emit "$msg"
    fi
    return 0
}
