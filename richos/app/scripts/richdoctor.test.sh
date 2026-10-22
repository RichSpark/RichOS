#!/usr/bin/env bash
# RichDoctor's own suite (the separate support app, crates/richdoctor and crates/richdoctor-core).
# The core crate's unit and integration tests are selected per file by proof-for.sh; this suite
# adds what no per-file rule reaches: the live adapter probe's own unit test (its live run is
# `cargo run --example adapter_probe`, measured in crates/richdoctor-core/MEASURED.md, and is
# never run here: it uses the person's Claude Code account).
# run-tests: no-host-screen: it builds and runs unit tests; it opens no window and launches no app
# run-tests: inputs richos/app/scripts/richdoctor.test.sh richos/app/crates/richdoctor-core richos/app/crates/richos-core/src/bug_report.rs
# run-tests: covers richos/app/crates/richdoctor-core/examples/adapter_probe.rs
set -euo pipefail
app="$(cd "$(dirname "$0")/.." && pwd)"

echo "== the core crate's examples (their own unit tests) =="
cargo test --offline --manifest-path "$app/crates/richdoctor-core/Cargo.toml" --examples
echo "richdoctor.test.sh: passed"
