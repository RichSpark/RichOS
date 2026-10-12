#!/usr/bin/env bash
# dependency-pins.test.sh — a branch commit that changes a file the engine's verification-input
# selector pins by SHA-256 is refused unless the same change renews the pin, and the refusal
# names the file and the command that renews it. Throwaway Git repositories with the real
# autocheck hooks; nothing builds, boots or opens a window, and this repository's hooks are
# never touched.
# run-tests: no-host-screen: throwaway Git repositories only; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/dependency-pins.test.sh richos/app/scripts/dependency-pins.test.py richos/app/scripts/autocheck richos/app/scripts/autocheck/autocheck.py richos/engine/scripts/lib/verification_inputs.py richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/app/scripts/autocheck/dependency-pins.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/dependency-pins.test.py"
