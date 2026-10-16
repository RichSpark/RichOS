//! Opt-in live native continuity probe with an entirely fictional private store.
//! cargo run -p richos-core --example continuity_roundtrip -- ENGINE PYTHON
use richos_core::{Cognition, EntityId, EntityRegistry, Ledger, Source, Spine};
use richos_core::ecs::EcsBridge;
use richos_core::native::{resolve_claude_bin, NativeCognition};
use serde_json::json;
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().map(|s| s == "--onboarding-mcp").unwrap_or(false) {
        richos_core::onboarding_tools::run_stdio(&PathBuf::from(&args[1]))?;
        return Ok(());
    }
    if !(2..=3).contains(&args.len()) { return Err("usage: continuity_roundtrip ENGINE PYTHON [DELIVERED_RUNTIME]".into()); }
    let engine = std::fs::canonicalize(&args[0])?;
    let python = std::fs::canonicalize(&args[1])?;
    let root = Scratch(std::env::temp_dir().join(format!("richos continuity {}", uuid::Uuid::new_v4())));
    if args.len() == 2 { std::fs::create_dir_all(root.0.join("coordination"))?; }
    std::fs::create_dir_all(root.0.join("corpus/ceo/records"))?;
    let doctrine = richos_core::doctrine::ensure_rendered(&root.0, &Default::default())?;
    let skills = richos_core::skills::ensure_rendered(&root.0, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app"))?;
    let bridge = EcsBridge::new(&python, &engine, &root.0.join("ecs"))?;
    let mut spine = Spine::new(Ledger::open(&root.0.join("ledger.jsonl"))?);
    let entity = EntityId::parse("depot")?;
    spine.set_entity_registry(EntityRegistry::from_existing_ids(&[entity]));
    let thread = spine.create_thread("Fictional continuity canary", &EntityId::parse("depot")?)?;
    spine.set_central_root(root.0.join("corpus"));
    spine.set_onboarding_record(root.0.join("onboarding.json"));
    let start = || -> Result<NativeCognition, Box<dyn std::error::Error>> {
        if let Some(runtime) = args.get(2) {
            let runtime = richos_core::runtime::EngineRuntime::load(&engine, Some(&PathBuf::from(runtime)))?;
            let profile = richos_core::engine_profile::EngineProfile::prepare(&engine, &root.0, runtime)?;
            Ok(NativeCognition::start_with_engine(&resolve_claude_bin(), &doctrine, &skills,
                &std::env::current_exe()?, bridge.clone(), profile, None)?)
        } else {
            Ok(NativeCognition::start_with_continuity(&resolve_claude_bin(), &root.0.join("coordination"),
                &root.0, &skills, &std::env::current_exe()?, bridge.clone(), None)?)
        }
    };
    let first = start()?;
    let old_session = first.session_id().to_string();
    spine.attach_lease(Box::new(first));
    spine.submit_prompt("This is a synthetic continuity test. Use mcp__richos_continuity__checkpoint to record exactly one commitment with id depot-manual and title Review the fictional depot manual, request_id canary-checkpoint. Do not perform the commitment. Reply only after checking the tool receipt.", Source::Text)?;
    let binding = bridge.request("current", json!({}))?["binding"].clone();
    let records = bridge.request("inspect", json!({"binding":binding,"query":{"section":"commitment"}}))?;
    assert_eq!(records["records"].as_array().unwrap().len(), 1, "actual model checkpoint missing");
    let second = start()?;
    assert_ne!(old_session, second.session_id());
    spine.attach_lease(Box::new(second));
    let turn = spine.submit_prompt("What obligation is still pending? Inspect continuity if needed. Do not execute it or mark it complete.", Source::Text)?;
    let reply = &spine.ledger().turn(&turn).unwrap().assistant_text;
    assert!(reply.to_lowercase().contains("manual"), "new process did not recover the obligation");
    assert_eq!(spine.active_thread(), Some(thread.as_str()));
    println!("{}", json!({"desktop_engine_profile":args.len()==3,"live_checkpoint":true,"new_session_recovered":true,"scope":"fictional","installed_acceptance":false}));
    drop(spine);
    Ok(())
}
