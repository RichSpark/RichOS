//! **THE HELP SESSION**: one draft, its conversation, its restricted Claude Code process and its
//! delivery, driven from the window's commands (PRD §2, §4, §7, §8; round 39 §2 to §6).
//!
//! Every step runs off the window's thread and reports through [`Emit`]: a JSON event the window
//! renders. Every conversational line is also kept in the draft and saved, so a quit, a crash or
//! a closed window loses nothing (PRD §2.6), and a reopened draft shows the conversation again
//! without re-running anything (round 39's unresolved point 4: Resume investigation is offered,
//! nothing re-runs on its own).
//!
//! The Claude Code process is RichDoctor's own ([`adapter::Session`]), started for a help session
//! and ended when the window closes, an operation is canceled or runs past 90 seconds, or
//! RichDoctor quits. A fresh process is primed with the draft's own sanitized record (PRD §4:
//! "RichDoctor owns the sanitized conversation/evidence record and supplies it to a fresh
//! process"); no CLI session is ever resumed.
use richdoctor_core::adapter::{self, Failure, Next, Session, Spec, StreamEvent};
use richdoctor_core::collect::{self, Category, Scope, Window};
use richdoctor_core::delivery::{self, Destination, NotSent, State, Upload};
use richdoctor_core::report::{self, Provenance, Report};
use richdoctor_core::scrub::{self, Context};
use richdoctor_core::store::{Delivery, Draft, Entry, SaveError, Shot, Stage, Store};
use richdoctor_core::{evidence, install, mcp, util, when};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

/// What the open question is, so an answer goes to the right place.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    None,
    /// RichDoctor's own: which of two installed copies.
    Which,
    /// RichDoctor's own: about when.
    When,
    /// RichDoctor's own: about what time (after "Enter a time…").
    Rough,
    /// Claude's question.
    Claude(String),
}

struct Inner {
    draft: Option<Draft>,
    pending: Pending,
    operation: u64,
    /// The message the last Claude operation sent, for Retry and Resume investigation.
    last_message: Option<String>,
    stopper: Option<adapter::Stopper>,
    /// The screenshots approved since Claude's last message, for its next one: (media type, base64).
    pictures: Vec<(String, String)>,
    unsaved: Option<SaveError>,
    /// The per-image list of the Send report in progress (round 40 §5), in manifest order.
    upload: Vec<UploadRow>,
    /// The Cancel of the Send report in progress.
    cancel_upload: Option<Arc<AtomicBool>>,
    /// Report created and screenshots unconfirmed: RichDoctor is still asking (up to 2 minutes).
    confirming: bool,
}

/// One row of the per-image list: Waiting, a real percentage, Received, Received earlier, Stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UploadRow {
    id: String,
    sent: u64,
    total: u64,
    /// "wait", "live", "ok", "earlier", "stopped"
    st: &'static str,
}

fn rows_json(rows: &[UploadRow]) -> Value {
    Value::Array(rows.iter().map(|r| json!({"id": r.id, "sent": r.sent, "total": r.total, "st": r.st})).collect())
}

/// How long RichDoctor keeps asking whether the support team can open every screenshot once
/// the report exists (round 40: "which takes up to two minutes"), and how often.
const CONFIRM_FOR: Duration = Duration::from_secs(120);
const CONFIRM_EVERY: Duration = Duration::from_secs(15);

/// The fixed facts of this Mac and this run.
pub struct Env {
    pub store: Store,
    pub home: PathBuf,
    pub exe: PathBuf,
    pub claude: Option<PathBuf>,
    pub dest: Result<Destination, String>,
}

pub struct Engine {
    inner: Arc<Mutex<Inner>>,
    session: Arc<Mutex<Option<Session>>>,
    canceled: Arc<AtomicBool>,
    /// A cancel made by Prepare report now: the operation ends without the "canceled" card.
    quiet: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    env: Arc<Env>,
    emit: Emit,
}

impl Clone for Engine {
    fn clone(&self) -> Self {
        Engine { inner: self.inner.clone(), session: self.session.clone(), canceled: self.canceled.clone(), quiet: self.quiet.clone(), busy: self.busy.clone(), env: self.env.clone(), emit: self.emit.clone() }
    }
}

fn ctx(env: &Env) -> Context {
    Context::gather(&env.home)
}

impl Engine {
    pub fn new(env: Env, emit: Emit) -> Engine {
        Engine {
            inner: Arc::new(Mutex::new(Inner { draft: None, pending: Pending::None, operation: 0, last_message: None, stopper: None, pictures: Vec::new(), unsaved: None, upload: Vec::new(), cancel_upload: None, confirming: false })),
            session: Arc::new(Mutex::new(None)),
            canceled: Arc::new(AtomicBool::new(false)),
            quiet: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
            env: Arc::new(env),
            emit,
        }
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    fn send(&self, v: Value) {
        (self.emit)(v)
    }

    /// Whether a help session is open: a draft that is being worked on, or work not saved.
    pub fn busy(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        self.busy.load(Ordering::SeqCst) || inner.unsaved.is_some() || inner.draft.as_ref().is_some_and(|d| matches!(d.stage, Stage::Investigating | Stage::Review))
    }

    // -----------------------------------------------------------------------------------
    // saving
    // -----------------------------------------------------------------------------------

    /// Save the current draft; report the storage state to the window either way.
    fn save(&self, inner: &mut Inner) {
        let Some(d) = inner.draft.as_mut() else { return };
        match self.env.store.save(d, util::now_millis()) {
            Ok(()) => {
                if inner.unsaved.take().is_some() {
                    self.send(json!({"t": "storage", "ok": true}));
                }
            }
            Err(e) => {
                self.send(json!({"t": "storage", "ok": false, "sentence": e.sentence(), "disk_full": e.disk_full}));
                inner.unsaved = Some(e);
            }
        }
    }

    /// Append a conversational line to the draft, save, and show it.
    fn line(&self, entry: Entry) {
        let v = entry_event(&entry);
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(d) = inner.draft.as_mut() {
                d.conversation.push(entry);
            }
            self.save(&mut inner);
        }
        self.send(v);
    }

    fn draft_dir(&self) -> Option<PathBuf> {
        self.inner.lock().unwrap().draft.as_ref().map(|d| self.env.store.draft_dir(&d.id))
    }

    // -----------------------------------------------------------------------------------
    // the first screen
    // -----------------------------------------------------------------------------------

    /// What the first screen needs: the installation line, whether Claude Code was found, a
    /// draft to resume.
    pub fn first_screen(&self) -> Value {
        let found = install::richos(&self.env.home);
        let resume = self.env.store.list().into_iter().find(|d| d.stage != Stage::Approved && !d.description.trim().is_empty());
        json!({
            "installation": install::installation_line(&found),
            "ambiguous": install::ambiguous(&found),
            "claude_found": self.env.claude.is_some(),
            "resume": resume.map(|d| json!({"id": d.id, "description": d.description, "saved_at": util::local_clock(d.updated_at), "left": d.left})),
        })
    }

    /// **CONTINUE** or **PREPARE WITHOUT AI** on the first screen.
    pub fn start(&self, description: String, no_diagnostics: bool, without_ai: bool) -> Result<(), String> {
        let now = util::now_millis();
        let mut draft = Draft::new(now).map_err(|e| e.to_string())?;
        draft.description = description.trim().to_string();
        draft.no_diagnostics = no_diagnostics;
        draft.without_ai = without_ai;
        draft.stage = Stage::Investigating;
        draft.left = "draft, during the investigation".into();
        draft.window = Some(Window::unknown(now));
        if draft.description.is_empty() {
            return Err("Describe what went wrong first.".into());
        }
        {
            let mut inner = self.inner.lock().unwrap();
            self.stop_session_locked(&mut inner);
            inner.draft = Some(draft);
            inner.pending = Pending::None;
            inner.last_message = None;
            inner.pictures.clear();
            inner.operation = 0;
        }
        self.canceled.store(false, Ordering::SeqCst);
        self.write_scope();
        self.line(Entry::Person { text: description.trim().to_string() });
        let this = self.clone();
        std::thread::spawn(move || this.after_start());
        Ok(())
    }

    fn after_start(&self) {
        let (without_ai, ambiguous) = {
            let inner = self.inner.lock().unwrap();
            let d = inner.draft.as_ref().unwrap();
            (d.without_ai, install::ambiguous(&install::richos(&self.env.home)))
        };
        self.send(json!({"t": "sub", "text": if without_ai { "Preparing without AI" } else { "Investigating with your Claude Code account" }}));
        if without_ai {
            self.without_ai();
            return;
        }
        if self.env.claude.is_none() {
            self.ai_failed(Failure::NotInstalled);
            return;
        }
        if ambiguous {
            let found = install::richos(&self.env.home);
            let mut choices: Vec<String> = found.iter().map(|f| format!("{} in {}", f.version.as_deref().unwrap_or("Unknown version"), f.place.words().replace("your user", "my user"))).collect();
            choices.push("Not sure".into());
            self.ask(Pending::Which, when::WHICH_QUESTION, choices);
            return;
        }
        self.ask(Pending::When, when::QUESTION, when::CHOICES.iter().map(|s| s.to_string()).collect());
    }

