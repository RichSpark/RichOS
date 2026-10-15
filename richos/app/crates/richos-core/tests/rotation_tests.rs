//! Rotation, crash-recovery, and proactive-seam invariants — P1.4's continuity leg.
//! All run headless with `MockCognition`/`MockLeaseFactory` (no live Claude/network);
//! the live round-trip itself is proven separately (examples/native_roundtrip.rs).
//!
//! Test names document the invariant, per the engineering convention this codebase
//! already follows (spine_tests.rs).

use richos_core::cognition::{Cognition, CognitionError, TurnItem, MockCognition, MockLeaseFactory};
use richos_core::ledger::{ActionVisibility, AttentionTier, Ledger, Source, TurnState};
use richos_core::machinery::MachineryRecord;
use richos_core::spine::ContextSource;
use richos_core::entity::EntityId;

/// The dogfood entity these tests run under. Every thread now has an immutable entity
/// home (ECS §3.2) and there is no entity-less path, so the tests NAME one rather than
/// inheriting a default that no longer exists.
fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}
use richos_core::stream::{StreamEvent, TurnObserver};
use richos_core::LeaseFactory;
use std::sync::{Arc, Mutex};

mod support;

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-rotation-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

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

/// A lease that streams one partial chunk then dies (a positive-signal crash, never
/// inferred from silence) — mirrors `spine_tests.rs`'s private `FailingCognition`.
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

/// A `LeaseFactory` whose every spawned lease ALSO dies immediately — used to prove
/// recovery is bounded to ONE attempt, not an infinite crash loop.
struct AlwaysFailingLeaseFactory {
    spawn_count: Arc<Mutex<u64>>,
}
impl LeaseFactory for AlwaysFailingLeaseFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let mut n = self.spawn_count.lock().unwrap();
        *n += 1;
        Ok(Box::new(FailingCognition { session_id: format!("sess-always-fails-{n}") }))
    }
}

// ============================================================================
// Rotation — invisible continuity (continuity design §3, done-criterion (a))
// ============================================================================

#[test]
fn explicit_rotation_swaps_the_lease_and_the_conversation_survives_it() {
    // Forces a rotation mid-conversation (the "!rotate equivalent" trigger, §3.2) and
    // shows the conversation continuing seamlessly on the successor — done-criterion (a).
    let (path, ledger) = tmp_ledger("explicit-rotation");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    let factory = MockLeaseFactory::new(vec!["reply from successor lease"]);
    let initial = MockCognition::new("sess-initial", vec!["reply from the first lease"]);
    spine.attach_lease(Box::new(initial));
    spine.set_lease_factory(Box::new(factory));

    // Turn 1, on the ORIGINAL lease.
    let turn1 = spine.submit_prompt("what's on my plate today?", Source::Text).unwrap();
    let session_after_turn1 = spine.ledger().turn(&turn1).unwrap().session_id.clone();

    assert_eq!(spine.rotation_count(), 0, "no rotation triggered yet");

    // Schedule it without doing invisible model work after the terminal event.
    spine.request_rotation("test-forced").unwrap();
    assert_eq!(spine.rotation_count(), 0);

    // Turn 2, on the SUCCESSOR lease — the CEO just keeps talking.
    let turn2 = spine.submit_prompt("and what's still open from before?", Source::Text).unwrap();
    let session_after_turn2 = spine.ledger().turn(&turn2).unwrap().session_id.clone();
    assert_eq!(spine.rotation_count(), 1);
    assert_eq!(spine.last_rotation_reason(), Some("test-forced"));

    // Different backing sessions...
    assert_ne!(session_after_turn1, session_after_turn2, "rotation actually swapped the lease");
    // ...but ONE unbroken conversation: both exchanges are still there, in order.
    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.len(), 4, "both turns' user+assistant pairs are intact across the rotation");
    assert_eq!(msgs[0].text, "what's on my plate today?");
    assert_eq!(msgs[1].text, "reply from the first lease");
    assert_eq!(msgs[2].text, "and what's still open from before?");
    assert_eq!(msgs[3].text, "reply from successor lease");
    let _ = std::fs::remove_file(&path);
}

/// A lease that streams a reply AND reports its context usage the way the real client does.
///
/// The usage record is built by the PRODUCTION constructor
/// (`MachineryRecord::from_context_usage`) with the verbatim `usage` object measured on the
/// native wire (`raw/run9-rust-driven.jsonl:19`), so these tests exercise the same record
/// the live client emits rather than a hand-built struct that could drift away from it.
///
/// **DERIVED on this wire, and the record says so.** ACP handed `{used, size}` over on a
/// `usage_update`; the native binary reports the numerator mid-turn on `message_delta` and
/// the denominator only when a turn ENDS. `native.rs` holds both halves and emits nothing
/// until it has them — spike caveat C3.
struct ReportingCognition {
    session_id: String,
    /// The `{used, size}` pairs to report, in order, before the reply text.
    usage: Vec<(u64, u64)>,
    reply: String,
}

impl ReportingCognition {
    fn new(session_id: &str, usage: Vec<(u64, u64)>, reply: &str) -> Self {
        ReportingCognition { session_id: session_id.to_string(), usage, reply: reply.to_string() }
    }
    fn emit_usage(&self, on_item: &mut dyn FnMut(TurnItem), seq: &mut u64) {
        for (used, size) in &self.usage {
            // The verbatim vendor object the numerator is summed from, retained with it.
            let usage = serde_json::json!({"input_tokens":2,"cache_creation_input_tokens":3603,
                                           "cache_read_input_tokens":used.saturating_sub(3605)});
            let rec =
                MachineryRecord::from_context_usage(*used, *size, &usage, &self.session_id, *seq);
            on_item(TurnItem::Machinery(rec));
            *seq += 1;
        }
    }
}

impl Cognition for ReportingCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }
    fn reprime(&mut self, _priming_text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
    }
    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let mut seq = 0u64;
        self.emit_usage(on_item, &mut seq);
        on_item(TurnItem::Text { seq, text: &self.reply });
        Ok("end_turn".to_string())
    }
}

