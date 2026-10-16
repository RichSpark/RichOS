//! ACTION-LEDGER WRITER invariants — the fix for the v1 correctness bug in which
//! `Ledger::record_action` had NO production caller, so the action ledger was
//! permanently EMPTY at runtime while `reprime.rs`'s identity assertion told every
//! rotated session that it was "ground truth for what Rich has done — authoritative."
//! The anti-false-attribution guarantee of the continuity design (§5.4 / §6.1) rested
//! on nothing, and — like the dictation bug — it failed SILENTLY while appearing to work.
//!
//! These tests are the proof that the claim is now true rather than aspirational:
//! actions are written by PRODUCTION code paths (never by the test itself), they are
//! non-empty at runtime, they survive a rotation into the successor's priming prompt,
//! and the machinery half never leaks into it.
//!
//! Headless throughout (`MockCognition` / `MockLeaseFactory`): no live Claude, no network.
//! Test names document the invariant, per this codebase's convention.

use richos_core::cognition::{Cognition, CognitionError, TurnItem, MockCognition, MockLeaseFactory};
use richos_core::ledger::{
    ActionStatus, ActionVisibility, AttentionTier, Ledger, LedgerError, Source, ACTION_DETAIL_MAX_CHARS,
};
use richos_core::reprime::RePrimePayload;
use richos_core::entity::EntityId;

mod support;

/// The dogfood entity these tests run under. Every thread now has an immutable entity
/// home (ECS §3.2) and there is no entity-less path, so the tests NAME one rather than
/// inheriting a default that no longer exists.
fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-action-ledger-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

/// A lease that streams one partial chunk then dies — a POSITIVE termination signal
/// (§5.2: death is never inferred from silence).
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

// =============================================================================
// 1. The bug itself: the ledger is NON-EMPTY at runtime, from production paths
// =============================================================================

