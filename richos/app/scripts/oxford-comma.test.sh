#!/usr/bin/env bash
# oxford-comma.test.sh — the fixer git runs at every commit deletes ", and", ", or" and ", nor"
# from the docs and UI text a commit changes, and never from code (CEO, 2026-10-19). The fixer's
# own --json interface over planted text, reading the real UI tree only to classify a path;
# nothing builds, boots or opens a window, and nothing is written.
# run-tests: no-host-screen: the fixer over planted text; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/oxford-comma.test.sh richos/app/scripts/oxford-comma.test.py richos/app/scripts/oxford-comma.js richos/app/ui/oxford-comma.js richos/app/ui/tests/lib/state-strings.js richos/app/ui/tests/lib/ui-sources.js
# run-tests: covers richos/app/scripts/oxford-comma.js
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/oxford-comma.test.py"
