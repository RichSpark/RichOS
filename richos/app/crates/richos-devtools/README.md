# Offline developer tools

This standalone crate owns Cargo log summaries, caption comparison, conversation
ledger projections, dictionary generation and the UI test timeout source check.
It has no dependency on the app, audio workspace or installed engine hooks.
Transcript episode accounting and shell guards remain unchanged.

The existing Python commands and Node import stay available as small adapters.
On first use they build this crate with Cargo, using the existing machine storage
settings and a target directory private to this workspace. Later invocations let
Cargo check freshness before running the cached executable. A Rust toolchain is
required only when using these developer tools. Normal Claude Code terminal
sessions do not build or invoke this crate.

Build or test independently:

```sh
cargo test --locked --manifest-path richos/app/crates/richos-devtools/Cargo.toml
python3 -B richos/app/scripts/devtools.test.py
```

`RICHOS_DEVTOOLS_BIN` can name an absolute prebuilt executable for isolated test
copies. The caller is responsible for its source identity. Verification entry
points build the source under test rather than trusting an inherited override.
The RPC interface takes JSON on stdin and returns a JSON result or error. It
never evaluates input as code. Dictionary generation still requires the pinned
VarCon bytes, refuses conflicting data and emits the existing licensed header.

Golden fixtures contain synthetic inputs and the original tools' outputs. They
check observable behavior independently of the new implementation. The original
Python and JavaScript contract tests remain in place.
