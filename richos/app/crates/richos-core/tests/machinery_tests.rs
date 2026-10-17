//! Machinery routing + retention, end to end through the spine.
//!
//! Built to `docs/plans/richos-techy-mode-2026-08-26.md` Phase 1 (§5) and verified against
//! the frames MEASURED on the native `claude` wire
//! (`docs/verification/native-claude-stream-json-2026-08-31/raw/run9-rust-driven.jsonl` and
//! `.../native-claude-tool-status-2026-08-31/raw/run13-longtool-bash-ticking.jsonl`).
//! **Ported from the ACP wire when the adapter was deleted (`wiki/ceo-decisions.md` §16)** —
//! same tests, same invariants, new vocabulary. Every payload below is a verbatim shape from
//! those artifacts — an empty `input` on the open frame, a name-less `tool_result`, a
//! heartbeat keyed on `parent_tool_use_id` and all — so these tests fail if the
//! normalization stops matching the wire, not merely if it stops matching my opinion of it.
//!
//! No live Claude and no network: the lease is a mock that replays recorded wire shapes.
//! The real-binary proof is `examples/machinery_roundtrip.rs`.

use richos_core::cognition::{Cognition, CognitionError, TurnItem};
use richos_core::journal::MachineryJournal;
use richos_core::ledger::{Ledger, Source};
use richos_core::machinery::{MachineryKind, MachineryObserver, MachineryRecord, ToolStatus, EVENT_MACHINERY};
use richos_core::entity::EntityId;
use richos_core::spine::Spine;
use richos_core::timeline::ViewMode;

/// The dogfood entity these tests run under. Every thread now has an immutable entity
/// home (ECS §3.2) and there is no entity-less path, so the tests NAME one rather than
/// inheriting a default that no longer exists.
fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}
use richos_core::stream::{StreamEvent, TurnObserver, EVENT_CHUNK, EVENT_PROACTIVE_MESSAGE, EVENT_TURN_COMPLETED, EVENT_TURN_ERROR, EVENT_TURN_STARTED};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

mod support;

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("richos-mach-{tag}-{}", std::process::id()))
        .join(format!("{:?}", std::time::SystemTime::now()).replace([' ', ':', '{', '}'], "_"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One turn of REAL recorded traffic: "text, a whole tool-call lifecycle, more text", which
/// is precisely the interleaving §1.4 G1 exists to make reconstructible.
///
/// SIX machinery frames for one tool call where ACP had four, because the native wire opens
/// the row on the STREAM before the arguments exist and closes it with a separate
/// `tool_result` — and adds a heartbeat ACP had no equivalent for at all.
fn recorded_turn() -> Vec<Item> {
    vec![
        Item::Text("Let me check.".into()),
        // run9:5 — the OPEN frame: the real tool name, `input: {}`. Deviation 2.
        Item::Frame(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_A","name":"Bash","input":{},
                             "caller":{"type":"direct"}}}})),
        // run9:15 — the complete arguments, on the whole-message frame. NO status: on this
        // wire status is a position, not a field (deviation 3), so this must not blank one.
        Item::Frame(json!({"type":"assistant","message":{"role":"assistant","content":[
            {"type":"tool_use","id":"toolu_A","name":"Bash",
             "input":{"command":"cat engine/VERSION"}}]}})),
        // run9:17 — the permission request, auto-approved by `native::decide_permission`.
        Item::Permission(json!({"subtype":"can_use_tool","tool_name":"Bash","display_name":"Bash",
            "input":{"command":"cat engine/VERSION"},"description":"cat engine/VERSION",
            "decision_reason":"Path is outside allowed working directories",
            "decision_reason_type":"workingDir","tool_use_id":"toolu_A"})),
        // run13:22 — the 30s heartbeat, keyed on `parent_tool_use_id` (deviation 1). This
        // is the frame ACP had NO equivalent for: `in_progress` never once appeared there.
        Item::Frame(json!({"type":"tool_progress","tool_use_id":"toolu_A-heartbeat-0",
            "tool_name":"Bash","parent_tool_use_id":"toolu_A","elapsed_time_seconds":30,
            "heartbeat":true})),
        // run9:18 — terminal. Carries the outcome and NO tool name.
        Item::Frame(json!({"type":"user","message":{"role":"user","content":[
            {"tool_use_id":"toolu_A","type":"tool_result","content":"1.0.0"}]}})),
        // run9:19 — untyped vendor frame: retained as Unknown, never dropped.
        Item::Frame(json!({"type":"stream_event","event":{"type":"message_delta",
            "delta":{"stop_reason":"tool_use"},
            "usage":{"input_tokens":2,"cache_read_input_tokens":25737,
                     "cache_creation_input_tokens":3603,"output_tokens":95}}})),
        // §1.2's ONE deliberate drop, ported: a `user` frame's text block.
        Item::Frame(json!({"type":"user","message":{"role":"user","content":[
            {"type":"text","text":"[Request interrupted by user]"}]}})),
        Item::Text(" It says 1.0.0.".into()),
    ]
}

