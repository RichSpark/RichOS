#!/usr/bin/env bash
# lib/launchd-install.sh refuses a failed plist move, using fixture launchctl only.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PYTHONDONTWRITEBYTECODE=1
python3 -B "$HERE/launchd-install-failures.test.py" "$@"
