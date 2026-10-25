# shellcheck shell=bash
# shellcheck disable=SC2034  # LAUNCHD_INSTALL_* are this file's results, read by the caller
#
# launchd-install.sh — put a generated plist under launchd WITHOUT disturbing a
#                      job that is already right, never kill a run, and apply a
#                      change that had to wait for a run by itself when it ends.
#
# Sourced by the --install of scratch-reaper.sh and disk-watchdog.sh, the two
# jobs install.sh arms on every engine install, and by
# launchd-install-after-run.sh, the continuation described below.
#
# 2026-10-25. Both --install paths used to run `launchctl bootout` and then
# `bootstrap` every time, even when nothing had changed. install.sh calls them
# on every land, so every land unloaded the reaper. REPRODUCED: bootout sends
# SIGTERM to a run in progress, and a killed cleanup leaves no trace at all
# (Python's buffered plan never reaches the log, no state is written, the job's
# run history is reset). INFERRED, NOT PROVEN: that this is why the 04:40
# cleanup that morning left nothing behind. The job had been loaded since 03:57
# with the Mac awake, a land's install.sh booted it out at 04:42:24, inside the
# 2-26 minutes a run takes, and the 10:40 run the same day wrote nothing to its
# log for its first minutes, which is what a run killed at that point leaves.
# The timing fits closely, but no surviving record shows that a 04:40 process
# started and was killed.
#
# So: a loaded job whose plist on disk is byte-identical to the new one is left
# exactly as it is. A job that really changed is replaced only when it is not
# running; while a run is in progress it is NOT touched, and the change is
# applied BY ITSELF when that run ends (launchd_install_after_run below). Nobody
# has to install again.
#
# WHY THE PLIST ON DISK STANDS FOR THE LOADED JOB: this function writes the new
# plist only after the old job is confirmed unloaded (or was never loaded), and
# bootstraps from that file. So "loaded, and the file equals what we would
# write" means the job was loaded from exactly this content. A failed bootstrap
# leaves the job unloaded, which the next install sees and loads.

# Where the pending change's two files live: the state directory every other
# state file resolves (CLAUDE_CONFIG_DIR, else ~/.claude), which is also where
# the disk watchdog's health check reads the give-up file.
launchd_install_state_dir() {
    printf '%s/state' "${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
}
# LABEL -> the pid file of the continuation waiting to apply its change.
launchd_install_pending_file() {
    printf '%s/launchd-pending-%s.pid' "$(launchd_install_state_dir)" "$1"
}
# LABEL -> the file a continuation that gave up leaves for the health check.
# disk-watchdog.py cleanup_health() reads exactly this name.
launchd_install_gave_up_file() {
    printf '%s/launchd-pending-%s.gave-up' "$(launchd_install_state_dir)" "$1"
}

# launchd_install LABEL PLIST NEWFILE
#   NEWFILE is the generated plist, written beside PLIST by the caller; this
#   function moves it into place or removes it.
#   Sets LAUNCHD_INSTALL_RESULT to one of:
#     unchanged  loaded, identical; nothing was touched          (return 0)
#     loaded     was not loaded; bootstrapped                    (return 0)
#     replaced   loaded but different, not running; replaced     (return 0)
#     running    different, but a run is in progress; NOT touched (return 3)
#     failed     launchd refused; LAUNCHD_INSTALL_DETAIL says how (return 2)
#   LAUNCHD_INSTALL_PID is the running pid for `running`.
#   Every return 0 means the job now runs from this content, so an earlier
#   give-up for this label is no longer true and its file is removed.
launchd_install() {
    _li_label="$1"; _li_plist="$2"; _li_new="$3"
    _li_domain="gui/$(id -u)"
    _li_target="$_li_domain/$_li_label"
    LAUNCHD_INSTALL_RESULT=""; LAUNCHD_INSTALL_DETAIL=""; LAUNCHD_INSTALL_PID=""

    if _li_info="$(launchctl print "$_li_target" 2>/dev/null)"; then
        if [ -f "$_li_plist" ] && cmp -s "$_li_new" "$_li_plist"; then
            rm -f "$_li_new" "$(launchd_install_gave_up_file "$_li_label")"
            LAUNCHD_INSTALL_RESULT=unchanged
            return 0
        fi
        if printf '%s\n' "$_li_info" | grep -qE '^[[:space:]]*state = running[[:space:]]*$'; then
            LAUNCHD_INSTALL_PID="$(printf '%s\n' "$_li_info" \
                | sed -n 's/^[[:space:]]*pid = \([0-9][0-9]*\).*/\1/p' | head -1)"
            rm -f "$_li_new"
            LAUNCHD_INSTALL_RESULT=running
            return 3
        fi
        launchctl bootout "$_li_target" >/dev/null 2>&1 || true
        if launchctl print "$_li_target" >/dev/null 2>&1; then
            rm -f "$_li_new"
            LAUNCHD_INSTALL_RESULT=failed
            LAUNCHD_INSTALL_DETAIL="launchctl bootout did not unload the old job; it is left as it was"
            return 2
        fi
        LAUNCHD_INSTALL_RESULT=replaced
    else
        LAUNCHD_INSTALL_RESULT=loaded
    fi

    mv -f "$_li_new" "$_li_plist"
    if ! launchctl bootstrap "$_li_domain" "$_li_plist" >/dev/null 2>&1; then
        LAUNCHD_INSTALL_RESULT=failed
        LAUNCHD_INSTALL_DETAIL="wrote $_li_plist but launchctl bootstrap failed"
        return 2
    fi
    if ! launchctl print "$_li_target" >/dev/null 2>&1; then
        LAUNCHD_INSTALL_RESULT=failed
        LAUNCHD_INSTALL_DETAIL="launchctl does not list $_li_label after bootstrap"
        return 2
    fi
    rm -f "$(launchd_install_gave_up_file "$_li_label")"
    return 0
}

