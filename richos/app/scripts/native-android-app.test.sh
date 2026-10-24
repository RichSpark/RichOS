#!/usr/bin/env bash
# Native Android app, stream A1 app shell: the app's entry, store wiring and debug-only bridge,
# tested headless on the JVM with Robolectric (no emulator), then the release APK and bundle
# checked for the bridge's absence and the permanent ID (build plan §3.1), the launcher icon
# checked for drift from its source, and the release-only signing path run end to end with a
# throwaway key made here (the real upload key never reaches a test; T3 ledger §2.4: release-only
# steps are exercised on the ordinary path, not only when releasing). A host without Java, the external SSD, Android SDK
# platform 37.0 (compile) or build-tools 36.0.0 prints NOT RUN and exits 2; a build or test
# failure on a capable host exits 1.
#
# THE EMULATOR LAYER IS OPT-IN: RANDROID_SUITE_EMULATOR=1 also boots this checkout's own
# emulator (-no-window, by the serial bin/randroid records), checks that the on-device bridge
# returns the same states as headless and that the screen shows the core's draft, then shuts
# the emulator down and deletes its AVD. Off by default because every land and nightly runs
# every suite and a cold emulator boot is the most expensive step in this file.
# run-tests: no-host-screen: Robolectric runs on the JVM; the opt-in emulator is started with -no-window
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/mobile/native-android/app richos/mobile/native-android/core richos/mobile/native-android/bin richos/mobile/native-android/gradle richos/mobile/native-android/gradlew richos/mobile/native-android/settings.gradle.kts richos/mobile/native-android/build.gradle.kts richos/mobile/native-android/gradle.properties richos/mobile/native-android/release richos/mobile/security richos/app/icon-source richos/web/web-app/bin/make-icons.js richos/mobile/perf/shipgate.py richos/mobile/perf/watch.py richos/mobile/perf/perfcore.py richos/mobile/perf/reviewwalk.py richos/app/scripts/lib/ship_gate_fixture.py richos/app/scripts/native-android-app.test.sh
# run-tests: covers richos/mobile/native-android/app/src/debug/kotlin/dev/richos/android/app/debug/DevBridge.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/app/AppPorts.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/app/AppStore.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/app/MainActivity.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/app/RichApplication.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/Attachments.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/HttpsMac.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/KeystoreKeys.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/MicRecorder.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/Notifications.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/ShareToRich.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/Wav.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/AppTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/LauncherIconTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/AttachmentsTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/IdentityTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/NotificationsTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/WavTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/WireTest.kt richos/mobile/native-android/app/src/testDebug/kotlin/dev/richos/android/app/debug/DevBridgeTest.kt richos/mobile/native-android/app/src/testDebug/kotlin/dev/richos/android/app/debug/ScreenFollowsCoreTest.kt richos/mobile/native-android/app/src/testDebug/kotlin/dev/richos/android/app/debug/ShareTest.kt richos/mobile/native-android/bin/randroid richos/mobile/native-android/gradlew richos/mobile/native-android/gradle/libs.versions.toml richos/mobile/native-android/release/make-app-icon.cjs richos/mobile/native-android/release/upload-certificate.sha256 richos/mobile/native-android/app/src/testDebug/kotlin/dev/richos/android/app/debug/ScanImageTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/OutboxDrainTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/NetworkWake.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/NotificationTaps.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/IntakeLimitsTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/NetworkWakeTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/NotificationTapsTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/OnScreenTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/PlatformSecurityTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/TokenRefreshTest.kt richos/mobile/native-android/app/src/testDebug/kotlin/dev/richos/android/app/debug/ShareConsentTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/FollowsThePhoneTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/AppLinksTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/ForgetInstallationTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/StagedPhotos.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/AttachmentPickerTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/AppStoreIdleTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/AppStoreMacWaitTest.kt richos/mobile/native-android/app/src/testDebug/kotlin/dev/richos/android/app/debug/PairingV2TapTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/app/LocalSessionStore.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/LocalSessionStoreTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/ReplayedReplyRestoreTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/MicrophoneMirrorTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/ReadReplies.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/platform/ReadRepliesTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/app/PortraitOnlyTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/KeptAudioPlayer.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/platform/RichFileKeeper.kt
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
NA="$ROOT/richos/mobile/native-android"
RANDROID="$NA/bin/randroid"

MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
python3 "$MACHINE_LIB/machine_settings.py" cache >/dev/null || exit 2
# Java: bin/randroid picks a 21+ runtime (RANDROID_JAVA_HOME, JAVA_HOME, Android Studio's) and
# exits 2 with NOT RUN when there is none; that exit is passed through below.
SDK="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
if [ ! -f "$SDK/platforms/android-37.0/android.jar" ] || [ ! -x "$SDK/build-tools/36.0.0/aapt2" ]; then
  echo "  NOT RUN  native-android-app: Android SDK platform 37.0 and build-tools 36.0.0 are not installed under $SDK"
  exit 2
fi

fails=0
bad() { echo "  FAIL  native-android-app: $1"; fails=$((fails + 1)); }

# One scratch directory for the whole suite, removed however the suite ends (with the emulator
# this suite boots, when it boots one).
SCRATCH_PARENT="${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}"
mkdir -p "$SCRATCH_PARENT"
SCRATCH="$(mktemp -d "$SCRATCH_PARENT/native-android-app.XXXXXX")" || { echo "  FAIL  native-android-app: no scratch"; exit 1; }
EMU_STARTED=0
cleanup() {
  if [ "$EMU_STARTED" = "1" ]; then "$RANDROID" emu delete >/dev/null 2>&1; fi
  rm -rf "$SCRATCH"
}
trap cleanup EXIT HUP INT TERM

# Only this stream's tests; the screens' screenshot sweep is native-android-ui.test.sh (A2).
"$RANDROID" test app -q --tests 'dev.richos.android.app.*' --tests 'dev.richos.android.platform.*'
code=$?
if [ "$code" -eq 2 ]; then echo "  NOT RUN  native-android-app: bin/randroid reported a missing host tool"; exit 2; fi
[ "$code" -eq 0 ] || bad "Robolectric tests (:app:testDebugUnitTest) exited $code"

"$RANDROID" check-release >/dev/null || bad "check-release: the development bridge reached the release APK or bundle, the permanent ID is wrong, or it did not build"

if node "$NA/release/make-app-icon.cjs" --check >"$SCRATCH/icon.log" 2>&1; then :; else bad "launcher icon: $(cat "$SCRATCH/icon.log")"; fi

# The signing path, end to end, with a throwaway upload key made here. `bundle` must sign and
# verify it when told to expect that key; `verify-bundle` against the COMMITTED upload certificate
# must refuse the same file, so the check is keyed to the real certificate and not to any key.
# The throwaway key lives two days in this suite's scratch and is deleted with it; the Firebase
# values are placeholders, so the smoke bundle is never an uploadable app. Every bundle asks the speed
# gate first, whatever key signs it (CEO §106), so the suite gives it a STAND-IN passing verdict for
# this checkout's HEAD, in its own scratch (lib/ship_gate_fixture.py); the stand-in also names the exact
# uncommitted app code in this tree (a merge gate bundles its uncommitted merge tree), so the test checks
# the bundle step; the real gate still refuses any uncommitted app code. It also asks the review-walk gate (CEO §107), so
# the same fixture writes a STAND-IN passing walk record for HEAD (--walks, RICHOS_REVIEW_WALK_RECORDS).
KEYTOOL=""
for home in "${RANDROID_JAVA_HOME:-}" "${JAVA_HOME:-}" "/Applications/Android Studio.app/Contents/jbr/Contents/Home"; do
  if [ -n "$home" ] && [ -x "$home/bin/keytool" ]; then KEYTOOL="$home/bin/keytool"; break; fi
done
if [ -z "$KEYTOOL" ]; then
  bad "no keytool for the release-signing smoke test (set RANDROID_JAVA_HOME)"
