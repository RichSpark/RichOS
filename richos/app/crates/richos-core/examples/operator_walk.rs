//! THE OPERATOR WALK'S BACK END — the app's own wiring of his team, without the window
//! (operator back-end spec r3 §6 verification 5, W2, as r4 §5 changes it; the operator-client
//! record's §7 items 1-3).
//!
//! The switch stays OFF in every build. So W2 is walked here, in the test VM, against the real
//! engine and a real `claude`, through the SAME composition the shell builds at launch on an
//! operator install (`src-tauri/src/main.rs` `operator_desk_at_boot`): a spine that keeps the
//! mouth each sentence came through, writing the conversation ledger in the data folder; the
//! work host with his team's desk as its intake, so every message to a lead goes through the
//! register, the adoption and the desk exactly as the app sends it; the desk's socket and the
//! front desk's `richos_operator` tools as a real child process speaking to it; and the seated
//! settle through a real ECS bridge when the walk is given a Python. What it cannot reach is the
//! window itself (the notice surface, the quit sheet's rendering), which the walk says.
//!
//!   operator_walk --operator-mcp <scope>        his lead's report tool, as the app serves it
//!   operator_walk --operator-desk-mcp <scope>   the front desk's operator tools, as the app serves them
//!   operator_walk gate <data dir>               one gate reading, as JSON
//!   operator_walk host <data dir> <engine-state> <operator dir>
//!       reads one JSON command per line on stdin, answers one JSON line each on stdout, and
//!       writes every notice as {"event":"say",…} on stdout as it happens
//!
//! Commands: assign, answer, questions, answer-question, stop, interrupt, read, stop-assignment, settled, team, retire,
//! take, lease-spawns, quit. Nothing here is product code; it is the walk's stand-in for the
//! shell, built from the shell's own parts.
use richos_core::assignment::{self, AssignmentKind, AssignmentState, Registration};
use richos_core::assignment_tools::{EcsObligations, ObligationDesk, ObligationOpener};
use richos_core::cognition::MockLeaseFactory;
use richos_core::ecs::EcsBridge;
use richos_core::entity::{Entity, EntityId, EntityRegistry};
use richos_core::ledger::{Ledger, Source};
use richos_core::operator_declaration::{gate, EngineFenceStatus, Gate, Refusal};
use richos_core::operator_desk::{DeskParts, LedgerOrigins, OperatorDesk};
use richos_core::operator_desk_tools::{DeskSocket, DeskToolScope};
use richos_core::operator_host::{ConversationKey, Settle};
use richos_core::operator_lead::NoPermissionDesk;
use richos_core::operator_profile::SessionEnvironment;
use richos_core::operator_runtime::{EcsSettle, EngineScripts, OperatorNotice, ProfileLauncher, SessionValues};
use richos_core::spine::Spine;
use richos_core::steering::TurnControl;
use richos_core::work_host::{SilentNotifier, WorkHost};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The entity every walk conversation belongs to.
const ENTITY: &str = "femcboost";

/// Settlement with no ECS bridge: recorded, and refused with the sentence, so the host keeps
/// the assignment open and says so.
struct RecordedSettle(Mutex<std::fs::File>);

impl Settle for RecordedSettle {
    fn complete(&self, key: &ConversationKey, obligation_id: &str, source_ref: &str, status: &str,
                evidence: &[String], _answer_text: &str) -> Result<(), String> {
        let line = json!({"thread": key.thread_id, "obligation_id": obligation_id, "source_ref": source_ref,
                          "status": status, "evidence": evidence});
        writeln!(self.0.lock().unwrap(), "{line}").map_err(|e| format!("the walk's settle record could not be written ({e})"))?;
        Err("not wired in this walk: no Python was given for the engine's ECS store".into())
    }
}

/// The work host's lease factory in the walk: it counts every work lease asked for and opens
/// none. On his team's path the count stays zero, and the walk says so (`lease-spawns`).
struct CountingLeases(Arc<std::sync::atomic::AtomicUsize>);

