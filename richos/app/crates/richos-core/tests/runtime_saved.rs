#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
//! **A runtime verified whole once is trusted as written** (launch-speed audit 2026-11-14,
//! piece 1, and the CEO's question the same day: "Isn't there a more elegant way to completely
//! eliminate the need for that check?"). Every launch used to read and hash the whole engine
//! runtime (431 MB in 6,542 files on the release Mac) while the window waited. Now it is verified
//! in full once -- at its install, or the first time the app meets it -- and the late background
//! check finds damage afterwards. Each launch here is a NEW PROCESS (this test binary run again as
//! a child), because what one process remembers in memory proves nothing about the next launch.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const ENGINE: &str = "RICHOS_RUNTIME_SAVED_ENGINE";
const SAVED: &str = "RICHOS_RUNTIME_SAVED_FILE";
const MODE: &str = "RICHOS_RUNTIME_SAVED_MODE";

/// Not a test of its own: the body each child process runs. Without the variables it is a no-op.
#[test]
fn child_entry() {
    let (Ok(engine), Ok(saved)) = (std::env::var(ENGINE), std::env::var(SAVED)) else { return };
    richos_core::runtime::remember_verified_at(Path::new(&saved));
    let engine = PathBuf::from(engine);
    let root = std::fs::canonicalize(engine.join("runtime")).unwrap();
    match std::env::var(MODE).as_deref() {
        Ok("recheck") => {
            println!("RESULT {:?}", richos_core::runtime::recheck_runtime_bytes(&engine).map_err(|e| e.to_string()));
        }
        Ok("installed") => {
            richos_core::runtime::record_delivered_whole(&engine);
            println!("RESULT recorded");
        }
        _ => {
            let loaded = richos_core::runtime::EngineRuntime::load(&engine, None).map(|_| ()).map_err(|e| e.to_string());
            println!("RESULT full={} hashed={} {:?}", richos_core::runtime::full_checks_under(&root),
                     richos_core::runtime::files_hashed_under(&root), loaded);
        }
    }
}

/// One launch: this binary as a child, running only `child_entry`. Returns its RESULT line.
fn launch(engine: &Path, saved: &Path, mode: &str) -> String {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(ENGINE, engine)
        .env(SAVED, saved)
        .env(MODE, mode)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // The harness prints `test child_entry ... ` on the same line, before the body's output.
    text.lines().find_map(|l| l.find("RESULT ").map(|at| l[at + "RESULT ".len()..].trim_end().to_owned()))
        .unwrap_or_else(|| panic!("the child printed no result: {text}{}", String::from_utf8_lossy(&out.stderr)))
}

struct Fixture { dir: PathBuf }
impl Fixture {
    /// An engine with a seven-file runtime, and a separate app data folder for the record.
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("richos runtime saved {}", uuid::Uuid::new_v4()));
        let runtime = dir.join("engine/runtime");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::create_dir_all(runtime.join("git/libexec/git-core")).unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let fixture = Self { dir };
        fixture.deliver("");
        fixture
    }
    /// (Re)write the seven commands and their `delivery.json`; `mark` changes their bytes.
    fn deliver(&self, mark: &str) {
        let runtime = self.engine().join("runtime");
        let mut files = serde_json::Map::new();
        for name in ["bin/python3", "bin/node", "bin/git", "bin/jq", "bin/ffmpeg", "bin/ffprobe", "git/libexec/git-core/git-remote-https"] {
            let content = format!("#!/bin/sh\n# {name}{mark}\nexit 0\n");
            std::fs::write(runtime.join(name), &content).unwrap();
            std::fs::set_permissions(runtime.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
            use sha2::Digest;
            files.insert(name.into(), serde_json::json!(format!("{:x}", sha2::Sha256::digest(content.as_bytes()))));
        }
        std::fs::write(runtime.join("delivery.json"), serde_json::to_vec(&serde_json::json!({
            "schema":1,"platform":"aarch64-apple-darwin","versions":{},"files":files,"links":{}
        })).unwrap()).unwrap();
    }
    fn engine(&self) -> PathBuf { self.dir.join("engine") }
    fn saved(&self) -> PathBuf { self.dir.join("data/runtime-verified.txt") }
    fn file(&self, name: &str) -> PathBuf { self.engine().join("runtime").join(name) }
}
impl Drop for Fixture { fn drop(&mut self) { std::fs::remove_dir_all(&self.dir).ok(); } }

