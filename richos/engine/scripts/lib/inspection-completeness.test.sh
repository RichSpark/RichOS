#!/usr/bin/env bash
# Fixtures for build_caches.py, recovery_controls.py and ci-shard.sh state inspections.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PYTHONDONTWRITEBYTECODE=1
python3 -B "$HERE/inspection-completeness.test.py" "$@"
