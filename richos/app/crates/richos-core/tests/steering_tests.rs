//! STEERING AND STOP (UX §9.2 / §9.3 / §25 "Steering and stop") — the invariants.
//!
//! The problem this file exists to close, stated as slice 5 left it (`timeline.rs`,
//! before this change):
//!
//! > `Interrupted` covers a crash, a rotation and a cancel alike, so rendering *"You
//! > stopped after 18s"* would attribute the stop to the CEO on no evidence.
//!
//! §6.1 spends `You stopped after {duration}` on an ATTRIBUTION to the CEO. An attribution
//! needs evidence, and the evidence is a stop REQUEST that was durable before anything was
//! interrupted. These tests prove the two states never bleed into one another in either
//! direction: a crash can never be reported as a CEO stop, and a CEO stop is never
//! reported as a crash.
//!
//! Headless throughout: no live Claude, no network, no Tauri.

use richos_core::cognition::{CancellableMockCognition, Cognition, CognitionError, MockLeaseFactory, TurnItem};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source, TurnState};
use richos_core::live::{LiveEvent, LiveObserver};
use richos_core::spine::Spine;
use richos_core::steering::{StopOutcome, TurnCancel, TurnControl, ConversationControl};
use richos_core::timeline::{Timeline, TimelineItem, ViewMode, WorkState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-steering-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

/// The turns the CEO can actually see. `thread_turns_scoped` includes the internal
/// priming turn (`[re-prime]`, `Source::Internal`), which has no render path anywhere —
/// `messages()` and the timeline both drop it — so a test that asserts on the
/// conversation filters it out the same way rather than asserting on machinery.
fn ceo_turns<'a>(ledger: &'a Ledger, thread: &str) -> Vec<&'a richos_core::ledger::Turn> {
    let binding = ledger.thread_binding(thread).unwrap();
    ledger
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .filter(|t| t.source == Source::Text)
        .collect()
}

