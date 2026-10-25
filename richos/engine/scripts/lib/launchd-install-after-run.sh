#!/usr/bin/env bash
#
# launchd-install-after-run.sh — apply a scheduled job's change once the run
#                                that was in progress has ended.
#
#   launchd-install-after-run.sh LABEL MAX_SECONDS POLL_SECONDS -- INSTALL_CMD...
#
# Started ONLY by launchd_install_after_run (launchd-install.sh, same
# directory), detached into its own session, when an --install found the job
# changed while a run was in progress. That install left the run alone; this
# finishes its job, so nobody has to install again.
#
#   * It waits, reading `launchctl print` every POLL_SECONDS, while the job is
#     running. It never stops, signals or reloads a running job.
#   * When the job is not running, it runs INSTALL_CMD, the job's own --install,
#     which regenerates the plist from the engine on disk and replaces the job
#     through launchd_install. Exit 3 from it means a new run started in the
#     gap, so it goes on waiting.
#   * Past MAX_SECONDS it GIVES UP, loudly: a line in the job's log (its stdout)
#     and the give-up file the disk watchdog's health check reports until a
#     later install succeeds. The job keeps its previous schedule meanwhile.
#   * The pid file names the one waiter per label. If it is removed (--uninstall)
#     or names another process, this one ends without applying anything.
#
# Everything it says goes to stdout, which its starter points at the job's own
# launchd log, one line per event, each with the time and the label.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=launchd-install.sh
. "$HERE/launchd-install.sh"

LABEL="${1:?label}"; MAX="${2:?max seconds}"; POLL="${3:?poll seconds}"; shift 3
[ "${1:-}" = "--" ] && shift
[ $# -gt 0 ] || { echo "launchd-install-after-run.sh: no install command" >&2; exit 2; }

TARGET="gui/$(id -u)/$LABEL"
PENDING="$(launchd_install_pending_file "$LABEL")"
GAVE_UP="$(launchd_install_gave_up_file "$LABEL")"

stamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
say() { printf '%s %s: %s\n' "$(stamp)" "$LABEL" "$*"; }
mine() { [ "$(tr -cd '0-9' <"$PENDING" 2>/dev/null)" = "$$" ]; }
# The one way this ends without applying: said in the log AND left for the
# disk watchdog's health check, never only one of the two.
give_up() {
    say "$1"
    printf '%s %s: %s\n' "$(stamp)" "$LABEL" "$1" >"$GAVE_UP"
    rm -f "$PENDING"
    exit "$2"
}
# The running pid on stdout while the job is running; status 1 otherwise.
running_pid() {
    _info="$(launchctl print "$TARGET" 2>/dev/null)" || return 1
    printf '%s\n' "$_info" | grep -qE '^[[:space:]]*state = running[[:space:]]*$' || return 1
    printf '%s\n' "$_info" | sed -n 's/^[[:space:]]*pid = \([0-9][0-9]*\).*/\1/p' | head -1
}

mkdir -p "$(dirname "$PENDING")"
printf '%s\n' "$$" >"$PENDING"
deadline=$(( $(date +%s) + MAX ))
say "CHANGE PENDING: the job changed while a run was in progress. Waiting (up to $((MAX / 60)) min, pid $$) for that run to end, then applying it with: $*"

while :; do
    if ! mine; then
        say "no longer pending (its pid file was removed or names another waiter); nothing applied"
        exit 0
    fi
    if run_pid="$(running_pid)"; then
        [ "$(date +%s)" -lt "$deadline" ] || give_up "GAVE UP: a changed schedule was NOT applied, because the run in progress (pid ${run_pid:-unknown}) was still running after $((MAX / 60)) min ($MAX s). That run was not touched, and the job keeps its previous schedule until an install succeeds." 4
        sleep "$POLL"
        continue
    fi
    [ "$(date +%s)" -lt "$deadline" ] || give_up "GAVE UP: a changed schedule was NOT applied within $((MAX / 60)) min ($MAX s); every attempt found a new run starting. The job keeps its previous schedule until an install succeeds." 4
    out="$("$@" 2>&1)"; rc=$?
    [ -z "$out" ] || printf '%s\n' "$out" | sed 's/^/    /'
    case "$rc" in
        0)
            rm -f "$PENDING"
            say "APPLIED: the run had ended, and the changed job is now loaded."
            exit 0 ;;
        3)
            say "a new run started before the change could be applied; still waiting"
            sleep "$POLL" ;;
        *)
            give_up "GAVE UP: the run had ended, but applying the changed job failed (exit $rc of: $*). See the lines above in this log." 2 ;;
    esac
done
