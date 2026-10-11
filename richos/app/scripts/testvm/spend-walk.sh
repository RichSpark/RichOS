#!/bin/bash
# spend-walk.sh — the Token spend screen (round 22.1) on the real app, in one guest run, dark and
# light: the row in quick settings, its own screen, the figure the app priced from real-format
# transcripts, the periods, Refresh, and the way out.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <home with .claude/projects> --engine <engine.tar.gz> \
#     --report <json> -- spend-walk.sh <out-dir> [<expected figure>]
#
# His words (§118): "I want to add this feature that will allow the user to see and brag about
# (via screenshots and screen recording videos) about their "API-equivalent token spend"" and
# "That's needs to be its own screen/page." The design is round 22.1 (richos-hq
# design/mockups/rounds/round-22.1/).
#
# FIXTURE-DRIVEN: the home's `.claude/projects/` holds transcript rows in Claude Code's own shape
# (richos-core `src/spend/fixture-requests.jsonl`, whose hand-computed total is $0.5234329), so the
# figure on screen is the app's own reading and pricing of them, compared with that hand total.
# No real Claude account is read. run-walk.py quits the app, stops the guest and deletes the clone
# when this returns (CEO §54). In <out-dir>:
#   0-menu-{dark,light}      Settings: the Token spend row, under Technical view, with this month's figure
#   1-meter-{dark,light}     the screen, This month: the figure, the two sentences, the legend
#   2-all-time-dark          All time
#   3-closed                 Back to the conversation (Esc)
# EXIT STATUS: 1 when the setup fails or any capture or check below fails (named at the end).
set -u
VM="$1"
S="${2:?usage: spend-walk.sh <vm> <out-dir> [<expected figure>]}"
EXPECT="${3:-\$0.52}"
T="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$S"
export TESTVM_AX_TIMEOUT="${TESTVM_AX_TIMEOUT:-40}"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
ax() { "$T/ax.sh" "$VM" "$@"; }
shot() { "$T/shot.sh" "$VM" "$S/$1.png" || fail "capture $1"; }
wait_for() {
  for _ in $(seq 1 30); do
    ax find --title "$1" --contains --first >/dev/null 2>&1 && return 0
    ax find --value "$1" --contains --first >/dev/null 2>&1 && return 0
    sleep 2
  done
  note "never appeared: $1"
  return 1
}
has() {  # has <words> <check name>: the words are on screen, read rather than assumed
  for _ in $(seq 1 10); do
    ax find --value "$1" --contains --first >/dev/null 2>&1 && { note "read: $1"; return 0; }
    ax find --title "$1" --contains --first >/dev/null 2>&1 && { note "read: $1"; return 0; }
    sleep 2
  done
  fail "$2 (not on screen: $1)"
}
press() {
  for _ in 1 2 3; do
    ax click --title "$1" --contains --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  fail "$2 (could not press: $1)"
  return 1
}
menu_open() { ax find --title 'Light theme' --first >/dev/null 2>&1; }
menu() {
  for _ in 1 2 3; do
    if [ "$1" = open ]; then menu_open && return 0; else menu_open || return 0; fi
    ax click --title 'Settings' --role AXPopUpButton >/dev/null 2>&1 || true
    sleep 3
  done
  if [ "$1" = closed ]; then ax --key 53 >/dev/null 2>&1 || true; sleep 2; menu_open || return 0; fi
  note "the menu would not be $1"
}
theme() {
  menu open
  for _ in 1 2 3; do
    ax click --title "$1 theme" >/dev/null 2>&1 && break
    sleep 2
  done
  sleep 1
  menu closed
}
screen_open() { ax find --title 'Refresh the numbers' --contains --first >/dev/null 2>&1; }

# A first run asks its questions first (the setup sheet, the memory folder, the company); each
# is answered the way handoff-walk.sh and accounts-walk.sh answer them.
wait_for 'Not now' || true
for _ in 1 2 3; do
  ax find --title 'Add this company' --first >/dev/null 2>&1 && break
  ax click --title 'Not now' --first >/dev/null 2>&1 || true
  sleep 4
done
wait_for 'Add this company' || true
for attempt in 1 2 3; do
  ax type 'Northwind Traders' --role AXTextField --first --replace || true
  sleep 1
  ax click --title 'Add this company' || true
  sleep 6
  ax find --title 'Add this company' --first >/dev/null 2>&1 || break
  note "company not added yet (attempt $attempt)"
done

for mode in Dark Light; do
  lower="$(echo "$mode" | tr '[:upper:]' '[:lower:]')"
  theme "$mode"
  note "0 $mode: Settings shows the Token spend row with this month's figure"
  menu open
  has 'Token spend' "0 $lower the Token spend row"
  has 'this month' "0 $lower the row's figure"
  shot "0-menu-$lower"
  note "1 $mode: the row opens its own screen"
  ax click --title 'Token spend' --role AXMenuItem --contains --first >/dev/null 2>&1 || press 'Token spend' "1 the row"
  sleep 3
  for _ in $(seq 1 30); do screen_open && break; sleep 2; done
  screen_open || fail "1 $lower the screen did not open"
  has 'API-equivalent token spend' "1 $lower the title"
  has "$EXPECT" "1 $lower the figure the app priced (by hand: \$0.5234329)"
  has 'What the same work would have cost' "1 $lower the first sentence"
  has 'Billed by the API: nothing.' "1 $lower the second sentence"
  has 'Not a bill.' "1 $lower the footer"
  shot "1-meter-$lower"
  if [ "$mode" = Dark ]; then
    press 'All time' "2 All time"
    sleep 3
    has 'since' "2 All time says since when"
    has "$EXPECT" "2 the all-time figure"
    shot 2-all-time-dark
    press 'This month' "2 back to This month"
  fi
  # Esc is Back: the screen closes and the conversation is there again.
  ax --key 53 >/dev/null 2>&1 || true
  sleep 2
  screen_open && { press 'Back' "3 Back"; sleep 2; }
  screen_open && fail "3 $lower the screen did not close"
done
shot 3-closed

ax tree --json > "$S/final-tree.json" 2>/dev/null || true
if [ "${#FAILS[@]}" -gt 0 ]; then
  note "FAILED CHECKS: ${#FAILS[@]}"
  for f in "${FAILS[@]}"; do note "  $f"; done
  exit 1
fi
note "every check read what it should"
exit 0
