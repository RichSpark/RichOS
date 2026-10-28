//! Direct native-session evidence and durable peer input. No provider turn lock is used.
//! Transport acceptance advances the existing cursor. An accepted handoff stays active
//! until its matching Stu report is terminal or the user's Stop is durable.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignments_scope: Option<PathBuf>,
}

fn component(text: &str) -> bool {
    !text.is_empty() && text.len() <= 128
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn bind(scope_path: &Path, scope: &StatusToolScope, session: &str, role: &str, instruction: Option<crate::ecs::UserInstruction>) -> Result<(), String> {
    bind_with_assignments(scope_path,scope,session,role,instruction,None)
}

pub(crate) fn bind_with_assignments(scope_path: &Path, scope: &StatusToolScope, session: &str, role: &str,
    instruction: Option<crate::ecs::UserInstruction>, assignments: Option<&Path>) -> Result<(), String> {
    if !component(session) || !matches!(role, "rich" | "stu") {
        return Err("Invalid native session binding".into());
    }
    let instruction=if role == "stu" {None} else {instruction.or_else(||std::fs::read(scope_path.with_extension("session.json")).ok()
        .and_then(|bytes|serde_json::from_slice::<Session>(&bytes).ok())
        .filter(|old|old.session_id==session && old.role==role && old.entity_id==scope.entity_id && old.thread_id==scope.thread_id)
        .and_then(|old|old.instruction))};
    let record = Session {session_id:session.into(), role:role.into(),
        entity_id:scope.entity_id.clone(), thread_id:scope.thread_id.clone(),instruction,
        assignments_scope:assignments.filter(|_|role=="rich").map(Path::to_path_buf)};
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

pub(crate) fn native_path(scope: &StatusToolScope, session: &str) -> Result<PathBuf,String> {
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

/// References to the existing work record, not another execution or tracking record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkReference {pub assignment_id:String, pub obligation_id:String}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {#[serde(default, skip_serializing_if="std::ops::Not::not")] pub handoff:bool, pub from:String, pub text:String, #[serde(default)] pub instruction:Option<crate::ecs::UserInstruction>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub work:Option<WorkReference>}

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
    append(scope,peer,&Message {handoff:false,from:sender.role,text:args.text,instruction:sender.instruction,work:None})?;
    Ok(json!({"saved":true,"to":peer}))
}

/// A host observation is information, never a user instruction or a peer's authorship.
pub(crate) fn observe_stall(scope:&StatusToolScope,text:&str)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"stall-watch".into(),text:text.into(),instruction:None,work:None})
}

/// Codex wrote a new entry to Rich in one of the user's Codex channels (`codex_watch.rs`): the
/// engine watcher's block, as written, for Rich. A host observation, like the stall watch's.
pub(crate) fn observe_codex(scope:&StatusToolScope,text:&str)->Result<(),String> {
    append(scope,"rich",&Message {handoff:false,from:"codex-watch".into(),text:text.into(),instruction:None,work:None})
}

pub(crate) fn observe_work_status(scope:&StatusToolScope,facts:Value)->Result<(),String> {
    append(scope,"rich",&Message {handoff:false,from:"work-status".into(),text:facts.to_string(),instruction:None,work:None})
}

/// One of Stu's workers, or an automatic turn of his own, met an overload: its wait is over, or
/// its retries are used up (`work_host.rs` observe_worker, observe_automatic_turn). A host
/// observation, like the stall watch's.
pub(crate) fn observe_overload_retry(scope:&StatusToolScope,facts:&Value)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"overload-retry".into(),text:facts.to_string(),instruction:None,work:None})
}

/// Stu's Claude account is being switched (`account_switch`: something of his still runs on the
/// account being left) or he has been moved to it (`account_moved`) (`work_host.rs`
/// move_to_account_in_use). A host observation, like the overload retry's.
pub(crate) fn observe_account_switch(scope:&StatusToolScope,facts:&Value)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"account-switch".into(),text:facts.to_string(),instruction:None,work:None})
}