#[test]
fn the_watermark_is_driven_by_the_measured_usage_not_by_the_char_estimate() {
    // Frank F2, the fix. 70% of a MEASURED 1_000_000 window is 700_000 tokens; the lease
    // says it is there, so rotation fires — while the char estimate over the same turn is
    // three orders of magnitude smaller and could not have triggered anything.
    let (path, ledger) = tmp_ledger("measured-watermark");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ReportingCognition::new("sess-1", vec![(700_000, 1_000_000)], "ok")));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["post-rotation reply"])));

    assert_eq!(spine.context_source(), ContextSource::Estimated, "nothing reported yet");
    spine.submit_prompt("hello", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "rotation waits for a cancellable request");
    spine.submit_prompt("next request", Source::Text).unwrap();

    // 700_000 / 1_000_000 = 0.70, and the default ratio is 0.70 -> reached (>=, not >).
    assert_eq!(spine.rotation_count(), 1, "the MEASURED watermark rotated at the boundary");
    assert_eq!(spine.last_rotation_reason(), Some("context-watermark"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_unreported_lease_falls_back_to_the_estimate_and_says_it_is_an_estimate() {
    // The fallback must be honest about being one. A lease that has not reported is in a
    // genuinely different state, and `context_source()` is the type-level way to say so.
    let (path, ledger) = tmp_ledger("fallback-honest");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["short reply"])));

    assert_eq!(spine.context_source(), ContextSource::Estimated);
    assert!(!spine.context_source().is_measured(), "an estimate must never read as measured");
    assert_eq!(spine.context_usage(), None, "no measurement exists to hand out");

    spine.submit_prompt("hello", Source::Text).unwrap();

    // MockCognition emits no usage_update ever, so it stays on the fallback forever - and
    // the fallback still WORKS: the estimate is non-zero and the window is the configured
    // one, not a measured one.
    assert_eq!(spine.context_source(), ContextSource::Estimated, "still nothing reported");
    assert!(spine.context_estimate_tokens() > 0, "the estimate still accumulates");
    assert_eq!(spine.context_window_tokens(), 200_000, "no measurement -> the configured fallback");
    assert_eq!(spine.context_used_tokens(), spine.context_estimate_tokens());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_lease_that_has_not_reported_yet_still_rotates_sensibly() {
    // The fallback path, exercised end to end: same tiny-budget setup as the pre-existing
    // watermark test, against a lease that never reports. Rotation must still happen -
    // "we have no measurement" may not become "we never rotate".
    let (path, ledger) = tmp_ledger("fallback-rotates");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["short reply"])));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["post-rotation reply"])));
    spine.set_context_budget(1000, 0.001); // threshold = 1 token

    spine.submit_prompt("hello", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "rotation waits for a cancellable request");
    spine.submit_prompt("next request", Source::Text).unwrap();

    assert_eq!(spine.rotation_count(), 1, "the ESTIMATE still triggers rotation when it is all we have");
    assert_eq!(spine.last_rotation_reason(), Some("context-watermark"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_measured_window_supersedes_the_configured_one_and_the_configured_ratio_survives() {
    // The "wrong scale" half of F2. 200_000 was the app's guess; 1_000_000 is what the
    // adapter reported in 50 of 50 measured events. The wire wins on the WINDOW (it knows
    // which model is behind the session); the RATIO is policy and stays ours.
    let (path, ledger) = tmp_ledger("measured-window");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ReportingCognition::new("sess-1", vec![(150_000, 1_000_000)], "ok")));
    spine.set_context_budget(200_000, 0.70);

    spine.submit_prompt("hello", Source::Text).unwrap();

    assert_eq!(spine.context_source(), ContextSource::Measured);
    assert_eq!(spine.context_window_tokens(), 1_000_000, "the wire's size, not the guess");
    assert_eq!(spine.configured_context_window_tokens(), 200_000, "the guess stays inspectable");
    assert_eq!(spine.context_used_tokens(), 150_000);
    // 150_000 / 1_000_000 = 0.15, ratio 0.70 -> not reached. Under the OLD arithmetic
    // 150_000 >= 200_000 * 0.70 = 140_000, so it WOULD have rotated at 15% of capacity.
    assert!(!spine.watermark_reached(), "15% of the real window is not 70% of it");
    assert!((spine.context_fraction() - 0.15).abs() < 1e-12, "got {}", spine.context_fraction());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn removing_the_measurement_brings_the_defect_back_the_lease_runs_to_the_wall_unrotated() {
    // THE NEGATIVE CONTROL, and it is a real one: the only thing removed is the signal.
    // Two identical spines, identical budget, identical prompt; one lease reports
    // usage_update, the other does not. The non-reporting lease IS the pre-fix code path
    // (`context_usage == None` selects exactly the old branch), and it is at 99% of a real
    // 1_000_000-token window while its char estimate reads a few dozen tokens.
    //
    // This is the failure that matters: not an early rotation, but a lease that never
    // rotates and then hits the hard limit mid-turn.
    let (path_a, ledger_a) = tmp_ledger("negctl-measured");
    let mut measured = support::spine(ledger_a);
    measured.create_thread("General", &femcboost()).unwrap();
    measured.attach_lease(Box::new(ReportingCognition::new("sess-m", vec![(990_000, 1_000_000)], "ok")));
    measured.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["successor"])));
    measured.submit_prompt("hello", Source::Text).unwrap();
    measured.submit_prompt("next request", Source::Text).unwrap();

    let (path_b, ledger_b) = tmp_ledger("negctl-blind");
    let mut blind = support::spine(ledger_b);
    blind.create_thread("General", &femcboost()).unwrap();
    blind.attach_lease(Box::new(MockCognition::new("sess-b", vec!["ok"])));
    blind.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["successor"])));
    blind.submit_prompt("hello", Source::Text).unwrap();
    blind.submit_prompt("next request", Source::Text).unwrap();

    assert!(measured.rotation_count() >= 1, "with the measurement, it rotates");
    assert_eq!(
        blind.rotation_count(),
        0,
        "WITHOUT the measurement the defect returns: the char estimate reads {} tokens \
         against a 140_000 threshold, so nothing rotates - while the real session it \
         describes could be at 99% of a 1_000_000-token window",
        blind.context_estimate_tokens()
    );
    assert!(
        blind.context_estimate_tokens() < 140_000,
        "the estimate is the thing that never gets there: {}",
        blind.context_estimate_tokens()
    );
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

