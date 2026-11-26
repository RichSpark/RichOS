//! **Opening the app starts no turn of Rich's for anything saved from before** (CEO 2026-11-26:
//! "Everything was either stopped, finished, ended or paused in the RichOS app. So, unless I'm
//! missing something major, there should have been absolutely zero usage after me just opening
//! the app."), and a turn that was running when the app ended still carries on.

use richos_core::{
    cognition::MockCognition,
    entity::EntityId,
    ledger::{Ledger, Source},
    turn_gate::TurnGate,
};
use std::sync::Arc;
mod support;

/// The app's folders: the ledger beside the engine state, as the app keeps them.
fn opened() -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("turn-gate-launch-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("engine-state")).unwrap();
    let ledger = root.join("conversation-ledger.jsonl");
    (root, ledger)
}

/// A message an agent left for Rich before the quit, never started, is not sent at launch; the
/// user's first words in that chat are, and the saved message follows them.
#[test]
fn a_message_saved_before_the_app_opened_waits_until_the_user_speaks_in_that_chat() {
    let (root, path) = opened();
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("fixture", &EntityId::parse("femcboost").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    ledger.record_prompt_received(&binding, "{\"from\":\"stu\",\"text\":\"a report from yesterday\"}", Source::Agent).unwrap();
    drop(ledger);
    // The ledger's clock is in milliseconds: what was written before is strictly earlier.
    // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
    std::thread::sleep(std::time::Duration::from_millis(3));

    let mut spine = support::spine(Ledger::open(&path).unwrap());
    spine.set_turn_gate(Arc::new(TurnGate::at_launch(&root.join("engine-state"))));
    let mock = MockCognition::new("after-launch", vec!["Here is what Codex said", "Noted"]);
    let prompts = mock.prompts.clone();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));
    spine.reconcile_intake().unwrap();
    spine.reconcile_intake().unwrap();
    assert!(prompts.lock().unwrap().is_empty(), "opening the app sent a saved message to Rich");

    spine.submit_prompt("What did Codex say?", Source::Text).unwrap();
    let sent = prompts.lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "after his words the saved message did not follow: {sent:?}");
    assert_eq!(sent[0], "What did Codex say?");
    assert!(sent[1].contains("a report from yesterday"));
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

/// The user's own turn that was running when the app ended (an update restart, a crash, a
/// quit mid-turn) is a job actually running: it is picked up again at launch, as before.
#[test]
fn a_turn_that_was_running_when_the_app_ended_carries_on() {
    let (root, path) = opened();
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("fixture", &EntityId::parse("femcboost").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let original = ledger.record_prompt_received(&binding, "Finish the draft", Source::Text).unwrap();
    ledger.mark_turn_started(&original, "original-session").unwrap();
    drop(ledger);
    // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
    std::thread::sleep(std::time::Duration::from_millis(3));

    let mut spine = support::spine(Ledger::open(&path).unwrap());
    spine.set_turn_gate(Arc::new(TurnGate::at_launch(&root.join("engine-state"))));
    let mock = MockCognition::new("same-native-session", vec!["Done"]);
    let prompts = mock.prompts.clone();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));
    spine.detach_conversation_io();
    spine.recover_after_restart().unwrap();
    let shared = std::sync::Mutex::new(spine);
    richos_core::spine::drive_shared_turns(&shared).unwrap();
    assert_eq!(prompts.lock().unwrap().len(), 1, "the turn running at the quit was not picked up again");
    assert!(prompts.lock().unwrap()[0].contains("Finish the draft"));
    drop(shared);
    std::fs::remove_dir_all(root).unwrap();
}
