#!/usr/bin/env bash
# autocheck.test.sh — git runs the basic checks itself: the lint at every commit, the owning
# suites plus the lint at every land into main, and every --no-verify is recorded for the
# lead (CEO ruling §97). Throwaway Git repositories with stand-in tools and a scratch
# escalation ledger; nothing builds, boots or opens a window, and this repository's hooks
# are never touched.
# run-tests: no-host-screen: throwaway Git repositories only; nothing is launched on any screen
# run-tests: inputs richos/engine/scripts/lib richos/engine/orchestration.config richos/app/scripts/lib/owned_command.py richos/app/scripts/lint/load_rules.py richos/app/scripts/lint/common.py richos/app/scripts/autocheck.test.sh richos/app/scripts/autocheck.test.py richos/app/scripts/autocheck richos/app/scripts/battery-check.py richos/engine/scripts/escalate.sh richos/engine/scripts/lib/escalations.py richos/engine/scripts/lib/escalations.sh richos/app/scripts/nightly-local.mutation.py richos/app/scripts/runner-reliability.mutation.py richos/app/scripts/phone-apps-independent.mutation.py richos/app/scripts/testvm/test/claude-login.mutation.py docs/development/verification-input-qualifications.json
# run-tests: covers richos/app/scripts/lib/owned_command.py richos/app/scripts/autocheck/autocheck.py richos/app/scripts/autocheck/shim.sh richos/app/scripts/autocheck/install.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/autocheck.test.py"
