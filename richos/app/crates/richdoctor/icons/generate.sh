#!/usr/bin/env bash
# Make RichDoctor's bundle icons from the CEO's chosen artwork: round 38 v3 "Badge"
# (richos-hq design/mockups/rounds/round-38/v3/richdoctor-icon-1024.png), and the menu bar
# template image from the same round's menubar/richdoctor-menubar@2x.png. Uses macOS's own
# sips and iconutil; the outputs are committed beside this script.
#
#   richos/app/crates/richdoctor/icons/generate.sh <richos-hq checkout>
set -euo pipefail
hq="${1:?usage: generate.sh <richos-hq checkout>}"
here="$(cd "$(dirname "$0")" && pwd)"
src="$hq/design/mockups/rounds/round-38/v3/richdoctor-icon-1024.png"
bar="$hq/design/mockups/rounds/round-38/v3/menubar/richdoctor-menubar@2x.png"
[ -f "$src" ] || { echo "missing $src" >&2; exit 1; }
[ -f "$bar" ] || { echo "missing $bar" >&2; exit 1; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
set="$work/icon.iconset"
mkdir "$set"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$src" --out "$set/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$src" --out "$set/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$set" -o "$here/icon.icns"
sips -z 32 32 "$src" --out "$here/32x32.png" >/dev/null
sips -z 128 128 "$src" --out "$here/128x128.png" >/dev/null
sips -z 256 256 "$src" --out "$here/128x128@2x.png" >/dev/null
cp "$bar" "$here/menubar@2x.png"
echo "icons written to $here"
