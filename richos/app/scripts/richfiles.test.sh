#!/usr/bin/env bash
# run-tests: no-host-screen: native tests with synthetic files, a separate pasteboard and a local development bundle
# run-tests: inputs richos/mobile/native-ios/App/Design/Fonts richos/app/scripts/richfiles.test.sh richos/app/scripts/qa/timeline-window.swift richos/tools/richfiles richos/engine/scripts/lib/native-work.py richos/engine/scripts/lib/machine-settings.sh richos/engine/scripts/lib/machine_settings.py richos/app/scripts/package-richfiles.sh richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/lib/cargo_identity.py richos/app/scripts/bin/cargo
# run-tests: covers richos/tools/richfiles/Sources/RichFilesCore/SuiteRestart.swift richos/tools/richfiles/Sources/RichFiles/WindowRecording.swift richos/app/scripts/qa/timeline-window.swift richos/tools/richfiles/Sources/RichFiles/FolderAssociation.swift richos/tools/richfiles/Sources/RichFiles/SharedBrowserSettings.swift richos/tools/richfiles/Sources/RichFilesIntegration/SettingsBridge.swift richos/tools/richfiles/Sources/RichFiles/TerminalAtFolder.swift richos/app/scripts/package-richfiles.sh richos/tools/richfiles/Info.plist richos/tools/richfiles/Package.swift richos/tools/richfiles/Resources/RichFiles.icns richos/tools/richfiles/Sources/RichFiles/ArchiveTools.swift richos/tools/richfiles/Sources/RichFiles/BrowserAppearance.swift richos/tools/richfiles/Sources/RichFiles/BrowserActions.swift richos/tools/richfiles/Sources/RichFiles/BrowserDragSurface.swift richos/tools/richfiles/Sources/RichFiles/BrowserModel.swift richos/tools/richfiles/Sources/RichFiles/BrowserSheets.swift richos/tools/richfiles/Sources/RichFiles/BrowserTransfer.swift richos/tools/richfiles/Sources/RichFiles/BrowserView.swift richos/tools/richfiles/Sources/RichFiles/DiskCleanupModel.swift richos/tools/richfiles/Sources/RichFiles/DiskCleanupView.swift richos/tools/richfiles/Sources/RichFiles/DiskCleanupRing.swift richos/tools/richfiles/Sources/RichFiles/BrowserWindowLayout.swift richos/tools/richfiles/Sources/RichFiles/DetailsViewport.swift richos/tools/richfiles/Sources/RichFiles/KeyboardBridge.swift richos/tools/richfiles/Sources/RichFiles/RichFilesApp.swift richos/tools/richfiles/Sources/RichFilesCore/RichFilesDiagnostics.swift richos/tools/richfiles/Sources/RichFiles/RichOSServices.swift richos/tools/richfiles/Sources/RichFiles/TransferJournal.swift richos/tools/richfiles/Sources/RichFiles/TrashIndex.swift richos/tools/richfiles/Sources/RichFiles/WindowWorkspace.swift richos/tools/richfiles/Sources/RichFiles/WorkspaceHandoff.swift richos/tools/richfiles/Sources/RichFilesApp/Application.swift richos/tools/richfiles/Sources/RichFilesIntegration/OutputGitHost.swift richos/tools/richfiles/Sources/RichFilesCore/BatchRename.swift richos/tools/richfiles/Sources/RichFilesCore/DirectoryReader.swift richos/tools/richfiles/Sources/RichFilesCore/FileOperations.swift richos/tools/richfiles/Sources/RichFilesCore/FolderComparison.swift richos/tools/richfiles/Sources/RichFilesCore/FolderSync.swift richos/tools/richfiles/Sources/RichFilesCore/ImageQuickActions.swift richos/tools/richfiles/Sources/RichFilesCore/Locations.swift richos/tools/richfiles/Sources/RichFilesCore/StorageAnalysis.swift richos/tools/richfiles/Sources/RichFilesCore/StorageCleanup.swift richos/tools/richfiles/Sources/RichFilesCore/TextContentSearch.swift richos/tools/richfiles/Sources/RichFilesCore/VerifiedTree.swift richos/tools/richfiles/Sources/RichFilesCore/WorkspaceServices.swift richos/tools/richfiles/Tests/RichFilesCoreTests/BatchRenameTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/DirectoryReaderTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/FileOperationsTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/TextContentSearchTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/WorkspaceServicesTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserActionsTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserAppearanceTests.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserModelTests.swift richos/tools/richfiles/archive/Cargo.lock richos/tools/richfiles/archive/Cargo.toml richos/tools/richfiles/archive/src/archive.rs richos/tools/richfiles/archive/src/main.rs richos/tools/richfiles/archive/src/native.rs richos/tools/richfiles/archive/src/stream.rs richos/tools/richfiles/archive/src/tests.rs richos/tools/richfiles/Sources/RichFilesCore/GitRepository.swift richos/tools/richfiles/Tests/RichFilesCoreTests/GitRepositoryTests.swift richos/tools/richfiles/Sources/RichFiles/GitPanel.swift richos/tools/richfiles/Sources/RichFiles/GitPanelModel.swift richos/tools/richfiles/RichGlance-Info.plist richos/tools/richfiles/Resources/RichGlance.icns richos/tools/richfiles/Sources/RichFilesCore/GlanceImage.swift richos/tools/richfiles/Sources/RichGlance/Application.swift richos/tools/richfiles/Sources/RichGlance/Canvas.swift richos/tools/richfiles/Sources/RichGlance/Viewer.swift richos/tools/richfiles/Tests/RichFilesCoreTests/GlanceImageTests.swift richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.avif richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.bmp richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.heic richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.icns richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.ico richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.jpe richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/sample.tif richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/still.webp richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/delta.gif richos/tools/richfiles/Tests/RichFilesCoreTests/Fixtures/Glance/delta.webp richos/tools/richfiles/Sources/RichFiles/GlanceDefaults.swift richos/tools/richfiles/Sources/RichFilesCore/GlanceMetadata.swift richos/tools/richfiles/Sources/RichGlance/GlanceInfo.swift richos/tools/richfiles/Tests/RichFilesCoreTests/GlanceMetadataTests.swift richos/tools/richfiles/Sources/RichFilesCore/GlanceCaptureSelection.swift richos/tools/richfiles/Tests/RichFilesCoreTests/GlanceCaptureSelectionTests.swift richos/tools/richfiles/Sources/RichGlance/CaptureFrame.swift richos/tools/richfiles/Sources/RichGlance/CaptureController.swift richos/tools/richfiles/Sources/RichGlance/CaptureCanvas.swift richos/tools/richfiles/Tests/RichGlanceTests/GlanceInfoContrastTests.swift richos/tools/richfiles/RichFilesWindowGestures-Info.plist richos/tools/richfiles/Sources/RichFiles/WindowGestureSettings.swift richos/tools/richfiles/Sources/RichFilesCore/WindowGesturePreferences.swift richos/tools/richfiles/Sources/RichFilesWindowGestures/GestureController.swift richos/tools/richfiles/Sources/RichFilesWindowGestures/DesktopFolderClick.swift richos/tools/richfiles/Sources/RichFilesCore/DesktopTrashShortcut.swift richos/tools/richfiles/Tests/RichFilesCoreTests/DesktopTrashShortcutTests.swift richos/tools/richfiles/Sources/RichFilesWindowGestures/WindowAccessibility.swift richos/tools/richfiles/Sources/RichFilesWindowGestures/main.swift richos/tools/richfiles/Sources/RichGlance/CaptureControls.swift richos/tools/richfiles/Sources/RichGlance/CaptureCountdown.swift richos/tools/richfiles/Sources/RichGlance/LiveCaptureKeys.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotCanvas.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotDocument.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotEditor.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotEditorChrome.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotHistory.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotRecents.swift richos/tools/richfiles/Tests/RichGlanceTests/ScreenshotWorkflowTests.swift richos/tools/richfiles/Sources/RichGlance/ScrollStitcher.swift richos/tools/richfiles/Sources/RichGlance/ScrollingCapture.swift richos/tools/richfiles/Tests/RichGlanceTests/ScrollStitcherTests.swift richos/tools/richfiles/Tests/RichGlanceTests/GlanceViewerSizingTests.swift richos/tools/richfiles/Tests/RichFilesTests/DiskCleanupTests.swift richos/tools/richfiles/Sources/RichFiles/BrowserWindowPlacement.swift richos/tools/richfiles/Tests/RichFilesTests/BrowserWindowPlacementTests.swift richos/tools/richfiles/Sources/RichFilesCore/WindowTile.swift richos/tools/richfiles/Tests/RichFilesCoreTests/WindowTileTests.swift richos/tools/richfiles/Sources/RichFilesWindowGestures/WindowSwitcher.swift richos/tools/richfiles/Sources/RichFilesCore/CaptureRequest.swift richos/tools/richfiles/Sources/RichFilesCore/GlanceHelper.swift richos/tools/richfiles/Sources/RichGlance/GlanceHelperControl.swift richos/tools/richfiles/Sources/RichGlance/BackgroundCaptureSettings.swift richos/tools/richfiles/Sources/RichGlanceCaptureHelper/main.swift richos/tools/richfiles/RichGlanceCaptureHelper-Info.plist richos/tools/richfiles/Tests/RichFilesCoreTests/GlanceHelperTests.swift richos/tools/richfiles/Tests/RichGlanceTests/GlanceHelperSessionTests.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotCompareView.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotComparison.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotRepeatTarget.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotText.swift richos/tools/richfiles/Tests/RichGlanceTests/ScreenshotComparisonTests.swift richos/tools/richfiles/Tests/RichGlanceTests/ScreenshotTextTests.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotPin.swift richos/tools/richfiles/Sources/RichFilesCore/GlancePins.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotDrag.swift richos/tools/richfiles/Sources/RichGlance/ScreenshotChat.swift
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
swiftc -typecheck "$root/richos/app/scripts/qa/timeline-window.swift"

