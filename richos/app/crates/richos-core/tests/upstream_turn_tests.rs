//! **A TURN THAT DIES TO THE MODEL API DOES NOT LOSE SILENTLY** — row 3.30's second and
//! third answers, driven through the REAL `Spine` against an INJECTED upstream.
//!
//! Nothing here touches a network. The fault is injected at the `Cognition` seam by two
//! doubles that reproduce the two shapes an upstream failure can take, and the failure
//! TEXT is the verbatim `529` line this machine captured on 2026-09-03 (committed at
//! `docs/verification/upstream-failure-2026-09-05/captured-529.txt`, request id
//! `req_011Cegb417YK6i1BEVDFmzU1`).
//!
//! **The second shape is the one that mattered most and would have been missed.** `claude`
//! reports an API failure as an ASSISTANT MESSAGE, so the turn ends with an ordinary
//! terminal and the vendor's diagnostic is persisted as Rich's own reply. A test that only
//! injected an `Err` would have proved nothing about it.
//!
//! **What this suite does NOT cover, named rather than left to be found.** Neither wire
//! shape is captured: the twelve runs in
//! `docs/verification/native-claude-stream-json-2026-08-31/raw/` contain no API error at
//! all. These doubles reproduce what the vendor's own code says it emits and what its own
//! transcripts on this machine contain; they are not a recording of the `claude`
//! stream-json wire under an outage, because no such recording exists.

use richos_core::cognition::{Cognition, CognitionError, TurnItem};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source, TurnState};
use richos_core::stream::{StreamEvent, TurnObserver};
use richos_core::timeline::{TimelineItem, ViewMode, Visibility};
use richos_core::steering::TurnControl;
use richos_core::upstream::{RetryBudget, RetryClock, UpstreamFault, MAX_OVERLOAD_RETRIES, OVERLOAD_RETRY_WAITS};
use richos_core::LeaseFactory;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod support;

/// The verbatim bytes. Line 1 carries `req_011Cegb417YK6i1BEVDFmzU1`.
const CAPTURED_529: &str =
    include_str!("../../../../../docs/verification/upstream-failure-2026-09-05/captured-529.txt");
const CONSTRUCTED_429: &str =
    include_str!("../../../../../docs/verification/upstream-failure-2026-09-05/constructed-429.txt");

fn captured_529() -> &'static str {
    CAPTURED_529.lines().next().unwrap()
}

fn constructed_429() -> &'static str {
    CONSTRUCTED_429.lines().next().unwrap()
}

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-upstream-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

/// A clock that waits no time and writes down every wait it was asked for. Every spine in this
/// suite runs on one, because an overload now waits out the CEO's schedule (158 minutes in all)
/// before it gives up.
#[derive(Default)]
struct FakeClock(Mutex<Vec<Duration>>);
impl RetryClock for FakeClock {
    fn wait(&self, wait: Duration, stop_asked: &dyn Fn() -> bool) -> bool {
        self.0.lock().unwrap().push(wait);
        !stop_asked()
    }
}

fn spine(ledger: Ledger) -> richos_core::spine::Spine {
    let mut spine = support::spine(ledger);
    spine.set_retry_clock(Arc::new(FakeClock::default()));
    spine
}

