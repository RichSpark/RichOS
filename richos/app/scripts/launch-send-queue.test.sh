#!/usr/bin/env bash
# launch-send-queue.test.sh — every Send pressed before the launch has opened his conversation
# reaches Rich, in the order he pressed them (handover review rv-20261020T191206Z-fa2ee4a4-778c).
# Runs main.js's real send() and openThread() over the real timeline model in Node; no browser.
# run-tests: no-host-screen: Node over the UI's own source; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/launch-send-queue.test.sh richos/app/ui/tests/lib/launch-send-queue.test.js richos/app/ui/main.js richos/app/ui/timeline.js
# run-tests: covers richos/app/ui/main.js richos/app/ui/tests/lib/launch-send-queue.test.js
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
node "$here/../ui/tests/lib/launch-send-queue.test.js"
