//! Filesystem notifications deliver peer input through the existing receiving actors.
use crate::AppState;
use richos_core::read_view::SpineView;
use notify::Watcher;
use richos_core::{session_tools, status_tools::StatusToolScope};
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Manager};

struct Watch {_watcher:Mutex<notify::RecommendedWatcher>}

fn drain(app:&AppHandle)->Result<(),String> {
    let state=app.state::<AppState>();
    let view=state.reader.snapshot();
    for thread in view.threads() {
        let Ok(binding)=view.ledger().thread_binding(&thread.id) else {continue};
        let scope=StatusToolScope {version:1,state_root:state.data_dir.join("engine-state"),
            entity_id:binding.entity_id().to_string(),thread_id:thread.id.clone()};
        let mut delivered=false;
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

fn watch(root:&std::path::Path,tx:mpsc::SyncSender<()>)->notify::Result<notify::RecommendedWatcher> {
    let mut watcher=notify::recommended_watcher(move |event:notify::Result<notify::Event>| {
        match event {
            Ok(event) if matches!(event.kind,notify::EventKind::Create(_)|notify::EventKind::Modify(_)|notify::EventKind::Remove(_)) && event.paths.iter().any(|p|p.extension().is_some_and(|e|e=="jsonl"))=>{if let Err(mpsc::TrySendError::Disconnected(()))=tx.try_send(()) {eprintln!("[richos] peer input receiver closed");}},
            Err(error)=>{eprintln!("[richos] peer message watch: {error}");if let Err(mpsc::TrySendError::Disconnected(()))=tx.try_send(()) {eprintln!("[richos] peer input receiver closed");}},
            _=>{},
        }
    })?;
    watcher.watch(root,notify::RecursiveMode::Recursive)?;
    Ok(watcher)
}

pub fn start(app:AppHandle)->Result<(),Box<dyn std::error::Error>> {
    let root=app.state::<AppState>().data_dir.join("engine-state/messages");
    std::fs::create_dir_all(&root)?;
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
    fn a_durable_peer_append_wakes_the_kernel_watch_and_remains_until_accepted() {
        let root=std::env::temp_dir().join(format!("richos-peer-watch-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"conversation".into()};
        let messages=root.join("messages");std::fs::create_dir_all(&messages).unwrap();
        let (tx,rx)=mpsc::sync_channel(1);
        let watcher=watch(&messages,tx).unwrap();
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
