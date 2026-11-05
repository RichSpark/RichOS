use richos_core::{entity::EntityId, ledger::{Ledger, Source}, questions::*, swearmeter_questions};
use std::{path::PathBuf, sync::{Arc, Mutex}};

struct Fixture { root: PathBuf, store: Store, cards: Arc<Mutex<swearmeter_questions::Projection>> }
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("swearmeter-{}", uuid::Uuid::new_v4()));
        let cards = Arc::new(Mutex::new(swearmeter_questions::Projection::empty()));
        Self { store: Store::new(&root).with_swearmeter(cards.clone()), root, cards }
    }
    fn ask(&self, asker: &str) -> Vec<Question> {
        let scope = AskScope { root:self.root.clone(), entity_id:"fixture".into(),
            thread_id:"thread".into(), turn_id:"turn".into(), asker:asker.into(),
            session_id:"fixture-session".into(), obligation_id:None, engine:None,
            entity_root:None, app_run:None };
        self.store.ask(&scope, (0..2).map(|_| QuestionInput {
            text:"Which damn build?".into(), options:vec![
                OptionInput { label:"The shit build".into(), description:"First".into() },
                OptionInput { label:"The other build".into(), description:"Second".into() }],
            multiple:false, free_answer:true, recommended:None,
        }).collect()).unwrap()
    }
    fn answer(&self, question: &Question, id: &str, text: &str, origin: AnswerOrigin) -> AnswerResult {
        self.store.answer_from("fixture", "thread", AnswerRequest {
            question_id:question.id.clone(), client_id:id.into(),
            option_ids:if text.is_empty() { vec![question.options[0].id.clone()] } else { vec![] },
            text:text.into(), expected_revision:None,
        }, "typed", "mac", (Some(1), origin)).unwrap()
    }
}
impl Drop for Fixture { fn drop(&mut self) { drop(std::fs::remove_dir_all(&self.root)); } }

#[test]
fn durable_ledger_replay_keeps_the_count_without_a_stats_file() {
    let f = Fixture::new();
    let path = f.root.join("ledger.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("Fixture", &EntityId::parse("fixture").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let original = ledger.record_prompt_received_from_intake(&binding, "fucking-shit", Source::Text, 1).unwrap();
    ledger.record_prompt_received_spoken(&binding, "shit shit", Source::Jam, false).unwrap();
    ledger.record_prompt_received_pressed(&binding, "fuck", Source::Text).unwrap();
    ledger.replay_turn(&binding, &original).unwrap();
    assert_eq!(ledger.swearmeter().count(), 3);
    drop(ledger);
    let mut reopened = Ledger::open(&path).unwrap();
    assert_eq!(reopened.swearmeter().count(), 3);
    let binding = reopened.thread_binding(&thread).unwrap();
    reopened.record_prompt_received(&binding, "fucking-shit", Source::Text).unwrap();
    assert_eq!(reopened.swearmeter().count(), 4);
}

#[test]
fn card_inputs_count_when_saved_before_any_set_or_job_is_delivered() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    f.answer(&questions[0], "first", "the fucking-shit build", AnswerOrigin::Direct);
    assert!(f.store.pending_threads().unwrap().is_empty());
    assert_eq!(f.cards.lock().unwrap().count, 1);
    f.answer(&questions[0], "first", "the fucking-shit build", AnswerOrigin::Conversation);
    assert_eq!(f.cards.lock().unwrap().count, 1); // Retry retains the original origin.
    f.answer(&questions[1], "option", "", AnswerOrigin::Direct);
    f.store.deliver("fixture", "thread", "front_desk", |d| Ok(format!("sink:{}", d.id))).unwrap();
    assert_eq!(f.cards.lock().unwrap().count, 1);
    assert_eq!(Store::new(&f.root).swearmeter_projection().unwrap().count, 1);
    let team = f.ask("job-fixture");
    f.answer(&team[0], "team", "that damn build", AnswerOrigin::Direct);
    assert_eq!(f.cards.lock().unwrap().count, 2);
}

