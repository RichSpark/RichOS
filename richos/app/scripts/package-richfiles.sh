#!/usr/bin/env bash
# Build a standalone development bundle, or a release-mode payload for RichOS.
set -euo pipefail

embedded=0
if [ "${1:-}" = "--embedded" ]; then
  embedded=1
  shift
fi

if [ "$#" -ne 1 ] || [[ "$1" != /* || "$1" == */ ]]; then
  echo "Usage: $0 [--embedded] /absolute/new-output-directory" >&2
  exit 2
fi
output="$1"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)"
machine_lib="$root/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$machine_lib/machine-settings.sh"
richos_machine_settings
cache="$(python3 "$machine_lib/machine_settings.py" cache)"
checkout_key="$(printf '%s' "$root" | shasum -a 256 | cut -c1-10)"
build="$cache/richfiles/proof/$checkout_key"
package="$root/richos/tools/richfiles"

mkdir "$output"
complete=0
trap 'if [ "$complete" -eq 0 ]; then rm -rf -- "$output"; fi' EXIT
configuration=debug
stamp_suite_version() {
  if [ "$embedded" -eq 1 ] && [ -n "${RICHOS_SUITE_VERSION:-}" ]; then
    /usr/libexec/PlistBuddy -c "Add :RichOSSuiteVersion string $RICHOS_SUITE_VERSION" "$1"
  fi
}
swift_args=(--package-path "$package" --scratch-path "$build")
if [ "$embedded" -eq 1 ]; then
  configuration=release
  swift_args+=(-Xswiftc -file-prefix-map -Xswiftc "$root=/build/richos")
  # The linker otherwise records every object file's absolute scratch path as a debug map.
  swift_args+=(-Xlinker -S)
fi
# SwiftPM also links an absolute rpath to the toolchain's back-deployment libraries,
# which names the build machine's Xcode and does not exist on any other Mac.
drop_build_machine_rpaths() {
  local binary="$1" rpath
  while IFS= read -r rpath; do
    case "$rpath" in
      @*|/usr/lib/*|/System/*) ;;
      *) install_name_tool -delete_rpath "$rpath" "$binary" ;;
    esac
  done < <(otool -l "$binary" | awk '$1 == "cmd" { in_rpath = ($2 == "LC_RPATH") }
    in_rpath && $1 == "path" { sub(/^ *path /, ""); sub(/ \(offset [0-9]+\)$/, ""); print }')
  codesign --force --sign - "$binary"
}
python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichFiles
python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichGlance
python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichGlanceCaptureHelper
python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichFilesWindowGestures
if [ "$embedded" -eq 1 ]; then
  python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichFilesIntegration
  install -m 755 "$build/$configuration/libRichFilesIntegration.dylib" "$output/libRichFilesIntegration.dylib"
  drop_build_machine_rpaths "$output/libRichFilesIntegration.dylib"
fi
# shellcheck source=lib/gui-launch.sh
. "$root/richos/app/scripts/lib/gui-launch.sh"
archive_build="${CARGO_TARGET_DIR:-$cache}/richfiles-archive/$checkout_key"
archive_artifacts="$output/cargo-artifacts.jsonl"
python3 "$machine_lib/native-work.py" -- cargo build --release --locked \
  --manifest-path "$package/archive/Cargo.toml" --target-dir "$archive_build" \
  --message-format=json-render-diagnostics > "$archive_artifacts"
archive_executable="$(gui_artifact "$archive_artifacts" richfiles-archive bin)"
rm -- "$archive_artifacts"

app="$output/RichFiles.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
fonts="$root/richos/mobile/native-ios/App/Design/Fonts"
for font in Inter-Variable.ttf Newsreader-Regular.ttf LICENSE-Inter.txt LICENSE-Newsreader.txt; do
  install -m 644 "$fonts/$font" "$app/Contents/Resources/$font"
done
install -m 755 "$build/$configuration/RichFiles" "$app/Contents/MacOS/RichFiles"
if [ "$embedded" -eq 1 ]; then
  drop_build_machine_rpaths "$app/Contents/MacOS/RichFiles"
fi
install -m 755 "$archive_executable" "$app/Contents/MacOS/richfiles-archive"
install -m 644 "$package/Info.plist" "$app/Contents/Info.plist"
install -m 644 "$package/Resources/RichFiles.icns" "$app/Contents/Resources/RichFiles.icns"
glance="$app/Contents/Applications/RichGlance.app"
mkdir -p "$glance/Contents/MacOS" "$glance/Contents/Resources"
install -m 755 "$build/$configuration/RichGlance" "$glance/Contents/MacOS/RichGlance"
install -m 644 "$package/RichGlance-Info.plist" "$glance/Contents/Info.plist"
install -m 644 "$package/Resources/RichGlance.icns" "$glance/Contents/Resources/RichGlance.icns"
for font in Newsreader-Regular.ttf LICENSE-Newsreader.txt; do
  install -m 644 "$fonts/$font" "$glance/Contents/Resources/$font"
done
/usr/libexec/PlistBuddy -c "Add :RichOSSourceRevision string $(git -C "$root" rev-parse HEAD)" "$glance/Contents/Info.plist"
if [ "$embedded" -eq 1 ]; then
  drop_build_machine_rpaths "$glance/Contents/MacOS/RichGlance"
fi
capture_helper="$glance/Contents/Library/LoginItems/RichGlance Capture.app"
mkdir -p "$capture_helper/Contents/MacOS"
install -m 755 "$build/$configuration/RichGlanceCaptureHelper" "$capture_helper/Contents/MacOS/RichGlanceCaptureHelper"
install -m 644 "$package/RichGlanceCaptureHelper-Info.plist" "$capture_helper/Contents/Info.plist"
if [ "$embedded" -eq 1 ]; then
  drop_build_machine_rpaths "$capture_helper/Contents/MacOS/RichGlanceCaptureHelper"
fi
/usr/libexec/PlistBuddy -c "Add :RichOSSourceRevision string $(git -C "$root" rev-parse HEAD)" "$capture_helper/Contents/Info.plist"
stamp_suite_version "$capture_helper/Contents/Info.plist"
plutil -lint "$capture_helper/Contents/Info.plist"
codesign --force --sign - "$capture_helper"
stamp_suite_version "$glance/Contents/Info.plist"
plutil -lint "$glance/Contents/Info.plist"
codesign --force --sign - "$glance"
gestures="$app/Contents/Library/LoginItems/RichFiles Window Gestures.app"
mkdir -p "$gestures/Contents/MacOS"
install -m 755 "$build/$configuration/RichFilesWindowGestures" "$gestures/Contents/MacOS/RichFilesWindowGestures"
install -m 644 "$package/RichFilesWindowGestures-Info.plist" "$gestures/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :RichOSSourceRevision string $(git -C "$root" rev-parse HEAD)" "$gestures/Contents/Info.plist"
if [ "$embedded" -eq 1 ]; then
  drop_build_machine_rpaths "$gestures/Contents/MacOS/RichFilesWindowGestures"
fi
stamp_suite_version "$gestures/Contents/Info.plist"
plutil -lint "$gestures/Contents/Info.plist"
codesign --force --sign - "$gestures"
stamp_suite_version "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app/Contents/MacOS/richfiles-archive"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
complete=1
printf '%s\n' "$app"
