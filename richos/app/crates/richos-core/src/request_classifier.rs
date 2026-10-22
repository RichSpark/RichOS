//! A tool-free Haiku process owned by one conversation. It never dispatches work.
use crate::{engine_profile::EngineProfile, owned_process::OwnedChild};
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Read, Write}, path::{Path, PathBuf}, process::{Command, Stdio},
    sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, Sender, SyncSender}}, thread::JoinHandle, time::{Duration, Instant}};

// A prepared classifier gets a short inference budget. Cold startup and process
// cleanup never run on the visible request path. Failure leaves judgment to Rich.
const WAIT: Duration = Duration::from_secs(5);
const WARMUP: Duration = Duration::from_secs(30);
const MAX_BYTES: usize = 64 * 1024;
const HAIKU: &str = "claude-haiku-5-5";

pub(crate) struct Live {
    child: OwnedChild,
    input: Option<Sender<String>>,
    frames: Receiver<Result<Value,String>>,
    readers: Vec<JoinHandle<()>>,
    /// The model every answer must come from, when the launch named an exact one.
    model: Option<String>,
    /// Whether this launch may use tools (only [`Provider::writing`] gives it any).
    tools: bool,
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

struct Request {
    body: String,
    deadline: Instant,
    canceled: Arc<AtomicBool>,
    reply: Sender<Result<String,String>>,
}

pub struct RequestClassifier {
    skills: PathBuf,
    requests: Option<SyncSender<Request>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for RequestClassifier {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        drop(self.requests.take());
        if let Some(worker)=self.worker.take() {drop(worker.join());}
    }
}

/// A tool-free Haiku launch: the instruction file it is given (relative to the engine) and
/// the private folder it runs in. The classifier and the conversation titler
/// (`thread_title.rs`) start the same way and differ only in these two. The update of the
/// user's own instructions (`instructions_update.rs`) starts the same launch on another model
/// with the Write tool ([`Provider::writing`]).
pub(crate) struct Provider {
    bin: PathBuf,
    profile: EngineProfile,
    instructions: &'static str,
    folder: &'static str,
    model: String,
    effort: &'static str,
    tools: &'static str,
}

impl RequestClassifier {
    pub fn new(bin:&Path, profile:&EngineProfile, skills:&Path)->Self {
        // No waiting queue: a request can enter only while the warm owner is
        // receiving. Startup, failure cleanup and recovery leave Rich unblocked.
        let (send, requests)=mpsc::sync_channel::<Request>(0);
        let stopped=Arc::new(AtomicBool::new(false));
        let stop=stopped.clone();
        let provider=Provider::new(bin,profile,"app/routing.md","classification");
        let worker=std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let run=||->Result<(),String> {
                    let mut live=provider.start()?;
                    live.exchange(json!({"type":"user","message":{"role":"user","content":"{\"warmup\":true}"}}).to_string(),
                        Instant::now()+WARMUP,||stop.load(Ordering::Acquire),choice)?;
                    eprintln!("[richos] classifier warm and ready");
                    while !stop.load(Ordering::Acquire) {
                        let request=match requests.recv() {
                            Ok(request)=>request,
                            Err(_)=>return Ok(()),
                        };
                        let answer=live.exchange(request.body,request.deadline,
                            ||stop.load(Ordering::Acquire) || request.canceled.load(Ordering::Acquire),choice);
                        let failed=answer.is_err();
                        drop(request.reply.send(answer));
                        if failed {return Err("Classifier must prepare again".into())}
                    }
                    Ok(())
                };
                if let Err(error)=run() {
                    eprintln!("[richos] classifier unavailable: {error}");
                } else {break}
                if stop.load(Ordering::Acquire) {break}
                // Idle recovery, not a cold start charged to the next message.
                match requests.recv_timeout(Duration::from_secs(5)) {
                    Ok(request)=>{drop(request.reply.send(Err("Classifier preparing".into())));}
                    Err(mpsc::RecvTimeoutError::Disconnected)=>break,
                    Err(mpsc::RecvTimeoutError::Timeout)=>{}
                }
            }
        });
        Self {skills:skills.to_path_buf(),requests:Some(send),stopped,worker:Some(worker)}
    }

    pub fn classify(&mut self, original:&str, context:&str, canceled:impl Fn()->bool)->Result<String,String> {
        if canceled() {return Err("Classification canceled".into())}
        let roles=|name|std::fs::read_to_string(self.skills.join(format!("agents/{name}.md"))).map_err(|e|e.to_string());
        let body=json!({"type":"user","message":{"role":"user","content":json!({"user_request":original,
            "recent_conversation":context,"roles":{"rich":roles("rich")?,"stu":roles("stu")?}}).to_string()}}).to_string();
        if body.len()>MAX_BYTES {return Err("Request exceeds classifier context budget".into())}
        let (reply,result)=mpsc::channel();
        let cancel=Arc::new(AtomicBool::new(false));
        let deadline=Instant::now()+WAIT;
        self.requests.as_ref().ok_or("Classifier closed")?.try_send(Request {
            body,deadline,canceled:cancel.clone(),reply,
        }).map_err(|_|"Classifier not ready".to_string())?;
        loop {
            if canceled() || Instant::now()>=deadline {
                cancel.store(true,Ordering::Release);
                return Err("Classification canceled or slow".into())
            }
            match result.recv_timeout(deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(50))) {
                Ok(answer)=>return answer,
                Err(mpsc::RecvTimeoutError::Timeout)=>continue,
                Err(_)=>return Err("Classifier disconnected".into()),
            }
        }
    }
}

