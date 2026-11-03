#!/usr/bin/env bash
# Runs `token-track.py sample` at every clock minute for both signed-in accounts, as a per-user launchd
# agent (no session needed; starts again at login/reboot).
#   token-track-launchd.sh install   |   token-track-launchd.sh remove   |   token-track-launchd.sh plist
# `plist` prints the plist `install` would write, and loads nothing.
# The agent runs the token-track.py next to this script; reinstall after the script moves.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
label=com.richos.token-track
plist="$HOME/Library/LaunchAgents/$label.plist"
work="${TESTVM_WORK_HOST_FOLDER:-$HOME/Library/Application Support/com.richos.app/claude-accounts/2}"
# Writes the plist to $1 ("-" is stdout). StartCalendarInterval with one {Minute: m} per minute 0-59
# fires at the start of every clock minute (man launchd.plist: missing keys are wildcards, an array
# schedules several intervals). StartInterval 60 drifted: a firing during a run is missed, so runs
# came 64 to 86 s apart and some clock minutes had no reading.
render() {
  python3 - "$1" "$label" "$here/token-track.py" "$HOME/.claude" "$work" "$HOME/.claude/state/token-track/launchd.log" <<'PY'
import getpass, plistlib, sys
out, label, script, first, work, log = sys.argv[1:]
d = {"Label": label,
     "ProgramArguments": ["/usr/bin/python3", script, "sample", "--account", "default=" + first, "--account", "work=" + work],
     "EnvironmentVariables": {"PATH": "/Users/%s/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" % getpass.getuser()},
     "StartCalendarInterval": [{"Minute": m} for m in range(60)],
     "RunAtLoad": True, "StandardOutPath": log, "StandardErrorPath": log}
if out == "-":
    plistlib.dump(d, sys.stdout.buffer)
else:
    with open(out, "wb") as f:
        plistlib.dump(d, f)
PY
}
case "${1:-}" in
install)
  mkdir -p "$HOME/.claude/state/token-track" "$(dirname "$plist")"
  render "$plist"
  launchctl unload "$plist" 2>/dev/null || true
  launchctl load "$plist"
  echo "installed $label (every clock minute, log ~/.claude/state/token-track/launchd.log)" ;;
plist)
  render - ;;
remove)
  launchctl unload "$plist" 2>/dev/null || true
  rm -f "$plist"
  echo "removed $label" ;;
*) echo "usage: $0 install|remove|plist" >&2; exit 2 ;;
esac
