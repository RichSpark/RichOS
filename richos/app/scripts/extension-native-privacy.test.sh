#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/extension-native-privacy.test.sh richos/tools/richos-extension/tests/native-transport-e2e.mjs richos/tools/richos-extension/tests/native-host-privacy.mjs richos/tools/richos-extension/tests/native-host-privacy.test.mjs richos/tools/richos-extension/tests/chrome-process.mjs richos/tools/richos-extension/tests/buffered-export.mjs richos/tools/richos-service/lib/workspace/privacy.js
# run-tests: covers richos/tools/richos-extension/tests/native-host-privacy.mjs richos/tools/richos-extension/tests/native-host-privacy.test.mjs
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
node "$ROOT/richos/tools/richos-extension/tests/native-host-privacy.test.mjs"