else
  SMOKE_SECRET="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
  if SMOKE_SECRET="$SMOKE_SECRET" "$KEYTOOL" -genkeypair -storetype PKCS12 -keystore "$SCRATCH/smoke.p12" \
       -alias upload -keyalg RSA -keysize 2048 -validity 2 -dname "CN=release smoke test" \
       -storepass:env SMOKE_SECRET -keypass:env SMOKE_SECRET >/dev/null 2>&1; then
    SMOKE_SHA="$(SMOKE_SECRET="$SMOKE_SECRET" "$KEYTOOL" -list -v -keystore "$SCRATCH/smoke.p12" -alias upload \
      -storepass:env SMOKE_SECRET 2>/dev/null | sed -n 's/^[[:space:]]*SHA256:[[:space:]]*//p' | head -1)"
    # An empty fingerprint would make bundle verify against the committed certificate instead.
    if [ -z "$SMOKE_SHA" ]; then
      bad "could not read the throwaway key's fingerprint"
    elif ! python3 "$DIR/lib/ship_gate_fixture.py" --dir "$SCRATCH/speed-verdicts" --platform android \
         --commit "$(git -C "$ROOT" rev-parse HEAD)" --repo "$ROOT" --walks "$SCRATCH/review-walks" >/dev/null; then
      bad "could not write the stand-in speed verdict and review walk"
    elif env RANDROID_RELEASE_DIR="$SCRATCH/release" RANDROID_UPLOAD_CERT_SHA256="$SMOKE_SHA" \
         RICHOS_SHIP_GATE_VERDICTS="$SCRATCH/speed-verdicts" \
         RICHOS_REVIEW_WALK_RECORDS="$SCRATCH/review-walks" \
         "ORG_GRADLE_PROJECT_richos.upload.storeFile=$SCRATCH/smoke.p12" \
         "ORG_GRADLE_PROJECT_richos.upload.storePassword=$SMOKE_SECRET" \
         "ORG_GRADLE_PROJECT_richos.upload.keyAlias=upload" \
         "ORG_GRADLE_PROJECT_richos.upload.keyPassword=$SMOKE_SECRET" \
         "ORG_GRADLE_PROJECT_richos.firebase.projectId=release-smoke" \
         "ORG_GRADLE_PROJECT_richos.firebase.appId=release-smoke" \
         "ORG_GRADLE_PROJECT_richos.firebase.apiKey=release-smoke" \
         "ORG_GRADLE_PROJECT_richos.firebase.senderId=release-smoke" \
         "$RANDROID" bundle --allow-dirty >"$SCRATCH/bundle.json" 2>"$SCRATCH/bundle.err"; then
      SMOKE_AABS=("$SCRATCH"/release/RichConnect-*.aab)
      SMOKE_AAB="${SMOKE_AABS[0]}"
      if [ ! -f "$SMOKE_AAB" ]; then bad "bundle reported success but wrote no .aab"
      elif "$RANDROID" verify-bundle "$SMOKE_AAB" >/dev/null 2>&1; then
        bad "verify-bundle accepted a bundle signed by a key that is not the committed upload certificate"
      fi
    else
      bad "bundle (throwaway key): $(tail -1 "$SCRATCH/bundle.err")"
    fi
  else
    bad "keytool could not make the throwaway key"
  fi
fi

if [ "${RANDROID_SUITE_EMULATOR:-0}" = "1" ]; then
  export RANDROID_SESSION="$SCRATCH/headless.json"
  # The emulator this suite boots is deleted however the suite ends (cleanup, above).
  EMU_STARTED=1
  # randroid binds this run to the actual emulator PID and start time.
  export RICHOS_TEST_DEVICE_OWNER_PID=$$
  "$RANDROID" emu prepare >/dev/null || bad "emu prepare (boot, build, install, launch) failed"
  if [ "$fails" -eq 0 ]; then
    "$RANDROID" emu parity draft-survives-restart >/dev/null || bad "the emulator's states differ from headless for draft-survives-restart"
    "$RANDROID" emu action '{"type":"compose","text":"On the screen"}' >/dev/null || bad "emu action failed"
    "$RANDROID" emu verify >/dev/null || bad "the screen does not show the core's draft"
    "$RANDROID" emu restart >/dev/null || bad "emu restart failed"
    "$RANDROID" emu verify >/dev/null || bad "the draft did not survive a real process restart on the device"
  fi
else
  echo "  NOTE  native-android-app: emulator layer not run (set RANDROID_SUITE_EMULATOR=1)"
fi

if [ "$fails" -gt 0 ]; then exit 1; fi
echo '  PASS  native-android-app: Robolectric entry + bridge tests, release APK and bundle free of the bridge with the permanent ID, icon current, release signing path'
