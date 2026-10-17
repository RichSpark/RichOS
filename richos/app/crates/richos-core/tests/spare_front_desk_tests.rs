//! **THE DESK THAT IS PRIMED BEFORE THE THREAD IT WILL SERVE EXISTS** — the CEO's ruling §55,
//! 2026-09-18, and the half of it `first-words-2026-09-18-sendlock.md` §2.2 could not move.
//!
//! *"Yes, a few seconds is fine. But 35 seconds of waiting for the first response is not. Go."*
//!
//! # The measurement these tests are about
//!
//! Run F measured a contended brand-new thread at **9.647 s** from Send to his first words, of
//! which **2.368 s** was the remainder of a priming turn his message was queued behind. The
//! sendlock slice made the Send itself instant (4 ms) and durable; it did not and could not move
//! those seconds, because the priming is a model turn and the lease is serial (continuity §3.1).
//!
//! And the real path is worse than "a remainder". `ui/main.js:1688` creates the thread INSIDE the
//! Send handler, so the prime and the send are back-to-back: he pays the WHOLE priming turn. The
//! only thing that removes it is a desk that was primed before the thread existed.
//!
//! Every test here runs headless on `MockCognition` — no provider, no network, no model turn. The
//! real-provider numbers are `examples/first_reply_timing_e2e.rs`'s job and are recorded in
//! `docs/verification/first-words-2026-09-18-primed.md`.

use richos_core::cognition::{MockCognition, MockLeaseFactory};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::spine::{SpareReady, FrontDeskReady};

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn deeply() -> EntityId {
    EntityId::parse("deeply").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-spare-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

/// **THE WHOLE POINT, AS A SEQUENCE.** The priming turn is spent while no thread exists; the
/// thread is then created; his first message finds a primed desk and no second priming turn is
/// ever run. That is the 2.368 s of run F, removed.
#[test]
fn the_priming_turn_is_spent_before_the_thread_exists_and_his_first_message_finds_it_done() {
    let (path, ledger) = tmp_ledger("primed-before-the-thread");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    let prompts = factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));

    // Nothing exists yet: no thread, no child, no turn.
    assert_eq!(spine.threads().len(), 0);
    assert_eq!(spawned.lock().unwrap().len(), 0);

    match spine.ready_a_spare_front_desk(&femcboost()) {
        SpareReady::Ready { .. } => {}
        other => panic!("the spare was not made ready: {other:?}"),
    }
    // ONE child, ONE priming turn, and still NO thread record (UX §3.3).
    assert_eq!(spawned.lock().unwrap().len(), 1, "exactly one spare child");
    assert_eq!(spawned.lock().unwrap()[0].lock().unwrap().len(), 1, "exactly one priming turn");
    assert_eq!(spine.threads().len(), 0, "§3.3: no thread record until he sends");

    // He presses Send on the entity's new-thread screen. The thread is created here, inside
    // the send handler, exactly as `ui/main.js` does it.
    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();

    // His first message. Nothing is spawned and nothing is primed a second time.
    let verdict = spine.prime_front_desk(&thread);
    assert_eq!(spawned.lock().unwrap().len(), 1, "no second child was spawned for his thread");
    assert_eq!(
        spawned.lock().unwrap()[0].lock().unwrap().len(), 1,
        "the desk was primed once, before his thread existed — not again for it"
    );
    match verdict {
        // `prepare_request` resumed the adopted desk and `prime_lease_if_needed` returned
        // early, so the "prime" cost nothing. The verdict is `Ready` rather than
        // `AlreadyReady` because the chair was empty when it was asked.
        FrontDeskReady::Ready { millis, spawned: was_spawned } => {
            assert!(!was_spawned, "the lease was adopted, not started");
            assert!(millis < 500, "a prime that ran no model turn took {millis} ms");
        }
        FrontDeskReady::AlreadyReady => {}
        other => panic!("the desk was not ready for his first message: {other:?}"),
    }

    spine.submit_prompt("Land the pricing branch.", Source::Text).unwrap();
    assert_eq!(
        prompts.lock().unwrap()[0].lock().unwrap().len(), 1,
        "his sentence went to the desk that was already standing there"
    );
    let _ = std::fs::remove_file(path);
}

