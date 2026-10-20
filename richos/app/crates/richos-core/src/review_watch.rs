//! **The host's review-watch child: the second review, started by itself, in the app.**
//!
//! Slice 4 of richos-hq `docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md`, with
//! Sage's check beside it (`...-sage-check.md` §1.3, §2 catch 7, §4 row 4). His words (ruling
//! §113): *"A regular RichOS user can never be expected anything even remotely close to that.
//! So, this all must be completely automated."*
//!
//! The engine's `review-watch` is a Claude Code plugin monitor, and neither of the app's two
//! kinds of lead can run one: a product lease loads no settings and no monitors
//! (`native.rs`, `--setting-sources ''`), and a monitor inside the operator install's print-mode
//! lead would start a turn the host never asked for and cannot attribute (`operator_lead.rs`
//! identifies a turn by the user message this host sent). So the HOST runs it, as its own
//! child, started at launch and ended at quit:
//!
//! * [`Mode::App`]: every install. `review_watch.py --app-state <engine-state>` reads the
//!   app's own registry and starts MID-JOB reviews of running workers (the handover review is
//!   the coordinator's reviewer, which `integrate` already requires). Its verdicts reach the
//!   running worker through the app's hook (`scripts/app-engine-hook.py`), so nothing it prints
//!   needs delivering; its output goes to a log. Each review runs on the Claude account the
//!   app's work runs on when its reviewer starts (`--accounts`, the app's account list).
//! * [`Mode::Operator`]: his team's install. `review_watch.py --host-json` against his
//!   governed repository's `orchestration.config`; each notice comes back as one JSON line per
//!   lead session, and the shell hands it to [`crate::operator_desk::OperatorDesk::tell_lead`],
//!   which sends it to that lead as a message of the host's own. **A verdict counts as delivered
//!   only once its lead was told** (the real second review of 0d83e456d, finding 3): the child is
//!   started with [`ACKS_ENV`] set, and for each notice the sink accepted the host writes the
//!   notice's review ids back on the child's stdin as `{"ack": [...]}`; a notice the desk
//!   refused gets no acknowledgment and is told again at the child's next look.
//!
//! **Its process id is recorded** (`<state>/review-watch/host-child-<mode>.json`) while it
//! runs, and the record goes when it has been stopped and reaped. **Stopping** is SIGTERM to
//! the leader, which stops the reviews it started (each in its own session, out of this
//! group's reach) and exits; the group SIGKILL follows only after [`STOP_BOUND`]. A child whose
//! app died without a quit notices on its own: it exits when its parent process id is no longer
//! this host's, which it is given at spawn ([`HOST_ENV`]), so an app that dies while the child is
//! still starting is noticed before its first look too.
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
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

/// Set in the operator child's environment: this host acknowledges what it delivered, so the
/// child records a verdict delivered only on that acknowledgment (`review_watch.py` `host_acks`).
/// An engine that predates it ignores the variable, and its lines carry no ids to acknowledge.
pub const ACKS_ENV: &str = "RICHOS_REVIEW_WATCH_ACKS";

/// Set in every child's environment: this host's own process id, which the child checks its
/// parent against before its first look (`review_watch.py` `run_host_loop`; the real second
/// review of 16c154f5a, finding 2).
pub const HOST_ENV: &str = "RICHOS_REVIEW_WATCH_HOST";

/// One notice for one lead, from the operator child's JSON line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub session: String,
    pub text: String,
    /// The review ids of the verdicts in it, acknowledged back to the child once delivered.
    pub keys: Vec<String>,
}

/// Where a notice goes, called on the child's reader thread: true once its lead was told.
pub type NoticeSink = Arc<dyn Fn(Notice) -> bool + Send + Sync>;

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
    /// The operator install's watcher over his team's registry.
    Operator {
        /// The governed repository's `orchestration.config` (`SECOND_REVIEW_REPOS`).
        config: PathBuf,
        sink: NoticeSink,
    },
}

impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::App { .. } => "app",
            Mode::Operator { .. } => "operator",
        }
    }
}

/// Everything one child needs. `state` is `<app data>/engine-state` in both modes: the child's
/// record and log live under it, and in [`Mode::App`] it is also the app state the watcher reads.
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
            Mode::Operator { config, .. } => {
                command.arg("--host-json").arg("--config").arg(config);
            }
        }
        command.env_clear().envs(&launch.environment)
            .env("PYTHONDONTWRITEBYTECODE", "1").env("PYTHONNOUSERSITE", "1")
            .env(HOST_ENV, std::process::id().to_string());
        let reads = matches!(launch.mode, Mode::Operator { .. });
        if reads {
            command.env(ACKS_ENV, "1");
        }
        command.current_dir(&launch.state).stderr(Stdio::from(log.try_clone()?));
        command.stdin(if reads { Stdio::piped() } else { Stdio::null() });
        command.stdout(if reads { Stdio::piped() } else { Stdio::from(log) });
        OwnedChild::configure(&mut command);
        let mut child = command.spawn()?;
        let pid = child.id();
        let (out, acks) = (child.stdout.take(), child.stdin.take());
        // Fenced before anything else can fail, so an error below still ends it.
        let owned = OwnedChild::supervised_in(child, STOP_BOUND, &WATCHERS);
        if let (Mode::Operator { sink, .. }, Some(out), Some(mut acks)) = (&launch.mode, out, acks) {
            let sink = sink.clone();
            std::thread::Builder::new().name("richos-review-watch".into()).spawn(move || {
                for line in io::BufReader::new(out).lines() {
                    let Ok(line) = line else { return };
                    let Some(notice) = parse_notice(&line) else { continue };
                    let keys = notice.keys.clone();
                    if sink(notice) && !keys.is_empty() {
                        if let Err(e) = writeln!(acks, "{}", serde_json::json!({"ack": keys})).and_then(|()| acks.flush()) {
                            // Unacknowledged, the watcher tells it again: a repeat, never a loss.
                            eprintln!("[richos] second review: a delivered notice could not be acknowledged ({e})");
                        }
                    }
                }
            })?;
        }
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

