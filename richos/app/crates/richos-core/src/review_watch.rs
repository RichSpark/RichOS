//! **The host's review-watch child: the second review, started by itself, in the app.**
//!
//! Slice 4 of richos-hq `docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md`, with
//! Sage's check beside it (`...-sage-check.md` §1.3, §2 catch 7, §4 row 4). His words (ruling
//! §113): *"A regular RichOS user can never be expected anything even remotely close to that.
//! So, this all must be completely automated."*
//!
//! The engine's `review-watch` is a Claude Code plugin monitor, and the app's leases cannot run
//! one: a product lease loads no settings and no monitors (`native.rs`, `--setting-sources ''`).
//! So the HOST runs it, as its own child, started at launch and ended at quit:
//!
//! * [`Mode::App`]: every install. `review_watch.py --app-state <engine-state>` reads the
//!   app's own registry and starts MID-JOB reviews of running workers (the handover review is
//!   the coordinator's reviewer, which `integrate` already requires). Its verdicts reach the
//!   running worker through the app's hook (`scripts/app-engine-hook.py`), so nothing it prints
//!   needs delivering; its output goes to a log. Each review runs on the Claude account the
//!   app's work runs on when its reviewer starts (`--accounts`, the app's account list).
//!
//! **Its process id is recorded** (`<state>/review-watch/host-child-<mode>.json`) while it
//! runs, and the record goes when it has been stopped and reaped. **Stopping** is SIGTERM to
//! the leader, which stops the reviews it started (each in its own session, out of this
//! group's reach) and exits; the group SIGKILL follows only after [`STOP_BOUND`]. A child whose
//! app died without a quit notices on its own: it exits when its parent process id is no longer
//! this host's, which it is given at spawn ([`HOST_ENV`]), so an app that dies while the child is
//! still starting is noticed before its first look too.
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use crate::owned_process::{OwnedChild, SupervisedSet};

/// How long the watcher gets after SIGTERM to stop its reviews and exit before its group is
/// SIGKILLed. Each review's own stop is a SIGTERM to the process group it leads, which holds
/// `second_review.py` and its reviewer alike, milliseconds when nothing is wedged. The watcher
/// stops all of its reviews AT ONCE and escalates a wedged one to SIGKILL itself, inside
/// `2 * FREEZE_SECONDS + QUIT_TERM_SECONDS + QUIT_KILL_SECONDS` = 3 s however many run
/// (`review_watch.py` `stop_own`), so every review is gone before this bound ends: the group
/// SIGKILL cannot reach them, each leads its own session. `estimate:` five seconds; the quit
/// path's own lease bound is two.
pub const STOP_BOUND: Duration = Duration::from_secs(5);

/// The fences of these children, apart from the leases' set, so the quit path's lease count
/// stays a count of leases.
static WATCHERS: SupervisedSet = SupervisedSet::new();

/// Set in every child's environment: this host's own process id, which the child checks its
/// parent against before its first look (`review_watch.py` `run_host_loop`; the real second
/// review of 16c154f5a, finding 2).
pub const HOST_ENV: &str = "RICHOS_REVIEW_WATCH_HOST";

#[derive(Clone)]
pub enum Mode {
    /// The app's own watcher: mid-job reviews of the app's workers.
    App {
        /// The Claude CLI the app ships, which the reviews run on.
        claude: Option<PathBuf>,
        /// The app's account list (`<data>/`[`crate::claude_accounts::LIST_FILE`]). Each review
        /// reads the account in use from it when its reviewer starts and runs on that account's
        /// folder, as a work lease does (`quota::Service::lease_account`, `engine_profile.rs`):
        /// this child's environment is cleared, so a switch would otherwise never reach it (the
        /// real second review of 802194f0e, finding 1).
        accounts: PathBuf,
        /// The Settings switch "Let Codex review your team's work"
        /// (`<data>/`[`crate::codex_reviews::SETTING_FILE`], round 20.2). Read by the watcher when
        /// each review starts: on, Codex reviews whenever it is installed and signed in, with the
        /// isolation that keeps it out of the user's own Codex and ChatGPT apps; off (and on first
        /// run), Claude.
        codex_reviews: PathBuf,
    },
}

impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::App { .. } => "app",
        }
    }
}

