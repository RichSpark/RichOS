//! **WHICH CONVERSATIONS' TURNS ENDED** — the spine's answer to the one question every
//! entrance asks at its turn boundary (`Spine::take_ended_turns`).
//!
//! CEO ruling §88: *"The user should be able to answer on their phone just as well as they
//! can answer on the desktop app."* A task given aloud or from the phone sat `Registered`
//! until he next typed (esc-20260926T083231Z-23865924), because only the typed send adopted
//! work at its boundary, and it adopted for whichever thread was active afterwards. The shell
//! now asks the spine instead of guessing (`src-tauri/src/main.rs`,
//! `adopt_at_the_turn_boundary`); these tests pin what the spine answers, for each road a
//! turn can take: typed, spoken, the phone's drain, the boot's reconciliation, and a failed
//! turn. Every "nothing ended" below is paired with the same fixture in which something does.
//!
//! Headless throughout: no live Claude, no network, no Tauri.

use richos_core::cognition::{Cognition, CognitionError, MockLeaseFactory, TurnItem};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::spine::Spine;
use richos_core::steering::TurnControl;

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_path(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "richos-ended-turns-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    remove_if_present(&p);
    p
}

/// A file this suite owns is gone afterwards; "it was never there" is the one acceptable failure.
fn remove_if_present(p: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(p) {
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "could not remove {}: {error}", p.display());
    }
}

/// Two conversations in one company, the first one active, with an intake log attached.
fn two_threads(tag: &str) -> (Spine, TurnControl, String, String, Vec<std::path::PathBuf>) {
    let ledger = tmp_path(&format!("{tag}-ledger"));
    let intake = tmp_path(&format!("{tag}-intake"));
    let mut spine = support::spine(Ledger::open(&ledger).unwrap());
    let first = spine.create_thread("the proposal", &femcboost()).unwrap();
    let second = spine.create_thread("the hiring plan", &femcboost()).unwrap();
    spine.switch_thread(&first).unwrap();
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec![])));
    let control = TurnControl::open(&intake).unwrap();
    spine.set_turn_control(control.clone());
    (spine, control, first, second, vec![ledger, intake])
}

fn threads(ended: &[richos_core::entity::ThreadBinding]) -> Vec<String> {
    ended.iter().map(|b| b.thread_id().to_string()).collect()
}

fn clean(paths: Vec<std::path::PathBuf>) {
    for p in paths {
        remove_if_present(&p);
    }
}

#[test]
fn a_typed_turn_is_reported_once_and_the_list_is_then_empty() {
    let (mut spine, _control, first, _second, paths) = two_threads("typed");
    assert!(spine.take_ended_turns().is_empty(), "positive control: nothing has run yet");
    spine.submit_prompt("land the three branches", Source::Text).unwrap();
    assert_eq!(threads(&spine.take_ended_turns()), vec![first]);
    assert!(spine.take_ended_turns().is_empty(), "taking it must empty it, or work is adopted twice");
    clean(paths);
}

#[test]
fn a_spoken_turn_is_reported_exactly_as_a_typed_one() {
    let (mut spine, _control, first, _second, paths) = two_threads("spoken");
    spine.submit_prompt_spoken("land the three branches", Source::Jam, false).unwrap();
    assert_eq!(threads(&spine.take_ended_turns()), vec![first]);
    clean(paths);
}

#[test]
fn a_phone_turn_is_reported_on_its_own_thread_and_not_on_the_active_one() {
    let (mut spine, control, first, second, paths) = two_threads("phone");
    // His phone is on the SECOND conversation while the Mac shows the first: the answer must
    // be the thread his words were for, which is what the old active-thread read got wrong.
    control.submit_from_channel(&second, Some(femcboost()), "land the three branches", "phone").unwrap();
    spine.poll_intake().unwrap();
    assert_eq!(spine.active_thread(), Some(first.as_str()), "the drain does not move the Mac");
    assert_eq!(threads(&spine.take_ended_turns()), vec![second]);
    clean(paths);
}

#[test]
fn an_idle_drain_with_nothing_on_the_log_reports_nothing() {
    let (mut spine, control, _first, second, paths) = two_threads("idle");
    spine.poll_intake().unwrap();
    assert!(spine.take_ended_turns().is_empty(), "no turn ran, so nothing may be adopted");
    // Positive control: the same fixture with one record on the log.
    control.submit_from_channel(&second, Some(femcboost()), "one message", "phone").unwrap();
    spine.poll_intake().unwrap();
    assert_eq!(threads(&spine.take_ended_turns()), vec![second]);
    clean(paths);
}

#[test]
fn one_drain_that_runs_two_conversations_reports_both_and_each_once() {
    let (mut spine, control, first, second, paths) = two_threads("two");
    control.submit_from_channel(&second, Some(femcboost()), "one for the hiring plan", "phone").unwrap();
    control.submit_from_channel(&first, Some(femcboost()), "one for the proposal", "phone").unwrap();
    control.submit_from_channel(&second, Some(femcboost()), "another for the hiring plan", "phone").unwrap();
    spine.poll_intake().unwrap();
    assert_eq!(threads(&spine.take_ended_turns()), vec![second, first]);
    clean(paths);
}

#[test]
fn what_the_boot_reconciliation_runs_is_reported_too() {
    let (mut spine, control, _first, second, paths) = two_threads("boot");
    // A phone message that outlived the last process: on the log, never drained.
    control.submit_from_channel(&second, Some(femcboost()), "sent before the Mac restarted", "phone").unwrap();
    spine.reconcile_intake().unwrap();
    assert_eq!(threads(&spine.take_ended_turns()), vec![second]);
    clean(paths);
}

#[test]
fn what_the_front_desk_priming_drains_is_reported_too() {
    let (mut spine, control, first, _second, paths) = two_threads("priming");
    // A priming turn is internal and is not his turn: priming with nothing waiting reports
    // nothing, which is the control for the line below.
    assert!(matches!(spine.prime_front_desk(&first), richos_core::spine::FrontDeskReady::Ready { .. }));
    assert!(spine.take_ended_turns().is_empty(), "the priming turn itself registers nothing of his");
    // What he sent while a desk was being primed is drained by the priming call itself.
    let (mut spine2, control2, first2, _s2, paths2) = two_threads("priming-drain");
    control2.submit_from_desk(&first2, Some(femcboost()), "land the three branches").unwrap();
    assert!(matches!(spine2.prime_front_desk(&first2), richos_core::spine::FrontDeskReady::Ready { .. }));
    assert_eq!(threads(&spine2.take_ended_turns()), vec![first2]);
    drop(control);
    clean(paths);
    clean(paths2);
}

/// A lease whose turn fails, the way a provider error ends one.
struct FailingTurn;

impl Cognition for FailingTurn {
    fn session_id(&self) -> &str {
        "failing-turn"
    }

    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        Err(CognitionError::Protocol("the provider ended the turn".into()))
    }
}

#[test]
fn a_turn_that_failed_is_still_reported_because_what_it_wrote_down_is_a_receipt() {
    let ledger = tmp_path("failed-ledger");
    let mut spine = support::spine(Ledger::open(&ledger).unwrap());
    let thread = spine.create_thread("the proposal", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(FailingTurn));
    assert!(spine.submit_prompt("land the three branches", Source::Text).is_err(), "positive control: it failed");
    assert_eq!(threads(&spine.take_ended_turns()), vec![thread]);
    clean(vec![ledger]);
}
