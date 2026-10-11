//! **"NOT NOW", WRITTEN DOWN** — the two spine methods the first-run notice is built on, and
//! the one property that makes the notice worth building.
//!
//! `OnboardingRecord::record_declination` shipped in `onboarding.rs` on 2026-09-06 with NO
//! CALLER anywhere in the product:
//!
//! ```text
//! $ grep -rn "record_declination" app/ --include="*.rs" | grep -v "src/onboarding.rs"
//!   (no output)
//! ```
//!
//! So `OnboardingState::Declined` and `DECLINED_BLOCK` were unreachable outside their own unit
//! tests, and a CEO who said "not now" into the conversation was re-offered the interview at
//! every prime of every new thread, forever — the M5 nag with nothing holding it back, in a
//! module whose own doc names that nag as the reason the record exists.
//!
//! That is the same defect one layer up that this whole line of work is about: a correct,
//! tested mechanism that nothing invokes. So the tests that matter are not "does the record
//! round-trip" — `onboarding.rs` already proves that. They are:
//!
//!   1. the button's write REACHES RICH — the next priming turn stops offering and says he
//!      was asked (`the_declination_changes_what_rich_is_primed_with`);
//!   2. the notice and the priming turn are DERIVED FROM THE SAME FACT and cannot disagree
//!      (`the_notice_state_and_the_priming_block_never_disagree`);
//!   3. a write with nowhere to go is REFUSED and says so, rather than reporting success over
//!      a record that was never written (`a_declination_with_nowhere_to_go_is_refused`).
//!
//! Every test uses `MockCognition`. No `claude` process, no network, no audio device.

use richos_core::cognition::MockCognition;
use richos_core::company::{company_file, company_home};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::onboarding::{record_path, OnboardingRecord, OnboardingState};
use richos_core::spine::Spine;
use std::path::PathBuf;

mod support;

/// `femcboost` rather than a new id: `support::registry()` is the shared fixture and a
/// bare spine accepts no entity outside it. The company being described here is a fixture and
/// exists on no machine.
fn company() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

