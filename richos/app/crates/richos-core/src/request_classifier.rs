//! A tool-free Haiku process owned by one conversation. It never dispatches work.
use crate::{engine_profile::EngineProfile, owned_process::OwnedChild};
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Read, Write}, path::{Path, PathBuf}, process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender}, thread::JoinHandle, time::{Duration, Instant}};

// Real-provider samples reached 4.2 s. Bound the classification wait rather than
// making the user wait indefinitely. Failure preserves Rich's ordinary judgment.
const WAIT: Duration = Duration::from_secs(5);
const MAX_BYTES: usize = 64 * 1024;

struct Live {
    child: OwnedChild,
    input: Option<Sender<String>>,
    frames: Receiver<Result<Value,String>>,
    readers: Vec<JoinHandle<()>>,
}
impl Drop for Live {
    fn drop(&mut self) {
        drop(self.input.take());
        if let Err(error)=self.child.kill() {eprintln!("[richos] classifier stop failed: {error}");}
        if let Err(error)=self.child.wait() {eprintln!("[richos] classifier cleanup failed: {error}");}
        for reader in self.readers.drain(..) {
            if reader.join().is_err() {eprintln!("[richos] classifier reader ended unexpectedly");}
        }
    }
}

pub struct RequestClassifier {
    bin: PathBuf,
    profile: EngineProfile,
    skills: PathBuf,
    live: Option<Live>,
}

impl RequestClassifier {
    pub fn new(bin:&Path, profile:&EngineProfile, skills:&Path)->Self {
        Self {bin:bin.to_path_buf(),profile:profile.clone(),skills:skills.to_path_buf(),live:None}
    }

    fn start(&self)->Result<Live,String> {
        let standing=std::fs::read_to_string(self.profile.engine.join("app/routing.md")).map_err(|e|e.to_string())?;
        let folder=self.profile.state.join("classification");
        std::fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
        let id=uuid::Uuid::new_v4().to_string();
        let (state,log)=crate::lease_commands::prepare(&self.profile.state,&id).map_err(|e|e.to_string())?;
        let mut command=Command::new(&self.profile.runtime.python);
        command.args(crate::lease_commands::supervisor_args(&self.profile.engine,&state,&log)).arg(&self.bin);
        self.profile.configure(&mut command,&self.bin,&id,&folder.join("no-action-scope.json"));
        command.env_remove("CLAUDECODE");
        command.args(["-p","--model","claude-haiku-5-5","--effort","low",
            "--input-format","stream-json","--output-format","stream-json","--verbose",
            "--system-prompt",&standing,"--tools","","--strict-mcp-config","--mcp-config",
            "{\"mcpServers\":{}}","--setting-sources","","--no-session-persistence",
            "--settings","{\"disableAllHooks\":true,\"autoMemoryEnabled\":false,\"claudeMdExcludes\":[\"**/CLAUDE.md\",\"**/CLAUDE.local.md\",\"**/.claude/rules/**\"]}",
            "--permission-mode","bypassPermissions"])
            .current_dir(folder).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        OwnedChild::configure(&mut command);
        let mut child=command.spawn().map_err(|e|e.to_string())?;
        let mut stdin=child.stdin.take().ok_or("Classifier input unavailable")?;
        let stdout=child.stdout.take().ok_or("Classifier output unavailable")?;
        let child=OwnedChild::supervised(child);
        let (input,requests)=mpsc::channel::<String>();
        // A provider that stops reading cannot block the conversation's write.
        let writer=std::thread::spawn(move || {for request in requests {
            if stdin.write_all(request.as_bytes()).and_then(|_|stdin.write_all(b"\n")).and_then(|_|stdin.flush()).is_err() {break}
        }});
        let (send,frames)=mpsc::channel();
        let sink=self.profile.claude_account.as_ref().and_then(|account|Some((account.id.clone(),self.profile.token_sink.clone()?)));
        let reader=std::thread::spawn(move || {
            let mut reader=BufReader::new(stdout);let mut tally=crate::quota::tokens::Tally::default();
            loop {
                let mut line=String::new();
                let frame=match reader.by_ref().take(MAX_BYTES as u64+1).read_line(&mut line) {
                    Ok(0)=>break,
                    Ok(n) if n>MAX_BYTES || !line.ends_with('\n')=>Err("Invalid classifier frame".into()),
                    Ok(_)=>serde_json::from_str::<Value>(&line).map_err(|e|e.to_string()),
                    Err(e)=>Err(e.to_string()),
                };
                if let (Ok(frame),Some((account,sink)))=(&frame,&sink) {
                    for found in tally.frame(frame,crate::util::now_millis()) {sink.push(account,found);}
                }
                let failed=frame.is_err();
                if send.send(frame).is_err() || failed {break}
            }
        });
        Ok(Live {child,input:Some(input),frames,readers:vec![writer,reader]})
    }