/// One received + started turn, ready to be ended one way or the other.
fn started_turn(ledger: &mut Ledger, text: &str) -> (String, String) {
    let thread = ledger.create_thread("stop", &femcboost()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let turn = ledger.record_prompt_received(&binding, text, Source::Text).unwrap();
    ledger.mark_turn_started(&turn, "sess-1").unwrap();
    (thread, turn)
}

#[test]
fn a_ceo_stop_and_a_crash_are_different_terminal_states_in_the_log() {
    let (_p, mut ledger) = tmp_ledger("distinct");
    let (_t, stopped) = started_turn(&mut ledger, "the one he stopped");
    let (_t2, crashed) = started_turn(&mut ledger, "the one that died");

    ledger.stop_turn(&stopped, 1_000).unwrap();
    ledger.interrupt_turn(&crashed, "cognition io: broken pipe").unwrap();

    assert_eq!(ledger.turn(&stopped).unwrap().state, TurnState::Stopped);
    assert_eq!(ledger.turn(&crashed).unwrap().state, TurnState::Interrupted);
    // The stop carries the CEO's request time; the crash carries nobody's.
    assert_eq!(ledger.turn(&stopped).unwrap().stop_requested_at, Some(1_000));
    assert_eq!(ledger.turn(&crashed).unwrap().stop_requested_at, None);
}

#[test]
fn the_distinction_survives_a_reopen_because_it_is_two_events_not_one_reason_string() {
    let path;
    let (stopped, crashed);
    {
        let (p, mut ledger) = tmp_ledger("reopen");
        path = p;
        let (_t, s) = started_turn(&mut ledger, "stopped");
        let (_t2, c) = started_turn(&mut ledger, "crashed");
        ledger.stop_turn(&s, 4_242).unwrap();
        ledger.interrupt_turn(&c, "adapter_exited").unwrap();
        stopped = s;
        crashed = c;
    }
    let reopened = Ledger::open(&path).unwrap();
    assert_eq!(reopened.turn(&stopped).unwrap().state, TurnState::Stopped);
    assert_eq!(reopened.turn(&stopped).unwrap().stop_requested_at, Some(4_242));
    assert_eq!(reopened.turn(&crashed).unwrap().state, TurnState::Interrupted);
}

#[test]
fn the_ledger_refuses_to_stop_a_turn_that_already_completed() {
    // SCOPE, STATED HONESTLY — this proves the LEDGER PRIMITIVE and nothing above it.
    //
    // It used to open with "The race §9.3 has to survive: the CEO presses stop at the exact
    // moment Rich finishes", which read as though the race were covered. It was not. This
    // calls `complete_turn` then `stop_turn` DIRECTLY, and until 2026-08-29 the spine never
    // called those two in that order — `deliver()` took the stop branch before reading what
    // the lease reported, so `complete_turn` was unreachable whenever a stop claim existed
    // and the guard exercised below could not fire in production. A green test that names a
    // race it does not run is worse than no test: it stops anyone looking.
    //
    // The race itself is now covered at the level where it happens, by
    // `a_turn_that_completed_is_never_rendered_as_one_the_ceo_stopped` (a real spine, a real
    // stop request, a lease that finishes anyway). This test keeps its own narrower job:
    // the projection rule that makes that fix possible.
    let (_p, mut ledger) = tmp_ledger("late-stop");
    let (_t, turn) = started_turn(&mut ledger, "nearly done");
    ledger.complete_turn(&turn, "end_turn").unwrap();
    ledger.stop_turn(&turn, richos_core::util::now_millis()).unwrap();

    assert_eq!(ledger.turn(&turn).unwrap().state, TurnState::Completed);
    assert_eq!(ledger.turn(&turn).unwrap().stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(ledger.turn(&turn).unwrap().stop_requested_at, None);
}

#[test]
fn a_stopped_turn_is_measured_not_estimated_and_reaches_the_timeline_as_stopped() {
    let (_p, mut ledger) = tmp_ledger("measured");
    let (thread, turn) = started_turn(&mut ledger, "measure me");
    ledger.stop_turn(&turn, 1).unwrap();

    let t = ledger.turn(&turn).unwrap();
    // ended_at was written by the stop event, so the span is MEASURED — the §6.1 label
    // "You stopped after {duration}" has a real number behind it rather than a clock read
    // at render time (§6.3's twelve-hour trap).
    assert!(t.ended_at.is_some(), "a stop must write an end time or the duration is unrenderable");
    assert!(t.active_ms().is_some());

    let binding = ledger.thread_binding(&thread).unwrap();
    let timeline = Timeline::project(&ledger, &binding, &[]).unwrap();
    let view = timeline.view(ViewMode::Ceo);
    let states: Vec<WorkState> = view
        .items
        .iter()
        .filter_map(|i| match i {
            TimelineItem::WorkDuration { state, .. } => Some(*state),
            _ => None,
        })
        .collect();
    assert_eq!(states, vec![WorkState::Stopped]);
}

#[test]
fn a_stopped_turn_raises_no_system_error_row_because_nothing_went_wrong() {
    // §5.5/§21's failure treatment belongs to failures. The CEO pressing stop is not one,
    // and drawing an error under it would tell him something broke.
    let (_p, mut ledger) = tmp_ledger("no-error-row");
    let (thread, turn) = started_turn(&mut ledger, "stop me");
    ledger.stop_turn(&turn, 1).unwrap();

    let binding = ledger.thread_binding(&thread).unwrap();
    let timeline = Timeline::project(&ledger, &binding, &[]).unwrap();
    let view = timeline.view(ViewMode::Technical);
    let errors = view.items.iter().filter(|i| matches!(i, TimelineItem::SystemError { .. })).count();
    assert_eq!(errors, 0, "a CEO stop is not a system error");
}

// =======================================================================================
// THE PART THAT MATTERS: A REAL TURN, STOPPED WHILE IT IS RUNNING
// =======================================================================================
//
// Everything above is about what the log says. This is about whether the button works.
//
// The wall, measured in slice 4 and re-measured here: `submit_prompt` takes `&mut self`
// and does not return until the turn ends, and the shell holds one `Mutex<Spine>`. These
// tests reproduce that exactly — the spine goes behind an `Arc<Mutex<..>>`, a thread takes
// the lock and runs a turn, and the stop is pressed from another thread that CANNOT have
// the lock. `the_spine_lock_is_genuinely_held...` asserts that directly with `try_lock`,
// so if some future change made the lock available mid-turn this stops being a proof and
// says so.

#[derive(Clone, Default)]
struct RecordingLive {
    events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl RecordingLive {
    fn statuses(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == richos_core::live::EVENT_TURN_STATUS)
            .filter_map(|(_, p)| p.get("status").and_then(|s| s.as_str()).map(|s| s.to_string()))
            .collect()
    }
}

impl LiveObserver for RecordingLive {
    fn on_live_event(&self, event: &LiveEvent) {
        self.events.lock().unwrap().push((event.event_name().to_string(), event.payload()));
    }
}

fn tmp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "richos-steering-{tag}-{}-{}",
        std::process::id(),
        richos_core::util::now_millis()
    ))
}

