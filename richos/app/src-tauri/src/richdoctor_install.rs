//! **RICHDOCTOR, INSTALLED BESIDE RICHOS** (richos-hq/docs/prds/2026-10-22-richdoctor-v1.md (the private record) §3).
//!
//! This RichOS carries the signed RichDoctor app as a sealed payload
//! (`Contents/Resources/RichDoctor/RichDoctor.app.tar.gz`, with its version beside it in
//! `RichDoctor/version`), put there by `scripts/package-app.sh`. Early in every start, before
//! settings or the engine exist, [`start`] hands the install or refresh to a background thread
//! and returns at once: RichOS never waits for the quit, replacement or relaunch of RichDoctor
//! before showing its own window, and a failure here cannot stop RichOS. What the refresh does
//! is `richdoctor_core::bootstrap::refresh`, and its rules are in that file.
//!
//! A RichOS built without the payload (a plain `cargo build`, a test) finds nothing and does
//! nothing.
use std::path::Path;

/// Start the install or refresh in the background. Returns at once: whether a refresh began.
pub fn start() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some((payload, richos_app)) = richdoctor_core::bootstrap::payload_of(&exe) else { return false };
    if !payload.is_file() {
        return false;
    }
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else { return false };
    std::thread::Builder::new()
        .name("richdoctor-install".into())
        .spawn(move || {
            let version = std::fs::read_to_string(payload.with_file_name("version")).map(|v| v.trim().to_string()).unwrap_or_default();
            if version.is_empty() {
                eprintln!("[richos] richdoctor: the payload has no version; nothing installed");
                return;
            }
            let team = match richdoctor_core::bootstrap::signing_team(&richos_app) {
                Ok(team) => team,
                Err(e) => {
                    eprintln!("[richos] richdoctor: RichOS's own signature could not be read ({e}); nothing installed");
                    return;
                }
            };
            let places = richdoctor_core::bootstrap::Places::for_home(&home, Some(Path::new(&richos_app)));
            let outcome = richdoctor_core::bootstrap::refresh(&payload, &version, &team, &places);
            eprintln!("[richos] richdoctor: {outcome:?}");
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    /// A RichOS without the payload (this test binary is in no RichOS.app) installs nothing and
    /// starts no thread; the installer's own rules are tested in richdoctor-core's bootstrap.rs.
    #[test]
    fn a_richos_without_the_payload_installs_nothing() {
        assert!(!super::start());
    }
}
