#!/usr/bin/env bash
# User: status | approve <offer-id> | revoke. Watcher: tick (never approves).
# Uses the desktop's service and data directory even when the UI is not running.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PRODUCT="$(cd "$HERE/../.." && pwd)"

compiled_helper() {
  # The machine-wide folder is /Applications. Tests point RICHOS_SYSTEM_APPLICATIONS at their
  # own fixture so they never run the app a user installed there.
  for binary in "$HERE/../bin/richos-quota" "$HOME/Applications/RichOS.app/Contents/MacOS/richos-tauri" "${RICHOS_SYSTEM_APPLICATIONS:-/Applications}/RichOS.app/Contents/MacOS/richos-tauri"; do
    if [ -x "$binary" ]; then
      case "$binary" in
        */richos-tauri)
          # Inspect before executing: an older app may open its GUI for an unknown flag.
          /usr/bin/grep -aFq -- '--richos-quota-v1' "$binary" || continue
          exec "$binary" --richos-quota-v1 "$@";;
        *) exec "$binary" "$@";;
      esac
    fi
  done
}

# Hunt P5-33 (v3): the watcher's tick runs every poll. From a source checkout it
# went through `cargo run` each time (Cargo startup and the build lock on every
# tick, contending with real builds); a compiled helper, when one exists, serves
# it instead. The user's own commands keep the source-aligned helper first.
if [ "${1:-}" = tick ]; then
  compiled_helper "$@"
fi
if [ -f "$PRODUCT/app/Cargo.toml" ]; then
  CARGO="$(command -v cargo || true)"
  [ -n "$CARGO" ] || CARGO="$HOME/.cargo/bin/cargo"
  exec "$CARGO" run --quiet --manifest-path "$PRODUCT/app/Cargo.toml" -p richos-core --bin richos-quota -- "$@"
fi
compiled_helper "$@"
echo 'The quota helper is unavailable. Install a RichOS version with terminal quota support, or run this from the source checkout with Cargo installed.' >&2
exit 2