/// The reload after Stu's turn registered a changed team (`work_host.rs` run_message): one
/// more turn for him, so the work he left for it goes on. A host observation, like the others.
pub(crate) fn observe_team_reloaded(scope:&StatusToolScope)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"team-reload".into(),text:json!({"event":"team_reloaded"}).to_string(),instruction:None,work:None})
}

pub(crate) fn observe_user_stop(scope:&StatusToolScope,assignment:&str,obligation:&str,at:u64)->Result<(),String> {
    append(scope,"stu",&Message {handoff:false,from:"user-control".into(),text:json!({"event":"user_stop",
        "assignment_id":assignment,"obligation_id":obligation,"requested_at_ms":at}).to_string(),instruction:None,work:None})
}

/// Execution ownership is recorded in the existing inbox, using the original ledger reference.
pub fn delegate(scope_path:&Path,scope:&StatusToolScope,args:Value)->Result<Value,String> {
    delegate_with(scope_path,scope,args,&crate::assignment_tools::EcsObligations)
}

pub(crate) fn delegate_with(scope_path:&Path,scope:&StatusToolScope,args:Value,
    opener:&dyn crate::assignment_tools::ObligationOpener)->Result<Value,String> {
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
    let mut message=Message {handoff:true,from:"rich".into(),text:args.text,instruction:Some(instruction),work:None};
    let (through,already_saved)=append_record(scope,"stu",&mut message,true,Some((sender.assignments_scope.as_deref(),opener)))?;
    Ok(json!({"saved":true,"to":"stu","through":through,"already_saved":already_saved,
        "work":message.work,"instruction_ledger_ref":message.instruction.unwrap().ledger_ref}))
}

pub fn saved_handoff(scope:&StatusToolScope,instruction:&crate::ecs::UserInstruction)->Result<Option<(Message,u64)>,String> {
    let path=inbox(scope,"stu")?;
    let file=match std::fs::File::open(path) {Ok(file)=>file,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    find_handoff(&mut BufReader::new(file),instruction)
}

fn find_handoff(reader:&mut impl BufRead,instruction:&crate::ecs::UserInstruction)->Result<Option<(Message,u64)>,String> {
    find_record(reader,|message| {
        if !message.handoff || message.instruction.as_ref().is_none_or(|i|i.ledger_ref!=instruction.ledger_ref) {return Ok(false)}
        if message.instruction.as_ref().unwrap().sha256!=instruction.sha256 {return Err("Saved handoff disagrees with the original request".into())}
        Ok(true)
    })
}

fn work_failure_input(message:&Message)->Option<String> {
    if message.from!="work-status" {return None}
    let facts:Value=serde_json::from_str(&message.text).ok()?;
    (facts["event"]=="upstream_turn").then(||facts["input_id"].as_str().map(str::to_string)).flatten()
}

fn find_record(reader:&mut impl BufRead,mut matches:impl FnMut(&Message)->Result<bool,String>)->Result<Option<(Message,u64)>,String> {
    let mut through=0;
    loop {
        let mut line=String::new();
        let count=reader.take(256*1024+1).read_line(&mut line).map_err(|e|e.to_string())?;
        if count>256*1024 {return Err("Saved peer input exceeds its frame limit".into())}
        if count==0 || !line.ends_with('\n') {return Ok(None)}
        through+=count as u64;
        let message:Message=serde_json::from_str(&line).map_err(|e|e.to_string())?;
        if matches(&message)? {return Ok(Some((message,through)))}
    }
}

fn append(scope:&StatusToolScope,role:&str,message:&Message)->Result<(),String> {
    append_record(scope,role,&mut message.clone(),false,None).map(|_|())
}

fn locked_inbox(scope:&StatusToolScope,role:&str)->Result<std::fs::File,String> {
    let path=inbox(scope,role)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e|e.to_string())?;
    let file=std::fs::OpenOptions::new().create(true).append(true).read(true).open(&path).map_err(|e|e.to_string())?;
    #[cfg(unix)]
    {use std::os::fd::AsRawFd;
        // Locks are released by closing this handle on success or any error.
        if unsafe {libc::flock(file.as_raw_fd(),libc::LOCK_EX)}!=0 {return Err(std::io::Error::last_os_error().to_string())}
    }
    Ok(file)
}

