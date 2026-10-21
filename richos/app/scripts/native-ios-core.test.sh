#!/usr/bin/env bash
# The RichOS native iPhone app's core, headless: the Swift package's unit tests and the `bin/rios`
# command line over the real core, on this Mac with no simulator (build plan §3.1 loops L1, L1′;
# stream I1). A host without Swift or the external SSD prints NOT RUN and exits 2; a failure on a
# capable host is red.
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/app/scripts/native-ios-lock.test.py richos/mobile/native-ios/Core richos/mobile/native-ios/bin richos/mobile/perf/condition.py docs/verification/2026-09-17-dev-walk/18-text-sent.png richos/web/web-app/lib/wordlist.js richos/mobile/conformance/vectors richos/app/scripts/native-ios-core.test.sh
# run-tests: covers richos/app/scripts/native-ios-lock.test.py richos/mobile/native-ios/Core/Package.swift richos/mobile/native-ios/Core/Sources/RichOSCore/AppState.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Action.swift richos/mobile/native-ios/Core/Sources/RichOSCore/ActionCoding.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Ports.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Pairing/PairLink.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Pairing/PairingReducer.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Pairing/WordList.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Voice/VoiceGeometry.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Voice/VoiceReducer.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Settings/SettingsReducer.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Settings/AppLinks.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/AttachmentPicking.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/ConversationReducer.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/Attachments.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/AttachmentTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Connection/ConnectionReducer.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Connection/TailscaleTunnel.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Connection/NetworkPath.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Connection/LiveConnection.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Connection/NetworkEffects.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/Identity.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/NetworkEffectsTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/LiveConnectionTests.swift richos/mobile/native-ios/Core/Sources/RichOSFixtures/Commands.swift richos/mobile/native-ios/Core/Sources/RichOSFixtures/Fixtures.swift richos/mobile/native-ios/Core/Sources/RichOSCLI/main.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/CoreTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/AttachmentPickingTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/PairingRemovedTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/SettingsLinksTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/ConformanceTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/Signing.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/Stream.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/APIClient.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/Delivery.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Protocol/PairingAPI.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/PairingConformanceTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/TransportConformanceTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/StreamConformanceTests.swift richos/mobile/native-ios/bin/rios richos/mobile/native-ios/Core/Sources/RichOSCore/TickSchedule.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/TickScheduleTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/LifecycleTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/AppWiringTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/AppStore.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/AppStoreDurabilityTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Pairing/MacWait.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/PairingV2Tests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/Transcript.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/EchoBeforeAcceptanceTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/PairWaitTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/ReadReplies.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/ReadRepliesTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/MicrophoneCardTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/HungMacTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/TailscaleOffTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/Questions.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/QuestionsTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/CompletionTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/BackgroundSendTimeTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/SendThenHomeTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/SilentStreamOutboxTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/SilentStreamTests.swift richos/mobile/native-ios/Core/Sources/RichOSPerfSeed/PerfSeed.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/PerfSeedTests.swift richos/mobile/native-ios/Core/Sources/RichOSCLI/ScreenText.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/ReopenAfterHelloTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/ReturnBackOffTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/ReturnStreamProbeTests.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/RouteChangeTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/ReplyAudio.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/ReplyAudioTests.swift richos/mobile/native-ios/Core/Sources/RichOSCore/Conversation/RichFiles.swift richos/mobile/native-ios/Core/Tests/RichOSCoreTests/RichFileTests.swift
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
RIOS="$ROOT/richos/mobile/native-ios/bin/rios"
MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2

if [ "$(uname -s)" != Darwin ]; then echo '  NOT RUN  native-ios-core: macOS is required'; exit 2; fi
if ! command -v swift >/dev/null 2>&1; then echo '  NOT RUN  native-ios-core: swift is unavailable'; exit 2; fi

# The build cache is reused across runs of this checkout (a cold core build is ~20 s); the
# headless session inside it is reset at the start and removed at the end.
KEY="$(printf '%s' "$ROOT" | shasum -a 256 | cut -c1-10)"
export RICHOS_NATIVE_IOS_CACHE="$CACHE_ROOT/richos-native-ios-proof/$KEY"
SCRATCH="$(mktemp -d "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}/native-ios-core.XXXXXX")" || { echo '  FAIL  scratch directory'; exit 1; }
trap 'rm -rf "$SCRATCH" "$RICHOS_NATIVE_IOS_CACHE/headless"' EXIT HUP INT TERM

