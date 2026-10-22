//! One independent Stu session per conversation, driven only by its durable inbox.
//! Job reports belong to Stu and the existing engine work tools. This host manages
//! provider connections and transports facts; process lifecycle never changes job state.
use crate::assignment::{self, Assignment, AssignmentState, PendingNotice};
use crate::cognition::{Cognition, CognitionError, LeaseFactory, TurnItem};
use crate::entity::ThreadBinding;
use crate::steering::{CommandStop, TurnCancel};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

pub trait WorkNotifier: Send + Sync {
    fn raised(&self, thread_id: &str, notice: &PendingNotice);
    fn nothing_left_to_do(&self) {}
    /// Stu's work met an overload and waits out the CEO's schedule: `sentence` is
    /// `RetryBudget::ceo_message`, the same "RichOS will try again in N minutes" line his
    /// conversation shows for Rich, for the conversation `thread_id`.
    fn overload_retry(&self, _thread_id: &str, _sentence: &str) {}
}
pub trait OperatorIntake: Send + Sync {
    fn take(&self, record: Assignment);
    fn stop(&self, entity: &str, thread: &str, id: &str) -> Result<(), String>;
}
// Retained for the separate operator client's verified report completion adapter.
pub trait AnsweredClose: Send + Sync {
    fn close_answered(&self, record: &Assignment, answer: &str) -> Result<(), String>;
}
pub type LeadSink<'a> = &'a dyn Fn(&crate::questions::Delivery) -> Result<String, String>;
pub const ROTATION_WAITS_FOR_COMMANDS: std::time::Duration = std::time::Duration::from_secs(600);
/// **How often an idle back end checks that Stu is on the account in use** (review
/// rv-20261019T005929Z-e3d9ad03-3e7b, finding 2). A switch decided while Stu is idle, by Rich's
/// turn or the quota service's own check, otherwise left his session on the old account until
/// the next message the app sent him. The check reads two ids; the move itself waits for a
/// boundary (`WorkHost::move_to_account_in_use`).
pub const ACCOUNT_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// What renewing Stu's lease for the account in use came to.
#[derive(Debug, PartialEq)]
enum Renewal {
    /// No switch is due, or there is no lease to move.
    NotDue,
    /// Something of Stu's still runs on the old account: a turn (his own or an automatic one),
    /// input of ours, a command or a helper. Ending his session now would end it.
    Waits { from: Option<String> },
    /// He is on a new session on the account in use.
    Moved { from: Option<String>, to: Option<String> },
    /// The old session ended and no new one could be opened; the next boundary opens one.
    Failed,
}

#[derive(Clone)]
enum InputReceipt { Peer(u64), Answer(String) }
impl InputReceipt {
    fn input_id(&self)->String {match self {Self::Peer(through)=>format!("peer:{through}"),Self::Answer(id)=>format!("answer:{id}")}}
}

/// A wait of the workers' overload clock on one Stu session: the retry it is and how long it
/// is, and the `worker_overload` fact for each worker it will restart.
struct WorkerWait {
    session: String,
    retry: u32,
    wait: std::time::Duration,
    facts: Vec<serde_json::Value>,
}

