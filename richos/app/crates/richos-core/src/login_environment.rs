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

static CURRENT: Mutex<Option<Arc<BTreeMap<String, String>>>> = Mutex::new(None);

/// **This install's login environment**, read once and kept for the life of the process. A
/// read that fails is reported and kept as empty, which is the environment every lease had
/// before (the app's own). Concurrent first callers wait for one read rather than run two.
pub fn current() -> Arc<BTreeMap<String, String>> {
    let mut held = CURRENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(found) = held.as_ref() { return found.clone(); }
    let found = Arc::new(read(&login_shell(), &[], TIMEOUT).unwrap_or_else(|why| {
        eprintln!("[richos] the login shell's environment was not read: {why}");
        BTreeMap::new()
    }));
    *held = Some(found.clone());
    found
}

/// Read it again now, for a user who changed the shell profile while the app was open.
pub fn refresh() -> Arc<BTreeMap<String, String>> {
    *CURRENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
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
}
