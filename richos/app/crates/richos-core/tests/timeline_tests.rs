//! THE TYPED TIMELINE — scope containment, the shared sequence, the visibility gate, and
//! restart.
//!
//! Slice 2 of the Codex-inspired conversation UX brief §24: *"core: add typed timeline and
//! worker lifecycle events"*. These tests pin the four properties the brief makes
//! load-bearing and one it forbids:
//!
//!   - **no machinery from one entity may ever render in another entity's thread** (§22
//!     "must not be faked: cross-entity context", §25 Integrity) — a negative control with
//!     a positive probe, in the same shape as
//!     `entity_binding_tests::no_event_from_one_entity_renders_in_another_entitys_thread`;
//!   - **one shared per-turn sequence** across assistant text, machinery AND the new
//!     timeline records (techy-mode §1.4 G1) — dense, unique, no second counter, live and
//!     after a restart;
//!   - **visibility is a gate**: no internal item and no raw command can reach a CEO view;
//!   - **restart neither loses nor invents** (§23 Phase 1 exit gate);
//!   - **no worker is ever invented** (§22) — including from the one tool call a worker
//!     could plausibly be inferred from.

use richos_core::cognition::{Cognition, CognitionError, TurnItem};
use richos_core::entity::EntityId;
use richos_core::journal::MachineryJournal;
use richos_core::ledger::{Ledger, Source};
use richos_core::machinery::{MachineryObserver, MachineryRecord};
use richos_core::spine::{Spine, WorkerEventsSource};
use richos_core::stream::{StreamEvent, TurnObserver};
use richos_core::timeline::{
    ActivityState, ActivityType, RejectionReason, RichMessagePhase, Timeline, TimelineItem, TimelineSlot, ViewMode,
    Visibility, WorkerState,
};
use richos_core::worker_events::{ObservedWorkerState, WorkerEventRow};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "richos-timeline-{tag}-{}-{}{ext}",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_dir_all(&p);
    p
}

// ---------------------------------------------------------------------------
// A LEASE THAT INTERLEAVES TEXT AND MACHINERY
// ---------------------------------------------------------------------------

/// One step of a scripted turn.
enum Step {
    Text(&'static str),
    /// A raw native stdout frame, verbatim in the shape measured on 2026-08-31.
    Frame(Value),
}

/// A `Cognition` that emits text AND machinery in one interleaved stream, assigning `seq`
/// exactly where the real client assigns it — once, at the drain point, shared by both
/// families (`native.rs`'s `prompt` drain loop, §1.4 G1). `MockCognition` only emits text, so it cannot
/// exercise the interleaving these tests exist to prove.
struct ScriptedLease {
    session_id: String,
    script: Vec<Step>,
}

impl Cognition for ScriptedLease {
    fn session_id(&self) -> &str {
        &self.session_id
    }



    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let mut seq = 0u64;
        for step in &self.script {
            match step {
                Step::Text(t) => {
                    on_item(TurnItem::Text { seq, text: t });
                    seq += 1;
                }
                Step::Frame(f) => {
                    for r in MachineryRecord::from_native_event(f, &self.session_id, seq) {
                        seq += 1;
                        on_item(TurnItem::Machinery(r));
                    }
                }
            }
        }
        Ok("end_turn".to_string())
    }
}

/// Records what the UI actually saw, live, in both event families.
#[derive(Clone, Default)]
struct Live {
    text: Arc<Mutex<Vec<u64>>>,
    machinery: Arc<Mutex<Vec<u64>>>,
}
impl TurnObserver for Live {
    fn on_event(&self, event: &StreamEvent) {
        if let StreamEvent::Chunk { seq, .. } = event {
            self.text.lock().unwrap().push(*seq);
        }
    }
}
impl MachineryObserver for Live {
    fn on_machinery(&self, r: &MachineryRecord) {
        if r.turn_id.is_some() && !r.internal {
            self.machinery.lock().unwrap().push(r.seq);
        }
    }
}

