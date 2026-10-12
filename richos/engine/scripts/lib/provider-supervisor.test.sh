#!/usr/bin/env bash
# Covers scripts/provider-supervisor.py, including job process-scratch teardown.
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1
# Fixture providers never sweep the caller's real temp root. R13 sets its own snapshot.
unset RICHOS_MACHINE_SETTINGS_RESOLVED
HERE="$(cd "$(dirname "$0")" && pwd)"
python3 "$HERE/provider-supervisor.test.py"
# M: the operator-mode reaping's mutation harness. Not from inside a mutant's run.
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$HERE/provider-supervisor.mutation.sh" ]; then
    if bash "$HERE/provider-supervisor.mutation.sh"; then
        echo "  PASS  M. every reaping rule above has been watched fail"
    else
        echo "  FAIL  M. the mutation harness found a reaping rule this suite does not prove"
        exit 1
    fi
fi