#[test]
fn a_successor_never_inherits_its_predecessors_measurement() {
    // If it did, every fresh lease would open above the watermark and rotate immediately -
    // rotation as a loop, which is worse than no rotation at all. `install_lease` is the
    // only place `self.lease` is assigned, so clearing there makes this structural.
    let (path, ledger) = tmp_ledger("no-inherit");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    // 0.90 measured: over the 0.70 watermark, UNDER the 0.95 critical threshold, so this
    // rotates for the ordinary reason and the test stays about inheritance.
    spine.attach_lease(Box::new(ReportingCognition::new("sess-1", vec![(900_000, 1_000_000)], "ok")));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["successor reply", "another"])));

    spine.submit_prompt("hello", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "rotation waits for a cancellable request");
    spine.submit_prompt("next request", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1, "the measured watermark rotated once");
    assert_eq!(spine.last_rotation_reason(), Some("context-watermark"));

    // The successor is a MockCognition (never reports), so after the swap the spine must
    // be back on the honest fallback with NO carried-over number.
    assert_eq!(spine.context_usage(), None, "the dead session's number did not survive it");
    assert_eq!(spine.context_source(), ContextSource::Estimated);

    // And the proof that this is not a rotation loop: another turn, still one rotation.
    spine.submit_prompt("again", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1, "a fresh lease must not rotate itself immediately");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_mid_turn_crossing_of_the_hard_limit_is_recorded_and_settled_at_the_boundary_never_inside_it() {
    // The failure the whole change is aimed at: a turn's OWN consumption crossing the
    // limit while it is running. Rotation is forbidden there (continuity §3.1), so what
    // this system does is: finish the turn, write the crossing down against that turn,
    // then rotate at the first legal instant under `context-critical`.
    let (path, ledger) = tmp_ledger("mid-turn-critical");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    // 0.99 configured: an operator ratio that would NOT have rotated at 96%. The critical
    // threshold outranks it, because at 96% measured the policy has been overtaken.
    spine.set_context_budget(1_000_000, 0.99);
    // A UUID-shaped session id on purpose: Action.detail is capped at
    // ACTION_DETAIL_MAX_CHARS (160) and real session ids are 36 chars (a hyphenated uuid,
    // which is what `--session-id` requires), so a short
    // fixture id would hide a truncation that production would hit. The load-bearing
    // clauses must survive the cap with a REAL-LENGTH id in the string.
    spine.attach_lease(Box::new(ReportingCognition::new(
        "55c79b81-ace3-4b07-a5f3-406853ac1a36",
        vec![(500_000, 1_000_000), (960_000, 1_000_000), (970_000, 1_000_000)],
        "the reply still finished",
    )));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["successor reply"])));

    spine.submit_prompt("a turn that eats the window", Source::Text).unwrap();

    // 1. The turn FINISHED. Nothing rotated inside it.
    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.last().unwrap().text, "the reply still finished", "the running turn was never cut short");
    let turn_id = msgs.last().unwrap().turn_id.clone();
    assert_eq!(spine.ledger().turn(&turn_id).unwrap().state, TurnState::Completed);

    // 2. It waits until the next request owns its progress and Stop control.
    assert_eq!(spine.rotation_count(), 0);
    spine.submit_prompt("next request", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1);
    assert_eq!(spine.last_rotation_reason(), Some("context-critical"));

    // 3. The crossing is durable, Internal, and attached to the turn it happened in -
    //    the FIRST crossing (960_000), not the last reading.
    let pressure: Vec<_> =
        spine.ledger().internal_actions().into_iter().filter(|a| a.kind == "context_pressure").collect();
    assert_eq!(pressure.len(), 1, "exactly one crossing per turn, at the first crossing");
    assert_eq!(pressure[0].visibility, ActionVisibility::Internal, "the CEO never sees rotation's cause");
    assert_eq!(pressure[0].turn_id.as_deref(), Some(turn_id.as_str()));
    assert!(
        pressure[0].detail.contains("used=960000"),
        "the FIRST crossing is the honest 'when': {}",
        pressure[0].detail
    );
    assert!(pressure[0].detail.contains("size=1000000"), "detail = {}", pressure[0].detail);
    assert!(
        pressure[0].detail.starts_with("rotation deferred to this boundary"),
        "the outcome must lead, so truncation can never eat it: {}",
        pressure[0].detail
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn staying_under_the_critical_ratio_records_no_pressure_and_no_critical_rotation() {
    // The positive probe for the negative test above: the same machinery, below 0.95,
    // must produce nothing. A guard that fires either way proves nothing.
    let (path, ledger) = tmp_ledger("no-critical");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.set_context_budget(1_000_000, 0.99);
    spine.attach_lease(Box::new(ReportingCognition::new("sess-1", vec![(940_000, 1_000_000)], "ok")));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["successor"])));

    spine.submit_prompt("hello", Source::Text).unwrap();

    // 0.94 < 0.95 critical AND < 0.99 configured -> nothing happens at all.
    assert_eq!(spine.rotation_count(), 0);
    assert!(spine.context_pressure().is_none());
    assert!(spine
        .ledger()
        .internal_actions()
        .into_iter()
        .all(|a| a.kind != "context_pressure"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn mid_turn_pressure_without_a_lease_factory_records_the_fact_and_does_not_fail_a_turn_that_succeeded() {
    // The honest degrade, matching the watermark path's existing one. A spine with no
    // factory cannot rotate, so the crossing is DETECTED and RECORDED and nothing else
    // happens - in particular `submit_prompt` must still return Ok, because the turn it
    // describes completed. Returning NoLeaseFactory here would report a failure for work
    // that succeeded, which is the wrong direction to be wrong in.
    let (path, ledger) = tmp_ledger("pressure-no-factory");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ReportingCognition::new(
        "sess-1",
        vec![(980_000, 1_000_000)],
        "the reply still finished",
    )));
    // Deliberately NO set_lease_factory.

    let result = spine.submit_prompt("hello", Source::Text);
    assert!(result.is_ok(), "a turn that completed must not report a rotation failure: {result:?}");

    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.last().unwrap().text, "the reply still finished");
    assert_eq!(spine.rotation_count(), 0, "nothing to rotate into");

    let pressure: Vec<_> =
        spine.ledger().internal_actions().into_iter().filter(|a| a.kind == "context_pressure").collect();
    assert_eq!(pressure.len(), 1, "the crossing is still written down - detection is the valuable part");
    assert!(
        pressure[0].detail.contains("cannot proceed"),
        "the record must say rotation could not happen, not imply it did: {}",
        pressure[0].detail
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_adapter_that_reports_a_zero_window_falls_back_rather_than_dividing_by_it() {
    // Never observed on the wire; refused anyway, because a NaN watermark would rotate
    // never or always and there is no way to tell which from the outside.
    let (path, ledger) = tmp_ledger("zero-window");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(ReportingCognition::new("sess-1", vec![(500, 0)], "ok")));
    spine.set_context_budget(1000, 0.001); // the fallback WOULD fire

    spine.submit_prompt("hello", Source::Text).unwrap();

    assert!(spine.context_fraction().is_finite(), "no NaN, no inf");
    assert!(spine.watermark_reached(), "a zero window falls back to the estimate, which is over");
    assert_eq!(spine.context_window_tokens(), 1000, "a zero size is not a window");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn watermark_triggers_rotation_automatically_at_the_next_turn_boundary() {
    // continuity §3.2's PRIMARY trigger: the context watermark, not idle/explicit. Set
    // an artificially tiny budget so a single turn's measured context crosses it, then
    // confirm rotation fires WITHOUT any explicit request — proving the scheduled,
    // automatic path (not just the manual one exercised above).
    let (path, ledger) = tmp_ledger("watermark-rotation");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["short reply"])));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["post-rotation reply"])));

    // window=1000 tokens, ratio=0.001 -> threshold = 1 token. Nothing has been sent yet,
    // so the measured estimate is exactly 0 tokens (below threshold) — then the very
    // first turn's re-prime injection alone (dozens of chars) crosses it.
    spine.set_context_budget(1000, 0.001);
    assert_eq!(spine.context_estimate_tokens(), 0, "nothing sent yet");
    assert!(!spine.watermark_reached(), "0 tokens is below a 1-token threshold");

    spine.submit_prompt("hello", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "rotation waits for a cancellable request");
    spine.submit_prompt("next request", Source::Text).unwrap();

    assert_eq!(spine.rotation_count(), 1, "watermark crossing scheduled a rotation automatically");
    assert_eq!(spine.last_rotation_reason(), Some("context-watermark"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn context_estimate_is_measured_not_asserted() {
    // The voice engineer's discipline: show the frame math, don't trust it (precedent:
    // independently re-derive `313 × 256 ÷ 16000 = 5.008s` rather than trusting a
    // comment). Rather than re-deriving the priming text's length by a SEPARATE call
    // (which can drift from what the spine actually computed, since re-prime assembly
    // depends on exact ledger state at injection time), read the EXACT text the spine
    // actually sent — captured by the mock's `reprimes` log — and measure THAT.
    let (path, ledger) = tmp_ledger("context-math");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();

    let reply = "exactly forty chars in this canned reply!!"; // length MEASURED below, not assumed
    let mock = MockCognition::new("sess-1", vec![reply]);
    let reprimes = mock.reprimes.clone();
    spine.attach_lease(Box::new(mock));
    let prompt_text = "hi";
    spine.submit_prompt(prompt_text, Source::Text).unwrap();

    let sent_priming_text = reprimes.lock().unwrap()[0].clone();

    // MEASURED formula (spine.rs `deliver`/`prime_lease_if_needed`): context_chars =
    // len(priming text ACTUALLY injected) + len(prompt) + len(reply). Token estimate =
    // chars / 4 (CHARS_PER_TOKEN_ESTIMATE — an estimate, shown not asserted).
    let expected_chars = sent_priming_text.len() + prompt_text.len() + reply.len();
    let expected_tokens = expected_chars / 4;

    assert_eq!(
        spine.context_estimate_tokens(),
        expected_tokens,
        "measured token estimate must equal chars/4 over EXACTLY (the priming text actually \
         sent + prompt + reply) — priming_len={}, prompt_len={}, reply_len={}",
        sent_priming_text.len(),
        prompt_text.len(),
        reply.len()
    );

    // And the load-bearing claim this whole mechanism exists for: the estimate is NEVER
    // just "this turn's prompt+reply" — it must include the re-prime payload too, or the
    // watermark would systematically under-count and rotation would fire too late.
    let naive_wrong_estimate = (prompt_text.len() + reply.len()) / 4;
    assert!(
        spine.context_estimate_tokens() > naive_wrong_estimate,
        "context estimate must include the re-prime payload, not just this turn's prompt+reply \
         (got {} tokens, naive-wrong estimate would be {naive_wrong_estimate})",
        spine.context_estimate_tokens()
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rotation_re_primes_the_successor_with_identity_and_the_action_ledger() {
    // The mechanism behind done-criterion (b), "no false attribution": EVERY successor
    // is re-primed BEFORE its first CEO-visible turn, and the re-prime text carries the
    // action ledger as ground truth. Verify this lands on the ACTUAL spawned successor
    // lease (via MockLeaseFactory's per-spawn reprime log), not just on the payload
    // object in isolation.
    let (path, mut ledger) = tmp_ledger("attribution-rotation");
    let thread = ledger.create_thread("General", &femcboost()).unwrap();
    let turn = ledger.record_prompt_received(&ledger.thread_binding(&thread).unwrap(), "dispatch a worker", Source::Text).unwrap();
    ledger.record_action(&turn, "dispatch", "spawned worker mark-sonnet-f1").unwrap();

    let mut spine = support::spine(ledger);
    spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-initial", vec!["ack"])));
    let factory = MockLeaseFactory::new(vec!["ack2"]);
    let spawned_reprimes = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.request_rotation("test").unwrap();
    spine.submit_prompt("continue", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1);

    let per_spawn_reprimes = spawned_reprimes.lock().unwrap();
    assert_eq!(per_spawn_reprimes.len(), 1, "exactly one successor lease was spawned");
    let successor_reprimes = per_spawn_reprimes[0].lock().unwrap();
    assert_eq!(successor_reprimes.len(), 1, "the successor was re-primed exactly once, before any CEO turn");
    let priming_text = &successor_reprimes[0];
    // WHO Rich is, and the no-denial-from-absent-memory rule, moved OUT of the priming turn on
    // 2026-09-06 and into the standing instruction delivered as a system prompt
    // (`doctrine.rs`; inner-doctrine design §4.2, §5.1). Both are checked in their new home by
    // `action_ledger_tests::the_ledgers_partial_coverage_is_stated_rather_than_overclaimed`.
    // What the TURN still has to carry is what a prompt fixed at spawn cannot: which
    // conversation this is, and a pointer to the ledger printed below it.
    assert!(priming_text.contains(&format!("continuing conversation {thread}")), "the successor is told which conversation it is in: {priming_text}");
    assert!(priming_text.contains("ground truth for the actions it records"), "the ledger pointer is present");
    assert!(priming_text.contains("never mis-attribute your own prior actions"), "anti-false-attribution rule present");
    assert!(
        priming_text.contains("spawned worker mark-sonnet-f1"),
        "the action ledger (ground truth) is in the successor's re-prime, not just the predecessor's memory"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn clean_rotation_asks_the_outgoing_lease_for_a_self_authored_handoff_summary() {
    // continuity §2.4: "before tearing down the outgoing session at a turn boundary, the
    // app asks it for a self-authored handoff summary" — one cheap INTERNAL turn, never
    // rendered. Confirm the outgoing lease was prompted, the summary text is what the
    // successor's re-prime carries (via handoff_summary upgrading rolling_summary), and
    // the internal ask never appears in the CEO-visible conversation.
    let (path, ledger) = tmp_ledger("handoff-summary");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    let outgoing = MockCognition::new("sess-outgoing", vec!["the CEO's own reply", "We discussed the Acme deal and Q4 hiring."]);
    let outgoing_prompts = outgoing.prompts.clone();
    spine.attach_lease(Box::new(outgoing));
    let factory = MockLeaseFactory::new(vec!["ack from successor"]);
    spine.set_lease_factory(Box::new(factory));

    spine.submit_prompt("what's the Acme status?", Source::Text).unwrap();
    spine.request_rotation("test").unwrap();
    spine.submit_prompt("continue", Source::Text).unwrap();

    // The outgoing lease was asked TWO things: the CEO's real turn, then the internal
    // handoff-summary request (in that order).
    let prompts = outgoing_prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    let event: serde_json::Value = serde_json::from_str(&prompts[1]).unwrap();
    assert_eq!(event["event"], "session_handoff_requested");

    // The internal ask + its reply are durable (ledger.turns()) but NEVER rendered.
    let msgs = spine.messages(&thread).unwrap();
    assert!(msgs.iter().all(|m| !m.text.contains("We discussed the Acme deal")), "internal handoff turn is not CEO-visible");
    assert!(spine
        .ledger()
        .turns()
        .iter()
        .any(|t| t.source == Source::Internal && t.assistant_text.contains("We discussed the Acme deal")));

    // And it became the ROLLING SUMMARY the successor carries forward (continuity §2.4:
    // "upgrade to a self-authored summary on clean rotation").
    assert_eq!(spine.ledger().handoff_summary(&thread), Some("We discussed the Acme deal and Q4 hiring."));
    let _ = std::fs::remove_file(&path);
}

// ============================================================================
// Mid-turn-crash recovery (continuity §5, done-criterion (c))
// ============================================================================

#[test]
fn mid_turn_crash_recovers_and_replays_without_duplicating_the_message() {
    // The child dies mid-turn (a positive signal — a partial chunk then Err, never
    // inferred from silence). A lease factory is attached, so the spine automatically
    // respawns, re-primes, and RE-SERVES the CEO's original prompt — done-criterion (c),
    // "the CEO's prompt is provably never lost." Also verifies the render stays CLEAN:
    // one exchange, not a duplicate of the failed attempt.
    let (path, ledger) = tmp_ledger("crash-recovery");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-doomed".into() }));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["I'm back — here's your answer."])));

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    // submit_prompt is TRANSPARENT: the crash + recovery happen inside this ONE call,
    // and it still returns Ok because the replay succeeded.
    let result = spine.submit_prompt("what's my Q3 revenue?", Source::Text);
    assert!(result.is_ok(), "recovery succeeded, so the CEO's call never surfaces a hard error");

    assert_eq!(spine.rotation_count(), 1);
    assert_eq!(spine.last_rotation_reason(), Some("mid-turn-crash"));
    assert!(!spine.is_turn_in_progress(), "turn boundary is clear again after recovery");

    // CLEAN OUTPUT: exactly ONE user+assistant pair, not two — the failed attempt is
    // superseded, not re-shown.
    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.len(), 2, "no duplicate exchange from the failed attempt");
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "what's my Q3 revenue?");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "I'm back — here's your answer.");

    // But the CRASH RECORD is still durable in the raw ledger (never edited in place) —
    // "provably never lost" means provable, not just asserted.
    let raw_turns = spine.ledger().turns();
    let failed = raw_turns
        .iter()
        .find(|t| t.user_text == "what's my Q3 revenue?" && t.state == TurnState::Interrupted)
        .expect("the interrupted attempt is still in the durable ledger");
    assert_eq!(failed.assistant_text, "partial before the lease dies");
    assert!(failed.superseded_by.is_some(), "marked superseded, not deleted");
    let replay = raw_turns.iter().find(|t| t.id == *failed.superseded_by.as_ref().unwrap()).unwrap();
    assert_eq!(replay.state, TurnState::Completed);
    assert_eq!(replay.assistant_text, "I'm back — here's your answer.");

    // A calm reconnect cue was ALSO emitted (the UI's job — main.js renders this as an
    // ephemeral "lost my train of thought" bubble, never a stack trace).
    assert!(observer.events().iter().any(|e| matches!(e, StreamEvent::TurnError { .. })));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn mid_turn_crash_without_a_lease_factory_degrades_to_an_honest_error() {
    // No factory attached => no way to respawn => the failure surfaces honestly rather
    // than silently vanishing or hanging. (The always-there floor beneath automatic
    // recovery.)
    let (path, ledger) = tmp_ledger("crash-no-factory");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-doomed".into() }));
    // Deliberately NOT calling set_lease_factory.

    let result = spine.submit_prompt("hello", Source::Text);
    assert!(result.is_err(), "with no recovery path, the crash must surface, never be swallowed");
    assert_eq!(spine.rotation_count(), 0, "no recovery was attempted without a factory");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn mid_turn_crash_recovery_is_bounded_to_one_attempt() {
    // If the FRESH lease ALSO dies immediately, the spine must not loop forever — one
    // recovery attempt, then an honest failure (positive-signal doctrine: never infer,
    // never spin).
    let (path, ledger) = tmp_ledger("crash-loop-bound");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-doomed-1".into() }));
    let spawn_count = Arc::new(Mutex::new(0u64));
    spine.set_lease_factory(Box::new(AlwaysFailingLeaseFactory { spawn_count: spawn_count.clone() }));

    let result = spine.submit_prompt("hello", Source::Text);
    assert!(result.is_err(), "the replay also failed, so this must surface, not hang");
    assert_eq!(*spawn_count.lock().unwrap(), 1, "exactly ONE respawn was attempted, not an infinite loop");
    assert_eq!(spine.rotation_count(), 1, "the one attempted recovery is still recorded");
    let _ = std::fs::remove_file(&path);
}

/// A lease that refuses the way a NOT-SIGNED-IN machine does, counting every prompt it is
/// handed. The string is the vendor's own constant, read verbatim out of the installed
/// Claude Code bundle and wrapped in the `CognitionError` `Display` `native.rs` produces.
struct RefusingCognition {
    session_id: String,
    prompts: Arc<Mutex<Vec<String>>>,
}
impl Cognition for RefusingCognition {
    fn session_id(&self) -> &str {
        &self.session_id
    }
    fn reprime(&mut self, _priming_text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
    }
    fn prompt(&mut self, text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push(text.to_string());
        Err(CognitionError::Protocol("\"Not logged in · Please run /login\"".into()))
    }
}

/// A factory whose leases all refuse the same way, sharing one prompt counter with the
/// lease that was already attached — so "how many requests did one Enter produce?" is a
/// single number rather than a sum somebody has to assemble.
struct RefusingLeaseFactory {
    prompts: Arc<Mutex<Vec<String>>>,
    spawns: Arc<Mutex<u64>>,
}
impl LeaseFactory for RefusingLeaseFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let mut n = self.spawns.lock().unwrap();
        *n += 1;
        Ok(Box::new(RefusingCognition {
            session_id: format!("sess-refusing-{n}"),
            prompts: self.prompts.clone(),
        }))
    }
}

/// **ONE PRESS OF ENTER IS ONE REQUEST** — the dev walk's N4
/// (`docs/verification/2026-09-17-main-aa0165cc-dev-walk-audit.md`).
///
/// Its evidence is the app's own ledger, from a single keystroke:
///
/// ```text
/// PromptReceived   turn_7ba34774…  "What is 17 times 3? …"   at …297651
/// TurnInterrupted  turn_7ba34774…  "cognition io: …"         at …297664
/// ActionRecorded   act_fa1397b0…   crash_recovery
/// PromptReceived   turn_0cc5570d…  "What is 17 times 3? …"   at …297673
/// ```
///
/// Two prompts, 22 ms apart, with identical text. On that walk both were refused locally so
/// nothing was charged; on a build that reaches the model it is two requests for one
/// question, and against a permanent condition the replay can never succeed and he pays for
/// it anyway. The audit's own sentence is the rule: *"An immediate automatic replay is the
/// right instinct for a crash and the wrong one for a refusal the app has already read."*
///
/// The discriminator is the interruption classification, which the spine now writes at the
/// failure boundary — `offers_retry` is already the answer to "can asking again help?", and
/// it is the same fact that decides whether a retry control is offered to HIM. The app
/// cannot coherently call a retry pointless and then perform one itself without asking.
#[test]
fn a_refusal_the_app_has_already_read_is_never_replayed_automatically() {
    let (path, ledger) = tmp_ledger("n4-no-double-submit");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawns = Arc::new(Mutex::new(0u64));
    spine.attach_lease(Box::new(RefusingCognition {
        session_id: "sess-refusing-0".into(),
        prompts: prompts.clone(),
    }));
    spine.set_lease_factory(Box::new(RefusingLeaseFactory {
        prompts: prompts.clone(),
        spawns: spawns.clone(),
    }));

    let result = spine.submit_prompt("What is 17 times 3?", Source::Text);
    assert!(result.is_err(), "the turn failed, and that must surface");

    // THE WHOLE OF N4, IN ONE NUMBER.
    let sent = prompts.lock().unwrap().clone();
    assert_eq!(
        sent,
        vec!["What is 17 times 3?".to_string()],
        "one press of Enter produced {} request(s) — N4 is back",
        sent.len()
    );
    assert_eq!(*spawns.lock().unwrap(), 0, "no lease was respawned for a refusal");
    assert_eq!(spine.rotation_count(), 0, "and no recovery was recorded");

    // ONE TURN ON DISK, not a turn plus its superseding replay.
    let ceo_turns: Vec<_> =
        spine.ledger().turns().iter().filter(|t| t.source != Source::Internal).collect();
    assert_eq!(ceo_turns.len(), 1, "a second turn was journaled: {ceo_turns:?}");
    assert_eq!(ceo_turns[0].superseded_by, None, "the turn was superseded by a replay");
    // And the reason it was not replayed is recorded, not merely acted on.
    let cause = ceo_turns[0].interruption.as_ref().expect("the cause is recorded");
    assert_eq!(cause.cause, "not-signed-in");
    assert!(!cause.offers_retry, "this class must offer no retry, to him or to itself");
    let _ = std::fs::remove_file(&path);
}

/// **THE POSITIVE CONTROL FOR N4, and the suite is worthless without it.**
///
/// `a_refusal_…_is_never_replayed_automatically` would pass identically if crash recovery
/// had simply been switched off, which would delete continuity §5.3 rather than fix N4. This
/// asserts the other half on the same day: a CRASHED child — the case the seam exists for —
/// still gets its one silent respawn and replay, and the CEO's question is asked again
/// exactly once.
#[test]
fn a_crashed_lease_is_still_replayed_exactly_once() {
    let (path, ledger) = tmp_ledger("n4-crash-still-recovers");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-doomed-1".into() }));
    let spawn_count = Arc::new(Mutex::new(0u64));
    spine.set_lease_factory(Box::new(AlwaysFailingLeaseFactory { spawn_count: spawn_count.clone() }));

    let result = spine.submit_prompt("What is 17 times 3?", Source::Text);
    assert!(result.is_err(), "the replay also failed, so this must surface");
    assert_eq!(
        *spawn_count.lock().unwrap(),
        1,
        "a crashed child must still be recovered — N4's fix must not have deleted §5.3"
    );
    assert_eq!(spine.rotation_count(), 1, "and the recovery is recorded");
    let first = spine
        .ledger()
        .turns()
        .iter()
        .find(|t| t.source != Source::Internal)
        .expect("a CEO turn")
        .clone();
    assert_eq!(first.interruption.as_ref().map(|c| c.cause.clone()), Some("transient".into()));
    let _ = std::fs::remove_file(&path);
}

// ============================================================================
// The proactive-attention seam (persistence + UI event; judgment is a LATER leg)
// ============================================================================

#[test]
fn proactive_tier1_and_tier2_render_as_rich_only_messages_no_preceding_ceo_prompt() {
    let (path, ledger) = tmp_ledger("proactive-render");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    spine
        .raise_proactive(None, AttentionTier::InterruptNow, "The Acme counter expires at noon — what's your floor?")
        .unwrap();
    spine
        .raise_proactive(None, AttentionTier::Digest, "Morning brief: three things need your attention.")
        .unwrap();

    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.len(), 2, "both tiers render; NO paired user message (nothing was prompted)");
    assert!(msgs.iter().all(|m| m.role == "assistant"));
    assert_eq!(msgs[0].text, "The Acme counter expires at noon — what's your floor?");
    assert_eq!(msgs[1].text, "Morning brief: three things need your attention.");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn proactive_silent_tier_never_renders_but_stays_durable() {
    // UX §5.1 Tier 3: "Does not appear in the conversation and never notifies." But it
    // must still be durably logged (a CEO who goes looking, or a future activity view).
    let (path, ledger) = tmp_ledger("proactive-silent");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    spine.raise_proactive(None, AttentionTier::Silent, "FYI: renewed the Acme NDA, nothing needed from you.").unwrap();

    assert!(spine.messages(&thread).unwrap().is_empty(), "Silent tier has NO render path");
    assert!(observer.events().is_empty(), "Silent tier never notifies — no live UI event either");
    // But it's not lost — durable in the raw ledger.
    assert!(spine
        .ledger()
        .turns()
        .iter()
        .any(|t| t.assistant_text.contains("renewed the Acme NDA")));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn proactive_message_raised_mid_turn_is_durable_and_shown_at_once() {
    // Round 16 draws Rich's line beside the "working" row, so a message raised while a turn
    // is in flight is written AND sent to the window at once, exactly once — the next turn
    // boundary does not send it again. This spine is fully synchronous/single-threaded
    // (module doc), so the test uses the documented test-only seam to force the in-flight
    // state deterministically.
    let (path, ledger) = tmp_ledger("proactive-mid-turn");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["a reply"])));

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    spine.debug_set_turn_in_progress(true);
    let proactive_turn_id = spine.raise_proactive(None, AttentionTier::Digest, "said while busy").unwrap();
    let sent = |events: &[StreamEvent]| events.iter().filter(|e| matches!(
        e, StreamEvent::ProactiveMessage { turn_id, .. } if turn_id == &proactive_turn_id)).count();
    assert_eq!(sent(&observer.events()), 1, "sent to the window while the turn is (simulated) in flight");
    assert_eq!(spine.messages(&thread).unwrap().len(), 1, "the message is already durable/readable");
    spine.debug_set_turn_in_progress(false);

    // A REAL turn boundary does not send it a second time.
    spine.submit_prompt("hi", Source::Text).unwrap();
    assert_eq!(sent(&observer.events()), 1, "the boundary sent the message again");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn proactive_message_defaults_to_active_thread_when_none_given() {
    let (path, ledger) = tmp_ledger("proactive-default-thread");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.raise_proactive(None, AttentionTier::Digest, "hello").unwrap();
    assert_eq!(spine.messages(&thread).unwrap().len(), 1);
    let _ = std::fs::remove_file(&path);
}

/// A lease whose supervisor reports tool commands running outside its provider's group, as a
/// product lease's state file does (`lease_commands.rs`).
struct LeaseWithCommands {
    inner: MockCognition,
    commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>>,
}
impl Cognition for LeaseWithCommands {
    fn session_id(&self) -> &str {
        self.inner.session_id()
    }
    fn reprime(&mut self, priming_text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.inner.reprime(priming_text, on_item)
    }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.inner.prompt(text, on_item)
    }
    fn running_commands(&self) -> Option<richos_core::lease_commands::CommandReading> {
        *self.commands.lock().unwrap()
    }
}

/// **The product reap gap design C6, on the conversation.** Retiring a lease now ends its tool
/// commands, and a renewal is invisible by his standing order, so a watermark renewal waits
/// while the front desk has a command running (or its state cannot be read), and happens at
/// the first request after it ends. An explicit rotation is not held.
#[test]
fn a_watermark_renewal_waits_for_a_command_the_front_desk_started() {
    use richos_core::lease_commands::CommandReading;
    let (path, ledger) = tmp_ledger("renewal-waits-for-commands");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let commands = Arc::new(Mutex::new(Some(CommandReading::Running(1))));
    spine.attach_lease(Box::new(LeaseWithCommands {
        inner: MockCognition::new("sess-with-a-server", vec!["started it", "still here", "still here"]),
        commands: commands.clone(),
    }));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["on the successor", "on the successor"])));

    spine.submit_prompt("start the dev server", Source::Text).unwrap();
    spine.request_rotation("context-watermark").unwrap();
    spine.submit_prompt("is it up?", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "a renewal ended a command the front desk had started");
    *commands.lock().unwrap() = Some(CommandReading::Unreadable);
    spine.submit_prompt("and now?", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "an unreadable state was read as nothing running");

    *commands.lock().unwrap() = Some(CommandReading::Clear);
    spine.submit_prompt("it is done", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1, "the renewal never happened once the command ended");
    assert_eq!(spine.last_rotation_reason(), Some("context-watermark"));
    std::fs::remove_file(&path).ok();
}

