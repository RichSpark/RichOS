//! Small shared helpers (time + ids), kept dependency-light.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch. Used only for ordering/labelling events —
/// it is NEVER the durability signal (the append-and-flush is). Per the engine's
/// freshness doctrine, identity/ordering come from the event stream, not the clock.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `HH:MM:SS.mmmZ` in UTC: the stamp the voice session's `[richos-voice]` lines carry
/// (`richos-voice`'s `controller::wall_clock_utc`), so a `[richos] reply timing:` line lines
/// up with them and with the ledger's epoch milliseconds in one `app.log`.
pub fn wall_clock_utc() -> String {
    let ms = now_millis();
    let secs = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}Z", (secs / 3600) % 24, (secs / 60) % 60, secs % 60, ms % 1000)
}

/// A fresh random id (uuid v4, hyphen-free) for threads / turns / actions.
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

/// Separate a previous incomplete append from the next record without discarding bytes.
/// The file must be opened for reading and appending. The caller flushes or syncs
/// this separator together with the new record before acknowledging it.
pub(crate) fn ensure_line_boundary(file: &mut std::fs::File) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    if file.metadata()?.len() != 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            file.write_all(b"\n")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn line_boundary_preserves_empty_complete_and_unterminated_tails() {
        let path = std::env::temp_dir().join(new_id("line-boundary"));
        for (before, expected) in [
            ("", "next\n"),
            ("{}\n", "{}\nnext\n"),
            ("{}", "{}\nnext\n"),
            ("{broken", "{broken\nnext\n"),
        ] {
            std::fs::write(&path, before).unwrap();
            let mut file = std::fs::OpenOptions::new().read(true).append(true).open(&path).unwrap();
            ensure_line_boundary(&mut file).unwrap();
            ensure_line_boundary(&mut file).unwrap();
            file.write_all(b"next\n").unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        }
        std::fs::remove_file(path).unwrap();
    }
}
