#!/bin/bash
# accounts-walk.sh — feedback item 9 (round 18) on the real app, in one guest run: a person who
# is not technical finds "Claude accounts" in Settings, is offered a second account by Rich at
# 86% of the week, adds it in two steps (signing in with the same account first, then the right
# one), and sees why Rich switched, in the conversation, in the sheet's banner and in Recent
# changes.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- accounts-walk.sh <out-dir>
#
# His words (richos-hq docs/ceo-input/2026-10-06_01/feedback.md, item 9): "Non-technical users
# should also be able to take advantage of the multi-account setup". The design is round 18
# (richos-hq design/mockups/rounds/round-18/multi-account.html and NOTES.md).
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the clone
# when this returns (CEO §54). FIXTURE-DRIVEN, as use-first-walk.sh: fake-claude-fill-first.pl
# answers every `claude` call from per-account usage files, and says who each folder is signed in
# as (login-as), so no real Claude account is read, signed in or switched. In <out-dir>:
#   0-menu-one          Settings, Technical view off: "Claude accounts", "86% of this week used"
#   1-nudge             Rich's suggestion in the conversation, with Add a second account / Not now
#   2-add-step1         Adding, step 1 of 2: the two names
#   3-add-step2         Step 2 of 2: Open sign-in
#   4-same-account      Signed in with the account already in use: its own screen, Try again
#   5-two-{dark,light}  The right account: Work is ready, two cards, Recent changes
#   5b-d13-sheet-{dark,light}  D13: two accounts, nothing pressed, "Pause the team until it is
#                       fresh again" selected (AXValue in 5b-radios-*.json); 5b-d13-quota-on the
#                       Technical view's Automatic pause on; 5b-d13-quota-held Home's five-hour at
#                       95%: "Ready to pause", and no switch. ACCOUNTS_WALK_UNTIL=d13 ends here.
#   5c-age-{fresh-dark,dark,light}  The quota sheet's account lanes: no age while both readings
#                       are current, then "read N min ago" on both after 5.5 min of null answers
#                       (with .read.json, the matching nodes, and .tree). ACCOUNTS_WALK_UNTIL=age runs 5c and ends there.
#   5d-*                The CEO's §153: a weekly switch while a helper runs. ACCOUNTS_WALK_UNTIL=weekly-helper
#                       runs 5d after 5 (before 5b) and ends there: 5d-job-running, 5d-switch-helper-running,
#                       5d-after-move, with 5d-calls.log, 5d-calls-running.log, 5d-journal.txt and
#                       5d-records-{at-switch,end}.txt (the block's own comment says what is checked).
#   5e-*                The CEO's §153: during a hold a helper's command runs and finishes, and its
#                       next model call waits until the hold releases. ACCOUNTS_WALK_UNTIL=hold-helper
#                       runs 5e after 5 and ends there: 5e-held-helper, with 5e-calls.log,
#                       5e-steps-held.txt, 5e-waits-held.txt and the rest the block names.
#   6-switched-chat     Home's week at 99%: Rich's line, with See your accounts
#   7-switched-{dark,light}  The sheet: the banner (when and why) and Recent changes
#   8-menu-two          Settings: "Using Work, switched <time>"
#   9-chose-side        Item 8 (his test): a third account, Side, added at 97% of its week (Home
#                       rests at 99% until its week resets, so it is never offered again), chosen
#                       with "Use this one now"; 9-side-in-use the record says Side and Recent
#                       changes "You chose Side"; then Side reaches 99% and 9-switched-again: Rich
#                       switched to Work by himself
#   10-menu-technical   2026-10-06_02: Technical view on, the quick settings quota row reads the
#                       in-use account's WEEKLY figure ("Work 12% weekly"; its five-hour is 0%)
#   11-menu-1024-{light,dark}  The window at 1024x700: the "Using Work, switched <time>" row,
#                       with its box in 11-row-{light,dark}.json, for qa/frame.py and contrast.py
#   12-no-five-{dark,light}  ACCOUNTS_WALK_UNTIL=no-five, and nothing else: the one account answers
#                       with only its week (49%), as an account with no five-hour limit does since
#                       2026-10-31 (the CEO: "If no 5-hour-limit data, then don't show/don't handle
#                       it"). The sheet shows the Weekly limit meter alone and a lede that names
#                       the week only: nothing says 5-hour limit or 5 hours.
# Steps 9 to 11 were added by the nightly 41 walk (ray-opus-walk41): no walk pressed Use this one
# now, read the quota row with Technical view on, or looked at the row in the smallest window.
# EXIT STATUS: 1 when the setup fails or any capture or check below fails (named at the end);
# 0 only when every capture was saved and every check read what it should.
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
sheet_open() { ax find --title 'Close Claude accounts' --first >/dev/null 2>&1; }
# The Settings row is an AXButton (an AXMenuItem before the start screen) whose title runs on into its line ("Claude accounts Using Work,
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
if [ "${ACCOUNTS_WALK_UNTIL:-}" = no-five ]; then
  usage /Users/admin/fill-first/usage-1.json null 49 || setup_failed "the weekly-only usage fixture" # machine-path-exempt: isolated test VM account/path; never a customer default
else
  usage /Users/admin/fill-first/usage-1.json 20 86 || setup_failed "the one account's usage fixture" # machine-path-exempt: isolated test VM account/path; never a customer default
fi
relaunch_app || setup_failed "relaunching the app with the fake claude"

