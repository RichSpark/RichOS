#!/usr/bin/env bash
# PreToolUse[Agent] input transformer. Never blocks, never approves.
# UNEVALUATED-PAYLOAD-EXEMPT: payload-independent — this hook only rewrites the prompt of a payload it can read; it never blocks or approves, so an unreadable payload passes through unchanged, which is exactly what it should do.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if command -v python3 >/dev/null 2>&1 && [ -f "$SCRIPT_DIR/strip-ack-lines.py" ]; then
    python3 "$SCRIPT_DIR/strip-ack-lines.py"; exit $?   # a function (hook-python.sh), so never exec
fi
exit 0
