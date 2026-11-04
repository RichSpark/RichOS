#!/usr/bin/env bash
# Always-on accounting and the optional display, using synthetic local fixtures.
# run-tests: no-host-screen: headless WebKit and core fixtures, no model or network
# run-tests: inputs richos/app/scripts/swearmeter.test.sh richos/app/ui/tests/swearmeter.js richos/app/ui/swearmeter.js richos/app/ui/settings-button.js richos/app/ui/home.js richos/app/ui/home.css richos/app/ui/mock.js richos/app/src-tauri/src/swearmeter_host.rs richos/app/src-tauri/src/question_host.rs richos/app/crates/richos-core/src/swearmeter.rs richos/app/crates/richos-core/src/swearmeter_ledger.rs richos/app/crates/richos-core/src/swearmeter_questions.rs richos/app/crates/richos-core/tests/swearmeter.rs
# run-tests: covers richos/app/ui/swearmeter.js richos/app/src-tauri/src/swearmeter_host.rs
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo test --manifest-path "$here/../Cargo.toml" -p richos-core --test swearmeter
cargo test --manifest-path "$here/../Cargo.toml" -p richos-core --lib swearmeter
node "$here/../ui/tests/swearmeter.js"
