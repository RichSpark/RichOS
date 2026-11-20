//! One provider read budget for an account, shared by desktop and terminal processes.
//! The terminal watcher uses the same file and flock protocol in quota_usage_store.py.
use super::{atomic_write, is_weekly, same_reset, ReadError, Window,
    FAST_POINTS_PER_CHECK, FAST_REFRESH_INTERVAL_MS, REFRESH_INTERVAL_MS, SPEED_MIN_GAP_MS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

pub(super) const IDLE_INTERVAL_MS: u64 = 30 * 60_000;
const FAST_ABOVE_PERCENT: f64 = 80.;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Answer {
    pub checked_at: u64,
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Refusal {
    pub at: u64,
    pub error: ReadError,
    #[serde(default)]
    pub explicit_refusal: bool,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub retry_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Saved {
    pub requested_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub fast_until: Option<u64>,
    pub answer: Option<Answer>,
    pub refusal: Option<Refusal>,
}

impl Saved {
    pub fn interval(&self, now: u64, idle_interval: u64) -> u64 {
        if idle_interval <= REFRESH_INTERVAL_MS && self.refusal.is_none()
            && self.fast_until.is_some_and(|until| now < until) {
            FAST_REFRESH_INTERVAL_MS
        } else {
            idle_interval.max(REFRESH_INTERVAL_MS)
        }
    }

    pub fn next_request_at(&self, now: u64, idle_interval: u64) -> u64 {
        let spacing = self.interval(now, idle_interval);
        // The control session may initialize between claiming a slot and asking.
        // Counting from its reply guarantees spacing between actual requests.
        let by_interval = self.requested_at.max(self.finished_at).map_or(0, |at| at.saturating_add(spacing));
        let after_refusal = self.refusal.as_ref().map_or(0, |failure| {
            failure.at.saturating_add(REFRESH_INTERVAL_MS).max(failure.retry_at.unwrap_or(0))
        });
        by_interval.max(after_refusal)
    }
}

fn fast_until(answer: &Answer, previous: Option<&Answer>) -> Option<u64> {
    let previous = previous?;
    let at = answer.checked_at;
    let gap = at.saturating_sub(previous.checked_at);
    if gap < SPEED_MIN_GAP_MS { return None; }
    answer.windows.iter().filter_map(|window| {
        let end = window.resets_at.unwrap_or(u64::MAX);
        if end <= at || window.used_percent <= FAST_ABOVE_PERCENT
            || !(window.id == "five_hour" || is_weekly(&window.id)) {
            return None;
        }
        let older = previous.windows.iter().find(|old| old.id == window.id)?;
        let same = match (older.resets_at, window.resets_at) {
            (Some(a), Some(b)) => same_reset(a, b),
            (None, None) => true,
            _ => false,
        };
        let rise = (window.used_percent - older.used_percent) * REFRESH_INTERVAL_MS as f64 / gap as f64;
        (same && rise >= FAST_POINTS_PER_CHECK).then(|| end.min(at.saturating_add(REFRESH_INTERVAL_MS)))
    }).max()
}

#[derive(Clone, Debug)]
pub(super) struct Store {
    root: PathBuf,
    key: String,
    verified: bool,
}

struct Lock {
    #[cfg(unix)]
    file: std::fs::File,
}

#[cfg(unix)]
impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // Release before close in case another thread is between fork and exec.
        let _unlocked = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

impl Store {
    pub fn new(data_dir: &Path, key: String) -> Self {
        Self { root: data_dir.join("claude-usage"), key, verified: true }
    }

    pub fn for_account(data_dir: &Path, bin: &Path, folder: Option<&Path>) -> Self {
        Self::from_identity(data_dir, folder_key(folder), crate::provider_auth::quota_account_key(bin, folder))
    }

    fn from_identity(data_dir: &Path, folder: String, identity: Option<String>) -> Self {
        let binding = data_dir.join("claude-usage").join(&folder).with_extension("identity");
        let previous = super::gate::read_json::<String>(&binding).ok().filter(|key| {
            key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        });
        if let Some(key) = identity {
            if previous.as_ref() != Some(&key) {
                let _saved = atomic_write(&binding, &key);
            }
            return Self::new(data_dir, key);
        }
        // A transient auth-status failure can't select a second budget. Retain
        // the last bound reading and leave requests to a verified reader.
        Self { root: data_dir.join("claude-usage"), key: previous.unwrap_or(folder), verified: false }
    }

    pub fn verified(&self) -> bool {
        self.verified
    }

    #[cfg(test)]
    pub fn fixture(mut self) -> Self {
        self.verified = true;
        self
    }

    fn record(&self) -> PathBuf {
        self.root.join(&self.key).with_extension("json")
    }

    pub fn saved(&self) -> Saved {
        super::gate::read_json(&self.record()).unwrap_or_default()
    }

    /// Publish the reservation before the request. A process killed anywhere after this
    /// point still used its slot. Readers can use the last answer throughout the request.
    pub fn claim(&self, now: u64, interval_ms: u64, source: &str) -> io::Result<bool> {
        if !self.verified { return Ok(false); }
        if now < self.saved().next_request_at(now, interval_ms) { return Ok(false); }
        let Some(_lock) = self.lock(false)? else { return Ok(false) };
        let mut saved = self.saved();
        if now < saved.next_request_at(now, interval_ms) { return Ok(false); }
        let cadence = saved.interval(now, interval_ms);
        saved.requested_at = Some(now);
        atomic_write(&self.record(), &saved)?;
        eprintln!("[richos] quota request: account={} at_ms={now} source={source} interval_ms={cadence}", self.key);
        Ok(true)
    }

    pub fn finish(
        &self, request_at: u64, checked_at: u64, result: Result<Option<Vec<Window>>, Refusal>,
    ) -> io::Result<()> {
        let Some(_lock) = self.lock(true)? else { return Ok(()) };
        let mut saved = self.saved();
        if saved.requested_at != Some(request_at) { return Ok(()); }
        saved.finished_at = Some(checked_at.max(request_at));
        match result {
            Ok(windows) => {
                if let Some(windows) = windows {
                    let answer = Answer { checked_at, windows };
                    saved.fast_until = fast_until(&answer, saved.answer.as_ref());
                    saved.answer = Some(answer);
                }
                saved.refusal = None;
            }
            Err(failure) => saved.refusal = Some(failure),
        }
        atomic_write(&self.record(), &saved)
    }

    pub fn clear_answer(&self, request_at: u64) -> io::Result<()> {
        let Some(_lock) = self.lock(true)? else { return Ok(()) };
        let mut saved = self.saved();
        if saved.requested_at == Some(request_at) {
            saved.answer = None;
            saved.fast_until = None;
            atomic_write(&self.record(), &saved)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn backdate_request(&self, now: u64) {
        let mut saved = self.saved();
        let elapsed = now.saturating_sub(IDLE_INTERVAL_MS);
        saved.requested_at = saved.requested_at.map(|_| elapsed);
        saved.finished_at = saved.finished_at.map(|_| elapsed);
        if let Some(refusal) = &mut saved.refusal {
            refusal.at = elapsed;
        }
        atomic_write(&self.record(), &saved).unwrap();
    }

    #[cfg(unix)]
    fn lock(&self, wait: bool) -> io::Result<Option<Lock>> {
        use std::{fs::OpenOptions, os::{fd::AsRawFd, unix::fs::OpenOptionsExt}};
        std::fs::create_dir_all(&self.root)?;
        let file = OpenOptions::new().create(true).truncate(false).write(true).mode(0o600)
            .open(self.root.join(&self.key).with_extension("lock"))?;
        // lock-busy: the caller uses the saved reading and checks for a free slot on its next poll.
        let flags = libc::LOCK_EX | if wait { 0 } else { libc::LOCK_NB };
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), flags) } == 0 { return Ok(Some(Lock { file })); }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::WouldBlock if !wait => return Ok(None),
                io::ErrorKind::Interrupted => continue,
                _ => return Err(error),
            }
        }
    }

    #[cfg(not(unix))]
    fn lock(&self, _wait: bool) -> io::Result<Option<Lock>> {
        Err(io::Error::other("The shared quota reader requires file locks."))
    }
}