/// Everything one child needs. `state` is `<app data>/engine-state`: the child's record and log
/// live under it, and it is also the app state the watcher reads.
#[derive(Clone)]
pub struct Launch {
    pub python: PathBuf,
    pub engine: PathBuf,
    pub state: PathBuf,
    /// The child's whole environment (it inherits nothing else): PATH first of all.
    pub environment: BTreeMap<String, String>,
    pub mode: Mode,
}

impl Launch {
    pub fn record_path(&self) -> PathBuf {
        state_dir(&self.state).join(format!("host-child-{}.json", self.mode.name()))
    }
    pub fn log_path(&self) -> PathBuf {
        state_dir(&self.state).join(format!("host-child-{}.log", self.mode.name()))
    }
}

/// Where the child's record and log live.
pub fn state_dir(state: &Path) -> PathBuf {
    state.join("review-watch")
}

pub struct ReviewWatch {
    child: Mutex<OwnedChild>,
    pid: u32,
    record: PathBuf,
}

impl ReviewWatch {
    /// Start the child and record its process id. An engine without the watcher refuses here,
    /// with nothing started.
    pub fn start(launch: &Launch) -> io::Result<Self> {
        let script = launch.engine.join("scripts/lib/review_watch.py");
        if !script.is_file() {
            return Err(io::Error::other(format!("this engine has no {}", script.display())));
        }
        std::fs::create_dir_all(state_dir(&launch.state))?;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(launch.log_path())?;
        let mut command = crate::runtime::interpreter_command(&launch.python);
        command.arg(&script).arg("--monitor").arg("--engine-root").arg(&launch.engine);
        match &launch.mode {
            Mode::App { claude, accounts, codex_reviews } => {
                command.arg("--app-state").arg(&launch.state).arg("--accounts").arg(accounts)
                    .arg("--codex-reviews").arg(codex_reviews);
                if let Some(claude) = claude {
                    command.arg("--claude").arg(claude);
                }
            }
        }
        command.env_clear().envs(&launch.environment)
            .env("PYTHONDONTWRITEBYTECODE", "1").env("PYTHONNOUSERSITE", "1")
            .env(HOST_ENV, std::process::id().to_string());
        command.current_dir(&launch.state).stderr(Stdio::from(log.try_clone()?));
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(log));
        OwnedChild::configure(&mut command);
        let child = command.spawn()?;
        let pid = child.id();
        // Fenced before anything else can fail, so an error below still ends it.
        let owned = OwnedChild::supervised_in(child, STOP_BOUND, &WATCHERS);
        let record = launch.record_path();
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        write_record(&record, &serde_json::json!({"pid": pid, "started_at": started, "mode": launch.mode.name()}))?;
        Ok(ReviewWatch { child: Mutex::new(owned), pid, record })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Still running (not exited)?
    pub fn running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// SIGTERM, at most [`STOP_BOUND`] for it to stop its reviews and exit, then the group
    /// SIGKILL; reaped either way, and the record removed once nothing is left for it to name.
    pub fn stop(&self) {
        if let Err(e) = self.child.lock().unwrap().wait() {
            eprintln!("[richos] second review: watcher {} could not be reaped ({e})", self.pid);
        }
        match std::fs::remove_file(&self.record) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                eprintln!("[richos] second review: {} could not be removed ({e})", self.record.display());
            }
            _ => {}
        }
    }
}

impl Drop for ReviewWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A child's record, written whole.
pub(crate) fn write_record(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    let temporary = path.with_extension(format!("{}.incoming", uuid::Uuid::new_v4()));
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() && std::fs::remove_file(&temporary).is_err() {
        eprintln!("[richos] a watcher's record: {} was left behind", temporary.display());
    }
    written
}

/// The process's own children, one per mode, started at launch and ended at quit.
static RUNNING: Mutex<BTreeMap<&'static str, ReviewWatch>> = Mutex::new(BTreeMap::new());

/// Start this mode's child unless one is already running (the shell calls it at launch and
/// again when a work lease opens, which covers an engine first-run setup put in place after
/// launch). Its pid.
pub fn ensure(launch: &Launch) -> io::Result<u32> {
    let mut running = RUNNING.lock().unwrap();
    let name = launch.mode.name();
    if let Some(existing) = running.get(name) {
        if existing.running() {
            return Ok(existing.pid());
        }
        running.remove(name);
    }
    let started = ReviewWatch::start(launch)?;
    let pid = started.pid();
    running.insert(name, started);
    Ok(pid)
}

