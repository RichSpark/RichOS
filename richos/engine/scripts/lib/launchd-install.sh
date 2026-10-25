# shellcheck shell=bash
# shellcheck disable=SC2034  # LAUNCHD_INSTALL_* are this file's results, read by the caller
#
# launchd-install.sh — put a generated plist under launchd WITHOUT disturbing a
#                      job that is already right, and never kill a run.
#
# Sourced by the --install of scratch-reaper.sh and disk-watchdog.sh, the two
# jobs install.sh arms on every engine install.
#
# 2026-10-25. Both --install paths used to run `launchctl bootout` and then
# `bootstrap` every time, even when nothing had changed. install.sh calls them
# on every land, so every land unloaded the reaper: a cleanup in the middle of
# its scan was killed (bootout sends SIGTERM; Python's buffered plan never
# reaches the log, so the killed run leaves no trace at all) and the job's run
# history was reset. That morning the 04:40 cleanup, due with the Mac awake and
# the job loaded since 03:57, left nothing behind; a land's install.sh booted
# the job out at 04:42:24, inside the 2-26 minutes a run takes. The 10:40 run
# the same day started on time and wrote nothing to its log for its first
# minutes, which is exactly what a run killed at that point leaves.
#
# So: a loaded job whose plist on disk is byte-identical to the new one is left
# exactly as it is. A job that really changed is replaced only when it is not
# running; while a run is in progress it is NOT touched and the caller is told.
#
# WHY THE PLIST ON DISK STANDS FOR THE LOADED JOB: this function writes the new
# plist only after the old job is confirmed unloaded (or was never loaded), and
# bootstraps from that file. So "loaded, and the file equals what we would
# write" means the job was loaded from exactly this content. A failed bootstrap
# leaves the job unloaded, which the next install sees and loads.
#
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
launchd_install() {
    _li_label="$1"; _li_plist="$2"; _li_new="$3"
    _li_domain="gui/$(id -u)"
    _li_target="$_li_domain/$_li_label"
    LAUNCHD_INSTALL_RESULT=""; LAUNCHD_INSTALL_DETAIL=""; LAUNCHD_INSTALL_PID=""

    if _li_info="$(launchctl print "$_li_target" 2>/dev/null)"; then
        if [ -f "$_li_plist" ] && cmp -s "$_li_new" "$_li_plist"; then
            rm -f "$_li_new"
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
    return 0
}
