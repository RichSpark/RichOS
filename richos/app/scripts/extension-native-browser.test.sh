#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/extension-native-browser.test.sh richos/tools/richos-extension richos/app/ui/tests/lib/quiet-browser.js richos/tools/richos-service richos/app/scripts/autocheck/autocheck.py richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/tools/richos-extension/tests/native-transport-e2e.mjs richos/tools/richos-extension/tests/buffered-export.mjs richos/tools/richos-extension/tests/chrome-process.mjs
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
node "$ROOT/richos/tools/richos-extension/tests/native-transport-e2e.mjs"
