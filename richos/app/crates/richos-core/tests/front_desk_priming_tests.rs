//! Company context stays separate from original input and never requires a hidden model turn.
use richos_core::cognition::MockCognition;
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger,Source};
use richos_core::spine::FrontDeskReady;
mod support;
fn femcboost()->EntityId {EntityId::parse("femcboost").unwrap()}
fn tmp_ledger(tag:&str)->(std::path::PathBuf,Ledger) {
    let path=std::env::temp_dir().join(format!("company-context-{tag}-{}.jsonl",uuid::Uuid::new_v4()));
    let ledger=Ledger::open(&path).unwrap();
    (path,ledger)
}

type CompanyDeliveryLog = std::sync::Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;

struct InlineCompany {
    mock: MockCognition,
    current: Option<String>,
    sent: CompanyDeliveryLog,
}

impl richos_core::cognition::Cognition for InlineCompany {
    fn session_id(&self) -> &str { self.mock.session_id() }
    fn set_company_context(&mut self,context:Option<&str>)->Result<bool,richos_core::cognition::CognitionError> {
        self.current=context.map(str::to_string);
        Ok(true)
    }


    fn prompt(&mut self, text: &str, sink: &mut dyn FnMut(richos_core::cognition::TurnItem))
        -> Result<String, richos_core::cognition::CognitionError> {
        self.sent.lock().unwrap().push((text.to_string(), self.current.clone()));
        self.mock.prompt(text, sink)
    }
}

#[test]
fn a_thread_that_does_not_exist_is_refused_rather_than_created_or_guessed() {
    // Fails closed, and says which fact is missing. A priming call that CREATED a thread would
    // put a thread in his sidebar because the app started.
    let (path, ledger) = tmp_ledger("no-such-thread");
    let mut spine = support::spine(ledger);
    let before = spine.threads().len();
    let verdict = spine.prime_front_desk("thr_nothing_here");
    match verdict {
        FrontDeskReady::NotReady(why) => assert!(!why.is_empty(), "the refusal must name the fault"),
        other => panic!("an unknown thread must be refused: {other:?}"),
    }
    assert_eq!(spine.threads().len(), before, "priming must never create a thread");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rich_is_given_the_company_name_exactly_as_he_typed_it() {
    // The CEO, 2026-10-13 (onboarding2.png): he typed a name with a word in capitals and Rich
    // wrote it back re-capitalized, because the priming named the company only by its id, and
    // the id is lower case. Rich has to be told the name itself, letter for letter, whether or
    // not the company has notes on file.
    let (path, ledger) = tmp_ledger("company-name");
    let mut spine = richos_core::spine::Spine::new(ledger);
    spine.set_entity_registry(
        richos_core::entity::EntityRegistry::new(vec![
            richos_core::entity::Entity::new("brightwave-labs", "Brightwave LABS", &["/fixture/brightwave"]).unwrap(),
        ])
        .unwrap(),
    );
    spine.create_thread("Onboarding", &EntityId::parse("brightwave-labs").unwrap()).unwrap();
    let lease = MockCognition::new("sess-one", vec!["On it!"]);
    let reprimes = lease.company_contexts.clone();
    spine.attach_lease(Box::new(lease));
    assert!(reprimes.lock().unwrap().is_empty(),"opening paid a hidden context delivery");
    spine.submit_prompt("Tell me about this company",Source::Text).unwrap();
    let priming = reprimes.lock().unwrap()[0].clone();
    assert!(priming.contains("\"Brightwave LABS\""), "the name as he typed it is in the priming: {priming}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&priming).unwrap()["company_name"],"Brightwave LABS");
    std::fs::remove_file(&path).expect("the test ledger is removable");
}

#[test]
fn saved_interview_answers_reach_the_next_request_without_repriming_the_session() {
    let (path, ledger) = tmp_ledger("inline-company");
    let root = std::env::temp_dir().join(format!("inline-company-{}", uuid::Uuid::new_v4()));
    let company = richos_core::company::company_file(&root, &femcboost());
    std::fs::create_dir_all(company.parent().unwrap()).unwrap();
    std::fs::write(&company, "# Company\nWe sell bicycle parts.\n").unwrap();
    let mut spine = support::spine(ledger);
    spine.set_central_root(root.clone());
    let thread = spine.create_thread("Interview", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    let mock = MockCognition::new("inline-company", vec!["First answer", "Second answer"]);
    let primes = mock.reprimes.clone();
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(InlineCompany { mock, current: None, sent: sent.clone() }));
    spine.prime_front_desk(&thread);
    let first = spine.submit_prompt("What do you know?", Source::Text).unwrap();
    assert!(sent.lock().unwrap()[0].1.as_deref().unwrap().contains("We sell bicycle parts."));
    std::fs::write(&company, "# Company\nWe repair bicycles. Our next priority is service bookings.\n").unwrap();
    let second = spine.submit_prompt("What comes next?", Source::Text).unwrap();
    assert_eq!(primes.lock().unwrap().len(), 0, "a saved answer caused another hidden model turn");
    let delivered = sent.lock().unwrap();
    assert_eq!(delivered[1].0, "What comes next?");
    let context = delivered[1].1.as_deref().unwrap();
    assert!(context.contains("Our next priority is service bookings."));
    assert!(!context.contains("We sell bicycle parts."));
    assert_eq!(spine.ledger().turn(&first).unwrap().user_text, "What do you know?");
    assert_eq!(spine.ledger().turn(&second).unwrap().user_text, "What comes next?");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
