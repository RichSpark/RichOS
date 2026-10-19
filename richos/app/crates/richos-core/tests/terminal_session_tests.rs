use richos_core::{
    cognition::{Cognition, CognitionError, MockLeaseFactory, TurnItem},
    entity::{Entity, EntityId, EntityRegistry},
    ledger::{Ledger, Source},
    spine::Spine,
};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

fn fixture() -> (std::path::PathBuf, Spine, EntityId) {
    let root = std::env::temp_dir().join(format!("native-sessions-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = Spine::new(Ledger::open(root.join("conversation.jsonl")).unwrap());
    spine.set_entity_registry(EntityRegistry::new(vec![Entity::new("example", "Example", &["/fixture/example"]).unwrap()]).unwrap());
    (root, spine, EntityId::parse("example").unwrap())
}

#[test]
fn opening_a_conversation_sends_no_hidden_model_turn() {
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let factory = MockLeaseFactory::new(vec!["Reply"]);
    let priming = factory.spawned.clone();
    let prompts = factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.prime_front_desk(&thread);
    assert_eq!(priming.lock().unwrap().len(), 1, "the native handshake still starts ahead of typing");
    assert!(priming.lock().unwrap().iter().all(|log| log.lock().unwrap().is_empty()), "opening spent a hidden model turn");
    assert!(prompts.lock().unwrap().iter().all(|log| log.lock().unwrap().is_empty()));
    spine.submit_prompt("Question", Source::Text).unwrap();
    assert_eq!(*prompts.lock().unwrap()[0].lock().unwrap(), ["Question"]);
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn context_growth_is_owned_by_the_existing_native_session() {
    let (root, mut spine, company) = fixture();
    spine.create_thread("Conversation", &company).unwrap();
    let factory = MockLeaseFactory::new(vec!["First", "Second"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.submit_prompt(&"x".repeat(600_000), Source::Text).unwrap();
    spine.submit_prompt("Continue", Source::Text).unwrap();
    assert_eq!(spawned.lock().unwrap().len(), 1, "the app replaced the native session instead of allowing its own compaction");
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

struct Disconnected {
    prompts: Arc<AtomicUsize>,
}
impl Cognition for Disconnected {
    fn session_id(&self) -> &str { "disconnected-session" }

    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        Err(CognitionError::Io("fixture disconnected".into()))
    }
}

#[test]
fn a_disconnected_turn_does_not_spawn_and_replay_the_user_request() {
    let (root, mut spine, company) = fixture();
    spine.create_thread("Conversation", &company).unwrap();
    let prompts = Arc::new(AtomicUsize::new(0));
    let factory = MockLeaseFactory::new(vec!["An unsolicited replay"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.attach_lease(Box::new(Disconnected { prompts: prompts.clone() }));
    assert!(spine.submit_prompt("Do the job", Source::Text).is_err());
    assert_eq!(prompts.load(Ordering::SeqCst), 1);
    assert!(spawned.lock().unwrap().is_empty(), "the app spawned a replacement and replayed the job");
    drop(spine);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn a_second_native_input_reaches_the_same_session_before_the_first_reply() {
    use richos_core::{native::NativeCognition, spine::drive_shared_turns};
    use std::{os::unix::fs::PermissionsExt, sync::Mutex};
    let (root, mut spine, company) = fixture();
    let thread=spine.create_thread("Conversation", &company).unwrap();
    let script=root.join("provider.py");
    std::fs::write(&script,r#"#!/usr/bin/env python3
import json,sys,select,os
buffer=b""

def send(value):
 print(json.dumps(value),flush=True)
def read():
 global buffer
 while b"\n" not in buffer:
  ready,_,_=select.select([sys.stdin],[],[],10) # Hang guard: progress requires the second SDK input, not elapsed time.
  if not ready: raise RuntimeError('second input did not reach the live session before its first reply')
  chunk=os.read(sys.stdin.fileno(),4096)
  if not chunk: raise EOFError('app closed stdin')
  buffer+=chunk
 line,buffer=buffer.split(b"\n",1)
 return json.loads(line)
init=read()
send({'type':'control_response','response':{'subtype':'success','request_id':init['request_id'],'response':{}}})
first=read()
second=read()
for index,prompt in enumerate([first,second]):
 command=prompt['uuid']
 send({'type':'command_lifecycle','state':'started','command_uuid':command})
 send({'type':'control_request','request_id':command,'request':{'subtype':'hook_callback','callback_id':'richos-input-context','input':{'hook_event_name':'UserPromptSubmit'}}})
 assert read()['response']['subtype']=='success'
 send({'type':'assistant','message':{'role':'assistant','content':[{'type':'text','text':'Answer '+str(index+1)}]}})
 send({'type':'result','is_error':False,'stop_reason':'end_turn','command_uuid':command,'queued_turn_count':1-index})
"#).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let doctrine=root.join("rich.md");std::fs::write(&doctrine,"Fixture").unwrap();
    let skills=richos_core::skills::ensure_rendered(&root.join("skills"),
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    let native=NativeCognition::start(&script,&root,&doctrine,&skills).unwrap();
    let native_id=native.session_id().to_string();
    spine.attach_lease(Box::new(native));
    spine.detach_conversation_io();
    let first=spine.submit_prompt("First original instruction",Source::Text).unwrap();
    let second=spine.submit_prompt("Second original instruction",Source::Text).unwrap();
    let spine=Mutex::new(spine);
    drive_shared_turns(&spine).unwrap();
    let spine=spine.into_inner().unwrap();
    let one=spine.ledger().turn(&first).unwrap();let two=spine.ledger().turn(&second).unwrap();
    assert_eq!(one.assistant_text,"Answer 1");assert_eq!(two.assistant_text,"Answer 2");
    assert_eq!(one.session_id.as_deref(),Some(native_id.as_str()));
    assert_eq!(two.session_id.as_deref(),Some(native_id.as_str()));
    assert_eq!(spine.ledger().thread_turns(&thread).unwrap().len(),2);
    drop(spine);std::fs::remove_dir_all(root).unwrap();
}


#[test]
fn app_introductions_and_key_changes_follow_their_conversation_without_a_hidden_turn() {
    use richos_core::cognition::MockCognition;
    let (root,mut spine,company)=fixture();
    let (thread,intro)=spine.open_conversation_with_message(&company,"Dictation","Use F2 for dictation.").unwrap();
    spine.open_conversation_with_message(&company,"Unrelated","Unrelated company thread notice").unwrap();
    spine.switch_thread(&thread).unwrap();
    let first=MockCognition::new("before-change",vec!["Ready"]);
    let first_messages=first.app_messages.clone();
    spine.attach_lease(Box::new(first));
    spine.submit_prompt("Which key?",Source::Text).unwrap();
    assert_eq!(*first_messages.lock().unwrap(),[(intro.clone(),"Use F2 for dictation.".into())]);
    let changed=spine.replace_app_message(&thread,Some(&intro),"Use F3 for dictation.","Changed dictation key from F2 to F3.").unwrap();
    let second=MockCognition::new("after-change",vec!["F3"]);
    let messages=second.app_messages.clone();
    spine.attach_lease(Box::new(second));
    spine.submit_prompt("And now?",Source::Text).unwrap();
    let observed=messages.lock().unwrap();
    assert_eq!(observed.len(),2,"only the current message and its settings action belong here");
    assert!(observed.contains(&(changed,"Use F3 for dictation.".into())));
    assert!(observed.iter().any(|(_,text)|text.contains("Changed dictation key from F2 to F3")));
    drop(observed);drop(spine);std::fs::remove_dir_all(root).unwrap();
}


#[test]
fn adopted_and_cold_sessions_both_register_stop_and_live_model_controls() {
    use richos_core::{cognition::LeaseFactory,config::ModelChoice,steering::{ConversationControl,TurnCancel,TurnControl}};
    struct Signal {models:AtomicUsize,stops:AtomicUsize}
    impl TurnCancel for Signal {
        fn cancel(&self)->bool {self.stops.fetch_add(1,Ordering::SeqCst);true}
        fn set_model(&self,_:ModelChoice)->Result<bool,String> {self.models.fetch_add(1,Ordering::SeqCst);Ok(true)}
    }
    struct Session {control:ConversationControl,signal:Arc<Signal>}
    impl Cognition for Session {
        fn session_id(&self)->&str {"controlled-session"}
        fn cancel_handle(&self)->Option<Arc<dyn TurnCancel>> {Some(self.signal.clone())}
        fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
            self.control.request_stop().unwrap();Ok("end_turn".into())
        }
    }
    struct Factory(Arc<Signal>);
    impl LeaseFactory for Factory {
        fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {panic!("a conversation must have its own scope")}
        fn spawn_scoped(&self,_:&richos_core::entity::ThreadBinding,control:&ConversationControl)->Result<Box<dyn Cognition>,CognitionError> {
            Ok(Box::new(Session {control:control.clone(),signal:self.0.clone()}))
        }
    }
    for adopt in [false,true] {
        let (root,mut spine,company)=fixture();
        let controls=TurnControl::open(root.join("intake.jsonl")).unwrap();
        spine.set_turn_control(controls.clone());
        let signal=Arc::new(Signal {models:AtomicUsize::new(0),stops:AtomicUsize::new(0)});
        spine.set_lease_factory(Box::new(Factory(signal.clone())));
        if adopt {
            spine.ready_a_spare_front_desk(&company);
            controls.set_model(ModelChoice::Fable).unwrap();
            assert_eq!(signal.models.load(Ordering::SeqCst),1,"Settings missed the waiting spare");
        }
        let thread=spine.create_thread("Conversation",&company).unwrap();
        spine.prime_front_desk(&thread);
        let control=spine.turn_control();
        assert_eq!(control.lease_session().as_deref(),Some("controlled-session"),"adopted={adopt}");
        assert!(control.set_model(ModelChoice::Fable).unwrap(),"adopted={adopt}");
        assert_eq!(signal.models.load(Ordering::SeqCst),if adopt {2}else{1});
        drop(spine.submit_prompt("Stop this request",Source::Text));
        assert!(signal.stops.load(Ordering::SeqCst)>0,"Stop missed adopted={adopt}");
        spine.ready_a_spare_front_desk(&company);
        let reserved=spine.spare_front_desk_reserved_thread().unwrap().to_string();
        assert!(controls.for_thread(&reserved).lease_session().is_some());
        spine.replace_app_message(&thread,None,"New app setting.","Changed the setting.").unwrap();
        assert!(controls.for_thread(&reserved).lease_session().is_none(),"discarded spare retained a live control");
        let before=signal.models.load(Ordering::SeqCst);
        controls.set_model(ModelChoice::Opus).unwrap();
        assert_eq!(signal.models.load(Ordering::SeqCst),before,"discarded sessions received model controls");
        drop(spine);std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn an_explicit_message_after_stop_reopens_instead_of_reusing_the_terminated_provider() {
    use richos_core::steering::{ConversationControl,TurnCancel,TurnControl};
    use std::sync::atomic::AtomicBool;
    struct Killed(AtomicBool);
    impl TurnCancel for Killed {
        fn cancel(&self)->bool {self.0.store(true,Ordering::SeqCst);true}
    }
    struct Stopped {control:ConversationControl,killed:Arc<Killed>,reason:Option<&'static str>,calls:Arc<AtomicUsize>}
    impl Cognition for Stopped {
        fn session_id(&self)->&str {"saved-native-session"}
        fn is_closed(&self)->bool {self.killed.0.load(Ordering::SeqCst)}
        fn cancel_handle(&self)->Option<Arc<dyn TurnCancel>> {Some(self.killed.clone())}
        fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
            panic!("this fixture submits before waiting, as the native provider does")
        }
        fn enqueue_prompt(&mut self,_:&str,started:Box<dyn FnOnce()->Result<(),String>+Send>)->Result<richos_core::cognition::PendingReply,CognitionError> {
            self.calls.fetch_add(1,Ordering::SeqCst);
            if self.killed.0.load(Ordering::SeqCst) {return Err(CognitionError::Io("provider closed".into()))}
            started().map_err(CognitionError::Protocol)?;
            let control=self.control.clone();let reason=self.reason;
            Ok(richos_core::cognition::PendingReply::new(move |_| {
                control.request_stop().unwrap();
                reason.map(|reason|Ok(reason.into())).unwrap_or_else(||Err(CognitionError::Io("provider EOF".into())))
            }))
        }
    }
    for reason in [None,Some(richos_core::native::STOP_REASON_CANCELLED),Some(richos_core::native::STOP_REASON_CANCEL_UNACKNOWLEDGED),Some("end_turn")] {
        let (root,mut spine,company)=fixture();
        let controls=TurnControl::open(root.join("intake.jsonl")).unwrap();
        spine.set_turn_control(controls.clone());
        let thread=spine.create_thread("Conversation",&company).unwrap();
        let factory=MockLeaseFactory::new(vec!["Explicit next reply"]);
        let opened=factory.spawned.clone();
        let prompts=factory.spawned_prompts.clone();
        spine.set_lease_factory(Box::new(factory));
        let calls=Arc::new(AtomicUsize::new(0));
        spine.attach_lease(Box::new(Stopped {control:controls.for_thread(&thread),killed:Arc::new(Killed(AtomicBool::new(false))),reason,calls:calls.clone()}));
        spine.submit_prompt("Stop this",Source::Text).unwrap();
        assert!(opened.lock().unwrap().is_empty(),"Stop must not reopen or replay by itself");
        spine.submit_prompt("This is my next explicit message",Source::Text).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst),1,"a terminated provider was reused for {reason:?}");
        assert_eq!(opened.lock().unwrap().len(),1);
        assert_eq!(*prompts.lock().unwrap()[0].lock().unwrap(),["This is my next explicit message"]);
        let turns=spine.ledger().thread_turns(&thread).unwrap();
        assert_eq!(turns.len(),2);
        assert_eq!(turns[0].state,if reason==Some("end_turn") {richos_core::ledger::TurnState::Completed} else {richos_core::ledger::TurnState::Stopped});
        assert_eq!(turns[1].assistant_text,"Explicit next reply");
        drop(spine);std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn native_eof_reopens_only_when_the_user_sends_another_message() {
    use richos_core::native::NativeCognition;
    use std::os::unix::fs::PermissionsExt;
    let (root,mut spine,company)=fixture();
    let thread=spine.create_thread("Conversation",&company).unwrap();
    let script=root.join("eof-provider.py");
    std::fs::write(&script,r#"#!/usr/bin/env python3
import json,sys
for line in sys.stdin:
 value=json.loads(line)
 if value['type']=='control_request':
  print(json.dumps({'type':'control_response','response':{'subtype':'success','request_id':value['request_id'],'response':{}}}),flush=True)
 elif value['type']=='user':
  sys.exit(0)
"#).unwrap();
    std::fs::set_permissions(&script,std::fs::Permissions::from_mode(0o755)).unwrap();
    let doctrine=root.join("rich.md");std::fs::write(&doctrine,"Fixture").unwrap();
    let skills=richos_core::skills::ensure_rendered(&root.join("skills"),
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    let native=NativeCognition::start(&script,&root,&doctrine,&skills).unwrap();
    let factory=MockLeaseFactory::new(vec!["Explicit next reply"]);
    let opened=factory.spawned.clone();let prompts=factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));spine.attach_lease(Box::new(native));
    assert!(spine.submit_prompt("The provider exits during this input",Source::Text).is_err());
    assert!(opened.lock().unwrap().is_empty(),"EOF must not trigger automatic replay");
    spine.submit_prompt("This is another explicit input",Source::Text).unwrap();
    assert_eq!(opened.lock().unwrap().len(),1);
    assert_eq!(*prompts.lock().unwrap()[0].lock().unwrap(),["This is another explicit input"]);
    let turns=spine.ledger().thread_turns(&thread).unwrap();
    assert_eq!(turns.len(),2);assert_eq!(turns[1].assistant_text,"Explicit next reply");
    drop(spine);std::fs::remove_dir_all(root).unwrap();
}


#[cfg(unix)]
#[test]
fn production_stop_saves_queued_input_and_reaps_workers_before_one_resume() {
    use richos_core::{cognition::LeaseFactory, engine_profile::EngineProfile, native::NativeCognition,
        runtime::EngineRuntime, spine::drive_shared_turns, steering::{ConversationControl, StopOutcome, TurnControl}};
    use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Mutex, time::{Duration, Instant}};
    fn until(what: &str, ready: impl Fn() -> bool) {
        // load-bound: hang guard; correctness requires a positive lifecycle, ledger or process fact.
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    struct DesktopFactory {
        profile: EngineProfile, bridge: richos_core::ecs::EcsBridge,
        script: PathBuf, skills: PathBuf, opened: Arc<AtomicUsize>,
    }
    impl LeaseFactory for DesktopFactory {
        fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
            Err(CognitionError::Protocol("this fixture requires a conversation binding".into()))
        }
        fn spawn_scoped(&self, binding: &richos_core::entity::ThreadBinding, control: &ConversationControl)
            -> Result<Box<dyn Cognition>, CognitionError> {
            let mut profile = self.profile.clone();
            profile.scope_to(binding).map_err(|e| CognitionError::Io(e.to_string()))?;
            let native = NativeCognition::start_with_engine(&self.script, &self.skills.join("agents/rich.md"),
                &self.skills, &self.script, self.bridge.clone(), profile, Some(control))?;
            self.opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(native))
        }
    }
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    spine.set_central_root(root.join("companies"));
    spine.set_onboarding_record(root.join("onboarding.json"));
    let controls = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(controls.clone());
    let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap();
    let runtime = if let Some(path) = std::env::var_os("RICHOS_RUNTIME_DIR") {
        EngineRuntime::load(&engine, Some(std::path::Path::new(&path))).unwrap()
    } else {
        // Source-test toolchain only. Node is not invoked by this controlled provider.
        let python = std::process::Command::new("python3").args(["-c", "import sys;print(sys.executable)"]).output().unwrap();
        EngineRuntime { root: root.join("runtime"), python: String::from_utf8(python.stdout).unwrap().trim().into(),
            git: "/usr/bin/git".into(), node: "/usr/bin/true".into(), versions: Default::default() }
    };
    let profile = EngineProfile::prepare_with_settings(&engine, &root.join("data"), runtime, Default::default()).unwrap();
    let bridge = richos_core::ecs::EcsBridge::new(&profile.runtime.python, &engine, &profile.state).unwrap();
    let skills = richos_core::skills::ensure_rendered(&root.join("skills"), &engine.join("app")).unwrap();
    let script = root.join("production-provider.py");
    std::fs::write(&script, r#"#!/usr/bin/env python3
import json,os,pathlib,select,subprocess,sys
root=pathlib.Path(__file__).resolve().parent
os.chdir(root)
def send(v): print(json.dumps(v),flush=True)
if "--no-session-persistence" in sys.argv:
    for line in sys.stdin:
        send({"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"text","text":"rich_handles_message"}]}})
        send({"type":"result","is_error":False,"result":"rich_handles_message"})
    sys.exit(0)
buffer=b""
log=open("provider.log","a")
def read():
    global buffer
    while b"\n" not in buffer:
        ready,_,_=select.select([sys.stdin],[],[],20)
        if not ready: raise RuntimeError("native input hang guard")
        chunk=os.read(sys.stdin.fileno(),4096)
        if not chunk: raise EOFError("native input closed")
        buffer+=chunk
    line,buffer=buffer.split(b"\n",1)
    value=json.loads(line);log.write(json.dumps(value)+"\n");log.flush();return value
def wanted(kind):
    while True:
        value=read()
        if value["type"]==kind:return value
def start(value):
    send({"type":"command_lifecycle","command_uuid":value["uuid"],"state":"started"})
    send({"type":"control_request","request_id":value["uuid"],"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt":value["message"]["content"][0]["text"]}}})
    return wanted("control_response")["response"]["response"]
resumed="--resume" in sys.argv
session=pathlib.Path(sys.argv[sys.argv.index("--resume")+1]).stem if resumed else sys.argv[sys.argv.index("--session-id")+1]
config=json.loads(sys.argv[sys.argv.index("--mcp-config")+1])
state=pathlib.Path(config["mcpServers"]["richos_onboarding"]["args"][1]).parent.parent
transcript=root/(session+".jsonl");transcript.touch(exist_ok=True)
evidence=state/"evidence"/session;evidence.mkdir(parents=True,exist_ok=True)
(evidence/"callbacks.jsonl").write_text(json.dumps({"callback":{"transcript_path":str(transcript)}})+"\n")
log.write(json.dumps({"event":"provider-start","session":session,"resumed":resumed})+"\n");log.flush()
init=read();send({"type":"control_response","response":{"subtype":"success","request_id":init["request_id"],"response":{}}})
first=wanted("user");hook=start(first)
assert hook.get("decision")!="block",hook
if resumed:
    send({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Explicit next reply"}]}})
    send({"type":"result","stop_reason":"end_turn"})
    while True:read()
worker=subprocess.Popen([sys.executable,"-c","import time;time.sleep(60)"],start_new_session=True,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
(root/"worker-pid").write_text(str(worker.pid))
send({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Working on one. "}]}})
second=wanted("user")
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"queued"})
(root/"second-queued").touch()
for _ in range(2):
    following=wanted("user")
    send({"type":"command_lifecycle","command_uuid":following["uuid"],"state":"queued"})
(root/"all-inputs-queued").touch()
# The production supervisor must reap this provider and its separate worker group.
# No fake interrupt acknowledgement or child cleanup stands in for that teardown.
while True:read()
"#).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let opened = Arc::new(AtomicUsize::new(0));
    spine.set_lease_factory(Box::new(DesktopFactory { profile: profile.clone(), bridge, script, skills, opened: opened.clone() }));
    spine.detach_conversation_io();
    let shared = Arc::new(Mutex::new(spine));
    let run = |shared: Arc<Mutex<Spine>>| std::thread::spawn(move || {
        shared.lock().unwrap().poll_intake().unwrap(); drive_shared_turns(&shared)
    });
    let first = controls.submit_from_desk(&thread, Some(company.clone()), "First message").unwrap();
    let first_driver = run(shared.clone());
    until("production provider never streamed", || shared.lock().unwrap().ledger().turn_for_intake(first.id()).is_some_and(|t|!t.assistant_text.is_empty()));
    let control = controls.for_thread(&thread);
    let session = control.lease_session().unwrap();
    let second = controls.submit_from_desk(&thread, Some(company.clone()), "Second message").unwrap();
    let second_driver = run(shared.clone());
    until("second input never reached the same production provider", || root.join("second-queued").exists());
    let spoken = shared.lock().unwrap().submit_prompt_spoken("Spoken follow-up", Source::Jam, true).unwrap();
    let spoken_driver = run(shared.clone());
    let pressed = shared.lock().unwrap().submit_pressed_prompt_to(&thread, "Pressed follow-up").unwrap();
    let pressed_driver = run(shared.clone());
    until("spoken or pressed input never reached the same provider", || root.join("all-inputs-queued").exists());
    let command_state = richos_core::lease_commands::state_path(&profile.state, &session);
    until("supervisor never observed its separate worker group", || matches!(richos_core::lease_commands::read(&command_state), richos_core::lease_commands::CommandReading::Running(n) if n>0));
    assert!(matches!(control.request_stop().unwrap(), StopOutcome::Requested {reached_lease:true,..}));
    let outcomes = [first_driver.join().unwrap(), second_driver.join().unwrap(),
        spoken_driver.join().unwrap(), pressed_driver.join().unwrap()];
    let worker: libc::pid_t = std::fs::read_to_string(root.join("worker-pid")).unwrap().parse().unwrap();
    until("production Stop left its worker alive", || unsafe { libc::kill(worker,0) } != 0);
    {
        let spine = shared.lock().unwrap();
        let a=spine.ledger().turn_for_intake(first.id()).unwrap();
        let b=spine.ledger().turn_for_intake(second.id()).unwrap();
        assert_eq!(a.state, richos_core::ledger::TurnState::Stopped);
        assert_eq!(b.state, richos_core::ledger::TurnState::Stopped, "intentional provider teardown must not report queued input as a failure");
        assert_eq!(b.user_text, "Second message"); assert!(b.assistant_text.is_empty());
        for id in [&spoken, &pressed] {
            let pending = spine.ledger().turn(id).unwrap();
            assert_eq!(pending.state, richos_core::ledger::TurnState::Stopped,
                "every accepted desktop input must share the same Stop cut: {}", pending.user_text);
            assert!(pending.assistant_text.is_empty());
            assert!(pending.intake_id.is_some(), "desktop input bypassed durable acceptance");
        }
        let speech = spine.ledger().turn(&spoken).unwrap();
        assert_eq!(speech.source, Source::Jam); assert_eq!(speech.rich_audible, Some(true));
        assert!(spine.ledger().turn(&pressed).unwrap().pressed);
        assert_eq!(opened.load(Ordering::SeqCst),1);
    }
    for outcome in outcomes { outcome.unwrap(); }
    let next=controls.submit_from_desk(&thread,Some(company),"Explicit next message").unwrap();
    shared.lock().unwrap().poll_intake().unwrap(); drive_shared_turns(&shared).unwrap();
    assert_eq!(opened.load(Ordering::SeqCst),2,"one explicit input must reopen exactly once");
    assert_eq!(control.lease_session().as_deref(),Some(session.as_str()),"resume must retain native history identity");
    assert_eq!(shared.lock().unwrap().ledger().turn_for_intake(next.id()).unwrap().assistant_text,"Explicit next reply");
    let frames: Vec<serde_json::Value> = std::fs::read_to_string(root.join("provider.log")).unwrap().lines().map(|l|serde_json::from_str(l).unwrap()).collect();
    let sent: Vec<_> = frames.iter().filter(|f|f["type"]=="user").map(|f|f["message"]["content"][0]["text"].as_str().unwrap()).collect();
    assert_eq!(sent,["First message","Second message","Spoken follow-up","Pressed follow-up","Explicit next message"]);
    assert_eq!(frames.iter().filter(|f|f["event"]=="provider-start").count(),2);
    drop(shared); std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn restart_applies_the_durable_stop_cut_without_stopping_other_or_later_inputs() {
    use richos_core::{ledger::TurnState, steering::{ActiveTurn, TurnControl}};
    // Cover B before ledger admission, received and already submitted when Stop was saved.
    for phase in 0..3 {
        let (root, spine, company) = fixture();
        drop(spine);
        let path = root.join("conversation.jsonl");
        let intake = root.join("intake.jsonl");
        let mut ledger = Ledger::open(&path).unwrap();
        let thread = ledger.create_thread("Stopped conversation", &company).unwrap();
        let other = ledger.create_thread("Independent conversation", &company).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let other_binding = ledger.thread_binding(&other).unwrap();
        let controls = TurnControl::open(&intake).unwrap();
        let a = controls.submit_from_desk(&thread, Some(company.clone()), "A running").unwrap();
        let a_turn = ledger.record_prompt_received_from_intake(&binding, "A running", Source::Text, a.id()).unwrap();
        ledger.mark_turn_started(&a_turn, "saved-native-session").unwrap();
        let b = controls.submit_from_desk(&thread, Some(company.clone()), "B saved before Stop").unwrap();
        if phase > 0 {
            let b_turn = ledger.record_prompt_received_from_intake(&binding, "B saved before Stop", Source::Text, b.id()).unwrap();
            if phase == 2 { ledger.mark_turn_started(&b_turn, "saved-native-session").unwrap(); }
        }
        let independent = controls.submit_from_desk(&other, Some(company.clone()), "Other conversation").unwrap();
        ledger.record_prompt_received_from_intake(&other_binding, "Other conversation", Source::Text, independent.id()).unwrap();
        controls.for_thread(&thread).begin_turn(ActiveTurn { turn_id: a_turn.clone(), thread_id: thread.clone(),
            entity_id: Some(company.clone()), started_at: Some(1) });
        controls.for_thread(&thread).request_stop().unwrap();
        let next = controls.submit_from_desk(&thread, Some(company.clone()), "Explicit input after Stop").unwrap();
        drop(ledger); drop(controls); // Crash before either terminal settlement or drain marker.
        let mut restored = Spine::new(Ledger::open(&path).unwrap());
        restored.set_entity_registry(EntityRegistry::new(vec![Entity::new("example", "Example", &["/fixture/example"]).unwrap()]).unwrap());
        let factory = MockLeaseFactory::new(vec!["Reply"]);
        let prompts = factory.spawned_prompts.clone();
        restored.set_lease_factory(Box::new(factory));
        let controls = TurnControl::open(&intake).unwrap();
        restored.set_turn_control(controls.clone());
        restored.reconcile_intake().unwrap(); restored.reconcile_intake().unwrap();
        for id in [a.id(), b.id()] {
            let turn = restored.ledger().turn_for_intake(id).unwrap();
            assert_eq!(turn.state, TurnState::Stopped, "phase {phase}, input {id}");
            assert!(turn.assistant_text.is_empty());
        }
        for id in [independent.id(), next.id()] {
            assert_eq!(restored.ledger().turn_for_intake(id).unwrap().state, TurnState::Completed);
        }
        let sent: Vec<_> = prompts.lock().unwrap().iter().flat_map(|p|p.lock().unwrap().clone()).collect();
        assert_eq!(sent, ["Other conversation", "Explicit input after Stop"]);
        assert!(controls.pending_intake().is_empty());
        drop(restored); std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn restart_cannot_reuse_a_direct_ledger_inputs_order() {
    use richos_core::steering::TurnControl;
    let (root, mut spine, company) = fixture();
    let path = root.join("conversation.jsonl");
    let intake = root.join("intake.jsonl");
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let controls = TurnControl::open(&intake).unwrap();
    spine.set_turn_control(controls.clone());
    spine.detach_conversation_io();
    let spoken = spine.submit_prompt_spoken("Spoken request", Source::Jam, true).unwrap();
    let pressed = spine.submit_pressed_prompt_to(&thread, "Pressed request").unwrap();
    let first = spine.ledger().turn(&spoken).unwrap().intake_id.unwrap();
    let last = spine.ledger().turn(&pressed).unwrap().intake_id.unwrap();
    assert!(last > first);
    drop(spine); drop(controls);
    let mut restored = Spine::new(Ledger::open(&path).unwrap());
    restored.set_entity_registry(EntityRegistry::new(vec![Entity::new("example", "Example", &["/fixture/example"]).unwrap()]).unwrap());
    restored.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Reply"])));
    let controls = TurnControl::open(&intake).unwrap();
    restored.set_turn_control(controls.clone());
    assert_eq!(restored.ledger().turn(&spoken).unwrap().rich_audible, Some(true));
    assert!(restored.ledger().turn(&pressed).unwrap().pressed);
    let next = controls.submit_from_desk(&thread, Some(company), "New request after restart").unwrap();
    assert!(next.id() > last, "restart reused an order already saved with a direct ledger input");
    restored.poll_intake().unwrap();
    assert_eq!(restored.ledger().turn_for_intake(next.id()).unwrap().user_text, "New request after restart");
    assert_eq!(restored.ledger().thread_turns(&thread).unwrap().len(), 3);
    drop(restored); std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_continuation_keeps_its_original_order_when_stop_names_a_follow_up() {
    use richos_core::{ledger::TurnState, steering::{ActiveTurn, TurnControl}};
    let (root, mut spine, company) = fixture();
    let path = root.join("conversation.jsonl");
    let intake = root.join("intake.jsonl");
    let thread = spine.create_thread("Conversation", &company).unwrap();
    drop(spine);
    let mut ledger = Ledger::open(&path).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let controls = TurnControl::open(&intake).unwrap();
    let a = controls.submit_from_desk(&thread, Some(company.clone()), "Original request").unwrap();
    let a_turn = ledger.record_prompt_received_from_intake(&binding, "Original request", Source::Text, a.id()).unwrap();
    let continued = ledger.replay_turn(&binding, &a_turn).unwrap();
    ledger.mark_turn_started(&continued, "native-session").unwrap();
    let b = controls.submit_from_desk(&thread, Some(company.clone()), "Follow-up").unwrap();
    let b_turn = ledger.record_prompt_received_from_intake(&binding, "Follow-up", Source::Text, b.id()).unwrap();
    ledger.mark_turn_started(&b_turn, "native-session").unwrap();
    controls.for_thread(&thread).begin_turn(ActiveTurn {turn_id:b_turn.clone(), thread_id:thread, entity_id:Some(company), started_at:Some(1)});
    controls.for_thread(binding.thread_id()).request_stop().unwrap();
    drop(ledger); drop(controls);
    let mut restored = Spine::new(Ledger::open(&path).unwrap());
    restored.set_turn_control(TurnControl::open(&intake).unwrap());
    restored.reconcile_intake().unwrap();
    assert_eq!(restored.ledger().turn(&continued).unwrap().state, TurnState::Stopped, "continuation lost the original input order");
    assert_eq!(restored.ledger().turn(&continued).unwrap().intake_id, Some(a.id()));
    assert_eq!(restored.ledger().turn(&b_turn).unwrap().state, TurnState::Stopped);
    drop(restored); std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn restart_stops_saved_direct_speech_and_pressed_inputs() {
    use richos_core::{ledger::TurnState, steering::{ActiveTurn, TurnControl}};
    for (source, audible, pressed) in [(Source::Jam, Some(false), false), (Source::Jam, Some(true), false), (Source::Text, None, true)] {
        for submitted in [false, true] {
            let (root, mut spine, company) = fixture();
            let path = root.join("conversation.jsonl");
            let intake = root.join("intake.jsonl");
            let thread = spine.create_thread("Conversation", &company).unwrap();
            let controls = TurnControl::open(&intake).unwrap();
            spine.set_turn_control(controls.clone());
            spine.detach_conversation_io();
            let a = spine.submit_prompt("First request", Source::Text).unwrap();
            let pending = if pressed {spine.submit_pressed_prompt_to(&thread, "Queued input").unwrap()}
                else {spine.submit_prompt_spoken("Queued input", source, audible.unwrap()).unwrap()};
            let order = spine.ledger().turn(&pending).unwrap().intake_id.unwrap();
            drop(spine);
            let mut ledger = Ledger::open(&path).unwrap();
            if submitted {ledger.mark_turn_started(&pending, "native-session").unwrap();}
            controls.for_thread(&thread).begin_turn(ActiveTurn {turn_id:a.clone(), thread_id:thread.clone(), entity_id:Some(company.clone()), started_at:Some(1)});
            controls.for_thread(&thread).request_stop().unwrap();
            drop(ledger); drop(controls);
            let mut restored = Spine::new(Ledger::open(&path).unwrap());
            restored.set_turn_control(TurnControl::open(&intake).unwrap());
            restored.reconcile_intake().unwrap();
            assert_eq!(restored.ledger().turn(&a).unwrap().state, TurnState::Stopped);
            let saved = restored.ledger().turn(&pending).unwrap();
            assert_eq!(saved.state, TurnState::Stopped, "source {source:?}, submitted {submitted}");
            assert_eq!(saved.intake_id, Some(order));
            assert_eq!(saved.source, source); assert_eq!(saved.rich_audible, audible); assert_eq!(saved.pressed, pressed);
            assert_eq!(saved.user_text, "Queued input"); assert!(saved.assistant_text.is_empty());
            drop(restored); std::fs::remove_dir_all(root).unwrap();
        }
    }
}

// Stop pressed on the loading screen goes to the conversation it names (follow-ups after
// nightly 65, item 9; review rv-20261018T050016Z-78f97c9d-30c4). Opening B activates it
// before its saved messages are read, while the screen still offers Stop for A.
#[test]
fn stop_pressed_while_another_conversation_opens_reaches_the_one_it_names() {
    use richos_core::{read_view::SpineView, steering::{StopOutcome, TurnCancel, TurnControl}};
    struct Counted(Arc<AtomicUsize>);
    impl TurnCancel for Counted {
        fn cancel(&self) -> bool { self.0.fetch_add(1, Ordering::SeqCst); true }
    }
    let (root, mut spine, company) = fixture();
    let controls = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(controls.clone());
    let previous = spine.create_thread("Previous conversation", &company).unwrap();
    let destination = spine.create_thread("Destination conversation", &company).unwrap();
    spine.switch_thread(&previous).unwrap();
    spine.detach_conversation_io();
    spine.submit_prompt("Keep working on this", Source::Text).unwrap();
    let cancels = Arc::new(AtomicUsize::new(0));
    controls.for_thread(&previous).set_cancel(Some(Arc::new(Counted(cancels.clone()))));
    let reader = spine.reader();
    spine.switch_thread(&destination).unwrap();
    let view = reader.snapshot();
    assert_eq!(view.active_thread(), Some(destination.as_str()));
    let named = view.stop_conversation(Some(&previous));
    assert_eq!(named, Some(previous.as_str()), "Stop for the previous conversation was dropped");
    let outcome = controls.for_thread(named.unwrap()).request_stop().unwrap();
    assert!(matches!(outcome, StopOutcome::Requested { reached_lease: true, .. }), "{outcome:?}");
    assert_eq!(cancels.load(Ordering::SeqCst), 1);
    assert_eq!(view.stop_conversation(None), Some(destination.as_str()));
    assert_eq!(view.stop_conversation(Some("thr_not_in_this_ledger")), None);
    drop(view); drop(spine); std::fs::remove_dir_all(root).unwrap();
}

// A question-card answer is something he sent, so his Stop covers it like a typed
// message (follow-ups after nightly 65, item 8; review rv-20261018T045039Z-78f97c9d-20fc#1).
fn card_answer(thread: &str, company: &EntityId, id: &str, text: &str) -> richos_core::questions::Delivery {
    richos_core::questions::Delivery {
        id: id.into(), entity_id: company.to_string(), thread_id: thread.into(), asker: "front_desk".into(),
        obligation_id: None, set_id: Some("questions".into()), text: text.into(), receipt: None, order: None,
    }
}

#[derive(Default)]
struct HeldProvider { state: std::sync::Mutex<(usize, bool)>, changed: std::sync::Condvar }
impl HeldProvider {
    fn await_inputs(&self, count: usize) {
        let guard = self.state.lock().unwrap();
        let (guard, _) = self.changed.wait_timeout_while(guard, std::time::Duration::from_secs(10), |s| s.0 < count).unwrap();
        assert!(guard.0 >= count, "input never submitted");
    }
}
impl richos_core::steering::TurnCancel for HeldProvider {
    fn cancel(&self) -> bool {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
        true
    }
}
struct HeldLease(Arc<HeldProvider>);
impl Cognition for HeldLease {
    fn session_id(&self) -> &str { "held-session" }
    fn is_closed(&self) -> bool { self.0.state.lock().unwrap().1 }
    fn cancel_handle(&self) -> Option<Arc<dyn richos_core::steering::TurnCancel>> { Some(self.0.clone()) }
    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        unreachable!("replies are detached, as in the app")
    }
    fn enqueue_prompt(&mut self, _: &str, started: Box<dyn FnOnce() -> Result<(), String> + Send>)
        -> Result<richos_core::cognition::PendingReply, CognitionError> {
        started().map_err(CognitionError::Protocol)?;
        self.0.state.lock().unwrap().0 += 1;
        self.0.changed.notify_all();
        let provider = self.0.clone();
        Ok(richos_core::cognition::PendingReply::new(move |_| {
            let guard = provider.state.lock().unwrap();
            let (guard, _) = provider.changed.wait_timeout_while(guard, std::time::Duration::from_secs(10), |s| !s.1).unwrap();
            assert!(guard.1, "Stop never closed the provider");
            Err(CognitionError::from(richos_core::native::NativeError::Closed))
        }))
    }
}

#[test]
fn stop_covers_a_question_card_answer_sent_before_it() {
    use richos_core::{ledger::TurnState, spine::drive_shared_turns, steering::{ActiveTurn, TurnControl}};
    use std::sync::Mutex;

    // (1) The answer waits behind the request Rich is working on when he presses Stop.
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let controls = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(controls.clone());
    let factory = MockLeaseFactory::new(vec!["Continued despite Stop"]);
    let prompts = factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.detach_conversation_io();
    let first = spine.submit_prompt("First request", Source::Text).unwrap();
    let answer = spine.queue_question_answer(&card_answer(&thread, &company, "queued", "My answer sent before Stop")).unwrap();
    controls.for_thread(&thread).request_stop().unwrap();
    let shared = Mutex::new(spine);
    drive_shared_turns(&shared).unwrap();
    let spine = shared.into_inner().unwrap();
    assert_eq!(spine.ledger().turn(&first).unwrap().state, TurnState::Stopped);
    assert_eq!(spine.ledger().turn(&answer).unwrap().state, TurnState::Stopped, "a card answer sent before Stop ran after it");
    let sent: Vec<_> = prompts.lock().unwrap().iter().flat_map(|p| p.lock().unwrap().clone()).collect();
    assert!(!sent.iter().any(|p| p.contains("My answer sent before Stop")), "sent after Stop: {sent:?}");
    drop(spine); std::fs::remove_dir_all(root).unwrap();

    // (2) Rich is answering the card when he types a follow-up, then presses Stop.
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let controls = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(controls.clone());
    let provider = Arc::new(HeldProvider::default());
    spine.attach_lease(Box::new(HeldLease(provider.clone())));
    spine.detach_conversation_io();
    let answer = spine.queue_question_answer(&card_answer(&thread, &company, "live", "My clicked answer")).unwrap();
    let shared = Arc::new(Mutex::new(spine));
    let first_driver = { let shared = shared.clone(); std::thread::spawn(move || drive_shared_turns(&shared)) };
    provider.await_inputs(1);
    let typed = shared.lock().unwrap().submit_prompt("Typed follow-up", Source::Text).unwrap();
    let second_driver = { let shared = shared.clone(); std::thread::spawn(move || drive_shared_turns(&shared)) };
    provider.await_inputs(2);
    controls.for_thread(&thread).request_stop().unwrap();
    drop(first_driver.join().unwrap()); drop(second_driver.join().unwrap());
    let spine = shared.lock().unwrap();
    assert_eq!(spine.ledger().turn(&answer).unwrap().state, TurnState::Stopped, "the card answer's reply was not stopped");
    assert_eq!(spine.ledger().turn(&typed).unwrap().state, TurnState::Stopped);
    drop(spine); drop(shared); std::fs::remove_dir_all(root).unwrap();

    // (3) The app ends before the Stop settles; with and without an account continuation.
    for continued in [false, true] {
        let (root, mut spine, company) = fixture();
        let path = root.join("conversation.jsonl");
        let intake = root.join("intake.jsonl");
        let thread = spine.create_thread("Conversation", &company).unwrap();
        let controls = TurnControl::open(&intake).unwrap();
        spine.set_turn_control(controls.clone());
        spine.detach_conversation_io();
        let mut answer = spine.queue_question_answer(&card_answer(&thread, &company, "saved", "Saved clicked answer")).unwrap();
        let typed = spine.submit_prompt("Typed follow-up", Source::Text).unwrap();
        drop(spine);
        let mut ledger = Ledger::open(&path).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        if continued { answer = ledger.replay_turn(&binding, &answer).unwrap(); }
        ledger.mark_turn_started(&answer, "saved-native-session").unwrap();
        ledger.mark_turn_started(&typed, "saved-native-session").unwrap();
        controls.for_thread(&thread).begin_turn(ActiveTurn { turn_id: typed.clone(), thread_id: thread.clone(),
            entity_id: Some(company.clone()), started_at: Some(1) });
        controls.for_thread(&thread).request_stop().unwrap();
        let next = controls.submit_from_desk(&thread, Some(company), "New request after Stop").unwrap();
        drop(ledger); drop(controls);
        let mut restored = Spine::new(Ledger::open(&path).unwrap());
        restored.set_entity_registry(EntityRegistry::new(vec![Entity::new("example", "Example", &["/fixture/example"]).unwrap()]).unwrap());
        restored.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Next reply"])));
        restored.set_turn_control(TurnControl::open(&intake).unwrap());
        restored.reconcile_intake().unwrap();
        assert_eq!(restored.ledger().turn(&answer).unwrap().state, TurnState::Stopped, "continuation {continued}");
        assert_eq!(restored.ledger().turn(&typed).unwrap().state, TurnState::Stopped);
        assert_eq!(restored.ledger().turn_for_intake(next.id()).unwrap().state, TurnState::Completed);
        drop(restored); std::fs::remove_dir_all(root).unwrap();
    }

    // (4) The click is accepted and saved, he presses Stop, and only then does the delivery
    // worker hand the answer to the spine (review rv-20261018T195734Z-6e3fa251-5bed#1). The
    // order is taken where the app's click handler takes it (question_host::answer).
    use richos_core::questions::{AnswerRequest, AskScope, OptionInput, QuestionInput, Store};
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let controls = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(controls.clone());
    spine.detach_conversation_io();
    let factory = MockLeaseFactory::new(vec!["Continued despite Stop"]);
    let prompts = factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));
    let first = spine.submit_prompt("Keep working", Source::Text).unwrap();
    let store = Store::new(root.join("engine-state"));
    let q = store.ask(&AskScope { root: root.join("engine-state"), entity_id: company.to_string(), thread_id: thread.clone(),
        turn_id: first.clone(), asker: "front_desk".into(), obligation_id: None, session_id: "fixture-session".into(),
        engine: None, entity_root: None, app_run: None }, vec![QuestionInput {
        text: "When should the release ship?".into(), multiple: false, free_answer: true, recommended: Some(1),
        options: vec![OptionInput { label: "Today".into(), description: "Sooner".into() },
            OptionInput { label: "Tomorrow".into(), description: "More checking".into() }] }]).unwrap().remove(0);
    let order = controls.take_input_order();
    let accepted = store.answer_in_order(company.as_str(), &thread, AnswerRequest { question_id: q.id.clone(),
        client_id: "clicked-before-stop".into(), option_ids: vec![q.options[1].id.clone()], text: String::new(),
        expected_revision: None }, "click", "mac", order).unwrap();
    assert_eq!(accepted.outcome, "accepted");
    assert_eq!(store.highest_order().unwrap(), order, "a restart advances past the saved order");
    controls.for_thread(&thread).request_stop().unwrap();
    assert!(controls.for_thread(&thread).stop_claim().is_some(), "the Stop is still pending at delivery");
    let mut answer = None;
    store.deliver(company.as_str(), &thread, "front_desk", |delivery| {
        let receipt = spine.queue_question_answer(delivery).map_err(|e| e.to_string())?;
        answer = Some(receipt.trim_start_matches(richos_core::questions::STOPPED_RECEIPT_PREFIX).to_string());
        Ok(receipt)
    }).unwrap();
    let answer = answer.expect("the saved answer was delivered");
    assert_eq!(spine.ledger().turn(&answer).unwrap().intake_id, order, "the order taken at the click reaches the ledger");
    // The card says his Stop covered it (review rv-20261019T115204Z-2b619840-5405#2): with the
    // Stop still pending, the receipt was an ordinary one and the card read "Rich has your answer".
    let card = store.list(company.as_str(), &thread).unwrap().remove(0).public_value();
    assert_eq!((&card["delivered"], &card["ended_before_taken"]), (&serde_json::json!(false), &serde_json::json!(true)),
        "the card of an answer a pending Stop covered: {card}");
    let shared = Mutex::new(spine);
    drive_shared_turns(&shared).unwrap();
    let spine = shared.into_inner().unwrap();
    assert_eq!(spine.ledger().turn(&first).unwrap().state, TurnState::Stopped);
    assert_eq!(spine.ledger().turn(&answer).unwrap().state, TurnState::Stopped, "an answer clicked before Stop ran after it");
    let sent: Vec<_> = prompts.lock().unwrap().iter().flat_map(|p| p.lock().unwrap().clone()).collect();
    assert!(!sent.iter().any(|p| p.contains("Tomorrow")), "sent after Stop: {sent:?}");
    drop(spine); std::fs::remove_dir_all(root).unwrap();

    // (5) As (4), but the stopped turn finishes and its Stop settles before the delivery
    // worker gets the spine (review rv-20261018T203003Z-956a6d9c-1297#1).
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let controls = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(controls.clone());
    spine.detach_conversation_io();
    let factory = MockLeaseFactory::new(vec!["Continued despite Stop"]);
    let prompts = factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));
    let first = spine.submit_prompt("Keep working", Source::Text).unwrap();
    let store = Store::new(root.join("engine-state"));
    let q = store.ask(&AskScope { root: root.join("engine-state"), entity_id: company.to_string(), thread_id: thread.clone(),
        turn_id: first.clone(), asker: "front_desk".into(), obligation_id: None, session_id: "fixture-session".into(),
        engine: None, entity_root: None, app_run: None }, vec![QuestionInput {
        text: "When should the release ship?".into(), multiple: false, free_answer: true, recommended: Some(1),
        options: vec![OptionInput { label: "Today".into(), description: "Sooner".into() },
            OptionInput { label: "Tomorrow".into(), description: "More checking".into() }] }]).unwrap().remove(0);
    let order = controls.take_input_order();
    store.answer_in_order(company.as_str(), &thread, AnswerRequest { question_id: q.id.clone(),
        client_id: "clicked-before-stop".into(), option_ids: vec![q.options[1].id.clone()], text: String::new(),
        expected_revision: None }, "click", "mac", order).unwrap();
    controls.for_thread(&thread).request_stop().unwrap();
    let shared = Mutex::new(spine);
    drive_shared_turns(&shared).unwrap();
    assert_eq!(shared.lock().unwrap().ledger().turn(&first).unwrap().state, TurnState::Stopped);
    assert!(controls.for_thread(&thread).stop_claim().is_none(), "the Stop settled before delivery");
    let mut answer = None;
    store.deliver(company.as_str(), &thread, "front_desk", |delivery| {
        let receipt = shared.lock().unwrap().queue_question_answer(delivery).map_err(|e| e.to_string())?;
        answer = Some(receipt.trim_start_matches(richos_core::questions::STOPPED_RECEIPT_PREFIX).to_string());
        Ok(receipt)
    }).unwrap();
    drive_shared_turns(&shared).unwrap();
    let spine = shared.into_inner().unwrap();
    let answer = answer.expect("the saved answer was delivered");
    assert_eq!(spine.ledger().turn(&answer).unwrap().state, TurnState::Stopped, "an answer clicked before Stop ran after it settled");
    // ... and the card says so (review rv-20261018T204635Z-c68a30ec-f61e#1): it read "Rich has
    // your answer" with no button, for an answer his Stop had covered.
    let card = store.list(company.as_str(), &thread).unwrap().remove(0).public_value();
    assert_eq!((&card["delivered"], &card["ended_before_taken"]), (&serde_json::json!(false), &serde_json::json!(true)),
        "the card of a stopped answer: {card}");
    let sent: Vec<_> = prompts.lock().unwrap().iter().flat_map(|p| p.lock().unwrap().clone()).collect();
    assert!(!sent.iter().any(|p| p.contains("Tomorrow")), "sent after Stop: {sent:?}");
    drop(spine); std::fs::remove_dir_all(root).unwrap();

    // (6) As (5), and the app restarts after the Stop settled and before the delivery worker
    // gets the spine (review rv-20261018T204635Z-c68a30ec-f61e#2): the cut must outlive memory.
    let (root, mut spine, company) = fixture();
    let thread = spine.create_thread("Conversation", &company).unwrap();
    let intake = root.join("intake.jsonl");
    let controls = TurnControl::open(&intake).unwrap();
    spine.set_turn_control(controls.clone());
    spine.detach_conversation_io();
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Continued despite Stop"])));
    let first = spine.submit_prompt("Keep working", Source::Text).unwrap();
    let store = Store::new(root.join("engine-state"));
    let q = store.ask(&AskScope { root: root.join("engine-state"), entity_id: company.to_string(), thread_id: thread.clone(),
        turn_id: first.clone(), asker: "front_desk".into(), obligation_id: None, session_id: "fixture-session".into(),
        engine: None, entity_root: None, app_run: None }, vec![QuestionInput {
        text: "When should the release ship?".into(), multiple: false, free_answer: true, recommended: Some(1),
        options: vec![OptionInput { label: "Today".into(), description: "Sooner".into() },
            OptionInput { label: "Tomorrow".into(), description: "More checking".into() }] }]).unwrap().remove(0);
    let order = controls.take_input_order();
    store.answer_in_order(company.as_str(), &thread, AnswerRequest { question_id: q.id.clone(),
        client_id: "clicked-before-stop".into(), option_ids: vec![q.options[1].id.clone()], text: String::new(),
        expected_revision: None }, "click", "mac", order).unwrap();
    controls.for_thread(&thread).request_stop().unwrap();
    let shared = Mutex::new(spine);
    drive_shared_turns(&shared).unwrap();
    assert!(controls.for_thread(&thread).stop_claim().is_none(), "the Stop settled before the restart");
    drop(shared); drop(controls);
    let mut spine = Spine::new(Ledger::open(root.join("conversation.jsonl")).unwrap());
    spine.set_entity_registry(EntityRegistry::new(vec![Entity::new("example", "Example", &["/fixture/example"]).unwrap()]).unwrap());
    let controls = TurnControl::open(&intake).unwrap();
    controls.advance_input_order(store.highest_order().unwrap().unwrap());
    spine.set_turn_control(controls.clone());
    spine.detach_conversation_io();
    let factory = MockLeaseFactory::new(vec!["Continued after restart"]);
    let prompts = factory.spawned_prompts.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.reconcile_intake().unwrap();
    let mut answer = None;
    store.deliver(company.as_str(), &thread, "front_desk", |delivery| {
        let receipt = spine.queue_question_answer(delivery).map_err(|e| e.to_string())?;
        answer = Some(receipt.trim_start_matches(richos_core::questions::STOPPED_RECEIPT_PREFIX).to_string());
        Ok(receipt)
    }).unwrap();
    let shared = Mutex::new(spine);
    drive_shared_turns(&shared).unwrap();
    let spine = shared.into_inner().unwrap();
    let answer = answer.expect("the saved answer was delivered");
    assert_eq!(spine.ledger().turn(&answer).unwrap().state, TurnState::Stopped, "an answer clicked before Stop ran after a restart");
    let sent: Vec<_> = prompts.lock().unwrap().iter().flat_map(|p| p.lock().unwrap().clone()).collect();
    assert!(!sent.iter().any(|p| p.contains("Tomorrow")), "sent after Stop and a restart: {sent:?}");
    drop(spine); std::fs::remove_dir_all(root).unwrap();
}
