# Qdrant Edge 0.8.0, carried here with one change

This folder is the `qdrant-edge` 0.8.0 crate exactly as published on crates.io
(<https://crates.io/crates/qdrant-edge/0.8.0>, Apache-2.0, `LICENSE` beside this note, taken
from the Qdrant repository at `016542aa5deb6c66380bb137badf73d54f742bde`; that repository
ships no NOTICE file), with one change. Both Cargo workspaces (`app/Cargo.toml` and
`app/src-tauri/Cargo.toml`) point `qdrant-edge` here through `[patch.crates-io]`.

## What was changed, and why

The published crate does not compile on RichOS's Rust toolchain (1.95.0): `src/shard/optimize.rs`
uses `std::debug_assert_matches!`, which that release still treats as an unstable library
feature (`error[E0658]`). The change, in `src/shard/optimize.rs` only:

- removed the line `use std::debug_assert_matches;`
- replaced `debug_assert_matches!(proxy.wrapped_segment(), LockedSegment::Original(_), "…")`
  with `debug_assert!(matches!(proxy.wrapped_segment(), LockedSegment::Original(_)), "…")`

The check is the same debug-build assertion with the same message; release builds compile it
out either way. Nothing else in the crate differs from the published 0.8.0.

## When this folder goes away

Delete it, and the two `[patch.crates-io]` entries, once an upstream `qdrant-edge` release
builds on the Rust toolchain RichOS uses (or RichOS moves to a toolchain that builds 0.8.0
unchanged). Until then a version bump is a new copy of the published crate with this change
re-applied, never an edit to other files here.