#[test]
fn rich_recording_a_conversation_answer_adds_no_words_on_any_later_sink() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    f.answer(&questions[0], "model", "the fucking build", AnswerOrigin::Conversation);
    f.answer(&questions[1], "human", "shitshow", AnswerOrigin::Direct);
    assert_eq!(f.cards.lock().unwrap().count, 1);
    f.store.deliver_set("fixture", "thread", "front_desk", &questions[0].set_id,
        |delivery| Ok(format!("tool-result:{}", delivery.id))).unwrap();
    let reopened = Store::new(&f.root).swearmeter_projection().unwrap();
    assert_eq!(reopened.count, 1);
    assert_eq!(reopened.unknown_inputs, 0);
}

#[test]
fn new_identical_card_submissions_count_and_invalid_unsaved_requests_do_not() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    f.answer(&questions[0], "first", "shit", AnswerOrigin::Direct);
    assert_eq!(f.answer(&questions[0], "again", "shit", AnswerOrigin::Direct).outcome, "already_answered");
    f.answer(&questions[0], "later", "fuck", AnswerOrigin::Direct);
    assert_eq!(f.cards.lock().unwrap().count, 3);
    let invalid = AnswerRequest { question_id:questions[1].id.clone(), client_id:String::new(),
        option_ids:vec![], text:"fuck".into(), expected_revision:None };
    assert!(f.store.answer_from("fixture", "thread", invalid, "typed", "mac", (None, AnswerOrigin::Direct)).is_err());
    assert_eq!(f.cards.lock().unwrap().count, 3);
    assert_eq!(Store::new(&f.root).swearmeter_projection().unwrap().count, 3);
}

#[test]
fn unknown_legacy_origins_are_partial_only_for_nonempty_text() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    f.answer(&questions[0], "legacy-text", "shit", AnswerOrigin::Unknown);
    f.answer(&questions[1], "legacy-option", "", AnswerOrigin::Unknown);
    let counts = Store::new(&f.root).swearmeter_projection().unwrap();
    assert_eq!(counts.count, 0);
    assert_eq!(counts.unknown_inputs, 1);
    let mut forged = serde_json::json!({"question_id":"q", "client_id":"c", "text":"fuck", "origin":"direct"});
    assert!(serde_json::from_value::<AnswerRequest>(forged.clone()).is_err());
    forged.as_object_mut().unwrap().remove("origin");
    assert!(serde_json::from_value::<AnswerRequest>(forged).is_ok());
}

mod support;

#[test]
fn attachment_authorship_survives_a_durable_queue_and_a_restart() {
    use richos_core::{cognition::MockLeaseFactory, steering::TurnControl};
    let f = Fixture::new();
    let path = f.root.join("ledger.jsonl");
    let intake = f.root.join("intake.jsonl");
    let entity = EntityId::parse("femcboost").unwrap();
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let thread = spine.create_thread("Attachments", &entity).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Ready", "Received"])));
    let text = "damn résumé";
    let folded = format!("{text}\n\nAttached from the phone (1 file):\n/fixture/wtf.png");
    let control = TurnControl::open(&intake).unwrap();
    control.submit_authored_from_channel(&thread, Some(entity), &folded, "phone", Some("attachment"), text.len()).unwrap();
    drop(control);
    spine.set_turn_control(TurnControl::open(&intake).unwrap());
    spine.poll_intake().unwrap();
    assert_eq!(spine.ledger().swearmeter().count(), 1);
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let turns = spine.ledger().thread_turns_scoped(&binding).unwrap();
    let turn = turns.iter().find(|t| t.user_text == folded).expect("full model message is preserved");
    assert_eq!(turn.authored_bytes, Some(text.len()));
    drop(spine);
    let mut ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.swearmeter().count(), 1);
    assert_eq!(ledger.swearmeter().unknown_inputs(), 0);
    let binding = ledger.thread_binding(&thread).unwrap();
    // A deliberately pasted lookalike is wholly human text, including its filename.
    ledger.record_prompt_received(&binding, &folded, Source::Text).unwrap();
    assert_eq!(ledger.swearmeter().count(), 3);
}

