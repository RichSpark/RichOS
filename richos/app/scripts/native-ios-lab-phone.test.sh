#!/usr/bin/env bash
# lab-phone (richos/mobile/native-ios/Tools/LabPhone): it builds against the current core, and it
# refuses before touching the network whatever is not an isolated lab: no flags, a lab whose data
# directory lacks the owner marker, a lab with no open pairing link. No network, no device, no
# simulator, no window; the first build of a checkout takes about a minute, later ones seconds.
#
# Its real run (pairing an isolated lab through the managed route and keeping a send in flight
# across "Home" with qa/lab-pause.py) needs a live lab and is a walker's or engineer's check, not
# this suite's.
# run-tests: no-host-screen: a Swift build and three refusals; nothing is drawn, captured or pressed
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/app/scripts/native-ios-lab-phone.test.sh richos/mobile/native-ios/Tools/LabPhone richos/mobile/native-ios/Core
# run-tests: covers richos/mobile/native-ios/Tools/LabPhone/Package.swift richos/mobile/native-ios/Tools/LabPhone/Sources/lab-phone/main.swift
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
PKG="$ROOT/richos/mobile/native-ios/Tools/LabPhone"

if ! command -v swift >/dev/null 2>&1; then
  echo "  NOT RUN  native-ios-lab-phone: swift is unavailable"
  exit 2
fi
MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2
KEY=$(printf '%s' "$ROOT" | shasum | cut -c1-10)
BUILD="$CACHE_ROOT/richos-native-ios/lab-phone-$KEY"
SCRATCH=$(mktemp -d "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}/lab-phone-test.XXXXXX")
trap 'rm -rf "$SCRATCH"' EXIT

if ! swift build --package-path "$PKG" --scratch-path "$BUILD" > "$SCRATCH/build.log" 2>&1; then
  tail -30 "$SCRATCH/build.log"
  echo "=== native-ios-lab-phone: FAILED (build) ==="
  exit 1
fi
BIN="$(swift build --package-path "$PKG" --scratch-path "$BUILD" --show-bin-path)/lab-phone"
failed=0
expect_refusal() {  # NAME EXPECTED-TEXT ARGS...
  local name=$1 text=$2; shift 2
  local out code
  out=$("$BIN" "$@" 2>&1); code=$?
  if [ "$code" -eq 2 ] && printf '%s' "$out" | grep -qF -- "$text"; then
    echo "  ok    $name"
  else
    echo "  FAIL  $name: exit $code, said: $out"
    failed=1
  fi
}

expect_refusal "no flags" "--lab and --marks are required"
mkdir -p "$SCRATCH/lab/data"
printf '{"pairLink":"https://example.invalid/#pair=CODE","data":"%s"}' "$SCRATCH/lab/data" > "$SCRATCH/lab/mac.json"
expect_refusal "a lab without the owner marker" "no isolated lab owner marker" --lab "$SCRATCH/lab" --marks "$SCRATCH/marks.log"
printf 'richos-mobile-isolated-v1' > "$SCRATCH/lab/data/lab-owner"
printf '{"data":"%s"}' "$SCRATCH/lab/data" > "$SCRATCH/lab/mac.json"
expect_refusal "a lab with no open pairing link" "no open pairing link" --lab "$SCRATCH/lab" --marks "$SCRATCH/marks.log"

if [ "$failed" -eq 0 ]; then
  echo "=== native-ios-lab-phone: passed ==="
else
  echo "=== native-ios-lab-phone: FAILED ==="
  exit 1
fi