/// The same per-process counter every other suite in this crate needed, for the same measured
/// reason: cargo runs these in parallel threads of ONE process and `now_millis()` collides.
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tag(name: &str) -> String {
    format!(
        "{name}-{}-{}-{}",
        std::process::id(),
        richos_core::util::now_millis(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn tmp_ledger(name: &str) -> (PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!("richos-declination-{}.jsonl", tag(name)));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("richos-declination-{}", tag(name)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Drive one prompt through a spine and hand back every priming turn the lease received.
fn primings(spine: &mut Spine) -> Vec<String> {
    let mock = MockCognition::new("s-1", vec!["ok"]);
    let reprimes = mock.reprimes.clone();
    spine.attach_lease(Box::new(mock));
    spine.submit_prompt("what are we doing", Source::Text).unwrap();
    let out = reprimes.lock().unwrap().clone();
    out
}

// ---------------------------------------------------------------------------
// 1. The button's write reaches Rich. This is the whole reason the control exists.
// ---------------------------------------------------------------------------

/// BEFORE the press Rich is told to offer; AFTER it he is told he already asked. Both halves
/// in one test, on one spine, with nothing changing between them but the press — because the
/// "before" is what makes the "after" mean anything, and a test that only asserted the after
/// would pass against a spine that never offered in the first place.
#[test]
fn the_declination_changes_what_rich_is_primed_with() {
    let (path, ledger) = tmp_ledger("reaches-rich");
    let central = tmp_dir("reaches-rich-central");
    let config = tmp_dir("reaches-rich-config");
    let mut spine = support::spine(ledger);
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    spine.ensure_active_thread_in(&company()).unwrap();

    let before = primings(&mut spine);
    assert!(
        before[0].contains("He has not been asked yet."),
        "before the press, Rich is told to offer: {}",
        before[0]
    );

    spine.record_onboarding_declination(1_788_634_800_000).unwrap();

    // A SECOND SPINE OVER THE SAME FILES, not the same one re-primed: the record is on disk
    // and the next launch has to read it, which is the property the notice depends on.
    let ledger2 = Ledger::open(&path).unwrap();
    let mut spine2 = support::spine(ledger2);
    spine2.set_central_root(central.clone());
    spine2.set_onboarding_record(record_path(&config));
    spine2.ensure_active_thread_in(&company()).unwrap();

    let after = primings(&mut spine2);
    assert!(
        after[0].contains("said not now"),
        "after the press, Rich is told he already asked: {}",
        after[0]
    );
    assert!(
        !after[0].contains("He has not been asked yet."),
        "and he is NOT told to offer again — that is the nag: {}",
        after[0]
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// 2. One fact, two consumers. The screen and the priming turn cannot drift.
// ---------------------------------------------------------------------------

/// `Spine::onboarding_state` is what the window branches on and `company_block` is what Rich
/// is primed with. If those two were derived independently the notice could offer an interview
/// Rich had been told not to mention, or stay silent while he offered — so this walks all four
/// reachable states and asserts they move together.
#[test]
fn the_notice_state_and_the_priming_block_never_disagree() {
    let (path, ledger) = tmp_ledger("agree");
    let central = tmp_dir("agree-central");
    let config = tmp_dir("agree-config");
    let mut spine = support::spine(ledger);
    let b = spine.ensure_active_thread_in(&company()).unwrap();

    // (a) nobody has told the spine where to look.
    assert_eq!(spine.onboarding_state(&b), OnboardingState::NoCentralFolder);
    assert!(
        !primings(&mut spine)[0].contains("ONBOARDING —"),
        "nothing has looked, so nothing is claimed"
    );

    // (b) it looked, and this company is not described.
    let (path_b, ledger_b) = tmp_ledger("agree-b");
    let mut spine = support::spine(ledger_b);
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    let b = spine.ensure_active_thread_in(&company()).unwrap();
    assert_eq!(spine.onboarding_state(&b), OnboardingState::NotYet);
    assert!(primings(&mut spine)[0].contains("He has not been asked yet."));

    // (c) he was asked and declined.
    spine.record_onboarding_declination(1_788_634_800_000).unwrap();
    let (path_c, ledger_c) = tmp_ledger("agree-c");
    let mut spine = support::spine(ledger_c);
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    let b = spine.ensure_active_thread_in(&company()).unwrap();
    assert_eq!(spine.onboarding_state(&b), OnboardingState::Declined { at_millis: 1_788_634_800_000 });
    assert!(primings(&mut spine)[0].contains("said not now"));

    // (d) described — and DESCRIBED WINS OVER A DECLINATION, which is the ordering the
    // notice depends on: a company he later wrote up must stop being a company he declined
    // to be asked about, or the screen would offer him an interview about material it is
    // already using.
    std::fs::create_dir_all(company_home(&central, &company())).unwrap();
    std::fs::write(
        company_file(&central, &company()),
        "# FemcBoost\n\nWe run coastal freight scheduling for mid-size ports.\n",
    )
    .unwrap();
    let (path_d, ledger_d) = tmp_ledger("agree-d");
    let mut spine = support::spine(ledger_d);
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    let b = spine.ensure_active_thread_in(&company()).unwrap();
    assert_eq!(spine.onboarding_state(&b), OnboardingState::Described);
    let primed = primings(&mut spine)[0].clone();
    assert!(primed.contains("coastal freight scheduling"), "the material, not the offer: {primed}");
    assert!(!primed.contains("said not now"), "and not the declination either: {primed}");

    for p in [&path, &path_b, &path_c, &path_d] {
        let _ = std::fs::remove_file(p);
    }
}

// ---------------------------------------------------------------------------
// 3. A write with nowhere to go is refused, out loud.
// ---------------------------------------------------------------------------

/// The failure this refuses is the one the whole record is about: reporting success over work
/// that never happened. A press that silently wrote nothing would put him back in the
/// re-offered-forever state with a screen that told him he had settled it.
#[test]
fn a_declination_with_nowhere_to_go_is_refused() {
    let (path, ledger) = tmp_ledger("nowhere");
    let mut spine = support::spine(ledger);
    // `set_onboarding_record` deliberately NOT called — the shipped default for every
    // headless example and every other test in this crate.
    let err = spine
        .record_onboarding_declination(1_788_634_800_000)
        .expect_err("a declination with nowhere to go must be refused, not silently dropped");
    let said = err.to_string();
    assert!(
        said.contains("nowhere to write") || said.contains("nobody told this spine where"),
        "and the refusal must say what did not happen: {said}"
    );
    let _ = std::fs::remove_file(&path);
}

/// The record on disk still holds ONE fact after a write through the spine. `onboarding.rs`
/// has a unit test whose only job is to refuse a second field on the struct; this is the same
/// guarantee at the file, reached through the caller the product actually uses.
#[test]
fn the_spines_write_produces_a_record_holding_only_a_declination() {
    let config = tmp_dir("onefact-config");
    let (path, ledger) = tmp_ledger("onefact");
    let mut spine = support::spine(ledger);
    spine.set_onboarding_record(record_path(&config));
    spine.ensure_active_thread_in(&company()).unwrap();
    spine.record_onboarding_declination(1_788_634_800_000).unwrap();

    let body = std::fs::read_to_string(record_path(&config)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["declined_by_entity"][company().as_str()], 1_788_634_800_000_u64);
    assert_eq!(parsed["declined_by_entity"].as_object().unwrap().len(), 1);
    assert_eq!(
        OnboardingRecord::load(&record_path(&config)).for_entity(&company()).declined_at_millis(),
        Some(1_788_634_800_000),
        "and it reads back as what was written"
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// 4. "Not now" during boot priming costs no extra hidden model turn.
// ---------------------------------------------------------------------------

/// A lease whose FIRST priming turn waits, with the spine held, until the test lets it go:
/// the boot prime still running when he presses "Not now" and sends a message.
struct HeldPrime {
    mock: MockCognition,
    entered: Option<std::sync::mpsc::Sender<()>>,
    release: std::sync::mpsc::Receiver<()>,
}

impl richos_core::cognition::Cognition for HeldPrime {
    fn session_id(&self) -> &str {
        self.mock.session_id()
    }
    fn reprime(
        &mut self,
        text: &str,
        sink: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<(), richos_core::cognition::CognitionError> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.release.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        }
        self.mock.reprime(text, sink)
    }
    fn prompt(
        &mut self,
        text: &str,
        sink: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<String, richos_core::cognition::CognitionError> {
        self.mock.prompt(text, sink)
    }
}

/// The ordering from review `rv-20261011T101519Z-e4ab1771-98bb`: "Not now" is written from the
/// published view while `prime_front_desk` holds the spine, and a message is sent while the
/// prime continues. The prime drains that message before it lets go, and the message's turn
/// re-primes the desk with the declined block (priming 2). The deferred step that runs after
/// it must leave that up-to-date desk alone: his next message goes straight to the model.
/// On `e4ab1771e` it un-primed the desk anyway and the next message paid priming 3.
#[test]
fn not_now_during_boot_priming_with_a_queued_message_costs_no_extra_priming() {
    use richos_core::read_view::SpineView;
    use std::sync::{mpsc, Arc, Mutex};

    let (path, ledger) = tmp_ledger("not-now-queued");
    let central = tmp_dir("not-now-queued-central");
    let config = tmp_dir("not-now-queued-config");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Work", &company()).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    let control = richos_core::steering::TurnControl::open(config.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let mock = MockCognition::new("s-held", vec!["First reply", "Second reply", "Third reply"]);
    let primes = mock.reprimes.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    spine.attach_lease(Box::new(HeldPrime { mock, entered: Some(entered_tx), release: release_rx }));
    let reader = spine.reader();
    let spine = Arc::new(Mutex::new(spine));

    // Boot priming takes the spine and is inside its model turn.
    let prime_spine = spine.clone();
    let prime_thread = thread.clone();
    let prime = std::thread::spawn(move || {
        prime_spine.lock().unwrap().prime_front_desk(&prime_thread);
    });
    entered_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();

    // "Not now": written from the published view, the deferred step waiting for the spine.
    reader.snapshot().record_onboarding_declination(1_000).unwrap();
    let deferred_spine = spine.clone();
    let deferred = std::thread::spawn(move || deferred_spine.lock().unwrap().onboarding_answer_recorded());

    // His message, sent while the prime continues.
    let active = reader.snapshot().active_thread().unwrap().to_string();
    control.submit_from_desk(&active, Some(company()), "Please help with my business").unwrap();
    release_tx.send(()).unwrap();
    prime.join().unwrap();
    deferred.join().unwrap();

    let after_prime = primes.lock().unwrap().len();
    assert_eq!(after_prime, 2, "the boot prime, then the queued message's re-prime with the decline");
    assert!(
        primes.lock().unwrap()[1].contains("said not now"),
        "the queued message's re-prime already carries the decline"
    );

    spine.lock().unwrap().submit_prompt("Thanks, next question", Source::Text).unwrap();
    let after_next = primes.lock().unwrap().len();
    eprintln!(
        "[not-now-queued] primes after the prime: {after_prime}; after the deferred step and the next message: {after_next}"
    );
    assert_eq!(
        after_next, after_prime,
        "the deferred step un-primed a desk that already carried the decline, so the next message \
         waited through {} extra hidden priming turn(s)",
        after_next - after_prime
    );
    drop(spine);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&central).unwrap();
    std::fs::remove_dir_all(&config).unwrap();
}

// ---------------------------------------------------------------------------
// 5. After "Not now", every kind of desk carries the decline at its next message, and none
//    pays a priming turn it does not need.
// ---------------------------------------------------------------------------

/// Every priming turn any desk received, as `(desk, text)`, in order.
type PrimeLog = std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>;

/// The spawn gate: told when a spawn starts, then waited on until the test lets it go.
type Starting = std::sync::Arc<std::sync::Mutex<Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>>>;

/// A factory whose desks are isolated per thread (so a switch parks the outgoing desk, as the
/// shipped desks do) and log their priming turns under their own name: `desk-1`, `desk-2`, ...
/// in spawn order. It can be duplicated, so a spare takes the road the app takes, with the
/// spine's mutex down. When `starting` is set, the next spawn says so and then waits: a spare
/// still starting its process.
#[derive(Clone, Default)]
struct DeskFactory {
    primes: PrimeLog,
    spawns: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    starting: Starting,
}

impl richos_core::cognition::LeaseFactory for DeskFactory {
    fn duplicate(&self) -> Option<Box<dyn richos_core::cognition::LeaseFactory>> {
        Some(Box::new(self.clone()))
    }
    fn spawn(&self) -> Result<Box<dyn richos_core::cognition::Cognition>, richos_core::cognition::CognitionError> {
        let starting = self.starting.lock().unwrap().take();
        if let Some((entered, release)) = starting {
            entered.send(()).unwrap();
            release.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        }
        let n = self.spawns.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        Ok(Box::new(LoggedDesk { name: format!("desk-{n}"), primes: self.primes.clone() }))
    }
}

struct LoggedDesk {
    name: String,
    primes: PrimeLog,
}

impl richos_core::cognition::Cognition for LoggedDesk {
    fn session_id(&self) -> &str {
        &self.name
    }
    fn requires_thread_isolation(&self) -> bool {
        true
    }
    fn reprime(
        &mut self,
        text: &str,
        _: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<(), richos_core::cognition::CognitionError> {
        self.primes.lock().unwrap().push((self.name.clone(), text.to_string()));
        Ok(())
    }
    fn prompt(
        &mut self,
        _: &str,
        _: &mut dyn FnMut(richos_core::cognition::TurnItem),
    ) -> Result<String, richos_core::cognition::CognitionError> {
        Ok("ok".into())
    }
}

fn primes_of(log: &PrimeLog, desk: &str) -> Vec<String> {
    log.lock().unwrap().iter().filter(|(name, _)| name == desk).map(|(_, text)| text.clone()).collect()
}

/// "Not now" is written from the published view, the way the shell writes it
/// (`decline_onboarding_in`), and then each kind of desk takes its next message: the one in
/// use, a parked one, and the standing spare (adopted by the next new thread). Each must be
/// re-primed once, with the declined block, where it stands: no desk keeps offering the
/// interview and no desk is replaced. What makes that true is the comparison every message
/// runs (`prime_lease_if_needed`), against the block each desk carries with it. Nothing else
/// is run between the answer and the messages.
#[test]
fn after_not_now_the_next_message_on_every_kind_of_desk_carries_the_decline() {
    let (path, ledger) = tmp_ledger("every-desk");
    let central = tmp_dir("every-desk-central");
    let config = tmp_dir("every-desk-config");
    let mut spine = support::spine(ledger);
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    let factory = DeskFactory::default();
    let primes = factory.primes.clone();
    let spawns = factory.spawns.clone();
    spine.set_lease_factory(Box::new(factory));

    let parked = spine.create_thread("Parked work", &company()).unwrap();
    let in_use = spine.create_thread("Work in use", &company()).unwrap();
    spine.switch_thread(&parked).unwrap();
    spine.submit_prompt("first", Source::Text).unwrap(); // desk-1, then parked
    spine.switch_thread(&in_use).unwrap();
    spine.submit_prompt("second", Source::Text).unwrap(); // desk-2, in the chair
    match spine.ready_a_spare_front_desk(&company()) {
        richos_core::spine::SpareReady::Ready { .. } => {} // desk-3, standing
        other => panic!("the spare was not made ready: {other:?}"),
    }
    for desk in ["desk-1", "desk-2", "desk-3"] {
        let p = primes_of(&primes, desk);
        assert_eq!(p.len(), 1, "{desk} was primed once before the answer");
        assert!(p[0].contains("He has not been asked yet."), "{desk} was primed with the offer");
    }

    spine.reader().snapshot().record_onboarding_declination(1_000).unwrap();

    spine.submit_prompt("after not now, in use", Source::Text).unwrap();
    spine.switch_thread(&parked).unwrap();
    spine.submit_prompt("after not now, parked", Source::Text).unwrap();
    let adopted = spine.create_thread("New work", &company()).unwrap();
    spine.switch_thread(&adopted).unwrap();
    spine.submit_prompt("after not now, spare", Source::Text).unwrap();

    for (kind, desk) in [("in use", "desk-2"), ("parked", "desk-1"), ("spare", "desk-3")] {
        let p = primes_of(&primes, desk);
        let carries = p.last().is_some_and(|text| text.contains("said not now"));
        eprintln!("[not-now-every-desk] {kind} ({desk}): {} primes; the last carries the decline: {carries}", p.len());
        assert_eq!(p.len(), 2, "the {kind} desk is re-primed exactly once after the answer");
        assert!(carries, "the {kind} desk's next message carries the decline: {}", p[1]);
        assert!(!p[1].contains("He has not been asked yet."), "and no longer the offer ({kind})");
    }
    assert_eq!(
        spawns.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "no desk was replaced: each was re-primed where it stood"
    );
    drop(spine);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&central).unwrap();
    std::fs::remove_dir_all(&config).unwrap();
}

/// The ordering from review `rv-20261011T104029Z-c8b2a74a-37e4`, ported from its fixture
/// (`spare_decline.rs`): the shell starts a spare at boot with the spine's mutex down, and
/// "Not now" is pressed while its process is still starting. The spare assembles its payload
/// after the answer, so it is primed with the decline. Its first message must not pay a
/// second priming turn.
///
/// On `c8b2a74a5` the deferred step ran here (nobody holds the spine, so it runs at once),
/// moved the generation the spare captured before it started, and the spare was installed
/// un-primed although it already carried the decline: 1 redundant hidden priming turn.
#[test]
fn not_now_while_a_spare_is_starting_costs_its_first_message_no_extra_priming() {
    use std::sync::{mpsc, Arc, Mutex};

    let (path, ledger) = tmp_ledger("not-now-spare");
    let central = tmp_dir("not-now-spare-central");
    let config = tmp_dir("not-now-spare-config");
    let mut spine = support::spine(ledger);
    let existing = spine.create_thread("Existing work", &company()).unwrap();
    spine.switch_thread(&existing).unwrap();
    spine.set_central_root(central.clone());
    spine.set_onboarding_record(record_path(&config));
    let factory = DeskFactory::default();
    let primes = factory.primes.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    *factory.starting.lock().unwrap() = Some((entered_tx, release_rx));
    spine.set_lease_factory(Box::new(factory));
    let reader = spine.reader();
    let spine = Arc::new(Mutex::new(spine));

    let spare_spine = spine.clone();
    let spare = std::thread::spawn(move || {
        richos_core::spine::ready_a_spare_front_desk_without_the_spine(&spare_spine, &company())
    });
    entered_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();

    // "Not now", while the spare's process is starting.
    reader.snapshot().record_onboarding_declination(1_000).unwrap();
    // The deferred step the shell hands the decline's spine work to; with the spare's mutex
    // down it takes the spine at once.
    spine.lock().unwrap().onboarding_answer_recorded();

    release_tx.send(()).unwrap();
    let verdict = spare.join().unwrap();
    {
        let log = primes.lock().unwrap();
        assert_eq!(log.len(), 1, "the spare's own priming turn");
        assert!(log[0].1.contains("said not now"), "the spare was primed after the answer, with the decline");
    }

    let mut held = spine.lock().unwrap();
    let new_thread = held.create_thread("New work", &company()).unwrap();
    held.switch_thread(&new_thread).unwrap();
    held.submit_prompt("Help with my business", Source::Text).unwrap();
    let after = primes.lock().unwrap().len();
    eprintln!("[not-now-spare] spare installed: {verdict:?}; primes after its first message: {after}");
    assert_eq!(
        after,
        1,
        "the spare already carried the decline, yet its first message waited through {} extra \
         hidden priming turn(s)",
        after - 1
    );
    drop(held);
    drop(spine);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&central).unwrap();
    std::fs::remove_dir_all(&config).unwrap();
}
