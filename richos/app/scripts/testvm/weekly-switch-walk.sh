#!/bin/bash
# weekly-switch-walk.sh — a weekly account switch while a helper runs, on the real app, fake
# Claude, in one guest run.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- weekly-switch-walk.sh <out-dir>
#
# The CEO's §153 (2026-10-31): the weekly switch is handled exactly as the five-hour one. No
# helper is told to stop, commit or hand off; running work carries on, and Stu moves once nothing
# of his runs. Account 1 weekly 50, Account 2 (Work) weekly 20, RICHOS_TEST_WEEKLY_CUTOFF=1:51,
# automatic pause on. The front desk writes a job down; the back end launches one helper, which
# keeps stepping through the app's gate after the back end's turn ends. Account 1's week is then
# written as 51: the switch to Work is decided and Account 1 is being left (`leaving`).
#   PASS: every step of the helper after that, on Account 1, is admitted (calls.log
#   "gate work-agent-1 exit 0"): no order, no refusal, no hold; while it runs no back-end session
#   opens on Work (Stu is not moved under it, though the app's 30 s idle check passes); once it
#   finishes on its own (work-agents-finish), a back-end session opens on Work and Stu is told
#   `account_moved` there. Nothing says handoff, no handoff marker exists, no work record says
#   failed.
#
# The helper has to be a back-end helper of a job the front desk wrote down: only those are
# Stu's. The fake writes the job down through the app's own register (`register`) and launches
# the helper on the back end (`work-agents`); fake-claude-fill-first.pl's header says how.
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the clone
# when this returns (CEO §54). In <out-dir>: 1-panel, 2-two-accounts, 3-job-running,
# 4-switch-helper-running, 5-after-move (screenshots), calls.log, journal.txt (the evidence
# journals), records-at-switch.txt and records.txt (the work records, the accounts, the quota
# file and the policy).
# EXIT STATUS: 1 when the setup fails or any capture or check fails (named at the end); 0 only
# when every capture was saved and every check read what it should.
set -u
VM="$1"
S="${2:?usage: weekly-switch-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
G=/Users/admin/fill-first # machine-path-exempt: isolated guest fixture folder; no customer default
JOB='Start the Northwind job and keep Mark on it.'
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
wait_for() {
  for _ in $(seq 1 30); do
    "$T/ax.sh" "$VM" find --title "$1" --first >/dev/null 2>&1 && return 0
    sleep 2
  done
  note "never appeared: $1"
  return 1
}
# Wait (at most $2 x 5 s, default 2 min) until the guest's calls.log has a line matching $1
# (extended regex).
wait_calls() {
  for _ in $(seq 1 "${2:-24}"); do
    "$T/guest.sh" "$VM" "grep -Eq '$1' $G/calls.log" >/dev/null 2>&1 && return 0
    sleep 5
  done
  return 1
}
# The quota panel is the Settings menu's "Claude Code quota" row (settings-button.js
# set-quota-open). A refused press of the Settings button is retried as a click on its center.
panel() {
  local try
  for try in 1 2; do
    if ! "$T/ax.sh" "$VM" click --id set-quota-open >/dev/null 2>&1; then
      "$T/ax.sh" "$VM" --key 53 >/dev/null 2>&1 || true
      sleep 1
      "$T/ax.sh" "$VM" click --title 'Settings' --role AXPopUpButton || "$T/ax.sh" "$VM" click --at 1505,131 || true
      sleep 2
      "$T/ax.sh" "$VM" click --id set-quota-open || true
    fi
    sleep 3
    "$T/ax.sh" "$VM" find --id quota-refresh >/dev/null 2>&1 && return 0
    note "the quota panel did not open (try $try)"
  done
  "$T/shot.sh" "$VM" "$S/panel-not-open-$(date -u +%H%M%S).png" || true
  return 1
}
close_panel() { "$T/ax.sh" "$VM" --key 53 || true; sleep 2; }
refresh() { "$T/ax.sh" "$VM" click --id quota-refresh || true; }
# The work records (engine-state/assignments), anything under engine-state/handoffs (there must
# be nothing), the published quota file, the accounts file and the automatic pause policy.
records() {
  "$T/guest.sh" "$VM" "find \"$DATA/engine-state/assignments\" \"$DATA/engine-state/handoffs\" -type f -name '*.json' 2>/dev/null | sort | while IFS= read -r f; do echo \"== \$f\"; cat \"\$f\"; echo; done; echo '== claude-quota.json'; cat \"$DATA/engine-state/claude-quota.json\" 2>&1; echo; echo '== claude-accounts.json'; cat \"$DATA/claude-accounts.json\" 2>&1; echo; echo '== claude-quota-policy.json'; cat \"$DATA/claude-quota-policy.json\" 2>&1; echo; true" > "$1" 2>&1
}
collect() {
  "$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/calls.log" 2>&1 || fail "calls.log"
  "$T/guest.sh" "$VM" "find \"$DATA/engine-state/evidence\" -type f -name callbacks.jsonl 2>/dev/null | sort | while IFS= read -r f; do echo \"== \$f\"; cat \"\$f\"; done" > "$S/journal.txt" 2>&1 \
    || fail "the evidence journals"
  records "$S/records.txt" || fail "the app's records"
}
# A step everything after it depends on failed: save what there is and end now.
stop_early() {
  fail "$1; nothing after it can pass"
  collect
  note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
  exit 1
}
guest_now() { "$T/guest.sh" "$VM" 'date +%s'; }

