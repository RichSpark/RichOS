#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/quiet-browser.test.sh richos/app/ui/tests/lib richos/tools richos/mobile richos/web richos/app/ui/tests richos/app/scripts
# run-tests: covers richos/app/ui/tests/lib/quiet-browser.js richos/app/ui/tests/lib/quiet-browser.test.js richos/app/ui/tests/lib/harness.js richos/tools/richos-extension/tests/native-transport-e2e.mjs richos/tools/richos-extension/tests/live-capture.mjs
# Every browser a suite or the harness starts on this Mac is quiet: headless, a throwaway profile,
# notifications and permissions denied, never registered with macOS Notification Center
# (richos/app/ui/tests/lib/quiet-browser.js; the 2026-10-29 "Google Chrome for Testing" prompts).
# No browser starts here: a fake Playwright and a stand-in Chrome for Testing script record what
# each launch would have been given.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo '  NOT RUN  quiet-browser: Node is unavailable'
  exit 2
fi
node "$DIR/../ui/tests/lib/quiet-browser.test.js"
