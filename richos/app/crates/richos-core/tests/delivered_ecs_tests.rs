use richos_core::ecs::EcsBridge;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture(PathBuf);
impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
fn fixture() -> (Fixture, EcsBridge) {
    let dir = std::env::temp_dir().join(format!("richos ecs bridge {}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new("python3").args(["-c", "import sys; print(sys.executable)"]).output().unwrap();
    assert!(out.status.success());
    let python = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine");
    let bridge = EcsBridge::new(&python, &engine, &dir.join("state")).unwrap();
    (Fixture(dir), bridge)
}

#[test]
fn restart_recovers_scoped_obligations_and_receipts_without_auto_memory() {
    let (_fixture, bridge) = fixture();
    let first = bridge.bind("depot", "thread-a", "session-a", "turn-a", None, "ceo").unwrap();
    assert_eq!(first, bridge.bind("depot", "thread-a", "session-a", "turn-a", None, "ceo").unwrap());
    let checkpoint = json!({"binding":first,"request_id":"manual",
        "checkpoint":{"statements":[{"verb":"commitment","fields":{"id":"manual","title":"Review the depot manual"}}]}});
    assert_eq!(bridge.request("checkpoint", checkpoint.clone()).unwrap()["accepted"], true);
    assert_eq!(bridge.request("checkpoint", checkpoint).unwrap()["duplicate"], true);
    let restarted = bridge.clone();
    let second = restarted.bind("depot", "thread-a", "session-b", "turn-b", None, "ceo").unwrap();
    assert!(restarted.brief(&second, None).unwrap().contains("Review the depot manual"));
    assert!(bridge.brief(&first, None).is_err());
    let other = bridge.bind("studio", "thread-b", "session-c", "turn-c", None, "ceo").unwrap();
    assert!(!bridge.brief(&other, None).unwrap().contains("depot manual"));
}

#[test]
fn restarted_stu_can_reenter_a_saved_turn_after_a_question_answer() {
    let (_fixture, bridge) = fixture();
    let seat = "stu-thread:thread-a";
    let original = bridge.bind("depot", "thread-a", "session-stu", "peer:11104", Some(seat), "rich").unwrap();
    let answer = bridge.bind("depot", "thread-a", "session-stu", "answer:tone", Some(seat), "rich").unwrap();
    assert_ne!(original.revision, answer.revision);
    // Reopening the bridge carries no in-memory transition identity across restart.
    let restarted = EcsBridge::new(&bridge.python, bridge.component.parent().unwrap(), &bridge.state_root).unwrap();
    let resumed = restarted.bind("depot", "thread-a", "session-stu", "peer:11104", Some(seat), "rich").unwrap();
    assert_ne!(answer.revision, resumed.revision);
    assert!(bridge.brief(&original, Some(seat)).is_err(), "the original receipt became current again");
    assert_eq!(resumed, restarted.bind("depot", "thread-a", "session-stu", "peer:11104", Some(seat), "rich").unwrap());
    assert!(restarted.brief(&resumed, Some(seat)).is_ok());
}

#[test]
fn component_errors_never_look_like_an_empty_store() {
    let (_fixture, bridge) = fixture();
    assert!(bridge.request("unrecognized", json!({})).is_err());
    let mut broken = bridge;
    broken.component = broken.state_root.join("missing-component");
    assert!(broken.request("hello", json!({})).is_err());
}
