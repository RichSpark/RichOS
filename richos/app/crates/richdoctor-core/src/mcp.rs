//! **THE SUPPORT TOOL ADAPTER: TWO READ-ONLY OPERATIONS, SERVED BY RICHDOCTOR ITSELF** (PRD §4).
//!
//! The Claude Code CLI starts RichDoctor's own executable as `RichDoctor --support-adapter
//! <draft folder>` ([`crate::adapter::mcp_config`]) and talks MCP to it over standard input and
//! output (newline-delimited JSON-RPC 2.0). It offers exactly two tools:
//!
//!   - `collect_diagnostics(category)`: run one named collector ([`crate::collect`]) for this
//!     draft's incident and answer with its items' summaries and opaque IDs;
//!   - `read_evidence(id)`: answer with one item's sanitized text.
//!
//! No arbitrary paths, shell commands, file writes (beyond the draft's own evidence folder),
//! web fetches, GitHub posting or tool installation. Arguments are validated here, by host
//! code, whatever the CLI already checked: an unknown tool, an extra or missing argument, a
//! category not in the list or an ID not of the evidence shape is refused. Where to read and for
//! which incident come from the host-written `scope.json`, never from the model.
//!
//! The host's cap of [`CALLS_PER_OPERATION`] diagnostic calls per operation is enforced here:
//! the host writes the current operation's number to `operation.json` before each message, and
//! a ninth call in one operation is answered with an error the model is told to draft from.
use crate::adapter::{CALLS_PER_OPERATION, TOOL_COLLECT, TOOL_READ};
use crate::collect::{Category, Scope};
use crate::evidence::{is_id, Store};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// The protocol versions this server speaks, newest first.
const PROTOCOLS: [&str; 3] = ["2025-11-25", "2025-06-18", "2024-11-05"];

/// The two tools' declarations.
pub fn tools() -> Value {
    let categories: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
    json!([
        {
            "name": TOOL_COLLECT,
            "description": "Collect one category of RichOS diagnostic information from this Mac, already cleaned of private details. Returns short summaries and opaque evidence IDs. Categories: installation, startup_crash, conversation_failure, configuration, richscribe, claude_availability.",
            "inputSchema": {"type": "object", "properties": {"category": {"type": "string", "enum": categories}}, "required": ["category"], "additionalProperties": false},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": TOOL_READ,
            "description": "Read one collected, cleaned evidence item by its opaque ID (from collect_diagnostics).",
            "inputSchema": {"type": "object", "properties": {"id": {"type": "string", "pattern": "^ev-[0-9a-f]{12}$"}}, "required": ["id"], "additionalProperties": false},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        }
    ])
}

/// The draft folder's operation counter, written by the host before each message.
pub fn write_operation(draft: &Path, operation: u64) -> std::io::Result<()> {
    crate::util::write_private(&draft.join("operation.json"), json!({ "operation": operation }).to_string().as_bytes())
}

