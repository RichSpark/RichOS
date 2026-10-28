#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
unset RICHOS_DEVTOOLS_BIN
RICHOS_DEVTOOLS_BIN="$(python3 -B "$SCRIPT_DIR/../../../app/scripts/lib/devtools_bridge.py" --binary)"
export RICHOS_DEVTOOLS_BIN
PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/session-evidence.test.py" -v
"$SCRIPT_DIR/session-evidence.mutation.sh"
