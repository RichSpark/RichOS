#!/bin/bash
# Verify RichGlance scrolling capture from selection to PNG in a disposable macOS guest.
#
#   glance-scroll-walk.sh <RichFiles.app> <out-dir> [down|right|both|manual] [copy|save] [opaque|grid|sticky|inset]
#
# The opaque header and footer page and the plain two-axis grid require a complete PNG, checked
# pixel by pixel. The translucent header and inset bar pages must show the retry, with no Copy or
# Save action. This also checks that retry opens a new selection. The test checks local timing
# records, quits both guest apps and removes the muted disposable clone even after a failure.
set -euo pipefail
T="$(cd "$(dirname "$0")" && pwd)"

if [ "${1:-}" != --guest ]; then
    APP="${1:?usage: glance-scroll-walk.sh <RichFiles.app> <out-dir> [down|right|both|manual] [copy|save] [opaque|grid|sticky|inset]}"
    OUT="${2:?usage: glance-scroll-walk.sh <RichFiles.app> <out-dir> [down|right|both|manual] [copy|save] [opaque|grid|sticky|inset]}"
    MODE="${3:-down}"
    DEST="${4:-copy}"
    PAGE="${5:-opaque}"
    case "$MODE" in down|right|both|manual) ;; *) echo "unknown mode: $MODE" >&2; exit 2 ;; esac
    case "$DEST" in copy|save) ;; *) echo "unknown destination: $DEST" >&2; exit 2 ;; esac
    case "$PAGE" in opaque|grid|sticky|inset) ;; *) echo "unknown page: $PAGE" >&2; exit 2 ;; esac
    if [ "$PAGE" != grid ] && [ "$MODE" != down ]; then
        echo 'The sticky-page fixtures are vertical. Use grid for other modes.' >&2
        exit 2
    fi
    [ "$(basename "$APP")" = RichFiles.app ] && [ -d "$APP" ] || { echo "not a RichFiles.app: $APP" >&2; exit 2; }
    mkdir -p "$OUT"
    OUT="$(cd "$OUT" && pwd)"
    WORK="$(mktemp -d "${TMPDIR:?}/glance-scroll-walk.XXXXXX")"
    trap 'rm -rf -- "$WORK"' EXIT
    fixture="$WORK/Scroll Fixture.app"
    mkdir -p "$fixture/Contents/MacOS"
    cat > "$fixture/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key><string>app.richos.test.capturepattern</string>
	<key>CFBundleExecutable</key><string>CaptureProbe</string>
	<key>CFBundleName</key><string>Scroll Fixture</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>1.0</string>
	<key>CFBundleVersion</key><string>1</string>
	<key>LSMinimumSystemVersion</key><string>13.0</string>
	<key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST
    xcrun swiftc -O -target arm64-apple-macosx13.0 -o "$fixture/Contents/MacOS/CaptureProbe" "$T/glance-scroll-fixture.swift"
    codesign --force --sign - "$fixture"
    ditto -c -k --keepParent "$fixture" "$WORK/fixture.zip"
    xcrun swiftc -O -o "$WORK/check" "$T/glance-scroll-check.swift"
    ditto -c -k --keepParent "$APP" "$WORK/app.zip"
    python3 "$T/slots.py" run --wait "${GLANCE_WALK_WAIT:-1800}" --purpose "RichGlance scrolling capture walk" -- \
        bash "$0" --guest "$WORK" "$OUT" "$MODE" "$DEST" "$PAGE" 2>&1 | tee "$OUT/walk.log"
    exit "${PIPESTATUS[0]}"
fi

# The guest part, inside a held slot.
work="$2"
evidence="$3"
mode="$4"
destination="$5"
page="$6"
# shellcheck source=lib.sh
. "$T/lib.sh"
preflight_tart
vm="glance-cap-$(/usr/bin/uuidgen | cut -c1-8 | tr '[:upper:]' '[:lower:]')"
state="$TESTVM_RUN/$vm"
slot=$(python3 "$T/slots.py" check)
mkdir -p "$state"
printf '%s\n' "$slot" > "$state/slot"
trap '"$T/stop.sh" "$vm"' EXIT
vm_exists "$TESTVM_BASE_VM"
if vm_running "$TESTVM_BASE_VM"; then die 'test base is already running'; fi
tart clone "$TESTVM_BASE_VM" "$vm"
printf 'ephemeral\n' > "$state/ephemeral"
tart set "$vm" --cpu "$TESTVM_CPU" --memory "$TESTVM_RAM_MB" --display "$TESTVM_DISPLAY"
nohup "$TART_BIN" run "$vm" --no-graphics --no-clipboard --no-audio > "$TESTVM_LOG/$vm.log" 2>&1 &
printf '%s\n' "$!" > "$state/vm.pid"
printf 'off\n' > "$state/audio"
vm_ip "$vm" > "$state/ip"
for i in $(seq 1 30); do
    if guest_ssh "$vm" /usr/bin/true 2>/dev/null; then break; fi
    test "$i" -lt 30
    sleep 2
