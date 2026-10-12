#!/usr/bin/env bash
# The iPhone platform code's own tests, on this Mac with no simulator (stream I3's loop L1).
#
#   Release/platform-tests.sh <output directory in the selected storage folder>
#
# Compiles the core (RichOSCore) as a module, then two test programs with swiftc:
#   notification  NotificationService/Tests/main.swift — preview decryption against the Mac's own
#                 sealed fixture, the extension's decision, tap parsing and routing, registration
#   share         ShareExtension/Tests/main.swift — the inbox, the Mac's attachment limits and
#                 types, delivery against a fake Mac, the 3-second rule, HEIC to JPEG, and the
#                 share sheet's activation rule
# Each prints "<name>: N checks passed" and exits 0, or lists its failures and exits 1.
# Why swiftc and not the core's Swift package: these files belong to the app and its extensions
# (App/Platform/Shared is compiled into all three), and the package is stream I1's file.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:?usage: platform-tests.sh <output directory>}"
OUT="$(python3 "$HERE/../../engine/scripts/lib/machine_settings.py" validate "$OUT")" || exit 2
mkdir -p "$OUT/module-cache" "$OUT/work"
export CLANG_MODULE_CACHE_PATH="$OUT/module-cache"
SWIFTC=(xcrun swiftc -swift-version 5 -module-cache-path "$OUT/module-cache")

CORE_SOURCES=()
while IFS= read -r file; do CORE_SOURCES+=("$file"); done < <(find "$HERE/Core/Sources/RichOSCore" -name '*.swift' | sort)
SHARED=()
while IFS= read -r file; do SHARED+=("$file"); done < <(find "$HERE/App/Platform/Shared" -name '*.swift' | sort)

"${SWIFTC[@]}" -parse-as-library -emit-library -static -module-name RichOSCore \
  -emit-module -emit-module-path "$OUT/RichOSCore.swiftmodule" -o "$OUT/libRichOSCore.a" "${CORE_SOURCES[@]}"

"${SWIFTC[@]}" -I "$OUT" -L "$OUT" -lRichOSCore -o "$OUT/notification-tests" \
  "$HERE/NotificationService/Tests/main.swift" "$HERE/NotificationService/Sources/NotificationService.swift" \
  "$HERE/App/Platform/NotificationTapRouter.swift" "${SHARED[@]}"

"${SWIFTC[@]}" -I "$OUT" -L "$OUT" -lRichOSCore -o "$OUT/share-tests" \
  "$HERE/ShareExtension/Tests/main.swift" "$HERE/App/Platform/SharePlatform.swift" "${SHARED[@]}"

FIXTURE="$HERE/../test/fixtures/notification-preview.json"
"$OUT/notification-tests" "$FIXTURE"
rm -rf "$OUT/work"
"$OUT/share-tests" "$OUT/work" "$HERE/ShareExtension/Info.plist"
rm -rf "$OUT/work"