"$T/guest.sh" "$VM" "mkdir -p $G" || setup_failed "the fixture folder"
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" $G/claude || setup_failed "pushing the fake claude"
"$T/guest.sh" "$VM" "chmod 755 $G/claude; rm -f $G/work-agents-finish; printf '{\"five\":10,\"weekly\":50}' > $G/usage-1.json; touch $G/log-turns; printf 'Mark\n' > $G/work-agents; printf '%s\n' '$JOB' > $G/register; printf 'Carrying on.' > $G/reply.txt" \
  || setup_failed "the first usage fixture"
python3 - "$VM" "$T" <<'PY' || setup_failed "relaunching the app with the fake claude and the cut-off"
import sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment={'RICHOS_CLAUDE_BIN': '/Users/admin/fill-first/claude',  # machine-path-exempt: isolated guest fixture
                                          'RICHOS_TEST_WEEKLY_CUTOFF': '1:51'}))
PY

wait_for 'Not now' || true
# A first run asks for the setup essentials and then the memory folder; each says Not now.
first_run_not_now() {
  for _ in 1 2 3; do
    "$T/ax.sh" "$VM" find --title 'Add this company' --first >/dev/null 2>&1 && return 0
    "$T/ax.sh" "$VM" click --title 'Not now' --first >/dev/null 2>&1 || true
    sleep 4
  done
}
first_run_not_now
wait_for 'Add this company' || true
sleep 2
for attempt in 1 2 3; do
  "$T/ax.sh" "$VM" type 'Northwind Traders' --role AXTextField --first --replace || true
  sleep 1
  "$T/ax.sh" "$VM" click --title 'Add this company' || true
  sleep 6
  "$T/ax.sh" "$VM" find --title 'Add this company' --first >/dev/null 2>&1 || break
  note "company not added yet (attempt $attempt)"
done

note "technical view on"
"$T/ax.sh" "$VM" click --title 'Settings' --role AXPopUpButton || true
sleep 2
"$T/ax.sh" "$VM" click --at 1492,284 || true
sleep 2
"$T/ax.sh" "$VM" click --title 'Turn it on' || true
sleep 2

note "open the quota panel"
panel
sleep 2
"$T/shot.sh" "$VM" "$S/1-panel.png" || fail "capture 1-panel"

