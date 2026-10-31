#!/usr/bin/env bash
# Only real Agent dispatches run the semantic inspection, not spawn.sh's dry runs.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$HERE/../lib/pierce.py"   # a function (hook-python.sh), so never exec
