#!/usr/bin/env bash
# The ECS unittests. test_app.py drives the continuity tools' adapter, ecs/adapters/mcp.py
# (`from mcp import call`), including its scope checks. The offline handover uses
# ecs_inspect.py against an unchanged app store. Waiting work uses
# ecs_extract.py, publication.py and 008_waiting_conditions.sql. test_import.py
# covers binding a pending imported item to a confirmed order.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -m unittest discover -s "$HERE" -p 'test_*.py'
