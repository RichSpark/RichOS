#!/bin/bash
# fbcopy-walk.sh — the CEO's feedback of 2026-10-06, items 3 to 7, read off the real app in one
# guest run.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- fbcopy-walk.sh <out-dir>
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the
# clone when this returns (CEO §54). Fixture-driven like round16-panel-walk.sh: the fake
# `claude` (fake-claude-fill-first.pl) stands in, so no real account is read or signed in.
#
# It reads the accessibility tree (what the app really shows, as text) and photographs:
#   1-quick-settings   the universal Settings menu: NO "Splash screen" row (item 7)
#   2-general-settings the rail gear's popover: the "Splash screen" group, and "Keep the stored
#                      output" with Forever chosen on this fresh install (items 6 and 7)
#   3-phone            "Use Rich from your phone": "RichConnect for RichOS" where it is offered,
#                      no "RichOS Connect", no "private pilot" (items 4 and 5)
# and greps every tree for an m-dash or n-dash (item 3). Verdicts go to <out-dir>/verdict.txt,
# one line each, PASS, FAIL or UNKNOWN (a screen that could not be read); the walk exits 1 when any
# line is FAIL or UNKNOWN.
set -u
VM="$1"
S="${2:?usage: fbcopy-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$S"
: > "$S/verdict.txt"
# 60 s per accessibility call instead of ax.sh's 20: a whole-window tree read took 16 to 20 s in
# the guest on the nightly 41 walk (walk-54299f44c8c8) and two of them hit the 20 s deadline,
# which is the harness, not the app.
export TESTVM_AX_TIMEOUT="${TESTVM_AX_TIMEOUT:-60}"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
verdict() { echo "$1  $2" | tee -a "$S/verdict.txt"; }
ax() { "$T/ax.sh" "$VM" "$@"; }
wait_for() {
  for _ in $(seq 1 30); do
    ax find --title "$1" --first >/dev/null 2>&1 && return 0
    sleep 2
  done
  note "never appeared: $1"
  return 1
}
# A tree read that fails, or that returns the harness's own error object (the 2026-10-06 run got
# "TCC grant did not take" from every read), is kept apart as <name>-ax.err and is no evidence:
# every item that depends on it is UNKNOWN, never a PASS over an empty read and never a FAIL
# over the harness's own words.
tree() {
  if ax tree --depth 40 > "$S/$1-ax.tmp" 2>&1 && ! head -1 "$S/$1-ax.tmp" | grep -q '"error"'; then
    mv "$S/$1-ax.tmp" "$S/$1-ax.txt"
  else
    mv "$S/$1-ax.tmp" "$S/$1-ax.err"
    verdict UNKNOWN "capture $1: the accessibility tree could not be read (see $1-ax.err)"
  fi
}
read_ok() { [ -f "$S/$1-ax.txt" ]; }
shot() { "$T/shot.sh" "$VM" "$S/$1.png" || verdict FAIL "capture $1: no frame"; }
has() { grep -qF -- "$2" "$S/$1-ax.txt"; }

"$T/guest.sh" "$VM" 'mkdir -p /Users/admin/fill-first'
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" /Users/admin/fill-first/claude
"$T/guest.sh" "$VM" 'chmod 755 /Users/admin/fill-first/claude'
"$T/guest.sh" "$VM" "printf '{\"five\":10,\"weekly\":20,\"five_at\":0,\"week_at\":0}' > /Users/admin/fill-first/usage-1.json"
python3 - "$VM" "$T" <<'PY'
import sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment={'RICHOS_CLAUDE_BIN': '/Users/admin/fill-first/claude'}))
PY

wait_for 'Not now' || true
ax click --title 'Not now' --first || true
wait_for 'Add this company' || true
sleep 2
for attempt in 1 2 3; do
  ax type 'Northwind Traders' --role AXTextField --first --replace || true
  sleep 1
  ax click --title 'Add this company' || true
  sleep 6
  ax find --title 'Add this company' --first >/dev/null 2>&1 || break
  note "company not added yet (attempt $attempt)"