/// **THE SCOPING INVARIANT, AND IT IS THE FIRST DECISION THIS SLICE HAD TO MAKE.**
///
/// `EngineProfile::scope_to` pins `(entity_id, thread_id)` into the child's environment and
/// workspace partition before it is spawned, and `engine/mega-lander/app.py:62` refuses a
/// dispatch whose binding disagrees with them. So the spare is spawned under the id the thread
/// ends up with — the scoping does NOT move, and the id is reserved instead.
#[test]
fn the_spare_is_spawned_under_the_very_id_the_thread_is_then_created_with() {
    let (path, ledger) = tmp_ledger("scoped-to-the-reserved-id");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let scoped_to = factory.scoped_to.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.ready_a_spare_front_desk(&femcboost());
    let reserved = spine.spare_front_desk_reserved_thread().unwrap().to_string();
    assert_eq!(
        scoped_to.lock().unwrap().as_slice(),
        &[("femcboost".to_string(), reserved.clone())],
        "the child was spawned scoped to the reserved id and its entity"
    );

    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();
    assert_eq!(thread, reserved, "the thread took the id its desk was already scoped to");
    assert_eq!(
        spine.ledger().thread_binding(&thread).unwrap().entity_id().as_str(),
        "femcboost"
    );
    let _ = std::fs::remove_file(path);
}

/// UX §3.3, verbatim: *"no pre-created thread record until the CEO sends the first message"*.
/// A reserved id is not a record — nothing is appended, nothing is listed, and an app that is
/// quit before he types leaves no trace of the reservation at all.
#[test]
fn a_reserved_id_writes_no_thread_record_and_a_spare_he_never_uses_leaves_none() {
    let (path, ledger) = tmp_ledger("no-record-until-he-sends");
    let mut spine = support::spine(ledger);
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["On it!"])));

    spine.ready_a_spare_front_desk(&femcboost());
    let reserved = spine.spare_front_desk_reserved_thread().unwrap().to_string();
    assert_eq!(spine.threads().len(), 0);
    assert!(spine.ledger().thread_binding(&reserved).is_err(), "a reserved id is not a thread");

    // He quits without typing. The ledger on disk is re-read from scratch: no thread.
    drop(spine);
    let reopened = Ledger::open(&path).unwrap();
    assert_eq!(reopened.threads().len(), 0, "a spare he never spoke to left no record");
    let _ = std::fs::remove_file(path);
}

/// **A SPARE PRIMED FOR ONE ENTITY IS NEVER ADOPTED BY ANOTHER** — the entity-boundary rule
/// `IntakeRecord::Steer::thread_id` states, and the re-prime payload is entity-scoped
/// (`reprime.rs`'s action digest is `ceo_facing_actions_for_entity`), so adopting across
/// entities would hand one company's Rich to another company's first sentence.
#[test]
fn a_spare_primed_for_one_company_is_never_adopted_by_another() {
    let (path, ledger) = tmp_ledger("never-across-entities");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!", "On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.ready_a_spare_front_desk(&femcboost());
    let reserved = spine.spare_front_desk_reserved_thread().unwrap().to_string();

    // He starts a thread in the OTHER company.
    let thread = spine.create_thread("Q4 board pack", &deeply()).unwrap();
    assert_ne!(thread, reserved, "Deeply's thread did not take FemcBoost's reserved id");
    assert!(
        spine.spare_front_desk_is_ready_for(&femcboost()),
        "FemcBoost's spare is still standing — starting a thread in Deeply says nothing about it"
    );
    assert_eq!(spawned.lock().unwrap().len(), 1, "no second spare was spawned by the switch");
    let _ = std::fs::remove_file(path);
}

/// The other half of the entity decision: asking for a spare in a DIFFERENT company retires the
/// one standing there rather than keeping one per company. Discard-and-re-prime, decided from
/// the idle-cost measurement in `docs/verification/first-words-2026-09-18-primed.md` §2.
#[test]
fn asking_for_a_spare_in_another_company_retires_the_one_standing_there() {
    let (path, ledger) = tmp_ledger("one-spare-only");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!", "On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.ready_a_spare_front_desk(&femcboost());
    let first = spine.spare_front_desk_reserved_thread().unwrap().to_string();
    match spine.ready_a_spare_front_desk(&deeply()) {
        SpareReady::Ready { .. } => {}
        other => panic!("the second spare was not made ready: {other:?}"),
    }
    assert!(!spine.spare_front_desk_is_ready_for(&femcboost()));
    assert!(spine.spare_front_desk_is_ready_for(&deeply()));
    assert_ne!(spine.spare_front_desk_reserved_thread().unwrap(), first);
    assert_eq!(spawned.lock().unwrap().len(), 2, "two spares were spawned, never held at once");
    let _ = std::fs::remove_file(path);
}

