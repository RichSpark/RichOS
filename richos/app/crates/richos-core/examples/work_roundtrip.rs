//! Opt-in real provider dispatch through the desktop work tools, with synthetic repos.
use richos_core::{Cognition, EntityId, EntityRegistry, Ledger, Source, Spine};
use richos_core::{ecs::EcsBridge, native::{resolve_claude_bin, NativeCognition}};
use std::path::PathBuf;
struct PermissionTrace;
impl richos_core::machinery::MachineryObserver for PermissionTrace {
 fn on_machinery(&self, record: &richos_core::machinery::MachineryRecord) {
  if record.kind == richos_core::machinery::MachineryKind::PermissionRequested {eprintln!("Native permission record: {}",serde_json::to_string(record).unwrap());}
 }
}
struct Scratch(PathBuf);
impl Drop for Scratch{fn drop(&mut self){if std::env::var_os("RICHOS_PROBE_KEEP_FIXTURE").is_some(){eprintln!("Retained fictional fixture: {}",self.0.display());}else{let _=std::fs::remove_dir_all(&self.0);}}}
fn verify_worker_attribution(spine: &Spine, thread: &str, turn: &str) {
 let records=spine.machinery_journal().unwrap().read_thread(thread);
 let reports:Vec<_>=records.iter().filter(|r|r.turn_id.as_deref()==Some(turn) && r.title=="Worker report").collect();
 assert!(!reports.is_empty(),"actual worker reports were not preserved separately");
 let visible=&spine.ledger().turn(turn).unwrap().assistant_text;
 for report in reports {
  let payload=report.payload.as_ref().expect("fictional report payload");
  assert!(payload["parent_tool_use_id"].is_string());
  if let Some(text)=payload["block"]["text"].as_str().filter(|s|s.len()>160) {
   assert!(!visible.contains(text),"a whole worker report leaked into the lead conversation");
  }
 }
}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let args:Vec<_>=std::env::args_os().skip(1).collect();
 if args.first().is_some_and(|a|a=="--onboarding-mcp"){richos_core::onboarding_tools::run_stdio(&PathBuf::from(&args[1]))?;return Ok(());}
 if !(args.len()==2 || (args.len()==3 && (args[2]=="--natural" || args[2]=="--revision"))){return Err("usage: work_roundtrip ENGINE DELIVERED_RUNTIME [--natural|--revision]".into());}
 let natural=args.len()==3 && args[2]=="--natural";
 let revision=args.len()==3 && args[2]=="--revision";
 let engine=std::fs::canonicalize(&args[0])?;
 let root=Scratch(std::env::temp_dir().join(format!("richos work {}",uuid::Uuid::new_v4())));
 let data=root.0.join("app data");std::fs::create_dir_all(data.join("corpus/ceo/records"))?;
 let runtime=richos_core::runtime::EngineRuntime::load(&engine,Some(&PathBuf::from(&args[1])))?;
 let mut profile=richos_core::engine_profile::EngineProfile::prepare(&engine,&data,runtime.clone())?;

 let mut registry=EntityRegistry::from_existing_ids(&[EntityId::parse("depot")?]);
 for name in ["first project","second project"]{
  let repo=root.0.join(name);std::fs::create_dir(&repo)?;
  registry=richos_core::repositories::connect(&registry,&EntityId::parse("depot")?,&repo,true,&runtime,&[&engine,&data])?.0;
 }
 registry.save(&data.join("entities.json"))?;
 let doctrine=richos_core::doctrine::ensure_rendered(&data,&Default::default())?;
 let skills=richos_core::skills::ensure_rendered(&data, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app"))?;
 let bridge=EcsBridge::new(&runtime.python,&engine,&data.join("ecs"))?;
 let mut spine=Spine::new(Ledger::open(&data.join("ledger.jsonl"))?);spine.set_entity_registry(registry);spine.set_machinery_journal(richos_core::journal::MachineryJournal::new(data.join("machinery")));spine.set_machinery_observer(Box::new(PermissionTrace));
 let thread=spine.create_thread("Fictional dispatch probe",&EntityId::parse("depot")?)?;
 profile.scope_to(&spine.ledger().thread_binding(&thread)?)?;
 let targets=profile.target_state();
 let cognition=NativeCognition::start_with_engine(&resolve_claude_bin(),&doctrine,&skills,&std::env::current_exe()?,bridge.clone(),profile,None)?;
 let session=cognition.session_id().to_string();
 spine.set_central_root(data.join("corpus"));spine.set_onboarding_record(data.join("onboarding.json"));spine.attach_lease(Box::new(cognition));
 if natural {
  let backlog=root.0.join("first project/fictional-backlog.md");
  std::fs::write(&backlog,"# Fictional delivery\n\nCreate marker.txt containing exactly FICTIONAL in each connected project. No other project files should change. Verify the committed bytes, obtain independent review, integrate into each local main branch and clean up eligible workspaces. Never publish.\n")?;
  for args in [vec!["add","fictional-backlog.md"],vec!["commit","-qm","Fictional backlog fixture"]] {
   let output=std::process::Command::new(&runtime.git).env_clear().env("PATH",runtime.path()).env("GIT_CONFIG_NOSYSTEM","1").env("GIT_CONFIG_GLOBAL","/dev/null")
    .arg("-C").arg(root.0.join("first project")).args(["-c","core.hooksPath=/dev/null","-c","commit.gpgSign=false","-c","user.name=Fixture","-c","user.email=fixture@example.invalid"]).args(args).output()?;
   assert!(output.status.success(),"could not commit the fictional backlog fixture");
  }
  let turn=spine.submit_prompt(&format!("Please complete the job in {} for both connected repositories. Treat it as one assignment. The backlog gives the success conditions. You may commit and integrate the changes locally. Do not publish anything.",backlog.display()),Source::Text)?;
  verify_worker_attribution(&spine,&thread,&turn);
  let mut workers=Vec::new();
  for partition in std::fs::read_dir(data.join("engine-state/work-receipts"))? {
   for file in std::fs::read_dir(partition?.path())? {
    let path=file?.path();if !path.extension().is_some_and(|e|e=="json"){continue;}
    let receipt:serde_json::Value=serde_json::from_slice(&std::fs::read(path)?)?;
    if receipt["request"]["role"]=="worker" {workers.push(receipt);}
   }
  }
  eprintln!("Natural assignment response: {}",spine.ledger().turn(&turn).unwrap().assistant_text);
  assert_eq!(workers.len(),2,"one actual implementation worker per repository");
  let obligation=workers[0]["request"]["obligation_id"].clone();
  for worker in &workers {
   assert_eq!(worker["request"]["obligation_id"],obligation);
   assert!(worker["agent_id"].is_string());
   assert_eq!(worker["integration"]["verified"],true,"natural assignment did not reach verified integration");
   assert_eq!(worker["integration"]["cleanup_pending"],serde_json::json!([]));
   let repo=std::path::Path::new(worker["request"]["repo"].as_str().unwrap());
   assert_eq!(std::fs::read_to_string(repo.join("marker.txt"))?.trim(),"FICTIONAL");
   let committed=std::process::Command::new(&runtime.git).arg("-C").arg(repo).args(["show","main:marker.txt"]).output()?;
   assert!(committed.status.success());assert_eq!(String::from_utf8(committed.stdout)?.trim(),"FICTIONAL");
  }
  let binding=bridge.request("current",serde_json::json!({}))?["binding"].clone();
  let item=bridge.request("inspect",serde_json::json!({"binding":binding,"query":{"item_id":obligation}}))?;
  assert_eq!(item["item"]["status"],"completed","parent assignment was not verified complete");
  drop(spine);
  println!("{}",serde_json::json!({"worker_reports_attributed_separately":true,"natural_language_assignment":true,"two_repositories_integrated":true,"independent_reviews_verified":true,"canonical_cleanup_verified":true,"parent_obligation_completed":true,"installed_acceptance":false}));return Ok(());
 }
 let projects:&[&str]=if revision { &["first project"] } else { &["first project","second project"] };
 for (index, name) in projects.iter().enumerate() {
 let repo=std::fs::canonicalize(root.0.join(name))?;
 let prompt=format!("This is a disposable product integration test. Do exactly these operations. First use the continuity checkpoint tool to accept one commitment id marker-task, title Create the fictional marker. Then use richos_work.repositories to confirm the two connected repositories. Use richos_work.prepare with request_id marker-worker, obligation_id marker-task, repo {}, integration main, role worker and title Create fictional marker. Its brief must explicitly name the returned cross-repository target as the implementation directory (the native coordination worktree is not the target) and require the worker to create marker.txt containing exactly FICTIONAL in its assigned target worktree, read it, then commit only marker.txt using git -c user.name=Fixture -c user.email=fixture@example.invalid -c commit.gpgSign=false commit. No publication or other changes are authorized. Submit the returned agent_payload to Agent exactly, without reconstructing it or changing any field, and use TaskOutput with block:true to wait for that actual worker result. Call richos_work.inspect and report what actually happened. Do not implement the file yourself, integrate it or mark the assignment complete. Use only the described MCP tools, Agent and TaskOutput. Do not end by promising an action you have not taken.",repo.display());
 let prompt=prompt.replace("marker-task", &format!("marker-task-{index}")).replace("marker-worker", &format!("marker-worker-{index}"));
 let prompt=if revision {prompt.replace("containing exactly FICTIONAL", "containing exactly WRONG")} else {prompt};
 let expected=if revision {"WRONG"} else {"FICTIONAL"};
 let turn=spine.submit_prompt(&prompt,Source::Text)?;
 verify_worker_attribution(&spine,&thread,&turn);
 let mut receipts=Vec::new();
 for partition in std::fs::read_dir(data.join("engine-state/work-receipts"))?{
  for file in std::fs::read_dir(partition?.path())?{
   let path=file?.path();if path.extension().is_some_and(|e|e=="json"){receipts.push(serde_json::from_slice::<serde_json::Value>(&std::fs::read(path)?)?);}
  }
 }
 receipts.retain(|r|r["request"]["repo"].as_str()==repo.to_str() && r["request"]["role"]=="worker");
 assert_eq!(receipts.len(),1,"expected one actual prepared worker");let receipt=&receipts[0];
 if receipt["status"]!="run-ended"{eprintln!("receipt state: {}\nlast response: {}",receipt,spine.ledger().turn(&turn).unwrap().assistant_text);}
 assert_eq!(receipt["status"],"run-ended","provider run did not end");assert!(receipt["agent_id"].is_string());
 let target=targets.join(receipt["name"].as_str().unwrap());
 assert_eq!(std::fs::read_to_string(target.join("marker.txt"))?.trim(),expected);assert!(!repo.join("marker.txt").exists());
 let head=std::process::Command::new(&runtime.git).arg("-C").arg(&target).args(["show","HEAD:marker.txt"]).output()?;
 if !head.status.success(){eprintln!("Git verification failed: {}\nProvider report: {}",String::from_utf8_lossy(&head.stderr),spine.ledger().turn(&turn).unwrap().assistant_text);}
 assert!(head.status.success());assert_eq!(String::from_utf8(head.stdout)?.trim(),expected);
 let binding=bridge.request("current",serde_json::json!({}))?["binding"].clone();
 let records=bridge.request("inspect",serde_json::json!({"binding":binding,"query":{"section":"work"}}))?;
 let observed=records["records"].as_array().unwrap().iter().find(|r|r["external_id"]==receipt["id"]).expect("worker observation");assert_ne!(observed["status"],"completed");
 let transcript=std::fs::read_to_string(data.join("engine-state/evidence").join(receipt["binding"]["session_id"].as_str().unwrap()).join("guard-transcript.jsonl"))?;
 assert!(transcript.lines().filter_map(|line|serde_json::from_str::<serde_json::Value>(line).ok()).any(|row|row["evidenceSource"]=="richos-ledger-attested-hook-v1"),"visible user instruction was not attested from the ledger");
 // Replace the reasoning process while retaining the scoped obligation and work receipt.
 let mut replacement=richos_core::engine_profile::EngineProfile::prepare(&engine,&data,runtime.clone())?;
 replacement.scope_to(&spine.ledger().thread_binding(&thread)?)?;
 let fresh=NativeCognition::start_with_engine(&resolve_claude_bin(),&doctrine,&skills,&std::env::current_exe()?,bridge.clone(),replacement,None)?;
 assert_ne!(fresh.session_id(),session);spine.attach_lease(Box::new(fresh));
 let review_prompt=format!("The fictional worker's commit is ready. Its worker receipt is {}. Prepare an independent reviewer using richos_work.prepare: request_id marker-review, obligation_id marker-task, repo {}, integration main, role reviewer, review_of that worker receipt, title Review fictional marker. Require checking the exact prepared commit and confirming marker.txt contains exactly FICTIONAL with no other changes, without editing or creating commits. Submit the exact returned agent_payload to Agent once and wait through TaskOutput block:true. Then use richos_work.inspect. If the host captured a passing review, I authorize richos_work.integrate with this worker_id and the actual reviewer_id to fast-forward local main and perform canonical workspace cleanup. No publication. Report only verified results. Do not use Bash or write files yourself; use the work MCP tools, Agent and TaskOutput.",receipt["id"].as_str().unwrap(),repo.display());
 let review_prompt=review_prompt.replace("marker-task", &format!("marker-task-{index}")).replace("marker-review", &format!("marker-review-{index}"));
 let review_prompt=if revision {format!("Review the saved worker {} in {} independently against this success condition: marker.txt must contain exactly FICTIONAL. The fixture currently contains a deliberate defect. First obtain and observe the independent review of that existing commit. If the reviewer requests changes, continue the saved worker with the specific correction, obtain a fresh independent review of the corrected commit, integrate locally and clean up all eligible workspaces. Close the assignment only from verified evidence. I authorize that revision and local integration. Do not publish. Complete the cycle without asking me to operate internal tools or IDs.",receipt["id"].as_str().unwrap(),repo.display())}else{review_prompt};
 let reviewed=spine.submit_prompt(&review_prompt,Source::Text)?;
 verify_worker_attribution(&spine,&thread,&reviewed);
 let mut updated:serde_json::Value=serde_json::from_slice(&std::fs::read(data.join("engine-state/work-receipts").join(std::fs::read_dir(data.join("engine-state/work-receipts"))?.next().unwrap()?.file_name()).join(format!("{}.json",receipt["id"].as_str().unwrap())))?)?;
 if revision {
  let partition=data.join("engine-state/work-receipts").join(std::fs::read_dir(data.join("engine-state/work-receipts"))?.next().unwrap()?.file_name());
  let mut rejected=false;let mut continued=false;
  for file in std::fs::read_dir(partition)? {
   let path=file?.path();if !path.extension().is_some_and(|e|e=="json"){continue;}
   let record:serde_json::Value=serde_json::from_slice(&std::fs::read(path)?)?;
   if record["review_observation"]["report"]["verdict"]=="changes-requested" {rejected=true;}
   if record["continuation"]["worker_id"]==receipt["id"] {updated=record.clone();continued=true;}
   assert!(!targets.join(record["name"].as_str().unwrap()).exists(),"an old review or worker workspace was left behind");
  }
  assert!(rejected && continued,"the real rejection and revision cycle was not observed");
  let binding=bridge.request("current",serde_json::json!({}))?["binding"].clone();
  let item=bridge.request("inspect",serde_json::json!({"binding":binding,"query":{"item_id":receipt["request"]["obligation_id"]}}))?;
  assert_eq!(item["item"]["status"],"completed");
 }
 if updated["integration"]["verified"]!=true {eprintln!("Review/integration report: {}\nReceipt: {}",spine.ledger().turn(&reviewed).unwrap().assistant_text,updated);}
 assert_eq!(updated["integration"]["verified"],true,"actual review/integration did not finish");
 assert_eq!(updated["integration"]["cleanup_pending"],serde_json::json!([]));
 assert_eq!(std::fs::read_to_string(repo.join("marker.txt"))?.trim(),"FICTIONAL");assert!(!target.exists());
 }
 println!("{}",serde_json::json!({"two_repositories_integrated":!revision,"rejected_review_revised_and_verified":revision,"real_worker_dispatched":true,"worker_commit_verified":true,"target_main_untouched_before_integration":true,"ecs_completion_not_inferred_from_exit":true,"fresh_provider_recovered_work":true,"real_reviewer_observed":true,"reviewed_commit_integrated":true,"canonical_cleanup_verified":true,"host_user_instruction_attested":true,"installed_acceptance":false}));drop(spine);Ok(())
}
