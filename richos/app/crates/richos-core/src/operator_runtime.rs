//! THE OPERATOR HOST'S SEAMS, FOR REAL — his engine's scripts, the lead launcher with the
//! claim, closing an obligation through the engine, and what is said to him, held on disk
//! (operator back-end spec r3 (b), (c), (d), (e), (q); r4 §2.1).
//!
//! `operator_host.rs` decides; this file carries the decisions out against the real things.
//! Every engine script runs from the DECLARED root (r3 (b), B9), seated in his entity, with his
//! stored environment built from empty (`operator_declaration::script_environment`), bounded in
//! time, in its own process group, and owned by pid: nothing here is ever selected by name.
use crate::ecs::EcsBridge;
use crate::operator_claim::{AppClaim, ClaimPlace, Kernel, ProcessId, ProcessTable};
use crate::operator_declaration::{script_environment, Declaration, EngineFenceStatus, FenceStatus, Refusal};
use crate::operator_host::{AgentLiveness, ConversationKey, ConversationPaths, Lane, LeadHandle, LeadLauncher,
                           OperatorDelivery, OperatorEngine, Say, Settle};
use crate::operator_lead::{ControlRoute, InterruptReply, LeadError, LeadSink, OperatorLead, Quit, TaskBook,
                           CONTROL_TIMEOUT, HANDSHAKE_TIMEOUT, QUIT_GRACE};
use crate::operator_profile::{mcp_config, LeadStart, OperatorProfile, SessionEnvironment};
use crate::operator_report::{write_scope, ReportScope};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// =============================================================================================
// his engine's scripts
// =============================================================================================

/// One engine script's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRun {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run `<engine>/scripts/<script> args…` from his entity, with his environment and nothing of
/// this app's, for at most `timeout`. Owned: its own group, killed by pid on every exit path.
pub fn run_engine_script(declaration: &Declaration, script: &str, args: &[&str], extra: &[(&str, &str)],
                         timeout: Duration) -> Result<ScriptRun, String> {
    let path = declaration.engine_root.join("scripts").join(script);
    if !path.is_file() {
        return Err(format!("{script} is missing from the engine"));
    }
    let mut command = Command::new("/bin/bash");
    command.arg(&path).args(args)
        .current_dir(&declaration.entity_root)
        .env_clear()
        .envs(script_environment(declaration))
        .env("CLAUDE_PROJECT_DIR", &declaration.entity_root)
        .envs(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())))
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::owned_process::OwnedChild::configure(&mut command);
    let mut child = command.spawn().map_err(|e| format!("{script} could not be started ({e})"))?;
    let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        // Unreadable output (not UTF-8) is kept as a marker, so a caller parsing it refuses
        // rather than reading a silently truncated answer.
        if let Err(e) = out.by_ref().take(1024 * 1024).read_to_string(&mut text) {
            text.push_str(&format!("\n[output unreadable: {e}]"));
        }
        text
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        // Unreadable output (not UTF-8) is kept as a marker, so a caller parsing it refuses
        // rather than reading a silently truncated answer.
        if let Err(e) = err.by_ref().take(256 * 1024).read_to_string(&mut text) {
            text.push_str(&format!("\n[output unreadable: {e}]"));
        }
        text
    });
    let mut child = crate::owned_process::OwnedChild::new(child);
    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() >= deadline => {
                // Either fails only when the script has just exited on its own, which is fine.
                child.kill().ok();
                child.wait().ok();
                return Err(format!("{script} did not answer within {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("{script} could not be waited for ({e})")),
        }
    };
    Ok(ScriptRun { code, stdout: reader.join().unwrap_or_default(), stderr: errors.join().unwrap_or_default() })
}

/// The fenced repositories his `orchestration.config` declares (`OPERATOR_FENCES_REPOS`,
/// space-separated, `~` his home), the list the engine's own fence installer reads.
pub fn fenced_repositories(declaration: &Declaration) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(declaration.entity_root.join("orchestration.config")).unwrap_or_default();
    let mut raw = String::new();
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("OPERATOR_FENCES_REPOS=") {
            raw = value.split('#').next().unwrap_or("").trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }
    raw.split_whitespace().map(|p| match p.strip_prefix("~/") {
        Some(rest) => declaration.home.join(rest),
        None => PathBuf::from(p),
    }).collect()
}

/// [`OperatorEngine`] against his real engine.
pub struct EngineScripts {
    declaration: Declaration,
}

impl EngineScripts {
    pub fn new(declaration: Declaration) -> Self {
        EngineScripts { declaration }
    }
}

impl OperatorEngine for EngineScripts {
    fn stop_words(&self, names: &[String], words: &str) -> Result<String, String> {
        let entity = self.declaration.entity_root.display().to_string();
        let mut args: Vec<&str> = names.iter().map(String::as_str).collect();
        args.extend(["--entity", entity.as_str(), "--ceo-word", words]);
        let run = run_engine_script(&self.declaration, "stop.sh", &args, &[], Duration::from_secs(60))?;
        if run.code == Some(0) { Ok(run.stdout) } else { Err(format!("stop.sh answered {:?}: {}", run.code, run.stderr.trim())) }
    }

    fn liveness(&self, agent_id: &str) -> AgentLiveness {
        let entity = self.declaration.entity_root.display().to_string();
        let run = match run_engine_script(&self.declaration, "agent-liveness.sh",
                                          &["--entity", &entity, "--json", agent_id], &[], Duration::from_secs(30)) {
            Ok(run) => run,
            Err(why) => return AgentLiveness::Indeterminate(why),
        };
        match run.code {
            Some(0) => AgentLiveness::NotAlive,
            Some(10) => AgentLiveness::Alive,
            Some(11) => AgentLiveness::Indeterminate(serde_json::from_str::<Value>(&run.stdout).ok()
                .and_then(|v| v.get("reason").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "the resolver could not tell".into())),
            other => AgentLiveness::Indeterminate(format!("agent-liveness.sh answered {other:?}")),
        }
    }

