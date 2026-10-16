//! TIER C, WIRED — company memory reaching a real re-prime, and what happens when it can't.
//!
//! The gap these tests close (open-items 3.5): `spine.rs` never set a compiler, so every
//! re-prime primed a fresh Rich with no company memory at all while the compiler sat
//! complete one directory away. The seam existed; nothing was plugged into it.
//!
//! Every test here runs against a FAKE compiler holding invented content. Not one byte of
//! the CEO's corpus is in this repository, and `richos` goes public. The real compiler is
//! exercised end to end against the real corpus separately — `examples/loro_reprime_demo.rs`
//! — which reads a corpus root from the environment and prints, never commits.

use richos_core::cognition::MockCognition;
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::reprime::{LoroContextCompiler, LoroTier, RePrimePayload, SliceRequest, DEFAULT_TAIL_TURNS};
use richos_core::spine::Spine;
use std::sync::{Arc, Mutex};

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

/// A per-process counter, because `now_millis()` alone is NOT unique: cargo runs these
/// tests in parallel threads of ONE process, three of them call this within the same
/// millisecond, and they then share a ledger file and each other's turns. That produced
/// three failures that passed individually — a test-harness collision, not a product
/// defect, and worth naming so nobody re-derives it.
static LEDGER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-loro-test-{tag}-{}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis(),
        LEDGER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

/// A compiler that records what it was ASKED, and answers with whatever it was told to.
#[derive(Clone)]
struct FakeCompiler {
    answer: LoroTier,
    asked: Arc<Mutex<Vec<(String, String, String, usize)>>>,
}

impl FakeCompiler {
    fn new(answer: LoroTier) -> Self {
        FakeCompiler { answer, asked: Arc::new(Mutex::new(Vec::new())) }
    }
}

impl LoroContextCompiler for FakeCompiler {
    fn compile_slice(&self, req: &SliceRequest<'_>) -> LoroTier {
        self.asked.lock().unwrap().push((
            req.thread_id.to_string(),
            req.entity_id.to_string(),
            req.topic.to_string(),
            req.budget_chars,
        ));
        self.answer.clone()
    }
}

fn binding(spine: &mut Spine) -> richos_core::entity::ThreadBinding {
    spine.ensure_active_thread_in(&femcboost()).unwrap()
}

// ---------------------------------------------------------------------------
// the four states, rendered
// ---------------------------------------------------------------------------

#[test]
fn a_compiled_slice_is_injected_verbatim_with_no_second_heading() {
    let p = payload_with_tier(LoroTier::Slice(
        "COMPANY MEMORY (loro) — bearing on: \"pricing\"\n• [decision] Pricing moved to per-seat".into(),
    ));
    let prompt = p.to_priming_prompt();
    assert!(support::context(&prompt)["loro"]["Slice"].as_str().unwrap().contains("pricing"), "{prompt}");
    assert!(prompt.contains("• [decision] Pricing moved to per-seat"));
    // CONTEXT-CONTRACT.md §3: `slice.text` is self-contained and carries its own heading.
    // The old render prefixed "RELEVANT COMPANY MEMORY (loro slice): ", which doubled it.
    assert!(!prompt.contains("RELEVANT COMPANY MEMORY"), "the heading must not be doubled: {prompt}");
}

#[test]
fn an_empty_corpus_says_nothing_is_recorded_and_never_fabricates_a_slice() {
    // The removed absolute floor used to compile a one-record partition to nothing behind a
    // thin slice; the failure to guard against is the opposite one — inventing content, or
    // falling silent so a successor infers there is none.
    let thin = "COMPANY MEMORY (loro): nothing recorded bears on \"pricing\". Do not assume company \
                facts — ask the CEO or check a live system.";
    let p = payload_with_tier(LoroTier::NothingRecorded(thin.into()));
    let prompt = p.to_priming_prompt();
    assert!(support::context(&prompt)["loro"]["NothingRecorded"].as_str().unwrap().contains("pricing"), "{prompt}");
    assert!(prompt.contains("Do not assume company facts"), "{prompt}");
    assert!(!prompt.contains("Unavailable"), "a CHECKED nothing is not an unknown: {prompt}");
}

