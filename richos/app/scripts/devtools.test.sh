#!/usr/bin/env bash
# run-tests: no-host-screen: synthetic logs and source snippets only
# run-tests: inputs richos/app/scripts/devtools.test.sh richos/app/scripts/devtools.test.py richos/app/scripts/lib/devtools_bridge.py richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/machine_storage.py richos/engine/scripts/lib/machine_settings.py richos/app/crates/richos-devtools richos/app/scripts/rust-test-summary.py richos/app/scripts/qa/caption-wer.py richos/app/scripts/qa/ledger-timeline.py richos/app/ui/tests/lib/wait-shape.js richos/engine/scripts/lib/dialect/dialect-table.py
# run-tests: covers richos/app/scripts/devtools.test.py richos/app/scripts/lib/devtools_bridge.py richos/app/scripts/rust-test-summary.py richos/app/crates/richos-devtools/src/main.rs richos/app/crates/richos-devtools/tests/fixtures/original-outputs.json
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/devtools.test.py"
