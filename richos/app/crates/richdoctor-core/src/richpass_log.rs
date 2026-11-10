//! Bounded RichPass metadata only, collected with Configuration through existing review controls.
use std::{io::Write, path::Path};
const CAP: u64 = 64 * 1024;
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Finite outcomes only. The type deliberately accepts no path, content or arbitrary error.
#[derive(Clone, Copy)]
pub enum Outcome {
    Idle,
    Verified,
    Paused,
    Waiting,
    Blocked,
    SourceUnavailable,
    Unavailable,
}
pub fn record(
    root: &Path,
    at: u64,
    outcome: Outcome,
    problem: Option<richos_core::richpass::Issue>,
    elapsed_ms: u64,
    event_watch: bool,
    build: &'static str,
) {
    let phase = match outcome {
        Outcome::Idle => "idle",
        Outcome::Verified => "verified",
        Outcome::Paused => "paused",
        Outcome::Waiting => "waiting",
        Outcome::Blocked => "blocked",
        Outcome::SourceUnavailable => "cache_unavailable",
        Outcome::Unavailable => "unavailable",
    };
    append(root,at,&format!("richpass check phase={} elapsed_ms={} event_watch={} problem={:?} build={} collector_version={}",phase,elapsed_ms,event_watch,problem,build,env!("CARGO_PKG_VERSION")));
}
#[derive(Clone, Copy)]
pub enum FailurePoint {
    FileAction,
    MonitorStart,
}
/// Manual failures are recorded at the failed action, without paths or an invented timing.
pub fn failure(
    root: &Path,
    at: u64,
    point: FailurePoint,
    problem: Option<richos_core::richpass::Issue>,
    build: &'static str,
) {
    let point = match point {
        FailurePoint::FileAction => "file_action",
        FailurePoint::MonitorStart => "monitor_start",
    };
    append(root,at,&format!("richpass {point} outcome=unavailable problem={problem:?} elapsed_ms=unmeasured build={build}"));
}
fn append(root: &Path, at: u64, message: &str) {
    let _guard = WRITER.lock().unwrap_or_else(|p| p.into_inner());
    let path = root.join("richpass.log");
    if let Ok(m) = std::fs::symlink_metadata(&path) {
        if !m.is_file() {
            return;
        }
        if m.len() >= CAP && std::fs::rename(&path, root.join("richpass.log.1")).is_err() {
            return;
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    if let Ok(mut file) = options.open(path) {
        drop(writeln!(
            file,
            "{} {message}",
            crate::util::utc_stamp(at.saturating_mul(1000))
        ));
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    #[test]
    fn refuses_a_redirected_log_without_appending_to_its_target() {
        let root = std::env::temp_dir().join(format!(
            "richpass-log-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("unrelated");
        std::fs::write(&target, b"unrelated bytes").unwrap();
        std::os::unix::fs::symlink(&target, root.join("richpass.log")).unwrap();
        record(
            &root,
            100,
            Outcome::Blocked,
            None,
            7,
            false,
            "fixture-1.3.0",
        );
        assert_eq!(std::fs::read(target).unwrap(), b"unrelated bytes");
        std::fs::remove_dir_all(root).unwrap();
    }
}