    fn registry_stop(&self, session_id: &str, name: &str, words: &str) -> Result<String, String> {
        let run = run_engine_script(&self.declaration, "workspaces.sh", &["stop", name, "--why", words],
                                    &[("RICHOS_SESSION_ID", session_id)], Duration::from_secs(60))?;
        if run.code == Some(0) { Ok(run.stdout) } else { Err(format!("workspaces.sh stop answered {:?}: {}", run.code, run.stderr.trim())) }
    }

    fn held_leases(&self) -> Vec<String> {
        let mut out = Vec::new();
        for repo in fenced_repositories(&self.declaration) {
            let repo = repo.display().to_string();
            if let Ok(run) = run_engine_script(&self.declaration, "land-lease.sh", &["status", "--repo", &repo], &[],
                                               Duration::from_secs(30)) {
                out.extend(run.stdout.lines().filter(|l| l.contains("held by")).map(str::to_string));
            }
        }
        out
    }
}

// =============================================================================================
// the launcher: profile, claim, report scope, process, handshake
// =============================================================================================

/// The children of `pid`, straight from the kernel (`proc_listchildpids`), each confirmed to
/// still name `pid` as its parent. No `ps`, no name.
pub fn child_pids(pid: u32) -> Vec<u32> {
    #[cfg(target_os = "macos")]
    {
        let mut buffer = vec![0i32; 256];
        let bytes = (buffer.len() * std::mem::size_of::<i32>()) as libc::c_int;
        // SAFETY: the buffer is writable for `bytes`; the call writes at most that.
        let n = unsafe { libc::proc_listchildpids(pid as libc::pid_t, buffer.as_mut_ptr().cast(), bytes) };
        if n <= 0 {
            return Vec::new();
        }
        // libproc returns a count on current macOS; a byte count on some older ones. Either
        // way the unused tail is zero, and every pid is re-checked below.
        buffer.into_iter().take((n as usize).min(256)).filter(|p| *p > 0).map(|p| p as u32)
            .filter(|child| parent_of(*child) == Some(pid)).collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn parent_of(pid: u32) -> Option<u32> {
    // SAFETY: as in `operator_claim::Kernel`.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let n = unsafe {
        libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, (&mut info as *mut libc::proc_bsdinfo).cast(), size)
    };
    (n == size).then_some(info.pbi_ppid)
}

/// A running lead, with its place in the claim: quitting it removes it from the claim once its
/// supervisor has exited (r3 (e) item 3: the claim goes dead only after that).
pub struct ClaimedLead {
    lead: OperatorLead,
    claim: AppClaim,
    supervisor: Option<ProcessId>,
    claude: Option<ProcessId>,
}

impl LeadHandle for ClaimedLead {
    fn session_id(&self) -> String {
        self.lead.session_id().to_string()
    }
    fn send(&self, text: &str) -> Result<String, LeadError> {
        self.lead.send(text)
    }
    fn send_with_uuid(&self, uuid: &str, text: &str) -> Result<(), LeadError> {
        self.lead.send_with_uuid(uuid, text)
    }
    fn stop_task(&self, task_id: &str) -> Result<(), LeadError> {
        self.lead.stop_task(task_id, CONTROL_TIMEOUT)
    }
    fn interrupt(&self) -> Result<InterruptReply, LeadError> {
        self.lead.interrupt(CONTROL_TIMEOUT)
    }
    fn tasks(&self) -> TaskBook {
        self.lead.tasks()
    }
    fn exited(&self) -> bool {
        self.lead.exited()
    }
    fn quit(&self) -> Quit {
        let quit = self.lead.quit(QUIT_GRACE);
        if let (Some(supervisor), Some(claude)) = (self.supervisor, self.claude) {
            if let Err(refusal) = self.claim.remove_lead(supervisor, claude, &Kernel) {
                // The lead is gone either way; a stale entry is read as dead by its start time.
                eprintln!("operator claim: the ended lead could not be removed ({})", refusal.0);
            }
        }
        quit
    }
}

/// Derives the per-start values (r3 (i)); a trait so a test can supply them.
pub trait SessionValues: Send + Sync {
    fn derive(&self) -> Result<SessionEnvironment, Refusal>;
}

/// `launchctl getenv SSH_AUTH_SOCK` and the OS's per-user temporary folder.
pub struct LaunchdSession;

impl SessionValues for LaunchdSession {
    fn derive(&self) -> Result<SessionEnvironment, Refusal> {
        SessionEnvironment::derive()
    }
}

/// [`LeadLauncher`] for his team: the operator profile, the claim, the report scope, the
/// supervised process and the handshake, in that order. Any step that fails is one sentence.
pub struct ProfileLauncher {
    declaration: Declaration,
    /// This app's executable, run as `--operator-mcp <scope>` for the report tool (c).
    executable: PathBuf,
    /// `<engine-state>`, where the report server checks handles against the register.
    state_root: PathBuf,
    /// The operator log the supervisors append to.
    log: PathBuf,
    claim: Mutex<Option<AppClaim>>,
    route: Arc<dyn ControlRoute>,
    session: Box<dyn SessionValues>,
    fences: Box<dyn FenceStatus + Send + Sync>,
    /// F6: where every lead's `stop` reaches the desk, when the app serves one.
    lead_desk: Option<crate::operator_report::LeadDesk>,
}

impl ProfileLauncher {
    pub fn new(declaration: Declaration, executable: &Path, state_root: &Path, log: &Path, route: Arc<dyn ControlRoute>)
               -> Self {
        Self::with(declaration, executable, state_root, log, route, Box::new(LaunchdSession), Box::new(EngineFenceStatus))
    }

    pub fn with(declaration: Declaration, executable: &Path, state_root: &Path, log: &Path, route: Arc<dyn ControlRoute>,
                session: Box<dyn SessionValues>, fences: Box<dyn FenceStatus + Send + Sync>) -> Self {
        ProfileLauncher { declaration, executable: executable.to_path_buf(), state_root: state_root.to_path_buf(),
                          log: log.to_path_buf(), claim: Mutex::new(None), route, session, fences, lead_desk: None }
    }

    /// **F6: every lead this launcher starts can stop a named agent in any conversation**, through
    /// the desk's socket with the leads' stop-only token (`DeskSocket::serve_with_lead`).
    pub fn with_lead_desk(mut self, desk: Option<crate::operator_report::LeadDesk>) -> Self {
        self.lead_desk = desk;
        self
    }

