//! THE WORK PATH'S CRASH-MATRIX HOST — his answers to a background job, through a crash
//! (richos-hq `docs/plans/2026-09-27-work-path-answer-delivery-design.md` §4.2, the W4 step of
//! the operator probes' guest runner, retired before that harness was deleted on 2026-10-21).
//!
//! It is the app's own wiring of the ordinary work path, without the window: the real
//! `WorkHost` with the production work lease (`NativeCognition::start_work_lease`, the factory
//! reproduced from `work_lease_roundtrip.rs`, which reproduces `src-tauri/src/main.rs`), the
//! durable question store, the receiving inbox, boot recovery, and the question worker's pass
//! exactly as it ships (`WorkHost::deliver_team_answers`, design D8). Built with the
//! `crash-points` feature for the VM, it aborts where `RICHOS_CRASH_POINT` says, as a crash
//! would, with no cleanup.
//!
//!   work_walk --questions-mcp <scope>              the back end's question tool, as the app serves it
//!   work_walk --claude-reset-mcp <scope> <root> <bin>   its reset tool, as the app serves it
//!   work_walk host <engine> <delivered runtime> <data dir>
//!       one JSON command per line on stdin, one JSON answer per line on stdout; every notice
//!       the host raises is written as {"event":"say",...} as it happens
//!
//! Commands: assign, rows, questions, answer-question, wake, inbox, spawns, quit. `quit` ends
//! the host the way a quit does (every open job is interrupted), so a relaunch that must find
//! a job still waiting is made after a kill, which is a crash. Nothing here is product code.
use richos_core::assignment::{self, AssignmentKind, Registration};
use richos_core::cognition::{Cognition, CognitionError, LeaseFactory};
use richos_core::ecs::EcsBridge;
use richos_core::entity::{EntityId, EntityRegistry, ThreadBinding};
use richos_core::ledger::Ledger;
use richos_core::native::resolve_claude_bin;
use richos_core::work_host::{WorkHost, WorkNotifier};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const ENTITY: &str = "qa-test-co";

struct Out(Mutex<std::io::Stdout>);
impl Out {
    fn line(&self, value: &Value) {
        let mut out = self.0.lock().unwrap();
        if let Err(e) = writeln!(out, "{value}").and_then(|_| out.flush()) {
            eprintln!("work_walk: stdout: {e}: {value}");
        }
    }
}

/// Every notice, on stdout as it is raised: the words he would have been told.
struct Says {
    out: Arc<Out>,
    names: Arc<Mutex<HashMap<String, String>>>,
}
impl WorkNotifier for Says {
    fn raised(&self, thread_id: &str, notice: &assignment::PendingNotice) {
        let name = self.names.lock().unwrap().iter().find(|(_, id)| *id == thread_id).map(|(n, _)| n.clone());
        self.out.line(&json!({"event": "say", "thread": name.unwrap_or_else(|| thread_id.to_string()),
                              "assignment": notice.assignment_id, "kind": format!("{:?}", notice.kind), "text": notice.text}));
    }
}

/// The production factory's `spawn_work` (`src-tauri/src/main.rs`, via `work_lease_roundtrip.rs`),
/// counting every lease it opens, so a cell can say how many fresh back ends a retry took.
struct RealWorkLeases {
    engine: PathBuf,
    data: PathBuf,
    runtime: richos_core::runtime::EngineRuntime,
    doctrine: PathBuf,
    skills: PathBuf,

    spawns: Arc<AtomicUsize>,
}
impl LeaseFactory for RealWorkLeases {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Err(CognitionError::Protocol("this walk opens work leases only".into()))
    }
    fn spawn_work(&self, binding: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        let mut profile = richos_core::engine_profile::EngineProfile::prepare(&self.engine, &self.data, self.runtime.clone())
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        profile.scope_to(binding).map_err(|e| CognitionError::Io(e.to_string()))?;

        let bridge = EcsBridge::new(&self.runtime.python, &self.engine, &self.data.join("ecs"))
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        let lease = richos_core::native::NativeCognition::start_work_lease(
            &resolve_claude_bin(), &self.doctrine, &self.skills,
            &std::env::current_exe().map_err(|e| CognitionError::Io(e.to_string()))?, bridge, profile)?;
        Ok(Box::new(lease))
    }
}

struct Walk {
    data: PathBuf,
    state: PathBuf,
    host: Arc<WorkHost>,
    ledger: Mutex<Ledger>,
    names: Arc<Mutex<HashMap<String, String>>>,
    bridge: EcsBridge,
    spawns: Arc<AtomicUsize>,
    turns: AtomicUsize,
}

impl Walk {
    fn names_path(&self) -> PathBuf {
        self.data.join("walk-threads.json")
    }

    fn binding(&self, thread: &str) -> Option<ThreadBinding> {
        self.ledger.lock().unwrap().thread_binding(thread).ok()
    }

