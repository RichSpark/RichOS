//! **Codex's entries reach Rich in the app, by themselves, once each** (gap X of richos-hq
//! `docs/plans/2026-10-21-switching-to-the-app.md`, item 5d).
//!
//! Codex writes to Rich through append-only `to-rich.md` files, one per channel folder, and Rich
//! answers in the `to-codex.md` beside each. In the terminal the engine's `codex-watch` plugin
//! monitor tells the lead each new entry. The app's leases load no plugin monitors
//! (`--setting-sources ''`), so the app runs the same engine watcher itself, as review-watch is
//! run ([`crate::review_watch`]):
//!
//! * **Which folders are channels is the user's setting**: `CODEX_CHANNELS` in their
//!   `orchestration.config` (colon-separated folders, `~` for the home folder; set empty, it
//!   watches none), read the way the terminal's watcher reads it. Unset, the conventional
//!   folders, [`DEFAULT_CHANNELS`] ([`channel_folders`]).
//! * **The watcher** is `codex_watch.py --monitor --host-json`, the app's own child from launch
//!   to quit, given the channels' `to-rich.md` files and its own state folder under the app's
//!   data ([`state_dir`]): an app's first start replays no backlog, and a later one tells what
//!   was written while it was closed. Each entry comes back as one JSON line, and its text goes
//!   to the sink the shell gives ([`Launch::sink`]), which saves it for Rich in the
//!   conversation on screen ([`deliver`]); the shell's peer-input watch hands it to him there
//!   as it hands him Stu's messages.
//! * **Rich and Stu can answer**: each channel folder that exists is one of their sessions'
//!   folders (`engine_profile.rs` `configure`), so either can write its `to-codex.md`.
//! * **Its process id is recorded** (`<state>/codex-watch/host-child.json`) while it runs, and
//!   the record goes when it has been stopped and reaped; a child whose app died without a quit
//!   exits on its own (it runs while its parent is the host whose pid it was given,
//!   `host_child.py`).
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::owned_process::{OwnedChild, SupervisedSet};

/// The user's setting in their `orchestration.config`.
pub const CONFIG_KEY: &str = "CODEX_CHANNELS";
/// The conventional channel folders, used when the user's `orchestration.config` sets no
/// [`CONFIG_KEY`]. The engine watcher's own default is the same value
/// (`engine/scripts/lib/codex_watch.py` `DEFAULT_CHANNELS`, held equal by a test below).
pub const DEFAULT_CHANNELS: &str = "~/.richos-coordination/rich-codex:~/.richos-coordination/rich-codex-questions";
/// The file in each channel folder Codex writes to Rich in.
pub const TO_RICH: &str = "to-rich.md";
/// The file in each channel folder Rich answers Codex in.
pub const TO_CODEX: &str = "to-codex.md";

/// The bound review-watch's children get after SIGTERM; this one starts nothing, so SIGTERM ends
/// it at once.
pub const STOP_BOUND: std::time::Duration = crate::review_watch::STOP_BOUND;

/// Set in the child's environment: this host's own process id (`host_child.py` `start`).
pub const HOST_ENV: &str = "RICHOS_WATCH_HOST";

/// Its fence, apart from the leases' set and review-watch's.
static WATCHERS: SupervisedSet = SupervisedSet::new();

/// **The user's Codex channel folders**: [`CONFIG_KEY`] in the `orchestration.config` of the
/// user's folder, else [`DEFAULT_CHANNELS`]. `~` is `home`; a relative entry, or `~` with no
/// home, names nothing. No user folder (tests, an install with no home folder) is no channel.
pub fn channel_folders(user_folder: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let Some(folder) = user_folder else { return Vec::new() };
    let value = match crate::user_settings::config_value(folder, CONFIG_KEY) {
        Ok(Some(value)) => value,
        Ok(None) => DEFAULT_CHANNELS.to_string(),
        Err(e) => {
            eprintln!("[richos] Codex channels: {e}; the conventional folders are watched");
            DEFAULT_CHANNELS.to_string()
        }
    };
    let mut folders = Vec::new();
    for entry in value.split(':').map(str::trim).filter(|entry| !entry.is_empty()) {
        let path = match entry.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') =>
                match home { Some(home) => home.join(rest.trim_start_matches('/')), None => continue },
            _ => PathBuf::from(entry),
        };
        if path.is_absolute() && !folders.contains(&path) {
            folders.push(path);
        }
    }
    folders
}

