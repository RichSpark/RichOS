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
gs() { "$T/guest.sh" "$VM" "$@"; }

PAYLOAD=$(cat "${TESTVM_ROOT:-$HOME/.richos-testvm}/run/$VM/payload")
case "$PAYLOAD" in /Users/admin/testvm/*) ;; *) note "FAILED: the guest payload ($PAYLOAD)"; exit 1 ;; esac

finish() {
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

# Make it the first RichDoctor: version 1.0.0, same signing (ad hoc), quit.
gs "pkill -x RichDoctor 2>/dev/null; sleep 2; /usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 1.0.0' '$RD/Contents/Info.plist'; /usr/libexec/PlistBuddy -c 'Set :CFBundleVersion 1.0.0' '$RD/Contents/Info.plist'; codesign --force --sign - --timestamp=none '$RD'" >> "$S/downgrade.log" 2>&1
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
finish
