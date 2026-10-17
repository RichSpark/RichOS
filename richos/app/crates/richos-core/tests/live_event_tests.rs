//! End-to-end tests for the ADDITIVE §13 event family (`live.rs`), driven through the
//! real `Spine` with a mock lease — no live Claude, no network.
//!
//! Four things are proven here, in this order of importance:
//!
//!   1. **The four existing events are untouched.** The exact `StreamEvent` sequence and
//!      the exact JSON payloads `app/STREAMING.md` documents are asserted BYTE-FOR-BYTE,
//!      and asserted to be IDENTICAL with and without a live observer attached. Slice 3
//!      adds a contract for a renderer that does not exist yet; breaking the shipping UI
//!      to do it would be a strict regression.
//!   2. **The wire and a reload agree.** The `rich://activity-upserted` payload is
//!      compared field-by-field with the item `Timeline::project` produces from the same
//!      durable records after a cold reopen.
//!   3. **Nothing crosses an entity boundary, including on a DEFERRED emit** — the
//!      cross-entity negative control, which fails when the guard is removed.
//!   4. **The gate holds**: model reasoning and internal machinery never reach the
//!      webview on this family.

use richos_core::cognition::{Cognition, CognitionError, MockCognition, TurnItem};
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::live::{LiveEvent, LiveObserver};
use richos_core::machinery::{MachineryObserver, MachineryRecord};
use richos_core::spine::WorkerEventsSource;
use richos_core::stream::{StreamEvent, TurnObserver};
use richos_core::timeline::{Timeline, ViewMode, Visibility};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn deeply() -> EntityId {
    EntityId::parse("deeply").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-live-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

// ---------------------------------------------------------------------------
// Recorders
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct RecordingLive {
    events: Arc<Mutex<Vec<(String, Value)>>>,
}

impl RecordingLive {
    fn payloads(&self) -> Vec<(String, Value)> {
        self.events.lock().unwrap().clone()
    }

    fn of(&self, name: &str) -> Vec<Value> {
        self.payloads().into_iter().filter(|(n, _)| n == name).map(|(_, p)| p).collect()
    }

    fn names(&self) -> Vec<String> {
        self.payloads().into_iter().map(|(n, _)| n).collect()
    }
}

impl LiveObserver for RecordingLive {
    fn on_live_event(&self, event: &LiveEvent) {
        // Recorded as NAME + PAYLOAD, i.e. exactly what the Tauri shell forwards to the
        // webview — never the Rust value, so a test cannot pass on a field a renderer
        // would never see.
        self.events.lock().unwrap().push((event.event_name().to_string(), event.payload()));
    }
}

#[derive(Clone, Default)]
struct RecordingStream {
    events: Arc<Mutex<Vec<(String, Value)>>>,
}

impl RecordingStream {
    fn payloads(&self) -> Vec<(String, Value)> {
        self.events.lock().unwrap().clone()
    }
}

impl TurnObserver for RecordingStream {
    fn on_event(&self, event: &StreamEvent) {
        self.events.lock().unwrap().push((event.event_name().to_string(), event.payload()));
    }
}

#[derive(Clone, Default)]
struct RecordingMachinery {
    records: Arc<Mutex<Vec<MachineryRecord>>>,
}

impl MachineryObserver for RecordingMachinery {
    fn on_machinery(&self, record: &MachineryRecord) {
        self.records.lock().unwrap().push(record.clone());
    }
}

// ---------------------------------------------------------------------------
// A lease that interleaves text and machinery on ONE shared counter
// ---------------------------------------------------------------------------

/// What the lease should emit next, in arrival order. `seq` is assigned by the lease at its
/// drain point exactly as `native.rs` does — one counter shared by both arms (§1.4 G1),
/// advanced by the NUMBER of records a frame produced.
enum Beat {
    Text(&'static str),
    /// A raw native stdout frame, turned into `MachineryRecord`s the same way `native.rs`
    /// turns one.
    Frame(Value),
    /// SEVERAL frames that belong to one fixture. On this wire a tool call is three frames
    /// (open, arguments, result) and a delegated Task is two — the ACP adapter squeezed each
    /// into fewer, and a fixture that kept doing so would be testing a wire the binary does
    /// not speak.
    Frames(Vec<Value>),
    /// A side effect that happens BETWEEN two beats — i.e. genuinely mid-turn, while the
    /// spine holds the ledger. Used to reproduce the engine hook writing its worker row in
    /// another process AFTER the tool result that named the worker has already been
    /// observed. It consumes no `seq`, because nothing reached the stream.
    Effect(Box<dyn Fn() + Send>),
    /// The lease saying his answer is complete while the turn runs on (finding 21).
    AnswerComplete,
}

struct ScriptedLease {
    session_id: String,
    script: Vec<Beat>,
    /// When set, `prompt` returns this error AFTER replaying the script — a mid-turn
    /// crash with a POSITIVE termination signal, never a silence.
    die_with: Option<String>,
}

impl ScriptedLease {
    fn new(session_id: &str, script: Vec<Beat>) -> Self {
        ScriptedLease { session_id: session_id.into(), script, die_with: None }
    }
}

impl Cognition for ScriptedLease {
    fn session_id(&self) -> &str {
        &self.session_id
    }



    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let mut seq = 0u64;
        for beat in &self.script {
            match beat {
                Beat::Text(t) => {
                    on_item(TurnItem::Text { seq, text: t });
                    seq += 1;
                }
                Beat::Frame(f) => {
                    for rec in MachineryRecord::from_native_event(f, &self.session_id, seq) {
                        seq += 1;
                        on_item(TurnItem::Machinery(rec));
                    }
                }
                Beat::Frames(fs) => {
                    for f in fs {
                        for rec in MachineryRecord::from_native_event(f, &self.session_id, seq) {
                            seq += 1;
                            on_item(TurnItem::Machinery(rec));
                        }
                    }
                }
                Beat::Effect(f) => f(),
                Beat::AnswerComplete => on_item(TurnItem::AnswerComplete),
            }
        }
        match &self.die_with {
            Some(reason) => Err(CognitionError::Io(reason.clone())),
            None => Ok("end_turn".to_string()),
        }
    }
}

/// THE THREE FRAMES of one real tool call, as measured on the native wire
/// (`docs/verification/native-claude-stream-json-2026-08-31/raw/run9-rust-driven.jsonl`
/// lines 5, 15 and 18): the row OPENS on the stream with `input: {}`, the complete arguments
/// arrive on the whole-message `assistant` frame, and the outcome closes it on a
/// `tool_result` that carries no tool name at all.
///
/// Three where ACP sent two, and the difference is not cosmetic: the merge keeps the LAST
/// payload, so the arguments frame is the only thing that puts what-was-run on the row.
fn tool_lifecycle(id: &str, tool: &str, command: &str, output: &str) -> Vec<Value> {
    vec![
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":id,"name":tool,"input":{}}}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[
               {"type":"tool_use","id":id,"name":tool,"input":{"command":command}}]}}),
        json!({"type":"user","message":{"role":"user","content":[
               {"tool_use_id":id,"type":"tool_result","content":output}]}}),
    ]
}

