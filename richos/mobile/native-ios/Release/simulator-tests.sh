#!/usr/bin/env bash
# Stream I3's tests on a simulator (target RichOSPlatformTests): the Share to Rich sheet's model
# and rendering, the platform effect handler, the notification platform.
#
#   Release/simulator-tests.sh <output directory in the selected storage folder>
#
# Leases ONE prepared simulator, boots it headlessly (no Simulator.app, nothing on the
# Mac's screen), grants the microphone to the host app as I2's UI tests do, runs the tests, and shuts
# the simulator down and retains its prepared OS however the run ends. It never uses `booted`. The share sheet's
# rendered states are written to <output>/snapshots/*.png. Prints the xcresult counts and exits 0
# only when every test passed and at least one ran.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "$HERE/../../engine/scripts/lib/cpu_guard.py" check-ios || exit 2
OUT="${1:?usage: simulator-tests.sh <output directory>}"
OUT="$(python3 "$HERE/../../engine/scripts/lib/machine_settings.py" validate "$OUT")" || exit 2
BUDGET="$HERE/../../app/scripts/lib/simulator_budget.py"
if [ -z "${RICHOS_WORKER_TOKENS:-}" ]; then
  exec python3 "$HERE/../../engine/scripts/lib/worker_tokens.py" machine -- bash "${BASH_SOURCE[0]}" "$@"
fi
OUT="$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$OUT")"
if [ "${RICHOS_SIMULATOR_CACHE_HELD:-}" != "$OUT" ]; then
  exec python3 "$BUDGET" cache "$OUT" -- bash "${BASH_SOURCE[0]}" "$@"
fi
mkdir -p "$OUT/logs"
rm -rf "$OUT/snapshots" "$OUT/result.xcresult"

PROJECT="$("$HERE/Release/generate.sh" "$OUT/project")" || { echo "FAIL generate"; exit 1; }
RUNTIME="$(xcrun simctl list runtimes --json | python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x.get("isAvailable") and x["name"].startswith("iOS")]; print(r[-1]["identifier"] if r else "")')"
[ -n "$RUNTIME" ] || { echo "NOT RUN no iOS simulator runtime"; exit 2; }
export RICHOS_TEST_DEVICE_OWNER_PID=$$
UDID="$(python3 "$HERE/../../engine/scripts/lib/testdevices.py" acquire-ios --type "com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro" --runtime "$RUNTIME" --owner-pid $$ 2>"$OUT/lease.err")" || {
  cat "$OUT/lease.err" >&2
  if grep -qF 'prepared simulator is leased by another run' "$OUT/lease.err"; then
    echo "NOT RUN prepared simulator is leased by another run; no simulator test ran"; exit 2
  fi
  echo "FAIL simctl create"; exit 1
}
cleanup() {
  python3 "$HERE/../../engine/scripts/lib/testdevices.py" release-ios --id "$UDID" --owner-pid $$ >&2 || return 1
}
trap 'rc=$?; cleanup || rc=1; exit "$rc"' EXIT
trap 'exit 130' HUP INT TERM
# A run killed before its trap leaves this simulator behind; registering it with this shell's pid lets
# the engine remove it once this shell is gone (richos/engine/scripts/lib/testdevices.py, §54).
python3 "$HERE/../../engine/scripts/lib/testdevices.py" register --kind ios-simulator --id "$UDID" \
  --owner-pid $$ --script simulator-tests.sh >/dev/null || { echo "FAIL device ownership registration" >&2; exit 1; }

python3 "$HERE/../../engine/scripts/lib/testdevices.py" boot-ios --id "$UDID" >/dev/null || { echo "FAIL simctl boot $UDID"; exit 1; }

