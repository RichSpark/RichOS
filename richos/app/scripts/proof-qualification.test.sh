#!/usr/bin/env bash
# Production recipes and reviewed pins: source validation only.
# Generated reuse/receipt fixtures remain in proof-evidence.test.sh.
# run-tests: no-host-screen: reads source declarations only
# run-tests: inputs richos/app/scripts/proof-qualification.test.sh richos/app/scripts/proof-qualification.test.py richos/app/scripts/lib/proof_evidence.py richos/app/scripts/proof-inputs.json docs/development/verification-input-qualifications.json docs/development/native-ios-ui-inputs.json richos/engine/scripts/lib/host_shield.py richos/app/scripts/proof-for.ui-inputs richos/app/ui/tests richos/app/scripts/proof-run.py richos/app/scripts/lib/ios_ui_shards.py
# run-tests: covers richos/app/scripts/proof-qualification.test.py richos/app/scripts/lib/proof_evidence.py richos/app/scripts/proof-inputs.json docs/development/verification-input-qualifications.json docs/development/native-ios-ui-inputs.json
# It checks every UI test file for the environment names it reads, so a change to one selects
# it at the merge gate too (a missed land failed nightly 69, 2026-10-20).
# run-tests: gate-inputs richos/app/ui/tests
# Every reviewed recipe reader selects the suite that checks its pin.
# run-tests: pins docs/development/verification-input-qualifications.json
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -B "$here/proof-qualification.test.py"
