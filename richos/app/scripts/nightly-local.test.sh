#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/lib/owned_command.py richos/app/scripts/nightly-local.test.sh richos/app/scripts/nightly-local.test.py richos/app/scripts/nightly-local.mutation.py richos/app/scripts/nightly-local.py richos/app/scripts/nightly.py richos/app/scripts/make-release.sh richos/app/scripts/package-app.sh richos/app/scripts/run-tests.sh richos/app/scripts/gui-proof-in-vm.sh richos/app/scripts/lib/home-probe.sh richos/engine/scripts/lib/worker_tokens.py richos/engine/scripts/lib/proc_tree.py richos/engine/scripts/lib/testdevices.py richos/app/scripts/lib/runtime_cache.py richos/app/scripts/lib/nightly_gate_evidence.py richos/app/scripts/lib/proof_evidence.py
# run-tests: covers richos/app/scripts/lib/owned_command.py richos/app/scripts/nightly-local.py richos/app/scripts/nightly-local.mutation.py richos/app/scripts/lib/runtime_cache.py richos/app/scripts/lib/nightly_gate_evidence.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$here/nightly-local.test.py"
# Each property of gates-at-once watched going red (the shipped file is never written).
if [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
  python3 "$here/nightly-local.mutation.py"
fi
echo '  PASS  manual-only releases, local credentials, worktree isolation and failure gates'