# This build opens on the start screen, with its door into the conversation: "Talk to Rich" (as
# github-move-walk.sh and output-walk.py press it). First-run questions then sit
# over the conversation (setup sheet, memory question; each says Not now, main.js WHAT A FIRST RUN
# ASKS) and the company question after them; the composer is off until a company is added.
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
if [ "${ACCOUNTS_WALK_UNTIL:-}" = no-five ]; then
  note "12 an account with no five-hour window: the Weekly limit meter alone"
  for t in Dark Light; do
    if [ "$t" = Light ]; then lower=light; else lower=dark; fi
    theme "$t"
    open_sheet || fail "12 the sheet ($lower)"
    has 'Weekly limit' "12 the Weekly limit meter ($lower)"
    has '49' "12 the week's 49% ($lower)"
    has 'can do in a week' "12 the lede names the week only ($lower)"
    for words in '5-hour limit' '5 hours'; do
      if ax find --title "$words" --contains --first >/dev/null 2>&1 || ax find --value "$words" --contains --first >/dev/null 2>&1; then
        fail "12 '$words' is on screen ($lower)"
      else
        note "read: no '$words' on screen ($lower)"
      fi
    done
    ax tree > "$S/12-no-five-$lower.tree" 2>&1 || true
    shot "12-no-five-$lower"
    close_sheet
  done
  if [ "${#FAILS[@]}" -gt 0 ]; then
    note "done at 12, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
    exit 1
  fi
  note "done at 12 (ACCOUNTS_WALK_UNTIL=no-five)"
  exit 0
fi
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

