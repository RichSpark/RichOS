#!/usr/bin/env bash
# Covers agent_roster.py: the folder every reader of the teammate definitions takes under an app lease.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/agent_roster.test.py" "$@"
