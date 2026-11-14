#!/usr/bin/env bash
# spawn.sh numbers a teammate's first name by the active teammates of that first name.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/spawn-number.test.py"