# 5d (the CEO's §153, 2026-10-31): THE WEEKLY SWITCH WHILE A HELPER RUNS, handled exactly as the
# five-hour one. No helper is told to stop, commit or hand off; running work carries on, and Stu
# moves once nothing of his runs. Two accounts, automatic pause on (checked below; 5b reads the
# same switch on screen). It runs before 5b: 5b's jump of Home's five hours from 20% to 95% is a
# measured speed that keeps Home's five-hour hold on for the rest of that window (quota.rs
# pre_hold), and a helper on Home would be held by it, which is not what this stage asks. The front desk
# writes a job down and the back end launches one helper (the fake's `register` and
# `work-agents`), which steps through the app's gate every 2 s after the back end's turn ends.
# Home's week is then written as 99 and read from Rich's next turns (each streams its usage, as
# step 6 does): the switch to Work is decided and Home is being left (`leaving: ["1"]`).
# Checked from the fake's calls.log, where each gate step is a line:
#   - the app's QUOTA gate admits every helper step on Home after the switch: no order, no
#     quota refusal and no hold, although the weekly hold stands on Home's row, at least 3 in
#     45 s, which is past the app's 30 s idle check (work_host.rs ACCOUNT_CHECK_EVERY).
#     Admitted is exit 0 or a refusal by the engine's hook behind the gate, which the gate runs
#     only once it has admitted the call. The quota gate's own refusals start "RichOS desktop
#     quota:" or "RichOS desktop:", the old order said "Stop the task now", and a held step
#     never returns;
#   - no back-end session opens on Work while the helper runs, and Stu is told `account_switch`
#     on Home, where he keeps his session;
#   - once it finishes on its own (work-agents-finish), a back-end session opens on Work and Stu
#     is told `account_moved` there;
#   - nothing says stop or handoff, no handoff marker exists, no work record says failed.
# ACCOUNTS_WALK_UNTIL=weekly-helper runs 5d after 5 and ends there.
# What 5d and 5e share: the fake's folder, the job, a wait on its calls.log, the app's records and
# the automatic-pause check.
G=/Users/admin/fill-first # machine-path-exempt: isolated test VM account/path; never a customer default
JOB='Start the Northwind job and keep Mark on it.'
wait_calls() {  # wait_calls <extended regex> [tries of 5 s, default 24]
  for _ in $(seq 1 "${2:-24}"); do
    "$T/guest.sh" "$VM" "grep -Eq '$1' $G/calls.log" >/dev/null 2>&1 && return 0
    sleep 5
  done
  return 1
}
records() {  # the work records, anything under engine-state/handoffs, the quota file, the accounts, the policy
  "$T/guest.sh" "$VM" "find \"$DATA/engine-state/assignments\" \"$DATA/engine-state/handoffs\" -type f -name '*.json' 2>/dev/null | sort | while IFS= read -r f; do echo \"== \$f\"; cat \"\$f\"; echo; done; echo '== claude-quota.json'; cat \"$DATA/engine-state/claude-quota.json\" 2>&1; echo; echo '== claude-accounts.json'; cat \"$DATA/claude-accounts.json\" 2>&1; echo; echo '== claude-quota-policy.json'; cat \"$DATA/claude-quota-policy.json\" 2>&1; echo; true" > "$1" 2>&1
}
# The guest command that succeeds when automatic pause is on: claude-quota-policy.json once a
# choice was saved, else the published file's own (quota/gate.rs decide). 5b found no saved file
# and the switch on.
policy_on() {
  if "$T/guest.sh" "$VM" "test -f \"$DATA/claude-quota-policy.json\"" >/dev/null 2>&1; then
    echo "grep -Eq '\"enabled\": *true' \"$DATA/claude-quota-policy.json\""
  else
    echo "grep -Eq '\"policy\": *\\{ *\"enabled\": *true' \"$DATA/engine-state/claude-quota.json\""
  fi
}
if [ "${ACCOUNTS_WALK_UNTIL:-}" = weekly-helper ]; then
  collect5d() {
    "$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/5d-calls.log" 2>&1 || fail "5d calls.log"
    "$T/guest.sh" "$VM" "find \"$DATA/engine-state/evidence\" -type f -name callbacks.jsonl 2>/dev/null | sort | while IFS= read -r f; do echo \"== \$f\"; cat \"\$f\"; done" > "$S/5d-journal.txt" 2>&1 \
      || fail "5d the evidence journals"
    records "$S/5d-records-end.txt" || fail "5d the app's records"
    # A gate step still waiting (a held call) has a gate process and its gate-err file in the
    # fixture folder: evidence for a run where the helper's steps stopped coming.
    "$T/guest.sh" "$VM" "ps -axo pid,etime,command | grep -e '--claude-quota-gate' | grep -v grep; for f in $G/gate-err-*; do [ -e \"\$f\" ] && { echo \"== \$f\"; cat \"\$f\"; }; done; true" > "$S/5d-gates.txt" 2>&1 || true
    # The app's own log (relaunch.py's), for what the work host said about Stu's session.
    # shellcheck disable=SC2016  # the $(...) is meant to expand in the guest's shell, not here
    "$T/guest.sh" "$VM" 'cat "$(ls -t /Users/admin/testvm/*/relaunch-*.log 2>/dev/null | head -1)"' > "$S/5d-app.log" 2>&1 || true # machine-path-exempt: isolated test VM account/path; never a customer default
  }
  end5d() {
    collect5d
    if [ "${#FAILS[@]}" -gt 0 ]; then
      note "done at 5d, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
      exit 1
    fi
    note "done at 5d (ACCOUNTS_WALK_UNTIL=weekly-helper)"
    exit 0
  }
  stop5d() { fail "$1; nothing after it can pass"; end5d; }

  note "5d a job with a helper on Home, then Home's week reaches 99%"
  "$T/guest.sh" "$VM" "rm -f $G/work-agents-finish $G/work-agents-started; : > $G/calls.log; touch $G/log-turns; printf 'Mark\n' > $G/work-agents; printf '%s\n' '$JOB' > $G/register" \
    || stop5d "5d the job fixtures"
  "$T/guest.sh" "$VM" "$(policy_on)" >/dev/null 2>&1 \
    || fail "5d automatic pause is not on: the weekly hold would not stand on Home"
  send_turn "$JOB"
  wait_calls 'register ok' || stop5d "5d the front desk never wrote the job down (no 'register ok')"
  # A step the quota gate admitted: exit 0, or refused by the engine's hook behind it (above).
  wait_calls 'gate work-agent-1 exit (0|2 said: [^R]|2 said: R[^i])' \
    || stop5d "5d the back end's helper never stepped through the quota gate admitted"
  note "5d ok: the helper runs and steps through the gate"
  shot 5d-job-running
  usage "$G/usage-1.json" 20 99 || fail "5d Home's 99% weekly fixture"
  send_turn 'How is the Northwind job going?'
  send_turn 'And the pairing screens?'
  switched=""
  for _ in $(seq 1 24); do
    if "$T/guest.sh" "$VM" "grep -Eq '\"leaving\": *\\[ *\"1\" *\\]' \"$DATA/engine-state/claude-quota.json\" && grep -Eq '\"inUse\": *\"2\"' \"$DATA/claude-accounts.json\"" >/dev/null 2>&1; then
      switched=1; break
    fi
    sleep 5
  done
  [ -n "$switched" ] || stop5d "5d the weekly switch was never decided (no leaving [\"1\"] with Work in use)"
  AT=$("$T/guest.sh" "$VM" 'date +%s')
  note "5d ok: Home is being left for Work (guest clock $AT)"
  records "$S/5d-records-at-switch.txt" || fail "5d the records at the switch"

  note "5d the helper carries on through the switch (45 s)"
  sleep 45
  "$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/5d-calls-running.log" 2>&1 || fail "5d calls.log while the helper runs"
  "$T/guest.sh" "$VM" "ps -axo pid,ppid,etime,command | grep -e 'fill-first/claude' | grep -v grep | cut -c1-300; true" > "$S/5d-procs-running.txt" 2>&1 || true
  steps=$(awk -v at="$AT" '$1 >= at && $3 == "gate" && $4 == "work-agent-1"' "$S/5d-calls-running.log")
  gate_refused() { awk 'NF && (/Stop the task now/ || ($6 != "0" && / said: RichOS desktop( quota)?: /))'; }
  admitted=$(printf '%s\n' "$steps" | awk 'NF && !(/Stop the task now/ || ($6 != "0" && / said: RichOS desktop( quota)?: /))' | grep -c . || true)
  other=$(printf '%s\n' "$steps" | gate_refused | grep -c . || true)
  printf '%s\n' "$steps" | gate_refused > "$S/5d-gate-refusals.txt"
  # Each lease is one "lease <kind> <session>" line (fake-claude-fill-first.pl); Work's folder
  # has a space in it, so a line is matched by its words, never by a field number.
  on_work=$(awk -v at="$AT" '$1 >= at && /claude-accounts\/2 gate work-agent-1 /' "$S/5d-calls-running.log" | grep -c . || true)
  if [ "$admitted" -ge 3 ] && [ "$other" = 0 ]; then
    note "5d ok: $admitted steps of the helper after the switch, every one admitted by the quota gate"
  else
    fail "5d the helper after the switch: $admitted admitted, $other refused by the quota gate (want 3 or more admitted and none refused; 5d-gate-refusals.txt)"
  fi
  [ "$on_work" = 0 ] || fail "5d $on_work helper steps ran under Work's folder: the helper was not on Home"
  if awk -v at="$AT" '$1 >= at && /claude-accounts\/2 lease back-end /' "$S/5d-calls-running.log" | grep -q .; then
    fail "5d a back-end session opened on Work while the helper still ran (Stu moved under running work)"
  else
    note "5d ok: no back-end session on Work while the helper runs"
  fi
  # Something of Stu's runs, so he is told the switch is coming on the session he keeps (Home).
  if awk -v at="$AT" '$1 >= at && !/claude-accounts\/2/ && / turn .*account_switch/' "$S/5d-calls-running.log" | grep -q .; then
    note "5d ok: Stu was told account_switch on Home and kept his session"
  else
    fail "5d Stu was not told account_switch on Home while his helper ran"
  fi
  shot 5d-switch-helper-running

  note "5d the helper finishes on its own"
  "$T/guest.sh" "$VM" "touch $G/work-agents-finish" || fail "5d the finish fixture"
  wait_calls 'gate work-agent-1 exit [0-9]+ finished' || stop5d "5d the helper never finished (no 'gate work-agent-1 exit N finished')"
  # When it finished is the time on its own last line, not when this walk next looked.
  FIN=$("$T/guest.sh" "$VM" "awk '/ gate work-agent-1 exit [0-9]+ finished/ {print \$1; exit}' $G/calls.log")
  note "5d ok: the helper finished with an admitted step (guest clock $FIN)"
  if wait_calls 'claude-accounts/2 lease back-end ' 36; then
    note "5d ok: a back-end session opened on Work"
  else
    fail "5d no back-end session opened on Work after the helper finished"
  fi
  if wait_calls 'claude-accounts/2 turn .*account_moved' 36; then
    note "5d ok: Stu was told account_moved on Work"
  else
    fail "5d no turn on Work told Stu account_moved"
  fi
  sleep 5
  shot 5d-after-move
  collect5d
  first_work=$(awk '/claude-accounts\/2 lease back-end / {print $1; exit}' "$S/5d-calls.log")
  if [ -n "$first_work" ] && [ "$first_work" -ge "$FIN" ]; then
    note "5d ok: the back end's first session on Work started after the helper finished"
  else
    fail "5d the back end's first session on Work (${first_work:-none}) did not start after the helper finished ($FIN)"
  fi
  if grep -Eqi 'Stop the task now|RichOS handoff|hand off' "$S/5d-calls.log"; then
    fail "5d calls.log carries a stop order or a handoff"
  else
    note "5d ok: nothing in calls.log says stop or handoff"
  fi
  if grep -q '^== .*/engine-state/handoffs/' "$S/5d-records-end.txt"; then fail "5d a handoff marker exists"; fi
  if grep '"hook_event_name":"SubagentStop"' "$S/5d-journal.txt" | grep -q '"agent_id":"work-agent-1"'; then
    note "5d ok: the helper ended (SubagentStop for work-agent-1)"
  else
    fail "5d no SubagentStop for work-agent-1 in the evidence journals"
  fi
  work_states() { awk '/^== / {w = ($0 ~ /\/engine-state\/assignments\//)} w' "$@" | grep -o '"state":"[a-z-]*"'; }
  if ! grep -q '^== .*/engine-state/assignments/' "$S/5d-records-end.txt"; then
    fail "5d no work record was written (the job never reached the register)"
  elif work_states "$S/5d-records-at-switch.txt" "$S/5d-records-end.txt" | grep -q '"state":"failed"'; then
    fail "5d a work record says failed"
  else
    note "5d ok: no work record says failed (at the switch: $(work_states "$S/5d-records-at-switch.txt" | tr '\n' ' '); now: $(work_states "$S/5d-records-end.txt" | tr '\n' ' '))"
  fi
  end5d
