#!/usr/bin/env bash
# run-tests: inputs richos/app/ui/tests richos/engine/scripts/lib richos/app/scripts/ui-ledger.test.py richos/app/scripts/ui-ledger.test.sh
# run-tests: covers richos/app/ui/tests/run.js richos/app/ui/tests/lib/blocking-stdio.js richos/app/ui/tests/lib/ui-sources.js richos/app/ui/tests/lib/harness.js
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$DIR/ui-ledger.test.py"
echo '  PASS  UI ledger publication races and abandoned-owner cleanup'
