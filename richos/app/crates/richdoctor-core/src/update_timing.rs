//! Bounded update phase timings. No paths, URLs or vendor error text enter the log.
use std::path::Path;

pub(crate) const LOG: &str = "update-timing.log";

pub fn record(
    root: &Path,
    at_ms: u64,
    attempt: u64,
    phase: &'static str,
    outcome: &'static str,
    elapsed_ms: u64,
    build: &'static str,
) {
    crate::richpass_log::append_bounded(
        root,
        LOG,
        at_ms,
        &format!(
            "update build={build} pid={} attempt={attempt} phase={phase} outcome={outcome} elapsed_ms={elapsed_ms}",
            std::process::id()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_timing_keeps_one_old_log_and_writes_the_new_record() {
        let stamp = crate::util::hex(&crate::util::random_bytes(8).unwrap());
        let root = std::env::temp_dir().join(format!("richos-update-timing-{stamp}"));
        std::fs::create_dir(&root).unwrap();
        let prior = vec![b'x'; 64 * 1024];
        std::fs::write(root.join(LOG), &prior).unwrap();
        record(&root, 3000, 9, "installing", "ready", 5740, "1.3.1");
        let active = std::fs::read_to_string(root.join(LOG));
        let rotated = std::fs::read(root.join(format!("{LOG}.1")));
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(rotated.unwrap(), prior);
        assert!(active.unwrap().contains("attempt=9 phase=installing outcome=ready elapsed_ms=5740"));
    }
}
