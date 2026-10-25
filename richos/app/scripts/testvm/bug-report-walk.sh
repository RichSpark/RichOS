#!/bin/bash
# bug-report-walk.sh — Bust a bug on the real app, in one guest run, against a local stand-in
# for RichOS's reporting relay.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- bug-report-walk.sh <out-dir>
#
# The CEO's §115 (2026-10-09), built to round 21: the user says what is wrong, Rich writes it
# up, the user reads the whole report and sends it, and it is filed as a GitHub issue; a failed
# send is kept on the Mac and sent once the connection is back. This walk drives exactly that,
# with the guest's own `claude` replaced by fake-claude-bug-report.py (Rich's write-up names
# the walk's company and a person) and the relay's test override (RICHDOCTOR_REPORT_RELAY and
# RICHDOCTOR_REPORT_REPOSITORY) pointed at report-relay-stand-in.py on 127.0.0.1:8765, whose test
# repository is richos-test/report-relay-local. No token is anywhere: the relay files the report.
#
# PASS when:
#   1  Bust a bug (Settings) starts the exchange in the Rich panel (`#bugdock`, where every press
#      opens it since fe4cd6b14 and 8e454cff1, 2026-10-10): Rich asks what went wrong;
#   2  the answer typed in the panel's own answer box (`#bugdock-input`, never the conversation's
#      composer) comes back as the report card, "Not sent yet", with the
#      company, the person and A FILE PATH WITH SPACES IN IT (~/Library/Application Support/
#      Client Plans/budget.xlsx) replaced by stand-ins, and nothing has reached the endpoint;
#      Rich was given what was on the screen: its visible words (the conversation's "Noted.")
#      and a JPEG picture of the window, taken with no permission prompt, and the digest says
#      "Looked at the screen you were on" (second review rv-20261009T102303Z-1c3dda1d-4c78,
#      findings 1 and 6);
#   3  Send with the endpoint down leaves the card "Waiting to send · saved on this Mac", with
#      Rich's offline line, and the report on disk under bug-reports/waiting;
#   4  once the endpoint is up, the report goes out BY ITSELF: the card says "Sent · #412",
#      Rich says "You're back online, so I sent your bug report.", the relay received one
#      registration and one completion, with no Authorization header, the real relay's own
#      checks (checks.mjs, on the host) take what was registered, and its body carries neither
#      the company nor the person; the issue is richos-test/report-relay-local/issues/412;
#   5  CANCEL WHILE A SEND IS IN FLIGHT (the same review, finding 2): a second report waits
#      (endpoint down), the endpoint comes back holding its answer 15 s, and Cancel report is
#      pressed while the retry's request is in flight. The card says "Canceling…", then
#      "Sent · #413", and Rich says "It had already gone out before I could cancel it."; never
#      "Canceled · nothing sent" and never "Nothing was sent";
#   6  screenshots of the report, the waiting card, the sent card and the in-flight cancel in the
#      dark theme and the light theme, for comparison with round 21.
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the
# clone when this returns (CEO §54). EXIT STATUS: 0 only when every capture was saved and
# every check passed.
set -u
VM="$1"
S="${2:?usage: bug-report-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
G=/Users/admin/fill-first
PORT=8765
COMPANY='Northwind Traders'
PERSON='Dana Whitfield'
# A path as a Mac writes it: a folder macOS names with a space, then one the user named.
# shellcheck disable=SC2088  # the tilde is the user's typed words, meant literally, never expanded
SPACED_PATH='~/Library/Application Support/Client Plans/budget.xlsx'
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
finish() {
  "$T/guest.sh" "$VM" "cat $G/stand-in.log 2>/dev/null; true" > "$S/stand-in.log" 2>&1
  "$T/guest.sh" "$VM" "cat $G/stand-in-2.log 2>/dev/null; true" > "$S/stand-in-2.log" 2>&1
  "$T/guest.sh" "$VM" "cat $G/writer-prompts.log 2>/dev/null; true" > "$S/writer-prompts.log" 2>&1
  # The stand-in this walk started, by the PID it recorded at its start, never by its name.
  "$T/guest.sh" "$VM" "test -s $G/stand-in.pid && kill \$(cat $G/stand-in.pid) 2>/dev/null; true" >/dev/null 2>&1
  if [ "${#FAILS[@]}" -gt 0 ]; then
    note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
    exit 1
  fi
  note "done"
  exit 0
}
# Wait (at most $2 x 3 s, default 90 s) until the app's screen has a node whose value or title
# contains $1.
wait_text() {
  for _ in $(seq 1 "${2:-30}"); do
    "$T/ax.sh" "$VM" find --value "$1" --contains --first >/dev/null 2>&1 && return 0
    "$T/ax.sh" "$VM" find --title "$1" --contains --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  return 1
}
theme() { # dark | light, through the Settings button's own theme row, then the menu closed
  # Each press stops at the first match and is tried up to three times, and Escape closes the menu
  # after (feature-request-walk.sh's theme): with the report in the Rich panel, walk-28517e6e980a's
  # 5-canceled-in-flight-light capture was dark, with the Settings menu left open over the card.
  local name="Dark theme"; [ "$1" = light ] && name="Light theme"
  "$T/ax.sh" "$VM" click --title "Settings" --first >/dev/null 2>&1 || true
  sleep 1
  for _ in 1 2 3; do "$T/ax.sh" "$VM" click --title "$name" --first >/dev/null 2>&1 && break; sleep 2; done
  sleep 1
  "$T/ax.sh" "$VM" --key 53 >/dev/null 2>&1 || true
  sleep 2
}
both() { # $1 name: one capture in each theme, dark first, and back to dark
  theme dark; "$T/shot.sh" "$VM" "$S/$1-dark.png" || fail "capture $1-dark"
  theme light; "$T/shot.sh" "$VM" "$S/$1-light.png" || fail "capture $1-light"
  theme dark
}