    fn ask(&self, pending: Pending, text: &str, choices: Vec<String>) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pending = pending;
            if let Some(d) = inner.draft.as_mut() {
                d.questions.push((text.to_string(), None));
            }
        }
        self.line(Entry::Question { text: text.to_string(), choices });
    }

    /// Record the answer to the open question (the last one asked).
    fn record_answer(&self, text: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(d) = inner.draft.as_mut() {
            if let Some(last) = d.questions.iter_mut().rev().find(|q| q.1.is_none()) {
                last.1 = Some(text.to_string());
            }
        }
    }

    fn write_scope(&self) {
        let (dir, window, diagnostics) = {
            let inner = self.inner.lock().unwrap();
            let Some(d) = inner.draft.as_ref() else { return };
            (self.env.store.draft_dir(&d.id), d.window.unwrap_or_else(|| Window::unknown(util::now_millis())), !d.no_diagnostics)
        };
        let scope = Scope { home: self.env.home.clone(), window, claude_bin: self.env.claude.clone(), diagnostics };
        if let Err(e) = mcp::write_scope(&dir, &scope) {
            let mut inner = self.inner.lock().unwrap();
            let err = SaveError { disk_full: util::is_disk_full(&e), message: e.to_string() };
            self.send(json!({"t": "storage", "ok": false, "sentence": err.sentence(), "disk_full": err.disk_full}));
            inner.unsaved = Some(err);
        }
    }

    // -----------------------------------------------------------------------------------
    // answers
    // -----------------------------------------------------------------------------------

    /// **AN ANSWER** (a chip or the answer field).
    pub fn answer(&self, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() || self.busy.load(Ordering::SeqCst) {
            return;
        }
        let pending = self.inner.lock().unwrap().pending.clone();
        self.line(Entry::Person { text: text.clone() });
        match pending {
            Pending::Which => {
                self.record_answer(&text);
                self.ask(Pending::When, when::QUESTION, when::CHOICES.iter().map(|s| s.to_string()).collect());
            }
            Pending::When if text == when::ENTER => {
                self.record_answer(&text);
                {
                    // "Enter a time…" is not an answer to the time; the follow-up is.
                    let mut inner = self.inner.lock().unwrap();
                    if let Some(d) = inner.draft.as_mut() {
                        if let Some(last) = d.questions.last_mut() {
                            last.1 = Some("asked for a time".into());
                        }
                    }
                }
                self.ask(Pending::Rough, when::ROUGH_QUESTION, when::ROUGH.iter().map(|s| s.to_string()).collect());
            }
            Pending::When | Pending::Rough => {
                self.record_answer(&text);
                let (window, words) = when::window_for(&text, util::now_millis());
                {
                    let mut inner = self.inner.lock().unwrap();
                    inner.pending = Pending::None;
                    if let Some(d) = inner.draft.as_mut() {
                        d.window = Some(window);
                        d.when = words;
                    }
                    self.save(&mut inner);
                }
                self.write_scope();
                let first = self.first_message();
                self.claude(first);
            }
            Pending::Claude(_) | Pending::None => {
                self.record_answer(&text);
                self.inner.lock().unwrap().pending = Pending::None;
                let (clean, _) = scrub::scrub(&text, &ctx(&self.env));
                self.claude(format!("The person answered: \"{clean}\""));
            }
        }
    }

    /// The first message to Claude: the incident, already cleaned of private details.
    fn first_message(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let d = inner.draft.as_ref().unwrap();
        let c = ctx(&self.env);
        let (desc, _) = scrub::scrub(&d.description, &c);
        let found = install::richos(&self.env.home);
        let which = d.questions.iter().find(|q| q.0 == when::WHICH_QUESTION).and_then(|q| q.1.clone());
        json!({
            "event": "new_incident",
            "description": desc,
            "when": d.when,
            "installation": install::installation_line(&found),
            "which_richos": which,
            "diagnostics_allowed": !d.no_diagnostics,
            "next": "Investigate with your tools if they help, then ask your first question or say you are ready.",
        })
        .to_string()
    }

    /// The record a fresh Claude Code process starts from after a cancel, a timeout, a crash or
    /// a restart: the conversation so far and the evidence listing, all already sanitized.
    fn replay(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let Some(d) = inner.draft.as_ref() else { return String::new() };
        let c = ctx(&self.env);
        let lines: Vec<Value> = d
            .conversation
            .iter()
            .filter_map(|e| match e {
                Entry::Person { text } => Some(json!({"person": scrub::scrub(text, &c).0})),
                Entry::Doctor { text } => Some(json!({"you": text})),
                Entry::Question { text, .. } => Some(json!({"you_asked": text})),
                Entry::Done { text } => Some(json!({"done": text})),
                _ => None,
            })
            .collect();
        let evidence: Vec<Value> = evidence::Store::at(&self.env.store.draft_dir(&d.id)).all().iter().map(|i| i.listing()).collect();
        json!({"event": "conversation_restarted", "note": "RichDoctor started a new Claude Code process for the same report. This is the record so far, already cleaned of private details. Do not repeat answered questions.", "when": d.when, "record": lines, "evidence_already_collected": evidence}).to_string()
    }

    // -----------------------------------------------------------------------------------
    // one Claude operation
    // -----------------------------------------------------------------------------------

    fn claude(&self, message: String) {
        let this = self.clone();
        std::thread::spawn(move || this.claude_now(message, false));
    }

    /// Run one operation: send `message`, show progress, and show Claude's turn (or, when
    /// `report` is set, return its drafted report). Bounded by [`adapter::OPERATION_BOUND`].
    fn claude_now(&self, message: String, report: bool) -> Option<String> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return None;
        }
        self.canceled.store(false, Ordering::SeqCst);
        self.send(json!({"t": "busy", "on": true}));
        let result = self.operation(&message);
        self.busy.store(false, Ordering::SeqCst);
        self.send(json!({"t": "busy", "on": false}));
        match result {
            Ok(text) if report => Some(text),
            Ok(text) => {
                let turn = adapter::parse_turn(&text);
                if !turn.say.is_empty() {
                    self.line(Entry::Doctor { text: turn.say.clone() });
                }
                if turn.ready {
                    self.inner.lock().unwrap().pending = Pending::None;
                    self.send(json!({"t": "ready"}));
                } else if !turn.question.is_empty() {
                    self.ask(Pending::Claude(turn.question.clone()), &turn.question, turn.choices);
                } else {
                    self.send(json!({"t": "answer-open"}));
                }
                None
            }
            Err(None) if self.quiet.swap(false, Ordering::SeqCst) => None,
            Err(None) => {
                self.line(Entry::Note { text: "canceled".into() });
                self.send(json!({"t": "canceled"}));
                None
            }
            Err(Some(failure)) => {
                self.ai_failed(failure);
                None
            }
        }
    }

    /// `Err(None)`: canceled. `Err(Some(f))`: Claude Code could not help.
    fn operation(&self, message: &str) -> Result<String, Option<Failure>> {
        let Some(dir) = self.draft_dir() else { return Err(Some(Failure::Ended)) };
        let Some(bin) = self.env.claude.clone() else { return Err(Some(Failure::NotInstalled)) };
        let (op, picture) = {
            let mut inner = self.inner.lock().unwrap();
            inner.operation += 1;
            inner.last_message = Some(message.to_string());
            (inner.operation, std::mem::take(&mut inner.pictures))
        };
        mcp::write_operation(&dir, op).ok();
        let pictures: Vec<richos_core::bug_report::Picture> = picture.into_iter().map(|(media_type, base64)| richos_core::bug_report::Picture { media_type, base64 }).collect();
        let mut slot = self.session.lock().unwrap();
        let fresh = slot.is_none();
        if fresh {
            let spec = Spec { server_exe: self.env.exe.clone(), draft_dir: dir.clone() };
            let env = adapter::environment(|k| std::env::var(k).ok());
            match Session::start(&bin, &spec, &self.env.store.work_dir(), &env) {
                Ok(s) => {
                    self.inner.lock().unwrap().stopper = Some(s.stopper());
                    *slot = Some(s);
                }
                Err((f, _)) => return Err(Some(f)),
            }
        }
        let session = slot.as_mut().unwrap();
        let text = if fresh && op > 1 { format!("{}\n\n{message}", self.replay()) } else { message.to_string() };
        let line = adapter::user_message_with(&text, &pictures);
        let sent = if fresh { session.begin(&line, Duration::from_secs(30)) } else { session.send(&line).map_err(|e| (Failure::Ended, e.to_string())) };
        if let Err((f, _)) = sent {
            *slot = None;
            return if self.canceled.load(Ordering::SeqCst) { Err(None) } else { Err(Some(f)) };
        }
        let deadline = Instant::now() + adapter::OPERATION_BOUND;
        let mut steps: Vec<String> = Vec::new();
        let ev = evidence::Store::at(&dir);
        let outcome = loop {
            match session.next(deadline.min(Instant::now() + Duration::from_millis(500))) {
                Next::Event(StreamEvent::ToolUse { name, input }) => {
                    let line = adapter::progress_line(&name, &input, |id| ev.get(id).map(|i| i.label));
                    steps.push(line.clone());
                    self.send(json!({"t": "progress", "text": line}));
                }
                Next::Event(StreamEvent::Result { is_error: false, text, .. }) => break Ok(text),
                Next::Event(StreamEvent::Result { is_error: true, text, .. }) => break Err(Some(adapter::classify(&text))),
                Next::Event(_) => {}
                Next::Waiting if self.canceled.load(Ordering::SeqCst) => break Err(None),
                Next::Waiting if Instant::now() >= deadline => break Err(Some(Failure::Timeout)),
                Next::Waiting => {}
                Next::Ended if self.canceled.load(Ordering::SeqCst) => break Err(None),
                Next::Ended => {
                    std::thread::sleep(Duration::from_millis(100));
                    break Err(Some(adapter::classify(&session.stderr_text())));
                }
            }
        };
        if !steps.is_empty() {
            let done = steps.iter().map(|s| folded(s)).collect::<Vec<_>>().join(" · ");
            self.line(Entry::Done { text: done });
        }
        if outcome.is_err() {
            if let Some(mut s) = slot.take() {
                s.stop();
            }
        }
        outcome
    }

    fn ai_failed(&self, failure: Failure) {
        let kept_answers = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(d) = inner.draft.as_mut() {
                d.ai_failure = Some(failure);
            }
            inner.pending = Pending::None;
            inner.draft.as_ref().is_some_and(|d| d.questions.iter().any(|q| q.1.is_some()))
        };
        self.line(Entry::AiFailed { failure });
        self.send(json!({"t": "sub", "text": "Claude Code is not available right now"}));
        let _ = kept_answers;
    }

    /// **CANCEL**: stop the operation now; everything is kept.
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
        let mut inner = self.inner.lock().unwrap();
        if let Some(stopper) = inner.stopper.take() {
            stopper.stop();
        }
        if let Some(d) = inner.draft.as_mut() {
            d.left = "draft, interrupted during the investigation".into();
        }
        self.save(&mut inner);
        if !self.busy.load(Ordering::SeqCst) {
            drop(inner);
            self.line(Entry::Note { text: "canceled".into() });
            self.send(json!({"t": "canceled"}));
        }
    }

    /// **RETRY** after Claude Code failed, or **RESUME INVESTIGATION** after a cancel: the last
    /// message again, on a fresh process primed with the record. Nothing else is re-run.
    pub fn retry(&self) {
        let message = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(d) = inner.draft.as_mut() {
                d.ai_failure = None;
            }
            inner.last_message.clone()
        };
        self.send(json!({"t": "sub", "text": "Investigating with your Claude Code account"}));
        match message {
            Some(m) => self.claude(m),
            None => {
                // Nothing was sent to Claude yet (it failed at the first message).
                let first = self.first_message();
                self.claude(first);
            }
        }
    }

    fn stop_session_locked(&self, inner: &mut Inner) {
        if let Some(stopper) = inner.stopper.take() {
            stopper.stop();
        }
    }

    /// The window closed or RichDoctor quits: end the Claude Code process, keep everything.
    pub fn close(&self) {
        self.canceled.store(true, Ordering::SeqCst);
        let mut inner = self.inner.lock().unwrap();
        self.stop_session_locked(&mut inner);
        self.save(&mut inner);
    }

    // -----------------------------------------------------------------------------------
    // without AI, and preparing the report
    // -----------------------------------------------------------------------------------

    /// Collect `categories` in this process (the same collectors Claude's tool uses), showing
    /// each step, and fold the steps into one line.
    fn collect_here(&self, categories: &[Category]) {
        let Some(dir) = self.draft_dir() else { return };
        let scope = match mcp::read_scope(&dir) {
            Some(s) => s,
            None => return,
        };
        let store = evidence::Store::at(&dir);
        let c = Context::gather(&scope.home);
        let mut done = Vec::new();
        for &cat in categories {
            if !store.of(cat).is_empty() || (!scope.diagnostics && cat != Category::Installation) {
                continue;
            }
            self.send(json!({"t": "progress", "text": cat.progress()}));
            let used = store.input_bytes();
            for item in collect::collect(cat, &scope, &c, used) {
                store.put(&item).ok();
            }
            done.push(cat.done().to_string());
        }
        if !done.is_empty() {
            self.line(Entry::Done { text: done.join(" · ") });
        }
    }

    fn without_ai(&self) {
        let diagnostics = !self.inner.lock().unwrap().draft.as_ref().unwrap().no_diagnostics;
        self.busy.store(true, Ordering::SeqCst);
        self.collect_here(&report::without_ai_categories(diagnostics));
        self.busy.store(false, Ordering::SeqCst);
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(d) = inner.draft.as_mut() {
                for q in report::WITHOUT_AI_QUESTIONS {
                    d.questions.push((q.to_string(), None));
                }
            }
        }
        let text = if diagnostics {
            "Your report is ready to review. It contains your description and what RichDoctor collected on this Mac. Questions Claude would have asked are listed as unknown so you can fill them in."
        } else {
            "Your report is ready to review. It contains your description and the RichOS version; nothing else was collected."
        };
        self.line(Entry::Doctor { text: text.into() });
        self.send(json!({"t": "ready"}));
    }

    /// **PREPARE REPORT NOW** / **REVIEW REPORT**: ask Claude for the draft when it can help,
    /// collect what the no-AI path collects when it cannot, and build the review.
    pub fn prepare(&self) {
        let this = self.clone();
        std::thread::spawn(move || {
            // "Prepare report now is always available" (PRD §2.3): a running operation is ended
            // first, quietly, and its answer is not waited for.
            if this.busy.load(Ordering::SeqCst) {
                this.quiet.store(true, Ordering::SeqCst);
                this.canceled.store(true, Ordering::SeqCst);
                if let Some(stopper) = this.inner.lock().unwrap().stopper.take() {
                    stopper.stop();
                }
                let until = Instant::now() + Duration::from_secs(5);
                while this.busy.load(Ordering::SeqCst) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            this.canceled.store(false, Ordering::SeqCst);
            let (ai, diagnostics) = {
                let inner = this.inner.lock().unwrap();
                let d = inner.draft.as_ref().unwrap();
                (!d.without_ai && d.ai_failure.is_none() && this.env.claude.is_some() && inner.last_message.is_some(), !d.no_diagnostics)
            };
            let drafted = if ai { this.claude_now(adapter::PREPARE_REPORT.into(), true).as_deref().and_then(adapter::parse_drafted) } else { None };
            let provenance = {
                let inner = this.inner.lock().unwrap();
                let d = inner.draft.as_ref().unwrap();
                if drafted.is_some() {
                    Provenance::Claude
                } else if d.without_ai {
                    Provenance::WithoutAi
                } else {
                    Provenance::AiFailed
                }
            };
            if drafted.is_none() {
                this.busy.store(true, Ordering::SeqCst);
                this.collect_here(&report::without_ai_categories(diagnostics));
                this.busy.store(false, Ordering::SeqCst);
            }
            this.build(drafted, provenance);
            this.send(json!({"t": "review"}));
        });
    }

    fn build(&self, drafted: Option<adapter::Drafted>, provenance: Provenance) {
        let mut inner = self.inner.lock().unwrap();
        let Some(d) = inner.draft.as_mut() else { return };
        let dir = self.env.store.draft_dir(&d.id);
        let items = evidence::Store::at(&dir).all();
        let c = ctx(&self.env);
        let report_id = d.report.as_ref().map(|r| r.report_id.clone()).unwrap_or_else(|| delivery::new_report_id().unwrap_or_else(|_| "RD-2222-2222".into()));
        let shots: Vec<String> = d.screenshots.iter().map(|s| s.id.clone()).collect();
        let questions: Vec<(String, Option<String>)> = d.questions.iter().filter(|q| q.1.as_deref() != Some("asked for a time")).cloned().collect();
        let r = Report::build(&report::Input {
            report_id: &report_id,
            description: &d.description,
            answers: &questions,
            when: &d.when,
            evidence: &items,
            drafted: drafted.as_ref(),
            provenance,
            screenshots: &shots,
            relay: self.env.dest.as_ref().map(|d| d.relay.as_str()).unwrap_or(delivery::RELAY),
            ctx: &c,
        });
        d.report = Some(r);
        d.stage = Stage::Review;
        d.left = "draft, ready to review".into();
        self.save(&mut inner);
    }

    // -----------------------------------------------------------------------------------
    // review
    // -----------------------------------------------------------------------------------

    /// The review screen's model (round 39 §4): sections, excerpts, notes, the exact text, what
    /// the last check found, and which mode (a draft, or an edit of an approved report).
    pub fn review(&self) -> Value {
        let inner = self.inner.lock().unwrap();
        let Some(d) = inner.draft.as_ref() else { return Value::Null };
        let Some(r) = d.report.as_ref() else { return Value::Null };
        let mut v = review_view(d, r);
        let facts = self.shot_facts(d);
        let total: u64 = facts.iter().filter_map(|f| f["bytes"].as_u64()).sum();
        v["shots"] = Value::Array(facts);
        v["shots_bytes"] = json!(total);
        if let Some(del) = &d.delivery {
            v["received"] = json!(del.stored.len());
            v["attempt"] = json!(del.approved.manifest.len());
            v["changed"] = json!(!self.unchanged(d));
        }
        v
    }

    /// Each approved screenshot's facts, read from its saved bytes: number, neutral name, size,
    /// dimensions, covers, whether it was reduced to fit.
    fn shot_facts(&self, d: &Draft) -> Vec<Value> {
        use richdoctor_core::image_profile::Entry;
        d.screenshots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let bytes = self.env.store.shot_bytes(&d.id, s)?;
                let e = Entry::of(i as u32 + 1, &bytes).ok()?;
                Some(json!({"n": i + 1, "name": e.filename, "width": e.width, "height": e.height, "bytes": e.bytes, "covers": s.covers, "reduced": s.reduced}))
            })
            .collect()
    }

    /// Whether the current report and screenshots are exactly what was approved under the
    /// earlier ID (round 40 §6: unchanged, Send report resumes; changed, it is a revision).
    fn unchanged(&self, d: &Draft) -> bool {
        let (Some(del), Some(r)) = (&d.delivery, &d.report) else { return false };
        let Ok(manifest) = self.env.store.manifest(d) else { return false };
        let (title, body) = r.render();
        delivery::content_sha256(&title, &body, &manifest) == del.approved.content_sha256
    }

    pub fn edit(&self, section: &str, text: &str) -> Result<Value, String> {
        let c = ctx(&self.env);
        let mut inner = self.inner.lock().unwrap();
        let d = inner.draft.as_mut().ok_or("no report")?;
        d.report.as_mut().ok_or("no report")?.edit(section, text, &c)?;
        self.save(&mut inner);
        let d = inner.draft.as_ref().unwrap();
        Ok(review_view(d, d.report.as_ref().unwrap()))
    }

    pub fn set_excerpt(&self, id: &str, removed: bool) -> Value {
        let mut inner = self.inner.lock().unwrap();
        if let Some(r) = inner.draft.as_mut().and_then(|d| d.report.as_mut()) {
            r.set_excerpt_removed(id, removed);
        }
        self.save(&mut inner);
        let d = inner.draft.as_ref().unwrap();
        review_view(d, d.report.as_ref().unwrap())
    }

    /// **SAVE FOR LATER**: the words round 39 shows, honest about whether it saved.
    pub fn save_later(&self) -> String {
        let mut inner = self.inner.lock().unwrap();
        if let Some(d) = inner.draft.as_mut() {
            d.left = "draft, saved to finish later".into();
        }
        self.save(&mut inner);
        match &inner.unsaved {
            None => "Saved on this Mac. Nothing was sent.".into(),
            Some(e) if e.disk_full => "Could not save: the disk is full. Copy the report or choose another location.".into(),
            Some(_) => "Could not save: RichDoctor could not write to its folder. Copy the report or choose another location.".into(),
        }
    }

    /// The exact reviewed text (for Copy report, Save report and Email support).
    pub fn exact_text(&self) -> Option<(String, String)> {
        let inner = self.inner.lock().unwrap();
        let d = inner.draft.as_ref()?;
        if let Some(del) = &d.delivery {
            return Some((del.approved.report_id.clone(), format!("{}\n\n{}", del.approved.title, del.approved.body)));
        }
        d.report.as_ref().map(|r| (r.report_id.clone(), r.exact()))
    }

    /// The approved screenshots with their neutral names (`screenshot-1.png`), in order: what an
    /// email or a saved package carries (never an original filename or path).
    pub fn image_files(&self) -> Vec<(String, Vec<u8>)> {
        let inner = self.inner.lock().unwrap();
        let Some(d) = inner.draft.as_ref() else { return Vec::new() };
        d.screenshots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let ext = if s.media_type == "image/jpeg" { "jpg" } else { "png" };
                self.env.store.shot_bytes(&d.id, s).map(|b| (format!("screenshot-{}.{ext}", i + 1), b))
            })
            .collect()
    }

    /// **SAVE REPORT AND SCREENSHOTS** into `dir`: the reviewed text and every approved
    /// screenshot, unchanged; as numbered parts of at most 10 MiB when the images need it (PRD
    /// §7). Returns what was written.
    pub fn export(&self, dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
        use richdoctor_core::email;
        let (id, text) = self.exact_text().ok_or("There is no reviewed report yet.")?;
        let files = self.image_files();
        let sizes: Vec<u64> = files.iter().map(|(_, b)| b.len() as u64).collect();
        let parts = if email::needs_parts(&sizes) { email::packages(text.len() as u64, &sizes)? } else { vec![email::Part { number: 1, images: (0..files.len()).collect(), bytes: 0 }] };
        let mut written = Vec::new();
        let many = parts.len() > 1;
        for part in &parts {
            let folder = if many { dir.join(format!("RichOS report {id} part {} of {}", part.number, parts.len())) } else { dir.join(format!("RichOS report {id}")) };
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let cover = email::cover(&id, many.then_some((part.number, parts.len() as u32)), part.images.len());
            std::fs::write(folder.join(format!("RichOS report {id}.txt")), format!("{cover}\nSend to {}.\n\n{text}\n", email::SUPPORT)).map_err(|e| e.to_string())?;
            for &i in &part.images {
                std::fs::write(folder.join(&files[i].0), &files[i].1).map_err(|e| e.to_string())?;
            }
            written.push(folder);
        }
        Ok(written)
    }

    // -----------------------------------------------------------------------------------
    // send, and delivery
    // -----------------------------------------------------------------------------------

    /// **SEND REPORT**: freeze the exact reviewed text and screenshot set under its report ID and
    /// a new receipt, save it BEFORE any request, then deliver (register, upload, complete).
    /// Refused when the last check finds anything, when it cannot be saved, or while an earlier
    /// approval of this report is unresolved. A changed revision first retires the earlier ID at
    /// the relay (PRD §7); only a `retired` answer lets it go.
    pub fn send_report(&self) -> Result<(), String> {
        let dest = self.env.dest.clone()?;
        let earlier = {
            let inner = self.inner.lock().unwrap();
            let d = inner.draft.as_ref().ok_or("no report")?;
            match &d.delivery {
                Some(del) if !del.state.permits_revision() => {
                    return Err("The earlier report may already have reached the team; this edit cannot be sent until that is known.".into());
                }
                Some(del) if del.state.permits_try_again() && self.unchanged(d) => {
                    // Round 40 §6: "Send this report unchanged and RichDoctor resumes with the
                    // missing screenshots" under the same report ID and receipt.
                    let approved = del.approved.clone();
                    let this = self.clone();
                    std::thread::spawn(move || this.deliver(approved, dest, true));
                    return Ok(());
                }
                Some(del) => Some((del.approved.clone(), del.completion_attempted)),
                None => None,
            }
        };
        let this = self.clone();
        std::thread::spawn(move || {
            if let Some((old, completion_attempted)) = earlier {
                let relay = match crate::relay::Https::new() {
                    Ok(r) => r,
                    Err(_) => return this.set_state(&State::NotSent { why: delivery::NotSent::Unreachable, at: util::now_millis() }),
                };
                match delivery::retire(&old, &dest, &relay, util::now_millis()) {
                    Ok(()) => {
                        let mut inner = this.inner.lock().unwrap();
                        if let Some(d) = inner.draft.as_mut() {
                            d.retire_for_revision().ok();
                            // A revision gets a new report ID (round 39: "Sending this one gives it a
                            // new report ID and retires the old one on this Mac").
                            if let Some(r) = d.report.as_mut() {
                                r.report_id = delivery::new_report_id().unwrap_or_else(|_| r.report_id.clone());
                                r.fit();
                            }
                        }
                    }
                    // Refused or lost: the revision stays unsent and the earlier report's real
                    // state is what the person sees.
                    // The V6 handover's clarification 1: no completion was ever sent for the
                    // earlier ID, so no issue can exist. A failed retirement is then only the
                    // connection outcome: the revision stays unsent, and Send report retries the
                    // idempotent retirement. Never "could not be confirmed".
                    Err(State::NotSent { why: delivery::NotSent::Offline, .. }) if !completion_attempted => {
                        return this.send(json!({"t": "send-refused", "why": "Your Mac is not connected to the internet. Your revised report is kept on this Mac and was not sent; choose Send report again when you are online."}));
                    }
                    Err(State::NotSent { why: delivery::NotSent::Unreachable | delivery::NotSent::Busy { .. }, .. }) | Err(State::Checking { .. }) if !completion_attempted => {
                        return this.send(json!({"t": "send-refused", "why": "RichDoctor could not reach the reporting service. Your revised report is kept on this Mac and was not sent; choose Send report again to try."}));
                    }
                    Err(state) => {
                        let shown = if matches!(state, State::Checking { .. }) {
                            delivery::after_check(delivery::check(&old, &dest, &relay, util::now_millis()), &State::Unconfirmed { checked_at: util::now_millis() }, util::now_millis())
                        } else {
                            state
                        };
                        return this.set_state(&shown);
                    }
                }
            }
            match this.approve_and_save(&dest) {
                Ok(approved) => this.deliver(approved, dest, false),
                Err(why) => this.send(json!({"t": "send-refused", "why": why})),
            }
        });
        Ok(())
    }

    /// Freeze and save the current revision, or say why not.
    fn approve_and_save(&self, dest: &Destination) -> Result<delivery::Approved, String> {
        let mut inner = self.inner.lock().unwrap();
        let d = inner.draft.as_mut().ok_or("no report")?;
        let manifest = self.env.store.manifest(d)?;
        let r = d.report.as_ref().ok_or("no report")?;
        let (title, body) = r.render();
        let approved = delivery::approve(&r.report_id, &title, &body, manifest, &dest.relay, util::now_millis(), &self.env.store.installation_id())
            .map_err(|p| p.iter().map(|p| format!("{}: {}", p.item, p.detail)).collect::<Vec<_>>().join(" "))?;
        d.delivery = Some(Delivery { approved: approved.clone(), state: State::Sending, completion_attempted: false, stored: Vec::new() });
        d.stage = Stage::Approved;
        d.left = "approved".into();
        if let Err(e) = self.env.store.save(d, util::now_millis()) {
            // Not durable: nothing is sent (PRD §7: persisted before the first request).
            d.delivery = None;
            d.stage = Stage::Review;
            inner.unsaved = Some(e.clone());
            self.send(json!({"t": "storage", "ok": false, "sentence": e.sentence(), "disk_full": e.disk_full}));
            return Err("The report could not be saved on this Mac, so it was not sent. Copy it or save it somewhere else first.".into());
        }
        Ok(approved)
    }

    fn set_state(&self, state: &State) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(del) = inner.draft.as_mut().and_then(|d| d.delivery.as_mut()) {
            if matches!(state, State::Creating) {
                del.completion_attempted = true;
            }
            del.state = state.clone();
        }
        if matches!(state, State::NotSent { why: NotSent::UploadStopped { .. } | NotSent::Canceled, .. }) {
            for row in inner.upload.iter_mut().filter(|r| r.st == "live") {
                row.st = "stopped";
            }
        }
        if let Some(d) = inner.draft.as_mut() {
            d.left = left_words(state);
        }
        self.save(&mut inner);
        let view = self.view(&inner);
        drop(inner);
        if let Some(v) = view {
            self.send(json!({"t": "delivery", "view": v}));
        }
    }

    fn deliver(&self, approved: delivery::Approved, dest: Destination, retry: bool) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().unwrap();
            inner.cancel_upload = Some(cancel.clone());
            inner.confirming = false;
            let stored = inner.draft.as_ref().and_then(|d| d.delivery.as_ref()).map(|x| x.stored.clone()).unwrap_or_default();
            inner.upload = approved
                .manifest
                .iter()
                .map(|e| {
                    let earlier = stored.contains(&e.attachment_id);
                    UploadRow { id: e.attachment_id.clone(), sent: if earlier { e.bytes } else { 0 }, total: e.bytes, st: if earlier { "earlier" } else { "wait" } }
                })
                .collect();
        }
        let me = self.clone();
        let listener: Arc<dyn Fn(Upload) + Send + Sync> = Arc::new(move |e| me.heard(e));
        let relay = match crate::relay::Https::watched(cancel, listener) {
            Ok(r) => r,
            Err(_) => {
                self.set_state(&State::NotSent { why: delivery::NotSent::Unreachable, at: util::now_millis() });
                return;
            }
        };
        let (store, draft) = {
            let inner = self.inner.lock().unwrap();
            (self.env.store.clone(), inner.draft.clone())
        };
        let Some(draft) = draft else { return };
        // The approved images, read back from the draft and checked against the manifest.
        let loader = |e: &richdoctor_core::image_profile::Entry| {
            let shot = draft.screenshots.get(e.order as usize - 1)?;
            store.shot_bytes(&draft.id, shot).filter(|b| util::sha256_hex(b) == e.sha256)
        };
        let this = self.clone();
        let end = delivery::attempt(&approved, &loader, &dest, &relay, &util::now_millis, &|ms| std::thread::sleep(Duration::from_millis(ms)), &mut |s| this.set_state(s), retry);
        self.inner.lock().unwrap().cancel_upload = None;
        if matches!(end, State::CreatedCheckingImages { .. }) {
            self.confirm_images(&approved, &dest, &relay);
        }
    }

    /// Report created; checking screenshots: ask about the same report every 15 seconds for up
    /// to two minutes. Never sends again; the last answer is what the person sees.
    fn confirm_images(&self, approved: &delivery::Approved, dest: &Destination, relay: &crate::relay::Https) {
        self.inner.lock().unwrap().confirming = true;
        let started = Instant::now();
        let mut current = self.inner.lock().unwrap().draft.as_ref().and_then(|d| d.delivery.as_ref()).map(|x| x.state.clone());
        while started.elapsed() < CONFIRM_FOR {
            std::thread::sleep(CONFIRM_EVERY);
            let Some(now_state) = current.clone() else { break };
            let now = util::now_millis();
            let next = delivery::after_check(delivery::check(approved, dest, relay, now), &now_state, now);
            let done = !matches!(next, State::CreatedCheckingImages { .. });
            if done {
                self.inner.lock().unwrap().confirming = false;
            }
            self.set_state(&next);
            current = Some(next);
            if done {
                return;
            }
        }
        self.inner.lock().unwrap().confirming = false;
        if let Some(s) = current {
            self.set_state(&s);
        }
    }

    /// The per-image list hears the relay: the plan (received earlier), real bytes, receipts.
    fn heard(&self, e: Upload) {
        let mut inner = self.inner.lock().unwrap();
        let mut keep = false;
        match &e {
            Upload::Plan { missing } => {
                let mut stored = Vec::new();
                for r in inner.upload.iter_mut() {
                    if !missing.contains(&r.id) {
                        r.st = if r.st == "ok" { "ok" } else { "earlier" };
                        r.sent = r.total;
                        stored.push(r.id.clone());
                    }
                }
                if let Some(del) = inner.draft.as_mut().and_then(|d| d.delivery.as_mut()) {
                    del.stored = stored;
                }
                keep = true;
            }
            Upload::Bytes { id, sent, total } => {
                if let Some(r) = inner.upload.iter_mut().find(|r| &r.id == id) {
                    r.sent = *sent.min(total);
                    r.total = *total;
                    if r.st == "wait" || r.st == "stopped" {
                        r.st = "live";
                    }
                }
            }
            Upload::Received { id } => {
                if let Some(r) = inner.upload.iter_mut().find(|r| &r.id == id) {
                    r.st = "ok";
                    r.sent = r.total;
                }
                if let Some(del) = inner.draft.as_mut().and_then(|d| d.delivery.as_mut()) {
                    if !del.stored.contains(id) {
                        del.stored.push(id.clone());
                    }
                }
                keep = true;
            }
        }
        if keep {
            self.save(&mut inner);
        }
        let rows = rows_json(&inner.upload);
        drop(inner);
        self.send(json!({"t": "upload", "rows": rows}));
    }

    /// **CANCEL** during Sending screenshots (round 40): the upload in progress ends at once and
    /// nothing that could create the public report is sent. Everything stays on this Mac.
    pub fn cancel_upload(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        let sending = inner.draft.as_ref().and_then(|d| d.delivery.as_ref()).is_some_and(|d| matches!(d.state, State::Sending | State::SendingScreenshots { .. }));
        match (&inner.cancel_upload, sending) {
            (Some(c), true) => {
                c.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    /// The delivery view with the live per-image rows, when a send is in progress or just ended.
    fn view(&self, inner: &Inner) -> Option<Value> {
        let d = inner.draft.as_ref()?;
        let mut v = delivery_view(d);
        if v.is_null() {
            return None;
        }
        if !inner.upload.is_empty() {
            v["rows"] = rows_json(&inner.upload);
        }
        if v["kind"] == "created" && inner.confirming {
            v["kind"] = json!("created-checking");
        }
        let facts = self.shot_facts(d);
        v["shots"] = Value::Array(facts);
        Some(v)
    }

    /// **TRY AGAIN**: the same approved report, ID, receipt, text and images. Only after a
    /// definite "not sent"; images already stored are not sent again.
    pub fn try_again(&self) -> Result<(), String> {
        let dest = self.env.dest.clone()?;
        let approved = {
            let inner = self.inner.lock().unwrap();
            let del = inner.draft.as_ref().and_then(|d| d.delivery.as_ref()).ok_or("nothing approved")?;
            if !del.state.permits_try_again() {
                return Err("This report cannot be sent again now.".into());
            }
            del.approved.clone()
        };
        let this = self.clone();
        std::thread::spawn(move || this.deliver(approved, dest, true));
        Ok(())
    }

    /// **CHECK STATUS**: ask about the same report. Never sends again.
    pub fn check_status(&self) -> Result<(), String> {
        let dest = self.env.dest.clone()?;
        let (approved, current) = {
            let inner = self.inner.lock().unwrap();
            let del = inner.draft.as_ref().and_then(|d| d.delivery.as_ref()).ok_or("nothing approved")?;
            (del.approved.clone(), del.state.clone())
        };
        self.send(json!({"t": "checking-status", "report_id": approved.report_id}));
        let this = self.clone();
        std::thread::spawn(move || {
            let relay = match crate::relay::Https::new() {
                Ok(r) => r,
                Err(_) => return,
            };
            let now = util::now_millis();
            let state = delivery::after_check(delivery::check(&approved, &dest, &relay, now), &current, now);
            this.set_state(&state);
        });
        Ok(())
    }

    // -----------------------------------------------------------------------------------
    // screenshots
    // -----------------------------------------------------------------------------------

    /// **USE THIS IMAGE**: the image the window made with the person's covers burned in, made
    /// final here ([`richdoctor_core::image_profile::finalize`]: new pixels, no metadata, the
    /// shared format profile), checked against the limits (10 images, 5 MiB each, 25 MiB
    /// together), kept with the draft (owner-only) and handed to Claude with the next message
    /// when Claude Code is helping. Returns the final bytes, which are what the window shows.
    /// **BEFORE ANY DECODE** (PRD §6, round 40 §3): what the file is, from its header only. A
    /// file that is not PNG or JPEG, cannot be read, or is over 40 megapixels is refused here,
    /// before the window allocates its pixels.
    pub fn inspect_image(&self, image_base64: &str) -> Value {
        use richdoctor_core::image_profile as ip;
        let Some(bytes) = util::base64_decode(image_base64) else { return json!({"ok": false, "problem": "unreadable"}) };
        let kind = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "png" } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) { "jpeg" } else { return json!({"ok": false, "problem": "type"}) };
        let Some((w, h)) = ip::declared(&bytes) else { return json!({"ok": false, "problem": "unreadable"}) };
        if (w as u64) * (h as u64) > ip::MAX_PIXELS {
            return json!({"ok": false, "problem": "pixels", "width": w, "height": h});
        }
        json!({"ok": true, "kind": kind, "width": w, "height": h, "bytes": bytes.len()})
    }

    /// **CHECK THE EXACT IMAGE** (round 40 §2): the covered working copy made final (new pixels,
    /// no metadata, the shared profile) and returned, not yet kept. Over 5 MiB it answers
    /// `too-large:<bytes>` so the window reduces it and asks again.
    pub fn prepare_image(&self, image_base64: &str) -> Result<Value, String> {
        use richdoctor_core::image_profile as ip;
        let bytes = util::base64_decode(image_base64).ok_or("RichDoctor could not read that image.")?;
        let (clean, shape) = ip::finalize(&bytes).map_err(|e| format!("That image cannot be used: {e}."))?;
        if clean.len() as u64 > ip::MAX_IMAGE_BYTES {
            return Err(format!("too-large:{}", clean.len()));
        }
        let b64 = util::base64_std(&clean);
        Ok(json!({"url": format!("data:{};base64,{b64}", shape.kind.media_type()), "b64": b64, "width": shape.width, "height": shape.height, "bytes": clean.len()}))
    }

    /// What Email support would carry: the screenshots' total size and how many numbered parts
    /// of at most 10 MiB they need (computed from the real sizes, PRD §7).
    pub fn email_plan(&self) -> Value {
        use richdoctor_core::email;
        let files = self.image_files();
        let sizes: Vec<u64> = files.iter().map(|(_, b)| b.len() as u64).collect();
        let text = self.exact_text().map(|(_, t)| t.len() as u64).unwrap_or(0);
        let parts = if email::needs_parts(&sizes) { email::packages(text, &sizes).map(|p| p.len()).unwrap_or(0) } else { 1 };
        json!({"count": files.len(), "bytes": sizes.iter().sum::<u64>(), "parts": parts, "names": files.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()})
    }

    /// **USE THIS IMAGE** again for screenshot `index` (Edit covers, or Reduce after the
    /// five-minute cap): a fresh approval replacing that one, same number.
    pub fn replace_image(&self, index: usize, image_base64: &str, covers: u32, reduced: bool) -> Result<Value, String> {
        {
            let inner = self.inner.lock().unwrap();
            let d = inner.draft.as_ref().ok_or("no report")?;
            if index >= d.screenshots.len() {
                return Err("no such screenshot".into());
            }
        }
        let v = self.use_image(image_base64, covers, reduced)?;
        let mut inner = self.inner.lock().unwrap();
        let d = inner.draft.as_mut().ok_or("no report")?;
        let new = d.screenshots.pop().ok_or("no report")?;
        let old = std::mem::replace(&mut d.screenshots[index], new);
        std::fs::remove_file(self.env.store.draft_dir(&d.id).join("shots").join(&old.id)).ok();
        if let Some(r) = d.report.as_mut() {
            r.screenshots = d.screenshots.iter().map(|s| s.id.clone()).collect();
            r.fit();
        }
        self.save(&mut inner);
        let mut v = v;
        v["n"] = json!(index + 1);
        Ok(v)
    }

    pub fn use_image(&self, image_base64: &str, covers: u32, reduced: bool) -> Result<Value, String> {
        use richdoctor_core::image_profile as ip;
        let bytes = util::base64_decode(image_base64).ok_or("RichDoctor could not read that image.")?;
        if bytes.len() as u64 > 4 * ip::MAX_IMAGE_BYTES {
            return Err("That image is too large to use.".into());
        }
        let (clean, shape) = match ip::check(&bytes) {
            Ok(shape) => (bytes, shape),
            Err(_) => ip::finalize(&bytes).map_err(|e| format!("That image cannot be used: {e}."))?,
        };
        if clean.len() as u64 > ip::MAX_IMAGE_BYTES {
            return Err(format!("too-large:{}", clean.len()));
        }
        let sha = util::sha256_hex(&clean);
        let id = util::new_id("shot").map_err(|e| e.to_string())?;
        {
            let mut inner = self.inner.lock().unwrap();
            let d = inner.draft.as_mut().ok_or("no report")?;
            if d.screenshots.len() >= ip::MAX_IMAGES {
                return Err(format!("A report can carry {} screenshots at most.", ip::MAX_IMAGES));
            }
            let total: u64 = d.screenshots.iter().filter_map(|s| self.env.store.shot_bytes(&d.id, s)).map(|b| b.len() as u64).sum();
            if total + clean.len() as u64 > ip::MAX_TOTAL_BYTES {
                return Err("The screenshots together would be more than 25 MiB. Remove one first.".into());
            }
            let dir = self.env.store.draft_dir(&d.id).join("shots");
            util::write_private(&dir.join(&id), &clean).map_err(|e| e.to_string())?;
            d.screenshots.push(Shot { id, sha256: sha, covers, approved_at: util::now_millis(), media_type: shape.kind.media_type().into(), reduced });
            if let Some(r) = d.report.as_mut() {
                r.screenshots = d.screenshots.iter().map(|s| s.id.clone()).collect();
                r.fit();
            }
            if !d.without_ai {
                inner.pictures.push((shape.kind.media_type().to_string(), util::base64_std(&clean)));
            }
            self.save(&mut inner);
        }
        let n = self.inner.lock().unwrap().draft.as_ref().map(|d| d.screenshots.len()).unwrap_or(0);
        let entry = richdoctor_core::image_profile::Entry::of(n as u32, &clean)?;
        Ok(json!({
            "url": format!("data:{};base64,{}", shape.kind.media_type(), util::base64_std(&clean)),
            "n": n, "name": entry.filename, "width": entry.width, "height": entry.height, "bytes": entry.bytes, "covers": covers, "reduced": reduced,
        }))
    }

    /// **REMOVE** an approved screenshot (a changed set needs a fresh review).
    pub fn remove_image(&self, index: usize) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        let d = inner.draft.as_mut().ok_or("no report")?;
        if index >= d.screenshots.len() {
            return Err("no such screenshot".into());
        }
        let shot = d.screenshots.remove(index);
        std::fs::remove_file(self.env.store.draft_dir(&d.id).join("shots").join(&shot.id)).ok();
        if let Some(r) = d.report.as_mut() {
            r.screenshots = d.screenshots.iter().map(|s| s.id.clone()).collect();
            r.fit();
        }
        self.save(&mut inner);
        Ok(())
    }

    /// The approved screenshots' final bytes, as the window shows them.
    pub fn images(&self) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        let Some(d) = inner.draft.as_ref() else { return Vec::new() };
        let facts = self.shot_facts(d);
        d.screenshots
            .iter()
            .zip(facts)
            .filter_map(|(s, mut f)| {
                let b = self.env.store.shot_bytes(&d.id, s)?;
                f["url"] = json!(format!("data:{};base64,{}", s.media_type, util::base64_std(&b)));
                Some(f)
            })
            .collect()
    }

    /// The conversation's line after a set of screenshots was approved (round 40 §2).
    pub fn note_added(&self, count: u32, covers: u32) {
        if count == 0 {
            return;
        }
        let shots = if count == 1 { "1 screenshot added".to_string() } else { format!("{count} screenshots added") };
        let words = if covers > 0 { format!("{shots} ({covers} {} covered)", if covers == 1 { "area" } else { "areas" }) } else { shots };
        self.line(Entry::Note { text: words });
    }

    // -----------------------------------------------------------------------------------
    // saved reports
    // -----------------------------------------------------------------------------------

    /// Round 39's Saved reports list.
    pub fn saved(&self) -> Value {
        let rows: Vec<Value> = self.env.store.list().iter().map(saved_row).collect();
        let sent: Vec<Value> = self
            .env
            .store
            .sent()
            .iter()
            .map(|s| json!({"kind": "sent-link", "title": s.title, "date": util::local_date(s.sent_at), "state": "submitted", "state_words": "Report submitted", "issue_url": s.issue_url, "report_id": s.report_id}))
            .collect();
        json!({"rows": rows, "sent": sent})
    }

    /// Open a saved report: its conversation, its review or its delivery state. An approved
    /// in-flight report is checked once (never sent again).
    pub fn open(&self, id: &str) -> Result<Value, String> {
        let d = self.env.store.load(id).ok_or("That report is no longer on this Mac.")?;
        {
            let mut inner = self.inner.lock().unwrap();
            self.stop_session_locked(&mut inner);
            inner.pending = Pending::None;
            inner.last_message = None;
            inner.operation = 0;
            inner.draft = Some(d.clone());
        }
        let in_flight = d.delivery.as_ref().is_some_and(|x| x.state.unresolved());
        if in_flight {
            self.check_status().ok();
        }
        Ok(json!({
            "stage": d.stage,
            "conversation": d.conversation.iter().map(entry_event).collect::<Vec<_>>(),
            "delivery": d.delivery.as_ref().map(|_| {
                let mut v = delivery_view(&d);
                v["shots"] = Value::Array(self.shot_facts(&d));
                v
            }),
            "has_report": d.report.is_some(),
            "pending_question": d.questions.iter().rev().find(|q| q.1.is_none()).map(|q| q.0.clone()),
            "without_ai": d.without_ai,
        }))
    }

    /// Continue a reopened draft: an open question waits for its answer; otherwise the person
    /// chooses Resume investigation or Prepare report now (nothing re-runs on its own).
    pub fn resume_pending(&self) {
        let inner = self.inner.lock().unwrap();
        let Some(d) = inner.draft.as_ref() else { return };
        let q = d.questions.iter().rev().find(|q| q.1.is_none()).map(|q| q.0.clone());
        drop(inner);
        let pending = match q.as_deref() {
            Some(when::QUESTION) => Pending::When,
            Some(when::ROUGH_QUESTION) => Pending::Rough,
            Some(when::WHICH_QUESTION) => Pending::Which,
            Some(other) => Pending::Claude(other.to_string()),
            None => Pending::None,
        };
        self.inner.lock().unwrap().pending = pending;
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.draft.as_ref().is_some_and(|d| d.id == id) {
                self.stop_session_locked(&mut inner);
                inner.draft = None;
            }
        }
        self.env.store.delete(id).map_err(|e| e.to_string())
    }

    /// At launch: check every approved report whose delivery is unresolved, once (PRD §7: "On
    /// reopen the app checks the status of previously approved in-flight reports"). Never sends.
    pub fn check_in_flight(&self) {
        let Ok(dest) = self.env.dest.clone() else { return };
        let store = self.env.store.clone();
        std::thread::spawn(move || {
            let Ok(relay) = crate::relay::Https::new() else { return };
            for mut d in store.list() {
                let Some(del) = d.delivery.as_mut() else { continue };
                if !del.state.unresolved() {
                    continue;
                }
                let now = util::now_millis();
                let reading = delivery::check(&del.approved, &dest, &relay, now);
                // A report that was mid-send when RichDoctor quit is checked, and an unclear
                // answer leaves it unconfirmed (it may exist), never sendable.
                let next = delivery::after_check(reading, &del.state, now);
                del.state = next.clone();
                d.left = left_words(&next);
                store.save(&mut d, now).ok();
            }
        });
    }

    /// Expire old submitted copies (PRD §6: 30 days, the link stays).
    pub fn expire(&self) {
        self.env.store.expire(util::now_millis());
    }

    /// The current draft's delivery view.
    pub fn delivery(&self) -> Value {
        let inner = self.inner.lock().unwrap();
        self.view(&inner).unwrap_or(Value::Null)
    }
}