/// Idempotent per entity: the shell may call this on every view change, and a primed spare
/// already standing there costs nothing and sends nothing.
#[test]
fn a_second_ask_for_the_same_company_spends_nothing() {
    let (path, ledger) = tmp_ledger("idempotent");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.ready_a_spare_front_desk(&femcboost());
    let reserved = spine.spare_front_desk_reserved_thread().unwrap().to_string();
    for _ in 0..5 {
        assert_eq!(spine.ready_a_spare_front_desk(&femcboost()), SpareReady::AlreadyReady);
    }
    assert_eq!(spawned.lock().unwrap().len(), 1);
    assert_eq!(spawned.lock().unwrap()[0].lock().unwrap().len(), 1, "one priming turn, not six");
    assert_eq!(spine.spare_front_desk_reserved_thread().unwrap(), reserved);
    let _ = std::fs::remove_file(path);
}

/// Priming is NEVER run underneath a turn. The spare's turn is on its own child, so it breaks no
/// continuity-§3.1 invariant — but the caller holds the spine's mutex for the whole of it, and a
/// second multi-second hold in front of a running turn is what `FrontDeskReady::TurnInProgress`
/// already refuses. Same answer, same reason.
#[test]
fn a_spare_is_never_primed_underneath_a_running_turn() {
    let (path, ledger) = tmp_ledger("never-under-a-turn");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.debug_set_turn_in_progress(true);

    assert_eq!(spine.ready_a_spare_front_desk(&femcboost()), SpareReady::TurnInProgress);
    assert_eq!(spawned.lock().unwrap().len(), 0, "nothing was spawned under a turn");
    assert!(spine.spare_front_desk_reserved_thread().is_none());
    let _ = std::fs::remove_file(path);
}

/// An unregistered entity is refused here exactly as it is everywhere else (ECS §3.3) — a spare
/// is a child scoped to a company, so inventing one would be the same fault with a process
/// attached to it.
#[test]
fn an_unregistered_company_gets_no_spare() {
    let (path, ledger) = tmp_ledger("unregistered");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    match spine.ready_a_spare_front_desk(&EntityId::parse("acme-holdings").unwrap()) {
        SpareReady::NotReady(why) => assert!(why.contains("acme-holdings"), "{why}"),
        other => panic!("an unregistered company got a spare: {other:?}"),
    }
    assert_eq!(spawned.lock().unwrap().len(), 0);
    let _ = std::fs::remove_file(path);
}

/// **AN UN-PRIMED SPARE IS ADOPTED, NOT THROWN AWAY.** The central folder moving un-primes every
/// front desk; a spare is the one primed EARLIEST, so it is the likeliest to be holding material
/// the app has stopped believing in. Its child is still correctly scoped, so it takes the chair
/// and primes at his first message — which is what every thread did before spares existed. The
/// degrade is the old path, never a new failure.
#[test]
fn a_spare_the_app_has_stopped_believing_in_still_takes_the_chair_and_primes_on_his_first_message() {
    let (path, ledger) = tmp_ledger("unprimed-spare");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.ready_a_spare_front_desk(&femcboost());
    let reserved = spine.spare_front_desk_reserved_thread().unwrap().to_string();
    assert_eq!(spawned.lock().unwrap()[0].lock().unwrap().len(), 1);

    // The company material moved. Every desk is un-primed, the spare included.
    spine.set_central_root(std::env::temp_dir().join("richos-spare-test-central"));
    assert!(!spine.spare_front_desk_is_ready_for(&femcboost()));

    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();
    assert_eq!(thread, reserved, "it was still adopted — the child is correctly scoped");
    spine.prime_front_desk(&thread);
    assert_eq!(spawned.lock().unwrap().len(), 1, "no second child: the same desk primed again");
    assert_eq!(
        spawned.lock().unwrap()[0].lock().unwrap().len(), 2,
        "it primed a second time, on his clock — the old path, which is the honest degrade"
    );
    let _ = std::fs::remove_file(path);
}