// ===========================================================================
// 1. THE FOUR EXISTING EVENTS ARE UNTOUCHED
// ===========================================================================

/// Run the identical turn twice — once with NO live observer, once with one — and compare
/// the four documented events byte-for-byte.
fn run_turn_capturing_stream(live: Option<RecordingLive>) -> Vec<(String, Value)> {
    let (path, ledger) = tmp_ledger("compat");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["Hello CEO, I'm Rich."])));
    let stream = RecordingStream::default();
    spine.set_observer(Box::new(stream.clone()));
    if let Some(l) = live {
        spine.set_live_observer(Box::new(l));
    }
    spine.submit_prompt("hi", Source::Text).unwrap();
    let out = stream.payloads();
    drop(spine);
    let _ = std::fs::remove_file(&path);
    out
}

#[test]
fn the_four_documented_events_are_byte_identical_with_and_without_the_additive_family() {
    let without = run_turn_capturing_stream(None);
    let with = run_turn_capturing_stream(Some(RecordingLive::default()));

    // Names and order first — STREAMING.md: "exactly one turn-started, then zero or more
    // chunks, then exactly one terminal event, never both".
    let names: Vec<&str> = without.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["rich://turn-started", "rich://chunk", "rich://chunk", "rich://turn-completed"],
        "the documented sequence for a two-delta reply"
    );
    let with_names: Vec<&str> = with.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, with_names, "attaching a live observer must not change the old family's shape");

    // Then the payloads, field by field. Three values legitimately differ between two
    // independent runs and are normalized rather than ignored: the wall-clock `at` (which
    // STREAMING.md explicitly calls a label, never the ordering key) and the freshly
    // generated thread/turn ids. Every other key AND value must be identical.
    for (i, ((n_a, p_a), (n_b, p_b))) in without.iter().zip(with.iter()).enumerate() {
        assert_eq!(n_a, n_b, "event {i} name");
        let normalize = |v: &Value| {
            let mut o = v.as_object().unwrap().clone();
            o.insert("at".into(), json!("<label>"));
            o.insert("threadId".into(), json!("<thread>"));
            o.insert("turnId".into(), json!("<turn>"));
            Value::Object(o)
        };
        assert_eq!(normalize(p_a), normalize(p_b), "event {i} payload changed when the live family was attached");
    }

    // And the exact documented shapes, so this test fails if a KEY is renamed rather than
    // only if the two runs diverge together.
    let started = &without[0].1;
    assert!(started.get("threadId").is_some() && started.get("turnId").is_some() && started.get("at").is_some());
    assert_eq!(started.as_object().unwrap().len(), 3, "turn-started is {{threadId, turnId, at}}");

    let chunk = &without[1].1;
    assert_eq!(chunk["seq"], json!(0), "seq is a per-turn counter starting at 0");
    assert!(chunk.get("textDelta").is_some());
    assert_eq!(chunk.as_object().unwrap().len(), 5, "chunk is {{threadId, turnId, seq, textDelta, at}}");

    let completed = &without[3].1;
    assert_eq!(completed["stopReason"], json!("end_turn"));
    assert_eq!(completed.as_object().unwrap().len(), 4, "turn-completed is {{threadId, turnId, stopReason, at}}");

    // Concatenating textDelta in seq order still reproduces the reply the ledger holds.
    let text: String = without
        .iter()
        .filter(|(n, _)| n == "rich://chunk")
        .map(|(_, p)| p["textDelta"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(text, "Hello CEO, I'm Rich.");
}

#[test]
fn the_shared_seq_still_gaps_where_machinery_happened_on_the_old_family() {
    // STREAMING.md: "seq is NOT contiguous ... never treat a gap as a lost chunk". The
    // additive family must not have quietly renumbered anything.
    let (path, ledger) = tmp_ledger("seqgap");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Text("Looking at the release"),
            Beat::Frames(tool_lifecycle("toolu_A", "Bash", "cat VERSION", "1.0.0")),
            Beat::Text("It shipped."),
        ],
    )));
    let stream = RecordingStream::default();
    spine.set_observer(Box::new(stream.clone()));
    spine.set_live_observer(Box::new(RecordingLive::default()));
    spine.submit_prompt("did it ship", Source::Text).unwrap();

    let seqs: Vec<u64> = stream
        .payloads()
        .iter()
        .filter(|(n, _)| n == "rich://chunk")
        .map(|(_, p)| p["seq"].as_u64().unwrap())
        .collect();
    assert_eq!(seqs, vec![0, 4], "three tool frames took positions 1, 2 and 3 — the gap is the point");
    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// 2. THE WIRE AND THE RELOAD AGREE
// ===========================================================================