/// "Checking what happened when RichOS last opened…" -> "Checked what happened when RichOS last
/// opened".
fn folded(progress: &str) -> String {
    let s = progress.trim_end_matches('…');
    if let Some(rest) = s.strip_prefix("Checking ") {
        format!("Checked {rest}")
    } else if let Some(rest) = s.strip_prefix("Reading ") {
        format!("Read {rest}")
    } else {
        s.to_string()
    }
}

fn entry_event(e: &Entry) -> Value {
    match e {
        Entry::Person { text } => json!({"t": "me", "text": text}),
        Entry::Doctor { text } => json!({"t": "dr", "text": text}),
        Entry::Question { text, choices } => json!({"t": "question", "text": text, "choices": choices}),
        Entry::Done { text } => json!({"t": "done", "text": text}),
        Entry::Note { text } => json!({"t": "note", "text": text}),
        Entry::AiFailed { failure } => json!({"t": "ai-failed", "message": failure.message(), "retry": failure.offers_retry()}),
    }
}

/// How a saved report was left, in Saved reports' words.
fn left_words(state: &State) -> String {
    match state {
        State::Sending => "approved, sending".into(),
        State::SendingScreenshots { .. } => "approved, sending screenshots".into(),
        State::Creating => "approved, filing".into(),
        State::CreatedCheckingImages { .. } => "created, checking screenshots".into(),
        State::Checking { .. } => "approved, checking delivery".into(),
        State::Unconfirmed { checked_at } => format!("approved, checked at {}", util::local_clock(*checked_at)),
        State::Submitted { at, .. } => format!("sent at {}", util::local_clock(*at)),
        State::NotSent { .. } => "approved, not sent yet".into(),
    }
}

