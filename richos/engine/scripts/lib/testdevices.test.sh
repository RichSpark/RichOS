#!/usr/bin/env bash
#
# testdevices.test.sh — the test-device collector (scripts/lib/testdevices.py),
# TESTED BY DEFEAT: every removal beside the case that must not remove.
#
# 2026-09-22: three iOS simulators booted by killed test runs outlived their
# agents and the session. The cases are in testdevices.test.py; this file only
# allocates the sandbox with the engine's own allocator, so a cleanup suite is
# never the thing that leaves garbage, and releases it however the run ends.
# Simulators here are rows in a fake simctl; the machine's own are never read.
# The cases also cover testdevice_alerts.py, the collector's failure writer and
# the standing() rule the disk watchdog counts with (T32-T32d).
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 is required" >&2; exit 1; }

# shellcheck source=scratch.sh
. "$SCRIPT_DIR/scratch.sh"
SANDBOX="$(scratch_new testdevices-test --ttl 30)" || { echo "FATAL: no sandbox" >&2; exit 1; }
trap 'scratch_release "$SANDBOX" >/dev/null 2>&1 || true' EXIT

echo "=== testdevices tests ==="
TESTDEVICES_SANDBOX="$SANDBOX" python3 -B -W ignore "$SCRIPT_DIR/testdevices.test.py" "$@" || exit 1
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ "$#" -eq 0 ] && [ -f "$SCRIPT_DIR/testdevices.mutation.sh" ]; then
    bash "$SCRIPT_DIR/testdevices.mutation.sh" || exit 1
fi
exit 0
