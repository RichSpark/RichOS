#!/bin/bash
# nightly81-walk.sh — the nightly 81 candidate on the real app, in one guest run: the accounts
# screen's move lines (c88e94b07: "Switching to", "Can't move yet", "now uses" only once the work
# has really moved), the quota hold card's staggered-resume sentence (eed78278e, 414fb603b), the
# update check, a plain message and its reply, and the installed engine's runtime/bin launchers
# (ac957af41: Mach-O, not #! scripts).
#
#   ACCOUNTS_WALK_VERSION=1.2.5-nightly.20261029.81 run-walk.py --bundle <RichOS.app.zip> \
#     --home <empty dir> --engine <engine.tar.gz> --report <json> -- nightly81-walk.sh <out-dir>
#
# FIXTURE-DRIVEN as accounts-walk.sh (whose setup and steps 0 to 5 are reused here; changed for this build: the start screen door, the company question, and the Settings rows as AXButton):
# fake-claude-fill-first.pl answers every `claude` call, so no real Claude account is read or
# switched. A held turn (the fake's `slow` file) and the `agents` file are how a walk keeps work
# running on the account in use while "Use this one now" is pressed. EXIT STATUS: 1 when any
# capture or check fails (named at the end), else 0.
#
# WHAT THIS WALK DOES NOT REACH (2026-10-29, measured): (1) "Can't move yet: work is still running
# on <account>" needs work the TEAM runs on the account in use; the fake's helpers are on Rich's
# own lease, so the real app answers "Switching to" for them. That line is checked by
# ui/tests/accounts.js (mock, both themes). (2) The quota card's "They resume one at a time from"
# needs the held team: the quota gate refuses the fake's helpers first with "this work has no open
# grant" (calls.log "gate walk-agent-N exit 2"), so no hold is recorded and the card reads "Ready to
# pause ... No pauses observed yet." A hold needs a real work grant (work-lease-walk.py's setup).
# That sentence is checked by ui/tests/quota.js (mock).
set -u
VM="$1"
S="${2:?usage: accounts-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$S"
# 40 s per accessibility call instead of ax.sh's 20: the 2026-10-06 runs shared the host with a
# second guest and suites at 92 to 99% CPU, and a read past 20 s was the harness, not the app.
export TESTVM_AX_TIMEOUT="${TESTVM_AX_TIMEOUT:-40}"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
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
# press <title> <check name>: one control, by its words, retried: a loaded host can push one
# accessibility read past its 20 s deadline (the 2026-10-06 run, "exit 124"), and that is the
# harness, not the app.
press() {
  for _ in 1 2 3; do
    ax click --title "$1" --contains --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  fail "$2 (could not press: $1)"
  return 1
}
menu_open() { ax find --title 'Light theme' --first >/dev/null 2>&1; }
# menu open|closed: pressed until the menu READS as that state, at most three times. On a loaded
# host a press can report its deadline after it took effect (the 2026-10-06 runs logged "no
# Settings button" while the menu had opened), so what is on screen decides, never the press's
# exit code: a second blind press only closed what the first had opened.
menu() {
  for _ in 1 2 3; do
    if [ "$1" = open ]; then menu_open && return 0; else menu_open || return 0; fi
    ax click --title 'Settings' --role AXPopUpButton >/dev/null 2>&1 || true
    sleep 3
  done
  if [ "$1" = closed ]; then ax --key 53 >/dev/null 2>&1 || true; sleep 2; menu_open || return 0; fi
  note "the menu would not be $1"
}
theme() {  # theme Light|Dark; the press is retried only while the menu is still open to take it
  menu open
  for _ in 1 2 3; do
    ax click --title "$1 theme" >/dev/null 2>&1 && break
    sleep 2
  done
  sleep 1
  menu closed
}
sheet_open() { ax find --title 'Close Claude accounts' --first >/dev/null 2>&1; }
# The Settings row is an AXMenuItem whose title runs on into its line ("Claude accounts Using Work,
# switched 2:35 PM"). Pressed by role: the first node titled Claude accounts is an empty AXGroup
# named after the closed sheet, and pressing it did nothing (the 2026-10-06 14:38Z find).
open_sheet() {
  for try in 1 2; do
    menu open
    ax click --title 'Claude accounts' --role AXButton --contains --first >/dev/null 2>&1 || note "no Claude accounts row"
    sleep 3
    sheet_open && return 0
    note "accounts sheet not open after try $try"
  done
  return 1
}
close_sheet() {
  sheet_open || return 0
  ax click --title 'Close Claude accounts' >/dev/null 2>&1 || true
  sleep 2
  sheet_open && { ax --key 53 || true; sleep 2; }
  return 0
}
relaunch_app() {
  python3 - "$VM" "$T" <<'PY'
import sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment={'RICHOS_CLAUDE_BIN': '/Users/admin/fill-first/claude'})) # machine-path-exempt: isolated test VM account/path; never a customer default
PY
}
# usage <file> <five %> <weekly %>
usage() { "$T/guest.sh" "$VM" "printf '{\"five\":$2,\"weekly\":$3}' > \"$1\""; }
login_as() { "$T/guest.sh" "$VM" "printf '%s' '$1' > /Users/admin/fill-first/login-as"; } # machine-path-exempt: isolated test VM account/path; never a customer default
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
field() {  # field <label> <text>: type into the input a label names
  ax type "$2" --title "$1" --contains --replace >/dev/null 2>&1 && return 0
  note "no field labeled $1"
  return 1
}

