//! **Dictation in any app: the tool's macOS edges** (`richos-hq/docs/plans/2026-10-08-dictation-anywhere.md`,
//! revision 2, section 1). Each file is thin and named for one platform API; every decision is in
//! `richos_voice::dictation`, where it is unit-tested.
//!
//! - `tool.rs`: the `--richos-dictation` entry and its own Tauri builder.
//! - `keytap.rs`: the active event tap.
//! - `insert.rs`: Accessibility, the pasteboard and Command-V.
//! - `ipc.rs`: the lock, the socket and the protocol, both ends.
//! - `store.rs`: `dictation.json`.
//! - `scratch.rs`: one dictation's private folder.
//! - `log.rs`: `dictation.log`, never the words.
//!
//! - `appkit.rs`: the windows as AppKit sees them, the screens, Secure Event Input (slice 3).
//! - `bar.rs`: the bar, the words' flight and the menu, as the tool's three windows (slice 3).
//! - `menubar.rs`: RichOS in the menu bar (slice 3).
//!
//! - `login.rs`: the LaunchAgent, its registration and its refusals; the tool's own start of the
//!   app (slice 5).
//!
//! Slice 1 built the tool and the engine with no new screens; slice 3 adds the bar, the flight
//! window and the menu bar item; slice 5 the login start and every way back into the app.

pub mod appkit;
pub mod bar;
pub mod insert;
pub mod ipc;
pub mod keytap;
pub mod log;
pub mod live_preview;
pub mod menubar;
pub mod scratch;
pub mod store;
pub mod tool;
pub mod timings;
pub mod vocabulary;
