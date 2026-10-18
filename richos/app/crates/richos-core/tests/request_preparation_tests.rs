//! Accepted requests own preparation, cancellation and every terminal outcome.
use richos_core::{
    cognition::{
        Cognition, CognitionError, LeaseFactory, MockCognition, MockLeaseFactory, TurnItem,
    },
    entity::EntityId,
    ledger::{Ledger, Source, TurnState},
    live::{LiveEvent, LiveObserver},
    spine::Spine,
    steering::{StopOutcome, TurnCancel, TurnControl, ConversationControl},
};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
mod support;
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<(String, serde_json::Value)>>>);
impl LiveObserver for Events {
    fn on_live_event(&self, event: &LiveEvent) {
        self.0
            .lock()
            .unwrap()
            .push((event.event_name().into(), event.payload()));
    }
}
impl Events {
    fn statuses(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "rich://turn-status")
            .map(|(_, p)| p["status"].as_str().unwrap().into())
            .collect()
    }
}
fn fixture() -> (Spine, ConversationControl, Events, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("richos-preparation-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    spine
        .ensure_active_thread_in(&EntityId::parse("femcboost").unwrap())
        .unwrap();
    let control = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let control=spine.turn_control();
    let events = Events::default();
    spine.set_live_observer(Box::new(events.clone()));
    (spine, control, events, root)
}
struct NotifyCancel(Mutex<mpsc::Sender<()>>);
impl TurnCancel for NotifyCancel {
    fn cancel(&self) -> bool {
        self.0.lock().unwrap().send(()).is_ok()
    }
}
#[test]
fn restart_continues_saved_requests_and_preserves_uncertain_attempt_timing() {
    let (spine, _, _, root) = fixture();
    let binding = spine.active_binding().unwrap().clone();
    drop(spine);
    let mut ledger = Ledger::open(root.join("ledger.jsonl")).unwrap();
    let first = ledger
        .record_prompt_received(&binding, "not started", Source::Text)
        .unwrap();
    let second = ledger
        .record_prompt_received(&binding, "may have side effects", Source::Text)
        .unwrap();
    ledger.mark_turn_started(&second, "old").unwrap();
    drop(ledger);
    let mut restored = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    restored.set_turn_control(TurnControl::open(root.join("intake.jsonl")).unwrap());
    let mock = MockCognition::new("new", vec![]);
    let prompts = mock.prompts.clone();
    let contexts = mock.recovery_contexts.clone();
    restored.switch_thread(binding.thread_id()).unwrap();
    restored.attach_lease(Box::new(mock));
    restored.reconcile_intake().unwrap();
    for id in [first.clone(), second.clone()] {
        let turn = restored.ledger().turn(&id).unwrap();
        assert_eq!(turn.state,if id==first {TurnState::Completed}else{TurnState::Interrupted});
        if id==first {continue}
        assert_eq!(turn.ended_at, None, "restart cannot know when the process died");
        assert_eq!(turn.active_ms(), None, "closed-app time must never count as work");
        let reloaded = Ledger::open(root.join("ledger.jsonl")).unwrap();
        assert_eq!(reloaded.turn(&id).unwrap().active_ms(), None);
    }
    assert_eq!(*prompts.lock().unwrap(),vec!["not started"]);
    assert!(contexts.lock().unwrap().is_empty(), "restart reconstructed a hidden context prompt");
    assert!(restored.ledger().turn(&second).unwrap().superseded_by.is_none(), "restart repeated uncertain side effects");
    assert_eq!(restored.messages(binding.thread_id()).unwrap().iter().filter(|m| m.role == "user").count(), 2);
}
struct WaitingFactory {
    entered: Mutex<mpsc::Sender<()>>,
}
impl LeaseFactory for WaitingFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        panic!("must use cancellable launch")
    }
    fn spawn_cancellable(&self, c: &ConversationControl) -> Result<Box<dyn Cognition>, CognitionError> {
        self.entered.lock().unwrap().send(()).unwrap();
        let start = std::time::Instant::now();
        while c.stop_claim().is_none() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(CognitionError::Io("launch cancelled".into()))
    }
}
#[test]
fn stop_during_initial_launch_is_terminal_and_never_retried() {
    let (mut spine, control, events, _) = fixture();
    let (tx, rx) = mpsc::channel();
    spine.set_lease_factory(Box::new(WaitingFactory {
        entered: Mutex::new(tx),
    }));
    let worker = std::thread::spawn(move || {
        let id = spine.submit_prompt("first request", Source::Text).unwrap();
        (spine, id)
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(events.statuses(), vec!["queued", "working"]);
    assert!(matches!(
        control.request_stop().unwrap(),
        StopOutcome::Requested { .. }
    ));
    let (spine, id) = worker.join().unwrap();
    assert_eq!(spine.ledger().turn(&id).unwrap().state, TurnState::Stopped);
    assert_eq!(events.statuses().last().unwrap(), "stopped");
}

struct GatedReply {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    first: bool,
    finished: Arc<(Mutex<bool>,std::sync::Condvar)>,
}
impl Cognition for GatedReply {
    fn session_id(&self) -> &str { "gated-visible-reply" }

    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String,CognitionError> {
        panic!("this fixture submits before waiting, as NativeCognition does")
    }
    fn enqueue_prompt(&mut self, text: &str, started: Box<dyn FnOnce() -> Result<(),String> + Send>) -> Result<richos_core::cognition::PendingReply,CognitionError> {
        let text=text.to_string();
        let finished=self.finished.clone();
        if std::mem::take(&mut self.first) {
            started().map_err(CognitionError::Protocol)?;
            self.entered.send(()).unwrap();
            let release=std::mem::replace(&mut self.release,mpsc::channel().1);
            Ok(richos_core::cognition::PendingReply::new(move |on| {
                // Hang guard: a positive channel release controls this reply.
                release.recv_timeout(Duration::from_secs(30)).unwrap();
                *finished.0.lock().unwrap()=true;
                finished.1.notify_all();
                on(TurnItem::Text{seq:0,text:&text});
                Ok("end_turn".into())
            }))
        } else {
            Ok(richos_core::cognition::PendingReply::new(move |on| {
                // Native admission waits for the previous native result, even when stdin is already written.
                let (done, timeout)=finished.1.wait_timeout_while(finished.0.lock().unwrap(),Duration::from_secs(30),|done|!*done).unwrap();
                assert!(*done && !timeout.timed_out(),"hang guard: previous native answer never finished");
                drop(done);
                started().map_err(CognitionError::Protocol)?;
                on(TurnItem::Text{seq:0,text:&text});
                Ok("end_turn".into())
            }))
        }
    }

}

#[derive(Clone,Default)]
struct CompletedReplies(Arc<Mutex<Vec<(String,String)>>>);
impl richos_core::stream::TurnObserver for CompletedReplies {
    fn on_event(&self,event:&richos_core::stream::StreamEvent) {
        if let richos_core::stream::StreamEvent::TurnCompleted {thread_id,turn_id,..}=event {
            self.0.lock().unwrap().push((thread_id.clone(),turn_id.clone()));
        }
    }
}

#[test]
fn a_shared_reply_leaves_navigation_and_durable_input_available_without_changing_its_destination() {
    let (mut spine, control, _, _) = fixture();
    let first_thread=spine.active_thread().unwrap().to_string();
    let second_thread=spine.create_thread("Other conversation", &EntityId::parse("femcboost").unwrap()).unwrap();
    let (entered, ready)=mpsc::channel();
    let (release, gated)=mpsc::channel();
    spine.attach_lease(Box::new(GatedReply {entered, release:gated, first:true,finished:Arc::new((Mutex::new(false),std::sync::Condvar::new()))}));
    spine.detach_conversation_io();
    let completed=CompletedReplies::default();
    spine.set_observer(Box::new(completed.clone()));
    let first=spine.submit_prompt_to(&first_thread,"first request",Source::Agent).unwrap();
    let shared=Arc::new(Mutex::new(spine));
    let driving=shared.clone();
    let worker=std::thread::spawn(move ||richos_core::spine::drive_shared_turns(&driving));
    ready.recv_timeout(Duration::from_secs(30)).unwrap();
    let second={
        let mut available=shared.try_lock().expect("model reply held the shared conversation lock");
        assert_eq!(control.active_turn().unwrap().turn_id,first);
        assert!(available.has_lease(),"an active provider disappeared from availability");
        available.switch_thread(&second_thread).unwrap();
        control.submit_from_desk(&first_thread,Some(EntityId::parse("femcboost").unwrap()),"second request").unwrap();
        available.poll_intake().unwrap();
        let second=available.ledger().turns().iter().find(|turn|turn.user_text=="second request").unwrap().id.clone();
        assert_eq!(available.active_thread(),Some(second_thread.as_str()));
        assert_eq!(control.active_turn().unwrap().turn_id,first);
        assert_eq!(available.ledger().turn(&second).unwrap().state,TurnState::InFlight);
        second
    };
    release.send(()).unwrap();
    worker.join().unwrap().unwrap();
    let finished=shared.lock().unwrap();
    assert!(control.active_turn().is_none());
    let mut actual=completed.0.lock().unwrap().clone();
    let mut expected=vec![(first_thread.clone(),first.clone()),(first_thread.clone(),second.clone())];
    // Independent reply owners can commit their receipts in either order.
    actual.sort();
    expected.sort();
    assert_eq!(actual,expected,
        "the ordinary completion observer must see both the original reply and its queued reply");
    for (id,thread,text) in [(first,first_thread.clone(),"first request"),(second,first_thread.clone(),"second request")] {
        let turn=finished.ledger().turn(&id).unwrap();
        assert_eq!(turn.state,TurnState::Completed);
        assert_eq!(turn.thread_id,thread);
        assert_eq!(turn.assistant_text,text);
    }
}

#[test]
fn another_conversation_completes_before_the_first_reply_is_released() {
    struct Completed(mpsc::Sender<String>);
    impl richos_core::stream::TurnObserver for Completed {
        fn on_event(&self,event:&richos_core::stream::StreamEvent) {
            if let richos_core::stream::StreamEvent::TurnCompleted {thread_id,..}=event {
                self.0.send(thread_id.clone()).unwrap();
            }
        }
    }
    let (mut spine, control, _, _) = fixture();
    let a=spine.active_thread().unwrap().to_string();
    let b=spine.create_thread("Independent conversation", &EntityId::parse("femcboost").unwrap()).unwrap();
    let (entered, ready)=mpsc::channel();
    let (release, gated)=mpsc::channel();
    spine.attach_lease(Box::new(GatedReply {entered, release:gated, first:true,finished:Arc::new((Mutex::new(false),std::sync::Condvar::new()))}));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["B answered"])));
    spine.detach_conversation_io();
    let (completed, completion)=mpsc::channel();
    spine.set_observer(Box::new(Completed(completed)));
    let first=spine.submit_prompt_to(&a,"A is busy",Source::Text).unwrap();
    let shared=Arc::new(Mutex::new(spine));
    let driving=shared.clone();
    let worker=std::thread::spawn(move ||richos_core::spine::drive_shared_turns(&driving));
    ready.recv_timeout(Duration::from_secs(30)).unwrap();
    let second={
        let mut state=shared.lock().unwrap();
        control.submit_from_desk(&b,Some(EntityId::parse("femcboost").unwrap()),"B question").unwrap();
        state.poll_intake().unwrap();
        state.ledger().thread_turns(&b).unwrap().iter().find(|t| t.source==Source::Text).map(|t|t.id.clone())
    };
    richos_core::spine::drive_shared_turns(&shared).unwrap();
    // Either driver may own B. Wait for B's actual completion while A remains held.
    let completed=completion.recv_timeout(Duration::from_secs(30));
    let b_finished=second.as_ref().is_some_and(|id|shared.lock().unwrap().ledger().turn(id).unwrap().state==TurnState::Completed);
    let a_still_working=shared.lock().unwrap().ledger().turn(&first).unwrap().state==TurnState::InFlight;
    // Release the owned reply before assertions, including on the expected red.
    release.send(()).unwrap();
    worker.join().unwrap().unwrap();
    assert_eq!(completed.unwrap(),b,"hang guard: independent conversation never completed");
    assert!(b_finished,"conversation B waited for A's reply instead of running its own session");
    assert!(a_still_working,"B's completion changed A's lifecycle");
    let state=shared.lock().unwrap();
    let second=state.ledger().turn(second.as_ref().unwrap()).unwrap();
    assert_eq!(control.for_thread(&b).lease_session().as_deref(),second.session_id.as_deref(),
        "a parked conversation lost its own session identity");
    assert_ne!(state.ledger().turn(&first).unwrap().session_id,second.session_id);
}

