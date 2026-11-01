//! The app's one owned Codex channel receiver.
//!
//! The shared engine discovers and decodes dedicated journals. Native acceptance
//! resolves their immutable ledger binding and syncs the existing Stu inbox before
//! saving an advisory position. Visibility and titles never route task contents.
//! Before explicit association, old Markdown observations are retained in the old
//! spool. The framed reader imports that saved spool once after designation; raw
//! tails remain preserved migration input. The child is fenced by the root's OS
//! receiver lock and reaped through the app's existing owned-process supervisor.
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

/// **Save one entry for Stu in `binding`'s conversation** (`<state>/messages/…/stu.jsonl`),
/// where the shell's peer-input watch hands it to him as it hands him Rich's messages.
pub fn deliver(state: &Path, binding: &crate::entity::ThreadBinding, text: &str) -> Result<(), String> {
    let scope = crate::status_tools::StatusToolScope {
        version: 1, state_root: state.to_path_buf(),
        entity_id: binding.entity_id().to_string(), thread_id: binding.thread_id().to_string(),
    };
    crate::session_tools::observe_codex(&scope, text)
}

/// A decoded event is accepted or held. Failed attempts repeat at the existing
/// engine scan cadence; source journals remain the authoritative outbox.
pub type Sink = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Saved observations emitted by the pre-association legacy reader. Retained
/// after migration so deterministic imports can replay against inbox receipts.
pub fn pending_path(state: &Path) -> PathBuf {
    state_dir(state).join("pending.jsonl")
}

/// Serialize pre-association legacy spool appends.
static PENDING: Mutex<()> = Mutex::new(());

