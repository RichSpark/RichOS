#!/usr/bin/env bash
# PreToolUse input transformer. Preserves host permissions; never auto-approves.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if command -v python3 >/dev/null 2>&1 && [ -f "$SCRIPT_DIR/shell-evidence.py" ]; then
    python3 "$SCRIPT_DIR/shell-evidence.py"; exit $?   # a function (hook-python.sh), so never exec
fi
printf '%s\n' '{"systemMessage":"Shell evidence hook is unavailable; command failure propagation is unverified."}'
