#!/usr/bin/env bash
# Fake-provider fixtures for scripts/lib/recovery_triage.py, recovery-profiles/triage.md
# and the triage subcommand of scripts/recovery.sh (scripts/lib/recovery_admin.py).
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/recovery-triage.test.py" "$@"
