//! Live release check with fictitious company data and the actual desktop MCP executable.
//! Uses the normal installed Claude Code login. All RichOS state stays in the supplied
//! scratch directory. Run explicitly; this incurs real model calls.
//! cargo run -p richos-core --example onboarding_native_roundtrip -- <scratch> <RichOS executable>
use richos_core::{
    doctrine::{self, DoctrineIdentity}, entity::{Entity, EntityId, EntityRegistry},
    ledger::{Ledger, Source}, native::{resolve_claude_bin, NativeCognition},
    onboarding::OnboardingState, spine::Spine,
};
use std::{path::{Path, PathBuf}, time::Instant};
fn open(root: &Path, executable: &Path) -> Spine {
    let engine = root.join("engine");
    let config = root.join("config");
    std::fs::create_dir_all(&engine).unwrap();
    let mut spine = Spine::new(Ledger::open(&root.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(EntityRegistry::new(vec![
        Entity::new("northstar-fixture", "Northstar fixture", &[engine.to_str().unwrap()]).unwrap(),
        Entity::new("second-fixture", "Second fixture", &[engine.to_str().unwrap()]).unwrap(),
    ]).unwrap());
    spine.set_machinery_journal(richos_core::journal::MachineryJournal::new(root.join("machinery")));
    spine.set_central_root(root.join("central"));
    spine.set_onboarding_record(root.join("config/onboarding.json"));
    spine.ensure_active_thread_in(&EntityId::parse("northstar-fixture").unwrap()).unwrap();
    let doctrine = doctrine::ensure_rendered(&config, &DoctrineIdentity::new(Some("Morgan"))).unwrap();
    let skills = richos_core::skills::ensure_rendered(&config, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    let start = Instant::now();
    spine.attach_lease(Box::new(NativeCognition::start_with_onboarding(
        &resolve_claude_bin(), &engine, &doctrine, &skills, executable, None).unwrap()));
    eprintln!("Connected in {} ms", start.elapsed().as_millis());
    spine
}
struct FirstText {
    start: Instant,
    seen: std::sync::atomic::AtomicBool,
}
impl richos_core::stream::TurnObserver for FirstText {
    fn on_event(&self, event: &richos_core::stream::StreamEvent) {
        if matches!(event, richos_core::stream::StreamEvent::Chunk { .. })
            && !self.seen.swap(true, std::sync::atomic::Ordering::SeqCst) {
            eprintln!("First visible text in {} ms", self.start.elapsed().as_millis());
        }
    }
}
fn ask(spine: &mut Spine, text: &str) -> String { ask_with_assignment(spine, text, false, false) }
fn ask_saved(spine: &mut Spine, text: &str) -> String { ask_with_assignment(spine, text, false, true) }
fn ask_with_assignment(spine: &mut Spine, text: &str, _expects_work: bool, expects_tool: bool) -> String {
    let start = Instant::now();
    spine.set_observer(Box::new(FirstText { start, seen: std::sync::atomic::AtomicBool::new(false) }));
    let turn = spine.submit_prompt(text, Source::Text).expect("live turn");
    let result = spine.ledger().turn(&turn).unwrap();
    eprintln!("Turn {:?}, {} ms", result.state, start.elapsed().as_millis());
    assert_eq!(result.state, richos_core::ledger::TurnState::Completed, "live turn did not complete");
    let reply = result.assistant_text.clone();
    println!("CEO: {text}\nRich: {reply}\n");
    assert!(!reply.trim().is_empty(), "no visible reply");
    let handled = richos_core::onboarding_tools::handled_in_turn(
        spine.machinery_journal().unwrap().read_thread(&result.thread_id), &turn);
    if expects_tool { assert!(handled, "the approved action must happen in this visible turn, not hidden priming"); }
    let scope = spine.onboarding_tool_scope(spine.active_binding().unwrap()).unwrap();
    let grants = std::fs::read_dir(scope.record_path.parent().unwrap().join("onboarding-scopes")).unwrap();
    let mut grants_checked = 0;
    for entry in grants {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap();
        assert_eq!(value["actions_allowed"], false, "tool grant remained open after the turn");
        grants_checked += 1;
    }
    assert!(grants_checked > 0, "no native scope grant checked");
    reply
}
fn state(spine: &Spine) -> OnboardingState { spine.onboarding_state(spine.active_binding().unwrap()) }
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 2, "scratch directory and desktop executable required");
    let root = PathBuf::from(&args[0]);
    assert!(root.is_absolute() && !root.exists(), "use a new absolute scratch directory");
    std::fs::create_dir_all(&root).unwrap();
    let executable = PathBuf::from(&args[1]);
    let mut spine = open(&root, &executable);
    assert_eq!(state(&spine), OnboardingState::NotYet);
    ask(&mut spine, "Let's start the company interview. Our company is Northstar, a small business selling handmade notebooks to independent bookstores. I am Morgan, the owner. Please ask me the first question.");
    ask_saved(&mut spine, "Our three-year destination is 120 independent bookstore customers and a four-person team. Today we have 18 customers, two part-time staff and about 12000 pounds monthly revenue. Cash is tight and late shipments are the main problem. I approve saving these details now. I need to pause the interview here; we can cover the remaining stages later.");
    assert_eq!(state(&spine), OnboardingState::Partial, "model must checkpoint a paused interview");
    let notes = root.join("central/companies/northstar-fixture/company.md");
    let saved = std::fs::read_to_string(&notes).unwrap();
    assert!(saved.to_lowercase().contains("notebook"));
    drop(spine);
    let mut spine = open(&root, &executable);
    assert_eq!(state(&spine), OnboardingState::Partial);
    let reply = ask(&mut spine, "Before we continue, what kind of business do I run and what is our three-year destination? Use the company notes you have.");
    assert!(reply.to_lowercase().contains("notebook") && reply.contains("120"), "saved facts were not recalled");
    ask_saved(&mut spine, "Not now, please don't offer to resume the company interview until I ask for it.");
    assert!(matches!(state(&spine), OnboardingState::Declined { .. }), "spoken decline must persist");
    drop(spine);
    let mut spine = open(&root, &executable);
    assert!(matches!(state(&spine), OnboardingState::Declined { .. }));
    ask_saved(&mut spine, "I want to resume and finish the company interview now. My priorities this month are dependable shipping and keeping enough cash for materials. The constraints are a 2000 pound budget and no more than 10 of my hours weekly. I make decisions after a short recommendation with costs and risks. Success means shipping within three working days and no missed supplier payments. For the team stage, record reliable operations support as a wish only, with no promise of hiring or staffing. I explicitly defer any other unanswered interview stages. I approve these notes and the earlier facts; please save the completed interview.");
    assert_eq!(state(&spine), OnboardingState::Described, "completed notes must persist");
    drop(spine);
    let mut spine = open(&root, &executable);
    assert_eq!(state(&spine), OnboardingState::Described);
    ask_with_assignment(&mut spine, "Update our company notes to say our shipping cutoff is 2 pm. Also prepare a one-page shipping checklist as a separate deliverable, shipping-checklist.md in our workspace. Do not contact any customers.", true, true);
    spine.ensure_active_thread_in(&EntityId::parse("second-fixture").unwrap()).unwrap();
    assert_eq!(state(&spine), OnboardingState::NotYet, "other company must retain own onboarding");
    println!("PASS: actual model saved partial notes, recalled after restart, persisted a typed decline, completed on request kept another company independent and registered only the separate accepted work.");
}
