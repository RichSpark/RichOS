//! **The user's terminal environment, for every lease** (build plan 2026-10-21, item 4).
//!
//! An app opened from the Finder gets launchd's environment, not the one the user's terminal
//! has: no `JAVA_HOME`, no `ANDROID_HOME`, a `PATH` of the system folders only. So the app
//! reads the login shell's environment once, `$SHELL -l -i -c`, bounded by [`TIMEOUT`], and
//! every lease and hook gets it (`EngineProfile::configure`). It is never logged and never
//! written to disk.
//!
//! The design is VS Code's `resolveShellEnv` (MIT), copied rather than adopted: run the
//! user's own shell as a login, interactive shell, print the environment between two marks,
//! and give up after a timeout with the app's own environment unchanged.
//!
//! Settings → Terminal setup reads it again when the user asks (richos-hq design round 41,
//! piece 1): [`read_again`] keeps the time of each read, a read that fails keeps the last good
//! one, and [`Setup`] says what the sheet shows, which is names only: the `PATH` folders in
//! order, the setting names, and what changed since the read before.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the shell may take: VS Code's default for the same read.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The mark printed on each side of the environment, so a profile's own output is never
/// read as part of it.
const MARK: &str = "__RICHOS_LOGIN_ENVIRONMENT_d1f3__";

/// Names a shell sets for its own invocation, which describe the reading shell rather than the
/// user's environment.
const PER_SHELL: [&str; 4] = ["_", "PWD", "OLDPWD", "SHLVL"];

/// The user's login shell: `$SHELL` when it is an absolute path, otherwise the account's shell
/// from the user database, otherwise `/bin/zsh` (the macOS default).
pub fn login_shell() -> PathBuf {
    if let Some(shell) = std::env::var_os("SHELL").map(PathBuf::from).filter(|s| s.is_absolute()) { return shell; }
    // SAFETY: getpwuid returns a pointer into static storage or null; it is read at once.
    let from_database = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() { None }
        else { std::ffi::CStr::from_ptr((*entry).pw_shell).to_str().ok().map(PathBuf::from) }
    };
    from_database.filter(|s| s.is_absolute()).unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

/// **Read `shell`'s login environment**: `shell -l -i -c <print>`, with `extra` added to the
/// environment it starts with, stdin closed, waiting at most `timeout`. The shell is a child
/// this process started and owns; on timeout that child, and nothing else, is killed.
pub fn read(shell: &Path, extra: &[(&str, &str)], timeout: Duration) -> Result<BTreeMap<String, String>, String> {
    let print = format!("printf '%s' {MARK}; /usr/bin/env -0; printf '%s' {MARK}");
    let mut child = std::process::Command::new(shell)
        .args(["-l", "-i", "-c", &print])
        .envs(extra.iter().copied())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("{} could not be started: {e}", shell.display()))?;
    let mut stdout = child.stdout.take().ok_or("the shell has no output")?;
    // Read on a thread, so a profile that leaves a background process holding the pipe open
    // cannot make this wait past the timeout: the closing mark is enough.
    let (send, receive) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 65536];
        while let Ok(n) = stdout.read(&mut buffer) {
            if n == 0 || send.send(buffer[..n].to_vec()).is_err() { break; }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut output = Vec::new();
    let found = loop {
        if let Some(found) = between_marks(&output) { break Some(found); }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() { break None; }
        match receive.recv_timeout(left) {
            Ok(chunk) => output.extend(chunk),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break between_marks(&output),
        }
    };
    // The shell has usually ended by now; a kill that finds it gone is not a fault.
    child.kill().ok();
    child.wait().ok();
    let found = found.ok_or_else(|| format!("{} gave no environment within {} s", shell.display(), timeout.as_secs()))?;
    Ok(parse(found))
}

fn between_marks(output: &[u8]) -> Option<&[u8]> {
    let mark = MARK.as_bytes();
    let start = output.windows(mark.len()).position(|w| w == mark)? + mark.len();
    let length = output[start..].windows(mark.len()).position(|w| w == mark)?;
    Some(&output[start..start + length])
}

