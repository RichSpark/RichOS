#!/usr/bin/env bash
# run-tests: inputs richos/app/ui/tests richos/app/scripts/ui-suite-defects.test.sh richos/app/crates/richos-devtools richos/app/scripts/lib/devtools_bridge.py richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/machine_storage.py richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/app/ui/tests/lib/tracked-tree-keeps-candidates.test.js richos/app/ui/tests/lib/splash-relief-race.test.js richos/app/ui/tests/lib/phone-expired-reopen.test.js richos/app/ui/tests/lib/wait-shape.test.js richos/app/ui/tests/lib/wait-shape.js richos/app/ui/tests/lib/gate-honesty-own-key.test.js richos/app/ui/tests/lib/splash-resume-race.test.js richos/app/ui/tests/lib/registry-rekey.test.js richos/app/ui/tests/lib/registry-rekey.js richos/app/ui/tests/lib/browser-reaper.test.js richos/app/ui/tests/lib/browser-reaper.js richos/app/ui/tests/lib/browser-untouched.test.js richos/app/ui/tests/package.json richos/app/ui/tests/tracked-tree.js
# Guards for defects in the UI suites themselves (hunt part 2, findings 27, 28, 29, 43). No
# browser: each test is decided in the suite source or on the files it writes.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS="$DIR/../ui/tests"
unset RICHOS_DEVTOOLS_BIN
RICHOS_DEVTOOLS_BIN="$(python3 -B "$DIR/lib/devtools_bridge.py" --binary)"
export RICHOS_DEVTOOLS_BIN
node "$TESTS/lib/tracked-tree-keeps-candidates.test.js"
node "$TESTS/lib/splash-relief-race.test.js"
node "$TESTS/lib/phone-expired-reopen.test.js"
node "$TESTS/lib/wait-shape.test.js"
node "$TESTS/lib/browser-reaper.test.js"
node "$TESTS/lib/gate-honesty-own-key.test.js"
node "$TESTS/lib/splash-resume-race.test.js"
node "$TESTS/lib/registry-rekey.test.js"
node "$TESTS/lib/browser-untouched.test.js"
