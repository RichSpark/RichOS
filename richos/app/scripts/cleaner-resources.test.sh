#!/usr/bin/env bash
# Resource manifest tampering and the narrow pinned PDF Unicode correction.
# run-tests: no-host-screen: Python temporary-file fixtures only
# run-tests: inputs richos/app/scripts/cleaner-resources.test.sh richos/app/scripts/cleaner-resources.test.py richos/app/scripts/build-cleaner-resources.py
# run-tests: covers richos/app/scripts/cleaner-resources.test.py richos/app/scripts/build-cleaner-resources.py
set -euo pipefail
PYTHONDONTWRITEBYTECODE=1 python3 "$(dirname "${BASH_SOURCE[0]}")/cleaner-resources.test.py"
