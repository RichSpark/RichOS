#!/bin/bash
# round16-panel-walk.sh — the Claude Code quota panel against round 16, on the real app, in one
# guest run, dark and light.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- round16-panel-walk.sh <out-dir>
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the
# clone when this returns (CEO §54). FIXTURE-DRIVEN, exactly as fill-first-walk.sh:
# fake-claude-fill-first.pl answers every `claude` call from per-account usage files, so no real
# Claude account is read, signed in or switched. It captures, in <out-dir>, each in dark AND
# light, named for the round-16 state it matches (richos-hq design/mockups/rounds/round-16/):
#   0-no-reading        ~ unavailable, pause on: Claude Code answers get_usage with
#                         rate_limits null (walk of nightly 36, D1): "No reading yet.", and
#                         "+ Add account" still offered beside "Ask Claude Code"; the card
#                         says nothing is held on the missing number
#   1-one-account       ~ fresh / low: one account, "+ Add account" beside Refresh
#   2-two-accounts      ~ two / choice-pause: Home and Work as lanes, the one sentence, Pause
#   3-switched-sheet    ~ switched: Switch chosen, Home past its line, Work in use
#   4-switched-line     ~ switched-line: Rich's one line in the conversation, with a turn in
#                         progress and three agents working on Work (the working row)
#   5-fast-sheet        ~ fast-switch: the moved line with its ghost, the speed on the card
#   6-fast-alert        ~ fast-alert: Rich's alert in the conversation, then a turn with the
#                         working row
#   7-back-to-normal    ~ normal-again: Rich's line when the speed comes back down, then a
#                         turn with the working row
# The working row ("3 agents working on Work") exists only while a turn of this conversation
# runs (main.rs get_worker_status reads the conversation's lease during its turn), so each
# conversation state that round 16 draws with the row holds one turn open for its shots.
set -u
VM="$1"
S="${2:?usage: round16-panel-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
ax() { "$T/ax.sh" "$VM" "$@"; }
wait_for() {
  for _ in $(seq 1 30); do
    ax find --title "$1" --first >/dev/null 2>&1 && return 0
    sleep 2
  done
  note "never appeared: $1"
  return 1
}
# The menu's state is read, never assumed: its theme control exists only while it is open, and
# Settings toggles it (runs 1-3 on 2026-10-04 lost the quota sheet to a toggle out of step).
menu_open() { ax find --title 'Light theme' --first >/dev/null 2>&1; }
menu() {  # menu open|closed
  if [ "$1" = open ]; then menu_open && return 0; else menu_open || return 0; fi
  ax click --title 'Settings' --role AXPopUpButton >/dev/null 2>&1 || note "no Settings button"
  sleep 2
}
theme() {  # the Settings menu's own theme control, then the menu closed again
  menu open
  if ! ax click --title "$1 theme" >/dev/null 2>&1; then
    # 2026-10-04 run 5 lost the menu here once on a loaded host: read it again, once.
    sleep 2; menu open
    ax click --title "$1 theme" >/dev/null 2>&1 || note "no $1 theme control"
  fi
  sleep 1
  menu closed
}
sheet_open() { ax find --title 'Close Claude Code quota' --first >/dev/null 2>&1; }
open_panel() {  # the quota row by its coordinate, as fill-first-walk.sh opens it, then checked
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
pair() {  # a conversation state, dark then light, then back to dark
  theme Dark; "$T/shot.sh" "$VM" "$S/$1-dark.png"
  theme Light; "$T/shot.sh" "$VM" "$S/$1-light.png"
  theme Dark
}
pair_sheet() {  # a sheet state: the theme is changed with the sheet closed, then it is reopened
  close_panel; theme Dark; open_panel; "$T/shot.sh" "$VM" "$S/$1-dark.png"
  close_panel; theme Light; open_panel; "$T/shot.sh" "$VM" "$S/$1-light.png"
  close_panel; theme Dark; open_panel
}
# usage <file> <five %> <weekly %> <five resets at> <week resets at> (epoch seconds)
usage() { "$T/guest.sh" "$VM" "printf '{\"five\":$2,\"weekly\":$3,\"five_at\":$4,\"week_at\":$5}' > \"$1\""; }
# Round 16's own clock: Home's five-hour window resets in 3 h 22 min (so at 41% the gold bar
# runs past the "now" tick, as its `two` state draws it) and its week in 4 d 2 h 13 min;
# Work's in 3 h and 1 d 5 h 40 min. Fixed at the start; the walk's half hour shortens them.
START=$(date +%s)
HOME_FIVE=$((START + 3*3600 + 22*60)); HOME_WEEK=$((START + 4*86400 + 2*3600 + 13*60))
WORK_FIVE=$((START + 3*3600)); WORK_WEEK=$((START + 86400 + 5*3600 + 40*60))

"$T/guest.sh" "$VM" 'mkdir -p /Users/admin/fill-first'
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" /Users/admin/fill-first/claude
"$T/guest.sh" "$VM" 'chmod 755 /Users/admin/fill-first/claude'
# State 0 first: Account 1's get_usage answers rate_limits null from launch (the fake's
# {"null": true}), as Claude Code 2.1.289 did in runs B and C1 of the nightly-36 walk.
"$T/guest.sh" "$VM" "printf '{\"null\":true}' > /Users/admin/fill-first/usage-1.json"
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

note "technical view on"
ax click --title 'Settings' --role AXPopUpButton || true
sleep 2
ax click --at 1492,284 || true
sleep 2
ax click --title 'Turn it on' || true
sleep 2

note "0 no reading: get_usage answers rate_limits null; the pause on"
open_panel
ax click --title 'Automatically pause' --contains --first || note "no pause switch"
sleep 3
# Read, never assumed: the entry to a second account and the way to ask again are on the sheet.
if ax find --title '+ Add account' --first >/dev/null 2>&1; then note "+ Add account offered with no reading"; else note "+ Add account MISSING with no reading"; fi
if ax find --title 'Ask Claude Code' --first >/dev/null 2>&1; then note "Ask Claude Code offered"; else note "Ask Claude Code MISSING"; fi
if ax find --value 'nothing is held' --contains --first >/dev/null 2>&1; then note "the card says nothing is held"; else note "the no-reading card MISSING"; fi
ax tree --depth 40 > "$S/0-no-reading-ax.txt" 2>&1 || true
pair_sheet 0-no-reading
ax click --title 'Automatically pause' --contains --first || note "no pause switch to turn off"
sleep 3
# The figures arrive: Claude Code answers again, and Ask Claude Code reads them now.
usage /Users/admin/fill-first/usage-1.json 41 28 "$HOME_FIVE" "$HOME_WEEK"
ax click --title 'Ask Claude Code' --first || note "no Ask Claude Code to click"
sleep 8
close_panel

note "1 one account"
open_panel
ax tree --depth 40 > "$S/1-one-account-ax.txt" 2>&1 || true
pair_sheet 1-one-account

note "2 Add account: name both Home and Work"
ax click --title '+ Add account' || true
sleep 2
if ! ax type 'Home' --title 'The account signed in now' --contains --replace; then
  # Add account focuses its first field itself (quota.js openAdd); type there, Tab to the next.
  note "current-account field not matched by its label; typing into the focused field"
  ax 'tell application "System Events" to keystroke "Home"' || true
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
usage "$WORK" 10 20 "$WORK_FIVE" "$WORK_WEEK"
ax click --title 'Refresh' --first || true
sleep 6
note "the automatic switch on (Pause, the default)"
ax click --title 'Automatic pause or switch at the line' || note "no switch"
sleep 3
ax tree --depth 40 > "$S/2-two-accounts-ax.txt" 2>&1 || true
pair_sheet 2-two-accounts

note "3 Switch chosen; Home's five-hour window at 95%"
ax click --title 'switch to Work' --contains --first || note "no switch verb"
sleep 2
usage /Users/admin/fill-first/usage-1.json 95 28 "$HOME_FIVE" "$HOME_WEEK"
sleep 6
ax click --title 'Refresh' --first || true
sleep 8
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/3-accounts-after-switch.txt" 2>&1
pair_sheet 3-switched-sheet

# A turn of Rich's in this conversation, held open so the working row can be photographed.
# fake-claude-fill-first.pl holds only the user's own turn, never the app's internal handoff
# or re-prime (the app installs a new lease only after its priming answers, so holding that
# kept the 2026-10-05 run's shot 4 on the old lease); it keeps each lease's evidence journal
# as Claude Code's SessionStart hook does; on the lease's first user turn it journals one
# background agent per line of the agents file; and it answers with reply.txt.
put() {  # put <guest path> <text>: a file the fake reads, pushed rather than quoted
  printf '%s\n' "$2" > "$S/.put"
  "$T/guest.sh" "$VM" --push "$S/.put" "$1" || note "could not write $1"
  rm -f "$S/.put"
}
hold_turn() {  # hold_turn <message> <reply>: send, then wait for the working row itself
  put /Users/admin/fill-first/reply.txt "$2"
  "$T/guest.sh" "$VM" 'touch /Users/admin/fill-first/slow'
  ax type "$1" --role AXTextArea --first --replace || note "no composer"
  ax click --title 'Send' --first || note "no Send"
  for _ in $(seq 1 45); do
    ax find --value 'agents working on' --contains --first >/dev/null 2>&1 && return 0
    sleep 2
  done
  note "the working row never appeared"
  ax tree --depth 40 > "$S/$3-no-row-ax.txt" 2>&1 || true
}
release_turn() {  # the fake answers; the turn has ended once Send is back
  "$T/guest.sh" "$VM" 'rm -f /Users/admin/fill-first/slow'
  for _ in $(seq 1 30); do
    ax find --title 'Send' --first >/dev/null 2>&1 && break
    sleep 2
  done
  sleep 3
}
evidence() {  # every lease's journal as the app reads it, for the record of this run
  "$T/guest.sh" "$VM" "for d in \"$DATA/engine-state/evidence\"/*/; do echo \"== \$d\"; cat \"\$d/callbacks.jsonl\"; done" > "$S/$1-evidence.txt" 2>&1 || true
}

note "4 the panel closed: Rich's switch line, then a turn on Work with three agents working"
close_panel
sleep 2
put /Users/admin/fill-first/agents "$(printf 'Mark\nAndy\nTom')"
hold_turn 'Keep going on the outbox and the pairing screens.' \
  "On it. I have three people on it: Mark on the outbox retry, Andy on the pairing screens, and Tom timing the iOS handshake. I'll tell you the moment any of them needs a decision from you." 4
evidence 4
pair 4-switched-line
release_turn

note "5 fast: Home back under its line (next again); Work rising from its own 10% and 20%, 9 and 3 points a minute"
usage /Users/admin/fill-first/usage-1.json 41 28 "$HOME_FIVE" "$HOME_WEEK"
# Fast mode checks every MINUTE (quota.rs FAST_REFRESH_INTERVAL_MS), and one flat reading
# afterwards is a return to normal, so the rise has to continue through the shots (2026-10-04
# run 4: a single 20 -> 45 step was back to normal before its screenshot). A loop in the guest
# raises Work's figures every 20 s until a stop file appears.
cat > "$S/rise.sh" <<'RISE'
#!/bin/sh
# rise.sh <usage.json> <five> <weekly> <five step> <weekly step> <stop file> <five at> <week at>
f=$2; w=$3
while [ ! -e "$6" ]; do
  printf '{"five":%s,"weekly":%s,"five_at":%s,"week_at":%s}' "$f" "$w" "$7" "$8" > "$1"
  f=$((f + $4)); w=$((w + $5)); sleep 20
done
RISE
"$T/guest.sh" "$VM" --push "$S/rise.sh" /Users/admin/fill-first/rise.sh
rm -f "$S/rise.sh"
# From Work's own 10% and 20% (the 2026-10-05 run started the five-hour at 5%: that drop left
# only the weekly speed measured on the first rising pair, so the burst's one alert named the
# weekly window). 9 points a minute moves the five-hour line to 91% (100 - 9 x the 1-minute
# check); the rise stops after shot 5, about 4 minutes in, near 46%, far under that line.
"$T/guest.sh" "$VM" "rm -f /Users/admin/fill-first/stop-rise; nohup sh /Users/admin/fill-first/rise.sh \"$WORK\" 10 20 3 1 /Users/admin/fill-first/stop-rise $WORK_FIVE $WORK_WEEK </dev/null >/dev/null 2>&1 &"
open_panel
ax click --title 'Refresh' --first || true
sleep 35
ax click --title 'Refresh' --first || true
sleep 6
pair_sheet 5-fast-sheet

note "6 the panel closed: usage speed stays in the panel, then a turn with the working row"
close_panel
# Routine usage speed changes no longer post into the conversation.
# Stop the rise before it reaches the moved pause threshold.
"$T/guest.sh" "$VM" 'touch /Users/admin/fill-first/stop-rise'
hold_turn 'How are the three of them doing?' 'All three are still working. Nothing needs you right now.' 6
pair 6-fast-alert
release_turn

note "7 flat readings: normal usage stays in the panel, then a turn with the working row"
open_panel
ax click --title 'Refresh' --first || true
sleep 6
close_panel
# Normal meter readings also stay out of the conversation.
hold_turn 'Thanks. Keep me posted.' 'Will do.' 7
evidence 7
pair 7-back-to-normal
release_turn
"$T/guest.sh" "$VM" 'cat /Users/admin/fill-first/calls.log' > "$S/calls.log" 2>&1
note "done"
exit 0
