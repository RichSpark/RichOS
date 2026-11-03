#!/bin/bash
# use-first-walk.sh — feedback item 8 on the real app, in one guest run: the CEO picks which
# Claude account is in use now ("Use this one now"), the pick survives a relaunch, and the
# automatic switch at 99% of the week still happens from the account he picked.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- use-first-walk.sh <out-dir>
#
# His words (richos-hq docs/ceo-input/2026-10-06_01/feedback.md, item 8): "I should be able to
# change/switch which account drains first. Because for testing now I should be able to switch
# to the account that currently has 97% weekly to see if it will auto-switch correctly after
# reaching 99% weekly." And (2026-10-06_02): the quick-settings quota row shows the WEEKLY window.
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the clone
# when this returns (CEO §54). FIXTURE-DRIVEN, exactly as fill-first-walk.sh and
# round16-panel-walk.sh: fake-claude-fill-first.pl answers every `claude` call from per-account
# usage files, so no real Claude account is read, signed in or switched. In <out-dir>:
#   0-quick-settings-one     the Settings menu, one account: the row reads "N% weekly used"
#   1-two-accounts           Account 1 (30% weekly) in use, Work (97% weekly) next
#   2-confirmation           Round 18's line, photographed the moment Use this one now is pressed
#   2-work-in-use-{dark,light}  after Use this one now on Work; claude-accounts.json inUse "2"
#   3-turn-on-work           a turn after the pick; calls.log shows it ran under Work's folder
#   4-after-relaunch         the app relaunched: Work still in use
#   5-switched-at-99         Work's week at 99%: Account 1 in use again, Rich's one line
#   6-quick-settings-two     the Settings menu, two accounts: "Account 1 N% weekly · next ..."
# EXIT STATUS: 1 when the setup fails or any capture or check below fails (named at the end);
# 0 only when every capture was saved and every check read what it should.
set -u
VM="$1"
S="${2:?usage: use-first-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
ax() { "$T/ax.sh" "$VM" "$@"; }
shot() { "$T/shot.sh" "$VM" "$S/$1.png" || fail "capture $1"; }
wait_for() {
  for _ in $(seq 1 30); do
    ax find --title "$1" --first >/dev/null 2>&1 && return 0
    sleep 2
  done
  note "never appeared: $1"
  return 1
}
# The line above the lanes ("In use now: <b>Work</b>. Next: ...") is several accessibility
# nodes, so it is never matched as one string (the 2026-10-06 run: on screen in every shot,
# never found by text). Which account is in use is read from the record and from the lanes'
# "Use <name> now" buttons, which only an account NOT in use carries; the shots show the line.
has() {  # has <words> <check name>: the words are on screen, read rather than assumed
  for _ in $(seq 1 10); do
    ax find --value "$1" --contains --first >/dev/null 2>&1 && { note "read: $1"; return 0; }
    ax find --title "$1" --contains --first >/dev/null 2>&1 && { note "read: $1"; return 0; }
    sleep 2
  done
  fail "$2 (not on screen: $1)"
}
menu_open() { ax find --title 'Light theme' --first >/dev/null 2>&1; }
menu() {  # menu open|closed
  if [ "$1" = open ]; then menu_open && return 0; else menu_open || return 0; fi
  ax click --title 'Settings' --role AXPopUpButton >/dev/null 2>&1 || note "no Settings button"
  sleep 2
}
theme() { # A failed press must end the walk before any capture is labeled with this theme.
  local selected=0
  menu open
  for _ in 1 2 3; do
    if ax click --title "$1 theme" >/dev/null 2>&1; then selected=1; break; fi
    sleep 2
    menu open
  done
  if [ "$selected" -eq 0 ]; then note "FAILED: could not select $1 theme"; exit 1; fi
  sleep 1
  menu closed
}
sheet_open() { ax find --title 'Close Claude Code quota' --first >/dev/null 2>&1; }
open_panel() {  # the quota row by its coordinate, as round16-panel-walk.sh opens it, then checked
  for try in 1 2; do
    menu open
    ax click --at 1330,327 || true
    sleep 4
    sheet_open && return 0
    note "quota sheet not open after try $try"
  done
}
close_panel() {
  sheet_open || return 0
  ax click --title 'Close Claude Code quota' >/dev/null 2>&1 || true
  sleep 2
  sheet_open && { ax --key 53 || true; sleep 2; }
  return 0
}
relaunch_app() {
  python3 - "$VM" "$T" <<'PY'
import sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment={'RICHOS_CLAUDE_BIN': '/Users/admin/fill-first/claude'}))
PY
}
# usage <file> <five %> <weekly %>
usage() { "$T/guest.sh" "$VM" "printf '{\"five\":$2,\"weekly\":$3}' > \"$1\""; }
send_turn() {  # one plain turn; the turn has ended once Send is back
  ax type "$1" --role AXTextArea --first --replace || note "no composer"
  ax click --title 'Send' --first || note "no Send"
  sleep 4
  for _ in $(seq 1 30); do
    ax find --title 'Send' --first >/dev/null 2>&1 && break
    sleep 2
  done
  sleep 3
}

