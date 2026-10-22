//! **RICHDOCTOR V1, EVERYTHING THAT IS NOT A WINDOW.** The spec is richos-hq
//! `docs/prds/2026-10-22-richdoctor-v1.md` (version 4); section numbers below are its.
//!
//! RichDoctor is a separate macOS app (§3) that lets a non-technical person report a broken
//! RichOS. Its normal engine is the person's own installed, signed-in Claude Code (§4), run as
//! one restricted multi-turn process ([`adapter`]) whose only capabilities are two read-only
//! support tools served by RichDoctor itself ([`mcp`]). The same collectors ([`collect`]) feed
//! the no-AI path. Evidence is filtered and scrubbed before anything sees it ([`scrub`]), the
//! report is the exact outgoing text ([`report`]), local work is durable ([`store`]) and
//! delivery is an honest state machine over the reporting relay's contract ([`delivery`]).
//!
//! Nothing here opens a window or a network connection. The app crate (`crates/richdoctor`)
//! owns the webview, the menu bar item, the login item and the HTTPS transport.
pub mod adapter;
pub mod collect;
pub mod delivery;
pub mod evidence;
pub mod install;
pub mod mcp;
pub mod report;
pub mod scrub;
pub mod store;
pub mod util;
pub mod when;

/// RichDoctor's own version, the one in the report footer.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The footer's short version: "1.0" for 1.0.0, "1.0.2" for 1.0.2 (the relay's footer rule is
/// `major.minor[.patch]`).
pub fn footer_version() -> String {
    let mut parts: Vec<&str> = VERSION.split(['.', '-', '+']).take(3).collect();
    if parts.len() == 3 && parts[2] == "0" {
        parts.pop();
    }
    parts.join(".")
}

/// RichDoctor's bundle identifier and data folder name.
pub const IDENTIFIER: &str = "com.richos.richdoctor";

/// RichOS's bundle identifier, the only app whose evidence is collected.
pub const RICHOS_IDENTIFIER: &str = "com.richos.app";

#[cfg(test)]
mod tests {
    #[test]
    fn the_footer_version_drops_a_zero_patch_only() {
        assert_eq!(super::footer_version(), "1.0");
    }
}
