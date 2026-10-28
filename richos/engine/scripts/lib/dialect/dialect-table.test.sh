#!/usr/bin/env bash
# dialect-table.test.sh - runs dialect-table.test.py.
#
# The suite covers, and so names for ci-affected-units.sh, every input and output of the
# generated American spelling table: dialect-table.py, dialect-en-US.generated.dict,
# dialect-en-US.overrides.dict, dialect-en-US.leave.tsv, ../dialect-en-US.dict,
# third_party/varcon/varcon.txt, fixtures/harper-review-list.tsv,
# fixtures/british-keyed.txt, fixtures/british-keyed.context-traps.txt.
# A change to any of them without a regenerated table fails here.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE="$HERE/../../../../app/scripts/lib/devtools_bridge.py"
if [ -f "$BRIDGE" ]; then
  unset RICHOS_DEVTOOLS_BIN
  RICHOS_DEVTOOLS_BIN="$(python3 -B "$BRIDGE" --binary)"
  export RICHOS_DEVTOOLS_BIN
fi
exec python3 -B "$HERE/dialect-table.test.py" "$@"