struct OneConversationFactory(Mutex<Option<Box<dyn Cognition>>>);
impl LeaseFactory for OneConversationFactory {
    fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {Ok(self.0.lock().unwrap().take().unwrap())}
}
struct CancellableConversation {
    entered:mpsc::Sender<()>,
    stopped:mpsc::Receiver<()>,
    cancel:Arc<NotifyCancel>,
}
impl Cognition for CancellableConversation {
    fn session_id(&self)->&str {"conversation-b"}

    fn cancel_handle(&self)->Option<Arc<dyn TurnCancel>> {Some(self.cancel.clone())}
    fn prompt(&mut self,_:&str,on_item:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
        on_item(TurnItem::Text{seq:0,text:"B partial"});
        self.entered.send(()).unwrap();
        // Only a hang guard: the matching native cancellation signal releases this reply.
        self.stopped.recv_timeout(Duration::from_secs(30)).unwrap();
        Ok(richos_core::native::STOP_REASON_CANCELLED.into())
    }
}
#[test]
fn stopping_one_of_two_live_conversations_keeps_the_other_and_its_queued_request() {
    let (mut spine,control,_,_)=fixture();
    let a=spine.active_thread().unwrap().to_string();
    let b=spine.create_thread("B",&EntityId::parse("femcboost").unwrap()).unwrap();
    let (entered,ready)=mpsc::channel();
    let (release,gated)=mpsc::channel();
    spine.attach_lease(Box::new(GatedReply{entered,release:gated,first:true,finished:Arc::new((Mutex::new(false),std::sync::Condvar::new()))}));
    let (b_entered,b_ready)=mpsc::channel();
    let (stop,stopped)=mpsc::channel();
    spine.set_lease_factory(Box::new(OneConversationFactory(Mutex::new(Some(Box::new(CancellableConversation{
        entered:b_entered,stopped,cancel:Arc::new(NotifyCancel(Mutex::new(stop))),
    }))))));
    spine.detach_conversation_io();
    let first=spine.submit_prompt_to(&a,"A first",Source::Text).unwrap();
    let shared=Arc::new(Mutex::new(spine));
    let driving=shared.clone();
    let a_worker=std::thread::spawn(move||richos_core::spine::drive_shared_turns(&driving));
    ready.recv_timeout(Duration::from_secs(30)).unwrap();
    let (queued,second)={
        let mut state=shared.lock().unwrap();
        let queued=state.submit_prompt_to(&a,"A next",Source::Text).unwrap();
        let second=state.submit_prompt_to(&b,"B first",Source::Text).unwrap();
        (queued,second)
    };
    let driving=shared.clone();
    let b_worker=std::thread::spawn(move||richos_core::spine::drive_shared_turns(&driving));
    b_ready.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(control.for_thread(&a).active_turn().is_some());
    assert!(matches!(control.for_thread(&b).request_stop_for(&second).unwrap(),StopOutcome::Requested{reached_lease:true,..}));
    assert_eq!(control.for_thread(&a).active_turn().unwrap().turn_id,first,"B Stop changed A's active input");
    assert_eq!(shared.lock().unwrap().ledger().turn(&first).unwrap().state,TurnState::InFlight);
    release.send(()).unwrap();
    a_worker.join().unwrap().unwrap();
    b_worker.join().unwrap().unwrap();
    let state=shared.lock().unwrap();
    assert_eq!(state.ledger().turn(&first).unwrap().state,TurnState::Completed);
    assert_eq!(state.ledger().turn(&second).unwrap().state,TurnState::Stopped);
    assert_eq!(state.ledger().turn(&queued).unwrap().state,TurnState::Completed);
    assert_eq!(state.ledger().turn(&queued).unwrap().assistant_text,"A next");
    assert!(!control.has_active_turns());
}

