#!/usr/bin/env bash
set -euo pipefail
# Behavioral readers in hunt-v5-g4.test.py, named here for the conservative selector:
# dialect-guard.sh dialect-fix.py commit-ceo-inputs.py notice-hook-staleness.sh
# hook-staleness.test.sh scan-secrets.sh guard-named-persons-writes.sh
# guard-named-persons-commands.sh snapshot-enforcing-hooks.sh dispatch-pretooluse.sh
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$here/hunt-v5-g4.test.py" "$@"