/// **THE SPARE'S SESSION-START CHATTER NEVER REACHES A TURN THE CEO IS WATCHING.**
///
/// A fresh child re-announces its commands on the between-turn lane at session start, with no
/// turn in flight to own them. The chair's priming drains that lane and stamps it
/// `internal: true` (`prime_lease_if_needed`'s last lines). The spare's binding cannot be
/// verified — its thread has no record — so `pump_between_turn_stamped` refuses it and leaves
/// those records sitting in the lease, where the FIRST VISIBLE TURN drains them at
/// `spine.rs:2332` with `internal: false` and stamps them against a turn he is looking at. That
/// is the rotation-tell shape §1.5 exists to exclude, arriving by a new road.
///
/// **A POSITIVE PROBE, not a negative that passes because the double is silent.**
/// `every_child_announces_itself` parks a real frame on every spawned lease, exactly as a live
/// child does, so removing the drain in `prime_the_spare` makes this test fail with the record
/// in hand.
#[test]
fn the_spares_own_session_start_chatter_is_dropped_and_never_lands_in_his_first_turn() {
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Seen(Mutex<Vec<(String, Option<String>, bool)>>);
    impl richos_core::machinery::MachineryObserver for Seen {
        fn on_machinery(&self, record: &richos_core::machinery::MachineryRecord) {
            self.0.lock().unwrap().push((
                record.title.clone(), record.turn_id.clone(), record.internal,
            ));
        }
    }

    let (path, ledger) = tmp_ledger("residue-dropped");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    // The announcement a real child makes at session start, in the vendor's own frame shape.
    factory.every_child_announces_itself(serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [{"type": "text", "text": "commands reloaded"}]}
    }));
    spine.set_lease_factory(Box::new(factory));
    let seen = Arc::new(Seen::default());
    struct Forward(Arc<Seen>);
    impl richos_core::machinery::MachineryObserver for Forward {
        fn on_machinery(&self, record: &richos_core::machinery::MachineryRecord) {
            self.0.on_machinery(record);
        }
    }
    spine.set_machinery_observer(Box::new(Forward(seen.clone())));

    spine.ready_a_spare_front_desk(&femcboost());
    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();
    spine.prime_front_desk(&thread);
    spine.submit_prompt("Land the pricing branch.", Source::Text).unwrap();

    let records = seen.0.lock().unwrap().clone();
    assert!(
        !records.iter().any(|(title, _, internal)| title == "assistant:text" && !*internal),
        "the spare's session-start chatter reached a turn the CEO can see: {records:#?}"
    );
    assert!(
        !records.iter().any(|(title, turn, _)| title == "assistant:text" && turn.is_some()),
        "it was attached to one of his turns: {records:#?}"
    );

    // And what he CAN see is his own sentence and its reply, and nothing else.
    let messages = spine.messages(&thread).unwrap();
    assert_eq!(messages.len(), 2, "one CEO message and one reply: {messages:#?}");
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let turns = spine.ledger().thread_turns_scoped(&binding).unwrap();
    assert_eq!(turns.iter().filter(|t| t.source == Source::Text).count(), 1, "one visible turn");
    assert_eq!(
        turns.iter().filter(|t| t.source == Source::Internal).count(), 0,
        "the spare's priming was recorded as an ACTION, never as an Internal turn against a \
         thread that did not exist when it ran"
    );
    assert_eq!(spawned.lock().unwrap().len(), 1);
    let _ = std::fs::remove_file(path);
}

/// The durable record that the spare WAS primed — the fact the whole saving rests on — is an
/// Internal action naming the reserved id, so a later reader can join it to the thread it became.
/// Internal, therefore never in the CEO-facing digest and never rendered.
#[test]
fn the_spares_priming_is_durably_recorded_and_is_never_ceo_facing() {
    let (path, ledger) = tmp_ledger("durable-action");
    let mut spine = support::spine(ledger);
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["On it!"])));

    spine.ready_a_spare_front_desk(&femcboost());
    let reserved = spine.spare_front_desk_reserved_thread().unwrap().to_string();

    let primed: Vec<_> = spine.ledger().actions().iter()
        .filter(|a| a.kind == "spare_front_desk_reprime").collect();
    assert_eq!(primed.len(), 1, "one priming, durably recorded");
    assert!(primed[0].detail.contains(&reserved), "it names the id it was scoped to");
    assert!(primed[0].detail.contains("femcboost"), "and the company it was primed for");
    assert_eq!(primed[0].status, richos_core::ledger::ActionStatus::Completed);
    assert!(
        !spine.ledger().ceo_facing_actions_for_entity(&femcboost()).iter()
            .any(|a| a.kind == "spare_front_desk_reprime"),
        "machinery is never in the digest that is asserted to a successor as ground truth"
    );
    let _ = std::fs::remove_file(path);
}