# shellcheck disable=SC2016  # the $(...) is meant to expand in the guest's shell, not here
DATA=$("$T/guest.sh" "$VM" 'd="$(find /Users/admin/testvm -type d -name engine-state 2>/dev/null | head -1)"; [ -n "$d" ] && dirname "$d"') # machine-path-exempt: isolated guest
[ -n "$DATA" ] || setup_failed "the app's data folder (no engine-state in the guest)"
note "app data: $DATA"
# The panel's own controls, by their DOM ids (ui/quota.js).
note "Add account: Home now, Work new"
"$T/ax.sh" "$VM" click --id quota-account-start || true
sleep 2
"$T/ax.sh" "$VM" type 'Home' --id quota-account-current --replace || true
"$T/ax.sh" "$VM" type 'Work' --id quota-account-label --replace || true
"$T/ax.sh" "$VM" click --id quota-account-add || true
added=""
for _ in $(seq 1 15); do
  "$T/guest.sh" "$VM" "test -d \"$DATA/claude-accounts/2\"" >/dev/null 2>&1 && { added=1; break; }
  sleep 2
done
[ -n "$added" ] || setup_failed "the second account (no $DATA/claude-accounts/2)"
"$T/guest.sh" "$VM" "printf '{\"five\":10,\"weekly\":20}' > \"$DATA/claude-accounts/2/usage.json\"" \
  || fail "Account 2's usage fixture (weekly 20)"
sleep 4
refresh
sleep 6
# Automatic pause on, so the weekly hold stands on Account 1 once it is being left: a helper
# there that was judged on it would be held, and this walk would see it.
note "automatic pause on"
"$T/ax.sh" "$VM" click --id quota-enabled || true
on=""
for _ in $(seq 1 10); do
  "$T/guest.sh" "$VM" "grep -q '\"enabled\": *true' \"$DATA/claude-quota-policy.json\"" >/dev/null 2>&1 && { on=1; break; }
  sleep 2
done
if [ -n "$on" ]; then
  note "ok: automatic pause is on (claude-quota-policy.json)"
else
  fail "automatic pause did not turn on (claude-quota-policy.json)"
fi
"$T/shot.sh" "$VM" "$S/2-two-accounts.png" || fail "capture 2-two-accounts"

note "close the panel; ask for the job"
close_panel
"$T/ax.sh" "$VM" type "$JOB" --role AXTextArea --first --replace || true
"$T/ax.sh" "$VM" click --title 'Send' --first || true
if wait_calls 'register ok'; then
  note "ok: the front desk wrote the job down"
else
  stop_early "the front desk never wrote the job down (no 'register ok' in calls.log)"
fi
if wait_calls 'gate work-agent-1 exit 0'; then
  note "ok: the back end's helper is running and stepping through the gate"
else
  stop_early "the back end's helper never stepped through the gate admitted"
fi
"$T/shot.sh" "$VM" "$S/3-job-running.png" || fail "capture 3-job-running"

note "fixture: Account 1's week reaches 51 (its cut-off)"
"$T/guest.sh" "$VM" "printf '{\"five\":10,\"weekly\":51}' > $G/usage-1.json" || fail "the 51% usage fixture"
panel
refresh
sleep 8
close_panel

switched=""
for _ in $(seq 1 24); do
  if "$T/guest.sh" "$VM" "grep -Eq '\"leaving\": *\\[ *\"1\" *\\]' \"$DATA/engine-state/claude-quota.json\" && grep -Eq '\"inUse\": *\"2\"' \"$DATA/claude-accounts.json\"" >/dev/null 2>&1; then
    switched=1; break
  fi
  sleep 5
done
[ -n "$switched" ] || stop_early "the weekly switch was never decided (no leaving [\"1\"] with Work in use)"
AT=$(guest_now)
note "ok: Account 1 is being left for Work (guest clock $AT)"
records "$S/records-at-switch.txt" || fail "the records at the switch"

# 45 s: past the app's 30 s idle check (work_host.rs ACCOUNT_CHECK_EVERY), which moves Stu as soon
# as nothing of his runs. At one step every 2 s that is about 22 helper steps.
note "the helper carries on through the switch"
sleep 45
"$T/guest.sh" "$VM" "cat $G/calls.log" > "$S/calls-running.log" 2>&1 || fail "calls.log while the helper runs"
steps=$(awk -v at="$AT" '$1 >= at && $3 == "gate" && $4 == "work-agent-1"' "$S/calls-running.log")
admitted=$(printf '%s\n' "$steps" | awk '$6 == "0"' | grep -c . || true)
other=$(printf '%s\n' "$steps" | awk 'NF && $6 != "0"' | grep -c . || true)
on_one=$(printf '%s\n' "$steps" | awk 'NF && $2 ~ /claude-accounts\/2/' | grep -c . || true)
if [ "$admitted" -ge 3 ] && [ "$other" = 0 ]; then
  note "ok: $admitted steps of the helper after the switch, every one admitted"