# ---- the fixture: the fake claude and the relay stand-in ----
"$T/guest.sh" "$VM" "mkdir -p $G" || setup_failed "the fixture folder"
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" $G/claude-fill-first || setup_failed "pushing the fill-first fake"
"$T/guest.sh" "$VM" --push "$T/fake-claude-bug-report.py" $G/claude || setup_failed "pushing the bug report fake"
"$T/guest.sh" "$VM" --push "$T/report-relay-stand-in.py" $G/relay-stand-in.py || setup_failed "pushing the stand-in"
"$T/guest.sh" "$VM" "chmod 755 $G/claude $G/claude-fill-first; printf '{\"five\":10,\"weekly\":20}' > $G/usage-1.json; printf 'Noted.' > $G/reply.txt" \
  || setup_failed "the fixture files"
PAYLOAD=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/payload")
# run.sh's payload is /Users/admin/testvm/<vm> (measured on walk-85648710ca8f).
case "$PAYLOAD" in /Users/admin/testvm/*) ;; *) setup_failed "the guest payload ($PAYLOAD)" ;; esac
python3 - "$VM" "$T" "$PORT" <<'PY' || setup_failed "relaunching the app with the fake claude and the stand-in endpoint"
import sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment={'RICHOS_CLAUDE_BIN': '/Users/admin/fill-first/claude',
                                         'RICHDOCTOR_REPORT_RELAY': 'http://127.0.0.1:' + sys.argv[3],
                                         'RICHDOCTOR_REPORT_REPOSITORY': 'richos-test/report-relay-local'}))
PY

# ---- a company and a conversation, as a first run makes them ----
wait_text 'Not now' 20 || true
for _ in 1 2 3; do
  "$T/ax.sh" "$VM" find --title 'Add this company' --first >/dev/null 2>&1 && break
  "$T/ax.sh" "$VM" click --title 'Not now' --first >/dev/null 2>&1 || true
  sleep 4
done
wait_text 'Add this company' 20 || true
for attempt in 1 2 3; do
  "$T/ax.sh" "$VM" type "$COMPANY" --role AXTextField --first --replace || true
  sleep 1
  "$T/ax.sh" "$VM" click --title 'Add this company' || true
  sleep 6
  "$T/ax.sh" "$VM" find --title 'Add this company' --first >/dev/null 2>&1 || break
  note "company not added yet (attempt $attempt)"
done
# Up to three tries: on a busy host the first type can land before the company's conversation
# is on screen (walk-330999851782 sat on the greeting with an empty composer).
answered=no
for attempt in 1 2 3; do
  "$T/ax.sh" "$VM" type "Good morning." --role AXTextArea --first --replace || true
  "$T/ax.sh" "$VM" click --title 'Send' --first || true
  if wait_text 'Noted.' 12; then answered=yes; break; fi
  note "the first message got no answer yet (attempt $attempt)"
done
[ "$answered" = yes ] || setup_failed "the conversation never answered (no 'Noted.')"
note "ok: a conversation in $COMPANY"

# One check: run the command; "ok: $1" when it succeeds, a failure named $2 when it does not.
check() { local ok="$1" bad="$2"; shift 2; if "$@"; then note "ok: $ok"; else fail "$bad"; fi; }
# The reverse: a failure named $1 when the command succeeds.
refuse() { local bad="$1"; shift; if "$@"; then fail "$bad"; fi; }
# shellcheck disable=SC2329  # called through check and refuse, which shellcheck cannot follow
has() { grep -q -- "$1" "$2"; }
# What the app registered, through the REAL relay's own checks (richos/mobile/service/report/
# checks.mjs: the schema, the fingerprint and the content rules, with the footer), on this host.
# shellcheck disable=SC2329  # called through check, which shellcheck cannot follow
real_relay_takes() { # $1 the stand-in's request log, $2 the schema expected
  node --input-type=module - "$1" "$2" "$T/../../../mobile/service/report/checks.mjs" <<'JS'
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const [log, schema, checks] = process.argv.slice(2);
const { checkCreate, contentProblem } = await import(pathToFileURL(checks).href);
const sent = readFileSync(log, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l)).filter(r => r.path === '/v1/reports');
if (!sent.length) { console.error('nothing was registered'); process.exit(1); }
for (const r of sent) {
  const data = JSON.parse(r.body), checked = await checkCreate(data, Date.now());
  if (checked.error) { console.error('refused:', JSON.stringify(checked.error)); process.exit(1); }
  if (data.schema !== schema) { console.error('schema', data.schema); process.exit(1); }
  const problem = contentProblem({ ...checked.value, origin: 'http://127.0.0.1:8765', count: 0 });
  if (problem) { console.error('content refused:', JSON.stringify(problem)); process.exit(1); }
}
JS
}

# ---- 1. Bust a bug starts the exchange ----
"$T/ax.sh" "$VM" click --title 'Settings' --first || fail "the Settings button"
sleep 1
"$T/ax.sh" "$VM" click --title 'Bust a bug!' --contains || fail "the Bust a bug button"
check "Rich asks what went wrong" "Rich did not ask" wait_text 'What went wrong? Tell me in your own words' 10
"$T/shot.sh" "$VM" "$S/1-ask-dark.png" || fail "capture 1-ask-dark"

# ---- 2. the answer, Rich's write-up, the card ----
# Into the Rich panel's own answer box, named by its id: the first text area on screen is the
# conversation's composer, and an answer typed there is a message to Rich (walk-a00bc5100694, on
# nightly 51: the answer went to the conversation, Rich said "Noted." and no card ever came).
"$T/ax.sh" "$VM" type "In the $COMPANY chat the names on the left get cut off when I make the text bigger. My notes in $SPACED_PATH are gone too." --role AXTextArea --id bugdock-input --replace || fail "typing the answer"
"$T/ax.sh" "$VM" --key 36 || fail "Return"
check "the report card, not sent yet" "no report card" wait_text 'Not sent yet' 40
# Asked node by node: a whole-tree read of the conversation outlasts ax.sh's 20 s deadline on a
# busy host (walk-aba5c01c19ce, exit 124).
check "the company is a stand-in" "no [a company] stand-in on the card" wait_text '[a company]' 3
check "the person is a stand-in" "no [a person] stand-in on the card" wait_text '[a person]' 3
refuse "the person's name is on the card" wait_text "$PERSON" 1
check "the path with spaces is a stand-in" "no [a file on this Mac] stand-in on the card" wait_text '[a file on this Mac]' 3
# The whole path is ONE stand-in: the line under the sheet counts one file path. (Its words are
# not searched for on screen: the user's own message above the card shows what they typed, so
# walk-5a15093aaa62 found "Client Plans" there. What leaves the Mac is checked at the endpoint.)
check "the whole path is one stand-in" "the left-out line does not count one file path" wait_text 'one file path' 3
check "From the RichOS reporting account" "no From line" wait_text 'the RichOS reporting account' 3
check "Rich says he looked at the screen" "no 'Looked at the screen you were on' over Rich's answer" wait_text 'Looked at the screen you were on' 3
refuse "something reached the endpoint before Send" "$T/guest.sh" "$VM" "test -s $G/stand-in.log"
"$T/guest.sh" "$VM" "cat $G/writer-prompts.log" > "$S/writer-prompts.log" 2>&1 || true
check "Rich was given the user's words" "Rich was not given the user's words" has 'cut off when I make the text bigger' "$S/writer-prompts.log"
# What the user was looking at: the screen's words (the conversation's own reply) and a picture.
check "Rich was given the screen's words" "the screen's words were not in Rich's prompt" has 'visible words top to bottom' "$S/writer-prompts.log"
check "the screen's words are the conversation's" "the conversation's 'Noted.' was not among them" has 'Noted\.' "$S/writer-prompts.log"
check "Rich was given a JPEG picture of the window" "no JPEG picture reached Rich" has '"media_type": "image/jpeg", "bytes": [0-9]*, "head": "ffd8ff' "$S/writer-prompts.log"
"$T/guest.sh" "$VM" --pull "$G/rich-saw.jpg" "$S/2-rich-saw.jpg" || fail "pulling the picture Rich was given"
both 2-report

# ---- 3. Send while the endpoint is down: kept on this Mac ----
"$T/ax.sh" "$VM" click --title 'Send report' || fail "Send report"
check "waiting to send, saved on this Mac" "the card never said Waiting to send" wait_text 'Waiting to send' 12
check "Rich's offline line" "no offline line" wait_text "This Mac is offline, so the report didn't go out" 5
# shellcheck disable=SC2016  # the $(...) is meant to expand in the guest's shell, not here
DATA=$("$T/guest.sh" "$VM" 'd="$(find /Users/admin -type d -name bug-reports -path "*com.richos.app*" 2>/dev/null | head -1)"; echo "$d"')
"$T/guest.sh" "$VM" "ls '$DATA/waiting'; cat '$DATA'/waiting/*.json" > "$S/3-waiting.txt" 2>&1 || true
check "on disk, waiting, offline ($DATA/waiting)" "the waiting report is not on disk" has '"reason":"offline"' "$S/3-waiting.txt"
both 3-waiting

# ---- 4. the endpoint comes up: it goes out by itself ----
"$T/guest.sh" "$VM" "nohup python3 $G/relay-stand-in.py $PORT $G/relay-1.json $G/stand-in.log 0 412 >/dev/null 2>&1 & echo \$! > $G/stand-in.pid; echo started" || fail "starting the stand-in"
note "the stand-in endpoint is up; waiting for the report to go out by itself"
check "Sent · #412, by itself" "the card never said Sent · #412" wait_text 'Sent · #412' 16
check "Rich says it went out" "Rich did not say it went out" wait_text "You're back online, so I sent your bug report" 5
"$T/guest.sh" "$VM" "cat $G/stand-in.log" > "$S/stand-in.log" 2>&1 || true
posts=$(grep -c '"path": "/v1/reports/RD-[^"]*/complete"' "$S/stand-in.log" || true)
check "one completion at the relay" "$posts completions at the relay, not 1" test "$posts" = 1
# Header names are matched in any case: the app's client sends them lower-case.
refuse "a token went to the relay" grep -qi '"authorization"' "$S/stand-in.log"
check "the real relay's checks take what the app sent" "the real relay's checks refuse what the app sent" real_relay_takes "$S/stand-in.log" richdoctor-report/2
refuse "a private name reached the endpoint" grep -q -e "$COMPANY" -e "$PERSON" -e Northwind -e Dana "$S/stand-in.log"
refuse "the path with spaces reached the endpoint" grep -q -e 'Client Plans' -e 'budget.xlsx' -e 'Application Support' "$S/stand-in.log"
check "the issue says a file was left out" "no file stand-in in the issue" grep -qF 'a file on this Mac' "$S/stand-in.log"
check "the title Rich wrote" "the title is not Rich's" has 'Conversation names in the sidebar are cut off' "$S/stand-in.log"
"$T/guest.sh" "$VM" "ls '$DATA/waiting' | wc -l; cat '$DATA/sent.jsonl'" > "$S/4-store.txt" 2>&1 || true
check "recorded as sent" "not recorded as sent" has '"number":412' "$S/4-store.txt"
check "the issue in the test repository" "no richos-test/report-relay-local issue 412 recorded" has 'github.com/richos-test/report-relay-local/issues/412' "$S/4-store.txt"
both 4-sent