/// The user's channel folders on this Mac: [`channel_folders`] of the app's central folder,
/// `~` being `$HOME`.
pub fn installed_channel_folders() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).filter(|home| !home.as_os_str().is_empty());
    channel_folders(crate::company::install_central_root().as_deref(), home.as_deref())
}

/// **Save one entry for Rich in `binding`'s conversation** (`<state>/messages/…/rich.jsonl`),
/// where the shell's peer-input watch hands it to him as it hands him Stu's messages.
pub fn deliver(state: &Path, binding: &crate::entity::ThreadBinding, text: &str) -> Result<(), String> {
    let scope = crate::status_tools::StatusToolScope {
        version: 1, state_root: state.to_path_buf(),
        entity_id: binding.entity_id().to_string(), thread_id: binding.thread_id().to_string(),
    };
    crate::session_tools::observe_codex(&scope, text)
}

/// Where each entry's text goes, called on the child's reader thread, one entry at a time. It
/// returns once the entry is saved for Rich (or the app is quitting): the next waits for it.
pub type Sink = Arc<dyn Fn(String) + Send + Sync>;

/// Everything the child needs. `state` is `<app data>/engine-state`.
#[derive(Clone)]
pub struct Launch {
    pub python: PathBuf,
    pub engine: PathBuf,
    pub state: PathBuf,
    /// The child's whole environment (it inherits nothing else): PATH first of all.
    pub environment: std::collections::BTreeMap<String, String>,
    /// The channel folders watched ([`channel_folders`]).
    pub channels: Vec<PathBuf>,
    pub sink: Sink,
}

impl Launch {
    pub fn record_path(&self) -> PathBuf {
        state_dir(&self.state).join("host-child.json")
    }
    pub fn log_path(&self) -> PathBuf {
        state_dir(&self.state).join("host-child.log")
    }
}

/// Where the child's record, log and the watcher's own state live.
pub fn state_dir(state: &Path) -> PathBuf {
    state.join("codex-watch")
}

struct CodexWatch {
    child: Mutex<OwnedChild>,
    pid: u32,
    record: PathBuf,
    channels: Vec<PathBuf>,
    _reader: JoinHandle<()>,
}

impl CodexWatch {
    fn start(launch: &Launch) -> io::Result<Self> {
        let script = launch.engine.join("scripts/lib/codex_watch.py");
        if !script.is_file() {
            return Err(io::Error::other(format!("this engine has no {}", script.display())));
        }
        let dir = state_dir(&launch.state);
        std::fs::create_dir_all(&dir)?;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(launch.log_path())?;
        let files: Vec<String> = launch.channels.iter().map(|folder| folder.join(TO_RICH).to_string_lossy().into_owned()).collect();
        let mut command = crate::runtime::interpreter_command(&launch.python);
        command.arg(&script).arg("--monitor").arg("--host-json").arg("--engine-root").arg(&launch.engine);
        command.env_clear().envs(&launch.environment)
            .env("PYTHONDONTWRITEBYTECODE", "1").env("PYTHONNOUSERSITE", "1")
            .env(HOST_ENV, std::process::id().to_string())
            .env("CODEX_WATCH_CHANNELS", files.join(":"))
            .env("CODEX_WATCH_STATE_DIR", dir.join("watch"));
        command.current_dir(&launch.state).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::from(log));
        OwnedChild::configure(&mut command);
        let mut child = command.spawn()?;
        let pid = child.id();
        let output = child.stdout.take();
        // Fenced before anything else can fail, so an error below still ends it.
        let owned = OwnedChild::supervised_in(child, STOP_BOUND, &WATCHERS);
        let output = output.ok_or_else(|| io::Error::other("the watcher's output could not be read"))?;
        let sink = Arc::clone(&launch.sink);
        let reader = std::thread::Builder::new().name("richos-codex-watch".into()).spawn(move || {
            for line in BufReader::new(output).lines() {
                let text = line.map_err(|e| e.to_string()).and_then(|line| {
                    let value: serde_json::Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
                    value["text"].as_str().map(str::to_owned).ok_or_else(|| "a line with no text".to_string())
                });
                match text {
                    Ok(text) => sink(text),
                    Err(e) => eprintln!("[richos] Codex watch: its output could not be read ({e})"),
                }
            }
        })?;
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        crate::review_watch::write_record(&launch.record_path(), &serde_json::json!({
            "pid": pid, "started_at": started, "channels": files}))?;
        Ok(CodexWatch { child: Mutex::new(owned), pid, record: launch.record_path(), channels: launch.channels.clone(), _reader: reader })
    }

    fn running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// SIGTERM, at most [`STOP_BOUND`], then the group SIGKILL; reaped either way, and the
    /// record removed.
    fn stop(&self) {
        if let Err(e) = self.child.lock().unwrap().wait() {
            eprintln!("[richos] Codex watch: watcher {} could not be reaped ({e})", self.pid);
        }
        match std::fs::remove_file(&self.record) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                eprintln!("[richos] Codex watch: {} could not be removed ({e})", self.record.display());
            }
            _ => {}
        }
    }
}

