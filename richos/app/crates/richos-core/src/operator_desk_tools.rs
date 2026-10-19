//! THE FRONT DESK'S OPERATOR TOOLS — `richos_operator.stop`, `.read` and `.interrupt` (operator
//! back-end spec r3 (d), (o), with r4 §1.2; the operator-client record's §7 item 2).
//!
//! **What they are for, in his words.** §67: *"A stop from him, in any wording … is
//! unconditional for the agents his words name, and for no other."* His team runs behind the
//! front desk, so the front desk needs a way to reach it that is not the lead's model: a named
//! stop goes to the named agent in seconds (r3 (d)), "stop what you're doing" ends his team's
//! turn without its agents (his Esc, (d) item 6), and "what is my team doing" is answered from
//! the host's own record ((o)).
//!
//! **Two halves, because the front desk's tools run in a child process.** Every app-owned tool
//! on the front desk is this executable, spawned by `claude` over stdio and reading a scope the
//! app wrote for the turn (`assignment_tools.rs`, `status_tools.rs`). Those tools only read or
//! write records on disk; these must reach the LIVE host, which holds each lead's input. So the
//! app serves a Unix socket ([`DeskSocket`]) and the child ([`run_stdio`]) speaks one JSON line
//! to it per call. The socket lives in the app's own data folder (or, when that path is longer
//! than macOS's 103-character socket limit, under the per-user temporary folder), mode 0600,
//! and every request carries a token the app generated at launch and wrote only into the
//! 0600 scope: a process that cannot read the app's scope cannot use it.
//!
//! **The doctrine lines travel with the tools** (the server's `instructions` and each tool's
//! description), so a front desk that has these tools has their rules. The operator contract
//! and the front desk's addendum (r3 (a)) are Sage's text; these lines are the minimum the
//! tools cannot be used safely without, and the addendum, when it exists, extends them.
use crate::operator_host::{ConversationKey, ConversationRead, StopResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const SERVER_NAME: &str = "richos_operator";
pub const STOP_TOOL: &str = "stop";
pub const READ_TOOL: &str = "read";
pub const INTERRUPT_TOOL: &str = "interrupt";
/// The operator tools whose registration is reported by the native session.
pub const QUALIFIED_TOOLS: [&str; 3] =
    ["mcp__richos_operator__stop", "mcp__richos_operator__read", "mcp__richos_operator__interrupt"];

const MAX_FRAME_BYTES: usize = 64 * 1024;
const MAX_SCOPE_BYTES: u64 = 16 * 1024;
const MAX_NAMES: usize = 20;
const MAX_WORDS: usize = 4000;
/// A named stop waits up to 10 s per attempt and tries twice (r3 (d) item 3), then tells his
/// registry; `stop.sh` itself is bounded at 60 s. The reply may take that long, never longer.
const REPLY_WAIT: Duration = Duration::from_secs(150);
/// macOS's `sun_path` holds 104 bytes including the terminator.
const SOCKET_PATH_LIMIT: usize = 103;

/// The scope the app writes for the front desk's turn. Everything a request could use to
/// redirect itself is here, and none of it is a tool argument.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeskToolScope {
    pub version: u32,
    pub socket: PathBuf,
    /// The app's token for this launch. The scope is written 0600, so only this user's
    /// processes that can read the app's own folder hold it.
    pub token: String,
    pub entity_id: String,
    pub thread_id: String,
    /// `ledger:<thread>:<turn>`: the turn his words were spoken in, for the operator log's
    /// origin. It never gates a stop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger_ref: Option<String>,
    /// The declaration's `origins`: the mouths that may give his team work. The front desk's
    /// register reads them from here (`listed_origins_beside`) and answers work from any
    /// other mouth itself, in his turn.
    pub origins: Vec<String>,
}

