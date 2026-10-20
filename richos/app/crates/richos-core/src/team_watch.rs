//! **The host's quota-watch, stall-watch and codex-watch children: his team's lead is woken in the
//! app as the terminal's monitors wake it.**
//!
//! Of the engine's four plugin monitors (`engine/monitors/monitors.json`), Claude Code starts
//! none in his team's lead in the app: the lead runs with `--print` (`operator_profile.rs`), and
//! plugin monitors start only in interactive sessions. Review-watch was the first to be replaced
//! by a child of the HOST ([`crate::review_watch`], d74d92732); this module does exactly the same
//! for the other three, on his team's install only:
//!
//! * each runs as `bash <engine>/scripts/<name>.sh --monitor --host-json`, the host's own child
//!   from launch to quit, with his stored environment and his `HOME`
//!   ([`crate::operator_declaration::script_environment`]) and his governed repository as
//!   `RICHOS_ENTITY_ROOT`;
//! * each notice comes back as one JSON line `{"text": ...}` and goes to the shell's sink, which
//!   hands it to [`crate::operator_desk::OperatorDesk::tell_team`]: sent to his team's lead as a
//!   message of the host's own, the way a review-watch notice is, or kept and sent when a lead
//!   starts ([`crate::operator_host::OperatorHost::tell_team`]);
//! * the lead the watchers watch for is the one the host names in a file
//!   ([`crate::operator_host::OperatorHost::team_watch_lead_path`], given as [`LEAD_ENV`]), read
//!   at every look (`engine/scripts/lib/host_child.py`), so stall-watch looks at that lead's
//!   teammates and escalations and quota-watch names that lead's working teammates, as each does
//!   for the terminal session it runs in;
//! * its process id is recorded (`<state>/team-watch/<name>.json`) while it runs, and stopping
//!   is SIGTERM to the leader and the group SIGKILL only after [`STOP_BOUND`], reaped, the record
//!   removed. A child whose app died without a quit exits on its own: it runs while its parent
//!   is the host whose pid it was given at spawn ([`HOST_ENV`]).
use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use crate::owned_process::{OwnedChild, SupervisedSet};

/// How long a watcher gets after SIGTERM before its group is SIGKILLed: review-watch's bound.
/// These start nothing that must be stopped first (quota-watch's `get_usage` read is its own
/// process group, ended by that read's own deadline), so SIGTERM ends each at once.
pub const STOP_BOUND: std::time::Duration = crate::review_watch::STOP_BOUND;

/// Their fences, apart from the leases' set and review-watch's.
static WATCHERS: SupervisedSet = SupervisedSet::new();

/// Set in every child's environment: this host's own process id (`host_child.py` `start`).
pub const HOST_ENV: &str = "RICHOS_WATCH_HOST";
/// Set in every child's environment: the file naming the session of his team's lead
/// (`host_child.py` `session`).
pub const LEAD_ENV: &str = "RICHOS_WATCH_LEAD";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Watcher {
    Quota,
    Stall,
    Codex,
}

impl Watcher {
    pub const ALL: [Watcher; 3] = [Watcher::Quota, Watcher::Stall, Watcher::Codex];

    /// The engine's own name for it: its monitor's name and its script's.
    pub fn name(self) -> &'static str {
        match self {
            Watcher::Quota => "quota-watch",
            Watcher::Stall => "stall-watch",
            Watcher::Codex => "codex-watch",
        }
    }
}

/// Where a notice goes, called on the child's reader thread.
pub type NoticeSink = Arc<dyn Fn(Watcher, String) + Send + Sync>;

/// Everything the children need.
#[derive(Clone)]
pub struct Launch {
    pub engine: PathBuf,
    /// His governed repository, whose `orchestration.config` each watcher reads.
    pub entity: PathBuf,
    /// `<app data>/engine-state`: each child's record and log live under it.
    pub state: PathBuf,
    /// The file naming his team's lead's session.
    pub lead: PathBuf,
    /// The children's whole environment (they inherit nothing else): PATH first of all.
    pub environment: BTreeMap<String, String>,
    pub sink: NoticeSink,
}

impl Launch {
    pub fn record_path(&self, watcher: Watcher) -> PathBuf {
        state_dir(&self.state).join(format!("{}.json", watcher.name()))
    }
    pub fn log_path(&self, watcher: Watcher) -> PathBuf {
        state_dir(&self.state).join(format!("{}.log", watcher.name()))
    }
}