done
guest() { "$T/guest.sh" "$vm" "$@"; }
probe() { guest "/Users/admin/Applications/Scroll Fixture.app/Contents/MacOS/CaptureProbe" "$@"; }
shot() { "$T/shot.sh" "$vm" "$evidence/$vm-$1.png"; }
wait_for() {
    local expected="$1" actual; shift
    for i in $(seq 1 28); do
        actual=$(probe "$@" 2>/dev/null || true)
        if [ "$actual" = "$expected" ]; then printf 'PASS %s = %s\n' "$*" "$expected"; return 0; fi
        if [ "$*" = front ] && { [ "$i" -eq 8 ] || [ "$i" -eq 16 ]; } &&
           { [ "$actual" = com.apple.loginwindow ] || [ "$actual" = com.apple.UserNotificationCenter ]; }; then
            # The macOS 15 privacy reminder appeared in this disposable guest. Prefer its accessible
            # control; this guest's display is 1680 × 1050.
            if ! probe press Allow 2>/dev/null; then probe down 840 433; probe up 840 433; fi
        fi
        sleep 0.25
    done
    printf 'FAIL %s = %s (expected %s)\n' "$*" "$actual" "$expected" >&2
    shot failed-state || true
    guest --pull /Users/admin/Library/Logs/RichOS/richfiles-timing.jsonl "$evidence/$vm-incomplete-timing.jsonl" 2>/dev/null || true
    return 1
}
guest /usr/bin/sw_vers -productVersion
guest --push "$work/fixture.zip" /Users/admin/fixture.zip
guest --push "$work/app.zip" /Users/admin/walk-app.zip
guest 'mkdir -p ~/Applications; ditto -x -k ~/fixture.zip ~/Applications; ditto -x -k ~/walk-app.zip ~/Applications'
viewer=/Users/admin/Applications/RichFiles.app/Contents/Applications/RichGlance.app
guest codesign --verify --deep --strict "$viewer"
guest_ssh "$vm" /usr/bin/python3 - < "$T/capture-consent.py"

# Disposable guest only: seed the test grants so the pixel proof can run. Manual capture needs no
# Accessibility or event access (0); automatic scrolling does (2). The host's TCC is out of scope.
grant() {
    local value="$1"
    for guest_app in "$viewer" "${viewer%/Contents/Applications/*}"; do
        guest /bin/bash -s -- "$guest_app" "$value" <<'GRANT'
set -euo pipefail
app="$1"
value="$2"
id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist")
requirement=$(/usr/bin/codesign -d -r- "$app" 2>&1 | sed -n 's/^#* *designated => //p')
test -n "$requirement"
record=$(mktemp -d)
trap 'rm -rf "$record"' EXIT
/usr/bin/csreq -r "=$requirement" -b "$record/requirement"
blob=$(xxd -p "$record/requirement" | tr -d '\n')
test -n "$blob"
for service in kTCCServiceScreenCapture kTCCServiceAccessibility kTCCServicePostEvent; do
    if [ "$service" = kTCCServiceScreenCapture ]; then allowed=2; else allowed="$value"; fi
    sql="INSERT OR REPLACE INTO access (service,client,client_type,auth_value,auth_reason,auth_version,csreq,indirect_object_identifier_type,indirect_object_identifier,flags,last_modified) VALUES ('$service','$id',0,$allowed,3,1,X'$blob',0,'UNUSED',0,strftime('%s','now'));"
    sudo -n /usr/bin/sqlite3 -cmd ".timeout 5000" '/Library/Application Support/com.apple.TCC/TCC.db' "$sql"
    if [ -f "$HOME/Library/Application Support/com.apple.TCC/TCC.db" ]; then
        sudo -n /usr/bin/sqlite3 -cmd ".timeout 5000" "$HOME/Library/Application Support/com.apple.TCC/TCC.db" "$sql"
    fi
done
printf 'Seeded guest screen capture decision %s for %s\n' "$value" "$id"
GRANT
    done
    guest 'sudo -n /usr/bin/killall tccd || true'
}
if [ "$mode" = manual ]; then grant 0; else grant 2; fi