# ---- 5. Cancel while a send is in flight: it went out, and Rich says so ----
# The endpoint goes down again (the stand-in this walk started, by its recorded PID).
"$T/guest.sh" "$VM" "kill \$(cat $G/stand-in.pid)" || fail "stopping the stand-in"
"$T/ax.sh" "$VM" click --title 'Settings' --first || fail "the Settings button (second report)"
sleep 1
"$T/ax.sh" "$VM" click --title 'Bust a bug!' --contains || fail "the Bust a bug button (second report)"
sleep 2
"$T/ax.sh" "$VM" type "The Send button flickers when I press it." --role AXTextArea --id bugdock-input --replace || fail "typing the second answer"
"$T/ax.sh" "$VM" --key 36 || fail "Return (second report)"
check "the second report card" "no second report card" wait_text 'Not sent yet' 40
# With two cards on screen a search of the whole tree can outlast ax.sh's deadline on a busy host
# (walk-5a15093aaa62: "Send report" was not clicked), so each click stops at the first match and
# is tried up to three times.
#
# THE PRESS ITSELF CAN FAIL WHERE THE FIND DOES NOT: walk-29408ebd443e found the second card's
# "Send report" (matches=1) and every AXPress on it answered "Can't get object", three times. So
# the last resort is the pointer: the button's own position from the find, clicked at its center.
click_first() {
  local title="$1" at
  for _ in 1 2; do "$T/ax.sh" "$VM" click --title "$title" --first && return 0; sleep 2; done
  at=$("$T/ax.sh" "$VM" find --title "$title" --first 2>/dev/null | sed -n "s/.* pos=\([0-9.-]*\) \([0-9.-]*\) size=\([0-9.]*\) \([0-9.]*\).*/\1 \2 \3 \4/p" | head -1)
  [ -n "$at" ] || return 1
  # shellcheck disable=SC2086  # four numbers, split on purpose
  set -- $at
  note "pressing '$title' failed; clicking it at its center ($at) instead"
  "$T/ax.sh" "$VM" click --at "$(python3 -c 'import sys; x,y,w,h=map(float,sys.argv[1:]); print("%d,%d" % (x+w/2, y+h/2))' "$1" "$2" "$3" "$4")"
}
# The card arrives with a 2.6 s entrance (`is-new`); step 3 pressed Send long after it ended.
sleep 4
"$T/shot.sh" "$VM" "$S/5-report-dark.png" || fail "capture 5-report-dark"
click_first 'Send report' || fail "Send report (second report)"
check "the second report waits to send" "the second report never said Waiting to send" wait_text 'Waiting to send' 12
# Where Cancel report is, read now, while nothing is racing: the click in flight must land within
# the stand-in's hold, and a find plus a press can take 20 s on a busy host.
cancel_at=$("$T/ax.sh" "$VM" find --title 'Cancel report' --first 2>/dev/null | sed -n "s/.* pos=\([0-9.-]*\) \([0-9.-]*\) size=\([0-9.]*\) \([0-9.]*\).*/\1 \2 \3 \4/p" | head -1)
# shellcheck disable=SC2086  # four numbers, split on purpose
cancel_at=$([ -n "$cancel_at" ] && python3 -c 'import sys; x,y,w,h=map(float,sys.argv[1:]); print("%d,%d" % (x+w/2, y+h/2))' $cancel_at)
note "Cancel report is at ${cancel_at:-an unknown place}"
# Back up, holding each completion 22 s (under the app's 30 s request bound): the retry loop's
# request arrives and is IN FLIGHT, holding the one-send-at-a-time lock, while Cancel is pressed.
"$T/guest.sh" "$VM" "nohup python3 $G/relay-stand-in.py $PORT $G/relay-2.json $G/stand-in-2.log 22 413 >/dev/null 2>&1 & echo \$! > $G/stand-in.pid; echo started" || fail "starting the holding stand-in"
arrived=no
for _ in $(seq 1 45); do
  if "$T/guest.sh" "$VM" "test -s $G/stand-in-2.log" >/dev/null 2>&1; then arrived=yes; break; fi
  sleep 2
