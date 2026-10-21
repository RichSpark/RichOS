//! Integration tests for the runtime spine — the crash-safety, queue-not-interrupt,
//! thread-model, and re-prime-continuity invariants — all with a MOCK cognition, so
//! they run green with NO live Claude / network (the live round-trip itself is proven
//! separately by examples/native_roundtrip.rs).

use richos_core::cognition::{Cognition, CognitionError, TurnItem, MockCognition};
use richos_core::ledger::{Ledger, Source, TurnState};
use richos_core::entity::EntityId;
use richos_core::spine::Spine;

/// The dogfood entity these tests run under. Every thread now has an immutable entity
/// home (ECS §3.2) and there is no entity-less path, so the tests NAME one rather than
/// inheriting a default that no longer exists.
fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}
use richos_core::stream::{StreamEvent, TurnObserver};
use std::sync::{Arc, Mutex};

mod support;

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

#[test]
fn prompt_is_persisted_received_before_send() {
    // Crash-safety: even with NO lease attached, submitting persists the prompt.
    let (path, ledger) = tmp_ledger("crashsafe");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();

    // No lease attached -> delivery fails, but the prompt MUST already be durable.
    let result = spine.submit_prompt("remember this even if we crash", Source::Text);
    assert!(result.is_err(), "delivery should fail with no lease");

    // Re-open from disk: the prompt survived as a `received`/interrupted turn.
    drop(spine);
    let reopened = Ledger::open(&path).unwrap();
    let survived = reopened
        .turns()
        .iter()
        .any(|t| t.user_text == "remember this even if we crash");
    assert!(survived, "CEO prompt must be durable before send");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn full_roundtrip_persists_and_renders_clean() {
    let (path, ledger) = tmp_ledger("roundtrip");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["Hello CEO, I'm Rich."])));

    let turn_id = spine.submit_prompt("hi", Source::Text).unwrap();

    let turn = spine.ledger().turn(&turn_id).unwrap();
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(turn.assistant_text, "Hello CEO, I'm Rich.");

    // Clean output view: user + assistant, no internal machinery.
    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Hello CEO, I'm Rich.");
    let _ = std::fs::remove_file(&path);
}





#[test]
fn default_thread_title_is_onboarding() {
    // The CEO, 2026-10-13: the first conversation of the first company "should be named
    // "Onboarding"", not "Running". The pinned default thread's REAL title, stored by the
    // backend, never a client-side relabel.
    let (path, ledger) = tmp_ledger("default-title");
    let mut spine = support::spine(ledger);
    let thread_id = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    let summaries = spine.threads();
    let default = summaries.iter().find(|t| t.id == thread_id).unwrap();
    assert_eq!(default.title, "Onboarding");
    let _ = std::fs::remove_file(&path);
}





#[test]
fn threads_are_views_over_one_shared_ledger() {
    let (path, ledger) = tmp_ledger("threads");
    let mut spine = support::spine(ledger);
    let germany = spine.create_thread("Germany", &femcboost()).unwrap();
    let hiring = spine.create_thread("Q4 hiring", &femcboost()).unwrap();
    spine.switch_thread(&germany).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("germany-session", vec!["a"])));
    spine.submit_prompt("about germany", Source::Text).unwrap();
    spine.switch_thread(&hiring).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("hiring-session", vec!["b"])));
    spine.submit_prompt("about hiring", Source::Text).unwrap();

    // Two threads, each carrying only its own messages, but both projected from the
    // one shared ledger (topic separation WITHOUT fragmentation).
    assert_eq!(spine.messages(&germany).unwrap().len(), 2);
    assert_eq!(spine.messages(&hiring).unwrap().len(), 2);
    assert_eq!(spine.messages(&germany).unwrap()[0].text, "about germany");
    assert_eq!(spine.messages(&hiring).unwrap()[0].text, "about hiring");

    let summaries = spine.threads();
    assert_eq!(summaries.len(), 2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn history_survives_restart() {
    let (path, ledger) = tmp_ledger("restart");
    let thread;
    {
        let mut spine = support::spine(ledger);
        thread = spine.create_thread("General", &femcboost()).unwrap();
        spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["persisted reply"])));
        spine.submit_prompt("remember me", Source::Text).unwrap();
    } // spine + ledger dropped (app "restart")

    let reopened = Ledger::open(&path).unwrap();
    let spine2 = support::spine(reopened);
    let msgs = spine2.messages(&thread).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].text, "remember me");
    assert_eq!(msgs[1].text, "persisted reply");
    let _ = std::fs::remove_file(&path);
}


