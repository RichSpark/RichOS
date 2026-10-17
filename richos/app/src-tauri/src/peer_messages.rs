//! Filesystem notifications deliver peer input through the existing receiving actors.
use crate::AppState;
use richos_core::read_view::SpineView;
use notify::Watcher;
use richos_core::{session_tools, status_tools::StatusToolScope};
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Manager};

struct Watch {_watcher:Mutex<notify::RecommendedWatcher>}

/// Accept through the existing durable intake before acknowledging the source.
/// Only immutable notice facts enter the message, so retries retain one identity.
fn forward_work_notices(scope:&StatusToolScope,mut accept:impl FnMut(&str,&str)->Result<(),String>)->Result<bool,String> {
    use richos_core::assignment;
    let notices=assignment::pending_notices_of(&scope.state_root,&scope.entity_id,&scope.thread_id,|kind|!kind.is_result())
        .map_err(|e|e.to_string())?;
    for notice in &notices {
        let facts=serde_json::json!({"event":"work_status","assignment_id":notice.assignment_id,
            "notice_kind":notice.kind,"observed_at_ms":notice.raised_at_ms}).to_string();
        accept(&facts,&notice.delivery_id(&scope.entity_id,&scope.thread_id))?;
        assignment::acknowledge_notice(&scope.state_root,&scope.entity_id,&scope.thread_id,notice).map_err(|e|e.to_string())?;
    }
    Ok(!notices.is_empty())
}

fn drain(app:&AppHandle)->Result<(),String> {
    let state=app.state::<AppState>();
    let view=state.reader.snapshot();
    for thread in view.threads() {
        let Ok(binding)=view.ledger().thread_binding(&thread.id) else {continue};
        let scope=StatusToolScope {version:1,state_root:state.data_dir.join("engine-state"),
            entity_id:binding.entity_id().to_string(),thread_id:thread.id.clone()};
        let mut delivered=forward_work_notices(&scope,|facts,id| {
            state.control.submit_agent_message(&thread.id,Some(binding.entity_id().clone()),facts,"work-status",id)
                .map(|_|()).map_err(|e|e.to_string())
        })?;
        while let Some((message,through))=session_tools::pending(&scope,"rich")? {
            state.control.submit_agent_message(&thread.id,Some(binding.entity_id().clone()),
                &serde_json::to_string(&message).map_err(|e|e.to_string())?,&message.from,
                &format!("peer:{}:{}:{through}",scope.entity_id,scope.thread_id)).map_err(|e|e.to_string())?;
            session_tools::acknowledge(&scope,"rich",through)?;
            delivered=true;
        }
        if delivered {crate::dispatch_incoming_messages(app,&thread.id)?;}
        if session_tools::pending(&scope,"stu")?.is_some() {state.work.wake_messages(&binding);}
    }
    Ok(())
}

fn receive(event:notify::Result<notify::Event>, tx:&mpsc::SyncSender<()>) {
        match event {
            Ok(event) if event.need_rescan() || matches!(event.kind,notify::EventKind::Create(_)|notify::EventKind::Modify(_)|notify::EventKind::Remove(_)) && event.paths.iter().any(|p|p.extension().is_some_and(|e|e=="jsonl" || e=="json"))=>{if let Err(mpsc::TrySendError::Disconnected(()))=tx.try_send(()) {eprintln!("[richos] peer input receiver closed");}},
            Err(error)=>{eprintln!("[richos] peer message watch: {error}");if let Err(mpsc::TrySendError::Disconnected(()))=tx.try_send(()) {eprintln!("[richos] peer input receiver closed");}},
            _=>{},
        }
}

fn watch(root:&std::path::Path,tx:mpsc::SyncSender<()>)->notify::Result<notify::RecommendedWatcher> {
    let mut watcher=notify::recommended_watcher(move |event:notify::Result<notify::Event>| {
        receive(event, &tx);
    })?;
    for name in ["messages","assignments"] {watcher.watch(&root.join(name),notify::RecursiveMode::Recursive)?;}
    Ok(watcher)
}