#[derive(Default)]
struct Inner {
    peer_input: Option<Arc<dyn TurnCancel>>,
    overload: crate::upstream::RetryBudget,
    /// ONE overload clock for all of Stu's WORKERS (follow-up 11), counted apart from his own
    /// so a turn of his that succeeds does not restart theirs. An overload is Anthropic's
    /// servers being overloaded and hits every running worker at once, so it is one schedule,
    /// as Rich's and Stu's own are: the overloads that arrive during a wait join it
    /// ([`Inner::worker_wait`]), the next one after it moves one step, a success resets it.
    workers: crate::upstream::RetryBudget,
    /// The wait the workers' clock is running, and every worker the overload stopped, each told
    /// to Stu when it ends.
    worker_wait: Option<WorkerWait>,
    /// The description each worker run was started under, by the tool call that started it
    /// (Claude Code's `task_started`), told to Stu as the worker's task.
    worker_names: std::collections::HashMap<String, String>,
    /// Inputs accepted by this live backend. Unfinished handoffs remain durable for restart.
    // Input already delivered to this session stays delivered even when a turn fails.
    peer_active_through: Option<u64>,
    peer_stopper: Option<(String, Arc<dyn TurnCancel>)>,
    processing: bool,
    cancel: Option<Arc<dyn TurnCancel>>,
    lease_session: Option<String>,
    binding: Option<ThreadBinding>,
    closing: bool,
    completed: u64,
    rotations: u64,
    last_rotation_reason: Option<String>,
    lease_team: Option<String>,
    /// **An automatic turn of Stu's is running on the live lease** (Claude Code started it
    /// itself: a helper's hand-back, a task notification): begun on the between-turn lane and
    /// not yet ended by its `result`. Nightly 65's whole website job ran as such turns.
    automatic_turn: bool,
    /// The run loop is to check that Stu is on the account in use: `Some(true)` when one of his
    /// automatic turns has ended (the full decision, as before every turn), `Some(false)` at the
    /// idle check ([`ACCOUNT_CHECK_EVERY`]).
    move_check: Option<bool>,
    /// The account Stu was told he is being moved to (`account_switch`), until he has moved.
    switch_told: Option<String>,
}
struct Backend {
    live_delivery: Mutex<()>,
    lease: Mutex<Option<Box<dyn Cognition>>>,
    inner: Mutex<Inner>,
    wake: Condvar,
}
impl Backend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            live_delivery: Mutex::new(()),
            lease: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            wake: Condvar::new(),
        })
    }
}
pub struct WorkHost {
    state: PathBuf,
    factory: Arc<dyn LeaseFactory>,
    backends: Mutex<std::collections::HashMap<String, Arc<Backend>>>,
    closing: Mutex<bool>,
    notifier: Arc<dyn WorkNotifier>,
    seen: Mutex<std::collections::HashSet<String>>,
    screen: Mutex<Arc<dyn crate::screen::ScreenSource>>,
    quota: Mutex<Option<Arc<crate::quota::Service>>>,
    operator: Mutex<Option<Arc<dyn OperatorIntake>>>,
    output: Mutex<Option<crate::output::OutputStore>>,
    retry_clock: Mutex<Arc<dyn crate::upstream::RetryClock>>,
}
impl WorkHost {
    pub fn new(
        state: &Path,
        factory: Box<dyn LeaseFactory>,
        notifier: Arc<dyn WorkNotifier>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: state.into(),
            factory: Arc::from(factory),
            backends: Mutex::new(Default::default()),
            closing: Mutex::new(false),
            notifier,
            seen: Mutex::new(Default::default()),
            screen: Mutex::new(Arc::new(crate::screen::UnknownScreen)),
            quota: Mutex::new(None),
            operator: Mutex::new(None),
            output: Mutex::new(None),
            retry_clock: Mutex::new(Arc::new(crate::upstream::SystemRetryClock)),
        })
    }
    pub fn set_retry_clock(&self, clock: Arc<dyn crate::upstream::RetryClock>) {
        *self.retry_clock.lock().unwrap() = clock;
    }

    fn overload_wait(&self, backend: &Backend, binding: &ThreadBinding, result: &Result<String, String>) -> Option<std::time::Duration> {
        let overloaded = result.as_ref().err()
            .and_then(|error| crate::upstream::UpstreamFailure::classify_lines(error))
            .is_some_and(|failure| failure.fault == crate::upstream::UpstreamFault::Overloaded);
        let mut inner = backend.inner.lock().unwrap();
        if !overloaded {
            if result.is_ok() { inner.overload.succeeded(); }
            return None;
        }
        if !inner.overload.charge(crate::upstream::UpstreamFault::Overloaded) { return None; }
        let wait = inner.overload.wait_before_retry();
        let said = inner.overload.ceo_message();
        drop(inner);
        // The wait is shown as Rich's is: RichOS's own retry sentence, never Stu's words.
        if let Some(said) = said { self.notifier.overload_retry(binding.thread_id(), &said); }
        wait
    }

    fn retry_stopped(&self, backend: &Backend, binding: &ThreadBinding,
        message: &crate::session_tools::Message, session: &str) -> bool {
        let inner = backend.inner.lock().unwrap();
        let ended = inner.closing || inner.lease_session.as_deref() != Some(session);
        drop(inner);
        ended || self.handoff_stopped(binding, message).unwrap_or(true)
    }

    fn live_completion(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        message: crate::session_tools::Message, delivered: (InputReceipt, Option<String>), text: String,
    ) -> Box<dyn FnOnce(crate::steering::InputOutcome) + Send> {
        let host = Arc::downgrade(self);
        let receiving = Arc::downgrade(backend);
        let binding = binding.clone();
        Box::new(move |outcome| {
            let Some(host) = host.upgrade() else { return };
            let Some(backend) = receiving.upgrade() else { return };
            let (input, session) = delivered;
            let input_id=input.input_id();
            let scope = host.message_scope(&binding);
            let succeeded=outcome.result.as_ref().is_ok_and(|reason| reason=="end_turn");
            let receipt = match &input {
                InputReceipt::Peer(through) if succeeded=>crate::session_tools::acknowledge(&scope,"stu",*through),
                InputReceipt::Peer(through)=>crate::session_tools::accept_live_handoff(&scope,"stu",*through),
                InputReceipt::Answer(id) if succeeded=>crate::question_work::taken(&host.state,std::slice::from_ref(id),session.as_deref().unwrap_or(""))
                    .and_then(|_|crate::session_tools::complete_attempt(&scope,"stu",&input_id)),
                InputReceipt::Answer(_)=>Ok(()),
            };
            if let Err(error) = receipt { eprintln!("[richos] native input receipt remains pending: {error}"); }
            if outcome.report_failure {
                if let Some(wait) = host.overload_wait(&backend, &binding, &outcome.result) {
                    // The native reader must keep reading while the existing retry clock waits.
                    let retry_host = Arc::downgrade(&host);
                    let retry_backend = Arc::downgrade(&backend);
                    let retry_binding = binding.clone();
                    let retry_message = message.clone();
                    let retry_session = session.clone();
                    let original_error = outcome.result.clone();
                    let retry_receipt=input.clone();
                    let retry_input_id=input_id.clone();
                    let retry = std::thread::Builder::new().name("richos-stu-overload".into()).spawn(move || {
                        let (Some(host), Some(backend)) = (retry_host.upgrade(), retry_backend.upgrade()) else { return };
                        let owner = retry_session.as_deref().unwrap_or("");
                        let clock = host.retry_clock.lock().unwrap().clone();
                        if !clock.wait(wait, &||host.retry_stopped(&backend, &retry_binding, &retry_message, owner)) { return; }
                        let _delivery = backend.live_delivery.lock().unwrap();
                        if host.retry_stopped(&backend, &retry_binding, &retry_message, owner) { return; }
                        let input = backend.inner.lock().unwrap().peer_stopper.clone()
                            .filter(|(session, _)| session == owner).map(|(_, input)| input);
                        let completed = host.live_completion(&backend, &retry_binding, retry_message.clone(),
                            (retry_receipt, retry_session.clone()), text.clone());
                        let sent = input.map(|input|input.send_input(&uuid::Uuid::new_v4().to_string(), &text, completed));
                        if !matches!(sent, Some(Ok(true))) {
                            if let Err(error) = host.observe_turn_result(&retry_binding, Some(&retry_message),
                                Some(&retry_input_id), owner, &original_error) {
                                eprintln!("[richos] native input status: {error}");
                            }
                        }
                    });
                    if retry.is_ok() { backend.wake.notify_all(); return; }
                }
                if let Err(error) = host.observe_turn_result(&binding, Some(&message),
                    Some(&input_id), session.as_deref().unwrap_or(""), &outcome.result) {
                    eprintln!("[richos] native input status: {error}");
                }
            }
            backend.wake.notify_all();
        })
    }

    /// **A worker stopped by an overload gets the CEO's waits too** (follow-up 11; his order of
    /// 2026-10-07: 1, 2, 5, 10, 20, 40 and 80 minutes). A worker belongs to Claude Code, which
    /// cannot be set to those waits, so this host waits, on the same budget, clock and Stop
    /// checks as Stu's own retry above, and then tells Stu, who restarts the worker (stu.md).
    /// All workers share ONE clock ([`Inner::workers`]): an overload during a running wait joins
    /// it, and when it ends Stu gets one `worker_overload` message per worker it stopped. Any
    /// other answer a worker gets from the API starts the schedule again.
    fn observe_worker(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, session: &str,
        frame: &serde_json::Value) {
        if frame["type"] == "system" && frame["subtype"] == "task_started" && frame["task_type"] == "local_agent" {
            if let (Some(call), Some(name)) = (frame["tool_use_id"].as_str(), frame["description"].as_str().filter(|name| !name.is_empty())) {
                backend.inner.lock().unwrap().worker_names.insert(call.to_string(), name.to_string());
            }
            return;
        }
        if frame["type"] != "assistant" { return; }
        let call = &frame["parent_tool_use_id"];
        let mut inner = backend.inner.lock().unwrap();
        let Some(failure) = crate::native::overloaded_api_error_message(frame) else {
            if frame.get("error").is_none_or(serde_json::Value::is_null) { inner.workers.succeeded(); }
            return;
        };
        let task = call.as_str().and_then(|call| inner.worker_names.get(call).cloned()).map(serde_json::Value::from)
            .or_else(|| frame.get("task_description").cloned());
        let fact = |wait: Option<(u32, std::time::Duration)>| serde_json::json!({"event":"worker_overload",
            "worker":{"tool_use_id":call,"agent_id":frame.get("agent_id"),"task":task},
            "error":failure.raw,"retry":wait.map(|(retry, _)| retry),
            "retries_left":wait.map_or(0, |(retry, _)| crate::upstream::MAX_OVERLOAD_RETRIES - retry),
            "waited_minutes":wait.map(|(_, wait)| wait.as_secs() / 60),
            // The end is the overload granted no retry, never the seventh, whose retry still runs.
            "retries_used_up":wait.is_none()});
        if let Some(running) = inner.worker_wait.as_mut().filter(|running| running.session == session) {
            // The same overload, stopping another worker: it joins the wait, the step stays.
            if !running.facts.iter().any(|told| &told["worker"]["tool_use_id"] == call) {
                let joined = fact(Some((running.retry, running.wait)));
                running.facts.push(joined);
            }
            return;
        }
        let granted = inner.workers.charge(crate::upstream::UpstreamFault::Overloaded);
        let wait = granted.then(|| inner.workers.wait_before_retry()).flatten();
        let retry = inner.workers.retries_spent();
        let told = fact(wait.map(|wait| (retry, wait)));
        let started = match wait {
            Some(wait) => {
                inner.worker_wait = Some(WorkerWait {session: session.to_string(), retry, wait, facts: vec![told]});
                drop(inner);
                let ours = session.to_string();
                // Every worker it stopped, taken when the wait ends; nobody when it was cut short.
                self.tell_stu_after(backend, binding, session, Some(wait), move |backend, elapsed| {
                    let taken = backend.inner.lock().unwrap().worker_wait.take_if(|running| running.session == ours);
                    taken.filter(|_| elapsed).map_or_else(Vec::new, |running| running.facts)
                })
            }
            None => {
                drop(inner);
                self.tell_stu_after(backend, binding, session, None, move |_, _| vec![told])
            }
        };
        if let Err(error) = started {
            backend.inner.lock().unwrap().worker_wait.take_if(|running| running.session == session);
            eprintln!("[richos] a worker's overload wait could not start: {error}");
        }
    }

    /// Off the native reader: wait `wait` on the retry clock unless the session ends first, then
    /// put the facts `told` hands back in Stu's own inbox, one message each, and wake him on the
    /// same session. `told` is asked once either way, with whether the wait elapsed.
    fn tell_stu_after(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, session: &str,
        wait: Option<std::time::Duration>,
        told: impl FnOnce(&Backend, bool) -> Vec<serde_json::Value> + Send + 'static) -> std::io::Result<()> {
        let (host, receiving, binding, session) = (Arc::downgrade(self), Arc::downgrade(backend), binding.clone(), session.to_string());
        std::thread::Builder::new().name("richos-overload-retry".into()).spawn(move || {
            let (Some(host), Some(backend)) = (host.upgrade(), receiving.upgrade()) else { return };
            let elapsed = wait.is_none_or(|wait| {
                let ended = || { let inner = backend.inner.lock().unwrap(); inner.closing || inner.lease_session.as_deref() != Some(session.as_str()) };
                let clock = host.retry_clock.lock().unwrap().clone();
                clock.wait(wait, &ended)
            });
            let facts = told(&backend, elapsed);
            if facts.is_empty() { return; }
            for fact in &facts {
                if let Err(error) = crate::session_tools::observe_overload_retry(&host.message_scope(&binding), fact) {
                    eprintln!("[richos] an overload could not be passed to Stu: {error}");
                    return;
                }
            }
            host.wake_messages(&binding);
        }).map(|_| ())
    }

    /// **An automatic turn of Stu's that Claude Code gave up on with an overload waits the same
    /// schedule as his other turns** (review rv-20261018T200657Z-585c24df-f9d6, finding 3).
    /// Claude Code starts these turns itself, after a helper hands back or a task notification,
    /// so no input of ours can be sent again: after the wait RichOS wakes Stu through his own
    /// inbox to carry on. When the schedule grants no retry, the failure reaches Rich as before.
    fn observe_automatic_turn(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, session: &str,
        result: &Result<String, String>) {
        if let Some(wait) = self.overload_wait(backend, binding, result) {
            let fact = serde_json::json!({"event":"turn_overload","error":result.as_ref().err(),"waited_minutes":wait.as_secs()/60});
            match self.tell_stu_after(backend, binding, session, Some(wait), move |_, elapsed| if elapsed { vec![fact] } else { vec![] }) {
                Ok(()) => return,
                Err(error) => eprintln!("[richos] an automatic turn's overload wait could not start: {error}"),
            }
        }
        if let Err(error) = self.observe_turn_result(binding, None, None, session, result) {
            eprintln!("[richos] native turn status: {error}");
        }
    }

    fn project_output(&self, binding: &ThreadBinding, session: &str) {
        let Some(store) = self.output.lock().unwrap().clone() else {
            return;
        };
        let join = crate::output::SessionJoin::from_parts(
            Vec::new(),
            assignment::read_all(
                &self.state,
                binding.entity_id().as_str(),
                binding.thread_id(),
            )
            .unwrap_or_default()
            .iter()
            .filter(|record| record.work_session.as_deref() == Some(session))
            .map(|record| crate::output::BackendWork::for_assignment(record, session))
            .collect(),
        );
        if let Err(e) = store.project_session(
            binding.thread_id(),
            &self.state.join("evidence"),
            session,
            &join,
        ) {
            eprintln!("[richos] output: the back end's written files could not be recorded: {e}");
        }
    }
    pub fn signed_in(&self) {
        for backend in self.backends.lock().unwrap().values() {
            backend.wake.notify_all();
        }
    }
    pub fn set_output_store(&self, store: crate::output::OutputStore) {
        *self.output.lock().unwrap() = Some(store);
    }
    pub fn set_quota(&self, quota: Arc<crate::quota::Service>) {
        *self.quota.lock().unwrap() = Some(quota);
    }

    pub fn set_operator(&self, intake: Arc<dyn OperatorIntake>) {
        *self.operator.lock().unwrap() = Some(intake);
    }

    pub fn set_screen(&self, screen: Arc<dyn crate::screen::ScreenSource>) {
        *self.screen.lock().unwrap() = screen;
    }

    pub fn screen_reading(&self) -> crate::screen::ScreenReading {
        self.screen.lock().unwrap().read()
    }

    pub fn remember_binding(self: &Arc<Self>, binding: &ThreadBinding) {
        if let Some(backend) = self.backend_for(binding.thread_id()) {
            backend.inner.lock().unwrap().binding = Some(binding.clone());
        }
    }

    pub fn wake_messages(self: &Arc<Self>, binding: &ThreadBinding) {
        let Some(backend) = self.backend_for(binding.thread_id()) else {
            return;
        };
        let _delivery = backend.live_delivery.lock().unwrap();
        let (input, after, session) = {
            let mut inner = backend.inner.lock().unwrap();
            inner.binding = Some(binding.clone());
            if inner.closing {
                return;
            }
            (
                inner.peer_input.clone(),
                inner.peer_active_through.unwrap_or(0),
                inner.lease_session.clone(),
            )
        };
        backend.wake.notify_all();
        let Some(input) = input else { return };
        let scope = self.message_scope(binding);
        let result = (|| {
            if let Some(session)=session.as_deref() {
                for answer in crate::question_work::pending(&self.state)?.into_iter().filter(|answer|
                    !answer.asker.starts_with("operator:") && answer.entity_id==binding.entity_id().as_str() && answer.thread_id==binding.thread_id()) {
                    let input_id=format!("answer:{}",answer.id);
                    if crate::session_tools::previous_attempt(&scope,"stu",&input_id)?.as_ref()
                        .and_then(|attempt|attempt["session_id"].as_str())==Some(session) {continue}
                    let instruction=self.question_answer_instruction(&answer)?;
                    let message=crate::session_tools::Message {handoff:false,from:"user".into(),text:answer.text.clone(),instruction,work:None};
                    let text=serde_json::to_string(&message).map_err(|e|e.to_string())?;
                    use sha2::Digest;
                    let digest=sha2::Sha256::digest(format!("{}\0{}\0{input_id}",scope.entity_id,scope.thread_id));
                    let id=uuid::Uuid::from_slice(&digest[..16]).unwrap().to_string();
                    crate::session_tools::begin_attempt(&scope,"stu",&input_id,session)?;
                    let completed=self.live_completion(&backend,binding,message.clone(),
                        (InputReceipt::Answer(answer.id.clone()),Some(session.to_string())),text.clone());
                    match input.send_input(&id,&text,completed) {
                        Ok(true)=>{},
                        Ok(false)=>{crate::session_tools::complete_attempt(&scope,"stu",&input_id)?;return Ok(())},
                        Err(error)=>{
                            self.observe_turn_result(binding,Some(&message),Some(&input_id),session,&Err(error.clone()))?;
                            return Err(error);
                        }
                    }
                }
            }
            let mut after = after;
            loop {
            let (mut message, through) = loop {
                let Some((message, through)) =
                    crate::session_tools::pending_after(&scope, "stu", after)?
                else {
                    return Ok::<_, String>(());
                };
                if !self.handoff_stopped(binding, &message)? {
                    break (message, through);
                }
                crate::session_tools::acknowledge(&scope, "stu", through)?;
                after = through;
            };
            if message.handoff {
                message.work = crate::session_tools::work_reference(&scope, &message)?;
            }
            use sha2::Digest;
            let digest = sha2::Sha256::digest(format!(
                "{}\0{}\0{through}",
                scope.entity_id, scope.thread_id
            ));
            let id = uuid::Uuid::from_slice(&digest[..16]).unwrap().to_string();
            let mut payload = serde_json::to_value(&message).map_err(|e| e.to_string())?;
            payload["previous_delivery_attempt"] = serde_json::to_value(
                crate::session_tools::previous_attempt(&scope, "stu", &format!("peer:{through}"))?,
            )
            .map_err(|e| e.to_string())?;
            let text = payload.to_string();
            if let Some(session) = &session {
                crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),session)?;
            }
            let completed = self.live_completion(&backend, binding, message.clone(),
                (InputReceipt::Peer(through), session.clone()), text.clone());
            let delivered = match input.send_input(&id, &text, completed) {
                Ok(delivered)=>delivered,
                Err(error)=>{
                    self.observe_turn_result(binding,Some(&message),Some(&format!("peer:{through}")),
                        session.as_deref().unwrap_or(""),&Err(error.clone()))?;
                    return Err(error);
                }
            };
            if !delivered { return Ok(()); }
            if delivered {
                let mut inner = backend.inner.lock().unwrap();
                if inner.lease_session == session {
                    inner.peer_active_through=Some(inner.peer_active_through.unwrap_or(0).max(through));
                }
            }
            after = through;
            }
        })();
        if let Err(error) = result {
            eprintln!("[richos] operator live input remains pending: {error}");
        }
    }

    fn observe_turn_result(&self, binding: &ThreadBinding, message: Option<&crate::session_tools::Message>,
        input: Option<&str>, session: &str, result: &Result<String,String>) -> Result<(),String> {
        if result.as_ref().is_ok_and(|reason| matches!(reason.as_str(), "end_turn" | crate::native::STOP_REASON_CANCELLED | crate::native::STOP_REASON_CANCEL_UNACKNOWLEDGED))
            || *self.closing.lock().unwrap() || match message {Some(message)=>self.handoff_stopped(binding,message)?,None=>false} { return Ok(()); }
        crate::session_tools::observe_work_status(&self.message_scope(binding), serde_json::json!({
            "event":"upstream_turn","input_id":input,"session_id":session,"work":message.and_then(|message|message.work.as_ref()),
            "stop_reason":result.as_ref().ok(),"error":result.as_ref().err()
        }))
    }

    fn publish_peer_input(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.peer_input = inner.cancel.clone();
        }
        self.wake_messages(binding);
    }

    fn message_scope(&self, binding: &ThreadBinding) -> crate::status_tools::StatusToolScope {
        crate::status_tools::StatusToolScope {
            version: 1,
            state_root: self.state.clone(),
            entity_id: binding.entity_id().to_string(),
            thread_id: binding.thread_id().to_string(),
        }
    }

    fn refresh_peer_team(
        &self,
        backend: &Arc<Backend>,
        lease: &mut dyn Cognition,
    ) -> Result<bool, String> {
        let team = self.factory.team_digest();
        if team.is_none() || team == backend.inner.lock().unwrap().lease_team {
            return Ok(false);
        }
        let changed = lease.refresh_team().map_err(|e| e.to_string())?;
        backend.inner.lock().unwrap().lease_team = team;
        Ok(changed)
    }

    fn handoff_stopped(
        &self,
        binding: &ThreadBinding,
        message: &crate::session_tools::Message,
    ) -> Result<bool, String> {
        let Some(instruction) = message.instruction.as_ref().filter(|_| message.handoff) else {
            return Ok(false);
        };
        Ok(assignment::read_all(
            &self.state,
            binding.entity_id().as_str(),
            binding.thread_id(),
        )
        .map_err(|e| e.to_string())?
        .iter()
        .any(|record| {
            record.instruction_ledger_ref == instruction.ledger_ref
                && record.instruction_sha256 == instruction.sha256
                && record.user_stop_requested_at_ms.is_some()
        }))
    }

    fn run_message(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        binding: &ThreadBinding,
        message: &crate::session_tools::Message,
        turn: &str,
    ) -> Result<String, String> {
        let result=(|| {
        // This is queue cancellation, not per-tool authority on Stu's session.
        let stopped = || self.handoff_stopped(binding, message);
        self.move_to_account_in_use(backend, binding, true);
        let _ = self.renew_for_user_settings(backend, binding);
        let input = serde_json::to_string(message).map_err(|e| e.to_string())?;
        {
            self.ensure_lease(backend, binding)?;
            let mut guard = backend.lease.lock().unwrap();
            let lease = guard.as_mut().ok_or("The operator connection closed")?;
            let session = lease.session_id().to_string();
            if stopped()? {
                return Ok(session);
            }
            self.refresh_peer_team(backend, lease.as_mut())?;
            lease
                .prepare_agent_message(binding, turn, message)
                .map_err(|e| e.to_string())?;
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.cancel = lease.cancel_handle();
                inner.peer_stopper = inner
                    .cancel
                    .clone()
                    .map(|handle| (lease.session_id().to_string(), handle));
                if let Some(through)=turn.strip_prefix("peer:").and_then(|n| n.parse::<u64>().ok()) {
                    inner.peer_active_through=Some(inner.peer_active_through.unwrap_or(0).max(through));
                }
            }
            crate::session_tools::begin_attempt(
                &self.message_scope(binding),
                "stu",
                turn,
                &session,
            )?;
            // Stop may have arrived during connection/scope preparation, before a
            // native stop handle existed. Never deliver that saved handoff afterward.
            if stopped()? {
                return Ok(session);
            }
            let mut published = false;
            let result = loop {
                let mut said = String::new();
                let mut sent=lease.enqueue_prompt(&input,Box::new(||Ok(()))).and_then(|reply| {
                    if !published {
                        published=true;
                        self.publish_peer_input(backend,binding);
                    }
                    reply.wait(&mut |item| {
                        if let TurnItem::Text {text,..}=item {
                            if said.len()<64*1024 {said.push_str(text);}
                        }
                    })
                });
                if let Some(failure) = crate::upstream::UpstreamFailure::classify_lines(&said)
                    .filter(|failure|failure.fault == crate::upstream::UpstreamFault::Overloaded) {
                    sent = Err(CognitionError::Protocol(failure.raw));
                }
                let observed = sent.as_ref().map(Clone::clone).map_err(ToString::to_string);
                let Some(wait) = self.overload_wait(backend, binding, &observed) else { break sent };
                let clock = self.retry_clock.lock().unwrap().clone();
                if !clock.wait(wait, &||self.retry_stopped(backend, binding, message, &session)) { break sent; }
            };
            {
                let _delivery=backend.live_delivery.lock().unwrap();
                let mut inner=backend.inner.lock().unwrap();
                inner.peer_input=None;
                inner.cancel=None;
                // Input of ours sent during an automatic turn is folded into it, and that turn's
                // result is then ours: it has ended with this one.
                inner.automatic_turn=false;
            }
            if matches!(&result, Err(CognitionError::Io(_))) {
                drop(guard.take());
                self.note_back_end(binding, None);
                backend.inner.lock().unwrap().peer_active_through=None;
            }
            let reason = result.map_err(|e| e.to_string())?;
            if reason != "end_turn" {
                return Err(format!("Operator input did not complete: {reason}"));
            }
            let lease = guard
                .as_mut()
                .ok_or("The operator connection closed during team reload")?;
            self.project_output(binding, &session);
            self.refresh_peer_team(backend, lease.as_mut())?;
            Ok(session)
        }
        })();
        if result.is_err() {
            let session=backend.inner.lock().unwrap().lease_session.clone().unwrap_or_default();
            self.observe_turn_result(binding,Some(message),Some(turn),&session,&result)?;
        }
        result

    }

    pub fn adopt_registered(self: &Arc<Self>, binding: &ThreadBinding) -> usize {
        self.wake_messages(binding);
        // Desktop work is dispatched only through Stu's durable peer input.
        // The separate operator client retains its explicit assignment intake.
        if self.operator.lock().unwrap().is_none() {
            return 0;
        }
        let Ok(rows) = assignment::read_all(
            &self.state,
            &binding.entity_id().to_string(),
            binding.thread_id(),
        ) else {
            return 0;
        };
        let mut adopted = 0;
        for record in rows
            .into_iter()
            .filter(|row| row.state == AssignmentState::Registered)
        {
            if self.seen.lock().unwrap().contains(&record.id) {
                continue;
            }
            if let Some(operator) = self.operator.lock().unwrap().clone() {
                operator.take(record);
                adopted += 1;
            }
        }
        adopted
    }

    fn backend_for(self: &Arc<Self>, thread: &str) -> Option<Arc<Backend>> {
        if *self.closing.lock().unwrap() {
            return None;
        }
        let mut backends = self.backends.lock().unwrap();
        if let Some(existing) = backends.get(thread) {
            return Some(Arc::clone(existing));
        }
        let backend = Backend::new();
        backends.insert(thread.to_string(), Arc::clone(&backend));
        drop(backends);
        let host = Arc::clone(self);
        let runner = Arc::clone(&backend);
        std::thread::Builder::new()
            // Named per conversation, so a stack from a stuck back end says which one.
            .name(format!(
                "richos-back-end:{}",
                &thread[..thread.len().min(24)]
            ))
            .spawn(move || host.run(runner))
            .expect("a back end could not be started");
        Some(backend)
    }

    pub fn start(self: &Arc<Self>) {
        *self.closing.lock().unwrap() = false;
    }

    pub fn deliver_team_answers(
        self: &Arc<Self>,
        binding_for: &dyn Fn(&str) -> Option<ThreadBinding>,
        lead: Option<LeadSink<'_>>,
    ) {
        for delivery in crate::question_work::pending(&self.state).unwrap_or_default() {
            if delivery.asker.starts_with("operator:") {
                continue;
            }
            if let Some(binding) = binding_for(&delivery.thread_id) {
                drop(self.queue_question_answer(&binding, &delivery));
            }
        }
        let store = crate::questions::Store::new(&self.state);
        for (entity, thread, asker) in store.pending_threads().unwrap_or_default() {
            if asker == "front_desk" {
                continue;
            }
            let Some(binding) = binding_for(&thread) else {
                continue;
            };
            if let Err(error) = store.deliver(&entity, &thread, &asker, |d| {
                if d.asker.starts_with("operator:") {
                    lead.ok_or("Your team is unavailable")?(d)
                } else {
                    self.queue_question_answer(&binding, d)
                }
            }) {
                eprintln!("[richos] team answer remains saved: {error}");
            }
        }
    }

    fn question_answer_instruction(
        &self,
        delivery: &crate::questions::Delivery,
    ) -> Result<Option<crate::ecs::UserInstruction>, String> {
        let Some(owner) = delivery.obligation_id.as_deref() else {
            return Ok(None);
        };
        let record = assignment::read_all(&self.state, &delivery.entity_id, &delivery.thread_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|record| record.obligation_id == owner)
            .ok_or("The asking assignment is unavailable")?;
        if !record.accepts_input() {
            crate::question_work::discard(
                &self.state,
                &delivery.entity_id,
                &delivery.thread_id,
                owner,
            )?;
            return Err("The asking assignment has stopped".into());
        }
        let text = verified_instruction(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.instruction_ledger_ref,
            &record.instruction_sha256,
        )
        .ok()
        .filter(|text| text.len() <= crate::ecs::INSTRUCTION_TEXT_LIMIT);
        Ok(Some(crate::ecs::UserInstruction {
            ledger_ref: record.instruction_ledger_ref,
            sha256: record.instruction_sha256,
            text,
        }))
    }

    fn observe_lease(self: &Arc<Self>, backend: &Arc<Backend>, lease: &mut dyn Cognition, binding: &ThreadBinding) {
        let host = Arc::downgrade(self);
        let receiving = Arc::downgrade(backend);
        let binding = binding.clone();
        let session = lease.session_id().to_string();
        // An automatic turn (a helper's hand-back, a task notification) reaches this sink whole,
        // no input of ours being parked: its last message says whether Claude Code gave up on an
        // overload before the turn's ordinary end_turn, as a live input's sink reads it.
        let said: Mutex<Option<String>> = Mutex::new(None);
        lease.set_between_turn_sink(Arc::new(move |records| {
            let Some(host) = host.upgrade() else { return };
            for frame in records {
                if frame.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null()) {
                    if let Some(backend) = receiving.upgrade() { host.observe_worker(&backend, &binding, &session, &frame); }
                    continue;
                }
                // A worker's start names its task, told to Stu with its overload.
                if frame["type"] == "system" && frame["subtype"] == "task_started" {
                    if let Some(backend) = receiving.upgrade() { host.observe_worker(&backend, &binding, &session, &frame); }
                    continue;
                }
                // Stu's own frames between turns of ours are an automatic turn under way; a task
                // notification starts one.
                if matches!(frame["type"].as_str(), Some("assistant" | "user" | "stream_event"))
                    || (frame["type"] == "system" && frame["subtype"] == "task_notification") {
                    if let Some(backend) = receiving.upgrade() { host.note_automatic_turn(&backend, &session, true); }
                }
                if frame["type"] == "assistant" {
                    *said.lock().unwrap() = crate::native::overloaded_api_error_message(&frame).map(|failure| failure.raw);
                    continue;
                }
                if frame["type"] != "result" { continue }
                let overload = said.lock().unwrap().take();
                let result = crate::native::native_result(&frame).map_err(|error| error.to_string())
                    .and_then(|reason| match overload {
                        Some(raw) if reason == "end_turn" => Err(raw),
                        _ => Ok(reason),
                    });
                match receiving.upgrade() {
                    Some(backend) => {
                        host.observe_automatic_turn(&backend, &binding, &session, &result);
                        host.note_automatic_turn(&backend, &session, false);
                    }
                    None => if let Err(error) = host.observe_turn_result(&binding,None,None,&session,&result) {
                        eprintln!("[richos] native turn status: {error}");
                    },
                }
            }
        }));
    }

    fn ensure_lease(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        binding: &ThreadBinding,
    ) -> Result<(), String> {
        let mut lease = backend.lease.lock().unwrap();
        if lease.as_ref().is_some_and(|lease|lease.is_closed()) {drop(lease.take());self.note_back_end(binding,None);}
        if lease.is_some() {
            return Ok(());
        }
        // The team it registers, read just before it is spawned (see `Inner::lease_team`).
        let (team, opened) = {
            let factory = &self.factory;
            (factory.team_digest(), factory.spawn_work(binding))
        };
        let mut opened = opened.map_err(|e| e.to_string())?;
        self.observe_lease(backend, opened.as_mut(), binding);
        // A provider opened after shutdown started is never installed.
        let closing = {
            let mut inner = backend.inner.lock().unwrap();
            if !inner.closing {
                inner.automatic_turn = false;
                inner.lease_session = Some(opened.session_id().to_string());
                inner.peer_stopper=opened.cancel_handle().map(|handle|(opened.session_id().to_string(),handle));
                inner.lease_team = team;
            }
            inner.closing
        };
        if closing {
            drop(lease);
            drop(opened);
            return Err("RichOS is closing down. Nothing was started.".into());
        }
        let account = opened.account().map(str::to_string);
        *lease = Some(opened);
        drop(lease);
        self.note_back_end(binding, account.as_deref());
        Ok(())
    }

    /// **Where this conversation's back end runs now** (`None`: it has no live lease), for the
    /// switch line: "I switched the team" is said only once Stu is on the new account too
    /// (`claude_accounts::Accounts::back_end_on`).
    fn note_back_end(&self, binding: &ThreadBinding, account: Option<&str>) {
        if let Some(quota) = self.quota.lock().unwrap().clone() {
            quota.accounts.back_end_on(binding.thread_id(), account);
        }
    }

    /// **An automatic turn of Stu's began (`running`) or ended**, on the lease of `session`. Its
    /// end is a boundary of his work: the run loop checks there that he is on the account in use.
    /// Its start is checked too, so a switch already decided reaches him while the turn runs
    /// rather than after it (the idle check covers one decided during the turn).
    fn note_automatic_turn(&self, backend: &Backend, session: &str, running: bool) {
        let mut inner = backend.inner.lock().unwrap();
        if inner.lease_session.as_deref() != Some(session) { return; }
        let began = running && !inner.automatic_turn;
        inner.automatic_turn = running;
        if !running {
            inner.move_check = Some(true);
            backend.wake.notify_all();
        } else if began {
            inner.move_check.get_or_insert(false);
            backend.wake.notify_all();
        }
    }

    /// **Stu moves to the account in use at the next boundary of his work** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 2; nightly 65: his whole website job ran as
    /// automatic turns on Work after the switch to Home, because he was moved only before a
    /// message the app sent him). Called before every message the app sends him, when one of his
    /// automatic turns ends (`at_turn_end`: the full decision, as before every turn) and at the
    /// idle check.
    ///
    /// When nothing of his is running, his session ends and the normal opener resumes it on the
    /// account in use. When something is (a turn, a command, a helper), ending it would end that
    /// work, so he is told once (`account_switch`) and moves at a later boundary; every time he
    /// has moved, he is told so (`account_moved`) on the new session. What each
    /// message asks of him is in his instructions (`stu.md`), never here.
    fn move_to_account_in_use(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, at_turn_end: bool) {
        let due = if at_turn_end { self.account_switch_due(backend) } else { self.account_left_behind(backend) };
        if !due { return; }
        let outcome = self.renew_lease(backend, binding);
        let Some(quota) = self.quota.lock().unwrap().clone() else { return };
        let label = |id: &Option<String>| id.as_deref().and_then(|id| quota.accounts.list().into_iter().find(|a| a.id == id)).map(|a| a.label);
        let fact = match outcome {
            Renewal::Waits { from } => {
                let to = quota.accounts.in_use();
                {
                    let mut inner = backend.inner.lock().unwrap();
                    if inner.switch_told.as_deref() == Some(to.id.as_str()) { return; }
                    inner.switch_told = Some(to.id.clone());
                }
                serde_json::json!({"event":"account_switch","from":label(&from),"to":to.label})
            }
            // Told or not, his new session has nothing else telling it to carry on (review
            // rv-20261019T030516Z-0a1a8728-e798, finding 1).
            Renewal::Moved { from, to } => {
                backend.inner.lock().unwrap().switch_told = None;
                serde_json::json!({"event":"account_moved","from":label(&from),"to":label(&to)})
            }
            Renewal::NotDue | Renewal::Failed => return,
        };
        if let Err(error) = crate::session_tools::observe_account_switch(&self.message_scope(binding), &fact) {
            eprintln!("[richos] the account switch could not be passed to Stu: {error}");
        }
    }

    /// **The user's own settings files changed since Stu's session read them** (build plan
    /// item 0): his session ends and the normal opener resumes it under the current settings,
    /// unless something of his still runs ([`Renewal::Waits`]), when the next message asks again.
    fn renew_for_user_settings(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Renewal {
        let changed = backend.lease.lock().unwrap().as_ref()
            .is_some_and(|lease| crate::cognition::user_settings_changed(Some(self.factory.as_ref()), lease.as_ref()));
        if !changed { return Renewal::NotDue; }
        self.renew_lease(backend, binding)
    }

    /// The renewal for a switch, decided on the lease's own freshest reading. Kept for the
    /// tests that drive the boundary rule itself.
    #[cfg(test)]
    fn renew_if_account_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Renewal {
        if !self.account_switch_due(backend) {
            return Renewal::NotDue;
        }
        self.renew_lease(backend, binding)
    }

    /// End Stu's session and resume it on the account in use, unless something of his still
    /// runs on it ([`Renewal::Waits`]).
    fn renew_lease(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Renewal {
        // Finish the previous process before the normal opener resumes its transcript.
        // Both processes have the same native session ID and must never overlap.
        let from = {
            let mut lease = backend.lease.lock().unwrap();
            let Some(current) = lease.as_ref() else { return Renewal::NotDue };
            let from = current.account().map(str::to_string);
            // A running command or helper would end with the process, and so would a turn.
            let busy = crate::cognition::account_resume_blocked(current.as_ref())
                || current.worker_status().is_some_and(|workers| workers.active > 0)
                || backend.inner.lock().unwrap().automatic_turn;
            if busy {
                return Renewal::Waits { from };
            }
            let mut inner = backend.inner.lock().unwrap();
            inner.cancel = None;
            inner.peer_input = None;
            inner.peer_stopper = None;
            inner.peer_active_through = None;
            drop(inner);
            drop(lease.take());
            from
        };
        self.note_back_end(binding, None);
        if let Err(why) = self.ensure_lease(backend, binding) {
            eprintln!("[richos] back end: not moved to the next Claude account ({why})");
            return Renewal::Failed;
        }
        let to = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.account().map(str::to_string));
        Renewal::Moved { from, to }
    }

    pub fn set_model(&self,model:crate::config::ModelChoice)->Result<(),String> {
        let backends:Vec<_>=self.backends.lock().unwrap().values().cloned().collect();
        for backend in backends {Self::set_backend_model(&backend,model)?;}
        Ok(())
    }
    fn set_backend_model(backend:&Backend,model:crate::config::ModelChoice)->Result<(),String> {
        let handle={let inner=backend.inner.lock().unwrap();inner.cancel.clone().or_else(||inner.peer_stopper.as_ref()
            .filter(|(session,_)|inner.lease_session.as_ref()==Some(session)).map(|(_,handle)|handle.clone()))};
        if let Some(handle)=handle {handle.set_model(model)?;}
        Ok(())
    }
    /// Is Stu's lease on another account than the one in use? Two ids, no decision: the idle
    /// check's question ([`ACCOUNT_CHECK_EVERY`]).
    fn account_left_behind(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota)=self.quota.lock().unwrap().clone() else {return false};
        let lease=backend.lease.lock().unwrap();
        lease.as_ref().and_then(|lease|lease.account().map(str::to_string))
            .is_some_and(|account|quota.accounts.in_use().id!=account)
    }
    fn account_switch_due(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota)=self.quota.lock().unwrap().clone() else {return false};
        let lease=backend.lease.lock().unwrap();
        let Some(lease)=lease.as_ref() else {return false};
        let Some(account)=lease.account() else {return false};
        quota.before_turn(account,lease.streamed_usage())!=account
    }

    pub fn rotations(&self, thread: &str) -> (u64, Option<String>) {
        match self.backend(thread) {
            None => (0, None),
            Some(backend) => {
                let inner = backend.inner.lock().unwrap();
                (inner.rotations, inner.last_rotation_reason.clone())
            }
        }
    }

    pub fn open_assignments(&self) -> Result<Vec<Assignment>, String> {
        assignment::open(&self.state).map_err(|e| e.to_string())
    }

    pub fn peer_work_pending(&self) -> bool {
        let backends: Vec<_> = self.backends.lock().unwrap().values().cloned().collect();
        backends.into_iter().any(|backend| {
            let inner = backend.inner.lock().unwrap();
            if inner.processing {
                return true;
            }
            inner.binding.as_ref().is_some_and(|binding| {
                let scope = self.message_scope(binding);
                ["rich", "stu"].iter().any(|role| {
                    crate::session_tools::pending(&scope, role)
                        .map(|m| m.is_some())
                        .unwrap_or(true)
                })
            })
        })
    }

    fn backend(&self, thread: &str) -> Option<Arc<Backend>> {
        self.backends.lock().unwrap().get(thread).map(Arc::clone)
    }

    pub fn lease_sessions(&self) -> Vec<String> {
        self.backends
            .lock()
            .unwrap()
            .values()
            .filter_map(|backend| backend.inner.lock().unwrap().lease_session.clone())
            .collect()
    }

    pub fn wait_for_completed(&self, count: u64, limit: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + limit;
        loop {
            // Summed across back ends, because the unit of progress a test cares about is
            // "assignments finished", and they may be finishing on two conversations at once.
            let done: u64 = self
                .backends
                .lock()
                .unwrap()
                .values()
                .map(|backend| backend.inner.lock().unwrap().completed)
                .sum();
            if done >= count {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    fn run(self: Arc<Self>, backend: Arc<Backend>) {
        enum Next {
            Message(ThreadBinding, crate::session_tools::Message, u64),
            Answer(ThreadBinding, crate::questions::Delivery),
            /// A boundary of Stu's work, or the idle check: is he on the account in use?
            Move(ThreadBinding, bool),
        }
        loop {
            let live_session = backend.lease.lock().unwrap().as_ref().map(|lease| lease.session_id().to_string());
            let next = {
                let mut inner = backend.inner.lock().unwrap();
                loop {
                    if inner.closing {
                        return;
                    }
                    // During an automatic turn of Stu's too (review
                    // rv-20261019T030516Z-0a1a8728-e798, finding 1): he is told there that the
                    // switch is coming, and the renewal never ends a running turn.
                    if let (Some(binding), Some(at_turn_end)) = (inner.binding.clone(), inner.move_check) {
                        inner.move_check = None;
                        inner.processing = true;
                        break Next::Move(binding, at_turn_end);
                    }
                    if let Some(binding) = inner.binding.clone() {
                        if let Some(answer) = crate::question_work::pending(&self.state)
                            .unwrap_or_default()
                            .into_iter()
                            .find(|d| {
                                !d.asker.starts_with("operator:")
                                    && d.entity_id == binding.entity_id().as_str()
                                    && d.thread_id == binding.thread_id()
                                    && live_session.as_deref().is_none_or(|session| {
                                        match crate::session_tools::previous_attempt(&self.message_scope(&binding), "stu", &format!("answer:{}", d.id)) {
                                            Ok(attempt) => attempt.as_ref().and_then(|a| a["session_id"].as_str()) != Some(session),
                                            Err(error) => { eprintln!("[richos] answer receipt remains unreadable: {error}"); false }
                                        }
                                    })
                            })
                        {
                            inner.processing = true;
                            break Next::Answer(binding, answer);
                        }
                        match crate::session_tools::pending_after(&self.message_scope(&binding), "stu",inner.peer_active_through.unwrap_or(0)) {
                            Ok(Some((message, through))) => {
                                inner.processing = true;
                                break Next::Message(binding, message, through);
                            }
                            Ok(None) => {}
                            Err(error) => eprintln!("[richos] operator inbox: {error}"),
                        }
                    }
                    let (woken, waited) = backend.wake.wait_timeout(inner, ACCOUNT_CHECK_EVERY).unwrap();
                    inner = woken;
                    if waited.timed_out() && inner.move_check.is_none() {
                        inner.move_check = Some(false);
                    }
                }
            };
            let result = match next {
                Next::Move(binding, at_turn_end) => {
                    self.move_to_account_in_use(&backend, &binding, at_turn_end);
                    let mut inner = backend.inner.lock().unwrap();
                    inner.processing = false;
                    backend.wake.notify_all();
                    continue;
                }
                Next::Message(binding, message, through) => self
                    .run_message(&backend, &binding, &message, &format!("peer:{through}"))
                    .and_then(|_| {
                        crate::session_tools::acknowledge(
                            &self.message_scope(&binding),
                            "stu",
                            through,
                        )
                    }),
                Next::Answer(binding, delivery) => self
                    .question_answer_instruction(&delivery)
                    .and_then(|instruction| {
                        let message = crate::session_tools::Message {
                            handoff: false,
                            from: "user".into(),
                            text: delivery.text.clone(),
                            instruction,
                            work: None,
                        };
                        self.run_message(
                            &backend,
                            &binding,
                            &message,
                            &format!("answer:{}", delivery.id),
                        )
                    })
                    .and_then(|session| {
                        crate::question_work::taken(
                            &self.state,
                            std::slice::from_ref(&delivery.id),
                            &session,
                        )?;
                        crate::session_tools::complete_attempt(
                            &self.message_scope(&binding),
                            "stu",
                            &format!("answer:{}", delivery.id),
                        )
                    }),
            };
            let mut inner = backend.inner.lock().unwrap();
            inner.peer_input = None;
            inner.processing = false;
            inner.completed += 1;
            // The end of this turn is a boundary of Stu's work too.
            inner.move_check.get_or_insert(false);
            backend.wake.notify_all();
            if let Err(error) = result {
                eprintln!("[richos] operator input remains saved: {error}");
                if inner.closing {
                    return;
                }
                drop(backend.wake.wait(inner).unwrap());
            } else {
                drop(inner);
                if self.open_assignments().is_ok_and(|open| open.is_empty()) {
                    self.notifier.nothing_left_to_do();
                }
            }
        }
    }

    /// An answer is input to Stu, never an app instruction to execute a job again.
    pub fn queue_question_answer(
        self: &Arc<Self>,
        binding: &ThreadBinding,
        delivery: &crate::questions::Delivery,
    ) -> Result<String, String> {
        if delivery.entity_id != binding.entity_id().as_str()
            || delivery.thread_id != binding.thread_id()
        {
            return Err("The answer belongs to another conversation".into());
        }
        self.question_answer_instruction(delivery)?;
        crate::question_work::enqueue(&self.state, delivery)?;
        let backend = self
            .backend_for(binding.thread_id())
            .ok_or("RichOS is closing")?;
        backend.inner.lock().unwrap().binding = Some(binding.clone());
        self.wake_messages(binding);
        Ok(format!("work-input:{}", delivery.id))
    }

    /// Job execution status is read from Stu's existing reports, never maintained here.
    pub fn background_work(&self) -> crate::work_gate::BackgroundWork {
        match self.open_assignments() {
            Ok(open) => {
                if self.operator.lock().unwrap().is_some() {
                    return crate::work_gate::BackgroundWork::nothing();
                }
                let awaiting_you = open
                    .iter()
                    .filter(|row| row.reported_state() == Some(AssignmentState::Blocked))
                    .count();
                crate::work_gate::BackgroundWork {
                    running: open.len() - awaiting_you,
                    awaiting_you,
                    readable: true,
                }
            }
            Err(_) => crate::work_gate::BackgroundWork {
                running: 0,
                awaiting_you: 0,
                readable: false,
            },
        }
    }

    /// Stop one user's job. Persist its provenance before waiting for dispatch or provider I/O.
    /// Stu remains alive to handle its other jobs and his own native background commands.
    pub fn stop_assignment(
        self: &Arc<Self>,
        entity: &str,
        thread: &str,
        id: &str,
    ) -> Result<(), String> {
        self.stop_assignment_and_front_desk(entity, thread, id, None)
    }

    /// The same Stop, also reaching the conversation's front desk (`ConversationControl::
    /// front_desk`): the shell calls Rich made in the turn that gave this job are this job's
    /// commands too, by the instruction his hook evidence stamped on each.
    pub fn stop_assignment_and_front_desk(
        self: &Arc<Self>,
        entity: &str,
        thread: &str,
        id: &str,
        front_desk: Option<(String, Arc<dyn TurnCancel>)>,
    ) -> Result<(), String> {
        let record =
            assignment::read(&self.state, entity, thread, id).map_err(|e| e.to_string())?;
        if !record.accepts_input() {
            return Err("That assignment has already stopped.".into());
        }
        let stopped = assignment::record_user_stop(&self.state, entity, thread, id)
            .map_err(|e| e.to_string())?;
        if let Some(operator) = self.operator.lock().unwrap().clone() {
            return operator.stop(entity, thread, id);
        }
        let stopper = self
            .backend(thread)
            .and_then(|b| b.inner.lock().unwrap().peer_stopper.clone());
        let dispatch_lock = crate::work_status::lock_receipts(&self.state, entity, thread)?;
        let targets =
            crate::work_status::trail(&self.state, entity, thread, &record.obligation_id)?
                .stop_targets;
        drop(dispatch_lock);
        // Stu's own commands for this job first, the one he waits on in the foreground among
        // them: they are his native tasks, so each ends by its own id and nothing else stops.
        let mut outcomes: Vec<_> = match &stopper {
            Some((_, handle)) => {
                let records = assignment::read_all(&self.state, entity, thread).map_err(|e| e.to_string())?;
                job_commands(&records, id, &handle.own_commands())
                    .iter()
                    .map(|task| handle.stop_background_command(task))
                    .collect()
            }
            None => Vec::new(),
        };
        if let Some((session, handle)) = &front_desk {
            for tool in crate::work_status::front_desk_calls(&self.state, session,
                &record.instruction_ledger_ref, &record.instruction_sha256)? {
                // A call that started no task, or one that already ended, is no command to stop.
                match handle.stop_worker(&tool) {
                    CommandStop::NotDelivered => {}
                    outcome => outcomes.push(outcome),
                }
            }
        }
        outcomes.extend(targets.iter().map(|(session, tool)| match &stopper {
            Some((owner, handle)) if owner == session => handle.stop_worker(tool),
            _ => CommandStop::NotDelivered,
        }));
        assignment::record_stop_observation(
            &self.state,
            entity,
            thread,
            id,
            stop_detail(&outcomes),
        )
        .map_err(|e| e.to_string())?;
        crate::questions::Store::new(&self.state).close(
            entity,
            thread,
            Some(&record.obligation_id),
            "The user stopped this assignment",
            false,
        )?;
        crate::question_work::discard(&self.state, entity, thread, &record.obligation_id)?;
        let scope = crate::status_tools::StatusToolScope {
            version: 1,
            state_root: self.state.clone(),
            entity_id: entity.into(),
            thread_id: thread.into(),
        };
        crate::session_tools::observe_user_stop(
            &scope,
            id,
            &record.obligation_id,
            stopped.user_stop_requested_at_ms.unwrap(),
        )?;
        if let Some(binding) = self
            .backend(thread)
            .and_then(|b| b.inner.lock().unwrap().binding.clone())
        {
            self.wake_messages(&binding);
        }
        Ok(())
    }

    /// Quit ends provider connections. Job reports, questions and inbox receipts remain unchanged.
    pub fn shutdown(&self) {
        *self.closing.lock().unwrap() = true;
        let backends: Vec<_> = self.backends.lock().unwrap().values().cloned().collect();
        for backend in &backends {
            let cancel = {
                let mut inner = backend.inner.lock().unwrap();
                inner.closing = true;
                inner.cancel.clone().or_else(|| {
                    inner
                        .peer_stopper
                        .as_ref()
                        .map(|(_, handle)| handle.clone())
                })
            };
            backend.wake.notify_all();
            if let Some(cancel) = cancel {
                cancel.shutdown();
            }
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        for backend in &backends {
            let mut inner = backend.inner.lock().unwrap();
            while inner.processing {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    break;
                }
                inner = backend.wake.wait_timeout(inner, left).unwrap().0;
            }
            drop(inner);
            let Some(mut lease) = lock_until(&backend.lease, deadline) else {
                continue;
            };
            let retired = lease.take();
            drop(lease);
            drop(retired);
        }
    }
}

fn lock_until<T>(
    lock: &Mutex<T>,
    deadline: std::time::Instant,
) -> Option<std::sync::MutexGuard<'_, T>> {
    loop {
        match lock.try_lock() {
            Ok(guard) => return Some(guard),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub(crate) fn verified_instruction(
    state: &Path,
    entity: &str,
    thread: &str,
    ledger_ref: &str,
    sha256: &str,
) -> Result<String, &'static str> {
    use sha2::{Digest, Sha256};
    use std::io::BufRead;
    const UNAVAILABLE: &str = "The original request could not be verified. Nothing was started.";
    let prefix = format!("ledger:{thread}:");
    let turn = ledger_ref
        .strip_prefix(&prefix)
        .filter(|turn| !turn.is_empty())
        .ok_or(UNAVAILABLE)?;
    let path = state
        .parent()
        .ok_or(UNAVAILABLE)?
        .join("conversation-ledger.jsonl");
    let file = std::fs::File::open(path).map_err(|_| UNAVAILABLE)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut found = None;
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|_| UNAVAILABLE)? == 0 {
            break;
        }
        // A concurrent append may not have completed its last record yet.
        if !line.ends_with('\n') {
            break;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if row["event"] != "PromptReceived" || row["turn_id"] != turn {
            continue;
        }
        let text = row["text"].as_str().ok_or(UNAVAILABLE)?;
        if found.is_some()
            || row["thread_id"] != thread
            || row["entity_id"] != entity
            || !matches!(row["source"].as_str(), Some("text" | "jam"))
            || format!("{:x}", Sha256::digest(text.as_bytes())) != sha256
        {
            return Err(UNAVAILABLE);
        }
        found = Some(text.to_owned());
    }
    found.ok_or(UNAVAILABLE)
}

pub const STARTED_DETAIL: &str = "Rich has started on it.";
pub struct SilentNotifier;
impl WorkNotifier for SilentNotifier {
    fn raised(&self, _: &str, _: &PendingNotice) {}
}
pub fn for_him(row: &Assignment) -> bool {
    row.waiting_sentence || row.state.awaits_his_word()
}

/// Which of Stu's own running commands are job `id`'s. **The job travels with the command:**
/// Stu names the job's `obligation_id` in the description of every command he runs for it
/// (stu.md), so a command naming this job is its, and one naming another job is never its.
/// A command that names no job is this job's only when it started after the job was
/// registered and no other job could be running it: another job counts when it is still
/// open, was registered before the command started, and was not already waiting (on the user,
/// the screen or the allowance) with no change since. Anything else is left to Stu, who is
/// told of the Stop, rather than guessed.
fn job_commands(records: &[assignment::Assignment], id: &str, commands: &[crate::steering::OwnCommand]) -> Vec<String> {
    use AssignmentState::{Blocked, WaitingForQuota, WaitingForScreen};
    let Some(job) = records.iter().find(|r| r.id == id) else { return Vec::new() };
    // The whole id, never a part of a longer one ("obligation-7" is not in "obligation-70").
    let names = |command: &crate::steering::OwnCommand, record: &assignment::Assignment| {
        let (text, wanted) = (command.description.as_str(), record.obligation_id.as_str());
        let part_of_id = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        !wanted.is_empty() && text.match_indices(wanted).any(|(at, _)| {
            !part_of_id(text[..at].chars().next_back()) && !part_of_id(text[at + wanted.len()..].chars().next())
        })
    };
    let another_could_run = |at: u64| {
        records.iter().any(|r| {
            let waiting_since_before = matches!(r.reported_state(), Some(Blocked | WaitingForScreen | WaitingForQuota))
                && r.updated_at_ms <= at;
            r.id != id && r.accepts_input() && r.registered_at_ms <= at && !waiting_since_before
        })
    };
    commands
        .iter()
        .filter(|command| {
            if names(command, job) {
                return true;
            }
            !records.iter().any(|r| r.id != id && names(command, r))
                && command.started_at_ms >= job.registered_at_ms
                && !another_could_run(command.started_at_ms)
        })
        .map(|command| command.task_id.clone())
        .collect()
}

fn stop_detail(outcomes: &[CommandStop]) -> &'static str {
    if outcomes.is_empty() {
        "Stop requested by the user. Stu has been notified."
    } else if outcomes
        .iter()
        .any(|outcome| matches!(outcome, CommandStop::Refused(_)))
    {
        "Stop requested. A command refused to stop and is still running."
    } else if outcomes.contains(&CommandStop::NotDelivered) {
        "Stop requested. A command could not be reached."
    } else if outcomes.contains(&CommandStop::Unconfirmed) {
        "Stop requested. A command's ending has not been confirmed."
    } else {
        "The job's recorded work stopped. Stu has been notified."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::Registration;
    use crate::cognition::WorkAssignment;
    use crate::entity::{EntityId, PersonId};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StartGate(Mutex<bool>, Condvar, std::sync::atomic::AtomicBool);
    impl StartGate {
        fn open_now() -> Arc<Self> {
            Arc::new(Self(
                Mutex::new(true),
                Condvar::new(),
                std::sync::atomic::AtomicBool::new(false),
            ))
        }
        fn shut(&self) {
            self.2.store(false, Ordering::SeqCst);
            *self.0.lock().unwrap() = false;
        }
        fn release(&self) {
            *self.0.lock().unwrap() = true;
            self.1.notify_all();
        }
        fn wait_until_open(&self) {
            let mut open = self.0.lock().unwrap();
            while !*open {
                self.2.store(true, Ordering::SeqCst);
                open = self.1.wait(open).unwrap();
            }
        }
    }
    #[derive(Default)]
    struct TeamScript {
        digest: Option<String>,
        /// The user's settings digest the factory reports now; each lease keeps the one it
        /// was spawned under.
        settings: Option<String>,
        in_turn: VecDeque<Option<String>>,
        turn_results: VecDeque<&'static str>,
    }
    struct Lease {
        session: String,
        prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
        settings: Option<String>,
    }
    impl Cognition for Lease {
        fn session_id(&self) -> &str {
            &self.session
        }
        fn user_settings_digest(&self) -> Option<String> {
            self.settings.clone()
        }
        fn refresh_team(&mut self) -> Result<bool, CognitionError> {
            Ok(true)
        }
        fn running_commands(&self) -> Option<crate::lease_commands::CommandReading> {
            *self.commands.lock().unwrap()
        }
        fn bind_work_assignment(&mut self, _: &WorkAssignment) -> Result<(), CognitionError> {
            panic!("Stu is never rebound to a job")
        }
        fn prompt(
            &mut self,
            text: &str,
            on: &mut dyn FnMut(TurnItem),
        ) -> Result<String, CognitionError> {
            self.prompts.lock().unwrap().push(text.into());
            let mut team = self.team.lock().unwrap();
            if let Some(digest) = team.in_turn.pop_front() {
                team.digest = digest;
            }
            let result=team.turn_results.pop_front().unwrap_or("end_turn");
            drop(team);
            if result=="retry_progress" {
                for record in crate::machinery::MachineryRecord::from_native_event(&serde_json::json!({"type":"system","subtype":"api_retry","attempt":1,"retry_delay_ms":1000,"error_status":529}),&self.session,0) {on(TurnItem::Machinery(record));}
                return Ok("end_turn".into());
            }
            if result=="native_quota" {return Err(CognitionError::Protocol(crate::claude_accounts::usage_limit_error(None,"native quota refusal")))}
            if result=="max_tokens_no_text" {return Ok("max_tokens".into())}
            if let Some(status)=result.strip_prefix("pre_model_") {
                if status=="protocol" {return Err(CognitionError::Protocol("API Error: 503 Server error.".into()))}
                let text=match status {"529"=>"API Error: 529 Overloaded.","429"=>"API Error: 429 Rate limited.",_=>"API Error: 503 Server error."};
                return Err(CognitionError::Protocol(text.into()));
            }
            if result=="tool_error" {
                for record in crate::machinery::MachineryRecord::from_native_event(&serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool-one","name":"Bash","input":{"command":"fictional"}}]}}),&self.session,0) {on(TurnItem::Machinery(record));}
                return Err(CognitionError::Protocol("API Error: 503 Server error.".into()));
            }
            on(TurnItem::Text {
                seq: 0,
                text: "Recorded reply",
            });
            match result {
                "provider_error"=>Err(CognitionError::Protocol("API Error: 529 Overloaded.".into())),
                "io_error"=>Err(CognitionError::Io("connection ended".into())),
                reason=>Ok(reason.into()),
            }
        }
    }
    struct Factory {
        spawns: Arc<AtomicUsize>,
        prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
        gate: Arc<StartGate>,
    }
    impl LeaseFactory for Factory {
        fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
            panic!("Stu has an independent scoped session")
        }
        fn team_digest(&self) -> Option<String> {
            self.team.lock().unwrap().digest.clone()
        }
        fn user_settings_digest(&self) -> Option<String> {
            self.team.lock().unwrap().settings.clone()
        }
        fn spawn_work(&self, _: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
            self.gate.wait_until_open();
            let n = self.spawns.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Lease {
                session: if n == 0 {
                    "work-session-one".into()
                } else {
                    format!("work-session-rotated-{n}")
                },
                prompts: self.prompts.clone(),
                commands: self.commands.clone(),
                settings: self.team.lock().unwrap().settings.clone(),
                team: self.team.clone(),
            }))
        }
    }
    #[derive(Default)]
    struct RecordedRetryClock(Mutex<Vec<std::time::Duration>>);
    impl crate::upstream::RetryClock for RecordedRetryClock {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            self.0.lock().unwrap().push(wait);
            !stop()
        }
    }
    struct Harness {
        root: PathBuf,
        state: PathBuf,
        host: Arc<WorkHost>,
        binding: ThreadBinding,
        bound: Arc<Mutex<Vec<WorkAssignment>>>,
        spawns: Arc<AtomicUsize>,
        work_prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
        start_gate: Arc<StartGate>,
        notices: Arc<RecordingNotifier>,
    }
    /// The overload lines the host shows, in order, with the conversation each was for.
    #[derive(Default)]
    struct RecordingNotifier(Mutex<Vec<(String, String)>>);
    impl WorkNotifier for RecordingNotifier {
        fn raised(&self, _: &str, _: &PendingNotice) {}
        fn overload_retry(&self, thread_id: &str, sentence: &str) {
            self.0.lock().unwrap().push((thread_id.to_string(), sentence.to_string()));
        }
    }
    fn harness(_: u64) -> Harness {
        let root = std::env::temp_dir().join(format!("richos-peer-host-{}", uuid::Uuid::new_v4()));
        let state = root.join("engine-state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(root.join("conversation-ledger.jsonl"),serde_json::json!({"event":"PromptReceived",
            "entity_id":"depot","thread_id":"thread-one","turn_id":"turn-7","source":"text","text":"landing the three branches"}).to_string()+"\n").unwrap();
        let binding = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-one",
            1,
        );
        let spawns = Arc::new(AtomicUsize::new(0));
        let work_prompts = Arc::new(Mutex::new(vec![]));
        let commands = Arc::new(Mutex::new(None));
        let team = Arc::new(Mutex::new(TeamScript::default()));
        let start_gate = StartGate::open_now();
        let factory = Factory {
            spawns: spawns.clone(),
            prompts: work_prompts.clone(),
            commands: commands.clone(),
            team: team.clone(),
            gate: start_gate.clone(),
        };
        let notices = Arc::new(RecordingNotifier::default());
        let host = WorkHost::new(
            &state,
            Box::new(factory),
            notices.clone(),

        );
        host.set_retry_clock(Arc::new(RecordedRetryClock::default()));

        Harness {
            root,
            state,
            host,
            binding,
            bound: Arc::new(Mutex::new(vec![])),
            spawns,
            work_prompts,
            commands,
            team,
            start_gate,
            notices,
        }
    }
    fn every_worker_observed_ending(h: &Harness) {
        let folder = h.state.join("evidence/work-session-one");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
    }

    fn registration(harness: &Harness) -> Registration {
        Registration {
            entity_id: harness.binding.entity_id().to_string(),
            thread_id: harness.binding.thread_id().to_string(),
            obligation_id: "obligation-7".into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: {
                use sha2::Digest;
                format!("{:x}", sha2::Sha256::digest(b"landing the three branches"))
            },
            title: "landing the three branches".into(),
            repositories: vec!["/fictional/project".into()],
            needs_screen: false,
        }
    }

    fn until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn a_closed_stu_provider_reopens_at_the_next_normal_input_boundary() {
        struct Closed;
        impl Cognition for Closed {
            fn session_id(&self)->&str {"ended-stu-session"}
            fn is_closed(&self)->bool {true}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
                panic!("a closed provider cannot accept input")
            }
        }
        let h=harness(0);
        let backend=h.host.backend_for(h.binding.thread_id()).unwrap();
        *backend.lease.lock().unwrap()=Some(Box::new(Closed));
        assert_eq!(h.spawns.load(Ordering::SeqCst),0);
        h.host.ensure_lease(&backend,&h.binding).unwrap();
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"an idle EOF left Stu on a closed provider");
        h.host.ensure_lease(&backend,&h.binding).unwrap();
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"a healthy connection was replaced");
        h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn account_resume_preserves_background_commands_and_waits_for_the_input_boundary() {
        use crate::lease_commands::CommandReading;
        struct Busy {
            dropped:Arc<std::sync::atomic::AtomicBool>,
            commands:Arc<Mutex<CommandReading>>,
            pending:Arc<std::sync::atomic::AtomicBool>,
        }
        impl Cognition for Busy {
            fn session_id(&self)->&str {"old-account-session"}
            fn account(&self)->Option<&str> {Some(crate::claude_accounts::ACCOUNT_ONE)}
            fn has_pending_input(&self)->bool {self.pending.load(Ordering::SeqCst)}
            fn running_commands(&self)->Option<CommandReading> {Some(*self.commands.lock().unwrap())}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {Ok("end_turn".into())}
        }
        impl Drop for Busy {fn drop(&mut self){self.dropped.store(true,Ordering::SeqCst);}}
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let next=quota.accounts.add("Other").unwrap();quota.accounts.use_first(&next.id).unwrap();
        h.host.set_quota(quota);
        let backend=h.host.backend_for(h.binding.thread_id()).unwrap();
        let dropped=Arc::new(std::sync::atomic::AtomicBool::new(false));
        let commands=Arc::new(Mutex::new(CommandReading::Running(1)));
        let pending=Arc::new(std::sync::atomic::AtomicBool::new(false));
        *backend.lease.lock().unwrap()=Some(Box::new(Busy {dropped:dropped.clone(),commands:commands.clone(),pending:pending.clone()}));
        for reading in [CommandReading::Running(1),CommandReading::Unreadable] {
            *commands.lock().unwrap()=reading;
            h.host.renew_if_account_changed(&backend,&h.binding);
            assert!(!dropped.load(Ordering::SeqCst),"resume killed a possibly running command");
            assert_eq!(h.spawns.load(Ordering::SeqCst),0);
        }
        *commands.lock().unwrap()=CommandReading::Clear;pending.store(true,Ordering::SeqCst);
        h.host.renew_if_account_changed(&backend,&h.binding);
        assert!(!dropped.load(Ordering::SeqCst));
        pending.store(false,Ordering::SeqCst);
        h.host.renew_if_account_changed(&backend,&h.binding);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"clear commands and finished input should use the common opener");
        h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Stu's lease tells the accounts where it runs** (nightly 65, 2026-10-18: "I switched
    /// the team to your Home account" was said while Stu's whole job ran on Work). The switch
    /// from Account 1 to Home is decided while Stu's lease is on Account 1: Rich's turn on Home
    /// says nothing yet and the panel shows no switch card; Stu's lease moving at its boundary
    /// releases the line and the card.
    #[test]
    fn the_switch_is_announced_only_once_stus_lease_has_moved_too() {
        struct OnAccount(String);
        impl Cognition for OnAccount {
            fn session_id(&self)->&str {"stu-on-an-account"}
            fn account(&self)->Option<&str> {Some(&self.0)}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {Ok("end_turn".into())}
        }
        struct Follows(Arc<crate::quota::Service>);
        impl LeaseFactory for Follows {
            fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
            fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
                Ok(Box::new(OnAccount(self.0.lease_account().id)))
            }
        }
        use crate::claude_accounts::tests::{reading, NOW};
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let home=quota.accounts.add("Home").unwrap();
        let host=WorkHost::new(&h.state,Box::new(Follows(quota.clone())),Arc::new(SilentNotifier));
        host.set_quota(quota.clone());
        let backend=host.backend_for(h.binding.thread_id()).unwrap();
        host.ensure_lease(&backend,&h.binding).unwrap();
        let readings=[("1".to_string(),reading(10.,3_600_000,99.,48*3_600_000)),
            (home.id.clone(),reading(0.,3_600_000,0.,72*3_600_000))].into_iter().collect();
        assert!(quota.accounts.evaluate(&readings,None,NOW).unwrap(),"Account 1 at 99% of its week is left for Home");
        quota.accounts.ran_on("rich-reply",&home.id);
        assert_eq!(quota.accounts.take_notice(),None,"Stu is still on Account 1");
        assert!(quota.view().last_switch.is_none(),"no switch card while Stu is on Account 1");
        host.renew_if_account_changed(&backend,&h.binding);
        assert_eq!(backend.lease.lock().unwrap().as_ref().and_then(|lease|lease.account().map(str::to_string)),Some(home.id.clone()));
        assert!(quota.accounts.take_notice().unwrap().starts_with("I switched the team to your **Home** account."));
        assert_eq!(quota.view().last_switch.map(|s|s.to),Some(home.id.clone()));
        host.shutdown();h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    /// A Stu whose turns can be driven as Claude Code drives an automatic one: the test hands
    /// frames to the between-turn sink the host installed, as the native reader does.
    struct Driven {
        session: String,
        account: String,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        sink: Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
    }
    impl Cognition for Driven {
        fn session_id(&self)->&str {&self.session}
        fn account(&self)->Option<&str> {Some(&self.account)}
        fn running_commands(&self)->Option<crate::lease_commands::CommandReading> {Some(*self.commands.lock().unwrap())}
        fn set_between_turn_sink(&mut self,sink:crate::cognition::BetweenTurnSink) {*self.sink.lock().unwrap()=Some(sink);}
        fn prompt(&mut self,text:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
            self.prompts.lock().unwrap().push((self.account.clone(),text.to_string()));
            Ok("end_turn".into())
        }
    }
    /// Opens every lease on the account in use, as the production factory does
    /// (`lease_account`), and keeps the last one's sink.
    struct OnAccountInUse {
        quota: Arc<crate::quota::Service>,
        spawns: Arc<AtomicUsize>,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        sink: Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
    }
    impl LeaseFactory for OnAccountInUse {
        fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
        fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
            let n=self.spawns.fetch_add(1,Ordering::SeqCst);
            Ok(Box::new(Driven {session:format!("stu-session-{n}"),account:self.quota.lease_account().id,
                commands:self.commands.clone(),sink:self.sink.clone(),prompts:self.prompts.clone()}))
        }
    }
    struct Moving {
        h: Harness,
        quota: Arc<crate::quota::Service>,
        home: crate::claude_accounts::Account,
        host: Arc<WorkHost>,
        spawns: Arc<AtomicUsize>,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        sink: Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
    }
    /// Stu started on Account 1 by a message in his inbox (the run loop opens his lease), then
    /// the switch from Account 1 (99% of its week) to Home is decided while a command of his runs.
    fn stu_on_account_one_then_a_switch() -> Moving {
        let m=stu_on_account_one();
        *m.commands.lock().unwrap()=crate::lease_commands::CommandReading::Running(1);
        switch_to_home(&m);
        m
    }
    /// The switch from Account 1 (99% of its week) to Home is decided.
    fn switch_to_home(m: &Moving) {
        use crate::claude_accounts::tests::{reading, NOW};
        let readings=[("1".to_string(),reading(10.,3_600_000,99.,48*3_600_000)),
            (m.home.id.clone(),reading(0.,3_600_000,0.,72*3_600_000))].into_iter().collect();
        assert!(m.quota.accounts.evaluate(&readings,None,NOW).unwrap(),"Account 1 at 99% of its week is left for Home");
    }
    /// Stu started on Account 1 by a message in his inbox (the run loop opens his lease).
    fn stu_on_account_one() -> Moving {
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let home=quota.accounts.add("Home").unwrap();
        let (spawns,prompts)=(Arc::new(AtomicUsize::new(0)),Arc::new(Mutex::new(Vec::new())));
        let commands=Arc::new(Mutex::new(crate::lease_commands::CommandReading::Clear));
        let sink=Arc::new(Mutex::new(None));
        let host=WorkHost::new(&h.state,Box::new(OnAccountInUse {quota:quota.clone(),spawns:spawns.clone(),
            commands:commands.clone(),sink:sink.clone(),prompts:prompts.clone()}),Arc::new(SilentNotifier));
        host.set_quota(quota.clone());
        crate::session_tools::observe_stall(&host.message_scope(&h.binding),"the website job").unwrap();
        host.wake_messages(&h.binding);
        until("Stu's first turn on Account 1",||prompts.lock().unwrap().len()==1);
        assert_eq!(prompts.lock().unwrap()[0].0,"1");
        Moving {h,quota,home,host,spawns,commands,sink,prompts}
    }
    /// Claude Code starts an automatic turn of Stu's on its own (a task notification) and he
    /// takes his first step in it; the turn's result comes later ([`automatic_turn_ends`]).
    fn automatic_turn_starts(m: &Moving) {
        let sink=m.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
        sink(vec![serde_json::json!({"type":"system","subtype":"task_notification","task_id":"build","status":"completed"}),
            serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"The build finished. Deploying next."}]}})]);
    }
    fn automatic_turn_ends(m: &Moving) {
        let sink=m.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
        sink(vec![serde_json::json!({"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn"})]);
    }
    /// Every message the app sent Stu with `event` in it, with the account it reached him on.
    fn told(m: &Moving, event: &str) -> Vec<(String, String)> {
        m.prompts.lock().unwrap().iter().filter(|(_,text)|text.contains(event)).cloned().collect()
    }
    /// One automatic turn of Stu's, as Claude Code reports it between turns of ours.
    fn automatic_turn(m: &Moving) {
        let sink=m.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
        sink(vec![serde_json::json!({"type":"system","subtype":"task_notification","task_id":"build","status":"completed"}),
            serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"The build finished."}]}}),
            serde_json::json!({"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn"})]);
    }

    /// **Stu really moves at the next boundary of his work, automatic turns included** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 2; nightly 65: after the switch to Home his whole
    /// website job ran as automatic turns, helper hand-backs and task notifications, on Work,
    /// because he was moved only before a message the app sent him). Nothing here calls the
    /// renewal: his turns end as Claude Code reports them.
    ///
    /// The switch comes while a command of his runs: ending his session would end it, so he is
    /// told once (`account_switch`, delivered on Account 1) and keeps his session. When an
    /// automatic turn ends with nothing of his running, he is on a new session on Home within
    /// seconds (well inside the idle check's 30 s, so it is the turn's end that moved him) and
    /// told so there (`account_moved`), and the panel shows the team on Home.
    #[test]
    fn stu_moves_to_the_new_account_when_an_automatic_turn_ends_and_is_told_while_a_command_runs() {
        use crate::lease_commands::CommandReading;
        let m=stu_on_account_one_then_a_switch();
        assert_eq!(m.quota.accounts.in_use().id,m.home.id,"the switch to Home is decided");
        automatic_turn(&m);
        until("Stu is told the switch is coming",||m.prompts.lock().unwrap().iter().any(|(_,text)|text.contains("account_switch")));
        assert_eq!(m.spawns.load(Ordering::SeqCst),1,"a running command was ended by a move");
        let told:Vec<_>=m.prompts.lock().unwrap().iter().filter(|(_,text)|text.contains("account_switch")).cloned().collect();
        assert_eq!(told.len(),1,"told once: {told:?}");
        assert_eq!(told[0].0,"1","told on the session that is being left");
        assert!(told[0].1.contains(r#"\"from\":\"Account 1\""#) && told[0].1.contains(r#"\"to\":\"Home\""#),"{}",told[0].1);

        *m.commands.lock().unwrap()=CommandReading::Clear;
        let ended=std::time::Instant::now();
        automatic_turn(&m);
        until("Stu is on Home",||m.spawns.load(Ordering::SeqCst)==2);
        // A third of the idle check's 30 s (`ACCOUNT_CHECK_EVERY`).
        assert!(ended.elapsed()<std::time::Duration::from_secs(10),"moved by the turn's end, not the idle check: {:?}",ended.elapsed());
        until("Stu is told he moved, on Home",||m.prompts.lock().unwrap().iter().any(|(account,text)|account==&m.home.id&&text.contains("account_moved")));
        let view=serde_json::to_value(m.quota.view()).unwrap();
        let working=|id:&str|view["accounts"].as_array().unwrap().iter().find(|a|a["id"]==id).map(|a|a["working"].clone());
        assert_eq!(working(&m.home.id),Some(serde_json::json!(["team"])));
        assert_eq!(working("1"),Some(serde_json::Value::Null),"nobody is left on Account 1");
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// The run loop has nothing left to do for now: no input of ours and no account check.
    fn settled(m: &Moving) {
        let backend=m.host.backend(m.h.binding.thread_id()).unwrap();
        until("the run loop settles",||{let inner=backend.inner.lock().unwrap();!inner.processing&&inner.move_check.is_none()});
    }

    /// **Stu is told the switch is coming while an automatic turn of his runs** (review
    /// rv-20261019T030516Z-0a1a8728-e798, finding 1: the run loop skipped every account check
    /// while one ran, so through a job of many steps he kept spending the old account and was
    /// never asked to save and stop). The switch is decided while a command of his runs; then
    /// Claude Code starts an automatic turn and he takes a step. His turn's result has not come
    /// yet: he is told now, on Account 1, and his session is not ended under him. When the
    /// turn's result comes with nothing of his running, he is on Home and told to carry on.
    #[test]
    fn stu_is_told_to_save_and_stop_while_an_automatic_turn_runs() {
        use crate::lease_commands::CommandReading;
        let m=stu_on_account_one_then_a_switch();
        automatic_turn_starts(&m);
        until("Stu is told the switch is coming while his automatic turn runs",||!told(&m,"account_switch").is_empty());
        let switch=told(&m,"account_switch");
        assert_eq!(switch.len(),1,"told once: {switch:?}");
        assert_eq!(switch[0].0,"1","told on the session he is still on");
        assert_eq!(m.spawns.load(Ordering::SeqCst),1,"his running turn and command were ended by a move");

        *m.commands.lock().unwrap()=CommandReading::Clear;
        automatic_turn_ends(&m);
        until("Stu is on Home",||m.spawns.load(Ordering::SeqCst)==2);
        until("Stu is told to carry on, on Home",||told(&m,"account_moved").iter().any(|(account,_)|account==&m.home.id));
        assert_eq!(told(&m,"account_switch").len(),1,"told again after it was said: {:?}",told(&m,"account_switch"));
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// **Stu is always told to carry on after a move** (review rv-20261019T030516Z-0a1a8728-e798,
    /// finding 1: a move he had not been warned of sent nothing, so his resumed session had
    /// nothing telling it to continue). An automatic turn of his starts before any switch; the
    /// switch is decided while it runs; its result comes with nothing of his running, so he is
    /// moved at once, and told so on Home.
    #[test]
    fn stu_is_told_to_carry_on_after_a_move_he_was_not_warned_of() {
        let m=stu_on_account_one();
        automatic_turn_starts(&m);
        settled(&m);
        switch_to_home(&m);
        automatic_turn_ends(&m);
        until("Stu is on Home",||m.spawns.load(Ordering::SeqCst)==2);
        assert_eq!(told(&m,"account_switch"),Vec::<(String,String)>::new(),"he was warned after all");
        until("Stu is told to carry on, on Home",||told(&m,"account_moved").iter().any(|(account,_)|account==&m.home.id));
        let moved=told(&m,"account_moved");
        assert!(moved[0].1.contains(r#"\"from\":\"Account 1\""#) && moved[0].1.contains(r#"\"to\":\"Home\""#),"{}",moved[0].1);
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// **The panel shows the account each session is really on** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 3: Home was "In use now" while Stu still ran on
    /// Work). The switch to Home is decided and Rich's next turn runs on Home; Stu's session is
    /// still on Account 1 with a command running. The published view says so per account.
    #[test]
    fn the_accounts_view_shows_where_rich_and_the_team_really_are_during_a_switch() {
        let m=stu_on_account_one_then_a_switch();
        m.quota.accounts.ran_on("rich-reply",&m.home.id);
        let view=serde_json::to_value(m.quota.view()).unwrap();
        let account=|id:&str|view["accounts"].as_array().unwrap().iter().find(|a|a["id"]==id).cloned().unwrap();
        assert_eq!((account(&m.home.id)["inUse"].clone(),account(&m.home.id)["working"].clone()),
            (serde_json::json!(true),serde_json::json!(["rich"])),"Home: in use, and Rich is on it");
        assert_eq!((account("1")["inUse"].clone(),account("1")["working"].clone()),
            (serde_json::json!(false),serde_json::json!(["team"])),"Account 1: the team is still on it");
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    #[test]
    fn a_finished_native_turn_keeps_its_unfinished_job_resumable_after_quit() {
        for terminal in [Some(AssignmentState::Settled),Some(AssignmentState::Failed),None] {
        for result in ["end_turn","max_tokens","provider_error"] {
        let h=harness(1);let record=assignment::register(&h.state,&registration(&h)).unwrap();
        assignment::advance(&h.state,"depot","thread-one",&record.id,AssignmentState::Running,"Worker start observed").unwrap();
        let scope=h.host.message_scope(&h.binding);let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front-session","rich",Some(crate::ecs::UserInstruction{ledger_ref:registration(&h).instruction_ledger_ref,sha256:registration(&h).instruction_sha256,text:None})).unwrap();
        crate::session_tools::delegate(&path,&scope,serde_json::json!({"text":"Land the three branches"})).unwrap();
        let through=crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1;
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(30)),"native turn did not end");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"native end_turn erased the still-unfinished handoff");
        assert!(crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap().is_some(),"native end_turn erased Stu's recovery evidence");
        crate::session_tools::complete_attempt(&scope,"stu",&format!("peer:{through}")).unwrap();
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"generic attempt completion bypassed the job's primary report");
        // A later correction is independent input, not a request to execute the handoff again.
        h.team.lock().unwrap().turn_results.push_back(result);
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Keep the original release date"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(2,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        let retry = usize::from(result=="provider_error");
        assert_eq!(prompts.len(),2+retry,"unfinished work was repeatedly redispatched to the same Stu: {prompts:?}");
        assert!(prompts[1].contains("Keep the original release date"));
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Use the connected repository"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(3,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(),3+retry,"a failed turn made accepted work new again: {prompts:?}");
        assert!(prompts[2+retry].contains("Use the connected repository"),"old work was repeated after {result}: {prompts:?}");
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"a turn result replaced live Stu");
        h.host.shutdown();
        let resumed=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();assert_eq!(resumed.1,through);
        let factory=Factory{spawns:h.spawns.clone(),prompts:h.work_prompts.clone(),commands:h.commands.clone(),team:h.team.clone(),gate:h.start_gate.clone()};
        let restarted=WorkHost::new(&h.state,Box::new(factory),Arc::new(SilentNotifier));
        let recovered=if matches!(result,"end_turn"|"provider_error") {1}else{2};
        restarted.wake_messages(&h.binding);assert!(restarted.wait_for_completed(recovered,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(),3+retry+recovered as usize,"restart repeated input beyond its unfinished receipts: {prompts:?}");
        assert!(prompts[3+retry].contains("Land the three branches"));
        if !matches!(result,"end_turn"|"provider_error") {assert!(prompts[4+retry].contains("Keep the original release date"),"the uncertain correction was lost");}
        assert!(!prompts[3+retry..].iter().any(|p|p.contains("Use the connected repository")),"successfully completed input replayed");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some());
        // Only Stu's actual terminal report retires the original input.
        if let Some(terminal)=terminal {assignment::advance(&h.state,"depot","thread-one",&record.id,terminal,"Stu's terminal report").unwrap();}
        else {assignment::record_user_stop(&h.state,"depot","thread-one",&record.id).unwrap();}
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
        assert!(crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap().is_none());
        restarted.shutdown();
        }
        }
    }

    #[test]
    fn scheduled_input_retries_only_overload_and_recovers_unfinished_work_after_restart() {
        for kind in ["handoff", "peer", "standing-answer", "job-answer"] {
            for result in ["end_turn", "max_tokens", "provider_error", "io_error", "pre_model_503", "pre_model_529", "pre_model_429", "pre_model_protocol", "tool_error", "native_quota", "max_tokens_no_text"] {
                let h = harness(0);
                let scope = h.host.message_scope(&h.binding);
                let path = h.root.join("peer-status.json");
                let instruction = if matches!(kind, "handoff" | "job-answer") {
                    assignment::register(&h.state, &registration(&h)).unwrap();
                    Some(crate::ecs::UserInstruction {
                        ledger_ref: registration(&h).instruction_ledger_ref,
                        sha256: registration(&h).instruction_sha256,
                        text: None,
                    })
                } else {
                    None
                };
                crate::session_tools::bind(&path, &scope, "front-session", "rich", instruction).unwrap();
                h.team.lock().unwrap().turn_results.push_back(result);
                match kind {
                    "handoff" => { crate::session_tools::delegate(&path, &scope, serde_json::json!({"text":"Input under test"})).unwrap(); }
                    "peer" => { crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Input under test"})).unwrap(); }
                    _ => {
                        let answer = crate::questions::Delivery {
                            obligation_id: (kind == "job-answer").then(|| registration(&h).obligation_id),
                            id: "answer-under-test".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
                            asker: "stu".into(), set_id: None, text: "Input under test".into(), receipt: None, order: None,
                        };
                        h.host.queue_question_answer(&h.binding, &answer).unwrap();
                    }
                }
                h.host.wake_messages(&h.binding);
                assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)));
                crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Independent later input"})).unwrap();
                h.host.wake_messages(&h.binding);
                until("later input must pass an answer already received by live Stu", || h.work_prompts.lock().unwrap().iter().any(|p| p.contains("Independent later input")));
                h.host.shutdown();
                let prompts = h.work_prompts.lock().unwrap().clone();
                let recovered_overload = matches!(result,"provider_error"|"pre_model_529");
                let expected = if result == "io_error" || recovered_overload {2} else {1};
                assert_eq!(prompts.iter().filter(|p|p.contains("Input under test")).count(),expected,
                    "{kind}/{result}: input repeated in the same live session: {prompts:?}");
                assert_eq!(h.spawns.load(Ordering::SeqCst), if result=="io_error" {2}else{1});
                let mut facts=Vec::new();
                while let Some((message,through))=crate::session_tools::pending(&scope,"rich").unwrap() {
                    assert_eq!(message.from,"work-status");assert!(message.instruction.is_none());
                    facts.push(serde_json::from_str::<serde_json::Value>(&message.text).unwrap());
                    crate::session_tools::acknowledge(&scope,"rich",through).unwrap();
                }
                assert_eq!(facts.len(),usize::from(result!="end_turn" && !recovered_overload),"{kind}/{result}: wrong failure-report count");
                let before = prompts.len();
                crate::session_tools::send(&path, &scope, serde_json::json!({"text":"After restart"})).unwrap();
                let restarted = WorkHost::new(&h.state, Box::new(Factory {
                    spawns: h.spawns.clone(), prompts: h.work_prompts.clone(), commands: h.commands.clone(),
                    team: h.team.clone(), gate: h.start_gate.clone(),
                }), Arc::new(SilentNotifier));
                restarted.wake_messages(&h.binding);
                until("restart must deliver its new input", || h.work_prompts.lock().unwrap().iter().any(|p| p.contains("After restart")));
                restarted.shutdown();
                let prompts = h.work_prompts.lock().unwrap().clone();
                let unfinished = kind == "handoff" || (!matches!(result,"end_turn"|"io_error") && !recovered_overload);
                assert_eq!(prompts[before..].iter().filter(|p| p.contains("Input under test")).count(), usize::from(unfinished),
                    "{kind}/{result}: new session lost or repeated recovery input: {prompts:?}");
                assert!(!prompts[before..].iter().any(|p| p.contains("Independent later input")), "completed input replayed");
                std::fs::remove_dir_all(h.root).unwrap();
            }
        }
    }

    #[test]
    fn stu_overload_uses_the_ceos_seven_waits_on_one_session_and_success_resets_them() {
        let h = harness(0);
        let clock = Arc::new(RecordedRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let scope = h.host.message_scope(&h.binding);
        let path = h.root.join("peer-status.json");
        crate::session_tools::bind(&path, &scope, "front", "rich", None).unwrap();
        h.team.lock().unwrap().turn_results.extend(std::iter::repeat_n("pre_model_529", 7).chain(["end_turn"]));
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Original request"})).unwrap();
        h.host.wake_messages(&h.binding);
        // Hang guard: the clock is controlled; actual input completion is the verdict.
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)), "retry fixture did not finish");
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 8);
        assert!(prompts.iter().all(|prompt| prompt == &prompts[0]), "retry changed the original input");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        assert_eq!(*clock.0.lock().unwrap(), crate::upstream::OVERLOAD_RETRY_WAITS);
        assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none(), "recovered failure was announced as terminal");
        // Each wait is shown in his conversation as RichOS's own retry line, as Rich's is.
        let shown = h.notices.0.lock().unwrap().clone();
        let expected: Vec<(String, String)> = [1, 2, 5, 10, 20, 40, 80].iter().enumerate().map(|(k, minutes)| {
            let when = if *minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") };
            ("thread-one".to_string(), format!("{} RichOS will try again in {when}, in the same conversation, so nothing is \
                lost. This is retry {} of 7.", crate::upstream::UpstreamFault::Overloaded.ceo_message(), k + 1))
        }).collect();
        assert_eq!(shown, expected, "Stu's overload waits were not shown as RichOS's retry line");
        h.team.lock().unwrap().turn_results.extend(["pre_model_529", "end_turn"]);
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Independent later request"})).unwrap();
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(30)));
        assert_eq!(clock.0.lock().unwrap().as_slice(), [crate::upstream::OVERLOAD_RETRY_WAITS.as_slice(), &[std::time::Duration::from_secs(60)]].concat());
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// The `worker_overload` (and `turn_overload`) facts Stu has been sent, in order.
    fn overload_notices(h: &Harness) -> Vec<serde_json::Value> {
        h.work_prompts.lock().unwrap().iter().filter_map(|prompt| {
            let message: serde_json::Value = serde_json::from_str(prompt).ok()?;
            (message["from"] == "overload-retry").then(|| serde_json::from_str::<serde_json::Value>(message["text"].as_str()?).ok()).flatten()
        }).collect()
    }

    fn worker_overloaded(call: &str) -> serde_json::Value {
        serde_json::json!({"type":"assistant","parent_tool_use_id":call,"error":"overloaded",
            "message":{"content":[{"type":"text","text":"API Error: 529 Overloaded."}]}})
    }

    /// A retry clock that holds each wait open until the test ends it, so an overload can arrive
    /// DURING a wait. It writes down each wait it was asked for.
    #[derive(Default)]
    struct HeldRetryClock { waits: Mutex<Vec<std::time::Duration>>, ended: Mutex<usize>, turn: Condvar }
    impl HeldRetryClock {
        fn waits(&self) -> Vec<std::time::Duration> { self.waits.lock().unwrap().clone() }
        fn end_one(&self) { *self.ended.lock().unwrap() += 1; self.turn.notify_all(); }
    }
    impl crate::upstream::RetryClock for HeldRetryClock {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            let mine = { let mut waits = self.waits.lock().unwrap(); waits.push(wait); waits.len() };
            let mut ended = self.ended.lock().unwrap();
            while *ended < mine {
                if stop() { return false; }
                ended = self.turn.wait_timeout(ended, std::time::Duration::from_millis(10)).unwrap().0;
            }
            !stop()
        }
    }

    /// Follow-up 11, ONE clock: an overload hits every running worker at once. Eight workers
    /// overloaded together wait one 1-minute wait and each gets the restart message when it
    /// ends; a ninth overload during that wait joins it, and a worker reported twice is told
    /// once. The next overload after the wait moves one step, to 2 minutes.
    #[test]
    fn eight_workers_overloaded_together_wait_one_minute_and_all_are_told_to_restart() {
        let h = harness(0);
        let clock = Arc::new(HeldRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        let observe = |frame: serde_json::Value| h.host.observe_worker(&backend, &h.binding, "stu-session", &frame);
        let minute = std::time::Duration::from_secs(60);
        for n in 1..=8 { observe(worker_overloaded(&format!("W{n}"))); }
        until("the workers' overload did not start a wait", || clock.waits().len() == 1);
        observe(worker_overloaded("W9"));
        observe(worker_overloaded("W1"));
        assert_eq!(clock.waits(), [minute], "an overload during the wait started a wait of its own");
        assert!(overload_notices(&h).is_empty(), "Stu was told before the wait ended");
        clock.end_one();
        until("Stu was not told to restart every stopped worker", || overload_notices(&h).len() == 9);
        let mut told: Vec<_> = overload_notices(&h).iter().map(|fact| (fact["worker"]["tool_use_id"].as_str().unwrap_or("").to_string(),
            fact["event"].as_str().map(str::to_string), fact["retry"].as_u64(), fact["waited_minutes"].as_u64(), fact["retries_used_up"].as_bool())).collect();
        told.sort();
        let expected: Vec<_> = (1..=9).map(|n| (format!("W{n}"), Some("worker_overload".to_string()), Some(1), Some(1), Some(false))).collect();
        assert_eq!(told, expected, "the stopped workers did not all wait the one 1-minute wait");
        observe(worker_overloaded("W1"));
        until("the next overload did not start a wait", || clock.waits().len() == 2);
        assert_eq!(clock.waits(), [minute, 2 * minute], "the next overload after the wait did not move one step");
        clock.end_one();
        until("Stu was not told after the second wait", || overload_notices(&h).len() == 10);
        let last = overload_notices(&h).pop().unwrap();
        assert_eq!((&last["worker"]["tool_use_id"], &last["retry"], &last["waited_minutes"]),
            (&serde_json::json!("W1"), &serde_json::json!(2), &serde_json::json!(2)), "{last}");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Follow-up 11, ONE clock: any worker's ordinary answer from the API resets it, so the next
    /// overload, whichever worker it stops, waits the first minute again.
    #[test]
    fn a_workers_success_resets_the_workers_overload_clock_to_one_minute() {
        let h = harness(0);
        let clock = Arc::new(RecordedRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        let observe = |frame: serde_json::Value| h.host.observe_worker(&backend, &h.binding, "stu-session", &frame);
        observe(worker_overloaded("A"));
        until("Stu was not told after the first wait", || overload_notices(&h).len() == 1);
        observe(worker_overloaded("B"));
        until("Stu was not told after the second wait", || overload_notices(&h).len() == 2);
        observe(serde_json::json!({"type":"assistant","parent_tool_use_id":"C",
            "message":{"content":[{"type":"text","text":"Reading the files again."}]}}));
        observe(worker_overloaded("A"));
        until("Stu was not told after the third wait", || overload_notices(&h).len() == 3);
        let minute = std::time::Duration::from_secs(60);
        assert_eq!(*clock.0.lock().unwrap(), [minute, 2 * minute, minute], "a worker's success did not reset the clock");
        let told: Vec<_> = overload_notices(&h).iter().map(|fact| (fact["worker"]["tool_use_id"].clone(), fact["retry"].clone())).collect();
        assert_eq!(told, [(serde_json::json!("A"), serde_json::json!(1)), (serde_json::json!("B"), serde_json::json!(2)),
            (serde_json::json!("A"), serde_json::json!(1))]);
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Follow-up 11: overloads one after another, whichever worker each stops (Stu restarts a
    /// worker as a new run), wait 1, 2, 5, 10, 20, 40 and 80 minutes and Stu is told after each
    /// wait; the eighth is told at once, with no retries left. A frame Claude Code did not mark
    /// as an API error is the worker's own words.
    #[test]
    fn a_workers_eighth_overload_in_a_row_tells_stu_its_retries_are_used_up() {
        let h = harness(0);
        let clock = Arc::new(RecordedRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        let quoted = serde_json::json!({"type":"assistant","parent_tool_use_id":"run-1",
            "message":{"content":[{"type":"text","text":"API Error: 529 Overloaded."}]}});
        h.host.observe_worker(&backend, &h.binding, "stu-session", &quoted);
        for n in 1..=8 {
            h.host.observe_worker(&backend, &h.binding, "stu-session", &worker_overloaded(&format!("run-{n}")));
            until("Stu was not told about his worker's overload", || overload_notices(&h).len() == n);
        }
        let told: Vec<_> = overload_notices(&h).iter().map(|fact| (fact["retry"].as_u64(), fact["retries_left"].clone(), fact["waited_minutes"].as_u64())).collect();
        let mut expected: Vec<_> = [1, 2, 5, 10, 20, 40, 80].iter().enumerate()
            .map(|(k, minutes)| (Some(k as u64 + 1), serde_json::json!(6 - k), Some(*minutes as u64))).collect();
        expected.push((None, serde_json::json!(0), None));
        assert_eq!(told, expected);
        assert_eq!(*clock.0.lock().unwrap(), crate::upstream::OVERLOAD_RETRY_WAITS, "the quoted line was taken for an overload, or a wait was skipped");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Review rv-20261018T200657Z-585c24df-f9d6, finding 2: the seventh overload is granted the
    /// 80-minute wait and its retry still runs. Only the eighth, which is granted none, says the
    /// retries are used up, and that is what Stu's instruction reports to Rich.
    #[test]
    fn the_seventh_worker_retry_runs_and_only_the_eighth_overload_ends_the_schedule() {
        let h = harness(0);
        h.host.set_retry_clock(Arc::new(RecordedRetryClock::default()));
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        for n in 1..=8 {
            h.host.observe_worker(&backend, &h.binding, "stu-session", &worker_overloaded("worker-call"));
            until("Stu was not told about his worker's overload", || overload_notices(&h).len() == n);
        }
        let told = overload_notices(&h);
        let seventh = told.iter().find(|fact| fact["retry"] == 7).expect("no seventh retry");
        assert_eq!((&seventh["waited_minutes"], &seventh["retries_used_up"]), (&serde_json::json!(80), &serde_json::json!(false)),
            "the granted seventh retry was marked as the end: {seventh}");
        let eighth = told.iter().find(|fact| fact["retry"].is_null()).expect("no eighth overload");
        assert_eq!(eighth["retries_used_up"], true, "the eighth overload, granted no retry, did not end the schedule: {eighth}");
        let stu = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app/agents/stu.md")).unwrap();
        assert!(stu.contains("`retries_used_up`") && !stu.contains("with no retries left means"),
            "stu.md does not report the failure on the fact the host sets only when no retry is granted");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_live_overload_wait_releases_the_native_reader_and_does_not_block_other_input() {
        use std::sync::mpsc;
        struct HeldClock { entered: mpsc::Sender<()>, release: Mutex<mpsc::Receiver<()>> }
        impl crate::upstream::RetryClock for HeldClock {
            fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
                assert_eq!(wait, std::time::Duration::from_secs(60));
                self.entered.send(()).unwrap();
                // Hang guard only: the test releases the actual retry, never elapsed time.
                self.release.lock().unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap();
                !stop()
            }
        }
        struct NativeInput { calls: Arc<Mutex<Vec<String>>>, finished: mpsc::Sender<()> }
        impl TurnCancel for NativeInput {
            fn cancel(&self) -> bool { true }
            fn send_input(&self, _: &str, text: &str, completed: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>) -> Result<bool,String> {
                self.calls.lock().unwrap().push(text.to_string());
                completed(crate::steering::InputOutcome { result: Ok("end_turn".into()), report_failure:true });
                if text == "Original live input" { self.finished.send(()).unwrap(); }
                Ok(true)
            }
        }
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.root.join("peer-status.json");
        crate::session_tools::bind(&path, &scope, "front", "rich", None).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Original live input"})).unwrap();
        let (message, through) = crate::session_tools::pending(&scope, "stu").unwrap().unwrap();
        let backend = Backend::new();
        let session = "existing-live-stu".to_string();
        backend.inner.lock().unwrap().lease_session = Some(session.clone());
        crate::session_tools::begin_attempt(&scope, "stu", &format!("peer:{through}"), &session).unwrap();
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        h.host.set_retry_clock(Arc::new(HeldClock { entered, release:Mutex::new(released) }));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (finished, done) = mpsc::channel();
        let native = Arc::new(NativeInput { calls:calls.clone(), finished });
        backend.inner.lock().unwrap().peer_stopper = Some((session.clone(), native.clone()));
        let completed = h.host.live_completion(&backend, &h.binding, message,
            (InputReceipt::Peer(through), Some(session)), "Original live input".into());
        let (returned, reader_returned) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            completed(crate::steering::InputOutcome { result:Err("API Error: 529 Overloaded.".into()), report_failure:true });
            returned.send(()).unwrap();
        });
        // Hang guards: callback return and retry admission are separate positive events.
        waiting.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        reader_returned.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        reader.join().unwrap();
        assert!(calls.lock().unwrap().is_empty());
        assert!(native.send_input("independent", "Independent later input", Box::new(|outcome|assert!(outcome.result.is_ok()))).unwrap());
        release.send(()).unwrap();
        // Hang guard: completion comes from the retry's actual native callback.
        done.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert_eq!(*calls.lock().unwrap(), ["Independent later input", "Original live input"]);
        assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none());
        assert!(crate::session_tools::pending(&scope, "stu").unwrap().is_none());
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn native_retry_activity_does_not_create_an_app_retry_or_failure_report() {
        let h=harness(0);let scope=h.host.message_scope(&h.binding);let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front","rich",None).unwrap();
        h.team.lock().unwrap().turn_results.push_back("retry_progress");
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"One request"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(30)));
        h.host.shutdown();assert_eq!(h.work_prompts.lock().unwrap().len(),1);
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none());
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_disconnected_stu_reconciles_unfinished_input_in_a_new_session_once() {
        let h=harness(1);assignment::register(&h.state,&registration(&h)).unwrap();
        let scope=h.host.message_scope(&h.binding);let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front-session","rich",Some(crate::ecs::UserInstruction{ledger_ref:registration(&h).instruction_ledger_ref,sha256:registration(&h).instruction_sha256,text:None})).unwrap();
        crate::session_tools::delegate(&path,&scope,serde_json::json!({"text":"Land the three branches"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(30)));
        h.team.lock().unwrap().turn_results.push_back("io_error");
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Keep the original release date"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(2,std::time::Duration::from_secs(30)));
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(4,std::time::Duration::from_secs(30)));
        h.host.shutdown();
        assert_eq!(h.spawns.load(Ordering::SeqCst),2,"a disconnected connection was reused");
        let prompts=h.work_prompts.lock().unwrap().clone();assert_eq!(prompts.len(),4,"recovery input repeated: {prompts:?}");
        assert!(prompts[2].contains("Land the three branches"));assert!(prompts[3].contains("Keep the original release date"));
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"unfinished work was lost on disconnect");
    }

    #[test]
    fn one_stu_connection_can_open_while_another_is_still_starting() {
        struct IndependentFactory(Arc<StartGate>);
        impl LeaseFactory for IndependentFactory {
            fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
                panic!("scoped Stu only")
            }
            fn spawn_work(
                &self,
                binding: &ThreadBinding,
            ) -> Result<Box<dyn Cognition>, CognitionError> {
                if binding.thread_id() == "thread-one" {
                    self.0.wait_until_open();
                }
                Ok(Box::new(crate::cognition::MockCognition::new(
                    binding.thread_id(),
                    Vec::new(),
                )))
            }
        }
        let h = harness(1);
        let gate = StartGate::open_now();
        gate.shut();
        let host = WorkHost::new(
            &h.state,
            Box::new(IndependentFactory(gate.clone())),
            Arc::new(SilentNotifier),

        );
        let a = Backend::new();
        let b = Backend::new();
        let opening = host.clone();
        let binding = h.binding.clone();
        let worker = std::thread::spawn(move || opening.ensure_lease(&a, &binding));
        // load-bound: hang guard only; the gate's recorded entry proves A is still opening.
        until("A never reached its startup gate", || {
            gate.2.load(Ordering::SeqCst)
        });
        let binding = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-two",
            1,
        );
        let opening = host.clone();
        let second = b.clone();
        let (sent, received) = std::sync::mpsc::channel();
        let other = std::thread::spawn(move || {
            let result = opening.ensure_lease(&second, &binding);
            sent.send(result).unwrap();
        });
        // load-bound: hang guard only; receiving B's result before releasing A proves independence.
        let result = received.recv_timeout(std::time::Duration::from_secs(60));
        let a_held = !worker.is_finished();
        gate.release();
        worker.join().unwrap().unwrap();
        other.join().unwrap();
        result
            .expect("B waited for A's connection startup")
            .unwrap();
        assert!(a_held);
        assert_eq!(
            b.lease.lock().unwrap().as_ref().unwrap().session_id(),
            "thread-two"
        );
    }

    #[test]
    fn a_correction_reaches_stus_live_input_while_his_job_owns_the_lease() {
        type Delivery = (String, String, Box<dyn FnOnce(crate::steering::InputOutcome) + Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self) -> bool {
                panic!("a correction must not cancel the job")
            }
            fn send_input(
                &self,
                id: &str,
                text: &str,
                taken: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
            ) -> Result<bool, String> {
                self.0.send((id.into(), text.into(), taken)).unwrap();
                Ok(true)
            }
        }
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Original request"}),
        )
        .unwrap();
        let original = crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .unwrap()
            .1;
        let backend = Backend::new();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.processing = true;
            inner.binding = Some(h.binding.clone());
            inner.peer_active_through = Some(original);
            inner.peer_input = Some(Arc::new(Input(tx)));
            inner.lease_session = Some("same-stu".into());
        }
        h.host
            .backends
            .lock()
            .unwrap()
            .insert(h.binding.thread_id().into(), backend.clone());
        let running = backend.lease.lock().unwrap();
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Correction: keep the existing release date"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        let (_, text, taken) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: correction must reach input while the job still holds the lease");
        assert!(text.contains("keep the existing release date"));
        assert_eq!(
            crate::session_tools::pending(&scope, "stu")
                .unwrap()
                .unwrap()
                .1,
            original,
            "writing is not consumption"
        );
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Then retain the current team"}),
        )
        .unwrap();
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        h.host.wake_messages(&h.binding);
        let (_, text, taken) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: consumption forwards the next durable message");
        assert!(text.contains("retain the current team"));
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        crate::session_tools::acknowledge(&scope, "stu", original).unwrap();
        assert!(
            crate::session_tools::pending(&scope, "stu")
                .unwrap()
                .is_none(),
            "finishing the original turn must not rewind the consumed cursor"
        );
        assert!(
            backend.inner.lock().unwrap().processing,
            "the running job was not ended to deliver input"
        );
        assert!(h.host.open_assignments().unwrap().is_empty());
        use sha2::Digest;
        let instruction = crate::ecs::UserInstruction::with_text(
            "ledger:thread-one:turn-7".into(),
            format!("{:x}", sha2::Sha256::digest("landing the three branches")),
            Some("landing the three branches"),
        );
        let mut request = registration(&h);
        request.instruction_ledger_ref = instruction.ledger_ref.clone();
        request.instruction_sha256 = instruction.sha256.clone();
        let receipt = assignment::register(&h.state, &request).unwrap();
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction))
            .unwrap();
        crate::session_tools::delegate(
            &path,
            &scope,
            serde_json::json!({"text":"New execution request"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        let (_,text,taken)=rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: a new handoff must reach the same live input before Stu ends his current turn");
        let handoff: crate::session_tools::Message = serde_json::from_str(&text).unwrap();
        assert!(handoff.handoff);
        assert_eq!(handoff.text, "New execution request");
        assert_eq!(handoff.work.as_ref().unwrap().assignment_id, receipt.id);
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Correction to the current job: do not change the date"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        let (_, text, taken) = rx.recv_timeout(std::time::Duration::from_secs(5)).expect(
            "hang guard: a queued new job must not block a quick correction to current work",
        );
        assert!(text.contains("do not change the date"));
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        let pending=crate::session_tools::pending(&scope,"stu").unwrap().expect("native turn completion erased the job");
        assert_eq!(pending.0.work.as_ref().unwrap().assignment_id,receipt.id);
        drop(running);
        *backend.lease.lock().unwrap()=Some(Box::new(Lease {
            session:"same-stu".into(),prompts:h.work_prompts.clone(),commands:h.commands.clone(),team:h.team.clone(),settings:None,
        }));
        h.team.lock().unwrap().turn_results.push_back("max_tokens");
        let message=crate::session_tools::Message {handoff:false,from:"rich".into(),text:"Receiving turn".into(),instruction:None,work:None};
        assert!(h.host.run_message(&backend,&h.binding,&message,"live-input-receiving-turn").is_err());
        let after=backend.inner.lock().unwrap().peer_active_through.unwrap();
        assert!(crate::session_tools::pending_after(&scope,"stu",after).unwrap().is_none(),"accepted live input became new after the receiving turn failed");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"failed turn lost the unfinished handoff");
        assignment::advance(&h.state,"depot","thread-one",&receipt.id,AssignmentState::Settled,"Stu reported completion").unwrap();
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none(),"completed job replayed after Stu's report");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn unfinished_live_handoffs_reach_recovered_stu_before_newer_input() {
        use std::io::Write as _;
        type Delivery = (String, Box<dyn FnOnce(crate::steering::InputOutcome) + Send>);
        struct Input {
            seen: Mutex<std::collections::HashSet<String>>,
            tx: std::sync::mpsc::Sender<Delivery>,
        }
        impl TurnCancel for Input {
            fn cancel(&self) -> bool {
                panic!("live recovery must not interrupt Stu")
            }
            fn send_input(
                &self,
                id: &str,
                text: &str,
                taken: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
            ) -> Result<bool, String> {
                if !self.seen.lock().unwrap().insert(id.into()) {
                    return Ok(false);
                }
                self.tx.send((text.into(), taken)).unwrap();
                Ok(true)
            }
        }
        for advanced_frontier in [false, true] {
            let h = harness(0);
            let scope = h.host.message_scope(&h.binding);
            let path = crate::session_tools::inbox(&scope, "stu").unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let add = |label: &str| {
                use sha2::Digest;
                let request=Registration{obligation_id:format!("work-{label}"),instruction_ledger_ref:format!("ledger:thread-one:{label}"),
                    instruction_sha256:format!("{:x}",sha2::Sha256::digest(label)),..registration(&h)};
                let receipt=assignment::register(&h.state,&request).unwrap();
                let instruction=crate::ecs::UserInstruction::with_text(request.instruction_ledger_ref,request.instruction_sha256,Some(label));
                let message = crate::session_tools::Message {
                    handoff: true,
                    from: "rich".into(),
                    text: label.into(),
                    instruction: Some(instruction),
                    work: Some(crate::session_tools::WorkReference{assignment_id:receipt.id,obligation_id:request.obligation_id}),
                };
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .unwrap();
                writeln!(file, "{}", serde_json::to_string(&message).unwrap()).unwrap();
                file.sync_all().unwrap();
                file.metadata().unwrap().len()
            };
            let a = add("A");
            crate::session_tools::begin_attempt(
                &scope,
                "stu",
                &format!("peer:{a}"),
                "before-relaunch",
            )
            .unwrap();
            let b = add("B");
            crate::session_tools::begin_attempt(
                &scope,
                "stu",
                &format!("peer:{b}"),
                "before-relaunch",
            )
            .unwrap();
            crate::session_tools::accept_live_handoff(&scope, "stu", b).unwrap();
            if advanced_frontier {
                crate::session_tools::accept_live_handoff(&scope, "stu", a).unwrap();
            }
            assert_eq!(
                crate::session_tools::pending(&scope, "stu")
                    .unwrap()
                    .unwrap()
                    .0
                    .text,
                "A"
            );
            crate::session_tools::begin_attempt(
                &scope,
                "stu",
                &format!("peer:{a}"),
                "after-relaunch",
            )
            .unwrap();
            let c = add("C");
            let backend = Backend::new();
            let (tx, rx) = std::sync::mpsc::channel();
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.binding = Some(h.binding.clone());
                inner.peer_active_through = Some(a);
                inner.peer_input = Some(Arc::new(Input {
                    seen: Mutex::new(Default::default()),
                    tx,
                }));
                inner.lease_session = Some("after-relaunch".into());
            }
            h.host
                .backends
                .lock()
                .unwrap()
                .insert(h.binding.thread_id().into(), backend.clone());
            let lease = backend.lease.lock().unwrap();
            h.host.wake_messages(&h.binding);
            let (text, taken) = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("hang guard: recovered B must reach Stu while A still holds the lease");
            let message: crate::session_tools::Message = serde_json::from_str(&text).unwrap();
            assert_eq!(message.text, "B", "new C overtook unfinished B");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap()
                    ["previous_delivery_attempt"]["session_id"],
                "before-relaunch"
            );
            assert!(
                crate::session_tools::previous_attempt(&scope, "stu", &format!("peer:{b}"))
                    .unwrap()
                    .is_some(),
                "sending without a provider receipt was called complete"
            );
            h.host.wake_messages(&h.binding);
            let (text, complete_c) = rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("native queue must receive C after B without waiting for B's result");
            assert_eq!(serde_json::from_str::<crate::session_tools::Message>(&text).unwrap().text,"C");
            assert!(rx.try_recv().is_err(),"an input was submitted twice to the same native session");
            taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
            complete_c(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
            assert_eq!(backend.inner.lock().unwrap().peer_active_through, Some(c));
            crate::session_tools::acknowledge(&scope, "stu", a).unwrap();
            // The existing completion API is bounded by actual receipts in the fix.
            assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"end_turn erased unfinished jobs");
            for record in assignment::read_all(&h.state,"depot","thread-one").unwrap(){
                assignment::advance(&h.state,"depot","thread-one",&record.id,AssignmentState::Settled,"Stu's terminal report").unwrap();
            }
            assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
            drop(lease);
            h.host.shutdown();
            std::fs::remove_dir_all(h.root).unwrap();
        }
    }

    #[test]
    fn stopped_new_handoff_is_skipped_on_live_input_without_blocking_a_correction() {
        type Delivery = (String, Box<dyn FnOnce(crate::steering::InputOutcome) + Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self) -> bool {
                panic!("Stop of B must not cancel Stu's other work")
            }
            fn send_input(
                &self,
                _: &str,
                text: &str,
                taken: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
            ) -> Result<bool, String> {
                self.0.send((text.into(), taken)).unwrap();
                Ok(true)
            }
        }
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"A is running"}))
            .unwrap();
        let a = crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .unwrap()
            .1;
        let record = assignment::register(&h.state, &registration(&h)).unwrap();
        let instruction = crate::ecs::UserInstruction::with_text(
            "ledger:thread-one:turn-7".into(),
            registration(&h).instruction_sha256,
            Some("landing the three branches"),
        );
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction))
            .unwrap();
        crate::session_tools::delegate(
            &path,
            &scope,
            serde_json::json!({"text":"B must never start"}),
        )
        .unwrap();
        assignment::record_user_stop(&h.state, "depot", "thread-one", &record.id).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Correction to A"}))
            .unwrap();
        let backend = Backend::new();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.binding = Some(h.binding.clone());
            inner.peer_active_through = Some(a);
            inner.peer_input = Some(Arc::new(Input(tx)));
            inner.lease_session = Some("same-stu".into());
        }
        h.host
            .backends
            .lock()
            .unwrap()
            .insert(h.binding.thread_id().into(), backend.clone());
        let lease = backend.lease.lock().unwrap();
        h.host.wake_messages(&h.binding);
        let (text, taken) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: stopped B must not block A's correction");
        let message: crate::session_tools::Message = serde_json::from_str(&text).unwrap();
        assert!(!message.handoff);
        assert_eq!(message.text, "Correction to A");
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        assert!(rx.try_recv().is_err(), "a stopped handoff was sent");
        crate::session_tools::acknowledge(&scope, "stu", a).unwrap();
        assert!(crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .is_none());
        drop(lease);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn ordinary_peer_input_runs_without_an_assignment_and_is_not_replayed_after_ack() {
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Inspect the current evidence"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        assert!(
            h.host
                .wait_for_completed(1, std::time::Duration::from_secs(10)),
            "hang guard: wait for the receiving turn to complete"
        );
        assert!(
            h.bound.lock().unwrap().is_empty(),
            "communication manufactured an assignment"
        );
        assert!(h.host.open_assignments().unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert!(h.work_prompts.lock().unwrap()[0].contains("Inspect the current evidence"));
        assert!(crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .is_none());
        h.host.shutdown();
    }

    #[test]
    fn peer_input_and_question_answer_reach_stu_before_his_first_output_or_result() {
        type Delivery=(String,Box<dyn FnOnce(crate::steering::InputOutcome)+Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self)->bool {false}
            fn send_input(&self,_:&str,text:&str,completed:Box<dyn FnOnce(crate::steering::InputOutcome)+Send>)->Result<bool,String> {
                self.0.send((text.to_string(),completed)).map_err(|e|e.to_string())?;
                Ok(true)
            }
        }
        struct Early {
            input:Arc<Input>,
            entered:std::sync::mpsc::Sender<()>,
            release:Option<std::sync::mpsc::Receiver<()>>,
        }
        impl Cognition for Early {
            fn session_id(&self)->&str {"early-native-stu"}
            fn cancel_handle(&self)->Option<Arc<dyn TurnCancel>> {Some(self.input.clone())}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {panic!("submit must precede reply waiting")}
            fn enqueue_prompt(&mut self,_:&str,started:Box<dyn FnOnce()->Result<(),String>+Send>)->Result<crate::cognition::PendingReply,CognitionError> {
                started().map_err(CognitionError::Protocol)?;
                self.entered.send(()).unwrap();
                let release=self.release.take().unwrap();
                Ok(crate::cognition::PendingReply::new(move |_| {
                    // load-bound: a positive release owns the answer; this deadline only catches a hang.
                    release.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
                    Ok("end_turn".into())
                }))
            }
        }
        struct Factory(Mutex<Option<Early>>);
        impl LeaseFactory for Factory {
            fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
            fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
                Ok(Box::new(self.0.lock().unwrap().take().expect("Stu must keep one native session")))
            }
        }
        let h=harness(0);
        let (sent,received)=std::sync::mpsc::channel();
        let (entered,ready)=std::sync::mpsc::channel();
        let (release,waiting)=std::sync::mpsc::channel();
        let host=WorkHost::new(&h.state,Box::new(Factory(Mutex::new(Some(Early {
            input:Arc::new(Input(sent)),entered,release:Some(waiting),
        })))),Arc::new(SilentNotifier));
        let scope=host.message_scope(&h.binding);
        let path=h.state.join("early-rich.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"First input"})).unwrap();
        host.wake_messages(&h.binding);
        // load-bound: only a hang guard; the first reply cannot finish before release is sent.
        ready.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        until("Stu's input handle waited for output",||host.backend("thread-one").unwrap().inner.lock().unwrap().peer_input.is_some());
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Correction before any output"})).unwrap();
        host.wake_messages(&h.binding);
        // load-bound: wait for the actual submitted input, with the first reply still held.
        let (peer,complete_peer)=received.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert!(peer.contains("Correction before any output"));
        let answer=crate::questions::Delivery {obligation_id:None,id:"early-answer".into(),entity_id:"depot".into(),
            thread_id:"thread-one".into(),asker:"stu".into(),set_id:None,text:"Use Friday".into(),receipt:None,order:None};
        host.queue_question_answer(&h.binding,&answer).unwrap();
        // load-bound: a submitted answer is the verdict; the first result remains impossible until release.
        let (text,complete_answer)=received.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert!(text.contains("Use Friday"));
        host.wake_messages(&h.binding);
        assert!(received.try_recv().is_err(),"a repeated wake duplicated native input");
        assert!(crate::question_work::waiting(&h.state,&answer.id),"stdin submission falsely marked the answer taken");
        complete_peer(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        complete_answer(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        assert!(!crate::question_work::waiting(&h.state,&answer.id));
        release.send(()).unwrap();
        assert!(host.wait_for_completed(1,std::time::Duration::from_secs(30)));
        host.shutdown();
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_standing_stu_question_answer_reaches_his_turn_without_an_assignment() {
        let h = harness(0);
        let answer = crate::questions::Delivery {
            obligation_id: None,
            id: "standing-answer".into(),
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            asker: "stu".into(),
            set_id: Some("standing-choice".into()),
            text: "Use the existing launch date".into(),
            receipt: None,
            order: None,
        };
        h.host.queue_question_answer(&h.binding, &answer).unwrap();
        assert!(
            h.host
                .wait_for_completed(1, std::time::Duration::from_secs(10)),
            "hang guard: wait for the answer turn"
        );
        assert!(h.host.open_assignments().unwrap().is_empty());
        assert!(h.bound.lock().unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert!(h.work_prompts.lock().unwrap()[0].contains(&answer.text));
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        let wrong = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-two",
            1,
        );
        assert!(h.host.queue_question_answer(&wrong, &answer).is_err());
        h.host.shutdown();
        // A's answer retains A's authority even when B is the latest session input.
        let a = assignment::register(&h.state, &registration(&h)).unwrap();
        let b = assignment::register(
            &h.state,
            &Registration {
                obligation_id: "work-B".into(),
                instruction_ledger_ref: "ledger:thread-one:turn-B".into(),
                instruction_sha256: "b".repeat(64),
                ..registration(&h)
            },
        )
        .unwrap();
        let record = assignment::read(&h.state, "depot", "thread-one", &a.id).unwrap();
        let selected = crate::questions::Delivery {
            obligation_id: Some(record.obligation_id.clone()),
            ..answer.clone()
        };
        assert_eq!(
            h.host
                .question_answer_instruction(&selected)
                .unwrap()
                .unwrap()
                .ledger_ref,
            record.instruction_ledger_ref
        );
        let b_record = assignment::read(&h.state, "depot", "thread-one", &b.id).unwrap();
        let status = h.state.join("stu-status.json");
        crate::session_tools::bind(
            &status,
            &h.host.message_scope(&h.binding),
            "question-stu",
            "stu",
            Some(crate::ecs::UserInstruction {
                ledger_ref: b_record.instruction_ledger_ref,
                sha256: b_record.instruction_sha256,
                text: None,
            }),
        )
        .unwrap();
        let qscope = h.state.join("questions-scope.json");
        crate::question_tools::write_scope(
            &qscope,
            &crate::question_tools::Scope {
                context: crate::questions::AskScope {
                    obligation_id: None,
                    root: h.state.clone(),
                    entity_id: "depot".into(),
                    thread_id: "thread-one".into(),
                    turn_id: "peer:latest-B".into(),
                    asker: "stu".into(),
                    session_id: "question-stu".into(),
                    engine: None,
                    entity_root: None,
                    app_run: None,
                },
                actions_allowed: true,
                answer_method: "typed".into(),
                surface: "mac".into(),
            },
        )
        .unwrap();
        let args = serde_json::json!({"questions":[{"text":"When should the release ship?", "options":[
            {"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More checks"}]}]});
        let current = crate::question_tools::call(&qscope, "ask", args.clone()).unwrap();
        assert!(
            current["questions"][0]["obligation_id"].is_null(),
            "a conversation question must not inherit the latest job"
        );
        let mut for_b = args.clone();
        for_b["obligation_id"] = "work-B".into();
        assert_eq!(
            crate::question_tools::call(&qscope, "ask", for_b.clone()).unwrap()["questions"][0]
                ["obligation_id"],
            "work-B"
        );
        let mut for_a = args.clone();
        for_a["obligation_id"] = record.obligation_id.clone().into();
        let owned = crate::question_tools::call(&qscope, "ask", for_a.clone()).unwrap();
        assert_eq!(owned["questions"][0]["obligation_id"], record.obligation_id);
        assignment::record_user_stop(&h.state, "depot", "thread-one", &b.id).unwrap();
        assert!(crate::question_tools::call(&qscope, "ask", for_b).is_err());
        assert!(
            crate::question_tools::call(&qscope, "ask", args).is_ok(),
            "job Stop must not close conversation questions"
        );
        assert!(
            crate::question_tools::call(&qscope, "ask", for_a).is_ok(),
            "B's Stop revoked questions about A"
        );
        assignment::record_user_stop(&h.state, "depot", "thread-one", &a.id).unwrap();
        assert!(
            h.host.question_answer_instruction(&selected).is_err(),
            "stale answer reauthorized a stopped job"
        );
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_handoff_refused_before_stu_starts_is_saved_and_reports_one_failure() {
        struct Refused;
        impl LeaseFactory for Refused {
            fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
            fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
                Err(CognitionError::Io("provider failed before opening".into()))
            }
        }
        let h=harness(0);
        let host=WorkHost::new(&h.state,Box::new(Refused),Arc::new(SilentNotifier));
        let scope=host.message_scope(&h.binding);
        let path=h.state.join("rich-status.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Prepare the report"})).unwrap();
        let (message,through)=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();
        let error=host.run_message(&Backend::new(),&h.binding,&message,&format!("peer:{through}")).unwrap_err();
        assert!(error.contains("provider failed before opening"));
        let (saved,saved_through)=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();
        assert_eq!(saved_through,through);
        assert_eq!(saved.text,"Prepare the report");
        let (notice,end)=crate::session_tools::pending(&scope,"rich").unwrap().unwrap();
        assert_eq!(notice.from,"work-status");
        let facts:serde_json::Value=serde_json::from_str(&notice.text).unwrap();
        assert_eq!(facts["input_id"],format!("peer:{through}"));
        assert!(facts["error"].as_str().unwrap().contains("provider failed before opening"));
        crate::session_tools::acknowledge(&scope,"rich",end).unwrap();
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(),"one failure produced duplicate notices");
        for attempt in 0..3 {
            let reopened=WorkHost::new(&h.state,Box::new(Refused),Arc::new(SilentNotifier));
            assert!(reopened.run_message(&Backend::new(),&h.binding,&message,&format!("peer:{through}")).is_err());
            assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(),"retry {attempt} reported the same failed input again");
            assert_eq!(crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1,through,"unsent input was discarded");
            reopened.shutdown();
        }
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"A different request"})).unwrap();
        let (second,second_through)=crate::session_tools::pending_after(&scope,"stu",through).unwrap().unwrap();
        assert!(host.run_message(&Backend::new(),&h.binding,&second,&format!("peer:{second_through}")).is_err());
        let (notice,_)=crate::session_tools::pending(&scope,"rich").unwrap().unwrap();
        let facts:serde_json::Value=serde_json::from_str(&notice.text).unwrap();
        assert_eq!(facts["input_id"],format!("peer:{second_through}"),"dedup hid a different failed request");
        host.shutdown();
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Build plan 2026-10-21 item 0: a changed settings file renews Stu at his next message**,
    /// never while something of his runs, and an unchanged one renews nothing.
    #[test]
    fn a_change_to_the_users_settings_files_renews_stu_at_his_next_message() {
        let h = harness(0);
        h.team.lock().unwrap().settings = Some("as first read".into());
        every_worker_observed_ending(&h);
        for n in 1..=3 {
            let folder = h.state.join("evidence").join(format!("work-session-rotated-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
            std::fs::write(folder.join(".lock"), "").unwrap();
        }
        let backend = Backend::new();
        let message = crate::session_tools::Message {
            handoff: false, from: "rich".into(), text: "Carry on".into(), instruction: None, work: None,
        };
        let run = |n| h.host.run_message(&backend, &h.binding, &message, &format!("settings-test:{n}")).unwrap();
        run(1);
        run(2);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "unchanged settings renewed Stu");
        h.team.lock().unwrap().settings = Some("edited".into());
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Running(1));
        run(3);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "a renewal ended a running command");
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Clear);
        run(4);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the edited settings did not reach Stu's next message");
        run(5);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the renewed session was renewed again");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn peer_handoff_with_operational_record_executes_once() {
        let h = harness(0);
        h.team.lock().unwrap().digest = Some("stock".into());
        every_worker_observed_ending(&h);
        for n in 1..=4 {
            let folder = h
                .state
                .join("evidence")
                .join(format!("work-session-rotated-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
            std::fs::write(folder.join(".lock"), "").unwrap();
        }
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        let receipt = assignment::register(&h.state, &registration(&h)).unwrap();
        assert_eq!(
            h.host.adopt_registered(&h.binding),
            0,
            "an operational record must never dispatch a second execution"
        );
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Inspect the current evidence"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        // load-bound: hang guard only; the verdict is one consumed prompt and an empty inbox, not elapsed time.
        assert!(
            h.host
                .wait_for_completed(1, std::time::Duration::from_secs(10)),
            "hang guard: wait for peer consumption"
        );
        assert_eq!(h.host.adopt_registered(&h.binding), 0);
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &receipt.id)
                .unwrap()
                .state,
            AssignmentState::Registered
        );
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert!(
            h.bound.lock().unwrap().is_empty(),
            "Stu was rebound to a formal assignment"
        );
        assert!(crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .is_none());
        let backend = h.host.backend_for("thread-one").unwrap();
        let message = crate::session_tools::Message {
            handoff: true,
            from: "rich".into(),
            text: "Use the saved teammate for this authorized work".into(),
            instruction: None,
            work: None,
        };
        let run = |n| {
            h.host
                .run_message(&backend, &h.binding, &message, &format!("team-test:{n}"))
                .unwrap()
        };
        run(2);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "unchanged team renewed unnecessarily"
        );
        h.team.lock().unwrap().digest = Some("stock and mark".into());
        let evidence = h.state.join("evidence/work-session-one/callbacks.jsonl");
        let start=serde_json::json!({"schema":1,"callback":{"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":"live-worker"}}).to_string()+"\n";
        std::fs::write(&evidence, &start).unwrap();
        run(3);
        assert_eq!(
            backend.inner.lock().unwrap().lease_team.as_deref(),
            Some("stock and mark")
        );
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "renewal killed a live native worker"
        );
        let end=serde_json::json!({"schema":1,"callback":{"session_id":"work-session-one","hook_event_name":"SubagentStop","agent_id":"live-worker"}}).to_string()+"\n";
        std::fs::write(&evidence, start + &end).unwrap();
        run(4);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Running(1));
        h.team.lock().unwrap().digest = Some("stock, mark and norm".into());
        run(5);
        assert_eq!(
            backend.inner.lock().unwrap().lease_team.as_deref(),
            Some("stock, mark and norm")
        );
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Clear);
        run(6);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        h.team
            .lock()
            .unwrap()
            .in_turn
            .push_back(Some("stock, mark, norm and echo".into()));
        run(7);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        let prompts = h.work_prompts.lock().unwrap();
        assert_eq!(
            prompts.len(),
            7,
            "roster reload must not add a synthetic input or replay the accepted handoff"
        );
        drop(prompts);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn stopped_peer_handoff_is_not_delivered_after_first_connection_opens() {
        for during_open in [false, true] {
            let h = harness(0);
            let record = assignment::register(&h.state, &registration(&h)).unwrap();
            let scope = h.host.message_scope(&h.binding);
            let path = h.state.join("rich-status.json");
            let instruction = crate::ecs::UserInstruction::with_text(
                "ledger:thread-one:turn-7".into(),
                registration(&h).instruction_sha256,
                Some("landing the three branches"),
            );
            crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction))
                .unwrap();
            crate::session_tools::delegate(
                &path,
                &scope,
                serde_json::json!({"text":"Land the three branches"}),
            )
            .unwrap();
            let (message, through) = crate::session_tools::pending(&scope, "stu")
                .unwrap()
                .unwrap();
            let backend = Backend::new();
            backend.inner.lock().unwrap().binding = Some(h.binding.clone());
            h.host
                .backends
                .lock()
                .unwrap()
                .insert("thread-one".into(), backend.clone());
            if during_open {
                h.start_gate.shut()
            } else {
                h.host
                    .stop_assignment("depot", "thread-one", &record.id)
                    .unwrap()
            }
            let host = h.host.clone();
            let binding = h.binding.clone();
            let run = std::thread::spawn(move || {
                host.run_message(&backend, &binding, &message, &format!("peer:{through}"))
                    .unwrap();
                crate::session_tools::acknowledge(&host.message_scope(&binding), "stu", through)
                    .unwrap();
            });
            if during_open {
                // load-bound: hang guard only; the gate records actual entry into
                // the blocked factory call before Stop.
                until(
                    "the first peer connection never reached its opening gate",
                    || h.start_gate.2.load(Ordering::SeqCst),
                );
                h.host
                    .stop_assignment("depot", "thread-one", &record.id)
                    .unwrap();
                h.start_gate.release();
            }
            run.join().unwrap();
            assert!(
                h.work_prompts.lock().unwrap().is_empty(),
                "the stopped handoff reached Stu"
            );
            assert!(
                crate::session_tools::pending(&scope, "stu")
                    .unwrap()
                    .is_none_or(|(message, _)| !message.handoff && message.from == "user-control"),
                "stopped execution must not replay; the user Stop fact may remain for Stu"
            );
            assert_eq!(
                assignment::read(&h.state, "depot", "thread-one", &record.id)
                    .unwrap()
                    .state,
                AssignmentState::Interrupted
            );
            h.host.shutdown();
            std::fs::remove_dir_all(h.root).unwrap();
        }
    }

    #[test]
    fn independent_stu_stop_is_durable_while_a_dispatch_writer_is_busy() {
        struct Noop;
        impl TurnCancel for Noop {
            fn cancel(&self) -> bool {
                false
            }
        }
        let h = harness(0);
        let receipt = assignment::register(&h.state, &registration(&h)).unwrap();
        let backend = Backend::new();
        backend.inner.lock().unwrap().peer_stopper =
            Some(("independent-stu".into(), Arc::new(Noop)));
        h.host
            .backends
            .lock()
            .unwrap()
            .insert("thread-one".into(), backend);
        let writer = crate::work_status::lock_receipts(&h.state, "depot", "thread-one").unwrap();
        let host = h.host.clone();
        let id = receipt.id.clone();
        let stop = std::thread::spawn(move || host.stop_assignment("depot", "thread-one", &id));
        // The existing bounded wait is a hang guard. The verdict requires the
        // actual Stop fact while the dispatch lock is still held, never elapsed time.
        let durable = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            until("the Stop fact waited for the dispatch writer", || {
                assignment::read(&h.state, "depot", "thread-one", &receipt.id)
                    .unwrap()
                    .user_stop_requested_at_ms
                    .is_some()
            })
        }))
        .is_ok();
        drop(writer);
        stop.join().unwrap().unwrap();
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
        assert!(
            durable,
            "the user's Stop must be durable before waiting for a busy dispatch writer"
        );
    }

    #[test]
    fn independent_stu_stop_reaches_only_the_selected_assignments_worker() {
        struct Stops(Mutex<Vec<String>>);
        impl TurnCancel for Stops {
            fn cancel(&self) -> bool {
                panic!("per-assignment Stop must not cancel Stu")
            }
            fn shutdown(&self) {}
            fn stop_worker(&self, tool: &str) -> CommandStop {
                self.0.lock().unwrap().push(tool.into());
                CommandStop::Ended
            }
        }
        let h = harness(0);
        let first = assignment::register(&h.state, &registration(&h)).unwrap();
        let second = assignment::register(
            &h.state,
            &Registration {
                obligation_id: "another-obligation".into(),
                ..registration(&h)
            },
        )
        .unwrap();
        let backend = Backend::new();
        let stopper = Arc::new(Stops(Mutex::new(vec![])));
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.lease_session = Some("independent-stu".into());
            inner.peer_stopper = Some(("independent-stu".into(), stopper.clone()));
        }
        h.host
            .backends
            .lock()
            .unwrap()
            .insert("thread-one".into(), backend);
        let root = h.state.join("work-receipts").join({
            use sha2::Digest;
            format!("{:x}", sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]"))
        });
        std::fs::create_dir_all(&root).unwrap();
        for (id, obligation, tool) in [
            ("first", "obligation-7", "call-first"),
            ("second", "another-obligation", "call-second"),
        ] {
            std::fs::write(root.join(format!("{id}.json")),serde_json::json!({"schema":1,"id":id,
                "binding":{"entity_id":"depot","thread_id":"thread-one","session_id":"independent-stu"},
                "request":{"obligation_id":obligation,"role":"worker"},"status":"running","tool_use_id":tool}).to_string()).unwrap();
        }
        h.host
            .stop_assignment("depot", "thread-one", &first.id)
            .unwrap();
        assert_eq!(*stopper.0.lock().unwrap(), vec!["call-first"]);
        let stopped = assignment::read(&h.state, "depot", "thread-one", &first.id).unwrap();
        assert!(stopped.user_stop_requested_at_ms.is_some());
        let control = crate::session_tools::pending(&h.host.message_scope(&h.binding), "stu")
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(control.from, "user-control");
        let facts: serde_json::Value = serde_json::from_str(&control.text).unwrap();
        assert_eq!(facts["event"], "user_stop");
        assert_eq!(facts["assignment_id"], first.id);
        assert_eq!(
            facts["requested_at_ms"],
            stopped.user_stop_requested_at_ms.unwrap()
        );
        assert!(
            crate::session_tools::pending(&h.host.message_scope(&h.binding), "rich")
                .unwrap()
                .is_none()
        );
        assert_eq!(stopped.state, AssignmentState::Interrupted);
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &second.id)
                .unwrap()
                .state,
            AssignmentState::Registered
        );
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A job's Stop ends the command Stu is running for it in the foreground** (reap walk 10,
    /// esc-20261021T224235Z-4a2d4e70: "stop-fg (pid 3294) was still running after the trigger").
    /// A real `sleep` stands in for the command; the stand-in `claude` ends it only on a
    /// `stop_task` for that command's own task, as Claude Code 2.1.307 does (measured: the
    /// foreground task's `stop_task` answered success and the process was gone 28 ms later, with
    /// the turn going on). A helper's command in the same stream is not Stu's and is not stopped.
    /// RED before the fix: Stop sent nothing for Stu's own command, so the `sleep` outlived the bound.
    #[test]
    fn a_jobs_stop_ends_the_foreground_command_stu_runs_for_it() {
        let h = harness(0);
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "Run the heartbeat", false);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Two jobs running at once: the job travels with its command.** Job B is running when
    /// A's foreground command starts, and the command's description names A's obligation_id
    /// (stu.md). Stop on A ends it; B is left as it was. RED at 9b7b70f64: with B open, the
    /// command was nobody's, and it outlived the bound.
    #[test]
    fn a_jobs_stop_ends_the_command_naming_it_while_another_job_runs() {
        let h = harness(0);
        let running = assignment::register(&h.state, &Registration { obligation_id: "obligation-b".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &running.id, AssignmentState::Running, "Working.").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "obligation-7: run the heartbeat", false);
        let other = assignment::read(&h.state, "depot", "thread-one", &running.id).unwrap();
        assert_eq!((other.state, other.user_stop_requested_at_ms), (AssignmentState::Running, None), "the other job is not touched");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **The job's commands on the front desk** (reap walk walk-2bfb274d19d0): the job was handed
    /// to Stu, and Rich ran its commands himself in that same turn, the foreground one included.
    /// Stop on the job ends Rich's command by its own task, and his turn goes on. RED at
    /// ae213107a: the Stop reached only Stu's session, so the `sleep` outlived the bound.
    #[test]
    fn a_jobs_stop_ends_the_foreground_command_rich_ran_for_it() {
        let h = harness(0);
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "Run foreground heartbeat test for 300 seconds", true);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// The review of 9b7b70f64 (rv-20261022T093512Z-9b7b70f6-d496, finding 1): job B, open but
    /// only waiting for his decision since before A's command started, kept A's command out of
    /// A's Stop ("A foreground still running after 2015 ms: true"), even a command naming no
    /// job. B is left as it was. RED at 9b7b70f64: any other open job made the command nobody's.
    #[test]
    fn a_jobs_stop_ends_its_foreground_command_while_another_job_waits_for_a_decision() {
        let h = harness(0);
        let waiting = assignment::register(&h.state, &Registration { obligation_id: "waiting-for-him".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &waiting.id, AssignmentState::Blocked, "Which branch first?").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "Run the heartbeat", false);
        let other = assignment::read(&h.state, "depot", "thread-one", &waiting.id).unwrap();
        assert_eq!((other.state, other.user_stop_requested_at_ms), (AssignmentState::Blocked, None), "the waiting job is not touched");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// Stu runs one foreground command (a real `sleep`) beside a helper's; Stop on `job` must
    /// end Stu's within the reap walk's bound, with only that command's `stop_task`.
    /// With `front_desk`, the command is Rich's, made in the turn that gave the job (his hook
    /// evidence says so), and the Stop reaches his session instead of Stu's.
    fn stop_ends_the_foreground_command(h: &Harness, job: &str, description: &str, front_desk: bool) {
        let (client, dir) = crate::native::native_driver_tests::fixture_client("stop-foreground-command", &r#"
here=$(dirname "$0")
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
sleep 300 &
command=$!
echo "$command" > "$here/command.pid"
printf '%s\n' '{"type":"assistant","message":{"id":"msg_stu","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_fg","name":"Bash","input":{"command":"python3 heartbeat.py beat stop-fg /tmp/reap-walk --seconds 300","description":"DESCRIPTION"}}],"usage":{"input_tokens":0,"output_tokens":1}},"parent_tool_use_id":null}'
printf '%s\n' '{"type":"assistant","message":{"id":"msg_helper","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_helper","name":"Bash","input":{"command":"sleep 1"}}],"usage":{"input_tokens":0,"output_tokens":1}},"parent_tool_use_id":"toolu_worker"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"bhelper","tool_use_id":"toolu_helper","is_backgrounded":false}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"bforeground","tool_use_id":"toolu_fg","is_backgrounded":false}'
read -r stop
case "$stop" in *'"subtype":"stop_task","task_id":"bforeground"'*) ;; *) exit 9;; esac
kill "$command"
wait "$command" 2>/dev/null
id=$(printf '%s' "$stop" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{}}}"
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bforeground","tool_use_id":"toolu_fg","status":"stopped"}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn"}'
read -r keep_alive
"#.replace("DESCRIPTION", description));
        let handle = client.cancel_handle();
        let turn = std::thread::spawn(move || {
            let ended = client.prompt("run the job's commands", &mut |_| {});
            (client, ended)
        });
        until("Stu's foreground command never started", || handle.own_commands().len() == 1);
        assert_eq!(handle.own_commands()[0].task_id, "bforeground", "a helper's command is not Stu's own");
        assert_eq!(handle.own_commands()[0].description, description, "the job travels in the call's own description");
        let pid: i32 = std::fs::read_to_string(dir.join("command.pid")).unwrap().trim().parse().unwrap();
        let alive = || unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(), "the stand-in command must be running before Stop");
        let stopper: Arc<dyn TurnCancel> = handle.clone();
        let desk = if front_desk {
            let record = assignment::read(&h.state, "depot", "thread-one", job).unwrap();
            let evidence = h.state.join("evidence/rich-session");
            std::fs::create_dir_all(&evidence).unwrap();
            let call = |tool: &str, reference: &str| serde_json::json!({"schema": 1,
                "instruction": {"ledger_ref": reference, "sha256": record.instruction_sha256},
                "callback": {"session_id": "rich-session", "hook_event_name": "PreToolUse", "tool_name": "Bash",
                    "tool_use_id": tool, "tool_input": {"command": "python3 heartbeat.py"}}}).to_string() + "\n";
            // A long unrelated row first, then this turn's call and another turn's.
            std::fs::write(evidence.join("callbacks.jsonl"), "x".repeat(1024 * 1024 + 7) + "\n"
                + &call("toolu_fg", &record.instruction_ledger_ref) + &call("toolu_other_turn", "ledger:thread-one:turn-8")).unwrap();
            Some(("rich-session".to_string(), stopper))
        } else {
            let backend = Backend::new();
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.lease_session = Some("stu-session".into());
                inner.peer_stopper = Some(("stu-session".into(), stopper));
            }
            h.host.backends.lock().unwrap().insert("thread-one".into(), backend);
            None
        };
        let pressed = std::time::Instant::now();
        h.host.stop_assignment_and_front_desk("depot", "thread-one", job, desk).unwrap();
        // The reap walk's bound (reap-walk.py BOUND_MS), from the press.
        while alive() && pressed.elapsed() < std::time::Duration::from_millis(2000) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let gone_after = pressed.elapsed();
        if alive() {
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("the job's foreground command (pid {pid}) was still running {gone_after:?} after Stop");
        }
        let (_client, ended) = turn.join().unwrap();
        assert_eq!(ended.unwrap(), "end_turn", "Stu's turn goes on past the stopped command; it is not interrupted");
        let stopped = assignment::read(&h.state, "depot", "thread-one", job).unwrap();
        assert_eq!(stopped.state, AssignmentState::Interrupted);
        assert_eq!(stopped.detail, "The job's recorded work stopped. Stu has been notified.");
    }

    /// Which commands a job's Stop takes. A command naming the job is its, one naming another
    /// job never is, whatever else is open; one naming no job is its only when no other job
    /// could be running it.
    #[test]
    fn a_jobs_stop_takes_its_own_commands_and_never_another_jobs() {
        let h = harness(0);
        let other = assignment::register(&h.state, &Registration { obligation_id: "obligation-70".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &other.id, AssignmentState::Running, "Working.").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        let read = |id: &str| assignment::read(&h.state, "depot", "thread-one", id).unwrap();
        let all = || assignment::read_all(&h.state, "depot", "thread-one").unwrap();
        let at = read(&job.id).registered_at_ms;
        let command = |task: &str, started_at_ms: u64, description: &str| crate::steering::OwnCommand {
            task_id: task.into(), started_at_ms, description: description.into() };
        let commands = vec![
            command("names-this-job", at, "obligation-7: build the site"),
            command("names-the-other-job", at, "Tests for obligation-70"),
            command("names-no-job", at, "List the files"),
            command("before-this-job", at.saturating_sub(1), "List the files"),
        ];
        let taken = |records: &[assignment::Assignment]| job_commands(records, &job.id, &commands);
        // The other job runs: only the command naming this job ("obligation-7" is not in "obligation-70").
        assert_eq!(taken(&all()), vec!["names-this-job".to_string()]);
        // Its Stop takes its own command, and the unnamed one from before this job existed,
        // when it was the only job open; none of this job's.
        assert_eq!(job_commands(&all(), &other.id, &commands), vec!["names-the-other-job".to_string(), "before-this-job".to_string()],
            "the other job's Stop takes its own commands and none of this job's");
        // The other job waits for his decision since before the commands: it runs none of them.
        assignment::advance(&h.state, "depot", "thread-one", &other.id, AssignmentState::Blocked, "Which branch first?").unwrap();
        let since = read(&other.id).updated_at_ms;
        let later = vec![command("names-no-job", since, "List the files"), command("names-the-other-job", since, "Tests for obligation-70")];
        assert_eq!(job_commands(&all(), &job.id, &later), vec!["names-no-job".to_string()]);
        // A command naming no job that started while the other job still ran stays with Stu.
        assert!(!taken(&all()).contains(&"names-no-job".to_string()));
        // The other job has closed: its command is still never this job's.
        assignment::record_user_stop(&h.state, "depot", "thread-one", &other.id).unwrap();
        assert_eq!(job_commands(&all(), &job.id, &later), vec!["names-no-job".to_string()]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }
}