/// Where the children's records and logs live.
pub fn state_dir(state: &Path) -> PathBuf {
    state.join("team-watch")
}

pub struct TeamWatch {
    child: Mutex<OwnedChild>,
    pid: u32,
    record: PathBuf,
}

impl TeamWatch {
    /// Start the child and record its process id. An engine without the watcher refuses here,
    /// with nothing started.
    pub fn start(launch: &Launch, watcher: Watcher) -> io::Result<Self> {
        let script = launch.engine.join("scripts").join(format!("{}.sh", watcher.name()));
        if !script.is_file() {
            return Err(io::Error::other(format!("this engine has no {}", script.display())));
        }
        std::fs::create_dir_all(state_dir(&launch.state))?;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(launch.log_path(watcher))?;
        let mut command = std::process::Command::new("/bin/bash");
        command.arg(&script).arg("--monitor").arg("--host-json")
            .env_clear().envs(&launch.environment)
            .env("RICHOS_ENTITY_ROOT", &launch.entity)
            .env("PYTHONDONTWRITEBYTECODE", "1").env("PYTHONNOUSERSITE", "1")
            .env(HOST_ENV, std::process::id().to_string())
            .env(LEAD_ENV, &launch.lead)
            .current_dir(&launch.state)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::from(log));
        OwnedChild::configure(&mut command);
        let mut child = command.spawn()?;
        let pid = child.id();
        let out = child.stdout.take();
        // Fenced before anything else can fail, so an error below still ends it.
        let owned = OwnedChild::supervised_in(child, STOP_BOUND, &WATCHERS);
        if let Some(out) = out {
            let sink = launch.sink.clone();
            std::thread::Builder::new().name(format!("richos-{}", watcher.name())).spawn(move || {
                for line in io::BufReader::new(out).lines() {
                    let Ok(line) = line else { return };
                    match parse_notice(&line) {
                        Some(text) => sink(watcher, text),
                        None => eprintln!("[richos] his team: {} printed a line that is not a notice: {line}", watcher.name()),
                    }
                }
            })?;
        }
        let record = launch.record_path(watcher);
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        crate::review_watch::write_record(&record, &serde_json::json!({"pid": pid, "started_at": started, "watcher": watcher.name()}))?;
        Ok(TeamWatch { child: Mutex::new(owned), pid, record })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Still running (not exited)?
    pub fn running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// SIGTERM, at most [`STOP_BOUND`], then the group SIGKILL; reaped either way, and the
    /// record removed once nothing is left for it to name.
    pub fn stop(&self) {
        if let Err(e) = self.child.lock().unwrap().wait() {
            eprintln!("[richos] his team: watcher {} could not be reaped ({e})", self.pid);
        }
        match std::fs::remove_file(&self.record) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                eprintln!("[richos] his team: {} could not be removed ({e})", self.record.display());
            }
            _ => {}
        }
    }
}

impl Drop for TeamWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The text of one `{"text": ...}` line; `None` for anything else.
fn parse_notice(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = value["text"].as_str()?;
    (!text.is_empty()).then(|| text.to_string())
}

/// The process's own children, one per watcher, started at launch and ended at quit.
static RUNNING: Mutex<BTreeMap<Watcher, TeamWatch>> = Mutex::new(BTreeMap::new());

/// Start this watcher's child unless one is already running. Its pid.
pub fn ensure(launch: &Launch, watcher: Watcher) -> io::Result<u32> {
    let mut running = RUNNING.lock().unwrap();
    if let Some(existing) = running.get(&watcher) {
        if existing.running() {
            return Ok(existing.pid());
        }
        running.remove(&watcher);
    }
    let started = TeamWatch::start(launch, watcher)?;
    let pid = started.pid();
    running.insert(watcher, started);
    Ok(pid)
}

/// The quit path: every child stopped (side by side, within one [`STOP_BOUND`]) and reaped, and
/// its record removed. The pids that were stopped.
pub fn stop_all() -> Vec<u32> {
    let all: Vec<TeamWatch> = std::mem::take(&mut *RUNNING.lock().unwrap()).into_values().collect();
    let pids = all.iter().map(TeamWatch::pid).collect();
    let threads: Vec<_> = all.into_iter().map(|watch| std::thread::spawn(move || watch.stop())).collect();
    for thread in threads {
        if thread.join().is_err() {
            eprintln!("[richos] his team: stopping a watcher panicked");
        }
    }
    pids
}

