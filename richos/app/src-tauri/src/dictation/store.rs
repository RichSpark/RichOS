//! `<data dir>/dictation.json`: the one settings file dictation has (plan section 1).
//!
//! **The app writes it**; the tool reads it at start and again when the app says it changed
//! (`settings-changed`). **The one exception is the tool's menu bar menu** (slice 3): its
//! Accuracy rows and **Turn dictation off** live in the tool and must work with RichOS's window
//! closed, so the tool changes exactly those two fields with [`update`], and then tells every
//! app `settings-changed`. The accuracy label is derived from `model`, never stored beside it,
//! so the label cannot claim a model the next dictation will not use.
//!
//! **Two writers, one file, no lost change** (slice 5, once the tool runs without the app):
//! every change goes through [`update`], which holds an advisory lock on `.dictation.json.lock`
//! beside the file across its read, its change and its atomic write. The app's and the tool's
//! read-modify-writes therefore never interleave: the second waits for the first's write and
//! then reads it. An in-process mutex could not do this (two processes), and a lock-free
//! read-then-write could lose the app's key change under the tool's accuracy change (the test
//! `two_writers_lose_nothing` is that race, made deterministic).

use richos_voice::dictation::{DEFAULT_KEY, MORE_ACCURATE};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "dictation.json";
/// The lock file beside it (never the file itself, which is replaced by rename).
pub const LOCK_NAME: &str = ".dictation.json.lock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Dictation is on.
    pub on: bool,
    /// The key, as an F-number, 1 to 19.
    pub key: u8,
    /// The model id: More accurate (`large-v3-turbo-q5_0`, the default) or Faster (`small.en`).
    pub model: String,
    /// RichOS asked macOS for the microphone once.
    pub mic_asked: bool,
    /// RichOS asked macOS for Accessibility once, so "never asked" and "refused" differ.
    pub ax_asked: bool,
    /// Rich made his one offer.
    pub offered: bool,
    /// The "Dictation feature" conversation opened once dictation was set up
    /// (`dictation_offer::mark_feature_thread`). Kept here so every write through [`update`]
    /// keeps it: dropped, the next turn-on would open a second one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_thread: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            on: false,
            key: DEFAULT_KEY,
            model: MORE_ACCURATE.to_string(),
            mic_asked: false,
            ax_asked: false,
            offered: false,
            feature_thread: None,
        }
    }
}

