//! **The front desk is ready BEFORE he types** — the CEO's ruling §55, 2026-09-18, and the half
//! of it that is not about the reply.
//!
//! *"Yes, a few seconds is fine. But 35 seconds of waiting for the first response is not. Go."*
//!
//! # The measurement these tests are about
//!
//! `docs/verification/first-reply-2026-09-18.md` measured the lease's FIRST visible turn at
//! **13.464 s / 13.628 s / 15.513 s** and its warm turns at **7.128 s / 8.505 s / 8.920 s**, and
//! attributed the ~8 s difference to "the lease waking up while he waits". The code says what that
//! is, exactly: `Spine::prime_lease_if_needed` runs `Cognition::reprime` — a real model turn — and
//! it is called from `prepare_request`, which runs INSIDE `submit_prompt`. His first message pays
//! for the priming of the lease that is about to answer it.
//!
//! So the seconds are not a process start, they are a TURN, and the only thing that can be done
//! about them is to spend that turn before he is waiting on it. That is `prime_front_desk`, and
//! these are its invariants. Every one of them runs headless on `MockCognition` — no provider, no
//! network, no model turn. The real-provider numbers are
//! `examples/first_reply_timing_e2e.rs`'s job.

use richos_core::cognition::{MockCognition, MockLeaseFactory};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::spine::FrontDeskReady;

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-priming-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

#[test]
fn the_priming_turn_happens_before_he_types_and_his_first_message_no_longer_pays_for_it() {
    // **THE WHOLE POINT, AS A SEQUENCE.** Before this call the lease has never been primed;
    // after it, it has — and his first message runs on a primed desk, which is the ~8 s that
    // used to sit in front of his first words.
    let (path, ledger) = tmp_ledger("primed-before-he-types");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    let prompts = lease.prompts.clone();
    spine.attach_lease(Box::new(lease));

    // Nothing has been sent to the provider yet, in either lane.
    assert!(reprimes.lock().unwrap().is_empty());
    assert!(prompts.lock().unwrap().is_empty());

    match spine.prime_front_desk(&thread) {
        FrontDeskReady::Ready { spawned, .. } => {
            // `attach_lease` supplied the lease, so nothing was spawned here; the turn is the cost.
            assert!(!spawned, "this lease was attached, not spawned by the priming call");
        }
        other => panic!("the desk should have been made ready: {other:?}"),
    }
    // ONE priming turn, and it carries the re-prime payload rather than a greeting.
    assert_eq!(reprimes.lock().unwrap().len(), 1, "priming is one turn, not a loop");
    assert!(prompts.lock().unwrap().is_empty(), "priming must not put a visible prompt in his thread");

    // **AND HIS MESSAGE DOES NOT PRIME AGAIN.** This is the assertion that makes the timing
    // claim true rather than merely plausible: if priming still happened on his turn, the
    // re-prime count would be 2 here and his first words would still be behind it.
    let turn = spine.submit_prompt("Land the pricing branch and get the staging deploy done.", Source::Text).unwrap();
    assert_eq!(reprimes.lock().unwrap().len(), 1, "his first message re-primed a desk that was ready");
    assert_eq!(prompts.lock().unwrap().len(), 1);
    assert_eq!(spine.ledger().turn(&turn).unwrap().assistant_text, "On it!");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn priming_a_desk_that_is_already_ready_spends_nothing_and_sends_nothing() {
    // Launch, then a thread open, then a second thread open is three calls and one turn. An
    // idempotence that cost a model turn each time would be a worse defect than the one this
    // whole slice removes.
    let (path, ledger) = tmp_ledger("already-ready");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));

    assert!(matches!(spine.prime_front_desk(&thread), FrontDeskReady::Ready { .. }));
    for _ in 0..3 {
        assert_eq!(spine.prime_front_desk(&thread), FrontDeskReady::AlreadyReady);
    }
    assert_eq!(reprimes.lock().unwrap().len(), 1, "an already-ready desk must cost nothing");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn priming_spawns_the_lease_when_there_is_none_which_is_what_a_launch_looks_like() {
    // The app's real launch shape: no lease at all until something asks for one
    // (`prepare_request` spawns from the lease factory on his FIRST message today). With this
    // call at launch, the spawn AND the priming turn are both behind him before he types.
    let (path, ledger) = tmp_ledger("spawn-at-launch");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    let factory = MockLeaseFactory::new(vec!["On it!"]);
    let spawned_reprimes = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    match spine.prime_front_desk(&thread) {
        FrontDeskReady::Ready { spawned, .. } => assert!(spawned, "there was no lease to prime"),
        other => panic!("a launch with a factory should reach a ready desk: {other:?}"),
    }
    let spawned = spawned_reprimes.lock().unwrap();
    assert_eq!(spawned.len(), 1, "exactly one lease was spawned");
    assert_eq!(spawned[0].lock().unwrap().len(), 1, "and it was primed once");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_thread_that_does_not_exist_is_refused_rather_than_created_or_guessed() {
    // Fails closed, and says which fact is missing. A priming call that CREATED a thread would
    // put a thread in his sidebar because the app started.
    let (path, ledger) = tmp_ledger("no-such-thread");
    let mut spine = support::spine(ledger);
    let before = spine.threads().len();
    let verdict = spine.prime_front_desk("thr_nothing_here");
    match verdict {
        FrontDeskReady::NotReady(why) => assert!(!why.is_empty(), "the refusal must name the fault"),
        other => panic!("an unknown thread must be refused: {other:?}"),
    }
    assert_eq!(spine.threads().len(), before, "priming must never create a thread");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn priming_is_never_run_underneath_a_turn() {
    // **THE CONTINUITY INVARIANT, IN ITS SMALLEST FORM** (design §3.1): the lease is the thing
    // running his turn, and a second prompt into it mid-turn is exactly what must never happen.
    // A launch-time optimization is not worth a millisecond of that, so the answer is a verdict
    // and not a wait.
    let (path, ledger) = tmp_ledger("never-under-a-turn");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));
    spine.debug_set_turn_in_progress(true);
    assert_eq!(spine.prime_front_desk(&thread), FrontDeskReady::TurnInProgress);
    assert!(reprimes.lock().unwrap().is_empty(), "nothing may be sent while a turn is in flight");
    spine.debug_set_turn_in_progress(false);
    let _ = std::fs::remove_file(&path);
}

