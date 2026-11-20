#!/usr/bin/env bash
# Inspect real Agent dispatches and terminal Codex handoffs before delivery.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$HERE/../lib/pierce.py"   # a function (hook-python.sh), so never exec
