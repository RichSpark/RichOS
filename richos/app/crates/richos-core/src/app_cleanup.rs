//! **THE APP CLEANS UP ITS OWN GARBAGE, NO MATTER WHAT** (CEO §54 addendum 2, 2026-09-18:
//! *"The RichOS app must always clean up its garbage automatically, no matter what."*).
//!
//! The deciding and the deleting are the engine's proven reaper, never a second sweeper here:
//! `scripts/scratch-reaper.sh --apply --app <data folder>` (`scratch-reaper.py` `scan_app`), with
//! its liveness proof, its four walls, its open-file check and its deletion log. This module only
//! says WHEN: at start (what an earlier run left, so a crash or a force-quit is covered by the
//! next start), when the work host has nothing left to do (a job settled), and at quit.
//!
//! **Only the app's places.** The app scope plans what the app itself left behind and nothing
//! else; the machine's other arms (Codex worktrees, Docker, another program's temporary files)
//! stay the scheduled job's. An engine older than this app refuses `--app` outright (exit 2), so
//! the app deletes nothing rather than running the whole machine's sweep. That is why the app
//! calls `scratch-reaper.sh`, which hands an argument it does not know to the reaper, and not
//! `scratch-sweep.sh`, which ignores one.
//!
//! **One at a time, and bounded.** The three moments can overlap (a settle as he quits), so
//! calls are serialized in this process; each is ended after [`BOUND`], and what it did not get
//! to is taken by the next one.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long one sweep may take before it is ended. Measured in the test VM: an app sweep with
/// nothing to do takes about a second (Python start, the process table, one `lsof` per
/// candidate); the quit path waits for it, so the bound is what a quit can be held up by.
pub const BOUND: Duration = Duration::from_secs(10);

/// When a sweep runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    Start,
    Settled,
    Quit,
}

impl Moment {
    pub fn as_str(self) -> &'static str {
        match self {
            Moment::Start => "start",
            Moment::Settled => "settled",
            Moment::Quit => "quit",
        }
    }
}

/// What one sweep did, read from the reaper's own `applied:` and `verdict:` lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub exit: Option<i32>,
    pub deleted: u64,
    pub failures: u64,
    pub undecidable: u64,
    /// The reaper's two lines, as they were printed.
    pub summary: String,
}

/// **Who made a per-lease folder** (an engine profile): written into it when it is made, read by
/// the engine's app sweep. A folder whose app process has ended, and that no running process
/// names on its command line, is garbage; one whose app is still running is that app's to retire.
pub const OWNER_FILE: &str = ".richos-owner.json";

/// This process's start as the engine reads a start (`workspaces.py` `process_start`: `ps -o
/// lstart=` under `LC_ALL=C TZ=UTC0`), so the two compare as text. Read once.
pub fn own_start() -> Option<String> {
    static START: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    START.get_or_init(|| {
        let out = Command::new("ps").args(["-o", "lstart=", "-p", &std::process::id().to_string()])
            .env("LC_ALL", "C").env("LANG", "C").env("TZ", "UTC0").output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).split_whitespace().collect::<Vec<_>>().join(" ");
        (out.status.success() && !text.is_empty()).then_some(text)
    }).clone()
}

/// Write [`OWNER_FILE`] into `folder`: this process and its start.
pub fn write_owner(folder: &Path) -> std::io::Result<()> {
    let start = own_start().ok_or_else(|| std::io::Error::other("this process's start could not be read"))?;
    std::fs::write(folder.join(OWNER_FILE),
                   serde_json::json!({"pid": std::process::id(), "procStart": start}).to_string())
}

/// What the work host calls when Stu's jobs have settled: the shell's sweep over the engine in
/// use at that moment (first-run setup can install one after launch). It must not block.
pub type Hook = Arc<dyn Fn(Moment) + Send + Sync>;

