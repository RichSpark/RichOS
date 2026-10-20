#!/usr/bin/env bash
# pointer-drag.sh — drag inside the app under test with the real mouse, the way a hand does.
#
#   testvm/pointer-drag.sh <vm> <x>,<y> <x>,<y> [<x>,<y> ...] [--step PX] [--pause S] [--rest S] [--right | --hover]
#
#   --hover presses no button at all: the pointer only moves through the points (one is enough) and
#   rests on the last, the way a hand hovers, so a tooltip shown by `mouseover` can be looked at.
#   Added with the nightly 68 walk: a drag holds the button, and WebKit sends no hover while it is held.
#
#   --right holds the RIGHT button instead. Given the same point twice it is a real right-click
#   (a `contextmenu` event in the app), the way a hand opens an Output panel row's menu (PRD
#   §6.3). Added with the candidate 38 walk: nothing in the toolkit could right-click.
#
#   The left button goes down at the first point, the pointer moves through every point in
#   steps of at most --step pixels (default 8) with --pause seconds between moves (default
#   0.02), rests --rest seconds at each point it reaches (default 0.15), and the button comes up
#   at the last point. Coordinates are the guest's screen coordinates (take them from
#   `ax.sh find --json`: x, y, w, h).
#
# ===========================================================================
# WHY THIS IS IN THE TOOLKIT
# ===========================================================================
# The Output panel's divider (output side-panel PRD §12.9) is felt, not pressed: "when I'm
# dragging up to here, then there's a stop … if I keep dragging … it snaps open completely".
# `ax.sh click` presses an element and `hand-file.sh drag` carries a Finder file; nothing in
# the toolkit held the button down and moved inside the app. A walk that needs this and writes
# it as a throwaway is the thing `scripts/qa/README.md` exists to stop.
#
# ===========================================================================
# THE SAME DISCIPLINE AS ax.sh AND hand-file.sh, BECAUSE IT SENDS THE SAME KIND OF INPUT
# ===========================================================================
# The drag goes only to the app under test, resolved by the PID run.sh recorded, and only once
# that PID is FRONTMOST. The guest gets its own deadline, as hand-file.sh's does, so an osascript
# whose host call timed out cannot finish its drag minutes later. Everything runs in the owned
# guest; nothing here touches the host's display or input (CEO §65).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "${TESTVM_AX_SUPERVISED:-}" != 1 ]; then
  exec env TESTVM_AX_SUPERVISED=1 python3 "$HERE/ax-deadline.py" --host "${TESTVM_AX_TIMEOUT:-30}" bash "$0" "$@" </dev/null
fi
. "$HERE/lib.sh"

usage="usage: pointer-drag.sh <vm> <x>,<y> <x>,<y> [<x>,<y> ...] [--step PX] [--pause S] [--rest S] [--right | --hover]"
VM="${1:-}"
[ -n "$VM" ] || die "$usage"
shift
POINTS=()
STEP=8; PAUSE=0.02; REST=0.15; RIGHT=0; HOVER=0
while [ $# -gt 0 ]; do
  case "$1" in
    --step) STEP="${2:-}"; shift 2 ;;
    --pause) PAUSE="${2:-}"; shift 2 ;;
    --rest) REST="${2:-}"; shift 2 ;;
    --right) RIGHT=1; shift ;;
    --hover) HOVER=1; shift ;;
    *,*) POINTS+=("$1"); shift ;;
    *) die "unknown argument: $1 — $usage" ;;
  esac
done
[ "$HOVER" = 0 ] || [ "$RIGHT" = 0 ] || die "--hover moves with no button, so it cannot also be --right — $usage"
if [ "$HOVER" = 1 ]; then
  [ "${#POINTS[@]}" -ge 1 ] || die "a hover needs at least one point — $usage"
else
  [ "${#POINTS[@]}" -ge 2 ] || die "a drag needs at least two points — $usage"
fi

ag() {  # one indirection to the guest, as ax.sh has, so the tests can use a stub guest
  if [ -n "${TESTVM_GUEST_EXEC:-}" ]; then "$TESTVM_GUEST_EXEC" "$VM" "$@"; else guest_ssh "$VM" "$@"; fi
}
if [ -z "${TESTVM_GUEST_EXEC:-}" ]; then
  preflight_tart
  require_vm_running "$VM"
fi
PID="$(cat "$TESTVM_RUN/$VM/app.pid" 2>/dev/null || true)"
[ -n "$PID" ] || die "no app pid recorded for $VM — was it started by run.sh?"

# The guest's own deadline, built exactly as hand-file.sh builds it.
REMOTE_OSA="$(python3 - "$HERE/ax-deadline.py" "${TESTVM_AX_TIMEOUT:-30}" <<'DEADLINE'
import os, pathlib, shlex, sys, time
try:
    seconds = float(sys.argv[2])
    assert 1 <= seconds <= 300
except (ValueError, AssertionError):
    raise SystemExit("TESTVM_AX_TIMEOUT must be between 1 and 300 seconds")
remaining = float(os.environ['TESTVM_AX_DEADLINE']) - time.monotonic() - 2
if remaining < 1: raise SystemExit("preflight consumed the deadline")
seconds = min(seconds, remaining)
print("python3 -c " + shlex.quote(pathlib.Path(sys.argv[1]).read_text()) +
      " " + shlex.quote(str(seconds)) + " osascript -l JavaScript -")
DEADLINE
)"

# Every number is parsed here, so the guest program only ever receives JSON.
PARAMS="$(PD_PID="$PID" PD_STEP="$STEP" PD_PAUSE="$PAUSE" PD_REST="$REST" PD_RIGHT="$RIGHT" PD_HOVER="$HOVER" python3 - "${POINTS[@]}" <<'PY'
import json, os, sys
try:
    points = [[float(v) for v in p.split(',', 1)] for p in sys.argv[1:]]
    step, pause, rest = float(os.environ['PD_STEP']), float(os.environ['PD_PAUSE']), float(os.environ['PD_REST'])
    assert step > 0 and 0 <= pause <= 1 and 0 <= rest <= 5
except (ValueError, AssertionError):
    raise SystemExit("points are x,y numbers; --step > 0, --pause 0..1 s, --rest 0..5 s")
print("var DRAG_PARAMS = " + json.dumps({"pid": int(os.environ['PD_PID']), "points": points,
                                         "step": step, "pause": pause, "rest": rest,
                                         "right": os.environ['PD_RIGHT'] == '1',
                                         "hover": os.environ['PD_HOVER'] == '1'}) + ";")
PY
)"

# The program travels on stdin, never in a remote command line.
{ printf '%s\n' "$PARAMS"; cat "$HERE/pointer-drag.js"; } | ag "$REMOTE_OSA" 2>&1 | {
  answer="$(cat)"
  printf '%s\n' "$answer"
  case "$answer" in *'"error"'*) exit 3 ;; *'"dragged"'*|*'"hovered"'*) exit 0 ;; *) exit 4 ;; esac
}
