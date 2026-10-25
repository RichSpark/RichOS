#!/usr/bin/env bash
#
# scratch-reaper.sh — SCRATCH NOBODY IS USING GETS DELETED ON A SCHEDULE, AND
#                     EVERY DELETION IS ON THE RECORD.
#
# ===========================================================================
# THE MORNING THIS EXISTS FOR
# ===========================================================================
# 2026-09-17. macOS said "Your disk is almost full". `df -h /` showed 1.8 GB
# free of 460 GB. 19 GB of scratch came back — three finished sessions'
# scratchpads, one session's build and bisect leftovers, a stale build cache —
# because a person read the warning and deleted them BY HAND.
#
# The CEO, that morning: "There needs to be a mechanism to make sure that
# scratch directories get deleted on a regular basis. I don't want piles of
# garbage to pile up endlessly."
#
# The failure was never the garbage. It was that reclaiming it took a warning
# and a person, which means on the day nobody is at the machine it does not
# happen at all.
#
# ===========================================================================
# WHAT IT DOES, AND THE ONE THING IT WILL NOT DO
# ===========================================================================
# Deletes, only ever after PROVING nobody owns it (scripts/lib/scratch-reaper.py
# carries the proof and the four walls):
#
#   * a finished session's Claude scratchpad and tasks
#   * the empty scratch root a finished session leaves behind
#   * a file in a scratch root that no running session can have written
#   * a harness temp workspace under $TMPDIR that no process holds open
#   * nightly build products beyond the declared retention
#
# It will not delete anything a live session owns, any registered workspace,
# anything containing a .git, ~/RichOS, ~/.richos-signing, the nightly source
# worktree or its runtime. MEASURED on the morning it was written: the single
# biggest pile on the machine, 4.27 GB, belonged to the session that was
# RUNNING — so this program would not have prevented that disk-full warning,
# and saying so is worth more than a claim that it would.
#
# ===========================================================================
# USAGE
# ===========================================================================
#   scratch-reaper.sh                 the plan. DELETES NOTHING.
#   scratch-reaper.sh --verbose       the plan including every KEEP and why
#   scratch-reaper.sh --apply         delete, and write the log
#   scratch-reaper.sh --json          the plan as a record
#   scratch-reaper.sh --notice        one line, only if the declared threshold
#                                     of reclaimable scratch is exceeded
#   scratch-reaper.sh --install       schedule it (launchd, every 6 hours)
#   scratch-reaper.sh --uninstall     unschedule it
#
# NOT DELETING IS THE DEFAULT, and that is deliberate: a program whose safe
# mode needs a flag is a program that deletes the first time somebody types it
# wrong. `--dry-run` is accepted and means the default.
#
# The plan printed by --apply is byte-identical to the plan printed without it,
# plus one final `applied:` line. The scan happens before any deletion and the
# report carries no timestamp, so the two can be diffed — which is how the
# test proves that --apply decides nothing --dry-run did not show.
#
# EXIT CODES
#   0  every candidate was decided and nothing failed to delete
#   2  a threshold is not declared, or the liveness primitive is missing
#   3  something was UNDECIDABLE — a running process that no session file
#      names, a wall that tripped. Undecidable is a failure, never a footnote
#      beside a success-shaped count, which is the rule the worktree reaper
#      learned the same way.
#   4  A DELETION FAILED. Added 2026-09-18: it used to be `3 if undecidable
#      else 0`, with the failure list never consulted, so a run in which every
#      deletion failed printed `applied: deleted=0 freed=0 B` — the same shape
#      as a run with nothing to do — and handed launchd a green exit. The word
#      FAILED existed only inside a log file nobody reads. 4 outranks 3 because
#      a failed deletion is the branch of §54 where Rich deletes it by hand.
#
# ===========================================================================
# EVERY THRESHOLD IS DECLARED. NONE IS A DEFAULT.
# ===========================================================================
# Cadence, age floor, retention and notice threshold are SCRATCH_* in
# orchestration.config, each with the measurement that chose it. This script
# reads the ENGINE's own orchestration.config — it runs from launchd with no
# repository and no session, so there is no entity root to resolve, and a job
# whose configuration depended on where it was started from would behave
# differently at 04:40 than it did when somebody tested it by hand.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB="$SCRIPT_DIR/lib/scratch-reaper.py"
CONFIG="${SCRATCH_REAPER_CONFIG:-$ENGINE_ROOT/orchestration.config}"

