//! Direct native-session evidence and durable peer input. No provider turn lock is used.
use crate::status_tools::StatusToolScope;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub role: String,
    pub entity_id: String,
    pub thread_id: String,
    #[serde(default)]
    pub instruction: Option<crate::ecs::UserInstruction>,
}

fn component(text: &str) -> bool {
    !text.is_empty() && text.len() <= 128
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn bind(scope_path: &Path, scope: &StatusToolScope, session: &str, role: &str, instruction: Option<crate::ecs::UserInstruction>) -> Result<(), String> {
    if !component(session) || !matches!(role, "rich" | "stu") {
        return Err("Invalid native session binding".into());
    }
    let instruction=if role == "stu" {None} else {instruction.or_else(||std::fs::read(scope_path.with_extension("session.json")).ok()
        .and_then(|bytes|serde_json::from_slice::<Session>(&bytes).ok())
        .filter(|old|old.session_id==session && old.role==role && old.entity_id==scope.entity_id && old.thread_id==scope.thread_id)
        .and_then(|old|old.instruction))};
    let record = Session {session_id:session.into(), role:role.into(),
        entity_id:scope.entity_id.clone(), thread_id:scope.thread_id.clone(),instruction};
    let text = serde_json::to_string(&record).map_err(|e|e.to_string())?;
    let folder=scope.state_root.join("evidence").join(session);
    std::fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    crate::doctrine::write_verified(&folder.join("binding.json"), &text).map_err(|e|e.to_string())?;
    crate::doctrine::write_verified(&scope_path.with_extension("session.json"), &text).map_err(|e|e.to_string())
}

fn sessions(scope: &StatusToolScope) -> Result<Vec<Session>, String> {
    let entries=match std::fs::read_dir(scope.state_root.join("evidence")) {
        Ok(entries)=>entries, Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(vec![]),
        Err(e)=>return Err(e.to_string()),
    };
    let mut found=Vec::new();
    for entry in entries {
        let entry=entry.map_err(|e|e.to_string())?;
        if !entry.file_type().map_err(|e|e.to_string())?.is_dir() {continue}
        let path=entry.path().join("binding.json");
        if !path.exists() {continue}
        let record:Session=serde_json::from_slice(&std::fs::read(&path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if record.entity_id==scope.entity_id && record.thread_id==scope.thread_id
            && entry.file_name()==record.session_id.as_str() && component(&record.session_id) {
            found.push(record);
        }
    }
    found.sort_by(|a,b|a.session_id.cmp(&b.session_id));
    Ok(found)
}

fn native_path(scope: &StatusToolScope, session: &str) -> Result<PathBuf,String> {
    if !component(session) {return Err("Invalid native session identity".into())}
    let record:Session=serde_json::from_slice(&std::fs::read(scope.state_root.join("evidence").join(session).join("binding.json"))
        .map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if record.session_id!=session || record.entity_id!=scope.entity_id || record.thread_id!=scope.thread_id {
        return Err("That native session does not belong to this company and conversation".into());
    }
    let file=std::fs::File::open(scope.state_root.join("evidence").join(session).join("callbacks.jsonl")).map_err(|e|e.to_string())?;
    // SessionStart is near the beginning. Do not scan an entire live transcript to locate it.
    for line in BufReader::new(file.take(1024*1024)).lines() {
        let line=line.map_err(|e|e.to_string())?;
        let Ok(record)=serde_json::from_str::<Value>(&line) else {continue};
        let Some(raw)=record.pointer("/callback/transcript_path").and_then(Value::as_str) else {continue};
        let path=PathBuf::from(raw);
        if path.is_absolute() && path.file_name().is_some_and(|n|n==format!("{session}.jsonl").as_str()) {
            return Ok(path);
        }
    }
    Err("The provider has not recorded this session's native transcript path".into())
}

pub fn call(scope: &StatusToolScope, name: &str, args: Value) -> Result<Value,String> {
    if name=="sessions" {return Ok(json!({"sessions":sessions(scope)?.into_iter().map(|s|json!({
        "session_id":s.session_id,"role":s.role,"entity_id":s.entity_id,"thread_id":s.thread_id})).collect::<Vec<_>>()}))}
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ReadArgs {session_id:String, worker:Option<String>, offset:Option<u64>, limit:Option<usize>}
    let args:ReadArgs=serde_json::from_value(args).map_err(|e|e.to_string())?;
    let mut path=native_path(scope,&args.session_id)?;
    if let Some(worker)=args.worker {
        if !component(&worker) {return Err("Invalid worker identity".into())}
        path=path.with_extension("").join("subagents").join(format!("agent-{worker}.jsonl"));
    }
    let mut file=std::fs::File::open(&path).map_err(|e|e.to_string())?;
    let length=file.metadata().map_err(|e|e.to_string())?.len();
    let limit=args.limit.unwrap_or(64*1024).clamp(1,128*1024);
    let offset=args.offset.unwrap_or_else(||length.saturating_sub(limit as u64)).min(length);
    file.seek(SeekFrom::Start(offset)).map_err(|e|e.to_string())?;
    let mut bytes=Vec::new();file.take(limit as u64).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    Ok(json!({"session_id":args.session_id,"path":path,"offset":offset,
        "next_offset":offset+bytes.len() as u64,"observed_length":length,
        "native_jsonl":String::from_utf8_lossy(&bytes),"partial_lines_possible":true}))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {#[serde(default, skip_serializing_if="std::ops::Not::not")] pub handoff:bool, pub from:String, pub text:String, #[serde(default)] pub instruction:Option<crate::ecs::UserInstruction>}

pub fn inbox(scope: &StatusToolScope, role: &str) -> Result<PathBuf,String> {
    if !component(&scope.entity_id) || !component(&scope.thread_id) || !matches!(role,"rich"|"stu") {
        return Err("Invalid conversation message scope".into())
    }
    Ok(scope.state_root.join("messages").join(&scope.entity_id).join(&scope.thread_id).join(format!("{role}.jsonl")))
}

pub fn send(scope_path: &Path, scope: &StatusToolScope, args:Value) -> Result<Value,String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SendArgs {text:String}
    let args:SendArgs=serde_json::from_value(args).map_err(|e|e.to_string())?;
    if args.text.trim().is_empty() || args.text.len()>128*1024 {return Err("Message must contain 1–131072 bytes".into())}
    let sender:Session=serde_json::from_slice(&std::fs::read(scope_path.with_extension("session.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if sender.entity_id!=scope.entity_id || sender.thread_id!=scope.thread_id {return Err("Message scope changed".into())}
    let peer=match sender.role.as_str() {"rich"=>"stu","stu"=>"rich",_=>return Err("Invalid sender role".into())};
    append(scope,peer,&Message {handoff:false,from:sender.role,text:args.text,instruction:sender.instruction})?;
    Ok(json!({"saved":true,"to":peer}))
}

/// A host observation is information, never a user instruction or a peer's authorship.
pub(crate) fn observe_stall(scope:&StatusToolScope,text:&str)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"stall-watch".into(),text:text.into(),instruction:None})
}

pub(crate) fn observe_user_stop(scope:&StatusToolScope,assignment:&str,obligation:&str,at:u64)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"user-control".into(),text:json!({"event":"user_stop",
        "assignment_id":assignment,"obligation_id":obligation,"requested_at_ms":at}).to_string(),instruction:None})
}

/// Execution ownership is recorded in the existing inbox, using the original ledger reference.
pub fn delegate(scope_path:&Path,scope:&StatusToolScope,args:Value)->Result<Value,String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Args {text:String}
    let args:Args=serde_json::from_value(args).map_err(|e|e.to_string())?;
    if args.text.trim().is_empty() || args.text.len()>128*1024 {return Err("Handoff must contain 1–131072 bytes".into())}
    let sender:Session=serde_json::from_slice(&std::fs::read(scope_path.with_extension("session.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if sender.role!="rich" || sender.entity_id!=scope.entity_id || sender.thread_id!=scope.thread_id {return Err("Only this conversation's Rich can hand work to Stu".into())}
    let mut instruction=sender.instruction.ok_or("No original user request is bound to this turn")?;
    let text=crate::work_host::verified_instruction(&scope.state_root,&scope.entity_id,&scope.thread_id,&instruction.ledger_ref,&instruction.sha256).map_err(str::to_string)?;
    instruction=crate::ecs::UserInstruction::with_text(instruction.ledger_ref,instruction.sha256,Some(&text));
    let message=Message {handoff:true,from:"rich".into(),text:args.text,instruction:Some(instruction)};
    let (through,already_saved)=append_record(scope,"stu",&message,true)?;
    Ok(json!({"saved":true,"to":"stu","through":through,"already_saved":already_saved,"instruction_ledger_ref":message.instruction.unwrap().ledger_ref}))
}

pub fn saved_handoff(scope:&StatusToolScope,instruction:&crate::ecs::UserInstruction)->Result<Option<(Message,u64)>,String> {
    let path=inbox(scope,"stu")?;
    let file=match std::fs::File::open(path) {Ok(file)=>file,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    find_handoff(&mut BufReader::new(file),instruction)
}

fn find_handoff(reader:&mut impl BufRead,instruction:&crate::ecs::UserInstruction)->Result<Option<(Message,u64)>,String> {
    let mut through=0;
    loop {
        let mut line=String::new();
        let count=reader.take(256*1024+1).read_line(&mut line).map_err(|e|e.to_string())?;
        if count>256*1024 {return Err("Saved peer input exceeds its frame limit".into())}
        if count==0 || !line.ends_with('\n') {return Ok(None)}
        through+=count as u64;
        let message:Message=serde_json::from_str(&line).map_err(|e|e.to_string())?;
        if message.handoff && message.instruction.as_ref().is_some_and(|i|i.ledger_ref==instruction.ledger_ref) {
            if message.instruction.as_ref().unwrap().sha256!=instruction.sha256 {return Err("Saved handoff disagrees with the original request".into())}
            return Ok(Some((message,through)))
        }
    }
}

fn append(scope:&StatusToolScope,role:&str,message:&Message)->Result<(),String> {
    append_record(scope,role,message,false).map(|_|())
}

fn append_record(scope:&StatusToolScope,role:&str,message:&Message,unique:bool)->Result<(u64,bool),String> {

    let path=inbox(scope,role)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e|e.to_string())?;
    let mut file=std::fs::OpenOptions::new().create(true).append(true).read(true).open(&path).map_err(|e|e.to_string())?;
    #[cfg(unix)]
    {use std::os::fd::AsRawFd;
        // Locks are released by closing this handle on success or any error.
        if unsafe {libc::flock(file.as_raw_fd(),libc::LOCK_EX)}!=0 {return Err(std::io::Error::last_os_error().to_string())}
    }
    let mut record=serde_json::to_vec(message).map_err(|e|e.to_string())?;record.push(b'\n');
    if record.len()>256*1024 {return Err("Encoded message exceeds the input frame limit".into())}
    let length=file.metadata().map_err(|e|e.to_string())?.len();
    if length>0 {
        let start=length.saturating_sub(256*1024);
        file.seek(SeekFrom::Start(start)).map_err(|e|e.to_string())?;
        let mut tail=Vec::new();file.read_to_end(&mut tail).map_err(|e|e.to_string())?;
        if tail.last()!=Some(&b'\n') {
            let complete=tail.iter().rposition(|b|*b==b'\n').map(|n|start+n as u64+1).unwrap_or(0);
            if complete==0 && start>0 {return Err("Incomplete message exceeds the frame limit".into())}
            file.set_len(complete).map_err(|e|e.to_string())?;
        }
    }
    if unique {
        file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
        if let Some((_,through))=find_handoff(&mut BufReader::new(&mut file),message.instruction.as_ref().ok_or("Handoff has no request")?)? {file.sync_all().map_err(|e|e.to_string())?;return Ok((through,true))}
    }
    file.write_all(&record).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
    Ok((file.metadata().map_err(|e|e.to_string())?.len(),false))
}

/// The cursor is acknowledged only after the receiving path durably accepts the record.
pub fn pending(scope: &StatusToolScope, role: &str) -> Result<Option<(Message,u64)>,String> {
    let path=inbox(scope,role)?;
    let cursor=cursor_state(&path)?;
    let accepted=cursor["through"].as_u64().unwrap();
    let mut unfinished=None;
    for input in cursor["active"].as_object().unwrap().keys() {
        let Some(offset)=input.strip_prefix("peer:") else {continue};
        let offset=offset.parse::<u64>().map_err(|_|"Invalid unfinished input identity")?;
        if offset<=accepted {unfinished=Some(unfinished.map_or(offset,|old:u64|old.min(offset)));}
    }
    if let Some(through)=unfinished {
        // A live correction can advance acceptance past the native turn still
        // executing. Recover that exact saved input and its previous session.
        // The existing frame bound keeps this read independent of inbox history.
        let mut file=std::fs::File::open(&path).map_err(|e|e.to_string())?;
        let length=file.metadata().map_err(|e|e.to_string())?.len();
        if through==0 || through>length || accepted>length {
            return Err("Unfinished input no longer matches its durable inbox".into());
        }
        let start=through.saturating_sub(256*1024+1);
        file.seek(SeekFrom::Start(start)).map_err(|e|e.to_string())?;
        let mut bytes=Vec::new();file.take(through-start).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
        if bytes.last()!=Some(&b'\n') {return Err("Unfinished input is not a complete durable record".into())}
        let end=bytes.len()-1;
        let from=bytes[..end].iter().rposition(|byte|*byte==b'\n').map_or(0,|n|n+1);
        if from==0 && start>0 {return Err("Unfinished input exceeds the existing frame limit".into())}
        let message=serde_json::from_slice(&bytes[from..end]).map_err(|e|e.to_string())?;
        return Ok(Some((message,through)));
    }
    pending_after(scope, role, 0)
}

/// Skip only input already being handled on this live session, without acknowledging it.
pub(crate) fn pending_after(scope:&StatusToolScope,role:&str,after:u64)->Result<Option<(Message,u64)>,String> {
    pending_matching(scope,role,after,true)
}

/// Live communication may pass queued execution, without consuming that execution.
pub(crate) fn pending_live_after(scope:&StatusToolScope,role:&str,after:u64)->Result<Option<(Message,u64)>,String> {
    pending_matching(scope,role,after,false)
}

fn pending_matching(scope:&StatusToolScope,role:&str,after:u64,execution:bool)->Result<Option<(Message,u64)>,String> {
    let path=inbox(scope,role)?;
    let mut file=match std::fs::File::open(&path) {Ok(f)=>f,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    let cursor=cursor_state(&path)?;
    let mut through=cursor["through"].as_u64().ok_or("Invalid input cursor")?.max(after);
    if through>file.metadata().map_err(|e|e.to_string())?.len() {return Err("Message input is shorter than its saved cursor".into())}
    file.seek(SeekFrom::Start(through)).map_err(|e|e.to_string())?;
    let mut reader=BufReader::new(file);
    loop {
        let mut line=String::new();let count=(&mut reader).take(256*1024+1).read_line(&mut line).map_err(|e|e.to_string())?;
        if count>256*1024 {return Err("Saved peer input exceeds its frame limit".into())}
        if count==0 || !line.ends_with('\n') {return Ok(None)}
        through+=count as u64;
        if cursor["accepted"].get(through.to_string()).is_some() {continue}
        let message:Message=serde_json::from_str(&line).map_err(|e|e.to_string())?;
        if execution || !message.handoff {return Ok(Some((message,through)))}
    }
}

pub fn acknowledge(scope:&StatusToolScope,role:&str,through:u64)->Result<(),String> {
    let path=inbox(scope,role)?;
    update_cursor(scope,role,|cursor| {
        cursor["active"].as_object_mut().unwrap().remove(&format!("peer:{through}"));
        let mut frontier=cursor["through"].as_u64().unwrap();
        if through<=frontier {return Ok(())}
        cursor["accepted"][through.to_string()]=json!(true);
        let mut file=std::fs::File::open(&path).map_err(|e|e.to_string())?;
        file.seek(SeekFrom::Start(frontier)).map_err(|e|e.to_string())?;
        let mut reader=BufReader::new(file);
        loop {
            let mut line=String::new();let count=(&mut reader).take(256*1024+1).read_line(&mut line).map_err(|e|e.to_string())?;
            if count>256*1024 {return Err("Saved peer input exceeds its frame limit".into())}
            if count==0 || !line.ends_with('\n') {break}
            let next=frontier+count as u64;
            if cursor["accepted"].as_object_mut().unwrap().remove(&next.to_string()).is_none() {break}
            frontier=next;
        }
        cursor["through"]=json!(frontier);Ok(())
    })
}

fn update_cursor(scope:&StatusToolScope,role:&str,edit:impl FnOnce(&mut Value)->Result<(),String>)->Result<(),String> {
    let path=inbox(scope,role)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e|e.to_string())?;
    let lock=std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e|e.to_string())?;
    #[cfg(unix)] {
        use std::os::fd::AsRawFd;
        if unsafe {libc::flock(lock.as_raw_fd(),libc::LOCK_EX)}!=0 {return Err(std::io::Error::last_os_error().to_string())}
    }
    let mut cursor=cursor_state(&path)?;
    edit(&mut cursor)?;
    crate::doctrine::write_verified(&path.with_extension("cursor"),&cursor.to_string()).map_err(|e|e.to_string())
}

fn cursor_state(inbox:&Path)->Result<Value,String> {
    let mut value=match std::fs::read_to_string(inbox.with_extension("cursor")) {
        Ok(text)=>serde_json::from_str::<Value>(&text).map_err(|e|e.to_string())?,
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>json!({"through":0}),
        Err(e)=>return Err(e.to_string()),
    };
    if value.is_u64() {value=json!({"through":value});}
    if !value.is_object() || !value["through"].is_u64() {return Err("Invalid input cursor".into())}
    if let Some(input)=value["active"]["input"].as_str().map(str::to_string) {
        let previous=value["active"].clone();value["active"]=json!({input:previous});
    } else if value["active"].is_null() {value["active"]=json!({});}
    if !value["active"].is_object() {return Err("Invalid active input record".into())}
    if value["accepted"].is_null() {value["accepted"]=json!({});}
    if !value["accepted"].as_object().is_some_and(|rows|rows.iter().all(|(key,value)|key.parse::<u64>().is_ok() && value==true)) {return Err("Invalid accepted input receipts".into())}
    Ok(value)
}

pub fn previous_attempt(scope:&StatusToolScope,role:&str,input:&str)->Result<Option<Value>,String> {
    Ok(cursor_state(&inbox(scope,role)?)?["active"].get(input).cloned())
}

/// Keep uncertain peer delivery and question-answer evidence independently until accepted.
pub fn complete_attempt(scope:&StatusToolScope,role:&str,input:&str)->Result<(),String> {
    update_cursor(scope, role, |cursor| { cursor["active"].as_object_mut().unwrap().remove(input); Ok(()) })
}

/// Preserve the actual native session that may have acted before an interrupted delivery.
pub fn begin_attempt(scope:&StatusToolScope,role:&str,input:&str,session:&str)->Result<(),String> {
    update_cursor(scope, role, |cursor| { cursor["active"][input]=json!({"input":input,"session_id":session}); Ok(()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_hook_and_manual_handoffs_survive_restart_without_duplicating_execution() {
        use crate::{ledger::{Ledger,Source},entity::EntityId};
        use sha2::Digest;
        let root=std::env::temp_dir().join(format!("richos-handoff-{}",uuid::Uuid::new_v4()));
        let mut ledger=Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
        let entity=EntityId::parse("company").unwrap();
        let thread=ledger.create_thread("work",&entity).unwrap();
        let binding=ledger.thread_binding(&thread).unwrap();
        let original="Investigate the repository and implement the requested fix";
        let turn=ledger.record_prompt_received(&binding,original,Source::Text).unwrap();
        let instruction=crate::ecs::UserInstruction::with_text(format!("ledger:{thread}:{turn}"),format!("{:x}",sha2::Sha256::digest(original)),Some(original));
        let scope=StatusToolScope {version:1,state_root:root.join("engine-state"),entity_id:entity.to_string(),thread_id:thread};
        let path=root.join("status.json");bind(&path,&scope,"native-rich","rich",Some(instruction.clone())).unwrap();
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(3));
        let results=std::thread::scope(|threads| {
            let run=|text| {let b=barrier.clone();let path=&path;let scope=&scope;threads.spawn(move || {b.wait();delegate(path,scope,json!({"text":text})).unwrap()})};
            let hook=run("Original request and recorded context");let manual=run("Rich's handoff with his findings");barrier.wait();
            [hook.join().unwrap(),manual.join().unwrap()]
        });
        assert_eq!(results[0]["through"],results[1]["through"]);
        assert_ne!(results[0]["already_saved"],results[1]["already_saved"]);
        let (message,through)=pending(&scope,"stu").unwrap().unwrap();assert!(message.handoff);
        assert_eq!(message.instruction.as_ref().unwrap().text.as_deref(),Some(original));
        begin_attempt(&scope,"stu",&format!("peer:{through}"),"stu-before-crash").unwrap();
        // A fresh Rich session after a crash retries the same canonical request.
        bind(&path,&scope,"rich-after-crash","rich",Some(instruction.clone())).unwrap();
        assert_eq!(delegate(&path,&scope,json!({"text":"Retry after uncertain HTTP result"})).unwrap()["through"],through);
        assert_eq!(previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap().unwrap()["session_id"],"stu-before-crash");
        acknowledge(&scope,"stu",through).unwrap();
        assert!(delegate(&path,&scope,json!({"text":"Retry after Stu consumed it"})).unwrap()["already_saved"].as_bool().unwrap());
        assert!(pending(&scope,"stu").unwrap().is_none());
        send(&path,&scope,json!({"text":"A correction is still ordinary independent communication"})).unwrap();
        let (correction,offset)=pending(&scope,"stu").unwrap().unwrap();assert!(!correction.handoff);acknowledge(&scope,"stu",offset).unwrap();
        // Identical words in a NEW user turn remain a distinct request.
        let next=ledger.record_prompt_received(&binding,original,Source::Text).unwrap();
        let next_instruction=crate::ecs::UserInstruction::with_text(format!("ledger:{}:{next}",scope.thread_id),instruction.sha256.clone(),Some(original));
        bind(&path,&scope,"rich-after-crash","rich",Some(next_instruction)).unwrap();
        let next=delegate(&path,&scope,json!({"text":"The user asked again"})).unwrap();assert_ne!(next["through"],through);
        bind(&path,&scope,"native-stu","stu",Some(instruction.clone())).unwrap();assert!(delegate(&path,&scope,json!({"text":"wrong role"})).is_err());
        let mut forged=instruction;forged.sha256="0".repeat(64);bind(&path,&scope,"native-rich","rich",Some(forged)).unwrap();
        assert!(delegate(&path,&scope,json!({"text":"forged request"})).is_err());
        drop(ledger);std::fs::remove_dir_all(root).unwrap();
    }


    #[test]
    fn a_live_update_ack_does_not_hide_an_older_unfinished_native_turn_on_restart() {
        let root=std::env::temp_dir().join(format!("richos-peer-frontier-{}",uuid::Uuid::new_v4()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"thread".into()};
        let path=root.join("scope.json");bind(&path,&scope,"native-rich","rich",None).unwrap();
        for _ in 0..3 {
            send(&path,&scope,json!({"text":"x".repeat(128*1024)})).unwrap();
            let through=pending(&scope,"stu").unwrap().unwrap().1;
            acknowledge(&scope,"stu",through).unwrap();
        }
        send(&path,&scope,json!({"text":"Original work still in progress"})).unwrap();
        let original=pending(&scope,"stu").unwrap().unwrap().1;
        let original_input=format!("peer:{original}");
        begin_attempt(&scope,"stu",&original_input,"native-before-restart").unwrap();
        send(&path,&scope,json!({"text":"Correction to that ongoing work"})).unwrap();
        let update=pending_after(&scope,"stu",original).unwrap().unwrap().1;
        begin_attempt(&scope,"stu",&format!("peer:{update}"),"native-before-restart").unwrap();
        acknowledge(&scope,"stu",update).unwrap();
        // Fresh reads after a process boundary must recover the unfinished input,
        // despite a larger accepted cursor. Its native session may already have acted.
        let saved=previous_attempt(&scope,"stu",&original_input).unwrap().unwrap();
        assert_eq!(saved["session_id"],"native-before-restart");
        let (message,through)=pending(&scope,"stu").unwrap().expect("unfinished native turn was hidden behind a later live update acknowledgement");
        assert_eq!(through,original);assert_eq!(message.text,"Original work still in progress");
        let cursor_file=inbox(&scope,"stu").unwrap().with_extension("cursor");
        let saved_cursor=std::fs::read_to_string(&cursor_file).unwrap();
        let mut damaged:Value=serde_json::from_str(&saved_cursor).unwrap();
        damaged["through"]=json!(std::fs::metadata(inbox(&scope,"stu").unwrap()).unwrap().len()+1);
        crate::doctrine::write_verified(&cursor_file,&damaged.to_string()).unwrap();
        let refused=pending(&scope,"stu").is_err();
        crate::doctrine::write_verified(&cursor_file,&saved_cursor).unwrap();
        assert!(refused,"unfinished-input recovery must retain the shorter-than-cursor refusal");
        acknowledge(&scope,"stu",original).unwrap();
        assert!(pending(&scope,"stu").unwrap().is_none(),"recovery must not rewind an accepted correction");
        assert!(previous_attempt(&scope,"stu",&original_input).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn peer_input_is_durable_scoped_and_recovers_an_unfinished_append() {
        let root=std::env::temp_dir().join(format!("richos-peer-{}",uuid::Uuid::new_v4()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"thread".into()};
        let path=root.join("scope.json");
        bind(&path,&scope,"native-rich","rich",None).unwrap();
        send(&path,&scope,json!({"text":"first"})).unwrap();
        let (message,through)=pending(&scope,"stu").unwrap().unwrap();
        assert_eq!(message.from,"rich");assert_eq!(message.text,"first");
        assert_eq!(pending(&scope,"stu").unwrap().unwrap().1,through,"reading must not acknowledge");
        let input=format!("peer:{through}");
        begin_attempt(&scope,"stu",&input,"old-native-session").unwrap();
        begin_attempt(&scope,"stu","answer:question","answer-native-session").unwrap();
        assert_eq!(previous_attempt(&scope,"stu",&input).unwrap().unwrap()["session_id"],"old-native-session");
        assert!(previous_attempt(&scope,"stu","peer:2").unwrap().is_none());
        acknowledge(&scope,"stu",through).unwrap();assert!(pending(&scope,"stu").unwrap().is_none());
        assert!(previous_attempt(&scope,"stu",&input).unwrap().is_none());
        assert_eq!(previous_attempt(&scope,"stu","answer:question").unwrap().unwrap()["session_id"],"answer-native-session");
        complete_attempt(&scope,"stu","answer:question").unwrap();
        assert!(previous_attempt(&scope,"stu","answer:question").unwrap().is_none());
        let mut file=std::fs::OpenOptions::new().append(true).open(inbox(&scope,"stu").unwrap()).unwrap();
        file.write_all(b"{\"from\":").unwrap();file.sync_all().unwrap();
        assert!(pending(&scope,"stu").unwrap().is_none());
        send(&path,&scope,json!({"text":"second"})).unwrap();
        assert_eq!(pending(&scope,"stu").unwrap().unwrap().0.text,"second");
        let other=StatusToolScope {thread_id:"other".into(),..scope.clone()};
        assert!(pending(&other,"stu").unwrap().is_none());assert!(send(&path,&other,json!({"text":"wrong"})).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_transcript_read_needs_the_host_binding_and_reads_current_tool_results() {
        let root=std::env::temp_dir().join(format!("richos-native-read-{}",uuid::Uuid::new_v4()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"thread".into()};
        let path=root.join("status.json");bind(&path,&scope,"native-stu","stu",Some(crate::ecs::UserInstruction {
            ledger_ref:"ledger:thread:latest-job".into(),sha256:"a".repeat(64),text:None})).unwrap();
        let bound:Session=serde_json::from_slice(&std::fs::read(path.with_extension("session.json")).unwrap()).unwrap();
        assert!(bound.instruction.is_none(),"Stu must not inherit a job's authority from session binding");
        let transcript=root.join("native-stu.jsonl");std::fs::write(&transcript,b"{\"type\":\"tool_result\",\"content\":\"actual-output\"}\n").unwrap();
        std::fs::write(root.join("evidence/native-stu/callbacks.jsonl"),format!("{}\n",json!({"callback":{"transcript_path":transcript}}))).unwrap();
        let listed=call(&scope,"sessions",json!({})).unwrap();
        assert!(listed["sessions"][0].get("instruction").is_none(),"listing sessions must not replay authorization history");
        let unrelated=root.join("evidence/unrelated-native");std::fs::create_dir_all(&unrelated).unwrap();
        std::fs::write(unrelated.join("binding.json"),b"interrupted metadata").unwrap();
        let answer=call(&scope,"transcript",json!({"session_id":"native-stu","offset":0})).unwrap();
        assert!(answer["native_jsonl"].as_str().unwrap().contains("actual-output"));
        let other=StatusToolScope {entity_id:"other".into(),..scope.clone()};
        assert!(call(&other,"transcript",json!({"session_id":"native-stu"})).is_err());
        assert!(call(&scope,"transcript",json!({"session_id":"../outside"})).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