/// A reserved id is spent exactly once. Ids are uuid v4, so a second spend would mean the
/// reservation was used twice — two conversations in one workspace partition — and the ledger
/// refuses rather than quietly picking another id.
#[test]
fn a_reserved_id_can_be_spent_exactly_once() {
    let (path, ledger) = tmp_ledger("spent-once");
    let mut ledger = ledger;
    let reserved = Ledger::reserve_thread_id();
    let id = ledger.create_thread_with_reserved_id(&reserved, "Pricing", &femcboost()).unwrap();
    assert_eq!(id, reserved);
    let again = ledger.create_thread_with_reserved_id(&reserved, "Pricing again", &femcboost());
    assert!(matches!(again, Err(richos_core::ledger::LedgerError::ThreadIdAlreadySpent(_))), "{again:?}");
    assert_eq!(ledger.threads().len(), 1, "nothing was appended by the refused call");
    let _ = std::fs::remove_file(path);
}

/// With no lease factory the app cannot spawn anything, so it says so and the next brand-new
/// thread primes the way it does today. Nothing is surfaced to the CEO and nothing is retried.
#[test]
fn a_build_that_cannot_spawn_a_lease_says_so_and_costs_him_nothing() {
    let (path, ledger) = tmp_ledger("no-factory");
    let mut spine = support::spine(ledger);
    match spine.ready_a_spare_front_desk(&femcboost()) {
        SpareReady::NotReady(_) => {}
        other => panic!("a spine with no factory claimed a spare: {other:?}"),
    }
    // And the ordinary path is untouched: his thread is created and primed exactly as before.
    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));
    spine.prime_front_desk(&thread);
    assert_eq!(reprimes.lock().unwrap().len(), 1);
    let _ = std::fs::remove_file(path);
}

/// **THE DEFECT RUN H BOUGHT, AS A TEST THAT COSTS NOTHING.**
///
/// Run H (`docs/verification/first-words-2026-09-18-primed.md` §3) primed a spare in 4.709 s,
/// watched his thread adopt it (`spawned: false`), and then measured a SECOND priming turn of
/// 2.123 s on his own clock. The saving was zero and the spare's turn was thrown away.
///
/// The cause is one line in `prime_lease_if_needed`:
/// `if onboarding_block != self.onboarding_primed_block { self.lease_primed = false; }`.
/// `onboarding_primed_block` was a SPINE-WIDE field describing whatever desk was last in the
/// chair — and a spare primes while no desk is in the chair at all, so the field still read
/// `None` while the adopted desk was holding the company block. The comparison un-primed a desk
/// that was perfectly primed.
///
/// **The tests above did not catch it because none of them set a central root**, so the block
/// was `None` on both sides and the comparison passed for the wrong reason. This one sets one,
/// which is the shipping state of any install whose owner has a company folder.
#[test]
fn a_spare_primed_with_the_company_material_is_not_re_primed_when_his_thread_adopts_it() {
    let (path, ledger) = tmp_ledger("company-block-travels");
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    // A real install: the CEO's central folder is known, and the declination record with it.
    // Both are set BEFORE the spare is readied, because each un-primes every desk.
    let central = std::env::temp_dir().join(format!(
        "richos-spare-central-{}-{}", std::process::id(), richos_core::util::now_millis()
    ));
    std::fs::create_dir_all(central.join("femcboost")).unwrap();
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(central.join("onboarding.json"));

    spine.ready_a_spare_front_desk(&femcboost());
    assert_eq!(spawned.lock().unwrap()[0].lock().unwrap().len(), 1, "the spare primed once");

    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();
    spine.prime_front_desk(&thread);

    assert_eq!(spawned.lock().unwrap().len(), 1, "no second child");
    assert_eq!(
        spawned.lock().unwrap()[0].lock().unwrap().len(), 1,
        "the adopted desk was primed a SECOND time on his clock — the company material it was \
         primed with did not travel with it, which is exactly what run H measured"
    );
    let _ = std::fs::remove_dir_all(central);
    let _ = std::fs::remove_file(path);
}

