#!/usr/bin/env bash
# Settings folder discovery, batch ownership and native Service input boundaries.
# run-tests: no-host-screen: Rust owner tests and a uniquely named pasteboard, no GUI, system clipboard or microphone
# run-tests: inputs richos/app/scripts/cleaner-host.test.sh richos/app/src-tauri/src/cleaner_host.rs richos/app/src-tauri/src/cleaner_host/batch.rs richos/app/src-tauri/src/cleaner_service.rs richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Cargo.lock richos/app/crates/richos-core/src/cleaner/ richos/app/src-tauri/src/
# run-tests: covers richos/app/src-tauri/src/cleaner_host.rs richos/app/src-tauri/src/cleaner_host/batch.rs richos/app/src-tauri/src/cleaner_service.rs
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$here/lib/cargo-cache-env.sh"
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" --bin richos-tauri cleaner_host::
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" --bin richos-tauri cleaner_service::tests