pub fn review_view(d: &Draft, r: &Report) -> Value {
    let mode = match &d.delivery {
        Some(del) if del.state.unresolved() => "revision-uncertain",
        Some(Delivery { state: State::NotSent { why: NotSent::UploadStopped { .. } | NotSent::Canceled, .. }, .. }) => "revision-upload",
        Some(del) if del.state.permits_revision() => "revision-notsent",
        Some(_) => "approved",
        None => "draft",
    };
    let mut notes = Vec::new();
    match (r.provenance, d.ai_failure) {
        (Provenance::AiFailed, Some(f)) => notes.push(format!("Claude Code stopped part way ({}). This report was prepared from your answers and what was collected; unanswered questions are listed under Missing information.", f.message().trim_end_matches('.').to_lowercase().replacen("richdoctor", "RichDoctor", 1).replacen("claude code", "Claude Code", 1))),
        (Provenance::AiFailed, None) | (Provenance::WithoutAi, _) => notes.push("Prepared without AI. Questions Claude would have asked are listed under Missing information; add anything you know.".into()),
        _ => {}
    }
    if let Some(n) = r.removal_note() {
        notes.push(n);
    }
    notes.extend(r.shortened.iter().cloned());
    let (title, body) = r.render();
    let problems: Vec<Value> = scrub::check_outgoing(&title, &body, None)
        .iter()
        .map(|p| {
            let line = p.item.strip_prefix("line ").and_then(|n| n.parse::<usize>().ok()).and_then(|n| body.split('\n').nth(n - 1)).map(|l| l.replace('\u{200B}', "").replace('\\', ""));
            json!({"item": p.item, "detail": p.detail, "line": line})
        })
        .collect();
    let old_id = d.delivery.as_ref().map(|x| x.approved.report_id.clone());
    json!({
        "mode": mode,
        "report_id": r.report_id,
        "old_id": old_id,
        "title": r.title,
        "sections": [
            {"key": "what_happened", "heading": "What happened", "text": if r.expected.trim().is_empty() { r.what_happened.clone() } else { format!("{}\n\nExpected: {}", r.what_happened, r.expected) }},
            {"key": "when", "heading": "When", "text": r.when},
            {"key": "versions", "heading": "Versions", "text": r.versions},
        ],
        "excerpts": r.excerpts,
        "screenshots": r.screenshots.len(),
        "after": [
            {"key": "causes", "heading": "Possible cause (not confirmed)", "text": r.causes},
            {"key": "missing", "heading": "Missing information", "text": r.missing.iter().map(|m| format!("• {m}")).collect::<Vec<_>>().join("\n")},
        ],
        "footer": report::footer(&r.report_id),
        "notes": notes,
        "exact": format!("{title}\n\n{body}"),
        "problems": problems,
    })
}