# launchd_install_pending_pid LABEL
#   Prints the pid of this label's live continuation, or nothing. The pid file
#   is only believed when that pid is alive AND is this continuation for this
#   label, so a stale file or a reused pid never counts as pending.
launchd_install_pending_pid() {
    _lp_file="$(launchd_install_pending_file "$1")"
    [ -f "$_lp_file" ] || return 0
    _lp_pid="$(tr -cd '0-9' <"$_lp_file")"
    [ -n "$_lp_pid" ] || return 0
    _lp_cmd="$(ps -p "$_lp_pid" -o command= 2>/dev/null || true)"
    case "$_lp_cmd" in
        *launchd-install-after-run.sh*" $1 "*) printf '%s' "$_lp_pid" ;;
    esac
}

# launchd_install_after_run LABEL LOG MAX_SECONDS POLL_SECONDS INSTALL_CMD...
#   For `running`: start the ONE continuation that applies this job's change
#   when the run ends. It is detached into its own session (os.setsid), so the
#   install that started it can finish and its terminal can close; it waits,
#   polling `launchctl print` every POLL_SECONDS and never touching the run,
#   until the job is no longer running, then runs INSTALL_CMD (the job's own
#   --install, which regenerates the plist from the engine on disk and goes
#   through launchd_install above). It writes what it did to LOG, the job's own
#   launchd log. Past MAX_SECONDS it gives up: a line in LOG and the give-up
#   file the disk watchdog's health check reports until a later install works.
#
#   If a continuation for this label is already waiting, none is added: that
#   one runs the --install of the engine on disk when it applies, so it applies
#   the newest change too.
#   Sets LAUNCHD_INSTALL_CONTINUATION (started|already) and
#   LAUNCHD_INSTALL_CONTINUATION_PID; returns 2 with LAUNCHD_INSTALL_DETAIL
#   when it could not be started (the give-up file is written then too).
launchd_install_after_run() {
    _la_label="$1"; _la_log="$2"; _la_max="$3"; _la_poll="$4"; shift 4
    LAUNCHD_INSTALL_CONTINUATION=""; LAUNCHD_INSTALL_CONTINUATION_PID=""
    _la_here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    _la_gave_up="$(launchd_install_gave_up_file "$_la_label")"
    mkdir -p "$(launchd_install_state_dir)" "$(dirname "$_la_log")" 2>/dev/null || true

    case "$_la_max" in ''|*[!0-9]*) _la_max="" ;; esac
    case "$_la_poll" in ''|*[!0-9]*) _la_poll="" ;; esac
    if [ -z "$_la_max" ] || [ -z "$_la_poll" ]; then
        LAUNCHD_INSTALL_DETAIL="LAUNCHD_APPLY_WAIT_MAX_SECONDS / LAUNCHD_APPLY_WAIT_POLL_SECONDS are not declared as whole seconds in orchestration.config, so the change cannot wait for the run"
        printf '%s %s: the changed job was not applied: %s\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$_la_label" "$LAUNCHD_INSTALL_DETAIL" >"$_la_gave_up"
        return 2
    fi
    _la_pid="$(launchd_install_pending_pid "$_la_label")"
    if [ -n "$_la_pid" ]; then
        LAUNCHD_INSTALL_CONTINUATION=already
        LAUNCHD_INSTALL_CONTINUATION_PID="$_la_pid"
        return 0
    fi
    if ! command -v python3 >/dev/null 2>&1; then
        LAUNCHD_INSTALL_DETAIL="python3 is not on PATH, so the change cannot wait for the run"
        printf '%s %s: the changed job was not applied: %s\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$_la_label" "$LAUNCHD_INSTALL_DETAIL" >"$_la_gave_up"
        return 2
    fi
    # The python3 process EXECS bash, so $! is the continuation's own pid.
    # shellcheck disable=SC2016
    python3 -c 'import os, sys; os.setsid(); os.execv(sys.argv[1], sys.argv[1:])' \
        /bin/bash "$_la_here/launchd-install-after-run.sh" \
        "$_la_label" "$_la_max" "$_la_poll" -- "$@" \
        </dev/null >>"$_la_log" 2>&1 &
    _la_pid=$!
    printf '%s\n' "$_la_pid" >"$(launchd_install_pending_file "$_la_label")"
    LAUNCHD_INSTALL_CONTINUATION=started
    LAUNCHD_INSTALL_CONTINUATION_PID="$_la_pid"
    return 0
}
