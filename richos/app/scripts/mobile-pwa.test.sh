#!/usr/bin/env bash
# The actual PWA browser worker is a separate proof, never inferred from Node tests.
# run-tests: no-host-screen: proof-driver forces a headless browser and stops it on exit
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/mobile/machine_storage.py richos/mobile/cli/machine-settings.mjs richos/mobile/test/client-ui.mjs richos/mobile/ui richos/mobile/core richos/mobile/platform richos/mobile/release-config.json richos/app/scripts/mobile-pwa.test.sh richos/mobile/cli richos/mobile/core richos/mobile/dev/runtime.js richos/mobile/test/proof-driver.mjs richos/web/web-app richos/tools/phone-probe/lib richos/app/ui/tests/lib richos/mobile/test/run-suite.cjs richos/mobile/test/storage.cjs
# run-tests: covers richos/web/web-app/test/stub-mac.js richos/web/web-app/lib/follow.js richos/web/web-app/lib/notification-target.js richos/web/web-app/app.js richos/web/web-app/index.html richos/web/web-app/styles.css richos/web/web-app/sw.js richos/web/web-app/test/desktop-verify.js richos/mobile/ui/client-entry.js richos/mobile/ui/client.css richos/mobile/test/client-ui.mjs richos/mobile/cli/pwa.mjs richos/mobile/cli/pwa-worker.mjs richos/mobile/test/proof-driver.mjs
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo '  NOT RUN  mobile-pwa: Node is unavailable'
  exit 2
fi
node "$ROOT/richos/mobile/test/run-suite.cjs" pwa
