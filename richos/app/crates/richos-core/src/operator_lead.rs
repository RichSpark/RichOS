//! THE OPERATOR CLIENT — his lead, driven over stream-json the way the operator probes drove
//! it (operator back-end spec r3 §4 (g), (r), (d), with r4 §1.1-§1.3; richos-hq
//! `docs/plans/2026-09-25-operator-back-end-spec-r4.md`).
//!
//! **Not the product client.** `native.rs` holds one parked prompt, kills its group on any
//! error path and on a Stop, and rotates. None of that is right for his team, whose agents
//! outlive every turn and must never be killed by an error or a Stop of something else
//! (r3 (q) item 1). This client:
//!
//! - **declares `perTaskStopAffordance: true`** in its `initialize` request, because it is
//!   exactly the per-task stop control that field describes: it writes `stop_task` for a named
//!   agent ((d)). The 2.1.282 binary spares running background agents on an interrupt only for
//!   a client that declares it (P3, measured both ways; r4 §1.2);
//! - **never sets `priority`** on anything it writes: `now` was measured to abort the lead's
//!   running turn (P13; r4 §1.3), and `next` is unmeasured;
//! - **writes many messages without waiting**: the CLI queues them and takes them in order
//!   after the running turn (P13's control), so the host relays and never holds ((g));
//! - **never closes the lead's input while an agent of the lead is alive**: with input closed
//!   the binary kills background tasks whatever the declaration says (its own closed-input
//!   exception, r4 §1.2), so [`OperatorLead::close_input`] refuses;
//! - **keeps the lead on every error**: a frame it cannot read, a write that fails, a control
//!   reply it did not expect — each is reported and the process is left alone. Only a positive
//!   signal ends a lead: stdout end-of-file, confirmed by `waitid` ([`OperatorLead::exited`]),
//!   never an inference from silence (continuity §5.2);
//! - **quits by SIGTERM to the supervisor**, never by a group kill first: the supervisor's
//!   `--reap-descendants` sequence needs the SIGTERM to reap the lead's tool shells, which live
//!   in their own process groups (P15; r3 (q) item 2). The group kill is the fallback after
//!   [`QUIT_GRACE`].
//!
//! **What the host learns from the stream, and from where** (every shape is from the recorded
//! frames of the operator probes, richos-hq `docs/verification/2026-09-24-operator-probes/`):
//!
//! | frame | used for |
//! |---|---|
//! | `assistant` `tool_use` named `Agent` or `Task`, with `input.name` | the agent's name, by its `tool_use` id (P12's mapping frame) |
//! | `system/task_started` `{task_id, tool_use_id}` | name → task id, for `stop_task` ((d)) |
//! | `system/task_notification` `{task_id, status}` | the agent's last status (r4 §1.1) |
//! | `system/task_updated` `{task_id, patch: {status}}` | `killed`, which arrives beside `stopped` (P12) |
//! | `user` echoed with a uuid this client sent (`--replay-user-messages`) | which message started a turn (P13) |
//! | `command_lifecycle` `{command_uuid, state}` `completed` with no `started` | a resent message the CLI dropped (P18) |
//! | `result` `{result}` | the turn's end and its final text ((c)) |
//! | `system/hook_response`, `system/informational` | his engine's alarms ([`crate::operator_frames`]) |
pub use crate::agent_tasks::{stop_task_request, AgentTask, TaskBook, TaskStatus};
use crate::operator_frames::{alarms_in, Alarm};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the `initialize` handshake may take. The probes measured it well under a second;
/// a lead that has not answered in a minute is not starting.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a control request (`stop_task`, `interrupt`) may take to be answered. The probes
/// measured the replies in the same frame batch as the effect (P12: 131.754 s both).
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
/// r3 (q) item 2: *"The host waits up to 10 s for the supervisor to exit, and only then falls
/// back to the group kill."* The supervisor's own grace is 5 s (`OPERATOR_REAP_GRACE`), plus up
/// to three reap passes of 0.1 s, so 10 s is its whole sequence with margin.
pub const QUIT_GRACE: Duration = Duration::from_secs(10);
/// One frame's bound. P7's frames at a million tokens of context were tens of megabytes of file
/// text across a whole session; a single line past this is a defect, drained and reported.
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

// =============================================================================================
// the wire, as pure functions: the bytes this client writes are decided here and tested here
// =============================================================================================

/// The handshake (r4 §1.2). `perTaskStopAffordance: true` is truthful for this client: it
/// writes [`stop_task_request`] for one named agent. `hooks: {}` registers no SDK hooks: his
/// hooks are his engine's, loaded natively.
pub fn initialize_request(request_id: &str) -> Value {
    json!({"type": "control_request", "request_id": request_id,
           "request": {"subtype": "initialize", "hooks": {}, "perTaskStopAffordance": true}})
}

/// One message to the lead. **No `priority`, ever** (r4 §1.3): `now` aborts the running turn,
/// and a message taken after the running turn is the behavior the host is built on.
pub fn user_message(uuid: &str, text: &str) -> Value {
    json!({"type": "user", "uuid": uuid,
           "message": {"role": "user", "content": [{"type": "text", "text": text}]}})
}

/// His Esc: end the lead's running turn (r3 (d) item 6). With the declaration above, the lead's
/// background agents survive it (P3).
pub fn interrupt_request(request_id: &str) -> Value {
    json!({"type": "control_request", "request_id": request_id, "request": {"subtype": "interrupt"}})
}

fn control_reply(request_id: &str, body: Result<Value, String>) -> Value {
    match body {
        Ok(response) => json!({"type": "control_response",
                               "response": {"subtype": "success", "request_id": request_id, "response": response}}),
        Err(error) => json!({"type": "control_response",
                             "response": {"subtype": "error", "request_id": request_id, "error": error}}),
    }
}

