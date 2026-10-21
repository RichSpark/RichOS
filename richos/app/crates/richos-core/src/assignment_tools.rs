//! Scoped operational assignment records and their existing ECS obligations.
//! Records preserve work facts. They do not send a message or dispatch execution.
//! Rich and Stu communicate through the durable peer inbox. Stu is an independent
//! top-level session and selects obligations by the identifiers returned here.

use crate::assignment::{self, AssignmentKind, Registration};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

pub const SERVER_NAME: &str = "richos_assignments";
pub const RECORD_TOOL_NAME: &str = "record";
pub const QUALIFIED_RECORD_TOOL: &str = "mcp__richos_assignments__record";

const MAX_FRAME_BYTES: usize = 256 * 1024;
const MAX_SCOPE_BYTES: u64 = 16 * 1024;

/// The scope the app writes for this lease, and the only thing this server trusts.
///
/// Every field a model could use to redirect the record — the company, the conversation,
/// the state root, the instruction — is here and none of them is a tool argument. The
/// model supplies two things: what the assignment is, in the CEO's own terms, and which
/// repositories it touches.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentToolScope {
    pub version: u32,
    /// The app's grant for one visible conversation turn. Missing is denied — the same
    /// rule the other two app-owned scopes follow, and the reason a background lease
    /// cannot register more work for itself.
    #[serde(default)]
    pub actions_allowed: bool,
    /// `<data_dir>/engine-state`. The assignment register lives beside the work receipts.
    pub state_root: PathBuf,
    pub entity_id: String,
    pub thread_id: String,
    /// `ledger:<thread>:<turn>` — the turn he is speaking in, frozen into the assignment
    /// (spec §3.6).
    pub instruction_ledger_ref: String,
    /// The host's SHA-256 of that turn's exact ledger text. **His words are not copied
    /// here**; the digest is what attests them, and it is computed where they already are.
    pub instruction_sha256: String,
    /// **Where this register opens the obligation** — see the module doc.
    ///
    /// `#[serde(default)]`, so a scope written by an older build still reads; and a register
    /// that finds no desk REFUSES rather than recording an assignment nothing could ever
    /// dispatch (`no_obligation_desk_records_nothing`). Fail-closed is the only safe
    /// direction here: the failure it replaces was silent and only showed up on the work
    /// lease, minutes later, as a refused `prepare`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_desk: Option<ObligationDesk>,
}

/// The app's own seat at the engine's continuity store, carried into the register's
/// short-lived process so it can open the obligation without a model round trip.
///
/// **Every field here is the host's.** They are the same three values `prepare_work_turn`
/// wrote into the continuity scope one line earlier — the bridge it just spoke to, the binding
/// it just bound, and the CEO's own per-thread seat — so the item opens on his cursor, in his
/// thread, fenced exactly as a checkpoint of his own would be (`engine/ecs/adapters/app.py`'s
/// `CONVERSATION_ONLY`: `checkpoint` is refused on any seat but the conversation's).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationDesk {
    pub bridge: crate::ecs::EcsBridge,
    pub binding: crate::ecs::Binding,
    /// `ceo-thread:<thread_id>`, or `None` on an engine without per-thread seats — the same
    /// value the continuity scope carries, for the reason stated there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
}

/// Opening the obligation, as a seam — **so the refusal paths are unit-tested without a
/// Python interpreter and `cargo test -p richos-core` stays native-dep-free.**
///
/// The production implementation is [`EcsObligations`].
pub trait ObligationOpener {
    /// Open the obligation, or say why not in a sentence that can be read aloud.
    fn open(&self, desk: &ObligationDesk, obligation: &str, title: &str, kind: AssignmentKind)
        -> Result<(), String>;
    /// Best-effort undo for an obligation whose assignment then failed to be written. It
    /// cannot be relied on — hence "best effort" — and a failure here is never reported to the
    /// CEO, because the thing he is waiting to hear is that his request was not recorded.
    fn abandon(&self, desk: &ObligationDesk, obligation: &str);
}

/// The engine's own terminal status for an obligation the app opened and could not use
/// (`engine/ecs/core/ecs_core.py:1067`), which needs no evidence where `completed` would.
const ABANDONED_OBLIGATION_STATUS: &str = "cancelled"; // dialect-exempt: the engine's protocol literal, never prose he reads.

/// The real thing: one `checkpoint` through the engine's own ECS bridge.
pub struct EcsObligations;

impl ObligationOpener for EcsObligations {
    fn open(&self, desk: &ObligationDesk, obligation: &str, title: &str, kind: AssignmentKind)
        -> Result<(), String> {
        // **A piece of work is a `commitment`; a question of his is an `open_loop`.** Both are
        // in the engine's `CONTINUITY_ITEM_TYPES` and both open `status: "active"`, which is in
        // the set `prepare` accepts (`mega-lander/app.py:320`, measured). The distinction is not
        // decoration: this item is what his executive brief shows him next turn, and "a
        // commitment to know why the nightly is red" is not what he asked for.
        let verb = if kind.is_question() { "open_loop" } else { "commitment" };
        let result = desk
            .bridge
            .request(
                "checkpoint",
                crate::ecs::seated_request(
                    desk.seat.as_deref(),
                    json!({
                        "binding": desk.binding,
                        // Derived from the obligation, which is a fresh id per registration.
                        // The identical call twice is a duplicate receipt and one item.
                        "request_id": format!("assignment-obligation:{obligation}"),
                        "checkpoint": {"statements": [{"verb": verb, "fields": {
                            "id": obligation, "title": title,
                        }}]},
                    }),
                ),
            )
            .map_err(|e| e.0)?;
        // **The engine's own verdict, not the absence of an error.** A checkpoint can succeed
        // as a command and reject the statement inside it (`ecs_extract.py`'s
        // `apply_statements` *"never raise past a statement"*), and a rejected statement means
        // no item — which is exactly the state this whole change exists to stop happening
        // silently.
        if result["accepted"] != true {
            let why = result["rejections"].as_array().and_then(|rows| rows.first())
                .and_then(Value::as_str).unwrap_or("the operational record would not take it");
            return Err(why.to_string());
        }
        Ok(())
    }

