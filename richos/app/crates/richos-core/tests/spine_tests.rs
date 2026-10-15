//! Integration tests for the runtime spine — the crash-safety, queue-not-interrupt,
//! thread-model, and re-prime-continuity invariants — all with a MOCK cognition, so
//! they run green with NO live Claude / network (the live round-trip itself is proven
//! separately by examples/native_roundtrip.rs).

use richos_core::cognition::{Cognition, CognitionError, TurnItem, MockCognition, MockLeaseFactory};
use richos_core::ledger::{Ledger, Source, TurnState};
use richos_core::reprime::RePrimePayload;
use richos_core::entity::EntityId;
use richos_core::spine::{FrontDeskReady, Spine};

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
fn lease_is_reprimed_before_first_turn() {
    // Continuity foundation: the re-prime (identity assertion) is injected once,
    // before the first CEO-visible turn.
    let (path, ledger) = tmp_ledger("reprime");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();

    let mock = MockCognition::new("sess-1", vec!["reply"]);
    let reprimes_ref = mock.reprimes.clone();
    let prompts_ref = mock.prompts.clone();
    spine.attach_lease(Box::new(mock));

    spine.submit_prompt("hello", Source::Text).unwrap();

    let reprimes = reprimes_ref.lock().unwrap();
    assert_eq!(reprimes.len(), 1, "exactly one re-prime injection");
    // "You are Rich" and the no-denial-from-absent-memory rule moved to the standing
    // instruction on 2026-09-06 — a system prompt, fixed at spawn, rather than a turn
    // (`doctrine.rs`; inner-doctrine design §4.2, §5.1). They are checked there by
    // `action_ledger_tests::the_ledgers_partial_coverage_is_stated_rather_than_overclaimed`.
    assert!(reprimes[0].contains("continuing conversation"), "the successor is told which conversation it is in");
    assert!(reprimes[0].contains("never mis-attribute your own prior actions"), "anti-false-attribution rule present");
    // The re-prime happens; the CEO prompt is delivered after it.
    assert_eq!(*prompts_ref.lock().unwrap(), vec!["hello".to_string()]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn internal_reprime_turn_is_never_rendered() {
    let (path, ledger) = tmp_ledger("internal");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["ok"])));
    spine.submit_prompt("hey", Source::Text).unwrap();

    // A `received` Internal turn exists in the ledger...
    assert!(spine.ledger().turns().iter().any(|t| t.source == Source::Internal));
    // ...but it has NO render path: messages() only shows the real user + assistant.
    let msgs = spine.messages(&thread).unwrap();
    assert!(msgs.iter().all(|m| m.text != "[re-prime]"));
    assert_eq!(msgs.len(), 2);
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
fn a_conversation_the_app_opens_carries_richs_message_and_rich_is_primed_with_it() {
    // The CEO, 2026-10-13: once dictation is set up, a conversation titled "Dictation feature"
    // is created under the company with Rich's message in it, and "If you have any questions
    // about this, we can discuss it here." So the message is Rich's, in the ledger, and the desk
    // that answers in that conversation is primed with it; the conversation he is in stays open.
    let (path, ledger) = tmp_ledger("app-opened");
    let mut spine = support::spine(ledger);
    let first = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    let (id, _message) = spine
        .open_conversation_with_message(&femcboost(), "Dictation feature", "Good news! Dictation is set up and enabled.")
        .unwrap();
    assert_ne!(id, first);
    assert_eq!(spine.active_thread(), Some(first.as_str()), "the conversation he is in stays the one he is in");
    let row = spine.threads().into_iter().find(|t| t.id == id).expect("the new conversation is listed");
    assert_eq!(row.title, "Dictation feature");
    assert_eq!(row.entity_id.as_deref(), Some("femcboost"), "under his company");
    let said = spine.messages(&id).unwrap();
    assert_eq!(said.len(), 1);
    assert_eq!((said[0].role.as_str(), said[0].text.as_str()), ("assistant", "Good news! Dictation is set up and enabled."));
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));
    spine.prime_front_desk(&id);
    let primed = reprimes.lock().unwrap().join("\n");
    assert!(primed.contains("Good news! Dictation is set up and enabled."), "Rich knows what he said there: {primed}");
    std::fs::remove_file(&path).expect("the test ledger is removable");
}

