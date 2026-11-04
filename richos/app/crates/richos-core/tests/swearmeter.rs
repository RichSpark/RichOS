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