// =========================================================================================
// THE SIX SECONDS: A SPARE'S SPAWN AND PRIMING TURN MUST NOT HOLD THE SPINE
// =========================================================================================
//
// **The measurement these three tests exist for.** Ray's candidate-.12 re-walk put "On it!" on
// the real window at 14.24 s with the turn's own header reading "Working for 8s", so ~6 s went
// somewhere before the turn's clock started
// (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.1-mac-and-android-rewalk-audit.md`
// §2). The cause is in that walk's own committed log, in the ORDER of four lines: the spare
// primed for 5378 ms, and only AFTER it did `the front desk is ready before he types: 0 ms`
// appear — the timeline read that opens his new thread had been queued behind it.
// `create_thread_in` asks for the next spare microseconds before the window issues eight more
// bridge calls, and every one of them takes the same mutex the priming turn was holding.
//
// Re-measured on the real window on 2026-09-19 with per-hop instrumentation, on a debug build
// whose spare costs 18.3 s rather than 5.4 s:
//
//     [richos] a front desk is primed and waiting for the next new thread in sixsec-co: 18318 ms
//     [richos] a window command waited 18314 ms for the spine at src/main.rs:3964
//     [richos] a window command waited 18028 ms for the spine at src/main.rs:746
//     [richos] the front desk is ready before he types: 0 ms
//
// and the turn's own header first read "Working for 1s" at t = 19.61 s after the keystroke.
//
// These tests are the headless, deterministic form of that, and the first of them is the
// POSITIVE PROBE: it asserts the old road really does hold the mutex, so a green result from
// the second one is evidence rather than a test that cannot fail.

/// A gate two threads can hand a moment through.
struct Gate {
    open: std::sync::Mutex<bool>,
    cv: std::sync::Condvar,
}

impl Gate {
    fn new() -> Self {
        Gate { open: std::sync::Mutex::new(false), cv: std::sync::Condvar::new() }
    }
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.cv.notify_all();
    }
    /// Waits, and fails the test rather than hanging the suite for ever.
    fn wait(&self, what: &str) {
        let guard = self.open.lock().unwrap();
        let (guard, timeout) = self
            .cv
            .wait_timeout_while(guard, std::time::Duration::from_secs(10), |open| !*open)
            .unwrap();
        assert!(!timeout.timed_out(), "timed out waiting for {what}");
        drop(guard);
    }
}

/// A desk whose priming turn takes exactly as long as the test wants, and says when it started.
struct SlowDesk {
    session: String,
    started: std::sync::Arc<Gate>,
    release: std::sync::Arc<Gate>,
}