#[test]
fn an_explicit_rotation_is_not_held_by_a_running_command() {
    use richos_core::lease_commands::CommandReading;
    let (path, ledger) = tmp_ledger("explicit-rotation-not-held");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(LeaseWithCommands {
        inner: MockCognition::new("sess-busy", vec!["first"]),
        commands: Arc::new(Mutex::new(Some(CommandReading::Running(2)))),
    }));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["second"])));
    spine.submit_prompt("one", Source::Text).unwrap();
    spine.request_rotation("test-forced").unwrap();
    spine.submit_prompt("two", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1);
    std::fs::remove_file(&path).ok();
}

#[test]
fn phone_answer_provenance_survives_a_lease_crash_without_changing_product_retention() {
    struct Capture {
        inner: Box<dyn Cognition>,
        channels: Arc<Mutex<Vec<Option<String>>>>,
    }
    impl Cognition for Capture {
        fn session_id(&self) -> &str { self.inner.session_id() }
        fn reprime(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
            self.inner.reprime(text,on_item)
        }
        fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String,CognitionError> {
            self.inner.prompt(text,on_item)
        }
        fn set_input_channel(&mut self, channel: Option<&str>) -> Result<(),CognitionError> {
            self.channels.lock().unwrap().push(channel.map(str::to_string)); Ok(())
        }
    }
    struct Factory(Arc<Mutex<Vec<Option<String>>>>);
    impl LeaseFactory for Factory {
        fn spawn(&self) -> Result<Box<dyn Cognition>,CognitionError> {
            Ok(Box::new(Capture { inner:Box::new(MockCognition::new("recovered",vec!["Got it", "Next reply"])),channels:self.0.clone() }))
        }
    }
    for keep in [false,true] {
        for channel in ["phone_typed","phone_voice"] {
            let (path, ledger)=tmp_ledger(&format!("phone-replay-{keep}-{channel}"));
            let mut spine=support::spine(ledger);
            let thread=spine.create_thread("General",&femcboost()).unwrap();
            spine.keep_intake_channel(keep);
            let channels=Arc::new(Mutex::new(Vec::new()));
            spine.attach_lease(Box::new(Capture {inner:Box::new(FailingCognition {session_id:"doomed".into()}),channels:channels.clone()}));
            spine.set_lease_factory(Box::new(Factory(channels.clone())));
            let intake=path.with_extension("intake");
            let control=richos_core::steering::TurnControl::open(&intake).unwrap();
            spine.set_turn_control(control.clone());
            control.submit_from_channel(&thread,Some(femcboost()),"Choose tomorrow",channel).unwrap();
            spine.poll_intake().unwrap();
            assert_eq!(*channels.lock().unwrap(),vec![Some(channel.into()),Some(channel.into())]);
            let binding=spine.ledger().thread_binding(&thread).unwrap();
            for turn in spine.ledger().thread_turns_scoped(&binding).unwrap().into_iter().filter(|t|t.source==Source::Text) {
                assert_eq!(turn.channel.as_deref(),keep.then_some(channel));
            }
            spine.submit_prompt("Next request from this Mac",Source::Text).unwrap();
            assert_eq!(channels.lock().unwrap().last().cloned(),Some(keep.then(|| "desk".to_string())));
            drop(spine);
            drop(std::fs::remove_file(path));
            drop(std::fs::remove_file(intake));
        }
    }
}

