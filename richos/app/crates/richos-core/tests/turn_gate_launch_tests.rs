//! **Opening the app starts no turn of Rich's for anything saved from before** (CEO 2026-11-26:
//! "Everything was either stopped, finished, ended or paused in the RichOS app. So, unless I'm
//! missing something major, there should have been absolutely zero usage after me just opening
//! the app."), and Rich's answer the app's end cut off waits for the user's next words there.

use richos_core::{
    cognition::MockCognition,
    entity::EntityId,
    ledger::{Ledger, Source},
    steering::TurnControl,
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

/// A message an agent left in Rich's intake before the app opened is not even filed as a turn
/// at launch (a turn in the ledger shows its chat as working); after the user's first words in
/// that chat it is, and it follows them.
#[test]
fn an_agents_message_left_in_the_intake_is_no_turn_until_the_user_speaks() {
    let (root, path) = opened();
    let intake = root.join("intake.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let entity = EntityId::parse("femcboost").unwrap();
    let thread = ledger.create_thread("fixture", &entity).unwrap();
    drop(ledger);
    let control = TurnControl::open(&intake).unwrap();
    control.submit_agent_message(&thread, Some(entity), "{\"from\":\"stu\",\"text\":\"a report from yesterday\"}", "stu", "peer:fixture:1").unwrap();
    drop(control);
    // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
    std::thread::sleep(std::time::Duration::from_millis(3));

    let mut spine = support::spine(Ledger::open(&path).unwrap());
    spine.set_turn_control(TurnControl::open(&intake).unwrap());
    spine.set_turn_gate(Arc::new(TurnGate::at_launch(&root.join("engine-state"))));
    let mock = MockCognition::new("after-launch", vec!["Here is what Codex said", "Noted"]);
    let prompts = mock.prompts.clone();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));
    spine.reconcile_intake().unwrap();
    assert!(prompts.lock().unwrap().is_empty(), "opening the app sent Rich an agent's saved message");
    assert!(spine.ledger().thread_turns(&thread).unwrap().is_empty(), "the saved message was filed as a turn, showing the chat as working");

    spine.submit_prompt("What did Codex say?", Source::Text).unwrap();
    let sent = prompts.lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "after his words the saved message did not follow: {sent:?}");
    assert!(sent[1].contains("a report from yesterday"));
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

/// **A held message is never lost** (review rv-20261126T151735Z-c45967b1-2484, finding 2): a
/// report filed after it, while the app is open, is delivered by itself, and marking that one
/// drained must not drop the held one, in memory or in the intake log on disk.
#[test]
fn a_report_delivered_while_open_never_drops_the_held_one_before_it() {
    let (root, path) = opened();
    let intake = root.join("intake.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let entity = EntityId::parse("femcboost").unwrap();
    let thread = ledger.create_thread("fixture", &entity).unwrap();
    drop(ledger);
    let control = TurnControl::open(&intake).unwrap();
    control.submit_agent_message(&thread, Some(entity.clone()), "{\"from\":\"stu\",\"text\":\"held from yesterday\"}", "stu", "peer:fixture:1").unwrap();
    drop(control);
    // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
    std::thread::sleep(std::time::Duration::from_millis(3));

    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let control = TurnControl::open(&intake).unwrap();
    spine.set_turn_control(control.clone());
    spine.set_turn_gate(Arc::new(TurnGate::at_launch(&root.join("engine-state"))));
    let mock = MockCognition::new("after-launch", vec!["Seen", "Answered", "Noted"]);
    let prompts = mock.prompts.clone();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));
    control.submit_agent_message(&thread, Some(entity), "{\"from\":\"stu\",\"text\":\"written while open\"}", "stu", "peer:fixture:2").unwrap();
    spine.reconcile_intake().unwrap();
    assert_eq!(prompts.lock().unwrap().len(), 1, "the report written while open did not go by itself");
    assert!(prompts.lock().unwrap()[0].contains("written while open"));
    let held = |records: Vec<richos_core::steering::IntakeRecord>| records.iter().any(|record| matches!(record,
        richos_core::steering::IntakeRecord::Channel { text, .. } if text.contains("held from yesterday")));
    assert!(held(control.pending_intake()), "the held report was dropped from the intake in memory");
    assert!(held(TurnControl::open(&intake).unwrap().pending_intake()), "the held report was dropped from the intake log on disk");

    spine.submit_prompt("Anything from yesterday?", Source::Text).unwrap();
    let sent = prompts.lock().unwrap().clone();
    assert_eq!(sent.len(), 3, "after his words the held report did not follow: {sent:?}");
    assert!(sent[2].contains("held from yesterday"));
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