"$T/guest.sh" "$VM" 'mkdir -p /Users/admin/fill-first' || setup_failed "the fixture folder"
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" /Users/admin/fill-first/claude || setup_failed "pushing the fake claude"
"$T/guest.sh" "$VM" 'chmod 755 /Users/admin/fill-first/claude' || setup_failed "the fake claude's mode"
usage /Users/admin/fill-first/usage-1.json 10 30 || setup_failed "Account 1's usage fixture"
relaunch_app || setup_failed "relaunching the app with the fake claude"

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

note "technical view on"
ax click --title 'Settings' --role AXPopUpButton || true
sleep 2
ax click --at 1492,284 || true
sleep 2
ax click --title 'Turn it on' || true
sleep 4

note "0 quick settings, one account: the weekly window, labeled"
menu open
has '30% weekly used' "0 quick settings row reads the week"
shot 0-quick-settings-one

note "1 add Work; Work's week at 97%"
open_panel
ax click --title '+ Add account' || true
sleep 2
if ! ax type 'Account 1' --title 'The account signed in now' --contains --replace; then
  ax 'tell application "System Events" to keystroke "Account 1"' || true
  ax 'tell application "System Events" to key code 48' || true
  ax 'tell application "System Events" to keystroke "Work"' || note "no new-account field"
else
  ax type 'Work' --title 'The new account' --contains --replace || note "no new-account field"
fi
sleep 1
ax click --title 'Add and sign in' || true
sleep 8
# shellcheck disable=SC2016  # the $(...) is meant to expand in the guest's shell, not here
DATA=$("$T/guest.sh" "$VM" 'dirname "$(find /Users/admin/testvm -type d -name claude-accounts 2>/dev/null | head -1)"')
note "app data: $DATA"
WORK="$DATA/claude-accounts/2/usage.json"
usage "$WORK" 10 97 || fail "Work's 97% usage fixture"
ax click --title 'Refresh' --first || true
sleep 8
has 'Use Work now' "1 Work, with room and not in use, offers Use this one now"
shot 1-two-accounts

note "2 Use this one now on Work"
ax click --title 'Use Work now' --first || fail "2 no Use this one now on Work"
sleep 1
# The confirmation is said in passing (3.6 s), so it is photographed at once, not searched for.
shot 2-confirmation
has 'Use Account 1 now' "2 Account 1 is no longer in use"
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/2-accounts-after-pick.txt" 2>&1 || fail "2 the accounts record"
grep -q '"inUse": *"2"' "$S/2-accounts-after-pick.txt" || fail "2 claude-accounts.json does not record Work in use"
close_panel; theme Dark; open_panel; shot 2-work-in-use-dark
close_panel; theme Light; open_panel; shot 2-work-in-use-light
close_panel; theme Dark

note "3 a turn after the pick runs under Work's folder"
: > "$S/.empty"; "$T/guest.sh" "$VM" --push "$S/.empty" /Users/admin/fill-first/calls.log || true; rm -f "$S/.empty"
send_turn 'Which account answers this?'
shot 3-turn-on-work
"$T/guest.sh" "$VM" 'cat /Users/admin/fill-first/calls.log' > "$S/3-calls.log" 2>&1 || fail "3 calls.log"
grep -q 'claude-accounts/2.*--session-id' "$S/3-calls.log" || fail "3 no lease started under Work's folder after the pick"

note "4 relaunch: the pick survives"
relaunch_app > "$S/4-relaunch.txt" 2>&1 || fail "4 relaunching the app"
sleep 10
open_panel
has 'Use Account 1 now' "4 Account 1 is still not in use after the relaunch"
shot 4-after-relaunch
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/4-accounts-after-relaunch.txt" 2>&1 || fail "4 the accounts record"
grep -q '"inUse": *"2"' "$S/4-accounts-after-relaunch.txt" || fail "4 claude-accounts.json does not record Work in use after the relaunch"

note "5 Work's week reaches 99%: the automatic switch moves on"
usage "$WORK" 10 99 || fail "Work's 99% usage fixture"
ax click --title 'Refresh' --first || true
sleep 8
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/5-accounts-after-99.txt" 2>&1 || fail "5 the accounts record"
grep -q '"inUse": *"1"' "$S/5-accounts-after-99.txt" || fail "5 claude-accounts.json does not record Account 1 in use"
shot 5-switched-at-99-sheet
close_panel
send_turn 'And this one?'
has "Switched to Account 1" "5 Rich's one line in the conversation"
shot 5-switched-at-99-line
"$T/guest.sh" "$VM" 'cat /Users/admin/fill-first/calls.log' > "$S/5-calls.log" 2>&1 || fail "5 calls.log"

note "6 quick settings, two accounts"
menu open
has '30% weekly' "6 quick settings row reads Account 1's week"
shot 6-quick-settings-two
menu closed

if [ "${#FAILS[@]}" -gt 0 ]; then
  note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
  exit 1
fi
note "done"
exit 0