#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<StreamEvent>>>,
}
impl RecordingObserver {
    fn errors(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                StreamEvent::TurnError { reason, .. } => Some(reason.clone()),
                _ => None,
            })
            .collect()
    }
}
impl TurnObserver for RecordingObserver {
    fn on_event(&self, event: &StreamEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

// =========================================================================================
// THE TWO INJECTED SHAPES
// =========================================================================================

/// **SHAPE 1 — the vendor's line arrives as ASSISTANT TEXT and the turn ends normally.**
///
/// This is what `claude` actually does: an API error is an assistant message with
/// `isApiErrorMessage`, so `native.rs` streams it as `TurnItem::Text` and `prompt` returns
/// an ordinary terminal. Before this row's work, that produced a COMPLETED turn whose
/// answer was a vendor diagnostic written in Rich's voice.
///
/// `preamble` is real reply text that arrived before the outage, so the tests can prove
/// the loss statement counts what survived and does NOT count the error message as part of
/// the answer.
struct ApiErrorAsTextCognition {
    session_id: String,
    preamble: Option<String>,
    error_line: String,
}

impl Cognition for ApiErrorAsTextCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn prompt(
        &mut self,
        _text: &str,
        on_item: &mut dyn FnMut(TurnItem),
    ) -> Result<String, CognitionError> {
        let mut seq = 0u64;
        if let Some(p) = &self.preamble {
            on_item(TurnItem::Text { seq, text: p });
            seq += 1;
        }
        on_item(TurnItem::Text { seq, text: &self.error_line });
        // The vendor's own terminal for this case: the turn ended, and nothing about the
        // stop reason says an outage happened.
        Ok("end_turn".to_string())
    }
}

/// **SHAPE 2 — the client gives up and the line is in the error's `Display`.**
struct ApiErrorAsErrCognition {
    session_id: String,
    error_line: String,
}

impl Cognition for ApiErrorAsErrCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn prompt(
        &mut self,
        _text: &str,
        _on: &mut dyn FnMut(TurnItem),
    ) -> Result<String, CognitionError> {
        Err(CognitionError::Io(self.error_line.clone()))
    }
}

/// A factory whose every lease fails the same way — so a retry that DOES happen is
/// observable by its spawn count, and one that does not is observable by its absence.
struct OutageFactory {
    spawns: Arc<Mutex<u64>>,
    error_line: String,
}
impl LeaseFactory for OutageFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let mut n = self.spawns.lock().unwrap();
        *n += 1;
        Ok(Box::new(ApiErrorAsErrCognition {
            session_id: format!("sess-outage-{n}"),
            error_line: self.error_line.clone(),
        }))
    }
}

// =========================================================================================
// 2. A DYING TASK MUST NOT LOSE SILENTLY
// =========================================================================================

/// INVARIANT: a `529` arriving as assistant text ends the turn as INTERRUPTED with a
/// durable upstream record — not as a completed turn whose answer was an error message.
///
/// This is the whole of the second answer in one test. The `assert_ne!` on
/// `TurnState::Completed` is the half that would have failed before this row's work.
#[test]
fn an_api_error_delivered_as_assistant_text_is_a_failure_and_not_a_completed_turn() {
    let (path, ledger) = tmp_ledger("text-shape");
    let mut spine = spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsTextCognition {
        session_id: "sess-1".into(),
        preamble: Some("Here is what I found so far. ".into()),
        error_line: captured_529().to_string(),
    }));

    let turn = spine.submit_prompt("how is the release going?", Source::Text).unwrap_err();
    // The turn id is not returned on the failure path, so it is read off the ledger.
    let _ = turn;
    let t = spine.ledger().thread_turns(&thread).unwrap();
    let t = t.iter().find(|t| t.source == Source::Text).expect("the CEO's turn exists");

    assert_ne!(t.state, TurnState::Completed, "a turn that never got an answer is not completed");
    assert_eq!(t.state, TurnState::Interrupted);

    let up = t.upstream_failure.as_ref().expect("the classification is durable");
    assert_eq!(up.fault, UpstreamFault::Overloaded.tag());
    assert_eq!(up.status, Some(529));
    assert_eq!(up.request_id.as_deref(), Some("req_011Cegb417YK6i1BEVDFmzU1"));
    assert_eq!(up.model.as_deref(), Some("claude-fable-5-1"));
    let _ = std::fs::remove_file(&path);
}

/// INVARIANT: the same for the OTHER shape — the client's own `Err`.
#[test]
fn an_api_error_delivered_as_a_client_error_classifies_the_same_way() {
    let (path, ledger) = tmp_ledger("err-shape");
    let mut spine = spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsErrCognition {
        session_id: "sess-1".into(),
        error_line: captured_529().to_string(),
    }));

    let _ = spine.submit_prompt("how is the release going?", Source::Text);
    let turns = spine.ledger().thread_turns(&thread).unwrap();
    let t = turns.iter().find(|t| t.source == Source::Text).unwrap();
    let up = t.upstream_failure.as_ref().expect("the classification is durable on this shape too");
    assert_eq!(up.fault, UpstreamFault::Overloaded.tag());
    let _ = std::fs::remove_file(&path);
}

