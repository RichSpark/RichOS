#!/usr/bin/env bash
#
# branch-bundle.sh — a walkable RichOS.app zip of THIS checkout, for run-walk.py --bundle.
#
#   testvm/branch-bundle.sh --out <zip> --engine-version V --engine-url URL --engine-sha256 SHA [--richfiles] [--companion]
#
# WHY. A fix is proved on the real app in a guest before it is handed over, and a walk needs a
# bundle. A release zip is not this branch, and a debug executable copied into a release
# bundle does not start: update_startup sees a changed application with no publication receipt
# (2026-10-21, "Changed application has no publication receipt.", 0 windows). This builds the
# debug executable the guest suites already run (lib/gui-launch.sh `cargo build --bin
# richos-tauri`) and wraps it with the harness's own gui_bundle, whose Info.plist carries the
# identity the binary reports, so the app starts as itself.
#
# THE ENGINE PIN. A new user's first setup fetches the engine only when the build carries a pin
# (setup.rs engine_pin, compile-time RICHOS_ENGINE_VERSION/_URL/_SHA256; without one the setup
# refuses with EngineUnpinned), so a first-run walk needs it. Give the pin of the release whose
# engine the walk should install; a release's executable carries its own (the 1.2.1 walk of
# 2026-10-21 used engine 1.2.0, sha256 c772156f…).
#
# --richfiles also builds RichFiles from this checkout and puts it where the packaged app has it
# (package-app.sh's package_richfiles: Contents/Applications/RichFiles.app and
# Contents/Frameworks/libRichFilesRail.dylib), so a walk of the RichFiles page or its undocked
# app runs this branch's RichFiles. Without it the page says "This build does not contain
# RichFiles." (richfiles_pane.rs).
#
# --companion also puts the meeting-notes capture companion (prepare-companion.py --binary, the
# cached build) at Contents/MacOS/richos-companion, ad-hoc signed as package-app.sh signs it, so a
# meeting walk (Take Notes, the notes and their Source links) runs. Without it the app runs with no
# meeting notes (meetings.rs looks for that file).
#
# Unsigned (ad-hoc), never for anyone's own Mac: the guest is the only place it runs.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd "$HERE/../.." && pwd)"
OUT=""; ENGINE_VERSION=""; ENGINE_URL=""; ENGINE_SHA256=""; RICHFILES=0; COMPANION=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --engine-version) ENGINE_VERSION="$2"; shift 2 ;;
    --engine-url) ENGINE_URL="$2"; shift 2 ;;
    --engine-sha256) ENGINE_SHA256="$2"; shift 2 ;;
    --richfiles) RICHFILES=1; shift ;;
    --companion) COMPANION=1; shift ;;
    -h|--help) sed -n '2,34p' "$0"; exit 0 ;;
    *) echo "branch-bundle.sh: unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT" ] || { echo "branch-bundle.sh: --out <zip> is required" >&2; exit 2; }
[ -n "$ENGINE_VERSION" ] && [ -n "$ENGINE_URL" ] && [ -n "$ENGINE_SHA256" ] || {
  echo "branch-bundle.sh: --engine-version, --engine-url and --engine-sha256 are required: a first run cannot install an engine without a pin" >&2; exit 2; }

# Resolve the destination before entering disposable staging directories.
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac

SHA="$(git -C "$APP_DIR" rev-parse HEAD)"
JSON="$(mktemp "${TMPDIR:-/tmp}/branch-bundle.XXXXXX")"
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/branch-bundle.XXXXXX")"
trap 'rm -rf "$JSON" "$STAGE"' EXIT
echo "branch-bundle.sh: building richos-tauri at $SHA with engine $ENGINE_VERSION pinned" >&2
( cd "$APP_DIR/src-tauri" && RICHOS_SOURCE_SHA="$SHA" RICHOS_ENGINE_VERSION="$ENGINE_VERSION" \
    RICHOS_ENGINE_URL="$ENGINE_URL" RICHOS_ENGINE_SHA256="$ENGINE_SHA256" \
    cargo build --quiet --bin richos-tauri --message-format=json-render-diagnostics ) > "$JSON"
EXE="$(sed -n 's/.*"executable":"\([^"]*richos-tauri\)".*/\1/p' "$JSON" | tail -1)"
[ -x "$EXE" ] || { echo "branch-bundle.sh: cargo reported no richos-tauri executable" >&2; exit 1; }

GUI_APP_DIR="$APP_DIR"; export GUI_APP_DIR
# shellcheck source=../lib/gui-launch.sh
. "$APP_DIR/scripts/lib/gui-launch.sh"
read -r IDENTIFIER VERSION < <(gui_identity "$EXE")
gui_bundle "$STAGE/RichOS.app" "$EXE" "$IDENTIFIER" "$VERSION"
if [ "$RICHFILES" -eq 1 ]; then
  echo "branch-bundle.sh: building RichFiles from this checkout" >&2
  bash "$APP_DIR/scripts/package-richfiles.sh" --embedded "$STAGE/richfiles"
  mkdir -p "$STAGE/RichOS.app/Contents/Applications" "$STAGE/RichOS.app/Contents/Frameworks"
  ditto "$STAGE/richfiles/RichFiles.app" "$STAGE/RichOS.app/Contents/Applications/RichFiles.app"
  install -m 755 "$STAGE/richfiles/libRichFilesRail.dylib" "$STAGE/RichOS.app/Contents/Frameworks/libRichFilesRail.dylib"
fi
if [ "$COMPANION" -eq 1 ]; then
  COMPANION_BIN="$(python3 "$APP_DIR/scripts/prepare-companion.py" --binary)"
  install -m 755 "$COMPANION_BIN" "$STAGE/RichOS.app/Contents/MacOS/richos-companion"
  codesign --force --sign - --timestamp=none "$STAGE/RichOS.app/Contents/MacOS/richos-companion"
fi
codesign --force -s - "$STAGE/RichOS.app" >/dev/null 2>&1
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
( cd "$STAGE" && ditto -c -k --keepParent RichOS.app "$OUT" )
echo "bundle=$OUT commit=$SHA identity=$IDENTIFIER $VERSION engine=$ENGINE_VERSION richfiles=$RICHFILES companion=$COMPANION"
