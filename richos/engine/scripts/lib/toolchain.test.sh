#!/usr/bin/env bash
# toolchain.test.sh: nothing an agent or a test runs reaches an Apple developer-tools stub without
# a usable toolchain behind it (scripts/lib/toolchain.py, hooks/guard-toolchain.sh, host_shield).
# Four Macs (Xcode in /Applications, Command Line Tools only, no developer tools, Xcode on another
# volume) plus the ask-once ledger, built under a temporary root; no real stub ever runs.
# The cases are in scripts/lib/toolchain.test.py.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$here/toolchain.test.py"