impl Provider {
    pub(crate) fn new(bin:&Path, profile:&EngineProfile, instructions:&'static str, folder:&'static str)->Self {
        Provider {bin:bin.to_path_buf(),profile:profile.clone(),instructions,folder,
            model:HAIKU.into(),effort:"low",tools:""}
    }

    /// The same launch on `model` (a CLI alias) at high effort, with only the `tools` named.
    pub(crate) fn writing(mut self, model:&str, tools:&'static str)->Self {
        self.model=model.into();
        self.effort="high";
        self.tools=tools;
        self
    }

    pub(crate) fn start(&self)->Result<Live,String> {
        let standing=std::fs::read_to_string(self.profile.engine.join(self.instructions)).map_err(|e|e.to_string())?;
        let folder=self.profile.state.join(self.folder);
        std::fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
        let id=uuid::Uuid::new_v4().to_string();
        let (state,log)=crate::lease_commands::prepare(&self.profile.state,&id).map_err(|e|e.to_string())?;
        let mut command=Command::new(&self.profile.runtime.python);
        command.args(crate::lease_commands::supervisor_args(&self.profile.engine,&state,&log)).arg(&self.bin);
        self.profile.configure(&mut command,&self.bin,&id,&folder.join("no-action-scope.json"));
        command.env_remove("CLAUDECODE");
        command.args(["-p","--model",&self.model,"--effort",self.effort,
            "--input-format","stream-json","--output-format","stream-json","--verbose",
            "--system-prompt",&standing,"--tools",self.tools,"--strict-mcp-config","--mcp-config",
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
        Ok(Live {child,input:Some(input),frames,readers:vec![writer,reader],
            model:(self.model==HAIKU).then(||self.model.clone()),tools:!self.tools.is_empty()})
    }

}

impl Live {
    /// One request, one answer. `answer` reads the provider's final `result` frame; another
    /// model, a tool call or an error frame is refused before it is read.
    pub(crate) fn exchange(&mut self, request:String, deadline:Instant, canceled:impl Fn()->bool,
        answer:fn(&Value)->Result<String,String>)->Result<String,String> {
        if canceled() || Instant::now()>=deadline {return Err("Classification canceled or slow".into())}
        self.input.as_ref().ok_or("Classifier closed")?.send(request).map_err(|e|e.to_string())?;
        let mut model_seen=false;
        loop {
            if canceled() {return Err("Classification canceled".into())}
            let remaining=deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {return Err("Classification deadline".into())}
            let frame=match self.frames.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(frame)=>frame?,Err(mpsc::RecvTimeoutError::Timeout)=>continue,
                Err(_)=>return Err("Classifier disconnected".into()),
            };
            if !self.tools && frame["type"]=="system" && frame["subtype"]=="init" && frame["tools"].as_array().is_some_and(|tools|!tools.is_empty()) {
                return Err("Classifier unexpectedly has tools".into())
            }
            if frame["type"]=="assistant" {
                if self.model.as_ref().is_some_and(|model|frame["message"]["model"]!=model.as_str()) || !self.tools && frame["message"]["content"].as_array()
                    .is_some_and(|blocks|blocks.iter().any(|b|b["type"]=="tool_use")) {return Err("Invalid classifier model or action".into())}
                model_seen=true;
            }
            if frame["type"]=="result" {
                if !model_seen || frame["is_error"]!=false {return Err("Classifier failed".into())}
                return answer(&frame);
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
pub(crate) mod tests {
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
    /// A stub `claude` (the Python `body`) over the real engine's instruction files; the
    /// conversation titler's tests start their provider through it too.
    pub(crate) fn fixture(body:&str)->(PathBuf,PathBuf,EngineProfile,PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root=std::env::temp_dir().join(format!("richos-classifier-{}",uuid::Uuid::new_v4()));
        let skills=root.join("skills");
        std::fs::create_dir_all(skills.join("agents")).unwrap();
        for role in ["rich","stu"] {std::fs::write(skills.join(format!("agents/{role}.md")),role).unwrap();}
        let bin=root.join("provider");
        std::fs::write(&bin,format!("#!/usr/bin/python3\nimport json,os,sys,time\n{body}\n")).unwrap();
        std::fs::set_permissions(&bin,std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap();
        let profile=EngineProfile {
            engine,coordination:root.join("coordination"),plugin:root.join("plugin"),state:root.join("engine-state"),
            user:Default::default(),
            runtime:crate::runtime::EngineRuntime {root:PathBuf::from("/usr"),python:PathBuf::from("/usr/bin/python3"),
                node:PathBuf::from("/usr/bin/false"),git:PathBuf::from("/usr/bin/git"),versions:Default::default()},
            work_scope:None,
            operator_desk:None,claude_account:None,token_sink:None,speech_model:None,
            lease_models:Default::default(),machine_settings:Default::default(),
        };
        (root,bin,profile,skills)
    }

    #[test]
    fn cold_classifier_falls_back_without_polling_or_sending_the_visible_request() {
        let (root,bin,profile,skills)=fixture("for line in sys.stdin:\n    with open('received.jsonl','a') as f: f.write(line)\n    time.sleep(60) # Stalled provider; owned teardown, not timing, decides cleanup.");
        let mut classifier=RequestClassifier::new(&bin,&profile,&skills);
        let calls=std::sync::atomic::AtomicUsize::new(0);
        let answer=classifier.classify("Visible request", "context",|| {
            calls.fetch_add(1,std::sync::atomic::Ordering::Relaxed);false
        });
        assert_eq!(answer.unwrap_err(),"Classifier not ready");
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed),1,
            "Cold preparation polled on the user's request path");
        drop(classifier);
        let received=std::fs::read_to_string(profile.state.join("classification/received.jsonl")).unwrap_or_default();
        assert!(!received.contains("Visible request"), "Cold user input was sent to the provider");
        std::fs::remove_dir_all(root).unwrap();
    }

    // The bound prevents a broken fixture hanging the suite. Correctness comes
    // from the written startup/input facts, never from how fast this Mac is.
    fn until<T>(mut fact:impl FnMut()->Option<T>)->T {
        let guard=Instant::now()+Duration::from_secs(60);
        loop {
            if let Some(value)=fact() {return value}
            assert!(Instant::now()<guard,"Classifier fixture never reached its expected fact");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn classifier_warms_before_use_and_reuses_one_tool_free_process() {
        let (root,bin,profile,skills)=fixture(r#"
assert '--model' in sys.argv and 'claude-haiku-5-5' in sys.argv
assert sys.argv[sys.argv.index('--tools')+1] == ''
print(json.dumps({'type':'system','subtype':'init','tools':[]}),flush=True)
for line in sys.stdin:
    request=json.loads(line)
    data=json.loads(request['message']['content'])
    with open('seen.jsonl','a') as f: f.write(json.dumps({'pid':os.getpid(),'data':data})+'\n')
    result='new_stu_handoff' if data.get('user_request')=='Substantial work' else 'rich_handles_message'
    print(json.dumps({'type':'assistant','message':{'model':'claude-haiku-5-5','content':[{'type':'text','text':result}]}}),flush=True)
    print(json.dumps({'type':'result','is_error':False,'result':result}),flush=True)
"#);
        let mut classifier=RequestClassifier::new(&bin,&profile,&skills);
        let seen=profile.state.join("classification/seen.jsonl");
        until(||std::fs::read_to_string(&seen).ok().filter(|text|text.contains("warmup")));
        let mut route=|text:&str|until(|| match classifier.classify(text,"recent conversation",||false) {
            Ok(answer)=>Some(answer),
            Err(error) if error=="Classifier not ready"=>None,
            Err(error)=>panic!("{error}"),
        });
        assert_eq!(route("Quick question"),"rich");
        assert_eq!(route("Substantial work"),"stu");
        let rows:Vec<Value>=std::fs::read_to_string(seen).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();
        assert_eq!(rows.len(),3);
        assert_eq!(rows[0]["data"]["warmup"],true);
        assert!(rows.iter().all(|row|row["pid"]==rows[0]["pid"]));
        assert_eq!(rows[1]["data"]["recent_conversation"],"recent conversation");
        assert_eq!(rows[2]["data"]["user_request"],"Substantial work");
        drop(classifier);
        std::fs::remove_dir_all(root).unwrap();
    }


    #[test]
    fn ready_classifier_waits_for_the_model_instead_of_expiring_the_request() {
        let (root,bin,profile,skills)=fixture(r#"
print(json.dumps({'type':'system','subtype':'init','tools':[]}),flush=True)
for line in sys.stdin:
    data=json.loads(json.loads(line)['message']['content'])
    if data.get('user_request'):
        while not os.path.exists('release-answer'):
            time.sleep(0.005) # Controlled reply barrier; the owner stops the fixture on failure.
    result='new_stu_handoff'
    print(json.dumps({'type':'assistant','message':{'model':'claude-haiku-5-5','content':[{'type':'text','text':result}]}}),flush=True)
    print(json.dumps({'type':'result','is_error':False,'result':result}),flush=True)
"#);
        let mut classifier=RequestClassifier::new(&bin,&profile,&skills);
        let release=profile.state.join("classification/release-answer");
        // Release only after forty cancellation polls while the model is still
        // answering. The verdict is receipt of its answer, not an elapsed-time
        // assertion. `until` supplies the existing fixture hang guard.
        let answer=until(|| {
            let polls=std::cell::Cell::new(0);
            match classifier.classify("Substantial work","context",|| {
                let count=polls.get()+1;polls.set(count);
                if count==40 {std::fs::write(&release,"ready").unwrap();}
                false
            }) {
                Ok(answer)=>Some(answer),
                Err(error) if error=="Classifier not ready"=>None,
                Err(error)=>panic!("{error}"),
            }
        });
        assert_eq!(answer,"stu");
        drop(classifier);
        std::fs::remove_dir_all(root).unwrap();
    }

}