# A prepared simulator may have no app installed. Installing after a privacy grant can
# discard it, leaving the async permission test waiting on an unattended OS prompt.
# Build once, install these exact products, then require the grant before running them.
python3 "$HERE/../../engine/scripts/lib/testdevices.py" run-active \
  --kind ios-simulator --id "$UDID" --owner-pid $$ -- \
  python3 "$HERE/../../engine/scripts/lib/native-work.py" -- xcodebuild build-for-testing -project "$PROJECT" -scheme RichOSPlatformTests \
  -destination "platform=iOS Simulator,id=$UDID" -derivedDataPath "$OUT/DerivedData" \
  -clonedSourcePackagesDirPath "$OUT/SourcePackages" CODE_SIGN_IDENTITY=- >"$OUT/logs/build.log" 2>&1
CODE=$?
if [ "$CODE" -eq 75 ]; then
  echo "NOT RUN simulator tests: this run's simulator lease ended during the build; full log $OUT/logs/build.log"
  exit 75
elif [ "$CODE" -ne 0 ]; then
  echo "FAIL build-for-testing; full log $OUT/logs/build.log"
  exit 1
fi
xcrun simctl install "$UDID" "$OUT/DerivedData/Build/Products/Debug-iphonesimulator/RichOSNative.app" \
  || { echo "FAIL installing the simulator test host"; exit 1; }
xcrun simctl privacy "$UDID" grant microphone dev.richos.connect \
  || { echo "FAIL granting the simulator test host microphone permission"; exit 1; }

# run-active renews the lease's inactivity clock while this build-and-test run lives; without it a
# run longer than five minutes lost its simulator to the collector (esc-20260924T220236Z-52fae3ec).
TEST_RUNNER_RICHOS_SNAPSHOT_DIR="$OUT/snapshots" python3 "$HERE/../../engine/scripts/lib/testdevices.py" run-active \
  --kind ios-simulator --id "$UDID" --owner-pid $$ -- \
  python3 "$HERE/../../engine/scripts/lib/native-work.py" -- xcodebuild test-without-building -project "$PROJECT" -scheme RichOSPlatformTests \
  -destination "platform=iOS Simulator,id=$UDID" -derivedDataPath "$OUT/DerivedData" \
  -clonedSourcePackagesDirPath "$OUT/SourcePackages" -resultBundlePath "$OUT/result.xcresult" \
  CODE_SIGN_IDENTITY=- >"$OUT/logs/test.log" 2>&1
CODE=$?
# 75: the lease ended mid-run and run-active stopped the run (esc-20260925T014934Z-0a4bf206).
# Whatever the result bundle holds may be another run's device's doing: NOT RUN, never a verdict.
if [ "$CODE" -eq 75 ]; then
  grep 'LEASE LOST' "$OUT/logs/test.log" | head -1
  echo "NOT RUN simulator tests: this run's simulator lease ended mid-run; full log $OUT/logs/test.log"
  exit 75
fi

SUMMARY="$(xcrun xcresulttool get test-results summary --path "$OUT/result.xcresult" 2>/dev/null)"
COUNTS="$(printf '%s' "$SUMMARY" | python3 -c 'import json,sys
try: d=json.load(sys.stdin)
except Exception: print("0 0 0"); sys.exit()
print(d.get("totalTestCount",0), d.get("passedTests",0), d.get("failedTests",0))')"
read -r TOTAL PASSED FAILED <<<"$COUNTS"
SHOTS="$(find "$OUT/snapshots" -name '*.png' 2>/dev/null | wc -l | tr -d ' ')"
echo "simulator tests: $PASSED passed, $FAILED failed of $TOTAL; $SHOTS share-sheet snapshots in $OUT/snapshots; device $UDID (shut down on exit; prepared OS retained)"
if [ "$CODE" -ne 0 ] || [ "${TOTAL:-0}" -eq 0 ] || [ "${FAILED:-1}" -ne 0 ]; then
  grep -E 'error:|failed|XCTAssert' "$OUT/logs/test.log" | head -20
  echo "FAIL full log $OUT/logs/test.log"
  exit 1
fi