pub fn delivery_view(d: &Draft) -> Value {
    let Some(del) = &d.delivery else { return Value::Null };
    let first = del.approved.body.split("\n## ").next().unwrap_or("").trim_start_matches("## What happened").trim().replace('\\', "");
    let sections = del.approved.body.matches("\n## ").count() + usize::from(del.approved.body.starts_with("## "));
    let excerpts = d.report.as_ref().map(|r| r.excerpts.iter().filter(|e| !e.removed).count()).unwrap_or(0);
    let (kind, extra) = match &del.state {
        State::Sending | State::Creating => ("sending", json!({})),
        State::SendingScreenshots { done, total } => ("uploading", json!({"done": done, "total": total})),
        State::CreatedCheckingImages { issue_url, confirmed, expected, .. } => ("created", json!({"issue_url": issue_url, "confirmed": confirmed, "expected": expected})),
        State::Checking { .. } => ("checking", json!({})),
        State::Unconfirmed { checked_at } => ("unconfirmed", json!({"checked_at": util::local_clock(*checked_at)})),
        State::Submitted { issue_url, .. } => ("submitted", json!({"issue_url": issue_url})),
        State::NotSent { why, .. } => {
            // Round 40 §5: an interrupted upload names the image that stopped; past the
            // five-minute cap it also offers Reduce.
            let (upload, item, toolong) = match why {
                NotSent::UploadStopped { item, why } => (true, Some(item.clone()), why.contains("five minutes")),
                NotSent::Canceled => (true, None, false),
                _ => (false, None, false),
            };
            ("notsent", json!({"reason": why.reason(), "review": why.needs_review(), "try_again": del.state.permits_try_again(), "upload": upload, "item": item, "toolong": toolong, "canceled": matches!(why, NotSent::Canceled)}))
        }
    };
    let rows: Vec<Value> = del
        .approved
        .manifest
        .iter()
        .map(|e| {
            let ok = del.stored.contains(&e.attachment_id);
            json!({"id": e.attachment_id, "sent": if ok { e.bytes } else { 0 }, "total": e.bytes, "st": if ok { "ok" } else { "wait" }})
        })
        .collect();
    json!({
        "kind": kind,
        "report_id": del.approved.report_id,
        "title": del.approved.title,
        "first": first.split("\n\nExpected").next().unwrap_or("").chars().take(600).collect::<String>(),
        "summary": format!("{sections} sections, {excerpts} diagnostic {}, the report ID footer. The exact text is in Saved reports.", if excerpts == 1 { "excerpt" } else { "excerpts" }),
        "extra": extra,
        "rows": rows,
        "received": del.stored.len(),
        "attempt": del.approved.manifest.len(),
    })
}

