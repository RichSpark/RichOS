#!/usr/bin/env bash
# Shared engine helper, on the same runtime PATH as the other engine commands.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -n "${RICHOS_RUNTIME_PYTHON:-}" ]; then
    interpreter="$RICHOS_RUNTIME_PYTHON"
elif [ -x "$SCRIPT_DIR/../runtime/bin/python3" ]; then
    interpreter="$SCRIPT_DIR/../runtime/bin/python3"
else
    interpreter=python3  # source checkout / existing terminal engine environment
fi
exec "$interpreter" -B "$SCRIPT_DIR/lib/codex_channels.py" "$@"