#[test]
fn an_app_message_replaced_is_the_one_message_there_and_the_one_rich_is_primed_with() {
    // The CEO, 2026-10-13: "make sure the correct key is actually within that message". When he
    // picks another key later, the "Dictation feature" message is replaced: the conversation
    // shows the new one only, and the desk answering there is primed with the new one only.
    let (path, ledger) = tmp_ledger("app-replaced");
    let mut spine = support::spine(ledger);
    spine.ensure_active_thread_in(&femcboost()).unwrap();
    let (id, first) = spine.open_conversation_with_message(&femcboost(), "Dictation feature", "tap F2, then").unwrap();
    let second = spine.replace_app_message(&id, Some(&first), "tap F3, then", "The dictation key is now F3.").unwrap();
    assert_ne!(first, second);
    let said: Vec<String> = spine.messages(&id).unwrap().into_iter().map(|m| m.text).collect();
    assert_eq!(said, vec!["tap F3, then".to_string()], "one message, naming the new key");
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));
    spine.prime_front_desk(&id);
    let primed = reprimes.lock().unwrap().join("\n");
    assert!(primed.contains("tap F3, then"), "{primed}");
    // The conversation and the action ledger digest both hold the new message only: a digest line
    // for the replaced one made Rich answer "an earlier one said F2" (esc-20261014T041934Z-2acf6aeb).
    assert!(!primed.contains("tap F2, then"), "the replaced message is not in the priming: {primed}");
    std::fs::remove_file(&path).expect("the test ledger is removable");
}

#[test]
fn an_app_message_with_no_turn_on_record_is_replaced_and_the_new_key_recorded() {
    // Review rv-20261014T050558Z-e9854621-0b36: a "Dictation feature" record written before the
    // message's turn was kept names only the conversation. The message replaced is the one the
    // conversation was opened with, and the dated new-key fact is recorded all the same.
    let (path, ledger) = tmp_ledger("app-replaced-legacy");
    let mut spine = support::spine(ledger);
    spine.ensure_active_thread_in(&femcboost()).unwrap();
    let (id, _) = spine.open_conversation_with_message(&femcboost(), "Dictation feature", "tap F2, then").unwrap();
    spine.replace_app_message(&id, None, "tap F3, then", "The dictation key is now F3.").unwrap();
    let said: Vec<String> = spine.messages(&id).unwrap().into_iter().map(|m| m.text).collect();
    assert_eq!(said, vec!["tap F3, then".to_string()], "one message, naming the new key");
    let today = time::OffsetDateTime::now_utc().date();
    let fact = format!("{today}: The dictation key is now F3.");
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.reprimes.clone();
    spine.attach_lease(Box::new(lease));
    spine.prime_front_desk(&id);
    let primed = reprimes.lock().unwrap().join("\n");
    assert!(primed.contains(&fact), "Rich is told the new key: {primed}");
    std::fs::remove_file(&path).expect("the test ledger is removable");
}