impl richos_core::Cognition for SlowDesk {
    fn session_id(&self) -> &str {
        &self.session
    }
    fn reprime(
        &mut self,
        _priming_text: &str,
        _on_item: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<(), richos_core::cognition::CognitionError> {
        self.started.open();
        self.release.wait("the test to release the priming turn");
        Ok(())
    }
    fn prompt(
        &mut self,
        _text: &str,
        _on_item: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<String, richos_core::cognition::CognitionError> {
        Ok("end_turn".to_string())
    }
}

/// A factory that CAN hand out a second handle, which is what the unlocked road needs.
#[derive(Clone)]
struct SlowFactory {
    started: std::sync::Arc<Gate>,
    release: std::sync::Arc<Gate>,
    spawns: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl richos_core::cognition::LeaseFactory for SlowFactory {
    fn spawn(&self) -> Result<Box<dyn richos_core::Cognition>, richos_core::cognition::CognitionError> {
        let n = self.spawns.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Box::new(SlowDesk {
            session: format!("slow-{n}"),
            started: self.started.clone(),
            release: self.release.clone(),
        }))
    }

}

fn a_slow_spine(tag: &str) -> (std::path::PathBuf, std::sync::Arc<std::sync::Mutex<richos_core::spine::Spine>>, SlowFactory) {
    let (path, ledger) = tmp_ledger(tag);
    let mut spine = support::spine(ledger);
    let factory = SlowFactory {
        started: std::sync::Arc::new(Gate::new()),
        release: std::sync::Arc::new(Gate::new()),
        spawns: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    spine.set_lease_factory(Box::new(factory.clone()));
    (path, std::sync::Arc::new(std::sync::Mutex::new(spine)), factory)
}

/// **THE POSITIVE PROBE.** The old road holds the spine for the whole priming turn.
///
/// Without this, the test below would pass just as happily against a spare that never primed
/// at all, or against a `try_lock` that always succeeds for some unrelated reason. This is the
/// half that proves the instrument is alive on this run — the discipline
/// `feedback_negative_tests_pass_for_wrong_reason` names, and the one `gui-boot.test.sh`'s
/// B3-B8 exist for.
#[test]
fn the_locked_road_holds_the_spine_across_the_priming_turn_which_is_what_he_waited_for() {
    let (path, spine, factory) = a_slow_spine("locked-road-holds");
    let priming = std::thread::spawn({
        let spine = std::sync::Arc::clone(&spine);
        move || spine.lock().unwrap().ready_a_spare_front_desk(&femcboost())
    });
    factory.started.wait("the priming turn to start");
    assert!(
        spine.try_lock().is_err(),
        "the &mut self road is supposed to hold the mutex for the whole of the priming turn — \
         if it does not, the measurement below proves nothing"
    );
    factory.release.open();
    match priming.join().unwrap() {
        SpareReady::Ready { .. } => {}
        other => panic!("the spare was not made ready: {other:?}"),
    }
    let _ = std::fs::remove_file(path);
}

/// **AND THE ROAD THE APP TAKES DOES NOT.** His window is served in microseconds while a model
/// turn runs on a child he has never seen.
#[test]
fn the_window_is_served_while_the_spare_is_spawning_and_priming() {
    let (path, spine, factory) = a_slow_spine("window-is-served");
    let priming = std::thread::spawn({
        let spine = std::sync::Arc::clone(&spine);
        move || richos_core::spine::ready_a_spare_front_desk_without_the_spine(&spine, &femcboost())
    });
    factory.started.wait("the priming turn to start");

    // THE CLAIM, in the form the window makes it: a bridge command arriving in the middle of a
    // spare's priming turn takes the spine and answers. `switch_thread`, `get_timeline`,
    // `navigation_tree`, `onboarding_view` are all this call.
    {
        let served = spine.try_lock();
        assert!(
            served.is_ok(),
            "a window command would have queued behind the spare's priming turn — that is the \
             six seconds Ray measured on candidate .12"
        );
        // And it is a working spine, not merely an unlocked one.
        assert_eq!(served.unwrap().threads().len(), 0);
    }

    factory.release.open();
    match priming.join().unwrap() {
        SpareReady::Ready { .. } => {}
        other => panic!("the spare was not made ready: {other:?}"),
    }
    assert!(
        spine.lock().unwrap().spare_front_desk_is_ready_for(&femcboost()),
        "the spare has to be standing, primed, when the unlocked road returns Ready"
    );
    assert_eq!(factory.spawns.load(std::sync::atomic::Ordering::SeqCst), 1, "exactly one child");
    let _ = std::fs::remove_file(path);
}

/// **AND WHAT THE OPEN WINDOW COSTS: the app may stop believing in the material while the
/// priming turn is still running, and the finished desk must not be handed over as primed.**
///
/// `unprime_every_front_desk` reaches every desk it can see. A spare mid-prime is in no field
/// it can see — that is the whole point of the unlocked road — so the generation is what
/// catches it. Without the generation this test installs a `primed: true` desk carrying
/// material the app discarded, silently, which is the defect that method's own documentation
/// exists to prevent.
#[test]
fn a_spare_primed_against_a_world_that_moved_is_kept_but_never_called_primed() {
    let (path, spine, factory) = a_slow_spine("world-moved");
    let priming = std::thread::spawn({
        let spine = std::sync::Arc::clone(&spine);
        move || richos_core::spine::ready_a_spare_front_desk_without_the_spine(&spine, &femcboost())
    });
    factory.started.wait("the priming turn to start");

    // The central folder moving is one of the things that un-primes every desk, and it is a
    // command the window can issue at any moment — including this one.
    spine.lock().unwrap().set_central_root(std::env::temp_dir().join("richos-spare-world-moved"));

    factory.release.open();
    match priming.join().unwrap() {
        SpareReady::NotReady(why) => assert!(
            why.contains("un-primed"),
            "the verdict has to say what happened rather than looking like a spawn failure: {why}"
        ),
        other => panic!("a spare primed against discarded material must not report Ready: {other:?}"),
    }
    let spine = spine.lock().unwrap();
    assert!(
        !spine.spare_front_desk_is_ready_for(&femcboost()),
        "it must not be offered as primed"
    );
    assert!(
        spine.spare_front_desk_reserved_thread().is_some(),
        "and it must be KEPT rather than killed — the child is alive and correctly scoped, and \
         his first message primes it the way every thread did before spares existed"
    );
    let _ = std::fs::remove_file(path);
}

/// What a desk was asked to do, in order, on every child the factory spawned.
type Calls = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// A desk that only records: its priming turn, the context brief supplied ahead and the
/// thread it was supplied for, and his turns.
struct BriefDesk {
    session: String,
    calls: Calls,
}

impl richos_core::Cognition for BriefDesk {
    fn session_id(&self) -> &str {
        &self.session
    }
    fn reprime(
        &mut self,
        _priming_text: &str,
        _on_item: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<(), richos_core::cognition::CognitionError> {
        self.calls.lock().unwrap().push("reprime".into());
        Ok(())
    }
    fn supply_context_ahead(
        &mut self,
        binding: &richos_core::entity::ThreadBinding,
        _turn: &str,
        _on_item: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<(), richos_core::cognition::CognitionError> {
        self.calls.lock().unwrap().push(format!("brief ahead for {}", binding.thread_id()));
        Ok(())
    }
    fn prompt(
        &mut self,
        _text: &str,
        _on_item: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<String, richos_core::cognition::CognitionError> {
        self.calls.lock().unwrap().push("his turn".into());
        Ok("end_turn".to_string())
    }
}

#[derive(Clone)]
struct BriefFactory {
    calls: Calls,
}

impl richos_core::cognition::LeaseFactory for BriefFactory {
    fn spawn(&self) -> Result<Box<dyn richos_core::Cognition>, richos_core::cognition::CognitionError> {
        Ok(Box::new(BriefDesk { session: "brief-desk".into(), calls: self.calls.clone() }))
    }

}

/// **THE ADOPTED SPARE HOLDS HIS CONTEXT BRIEF BEFORE HE SPEAKS** (CEO 2026-10-14: "From barge
/// in to a 10-second delay now?"; review rv-20261014T073429Z-4fd01c4c-58c4). A new conversation
/// adopts the spare primed at boot, and the adopted desk keeps `primed`, so the chair's priming
/// returns before it supplies the brief. Unless the spare supplied it while being primed, his
/// first turn sends it as a hidden model round trip while he waits (1.949 s on main 0a2ae649f).
/// It is supplied ONCE, for the id the thread is then created with, and before his turn.
#[test]
fn an_adopted_spare_was_given_his_context_brief_while_it_was_primed() {
    let (path, ledger) = tmp_ledger("adopted-spare-brief-ahead");
    let mut spine = support::spine(ledger);
    let calls: Calls = Default::default();
    spine.set_lease_factory(Box::new(BriefFactory { calls: calls.clone() }));
    let spine = std::sync::Arc::new(std::sync::Mutex::new(spine));

    // The road the app takes (`main.rs` `ready_a_spare_front_desk`).
    match richos_core::spine::ready_a_spare_front_desk_without_the_spine(&spine, &femcboost()) {
        SpareReady::Ready { .. } => {}
        other => panic!("the spare was not made ready: {other:?}"),
    }
    let mut spine = spine.lock().unwrap();
    let thread = spine.create_thread("Pricing", &femcboost()).unwrap();
    spine.prime_front_desk(&thread);
    spine.submit_prompt("Land the pricing branch.", Source::Text).unwrap();

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &["reprime".to_string(), format!("brief ahead for {thread}"), "his turn".to_string()],
        "the adopted spare must be given the brief once, for his thread, before his turn"
    );
    let _ = std::fs::remove_file(path);
}
