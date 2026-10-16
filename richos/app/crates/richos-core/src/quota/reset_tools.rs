//! Desktop work-lease MCP tools. The model can inspect or consume a USER approval,
//! never create one. The desktop host also runs the prepared action at 99% weekly,
//! so exhaustion cannot prevent it by blocking the orchestrator's next model turn.
use super::{gate::read_json, reset_transport::System, resets::Service};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

pub const SERVER: &str = "richos_quota";

fn authorized(scope: &Path) -> bool {
    read_json::<Value>(scope).is_ok_and(|s| s["version"] == 1 && s["actions_allowed"] == true)
}
fn invoke(
    scope: &Path,
    root: &Path,
    bin: &Path,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    if !args.as_object().is_some_and(|a| a.is_empty()) {
        return Err(
            "Quota tools take no arguments. Approval comes from the user's settings.".into(),
        );
    }
    if !authorized(scope) {
        return Err("This work session is no longer authorized.".into());
    }
    let service = Service::new(root);
    let view = match name {
        "status" => service.view(),
        "use_approved_reset" => {
            if service.view().approval.is_none() {
                return Err("The user has not approved a reset in Technical Settings.".into());
            }
            let mut transport = System::connect(bin)?;
            service.use_approved(&mut transport, || authorized(scope))?
        }
        _ => return Err("Unknown quota tool.".into()),
    };
    serde_json::to_value(view).map_err(|_| "Could not read reset status.".into())
}
fn tools() -> Value {
    json!({"tools":[
        {"name":"status","description":"Read Claude reset offers, 99% weekly trigger and explicit prior user approval. No reset is used.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"use_approved_reset","description":"Use exactly one previously user-approved reset, only with fresh weekly usage at or above 99%. Does not grant approval. A five-hour pause never qualifies. The desktop host already runs the prepared action automatically. Do not retry an uncertain outcome.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false}}
    ]})
}
fn response(
    scope: &Path,
    root: &Path,
    bin: &Path,
    value: Value,
    initialized: &mut bool,
) -> Option<Value> {
    let id = value.get("id")?.clone(); // Notifications cannot execute actions.
    let method = value.get("method").and_then(Value::as_str);
    let fail =
        |code, message| json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}});
    if value["jsonrpc"] != "2.0" || (!id.is_string() && !id.is_number()) {
        return Some(fail(-32600, "Invalid request"));
    }
    let result = match method {
        Some("initialize") => {
            *initialized = true;
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":SERVER,"version":"1"}})
        }
        Some("ping") => json!({}),
        _ if !*initialized => return Some(fail(-32002, "Initialize first")),
        Some("tools/list") => tools(),
        Some("tools/call") => {
            let args = value
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(json!({}));
            let name = value
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            match invoke(scope, root, bin, name, &args) {
                Ok(v) => json!({"content":[{"type":"text","text":v.to_string()}],"isError":false}),
                Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
            }
        }
        _ => return Some(fail(-32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
pub fn serve(
    scope: &Path,
    root: &Path,
    bin: &Path,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    let mut initialized = false;
    loop {
        let mut line = Vec::new();
        let mut oversized = false;
        loop {
            let buffer = input.fill_buf()?;
            if buffer.is_empty() {
                break;
            }
            let count = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buffer.len(), |i| i + 1);
            let end = buffer[count - 1] == b'\n';
            if line.len() + count > 262144 {
                oversized = true;
            }
            if !oversized {
                line.extend_from_slice(&buffer[..count]);
            }
            input.consume(count);
            if end {
                break;
            }
        }
        if line.is_empty() && !oversized {
            break;
        }
        let reply = if oversized {
            Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Message too large"}}),
            )
        } else {
            match serde_json::from_slice(&line) {
                Ok(v) => response(scope, root, bin, v, &mut initialized),
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}}),
                ),
            }
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}
pub fn run_stdio(scope: &Path, root: &Path, bin: &Path) -> io::Result<()> {
    let shared = super::terminal::data_dir().map_err(io::Error::other)?;
    Service::new(&shared).import_legacy(root).map_err(io::Error::other)?;
    serve(scope, &shared, bin, io::stdin().lock(), io::stdout().lock())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tools_cannot_grant_approval_and_notifications_cannot_act() {
        let names: Vec<_> = tools()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["status", "use_approved_reset"]);
        let absent = Path::new("/nonexistent-quota-scope");
        assert!(invoke(absent, absent, absent, "use_approved_reset", &json!({})).is_err());
        let mut initialized = true;
        assert!(response(
            absent,
            absent,
            absent,
            json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"use_approved_reset"}}),
            &mut initialized
        )
        .is_none());
    }
    #[test]
    fn live_scope_still_cannot_reset_without_user_approval_or_accept_model_approval() {
        let scratch = super::super::tests::Scratch::new();
        let scope = scratch.path().join("scope.json");
        std::fs::write(&scope, r#"{"version":1,"actions_allowed":true}"#).unwrap();
        let bin = Path::new("/must-not-start");
        assert!(invoke(
            &scope,
            scratch.path(),
            bin,
            "use_approved_reset",
            &json!({})
        )
        .unwrap_err()
        .contains("not approved"));
        assert!(invoke(
            &scope,
            scratch.path(),
            bin,
            "use_approved_reset",
            &json!({"approved":true})
        )
        .is_err());
        assert!(invoke(&scope, scratch.path(), bin, "approve", &json!({})).is_err());
    }
}