impl Drop for CodexWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The process's own watcher, started at launch and ended at quit.
static RUNNING: Mutex<Option<CodexWatch>> = Mutex::new(None);

/// **Start the watcher unless it already runs on these channels** (the shell calls it at launch
/// and again when a session opens, which covers an engine set up after launch and a changed
/// setting). One running on other channels is stopped and started on these; with no channel
/// (the user set the setting empty) none runs. Its pid, or `None` when none runs.
pub fn ensure(launch: &Launch) -> io::Result<Option<u32>> {
    let mut running = RUNNING.lock().unwrap();
    if let Some(existing) = running.as_ref() {
        if existing.channels == launch.channels && existing.running() {
            return Ok(Some(existing.pid));
        }
    }
    drop(running.take());
    if launch.channels.is_empty() {
        return Ok(None);
    }
    let started = CodexWatch::start(launch)?;
    let pid = started.pid;
    *running = Some(started);
    Ok(Some(pid))
}

/// The quit path: the watcher stopped and reaped, its record removed. The pid that was stopped.
pub fn stop_all() -> Option<u32> {
    let watch = RUNNING.lock().unwrap().take()?;
    let pid = watch.pid;
    drop(watch);
    Some(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn engine() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap()
    }

    fn python() -> PathBuf {
        ["/usr/bin/python3", "/opt/homebrew/bin/python3", "/usr/local/bin/python3"]
            .iter().map(PathBuf::from).find(|p| p.is_file()).expect("a python3 for the engine's watcher")
    }

    fn alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("codex-watch-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root.canonicalize().unwrap()
    }

    /// **The setting names the channels; unset, the conventional folders; set empty, none.**
    #[test]
    fn the_users_setting_names_the_channel_folders_with_the_conventional_ones_as_default() {
        let root = scratch("setting");
        let (user, home) = (root.join("central"), root.join("home"));
        std::fs::create_dir_all(&user).unwrap();
        assert_eq!(channel_folders(Some(&user), Some(&home)),
                   vec![home.join(".richos-coordination/rich-codex"), home.join(".richos-coordination/rich-codex-questions")],
                   "no setting: the conventional folders");
        std::fs::write(user.join(crate::user_settings::ORCHESTRATION_CONFIG),
            format!("CODEX_CHANNELS=\"~/codex one:{}:relative/ignored:~\"\n", root.join("abs").display())).unwrap();
        assert_eq!(channel_folders(Some(&user), Some(&home)), vec![home.join("codex one"), root.join("abs"), home.clone()]);
        assert_eq!(channel_folders(Some(&user), None), vec![root.join("abs")], "~ with no home names nothing");
        std::fs::write(user.join(crate::user_settings::ORCHESTRATION_CONFIG), "CODEX_CHANNELS=\"\"\n").unwrap();
        assert!(channel_folders(Some(&user), Some(&home)).is_empty(), "set empty: no channel");
        assert!(channel_folders(None, Some(&home)).is_empty(), "no user folder: no channel");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The app's default and the engine watcher's are one value.
    #[test]
    fn the_apps_default_is_the_engine_watchers_default() {
        let source = std::fs::read_to_string(engine().join("scripts/lib/codex_watch.py")).unwrap();
        assert!(source.contains(&format!("DEFAULT_CHANNELS = \"{DEFAULT_CHANNELS}\"")),
                "codex_watch.py's DEFAULT_CHANNELS differs from the app's");
    }

    /// **A NEW ENTRY IN THE USER'S CODEX CHANNEL REACHES RICH, BY ITSELF AND ONCE** (gap X). The
    /// real engine watcher as the app's child, on a fixture channel folder the user's setting
    /// names: the backlog is not told, an appended entry reaches the sink once with every line,
    /// the sink's [`deliver`] saves it in Rich's inbox of the conversation, and quit ends the
    /// watcher and removes its record.
    #[test]
    fn a_new_entry_in_the_users_codex_channel_reaches_rich_once_and_the_watcher_ends_at_quit() {
        let root = scratch("child");
        let (user, channel, state) = (root.join("central"), root.join("my codex channel"), root.join("engine-state"));
        for dir in [&user, &channel, &state] { std::fs::create_dir_all(dir).unwrap(); }
        std::fs::write(user.join(crate::user_settings::ORCHESTRATION_CONFIG), format!("CODEX_CHANNELS={}\n", channel.display())).unwrap();
        std::fs::write(channel.join(TO_RICH), "## 2026-10-28 07:00Z — OLD BACKLOG\nold-backlog-line\n\n").unwrap();
        let channels = channel_folders(Some(&user), None);
        assert_eq!(channels, vec![channel.clone()]);

        let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
        let thread = ledger.create_thread("Codex", &crate::EntityId::parse("company").unwrap()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let (told, heard) = mpsc::channel::<String>();
        let (sink_state, sink_binding) = (state.clone(), binding.clone());
        let told = Mutex::new(told);
        let launch = Launch {
            python: python(), engine: engine(), state: state.clone(),
            // A look every half second here, so the test waits on the watcher, not its clock.
            environment: {
                let mut environment = crate::review_watch::environment("/usr/bin:/bin:/usr/sbin:/sbin");
                environment.insert("CODEX_WATCH_POLL_SECONDS".into(), "0.5".into());
                environment
            },
            channels,
            sink: Arc::new(move |text: String| {
                deliver(&sink_state, &sink_binding, &text).unwrap();
                told.lock().unwrap().send(text).unwrap();
            }),
        };
        let pid = ensure(&launch).unwrap().expect("a watcher on one channel");
        assert_eq!(ensure(&launch).unwrap(), Some(pid), "a second call started a second watcher");
        let record: serde_json::Value = serde_json::from_slice(&std::fs::read(launch.record_path()).unwrap()).unwrap();
        assert_eq!(record["pid"], pid);
        // The fact, not the clock: its first look writes its offsets. The bound only catches a hang.
        let looked = state_dir(&state).join("watch/app-host/offsets.json");
        let began = std::time::Instant::now();
        while !looked.is_file() && began.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(looked.is_file(), "the watcher never looked: {}", std::fs::read_to_string(launch.log_path()).unwrap_or_default());

        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(channel.join(TO_RICH)).unwrap()
            .write_all("## 2026-10-28 08:11Z — READY TO LAND: codex/fixture-branch\nBranch tip 0123abc.\nline-three\n".as_bytes()).unwrap();
        // load-bound: hang guard only (told at the look after the file has settled).
        let text = heard.recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("the entry never reached Rich: {}", std::fs::read_to_string(launch.log_path()).unwrap_or_default()));
        assert!(text.starts_with("CODEX-WATCH ") && text.contains("my codex channel/to-rich.md"), "{text}");
        assert!(text.contains("## 2026-10-28 08:11Z — READY TO LAND: codex/fixture-branch")
                && text.contains("Branch tip 0123abc.") && text.ends_with("line-three"), "{text}");
        assert!(!text.contains("old-backlog-line"), "the backlog was told: {text}");

        let scope = crate::status_tools::StatusToolScope {
            version: 1, state_root: state.clone(), entity_id: "company".into(), thread_id: thread.clone() };
        let (message, through) = crate::session_tools::pending(&scope, "rich").unwrap().expect("nothing saved for Rich");
        assert_eq!((message.from.as_str(), message.text.as_str(), message.handoff), ("codex-watch", text.as_str(), false));
        assert!(message.instruction.is_none(), "a Codex entry is never the user's instruction");
        crate::session_tools::acknowledge(&scope, "rich", through).unwrap();
        // Told once, by the watcher's own record: it has told up to the file's end, and a look
        // finished after that told nothing more. The bounds only catch a hang.
        let end = std::fs::metadata(channel.join(TO_RICH)).unwrap().len();
        let key = channel.join(TO_RICH).to_string_lossy().into_owned();
        let offset = || std::fs::read(&looked).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|offsets| offsets[&key]["offset"].as_u64());
        let began = std::time::Instant::now();
        while offset() != Some(end) && began.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(offset(), Some(end), "the watcher never recorded the entry as told");
        let recorded = std::time::SystemTime::now();
        let later_look = || std::fs::metadata(&looked).and_then(|m| m.modified()).is_ok_and(|at| at > recorded);
        while !later_look() && began.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(later_look(), "the watcher never looked again: {}", std::fs::read_to_string(launch.log_path()).unwrap_or_default());
        assert!(heard.try_recv().is_err(), "the entry was told twice");
        assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none());

        assert_eq!(stop_all(), Some(pid));
        assert!(!alive(pid), "the recorded pid {pid} is still running after quit");
        assert!(!launch.record_path().exists(), "the record outlived the child");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