# A monthly macOS capture reminder can interrupt the first test-only session even after the initial
# region is visible. Acknowledge it in the guest, then restart both apps so the measured scroll
# begins from the original viewport.
for attempt in 1 2; do
    interrupted=no
    guest open -a "/Users/admin/Applications/Scroll Fixture.app" --args page "$page"
    sleep 2
    wait_for app.richos.test.capturepattern front
    origin=$(probe app-pid app.richos.test.capturepattern)
    probe move 480 318
    id=$(uuidgen | tr '[:upper:]' '[:lower:]')
    # shellcheck disable=SC2016  # $HOME, $1 and $2 are the guest shell's, expanded there
    guest /bin/bash -c 'dir="$HOME/Library/Application Support/app.richos.richglance/Capture Requests"; umask 077; mkdir -p "$dir"; printf "%s" "$2" > "$dir/$1"' bash "$id" "$origin"
    guest open -g -a "$viewer" "richglance-capture://request/$id" --args --capture-request
    stable=0
    for i in $(seq 1 45); do
        front="$(probe front || true)"
        if [ "$(probe approve-capture-reminder 2>/dev/null || true)" = yes ]; then
            stable=0
        elif [ "$front" = com.apple.UserNotificationCenter ]; then
            probe press Allow || true
            stable=0
        elif [ "$(probe selection 2>/dev/null || true)" = 'Display 1680 × 1050 px' ]; then
            stable=$((stable+1))
            if [ "$stable" -eq 5 ]; then break; fi
        else
            stable=0
        fi
        sleep 0.30
    done
    if [ "$stable" -ne 5 ]; then shot failed-select || true; probe front || true; exit 1; fi
    wait_for 'Display 1680 × 1050 px' selection
    for _ in 1 2 3; do
        "$T/pointer-drag.sh" "$vm" 200,162 776,524 --step 45 --pause 0.02 --rest 0.1 --app RichGlance
        if [ "$(probe selection 2>/dev/null || true)" = 'Crop 576 × 362 px' ]; then break; fi
    done
    wait_for 'Crop 576 × 362 px' selection
    shot 01-region
    probe move 1180 341
    probe down 1180 341
    probe up 1180 341
    sleep 0.4
    shot 02-mode-picker
    case "$mode" in
        manual) menuY=376; title='Scroll Manually' ;;
        down) menuY=397; title='Auto Scroll Down' ;;
        right) menuY=420; title='Auto Scroll Right' ;;
        both) menuY=442; title='Auto Scroll Down and Right' ;;
    esac
    # Press the menu item by its accessible name; a synthetic click on the open menu is sometimes not
    # taken, and the menu stays open. Click it when the press is unavailable or the menu is still open.
    pick() {
        if ! probe glance-press "$title" 2>/dev/null; then probe move 1160 "$menuY"; probe down 1160 "$menuY"; probe up 1160 "$menuY"; fi
    }
    pick
    sleep 0.4
    if [ "$(probe glance-has 'Auto Scroll Right' 2>/dev/null || true)" = yes ]; then
        probe move 1160 "$menuY"; probe down 1160 "$menuY"; probe up 1160 "$menuY"
    fi
    if [ "$mode" = manual ]; then
        wait_for yes glance-has 'Scroll the selected area, then click Stop.'
        wait_for app.richos.test.capturepattern front
        shot 03-manual-controls
        for i in $(seq 1 6); do probe scroll 450 322 -120 0; sleep 0.45; done
        shot 04-scroll-progress
        probe glance-press Stop
    else
        shot 03-auto-controls
    fi
    ready=no
    refused=no
    for i in $(seq 1 96); do
        if [ "$(probe approve-capture-reminder 2>/dev/null || true)" = yes ]; then
            interrupted=yes
            break
        fi
        ready=$(probe glance-has 'Copy Image' 2>/dev/null || true)
        if [ "$ready" = yes ]; then break; fi
        refused=$(probe glance-has 'Try Again…' 2>/dev/null || true)
        if [ "$refused" = yes ]; then break; fi
        sleep 0.65
    done
    if [ "$interrupted" = no ]; then break; fi
    test "$attempt" -lt 2
    printf 'Acknowledged the macOS Screen Recording reminder. Restarting the guest capture.\n'
    shot reminder-dismissed
    for bundle in app.richos.richglance app.richos.test.capturepattern; do
        pid=$(probe app-pid "$bundle")
        if [ "$pid" != 0 ]; then guest kill "$pid"; fi
    done
    wait_for 0 app-pid app.richos.richglance
    wait_for 0 app-pid app.richos.test.capturepattern