#[test]
fn after_a_key_change_the_desk_that_answers_knows_only_the_new_key() {
    // Ray, esc-20261014T041934Z-2acf6aeb: set up F2, pick F3 in Settings, then ask Rich in
    // "Dictation feature" which key starts dictation. His first turn there named F3 and added
    // "an earlier one said F2": the desk opening the conversation had readied was primed with
    // F2 and only primed again on top of it, and the action ledger digest every priming carries
    // still listed the replaced message. The CEO: the key he picked is the one named,
    // everywhere, so the onboarding conversation is asked too.
    //
    // History is KEPT on purpose (review rv-20261014T044933Z-51176a08-f956): Rich's own earlier
    // reply naming F2 stays in the conversation and a saved handoff summary naming F2 stays in
    // the priming. What wins is one dated fact recorded at the change, primed AFTER every older
    // mention: "The dictation key is now F3. The old key F2 no longer works."
    let (path, ledger) = tmp_ledger("app-replaced-only-new-key");
    let (onboarding, id, first);
    {
        let mut spine = support::spine(ledger);
        onboarding = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
        (id, first) = spine.open_conversation_with_message(&femcboost(), "Dictation feature", "tap F2, then").unwrap();
        spine.attach_lease(Box::new(MockCognition::new("sess-before", vec!["Tap F2 to start dictation."])));
        spine.switch_thread(&id).unwrap();
        spine.submit_prompt("Which key starts dictation?", Source::Text).unwrap();
    } // the app quits; what it said is in the ledger
    let mut ledger = Ledger::open(&path).unwrap();
    for thread in [&id, &onboarding] {
        ledger.record_handoff_summary(thread, "The start/stop key is F2.").unwrap();
    }
    let mut spine = support::spine(ledger);
    let factory = MockLeaseFactory::new(vec!["F3.", "F3."]);
    let prompts = factory.spawned_prompts.clone();
    let primes = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));
    // Opening each conversation readies its desk with what is there now: F2.
    assert!(matches!(spine.prime_front_desk(&id), FrontDeskReady::Ready { .. }));
    assert!(matches!(spine.prime_front_desk(&onboarding), FrontDeskReady::Ready { .. }));
    spine.replace_app_message(&id, Some(&first), "tap F3, then", "The dictation key is now F3. The old key F2 no longer works.").unwrap();
    let fact = "The dictation key is now F3. The old key F2 no longer works.";
    for thread in [&id, &onboarding] {
        spine.switch_thread(thread).unwrap();
        let question = format!("Which key do I tap to start dictation now? ({thread})");
        spine.submit_prompt(&question, Source::Text).unwrap();
        // Every priming the desk that answered was ever handed, from its start.
        let desks = prompts.lock().unwrap().clone();
        let answering = desks.iter().position(|asked| asked.lock().unwrap().contains(&question))
            .expect("a desk answered the question");
        let primed = primes.lock().unwrap()[answering].lock().unwrap().join("\n");
        let fact_at = primed.rfind(fact).unwrap_or_else(|| panic!("the desk answering in {thread} is not told the key changed: {primed}"));
        let today = time::OffsetDateTime::now_utc().date().to_string();
        assert!(primed[..fact_at].ends_with(&format!("{today}: ")), "the fact is dated: {primed}");
        assert!(primed[..fact_at].contains("The start/stop key is F2."), "the saved summary is kept: {primed}");
        assert_eq!(primed[fact_at..].matches("F2").count(), 1, "the fact comes after every older F2 mention in {thread}: {primed}");
        if thread == &id {
            assert!(primed[..fact_at].contains("Tap F2 to start dictation."), "Rich's earlier reply is kept: {primed}");
            assert!(primed.contains("tap F3, then"), "Rich's first turn there carries F3: {primed}");
        }
    }
    std::fs::remove_file(&path).expect("the test ledger is removable");
}

