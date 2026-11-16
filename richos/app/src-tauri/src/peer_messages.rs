//! Filesystem notifications deliver peer input through the existing receiving actors.
use crate::AppState;
use richos_core::read_view::SpineView;
use notify::Watcher;
use richos_core::{session_tools, status_tools::StatusToolScope};
use std::sync::{mpsc, Mutex};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

struct Watch {_watcher:Mutex<notify::RecommendedWatcher>}



fn drain(app:&AppHandle)->Result<(),String> {
    let state=app.state::<AppState>();
    let view=state.reader.snapshot();
    let mut held=Vec::new();
    for thread in view.threads() {
        // One unreadable inbox holds its owner, never the other conversations.
        let Ok(binding)=view.ledger().thread_binding(&thread.id) else {continue};
        let scope=StatusToolScope { version:1,state_root:state.data_dir.join("engine-state"),
            entity_id:binding.entity_id().to_string(),thread_id:thread.id.clone()};
        let result=drain_inboxes(app,&state,&thread.id,&binding,&scope);
        if let Err(error)=result {held.push(format!("{}: {error}",thread.id));}
    }
    if held.is_empty() {Ok(())} else {Err(held.join("; "))}
}

// This file is only a wakeup from the existing publication owner. Its contents
// are never used as release evidence.
const WAITING_WAKEUP: &str = "reconcile-waiting.json";

pub(crate) fn wake_waiting(state_root: &std::path::Path) -> std::io::Result<()> {
    let root = state_root.join("messages");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join(WAITING_WAKEUP), [])
}

fn reconcile_waiting(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let ecs = state.data_dir.join("ecs");
    if !ecs.join("ecs.sqlite3").is_file() { return Ok(()); }
    let engine = state.engine_dir.lock().map_err(|_| "engine unavailable")?.clone();
    let runtime = richos_core::runtime::verify_engine(&engine).map_err(|e| e.to_string())?;
    let bridge = richos_core::ecs::EcsBridge::new(&runtime.python, &engine, &ecs).map_err(|e| e.to_string())?;
    let hello = bridge.request("hello", json!({})).map_err(|e| e.to_string())?;
    if !hello["commands"].as_array().is_some_and(|commands| commands.iter().any(|command| command == "waiting-work")) {
        return Ok(());
    }
    let view = state.reader.snapshot();
    let pending = bridge.request("pending-replies", json!({})).map_err(|e| e.to_string())?;
    for reply in pending["items"].as_array().into_iter().flatten() {
        let (Some(thread), Some(entity), Some(reference)) = (
            reply["thread_id"].as_str(), reply["entity_id"].as_str(), reply["reply_turn"].as_str(),
        ) else { continue };
        let prefix = format!("ledger:{thread}:");
        let Some(turn) = reference.strip_prefix(&prefix).and_then(|id| view.ledger().turn(id)) else { continue };
        if turn.thread_id == thread && turn.entity_id.as_ref().is_some_and(|id| id.as_str() == entity) {
            if let Err(error) = bridge.delivered_reply(turn) {
                eprintln!("[richos] ECS reply remains owed: {error}");
            }
        }
    }
    let result = bridge.request("waiting-work", json!({})).map_err(|e| e.to_string())?;
    for item in result["items"].as_array().ok_or("missing waiting conditions")? {
        let (Some(entity), Some(thread), Some(id), Some(conditions)) = (
            item["entity_id"].as_str(), item["thread_id"].as_str(), item["item_id"].as_str(), item["conditions"].as_array(),
        ) else { continue };
        let Ok(binding) = view.ledger().thread_binding(thread) else { continue };
        if binding.entity_id().as_str() != entity { continue; }
        let scope = StatusToolScope { version: 1, state_root: state.data_dir.join("engine-state"),
            entity_id: entity.into(), thread_id: thread.into() };
        let skipped = conditions.iter().any(|condition| condition["state"] == "skipped");
        let ready = conditions.iter().all(|condition| waiting_condition_satisfied(&scope, condition));
        if (!ready && !skipped) || conditions.is_empty() { continue; }
        let original = if ready && !skipped {
            let instruction: richos_core::ecs::UserInstruction = serde_json::from_value(item["instruction"].clone())
                .map_err(|e| e.to_string())?;
            let prefix = format!("ledger:{thread}:");
            let turn = instruction.ledger_ref.strip_prefix(&prefix).and_then(|turn| view.ledger().turn(turn));
            if !turn.is_some_and(|turn| turn.entity_id.as_ref().is_some_and(|id| id.as_str() == entity)
                && !turn.quarantined && turn.thread_id == thread
                && matches!(turn.source, richos_core::ledger::Source::Text | richos_core::ledger::Source::Jam)
                && instruction.matches_text(&turn.user_text))
            { continue; }
            Some(instruction)
        } else { None };
        let status = if skipped { "skipped" } else { "ready" };
        let mut assignment_id = None;
        if let Some(instruction) = original.as_ref().filter(|_| item["launch"] == true) {
            let action = item["next_action"].as_str().ok_or("missing authorized next action")?;
            let row = richos_core::assignment::register_waiting(&scope.state_root, &richos_core::assignment::Registration {
                entity_id: entity.into(), thread_id: thread.into(), obligation_id: id.into(),
                instruction_ledger_ref: instruction.ledger_ref.clone(), instruction_sha256: instruction.sha256.clone(),
                title: action.into(), repositories: Vec::new(), needs_screen: false,
            }).map_err(|e| e.to_string())?;
            assignment_id = Some(row.id);
        }
        session_tools::observe_waiting(&scope, json!({
            "event": "waiting_condition", "input_id": format!("{id}:{status}"),
            "obligation_id": id, "status": status, "conditions": conditions,
            "assignment_id": assignment_id,
        }), original.filter(|_| assignment_id.is_some()))?;
    }
    Ok(())
}