/// The bound on every wait in this file for something the test itself set in motion: a hang
/// guard, never a verdict. It was 5 s (audit
/// `docs/verification/2026-09-29-load-sensitive-checks-audit.md` R9, steering_tests `< 5 s`).
const HANG_GUARD: Duration = Duration::from_secs(60);

/// A turn that only a stop can end in time: 20 ms chunks for a whole hang guard. These tests
/// used to script 60 chunks (1.2 s), so a test thread descheduled for about a second between
/// "the turn is running" and "press stop" watched the turn finish on its own and failed with
/// the stop path right. Now the only way the turn ends before the hang guard is the stop, and
/// `delivered < STOPPABLE_TURN_CHUNKS` still says the stop cut it short.
const STOPPABLE_TURN_CHUNKS: usize = (HANG_GUARD.as_millis() / 20) as usize;

/// A spine wired the way the shell wires it: durable ledger, durable intake, a lease whose
/// turn takes long enough to be interrupted.
fn running_spine(tag: &str, chunks: usize) -> (Arc<Mutex<Spine>>, ConversationControl, RecordingLive, String) {
    let text: Vec<String> = (0..chunks).map(|i| format!("chunk {i} ")).collect();
    let lease = CancellableMockCognition::new(
        "sess-cancellable",
        text.iter().map(|s| s.as_str()).collect(),
        Duration::from_millis(20),
    );
    spine_with_lease(tag, Box::new(lease))
}

fn spine_with_lease(tag: &str, lease: Box<dyn Cognition>) -> (Arc<Mutex<Spine>>, ConversationControl, RecordingLive, String) {
    let ledger_path = tmp_path(&format!("{tag}-ledger")).with_extension("jsonl");
    let intake_path = tmp_path(&format!("{tag}-intake")).with_extension("jsonl");
    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let thread = spine.create_thread("stop me", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(lease);
    let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
    spine.set_turn_control(control.clone());
    (Arc::new(Mutex::new(spine)), control, live, thread)
}

/// A lease whose FIRST turn says one thing and then holds until the test lets it finish, and
/// whose later turns answer at once. For a test that needs the spine genuinely busy while it
/// does something, and the turn to END naturally afterwards: a fixed-length turn raced the
/// test thread and, on a busy Mac, ended before the test looked.
struct HeldUntilReleased {
    release: Arc<AtomicBool>,
    turns: usize,
}

impl Cognition for HeldUntilReleased {
    fn session_id(&self) -> &str {
        "sess-held"
    }

    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.turns += 1;
        on_item(TurnItem::Text { seq: 0, text: "working on it " });
        if self.turns == 1 {
            let began = std::time::Instant::now();
            while !self.release.load(Ordering::SeqCst) {
                assert!(began.elapsed() < HANG_GUARD, "the test never released the first turn");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        Ok("end_turn".to_string())
    }
}

#[test]
fn the_spine_lock_is_genuinely_held_for_the_whole_turn_and_the_stop_does_not_wait_for_it() {
    // A turn a whole hang guard long (STOPPABLE_TURN_CHUNKS x 20 ms), which the stop needs
    // 20 ms of. If the stop were routed through the spine lock this test would wait out the
    // turn and it would complete normally — which is exactly what a decorative stop button
    // looks like.
    let (spine, control, live, _thread) = running_spine("lock", STOPPABLE_TURN_CHUNKS);
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || {
            spine.lock().unwrap().submit_prompt("go", Source::Text).unwrap()
        })
    };

    // Wait until the turn is genuinely running — observed through the control's mirror,
    // never through a sleep-and-hope.
    let began = std::time::Instant::now();
    // Active now includes preparation. Wait for actual output in this test of
    // preserving partial output; preparation-only Stop has its own regressions.
    while !live.events.lock().unwrap().iter().any(|(name, _)| name == "rich://message-delta") {
        assert!(began.elapsed() < HANG_GUARD, "the turn never produced output");
        std::thread::sleep(Duration::from_millis(2));
    }

    // THE PROOF. The turn owns the lock; nothing that needs it could stop anything.
    assert!(spine.try_lock().is_err(), "the spine lock was free mid-turn — this test no longer proves anything");

    let outcome = control.request_stop().unwrap();
    match outcome {
        StopOutcome::Requested { reached_lease, .. } => assert!(reached_lease, "the cancel reached the lease"),
        other => panic!("expected Requested, got {other:?}"),
    }

    let turn_id = runner.join().unwrap();
    let guard = spine.lock().unwrap();
    let turn = guard.ledger().turn(&turn_id).unwrap();
    assert_eq!(turn.state, TurnState::Stopped, "the turn must record that the CEO ended it");
    // It really was cut short: STOPPABLE_TURN_CHUNKS were scripted, fewer arrived.
    let delivered = turn.assistant_text.split_whitespace().filter(|w| *w == "chunk").count();
    assert!(delivered > 0, "partial output must be preserved (§9.3 step 4), got none");
    assert!(delivered < STOPPABLE_TURN_CHUNKS, "the turn ran to completion — nothing was actually stopped");
}