impl richos_core::cognition::LeaseFactory for CountingLeases {
    fn spawn(&self) -> Result<Box<dyn richos_core::cognition::Cognition>, richos_core::cognition::CognitionError> {
        Err(richos_core::cognition::CognitionError::Protocol("the walk opens no lease".into()))
    }
    fn spawn_work(&self, _binding: &richos_core::entity::ThreadBinding)
                  -> Result<Box<dyn richos_core::cognition::Cognition>, richos_core::cognition::CognitionError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(richos_core::cognition::CognitionError::Protocol("the walk opens no work lease".into()))
    }
}

/// The per-start values from a named `launchctl` (the probe record's harness finding 8).
struct WalkSession(PathBuf);

impl SessionValues for WalkSession {
    fn derive(&self) -> Result<SessionEnvironment, Refusal> {
        SessionEnvironment::derive_with(&self.0)
    }
}

struct Out(Mutex<std::io::Stdout>);

impl Out {
    fn line(&self, value: &Value) {
        let mut out = self.0.lock().unwrap();
        if let Err(e) = writeln!(out, "{value}").and_then(|_| out.flush()) {
            eprintln!("operator_walk: stdout: {e}: {value}");
        }
    }
}

/// The walk's conversation names (`w2-a`) and the spine's thread ids, both ways, kept on disk
/// so a relaunched walk (step 11) speaks to the same conversations.
#[derive(Default)]
struct Names {
    by_name: HashMap<String, String>,
    by_id: HashMap<String, String>,
}

struct App {
    data: PathBuf,
    root: PathBuf,
    state: PathBuf,
    desk: Arc<OperatorDesk>,
    work: Arc<WorkHost>,
    /// How many work leases the work host ever asked for (none, on his team's path).
    work_leases: Arc<std::sync::atomic::AtomicUsize>,
    spine: Mutex<Spine>,
    control: TurnControl,
    names: Arc<Mutex<Names>>,
    socket: DeskSocket,
    /// The declaration's listed mouths, written into each front-desk tool scope as the app does.
    origins: Vec<String>,
    ecs: Option<(EcsBridge, bool)>,
}

impl App {
    /// Resolved phone answers enter the same durable store/inbox as the shell. This walk
    /// starts after authentication and word recognition; it does not prove either one.
    fn answer_question(&self, thread: &str, value: &Value) -> Result<Value, String> {
        use richos_core::questions::{AnswerRequest, Store};
        let method = value["method"].as_str().ok_or("Supply an answer method")?;
        if !matches!(method, "phone_tap" | "phone_typed" | "phone_voice") {
            return Err("Unknown resolved phone answer method".into());
        }
        let request: AnswerRequest = serde_json::from_value(value["answer"].clone()).map_err(|e| e.to_string())?;
        let store = Store::new(&self.state);
        let question = store.list(ENTITY, thread)?.into_iter()
            .find(|q| q.id == request.question_id).ok_or("No question in this conversation")?;
        if !question.asker.starts_with("operator:") {
            return Err("This walk answers operator questions".into());
        }
        let answer = store.answer(ENTITY, thread, request, method, "phone")?;
        let enqueued = store.deliver(ENTITY, thread, &question.asker,
            |delivery| self.desk.queue_question_answer(delivery))?;
        // A fresh reader reconciles the same durable identity before the lead is invoked.
        let reconciled = Store::new(&self.state).deliver(ENTITY, thread, &question.asker,
            |delivery| self.desk.queue_question_answer(delivery))?;
        self.desk.flush_question_answers()?;
        self.desk.flush_question_answers()?;
        Ok(json!({"answer": answer.public_value(), "enqueued": enqueued, "reconciled": reconciled}))
    }

    fn names_file(data: &Path) -> PathBuf {
        data.join("walk-threads.json")
    }

    fn save_names(&self) {
        let names = self.names.lock().unwrap();
        if let Err(e) = std::fs::write(Self::names_file(&self.data), json!(names.by_name).to_string()) {
            eprintln!("operator_walk: the thread names could not be saved ({e})");
        }
    }

    /// The spine thread for a walk conversation, made the first time it is named.
    fn thread(&self, name: &str, title: &str) -> Result<String, String> {
        if let Some(id) = self.names.lock().unwrap().by_name.get(name).cloned() {
            return Ok(id);
        }
        let entity = EntityId::parse(ENTITY).map_err(|e| e.to_string())?;
        let id = self.spine.lock().unwrap().create_thread(if title.is_empty() { name } else { title }, &entity)
            .map_err(|e| e.to_string())?;
        {
            let mut names = self.names.lock().unwrap();
            names.by_name.insert(name.to_string(), id.clone());
            names.by_id.insert(id.clone(), name.to_string());
        }
        self.save_names();
        Ok(id)
    }