pub fn start(app:AppHandle)->Result<(),Box<dyn std::error::Error>> {
    let root=app.state::<AppState>().data_dir.join("engine-state");
    for name in ["messages","assignments"] {std::fs::create_dir_all(root.join(name))?;}
    let (tx,rx)=mpsc::sync_channel(1);
    let watcher=watch(&root,tx)?;
    // Keep the kernel watch alive with the application, not a detached polling process.
    app.manage(Watch {_watcher:Mutex::new(watcher)});
    std::thread::Builder::new().name("richos-peer-input".into()).spawn(move || {
        // Drain before waiting, including input saved before a restart or watch registration.
        loop {
            match drain(&app) {
                Ok(())=>if rx.recv().is_err() {return},
                Err(error)=>{
                    eprintln!("[richos] peer input remains on disk: {error}");
                    if matches!(rx.recv_timeout(std::time::Duration::from_secs(5)),Err(mpsc::RecvTimeoutError::Disconnected)) {return}
                }
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropped_notifications_request_a_drain_without_paths() {
        let (tx,rx)=mpsc::sync_channel(1);
        let event=notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert!(event.need_rescan());
        assert!(event.paths.is_empty());
        receive(Ok(event),&tx);
        assert_eq!(rx.try_recv().expect("rescan must wake the existing drain"),());
        receive(Ok(notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any)).add_path("noise.log".into())),&tx);
        assert!(matches!(rx.try_recv(),Err(mpsc::TryRecvError::Empty)));
        receive(Ok(notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any)).add_path("assignments/company/job.json".into())),&tx);
        assert_eq!(rx.try_recv().expect("a saved work observation must wake the same receiver"),());
    }

    #[test]
    fn work_observations_retry_with_one_identity_and_never_replay_legacy_prose() {
        use richos_core::assignment;
        let root=std::env::temp_dir().join(format!("richos-work-facts-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"thread".into()};
        let record=assignment::register(&root,&assignment::Registration {entity_id:scope.entity_id.clone(),thread_id:scope.thread_id.clone(),
            obligation_id:"work-test".into(),instruction_ledger_ref:"ledger:thread:turn".into(),instruction_sha256:"a".repeat(64),
            title:"Investigate the saved request".into(),repositories:vec![],needs_screen:false}).unwrap();
        assignment::raise_notice(&root,"company","thread",&record.id,assignment::NoticeKind::Unknown,
            "Nothing is running now. Say the word and I'll pick it back up.").unwrap();
        assignment::raise_notice(&root,"company","thread",&record.id,assignment::NoticeKind::Settled,"Legacy result").unwrap();
        let mut first=None;
        assert!(forward_work_notices(&scope,|text,id| {first=Some((text.to_string(),id.to_string()));Err("acceptance uncertain".into())}).is_err());
        assert_eq!(assignment::pending_notices_of(&root,"company","thread",|kind|!kind.is_result()).unwrap().len(),1);
        assignment::advance(&root,"company","thread",&record.id,assignment::AssignmentState::Running,"Running").unwrap();
        assert!(forward_work_notices(&scope,|text,id| {
            assert_eq!(first.as_ref().unwrap(),&(text.to_string(),id.to_string()),"a retry changed durable input identity or facts");
            let facts:serde_json::Value=serde_json::from_str(text).unwrap();
            assert_eq!(facts["notice_kind"],"unknown");assert_eq!(facts["assignment_id"],record.id);
            assert!(facts.get("recovery").is_none(),"obsolete restart authority entered Rich's input");
            assert!(!text.contains("Nothing is running now"));assert!(!text.contains("Say the word"));
            Ok(())
        }).unwrap());
        assert!(!forward_work_notices(&scope,|_,_|panic!("accepted work observation was replayed")).unwrap());
        assert_eq!(assignment::pending_notices_of(&root,"company","thread",|kind|kind.is_result()).unwrap().len(),1,"a job record duplicated Stu's authored result");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_durable_peer_append_wakes_the_kernel_watch_and_remains_until_accepted() {
        let root=std::env::temp_dir().join(format!("richos-peer-watch-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"conversation".into()};
        for name in ["messages","assignments"] {std::fs::create_dir_all(root.join(name)).unwrap();}
        let (tx,rx)=mpsc::sync_channel(1);
        let watcher=watch(&root,tx).unwrap();
        let binding=root.join("sender.json");
        session_tools::bind(&binding,&scope,"native-rich","rich",None).unwrap();
        session_tools::send(&binding,&scope,serde_json::json!({"text":"Inspect the actual work"})).unwrap();
        // Hang guard only. Correctness is the observed kernel event and unchanged durable input.
        rx.recv_timeout(std::time::Duration::from_secs(10)).expect("no kernel notification for the append");
        let (message,through)=session_tools::pending(&scope,"stu").unwrap().unwrap();
        assert_eq!(message.text,"Inspect the actual work");
        assert_eq!(session_tools::pending(&scope,"stu").unwrap().unwrap().1,through);
        session_tools::acknowledge(&scope,"stu",through).unwrap();
        assert!(session_tools::pending(&scope,"stu").unwrap().is_none());
        drop(watcher);std::fs::remove_dir_all(root).unwrap();
    }
}