#[test]
fn a_stopped_turn_reaches_the_wire_as_stopped_and_never_as_completed_or_failed() {
    let (spine, control, live, _thread) = running_spine("wire", STOPPABLE_TURN_CHUNKS);
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || spine.lock().unwrap().submit_prompt("go", Source::Text).unwrap())
    };
    while control.active_turn().is_none() {
        std::thread::sleep(Duration::from_millis(2));
    }
    control.request_stop().unwrap();
    runner.join().unwrap();

    let statuses = live.statuses();
    assert!(statuses.contains(&"stopped".to_string()), "no stopped status on the wire: {statuses:?}");
    assert!(!statuses.contains(&"completed".to_string()), "a stopped turn was announced as completed: {statuses:?}");
    assert!(!statuses.contains(&"failed".to_string()), "a stopped turn was announced as failed: {statuses:?}");
}

#[test]
fn steering_written_while_rich_works_is_durable_immediately_and_delivered_at_the_boundary() {
    // The first turn holds until this test releases it, so "the turn is still running while
    // he steers" is a fact the test arranges, not a race with a 0.4 s turn (20 chunks, before
    // 2026-09-29) that a descheduled test thread could lose.
    let release = Arc::new(AtomicBool::new(false));
    let (spine, control, _live, thread) =
        spine_with_lease("steer", Box::new(HeldUntilReleased { release: Arc::clone(&release), turns: 0 }));
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || spine.lock().unwrap().submit_prompt("first", Source::Text).unwrap())
    };
    let began = std::time::Instant::now();
    while control.active_turn().is_none() {
        assert!(began.elapsed() < HANG_GUARD, "the turn never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    // The spine is locked. This still returns.
    assert!(spine.try_lock().is_err());
    let record = control.steer("also check the invoice").unwrap();
    // Durable BEFORE anything is delivered (§9.2) — the bytes are on disk right now, while
    // the turn that will eventually carry them has not even ended.
    let on_disk = std::fs::read_to_string(control.intake_path().unwrap()).unwrap();
    assert!(on_disk.contains("also check the invoice"), "steering must be persisted before delivery");
    assert!(record.id() > 0);

    release.store(true, Ordering::SeqCst);
    runner.join().unwrap();
    let guard = spine.lock().unwrap();
    let turns = ceo_turns(guard.ledger(), &thread);
    let texts: Vec<&str> = turns.iter().map(|t| t.user_text.as_str()).collect();
    assert_eq!(texts, vec!["first", "also check the invoice"], "steering joins the thread in order");
    // And it was actually delivered to a lease rather than parked forever.
    assert_eq!(turns.last().unwrap().state, TurnState::Completed);
}

#[test]
fn a_stop_also_stops_the_work_the_ceo_had_queued_behind_it() {
    // A stop that immediately starts the next queued turn is not a stop.
    let (spine, control, _live, thread) = running_spine("queued", STOPPABLE_TURN_CHUNKS);
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || spine.lock().unwrap().submit_prompt("first", Source::Text).unwrap())
    };
    while control.active_turn().is_none() {
        std::thread::sleep(Duration::from_millis(2));
    }
    control.steer("and then this").unwrap();
    control.request_stop().unwrap();
    runner.join().unwrap();

    let guard = spine.lock().unwrap();
    let turns = ceo_turns(guard.ledger(), &thread);
    // His words are still there — STOPPED, NOT DELETED. He watched that bubble appear; if
    // it never became a ledger turn it would survive until the next reload and then vanish.
    let texts: Vec<&str> = turns.iter().map(|t| t.user_text.as_str()).collect();
    assert!(texts.contains(&"and then this"), "the CEO's words must survive a stop: {texts:?}");
    for t in &turns {
        assert!(
            matches!(t.state, TurnState::Stopped),
            "turn {:?} should be stopped, is {:?}",
            t.user_text,
            t.state
        );
    }
}

