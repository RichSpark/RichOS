//! **THE NEXT macOS APP MANAGEMENT NOTICE COMES WITH ITS OWN EVIDENCE** (follow-ups,
//! 2026-10-28: "'RichOS.app' was prevented from modifying apps on your Mac", trigger unknown).
//!
//! The unified log keeps the kernel's `deny(1) file-write…` lines and tccd's attribution of an
//! App Management request for a short while only, so the evidence behind a notice was gone by the
//! time anyone looked. The app therefore runs the engine's read-only watcher,
//! `scripts/lib/app_management_watch.py`, as its own child from launch to quit, and the watcher
//! appends each such line, with full paths, to `<app data>/engine-state/app-management/
//! ledger.jsonl`.
//!
//! * **Read-only.** It runs `log stream` and writes only its ledger. RichOS needs no App
//!   Management permission; nothing here asks for one or works around the block.
//! * **Nearly free while idle.** The watcher blocks on the log stream (logd filters by
//!   predicate) and on a kqueue for this process's exit; it has no polling loop.
//! * **Its process id is recorded** (`<state>/app-management/host-child.json`) while it runs, and
//!   the record goes once it has been stopped and reaped. A watcher whose app died without a quit
//!   exits on its own (it is given this process's pid, [`HOST_ENV`]).
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use crate::owned_process::{OwnedChild, SupervisedSet};

/// The bound the watcher gets after SIGTERM; it starts only `log stream`, so it ends at once.
pub const STOP_BOUND: std::time::Duration = crate::review_watch::STOP_BOUND;

/// Set in the child's environment: this host's own process id.
pub const HOST_ENV: &str = "RICHOS_WATCH_HOST";

/// The watcher's own folder under the engine state, and the ledger's name in it.
pub const DIR: &str = "app-management";
pub const LEDGER: &str = "ledger.jsonl";

/// Its fence, apart from the leases' set and the other watchers'.
static WATCHERS: SupervisedSet = SupervisedSet::new();

/// Everything the child needs. `state` is `<app data>/engine-state`.
#[derive(Clone)]
pub struct Launch {
    pub python: PathBuf,
    pub engine: PathBuf,
    pub state: PathBuf,
    /// The child's whole environment (it inherits nothing else).
    pub environment: BTreeMap<String, String>,
}

impl Launch {
    pub fn record_path(&self) -> PathBuf {
        state_dir(&self.state).join("host-child.json")
    }
    pub fn log_path(&self) -> PathBuf {
        state_dir(&self.state).join("host-child.log")
    }
    pub fn ledger_path(&self) -> PathBuf {
        state_dir(&self.state).join(LEDGER)
    }
}

/// Where the ledger, the child's record and its log live.
pub fn state_dir(state: &Path) -> PathBuf {
    state.join(DIR)
}

struct Watch {
    child: Mutex<OwnedChild>,
    pid: u32,
    record: PathBuf,
}

impl Watch {
    fn start(launch: &Launch) -> io::Result<Self> {
        let script = launch.engine.join("scripts/lib/app_management_watch.py");
        if !script.is_file() {
            return Err(io::Error::other(format!("this engine has no {}", script.display())));
        }
        std::fs::create_dir_all(state_dir(&launch.state))?;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(launch.log_path())?;
        let mut command = crate::runtime::interpreter_command(&launch.python);
        command.arg(&script).arg("--state").arg(&launch.state);
        command.env_clear().envs(&launch.environment)
            .env("PYTHONDONTWRITEBYTECODE", "1").env("PYTHONNOUSERSITE", "1")
            .env(HOST_ENV, std::process::id().to_string());
        command.current_dir(&launch.state).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(log));
        OwnedChild::configure(&mut command);
        let child = command.spawn()?;
        let pid = child.id();
        let owned = OwnedChild::supervised_in(child, STOP_BOUND, &WATCHERS);
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        crate::review_watch::write_record(&launch.record_path(), &serde_json::json!({
            "pid": pid, "started_at": started, "ledger": launch.ledger_path()}))?;
        Ok(Watch { child: Mutex::new(owned), pid, record: launch.record_path() })
    }

    fn running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// SIGTERM, at most [`STOP_BOUND`], then the group SIGKILL; reaped either way, and the record
    /// removed.
    fn stop(&self) {
        if let Err(e) = self.child.lock().unwrap().wait() {
            eprintln!("[richos] App Management watch: watcher {} could not be reaped ({e})", self.pid);
        }
        match std::fs::remove_file(&self.record) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                eprintln!("[richos] App Management watch: {} could not be removed ({e})", self.record.display());
            }
            _ => {}
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The process's own watcher, started at launch and ended at quit.
static RUNNING: Mutex<Option<Watch>> = Mutex::new(None);