/// A child's record, written whole (also [`crate::team_watch`]'s).
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

fn parse_notice(line: &str) -> Option<Notice> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = value["text"].as_str()?.to_string();
    let session = value["session"].as_str().unwrap_or_default().to_string();
    let keys = value["keys"].as_array()
        .map(|keys| keys.iter().filter_map(|k| k.as_str().map(String::from)).collect())
        .unwrap_or_default();
    (!text.is_empty()).then_some(Notice { session, text, keys })
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

    /// The operator child's notices reach the sink, one per JSON line, with their session.
    #[test]
    fn an_operator_notice_line_carries_its_session_and_text() {
        assert_eq!(parse_notice(r#"{"session":"s-1","text":"REVIEW-WATCH 10:00Z: 1 notice"}"#),
                   Some(Notice { session: "s-1".into(), text: "REVIEW-WATCH 10:00Z: 1 notice".into(), keys: vec![] }));
        assert_eq!(parse_notice(r#"{"session":"s-1","text":"t","keys":["rv-1","rv-2"]}"#).unwrap().keys,
                   vec!["rv-1".to_string(), "rv-2".to_string()]);
        assert_eq!(parse_notice("REVIEW-WATCH 10:00Z: not json"), None);
        assert_eq!(parse_notice(r#"{"session":"s-1","text":""}"#), None);
    }

    /// **The real second review of 0d83e456d, finding 3: only a notice its lead was told is
    /// acknowledged.** A stand-in for the engine's watcher prints three notices: one the sink
    /// accepts, one it refuses (a desk that refuses at quit) and one with no ids (an engine that
    /// predates acknowledgments); it records what comes back on its stdin and whether it was
    /// started with [`ACKS_ENV`]. One acknowledgment, of the accepted notice's ids only.
    #[test]
    fn the_host_acknowledges_only_the_notices_its_lead_was_told() {
        let root = std::env::temp_dir().join(format!("review-watch-acks-{}", uuid::Uuid::new_v4()));
        let engine = root.join("engine");
        std::fs::create_dir_all(engine.join("scripts/lib")).unwrap();
        std::fs::create_dir_all(root.join("engine-state")).unwrap();
        let got = root.join("got.json");
        std::fs::write(engine.join("scripts/lib/review_watch.py"), format!(r#"
import json, os, select, sys, time
print(json.dumps({{"session": "told", "text": "REVIEW-WATCH: 1 notice", "keys": ["rv-told"]}}), flush=True)
print(json.dumps({{"session": "refused", "text": "REVIEW-WATCH: 1 notice", "keys": ["rv-refused"]}}), flush=True)
print(json.dumps({{"session": "told", "text": "REVIEW-WATCH: 1 notice"}}), flush=True)
acks, end = [], time.monotonic() + 5
while time.monotonic() < end and len(acks) < 3:
    if select.select([sys.stdin], [], [], 0.6)[0]:
        line = sys.stdin.readline()
        if not line:
            break
        acks.append(json.loads(line))
    elif acks:
        break
with open({got:?} + ".pending", "w") as target:
    json.dump({{"acks": acks, "env": os.environ.get("{ACKS_ENV}")}}, target)
os.replace({got:?} + ".pending", {got:?})
time.sleep(60)
"#)).unwrap();
        let sink: NoticeSink = Arc::new(|notice: Notice| notice.session == "told");
        let launch = Launch {
            python: python(), engine, state: root.join("engine-state"),
            environment: environment("/usr/bin:/bin:/usr/sbin:/sbin"),
            mode: Mode::Operator { config: root.join("orchestration.config"), sink },
        };
        let watch = ReviewWatch::start(&launch).unwrap();
        let began = std::time::Instant::now();
        while !got.is_file() && began.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let recorded = std::fs::read(&got).unwrap_or_default();
        drop(watch);
        let recorded: serde_json::Value = serde_json::from_slice(&recorded).unwrap_or_else(|e| panic!(
            "the stand-in recorded nothing ({e}): {}", std::fs::read_to_string(launch.log_path()).unwrap_or_default()));
        assert_eq!(recorded, serde_json::json!({"acks": [{"ack": ["rv-told"]}], "env": "1"}));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