// ============================================================================
// Fill-first: several Claude accounts (plan richos-hq 2026-10-04 §15, ruling §108)
// ============================================================================

/// A lease that says which account it runs under and what usage its child streamed, the two
/// things `NativeCognition` reports on the real wire. `fail_with` makes its turn end the way a
/// usage-limit refusal does (`native.rs`, `usage_limit_signal`).
struct AccountLease {
    session_id: String,
    account: String,
    streamed: Arc<Mutex<Option<richos_core::quota::StreamedReading>>>,
    fail_with: Option<String>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
}
impl Cognition for AccountLease {
    fn session_id(&self) -> &str { &self.session_id }
    fn account(&self) -> Option<&str> { Some(&self.account) }
    fn streamed_usage(&self) -> Option<richos_core::quota::StreamedReading> { self.streamed.lock().unwrap().clone() }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push((self.account.clone(), text.to_string()));
        if let Some(error) = &self.fail_with {
            on_item(TurnItem::Text { seq: 0, text: "Partial answer before the limit" });
            return Err(CognitionError::Protocol(error.clone()));
        }
        on_item(TurnItem::Text { seq: 0, text: &format!("answered on account {}", self.account) });
        Ok("end_turn".into())
    }
}

/// The lease factory the desktop shell is, reduced to the fill-first part: every lease is
/// spawned under the account the quota service says is in use (`main.rs`, `spawn_chat`).
struct AccountFactory {
    quota: Arc<richos_core::quota::Service>,
    spawned: Arc<Mutex<Vec<String>>>,
    streamed: Arc<Mutex<Option<richos_core::quota::StreamedReading>>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
}
impl LeaseFactory for AccountFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let account = self.quota.lease_account().id;
        let mut spawned = self.spawned.lock().unwrap();
        spawned.push(account.clone());
        Ok(Box::new(AccountLease {
            session_id: format!("sess-account-{account}-{}", spawned.len()),
            account,
            streamed: self.streamed.clone(),
            fail_with: None,
            prompts: self.prompts.clone(),
        }))
    }
    fn quota(&self) -> Option<Arc<richos_core::quota::Service>> { Some(self.quota.clone()) }
}

