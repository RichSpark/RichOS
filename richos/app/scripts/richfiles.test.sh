#!/usr/bin/env bash
# run-tests: no-host-screen: Swift tests use synthetic files in disposable scratch
# run-tests: inputs richos/app/scripts/richfiles.test.sh richos/tools/richfiles richos/engine/scripts/lib/native-work.py richos/engine/scripts/lib/machine-settings.sh richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/tools/richfiles/Package.swift richos/tools/richfiles/Sources/RichFilesCore/DirectoryReader.swift richos/tools/richfiles/Sources/RichFilesCore/Locations.swift richos/tools/richfiles/Tests/RichFilesCoreTests/DirectoryReaderTests.swift richos/tools/richfiles/Sources/RichFilesCore/FileOperations.swift richos/tools/richfiles/Tests/RichFilesCoreTests/FileOperationsTests.swift
set -euo pipefail

if [ "$(uname -s)" != Darwin ]; then
  echo '  NOT RUN  richfiles: macOS is required'
  exit 2
fi
if ! command -v swift >/dev/null 2>&1; then
  echo '  NOT RUN  richfiles: Swift is unavailable'
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)"
machine_lib="$root/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$machine_lib/machine-settings.sh"
richos_machine_settings
cache="$(python3 "$machine_lib/machine_settings.py" cache)"
checkout_key="$(printf '%s' "$root" | shasum -a 256 | cut -c1-10)"
scratch="$(mktemp -d "${TMPDIR:?}/richfiles-test.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT

TMPDIR="$scratch" python3 "$machine_lib/native-work.py" -- swift test \
  --package-path "$root/richos/tools/richfiles" \
  --scratch-path "$cache/richfiles/proof/$checkout_key" >"$scratch/swift-test.log" 2>&1 || {
    cat "$scratch/swift-test.log"
    exit 1
  }
cat "$scratch/swift-test.log"
count="$(awk '/Executed [1-9][0-9]* tests?, with 0 failures/ { count = $2 } END { print count + 0 }' "$scratch/swift-test.log")"
if [ "$count" -eq 0 ]; then
  echo '  FAIL  richfiles: Swift did not report any passing XCTest cases' >&2
  exit 1
fi
printf '  ok  richfiles: %s XCTest cases passed\n' "$count"