#[test]
fn the_wire_and_the_reload_agree_on_every_field() {
    let (path, ledger) = tmp_ledger("agree");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Text("Checking the version"),
            Beat::Frames(tool_lifecycle("toolu_A", "Bash", "cat engine/VERSION", "1.0.0")),
            Beat::Text("It's 1.0.0."),
        ],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let machinery = RecordingMachinery::default();
    spine.set_machinery_observer(Box::new(machinery.clone()));
    let turn_id = spine.submit_prompt("what version", Source::Text).unwrap();

    // --- the LAST upsert for the tool call is what a reload will show ---
    let upserts = live.of("rich://activity-upserted");
    assert_eq!(
        upserts.len(),
        3,
        "one event per raw record: the open, the arguments, then the merged close"
    );
    let wire = upserts.last().unwrap().clone();

    // --- rebuild the SAME thread from the durable records, the way a cold reopen does ---
    let records = machinery.records.lock().unwrap().clone();
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let reloaded = Timeline::project(spine.ledger(), &binding, &records).unwrap();
    let view = reloaded.view(ViewMode::Ceo);
    let projected = view
        .items()
        .iter()
        .find(|i| matches!(i, richos_core::timeline::TimelineItem::Activity { .. }))
        .expect("the reload projects one activity row");
    let mut reload = serde_json::to_value(projected).unwrap();

    // TWO fields legitimately differ, and the second one is a finding rather than a nit.
    //
    //   `at`             — §13's timestamp label, which a timeline item does not carry.
    //
    //   `bindingRevision` — the LIVE fence carries the revision of the ACTIVATION that
    //     produced the event (`rebind_at_new_revision`, ECS §11.3), while a re-projection
    //     carries whatever revision the binding handed to `Timeline::project` holds — for
    //     `Ledger::thread_binding` that is the thread's DURABLE home revision, which is
    //     lower. Both are correct and neither is the item's own property: the revision is
    //     a fencing token about the read, not about the record. The consequence for §13's
    //     "the renderer rejects events that do not match the immutable binding" is
    //     concrete: the IMMUTABLE part is entityId + threadId, and bindingRevision must be
    //     used as a staleness fence (reject anything OLDER than the current activation),
    //     never as an equality key — an equality check would reject every live event after
    //     any thread switch. `app/STREAMING.md` states this for the renderer.
    let at = wire["at"].clone();
    let live_revision = wire["bindingRevision"].as_u64().unwrap();
    let durable_revision = reload["bindingRevision"].as_u64().unwrap();
    assert!(
        live_revision >= durable_revision,
        "an activation fence never moves backwards: live {live_revision} vs durable {durable_revision}"
    );
    let mut wire_stripped = wire.as_object().unwrap().clone();
    wire_stripped.remove("at");
    wire_stripped.insert("bindingRevision".into(), json!(durable_revision));
    reload.as_object_mut().unwrap().remove("at");
    assert_eq!(
        Value::Object(wire_stripped),
        reload,
        "an event that reached the webview must be the SAME record a restart re-projects — \
         otherwise a cold reopen either duplicates what the CEO saw or contradicts it"
    );
    assert!(at.is_u64(), "§13's `at` is present on the wire");

    // The three properties that matter, named rather than implied by the equality above.
    assert_eq!(wire["kind"], json!("activity"));
    assert_eq!(wire["entityId"], json!("femcboost"));
    assert_eq!(wire["threadId"], json!(thread));
    assert_eq!(wire["turnId"], json!(turn_id));
    assert!(wire["bindingRevision"].is_u64(), "the fencing token is present on the wire");
    assert_eq!(wire["visibility"], json!("ceo"));
    assert_eq!(wire["state"], json!("completed"), "the wire status merged in from the closing update");
    assert_eq!(wire["summary"], json!("Ran a command"), "§5.3: semantic, never raw syntax");
    assert_eq!(wire["sequence"], json!(1), "the OPENING record's shared-counter position, not a new number");
    assert!(wire.get("detail").is_none(), "the exact command was removed, not flagged");
    let flat = serde_json::to_string(&wire).unwrap();
    assert!(!flat.contains("cat engine/VERSION"), "no command text on the calm family: {flat}");
    assert!(!flat.contains("1.0.0"), "no tool output on the calm family: {flat}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_live_message_id_is_the_id_a_reload_projects() {
    let (path, ledger) = tmp_ledger("msgid");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Text("Looking at the release"),
            Beat::Frames(tool_lifecycle("toolu_A", "Bash", "git log -1", "abc123")),
            Beat::Text("It shipped."),
        ],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let turn_id = spine.submit_prompt("did it ship", Source::Text).unwrap();

    let started = live.of("rich://message-started");
    let completed = live.of("rich://message-completed");
    assert_eq!(started.len(), 2, "one run of prose either side of the tool call");
    assert_eq!(completed.len(), 2, "each run is closed exactly once");

    let live_ids: Vec<&str> = started.iter().map(|p| p["messageId"].as_str().unwrap()).collect();
    assert_eq!(live_ids, vec![format!("{turn_id}:text:0"), format!("{turn_id}:text:1")]);

    // Reload from disk and re-project: the same ids come back.
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let reloaded = Timeline::project(spine.ledger(), &binding, &[]).unwrap();
    let projected: Vec<String> = reloaded
        .view(ViewMode::Ceo)
        .items()
        .iter()
        .filter(|i| matches!(i, richos_core::timeline::TimelineItem::RichMessage { .. }))
        .map(|i| i.id().to_string())
        .collect();
    assert_eq!(projected, live_ids, "§13: repeated event IDs are idempotent — across a restart too");

    // The runs split where the LEDGER says the prose was interrupted, and each carries its
    // full text so a consumer that missed every delta is still correct.
    assert_eq!(completed[0]["text"], json!("Looking at the release"));
    assert_eq!(completed[1]["text"], json!("It shipped."));
    assert_eq!(started[0]["seq"], json!(0));
    assert_eq!(started[1]["seq"], json!(4), "the real position, gap and all");

    // And the first run is CLOSED BEFORE the tool call's activity row — §5.2's
    // "commentary, then activity" is live-accurate, not reconstructed at turn end.
    let order = live.names();
    let first_completed = order.iter().position(|n| n == "rich://message-completed").unwrap();
    let first_activity = order.iter().position(|n| n == "rich://activity-upserted").unwrap();
    assert!(first_completed < first_activity, "the run of prose closes when the tool call starts");

    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// 3. THE MESSAGE PHASE
// ===========================================================================

#[test]
fn every_streamed_message_is_phase_unknown_and_never_final() {
    let (path, ledger) = tmp_ledger("phase");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    // The exact shape that makes "the last run is the final answer" false: Rich writes his
    // conclusion, THEN verifies it, THEN says two words. A heuristic would label "Confirmed."
    // as the deliverable and hide the actual answer as commentary.
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Text("The release shipped at 14:02 and the migration is complete."),
            Beat::Frames(tool_lifecycle("toolu_A", "Bash", "git log -1", "abc123")),
            Beat::Text("Confirmed."),
        ],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("did it ship", Source::Text).unwrap();

    for p in live.of("rich://message-started").iter().chain(live.of("rich://message-completed").iter()) {
        assert_eq!(
            p["phase"],
            json!("unknown"),
            "the wire carries no commentary-vs-final signal; message-started fires \
             before the turn is even over"
        );
        assert!(p.get("phase").is_some(), "serialized explicitly — an absent field invites `?? 'final'`");
    }
    // Neither run is labelled, INCLUDING the last one. That is the whole point: the
    // deliverable here is run 0, and run 1 is a two-word verification.
    let completed = live.of("rich://message-completed");
    assert_eq!(completed.last().unwrap()["text"], json!("Confirmed."));
    assert_eq!(completed.last().unwrap()["phase"], json!("unknown"));

    // No turn-status ever claims §11's `streaming_final` either — same missing signal.
    for p in live.of("rich://turn-status") {
        assert_ne!(p["status"], json!("streaming_final"));
    }
    let _ = std::fs::remove_file(&path);
}



// ===========================================================================
// 4. TURN STATUS
// ===========================================================================

