//! Before-inference routing shares the peer inbox's original-request ownership.
use crate::{ecs::{ToolScope,UserInstruction},session_tools::{self,Session},status_tools::StatusToolScope};
use serde_json::{json,Value};
use std::{io::BufRead,path::Path};

/// Only a visible user delivery may be classified. Peer reports and hidden preparation
/// cannot recursively delegate the original request back to Stu.
fn user_delivery(scope:&StatusToolScope,delivery:&str,instruction:&UserInstruction)->Result<bool,String> {
    let prefix=format!("ledger:{}:",scope.thread_id);
    let mut current=instruction.ledger_ref.strip_prefix(&prefix).ok_or("Invalid original request reference")?.to_string();
    let file=std::fs::File::open(scope.state_root.parent().ok_or("Missing app data")?.join("conversation-ledger.jsonl")).map_err(|e|e.to_string())?;
    let mut found=false;
    for line in std::io::BufReader::new(file).lines() {
        let line=line.map_err(|e|e.to_string())?;
        let Ok(row)=serde_json::from_str::<Value>(&line) else {continue};
        if row["event"]=="PromptReceived" && row["turn_id"]==current {
            found=row["entity_id"]==scope.entity_id && row["thread_id"]==scope.thread_id
                && matches!(row["source"].as_str(),Some("text"|"jam"));
        }
        if found && row["event"]=="TurnReplayed" && row["turn_id"]==current {
            current=row["by_turn_id"].as_str().ok_or("Invalid continuation identity")?.to_string();
        }
        if found && current==delivery {return Ok(true)}
    }
    Ok(false)
}

