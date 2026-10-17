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
pub const REPOSITORIES_CHANGED: &str = "repositories-connected";
const TEAM_CHANGED_CONTINUATION: &str = r#"{"event":"team_changed","registered":true}"#;
const HANDOFF_ASK: &str = r#"{"event":"session_handoff_requested","max_chars":1500}"#;

#[derive(Default)]
struct Inner {
    peer_input: Option<Arc<dyn TurnCancel>>,
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
    lease_repositories: Option<Vec<PathBuf>>,
    lease_team: Option<String>,
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
    hold_check: Mutex<std::time::Duration>,
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
            hold_check: Mutex::new(std::time::Duration::from_secs(120)),
        })
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
            let host = Arc::downgrade(self);
            let receiving = Arc::downgrade(&backend);
            let delivered_scope = scope.clone();
            let delivered_session = session.clone();
            let delivered_binding = binding.clone();
            if let Some(session) = &session {
                crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),session)?;
            }
            let delivered = input.send_input(&id,&text,Box::new(move |outcome| {
                let receipt = if outcome.result.as_ref().is_ok_and(|reason| reason == "end_turn") {
                    crate::session_tools::acknowledge(&delivered_scope,"stu",through)
                } else {
                    crate::session_tools::accept_live_handoff(&delivered_scope,"stu",through)
                };
                if let Err(error) = receipt { eprintln!("[richos] native input receipt remains pending: {error}"); }
                if let Some(host) = host.upgrade() {
                    if outcome.report_failure {
                        if let Err(error) = host.observe_turn_result(&delivered_binding,Some(&message),
                            Some(&format!("peer:{through}")),delivered_session.as_deref().unwrap_or(""),&outcome.result) {
                            eprintln!("[richos] native input status: {error}");
                        }
                    }
                }
                if let Some(backend) = receiving.upgrade() { backend.wake.notify_all(); }
            }))?;
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
        // This is queue cancellation, not per-tool authority on Stu's session.
        let stopped = || self.handoff_stopped(binding, message);
        // A native connection owns its workers as well as its shell commands.
        let workers_finished = || {
            let session = backend.inner.lock().unwrap().lease_session.clone();
            session.is_none_or(|session| {
                let view = crate::app_workers::status(&self.state, Some(&session));
                view.is_attributed() && view.active + view.liveness_unknown == 0
            })
        };
        // Peer delivery uses the same connection lifecycle as assignment delivery.
        if workers_finished() {
            self.renew_if_repositories_changed(backend, binding);
            self.renew_if_account_changed(backend, binding);
            self.renew_if_model_changed(backend, binding);
        }
        let original = serde_json::to_string(message).map_err(|e| e.to_string())?;
        let mut input = original.clone();
        loop {
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
            let result = lease.prompt(&input, &mut |_item| {
                if !published {
                    published = true;
                    self.publish_peer_input(backend, binding);
                }
            });
            let result =
                result.and_then(|reason| lease.work_readiness_after_turn().map(|_| reason));
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.peer_input = None;
                inner.cancel = None;
            }
            if matches!(&result, Err(CognitionError::Io(_))) {
                drop(guard.take());
                backend.inner.lock().unwrap().peer_active_through=None;
            }
            self.observe_turn_result(binding,Some(message),Some(turn),&session,&result.as_ref().map(Clone::clone).map_err(ToString::to_string))?;
            let reason = result.map_err(|e| e.to_string())?;
            if reason != "end_turn" {
                return Err(format!("Operator input did not complete: {reason}"));
            }
            let lease = guard
                .as_mut()
                .ok_or("The operator connection closed during team reload")?;
            self.project_output(binding, &session);
            if !self.refresh_peer_team(backend, lease.as_mut())? {
                return Ok(session);
            }
            input = TEAM_CHANGED_CONTINUATION.to_string();
        }
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

    fn connected_repositories(&self, binding: &ThreadBinding) -> Option<Vec<PathBuf>> {
        let data_dir = self.state.parent()?;
        let load =
            crate::entity::EntityRegistry::load(&crate::entity::entity_registry_path(data_dir));
        if load.source == crate::entity::RegistrySource::Unreadable {
            return None;
        }
        let mut repositories = load
            .registry
            .get(binding.entity_id())
            .map(|entity| entity.connected_repositories.clone())
            .unwrap_or_default();
        repositories.sort();
        Some(repositories)
    }

    fn observe_lease(self: &Arc<Self>, lease: &mut dyn Cognition, binding: &ThreadBinding) {
        let host = Arc::downgrade(self);
        let binding = binding.clone();
        let session = lease.session_id().to_string();
        lease.set_between_turn_sink(Arc::new(move |records| {
            let Some(host) = host.upgrade() else { return };
            for frame in records {
                if frame["type"] != "result" || frame.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null()) { continue }
                let result = crate::native::native_result(&frame).map_err(|error| error.to_string());
                if let Err(error) = host.observe_turn_result(&binding,None,None,&session,&result) {
                    eprintln!("[richos] native turn status: {error}");
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
        if lease.is_some() {
            return Ok(());
        }
        // The team it registers, read just before it is spawned (see `Inner::lease_team`).
        let (team, opened) = {
            let factory = &self.factory;
            (factory.team_digest(), factory.spawn_work(binding))
        };
        let mut opened = opened.map_err(|e| e.to_string())?;
        self.observe_lease(opened.as_mut(), binding);
        // A provider opened after shutdown started is never installed.
        let closing = {
            let mut inner = backend.inner.lock().unwrap();
            if !inner.closing {
                inner.lease_session = Some(opened.session_id().to_string());
                inner.lease_repositories = self.connected_repositories(binding);
                inner.lease_team = team;
            }
            inner.closing
        };
        if closing {
            drop(lease);
            drop(opened);
            return Err("RichOS is closing down. Nothing was started.".into());
        }
        *lease = Some(opened);
        Ok(())
    }

    fn renew_if_repositories_changed(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        binding: &ThreadBinding,
    ) {
        if backend.lease.lock().unwrap().is_none() {
            return;
        }
        let Some(now) = self.connected_repositories(binding) else {
            return;
        };
        let Some(opened_with) = backend.inner.lock().unwrap().lease_repositories.clone() else {
            return;
        };
        if opened_with == now {
            return;
        }
        let commands = backend
            .lease
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|lease| lease.running_commands());
        if matches!(
            commands,
            Some(
                crate::lease_commands::CommandReading::Running(_)
                    | crate::lease_commands::CommandReading::Unreadable
            )
        ) {
            eprintln!(
                "[richos] back end: a repository was connected since it opened; renewal deferred \
                 while a command it started may still be running"
            );
            return;
        }
        if let Err(why) = self.rotate(backend, binding, REPOSITORIES_CHANGED) {
            eprintln!("[richos] back end: not renewed for the newly connected repository ({why})");
        }
    }

    fn renew_if_account_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        if !self.account_switch_due(backend) {
            return;
        }
        if let Err(why) = self.rotate(backend, binding, crate::spine::ACCOUNT_SWITCH) {
            eprintln!("[richos] back end: not moved to the next Claude account ({why})");
        }
    }

    fn renew_if_model_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        let Some(wanted) = self.factory.lease_models() else {
            return;
        };
        let running_on = backend
            .lease
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|lease| lease.lease_model());
        if running_on.is_none_or(|model| model == wanted.stu)
            || self.renewal_waits_for_a_command(backend, "its model changed in Preferences")
        {
            return;
        }
        if let Err(why) = self.rotate(backend, binding, crate::spine::MODEL_CHANGED) {
            eprintln!("[richos] back end: not moved to the model chosen in Preferences ({why})");
        }
    }

    fn account_switch_due(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota) = self.quota.lock().unwrap().clone() else {
            return false;
        };
        let (account, streamed, commands) = {
            let lease = backend.lease.lock().unwrap();
            let Some(lease) = lease.as_ref() else {
                return false;
            };
            let Some(account) = lease.account() else {
                return false;
            };
            (
                account.to_string(),
                lease.streamed_usage(),
                lease.running_commands(),
            )
        };
        if quota.before_turn(&account, streamed) == account {
            return false;
        }
        if matches!(
            commands,
            Some(
                crate::lease_commands::CommandReading::Running(_)
                    | crate::lease_commands::CommandReading::Unreadable
            )
        ) {
            eprintln!("[richos] back end: its Claude account is to be left; renewal deferred while a command it started may still be running");
            return false;
        }
        true
    }

    fn renewal_waits_for_a_command(&self, backend: &Arc<Backend>, why: &str) -> bool {
        let commands = backend
            .lease
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|lease| lease.running_commands());
        let waits = matches!(
            commands,
            Some(
                crate::lease_commands::CommandReading::Running(_)
                    | crate::lease_commands::CommandReading::Unreadable
            )
        );
        if waits {
            eprintln!("[richos] back end: {why}; renewal deferred while a command it started may still be running");
        }
        waits
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
        }
        loop {
            let live_session = backend.lease.lock().unwrap().as_ref().map(|lease| lease.session_id().to_string());
            let next = {
                let mut inner = backend.inner.lock().unwrap();
                loop {
                    if inner.closing {
                        return;
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
                    inner = backend.wake.wait(inner).unwrap();
                }
            };
            let result = match next {
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
            backend.wake.notify_all();
            if let Err(error) = result {
                eprintln!("[richos] operator input remains saved: {error}");
                if inner.closing {
                    return;
                }
                drop(
                    backend
                        .wake
                        .wait_timeout(inner, *self.hold_check.lock().unwrap())
                        .unwrap(),
                );
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
        backend.wake.notify_all();
        Ok(format!("work-input:{}", delivery.id))
    }

    fn rotate(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        binding: &ThreadBinding,
        reason: &str,
    ) -> Result<(), String> {
        let mut handoff = String::new();
        {
            let mut lease = backend.lease.lock().unwrap();
            if let Some(lease) = lease.as_mut() {
                let said = lease.prompt_context_only(HANDOFF_ASK, &mut |item| {
                    if let TurnItem::Text { text, .. } = item {
                        if handoff.len() < 6000 {
                            handoff.push_str(text);
                        }
                    }
                });
                if said.is_err() {
                    handoff.clear();
                }
            }
        }
        let (team, fresh) = {
            let factory = &self.factory;
            (factory.team_digest(), factory.spawn_work(binding))
        };
        let mut fresh = fresh.map_err(|e| e.to_string())?;
        self.observe_lease(fresh.as_mut(), binding);
        let reports = assignment::read_all(
            &self.state,
            binding.entity_id().as_str(),
            binding.thread_id(),
        )
        .map_err(|e| e.to_string())?;
        let context=serde_json::json!({"event":"session_replaced","reason":reason,"previous_session_summary":handoff,
            "work_records":reports}).to_string();
        fresh
            .reprime(&context, &mut |_| {})
            .map_err(|e| e.to_string())?;
        let session = fresh.session_id().to_string();
        let repositories = self.connected_repositories(binding);
        let mut lease = backend.lease.lock().unwrap();
        let retired = lease.replace(fresh);
        let mut inner = backend.inner.lock().unwrap();
        inner.lease_session = Some(session);
        inner.peer_active_through = None;
        inner.rotations += 1;
        inner.last_rotation_reason = Some(reason.into());
        inner.lease_repositories = repositories;
        inner.lease_team = team;
        drop(inner);
        drop(lease);
        drop(retired);
        Ok(())
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
        let outcomes: Vec<_> = targets
            .iter()
            .map(|(session, tool)| match &stopper {
                Some((owner, handle)) if owner == session => handle.stop_worker(tool),
                _ => CommandStop::NotDelivered,
            })
            .collect();
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
        "The recorded workers stopped. Stu has been notified."
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
        in_turn: VecDeque<Option<String>>,
        turn_results: VecDeque<&'static str>,
    }
    struct Lease {
        session: String,
        prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
    }
    impl Cognition for Lease {
        fn session_id(&self) -> &str {
            &self.session
        }
        fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
            Ok(())
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
                team: self.team.clone(),
            }))
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
        let host = WorkHost::new(
            &state,
            Box::new(factory),
            Arc::new(SilentNotifier),

        );

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
        assert_eq!(prompts.len(),2,"unfinished work was repeatedly redispatched to the same Stu: {prompts:?}");
        assert!(prompts[1].contains("Keep the original release date"));
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Use the connected repository"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(3,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(),3,"a failed turn made accepted work new again: {prompts:?}");
        assert!(prompts[2].contains("Use the connected repository"),"old work was repeated after {result}: {prompts:?}");
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"a turn result replaced live Stu");
        h.host.shutdown();
        let resumed=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();assert_eq!(resumed.1,through);
        let factory=Factory{spawns:h.spawns.clone(),prompts:h.work_prompts.clone(),commands:h.commands.clone(),team:h.team.clone(),gate:h.start_gate.clone()};
        let restarted=WorkHost::new(&h.state,Box::new(factory),Arc::new(SilentNotifier));
        let recovered=if result=="end_turn" {1}else{2};
        restarted.wake_messages(&h.binding);assert!(restarted.wait_for_completed(recovered,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(),3+recovered as usize,"restart repeated input beyond its unfinished receipts: {prompts:?}");
        assert!(prompts[3].contains("Land the three branches"));
        if result!="end_turn" {assert!(prompts[4].contains("Keep the original release date"),"the uncertain correction was lost");}
        assert!(!prompts[3..].iter().any(|p|p.contains("Use the connected repository")),"successfully completed input replayed");
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
    fn every_scheduled_input_is_delivered_once_per_live_stu_despite_a_failed_turn() {
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
                            asker: "stu".into(), set_id: None, text: "Input under test".into(), receipt: None,
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
                let expected = if result == "io_error" {2} else {1};
                assert_eq!(prompts.iter().filter(|p|p.contains("Input under test")).count(),expected,
                    "{kind}/{result}: input repeated in the same live session: {prompts:?}");
                assert_eq!(h.spawns.load(Ordering::SeqCst), if result=="io_error" {2}else{1});
                let mut facts=Vec::new();
                while let Some((message,through))=crate::session_tools::pending(&scope,"rich").unwrap() {
                    assert_eq!(message.from,"work-status");assert!(message.instruction.is_none());
                    facts.push(serde_json::from_str::<serde_json::Value>(&message.text).unwrap());
                    crate::session_tools::acknowledge(&scope,"rich",through).unwrap();
                }
                assert_eq!(facts.len(),usize::from(result!="end_turn"),"{kind}/{result}: wrong failure-report count");
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
                let unfinished = kind == "handoff" || !matches!(result,"end_turn"|"io_error");
                assert_eq!(prompts[before..].iter().filter(|p| p.contains("Input under test")).count(), usize::from(unfinished),
                    "{kind}/{result}: new session lost or repeated recovery input: {prompts:?}");
                assert!(!prompts[before..].iter().any(|p| p.contains("Independent later input")), "completed input replayed");
                std::fs::remove_dir_all(h.root).unwrap();
            }
        }
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
            session:"same-stu".into(),prompts:h.work_prompts.clone(),commands:h.commands.clone(),team:h.team.clone(),
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
            8,
            "same handoff did not continue after registering the new teammate"
        );
        assert!(prompts
            .last()
            .unwrap()
            .starts_with(TEAM_CHANGED_CONTINUATION));
        assert_eq!(
            prompts.last().unwrap(),
            TEAM_CHANGED_CONTINUATION,
            "roster reload must not replay the accepted handoff"
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
}