"$T/guest.sh" "$VM" 'mkdir -p /Users/admin/fill-first' || setup_failed "the fixture folder" # machine-path-exempt: isolated test VM account/path; never a customer default
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" /Users/admin/fill-first/claude || setup_failed "pushing the fake claude" # machine-path-exempt: isolated test VM account/path; never a customer default
"$T/guest.sh" "$VM" 'chmod 755 /Users/admin/fill-first/claude' || setup_failed "the fake claude's mode" # machine-path-exempt: isolated test VM account/path; never a customer default
usage /Users/admin/fill-first/usage-1.json 20 86 || setup_failed "the one account's usage fixture" # machine-path-exempt: isolated test VM account/path; never a customer default
relaunch_app || setup_failed "relaunching the app with the fake claude"

# This build opens on the start screen, with its door into the conversation: "Talk to Rich" (as
# github-move-walk.sh and output-walk.py press it). First-run questions then sit over the
# conversation (setup sheet, memory question; each says Not now, main.js WHAT A FIRST RUN ASKS) and
# the company question after them; the composer is off until a company is added.
wait_for 'Talk to Rich' || setup_failed "no start screen with Talk to Rich"
for _ in 1 2 3; do ax click --title 'Talk to Rich' --first >/dev/null 2>&1 && break; sleep 3; done
sleep 5
for _ in 1 2 3 4 5; do
  ax find --title 'Add this company' --first >/dev/null 2>&1 && break
  ax click --title 'Not now' --first >/dev/null 2>&1 || true
  sleep 4
done
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
ax find --role AXTextArea --first >/dev/null 2>&1 || note "no composer after the company was added"

# The guest starts on the system theme (light); the shots named -dark are taken in Dark.
theme Dark
note "0 Settings with Technical view off: the row is there, with its plain line"
menu open
# Technical view off: the detailed quota row is hidden. Not asked of the accessibility tree: on the
# 2026-10-06 run a find of "Claude Code quota" succeeded while the guest's screen showed no such
# row. The shot 0-menu-one shows the menu, and tests/accounts.js asserts the row hidden.
has 'Claude accounts' "0 the Claude accounts row"
has '86% of this week used' "0 the row's line"
# The build under test names itself in the menu's version line (ACCOUNTS_WALK_VERSION, the
# candidate's version, e.g. 1.2.0-nightly.20261007.42); unset, the line is not asked for.
if [ -n "${ACCOUNTS_WALK_VERSION:-}" ]; then has "RichOS ${ACCOUNTS_WALK_VERSION}" "0 the version line"; fi
shot 0-menu-one
menu closed

note "1 Rich suggests a second account, in the conversation"
send_turn 'What is on my plate today?'
wait_for 'Add a second account' || true
has 'If you have a second Claude account' "1 Rich's suggestion"
has 'Not now' "1 Not now beside it"
shot 1-nudge

note "2 adding, step 1 of 2: the two names (from Rich's button)"
press 'Add a second account' "2 Add a second account"
sleep 3
has 'Step 1 of 2' "2 step 1"
shot 2-add-step1
field 'The account you use now' 'Home' || fail "2 the first name"
field 'The new account' 'Work' || fail "2 the new name"
press 'Continue' "2 Continue"
sleep 2
has 'Sign in to Work' "3 step 2"
shot 3-add-step2