/// `env -0` output: `NAME=value` records separated by NUL. A record that is not valid UTF-8 or
/// has no name is left out, and so are the [`PER_SHELL`] names.
fn parse(raw: &[u8]) -> BTreeMap<String, String> {
    raw.split(|b| *b == 0).filter_map(|record| std::str::from_utf8(record).ok())
        .filter_map(|record| record.split_once('='))
        .filter(|(name, _)| !name.is_empty() && !PER_SHELL.contains(name))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

type Environment = Arc<BTreeMap<String, String>>;

/// One good read: what it found and when.
#[derive(Clone)]
struct GoodRead {
    environment: Environment,
    at_ms: u64,
    /// The read the app made when it started, not one the user asked for.
    at_start: bool,
}

/// What a reader holds: the last good read, the good read before it (what "What changed" is
/// measured against), and the last attempt when it failed.
#[derive(Clone, Default)]
struct Held {
    last: Option<GoodRead>,
    before: Option<GoodRead>,
    failed: Option<Failed>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failed {
    pub at_ms: u64,
    pub at_start: bool,
}

/// **A login environment reader**: one per process ([`current`] and [`read_again`] use the
/// app's), so a test can hold its own with its own shell.
pub struct Reader {
    shell: fn() -> PathBuf,
    timeout: Duration,
    held: Mutex<Option<Held>>,
    /// Held for the whole of a read the user asked for, so two presses run one read after the
    /// other, never two at once.
    again: Mutex<()>,
}

impl Reader {
    pub const fn new(shell: fn() -> PathBuf, timeout: Duration) -> Self {
        Self { shell, timeout, held: Mutex::new(None), again: Mutex::new(()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Held>> {
        self.held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The environment the leases get: the last good read, or empty when none has succeeded
    /// (the app's own environment, which is what every lease had before this read existed).
    /// The first caller makes the start read; concurrent first callers wait for it.
    pub fn current(&self) -> Environment {
        let mut held = self.lock();
        if let Some(held) = held.as_ref() {
            return held.last.as_ref().map(|r| r.environment.clone()).unwrap_or_default();
        }
        let result = read(&(self.shell)(), &[], self.timeout);
        let mut start = Held::default();
        record(&mut start, result, crate::util::now_millis(), true);
        let found = start.last.as_ref().map(|r| r.environment.clone()).unwrap_or_default();
        *held = Some(start);
        found
    }

    /// The environment when a read has finished, without waiting for one: `None` while the
    /// start read is running or before it began.
    pub fn peek(&self) -> Option<Environment> {
        let held = self.held.try_lock().ok()?;
        let held = held.as_ref()?;
        Some(held.last.as_ref().map(|r| r.environment.clone()).unwrap_or_default())
    }

    /// **Read it again now**, for a user who installed a tool or changed the shell profile
    /// while the app was open. The leases keep the last good read while this runs, and keep
    /// it when this read fails.
    pub fn read_again(&self) -> Setup {
        let _one_at_a_time = self.again.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.current();
        let result = read(&(self.shell)(), &[], self.timeout);
        let mut held = self.lock();
        let held = held.get_or_insert_with(Held::default);
        record(held, result, crate::util::now_millis(), false);
        setup_of(held, &(self.shell)())
    }

    /// What the Terminal setup sheet shows: the last read, what it found by name, and what
    /// changed since the read before it. Waits for the start read when it is still running.
    pub fn setup(&self) -> Setup {
        self.current();
        let held = self.lock();
        setup_of(held.as_ref().unwrap_or(&Held::default()), &(self.shell)())
    }
}

/// Fold one read's result into what is held. A failed read keeps the last good one.
fn record(held: &mut Held, result: Result<BTreeMap<String, String>, String>, at_ms: u64, at_start: bool) {
    match result {
        Ok(found) => {
            held.before = held.last.take();
            held.last = Some(GoodRead { environment: Arc::new(found), at_ms, at_start });
            held.failed = None;
        }
        Err(why) => {
            // Never the environment: the reason names only the shell and the limit.
            eprintln!("[richos] the login shell's environment was not read: {why}");
            held.failed = Some(Failed { at_ms, at_start });
        }
    }
}

/// **The Terminal setup, as the sheet shows it** (round 41 piece 1). Names only: the folders
/// are the `PATH` entries in the order the shell looks in them, the settings are names, and a
/// change says what kind it is and which name. No value is in here.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// The shell's name (`zsh`) and its path (`/bin/zsh`).
    pub shell: String,
    pub shell_path: String,
    /// When the last good read was made, and whether it was the read at the app's start.
    pub read_at_ms: Option<u64>,
    pub at_start: bool,
    /// `PATH`, one folder each, the home folder written `~`.
    pub folders: Vec<String>,
    /// Every name the environment has, sorted.
    pub settings: Vec<String>,
    /// After a read the user asked for: what differs from the read before it (empty when
    /// nothing did), and when that read was made. `None` for the start read.
    pub changes: Option<Vec<Change>>,
    pub since_ms: Option<u64>,
    /// The last attempt, when it failed. The lists above are then the last good read's.
    pub failed: Option<Failed>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// `new`, `gone` or `changed`.
    pub kind: &'static str,
    /// `folder` or `setting`, or `order`: the folders both reads have, in a new order.
    pub of: &'static str,
    pub name: String,
}

fn setup_of(held: &Held, shell: &Path) -> Setup {
    let last = held.last.as_ref();
    let environment = last.map(|r| r.environment.clone()).unwrap_or_default();
    let changes = match (last, held.before.as_ref()) {
        (Some(after), Some(before)) if !after.at_start => Some(changes(&before.environment, &after.environment)),
        _ => None,
    };
    Setup {
        shell: shell.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        shell_path: shell.display().to_string(),
        read_at_ms: last.map(|r| r.at_ms),
        at_start: last.is_some_and(|r| r.at_start),
        folders: folders(&environment),
        settings: environment.keys().cloned().collect(),
        since_ms: changes.as_ref().and(held.before.as_ref()).map(|r| r.at_ms),
        changes,
        failed: held.failed,
    }
}

/// `PATH`'s folders in order, each once, with the home folder written `~`.
pub fn folders(environment: &BTreeMap<String, String>) -> Vec<String> {
    let home = environment.get("HOME").map(|h| h.trim_end_matches('/')).filter(|h| !h.is_empty());
    let mut out: Vec<String> = Vec::new();
    for folder in environment.get("PATH").map(String::as_str).unwrap_or_default().split(':').filter(|f| !f.is_empty()) {
        let shown = match home.and_then(|home| folder.strip_prefix(home)) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => folder.to_string(),
        };
        if !out.contains(&shown) { out.push(shown); }
    }
    out
}

/// **What changed between two reads**, in the sheet's order: tool folders added and gone, then
/// settings added, changed and gone. `PATH` itself is said as its folders, never as a changed
/// setting. A changed setting is only named: its values stay in this process.
pub fn changes(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<Change> {
    let (was, now) = (folders(before), folders(after));
    let mut out = Vec::new();
    out.extend(now.iter().filter(|f| !was.contains(f)).map(|f| Change { kind: "new", of: "folder", name: f.clone() }));
    out.extend(was.iter().filter(|f| !now.contains(f)).map(|f| Change { kind: "gone", of: "folder", name: f.clone() }));
    // The folders both reads have, in a new order: the first one that has a tool wins, so a tool
    // can now come from another folder (review rv-20261025T011529Z-041708c7-5916, finding 2).
    // Named by the folder that now comes first among the ones that moved.
    let kept_was: Vec<&String> = was.iter().filter(|f| now.contains(f)).collect();
    let kept_now: Vec<&String> = now.iter().filter(|f| was.contains(f)).collect();
    if let Some(first) = kept_now.iter().zip(&kept_was).find(|(a, b)| a != b).map(|(a, _)| (*a).clone()) {
        out.push(Change { kind: "changed", of: "order", name: first });
    }
    out.extend(after.keys().filter(|n| !before.contains_key(*n)).map(|n| Change { kind: "new", of: "setting", name: n.clone() }));
    out.extend(after.iter().filter(|(n, v)| n.as_str() != "PATH" && before.get(*n).is_some_and(|old| old != *v))
        .map(|(n, _)| Change { kind: "changed", of: "setting", name: n.clone() }));
    out.extend(before.keys().filter(|n| !after.contains_key(*n)).map(|n| Change { kind: "gone", of: "setting", name: n.clone() }));
    out
}

/// **A digest of an environment**, so a lease can tell that a read again found something
/// different from what it started with. Held in memory only, like the environment.
pub fn fingerprint(environment: &BTreeMap<String, String>) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for (name, value) in environment {
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

static APP: Reader = Reader::new(login_shell, TIMEOUT);

/// **This install's login environment**, read once at start and again only when the user asks
/// ([`read_again`]). A read that fails is reported and keeps the last good one, or empty when
/// there is none, which is the environment every lease had before (the app's own). Concurrent
/// first callers wait for one read rather than run two.
pub fn current() -> Arc<BTreeMap<String, String>> {
    APP.current()
}

/// The app's environment once its start read has finished, without waiting (`Reader::peek`).
pub fn peek() -> Option<Arc<BTreeMap<String, String>>> {
    APP.peek()
}

/// Read it again now and say what it found (`Reader::read_again`).
pub fn read_again() -> Setup {
    APP.read_again()
}

/// The Terminal setup as last read (`Reader::setup`).
pub fn setup() -> Setup {
    APP.setup()
}

/// Read it again now, for a user who changed the shell profile while the app was open.
pub fn refresh() -> Arc<BTreeMap<String, String>> {
    APP.read_again();
    current()
}

/// Start the read at launch on its own thread, so the first lease does not wait for it.
pub fn warm() {
    if let Err(why) = std::thread::Builder::new().name("richos-login-environment".into()).spawn(|| { current(); }) {
        eprintln!("[richos] the login environment read could not start early ({why}); the first lease reads it");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_is_read_between_the_marks_and_per_shell_names_are_left_out() {
        let raw = format!("profile noise{MARK}A=1\0PATH=/x:/y\0_=/usr/bin/env\0SHLVL=2\0EMPTY=\0=bad\0{MARK}trailing");
        let found = parse(between_marks(raw.as_bytes()).unwrap());
        assert_eq!(found, BTreeMap::from([("A".into(), "1".into()), ("PATH".into(), "/x:/y".into()), ("EMPTY".into(), String::new())]));
        assert_eq!(between_marks(format!("{MARK}unfinished").as_bytes()), None);
    }

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// The sheet's folders are `PATH` in order, once each, with the home folder as `~`; a
    /// folder that only starts with the home folder's name is left as it is.
    #[test]
    fn the_tool_folders_are_path_in_order_with_home_as_a_tilde() {
        let e = env(&[("HOME", "/Users/dana"), ("PATH", "/opt/homebrew/bin:/Users/dana/.cargo/bin::/usr/bin:/Users/danabackup/bin:/opt/homebrew/bin:/Users/dana")]);
        assert_eq!(folders(&e), ["/opt/homebrew/bin", "~/.cargo/bin", "/usr/bin", "/Users/danabackup/bin", "~"]);
        assert_eq!(folders(&env(&[("HOME", "/Users/dana")])), Vec::<String>::new());
    }

    /// **What changed** is round 41's five kinds in its order, by name only, and `PATH` is
    /// said as its folders.
    #[test]
    fn what_changed_names_each_folder_and_setting_and_never_a_value() {
        let before = env(&[("HOME", "/Users/dana"), ("PATH", "/opt/homebrew/bin:/Users/dana/.nvm/v20/bin:/usr/bin"),
            ("ANDROID_HOME", "/old/sdk"), ("NVM_DIR", "/Users/dana/.nvm"), ("EDITOR", "vim")]);
        let after = env(&[("HOME", "/Users/dana"), ("PATH", "/opt/homebrew/opt/openjdk/bin:/opt/homebrew/bin:/usr/bin"),
            ("ANDROID_HOME", "/new/sdk"), ("JAVA_HOME", "/opt/jdk"), ("EDITOR", "vim")]);
        let found: Vec<_> = changes(&before, &after).into_iter().map(|c| format!("{} {} {}", c.kind, c.of, c.name)).collect();
        assert_eq!(found, ["new folder /opt/homebrew/opt/openjdk/bin", "gone folder ~/.nvm/v20/bin",
            "new setting JAVA_HOME", "changed setting ANDROID_HOME", "gone setting NVM_DIR"]);
        assert_eq!(changes(&after, &after), Vec::<Change>::new());
        // Review rv-20261025T011529Z-041708c7-5916, finding 2: the same folders in a new order.
        let reordered = changes(&env(&[("PATH", "/opt/homebrew/bin:/usr/bin")]), &env(&[("PATH", "/usr/bin:/opt/homebrew/bin")]));
        assert_eq!(reordered, [Change { kind: "changed", of: "order", name: "/usr/bin".into() }]);
        let shown = serde_json::to_string(&changes(&before, &after)).unwrap();
        for value in ["/old/sdk", "/new/sdk", "/opt/jdk", "vim"] { assert!(!shown.contains(value), "{value} in {shown}"); }
    }

    static FIXTURE_SHELL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    fn fixture_shell() -> PathBuf {
        FIXTURE_SHELL.get().unwrap().clone()
    }

    /// **Read it again**: the start read, then a read that finds a new tool and setting, then
    /// one that finds nothing new, then one that does not finish, which keeps the last good
    /// read; the leases get the last good read throughout.
    #[test]
    fn reading_again_says_what_changed_and_a_failed_read_keeps_the_last_good_one() {
        let dir = std::env::temp_dir().join(format!("richos-login-again-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shell = dir.join("shell");
        // A shell that prints what `profile` says, between the marks, the way `read` asks.
        let write = |profile: &str| {
            std::fs::write(&shell, format!("#!/bin/sh\n{profile}\nprintf '%s' {MARK}; /usr/bin/env -0; printf '%s' {MARK}\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        FIXTURE_SHELL.set(shell.clone()).unwrap();
        let reader = Reader::new(fixture_shell, Duration::from_secs(5));
        write("export PATH=/fixture/bin:/usr/bin HOME=/fixture/home FIXTURE_A=1");
        let start = reader.setup();
        assert!(start.at_start && start.read_at_ms.is_some() && start.changes.is_none() && start.failed.is_none(), "{start:?}");
        assert_eq!(start.folders, ["/fixture/bin", "/usr/bin"]);
        assert!(start.settings.contains(&"FIXTURE_A".to_string()));
        assert_eq!(start.shell, "shell");

        write("export PATH=/fixture/jdk/bin:/fixture/bin:/usr/bin HOME=/fixture/home FIXTURE_A=2 FIXTURE_TOOL_HOME=/fixture/jdk");
        let again = reader.read_again();
        assert!(!again.at_start && again.since_ms == start.read_at_ms, "{again:?}");
        let said: Vec<_> = again.changes.clone().unwrap().into_iter().map(|c| format!("{} {}", c.kind, c.name)).collect();
        assert_eq!(said, ["new /fixture/jdk/bin", "new FIXTURE_TOOL_HOME", "changed FIXTURE_A"]);
        assert_eq!(reader.current().get("FIXTURE_TOOL_HOME").map(String::as_str), Some("/fixture/jdk"), "the next lease gets the new read");

        let same = reader.read_again();
        assert_eq!(same.changes, Some(vec![]), "nothing changed");
        assert_eq!(same.since_ms, again.read_at_ms);

        write("exec sleep 30");
        let reader_slow = Reader { timeout: Duration::from_millis(400), ..Reader::new(fixture_shell, Duration::ZERO) };
        *reader_slow.held.lock().unwrap() = reader.held.lock().unwrap().clone();
        let failed = reader_slow.read_again();
        let failure = failed.failed.expect("the read did not finish");
        assert!(!failure.at_start);
        assert_eq!(failed.read_at_ms, same.read_at_ms, "the last good read is kept");
        assert_eq!(failed.folders, ["/fixture/jdk/bin", "/fixture/bin", "/usr/bin"]);
        assert_eq!(reader_slow.current().get("FIXTURE_TOOL_HOME").map(String::as_str), Some("/fixture/jdk"), "the leases keep it too");

        // A start read that fails: nothing read, the leases get the app's own environment.
        let fresh = Reader { timeout: Duration::from_millis(400), ..Reader::new(fixture_shell, Duration::ZERO) };
        let none = fresh.setup();
        assert!(none.read_at_ms.is_none() && none.folders.is_empty() && none.failed.is_some_and(|f| f.at_start), "{none:?}");
        assert!(fresh.current().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_fingerprint_moves_with_any_name_or_value_and_only_then() {
        let a = env(&[("A", "1"), ("B", "2")]);
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
        assert_ne!(fingerprint(&a), fingerprint(&env(&[("A", "1"), ("B", "3")])));
        assert_ne!(fingerprint(&a), fingerprint(&env(&[("A", "12")])));
    }
}