    fn abandon(&self, desk: &ObligationDesk, obligation: &str) {
        // The app adapter refuses only `completed` and a bare `close`
        // (`engine/ecs/adapters/app.py:186-192`), so this transition is allowed to a model and
        // is certainly allowed to the host that opened the item one call ago.
        let _ = desk.bridge.request(
            "checkpoint",
            crate::ecs::seated_request(
                desk.seat.as_deref(),
                json!({
                    "binding": desk.binding,
                    "request_id": format!("assignment-obligation-abandoned:{obligation}"),
                    "checkpoint": {"statements": [{"verb": "close", "fields": {
                        "id": obligation, "status": ABANDONED_OBLIGATION_STATUS,
                    }}]},
                }),
            ),
        );
    }
}

pub fn write_scope(path: &Path, scope: &AssignmentToolScope) -> Result<(), String> {
    if scope.version != 1 || !scope.state_root.is_absolute() {
        return Err("The app has not supplied a valid assignment scope. Nothing was recorded.".into());
    }
    let text = serde_json::to_string(scope).map_err(|e| e.to_string())?;
    crate::doctrine::write_verified(path, &text).map_err(|e| e.to_string())
}

pub fn set_actions_allowed(path: &Path, allowed: bool) -> Result<(), String> {
    let mut scope = read_scope(path)?;
    scope.actions_allowed = allowed;
    write_scope(path, &scope)
}

pub(crate) fn read_scope(path: &Path) -> Result<AssignmentToolScope, String> {
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|_| "RichOS has not opened this conversation for assignments.".to_string())?;
    let mut text = String::new();
    file.take(MAX_SCOPE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| "The assignment scope could not be read. Nothing was recorded.".to_string())?;
    if text.len() as u64 > MAX_SCOPE_BYTES {
        return Err("The assignment scope is too large. Nothing was recorded.".into());
    }
    let scope: AssignmentToolScope = serde_json::from_str(&text)
        .map_err(|_| "The assignment scope is unreadable. Nothing was recorded.".to_string())?;
    if scope.version != 1 || !scope.state_root.is_absolute() {
        return Err("The assignment scope is not usable. Nothing was recorded.".into());
    }
    Ok(scope)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordArguments {
    assignment: String,
    #[serde(default)]
    repositories: Vec<String>,
    /// **Will this work need the Mac's screen?** The CEO's ruling §56's one input — see
    /// [`assignment::Assignment::needs_screen`].
    ///
    /// A model argument because the app cannot tell from
    /// a title whether a job will drive a window, take a screenshot or walk a build, and only
    /// the thing that read his request knows. It chooses nothing about WHAT happens — the wait
    /// and the sentence are the app's ([`crate::screen`]) — and getting it wrong loses no work
    /// in either direction: a false `true` waits for a screen it did not need, a false `false`
    /// behaves exactly as this app did before §56.
    #[serde(default)]
    needs_screen: bool,
    /// **Is this a piece of work, or a question of his you must go and find out?** The CEO's
    /// ruling §58, 2026-09-18.
    ///
    /// Three values, and the model reports one fact to pick between them: `task` for work he
    /// asked for, `check` for a question whose answer it expects quickly, `investigate` for
    /// one it expects to take more than about a minute. His words on how hard that call is:
    /// *"just a rough estimate whether or not the answer is expected quickly is all we need
    /// to distinguish between "I'll check" or "I'll investigate"."*
    ///
    /// It chooses nothing else. All three sentences are the app's
    /// ([`assignment::Receipt::sentence`]), so a model that gets this wrong says the other
    /// short reply rather than a paragraph of its own. Absent means `task`, which is what
    /// every caller before today meant.
    #[serde(default)]
    kind: Option<String>,

}