note "4 the same account signed in again"
login_as 'account-1@fixture.invalid' || fail "4 the login-as fixture"
: > "$S/.empty"; "$T/guest.sh" "$VM" --push "$S/.empty" /Users/admin/fill-first/calls.log || true; rm -f "$S/.empty" # machine-path-exempt: isolated test VM account/path; never a customer default
press 'Open sign-in' "4 Open sign-in"
wait_for 'That is the account you already use' || true
has 'You signed in as' "4 the same-account screen"
has 'Try again' "4 Try again"
shot 4-same-account
"$T/guest.sh" "$VM" 'cat /Users/admin/fill-first/calls.log' > "$S/4-calls.log" 2>&1 || fail "4 calls.log" # machine-path-exempt: isolated test VM account/path; never a customer default
grep -q 'claude-accounts/2 auth logout' "$S/4-calls.log" || fail "4 the same account was not signed out of the new folder"

note "5 the right account: Work is ready"
login_as 'work@fixture.invalid' || fail "5 the login-as fixture"
# shellcheck disable=SC2016  # the $(...) is meant to expand in the guest's shell, not here
# The app's own data folder: the claude-accounts under com.richos.app. The bare name matched a
# folder directly in the guest home on candidate 43, so every record read missed (walk
# walk-cb80366dad73: "app data: <guest-home>").
DATA=$("$T/guest.sh" "$VM" 'dirname "$(find /Users/admin/testvm -type d -path "*/com.richos.app/claude-accounts" 2>/dev/null | head -1)"') # machine-path-exempt: isolated test VM account/path; never a customer default
note "app data: $DATA"
usage "$DATA/claude-accounts/2/usage.json" 0 12 || fail "Work's usage fixture"
press 'Try again' "5 Try again"
wait_for 'is ready' || true
has 'is ready. Rich switches to it when Home is nearly full' "5 Work is ready"
has 'You added' "5 Recent changes"
shot 5-two-dark
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/5-accounts.txt" 2>&1 || fail "5 the accounts record"
grep -q '"label": *"Home"' "$S/5-accounts.txt" || fail "5 the first account is not named Home"
grep -q '"label": *"Work"' "$S/5-accounts.txt" || fail "5 Work is not on the record"
grep -q '"kind": *"added"' "$S/5-accounts.txt" || fail "5 Recent changes has no added row"
close_sheet; theme Light; open_sheet || fail "5 the sheet in light"; shot 5-two-light
close_sheet; theme Dark
# ---------------------------------------------------------------------------------------------
# Nightly 81 steps. Steps 0 to 5 above are accounts-walk.sh's: Home in use, Work added and ready,
# the app named in Settings (ACCOUNTS_WALK_VERSION) and one plain message and reply already sent.
# ---------------------------------------------------------------------------------------------
FF=/Users/admin/fill-first # machine-path-exempt: isolated test VM account/path; never a customer default
gput() {  # gput <guest path> <text>: a file the fake reads, pushed rather than quoted
  printf '%s\n' "$2" > "$S/.put"
  "$T/guest.sh" "$VM" --push "$S/.put" "$1" || note "could not write $1"
  rm -f "$S/.put"
}
record() { "$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/$1.txt" 2>&1 || fail "$1 the accounts record"; }
in_use() { "$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" 2>/dev/null | grep -o '"inUse": *"[0-9]*"' | head -1 | grep -o '[0-9]*'; }
start_turn() {  # a turn sent and left running (the fake holds it while `slow` exists)
  ax type "$1" --role AXTextArea --first --replace || note "no composer"
  ax click --title 'Send' --first || note "no Send"
  sleep 6
}
end_turn() {  # the fake answers; the turn has ended once Send is back
  "$T/guest.sh" "$VM" "rm -f $FF/slow"
  for _ in $(seq 1 40); do
    ax find --title 'Send' --first >/dev/null 2>&1 && break
    sleep 2
  done
  sleep 3
}
move_line() {  # which of the four Use-this-one-now lines the sheet draws right now
  for w in "Switching to" "Can't move yet" "Rich now uses" "Rich still uses"; do
    ax find --value "$w" --contains --first >/dev/null 2>&1 && { echo "$w"; return; }
  done
  echo none
}
await_line() {  # await_line <words> <seconds>: the line reads as that, polled
  local end=$(( $(date +%s) + $2 ))
  while [ "$(date +%s)" -lt "$end" ]; do
    ax find --value "$1" --contains --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  return 1
}
line_nodes() {  # line_nodes <name> <words>: the node (with its geometry) holding the line
  ax find --value "$2" --contains --json > "$S/$1.json" 2>&1 || true
}