done
if [ "$arrived" = yes ]; then
  note "ok: the retry's request is in flight"
  if [ -n "$cancel_at" ]; then
    "$T/ax.sh" "$VM" click --at "$cancel_at" || fail "Cancel report while the send was in flight"
  else
    click_first 'Cancel report' || fail "Cancel report while the send was in flight"
  fi
  check "the card says Canceling…" "the card never said Canceling…" wait_text 'Canceling…' 2
  "$T/shot.sh" "$VM" "$S/5-canceling-dark.png" || fail "capture 5-canceling-dark"
  check "Sent · #413: it had gone out" "the card never said Sent · #413" wait_text 'Sent · #413' 15
  check "Rich says it had already gone out" "Rich did not say it had already gone out" wait_text 'It had already gone out before I could cancel it' 5
  refuse "the card says canceled although it went out" wait_text 'Canceled · nothing sent' 1
  refuse "Rich says nothing was sent although it went out" wait_text 'Nothing was sent' 1
  "$T/guest.sh" "$VM" "cat $G/stand-in-2.log" > "$S/stand-in-2.log" 2>&1 || true
  posts=$(grep -c '"path": "/v1/reports/RD-[^"]*/complete"' "$S/stand-in-2.log" || true)
  check "the second report was filed once" "$posts POSTs for the second report, not 1" test "$posts" = 1
  "$T/guest.sh" "$VM" "ls '$DATA/waiting' | wc -l; cat '$DATA/sent.jsonl'" > "$S/5-store.txt" 2>&1 || true
  check "nothing left waiting" "a report is still waiting on disk" grep -qx -e ' *0' "$S/5-store.txt"
  check "recorded as sent, #413" "#413 is not recorded as sent" has '"number":413' "$S/5-store.txt"
  both 5-canceled-in-flight
else
  fail "the retry's request never reached the holding stand-in in 90 s"
fi
finish
