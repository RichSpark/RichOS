#!/usr/bin/env bash
# Mutable Cargo output belongs to one physical workspace. Shared Cargo registry
# downloads and a configured RUSTC_WRAPPER compiler cache retain safe reuse.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
MODE='link'
CHECKOUT=""
for arg in "$@"; do
  case "$arg" in
    --check) MODE=check ;;
    --adopt) MODE=adopt ;;
    -h|--help) echo "shared-build-cache.sh [--check|--adopt] [checkout]"; exit 0 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) CHECKOUT="$arg" ;;
  esac
done
CHECKOUT="$(cd "${CHECKOUT:-$HERE/../../..}" && pwd -P)"
[ -d "$CHECKOUT/richos/app" ] || { echo "not a RichOS checkout: $CHECKOUT" >&2; exit 2; }
NIGHTLY="$HOME/.richos-nightly/source"
if [ -d "$NIGHTLY" ] && [ "$(cd "$NIGHTLY" && pwd -P)" = "$CHECKOUT" ]; then
  echo "nightly release checkout manages its own build output" >&2; exit 2
fi
. "$HERE/../../engine/scripts/lib/machine-settings.sh"
richos_machine_settings
CACHE_ROOT="${RICHOS_BUILD_CACHE:-$(python3 "$HERE/../../engine/scripts/lib/machine_settings.py" cache cargo-target --key CARGO_TARGET_DIR)}"
RC=0
for REL in richos/app richos/app/src-tauri; do
  LINK="$CHECKOUT/$REL/target"
  DEST="$(CARGO_TARGET_DIR="$CACHE_ROOT" RICHOS_CARGO_CACHE_ROOT="$CACHE_ROOT" "$HERE/bin/cargo" --richos-target-dir "$CHECKOUT/$REL")"
  if [ -L "$LINK" ]; then
    CUR="$(readlink "$LINK")"
    if [ "$CUR" = "$DEST" ] && [ -d "$DEST" ]; then
      echo "ok $REL/target -> $DEST"; continue
    fi
    if [ "$MODE" = check ]; then echo "unisolated $LINK -> $CUR" >&2; RC=1; continue; fi
    # Retarget only the link. Existing shared artifacts remain untouched.
    mkdir -p "$DEST"
    rm "$LINK"
    ln -s "$DEST" "$LINK"
    echo "isolated $REL/target -> $DEST"
    continue
  fi
  if [ "$MODE" = check ]; then echo "unlinked $LINK" >&2; RC=1; continue; fi
  if [ -d "$LINK" ] && [ -n "$(ls -A "$LINK")" ]; then
    if [ "$MODE" = adopt ] && [ ! -e "$DEST" ]; then
      mkdir -p "$(dirname "$DEST")"
      mv "$LINK" "$DEST"
    else
      echo "in use: $LINK; use --adopt when its private destination is absent" >&2
      RC=1; continue
    fi
  elif [ -d "$LINK" ]; then
    rmdir "$LINK"
  elif [ -e "$LINK" ]; then
    echo "refusing to replace a non-directory: $LINK" >&2; RC=1; continue
  fi
  mkdir -p "$DEST"
  ln -s "$DEST" "$LINK"
  echo "isolated $REL/target -> $DEST"
done
exit "$RC"