#[test]
fn turn_status_walks_the_states_it_can_actually_observe() {
    let (path, ledger) = tmp_ledger("status");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["done"])));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("go", Source::Text).unwrap();

    let statuses: Vec<String> =
        live.of("rich://turn-status").iter().map(|p| p["status"].as_str().unwrap().to_string()).collect();
    assert_eq!(statuses, vec!["queued", "working", "completed"], "§11, restricted to what is observable");

    let s = live.of("rich://turn-status");
    assert_eq!(s[0]["activeDurationMs"], json!(null), "queued: nothing has been timed");
    assert_eq!(s[1]["activeDurationMs"], json!(null), "working: the span is not over, so it is not a span");
    assert!(s[1]["startedAt"].is_u64(), "but startedAt is real, so a live view can tick from it");
    assert!(
        s[2]["activeDurationMs"].is_u64(),
        "completed: ended_at - started_at, MEASURED — never now() - started_at (§6.3)"
    );

    // The thread's sidebar row tracked it, using the same numbers `thread::summaries` uses.
    let summaries: Vec<String> = live
        .of("rich://thread-summary-updated")
        .iter()
        .map(|p| p["status"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(summaries, vec!["queued", "working", "idle"]);
    let last = live.of("rich://thread-summary-updated").last().unwrap().clone();
    assert_eq!(last["title"], json!("General"));
    assert_eq!(last["messageCount"], json!(2), "the CEO's prompt and Rich's reply");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_answer_complete_turn_says_so_while_it_is_still_working_and_ends_as_before() {
    // Product hunt 2026-10-15, finding 21: "Rich is working" stayed up about 22 s after the
    // answer was on screen. The lease's "answer complete" reaches the screen as a second
    // `working` status carrying `answered: true`, between the turn's start and its own end,
    // and the turn still completes exactly once, at its end.
    let (path, ledger) = tmp_ledger("answered");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Text("Here are three."), Beat::AnswerComplete, Beat::Frame(json!({"type":"stream_event",
            "event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t9","name":"Bash","input":{}}}}))],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("go", Source::Text).unwrap();

    let s = live.of("rich://turn-status");
    let statuses: Vec<&str> = s.iter().map(|p| p["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, vec!["queued", "working", "working", "completed"]);
    assert!(s[1].get("answered").is_none(), "the turn's start says nothing about an answer");
    assert_eq!(s[2]["answered"], json!(true));
    assert_eq!(s[2]["startedAt"], s[1]["startedAt"], "the same turn, still timed from its start");
    assert_eq!(s[2]["activeDurationMs"], json!(null), "still running, so not a span");
    assert!(s[3].get("answered").is_none() && s[3]["activeDurationMs"].is_u64());

    drop(spine);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn a_crash_with_no_recovery_path_says_failed_and_says_it_once() {
    let (path, ledger) = tmp_ledger("failed");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let mut lease = ScriptedLease::new("sess-1", vec![Beat::Text("I started to say")]);
    lease.die_with = Some("broken pipe".into());
    spine.attach_lease(Box::new(lease));
    let live = RecordingLive::default();
    let stream = RecordingStream::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.set_observer(Box::new(stream.clone()));

    // No lease factory attached -> no recovery is possible, so `failed` is the truth.
    assert!(spine.submit_prompt("go", Source::Text).is_err());

    let statuses: Vec<String> =
        live.of("rich://turn-status").iter().map(|p| p["status"].as_str().unwrap().to_string()).collect();
    assert_eq!(statuses, vec!["queued", "working", "failed"]);
    assert!(!statuses.contains(&"recovering".to_string()), "nothing is recovering when nothing can recover");

    // The partial reply was still finalized — it is already durable.
    let completed = live.of("rich://message-completed");
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0]["text"], json!("I started to say"));

    // The old family behaved exactly as STREAMING.md documents: partial chunks, then
    // turn-error, and never a turn-completed alongside it.
    let names: Vec<String> = stream.payloads().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, vec!["rich://turn-started", "rich://chunk", "rich://turn-error"]);

    // And no raw reason reached the calm family — the FACT of the failure did, the
    // stack-trace-ish text did not.
    let flat = serde_json::to_string(&live.payloads()).unwrap();
    assert!(!flat.contains("broken pipe"), "the calm family carries state, not a stack trace: {flat}");

    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// 5. THE GATE
// ===========================================================================

