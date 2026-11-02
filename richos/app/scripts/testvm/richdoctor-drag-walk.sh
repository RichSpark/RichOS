#!/bin/bash
# richdoctor-drag-walk.sh — does RichDoctor's window move by its title strip, as in any Mac app?
#
#   run-walk.py --no-app --home <empty dir> --engine "" --report <json> -- \
#     richdoctor-drag-walk.sh <RichDoctor.app> <out-dir>
#
# CEO 2026-11-02: "why the hell can I not move the RichDoctor window to some different place by
# grabbing its title bar like I can with any other app on the Mac?" Cause: the strip
# (data-tauri-drag-region) calls Tauri's start_dragging, which core:default does not grant.
# The window is dragged with the REAL mouse (pointer-drag.sh) on its first screen and on the
# next one (a typed description and Continue need no Claude Code), and its position is read
# from System Events before and after. EXIT STATUS: 0 only when both drags moved it.
set -u
VM="$1"
APP="${2:?usage: richdoctor-drag-walk.sh <vm> <RichDoctor.app> <out-dir>}"
S="${3:?usage: richdoctor-drag-walk.sh <vm> <RichDoctor.app> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
G=/Users/admin/richdoctor-drag-walk
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
pass() { note "ok: $1"; }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
gs() { "$T/guest.sh" "$VM" "$@"; }

PAYLOAD=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/payload")
case "$PAYLOAD" in /Users/admin/testvm/*) ;; *) setup_failed "the guest payload ($PAYLOAD)" ;; esac
GHOME="$PAYLOAD/home"
AS="sudo launchctl asuser \$(id -u admin) sudo -u admin env HOME='$GHOME'"

has() {
  "$T/ax.sh" "$VM" find --value "$1" --contains --app RichDoctor --first >/dev/null 2>&1 \
    || "$T/ax.sh" "$VM" find --title "$1" --contains --app RichDoctor --first >/dev/null 2>&1
}
wait_text() { for _ in $(seq 1 "${2:-30}"); do has "$1" && return 0; sleep 3; done; return 1; }
winpos() { # "x y w h" of RichDoctor's window 1
  "$T/ax.sh" "$VM" 'tell application "System Events" to tell process "RichDoctor" to (get position of window 1) & (get size of window 1)' 2>/dev/null \
    | grep -E '^[-0-9{}, ]+$' | tr -d '{}' | tr ',' ' ' | tr -s ' ' | sed 's/^ //;s/ $//' | head -1
}
drag_strip() { # $1 label: grab the middle of the 40 px strip and move it 200 right, 120 down
  local label="$1" before after x y w h bx by ax ay
  before=$(winpos)
  read -r x y w h <<<"$before"
  if [ -z "${x:-}" ] || [ -z "${w:-}" ]; then fail "$label: could not read the window position ('$before')"; return; fi
  bx=$((x + w / 2)); by=$((y + 20))
  note "$label: window before: x=$x y=$y w=$w h=$h; grabbing the strip at $bx,$by"
  "$T/pointer-drag.sh" "$VM" "$bx,$by" "$((bx + 200)),$((by + 120))" 2>&1 | tee -a "$S/walk.log" | tail -3; [ "${PIPESTATUS[0]}" = 0 ] || fail "$label: the pointer drag failed"
  sleep 1
  after=$(winpos)
  note "$label: window after: $after"
  "$T/shot.sh" "$VM" "$S/$label.png" || fail "capture $label"
  read -r ax ay _ <<<"$after"
  if [ "${ax:-$x}" != "$x" ] || [ "${ay:-$y}" != "$y" ]; then pass "$label: the window moved ($x,$y -> $ax,$ay)"
  else fail "$label: the window did not move ($x,$y)"; fi
}

gs "mkdir -p $G '$GHOME/Applications' '$GHOME/Library'" || setup_failed "folders"
ditto -c -k --keepParent "$APP" "$S/RichDoctor.zip" || setup_failed "zipping RichDoctor"
gs --push "$S/RichDoctor.zip" "$G/RichDoctor.zip" || setup_failed "pushing RichDoctor"
gs "rm -rf '$GHOME/Applications/RichDoctor.app' && ditto -x -k $G/RichDoctor.zip '$GHOME/Applications/' && chown -R admin '$GHOME/Applications/RichDoctor.app' '$GHOME/Library'" || setup_failed "installing RichDoctor"
gs "$AS nohup '$GHOME/Applications/RichDoctor.app/Contents/MacOS/RichDoctor' > $G/app.log 2>&1 & echo \$! > $G/app.pid" || setup_failed "launching RichDoctor"
sleep 3
# pointer-drag.sh drags only into the PID run.sh recorded; --no-app records none, so record the app's own (in the owned guest).
RDPID=$(gs "pgrep -x RichDoctor | head -1" | tr -d '\r' | grep -E '^[0-9]+$' | head -1)
[ -n "$RDPID" ] || setup_failed "RichDoctor is not running in the guest"
printf '%s\n' "$RDPID" > "${TESTVM_RUN:-${TESTVM_ROOT:-$HOME/.richos-testvm}/run}/$VM/app.pid"
note "RichDoctor launched in the guest, pid $RDPID"

if wait_text "What went wrong?" 20; then pass "the first screen is open"; else fail "the first screen did not open"; fi
drag_strip drag-1-first-screen

"$T/ax.sh" "$VM" click --role AXTextArea --app RichDoctor --first >/dev/null 2>&1 || true
"$T/ax.sh" "$VM" 'tell application "System Events" to keystroke "RichOS closes before its window opens."' >/dev/null 2>&1
sleep 1
"$T/ax.sh" "$VM" click --title "Continue" --app RichDoctor --first >/dev/null 2>&1 || fail "pressing Continue"
if wait_text "About when did it last happen?" 20; then pass "the time question (a later screen) is open"; else fail "the later screen did not open"; fi
drag_strip drag-2-later-screen

if [ "${#FAILS[@]}" -gt 0 ]; then note "done, with ${#FAILS[@]} failed step(s)"; exit 1; fi
note "done: every check passed"
