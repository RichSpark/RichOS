//! THE FULL CHECKS, LATE AND IN THE BACKGROUND — the CEO's "completely eliminate the need for that
//! check" (2026-11-14) and Sage's plan of the same day, rule 3. A launch runs no check of the
//! updated bundle or the engine runtime: both were checked in full when they were written, and they
//! land whole (staged, checked, one rename or exchange). What is left is damage after the fact. So
//! once per launch, well after the app is usable (it is started only after the page has taken over
//! the splash, then waits [`LATE_CHECK_DELAY`]) and at background priority (the thread's processor
//! and disk use both yield to anything else), every byte of both is read once more. Each ending is
//! defined where it lives: the bundle's next launch checks in full and refuses
//! (`richos_user_update::recheck_preferred`); the engine's next check reads every file and refuses,
//! so set-up offers the reinstall (`runtime::recheck_runtime_bytes`). Neither stops the running app.
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long after the page took over the full checks begin: about 750 MB of reading.
pub const LATE_CHECK_DELAY: Duration = Duration::from_secs(60);

pub fn spawn_late_full_checks(engine: Option<PathBuf>) {
    let started = std::thread::Builder::new().name("richos-late-check".into()).spawn(move || {
        std::thread::sleep(LATE_CHECK_DELAY);
        #[cfg(target_os = "macos")]
        // SAFETY: a plain system call on this thread's own priority.
        if unsafe { libc::setpriority(libc::PRIO_DARWIN_THREAD, 0, libc::PRIO_DARWIN_BG) } != 0 {
            eprintln!("[richos] late check: background priority refused; running at normal priority");
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let exe = std::env::current_exe().ok();
        for line in check_once(home.as_deref(), exe.as_deref(), engine.as_deref()) {
            eprintln!("{line}");
        }
    });
    if let Err(error) = started {
        eprintln!("[richos] late check: not started ({error}); the next launch's checks still run");
    }
}

/// Both full checks, now, on this thread; the log lines they produce.
fn check_once(home: Option<&Path>, exe: Option<&Path>, engine: Option<&Path>) -> Vec<String> {
    let mut said = Vec::new();
    #[cfg(target_os = "macos")]
    if let (Some(home), Some(exe)) = (home, exe) {
        let at = std::time::Instant::now();
        match richos_user_update::recheck_preferred(home, exe) {
            Ok(richos_user_update::BundleRecheck::NotPreferred) => {}
            Ok(richos_user_update::BundleRecheck::Confirmed) => said.push(format!(
                "[richos] late check: the updated bundle's every byte matches its receipt ({} ms)",
                at.elapsed().as_millis())),
            Ok(richos_user_update::BundleRecheck::Mismatch) => said.push(
                "[richos] late check: the updated bundle's bytes NO LONGER match its receipt; the damage \
                 is recorded, so the next launch checks it in full and refuses".into()),
            Err(error) => said.push(format!("[richos] late check: the updated bundle could not be read in full: {error}")),
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (home, exe);
    if let Some(engine) = engine {
        let at = std::time::Instant::now();
        match richos_core::runtime::recheck_runtime_bytes(engine) {
            Ok(richos_core::runtime::RuntimeRecheck::Confirmed) => said.push(format!(
                "[richos] late check: the engine runtime's every byte matches its delivery ({} ms)",
                at.elapsed().as_millis())),
            Ok(richos_core::runtime::RuntimeRecheck::Changed(name)) => said.push(format!(
                "[richos] late check: the engine runtime file {name} NO LONGER matches its delivery; \
                 what was verified is forgotten, so the next engine check refuses and set-up offers \
                 the reinstall. Work already running is left alone.")),
            Err(error) => said.push(format!("[richos] late check: the engine runtime could not be read in full: {error}")),
        }
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHA-256 of the bytes `node` (`printf node | shasum -a 256`).
    const NODE: &str = "545ea538461003efdc8c81c244531b003f6f26cfccf6c0073b3239fdedf49446";

    /// A one-file runtime whose delivery names the digest of `node`, holding `bytes`.
    fn engine_with(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("richos-late-check-{}-{name}", std::process::id()));
        let runtime = dir.join("runtime");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::write(runtime.join("bin/node"), bytes).unwrap();
        std::fs::write(runtime.join("delivery.json"), serde_json::to_vec(&serde_json::json!({
            "schema": 1, "platform": "aarch64-apple-darwin", "versions": {},
            "files": {"bin/node": NODE}, "links": {}
        })).unwrap()).unwrap();
        dir
    }

    #[test]
    fn the_late_check_says_what_it_found_and_names_a_damaged_file() {
        let whole = engine_with("whole", b"node");
        let said = check_once(None, None, Some(&whole));
        assert!(said.len() == 1 && said[0].contains("every byte matches its delivery"), "{said:?}");
        let damaged = engine_with("damaged", b"n0de");
        let said = check_once(None, None, Some(&damaged));
        assert!(said.len() == 1 && said[0].contains("bin/node NO LONGER matches"), "{said:?}");
        assert!(check_once(None, None, None).is_empty(), "with no engine and no update it says nothing");
        for dir in [whole, damaged] {
            std::fs::remove_dir_all(dir).ok();
        }
    }
}