// =============================================================================================
// turns: which message started each one, and its final text ((c))
// =============================================================================================

/// One turn's end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnEnd {
    /// The uuids of this client's messages the CLI took into this turn (their `--replay-user-
    /// messages` echo). Empty means the platform started the turn itself (P5: a task
    /// notification), which (c) delivers on the lead's own conversation.
    pub started_by: Vec<String>,
    /// `result.result`: the turn's final assistant text, when it has one.
    pub text: Option<String>,
    pub is_error: bool,
    /// `result.subtype` (`success`, `error_during_execution`, …).
    pub subtype: String,
}

/// Pure: feed frames in order.
#[derive(Clone, Debug, Default)]
pub struct TurnTracker {
    sent: HashSet<String>,
    taken: Vec<String>,
    /// This client's messages the CLI said it `started` (`command_lifecycle`).
    started: HashSet<String>,
}

impl TurnTracker {
    pub fn mark_sent(&mut self, uuid: &str) {
        self.sent.insert(uuid.to_string());
    }

    /// **A duplicate the CLI dropped** (design C5, measured by P18 on 2.1.283): its echo came,
    /// then `command_lifecycle` `completed` for it with no `started` before, so no turn ran for
    /// it. A message the CLI runs is `started` before its echo in every recorded run. Returns
    /// the dropped uuid, and takes it out of the turn attribution. A CLI that sends no
    /// lifecycle frames reports no drop, which is today's reading.
    pub fn lifecycle(&mut self, frame: &Value) -> Option<String> {
        if frame.get("type").and_then(Value::as_str) != Some("command_lifecycle") {
            return None;
        }
        let uuid = frame.get("command_uuid").and_then(Value::as_str)?;
        if !self.sent.contains(uuid) {
            return None;
        }
        match frame.get("state").and_then(Value::as_str) {
            Some("started") => {
                self.started.insert(uuid.to_string());
                None
            }
            Some("completed") if !self.started.contains(uuid) && self.taken.iter().any(|u| u == uuid) => {
                self.taken.retain(|u| u != uuid);
                Some(uuid.to_string())
            }
            _ => None,
        }
    }

    /// **The moment the CLI takes one of this client's messages** (its `--replay-user-messages`
    /// echo), before the frame is observed. The host needs it WHILE the turn runs, not only at
    /// its end: an agent started in that turn belongs to the handle the message was about (r3
    /// (d) item 5, "the names the app saw start on that assignment's turns"), and a turn's end
    /// comes after its agents have started.
    pub fn taking(&self, frame: &Value) -> Option<String> {
        if frame.get("type").and_then(Value::as_str) != Some("user") {
            return None;
        }
        let uuid = frame.get("uuid").and_then(Value::as_str)?;
        (self.sent.contains(uuid) && !self.taken.iter().any(|u| u == uuid)).then(|| uuid.to_string())
    }

    pub fn observe(&mut self, frame: &Value) -> Option<TurnEnd> {
        match frame.get("type").and_then(Value::as_str) {
            Some("user") => {
                if let Some(uuid) = frame.get("uuid").and_then(Value::as_str) {
                    if self.sent.contains(uuid) && !self.taken.iter().any(|u| u == uuid) {
                        self.taken.push(uuid.to_string());
                    }
                }
                None
            }
            Some("result") => Some(TurnEnd {
                started_by: std::mem::take(&mut self.taken),
                text: frame.get("result").and_then(Value::as_str).map(str::trim)
                    .filter(|t| !t.is_empty()).map(str::to_string),
                is_error: frame.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                subtype: frame.get("subtype").and_then(Value::as_str).unwrap_or("").to_string(),
            }),
            _ => None,
        }
    }
}

/// Sees the report tool's result arrive, so the host reads the outbox then and not only at the
/// turn's end: a question asked mid-turn must reach him while the lead carries on (r3 (c); the
/// tool's own description tells the lead to ask and continue). Pure: feed frames in order.
#[derive(Clone, Debug, Default)]
pub struct ReportWatch {
    calls: HashSet<String>,
}

impl ReportWatch {
    /// `true` when this frame carries the result of a `report` call.
    pub fn observe(&mut self, frame: &Value) -> bool {
        let blocks = frame.pointer("/message/content").and_then(Value::as_array);
        match frame.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for block in blocks.into_iter().flatten() {
                    if block.get("type").and_then(Value::as_str) == Some("tool_use")
                        && block.get("name").and_then(Value::as_str) == Some(crate::operator_profile::REPORT_TOOL) {
                        if let Some(id) = block.get("id").and_then(Value::as_str) {
                            self.calls.insert(id.to_string());
                        }
                    }
                }
                false
            }
            Some("user") => blocks.into_iter().flatten().any(|block| {
                block.get("type").and_then(Value::as_str) == Some("tool_result")
                    && block.get("tool_use_id").and_then(Value::as_str).is_some_and(|id| self.calls.remove(id))
            }),
            _ => false,
        }
    }
}

// =============================================================================================
// the process
// =============================================================================================