const TRUSTED: &str = "full=0 hashed=0 Ok(())";
const VERIFIED_IN_FULL: &str = "full=1 hashed=7 Ok(())";

#[test]
fn a_runtime_verified_once_is_never_read_again_and_nothing_is_written_into_the_engine() {
    let f = Fixture::new();
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), VERIFIED_IN_FULL, "the first meeting verifies in full");
    assert!(f.saved().is_file(), "nothing was recorded in the app's data folder");
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), TRUSTED, "a later launch read the runtime again");
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), TRUSTED);
    let mut beside: Vec<_> = std::fs::read_dir(f.engine()).unwrap().map(|e| e.unwrap().file_name()).collect();
    beside.sort();
    assert_eq!(beside, ["runtime"], "something was written beside the runtime");
}

/// The install verifies the staged runtime in full and moves it into place whole; it records it
/// there, so not even the first launch after an install reads it (`setup::install_engine`).
#[test]
fn an_installed_runtime_is_trusted_from_its_first_launch() {
    let f = Fixture::new();
    assert_eq!(launch(&f.engine(), &f.saved(), "installed"), "recorded");
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), TRUSTED);
}

/// Damage after the fact is not seen by a launch -- that is the point -- and is found by the late
/// full check, which forgets the record: the next launch verifies in full and refuses, and
/// set-up offers the reinstall (Sage's review, finding 6).
#[test]
fn damage_is_found_by_the_late_check_and_the_next_launch_refuses() {
    let f = Fixture::new();
    launch(&f.engine(), &f.saved(), "load");
    assert_eq!(launch(&f.engine(), &f.saved(), "recheck"), "Ok(Confirmed)");
    let node = f.file("bin/node");
    let modified = std::fs::metadata(&node).unwrap().modified().unwrap();
    std::fs::write(&node, b"#!/bin/sh\n# damaged!\nexit 9\n").unwrap();
    std::fs::OpenOptions::new().write(true).open(&node).unwrap().set_modified(modified).unwrap();
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), TRUSTED, "a launch read the runtime");
    assert_eq!(launch(&f.engine(), &f.saved(), "recheck"), "Ok(Changed(\"bin/node\"))");
    let left = std::fs::read_to_string(f.saved()).unwrap_or_default();
    assert_eq!(left.lines().count(), 1, "the record still names the runtime: {left}");
    let next = launch(&f.engine(), &f.saved(), "load");
    assert!(next.starts_with("full=1 ") && next.contains("runtime file changed: bin/node"),
            "the next launch did not verify and refuse: {next}");
}

/// A record that does not read cleanly, or names another runtime, is no record: one full check,
/// never a refusal. A runtime delivered again (a new `delivery.json`) is a different runtime.
#[test]
fn a_record_for_anything_else_costs_one_full_check_and_never_a_refusal() {
    let f = Fixture::new();
    for nonsense in [&b"not the format"[..], b"richos-runtime-whole 1\nroot\t/somewhere/else\ndelivery\t1 2 3 4 5 6 7\n", b""] {
        std::fs::write(f.saved(), nonsense).unwrap();
        assert_eq!(launch(&f.engine(), &f.saved(), "load"), VERIFIED_IN_FULL);
        assert_eq!(launch(&f.engine(), &f.saved(), "load"), TRUSTED, "a good record was not written back");
    }
    f.deliver(" again");
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), VERIFIED_IN_FULL, "a new delivery was trusted unread");
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), TRUSTED);
}
