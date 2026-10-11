#!/usr/bin/env bash
# Isolated state fixtures. No real advisor, registry or user transcript.
# Exercises scripts/lib/recovery.py and scripts/recovery.test.py.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/recovery.test.py" "$@"