pub(crate) fn work_reference(scope:&StatusToolScope,message:&Message)->Result<Option<WorkReference>,String> {
    let Some(instruction)=message.instruction.as_ref().filter(|_|message.handoff) else {return Ok(None)};
    let matches:Vec<_>=crate::assignment::read_all(&scope.state_root,&scope.entity_id,&scope.thread_id)
        .map_err(|e|e.to_string())?.into_iter().filter(|r|r.instruction_ledger_ref==instruction.ledger_ref
            && r.instruction_sha256==instruction.sha256).collect();
    if matches.len()>1 {return Err("This request has more than one operational record; inspect it before continuing".into())}
    let found=matches.first().map(|r|WorkReference {assignment_id:r.id.clone(),obligation_id:r.obligation_id.clone()});
    if message.work.as_ref().is_some_and(|given|Some(given)!=found.as_ref()) {
        return Err("The handoff's work record disagrees with its original request".into())
    }
    Ok(found)
}

fn record_handoff(scope:&StatusToolScope,message:&mut Message,path:Option<&Path>,
    opener:&dyn crate::assignment_tools::ObligationOpener)->Result<(),String> {
    if let Some(work)=work_reference(scope,message)? {message.work=Some(work);return Ok(())}
    let instruction=message.instruction.as_ref().ok_or("Handoff has no original request")?;
    let text=crate::work_host::verified_instruction(&scope.state_root,&scope.entity_id,&scope.thread_id,
        &instruction.ledger_ref,&instruction.sha256).map_err(str::to_string)?;
    let path=path.ok_or("The handoff has no assignment recording context")?;
    let recording=crate::assignment_tools::read_scope(path)?;
    if recording.state_root!=scope.state_root || recording.entity_id!=scope.entity_id || recording.thread_id!=scope.thread_id
        || recording.instruction_ledger_ref!=instruction.ledger_ref || recording.instruction_sha256!=instruction.sha256 {
        return Err("The assignment recording context does not match the original request".into())
    }
    // The row describes the user's request; the full authored handoff remains in this inbox.
    let facts=crate::assignment_tools::call_with(path,crate::assignment_tools::RECORD_TOOL_NAME,json!({"assignment":text}),opener)?;
    message.work=Some(WorkReference {
        assignment_id:facts["assignment_id"].as_str().ok_or("No assignment was recorded")?.into(),
        obligation_id:facts["obligation_id"].as_str().ok_or("No work obligation was recorded")?.into()});
    Ok(())
}

/// Upgrade a pending older handoff using the same inbox lock and existing register.
pub(crate) fn record_pending_handoff(scope:&StatusToolScope,message:&mut Message,path:&Path)->Result<(),String> {
    let _lock=locked_inbox(scope,"stu")?;
    record_handoff(scope,message,Some(path),&crate::assignment_tools::EcsObligations)
}

