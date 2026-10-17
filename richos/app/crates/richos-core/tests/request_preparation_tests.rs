//! Accepted requests own preparation, cancellation and every terminal outcome.
use richos_core::{
    cognition::{
        Cognition, CognitionError, LeaseFactory, MockCognition, MockLeaseFactory, TurnItem,
    },
    entity::EntityId,
    ledger::{Ledger, Source, TurnState},
    live::{LiveEvent, LiveObserver},
    spine::Spine,
    steering::{StopOutcome, TurnCancel, TurnControl},
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
fn fixture() -> (Spine, TurnControl, Events, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("richos-preparation-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    spine
        .ensure_active_thread_in(&EntityId::parse("femcboost").unwrap())
        .unwrap();
    let control = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let events = Events::default();
    spine.set_live_observer(Box::new(events.clone()));
    (spine, control, events, root)
}
struct FailedPrime;
impl Cognition for FailedPrime {
    fn session_id(&self) -> &str {
        "failed-prime"
    }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Err(CognitionError::Io("first prime disconnected".into()))
    }
    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        panic!("user prompt must not follow failed priming")
    }
}
#[test]
fn prime_failure_terminalizes_the_accepted_turn_and_survives_reload() {
    let (mut spine, control, events, root) = fixture();
    spine.attach_lease(Box::new(FailedPrime));
    assert!(spine
        .submit_prompt("start the interview", Source::Text)
        .is_err());
    let turn = spine
        .ledger()
        .turns()
        .iter()
        .find(|t| t.source == Source::Text)
        .unwrap();
    assert_eq!(turn.state,TurnState::Interrupted);
    assert!(turn.ended_at.is_some());
    assert_eq!(events.statuses(), vec!["queued", "working", "failed"]);
    assert!(control.active_turn().is_none());
    assert!(!spine.is_turn_in_progress());
    let id = turn.id.clone();
    drop(spine);
    assert_eq!(
        Ledger::open(root.join("ledger.jsonl"))
            .unwrap()
            .turn(&id)
            .unwrap()
            .state,
        TurnState::Interrupted
    );
}
struct NotifyCancel(Mutex<mpsc::Sender<()>>);
impl TurnCancel for NotifyCancel {
    fn cancel(&self) -> bool {
        self.0.lock().unwrap().send(()).is_ok()
    }
}
struct WaitingPrime {
    entered: mpsc::Sender<()>,
    cancelled: mpsc::Receiver<()>,
    handle: Arc<NotifyCancel>,
    natural_finish: bool,
}
impl Cognition for WaitingPrime {
    fn session_id(&self) -> &str {
        "waiting-prime"
    }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.entered.send(()).unwrap();
        self.cancelled.recv_timeout(Duration::from_secs(5)).unwrap();
        if self.natural_finish {
            Ok(())
        } else {
            Err(CognitionError::PrimingStopped("cancelled".into()))
        }
    }
    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        panic!("Stop during preparation must prevent user-prompt delivery")
    }
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(self.handle.clone())
    }
}
fn stop_prime(natural_finish: bool) {
    let (mut spine, control, events, _) = fixture();
    let (tx, rx) = mpsc::channel();
    let (cx, cr) = mpsc::channel();
    spine.attach_lease(Box::new(WaitingPrime {
        entered: tx,
        cancelled: cr,
        handle: Arc::new(NotifyCancel(Mutex::new(cx))),
        natural_finish,
    }));
    let worker = std::thread::spawn(move || {
        let id = spine.submit_prompt("start", Source::Text).unwrap();
        (spine, id)
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(events.statuses(), vec!["queued", "working"]);
    assert!(matches!(
        control.request_stop().unwrap(),
        StopOutcome::Requested {
            reached_lease: true,
            ..
        }
    ));
    let (spine, id) = worker.join().unwrap();
    assert_eq!(spine.ledger().turn(&id).unwrap().state, TurnState::Stopped);
    assert_eq!(events.statuses(), vec!["queued", "working", "stopped"]);
    assert!(control.active_turn().is_none());
}
#[test]
fn first_prime_is_cancellable() {
    stop_prime(false)
}
#[test]
fn stop_landing_as_internal_prime_completes_still_prevents_the_user_prompt() {
    stop_prime(true)
}
#[test]
fn recovery_factory_failure_always_resolves_recovering() {
    let (mut spine, control, events, _) = fixture();
    spine.attach_lease(Box::new(FailedPrime));
    let factory = MockLeaseFactory::new(vec!["unused"]);
    factory.fail_next_spawn();
    spine.set_lease_factory(Box::new(factory));
    assert!(spine.submit_prompt("retry", Source::Text).is_err());
    assert_eq!(events.statuses().last().unwrap(), "failed");
    assert!(spine
        .ledger()
        .pending_turns()
        .iter()
        .all(|t| t.source == Source::Internal));
    assert!(control.active_turn().is_none());
}
#[test]
fn prime_failure_has_one_bounded_recovery_and_one_visible_exchange() {
    let (mut spine, _, events, _) = fixture();
    spine.attach_lease(Box::new(FailedPrime));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["ready"])));
    spine.submit_prompt("start", Source::Text).unwrap();
    assert_eq!(events.statuses().last().unwrap(), "completed");
    assert_eq!(
        spine
            .ledger()
            .turns()
            .iter()
            .filter(|t| t.source == Source::Text && t.superseded_by.is_none())
            .count(),
        1
    );
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
    assert_eq!(*prompts.lock().unwrap(),vec!["not started", "may have side effects"]);
    let prior: serde_json::Value = serde_json::from_str(&contexts.lock().unwrap()[0]).unwrap();
    assert_eq!(prior["previous_delivery_attempt"]["turn_id"], second);
    assert_eq!(prior["previous_delivery_attempt"]["session_id"], "old");
    assert_eq!(prior["previous_delivery_attempt"]["delivery_outcome"], "unknown");
    let successor = restored.ledger().turn(&second).unwrap().superseded_by.as_ref().unwrap();
    assert_eq!(restored.ledger().turn(successor).unwrap().state, TurnState::Completed);
    assert_eq!(restored.messages(binding.thread_id()).unwrap().iter().filter(|m| m.role == "user").count(), 2);
}
struct WaitingFactory {
    entered: Mutex<mpsc::Sender<()>>,
}
impl LeaseFactory for WaitingFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        panic!("must use cancellable launch")
    }
    fn spawn_cancellable(&self, c: &TurnControl) -> Result<Box<dyn Cognition>, CognitionError> {
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

struct UnacknowledgedPrime {
    control: TurnControl,
    dropped: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for UnacknowledgedPrime {
    fn drop(&mut self) {
        self.dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
impl Cognition for UnacknowledgedPrime {
    fn session_id(&self) -> &str {
        "deaf-prime"
    }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.control.request_stop().unwrap();
        Err(CognitionError::PrimingStopped(
            "cancel_unacknowledged".into(),
        ))
    }
    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        panic!("a deaf prime must never receive a user prompt or a handoff request")
    }
}
#[test]
fn an_unacknowledged_priming_stop_retires_the_child_before_the_next_request() {
    let (mut spine, control, _, _) = fixture();
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    spine.attach_lease(Box::new(UnacknowledgedPrime {
        control,
        dropped: dropped.clone(),
    }));
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["new answer"])));
    let stopped = spine.submit_prompt("stop this", Source::Text).unwrap();
    assert_eq!(
        spine.ledger().turn(&stopped).unwrap().state,
        TurnState::Stopped
    );
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!spine.has_lease());
    let next = spine.submit_prompt("a new request", Source::Text).unwrap();
    assert_eq!(
        spine.ledger().turn(&next).unwrap().assistant_text,
        "new answer"
    );
    assert_eq!(
        spine.ledger().turn(&next).unwrap().state,
        TurnState::Completed
    );
}

