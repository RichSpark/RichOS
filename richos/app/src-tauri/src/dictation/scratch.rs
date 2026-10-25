//! **One dictation's scratch folder**: random, 0700, deleted when the dictation ends.
//!
//! What it holds is his recorded voice and the words whisper-cli wrote, so each dictation gets
//! its own (Frank's M6): `transcribe_bounded` writes `utt-<pid>.wav`, `decoder.stdout` and
//! `decoder.stderr`, opens the last two with `create_new`, and leaves the WAV behind on a
//! timeout, so a shared folder would keep his voice on disk and fail the second dictation with
//! "File exists". The pattern is the phone path's (`phone/voice.rs`, `Scratch`). A failed
//! deletion is logged, never dropped.
//!
//! A kill of the tool in the middle of a dictation skips the drop, so each folder carries its
//! owner record (`richos_core::app_cleanup::OWNER_FILE`, this tool's pid and start), as a lease's
//! engine profile does: the app's sweep (`scratch-reaper.py` `scan_app_dictation`) takes the
//! folder once its tool has ended, and keeps it while the tool runs or a process holds it open.

use std::path::{Path, PathBuf};

/// The parent every dictation's folder is made in, under the tool's data folder.
pub const PARENT: &str = "dictation-scratch";

pub struct Scratch(PathBuf);

impl Scratch {
    /// A new folder under `<data dir>/dictation-scratch/`, named by 12 random bytes, mode 0700.
    pub fn new(data_dir: &Path) -> std::io::Result<Scratch> {
        use std::os::unix::fs::DirBuilderExt;
        let name = crate::phone::hex(
            &crate::phone::random_bytes(12).map_err(|_| std::io::Error::other("the system random source refused"))?,
        );
        let parent = data_dir.join(PARENT);
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&parent)?;
        let path = parent.join(name);
        // Not recursive: the random name must be new, or this is not this dictation's folder.
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        // Without it the folder is still taken, later, once it is past the reaper's age floor.
        if let Err(error) = richos_core::app_cleanup::write_owner(&path) {
            super::log::line(&format!("a dictation's scratch folder carries no owner record: {error}"));
        }
        Ok(Scratch(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Already gone is the one outcome that is not a failure.
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                super::log::line(&format!("a dictation's scratch folder could not be removed: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn data(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-dictation-scratch-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    /// INVARIANT: each dictation's folder is new, private, names its owner, and is gone when the
    /// dictation ends.
    #[test]
    fn a_folder_is_private_new_and_removed_on_drop() {
        let d = data("drop");
        let a = Scratch::new(&d).unwrap();
        let b = Scratch::new(&d).unwrap();
        assert_ne!(a.path(), b.path());
        let mode = std::fs::metadata(a.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        // Its owner record: this process, as the app's sweep reads it.
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(a.path().join(richos_core::app_cleanup::OWNER_FILE)).unwrap()).unwrap();
        assert_eq!(record["pid"].as_u64(), Some(u64::from(std::process::id())));
        assert_eq!(record["procStart"].as_str(), richos_core::app_cleanup::own_start().as_deref());
        std::fs::write(a.path().join("utt-1.wav"), b"his voice").unwrap();
        let (pa, pb) = (a.path().to_path_buf(), b.path().to_path_buf());
        drop(a);
        drop(b);
        assert!(!pa.exists() && !pb.exists());
        assert_eq!(std::fs::read_dir(d.join(PARENT)).unwrap().count(), 0, "nothing left");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT (M6): two dictations back to back through the REAL bounded decoder each get a
    /// clean folder: the second does not fail with "File exists", and nothing is left after
    /// either. A shell script stands in for whisper-cli; it is the decoder's process handling
    /// that is under test, not recognition.
    #[test]
    fn two_dictations_back_to_back_leave_nothing() {
        let d = data("twice");
        let fake = d.join("fake-whisper");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(&fake, "#!/bin/sh\necho ' Hello there.'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        for _ in 0..2 {
            let scratch = Scratch::new(&d).unwrap();
            let wav = scratch.path().join("utt.wav");
            std::fs::write(&wav, b"RIFF").unwrap();
            let out = richos_voice::stt::bounded_decoder(
                &mut std::process::Command::new(&fake),
                scratch.path(),
                std::time::Duration::from_secs(10),
            )
            .expect("the second run must not meet the first run's files");
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Hello there.");
        }
        assert_eq!(std::fs::read_dir(d.join(PARENT)).unwrap().count(), 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT (M6): a decoder that passes its bound is stopped, and its folder, holding the
    /// recording and whatever it wrote, is still removed.
    #[test]
    fn nothing_is_left_after_a_timeout() {
        let d = data("timeout");
        let fake = d.join("slow-whisper");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(&fake, "#!/bin/sh\necho partial\nsleep 30\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let folder;
        {
            let scratch = Scratch::new(&d).unwrap();
            folder = scratch.path().to_path_buf();
            std::fs::write(folder.join("utt.wav"), b"his voice").unwrap();
            let started = std::time::Instant::now();
            let err = richos_voice::stt::bounded_decoder(
                &mut std::process::Command::new(&fake),
                scratch.path(),
                std::time::Duration::from_millis(300),
            )
            .unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
            assert!(started.elapsed() < std::time::Duration::from_secs(5), "the decoder was not stopped");
            assert!(folder.join("utt.wav").exists(), "the recording is still there before the drop");
        }
        assert!(!folder.exists(), "the recording and the partial words are gone");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