/// **`Ready.spawned` NAMES A CHILD THAT WAS STARTED, NEVER A CHAIR THAT WAS EMPTY.**
///
/// The shell prints this flag straight into `app.log` — *"the front desk is ready before he
/// types: {millis} ms, including the lease's own start"* — so it is a number a record is written
/// from two days later, which is exactly what §3 of `first-words-2026-09-18-sendlock.md` is about
/// happening to `Ready.millis`.
///
/// It was `self.lease.is_none()` taken before `prepare_request`, and that answered "was the chair
/// empty". The two agreed only while an empty chair could be filled in one way. A spare front
/// desk fills one WITHOUT starting anything (`Spine::create_thread` files it as a resident), and
/// the old reading called that a spawn. Both halves are pinned here: a genuine spawn says `true`,
/// an adoption says `false`, and the same code path produces both — restore `self.lease.is_none()`
/// and the second half fails alone.
#[test]
fn the_ready_verdict_says_spawned_only_when_a_child_was_actually_started() {
    use richos_core::cognition::MockLeaseFactory;

    // (a) A genuinely empty chair with no desk anywhere: a child IS started.
    let (path, ledger) = tmp_ledger("spawned-true");
    let mut spine = support::spine(ledger);
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["On it!"])));
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    match spine.prime_front_desk(&thread) {
        FrontDeskReady::Ready { spawned, .. } => assert!(spawned, "a child was started and it said so"),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_file(path);

    // (b) An empty chair and a primed spare standing beside it: NOTHING is started. The chair
    // was just as empty as in (a) — which is the whole reason emptiness cannot be the witness.
    let (path, ledger) = tmp_ledger("spawned-false");
    let mut spine = support::spine(ledger);
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["On it!"])));
    spine.ready_a_spare_front_desk(&femcboost());
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    assert!(!spine.has_lease(), "the chair is empty here too");
    match spine.prime_front_desk(&thread) {
        FrontDeskReady::Ready { spawned, .. } =>
            assert!(!spawned, "the desk was adopted, not started — saying otherwise is a wrong \
                               number in app.log and then in a record"),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_file(path);
}