note "I identity: the app on disk, read in the guest"
# shellcheck disable=SC2016  # the $(...) and $p are meant to expand in the guest's shell, not here
"$T/guest.sh" "$VM" 'for p in $(find /Users/admin -maxdepth 6 -name Info.plist -path "*RichOS.app/Contents/Info.plist" 2>/dev/null); do echo "$p"; /usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" -c "Print :CFBundleVersion" "$p"; done' > "$S/I-app-version.txt" 2>&1 || fail "I the app version in the guest" # machine-path-exempt: isolated test VM account/path; never a customer default

note "S smoke: a message and its reply, then Settings and the update check"
gput "$FF/reply.txt" "Smoke reply for nightly 81: the client report is on track."
send_turn 'Smoke check: please confirm you are there.'
has 'Smoke reply for nightly 81' "S the reply to a plain message"
shot S1-reply
ax tree > "$S/S1-reply.tree" 2>&1 || true
"$T/guest.sh" "$VM" "rm -f $FF/reply.txt"
menu open
shot S2-settings
ax click --title 'Check for updates' --contains --first >/dev/null 2>&1 || fail "S no Check for updates control"
sleep 10
shot S3-update-check
ax tree > "$S/S3-update-check.tree" 2>&1 || true
ax find --value 'update' --contains --json > "$S/S3-update-text.json" 2>&1 || true
menu closed

note "R runtime/bin: every launcher of the installed engine is a Mach-O executable, not a #! script"
# shellcheck disable=SC2016  # the $(...), $d and $f are meant to expand in the guest's shell, not here
"$T/guest.sh" "$VM" 'for d in $(find /Users/admin -type d -path "*/runtime/bin" 2>/dev/null | head -5); do echo "## $d"; for f in "$d"/*; do echo "-- $f"; file "$f"; head -c 2 "$f" | od -An -c; done; done' > "$S/R-runtime-bin.txt" 2>&1 || fail "R reading runtime/bin" # machine-path-exempt: isolated test VM account/path; never a customer default

note "M1 pending, dark: a turn is running on Home; Use this one now on Work"
close_sheet; theme Dark
echo 1 > "$S/.one"; "$T/guest.sh" "$VM" --push "$S/.one" "$FF/slow"; rm -f "$S/.one"
start_turn 'Keep working on the client report until I say stop.'
open_sheet || fail "M1 the sheet"
shot M1a-before-dark
ax tree > "$S/M1a-before-dark.tree" 2>&1 || true
n=$(ax find --title 'Use Work now' --first >/dev/null 2>&1 && echo yes || echo no)
note "M1 Use Work now offered: $n"
press 'Use Work now' "M1 Use this one now on Work"
sleep 4
M1=$(move_line); note "M1 line after the press: $M1"
echo "$M1" > "$S/M1b-line.txt"
line_nodes M1b-line-dark "$M1"
shot M1b-pressed-dark
ax tree > "$S/M1b-pressed-dark.tree" 2>&1 || true
record M1b-record-pressed
[ "$M1" = "Rich now uses" ] && fail "M1 the line says Rich now uses Work while the work still runs on Home"
note "M1 in use right after the press: $(in_use) (1 = Home)"
sleep 12
M1c=$(move_line); note "M1 line 12 s later, turn still running: $M1c"
echo "$M1c" > "$S/M1c-line.txt"
[ "$M1c" = "Rich now uses" ] && fail "M1 still-running turn: the line says Rich now uses Work"
note "M1 the turn ends"
end_turn
sleep 3
M1d=$(move_line); note "M1 line right after the turn ended: $M1d"
echo "$M1d" > "$S/M1d-line.txt"
shot M1d-turn-ended-dark
record M1d-record-turn-ended
if [ "$M1d" != "Rich now uses" ]; then
  note "M1 the move needs Rich's next reply: sending one"
  send_turn 'Thanks. Anything else on the report?'
  await_line 'Rich now uses' 90 || fail "M1 the line never read Rich now uses Work after the next reply"
fi
M1e=$(move_line); note "M1 line at the end: $M1e; in use: $(in_use) (2 = Work)"
line_nodes M1e-line-dark "$M1e"
shot M1e-moved-dark
ax tree > "$S/M1e-moved-dark.tree" 2>&1 || true
record M1e-record-moved
[ "$(in_use)" = 2 ] || fail "M1 Work is not the account in use after the move"
[ "$M1e" = "Rich now uses" ] || fail "M1 the final line is not Rich now uses (it is: $M1e)"