    /// The claim, taken once per app and adopted after (idempotent).
    fn claim(&self) -> Result<AppClaim, String> {
        let mut held = self.claim.lock().unwrap();
        if let Some(claim) = held.as_ref() {
            return Ok(claim.clone());
        }
        let me = crate::operator_claim::this_process(&Kernel).ok_or("RichOS could not read its own process start, so it cannot claim your team.")?;
        let claim = AppClaim::acquire(&ClaimPlace::from_declaration(&self.declaration), me, &[], &Kernel).map_err(|r| r.0)?;
        *held = Some(claim.clone());
        Ok(claim)
    }

    /// **The report scope one lead is started with.** `claim_id` is this launch's claim, which
    /// is also the app run his questions are witnessed under (F10): an ask one conversation's
    /// lead put to him counts for every lead of this run, and for no other run.
    fn report_scope(&self, key: &ConversationKey, paths: &ConversationPaths, claim_id: &str, session_id: &str)
                    -> ReportScope {
        ReportScope {
            version: 1, outbox: paths.outbox.clone(), attachments: paths.attachments.clone(),
            file_roots: self.declaration.file_roots.clone(), state_root: self.state_root.clone(),
            entity_id: key.entity_id.clone(), thread_id: key.thread_id.clone(), lead: claim_id.to_string(),
            question_context: Some(crate::questions::AskScope { obligation_id:None,
                root: self.state_root.clone(), entity_id: key.entity_id.clone(), thread_id: key.thread_id.clone(),
                turn_id: format!("operator:{session_id}"), asker: "operator:conversation".into(), session_id: session_id.to_string(),
                engine: Some(self.declaration.engine_root.clone()), entity_root: Some(self.declaration.entity_root.clone()),
                app_run: Some(claim_id.to_string()),
            }),
            desk: self.lead_desk.clone(),
        }
    }

    /// Give the claim up at quit, after every lead has quit.
    pub fn release(&self) {
        if let Some(claim) = self.claim.lock().unwrap().take() {
            if let Err(refusal) = claim.release(&Kernel) {
                // A claim left behind names this app's pid and start, which read dead once it exits.
                eprintln!("operator claim: the claim could not be released ({})", refusal.0);
            }
        }
    }
}

/// Even a provider that ignores disallowed tools cannot turn a question into a
/// permission wait. Scope comes from this launched conversation, never tool input.
struct QuestionRoute {
    scope: PathBuf,
    fallback: Arc<dyn ControlRoute>,
}
impl ControlRoute for QuestionRoute {
    fn answer(&self, request: &Value) -> Result<Value, String> {
        if request["subtype"] != "can_use_tool" || request["tool_name"] != "AskUserQuestion" {
            return self.fallback.answer(request);
        }
        let result = crate::operator_report::read_scope(&self.scope).and_then(|scope| {
            let context = scope.question_context.ok_or("The operator question scope is unavailable")?;
            crate::question_tools::convert_vendor_scoped(&context, &request["input"])
        });
        let message = match result {
            Ok(_) => "The questions are recorded for display. Continue work that does not depend on the answer.".to_string(),
            Err(error) => format!("The question was not recorded: {error}. Use richos_operator.report to ask with prepared options."),
        };
        Ok(json!({"behavior":"deny", "message":message}))
    }
}