    fn key(&self, id: &str) -> ConversationKey {
        ConversationKey { entity_id: ENTITY.into(), thread_id: id.to_string() }
    }

    /// His words, into the ledger through the mouth named, the way the app's own paths put them
    /// there: the composer (`desk-typed`), the microphone (`desk-voice`), the phone's intake
    /// (`phone`), or a turn recorded before keeping began (`unrecorded`). Returns the turn id.
    fn say(&self, id: &str, via: &str, text: &str) -> Result<String, String> {
        let mut spine = self.spine.lock().unwrap();
        let turn = match via {
            "desk-typed" => spine.submit_prompt_to(id, text, Source::Text).map_err(|e| e.to_string())?,
            "desk-voice" => {
                spine.switch_thread(id).map_err(|e| e.to_string())?;
                spine.submit_prompt_spoken(text, Source::Jam, false).map_err(|e| e.to_string())?
            }
            "phone" => {
                let entity = EntityId::parse(ENTITY).map_err(|e| e.to_string())?;
                let record = self.control.submit_from_channel(id, Some(entity), text, "phone").map_err(|e| e.to_string())?;
                spine.poll_intake().map_err(|e| e.to_string())?;
                spine.ledger().turn_for_intake(record.id()).map(|t| t.id.clone()).ok_or("the phone's words did not become a turn")?
            }
            "unrecorded" => {
                spine.keep_intake_channel(false);
                let turn = spine.submit_prompt_to(id, text, Source::Text).map_err(|e| e.to_string());
                spine.keep_intake_channel(true);
                turn?
            }
            other => return Err(format!("the walk does not know the mouth {other:?}")),
        };
        Ok(turn)
    }

    /// His seat for this conversation in the ECS store, as the front desk derives it.
    fn seat(&self, id: &str) -> Option<String> {
        self.ecs.as_ref().filter(|(_, seats)| *seats).and_then(|_| richos_core::ecs::ceo_seat(id))
    }

    /// **One assignment, the app's way**: his words into the ledger through their mouth, the
    /// obligation opened on his seat (when the walk has the ECS store), the register written as
    /// the front desk's register writes it, and the work host's boundary adoption handing it to
    /// the desk. Answers with the record as the desk left it.
    fn assign(&self, v: &Value) -> Value {
        let name = v["thread"].as_str().unwrap_or("");
        // The conversation's title names the thread; the ASSIGNMENT's title is what the front
        // desk writes for this one request. r5a used the conversation's ("Walk A: land") for every
        // assignment, and the leads rightly read a request to echo a word as a land to verify
        // (steps 5, 7, 8 and 13's F). So the assignment gets its own, and a neutral one when the
        // driver names none.
        let title = v["title"].as_str().unwrap_or("Walk assignment");
        let task = v["task"].as_str().unwrap_or("Carry out the probe request in his words below");
        let text = v["text"].as_str().unwrap_or("");
        let via = v["origin"].as_str().unwrap_or("desk-typed");
        let run = || -> Result<Value, String> {
            let id = self.thread(name, title)?;
            let turn = self.say(&id, via, text)?;
            let obligation = format!("walk-{}", uuid::Uuid::new_v4());
            let mut opened = Value::Null;
            if let Some((bridge, _)) = &self.ecs {
                let seat = self.seat(&id);
                // One front-desk session per conversation, as the app has (each conversation's
                // resident front desk): the store holds a session to the first conversation it
                // bound ("session scope is immutable"), which r5a hit with one shared id.
                let session = format!("walk-front-desk-{id}");
                let desk = bridge.bind(ENTITY, &id, &session, &turn, seat.as_deref(), "ceo")
                    .map(|binding| ObligationDesk { bridge: bridge.clone(), binding, seat: seat.clone() })
                    .map_err(|e| e.to_string());
                opened = match desk.and_then(|d| EcsObligations.open(&d, &obligation, task, AssignmentKind::Task)) {
                    Ok(()) => json!("opened"),
                    Err(e) => json!(format!("not opened: {e}")),
                };
            }
            use sha2::Digest;
            let registration = Registration {
                entity_id: ENTITY.into(), thread_id: id.clone(), obligation_id: obligation,
                instruction_ledger_ref: format!("ledger:{id}:{turn}"),
                instruction_sha256: format!("{:x}", sha2::Sha256::digest(text.as_bytes())),
                title: task.to_string(), repositories: vec![], needs_screen: false,
            };
            let receipt = assignment::register_kind(&self.state, &registration, AssignmentKind::Task).map_err(|e| e.to_string())?;
            let binding = self.spine.lock().unwrap().ledger().thread_binding(&id).map_err(|e| e.to_string())?;
            let adopted = self.work.adopt_registered(&binding);
            let quiet = self.desk.wait_quiet(Duration::from_secs(420));
            let record = assignment::read(&self.state, ENTITY, &id, &receipt.id).map_err(|e| e.to_string())?;
            let relayed = record.state == AssignmentState::Running;
            Ok(json!({"handle": receipt.id, "turn": turn, "adopted": adopted, "quiet": quiet, "obligation": opened,
                      "state": record.state.as_str(), "relayed": relayed,
                      "sentence": if relayed { Value::Null } else { json!(record.detail) },
                      "error": if relayed { Value::Null } else { json!(record.detail) }}))
        };
        run().unwrap_or_else(|e| json!({"error": e}))
    }

