#!/usr/bin/env bash
#
# release-bundle.sh — branch-bundle.sh's walkable RichOS.app zip, built with `cargo build --release`,
# so a launch-time measurement in the guest times optimized code (a debug build's Rust before the
# window is slower, and the CEO's 380 ms is a release-build number).
#
#   release-bundle.sh <richos-app-dir> --out <zip> --engine-version V --engine-url URL --engine-sha256 SHA
#
# Same steps as richos/app/scripts/testvm/branch-bundle.sh (the harness's own gui_identity and
# gui_bundle, ad-hoc signed, never for anyone's own Mac), with --release.
set -euo pipefail
APP_DIR="$(cd "$1" && pwd)"; shift
OUT=""; ENGINE_VERSION=""; ENGINE_URL=""; ENGINE_SHA256=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --engine-version) ENGINE_VERSION="$2"; shift 2 ;;
    --engine-url) ENGINE_URL="$2"; shift 2 ;;
    --engine-sha256) ENGINE_SHA256="$2"; shift 2 ;;
    *) echo "release-bundle.sh: unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT" ] && [ -n "$ENGINE_VERSION" ] && [ -n "$ENGINE_URL" ] && [ -n "$ENGINE_SHA256" ] || {
  echo "release-bundle.sh: --out and the engine pin are required" >&2; exit 2; }
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac
SHA="$(git -C "$APP_DIR" rev-parse HEAD)"
JSON="$(mktemp "${TMPDIR:-/tmp}/release-bundle.XXXXXX")"
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/release-bundle.XXXXXX")"
trap 'rm -rf "$JSON" "$STAGE"' EXIT
echo "release-bundle.sh: building richos-tauri --release at $SHA (plus the working tree) with engine $ENGINE_VERSION pinned" >&2
( cd "$APP_DIR/src-tauri" && RICHOS_SOURCE_SHA="$SHA" RICHOS_ENGINE_VERSION="$ENGINE_VERSION" \
    RICHOS_ENGINE_URL="$ENGINE_URL" RICHOS_ENGINE_SHA256="$ENGINE_SHA256" \
    cargo build --release --quiet --bin richos-tauri --message-format=json-render-diagnostics ) > "$JSON"
EXE="$(sed -n 's/.*"executable":"\([^"]*richos-tauri\)".*/\1/p' "$JSON" | tail -1)"
[ -x "$EXE" ] || { echo "release-bundle.sh: cargo reported no richos-tauri executable" >&2; exit 1; }
GUI_APP_DIR="$APP_DIR"; export GUI_APP_DIR
# shellcheck source=/dev/null
. "$APP_DIR/scripts/lib/gui-launch.sh"
read -r IDENTIFIER VERSION < <(gui_identity "$EXE")
gui_bundle "$STAGE/RichOS.app" "$EXE" "$IDENTIFIER" "$VERSION"
codesign --force -s - "$STAGE/RichOS.app" >/dev/null 2>&1
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
( cd "$STAGE" && ditto -c -k --keepParent RichOS.app "$OUT" )
echo "bundle=$OUT commit=$SHA identity=$IDENTIFIER $VERSION engine=$ENGINE_VERSION profile=release"
