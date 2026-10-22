//! The shared screenshot format profile against the golden fixtures both sides read
//! (`richos/mobile/service/report/fixtures/`): RichDoctor's own encoder output (`client/`,
//! written by `examples/profile_fixtures.rs`) and the relay's (`relay/`, when present). Every
//! `accept-*` passes the profile unchanged; every `reject-*` is refused, and finalizing it gives
//! an image that passes. One profile, checked from both ends.
use richdoctor_core::image_profile::{check, finalize};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mobile/service/report/fixtures")
}

#[test]
fn every_shared_fixture_is_accepted_or_refused_as_named() {
    let mut seen = 0;
    for side in ["client", "relay"] {
        let Ok(entries) = std::fs::read_dir(fixtures().join(side)) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".jpeg")) {
                continue;
            }
            let bytes = std::fs::read(e.path()).unwrap();
            if name.starts_with("accept-") {
                assert!(check(&bytes).is_ok(), "{side}/{name}: {:?}", check(&bytes));
                assert_eq!(finalize(&bytes).unwrap().0.len() > 0, true);
                seen += 1;
            } else if name.starts_with("reject-") {
                assert!(check(&bytes).is_err(), "{side}/{name} passed the profile");
                seen += 1;
            }
        }
    }
    assert!(seen >= 4, "RichDoctor's own fixtures are committed ({seen} found)");
}
