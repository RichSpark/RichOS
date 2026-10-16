use richos_core::{
    cognition::MockCognition,
    entity::EntityId,
    ledger::{Ledger, Source, TurnState},
    steering::{ActiveTurn, TurnControl},
};
mod support;
fn paths() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("restart-continuation-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn saved_inflight_input_is_answered_automatically_once_after_restart() {
    let root = paths();
    let path = root.join("ledger.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger
        .create_thread("fixture", &EntityId::parse("femcboost").unwrap())
        .unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let id = ledger
        .record_prompt_received(&binding, "Also, what is 5 plus 5?", Source::Text)
        .unwrap();
    ledger
        .mark_turn_started(&id, "previous-native-session")
        .unwrap();
    drop(ledger);
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let mock = MockCognition::new("restarted", vec!["10"]);
    let prompts = mock.prompts.clone();
    let origins = mock.request_origins.clone();
    spine.attach_lease(Box::new(mock));
    spine.reconcile_intake().unwrap();
    spine.reconcile_intake().unwrap();
    assert_eq!(*prompts.lock().unwrap(), vec!["Also, what is 5 plus 5?"]);
    let origins=origins.lock().unwrap();
    assert_eq!(origins.len(),1);
    assert_eq!(origins[0].1,id,"restart must retain the original request's execution identity");
    assert_ne!(origins[0].0,id,"the recovered delivery must keep its own lifecycle identity");
    let m = spine.messages(&thread).unwrap();
    assert_eq!(m.iter().filter(|x| x.role == "user").count(), 1);
    assert_eq!(m.iter().find(|x| x.role == "assistant").unwrap().text, "10");
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn continuation_preserves_uncertainty_and_previous_transcript_identity() {
    let root = paths();
    let path = root.join("ledger.jsonl");
    let mut l = Ledger::open(&path).unwrap();
    let thread = l
        .create_thread("fixture", &EntityId::parse("femcboost").unwrap())
        .unwrap();
    let b = l.thread_binding(&thread).unwrap();
    let id = l
        .record_prompt_received(&b, "Finish the authorized work", Source::Text)
        .unwrap();
    l.mark_turn_started(&id, "prior-native").unwrap();
    l.append_assistant_delta(&id, "I started checking", 1)
        .unwrap();
    drop(l);
    let mut s = support::spine(Ledger::open(&path).unwrap());
    let mock = MockCognition::new("after", vec!["Checked the previous attempt"]);
    let contexts = mock.recovery_contexts.clone();
    let prompts = mock.prompts.clone();
    s.attach_lease(Box::new(mock));
    s.reconcile_intake().unwrap();
    assert_eq!(*prompts.lock().unwrap(), vec!["Finish the authorized work"]);
    let c: serde_json::Value = serde_json::from_str(&contexts.lock().unwrap()[0]).unwrap();
    let p = &c["previous_delivery_attempt"];
    assert_eq!(p["turn_id"], id);
    assert_eq!(p["session_id"], "prior-native");
    assert_eq!(p["delivery_outcome"], "unknown");
    assert_eq!(p["assistant_text"], "I started checking");
    assert!(!s
        .messages(&thread)
        .unwrap()
        .iter()
        .any(|x| x.text == "I started checking"));
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn crash_after_continuation_record_keeps_one_pending_input() {
    let root = paths();
    let path = root.join("ledger.jsonl");
    let mut l = Ledger::open(&path).unwrap();
    let thread = l
        .create_thread("fixture", &EntityId::parse("femcboost").unwrap())
        .unwrap();
    let b = l.thread_binding(&thread).unwrap();
    let id = l
        .record_prompt_received(&b, "Proceed", Source::Text)
        .unwrap();
    l.mark_turn_started(&id, "prior").unwrap();
    let next = l.replay_turn(&b, &id).unwrap();
    assert_eq!(l.replay_turn(&b, &id).unwrap(), next);
    assert_eq!(l.turn(&id).unwrap().state, TurnState::Interrupted);
    assert!(l.turn(&id).unwrap().ended_at.is_none());
    drop(l);
    let l = Ledger::open(&path).unwrap();
    assert_eq!(l.pending_turns().len(), 1);
    assert_eq!(l.pending_turns()[0].id, next);
    assert_eq!(l.messages(&thread).unwrap().len(), 1);
    drop(l);
    let mut s = support::spine(Ledger::open(&path).unwrap());
    let mock = MockCognition::new("after", vec!["Done"]);
    let prompts = mock.prompts.clone();
    s.attach_lease(Box::new(mock));
    s.reconcile_intake().unwrap();
    assert_eq!(*prompts.lock().unwrap(), vec!["Proceed"]);
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn durable_user_stop_prevents_automatic_continuation() {
    let root = paths();
    let path = root.join("ledger.jsonl");
    let intake = root.join("intake.jsonl");
    let mut l = Ledger::open(&path).unwrap();
    let entity = EntityId::parse("femcboost").unwrap();
    let thread = l.create_thread("fixture", &entity).unwrap();
    let b = l.thread_binding(&thread).unwrap();
    let id = l
        .record_prompt_received(&b, "Do the work", Source::Text)
        .unwrap();
    l.mark_turn_started(&id, "prior").unwrap();
    drop(l);
    let control = TurnControl::open(&intake).unwrap();
    control.begin_turn(ActiveTurn {
        turn_id: id.clone(),
        thread_id: thread.clone(),
        entity_id: Some(entity),
        started_at: Some(1),
    });
    control.request_stop().unwrap();
    drop(control);
    let mut s = support::spine(Ledger::open(&path).unwrap());
    s.set_turn_control(TurnControl::open(&intake).unwrap());
    let mock = MockCognition::new("after", vec![]);
    let prompts = mock.prompts.clone();
    s.attach_lease(Box::new(mock));
    s.reconcile_intake().unwrap();
    assert!(prompts.lock().unwrap().is_empty());
    assert_eq!(s.ledger().turn(&id).unwrap().state, TurnState::Stopped);
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_pressed_control_keeps_its_provenance_and_does_not_become_a_typed_bubble() {
    let root = paths();
    let path = root.join("ledger.jsonl");
    let mut l = Ledger::open(&path).unwrap();
    let thread = l
        .create_thread("fixture", &EntityId::parse("femcboost").unwrap())
        .unwrap();
    let b = l.thread_binding(&thread).unwrap();
    let id = l
        .record_prompt_received_pressed(&b, "Proceed", Source::Text)
        .unwrap();
    l.mark_turn_started(&id, "prior").unwrap();
    let next = l.replay_turn(&b, &id).unwrap();
    drop(l);
    let l = Ledger::open(&path).unwrap();
    assert!(l.turn(&next).unwrap().pressed);
    assert_eq!(l.pending_turns().len(), 1);
    assert_eq!(l.pending_turns()[0].id, next);
    assert!(l.messages(&thread).unwrap().is_empty());
    drop(l);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_stop_saved_between_replay_and_delivery_stops_the_continuation() {
    for restart in [true, false] {
        let root = paths();
        let path = root.join("ledger.jsonl");
        let intake = root.join("intake.jsonl");
        let mut ledger = Ledger::open(&path).unwrap();
        let entity = EntityId::parse("femcboost").unwrap();
        let thread = ledger.create_thread("fixture", &entity).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let original = ledger
            .record_prompt_received(&binding, "Finish the authorized work", Source::Text)
            .unwrap();
        ledger.mark_turn_started(&original, "prior-native").unwrap();
        let control = TurnControl::open(&intake).unwrap();
        control.begin_turn(ActiveTurn {
            turn_id: original.clone(),
            thread_id: thread.clone(),
            entity_id: Some(entity),
            started_at: Some(1),
        });
        let continuation = ledger.replay_turn(&binding, &original).unwrap();
        control.request_stop().unwrap();
        drop(control);
        drop(ledger);
        let mut spine = support::spine(Ledger::open(&path).unwrap());
        spine.set_turn_control(TurnControl::open(&intake).unwrap());
        let mock = MockCognition::new("restarted", vec![]);
        let prompts = mock.prompts.clone();
        spine.attach_lease(Box::new(mock));
        if restart {
            spine.reconcile_intake().unwrap();
        } else {
            spine.poll_intake().unwrap();
        }
        assert!(
            prompts.lock().unwrap().is_empty(),
            "Stop of the saved request was lost across its continuation record"
        );
        assert_eq!(
            spine.ledger().turn(&continuation).unwrap().state,
            TurnState::Stopped
        );
        assert!(spine
            .ledger()
            .turn(&continuation)
            .unwrap()
            .stop_requested_at
            .is_some());
        drop(spine);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn repeated_restarts_preserve_every_uncertain_native_attempt() {
    let root = paths();
    let path = root.join("ledger.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("fixture", &EntityId::parse("femcboost").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let original = ledger.record_prompt_received(&binding, "Finish the authorized work", Source::Text).unwrap();
    ledger.mark_turn_started(&original, "first-native-effect").unwrap();
    let previous = ledger.replay_turn(&binding, &original).unwrap();
    ledger.mark_turn_started(&previous, "second-native-uncertain").unwrap();
    drop(ledger);
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let mock = MockCognition::new("third-native", vec!["Checked all previous attempts"]);
    let contexts = mock.recovery_contexts.clone();
    let prompts = mock.prompts.clone();
    spine.attach_lease(Box::new(mock));
    spine.reconcile_intake().unwrap();
    assert_eq!(*prompts.lock().unwrap(), vec!["Finish the authorized work"]);
    let facts: serde_json::Value = serde_json::from_str(&contexts.lock().unwrap()[0]).unwrap();
    assert_eq!(facts["previous_delivery_attempt"]["session_id"], "second-native-uncertain");
    assert_eq!(facts["earlier_delivery_attempts"][0]["session_id"], "first-native-effect");
    assert_eq!(facts["earlier_delivery_attempts"][0]["turn_id"], original);
    assert_eq!(facts["earlier_delivery_attempts"][0]["delivery_outcome"], "unknown");
    assert_eq!(spine.messages(&thread).unwrap().iter().filter(|m| m.role == "user").count(), 1);
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_delivers_older_ledger_requests_before_newer_intake_and_applies_stops_first() {
    for inflight in [false, true] {
        for stopped in [false, true] {
            for phone in [false, true] {
                let root = paths();
                let path = root.join("ledger.jsonl");
                let intake = root.join("intake.jsonl");
                let mut ledger = Ledger::open(&path).unwrap();
                let entity = EntityId::parse("femcboost").unwrap();
                let thread = ledger.create_thread("fixture", &entity).unwrap();
                let binding = ledger.thread_binding(&thread).unwrap();
                let older = ledger.record_prompt_received(&binding, "Use Monday", Source::Text).unwrap();
                if inflight { ledger.mark_turn_started(&older, "before-restart").unwrap(); }
                let control = TurnControl::open(&intake).unwrap();
                if stopped {
                    control.begin_turn(ActiveTurn { turn_id: older.clone(), thread_id: thread.clone(), entity_id: Some(entity.clone()), started_at: None });
                    control.request_stop().unwrap();
                }
                if phone {
                    control.submit_from_channel(&thread, Some(entity), "Actually use Tuesday", "phone").unwrap();
                } else {
                    control.submit_from_desk(&thread, Some(entity), "Actually use Tuesday").unwrap();
                }
                drop(ledger);
                drop(control);
                let mut spine = support::spine(Ledger::open(&path).unwrap());
                spine.set_turn_control(TurnControl::open(&intake).unwrap());
                let mock = MockCognition::new("after-restart", vec!["Understood", "Updated"]);
                let prompts = mock.prompts.clone();
                spine.attach_lease(Box::new(mock));
                spine.reconcile_intake().unwrap();
                let expected = if stopped { vec!["Actually use Tuesday"] } else { vec!["Use Monday", "Actually use Tuesday"] };
                assert_eq!(*prompts.lock().unwrap(), expected, "inflight={inflight}, stopped={stopped}, phone={phone}");
                spine.reconcile_intake().unwrap();
                assert_eq!(*prompts.lock().unwrap(), expected, "reconciliation must not deliver either request twice");
                drop(spine);
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }
}