/// A lease that dies the instant it is cancelled — "the CEO pressed stop and the child
/// fell over" is one event, not two, and the ledger must not describe it as a crash.
struct DiesOnCancel {
    flag: Arc<AtomicBool>,
}

struct DiesOnCancelHandle(Arc<AtomicBool>);
impl TurnCancel for DiesOnCancelHandle {
    fn cancel(&self) -> bool {
        self.0.store(true, Ordering::SeqCst);
        true
    }
}

impl Cognition for DiesOnCancel {
    fn session_id(&self) -> &str {
        "sess-dies"
    }

    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        on_item(TurnItem::Text { seq: 0, text: "starting" });
        while !self.flag.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(CognitionError::Io("broken pipe".into()))
    }
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(Arc::new(DiesOnCancelHandle(Arc::clone(&self.flag))) as Arc<dyn TurnCancel>)
    }
}

#[test]
fn a_turn_the_ceo_stopped_is_never_crash_replayed_even_when_the_lease_dies_with_it() {
    // The nastiest case, and the reason the stop claim is read BEFORE either terminal
    // branch: without it, `prompt` returning `Err` sends this straight into §5.3's
    // automatic replay, and Rich re-runs the exact work the CEO just told him to stop.
    let ledger_path = tmp_path("replay-ledger").with_extension("jsonl");
    let intake_path = tmp_path("replay-intake").with_extension("jsonl");
    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    let thread = spine.create_thread("no replay", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(DiesOnCancel { flag: Arc::new(AtomicBool::new(false)) }));
    // A factory IS attached, so recovery is fully available — and must still not fire.
    let factory = MockLeaseFactory::new(vec!["I have redone the thing you stopped"]);
    spine.set_lease_factory(Box::new(factory));
    let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
    spine.set_turn_control(control.clone());

    let spine = Arc::new(Mutex::new(spine));
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || spine.lock().unwrap().submit_prompt("do the thing", Source::Text))
    };
    while control.active_turn().is_none() {
        std::thread::sleep(Duration::from_millis(2));
    }
    control.request_stop().unwrap();
    let outcome = runner.join().unwrap();
    assert!(outcome.is_ok(), "a stopped turn is not an error: {outcome:?}");

    let guard = spine.lock().unwrap();
    let turns = ceo_turns(guard.ledger(), &thread);
    assert_eq!(
        turns.len(),
        1,
        "the stopped turn was replayed: {:?}",
        turns.iter().map(|t| &t.user_text).collect::<Vec<_>>()
    );
    assert_eq!(turns[0].state, TurnState::Stopped);
    assert!(turns[0].superseded_by.is_none(), "a stopped turn must never be superseded by a replay");
}

