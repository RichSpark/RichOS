#!/usr/bin/env bash
# RichDoctor's own suite (the separate support app, crates/richdoctor and crates/richdoctor-core).
# The core crate's unit and integration tests are selected per file by proof-for.sh; this suite
# adds what no per-file rule reaches: the live adapter probe's own unit test, and that the committed
# PNG screenshot fixtures are byte for byte what the encoder writes today (its live run is
# `cargo run --example adapter_probe`, measured in crates/richdoctor-core/MEASURED.md, and is
# never run here: it uses the person's Claude Code account).
# run-tests: no-host-screen: it builds and runs unit tests; it opens no window and launches no app
# run-tests: inputs richos/app/scripts/richdoctor.test.sh richos/app/crates/richdoctor-core richos/app/crates/richos-core/src/bug_report.rs richos/mobile/service/report/fixtures/client
# run-tests: covers richos/app/crates/richdoctor-core/examples/adapter_probe.rs richos/app/crates/richdoctor-core/examples/profile_fixtures.rs
set -euo pipefail
app="$(cd "$(dirname "$0")/.." && pwd)"

echo "== the core crate's examples (their own unit tests) =="
cargo test --offline --manifest-path "$app/crates/richdoctor-core/Cargo.toml" --examples
echo "== the shared screenshot fixtures are what RichDoctor's encoder writes today =="
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cargo run -q --offline --manifest-path "$app/crates/richdoctor-core/Cargo.toml" --example profile_fixtures -- "$work"
cmp "$work/accept-final.png" "$app/../mobile/service/report/fixtures/client/accept-final.png"
cmp "$work/reject-text-chunk.png" "$app/../mobile/service/report/fixtures/client/reject-text-chunk.png"
echo "richdoctor.test.sh: passed"
