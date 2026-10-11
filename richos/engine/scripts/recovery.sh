#!/usr/bin/env bash
# Inspect recovery state or request an exact-finding applicability review.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/lib/recovery_admin.py" "$@"
