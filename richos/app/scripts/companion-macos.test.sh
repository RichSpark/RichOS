#!/usr/bin/env bash
# The macOS capture companion (`richos/tools/richos-service/companion-macos`): its Swift package's
# unit tests, and the `watch` command's lifetime contract, on this Mac with no permission prompt.
# Until 2026-10-13 no suite claimed the companion's sources, so a change to the capture contract,
# the drop zone or the meeting watcher was UNCOVERED at commit (Granola PRD V4 work).
# A host without Swift prints NOT RUN and exits 2; a failure on a capable host is red.
# run-tests: inputs richos/app/scripts/companion-macos.test.sh richos/app/scripts/prepare-companion.py richos/app/scripts/lib/machine_storage.py richos/tools/richos-service/companion-macos richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh
# run-tests: covers richos/app/scripts/prepare-companion.py richos/tools/richos-service/companion-macos/Package.swift richos/tools/richos-service/companion-macos/Sources/richos-companion/CompanionCoordinator.swift richos/tools/richos-service/companion-macos/Sources/richos-companion/CoreAudioCapture.swift richos/tools/richos-service/companion-macos/Sources/richos-companion/LiveCapture.swift richos/tools/richos-service/companion-macos/Sources/richos-companion/main.swift richos/tools/richos-service/companion-macos/Sources/richos-companion/Notifier.swift richos/tools/richos-service/companion-macos/Sources/richos-companion/Watch.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/ChannelMixer.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/Coordination.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/DropZone.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/JSON.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/MicActivity.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/MixDecision.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/RingBuffer.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/SessionContract.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/SessionWriter.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/WavReader.swift richos/tools/richos-service/companion-macos/Sources/RichOSCompanionCore/WavWriter.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/ChannelMixerTests.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/CoordinationTests.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/DropZoneTests.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/MicActivityTests.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/MixDecisionTests.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/SessionContractTests.swift richos/tools/richos-service/companion-macos/Tests/RichOSCompanionCoreTests/SessionWriterTests.swift
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
PKG="$ROOT/richos/tools/richos-service/companion-macos"
MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2

if [ "$(uname -s)" != Darwin ]; then echo '  NOT RUN  companion-macos: macOS is required'; exit 2; fi
if ! command -v swift >/dev/null 2>&1; then echo '  NOT RUN  companion-macos: swift is unavailable'; exit 2; fi

# The build cache is reused across runs of this checkout; the scratch for logs is removed at the end.
KEY="$(printf '%s' "$ROOT" | shasum -a 256 | cut -c1-10)"
BUILD="$CACHE_ROOT/richos-companion-proof/$KEY"
SCRATCH="$(mktemp -d "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}/companion-macos.XXXXXX")" || { echo '  FAIL  scratch directory'; exit 1; }
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM
NATIVE_WORK="$ROOT/richos/engine/scripts/lib/native-work.py"

PASS=0; FAILED=0
ok()  { PASS=$((PASS + 1)); echo "  ok  $1"; }
bad() { FAILED=$((FAILED + 1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; return 0; }

echo "=== companion-macos ==="

# C1 — the package's unit tests. The count is read back so a run of nothing cannot pass.
if python3 "$NATIVE_WORK" -- swift test --package-path "$PKG" --scratch-path "$BUILD" >"$SCRATCH/test.log" 2>&1; then
  COUNT="$(grep -E 'Executed [0-9]+ tests?, with 0 failures' "$SCRATCH/test.log" | grep -Eo 'Executed [0-9]+' | grep -Eo '[0-9]+' | tail -1)"
  if [ "${COUNT:-0}" -gt 0 ]; then ok "C1 unit tests: $COUNT passed"
  else bad "C1 unit tests" "no passing test count in the output: $(tail -3 "$SCRATCH/test.log" | tr '\n' ' ')"; fi
else
  bad "C1 unit tests" "$(grep -E 'error:|failed' "$SCRATCH/test.log" | head -5 | tr '\n' ' ')"
fi

# C2 — `watch` says hello and EXITS BY ITSELF when its stdin closes: the app holds that pipe, so a
# watcher can never outlive the app that started it. Reading the process list needs no permission.
BIN="$BUILD/debug/richos-companion"
if [ ! -x "$BIN" ]; then
  python3 "$NATIVE_WORK" -- swift build --package-path "$PKG" --scratch-path "$BUILD" >"$SCRATCH/build.log" 2>&1 || true
fi
if [ -x "$BIN" ]; then
  START=$(python3 -c 'import time; print(time.time())')
  if (sleep 1 | "$BIN" watch >"$SCRATCH/watch.out" 2>"$SCRATCH/watch.err"); then
    ELAPSED=$(python3 -c "import time; print(round(time.time() - $START, 1))")
    if python3 -c 'import json,sys; d=json.loads(open(sys.argv[1]).readline()); assert d["type"]=="hello" and d["pid"]>0' "$SCRATCH/watch.out" 2>/dev/null; then
      ok "C2 watch says hello and exits when stdin closes (${ELAPSED}s)"
    else bad "C2 watch hello line" "$(head -c 300 "$SCRATCH/watch.out") $(head -c 300 "$SCRATCH/watch.err")"; fi
  else bad "C2 watch exit status" "$(head -c 300 "$SCRATCH/watch.err")"; fi
  # C3 — a malformed exclusion is refused, never silently ignored (it would let RichOS's own
  # microphone use prompt for a call).
  if "$BIN" watch --exclude-pid notanumber </dev/null >/dev/null 2>&1; then bad "C3 bad --exclude-pid refused" "it exited 0"
  else ok "C3 bad --exclude-pid refused"; fi
else
  bad "C2 build richos-companion" "$(tail -5 "$SCRATCH/build.log" 2>/dev/null | tr '\n' ' ')"
fi

# C4 — the bundle build's cache key (scripts/prepare-companion.py, which puts this package inside
# RichOS.app): deterministic over the package's sources, so an unchanged companion is never rebuilt
# and a changed one always is. Builds nothing.
PREP="$ROOT/richos/app/scripts/prepare-companion.py"
K1="$(python3 "$PREP" --check 2>"$SCRATCH/prep.err")"; K2="$(python3 "$PREP" --check 2>>"$SCRATCH/prep.err")"
if python3 -c 'import json,re,sys; a=json.loads(sys.argv[1]); b=json.loads(sys.argv[2]); assert a==b and re.fullmatch(r"[0-9a-f]{16}", a["key"]) and isinstance(a["cached"], bool)' "$K1" "$K2" 2>/dev/null; then
  ok "C4 prepare-companion.py --check: a stable source key ($(printf '%s' "$K1" | python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])'))"
else
  bad "C4 prepare-companion.py --check" "$K1 / $K2 $(head -c 300 "$SCRATCH/prep.err")"
fi

echo "companion-macos: $PASS passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