fn waiting_condition_satisfied(scope: &StatusToolScope, observation: &Value) -> bool {
    let condition = &observation["condition"];
    let id = condition["id"].as_str().unwrap_or_default();
    match condition["kind"].as_str() {
        Some("publication") => observation["state"] == "ready",
        Some("assignment") => richos_core::assignment::read(&scope.state_root, &scope.entity_id, &scope.thread_id, id)
            .is_ok_and(|assignment| assignment.state == richos_core::assignment::AssignmentState::Settled),
        Some("question") => richos_core::questions::Store::new(&scope.state_root).list(&scope.entity_id, &scope.thread_id)
            .is_ok_and(|questions| questions.iter().any(|q| q.id == id && q.state == richos_core::questions::State::Answered)),
        _ => false,
    }
}

fn drain_inboxes(app:&AppHandle,state:&AppState,thread:&str,binding:&richos_core::entity::ThreadBinding,scope:&StatusToolScope)->Result<(),String> {
        let mut delivered=false;
        while let Some((message,through))=session_tools::pending(scope,"rich")? {
            state.control.submit_agent_message(thread,Some(binding.entity_id().clone()),
                &serde_json::to_string(&message).map_err(|e|e.to_string())?,&message.from,
                &format!("peer:{}:{}:{through}",scope.entity_id,scope.thread_id)).map_err(|e|e.to_string())?;
            session_tools::acknowledge(scope,"rich",through)?;
            if message.from == "work-status" {
                if let Err(error) = wake_waiting(&scope.state_root) {
                    eprintln!("[richos] waiting-work check could not be requested: {error}");
                }
            }
            delivered=true;
        }
        if delivered {crate::dispatch_incoming_messages(app,thread)?;}
        if session_tools::pending(scope,"stu")?.is_some() {state.work.wake_messages(binding);}
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
    watcher.watch(&root.join("messages"),notify::RecursiveMode::Recursive)?;
    Ok(watcher)
}

/// **Rich's own background command ended after his turn, and Claude Code started his next one**
/// (CEO §154: in the terminal that turn is an ordinary turn the CEO sees and can stop). It is
/// filed here as a turn of that conversation from `CONTINUATION_CHANNEL`, through the same
/// agent-input road peer messages take, and its delivery adopts the provider's turn rather than
/// sending a second message (`NativeClient::adopt_continuation`).
fn wire_continuations(app:&AppHandle) {
    let handle=app.clone();
    app.state::<AppState>().control.set_continuation_notice(std::sync::Arc::new(move |thread:&str,summary:&str| {
        let state=handle.state::<AppState>();
        let view=state.reader.snapshot();
        let entity=view.ledger().thread_binding(thread).ok().map(|b|b.entity_id().clone());
        let input=richos_core::native::continuation_input(view.ledger(),thread,summary);
        let id=format!("continuation:{thread}:{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|d|d.as_nanos()));
        match state.control.submit_agent_message(thread,entity,&input,richos_core::native::CONTINUATION_CHANNEL,&id) {
            Ok(_)=>if let Err(error)=crate::dispatch_incoming_messages(&handle,thread) {
                eprintln!("[richos] the turn after a background command could not be started: {error}");
            },
            Err(error)=>eprintln!("[richos] the turn after a background command could not be filed: {error}"),
        }
    }));
}

pub fn start(app:AppHandle)->Result<(),Box<dyn std::error::Error>> {
    wire_continuations(&app);
    let root=app.state::<AppState>().data_dir.join("engine-state");
    std::fs::create_dir_all(root.join("messages"))?;
    let (tx,rx)=mpsc::sync_channel(1);
    let watcher=watch(&root,tx)?;
    // Keep the kernel watch alive with the application, not a detached polling process.
    app.manage(Watch {_watcher:Mutex::new(watcher)});
    std::thread::Builder::new().name("richos-peer-input".into()).spawn(move || {
        // Drain before waiting, including input saved before a restart or watch registration.
        let mut said=None;
        let signal = root.join(WAITING_WAKEUP);
        let mut seen = None;
        loop {
            let revision = std::fs::metadata(&signal).and_then(|file| file.modified()).ok();
            if seen != Some(revision) {
                seen = Some(revision);
                let handle = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = reconcile_waiting(&handle) {
                        eprintln!("[richos] waiting-work evidence unavailable: {error}");
                    }
                });
            }
            match drain(&app) {
                Ok(())=>{said=None;if rx.recv().is_err() {return}},
                Err(error)=>{
                    if said.as_ref()!=Some(&error) {eprintln!("[richos] peer input remains on disk: {error}");said=Some(error);}

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
    fn a_durable_peer_append_wakes_the_kernel_watch_and_remains_until_accepted() {
        let root=std::env::temp_dir().join(format!("richos-peer-watch-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let scope=StatusToolScope { version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"conversation".into()};
        std::fs::create_dir_all(root.join("messages")).unwrap();
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
