#!/usr/bin/env bash
# Native desktop callback fixtures for scripts/app-engine-hook.py and scripts/test-app-engine-hook.py.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/test-app-engine-hook.py" "$@"
