//! Durable cold-start intake preserves original requests, order and conversation scope.
//! Native session tests cover immediate SDK delivery while a reply is active.
use richos_core::{cognition::MockCognition,entity::EntityId,ledger::{Ledger,Source},staging::CandidateDesk,steering::{IntakeLog,IntakeRecord,SteeringError,TurnControl}};
mod support;
fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_path(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "richos-deferred-send-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn a_control_with_nowhere_durable_to_write_refuses_rather_than_accepting_what_it_cannot_record() {
    // The same refusal `steer` and `request_stop` make, and for the same reason: accepting a
    // message that is not written down is worse than making the caller block, because the
    // caller's fallback path works and a silently dropped sentence does not.
    let control = TurnControl::detached();
    control.for_thread("thr_1").begin_front_desk_prime();
    assert!(matches!(
        control.defer_send("thr_1", Some(femcboost()), "no log here"),
        Err(SteeringError::NoDurableIntake)
    ));
}

#[test]
fn a_correction_he_types_during_a_prime_is_staged_exactly_as_one_he_types_at_any_other_time() {
    // **THE REGRESSION THIS SLICE WOULD HAVE SHIPPED, PINNED SO IT CANNOT COME BACK.**
    //
    // `submit_prompt_inner`'s own comment calls it *"the ONE function every CEO utterance
    // passes through"*, and three correction families hang off that claim. `drain_intake`'s
    // `Steer` and `Channel` arms file the turn and stage NOTHING — today that only costs the
    // phone. Had a deferred desktop send been filed as a `Channel`, a correction he typed
    // during a front-desk prime would have vanished with no trace anywhere: the turn would
    // look perfectly normal and the flywheel would simply not have fired.
    //
    // So the `Desk` arm goes through `accept_prompt`, and this is the assertion that says so.
    // The correction and the misspelling are invented for the test.
    // **REMOVED HOWEVER THIS TEST ENDS, not only when it passes** (CEO §54: cleanup is a
    // mechanism, not a per-run judgment). The first version removed the directory on its last
    // line, so the RED-FIRST run that proved this test can fail left its scratch behind — the
    // exact shape of the failure §54 was written about, at one-thousandth the size.
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Scratch(std::env::temp_dir().join(format!(
        "richos-deferred-correction-{}-{}",
        std::process::id(),
        richos_core::util::now_millis()
    )));
    let dir = &dir.0;
    std::fs::create_dir_all(dir).unwrap();
    let mut spine = support::spine(Ledger::open(dir.join("ledger.jsonl")).unwrap());
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(richos_core::cognition::MockCognition::new(
        "sess-1",
        vec!["I have put the Kestral review in for the fourteenth.", "Noted — Kestrel."],
    )));
    let control = TurnControl::open(dir.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let desk = CandidateDesk::open(dir.join("candidates.jsonl")).unwrap().shared();
    spine.set_candidate_desk(desk.clone());

    spine.submit_prompt("What is on for the Kestral account?", Source::Text).unwrap();
    assert!(desk.lock().unwrap().pending().is_empty(), "nothing to correct yet");

    // The front desk is being primed and his correction goes down the deferred road.
    control.for_thread(&thread).begin_front_desk_prime();
    control
        .defer_send(&thread, Some(femcboost()), "No, it's Kestrel, not Kestral.")
        .unwrap()
        .expect("accepted");
    control.for_thread(&thread).end_front_desk_prime();
    spine.poll_intake().unwrap();

    let guard = desk.lock().unwrap();
    let pending = guard.pending();
    assert_eq!(pending.len(), 1, "the correction was dropped because of which road it took: {pending:?}");
    assert_eq!(pending[0].ask.from, "Kestral");
    assert_eq!(pending[0].ask.to, "Kestrel");
    assert_eq!(
        pending[0].ask.anchor.as_deref(),
        Some("I have put the Kestral review in for the fourteenth."),
        "the anchor must quote the line the wrong word appeared on, deferred or not"
    );
    drop(guard);
}

#[test]
fn ending_one_conversations_prime_cannot_clear_anothers_durable_intake_claim() {
    let path=tmp_path("independent-prime-claims");
    let control=TurnControl::open(&path).unwrap();let a=control.for_thread("a");let b=control.for_thread("b");
    a.begin_front_desk_prime();b.begin_front_desk_prime();
    assert_eq!(a.end_front_desk_prime().as_deref(),Some("a"));
    assert!(a.front_desk_priming().is_none());assert_eq!(b.front_desk_priming().as_deref(),Some("b"));
    assert!(control.defer_send("a",Some(femcboost()),"A now takes the normal road").unwrap().is_none());
    assert!(control.defer_send("b",Some(femcboost()),"B stays durable while priming").unwrap().is_some());
    assert_eq!(b.end_front_desk_prime().as_deref(),Some("b"));drop(control);
    let reopened=TurnControl::open(&path).unwrap();assert_eq!(reopened.pending_intake().len(),1);
    assert!(std::fs::read_to_string(&path).unwrap().contains("B stays durable while priming"));
    drop(reopened);std::fs::remove_file(path).unwrap();
}

#[test]
fn cold_start_intake_is_durable_ordered_and_delivered_once() {
    let ledger=tmp_path("ordered-ledger");let intake=tmp_path("ordered-intake");
    let mut spine=support::spine(Ledger::open(&ledger).unwrap());
    let thread=spine.create_thread("Conversation",&femcboost()).unwrap();
    let mock=MockCognition::new("original-native-session",vec!["First answer","Second answer"]);
    let prompts=mock.prompts.clone();spine.attach_lease(Box::new(mock));
    let control=TurnControl::open(&intake).unwrap();spine.set_turn_control(control.clone());
    let cold=control.for_thread(&thread);cold.begin_front_desk_prime();
    let mut identities=Vec::new();
    for text in ["First unchanged request","Second unchanged request"] {
        identities.push(control.defer_send(&thread,Some(femcboost()),text).unwrap().unwrap().id());
    }
    let reopened=IntakeLog::open(&intake).unwrap();assert!(reopened.health().is_clean());
    let pending=reopened.pending();assert_eq!(pending.len(),2);
    for (index,record) in pending.iter().enumerate() {
        match record { IntakeRecord::Desk{id,thread_id,text,..}=>{
            assert_eq!(*id,identities[index]);assert_eq!(thread_id,&thread);
            assert_eq!(text,["First unchanged request","Second unchanged request"][index]);
        },other=>panic!("wrong intake record: {other:?}") }
    }
    cold.end_front_desk_prime();
    for _ in 0..3 {spine.poll_intake().unwrap();}
    assert_eq!(*prompts.lock().unwrap(),["First unchanged request","Second unchanged request"]);
    assert_eq!(spine.ledger().thread_turns(&thread).unwrap().len(),2);
    for id in identities {assert!(spine.ledger().turn_for_intake(id).is_some());}
    drop(spine);drop(control);drop(reopened);std::fs::remove_file(ledger).unwrap();std::fs::remove_file(intake).unwrap();
}

#[test]
fn an_available_connection_uses_the_ordinary_path_without_extra_intake() {
    let path=tmp_path("ordinary");let control=TurnControl::open(&path).unwrap();
    assert!(control.defer_send("thread",Some(femcboost()),"Original request").unwrap().is_none());
    assert!(control.pending_intake().is_empty());assert!(!path.exists(),"an ordinary send created an intake file");drop(control);
}
