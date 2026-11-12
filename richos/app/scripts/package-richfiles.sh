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
swift_args=(--package-path "$package" --scratch-path "$build")
if [ "$embedded" -eq 1 ]; then
  configuration=release
  swift_args+=(-Xswiftc -file-prefix-map -Xswiftc "$root=/build/richos")
fi
python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichFiles
if [ "$embedded" -eq 1 ]; then
  python3 "$machine_lib/native-work.py" -- swift build "${swift_args[@]}" -c "$configuration" --product RichFilesRail
  install -m 755 "$build/$configuration/libRichFilesRail.dylib" "$output/libRichFilesRail.dylib"
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
install -m 755 "$archive_executable" "$app/Contents/MacOS/richfiles-archive"
install -m 644 "$package/Info.plist" "$app/Contents/Info.plist"
install -m 644 "$package/Resources/RichFiles.icns" "$app/Contents/Resources/RichFiles.icns"
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app/Contents/MacOS/richfiles-archive"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
complete=1
printf '%s\n' "$app"