fn append_record(scope:&StatusToolScope,role:&str,message:&mut Message,unique:bool,
    recording:Option<(Option<&Path>,&dyn crate::assignment_tools::ObligationOpener)>)->Result<(u64,bool),String> {
    let mut file=locked_inbox(scope,role)?;
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
    // Retries and restarts report one failed logical input, using the inbox itself.
    // Scan acknowledged records too: Rich having read a failure cannot make it new.
    if let Some(input)=work_failure_input(message) {
        file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
        if let Some((_,through))=find_record(&mut BufReader::new(&mut file),|saved|Ok(work_failure_input(saved).as_ref()==Some(&input)))? {
            return Ok((through,true))
        }
    }
    if unique {
        file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
        if let Some((mut saved,through))=find_handoff(&mut BufReader::new(&mut file),message.instruction.as_ref().ok_or("Handoff has no request")?)? {
            if let Some((path,opener))=recording {record_handoff(scope,&mut saved,path,opener)?;}
            message.work=saved.work;
            file.sync_all().map_err(|e|e.to_string())?;return Ok((through,true))
        }
    }
    if let Some((path,opener))=recording {record_handoff(scope,message,path,opener)?;}
    let mut record=serde_json::to_vec(message).map_err(|e|e.to_string())?;record.push(b'\n');
    if record.len()>256*1024 {return Err("Encoded message exceeds the input frame limit".into())}
    file.write_all(&record).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
    Ok((file.metadata().map_err(|e|e.to_string())?.len(),false))
}

/// The cursor is acknowledged only after the receiving path durably accepts the record.
pub fn pending(scope: &StatusToolScope, role: &str) -> Result<Option<(Message,u64)>,String> {
    pending_after(scope,role,0)
}

/// Read an existing inbox record by its bounded frame offset, independent of history.
fn input_at(path:&Path,through:u64)->Result<Message,String> {
    let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
    if through==0 || through>file.metadata().map_err(|e|e.to_string())?.len(){return Err("Unfinished input no longer matches its durable inbox".into())}
    let start=through.saturating_sub(256*1024+1);file.seek(SeekFrom::Start(start)).map_err(|e|e.to_string())?;
    let mut bytes=Vec::new();file.take(through-start).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.last()!=Some(&b'\n'){return Err("Unfinished input is not a complete durable record".into())}
    let end=bytes.len()-1;let from=bytes[..end].iter().rposition(|b|*b==b'\n').map_or(0,|n|n+1);
    if from==0 && start>0{return Err("Unfinished input exceeds the existing frame limit".into())}
    serde_json::from_slice(&bytes[from..end]).map_err(|e|e.to_string())
}

/// The saved input at `through`: a handoff whose screen wait has ended (`work_host.rs`,
/// `held_for_screen`) is delivered from its own place in the inbox.
pub(crate) fn saved_input(scope:&StatusToolScope,role:&str,through:u64)->Result<Message,String> {
    input_at(&inbox(scope,role)?,through)
}

/// Only the matching Stu-authored terminal report or durable user Stop ends a handoff.
fn handoff_finished(scope:&StatusToolScope,message:&Message)->Result<bool,String> {
    if !message.handoff{return Ok(true)}
    let reference=match &message.work {Some(work)=>Some(work.clone()),None=>work_reference(scope,message)?};
    let Some(work)=reference else{return Ok(false)};
    let record=crate::assignment::read(&scope.state_root,&scope.entity_id,&scope.thread_id,&work.assignment_id).map_err(|e|e.to_string())?;
    if record.obligation_id!=work.obligation_id || message.instruction.as_ref().is_some_and(|i|record.instruction_ledger_ref!=i.ledger_ref || record.instruction_sha256!=i.sha256){return Err("The handoff's terminal report belongs to a different request".into())}
    Ok(record.user_stop_requested_at_ms.is_some() || matches!(record.reported_state(),Some(crate::assignment::AssignmentState::Settled|crate::assignment::AssignmentState::Failed)))
}

/// Older builds consumed handoffs when a native turn ended, even with work unfinished.
/// Repair their existing receipts once from the canonical inbox, never from a dispatcher.
fn retain_legacy_handoffs(scope:&StatusToolScope)->Result<(),String> {
    let path=inbox(scope,"stu")?;
    update_cursor(scope,"stu",|cursor| {
        if cursor["handoffs_retained"]==true {return Ok(())}
        let mut reader=BufReader::new(std::fs::File::open(&path).map_err(|e|e.to_string())?);
        let mut through=0;
        while let Some((message,length))=find_record(&mut reader,|_|Ok(true))? {
            through+=length;
            let accepted=through<=cursor["through"].as_u64().unwrap() || cursor["accepted"].get(through.to_string()).is_some();
            if accepted && message.handoff && !handoff_finished(scope,&message)? {
                let input=format!("peer:{through}");
                if cursor["active"].get(&input).is_none() {
                    cursor["active"][&input]=json!({"input":input,"session_id":null});
                }
            }
        }
        cursor["handoffs_retained"]=json!(true);
        Ok(())
    })
}