#[test]
fn queue_not_interrupt_orders_prompts_without_dropping() {
    // With the mock, turns complete synchronously, so this asserts the QUEUE PATH is
    // wired and ordered (a mid-turn arrival is journaled + delivered, never dropped).
    let (path, ledger) = tmp_ledger("queue");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["r1", "r2", "r3"])));

    spine.submit_prompt("first", Source::Text).unwrap();
    spine.submit_prompt("second", Source::Text).unwrap();
    spine.submit_prompt("third", Source::Text).unwrap();

    let msgs = spine.messages(&thread).unwrap();
    let user_texts: Vec<_> = msgs.iter().filter(|m| m.role == "user").map(|m| m.text.clone()).collect();
    assert_eq!(user_texts, vec!["first", "second", "third"]);
    assert_eq!(spine.queue_depth(), 0);
    assert!(!spine.is_turn_in_progress());
    let _ = std::fs::remove_file(&path);
}

// ---- streaming / turn-state emission -------------------------------------------

/// A recording UI sink: captures every emitted event so tests can assert order, ids,
/// seq, and payload shape — the exact contract the Tauri UI consumes.
#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<StreamEvent>>>,
}
impl RecordingObserver {
    fn events(&self) -> Vec<StreamEvent> {
        self.events.lock().unwrap().clone()
    }
}
impl TurnObserver for RecordingObserver {
    fn on_event(&self, event: &StreamEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

/// A cognition that streams one chunk then fails the turn — to exercise the error path.
struct FailingCognition {
    session_id: String,
}
impl Cognition for FailingCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        on_item(TurnItem::Text { seq: 0, text: "partial before the lease dies" });
        Err(CognitionError::Io("adapter exited mid-turn".into()))
    }
}