/// The classifier supplies a choice only. All dispatch uses `session_tools::delegate`.
/// Recheck the existing inbox after failure: a saved handoff always retains ownership.
pub fn route(scope_path:&Path,scope:&StatusToolScope,active:&ToolScope,session:&str,context:&str,
    classify:impl FnOnce(&str,&str)->Result<String,String>)->Result<Option<Value>,String> {
    if !active.actions_allowed {return Ok(None)}
    let sender:Session=serde_json::from_slice(&std::fs::read(scope_path.with_extension("session.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if sender.role!="rich" || sender.entity_id!=scope.entity_id || sender.thread_id!=scope.thread_id
        || session!=sender.session_id || active.binding.session_id!=sender.session_id
        || active.binding.entity_id!=scope.entity_id || active.binding.thread_id!=scope.thread_id {return Ok(None)}
    let Some(instruction)=sender.instruction.as_ref() else {return Ok(None)};
    if !user_delivery(scope,&active.binding.turn_id,instruction)? {return Ok(None)}
    let original=crate::work_host::verified_instruction(&scope.state_root,&scope.entity_id,&scope.thread_id,
        &instruction.ledger_ref,&instruction.sha256).map_err(str::to_string)?;
    let mut choice="already_saved".to_string();
    if session_tools::saved_handoff(scope,instruction)?.is_none() {
        choice=classify(&original,context).unwrap_or_else(|_|"unavailable".into());
        if choice=="stu" {
            session_tools::delegate(scope_path,scope,json!({"text":json!({"original_user_request":original,"recent_conversation":context}).to_string()}))?;
        }
    }
    let saved=session_tools::saved_handoff(scope,instruction)?;
    Ok(Some(json!({"event":"execution_routing","owner":if saved.is_some(){"stu"}else{"rich"},
        "classifier":choice,"handoff_saved":saved.is_some(),"instruction_ledger_ref":instruction.ledger_ref})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ecs::{Binding,EcsBridge},entity::EntityId,ledger::{Ledger,Source}};
    use sha2::Digest;
    const CONTEXT:&str="assistant: The proposal needs repository investigation.\nuser: Go ahead";
    fn fixture()->(std::path::PathBuf,StatusToolScope,ToolScope,Ledger,crate::entity::ThreadBinding) {
        let root=std::env::temp_dir().join(format!("richos-routing-{}",uuid::Uuid::new_v4()));
        let mut ledger=Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
        let entity=EntityId::parse("company").unwrap();let thread=ledger.create_thread("work",&entity).unwrap();
        let b=ledger.thread_binding(&thread).unwrap();let text="Investigate and implement the authorized change";
        let turn=ledger.record_prompt_received(&b,text,Source::Text).unwrap();
        let i=UserInstruction::with_text(format!("ledger:{thread}:{turn}"),format!("{:x}",sha2::Sha256::digest(text)),Some(text));
        let scope=StatusToolScope {version:1,state_root:root.join("engine-state"),entity_id:entity.to_string(),thread_id:thread.clone()};
        let path=root.join("status.json");session_tools::bind(&path,&scope,"native-rich","rich",Some(i.clone())).unwrap();
        crate::assignment::register(&scope.state_root,&crate::assignment::Registration {
            entity_id:scope.entity_id.clone(),thread_id:scope.thread_id.clone(),obligation_id:"work-fixture".into(),
            instruction_ledger_ref:i.ledger_ref.clone(),instruction_sha256:i.sha256.clone(),title:text.into(),repositories:vec![],needs_screen:false}).unwrap();
        let active=ToolScope {version:1,actions_allowed:true,background_work_allowed:false,closed:None,user_instruction:Some(i),seat:None,
            bridge:EcsBridge {python:root.join("python"),component:root.join("ecs"),state_root:root.join("state")},
            binding:Binding {entity_id:entity.to_string(),thread_id:thread,session_id:"native-rich".into(),turn_id:turn,audience:"rich".into(),revision:1}};
        (path,scope,active,ledger,b)
    }
    #[test]
    fn fallback_never_dispatches_but_a_saved_handoff_survives_failure_and_restart() {
        let (path,scope,mut active,mut ledger,binding)=fixture();
        let payload="native-rich";
        for result in [Ok("rich".into()),Ok("uncertain".into()),Ok("invalid".into()),Err("timeout".into())] {
            let receipt=route(&path,&scope,&active,payload,CONTEXT,|text,context| {assert_eq!(text,"Investigate and implement the authorized change");assert!(context.contains("repository investigation"));result}).unwrap().unwrap();
            assert_eq!(receipt["owner"],"rich");assert!(session_tools::pending(&scope,"stu").unwrap().is_none());
        }
        // Persist succeeded but the caller lost its response. Failure must NOT hand
        // execution back to Rich, even when this delivery is subsequently recovered.
        let receipt=route(&path,&scope,&active,payload,CONTEXT,|_,_| {
            session_tools::delegate(&path,&scope,json!({"text":"Original user request and Rich's findings"})).unwrap();Err("lost response".into())
        }).unwrap().unwrap();assert_eq!(receipt["owner"],"stu");
        let (_,through)=session_tools::pending(&scope,"stu").unwrap().unwrap();session_tools::acknowledge(&scope,"stu",through).unwrap();
        ledger.mark_turn_started(&active.binding.turn_id,"native-before-crash").unwrap();
        active.binding.turn_id=ledger.replay_turn(&binding,&active.binding.turn_id).unwrap();
        let receipt=route(&path,&scope,&active,payload,CONTEXT,|_,_|panic!("saved ownership must skip classification")).unwrap().unwrap();
        assert_eq!(receipt["owner"],"stu");
        assert_eq!(session_tools::pending(&scope,"stu").unwrap().unwrap().1,through,"accepted unfinished work must survive restart");
        let record=crate::assignment::read_all(&scope.state_root,&scope.entity_id,&scope.thread_id).unwrap().remove(0);
        crate::assignment::advance(&scope.state_root,&scope.entity_id,&scope.thread_id,&record.id,crate::assignment::AssignmentState::Settled,"Stu verified completion").unwrap();
        assert!(session_tools::pending(&scope,"stu").unwrap().is_none());
        assert_eq!(route(&path,&scope,&active,payload,CONTEXT,|_,_|panic!("a completed saved request was classified twice")).unwrap().unwrap()["owner"],"stu");
        assert!(session_tools::pending(&scope,"stu").unwrap().is_none(),"completed work was dispatched again");
        drop(ledger);std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn peer_reports_and_hidden_preparation_cannot_route_user_work_again() {
        let (path,scope,mut active,mut ledger,binding)=fixture();
        let payload="native-rich";
        let original_turn=active.binding.turn_id.clone();
        active.actions_allowed=false;
        assert!(route(&path,&scope,&active,payload,CONTEXT,|_,_|panic!("hidden preparation classified")).unwrap().is_none());
        active.actions_allowed=true;active.binding.turn_id=ledger.record_prompt_received(&binding,"Work result",Source::Agent).unwrap();
        assert!(route(&path,&scope,&active,payload,CONTEXT,|_,_|panic!("peer report classified")).unwrap().is_none());
        active.binding.turn_id=original_turn;
        assert!(route(&path,&scope,&active,"another-session",CONTEXT,|_,_|panic!("stale session classified")).unwrap().is_none());
        let receipt=route(&path,&scope,&active,payload,CONTEXT,|_,_|Ok("stu".into())).unwrap().unwrap();assert_eq!(receipt["owner"],"stu");
        let handoff=session_tools::pending(&scope,"stu").unwrap().unwrap().0;assert!(handoff.handoff);
        let data:Value=serde_json::from_str(&handoff.text).unwrap();assert!(data["recent_conversation"].as_str().unwrap().contains("repository investigation"));
        drop(ledger);std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