/// A quota service with Account 1 and "Work", both read once by a fake `claude` (the
/// `quota/probe.rs` style): Account 1's five-hour window at `one_five`%, Work's at 5%.
fn two_accounts(tag: &str, one_five: f64) -> (std::path::PathBuf, Arc<richos_core::quota::Service>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("richos-fill-first-{tag}-{}-{}", std::process::id(), richos_core::util::now_millis()));
    std::fs::create_dir_all(&dir).unwrap();
    let quota = Arc::new(richos_core::quota::Service::open(&dir).unwrap());
    quota.accounts.add("Work").unwrap();
    let bin = dir.join("claude-fixture");
    std::fs::write(&bin, format!(r#"#!/usr/bin/env python3
import json, os, sys
folder = os.environ.get("CLAUDE_CONFIG_DIR", "")
five = 5 if "claude-accounts" in folder else {one_five}
for line in sys.stdin:
    v = json.loads(line)
    payload = {{}} if v["request"]["subtype"] == "initialize" else {{"rate_limits_available": True, "rate_limits": {{
        "five_hour": {{"utilization": five, "resets_at": "2099-01-01T00:00:00Z"}},
        "seven_day": {{"utilization": 40, "resets_at": "2099-01-05T00:00:00Z"}}}}}}
    print(json.dumps({{"type": "control_response", "response": {{"subtype": "success", "request_id": v["request_id"], "response": payload}}}}), flush=True)
"#)).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
    quota.refresh(&bin, true);
    (dir, quota)
}

/// 2099-01-01T00:00:00Z, the fake's five-hour reset, in ms — so a streamed reading is the same
/// window as the probe's.
const FIVE_HOUR_RESET: u64 = 4_070_908_800_000;
fn streamed_five_hour(used: f64, at: u64) -> richos_core::quota::StreamedReading {
    (vec![richos_core::quota::Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: used,
        resets_at: Some(FIVE_HOUR_RESET), duration_ms: 5 * 3_600_000 }], at)
}

