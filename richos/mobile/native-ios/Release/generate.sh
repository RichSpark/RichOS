#!/usr/bin/env bash
# Generates the RichOS native iPhone Xcode project WITH the release and extension additions
# (Release/platform.yml) into a directory in the selected storage folder, and prints the .xcodeproj path.
#
#   Release/generate.sh <output directory>
#
# When project.yml already includes Release/platform.yml (the one line requested of stream I1),
# this is plain `xcodegen generate`. Until then it writes a two-line spec that includes both, so the
# release checks and the suite build exactly what the include will build. Nothing is written into
# the source tree either way.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:?usage: generate.sh <output directory>}"
OUT="$(python3 "$HERE/../../engine/scripts/lib/machine_settings.py" validate "$OUT")"
mkdir -p "$OUT"
# platform.yml names its plist and entitlements files from this (see its header).
export RICHOS_NATIVE_IOS_ROOT="$HERE"
if grep -Eq '^[[:space:]]*-[[:space:]]*path:[[:space:]]*Release/platform\.yml' "$HERE/project.yml"; then
  SPEC="$HERE/project.yml"
else
  SPEC="$OUT/with-platform.yml"
  printf 'include:\n  - path: %s\n  - path: %s\n' "$HERE/project.yml" "$HERE/Release/platform.yml" > "$SPEC"
fi
# --use-cache: an unchanged spec leaves the project file untouched, so Xcode does not rebuild.
xcodegen generate --spec "$SPEC" --project "$OUT" --project-root "$HERE" --use-cache --cache-path "$OUT/xcodegen.cache" --quiet >&2
echo "$OUT/RichOSNative.xcodeproj"