/// "he said X, then ran Y, then said Z" — the sentence §1.4 G1 exists to make
/// reconstructible, as an actual turn: text, tool call, text, tool result.
fn interleaved_script() -> Vec<Step> {
    vec![
        Step::Text("Reading the release notes"),
        // seq 1 — the OPEN frame, verbatim from `raw/run9-rust-driven.jsonl:5`.
        Step::Frame(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_A","name":"Bash","input":{}}}})),
        Step::Text(" — the build is green"),
        // seq 3 — the CLOSE frame: an outcome and NO tool name. The measured shape.
        Step::Frame(json!({"type":"user","message":{"role":"user","content":[
            {"tool_use_id":"toolu_A","type":"tool_result","content":"1.0.0"}]}})),
    ]
}

/// A spine wired with a journal, an interleaving lease and a live recorder.
fn spine_with(script: Vec<Step>, ledger_path: &std::path::Path, journal_root: &std::path::Path) -> (Spine, Live, ScriptedLease) {
    let mut spine = support::spine(Ledger::open(ledger_path).unwrap());
    spine.set_machinery_journal(MachineryJournal::new(journal_root));
    let live = Live::default();
    spine.set_observer(Box::new(live.clone()));
    spine.set_machinery_observer(Box::new(live.clone()));
    (spine, live, ScriptedLease { session_id: "sess-1".into(), script })
}

// ---------------------------------------------------------------------------
// THE INTEGRITY PROPERTY
// ---------------------------------------------------------------------------

#[test]
fn no_machinery_from_one_entity_renders_in_another_entitys_thread() {
    // Slice 1 closed this leak for ledger TURNS. Machinery is the omission that would
    // re-open it: `MachineryRecord` carries a `thread_id` and NO `entity_id`, so a record
    // attached to a forged cross-entity turn has nothing on its own face to reject it by.
    //
    // The fixture is the slice-1 forgery — a turn filed under the femcboost thread while
    // claiming the deeply entity — with real machinery hanging off it, plus one record
    // that belongs to the deeply thread outright.
    let path = tmp("leak", ".jsonl");
    let secret = "deeply's Q4 term sheet numbers";
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_fem","title":"Avelor release","at":1,"entity_id":"femcboost","person_id":"ceo-default","binding_revision":1}"#,
            "\n",
            r#"{"event":"ThreadCreated","thread_id":"thr_dee","title":"Partner book","at":2,"entity_id":"deeply","person_id":"ceo-default","binding_revision":2}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_ok","thread_id":"thr_fem","text":"how is the release","source":"text","at":3,"entity_id":"femcboost","binding_revision":1}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_ok","session_id":"s1","at":4}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_ok","text":"on track","at":5,"seq":0}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_ok","stop_reason":"end_turn","at":6}"#,
            "\n",
            // THE FORGERY: thread_id says femcboost's thread, entity_id says deeply.
            r#"{"event":"PromptReceived","turn_id":"turn_leak","thread_id":"thr_fem","text":"deeply's Q4 term sheet numbers","source":"text","at":7,"entity_id":"deeply","binding_revision":2}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_leak","stop_reason":"end_turn","at":8}"#,
            "\n",
        ),
    )
    .unwrap();

    let journal_root = tmp("leak-journal", "");
    let journal = MachineryJournal::new(&journal_root);
    // One tool row from one native frame, for a fixture that needs exactly one.
    let mk = |frame: Value, thread: &str, turn: &str, seq: u64| {
        let mut v = MachineryRecord::from_native_event(&frame, "sess", seq);
        assert_eq!(v.len(), 1);
        v.remove(0).stamp(thread, Some(turn), false)
    };
    // A completed tool row whose TITLE is the thing that must not leak.
    let done = |id: &str, title: &str| {
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":id,"name":title,"input":{}}}})
    };
    // Legitimate femcboost machinery: a classified tool (`Bash`), so its row is in his view.
    journal
        .append(&mk(
            done("t_ok", "Bash"),
            "thr_fem",
            "turn_ok",
            1,
        ))
        .unwrap();
    // The forged turn's machinery — filed under the femcboost THREAD, carrying deeply's
    // secret in its title and its touched path.
    journal
        .append(&mk(
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"tool_use","id":"t_leak",
                                    "name":"cat deeply's Q4 term sheet numbers",
                                    "input":{"file_path":"/Users/alex/ab/deeply/q4-term-sheet.md"}}}}),
            "thr_fem",
            "turn_leak",
            0,
        ))
        .unwrap();
    // The forged turn again, this time sharing a toolCallId with the LEGITIMATE call
    // above. `machinery::project` merges by toolCallId across everything it is handed
    // (§1.4 G2, last-write-wins per present field), so if a quarantined turn's records
    // reach the merge, this one does not need a row of its own — it overwrites the title
    // of a row femcboost is entitled to see.
    journal
        .append(&mk(
            done("t_ok", "deeply's Q4 term sheet numbers"),
            "thr_fem",
            "turn_leak",
            1,
        ))
        .unwrap();
    // ...and the nastier one: a record stamped with the DEEPLY thread but carrying a
    // femcboost TURN id. A mis-stamp, a corrupt shard, a replayed stale active context —
    // and the only thing standing between it and femcboost's screen is the thread clause,
    // because its turn id is one this thread legitimately accepts.
    journal
        .append(&mk(
            done("t_dee", "deeply's Q4 term sheet numbers"),
            "thr_dee",
            "turn_ok",
            2,
        ))
        .unwrap();

    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();

    // (1) POSITIVE PROBE — the input really does contain foreign content, filed under the
    //     femcboost thread. Without this the assertion below could pass because nothing
    //     foreign was ever there (the negative test that passes for the wrong reason).
    let mut records = journal.read_thread("thr_fem");
    records.extend(journal.read_thread("thr_dee"));
    let raw_text: String = records.iter().map(|r| format!("{} {}", r.title, r.locations.join(","))).collect();
    assert!(raw_text.contains(secret), "the fixture must actually cross the boundary, or this test proves nothing");
    assert!(
        records.iter().any(|r| r.turn_id.as_deref() == Some("turn_leak") && r.thread_id == "thr_fem"),
        "the forged turn's machinery really is filed under the femcboost thread"
    );
    assert!(
        records.iter().any(|r| r.thread_id == "thr_dee" && r.turn_id.as_deref() == Some("turn_ok")),
        "and one record names the other thread while claiming a turn THIS thread accepts"
    );

    // (2) THE GUARD, and what each clause is for.
    //
    //     Delete the `r.thread_id != thread_id` clause in `Timeline::project` and the
    //     third record above — deeply's thread, femcboost's turn id — is placed into
    //     turn_ok's bucket and RENDERS: this assertion fails with deeply's term-sheet line
    //     inside a femcboost thread.
    //
    //     Delete the `rank.contains_key(turn_id)` clause and the quarantined turn's own
    //     row still cannot be placed (`rank` is the placement key, so there is no bucket
    //     for it) — but the fourth record above never needed a row: it shares a toolCallId
    //     with a legitimate call, so the merge folds its title into a row femcboost IS
    //     entitled to see, and this assertion fails with the secret rendered under
    //     turn_ok. Keeping foreign records out of the MERGE INPUT is what that clause is
    //     for; the placement key alone is not enough.
    let timeline = Timeline::project(&ledger, &binding, &records).unwrap();
    let rendered = format!("{:?}", timeline.audit_including_internal());
    assert!(!rendered.contains(secret), "CROSS-ENTITY LEAK in the timeline:\n{rendered}");
    assert!(!rendered.contains("q4-term-sheet"), "...including the touched path");
    for item in timeline.audit_including_internal() {
        assert_eq!(item.entity_id(), &femcboost(), "every item carries — and matches — the thread's entity");
        assert_ne!(item.turn_id(), "turn_leak", "the quarantined turn contributes nothing");
    }

    // (3) ...and it is REPORTED, not silently swallowed. Both clauses fired, each once.
    let violations = timeline.scope_violations();
    assert_eq!(violations.len(), 3, "three rejections, reported not hidden: {violations:?}");
    assert_eq!(
        violations.iter().filter(|v| v.reason == RejectionReason::UnscopedTurn).count(),
        2,
        "both of the forged turn's records are refused, including the one that would have \
         merged into a legitimate row: {violations:?}"
    );
    assert!(
        violations.iter().any(|v| v.reason == RejectionReason::ForeignThread && v.thread_id == "thr_dee"),
        "the other thread's machinery is refused: {violations:?}"
    );

    // (4) The legitimate femcboost activity still renders — the guard excludes, it does
    //     not blank the lane.
    let ceo = timeline.view(ViewMode::Ceo);
    assert!(
        ceo.items().iter().any(|i| matches!(i, TimelineItem::Activity { .. })),
        "femcboost's own tool call is still there"
    );

    // (5) The deeply thread is untouched by any of this.
    let dee_binding = ledger.thread_binding("thr_dee").unwrap();
    let dee = Timeline::project(&ledger, &dee_binding, &journal.read_thread("thr_dee")).unwrap();
    assert!(dee.audit_including_internal().is_empty(), "no turns there, so no items — and no femcboost content");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

// ---------------------------------------------------------------------------
// G1 — ONE SHARED PER-TURN SEQUENCE
// ---------------------------------------------------------------------------