/// Serializes this process's sweeps.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The app's sweeper: the verified engine, the runtime's PATH and the app's data folder.
#[derive(Clone, Debug)]
pub struct Sweeper {
    engine: PathBuf,
    data_dir: PathBuf,
    /// The child's whole environment: the runtime's PATH and the user's own HOME, TMPDIR,
    /// locale and Claude configuration folder (`review_watch::environment`), nothing else.
    env: std::collections::BTreeMap<String, String>,
}

impl Sweeper {
    pub fn new(engine: &Path, runtime_path: &str, data_dir: &Path) -> Self {
        Self { engine: engine.to_path_buf(), data_dir: data_dir.to_path_buf(),
               env: crate::review_watch::environment(runtime_path) }
    }

    /// The sweeper over `engine` once it verifies (its delivered runtime's PATH); `None` before
    /// an engine is installed, when there is nothing to sweep with and the next moment tries again.
    pub fn for_engine(engine: &Path, data_dir: &Path) -> Option<Self> {
        let runtime = crate::runtime::verify_engine(engine).ok()?;
        Some(Self::new(engine, &runtime.path(), data_dir))
    }

    /// A sweeper whose child sees `env` instead: a test's sandbox HOME and reaper config.
    #[cfg(test)]
    pub(crate) fn with_env(mut self, env: &[(&str, &Path)]) -> Self {
        for (key, value) in env {
            self.env.insert((*key).to_string(), value.to_string_lossy().into_owned());
        }
        self
    }

