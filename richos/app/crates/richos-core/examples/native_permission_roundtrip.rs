//! Opt-in live desktop permission probe. All writes are fictional disposable files.
use richos_core::{Cognition, EntityId, EntityRegistry, Ledger, Source, Spine};
use richos_core::{ecs::EcsBridge, native::{resolve_claude_bin, NativeCognition}};
use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}}, time::{Duration,Instant}};
struct Scratch(PathBuf);
impl Drop for Scratch {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let args:Vec<_>=std::env::args_os().skip(1).collect();
 if args.first().is_some_and(|a|a=="--onboarding-mcp"){richos_core::onboarding_tools::run_stdio(&PathBuf::from(&args[1]))?;return Ok(());}
 if args.len()!=2{return Err("usage: native_permission_roundtrip ENGINE DELIVERED_RUNTIME".into());}
 let engine=std::fs::canonicalize(&args[0])?;
 let root=Scratch(std::env::temp_dir().join(format!("richos permission {}",uuid::Uuid::new_v4())));
 std::fs::create_dir_all(root.0.join("corpus/ceo/records"))?;
 let runtime=richos_core::runtime::EngineRuntime::load(&engine,Some(&PathBuf::from(&args[1])))?;
 let profile=richos_core::engine_profile::EngineProfile::prepare(&engine,&root.0,runtime.clone())?;
 let desk=profile.permissions.clone();
 let doctrine=richos_core::doctrine::ensure_rendered(&root.0,&Default::default())?;
 let skills=richos_core::skills::ensure_rendered(&root.0, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app"))?;
 let bridge=EcsBridge::new(&runtime.python,&engine,&root.0.join("ecs"))?;
 let cognition=NativeCognition::start_with_engine(&resolve_claude_bin(),&doctrine,&skills,&std::env::current_exe()?,bridge,profile,None)?;
 let session=cognition.session_id().to_string();
 let mut spine=Spine::new(Ledger::open(&root.0.join("ledger.jsonl"))?);
 spine.set_entity_registry(EntityRegistry::from_existing_ids(&[EntityId::parse("depot")?]));
 spine.create_thread("Fictional permission probe",&EntityId::parse("depot")?)?;
 spine.set_central_root(root.0.join("corpus"));spine.set_onboarding_record(root.0.join("onboarding.json"));spine.attach_lease(Box::new(cognition));
 let finished=Arc::new(AtomicBool::new(false));let (allowed,denied)=(Arc::new(AtomicUsize::new(0)),Arc::new(AtomicUsize::new(0)));
 let allowed_path=root.0.join("ALLOWED.txt");let denied_path=root.0.join("DENIED.txt");
 let responder={let finished=finished.clone();let allowed=allowed.clone();let denied=denied.clone();let expected=allowed_path.clone();let forbidden=denied_path.clone();
  std::thread::spawn(move||{let deadline=Instant::now()+Duration::from_secs(180);while !finished.load(Ordering::SeqCst)&&Instant::now()<deadline{
   if let Some(request)=desk.current(){assert_eq!(request.binding.session_id,session);let file=request.input["file_path"].as_str().map(PathBuf::from);
    let permit=request.tool=="Write"&&file.as_ref()==Some(&expected);
    if permit{allowed.fetch_add(1,Ordering::SeqCst);}else if file.as_ref()==Some(&forbidden){denied.fetch_add(1,Ordering::SeqCst);}
    desk.resolve(&request.id,permit).expect("live permission scope");
   } std::thread::sleep(Duration::from_millis(20));
  }})};
 let result=spine.submit_prompt(&format!("This is a disposable permission test. Use Write to create {} with content DENIED. The app will refuse; do not retry or use another tool. Then use Write to create {} with exactly ALLOWED. The app will permit that one action. Do not create any other files or call Bash. Report the two observed results. This request authorizes only these fictional test attempts.",denied_path.display(),allowed_path.display()),Source::Text);
 finished.store(true,Ordering::SeqCst);responder.join().unwrap();result?;
 assert!(allowed.load(Ordering::SeqCst)>0);assert!(denied.load(Ordering::SeqCst)>0);assert!(!denied_path.exists());assert_eq!(std::fs::read_to_string(allowed_path)?.trim(),"ALLOWED");
 println!("{}",serde_json::json!({"native_permission_allowed":true,"native_permission_denied":true,"real_desktop_profile":true,"installed_acceptance":false}));drop(spine);Ok(())
}