#[test]
fn a_stop_request_that_outlived_the_process_is_applied_at_startup_not_replayed() {
    // The one-line crash window: the stop request is fsync'd, then the process dies before
    // the ledger's terminal event is written. On restart the turn is `in_flight` with a
    // durable stop request beside it — and crash-replay would re-run it.
    let ledger_path = tmp_path("outlive-ledger").with_extension("jsonl");
    let intake_path = tmp_path("outlive-intake").with_extension("jsonl");
    let turn_id;
    {
        let mut ledger = Ledger::open(&ledger_path).unwrap();
        let thread = ledger.create_thread("outlived", &femcboost()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        turn_id = ledger.record_prompt_received(&binding, "long job", Source::Text).unwrap();
        ledger.mark_turn_started(&turn_id, "sess-dead").unwrap();

        let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
        control.begin_turn(richos_core::steering::ActiveTurn {
            turn_id: turn_id.clone(),
            thread_id: thread.clone(),
            entity_id: Some(femcboost()),
            started_at: Some(1),
        });
        control.request_stop().unwrap();
        // ...and the process dies here. Nothing else is written.
    }

    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    let control = TurnControl::open(&intake_path).unwrap();
    assert_eq!(control.pending_intake().len(), 1, "the stop request survived the crash");
    spine.set_turn_control(control.clone());
    spine.reconcile_intake().unwrap();

    assert_eq!(spine.ledger().turn(&turn_id).unwrap().state, TurnState::Stopped);
    assert_eq!(spine.ledger().turn(&turn_id).unwrap().active_ms(), None,
        "a replayed Stop must not count the closed-app interval as active work");
    assert!(control.pending_intake().is_empty(), "the request was applied and marked drained");
}

#[test]
fn a_crash_between_the_ledger_write_and_the_drain_marker_files_the_message_once_not_twice() {
    // THE WINDOW, and why it is the direction it is. `drain_intake` writes the LEDGER first
    // and the drain marker SECOND, on purpose: a crash between them must re-present the
    // CEO's words rather than lose them. That makes the drain at-least-once — so without a
    // de-duplication key, a restart in that window files his one sentence as a second turn
    // and Rich answers it twice.
    //
    // The crash is reproduced exactly, not approximated: the ledger write is performed and
    // the drain marker is NOT, then the process boundary is crossed by dropping everything
    // and reopening both files from disk.
    let ledger_path = tmp_path("dedupe-ledger").with_extension("jsonl");
    let intake_path = tmp_path("dedupe-intake").with_extension("jsonl");
    let thread;
    let intake_id;
    {
        let mut ledger = Ledger::open(&ledger_path).unwrap();
        thread = ledger.create_thread("dedupe", &femcboost()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();

        let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
        control.begin_turn(richos_core::steering::ActiveTurn {
            turn_id: "turn_running".into(),
            thread_id: thread.clone(),
            entity_id: Some(femcboost()),
            started_at: Some(1),
        });
        let rec = control.steer("check the invoice too").unwrap();
        intake_id = rec.id();

        // The first half of the drain, and then the power goes out.
        ledger
            .record_prompt_received_from_intake(&binding, "check the invoice too", Source::Text, intake_id)
            .unwrap();
    }

    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
    assert_eq!(control.pending_intake().len(), 1, "the record is still undrained, as it must be");
    spine.set_turn_control(control.clone());
    spine.attach_lease(Box::new(richos_core::cognition::MockCognition::new("sess-after", vec!["on it"])));
    spine.switch_thread(&thread).unwrap();
    spine.reconcile_intake().unwrap();

    let turns = ceo_turns(spine.ledger(), &thread);
    let texts: Vec<&str> = turns.iter().map(|t| t.user_text.as_str()).collect();
    assert_eq!(texts, vec!["check the invoice too"], "the CEO's one message became {} turns", texts.len());
    assert!(control.pending_intake().is_empty(), "the record must end up drained either way");
}

#[test]
fn an_ordinary_typed_message_carries_no_intake_id_and_can_never_collide_with_one() {
    let (_p, mut ledger) = tmp_ledger("no-intake-id");
    let thread = ledger.create_thread("plain", &femcboost()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let turn = ledger.record_prompt_received(&binding, "just a message", Source::Text).unwrap();
    assert_eq!(ledger.turn(&turn).unwrap().intake_id, None);
    // And the lookup does not match a `None` against any id — the failure mode would be a
    // drain deciding its record was already handled by an unrelated typed message.
    assert!(ledger.turn_for_intake(0).is_none());
    assert!(ledger.turn_for_intake(1).is_none());
}

// ---------------------------------------------------------------------------------------
// THE RACE: A STOP THAT DID NOT LAND ON A TURN THAT FINISHED
// ---------------------------------------------------------------------------------------

/// A lease whose cancel seam WORKS — `cancel()` returns `true`, exactly as
/// `NativeCancelHandle::cancel` does once the interrupt has been written to the child —
/// and whose `prompt` nevertheless returns a natural `end_turn`.
///
/// That is not a contrived mock; it is the real ordering. `cancel()` clones the sink and
/// appends `ChunkMsg::Cancel` AFTER whatever is already queued, so when the adapter's
/// `Done` is already in the channel `rx.recv()` returns it first and `prompt` returns
/// `"end_turn"` (`native.rs`'s `prompt` loop). `NativeCancelHandle`'s doc comment addresses `Done`
/// RACING the wake; this is `Done` ALREADY QUEUED before the wake exists.
///
/// It is deterministic here, not timing-dependent: `prompt` blocks until the test says the
/// stop request has been registered, and only then reports its natural terminal.
struct FinishesDespiteTheStop {
    running: Arc<AtomicBool>,
    stop_registered: Arc<AtomicBool>,
    cancel: Arc<DeliveredButIgnored>,
}

/// `cancel()` returns `true` — the signal really was delivered to the child — and changes
/// nothing about the stream, because the answer was already on its way.
struct DeliveredButIgnored;
impl TurnCancel for DeliveredButIgnored {
    fn cancel(&self) -> bool {
        true
    }
}

impl FinishesDespiteTheStop {
    fn new() -> Self {
        FinishesDespiteTheStop {
            running: Arc::new(AtomicBool::new(false)),
            stop_registered: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(DeliveredButIgnored),
        }
    }
}

impl Cognition for FinishesDespiteTheStop {
    fn session_id(&self) -> &str {
        "sess-finishes-anyway"
    }

    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        on_item(TurnItem::Text { seq: 0, text: "Here is the complete answer you asked for." });
        self.running.store(true, Ordering::SeqCst);
        let began = std::time::Instant::now();
        while !self.stop_registered.load(Ordering::SeqCst) {
            assert!(began.elapsed() < HANG_GUARD, "the stop never registered");
            std::thread::sleep(Duration::from_millis(2));
        }
        // The whole point: the adapter's `Done` was already in the channel.
        Ok("end_turn".to_string())
    }
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(Arc::clone(&self.cancel) as Arc<dyn TurnCancel>)
    }
}

#[test]
fn a_turn_that_completed_is_never_rendered_as_one_the_ceo_stopped() {
    // THE DEFECT, at the surface where it does damage. `deliver()` took the stop branch
    // before reading what the lease reported, so `complete_turn` was unreachable whenever a
    // stop claim existed — and `ledger.rs`'s guard for exactly this race
    // ("a turn that completed before the stop request reached the lease stays completed,
    // because it did") could never fire on the live path, because the turn was still
    // `InFlight` when `stop_turn` was called.
    //
    // The result: `You stopped after {d}` — the ONE row in `app/ui/timeline.js` that names
    // the CEO as the cause of anything — rendered above a complete, successful answer. It
    // fails in the direction where he believes he prevented something he did not.
    let ledger_path = tmp_path("racecomplete-ledger").with_extension("jsonl");
    let intake_path = tmp_path("racecomplete-intake").with_extension("jsonl");
    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let thread = spine.create_thread("finishes anyway", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();

    let lease = FinishesDespiteTheStop::new();
    let running = Arc::clone(&lease.running);
    let stop_registered = Arc::clone(&lease.stop_registered);
    spine.attach_lease(Box::new(lease));
    let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
    spine.set_turn_control(control.clone());

    let spine = Arc::new(Mutex::new(spine));
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || spine.lock().unwrap().submit_prompt("do the thing", Source::Text).unwrap())
    };

    // Wait for the turn to be genuinely under way and about to finish — observed, never
    // slept-and-hoped.
    let began = std::time::Instant::now();
    while !(running.load(Ordering::SeqCst) && control.active_turn().is_some()) {
        assert!(began.elapsed() < HANG_GUARD, "the turn never started");
        std::thread::sleep(Duration::from_millis(2));
    }

    // The CEO presses stop. The signal IS delivered — and the answer was already on its way.
    let outcome = control.request_stop().unwrap();
    match outcome {
        StopOutcome::Requested { reached_lease, .. } => {
            assert!(reached_lease, "the cancel notification was delivered — that is the race, not a no-op")
        }
        other => panic!("expected Requested, got {other:?}"),
    }
    stop_registered.store(true, Ordering::SeqCst);
    let turn_id = runner.join().unwrap();

    let guard = spine.lock().unwrap();
    let turn = guard.ledger().turn(&turn_id).unwrap();

    // WHAT HAPPENED IS THAT THE TURN COMPLETED.
    assert_eq!(turn.state, TurnState::Completed, "a turn that ran to the end is completed, because it did");
    assert_eq!(turn.stop_reason.as_deref(), Some("end_turn"), "the lease's own terminal is kept verbatim");
    assert_eq!(turn.stop_requested_at, None, "and no CEO attribution is written onto it");
    assert!(turn.assistant_text.contains("complete answer"), "the full answer is still there");

    // §6.1: `WorkState::Stopped` is what `timeline.js` renders as "You stopped after {d}".
    // It must not be reachable for this turn.
    let binding = guard.ledger().thread_binding(&thread).unwrap();
    let timeline = Timeline::project(guard.ledger(), &binding, &[]).unwrap();
    let states: Vec<WorkState> = timeline
        .view(ViewMode::Ceo)
        .items
        .iter()
        .filter_map(|i| match i {
            TimelineItem::WorkDuration { state, .. } => Some(*state),
            _ => None,
        })
        .collect();
    assert!(
        !states.contains(&WorkState::Stopped),
        "\"You stopped after\" over a completed answer: {states:?}"
    );
    assert!(states.contains(&WorkState::Completed), "expected a completed duration row: {states:?}");

    // And the wire agrees with the ledger, so a reload cannot disagree with what he saw.
    let statuses = live.statuses();
    assert!(statuses.contains(&"completed".to_string()), "{statuses:?}");
    assert!(!statuses.contains(&"stopped".to_string()), "the wire announced a stop that never landed: {statuses:?}");
}