/// What the lead's stream told the host. Delivered on the reader thread, in stream order.
#[derive(Clone, Debug)]
pub enum LeadEvent {
    /// `system/init`, for the init check ((b)).
    Init(Value),
    /// One message his engine addressed to him ((r)); deduplicated by the host, not here.
    Alarm(Alarm),
    /// The CLI took one of this client's messages into the running turn: its uuid, from the
    /// `--replay-user-messages` echo. Delivered before anything that turn starts.
    Took(String),
    /// The CLI took this message and ran nothing for it: a resent uuid the session already
    /// holds as answered (its echo, then `command_lifecycle` `completed` with no `started`;
    /// P18). Follows its [`LeadEvent::Took`]; no turn follows it.
    Dropped(String),
    /// A named agent started, or its status changed (r4 §1.1).
    Agent(AgentTask),
    /// A turn ended ((c)).
    TurnEnded(TurnEnd),
    /// A `report` call returned: its record is in the outbox now, mid-turn or not ((c)).
    Reported,
    /// A frame this client could not read. Reported; the lead is kept (r3 (q) item 1).
    Protocol(String),
    /// A `can_use_tool` permission request arrived, for this tool, and the route answered it.
    /// P14 measured none in bypass mode, so each one is a sign the CLI changed (F17 / Frank
    /// Q3 (a)); the host logs it as a warning.
    PermissionAsked(String),
    /// The lead's stdout ended: a positive signal, never an inference. [`OperatorLead::exited`]
    /// then confirms with `waitid`.
    Ended,
}

/// Where the reader delivers events. Must not block for long: it runs on the reader thread.
pub trait LeadSink: Send + Sync {
    fn event(&self, event: LeadEvent);
}

/// How a control request FROM the lead is answered (`can_use_tool`, and anything else the
/// platform asks). Unsupported requests return the existing protocol error.
pub trait ControlRoute: Send + Sync {
    /// The `response` body for a `can_use_tool`, or `Err` for a request this route cannot answer.
    fn answer(&self, request: &Value) -> Result<Value, String>;
}

/// Unsupported control requests have no human approval route.
pub struct NoPermissionDesk;

impl ControlRoute for NoPermissionDesk {
    fn answer(&self, request: &Value) -> Result<Value, String> {
        Err(format!("RichOS does not answer {} requests on this lead.",
            request.get("subtype").and_then(Value::as_str).unwrap_or("unnamed")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeadError {
    /// The lead's input could not be written. The lead is NOT killed for it.
    Io(String),
    /// A control request got no answer in time.
    Timeout(&'static str),
    /// The lead answered a control request with an error.
    Refused(String),
    /// The lead's stdout has ended.
    Closed,
    /// r4 §1.2: input is never closed while an agent of the lead is alive.
    AgentsAlive(usize),
    /// The process could not be started.
    Spawn(String),
}

impl std::fmt::Display for LeadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "the lead's input could not be written ({e})"),
            Self::Timeout(what) => write!(f, "the lead did not answer {what} in time"),
            Self::Refused(e) => write!(f, "the lead refused: {e}"),
            Self::Closed => write!(f, "the lead's output has ended"),
            Self::AgentsAlive(n) => write!(f, "{n} of the lead's agents are still alive, so its input stays open"),
            Self::Spawn(e) => write!(f, "the lead could not be started ({e})"),
        }
    }
}

/// How a quit went, for the operator log (r3 (q) item 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Quit {
    /// The supervisor ended within the grace after its SIGTERM.
    Terminated { waited: Duration },
    /// It did not; its group was killed.
    GroupKilled,
    /// It had already ended.
    AlreadyEnded,
}

/// The interrupt's answer: the uuids of his messages still queued, which run next (P3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterruptReply {
    pub still_queued: Vec<String>,
}

type Pending = Arc<Mutex<HashMap<String, Sender<Value>>>>;

/// One running lead: the supervisor (its own process group) with `claude` under it.
pub struct OperatorLead {
    pid: u32,
    child: Mutex<Option<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    pending: Pending,
    book: Arc<Mutex<TaskBook>>,
    turns: Arc<Mutex<TurnTracker>>,
    reports: Arc<Mutex<ReportWatch>>,
    closed: Arc<AtomicBool>,
    next: AtomicU64,
    session_id: String,
    ended: Mutex<bool>,
}