# Verify the same bundle and ZIP executable a development build launches.
TMPDIR="$scratch" bash "$root/richos/app/scripts/package-richfiles.sh" "$scratch/package"
app="$scratch/package/RichFiles.app"
helper="$app/Contents/MacOS/richfiles-archive"
gestures="$app/Contents/Library/LoginItems/RichFiles Window Gestures.app"
test "$(plutil -extract CFBundleIdentifier raw "$gestures/Contents/Info.plist")" = app.richos.richfiles.window-gestures
test "$(plutil -extract LSUIElement raw "$gestures/Contents/Info.plist")" = true
test -x "$gestures/Contents/MacOS/RichFilesWindowGestures"
codesign --verify --strict "$gestures"
printf '%s\n' '  ok  bundled RichFiles Window Gestures login item is signed and stays out of the Dock'
glance="$app/Contents/Applications/RichGlance.app"
test -x "$glance/Contents/MacOS/RichGlance"
cmp "$root/richos/tools/richfiles/Resources/RichGlance.icns" "$glance/Contents/Resources/RichGlance.icns"
cmp "$root/richos/mobile/native-ios/App/Design/Fonts/Newsreader-Regular.ttf" "$glance/Contents/Resources/Newsreader-Regular.ttf"
test "$(plutil -extract CFBundleIdentifier raw "$glance/Contents/Info.plist")" = app.richos.richglance
test "$(plutil -extract CFBundleDocumentTypes.0.LSHandlerRank raw "$glance/Contents/Info.plist")" = Alternate
test "$(plutil -extract CFBundleDocumentTypes.0.LSItemContentTypes.0 raw "$glance/Contents/Info.plist")" = public.png
test "$(plutil -extract CFBundleDocumentTypes.0.LSItemContentTypes.1 raw "$glance/Contents/Info.plist")" = public.jpeg
capture_helper="$glance/Contents/Library/LoginItems/RichGlance Capture.app"
test -x "$capture_helper/Contents/MacOS/RichGlanceCaptureHelper"
test "$(plutil -extract CFBundleIdentifier raw "$capture_helper/Contents/Info.plist")" = app.richos.richglance.capture-helper
test "$(plutil -extract LSUIElement raw "$capture_helper/Contents/Info.plist")" = true
codesign --verify --deep --strict "$glance"
printf '%s\n' '  ok  signed RichGlance Capture login item is nested inside its owner app'
cmp "$root/richos/tools/richfiles/Resources/RichFiles.icns" "$app/Contents/Resources/RichFiles.icns"
test "$(plutil -extract CFBundleIconFile raw "$app/Contents/Info.plist")" = RichFiles.icns
test "$(plutil -extract CFBundleShortVersionString raw "$app/Contents/Info.plist")" = 1.0.1
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
embedded="$scratch/embedded/RichFiles.app"
python3 - "$HOME" "$embedded/Contents/MacOS/RichFiles" "$scratch/embedded/libRichFilesIntegration.dylib" \
  "$embedded/Contents/Applications/RichGlance.app/Contents/MacOS/RichGlance" \
  "$embedded/Contents/Applications/RichGlance.app/Contents/Library/LoginItems/RichGlance Capture.app/Contents/MacOS/RichGlanceCaptureHelper" \
  "$embedded/Contents/Library/LoginItems/RichFiles Window Gestures.app/Contents/MacOS/RichFilesWindowGestures" <<'PYTHON'
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
printf '%s\n' '  ok  release RichFiles, RichGlance, the login item and libRichFilesIntegration.dylib carry no build-machine path'
