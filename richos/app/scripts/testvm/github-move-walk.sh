#!/bin/bash
# github-move-walk.sh — the GitHub move (CEO, 2026-10-27) on the real app, in one guest run.
#
#   run-walk.py --bundle <zip> --home <empty dir> --engine <engine.tar.gz> --report <json> -- \
#     github-move-walk.sh <out-dir> <relay URL> <relay repository>
#
# The CEO is moving RichOS from WebDevBooster/richos to RichCore/RichOS and wants it done as if
# RichOS had active users. A copy of RichOS asks the new update address first and the old one
# second (src-tauri/src/updates.rs, first_answer), and shows a sent report with the link the
# relay answered on whichever repository it filed it. This walk shows both on the real app.
#
# THE BUNDLE is branch-bundle.sh's, built with the shipping order of the two addresses pointed at
# a server this walk starts in the guest:
#   TAURI_CONFIG='{"plugins":{"updater":{"endpoints":[
#     "http://127.0.0.1:8974/RichCore/RichOS/releases/latest/download/latest.json",
#     "http://127.0.0.1:8974/WebDevBooster/richos/releases/latest/download/latest.json"]}}}'
# (a debug build: plain http to the guest's own loopback is allowed there and nowhere else).
#
# THE RELAY is a TEST relay and its test repository (never production): the app's own test
# override, RICHDOCTOR_REPORT_RELAY and RICHDOCTOR_REPORT_REPOSITORY. Rich's write-up comes from
# fake-claude-bug-report.py, as in bug-report-walk.sh. The walk sends exactly one report.
#
# PASS when:
#   1  BEFORE THE MOVE (only the old address has a manifest): the app's own update check
#      (RICHOS_UPDATE_SELFTEST=check runs the same updates::check the Check button runs) finds
#      the old address's version, reports the old address as the one in force, and the server
#      saw the new address asked first (404) and then the old one;
#   2  AFTER THE MOVE (both have one): it finds the new address's version from the new address,
#      and the server saw no request for the old one;
#   3  the app relaunched as a person opens it: the start screen's update notice offers the new
#      address's version (the launch check, through the same two addresses), captured;
#   4  one Bust a bug report through the relay: the card says "Sent · #n" and Rich names
#      github.com/<relay repository>/issues/n, the link the relay answered with, captured.
#
# run-walk.py passes the VM name first, and quits the app, stops the guest and deletes the clone
# when this returns (CEO §54). EXIT STATUS: 0 only when every check passed.
set -u
VM="$1"
S="${2:?usage: github-move-walk.sh <vm> <out-dir> <relay URL> <relay repository>}"
RELAY="${3:?the relay URL}"
REPOSITORY="${4:?the relay repository, owner/name}"
T="$(cd "$(dirname "$0")" && pwd)"
# The fakes read their fixture files here (fake-claude-fill-first.pl), so the walk keeps its own there too.
G=/Users/admin/fill-first
PORT=8974
COMPANY='Northwind Traders'
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
setup_failed() { note "FAILED: $1; nothing to walk"; finish_now; exit 1; }
finish_now() {
  "$T/guest.sh" "$VM" "cat $G/server.log 2>/dev/null; true" > "$S/server.log" 2>&1
  # The server this walk started, by the PID it recorded at its start, never by its name.
  "$T/guest.sh" "$VM" "test -s $G/server.pid && kill \$(cat $G/server.pid) 2>/dev/null; true" >/dev/null 2>&1
}
finish() {
  finish_now
  if [ "${#FAILS[@]}" -gt 0 ]; then
    note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"
    exit 1
  fi
  note "done"
  exit 0
}
check() { local ok="$1" bad="$2"; shift 2; if "$@"; then note "ok: $ok"; else fail "$bad"; fi; }
# shellcheck disable=SC2329  # called through check, which shellcheck cannot follow
has() { grep -q -- "$1" "$2"; }
# shellcheck disable=SC2329  # called through check, which shellcheck cannot follow
lacks() { ! grep -q -- "$1" "$2"; }
# shellcheck disable=SC2329  # called through check, which shellcheck cannot follow
wait_text() { # Wait (at most $2 x 3 s) until a node's value or title contains $1.
  for _ in $(seq 1 "${2:-30}"); do
    "$T/ax.sh" "$VM" find --value "$1" --contains --first >/dev/null 2>&1 && return 0
    "$T/ax.sh" "$VM" find --title "$1" --contains --first >/dev/null 2>&1 && return 0
    sleep 3
  done
  return 1
}
relaunch() { # $1 JSON environment; prints the launch's log path in the guest
  python3 - "$VM" "$T" "$1" <<'PY'
import json, sys
sys.path.insert(0, sys.argv[2])
from relaunch import relaunch
print(relaunch(sys.argv[1], environment=json.loads(sys.argv[3]))['log'])
PY
}
manifest() { # $1 the address's path under the server, $2 the version it offers
  local dir="$G/serve/$1/releases/latest/download"
  "$T/guest.sh" "$VM" "mkdir -p $dir && printf '%s' '{\"version\":\"$2\",\"notes\":\"$1\",\"pub_date\":\"2026-10-27T00:00:00Z\",\"platforms\":{\"darwin-aarch64\":{\"signature\":\"not checked by a check\",\"url\":\"http://127.0.0.1:$PORT/$1/releases/download/v$2/RichOS.app.tar.gz\"}}}' > $dir/latest.json"
}
selftest() { # $1 name: one update check by the app itself; its lines into $S/$1.txt
  local log
  log=$(relaunch '{"RICHOS_UPDATE_SELFTEST": "check"}') || { fail "$1: the app did not run its update check"; return; }
  # relaunch answers as soon as the process exists; the check's last line says it has ended.
  for _ in $(seq 1 60); do
    "$T/guest.sh" "$VM" "grep -q 'RICHOS-UPDATE-SELFTEST exit=' $log" >/dev/null 2>&1 && break
    sleep 1
  done
  "$T/guest.sh" "$VM" "grep '^RICHOS-UPDATE-SELFTEST' $log; true" > "$S/$1.txt" 2>&1
  "$T/guest.sh" "$VM" "cat $G/server.log" > "$S/$1-server.log" 2>&1 || true
  sed 's/^/    /' "$S/$1.txt" | tee -a "$S/walk.log"
}