impl Settings {
    /// A key outside F1..F19 is not a key: the default stands in, so a hand-edited file can
    /// never leave the tool matching nothing.
    pub fn key(&self) -> u8 {
        if (1..=19).contains(&self.key) { self.key } else { DEFAULT_KEY }
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// The settings on disk. A missing file is the defaults (dictation off); an unreadable one is
/// an error the caller logs, never a silent "off" that hides a broken file.
pub fn read(data_dir: &Path) -> Result<Settings, String> {
    match std::fs::read(path(data_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{FILE_NAME} is not readable: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(format!("{FILE_NAME} could not be read: {e}")),
    }
}

/// Write the settings atomically (a sibling file renamed over the old one), so the tool never
/// reads half a file.
pub fn write(data_dir: &Path, settings: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("the data folder could not be created: {e}"))?;
    let target = path(data_dir);
    let partial = data_dir.join(format!(".{FILE_NAME}.{}", std::process::id()));
    let body = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&partial, body).map_err(|e| format!("{FILE_NAME} could not be written: {e}"))?;
    std::fs::rename(&partial, &target).map_err(|e| {
        std::fs::remove_file(&partial).ok();
        format!("{FILE_NAME} could not be replaced: {e}")
    })
}

/// **Change the file as it is on disk now**, under the file lock: read it fresh, apply `change`,
/// write it back atomically. What the app and the tool's menu both use, so a field the other
/// wrote a moment ago is kept. A file that cannot be read is never overwritten.
pub fn update(data_dir: &Path, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    let _held = lock(data_dir)?;
    let mut settings = read(data_dir)?;
    change(&mut settings);
    write(data_dir, &settings)?;
    Ok(settings)
}

/// Take the advisory lock (blocking); released when the returned file is dropped. `pub` for the
/// one other writer of the file, Rich's offer (`dictation_offer::mark_shown`), which keeps its
/// own JSON shape and so cannot go through [`update`].
pub fn lock(data_dir: &Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    std::fs::create_dir_all(data_dir).map_err(|e| format!("the data folder could not be created: {e}"))?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(data_dir.join(LOCK_NAME))
        .map_err(|e| format!("{LOCK_NAME} could not be opened: {e}"))?;
    // SAFETY: a valid descriptor we own for the life of `file`.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(format!("{LOCK_NAME} could not be locked: {}", std::io::Error::last_os_error()));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-dictation-store-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    /// INVARIANT: no file is dictation off, F1, More accurate.
    #[test]
    fn no_file_is_off_on_f1_more_accurate() {
        let d = dir("none");
        let s = read(&d).unwrap();
        assert_eq!(s, Settings::default());
        assert!(!s.on);
        assert_eq!(s.key(), 1);
        assert_eq!(s.model, "large-v3-turbo-q5_0");
    }

    /// INVARIANT: what the app writes is what the tool reads, under the names the plan gives.
    #[test]
    fn a_write_reads_back_with_the_plans_field_names() {
        let d = dir("roundtrip");
        let s = Settings { on: true, key: 5, model: "small.en".into(), mic_asked: true, ax_asked: true, offered: true, feature_thread: None };
        write(&d, &s).unwrap();
        assert_eq!(read(&d).unwrap(), s);
        let raw = std::fs::read_to_string(path(&d)).unwrap();
        for field in ["\"on\"", "\"key\"", "\"model\"", "\"micAsked\"", "\"axAsked\"", "\"offered\""] {
            assert!(raw.contains(field), "{field} in {raw}");
        }
        let leftovers: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from(FILE_NAME)], "no partial file left");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT (slice 5, two writers): the app's change and the tool's change to the same file
    /// both survive. The race, made deterministic: writer A is inside its change when writer B
    /// changes another field. With the lock, B waits for A's write and reads it; without it, B
    /// writes inside A's window and A's stale copy then erases B's field.
    #[test]
    fn two_writers_lose_nothing() {
        use std::sync::mpsc::channel;
        let d = dir("two-writers");
        write(&d, &Settings::default()).unwrap();
        let (b_started, started) = channel::<()>();
        let (a_free, free) = channel::<()>();
        let d2 = d.clone();
        let a = std::thread::spawn(move || {
            update(&d2, |s| {
                b_started.send(()).unwrap();
                // B is now trying its own update; with the lock it cannot finish while this runs.
                free.recv_timeout(std::time::Duration::from_millis(400)).ok();
                s.key = 5;
            })
            .unwrap()
        });
        started.recv().unwrap();
        let d3 = d.clone();
        let b = std::thread::spawn(move || update(&d3, |s| s.model = "small.en".into()).unwrap());
        // B is blocked on the lock: it does not finish while A holds it.
        assert!(!b.is_finished(), "writer B must wait for writer A");
        a_free.send(()).ok();
        a.join().unwrap();
        b.join().unwrap();
        let after = read(&d).unwrap();
        assert_eq!((after.key, after.model.as_str()), (5, "small.en"), "{after:?}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT: the menu's change keeps every field it does not touch, as the file holds it now.
    #[test]
    fn an_update_keeps_what_it_does_not_change() {
        let d = dir("update");
        let s = Settings { on: true, key: 5, model: "large-v3-turbo-q5_0".into(), mic_asked: true, ax_asked: true, offered: true, feature_thread: None };
        write(&d, &s).unwrap();
        let after = update(&d, |s| s.model = "small.en".into()).unwrap();
        assert_eq!(after, Settings { model: "small.en".into(), ..s.clone() });
        assert_eq!(read(&d).unwrap(), after);
        std::fs::write(path(&d), b"{broken").unwrap();
        assert!(update(&d, |s| s.on = false).is_err(), "a broken file is never overwritten with defaults");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT: a broken file is an error, never a silent off; a key out of range is F1.
    #[test]
    fn a_broken_file_is_said_and_a_bad_key_is_f1() {
        let d = dir("broken");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(path(&d), b"{not json").unwrap();
        assert!(read(&d).is_err());
        std::fs::write(path(&d), br#"{"on":true,"key":42}"#).unwrap();
        let s = read(&d).unwrap();
        assert!(s.on);
        assert_eq!(s.key(), 1);
        assert_eq!(s.model, "large-v3-turbo-q5_0", "a missing field takes its default");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