#[test]
fn model_reasoning_and_internal_machinery_never_reach_the_calm_family() {
    let (path, ledger) = tmp_ledger("gate");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            // §5.3 lists model reasoning under "do not render". The adapter emits this
            // route even though it is structurally empty on today's models: the native
            // captures carry 7 `thinking` blocks and EVERY ONE has empty text and a
            // signature only. The route is built, so the gate must hold on it.
            Beat::Frame(json!({"type":"assistant","message":{"role":"assistant","content":[
                {"type":"thinking",
                 "thinking":"The CEO probably means the staging deploy, not prod.",
                 "signature":"sig"}]}})),
            Beat::Text("The staging deploy is red."),
        ],
    )));
    let live = RecordingLive::default();
    let machinery = RecordingMachinery::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.set_machinery_observer(Box::new(machinery.clone()));
    spine.submit_prompt("what's broken", Source::Text).unwrap();

    // The thought WAS routed and retained — it just has no render path.
    let recorded = machinery.records.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1, "the technical family still carries it");

    assert!(live.of("rich://activity-upserted").is_empty(), "a thought is Internal in every mode");
    let flat = serde_json::to_string(&live.payloads()).unwrap();
    assert!(
        !flat.contains("probably means"),
        "model reasoning must not appear anywhere on the calm family: {flat}"
    );

    // THE NEGATIVE CONTROL, stated as the guard rather than as the symptom: the event was
    // CONSTRUCTED, carries the reasoning, and is refused only because visibility != Ceo.
    // Remove the `may_reach_webview()` check in `Spine::forward_live` and this content is
    // exactly what leaks.
    let projected = Timeline::project(
        spine.ledger(),
        &spine.ledger().thread_binding(spine.active_thread().unwrap()).unwrap(),
        &recorded,
    )
    .unwrap();
    let internal_row = projected
        .audit_including_internal()
        .iter()
        .find(|i| matches!(i, richos_core::timeline::TimelineItem::Activity { .. }))
        .expect("the row exists — it is the GATE that stops it, not its absence");
    assert_eq!(internal_row.visibility(), Visibility::Internal);
    assert!(projected.view(ViewMode::Technical).items().iter().all(
        |i| !matches!(i, richos_core::timeline::TimelineItem::Activity { .. })
    ), "not even technical mode renders reasoning");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_accounting_update_is_not_a_thing_rich_did() {
    // FOUND BY RUNNING A REAL TURN, not by reading the code
    // (examples/live_events_roundtrip.rs, live session 2026-08-29): `usage_update`,
    // `available_commands_update` and `session_info_update` all normalize to
    // MachineryKind::Unknown, and slice 2a stamped those Visibility::Ceo — so ONE real
    // command produced SIX CEO rows reading "Worked" (positions 0, 1, 5, 8, 12, 13)
    // against ONE "Ran a command". A 6:1 noise ratio on the calm timeline, and the 2026-08-28
    // probe measured 50 usage_updates across five runs, so a longer turn is worse.
    let (path, ledger) = tmp_ledger("accounting");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Frame(json!({"type":"stream_event","event":{"type":"message_delta",
                "delta":{"stop_reason":"tool_use"},
                "usage":{"input_tokens":2,"cache_read_input_tokens":25737}}})),
            Beat::Frames(tool_lifecycle("toolu_A", "Bash", "git rev-parse HEAD", "39a0968")),
            Beat::Frame(json!({"type":"system","subtype":"init","model":"claude-sonnet-5",
                "tools":[]})),
            Beat::Text("It is 39a0968."),
        ],
    )));
    let live = RecordingLive::default();
    let machinery = RecordingMachinery::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.set_machinery_observer(Box::new(machinery.clone()));
    spine.submit_prompt("what is HEAD", Source::Text).unwrap();

    let upserts = live.of("rich://activity-upserted");
    let summaries: Vec<&str> = upserts.iter().map(|p| p["summary"].as_str().unwrap()).collect();
    assert!(
        summaries.iter().all(|s| *s == "Ran a command"),
        "the calm family carries the one real action and no accounting rows: {summaries:?}"
    );
    assert!(!summaries.contains(&"Worked"), "\"Worked\" is not a thing Rich did");

    // NOTHING IS LOST. All five records were routed and retained, and the two untyped ones
    // still render in TECHNICAL mode with their vendor kind intact.
    assert_eq!(machinery.records.lock().unwrap().len(), 5, "every record is still routed");
    let records = machinery.records.lock().unwrap().clone();
    let binding = spine.ledger().thread_binding(spine.active_thread().unwrap()).unwrap();
    let timeline = Timeline::project(spine.ledger(), &binding, &records).unwrap();
    let technical: Vec<String> = timeline
        .view(ViewMode::Technical)
        .items()
        .iter()
        .filter_map(|i| match i {
            richos_core::timeline::TimelineItem::Activity { detail, .. } => {
                detail.as_ref().and_then(|d| d.vendor_kind.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        technical,
        vec!["stream_event:message_delta", "system:init"],
        "§1.4 G5's dim technical lines"
    );

    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// 6. THE CROSS-ENTITY NEGATIVE CONTROL
// ===========================================================================

#[test]
fn no_event_ever_carries_one_entitys_content_under_another_entitys_fence() {
    // TWO entities, TWO threads, ONE spine — the shape slice 1 proved leaks in the
    // re-prime digest and slice 2a proved leaks through a merged machinery row.
    let (path, ledger) = tmp_ledger("crossentity");
    let mut spine = support::spine(ledger);
    let femc_thread = spine.create_thread("Memory strategy", &femcboost()).unwrap();
    let deeply_thread = spine.create_thread("Pricing", &deeply()).unwrap();

    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.attach_lease(Box::new(MockCognition::new(
        "sess-1",
        vec!["FEMCBOOST-ONLY: the coach roster is 40.", "DEEPLY-ONLY: the price is 90."],
    )));

    spine.switch_thread(&femc_thread).unwrap();
    spine.submit_prompt("how many coaches", Source::Text).unwrap();
    spine.switch_thread(&deeply_thread).unwrap();
    spine.submit_prompt("what price", Source::Text).unwrap();

    // Every single event is scoped to the thread that produced it, and the entity of that
    // thread — never the entity that happens to be active.
    let mut femc_events = 0;
    let mut deeply_events = 0;
    for (name, p) in live.payloads() {
        let entity = p["entityId"].as_str().unwrap();
        let thread = p["threadId"].as_str().unwrap();
        let expected = if thread == femc_thread { "femcboost" } else { "deeply" };
        assert_eq!(entity, expected, "{name} carried thread {thread} under entity {entity}");
        assert!(p["bindingRevision"].is_u64(), "{name} must carry the fencing revision");
        assert!(p["turnId"].as_str().is_some_and(|t| !t.is_empty()), "{name} must carry a turn");

        let flat = serde_json::to_string(&p).unwrap();
        if entity == "femcboost" {
            femc_events += 1;
            assert!(!flat.contains("DEEPLY-ONLY"), "{name} leaked deeply's content into femcboost: {flat}");
        } else {
            deeply_events += 1;
            assert!(!flat.contains("FEMCBOOST-ONLY"), "{name} leaked femcboost's content into deeply: {flat}");
        }
    }
    assert!(femc_events > 0 && deeply_events > 0, "both entities actually produced events");

    let _ = std::fs::remove_file(&path);
}


#[test]
fn an_unbound_legacy_thread_emits_nothing_at_all() {
    // Fail-closed, the same posture as `messages()` and `Timeline::project`: a thread with
    // no entity home cannot produce a fence, so it cannot produce an event.
    let path = std::env::temp_dir().join(format!(
        "richos-live-unbound-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    // A thread written before entity scoping existed: no binding, ever.
    std::fs::write(&path, r#"{"event":"ThreadCreated","thread_id":"thr_old","title":"Legacy","at":1}"#.to_string() + "\n")
        .unwrap();
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));

    assert!(spine.switch_thread("thr_old").is_err(), "an unbound thread cannot be activated");
    assert!(spine.submit_prompt("hi", Source::Text).is_err(), "and no turn can be accepted for it");
    assert!(live.payloads().is_empty(), "no binding, no fence, no event");
    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// 5. `rich://worker-upserted` — THE DELEGATION THE CEO SEES WHILE IT HAPPENS
// ===========================================================================
//
// The defect these close, as the §26 fixture measured it before this slice: 0 worker chips
// live, 3 after a `get_timeline` read. During the turn — exactly when the CEO wants to know
// Rich has delegated — a delegation showed as a nameless "Worked" row.

/// A DELEGATED-WORK tool call: the two-part witness `worker-created-handoff.sh` requires —
/// the vendor tool name `Task`, and the harness's async-launch acknowledgement carrying an
/// extractable `agentId`. Missing either half and the call stays an ordinary activity row.
///
/// **TWO frames, because on this wire the two halves of the witness arrive on two.** The
/// name is on the opening frame; the acknowledgement is on a `tool_result` that names no
/// tool. `LiveTurn` memoizes the name per tool call for exactly this reason — see
/// `live.rs`'s `tool_names`.
fn task_call(id: &str, agent_id: &str) -> Vec<Value> {
    vec![
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":id,"name":"Task","input":{}}}}),
        json!({"type":"user","message":{"role":"user","content":[
               {"tool_use_id":id,"type":"tool_result",
                "content":format!("Async agent launched successfully. agentId: {agent_id}")}]}}),
    ]
}

/// One line of the engine's `worker-events.jsonl`, in the emitters' own format
/// (`engine/docs/worker-lifecycle-events.md`).
fn worker_line(state: &str, agent_id: &str, name: &str, agent_type: &str, session: &str, summary: &str) -> String {
    json!({
        "timestamp": "2026-08-29T04:00:00+00:00",
        "lifecycle_state": state,
        "source_hook": "test",
        "agent_id": agent_id,
        "worker_name": name,
        "agent_type": agent_type,
        "session_id": session,
        "summary": summary,
        "host_pid": 4242
    })
    .to_string()
}

fn write_worker_stream(tag: &str, lines: &[String]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "richos-live-workers-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let body = if lines.is_empty() { String::new() } else { lines.join("\n") + "\n" };
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn a_delegation_reaches_the_webview_during_the_turn_not_after_it() {
    // THE DEFECT, closed. `rich://worker-upserted` was deferred at live.rs:26 and unwired
    // in main.js, so the only route a worker row had to the screen was a `get_timeline`
    // snapshot. The assertion here is POSITIONAL, not merely existential: every worker
    // event must land before the turn's terminal status, because "after the turn" is
    // precisely the behavior that was wrong.
    let stream = write_worker_stream(
        "during",
        &[
            worker_line("created", "agt_sage", "Sage", "architecture", "sess-1", ""),
            worker_line("created", "agt_frank", "Frank", "red team", "sess-1", ""),
            worker_line("started", "agt_frank", "", "red team", "sess-1", ""),
        ],
    );
    let (path, ledger) = tmp_ledger("during");
    let mut spine = support::spine(ledger);
    spine.create_thread("Memory strategy", &femcboost()).unwrap();
    spine.set_worker_events(WorkerEventsSource::File(stream.clone()));
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Text("Pulling Sage and Frank onto this."),
            Beat::Frames(task_call("tc_t1", "agt_sage")),
            Beat::Frames(task_call("tc_t2", "agt_frank")),
            Beat::Frames(tool_lifecycle("tc_c1", "Bash", "git status --short", "clean")),
            Beat::Text("Both are running."),
        ],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("who's on the memory strategy?", Source::Text).unwrap();

    let names = live.names();
    let first_worker = names
        .iter()
        .position(|n| n == "rich://worker-upserted")
        .unwrap_or_else(|| panic!("no worker row reached the calm family at all: {names:?}"));
    // The terminal turn status is the LAST event of that name.
    let last_status = names.iter().rposition(|n| n == "rich://turn-status").unwrap();
    assert!(
        first_worker < last_status,
        "the worker rows arrived at or after the turn's terminal status — that is the \
         snapshot-only behavior this slice exists to remove: {names:?}"
    );

    let workers = live.of("rich://worker-upserted");
    let mut by_agent: std::collections::BTreeMap<String, Value> = Default::default();
    for w in &workers {
        by_agent.insert(w["worker"]["agentId"].as_str().unwrap().to_string(), w.clone());
    }
    assert_eq!(
        by_agent.keys().cloned().collect::<Vec<_>>(),
        vec!["agt_frank".to_string(), "agt_sage".to_string()],
        "both delegations reached the wire, joined by identity and nothing else"
    );
    let sage = &by_agent["agt_sage"];
    assert_eq!(sage["kind"], json!("worker_activity"), "a delegation is not a nameless activity row");
    assert_eq!(sage["worker"]["workerName"], json!("Sage"));
    assert_eq!(sage["worker"]["agentType"], json!("architecture"));
    assert_eq!(sage["worker"]["observedState"], json!("created"));
    assert_eq!(sage["worker"]["state"], json!("pending_init"));
    assert_eq!(by_agent["agt_frank"]["worker"]["state"], json!("running"), "`started` opens a running run");
    assert_eq!(sage["visibility"], json!("ceo"));
    assert!(sage.get("detail").is_none(), "the vendor title and paths were removed, not flagged");
    assert_eq!(sage["entityId"], json!("femcboost"), "the ECS fence rides on this event like every other");
    assert!(sage["bindingRevision"].is_u64());

    // The ordinary tool call beside them is untouched, and a delegation is never BOTH.
    let acts = live.of("rich://activity-upserted");
    assert!(
        acts.iter().any(|a| a["summary"] == json!("Ran a command")),
        "a non-delegating tool call still produces its activity row: {acts:?}"
    );
    assert!(
        !acts.iter().any(|a| a["id"] == sage["id"]),
        "one machinery record is ONE row — a delegation must not also be drawn as activity"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&stream);
}

#[test]
fn the_live_worker_row_and_the_reloaded_worker_row_are_the_same_row() {
    // The property that makes a live row safe to draw: a cold reopen must RE-STATE what the
    // CEO already saw — never contradict it, never duplicate it. Proven exactly the way
    // `the_wire_and_the_reload_agree_on_every_field` proves it for an activity row: field
    // by field against the item `Timeline::project_with_workers` builds from the same
    // durable records and the same lifecycle stream.
    let stream = write_worker_stream(
        "agree",
        &[
            worker_line("created", "agt_clark", "Clark", "research", "sess-1", ""),
            worker_line("started", "agt_clark", "", "research", "sess-1", ""),
            worker_line("updated", "agt_clark", "", "", "sess-1", "Pulled 14 sources on Claude Code memory"),
        ],
    );
    let (path, ledger) = tmp_ledger("agree-worker");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Memory strategy", &femcboost()).unwrap();
    spine.set_worker_events(WorkerEventsSource::File(stream.clone()));
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Text("Clark is on the sources."), Beat::Frames(task_call("tc_t1", "agt_clark"))],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let machinery = RecordingMachinery::default();
    spine.set_machinery_observer(Box::new(machinery.clone()));
    spine.submit_prompt("who's reading the sources?", Source::Text).unwrap();

    let wire = live.of("rich://worker-upserted").last().cloned().expect("a live worker row");

    // Rebuild from the durable records + the same stream, the way a cold reopen does.
    let records = machinery.records.lock().unwrap().clone();
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let rows = WorkerEventsSource::File(stream.clone()).read(None);
    let reloaded = Timeline::project_with_workers(spine.ledger(), &binding, &records, &rows).unwrap();
    let view = reloaded.view(ViewMode::Ceo);
    let projected = view
        .items()
        .iter()
        .find(|i| matches!(i, richos_core::timeline::TimelineItem::WorkerActivity { .. }))
        .expect("the reload projects one worker row");
    let mut reload = serde_json::to_value(projected).unwrap();

    // The same two fields that legitimately differ for an activity row, for the same two
    // reasons: `at` is §13's timestamp label, which a timeline item does not carry; and
    // `bindingRevision` is the ACTIVATION's revision live and the thread's durable one on a
    // re-projection. It is a staleness fence, never an equality key.
    let live_revision = wire["bindingRevision"].as_u64().unwrap();
    let durable_revision = reload["bindingRevision"].as_u64().unwrap();
    assert!(live_revision >= durable_revision, "an activation fence never moves backwards");
    let mut wire_stripped = wire.as_object().unwrap().clone();
    wire_stripped.remove("at");
    wire_stripped.insert("bindingRevision".into(), json!(durable_revision));
    reload.as_object_mut().unwrap().remove("at");
    assert_eq!(
        Value::Object(wire_stripped),
        reload,
        "the live worker row and the reloaded worker row must be the SAME row — a row that \
         changes when the turn ends is a new defect, not a fix"
    );

    // Named rather than left implied by the equality above.
    assert_eq!(wire["kind"], json!("worker_activity"));
    assert_eq!(wire["worker"]["workerName"], json!("Clark"));
    assert_eq!(wire["worker"]["latestUpdate"], json!("Pulled 14 sources on Claude Code memory"));
    assert_eq!(wire["worker"]["eventsObserved"], json!(3));

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&stream);
}

#[test]
fn a_run_that_ended_crosses_the_live_wire_as_unknown_never_as_a_completion() {
    // §7.1's `waiting`, `interrupted` and `failed` have NO witness anywhere, and `run_ended`
    // is the honest superset of completed/interrupted/failed. The live path must say the
    // same thing the reload path says, or the row changes meaning when the turn completes.
    let stream = write_worker_stream(
        "ended",
        &[
            worker_line("created", "agt_sage", "Sage", "architecture", "sess-1", ""),
            worker_line("run_ended", "agt_sage", "", "", "sess-1", ""),
        ],
    );
    let (path, ledger) = tmp_ledger("ended-worker");
    let mut spine = support::spine(ledger);
    spine.create_thread("Memory strategy", &femcboost()).unwrap();
    spine.set_worker_events(WorkerEventsSource::File(stream.clone()));
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Frames(task_call("tc_t1", "agt_sage")), Beat::Text("Sage's run is over.")],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("what happened to Sage?", Source::Text).unwrap();

    let workers = live.of("rich://worker-upserted");
    let w = workers.last().cloned().expect("a worker row");
    assert_eq!(w["worker"]["observedState"], json!("run_ended"));
    assert_eq!(
        w["worker"]["state"],
        json!("unknown"),
        "RUN_ENDED_WORKER_STATE is WorkerState::Unknown — `run_ended` is not a completion"
    );
    // Only the three witnessable states may cross this wire, on any event.
    for e in &workers {
        let st = e["worker"]["state"].as_str().unwrap();
        assert!(
            ["pending_init", "running", "unknown"].contains(&st),
            "a worker state the engine cannot witness reached the live wire: {st}"
        );
    }
    // And no field claiming an outcome was invented alongside it.
    let obj = w["worker"].as_object().unwrap();
    for banned in ["completedAt", "resultRef", "errorRef", "reason", "outcome", "durationMs", "elapsedMs"] {
        assert!(!obj.contains_key(banned), "the live payload invented `{banned}`");
    }

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&stream);
}

#[test]
fn no_worker_from_another_session_reaches_the_live_wire() {
    // THE CROSS-ENTITY NEGATIVE CONTROL for this event, with a positive probe and a
    // vacuity check — the same three-part shape the merged-machinery control uses.
    //
    // `agent_id` is the join key and it is NOT globally unique across sessions (the
    // engine's own residue at `~/.claude/worker-events.jsonl` reuses `aTESTWORKER00001`
    // across twelve rows). CLAUSE 3 of the join — a row is admitted only when its
    // `session_id` equals the record's — is the only thing between another entity's worker
    // NAME and authored SUMMARY and a row stamped with THIS binding's entity, thread and
    // turn. Such a row would look perfectly scoped and would not be.
    let foreign_name = "deeply-analyst";
    let foreign_summary = "deeply's Q4 term sheet numbers";
    let stream = write_worker_stream(
        "clause3",
        &[
            // MINE — same agent id, this session.
            worker_line("created", "agt_shared", "Sage", "architecture", "sess-1", ""),
            // ANOTHER SESSION's row, reusing the id, carrying another entity's content.
            worker_line("updated", "agt_shared", foreign_name, "deeply", "some-other-session", foreign_summary),
        ],
    );
    let (path, ledger) = tmp_ledger("clause3");
    let mut spine = support::spine(ledger);
    spine.create_thread("Memory strategy", &femcboost()).unwrap();
    spine.set_worker_events(WorkerEventsSource::File(stream.clone()));
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Frames(task_call("tc_t1", "agt_shared")), Beat::Text("Sage has it.")],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("who's on it", Source::Text).unwrap();

    // POSITIVE PROBE: the correctly-scoped row IS admitted, so the refusal below cannot
    // pass merely because the whole path is broken.
    let w = live
        .of("rich://worker-upserted")
        .last()
        .cloned()
        .expect("POSITIVE PROBE FAILED — the in-session row was refused too, so this control is vacuous");
    assert_eq!(w["worker"]["workerName"], json!("Sage"));
    assert_eq!(w["entityId"], json!("femcboost"));

    // THE REFUSAL: neither the foreign name nor the foreign authored summary appears
    // anywhere on the family — not on this row, not on any other event.
    let flat = serde_json::to_string(&live.payloads()).unwrap();
    assert!(!flat.contains(foreign_name), "another session's worker name reached the live wire: {flat}");
    assert!(!flat.contains("Q4 term sheet"), "another session's authored update reached the live wire");
    assert_eq!(w["worker"]["latestUpdate"], Value::Null, "the only `updated` row belongs to another session");
    assert_eq!(w["worker"]["eventsObserved"], json!(1), "one row admitted of the two on disk");

    // THE CONTROL IS NOT VACUOUS. The guard is one clause; re-apply the join WITHOUT it,
    // over the same public rows, and it admits exactly the row that was refused.
    let rows = WorkerEventsSource::File(stream.clone()).read(None);
    assert_eq!(rows.len(), 2, "both rows are on disk and both parse");
    let without_session_clause: Vec<&richos_core::worker_events::WorkerEventRow> =
        rows.iter().filter(|r| r.agent_id == "agt_shared").collect();
    assert_eq!(
        without_session_clause.len(),
        2,
        "with the session clause deleted the foreign row IS selected — the guard is doing the work"
    );
    assert!(
        without_session_clause.iter().any(|r| r.worker_name == foreign_name && r.summary == foreign_summary),
        "and the row it would admit is the one carrying the other entity's name and words"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&stream);
}