#[test]
fn threads_are_views_over_one_shared_ledger() {
    let (path, ledger) = tmp_ledger("threads");
    let mut spine = support::spine(ledger);
    let germany = spine.create_thread("Germany", &femcboost()).unwrap();
    let hiring = spine.create_thread("Q4 hiring", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["a", "b"])));

    spine.switch_thread(&germany).unwrap();
    spine.submit_prompt("about germany", Source::Text).unwrap();
    spine.switch_thread(&hiring).unwrap();
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
fn reprime_payload_carries_action_ledger_as_ground_truth() {
    // Anti-false-attribution: a recorded action appears in the re-prime as authoritative.
    let (path, mut ledger) = tmp_ledger("attribution");
    let thread = ledger.create_thread("General", &femcboost()).unwrap();
    let turn = ledger.record_prompt_received(&ledger.thread_binding(&thread).unwrap(), "dispatch a worker", Source::Text).unwrap();
    ledger.record_action(&turn, "dispatch", "spawned worker mark-sonnet-f1").unwrap();

    let payload = RePrimePayload::assemble(&ledger, &ledger.thread_binding(&thread).unwrap(), 8, None).unwrap();
    assert_eq!(payload.action_ledger_digest.len(), 1);
    assert_eq!(payload.action_ledger_digest[0].kind, "dispatch");
    let priming = payload.to_priming_prompt();
    assert!(priming.contains("ACTION LEDGER"));
    assert!(priming.contains("spawned worker mark-sonnet-f1"));
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
    fn reprime(&mut self, _priming_text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
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
                reason.contains("asking again is worth a try"),
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
    assert!(!cause.ceo_message.contains("Settings"), "a lost pipe is not fixed in Settings");
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
    assert_eq!(text.ceo_message, jam.ceo_message, "two surfaces, two sentences");
    assert_eq!(text.offers_retry, jam.offers_retry);
    // POSITIVE CONTROL: the shared record is a real classification and not a default that
    // both surfaces happen to share by doing nothing.
    assert_eq!(text.cause, "transient", "{text:?}");
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

/// Work reports reach Rich as internal data. Only his response reaches the durable
/// conversation. Failure retains the report; replay after publishing cannot duplicate it.
#[test]
fn a_jobs_result_is_written_into_the_conversation_once_and_is_there_after_a_relaunch() {
    use richos_core::assignment::{self, NoticeKind, Registration};
    struct ReportDesk(MockCognition);
    struct ImmediateCancel;
    impl richos_core::steering::TurnCancel for ImmediateCancel { fn cancel(&self) -> bool { false } }

    impl Cognition for ReportDesk {
        fn session_id(&self) -> &str { self.0.session_id() }
        fn cancel_handle(&self) -> Option<std::sync::Arc<dyn richos_core::steering::TurnCancel>> { Some(Arc::new(ImmediateCancel)) }
        fn reprime(&mut self, text: &str, sink: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
            self.0.reprime(text, sink)
        }
        fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            panic!("a work report must not run with user-turn tool authority")
        }
        fn prompt_context_only(&mut self, text: &str, sink: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            self.0.prompt(text, sink)
        }
    }
    let (path, ledger) = tmp_ledger("work-result");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Market research", &femcboost()).unwrap();
    let elsewhere = spine.create_thread("Another conversation", &femcboost()).unwrap();
    spine.switch_thread(&elsewhere).unwrap();
    let state = std::env::temp_dir().join(format!("richos-test-work-result-{}-{}",
        std::process::id(), richos_core::util::now_millis()));
    std::fs::create_dir_all(&state).unwrap();
    let receipt = assignment::register(&state, &Registration {
        entity_id: "femcboost".into(), thread_id: thread.clone(), obligation_id: "obligation-1".into(),
        instruction_ledger_ref: format!("ledger:{thread}:turn-1"), instruction_sha256: "0".repeat(64),
        title: "Compare the two markets".into(), repositories: Vec::new(), needs_screen: false,
    }).unwrap();
    let raw = "Retrying the research call. No sources yet. Research complete: the northern market has twice as many buyers.";
    let reply = "The northern market has twice as many buyers.";
    let held = "Your request is waiting until you sign in again.";
    assignment::raise_notice(&state, "femcboost", &thread, &receipt.id, NoticeKind::Held, held).unwrap();
    assignment::raise_notice(&state, "femcboost", &thread, &receipt.id, NoticeKind::Answer, raw).unwrap();

    assert!(spine.say_work_results(&state, &thread).is_err(), "no Rich is available yet");
    assert_eq!(assignment::pending_notices_of(&state, "femcboost", &thread, |k| k.is_result()).unwrap().len(), 1);
    assert!(spine.ledger().messages(&thread).unwrap().is_empty(), "raw backend text leaked on failure");
    let desk = ReportDesk(MockCognition::new("report-desk", vec![reply]));
    let prompts = desk.0.prompts.clone();
    spine.attach_lease(Box::new(desk));
    assert_eq!(spine.say_work_results(&state, &thread).unwrap(), [reply]);
    assert_eq!(spine.active_thread(), Some(elsewhere.as_str()), "delivery changed the selected conversation");
    let input: serde_json::Value = serde_json::from_str(&prompts.lock().unwrap()[0]).unwrap();
    assert_eq!(input["from"], "stu");
    assert_eq!(input["work_report"]["report"], raw);
    assert_eq!(input["work_report"]["request"], "Compare the two markets");
    assert!(spine.say_work_results(&state, &thread).unwrap().is_empty());
    assert_eq!(prompts.lock().unwrap().len(), 1);
    assert!(spine.ledger().messages(&elsewhere).unwrap().is_empty());

    // Simulate a crash after the conversation write but before report acknowledgment.
    let partition = std::fs::read_dir(state.join("assignments")).unwrap().next().unwrap().unwrap().path();
    let record_path = partition.join(format!("{}.json", receipt.id));
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
    for notice in record["notices"].as_array_mut().unwrap() { notice["delivered_at_ms"] = serde_json::Value::Null; }
    std::fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
    drop(spine);
    let mut relaunched = support::spine(Ledger::open(&path).unwrap());
    assert!(relaunched.say_work_results(&state, &thread).unwrap().is_empty(), "a delivered result needs no second model call");
    assert_eq!(relaunched.ledger().messages(&thread).unwrap().iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), [reply]);
    assert_eq!(assignment::take_pending_notices(&state, "femcboost", &thread).unwrap()
        .iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), [held]);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&state).unwrap();
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


