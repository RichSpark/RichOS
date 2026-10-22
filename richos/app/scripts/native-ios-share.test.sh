#!/usr/bin/env bash
# The RichOS native iPhone app's extensions and release readiness (build plan §5.0, stream I3):
# the notification service extension, "Share to Rich", the microphone and recorder's pure parts,
# the TestFlight tool, the app icon, the privacy manifest and the Release product.
# The platform code runs on this Mac with swiftc, the TestFlight tool against a fake App Store
# Connect, and the Release product is checked on disk; S7 then runs the share sheet, the platform
# effect handler and the notification platform on ONE simulator this suite creates and deletes. A host without
# macOS, Swift, Xcode, XcodeGen, Node or the external SSD prints NOT RUN and exits 2; a failure on a
# capable host is red.
# run-tests: no-host-screen: `simctl boot` starts a dedicated simulator without Simulator.app; snapshots are rendered in-process, never from the Mac's screen
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/app/scripts/lib/simulator_budget.py richos/engine/scripts/lib/worker_tokens.py richos/mobile/native-ios/App/Platform richos/mobile/native-ios/ShareExtension richos/mobile/native-ios/NotificationService richos/mobile/native-ios/PlatformTests richos/mobile/native-ios/Release richos/mobile/native-ios/Core/Sources/RichOSCore richos/mobile/native-ios/Core/Sources/RichOSCLI/Simulator.swift richos/mobile/native-ios/project.yml richos/mobile/native-ios/App/Design richos/mobile/test/fixtures/notification-preview.json richos/app/icon-source/richos-icon-1024.png richos/web/web-app/bin/make-icons.js richos/app/scripts/native-ios-share.test.sh richos/mobile/service/connect/schema.sql richos/engine/scripts/lib/proc_tree.py richos/engine/scripts/lib/testdevices.py richos/app/scripts/testvm/reserve.py richos/mobile/native-ios/Core/Package.swift richos/mobile/perf/reviewwalk.py
# run-tests: select-inputs richos/mobile/native-ios/App/Platform richos/mobile/native-ios/ShareExtension richos/mobile/native-ios/NotificationService richos/mobile/native-ios/PlatformTests richos/mobile/native-ios/Core/Sources/RichOSCore richos/mobile/native-ios/Core/Sources/RichOSCLI/Simulator.swift richos/mobile/native-ios/project.yml richos/mobile/native-ios/App/Design richos/mobile/test/fixtures/notification-preview.json richos/app/icon-source/richos-icon-1024.png richos/web/web-app/bin/make-icons.js richos/mobile/service/connect/schema.sql richos/mobile/native-ios/Release/platform.yml richos/mobile/native-ios/Release/App-Info.plist richos/mobile/native-ios/Release/RichOSNative.entitlements richos/mobile/native-ios/Release/ExportOptions.plist richos/mobile/native-ios/Release/generate.sh richos/mobile/native-ios/Release/check-release.sh richos/mobile/native-ios/Release/check_device_family.py richos/mobile/native-ios/Release/check_sdk_floor.test.py richos/mobile/native-ios/Release/make-app-icon.cjs richos/mobile/native-ios/Release/platform-tests.sh richos/mobile/native-ios/Release/testflight.ts richos/mobile/native-ios/Release/testflight.test.ts richos/mobile/native-ios/Core/Package.swift
# run-tests: covers richos/mobile/native-ios/App/Platform/Shared/PlatformIdentity.swift richos/mobile/native-ios/App/Platform/Shared/NotificationPreview.swift richos/mobile/native-ios/App/Platform/Shared/NotificationTarget.swift richos/mobile/native-ios/App/Platform/Shared/PushRegistration.swift richos/mobile/native-ios/App/Platform/Shared/ShareInbox.swift richos/mobile/native-ios/App/Platform/Shared/AttachmentDelivery.swift richos/mobile/native-ios/App/Platform/Shared/PhotoNormalizer.swift richos/mobile/native-ios/App/Platform/Shared/VoiceLevel.swift richos/mobile/native-ios/App/Platform/NotificationTapRouter.swift richos/mobile/native-ios/App/Platform/SharePlatform.swift richos/mobile/native-ios/App/Platform/PrivacyInfo.xcprivacy richos/mobile/native-ios/App/Platform/Assets.xcassets/Contents.json richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/Contents.json richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-40.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-58.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-60.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-80.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-87.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-120.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-180.png richos/mobile/native-ios/App/Platform/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png richos/mobile/native-ios/NotificationService/Sources/NotificationService.swift richos/mobile/native-ios/NotificationService/Tests/main.swift richos/mobile/native-ios/NotificationService/Info.plist richos/mobile/native-ios/NotificationService/NotificationService.entitlements richos/mobile/native-ios/ShareExtension/Tests/main.swift richos/mobile/native-ios/ShareExtension/Info.plist richos/mobile/native-ios/ShareExtension/ShareExtension.entitlements richos/mobile/native-ios/Release/platform.yml richos/mobile/native-ios/Release/App-Info.plist richos/mobile/native-ios/Release/RichOSNative.entitlements richos/mobile/native-ios/Release/generate.sh richos/mobile/native-ios/Release/check-release.sh richos/mobile/native-ios/Release/check_device_family.py richos/mobile/native-ios/Release/check_sdk_floor.test.py richos/mobile/native-ios/Release/platform-tests.sh richos/mobile/native-ios/Release/make-app-icon.cjs richos/mobile/native-ios/Release/testflight.ts richos/mobile/native-ios/Release/testflight.test.ts richos/mobile/native-ios/Release/ExportOptions.plist richos/mobile/native-ios/App/Platform/NotificationPlatform.swift richos/mobile/native-ios/App/Platform/PlatformEffects.swift richos/mobile/native-ios/App/Platform/AttachmentPicker.swift richos/mobile/native-ios/PlatformTests/AttachmentPickerTests.swift richos/mobile/native-ios/App/Platform/VoicePlatform.swift richos/mobile/native-ios/ShareExtension/Sources/ShareModel.swift richos/mobile/native-ios/ShareExtension/Sources/SharePayloadLoader.swift richos/mobile/native-ios/ShareExtension/Sources/ShareSheetView.swift richos/mobile/native-ios/ShareExtension/Sources/ShareViewController.swift richos/mobile/native-ios/PlatformTests/ShareSheetTests.swift richos/mobile/native-ios/PlatformTests/PlatformEffectsTests.swift
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
if [ -z "${RICHOS_WORKER_TOKENS:-}" ]; then
  exec python3 "$ROOT/richos/engine/scripts/lib/worker_tokens.py" machine -- bash "${BASH_SOURCE[0]}" "$@"