struct OneShotFactory(Mutex<Option<Box<dyn Cognition>>>);
impl LeaseFactory for OneShotFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        self.0
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| CognitionError::Io("no second automatic attempt".into()))
    }
}
#[test]
fn deferred_rotation_sends_stop_to_the_successor_while_it_primes() {
    let (mut spine, control, events, _) = fixture();
    spine.attach_lease(Box::new(MockCognition::new(
        "outgoing",
        vec!["answer", "summary"],
    )));
    spine.submit_prompt("first", Source::Text).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let successor = WaitingPrime {
        entered: entered_tx,
        cancelled: cancel_rx,
        handle: Arc::new(NotifyCancel(Mutex::new(cancel_tx))),
        natural_finish: false,
    };
    spine.set_lease_factory(Box::new(OneShotFactory(Mutex::new(Some(Box::new(
        successor,
    ))))));
    spine.request_rotation("test").unwrap();
    assert_eq!(spine.rotation_count(), 0);
    let worker = std::thread::spawn(move || {
        let id = spine.submit_prompt("second", Source::Text).unwrap();
        (spine, id)
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(events.statuses().last().unwrap(), "working");
    assert!(matches!(
        control.request_stop().unwrap(),
        StopOutcome::Requested {
            reached_lease: true,
            ..
        }
    ));
    let (spine, id) = worker.join().unwrap();
    assert_eq!(spine.ledger().turn(&id).unwrap().state, TurnState::Stopped);
    assert_eq!(events.statuses().last().unwrap(), "stopped");
}
struct WaitingHandoff {
    entered: mpsc::Sender<()>,
    cancelled: mpsc::Receiver<()>,
    handle: Arc<NotifyCancel>,
    calls: usize,
}
impl Cognition for WaitingHandoff {
    fn session_id(&self) -> &str {
        "outgoing-handoff"
    }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
    }
    fn prompt(
        &mut self,
        text: &str,
        _: &mut dyn FnMut(TurnItem),
    ) -> Result<String, CognitionError> {
        self.calls += 1;
        if self.calls == 1 {
            return Ok("end_turn".into());
        }
        let event: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(event["event"], "session_handoff_requested");
        self.entered.send(()).unwrap();
        self.cancelled.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok("cancelled".into())
    }
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(self.handle.clone())
    }
}
#[test]
fn deferred_rotation_handoff_is_cancellable_without_spawning_a_successor() {
    let (mut spine, control, events, _) = fixture();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    spine.attach_lease(Box::new(WaitingHandoff {
        entered: entered_tx,
        cancelled: cancel_rx,
        handle: Arc::new(NotifyCancel(Mutex::new(cancel_tx))),
        calls: 0,
    }));
    let factory = MockLeaseFactory::new(vec!["must never run"]);
    let spawned = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.submit_prompt("first", Source::Text).unwrap();
    spine.request_rotation("test").unwrap();
    assert!(
        entered_rx.try_recv().is_err(),
        "nothing starts after the completed request"
    );
    let worker = std::thread::spawn(move || {
        let id = spine.submit_prompt("next", Source::Text).unwrap();
        (spine, id)
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(events.statuses().last().unwrap(), "working");
    control.request_stop().unwrap();
    let (spine, id) = worker.join().unwrap();
    assert_eq!(spine.ledger().turn(&id).unwrap().state, TurnState::Stopped);
    assert!(spawned.lock().unwrap().is_empty());
}
#[test]
fn deferred_rotation_never_files_the_outgoing_company_summary_under_the_new_company() {
    let (mut spine, _, _, _) = fixture();
    let company_a_thread = spine.active_binding().unwrap().thread_id().to_string();
    spine.attach_lease(Box::new(MockCognition::new(
        "company-a",
        vec!["A reply", "A-ONLY-SECRET-8842"],
    )));
    let factory = MockLeaseFactory::new(vec!["B reply"]);
    let primed = factory.spawned.clone();
    spine.set_lease_factory(Box::new(factory));
    spine.submit_prompt("first", Source::Text).unwrap();
    spine.request_rotation("watermark").unwrap();
    let company_b_thread = spine
        .create_thread("Company B", &EntityId::parse("deeply").unwrap())
        .unwrap();
    spine.switch_thread(&company_b_thread).unwrap();
    let id = spine.submit_prompt("hello B", Source::Text).unwrap();
    assert_eq!(
        spine.ledger().handoff_summary(&company_a_thread),
        Some("A-ONLY-SECRET-8842")
    );
    assert_eq!(spine.ledger().handoff_summary(&company_b_thread), None);
    assert!(!primed.lock().unwrap()[0].lock().unwrap()[0].contains("A-ONLY-SECRET-8842"));
    assert_eq!(
        spine.ledger().turn(&id).unwrap().session_id.as_deref(),
        spine.lease_session_id()
    );
}

struct SteerThenFailPrime {
    control: TurnControl,
    first: bool,
}
impl Cognition for SteerThenFailPrime {
    fn session_id(&self) -> &str {
        "steer-prime"
    }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        if std::mem::take(&mut self.first) {
            self.control.steer("the queued request").unwrap();
            Err(CognitionError::Io("preparation failed".into()))
        } else {
            Ok(())
        }
    }
    fn prompt(
        &mut self,
        text: &str,
        on_item: &mut dyn FnMut(TurnItem),
    ) -> Result<String, CognitionError> {
        on_item(TurnItem::Text { seq: 0, text });
        Ok("end_turn".into())
    }
}
#[test]
fn a_failed_preparation_does_not_strand_durable_steering_behind_it() {
    let (mut spine, control, _, _) = fixture();
    spine.attach_lease(Box::new(SteerThenFailPrime {
        control,
        first: true,
    }));
    assert!(spine.submit_prompt("first request", Source::Text).is_err());
    let turns: Vec<_> = spine
        .ledger()
        .turns()
        .iter()
        .filter(|t| t.source == Source::Text)
        .collect();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].state, TurnState::Interrupted);
    assert_eq!(turns[1].state, TurnState::Completed);
    assert_eq!(turns[1].assistant_text, "the queued request");
    assert_eq!(spine.queue_depth(), 0);
}