#[test]
fn queued_words_count_before_delivery_and_never_double_during_the_drain() {
    use richos_core::{cognition::MockLeaseFactory, steering::TurnControl, read_view::SpineView};
    let f = Fixture::new();
    let mut spine = support::spine(Ledger::open(f.root.join("ledger")).unwrap());
    let entity = EntityId::parse("femcboost").unwrap();
    let thread = spine.create_thread("Queued", &entity).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Ready", "Received"])));
    let control = TurnControl::open(f.root.join("intake")).unwrap();
    spine.set_turn_control(control.clone());
    let reader = spine.reader();
    control.submit_from_desk(&thread, Some(entity), "shit").unwrap();
    let (view, pending, unknown, complete) = control.swearmeter_inputs(&reader);
    assert_eq!((view.ledger().swearmeter().count(), pending, unknown, complete), (0, 1, 0, true));
    drop(view);
    let reopened = TurnControl::open(f.root.join("intake")).unwrap();
    assert_eq!(reopened.swearmeter_inputs(&reader).1, 1);
    drop(reopened);
    spine.poll_intake().unwrap();
    let (view, pending, unknown, complete) = control.swearmeter_inputs(&reader);
    assert_eq!((view.ledger().swearmeter().count(), pending, unknown, complete), (1, 0, 0, true));
    drop(view);
    // The same phone client key may be retried before or after the original drains.
    control.submit_from_channel_as(&thread, None, "damn", "phone", Some("retry")).unwrap();
    control.submit_from_channel_as(&thread, None, "damn", "phone", Some("retry")).unwrap();
    assert_eq!(control.swearmeter_inputs(&reader).1, 1);
    spine.poll_intake().unwrap();
    control.submit_from_channel_as(&thread, None, "damn", "phone", Some("retry")).unwrap();
    let (view, pending, _, _) = control.swearmeter_inputs(&reader);
    assert_eq!(view.ledger().swearmeter().count() + pending, 2);
}

#[test]
fn display_preference_persists_and_a_failed_write_rolls_it_back() {
    use richos_core::config::ConfigStore;
    let f = Fixture::new();
    let path = f.root.join("config.json");
    let mut config = ConfigStore::open(&path).unwrap();
    assert!(!config.show_swearmeter());
    config.set_show_swearmeter(true).unwrap();
    assert!(ConfigStore::open(&path).unwrap().show_swearmeter());
    config.set_show_swearmeter(false).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(config.set_show_swearmeter(true).is_err());
    assert!(!config.show_swearmeter());
}

#[test]
fn a_crash_between_a_card_save_and_cache_update_recovers_from_the_request() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    let request = AnswerRequest { question_id: questions[0].id.clone(), client_id: "saved-before-crash".into(),
        option_ids: vec![], text: "shit".into(), expected_revision: None };
    // A detached writer performs the durable transaction without a runtime callback.
    Store::new(&f.root).answer_from("fixture", "thread", request.clone(), "phone_typed", "phone", (Some(1), AnswerOrigin::Direct)).unwrap();
    assert_eq!(f.cards.lock().unwrap().count, 0);
    let recovered = Store::new(&f.root).swearmeter_projection().unwrap();
    assert_eq!(recovered.count, 1);
    *f.cards.lock().unwrap() = recovered;
    f.store.answer_from("fixture", "thread", request, "phone_typed", "phone", (Some(2), AnswerOrigin::Direct)).unwrap();
    assert_eq!(f.cards.lock().unwrap().count, 1);
}