fi
NATIVE="$ROOT/richos/mobile/native-ios"
MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2

notrun() { echo "  NOT RUN  native-ios-share: $1"; exit 2; }
[ "$(uname -s)" = Darwin ] || notrun "macOS is required"
for tool in swift xcrun xcodebuild xcodegen node python3; do
  command -v "$tool" >/dev/null 2>&1 || notrun "$tool is unavailable"
done

# Build output is reused per checkout (a cold Debug + Release build is minutes); scratch is per run.
KEY="$(printf '%s' "$ROOT" | shasum -a 256 | cut -c1-10)"
CACHE="$(python3 "$MACHINE_LIB/machine_settings.py" validate "$CACHE_ROOT/richos-native-ios-proof/$KEY-share")" || exit 2
CACHE="$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$CACHE")"
if [ "${RICHOS_SIMULATOR_CACHE_HELD:-}" != "$CACHE" ]; then
  exec python3 "$DIR/lib/simulator_budget.py" cache "$CACHE" -- bash "${BASH_SOURCE[0]}" "$@"
fi
mkdir -p "$CACHE"
mkdir -p "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}"
SCRATCH="$(mktemp -d "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}/native-ios-share.XXXXXX")" || { echo '  FAIL  scratch directory'; exit 1; }
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM

PASS=0; FAILED=0
# A case this host cannot answer (S5 without its tag, S7 without a simulator runtime or when its
# lease ends) is named here and makes the whole suite exit 2 below: NOT RUN is never a pass
# (hunt part 2, finding 18; before 2026-09-30 only the lost lease did).
NOT_RUN=""
ok()  { PASS=$((PASS + 1)); echo "  ok  $1"; }
bad() { FAILED=$((FAILED + 1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; return 0; }

echo "=== native-ios-share ==="

# S1 — the platform code's own tests, on this Mac (notification extension, tap routing, the share
# inbox and delivery against a fake Mac, HEIC to JPEG, the activation rule, the voice level).
if "$NATIVE/Release/platform-tests.sh" "$CACHE/platform-tests" >"$SCRATCH/platform.log" 2>&1; then
  N="$(grep -Eo 'Notification platform: [0-9]+ checks passed' "$SCRATCH/platform.log" | grep -Eo '[0-9]+')"
  S="$(grep -Eo 'Share platform: [0-9]+ checks passed' "$SCRATCH/platform.log" | grep -Eo '[0-9]+')"
  if [ "${N:-0}" -gt 0 ] && [ "${S:-0}" -gt 0 ]; then ok "S1 platform tests: notification $N checks, share $S checks (Release/platform-tests.sh)"
  else bad "S1 platform tests" "no passing counts: $(tail -3 "$SCRATCH/platform.log" | tr '\n' ' ')"; fi
else
  bad "S1 platform tests" "$(grep -E 'error:|FAIL' "$SCRATCH/platform.log" | head -8 | tr '\n' ' ')"
  # The whole log kept with the run (it is in scratch, deleted on exit), and any test it names.
  python3 "$DIR/lib/test_results.py" report --label "native-ios-share S1" \
    ${RICHOS_TEST_RESULTS_DIR:+--into "$RICHOS_TEST_RESULTS_DIR"} --collect "platform.log=$SCRATCH/platform.log"
fi

# S2 — the TestFlight tool against a fake App Store Connect (no request leaves this Mac).
if node --test "$NATIVE/Release/testflight.test.ts" >"$SCRATCH/testflight.log" 2>&1; then
  T="$(grep -Eo '^ℹ pass [0-9]+' "$SCRATCH/testflight.log" | grep -Eo '[0-9]+')"
  if [ "${T:-0}" -gt 0 ]; then ok "S2 TestFlight tool: $T tests pass against a fake App Store Connect"
  else bad "S2 TestFlight tool" "no pass count: $(tail -5 "$SCRATCH/testflight.log" | tr '\n' ' ')"; fi
else
  bad "S2 TestFlight tool" "$(grep -E 'not ok|Error' "$SCRATCH/testflight.log" | head -6 | tr '\n' ' ')"
fi

# S3 — the committed icon is exactly what the generator makes from the RichOS icon source.
if node "$NATIVE/Release/make-app-icon.cjs" --check >"$SCRATCH/icon.log" 2>&1; then ok "S3 $(cat "$SCRATCH/icon.log")"
else bad "S3 app icon" "$(cat "$SCRATCH/icon.log")"; fi

# S4 — the extensions' identifiers derive from RICHOS_BUNDLE_ID, which the app's own identifier is (the device tools' test
# copy overrides that one setting on the command line, so the app, its extensions and its groups move together).
APP_ID="$(awk '/^  RichOSNative:/{t=1} t && /PRODUCT_BUNDLE_IDENTIFIER:/{print $2; exit}' "$NATIVE/project.yml")"
BASE_ID="$(awk '/RICHOS_BUNDLE_ID:/{print $2; exit}' "$NATIVE/Release/platform.yml")"
WIRE_ID="$(sed -n 's/.*static let apnsTopic = "\([^"]*\)".*/\1/p' "$NATIVE/Core/Sources/RichOSCore/Protocol/PairingAPI.swift")"
CLI_ID="$(sed -n 's/.*static let bundleID = "\([^"]*\)".*/\1/p' "$NATIVE/Core/Sources/RichOSCLI/Simulator.swift")"
# shellcheck disable=SC2016  # the build setting's own text, compared literally
if [ "$APP_ID" = '$(RICHOS_BUNDLE_ID)' ] && [ "$BASE_ID" = "dev.richos.connect" ] && [ "$BASE_ID" = "$WIRE_ID" ] && [ "$BASE_ID" = "$CLI_ID" ]; then ok "S4 the app, extensions, CLI and push registration share the permanent identity ($BASE_ID)"
else bad "S4 permanent application identity agrees across packaging and push" "app '$APP_ID', extensions '$BASE_ID', push '$WIRE_ID', CLI '$CLI_ID'"; fi

# S5 — the preserved iPhone app, its UI and the PWA are byte-unchanged (ceo-decisions §76: "preserved
# as is for now"). What is protected is the PRODUCT: every file of the three trees except their test
# suites. The suites are excluded on purpose: §76 keeps them running, and a suite may record a defect
# in the preserved product (tom-opus-pwa1's KNOWN DEFECT tests) without that product changing. The
# PWA's `test/` is also what build.rs PHONE_NOT_SHIPPED leaves out of the shipped phone app.
# Re-based 2026-09-24 to include c5250dc9 (the PWA's v2 pairing: the Mac must press "They match"),
# the one deliberate change since the 2026-09-22 tag: a security fix the PWA needs because it is the
# CEO's Android stand-in (§76 preservation was Rich's technical call, not a CEO sentence).
# Re-based again 2026-09-25 to include e2da1f28, f03d89dd and addaee46 (the PWA's pair-wait, Urban's
# pairing wording and Sage's old-Mac sentence), deliberate so the PWA says and does what both native
# apps do. Only app.js and lib/api.js moved; the preserved iPhone app and its UI are unchanged.
TAG="preserved/mobile-ios-and-pwa-2026-09-25"
PRESERVED=(richos/mobile/ios richos/mobile/ui richos/web/web-app)
NOT_PRODUCT=(richos/mobile/ios/Tests richos/mobile/ios/UITests richos/web/web-app/test)
if git -C "$ROOT" rev-parse -q --verify "$TAG^{commit}" >/dev/null 2>&1; then
  CHANGED="$(git -C "$ROOT" diff --name-only "$TAG^{commit}" -- "${PRESERVED[@]}" "${NOT_PRODUCT[@]/#/:(exclude)}")"
  if [ -z "$CHANGED" ]; then ok "S5 the preserved app, its UI and the PWA are unchanged since $TAG (their test suites excepted: ${NOT_PRODUCT[*]})"
  else bad "S5 the preserved app, its UI and the PWA are unchanged (test suites excepted)" "$(echo "$CHANGED" | head -5 | tr '\n' ' ')"; fi
else
  echo "  NOT RUN  S5 the tag $TAG is not in this clone"
  NOT_RUN="$NOT_RUN, S5 (the tag $TAG is not in this clone)"
fi

# S6 — the Release product, built from the merged project: extensions, purpose strings, export
# compliance, groups, the privacy manifest against the APIs the binaries import, the icon, no
# development code, nothing stray bundled (Release/check-release.sh R1–R10).
if "$NATIVE/Release/check-release.sh" "$CACHE/release" >"$SCRATCH/release.log" 2>&1; then
  ok "S6 Release product: $(grep -c '^  ok ' "$SCRATCH/release.log") checks (Release/check-release.sh)"
else
  bad "S6 Release product" "$(grep -E 'FAIL|error:' -A1 "$SCRATCH/release.log" | head -12 | tr '\n' ' ')"
fi

# S6b — the archive check refuses an app built with an SDK older than iOS 26 (Apple refused build 1).
if python3 "$NATIVE/Release/check_sdk_floor.test.py" >"$SCRATCH/sdk-floor.log" 2>&1; then
  ok "S6b SDK floor: iphoneos18.5 refused, iphoneos26.2 passes (Release/check_sdk_floor.test.py)"
else
  bad "S6b SDK floor" "$(tail -5 "$SCRATCH/sdk-floor.log" | tr '\n' ' ')"
fi

# S7 — on a simulator this suite creates, boots headless and deletes: the share sheet loads real item
# providers, composes, refuses a file over the Mac's limit, is Saved with no connection and Sent only
# on the Mac's acceptance, and renders every state (PNGs under the cache's snapshots/); the platform
# effect handler answers the microphone question and passes network effects on; the notification
# platform's token, cold-launch tap and preview key. Recording itself is not run here: a simulator
# records from the Mac's own microphone. (ShareViewController is compiled and embedded; its three
# lines of hosting run only inside a real share sheet.)
if xcrun simctl list runtimes 2>/dev/null | grep -q '^iOS .*com.apple.CoreSimulator.SimRuntime.iOS'; then
  touch "$SCRATCH/s7.start"
  if "$NATIVE/Release/simulator-tests.sh" "$CACHE/simulator" >"$SCRATCH/sim.log" 2>&1; then SIM_RC=0; else SIM_RC=$?; fi
  if [ "$SIM_RC" -eq 0 ]; then
    ok "S7 $(grep '^simulator tests:' "$SCRATCH/sim.log")"
  elif [ "$SIM_RC" -eq 75 ]; then
    # The lease ended mid-run: no result from that device, never a pass (exit 2 below).
    echo "  NOT RUN  S7 $(grep '^NOT RUN' "$SCRATCH/sim.log")"
    NOT_RUN="$NOT_RUN, S7 (its simulator lease ended mid-run)"
  else
    bad "S7 simulator tests" "$(tail -12 "$SCRATCH/sim.log" | tr '\n' ' ')"
    # Every failing test by name; this run's bundle (the next run of this suite deletes it
    # first, Release/simulator-tests.sh) moved, and the log copied, to the run's results.
    KEEP=(--collect "sim.log=$SCRATCH/sim.log")
    if [ -d "$CACHE/simulator/result.xcresult" ] && [ "$CACHE/simulator/result.xcresult" -nt "$SCRATCH/s7.start" ]; then
      KEEP+=(--move "simulator-result.xcresult=$CACHE/simulator/result.xcresult")
    fi
    python3 "$DIR/lib/test_results.py" report --label "native-ios-share S7" \
      ${RICHOS_TEST_RESULTS_DIR:+--into "$RICHOS_TEST_RESULTS_DIR"} "${KEEP[@]}"
  fi
else
  echo "  NOT RUN  S7 no iOS simulator runtime is installed"
  NOT_RUN="$NOT_RUN, S7 (no iOS simulator runtime is installed)"
fi

if [ "$FAILED" -eq 0 ] && [ -n "$NOT_RUN" ]; then
  echo "=== native-ios-share tests: $PASS passed, NOT RUN: ${NOT_RUN#, } ==="
  exit 2
fi
if [ "$FAILED" -eq 0 ]; then
  echo "=== native-ios-share tests: all $PASS passed ==="
  exit 0
fi
echo "=== native-ios-share tests: $FAILED FAILED, $PASS passed ==="
exit 1
