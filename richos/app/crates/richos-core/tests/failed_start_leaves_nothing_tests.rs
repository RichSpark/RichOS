//! A conversation lease that cannot start leaves nothing behind (2026-11-01 start: 44 failed
//! starts left two scope files and a started-and-killed Haiku each).
#![cfg(unix)]
use richos_core::{engine_profile::EngineProfile, native::NativeCognition, runtime::EngineRuntime};
use std::path::PathBuf;

#[test]
fn a_failed_start_removes_its_scope_files_and_never_starts_haiku() {
    let root = std::env::temp_dir().join(format!("failed-start-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap();
    let python = std::process::Command::new("python3").args(["-c", "import sys;print(sys.executable)"]).output().unwrap();
    let runtime = EngineRuntime { root: root.join("runtime"), python: String::from_utf8(python.stdout).unwrap().trim().into(),
        git: "/usr/bin/git".into(), node: "/usr/bin/true".into(), versions: Default::default() };
    let profile = EngineProfile::prepare_with_settings(&engine, &root.join("data"), runtime, Default::default()).unwrap();
    let bridge = richos_core::ecs::EcsBridge::new(&profile.runtime.python, &engine, &profile.state).unwrap();
    let skills = richos_core::skills::ensure_rendered(&root.join("skills"), &engine.join("app")).unwrap();
    let scopes = profile.state.join("scopes");
    let missing = root.join("no-such-claude");
    let result = NativeCognition::start_with_engine(&missing, &skills.join("agents/rich.md"), &skills,
        &std::env::current_exe().unwrap(), bridge, profile.clone(), None);
    assert!(result.is_err(), "a missing provider must fail the start");
    let left: Vec<_> = std::fs::read_dir(&scopes).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "a failed start left files behind: {left:?}");
    std::fs::remove_dir_all(root).unwrap();
}