    /// The invocation, byte-exact: the engine's reaper, deleting, scoped to this app. At quit
    /// it also names this process as the one quitting, so what this run made and nothing
    /// running uses goes now rather than at the next start.
    pub fn command(&self, moment: Moment) -> Command {
        let mut command = Command::new("/bin/bash");
        command
            .arg(self.engine.join("scripts/scratch-reaper.sh"))
            .args(["--apply", "--app"])
            .arg(&self.data_dir);
        if moment == Moment::Quit {
            command.arg("--app-exiting").arg(std::process::id().to_string());
        }
        command
            .env_clear()
            .envs(&self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    /// Sweep now, waiting at most [`BOUND`]; one line to the app's log either way.
    pub fn run(&self, moment: Moment) -> Result<Outcome, String> {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = self.run_bounded(moment, BOUND);
        match &result {
            Ok(outcome) => eprintln!("[richos] cleanup at {}: deleted {}, undecidable {}, failures {} ({})",
                                     moment.as_str(), outcome.deleted, outcome.undecidable, outcome.failures,
                                     outcome.summary),
            Err(error) => eprintln!("[richos] cleanup at {}: not done, the next sweep takes it ({error})", moment.as_str()),
        }
        result
    }

    /// [`Self::run`] on its own thread, for the moments that must not wait for it.
    pub fn spawn(self: &Arc<Self>, moment: Moment) {
        let sweeper = Arc::clone(self);
        if let Err(error) = std::thread::Builder::new().name("richos-cleanup".into())
            .spawn(move || { sweeper.run(moment).ok(); }) {
            eprintln!("[richos] cleanup at {}: could not start ({error})", moment.as_str());
        }
    }

    fn run_bounded(&self, moment: Moment, bound: Duration) -> Result<Outcome, String> {
        let mut command = self.command(moment);
        crate::owned_process::OwnedChild::configure(&mut command);
        let mut child = command.spawn().map_err(|e| format!("the engine's reaper could not start: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("no output from the reaper")?;
        let mut stderr = child.stderr.take().ok_or("no error output from the reaper")?;
        let mut child = crate::owned_process::OwnedChild::new(child);
        let out = std::thread::spawn(move || { let mut s = String::new(); stdout.read_to_string(&mut s).ok(); s });
        let err = std::thread::spawn(move || { let mut s = String::new(); stderr.read_to_string(&mut s).ok(); s });
        let deadline = Instant::now() + bound;
        let status = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    child.kill().ok();
                    child.wait().ok();
                    return Err(format!("it was still running after {} s and was ended", bound.as_secs()));
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        let out = out.join().unwrap_or_default();
        let err = err.join().unwrap_or_default();
        let exit = status.code();
        if exit == Some(2) {
            return Err(format!("the engine refused the app's sweep: {}", err.trim().lines().last().unwrap_or("")));
        }
        Ok(parse(&out, exit))
    }
}

/// The shell's hook: each call sweeps, on its own thread, with the engine in `engine` AT THAT
/// MOMENT, so an engine first-run setup installs after launch is used from then on.
pub fn hook(engine: Arc<Mutex<PathBuf>>, data_dir: PathBuf) -> Hook {
    Arc::new(move |moment| {
        let dir = engine.lock().map(|d| d.clone()).unwrap_or_default();
        match Sweeper::for_engine(&dir, &data_dir) {
            Some(sweeper) => Arc::new(sweeper).spawn(moment),
            None => eprintln!("[richos] cleanup at {}: no verified engine yet, the next sweep takes it", moment.as_str()),
        }
    })
}

/// The reaper's `verdict:` and `applied:` lines, read rather than recomputed.
pub fn parse(out: &str, exit: Option<i32>) -> Outcome {
    let field = |line: &str, key: &str| -> u64 {
        line.split_whitespace().find_map(|w| w.strip_prefix(key)).and_then(|v| v.parse().ok()).unwrap_or(0)
    };
    let verdict = out.lines().rev().find(|l| l.starts_with("verdict:")).unwrap_or("");
    let applied = out.lines().rev().find(|l| l.starts_with("applied:")).unwrap_or("");
    Outcome {
        exit,
        deleted: field(applied, "deleted="),
        failures: field(applied, "failures="),
        undecidable: field(verdict, "undecidable="),
        summary: [verdict, applied].iter().filter(|l| !l.is_empty()).cloned().collect::<Vec<_>>().join("; "),
    }
}

/// **A sandbox for the app's sweep, with the REAL repository engine**: its own HOME, Claude
/// configuration folder, TMPDIR and a literal reaper config naming a sandbox Claude scratch root.
/// Liveness is real: a session process is `/bin/sleep` started under a name no other process on
/// this Mac has, declared as the session process name, with the session file Claude Code writes
/// for it. Only processes the sandbox started are ended, by the handles it holds.
#[cfg(test)]
pub(crate) mod sandbox {
    use super::*;
    use std::process::Child;

    pub(crate) struct Sandbox {
        pub root: PathBuf,
        pub data: PathBuf,
        home: PathBuf,
        slug: PathBuf,
        name: String,
        config: PathBuf,
        children: Vec<Child>,
    }

    pub(crate) fn engine() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine")
    }

    impl Sandbox {
        /// `data`: the app's data folder (it holds `engine-state`); created inside the sandbox when `None`.
        pub fn new(data: Option<&Path>) -> Self {
            let root = std::env::temp_dir().join(format!("richos-app-cleanup-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            // The reaper logs real paths (/private/var/..., not /var/...).
            let root = root.canonicalize().unwrap();
            let data = data.map(|d| d.canonicalize().unwrap());
            let home = root.join("home");
            std::fs::create_dir_all(home.join(".claude/sessions")).unwrap();
            std::fs::create_dir_all(root.join("tmp")).unwrap();
            let claude_root = root.join("claude-root");
            let slug = claude_root.join("-Users-x-Library-Application-Support-com-richos-app-coordination");
            std::fs::create_dir_all(&slug).unwrap();
            let data = data.unwrap_or_else(|| home.join("Library/Application Support/com.richos.app"));
            std::fs::create_dir_all(data.join("engine-state/evidence")).unwrap();
            let name = format!("rapp{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
            let config = root.join("orchestration.config");
            std::fs::write(&config, format!(
                "SCRATCH_CLAUDE_ROOTS=\"{}\"\nSCRATCH_TMP_PATTERNS=\"none-*\"\nSCRATCH_AGE_FLOOR_MINUTES=\"60\"\n\
                 SCRATCH_NIGHTLY_DIR=\"{}\"\nSCRATCH_NIGHTLY_KEEP=\"3\"\nSCRATCH_NOTICE_BYTES=\"1\"\n\
                 SCRATCH_SESSION_PROCESS_NAMES=\"{name}\"\n",
                claude_root.display(), root.join("nightly").display())).unwrap();
            Self { root, data, home, slug, name, config, children: vec![] }
        }

        /// A session the app hosted (its binding record) and Claude Code's scratch for it.
        pub fn hosted(&self) -> (String, PathBuf) {
            let session = uuid::Uuid::new_v4().to_string();
            let record = self.data.join("engine-state/evidence").join(&session);
            std::fs::create_dir_all(&record).unwrap();
            std::fs::write(record.join("binding.json"), format!("{{\"session_id\":\"{session}\",\"role\":\"stu\"}}")).unwrap();
            let scratch = self.slug.join(&session);
            std::fs::create_dir_all(scratch.join("tasks")).unwrap();
            std::fs::create_dir_all(scratch.join("scratchpad")).unwrap();
            std::fs::write(scratch.join("tasks/b1.output"), "background command output\n".repeat(100)).unwrap();
            (session, scratch)
        }

        /// Claude scratch of a session the app did NOT host (a terminal's).
        pub fn not_the_apps(&self) -> PathBuf {
            let scratch = self.slug.join(uuid::Uuid::new_v4().to_string());
            std::fs::create_dir_all(scratch.join("tasks")).unwrap();
            std::fs::write(scratch.join("tasks/b1.output"), "a terminal session's output\n").unwrap();
            scratch
        }

        /// A lease's two engine-profile folders. `ours`: made by this process (the real owner
        /// record); otherwise by an app that has ended (an ended process's pid and start).
        pub fn profile(&mut self, ours: bool) -> (PathBuf, PathBuf) {
            let lease = uuid::Uuid::new_v4().to_string();
            let base = self.data.join("engine-profiles").join(&lease);
            std::fs::create_dir_all(base.join("agents")).unwrap();
            let standing = base.with_file_name(format!("{lease}-standing"));
            std::fs::create_dir_all(standing.join("agents")).unwrap();
            // In both folders, as engine_profile.rs writes them.
            if ours {
                write_owner(&base).unwrap();
                write_owner(&standing).unwrap();
            } else {
                let mut ended = Command::new("/bin/sleep").arg("600").spawn().unwrap();
                let start = Command::new("ps").args(["-o", "lstart=", "-p", &ended.id().to_string()])
                    .env("LC_ALL", "C").env("LANG", "C").env("TZ", "UTC0").output().unwrap();
                let start = String::from_utf8_lossy(&start.stdout).trim().to_string();
                ended.kill().ok();
                ended.wait().ok();
                for folder in [&base, &standing] {
                    std::fs::write(folder.join(OWNER_FILE),
                        serde_json::json!({"pid": ended.id(), "procStart": start}).to_string()).unwrap();
                }
            }
            (base, standing)
        }

        /// `session` is running: a session process with its session file. Returns its index.
        pub fn live(&mut self, session: &str) -> usize {
            use std::os::unix::process::CommandExt;
            let child = Command::new("/bin/sleep").arg0(&self.name).arg("600").spawn().unwrap();
            let start = Command::new("ps").args(["-o", "lstart=", "-p", &child.id().to_string()])
                .env("LC_ALL", "C").env("LANG", "C").env("TZ", "UTC0").output().unwrap();
            let start = String::from_utf8_lossy(&start.stdout).trim().to_string();
            std::fs::write(self.home.join(format!(".claude/sessions/{}.json", child.id())),
                serde_json::json!({"pid": child.id(), "sessionId": session, "procStart": start}).to_string()).unwrap();
            self.children.push(child);
            self.children.len() - 1
        }

        /// A process holds `file` open (as its standard input). Returns its index.
        pub fn hold(&mut self, file: &Path) -> usize {
            let held = std::fs::File::open(file).unwrap();
            // `spawn` returns once the child has exec'd, so it already holds the file.
            self.children.push(Command::new("/bin/sleep").arg("600").stdin(held).spawn().unwrap());
            self.children.len() - 1
        }

        pub fn end(&mut self, index: usize) {
            self.children[index].kill().ok();
            self.children[index].wait().ok();
        }

        /// The app's sweeper over the repository engine, seeing only this sandbox.
        pub fn sweeper(&self) -> Sweeper {
            Sweeper::new(&engine(), &std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()), &self.data)
                .with_env(&[("HOME", &self.home), ("CLAUDE_CONFIG_DIR", &self.home.join(".claude")),
                            ("TMPDIR", &self.root.join("tmp")), ("SCRATCH_REAPER_CONFIG", &self.config)])
        }

        /// The deletion log the reaper writes, in the sandbox's Claude configuration folder.
        pub fn log(&self) -> String {
            std::fs::read_to_string(self.home.join(".claude/state/scratch-reaper.log")).unwrap_or_default()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            for child in &mut self.children {
                child.kill().ok();
                child.wait().ok();
            }
            std::fs::remove_dir_all(&self.root).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::sandbox::{engine, Sandbox};

    /// **Start and quit, through the app's own call, with the real engine** (§54 addendum 2):
    /// what an earlier run left goes at start; a live session's scratch and one a process holds
    /// open stay, as does a terminal session's; once the live one has ended, quit takes it.
    #[test]
    fn start_and_quit_remove_the_apps_leftovers_and_never_a_live_held_or_foreign_one() {
        let mut s = Sandbox::new(None);
        let (_, left) = s.hosted();
        let (live, running) = s.hosted();
        let process = s.live(&live);
        let (_, open) = s.hosted();
        s.hold(&open.join("tasks/b1.output"));
        let foreign = s.not_the_apps();
        let (crashed, crashed_standing) = s.profile(false);
        let (mine, mine_standing) = s.profile(true);

        let start = s.sweeper().run(Moment::Start).expect("the start sweep ran");
        assert!(!left.exists(), "what an earlier run left is gone at start: {start:?}");
        assert!(!crashed.exists() && !crashed_standing.exists(), "an ended app's profile is gone at start: {start:?}");
        assert!(running.exists(), "a live session's scratch is never touched: {start:?}");
        assert!(open.exists(), "a directory a process holds open is never touched: {start:?}");
        assert!(foreign.exists(), "a session the app did not host is not the app's: {start:?}");
        assert!(mine.exists() && mine_standing.exists(), "a running app's profile is that app's to retire: {start:?}");
        assert_eq!((start.deleted, start.failures), (3, 0), "{start:?}");
        assert!(s.log().contains(&format!("DELETED {}", left.display())), "every deletion is recorded: {}", s.log());
        assert!(s.log().contains(&format!("DELETED {}", crashed.display())), "{}", s.log());

        s.end(process);
        let quit = s.sweeper().run(Moment::Quit).expect("the quit sweep ran");
        assert!(!running.exists(), "a session that ended is gone at quit: {quit:?}");
        assert!(!mine.exists() && !mine_standing.exists(), "the quitting app's own profile is gone at quit: {quit:?}");
        assert!(open.exists() && foreign.exists(), "{quit:?}");
    }

    #[test]
    fn the_invocation_is_the_engines_reaper_deleting_scoped_to_the_app() {
        let s = Sweeper::new(Path::new("/E"), "/R/bin", Path::new("/D/com.richos.app"));
        let args = |c: &Command| c.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>();
        for moment in [Moment::Start, Moment::Settled] {
            let c = s.command(moment);
            assert_eq!(c.get_program(), "/bin/bash");
            assert_eq!(args(&c), ["/E/scripts/scratch-reaper.sh", "--apply", "--app", "/D/com.richos.app"]);
            assert!(c.get_envs().any(|(k, v)| k == "PATH" && v == Some("/R/bin".as_ref())));
        }
        let me = std::process::id().to_string();
        assert_eq!(args(&s.command(Moment::Quit)),
                   ["/E/scripts/scratch-reaper.sh", "--apply", "--app", "/D/com.richos.app", "--app-exiting", me.as_str()]);
    }

    #[test]
    fn the_reapers_own_lines_are_what_is_reported() {
        let out = "DELETE 1.0 KB /x\nverdict: decided deletable=1 reclaimable=1.0 KB kept=2 undecidable=1 skipped=0 \
                   skipped_bytes=0 B scope=app unattributed=3\napplied: deleted=1 freed=1.0 KB failures=0 log=/l\n";
        let o = parse(out, Some(3));
        assert_eq!((o.deleted, o.failures, o.undecidable, o.exit), (1, 0, 1, Some(3)));
        assert!(o.summary.contains("scope=app") && o.summary.contains("applied: deleted=1"), "{}", o.summary);
    }

    #[test]
    fn an_engine_without_the_app_scope_deletes_nothing() {
        let root = std::env::temp_dir().join(format!("richos-app-cleanup-old-{}", uuid::Uuid::new_v4()));
        let engine = root.join("engine");
        std::fs::create_dir_all(engine.join("scripts")).unwrap();
        // The shape of an older reaper: an argument parser that refuses what it does not know.
        std::fs::write(engine.join("scripts/scratch-reaper.sh"),
            "for a in \"$@\"; do [ \"$a\" = --app ] && { echo 'error: unrecognized arguments: --app' >&2; exit 2; }; done\n\
             echo 'applied: deleted=99 freed=1 B failures=0 log=/x'\n").unwrap();
        let s = Sweeper::new(&engine, "/usr/bin:/bin", &root);
        let refused = s.run(Moment::Start).expect_err("an older engine refuses the app's sweep");
        assert!(refused.contains("refused"), "{refused}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_sweep_that_does_not_end_is_ended_at_its_bound() {
        let root = std::env::temp_dir().join(format!("richos-app-cleanup-slow-{}", uuid::Uuid::new_v4()));
        let engine = root.join("engine");
        std::fs::create_dir_all(engine.join("scripts")).unwrap();
        std::fs::write(engine.join("scripts/scratch-reaper.sh"), "exec sleep 30\n").unwrap();
        let s = Sweeper::new(&engine, "/usr/bin:/bin", &root);
        // A 30 s sleep that finished on its own would be Ok; only the bound makes this an error.
        let ended = s.run_bounded(Moment::Start, Duration::from_millis(300)).expect_err("bounded");
        assert!(ended.contains("ended"), "{ended}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_hook_waits_for_a_verified_engine_and_deletes_nothing_without_one() {
        let s = Sandbox::new(None);
        let (_, left) = s.hosted();
        assert!(Sweeper::for_engine(&s.root.join("no-engine"), &s.data).is_none());
        // With no verified engine the hook starts no sweep at all, so this is already final.
        let hook = hook(Arc::new(Mutex::new(s.root.join("no-engine"))), s.data.clone());
        hook(Moment::Settled);
        assert!(left.exists());
    }

    #[test]
    fn the_repository_engine_has_the_app_scope() {
        let reaper = std::fs::read_to_string(engine().join("scripts/lib/scratch-reaper.py")).unwrap();
        assert!(reaper.contains("ap.add_argument(\"--app\""), "the engine the app ships must carry scan_app");
    }
}