/// **A DESK WHOSE PRIMING FAILED FOR WANT OF A LEASE IS READY BEFORE HIS NEXT MESSAGE, ONCE
/// SOMETHING ASKS AGAIN** — Ray's candidate-.11 defect 2.1, as behavior.
///
/// The shape he walked, `docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md`
/// §2: a Mac with no engine on it, so every hook that reaches `spawn_scoped` fails and nothing
/// is primed; then the engine is installed, which fixes the CAUSE and fires none of those hooks
/// again; then his first message pays the whole priming turn — his turn starting at ~+11 s and
/// "On it!" reaching the screen at **+18.3 s**.
///
/// **THIS TEST IS THE BEHAVIORAL HALF AND IT IS GREEN ON `05ab7a0c` TOO, WHICH IS THE HONEST
/// STATEMENT.** Nothing in the spine was broken: priming a desk after the machine is repaired
/// has always worked, and the positive control on his own walk was his second message starting
/// its turn at +1.0 s. What was missing is the ASK, and the ask is the shell's
/// (`src-tauri/src/main.rs`, `run_setup` → `ready_the_front_desk_after_a_repair`), pinned by
/// `repair_priming_tests` there. This pins what that ask is WORTH: one priming turn spent off
/// his wait, and a first message that spends none.
#[test]
fn a_prime_that_failed_for_want_of_a_lease_is_free_to_his_next_message_once_it_is_asked_again() {
    let (path, ledger) = tmp_ledger("repaired-after-a-failed-prime");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Running", &femcboost()).unwrap();
    let factory = MockLeaseFactory::new(vec!["On it!", "On it!"]);
    let spawned = factory.spawned.clone();
    // THE MACHINE THE OFFER IS SHOWN ON: nothing to start `claude` in, so the spawn fails.
    factory.fail_next_spawn();
    spine.set_lease_factory(Box::new(factory));

    match spine.prime_front_desk(&thread) {
        FrontDeskReady::NotReady(why) => assert!(!why.is_empty(), "the refusal must name the fault"),
        other => panic!("a machine with nothing to spawn cannot reach a ready desk: {other:?}"),
    }
    assert!(spawned.lock().unwrap().is_empty(), "nothing was started, which is the premise");

    // ---- the install happens, and the repair asks again --------------------------------
    match spine.prime_front_desk(&thread) {
        FrontDeskReady::Ready { spawned, .. } => assert!(spawned, "there was still no lease to prime"),
        other => panic!("the repaired machine must reach a ready desk: {other:?}"),
    }
    {
        let log = spawned.lock().unwrap();
        assert_eq!(log.len(), 1, "exactly one lease was spawned");
        assert_eq!(log[0].lock().unwrap().len(), 1, "and its priming turn was spent here");
    }

    // **AND HIS NEXT MESSAGE SPENDS NOTHING.** This is the assertion that makes the +18.3 s
    // claim answerable: with the desk still unprimed the count below would be 2, and that
    // second priming turn is what sat in front of his first words on candidate .11.
    let turn = spine.submit_prompt("Land the pricing branch and get the staging deploy done.", Source::Text).unwrap();
    assert_eq!(spawned.lock().unwrap()[0].lock().unwrap().len(), 1,
        "his first message after the repair re-primed a desk that was already ready");
    assert_eq!(spine.ledger().turn(&turn).unwrap().assistant_text, "On it!");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rich_is_given_the_company_name_exactly_as_he_typed_it() {
    // The CEO, 2026-10-13 (onboarding2.png): he typed a name with a word in capitals and Rich
    // wrote it back re-capitalized, because the priming named the company only by its id, and
    // the id is lower case. Rich has to be told the name itself, letter for letter, whether or
    // not the company has notes on file.
    let (path, ledger) = tmp_ledger("company-name");
    let mut spine = richos_core::spine::Spine::new(ledger);
    spine.set_entity_registry(
        richos_core::entity::EntityRegistry::new(vec![
            richos_core::entity::Entity::new("brightwave-labs", "Brightwave LABS", &["/fixture/brightwave"]).unwrap(),
        ])
        .unwrap(),
    );
    let thread = spine.create_thread("Onboarding", &EntityId::parse("brightwave-labs").unwrap()).unwrap();
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));
    assert!(matches!(spine.prime_front_desk(&thread), FrontDeskReady::Ready { .. }));
    let priming = reprimes.lock().unwrap()[0].clone();
    assert!(priming.contains("\"Brightwave LABS\""), "the name as he typed it is in the priming: {priming}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&priming).unwrap()["company_material"]["company_name"],"Brightwave LABS");
    std::fs::remove_file(&path).expect("the test ledger is removable");
}