impl LeadLauncher for ProfileLauncher {
    fn launch(&self, key: &ConversationKey, title: &str, start: &LeadStart, paths: &ConversationPaths,
              sink: Arc<dyn LeadSink>) -> Result<Arc<dyn LeadHandle>, String> {
        let claim = self.claim()?;
        let session = self.session.derive().map_err(|r| r.sentence())?;
        let profile = OperatorProfile::new(self.declaration.clone(), session, claim.claim_id()).map_err(|r| r.sentence())?
            .with_supervisor_files(&self.log, &paths.reap_state);
        std::fs::create_dir_all(&paths.dir).map_err(|e| format!("Your team's folder could not be made ({e})."))?;
        let session_id = match start { LeadStart::New(id) | LeadStart::Resume(id) => id.clone() };
        write_scope(&paths.scope, &self.report_scope(key, paths, claim.claim_id(), &session_id))?;
        let command = profile.command(start, &mcp_config(&self.executable, &paths.scope));
        let lead = OperatorLead::spawn(command, &session_id, sink, Arc::new(QuestionRoute { scope: paths.scope.clone(), fallback: self.route.clone() }))
            .map_err(|e| format!("Your team could not be started ({e})."))?;
        if let Err(e) = lead.initialize(HANDSHAKE_TIMEOUT) {
            let _ = lead.quit(QUIT_GRACE);
            return Err(format!("Your team did not answer when it started ({e})."));
        }
        let supervisor = Kernel.start_of(lead.pid()).map(|start| ProcessId { pid: lead.pid(), start });
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut claude = None;
        while claude.is_none() && Instant::now() < deadline {
            claude = child_pids(lead.pid()).into_iter().next()
                .and_then(|pid| Kernel.start_of(pid).map(|start| ProcessId { pid, start }));
            if claude.is_none() {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        if let (Some(s), Some(c)) = (supervisor, claude) {
            if let Err(refusal) = claim.add_lead(s, c, &session_id, title, &Kernel) {
                let _ = lead.quit(QUIT_GRACE);
                return Err(refusal.0);
            }
        }
        Ok(Arc::new(ClaimedLead { lead, claim, supervisor, claude }))
    }

    fn fences(&self) -> Result<(), String> {
        self.fences.status(&self.declaration)
    }
}

// =============================================================================================
// closing an obligation: the engine's `operator-complete` (zach, contract §2.5)
// =============================================================================================

/// One request to the pinned engine's ECS store: a command and its fields in, the result or a
/// sentence out. [`EcsBridge::request`] in production; a fake store in the tests, so this suite
/// never needs a Python interpreter.
pub type EcsCall = Box<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

/// [`Settle`] through the pinned engine's ECS app adapter (zach's `operator-complete`, contract
/// §2.5), on the CONVERSATION's own seat.
///
/// **The binding is the store's, read at the moment of closing.** The adapter's `fence` refuses
/// any binding that is not the seat's current row (`engine/ecs/adapters/app.py` `fence`:
/// *"stale app binding"*), and the front desk rebinds that row at every turn of his. So a
/// binding kept from the turn that registered the work, or handed over by the spine, is stale
/// by the time his team reports. The seat's own `current` is the one binding that cannot be.
pub struct EcsSettle {
    call: EcsCall,
}

impl EcsSettle {
    pub fn new(bridge: EcsBridge) -> Self {
        Self::with(Box::new(move |command: &str, fields: Value| bridge.request(command, fields).map_err(|e| e.0)))
    }

    pub fn with(call: EcsCall) -> Self {
        EcsSettle { call }
    }

    /// The request body, exactly the contract's fields (engine-rest-2026-09-25.md §2.5).
    pub fn request_body(binding: &Value, obligation_id: &str, source_ref: &str, status: &str, evidence: &[String],
                        answer_text: &str) -> Value {
        json!({"binding": binding, "obligation_id": obligation_id, "source_ref": source_ref, "status": status,
               "evidence": evidence, "answer_text": answer_text})
    }
}

impl Settle for EcsSettle {
    fn complete(&self, key: &ConversationKey, obligation_id: &str, source_ref: &str, status: &str,
                evidence: &[String], answer_text: &str) -> Result<(), String> {
        close_on_conversation_seat(&self.call, &key.thread_id, "operator-complete", |binding| {
            Self::request_body(binding, obligation_id, source_ref, status, evidence, answer_text)
        })
    }
}

/// **Close an obligation through a host-only ECS verb, on the conversation's own seat, at the
/// store's CURRENT binding** — the one derivation [`EcsSettle`] and [`EcsAnsweredClose`] share.
///
/// His seat for this conversation, when the engine has per-thread seats: asked, never assumed,
/// exactly as the front desk asks (`native.rs`'s `ceo_thread_seat`), because an older engine
/// with one cursor would refuse a seat it does not have. The binding is the seat's `current`,
/// because the front desk rebinds that row at every turn of his (`engine/ecs/adapters/app.py`
/// `fence`: *"stale app binding"*). That rebinding between the read and the close is the one
/// race; it is asked once more at the new binding, and never more than once.
fn close_on_conversation_seat(call: &EcsCall, thread_id: &str, command: &str,
                              body: impl Fn(&Value) -> Value) -> Result<(), String> {
    let hello = call("hello", json!({}))?;
    let seats = hello["ceo_thread_seats"] == true && hello["ceo_seat_prefix"].as_str() == Some(crate::ecs::CEO_SEAT_PREFIX);
    let seat = if seats { crate::ecs::ceo_seat(thread_id) } else { None };
    for attempt in 0..2 {
        let current = call("current", crate::ecs::seated_request(seat.as_deref(), json!({})))?;
        let binding = current.get("binding").filter(|b| !b.is_null()).cloned()
            .ok_or("this conversation has no current binding to close it on")?;
        match call(command, crate::ecs::seated_request(seat.as_deref(), body(&binding))) {
            Ok(result) if result.get("obligation_closed").and_then(Value::as_bool) == Some(true) => return Ok(()),
            Ok(result) => return Err(format!("the engine did not close it ({result})")),
            Err(why) if attempt == 0 && why.contains("stale app binding") => continue,
            Err(why) => return Err(why),
        }
    }
    Err("the conversation moved on twice while this was being closed".into())
}

/// [`crate::work_host::AnsweredClose`] through the pinned engine's `answer-complete`: an
/// assignment the product's own back end handled itself closes on the words he was given.
///
/// An engine that does not announce the verb (one older than this app) is told apart from a
/// refusal and leaves the obligation open, which is what every build before this did.
pub struct EcsAnsweredClose {
    call: EcsCall,
}

impl EcsAnsweredClose {
    pub fn with(call: EcsCall) -> Self {
        EcsAnsweredClose { call }
    }
}

impl crate::work_host::AnsweredClose for EcsAnsweredClose {
    fn close_answered(&self, record: &crate::assignment::Assignment, answer: &str) -> Result<(), String> {
        let hello = (self.call)("hello", json!({}))?;
        let announced = hello["commands"].as_array().is_some_and(|all| all.iter().any(|c| c == "answer-complete"));
        if !announced {
            return Err("this engine has no answer-complete, so an answered assignment stays open in it".into());
        }
        // His instruction's ledger row is the source, as it is for `complete`; the register
        // refuses an assignment without one, so the fallback is for a record written before.
        let source = if record.instruction_ledger_ref.trim().is_empty() {
            format!("app-assignment:{}", record.id)
        } else {
            record.instruction_ledger_ref.clone()
        };
        close_on_conversation_seat(&self.call, &record.thread_id, "answer-complete", |binding| {
            json!({"binding": binding, "obligation_id": record.obligation_id, "source_ref": source,
                   "answer_text": answer})
        })
    }
}

// =============================================================================================
// what is said to him, held on disk until his surface takes it
// =============================================================================================

/// One thing said to him about his team, durable until delivered (the same reason the register's
/// notices are records and not events: he may not be looking).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorNotice {
    pub at_ms: u64,
    /// The assignment it is about, or none for the conversation.
    pub handle: Option<String>,
    pub kind: Say,
    pub text: String,
    #[serde(default)]
    pub delivered_at_ms: Option<u64>,
}

/// A live surface to push each notice to as it is said.
pub type NoticePush = Box<dyn Fn(&ConversationKey, &OperatorNotice) + Send + Sync>;

/// [`OperatorDelivery`] that appends to `<operator root>/<entity>/<thread>/notices.jsonl`, and
/// pushes to a live surface when one is attached.
pub struct DurableDelivery {
    root: PathBuf,
    push: Option<NoticePush>,
    lock: Mutex<()>,
}

impl DurableDelivery {
    pub fn new(root: &Path, push: Option<NoticePush>) -> Self {
        DurableDelivery { root: root.to_path_buf(), push, lock: Mutex::new(()) }
    }

    fn path(&self, key: &ConversationKey) -> PathBuf {
        ConversationPaths::under(&self.root, key).dir.join("notices.jsonl")
    }