done
tree 0-conversation
shot 0-conversation

note "1 quick settings"
ax click --title 'Settings' --role AXPopUpButton || note "no Settings menu button"
sleep 3
tree 1-quick-settings
shot 1-quick-settings
if ! read_ok 1-quick-settings; then verdict UNKNOWN "item 7: the quick settings were not read"
elif has 1-quick-settings 'Text size'; then
  if has 1-quick-settings 'Splash screen'; then verdict FAIL "item 7: the quick settings menu still has a Splash screen row"
  else verdict PASS "item 7: the quick settings menu has no Splash screen row"; fi
else verdict FAIL "item 7: the quick settings menu did not open (no Text size row in the tree)"; fi

note "3 Use Rich from your phone"
# The row is an AXMenuItem; the first node merely containing these words has no AXPress
# (walk-54299f44c8c8: "noaction: element has no AXPress; actions=AXShowMenu").
ax click --title 'Use Rich from your phone' --role AXMenuItem --contains --first || note "no phone row"
sleep 4
tree 3-phone
shot 3-phone
if ! read_ok 3-phone; then verdict UNKNOWN "items 4 and 5: the phone popup was not read"
elif has 3-phone 'RichOS Connect'; then verdict FAIL "item 4: the phone popup still says RichOS Connect"
else verdict PASS "item 4: the phone popup never says RichOS Connect"; fi
if read_ok 3-phone && has 3-phone 'RichConnect for RichOS'; then verdict PASS "item 4: the phone popup offers RichConnect for RichOS"
else note "item 4: RichConnect for RichOS is not offered on this guest's first screen (route chooser hidden)"; fi
if ! read_ok 3-phone; then :
elif has 3-phone 'private pilot' || has 3-phone 'Pilot setup reference'; then verdict FAIL "item 5: the phone popup mentions a pilot"
else verdict PASS "item 5: no pilot wording in the phone popup"; fi
ax click --title 'Close' --first || ax --key 53 || true
sleep 2

note "2 general settings (the rail gear)"
# The rail gear is the AXPopUpButton titled with the gear glyph at the rail's foot; no AXButton
# is titled Settings (walk-54299f44c8c8 found none, and its frame shows the top bar's tooltip).
ax click --title '⚙' --role AXPopUpButton --first || note "no rail gear button"
sleep 3
tree 2-general-settings
shot 2-general-settings
if ! read_ok 2-general-settings; then verdict UNKNOWN "items 6 and 7: the general settings were not read"
elif has 2-general-settings 'Play its sound'; then verdict PASS "item 7: the general settings hold the splash screen's row (its sound switch since 2026-11-14)"
else verdict FAIL "item 7: the general settings do not hold the splash screen's row"; fi
if ! read_ok 2-general-settings; then :
elif has 2-general-settings 'Nothing is ever removed'; then verdict PASS "item 6: a fresh install keeps the stored output forever"
else verdict FAIL "item 6: the retention hint is not the forever one on a fresh install"; fi
ax --key 53 || true

note "item 3: dashes in any tree"
if ! ls "$S"/*-ax.txt >/dev/null 2>&1; then verdict UNKNOWN "item 3: no screen was read, so nothing was scanned"
elif grep -lE $'\xe2\x80\x93|\xe2\x80\x94' "$S"/*-ax.txt > "$S/dash-files.txt" 2>/dev/null; then
  grep -nE $'\xe2\x80\x93|\xe2\x80\x94' "$S"/*-ax.txt > "$S/dash-lines.txt" 2>/dev/null || true
  verdict FAIL "item 3: an m-dash or n-dash is on screen (see dash-lines.txt)"
else verdict PASS "item 3: no m-dash or n-dash in any screen this walk read"; fi

grep -qE '^(FAIL|UNKNOWN)' "$S/verdict.txt" && exit 1
exit 0