note "M2 blocked, light: helpers are working on Work; Use this one now on Home"
close_sheet; theme Light
gput "$FF/agents" "$(printf 'Mark\nAndy\nTom')"
echo 1 > "$S/.one"; "$T/guest.sh" "$VM" --push "$S/.one" "$FF/slow"; rm -f "$S/.one"
start_turn 'Keep the team going on the outbox and the pairing screens.'
sleep 6
open_sheet || fail "M2 the sheet"
shot M2a-before-light
ax tree > "$S/M2a-before-light.tree" 2>&1 || true
press 'Use Home now' "M2 Use this one now on Home"
sleep 4
M2=$(move_line); note "M2 line after the press: $M2"
echo "$M2" > "$S/M2b-line.txt"
line_nodes M2b-line-light "$M2"
shot M2b-pressed-light
ax tree > "$S/M2b-pressed-light.tree" 2>&1 || true
record M2b-record-pressed
[ "$M2" = "Rich now uses" ] && fail "M2 the line says Rich now uses Home while the work still runs on Work"
note "M2 in use right after the press: $(in_use) (2 = Work)"
close_sheet

note "H the account in use at 95% of its five hours, 15 minutes to its reset (the held-team card needs a real work grant: see the header of this walk)"
end_turn   # M2's turn ends first
CUR=$(in_use); note "H the account in use: $CUR"
if [ "$CUR" = 1 ]; then UF="$FF/usage-1.json"; WK=86; else UF="$DATA/claude-accounts/$CUR/usage.json"; WK=12; fi
NOW=$("$T/guest.sh" "$VM" 'date +%s' | tr -dc 0-9)
printf '{"five":95,"weekly":%s,"five_at":%s,"week_at":%s}' "$WK" "$((NOW + 900))" "$((NOW + 4 * 86400))" > "$S/.held-usage"
"$T/guest.sh" "$VM" --push "$S/.held-usage" "$UF" || fail "H the usage fixture"
rm -f "$S/.held-usage"
# A turn held open: each second of it is a step of every helper, and the gate refuses them at the line.
echo 1 > "$S/.one"; "$T/guest.sh" "$VM" --push "$S/.one" "$FF/slow"; rm -f "$S/.one"
start_turn 'Keep the team going on the pairing screens.'
sleep 20
quota_refresh() {
  for _ in 1 2 3; do
    ax click --title 'Refresh' --role AXButton --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  fail "$1 (could not press: Refresh)"
}
quota_sheet() {
  for try in 1 2 3; do
    menu open
    ax click --title 'Claude Code quota' --role AXButton --contains --first >/dev/null 2>&1 || note "no Claude Code quota row (try $try)"
    sleep 4
    ax find --title 'Close Claude Code quota' --first >/dev/null 2>&1 && return 0
  done
  fail "$1 (the Claude Code quota sheet did not open)"
  return 1
}
quota_close() { ax click --title 'Close Claude Code quota' >/dev/null 2>&1 || ax --key 53 >/dev/null 2>&1 || true; sleep 2; }
theme Dark
menu open
ax click --title 'Technical view' --first >/dev/null 2>&1 || note "no Technical view switch"
sleep 3
if ax find --title 'Turn it on' --first >/dev/null 2>&1; then press 'Turn it on' "H Turn it on"; sleep 3; fi
for t in Dark Light; do
  if [ "$t" = Light ]; then lower=light; else lower=dark; fi
  theme "$t"
  quota_sheet "H the quota sheet ($lower)" || continue
  quota_refresh "H Refresh ($lower)"
  sleep 6
  ax tree > "$S/H-hold-$lower.tree" 2>&1 || true
  shot "H-hold-$lower"
  ax find --value 'resume' --contains --json > "$S/H-resume-$lower.json" 2>&1 || true
  ax find --value 'Holding' --contains --json > "$S/H-holding-$lower.json" 2>&1 || true
  has 'Ready to pause' "H at 95% the card says the team will pause ($lower)"
  quota_close
done
"$T/guest.sh" "$VM" "cat \"$DATA/engine-state/claude-quota.json\" 2>&1; echo; cat \"$FF/calls.log\" | grep -c gate" > "$S/H-quota-record.txt" 2>&1 || true
"$T/guest.sh" "$VM" "tail -40 \"$FF/calls.log\"" > "$S/H-calls-tail.txt" 2>&1 || true
record H-record
end_turn
menu open
ax click --title 'Technical view' --first >/dev/null 2>&1 || note "Technical view not switched back off"
sleep 3
if ax find --title 'Turn it off' --first >/dev/null 2>&1; then press 'Turn it off' "H Turn it off"; sleep 3; fi
menu closed

if [ "${#FAILS[@]}" -gt 0 ]; then
  note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
  exit 1
fi
note "done"
exit 0
