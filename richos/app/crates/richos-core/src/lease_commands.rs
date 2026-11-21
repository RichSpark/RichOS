//! **A PRODUCT LEASE'S RUNNING COMMANDS** — the product reap (richos-hq
//! `docs/plans/2026-09-27-product-reap-gap-design.md`, 1.3(a), C5, C6, C7, C11).
//!
//! Every product lease's leader is `provider-supervisor.py --reap-descendants`, which ends the
//! lease's tool commands with the lease (Quit, Stop, rotation, a forced-down lease, the app's
//! death, `claude`'s death). A command the app cannot see would then be ended without warning,
//! so the app has to see them: once a second the supervisor writes a small state file per
//! lease naming the process groups alive OUTSIDE the provider's own group, one per running tool
//! command (a background heartbeat, a server Rich was asked to start, a long `npm install`).
//! Language servers and MCP servers share the provider's group and are not counted.
//!
//! This module owns three things and nothing else:
//!
//! 1. **The invocation** ([`supervisor_args`]): every reap parameter on the SUPERVISOR's command
//!    line, never in the environment, so `claude`'s argv and environment stay byte-equal to what
//!    they were (C7; the engine's R11 proves the stripping).
//! 2. **The files** ([`prepare`]): one state file per lease, named by its session id, and one
//!    reap log beside them in the app's own engine state (C11: never `claude`'s stderr, whose
//!    tail is quoted to the user when a lease fails to start).
//! 3. **The reading** ([`read`], [`LiveLeases`]): what the update gate, the quit sheet, window
//!    close and rotation ask. It is read from files, without any lock the spine or the work host
//!    holds, because the spine's mutex is held for the whole of a turn.
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The directory, inside the app's engine state, holding one state file per live lease.
pub const LEASES_DIR: &str = "provider-leases";
/// The supervisors' shared reap log, beside it.
pub const REAP_LOG: &str = "provider-reap.log";
/// Past this size the log is moved aside to `provider-reap.log.1` when a lease starts, so it
/// can never grow without bound on a customer's Mac.
const REAP_LOG_LIMIT: u64 = 1 << 20;

/// One lease's state file.
pub fn state_path(engine_state: &Path, session: &str) -> PathBuf {
    engine_state.join(LEASES_DIR).join(format!("{session}.json"))
}

/// The reap log.
pub fn log_path(engine_state: &Path) -> PathBuf {
    engine_state.join(REAP_LOG)
}

/// **The product invocation, byte-exact** (design 1.3(a)): everything the supervisor is given
/// before `claude`'s own path. The caller runs it with the delivered Python and appends the
/// `claude` path and its arguments, unchanged.
pub fn supervisor_args(engine: &Path, state: &Path, log: &Path) -> Vec<OsString> {
    let with_value = |name: &str, path: &Path| {
        let mut option = OsString::from(name);
        option.push(path.as_os_str());
        option
    };
    vec![
        engine.join("scripts/provider-supervisor.py").into_os_string(),
        OsString::from("--reap-descendants"),
        OsString::from("--reap-grace=0"),
        with_value("--reap-state=", state),
        with_value("--reap-log=", log),
    ]
}

/// Make the lease's files ready: the directory, a bounded log, and no stale state file from a
/// process that has gone. Returns `(state, log)`.
pub fn prepare(engine_state: &Path, session: &str) -> std::io::Result<(PathBuf, PathBuf)> {
    let dir = engine_state.join(LEASES_DIR);
    std::fs::create_dir_all(&dir)?;
    let log = log_path(engine_state);
    if std::fs::metadata(&log).map(|m| m.len() > REAP_LOG_LIMIT).unwrap_or(false) {
        std::fs::rename(&log, engine_state.join(format!("{REAP_LOG}.1"))).ok();
    }
    // Garbage left by an earlier run of this app (§54): a state file whose supervisor answered
    // to another process. A live lease of THIS process is never touched.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let me = std::process::id();
        for entry in entries.flatten() {
            let path = entry.path();
            let owner = std::fs::read_to_string(&path).ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|value| value.get("owner").and_then(Value::as_u64));
            if owner.is_some_and(|owner| owner != me as u64) {
                std::fs::remove_file(&path).ok();
            }
        }
    }
    Ok((state_path(engine_state, session), log))
}

/// What one lease's state file says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandReading {
    /// No command is running outside the provider's group.
    Clear,
    /// This many commands (process groups outside the provider's) are running.
    Running(usize),
    /// The file is missing or could not be read. Never read as a zero.
    Unreadable,
}

/// Read one state file.
pub fn read(state: &Path) -> CommandReading {
    let Some(value) = std::fs::read_to_string(state).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok()) else {
        return CommandReading::Unreadable;
    };
    match value.get("outside_provider_groups").and_then(Value::as_array) {
        Some(groups) if groups.is_empty() => CommandReading::Clear,
        Some(groups) => CommandReading::Running(groups.len()),
        None => CommandReading::Unreadable,
    }
}

/// Every live product lease's reading, summed: the update gate's and the quit sheet's input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandsReading {
    /// Commands running, over every lease.
    pub running: usize,
    /// Leases whose state file could not be read.
    pub unreadable: usize,
}

impl CommandsReading {
    pub fn of(readings: impl IntoIterator<Item = CommandReading>) -> Self {
        readings.into_iter().fold(Self::default(), |mut all, reading| {
            match reading {
                CommandReading::Clear => {}
                CommandReading::Running(n) => all.running += n,
                CommandReading::Unreadable => all.unreadable += 1,
            }
            all
        })
    }
}