/// **Start the watcher unless it already runs** (the shell calls it at launch and again when a
/// session opens, which covers an engine set up after launch). Its pid.
pub fn ensure(launch: &Launch) -> io::Result<u32> {
    let mut running = RUNNING.lock().unwrap();
    if let Some(existing) = running.as_ref() {
        if existing.running() {
            return Ok(existing.pid);
        }
    }
    drop(running.take());
    let started = Watch::start(launch)?;
    let pid = started.pid;
    *running = Some(started);
    Ok(pid)
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

    /// **A KERNEL WRITE DENIAL REACHES THE LEDGER, AND THE WATCHER ENDS AT QUIT.** The real
    /// engine watcher as the app's child, reading a fixture stream (a fifo named by
    /// `APP_MANAGEMENT_WATCH_INPUT`, in place of `log stream`): the line is recorded with its
    /// full path, a second `ensure` does not start a second watcher, and quit ends it and
    /// removes its record.
    #[test]
    fn a_kernel_write_denial_reaches_the_ledger_and_the_watcher_ends_at_quit() {
        let root = std::env::temp_dir().join(format!("app-management-watch-{}", uuid::Uuid::new_v4()));
        let state = root.join("engine-state");
        std::fs::create_dir_all(&state).unwrap();
        let fifo = root.join("events");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "mkfifo");
        let mut environment = crate::review_watch::environment("/usr/bin:/bin:/usr/sbin:/sbin");
        environment.insert("APP_MANAGEMENT_WATCH_INPUT".into(), fifo.to_string_lossy().into_owned());
        let launch = Launch { python: python(), engine: engine(), state: state.clone(), environment };

        let pid = ensure(&launch).unwrap();
        assert_eq!(ensure(&launch).unwrap(), pid, "a second call started a second watcher");
        let record: serde_json::Value = serde_json::from_slice(&std::fs::read(launch.record_path()).unwrap()).unwrap();
        assert_eq!(record["pid"], pid);

        use std::io::Write;
        let mut writer = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();
        writer.write_all(concat!(r#"{"timestamp":"2026-10-28 12:16:53.1+0000","processImagePath":"/kernel","processID":0,"#,
            r#""eventMessage":"System Policy: python3(9120) deny(1) file-write-create /Users/fixture/Library/Application Support/com.richos.app/engine-state/provider-leases/.tmp-1.json"}"#, "\n").as_bytes()).unwrap();
        writer.flush().unwrap();
        let ledger = launch.ledger_path();
        let began = std::time::Instant::now();
        // load-bound: hang guard only (the watcher writes the line as soon as it reads it).
        while !ledger.is_file() && began.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let text = std::fs::read_to_string(&ledger)
            .unwrap_or_else(|_| panic!("nothing reached the ledger: {}", std::fs::read_to_string(launch.log_path()).unwrap_or_default()));
        let line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(line["kind"], "kernel-deny");
        assert_eq!(line["paths"][0],
                   "/Users/fixture/Library/Application Support/com.richos.app/engine-state/provider-leases/.tmp-1.json",
                   "the full path, spaces kept: {text}");

        assert_eq!(stop_all(), Some(pid));
        drop(writer);
        assert!(!alive(pid), "the recorded pid {pid} is still running after quit");
        assert!(!launch.record_path().exists(), "the record outlived the child");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