#[test]
fn quitting_records_every_active_stop_and_restart_replays_neither_conversation() {
    let (mut spine,control,_,root)=fixture();
    let a=spine.active_thread().unwrap().to_string();
    let b=spine.create_thread("B",&EntityId::parse("femcboost").unwrap()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("quit-a",vec!["unused"])));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["unused"])));
    spine.detach_conversation_io();
    let first=spine.submit_prompt_to(&a,"A unfinished",Source::Text).unwrap();
    let second=spine.submit_prompt_to(&b,"B unfinished",Source::Text).unwrap();
    assert_eq!(control.active_turn_count(),2);
    // The shipping Quit calls, before either delivery has finished.
    let stopped=control.request_stop_all();
    control.shutdown_leases();
    let durable=control.pending_intake().iter().filter(|record|matches!(record,richos_core::steering::IntakeRecord::Stop{..})).count();
    drop(spine);
    let mut restarted=support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    restarted.set_turn_control(TurnControl::open(root.join("intake.jsonl")).unwrap());
    let recovered=restarted.reconcile_intake();
    let states=[first,second].map(|id|restarted.ledger().turn(&id).unwrap().state);
    let replayed=restarted.ledger().turns().iter().filter(|turn|turn.superseded_by.is_some()).count();
    drop(restarted);
    drop(control);
    std::fs::remove_dir_all(root).unwrap();
    assert!(stopped.is_ok());
    assert_eq!(durable,2,"Quit did not persist every active conversation's Stop before shutdown");
    assert!(recovered.is_ok());
    assert_eq!(states,[TurnState::Stopped,TurnState::Stopped]);
    assert_eq!(replayed,0,"a request the user stopped on Quit was replayed after restart");
}
#[derive(Clone, Copy, Debug)]
enum PreparationGate {
    Spawn,
    Scope,
}
struct GateConnection {
    stage: PreparationGate,
    entered: mpsc::Sender<()>,
    release: Option<mpsc::Receiver<()>>,
}
impl GateConnection {
    fn hold(&mut self) {
        if let Some(release) = self.release.take() {
            self.entered.send(()).unwrap();
            // Hang guard only. The withheld channel release establishes ordering.
            release.recv_timeout(Duration::from_secs(60)).unwrap();
        }
    }
}
impl Cognition for GateConnection {
    fn session_id(&self) -> &str {
        "preparation-A"
    }
    fn requires_thread_isolation(&self) -> bool {
        true
    }