/// INVARIANT: the current retry notice reaches the CEO at failure. The durable loss
/// statement still records what is on disk without counting the vendor's error text.
///
/// The character count is the load-bearing assertion: it counts the real preamble and
/// EXCLUDES the vendor's error line, because telling him "169 characters of the answer are
/// saved" when all 169 are the error message is a true number about the wrong thing.
#[test]
fn the_retry_notice_is_live_and_durable_loss_counts_exclude_the_error_message() {
    let (path, ledger) = tmp_ledger("loss-live");
    let mut spine = spine(ledger);
    let obs = RecordingObserver::default();
    spine.set_observer(Box::new(obs.clone()));
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let preamble = "Here is what I found so far. ";
    spine.attach_lease(Box::new(ApiErrorAsTextCognition {
        session_id: "sess-1".into(),
        preamble: Some(preamble.to_string()),
        error_line: captured_529().to_string(),
    }));

    let _ = spine.submit_prompt("how is the release going?", Source::Text);

    let errors = obs.errors();
    assert_eq!(
        errors.len(),
        1 + MAX_OVERLOAD_RETRIES as usize,
        "one statement per attempt, each at the moment it happened (this lease never recovers)"
    );
    let m = &errors[0];
    assert!(m.contains("Anthropic's servers are at capacity"), "what happened: {m}");
    assert!(m.contains("retry 1 of 7") && m.contains("in the same conversation"), "the scheduled native-session retry: {m}");
    let first = spine.ledger().thread_turns(&thread).unwrap().into_iter()
        .find(|turn| turn.source == Source::Text).unwrap();
    let loss = &first.upstream_failure.as_ref().unwrap().loss_message;
    assert!(loss.contains("what you asked for is saved"), "what survived: {loss}");
    assert!(
        loss.contains(&format!("the {} characters", preamble.chars().count())),
        "the measured count is the PREAMBLE ({} chars), not the error line: {loss}",
        preamble.chars().count()
    );
    assert!(loss.contains("Not on disk:"), "what did not survive: {loss}");
    assert!(
        !m.contains("API Error: 529"),
        "the vendor's raw line is kept in the ledger, not read out to him here: {m}"
    );
    let _ = std::fs::remove_file(&path);
}

/// INVARIANT: it survives a reload. This is the difference between "visible at the moment
/// it happens" and "discoverable afterward" — the exact distinction row 3.30 draws.
///
/// The spine is dropped, the ledger reopened from the same bytes, and the CEO-mode
/// timeline still carries the outage row with all three sentences.
#[test]
fn the_statement_survives_a_cold_reopen_and_renders_in_the_ceo_view() {
    let (path, ledger) = tmp_ledger("reload");
    let thread = {
        let mut spine = spine(ledger);
        let thread = spine.create_thread("General", &femcboost()).unwrap();
        spine.attach_lease(Box::new(ApiErrorAsTextCognition {
            session_id: "sess-1".into(),
            preamble: None,
            error_line: captured_529().to_string(),
        }));
        let _ = spine.submit_prompt("how is the release going?", Source::Text);
        thread
    };

    let reopened = Ledger::open(&path).unwrap();
    let spine = spine(reopened);
    let view = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    let outage = view
        .items
        .iter()
        .find_map(|i| match i {
            TimelineItem::UpstreamOutage { fault, request_id, .. } => {
                Some((fault.clone(), request_id.clone()))
            }
            _ => None,
        })
        .expect("the outage row is in the CEO view after a cold reopen");
    assert_eq!(outage.0, "overloaded");
    assert_eq!(
        outage.1, None,
        "the vendor's request id is technical and is redacted out of a CEO view"
    );
    let _ = std::fs::remove_file(&path);
}

