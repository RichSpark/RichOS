#!/usr/bin/env bash
# verify-runtime.test.sh — the runtime inventory verifier and the tracked runtime recipe.
# Synthetic runtimes in temporary directories plus a read of the tracked JSON files; nothing is
# downloaded, built or launched.
# run-tests: no-host-screen: temporary files only; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/verify-runtime.test.sh richos/app/scripts/verify-runtime.test.py richos/app/scripts/verify-runtime.py richos/app/scripts/runtime-sources.json richos/engine/voice/models/model-pins.json richos/app/scripts/lib/source_status.py
# run-tests: covers richos/app/scripts/verify-runtime.py richos/app/scripts/runtime-sources.json
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/verify-runtime.test.py"