    fn prepare_work_turn(
        &mut self,
        _: &richos_core::entity::ThreadBinding,
        _: &str,
        _: &str,
        _: Source,
        _: &str,
        _: &mut dyn FnMut(TurnItem),
    ) -> Result<(), CognitionError> {
        if matches!(self.stage, PreparationGate::Scope) {
            self.hold()
        }
        Ok(())
    }
    fn prompt(&mut self, _: &str, on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        on(TurnItem::Text {
            seq: 0,
            text: "A answered",
        });
        Ok("end_turn".into())
    }
}
struct GateFactory {
    a: String,
    gate: Mutex<Option<GateConnection>>,
}
impl LeaseFactory for GateFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Ok(Box::new(MockCognition::new(
            "independent",
            vec!["B answered"],
        )))
    }
    fn spawn_scoped(
        &self,
        b: &richos_core::entity::ThreadBinding,
        _: &ConversationControl,
    ) -> Result<Box<dyn Cognition>, CognitionError> {
        if b.thread_id() == self.a {
            if let Some(mut gate) = self.gate.lock().unwrap().take() {
                gate.hold();
                return Ok(Box::new(gate));
            }
        }
        self.spawn()
    }
}
#[test]
fn spawn_and_scope_preparation_never_hold_another_conversation() {
    for stage in [
        PreparationGate::Spawn,
        PreparationGate::Scope,
    ] {
        let (mut spine, _, _, _) = fixture();
        let a = spine.active_thread().unwrap().to_string();
        let b = spine
            .create_thread("Independent B", &EntityId::parse("femcboost").unwrap())
            .unwrap();
        let (entered, ready) = mpsc::channel();
        let (release, gated) = mpsc::channel();
        let connection = GateConnection {
            stage,
            entered,
            release: Some(gated),
        };
        if matches!(stage, PreparationGate::Spawn) {
            spine.set_lease_factory(Box::new(GateFactory {
                a: a.clone(),
                gate: Mutex::new(Some(connection)),
            }));
        } else {
            spine.attach_lease(Box::new(connection));
            spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["B answered"])));

        }
        spine.detach_conversation_io();
        let shared = Arc::new(Mutex::new(spine));
        let driving = shared.clone();
        let (finished, a_finished) = mpsc::channel();
        let worker = std::thread::spawn(move || {
                driving
                    .lock()
                    .unwrap()
                    .submit_prompt_to(&a, "Next A", Source::Text)
                    .unwrap();
                richos_core::spine::drive_shared_turns(&driving).unwrap();
            finished.send(()).unwrap();
        });
        ready.recv_timeout(Duration::from_secs(60)).unwrap();
        let (available, second) = match shared.try_lock() {
            Ok(mut s) => (
                true,
                Some(s.submit_prompt_to(&b, "B question", Source::Text).unwrap()),
            ),
            Err(std::sync::TryLockError::WouldBlock) => (false, None),
            Err(e) => panic!("poisoned state: {e}"),
        };
        let b_finished = if let Some(id) = second {
            richos_core::spine::drive_shared_turns(&shared).unwrap();
            let s = shared.lock().unwrap();
            s.ledger().turn(&id).unwrap().state == TurnState::Completed
        } else {
            false
        };
        let a_held = matches!(a_finished.try_recv(), Err(mpsc::TryRecvError::Empty));
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(available, "{stage:?} held shared state");
        assert!(b_finished, "B waited behind {stage:?}");
        assert!(a_held, "{stage:?} was released before B finished");
    }
}