#[test]
fn text_machinery_and_timeline_records_share_one_dense_per_turn_sequence() {
    let path = tmp("g1", ".jsonl");
    let journal_root = tmp("g1-journal", "");
    let (mut spine, live, lease) = spine_with(interleaved_script(), &path, &journal_root);
    let thread = spine.create_thread("Avelor release", &femcboost()).unwrap();
    spine.attach_lease(Box::new(lease));
    let turn_id = spine.submit_prompt("how is the release", Source::Text).unwrap();

    // (1) THE COUNTER ITSELF IS DENSE AND UNIQUE. Four items left the lease and the two
    //     families between them consumed positions 0,1,2,3 — every position used exactly
    //     once, no gaps, no collisions. Neither family is dense alone: text has a hole at
    //     1 and 3, machinery at 0 and 2. That is the point of a shared counter.
    let text_live = live.text.lock().unwrap().clone();
    let mach_live = live.machinery.lock().unwrap().clone();
    assert_eq!(text_live, vec![0, 2]);
    assert_eq!(mach_live, vec![1, 3]);
    let mut all: Vec<u64> = text_live.iter().chain(mach_live.iter()).copied().collect();
    all.sort_unstable();
    let mut unique = all.clone();
    unique.dedup();
    assert_eq!(all, vec![0, 1, 2, 3], "the shared counter is dense across the two families");
    assert_eq!(unique, all, "and no position is claimed twice");

    let timeline = spine.timeline(&thread).unwrap();
    let turn_items: Vec<&TimelineItem> =
        timeline.audit_including_internal().iter().filter(|i| i.turn_id() == turn_id).collect();

    // (2) THE TIMELINE READS THAT COUNTER; IT DOES NOT OPEN ONE.
    //
    //     Its stream items sit at 0, 1, 2 — three items for four wire positions, and the
    //     missing one is NOT a lost item. Position 3 is the `tool_call_update` that closed
    //     the call opened at position 1, and §1.4 G2's merge folds it into that row:
    //     "four wire events, ONE row", positioned where the call STARTED. So the timeline
    //     sequence set is a strictly increasing SUBSET of the shared counter, never a
    //     renumbering of it — the check below pins exactly that, and would fail just as
    //     loudly for a second counter (which would restart at 0) as for a dropped item.
    let seqs: Vec<u64> = turn_items
        .iter()
        .filter(|i| i.base().slot == TimelineSlot::Stream)
        .map(|i| i.sequence().expect("a stream item written by this build always has a position"))
        .collect();
    assert_eq!(seqs, vec![0, 1, 2], "positions read straight off the shared counter, in order");
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "strictly increasing, and never re-sorted by the clock");
    assert!(seqs.iter().all(|s| all.contains(s)), "every timeline position is a position the counter actually issued");

    // ...and position 3 is accounted for rather than dropped: the row at 1 carries the
    // later event's outcome, which is what the merge means.
    let merged = turn_items
        .iter()
        .find_map(|i| match i {
            TimelineItem::Activity { base, state, .. } if base.sequence == Some(1) => Some(*state),
            _ => None,
        })
        .expect("the tool call is on the timeline at the position it started");
    assert_eq!(merged, ActivityState::Completed, "the seq-3 update merged into the seq-1 row (§1.4 G2)");

    // NO SECOND COUNTER: the timeline's positions ARE the live ones, not a renumbering.
    let text_seqs: Vec<u64> = turn_items
        .iter()
        .filter(|i| matches!(i, TimelineItem::RichMessage { .. }))
        .map(|i| i.sequence().unwrap())
        .collect();
    let mach_seqs: Vec<u64> = turn_items
        .iter()
        .filter(|i| matches!(i, TimelineItem::Activity { .. }))
        .map(|i| i.sequence().unwrap())
        .collect();
    assert_eq!(text_seqs, text_live, "the message items sit exactly where the live chunks sat");
    assert_eq!(mach_seqs, vec![1], "the two tool events merged into ONE row, positioned where the call STARTED");

    // THE SENTENCE G1 EXISTS FOR: "he said X, then ran Y, then said Z" reads back in order.
    let narrative: Vec<String> = turn_items
        .iter()
        .filter(|i| i.base().slot == TimelineSlot::Stream)
        .map(|i| match i {
            TimelineItem::RichMessage { text, .. } => format!("said {text:?}"),
            TimelineItem::Activity { summary, .. } => format!("did {summary:?}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        narrative,
        vec![
            r#"said "Reading the release notes""#.to_string(),
            r#"did "Ran a command""#.to_string(),
            r#"said " — the build is green""#.to_string(),
        ]
    );

    // The ledger's own runs agree: prose split at the gap the tool call made.
    let turn = spine.ledger().turn(&turn_id).unwrap();
    assert_eq!(turn.text_runs.len(), 2);
    assert_eq!(turn.text_runs[0].start_seq, Some(0));
    assert_eq!(turn.text_runs[1].start_seq, Some(2));
    assert_eq!(turn.assistant_text, "Reading the release notes — the build is green");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

// ---------------------------------------------------------------------------
// RESTART (§23 Phase 1 exit gate)
// ---------------------------------------------------------------------------

#[test]
fn a_timeline_survives_a_cold_reopen_neither_losing_nor_inventing_anything() {
    let path = tmp("restart", ".jsonl");
    let journal_root = tmp("restart-journal", "");
    let (before, thread);
    {
        let (mut spine, _live, lease) = spine_with(interleaved_script(), &path, &journal_root);
        thread = spine.create_thread("Avelor release", &femcboost()).unwrap();
        spine.attach_lease(Box::new(lease));
        spine.submit_prompt("how is the release", Source::Text).unwrap();
        before = spine.timeline(&thread).unwrap();
    }

    // KILL AND RESTART: a cold `Ledger::open` replays the log, a cold journal re-reads the
    // shards. No process state survives.
    let mut fresh = support::spine(Ledger::open(&path).unwrap());
    fresh.set_machinery_journal(MachineryJournal::new(&journal_root));
    let after = fresh.timeline(&thread).unwrap();

    // NOT LOST and NOT INVENTED — identical, item for item, id for id.
    assert_eq!(
        after, before,
        "a re-projection of the same durable inputs must be the same timeline, not a similar one"
    );
    // Ids are DERIVED, so they are stable across the restart rather than regenerated.
    let ids_before: Vec<&str> = before.audit_including_internal().iter().map(|i| i.id()).collect();
    let ids_after: Vec<&str> = after.audit_including_internal().iter().map(|i| i.id()).collect();
    assert_eq!(ids_after, ids_before);

    // The interleaving specifically — the thing that was NOT durable before this slice.
    let seqs: Vec<Option<u64>> = after
        .audit_including_internal()
        .iter()
        .filter(|i| i.base().slot == TimelineSlot::Stream && i.visibility() != Visibility::Internal)
        .map(|i| i.sequence())
        .collect();
    assert_eq!(seqs, vec![Some(0), Some(1), Some(2)], "text/tool/text, positions intact after a cold reopen");

    // Nothing was conjured: no worker, no plan, no approval card, no question, no artifact.
    for item in after.audit_including_internal() {
        assert!(
            !matches!(
                item,
                TimelineItem::Worker { .. }
                    | TimelineItem::Plan { .. }
                    | TimelineItem::Approval { .. }
                    | TimelineItem::Question { .. }
                    | TimelineItem::Artifact { .. }
            ),
            "restart invented an unsourced item: {item:?}"
        );
    }

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

// ---------------------------------------------------------------------------
// THE VISIBILITY GATE
// ---------------------------------------------------------------------------

#[test]
fn a_ceo_view_carries_no_internal_item_and_no_raw_command() {
    let path = tmp("gate", ".jsonl");
    let journal_root = tmp("gate-journal", "");
    let script = vec![
        Step::Text("Checking"),
        Step::Frame(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_S","name":"Bash","input":{}}}})),
        Step::Frame(json!({"type":"assistant","message":{"role":"assistant","content":[
            {"type":"tool_use","id":"toolu_S","name":"Bash",
             "input":{"command":"cat /Users/alex/.ssh/config"}}]}})),
        Step::Frame(json!({"type":"user","message":{"role":"user","content":[
            {"tool_use_id":"toolu_S","type":"tool_result","content":"Host prod\n  User root"}]}})),
        // Model reasoning — §5.3's "do not render: model reasoning text".
        Step::Frame(json!({"type":"assistant","message":{"role":"assistant","content":[
            {"type":"thinking","thinking":"maybe I should check the key","signature":"sig"}]}})),
    ];
    // Preserve a real historical internal row without asking the model a hidden question.
    let mut kept=Ledger::open(&path).unwrap();
    let thread=kept.create_thread("Avelor release",&femcboost()).unwrap();
    let binding=kept.thread_binding(&thread).unwrap();
    let internal=kept.record_prompt_received(&binding,"[historical internal]",Source::Internal).unwrap();
    kept.mark_turn_started(&internal,"historical-native").unwrap();
    kept.complete_turn(&internal,"end_turn").unwrap();
    drop(kept);
    let (mut spine, _live, lease) = spine_with(script, &path, &journal_root);
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(lease));
    spine.submit_prompt("check the deploy", Source::Text).unwrap();

    let timeline = spine.timeline(&thread).unwrap();
    let audit = format!("{:?}", timeline.audit_including_internal());
    // POSITIVE PROBE: the sensitive strings really are in the timeline's audit view, so
    // the assertions below are about a gate that has something to stop.
    assert!(audit.contains("cat /Users/alex/.ssh/config"), "the exact command IS retained (technical mode needs it)");
    assert!(audit.contains("maybe I should check the key"), "the reasoning text IS retained");
    assert!(audit.contains("[historical internal]"), "the historical internal turn IS retained");

    // THE CEO VIEW: nothing internal, and the technical detail is GONE — not flagged,
    // removed. A renderer cannot show a command it was never handed.
    let ceo = timeline.view(ViewMode::Ceo);
    let payload = ceo.payload().to_string();
    assert!(!payload.contains("cat /Users/alex/.ssh/config"), "raw command leaked into the CEO payload:\n{payload}");
    assert!(!payload.contains("Host prod"), "raw output leaked into the CEO payload");
    assert!(!payload.contains("maybe I should check the key"), "model reasoning leaked into the CEO payload");
    assert!(!payload.contains("[historical internal]"), "historical internal traffic leaked into the CEO payload");
    for item in ceo.items() {
        assert_eq!(item.visibility(), Visibility::Ceo, "a CEO view contains only CEO items: {item:?}");
        if let TimelineItem::Activity { detail, summary, .. } = item {
            assert!(detail.is_none(), "technical detail must be removed, not merely marked");
            assert_eq!(summary, "Ran a command", "§5.3: the CEO default is semantic");
        }
    }
    // ...and the CEO still sees the work happen. Silence is not privacy.
    assert!(ceo.items().iter().any(|i| matches!(i, TimelineItem::Activity { .. })));

    // TECHNICAL MODE: the same items, with the detail restored — and still no internal
    // item, because `Internal` renders in no mode at all.
    let tech = timeline.view(ViewMode::Technical);
    let tech_payload = tech.payload().to_string();
    assert!(tech_payload.contains("cat /Users/alex/.ssh/config"), "technical mode is where exact commands live");
    assert!(!tech_payload.contains("maybe I should check the key"), "reasoning text renders in NO mode");
    assert!(!tech_payload.contains("[re-prime]"), "re-prime traffic renders in NO mode");
    for item in tech.items() {
        assert_ne!(item.visibility(), Visibility::Internal);
    }

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

#[test]
fn between_turn_machinery_has_no_render_path_and_is_not_a_violation() {
    // Re-prime and rotation traffic is journaled with `turn_id: None` AND `internal: true`
    // (§1.5 G4) — a first-class state, not a bug. Those records are EXCLUDED at the guard,
    // before an item can exist; and the exclusion is classified honestly as "not
    // turn-scoped" rather than reported as a cross-entity violation.
    //
    // NARROWED 2026-08-30: `turn_id: None` alone is no longer the refusal. Between-turn
    // traffic carries the same absent turn and now has a home (`Timeline::between_turns`,
    // §1.5 gap #1), so what this reason means is the INTERNAL half — the machinery the
    // standing order forbids rendering. The fixture below is internal, which is why this
    // test reads the same as it did. See `between_turn_thread_tests.rs` for the other half.
    let path = tmp("unturned", ".jsonl");
    let journal_root = tmp("unturned-journal", "");
    let (mut spine, _live, lease) = spine_with(interleaved_script(), &path, &journal_root);
    let thread = spine.create_thread("Avelor release", &femcboost()).unwrap();
    spine.attach_lease(Box::new(lease));
    spine.submit_prompt("how is the release", Source::Text).unwrap();

    let journal = MachineryJournal::new(&journal_root);
    let unturned = MachineryRecord::from_native_event(
        &json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                "content_block":{"type":"tool_use","id":"t_rp","name":"rotate the lease","input":{}}}}),
        "sess",
        0,
    )
    .remove(0)
    .stamp(&thread, None, true);
    journal.append(&unturned).unwrap();

    let timeline = spine.timeline(&thread).unwrap();
    assert!(timeline.scope_violations().is_empty(), "an unturned record is not a leak");
    let rejections = timeline.rejections();
    assert_eq!(rejections.len(), 1);
    assert_eq!(rejections[0].reason, RejectionReason::NotTurnScoped);
    assert!(!format!("{:?}", timeline.audit_including_internal()).contains("rotate the lease"));

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

// ---------------------------------------------------------------------------
// MUST NOT BE FAKED
// ---------------------------------------------------------------------------

#[test]
fn a_delegated_task_never_becomes_a_worker_and_a_status_less_call_never_becomes_completed() {
    // §22's "must not be faked" list, as a projection over a real turn: active worker
    // count, worker waiting state, completion state.
    let path = tmp("nofake", ".jsonl");
    let journal_root = tmp("nofake-journal", "");
    let script = vec![
        // The vendor's delegated-work tool — the ONE place a worker could be inferred.
        Step::Frame(json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_T","name":"Task","input":{}}}})),
        // ...and a tool call that never reaches a terminal position at all. On this wire
        // that is a call whose `tool_result` never arrives — a tool still running when the
        // turn ended, which is exactly the shape a mid-turn crash leaves behind.
        Step::Frame(json!({"type":"assistant","message":{"role":"assistant","content":[
            {"type":"tool_use","id":"toolu_Q","name":"Read","input":{"file_path":"/tmp/x"}}]}})),
    ];
    let (mut spine, _live, lease) = spine_with(script, &path, &journal_root);
    let thread = spine.create_thread("Avelor release", &femcboost()).unwrap();
    spine.attach_lease(Box::new(lease));
    spine.submit_prompt("delegate the audit", Source::Text).unwrap();

    let timeline = spine.timeline(&thread).unwrap();
    let activities: Vec<(&ActivityType, &ActivityState)> = timeline
        .audit_including_internal()
        .iter()
        .filter_map(|i| match i {
            TimelineItem::Activity { activity_type, state, .. } => Some((activity_type, state)),
            _ => None,
        })
        .collect();

    // CHANGED DELIBERATELY (slice 2a pinned this as an absolute). `spine.timeline` calls
    // `Timeline::project`, which supplies NO worker stream — and this payload carries no
    // async-launch acknowledgement anyway — so there is no identity to join by and the Task
    // call is still just an activity that happened. The joined case is proven in
    // `a_task_call_joined_by_identity_becomes_a_worker_item`.
    assert!(
        !timeline
            .audit_including_internal()
            .iter()
            .any(|i| matches!(i, TimelineItem::Worker { .. } | TimelineItem::WorkerActivity { .. })),
        "with no lifecycle signal to join to, a Task call is an activity, not a worker claim"
    );
    assert!(
        activities.contains(&(&ActivityType::Other, &ActivityState::Queued)),
        "Task projects as an unclassified activity: {activities:?}"
    );
    assert!(
        activities.contains(&(&ActivityType::Read, &ActivityState::Unknown)),
        "no status on the wire ⇒ unknown state, never completed: {activities:?}"
    );
    assert!(
        !activities.iter().any(|(_, s)| **s == ActivityState::Completed),
        "nothing here reported completion, so nothing may claim it: {activities:?}"
    );

    // And no CEO turn's prose is labelled `final`, because no signal says which run is.
    let phases: Vec<RichMessagePhase> = timeline
        .audit_including_internal()
        .iter()
        .filter_map(|i| match i {
            TimelineItem::RichMessage { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect();
    assert!(!phases.contains(&RichMessagePhase::Final), "phase: {phases:?}");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

#[test]
fn an_unbound_legacy_thread_refuses_to_produce_a_timeline_at_all() {
    // Fail closed, same as `Ledger::messages`: "I will not serve this" and "there is
    // nothing here" are different statements, and a renderer must not show the second
    // when the first is true.
    let path = tmp("unbound", ".jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_old","title":"Running","at":1}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_old","thread_id":"thr_old","text":"an old conversation","source":"text","at":2}"#,
            "\n",
        ),
    )
    .unwrap();
    let spine = support::spine(Ledger::open(&path).unwrap());
    let err = spine.timeline("thr_old").unwrap_err();
    assert!(err.to_string().contains("will not guess"), "the refusal explains itself: {err}");
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// WORKERS — the join, and the leak class it introduces
// ---------------------------------------------------------------------------

fn wrow(json_text: &str) -> WorkerEventRow {
    serde_json::from_str(json_text).unwrap()
}

/// A ledger with one femcboost thread and one deeply thread, plus one completed femcboost
/// turn. The shared fixture for the two worker tests below.
fn two_entity_ledger(path: &std::path::Path) {
    std::fs::write(
        path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_fem","title":"Avelor release","at":1,"entity_id":"femcboost","person_id":"ceo-default","binding_revision":1}"#,
            "\n",
            r#"{"event":"ThreadCreated","thread_id":"thr_dee","title":"Partner book","at":2,"entity_id":"deeply","person_id":"ceo-default","binding_revision":2}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_ok","thread_id":"thr_fem","text":"delegate the audit","source":"text","at":3,"entity_id":"femcboost","binding_revision":1}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_ok","session_id":"s1","at":4}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_ok","text":"delegating now","at":5,"seq":0}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_ok","stop_reason":"end_turn","at":6}"#,
            "\n",
        ),
    )
    .unwrap();
}

/// The Task tool call that spawned `agt_shared`, in session `s1`, in femcboost's thread.
///
/// **TWO records where the ACP fixture was one**, and that is the wire, not a preference:
/// the tool NAME arrives when the call opens and the harness's async-launch acknowledgement
/// when it closes, and a `tool_result` carries no name at all. The identity witness
/// therefore spans both, which is why `resolve_agent_ids` resolves names in a first pass.
/// A fixture that kept them in one record would have tested a frame the binary never emits.
fn task_records(thread: &str, turn: &str, session: &str) -> Vec<MachineryRecord> {
    let open = json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
        "content_block":{"type":"tool_use","id":"toolu_T","name":"Task","input":{}}}});
    let ack = json!({"type":"user","message":{"role":"user","content":[
        {"tool_use_id":"toolu_T","type":"tool_result",
         "content":"Async agent launched successfully. agentId: agt_shared"}]}});
    let mut out = Vec::new();
    for (seq, f) in [open, ack].iter().enumerate() {
        for r in MachineryRecord::from_native_event(f, session, seq as u64 + 1) {
            out.push(r.stamp(thread, Some(turn), false));
        }
    }
    out
}

#[test]
fn a_task_call_joined_by_identity_becomes_a_worker_item() {
    // The positive half of the changed invariant. Same Task call as slice 2a's test, but
    // now carrying the harness's async-launch acknowledgement and joined to a real stream.
    let path = tmp("wjoin", ".jsonl");
    two_entity_ledger(&path);
    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();

    let rows = vec![
        wrow(r#"{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"created","agent_id":"agt_shared","worker_name":"sage-opus-r3","agent_type":"sage","session_id":"s1"}"#),
        wrow(r#"{"timestamp":"2026-08-29T04:00:01+00:00","lifecycle_state":"started","agent_id":"agt_shared","agent_type":"sage","session_id":"s1"}"#),
    ];

    let timeline =
        Timeline::project_with_workers(&ledger, &binding, &task_records("thr_fem", "turn_ok", "s1"), &rows).unwrap();

    let workers: Vec<_> = timeline
        .audit_including_internal()
        .iter()
        .filter_map(|i| match i {
            TimelineItem::WorkerActivity { base, worker, .. } => Some((base, worker)),
            _ => None,
        })
        .collect();
    assert_eq!(workers.len(), 1, "the Task call joined by agent_id is a worker row");
    let (base, worker) = workers[0];
    assert_eq!(worker.agent_id, "agt_shared");
    assert_eq!(worker.worker_name.as_deref(), Some("sage-opus-r3"));
    assert_eq!(worker.observed_state, ObservedWorkerState::Started);
    assert_eq!(worker.state, WorkerState::Running);

    // THE FULL ECS FENCE, on the new item type as on every other one.
    assert_eq!(base.entity_id, femcboost());
    assert_eq!(base.thread_id, "thr_fem");
    assert_eq!(base.turn_id, "turn_ok");
    assert_eq!(base.binding_revision, 1);
    assert_eq!(base.sequence, Some(1), "one shared per-turn counter, not a second one");
    assert_eq!(base.slot, TimelineSlot::Stream);

    // And it is NOT the modelled, unsourced §12 variant.
    assert!(!timeline.audit_including_internal().iter().any(|i| matches!(i, TimelineItem::Worker { .. })));

    // Visibility is a GATE, not a field: technical detail is REMOVED from a CEO view.
    let ceo = timeline.view(ViewMode::Ceo);
    let ceo_worker = ceo
        .items()
        .iter()
        .find_map(|i| match i {
            TimelineItem::WorkerActivity { detail, worker, .. } => Some((detail, worker)),
            _ => None,
        })
        .expect("a worker row is CEO-visible");
    assert!(ceo_worker.0.is_none(), "the technical half is removed, not flagged");
    assert_eq!(ceo_worker.1.worker_name.as_deref(), Some("sage-opus-r3"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn no_worker_row_from_another_session_attaches_to_this_sessions_task_call() {
    // THE NEGATIVE CONTROL FOR THE LEAK CLASS THIS SLICE INTRODUCES.
    //
    // `agent_id` is the join key and it is NOT globally unique across sessions — the
    // engine's own test residue at ~/.claude/worker-events.jsonl reuses a single id
    // (aTESTWORKER00001) across twelve rows. So a row from another session can collide
    // with this session's Task call by identity alone.
    //
    // If it attaches, its `worker_name` and its authored `summary` are rendered on a row
    // stamped with THIS binding's entity, thread, turn and revision. It looks perfectly
    // scoped and is not — the same shape slice 2a found in the toolCallId merge, where
    // stamping the entity from the binding was what made the leak invisible.
    let path = tmp("wleak", ".jsonl");
    let secret = "deeply's Q4 term sheet numbers";
    two_entity_ledger(&path);
    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();

    let rows = vec![
        wrow(r#"{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"created","agent_id":"agt_shared","worker_name":"sage-opus-r3","agent_type":"sage","session_id":"s1"}"#),
        wrow(r#"{"timestamp":"2026-08-29T04:00:01+00:00","lifecycle_state":"started","agent_id":"agt_shared","agent_type":"sage","session_id":"s1"}"#),
        // THE COLLISION. Same agent_id, DIFFERENT session, carrying deeply's content — and
        // placed LAST so that without the session clause it would win every field:
        // `observed_state` (it is the last row), `worker_name` and `latest_update` (both
        // resolved by reverse scan).
        wrow(r#"{"timestamp":"2026-08-29T04:00:02+00:00","lifecycle_state":"updated","agent_id":"agt_shared","worker_name":"deeply's Q4 term sheet numbers","agent_type":"deeply-analyst","session_id":"s_other","summary":"deeply's Q4 term sheet numbers"}"#),
    ];

    // (1) POSITIVE PROBE — the input really does contain the foreign content, keyed to the
    //     very id this thread's Task call joins on. Without this the assertions below
    //     could pass because nothing foreign was ever there.
    assert!(
        rows.iter().any(|r| r.agent_id == "agt_shared" && r.session_id == "s_other" && r.summary == secret),
        "the fixture must actually cross the boundary, or this test proves nothing"
    );
    assert!(
        rows.iter().filter(|r| r.agent_id == "agt_shared").count() == 3,
        "the foreign row really does collide on the join key"
    );

    // (2) THE GUARD. Delete clause 3 (the `in_scope` filter in `worker_activity`) and this
    //     test fails with deeply's term-sheet line rendered inside a femcboost thread,
    //     stamped entity_id=femcboost, thread_id=thr_fem, turn_id=turn_ok.
    let timeline =
        Timeline::project_with_workers(&ledger, &binding, &task_records("thr_fem", "turn_ok", "s1"), &rows).unwrap();

    let (base, worker) = timeline
        .audit_including_internal()
        .iter()
        .find_map(|i| match i {
            TimelineItem::WorkerActivity { base, worker, .. } => Some((base, worker)),
            _ => None,
        })
        .expect("this session's worker still projects");

    assert_eq!(base.entity_id, femcboost());
    assert_eq!(worker.worker_name.as_deref(), Some("sage-opus-r3"), "the foreign name must not win the reverse scan");
    assert_eq!(worker.agent_type.as_deref(), Some("sage"));
    assert_eq!(worker.latest_update, None, "the foreign summary is not an update this session witnessed");
    assert_eq!(worker.observed_state, ObservedWorkerState::Started, "the foreign row must not become the last state");
    assert_eq!(worker.events_observed, 2, "two in-session rows, not three");

    // (3) AND THE WHOLE RENDERED SURFACE, in both modes — the assertion that survives a
    //     future refactor moving where the name is read from.
    for mode in [ViewMode::Ceo, ViewMode::Technical] {
        let rendered = serde_json::to_string(&timeline.view(mode)).unwrap();
        assert!(!rendered.contains(secret), "{mode:?} view leaked the other session's content: {rendered}");
        assert!(!rendered.contains("s_other"), "{mode:?} view leaked the other session id");
        assert!(!rendered.contains("deeply-analyst"), "{mode:?} view leaked the other session's agent type");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn no_worker_item_is_built_from_a_task_call_belonging_to_another_thread() {
    // The pre-existing machinery guard, re-proven for the NEW item type: a worker row must
    // not be reachable by routing a foreign Task call through the join. The record names
    // the deeply thread while claiming a turn femcboost legitimately accepts — so only the
    // thread clause stands between it and femcboost's screen.
    let path = tmp("wforeign", ".jsonl");
    two_entity_ledger(&path);
    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();

    let rows = vec![wrow(
        r#"{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"started","agent_id":"agt_shared","worker_name":"deeply-worker","agent_type":"deeply","session_id":"s1"}"#,
    )];

    // POSITIVE PROBE: the identical record, stamped to THIS thread, really does produce a
    // worker item — so the assertion below cannot pass merely because the join is broken.
    let ok = Timeline::project_with_workers(&ledger, &binding, &task_records("thr_fem", "turn_ok", "s1"), &rows)
        .unwrap();
    assert!(
        ok.audit_including_internal().iter().any(|i| matches!(i, TimelineItem::WorkerActivity { .. })),
        "the join works when the record is in-thread — the negative below is meaningful"
    );

    let timeline =
        Timeline::project_with_workers(&ledger, &binding, &task_records("thr_dee", "turn_ok", "s1"), &rows).unwrap();
    assert!(
        !timeline.audit_including_internal().iter().any(|i| matches!(i, TimelineItem::WorkerActivity { .. })),
        "a foreign-thread Task call must not become a worker row in this entity's timeline"
    );
    let violations = timeline.scope_violations();
    // TWO, because the native wire spends two frames on one Task call (open, then the
    // acknowledgement) and BOTH are foreign-thread. Every refusal is REPORTED, not silently
    // dropped — which is a stronger reading of the same invariant, not a weaker one.
    assert_eq!(violations.len(), 2, "and every refusal is REPORTED, not silently dropped");
    assert!(violations.iter().all(|v| v.reason == RejectionReason::ForeignThread));
    let rendered = serde_json::to_string(&timeline.view(ViewMode::Technical)).unwrap();
    assert!(!rendered.contains("deeply-worker"), "leaked: {rendered}");
    let _ = std::fs::remove_file(&path);
}

// ===========================================================================================
// THE READ PATH THE APP ACTUALLY CALLS (slice 7, 2026-08-29)
// ===========================================================================================
//
// The three tests above prove `Timeline::project_with_workers`. Until this slice NOTHING in
// the app called it: `Spine::timeline` — the body of the `get_timeline` command — called
// `Timeline::project`, which supplies an EMPTY worker stream. So `TimelineItem::WorkerActivity`
// was fully specified, fully tested and UNREACHABLE on the wire, and a delegated `Task` call
// reached the CEO as one nameless activity row reading "Worked".
//
// These two tests pin the wiring itself, because a green test over a function nobody calls is
// exactly the failure that hid it.

/// Write the shared two-entity ledger AND a worker stream file, and return a spine whose
/// timeline read path can be pointed at that file.
fn spine_with_worker_stream(tag: &str, rows: &str) -> (Spine, std::path::PathBuf, std::path::PathBuf) {
    let ledger_path = tmp(tag, ".jsonl");
    two_entity_ledger(&ledger_path);
    let stream_path = tmp(&format!("{tag}-workers"), ".jsonl");
    std::fs::write(&stream_path, rows).unwrap();

    let ledger = Ledger::open(&ledger_path).unwrap();
    let mut spine = support::spine(ledger);
    let journal = MachineryJournal::new(tmp(&format!("{tag}-mach"), ""));
    for r in task_records("thr_fem", "turn_ok", "s1") {
        journal.append(&r).unwrap();
    }
    spine.set_machinery_journal(journal);
    (spine, ledger_path, stream_path)
}

#[test]
fn the_apps_own_read_path_joins_a_task_call_to_the_worker_it_spawned() {
    // THE REGRESSION FIX, at the producer. Same fixture as
    // `a_task_call_joined_by_identity_becomes_a_worker_item`, driven through `Spine::timeline`
    // — the function `get_timeline` calls — instead of through `project_with_workers` directly.
    let rows = concat!(
        r#"{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"created","agent_id":"agt_shared","worker_name":"sage-opus-r3","agent_type":"sage","session_id":"s1"}"#,
        "\n",
        r#"{"timestamp":"2026-08-29T04:00:01+00:00","lifecycle_state":"started","agent_id":"agt_shared","agent_type":"sage","session_id":"s1"}"#,
        "\n",
    );
    let (mut spine, lp, sp) = spine_with_worker_stream("spineworker", rows);

    // (1) THE POSITIVE PROBE FOR THE NEGATIVE BELOW. Default = Disabled = what shipped:
    //     the Task call is an ordinary activity row and no worker exists on the wire.
    let before = spine.timeline("thr_fem").unwrap();
    let ceo_before = before.view(ViewMode::Ceo);
    assert!(
        !ceo_before.items().iter().any(|i| matches!(i, TimelineItem::WorkerActivity { .. })),
        "with the source Disabled the app cannot produce a worker row — this is what main shipped"
    );
    // A delegation is not a nameless tool call (finding 13 exempts it): with no worker row it
    // still reaches his view as an ordinary activity row, naming the act rather than reading
    // "Worked". This fixture's call carries no arguments, so it cannot name more.
    assert!(
        ceo_before
            .items()
            .iter()
            .any(|i| matches!(i, TimelineItem::Activity { summary, .. } if summary == "Handed work to a teammate")),
        "and the delegation reached the CEO as one activity row naming the hand-over"
    );

    // (2) WIRED. Same ledger, same journal, same machinery — one setting.
    spine.set_worker_events(WorkerEventsSource::File(sp.clone()));
    let after = spine.timeline("thr_fem").unwrap();
    let ceo = after.view(ViewMode::Ceo);
    let worker = ceo
        .items()
        .iter()
        .find_map(|i| match i {
            TimelineItem::WorkerActivity { worker, detail, .. } => Some((worker, detail)),
            _ => None,
        })
        .expect("the app's own read path now produces a worker row");
    assert_eq!(worker.0.worker_name.as_deref(), Some("sage-opus-r3"));
    assert_eq!(worker.0.observed_state, ObservedWorkerState::Started);
    assert_eq!(worker.0.state, WorkerState::Running);
    assert!(worker.1.is_none(), "the technical half is still removed from a CEO view");
    assert!(
        !ceo.items()
            .iter()
            .any(|i| matches!(i, TimelineItem::Activity { summary, .. } if summary == "Handed work to a teammate")),
        "and the activity row is REPLACED, not duplicated alongside the worker row"
    );

    let _ = std::fs::remove_file(&lp);
    let _ = std::fs::remove_file(&sp);
}

#[test]
fn a_delegation_without_a_worker_row_names_what_rich_handed_over_and_to_whom() {
    // Product hunt 2026-10-15, finding 13: a row that just says "Worked" names nothing. A
    // delegation stays in his view before its worker row exists, and for a synchronous one
    // that never becomes a worker, so its row says what Rich handed over and to whom, from
    // the delegation call's own arguments. The three frames the native wire spends on it:
    // the streamed open (no arguments yet), the `assistant` frame with the whole input, and
    // the result.
    let path = tmp("delegname", ".jsonl");
    two_entity_ledger(&path);
    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();

    let label = |id: &str, input: Value| -> Vec<String> {
        let frames = [
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"tool_use","id":id,"name":"Agent","input":{}}}}),
            json!({"type":"assistant","message":{"role":"assistant","content":[
                   {"type":"tool_use","id":id,"name":"Agent","input":input}]}}),
            json!({"type":"user","message":{"role":"user","content":[
                   {"tool_use_id":id,"type":"tool_result","content":"The login works again."}]}}),
        ];
        let mut records = Vec::new();
        for (seq, f) in frames.iter().enumerate() {
            for r in MachineryRecord::from_native_event(f, "s1", seq as u64 + 1) {
                records.push(r.stamp("thr_fem", Some("turn_ok"), false));
            }
        }
        let timeline = Timeline::project_with_workers(&ledger, &binding, &records, &[]).unwrap();
        timeline
            .view(ViewMode::Ceo)
            .items()
            .iter()
            .filter_map(|i| match i {
                TimelineItem::Activity { summary, .. } => Some(summary.clone()),
                _ => None,
            })
            .collect()
    };

    // A teammate by name: the display name the worker row would show (`timeline.js`
    // `teammateName`: "mark-sonnet-f1" is "Mark").
    assert_eq!(
        label(
            "toolu_A",
            json!({"description":"Fix the login bug","name":"mark-sonnet-f1","subagent_type":"mark",
                   "prompt":"The login page rejects a correct password."})
        ),
        vec!["Handed to Mark: Fix the login bug".to_string()]
    );
    // No teammate name: its role, as the worker row falls back to it.
    assert_eq!(
        label("toolu_B", json!({"description":"Check the release notes","subagent_type":"ray"})),
        vec!["Handed to ray: Check the release notes".to_string()]
    );
    // Nobody named at all: still what was handed over.
    assert_eq!(
        label("toolu_C", json!({"description":"Look into the slow start","prompt":"..."})),
        vec!["Handed to a teammate: Look into the slow start".to_string()]
    );
    std::fs::remove_file(&path).unwrap_or_default();
}

#[test]
fn a_session_id_mismatch_is_reported_rather_than_silently_producing_no_worker() {
    // The failure mode that would otherwise be invisible. The stream HAS rows for exactly
    // the agent id this Task call spawned — but under a different session id, which is what
    // happens if the ACP session id and the harness session id turn out to be different id
    // spaces. The join must still refuse (agent_id is not globally unique), and the refusal
    // must be TELLABLE from "the engine emitted nothing at all".
    let rows = concat!(
        r#"{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"created","agent_id":"agt_shared","worker_name":"someone-elses-worker","agent_type":"sage","session_id":"a-different-uuid"}"#,
        "\n",
    );
    let (mut spine, lp, sp) = spine_with_worker_stream("spinemismatch", rows);
    spine.set_worker_events(WorkerEventsSource::File(sp.clone()));

    let timeline = spine.timeline("thr_fem").unwrap();
    assert!(
        !timeline.audit_including_internal().iter().any(|i| matches!(i, TimelineItem::WorkerActivity { .. })),
        "a foreign session's row must not attach — the leak guard holds"
    );
    let mismatches: Vec<_> = timeline
        .rejections()
        .iter()
        .filter(|r| r.reason == RejectionReason::WorkerSessionMismatch)
        .collect();
    assert_eq!(mismatches.len(), 1, "the refusal is reported: {:?}", timeline.rejections());
    assert!(
        !mismatches[0].is_scope_violation(),
        "nothing crossed a boundary — something was correctly kept out; it is a diagnostic, not a leak"
    );
    let rendered = serde_json::to_string(&timeline.view(ViewMode::Technical)).unwrap();
    assert!(!rendered.contains("someone-elses-worker"), "leaked: {rendered}");

    // And an EMPTY stream produces no such report — the two states are distinguishable.
    std::fs::write(&sp, "").unwrap();
    let quiet = spine.timeline("thr_fem").unwrap();
    assert!(
        !quiet.rejections().iter().any(|r| r.reason == RejectionReason::WorkerSessionMismatch),
        "no rows for the id at all is a different statement and must not report a mismatch"
    );

    let _ = std::fs::remove_file(&lp);
    let _ = std::fs::remove_file(&sp);
}

/// **ONE ANSWER PER TURN, EVEN WHEN THE TURN WAS RUN TWICE** — Ray's candidate-.12 row 10 /
/// defect C: *"the Mac renders the greeting twice, the phone once, same thread, same build,
/// same moment"* (OCR count Mac 2, phone 1; frames 23 and 25).
///
/// # The fixture is the walk's own ledger, copied line for line
///
/// From `conversation-ledger.jsonl` of the candidate-.12 run, lines 764-779, one turn:
///
/// ```text
///   TurnStarted   session 06bfcb45-…
///   TurnStarted   session 25a805fc-…        <- a DIFFERENT session, SAME turn id
///   PromptReceived  source=internal "[re-prime]"
///   AssistantDelta seq  7.. 12  "Hi. I've got you. What can I do for you?"
///   AssistantDelta seq 24.. 26  "Hi. I've got you. What can I do for you?"
///   TurnCompleted  end_turn
/// ```
///
/// The turn was handed to one lease, the lease was replaced and re-primed under the same turn
/// id, and the successor answered the same prompt again. `Ledger`'s replay folded both runs
/// into one turn's text, so the conversation carried the sentence twice.
///
/// **This is a projection test, not a log test.** Nothing rewrites the ledger: both runs are
/// still on disk, which is what makes the file evidence. What the CEO is shown is the answer
/// from the lease that finished the turn.
#[test]
fn a_turn_restarted_on_a_new_session_is_answered_once() {
    let path = tmp("restarted-turn", ".jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_fem","title":"Running","at":1,"entity_id":"femcboost","person_id":"ceo-default","binding_revision":1}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_x","thread_id":"thr_fem","text":"hello from my phone","source":"text","at":2,"entity_id":"femcboost","binding_revision":1}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_x","session_id":"06bfcb45","at":3}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_x","session_id":"25a805fc","at":4}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_x","text":"Hi. I've got you. What can I do for you?","at":5,"seq":7}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_x","text":"Hi. I've got you. What can I do for you?","at":6,"seq":24}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_x","stop_reason":"end_turn","at":7}"#,
            "\n",
        ),
    )
    .unwrap();

    // (1) POSITIVE PROBE — the FILE really does carry the sentence twice. Without this the
    //     assertion below could pass because the fixture only ever said it once, which is the
    //     negative test that passes for the wrong reason.
    let raw = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        raw.matches("Hi. I've got you.").count(),
        2,
        "the fixture must actually contain both runs, or this test proves nothing"
    );

    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();
    let timeline = Timeline::project(&ledger, &binding, &[]).unwrap();
    let shown = format!("{:?}", timeline.view(ViewMode::Ceo));

    assert_eq!(
        shown.matches("Hi. I've got you.").count(),
        1,
        "ONE question, ONE answer: the run from the lease that did not finish the turn is \
         superseded, not added to. Rendered:\n{shown}"
    );
    // And the answer he keeps is a whole sentence rather than half of each run.
    assert!(
        shown.contains("Hi. I've got you. What can I do for you?"),
        "the surviving run is not intact:\n{shown}"
    );
    // AND NOTHING WAS DELETED. The repeat is still projected — into a visibility that
    // renders in no mode — so the record of what happened survives the screen being right.
    let audited = format!("{:?}", timeline.audit_including_internal());
    assert_eq!(
        audited.matches("Hi. I've got you.").count(),
        2,
        "the second run must be RETAINED, not dropped: {audited}"
    );
    // The technical view is not a back door for it either: it is Internal, like a vendor
    // diagnostic, which renders in no mode at all.
    let techy = format!("{:?}", timeline.view(ViewMode::Technical));
    assert_eq!(
        techy.matches("Hi. I've got you.").count(),
        1,
        "the repeat must not reappear in the technical view: {techy}"
    );

    let _ = std::fs::remove_file(path);
}

/// **AND TWO DIFFERENT THINGS SAID IN ONE TURN ARE STILL TWO THINGS.** The negative half, so
/// the rule above cannot quietly become "a turn renders one paragraph".
///
/// The fixture is the ordinary shape it has to survive: Rich says something, a tool call takes
/// the next few sequence positions, and Rich says something else. That is two runs, two
/// bubbles, and it is what every turn with a tool call in it looks like.
#[test]
fn two_different_runs_in_one_turn_both_render() {
    let path = tmp("two-runs", ".jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_fem","title":"Running","at":1,"entity_id":"femcboost","person_id":"ceo-default","binding_revision":1}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_x","thread_id":"thr_fem","text":"how is the release","source":"text","at":2,"entity_id":"femcboost","binding_revision":1}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_x","session_id":"s1","at":3}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_x","text":"Let me look.","at":4,"seq":0}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_x","text":"It is on track.","at":5,"seq":9}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_x","stop_reason":"end_turn","at":6}"#,
            "\n",
        ),
    )
    .unwrap();
    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();
    let timeline = Timeline::project(&ledger, &binding, &[]).unwrap();
    let shown = format!("{:?}", timeline.view(ViewMode::Ceo));
    assert!(shown.contains("Let me look."), "the first run is gone:\n{shown}");
    assert!(shown.contains("It is on track."), "the second run is gone:\n{shown}");
    let _ = std::fs::remove_file(path);
}

