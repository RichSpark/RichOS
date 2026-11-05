//! `dictation.log`: model, length, latency and the event-tap numbers, never the words (plan
//! section 6). Every line also goes to stderr behind `[richos-dictation]`, which is the app's log
//! when the tool is the app's child.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::os::unix::fs::OpenOptionsExt;

pub const MAX_BYTES: u64 = 1024 * 1024;
static WRITER: Mutex<()> = Mutex::new(());

pub const FILE_NAME: &str = "dictation.log";

static PATH: OnceLock<PathBuf> = OnceLock::new();

/// Log into `<data dir>/dictation.log` from now on.
pub fn init(data_dir: &Path) {
    PATH.set(data_dir.join(FILE_NAME)).ok();
}

pub fn line(message: &str) {
    eprintln!("[richos-dictation] {message}");
    if let Some(path) = PATH.get() {
        let stamp = crate::startup_alert::utc_stamp(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        );
        append(path, &stamp, message);
    }
}

fn append(path: &Path, stamp: &str, message: &str) {
    let _writer = WRITER.lock().unwrap_or_else(|p| p.into_inner());
    let metadata = std::fs::symlink_metadata(path);
    if metadata.as_ref().is_ok_and(|m| !m.is_file()) { return; }
    if metadata.as_ref().is_ok_and(|m| m.len() >= MAX_BYTES) {
        let rotated = path.with_file_name("dictation.log.1");
        if std::fs::rename(path, rotated).is_err() { return; }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path) {
        let end = message.char_indices().nth(2048).map_or(message.len(), |(i, _)| i);
        let suffix = if end < message.len() { " [truncated]" } else { "" };
        writeln!(f, "{stamp} {}{suffix}", &message[..end]).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_retains_the_previous_bounded_log_and_restricts_new_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("dictation-log-rotation-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize]).unwrap();
        append(&path, "stamp", "current");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "stamp current\n");
        assert_eq!(std::fs::metadata(dir.join("dictation.log.1")).unwrap().len(), MAX_BYTES);
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::write(&path, vec![b'y'; MAX_BYTES as usize]).unwrap();
        append(&path, "stamp", "next");
        assert_eq!(std::fs::read(dir.join("dictation.log.1")).unwrap()[0], b'y');
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// INVARIANT: one line per call, stamped, appended: an earlier line is never overwritten.
    #[test]
    fn lines_are_appended_with_their_stamp() {
        let path = std::env::temp_dir().join(format!("richos-dictation-log-{}", std::process::id()));
        std::fs::remove_file(&path).ok();
        append(&path, "2026-10-08T10:00:00.000Z", "dictation: model small.en, 3.10 s of audio, written in 812 ms, pasted");
        append(&path, "2026-10-08T10:00:05.000Z", "key tap: longest callback 41 us in the last hour; 0 disable(s) so far");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.starts_with("2026-10-08T10:00:00.000Z dictation: model small.en"));
        std::fs::remove_file(&path).unwrap();
    }
}
