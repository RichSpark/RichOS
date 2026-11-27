//! Filesystem notifications deliver peer input through the existing receiving actors.
use crate::AppState;
use richos_core::read_view::SpineView;
use notify::Watcher;
use richos_core::{session_tools, status_tools::StatusToolScope};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, SystemTime};
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
const RETRY_AFTER: Duration = Duration::from_secs(5);

fn waiting_signal(state_root: &std::path::Path) -> std::path::PathBuf {
    state_root.join("messages").join(WAITING_WAKEUP)
}

pub(crate) fn wake_waiting(state_root: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(state_root.join("messages"))?;
    std::fs::write(waiting_signal(state_root), [])
}

fn waiting_bridge(state: &AppState) -> Result<richos_core::ecs::EcsBridge, String> {
    let engine = state.engine_dir.lock().map_err(|_| "engine unavailable")?.clone();
    let runtime = richos_core::runtime::verify_engine(&engine).map_err(|error| error.to_string())?;
    richos_core::ecs::EcsBridge::new(&runtime.python, &engine, &state.data_dir.join("ecs"))
        .map_err(|error| error.to_string())
}

fn reconcile_waiting(app: &AppHandle, at_launch: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let ecs = state.data_dir.join("ecs");
    if !ecs.join("ecs.sqlite3").try_exists().map_err(|error| error.to_string())? { return Ok(()); }
    let bridge = waiting_bridge(&state)?;
    let hello = bridge.request("hello", json!({})).map_err(|e| e.to_string())?;
    let commands = hello["commands"].as_array().ok_or("missing waiting-work capabilities")?;
    if !commands.iter().any(|command| command == "waiting-work") {
        return Ok(());
    }
    let view = state.reader.snapshot();
    let mut deferred = None;
    let pending = bridge.request("pending-replies", json!({})).map_err(|e| e.to_string())?;
    for reply in pending["items"].as_array().ok_or("missing pending replies")? {
        let (Some(thread), Some(entity), Some(reference)) = (
            reply["thread_id"].as_str(), reply["entity_id"].as_str(), reply["reply_turn"].as_str(),
        ) else { continue };
        let prefix = format!("ledger:{thread}:");
        let Some(turn) = reference.strip_prefix(&prefix).and_then(|id| view.ledger().turn(id)) else { continue };
        if turn.thread_id == thread && turn.entity_id.as_ref().is_some_and(|id| id.as_str() == entity) {
            if let Err(error) = bridge.delivered_reply(turn) {
                deferred.get_or_insert_with(|| format!("ECS reply remains owed: {error}"));
            }
        }
    }
    let result = bridge.request("waiting-work", json!({})).map_err(|e| e.to_string())?;
    let installed = app.package_info().version.to_string();
    for item in result["items"].as_array().ok_or("missing waiting conditions")? {
        let (Some(entity), Some(thread), Some(id), Some(conditions)) = (
            item["entity_id"].as_str(), item["thread_id"].as_str(), item["item_id"].as_str(), item["conditions"].as_array(),
        ) else { continue };
        let Ok(binding) = view.ledger().thread_binding(thread) else { continue };
        if binding.entity_id().as_str() != entity { continue; }
        let scope = StatusToolScope { version: 1, state_root: state.data_dir.join("engine-state"),
            entity_id: entity.into(), thread_id: thread.into() };
        if session_tools::terminal_handover_active(&scope) { continue; }
        let skipped = conditions.iter().any(|condition| condition["state"] == "skipped");
        let ready = if skipped {
            false
        } else {
            match conditions.iter().try_fold(true, |ready, condition| {
                if ready { waiting_condition_satisfied(&scope, condition, &installed) } else { Ok(false) }
            }) {
                Ok(ready) => ready,
                Err(error) => {
                    deferred.get_or_insert_with(|| format!("waiting work {id} could not be read: {error}"));
                    continue;
                }
            }
        };
        if (!ready && !skipped) || conditions.is_empty() { continue; }
        let original = if ready && !skipped {
            let Ok(instruction) = serde_json::from_value::<richos_core::ecs::UserInstruction>(item["instruction"].clone()) else {
                eprintln!("[richos] waiting work {id} has no readable original order");
                continue;
            };
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
        let mut action = None;
        if original.is_some() && item["launch"] == true {
            let Some(next) = item["next_action"].as_str() else {
                eprintln!("[richos] waiting work {id} has no authorized next action");
                continue;
            };
            action = Some(next);
        }
        // THE ONE RULE (richos-core `turn_gate`): an order whose condition the first look at
        // launch finds already true came true while the app was closed, so its notice says so
        // and waits for the user in its chat (`drain_inboxes`); one that comes true while the
        // app is open runs at once.
        if let Err(error) = session_tools::observe_waiting(&scope, json!({
            "event": "waiting_condition", "input_id": format!("{id}:{status}"),
            "obligation_id": id, "status": status, "conditions": conditions,
            "next_action": action, "found_at_launch": at_launch,
        }), original.filter(|_| action.is_some())) {
            deferred.get_or_insert_with(|| format!("waiting notice {id} remains pending: {error}"));
        }
    }
    deferred.map_or(Ok(()), Err)
}

fn waiting_condition_satisfied(scope: &StatusToolScope, observation: &Value, installed: &str) -> Result<bool, String> {
    let condition = &observation["condition"];
    let id = condition["id"].as_str().unwrap_or_default();
    let owner_thread = condition["thread_id"].as_str().unwrap_or(&scope.thread_id);
    match condition["kind"].as_str() {
        Some("publication") => Ok(observation["state"] == "ready"),
        Some("installation") => Ok(condition["release"].as_str().is_some_and(|release| {
            release.strip_prefix('v').unwrap_or(release) == installed || release.parse::<u64>().is_ok_and(|counter| {
                installed.split_once("-nightly.").and_then(|(_, serial)| serial.split_once('.'))
                    .and_then(|(_, value)| value.parse::<u64>().ok()) == Some(counter)
            })
        })),
        Some("assignment") => richos_core::assignment::read(&scope.state_root, &scope.entity_id, owner_thread, id)
            .map(|assignment| assignment.state == richos_core::assignment::AssignmentState::Settled)
            .map_err(|error| error.to_string()),
        Some("question") => richos_core::questions::Store::new(&scope.state_root).list(&scope.entity_id, owner_thread)
            .map(|questions| questions.iter().any(|q| q.id == id && q.state == richos_core::questions::State::Answered)),
        _ => Ok(false),
    }
}

fn record_waiting_notice(state: &AppState, scope: &StatusToolScope, message: &mut session_tools::Message,
    through: u64) -> Result<bool, String> {
    let mut facts: Value = serde_json::from_str(&message.text).map_err(|error| error.to_string())?;
    let unverified = "the engine did not verify this waiting notice";
    let id = facts["obligation_id"].as_str().ok_or(unverified)?;
    let mut evidence = format!("inbox:rich:{through}");
    if let Some(reason) = facts["conditions"].as_array().into_iter().flatten()
        .find(|condition| condition["state"] == "skipped").and_then(|condition| condition["reason"].as_str()) {
        evidence.push(' ');
        evidence.push_str(reason);
    }
    let recorded = waiting_bridge(state)?.request("waiting-noticed", json!({
        "entity_id": scope.entity_id, "thread_id": scope.thread_id, "item_id": id,
        "state": facts["status"], "evidence": evidence,
    })).map_err(|error| error.to_string())?;
    if !recorded["deliver"].as_bool().ok_or(unverified)? { return Ok(false); }
    if let Some(action) = facts["next_action"].as_str() {
        // The durable inbox can retry this exact registration after a crash.
        let instruction = message.instruction.as_ref().ok_or(unverified)?;
        let row = richos_core::assignment::register_waiting(&scope.state_root, &richos_core::assignment::Registration {
            entity_id: scope.entity_id.clone(), thread_id: scope.thread_id.clone(), obligation_id: id.into(),
            instruction_ledger_ref: instruction.ledger_ref.clone(), instruction_sha256: instruction.sha256.clone(),
            title: action.into(), repositories: Vec::new(), needs_screen: false,
        }).map_err(|error| error.to_string())?;
        message.work = Some(session_tools::WorkReference { assignment_id: row.id.clone(), obligation_id: id.into() });
        facts["assignment_id"] = json!(row.id);
        message.text = facts.to_string();
    }
    Ok(true)
}

fn drain_inboxes(app:&AppHandle,state:&AppState,thread:&str,binding:&richos_core::entity::ThreadBinding,scope:&StatusToolScope)->Result<(),String> {
        if session_tools::terminal_handover_active(scope) { return Ok(()); }
        let mut delivered=false;
        let mut after = 0;
        let mut deferred = None;
        let inbox=session_tools::inbox(scope,"rich")?;
        while let Some((mut message,through))=session_tools::pending_after(scope,"rich",after)? {
            after = through;
            // The existing durable inbox is the retry boundary. ECS must apply
            // its verified state before Rich can act on or answer this input.
            if message.from == "ecs-waiting" {
                match record_waiting_notice(state, scope, &mut message, through) {
                    Ok(true) => {}
                    Ok(false) => { session_tools::acknowledge(scope,"rich",through)?; continue; }
                    Err(error) => {
                        // The durable cursor can accept later independent inputs without losing this one.
                        deferred.get_or_insert(error);
                        continue;
                    }
                }
            }
            // A message left here before the app opened is filed as from before, so the one rule
            // holds its turn until the user is active in this chat (richos-core `turn_gate`,
            // `Spine::deliver`); one written since (an order whose condition came true now, a
            // report of work running now) is filed as now and goes.
            // An order's notice found at launch is about a condition that came true while the app
            // was closed (`reconcile_waiting`): from before, like a row saved then.
            let found_at_launch = message.from == "ecs-waiting" && serde_json::from_str::<Value>(&message.text)
                .is_ok_and(|facts| facts["found_at_launch"] == true);
            let written=if state.turn_gate.arrived_while_open(&inbox,through) && !found_at_launch {richos_core::util::now_millis()}
                else {state.turn_gate.since_ms().saturating_sub(1)};
            state.control.submit_agent_message_written_at(thread,Some(binding.entity_id().clone()),
                &serde_json::to_string(&message).map_err(|e|e.to_string())?,&message.from,
                &format!("peer:{}:{}:{through}",scope.entity_id,scope.thread_id),written).map_err(|e|e.to_string())?;
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
        deferred.map_or(Ok(()), Err)
}


fn receive(event: notify::Result<notify::Event>, receivers: &[mpsc::SyncSender<()>]) {
    match event {
        Ok(event) if event.need_rescan() || matches!(event.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_))
            && event.paths.iter().any(|p| p.extension().is_some_and(|e| e == "jsonl" || e == "json")) => {}
        Err(error) => eprintln!("[richos] peer message watch: {error}"),
        _ => return,
    }
    for tx in receivers {
        if let Err(mpsc::TrySendError::Disconnected(())) = tx.try_send(()) {
            eprintln!("[richos] peer input receiver closed");
        }
    }
}

fn watch(root:&std::path::Path,receivers:Vec<mpsc::SyncSender<()>>)->notify::Result<notify::RecommendedWatcher> {
    let mut watcher=notify::recommended_watcher(move |event:notify::Result<notify::Event>| {
        receive(event, &receivers);
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
    let (waiting_tx, waiting_rx) = mpsc::sync_channel(1);
    let watcher = watch(&root, vec![tx, waiting_tx])?;
    let waiting_app = app.clone();
    std::thread::Builder::new().name("richos-waiting-work".into()).spawn(move || {
        reconcile_until_closed(&root, waiting_rx, |at_launch| reconcile_waiting(&waiting_app, at_launch));
    })?;
    // Keep the kernel watch alive with the application, not a detached polling process.
    app.manage(Watch {_watcher:Mutex::new(watcher)});
    std::thread::Builder::new().name("richos-peer-input".into()).spawn(move || {
        receive_until_closed(rx, || drain(&app));
    })?;
    Ok(())
}

fn waiting_revision(path: &std::path::Path) -> Result<Option<SystemTime>, String> {
    match std::fs::metadata(path).and_then(|file| file.modified()) {
        Ok(revision) => Ok(Some(revision)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("could not read the waiting-work wakeup: {error}")),
    }
}

fn reconcile_until_closed(root: &std::path::Path, rx: mpsc::Receiver<()>,
    mut reconcile: impl FnMut(bool) -> Result<(), String>,
) {
    // Reconcile on startup and after owner signals. Keep a failed revision pending even
    // when there are no more filesystem events. This worker never holds the peer drain.
    let signal = waiting_signal(root);
    let mut settled = None;
    let mut said = None;
    loop {
        let result = waiting_revision(&signal).and_then(|revision| {
            if settled != Some(revision) {
                // Until a revision settles, even a retry is still the first look at launch.
                reconcile(settled.is_none())?;
                settled = Some(revision);
            }
            Ok(())
        });
        match result {
            Ok(()) => { said = None; if rx.recv().is_err() { return; } }
            Err(error) => {
                if said.as_ref() != Some(&error) {
                    eprintln!("[richos] waiting-work evidence unavailable: {error}");
                    said = Some(error);
                }
                if matches!(rx.recv_timeout(RETRY_AFTER), Err(mpsc::RecvTimeoutError::Disconnected)) {
                    return;
                }
            }
        }
    }
}

fn receive_until_closed(rx: mpsc::Receiver<()>, mut drain: impl FnMut() -> Result<(), String>) {
    let mut said = None;
    loop {
        match drain() {
            Ok(()) => { said = None; if rx.recv().is_err() { return; } }
            Err(error) => {
                if said.as_ref() != Some(&error) {
                    eprintln!("[richos] peer input remains on disk: {error}");
                    said = Some(error);
                }
                if matches!(rx.recv_timeout(RETRY_AFTER), Err(mpsc::RecvTimeoutError::Disconnected)) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_owner_wakes_the_running_receiver_after_startup() {
        use std::time::Duration;
        let root = std::env::temp_dir().join(format!("richos-waiting-watch-{}-{}", std::process::id(), richos_core::util::now_millis()));
        std::fs::create_dir_all(root.join("messages")).unwrap();
        let (wake, receiver) = mpsc::sync_channel(1);
        let watcher = watch(&root, vec![wake]).unwrap();
        let (proof, seen) = mpsc::channel();
        let result = std::thread::scope(|threads| {
            threads.spawn(|| reconcile_until_closed(&root, receiver, |at_launch| {
                proof.send(at_launch).unwrap();
                Ok(())
            }));
            // load-bound: the message is the proof. This only limits a dead kernel watch.
            let observed = || seen.recv_timeout(Duration::from_secs(30));
            let result = (|| {
                let startup = observed()?;
                std::fs::write(root.join("messages/reconcile-waiting.json"), b"from publisher").unwrap();
                let publisher = observed()?;
                wake_waiting(&root).unwrap();
                let first_app_wake = observed()?;
                wake_waiting(&root).unwrap();
                Ok::<_, mpsc::RecvTimeoutError>([startup, publisher, first_app_wake, observed()?])
            })();
            drop(watcher);
            result
        });
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(result.expect("startup, the publisher and the app must each wake the same running receiver"),
            [true, false, false, false]);
    }

    #[test]
    fn the_same_wakeup_retries_a_bridge_failure_and_files_one_notice() {
        let root = std::env::temp_dir().join(format!("richos-waiting-retry-{}-{}", std::process::id(), richos_core::util::now_millis()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/ecs"), r#"
import json
import pathlib
import sys

request = json.load(sys.stdin)
assert request["command"] == "waiting-work"
failed = pathlib.Path(sys.argv[2]) / "failed"
if not failed.exists():
    failed.touch()
    print(json.dumps({"protocol": 1, "ok": False, "error": {"message": "temporary store read failure"}}))
    sys.exit(1)
print(json.dumps({"protocol": 1, "ok": True, "result": {"items": []}}))
"#).unwrap();
        let bridge = richos_core::ecs::EcsBridge { python: "python3".into(), component: root.clone(), state_root: root.clone() };
        let scope = StatusToolScope { version: 1, state_root: root.clone(), entity_id: "company".into(), thread_id: "conversation".into() };
        wake_waiting(&root).unwrap();
        let (wake, receiver) = mpsc::channel();
        // Two ordinary notifications with no change to the owner's signal.
        wake.send(()).unwrap();
        wake.send(()).unwrap();
        drop(wake);
        let mut attempts = Vec::new();
        reconcile_until_closed(&root, receiver, |at_launch| {
            attempts.push(at_launch);
            // An item saved before a later read fails must be safe to process again.
            session_tools::observe_waiting(&scope, json!({
                "event": "waiting_condition", "input_id": "existing:ready", "status": "ready",
                "found_at_launch": at_launch,
            }), None)?;
            bridge.request("waiting-work", json!({})).map_err(|error| error.to_string())?;
            session_tools::observe_waiting(&scope, json!({
                "event": "waiting_condition", "input_id": "first-read:ready", "status": "ready",
                "found_at_launch": at_launch,
            }), None)?;
            Ok(())
        });
        assert_eq!(attempts, [true, true], "the first successful read is still at launch");
        let (_, through) = session_tools::pending(&scope, "rich").unwrap().unwrap();
        let (notice, through) = session_tools::pending_after(&scope, "rich", through).unwrap().unwrap();
        let facts: Value = serde_json::from_str(&notice.text).unwrap();
        assert_eq!(facts["input_id"], "first-read:ready", "the retry filed a duplicate");
        assert_eq!(facts["found_at_launch"], true);
        assert!(session_tools::pending_after(&scope, "rich", through).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropped_notifications_request_a_drain_without_paths() {
        let (tx,rx)=mpsc::sync_channel(1);
        let event=notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert!(event.need_rescan());
        assert!(event.paths.is_empty());
        let receivers = [tx];
        receive(Ok(event),&receivers);
        assert_eq!(rx.try_recv().expect("rescan must wake the existing drain"),());
        receive(Ok(notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any)).add_path("noise.log".into())),&receivers);
        assert!(matches!(rx.try_recv(),Err(mpsc::TryRecvError::Empty)));
        receive(Ok(notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any)).add_path("assignments/company/job.json".into())),&receivers);
        assert_eq!(rx.try_recv().expect("a saved work observation must wake the same receiver"),());
    }



    #[test]
    fn a_durable_peer_append_wakes_the_kernel_watch_and_remains_until_accepted() {
        let root=std::env::temp_dir().join(format!("richos-peer-watch-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let scope=StatusToolScope { version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"conversation".into()};
        std::fs::create_dir_all(root.join("messages")).unwrap();
        let (tx,rx)=mpsc::sync_channel(1);
        let watcher=watch(&root,vec![tx]).unwrap();
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