    /// A front-desk tool call, through the real `richos_operator` stdio server (this binary as
    /// `--operator-desk-mcp`) and the desk's socket, with a scope for this turn.
    fn desk_tool(&self, id: &str, ledger_ref: Option<String>, tool: &str, args: Value) -> Value {
        let scope = self.root.join(format!("desk-scope-{}.json", uuid::Uuid::new_v4().simple()));
        let run = || -> Result<Value, String> {
            richos_core::operator_desk_tools::write_scope(&scope, &DeskToolScope {
                version: 1, socket: self.socket.path().to_path_buf(), token: self.socket.token().to_string(),
                entity_id: ENTITY.into(), thread_id: id.to_string(), ledger_ref, origins: self.origins.clone() })?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let mut child = Command::new(exe).arg("--operator-desk-mcp").arg(&scope)
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().map_err(|e| e.to_string())?;
            {
                let mut stdin = child.stdin.take().ok_or("no stdin")?;
                let frames = [json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
                              json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}})];
                for frame in frames {
                    writeln!(stdin, "{frame}").map_err(|e| e.to_string())?;
                }
            }
            let mut out = String::new();
            child.stdout.take().ok_or("no stdout")?.read_to_string(&mut out).map_err(|e| e.to_string())?;
            child.wait().map_err(|e| e.to_string())?;
            let reply: Value = out.lines().nth(1).and_then(|l| serde_json::from_str(l).ok()).ok_or("the tool did not answer")?;
            let text = reply["result"]["content"][0]["text"].as_str().unwrap_or("").to_string();
            let body = serde_json::from_str::<Value>(&text).unwrap_or(json!({"say": text}));
            Ok(json!({"is_error": reply["result"]["isError"], "reply": body}))
        };
        let answer = run().unwrap_or_else(|e| json!({"error": e}));
        if let Err(e) = std::fs::remove_file(&scope) {
            eprintln!("operator_walk: the tool scope {} could not be removed ({e})", scope.display());
        }
        answer
    }

    /// Step 1's settlement, read from the ECS store itself: the obligation's state on his seat.
    fn settled(&self, v: &Value) -> Value {
        let name = v["thread"].as_str().unwrap_or("");
        let Some(id) = self.names.lock().unwrap().by_name.get(name).cloned() else { return json!({"error": "no such conversation"}) };
        let record = match assignment::read(&self.state, ENTITY, &id, v["handle"].as_str().unwrap_or("")) {
            Ok(record) => record,
            Err(e) => return json!({"error": e.to_string()}),
        };
        let ecs = self.ecs.as_ref().map(|(bridge, _)| {
            let seat = self.seat(&id);
            bridge.request("current", richos_core::ecs::seated_request(seat.as_deref(), json!({})))
                .ok().and_then(|c| serde_json::from_value::<richos_core::ecs::Binding>(c["binding"].clone()).ok())
                .map(|binding| format!("{:?}", bridge.obligation_state(&binding, seat.as_deref(), &record.obligation_id)))
                .unwrap_or_else(|| "no binding on the seat".into())
        });
        json!({"state": record.state.as_str(), "detail": record.detail, "obligation": record.obligation_id, "ecs": ecs})
    }
}

