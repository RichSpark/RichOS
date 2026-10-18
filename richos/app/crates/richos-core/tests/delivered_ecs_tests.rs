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

/// **The app's answer close against the REAL engine** (bgdone2 item 1): the obligation is opened
/// the way the register opens it, on the conversation's own per-thread seat; his next turn
/// makes that binding stale; `EcsAnsweredClose` still closes it, and the host's own settle
/// reading then says Settled. An assignment with a helper on its work seat is refused, and
/// stays open. The unit tests pin the request to a fake store; this pins it to the verb.
#[test]
fn an_answered_assignment_closes_in_the_real_engine_and_one_with_a_helper_does_not() {
    use richos_core::cognition::ObligationState;
    use richos_core::work_host::AnsweredClose;
    let (fixture, bridge) = fixture();
    let seat = richos_core::ecs::ceo_seat("thread-a").unwrap();
    let first = bridge.bind("depot", "thread-a", "session-a", "turn-a", Some(&seat), "ceo").unwrap();
    for id in ["work-ran-it", "work-coded-it"] {
        let open = richos_core::ecs::seated_request(Some(&seat), json!({"binding": first,
            "request_id": format!("assignment-obligation:{id}"),
            "checkpoint": {"statements": [{"verb": "commitment", "fields": {"id": id, "title": id}}]}}));
        assert_eq!(bridge.request("checkpoint", open).unwrap()["accepted"], true);
    }
    // A helper was prepared for the second: its work unit sits on that assignment's work seat.
    let work_seat = "work-seat:work-coded-it";
    let work = bridge.bind("depot", "thread-a", "session-w", "work-coded-it", Some(work_seat), "worker").unwrap();
    bridge.request("observe", richos_core::ecs::seated_request(Some(work_seat), json!({"binding": work,
        "request_id": "prep-1", "source_ref": "app-dispatch:prep-1",
        "work": {"authority": "richos-provider-v1", "work_unit_id": "wu-prep-1", "external_id": "receipt-1",
                 "title": "code it", "owner": "work", "status": "started"}}))).unwrap();
    // His next turn: the binding the obligations were opened under is stale from here.
    let now = bridge.bind("depot", "thread-a", "session-a", "turn-b", Some(&seat), "ceo").unwrap();
    assert!(bridge.obligation_state(&first, Some(&seat), "work-ran-it").is_err(), "the first binding is still current");

    let state = fixture.0.join("app-state");
    let record = |obligation: &str| {
        let receipt = richos_core::assignment::register_kind(&state, &richos_core::assignment::Registration {
            entity_id: "depot".into(), thread_id: "thread-a".into(), obligation_id: obligation.into(),
            instruction_ledger_ref: "ledger:thread-a:turn-a".into(), instruction_sha256: "0".repeat(64),
            title: obligation.into(), repositories: vec![], needs_screen: false,
        }, richos_core::assignment::AssignmentKind::Task).unwrap();
        richos_core::assignment::read(&state, "depot", "thread-a", &receipt.id).unwrap()
    };
    let store = bridge.clone();
    let close = richos_core::operator_runtime::EcsAnsweredClose::with(Box::new(move |command, fields| {
        store.request(command, fields).map_err(|e| e.0)
    }));
    close.close_answered(&record("work-ran-it"), "It ran and printed one line: 82208dc init.").unwrap();
    assert_eq!(bridge.obligation_state(&now, Some(&seat), "work-ran-it").unwrap(), ObligationState::Settled);

    let refused = close.close_answered(&record("work-coded-it"), "Done.").unwrap_err();
    assert!(refused.contains("helper"), "{refused}");
    assert_eq!(bridge.obligation_state(&now, Some(&seat), "work-coded-it").unwrap(), ObligationState::Open);
}

#[test]
fn component_errors_never_look_like_an_empty_store() {
    let (_fixture, bridge) = fixture();
    assert!(bridge.request("unrecognized", json!({})).is_err());
    let mut broken = bridge;
    broken.component = broken.state_root.join("missing-component");
    assert!(broken.request("hello", json!({})).is_err());
}