/// The quit path: every child stopped (side by side, within one [`STOP_BOUND`]) and reaped,
/// and its record removed. The pids that were stopped.
pub fn stop_all() -> Vec<u32> {
    let all: Vec<ReviewWatch> = std::mem::take(&mut *RUNNING.lock().unwrap()).into_values().collect();
    let pids = all.iter().map(ReviewWatch::pid).collect();
    let threads: Vec<_> = all.into_iter().map(|watch| std::thread::spawn(move || watch.stop())).collect();
    for thread in threads {
        if thread.join().is_err() {
            eprintln!("[richos] second review: stopping a watcher panicked");
        }
    }
    pids
}

/// A child's environment: the PATH given (the verified runtime's, so `python3`, `git` and
/// `bash` are the delivered ones) and the user's own HOME and locale, nothing else inherited,
/// except the app's own CLAUDE_CONFIG_DIR: Account 1 has no folder of its own and runs where the
/// app's environment says, for a review as for a work lease (`engine_profile.rs`).
pub fn environment(path: &str) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("PATH".into(), path.into());
    for key in ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "CLAUDE_CONFIG_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value.to_string_lossy().into_owned());
        }
    }
    env
}

/// [`Mode::App`]'s environment: [`environment`] over the runtime's PATH, and `codex_search`, the
/// PATH the Settings row looks for Codex on ([`crate::codex_reviews::search_path`]), as
/// [`crate::codex_reviews::SEARCH_ENV`]: every review looks for Codex there, after ChatGPT.app,
/// exactly as the row does, so the reviewer the row shows is the one the review runs (the real
/// second review of cf3c4482f). The runtime's PATH alone leaves out global installs.
pub fn app_environment(runtime_path: &str, codex_search: &str) -> BTreeMap<String, String> {
    let mut env = environment(runtime_path);
    env.insert(crate::codex_reviews::SEARCH_ENV.into(), codex_search.into());
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap()
    }

    fn alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    fn python() -> PathBuf {
        ["/usr/bin/python3", "/opt/homebrew/bin/python3", "/usr/local/bin/python3"]
            .iter().map(PathBuf::from).find(|p| p.is_file()).expect("a python3 for the engine's watcher")
    }

    /// **The child starts at launch and its recorded PID is gone after quit** (slice 4's
    /// proof). The real engine watcher in app mode, over an empty app state: started, its pid
    /// recorded, still running a moment later (an engine whose watcher refuses `--app-state`
    /// exits at once and fails here), then the quit path ends it and the record is removed.
    #[test]
    fn the_apps_review_watch_starts_records_its_pid_and_is_gone_after_quit() {
        let root = std::env::temp_dir().join(format!("review-watch-app-{}", uuid::Uuid::new_v4()));
        let state = root.join("engine-state");
        std::fs::create_dir_all(&state).unwrap();
        let state = state.canonicalize().unwrap();
        let launch = Launch {
            python: python(), engine: engine(), state: state.clone(),
            environment: environment("/usr/bin:/bin:/usr/sbin:/sbin"),
            // The app's account list beside the state (the real second review of 802194f0e,
            // finding 1): an engine whose watcher refuses --accounts exits at once and fails here.
            // And the Codex switch beside it (round 20.2): an engine whose watcher refuses
            // --codex-reviews exits at once and fails here too.
            mode: Mode::App { claude: None, accounts: root.join(crate::claude_accounts::LIST_FILE),
                              codex_reviews: root.join(crate::codex_reviews::SETTING_FILE) },
        };
        let pid = ensure(&launch).unwrap();
        assert_eq!(ensure(&launch).unwrap(), pid, "a second call started a second watcher");
        let record: serde_json::Value = serde_json::from_slice(&std::fs::read(launch.record_path()).unwrap()).unwrap();
        assert_eq!(record["pid"], pid);
        // The fact, not the clock: its first look writes the app session's record. The bound
        // only catches a hang.
        let looked = state.join("review-watch/sessions/app/told.json");
        let began = std::time::Instant::now();
        while !looked.is_file() && began.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(looked.is_file(), "the watcher never looked: {}",
                std::fs::read_to_string(launch.log_path()).unwrap_or_default());
        assert!(alive(pid), "the watcher exited on its own: {}",
                std::fs::read_to_string(launch.log_path()).unwrap_or_default());
        assert_eq!(stop_all(), vec![pid]);
        assert!(!alive(pid), "the recorded pid {pid} is still running after quit");
        assert!(!launch.record_path().exists(), "the record outlived the child");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
