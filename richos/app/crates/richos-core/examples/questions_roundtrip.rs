//! Opt-in real-provider proof. Uses a disposable directory supplied on the command line.
//! cargo run -p richos-core --example questions_roundtrip -- /external/scratch/directory
//! The same executable hosts the production MCP servers for its Claude children.
use richos_core::{
    entity::{Entity, EntityId, EntityRegistry},
    ledger::{Ledger, Source},
    native::{resolve_claude_bin, NativeCognition},
    questions::{AnswerRequest, Store},
    spine::Spine,
    Cognition,
};
use std::path::{Path, PathBuf};
fn attach(spine: &mut Spine, root: &Path, doctrine: &Path, skills: &Path) {
    let lease = NativeCognition::start_with_onboarding(
        &resolve_claude_bin(),
        root,
        doctrine,
        skills,
        &std::env::current_exe().unwrap(),
        None,
    )
    .expect("real Claude handshake");
    eprintln!("real provider session {}", lease.session_id());
    spine.attach_lease(Box::new(lease));
}
fn open(root: &Path) -> Spine {
    let mut spine = Spine::new(Ledger::open(root.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(
        EntityRegistry::new(vec![Entity::new(
            "fixture",
            "Fixture",
            &[root.to_str().unwrap()],
        )
        .unwrap()])
        .unwrap(),
    );
    spine.set_central_root(root.join("central"));
    spine.set_onboarding_record(root.join("onboarding.json"));
    spine
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s.starts_with("--")) {
        let scope = Path::new(&args[1]);
        match args[0].as_str() {
            "--questions-mcp" => richos_core::question_tools::run_stdio(scope).unwrap(),
            "--onboarding-mcp" => richos_core::onboarding_tools::run_stdio(scope).unwrap(),
            "--assignments-mcp" => richos_core::assignment_tools::run_stdio(scope).unwrap(),
            "--status-mcp" => {
                richos_core::status_tools::run_stdio(scope, &richos_core::screen::UnknownScreen)
                    .unwrap()
            }
            _ => panic!("unknown server"),
        }
        return;
    }
    let root = PathBuf::from(args.first().expect("supply a disposable scratch directory"));
    assert!(root.is_absolute());
    std::fs::create_dir_all(&root).unwrap();
    let doctrine = richos_core::doctrine::ensure_rendered(
        &root,
        &richos_core::doctrine::DoctrineIdentity::new(Some("Fixture CEO")),
    )
    .unwrap();
    let skills = richos_core::skills::ensure_rendered(&root, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    let mut spine = open(&root);
    let thread = spine
        .create_thread(
            "Non-blocking question proof",
            &EntityId::parse("fixture").unwrap(),
        )
        .unwrap();
    attach(&mut spine, &root, &doctrine, &skills);
    let prompt="This is a synthetic UI verification. Ask one prepared question using richos_questions.ask: When should the release ship? Two options: Ship today (Earlier fixes) and Ship tomorrow (More testing). Do not inspect files, run commands or dispatch work. The whole response should be that question.";
    let turn = spine
        .submit_prompt(prompt, Source::Text)
        .expect("asking turn");
    let store = Store::new(root.join("engine-state"));
    let q = store
        .list("fixture", &thread)
        .unwrap()
        .pop()
        .expect("real provider posted a question");
    assert_eq!(
        spine.ledger().turn(&turn).unwrap().stop_reason.as_deref(),
        Some("question_asked")
    );
    assert!(q.shown.is_none());
    drop(spine);
    let store = Store::new(root.join("engine-state"));
    let restored = store.list("fixture", &thread).unwrap().pop().unwrap();
    assert_eq!(restored.id, q.id);
    let choice = restored
        .options
        .iter()
        .find(|o| o.label == "Ship tomorrow")
        .expect("spoken-safe option");
    store
        .answer(
            "fixture",
            &thread,
            AnswerRequest {
                question_id: q.id.clone(),
                client_id: "phone-after-restart".into(),
                option_ids: vec![choice.id.clone()],
                text: String::new(),
                expected_revision: None,
            },
            "phone_tap",
            "phone",
        )
        .unwrap();
    let mut spine = open(&root);
    spine.switch_thread(&thread).unwrap();
    attach(&mut spine, &root, &doctrine, &skills);
    assert_eq!(
        store
            .deliver("fixture", &thread, "front_desk", |d| spine
                .queue_question_answer(d)
                .map_err(|e| e.to_string()))
            .unwrap(),
        1
    );
    spine.poll_intake().expect("answer continuation");
    assert_eq!(
        store
            .deliver("fixture", &thread, "front_desk", |_| panic!(
                "duplicate answer turn"
            ))
            .unwrap(),
        0
    );
    let replies: Vec<_> = spine
        .messages(&thread)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.text)
        .collect();
    assert!(
        !replies.is_empty(),
        "answer did not reach a real receiving turn"
    );
    println!(
        "{}",
        serde_json::json!({"ask_turn_released":true,"restored_question":q.id,"phone_answer_delivered_once":true,"replies":replies})
    );
}
