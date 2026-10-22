#!/usr/bin/env bash
# run-tests: inputs richos/app richos/web richos/engine/scripts richos/engine/orchestration.config
# run-tests: covers richos/app/scripts/lint/driver.py richos/app/scripts/lint/common.py richos/app/scripts/lint/rust.py richos/app/scripts/lint/ratchet.py richos/app/scripts/lint/dialect.py richos/app/scripts/lint/process_rules.py richos/app/scripts/lint/shell_source.py richos/app/scripts/lint/timeout_rules.py richos/app/scripts/lint/suite_rules.py richos/app/scripts/lint/advisory_rules.py richos/app/scripts/lint/load_rules.py richos/app/scripts/lint/lock_rules.py richos/app/scripts/lint.sh richos/app/scripts/lint/baselines/custom.json richos/app/scripts/lint/baselines/load.json richos/app/scripts/lint/baselines/rust-fast.json richos/app/scripts/lint/baselines/shell.json richos/app/scripts/lint/baselines/tauri.json
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PYTHONDONTWRITEBYTECODE=1
# `--all`, not `--fast`: the nightly's gates/lint-tauri refuses on the Tauri ceiling in
# baselines/tauri.json, and this suite is what a land runs (proof-for.sh selects it for every
# change under richos/app). With `--fast` here, c2bfe118 passed its land and was then refused
# by the nightly build (let_underscore_must_use 209 > 166, run 20260922T172720Z-a249dfe5).
# A build must not discover what the land could have. Tauri Clippy costs 49 s cold / 4.6 s
# warm (measured in 9a5b9354) under the same 180-second cap the gate uses.
bash "$DIR/lint.sh" --all
echo '  PASS  Rust/shell fast and Tauri ratchets'
