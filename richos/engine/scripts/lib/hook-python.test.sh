#!/usr/bin/env bash
# hook-python.test.sh: no hook can reach Apple's python3 (or git) install-dialog stub
# (scripts/lib/hook-python.sh). Fake Macs under a temporary root, the whole hook chain of the
# terminal plugin and the desktop app run on one; no real stub ever runs.
# The cases are in scripts/lib/hook-python.test.py.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$here/hook-python.test.py"