    pub fn classify(&mut self, original:&str, context:&str, canceled:impl Fn()->bool)->Result<String,String> {
        let result=self.classify_inner(original,context,canceled);
        // Never let a late response to an expired request classify the next input.
        if result.is_err() {self.live=None;}
        result
    }

    fn classify_inner(&mut self, original:&str, context:&str, canceled:impl Fn()->bool)->Result<String,String> {
        let roles=|name|std::fs::read_to_string(self.skills.join(format!("agents/{name}.md"))).map_err(|e|e.to_string());
        let request=json!({"type":"user","message":{"role":"user","content":json!({"user_request":original,
            "recent_conversation":context,"roles":{"rich":roles("rich")?,"stu":roles("stu")?}}).to_string()}}).to_string();
        if request.len()>MAX_BYTES {return Err("Request exceeds classifier context budget".into())}
        if canceled() {return Err("Classification canceled".into())}
        if self.live.is_none() {self.live=Some(self.start()?);}
        let live=self.live.as_ref().ok_or("Classifier unavailable")?;
        live.input.as_ref().ok_or("Classifier closed")?.send(request).map_err(|e|e.to_string())?;
        let deadline=Instant::now()+WAIT;let mut model_seen=false;
        loop {
            if canceled() {return Err("Classification canceled".into())}
            let remaining=deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {return Err("Classification deadline".into())}
            let frame=match live.frames.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(frame)=>frame?,Err(mpsc::RecvTimeoutError::Timeout)=>continue,
                Err(_)=>return Err("Classifier disconnected".into()),
            };
            if frame["type"]=="system" && frame["subtype"]=="init" && frame["tools"].as_array().is_some_and(|tools|!tools.is_empty()) {
                return Err("Classifier unexpectedly has tools".into())
            }
            if frame["type"]=="assistant" {
                if frame["message"]["model"]!="claude-haiku-5-5" || frame["message"]["content"].as_array()
                    .is_some_and(|blocks|blocks.iter().any(|b|b["type"]=="tool_use")) {return Err("Invalid classifier model or action".into())}
                model_seen=true;
            }
            if frame["type"]=="result" {
                if !model_seen || frame["is_error"]!=false {return Err("Classifier failed".into())}
                return choice(&frame);
            }
        }
    }
}

fn choice(frame:&Value)->Result<String,String> {
    match frame["result"].as_str().map(str::trim) {
        Some("rich_handles_message")=>Ok("rich".into()),
        Some("new_stu_handoff")=>Ok("stu".into()),
        Some("uncertain")=>Ok("uncertain".into()),
        _=>Err("Invalid classifier decision".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decisions_name_the_next_action_and_reject_ambiguous_recipient_names() {
        for (label,handler) in [("rich_handles_message","rich"),("new_stu_handoff","stu"),("uncertain","uncertain")] {
            assert_eq!(choice(&json!({"result":label})).unwrap(),handler);
        }
        for result in [json!("stu"),json!("rich"),json!(null),json!("new_stu_handoff then rich_handles_message")] {
            assert!(choice(&json!({"result":result})).is_err());
        }
    }
}