    /// Everything not yet delivered on this conversation, oldest first, marked delivered.
    pub fn take_pending(&self, key: &ConversationKey) -> Result<Vec<OperatorNotice>, String> {
        let _guard = self.lock.lock().unwrap();
        let path = self.path(key);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
        };
        let mut all: Vec<OperatorNotice> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let now = crate::assignment::now_ms();
        let mut pending = Vec::new();
        for notice in all.iter_mut().filter(|n| n.delivered_at_ms.is_none()) {
            notice.delivered_at_ms = Some(now);
            pending.push(notice.clone());
        }
        if !pending.is_empty() {
            let body: String = all.iter().filter_map(|n| serde_json::to_string(n).ok()).map(|l| l + "\n").collect();
            let tmp = path.with_extension("jsonl.tmp");
            std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())?;
        }
        Ok(pending)
    }
}

impl OperatorDelivery for DurableDelivery {
    fn say(&self, key: &ConversationKey, lane: &Lane, kind: Say, text: &str) {
        let notice = OperatorNotice {
            at_ms: crate::assignment::now_ms(),
            handle: match lane { Lane::Handle(h) => Some(h.clone()), Lane::Conversation => None },
            kind,
            text: text.to_string(),
            delivered_at_ms: None,
        };
        {
            let _guard = self.lock.lock().unwrap();
            let path = self.path(key);
            let saved = serde_json::to_string(&notice).map_err(|e| e.to_string()).and_then(|line| {
                path.parent().map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|_| std::fs::OpenOptions::new().create(true).append(true).open(&path))
                    .and_then(|mut file| {
                        use std::io::Write;
                        writeln!(file, "{line}")
                    })
                    .map_err(|e| e.to_string())
            });
            if let Err(e) = saved {
                // Still pushed below if a surface is attached; never dropped without a trace.
                eprintln!("operator notice: not saved at {} ({e}): {}", path.display(), notice.text);
            }
        }
        if let Some(push) = &self.push {
            push(key, &notice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator_declaration::ClaimPaths;
    use std::collections::BTreeMap;

    struct Fixture {
        root: PathBuf,
        declaration: Declaration,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("operator-runtime-{}", uuid::Uuid::new_v4()));
        let root = { std::fs::create_dir_all(&root).unwrap(); std::fs::canonicalize(&root).unwrap() };
        let home = root.join("home");
        let entity = home.join("ab/femcboost");
        let engine = home.join("ab/richos/richos/engine");
        write(&entity.join("orchestration.config"), "OPERATOR_FENCES=\"on\"\nOPERATOR_FENCES_REPOS=\"~/ab/femcboost ~/ab/richos\"\n");
        let mut environment = BTreeMap::new();
        environment.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        let declaration = Declaration {
            entity_root: entity, engine_root: engine, home: home.clone(),
            claim: ClaimPaths { file: home.join(".claude/state/operator-lead.json"), lock: home.join(".claude/state/operator-lead.lock") },
            permission_mode: "bypassPermissions".into(), environment, origins: vec!["desk-typed".into()],
            file_roots: vec![home.join("ab")], contract_path: home.join("ab/contract.md"), contract_sha256: "0".repeat(64),
        };
        Fixture { root, declaration }
    }

    fn script(f: &Fixture, name: &str, body: &str) {
        write(&f.declaration.engine_root.join("scripts").join(name), body);
    }

    #[test]
    fn vendor_question_is_recorded_without_reaching_the_permission_desk() {
        struct NeverPermission;
        impl ControlRoute for NeverPermission {
            fn answer(&self, _: &Value) -> Result<Value,String> { Err("permission route reached".into()) }
        }
        let f=fixture();
        let scope=f.root.join("scope.json");
        let state=f.root.join("state");
        write_scope(&scope,&ReportScope {
            version:1,outbox:f.root.join("outbox"),attachments:f.root.join("attachments"),
            file_roots:vec![f.root.clone()],state_root:state.clone(),entity_id:"entity".into(),thread_id:"thread".into(),lead:"lead".into(),
            question_context:Some(crate::questions::AskScope { obligation_id:None,
                root:state.clone(),entity_id:"entity".into(),thread_id:"thread".into(),asker:"operator:conversation".into(),
                turn_id:"turn".into(),session_id:"original-session".into(),engine:None,entity_root:None,app_run:None,
            }),
            desk:None,
        }).unwrap();
        let route=QuestionRoute { scope,fallback:Arc::new(NeverPermission) };
        // Nobody in this test ever answers the question, so a route that waited for his
        // answer would never return: returning at all, with the question recorded, is the
        // proof that it did not wait. The `< 1 s` clock that stood here added nothing to that
        // and failed on a busy Mac (audit R9).
        let reply=route.answer(&json!({"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{
            "question":"When should we ship?","options":[{"label":"Today","description":"Earlier fixes"},{"label":"Tomorrow","description":"More testing"}],"multiSelect":false
        }]}})).unwrap();
        assert_eq!(reply["behavior"],"deny");
        assert!(reply["message"].as_str().unwrap().contains("recorded"));
        let questions=crate::questions::Store::new(&state).all().unwrap();
        assert_eq!(questions.len(),1);
        assert_eq!(questions[0].asker,"operator:conversation");
        assert_eq!(questions[0].session_id,"original-session");
        assert!(route.answer(&json!({"subtype":"can_use_tool","tool_name":"Bash"})).unwrap_err().contains("permission route"));
    }

    // ---- engine scripts ------------------------------------------------------------------

    #[test]
    fn liveness_reads_the_resolver_s_exit_code_and_asks_by_agent_id() {
        let f = fixture();
        let seen = f.root.join("asked.txt");
        script(&f, "agent-liveness.sh", &format!(
            "printf '%s\\n' \"$*\" >> '{}'\ncase \"$4\" in alive) exit 10;; gone) exit 0;; unsure) echo '{{\"reason\":\"lock has no pid\"}}'; exit 11;; *) exit 3;; esac\n",
            seen.display()));
        let engine = EngineScripts::new(f.declaration.clone());
        assert_eq!(engine.liveness("alive"), AgentLiveness::Alive);
        assert_eq!(engine.liveness("gone"), AgentLiveness::NotAlive);
        assert_eq!(engine.liveness("unsure"), AgentLiveness::Indeterminate("lock has no pid".into()));
        assert!(matches!(engine.liveness("odd"), AgentLiveness::Indeterminate(w) if w.contains("Some(3)")), "an unknown exit is never a verdict");
        let asked = std::fs::read_to_string(&seen).unwrap();
        assert!(asked.lines().all(|l| l.starts_with(&format!("--entity {} --json ", f.declaration.entity_root.display()))), "{asked}");
    }

    #[test]
    fn the_stop_and_the_registry_step_run_his_scripts_with_his_words_and_the_lead_s_session() {
        let f = fixture();
        let out = f.root.join("calls.txt");
        for name in ["stop.sh", "workspaces.sh"] {
            script(&f, name, &format!("printf '%s|%s|%s\\n' \"$0\" \"$*\" \"${{RICHOS_SESSION_ID:-none}}\" >> '{}'\n", out.display()));
        }
        let engine = EngineScripts::new(f.declaration.clone());
        engine.stop_words(&["mark-sonnet-a".into(), "ray-opus-b".into()], "stop those two").unwrap();
        engine.registry_stop("session-1", "mark-sonnet-a", "stop those two").unwrap();
        let calls = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = calls.lines().collect();
        assert!(lines[0].ends_with(&format!("|mark-sonnet-a ray-opus-b --entity {} --ceo-word stop those two|none",
                                            f.declaration.entity_root.display())), "{}", lines[0]);
        assert!(lines[1].ends_with("|stop mark-sonnet-a --why stop those two|session-1"), "{}", lines[1]);
    }

    #[test]
    fn a_script_runs_with_his_environment_and_nothing_of_this_process() {
        let f = fixture();
        let out = f.root.join("env.txt");
        script(&f, "stop.sh", &format!("/usr/bin/env > '{}'\n", out.display()));
        EngineScripts::new(f.declaration.clone()).stop_words(&["x".into()], "w").unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        let mut names: Vec<&str> = text.lines().filter_map(|l| l.split_once('=').map(|(n, _)| n))
            .filter(|n| !["PWD", "OLDPWD", "SHLVL", "_"].contains(n)).collect();
        names.sort();
        assert_eq!(names, ["CLAUDE_PROJECT_DIR", "HOME", "PATH"], "{text}");
    }

    #[test]
    fn a_hung_script_is_bounded_and_a_missing_one_is_named() {
        let f = fixture();
        // The script sleeps for a hang guard's length (the deadline kills its process group
        // long before), so the only way out before then is the deadline, and "did not
        // answer" names that way. A run that waited for the script would have returned its
        // exit code instead, and `unwrap_err` would fail. That sentence is the verdict; the
        // `< 5 s` clock beside it only added a way to fail on a busy Mac (audit R9).
        script(&f, "slow.sh", "sleep 60\n");
        let err = run_engine_script(&f.declaration, "slow.sh", &[], &[], Duration::from_millis(300)).unwrap_err();
        assert!(err.contains("did not answer"), "{err}");
        assert!(run_engine_script(&f.declaration, "absent.sh", &[], &[], Duration::from_secs(1)).unwrap_err().contains("missing"));
    }

    #[test]
    fn held_leases_are_read_from_every_declared_repository() {
        let f = fixture();
        script(&f, "land-lease.sh", "case \"$3\" in *femcboost) echo 'land-lease: repository x; lease live, held by the RichOS app (\"A\") for 2 min; at rest.';; *) echo 'land-lease: repository y; lease none; at rest.';; esac\n");
        assert_eq!(fenced_repositories(&f.declaration), [f.declaration.home.join("ab/femcboost"), f.declaration.home.join("ab/richos")]);
        let held = EngineScripts::new(f.declaration.clone()).held_leases();
        assert_eq!(held.len(), 1);
        assert!(held[0].contains("held by the RichOS app (\"A\")"));
    }

    // ---- settlement -----------------------------------------------------------------------

    #[test]
    fn the_operator_complete_request_carries_exactly_the_contract_s_fields() {
        let binding = serde_json::to_value(crate::ecs::Binding { entity_id: "femcboost".into(), thread_id: "t".into(),
            session_id: "s".into(), turn_id: "turn".into(), audience: "ceo".into(), revision: 3 }).unwrap();
        let body = EcsSettle::request_body(&binding, "ob-1", "operator-report:claim:1", "completed",
                                           &["git:/r:main:abc".into()], "Done.");
        let mut keys: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["answer_text", "binding", "evidence", "obligation_id", "source_ref", "status"]);
        assert_eq!(body["binding"]["revision"], 3);
    }

    /// A fake ECS store: answers `hello`, `current` and `operator-complete` the way the
    /// engine's adapter does (`engine/ecs/adapters/app.py`), with the conversation's seat row
    /// holding `revisions[i]` on the i-th `current`.
    struct FakeStore {
        seats: bool,
        revisions: Mutex<Vec<Option<u64>>>,
        stale_first: Mutex<bool>,
        calls: Mutex<Vec<(String, Value)>>,
        /// An engine older than `answer-complete` does not announce it.
        older: Mutex<bool>,
    }
    impl FakeStore {
        fn new(seats: bool, revisions: Vec<Option<u64>>, stale_first: bool) -> Arc<Self> {
            Arc::new(FakeStore { seats, revisions: Mutex::new(revisions), stale_first: Mutex::new(stale_first), calls: Mutex::new(Vec::new()),
                                   older: Mutex::new(false) })
        }
        fn binding(revision: u64) -> Value {
            json!({"entity_id": "femcboost", "thread_id": "t-9", "session_id": "desk-s", "turn_id": format!("turn-{revision}"),
                   "audience": "ceo", "revision": revision})
        }
        fn call(self: &Arc<Self>) -> EcsCall {
            let store = self.clone();
            Box::new(move |command: &str, fields: Value| {
                store.calls.lock().unwrap().push((command.to_string(), fields.clone()));
                match command {
                    "hello" => {
                        let commands = if *store.older.lock().unwrap() { vec!["operator-complete"] } else { vec!["operator-complete", "answer-complete"] };
                        Ok(json!({"ceo_thread_seats": store.seats, "ceo_seat_prefix": crate::ecs::CEO_SEAT_PREFIX, "commands": commands}))
                    }
                    "current" => {
                        let mut revisions = store.revisions.lock().unwrap();
                        let next = if revisions.len() > 1 { revisions.remove(0) } else { revisions[0] };
                        Ok(json!({"binding": next.map(Self::binding)}))
                    }
                    "operator-complete" | "answer-complete" => {
                        if std::mem::replace(&mut *store.stale_first.lock().unwrap(), false) {
                            return Err("stale app binding; reconcile in its original scope before retrying".into());
                        }
                        Ok(json!({"obligation_closed": true, "obligation_id": fields["obligation_id"], "duplicate": false}))
                    }
                    other => Err(format!("unexpected command {other}")),
                }
            })
        }
        fn sent(&self, command: &str) -> Vec<Value> {
            self.calls.lock().unwrap().iter().filter(|(c, _)| c == command).map(|(_, f)| f.clone()).collect()
        }
    }

    fn conversation() -> ConversationKey {
        ConversationKey { entity_id: "femcboost".into(), thread_id: "t-9".into() }
    }

    /// **The settlement closes on the conversation's OWN seat, at that seat's live binding**
    /// (zach's contract §2.5: *"Request, on the conversation's own seat (a work seat is
    /// refused)"*; the adapter's `fence` refuses any binding that is not the seat's current
    /// row, and an unseated request is fenced against `ceo-default`). The binding is asked of
    /// the store at the moment of closing, because the front desk rebinds at every turn and a
    /// binding kept from the register's turn would be stale by the time his team reports.
    #[test]
    fn the_settlement_names_the_conversation_s_seat_and_closes_at_its_live_binding() {
        let store = FakeStore::new(true, vec![Some(7)], false);
        let settle = EcsSettle::with(store.call());
        settle.complete(&conversation(), "ob-1", "operator-report:c:1", "completed", &["answer:x".into()], "Done.").unwrap();
        let current = store.sent("current");
        assert_eq!(current.len(), 1);
        assert_eq!(current[0]["seat"], "ceo-thread:t-9", "the live binding is read from the conversation's own seat");
        let complete = store.sent("operator-complete");
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0]["seat"], "ceo-thread:t-9", "the close is fenced on the conversation's own seat: {}", complete[0]);
        assert_eq!(complete[0]["binding"], FakeStore::binding(7));
        assert_eq!(complete[0]["obligation_id"], "ob-1");
    }

    /// An engine without per-thread seats has one cursor, and a seated request would name a
    /// row it does not have: the request carries no seat, as every call did before seats.
    #[test]
    fn an_engine_without_per_thread_seats_is_asked_without_a_seat() {
        let store = FakeStore::new(false, vec![Some(3)], false);
        EcsSettle::with(store.call()).complete(&conversation(), "ob-1", "s", "completed", &["answer:x".into()], "Done.").unwrap();
        assert!(store.sent("current")[0].get("seat").is_none());
        assert!(store.sent("operator-complete")[0].get("seat").is_none());
    }

    /// The front desk rebinding between the read and the close is the one race: the close is
    /// asked once more at the new binding, and never more than once.
    #[test]
    fn a_rebind_between_the_read_and_the_close_is_retried_once_at_the_new_binding() {
        let store = FakeStore::new(true, vec![Some(7), Some(8)], true);
        EcsSettle::with(store.call()).complete(&conversation(), "ob-1", "s", "completed", &["answer:x".into()], "Done.").unwrap();
        let complete = store.sent("operator-complete");
        assert_eq!(complete.len(), 2);
        assert_eq!(complete[1]["binding"], FakeStore::binding(8));
    }

    /// No binding on the seat is a sentence, never a close on some other cursor.
    #[test]
    fn a_conversation_the_store_has_no_binding_for_is_refused_in_a_sentence() {
        let store = FakeStore::new(true, vec![None], false);
        let err = EcsSettle::with(store.call()).complete(&conversation(), "ob-1", "s", "completed", &[], "Done.").unwrap_err();
        assert!(err.contains("no current binding"), "{err}");
        assert!(store.sent("operator-complete").is_empty());
    }

    /// **An assignment the back end handled itself closes through `answer-complete`**, on the
    /// conversation's own seat at its live binding (the same derivation as the settlement
    /// above), carrying the words he was given, its obligation and his instruction's ledger
    /// row, and nothing an answer cannot certify: no status, no evidence list. An engine that
    /// does not announce the verb is told apart and asked nothing.
    #[test]
    fn an_answered_assignment_closes_through_answer_complete_on_the_conversation_s_seat() {
        use crate::work_host::AnsweredClose;
        let dir = std::env::temp_dir().join(format!("richos-answered-close-{}", uuid::Uuid::new_v4().simple()));
        let receipt = crate::assignment::register_kind(&dir, &crate::assignment::Registration {
            entity_id: "femcboost".into(), thread_id: "t-9".into(), obligation_id: "work-1".into(),
            instruction_ledger_ref: "ledger:t-9:turn-4".into(), instruction_sha256: "0".repeat(64),
            title: "run the tests".into(), repositories: vec![], needs_screen: false,
        }, crate::assignment::AssignmentKind::Task).unwrap();
        let record = crate::assignment::read(&dir, "femcboost", "t-9", &receipt.id).unwrap();
        let store = FakeStore::new(true, vec![Some(7), Some(8)], true);
        EcsAnsweredClose::with(store.call()).close_answered(&record, "It ran; 12 passed.").unwrap();
        let sent = store.sent("answer-complete");
        assert_eq!(sent.len(), 2, "a rebind between the read and the close is retried once");
        assert_eq!(sent[1]["seat"], "ceo-thread:t-9");
        assert_eq!(sent[1]["binding"], FakeStore::binding(8));
        let mut keys: Vec<&str> = sent[1].as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["answer_text", "binding", "obligation_id", "seat", "source_ref"]);
        assert_eq!((sent[1]["obligation_id"].as_str(), sent[1]["answer_text"].as_str(), sent[1]["source_ref"].as_str()),
                   (Some("work-1"), Some("It ran; 12 passed."), Some("ledger:t-9:turn-4")));
        assert!(store.sent("operator-complete").is_empty());

        let older = FakeStore::new(true, vec![Some(7)], false);
        *older.older.lock().unwrap() = true;
        let err = EcsAnsweredClose::with(older.call()).close_answered(&record, "x").unwrap_err();
        assert!(err.contains("no answer-complete"), "{err}");
        assert!(older.sent("current").is_empty() && older.sent("answer-complete").is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    // ---- delivery -------------------------------------------------------------------------

    #[test]
    fn what_is_said_is_held_until_taken_and_taken_once() {
        let f = fixture();
        let pushed = Arc::new(Mutex::new(0));
        let count = pushed.clone();
        let delivery = DurableDelivery::new(&f.root.join("operator"),
            Some(Box::new(move |_k: &ConversationKey, _n: &OperatorNotice| *count.lock().unwrap() += 1)));
        let key = ConversationKey { entity_id: "femcboost".into(), thread_id: "t".into() };
        delivery.say(&key, &Lane::Handle("h-1".into()), Say::Outcome, "Landed.");
        delivery.say(&key, &Lane::Conversation, Say::Alarm, "esc-1 waits");
        assert_eq!(*pushed.lock().unwrap(), 2);
        let first = delivery.take_pending(&key).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].handle.as_deref(), Some("h-1"));
        assert_eq!(first[1].kind, Say::Alarm);
        assert!(delivery.take_pending(&key).unwrap().is_empty(), "never told twice");
        delivery.say(&key, &Lane::Conversation, Say::Update, "More.");
        assert_eq!(delivery.take_pending(&key).unwrap().len(), 1);
    }

    // ---- the launcher, end to end with a fake supervisor and a fake claude ------------------

    struct Session;
    impl SessionValues for Session {
        fn derive(&self) -> Result<SessionEnvironment, Refusal> {
            Ok(SessionEnvironment { ssh_auth_sock: "/private/tmp/launchd-fixture/Listeners".into(), tmpdir: "/tmp/".into() })
        }
    }
    struct FencesOn;
    impl FenceStatus for FencesOn {
        fn status(&self, _: &Declaration) -> Result<(), String> { Ok(()) }
    }
    struct Nothing;
    impl LeadSink for Nothing {
        fn event(&self, _: crate::operator_lead::LeadEvent) {}
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_launcher_claims_writes_the_scope_starts_the_lead_and_quit_leaves_the_claim_clean() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        // A supervisor that forks the fake claude as its child, as the real one does.
        let bin = f.root.join("bin");
        write(&f.declaration.engine_root.join("scripts/provider-supervisor.py"),
              "import os, sys, signal\nassert sys.argv[1] == '--reap-descendants'\npid = os.fork()\nif pid == 0:\n    os.execvp(sys.argv[2], sys.argv[2:])\nsignal.signal(signal.SIGTERM, lambda *a: (os.kill(pid, signal.SIGKILL), os._exit(0)))\nos.waitpid(pid, 0)\n");
        let claude = bin.join("claude");
        write(&claude, "#!/bin/sh\nwhile IFS= read -r line; do\n  id=$(printf '%s' \"$line\" | sed -n 's/.*\"request_id\":\"\\([^\"]*\\)\".*/\\1/p')\n  case \"$line\" in *initialize*) printf '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"%s\",\"response\":{}}}\\n' \"$id\";; esac\ndone\n");
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut d = f.declaration.clone();
        // A python3 other than the xcrun shim, as the profile requires (never a silent return:
        // a test that returns is reported `ok`).
        let path = std::env::var("PATH").unwrap_or_default();
        let python_dir = path.split(':').chain(["/opt/homebrew/bin", "/usr/local/bin"])
            .find(|dir| *dir != "/usr/bin" && Path::new(dir).join("python3").is_file())
            .expect("this test needs a python3 other than /usr/bin/python3 (the xcrun shim) on PATH or in Homebrew")
            .to_string();
        d.environment.insert("PATH".into(), format!("{}:{python_dir}:/usr/bin:/bin", bin.display()));
        let desk = crate::operator_report::LeadDesk { socket: f.root.join("desk.sock"), token: "l".repeat(32) };
        let launcher = ProfileLauncher::with(d.clone(), Path::new("/fixture/RichOS"), &f.root.join("engine-state"),
                                             &f.root.join("operator/operator.log"), Arc::new(crate::operator_lead::NoPermissionDesk),
                                             Box::new(Session), Box::new(FencesOn))
            .with_lead_desk(Some(desk.clone()));
        let key = ConversationKey { entity_id: "femcboost".into(), thread_id: "t-1".into() };
        let paths = ConversationPaths::under(&f.root.join("operator"), &key);
        let lead = launcher.launch(&key, "Landing the fix", &LeadStart::New("session-1".into()), &paths, Arc::new(Nothing)).unwrap();
        let scope: Value = serde_json::from_str(&std::fs::read_to_string(&paths.scope).unwrap()).unwrap();
        assert_eq!(scope["thread_id"], "t-1");
        let claim: Value = serde_json::from_str(&std::fs::read_to_string(&d.claim.file).unwrap()).unwrap();
        assert_eq!(claim["owner"], "app");
        assert_eq!(scope["lead"], claim["claim_id"], "the report scope names the lead as the claim does");
        assert_eq!(scope["question_context"]["app_run"], claim["claim_id"],
                   "F10: his questions are witnessed under this launch's claim, the run every lead of it shares");
        assert_eq!(scope["desk"], json!({"socket": desk.socket, "token": desk.token}),
                   "F6: every lead's stop reaches the desk with the leads' token");
        assert_eq!(claim["leads"][0]["title"], "Landing the fix");
        assert_eq!(claim["leads"][0]["session_id"], "session-1");
        assert_eq!(claim["processes"].as_array().unwrap().len(), 3, "app, supervisor, lead: {claim}");
        assert!(matches!(lead.quit(), Quit::Terminated { .. }));
        let claim: Value = serde_json::from_str(&std::fs::read_to_string(&d.claim.file).unwrap()).unwrap();
        assert_eq!(claim["processes"].as_array().unwrap().len(), 1, "only the app remains: {claim}");
        launcher.release();
        assert!(!d.claim.file.exists());
    }
}