fi
# 5e (the CEO's §153, 2026-10-31: "THE WHOLE FUCKING QUOTA THING IS ONLY EVER AN ISSUE IF THE
# WORK IS CONSUMING QUOTA/TOKENS"): DURING A HOLD A HELPER'S COMMAND RUNS AND FINISHES; ITS NEXT
# MODEL CALL WAITS. Two accounts with the 5-hour choice on Pause (the default, which 5b reads on
# screen), automatic pause on: Home's five hours at 95% hold the team on Home and move nobody.
# The front desk writes a job down and the back end launches one helper (the fake's `register`
# and `work-agents`), whose steps are tool calls as Claude Code runs them: PreToolUse through the
# app's gate, the tool ("ran"), then PostToolBatch, where the app holds (quota/gate.rs
# HOLD_EVENT). While the fake's `thinking` file exists the helper takes no step (a model call in
# flight), so the hold is in force before its next tool call comes. Checked from the fake's
# calls.log, after the guest clock at which the published reading says held:
#   - the helper's next PreToolUse comes straight back through the quota gate: exit 0, or the
#     answer of the engine's hook, which the gate runs only once it has let the call through. On
#     main the gate held the call there, before the engine saw it, until the quota freed. The
#     fake's helper was launched by the fake, not dispatched by the app, so it has no worker
#     receipt, and the engine refuses its tool for that ("worker identity has not joined its app
#     receipt"), with or without a hold: "ran" comes only for a call the engine lets run;
#   - its batch started and has not come back 45 s later (past the app's 30 s idle check), with
#     the app's own pause record for work-agent-1 (engine-state/quota-waits) and a gate process
#     still waiting: its next model call waits;
#   - the hold releases (at Home's five-hour reset, five_at, or by the unchanged staggered release
#     inside its last twenty minutes): the batch comes back, exit 0, and the helper's next tool
#     call comes through the quota gate again.
# ACCOUNTS_WALK_UNTIL=hold-helper runs 5e after 5 and ends there. Evidence: 5e-calls.log,
# 5e-calls-held.log, 5e-steps-held.txt, 5e-records-{held,end}.txt, 5e-waits-held.txt,
# 5e-gates-held.txt, 5e-journal.txt, 5e-app.log and the shot 5e-held-helper.
# shellcheck disable=SC2016  # wait_after's conditions are awk programs: awk expands their $3 and $4 in the guest
if [ "${ACCOUNTS_WALK_UNTIL:-}" = hold-helper ]; then
  collect5e() {
    "$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/5e-calls.log" 2>&1 || fail "5e calls.log"
    "$T/guest.sh" "$VM" "find \"$DATA/engine-state/evidence\" -type f -name callbacks.jsonl 2>/dev/null | sort | while IFS= read -r f; do echo \"== \$f\"; cat \"\$f\"; done" > "$S/5e-journal.txt" 2>&1 \
      || fail "5e the evidence journals"
    records "$S/5e-records-end.txt" || fail "5e the app's records"
    # shellcheck disable=SC2016  # the $(...) is meant to expand in the guest's shell, not here
    "$T/guest.sh" "$VM" 'cat "$(ls -t /Users/admin/testvm/*/relaunch-*.log 2>/dev/null | head -1)"' > "$S/5e-app.log" 2>&1 || true # machine-path-exempt: isolated test VM account/path; never a customer default
  }
  end5e() {
    "$T/guest.sh" "$VM" "rm -f $G/thinking" >/dev/null 2>&1 || true
    collect5e
    if [ "${#FAILS[@]}" -gt 0 ]; then
      note "done at 5e, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
      exit 1
    fi
    note "done at 5e (ACCOUNTS_WALK_UNTIL=hold-helper)"
    exit 0
  }
  stop5e() { fail "$1; nothing after it can pass"; end5e; }
  wait_after() {  # wait_after <guest clock> <awk condition on a calls.log line> [tries of 5 s, default 24]
    for _ in $(seq 1 "${3:-24}"); do
      "$T/guest.sh" "$VM" "awk -v at=$1 '\$1 >= at && $2 {found = 1} END {exit !found}' $G/calls.log" >/dev/null 2>&1 && return 0
      sleep 5
    done
    return 1
  }
  last_step() {  # the kind of the last calls.log line about work-agent-1: gate, ran, batch-start or batch
    "$T/guest.sh" "$VM" "awk '\$4 == \"work-agent-1\" {last = \$3} END {print last}' $G/calls.log"
  }
  waits() {  # the app's pause records still waiting (not released)
    "$T/guest.sh" "$VM" "for f in \"$DATA/engine-state/quota-waits\"/*.json; do case \"\$f\" in *.released.json) ;; *) [ -e \"\$f\" ] && { echo \"== \$f\"; cat \"\$f\"; echo; } ;; esac; done; true"
  }

  note "5e a job with a helper on Home, then Home's five hours reach 95% (Pause: a hold, no switch)"
  "$T/guest.sh" "$VM" "rm -f $G/work-agents-finish $G/work-agents-started $G/thinking; : > $G/calls.log; touch $G/log-turns; printf 'Mark\n' > $G/work-agents; printf '%s\n' '$JOB' > $G/register" \
    || stop5e "5e the job fixtures"
  "$T/guest.sh" "$VM" "$(policy_on)" >/dev/null 2>&1 || fail "5e automatic pause is not on: no hold would stand on Home"
  if "$T/guest.sh" "$VM" "grep -Eq '\"atThreshold\": *\"switch\"' \"$DATA/claude-accounts.json\"" >/dev/null 2>&1; then
    stop5e "5e the 5-hour choice is Switch: Home at its line would move the team to Work, not hold it"
  fi
  send_turn "$JOB"
  wait_calls 'register ok' || stop5e "5e the front desk never wrote the job down (no 'register ok')"
  wait_calls 'batch work-agent-1 exit 0' || stop5e "5e the helper never completed a step (no 'batch work-agent-1 exit 0')"
  note "5e ok: the helper runs, one tool call after another"
  # Its model call in flight: no step until the hold stands. A step under way ends first.
  "$T/guest.sh" "$VM" "touch $G/thinking" || stop5e "5e the thinking fixture"
  settled=""
  for _ in $(seq 1 24); do
    sleep 5
    [ "$(last_step)" = batch ] && { settled=1; break; }
  done
  [ -n "$settled" ] || stop5e "5e the helper's step under way never came back before the hold"
  RESET=$(( $("$T/guest.sh" "$VM" 'date +%s') + 300 ))
  "$T/guest.sh" "$VM" "printf '{\"five\":95,\"weekly\":86,\"five_at\":$RESET}' > $G/usage-1.json" || stop5e "5e Home's 95% five-hour fixture"
  send_turn 'How is the Northwind job going?'
  send_turn 'And the pairing screens?'
  held=""
  for _ in $(seq 1 24); do
    if "$T/guest.sh" "$VM" "tr -d ' \\n' < \"$DATA/engine-state/claude-quota.json\" | grep -q '\"admission\":{\"state\":\"held\"'" >/dev/null 2>&1; then
      held=1; break
    fi
    sleep 5
  done
  [ -n "$held" ] || stop5e "5e the hold never stood on Home (the published admission is not held)"
  AT=$("$T/guest.sh" "$VM" 'date +%s')
  [ "$AT" -lt "$RESET" ] || stop5e "5e the hold was published only after Home's reset ($AT >= $RESET)"
  note "5e ok: the hold stands on Home (guest clock $AT; Home's five hours reset at $RESET)"
  records "$S/5e-records-held.txt" || fail "5e the records at the hold"
  if wait_after "$AT" '$4 == "work-agent-1"' 1; then
    fail "5e the helper stepped while its model call was in flight (the thinking fixture did not hold it)"
  fi

  note "5e the helper's next tool call comes during the hold"
  "$T/guest.sh" "$VM" "rm -f $G/thinking" || stop5e "5e removing the thinking fixture"
  if ! wait_after "$AT" '$3 == "batch-start" && $4 == "work-agent-1"' 12; then
    "$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/5e-calls-held.log" 2>&1 || true
    stop5e "5e the helper's tool call never reached its batch during the hold (held before it ran?)"
  fi
  note "5e the batch waits (45 s)"
  sleep 45
  "$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/5e-calls-held.log" 2>&1 || fail "5e calls.log during the hold"
  waits > "$S/5e-waits-held.txt" 2>&1 || true
  "$T/guest.sh" "$VM" "ps -axo pid,etime,command | grep -e '--claude-quota-gate' | grep -v grep; true" > "$S/5e-gates-held.txt" 2>&1 || true
  awk -v at="$AT" '$1 >= at && $4 == "work-agent-1"' "$S/5e-calls-held.log" > "$S/5e-steps-held.txt"
  # Through the quota gate: exit 0, or an answer that is not the quota gate's own ("RichOS
  # desktop quota:" or "RichOS desktop:"; the engine's hook says "RichOS desktop engine:").
  if awk '$3 == "gate" && !/ said: RichOS desktop( quota)?: /' "$S/5e-steps-held.txt" | grep -q .; then
    note "5e ok: during the hold the helper's PreToolUse came straight back through the quota gate: $(awk '$3 == "gate"' "$S/5e-steps-held.txt" | head -1 | cut -d' ' -f5-)"
  else
    fail "5e during the hold no PreToolUse of the helper came back through the quota gate (5e-steps-held.txt)"
  fi
  if awk '$3 == "ran"' "$S/5e-steps-held.txt" | grep -q .; then
    note "5e ok: its tool ran during the hold"
  else
    note "5e its tool did not run: the engine's hook refused it, not the quota gate (the fake's helper has no worker receipt)"
  fi
  if awk '$3 == "batch-start"' "$S/5e-steps-held.txt" | grep -q . && ! awk '$3 == "batch"' "$S/5e-steps-held.txt" | grep -q .; then
    note "5e ok: its batch has not come back after 45 s: its next model call waits"
  else
    fail "5e its batch did not wait during the hold (5e-steps-held.txt)"
  fi
  if grep -q 'work-agent-1' "$S/5e-waits-held.txt"; then
    note "5e ok: the app's pause record for work-agent-1 is there"
  else
    fail "5e no pause record for work-agent-1 in engine-state/quota-waits (5e-waits-held.txt)"
  fi
  grep -q -e '--claude-quota-gate' "$S/5e-gates-held.txt" || fail "5e no gate process is waiting (5e-gates-held.txt)"
  shot 5e-held-helper

  note "5e the hold releases and the batch comes back"
  now=$("$T/guest.sh" "$VM" 'date +%s')
  wait_after "$AT" '$3 == "batch" && $4 == "work-agent-1" && $6 == "0"' $(( (RESET - now) / 5 + 24 )) \
    || stop5e "5e the batch never came back after Home's reset"
  BACK=$("$T/guest.sh" "$VM" "awk -v at=$AT '\$1 >= at && \$3 == \"batch\" && \$4 == \"work-agent-1\" {print \$1; exit}' $G/calls.log")
  note "5e ok: the batch came back at $BACK (the hold stood from $AT; Home's reset at $RESET)"
  if wait_after "$BACK" '$3 == "gate" && $4 == "work-agent-1" && !/ said: RichOS desktop( quota)?: /' 6; then
    note "5e ok: the helper steps on after the release"
  else
    fail "5e the helper took no step after the release"
  fi
  end5e