#[test]
fn the_stop_request_is_still_recorded_as_a_fact_that_did_not_land() {
    // The stop is NOT erased — it is recorded and it did not win. `Ledger::stop_turn` is
    // called after `complete_turn` precisely so `ledger.rs`'s guard runs on the live path
    // instead of only in a unit test, and the durable event survives a reopen.
    let ledger_path = tmp_path("racefact-ledger").with_extension("jsonl");
    let intake_path = tmp_path("racefact-intake").with_extension("jsonl");
    let turn_id;
    {
        let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
        let thread = spine.create_thread("recorded", &femcboost()).unwrap();
        spine.switch_thread(&thread).unwrap();
        let lease = FinishesDespiteTheStop::new();
        let running = Arc::clone(&lease.running);
        let stop_registered = Arc::clone(&lease.stop_registered);
        spine.attach_lease(Box::new(lease));
        let control = TurnControl::open(&intake_path).unwrap().for_thread(&thread);
        spine.set_turn_control(control.clone());

        let spine = Arc::new(Mutex::new(spine));
        let runner = {
            let spine = Arc::clone(&spine);
            std::thread::spawn(move || spine.lock().unwrap().submit_prompt("go", Source::Text).unwrap())
        };
        while !(running.load(Ordering::SeqCst) && control.active_turn().is_some()) {
            std::thread::sleep(Duration::from_millis(2));
        }
        control.request_stop().unwrap();
        stop_registered.store(true, Ordering::SeqCst);
        turn_id = runner.join().unwrap();

        // The request is on disk in the intake log — durable before the lease was touched,
        // which is what makes any later attribution evidence-backed (§9.3 steps 1 and 2).
        let intake = std::fs::read_to_string(&intake_path).unwrap();
        assert!(intake.contains("stop"), "the CEO's request must be durable regardless of outcome");
    }

    // And the ledger's own event stream carries the TurnStopped that the projection
    // correctly refused to apply.
    let raw = std::fs::read_to_string(&ledger_path).unwrap();
    assert!(raw.contains("TurnStopped"), "the stop request must be a recorded fact: {raw}");
    let reopened = Ledger::open(&ledger_path).unwrap();
    assert_eq!(
        reopened.turn(&turn_id).unwrap().state,
        TurnState::Completed,
        "and replaying the log must reach the same verdict — completed"
    );
    assert_eq!(reopened.turn(&turn_id).unwrap().stop_requested_at, None);
}

#[test]
fn a_stop_the_lease_actually_honoured_is_still_a_stop() {
    // The control that keeps the fix from becoming a mute button: when the lease reports
    // `cancelled` — RichOS's name for the agent's `terminal_reason: "aborted_streaming"`,
    // mapped by `native::stop_reason_of` —
    // the turn IS stopped and IS attributed to the CEO. `STOP_REASON_CANCELLED` had no
    // production reader at all before this commit; `deliver` is now that reader, and this
    // is the assertion that it reads it correctly.
    let (spine, control, live, _thread) = running_spine("honored", STOPPABLE_TURN_CHUNKS);
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || spine.lock().unwrap().submit_prompt("go", Source::Text).unwrap())
    };
    while control.active_turn().is_none() {
        std::thread::sleep(Duration::from_millis(2));
    }
    control.request_stop().unwrap();
    let turn_id = runner.join().unwrap();

    let guard = spine.lock().unwrap();
    let turn = guard.ledger().turn(&turn_id).unwrap();
    assert_eq!(turn.state, TurnState::Stopped, "the lease said `cancelled`; the CEO stopped it");
    assert!(turn.stop_requested_at.is_some(), "and the attribution is anchored to his request");
    assert!(live.statuses().contains(&"stopped".to_string()));
}