/// INVARIANT: the vendor's diagnostic NEVER renders as Rich's prose, and the real reply
/// that arrived before it still does.
///
/// This is the sharpest edge of "fails silent": the CEO is shown a stack-shaped sentence
/// in Rich's voice and left to work out that anything went wrong.
#[test]
fn the_vendors_diagnostic_never_renders_as_richs_words_but_the_real_reply_still_does() {
    let (path, ledger) = tmp_ledger("no-vendor-voice");
    let mut spine = spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsTextCognition {
        session_id: "sess-1".into(),
        preamble: Some("Here is what I found so far. ".into()),
        error_line: captured_529().to_string(),
    }));
    let _ = spine.submit_prompt("how is the release going?", Source::Text);

    let view = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    let prose: Vec<String> = view
        .items
        .iter()
        .filter_map(|i| match i {
            TimelineItem::RichMessage { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    // POSITIVE CONTROL: the guard admits the good case. The real reply is still there.
    assert!(
        prose.iter().any(|t| t.contains("Here is what I found so far")),
        "the answer that DID arrive must still render: {prose:?}"
    );
    // And the negative side.
    assert!(
        !prose.iter().any(|t| t.contains("API Error: 529")),
        "a vendor diagnostic must never render as Rich's own words: {prose:?}"
    );

    // It is DEMOTED, not deleted — the bytes are still evidence, at Internal visibility.
    let all = spine.timeline(&thread).unwrap().view(ViewMode::Technical);
    let hidden = all.items.iter().any(|i| matches!(
        i,
        TimelineItem::RichMessage { text, base, .. } if text.contains("API Error: 529")
            && base.visibility == Visibility::Internal
    ));
    assert!(!hidden, "internal items render in NO mode, so it is absent here too");
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("API Error: 529"), "and the bytes are still in the ledger as evidence");
    let _ = std::fs::remove_file(&path);
}

// =========================================================================================
// 3. RETRY IS BOUNDED AND VISIBLE
// =========================================================================================

/// A lease that answers `529` or answers properly, as its script says, and writes down which
/// session every prompt reached and what it said. An empty script answers.
struct ScriptedCognition {
    session_id: String,
    /// `true` = this attempt meets the captured `529`.
    script: Arc<Mutex<VecDeque<bool>>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
}

impl Cognition for ScriptedCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push((self.session_id.clone(), text.to_string()));
        if self.script.lock().unwrap().pop_front().unwrap_or(false) {
            return Err(CognitionError::Io(captured_529().to_string()));
        }
        on_item(TurnItem::Text { seq: 0, text: "the release is on track" });
        Ok("end_turn".to_string())
    }
}

