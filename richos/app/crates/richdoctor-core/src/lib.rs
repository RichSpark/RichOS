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
pub mod bootstrap;
pub mod collect;
pub mod delivery;
pub mod email;
pub mod evidence;
pub mod image_profile;
pub mod install;
pub mod mcp;
pub mod report;
pub mod scrub;
pub mod store;
pub mod util;
pub mod when;

/// RichDoctor's own version, everywhere it states it (the report's Versions section, its footer,
/// the delivery, the MCP server). package-app.sh sets `RICHDOCTOR_VERSION` to RichOS's version
/// for the build, the same value it stamps into the bundle's Info.plist, so the bundle and what
/// RichDoctor says are one source; an unpackaged build falls back to this crate's own version.
pub const VERSION: &str = resolve_version(option_env!("RICHDOCTOR_VERSION"), env!("CARGO_PKG_VERSION"));

/// The packaged version when there is one, else the crate's own.
pub const fn resolve_version(packaged: Option<&'static str>, own: &'static str) -> &'static str {
    match packaged {
        Some(v) if !v.is_empty() => v,
        _ => own,
    }
}

/// The footer's short version: "1.0" for 1.0.0, "1.0.2" for 1.0.2 (the relay's footer rule is
/// `major.minor[.patch]`).
pub fn footer_version() -> String {
    footer_version_of(VERSION)
}

/// `footer_version` for a given version string.
pub fn footer_version_of(version: &str) -> String {
    let mut parts: Vec<&str> = version.split(['.', '-', '+']).take(3).collect();
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
        assert_eq!(super::footer_version_of("1.0.0"), "1.0");
        assert_eq!(super::footer_version_of("1.0.2"), "1.0.2");
    }

    #[test]
    fn the_packaged_version_is_the_one_stated_in_the_versions_section_and_the_footer() {
        let v = super::resolve_version(Some("1.3.0-nightly.86"), "1.0.0");
        assert_eq!(v, "1.3.0-nightly.86");
        assert_eq!(super::resolve_version(None, "1.0.0"), "1.0.0");
        assert_eq!(super::resolve_version(Some(""), "1.0.0"), "1.0.0");
        assert!(crate::collect::versions_first_line(v).starts_with("RichDoctor 1.3.0-nightly.86"));
        // The footer keeps the relay's `major.minor[.patch]` shape (the deployed relay rejects any
        // other), so a pre-release suffix is not in it; the Versions section carries the whole.
        assert_eq!(crate::report::footer_for(v, "RD-7K3M-Q9VX"), "Sent with RichDoctor 1.3 · Report ID RD-7K3M-Q9VX");
        assert_eq!(crate::report::footer_for("1.3.2", "RD-7K3M-Q9VX"), "Sent with RichDoctor 1.3.2 · Report ID RD-7K3M-Q9VX");
    }
}

pub mod richpass_log;
