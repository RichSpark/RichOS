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