#[test]
fn a_user_message_preempts_an_offscreen_report_and_the_report_survives_for_retry() {
    use richos_core::{assignment::{self, NoticeKind, Registration}, steering::{TurnCancel, TurnControl}};
    use std::sync::{mpsc, atomic::{AtomicUsize, Ordering}};
    use std::time::Duration;
    struct Cancel(mpsc::Sender<()>);
    impl TurnCancel for Cancel {
        fn cancel(&self) -> bool { self.0.send(()).is_ok() }
    }
    struct Desk {
        entered: mpsc::Sender<()>, canceled: mpsc::Receiver<()>,
        cancel: Arc<Cancel>, attempts: Arc<AtomicUsize>,
    }
    impl Cognition for Desk {
        fn session_id(&self) -> &str { "report-priority" }
        fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> { Some(self.cancel.clone()) }
        fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }
        fn prompt(&mut self, _: &str, on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            on(TurnItem::Text { seq: 0, text: "Here is your quick answer." });
            Ok("end_turn".into())
        }
        fn prompt_read_only(&mut self, _: &str, on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                on(TurnItem::Text { seq: 0, text: "INTERNAL PARTIAL REPORT" });
                self.entered.send(()).unwrap();
                // load-bound: hang guard only; channel cancellation and durable replies decide correctness, never elapsed time.
                self.canceled.recv_timeout(Duration::from_secs(5)).unwrap();
                return Ok("end_turn".into()); // A completion racing cancellation must still yield.
            }
            on(TurnItem::Text { seq: 0, text: "The work is complete." });
            Ok("end_turn".into())
        }
    }
    let (path, ledger) = tmp_ledger("report-priority");
    let state = path.with_extension("state");
    std::fs::create_dir_all(&state).unwrap();
    let control = TurnControl::open(state.join("intake.jsonl")).unwrap();
    let mut spine = support::spine(ledger);
    spine.set_turn_control(control.clone());
    let report_thread = spine.create_thread("Background", &femcboost()).unwrap();
    let user_thread = spine.create_thread("Current conversation", &femcboost()).unwrap();
    spine.switch_thread(&user_thread).unwrap();
    let receipt = assignment::register(&state, &Registration {
        entity_id: "femcboost".into(), thread_id: report_thread.clone(), obligation_id: "work-report".into(),
        instruction_ledger_ref: format!("ledger:{report_thread}:request"), instruction_sha256: "a".repeat(64),
        title: "Check the market".into(), repositories: Vec::new(), needs_screen: false,
    }).unwrap();
    assignment::raise_notice(&state, "femcboost", &report_thread, &receipt.id, NoticeKind::Answer, "Stu's report").unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let attempts = Arc::new(AtomicUsize::new(0));
    spine.attach_lease(Box::new(Desk {
        entered: entered_tx, canceled: cancel_rx, cancel: Arc::new(Cancel(cancel_tx)), attempts: attempts.clone(),
    }));
    let spine = Arc::new(Mutex::new(spine));
    let worker = {
        let spine = spine.clone(); let state = state.clone(); let thread = report_thread.clone();
        std::thread::spawn(move || spine.lock().unwrap().say_work_results(&state, &thread))
    };
    // load-bound: hang guard for fixture rendezvous; the actual report-start event establishes ordering.
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let user = control.prioritize_user_input();
    control.submit_from_desk(&user_thread, Some(femcboost()), "A quick question").unwrap();
    assert!(worker.join().unwrap().is_err(), "the interrupted report must remain unpublished");
    let mut spine = spine.lock().unwrap();
    assert!(spine.has_pending_work_results(&state, &report_thread).unwrap());
    assert!(spine.messages(&report_thread).unwrap().is_empty(), "partial report text leaked");
    assert!(control.stop_claim().is_none(), "a new message is not a user Stop");
    spine.poll_intake().unwrap();
    assert_eq!(spine.messages(&user_thread).unwrap().last().unwrap().text, "Here is your quick answer.");
    drop(user);
    assert_eq!(spine.say_work_results(&state, &report_thread).unwrap(), ["The work is complete."]);
    assert!(!spine.has_pending_work_results(&state, &report_thread).unwrap());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    drop(spine);
    std::fs::remove_dir_all(state).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn report_failures_keep_the_notice_and_recover_without_publishing_diagnostics() {
    use richos_core::{assignment::{self, NoticeKind, Registration}, cognition::LeaseFactory, steering::TurnCancel};
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Clone, Copy)]
    enum Failure { Closed, ErrorText, ErrorResult, Preparation }
    struct Cancel;
    impl TurnCancel for Cancel { fn cancel(&self) -> bool { true } }
    struct Desk { failure: Option<Failure>, calls: Arc<AtomicUsize> }
    impl Cognition for Desk {
        fn session_id(&self) -> &str { "report-recovery" }
        fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> { Some(Arc::new(Cancel)) }
        fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }
        fn prepare_work_turn(&mut self, _: &richos_core::entity::ThreadBinding, _: &str,
            _: Source, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
            if matches!(self.failure, Some(Failure::Preparation)) {
                return Err(CognitionError::Io("provider binding connection closed".into()));
            }
            Ok(())
        }
        fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            panic!("report authoring must stay read-only")
        }
        fn prompt_read_only(&mut self, _: &str, on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let text = match self.failure {
                Some(Failure::Closed) => return Err(CognitionError::Io("Claude channel closed (child exited?)".into())),
                Some(Failure::ErrorResult) => return Err(CognitionError::Protocol("API Error: 529 Overloaded.".into())),
                Some(Failure::ErrorText) => {
                    self.failure = None; // The connection survives this provider refusal.
                    "API Error: 529 Overloaded."
                }
                Some(Failure::Preparation) => panic!("a failed preparation cannot run the model"),
                None => "Your research is complete.",
            };
            on(TurnItem::Text { seq: 0, text });
            Ok("end_turn".into())
        }
    }
    struct Factory { spawns: Arc<AtomicUsize>, calls: Arc<AtomicUsize> }
    impl LeaseFactory for Factory {
        fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
            self.spawns.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Desk { failure: None, calls: self.calls.clone() }))
        }
    }
    for (tag, failure, replaces) in [("closed", Failure::Closed, 1), ("text", Failure::ErrorText, 0),
        ("error", Failure::ErrorResult, 1), ("preparation", Failure::Preparation, 1)] {
        let (path, ledger) = tmp_ledger(tag);
        let state = path.with_extension("state");
        std::fs::create_dir_all(&state).unwrap();
        let mut spine = support::spine(ledger);
        let thread = spine.create_thread("Research", &femcboost()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let spawns = Arc::new(AtomicUsize::new(0));
        spine.set_lease_factory(Box::new(Factory { spawns: spawns.clone(), calls: calls.clone() }));
        spine.attach_lease(Box::new(Desk { failure: Some(failure), calls: calls.clone() }));
        let receipt = assignment::register(&state, &Registration {
            entity_id: "femcboost".into(), thread_id: thread.clone(), obligation_id: "research".into(),
            instruction_ledger_ref: format!("ledger:{thread}:original"), instruction_sha256: "a".repeat(64),
            title: "Research the market".into(), repositories: Vec::new(), needs_screen: false,
        }).unwrap();
        assignment::raise_notice(&state, "femcboost", &thread, &receipt.id, NoticeKind::Answer, "Stu's actual work result").unwrap();
        assert!(spine.say_work_results(&state, &thread).is_err(), "{tag}: failure must not become a reply");
        assert!(spine.has_pending_work_results(&state, &thread).unwrap(), "{tag}: notice lost");
        assert!(spine.messages(&thread).unwrap().is_empty(), "{tag}: provider diagnostic leaked");
        assert_eq!(spine.say_work_results(&state, &thread).unwrap(), ["Your research is complete."], "{tag}");
        assert_eq!(spawns.load(Ordering::SeqCst), replaces, "{tag}: use a healthy connection");
        assert_eq!(calls.load(Ordering::SeqCst), if matches!(failure, Failure::Preparation) { 1 } else { 2 }, "{tag}");
        assert!(!spine.has_pending_work_results(&state, &thread).unwrap(), "{tag}: missing acknowledgment");
        assert!(spine.say_work_results(&state, &thread).unwrap().is_empty(), "{tag}: duplicate report");
        assert_eq!(spine.messages(&thread).unwrap().len(), 1);
        drop(spine);
        std::fs::remove_dir_all(state).unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