# ---- the guest's update server: the two addresses, under one loopback port ----
[ "$("$T/guest.sh" "$VM" 'uname -m')" = arm64 ] || setup_failed "the guest is not Apple silicon (the manifest names darwin-aarch64)"
"$T/guest.sh" "$VM" "mkdir -p $G/serve" || setup_failed "the fixture folder"
manifest WebDevBooster/richos 98.0.0 || setup_failed "the old address's manifest"
"$T/guest.sh" "$VM" "cd $G/serve && nohup python3 -m http.server $PORT --bind 127.0.0.1 > $G/server.log 2>&1 & echo \$! > $G/server.pid" \
  || setup_failed "starting the update server"
for _ in $(seq 1 20); do
  "$T/guest.sh" "$VM" "curl -fsS -o /dev/null http://127.0.0.1:$PORT/WebDevBooster/richos/releases/latest/download/latest.json" >/dev/null 2>&1 && break
  sleep 0.5
done

# ---- 1. before the move: the new address has nothing, the old one answers ----
"$T/guest.sh" "$VM" ": > $G/server.log" || true
selftest 1-before-the-move
check "before the move: the update is found (98.0.0)" "before the move: no update found" has 'state=available .*available=98.0.0' "$S/1-before-the-move.txt"
check "before the move: from the old address" "before the move: not from the old address" has "endpoint=http://127.0.0.1:$PORT/WebDevBooster/richos/" "$S/1-before-the-move.txt"
check "before the move: the new address was asked first and had nothing" "the new address was not asked, or answered" has '"GET /RichCore/RichOS/releases/latest/download/latest.json HTTP/1.1" 404' "$S/1-before-the-move-server.log"
check "before the move: then the old one answered" "the old address was not asked" has '"GET /WebDevBooster/richos/releases/latest/download/latest.json HTTP/1.1" 200' "$S/1-before-the-move-server.log"