done
shot 05-preview
printf 'capture ended: image offered %s, retry offered %s\n' "$ready" "$refused"
if [ "$page" = sticky ] || [ "$page" = inset ]; then
    test "$refused" = yes
    test "$ready" = no
    wait_for no glance-has 'Save PNG…'
    probe glance-press 'Try Again…'
    wait_for 'Display 1680 × 1050 px' selection
    shot 06-retry
    printf 'PASS retry restored the region selector and the rejected stitch offered no export\n'
elif [ "$ready" != yes ]; then
    "$T/ax.sh" "$vm" tree --app RichGlance --depth 7 || true
    guest --pull /Users/admin/Library/Logs/RichOS/richfiles-timing.jsonl "$evidence/$vm-incomplete-timing.jsonl" || true
    exit 1
elif [ "$destination" = save ]; then
    # The preview's buttons can be in the accessibility tree a moment before they take a press.
    for i in $(seq 1 10); do
        if probe glance-press 'Save PNG…' 2>/dev/null; then break; fi
        test "$i" -lt 10
        sleep 0.5
    done
    wait_for yes glance-has Save
    shot 06-save
    # The save panel's controls arrive in the accessibility tree a moment after it shows.
    pressed=no
    for i in $(seq 1 10); do
        if probe glance-press Save 2>/dev/null; then pressed=yes; break; fi
        sleep 0.5
    done
    printf 'save pressed through accessibility: %s\n' "$pressed"
    if [ "$pressed" = no ]; then probe move 998 465; probe down 998 465; probe up 998 465; fi
    saved=
    for i in $(seq 1 20); do
        # shellcheck disable=SC2016  # the guest's $HOME, expanded there
        saved=$(guest 'find "$HOME/Documents" "$HOME/Desktop" -maxdepth 2 -name "RichGlance Capture*.png" -print 2>/dev/null | head -1' || true)
        if [ -n "$saved" ]; then break; fi
        sleep 0.25
    done
    test -n "$saved"
    guest --pull "$saved" "$evidence/$vm-result.png"
else
    probe glance-press 'Copy Image'
    probe clipboard-write /Users/admin/scroll-result.png
    guest --pull /Users/admin/scroll-result.png "$evidence/$vm-result.png"
fi
guest --pull /Users/admin/Library/Logs/RichOS/richfiles-timing.jsonl "$evidence/$vm-timing.jsonl"
if [ "$ready" = yes ]; then
    "$work/check" "$evidence/$vm-result.png" "$page" "$mode"
fi
python3 - "$evidence/$vm-timing.jsonl" "$ready" <<'PYTHON'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as file:
    scroll = [row for line in file if (row := json.loads(line))["operation"] == "glanceScrollCapture"]
assert scroll, "Missing scroll capture diagnostics"
last = scroll[-1]
if sys.argv[2] == "yes":
    assert last["outcome"] == "complete" and last["confirmed"] > 1, last
    assert "scrollCaptureFailure" not in last, last
else:
    assert last["outcome"] == "issues" and last["scrollCaptureFailure"] in ("conflictingPixels", "lostOverlap"), last
print(f"PASS diagnostics: {last}")
PYTHON
# Quit both test apps in this disposable guest, then confirm they are gone.
for bundle in app.richos.richglance app.richos.test.capturepattern; do
    pid=$(probe app-pid "$bundle" || true)
    if [ -n "$pid" ] && [ "$pid" != 0 ]; then guest kill "$pid" || true; fi
done
sleep 1
for bundle in app.richos.richglance app.richos.test.capturepattern; do
    left=$(probe app-pid "$bundle" || true)
    if [ "$left" = 0 ]; then printf 'quit %s: gone\n' "$bundle"; else printf 'quit %s: still running pid %s\n' "$bundle" "$left"; exit 1; fi
done