/// **The mouths his team takes work from, for the register beside this scope** — `None` when
/// no operator scope is there (every product install: the register is the product's), the
/// listed mouths when it is, and `Err` when it is there and cannot be read (the register then
/// refuses rather than guessing).
pub fn listed_origins_beside(assignments_scope: &Path) -> Result<Option<Vec<String>>, String> {
    let beside = scope_beside(assignments_scope);
    if !beside.exists() {
        return Ok(None);
    }
    read_scope(&beside).map(|scope| Some(scope.origins))
}

/// **What the front desk's lease is given to reach the desk**, on an operator install only
/// (`EngineProfile::operator_desk`; `None` on every product install).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeskAccess {
    pub socket: PathBuf,
    pub token: String,
    /// The declaration's `origins`: the mouths that may give his team work. The front desk's
    /// register answers work from any other mouth itself (`assignment_tools.rs`).
    pub origins: Vec<String>,
}

/// The per-lease scope file for these tools: beside the register's, from the same per-lease
/// identity (`native.rs` names every scope `<identity>-<purpose>.json`), so the lease needs no
/// second name to remember.
pub fn scope_beside(assignments_scope: &Path) -> PathBuf {
    let name = assignments_scope.file_name().and_then(|n| n.to_str()).unwrap_or("front-desk");
    let stem = name.strip_suffix("-assignments.json").or_else(|| name.strip_suffix(".json")).unwrap_or(name);
    assignments_scope.with_file_name(format!("{stem}-operator.json"))
}

/// Write the scope 0600, whole or not at all.
pub fn write_scope(path: &Path, scope: &DeskToolScope) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    if scope.version != 1 || !scope.socket.is_absolute() || scope.token.len() < 16 {
        return Err("The app has not supplied a usable operator scope.".into());
    }
    let text = serde_json::to_string(scope).map_err(|e| e.to_string())?;
    let dir = path.parent().ok_or("the operator scope has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{}.{}", path.file_name().and_then(|n| n.to_str()).unwrap_or("scope"), uuid::Uuid::new_v4().simple()));
    let written = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)
        .and_then(|mut file| file.write_all(text.as_bytes()).and_then(|_| file.sync_all()))
        .and_then(|_| std::fs::rename(&tmp, path));
    if let Err(e) = written {
        if let Err(cleanup) = std::fs::remove_file(&tmp) {
            if cleanup.kind() != io::ErrorKind::NotFound {
                return Err(format!("{e}; and the partial scope {} could not be removed ({cleanup})", tmp.display()));
            }
        }
        return Err(e.to_string());
    }
    Ok(())
}

fn read_scope(path: &Path) -> Result<DeskToolScope, String> {
    let file = std::fs::File::open(path).map_err(|_| "RichOS has not opened your team to this conversation yet.".to_string())?;
    let mut text = String::new();
    file.take(MAX_SCOPE_BYTES + 1).read_to_string(&mut text).map_err(|_| "The operator scope could not be read.".to_string())?;
    if text.len() as u64 > MAX_SCOPE_BYTES {
        return Err("The operator scope is too large.".into());
    }
    let scope: DeskToolScope = serde_json::from_str(&text).map_err(|_| "The operator scope is unreadable.".to_string())?;
    if scope.version != 1 || !scope.socket.is_absolute() {
        return Err("The operator scope is not usable.".into());
    }
    Ok(scope)
}

/// A fresh token for one launch: 64 hex characters from two v4 UUIDs.
pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Where the desk's socket goes for this operator folder: `<root>/desk.sock` when that fits the
/// socket limit, otherwise a name under the per-user temporary folder made unique by the
/// folder's digest. Measured: his nightly folder's `<data>/operator/desk.sock` is 105 bytes.
pub fn socket_path(operator_root: &Path) -> PathBuf {
    let wanted = operator_root.join("desk.sock");
    if wanted.as_os_str().len() <= SOCKET_PATH_LIMIT {
        return wanted;
    }
    use sha2::Digest;
    let digest = format!("{:x}", sha2::Sha256::digest(operator_root.as_os_str().as_encoded_bytes()));
    std::env::temp_dir().join(format!("richos-desk-{}.sock", &digest[..16]))
}

// =============================================================================================
// the app's half: the socket and what it answers
// =============================================================================================