/// **The CEO, 2026-10-07: *"There needs to be an exponential backoff ... first retry after 1
/// minute; second retry: after another 2-minute wait; third retry: after another 5-minute
/// wait; 4th retry: after another 10-minute wait; 5th retry: after another 20-minute wait;
/// 6th retry: after another 40-minute wait; 7th retry: after another 80-minute wait."***
///
/// Through the real spine, on a fake clock:
///
/// 1. seven `529`s, then an answer: the waits are 1, 2, 5, 10, 20, 40 and 80 minutes, all
///    eight attempts carry his words to the SAME session (`sess-0`), no lease is spawned, and
///    the turn completes;
/// 2. the answer reset the schedule: one more `529` waits 1 minute again;
/// 3. eight `529`s: after the 7th retry fails, the turn ends as it always did, interrupted
///    with its upstream record, and nothing more is tried.
///
/// RED before this change: an overload bought one IMMEDIATE retry on a FRESH lease
/// (`recover_and_replay`), so no wait was asked for and the factory spawned.
#[test]
fn an_overload_waits_out_his_schedule_on_the_same_session_and_a_success_resets_it() {
    let (path, ledger) = tmp_ledger("schedule");
    let mut spine = support::spine(ledger);
    let clock = Arc::new(FakeClock::default());
    spine.set_retry_clock(clock.clone());
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let script = Arc::new(Mutex::new(VecDeque::new()));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(ScriptedCognition {
        session_id: "sess-0".into(),
        script: script.clone(),
        prompts: prompts.clone(),
    }));
    let spawns = Arc::new(Mutex::new(0u64));
    spine.set_lease_factory(Box::new(OutageFactory {
        spawns: spawns.clone(),
        error_line: captured_529().to_string(),
    }));
    let minutes = |m: &[u64]| m.iter().map(|m| Duration::from_secs(m * 60)).collect::<Vec<_>>();
    assert_eq!(OVERLOAD_RETRY_WAITS.to_vec(), minutes(&[1, 2, 5, 10, 20, 40, 80]), "his schedule, as written");

    // 1. Seven overloads, then the answer.
    script.lock().unwrap().extend([true; 7]);
    spine.submit_prompt("how is the release going?", Source::Text).expect("the eighth attempt answered");
    assert_eq!(*clock.0.lock().unwrap(), minutes(&[1, 2, 5, 10, 20, 40, 80]));
    let sent = prompts.lock().unwrap().clone();
    assert_eq!(sent.len(), 8, "the try and seven retries: {sent:?}");
    assert!(
        sent.iter().all(|(session, text)| session == "sess-0" && text == "how is the release going?"),
        "every retry is his same turn on the same session: {sent:?}"
    );
    assert_eq!(*spawns.lock().unwrap(), 0, "no fresh session was opened");
    assert_eq!(spine.lease_session_id().as_deref(), Some("sess-0"));
    let turns = spine.ledger().thread_turns(&thread).unwrap();
    let answered = turns.iter().rev().find(|t| t.user_text == "how is the release going?").unwrap();
    assert_eq!(answered.state, TurnState::Completed);

    // 2. The answer reset the schedule.
    script.lock().unwrap().push_back(true);
    spine.submit_prompt("and the budget?", Source::Text).expect("the retry answered");
    assert_eq!(*clock.0.lock().unwrap(), minutes(&[1, 2, 5, 10, 20, 40, 80, 1]), "starts again at 1 minute");

    // 3. Eight overloads: the 7th retry fails and the turn ends.
    script.lock().unwrap().extend([true; 8]);
    assert!(spine.submit_prompt("and the hiring plan?", Source::Text).is_err());
    assert_eq!(
        *clock.0.lock().unwrap(),
        minutes(&[1, 2, 5, 10, 20, 40, 80, 1, 1, 2, 5, 10, 20, 40, 80]),
        "seven waits and no eighth"
    );
    assert_eq!(prompts.lock().unwrap().len(), 8 + 2 + 8);
    let turns = spine.ledger().thread_turns(&thread).unwrap();
    let last = turns.iter().rev().find(|t| t.user_text == "and the hiring plan?").unwrap();
    assert_eq!(last.state, TurnState::Interrupted);
    assert_eq!(last.upstream_failure.as_ref().unwrap().fault, UpstreamFault::Overloaded.tag());
    assert_eq!(*spawns.lock().unwrap(), 0);
    let _ = std::fs::remove_file(&path);
}

/// **A conversation's inputs reach its provider in the order they were accepted, whichever
/// driver takes them off the queue** (nightly 75, 2026-10-25: the provider received "Pressed
/// follow-up" before "Spoken follow-up").
///
/// The desktop runs `drive_shared_turns` from every entrance at once: the typed send, the
/// voice callback, the phone drain, the question host and front-desk priming
/// (`src-tauri/src/main.rs`). Here driver A holds an overload retry in its wait, which is a
/// point between taking an input off the queue and writing it to the provider, and driver B
/// takes the follow-up he sent during that wait. RED before the fix: B wrote the follow-up
/// first, so the provider saw his follow-up before the question it follows.
#[test]
fn a_follow_up_sent_during_a_retry_wait_reaches_the_provider_after_the_retry() {
    use std::sync::mpsc;
    struct HeldClock {
        entered: Mutex<mpsc::Sender<()>>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl RetryClock for HeldClock {
        fn wait(&self, _wait: Duration, _stop_asked: &dyn Fn() -> bool) -> bool {
            self.entered.lock().unwrap().send(()).unwrap();
            // Hang guard only: the test releases the wait.
            self.release.lock().unwrap().recv_timeout(Duration::from_secs(30)).expect("hang guard: wait never released");
            true
        }
    }
    let (path, ledger) = tmp_ledger("order-across-drivers");
    let mut spine = support::spine(ledger);
    let (entered_tx, entered) = mpsc::channel();
    let (release, held) = mpsc::channel();
    spine.set_retry_clock(Arc::new(HeldClock { entered: Mutex::new(entered_tx), release: Mutex::new(held) }));
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(ScriptedCognition {
        session_id: "sess-0".into(),
        script: Arc::new(Mutex::new(VecDeque::from([true]))),
        prompts: prompts.clone(),
    }));
    spine.detach_conversation_io();
    spine.submit_prompt_to(&thread, "how is the release going?", Source::Text).unwrap();
    let shared = Arc::new(Mutex::new(spine));
    let a = { let shared = shared.clone(); std::thread::spawn(move || richos_core::spine::drive_shared_turns(&shared)) };
    entered.recv_timeout(Duration::from_secs(30)).expect("hang guard: the overload never reached its retry wait");
    shared.lock().unwrap().submit_prompt_to(&thread, "and the budget?", Source::Text).unwrap();
    let (b_done, b_finished) = mpsc::channel();
    let b = {
        let shared = shared.clone();
        std::thread::spawn(move || {
            let driven = richos_core::spine::drive_shared_turns(&shared);
            b_done.send(()).expect("the test waits for this driver");
            driven
        })
    };
    // On the red, B writes and finishes within microseconds while A is held. With the fix B
    // cannot write before A's retry at any timing, so this bound never decides the fix's verdict.
    let b_ran_ahead = b_finished.recv_timeout(Duration::from_secs(1)).is_ok(); // load-bound: a slow host can only hide the pre-fix red, never fail the fix
    release.send(()).unwrap();
    a.join().unwrap().unwrap();
    b.join().unwrap().unwrap();
    let sent: Vec<String> = prompts.lock().unwrap().iter().map(|(_, text)| text.clone()).collect();
    assert_eq!(sent, ["how is the release going?", "how is the release going?", "and the budget?"],
        "the retried question must reach the provider before the follow-up accepted after it (driver B finished while A was held: {b_ran_ahead})");
    std::fs::remove_file(&path).unwrap();
}

