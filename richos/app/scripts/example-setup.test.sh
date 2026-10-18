#!/usr/bin/env bash
# Prove the older development examples still open the conversation their live check needs
# (hunt part 1 finding 46). Each carries its own setup test (Cargo `test = true`); none
# starts a provider, reads a corpus or opens an audio device. The live runs themselves need
# a signed-in claude (and, for voice_loop, audio) and remain opt-in. setup_demo has no setup
# test of its own; building it here proves it still matches the setup API it demonstrates.
# run-tests: no-host-screen: Rust example setup tests; no provider, corpus or audio device
# run-tests: inputs richos/app/src-tauri/examples/stop_payload.rs richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Cargo.lock richos/app/scripts/example-setup.test.sh richos/app/crates/richos-core richos/app/crates/richos-voice richos/app/Cargo.toml richos/app/Cargo.lock
# run-tests: covers richos/app/crates/richos-core/examples/native_roundtrip.rs richos/app/crates/richos-core/examples/machinery_roundtrip.rs richos/app/crates/richos-core/examples/live_events_roundtrip.rs richos/app/crates/richos-voice/examples/voice_loop.rs richos/app/crates/richos-core/examples/setup_demo.rs richos/app/crates/richos-core/examples/onboarding_native_roundtrip.rs richos/app/crates/richos-core/examples/question_receipt_e2e.rs richos/app/crates/richos-core/examples/work_lease_roundtrip.rs richos/app/crates/richos-core/examples/work_roundtrip.rs richos/app/crates/richos-core/examples/conversation_wait_behind_another.rs richos/app/src-tauri/examples/stop_payload.rs
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -p richos-core -p richos-voice \
  --example native_roundtrip \
  --example machinery_roundtrip \
  --example live_events_roundtrip \
  --example setup_demo \
  --example onboarding_native_roundtrip \
  --example question_receipt_e2e \
  --example work_lease_roundtrip \
  --example work_roundtrip \
  --example conversation_wait_behind_another \
  --example voice_loop

# Compile the desktop Stop payload probe against the same scoped control API.
# Cargo uses its test harness; the interactive example main is not executed.
cargo test --manifest-path src-tauri/Cargo.toml --example stop_payload