    fn thread(&self, name: &str) -> Result<String, String> {
        if let Some(id) = self.names.lock().unwrap().get(name) {
            return Ok(id.clone());
        }
        let entity = EntityId::parse(ENTITY).map_err(|e| e.to_string())?;
        let id = self.ledger.lock().unwrap().create_thread(name, &entity).map_err(|e| e.to_string())?;
        self.names.lock().unwrap().insert(name.to_string(), id.clone());
        std::fs::write(self.names_path(), serde_json::to_vec(&*self.names.lock().unwrap()).unwrap())
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn wake(&self) {
        self.host.deliver_team_answers(&|thread| self.binding(thread));
    }

    /// His request on a conversation: the turn written to the conversation ledger the way the
    /// app writes it (the brief reads it back verbatim, `work_host::instruction_for`), the
    /// obligation opened the way the register opens it, then `register`.
    fn assign(&self, v: &Value) -> Result<Value, String> {
        let name = v["thread"].as_str().ok_or("Name the conversation")?;
        let title = v["title"].as_str().ok_or("Give a title")?;
        let text = v["text"].as_str().ok_or("Give the request")?;
        let thread = self.thread(name)?;
        let binding = self.binding(&thread).ok_or("The conversation has no binding")?;
        let turn = format!("turn-{}-{}", self.turns.fetch_add(1, Ordering::SeqCst), uuid::Uuid::new_v4().simple());
        let row = json!({"event": "PromptReceived", "turn_id": turn, "thread_id": thread, "entity_id": ENTITY,
                         "source": "text", "text": text});
        let mut ledger = std::fs::OpenOptions::new().create(true).append(true)
            .open(self.data.join("conversation-ledger.jsonl")).map_err(|e| e.to_string())?;
        writeln!(ledger, "{row}").and_then(|_| ledger.sync_all()).map_err(|e| e.to_string())?;
        let obligation = format!("work-{}", uuid::Uuid::new_v4().simple());
        let seat = richos_core::ecs::ceo_seat(&thread);
        let session = format!("walk-{}", uuid::Uuid::new_v4());
        let ceo = self.bridge.bind(ENTITY, &thread, &session, &turn, seat.as_deref(), "ceo").map_err(|e| e.0)?;
        let opened = self.bridge.request("checkpoint", richos_core::ecs::seated_request(seat.as_deref(), json!({
            "binding": ceo, "request_id": format!("assignment-obligation:{obligation}"),
            "checkpoint": {"statements": [{"verb": "commitment", "fields": {"id": obligation, "title": title}}]},
        }))).map_err(|e| e.0)?;
        if opened["accepted"] != true {
            return Err(format!("the obligation was not opened: {opened}"));
        }
        let receipt = assignment::register_kind(&self.state, &Registration {
            entity_id: ENTITY.into(),
            thread_id: thread.clone(),
            obligation_id: obligation.clone(),
            instruction_ledger_ref: format!("ledger:{thread}:{turn}"),
            instruction_sha256: format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(text.as_bytes())),
            title: title.into(),
            repositories: vec![],
            needs_screen: false,
        }, AssignmentKind::Task).map_err(|e|e.to_string())?;
        let scope=richos_core::status_tools::StatusToolScope {version:1,state_root:self.state.clone(),entity_id:ENTITY.into(),thread_id:thread.clone()};
        let sender=self.data.join(format!("walk-rich-{thread}.json"));
        richos_core::session_tools::bind(&sender,&scope,&session,"rich",Some(richos_core::ecs::UserInstruction {
            ledger_ref:format!("ledger:{thread}:{turn}"),sha256:format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(text.as_bytes())),text:Some(text.into())}))?;
        richos_core::session_tools::delegate(&sender,&scope,json!({"text":text}))?;
        self.host.wake_messages(&binding);

