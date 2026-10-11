#!/usr/bin/env bash
# Fixtures for scripts/lib/recovery_runner.py, scripts/lib/recovery_controls.py,
# scripts/lib/recovery.py and scripts/recovery-runner.test.py.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/recovery-runner.test.py" "$@"
