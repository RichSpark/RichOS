#!/bin/bash
# richdoctor-update-walk.sh — a newer RichOS replaces an older RichDoctor, and the version says so.
#
#   run-walk.py --bundle <RichOS.app> --home <empty dir> --engine "" --report <json> -- \
#     richdoctor-update-walk.sh <out-dir>
#
# CEO 2026-11-02: "What's the version of RichDoctor now and how do I know that it has actually
# been updated?" package-app.sh stamps RichDoctor with RichOS's own version; the refresh at launch
# replaces an installed copy only when it is older than the payload's.
#
#   U1  the RichOS under test installed RichDoctor, and its Info.plist version is RichOS's own.
#   U2  with that copy made a 1.0.0 one (version rewritten, re-signed ad hoc, quit), a RichOS
#       relaunch replaces it: the Info.plist version is RichOS's again and the executable is the
#       payload's byte for byte.
#   U3  a further relaunch with the same versions leaves it alone (Info.plist byte-identical).
#   U4  after the end no RichDoctor process is left in the guest.
#   U5  the version RichDoctor states in the Versions section of the report it prepares (an
#       installed copy's own support adapter, over a fixture home) is RichOS's own version.
#   U6  the footer of a fixture report sent to a loopback relay stand-in (never GitHub) carries
#       RichOS's version in the relay's `major.minor[.patch]` shape.
#
# run-walk.py passes the VM name first, then stops the guest and deletes the clone (CEO §54).
# EXIT STATUS: 0 only when every check passed.
set -u
VM="$1"
S="${2:?usage: richdoctor-update-walk.sh <vm> <out-dir>}"
T="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$S"
note() { echo "[walk] $(date -u +%H:%M:%SZ) $*" | tee -a "$S/walk.log"; }
FAILS=()
fail() { note "FAILED: $1"; FAILS+=("$1"); }
pass() { note "ok: $1"; }
wait_login_age() {
  LOGIN_AGE=$(gs "ps -o etime= -p \$(pgrep -x Dock | head -1) | tr -d ' ' | awk -F'[-:]' '{n=NF; s=\$n+\$(n-1)*60; if (n>=3) s+=\$(n-2)*3600; if (n>=4) s+=\$(n-3)*86400; print s}'" | tr -d '\r' | grep -E '^[0-9]+$' | tail -1)
  if [ -n "${LOGIN_AGE:-}" ] && [ "$LOGIN_AGE" -lt 130 ]; then note "waiting $((130 - LOGIN_AGE)) s so the login item's two minutes have passed"; sleep $((130 - LOGIN_AGE)); fi
}
gs() { "$T/guest.sh" "$VM" "$@"; }