/// **AND THE SAME QUESTION ASKED TWICE IS ANSWERED TWICE.** The rule is per TURN, and this is
/// the fixture that holds it there: two turns, the same words, both on screen.
#[test]
fn the_same_answer_in_a_later_turn_is_still_his_to_see() {
    let path = tmp("two-turns", ".jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_fem","title":"Running","at":1,"entity_id":"femcboost","person_id":"ceo-default","binding_revision":1}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_a","thread_id":"thr_fem","text":"say hello","source":"text","at":2,"entity_id":"femcboost","binding_revision":1}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_a","session_id":"s1","at":3}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_a","text":"Hi. I've got you.","at":4,"seq":0}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_a","stop_reason":"end_turn","at":5}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_b","thread_id":"thr_fem","text":"say it again","source":"text","at":6,"entity_id":"femcboost","binding_revision":1}"#,
            "\n",
            r#"{"event":"TurnStarted","turn_id":"turn_b","session_id":"s1","at":7}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_b","text":"Hi. I've got you.","at":8,"seq":0}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_b","stop_reason":"end_turn","at":9}"#,
            "\n",
        ),
    )
    .unwrap();
    let ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding("thr_fem").unwrap();
    let timeline = Timeline::project(&ledger, &binding, &[]).unwrap();
    let shown = format!("{:?}", timeline.view(ViewMode::Ceo));
    assert_eq!(
        shown.matches("Hi. I've got you.").count(),
        2,
        "he asked twice and is entitled to both answers:\n{shown}"
    );
    let _ = std::fs::remove_file(path);
}
