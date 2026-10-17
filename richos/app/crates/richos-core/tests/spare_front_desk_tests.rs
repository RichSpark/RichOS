//! A prepared native connection retains its company and reserved conversation identity.
//! Opening it spends no hidden model call. Native session regressions cover that boundary.
use richos_core::{cognition::{MockCognition,MockLeaseFactory},entity::EntityId,ledger::Ledger,spine::SpareReady};
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
    assert!(spawned.lock().unwrap()[0].lock().unwrap().is_empty(), "opening a connection spent a hidden model turn");
    assert_eq!(spine.spare_front_desk_reserved_thread().unwrap(), reserved);
    let _ = std::fs::remove_file(path);
}

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
    assert!(reprimes.lock().unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}