#[test]
fn a_lifecycle_row_that_lands_after_the_tool_result_still_reaches_the_screen_in_the_same_turn() {
    // THE RACE, run rather than reasoned about. The `agentId` is extractable only from the
    // async-launch acknowledgement on the tool RESULT, and `worker-created-handoff.sh`
    // writes its row at about that same instant, from `PostToolUse[Agent]` in another
    // process. Neither order is guaranteed.
    //
    // So: the delegation is observed while the stream is EMPTY (it draws as the ordinary
    // activity row — the honest degrade), the hook then writes, and the NEXT machinery
    // record upgrades it to a worker row UNDER THE SAME ITEM ID. An idempotent upsert, not
    // a second row.
    let stream = write_worker_stream("race", &[]);
    let (path, ledger) = tmp_ledger("race");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Memory strategy", &femcboost()).unwrap();
    spine.set_worker_events(WorkerEventsSource::File(stream.clone()));

    let hook_path = stream.clone();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![
            Beat::Frames(task_call("tc_t1", "agt_late")),
            // The hook fires here — mid-turn, after the result that named the worker.
            Beat::Effect(Box::new(move || {
                std::fs::write(
                    &hook_path,
                    worker_line("created", "agt_late", "Clark", "research", "sess-1", "") + "\n",
                )
                .unwrap();
            })),
            Beat::Frames(tool_lifecycle("tc_c1", "Bash", "git status --short", "clean")),
        ],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let machinery = RecordingMachinery::default();
    spine.set_machinery_observer(Box::new(machinery.clone()));
    spine.submit_prompt("delegate it", Source::Text).unwrap();

    let workers = live.of("rich://worker-upserted");
    let upgraded = workers.first().expect("the late lifecycle row still produced a worker event in this turn");
    let delegation_id = upgraded["id"].as_str().unwrap().to_string();
    assert_eq!(upgraded["worker"]["workerName"], json!("Clark"));

    let acts = live.of("rich://activity-upserted");
    assert!(
        acts.iter().any(|a| a["id"] == json!(delegation_id)),
        "before the hook wrote, the delegation was drawn as an ordinary activity row — the honest degrade"
    );
    // SAME id: §13's "repeated event IDs are idempotent". The renderer upserts by id, so
    // the upgrade replaces the row it already drew instead of adding a second one.
    assert_eq!(workers.len(), 1, "one delegation, one upgrade — not one per subsequent record");

    // And a reload agrees with where it ended up.
    let records = machinery.records.lock().unwrap().clone();
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let rows = WorkerEventsSource::File(stream.clone()).read(None);
    let reloaded = Timeline::project_with_workers(spine.ledger(), &binding, &records, &rows).unwrap();
    let view = reloaded.view(ViewMode::Ceo);
    let projected: Vec<&richos_core::timeline::TimelineItem> = view
        .items()
        .iter()
        .filter(|i| matches!(i, richos_core::timeline::TimelineItem::WorkerActivity { .. }))
        .collect();
    assert_eq!(projected.len(), 1, "the reload projects exactly one worker row");
    assert_eq!(projected[0].id(), delegation_id, "and it is the row the live path last drew");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&stream);
}

