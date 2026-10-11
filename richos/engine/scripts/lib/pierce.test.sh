#!/usr/bin/env bash
# Covers guard-pierce.sh, pierce.py, agents/pierce.md and hooks/hooks.json.
# Covers pierce_shadow.py: the Haiku parallel track (data collection only).
# Also checks app-engine-hook.py and engine_profile.rs for dispatch integration.
# Covers mega-lander/workspaces.py: concurrent hooks cannot register a refused spawn.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/pierce.test.py" "$@"
