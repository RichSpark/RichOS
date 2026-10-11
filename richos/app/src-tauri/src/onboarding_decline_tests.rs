//! **"Not now" on the interview card never waits for boot priming.**
//!
//! The defect, measured in the built app on 2026-10-11 (`docs/verification/
//! 2026-10-11-preferences-close-walk/README.md`, steps 16 and 17): `prime_front_desk` held the
//! spine for 98737 ms at boot, and `decline_onboarding` asked for it at +7898 ms and got it at
//! +98797 ms, so the card stayed on screen for 98797 - 7898 = 90899 ms after he pressed it.
//!
//! Here the spine is held by the test, standing in for that prime, and the decline must
//! answer, with the decision on disk, inside [`DECLINE_DEADLINE`]. Nothing else is owed to the
//! desks: each one compares the block it was primed with against the record before every
//! message, so its next message carries the decline (`richos-core`'s
//! `onboarding_declination_tests.rs`, every kind of desk).
use super::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The card answers his tap at once: well under a second, with the spine held.
const DECLINE_DEADLINE: Duration = Duration::from_secs(1);

#[test]
fn not_now_answers_and_is_recorded_while_boot_priming_holds_the_spine() {
    let dir = std::env::temp_dir().join(format!("not-now-while-priming-{}", std::process::id()));
    if let Err(error) = std::fs::remove_dir_all(&dir) {
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
    let central = dir.join("central");
    std::fs::create_dir_all(&central).unwrap();
    let record = dir.join("onboarding.json");

    let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(EntityRegistry::new(vec![
        Entity::new("acme", "Acme", &["/fixture/acme"]).unwrap(),
    ]).unwrap());
    let acme = EntityId::parse("acme").unwrap();
    let thread = spine.create_thread("Acme's work", &acme).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.set_central_root(central);
    spine.set_onboarding_record(record.clone());
    let reader = spine.reader();
    assert_eq!(onboarding_view_in(&reader).state, "not-yet", "the card is offered before he answers");
    let spine = Mutex::new(spine);

    // The stand-in for `prime_front_desk`: the spine taken and held, as the boot prime holds it.
    let held = spine.lock().unwrap();

    let (tx, rx) = mpsc::channel();
    let decline_reader = reader.clone();
    let sent = Instant::now();
    let decline = std::thread::spawn(move || {
        let answer = decline_onboarding_in(&decline_reader, "acme", 1_000);
        tx.send((answer.map(|view| view.state), sent.elapsed())).unwrap();
    });
    let answered = rx.recv_timeout(DECLINE_DEADLINE);
    let on_disk = richos_core::onboarding::OnboardingRecord::load(&record).for_entity(&acme).declined_at_millis();

    // Always let go and join, including on a red build, so nothing is left waiting.
    drop(held);
    decline.join().unwrap();
    let answered = answered.unwrap_or_else(|_| rx.recv().unwrap());
    let (state, took) = answered;
    eprintln!("[not-now] the decline answered in {} ms with the spine held", took.as_millis());
    assert!(
        took < DECLINE_DEADLINE,
        "\"Not now\" took {} ms to answer while priming held the spine; it waited for the prime",
        took.as_millis()
    );
    assert_eq!(state, Ok("declined".to_string()), "the card's answer is the declined state");
    assert_eq!(on_disk, Some(1_000), "the decision is on disk before the spine is free");
    drop(spine);
    std::fs::remove_dir_all(&dir).unwrap();
}
