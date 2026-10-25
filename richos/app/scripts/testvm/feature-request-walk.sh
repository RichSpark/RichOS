#!/bin/bash
# feature-request-walk.sh — Request a feature on the real app, in one guest run, against a local
# stand-in for RichOS's reporting relay.
#
#   run-walk.py --bundle <RichOS.app.zip> --home <empty dir> --engine <engine.tar.gz> \
#     --report <json> -- feature-request-walk.sh <out-dir>
#
# The CEO, 2026-10-11 (richos-hq docs/ceo-input/2026-10-11_01/request.md), built to round 24:
# "Under the "Bust a bug" button, I want to add a "Request a feature" button. But I don't want
# feature requests to be submitted or branded as "issues" on GitHub." The same exchange as Bust a
# bug (bug-report-walk.sh, whose fixture this copies): the guest's `claude` is
# fake-claude-bug-report.py (Rich's write-up of the idea names the walk's company and a person),
# the relay's test override (RICHDOCTOR_REPORT_RELAY and RICHDOCTOR_REPORT_REPOSITORY) points at
# report-relay-stand-in.py on 127.0.0.1:8765. No token is anywhere: the relay posts the idea.
#
# PASS when:
#   1  Request a feature, directly under Bust a bug in Settings, starts the exchange: Rich asks
#      what the user would like RichOS to do;
#   2  the answer comes back as the Feature idea card, "Not posted yet", To RichOS's Discussions
#      under Ideas, with the company and the person replaced by stand-ins, nothing has reached
#      the stand-in, and no word on screen says issue;
#   3  Post idea posts it: the card says "Posted · in Ideas", Rich says it is in RichOS's
#      Discussions under Ideas; the relay received one registration as richos-idea/1 and one
#      completion, with no Authorization header, nothing went to GitHub's GraphQL or an issues
#      path, the real relay's own checks (checks.mjs, on the host) take what was registered, and
#      neither the company nor the person is in it;
#   4  screenshots of the card and the posted card in the dark theme and the light theme, for
#      comparison with round 24.
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the
# clone when this returns (CEO §54). EXIT STATUS: 0 only when every capture was saved and every
# check passed.
set -u
VM="$1"
S="${2:?usage: feature-request-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
G=/Users/admin/fill-first
PORT=8765
COMPANY='Northwind Traders'
PERSON='Dana Whitfield'
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
setup_failed() { note "FAILED: $1; nothing to walk"; exit 1; }
finish() {
  "$T/guest.sh" "$VM" "cat $G/stand-in.log 2>/dev/null; true" > "$S/stand-in.log" 2>&1
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
  # after: walk-166ae6c12266's dark and light captures were both light, with the menu left open
  # over the card.
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
check() { local ok="$1" bad="$2"; shift 2; if "$@"; then note "ok: $ok"; else fail "$bad"; fi; }
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
# Press a button by its title: the first match, up to three tries, then the pointer at its center.
# A press that searches the whole tree, or by a partial title, can outlast ax.sh's 20 s deadline on
# a busy host: the first run of this walk (walk-aa3d555a63df) found "Request a feature" on screen
# and then lost its press to that deadline. bug-report-walk.sh step 5 met the same and does this.
click_first() {
  local title="$1" at
  for _ in 1 2 3; do "$T/ax.sh" "$VM" click --title "$title" --first && return 0; sleep 2; done
  at=$("$T/ax.sh" "$VM" find --title "$title" --first 2>/dev/null | sed -n "s/.* pos=\([0-9.-]*\) \([0-9.-]*\) size=\([0-9.]*\) \([0-9.]*\).*/\1 \2 \3 \4/p" | head -1)
  [ -n "$at" ] || return 1
  # shellcheck disable=SC2086  # four numbers, split on purpose
  set -- $at
  note "pressing '$title' failed; clicking it at its center ($at) instead"
  "$T/ax.sh" "$VM" click --at "$(python3 -c 'import sys; x,y,w,h=map(float,sys.argv[1:]); print("%d,%d" % (x+w/2, y+h/2))' "$1" "$2" "$3" "$4")"
}

# ---- the fixture: the fake claude and the relay stand-in ----
"$T/guest.sh" "$VM" "mkdir -p $G" || setup_failed "the fixture folder"
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" $G/claude-fill-first || setup_failed "pushing the fill-first fake"
"$T/guest.sh" "$VM" --push "$T/fake-claude-bug-report.py" $G/claude || setup_failed "pushing the write-up fake"
"$T/guest.sh" "$VM" --push "$T/report-relay-stand-in.py" $G/relay-stand-in.py || setup_failed "pushing the stand-in"
"$T/guest.sh" "$VM" "chmod 755 $G/claude $G/claude-fill-first; printf '{\"five\":10,\"weekly\":20}' > $G/usage-1.json; printf 'Noted.' > $G/reply.txt" \
  || setup_failed "the fixture files"
PAYLOAD=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/payload")
case "$PAYLOAD" in /Users/admin/testvm/*) ;; *) setup_failed "the guest payload ($PAYLOAD)" ;; esac
"$T/guest.sh" "$VM" "nohup python3 $G/relay-stand-in.py $PORT $G/relay.json $G/stand-in.log 0 413 >/dev/null 2>&1 & echo \$! > $G/stand-in.pid; echo started" \
  || setup_failed "starting the stand-in"
python3 - "$VM" "$T" "$PORT" <<'PY' || setup_failed "relaunching the app with the fake claude and the stand-in endpoint"
import sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment={'RICHOS_CLAUDE_BIN': '/Users/admin/fill-first/claude',
                                         'RICHDOCTOR_REPORT_RELAY': 'http://127.0.0.1:' + sys.argv[3],
                                         'RICHDOCTOR_REPORT_REPOSITORY': 'richos-test/report-relay-local'}))
PY

# ---- a company and a conversation, as a first run makes them (bug-report-walk.sh's steps) ----
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
answered=no
for attempt in 1 2 3; do
  "$T/ax.sh" "$VM" type "Good morning." --role AXTextArea --first --replace || true
  "$T/ax.sh" "$VM" click --title 'Send' --first || true
  if wait_text 'Noted.' 12; then answered=yes; break; fi
  note "the first message got no answer yet (attempt $attempt)"
done
[ "$answered" = yes ] || setup_failed "the conversation never answered (no 'Noted.')"
note "ok: a conversation in $COMPANY"

# ---- 1. Request a feature starts the exchange ----
"$T/ax.sh" "$VM" click --title 'Settings' --first || fail "the Settings button"
sleep 1
check "Request a feature is in Settings" "no Request a feature in Settings" wait_text 'Request a feature' 3
"$T/shot.sh" "$VM" "$S/1-settings-dark.png" || fail "capture 1-settings-dark"
click_first 'Request a feature' || fail "the Request a feature button"
check "Rich asks what the user would like" "Rich did not ask" wait_text 'What would you like RichOS to do?' 10

# ---- 2. the answer, Rich's write-up, the card ----
# Into the Rich panel's own answer box (`#bugdock-input`), named by its id: the first text area
# on screen is the conversation's composer, and an answer typed there is a message to Rich
# (walk-2160ec387035 typed the idea into it and no card ever came).
"$T/ax.sh" "$VM" type "It would be great if Rich sent me a short summary of each conversation every Friday afternoon, like what changed with $COMPANY this week." --role AXTextArea --id bugdock-input --replace || fail "typing the idea"
"$T/ax.sh" "$VM" --key 36 || fail "Return"
check "the idea card, not posted yet" "no idea card" wait_text 'Not posted yet' 40
check "the card says Feature idea" "no Feature idea head" wait_text 'Feature idea' 3
check "To RichOS's Discussions" "no Discussions in the To line" wait_text 'as a post in its Discussions' 3
check "the company is a stand-in" "no [a company] stand-in on the card" wait_text '[a company]' 3
check "the person is a stand-in" "no [a person] stand-in on the card" wait_text '[a person]' 3
refuse "the person's name is on the card" wait_text "$PERSON" 1
refuse "a word on screen says issue" wait_text 'issue' 1
check "Post idea is offered" "no Post idea button" wait_text 'Post idea' 3
refuse "something reached the stand-in before Post idea" "$T/guest.sh" "$VM" "test -s $G/stand-in.log"
"$T/guest.sh" "$VM" "cat $G/writer-prompts.log" > "$S/writer-prompts.log" 2>&1 || true
check "Rich was asked for an idea" "Rich was not asked for an idea" has 'pressed \\"Request a feature\\"' "$S/writer-prompts.log"
sleep 3
both 2-idea

# ---- 3. Post idea: a discussion under Ideas, never an issue ----
# By the button's own id (`#bug-send`), which a search reaches without walking the whole card.
"$T/ax.sh" "$VM" click --role AXButton --id bug-send --first || click_first 'Post idea' || fail "Post idea"
check "Posted · in Ideas" "the card never said Posted · in Ideas" wait_text 'Posted · in Ideas' 20
check "Rich says where it went" "Rich did not say where it went" wait_text "in RichOS's Discussions on GitHub, under Ideas" 5
refuse "a word on screen says issue" wait_text 'issue' 1
"$T/guest.sh" "$VM" "cat $G/stand-in.log" > "$S/stand-in.log" 2>&1 || true
posts=$(grep -c '"path": "/v1/reports/RD-[^"]*/complete"' "$S/stand-in.log" || true)
check "one completion at the relay" "$posts completions at the relay, not 1" test "$posts" = 1
check "registered as an idea" "not registered as richos-idea/1" grep -q 'richos-idea/1' "$S/stand-in.log"
refuse "a request went to GitHub's GraphQL or an issues path" grep -q -e '"path": "/graphql"' -e '"path": "/repos/' "$S/stand-in.log"
refuse "a token went to the relay" grep -qi '"authorization"' "$S/stand-in.log"
check "the real relay's checks take what the app sent" "the real relay's checks refuse what the app sent" real_relay_takes "$S/stand-in.log" richos-idea/1
refuse "a private name reached the stand-in" grep -q -e "$COMPANY" -e "$PERSON" -e Northwind -e Dana "$S/stand-in.log"
check "the title Rich wrote" "the title is not Rich's" has 'A short weekly summary of each conversation' "$S/stand-in.log"
sleep 2
both 3-posted
finish
