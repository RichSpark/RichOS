//! ENTITY BINDING + THE CROSS-ENTITY PRIVACY BOUNDARY.
//!
//! Slice 1 of the Codex-inspired conversation UX brief §24: *"core: bind threads and
//! turns to entities"*. These tests pin the properties the brief's §25 Integrity section
//! and the ECS architecture §3.2–3.4 make load-bearing:
//!
//!   - a thread has exactly one home entity, immutable after creation;
//!   - `entity_id` is required before retrieval or mutation, and its absence FAILS CLOSED;
//!   - **no event from one entity may ever render in another entity's thread**;
//!   - the binding revision is a fencing token, not a UI hint;
//!   - restart neither loses nor invents any of it.
//!
//! The leak test is a NEGATIVE CONTROL, not an assertion that passes because nothing
//! crosses: it builds a ledger on disk that genuinely does contain a foreign event, PROVES
//! the fixture crosses the boundary through the raw unscoped audit view, and only then
//! asserts the guarded view refuses it. Delete either clause of the guard in
//! `Ledger::thread_turns` and this test fails.

use richos_core::cognition::MockCognition;
use richos_core::entity::{EntityId, ThreadEntity};
use richos_core::ledger::{Ledger, LedgerError, Source};
use richos_core::spine::SpineError;

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn deeply() -> EntityId {
    EntityId::parse("deeply").unwrap()
}

