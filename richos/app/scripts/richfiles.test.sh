#!/usr/bin/env bash
# run-tests: no-host-screen: native tests with synthetic files, a separate pasteboard and a local development bundle
# run-tests: inputs richos/mobile/native-ios/App/Design/Fonts richos/app/scripts/richfiles.test.sh richos/tools/richfiles richos/engine/scripts/lib/native-work.py richos/engine/scripts/lib/machine-settings.sh richos/engine/scripts/lib/machine_settings.py richos/app/scripts/package-richfiles.sh richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/lib/cargo_identity.py richos/app/scripts/bin/cargo
# run-tests: covers richos/app/scripts/package-richfiles.sh richos/tools/richfiles/Info.plist richos/tools/richfiles/Package.swift richos/tools/richfiles/Resources/RichFiles.icns richos/tools/richfiles/Sources/RichFiles/ArchiveTools.swift richos/tools/richfiles/Sources/RichFiles/BrowserAppearance.swift richos/tools/richfiles/Sources/RichFiles/BrowserActions.swift richos/tools/richfiles/Sources/RichFiles/BrowserDragSurface.swift richos/tools/richfiles/Sources/RichFiles/BrowserModel.swift richos/tools/richfiles/Sources/RichFiles/BrowserSheets.swift richos/tools/richfiles/Sources/RichFiles/BrowserTransfer.swift richos/tools/richfiles/Sources/RichFiles/BrowserView.swift richos/tools/richfiles/Sources/RichFiles/BrowserWindowLayout.swift richos/tools/richfiles/Sources/RichFiles/DetailsViewport.swift richos/tools/richfiles/Sources/RichFiles/KeyboardBridge.swift richos/tools/richfiles/Sources/RichFiles/RichFilesApp.swift richos/tools/richfiles/Sources/RichFiles/RichFilesDiagnostics.swift richos/tools/richfiles/Sources/RichFiles/RichOSServices.swift richos/tools/richfiles/Sources/RichFiles/TransferJournal.swift richos/tools/richfiles/Sources/RichFiles/TrashIndex.swift richos/tools/richfiles/Sources/RichFiles/WindowWorkspace.swift richos/tools/richfiles/Sources/RichFiles/WorkspaceHandoff.swift richos/tools/richfiles/Sources/RichFilesApp/Application.swift richos/tools/richfiles/Sources/RichFilesRail/RailHost.swift richos/tools/richfiles/Sources/RichFilesCore/BatchRename.swift richos/tools/richfiles/Sources/RichFilesCore/DirectoryReader.swift richos/tools/richfiles/Sources/RichFilesCore/FileOperations.swift richos/tools/richfiles/Sources/RichFilesCore/FolderComparison.swift richos/tools/richfiles/Sources/RichFilesCore/FolderSync.swift richos/tools/richfiles/Sources/RichFilesCore/ImageQuickActions.swift richos/tools/richfiles/Sources/RichFilesCore/Locations.swift richos/tools/richfiles/Sources/RichFilesCore/StorageAnalysis.swift richos/tools/richfiles/Sources/RichFilesCore/TextContentSearch.swift richos/tools/richfiles/Sources/RichFilesCore/VerifiedTree.swift richos/tools/richfiles/Sources/RichFilesCore/WorkspaceServices.swift richos/tools/richfiles/Tests/RichFilesCoreTests/BatchRenameTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/DirectoryReaderTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/FileOperationsTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/TextContentSearchTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/WorkspaceServicesTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserActionsTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserAppearanceTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserModelTests.swift richos/tools/richfiles/archive/Cargo.lock richos/tools/richfiles/archive/Cargo.toml richos/tools/richfiles/archive/src/archive.rs richos/tools/richfiles/archive/src/main.rs richos/tools/richfiles/archive/src/native.rs richos/tools/richfiles/archive/src/stream.rs richos/tools/richfiles/archive/src/tests.rs
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
temporary="$(cd "${TMPDIR:?}" && pwd -P)"
scratch="$(mktemp -d "$temporary/richfiles-test.XXXXXX")"
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
cmp "$root/richos/tools/richfiles/Resources/RichFiles.icns" "$app/Contents/Resources/RichFiles.icns"
test "$(plutil -extract CFBundleIconFile raw "$app/Contents/Info.plist")" = RichFiles.icns
for font in Inter-Variable.ttf Newsreader-Regular.ttf LICENSE-Inter.txt LICENSE-Newsreader.txt; do
  cmp "$root/richos/mobile/native-ios/App/Design/Fonts/$font" "$app/Contents/Resources/$font"