/// **A turn that starts at 93% runs under the next account** (the CEO's correction and
/// ruling §108; setting on Switch). The probe read Account 1 at 60%; then the lease's OWN
/// streamed reading says 93%, newer than the probe. Before the next turn the spine decides on
/// that freshest reading, rotates at the boundary, and the turn is answered under Work. One
/// line in the conversation says so.
#[test]
fn a_turn_that_starts_at_93_percent_runs_under_the_next_account() {
    let (dir, quota) = two_accounts("boundary", 60.);
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    assert_eq!(quota.lease_account().id, "1", "60% has room: Account 1 stays");
    let (path, ledger) = tmp_ledger("fill-first-boundary");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: spawned.clone(),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));

    spine.submit_prompt("What is on my plate?", Source::Text).unwrap();

    assert_eq!(*spawned.lock().unwrap(), vec!["2".to_string()], "the successor was spawned under Work");
    assert_eq!(spine.last_rotation_reason(), Some(richos_core::spine::ACCOUNT_SWITCH));
    let his: Vec<_> = prompts.lock().unwrap().iter().filter(|(_, text)| text == "What is on my plate?").cloned().collect();
    assert_eq!(his, vec![("2".to_string(), "What is on my plate?".to_string())], "his turn ran ONLY under Work");
    let msgs = spine.messages(&thread).unwrap();
    assert!(msgs.iter().any(|m| m.text.starts_with("I switched the team to your **Work** account. Account 1 had used 93% of its 5-hour limit")), "{msgs:?}");
    assert!(msgs.iter().any(|m| m.text == "answered on account 2"));
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// **Hunt part 1 v3, finding 50: a switch happens only at a boundary where nothing is
/// running.** Retiring a lease ends the tool commands it started (`lease_commands.rs`), so
/// the conversation, like the work host (`work_host.rs`, `account_switch_due`), does not
/// leave its account while its lease reports a command running or cannot say. The turn runs
/// on the account it is on (the limit backstop covers it); the switch happens at the first
/// boundary after the command ends.
/// An [`AccountLease`] whose supervisor reports the commands it started as the test says.
struct Busy { lease: AccountLease, commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>> }
impl Cognition for Busy {
    fn session_id(&self) -> &str { self.lease.session_id() }
    fn account(&self) -> Option<&str> { self.lease.account() }
    fn streamed_usage(&self) -> Option<richos_core::quota::StreamedReading> { self.lease.streamed_usage() }
    fn running_commands(&self) -> Option<richos_core::lease_commands::CommandReading> { *self.commands.lock().unwrap() }
    fn reprime(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { self.lease.reprime(text, on_item) }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> { self.lease.prompt(text, on_item) }
}

#[test]
fn an_account_switch_waits_for_a_command_the_conversation_started() {
    use richos_core::lease_commands::CommandReading;
    let (dir, quota) = two_accounts("switch-waits", 60.);
    // The five-hour switch acts only while the automatic switch is on (e1ac24d27).
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("fill-first-switch-waits");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Some(CommandReading::Running(1))));
    spine.attach_lease(Box::new(Busy {
        lease: AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
            streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() },
        commands: commands.clone(),
    }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: spawned.clone(),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));

    spine.submit_prompt("Is the build done?", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "an account switch retired a lease with a command running");
    assert!(spawned.lock().unwrap().is_empty());
    *commands.lock().unwrap() = Some(CommandReading::Unreadable);
    spine.submit_prompt("And now?", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "an unreadable state was read as nothing running");

    *commands.lock().unwrap() = Some(CommandReading::Clear);
    spine.submit_prompt("It finished", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1, "the switch never happened once the command ended");
    assert_eq!(spine.last_rotation_reason(), Some(richos_core::spine::ACCOUNT_SWITCH));
    assert_eq!(*spawned.lock().unwrap(), vec!["2".to_string()]);
    let ran: Vec<_> = prompts.lock().unwrap().iter().filter(|(_, t)| !t.is_empty()).cloned().collect();
    assert!(ran.contains(&("1".to_string(), "Is the build done?".to_string())), "{ran:?}");
    assert!(ran.contains(&("2".to_string(), "It finished".to_string())), "{ran:?}");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// **"Switched to" only once the switch has happened.** The switch is decided (Account 1's
/// lease streams 93% of its five-hour window, setting on Switch) while the lease has a
/// command running, so the turn stays on Account 1 (finding 50). While that switch waits, the
/// conversation must not say "Switched to Work": not at that turn, not from the desktop's
/// quota monitor (`raise_quota_notices(None)`). Once the command ends, the next turn runs
/// under Work and the notice is said, once.
///
/// RED on main at `442b60f9e`: the notice was written the moment the switch was decided, so
/// it was in the conversation while his turn was still running on Account 1.
#[test]
fn the_switch_notice_waits_until_a_turn_really_runs_under_the_new_account() {
    use richos_core::lease_commands::CommandReading;
    // Round 18's words for the switch, held until a turn runs under Work. The rest of the line
    // says when Account 1 is fresh again, which moves with the clock.
    const NOTICE: &str = "I switched the team to your **Work** account. Account 1 had used 93% of its 5-hour limit";
    let (dir, quota) = two_accounts("notice-waits", 60.);
    // The five-hour switch acts only while the automatic switch is on (e1ac24d27).
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("fill-first-notice-waits");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Some(CommandReading::Running(1))));
    spine.attach_lease(Box::new(Busy {
        lease: AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
            streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() },
        commands: commands.clone(),
    }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: Arc::new(Mutex::new(Vec::new())),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));
    let notices = |spine: &richos_core::spine::Spine| spine.messages(&thread).unwrap().iter()
        .filter(|m| m.text.starts_with("I switched the team to")).map(|m| m.text.clone()).collect::<Vec<_>>();

    // ---- the switch is decided, and waits: the turn runs on Account 1 -------------------
    spine.submit_prompt("Is the build done?", Source::Text).unwrap();
    assert_eq!(quota.lease_account().id, "2", "the switch was decided");
    assert_eq!(spine.rotation_count(), 0, "the switch is still waiting on the command");
    assert!(prompts.lock().unwrap().contains(&("1".to_string(), "Is the build done?".to_string())));
    assert_eq!(notices(&spine), Vec::<String>::new(), "\"Switched to\" was said while the turn ran on Account 1");
    // The desktop's quota monitor says pending notices whenever the spine is free.
    spine.raise_quota_notices(None);
    assert_eq!(notices(&spine), Vec::<String>::new(), "the monitor said \"Switched to\" before the switch happened");

    // ---- the command ends: the next turn runs under Work, and the notice is said --------
    *commands.lock().unwrap() = Some(CommandReading::Clear);
    spine.submit_prompt("It finished", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1);
    assert!(prompts.lock().unwrap().contains(&("2".to_string(), "It finished".to_string())));
    assert_eq!(notices(&spine).iter().map(|t| t.starts_with(NOTICE)).collect::<Vec<_>>(), vec![true], "said once, once the turn ran under Work");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// **"Switched to" is drawn while the turn on the new account is still running** (round 16,
/// state `switched-line`: Rich's switch line, then "• 3 agents working on Work", a row that
/// exists only during a turn). The notice is raised inside his turn (`prepare_request`, once
/// the lease on Work is in the chair), so the window's signal to draw it,
/// `rich://proactive-message`, must leave the spine during that turn: after his turn started
/// (never before a turn runs on Work, `f2d3ab1c4`) and before it completed.
///
/// RED on `40de1962f` (main's deferral): the notice's event was held to the turn's boundary
/// and left after `turn-completed`, so the line appeared only once the turn was over.
#[test]
fn the_switch_notice_is_drawn_while_the_turn_on_the_new_account_runs() {
    let (dir, quota) = two_accounts("notice-during-turn", 60.);
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("fill-first-notice-during-turn");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: Arc::new(Mutex::new(Vec::new())),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));
    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    spine.submit_prompt("What is on my plate?", Source::Text).unwrap();

    assert!(prompts.lock().unwrap().contains(&("2".to_string(), "What is on my plate?".to_string())), "his turn ran under Work");
    let turns = spine.ledger().turns();
    let his = turns.iter().find(|t| t.user_text == "What is on my plate?").expect("his turn").id.clone();
    let notice = turns.iter().find(|t| t.assistant_text.starts_with("I switched the team to your **Work** account")).expect("the switch notice").id.clone();
    let events = observer.events();
    let at = |wanted: &dyn Fn(&StreamEvent) -> bool| events.iter().position(wanted);
    let started = at(&|e| matches!(e, StreamEvent::TurnStarted { turn_id, .. } if *turn_id == his)).expect("his turn started");
    let completed = at(&|e| matches!(e, StreamEvent::TurnCompleted { turn_id, .. } if *turn_id == his)).expect("his turn completed");
    let drawn = at(&|e| matches!(e, StreamEvent::ProactiveMessage { turn_id, .. } if *turn_id == notice))
        .expect("the switch notice was never sent to the window");
    assert!(started < drawn, "the switch line was sent before a turn ran on Work: {events:?}");
    assert!(drawn < completed, "the switch line was held until his turn ended: {events:?}");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// **The backstop, only for one turn that by itself used up what remained.** The lease on
