#!/usr/bin/env bash
# Native parity, persistence, visible controls and Release exclusion. Missing host
# tools/runtime exit 2 with NOT RUN; a build or test failure on a capable host is red.
# run-tests: no-host-screen: simctl and XCUITest use a dedicated simulator without opening Simulator.app
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/mobile/machine_storage.py richos/mobile/cli/machine-settings.mjs richos/web/web-app/styles.css richos/web/web-app/icons richos/mobile/release-config.json richos/mobile/service richos/app/scripts/mobile-ios.test.sh richos/mobile/ios richos/mobile/cli richos/mobile/core richos/mobile/dev richos/mobile/ui richos/mobile/platform richos/mobile/test/proof-driver.mjs richos/mobile/test/fixtures richos/web/web-app/lib richos/mobile/test/run-suite.cjs richos/mobile/test/storage.cjs
# run-tests: covers richos/mobile/test/fixtures/notification-preview.json richos/mobile/ios/Shared/NotificationPreview.swift richos/mobile/ios/NotificationExtension/NotificationService.swift richos/mobile/ios/Tests/NotificationPreviewTests.swift richos/mobile/ios/Sources/RecordingRecovery.swift richos/mobile/ios/Tests/RecordingRecoveryTests.swift richos/mobile/ios/Sources/PushService.swift richos/mobile/ios/Sources/PairScanner.swift richos/mobile/ios/Sources/UpdateService.swift richos/mobile/ui/client.css richos/mobile/ios/Sources/NativeServices.swift richos/mobile/ios/UITests/NativeClientTests.swift richos/mobile/ui/client-entry.js richos/mobile/ui/client.html richos/mobile/dev/client-inspect.js richos/mobile/ios/Sources/AppDelegate.swift richos/mobile/ios/UITests/ComposerTests.swift richos/mobile/dev/entry.js richos/mobile/ui/index.html richos/mobile/ui/style.css richos/mobile/ui/view.js richos/mobile/test/proof-driver.mjs
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo '  NOT RUN  mobile-ios: Node is unavailable'
  exit 2
fi
node "$ROOT/richos/mobile/test/run-suite.cjs" ios