#[test]
fn an_unavailable_compiler_states_the_unknown_instead_of_looking_like_an_empty_loro() {
    // Same rule, same reason as the LIVE WORKER STATE section: silence reads to a successor
    // as a denial. "loro could not be consulted" and "loro holds nothing" must not render
    // the same, or a fresh Rich tells the CEO his company has no position on something it
    // decided.
    let p = payload_with_tier(LoroTier::Unavailable("the loro compiler exited 2: no corpus configured".into()));
    let prompt = p.to_priming_prompt();
    assert!(prompt.contains("Unavailable"), "{prompt}");
    assert!(prompt.contains("the loro compiler exited 2"), "the REASON is carried, not swallowed: {prompt}");
    assert!(support::context(&prompt)["loro"].get("NothingRecorded").is_none(), "{prompt}");
    assert!(!prompt.contains("nothing recorded bears on"), "{prompt}");
}

#[test]
fn no_compiler_attached_says_so_about_the_install_not_about_the_memory() {
    let p = payload_with_tier(LoroTier::NotWired);
    let prompt = p.to_priming_prompt();
    assert!(support::context(&prompt)["loro"]=="NotWired", "{prompt}");
    assert!(support::context(&prompt)["loro"]!="NothingRecorded", "{prompt}");
}

fn payload_with_tier(tier: LoroTier) -> RePrimePayload {
    let (path, mut ledger) = tmp_ledger("render");
    let thread = ledger.create_thread("Pricing", &femcboost()).unwrap();
    let b = ledger.thread_binding(&thread).unwrap();
    let turn = ledger.record_prompt_received(&b, "how should we price it", Source::Text).unwrap();
    ledger.append_assistant_delta(&turn, "per seat", 1).unwrap();
    ledger.complete_turn(&turn, "end_turn").unwrap();
    let mut payload = RePrimePayload::assemble(&ledger, &b, DEFAULT_TAIL_TURNS, None).unwrap();
    payload.loro = tier;
    let _ = std::fs::remove_file(&path);
    payload
}

// ---------------------------------------------------------------------------
// the request the spine actually builds
// ---------------------------------------------------------------------------