/// Keep `text` at the end of the pending entries, on disk before this returns.
fn keep(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    let _one = PENDING.lock().unwrap();
    let mut line = serde_json::to_string(text).map_err(io::Error::other)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    file.sync_all()
}

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
    reader: Mutex<Option<JoinHandle<()>>>,

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
            .env("CODEX_WATCH_STATE_DIR", dir.join("watch"))
            .env("RICHOS_CODEX_CHANNEL_STATE",&launch.state)
            .env("CODEX_COORDINATION_ROOT",launch.environment.get("CODEX_COORDINATION_ROOT").map(PathBuf::from).unwrap_or_else(crate::codex_bridge::root));
        command.current_dir(&launch.state).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::from(log));
        OwnedChild::configure(&mut command);
        let mut child = command.spawn()?;
        let pid = child.id();
        let output = child.stdout.take();
        // Fenced before anything else can fail, so an error below still ends it.
        let owned = OwnedChild::supervised_in(child, STOP_BOUND, &WATCHERS);
        let output = output.ok_or_else(|| io::Error::other("the watcher's output could not be read"))?;
        // Legacy output is kept in pipe order before following framed imports.
        let pending = pending_path(&launch.state);
        let sink=Arc::clone(&launch.sink);
        let reader = std::thread::Builder::new().name("richos-codex-watch".into()).spawn(move || {
            let mut said=std::collections::BTreeMap::new();
            for line in BufReader::new(output).lines() {
                let result=line.map_err(|e|e.to_string()).and_then(|line| {
                    let value:serde_json::Value=serde_json::from_str(&line).map_err(|e|e.to_string())?;
                    if matches!(value["type"].as_str(),Some("channel"|"setup")) {
                        let key=value["locator"].as_str().unwrap_or("setup").to_string();
                        match sink(&line) {
                            Ok(())=>{said.remove(&key);},
                            Err(error)=>{if said.get(&key)!=Some(&error) {eprintln!("[richos] channel {key} waits: {error}");said.insert(key,error);}}
                        }
                        Ok(())
                    } else {
                        let text=value["text"].as_str().ok_or("Watcher line has no text")?;
                        keep(&pending,text).map_err(|e|format!("legacy spool write failed; source remains available: {e}"))
                    }
                });
                if let Err(error)=result {eprintln!("[richos] Codex watch: {error}");}
            }
        })?;
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        crate::review_watch::write_record(&launch.record_path(), &serde_json::json!({
            "pid": pid, "started_at": started, "channels": files}))?;
        Ok(CodexWatch { child: Mutex::new(owned), pid, record: launch.record_path(), channels: launch.channels.clone(),
            reader: Mutex::new(Some(reader)) })
    }

    fn running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// SIGTERM, at most [`STOP_BOUND`], then the group SIGKILL; reaped either way, and the
    /// record removed. The reader drains already emitted legacy observations;
    /// framed input remains in its source journal until inbox acceptance.
    fn stop(&self) {
        let reaped = match self.child.lock().unwrap().wait() {
            Ok(_) => true,
            Err(e) => {
                eprintln!("[richos] Codex watch: watcher {} could not be reaped ({e})", self.pid);
                false
            }
        };
        // Its pipe is closed once it is gone; one not reaped may still hold it open.
        if let Some(reader) = self.reader.lock().unwrap().take().filter(|_| reaped) {
            drop(reader.join());
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

    /// The process has one watcher ([`RUNNING`]): the tests that start it take turns.
    static ONE_WATCHER: Mutex<()> = Mutex::new(());

    /// Waits for `done`; `secs` only catches a hang, it is not the clock the test waits on.
    fn until(secs: u64, done: impl Fn() -> bool) -> bool {
        let began = std::time::Instant::now();
        while !done() && began.elapsed() < Duration::from_secs(secs) {
            std::thread::sleep(Duration::from_millis(50));
        }
        done()
    }

    fn watcher_environment(root:&Path) -> std::collections::BTreeMap<String, String> {
        // A look every half second here, so the test waits on the watcher, not its clock.
        let mut environment = crate::review_watch::environment("/usr/bin:/bin:/usr/sbin:/sbin");
        environment.insert("CODEX_WATCH_POLL_SECONDS".into(), "0.5".into());
        environment.insert("CODEX_COORDINATION_ROOT".into(),root.join("coord").to_string_lossy().into_owned());
        environment
    }

    #[test]
    fn legacy_spool_survives_quit_but_never_routes_to_an_arbitrary_open_chat() {
        let _one=ONE_WATCHER.lock().unwrap_or_else(|p|p.into_inner());
        let root=scratch("legacy");
        let channel=root.join("legacy");let state=root.join("state");
        std::fs::create_dir_all(&channel).unwrap();
        std::fs::write(channel.join(TO_RICH),"old backlog\n").unwrap();
        let launch=Launch {python:python(),engine:engine(),state:state.clone(),environment:watcher_environment(&root),
            channels:vec![channel.clone()],sink:Arc::new(|line| {
                let event:serde_json::Value=serde_json::from_str(line).unwrap();
                assert_eq!(event["type"],"setup","unframed task reached a visible chat");Ok(())
            })};
        ensure(&launch).unwrap();
        let offsets=state_dir(&state).join("watch/app-host/offsets.json");
        assert!(until(60,||offsets.exists()),"watcher did not initialize");
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(channel.join(TO_RICH)).unwrap()
            .write_all(b"## first\nfirst task\n## second\nsecond task\n").unwrap();
        assert!(until(60,||std::fs::read_to_string(pending_path(&state)).is_ok_and(|s|s.lines().count()==2)),"pending legacy input was not kept");
        let pid=stop_all().unwrap();assert!(!alive(pid));
        let preserved=std::fs::read(pending_path(&state)).unwrap();
        ensure(&launch).unwrap();
        assert!(until(60,||offsets.exists()));
        stop_all();
        assert_eq!(std::fs::read(pending_path(&state)).unwrap(),preserved);
        std::fs::remove_dir_all(root).unwrap();
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

    #[test]
    fn framed_channels_reach_only_their_ledger_owners_and_replay_is_idempotent() {
        let _one=ONE_WATCHER.lock().unwrap_or_else(|p|p.into_inner());
        let root=scratch("framed");let coord=root.join("coord");let state=root.join("state");
        let mut ledger=crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
        let a=ledger.create_thread("Same title",&crate::EntityId::parse("company-a").unwrap()).unwrap();
        let b=ledger.create_thread("Same title",&crate::EntityId::parse("company-b").unwrap()).unwrap();
        let bindings=vec![ledger.thread_binding(&a).unwrap(),ledger.thread_binding(&b).unwrap()];
        let helper=|args:Vec<String>|->serde_json::Value {
            let out=std::process::Command::new(python()).arg("-B").arg(engine().join("scripts/lib/codex_channels.py"))
                .args(args).env("CODEX_COORDINATION_ROOT",&coord).output().unwrap();
            assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
            serde_json::from_slice(&out.stdout).unwrap()
        };
        let mut locators=Vec::new();
        for (i,binding) in [bindings[0].clone(),bindings[0].clone(),bindings[1].clone()].iter().enumerate() {
            let scope=root.join(format!("scope-{i}.json"));
            std::fs::write(&scope,serde_json::json!({"version":1,"state_root":state,"entity_id":binding.entity_id(),"thread_id":binding.thread_id()}).to_string()).unwrap();
            let result=helper(vec!["connect".into(),"--scope".into(),scope.to_string_lossy().into_owned(),"--text".into(),"Original task".into(),"--provenance".into(),"fixture source".into()]);
            let locator=result["locator"].as_str().unwrap().to_string();
            helper(vec!["send".into(),"--locator".into(),locator.clone(),"--id".into(),format!("message-{i}"),"--text".into(),format!("Message {i}")]);
            locators.push(locator);
        }
        let (out,heard)=mpsc::channel();let out=Mutex::new(out);
        let (sink_state,sink_coord,sink_bindings)=(state.clone(),coord.clone(),bindings.clone());
        let launch=Launch {python:python(),engine:engine(),state:state.clone(),environment:watcher_environment(&root),
            channels:vec![root.join("unused-legacy")],sink:Arc::new(move |line| {
                let e:serde_json::Value=serde_json::from_str(line).map_err(|e|e.to_string())?;
                let binding=sink_bindings.iter().find(|b|Some(b.thread_id())==e["connection"]["thread_id"].as_str()).ok_or("missing ledger destination")?;
                crate::codex_bridge::accept(&sink_state,binding,&e,&sink_coord)?;
                out.lock().unwrap().send(()).unwrap();Ok(())
            })};
        let pid=ensure(&launch).unwrap().unwrap();assert_eq!(ensure(&launch).unwrap(),Some(pid));
        for _ in 0..3 {heard.recv_timeout(Duration::from_secs(60)).expect("channel did not reach its owning inbox");}
        stop_all();
        for (binding,count) in [(&bindings[0],2),(&bindings[1],1)] {
            let scope=crate::status_tools::StatusToolScope {version:1,state_root:state.clone(),entity_id:binding.entity_id().to_string(),thread_id:binding.thread_id().into()};
            let path=crate::session_tools::inbox(&scope,"stu").unwrap();
            let rows=std::fs::read_to_string(&path).unwrap();
            assert_eq!(rows.lines().count(),count);
            for row in rows.lines() {let m:crate::session_tools::Message=serde_json::from_str(row).unwrap();assert!(m.bridge.is_some());assert!(m.instruction.is_none());assert!(!m.handoff);}
            crate::session_tools::acknowledge(&scope,"stu",rows.len() as u64).unwrap();
        }
        // Lose every advisory position after acceptance, including acknowledged history.
        std::fs::remove_dir_all(state.join("codex-watch/accepted")).unwrap();
        ensure(&launch).unwrap();
        for _ in 0..3 {heard.recv_timeout(Duration::from_secs(60)).expect("replay did not reconcile");}
        stop_all();
        for (binding,count) in [(&bindings[0],2),(&bindings[1],1)] {
            let scope=crate::status_tools::StatusToolScope {version:1,state_root:state.clone(),entity_id:binding.entity_id().to_string(),thread_id:binding.thread_id().into()};
            assert_eq!(std::fs::read_to_string(crate::session_tools::inbox(&scope,"stu").unwrap()).unwrap().lines().count(),count);
        }
        assert!(!alive(pid));assert!(!launch.record_path().exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