enum Item {
    Text(String),
    Frame(Value),
    Permission(Value),
}

/// A lease that replays recorded wire traffic, assigning `seq` the way the real
/// `NativeClient::prompt` drain loop does — ONE counter over text and machinery, advanced by
/// the NUMBER of records a frame produced, and no position consumed by a dropped frame.
struct ReplayCognition {
    session_id: String,
    script: Vec<Item>,
    reprime_machinery: bool,
}

impl ReplayCognition {
    fn new(session_id: &str, script: Vec<Item>) -> Self {
        ReplayCognition { session_id: session_id.into(), script, reprime_machinery: false }
    }
    fn replay(&self, items: &[Item], on_item: &mut dyn FnMut(TurnItem)) {
        let mut seq = 0u64;
        for it in items {
            match it {
                Item::Text(t) => {
                    on_item(TurnItem::Text { seq, text: t });
                    seq += 1;
                }
                Item::Frame(f) => {
                    for r in MachineryRecord::from_native_event(f, &self.session_id, seq) {
                        seq += 1;
                        on_item(TurnItem::Machinery(r));
                    }
                }
                Item::Permission(p) => {
                    on_item(TurnItem::Machinery(MachineryRecord::from_permission_request(
                        p,
                        "allow",
                        &self.session_id,
                        seq,
                    )));
                    seq += 1;
                }
            }
        }
    }
}

impl Cognition for ReplayCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let script = std::mem::take(&mut self.script);
        self.replay(&script, on_item);
        self.script = script;
        Ok("end_turn".into())
    }
}