/// The three frames the native wire spends on one SYNCHRONOUS delegation: the streamed open
/// (no arguments yet), the `assistant` frame carrying the call's whole input, and a plain
/// `tool_result` with no async-launch acknowledgement, so it never becomes a worker.
fn synchronous_delegation(id: &str, input: Value) -> Vec<Value> {
    vec![
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":id,"name":"Agent","input":{}}}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[
               {"type":"tool_use","id":id,"name":"Agent","input":input}]}}),
        json!({"type":"user","message":{"role":"user","content":[
               {"tool_use_id":id,"type":"tool_result","content":"The login works again."}]}}),
    ]
}

#[test]
fn a_delegation_row_on_the_wire_names_what_rich_handed_over_and_to_whom() {
    // Product hunt 2026-10-15, finding 13: a row that just says "Worked" names nothing. A
    // delegation stays in his view (it is not a nameless tool call), so its row has to say
    // what Rich handed over and to whom, read off the delegation call's own arguments.
    let (path, ledger) = tmp_ledger("delegname");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Frames(synchronous_delegation(
            "toolu_D",
            json!({"description":"Fix the login bug","name":"mark-sonnet-f1","subagent_type":"mark",
                   "prompt":"The login page rejects a correct password."}),
        ))],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    spine.submit_prompt("get the login fixed", Source::Text).unwrap();

    let upserts = live.of("rich://activity-upserted");
    let summaries: Vec<&str> = upserts.iter().map(|p| p["summary"].as_str().unwrap()).collect();
    assert!(
        summaries.contains(&"Handed to Mark: Fix the login bug"),
        "the delegation row names the work and the teammate: {summaries:?}"
    );
    assert!(!summaries.contains(&"Worked"), "and never reads a bare \"Worked\": {summaries:?}");

    std::fs::remove_file(&path).unwrap_or_default();
}

