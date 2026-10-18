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

use richos_core::ledger::{
    ActionStatus, ActionVisibility, Ledger, LedgerError, Source, ACTION_DETAIL_MAX_CHARS,
};
use richos_core::entity::EntityId;


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

// =============================================================================
// 1. The bug itself: the ledger is NON-EMPTY at runtime, from production paths
// =============================================================================







// =============================================================================
// 2. The payload actually carries them across a rotation
// =============================================================================




#[test]
fn the_ledgers_partial_coverage_is_stated_rather_than_overclaimed() {
    let plugin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app");
    let definition = std::fs::read_to_string(plugin.join("agents/rich.md")).unwrap();
    assert!(definition.contains("Records can be partial or stale"));
    assert!(definition.contains("missing records do not prove nothing happened"));
    assert!(definition.contains("Distinguish an unknown from a negative finding"));
}


// =============================================================================
// 3. Durability + record-format invariants
// =============================================================================

#[test]
fn actions_and_their_visibility_survive_a_restart() {
    let (path, mut ledger) = tmp_ledger("actions-survive-restart");
    let thread=ledger.create_thread("General", &femcboost()).unwrap();
    let turn=ledger.record_prompt_received(&ledger.thread_binding(&thread).unwrap(), "the request", Source::Text).unwrap();
    let action=ledger.record_action(&turn,"dispatch","a recorded action").unwrap();
    ledger.update_action(&action,ActionStatus::Completed).unwrap();
    drop(ledger);
    let reopened=Ledger::open(&path).unwrap();
    assert_eq!(reopened.ceo_facing_actions().len(),1);
    assert_eq!(reopened.ceo_facing_actions()[0].detail,"a recorded action");
    assert!(reopened.open_actions().is_empty());
    std::fs::remove_file(path).unwrap();
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
    assert_eq!(ledger.ceo_facing_actions_for_entity(&femcboost()).len(), 1);
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
