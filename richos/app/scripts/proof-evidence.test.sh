#!/usr/bin/env bash
# Saved proof evidence: generated qualification, reuse, joining and receipt fixtures.
# Production recipes and reader pins are checked by proof-qualification.test.sh.
# Runner scheduling and ownership fixtures remain in proof-run.test.sh.
# run-tests: no-host-screen: private fixture commands only
# The CLI fixtures copy the engine helper inventory; these named helpers also select at --gate.
# run-tests: inputs richos/engine/scripts/lib richos/engine/scripts/hooks/guard-host-display-power.sh richos/engine/scripts/lib/verification-fixture.sh richos/engine/scripts/lib/cpu_guard.py richos/engine/scripts/lib/engine_pass.py richos/engine/scripts/lib/cpu_policy.py richos/engine/scripts/lib/native-work.py richos/engine/scripts/lib/worker_tokens.py richos/engine/scripts/lib/proc_tree.py richos/engine/scripts/lib/operator_fences.py richos/engine/scripts/lib/ci-receipts.py richos/engine/scripts/ci-shard.sh richos/engine/scripts/ci-units.sh richos/engine/VERSION richos/app/scripts/proof-evidence.test.sh richos/app/scripts/proof-evidence.test.py richos/app/scripts/proof-run.py richos/app/scripts/testvm/reserve.py richos/app/scripts/lib/proof_evidence.py richos/app/scripts/lib/proof_slots.py richos/app/scripts/lib/test_results.py richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/rerun_tree.py richos/app/scripts/lib/runtime_cache.py
# run-tests: covers richos/app/scripts/proof-evidence.test.py richos/app/scripts/lib/proof_evidence.py richos/engine/scripts/lib/ci-receipts.py richos/app/scripts/lib/rerun_tree.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$here/../../engine/scripts/lib/verification-fixture.sh"
python3 "$here/proof-evidence.test.py"
