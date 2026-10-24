#!/usr/bin/env bash
# The actual mobile client against Rust HTTP/auth/SSE, durable intake and gated Spine
# output. Cognition is deterministic; no physical device or live provider is claimed.
# run-tests: no-host-screen: headless Rust listener and Node client only
# run-tests: inputs richos/app/scripts/mobile-mac.test.sh richos/mobile/dev/mac-server.rs richos/mobile/dev/lab-files.py richos/mobile/dev/mac-server.mjs richos/mobile/test/mac-server.test.mjs richos/mobile/test/storage.cjs richos/mobile/core richos/mobile/platform richos/web/web-app richos/app/src-tauri/src richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Cargo.lock richos/app/src-tauri/build.rs richos/app/src-tauri/tauri.conf.json richos/app/src-tauri/capabilities richos/app/src-tauri/icons richos/app/crates richos/app/ui
# run-tests: covers richos/mobile/dev/mac-server.rs richos/mobile/dev/mac-server.mjs richos/mobile/dev/lab-files.py richos/mobile/test/mac-server.test.mjs
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
if [[ "$(uname -s)" != Darwin ]] || ! command -v node >/dev/null 2>&1; then
  echo '  NOT RUN  mobile-mac: macOS and Node are required'
  exit 2
fi
if ! command -v cargo >/dev/null 2>&1 && [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
  echo '  NOT RUN  mobile-mac: Rust toolchain is unavailable'
  exit 2
fi
if ! node -e 'const {createScratch}=require(process.argv[1]); const fs=require("node:fs"); try { fs.rmdirSync(createScratch("mac-preflight")); } catch(e) { console.error(e.message); process.exit(1); }' "$ROOT/richos/mobile/test/storage.cjs"; then
  echo '  NOT RUN  mobile-mac: external test storage is unavailable'
  exit 2
fi
mobile_cargo="$(command -v cargo || true)"
if [[ -z "$mobile_cargo" ]]; then mobile_cargo="$HOME/.cargo/bin/cargo"; fi
"$mobile_cargo" test --manifest-path "$ROOT/richos/app/src-tauri/Cargo.toml" mobile_mac_server::manual_session_has_no_deadline_but_automated_session_is_bounded -- --exact
node --test "$ROOT/richos/mobile/test/mac-server.test.mjs"
echo '  PASS  actual mobile client against isolated Rust Mac server, including restart'
