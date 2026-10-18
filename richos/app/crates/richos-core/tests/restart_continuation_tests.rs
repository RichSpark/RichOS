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
fn an_inflight_restart_preserves_the_input_without_automatic_redelivery() {
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
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));
    spine.reconcile_intake().unwrap();
    spine.reconcile_intake().unwrap();
    assert!(prompts.lock().unwrap().is_empty(),"restart must not resubmit uncertain input");
    assert!(origins.lock().unwrap().is_empty(),"restart must not invent another attempt");
    let old=spine.ledger().turn(&id).unwrap();
    assert_eq!(old.state,TurnState::Interrupted);
    assert_eq!(old.session_id.as_deref(),Some("previous-native-session"));
    assert!(old.ended_at.is_none());assert!(old.superseded_by.is_none());
    let m=spine.messages(&thread).unwrap();
    assert_eq!(m.iter().filter(|x|x.role=="user").count(),1);
    assert!(!m.iter().any(|x|x.role=="assistant"),"restart must not invent an answer");
    let next=spine.submit_prompt("Check the saved request",Source::Text).unwrap();
    assert_eq!(*prompts.lock().unwrap(),["Check the saved request"]);
    assert_eq!(*origins.lock().unwrap(),[(next.clone(),next)]);
    assert_eq!(spine.messages(&thread).unwrap().iter().filter(|m|m.text=="Also, what is 5 plus 5?").count(),1);
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
    let prompts = mock.prompts.clone();
    s.switch_thread(&thread).unwrap();
    s.attach_lease(Box::new(mock));
    s.reconcile_intake().unwrap();
    assert!(prompts.lock().unwrap().is_empty(),"uncertain input belongs to native history, not a reconstruction prompt");
    let previous=s.ledger().turn(&id).unwrap();
    assert_eq!(previous.state,TurnState::Interrupted);
    assert_eq!(previous.session_id.as_deref(),Some("prior-native"));
    assert_eq!(previous.assistant_text,"I started checking");
    assert!(previous.ended_at.is_none());assert!(previous.superseded_by.is_none());
    assert_eq!(s.messages(&thread).unwrap().iter().filter(|x|x.text=="I started checking").count(),1,
        "the actual partial answer stays visible once, without inventing a completion");
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
    s.switch_thread(&thread).unwrap();
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
    let control = TurnControl::open(&intake).unwrap().for_thread(&thread);
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
    s.switch_thread(&thread).unwrap();
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
        let control = TurnControl::open(&intake).unwrap().for_thread(&thread);
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
        spine.switch_thread(&thread).unwrap();
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
    ledger.record_prompt_received(&binding, "Actually use Tuesday", Source::Text).unwrap();
    let previous = ledger.replay_turn(&binding, &original).unwrap();
    ledger.mark_turn_started(&previous, "second-native-uncertain").unwrap();
    drop(ledger);
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let mock = MockCognition::new("third-native", vec!["Checked all previous attempts", "Updated"]);
    let prompts = mock.prompts.clone();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));
    spine.reconcile_intake().unwrap();
    assert_eq!(*prompts.lock().unwrap(),["Actually use Tuesday"],"only the never-submitted input is delivered");
    let first=spine.ledger().turn(&original).unwrap();let second=spine.ledger().turn(&previous).unwrap();
    assert_eq!(first.session_id.as_deref(),Some("first-native-effect"));
    assert_eq!(second.session_id.as_deref(),Some("second-native-uncertain"));
    assert!(first.ended_at.is_none());assert!(second.ended_at.is_none());
    assert_eq!(first.superseded_by.as_deref(),Some(previous.as_str()));
    assert!(second.superseded_by.is_none(),"restart must not create another uncertain attempt");
    assert_eq!(spine.messages(&thread).unwrap().iter().filter(|m| m.role == "user").count(), 2);
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
                let control = TurnControl::open(&intake).unwrap().for_thread(&thread);
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
                spine.switch_thread(&thread).unwrap();
                spine.attach_lease(Box::new(mock));
                spine.reconcile_intake().unwrap();
                let expected = if stopped || inflight { vec!["Actually use Tuesday"] } else { vec!["Use Monday", "Actually use Tuesday"] };
                assert_eq!(*prompts.lock().unwrap(), expected, "inflight={inflight}, stopped={stopped}, phone={phone}");
                spine.reconcile_intake().unwrap();
                assert_eq!(*prompts.lock().unwrap(), expected, "reconciliation must not deliver either request twice");
                drop(spine);
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }
}