/// **The leases alive in this process**, by session id, with their state files: registered when
/// a supervised lease starts and removed when it is dropped. Every one of them is read, the
/// conversation's, a parked desk's, a spare's and every back end's, so none can hide a command.
pub struct LiveLeases(Mutex<BTreeMap<String, PathBuf>>);

static PROCESS_LEASES: LiveLeases = LiveLeases(Mutex::new(BTreeMap::new()));

impl LiveLeases {
    pub const fn new() -> Self {
        LiveLeases(Mutex::new(BTreeMap::new()))
    }
    /// The set every product lease in this process joins.
    pub fn process() -> &'static LiveLeases {
        &PROCESS_LEASES
    }
    pub fn register(&self, session: &str, state: &Path) {
        self.0.lock().unwrap().insert(session.to_string(), state.to_path_buf());
    }
    /// The lease is gone: forget it and remove its file.
    pub fn retire(&self, session: &str) {
        if let Some(state) = self.0.lock().unwrap().remove(session) {
            std::fs::remove_file(state).ok();
        }
    }
    pub(crate) fn contains(&self, session: &str) -> bool {
        self.0.lock().unwrap().contains_key(session)
    }

    pub fn reading(&self) -> CommandsReading {
        let states: Vec<PathBuf> = self.0.lock().unwrap().values().cloned().collect();
        CommandsReading::of(states.iter().map(|state| read(state)))
    }
}

impl Default for LiveLeases {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("richos-lease-commands-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// **N1's successor** (design 1.5 clause 2): the product assembles exactly this, and every
    /// reap parameter is an argument of the supervisor's, none an environment name.
    #[test]
    fn the_product_invocation_is_byte_exact() {
        let args = supervisor_args(Path::new("/E/engine"), Path::new("/D/engine-state/provider-leases/s-1.json"),
                                   Path::new("/D/engine-state/provider-reap.log"));
        let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, [
            "/E/engine/scripts/provider-supervisor.py",
            "--reap-descendants",
            "--reap-grace=0",
            "--reap-state=/D/engine-state/provider-leases/s-1.json",
            "--reap-log=/D/engine-state/provider-reap.log",
        ]);
    }

    #[test]
    fn a_state_file_reads_as_clear_running_or_unreadable_and_never_as_a_guessed_zero() {
        let dir = root("read");
        let path = dir.join("s.json");
        assert_eq!(read(&path), CommandReading::Unreadable, "a missing file is not a zero");
        std::fs::write(&path, "{\"outside_provider_groups\":").unwrap();
        assert_eq!(read(&path), CommandReading::Unreadable, "a torn file is not a zero");
        std::fs::write(&path, json!({"outside_provider_group": [7]}).to_string()).unwrap();
        assert_eq!(read(&path), CommandReading::Unreadable, "a file without the groups is not a zero");
        std::fs::write(&path, json!({"outside_provider_groups": []}).to_string()).unwrap();
        assert_eq!(read(&path), CommandReading::Clear);
        std::fs::write(&path, json!({"outside_provider_groups": [4242, 4343]}).to_string()).unwrap();
        assert_eq!(read(&path), CommandReading::Running(2));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn every_live_lease_is_read_and_a_retired_one_is_forgotten_with_its_file() {
        let dir = root("live");
        let leases = LiveLeases::new();
        let (a, b) = (dir.join("a.json"), dir.join("b.json"));
        std::fs::write(&a, json!({"outside_provider_groups": [11]}).to_string()).unwrap();
        leases.register("a", &a);
        leases.register("b", &b);          // started, no snapshot yet
        assert_eq!(leases.reading(), CommandsReading { running: 1, unreadable: 1 });
        std::fs::write(&b, json!({"outside_provider_groups": []}).to_string()).unwrap();
        assert_eq!(leases.reading(), CommandsReading { running: 1, unreadable: 0 });
        leases.retire("a");
        assert!(!a.exists(), "a retired lease's file was left behind");
        assert_eq!(leases.reading(), CommandsReading { running: 0, unreadable: 0 });
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prepare_bounds_the_log_and_removes_only_another_process_s_state_files() {
        let dir = root("prepare");
        std::fs::create_dir_all(dir.join(LEASES_DIR)).unwrap();
        let theirs = dir.join(LEASES_DIR).join("old.json");
        let ours = dir.join(LEASES_DIR).join("live.json");
        std::fs::write(&theirs, json!({"owner": u32::MAX, "outside_provider_groups": [9]}).to_string()).unwrap();
        std::fs::write(&ours, json!({"owner": std::process::id(), "outside_provider_groups": []}).to_string()).unwrap();
        std::fs::write(log_path(&dir), vec![b'x'; (REAP_LOG_LIMIT + 1) as usize]).unwrap();
        let (state, log) = prepare(&dir, "fresh").unwrap();
        assert_eq!(state, dir.join(LEASES_DIR).join("fresh.json"));
        assert_eq!(log, dir.join(REAP_LOG));
        assert!(!theirs.exists(), "an earlier run's state file was kept");
        assert!(ours.exists(), "a live lease's state file was removed");
        assert!(!log.exists() && dir.join(format!("{REAP_LOG}.1")).exists(), "the log was not moved aside");
        std::fs::remove_dir_all(dir).ok();
    }
}