/// What the live desk answers. [`crate::operator_desk::OperatorDesk`] in the app; a fake in
/// the tests.
pub trait DeskService: Send + Sync {
    fn stop(&self, names: &[String], words: &str, ledger_ref: Option<&str>) -> Vec<StopResult>;
    fn read(&self, key: &ConversationKey, every: bool) -> Vec<ConversationRead>;
    fn interrupt(&self, key: &ConversationKey) -> String;
}

impl DeskService for crate::operator_desk::OperatorDesk {
    fn stop(&self, names: &[String], words: &str, ledger_ref: Option<&str>) -> Vec<StopResult> {
        self.stop_named(names, words, ledger_ref)
    }
    fn read(&self, key: &ConversationKey, every: bool) -> Vec<ConversationRead> {
        crate::operator_desk::OperatorDesk::read(self, key, every)
    }
    fn interrupt(&self, key: &ConversationKey) -> String {
        crate::operator_desk::OperatorDesk::interrupt(self, key)
    }
}

/// The socket the front desk's tools reach the desk through. Closed (and its file removed) on
/// drop or [`DeskSocket::close`].
pub struct DeskSocket {
    path: PathBuf,
    token: String,
    closing: Arc<AtomicBool>,
}

impl DeskSocket {
    /// Serve `service` at `path`, admitting only requests that carry `token`.
    pub fn serve(service: Arc<dyn DeskService>, path: &Path, token: &str) -> io::Result<DeskSocket> {
        Self::serve_with_lead(service, path, token, None)
    }

    /// Serve `service` at `path` to the front desk (`token`: every call) and, with `lead_token`,
    /// to his team's leads, who may make exactly one call: a named stop (F6, the lead-side stop
    /// of the operator contract notes §4 item 3). A lead holds the lead token in its own report
    /// scope, never the desk's, so it can stop an agent and can neither read another
    /// conversation nor interrupt one.
    pub fn serve_with_lead(service: Arc<dyn DeskService>, path: &Path, token: &str, lead_token: Option<&str>)
                           -> io::Result<DeskSocket> {
        use std::os::unix::fs::PermissionsExt;
        if token.len() < 16 || lead_token.is_some_and(|t| t.len() < 16) {
            return Err(io::Error::other("the desk's token is too short"));
        }
        if lead_token.is_some_and(|t| same_token(t, token)) {
            return Err(io::Error::other("the leads' token must not be the desk's own"));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // A socket file left by a launch that did not get to remove it: this app owns the
        // path, and the token is new, so the old file can serve nobody.
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() => return Err(io::Error::other("the desk's socket path is a folder")),
            Ok(_) => std::fs::remove_file(path)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let closing = Arc::new(AtomicBool::new(false));
        let (flag, expected, lead) = (closing.clone(), token.to_string(), lead_token.map(str::to_string));
        std::thread::Builder::new().name("richos-operator-socket".into()).spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let (service, expected, lead) = (service.clone(), expected.clone(), lead.clone());
                // One thread per call: a named stop may take most of a minute, and a read asked
                // meanwhile must not wait behind it.
                if let Err(e) = std::thread::Builder::new().name("richos-operator-call".into())
                    .spawn(move || answer(service.as_ref(), &expected, lead.as_deref(), stream)) {
                    eprintln!("operator desk: a call could not be served ({e})");
                }
            }
        })?;
        Ok(DeskSocket { path: path.to_path_buf(), token: token.to_string(), closing })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Stop accepting and remove the socket file.
    pub fn close(&self) {
        if self.closing.swap(true, Ordering::SeqCst) {
            return;
        }
        // Wakes the accept loop so it sees the flag; failing means it is already gone.
        drop(UnixStream::connect(&self.path));
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != io::ErrorKind::NotFound {
                eprintln!("operator desk: the socket {} could not be removed ({e})", self.path.display());
            }
        }
    }
}

impl Drop for DeskSocket {
    fn drop(&mut self) {
        self.close();
    }
}

fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len() && given.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn answer(service: &dyn DeskService, token: &str, lead_token: Option<&str>, stream: UnixStream) {
    let reply = match read_line(&stream) {
        Ok(line) => match serde_json::from_str::<Value>(&line) {
            Ok(request) => dispatch(service, token, lead_token, &request),
            Err(_) => json!({"ok": false, "say": "The request was not readable."}),
        },
        Err(e) => json!({"ok": false, "say": format!("The request could not be read ({e}).")}),
    };
    let mut stream = stream;
    if let Err(e) = writeln!(stream, "{reply}").and_then(|_| stream.flush()) {
        eprintln!("operator desk: a reply could not be written ({e})");
    }
}

fn read_line(stream: &UnixStream) -> io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_FRAME_BYTES as u64 + 1)).read_line(&mut line)?;
    if line.len() > MAX_FRAME_BYTES {
        return Err(io::Error::other("the request is too large"));
    }
    Ok(line)
}

fn dispatch(service: &dyn DeskService, token: &str, lead_token: Option<&str>, request: &Value) -> Value {
    let given = request["token"].as_str().unwrap_or("");
    let desk = same_token(given, token);
    if !desk && !lead_token.is_some_and(|lead| same_token(given, lead)) {
        return json!({"ok": false, "say": "This request did not come from RichOS."});
    }
    if !desk && request["op"].as_str() != Some("stop") {
        return json!({"ok": false, "say": "A lead can only stop named agents through the desk."});
    }
    let key = ConversationKey { entity_id: request["entity_id"].as_str().unwrap_or("").to_string(),
                                thread_id: request["thread_id"].as_str().unwrap_or("").to_string() };
    match request["op"].as_str() {
        Some("stop") => {
            let names: Vec<String> = request["names"].as_array().map(|a| a.iter().filter_map(Value::as_str)
                .map(str::to_string).collect()).unwrap_or_default();
            let results = service.stop(&names, request["words"].as_str().unwrap_or(""), request["ledger_ref"].as_str());
            json!({"ok": true, "say": results.iter().map(StopResult::sentence).collect::<Vec<_>>().join(" "),
                   "stopped": results.iter().filter_map(|r| match r { StopResult::Stopped { name, .. } => Some(name), _ => None })
                       .collect::<Vec<_>>()})
        }
        Some("read") => {
            let reads = service.read(&key, request["every"].as_bool().unwrap_or(false));
            json!({"ok": true, "conversations": reads.iter().map(|r| json!({
                "thread_id": r.key.thread_id, "team_running": r.lead_running,
                "agents": r.agents.iter().map(|(name, status)| json!({"name": name, "status": status})).collect::<Vec<_>>(),
                "last_words": r.texts, "open_questions": r.open_questions, "land_leases": r.leases})).collect::<Vec<_>>()})
        }
        Some("interrupt") => json!({"ok": true, "say": service.interrupt(&key)}),
        _ => json!({"ok": false, "say": "That is not something your team's desk does."}),
    }
}

// =============================================================================================
// the child's half: the stdio server the front desk calls
// =============================================================================================

fn tools() -> Value {
    json!({"tools": [
        {"name": STOP_TOOL,
         "description": "Stop named agents of his team and nothing else. Call it at once when he names agents to stop, \
from any channel. `names`: the agents exactly as he named them. `words`: his words, verbatim. It returns what was \
measured: say an agent stopped only when this says it did.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["names", "words"],
             "properties": {"names": {"type": "array", "minItems": 1, "maxItems": MAX_NAMES, "items": {"type": "string", "minLength": 1}},
                            "words": {"type": "string", "minLength": 1}}}},
        {"name": INTERRUPT_TOOL,
         "description": "Stop what his team is doing in this conversation right now: its current turn ends, its agents \
keep running and any messages of his still queued run next. Use it when he asks his team itself to stop, not named agents.",
         "inputSchema": {"type": "object", "additionalProperties": false, "properties": {}}},
        {"name": READ_TOOL,
         "description": "What his team is doing: whether it is running here, its named agents and their status, its last \
words, the questions it is still waiting on (asked and no answer relayed yet) and land leases it holds. `every`: true for every conversation, not only this one.",
         "inputSchema": {"type": "object", "additionalProperties": false,
             "properties": {"every": {"type": "boolean"}}}}
    ]})
}