#[test]
fn stream_emits_chunks_in_order_and_ledger_holds_full_reply() {
    // The core streaming guarantee: chunk events arrive in seq order, concatenate to the
    // full reply, and carry the right thread + turn ids — while the ledger (source of
    // truth) independently reflects the same full reply.
    let (path, ledger) = tmp_ledger("stream-order");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["Hello CEO, I am Rich and I am here."])));

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    let turn_id = spine.submit_prompt("hi", Source::Text).unwrap();

    let events = observer.events();

    // Chunk events, in arrival order.
    let chunks: Vec<(u64, String, String, String)> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Chunk { thread_id, turn_id, seq, text_delta, .. } => {
                Some((*seq, thread_id.clone(), turn_id.clone(), text_delta.clone()))
            }
            _ => None,
        })
        .collect();
    assert!(chunks.len() >= 2, "mock streams multiple chunks");

    // seq is 0-based and strictly increasing in arrival order.
    for (i, (seq, tid, tur, _)) in chunks.iter().enumerate() {
        assert_eq!(*seq, i as u64, "seq is 0-based and in order");
        assert_eq!(tid, &thread, "chunk carries the active thread id");
        assert_eq!(tur, &turn_id, "chunk carries the current turn id");
    }

    // Compare the actual live event against the separately persisted JSONL record.
    let durable: Vec<serde_json::Value> = std::fs::read_to_string(&path).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    for event in &events {
        if let StreamEvent::Chunk { turn_id, seq, at, .. } = event {
            let row = durable.iter().find(|row| row["event"] == "AssistantDelta"
                && row["turn_id"] == *turn_id && row["seq"] == *seq).unwrap();
            assert_eq!(row["at"].as_u64(), Some(*at), "emit metadata must date the persisted delta");
        }
    }

    // Concatenating textDelta in seq order reproduces the full reply...
    let streamed: String = chunks.iter().map(|(_, _, _, t)| t.clone()).collect();
    assert_eq!(streamed, "Hello CEO, I am Rich and I am here.");
    // ...and the ledger independently holds the same full reply.
    assert_eq!(spine.ledger().turn(&turn_id).unwrap().assistant_text, "Hello CEO, I am Rich and I am here.");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn turn_state_events_bracket_the_turn() {
    // turn-started fires first (the "Rich is working" affordance), turn-completed fires
    // last, both keyed to the same thread + turn; chunks live strictly between them.
    let (path, ledger) = tmp_ledger("stream-bracket");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["working then done"])));

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    let turn_id = spine.submit_prompt("go", Source::Text).unwrap();
    let events = observer.events();

    // First event is TurnStarted for this thread + turn.
    match &events[0] {
        StreamEvent::TurnStarted { thread_id, turn_id: t, .. } => {
            assert_eq!(thread_id, &thread);
            assert_eq!(t, &turn_id);
        }
        other => panic!("expected TurnStarted first, got {other:?}"),
    }
    // Last event is TurnCompleted for this turn, carrying the stopReason.
    match events.last().unwrap() {
        StreamEvent::TurnCompleted { turn_id: t, stop_reason, .. } => {
            assert_eq!(t, &turn_id);
            assert_eq!(stop_reason, "end_turn");
        }
        other => panic!("expected TurnCompleted last, got {other:?}"),
    }
    // Every chunk sits strictly between start and complete.
    let n = events.len();
    for (i, e) in events.iter().enumerate() {
        if matches!(e, StreamEvent::Chunk { .. }) {
            assert!(i > 0 && i < n - 1, "chunks are bracketed by start/complete");
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn failed_turn_emits_turn_error_and_persists_partial() {
    // A lease that dies mid-turn: the partial reply is persisted + streamed, and a
    // terminal turn-error is emitted (never a silent hang, never a leaked stack trace).
    let (path, ledger) = tmp_ledger("stream-error");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-x".into() }));

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    let turn_id = spine.submit_prompt("cause a failure", Source::Text).unwrap_err_turn(&spine);
    let events = observer.events();

    // A partial chunk streamed before the failure.
    assert!(events.iter().any(|e| matches!(e, StreamEvent::Chunk { .. })), "partial chunk streamed");
    // Terminal event is TurnError for this thread + turn.
    //
    // UNTIL 2026-09-17 THIS ASSERTED `reason.contains("adapter exited")` — the raw
    // `CognitionError` Display, on the CEO-facing wire. That is the nightly's D2 in its
    // original form: machinery reaching the surface, which then threw it away and
    // substituted two fixed sentences that were wrong for most causes. The reason is now
    // the statement `interruption.rs` authored for this class, and the machinery stays
    // where machinery belongs — the ledger's `stop_reason`, rendered at Technical
    // visibility and never shown to him.
    match events.last().unwrap() {
        StreamEvent::TurnError { thread_id, turn_id: t, reason, .. } => {
            assert_eq!(thread_id, &thread);
            assert_eq!(t, &turn_id);
            assert!(
                !reason.contains("adapter exited"),
                "machinery reached the CEO-facing wire: {reason}"
            );
            assert!(
                reason.contains("Press Restore my message, then send it again."),
                "a lost connection is the one class where a retry is honest: {reason}"
            );
            assert!(
                reason.contains("29 characters of the answer"),
                "the partial reply that IS on disk must be counted, not hand-waved: {reason}"
            );
        }
        other => panic!("expected TurnError last, got {other:?}"),
    }
    // The ledger captured the partial reply and marked the turn interrupted.
    let turn = spine.ledger().turn(&turn_id).unwrap();
    assert_eq!(turn.state, TurnState::Interrupted);
    assert_eq!(turn.assistant_text, "partial before the lease dies");
    // THE MACHINERY IS NOT LOST, it is filed correctly.
    assert!(
        turn.stop_reason.as_deref().unwrap_or_default().contains("adapter exited"),
        "{:?}",
        turn.stop_reason
    );
    // And the classification is durable beside it, with the control it was shown with.
    let cause = turn.interruption.as_ref().expect("the cause is recorded");
    assert_eq!(cause.cause, "transient");
    assert!(cause.offers_retry);
    // The turn boundary is clear again (queue-not-interrupt invariant intact).
    assert!(!spine.is_turn_in_progress());
    let _ = std::fs::remove_file(&path);
}

/// **A FAILED TURN IS ACCOUNTED FOR WHICHEVER SURFACE IT CAME FROM** — the nightly's D4.
///
/// `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D4: a failed
/// VOICE turn wrote `[richos] voice turn failed: …` to the boot log and a failed TEXT turn
/// wrote nothing at all, *"and the `gui-boot` log is the one artifact meant to hold every
/// line of a launch to account"*.
///
/// The asymmetry was in the SURFACE: `main.rs`'s voice submit wrapper logged its own
/// failures and `send_message` did not. The fix is not another surface-specific line — it is
/// that the failure is classified once, on the path both surfaces share, and the operator
/// line is printed from THAT (`spine.rs::record_interruption`).
///
/// This test is the durable half of that claim. It cannot read stderr, so it asserts the
/// thing the log line is printed from: both sources produce an identical interruption
/// record. A regression that made either surface skip classification shows up here, and a
/// log line that only one surface reaches becomes impossible to write.
#[test]
fn a_failed_turn_is_classified_identically_whether_it_was_typed_or_spoken() {
    let mut records = Vec::new();
    for (tag, source) in [("d4-text", Source::Text), ("d4-jam", Source::Jam)] {
        let (path, ledger) = tmp_ledger(tag);
        let mut spine = support::spine(ledger);
        spine.create_thread("General", &femcboost()).unwrap();
        spine.attach_lease(Box::new(FailingCognition { session_id: "sess-x".into() }));

        let turn_id = spine.submit_prompt("cause a failure", source).unwrap_err_turn(&spine);
        let turn = spine.ledger().turn(&turn_id).unwrap();
        assert_eq!(turn.source, source, "{tag}");
        let cause = turn
            .interruption
            .as_ref()
            .unwrap_or_else(|| panic!("{tag}: the failure was not classified, so nothing is logged"));
        records.push((tag, cause.clone()));
        let _ = std::fs::remove_file(&path);
    }

    let (text_tag, text) = &records[0];
    let (jam_tag, jam) = &records[1];
    assert_eq!(text.cause, jam.cause, "{text_tag} and {jam_tag} disagree on the class");
    assert_eq!(text, jam, "the typed and spoken surfaces must expose the same failure facts");
    assert_eq!(text.offers_retry, jam.offers_retry);
    // POSITIVE CONTROL: the shared record is a real classification and not a default that
    // both surfaces happen to share by doing nothing.
    assert_eq!(text.cause, "transient", "{text:?}");
}

/// **THE SAME LOST CONNECTION TWICE IN A ROW IS NOT TOLD AS PASSING THE SECOND TIME** (a new
/// user's first run, 2026-10-21: every message ended "RichOS lost its connection to Claude
/// Code ... This usually fixes itself. Press Restore my message, then send it again." in a
/// loop). Red on main, where every one of them is `transient` with the retry offered. The
/// first is a lost connection; the next one right after it is `repeated`, with no retry
/// offered; an answer in between makes the next failure a first one again.
#[test]
fn the_same_lost_connection_again_is_told_as_repeated_and_offers_no_retry() {
    let (path, ledger) = tmp_ledger("repeated-loss");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let mut causes = Vec::new();
    for words in ["what do I do now?", "Test to see if something is working."] {
        spine.attach_lease(Box::new(FailingCognition { session_id: "sess-r".into() }));
        let turn_id = spine.submit_prompt(words, Source::Text).unwrap_err_turn(&spine);
        let record = spine.ledger().turn(&turn_id).unwrap().interruption.clone().expect("classified");
        causes.push((record.cause.clone(), record.offers_retry, record.ceo_message.clone()));
    }
    assert_eq!((causes[0].0.as_str(), causes[0].1), ("transient", true), "{causes:?}");
    assert_eq!((causes[1].0.as_str(), causes[1].1), ("repeated", false), "{causes:?}");
    assert!(!causes[1].2.contains("send it again."), "the repeat does not invite another try: {}", causes[1].2);

    // An answer in between: the next failure is a first one again.
    spine.attach_lease(Box::new(MockCognition::new("sess-ok", vec!["Here is your answer."])));
    spine.submit_prompt("one more", Source::Text).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-r2".into() }));
    let turn_id = spine.submit_prompt("and again", Source::Text).unwrap_err_turn(&spine);
    let record = spine.ledger().turn(&turn_id).unwrap().interruption.clone().expect("classified");
    assert_eq!(record.cause, "transient", "an answer in between resets it");

    // THE WHOLE CLASS, NOT ONLY THIS ARM: an unexplained failure offers another try too, so it
    // cannot follow a failure that offered one either.
    spine.attach_lease(Box::new(UnexplainedCognition { session_id: "sess-u".into() }));
    let turn_id = spine.submit_prompt("still there?", Source::Text).unwrap_err_turn(&spine);
    let record = spine.ledger().turn(&turn_id).unwrap().interruption.clone().expect("classified");
    assert_eq!((record.cause.as_str(), record.offers_retry), ("repeated", false), "{record:?}");
    let _best_effort = std::fs::remove_file(&path);
}

/// **A REFUSED SIGN-IN OR KEY IS TOLD AS ITSELF, WITH ITS ROUTE, NEVER AS AN OUTAGE** (review
/// rv-20261021T163510Z-b612361c-0e62, finding 1). Both lines are Claude Code's own: the first
/// measured on 2.1.306 (2026-10-21) for an OAuth token Anthropic refuses, the second read out of
/// the bundle (`interruption_tests`). Each carries `API Error: 401`, which the upstream reader
/// claimed first, so the person got the outage card and no sign-in route. Red before the
/// upstream reader yielded to a recognized cause.
#[test]
fn a_refused_sign_in_or_key_is_classified_with_its_route_not_taken_for_an_outage() {
    for (line, cause) in [
        ("\"Failed to authenticate. API Error: 401 OAuth access token is invalid.\"", "not-signed-in"),
        ("\"API Error: 401 Invalid API key \u{b7} Please run /login\"", "credential-rejected"),
    ] {
        let (path, ledger) = tmp_ledger("refused-auth");
        let mut spine = support::spine(ledger);
        spine.create_thread("General", &femcboost()).unwrap();
        spine.attach_lease(Box::new(RefusingCognition { session_id: "sess-a".into(), line: line.into() }));
        let turn_id = spine.submit_prompt("what do I do now?", Source::Text).unwrap_err_turn(&spine);
        let turn = spine.ledger().turn(&turn_id).unwrap();
        assert!(turn.upstream_failure.is_none(), "{line}: taken for an outage: {:?}", turn.upstream_failure);
        let record = turn.interruption.clone().unwrap_or_else(|| panic!("{line}: not classified"));
        assert_eq!((record.cause.as_str(), record.offers_retry), (cause, false), "{line}");
        let _best_effort = std::fs::remove_file(&path);
    }
}

/// A lease whose turn ends on Claude Code's own `result` line, as `native_result` passes it on.
struct RefusingCognition {
    session_id: String,
    line: String,
}
impl Cognition for RefusingCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }
    fn prompt(&mut self, _text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        Err(CognitionError::Protocol(self.line.clone()))
    }
}