/// The way out of the wait: his Stop during it ends it at once, and his turn is not sent again.
#[test]
fn his_stop_during_the_wait_ends_it_and_nothing_is_sent_again() {
    struct StopsDuringTheWait(richos_core::steering::ConversationControl, Mutex<Vec<Duration>>);
    impl RetryClock for StopsDuringTheWait {
        fn wait(&self, wait: Duration, stop_asked: &dyn Fn() -> bool) -> bool {
            self.1.lock().unwrap().push(wait);
            self.0.request_stop().unwrap();
            !stop_asked()
        }
    }
    let (path, ledger) = tmp_ledger("stop-in-wait");
    let intake = path.with_extension("intake.jsonl");
    let control = TurnControl::open(&intake).unwrap();
    let mut spine = support::spine(ledger);
    spine.set_turn_control(control.clone());
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let clock = Arc::new(StopsDuringTheWait(control.for_thread(&thread), Mutex::new(Vec::new())));
    spine.set_retry_clock(clock.clone());
    let script = Arc::new(Mutex::new(VecDeque::from([true; 8])));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(ScriptedCognition {
        session_id: "sess-0".into(),
        script,
        prompts: prompts.clone(),
    }));

    let _ = spine.submit_prompt("how is the release going?", Source::Text);

    assert_eq!(*clock.1.lock().unwrap(), vec![Duration::from_secs(60)], "one wait, cut short");
    assert_eq!(prompts.lock().unwrap().len(), 1, "his stop came before the retry was sent");
    let turns = spine.ledger().thread_turns(&thread).unwrap();
    let last = turns.iter().rev().find(|t| t.user_text == "how is the release going?").unwrap();
    assert_eq!(last.state, TurnState::Stopped, "{last:?}");
    let _ = std::fs::remove_file(&path);
    std::fs::remove_file(&intake).unwrap_or_default();
}

/// POSITIVE CONTROL for the ceiling: it is an allowance, not a refusal. A turn that
/// SUCCEEDS restores it, so the app is not left with nothing on the day it matters.
#[test]
fn a_successful_turn_between_failures_restores_the_allowance() {
    // Held at the budget level, because driving a heal through the spine needs a lease
    // that changes behavior mid-run and that would prove the double, not the rule.
    let mut b = RetryBudget::new();
    for _ in 0..MAX_OVERLOAD_RETRIES {
        assert!(b.charge(UpstreamFault::Overloaded), "each failure up to the 7th buys a retry");
    }
    assert!(!b.charge(UpstreamFault::Overloaded), "the 8th does not");
    b.succeeded();
    assert!(b.may_retry(UpstreamFault::Overloaded), "a completed turn restores it");
    assert!(b.charge(UpstreamFault::Overloaded), "and it can be spent again");
    assert_eq!(b.wait_before_retry(), Some(Duration::from_secs(60)), "from the first wait");
}

