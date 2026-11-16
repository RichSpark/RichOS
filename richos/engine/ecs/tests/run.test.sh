#!/usr/bin/env bash
# The ECS unittests. test_app.py drives the continuity tools' adapter, ecs/adapters/mcp.py
# (`from mcp import call`), including the scope bound it applies. The offline handover
# cases drive the same projections in ecs_inspect.py against an unchanged app store.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -m unittest discover -s "$HERE" -p 'test_*.py'