/// A lease that fails with a reason `interruption::classify` has no story for (`unknown`).
struct UnexplainedCognition {
    session_id: String,
}
impl Cognition for UnexplainedCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }
    fn prompt(&mut self, _text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        Err(CognitionError::Protocol("something RichOS has no name for".into()))
    }
}

/// Small helper: submit_prompt returns the turn id even when delivery fails (the prompt
/// is always journaled first). This recovers that id from the ledger for the error test.
trait UnwrapErrTurn {
    fn unwrap_err_turn(self, spine: &Spine) -> String;
}
impl UnwrapErrTurn for Result<String, richos_core::spine::SpineError> {
    fn unwrap_err_turn(self, spine: &Spine) -> String {
        match self {
            Ok(id) => id,
            // THE LAST TURN THE CEO CAUSED, whatever surface he used. This matched
            // `Source::Text` exactly until 2026-09-17, which made it unusable for the one
            // question the nightly's D4 asks — whether a SPOKEN failure is accounted for the
            // same way a typed one is. `!= Internal` is the honest predicate: it excludes
            // the re-prime turn, which is the only thing this must not pick, and names no
            // surface.
            Err(_) => spine
                .ledger()
                .turns()
                .iter()
                .rev()
                .find(|t| t.source != Source::Internal)
                .expect("a turn the CEO caused was journaled")
                .id
                .clone(),
        }
    }
}