#[test]
fn failed_durable_acceptance_adds_nothing_and_stopping_an_accepted_turn_subtracts_nothing() {
    let f = Fixture::new();
    let path = f.root.join("ledger");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("Stopped", &EntityId::parse("fixture").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let turn = ledger.record_prompt_received(&binding, "damn", Source::Text).unwrap();
    ledger.stop_turn(&turn, 1).unwrap();
    assert_eq!(ledger.swearmeter().count(), 1);
    let mut unrelated = Ledger::open(f.root.join("other-ledger")).unwrap();
    assert!(unrelated.record_prompt_received(&binding, "shit", Source::Text).is_err());
    assert_eq!(unrelated.swearmeter().count(), 0);
    assert_eq!(ledger.swearmeter().count(), 1);
}


fn save_checkpoint(f: &Fixture, ledger: &Ledger) -> PathBuf {
    use richos_core::swearmeter_checkpoint::{CachedCards, Checkpoint};
    let (_, cards, source) = f.store.swearmeter_startup_cached(None).unwrap();
    let path = f.root.join("swearmeter-cache");
    Checkpoint::save(&path, &f.root.join("ledger"), &f.root.join("questions/store.json"), ledger,
        CachedCards { source, summary: cards.summary() }).unwrap();
    path
}

#[test]
fn a_bound_checkpoint_keeps_old_and_new_inputs_and_lineage_exact() {
    use richos_core::swearmeter_checkpoint::Checkpoint;
    let f = Fixture::new();
    let path = f.root.join("ledger");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("Cached", &EntityId::parse("fixture").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let first = ledger.record_prompt_received_from_intake(&binding, "shit", Source::Text, 1).unwrap();
    let second = ledger.record_prompt_received(&binding, "fuck fuck", Source::Text).unwrap();
    let questions = f.ask("front_desk");
    f.answer(&questions[0], "direct", "damn", AnswerOrigin::Direct);
    f.answer(&questions[1], "legacy", "shit", AnswerOrigin::Unknown);
    let cache_path = save_checkpoint(&f, &ledger);
    drop(ledger);
    let cache = Checkpoint::load(&cache_path, &path, &f.root.join("questions/store.json")).unwrap();
    let (order, cards, _) = f.store.swearmeter_startup_cached(Some(&cache.cards)).unwrap();
    assert_eq!(order, f.store.highest_order().unwrap());
    assert_eq!((cards.count, cards.unknown_inputs), (1, 1));
    *f.cards.lock().unwrap() = cards;
    // The restored summary has no historical seen-key set. The transaction itself
    // identifies retries, so a retry cannot add to the restored total.
    f.answer(&questions[0], "direct", "damn", AnswerOrigin::Direct);
    assert_eq!(f.cards.lock().unwrap().count, 1);
    let mut ledger = Ledger::open_cached(&path, Some((cache.ledger_source, cache.ledger))).unwrap();
    assert!(ledger.swearmeter().checkpoint_reused());
    assert_eq!(ledger.swearmeter().count(), 3);
    ledger.record_prompt_received_from_intake(&binding, "shit", Source::Text, 1).unwrap();
    ledger.replay_turn(&binding, &first).unwrap();
    assert_eq!(ledger.swearmeter().count(), 3);
    ledger.mark_turn_superseded(&first, &second).unwrap();
    assert_eq!(ledger.swearmeter().count(), 1);
    assert!(ledger.freeze_swearmeter().valid());
    let cache_path = save_checkpoint(&f, &ledger);
    drop(ledger);
    let cache = Checkpoint::load(&cache_path, &path, &f.root.join("questions/store.json")).unwrap();
    let mut ledger = Ledger::open_cached(&path, Some((cache.ledger_source, cache.ledger))).unwrap();
    assert_eq!(ledger.swearmeter().count(), 1);
    ledger.record_prompt_received(&binding, "shit", Source::Text).unwrap();
    assert_eq!(ledger.swearmeter().count(), 2);
    f.answer(&questions[0], "another-send", "damn", AnswerOrigin::Direct);
    assert_eq!(f.cards.lock().unwrap().count, 2);
}

#[test]
fn cache_misses_rebuild_after_corruption_either_source_change_or_file_replacement() {
    use richos_core::swearmeter_checkpoint::Checkpoint;
    let f = Fixture::new();
    let path = f.root.join("ledger");
    let cards = f.root.join("questions/store.json");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("Cached", &EntityId::parse("fixture").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    ledger.record_prompt_received(&binding, "shit", Source::Text).unwrap();
    let questions = f.ask("front_desk");
    let cache_path = save_checkpoint(&f, &ledger);
    assert!(Checkpoint::load(&cache_path, &path, &cards).is_some());
    std::fs::write(&cache_path, b"damaged cache").unwrap();
    assert!(Checkpoint::load(&cache_path, &path, &cards).is_none());
    save_checkpoint(&f, &ledger);
    f.answer(&questions[0], "after-checkpoint", "damn", AnswerOrigin::Direct);
    assert!(Checkpoint::load(&cache_path, &path, &cards).is_none());
    assert_eq!(f.store.swearmeter_projection().unwrap().count, 1);
    save_checkpoint(&f, &ledger);
    ledger.record_prompt_received(&binding, "fuck", Source::Text).unwrap();
    assert!(Checkpoint::load(&cache_path, &path, &cards).is_none());
    assert_eq!(Ledger::open(&path).unwrap().swearmeter().count(), 2);
    save_checkpoint(&f, &ledger);
    // Same bytes, same modification time and same pathname, different file generation.
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let replacement = f.root.join("replacement");
    std::fs::copy(&path, &replacement).unwrap();
    std::fs::File::open(&replacement).unwrap().set_modified(modified).unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert!(Checkpoint::load(&cache_path, &path, &cards).is_none());
    assert_eq!(Ledger::open(&path).unwrap().swearmeter().count(), 2);
}

#[test]
fn a_stale_checkpoint_cannot_hide_a_crash_after_acceptance_or_a_detector_change() {
    use richos_core::swearmeter_checkpoint::Checkpoint;
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let path = f.root.join("ledger");
    let cards = f.root.join("questions/store.json");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("Cached", &EntityId::parse("fixture").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    ledger.record_prompt_received(&binding, "shit", Source::Text).unwrap();
    let cache_path = save_checkpoint(&f, &ledger);
    let cache = Checkpoint::load(&cache_path, &path, &cards).unwrap();
    ledger.record_prompt_received(&binding, "fuck", Source::Text).unwrap();
    drop(ledger);
    // Even a cache loaded just before the new durable write is revalidated by open.
    let ledger = Ledger::open_cached(&path, Some((cache.ledger_source, cache.ledger))).unwrap();
    assert_eq!(ledger.swearmeter().count(), 2);
    save_checkpoint(&f, &ledger);
    let bytes = std::fs::read(&cache_path).unwrap();
    let mut payload: serde_json::Value = serde_json::from_slice(&bytes[32..]).unwrap();
    payload["version"] = serde_json::json!(999);
    let payload = serde_json::to_vec(&payload).unwrap();
    let mut changed = Sha256::digest(&payload).to_vec();
    changed.extend(&payload);
    std::fs::write(&cache_path, changed).unwrap();
    assert!(Checkpoint::load(&cache_path, &path, &cards).is_none());
    assert_eq!(Ledger::open(&path).unwrap().swearmeter().count(), 2);
}

#[test]
fn an_earlier_card_snapshot_is_refreshed_if_acceptance_changed_its_generation() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    let startup = f.store.swearmeter_startup_cached(None).unwrap();
    assert_eq!(startup.1.count, 0);
    f.answer(&questions[0], "accepted-during-startup", "shit", AnswerOrigin::Direct);
    let refreshed = f.store.swearmeter_refresh_startup(startup).unwrap();
    assert_eq!(refreshed.1.count, 1);
    assert_eq!(refreshed.0, f.store.highest_order().unwrap());
}

#[test]
fn duplicate_or_unreadable_operation_inputs_are_unavailable_instead_of_complete_zero() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    f.answer(&questions[0], "direct", "shit", AnswerOrigin::Direct);
    let path = f.root.join("questions/store.json");
    let mut data: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let operation = data["operations"].as_object().unwrap().values().next().unwrap().clone();
    data["operations"] = serde_json::json!({"bad": {"request":"unreadable", "result":{}, "origin":"direct"}});
    std::fs::write(&path, data.to_string()).unwrap();
    assert!(!f.store.swearmeter_projection().unwrap().available);
    let single = serde_json::json!({"questions":[], "operations":{}}).to_string();
    let duplicated = single.replace("\"operations\":{}", &format!("\"operations\":{{\"same\":{operation},\"same\":{operation}}}"));
    std::fs::write(&path, duplicated).unwrap();
    assert!(!f.store.swearmeter_projection().unwrap().available);
}

#[test]
fn edited_attachment_authorship_survives_intake_ledger_checkpoint_and_replay() {
    use richos_core::{cognition::MockLeaseFactory, steering::TurnControl, swearmeter::Authorship, read_view::SpineView};
    use richos_core::swearmeter_checkpoint::Checkpoint;
    let f = Fixture::new();
    let path = f.root.join("ledger");
    let mut spine = support::spine(Ledger::open(&path).unwrap());
    let entity = EntityId::parse("femcboost").unwrap();
    let thread = spine.create_thread("Edited attachments", &entity).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Ready", "Received"])));
    let control = TurnControl::open(f.root.join("intake")).unwrap();
    spine.set_turn_control(control.clone());
    let reader = spine.reader();
    let body = "shit\n\nAttached on this Mac (1 file, saved by RichOS):\n/fixture/wtf.png\n\nand damn";
    let last = body.rfind("and damn").unwrap();
    control.submit_from_desk_provenance(&thread, Some(entity), body, Authorship { bytes: None, ranges: Some(vec![(0,4),(last,body.len())]) }).unwrap();
    assert_eq!(control.swearmeter_inputs(&reader).1, 2);
    spine.poll_intake().unwrap();
    let view = reader.snapshot();
    assert_eq!(view.ledger().swearmeter().count(), 2);
    let turn = view.ledger().turns().iter().find(|t| t.user_text == body).unwrap().id.clone();
    drop(view);
    save_checkpoint(&f, spine.ledger());
    drop(spine);
    let cache = Checkpoint::load(&f.root.join("swearmeter-cache"), &path, &f.root.join("questions/store.json")).unwrap();
    let mut ledger = Ledger::open_cached(&path, Some((cache.ledger_source, cache.ledger))).unwrap();
    assert!(ledger.swearmeter().checkpoint_reused());
    let binding = ledger.thread_binding(&thread).unwrap();
    ledger.replay_turn(&binding, &turn).unwrap();
    assert_eq!(ledger.swearmeter().count(), 2);
    drop(ledger);
    assert_eq!(Ledger::open(&path).unwrap().swearmeter().count(), 2);
}

#[test]
fn damaged_numeric_cache_cannot_reject_or_forget_a_durable_card_answer() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    let cache = f.cards.clone();
    assert!(std::thread::spawn(move || {
        let _guard = cache.lock().unwrap();
        panic!("synthetic cache failure");
    }).join().is_err());
    f.answer(&questions[0], "after-cache-failure", "shit", AnswerOrigin::Direct);
    assert_eq!(swearmeter_questions::lock(&f.cards).count, 1);
    assert!(!swearmeter_questions::lock(&f.cards).available);
    assert!(swearmeter_questions::rebuild(&f.cards, || Err("source unreadable".into())).is_err());
    assert!(f.cards.is_poisoned());
    swearmeter_questions::rebuild(&f.cards, || f.store.swearmeter_projection()).unwrap();
    assert!(!f.cards.is_poisoned());
    assert!(swearmeter_questions::lock(&f.cards).available);
    assert_eq!(swearmeter_questions::lock(&f.cards).count, 1);
    f.answer(&questions[0], "after-cache-failure", "shit", AnswerOrigin::Direct);
    assert_eq!(swearmeter_questions::lock(&f.cards).count, 1);
    f.answer(&questions[1], "new-input-after-recovery", "damn", AnswerOrigin::Direct);
    assert_eq!(swearmeter_questions::lock(&f.cards).count, 2);
}

#[test]
fn source_rebuild_retains_identities_for_callbacks_waiting_after_the_save() {
    let f = Fixture::new();
    let questions = f.ask("front_desk");
    // Durable acceptance has finished; its statistics callback has not run yet.
    Store::new(&f.root).answer_from("fixture", "thread", AnswerRequest {
        question_id:questions[0].id.clone(), client_id:"saved-before-rebuild".into(),
        option_ids:vec![], text:"shit".into(), expected_revision:None,
    }, "typed", "mac", (Some(1), AnswerOrigin::Direct)).unwrap();
    swearmeter_questions::rebuild(&f.cards, || f.store.swearmeter_projection()).unwrap();
    let mut counts = swearmeter_questions::lock(&f.cards);
    assert_eq!(counts.count, 1);
    // The real callback's record operation arrives after source recovery.
    counts.record("fixture:thread:saved-before-rebuild", Some(AnswerOrigin::Direct), "shit");
    assert_eq!(counts.count, 1);
    counts.record("fixture:thread:next-real-input", Some(AnswerOrigin::Direct), "shit");
    assert_eq!(counts.count, 2);
}
