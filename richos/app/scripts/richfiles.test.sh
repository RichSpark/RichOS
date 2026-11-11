#!/usr/bin/env bash
# run-tests: no-host-screen: native tests with synthetic files, a separate pasteboard and a local development bundle
# run-tests: inputs richos/app/scripts/richfiles.test.sh richos/tools/richfiles richos/engine/scripts/lib/native-work.py richos/engine/scripts/lib/machine-settings.sh richos/engine/scripts/lib/machine_settings.py richos/app/scripts/package-richfiles.sh richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/lib/cargo_identity.py richos/app/scripts/bin/cargo
# run-tests: covers richos/app/scripts/package-richfiles.sh richos/tools/richfiles/Info.plist richos/tools/richfiles/Package.swift richos/tools/richfiles/Sources/RichFiles/ArchiveTools.swift richos/tools/richfiles/Sources/RichFiles/BrowserActions.swift richos/tools/richfiles/Sources/RichFiles/BrowserModel.swift richos/tools/richfiles/Sources/RichFiles/BrowserSheets.swift richos/tools/richfiles/Sources/RichFiles/BrowserTransfer.swift richos/tools/richfiles/Sources/RichFiles/BrowserView.swift richos/tools/richfiles/Sources/RichFiles/KeyboardBridge.swift richos/tools/richfiles/Sources/RichFiles/RichFilesApp.swift richos/tools/richfiles/Sources/RichFiles/RichOSServices.swift richos/tools/richfiles/Sources/RichFilesApp/Application.swift richos/tools/richfiles/Sources/RichFilesCore/BatchRename.swift richos/tools/richfiles/Sources/RichFilesCore/DirectoryReader.swift richos/tools/richfiles/Sources/RichFilesCore/FileOperations.swift richos/tools/richfiles/Sources/RichFilesCore/FolderComparison.swift richos/tools/richfiles/Sources/RichFilesCore/Locations.swift richos/tools/richfiles/Sources/RichFilesCore/StorageAnalysis.swift richos/tools/richfiles/Sources/RichFilesCore/TextContentSearch.swift richos/tools/richfiles/Sources/RichFilesCore/WorkspaceServices.swift richos/tools/richfiles/Tests/RichFilesCoreTests/BatchRenameTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/DirectoryReaderTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/FileOperationsTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/TextContentSearchTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/WorkspaceServicesTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserActionsTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserModelTests.swift richos/tools/richfiles/archive/Cargo.lock richos/tools/richfiles/archive/Cargo.toml richos/tools/richfiles/archive/src/archive.rs richos/tools/richfiles/archive/src/main.rs richos/tools/richfiles/archive/src/native.rs richos/tools/richfiles/archive/src/tests.rs
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

# Verify the same bundle and ZIP executable a development build launches.
TMPDIR="$scratch" bash "$root/richos/app/scripts/package-richfiles.sh" "$scratch/package"
app="$scratch/package/RichFiles.app"
helper="$app/Contents/MacOS/richfiles-archive"
printf '%s\n' 'ordinary archive fixture' > "$scratch/source.txt"
"$helper" create "$scratch" fixture.zip -- "$scratch/source.txt"
"$helper" list "$scratch/fixture.zip" > "$scratch/archive-list.json"
python3 - "$scratch/archive-list.json" <<'PY'
import json
import sys
with open(sys.argv[1]) as handle:
    entries = json.load(handle)
assert [(entry['index'], entry['path']) for entry in entries] == [(0, 'source.txt')]
PY
"$helper" extract "$scratch/fixture.zip" "$scratch" Extracted -- 0
cmp "$scratch/source.txt" "$scratch/Extracted/source.txt"
printf '%s\n' '  ok  packaged ZIP helper created, listed and extracted the fixture'