pub fn tools() -> Value {
    json!({"tools":[
        {"name":RECORD_TOOL_NAME,
         "description":"Create an operational work record with its original user-request reference and an open obligation. Returns registration facts without waiting for execution. Registration never dispatches work. Send a peer handoff separately. Reuse existing records for existing work; background_work exposes their assignment_id and obligation_id. Stu uses the selected obligation_id with work tools.",
         "inputSchema":{"type":"object","properties":{
             "assignment":{"type":"string","minLength":1,"maxLength":4096,"description":"Your complete handoff to Stu: outcome or question, relevant context, constraints and decisions. Preserved in full; a shortened display title is stored separately."},
             "repositories":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":4096},"description":"Absolute paths of the repositories this assignment touches, if he named any. Omit when he did not. Only a repository connected to this company is kept; any other path is left out of the record."},
             "needs_screen":{"type":"boolean","description":"True if this work needs the Mac's screen to be unlocked: it drives the app's own window, takes screenshots, walks a build on screen or otherwise cannot be done while the screen is locked. If the screen is locked when this comes up, the app waits for the unlock and carries on by itself; nobody is asked anything. Omit it for ordinary work, which is almost all work."},

             "kind":{"type":"string","enum":["task","check","investigate"],"description":"What this is. `task`: work he asked for; omit it and you get this. `check`: a QUESTION of his whose answer you expect quickly, because it is on file somewhere and only has to be looked up. `investigate`: a QUESTION of his that needs real digging: repositories, logs, the web, several places. This is operational metadata, not a reply template or an execution route."}
         },"required":["assignment"],"additionalProperties":false},
         "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}
    ]})
}

/// Validation and the record, with the real ECS desk behind it.
pub fn call(scope_path: &Path, name: &str, arguments: Value) -> Result<Value, String> {
    call_with(scope_path, name, arguments, &EcsObligations)
}

/// Validation and the record, shared by the protocol adapter and the tests.
///
/// `opener` is how the obligation is opened — [`EcsObligations`] in production, a stub in the
/// unit tests, which is what keeps every refusal path here provable without an interpreter.
pub fn call_with(
    scope_path: &Path,
    name: &str,
    arguments: Value,
    opener: &dyn ObligationOpener,
) -> Result<Value, String> {
    if name != RECORD_TOOL_NAME {
        return Err("That assignment tool does not exist. Nothing was recorded.".into());
    }
    let args: RecordArguments = serde_json::from_value(arguments).map_err(|_| {
        "Use only assignment, repositories, needs_screen and kind. Nothing was recorded."
            .to_string()
    })?;
    // Reject unknown kinds before opening an obligation.
    let kind = match args.kind.as_deref() {
        None => assignment::AssignmentKind::Task,
        Some(word) => assignment::AssignmentKind::parse(word)
            .map_err(|e| format!("{e}. Nothing was recorded."))?,
    };
    let scope = read_scope(scope_path)?;
    if !scope.actions_allowed {
        // Spec §5.1's rule, in its own place: with no visible turn the answer is a refusal,
        // and it names what is missing rather than implying the work began.
        return Err(
            "This conversation is not open for new assignments right now. Nothing was recorded.".into(),
        );
    }
    // ===================================================================================
    // THE OBLIGATION, OPENED HERE, WITH AN ID THE MODEL NEVER SEES
    // ===================================================================================
    //
    // **The title is sanitized BEFORE the obligation is opened**, and that ordering is the
    // cheap half of "no orphan": a title the app would refuse is refused with nothing written
    // anywhere. Everything else `register_kind` validates is the scope's, which this function
    // has already read.
    let title = assignment::sanitize_title(&args.assignment)
        .map_err(|e| assignment::failed_registration_sentence(&format!("{e}.")))?;
    let Some(desk) = scope.obligation_desk else {
        // Fail-closed, and it names what is missing without implying a start — the same shape
        // the closed-grant refusal above uses. An assignment recorded with no obligation behind
        // it is the defect this whole change removes; recording one anyway to be helpful would
        // be reproducing it.
        return Err("RichOS cannot open a record for new work in this conversation right now. Nothing was recorded.".into());
    };
    // `work-<uuid>`: no colon (the seat is spelled `work-seat:<obligation_id>`), inside the
    // engine's own `SAFE_ID` and inside `assignment.rs`'s `usable_identity`.
    let obligation_id = format!("work-{}", uuid::Uuid::new_v4().simple());
    opener
        .open(&desk, &obligation_id, &title, kind)
        .map_err(|why| assignment::failed_registration_sentence(&format!("{why}.")))?;
    let repositories = connected_only(&scope.state_root, &scope.entity_id, &args.repositories);
    let receipt = assignment::register_kind(
        &scope.state_root,
        &Registration {
            entity_id: scope.entity_id,
            thread_id: scope.thread_id,
            obligation_id: obligation_id.clone(),
            instruction_ledger_ref: scope.instruction_ledger_ref,
            instruction_sha256: scope.instruction_sha256,
            title: args.assignment,
            repositories,
            needs_screen: args.needs_screen,
        },
        kind,
    )
    .map_err(|e| {
        // The obligation is open and nothing will ever carry it out, so it is taken back
        // before the refusal goes out. Best effort: see [`ObligationOpener::abandon`].
        opener.abandon(&desk, &obligation_id);
        assignment::failed_registration_sentence(&format!("{e}."))
    })?;
    // Rich interprets these receipt facts and authors his own acknowledgment.
    Ok(json!({
        "recorded": true,
        "assignment_id": receipt.id, "obligation_id": obligation_id,
        "state": "registered", "kind": receipt.kind.as_str(),
    }))
}

/// **Only a repository he connected to this conversation's company is recorded** — the
/// leftover of esc-20260927T093052Z-85f3303f.
///
/// `repositories` is the one piece of model text this register puts into the record, and the
/// record feeds the back end's brief ("Repositories:", `work_host.rs`'s `brief_for`) and the
/// recovery pin. It grants nothing (the back end's `repositories` tool reads the registry
/// itself), so a made-up path could only ever mislead: in the 2026-09-27 VM run the front desk
/// named the company's central folder, which did not exist.
///
/// So each named path is read against the app's own company registry — the file the app, the
/// Connected repositories sheet and the back end's `repositories` tool all read
/// (`<data_dir>/entities.json`, the state root's parent) — and kept only as the connected
/// repository it is, or is inside. Anything else is left out, never replaced by a guess; a
/// relative path, a `..`, a missing or unreadable registry and an unknown company all leave
/// it out too. Leaving a path out refuses nothing: the assignment is still recorded, and the
/// back end still finds the connected repositories for itself.
fn connected_only(state_root: &Path, entity_id: &str, named: &[String]) -> Vec<String> {
    let connected: Vec<PathBuf> = match (state_root.parent(), crate::entity::EntityId::parse(entity_id)) {
        (Some(data_dir), Ok(id)) => crate::entity::EntityRegistry::load(&crate::entity::entity_registry_path(data_dir))
            .registry
            .get(&id)
            .map(|entity| entity.connected_repositories.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut kept: Vec<String> = Vec::new();
    let mut left_out = 0usize;
    for name in named {
        let path = Path::new(name);
        let usable = path.is_absolute() && !path.components().any(|c| c == std::path::Component::ParentDir);
        match connected.iter().find(|root| usable && path.starts_with(root)) {
            Some(root) => {
                let root = root.to_string_lossy().into_owned();
                if !kept.contains(&root) {
                    kept.push(root);
                }
            }
            None => left_out += 1,
        }
    }
    if left_out > 0 {
        eprintln!("[richos] register: left out {left_out} named repository path(s) not connected to this company");
    }
    kept
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn response(
    scope: &Path,
    request: Value,
    initialized: &mut bool,
    opener: &dyn ObligationOpener,
) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
        return Some(error(id.unwrap_or(Value::Null), -32600, "Invalid JSON-RPC request"));
    }
    // MCP notifications never receive a response or execute a tool.
    let id = id?;
    if !id.is_string() && !id.is_number() {
        return Some(error(Value::Null, -32600, "Invalid request id"));
    }
    let result = match method.unwrap() {
        "initialize" => {
            *initialized = true;
            let requested = request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("");
            let protocol = match requested {
                "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => requested,
                _ => "2025-11-25",
            };
            json!({"protocolVersion":protocol,"capabilities":{"tools":{}},"serverInfo":{"name":SERVER_NAME,"version":"1.0.0"}})
        }
        "ping" => json!({}),
        _ if !*initialized => return Some(error(id, -32002, "Initialize before recording assignments")),
        "tools/list" => tools(),
        "tools/call" => {
            let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "A tool name is required"));
            };
            let args = request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
            match call_with(scope, name, args, opener) {
                Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}],"isError":false}),
                Err(why) => json!({"content":[{"type":"text","text":why}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

/// Newline-delimited JSON-RPC with bounded allocation. An oversized message is drained
/// through its newline, rejected, and cannot be read as the command that follows it.
pub fn serve(scope_path: &Path, reader: impl BufRead, writer: impl Write) -> io::Result<()> {
    serve_with(scope_path, reader, writer, &EcsObligations)
}

/// The transport, with the obligation desk injected — see [`call_with`].
pub fn serve_with(
    scope_path: &Path,
    mut reader: impl BufRead,
    mut writer: impl Write,
    opener: &dyn ObligationOpener,
) -> io::Result<()> {
    let mut initialized = false;
    loop {
        let mut frame = Vec::new();
        let mut oversized = false;
        loop {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            let n = buf.iter().position(|b| *b == b'\n').map(|i| i + 1).unwrap_or(buf.len());
            let ends = buf[n - 1] == b'\n';
            if frame.len() + n <= MAX_FRAME_BYTES && !oversized {
                frame.extend_from_slice(&buf[..n]);
            } else {
                oversized = true;
            }
            reader.consume(n);
            if ends {
                break;
            }
        }
        if frame.is_empty() && !oversized {
            break;
        }
        let reply = if oversized {
            Some(error(Value::Null, -32600, "Message too large"))
        } else {
            match serde_json::from_slice::<Value>(&frame) {
                Ok(value) => response(scope_path, value, &mut initialized, opener),
                Err(_) => Some(error(Value::Null, -32700, "Invalid JSON")),
            }
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut writer, &reply)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Entry point used before the desktop shell initializes. Stdout is exclusively MCP frames.
pub fn run_stdio(scope_path: &Path) -> io::Result<()> {
    serve(scope_path, io::stdin().lock(), io::stdout().lock())
}

/// Did the child actually load this tool? Read from its own init inventory, never from the
/// fact that the config was accepted — the same rule `onboarding_tools::verdict_from_init`
/// follows, and for the same reason: `--mcp-config` takes a server that never connects
/// without saying so.
pub fn loaded_from_init(init: &Value) -> bool {
    init.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|tool| tool.as_str() == Some(QUALIFIED_RECORD_TOOL)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// **The fixture cleans itself up however the test ends** (CEO §54, 2026-09-18).
    ///
    /// The escalation that made this a rule: the richos-core suite left one directory per
    /// fixture in `TMPDIR`, **66,922 of them counted on main at `32238312`**, because every
    /// fixture here removed its root on the LAST line of the test and an assertion that fired
    /// earlier skipped it. A drop guard runs on the panic path too, which is the path that
    /// leaked.
    struct Fixture {
        root: PathBuf,
        scope: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    impl Fixture {
        fn state(&self) -> PathBuf {
            self.root.join("engine-state")
        }
        fn rows(&self) -> Vec<assignment::Assignment> {
            assignment::read_all(&self.state(), "depot", "thread-one").unwrap()
        }
    }

    /// A desk whose three values are never dereferenced by the stub opener below. The paths are
    /// absolute because the type is the host's and an absolute path is what the host writes.
    fn desk() -> ObligationDesk {
        ObligationDesk {
            bridge: crate::ecs::EcsBridge {
                python: PathBuf::from("/fictional/runtime/bin/python3"),
                component: PathBuf::from("/fictional/engine/ecs"),
                state_root: PathBuf::from("/fictional/state/ecs"),
            },
            binding: crate::ecs::Binding {
                entity_id: "depot".into(),
                thread_id: "thread-one".into(),
                session_id: "session-one".into(),
                turn_id: "turn-7".into(),
                audience: "ceo".into(),
                revision: 3,
            },
            seat: Some("ceo-thread:thread-one".into()),
        }
    }

    fn fixture() -> Fixture {
        fixture_with_desk(Some(desk()))
    }

    fn fixture_with_desk(obligation_desk: Option<ObligationDesk>) -> Fixture {
        let root = std::env::temp_dir().join(format!("richos-assignment-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let scope = root.join("assignments.json");
        write_scope(
            &scope,
            &AssignmentToolScope {
                version: 1,
                actions_allowed: true,
                state_root: root.join("engine-state"),
                entity_id: "depot".into(),
                thread_id: "thread-one".into(),
                instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
                instruction_sha256: "a".repeat(64),
                obligation_desk,
            },
        )
        .unwrap();
        Fixture { root, scope }
    }

    /// What the register asked the ECS desk to do, recorded rather than performed — so every
    /// path through [`call_with`] is provable with no interpreter, no engine and no store.
    #[derive(Default)]
    struct Opened {
        opens: Mutex<Vec<(String, String, String)>>,
        abandoned: Mutex<Vec<String>>,
        refuse: Option<String>,
    }
    impl Opened {
        fn refusing(why: &str) -> Self {
            Opened { refuse: Some(why.into()), ..Default::default() }
        }
        fn ids(&self) -> Vec<String> {
            self.opens.lock().unwrap().iter().map(|(id, _, _)| id.clone()).collect()
        }
    }
    impl ObligationOpener for Opened {
        fn open(&self, _desk: &ObligationDesk, obligation: &str, title: &str, kind: AssignmentKind)
            -> Result<(), String> {
            if let Some(why) = &self.refuse {
                return Err(why.clone());
            }
            self.opens.lock().unwrap().push((
                obligation.to_string(),
                title.to_string(),
                kind.as_str().to_string(),
            ));
            Ok(())
        }
        fn abandon(&self, _desk: &ObligationDesk, obligation: &str) {
            self.abandoned.lock().unwrap().push(obligation.to_string());
        }
    }

    /// The company registry beside the fixture's state root (`<data_dir>/entities.json`, the
    /// file the app and the back end's `repositories` tool read), with `connected` connected
    /// to the scope's company.
    fn connect(fixture: &Fixture, connected: &[&str]) {
        let id = crate::entity::EntityId::parse("depot").unwrap();
        let mut registry = crate::entity::EntityRegistry::empty();
        registry.register(crate::entity::Entity::new("depot", "Depot", &[]).unwrap()).unwrap();
        for path in connected {
            registry.connect_repository(&id, PathBuf::from(path)).unwrap();
        }
        registry.save(&crate::entity::entity_registry_path(&fixture.root)).unwrap();
    }

    /// **A REPOSITORY THE FRONT DESK NAMES IS RECORDED ONLY IF HE CONNECTED IT** — the
    /// leftover of esc-20260927T093052Z-85f3303f.
    ///
    /// `repositories` is model text ("Absolute paths of the repositories this assignment
    /// touches"), and `register_kind` checks only emptiness, length and newlines, so a path the
    /// model made up — the company's central folder, which did not exist — was recorded, and
    /// put a false "Repositories:" line into the back end's brief (`work_host.rs`'s
    /// `brief_for`) and an unreadable recovery pin. Now each named path is checked against the
    /// repositories connected to THIS conversation's company, read from the app's own
    /// registry: a connected repository, or a path inside one, is recorded as that repository;
    /// anything else is left out. Nothing is invented and the registration still succeeds.
    ///
    /// RED at `bfb20a48`: the made-up, relative and other-company paths were all recorded.
    #[test]
    fn a_repository_the_front_desk_names_is_recorded_only_if_he_connected_it() {
        let fixture = fixture();
        connect(&fixture, &["/fictional/acme"]);
        let opened = Opened::default();
        let named = json!(["/fictional/acme", "/fictional/acme/src/", "/fictional/central/Acme",
                           "fictional/acme", "/fictional/acme-other"]);
        let result = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"count the commits in Acme","repositories":named}), &opened).unwrap();
        assert_eq!(result["recorded"], true, "a made-up path refused the whole registration: {result}");
        assert_eq!(fixture.rows()[0].repositories, vec!["/fictional/acme".to_string()]);

        // No registry at all (nothing connected yet): nothing is recorded, nothing refused.
        let bare = fixture_with_desk(Some(desk()));
        let result = call_with(&bare.scope, RECORD_TOOL_NAME,
            json!({"assignment":"count the commits in Acme","repositories":["/fictional/acme"]}), &opened).unwrap();
        assert_eq!(result["recorded"], true);
        assert!(bare.rows()[0].repositories.is_empty(), "{:?}", bare.rows()[0].repositories);

        // A registry that cannot be read is not a list of connected repositories either.
        let broken = fixture_with_desk(Some(desk()));
        std::fs::write(crate::entity::entity_registry_path(&broken.root), "{").unwrap();
        call_with(&broken.scope, RECORD_TOOL_NAME,
            json!({"assignment":"count the commits in Acme","repositories":["/fictional/acme"]}), &opened).unwrap();
        assert!(broken.rows()[0].repositories.is_empty(), "{:?}", broken.rows()[0].repositories);
    }

    /// Spec §1.1/§1.2: one call, a durable record, and a sentence to end the turn with — and
    /// since 2026-09-18, the OBLIGATION opened by the same call.
    #[test]
    fn recording_opens_the_obligation_and_returns_only_registration_facts() {
        let fixture = fixture();
        connect(&fixture, &["/fictional/project"]);
        let desk = Opened::default();
        let result = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the three branches","repositories":["/fictional/project"]}), &desk)
            .unwrap();
        assert_eq!(result["recorded"], true);
        assert!(result.get("say").is_none());
        assert!(result.get("say_nothing_else").is_none());
        assert_eq!(result["state"], "registered");
        // **THE OBLIGATION WAS OPENED, ONCE, BY THIS CALL, WITH THE RECORD'S OWN ID.** That
        // identity is the join the work lease depends on: `prepare` reads the obligation off
        // the seat's binding and refuses a dispatch unless its item is open
        // (`mega-lander/app.py:318-321`).
        let rows = fixture.rows();
        assert_eq!(rows.len(), 1);
        let opens = desk.opens.lock().unwrap().clone();
        assert_eq!(opens.len(), 1, "one registration must open exactly one obligation: {opens:?}");
        assert_eq!(opens[0].0, rows[0].obligation_id, "the record and the obligation disagree");
        assert_eq!(opens[0].1, "landing the three branches", "his own words did not reach the record");
        assert_eq!(opens[0].2, "task");
        assert!(desk.abandoned.lock().unwrap().is_empty(), "a successful registration took its obligation back");
        // The id is the APP's: bounded, colon-free (the seat is spelled `work-seat:<id>`), and
        // nothing the model could have named.
        assert!(rows[0].obligation_id.starts_with("work-"), "{}", rows[0].obligation_id);
        assert_eq!(rows[0].seat, format!("work-seat:{}", rows[0].obligation_id));
        assert_eq!(rows[0].state, assignment::AssignmentState::Registered);
        assert_eq!(rows[0].instruction_ledger_ref, "ledger:thread-one:turn-7");
        assert_eq!(rows[0].repositories, vec!["/fictional/project".to_string()]);
        assert_eq!(result["assignment_id"], rows[0].id);
        assert_eq!(result["obligation_id"], rows[0].obligation_id);
        let handoff = "The pricing review. Preserve these agreed constraints: ".to_string() + &"context and decisions. ".repeat(30);
        let asked = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":handoff}), &desk).unwrap();
        assert!(asked.get("say").is_none());
        let saved = fixture.rows().into_iter().find(|row| row.id == asked["assignment_id"].as_str().unwrap()).unwrap();
        assert_eq!(saved.handoff, handoff);
        assert!(saved.title.chars().count() <= 161);
        let ids = desk.ids();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "two assignments shared one obligation, and so one seat");
    }

    /// **THE MODEL CANNOT NAME, GUESS OR REDIRECT THE OBLIGATION, because it is not an
    /// argument any more** — the CEO's §55 leg landed 2026-09-18.
    ///
    /// The engine reached this conclusion first, for the sibling field one level down:
    /// *"Naming the obligation in the brief instead would have been the wrong fix: it puts an
    /// identifier in front of a model, which is the thing that invites an invented one"*
    /// (`mega-lander/app.py`'s `carried_obligation`). An `obligation_id` the model still sent
    /// is refused outright rather than ignored, because an ignored field is how a caller learns
    /// it was accepted.
    #[test]
    fn the_obligation_is_not_a_model_argument_at_all() {
        let fixture = fixture();
        let desk = Opened::default();
        let refused = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"obligation_id":"obl-add-notes-line","assignment":"landing the branches"}), &desk)
            .unwrap_err();
        assert!(refused.contains("Nothing was recorded."), "{refused}");
        assert!(fixture.rows().is_empty());
        assert!(desk.opens.lock().unwrap().is_empty(), "a refused shape still opened an obligation");
        // It is not in the tool's own schema either, so a model reading the description is not
        // being asked for one.
        let schema = tools()["tools"][0]["inputSchema"].clone();
        assert!(schema["properties"].get("obligation_id").is_none(), "{schema}");
        assert_eq!(schema["required"], json!(["assignment"]));
        // And the description tells it there is nothing to open first — the sentence that used
        // to say the opposite is what produced the pre-reply checkpoint that cost him 3.0 s.
        let description = tools()["tools"][0]["description"].as_str().unwrap().to_string();
        assert!(description.contains("an open obligation"), "{description}");
        // Positive control: the same call without the identifier records.
        assert!(call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &desk).is_ok());
    }

    /// **A register with no desk RECORDS NOTHING** — fail-closed, because the alternative is
    /// the state this change removes: an assignment on disk whose obligation never existed,
    /// which `prepare` refuses minutes later with nothing on his screen to explain it.
    #[test]
    fn no_obligation_desk_records_nothing() {
        let fixture = fixture_with_desk(None);
        let desk = Opened::default();
        let refused = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &desk).unwrap_err();
        assert!(refused.contains("Nothing was recorded."), "{refused}");
        assert!(!refused.to_lowercase().contains("started"), "{refused}");
        assert!(fixture.rows().is_empty());
        assert!(desk.opens.lock().unwrap().is_empty());
        // A scope written before this field existed reads back as no desk, rather than failing
        // to parse — which is what makes the refusal above the observable behavior on an
        // older scope file rather than a crash.
        let legacy = fixture.root.join("legacy.json");
        std::fs::write(&legacy, json!({"version":1,"actions_allowed":true,
            "state_root":fixture.state(),"entity_id":"depot","thread_id":"thread-one",
            "instruction_ledger_ref":"ledger:thread-one:turn-7",
            "instruction_sha256":"a".repeat(64)}).to_string()).unwrap();
        let read = read_scope(&legacy).unwrap();
        assert!(read.obligation_desk.is_none());
    }

    /// **AN OBLIGATION THAT WOULD NOT OPEN MEANS NOTHING IS RECORDED**, and an obligation
    /// opened for a registration that then failed is TAKEN BACK.
    #[test]
    fn a_refused_obligation_records_nothing_and_a_failed_record_takes_its_obligation_back() {
        let fixture = fixture();
        let refusing = Opened::refusing("the operational record is unavailable");
        let refused = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &refusing).unwrap_err();
        // The register's own refusal sentence, which claims nothing and starts nothing —
        // `assignment::failed_registration_sentence`, the same one a failed write uses.
        assert!(refused.contains("could not write that assignment down"), "{refused}");
        assert!(refused.contains("Nothing is running and nothing was started"), "{refused}");
        assert!(refused.contains("the operational record is unavailable"), "the reason was swallowed: {refused}");
        assert!(fixture.rows().is_empty());

        // The other half: the obligation opens, and the record cannot be written. The state
        // root is made a FILE, so `register_kind`'s own write fails after the open.
        let blocked = fixture_with_desk(Some(desk()));
        std::fs::write(blocked.state(), "not a directory").unwrap();
        let desk_for_blocked = Opened::default();
        let failed = call_with(&blocked.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &desk_for_blocked).unwrap_err();
        assert!(failed.contains("Nothing is running"), "{failed}");
        let opened = desk_for_blocked.ids();
        assert_eq!(opened.len(), 1);
        assert_eq!(*desk_for_blocked.abandoned.lock().unwrap(), opened,
            "the obligation was left open with nothing to carry it out");
    }

    /// Rich chooses the work kind, which remains factual receipt data.
    #[test]
    fn delegated_questions_keep_richs_kind_without_scripting_his_reply() {
        let fixture = fixture();
        let desk = Opened::default();
        let quick = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"whether the pricing review ever landed","kind":"check"}), &desk).unwrap();
        assert_eq!(quick["kind"], "check");
        let deep = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"why the nightly has been red since Tuesday","kind":"investigate"}), &desk).unwrap();
        assert_eq!(deep["kind"], "investigate");
        assert!(quick.get("say").is_none() && deep.get("say").is_none());
        // The kind is on the record, which is what the timer and the status read use.
        let rows = fixture.rows();
        assert_eq!(rows.len(), 2);
        let kinds: Vec<&str> = rows.iter().map(|r| r.kind.as_str()).collect();
        assert!(kinds.contains(&"check") && kinds.contains(&"investigate"), "{kinds:?}");
        // **AND THE OBLIGATION KNOWS WHICH IT IS.** A question of his is an `open_loop` and a
        // piece of work is a `commitment` — both open in a live state `prepare` accepts, and the
        // difference is what his own executive brief shows him next turn.
        let opens = desk.opens.lock().unwrap().clone();
        assert_eq!(opens.iter().map(|(_, _, kind)| kind.as_str()).collect::<Vec<_>>(), ["check", "investigate"]);
        // An omitted kind is a task — every caller before today, unchanged.
        let task = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the three branches"}), &desk).unwrap();
        assert_eq!(task["kind"], "task");
        // **A kind the model mistyped is REFUSED, not defaulted** — and refused before anything
        // is opened, which is why the kind is parsed ahead of the desk.
        let typo = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"whether the branch landed","kind":"checking"}), &desk).unwrap_err();
        assert!(typo.contains("Nothing was recorded."), "{typo}");
        assert_eq!(fixture.rows().len(), 3, "a refused kind still wrote a record");
        assert_eq!(desk.ids().len(), 3, "a refused kind still opened an obligation");
    }

    /// The model supplies WHAT, never WHERE or WHOSE. Every redirection field is in the
    /// scope, and an argument that tries to carry one is refused outright rather than
    /// ignored — an ignored extra field is how a caller learns it was accepted.
    #[test]
    fn arguments_cannot_redirect_the_company_the_conversation_or_the_instruction() {
        let fixture = fixture();
        let desk = Opened::default();
        for sneaky in [
            json!({"assignment":"x","entity_id":"other"}),
            json!({"assignment":"x","thread_id":"thread-two"}),
            json!({"assignment":"x","state_root":"/tmp"}),
            json!({"assignment":"x","instruction_ledger_ref":"ledger:thread-one:turn-9"}),
            json!({"assignment":"x","obligation_desk":{}}),
        ] {
            assert!(call_with(&fixture.scope, RECORD_TOOL_NAME, sneaky, &desk).is_err());
        }
        assert!(call_with(&fixture.scope, "prepare", json!({"assignment":"x"}), &desk).is_err());
        assert!(call_with(&fixture.scope, RECORD_TOOL_NAME, json!({}), &desk).is_err());
        assert!(call_with(&fixture.scope, RECORD_TOOL_NAME, json!({"assignment":"   "}), &desk).is_err());
        assert!(desk.opens.lock().unwrap().is_empty(), "a refused call opened an obligation");
        // Positive control: the plain form records.
        assert!(call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &desk).is_ok());
    }

    /// With no visible turn the grant is closed and nothing is recorded — spec §5.1. This
    /// is also what stops a WORK lease registering more work for itself: it never gets this
    /// server at all, and if it somehow did, its scope carries no grant.
    #[test]
    fn a_closed_grant_records_nothing_and_says_so_without_implying_a_start() {
        let fixture = fixture();
        let desk = Opened::default();
        set_actions_allowed(&fixture.scope, false).unwrap();
        let refused = call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &desk).unwrap_err();
        assert!(refused.contains("Nothing was recorded."));
        assert!(!refused.to_lowercase().contains("started"));
        assert!(fixture.rows().is_empty());
        // **AND NOTHING WAS OPENED EITHER.** The grant is checked before the obligation, so a
        // conversation that is not open for work leaves no item behind in his continuity.
        assert!(desk.opens.lock().unwrap().is_empty());
        // Positive control: re-open it and the same call records. It also proves the desk
        // survived the grant rewrite, which reads and writes the whole scope.
        set_actions_allowed(&fixture.scope, true).unwrap();
        assert!(call_with(&fixture.scope, RECORD_TOOL_NAME,
            json!({"assignment":"landing the branches"}), &desk).is_ok());
        assert_eq!(desk.ids().len(), 1);
    }

    #[test]
    fn a_missing_or_damaged_scope_records_nothing() {
        let fixture = fixture();
        let desk = Opened::default();
        let absent = fixture.root.join("nothing.json");
        assert!(call_with(&absent, RECORD_TOOL_NAME, json!({"assignment":"x"}), &desk).is_err());
        let damaged = fixture.root.join("damaged.json");
        std::fs::write(&damaged, "{\"version\":").unwrap();
        assert!(call_with(&damaged, RECORD_TOOL_NAME, json!({"assignment":"x"}), &desk).is_err());
        let relative = fixture.root.join("relative.json");
        std::fs::write(&relative, json!({"version":1,"actions_allowed":true,"state_root":"engine-state",
            "entity_id":"depot","thread_id":"thread-one","instruction_ledger_ref":"ledger:thread-one:t",
            "instruction_sha256":"a".repeat(64)}).to_string()).unwrap();
        assert!(call_with(&relative, RECORD_TOOL_NAME, json!({"assignment":"x"}), &desk).is_err());
        assert!(desk.opens.lock().unwrap().is_empty());
    }

    /// The readiness verdict is a fact from the child, not from the config being accepted.
    #[test]
    fn the_tool_is_only_loaded_when_the_child_says_it_is() {
        assert!(loaded_from_init(&json!({"tools":[QUALIFIED_RECORD_TOOL]})));
        assert!(!loaded_from_init(&json!({"tools":["mcp__richos_work__prepare"]})));
        assert!(!loaded_from_init(&json!({})));
        assert!(!loaded_from_init(&json!({"tools":"record"})));
    }

    /// The transport: initialize gates everything, an oversized frame cannot be read as
    /// the command behind it, and a tool call reaches `call`.
    #[test]
    fn the_stdio_transport_gates_on_initialize_and_bounds_a_frame() {
        let fixture = fixture();
        let desk = Opened::default();
        let mut input = String::new();
        input.push_str(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string());
        input.push('\n');
        input.push_str(&json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}).to_string());
        input.push('\n');
        input.push_str(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":RECORD_TOOL_NAME,
            "arguments":{"assignment":"landing the three branches"}}}).to_string());
        input.push('\n');
        // Oversized, then a valid call: the second must not be swallowed by the first.
        input.push_str(&format!("{}\n", "x".repeat(MAX_FRAME_BYTES + 10)));
        input.push_str(&json!({"jsonrpc":"2.0","id":4,"method":"ping"}).to_string());
        input.push('\n');
        let mut out = Vec::new();
        serve_with(&fixture.scope, io::Cursor::new(input.into_bytes()), &mut out, &desk).unwrap();
        let replies: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies[0]["error"]["code"], -32002, "tools/list ran before initialize");
        assert_eq!(replies[1]["result"]["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(replies[2]["result"]["isError"], false);
        assert_eq!(replies[3]["error"]["code"], -32600);
        assert_eq!(replies[4]["id"], 4);
        assert_eq!(fixture.rows().len(), 1);
        assert_eq!(desk.ids().len(), 1, "the transport recorded without opening an obligation");
    }
}