LABEL="com.richos.scratch-reaper"
LAUNCHD_DIR="${RICHOS_LAUNCH_AGENTS_DIR:-$HOME/Library/LaunchAgents}"
PLIST="$LAUNCHD_DIR/$LABEL.plist"

INSTALL=0
UNINSTALL=0
PASS_ARGS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --install)    INSTALL=1 ;;
        --uninstall)  UNINSTALL=1 ;;
        --dry-run)    : ;;   # the default; accepted so it can be said out loud
        --help|-h)    sed -n '2,70p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)            PASS_ARGS+=("$1") ;;
    esac
    shift
done

# ---------------------------------------------------------------------------
# the declarations
# ---------------------------------------------------------------------------
if [ ! -f "$CONFIG" ]; then
    echo "scratch-reaper: no orchestration.config at $CONFIG — every threshold this" >&2
    echo "  job uses is declared there with the reason beside it, and a reaper that" >&2
    echo "  invented its own would be deleting to numbers nobody chose." >&2
    exit 2
fi
# shellcheck disable=SC1090
. "$CONFIG" >/dev/null 2>&1 || true

if [ "${SCRATCH_REAPER_ENABLE:-1}" = "0" ]; then
    echo "scratch-reaper: STOOD DOWN by SCRATCH_REAPER_ENABLE=0 in $CONFIG."
    echo "verdict: stood-down deletable=0 reclaimable=0 B kept=0 undecidable=0"
    exit 0
fi

# EVERY SCRATCH_* AND APP_* KEY THE CONFIG DECLARES IS EXPORTED, BY PATTERN AND
# NEVER BY NAME. This used to be seven `export` lines listing each key, and that
# shape has now failed twice: once when SCRATCH_DOCKER_UNTIL was declared and not
# exported (the symptom was a --status that could not see its own second volume),
# and once here — the deny-by-default keys were declared in orchestration.config,
# read by config_from_env, and arrived EMPTY, so the whole arm was silently off
# and the pass finished in 1.3 s looking exactly like a clean machine.
#
# A DECLARED-AND-UNREACHABLE KEY IS THE WORST FAILURE THIS FILE HAS, because both
# halves look right: the config says the coverage is on, the code says it reads
# the key, and nothing anywhere says the two never met. The list of names was the
# defect; deriving the list from the config is the fix. `compgen -v` is bash's own
# enumeration of what sourcing actually set, so a key cannot be declared and
# missed, and nothing has to be remembered by anybody.
while read -r _k; do
    case "$_k" in
        SCRATCH_*|APP_TEST_INSTANCE_*|APP_INSTANCE_*) export "$_k" ;;
    esac
done < <(compgen -v)
unset _k

# ---------------------------------------------------------------------------
# INSTALL / UNINSTALL — the schedule, generated, never hand-written
# ---------------------------------------------------------------------------
# The plist bakes in this script's absolute path, so it is generated from the
# path this script is actually at. A hand-written plist pointing at a moved
# engine is a job that fires perfectly and executes nothing.
if [ "$UNINSTALL" = "1" ]; then
    if command -v launchctl >/dev/null 2>&1 && [ -z "${RICHOS_LAUNCH_AGENTS_DIR:-}" ]; then
        launchctl bootout "gui/$(id -u)/$LABEL" >/dev/null 2>&1 || true
    fi
    # A change still waiting for a run to end is withdrawn with the job: its
    # waiter sees its pid file gone and ends without installing anything.
    # shellcheck source=lib/launchd-install.sh
    . "$SCRIPT_DIR/lib/launchd-install.sh"
    rm -f "$PLIST" "$(launchd_install_pending_file "$LABEL")" "$(launchd_install_gave_up_file "$LABEL")"
    echo "✓ unscheduled $LABEL"
    exit 0
fi