fi
# D13 (walk of nightly 41): on a fresh install the sheet's "When a 5-hour limit is almost used up"
# showed neither choice selected, while Pause says "The default." Nothing above presses either
# choice, so this is still the fresh install's own setting. Selected is read from the radios'
# AXValue (aria-checked); in effect is read off the Technical view's quota sheet, whose
# Automatic pause switch and status card say what the app will do, and then shown by behavior:
# Home's five-hour at 95% makes the card say it will pause, and Rich does not switch to Work.
radio_value() {  # radio_value <words>: the AXValue of the one radio whose name holds the words
  ax find --title "$1" --role AXRadioButton --contains --json 2>/dev/null | python3 -c '
import json, sys
hits = [o for o in (json.loads(l) for l in sys.stdin if l.strip().startswith("{")) if o.get("role") == "AXRadioButton"]
print(hits[0].get("value", "") if len(hits) == 1 else "?%d" % len(hits))'
}
is_on() { case "$1" in 1|true) return 0;; *) return 1;; esac; }
quota_refresh() {  # the quota sheet's own Refresh, an AXButton, retried as press is
  for _ in 1 2 3; do
    ax click --title 'Refresh' --role AXButton --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  fail "$1 (could not press: Refresh)"
}
note "5b D13: two accounts on a fresh install; Pause is the chosen 5-hour choice, and it is in effect"
for t in Dark Light; do
  if [ "$t" = Light ]; then lower=light; else lower=dark; fi
  theme "$t"
  open_sheet || fail "5b the sheet ($lower)"
  has 'When a 5-hour limit is almost used up' "5b the 5-hour section ($lower)"
  pause_v=$(radio_value 'Pause the team until it is fresh again')
  switch_v=$(radio_value 'Switch to')
  note "5b radios ($lower): Pause=$pause_v Switch=$switch_v"
  ax find --role AXRadioButton --json > "$S/5b-radios-$lower.json" 2>&1 || true
  is_on "$pause_v" || fail "5b Pause is not selected ($lower; AXValue $pause_v)"
  { [ "$switch_v" = 0 ] || [ "$switch_v" = false ]; } || fail "5b Switch is not unselected ($lower; AXValue $switch_v)"
  shot "5b-d13-sheet-$lower"
  close_sheet