struct GatedRetryClock {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl richos_core::upstream::RetryClock for GatedRetryClock {
    fn wait(&self, _: Duration, _: &dyn Fn() -> bool) -> bool {
        self.entered.send(()).unwrap();
        // Hang guard only. Retry stays held until B has finished.
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(60))
            .unwrap();
        true
    }
}
#[test]
fn another_conversation_finishes_while_the_first_waits_to_retry_its_provider() {
    let (mut spine, _, _, _) = fixture();
    let a = spine.active_thread().unwrap().to_string();
    let b = spine
        .create_thread("B during retry", &EntityId::parse("femcboost").unwrap())
        .unwrap();
    spine.attach_lease(Box::new(MockCognition::new(
        "A",
        vec!["API Error: 529 overloaded", "A retried"],
    )));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["B answered"])));
    let (entered, ready) = mpsc::channel();
    let (release, held) = mpsc::channel();
    spine.set_retry_clock(Arc::new(GatedRetryClock {
        entered,
        release: Mutex::new(held),
    }));
    spine.detach_conversation_io();
    spine.submit_prompt_to(&a, "A", Source::Text).unwrap();
    let shared = Arc::new(Mutex::new(spine));
    let driving = shared.clone();
    let worker = std::thread::spawn(move || richos_core::spine::drive_shared_turns(&driving));
    ready.recv_timeout(Duration::from_secs(60)).unwrap();
    let (available, second) = match shared.try_lock() {
        Ok(mut s) => (
            true,
            Some(s.submit_prompt_to(&b, "B", Source::Text).unwrap()),
        ),
        Err(std::sync::TryLockError::WouldBlock) => (false, None),
        Err(e) => panic!("poisoned: {e}"),
    };
    let finished = if let Some(id) = second {
        richos_core::spine::drive_shared_turns(&shared).unwrap();
        shared.lock().unwrap().ledger().turn(&id).unwrap().state == TurnState::Completed
    } else {
        false
    };
    let held = !worker.is_finished();
    release.send(()).unwrap();
    worker.join().unwrap().unwrap();
    assert!(available, "provider retry held shared conversation state");
    assert!(finished, "B waited for A's provider retry");
    assert!(held);
}