/// Rich interprets a reply in context before the app can publish any promise of work.
#[test]
fn go_on_a_proposal_is_interpreted_by_rich_before_any_reply() {
    struct Seen {
        prompts: Arc<Mutex<Vec<String>>>,
        chunks: Arc<Mutex<Vec<(String, String, usize)>>>,
    }
    impl TurnObserver for Seen {
        fn on_event(&self, event: &StreamEvent) {
            if let StreamEvent::Chunk { turn_id, text_delta, .. } = event {
                let asked = self.prompts.lock().unwrap().len();
                self.chunks.lock().unwrap().push((turn_id.clone(), text_delta.clone(), asked));
            }
        }
    }
    let (path, ledger) = tmp_ledger("go-on-it");
    let mut spine = support::spine(ledger);
    spine.create_thread("Onboarding", &femcboost()).unwrap();
    let proposal = "But based on what you told me here, I think we also need to add the following AI teammates \
                    to my team:\n\n- Will (Marketing Director)\n- Beth (Bookkeeper)\n\nAnd we can expand/change this \
                    team any time as needed. Say go and I'll get them hired now.";
    // The second reply is the register's receipt, as the real lease streams it; the third is
    // the model's own answer to a "Go." that follows something other than the proposal.
    let mock = MockCognition::new("sess-1", vec![proposal, "Which roles do you mean?", "Go where?"]);
    let prompts = mock.prompts.clone();
    let chunks = Arc::new(Mutex::new(Vec::new()));
    spine.set_observer(Box::new(Seen { prompts: prompts.clone(), chunks: chunks.clone() }));
    spine.attach_lease(Box::new(mock));

    spine.submit_prompt("Pricing and any spend over $1,000 is my call.", Source::Text).unwrap();
    let go = spine.submit_prompt("Go.", Source::Text).unwrap();
    let first = chunks.lock().unwrap().iter().find(|(turn, _, _)| *turn == go).cloned();
    let (_, words, asked) = first.expect("nothing was said on his go");
    assert!("Which roles do you mean?".starts_with(&words), "a reply was invented before Rich spoke: {words:?}");
    assert_eq!(asked, 2, "the app replied before Rich interpreted the request");
    assert_eq!(spine.ledger().turn(&go).unwrap().assistant_text, "Which roles do you mean?");
    assert_eq!(prompts.lock().unwrap().last().map(String::as_str), Some("Go."), "the model still gets his go and hires");

    let again = spine.submit_prompt("Go.", Source::Text).unwrap();
    assert_eq!(spine.ledger().turn(&again).unwrap().assistant_text, "Go where?", "a go on anything else is the model's");
    std::fs::remove_file(&path).unwrap();
}


