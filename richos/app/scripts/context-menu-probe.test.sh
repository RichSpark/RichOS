#!/usr/bin/env bash
# Prove the web view's own right-click menu on this Mac's real WebKit (src-tauri/src/webview_menu.rs
# and ui/no-native-menu.js together, through src-tauri/examples/context_menu_probe.rs): selected text
# in the composer offers only Cut, Copy, Paste and Select All; the PDF preview and the company name
# offer no menu (CEO, nightly 65: no Look Up, Translate, Search with Google, Share, Speech or
# Services). Then prove the probe sees the defect: without the fix the same checks must fail.
# run-tests: no-host-screen: the probe's window is never ordered on screen, its process may not show a Dock icon, its right-clicks are handed to the view rather than posted to the system, and no menu is drawn
# run-tests: inputs richos/app/scripts/context-menu-probe.test.sh richos/app/src-tauri/examples/context_menu_probe.rs richos/app/src-tauri/src/webview_menu.rs richos/app/ui/no-native-menu.js richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Cargo.lock
# run-tests: covers richos/app/src-tauri/src/webview_menu.rs richos/app/src-tauri/examples/context_menu_probe.rs
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
if [ "$(uname -s)" != Darwin ]; then
  echo "=== context-menu-probe: WebKit's macOS menu exists only on macOS; nothing to check here ==="
  exit 0
fi
cargo build --quiet --example context_menu_probe
cargo run --quiet --example context_menu_probe
if out="$(cargo run --quiet --example context_menu_probe -- --without-the-fix)"; then
  printf '%s\n' "$out"
  echo "FAIL without src/webview_menu.rs the checks still pass, so they cannot see the defect" >&2
  exit 1
fi
printf '%s\n' "$out"
# A crash is not the defect: the two checks must fail by seeing WebKit's own items.
for check in "a right-click on selected text in the composer" "a right-click in the PDF preview"; do
  if ! grep -q "^FAIL $check.*Look Up" <<<"$out"; then
    echo "FAIL without src/webview_menu.rs, '$check' did not fail by seeing Look Up" >&2
    exit 1
  fi
done
echo "PASS without src/webview_menu.rs the same checks fail on WebKit's own items, as they must"
echo "=== context-menu-probe: all passed ==="