/// **Rich's answer the app's end cut off stays paused until the user writes in that chat, and
/// then it carries on** (the CEO's standing default: after a terminate signal mid-answer, the
/// reopened app had shown the chat "Working" while nothing ran). At launch
/// nothing is sent and nothing in the ledger reads as running; the turn says why it stopped.
/// It stays so across another reopening. His next words there serve the request again first,
/// then his words.
#[test]
fn rich_s_answer_cut_off_by_the_apps_end_waits_for_his_next_words_and_then_carries_on() {
    let (root, path) = opened();
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("fixture", &EntityId::parse("femcboost").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let original = ledger.record_prompt_received(&binding, "Finish the draft", Source::Text).unwrap();
    ledger.mark_turn_started(&original, "original-session").unwrap();
    drop(ledger);
    let cut_off = richos_core::interruption::InterruptionCause::CutOffByAppEnd.tag();

    let mut prompts = None;
    for launch in 0..2 {
        // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
        std::thread::sleep(std::time::Duration::from_millis(3));
        let mut spine = support::spine(Ledger::open(&path).unwrap());
        spine.set_turn_gate(Arc::new(TurnGate::at_launch(&root.join("engine-state"))));
        let mock = MockCognition::new("after-launch", vec!["Here is the finished draft", "Yes, it is done"]);
        prompts = Some(mock.prompts.clone());
        spine.switch_thread(&thread).unwrap();
        spine.attach_lease(Box::new(mock));
        spine.detach_conversation_io();
        spine.recover_after_restart().unwrap();
        let shared = std::sync::Mutex::new(spine);
        richos_core::spine::drive_shared_turns(&shared).unwrap();
        let spine = shared.into_inner().unwrap();
        assert!(prompts.as_ref().unwrap().lock().unwrap().is_empty(), "the answer the app's end cut off carried on with nothing from the user");
        assert!(spine.ledger().pending_turns().is_empty(), "a turn reads as running, so the chat shows Working with nothing running");
        let turn = spine.ledger().turn(&original).unwrap();
        assert_eq!(turn.interruption.as_ref().map(|record| record.cause.as_str()), Some(cut_off), "the turn does not say it is paused");
        if launch == 1 {
            let shared = std::sync::Mutex::new(spine);
            shared.lock().unwrap().submit_prompt("Is it done?", Source::Text).unwrap();
            richos_core::spine::drive_shared_turns(&shared).unwrap();
        }
    }
    let sent = prompts.unwrap().lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "after his words the cut-off answer did not carry on: {sent:?}");
    assert!(sent[0].contains("Finish the draft"), "the cut-off request was not served first: {sent:?}");
    assert!(sent[1].contains("Is it done?"), "{sent:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fresh_words_after_a_damaged_history_row_resume_cut_off_work() {
    use richos_core::turn_gate::Origin;
    use std::io::Write;

    let (root, path) = opened();
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("fixture", &EntityId::parse("femcboost").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    drop(ledger);
    let mut journal = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    journal.write_all(&[0xff]).unwrap();

    let gate = TurnGate::at_launch(&root.join("engine-state"));
    assert!(!gate.may_start(&binding, Origin::CarriesOn));
    assert!(gate.cut_off(&thread));

    journal.write_all(b"\n").unwrap();
    drop(journal);
    let mut ledger = Ledger::open(&path).unwrap();
    ledger.record_prompt_received(&binding, "A new typed question", Source::Text).unwrap();
    gate.user_acted(&binding);
    let written = gate.written(&thread);
    let resumed = gate.may_start(&binding, Origin::CarriesOn);
    let paused = gate.cut_off(&thread);
    assert_eq!(Ledger::open(&path).unwrap().thread_turns(&thread).unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
    assert!(written && resumed && !paused, "the damaged row hid the user's new words");
}

#[test]
fn concurrent_chat_openings_publish_every_chat_to_the_review_watcher() {
    use richos_core::turn_gate::{Origin, RECORD};
    use serde_json::{json, Value};
    use std::collections::BTreeSet;
    use std::sync::Barrier;

    const CHATS: usize = 24;
    let (root, path) = opened();
    let mut ledger = Ledger::open(&path).unwrap();
    let entity = EntityId::parse("femcboost").unwrap();
    let bindings = (0..CHATS).map(|_| {
        let thread = ledger.create_thread("fixture", &entity).unwrap();
        ledger.thread_binding(&thread).unwrap()
    }).collect::<Vec<_>>();
    drop(ledger);
    let expected = bindings.iter().map(|binding| binding.thread_id()).collect::<BTreeSet<_>>();

    let state = root.join("engine-state");
    let mut failed = Vec::new();
    for round in 0..32 {
        let gate = TurnGate::at_launch(&state);
        let barrier = Barrier::new(CHATS);
        std::thread::scope(|workers| {
            for binding in &bindings {
                let gate = &gate;
                let barrier = &barrier;
                workers.spawn(move || {
                    barrier.wait();
                    assert!(gate.may_start(binding, Origin::Now));
                });
            }
        });
        let bytes = std::fs::read(state.join(RECORD)).unwrap();
        let valid = serde_json::from_slice::<Value>(&bytes).is_ok_and(|record| {
            record["version"] == json!(1)
                && record["since_ms"] == json!(gate.since_ms())
                && record["open"].as_array().is_some_and(|rows| {
                    rows.len() == CHATS
                        && rows.iter().all(|row| row["entity_id"] == entity.as_str())
                        && rows.iter().filter_map(|row| row["thread_id"].as_str())
                            .collect::<BTreeSet<_>>() == expected
                })
        });
        if !valid {
            failed.push(round);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
    assert!(failed.is_empty(), "the watcher record was incomplete or unreadable in rounds {failed:?}");
}