/// An agent contribution takes the ordinary durable incoming-message path. Only
/// Rich's reply appears in the user conversation, with sender provenance retained.
#[test]
fn agent_input_survives_restart_and_duplicate_delivery_without_a_report_writer() {
    use richos_core::steering::TurnControl;
    let (path, ledger) = tmp_ledger("ordinary-agent-input");
    let intake = path.with_extension("intake.jsonl");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Research", &femcboost()).unwrap();
    let control = TurnControl::open(&intake).unwrap();
    let report = r#"{"from":"stu","text":"The northern market has twice as many buyers."}"#;
    control.submit_agent_message(&thread, Some(femcboost()), report, "stu", "saved-message-1").unwrap();
    drop(spine);
    drop(control);

    let control = TurnControl::open(&intake).unwrap();
    // A duplicate notification/delivery must not create a second turn.
    control.submit_agent_message(&thread, Some(femcboost()), report, "stu", "saved-message-1").unwrap();
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    spine.set_turn_control(control);
    spine.switch_thread(&thread).unwrap();
    let reply = "The northern market has twice as many buyers.";
    let desk = MockCognition::new("rich", vec![reply]);
    let prompts = desk.prompts.clone();
    spine.attach_lease(Box::new(desk));
    spine.reconcile_intake().unwrap();
    spine.poll_intake().unwrap();
    assert_eq!(*prompts.lock().unwrap(), [report]);
    let messages = spine.messages(&thread).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "assistant");
    assert_eq!(messages[0].text, reply);
    let turn = spine.ledger().thread_turns(&thread).unwrap().into_iter().find(|t| t.source == Source::Agent).unwrap();
    assert_eq!(turn.channel.as_deref(), Some("stu"));
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(turn.source, Source::Agent, "Stu's report must never gain user authority");
    drop(spine);

    let mut reopened = support::spine(Ledger::open(&path).unwrap());
    reopened.set_turn_control(TurnControl::open(&intake).unwrap());
    reopened.switch_thread(&thread).unwrap();
    let desk = MockCognition::new("rich-restarted", vec![]);
    let replayed = desk.prompts.clone();
    reopened.attach_lease(Box::new(desk));
    reopened.reconcile_intake().unwrap();
    reopened.poll_intake().unwrap();
    assert!(replayed.lock().unwrap().is_empty());
    assert_eq!(reopened.messages(&thread).unwrap().len(), 1);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(intake).unwrap();
}