#[test]
fn raising_a_proactive_message_writes_a_ceo_facing_action_at_runtime() {
    // THE regression test for the defect. Nothing here calls `record_action` — the only
    // writer exercised is `Spine::raise_proactive`, the production path behind the
    // `raise_proactive_message` Tauri command (src-tauri/src/main.rs).
    let (path, ledger) = tmp_ledger("proactive-writes-action");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    assert!(
        spine.ledger().ceo_facing_actions().is_empty(),
        "precondition: a brand-new conversation has recorded no actions"
    );

    let turn_id = spine
        .raise_proactive(Some(&thread), AttentionTier::Digest, "Flagged the Acme renewal for you.")
        .unwrap();

    let actions = spine.ledger().ceo_facing_actions();
    assert_eq!(actions.len(), 1, "the action ledger is NON-EMPTY at runtime — the whole point");
    assert_eq!(actions[0].kind, "proactive_message");
    assert_eq!(actions[0].status, ActionStatus::Completed);
    assert_eq!(actions[0].visibility, ActionVisibility::CeoFacing);
    assert_eq!(
        actions[0].turn_id.as_deref(),
        Some(turn_id.as_str()),
        "a CEO-facing action is anchored to the turn it belongs to"
    );
    assert!(actions[0].detail.contains("Flagged the Acme renewal for you."));
    assert!(actions[0].detail.contains("digest"), "the tier it was raised at is part of the record");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_silent_tier_proactive_message_is_recorded_as_an_action_its_only_durable_surface() {
    // Tier 3 / Silent has NO render path at all (`messages()` skips it, UX §5.1), so
    // outside the action ledger there is NO surface on which a successor could ever
    // learn that Rich already noted this — it would re-raise it, or deny having raised
    // it. This is the sharpest case for wiring the ledger.
    let (path, ledger) = tmp_ledger("silent-recorded");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    spine
        .raise_proactive(Some(&thread), AttentionTier::Silent, "FYI: renewed the Acme NDA.")
        .unwrap();

    assert!(spine.messages(&thread).unwrap().is_empty(), "Silent still never renders to the CEO");
    let actions = spine.ledger().ceo_facing_actions();
    assert_eq!(actions.len(), 1, "...but it IS recorded as an action");
    assert!(actions[0].detail.contains("renewed the Acme NDA"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rotation_writes_internal_actions_claimed_then_completed() {
    // Claim-then-execute (§6.4) on the machinery half: the rotation and the successor's
    // re-prime injection are both claimed BEFORE they run and settled after, so a crash
    // mid-rotation leaves a durable `claimed` record instead of silence.
    let (path, ledger) = tmp_ledger("rotation-internal-actions");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-initial", vec!["ack"])));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["ack2"])));

    spine.request_rotation("test-rotation").unwrap();
    spine.submit_prompt("continue", Source::Text).unwrap();

    let internal = spine.ledger().internal_actions();
    let kinds: Vec<&str> = internal.iter().map(|a| a.kind.as_str()).collect();
    assert!(kinds.contains(&"session_rotation"), "the rotation itself is recorded: {kinds:?}");
    assert!(kinds.contains(&"session_reprime"), "the successor's priming is recorded: {kinds:?}");

    let rotation = internal.iter().find(|a| a.kind == "session_rotation").unwrap();
    assert_eq!(rotation.status, ActionStatus::Completed, "settled, not left dangling");
    assert!(rotation.detail.contains("reason=test-rotation"));
    assert!(rotation.turn_id.is_none(), "rotation happens BETWEEN turns — it belongs to none");

    let reprime = internal.iter().find(|a| a.kind == "session_reprime").unwrap();
    assert_eq!(reprime.status, ActionStatus::Completed);
    assert!(
        reprime.detail.contains("priming_chars="),
        "the payload size is MEASURED into the record, not asserted: {}",
        reprime.detail
    );

    assert!(
        spine.ledger().open_actions().is_empty(),
        "a clean rotation leaves NO action still claimed"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_rotation_that_cannot_spawn_a_successor_leaves_a_durable_failed_record() {
    // Before this fix a failed rotation vanished into an `Err` return with no trace.
    let (path, ledger) = tmp_ledger("rotation-spawn-fails");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-initial", vec!["ack"])));
    let factory = MockLeaseFactory::new(vec!["never used"]);
    factory.fail_next_spawn();
    spine.set_lease_factory(Box::new(factory));

    spine.request_rotation("watermark").unwrap();
    // Ordinary conversation retries a failed child through its existing recovery path.
    // The first failed rotation must remain recorded even when that recovery succeeds.
    spine.submit_prompt("continue", Source::Text).unwrap();

    let rotation = spine
        .ledger()
        .internal_actions()
        .into_iter()
        .find(|a| a.kind == "session_rotation")
        .expect("the attempted rotation was recorded even though it failed");
    assert_eq!(rotation.status, ActionStatus::Failed);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn first_lease_priming_is_recorded_as_an_internal_action() {
    // The boot path (`prime_lease_if_needed`), not the rotation path: "the successor WAS
    // primed" is the single fact the entire anti-false-attribution guarantee rests on,
    // so it is durable on the FIRST lease too, not only on rotated ones.
    let (path, ledger) = tmp_ledger("first-prime");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-boot", vec!["hi"])));

    spine.submit_prompt("hello", Source::Text).unwrap();

    let reprime = spine
        .ledger()
        .internal_actions()
        .into_iter()
        .find(|a| a.kind == "session_reprime")
        .expect("the boot lease's priming was recorded");
    assert_eq!(reprime.status, ActionStatus::Completed);
    assert!(reprime.detail.contains("session=sess-boot"), "detail: {}", reprime.detail);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn mid_turn_crash_recovery_is_recorded_as_an_internal_action() {
    // §5.3. The recovery is claimed BEFORE the respawn and completed after the replay
    // lands, so the durable record exists even if recovery itself dies partway.
    let (path, ledger) = tmp_ledger("crash-recovery-action");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(FailingCognition { session_id: "sess-doomed".into() }));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["recovered reply"])));

    spine.submit_prompt("do the thing", Source::Text).unwrap();

    let recovery = spine
        .ledger()
        .internal_actions()
        .into_iter()
        .find(|a| a.kind == "crash_recovery")
        .expect("the crash recovery was recorded");
    assert_eq!(recovery.status, ActionStatus::Completed);
    assert!(recovery.detail.contains("replay interrupted turn"), "detail: {}", recovery.detail);
    let _ = std::fs::remove_file(&path);
}

// =============================================================================
// 2. The payload actually carries them across a rotation
// =============================================================================

#[test]
fn a_ceo_facing_action_recorded_by_production_code_reaches_the_successors_priming_prompt() {
    // THE end-to-end proof of done-criterion (b), "no false attribution": an action
    // written ONLY by the production path (`raise_proactive`) is verified on the ACTUAL
    // spawned successor lease's re-prime log — not on a payload object in isolation.
    let (path, ledger) = tmp_ledger("action-survives-rotation");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-initial", vec!["ack"])));
    let factory = MockLeaseFactory::new(vec!["ack2"]);
    let spawned_reprimes = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    // Rich reaches out unprompted — the ONLY writer exercised here.
    spine
        .raise_proactive(Some(&thread), AttentionTier::Digest, "Told the CEO the Acme counter expires at noon.")
        .unwrap();

    spine.request_rotation("test").unwrap();
    spine.submit_prompt("continue", Source::Text).unwrap();

    let per_spawn = spawned_reprimes.lock().unwrap();
    assert_eq!(per_spawn.len(), 1, "exactly one successor lease was spawned");
    let successor_reprimes = per_spawn[0].lock().unwrap();
    assert_eq!(successor_reprimes.len(), 1, "primed exactly once, before any CEO turn");
    let priming = &successor_reprimes[0];

    assert!(priming.contains("action_ledger_digest"), "the section the identity assertion points at exists");
    assert!(
        priming.contains("Told the CEO the Acme counter expires at noon."),
        "the action recorded by production code SURVIVED into the successor's ground truth:\n{priming}"
    );
    assert!(priming.contains("proactive_message"), "with its kind, so the successor knows what it was");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn internal_machinery_actions_never_leak_into_a_priming_prompt() {
    // The counterpart invariant. The identity assertion orders the successor to NEVER
    // reveal or reference session rotation (§6.2); handing it "[done] session_rotation"
    // under a header calling that section authoritative ground truth for what Rich has
    // DONE would manufacture exactly that leak. Internal actions stay durable and unseen.
    let (path, ledger) = tmp_ledger("no-machinery-leak");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-initial", vec!["ack"])));
    let factory = MockLeaseFactory::new(vec!["a", "b"]);
    let spawned_reprimes = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));

    spine.request_rotation("first").unwrap();
    spine.submit_prompt("hello", Source::Text).unwrap();
    spine.request_rotation("second").unwrap();
    spine.submit_prompt("continue again", Source::Text).unwrap();

    // The machinery IS durably recorded...
    assert!(
        spine.ledger().internal_actions().len() >= 4,
        "two rotations + two re-primes were recorded: {:?}",
        spine.ledger().internal_actions().iter().map(|a| &a.kind).collect::<Vec<_>>()
    );

    // ...and reaches NO priming prompt.
    let per_spawn = spawned_reprimes.lock().unwrap();
    for lease_reprimes in per_spawn.iter() {
        for priming in lease_reprimes.lock().unwrap().iter() {
            assert!(!priming.contains("session_rotation"), "machinery leaked:\n{priming}");
            assert!(!priming.contains("session_reprime"), "machinery leaked:\n{priming}");
            assert!(!priming.contains("crash_recovery"), "machinery leaked:\n{priming}");
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_action_ledger_section_is_always_rendered_so_the_assertion_never_points_at_nothing() {
    // The dangling-reference half of the same bug: the identity assertion ALWAYS says
    // "The ACTION LEDGER below is ground truth", but `to_priming_prompt` used to omit
    // the section entirely when empty. A successor reading an assertion that points at a
    // missing section is one inference away from "no ledger ⇒ nothing was done" — false
    // DENIAL, the mirror image of false attribution.
    let (path, ledger) = tmp_ledger("empty-digest");
    let thread_holder = {
        let mut l = ledger;
        let t = l.create_thread("General", &femcboost()).unwrap();
        let payload = RePrimePayload::assemble(&l, &l.thread_binding(&t).unwrap(), 8, None).unwrap();
        assert!(payload.action_ledger_digest.is_empty());
        let priming = payload.to_priming_prompt();
        assert!(priming.contains("action_ledger_digest"), "the section is present even when empty");
        assert!(
            serde_json::from_str::<serde_json::Value>(&priming).unwrap()["context"]["action_ledger_digest"]==serde_json::json!([]),
            "and it says explicitly what an empty ledger does and does not prove:\n{priming}"
        );
        t
    };
    assert!(!thread_holder.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_ledgers_partial_coverage_is_stated_rather_than_overclaimed() {
    let plugin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app");
    let definition = std::fs::read_to_string(plugin.join("agents/rich.md")).unwrap();
    assert!(definition.contains("Records can be partial or stale"));
    assert!(definition.contains("missing records do not prove nothing happened"));
    assert!(definition.contains("Distinguish an unknown from a negative finding"));
    let assertion: serde_json::Value = serde_json::from_str(&RePrimePayload::identity_assertion("thr_demo")).unwrap();
    assert_eq!(assertion, serde_json::json!({"thread_id":"thr_demo"}));
}

#[test]
fn a_proactive_turn_in_the_verbatim_tail_carries_no_phantom_ceo_line() {
    // Found while wiring the action ledger, same false-attribution class. A Proactive
    // turn has `user_text == ""` by construction, and the tail builder used to emit the
    // user line unconditionally — so the priming prompt contained a literal
    //     user:
    //     assistant: <Rich's unprompted message>
    // A successor reads that as "the CEO said something and Rich answered", attributing
    // Rich's own initiative to the CEO. Verified against the real rendered prompt text,
    // not just the payload struct.
    let (path, ledger) = tmp_ledger("proactive-tail");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["ok"])));
    spine.submit_prompt("what's on my plate?", Source::Text).unwrap();
    spine
        .raise_proactive(Some(&thread), AttentionTier::Digest, "Acme counter expires at noon.")
        .unwrap();

    let payload = RePrimePayload::assemble(spine.ledger(), &spine.ledger().thread_binding(&thread).unwrap(), 8, None).unwrap();
    assert!(
        payload.recent_tail.iter().all(|tv| !tv.text.is_empty()),
        "no empty-text line survives into the tail: {:?}",
        payload.recent_tail.iter().map(|tv| (&tv.role, &tv.text)).collect::<Vec<_>>()
    );

    let priming = payload.to_priming_prompt();
    assert!(!priming.contains("  user: \n"), "no phantom blank CEO line:\n{priming}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&priming).unwrap()["context"]["recent_tail"].as_array().unwrap().iter().any(|row|row["role"]=="assistant (unprompted)" && row["text"]=="Acme counter expires at noon."),
        "Rich's unprompted message is labelled as his own initiative:\n{priming}"
    );
    // The real CEO turn is untouched.
    assert!(serde_json::from_str::<serde_json::Value>(&priming).unwrap()["context"]["recent_tail"].as_array().unwrap().iter().any(|row|row["role"]=="user" && row["text"]=="what's on my plate?"));
    let _ = std::fs::remove_file(&path);
}

// =============================================================================
// 3. Durability + record-format invariants
// =============================================================================

#[test]
fn actions_and_their_visibility_survive_a_restart() {
    // The ledger is the durable substrate; the in-memory projection is a disposable fold.
    let (path, ledger) = tmp_ledger("actions-survive-restart");
    {
        let mut spine = support::spine(ledger);
        let thread = spine.create_thread("General", &femcboost()).unwrap();
        spine.raise_proactive(Some(&thread), AttentionTier::Digest, "durable action").unwrap();
        spine.attach_lease(Box::new(MockCognition::new("sess-initial", vec!["ack"])));
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["ack2"])));
        spine.request_rotation("restart-test").unwrap();
        spine.submit_prompt("continue", Source::Text).unwrap();
    }

    let reopened = Ledger::open(&path).unwrap();
    assert_eq!(reopened.ceo_facing_actions().len(), 1, "CEO-facing action replayed");
    assert_eq!(reopened.ceo_facing_actions()[0].detail, "[digest] durable action");
    assert!(
        reopened.internal_actions().iter().any(|a| a.kind == "session_rotation"),
        "internal machinery replayed too"
    );
    assert!(reopened.open_actions().is_empty(), "settled statuses replayed, not just claims");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_pre_visibility_action_record_replays_as_ceo_facing() {
    // Backward compatibility of the append-only log: records written before `visibility`
    // and the optional `turn_id` existed must still replay, with their original meaning.
    let path = std::env::temp_dir().join(format!(
        "richos-action-legacy-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_1","title":"General","at":1}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_1","thread_id":"thr_1","text":"hi","source":"text","at":2}"#,
            "\n",
            // The OLD shape: turn_id as a bare string, no `visibility` field at all.
            r#"{"event":"ActionRecorded","action_id":"act_1","turn_id":"turn_1","kind":"dispatch","detail":"spawned worker mark-sonnet-f1","status":"claimed","at":3}"#,
            "\n",
        ),
    )
    .unwrap();

    let mut ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.actions().len(), 1, "the legacy record still replays");
    assert_eq!(ledger.actions()[0].visibility, ActionVisibility::CeoFacing, "with its original meaning");
    assert_eq!(ledger.actions()[0].turn_id.as_deref(), Some("turn_1"));

    // ...but the THREAD it belongs to predates entity binding, so it is Unbound and
    // FAILS CLOSED. No heuristic invents an entity for it, and no re-prime can run.
    assert!(!ledger.threads()[0].is_bound());
    assert!(matches!(ledger.thread_binding("thr_1"), Err(LedgerError::UnboundThread(_))));
    assert!(matches!(ledger.messages("thr_1"), Err(LedgerError::UnboundThread(_))));

    // The one exit is an EXPLICIT operator decision, recorded with its author. After it,
    // the legacy turn (which carries no entity stamp) inherits its thread's home — the
    // thread record is the authority, so that is not a guess — and the digest resolves.
    let binding = ledger.adopt_unbound_thread("thr_1", &femcboost(), "operator:echo").unwrap();
    assert_eq!(binding.entity_id().as_str(), "femcboost");
    let payload = RePrimePayload::assemble(&ledger, &binding, 8, None).unwrap();
    assert_eq!(payload.action_ledger_digest.len(), 1);
    assert_eq!(ledger.messages("thr_1").unwrap().len(), 1, "the legacy conversation is readable again");

    // ...and it is ONE-WAY: the entity home is immutable from the moment it exists.
    assert!(matches!(
        ledger.adopt_unbound_thread("thr_1", &EntityId::parse("deeply").unwrap(), "operator:echo"),
        Err(LedgerError::ThreadAlreadyBound { .. })
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn action_detail_is_truncated_on_a_char_boundary_not_a_byte_boundary() {
    // The CEO-facing digest is re-injected VERBATIM into every successor's priming
    // prompt and billed per rotation under BYO-Anthropic, so each entry is bounded.
    // `&s[..160]` would PANIC mid-codepoint on any non-ASCII input — an em-dash in the
    // CEO's own phrasing is enough. Frame the math: 300 '—' chars = 900 bytes.
    let (path, mut ledger) = tmp_ledger("truncation");
    let thread = ledger.create_thread("General", &femcboost()).unwrap();
    let long: String = "—".repeat(300);
    assert_eq!(long.len(), 900, "300 chars x 3 bytes = 900 bytes");
    assert_eq!(long.chars().count(), 300);

    let id = ledger
        .record_action_with(None, "note", &long, ActionVisibility::CeoFacing, ActionStatus::Completed)
        .unwrap();
    let action = ledger.actions().iter().find(|a| a.id == id).unwrap();
    assert_eq!(
        action.detail.chars().count(),
        ACTION_DETAIL_MAX_CHARS + 1,
        "160 kept chars + 1 ellipsis"
    );
    assert!(action.detail.ends_with('\u{2026}'));

    // Short details are untouched.
    let short = ledger
        .record_action_with(None, "note", "short", ActionVisibility::CeoFacing, ActionStatus::Completed)
        .unwrap();
    assert_eq!(ledger.actions().iter().find(|a| a.id == short).unwrap().detail, "short");
    assert!(!thread.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn record_action_keeps_its_ceo_facing_claim_then_execute_default() {
    // The pre-existing 3-arg entry point is unchanged in meaning: CEO-facing, Claimed,
    // turn-scoped. (Kept so the ledger's documented claim-then-execute API stays the
    // obvious one to reach for when a future typed-action writer lands.)
    let (path, mut ledger) = tmp_ledger("record-action-default");
    let thread = ledger.create_thread("General", &femcboost()).unwrap();
    let turn = ledger.record_prompt_received(&ledger.thread_binding(&thread).unwrap(), "dispatch a worker", Source::Text).unwrap();
    let id = ledger.record_action(&turn, "dispatch", "spawned worker mark-sonnet-f1").unwrap();

    let a = ledger.actions().iter().find(|a| a.id == id).unwrap();
    assert_eq!(a.status, ActionStatus::Claimed);
    assert_eq!(a.visibility, ActionVisibility::CeoFacing);
    assert_eq!(a.turn_id.as_deref(), Some(turn.as_str()));
    assert_eq!(ledger.open_actions().len(), 1, "a claim is OPEN until settled");

    ledger.update_action(&id, ActionStatus::Completed).unwrap();
    assert!(ledger.open_actions().is_empty());
    let _ = std::fs::remove_file(&path);
}
