#!/usr/bin/env bash
# caption-wer.py: rolling WebVTT read once, WER counts, capitalization and spelling pairs, loops.
# run-tests: no-host-screen: synthetic captions and transcripts only; no capture or input
# run-tests: inputs richos/app/scripts/caption-wer.test.sh richos/app/scripts/qa/caption-wer.py richos/app/scripts/qa/caption-wer.test.py richos/app/crates/richos-devtools richos/app/scripts/lib/devtools_bridge.py richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/machine_storage.py richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/app/scripts/qa/caption-wer.py richos/app/scripts/qa/caption-wer.test.py
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
unset RICHOS_DEVTOOLS_BIN
RICHOS_DEVTOOLS_BIN="$(python3 -B "$HERE/lib/devtools_bridge.py" --binary)"
export RICHOS_DEVTOOLS_BIN
exec python3 -B "$HERE/qa/caption-wer.test.py"