PASS=0; FAILED=0
ok()  { PASS=$((PASS + 1)); echo "  ok  $1"; }
bad() { FAILED=$((FAILED + 1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; return 0; }
json() { python3 -c 'import json,sys; d=json.load(sys.stdin); print(eval(sys.argv[1], {"d": d}))' "$1"; }

echo "=== native-ios-core ==="

# C1 — the core's unit tests (loop L1). The count is read back so a run of nothing cannot pass.
if "$RIOS" test >"$SCRATCH/test.log" 2>&1; then
  COUNT="$(grep -Eo 'Test run with [0-9]+ tests?( in [0-9]+ suites?)? passed' "$SCRATCH/test.log" | grep -Eo 'with [0-9]+' | grep -Eo '[0-9]+' | tail -1)"
  if [ "${COUNT:-0}" -gt 0 ]; then ok "C1 core unit tests: $COUNT passed (bin/rios test)"
  else bad "C1 core unit tests" "no passing test count in the output: $(tail -3 "$SCRATCH/test.log" | tr '\n' ' ')"; fi
else
  bad "C1 core unit tests" "$(grep -E 'error:|✘|failed' "$SCRATCH/test.log" | head -5 | tr '\n' ' ')"
  # Every failing test by name, and the whole log kept with the run (lib/test_results.py): the
  # five lines above are a digest, and this log is in scratch that is deleted on exit.
  python3 "$DIR/lib/test_results.py" report --label "native-ios-core C1" \
    ${RICHOS_TEST_RESULTS_DIR:+--into "$RICHOS_TEST_RESULTS_DIR"} --collect "core-tests.log=$SCRATCH/test.log"
fi

# C2 — a scenario through the real core, as a separate CLI process (loop L1′).
if "$RIOS" headless reset >/dev/null 2>"$SCRATCH/err" \
   && "$RIOS" headless scenario compose-draft >"$SCRATCH/scenario.json" 2>"$SCRATCH/err"; then
  if [ "$(json 'd["ok"] and d["result"]["name"] == "compose-draft" and len(d["result"]["trace"]) == 6' < "$SCRATCH/scenario.json")" = True ]; then
    ok "C2 headless scenario compose-draft: six steps, its checks passed"
  else bad "C2 headless scenario compose-draft" "unexpected result: $(head -c 300 "$SCRATCH/scenario.json")"; fi
else
  bad "C2 headless scenario compose-draft" "$(cat "$SCRATCH/err")"
fi

# C3 — state persists across separate CLI processes, as it will across app launches.
"$RIOS" headless fixture conv-empty >/dev/null 2>&1
"$RIOS" headless action '{"type":"compose","text":"Persisted across processes"}' >/dev/null 2>&1
if "$RIOS" headless state >"$SCRATCH/state.json" 2>"$SCRATCH/err" \
   && [ "$(json 'd["result"]["state"]["draft"] + "|" + d["result"]["state"]["screen"]' < "$SCRATCH/state.json")" = "Persisted across processes|conv-empty" ]; then
  ok "C3 a draft written by one CLI process is read by the next"
else
  bad "C3 a draft written by one CLI process is read by the next" "$(head -c 300 "$SCRATCH/state.json") $(cat "$SCRATCH/err")"
fi

# C4 — a refusal is structured: JSON on stderr naming what exists, exit 1, nothing on stdout.
"$RIOS" headless fixture no-such-screen >"$SCRATCH/out" 2>"$SCRATCH/err"; CODE=$?
if [ "$CODE" = 1 ] && [ ! -s "$SCRATCH/out" ] \
   && [ "$(json 'd["ok"] is False and "known: pair-intro" in d["error"]' < "$SCRATCH/err")" = True ]; then
  ok "C4 an unknown fixture is a structured refusal listing the known ones"
else
  bad "C4 an unknown fixture is a structured refusal" "exit $CODE; stderr $(head -c 300 "$SCRATCH/err")"
fi

# C5: a selected missing external volume refuses before any cache write.
MISSING_VOLUME="/Volumes/RichOSMissingFixture-$$" # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
RICHOS_NATIVE_IOS_CACHE="$MISSING_VOLUME/cache" "$RIOS" headless state >/dev/null 2>"$SCRATCH/err2"; CODE2=$?
if [ "$CODE2" = 2 ] && grep -q 'not mounted' "$SCRATCH/err2" && [ ! -e "$MISSING_VOLUME" ]; then
  ok "C5 a missing selected cache volume is refused before writes"
else
  bad "C5 missing selected volume" "exit $CODE2: $(cat "$SCRATCH/err2")"
fi

# C6: the built CLI refuses overlapping owners and recovers after a hard kill.
# Its throwaway cache uses this suite's TMPDIR and preserves kernel lock ownership.
if TMPDIR="$SCRATCH" python3 "$DIR/native-ios-lock.test.py" "$RICHOS_NATIVE_IOS_CACHE/spm/debug/rios-cli" >"$SCRATCH/lock.log" 2>&1; then
  ok "C6 $(tail -1 "$SCRATCH/lock.log")"
else
  bad "C6 command-lock crash recovery" "$(tail -15 "$SCRATCH/lock.log")"
fi

# C7: `rios perf-seed` (a separate CLI process) turns condition.py's own fixture into the app's
# saved state; a manifest that is not the fixture's is a structured refusal that writes nothing.
PERF="$ROOT/richos/mobile/perf"
EXPECTED="$(PYTHONDONTWRITEBYTECODE=1 python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import condition
condition.write_fixture(100, sys.argv[2]); print(condition.for_files(100, "release", "-")["conversation"]["sha256"])' "$PERF" "$SCRATCH/fixture")"
SEEDED='import json,sys; r=json.load(open(sys.argv[1]))["result"]
print(r["rows"] == 100 and r["fixtureSha256"] == sys.argv[2] and sorted(r["written"]) == ["history.json", "state.json"] and r["newestID"] == "row-00099")'
REFUSED='import json,sys; print(json.load(open(sys.argv[1]))["ok"] is False)'
if "$RIOS" perf-seed "$SCRATCH/fixture" "$EXPECTED" "$SCRATCH/seeded" >"$SCRATCH/seed.json" 2>"$SCRATCH/err" \
   && [ "$(python3 -c "$SEEDED" "$SCRATCH/seed.json" "$EXPECTED")" = True ]; then
  if "$RIOS" perf-seed "$SCRATCH/fixture" "$(printf '0%.0s' $(seq 64))" "$SCRATCH/refused" >"$SCRATCH/out" 2>"$SCRATCH/err" \
     || [ -s "$SCRATCH/out" ] || [ -e "$SCRATCH/refused" ] || [ "$(python3 -c "$REFUSED" "$SCRATCH/err")" != True ]; then
    bad "C7 rios perf-seed refuses a manifest that is not the fixture's" "$(head -c 300 "$SCRATCH/err")"
  else
    ok "C7 rios perf-seed writes condition.py's 100-row fixture as the app's saved state; a wrong manifest is refused, nothing written"
  fi
else
  bad "C7 rios perf-seed" "$(head -c 300 "$SCRATCH/seed.json") $(head -c 300 "$SCRATCH/err")"
fi

# C8: `rios screen-text` reads a committed screenshot's text, top to bottom (Vision).
SHOT="$ROOT/docs/verification/2026-09-17-dev-walk/18-text-sent.png"
if "$RIOS" screen-text "$SHOT" >"$SCRATCH/text.json" 2>"$SCRATCH/err" \
   && [ "$(python3 -c 'import json,sys; lines=json.load(open(sys.argv[1]))["result"]["lines"]; text=" ".join(" ".join(lines).split())
print("What is 17 times 3? Answer in one short sentence." in text and text.index("business yet") < text.index("17 times 3") < text.index("Pick it back up"))' "$SCRATCH/text.json")" = True ]; then
  ok "C8 rios screen-text reads a screenshot's sentences in screen order"
else
  bad "C8 rios screen-text" "$(head -c 400 "$SCRATCH/text.json") $(head -c 300 "$SCRATCH/err")"
fi

if [ "$FAILED" -eq 0 ]; then
  echo "=== native-ios-core tests: all $PASS passed ==="
  exit 0
fi
echo "=== native-ios-core tests: $FAILED FAILED, $PASS passed ==="
exit 1
