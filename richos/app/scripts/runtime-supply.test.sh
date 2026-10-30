#!/usr/bin/env bash
# runtime-supply.test.sh: a check that needs RICHOS_RUNTIME_DIR or the speech models gets them without
# anyone setting them by hand: lib/runtime_cache.py builds the runtime for the recipe (under a lock, never
# over an existing delivery), proof-run.py asks it when a selected check needs it, and a missing speech
# model is refused up front with the command that fetches it. Stand-in builders and fixture folders only.
# run-tests: no-host-screen: stand-in builders and fixture folders; nothing is downloaded, built or launched
# run-tests: inputs richos/app/scripts/runtime-supply.test.sh richos/app/scripts/runtime-supply.test.py richos/app/scripts/lib/runtime_cache.py richos/app/scripts/proof-run.py
# run-tests: covers richos/app/scripts/runtime-supply.test.py richos/app/scripts/lib/runtime_cache.py
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PYTHONDONTWRITEBYTECODE=1
python3 -B "$HERE/runtime-supply.test.py" "$@"