/// Adapter with the native lease's context attachment capability. The original user
/// instruction and current company snapshot are recorded separately, as on the wire.
type CompanyDeliveryLog = std::sync::Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;

struct InlineCompany {
    mock: MockCognition,
    current: Option<String>,
    sent: CompanyDeliveryLog,
}

impl richos_core::cognition::Cognition for InlineCompany {
    fn session_id(&self) -> &str { self.mock.session_id() }


    fn prompt(&mut self, text: &str, sink: &mut dyn FnMut(richos_core::cognition::TurnItem))
        -> Result<String, richos_core::cognition::CognitionError> {
        self.sent.lock().unwrap().push((text.to_string(), self.current.clone()));
        self.mock.prompt(text, sink)
    }
}

#[test]
fn saved_interview_answers_reach_the_next_request_without_repriming_the_session() {
    let (path, ledger) = tmp_ledger("inline-company");
    let root = std::env::temp_dir().join(format!("inline-company-{}", uuid::Uuid::new_v4()));
    let company = richos_core::company::company_file(&root, &femcboost());
    std::fs::create_dir_all(company.parent().unwrap()).unwrap();
    std::fs::write(&company, "# Company\nWe sell bicycle parts.\n").unwrap();
    let mut spine = support::spine(ledger);
    spine.set_central_root(root.clone());
    let thread = spine.create_thread("Interview", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    let mock = MockCognition::new("inline-company", vec!["First answer", "Second answer"]);
    let primes = mock.reprimes.clone();
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(InlineCompany { mock, current: None, sent: sent.clone() }));
    spine.prime_front_desk(&thread);
    let first = spine.submit_prompt("What do you know?", Source::Text).unwrap();
    assert!(sent.lock().unwrap()[0].1.as_deref().unwrap().contains("We sell bicycle parts."));
    std::fs::write(&company, "# Company\nWe repair bicycles. Our next priority is service bookings.\n").unwrap();
    let second = spine.submit_prompt("What comes next?", Source::Text).unwrap();
    assert_eq!(primes.lock().unwrap().len(), 1, "a saved answer caused another hidden model turn");
    let delivered = sent.lock().unwrap();
    assert_eq!(delivered[1].0, "What comes next?");
    let context = delivered[1].1.as_deref().unwrap();
    assert!(context.contains("Our next priority is service bookings."));
    assert!(!context.contains("We sell bicycle parts."));
    assert_eq!(spine.ledger().turn(&first).unwrap().user_text, "What do you know?");
    assert_eq!(spine.ledger().turn(&second).unwrap().user_text, "What comes next?");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