fn gate_json(data: &Path) -> Value {
    match gate(data) {
        Gate::Product => json!({"gate": "product"}),
        Gate::Refused(r) => json!({"gate": "refused", "sentence": r.sentence()}),
        Gate::Operator(d) => json!({"gate": "operator", "entity": d.entity_root, "engine": d.engine_root}),
    }
}

fn host(data: &Path, _driver_state: &Path, root: &Path) -> Result<(), String> {
    // **The register and his team's files live where the app keeps them**: the desk works on
    // `<data>/engine-state` and `<data>/operator` (`OperatorDesk::new`, as the shell's
    // `operator_desk_at_boot` gives it the app's data folder), so the walk's register, launcher
    // and report scope use the same folder. The driver's own state argument is kept for the
    // command line's shape only; `root` holds the walk's settle record and tool scopes.
    let state_dir = data.join("engine-state");
    let state = state_dir.as_path();
    let declaration = match gate(data) {
        Gate::Operator(d) => *d,
        Gate::Refused(r) => return Err(r.sentence()),
        Gate::Product => return Err("no declaration: the product path".into()),
    };
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let out = Arc::new(Out(Mutex::new(std::io::stdout())));
    let names: Arc<Mutex<Names>> = Arc::new(Mutex::new(Names::default()));
    if let Ok(text) = std::fs::read_to_string(App::names_file(data)) {
        let saved: HashMap<String, String> = serde_json::from_str(&text).unwrap_or_default();
        let mut n = names.lock().unwrap();
        for (name, id) in saved {
            n.by_id.insert(id.clone(), name.clone());
            n.by_name.insert(name, id);
        }
    }

    // The spine: the ledger in the data folder (where LedgerOrigins reads it, as the app's
    // does), his one entity, a mock front desk, keeping the mouth.
    let ledger = Ledger::open(data.join("conversation-ledger.jsonl")).map_err(|e| e.to_string())?;
    let mut spine = Spine::new(ledger);
    spine.set_entity_registry(EntityRegistry::new(vec![
        Entity::new(ENTITY, "FemcBoost", &[&declaration.entity_root.display().to_string()]).map_err(|e| e.to_string())?,
    ]).map_err(|e| e.to_string())?);
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec![])));
    let control = TurnControl::open(data.join("intake.jsonl")).map_err(|e| e.to_string())?;
    spine.set_turn_control(control.clone());
    spine.keep_intake_channel(true);

    // The ECS store the register opens obligations in, when the walk is given a Python.
    let ecs = std::env::var_os("RICHOS_WALK_PYTHON").and_then(|python| {
        match EcsBridge::new(Path::new(&python), &declaration.engine_root, &data.join("ecs")) {
            Ok(bridge) => {
                let seats = bridge.supports_ceo_thread_seats();
                Some((bridge, seats))
            }
            Err(e) => {
                eprintln!("operator_walk: no ECS bridge ({e}); settlement is recorded, not made");
                None
            }
        }
    });
    let settle: Arc<dyn Settle> = match &ecs {
        Some((bridge, _)) => Arc::new(EcsSettle::new(bridge.clone())),
        None => Arc::new(RecordedSettle(Mutex::new(std::fs::OpenOptions::new().create(true).append(true)
            .open(root.join("settle-calls.jsonl")).map_err(|e| e.to_string())?))),
    };

    // His team's desk, composed as the shell composes it at launch.
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let log = data.join("operator").join("operator.log");
    // F6, as the shell does it: the desk's socket, and the leads' stop-only token in every
    // lead's report scope.
    let socket_path = richos_core::operator_desk_tools::socket_path(&data.join("operator"));
    let desk_token = richos_core::operator_desk_tools::new_token();
    let lead_token = richos_core::operator_desk_tools::new_token();
    let lead_desk = richos_core::operator_report::LeadDesk { socket: socket_path.clone(), token: lead_token.clone() };
    let launcher = Arc::new(match std::env::var_os("RICHOS_WALK_LAUNCHCTL") {
        Some(launchctl) => ProfileLauncher::with(declaration.clone(), &executable, state, &log, Arc::new(NoPermissionDesk),
                                                 Box::new(WalkSession(PathBuf::from(launchctl))), Box::new(EngineFenceStatus)),
        None => ProfileLauncher::new(declaration.clone(), &executable, state, &log, Arc::new(NoPermissionDesk)),
    }.with_lead_desk(Some(lead_desk)));
    let push_out = out.clone();
    let push_names = names.clone();
    let releaser = launcher.clone();
    let gate_dir = data.to_path_buf();
    let desk = OperatorDesk::new(declaration.clone(), data, DeskParts {
        launcher,
        release: Box::new(move || releaser.release()),
        engine: Arc::new(EngineScripts::new(declaration.clone())),
        settle,
        push: Some(Box::new(move |k: &ConversationKey, n: &OperatorNotice| {
            let thread = push_names.lock().unwrap().by_id.get(&k.thread_id).cloned().unwrap_or_else(|| k.thread_id.clone());
            push_out.line(&json!({"event": "say", "thread": thread, "handle": n.handle,
                                  "kind": serde_json::to_value(n.kind).unwrap_or(Value::Null), "text": n.text}));
        })),
        origins: Arc::new(LedgerOrigins::new(&data.join("conversation-ledger.jsonl"))),
        gate: Box::new(move || gate(&gate_dir)),
    });
    let work_leases = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let work = WorkHost::new(state, Box::new(CountingLeases(work_leases.clone())), Arc::new(SilentNotifier));
    work.set_operator(desk.clone());
    work.start();
    let socket = DeskSocket::serve_with_lead(desk.clone(), &socket_path, &desk_token, Some(&lead_token))
        .map_err(|e| e.to_string())?;
    let app = App { data: data.to_path_buf(), root: root.to_path_buf(), state: state.to_path_buf(), desk, work, work_leases,
                    spine: Mutex::new(spine), control, names, socket, ecs, origins: declaration.origins.clone() };
    let _ = root;

    out.line(&json!({"ready": true, "pid": std::process::id(), "ecs": app.ecs.as_ref().map(|(_, seats)| json!({"seats": seats}))}));
    // As the app does at launch (`src-tauri/src/question_host.rs`: the question worker's first
    // wake flushes saved answers): a walk relaunched after a crash delivers every answer his
    // team has not taken (the crash matrix, answer-delivery design §4.2). Off this thread, as
    // the app's worker is, because a lead may have to start first.
    let flusher = app.desk.clone();
    std::thread::spawn(move || {
        if let Err(e) = flusher.flush_question_answers() {
            eprintln!("operator_walk: a saved answer is still waiting ({e})");
        }
    });
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let id = v["id"].clone();
        let name = v["thread"].as_str().unwrap_or("").to_string();
        let thread_id = app.names.lock().unwrap().by_name.get(&name).cloned();
        let answer = match v["cmd"].as_str().unwrap_or("") {
            "assign" => app.assign(&v),
            "questions" => match &thread_id {
                Some(tid) => match richos_core::questions::Store::new(&app.state).list(ENTITY, tid) {
                    Ok(questions) => {
                        let asker = v["handle"].as_str().map(|h| format!("operator:handle:{h}"));
                        json!({"questions": questions.iter().filter(|q| asker.as_ref().is_none_or(|a| &q.asker == a))
                            .map(richos_core::questions::Question::public_value).collect::<Vec<_>>()})
                    }
                    Err(e) => json!({"error": e}),
                },
                None => json!({"error": "no such conversation"}),
            },
            "answer-question" => match &thread_id {
                Some(tid) => app.answer_question(tid, &v).unwrap_or_else(|e| json!({"error": e})),
                None => json!({"error": "no such conversation"}),
            },
            "answer" => match &thread_id {
                Some(tid) => match app.desk.deliver_answer(&app.key(tid), v["handle"].as_str(), v["delivery"].as_str().unwrap_or(""),
                                                           v["text"].as_str().unwrap_or("")) {
                    Ok(outcome) => json!({"outcome": format!("{outcome:?}")}),
                    Err(e) => json!({"error": e}),
                },
                None => json!({"error": "no such conversation"}),
            },
            // A named stop, said through the mouth named, reaching the desk through the front
            // desk's own tool. The origin is recorded, never a gate.
            "stop" => match app.thread(&name, v["title"].as_str().unwrap_or("")) {
                Ok(tid) => {
                    let words = v["words"].as_str().unwrap_or("");
                    match app.say(&tid, v["origin"].as_str().unwrap_or("desk-typed"), words) {
                        Ok(turn) => app.desk_tool(&tid, Some(format!("ledger:{tid}:{turn}")), "stop",
                                                  json!({"names": v["names"], "words": words})),
                        Err(e) => json!({"error": e}),
                    }
                }
                Err(e) => json!({"error": e}),
            },
            "interrupt" => match &thread_id {
                Some(tid) => app.desk_tool(tid, None, "interrupt", json!({})),
                None => json!({"error": "no such conversation"}),
            },
            "read" => match app.thread(&name, "") {
                Ok(tid) => app.desk_tool(&tid, None, "read", json!({"every": v["every"].as_bool().unwrap_or(false)})),
                Err(e) => json!({"error": e}),
            },
            "stop-assignment" => match &thread_id {
                Some(tid) => match app.work.stop_assignment(ENTITY, tid, v["handle"].as_str().unwrap_or("")) {
                    Ok(()) => json!({"stopped": true}),
                    Err(e) => json!({"stopped": false, "sentence": e}),
                },
                None => json!({"error": "no such conversation"}),
            },
            "settled" => app.settled(&v),
            "team" => {
                let reading = app.desk.team();
                let (liveness, sentence) = richos_core::work_gate::operator_team(&reading);
                let quit_sheet = richos_core::work_gate::operator_quit_sentence(&app.desk.team_from_stream());
                json!({"alive": reading.alive, "working": reading.working, "unknown": reading.unknown,
                       "descendants": reading.descendants, "descendants_unknown": reading.descendants_unknown,
                       "liveness": format!("{liveness:?}"), "sentence": sentence, "quit_sheet": quit_sheet})
            }
            "retire" => json!({"retired": app.desk.retire_idle_now(Duration::from_secs(v["idle_seconds"].as_u64().unwrap_or(0)))
                .iter().map(|k| app.names.lock().unwrap().by_id.get(&k.thread_id).cloned().unwrap_or_else(|| k.thread_id.clone()))
                .collect::<Vec<_>>()}),
            "take" => match &thread_id {
                Some(tid) => match app.desk.take_pending(&app.key(tid)) {
                    Ok(n) => json!({"taken": n.len()}),
                    Err(e) => json!({"error": e}),
                },
                None => json!({"error": "no such conversation"}),
            },
            "lease-spawns" => json!({"work_leases_opened": app.work_leases.load(std::sync::atomic::Ordering::SeqCst)}),
            "quit" => {
                let quits = app.desk.quit();
                app.socket.close();
                out.line(&json!({"id": id, "quit": quits.iter().map(|(k, q)| json!([k.thread_id, format!("{q:?}")])).collect::<Vec<_>>()}));
                return Ok(());
            }
            other => json!({"error": format!("unknown command {other:?}")}),
        };
        let mut reply = answer;
        reply["id"] = id;
        out.line(&reply);
    }
    // Stdin closed without a quit: the app's own quit path, all the same.
    app.desk.quit();
    app.socket.close();
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("--operator-mcp") => richos_core::operator_report::run_stdio(Path::new(args.get(2).map(String::as_str).unwrap_or("")))
            .map_err(|e| e.to_string()),
        Some("--operator-desk-mcp") => richos_core::operator_desk_tools::run_stdio(Path::new(args.get(2).map(String::as_str).unwrap_or("")))
            .map_err(|e| e.to_string()),
        Some("gate") => {
            println!("{}", gate_json(Path::new(args.get(2).map(String::as_str).unwrap_or(""))));
            Ok(())
        }
        Some("host") if args.len() == 5 => host(Path::new(&args[2]), Path::new(&args[3]), &PathBuf::from(&args[4])),
        _ => Err("usage: operator_walk --operator-mcp <scope> | --operator-desk-mcp <scope> | gate <data> | host <data> <engine-state> <operator dir>".into()),
    };
    if let Err(e) = result {
        eprintln!("operator_walk: {e}");
        std::process::exit(2);
    }
}
