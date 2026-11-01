#!/usr/bin/env bash
# Shared engine helper, on the same runtime PATH as the other engine commands.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$SCRIPT_DIR/lib/codex_channels.py" "$@"