#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<StreamEvent>>>,
}
impl TurnObserver for RecordingObserver {
    fn on_event(&self, event: &StreamEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

#[derive(Clone, Default)]
struct RecordingMachineryObserver {
    records: Arc<Mutex<Vec<MachineryRecord>>>,
}
impl MachineryObserver for RecordingMachineryObserver {
    fn on_machinery(&self, record: &MachineryRecord) {
        self.records.lock().unwrap().push(record.clone());
    }
}

fn spine_with_journal(dir: &std::path::Path) -> (Spine, MachineryJournal) {
    let ledger = Ledger::open(dir.join("conversation-ledger.jsonl")).unwrap();
    let mut spine = support::spine(ledger);
    spine.set_machinery_journal(MachineryJournal::new(dir.join("machinery")));
    spine.attach_lease(Box::new(ReplayCognition::new("sess-1", recorded_turn())));
    (spine, MachineryJournal::new(dir.join("machinery")))
}

// ---------------------------------------------------------------------------
// the tests
// ---------------------------------------------------------------------------

#[test]
fn a_turns_machinery_is_retained_keyed_to_its_thread_and_turn() {
    let dir = tmp_dir("retain");
    let (mut spine, journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    let turn = spine.submit_prompt("what version is the engine?", Source::Text).unwrap();

    let rows = journal.project_thread(&thread);
    assert!(!rows.is_empty(), "machinery must be RETAINED, not merely emitted");
    for r in &rows {
        assert_eq!(r.thread_id, thread);
        assert_eq!(r.turn_id.as_deref(), Some(turn.as_str()));
        assert_eq!(r.session_id, "sess-1", "which lease produced it stays reconstructible");
    }
}

#[test]
fn one_tool_call_is_one_row_with_the_real_title_and_a_terminal_status() {
    // FIVE wire frames for toolu_A (open, args, permission, heartbeat, result) collapse to
    // ONE row — §1.4 G2, against the exact shapes measured on the native wire.
    let dir = tmp_dir("merge");
    let (mut spine, journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();

    let rows = journal.project_thread(&thread);
    let tool: Vec<_> = rows.iter().filter(|r| r.kind == MachineryKind::ToolCall).collect();
    assert_eq!(tool.len(), 1, "one tool id is one row, never two");
    // The open frame's title is the tool's REAL name (`Bash`, direct — no `_meta`
    // indirection and no `Terminal` placeholder to correct); the arguments frame replaces it
    // with the human line. Same end state the ACP path reached, by a different route.
    assert_eq!(tool[0].title, "cat engine/VERSION");
    assert_eq!(tool[0].status, Some(ToolStatus::Completed), "Pending -> InProgress -> Completed");
    assert_eq!(tool[0].summary.as_deref(), Some("1.0.0"), "the outcome, from the closing frame");
}

#[test]
fn text_and_machinery_share_one_sequence_so_the_true_order_is_reconstructible() {
    // §1.4 G1, the guarantee everything else rests on: "he said X, then ran Y, then said
    // Z". Two independent counters cannot express this.
    let dir = tmp_dir("seq");
    let (mut spine, journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    let stream = RecordingObserver::default();
    spine.set_observer(Box::new(stream.clone()));
    spine.submit_prompt("go", Source::Text).unwrap();

    let chunk_seqs: Vec<u64> = stream
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Chunk { seq, .. } => Some(*seq),
            _ => None,
        })
        .collect();
    let mach_seqs: Vec<u64> = journal.read_thread(&thread).iter().map(|r| r.seq).collect();

    assert_eq!(chunk_seqs, vec![0, 7], "text takes positions 0 and 7 — the gap IS the tool call");
    assert_eq!(mach_seqs, vec![1, 2, 3, 4, 5, 6]);
    let mut all: Vec<u64> = chunk_seqs.iter().chain(mach_seqs.iter()).copied().collect();
    all.sort_unstable();
    let unique_count = {
        let mut c = all.clone();
        c.dedup();
        c.len()
    };
    assert_eq!(unique_count, all.len(), "no two items ever share a position");
    assert_eq!(all, (0..=7).collect::<Vec<u64>>(), "one dense sequence across both families");
}

#[test]
fn the_dropped_user_text_frame_consumes_no_position_and_leaves_no_record() {
    // §1.2's ONE deliberate drop, ported: the ledger already holds the CEO's words verbatim
    // and fsync'd, and the stop control already records a stop, so neither may become a
    // second source of truth.
    let dir = tmp_dir("drop");
    let (mut spine, journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();
    let all = journal.read_thread(&thread);
    assert!(
        !all.iter().any(|r| r.title.starts_with("user")),
        "the injected user-frame text is never retained"
    );
    // 9 scripted items, 1 dropped, so positions run 0..=7 with nothing skipped.
    assert_eq!(all.len() + 2, 8, "6 machinery + 2 text = 8 delivered items");
}

#[test]
fn an_untyped_vendor_frame_is_retained_as_unknown_rather_than_dropped() {
    // `message_delta` has no typed route. It must still be KEPT — §1.4 G5, and the CEO's
    // whole argument that a dropped byte is a permanent hole.
    let dir = tmp_dir("unknown");
    let (mut spine, journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();
    let unknown: Vec<_> = journal
        .read_thread(&thread)
        .into_iter()
        .filter(|r| r.kind == MachineryKind::Unknown)
        .collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].title, "stream_event:message_delta", "the vendor frame survives");
    assert_eq!(
        unknown[0].payload.as_ref().unwrap()["event"]["usage"]["cache_read_input_tokens"],
        json!(25737),
        "verbatim payload"
    );
}

#[test]
fn the_calm_conversation_is_byte_identical_whether_or_not_machinery_flows() {
    // §3.3's provable-not-promised test (b): the Ledger gains no variant and `messages()`
    // is unchanged. Same script, same prompts; the only difference is whether a journal
    // and a machinery observer are attached at all.
    let with_dir = tmp_dir("calm-with");
    let (mut a, _) = spine_with_journal(&with_dir);
    a.set_machinery_observer(Box::new(RecordingMachineryObserver::default()));
    let ta = a.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    a.submit_prompt("what version is the engine?", Source::Text).unwrap();

    let without_dir = tmp_dir("calm-without");
    let ledger = Ledger::open(without_dir.join("conversation-ledger.jsonl")).unwrap();
    let mut b = support::spine(ledger);
    b.attach_lease(Box::new(ReplayCognition::new("sess-1", recorded_turn())));
    let tb = b.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    b.submit_prompt("what version is the engine?", Source::Text).unwrap();

    let ma: Vec<(String, String)> = a.messages(&ta).unwrap().into_iter().map(|m| (m.role, m.text)).collect();
    let mb: Vec<(String, String)> = b.messages(&tb).unwrap().into_iter().map(|m| (m.role, m.text)).collect();
    assert_eq!(ma, mb, "the CEO's conversation is identical with the journal on and off");
    assert_eq!(ma.last().unwrap().1, "Let me check. It says 1.0.0.");
}

#[test]
fn no_tool_output_ever_reaches_the_conversation_ledger() {
    // The correctness hazard §2.1 names: putting machinery in the same log would leave it
    // one missing filter away from the CEO's conversation. It is a different file.
    let dir = tmp_dir("ledger-clean");
    let (mut spine, _journal) = spine_with_journal(&dir);
    spine.ensure_active_thread_in(&femcboost()).unwrap();
    spine.submit_prompt("go", Source::Text).unwrap();
    drop(spine);

    let ledger_bytes = std::fs::read_to_string(dir.join("conversation-ledger.jsonl")).unwrap();
    for needle in ["tool_use_id", "tool_result", "tool_progress", "message_delta", "cat engine/VERSION"] {
        assert!(!ledger_bytes.contains(needle), "the ledger must not contain {needle}");
    }
    assert!(ledger_bytes.contains("It says 1.0.0."), "but it does hold the reply");
}

#[test]
fn machinery_never_reaches_the_stream_event_family() {
    // §3.3's test (a), expressed structurally: a `StreamEvent` cannot name the machinery
    // event, so a UI subscribed only to the four calm events cannot receive machinery.
    let names = [EVENT_TURN_STARTED, EVENT_CHUNK, EVENT_TURN_COMPLETED, EVENT_TURN_ERROR, EVENT_PROACTIVE_MESSAGE];
    assert!(!names.contains(&EVENT_MACHINERY), "two families, two name spaces");
    assert_eq!(EVENT_MACHINERY, "rich://machinery");

    let dir = tmp_dir("families");
    let (mut spine, _j) = spine_with_journal(&dir);
    let stream = RecordingObserver::default();
    let mach = RecordingMachineryObserver::default();
    spine.set_observer(Box::new(stream.clone()));
    spine.set_machinery_observer(Box::new(mach.clone()));
    spine.ensure_active_thread_in(&femcboost()).unwrap();
    spine.submit_prompt("go", Source::Text).unwrap();

    for e in stream.events.lock().unwrap().iter() {
        assert_ne!(e.event_name(), EVENT_MACHINERY);
        if let StreamEvent::Chunk { text_delta, .. } = e {
            assert!(!text_delta.contains("cat engine"), "no tool text in a chunk, ever");
        }
    }
    assert_eq!(mach.records.lock().unwrap().len(), 6, "and the machinery sink got all six");
}

#[test]
fn reprime_machinery_is_retained_internal_and_never_projected() {
    // §1.5: re-prime machinery is recorded, `internal: true`, `turnId: None`, and NEVER in
    // a thread render — the standing order that Rich never reveals session rotation.
    let dir = tmp_dir("reprime");
    let ledger = Ledger::open(dir.join("conversation-ledger.jsonl")).unwrap();
    let mut spine = support::spine(ledger);
    spine.set_machinery_journal(MachineryJournal::new(dir.join("machinery")));
    let mut lease = ReplayCognition::new("sess-1", recorded_turn());
    lease.reprime_machinery = true;
    spine.attach_lease(Box::new(lease));
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();

    let journal = MachineryJournal::new(dir.join("machinery"));
    let internal: Vec<_> = journal.read_thread(&thread).into_iter().filter(|r| r.internal).collect();
    assert_eq!(internal.len(), 1, "retained for debugging");
    assert_eq!(internal[0].title, "Read reprime payload");
    assert_eq!(internal[0].turn_id, None, "attached to the thread, not to a turn (G4)");

    let rendered = journal.project_thread(&thread);
    assert!(!rendered.iter().any(|r| r.internal), "and never rendered");
    assert!(!rendered.iter().any(|r| r.title == "Read reprime payload"));
}

#[test]
fn a_thread_from_before_this_commit_shows_the_honest_empty_state() {
    // THE retroactivity limit, stated as a test rather than a promise. Retention begins at
    // the routing commit; a thread that ran before it has nothing, and asking for it
    // returns an empty list — not an error, not a panic. The renderer's job is to say
    // "nothing was recorded for this conversation."
    let dir = tmp_dir("empty");
    let journal = MachineryJournal::new(dir.join("machinery"));
    assert!(journal.read_thread("thr_from_yesterday").is_empty());
    assert!(journal.project_thread("thr_from_yesterday").is_empty());
}

#[test]
fn a_journal_write_failure_never_fails_the_turn() {
    // §2.2's corollary. `spine.rs` makes a LEDGER write failure terminal for the turn —
    // correctly, because the ledger is truth. Machinery is not truth. Here the journal
    // root is a FILE, so every create_dir_all under it fails.
    let dir = tmp_dir("wedged");
    let blocked = dir.join("machinery");
    std::fs::write(&blocked, b"not a directory").unwrap();

    let ledger = Ledger::open(dir.join("conversation-ledger.jsonl")).unwrap();
    let mut spine = support::spine(ledger);
    spine.set_machinery_journal(MachineryJournal::new(&blocked));
    spine.attach_lease(Box::new(ReplayCognition::new("sess-1", recorded_turn())));
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();

    let turn = spine.submit_prompt("go", Source::Text).expect("the turn must still succeed");
    assert_eq!(spine.ledger().turn(&turn).unwrap().assistant_text, "Let me check. It says 1.0.0.");
    assert_eq!(spine.messages(&thread).unwrap().len(), 2, "the CEO's conversation is unharmed");
}

#[test]
fn a_spine_with_no_journal_still_routes_and_completes_the_turn() {
    // An honest degrade for headless runs and tests — NOT a supported product state: §3.2
    // says retention runs ALWAYS, because the requirement is to flip a thread the CEO
    // already had. Nothing here silently makes retention optional at runtime.
    let dir = tmp_dir("nojournal");
    let ledger = Ledger::open(dir.join("conversation-ledger.jsonl")).unwrap();
    let mut spine = support::spine(ledger);
    assert!(!spine.has_machinery_journal());
    let mach = RecordingMachineryObserver::default();
    spine.set_machinery_observer(Box::new(mach.clone()));
    spine.attach_lease(Box::new(ReplayCognition::new("sess-1", recorded_turn())));
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();
    assert_eq!(spine.messages(&thread).unwrap().len(), 2);
    assert_eq!(mach.records.lock().unwrap().len(), 6, "routed and emitted, just not retained");
}

#[test]
fn a_permission_request_is_recorded_as_an_observation_with_no_control_attached() {
    // §1.2 / §9: recording the auto-approval is a FACT. Gap #1 stays deferred; there is no
    // approve/deny anything. A window, not a cockpit.
    let dir = tmp_dir("perm");
    let (mut spine, journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();
    let perms: Vec<_> = journal
        .read_thread(&thread)
        .into_iter()
        .filter(|r| r.kind == MachineryKind::PermissionRequested)
        .collect();
    assert_eq!(perms.len(), 1);
    assert!(perms[0].payload.as_ref().unwrap().get("auto").is_none());
    assert_eq!(perms[0].payload.as_ref().unwrap()["chosen"], json!("allow"));
    assert_eq!(perms[0].tool_call_id.as_deref(), Some("toolu_A"), "linked to its tool call");
}

#[test]
fn no_governance_vocabulary_reaches_the_calm_view() {
    // R2 BUSINESS-ACTION GOVERNANCE IS DEFERRED TO V2 BY CEO DECISION, for v1 and all 1.x.
    // Every STRUCTURAL refusal in this family held: `rich://approval-requested` and
    // `-resolved` are not emitted, and `TimelineItem::Approval` is modelled with no
    // constructor. The NOUN walked past all of them — `MachineryKind::PermissionRequested`
    // fell through to `Visibility::Ceo` with the summary "Requested approval", which
    // `app/ui/timeline.js` rolled up as "Requested approval 7 times" (7 measured across
    // five short probe runs). A calm view that asserts a governance act nobody performed is
    // R2 by the back door: it manufactures the demand for the queue that was deferred.
    //
    // Checked as a WORD BAN over the whole serialized CEO view, not as an assertion about
    // one row, because the defect was never in the row that was designed — it was in a
    // string that reached a surface nobody re-read.
    let dir = tmp_dir("governance");
    let (mut spine, _journal) = spine_with_journal(&dir);
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    spine.submit_prompt("go", Source::Text).unwrap();

    let timeline = spine.timeline(&thread).unwrap();
    let ceo = serde_json::to_string(&timeline.view(ViewMode::Ceo)).unwrap().to_lowercase();
    for word in ["approval", "approve", "approved", "permission", "authoriz", "authoris", "denied", "grant"] {
        assert!(!ceo.contains(word), "governance vocabulary on the calm surface: `{word}` in {ceo}");
    }

    // NOT deleted — routed, retained, and visible in technical mode with the honest line.
    let technical = serde_json::to_string(&timeline.view(ViewMode::Technical)).unwrap();
    assert!(
        technical.contains("Answered a permission prompt automatically"),
        "the permission row must still exist as machinery: {technical}"
    );
    // And the words that are wrong are wrong in BOTH views: the client answered, it did not
    // decide, and there is no one to have approved anything.
    assert!(!technical.contains("Requested approval"), "the old string must be gone: {technical}");
    assert!(!technical.contains("Approved"), "no authorization verb anywhere: {technical}");

    // The work the request belonged to is unaffected — nothing was hidden, only reclassified.
    assert!(
        serde_json::to_string(&timeline.view(ViewMode::Ceo)).unwrap().contains("Ran a command"),
        "the tool call this permission belonged to still renders its own semantic row"
    );
}

#[test]
fn machinery_survives_a_restart_and_is_readable_from_a_fresh_process() {
    // Retroactivity is worth nothing if it does not outlive the process that wrote it.
    let dir = tmp_dir("restart");
    let thread = {
        let (mut spine, _j) = spine_with_journal(&dir);
        let t = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
        spine.submit_prompt("go", Source::Text).unwrap();
        t
    };
    let reopened = MachineryJournal::new(dir.join("machinery"));
    let rows = reopened.project_thread(&thread);
    assert!(rows.iter().any(|r| r.title == "cat engine/VERSION"));
    assert!(rows.iter().any(|r| r.kind == MachineryKind::Unknown));
}

/// A lease whose turn also makes the app hook write its evidence, the way the real hook does
/// DURING the turn: before the replayed frames, `during` appends to the evidence folder.
struct HookedCognition {
    inner: ReplayCognition,
    during: Box<dyn FnMut() + Send>,
}

impl Cognition for HookedCognition {
    fn session_id(&self) -> &str {
        self.inner.session_id()
    }

    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        (self.during)();
        self.inner.prompt(text, on_item)
    }
}

fn write_call(id: &str, path: &str, parent: Option<&str>) -> Vec<Item> {
    let parent = json!(parent);
    vec![
        Item::Frame(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":id,"name":"Write","input":{}}},"parent_tool_use_id":parent})),
        Item::Frame(json!({"type":"assistant","message":{"role":"assistant","content":[
            {"type":"tool_use","id":id,"name":"Write","input":{"file_path":path,"content":"x"}}]},
            "parent_tool_use_id":parent})),
        Item::Frame(json!({"type":"user","message":{"role":"user","content":[
            {"tool_use_id":id,"type":"tool_result","content":"File created successfully"}]},
            "tool_use_result":{"type":"create","filePath":path},"parent_tool_use_id":parent})),
    ]
}

/// THE OUTPUT RECORD THROUGH THE REAL SPINE (Output side panel PRD §4.1 (a)/(b), S2): in one
/// turn Rich writes a file himself and a worker he dispatched writes another. Rich's file is
/// recorded live by witness (a), as Rich's, once although the hook saw it too; the worker's
/// `Write` arrives nested on the same wire and is NOT recorded as Rich's; at the end of the turn
/// the hook's row for it is projected under the worker's own name.
#[test]
fn a_turns_written_files_reach_the_output_record_with_whoever_wrote_them() {
    use richos_core::output::{Actor, OutputStore, WriteSource};
    let dir = tmp_dir("output");
    let (mine, theirs) = (dir.join("brief.md"), dir.join("numbers.csv"));
    std::fs::write(&mine, "the brief").unwrap();
    std::fs::write(&theirs, "a,b").unwrap();
    let (mine, theirs) = (mine.to_string_lossy().into_owned(), theirs.to_string_lossy().into_owned());
    let mut script = write_call("toolu_MINE", &mine, None);
    script.extend(write_call("toolu_THEIRS", &theirs, Some("toolu_AGENT")));

    let evidence = dir.join("engine-state/evidence");
    let folder = evidence.join("sess-1");
    let (mine_seen, theirs_seen) = (mine.clone(), theirs.clone());
    let during = Box::new(move || {
        use std::io::Write;
        std::fs::create_dir_all(&folder).unwrap();
        let line = |file: &str, value: Value| {
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(folder.join(file)).unwrap();
            writeln!(f, "{value}").unwrap();
        };
        let now = richos_core::util::now_millis();
        line("callbacks.jsonl", json!({"schema":1,"callback":{"hook_event_name":"PostToolUse","session_id":"sess-1",
            "tool_name":"Agent","tool_use_id":"toolu_AGENT","tool_input":{"name":"norm-sonnet-a","prompt":"count"},
            "tool_response":{"status":"completed","agentId":"agent-7"}}}));
        line("writes.jsonl", json!({"schema":1,"session_id":"sess-1","agent_id":"agent-7","tool_use_id":"toolu_THEIRS",
            "path":theirs_seen,"at":now,"source":"hook"}));
        line("writes.jsonl", json!({"schema":1,"session_id":"sess-1","tool_use_id":"toolu_MINE",
            "path":mine_seen,"at":now,"source":"hook"}));
    });

    let ledger = Ledger::open(dir.join("conversation-ledger.jsonl")).unwrap();
    let mut spine = support::spine(ledger);
    spine.set_machinery_journal(MachineryJournal::new(dir.join("machinery")));
    let store = OutputStore::for_data_dir(&dir);
    spine.set_output_store(store.clone(), Some(evidence));
    spine.attach_lease(Box::new(HookedCognition { inner: ReplayCognition::new("sess-1", script), during }));
    let thread = spine.ensure_active_thread_in(&femcboost()).unwrap().thread_id().to_string();
    let turn = spine.submit_prompt("write the brief and have the numbers counted", Source::Text).unwrap();

    let list = store.project(&thread).unwrap();
    assert_eq!(list.count, 2, "{list:?}");
    let entry = |name: &str| list.files.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name}: {list:?}"));
    let brief = entry("brief.md");
    assert_eq!((brief.actor, brief.source, brief.writes), (Actor::Rich, WriteSource::Tool, 1));
    assert_eq!(brief.turn_id.as_deref(), Some(turn.as_str()));
    let numbers = entry("numbers.csv");
    assert_eq!(numbers.actor, Actor::Worker, "a nested worker's write is never Rich's");
    assert_eq!(numbers.worker_name.as_deref(), Some("norm-sonnet-a"));
    assert_eq!(numbers.turn_id.as_deref(), Some(turn.as_str()));
    let rows = store.read(&thread).unwrap().rows;
    assert!(!rows.iter().any(|r| r.path == theirs && r.actor == Actor::Rich), "{rows:?}");
}
