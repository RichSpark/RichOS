#!/usr/bin/env bash
# guest-sound.sh — what sound a test guest has: the mode run.sh booted it with, whether its sound is
#                  passed through to this Mac, and whether its output is muted. Read-only, and only
#                  the guest is asked; the host's sound is never read or set (CEO §65).
#
#   testvm/guest-sound.sh <vm>
#
# Usable as a run-walk.py command (the VM name comes first):
#   run-walk.py --no-app --home H --engine none --report R -- guest-sound.sh
#   run-walk.py --audio --bundle Z --home H --engine none --report R -- guest-sound.sh
#
# THE TELL IS THE INPUT. tart attaches a sound device either way (tart 2.33.1 Sources/tart/VM.swift):
# with the host passthrough (run.sh --audio) it has an input stream from this Mac's microphone and an
# output stream to its speakers, and the guest lists input and output channels; without it (run.sh's
# default, tart --no-audio) it is "just a null speaker", an output stream with no host sink, and
# the guest lists output channels only. Measured 2026-11-15: walk-00e4c9b11afc (default) "Output
# Channels: 2" and no input; walk-058e73951bb2 (--audio) "Input Channels: 2", "Output Channels: 2".
#
# Prints, one line each:
#   booted: audio on|off              run.sh's record (<run state>/audio)
#   muted by run.sh: muted|no         run.sh's record (<run state>/output), written only with --audio
#   passthrough to this Mac: yes|no   input channels present (yes) or output only, the null speaker (no)
#   output muted: true|false          the guest's own reading, the same AppleScript run.sh mutes with
#   app running: <pids>|none          richos-tauri in the guest, so a reading can be placed beside a launch
# then the guest's device listing itself. A failed guest read exits nonzero.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh"

VM="${1:-}"
[ -n "$VM" ] || die "usage: guest-sound.sh <vm>"
STATE="$TESTVM_RUN/$VM"

echo "booted: audio $(cat "$STATE/audio" 2>/dev/null || echo unknown)"
echo "muted by run.sh: $(cat "$STATE/output" 2>/dev/null || echo no)"
DEVICES="$(guest_ssh "$VM" 'system_profiler SPAudioDataType')" || die "could not read sound devices in $VM"
MUTED="$(guest_ssh "$VM" "osascript -e 'output muted of (get volume settings)'")" || die "could not read output mute in $VM"
case "$MUTED" in
  true|false) ;;
  *) die "could not read output mute in $VM: expected true or false" ;;
esac
# pgrep uses exit 1 for no match. Convert just that guest result to success before SSH returns.
APP="$(guest_ssh "$VM" 'pgrep -x richos-tauri || [ "$?" -eq 1 ]')" || die "could not read app processes in $VM"
if [[ "$DEVICES" == *'Input Channels:'* ]]; then
  echo "passthrough to this Mac: yes (input and output channels)"
elif [[ "$DEVICES" == *'Output Channels:'* ]]; then
  echo "passthrough to this Mac: no (output only: tart's null speaker, which plays nothing)"
else
  echo "passthrough to this Mac: no (no sound device listed)"
fi
echo "output muted: $MUTED"
printf 'app running: %s\n' "$(printf '%s' "${APP:-none}" | tr '\n' ' ')"
echo "--- system_profiler SPAudioDataType in the guest ---"
printf '%s\n' "${DEVICES:-(nothing listed)}"