fn saved_row(d: &Draft) -> Value {
    let date = util::local_date(d.updated_at);
    let (state, words, action) = match d.delivery.as_ref().map(|x| &x.state) {
        Some(State::Submitted { .. }) => ("submitted", "Report submitted".to_string(), "view"),
        Some(State::Unconfirmed { .. }) | Some(State::Checking { .. }) | Some(State::Sending) | Some(State::Creating) | Some(State::SendingScreenshots { .. }) => ("unconfirmed", "Delivery could not be confirmed".to_string(), "check"),
        Some(State::CreatedCheckingImages { confirmed, expected, .. }) => ("created", format!("Report created; checking screenshots · {confirmed} of {expected} confirmed"), "check"),
        Some(State::NotSent { why: delivery::NotSent::UploadStopped { item, .. }, .. }) => ("notsent", format!("Not sent yet · {item} of {} did not finish", d.screenshots.len()), "resume-upload"),
        Some(State::NotSent { why: delivery::NotSent::Canceled, .. }) => ("notsent", "Not sent yet · sending was canceled".to_string(), "resume-upload"),
        Some(State::NotSent { why, .. }) => ("notsent", format!("Not sent yet · {}", match why {
            delivery::NotSent::Offline => "offline",
            delivery::NotSent::Busy { .. } => "the service asked to wait",
            delivery::NotSent::Unreachable => "connection failed",
            delivery::NotSent::Rejected { .. } => "needs a change",
            delivery::NotSent::Refused { .. } => "could not be filed",
            delivery::NotSent::Closed { .. } => "the service is not taking reports",
            delivery::NotSent::NotReceived => "not received",
            delivery::NotSent::UploadStopped { .. } => "a screenshot did not finish uploading",
            delivery::NotSent::Retired => "replaced by a revision",
            delivery::NotSent::Canceled => "sending was canceled",
        }), if why.needs_review() { "review" } else { "try" }),
        None => ("saved", "Saved on this Mac".to_string(), "resume"),
    };
    let issue = match d.delivery.as_ref().map(|x| &x.state) {
        Some(State::Submitted { issue_url, .. }) | Some(State::CreatedCheckingImages { issue_url, .. }) => Some(issue_url.clone()),
        _ => None,
    };
    // Round 40 §7: the date line carries the screenshot facts.
    let k = d.screenshots.len();
    let shots = |n: usize| if n == 1 { "1 screenshot".to_string() } else { format!("{n} screenshots") };
    let shots_words = match d.delivery.as_ref() {
        Some(del) if matches!(del.state, State::Submitted { .. }) => if k == 0 { "no screenshots".to_string() } else { format!("{} received", shots(k)) },
        Some(del) if k > 0 && matches!(del.state, State::NotSent { .. }) => format!("{} of {} received", del.stored.len(), shots(k)),
        Some(_) if k == 0 => "no screenshots".to_string(),
        _ if k > 0 => shots(k),
        _ => String::new(),
    };
    json!({"id": d.id, "title": d.title(), "date": date, "left": d.left, "state": state, "state_words": words, "action": action, "issue_url": issue, "shots_words": shots_words})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(seed: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 2, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&[seed, 0, 0, 255, 0, seed, 0, 255]).unwrap();
        }
        out
    }

    /// An engine over a scratch store, with two approved screenshots and a report, and the
    /// events it emits.
    fn engine_with_shots() -> (Engine, Arc<Mutex<Vec<Value>>>, PathBuf) {
        let root = std::env::temp_dir().join(format!("rd-engine-{}", util::hex(&util::random_bytes(4).unwrap())));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let env = Env { store: Store::at(&root), home: root.clone(), exe: PathBuf::new(), claude: None, dest: Err("no relay in this test".into()) };
        let engine = Engine::new(env, Arc::new(move |v| s2.lock().unwrap().push(v)));
        {
            let mut inner = engine.inner.lock().unwrap();
            let mut d = Draft::new(1).unwrap();
            d.description = "RichOS closes before its window opens.".into();
            inner.draft = Some(d);
        }
        for seed in [1u8, 2] {
            engine.use_image(&util::base64_std(&png(seed)), u32::from(seed == 1), false).unwrap();
        }
        (engine, seen, root)
    }

    #[test]
    fn the_per_image_list_follows_the_relay_and_keeps_what_was_stored() {
        let (engine, seen, root) = engine_with_shots();
        let manifest = {
            let inner = engine.inner.lock().unwrap();
            engine.env.store.manifest(inner.draft.as_ref().unwrap()).unwrap()
        };
        {
            let mut inner = engine.inner.lock().unwrap();
            inner.upload = manifest.iter().map(|e| UploadRow { id: e.attachment_id.clone(), sent: 0, total: e.bytes, st: "wait" }).collect();
            let approved = delivery::approve("RD-7K3M-Q9VX", "t", &richdoctor_core::report::ending("https://report.richos.ceo", "RD-7K3M-Q9VX", 2), manifest.clone(), "https://report.richos.ceo", 1, &util::uuid_v4().unwrap()).unwrap();
            inner.draft.as_mut().unwrap().delivery = Some(Delivery { approved, state: State::SendingScreenshots { done: 1, total: 2 }, completion_attempted: false, stored: Vec::new() });
        }
        engine.heard(Upload::Plan { missing: vec!["2".into()] });
        engine.heard(Upload::Bytes { id: "2".into(), sent: manifest[1].bytes / 2, total: manifest[1].bytes });
        let rows = |engine: &Engine| engine.inner.lock().unwrap().upload.iter().map(|r| r.st).collect::<Vec<_>>();
        assert_eq!(rows(&engine), ["earlier", "live"]);
        let v = engine.delivery();
        assert_eq!(v["kind"], "uploading");
        assert_eq!(v["rows"][1]["sent"], manifest[1].bytes / 2, "a real byte count, never a guess");
        assert_eq!(v["shots"].as_array().unwrap().len(), 2);
        engine.heard(Upload::Received { id: "2".into() });
        assert_eq!(rows(&engine), ["earlier", "ok"]);
        let stored = engine.inner.lock().unwrap().draft.as_ref().unwrap().delivery.as_ref().unwrap().stored.clone();
        assert_eq!(stored, ["1", "2"], "what the relay stored is kept with the draft");
        assert!(seen.lock().unwrap().iter().any(|e| e["t"] == "upload"));
        // A stopped upload marks its row; the not-sent view names it and offers Resume.
        engine.inner.lock().unwrap().upload[1].st = "live";
        engine.set_state(&State::NotSent { why: NotSent::UploadStopped { item: "screenshot 2".into(), why: "it did not finish within five minutes.".into() }, at: 5 });
        assert_eq!(rows(&engine), ["earlier", "stopped"]);
        let v = engine.delivery();
        assert_eq!(v["kind"], "notsent");
        assert_eq!(v["extra"]["upload"], true);
        assert_eq!(v["extra"]["toolong"], true);
        // The review after it is round 40's "after an interrupted send", unchanged until edited.
        let r = engine.review();
        assert!(r.is_null() || r["mode"] == "revision-upload");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cancel_is_offered_only_while_sending() {
        let (engine, _, root) = engine_with_shots();
        assert!(!engine.cancel_upload(), "nothing to cancel");
        let flag = Arc::new(AtomicBool::new(false));
        {
            let mut inner = engine.inner.lock().unwrap();
            inner.cancel_upload = Some(flag.clone());
            let approved = delivery::approve("RD-7K3M-Q9VX", "t", &richdoctor_core::report::footer("RD-7K3M-Q9VX"), Vec::new(), "https://report.richos.ceo", 1, &util::uuid_v4().unwrap()).unwrap();
            inner.draft.as_mut().unwrap().delivery = Some(Delivery { approved, state: State::SendingScreenshots { done: 0, total: 2 }, completion_attempted: false, stored: Vec::new() });
        }
        assert!(engine.cancel_upload() && flag.load(Ordering::SeqCst));
        engine.inner.lock().unwrap().draft.as_mut().unwrap().delivery.as_mut().unwrap().state = State::Creating;
        flag.store(false, Ordering::SeqCst);
        assert!(!engine.cancel_upload() && !flag.load(Ordering::SeqCst), "once the report is being filed, Cancel is not offered");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn an_image_is_refused_from_its_header_before_any_decode() {
        let (engine, _, root) = engine_with_shots();
        let ok = engine.inspect_image(&util::base64_std(&png(3)));
        assert_eq!(ok["ok"], true);
        assert_eq!(ok["width"], 2);
        assert_eq!(engine.inspect_image(&util::base64_std(b"\0\0\0\x18ftypheic"))["problem"], "type");
        // A PNG header declaring 8000 x 6000 (48 megapixels) is refused on its header alone.
        let mut big = png(4);
        big[16..20].copy_from_slice(&8000u32.to_be_bytes());
        big[20..24].copy_from_slice(&6000u32.to_be_bytes());
        assert_eq!(engine.inspect_image(&util::base64_std(&big))["problem"], "pixels");
        // What use_image keeps is exactly what prepare_image showed.
        let shown = engine.prepare_image(&util::base64_std(&png(5))).unwrap();
        let kept = engine.use_image(shown["b64"].as_str().unwrap(), 0, false).unwrap();
        assert_eq!(shown["url"], kept["url"]);
        assert_eq!(kept["n"], 3);
        assert_eq!(kept["name"], "screenshot-3.png");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn progress_folds_into_one_soft_line() {
        assert_eq!(folded("Checking what happened when RichOS last opened…"), "Checked what happened when RichOS last opened");
        assert_eq!(folded("Reading the crash record from 8:43 AM…"), "Read the crash record from 8:43 AM");
    }
}