fn current_operation(draft: &Path) -> u64 {
    std::fs::read(draft.join("operation.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v["operation"].as_u64()).unwrap_or(0)
}

/// The scope the host wrote for this draft.
pub fn write_scope(draft: &Path, scope: &Scope) -> std::io::Result<()> {
    crate::util::write_private(&draft.join("scope.json"), &serde_json::to_vec(scope)?)
}

pub fn read_scope(draft: &Path) -> Option<Scope> {
    serde_json::from_slice(&std::fs::read(draft.join("scope.json")).ok()?).ok()
}

/// One server, for one draft.
pub struct Server {
    draft: PathBuf,
    calls: std::collections::HashMap<u64, u32>,
}

/// A tool's answer: its text, and whether it is an error.
fn answer(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

impl Server {
    pub fn new(draft: &Path) -> Server {
        Server { draft: draft.to_path_buf(), calls: Default::default() }
    }

    /// Answer one JSON-RPC message; `None` for a notification.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id")?.clone();
        let method = message["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => {
                let asked = message["params"]["protocolVersion"].as_str().unwrap_or("");
                let version = PROTOCOLS.iter().find(|p| **p == asked).copied().unwrap_or(PROTOCOLS[0]);
                Ok(json!({ "protocolVersion": version, "capabilities": { "tools": {} }, "serverInfo": { "name": crate::adapter::SERVER_NAME, "version": crate::VERSION } }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => Ok(self.call(&message["params"])),
            _ => Err(json!({ "code": -32601, "message": "method not found" })),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
        })
    }

    fn call(&mut self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let Some(args) = args.as_object() else { return answer("The arguments must be an object.".into(), true) };
        let only = |key: &str| args.len() == 1 && args.get(key).and_then(Value::as_str).is_some();
        match name {
            TOOL_COLLECT if only("category") => {
                let Some(category) = Category::parse(args["category"].as_str().unwrap()) else {
                    return answer("That is not one of the diagnostic categories.".into(), true);
                };
                let operation = current_operation(&self.draft);
                let used = self.calls.entry(operation).or_default();
                if *used >= CALLS_PER_OPERATION {
                    return answer(format!("The limit of {CALLS_PER_OPERATION} diagnostic requests for this step is reached. Continue with what has been collected."), true);
                }
                *used += 1;
                let Some(scope) = read_scope(&self.draft) else { return answer("RichDoctor has no incident to collect for.".into(), true) };
                if !scope.diagnostics && category != Category::Installation {
                    return answer("The person chose not to have diagnostic information collected from this Mac. Work from what they describe.".into(), true);
                }
                let store = Store::at(&self.draft);
                let ctx = crate::scrub::Context::gather(&scope.home);
                let used_bytes = store.input_bytes();
                store.clear(category);
                let items = crate::collect::collect(category, &scope, &ctx, used_bytes);
                let mut listing = Vec::new();
                for item in &items {
                    if store.put(item).is_err() {
                        return answer("RichDoctor could not keep what it collected.".into(), true);
                    }
                    listing.push(item.listing());
                }
                answer(json!({ "category": category.as_str(), "items": listing }).to_string(), false)
            }
            TOOL_READ if only("id") => {
                let id = args["id"].as_str().unwrap();
                if !is_id(id) {
                    return answer("That is not an evidence ID.".into(), true);
                }
                match Store::at(&self.draft).get(id) {
                    Some(item) => answer(item.reading().to_string(), false),
                    None => answer("There is no evidence with that ID.".into(), true),
                }
            }
            TOOL_COLLECT | TOOL_READ => answer("Those arguments are not accepted.".into(), true),
            _ => answer("There is no such tool.".into(), true),
        }
    }
}

/// **RUN THE SERVER** until standard input ends. One line is one message; a line that is not
/// JSON is answered with a parse error and the server goes on.
pub fn serve(draft: &Path, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let mut server = Server::new(draft);
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => server.handle(&message),
            Err(_) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } })),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Window;

    fn draft() -> (PathBuf, PathBuf) {
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("rd-mcp-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap())));
        let home = root.join("home");
        let draft = root.join("draft");
        std::fs::create_dir_all(home.join("Library/Logs/RichOS")).unwrap();
        std::fs::write(home.join("Library/Logs/RichOS/startup.log"), "==== 2026-10-22T08:43:10Z RichOS 1.2.1 pid 11 ====\nfailure: the engine did not start\n").unwrap();
        crate::util::private_dir(&draft).unwrap();
        write_scope(&draft, &Scope { home, window: Window::around(1_792_658_590_000), claude_bin: None, diagnostics: true }).unwrap();
        (root, draft)
    }

    fn call(s: &mut Server, name: &str, args: Value) -> Value {
        s.handle(&json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap()["result"].clone()
    }

    #[test]
    fn it_speaks_the_protocol_and_lists_exactly_two_read_only_tools() {
        let (root, draft) = draft();
        let mut s = Server::new(&draft);
        let init = s.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
        assert!(s.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let list = s.handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, [TOOL_COLLECT, TOOL_READ]);
        // The CLI's discovery probe and any other method: method not found, not a crash.
        let probe = s.handle(&json!({"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover","params":{}})).unwrap();
        assert_eq!(probe["error"]["code"], -32601);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collect_then_read_by_opaque_id_and_nothing_else() {
        let (root, draft) = draft();
        let mut s = Server::new(&draft);
        let got = call(&mut s, TOOL_COLLECT, json!({"category":"startup_crash"}));
        assert_eq!(got["isError"], false);
        let listing: Value = serde_json::from_str(got["content"][0]["text"].as_str().unwrap()).unwrap();
        let id = listing["items"][0]["id"].as_str().unwrap().to_string();
        assert!(listing["items"][0].get("text").is_none(), "a listing carries no text");
        let read = call(&mut s, TOOL_READ, json!({"id": id}));
        assert!(read["content"][0]["text"].as_str().unwrap().contains("the engine did not start"));
        for (name, args) in [
            (TOOL_READ, json!({"id":"../scope"})),
            (TOOL_READ, json!({"id":"ev-000000000000"})),
            (TOOL_READ, json!({"path":"/etc/hosts"})),
            (TOOL_COLLECT, json!({"category":"everything"})),
            (TOOL_COLLECT, json!({"category":"startup_crash","path":"/etc"})),
            (TOOL_COLLECT, json!("startup_crash")),
            ("run_command", json!({"command":"ls"})),
        ] {
            assert_eq!(call(&mut s, name, args.clone())["isError"], true, "{name} {args}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn declined_diagnostics_refuse_every_category_but_installation() {
        let (root, draft) = draft();
        let mut scope = read_scope(&draft).unwrap();
        scope.diagnostics = false;
        write_scope(&draft, &scope).unwrap();
        let mut s = Server::new(&draft);
        assert_eq!(call(&mut s, TOOL_COLLECT, json!({"category":"startup_crash"}))["isError"], true);
        assert_eq!(call(&mut s, TOOL_COLLECT, json!({"category":"installation"}))["isError"], false);
        assert!(Store::at(&draft).of(Category::StartupCrash).is_empty(), "nothing was read");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_ninth_diagnostic_call_in_one_operation_is_refused() {
        let (root, draft) = draft();
        let mut s = Server::new(&draft);
        write_operation(&draft, 1).unwrap();
        for _ in 0..CALLS_PER_OPERATION {
            assert_eq!(call(&mut s, TOOL_COLLECT, json!({"category":"installation"}))["isError"], false);
        }
        assert_eq!(call(&mut s, TOOL_COLLECT, json!({"category":"installation"}))["isError"], true);
        write_operation(&draft, 2).unwrap();
        assert_eq!(call(&mut s, TOOL_COLLECT, json!({"category":"installation"}))["isError"], false, "a new operation has its own allowance");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_server_loop_answers_line_by_line_and_survives_garbage() {
        let (root, draft) = draft();
        let input = b"not json\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n".to_vec();
        let mut out = Vec::new();
        serve(&draft, std::io::Cursor::new(input), &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines[0]["error"]["code"], -32700);
        assert_eq!(lines[1]["result"]["tools"].as_array().unwrap().len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