PAYLOAD=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/payload")
case "$PAYLOAD" in /Users/admin/testvm/*) ;; *) note "FAILED: the guest payload ($PAYLOAD)"; exit 1 ;; esac

G=/Users/admin/richdoctor-update-walk
PORT=8766
finish() {
  gs "test -s $G/stand-in.pid && kill \$(cat $G/stand-in.pid) 2>/dev/null; true" >/dev/null 2>&1
  gs "pkill -x RichDoctor 2>/dev/null; sleep 2; pgrep -x RichDoctor >/dev/null && echo up || echo gone" > "$S/gone.txt" 2>&1
  if [ "$(tail -1 "$S/gone.txt")" = gone ]; then pass "U4 no RichDoctor process is left in the guest"; else fail "U4 a RichDoctor process is still up"; fi
  if [ "${#FAILS[@]}" -gt 0 ]; then note "done, with ${#FAILS[@]} failed step(s): $(printf '%s; ' "${FAILS[@]}")"; exit 1; fi
  note "done: every check passed"
  exit 0
}

# RichOS's own version, from the bundle under test.
RICHOS_APP=$(gs "find '$PAYLOAD' -maxdepth 1 -name '*.app' -type d -print" | head -1)
RV=$(gs "/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' '$RICHOS_APP/Contents/Info.plist'")
note "RichOS under test: $RICHOS_APP says $RV"

find_rd() { gs "find /Applications '$PAYLOAD/home/Applications' '$PAYLOAD' -maxdepth 2 -name RichDoctor.app -type d -print 2>/dev/null | head -1"; }
rd_version() { gs "/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' '$1/Contents/Info.plist'"; }

RD=""
for _ in $(seq 1 40); do RD=$(find_rd); [ -n "$RD" ] && break; sleep 3; done
if [ -z "$RD" ]; then gs "grep -i richdoctor '$PAYLOAD/app.log'; ls -la '$PAYLOAD'" > "$S/app-log.txt" 2>&1; fail "U1 RichOS installed no RichDoctor within two minutes (app-log.txt)"; finish; fi
note "installed RichDoctor: $RD"
V1=$(rd_version "$RD")
if [ "$V1" = "$RV" ]; then pass "U1 RichDoctor says $V1, RichOS's own version"; else fail "U1 RichDoctor says '$V1', RichOS says '$RV'"; fi
PAYLOAD_SUM=$(gs "shasum -a 256 '$RD/Contents/MacOS/RichDoctor'" | cut -d' ' -f1)

# ---- U7: a refresh deferred by an open window happens by itself when the window closes ---------------
# RICHDOCTOR_OLD_ZIP=<RichDoctor zip of an earlier release> makes the older copy that real build
# (re-signed ad hoc), and a report is started in it first (its stage stays Investigating when the
# window closes, so that build answers "busy" for as long as it runs: bootstrap.rs `quit_running`
# stops it by the process id on its socket once no window of it is on screen).
# RICHDOCTOR_WALK=deferred runs only U1 and this (the rest needs a RichOS built like a release).
# bootstrap.rs `keep_trying`: RichOS asks again every minute, never closing a window itself, and
# writes each outcome to ~/Library/Logs/RichOS/richdoctor-install.log. The RichOS here is started
# with a SHORT home: RichDoctor's instance socket lives in the data folder and a Unix socket path
# over about 104 bytes cannot be bound or reached, which the walk's long payload path would exceed.
if [ "${RICHDOCTOR_WALK:-all}" = deferred ]; then
  SHORT=/Users/admin/rdh7
  APPS="$SHORT/Applications"
  LOG="$SHORT/Library/Logs/RichOS/richdoctor-install.log"
  AS="sudo launchctl asuser \$(id -u admin) sudo -u admin env HOME='$SHORT'"
  state="${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM"
  # An older copy: the installed one, made 1.0.0 (ad hoc, like the payload), in the short home.
  if [ -n "${RICHDOCTOR_OLD_ZIP:-}" ]; then
    # A REAL older build (nightly 86's RichDoctor, 1.0.0, the code the CEO's installed copy runs):
    # unzipped and re-signed AD HOC, because this RichOS is ad hoc and only trusts a copy signed by
    # its own team (bootstrap.rs `installed`). The re-sign changes the signature, not the code.
    gs "mkdir -p $G; chown admin $G" >/dev/null 2>&1
    gs --push "$RICHDOCTOR_OLD_ZIP" "$G/old-richdoctor.zip" >> "$S/u7.log" 2>&1 || { fail "U7 setup: pushing the older RichDoctor zip"; finish; }
    gs "pkill -x RichDoctor 2>/dev/null; sleep 2; rm -rf '$SHORT' $G/old; mkdir -p '$APPS' '$SHORT/Library/Application Support' $G/old && ditto -x -k $G/old-richdoctor.zip $G/old && mv $G/old/RichDoctor.app '$APPS/RichDoctor.app' && codesign --force --sign - --timestamp=none '$APPS/RichDoctor.app' && chown -R admin '$SHORT'" >> "$S/u7.log" 2>&1
  else
  gs "pkill -x RichDoctor 2>/dev/null; sleep 2; rm -rf '$SHORT'; mkdir -p '$APPS' '$SHORT/Library/Application Support' && mv '$RD' '$APPS/RichDoctor.app' && /usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 1.0.0' '$APPS/RichDoctor.app/Contents/Info.plist' && /usr/libexec/PlistBuddy -c 'Set :CFBundleVersion 1.0.0' '$APPS/RichDoctor.app/Contents/Info.plist' && codesign --force --sign - --timestamp=none '$APPS/RichDoctor.app' && chown -R admin '$SHORT'" >> "$S/u7.log" 2>&1
  fi
  RD="$APPS/RichDoctor.app"
  V0=$(rd_version "$RD"); note "U7 older copy: $V0 at $RD"
  if [ "$V0" != "1.0.0" ]; then fail "U7 setup: the older copy says '$V0'"; finish; fi
  wait_login_age
  # Its window open (a person is using it): started without the menu-bar-only flag.
  gs "mkdir -p $G; chown admin $G; $AS nohup '$RD/Contents/MacOS/RichDoctor' > $G/u7-rd.log 2>&1 &" >/dev/null 2>&1
  has() { "$T/ax.sh" "$VM" find --value "$1" --contains --app RichDoctor --first >/dev/null 2>&1 || "$T/ax.sh" "$VM" find --title "$1" --contains --app RichDoctor --first >/dev/null 2>&1; }
  UP=0; for _ in $(seq 1 20); do has "What went wrong?" && { UP=1; break; }; sleep 3; done
  if [ "$UP" = 1 ]; then pass "U7 the older RichDoctor is running with its window open"; else fail "U7 the older RichDoctor's window did not open (u7-rd.txt)"; gs "tail -5 $G/u7-rd.log" > "$S/u7-rd.txt" 2>&1; finish; fi
  RDPID=$(gs "pgrep -x RichDoctor | head -1" | tr -d '\r')
  # A report is started in it (stage Investigating, saved at once): the old copy then answers
  # `quit-if-idle` "busy" for as long as it runs, even after its window is closed and the report saved.
  DATA="$SHORT/Library/Application Support/com.richos.richdoctor"
  "$T/ax.sh" "$VM" click --role AXTextArea --app RichDoctor --first >/dev/null 2>&1 || true
  "$T/ax.sh" "$VM" 'tell application "System Events" to keystroke "RichOS shows a blank window after an update. It happened twice today."' >/dev/null 2>&1
  sleep 1
  "$T/ax.sh" "$VM" click --title "Continue" --app RichDoctor --first >/dev/null 2>&1 || fail "U7 pressing Continue"
  UP=0; for _ in $(seq 1 20); do has "About when did it last happen?" && { UP=1; break; }; sleep 3; done
  if [ "$UP" = 1 ]; then pass "U7 a report was started in the older RichDoctor (it asks when it last happened)"; else fail "U7 the report did not start (u7-rd.txt)"; fi
  DRAFT=$(gs "find '$DATA' -name draft.json 2>/dev/null | head -1" | tr -d '\r')
  DRAFT_SUM=$(gs "shasum -a 256 '$DRAFT' | cut -d' ' -f1" | tr -d '\r')
  gs "grep -o '\"stage\": *\"[a-z]*\"' '$DRAFT'" > "$S/u7-draft-stage.txt" 2>&1
  note "U7 saved report: $DRAFT ($(cat "$S/u7-draft-stage.txt" | head -1))"
  # RichOS, as a relaunch (the in-app updater's relaunch starts the same executable the same way).
  python3 -I "$T/relaunch.py" "$VM" --env "HOME=$SHORT" >> "$S/relaunch.log" 2>&1 || note "relaunch reported a problem (see relaunch.log)"
  OSPID=$(cat "$state/app.pid" 2>/dev/null || echo "")
  for _ in $(seq 1 20); do gs "grep -q Deferred '$LOG' 2>/dev/null" && break; sleep 3; done
  gs "cat '$LOG' 2>&1" > "$S/u7-install-log-1.txt" 2>&1
  V1=$(rd_version "$RD")
  if [ "$V1" = "1.0.0" ] && grep -q Deferred "$S/u7-install-log-1.txt" && [ "$(gs "pgrep -x RichDoctor | head -1" | tr -d '\r')" = "$RDPID" ]; then pass "U7 with the window open nothing was replaced or closed, and the log says why ($(tail -1 "$S/u7-install-log-1.txt" | cut -c1-120))"; else fail "U7 window open: RichDoctor says '$V1' (want 1.0.0, untouched) (u7-install-log-1.txt)"; fi
  # The person minimizes the window (an answer may be half typed in it): the copy must not be
  # stopped, so the same process, and the text in its box, is still there. RichOS asks again every
  # minute: wait past two asks.
  "$T/ax.sh" "$VM" 'tell application "System Events" to tell process "RichDoctor" to click (first button of window 1 whose subrole is "AXMinimizeButton")' >/dev/null 2>&1 || fail "U7 minimizing the RichDoctor window"
  note "U7 window minimized at $(date -u +%H:%M:%SZ)"
  sleep 150
  V1M=$(rd_version "$RD")
  PIDM=$(gs "pgrep -x RichDoctor | head -1" | tr -d '\r')
  gs "cat '$LOG' 2>&1" > "$S/u7-install-log-min.txt" 2>&1
  "$T/ax.sh" "$VM" 'tell application "System Events" to tell process "RichDoctor" to set value of attribute "AXMinimized" of window 1 to false' >/dev/null 2>&1 || note "U7 un-minimize reported a problem"
  sleep 2
  if [ "$V1M" = "1.0.0" ] && [ "$PIDM" = "$RDPID" ] && ! grep -q Refreshed "$S/u7-install-log-min.txt"; then pass "U7 minimized: still 1.0.0, same RichDoctor pid $RDPID, not stopped"; else fail "U7 minimized: RichDoctor says '$V1M' pid '$PIDM' (want 1.0.0, $RDPID) (u7-install-log-min.txt)"; fi
  # The person closes the window; no RichOS restart.
  "$T/ax.sh" "$VM" 'tell application "System Events" to tell process "RichDoctor" to click (first button of window 1 whose subrole is "AXCloseButton")' >/dev/null 2>&1 || fail "U7 closing the RichDoctor window"
  note "U7 window closed at $(date -u +%H:%M:%SZ); RichOS pid $OSPID"
  V2="$V1"; for _ in $(seq 1 50); do V2=$(rd_version "$RD" 2>/dev/null || echo "?"); [ "$V2" = "$RV" ] && break; sleep 3; done
  sleep 5
  gs "cat '$LOG' 2>&1" > "$S/u7-install-log-2.txt" 2>&1
  ALIVE=$(gs "kill -0 $OSPID 2>/dev/null && echo alive || echo gone" | tr -d '\r')
  NEWPID=$(gs "pgrep -x RichDoctor | head -1" | tr -d '\r')
  SUM7=$(gs "shasum -a 256 '$RD/Contents/MacOS/RichDoctor'" | cut -d' ' -f1)
  if [ "$V2" = "$RV" ] && [ "$ALIVE" = alive ] && grep -q Refreshed "$S/u7-install-log-2.txt" && [ "$SUM7" = "$PAYLOAD_SUM" ]; then
    DRAFT_AFTER=$(gs "shasum -a 256 '$DRAFT' | cut -d' ' -f1" | tr -d '\r')
    if [ "$DRAFT_AFTER" = "$DRAFT_SUM" ]; then pass "U7 the saved report is byte-identical after the replacement"; else fail "U7 the saved report changed ($DRAFT_SUM -> $DRAFT_AFTER)"; fi
    pass "U7 after the window closed RichDoctor was replaced by itself: 1.0.0 -> $V2 (executable is the payload's), RichOS pid $OSPID kept running, log: $(tail -1 "$S/u7-install-log-2.txt" | cut -c1-140); RichDoctor pid $RDPID -> ${NEWPID:-none}"
  else
    fail "U7 after the window closed: RichDoctor says '$V2' (want $RV), RichOS $ALIVE, sha $SUM7 (want $PAYLOAD_SUM) (u7-install-log-2.txt)"
  fi
  gs "kill $OSPID 2>/dev/null; true" >/dev/null 2>&1
  finish
fi

# Make it the first RichDoctor: version 1.0.0, same signing (ad hoc), quit.
if [ -n "${RICHDOCTOR_OLD_ZIP:-}" ]; then
  # A RELEASE RichOS only trusts an installed copy signed by its own team (bootstrap.rs `installed`),
  # so the ad-hoc re-sign below is never replaced under one. Give it a genuinely signed older copy
  # instead: RICHDOCTOR_OLD_ZIP is the RichDoctor zip of an earlier release of the same team
  # (a real older copy, such as the 1.0.0 users have).
  gs "mkdir -p $G" >/dev/null 2>&1
  gs --push "$RICHDOCTOR_OLD_ZIP" "$G/old-richdoctor.zip" >> "$S/downgrade.log" 2>&1 || { fail "U2 setup: pushing the older RichDoctor zip"; finish; }
  gs "pkill -x RichDoctor 2>/dev/null; sleep 2; rm -rf '$RD' $G/old && mkdir -p $G/old && ditto -x -k $G/old-richdoctor.zip $G/old && mv $G/old/RichDoctor.app '$RD'" >> "$S/downgrade.log" 2>&1
else
  gs "pkill -x RichDoctor 2>/dev/null; sleep 2; /usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 1.0.0' '$RD/Contents/Info.plist'; /usr/libexec/PlistBuddy -c 'Set :CFBundleVersion 1.0.0' '$RD/Contents/Info.plist'; codesign --force --sign - --timestamp=none '$RD'" >> "$S/downgrade.log" 2>&1
fi
V0=$(rd_version "$RD")
note "made it a first copy: $V0"
if [ "$V0" != "1.0.0" ]; then fail "U2 setup: could not make the installed copy 1.0.0 (says $V0)"; finish; fi

python3 -I "$T/relaunch.py" "$VM" >> "$S/relaunch.log" 2>&1 || note "relaunch reported a problem (see relaunch.log)"
V2="$V0"
for _ in $(seq 1 60); do V2=$(rd_version "$RD" 2>/dev/null || echo "?"); [ "$V2" != "1.0.0" ] && break; sleep 3; done
SUM2=$(gs "shasum -a 256 '$RD/Contents/MacOS/RichDoctor'" | cut -d' ' -f1)
if [ "$V2" = "$RV" ] && [ "$SUM2" = "$PAYLOAD_SUM" ]; then pass "U2 the 1.0.0 copy was replaced: $V0 -> $V2, executable is the payload's ($SUM2)"; else fail "U2 after relaunch RichDoctor says '$V2' (want $RV), executable sha256 $SUM2 (want $PAYLOAD_SUM)"; fi

BEFORE=$(gs "shasum -a 256 '$RD/Contents/Info.plist'" | cut -d' ' -f1)
gs "pkill -x RichDoctor 2>/dev/null; true" >/dev/null 2>&1
sleep 2
python3 -I "$T/relaunch.py" "$VM" >> "$S/relaunch.log" 2>&1 || true
sleep 20
AFTER=$(gs "shasum -a 256 '$RD/Contents/Info.plist'" | cut -d' ' -f1)
V3=$(rd_version "$RD")
if [ "$BEFORE" = "$AFTER" ] && [ "$V3" = "$RV" ]; then pass "U3 a relaunch at the same version left RichDoctor alone ($V3)"; else fail "U3 the same-version relaunch changed RichDoctor ($BEFORE -> $AFTER, says $V3)"; fi
# ---- U5: the Versions section of a report the installed copy prepares -----------------------------
GHOME="$PAYLOAD/home"
AS="sudo launchctl asuser \$(id -u admin) sudo -u admin env HOME='$GHOME'"
gs "pkill -x RichDoctor 2>/dev/null; mkdir -p $G/draft '$GHOME/Library/Logs/RichOS' && chown -R admin $G" >/dev/null 2>&1 || true
cat > "$S/adapter-probe.py" <<'PY'
import json, subprocess, sys
exe, draft, home = sys.argv[1:4]
json.dump({"home": home, "window": {"start": 0, "end": 1, "known": False}, "diagnostics": True}, open(draft + "/scope.json", "w"))
msgs = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "collect_diagnostics", "arguments": {"category": "installation"}}}]
p = subprocess.run([exe, "--support-adapter", draft], input="\n".join(json.dumps(m) for m in msgs) + "\n", capture_output=True, text=True, timeout=60)
out = [json.loads(l) for l in p.stdout.splitlines() if l.strip()]
server = out[0]["result"]["serverInfo"]["version"]
items = json.loads(out[1]["result"]["content"][0]["text"])["items"]
ev = items[0]["id"]
p2 = subprocess.run([exe, "--support-adapter", draft], input=json.dumps({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read_evidence", "arguments": {"id": ev}}}) + "\n", capture_output=True, text=True, timeout=60)
text = json.loads(json.loads(p2.stdout.splitlines()[0])["result"]["content"][0]["text"])["text"]
print(json.dumps({"server": server, "versions_first_line": text.split("\n")[0]}))
PY
gs --push "$S/adapter-probe.py" "$G/adapter-probe.py" || fail "U5 pushing the probe"
gs "$AS python3 $G/adapter-probe.py '$RD/Contents/MacOS/RichDoctor' $G/draft '$GHOME'" > "$S/adapter-probe.json" 2>&1
STATED=$(python3 -I -c 'import json,sys; print(json.loads(open(sys.argv[1]).read().strip().splitlines()[-1])["versions_first_line"])' "$S/adapter-probe.json" 2>/dev/null || echo "unreadable")
if [ "$STATED" = "RichDoctor $RV" ]; then pass "U5 the report's Versions section says '$STATED'"; else fail "U5 the Versions section says '$STATED', RichOS says '$RV' (adapter-probe.json)"; fi

# ---- U6: the footer of a report sent to the loopback stand-in ---------------------------------------
FOOT=$(python3 -I -c 'import sys; p=sys.argv[1].replace("-",".").replace("+",".").split(".")[:3]; p=p[:2] if len(p)==3 and p[2]=="0" else p; print(".".join(p))' "$RV")
python3 -I - "$S" "$PORT" <<'PY' || { fail "U6 writing the review fixture"; finish; }
import json, sys
out, port = sys.argv[1:3]
ms = 1793300000000
d = {"id": "d-6666aaaa6666", "created_at": ms - 60000, "updated_at": ms, "stage": "review", "description": "FIXTURE version report", "without_ai": True,
     "conversation": [{"kind": "person", "text": "FIXTURE version report"}], "left": "draft, saved to finish later", "screenshots": [],
     "report": {"report_id": "RD-6666-6666", "title": "FIXTURE version report", "what_happened": "FIXTURE version report", "expected": "It is filed.",
                "when": "Just now", "steps": [], "versions": "RichDoctor fixture", "excerpts": [], "screenshots": [],
                "relay": f"http://127.0.0.1:{port}", "causes": "", "missing": [], "provenance": "without-ai", "removals": [], "shortened": []}}
json.dump(d, open(f"{out}/d-6666aaaa6666.json", "w"), indent=2)
PY
# A short home of its own: RichDoctor's instance socket lives in its data folder and a Unix
# socket path over about 104 bytes is refused ("path must be shorter than SUN_LEN"), which the
# walk's long payload path would exceed.
SHORT=/Users/admin/rdh
AS="sudo launchctl asuser \$(id -u admin) sudo -u admin env HOME='$SHORT'"
DATA="$SHORT/Library/Application Support/com.richos.richdoctor"
gs "mkdir -p '$SHORT/Library/Application Support' && chown -R admin '$SHORT'" >/dev/null 2>&1 || true
gs --push "$T/report-relay-stand-in.py" "$G/relay-stand-in.py" || fail "U6 pushing the relay stand-in"
gs "mkdir -p '$DATA/drafts/d-6666aaaa6666'" >/dev/null 2>&1
gs --push "$S/d-6666aaaa6666.json" "$DATA/drafts/d-6666aaaa6666/draft.json" || fail "U6 pushing the fixture draft"
gs "chown -R admin '$SHORT'" >/dev/null 2>&1 || true
gs "nohup python3 $G/relay-stand-in.py $PORT $G/relay.json $G/stand-in.log 0 500 >/dev/null 2>&1 & echo \$! > $G/stand-in.pid; echo started" >/dev/null || fail "U6 starting the stand-in"
sleep 2
# A RichDoctor trusted by RichOS has its login item enabled, and one started within two minutes of the
# person's login is the login item's: it shows only its menu bar item, no window (mac.rs
# `launched_at_login`, by design). A fresh guest is always inside those two minutes, so wait them out
# (the Dock's age stands for the login's) before starting the copy this check needs a window from.
wait_login_age
gs "$AS RICHDOCTOR_REPORT_RELAY=http://127.0.0.1:$PORT RICHDOCTOR_REPORT_REPOSITORY=richos-test/report-relay-local nohup '$RD/Contents/MacOS/RichDoctor' > $G/app.log 2>&1 &" >/dev/null 2>&1
has() { "$T/ax.sh" "$VM" find --value "$1" --contains --app RichDoctor --first >/dev/null 2>&1 || "$T/ax.sh" "$VM" find --title "$1" --contains --app RichDoctor --first >/dev/null 2>&1; }
wait_text() { for _ in $(seq 1 "${2:-30}"); do has "$1" && return 0; sleep 3; done; return 1; }
wait_text "Continue your unfinished report?" 20 || { fail "U6 no card for the fixture report (u6-screen.txt)"; gs "tail -5 $G/app.log; echo '--- processes'; ps -axo pid,ppid,etime,command | grep -i '[R]ichDoctor\\|[R]ichOS'; echo '--- data folder'; ls -laR '$DATA' | head -40; echo '--- signature'; codesign -dv '$RD' 2>&1 | head -8" > "$S/u6-app-log.txt" 2>&1; "$T/ax.sh" "$VM" tree --app RichDoctor > "$S/u6-screen.txt" 2>&1; "$T/shot.sh" "$VM" "$S/u6-fail.png" >/dev/null 2>&1; }
"$T/ax.sh" "$VM" click --title "Continue" --app RichDoctor --first >/dev/null 2>&1 || fail "U6 pressing Continue"
wait_text "Send report" 15 || fail "U6 the review screen did not open"
"$T/ax.sh" "$VM" click --title "Send report" --app RichDoctor --first >/dev/null 2>&1 || fail "U6 pressing Send report"
for _ in $(seq 1 20); do gs "grep -q '/v1/reports' $G/stand-in.log 2>/dev/null" && break; sleep 3; done
gs "cat $G/stand-in.log" > "$S/stand-in.log" 2>&1 || true
LAST=$(python3 -I - "$S/stand-in.log" <<'PY'
import json, sys
last = "none"
for line in open(sys.argv[1]):
    try: r = json.loads(line)
    except Exception: continue
    if r.get("path") == "/v1/reports" and r.get("method") == "POST":
        last = json.loads(r["body"]).get("body", "").split("\n")[-1]
print(last)
PY
)
if [ "$LAST" = "Sent with RichDoctor $FOOT · Report ID RD-6666-6666" ]; then pass "U6 the footer says '$LAST'"; else fail "U6 the footer says '$LAST', wanted version $FOOT (stand-in.log)"; fi
gs "$AS osascript -e 'tell application id \"com.richos.richdoctor\" to quit'" >/dev/null 2>&1 || true
sleep 3
finish