/// Read unfinished input in durable order, skipping only what this live connection took.
pub(crate) fn pending_after(scope:&StatusToolScope,role:&str,after:u64)->Result<Option<(Message,u64)>,String> {
    let path=inbox(scope,role)?;let mut cursor=cursor_state(&path)?;
    if role=="stu" && cursor["handoffs_retained"]!=true && path.is_file() {
        retain_legacy_handoffs(scope)?;
        cursor=cursor_state(&path)?;
    }
    let accepted=cursor["through"].as_u64().unwrap();let mut unfinished=None;let mut retired=false;
    for input in cursor["active"].as_object().unwrap().keys() {
        let Some(offset)=input.strip_prefix("peer:") else{continue};
        let offset=offset.parse::<u64>().map_err(|_|"Invalid unfinished input identity")?;
        if role=="stu" {
            let message=input_at(&path,offset)?;
            if message.handoff && handoff_finished(scope,&message)?{acknowledge(scope,role,offset)?;retired=true;continue}
        }
        if offset<=accepted && offset>after{unfinished=Some(unfinished.map_or(offset,|old:u64|old.min(offset)));}
    }
    if retired {cursor=cursor_state(&path)?;}
    let accepted=cursor["through"].as_u64().unwrap();
    if let Some(through)=unfinished {
        if accepted>std::fs::metadata(&path).map_err(|e|e.to_string())?.len(){return Err("Message input is shorter than its saved cursor".into())}
        return Ok(Some((input_at(&path,through)?,through)))
    }
    let mut file=match std::fs::File::open(&path){Ok(f)=>f,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    let mut through=accepted.max(after);
    if through>file.metadata().map_err(|e|e.to_string())?.len(){return Err("Message input is shorter than its saved cursor".into())}
    file.seek(SeekFrom::Start(through)).map_err(|e|e.to_string())?;let mut reader=BufReader::new(file);
    loop {
        let mut line=String::new();let count=(&mut reader).take(256*1024+1).read_line(&mut line).map_err(|e|e.to_string())?;
        if count>256*1024{return Err("Saved peer input exceeds its frame limit".into())}
        if count==0 || !line.ends_with('\n'){return Ok(None)}through+=count as u64;
        if cursor["accepted"].get(through.to_string()).is_some() && cursor["active"].get(format!("peer:{through}")).is_none(){continue}
        let message:Message=serde_json::from_str(&line).map_err(|e|e.to_string())?;
        if role=="stu" && message.handoff && handoff_finished(scope,&message)?{acknowledge(scope,role,through)?;continue}
        return Ok(Some((message,through)))
    }
}

pub fn acknowledge(scope:&StatusToolScope,role:&str,through:u64)->Result<(),String> {
    let finished=role!="stu" || handoff_finished(scope,&input_at(&inbox(scope,role)?,through)?)?;
    accept_input(scope,role,through,finished)
}

/// Acceptance between tool calls keeps the interrupted-turn recovery witness.
pub(crate) fn accept_live_handoff(scope:&StatusToolScope,role:&str,through:u64)->Result<(),String> {
    accept_input(scope,role,through,false)
}

fn accept_input(scope:&StatusToolScope,role:&str,through:u64,finished:bool)->Result<(),String> {
    let path=inbox(scope,role)?;
    update_cursor(scope,role,|cursor| {
        let input=format!("peer:{through}");
        if finished {cursor["active"].as_object_mut().unwrap().remove(&input);}
        else if cursor["active"].get(&input).is_none(){cursor["active"][&input]=json!({"input":input,"session_id":null});}
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
    let _lock=locked_inbox(scope,role)?;
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
    if let Some(offset)=input.strip_prefix("peer:"){return acknowledge(scope,role,offset.parse().map_err(|_|"Invalid peer input identity")?)}
    update_cursor(scope, role, |cursor| { cursor["active"].as_object_mut().unwrap().remove(input); Ok(()) })
}

/// Preserve the actual native session that may have acted before an interrupted delivery.
pub fn begin_attempt(scope:&StatusToolScope,role:&str,input:&str,session:&str)->Result<(),String> {
    update_cursor(scope, role, |cursor| { cursor["active"][input]=json!({"input":input,"session_id":session}); Ok(()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Opened(std::sync::atomic::AtomicUsize);
    impl crate::assignment_tools::ObligationOpener for Opened {
        fn open(&self,_:&crate::assignment_tools::ObligationDesk,_:&str,_:&str,_:crate::assignment::AssignmentKind)->Result<(),String> {
            self.0.fetch_add(1,std::sync::atomic::Ordering::SeqCst);Ok(())
        }
        fn abandon(&self,_:&crate::assignment_tools::ObligationDesk,_:&str) {panic!("registration unexpectedly failed")}
    }
    fn recording(path:&Path,scope:&StatusToolScope,instruction:&crate::ecs::UserInstruction) {
        crate::assignment_tools::write_scope(path,&crate::assignment_tools::AssignmentToolScope {
            version:1,actions_allowed:true,state_root:scope.state_root.clone(),entity_id:scope.entity_id.clone(),thread_id:scope.thread_id.clone(),
            instruction_ledger_ref:instruction.ledger_ref.clone(),instruction_sha256:instruction.sha256.clone(),
            obligation_desk:Some(crate::assignment_tools::ObligationDesk {
                bridge:crate::ecs::EcsBridge {python:PathBuf::from("unused"),component:PathBuf::from("unused"),state_root:scope.state_root.clone()},
                binding:crate::ecs::Binding {entity_id:scope.entity_id.clone(),thread_id:scope.thread_id.clone(),session_id:"native-rich".into(),turn_id:"original".into(),audience:"rich".into(),revision:1},seat:None})}).unwrap();
    }
    #[test]
    fn upgrade_recovers_a_consumed_unfinished_handoff_without_repeating_finished_work() {
        let root=std::env::temp_dir().join(format!("richos-upgrade-inbox-{}",uuid::Uuid::new_v4()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"thread".into()};
        let mut offsets=Vec::new();let mut jobs=Vec::new();
        for index in 1..=2 {
            let instruction=crate::ecs::UserInstruction {ledger_ref:format!("ledger:thread:turn-{index}"),sha256:"0".repeat(64),text:None};
            let record=crate::assignment::register(&root,&crate::assignment::Registration {
                entity_id:"company".into(),thread_id:"thread".into(),obligation_id:format!("job-{index}"),
                instruction_ledger_ref:instruction.ledger_ref.clone(),instruction_sha256:instruction.sha256.clone(),
                title:format!("Original job {index}"),repositories:vec![],needs_screen:false,
            }).unwrap();
            append(&scope,"stu",&Message {handoff:true,from:"rich".into(),text:format!("Original job {index}"),instruction:Some(instruction),
                work:Some(WorkReference {assignment_id:record.id.clone(),obligation_id:format!("job-{index}")})}).unwrap();
            offsets.push(std::fs::metadata(inbox(&scope,"stu").unwrap()).unwrap().len());jobs.push(record.id);
        }
        crate::assignment::advance(&root,"company","thread",&jobs[0],crate::assignment::AssignmentState::Settled,"Verified completion before update").unwrap();
        crate::assignment::advance(&root,"company","thread",&jobs[1],crate::assignment::AssignmentState::Running,"Worker active before update").unwrap();
        append(&scope,"stu",&Message {handoff:false,from:"rich".into(),text:"Already consumed status request".into(),instruction:None,work:None}).unwrap();
        let path=inbox(&scope,"stu").unwrap();let through=std::fs::metadata(&path).unwrap().len();
        // Nightly 61 consumed the handoff at native end_turn and removed its active receipt.
        for legacy in [json!(through),json!({"through":through}),json!({"through":through,"active":{},"accepted":{}})] {
            crate::doctrine::write_verified(&path.with_extension("cursor"),&legacy.to_string()).unwrap();
            let (message,resumed)=pending(&scope,"stu").unwrap().expect("upgrade lost the unfinished canonical handoff");
            assert_eq!(resumed,offsets[1]);assert_eq!(message.work.unwrap().assignment_id,jobs[1]);
            acknowledge(&scope,"stu",resumed).unwrap();
            assert!(pending_after(&scope,"stu",resumed).unwrap().is_none(),"live Stu received completed work or an accepted correction twice");
            assert_eq!(pending(&scope,"stu").unwrap().unwrap().1,resumed,"another restart lost the recovery receipt");
            assert_eq!(crate::assignment::read_all(&root,"company","thread").unwrap().len(),2,"recovery registered duplicate work");
        }
        crate::assignment::advance(&root,"company","thread",&jobs[1],crate::assignment::AssignmentState::Settled,"Stu completed the resumed job").unwrap();
        assert!(pending(&scope,"stu").unwrap().is_none(),"completed jobs restarted again");
        std::fs::remove_dir_all(root).unwrap();
    }

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
        let path=root.join("status.json");let assignments=root.join("assignments.json");
        recording(&assignments,&scope,&instruction);
        bind_with_assignments(&path,&scope,"native-rich","rich",Some(instruction.clone()),Some(&assignments)).unwrap();
        let opened=Opened::default();
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(3));
        let results=std::thread::scope(|threads| {
            let run=|text| {let b=barrier.clone();let path=&path;let scope=&scope;let opened=&opened;threads.spawn(move || {b.wait();delegate_with(path,scope,json!({"text":text}),opened).unwrap()})};
            let hook=run("Original request and recorded context");let manual=run("Rich's handoff with his findings");barrier.wait();
            [hook.join().unwrap(),manual.join().unwrap()]
        });
        assert_eq!(results[0]["through"],results[1]["through"]);
        assert_ne!(results[0]["already_saved"],results[1]["already_saved"]);
        assert_eq!(opened.0.load(std::sync::atomic::Ordering::SeqCst),1,"concurrent routes opened duplicate obligations");
        let records=crate::assignment::read_all(&scope.state_root,&scope.entity_id,&scope.thread_id).unwrap();
        assert_eq!(records.len(),1,"job and Stop must exist before any Stu prompt");
        assert_eq!(results[0]["work"]["assignment_id"],records[0].id);
        let (message,through)=pending(&scope,"stu").unwrap().unwrap();assert!(message.handoff);
        assert_eq!(message.instruction.as_ref().unwrap().text.as_deref(),Some(original));
        crate::assignment::record_user_stop(&scope.state_root,&scope.entity_id,&scope.thread_id,&records[0].id).unwrap();
        begin_attempt(&scope,"stu",&format!("peer:{through}"),"stu-before-crash").unwrap();
        // A fresh Rich session after a crash retries the same canonical request.
        bind(&path,&scope,"rich-after-crash","rich",Some(instruction.clone())).unwrap();
        assert_eq!(delegate(&path,&scope,json!({"text":"Retry after uncertain HTTP result"})).unwrap()["through"],through);
        assert_eq!(crate::assignment::read_all(&scope.state_root,&scope.entity_id,&scope.thread_id).unwrap().len(),1);
        assert!(crate::assignment::read(&scope.state_root,&scope.entity_id,&scope.thread_id,&records[0].id).unwrap().user_stop_requested_at_ms.is_some(),"retry erased Stop");
        assert_eq!(previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap().unwrap()["session_id"],"stu-before-crash");
        acknowledge(&scope,"stu",through).unwrap();
        assert!(delegate(&path,&scope,json!({"text":"Retry after Stu consumed it"})).unwrap()["already_saved"].as_bool().unwrap());
        assert!(pending(&scope,"stu").unwrap().is_none());
        send(&path,&scope,json!({"text":"A correction is still ordinary independent communication"})).unwrap();
        let (correction,offset)=pending(&scope,"stu").unwrap().unwrap();assert!(!correction.handoff);acknowledge(&scope,"stu",offset).unwrap();
        // Identical words in a NEW user turn remain a distinct request.
        let next=ledger.record_prompt_received(&binding,original,Source::Text).unwrap();
        let next_instruction=crate::ecs::UserInstruction::with_text(format!("ledger:{}:{next}",scope.thread_id),instruction.sha256.clone(),Some(original));
        recording(&assignments,&scope,&next_instruction);
        bind_with_assignments(&path,&scope,"rich-after-crash","rich",Some(next_instruction),Some(&assignments)).unwrap();
        let next=delegate_with(&path,&scope,json!({"text":"The user asked again"}),&opened).unwrap();assert_ne!(next["through"],through);
        assert_eq!(opened.0.load(std::sync::atomic::Ordering::SeqCst),2);
        bind(&path,&scope,"native-stu","stu",Some(instruction.clone())).unwrap();assert!(delegate(&path,&scope,json!({"text":"wrong role"})).is_err());
        let mut forged=instruction;forged.sha256="0".repeat(64);bind(&path,&scope,"native-rich","rich",Some(forged)).unwrap();
        assert!(delegate(&path,&scope,json!({"text":"forged request"})).is_err());
        drop(ledger);std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepted_live_work_recovers_in_order_until_its_actual_native_turn_finishes() {
        let root=std::env::temp_dir().join(format!("richos-live-handoff-{}",uuid::Uuid::new_v4()));
        let scope=StatusToolScope {version:1,state_root:root.clone(),entity_id:"company".into(),thread_id:"thread".into()};
        let path=root.join("scope.json");bind(&path,&scope,"native-rich","rich",None).unwrap();
        send(&path,&scope,json!({"text":"A"})).unwrap();let a=pending(&scope,"stu").unwrap().unwrap().1;
        begin_attempt(&scope,"stu",&format!("peer:{a}"),"before-crash").unwrap();
        send(&path,&scope,json!({"text":"B"})).unwrap();let b=pending_after(&scope,"stu",a).unwrap().unwrap().1;
        begin_attempt(&scope,"stu",&format!("peer:{b}"),"before-crash").unwrap();
        accept_live_handoff(&scope,"stu",b).unwrap();
        send(&path,&scope,json!({"text":"correction"})).unwrap();let correction=pending_after(&scope,"stu",b).unwrap().unwrap().1;
        acknowledge(&scope,"stu",correction).unwrap();
        assert_eq!(pending(&scope,"stu").unwrap().unwrap().1,a);
        acknowledge(&scope,"stu",a).unwrap();
        acknowledge(&scope,"stu",correction).unwrap();
        assert_eq!(pending(&scope,"stu").unwrap().unwrap().1,b,"a different session cannot erase unfinished B");
        assert_eq!(previous_attempt(&scope,"stu",&format!("peer:{b}")).unwrap().unwrap()["session_id"],"before-crash");
        acknowledge(&scope,"stu",b).unwrap();
        assert!(pending(&scope,"stu").unwrap().is_none());
        send(&path,&scope,json!({"text":"C"})).unwrap();let c=pending(&scope,"stu").unwrap().unwrap().1;
        begin_attempt(&scope,"stu",&format!("peer:{c}"),"completed-session").unwrap();accept_live_handoff(&scope,"stu",c).unwrap();
        acknowledge(&scope,"stu",c).unwrap();
        assert!(pending(&scope,"stu").unwrap().is_none(),"completed live input replayed");
        std::fs::remove_dir_all(root).unwrap();
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