#[test]
fn the_topic_is_the_ceos_current_intent_and_never_richs_own_reply() {
    let (path, mut ledger) = tmp_ledger("topic");
    let thread = ledger.create_thread("Pricing", &femcboost()).unwrap();
    let b = ledger.thread_binding(&thread).unwrap();
    let t1 = ledger.record_prompt_received(&b, "what did we decide about pricing", Source::Text).unwrap();
    ledger.append_assistant_delta(&t1, "we moved to per-seat, and margins are 40%", 1).unwrap();
    ledger.complete_turn(&t1, "end_turn").unwrap();
    let payload = RePrimePayload::assemble(&ledger, &b, DEFAULT_TAIL_TURNS, None).unwrap();
    // No in-flight turn, so the fallback fires: CONTEXT-CONTRACT.md §3's "send its last
    // user turn". Rich's own reply is NOT a topic — a hallucinated noun in it would
    // otherwise steer what company memory the next re-prime retrieves.
    assert_eq!(payload.topic(), Some("what did we decide about pricing"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_unanswered_ask_outranks_the_tail_because_that_is_what_current_intent_means() {
    let (path, mut ledger) = tmp_ledger("intent");
    let thread = ledger.create_thread("Pricing", &femcboost()).unwrap();
    let b = ledger.thread_binding(&thread).unwrap();
    let t1 = ledger.record_prompt_received(&b, "an older finished question", Source::Text).unwrap();
    ledger.append_assistant_delta(&t1, "answered", 1).unwrap();
    ledger.complete_turn(&t1, "end_turn").unwrap();
    ledger.record_prompt_received(&b, "the thing we are doing right now", Source::Text).unwrap();
    let payload = RePrimePayload::assemble(&ledger, &b, DEFAULT_TAIL_TURNS, None).unwrap();
    assert_eq!(payload.topic(), Some("the thing we are doing right now"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_thread_with_nothing_the_ceo_has_said_is_not_compiled_at_all() {
    let (path, mut ledger) = tmp_ledger("notopic");
    let thread = ledger.create_thread("Empty", &femcboost()).unwrap();
    let b = ledger.thread_binding(&thread).unwrap();
    let payload = RePrimePayload::assemble(&ledger, &b, DEFAULT_TAIL_TURNS, None).unwrap();
    assert_eq!(payload.topic(), None, "there is no such thing as a topic-less slice");
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// end to end through the spine — the seam that was never set
// ---------------------------------------------------------------------------

#[test]
fn priming_a_lease_now_carries_company_memory_and_asks_for_the_right_thing() {
    let (path, ledger) = tmp_ledger("e2e");
    let mut spine = support::spine(ledger);
    let b = binding(&mut spine);
    assert!(!spine.has_loro_context_compiler(), "the seam is OFF by default — that is the shipped default");

    let fake = FakeCompiler::new(LoroTier::Slice(
        "COMPANY MEMORY (loro) — bearing on: \"how do we price it\"\n• [decision] per-seat".into(),
    ));
    let asked = fake.asked.clone();
    spine.set_loro_context_compiler(Box::new(fake));

    let mock = MockCognition::new("s-1", vec!["ok"]);
    let reprimes = mock.reprimes.clone();
    spine.attach_lease(Box::new(mock));
    spine.submit_prompt("how do we price it", Source::Text).unwrap();

    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1, "compiled exactly once per lease, at priming");
    let (thread_id, entity_id, topic, budget) = &asked[0];
    assert_eq!(thread_id, b.thread_id());
    assert_eq!(entity_id, "femcboost", "the ENTITY is passed — the compiler cannot recover it from a thread id");
    assert_eq!(topic, "how do we price it");
    assert_eq!(*budget, richos_core::reprime::DEFAULT_LORO_BUDGET_CHARS);

    let primed = reprimes.lock().unwrap();
    assert_eq!(primed.len(), 1);
    assert!(primed[0].contains("• [decision] per-seat"), "the slice reached the priming turn: {}", primed[0]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_compiler_that_cannot_answer_never_takes_down_the_turn() {
    // CONTEXT-CONTRACT.md §3: exit != 0 -> no slice, "never fail the turn on a memory
    // miss". The trait has no error arm, so there is no `?` for a future caller to add.
    let (path, ledger) = tmp_ledger("nofail");
    let mut spine = support::spine(ledger);
    binding(&mut spine);
    spine.set_loro_context_compiler(Box::new(FakeCompiler::new(LoroTier::Unavailable("node is not installed".into()))));
    let mock = MockCognition::new("s-1", vec!["ok"]);
    let reprimes = mock.reprimes.clone();
    spine.attach_lease(Box::new(mock));
    spine.submit_prompt("hello", Source::Text).unwrap();
    let primed = reprimes.lock().unwrap();
    assert!(primed[0].contains("node is not installed"), "{}", primed[0]);
    assert!(support::context(&primed[0]).is_object(), "the turn completed normally");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_rotation_recompiles_rather_than_reusing_the_slice_the_previous_lease_was_given() {
    // The topic moves with the conversation. A rotation that replayed the slice compiled at
    // boot would ground the successor in whatever the CEO happened to be asking about hours
    // earlier — stale company memory, injected under a header calling it authoritative.
    let (path, ledger) = tmp_ledger("rot");
    let mut spine = support::spine(ledger);
    binding(&mut spine);
    let fake = FakeCompiler::new(LoroTier::Slice("COMPANY MEMORY (loro) — bearing on: \"x\"\n• [fact] x".into()));
    let asked = fake.asked.clone();
    spine.set_loro_context_compiler(Box::new(fake));
    spine.set_lease_factory(Box::new(richos_core::cognition::MockLeaseFactory::new(vec!["ok", "ok", "ok"])));
    spine.attach_lease(Box::new(MockCognition::new("s-1", vec!["ok"])));

    spine.submit_prompt("first question about signing", Source::Text).unwrap();
    spine.request_rotation("test").unwrap();
    spine.submit_prompt("second question about packaging", Source::Text).unwrap();

    let asked = asked.lock().unwrap();
    assert!(asked.len() >= 2, "the rotation compiled its own slice: {asked:?}");
    let topics: Vec<&str> = asked.iter().map(|a| a.2.as_str()).collect();
    assert!(topics.contains(&"first question about signing"), "{topics:?}");
    let _ = std::fs::remove_file(&path);
}

/// **PART 1 HUNT FINDING 43.** The provenance sink describes the memory the CURRENT session was
/// given, but it was only ever replaced by an ACCEPTED slice. A session primed afterwards with a
/// thin answer, a refusal or a compiler failure kept the previous session's records, and a
/// correction could be filed citing memory this session never saw.
#[test]
fn a_session_primed_without_memory_keeps_no_provenance_from_the_session_before_it() {
    use richos_core::loro::{InjectedSlice, SharedSliceProvenance, SliceProvenance};
    let (path, ledger) = tmp_ledger("prov-forget");
    let mut spine = support::spine(ledger);
    let b = binding(&mut spine);
    let sink: SharedSliceProvenance = Arc::new(Mutex::new(SliceProvenance::new()));
    spine.set_loro_provenance(Arc::clone(&sink));
    // What an earlier session's accepted slice left behind for this thread.
    sink.lock().unwrap().record(InjectedSlice {
        thread_id: b.thread_id().to_string(),
        entity_id: "femcboost".into(),
        topic: "an earlier topic".into(),
        fingerprint: "sha256:earlier".into(),
        compiler: "compiler-1".into(),
        at: 1,
        records: Vec::new(),
    });
    assert!(sink.lock().unwrap().for_thread(b.thread_id()).is_some(), "the control entry was not recorded");

    // The session that is primed now is given nothing: the compiler found nothing recorded.
    spine.set_loro_context_compiler(Box::new(FakeCompiler::new(LoroTier::NothingRecorded(
        "COMPANY MEMORY (loro): nothing recorded bears on \"hello\".".into(),
    ))));
    spine.attach_lease(Box::new(MockCognition::new("s-2", vec!["ok"])));
    spine.submit_prompt("hello", Source::Text).unwrap();

    assert!(
        sink.lock().unwrap().for_thread(b.thread_id()).is_none(),
        "the previous session's memory is still resolvable after a session that was given none"
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn pending_question_selects_memory_without_becoming_a_hidden_model_request() {
    let (path, ledger) = tmp_ledger("pending-query-only");
    let mut spine = support::spine(ledger);
    let b = binding(&mut spine);
    // The compiler returns facts, not an echo of the pending request.
    let fake = FakeCompiler::new(LoroTier::Slice("COMPANY MEMORY: verified relevant facts".into()));
    let asked = fake.asked.clone();
    spine.set_loro_context_compiler(Box::new(fake));
    let first = "Current question about notebook pricing, unique 901.";
    let second = "Changed question about late shipments, unique 902.";
    let first_mock = MockCognition::new("initial", vec!["Pricing answer"]);
    let first_primes = first_mock.reprimes.clone();
    spine.attach_lease(Box::new(first_mock));
    spine.submit_prompt(first, Source::Text).unwrap();
    assert!(!first_primes.lock().unwrap()[0].contains(first));
    assert!(first_primes.lock().unwrap()[0].contains("verified relevant facts"));
    let second_mock = MockCognition::new("fresh", vec!["Shipping answer"]);
    let second_primes = second_mock.reprimes.clone();
    spine.attach_lease(Box::new(second_mock));
    spine.submit_prompt(second, Source::Text).unwrap();
    let primed = second_primes.lock().unwrap();
    assert!(primed[0].contains(first), "completed conversation is preserved");
    assert!(!primed[0].contains(second), "new request is not secretly delivered");
    assert!(!primed[0].contains("CURRENT INTENT (what we are doing right now)"));
    let queries = asked.lock().unwrap();
    assert_eq!(queries.iter().map(|q| q.2.as_str()).collect::<Vec<_>>(), [first, second]);
    assert!(queries.iter().all(|q| q.0 == b.thread_id() && q.1 == "femcboost"));
    let _ = std::fs::remove_file(&path);
}
