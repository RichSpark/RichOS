#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
//! **The engine check outlives the process** (launch-speed audit 2026-11-14, piece 1, built past
//! Sage's review, findings 5 and 6). Every launch used to read and hash the whole engine runtime
//! (431 MB in 6,542 files on the release Mac) while the window waited, because what it verified
//! was remembered only in memory. Each launch here is a NEW PROCESS (this test binary run again
//! as a child), because a second load in the same process was already cheap and proves nothing
//! about the next launch.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
    if std::env::var(MODE).as_deref() == Ok("recheck") {
        println!("RESULT {:?}", richos_core::runtime::recheck_runtime_bytes(&engine).map_err(|e| e.to_string()));
        return;
    }
    let loaded = richos_core::runtime::EngineRuntime::load(&engine, None).map(|_| ()).map_err(|e| e.to_string());
    println!("RESULT hashed={} {:?}", richos_core::runtime::files_hashed_under(&root), loaded);
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
    /// An engine with a seven-file runtime, and a separate app data folder for the saved file.
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("richos runtime saved {}", uuid::Uuid::new_v4()));
        let runtime = dir.join("engine/runtime");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::create_dir_all(runtime.join("git/libexec/git-core")).unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let mut files = serde_json::Map::new();
        for name in ["bin/python3", "bin/node", "bin/git", "bin/jq", "bin/ffmpeg", "bin/ffprobe", "git/libexec/git-core/git-remote-https"] {
            let content = format!("#!/bin/sh\n# {name}\nexit 0\n");
            std::fs::write(runtime.join(name), &content).unwrap();
            std::fs::set_permissions(runtime.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
            use sha2::Digest;
            files.insert(name.into(), serde_json::json!(format!("{:x}", sha2::Sha256::digest(content.as_bytes()))));
        }
        std::fs::write(runtime.join("delivery.json"), serde_json::to_vec(&serde_json::json!({
            "schema":1,"platform":"aarch64-apple-darwin","versions":{},"files":files,"links":{}
        })).unwrap()).unwrap();
        Self { dir }
    }
    fn engine(&self) -> PathBuf { self.dir.join("engine") }
    fn saved(&self) -> PathBuf { self.dir.join("data/runtime-verified.txt") }
    fn file(&self, name: &str) -> PathBuf { self.engine().join("runtime").join(name) }
}
impl Drop for Fixture { fn drop(&mut self) { std::fs::remove_dir_all(&self.dir).ok(); } }

#[test]
fn a_second_launch_reads_no_runtime_bytes_and_writes_nothing_into_the_engine() {
    let f = Fixture::new();
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), "hashed=7 Ok(())", "the first launch reads every file");
    assert!(f.saved().is_file(), "the first launch saved nothing in the app's data folder");
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), "hashed=0 Ok(())", "the second launch read the runtime again");
    // The engine folder holds exactly what was delivered: the load above would have refused an
    // unexpected file, and nothing was written beside the runtime either.
    let mut beside: Vec<_> = std::fs::read_dir(f.engine()).unwrap().map(|e| e.unwrap().file_name()).collect();
    beside.sort();
    assert_eq!(beside, ["runtime"]);
}

#[test]
fn a_touched_file_is_hashed_again_and_a_changed_one_refused() {
    let f = Fixture::new();
    launch(&f.engine(), &f.saved(), "load");
    // Same length, different bytes, modified time put back: the status-changed time still moves.
    let git = f.file("bin/git");
    let modified = std::fs::metadata(&git).unwrap().modified().unwrap();
    let before = std::fs::read(&git).unwrap();
    let mut changed = before.clone();
    changed[before.len() - 2] = b'1';
    std::fs::write(&git, &changed).unwrap();
    std::fs::OpenOptions::new().write(true).open(&git).unwrap().set_modified(modified).unwrap();
    // Only that file is read (the six others keep their stamps), and its bytes are refused.
    let result = launch(&f.engine(), &f.saved(), "load");
    assert!(result.starts_with("hashed=0 Err(") && result.contains("runtime file changed: bin/git"), "{result}");
}

#[test]
fn a_saved_file_that_does_not_read_cleanly_costs_one_full_check_and_never_a_refusal() {
    let f = Fixture::new();
    for nonsense in [&b"not the format"[..], b"richos-runtime-verified 1\nroot\t/somewhere/else\n", b""] {
        std::fs::write(f.saved(), nonsense).unwrap();
        assert_eq!(launch(&f.engine(), &f.saved(), "load"), "hashed=7 Ok(())");
        assert_eq!(launch(&f.engine(), &f.saved(), "load"), "hashed=0 Ok(())", "a good file was not written back");
    }
}

/// What a stamp cannot see is found by the late full check, which forgets what was verified so
/// the next launch reads everything and refuses (finding 6). Damage that keeps a file's stamp
/// cannot be made from userland, so the saved line is brought up to the damaged file's stamp,
/// which is exactly what such damage looks like to the quick check.
#[test]
fn the_late_full_check_finds_what_a_stamp_cannot_and_the_next_launch_refuses() {
    let f = Fixture::new();
    launch(&f.engine(), &f.saved(), "load");
    assert_eq!(launch(&f.engine(), &f.saved(), "recheck"), "Ok(Confirmed)");

    let node = f.file("bin/node");
    std::fs::write(&node, b"#!/bin/sh\n# damaged\nexit 9\n").unwrap();
    let m = std::fs::metadata(&node).unwrap();
    let stamp = format!("{} {} {} {} {} {} {}", m.len(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec(), m.ino(), m.dev());
    let text = std::fs::read_to_string(f.saved()).unwrap();
    let restamped: String = text.lines().map(|line| match line.split_once('\t') {
        Some((fields, "bin/node")) => format!("{} {stamp}\tbin/node\n", fields.split(' ').next().unwrap()),
        _ => format!("{line}\n"),
    }).collect();
    std::fs::write(f.saved(), restamped).unwrap();
    assert_eq!(launch(&f.engine(), &f.saved(), "load"), "hashed=0 Ok(())", "the quick check should not have seen it");

    assert_eq!(launch(&f.engine(), &f.saved(), "recheck"), "Ok(Changed(\"bin/node\"))");
    assert!(!f.saved().exists(), "what was verified was not forgotten");
    let next = launch(&f.engine(), &f.saved(), "load");
    assert!(next.contains("runtime file changed: bin/node"), "the next launch did not refuse: {next}");
}
