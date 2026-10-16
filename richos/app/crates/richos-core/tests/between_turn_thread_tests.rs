//! BETWEEN-TURN TRAFFIC, END TO END — techy-mode §1.5's gap #1, from the wire to the view.
//!
//! `between_turn_tests.rs` proves the client parks what it used to drop. This proves the
//! rest of the sentence: that it reaches the journal, attaches to the THREAD rather than to
//! a turn, obeys the retention setting like everything else, and shows up in the technical
//! view — while re-prime machinery structurally cannot.
//!
//! **Ported from the ACP wire to the native `claude` wire (`wiki/ceo-decisions.md` §16).**
//! Same six tests, same six invariants; only the vendor's vocabulary changed.
//!
//! **The first test drives a real child process.** Not a mocked lease: a POSIX-sh fake
//! agent, spawned through `NativeCognition::start`, whose stdout the real reader thread
//! reads and the real `dispatch` routes. The frame it emits at session start is the one the
//! design says *"hits no sink at all"*. Everything downstream of it here — journal,
//! projection, gate — is the shipping path.

use richos_core::native::NativeCognition;
use richos_core::cognition::{Cognition, CognitionError, MockCognition, TurnItem};
use richos_core::entity::EntityId;
use richos_core::journal::{MachineryJournal, RawRetention};
use richos_core::ledger::{Ledger, Source};
use richos_core::machinery::MachineryRecord;
use richos_core::spine::Spine;
use richos_core::timeline::{ViewMode, Visibility};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp(tag: &str, ext: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "richos-btl-{tag}-{}-{}{ext}",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// The session-start frame the real agent emits once per turn, verbatim in shape from
/// `raw/run9-rust-driven.jsonl:1` (trimmed to the fields these tests read).
fn session_init() -> serde_json::Value {
    json!({"type":"system","subtype":"init","model":"claude-sonnet-5","tools":["Bash"],
           "session_id":"s","uuid":"u-start"})
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f() {
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// An executable fake native agent. Passed to `NativeCognition::start` AS the binary, so it
/// carries its own shebang; the client's real flag vector arrives as arguments and `sh`
/// ignores them, which makes this a fake of the BINARY rather than of the protocol.
fn fake_agent(tag: &str) -> PathBuf {
    let path = tmp(&format!("agent-{tag}"), ".sh");
    let script = r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"initialize"'*)
      printf '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}\n'
      # SESSION START. No turn has been sent, so `current_prompt` is None and this is
      # precisely the frame §1.5 says hits no sink at all.
      printf '{"type":"system","subtype":"init","model":"claude-sonnet-5","tools":["Bash"],"session_id":"s","uuid":"u-start"}\n'
      ;;
    *'"type":"user"'*)
      printf '{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1"}}}\n'
      printf '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"the build is green"}}}\n'
      printf '{"type":"result","subtype":"success","stop_reason":"end_turn","terminal_reason":"completed"}\n'
      # AFTER the result: between turns again.
      printf '{"type":"system","subtype":"status","status":"requesting","session_id":"s","uuid":"u-st"}\n'
      ;;
  esac
done
"#;
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(script.as_bytes()).unwrap();
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

// ---------------------------------------------------------------------------
// THE COMPLETION CRITERION, over a real child process
// ---------------------------------------------------------------------------

#[test]
fn a_real_agents_session_start_frame_reaches_the_journal_and_the_technical_view() {
    let script = fake_agent("live");
    let ledger_path = tmp("live", ".jsonl");
    let journal_root = tmp("live-journal", "");

    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    spine.set_machinery_journal(MachineryJournal::new(&journal_root));
    let doctrine_dir = std::env::temp_dir().join(format!("richos-doctrine-btt-{}", std::process::id()));
    let doctrine = richos_core::doctrine::ensure_rendered(&doctrine_dir, &richos_core::doctrine::DoctrineIdentity::default()).unwrap();
    let skills = richos_core::skills::ensure_rendered(&doctrine_dir, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    spine.attach_lease(Box::new(NativeCognition::start(&script, Path::new("/tmp"), &doctrine, &skills).unwrap()));
    let thread = spine.create_thread("Avelor release", &femcboost()).unwrap();

    // The child writes its session-start frame immediately after answering the handshake,
    // so the reader thread may not have consumed it yet. Poll rather than sleep, so a real
    // regression reads as a regression.
    wait_for("the session-start frame to land in the journal", || spine.pump_between_turn() > 0);

    // IT IS ON DISK, with no turn (§1.4 G4) and stamped with the thread.
    let on_disk = MachineryJournal::new(&journal_root).read_thread(&thread);
    let start: Vec<&MachineryRecord> = on_disk.iter().filter(|r| r.title == "system:init").collect();
    assert_eq!(start.len(), 1, "one session-start frame, journaled: {on_disk:#?}");
    assert_eq!(start[0].turn_id, None, "it attaches to the thread, never to a turn");
    assert_eq!(start[0].thread_id, thread);
    assert!(!start[0].internal, "vendor session traffic is honest, not internal");
    assert_eq!(
        start[0].payload.as_ref().unwrap()["tools"],
        json!(["Bash"]),
        "retained verbatim (§1.4 G5)"
    );

    // IT IS IN THE TECHNICAL VIEW, in the between-turn lane.
    let timeline = spine.timeline(&thread).unwrap();
    let technical = timeline.view(ViewMode::Technical);
    let kinds: Vec<&str> = technical.between_turns().iter().map(|b| b.vendor_kind.as_str()).collect();
    assert!(kinds.contains(&"system:init"), "got {kinds:?}");

    // AND NOT IN THE CALM ONE. Every row in this lane is `Technical`, so the CEO view is
    // handed nothing to skip.
    assert!(timeline.view(ViewMode::Ceo).between_turns().is_empty());

    // A real turn still works, and its traffic is unaffected: the reply is the reply.
    spine.submit_prompt("how is the release", Source::Text).unwrap();
    let msgs = spine.messages(&thread).unwrap();
    assert!(
        msgs.iter().any(|m| m.text.contains("the build is green")),
        "the clean-output path is untouched: {msgs:#?}"
    );

    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&ledger_path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

// ---------------------------------------------------------------------------
// A SCRIPTED LEASE, for the properties a fixed fake adapter cannot script
// ---------------------------------------------------------------------------

/// A lease whose between-turn lane a test fills directly, and whose `reprime` emits real
/// machinery — so the standing order can be tested rather than argued about.
struct RepriminingLease {
    session_id: String,
    between: Vec<serde_json::Value>,
    /// What `reprime` emits. A real priming turn runs real tools; this is one of them.
    reprime_machinery: Vec<serde_json::Value>,
    next_seq: u64,
}

impl Cognition for RepriminingLease {
    fn session_id(&self) -> &str {
        &self.session_id
    }
    fn reprime(&mut self, _priming: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        let mut seq = 0u64;
        for f in self.reprime_machinery.clone().iter() {
            for r in MachineryRecord::from_native_event(f, &self.session_id, seq) {
                seq += 1;
                on_item(TurnItem::Machinery(r));
            }
        }
        Ok(())
    }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        on_item(TurnItem::Text { seq: 0, text: &format!("ack: {text}") });
        Ok("end_turn".to_string())
    }
    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        let mut out = Vec::new();
        for f in std::mem::take(&mut self.between) {
            let records = MachineryRecord::from_native_between_turn(&f, &self.session_id, self.next_seq);
            self.next_seq += records.len() as u64;
            out.extend(records);
        }
        out
    }
}

fn spine_with_lease(
    ledger_path: &Path,
    journal_root: &Path,
    lease: Box<dyn Cognition>,
) -> (Spine, String) {
    let mut spine = support::spine(Ledger::open(ledger_path).unwrap());
    spine.set_machinery_journal(MachineryJournal::new(journal_root));
    spine.attach_lease(lease);
    let thread = spine.create_thread("Avelor release", &femcboost()).unwrap();
    (spine, thread)
}

#[test]
fn reprime_machinery_is_recorded_and_can_never_reach_a_rendered_thread() {
    // THE STANDING ORDER, as a test. Re-prime runs a REAL turn and its machinery now flows
    // (§1.5) — so the question is not whether it is recorded but whether anything can ever
    // render it. Three answers, and all three must hold:
    //
    //   1. it IS on disk, `internal: true`, `turn_id: None`;
    //   2. the timeline REFUSES it at the guard — it never becomes an item or a lane row;
    //   3. neither view can produce it, in either mode.
    let ledger_path = tmp("reprime", ".jsonl");
    let journal_root = tmp("reprime-journal", "");
    let lease = RepriminingLease {
        session_id: "sess-1".into(),
        between: vec![session_init()],
        reprime_machinery: vec![json!({"type":"stream_event","event":{"type":"content_block_start",
            "index":0,"content_block":{"type":"tool_use","id":"toolu_RP","name":"read the action ledger",
                                       "input":{}}}})],
        next_seq: 0,
    };
    let (mut spine, thread) = spine_with_lease(&ledger_path, &journal_root, Box::new(lease));

    // A CEO turn: this is what triggers `prime_lease_if_needed`, which runs the priming
    // turn and drains its residue.
    spine.submit_prompt("how is the release", Source::Text).unwrap();

    // (1) RECORDED.
    let on_disk = MachineryJournal::new(&journal_root).read_thread(&thread);
    let priming: Vec<&MachineryRecord> =
        on_disk.iter().filter(|r| r.title == "read the action ledger").collect();
    assert_eq!(priming.len(), 1, "re-prime machinery is retained for debugging: {on_disk:#?}");
    assert!(priming[0].internal, "and it is stamped internal");
    assert_eq!(priming[0].turn_id, None);

    // (2) REFUSED AT THE GUARD.
    let timeline = spine.timeline(&thread).unwrap();
    assert!(timeline.scope_violations().is_empty(), "an internal record is not a leak");
    assert!(
        timeline.rejections().iter().any(|r| r.record_id == priming[0].machinery_id),
        "the guard must REPORT the refusal, not drop it silently"
    );
    // Not in the ungated lane either — it never became a row at all.
    assert!(timeline
        .between_turns_ungated()
        .iter()
        .all(|b| b.vendor_kind != "read the action ledger"));

    // (3) NO MODE CAN PRODUCE IT.
    for mode in [ViewMode::Ceo, ViewMode::Technical] {
        let view = timeline.view(mode);
        let rendered = format!("{:?}", (view.items(), view.between_turns()));
        assert!(
            !rendered.contains("read the action ledger"),
            "re-prime machinery reached a rendered thread in {mode:?}"
        );
    }

    // And the honest between-turn traffic that arrived BEFORE priming did render — so this
    // test is not passing because nothing renders at all.
    let technical = timeline.view(ViewMode::Technical);
    assert_eq!(
        technical.between_turns().iter().map(|b| b.vendor_kind.as_str()).collect::<Vec<_>>(),
        vec!["system:init"]
    );

    let _ = std::fs::remove_file(&ledger_path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

#[test]
fn a_lane_row_carries_no_session_id_because_that_is_where_a_rotation_would_show() {
    // Every `MachineryRecord` carries a `session_id` — rotation must stay reconstructible
    // (`ledger.rs:170`). No rendering type in `timeline.rs` has ever exposed one, and this
    // lane is the place where it would matter most: its rows arrive at session boundaries.
    let ledger_path = tmp("nosess", ".jsonl");
    let journal_root = tmp("nosess-journal", "");
    let lease = RepriminingLease {
        session_id: "sess-SECRET-ROTATION-ID".into(),
        between: vec![json!({"type":"system","subtype":"status","status":"requesting",
                             "session_id":"s","uuid":"u-st"})],
        reprime_machinery: vec![],
        next_seq: 0,
    };
    let (mut spine, thread) = spine_with_lease(&ledger_path, &journal_root, Box::new(lease));
    spine.pump_between_turn();

    let view = spine.timeline(&thread).unwrap().view(ViewMode::Technical);
    assert_eq!(view.between_turns().len(), 1);
    let wire = serde_json::to_string(&view).unwrap();
    assert!(wire.contains("system:status"), "the row is there: {wire}");
    assert!(!wire.contains("sess-SECRET-ROTATION-ID"), "no session id on the wire: {wire}");

    let _ = std::fs::remove_file(&ledger_path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

#[test]
fn a_thread_with_no_between_turn_traffic_has_an_empty_lane_and_that_is_not_a_failure() {
    // THE HONEST EMPTY STATE, at the data layer. A quiet lane must be an EMPTY lane and not
    // a missing field, an error, or a fabricated row — the renderer's sentence about it is
    // checked in `app/ui/tests/techy.js`.
    let ledger_path = tmp("quiet", ".jsonl");
    let journal_root = tmp("quiet-journal", "");
    let mock = MockCognition::new("sess-quiet", vec!["on track"]);
    let (mut spine, thread) = spine_with_lease(&ledger_path, &journal_root, Box::new(mock));
    spine.submit_prompt("how is the release", Source::Text).unwrap();

    assert_eq!(spine.pump_between_turn(), 0, "nothing to pump is an answer, not an error");
    let timeline = spine.timeline(&thread).unwrap();
    assert!(timeline.view(ViewMode::Technical).between_turns().is_empty());
    // The conversation itself is untouched — an empty lane is not an empty thread.
    assert!(!timeline.view(ViewMode::Ceo).items().is_empty());

    let _ = std::fs::remove_file(&ledger_path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

#[test]
fn between_turn_records_obey_the_retention_setting_like_every_other_record() {
    // §2.4/§7.2: the raw window is a value in the config store, and these records are not
    // exempt from it. They go through the same `MachineryJournal::append`, so the Tier-B
    // eviction that passes over a tool call passes over these too — and what survives is
    // the normalized row with `payload: None`, which is the honest degrade, never a blank.
    let ledger_path = tmp("retain", ".jsonl");
    let journal_root = tmp("retain-journal", "");
    let lease = RepriminingLease {
        session_id: "sess-1".into(),
        between: vec![session_init()],
        reprime_machinery: vec![],
        next_seq: 0,
    };
    let (mut spine, thread) = spine_with_lease(&ledger_path, &journal_root, Box::new(lease));
    assert_eq!(spine.pump_between_turn(), 1);

    let journal = MachineryJournal::new(&journal_root);
    assert!(journal.read_thread(&thread)[0].payload.is_some(), "the raw payload starts retained");

    // A window of ZERO days, applied 15 days after the record was written: 15 > 0, so the
    // shard is past the window. Re-derived rather than trusted — `evict_raw_within` compares
    // shard age in days against the window.
    let fifteen_days_on = richos_core::util::now_millis() + 15 * 86_400_000;
    let evicted = journal.evict_raw_within(fifteen_days_on, RawRetention::of(0, u64::MAX));
    assert_eq!(evicted, 1, "the between-turn record's raw shard was evicted like any other");

    let after = journal.read_thread(&thread);
    assert_eq!(after.len(), 1, "Tier A survives — the row still renders");
    assert!(after[0].payload.is_none(), "Tier B is gone, and says so by being absent");

    let _ = std::fs::remove_file(&ledger_path);
    let _ = std::fs::remove_dir_all(&journal_root);
}

#[test]
fn the_lane_is_gated_by_the_same_visibility_the_items_are() {
    // §1.5 says mirror the existing mechanism, do not invent a second one. This is that
    // sentence as an assertion: the rows carry `Visibility`, and `Timeline::view` filters
    // them with the SAME `renders_in` it filters items with.
    let ledger_path = tmp("gate", ".jsonl");
    let journal_root = tmp("gate-journal", "");
    let lease = RepriminingLease {
        session_id: "sess-1".into(),
        between: vec![session_init()],
        reprime_machinery: vec![],
        next_seq: 0,
    };
    let (mut spine, thread) = spine_with_lease(&ledger_path, &journal_root, Box::new(lease));
    spine.pump_between_turn();

    let timeline = spine.timeline(&thread).unwrap();
    let row = &timeline.between_turns_ungated()[0];
    assert_eq!(row.visibility, Visibility::Technical, "a vendor kind is a technical row");
    assert!(!row.visibility.renders_in(ViewMode::Ceo));
    assert!(row.visibility.renders_in(ViewMode::Technical));
    assert!(timeline.view(ViewMode::Ceo).between_turns().is_empty());
    assert_eq!(timeline.view(ViewMode::Technical).between_turns().len(), 1);

    let _ = std::fs::remove_file(&ledger_path);
    let _ = std::fs::remove_dir_all(&journal_root);
}
