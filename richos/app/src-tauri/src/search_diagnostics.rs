//! Bounded local palette timings. The writer receives numbers and fixed states only.
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 128 * 1024;
static PATH: OnceLock<PathBuf> = OnceLock::new();
static SENDER: OnceLock<mpsc::SyncSender<String>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn init(data: &Path) {
    let path = PATH.get_or_init(|| data.join(richdoctor_core::collect::SEARCH_TIMING_LOG)).clone();
    SENDER.get_or_init(|| {
        let (send, receive) = mpsc::sync_channel::<String>(64);
        let _worker = std::thread::Builder::new().name("richfinder-timings".into()).spawn(move || {
            for record in receive {
                append(&path, &record);
            }
        });
        send
    });
}

pub(crate) fn record(requested: bool, response: Option<&richos_search::SearchResponse>, elapsed: Duration, stages: &[(&str, f64)]) {
    let mode = match response {
        None => "superseded",
        Some(response) if response.effective.super_finder => "meaning",
        Some(_) if requested => "words_fallback",
        Some(_) => "words",
    };
    record_mode(mode, elapsed, stages);
}

pub(crate) fn record_unavailable(elapsed: Duration, stages: &[(&str, f64)]) {
    record_mode("unavailable", elapsed, stages);
}

fn record_mode(mode: &str, elapsed: Duration, stages: &[(&str, f64)]) {
    let Some(sender) = SENDER.get() else { return };
    let at_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis() as u64).unwrap_or(0);
    let entry = serde_json::json!({
        "at_ms": at_ms,
        "operation": NEXT.fetch_add(1, Ordering::Relaxed),
        "build": option_env!("RICHOS_SOURCE_SHA").unwrap_or(env!("CARGO_PKG_VERSION")),
        "mode": mode,
        "total_ms": elapsed.as_secs_f64() * 1_000.0,
        "stages": stages.iter().take(24).copied().collect::<Vec<_>>(),
    });
    let _sent = sender.try_send(entry.to_string());
}

fn append(path: &Path, record: &str) {
    let metadata = std::fs::symlink_metadata(path);
    if metadata.as_ref().is_ok_and(|m| !m.is_file()) { return; }
    if metadata.as_ref().is_ok_and(|m| m.len() >= MAX_BYTES)
        && std::fs::rename(path, path.with_extension("jsonl.1")).is_err()
    {
        return;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    if let Ok(mut file) = options.open(path) {
        let _written = writeln!(file, "{record}");
    }
}

/// A numeric excerpt for a user-initiated RichFinder bug report.
pub(crate) fn recent() -> Option<String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(PATH.get()?).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() { return None; }
    let end = metadata.len();
    file.seek(SeekFrom::Start(end.saturating_sub(8192))).ok()?;
    let mut text = String::new();
    file.take(8192).read_to_string(&mut text).ok()?;
    let lines: Vec<_> = richdoctor_core::collect::search_timing_lines(&text).into_iter().rev()
        .take(3).map(|(_, line)| line).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_log_rotates_and_keeps_the_new_record() {
        let folder = std::env::temp_dir().join(format!(
            "rf-timing-{}",
            richdoctor_core::util::base64url(&richdoctor_core::util::random_bytes(8).unwrap())
        ));
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join(richdoctor_core::collect::SEARCH_TIMING_LOG);
        let older = "x".repeat(MAX_BYTES as usize);
        append(&path, &older);
        append(&path, r#"{"total_ms":14.5}"#);
        let previous = std::fs::read_to_string(path.with_extension("jsonl.1")).unwrap();
        let current = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(previous.trim_end(), older);
        assert_eq!(current.trim_end(), r#"{"total_ms":14.5}"#);
    }
}
