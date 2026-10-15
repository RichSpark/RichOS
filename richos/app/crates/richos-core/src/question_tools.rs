//! Scoped MCP facade for app-owned questions. Tool calls never wait for an answer.
use crate::questions::{AnswerRequest, AskScope, QuestionInput, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};
pub const SERVER: &str = "richos_questions";
pub const ASK: &str = "mcp__richos_questions__ask";
#[derive(Clone, Serialize, Deserialize)]
pub struct Scope {
    pub context: AskScope,
    pub actions_allowed: bool,
    pub answer_method: String,
    pub surface: String,
}
pub fn path(assignments: &Path) -> PathBuf {
    assignments.with_extension("questions.json")
}
pub fn write_scope(path: &Path, scope: &Scope) -> Result<(), String> {
    crate::doctrine::write_verified(
        path,
        &serde_json::to_string(scope).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub fn read_scope(path: &Path) -> Result<Scope, String> {
    let bytes = std::fs::read(path).map_err(|_| "This turn is not open for questions.")?;
    if bytes.len() > 32768 {
        return Err("Invalid question scope.".into());
    }
    let scope: Scope = serde_json::from_slice(&bytes).map_err(|_| "Invalid question scope.")?;
    if !scope.context.root.is_absolute() {
        return Err("Invalid question store.".into());
    }
    Ok(scope)
}
pub fn set_actions_allowed(path: &Path, allowed: bool) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let mut scope = read_scope(path)?;
    scope.actions_allowed = allowed;
    write_scope(path, &scope)
}
type Fingerprint = (std::time::SystemTime, u64, u64);
type AskedKey = (PathBuf, String, String, String);
/// The last answer per asking turn, with the history fingerprint it was computed from.
static ASKED: std::sync::Mutex<Vec<(AskedKey, Fingerprint, bool)>> = std::sync::Mutex::new(Vec::new());

/// Whether the front desk's turn has raised a question. The native turn loop polls this every
/// 40 ms, so the question history is reloaded only when `store.json` has actually changed
/// (its fingerprint differs); an unchanged file answers from the last result. The
/// fingerprint is taken BEFORE the load, so a write racing the load leaves a stale
/// fingerprint and the next poll reloads.
pub fn has_asked(path: &Path) -> bool {
    let Some(s) = read_scope(path).ok().filter(|s| s.context.asker == "front_desk") else {
        return false;
    };
    let store = Store::new(&s.context.root);
    let key: AskedKey = (
        s.context.root.clone(),
        s.context.entity_id.clone(),
        s.context.thread_id.clone(),
        s.context.turn_id.clone(),
    );
    let fingerprint = store.fingerprint();
    if let Some(fp) = fingerprint {
        let cache = ASKED.lock().unwrap();
        if let Some((_, _, asked)) = cache.iter().find(|(k, f, _)| *k == key && *f == fp) {
            return *asked;
        }
    }
    let asked = store
        .list(&s.context.entity_id, &s.context.thread_id)
        .is_ok_and(|qs| qs.iter().any(|q| q.turn_id == s.context.turn_id));
    if let Some(fp) = fingerprint {
        let mut cache = ASKED.lock().unwrap();
        cache.retain(|(k, _, _)| *k != key);
        if cache.len() >= 64 {
            cache.remove(0);
        }
        cache.push((key, fp, asked));
    }
    asked
}
pub fn tools() -> Value {
    let option = json!({"type":"object","properties":{"label":{"type":"string"},"description":{"type":"string"}},"required":["label","description"],"additionalProperties":false});
    let question = json!({"type":"object","properties":{"text":{"type":"string"},"options":{"type":"array","minItems":2,"maxItems":4,"items":option},"multiple":{"type":"boolean"},"free_answer":{"type":"boolean"},"recommended":{"type":"integer","minimum":0,"maximum":3}},"required":["text","options"],"additionalProperties":false});
    json!({"tools":[
        {"name":"ask","description":"Ask a real unresolved choice with two to four spoken-safe options. One question unless several are necessary. Returns immediately. The front desk ends its turn; backend work independent of the answer continues. Never use AskUserQuestion.","inputSchema":{"type":"object","properties":{"questions":{"type":"array","minItems":1,"maxItems":4,"items":question}},"required":["questions"],"additionalProperties":false}},
        {"name":"withdraw","description":"Withdraw one of your own open questions with a readable reason when it no longer matters.","inputSchema":{"type":"object","properties":{"question_id":{"type":"string"},"reason":{"type":"string"}},"required":["question_id","reason"],"additionalProperties":false}},
        {"name":"answer","description":"Front desk only: record the user's typed or spoken answer to an open question. Resolve only an unmistakable choice; ask back when ambiguous. Use app option ids. The complete resolved set is returned, including earlier taps. Do not invent an answer.","inputSchema":{"type":"object","properties":{"question_id":{"type":"string"},"client_id":{"type":"string"},"option_ids":{"type":"array","items":{"type":"string"}},"text":{"type":"string"},"expected_revision":{"type":"integer"}},"required":["question_id","client_id"],"additionalProperties":false}}
    ]})
}
pub fn call(path: &Path, name: &str, args: Value) -> Result<Value, String> {
    let scope = read_scope(path)?;
    if !scope.actions_allowed {
        return Err("Questions are closed during internal preparation or after this turn.".into());
    }
    let s = &scope.context;
    let store = Store::new(&s.root);
    let keys = args
        .as_object()
        .ok_or("Tool arguments must be an object.")?;
    let allowed: &[&str] = match name {
        "ask" => &["questions"],
        "withdraw" => &["question_id", "reason"],
        "answer" => &[
            "question_id",
            "client_id",
            "option_ids",
            "text",
            "expected_revision",
        ],
        _ => return Err("Unknown question tool.".into()),
    };
    if keys.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unknown question argument. Scope is supplied by the app.".into());
    }
    match name {
        "ask" => {
            let qs: Vec<QuestionInput> =
                serde_json::from_value(args.get("questions").cloned().ok_or("Supply questions.")?)
                    .map_err(|e| e.to_string())?;
            let questions = store.ask(s, qs)?;
            Ok(
                json!({"recorded":true,"questions":questions,"answer_delivery":"later_turn"}),
            )
        }
        "withdraw" => {
            store.withdraw(
                &s.entity_id,
                &s.thread_id,
                &s.asker,
                args["question_id"].as_str().ok_or("Supply question_id.")?,
                args["reason"].as_str().ok_or("Supply a reason.")?,
            )?;
            Ok(json!({"withdrawn":true}))
        }
        "answer" => {
            if s.asker != "front_desk" {
                return Err(
                    "Only this conversation's front desk may resolve the user's words.".into(),
                );
            }
            let request: AnswerRequest = serde_json::from_value(args).map_err(|e| e.to_string())?;
            let result = store.answer(
                &s.entity_id,
                &s.thread_id,
                request,
                &scope.answer_method,
                &scope.surface,
            )?;
            // Limit in-turn delivery to the resolved question's set. Other queued sets
            // and late answers still enter the durable conversation intake.
            let mut complete = Vec::new();
            if result.outcome == "accepted" || result.outcome == "already_answered" {
                if let Some(q) = &result.question {
                    store.deliver_set(
                        &s.entity_id,
                        &s.thread_id,
                        "front_desk",
                        &q.set_id,
                        |d| crate::question_work::tool_result(&s.root, &s.turn_id, d),
                    )?;
                    if let Some(text) =
                        crate::question_work::read_tool_result(&s.root, &s.turn_id, &q.set_id)?
                    {
                        complete.push(text);
                    }
                }
            }
            Ok(json!({"answer":result,"complete_sets":complete}))
        }
        _ => Err("Unknown question tool.".into()),
    }
}
pub fn convert_vendor(path: &Path, input: &Value) -> Result<Value, String> {
    let scope = read_scope(path)?;
    if !scope.actions_allowed {
        return Err("Questions are closed outside a user-facing turn.".into());
    }
    convert_vendor_scoped(&scope.context, input)
}
/// Shared bounded fallback for runtime-owned front-desk and operator scopes.
pub fn convert_vendor_scoped(context: &AskScope, input: &Value) -> Result<Value, String> {
    let questions = input["questions"].as_array().ok_or("Supply questions.")?;
    let mapped:Vec<_>=questions.iter().map(|q|json!({"text":q["question"],"options":q["options"],"multiple":q["multiSelect"].as_bool().unwrap_or(false)})).collect();
    let inputs = serde_json::from_value(json!(mapped)).map_err(|e| e.to_string())?;
    let questions = Store::new(&context.root).ask_bounded(
        context,
        inputs,
        std::time::Duration::from_millis(800),
    )?;
    Ok(
        json!({"recorded":true,"questions":questions,"answer_delivery":"later_turn"}),
    )
}
pub fn run_stdio(path: &Path) -> io::Result<()> {
    serve(path, io::stdin().lock(), io::stdout().lock())
}
pub fn serve(path: &Path, mut reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    let mut initialized = false;
    loop {
        let mut frame = Vec::new();
        let mut oversized = false;
        loop {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            let n = buf
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buf.len(), |i| i + 1);
            let end = buf[n - 1] == b'\n';
            if frame.len() + n <= 262144 && !oversized {
                frame.extend_from_slice(&buf[..n]);
            } else {
                oversized = true;
            }
            reader.consume(n);
            if end {
                break;
            }
        }
        if frame.is_empty() && !oversized {
            break;
        }
        let request = serde_json::from_slice::<Value>(&frame);
        let reply = match request {
            Ok(r) if !oversized => {
                let Some(id) = r.get("id") else {
                    continue;
                };
                let result = match r["method"].as_str().unwrap_or("") {
                    "initialize" => {
                        initialized = true;
                        Ok(
                            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":SERVER,"version":"1"}}),
                        )
                    }
                    "ping" => Ok(json!({})),
                    "tools/list" if initialized => Ok(tools()),
                    "tools/call" if initialized => {
                        let result = call(
                            path,
                            r["params"]["name"].as_str().unwrap_or(""),
                            r["params"]["arguments"].clone(),
                        );
                        let error = result.is_err();
                        let text = match result {
                            Ok(v) => v.to_string(),
                            Err(e) => e,
                        };
                        Ok(json!({"isError":error,"content":[{"type":"text","text":text}]}))
                    }
                    _ => Err("Initialize before calling an available method."),
                };
                match result {
                    Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                    Err(e) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":e}}),
                }
            }
            _ => {
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid or oversized question request"}})
            }
        };
        serde_json::to_writer(&mut writer, &reply)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    Ok(())
}
