#!/usr/bin/env bash
# Offline QA regressions: process ownership, read refusals, captions and quota probes.
# run-tests: no-host-screen: synthetic providers and recorded AX/process tables only
# run-tests: inputs richos/app/scripts/qa-regressions.test.sh richos/app/scripts/qa richos/app/scripts/qa.test.sh richos/app/scripts/lib richos/app/crates/richos-devtools richos/app/ui/tests/lib/browser-reaper.js richos/engine/scripts/lib
# run-tests: covers richos/app/scripts/qa/test/run-regressions.sh richos/app/scripts/qa/test/caption-words.test.py richos/app/scripts/qa/test/process-identity.test.py richos/app/scripts/qa/test/text-reads.test.py richos/app/scripts/qa/test/usage-arguments.test.py richos/app/scripts/qa/test/usage-deadline.test.py richos/app/scripts/qa/test/usage-wire.test.py
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
unset RICHOS_DEVTOOLS_BIN
RICHOS_DEVTOOLS_BIN="$(python3 -B "$HERE/lib/devtools_bridge.py" --binary)"
export RICHOS_DEVTOOLS_BIN
exec bash "$HERE/qa/test/run-regressions.sh"