if [ "$INSTALL" = "1" ]; then
    # NEVER SCHEDULE FROM A LINKED WORKTREE OR A TEMPORARY DIRECTORY, for the
    # reason ci-surface-watch.sh gives in the same words: the plist bakes in
    # this script's absolute path, an agent worktree is deleted at land time,
    # and from that moment the job fires on time, every time, and executes
    # nothing. A reaper that stops running looks exactly like a machine with
    # no garbage on it.
    _WT_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel 2>/dev/null || true)"
    _WT_MARKER="${_WT_ROOT:-$ENGINE_ROOT/..}/.g""it"
    if [ -f "$_WT_MARKER" ] || case "$ENGINE_ROOT" in
            /tmp/*|/private/tmp/*|/private/var/folders/*|*/.claude/worktrees/*) true ;;
            *) false ;; esac; then
        {
            echo "REFUSING TO SCHEDULE FROM HERE."
            echo "  engine: $ENGINE_ROOT"
            echo "  This is a linked worktree or a temporary directory. The plist would"
            echo "  bake in a path that stops existing at land time, and the job would go"
            echo "  on firing four times a day against nothing."
            echo "  Install it from the MAIN checkout instead."
        } >&2
        exit 2
    fi
    HOURS="${SCRATCH_REAPER_HOURS:-}"
    MINUTE="${SCRATCH_REAPER_MINUTE:-}"
    if [ -z "$HOURS" ] || [ -z "$MINUTE" ]; then
        echo "scratch-reaper: SCRATCH_REAPER_HOURS / SCRATCH_REAPER_MINUTE are not declared in $CONFIG." >&2
        exit 2
    fi
    mkdir -p "$LAUNCHD_DIR" "$HOME/.claude/state"
    # Generated beside the plist, never over it: launchd_install decides whether
    # the job changed by comparing this with the plist the job was loaded from.
    NEW_PLIST="$PLIST.new.$$"
    {
        echo '<?xml version="1.0" encoding="UTF-8"?>'
        echo '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
        echo '<plist version="1.0">'
        echo '<dict>'
        echo "    <key>Label</key><string>$LABEL</string>"
        echo '    <key>ProgramArguments</key>'
        echo '    <array>'
        echo '        <string>/bin/bash</string>'
        echo "        <string>$SCRIPT_DIR/scratch-reaper.sh</string>"
        echo '        <string>--apply</string>'
        echo '    </array>'
        echo '    <key>StartCalendarInterval</key>'
        echo '    <array>'
        for h in $HOURS; do
            echo "        <dict><key>Hour</key><integer>$h</integer><key>Minute</key><integer>$MINUTE</integer></dict>"
        done
        echo '    </array>'
        # RunAtLoad is FALSE, unlike the CI watch. That job reads; this one
        # deletes, and a deleter that runs the instant it is installed deletes
        # before anybody has read its plan.
        echo '    <key>RunAtLoad</key><false/>'
        echo '    <key>ProcessType</key><string>Background</string>'
        echo "    <key>StandardOutPath</key><string>$HOME/.claude/state/scratch-reaper-launchd.log</string>"
        echo "    <key>StandardErrorPath</key><string>$HOME/.claude/state/scratch-reaper-launchd.log</string>"
        echo '    <key>EnvironmentVariables</key>'
        echo '    <dict>'
        echo '        <key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin</string>'
        echo "        <key>RICHOS_ENGINE_ROOT</key><string>$ENGINE_ROOT</string>"
        echo '    </dict>'
        echo '</dict>'
        echo '</plist>'
    } >"$NEW_PLIST"
    # AN UNCHANGED JOB IS LEFT ALONE, AND A RUNNING CLEANUP IS NEVER KILLED.
    # install.sh calls this on every engine install. It used to bootout and
    # bootstrap every time, and on 2026-10-25 the land at 04:42:24 booted the
    # job out two minutes after the 04:40 cleanup was due; that this killed the
    # 04:40 cleanup is inferred from the timing, not proven by any record.
    # A changed job found running is applied when the run ends.
    # launchd-install.sh carries the rule and the evidence.
    if command -v launchctl >/dev/null 2>&1 && [ -z "${RICHOS_LAUNCH_AGENTS_DIR:-}" ]; then
        # shellcheck source=lib/launchd-install.sh
        . "$SCRIPT_DIR/lib/launchd-install.sh"
        launchd_install "$LABEL" "$PLIST" "$NEW_PLIST"
        case "$LAUNCHD_INSTALL_RESULT" in
            unchanged)
                echo "✓ scheduled $LABEL (unchanged: already loaded from this plist, so it was not reloaded)"
                exit 0 ;;
            running)
                # The change waits for the cleanup and then applies by itself
                # (launchd-install.sh, launchd_install_after_run).
                _JOB_LOG="$HOME/.claude/state/scratch-reaper-launchd.log"
                if launchd_install_after_run "$LABEL" "$_JOB_LOG" \
                        "${LAUNCHD_APPLY_WAIT_MAX_SECONDS:-}" "${LAUNCHD_APPLY_WAIT_POLL_SECONDS:-}" \
                        /bin/bash "$SCRIPT_DIR/scratch-reaper.sh" --install; then
                    {
                        echo "scratch-reaper: CHANGE PENDING — a scheduled cleanup is running now (pid ${LAUNCHD_INSTALL_PID:-unknown}),"
                        echo "  so the changed job was not loaded yet and the cleanup was not interrupted."
                        echo "  It applies by itself when that cleanup ends (waiting up to $((LAUNCHD_APPLY_WAIT_MAX_SECONDS / 60)) min,"
                        echo "  waiter pid $LAUNCHD_INSTALL_CONTINUATION_PID, logged in $_JOB_LOG)."
                    } >&2
                    exit 3
                fi
                echo "scratch-reaper: the job changed while a scheduled cleanup is running (pid ${LAUNCHD_INSTALL_PID:-unknown}), and the wait to apply it afterwards could not start: $LAUNCHD_INSTALL_DETAIL. The job keeps its previous schedule; the disk watchdog's health check reports it." >&2
                exit 2 ;;
            failed)
                echo "scratch-reaper: $LAUNCHD_INSTALL_DETAIL — the job is NOT scheduled." >&2
                exit 2 ;;
        esac
    else
        mv -f "$NEW_PLIST" "$PLIST"
    fi
    echo "✓ scheduled $LABEL at $(for h in $HOURS; do printf '%02d:%s ' "$h" "$MINUTE"; done)-> $PLIST"
    echo "  It runs --apply. Read what it would do first:  $SCRIPT_DIR/scratch-reaper.sh --verbose"
    exit 0
fi

command -v python3 >/dev/null 2>&1 || {
    echo "scratch-reaper: python3 is not on PATH; the scan cannot run." >&2
    exit 2
}

# THE SYSTEM TOOL DIRECTORIES, for every program this job runs. lsof is in
# /usr/sbin, which the plist's PATH leaves out; scratch-reaper.py appends it for its
# own children, but workspaces.py's process listing (used for Codex worktrees)
# looks lsof up on this PATH and would answer "unknown" for every checkout.
case ":$PATH:" in
    *:/usr/sbin:*) ;;
    *) PATH="$PATH:/usr/sbin:/sbin"; export PATH ;;
esac

# THE INSTALLED JOB'S MARK, decided here and handed on (cleanup PRD R3/R4). launchd
# names the job in XPC_SERVICE_NAME. Measured 2026-10-23 under a throwaway launchd
# job: this shell sees the label, a Python it EXECs sees it too, but a Python started
# as a child reads "0". So the shell, which always sees launchd's value, decides,
# and the variable is set or cleared here only: a manual run never inherits it.
if [ "${XPC_SERVICE_NAME:-}" = "$LABEL" ]; then
    export SCRATCH_REAPER_SCHEDULED=1
else
    unset SCRATCH_REAPER_SCHEDULED
fi

# THE SCHEDULED JOB'S OWN OUTPUT LOG keeps one previous file, like the deletion
# log (cleanup PRD R4): launchd appends every run's plan to it, and on
# 2026-10-23 it held 35 MB. Renamed only when launchd started this run.
if [ "${SCRATCH_REAPER_SCHEDULED:-}" = "1" ]; then
    # The state directory every other state file resolves (CLAUDE_CONFIG_DIR, else
    # ~/.claude): the plist's path under launchd, and never a live file in a test.
    _LL="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/state/scratch-reaper-launchd.log"
    if [ "$(stat -f %z "$_LL" 2>/dev/null || echo 0)" -gt 16777216 ]; then
        mv -f "$_LL" "$_LL.1" 2>/dev/null || true
    fi
    unset _LL
fi
exec python3 "$LIB" "${PASS_ARGS[@]+"${PASS_ARGS[@]}"}"