/// One call to the desk: a JSON line out, a JSON line back.
fn call_desk(scope: &DeskToolScope, op: &str, fields: Value) -> Result<Value, String> {
    let mut request = json!({"token": scope.token, "op": op, "entity_id": scope.entity_id, "thread_id": scope.thread_id,
                             "ledger_ref": scope.ledger_ref});
    if let (Some(target), Some(extra)) = (request.as_object_mut(), fields.as_object()) {
        target.extend(extra.clone());
    }
    call_socket(&scope.socket, &request)
}

/// **A named stop from one of his team's leads** (F6): the same stop the front desk's tool
/// makes, through the same socket, with the lead token its report scope carries. The desk's
/// `stop_named` finds each agent in whichever conversation runs it (`owner_of`), so a stop that
/// reached the wrong lead as words is still executed once.
pub fn stop_through_desk(socket: &Path, lead_token: &str, names: &[String], words: &str) -> Result<Value, String> {
    call_socket(socket, &json!({"token": lead_token, "op": "stop", "names": names, "words": words}))
}

/// The names and his words of a stop call, checked the same way for the front desk and a lead.
pub(crate) fn stop_arguments(args: &Value) -> Result<(Vec<String>, String), String> {
    let names: Vec<String> = args["names"].as_array().ok_or("Name the agents to stop.")?
        .iter().filter_map(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).collect();
    if names.is_empty() || names.len() > MAX_NAMES {
        return Err(format!("Name between 1 and {MAX_NAMES} agents to stop."));
    }
    let words = args["words"].as_str().map(str::trim).filter(|w| !w.is_empty())
        .ok_or("Pass his words, verbatim.")?;
    if words.len() > MAX_WORDS {
        return Err("His words are too long to record; pass the sentence that asks for the stop.".into());
    }
    Ok((names, words.to_string()))
}

fn call_socket(socket: &Path, request: &Value) -> Result<Value, String> {
    let unreachable = |e: io::Error| format!("RichOS did not answer for your team ({e}). Nothing was changed.");
    let mut stream = UnixStream::connect(socket).map_err(unreachable)?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(unreachable)?;
    stream.set_read_timeout(Some(REPLY_WAIT)).map_err(unreachable)?;
    writeln!(stream, "{request}").map_err(unreachable)?;
    let mut line = String::new();
    BufReader::new(stream.take(4 * 1024 * 1024)).read_line(&mut line).map_err(unreachable)?;
    let reply: Value = serde_json::from_str(&line).map_err(|_| "RichOS's answer for your team was not readable.".to_string())?;
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(reply["say"].as_str().unwrap_or("RichOS refused that for your team.").to_string())
    }
}

