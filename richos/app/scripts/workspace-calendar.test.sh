#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/workspace-calendar.test.sh richos/tools/richos-service/lib richos/tools/richos-service/test/workspace.js richos/tools/richos-service/test/calendar-seed-acceptance.mjs
# run-tests: covers richos/tools/richos-service/lib/workspace/adapters/google-calendar.js richos/tools/richos-service/lib/workspace/adapters/microsoft-calendar.js richos/tools/richos-service/test/workspace.js
#
# The workspace service's own pure suite (`tools/richos-service/test/workspace.js`: adapters fed
# fixtures and injected clients, no network, no account), given a seat in the proof selector for
# the two calendar adapters it asserts on. Until 2026-10-14 no script suite claimed them, so a
# change to how a calendar event is normalized (the meeting link the meeting-notes prompt reads,
# handover review rv-20261014T105556Z-71dc7488-f9ab #3) was UNCOVERED at commit.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"
node richos/tools/richos-service/test/workspace.js