/// INVARIANT: each wait reaches the CEO as it starts, with its length, and the end says what
/// was spent, with the cost named.
#[test]
fn each_retry_is_announced_with_its_wait_and_the_last_statement_says_it_stopped() {
    let (path, ledger) = tmp_ledger("visible");
    let mut spine = spine(ledger);
    let obs = RecordingObserver::default();
    spine.set_observer(Box::new(obs.clone()));
    let _thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsErrCognition {
        session_id: "sess-0".into(),
        error_line: captured_529().to_string(),
    }));

    let _ = spine.submit_prompt("first", Source::Text);

    let errors = obs.errors();
    assert_eq!(errors.len(), 8, "{errors:?}");
    for (said, when) in errors.iter().zip(["1 minute", "2 minutes", "5 minutes", "10 minutes", "20 minutes", "40 minutes", "80 minutes"]) {
        assert!(said.contains(&format!("try again in {when}")), "{said}");
        assert!(said.contains("same conversation"), "{said}");
    }
    assert!(errors[7].contains("tried 8 times"), "last: {}", errors[7]);
    assert!(
        errors[7].contains("costs against your Claude usage"),
        "the reason the ceiling exists is named: {}",
        errors[7]
    );
    let _ = std::fs::remove_file(&path);
}

/// INVARIANT (CEO, 2026-10-07, "go with recommended"): after all seven retries fail, a NEW
/// message gets the full schedule again, starting at 1 minute.
#[test]
fn a_new_message_after_seven_failed_retries_waits_one_minute_first() {
    let (path, ledger) = tmp_ledger("new-message-restarts");
    let mut spine = spine(ledger);
    let obs = RecordingObserver::default();
    spine.set_observer(Box::new(obs.clone()));
    let _thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsErrCognition {
        session_id: "sess-0".into(),
        error_line: captured_529().to_string(),
    }));

    spine.submit_prompt("first", Source::Text).ok();
    assert_eq!(obs.errors().len(), 8, "seven retries, all failed");
    spine.submit_prompt("second", Source::Text).ok();

    let errors = obs.errors();
    assert_eq!(errors.len(), 16, "the new message gets the whole schedule: {errors:?}");
    assert!(errors[8].contains("try again in 1 minute"), "{}", errors[8]);
    assert!(errors[15].contains("tried 8 times"), "{}", errors[15]);
    std::fs::remove_file(&path).ok();
}

// =========================================================================================
// 5. `429` AND `529` ARE PRESENTED DIFFERENTLY, END TO END
// =========================================================================================

/// INVARIANT: through the real spine, a `429` produces a different sentence, a different
/// stored classification, and — the part that costs money — NO automatic retry.
#[test]
fn a_quota_failure_says_something_different_and_spends_no_retry() {
    let (path, ledger) = tmp_ledger("quota");
    let mut spine = spine(ledger);
    let obs = RecordingObserver::default();
    spine.set_observer(Box::new(obs.clone()));
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let spawns = Arc::new(Mutex::new(0u64));
    spine.attach_lease(Box::new(ApiErrorAsErrCognition {
        session_id: "sess-0".into(),
        error_line: constructed_429().to_string(),
    }));
    spine.set_lease_factory(Box::new(OutageFactory {
        spawns: spawns.clone(),
        error_line: constructed_429().to_string(),
    }));

    let _ = spine.submit_prompt("first", Source::Text);

    let turns = spine.ledger().thread_turns(&thread).unwrap();
    let t = turns.iter().find(|t| t.source == Source::Text).unwrap();
    assert_eq!(t.upstream_failure.as_ref().unwrap().fault, UpstreamFault::RateLimited.tag());

    let m = &obs.errors()[0];
    assert!(m.contains("usage limit is used up"), "{m}");
    assert!(m.contains("schedule"), "the wait has an end, and it says so: {m}");
    assert!(!m.contains("at capacity"), "it must not read as an overload: {m}");

    assert_eq!(
        *spawns.lock().unwrap(),
        0,
        "a usage window that rolls over in hours cannot be helped by an immediate retry, \
         so no attempt is spent on it"
    );

    // And the timeline carries the distinction as a BOOLEAN, so no renderer has to read
    // the prose to get it right.
    let view = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    let clears = view.items.iter().find_map(|i| match i {
        TimelineItem::UpstreamOutage { clears_on_a_known_schedule, .. } => {
            Some(*clears_on_a_known_schedule)
        }
        _ => None,
    });
    assert_eq!(clears, Some(true));
    let _ = std::fs::remove_file(&path);
}