fn call(scope_path: &Path, name: &str, args: Value) -> Result<Value, String> {
    let scope = read_scope(scope_path)?;
    match name {
        STOP_TOOL => {
            let (names, words) = stop_arguments(&args)?;
            call_desk(&scope, "stop", json!({"names": names, "words": words}))
        }
        INTERRUPT_TOOL => call_desk(&scope, "interrupt", json!({})),
        READ_TOOL => call_desk(&scope, "read", json!({"every": args["every"].as_bool().unwrap_or(false)})),
        other => Err(format!("{other} is not one of your team's tools.")),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn response(scope: &Path, request: Value, initialized: &mut bool) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
        return Some(error(id.unwrap_or(Value::Null), -32600, "Invalid JSON-RPC request"));
    }
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
            json!({"protocolVersion": protocol, "capabilities": {"tools": {}},
                   "serverInfo": {"name": SERVER_NAME, "version": "1.0.0"}})
        }
        "ping" => json!({}),
        _ if !*initialized => return Some(error(id, -32002, "Initialize before calling your team's tools")),
        "tools/list" => tools(),
        "tools/call" => {
            let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "A tool name is required"));
            };
            let args = request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
            match call(scope, name, args) {
                Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}],"isError":false}),
                Err(why) => json!({"content":[{"type":"text","text":why}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

/// Newline-delimited JSON-RPC with bounded frames, as the app's other stdio servers.
pub fn serve(scope_path: &Path, mut reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
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
                Ok(value) => response(scope_path, value, &mut initialized),
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

/// The stdio entry point (`--operator-desk-mcp <scope>`). Stdout is exclusively MCP frames.
pub fn run_stdio(scope_path: &Path) -> io::Result<()> {
    serve(scope_path, io::stdin().lock(), io::stdout().lock())
}

/// Did the front desk's `claude` actually load the stop tool? From its own init inventory.
pub fn loaded_from_init(init: &Value) -> bool {
    init.get("tools").and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|tool| tool.as_str() == Some(QUALIFIED_TOOLS[0])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// One stop the fake desk was asked for: the names, his words, the turn's reference.
    type AskedStop = (Vec<String>, String, Option<String>);

    #[derive(Default)]
    struct FakeDesk {
        stops: Mutex<Vec<AskedStop>>,
        interrupts: Mutex<Vec<ConversationKey>>,
    }
    impl DeskService for FakeDesk {
        fn stop(&self, names: &[String], words: &str, ledger_ref: Option<&str>) -> Vec<StopResult> {
            self.stops.lock().unwrap().push((names.to_vec(), words.to_string(), ledger_ref.map(str::to_string)));
            names.iter().map(|n| if n == "gone" { StopResult::NotFound { name: n.clone() } }
                             else { StopResult::Stopped { name: n.clone(), seconds: 1.2, registry: Ok(()) } }).collect()
        }
        fn read(&self, key: &ConversationKey, every: bool) -> Vec<ConversationRead> {
            vec![ConversationRead { key: key.clone(), lead_running: every, agents: vec![("mark-sonnet-a".into(), "running".into())],
                                    texts: vec!["Landed.".into()], open_questions: vec![], leases: vec![] }]
        }
        fn interrupt(&self, key: &ConversationKey) -> String {
            self.interrupts.lock().unwrap().push(key.clone());
            "Stopped what your team was doing here. Its agents keep running.".into()
        }
    }

    struct Fixture {
        root: PathBuf,
        scope: PathBuf,
        desk: Arc<FakeDesk>,
        _socket: DeskSocket,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }

    /// A short root: a socket path must fit macOS's 103 characters.
    fn fixture() -> Fixture {
        let root = PathBuf::from("/tmp").join(format!("rod-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]));
        std::fs::create_dir_all(&root).unwrap();
        let desk = Arc::new(FakeDesk::default());
        let token = uuid::Uuid::new_v4().simple().to_string();
        let socket = DeskSocket::serve(desk.clone(), &root.join("desk.sock"), &token).unwrap();
        let scope = root.join("scope.json");
        write_scope(&scope, &DeskToolScope { version: 1, socket: socket.path().to_path_buf(), token, entity_id: "femcboost".into(),
                                             thread_id: "t-1".into(), ledger_ref: Some("ledger:t-1:turn-9".into()),
                                             origins: vec!["desk-typed".into()] }).unwrap();
        Fixture { root, scope, desk, _socket: socket }
    }

    fn rpc(scope: &Path, frames: &[Value]) -> Vec<Value> {
        let input: String = frames.iter().map(|f| f.to_string() + "\n").collect();
        let mut out = Vec::new();
        serve(scope, input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    fn init() -> Value {
        json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})
    }

    fn tool(id: u64, name: &str, args: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
    }

    fn text(reply: &Value) -> Value {
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap_or_else(|_| Value::String(reply["result"]["content"][0]["text"].as_str().unwrap().to_string()))
    }

    /// **A named stop from the front desk reaches the live desk with his words and the turn they
    /// were spoken in, and he hears only what was measured** (r3 (d); §67).
    #[test]
    fn a_named_stop_reaches_the_live_desk_with_his_words_and_says_what_was_measured() {
        let f = fixture();
        let replies = rpc(&f.scope, &[init(), tool(1, STOP_TOOL, json!({"names": ["mark-sonnet-a", "gone"],
                                                                         "words": "stop mark and gone"}))]);
        assert!(replies[0]["result"].get("instructions").is_none());
        let said = text(&replies[1]);
        assert_eq!(said["say"], "Stopped mark-sonnet-a. gone isn't one of your team's agents in this app.");
        assert_eq!(said["stopped"], json!(["mark-sonnet-a"]));
        let stops = f.desk.stops.lock().unwrap().clone();
        assert_eq!(stops, [(vec!["mark-sonnet-a".to_string(), "gone".to_string()], "stop mark and gone".to_string(),
                            Some("ledger:t-1:turn-9".to_string()))]);
    }

    #[test]
    fn interrupt_and_read_answer_for_this_conversation() {
        let f = fixture();
        let replies = rpc(&f.scope, &[init(), tool(1, INTERRUPT_TOOL, json!({})), tool(2, READ_TOOL, json!({"every": true}))]);
        assert_eq!(text(&replies[1])["say"], "Stopped what your team was doing here. Its agents keep running.");
        assert_eq!(f.desk.interrupts.lock().unwrap()[0], ConversationKey { entity_id: "femcboost".into(), thread_id: "t-1".into() });
        let read = text(&replies[2]);
        assert_eq!(read["conversations"][0]["agents"][0]["name"], "mark-sonnet-a");
        assert_eq!(read["conversations"][0]["team_running"], true, "every was passed through");
    }

    /// **The socket answers only the app's own scope**: a request without this launch's token
    /// changes nothing, and neither does a stop with no names or no words.
    #[test]
    fn a_request_without_the_launch_s_token_or_without_names_and_words_changes_nothing() {
        let f = fixture();
        let mut forged = read_scope(&f.scope).unwrap();
        forged.token = "0".repeat(32);
        let forged_path = f.root.join("forged.json");
        write_scope(&forged_path, &forged).unwrap();
        let replies = rpc(&forged_path, &[init(), tool(1, STOP_TOOL, json!({"names": ["mark-sonnet-a"], "words": "stop"}))]);
        assert_eq!(replies[1]["result"]["isError"], true);
        assert_eq!(text(&replies[1]), "This request did not come from RichOS.");
        let replies = rpc(&f.scope, &[init(), tool(1, STOP_TOOL, json!({"names": [], "words": "stop"})),
                                      tool(2, STOP_TOOL, json!({"names": ["a"], "words": "  "}))]);
        assert_eq!(replies[1]["result"]["isError"], true);
        assert_eq!(replies[2]["result"]["isError"], true);
        assert!(f.desk.stops.lock().unwrap().is_empty(), "nothing reached the desk");
    }

    /// **F6: the leads' token stops named agents and does nothing else.** A lead holds it in its
    /// report scope; it can neither read another conversation nor interrupt one, and the desk's
    /// own token still does everything.
    #[test]
    fn the_leads_token_stops_named_agents_and_does_nothing_else() {
        let root = PathBuf::from("/tmp").join(format!("rol-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]));
        std::fs::create_dir_all(&root).unwrap();
        let desk = Arc::new(FakeDesk::default());
        let (token, lead) = (new_token(), new_token());
        assert!(DeskSocket::serve_with_lead(desk.clone(), &root.join("same.sock"), &token, Some(&token)).is_err(),
                "the leads' token is never the desk's own");
        let socket = DeskSocket::serve_with_lead(desk.clone(), &root.join("desk.sock"), &token, Some(&lead)).unwrap();
        let stopped = stop_through_desk(socket.path(), &lead, &["mark-sonnet-a".into()], "stop mark").unwrap();
        assert_eq!(stopped["stopped"], json!(["mark-sonnet-a"]));
        assert_eq!(desk.stops.lock().unwrap().clone(), [(vec!["mark-sonnet-a".to_string()], "stop mark".to_string(), None)]);
        for op in ["read", "interrupt"] {
            let refused = call_socket(socket.path(), &json!({"token": lead, "op": op, "entity_id": "femcboost", "thread_id": "t-1"}));
            assert_eq!(refused.unwrap_err(), "A lead can only stop named agents through the desk.", "{op}");
        }
        assert!(desk.interrupts.lock().unwrap().is_empty());
        let read = call_socket(socket.path(), &json!({"token": token, "op": "read", "entity_id": "femcboost", "thread_id": "t-1"}));
        assert_eq!(read.unwrap()["ok"], true, "the desk's own token still reads");
        socket.close();
        if let Err(e) = std::fs::remove_dir_all(&root) { eprintln!("fixture cleanup: {e}"); }
    }

    #[test]
    fn the_scope_and_the_socket_are_this_user_s_alone() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        assert_eq!(std::fs::metadata(&f.scope).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(f.root.join("desk.sock")).unwrap().permissions().mode() & 0o777, 0o600);
    }

    /// Measured: his nightly folder's `<data>/operator/desk.sock` is 105 bytes, over the limit.
    #[test]
    fn a_socket_path_too_long_for_macos_moves_to_the_temporary_folder_and_stays_unique() {
        let short = PathBuf::from("/tmp/x/operator");
        assert_eq!(socket_path(&short), short.join("desk.sock"));
        let nightly = PathBuf::from("/Users/alex/myrichos-nightly-a/home.noindex/Library/Application Support/com.richos.app/operator");
        assert_eq!(nightly.join("desk.sock").as_os_str().len(), 105);
        let moved = socket_path(&nightly);
        assert!(moved.as_os_str().len() <= SOCKET_PATH_LIMIT, "{}", moved.display());
        assert!(moved.starts_with(std::env::temp_dir()));
        assert_ne!(moved, socket_path(&nightly.join("other")), "two folders never share a socket");
    }

    #[test]
    fn a_desk_that_is_not_there_is_said_and_nothing_else_happens() {
        let f = fixture();
        let mut scope = read_scope(&f.scope).unwrap();
        scope.socket = f.root.join("absent.sock");
        let absent = f.root.join("absent.json");
        write_scope(&absent, &scope).unwrap();
        let replies = rpc(&absent, &[init(), tool(1, READ_TOOL, json!({}))]);
        assert_eq!(replies[1]["result"]["isError"], true);
        assert!(text(&replies[1]).as_str().unwrap().starts_with("RichOS did not answer for your team"));
    }

    #[test]
    fn the_scope_sits_beside_the_register_s_from_the_same_identity() {
        assert_eq!(scope_beside(Path::new("/s/scopes/abc-assignments.json")), PathBuf::from("/s/scopes/abc-operator.json"));
        assert_eq!(scope_beside(Path::new("/s/assignments.json")), PathBuf::from("/s/assignments-operator.json"));
    }

    /// **F16 of Frank's review:** the read lists only questions with no answer relayed yet
    /// (`operator_host.rs` `read_one`), so its description says so, as the addendum does.
    #[test]
    fn the_read_tool_says_it_lists_the_questions_still_waiting() {
        let listed = tools();
        let read = listed["tools"].as_array().unwrap().iter().find(|t| t["name"] == READ_TOOL).unwrap();
        let description = read["description"].as_str().unwrap();
        assert!(description.contains("the questions it is still waiting on"), "{description}");
        assert!(!description.contains("questions it asked him"), "{description}");
    }

    #[test]
    fn the_tools_are_listed_and_the_stop_tool_is_seen_in_an_init_inventory() {
        let f = fixture();
        let replies = rpc(&f.scope, &[init(), json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})]);
        let names: Vec<&str> = replies[1]["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, [STOP_TOOL, INTERRUPT_TOOL, READ_TOOL]);
        assert!(loaded_from_init(&json!({"tools": ["Bash", "mcp__richos_operator__stop"]})));
        assert!(!loaded_from_init(&json!({"tools": ["Bash"]})));
    }
}
