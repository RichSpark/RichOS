#!/usr/bin/env bash
# Part 3 of 8 of app.test.py, the synthetic desktop dispatch across real Git workspaces and the
# actual ECS store. The suite was one unit (app.test.sh) until 2026-11-12; why it is eight and how
# the cases are divided is in app.test.py _part. app.test.sh runs part 1 and the mutation harness.
#
# Every engine file app.test.sh names is named here too, so a change to any of them selects this
# unit exactly as it selects that one (the selector's basename rule), and mega-lander/app.py
# selects it as it selects its sibling suite app.test.sh: app.test.py drives
# scripts/hooks/deliver-review-verdict.sh through scripts/app-engine-hook.py,
# scripts/lib/review_watch.py's app mode, scripts/lib/instruction_files.py, scripts/lib/scope_closure.py
# and scripts/lib/notice_once.py.
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B -W ignore "$HERE/app.test.py" --part 3/8 || exit 1
exit 0