fn tmp_path(tag: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "richos-entity-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = tmp_path(tag);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

// ---------------------------------------------------------------------------
// THE INTEGRITY PROPERTY (§25 Integrity, §22 "Must not be faked: cross-entity context")
// ---------------------------------------------------------------------------

#[test]
fn no_event_from_one_entity_renders_in_another_entitys_thread() {
    // A hand-built ledger: one femcboost thread, one deeply thread, and a FORGED turn
    // that sits in the femcboost thread while claiming the deeply entity. This is the
    // shape a corrupt log, a bad migration, a replayed stale active context or a future
    // writer bug produces — and it is the shape that leaks one company's conversation
    // into another company's view if nothing checks.
    let path = tmp_path("leak");
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
            r#"{"event":"TurnCompleted","turn_id":"turn_ok","stop_reason":"end_turn","at":4}"#,
            "\n",
            // THE FORGERY: thread_id says femcboost's thread, entity_id says deeply.
            r#"{"event":"PromptReceived","turn_id":"turn_leak","thread_id":"thr_fem","text":"deeply's Q4 term sheet numbers","source":"text","at":5,"entity_id":"deeply","binding_revision":2}"#,
            "\n",
            r#"{"event":"AssistantDelta","turn_id":"turn_leak","text":"deeply's Q4 term sheet numbers","at":6}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_leak","stop_reason":"end_turn","at":7}"#,
            "\n",
        ),
    )
    .unwrap();

    let ledger = Ledger::open(&path).unwrap();

    // (1) POSITIVE PROBE — the fixture really does cross the boundary. Without this the
    //     test below could pass because nothing foreign was ever there (the classic
    //     negative test that passes for the wrong reason). The RAW, unscoped audit view
    //     holds the forged turn, attached to the femcboost thread, carrying the secret.
    let raw = ledger.turns();
    let forged = raw.iter().find(|t| t.id == "turn_leak").expect("the forged turn IS in the log");
    assert_eq!(forged.thread_id, "thr_fem", "it really is filed under the femcboost thread");
    assert_eq!(forged.entity_id.as_ref().unwrap().as_str(), "deeply", "and it really does claim deeply");
    assert_eq!(forged.user_text, secret);

    // (2) THE GUARD — the scoped projection refuses it. Remove either clause of the
    //     filter in `Ledger::thread_turns` (`!t.quarantined`, or the entity equality)
    //     and this assertion fails with the secret rendered into femcboost's thread.
    let rendered = ledger.messages("thr_fem").unwrap();
    let rendered_text: String = rendered.iter().map(|m| m.text.as_str()).collect::<Vec<_>>().join("\n");
    assert!(
        !rendered_text.contains(secret),
        "CROSS-ENTITY LEAK: deeply's content rendered in a femcboost thread:\n{rendered_text}"
    );
    assert_eq!(rendered.len(), 1, "only the legitimately-femcboost turn renders");
    assert_eq!(rendered[0].text, "how is the release");

    // (3) ...and it is not silently swallowed. The rejection is REPORTED.
    let violations = ledger.scope_violations();
    assert_eq!(violations.len(), 1, "exactly one rejection, reported not hidden");
    assert_eq!(violations[0].turn_id, "turn_leak");
    assert_eq!(violations[0].thread_id, "thr_fem");
    assert_eq!(violations[0].thread_entity.as_deref(), Some("femcboost"));
    assert_eq!(violations[0].turn_entity.as_deref(), Some("deeply"));

    // (5) ...and crash recovery will not resume it into anything.
    assert!(
        !ledger.pending_turns().iter().any(|t| t.id == "turn_leak"),
        "a quarantined turn must never be replayed into an entity"
    );

    // (6) The deeply thread is untouched by any of this — quarantine excludes, it does
    //     not corrupt the other side of the boundary.
    assert!(ledger.messages("thr_dee").unwrap().is_empty());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_action_read_for_one_company_never_carries_another_companys_actions() {
    // The leak that predated entity binding: the action digest was assembled from the
    // WHOLE ledger, so every rotation told every entity's session what Rich had done in
    // every OTHER entity, under a header calling it authoritative.
    let (path, mut ledger) = tmp_ledger("digest-scope");
    let fem = ledger.create_thread("Avelor release", &femcboost()).unwrap();
    let dee = ledger.create_thread("Partner book", &deeply()).unwrap();
    let fem_binding = ledger.thread_binding(&fem).unwrap();
    let dee_binding = ledger.thread_binding(&dee).unwrap();

    let fem_turn = ledger.record_prompt_received(&fem_binding, "ship the release", Source::Text).unwrap();
    let dee_turn = ledger.record_prompt_received(&dee_binding, "draft the partner note", Source::Text).unwrap();
    ledger.record_action(&fem_turn, "dispatch", "spawned mark-sonnet-f1 on Avelor").unwrap();
    ledger.record_action(&dee_turn, "dispatch", "emailed the deeply partner list").unwrap();

    let fem_digest = ledger.ceo_facing_actions_for_entity(&femcboost());
    let details: Vec<&str> = fem_digest.iter().map(|a| a.detail.as_str()).collect();
    assert_eq!(details, vec!["spawned mark-sonnet-f1 on Avelor"], "femcboost sees only femcboost");

    let dee_digest = ledger.ceo_facing_actions_for_entity(&deeply());
    let details: Vec<&str> = dee_digest.iter().map(|a| a.detail.as_str()).collect();
    assert_eq!(details, vec!["emailed the deeply partner list"], "deeply sees only deeply");

    // The unscoped audit view still holds both — nothing was deleted, only unasserted.
    assert_eq!(ledger.ceo_facing_actions().len(), 2);
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// IMMUTABILITY (ECS §3.2: "immutable after creation"; moving creates a NEW thread)
// ---------------------------------------------------------------------------

#[test]
fn a_threads_entity_home_is_immutable_and_moving_means_a_new_thread() {
    let (path, mut ledger) = tmp_ledger("immutable");
    let thread = ledger.create_thread("Avelor release", &femcboost()).unwrap();
    assert_eq!(ledger.thread_binding(&thread).unwrap().entity_id(), &femcboost());

    // There is no rebinding API at all — the only entity-writing entry point refuses a
    // bound thread. (`Thread.entity` is a private field with no setter, so this is the
    // complete surface, not merely the convenient one.)
    let err = ledger.adopt_unbound_thread(&thread, &deeply(), "operator:test").unwrap_err();
    assert!(matches!(err, LedgerError::ThreadAlreadyBound { .. }), "got {err:?}");
    assert_eq!(ledger.thread_binding(&thread).unwrap().entity_id(), &femcboost(), "unchanged");

    // "Moving a conversation to another entity creates a new thread" — and it is a
    // genuinely different thread id, not the same record re-pointed.
    let moved = ledger.create_thread("Avelor release", &deeply()).unwrap();
    assert_ne!(moved, thread);
    assert_eq!(ledger.thread_binding(&moved).unwrap().entity_id(), &deeply());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_binding_can_only_be_obtained_from_the_ledger_so_a_write_cannot_name_a_foreign_entity() {
    // The write-side half of the guard is STRUCTURAL rather than defensive.
    // `ThreadBinding`'s fields are private, it has no setter, and its constructor is
    // `pub(crate)` — so outside the crate the only source of a binding is
    // `Ledger::thread_binding()`, which reads the entity out of the immutable record.
    //
    // The consequence, which this test pins: every write goes to the entity the LEDGER
    // says the thread lives in, whatever the caller believed. There is no argument a
    // caller can pass to send femcboost text into deeply.
    let (path, mut ledger) = tmp_ledger("obtain-only");
    let fem = ledger.create_thread("Avelor release", &femcboost()).unwrap();
    let dee = ledger.create_thread("Partner book", &deeply()).unwrap();

    let fem_binding = ledger.thread_binding(&fem).unwrap();
    let dee_binding = ledger.thread_binding(&dee).unwrap();
    assert_eq!(fem_binding.entity_id(), &femcboost());
    assert_eq!(dee_binding.entity_id(), &deeply());
    // The binding names its own thread; there is no way to point it at another one.
    assert_eq!(fem_binding.thread_id(), fem);
    assert_eq!(dee_binding.thread_id(), dee);

    ledger.record_prompt_received(&fem_binding, "ship the release", Source::Text).unwrap();
    ledger.record_prompt_received(&dee_binding, "draft the partner note", Source::Text).unwrap();

    let fem_text: String = ledger.messages(&fem).unwrap().iter().map(|m| m.text.clone()).collect();
    let dee_text: String = ledger.messages(&dee).unwrap().iter().map(|m| m.text.clone()).collect();
    assert!(fem_text.contains("release") && !fem_text.contains("partner note"));
    assert!(dee_text.contains("partner note") && !dee_text.contains("release"));
    assert!(ledger.scope_violations().is_empty());
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// THE UNBOUND LEGACY THREAD (the migration decision)
// ---------------------------------------------------------------------------

#[test]
fn a_pre_entity_thread_is_unbound_and_fails_closed_on_every_read_and_write() {
    // THE DECISION: pre-existing threads are NOT migrated by a heuristic. Nothing durable
    // in this log records the repository root a thread was created under, and the only
    // entity-ish value the app ever persisted is config.rs's free-text `company_name`.
    // Deriving a privacy boundary from a display string is a guess, and §22 names
    // cross-entity context as something that must not be faked.
    let path = tmp_path("legacy");
    std::fs::write(
        &path,
        concat!(
            r#"{"event":"ThreadCreated","thread_id":"thr_old","title":"Running","at":1}"#,
            "\n",
            r#"{"event":"PromptReceived","turn_id":"turn_old","thread_id":"thr_old","text":"an old conversation","source":"text","at":2}"#,
            "\n",
            r#"{"event":"TurnCompleted","turn_id":"turn_old","stop_reason":"end_turn","at":3}"#,
            "\n",
        ),
    )
    .unwrap();

    let mut ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.threads().len(), 1, "the legacy record still replays — nothing is lost");
    assert_eq!(*ledger.threads()[0].entity(), ThreadEntity::Unbound);

    // WHAT AN OPERATOR SEES: the thread is listed (so it can be found and decided on)
    // with no entity and no message count...
    let summaries = richos_core::thread::summaries(&ledger);
    assert_eq!(summaries[0].entity_id, None);
    assert_eq!(summaries[0].message_count, 0);
    assert_eq!(summaries[0].title, "Running", "it is findable, not hidden");
    assert_eq!(ledger.unbound_threads().len(), 1);

    // ...and every scoped operation refuses, with a message that says why.
    let err = ledger.messages("thr_old").unwrap_err();
    assert!(matches!(err, LedgerError::UnboundThread(_)));
    assert!(err.to_string().contains("will not guess"), "the refusal explains itself: {err}");
    assert!(matches!(ledger.thread_binding("thr_old"), Err(LedgerError::UnboundThread(_))));
    assert!(matches!(ledger.thread_turns("thr_old"), Err(LedgerError::UnboundThread(_))));

    // An unbound thread cannot even be activated, so no turn can ever be accepted for it.
    {
        let mut spine = support::spine(Ledger::open(&path).unwrap());
        assert!(matches!(spine.switch_thread("thr_old"), Err(SpineError::Ledger(LedgerError::UnboundThread(_)))));
        assert!(matches!(spine.submit_prompt("hello", Source::Text), Err(SpineError::NoActiveThread)));
        assert_eq!(spine.ledger().turns().len(), 1, "no new turn was persisted by the refused send");
    }
    ledger = Ledger::open(&path).unwrap();

    // THE ONE EXIT: an explicit operator decision, recorded with its author. Not a
    // heuristic, not a default — a human statement, which ECS §5.1 classes as an
    // explicit instruction (a write class that needs no confirmation), unlike an
    // inference (candidate only).
    let binding = ledger.adopt_unbound_thread("thr_old", &femcboost(), "operator:echo").unwrap();
    assert_eq!(binding.entity_id(), &femcboost());
    assert_eq!(ledger.messages("thr_old").unwrap().len(), 1, "readable again, contents intact");
    assert!(ledger.scope_violations().is_empty(), "an unstamped legacy turn inherits its thread's home");
    assert!(ledger.unbound_threads().is_empty());

    // The decision itself is durable and survives restart.
    drop(ledger);
    let reopened = Ledger::open(&path).unwrap();
    assert_eq!(reopened.thread_binding("thr_old").unwrap().entity_id(), &femcboost());
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// FAILING LOUDLY WHEN THE ENTITY CANNOT BE RESOLVED
// ---------------------------------------------------------------------------

#[test]
fn a_turn_with_no_resolvable_entity_is_refused_loudly_and_never_defaults() {
    // UX §21 "Entity binding failure": *"Block send. State that Rich cannot safely
    // determine which entity the work belongs to. Require an explicit entity choice.
    // Never default to the last entity."*
    let path = tmp_path("no-entity");
    let fem;
    {
        // Set the stage: threads in TWO entities, then shut down. On a cold start there is
        // no active context, and "just pick the first thread" would be picking a privacy
        // boundary for the CEO.
        let mut spine = support::spine(Ledger::open(&path).unwrap());
        fem = spine.create_thread("Avelor release", &femcboost()).unwrap();
        spine.create_thread("Partner book", &deeply()).unwrap();
    }

    let mut fresh = support::spine(Ledger::open(&path).unwrap());
    fresh.attach_lease(Box::new(MockCognition::new("s2", vec!["ok"])));
    assert!(fresh.active_binding().is_none(), "a cold start has no active context");
    let err = fresh.submit_prompt("which company is this about?", Source::Text).unwrap_err();
    assert!(matches!(err, SpineError::NoActiveThread), "got {err:?}");
    assert!(err.to_string().contains("will not guess"), "the refusal explains itself: {err}");
    assert_eq!(
        fresh.ledger().turns().len(),
        0,
        "an unscoped turn is never persisted — ECS §3.4 rejects an unscoped event, so the \
         crash window must not contain one either"
    );

    // Naming the entity resolves it, and the turn lands in that entity and no other.
    let binding = fresh.ensure_active_thread_in(&femcboost()).unwrap();
    assert_eq!(binding.thread_id(), fem, "the existing femcboost thread, not a new one");
    fresh.submit_prompt("ship the release", Source::Text).unwrap();
    assert_eq!(fresh.messages(&fem).unwrap().len(), 2);
    assert!(fresh.ledger().scope_violations().is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_unregistered_entity_can_never_become_a_threads_home() {
    let (path, ledger) = tmp_ledger("unregistered");
    let mut spine = support::spine(ledger);
    let stranger = EntityId::parse("acme-holdings").unwrap();
    assert!(!support::registry().contains(&stranger));

    let err = spine.create_thread("Whose is this?", &stranger).unwrap_err();
    assert!(matches!(err, SpineError::UnknownEntity(_)), "got {err:?}");
    assert!(matches!(spine.ensure_active_thread_in(&stranger), Err(SpineError::UnknownEntity(_))));
    assert_eq!(spine.threads().len(), 0, "nothing was created");
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// THE FENCING TOKEN (ECS §3.4: "This is a fencing token, not a UI hint")
// ---------------------------------------------------------------------------

#[test]
fn switching_the_active_context_advances_the_fence_and_stales_the_old_binding() {
    let (path, ledger) = tmp_ledger("fence");
    let mut spine = support::spine(ledger);
    let fem = spine.create_thread("Avelor release", &femcboost()).unwrap();
    let dee = spine.create_thread("Partner book", &deeply()).unwrap();

    let first = spine.active_binding().cloned().unwrap();
    assert_eq!(first.thread_id(), fem);
    assert_eq!(first.entity_id(), &femcboost());

    spine.switch_thread(&dee).unwrap();
    let second = spine.active_binding().cloned().unwrap();
    assert_eq!(second.entity_id(), &deeply(), "entity and thread moved together — §11.3");
    assert!(
        second.binding_revision() > first.binding_revision(),
        "activation is a transaction that advances the fence: r{} -> r{}",
        first.binding_revision(),
        second.binding_revision()
    );

    // Going back is a NEW revision too — a fence never rewinds.
    spine.switch_thread(&fem).unwrap();
    let third = spine.active_binding().cloned().unwrap();
    assert_eq!(third.thread_id(), fem);
    assert!(third.binding_revision() > second.binding_revision());
    assert_eq!(third.entity_id(), &femcboost(), "the entity is re-read from the record, never carried over");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_binding_captured_before_a_context_switch_is_refused_afterwards() {
    // The fence doing its job. `first` was captured when femcboost was active; after the
    // CEO moves to deeply it is BOTH a foreign scope and an older revision, and an
    // outbound command carrying it is refused rather than allowed to write "into a newly
    // switched entity/thread" (ECS §3.4).
    let (path, ledger) = tmp_ledger("stale-fence");
    let mut spine = support::spine(ledger);
    let fem = spine.create_thread("Avelor release", &femcboost()).unwrap();
    let dee = spine.create_thread("Partner book", &deeply()).unwrap();

    spine.switch_thread(&fem).unwrap();
    let captured = spine.active_binding().cloned().unwrap();
    spine.verify_active_binding(&captured).expect("current while it is current");

    // Cross-entity switch: the captured binding is now foreign.
    spine.switch_thread(&dee).unwrap();
    let err = spine.verify_active_binding(&captured).unwrap_err();
    assert!(matches!(err, SpineError::Ledger(LedgerError::ScopeMismatch { .. })), "got {err:?}");

    // Same-thread re-activation: same entity, but an ADVANCED revision — the pure
    // stale-fence case, with no entity change to hide behind.
    spine.switch_thread(&fem).unwrap();
    let err = spine.verify_active_binding(&captured).unwrap_err();
    match err {
        SpineError::Ledger(LedgerError::StaleBinding { presented, current, .. }) => {
            assert_eq!(presented, captured.binding_revision());
            assert!(current > presented, "the fence advanced: r{presented} -> r{current}");
        }
        other => panic!("expected StaleBinding, got {other:?}"),
    }

    // The CURRENT binding is accepted — the fence rejects staleness, not the thread.
    let now = spine.active_binding().cloned().unwrap();
    spine.verify_active_binding(&now).expect("the current binding passes");
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// RESTART (§23 Phase 1 exit gate, restricted to what slice 1 touches)
// ---------------------------------------------------------------------------

#[test]
fn bindings_survive_restart_with_state_neither_lost_nor_invented() {
    let path = tmp_path("restart");
    let (fem, dee, fem_rev, dee_rev);
    {
        let mut spine = support::spine(Ledger::open(&path).unwrap());
        spine.attach_lease(Box::new(MockCognition::new("s1", vec!["release is on track", "note drafted"])));
        fem = spine.create_thread("Avelor release", &femcboost()).unwrap();
        dee = spine.create_thread("Partner book", &deeply()).unwrap();

        spine.switch_thread(&fem).unwrap();
        spine.submit_prompt("how is the release", Source::Text).unwrap();
        spine.switch_thread(&dee).unwrap();
        spine.submit_prompt("draft the partner note", Source::Text).unwrap();

        fem_rev = spine.ledger().thread_binding(&fem).unwrap().binding_revision();
        dee_rev = spine.ledger().thread_binding(&dee).unwrap().binding_revision();
        assert_eq!(spine.messages(&fem).unwrap().len(), 2);
        assert_eq!(spine.messages(&dee).unwrap().len(), 2);
    }

    // KILL AND RESTART — a cold `Ledger::open` replays the whole log from disk.
    let reopened = Ledger::open(&path).unwrap();

    // NOT LOST: both homes, both revisions, both conversations.
    assert_eq!(reopened.thread_binding(&fem).unwrap().entity_id(), &femcboost());
    assert_eq!(reopened.thread_binding(&dee).unwrap().entity_id(), &deeply());
    assert_eq!(reopened.thread_binding(&fem).unwrap().binding_revision(), fem_rev);
    assert_eq!(reopened.thread_binding(&dee).unwrap().binding_revision(), dee_rev);
    assert_eq!(reopened.messages(&fem).unwrap().len(), 2);
    assert_eq!(reopened.messages(&dee).unwrap().len(), 2);
    assert_eq!(reopened.messages(&fem).unwrap()[0].text, "how is the release");
    assert_eq!(reopened.messages(&dee).unwrap()[0].text, "draft the partner note");

    // NOT INVENTED: no thread gained an entity it did not have, no turn crossed, no
    // violation appeared out of a clean log, and the two entities did not merge.
    assert!(reopened.scope_violations().is_empty());
    assert!(reopened.unbound_threads().is_empty());
    let fem_msgs: String = reopened.messages(&fem).unwrap().iter().map(|m| m.text.clone()).collect();
    assert!(!fem_msgs.contains("partner note"), "deeply's turn did not migrate into femcboost");

    // ...and the fence does not rewind across the restart: a NEW activation issues a
    // revision above everything durably recorded.
    let mut spine = support::spine(reopened);
    spine.switch_thread(&fem).unwrap();
    let after = spine.active_binding().unwrap().binding_revision();
    assert!(
        after > fem_rev && after > dee_rev,
        "post-restart revision {after} must exceed every durable revision (r{fem_rev}, r{dee_rev})"
    );
    let _ = std::fs::remove_file(&path);
}
