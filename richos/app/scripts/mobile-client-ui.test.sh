#!/usr/bin/env bash
# The earlier iPhone app's bundled screens (richos/mobile/ui), rendered in Chromium and WebKit.
# Until 2026-10-30 this suite was mobile-pwa.test.sh and also drove the phone web page the Mac
# served; that page was retired (CEO: "yes, the phone web page needs to be retired now") and its
# browser proofs went with it.
# run-tests: no-host-screen: proof-driver forces a headless browser and stops it on exit
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/mobile/machine_storage.py richos/mobile/cli/machine-settings.mjs richos/mobile/test/client-ui.mjs richos/mobile/ui richos/mobile/core richos/mobile/platform richos/mobile/release-config.json richos/app/scripts/mobile-client-ui.test.sh richos/mobile/cli richos/mobile/dev/runtime.js richos/mobile/dev/client-runtime.js richos/mobile/test/proof-driver.mjs richos/web/web-app/lib richos/web/web-app/styles.css richos/app/ui/tests/lib richos/mobile/test/run-suite.cjs richos/mobile/test/storage.cjs
# run-tests: covers richos/web/web-app/lib/follow.js richos/web/web-app/lib/notification-target.js richos/mobile/ui/client-entry.js richos/mobile/ui/client.css richos/mobile/test/client-ui.mjs richos/mobile/test/proof-driver.mjs
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo '  NOT RUN  mobile-client-ui: Node is unavailable'
  exit 2
fi
node "$ROOT/richos/mobile/test/run-suite.cjs" client-ui