# ---- 2. after the move: the new address answers, the old one is not asked ----
manifest RichCore/RichOS 99.0.0 || fail "the new address's manifest"
"$T/guest.sh" "$VM" ": > $G/server.log" || true
selftest 2-after-the-move
check "after the move: the new address's update is found (99.0.0)" "after the move: not the new address's update" has 'state=available .*available=99.0.0' "$S/2-after-the-move.txt"
check "after the move: from the new address" "after the move: not from the new address" has "endpoint=http://127.0.0.1:$PORT/RichCore/RichOS/" "$S/2-after-the-move.txt"
check "after the move: the old address was not asked" "after the move: the old address was asked" lacks 'GET /WebDevBooster/' "$S/2-after-the-move-server.log"

# ---- 3. one report through the relay ----
"$T/guest.sh" "$VM" --push "$T/fake-claude-fill-first.pl" $G/claude-fill-first || setup_failed "pushing the fill-first fake"
"$T/guest.sh" "$VM" --push "$T/fake-claude-bug-report.py" $G/claude || setup_failed "pushing the bug report fake"
"$T/guest.sh" "$VM" "chmod 755 $G/claude $G/claude-fill-first; printf '{\"five\":10,\"weekly\":20}' > $G/usage-1.json; printf 'Noted.' > $G/reply.txt" \
  || setup_failed "the fixture files"
relaunch "{\"RICHOS_CLAUDE_BIN\": \"$G/claude\", \"RICHDOCTOR_REPORT_RELAY\": \"$RELAY\", \"RICHDOCTOR_REPORT_REPOSITORY\": \"$REPOSITORY\"}" >/dev/null \
  || setup_failed "relaunching the app with the fake claude and the relay"
# The start screen. Its update notice comes from the app's own check at launch, against the
# server above, which now has both addresses: it offers the new address's version.
check "the start screen offers the new address's 99.0.0" "no update notice for 99.0.0 on the start screen" wait_text 'RichOS 99.0.0 is available' 20
"$T/shot.sh" "$VM" "$S/3-start-screen.png" || fail "capture 3-start-screen"
# Its door into the conversation, as output-walk.py presses it.
check "the start screen's Talk to Rich" "no Talk to Rich on the start screen" wait_text 'Talk to Rich' 10
"$T/ax.sh" "$VM" click --title 'Talk to Rich' --first >/dev/null 2>&1 || fail "pressing Talk to Rich"
sleep 4
# A first-run question over the conversation keeps Settings from being pressed: put it off.
for _ in 1 2 3; do "$T/ax.sh" "$VM" click --title 'Not now' --first >/dev/null 2>&1 || break; sleep 3; done

"$T/ax.sh" "$VM" click --title 'Settings' --first || fail "the Settings button"
sleep 1
"$T/ax.sh" "$VM" click --title 'Bust a bug!' --contains || fail "the Bust a bug button"
check "Rich asks what went wrong" "Rich did not ask" wait_text 'What went wrong? Tell me in your own words' 10
"$T/ax.sh" "$VM" type "In the $COMPANY chat the names on the left get cut off when I make the text bigger." --role AXTextArea --id bugdock-input --replace || fail "typing the answer"
"$T/ax.sh" "$VM" --key 36 || fail "Return"
check "the report card, not sent yet" "no report card" wait_text 'Not sent yet' 40
"$T/ax.sh" "$VM" click --title 'Send report' || fail "Send report"
check "the card says Sent" "the card never said Sent" wait_text 'Sent · #' 30
check "Rich names the relay's link on $REPOSITORY" "Rich did not name a link on $REPOSITORY" wait_text "github.com/$REPOSITORY/issues/" 5
"$T/ax.sh" "$VM" find --value "github.com/$REPOSITORY/issues/" --contains --first > "$S/3-link.txt" 2>&1 || true
"$T/ax.sh" "$VM" find --value "Sent · #" --contains --first > "$S/3-pill.txt" 2>&1 || true
# Captured where it is: switching the theme goes through Settings and closes the panel the card
# is in (the 2026-10-27 run's theme captures showed the conversation behind it).
"$T/shot.sh" "$VM" "$S/3-sent.png" || fail "capture 3-sent"
finish
