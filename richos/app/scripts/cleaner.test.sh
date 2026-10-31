#!/usr/bin/env bash
# Filing/recovery uses real filesystem identities in a disposable directory. Metadata
# unit checks exercise the shipping writers; full-media qualification is separate.
# run-tests: no-host-screen: a native file helper and Rust tests; no window, key tap or microphone
# run-tests: inputs richos/app/scripts/cleaner.test.sh richos/app/native/cleaner.swift richos/app/src-tauri/src/cleaner_native.rs richos/app/src-tauri/src/cleaner_metadata.rs richos/app/src-tauri/examples/cleaner_probe.rs richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Cargo.lock richos/app/crates/richos-core/src/cleaner/
# run-tests: covers richos/app/native/cleaner.swift richos/app/src-tauri/src/cleaner_native.rs richos/app/src-tauri/src/cleaner_metadata.rs richos/app/src-tauri/examples/cleaner_probe.rs
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$here/lib/cargo-cache-env.sh"
[ "$(uname)" = Darwin ] || { echo 'RichCleaner native qualification requires macOS' >&2; exit 1; }
scratch="$(mktemp -d "${TMPDIR:?}richcleaner-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/resources" "$scratch/fixtures"
xcrun swiftc -O -module-cache-path "$scratch/modules" "$here/../native/cleaner.swift" -o "$scratch/resources/native"
printf 'ordinary original, kept as one file\n' > "$scratch/fixtures/context.txt"
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" --example cleaner_probe
cargo run --quiet --manifest-path "$here/../src-tauri/Cargo.toml" --example cleaner_probe -- "$scratch/resources" "$scratch/fixtures" "$scratch/report"
