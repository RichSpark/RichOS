#!/usr/bin/env bash
# The sentence macOS shows when RichOS asks for the microphone names only what is true on every
# install (product hunt 2026-10-15, finding 20). Iris's sentence, approved 2026-10-08, promised
# "I listen only after you tap F1 or press the talk button", and macOS showed it on the first
# press of Talk to Rich while dictation was off, when F1 did nothing. An app cannot change this
# string at run time, so it names no key: the whole string is compared, and a key name in it
# fails on its own line so the reason is plain.
# run-tests: no-host-screen: reads one property list; nothing is built, opened or asked
# run-tests: inputs richos/app/scripts/microphone-sentence.test.sh richos/app/src-tauri/Info.plist
# run-tests: covers richos/app/src-tauri/Info.plist
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
want='So you can talk instead of typing. I listen only when you ask me to.'
got="$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' Info.plist)"
if printf '%s' "$got" | grep -Eq '\bF[0-9]+\b'; then
  echo "FAIL  NSMicrophoneUsageDescription names a key, which does nothing while dictation is off"
  echo "      actual:   $got"
  exit 1
fi
if [ "$got" != "$want" ]; then
  echo "FAIL  NSMicrophoneUsageDescription is not the sentence this test holds"
  echo "      expected: $want"
  echo "      actual:   $got"
  exit 1
fi
echo "PASS  NSMicrophoneUsageDescription names no key and is the sentence this test holds"