done
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

# RichFiles sends the identity of the folder it showed before starting the helper.
destination="$scratch/Chosen Destination"
parked="$scratch/Original Destination"
mkdir "$destination"
destination_identity="$(/usr/bin/stat -f '%d:%i' "$destination")"
mv "$destination" "$parked"
mkdir "$destination"
printf '%s\n' 'The destination folder changed. Choose it again.' > "$scratch/expected-archive-error"
expect_archive_destination_changed() {
  if "$helper" --destination-identity "$destination_identity" "$@" > "$scratch/archive-error" 2>&1; then
    echo '  FAIL  packaged archive helper accepted a substitute destination' >&2
    exit 1
  fi
  cmp "$scratch/expected-archive-error" "$scratch/archive-error"
}
expect_archive_destination_changed create "$destination" Pinned.zip -- "$scratch/source.txt"
expect_archive_destination_changed extract "$scratch/fixture.zip" "$destination" Pinned -- 0
test -z "$(ls -A "$destination")"
test -z "$(ls -A "$parked")"
rmdir "$destination"
mv "$parked" "$destination"
ln -s "$destination" "$scratch/Destination Alias"
"$helper" --destination-identity "$destination_identity" create "$scratch/Destination Alias" Pinned.zip -- "$scratch/source.txt"
"$helper" --destination-identity "$destination_identity" extract "$destination/Pinned.zip" "$destination" Pinned -- 0
cmp "$scratch/source.txt" "$destination/Pinned/source.txt"
printf '%s\n' '  ok  packaged archive helper refused both operations in a substitute folder and accepted the restored original'

# Use the installed macOS archive writer only to generate independent fixtures.
COPYFILE_DISABLE=1 /usr/bin/tar -czf "$scratch/fixture.tar.gz" -C "$scratch" source.txt
COPYFILE_DISABLE=1 /usr/bin/tar --format=7zip -cf "$scratch/fixture.7z" -C "$scratch" source.txt
for archive in fixture.tar.gz fixture.7z; do
  "$helper" list "$scratch/$archive" > "$scratch/archive-list.json"
  python3 - "$scratch/archive-list.json" <<'PYTHON'
import json
import sys
with open(sys.argv[1]) as handle:
    entries = json.load(handle)
assert [(entry['index'], entry['path']) for entry in entries] == [(0, 'source.txt')]
PYTHON
  "$helper" extract "$scratch/$archive" "$scratch" "$archive-extracted" -- 0
  cmp "$scratch/source.txt" "$scratch/$archive-extracted/source.txt"
done
printf '%s\n' '  ok  packaged helper listed and extracted TAR.GZ and 7z'

# The release payload package-app.sh embeds must not carry the build machine's paths:
# make-release.sh refuses to sign a bundle that does. Swift's linker records each
# object file's absolute path as a debug map and SwiftPM adds an absolute rpath to
# the toolchain, so this scans the built bytes rather than trusting the flags.
TMPDIR="$scratch" bash "$root/richos/app/scripts/package-richfiles.sh" --embedded "$scratch/embedded"
python3 - "$HOME" "$scratch/embedded/RichFiles.app/Contents/MacOS/RichFiles" "$scratch/embedded/libRichFilesRail.dylib" <<'PYTHON'
import sys
home = sys.argv[1].rstrip('/').encode()
markers = [b'/Volumes/', b'/Users/'] + ([home] if home else [])
failed = False
for path in sys.argv[2:]:
    with open(path, 'rb') as handle:
        data = handle.read()
    counts = {marker.decode(): data.count(marker) for marker in markers}
    print('  ', path.rsplit('/', 1)[-1], ' '.join(f'{marker}={count}' for marker, count in counts.items()))
    failed = failed or any(counts.values())
if failed:
    print('  FAIL  the release RichFiles payload carries a build-machine path', file=sys.stderr)
    sys.exit(1)
PYTHON
printf '%s\n' '  ok  release RichFiles and libRichFilesRail.dylib carry no build-machine path'