/// POSITIVE CONTROL for the boolean above: the overload arm is `false`, so the field
/// discriminates rather than always answering the same way.
#[test]
fn the_overload_arm_reports_no_known_schedule() {
    let (path, ledger) = tmp_ledger("overload-bool");
    let mut spine = spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsErrCognition {
        session_id: "sess-0".into(),
        error_line: captured_529().to_string(),
    }));
    let _ = spine.submit_prompt("first", Source::Text);
    let view = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    let clears = view.items.iter().find_map(|i| match i {
        TimelineItem::UpstreamOutage { clears_on_a_known_schedule, .. } => {
            Some(*clears_on_a_known_schedule)
        }
        _ => None,
    });
    assert_eq!(clears, Some(false));
    let _ = std::fs::remove_file(&path);
}

// =========================================================================================
// THE ANTI-VACUOUS HALF
// =========================================================================================

/// POSITIVE CONTROL, and the most important test in this file. An ORDINARY failure — a
/// broken pipe, the thing that actually happens most days — must NOT be dressed up as an
/// outage, or the whole vocabulary is noise.
#[test]
fn an_ordinary_local_failure_produces_no_upstream_record_at_all() {
    let (path, ledger) = tmp_ledger("local");
    let mut spine = spine(ledger);
    let obs = RecordingObserver::default();
    spine.set_observer(Box::new(obs.clone()));
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ApiErrorAsErrCognition {
        session_id: "sess-0".into(),
        // NOT an API error. The `claude` child's stdio went away.
        error_line: "adapter exited mid-turn".into(),
    }));

    let _ = spine.submit_prompt("first", Source::Text);

    let turns = spine.ledger().thread_turns(&thread).unwrap();
    let t = turns.iter().find(|t| t.source == Source::Text).unwrap();
    assert_eq!(t.state, TurnState::Interrupted, "it still failed, and still says so");
    assert!(
        t.upstream_failure.is_none(),
        "but it was not the model API, and nothing claims it was"
    );
    let view = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    assert!(
        !view.items.iter().any(|i| matches!(i, TimelineItem::UpstreamOutage { .. })),
        "no outage row for a local failure"
    );
    let m = &obs.errors()[0];
    assert!(!m.contains("Anthropic"), "and he is not told to wait for Anthropic: {m}");
    let _ = std::fs::remove_file(&path);
}

/// POSITIVE CONTROL: a HEALTHY turn produces no outage row, no interruption and no error
/// event. A suite whose every case is a failure proves only that failing works.
#[test]
fn a_healthy_turn_produces_no_outage_and_no_error() {
    let (path, ledger) = tmp_ledger("healthy");
    let mut spine = spine(ledger);
    let obs = RecordingObserver::default();
    spine.set_observer(Box::new(obs.clone()));
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(richos_core::cognition::MockCognition::new(
        "sess-ok",
        vec!["the release is on track"],
    )));

    let turn = spine.submit_prompt("how is the release going?", Source::Text).unwrap();
    let t = spine.ledger().turn(&turn).unwrap();
    assert_eq!(t.state, TurnState::Completed);
    assert!(t.upstream_failure.is_none());
    assert!(obs.errors().is_empty(), "no error statement on a good turn");

    let view = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    assert!(!view.items.iter().any(|i| matches!(i, TimelineItem::UpstreamOutage { .. })));
    let prose: Vec<String> = view
        .items
        .iter()
        .filter_map(|i| match i {
            TimelineItem::RichMessage { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        prose.iter().any(|t| t.contains("release")),
        "and the answer still renders — the demotion is narrow: {prose:?}"
    );
    let _ = std::fs::remove_file(&path);
}