done
"$T/guest.sh" "$VM" "ls \"$DATA\"/claude-quota-policy.json 2>&1; cat \"$DATA/claude-accounts.json\"" > "$S/5b-record.txt" 2>&1 || true
theme Dark
menu open
ax click --title 'Technical view' --first >/dev/null 2>&1 || note "no Technical view switch"
sleep 3
if ax find --title 'Turn it on' --first >/dev/null 2>&1; then press 'Turn it on' "5b Turn it on"; sleep 3; fi
menu open
# Pressed by role, as open_sheet does: a plain press of the words hit a node that is not the row
# (walk-975166023ba2: "could not press", with the row on screen in 5b-d13-quota-on).
quota_open=0
for try in 1 2 3; do
  menu open
  ax click --title 'Claude Code quota' --role AXButton --contains --first >/dev/null 2>&1 || note "no Claude Code quota row (try $try)"
  sleep 4
  if ax find --title 'Close Claude Code quota' --first >/dev/null 2>&1; then quota_open=1; break; fi
done
[ "$quota_open" = 1 ] || fail "5b the Claude Code quota sheet did not open"
quota_refresh "5b Refresh, so the card reads a current figure"
sleep 6
# The switch is named "Automatically pause Rich's agents" with one account and "Automatic pause or
# switch at the line" with several (quota.js), so it is found by the word both names share
# (walk-7f24258100bf looked for the one-account name with two accounts and found nothing).
auto_v=$(ax find --title 'Automatic' --contains --json 2>/dev/null | python3 -c '
import json, sys
hits = [o for o in (json.loads(l) for l in sys.stdin if l.strip().startswith("{")) if o.get("role") == "AXCheckBox" or o.get("sub") == "AXSwitch"]
print(hits[0].get("value", "") if len(hits) == 1 else "?%d" % len(hits))')
note "5b Automatic pause switch: $auto_v"
ax find --title 'Automatic' --contains --json > "$S/5b-auto-switch.json" 2>&1 || true
is_on "$auto_v" || fail "5b the quota sheet's Automatic pause is not on (AXValue $auto_v)"
has 'On. Nothing is waiting.' "5b the status card says the pause is on"
shot 5b-d13-quota-on
usage /Users/admin/fill-first/usage-1.json 95 86 || fail "5b Home's five-hour 95% fixture" # machine-path-exempt: isolated test VM account/path; never a customer default
quota_refresh "5b Refresh"
sleep 6
has 'Ready to pause' "5b at 95% the card says the team will pause"
has 'The five-hour allowance has reached 93%' "5b the card's reason"
shot 5b-d13-quota-held
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/5b-accounts-held.txt" 2>&1 || fail "5b the accounts record"
grep -q '"inUse": *"1"' "$S/5b-accounts-held.txt" || fail "5b Rich switched to Work at the five-hour line, though Pause is chosen"
usage /Users/admin/fill-first/usage-1.json 20 86 || fail "5b Home's usage put back" # machine-path-exempt: isolated test VM account/path; never a customer default
quota_refresh "5b Refresh after"
sleep 4
ax click --title 'Close Claude Code quota' >/dev/null 2>&1 || ax --key 53 >/dev/null 2>&1 || true
sleep 2
# 5c (walk of nightly 43): an account lane whose last good reading is older than the normal check
# (5 min) says "read <age> ago" beside its figures (quota.js renderLanes, abba6a411). With both
# accounts answering null (the fake's {"null": true}, as Claude Code 2.1.289 does some of the
# time), the readings stay the last good ones and age. The lanes are read before (no age: both
# current) and after 5.5 min, in dark and light, from the window's accessibility tree.
# ACCOUNTS_WALK_UNTIL=age runs 5c after 5b and ends there.
quota_sheet() {  # open the Technical view's Claude Code quota sheet from the Settings menu
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
ages() {  # ages <shot>: the frame, and how many lanes say "read <age> ago"
  # Counted in the accessibility tree, exhaustively: a lane's age is the static text "read", its
  # age in bold, then "ago" (quota.js renderLanes), and nothing else on the sheet is the word
  # "read" alone. OCR read "read 9min ago" and missed a dark lane (run walk-cb80366dad73).
  "$T/shot.sh" "$VM" "$S/$1.png" >/dev/null 2>&1 || fail "capture $1"
  ax find --value 'read' --role AXStaticText --json > "$S/$1.read.json" 2>/dev/null || true
  grep -c '"role"' "$S/$1.read.json" || true
}
if [ "${ACCOUNTS_WALK_UNTIL:-}" = age ]; then
  note "5c the lanes' age: both current first"
  theme Dark
  quota_sheet "5c fresh" || true
  quota_refresh "5c Refresh, both accounts read now"
  sleep 6
  fresh_t=$(date +%s)
  n=$(ages 5c-age-fresh-dark)
  note "5c fresh: $n lane(s) say 'read N min ago'"
  [ "$n" = 0 ] || fail "5c a current reading is shown with an age ($n)"
  ax tree > "$S/5c-age-fresh-dark.tree" 2>&1 || true
  "$T/guest.sh" "$VM" "printf '{\"null\":true}' > /Users/admin/fill-first/usage-1.json; printf '{\"null\":true}' > \"$DATA/claude-accounts/2/usage.json\""  || fail "5c the null-answer fixtures" # machine-path-exempt: isolated guest or negative fixture input; no storage default
  quota_refresh "5c Refresh, both answer null"
  sleep 6
  quota_close
  left=$(( 330 - ($(date +%s) - fresh_t) ))
  note "5c waiting ${left}s, until both readings are 5.5 min old"
  [ "$left" -gt 0 ] && sleep "$left"
  for t in Dark Light; do
    if [ "$t" = Light ]; then lower=light; else lower=dark; fi
    theme "$t"
    quota_sheet "5c aged ($lower)" || continue
    quota_refresh "5c Refresh ($lower), null again"
    sleep 6
    n=$(ages "5c-age-$lower")
    note "5c aged ($lower): $n lane(s) say 'read N min ago'"
    [ "$n" = 2 ] || fail "5c both lanes should show their age ($lower: $n)"
    ax tree > "$S/5c-age-$lower.tree" 2>&1 || true
    quota_close
  done
  "$T/guest.sh" "$VM" "cat \"$DATA/engine-state/claude-quota.json\"" > "$S/5c-quota-record.txt" 2>&1 || true
  if [ "${#FAILS[@]}" -gt 0 ]; then
    note "done at 5c, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
    exit 1
  fi
  note "done at 5c (ACCOUNTS_WALK_UNTIL=age)"
  exit 0
fi
if [ "${ACCOUNTS_WALK_UNTIL:-}" = d13 ]; then
  if [ "${#FAILS[@]}" -gt 0 ]; then
    note "done at D13, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
    exit 1
  fi
  note "done at D13 (ACCOUNTS_WALK_UNTIL=d13)"
  exit 0
fi
menu open
ax click --title 'Technical view' --first >/dev/null 2>&1 || note "Technical view not switched back off"
sleep 3
if ax find --title 'Turn it off' --first >/dev/null 2>&1; then press 'Turn it off' "5b Turn it off"; sleep 3; fi
menu closed

note "6 Home's week reaches 99%: Rich switches, and says why"
usage /Users/admin/fill-first/usage-1.json 20 99 || fail "Home's 99% usage fixture" # machine-path-exempt: isolated test VM account/path; never a customer default
send_turn 'How is the client report going?'
send_turn 'And the pairing screens?'
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/6-accounts.txt" 2>&1 || fail "6 the accounts record"
grep -q '"inUse": *"2"' "$S/6-accounts.txt" || fail "6 claude-accounts.json does not record Work in use"
has 'I switched the team to your' "6 Rich's line"
has 'See your accounts' "6 See your accounts under it"
shot 6-switched-chat

note "7 the sheet: the banner says when and why; Recent changes"
press 'See your accounts' "7 See your accounts"
sleep 3
has 'Rich switched to Work at' "7 the banner"
has 'Why: Home had used 99% of its weekly limit' "7 the banner's why"
has 'Rich switched to' "7 Recent changes"
shot 7-switched-dark
close_sheet; theme Light; open_sheet || fail "7 the sheet in light"; shot 7-switched-light
close_sheet; theme Dark

note "8 Settings: which account, and when it switched"
menu open
has 'Using Work, switched' "8 the row's line after a switch"
shot 8-menu-two
menu closed

note "9 item 8: he chooses an account at 97% of its week, and Rich switches again at 99%"
# A week's usage only goes up. Home, switched away from at 99% in step 6, rests until its week
# resets (ui/accounts.js `resting`), so a later 97% reading of Home cannot happen and is never
# offered. His case is an account at 97% that nobody switched away from: a third one, Side,
# added from the sheet (account 3: claude_accounts.rs `add` numbers it after the highest).
open_sheet || fail "9 the sheet, to add a third account"
press 'Add another account' "9 Add another account"
sleep 3
field 'The new account' 'Side' || fail "9 the third account's name"
press 'Continue' "9 Continue"
sleep 2
has 'Sign in to Side' "9 step 2 for Side"
login_as 'side@fixture.invalid' || fail "9 the login-as fixture"
press 'Open sign-in' "9 Open sign-in"
wait_for 'is ready' || true
# "<b>Side</b> is ready." is two text nodes, so the words after the bold name are read.
has 'is ready. Rich switches to it when Work is nearly full' "9 Side is ready"
# Its folder exists once it is signed in; its week is then read at 97% (it was used elsewhere).
usage "$DATA/claude-accounts/3/usage.json" 20 97 || fail "Side's 97% usage fixture"
"$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/9-accounts-added.txt" 2>&1 || fail "9 the accounts record"
grep -q '"label": *"Side"' "$S/9-accounts-added.txt" || fail "9 Side is not on the record"
close_sheet
# A turn reads the quota, so the sheet is opened after one, and again after up to two more,
# until Side's card offers the choice.
chosen=0
for try in 1 2 3; do
  send_turn "Quick check $try on the weekly figures, please."
  open_sheet || { note "no sheet on try $try"; continue; }
  if ax find --title 'Use Side now' --first >/dev/null 2>&1; then chosen=1; break; fi
  note "Side's card offers no Use this one now yet (try $try)"
  close_sheet
done
if [ "$chosen" = 1 ]; then
  shot 9-chose-side
  press 'Use Side now' "9 Use this one now on Side"
  sleep 4
  "$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/9-accounts-chosen.txt" 2>&1 || fail "9 the accounts record"
  grep -q '"inUse": *"3"' "$S/9-accounts-chosen.txt" || fail "9 the record does not say Side is in use after Use this one now"
  grep -q '"kind": *"chose"' "$S/9-accounts-chosen.txt" || fail "9 Recent changes has no chose row"
  # "You chose <b>Side</b>." is two text nodes, so the words are read up to the bold name
  # (walk-434edf5caf3d: the frame showed the row and a find of the whole sentence missed it).
  has 'You chose' "9 Recent changes says You chose Side"
  shot 9-side-in-use
  close_sheet
  usage "$DATA/claude-accounts/3/usage.json" 20 99 || fail "Side's 99% usage fixture"
  # Home still rests at 99%, so Rich's next account is Work (12%), which steps 10 and 11 read.
  switched=0
  for try in 1 2 3; do
    send_turn "Status update $try on the client report?"
    "$T/guest.sh" "$VM" "cat \"$DATA/claude-accounts.json\"" > "$S/9-accounts-after.txt" 2>&1 || true
    if grep -q '"inUse": *"2"' "$S/9-accounts-after.txt"; then switched=1; break; fi
  done
  [ "$switched" = 1 ] || fail "9 Side reached 99% of its week and Rich did not switch to Work"
  shot 9-switched-again
else
  fail "9 Side at 97% never offered Use this one now"
fi
theme Dark

note "10 _02: with Technical view on, the quick settings quota row is the WEEKLY figure"
menu open
ax click --title 'Technical view' --first >/dev/null 2>&1 || note "no Technical view switch"
sleep 3
# Turning it on asks where ("Turn on the technical view": all conversations, this company, this
# one); its own default and "Turn it on" (walk-434edf5caf3d: left open, it blocked every later read).
if ax find --title 'Turn it on' --first >/dev/null 2>&1; then
  press 'Turn it on' "10 Turn it on"
  sleep 3
fi
menu open
has 'Claude Code quota' "10 the quota row with Technical view on"
has 'Work 12% weekly' "10 the row reads Work's weekly 12%, not its five-hour 0%"
ax find --title 'Claude Code quota' --contains --json > "$S/10-row.json" 2>&1 || true
shot 10-menu-technical
menu open
ax click --title 'Technical view' --first >/dev/null 2>&1 || note "Technical view not switched back off"
sleep 3
if ax find --title 'Turn it off' --first >/dev/null 2>&1; then
  press 'Turn it off' "10 Turn it off"
  sleep 3
fi
menu closed

note "11 the smallest window, 1024x700: the row's lines fit and can be read"
APP_PID=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/app.pid" 2>/dev/null || true)
ax --windows > "$S/11-windows-before.txt" 2>&1 || true
ax "tell application \"System Events\" to tell (first process whose unix id is $APP_PID) to set size of window 1 to {1024, 700}" >/dev/null 2>&1 || fail "11 resizing the window"
sleep 2
ax --windows > "$S/11-windows-1024.txt" 2>&1 || true
grep -q '1024' "$S/11-windows-1024.txt" || fail "11 the window is not 1024 wide"
for t in Light Dark; do
  if [ "$t" = Light ]; then lower=light; else lower=dark; fi
  theme "$t"
  menu open
  has 'Using Work, switched' "11 the row's line at 1024x700 ($lower)"
  ax find --title 'Claude accounts' --role AXButton --contains --json > "$S/11-row-$lower.json" 2>&1 || true
  shot "11-menu-1024-$lower"
  menu closed
done

if [ "${#FAILS[@]}" -gt 0 ]; then
  note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
  exit 1
fi
note "done"
exit 0
