#!/usr/bin/env bash
# app-management-watch.test.sh: the read-only App Management watcher (scripts/lib/app_management_watch.py):
# kernel write denials and tccd's App Management attributions reach its ledger with full paths, nothing
# else does, it uses no CPU while idle, it ends with its host, and `log` accepts its predicate.
# The cases are in scripts/lib/app_management_watch.test.py.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$here/app_management_watch.test.py"
