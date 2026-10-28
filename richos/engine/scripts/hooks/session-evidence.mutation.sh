#!/usr/bin/env bash
# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$SCRIPT_DIR" <<'PY'
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile

source = Path(sys.argv[1]).resolve().parents[2]
# dispatch-pretooluse.manifest is in this list because it is a REGISTRATION
# SURFACE: since 2026-09-15 seventeen guards are wired by one line each there
# rather than by an entry in hooks.json, and session-evidence.test.py's
# inventory comparison reads it. A sandbox without it makes that comparison see
# seventeen declared-but-unregistered guards and fail the baseline before a
# single mutation has been applied.
files = ['engine/hooks/hooks.json', 'app/scripts/rust-test-summary.py',
         'app/scripts/lib/devtools_bridge.py', 'app/scripts/lib/cargo_identity.py',
         'app/scripts/lib/machine_storage.py', 'engine/scripts/lib/machine_settings.py'] + [
    'engine/scripts/hooks/' + n for n in ('session-evidence.test.py', 'shell-evidence.py',
    'shell-evidence.sh', 'contract-integrity-probe.sh', 'commit-ceo-inputs.py', 'notice-ceo-inputs-unheld.sh', 'turn-manifest.py',
    'dispatch-pretooluse.manifest')]
mutations = [
 ('plugin hook absent from managed inventory', 'engine/scripts/hooks/contract-integrity-probe.sh',
  'shell-evidence.sh|PreToolUse', '', 'test_real_wrapper_and_registration'),
 # The same property on the other registration surface. A guard switched off by
 # deleting its manifest line has to be as visible as one switched off by
 # deleting its hooks.json entry, or the manifest is a place enforcement can go
 # quiet with nothing to say so.
 ('dispatcher rule absent from the manifest', 'engine/scripts/hooks/dispatch-pretooluse.manifest',
  'Bash|guard-ci-red-lands.sh', '', 'test_real_wrapper_and_registration'),
 ('shell failures hidden', 'engine/scripts/hooks/shell-evidence.py', 'set -e -o pipefail', 'set +e +o pipefail',
  'test_original_failures_are_reproduced_and_fixed_in_bash_and_zsh'),
 ('notifications treated as handovers', 'engine/scripts/hooks/commit-ceo-inputs.py',
  'text = re.sub(r"<(task-notification|system-reminder)\\b[^>]*>.*?</\\1>", "", text, flags=re.S)',
  'text = text', 'test_task_notification_does_not_become_a_handover'),
 ('directories treated as files', 'engine/scripts/hooks/commit-ceo-inputs.py',
  'if not is_handover_file(p):', 'if False:', 'test_root_and_directories_do_not_reach_ledger'),
 ('historical directory noise retained', 'engine/scripts/hooks/notice-ceo-inputs-unheld.sh',
  'if not ingress.is_handover_file(p):', 'if False:', 'test_historical_noise_is_retired_but_real_unheld_file_remains'),
 ('shell output called verified', 'engine/scripts/hooks/turn-manifest.py',
  'word = "RETURNED" if tool_name == "Bash" else "ok"', 'word = "ok"', 'test_manifest_does_not_call_unverified_bash_output_ok'),
 ('documentation counts mixed', 'app/crates/richos-devtools/src/reports.rs',
  'kind = 1;', 'kind = 0;', 'test_rust_doc_tests_are_counted_separately'),
]
with tempfile.TemporaryDirectory(prefix='session-evidence-mutations-') as tmp:
    root = Path(tmp)
    # Compile the source in this private copy. A prebuilt override would hide a
    # mutation in the Rust implementation behind an unchanged Python adapter.
    environment = dict(os.environ)
    environment.pop('RICHOS_DEVTOOLS_BIN', None)
    crate = 'app/crates/richos-devtools'
    files += [str(path.relative_to(source)) for path in (source/crate).rglob('*')
              if path.is_file() and (path.suffix == '.rs' or path.name in
                                    ('Cargo.toml', 'Cargo.lock', 'varcon_notice.txt'))]
    for rel in files:
        target = root/rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source/rel, target)
    test = root/'engine/scripts/hooks/session-evidence.test.py'
    baseline = subprocess.run([sys.executable, str(test)], env=environment, capture_output=True, text=True)
    if baseline.returncode:
        raise SystemExit('Baseline failed: '+baseline.stderr)
    for name, rel, old, new, method in mutations:
        path = root/rel
        original = path.read_bytes()
        text = original.decode()
        assert text.count(old) == 1, (name, 'mutation target must be unique')
        path.write_text(text.replace(old, new))
        try:
            run = subprocess.run([sys.executable, str(test), 'SessionEvidence.'+method], env=environment, capture_output=True, text=True)
            if run.returncode == 0 or 'FAIL' not in run.stderr:
                raise SystemExit('Mutation was not caught by its regression: '+name+'\n'+run.stderr)
            print('PASS negative control:', name)
        finally:
            path.write_bytes(original)
            assert path.read_bytes() == original
print(f'{len(mutations)} negative controls passed; source worktree was never mutated.')
PY