struct GatedReply {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    first: bool,
}
impl Cognition for GatedReply {
    fn session_id(&self) -> &str { "gated-visible-reply" }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        if std::mem::take(&mut self.first) {
            self.entered.send(()).unwrap();
            // Only a hang guard: channel entry and successful try_lock prove availability.
            self.release.recv_timeout(Duration::from_secs(30)).unwrap();
        }
        on_item(TurnItem::Text { seq: 0, text });
        Ok("end_turn".into())
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
    spine.attach_lease(Box::new(GatedReply {entered, release:gated, first:true}));
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
        let second=available.submit_prompt_to(&second_thread,"second request",Source::Text).unwrap();
        assert_eq!(control.active_turn().unwrap().turn_id,first);
        assert_eq!(available.ledger().turn(&second).unwrap().state,TurnState::Received);
        second
    };
    release.send(()).unwrap();
    worker.join().unwrap().unwrap();
    let finished=shared.lock().unwrap();
    assert!(control.active_turn().is_none());
    assert_eq!(*completed.0.lock().unwrap(),vec![(first_thread.clone(),first.clone()),(second_thread.clone(),second.clone())],
        "the ordinary completion observer must see both the original reply and its queued reply");
    for (id,thread,text) in [(first,first_thread,"first request"),(second,second_thread,"second request")] {
        let turn=finished.ledger().turn(&id).unwrap();
        assert_eq!(turn.state,TurnState::Completed);
        assert_eq!(turn.thread_id,thread);
        assert_eq!(turn.assistant_text,text);
    }
}