/// The engine this crate ships beside, and fixture data for its three watchers: every path they
/// read or write under `sandbox`, never the operator's own state, registry, ledger or channels.
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;

    pub fn engine() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap()
    }

    /// The children's environment over `sandbox`, with a governed repository at `<sandbox>/entity`
    /// whose threshold is 93 and Codex's channel at `<sandbox>/to-rich.md`. Each watcher looks
    /// every fraction of a second.
    pub fn environment(sandbox: &Path) -> BTreeMap<String, String> {
        let entity = sandbox.join("entity");
        for dir in ["entity", "ws/sessions", "ws/agents", "waits", "slots", "testvm", "hold", "projects", "psessions", "la"] {
            std::fs::create_dir_all(sandbox.join(dir)).unwrap();
        }
        std::fs::write(entity.join("orchestration.config"), "QUOTA_PAUSE_PERCENT=93\n").unwrap();
        std::fs::write(sandbox.join("slots/limit"), "1\n").unwrap();
        std::fs::write(sandbox.join("lead.jsonl"), "").unwrap();
        let path = |p: &str| sandbox.join(p).display().to_string();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let mut env = BTreeMap::new();
        env.insert("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into());
        env.insert("HOME".into(), path("home"));
        for (key, value) in [
            ("RICHOS_WORKSPACES_DIR", path("ws")), ("RICHOS_SESSIONS_DIR", path("psessions")),
            ("RICHOS_PROJECTS_DIR", path("projects")), ("RICHOS_WAITS_DIR", path("waits")),
            ("RICHOS_PROOF_RUN_SLOTS_DIR", path("slots")), ("TESTVM_ROOT", path("testvm")),
            ("RICHOS_AGENT_HOLD_DIR", path("hold")), ("RICHOS_TEST_DEVICES_DIR", path("devices")),
            ("RICHOS_ESCALATION_LEDGER", path("escalations.jsonl")), ("STALL_WATCH_LEAD_TRANSCRIPT", path("lead.jsonl")),
            ("STALL_WATCH_SESSION_START", (now - 120).to_string()), ("STALL_WATCH_CPU_BUSY", "30".into()),
            ("STALL_WATCH_STATE_DIR", path("stall-state")), ("STALL_WATCH_POLL_SECONDS", "0.2".into()),
            ("CODEX_WATCH_STATE_DIR", path("codex-state")), ("CODEX_WATCH_POLL_SECONDS", "0.2".into()),
            ("CODEX_WATCH_CHANNELS", path("to-rich.md")),
            ("QUOTA_WATCH_STATE_DIR", path("quota-state")), ("QUOTA_WATCH_POLL_SECONDS", "1".into()),
            ("QUOTA_WATCH_STALE_SECONDS", "30".into()), ("QUOTA_PAYLOAD", path("payload.json")),
            ("QUOTA_CLAUDE_BIN", path("no-claude-here")), ("QUOTA_RESET_HELPER", "/usr/bin/false".into()),
            ("LOGIN_ALARM_PROJECTS_DIR", path("la")), ("LOGIN_ALARM_STATE", path("la/state.json")),
            ("LOGIN_ALARM_SECURITY", path("la-security")), ("LOGIN_ALARM_NOTIFY_CMD", path("la-notify")),
        ] {
            env.insert(key.into(), value);
        }
        env
    }

    /// The status line's reading: `used`% of the five-hour window, resetting in two hours.
    pub fn quota_reading(sandbox: &Path, used: u32) {
        let resets = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 7200;
        std::fs::create_dir_all(sandbox).unwrap();
        std::fs::write(sandbox.join("payload.json"), serde_json::json!(
            {"rate_limits": {"five_hour": {"used_percentage": used, "resets_at": resets}}}).to_string()).unwrap();
    }

    /// A teammate's `proceeding` escalation for the lead, raised a minute ago (after the lead's
    /// session started), in the fixture ledger.
    pub fn escalation(sandbox: &Path, id: &str) {
        let raised = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 60;
        let raised = crate::operator_claim::iso_utc(raised);
        let row = serde_json::json!({"event": "Escalation", "id": id, "raised": raised, "teammate": "fixture-mate",
            "worktree": sandbox.join("no-such-worktree"), "branch": "cc/fixture-mate", "repo": "fixture", "head": "",
            "state": "proceeding", "for": "lead", "title": "fixture title", "question": "a fixture question long enough to be one",
            "tried": "", "meanwhile": "", "record": "", "session_id": "", "actor": "fixture"});
        std::fs::write(sandbox.join("escalations.jsonl"), format!("{row}\n")).unwrap();
    }

    /// Codex-watch has looked once (its state file is written at the end of every look), so its
    /// channel's end is settled and an entry written now is new to it.
    pub fn codex_looked(sandbox: &Path) -> bool {
        sandbox.join("codex-state/operator-host/offsets.json").is_file()
    }

    /// Codex writes one entry to the lead.
    pub fn codex_entry(sandbox: &Path, words: &str) {
        std::fs::write(sandbox.join("to-rich.md"), format!("## 2026-10-20T00:00Z — fixture\n{words}\n")).unwrap();
    }

    /// Wait, bounded, for `done`.
    pub fn until(seconds: u64, done: impl Fn() -> bool) -> bool {
        let began = std::time::Instant::now();
        while !done() {
            if began.elapsed() > std::time::Duration::from_secs(seconds) {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    /// **Each child starts at launch, prints its notices as lines the host reads, and its recorded
    /// pid is gone after quit.** The real engine's three watchers in host mode, on fixture data:
    /// started, each pid recorded, a second call starts nothing, codex-watch's notice reaches the
    /// sink, then the quit path ends all three and their records are removed. An engine whose
    /// watchers refuse `--host-json` exits at once, prints nothing, and fails here.
    #[test]
    fn the_three_watchers_start_record_their_pids_and_are_gone_after_quit() {
        let sandbox = std::env::temp_dir().join(format!("team-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(sandbox.join("engine-state")).unwrap();
        let sandbox = sandbox.canonicalize().unwrap();
        let got: Arc<Mutex<Vec<(Watcher, String)>>> = Arc::default();
        let seen = got.clone();
        let launch = Launch {
            engine: fixture::engine(), entity: sandbox.join("entity"), state: sandbox.join("engine-state"),
            lead: sandbox.join("lead-pointer.json"), environment: fixture::environment(&sandbox),
            sink: Arc::new(move |w, text| seen.lock().unwrap().push((w, text))),
        };
        let pids: Vec<u32> = Watcher::ALL.iter().map(|w| ensure(&launch, *w).unwrap()).collect();
        assert_eq!(Watcher::ALL.iter().map(|w| ensure(&launch, *w).unwrap()).collect::<Vec<_>>(), pids,
                   "a second call started a second watcher");
        for (w, pid) in Watcher::ALL.iter().zip(&pids) {
            let record: serde_json::Value = serde_json::from_slice(&std::fs::read(launch.record_path(*w)).unwrap()).unwrap();
            assert_eq!(record["pid"], *pid);
        }
        // Codex-watch's first look settles where its channel ends; an entry written after it is told.
        assert!(fixture::until(30, || fixture::codex_looked(&sandbox)), "codex-watch never looked: {}",
                std::fs::read_to_string(launch.log_path(Watcher::Codex)).unwrap_or_default());
        fixture::codex_entry(&sandbox, "READY TO LAND fixture");
        assert!(fixture::until(30, || got.lock().unwrap().iter().any(|(w, t)| *w == Watcher::Codex && t.contains("READY TO LAND fixture"))),
                "codex-watch's notice never reached the sink: {}",
                std::fs::read_to_string(launch.log_path(Watcher::Codex)).unwrap_or_default());
        for (w, pid) in Watcher::ALL.iter().zip(&pids) {
            assert!(alive(*pid), "{} exited on its own: {}", w.name(),
                    std::fs::read_to_string(launch.log_path(*w)).unwrap_or_default());
        }
        let mut stopped = stop_all();
        stopped.sort_unstable();
        let mut want = pids.clone();
        want.sort_unstable();
        assert_eq!(stopped, want);
        for (w, pid) in Watcher::ALL.iter().zip(&pids) {
            assert!(!alive(*pid), "the recorded pid {pid} of {} is still running after quit", w.name());
            assert!(!launch.record_path(*w).exists(), "the record of {} outlived its child", w.name());
        }
        std::fs::remove_dir_all(&sandbox).unwrap();
    }

    #[test]
    fn a_notice_line_carries_its_whole_text() {
        assert_eq!(parse_notice(r#"{"text":"CODEX-WATCH 10:00Z: one\ntwo"}"#), Some("CODEX-WATCH 10:00Z: one\ntwo".into()));
        assert_eq!(parse_notice("QUOTA-THRESHOLD 95%"), None);
        assert_eq!(parse_notice(r#"{"text":""}"#), None);
    }
}
