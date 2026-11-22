#!/usr/bin/env bash
# run-tests: no-host-screen: capture bridge, history preferences and syntax checks; does not register a global key or capture the screen
# run-tests: inputs richos/app/scripts/glance-capture-shortcut.test.sh richos/app/scripts/lib/cargo-cache-env.sh richos/app/src-tauri richos/app/crates richos/app/ui/glance-capture-settings.js richos/app/ui/settings-button.js richos/app/ui/index.html richos/app/ui/assets/app-icons/richglance-128.png
# run-tests: covers richos/app/src-tauri/src/glance_capture_shortcut.rs richos/app/ui/glance-capture-settings.js richos/app/ui/assets/app-icons/richglance-128.png
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$(uname -s)" != Darwin ]; then
  printf '%s\n' '  NOT RUN  RichGlance shortcut: macOS is required'
  exit 2
fi
node --check "$here/../ui/glance-capture-settings.js"
node --check "$here/../ui/settings-button.js"
. "$here/lib/cargo-cache-env.sh"
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" --locked --bin richos-tauri glance_capture_shortcut::
