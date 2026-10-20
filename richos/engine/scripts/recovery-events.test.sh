#!/usr/bin/env bash
# Mechanics of scripts/lib/recovery_events.py, the terminal event adapter, on the
# registry suite's sandbox (mega-lander/tests/workspaces.test.py's Env and Base):
# refused dispatches, same-worker round 4, duplicates, replacement, restart,
# missing context, provider failure, question delivery and questions-only refusals.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B -W ignore "$HERE/recovery-events.test.py" "$@"
