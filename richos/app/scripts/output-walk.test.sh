#!/usr/bin/env bash
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/app/scripts/output-walk.test.sh richos/app/scripts/output-walk.test.py richos/app/scripts/testvm/output-walk.py richos/app/scripts/testvm/command-walk.py richos/app/scripts/testvm/adopt-walk.py richos/app/scripts/testvm/relaunch.py
# run-tests: covers richos/app/scripts/output-walk.test.py richos/app/scripts/testvm/output-walk.py
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Offline checks of dispatch, failure evidence and the candidate walk's boundaries.
# Actual guest/UI behavior still needs the real candidate walk.
python3 -B "$DIR/output-walk.test.py"
echo '  PASS  output walk boundaries'