        Ok(json!({"assignment": receipt.id, "obligation": obligation, "thread_id": thread}))
    }

    fn rows(&self, v: &Value) -> Result<Value, String> {
        let thread = self.thread(v["thread"].as_str().ok_or("Name the conversation")?)?;
        let rows = assignment::read_all(&self.state, ENTITY, &thread).map_err(|e| e.to_string())?;
        Ok(json!({"rows": rows.iter().map(|r| json!({"id": r.id, "obligation": r.obligation_id, "state": r.state.as_str(),
            "detail": r.detail, "work_session": r.work_session,
            "notices": r.notices.iter().map(|n| json!({"kind": format!("{:?}", n.kind), "text": n.text})).collect::<Vec<_>>()}))
            .collect::<Vec<_>>()}))
    }

    fn questions(&self, v: &Value) -> Result<Value, String> {
        let thread = self.thread(v["thread"].as_str().ok_or("Name the conversation")?)?;
        let list = richos_core::questions::Store::new(&self.state).list(ENTITY, &thread)?;
        Ok(json!({"questions": list.iter().map(|q| {
            let mut card = q.public_value();
            card["internal_delivered"] = json!(q.delivered);
            card["awaiting_taker"] = json!(q.awaiting_taker);
            card
        }).collect::<Vec<_>>()}))
    }

    /// His answer on this Mac, into the durable store, then the store wake the app's observer
    /// gives the question worker.
    fn answer_question(&self, v: &Value) -> Result<Value, String> {
        let thread = self.thread(v["thread"].as_str().ok_or("Name the conversation")?)?;
        let request: richos_core::questions::AnswerRequest =
            serde_json::from_value(v["answer"].clone()).map_err(|e| e.to_string())?;
        let result = richos_core::questions::Store::new(&self.state).answer(ENTITY, &thread, request, "click", "mac")?;
        self.wake();
        Ok(result.public_value())
    }

    /// The receiving inbox, as files: what is pending, taken (and in which session), discarded.
    fn inbox(&self) -> Value {
        let mut rows = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.state.join("questions/work-inputs")) {
            for entry in entries.flatten() {
                if let Ok(value) = std::fs::read(entry.path()).map_err(|e| e.to_string())
                    .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string())) {
                    rows.push(json!({"id": value["delivery"]["id"], "thread_id": value["delivery"]["thread_id"],
                                     "handed": value["handed"], "taken_in": value["taken_in"], "discarded": value["discarded"]}));
                }
            }
        }
        json!({"inputs": rows})
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--questions-mcp") => {
            richos_core::question_tools::run_stdio(Path::new(args.get(1).ok_or("Missing question scope")?))?;
            return Ok(());
        }
        Some("--claude-reset-mcp") if args.len() >= 4 => {
            richos_core::quota::reset_tools::run_stdio(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]))?;
            return Ok(());
        }
        Some("host") if args.len() == 4 => {}
        _ => return Err("usage: work_walk host ENGINE DELIVERED_RUNTIME DATA_DIR".into()),
    }
    let engine = std::fs::canonicalize(&args[1])?;
    let runtime = richos_core::runtime::EngineRuntime::load(&engine, Some(Path::new(&args[2])))?;
    let data = PathBuf::from(&args[3]);
    std::fs::create_dir_all(data.join("corpus/ceo/records"))?;
    let state = data.join("engine-state");
    let entity = EntityId::parse(ENTITY)?;
    if !data.join("entities.json").exists() {
        EntityRegistry::from_existing_ids(std::slice::from_ref(&entity)).save(&data.join("entities.json"))?;
    }
    let out = Arc::new(Out(Mutex::new(std::io::stdout())));
    let names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(
        std::fs::read(data.join("walk-threads.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()));

    // Old scopes are closed; execution recovery remains with the receiving owner.
    let reconciled=richos_core::recovery::reconcile(&state);
    out.line(&json!({"event":"recovery","log":reconciled.log_message(),
        "grants_closed":reconciled.grants_closed,"grants_unreconciled":reconciled.grants_unreconciled}));

    let spawns = Arc::new(AtomicUsize::new(0));
    let host = WorkHost::new(&state, Box::new(RealWorkLeases {
        engine: engine.clone(), data: data.clone(), runtime: runtime.clone(),
        doctrine: richos_core::doctrine::ensure_rendered(&data, &Default::default())?,
        skills: richos_core::skills::ensure_rendered(&data, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app"))?,
        spawns: spawns.clone(),
    }), Arc::new(Says { out: out.clone(), names: names.clone() }));
    let walk = Walk {
        ledger: Mutex::new(Ledger::open(data.join("walk-ledger.jsonl"))?),
        bridge: EcsBridge::new(&runtime.python, &engine, &data.join("ecs"))?,
        data, state, host, names, spawns, turns: AtomicUsize::new(0),
    };
    for id in walk.names.lock().unwrap().values() {
        if let Some(binding) = walk.binding(id) {
            walk.host.remember_binding(&binding);
        }
    }
    walk.host.start();
    // The launch wake: the app's observer wakes the question worker once at start.
    walk.wake();
    out.line(&json!({"ready": true, "pid": std::process::id()}));

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let id = v["id"].clone();
        let result = match v["cmd"].as_str().unwrap_or("") {
            "assign" => walk.assign(&v),
            "rows" => walk.rows(&v),
            "questions" => walk.questions(&v),
            "answer-question" => walk.answer_question(&v),
            "wake" => { walk.wake(); Ok(json!({"woke": true})) }
            "inbox" => Ok(walk.inbox()),
            "spawns" => Ok(json!({"spawns": walk.spawns.load(Ordering::SeqCst)})),
            "quit" => {
                walk.host.shutdown();
                out.line(&json!({"id": id, "ok": {"quit": true}}));
                return Ok(());
            }
            other => Err(format!("unknown command {other}")),
        };
        out.line(&match result {
            Ok(value) => json!({"id": id, "ok": value}),
            Err(error) => json!({"id": id, "error": error}),
        });
    }
    walk.host.shutdown();
    Ok(())
}