fn folder_key(folder: Option<&Path>) -> String {
    let configured = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|s| !s.is_empty());
    let folder = folder.map(Path::to_path_buf).or_else(|| configured.map(PathBuf::from))
        .unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".claude"));
    let absolute = std::path::absolute(&folder).unwrap_or(folder);
    let folder = absolute.canonicalize().unwrap_or(absolute);
    format!("{:x}", Sha256::digest(format!("folder\0{}", folder.to_string_lossy())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::Scratch;

    #[test]
    fn two_readers_share_the_reservation_and_keep_the_answer_on_a_refusal() {
        let dir = Scratch::new();
        let app = Store::new(dir.path(), "account".into());
        let terminal = Store::new(dir.path(), "account".into());
        let other = Store::new(dir.path(), "other".into());
        let start = 1_000;
        let next = start + REFRESH_INTERVAL_MS + 100;
        assert!(app.claim(start, REFRESH_INTERVAL_MS, "fixture").unwrap());
        assert!(!terminal.claim(start + REFRESH_INTERVAL_MS - 1, 0, "fixture").unwrap());
        assert!(other.claim(start, REFRESH_INTERVAL_MS, "fixture").unwrap());
        app.finish(start, start + 100, Ok(Some(vec![]))).unwrap();
        assert!(!terminal.claim(next - 1, 0, "fixture").unwrap());
        assert!(terminal.claim(next, 0, "fixture").unwrap());
        let retry = next + 2 * REFRESH_INTERVAL_MS;
        terminal.finish(next, next + 10, Err(Refusal {
            at: next + 10, error: ReadError::Failed, explicit_refusal: true, retry_at: Some(retry),
            headers: BTreeMap::from([("retry-after".into(), "600".into())]),
        })).unwrap();
        let saved = app.saved();
        assert_eq!(saved.answer.unwrap().checked_at, start + 100);
        assert_eq!(saved.refusal.unwrap().retry_at, Some(retry));
        assert!(!app.claim(retry - 1, REFRESH_INTERVAL_MS, "fixture").unwrap());
        assert!(app.claim(retry, REFRESH_INTERVAL_MS, "fixture").unwrap());
    }

    #[test]
    fn an_idle_reader_defers_while_a_move_can_use_the_normal_slot() {
        let dir = Scratch::new();
        let account = Store::new(dir.path(), "account".into());
        let start = 1_000;
        assert!(account.claim(start, IDLE_INTERVAL_MS, "fixture").unwrap());
        assert!(!account.claim(start + IDLE_INTERVAL_MS - 1, IDLE_INTERVAL_MS, "fixture").unwrap());
        assert!(account.claim(start + REFRESH_INTERVAL_MS, REFRESH_INTERVAL_MS, "fixture").unwrap());
    }

    #[test]
    fn an_unverified_reader_can_reuse_the_bound_answer_without_taking_a_slot() {
        let dir = Scratch::new();
        let folder = folder_key(Some(dir.path()));
        let before = Store::from_identity(dir.path(), folder.clone(), None);
        assert!(!before.claim(1_000, 0, "fixture").unwrap());
        let verified = Store::from_identity(dir.path(), folder.clone(), Some("a".repeat(64)));
        assert!(verified.claim(1_000, 0, "fixture").unwrap());
        verified.finish(1_000, 1_001, Ok(Some(vec![]))).unwrap();
        let after = Store::from_identity(dir.path(), folder, None);
        assert_eq!(after.saved().answer.unwrap().checked_at, 1_001);
        let then = 1_001 + REFRESH_INTERVAL_MS;
        assert!(!after.claim(then, 0, "fixture").unwrap());
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/scripts/lib");
        let program = r#"
import sys
sys.path.insert(0, sys.argv[1])
from quota_usage_store import Store
store = Store.for_account(sys.argv[2], None, sys.argv[2])
assert store.saved()["answer"]["checkedAt"] == 1001
assert not store.claim(301001)
"#;
        let status = std::process::Command::new("python3").args(["-B", "-c", program])
            .arg(library).arg(dir.path()).status().unwrap();
        assert!(status.success());
        assert!(verified.claim(then, 0, "fixture").unwrap());
    }

    #[test]
    fn one_minute_requires_both_high_usage_and_a_fast_rise_in_the_same_window() {
        let dir = Scratch::new();
        let app = Store::new(dir.path(), "account".into());
        let terminal = Store::new(dir.path(), "account".into());
        let mut at = 1_000;
        for (used, expected_next) in [
            (55., REFRESH_INTERVAL_MS),
            (78., REFRESH_INTERVAL_MS), // Fast, below 80%.
            (81., REFRESH_INTERVAL_MS), // Above 80%, normal speed.
            (90., FAST_REFRESH_INTERVAL_MS),
            (92., FAST_REFRESH_INTERVAL_MS),
            (92., REFRESH_INTERVAL_MS), // Flat after a fast rise.
        ] {
            assert!(app.claim(at, REFRESH_INTERVAL_MS, "fixture").unwrap());
            let window = Window { id: "model:Fable".into(), label: "Fable".into(),
                used_percent: used, resets_at: Some(10_000_000), duration_ms: 604_800_000 };
            app.finish(at, at, Ok(Some(vec![window]))).unwrap();
            assert_eq!(terminal.saved().interval(at, 0), expected_next);
            assert!(!terminal.claim(at + expected_next - 1, 0, "fixture").unwrap());
            at += expected_next;
        }
    }

    #[test]
    fn a_refusal_uses_the_normal_interval_even_during_fast_usage() {
        let dir = Scratch::new();
        let store = Store::new(dir.path(), "account".into());
        let start = 1_000;
        let windows = |used| vec![Window { id: "seven_day".into(), label: "Weekly".into(),
            used_percent: used, resets_at: Some(10_000_000), duration_ms: 604_800_000 }];
        assert!(store.claim(start, 0, "fixture").unwrap());
        store.finish(start, start, Ok(Some(windows(80.)))).unwrap();
        let high = start + REFRESH_INTERVAL_MS;
        assert!(store.claim(high, 0, "fixture").unwrap());
        store.finish(high, high, Ok(Some(windows(90.)))).unwrap();
        let refused = high + FAST_REFRESH_INTERVAL_MS;
        assert!(store.claim(refused, 0, "fixture").unwrap());
        store.finish(refused, refused, Err(Refusal { at: refused, error: ReadError::NoReading,
            explicit_refusal: false, headers: BTreeMap::new(), retry_at: None })).unwrap();
        assert_eq!(store.saved().answer.unwrap().windows[0].used_percent, 90.);
        assert!(!store.claim(refused + REFRESH_INTERVAL_MS - 1, 0, "fixture").unwrap());
        assert!(store.claim(refused + REFRESH_INTERVAL_MS, 0, "fixture").unwrap());
    }

    #[test]
    fn the_terminal_and_app_reserve_the_same_file_across_processes() {
        let dir = Scratch::new();
        let store = Store::new(dir.path(), "account".into());
        let start = 1_000;
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/scripts/lib");
        assert!(store.claim(start, REFRESH_INTERVAL_MS, "fixture").unwrap());
        store.finish(start, start + 1, Ok(Some(vec![]))).unwrap();
        let program = r#"
import sys
sys.path.insert(0, sys.argv[1])
from quota_usage_store import Store
store = Store(sys.argv[2], "account")
assert store.saved()["answer"]["checkedAt"] == 1001
assert not store.claim(301000)
assert store.claim(301001)
store.finish(301001, refusal={"at": 301002, "error": "failed"})
"#;
        let status = std::process::Command::new("python3").args(["-B", "-c", program])
            .arg(library).arg(dir.path()).status().unwrap();
        assert!(status.success());
        let saved = store.saved();
        assert_eq!(saved.refusal.unwrap().error, ReadError::Failed);
        assert_eq!(saved.answer.unwrap().checked_at, start + 1);
        assert!(!store.claim(start + 2 * REFRESH_INTERVAL_MS - 1, 0, "fixture").unwrap());
    }
}