/// Account 1 is refused mid-turn with the error `native.rs` writes for any of the three real
/// signals. The account is marked gone, Work is put in use, and his prompt is re-served on a
/// fresh lease under Work: one clean exchange, the failed turn superseded and kept on disk.
#[test]
fn a_turn_cut_by_a_usage_limit_is_re_served_under_the_next_account() {
    let (dir, quota) = two_accounts("backstop", 40.);
    let (path, ledger) = tmp_ledger("fill-first-backstop");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let refusal = richos_core::claude_accounts::usage_limit_error(Some(4_070_908_800_000), "[\"You've hit your session limit\"]");
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: Arc::new(Mutex::new(None)), fail_with: Some(refusal), prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: spawned.clone(),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));

    spine.submit_prompt("Draft the board update", Source::Text).unwrap();

    assert_eq!(quota.lease_account().id, "2");
    assert_eq!(*spawned.lock().unwrap(), vec!["2".to_string()]);
    assert_eq!(spine.last_rotation_reason(), Some(richos_core::spine::ACCOUNT_EXHAUSTED));
    let msgs = spine.messages(&thread).unwrap();
    let exchange: Vec<_> = msgs.iter().filter(|m| !m.text.starts_with("I switched the team to")).map(|m| m.text.as_str()).collect();
    assert_eq!(exchange, vec!["Draft the board update", "answered on account 2"], "one clean exchange");
    assert!(msgs.iter().any(|m| m.text == "I switched the team to your **Work** account. Account 1 reached a Claude usage limit, so the step it turned away runs again on Work."));
    let failed = spine.ledger().turns().iter().find(|t| t.state == TurnState::Interrupted).cloned().expect("kept on disk");
    assert!(failed.superseded_by.is_some());
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// Fast and normal readings change meter cadence without posting into the conversation.
#[test]
fn fast_and_normal_usage_checks_do_not_interrupt_the_conversation() {
    let (dir, quota) = two_accounts("alert", 50.);
    let (path, ledger) = tmp_ledger("fill-first-alert");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let start = richos_core::util::now_millis() + 1_000;
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(50., start))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: Arc::new(Mutex::new(Vec::new())),
        streamed: Arc::new(Mutex::new(None)), prompts }));
    let alerts = |spine: &richos_core::spine::Spine| spine.messages(&thread).unwrap().iter()
        .filter(|m| m.text.starts_with("Usage is climbing fast")).map(|m| m.text.clone()).collect::<Vec<_>>();

    spine.submit_prompt("one", Source::Text).unwrap();
    assert!(alerts(&spine).is_empty(), "one reading is no speed");
    *streamed.lock().unwrap() = Some(streamed_five_hour(54., start + 60_000));
    spine.submit_prompt("two", Source::Text).unwrap();
    assert!(alerts(&spine).is_empty(), "fast usage stays out of the conversation");
    assert_eq!(quota.view().refresh_interval_ms, richos_core::quota::FAST_REFRESH_INTERVAL_MS);
    *streamed.lock().unwrap() = Some(streamed_five_hour(58., start + 120_000));
    spine.submit_prompt("three", Source::Text).unwrap();
    assert!(alerts(&spine).is_empty());
    *streamed.lock().unwrap() = Some(streamed_five_hour(58.1, start + 180_000));
    spine.submit_prompt("four", Source::Text).unwrap();
    assert_eq!(quota.view().refresh_interval_ms, richos_core::quota::REFRESH_INTERVAL_MS);
    assert!(spine.messages(&thread).unwrap().iter().all(|m|
        !m.text.starts_with("Usage is ")), "normal readings stay out of the conversation too");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

// ============================================================================
// §119 — the model he chose in Preferences is the model of the next reply
// ============================================================================

/// A lease that knows the model it was started on, as `NativeCognition::lease_model` does,
/// and says it in every answer so a test reads which model answered rather than assumes it.
struct ModelLease {
    session_id: String,
    model: richos_core::config::ModelChoice,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>>,
}
impl Cognition for ModelLease {
    fn session_id(&self) -> &str { &self.session_id }
    fn lease_model(&self) -> Option<richos_core::config::ModelChoice> { Some(self.model) }
    fn running_commands(&self) -> Option<richos_core::lease_commands::CommandReading> { *self.commands.lock().unwrap() }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push((self.model.as_str().to_string(), text.to_string()));
        on_item(TurnItem::Text { seq: 0, text: &format!("answered on {}", self.model.as_str()) });
        Ok("end_turn".into())
    }
}

/// The desktop's factory reduced to §119: every lease starts on the model the stored setting
/// names at that moment (`EngineProfile::prepare` reads `config.json` at every spawn), and
/// `lease_models` answers that same setting.
struct ModelFactory {
    stored: Arc<Mutex<richos_core::config::ModelChoice>>,
    spawned: Arc<Mutex<Vec<String>>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>>,
}
impl LeaseFactory for ModelFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let model = *self.stored.lock().unwrap();
        let mut spawned = self.spawned.lock().unwrap();
        spawned.push(model.as_str().to_string());
        Ok(Box::new(ModelLease {
            session_id: format!("sess-{}-{}", model.as_str(), spawned.len()),
            model,
            prompts: self.prompts.clone(),
            commands: self.commands.clone(),
        }))
    }
    fn lease_models(&self) -> Option<richos_core::config::LeaseModels> {
        Some(richos_core::config::LeaseModels { rich: *self.stored.lock().unwrap(), stu: richos_core::config::ModelChoice::Opus })
    }
}

/// **§119 and the CEO's addendum:** *"Closing the preferences panel IS the confirmation that the
/// settings chosen there now need to be saved and implemented."* Saving is the shell's
/// `set_lease_models`; this is the implementing half. With the stored choice unchanged no turn
/// renews the lease; once it names Fable, his next reply comes from a Fable lease, renewed at
/// the boundary before his words are sent and never inside a turn; back to Opus, the same.
#[test]
fn the_model_he_chose_in_preferences_answers_his_next_reply() {
    use richos_core::config::ModelChoice;
    use richos_core::lease_commands::CommandReading;
    let (path, ledger) = tmp_ledger("model-changed");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let stored = Arc::new(Mutex::new(ModelChoice::Opus));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Some(CommandReading::Clear)));
    spine.attach_lease(Box::new(ModelLease {
        session_id: "sess-opus-0".into(), model: ModelChoice::Opus, prompts: prompts.clone(), commands: commands.clone(),
    }));
    spine.set_lease_factory(Box::new(ModelFactory {
        stored: stored.clone(), spawned: spawned.clone(), prompts: prompts.clone(), commands: commands.clone(),
    }));
    let answered_by = |text: &str| -> Vec<String> {
        prompts.lock().unwrap().iter().filter(|(_, t)| t == text).map(|(m, _)| m.clone()).collect()
    };

    spine.submit_prompt("one", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "the lease was renewed with the stored model unchanged");
    assert_eq!(answered_by("one"), vec!["opus"]);

    // Preferences closed with "talks with you" on Fable: the next reply is Fable's.
    *stored.lock().unwrap() = ModelChoice::Fable;
    spine.submit_prompt("two", Source::Text).unwrap();
    assert_eq!(answered_by("two"), vec!["fable"], "his next reply did not come from the model he chose");
    assert_eq!(spine.rotation_count(), 1);
    assert_eq!(spine.last_rotation_reason(), Some(richos_core::spine::MODEL_CHANGED));
    assert_eq!(*spawned.lock().unwrap(), vec!["fable".to_string()]);
    spine.submit_prompt("three", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 1, "a lease already on Fable was renewed again");

    // Back to Opus while a command the lease started is running: that turn stays on Fable,
    // and the switch happens at the first boundary after the command ends.
    *stored.lock().unwrap() = ModelChoice::Opus;
    *commands.lock().unwrap() = Some(CommandReading::Running(1));
    spine.submit_prompt("four", Source::Text).unwrap();
    assert_eq!(answered_by("four"), vec!["fable"], "a renewal retired a lease with a command running");
    *commands.lock().unwrap() = Some(CommandReading::Clear);
    spine.submit_prompt("five", Source::Text).unwrap();
    assert_eq!(answered_by("five"), vec!["opus"]);
    assert_eq!(spine.rotation_count(), 2);

    // One unbroken conversation across both renewals.
    let mine: Vec<_> = spine.messages(&thread).unwrap().into_iter().filter(|m| m.text.starts_with("answered on ")).collect();
    assert_eq!(mine.len(), 5, "{mine:?}");
    drop(std::fs::remove_file(&path));
}