impl OperatorLead {
    /// Start `command` (the [`crate::operator_profile::OperatorProfile::command`] invocation, or
    /// a test's fake) in its own process group, and start reading it.
    pub fn spawn(mut command: Command, session_id: &str, sink: Arc<dyn LeadSink>, route: Arc<dyn ControlRoute>)
                 -> Result<Self, LeadError> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // The supervisor requires its own group (`provider-supervisor.py`), and the group is
            // what the fallback kill reaches.
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| LeadError::Spawn(e.to_string()))?;
        let stdin = child.stdin.take().ok_or_else(|| LeadError::Spawn("no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| LeadError::Spawn("no stdout".into()))?;
        let lead = OperatorLead {
            pid: child.id(),
            child: Mutex::new(Some(child)),
            stdin: Arc::new(Mutex::new(Some(stdin))),
            pending: Arc::new(Mutex::new(HashMap::new())),
            book: Arc::new(Mutex::new(TaskBook::default())),
            turns: Arc::new(Mutex::new(TurnTracker::default())),
            reports: Arc::new(Mutex::new(ReportWatch::default())),
            closed: Arc::new(AtomicBool::new(false)),
            next: AtomicU64::new(1),
            session_id: session_id.to_string(),
            ended: Mutex::new(false),
        };
        let (pending, book, turns, reports, closed, writer) = (lead.pending.clone(), lead.book.clone(), lead.turns.clone(),
                                                                lead.reports.clone(), lead.closed.clone(), lead.stdin.clone());
        let label: String = session_id.chars().take(8).collect();
        std::thread::Builder::new()
            .name(format!("richos-operator-lead:{label}"))
            .spawn(move || read_frames(stdout, pending, book, turns, reports, closed, writer, sink, route))
            .map_err(|e| LeadError::Spawn(e.to_string()))?;
        Ok(lead)
    }

    /// The supervisor's pid, which is also its process group.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// A copy of the task book, for (d)'s name mapping and (o)'s read.
    pub fn tasks(&self) -> TaskBook {
        self.book.lock().unwrap().clone()
    }

    fn request_id(&self, what: &str) -> String {
        format!("richos_operator_{what}_{}", self.next.fetch_add(1, Ordering::SeqCst))
    }

    fn control(&self, what: &'static str, frame: Value, id: &str, timeout: Duration) -> Result<Value, LeadError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(LeadError::Closed);
        }
        let (tx, rx) = channel();
        self.pending.lock().unwrap().insert(id.to_string(), tx);
        if let Err(e) = write_frame(&self.stdin, &frame) {
            self.pending.lock().unwrap().remove(id);
            return Err(e);
        }
        let reply = rx.recv_timeout(timeout).map_err(|_| {
            self.pending.lock().unwrap().remove(id);
            if self.closed.load(Ordering::SeqCst) { LeadError::Closed } else { LeadError::Timeout(what) }
        })?;
        match reply.get("subtype").and_then(Value::as_str) {
            Some("success") => Ok(reply.get("response").cloned().unwrap_or(Value::Null)),
            _ => Err(LeadError::Refused(reply.get("error").and_then(Value::as_str).unwrap_or("no reason given").to_string())),
        }
    }

    /// The handshake. Its reply carries his account and is dropped the moment it proves the
    /// lead answered (native.rs's license rule: RichOS never keeps it).
    pub fn initialize(&self, timeout: Duration) -> Result<(), LeadError> {
        let id = self.request_id("init");
        self.control("the handshake", initialize_request(&id), &id, timeout).map(|_| ())
    }

    /// Relay one message. Returns its uuid. Never waits for the lead to be idle.
    pub fn send(&self, text: &str) -> Result<String, LeadError> {
        let uuid = uuid::Uuid::new_v4().to_string();
        self.send_with_uuid(&uuid, text)?;
        Ok(uuid)
    }

    /// Relay one message under a uuid the caller chose. **An answer's retry reuses the uuid of
    /// its first send, into the same session,** so a lead that already has the answer drops the
    /// repeat and only echoes it (the provider's own check against the saved transcript;
    /// richos-hq `docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md` §1.2, C3, measured
    /// by probe P18). A fresh uuid on every call made that check unusable.
    pub fn send_with_uuid(&self, uuid: &str, text: &str) -> Result<(), LeadError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(LeadError::Closed);
        }
        self.turns.lock().unwrap().mark_sent(uuid);
        write_frame(&self.stdin, &user_message(uuid, text))
    }

    /// Stop one task (r3 (d) item 2).
    pub fn stop_task(&self, task_id: &str, timeout: Duration) -> Result<(), LeadError> {
        let id = self.request_id("stop");
        self.control("the stop", stop_task_request(&id, task_id), &id, timeout).map(|_| ())
    }

    /// His Esc (r3 (d) item 6).
    pub fn interrupt(&self, timeout: Duration) -> Result<InterruptReply, LeadError> {
        let id = self.request_id("interrupt");
        let body = self.control("the interrupt", interrupt_request(&id), &id, timeout)?;
        let still_queued = body.get("still_queued").and_then(Value::as_array)
            .map(|q| q.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
        Ok(InterruptReply { still_queued })
    }

    /// Close the lead's input, which ends it after its last turn. **Refused while any agent of
    /// the lead is alive** (r4 §1.2): `alive` is `agent-liveness.sh`'s count for this lead.
    pub fn close_input(&self, alive: usize) -> Result<(), LeadError> {
        if alive > 0 {
            return Err(LeadError::AgentsAlive(alive));
        }
        self.stdin.lock().unwrap().take();
        Ok(())
    }

    /// Has the supervisor exited? A positive answer from `waitid`, without reaping it, so its
    /// pid and group stay reserved until [`Self::quit`].
    pub fn exited(&self) -> bool {
        if *self.ended.lock().unwrap() {
            return true;
        }
        exited_without_reaping(self.pid)
    }

    /// End the lead the way r3 (q) item 2 says: SIGTERM to the supervisor, which reaps the
    /// lead's whole tree, then up to `grace` for it to exit, and only then the group kill.
    pub fn quit(&self, grace: Duration) -> Quit {
        let mut ended = self.ended.lock().unwrap();
        let Some(mut child) = self.child.lock().unwrap().take() else { return Quit::AlreadyEnded };
        if *ended || exited_without_reaping(self.pid) {
            fence_group(self.pid);
            reap(&mut child);
            *ended = true;
            return Quit::AlreadyEnded;
        }
        let began = Instant::now();
        terminate_one(self.pid);
        while began.elapsed() < grace {
            if exited_without_reaping(self.pid) {
                fence_group(self.pid);
                reap(&mut child);
                *ended = true;
                return Quit::Terminated { waited: began.elapsed() };
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        fence_group(self.pid);
        reap(&mut child);
        *ended = true;
        Quit::GroupKilled
    }
}

/// Reap the supervisor. `wait` fails only when it was already reaped, which is the state wanted.
fn reap(child: &mut Child) {
    child.wait().ok();
}

impl Drop for OperatorLead {
    fn drop(&mut self) {
        // The quit path, never the group kill first (r3 (q) item 2).
        let _ = self.quit(QUIT_GRACE);
    }
}

fn terminate_one(pid: u32) {
    #[cfg(unix)]
    // SAFETY: a signal to the pid this process spawned and has not reaped, so the pid cannot
    // have been reused.
    unsafe {
        let _ = libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// SIGKILL the group whose leader is `pid`, while the unreaped leader still reserves the id.
fn fence_group(pid: u32) {
    #[cfg(unix)]
    // SAFETY: as above; the group id is the unreaped leader's pid.
    unsafe {
        let _ = libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

fn exited_without_reaping(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `information` is a zeroed siginfo_t the kernel fills; WNOWAIT leaves the child
        // unreaped.
        let mut information: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(libc::P_PID, pid as libc::id_t, &mut information, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT)
        };
        result == 0 && unsafe { information.si_pid() } != 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn write_frame(stdin: &Arc<Mutex<Option<ChildStdin>>>, frame: &Value) -> Result<(), LeadError> {
    let mut guard = stdin.lock().unwrap();
    let Some(pipe) = guard.as_mut() else { return Err(LeadError::Io("its input is closed".into())) };
    let mut line = serde_json::to_vec(frame).map_err(|e| LeadError::Io(e.to_string()))?;
    line.push(b'\n');
    pipe.write_all(&line).and_then(|_| pipe.flush()).map_err(|e| LeadError::Io(e.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn read_frames(stdout: std::process::ChildStdout, pending: Pending, book: Arc<Mutex<TaskBook>>,
               turns: Arc<Mutex<TurnTracker>>, reports: Arc<Mutex<ReportWatch>>, closed: Arc<AtomicBool>,
               writer: Arc<Mutex<Option<ChildStdin>>>, sink: Arc<dyn LeadSink>, route: Arc<dyn ControlRoute>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let mut oversized = false;
        // Bounded read of one line: an oversized one is drained through its newline and
        // reported, and never read as the frame after it.
        loop {
            let buf = match reader.fill_buf() {
                Ok(buf) => buf,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            if buf.is_empty() {
                break;
            }
            let n = buf.iter().position(|b| *b == b'\n').map(|i| i + 1).unwrap_or(buf.len());
            let ends = buf[n - 1] == b'\n';
            if line.len() + n <= MAX_FRAME_BYTES && !oversized {
                line.extend_from_slice(&buf[..n]);
            } else {
                oversized = true;
            }
            reader.consume(n);
            if ends {
                break;
            }
        }
        if line.is_empty() && !oversized {
            break; // end of file
        }
        if oversized {
            sink.event(LeadEvent::Protocol(format!("a frame over {MAX_FRAME_BYTES} bytes was skipped")));
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let frame: Value = match serde_json::from_str(text) {
            Ok(frame) => frame,
            Err(e) => {
                sink.event(LeadEvent::Protocol(format!("a line that is not JSON was skipped ({e})")));
                continue;
            }
        };
        if !frame.is_object() {
            sink.event(LeadEvent::Protocol("a frame that is not an object was skipped".into()));
            continue;
        }
        match frame.get("type").and_then(Value::as_str) {
            Some("control_response") => {
                let response = frame.get("response").cloned().unwrap_or(Value::Null);
                let id = response.get("request_id").and_then(Value::as_str).unwrap_or("").to_string();
                let waiter = pending.lock().unwrap().remove(&id);
                match waiter {
                    Some(tx) => {
                        // A closed waiter gave up at its timeout and already said so.
                        tx.send(response).ok();
                    }
                    None => sink.event(LeadEvent::Protocol(format!("a reply to a request this client did not make ({id})"))),
                }
                continue;
            }
            Some("control_request") => {
                let request = frame.get("request").cloned().unwrap_or(Value::Null);
                let id = frame.get("request_id").and_then(Value::as_str).unwrap_or("");
                let reply = control_reply(id, route.answer(&request));
                if let Err(e) = write_frame(&writer, &reply) {
                    sink.event(LeadEvent::Protocol(format!("a control request could not be answered ({e})")));
                }
                if request.get("subtype").and_then(Value::as_str) == Some("can_use_tool") {
                    let tool = request.get("tool_name").and_then(Value::as_str).unwrap_or("an unnamed tool");
                    sink.event(LeadEvent::PermissionAsked(tool.to_string()));
                }
                continue;
            }
            _ => {}
        }
        if frame.get("type").and_then(Value::as_str) == Some("system")
            && frame.get("subtype").and_then(Value::as_str) == Some("init") {
            sink.event(LeadEvent::Init(frame.clone()));
        }
        for alarm in alarms_in(&frame) {
            sink.event(LeadEvent::Alarm(alarm));
        }
        let took = turns.lock().unwrap().taking(&frame);
        if let Some(uuid) = took {
            sink.event(LeadEvent::Took(uuid));
        }
        let dropped = turns.lock().unwrap().lifecycle(&frame);
        if let Some(uuid) = dropped {
            sink.event(LeadEvent::Dropped(uuid));
        }
        let changed = book.lock().unwrap().observe(&frame);
        for task in changed {
            sink.event(LeadEvent::Agent(task));
        }
        let reported = reports.lock().unwrap().observe(&frame);
        if reported {
            sink.event(LeadEvent::Reported);
        }
        let end = turns.lock().unwrap().observe(&frame);
        if let Some(end) = end {
            sink.event(LeadEvent::TurnEnded(end));
        }
    }
    closed.store(true, Ordering::SeqCst);
    // Fail every waiter so no caller hangs: a control request can never be answered now.
    pending.lock().unwrap().clear();
    sink.event(LeadEvent::Ended);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tasks::tests::{agent_use, notified, started};
    use std::path::{Path, PathBuf};
    use std::sync::Condvar;

    // ---- the wire -------------------------------------------------------------------------

    #[test]
    fn the_handshake_declares_the_per_task_stop_affordance() {
        let frame = initialize_request("r-1");
        assert_eq!(frame["request"]["subtype"], "initialize");
        assert_eq!(frame["request"]["perTaskStopAffordance"], json!(true),
            "r4 §1.2: without it an interrupt kills his background agents (P3)");
        assert_eq!(frame["request"]["hooks"], json!({}));
    }

    fn has_key(value: &Value, key: &str) -> bool {
        match value {
            Value::Object(map) => map.contains_key(key) || map.values().any(|v| has_key(v, key)),
            Value::Array(items) => items.iter().any(|v| has_key(v, key)),
            _ => false,
        }
    }

    #[test]
    fn nothing_the_client_writes_ever_carries_priority() {
        for frame in [user_message("u", "hello"), stop_task_request("r", "t"), interrupt_request("r"),
                      initialize_request("r"), control_reply("r", Ok(json!({}))), control_reply("r", Err("e".into()))] {
            assert!(!has_key(&frame, "priority"), "r4 §1.3: `now` aborts the running turn (P13): {frame}");
        }
        let m = user_message("u-1", "the words");
        assert_eq!(m["uuid"], "u-1");
        assert_eq!(m["message"]["content"][0]["text"], "the words");
    }

    // ---- turns ----------------------------------------------------------------------------

    #[test]
    fn a_turn_is_attributed_to_the_messages_the_cli_took_and_a_platform_turn_to_none() {
        let mut turns = TurnTracker::default();
        turns.mark_sent("u-1");
        turns.mark_sent("u-2");
        assert!(turns.observe(&json!({"type":"user","uuid":"u-1","message":{}})).is_none());
        // A tool result is also a `user` frame, with a uuid this client never sent.
        assert!(turns.observe(&json!({"type":"user","uuid":"tool-result-x","message":{}})).is_none());
        let end = turns.observe(&json!({"type":"result","subtype":"success","result":"  Done.  ","is_error":false})).unwrap();
        assert_eq!(end.started_by, ["u-1"]);
        assert_eq!(end.text.as_deref(), Some("Done."));
        let platform = turns.observe(&json!({"type":"result","subtype":"success","result":"The teammate finished."})).unwrap();
        assert!(platform.started_by.is_empty(), "P5: a turn the platform started itself");
    }

    #[test]
    fn the_moment_the_cli_takes_a_message_is_seen_before_the_turn_ends_and_only_once() {
        let mut turns = TurnTracker::default();
        turns.mark_sent("u-1");
        let echo = json!({"type":"user","uuid":"u-1","isReplay":true,"message":{}});
        assert_eq!(turns.taking(&echo).as_deref(), Some("u-1"));
        assert!(turns.observe(&echo).is_none());
        assert_eq!(turns.taking(&echo), None, "the same echo again is not a second taking");
        assert_eq!(turns.taking(&json!({"type":"user","uuid":"tool-result-x","message":{}})), None, "not this client's");
        assert_eq!(turns.taking(&json!({"type":"assistant","uuid":"u-1"})), None);
    }

    fn lifecycle(uuid: &str, state: &str) -> Value {
        json!({"type":"command_lifecycle","command_uuid":uuid,"state":state,"uuid":"frame-x","session_id":"s"})
    }

    /// **C5, from P18's recorded frames** (richos-hq `docs/verification/2026-09-27-operator-probe-p18/`,
    /// `P18-A-resumed.jsonl` t=3.146, `P18-C-resumed.jsonl` t=2.247-15.246): a message the CLI runs
    /// goes queued, started, echo, completed; a duplicate it drops goes echo, then completed, with
    /// no started. Only the second is a drop, and the dropped uuid is not the next turn's.
    #[test]
    fn a_duplicate_the_cli_drops_is_told_apart_from_a_message_it_runs() {
        let mut turns = TurnTracker::default();
        turns.mark_sent("dropped");
        turns.mark_sent("ran");
        // The drop: its echo, then completed, with no started.
        let echo = json!({"type":"user","uuid":"dropped","isReplay":true,"message":{}});
        assert_eq!(turns.lifecycle(&echo), None);
        assert_eq!(turns.taking(&echo).as_deref(), Some("dropped"));
        turns.observe(&echo);
        assert_eq!(turns.lifecycle(&lifecycle("dropped", "completed")).as_deref(), Some("dropped"));
        // The run: queued, started, echo, and its completed is no drop.
        for frame in [lifecycle("ran", "queued"), lifecycle("ran", "started")] {
            assert_eq!(turns.lifecycle(&frame), None);
        }
        let echo = json!({"type":"user","uuid":"ran","isReplay":true,"message":{}});
        turns.observe(&echo);
        assert_eq!(turns.lifecycle(&lifecycle("ran", "completed")), None);
        let end = turns.observe(&json!({"type":"result","subtype":"success","result":"MARK-D"})).unwrap();
        assert_eq!(end.started_by, ["ran"], "the dropped uuid is not attributed to the next turn");
        // Not this client's, or a completed never echoed: never a drop.
        assert_eq!(turns.lifecycle(&lifecycle("someone-else", "completed")), None);
        turns.mark_sent("never-echoed");
        assert_eq!(turns.lifecycle(&lifecycle("never-echoed", "completed")), None);
    }

    #[test]
    fn the_report_tool_s_result_is_seen_as_it_arrives() {
        let mut watch = ReportWatch::default();
        let call = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"t-r","name":crate::operator_profile::REPORT_TOOL,"input":{}}]}});
        let other = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"t-b","name":"Bash","input":{}}]}});
        let result = |id: &str| json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}});
        assert!(!watch.observe(&call));
        assert!(!watch.observe(&other));
        assert!(!watch.observe(&result("t-b")), "a Bash result is not a report");
        assert!(watch.observe(&result("t-r")));
        assert!(!watch.observe(&result("t-r")), "each report is seen once");
    }

    // ---- the process, with a fake lead ------------------------------------------------------

    struct Collect {
        events: Mutex<Vec<LeadEvent>>,
        wake: Condvar,
    }
    impl Collect {
        fn new() -> Arc<Self> {
            Arc::new(Collect { events: Mutex::new(Vec::new()), wake: Condvar::new() })
        }
        fn wait_for(&self, pred: impl Fn(&LeadEvent) -> bool, timeout: Duration) -> Option<LeadEvent> {
            let deadline = Instant::now() + timeout;
            let mut events = self.events.lock().unwrap();
            loop {
                if let Some(e) = events.iter().find(|e| pred(e)) {
                    return Some(e.clone());
                }
                let left = deadline.checked_duration_since(Instant::now())?;
                events = self.wake.wait_timeout(events, left).unwrap().0;
            }
        }
    }
    impl LeadSink for Collect {
        fn event(&self, event: LeadEvent) {
            self.events.lock().unwrap().push(event);
            self.wake.notify_all();
        }
    }

    struct Fixture {
        root: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("operator-lead-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    /// A fake lead: answers the handshake, a stop and an interrupt; records every byte written
    /// to it; runs `extra` shell before the loop.
    fn fake(root: &Path, extra: &str) -> Command {
        let input = root.join("input.jsonl");
        let script = format!(r#"{extra}
while IFS= read -r line; do
  printf '%s\n' "$line" >> '{input}'
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"subtype":"initialize"'*) printf '{{"type":"control_response","response":{{"subtype":"success","request_id":"%s","response":{{"account":{{"email":"x"}}}}}}}}\n' "$id" ;;
    *'"subtype":"stop_task"'*) printf '{{"type":"control_response","response":{{"subtype":"success","request_id":"%s","response":{{}}}}}}\n' "$id" ;;
    *'"subtype":"interrupt"'*) printf '{{"type":"control_response","response":{{"subtype":"success","request_id":"%s","response":{{"still_queued":["q-1","q-2"]}}}}}}\n' "$id" ;;
    *'"type":"user"'*) printf '{{"type":"result","subtype":"success","result":"echo","is_error":false}}\n' ;;
  esac
done
"#, input = input.display());
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        command
    }

    fn written(root: &Path) -> Vec<Value> {
        std::fs::read_to_string(root.join("input.jsonl")).unwrap_or_default().lines()
            .filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    fn alive(pid: u32) -> bool {
        // SAFETY: signal 0 only asks whether the pid exists.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    #[test]
    fn the_bytes_on_the_wire_declare_the_affordance_and_carry_no_priority() {
        let f = fixture();
        let sink = Collect::new();
        let lead = OperatorLead::spawn(fake(&f.root, ""), "s-1", sink.clone(), Arc::new(NoPermissionDesk)).unwrap();
        lead.initialize(Duration::from_secs(10)).unwrap();
        let uuid = lead.send("first").unwrap();
        lead.send("second, without waiting").unwrap();
        sink.wait_for(|e| matches!(e, LeadEvent::TurnEnded(_)), Duration::from_secs(10)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while written(&f.root).len() < 3 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let lines = written(&f.root);
        assert_eq!(lines[0]["request"]["perTaskStopAffordance"], json!(true), "{lines:?}");
        assert_eq!(lines[1]["uuid"], uuid.as_str());
        assert_eq!(lines.len(), 3, "two messages written back to back, none held: {lines:?}");
        assert!(lines.iter().all(|l| !has_key(l, "priority")));
        assert!(matches!(lead.quit(Duration::from_secs(5)), Quit::Terminated { .. }));
    }

    /// Design §2.2 and C3: a retry goes out under the uuid its first send used, and the CLI's
    /// echo of that uuid is seen as the lead taking it.
    #[test]
    fn a_chosen_uuid_goes_on_the_wire_and_its_echo_is_a_taking() {
        let f = fixture();
        let sink = Collect::new();
        let echo = r#"printf '%s\n' '{"type":"user","uuid":"answer-uuid-1","isReplay":true,"message":{}}'"#;
        let lead = OperatorLead::spawn(fake(&f.root, ""), "s-10", sink.clone(), Arc::new(NoPermissionDesk)).unwrap();
        lead.send_with_uuid("answer-uuid-1", "His answer.").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while written(&f.root).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(written(&f.root)[0]["uuid"], "answer-uuid-1");
        assert!(!has_key(&written(&f.root)[0], "priority"));
        drop(lead);
        // A lead whose stream echoes that uuid: the reader reports it as taken.
        let g = fixture();
        let sink2 = Collect::new();
        let lead = OperatorLead::spawn(fake(&g.root, &format!("read -r first; {echo}")), "s-11", sink2.clone(),
                                       Arc::new(NoPermissionDesk)).unwrap();
        lead.send_with_uuid("answer-uuid-1", "His answer.").unwrap();
        let took = sink2.wait_for(|e| matches!(e, LeadEvent::Took(u) if u == "answer-uuid-1"), Duration::from_secs(10));
        assert!(took.is_some(), "{:?}", sink2.events.lock().unwrap());
        let _ = sink;
    }

    #[test]
    fn a_bad_frame_is_reported_and_the_lead_is_kept() {
        let f = fixture();
        let sink = Collect::new();
        let noise = r#"printf 'this is not json\n[1,2]\n{"type":"control_response","response":{"subtype":"success","request_id":"nobody"}}\n'"#;
        let lead = OperatorLead::spawn(fake(&f.root, noise), "s-2", sink.clone(), Arc::new(NoPermissionDesk)).unwrap();
        for needle in ["not JSON", "not an object", "did not make"] {
            let got = sink.wait_for(|e| matches!(e, LeadEvent::Protocol(t) if t.contains(needle)), Duration::from_secs(10));
            assert!(got.is_some(), "{needle}");
        }
        lead.initialize(Duration::from_secs(10)).expect("the lead answers after three bad frames");
        assert!(alive(lead.pid()), "r3 (q) item 1: an error never ends a lead");
        assert!(!lead.exited());
    }

    #[test]
    fn stop_task_and_interrupt_are_answered_and_the_queue_is_read() {
        let f = fixture();
        let sink = Collect::new();
        let lead = OperatorLead::spawn(fake(&f.root, ""), "s-3", sink.clone(), Arc::new(NoPermissionDesk)).unwrap();
        lead.initialize(Duration::from_secs(10)).unwrap();
        lead.stop_task("a79a88c2", Duration::from_secs(10)).unwrap();
        let reply = lead.interrupt(Duration::from_secs(10)).unwrap();
        assert_eq!(reply.still_queued, ["q-1", "q-2"]);
        let lines = written(&f.root);
        let stop = lines.iter().find(|l| l["request"]["subtype"] == "stop_task").unwrap();
        assert_eq!(stop["request"]["task_id"], "a79a88c2");
    }

    #[test]
    fn input_is_never_closed_while_an_agent_is_alive() {
        let f = fixture();
        let lead = OperatorLead::spawn(fake(&f.root, ""), "s-4", Collect::new(), Arc::new(NoPermissionDesk)).unwrap();
        assert_eq!(lead.close_input(2), Err(LeadError::AgentsAlive(2)));
        lead.initialize(Duration::from_secs(10)).expect("input still open after the refusal");
        assert_eq!(lead.close_input(0), Ok(()));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !lead.exited() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(lead.exited(), "the fake ends when its input closes");
        assert_eq!(lead.quit(Duration::from_secs(1)), Quit::AlreadyEnded);
    }

    #[test]
    fn quit_sends_sigterm_first_and_kills_the_group_only_after_the_grace() {
        let f = fixture();
        // Ignores SIGTERM, so only the group kill ends it.
        let lead = OperatorLead::spawn(fake(&f.root, "trap '' TERM"), "s-5", Collect::new(), Arc::new(NoPermissionDesk)).unwrap();
        lead.initialize(Duration::from_secs(10)).unwrap();
        let began = Instant::now();
        assert_eq!(lead.quit(Duration::from_millis(400)), Quit::GroupKilled);
        assert!(began.elapsed() >= Duration::from_millis(400), "the grace was honored before the kill");
        assert!(!alive(lead.pid()));
        // And one that honors SIGTERM ends by it.
        let g = fixture();
        let lead = OperatorLead::spawn(fake(&g.root, ""), "s-6", Collect::new(), Arc::new(NoPermissionDesk)).unwrap();
        lead.initialize(Duration::from_secs(10)).unwrap();
        assert!(matches!(lead.quit(Duration::from_secs(5)), Quit::Terminated { .. }));
    }

    #[test]
    fn the_end_of_output_is_a_positive_event_and_fails_waiting_requests() {
        let f = fixture();
        let sink = Collect::new();
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("read -r line; exit 0");
        let lead = OperatorLead::spawn(command, "s-8", sink.clone(), Arc::new(NoPermissionDesk)).unwrap();
        let _ = f;
        let err = lead.initialize(Duration::from_secs(10)).unwrap_err();
        assert_eq!(err, LeadError::Closed);
        assert!(sink.wait_for(|e| matches!(e, LeadEvent::Ended), Duration::from_secs(10)).is_some());
        assert_eq!(lead.send("late"), Err(LeadError::Closed));
    }

    #[test]
    fn the_stream_feeds_the_book_the_turns_and_the_alarms() {
        let f = fixture();
        let sink = Collect::new();
        let frames = [
            json!({"type":"system","subtype":"init","tools":[],"plugins":[]}),
            agent_use("toolu_a", "mark-sonnet-x1"),
            started("t-a", "toolu_a"),
            json!({"type":"system","subtype":"hook_response","hook_event":"Stop","hook_name":"Stop","stdout":"{\"systemMessage\":\"esc-1 waits\"}","stderr":""}),
            notified("t-a", "completed"),
            json!({"type":"result","subtype":"success","result":"The teammate finished."}),
        ];
        let emit: String = frames.iter().map(|v| format!("printf '%s\\n' '{}'\n", v.to_string().replace('\'', "'\\''"))).collect();
        let lead = OperatorLead::spawn(fake(&f.root, &emit), "s-9", sink.clone(), Arc::new(NoPermissionDesk)).unwrap();
        assert!(sink.wait_for(|e| matches!(e, LeadEvent::Init(_)), Duration::from_secs(10)).is_some());
        assert!(sink.wait_for(|e| matches!(e, LeadEvent::Alarm(a) if a.text == "esc-1 waits"), Duration::from_secs(10)).is_some());
        let done = sink.wait_for(|e| matches!(e, LeadEvent::Agent(a) if a.status == TaskStatus::Completed), Duration::from_secs(10));
        assert!(done.is_some());
        let end = sink.wait_for(|e| matches!(e, LeadEvent::TurnEnded(_)), Duration::from_secs(10)).unwrap();
        let LeadEvent::TurnEnded(end) = end else { unreachable!() };
        assert!(end.started_by.is_empty());
        assert_eq!(lead.tasks().latest("mark-sonnet-x1").unwrap().status, TaskStatus::Completed);
    }
}