#[test]
fn a_delegation_on_an_internal_turn_is_constructed_and_then_refused() {
    // The gate, on the richest payload this family carries. A re-prime or rotation turn is
    // one the CEO must never see at all, and a delegation inside one would carry a worker's
    // name and authored words. The event is CONSTRUCTED — so the gate has something to
    // refuse and this control has something to observe — and stops at
    // `LiveEvent::may_reach_webview`, exactly like an internal activity row.
    let stream = write_worker_stream(
        "internal",
        &[worker_line("created", "agt_hidden", "SECRET-WORKER", "internal", "sess-1", "")],
    );
    let (path, ledger) = tmp_ledger("internal-worker");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Memory strategy", &femcboost()).unwrap();
    spine.set_worker_events(WorkerEventsSource::File(stream.clone()));
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Frames(task_call("tc_t1", "agt_hidden")), Beat::Text("priming")],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let machinery = RecordingMachinery::default();
    spine.set_machinery_observer(Box::new(machinery.clone()));
    spine.submit_prompt("[re-prime]", Source::Internal).unwrap();

    assert!(
        live.of("rich://worker-upserted").is_empty(),
        "a delegation inside an internal turn has no render path in any mode"
    );
    let flat = serde_json::to_string(&live.payloads()).unwrap();
    assert!(!flat.contains("SECRET-WORKER"), "an internal turn's worker name reached the wire: {flat}");

    // NOT VACUOUS: the row exists and carries the name — it is the GATE that stops it, not
    // its absence. `audit_including_internal` is the only way to see it, by design.
    let records = machinery.records.lock().unwrap().clone();
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let rows = WorkerEventsSource::File(stream.clone()).read(None);
    let reloaded = Timeline::project_with_workers(spine.ledger(), &binding, &records, &rows).unwrap();
    let internal_row = reloaded
        .audit_including_internal()
        .iter()
        .find(|i| matches!(i, richos_core::timeline::TimelineItem::WorkerActivity { .. }))
        .expect("the worker row was BUILT — the gate is what refuses it");
    assert_eq!(internal_row.visibility(), Visibility::Internal);
    assert!(
        reloaded.view(ViewMode::Technical).items().iter().all(
            |i| !matches!(i, richos_core::timeline::TimelineItem::WorkerActivity { .. })
        ),
        "not even technical mode renders an internal turn's delegation"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&stream);
}

#[test]
fn a_pressed_button_s_words_reach_rich_and_are_never_shown_as_his() {
    // Product hunt 2026-10-15, finding 14: pressing "Start the questions" posted "Let's do the
    // twenty minutes of questions about my business." as his own message, though he never typed
    // it. Rich still receives the sentence; no surface (the live row, the timeline the window
    // and the phone read, the message list) shows it in his name.
    let (path, ledger) = tmp_ledger("pressed");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("Onboarding", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ScriptedLease::new(
        "sess-1",
        vec![Beat::Text("Great, let us do it. What does the company do?")],
    )));
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));
    let words = "Let's do the twenty minutes of questions about my business.";
    let turn_id = spine.submit_pressed_prompt_to(&thread, words).unwrap();

    assert_eq!(spine.ledger().turn(&turn_id).unwrap().user_text, words, "Rich received the sentence");
    assert!(live.of("rich://ceo-message").is_empty(), "no live row in his name: {:?}", live.names());
    let ceo = spine.timeline(&thread).unwrap().view(ViewMode::Ceo);
    assert!(
        !ceo.items().iter().any(|i| matches!(i, richos_core::timeline::TimelineItem::UserMessage { .. })),
        "no message of his in the conversation"
    );
    assert!(
        ceo.items().iter().any(|i| matches!(i, richos_core::timeline::TimelineItem::RichMessage { .. })),
        "and Rich's answer is there"
    );
    let messages = spine.messages(&thread).unwrap();
    assert!(messages.iter().all(|m| m.role != "user"), "no line of his in the message list");

    // The control: the same sentence TYPED by him is his row.
    spine.submit_prompt_to(&thread, words, Source::Text).unwrap();
    assert_eq!(live.of("rich://ceo-message").len(), 1, "his own typed words are his row");

    std::fs::remove_file(&path).expect("the test ledger is removable");
}
