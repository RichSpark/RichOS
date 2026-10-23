//! **ONE RICHDOCTOR AT A TIME** (PRD §2: "Opening a second copy focuses the existing window").
//!
//! Opening the same app bundle again reaches the running copy through macOS itself (the reopen
//! event). A second copy of the bundle at another place (`/Applications` and `~/Applications`)
//! would be a second process, so the first one listens on an owner-only socket in RichDoctor's
//! data folder; a later one connects, asks it to open its window, and exits.
//!
//! The same socket answers RichOS's bundled installer (PRD §3), which may refresh an older copy
//! only when no help session is open: `quit-if-idle` is answered `ok` (the running copy saves and
//! quits) or `busy` (a help session is open, or work is not saved: the refresh waits for a later
//! launch). Nothing else is accepted.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SOCKET: &str = "instance.sock";

/// What another process asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Open,
    Saved,
    QuitIfIdle,
}

pub fn path(root: &Path) -> PathBuf {
    root.join(SOCKET)
}

/// Ask a running copy to `what`; its answer, or `None` when no copy is running.
pub fn ask(root: &Path, what: &str) -> Option<String> {
    let mut s = UnixStream::connect(path(root)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(6))).ok();
    s.set_write_timeout(Some(Duration::from_secs(2))).ok();
    writeln!(s, "{what}").ok()?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).ok()?;
    Some(line.trim().to_string())
}

/// Become the running copy: listen, and hand every request to `on` (which answers it). A stale
/// socket left by a copy that crashed is replaced; a live one means another copy runs (`Err`).
pub fn listen(root: &Path, on: impl Fn(Ask) -> &'static str + Send + 'static) -> Result<(), String> {
    richdoctor_core::util::private_dir(root).map_err(|e| e.to_string())?;
    let p = path(root);
    if UnixStream::connect(&p).is_ok() {
        return Err("another RichDoctor is running".into());
    }
    std::fs::remove_file(&p).ok();
    let listener = UnixListener::bind(&p).map_err(|e| e.to_string())?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).ok();
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut line = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let ask = match line.trim() {
                "open" => Ask::Open,
                "saved" => Ask::Saved,
                "quit-if-idle" => Ask::QuitIfIdle,
                _ => continue,
            };
            let reply = on(ask);
            let mut w = &stream;
            writeln!(w, "{reply}").ok();
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_copy_reaches_the_first_and_only_known_asks_are_answered() {
        let root = std::env::temp_dir().join(format!("rd-inst-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(ask(&root, "open"), None, "nothing runs yet");
        listen(&root, |a| match a {
            Ask::QuitIfIdle => "busy",
            _ => "ok",
        })
        .unwrap();
        assert_eq!(ask(&root, "open").as_deref(), Some("ok"));
        assert_eq!(ask(&root, "quit-if-idle").as_deref(), Some("busy"));
        assert!(listen(&root, |_| "ok").is_err(), "a second listener is refused while the first runs");
        std::fs::remove_dir_all(&root).ok();
    }
}