else
  fail "the helper after the switch: $admitted admitted, $other refused or held (want 3 or more admitted and none else)"
fi
[ "$on_one" = 0 ] || fail "$on_one helper steps ran under Work's folder: the helper was not on Account 1"
if awk -v at="$AT" '$1 >= at && $2 ~ /claude-accounts\/2/ && /richos_work/' "$S/calls-running.log" | grep -q .; then
  fail "a back-end session opened on Work while the helper still ran (Stu moved under running work)"
else
  note "ok: no back-end session on Work while the helper runs"
fi
"$T/shot.sh" "$VM" "$S/4-switch-helper-running.png" || fail "capture 4-switch-helper-running"

note "the helper finishes on its own"
"$T/guest.sh" "$VM" "touch $G/work-agents-finish" || fail "the finish fixture"
if wait_calls 'gate work-agent-1 exit 0 finished'; then
  note "ok: the helper finished with an admitted step"
else
  stop_early "the helper never finished (no 'gate work-agent-1 exit 0 finished')"
fi
FIN=$(guest_now)

note "Stu moves to Work and is told so there"
if wait_calls 'claude-accounts/2 .*richos_work' 36; then
  note "ok: a back-end session opened on Work"
else
  fail "no back-end session opened on Work after the helper finished"
fi
if wait_calls 'claude-accounts/2 turn .*account_moved' 36; then
  note "ok: Stu was told account_moved on Work"
else
  fail "no turn on Work told Stu account_moved"
fi
sleep 5
"$T/shot.sh" "$VM" "$S/5-after-move.png" || fail "capture 5-after-move"

collect

if awk -v fin="$FIN" '$2 ~ /claude-accounts\/2/ && /richos_work/ {print $1; exit}' "$S/calls.log" | awk -v fin="$FIN" '{exit ($1 + 2 >= fin) ? 0 : 1}'; then
  note "ok: the back end moved to Work only after the helper finished"
else
  fail "the back end's first session on Work started before the helper finished"
fi
if grep -Eqi 'Stop the task now|RichOS handoff|hand off' "$S/calls.log"; then
  fail "calls.log carries a stop order or a handoff"
else
  note "ok: nothing in calls.log says stop or handoff"
fi
if grep -q '^== .*/engine-state/handoffs/' "$S/records.txt"; then
  fail "a handoff marker exists (engine-state/handoffs)"
fi
if grep '"hook_event_name":"SubagentStop"' "$S/journal.txt" | grep -q '"agent_id":"work-agent-1"'; then
  note "ok: the helper ended (SubagentStop for work-agent-1)"
else
  fail "no SubagentStop for work-agent-1 in the evidence journals"
fi
# Never said failed: a work record (engine-state/assignments/<partition>/<id>.json, assignment.rs
# AssignmentState kebab-case) whose state is failed, at the switch or now.
work_states() { awk '/^== / {w = ($0 ~ /\/engine-state\/assignments\//)} w' "$@" | grep -o '"state":"[a-z-]*"'; }
if ! grep -q '^== .*/engine-state/assignments/' "$S/records.txt"; then
  fail "no work record was written (the job never reached the register)"
elif work_states "$S/records-at-switch.txt" "$S/records.txt" | grep -q '"state":"failed"'; then
  fail "a work record says failed"
else
  note "ok: no work record says failed (at the switch: $(work_states "$S/records-at-switch.txt" | tr '\n' ' '); now: $(work_states "$S/records.txt" | tr '\n' ' '))"
fi

if [ "${#FAILS[@]}" -gt 0 ]; then
  note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
  exit 1
fi
note "done"
exit 0
