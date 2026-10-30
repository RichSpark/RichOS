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
/// **`status`: the account in use, named, and only its figures** (richos-hq follow-ups.md,
/// 7fed0bef). On 2026-10-28 this tool answered with the shared reset record alone: `weeklyUsed:
/// 99` with no account named, Home's figure while the app ran on Work at 10%, and a lead deferred
/// work until Thursday on it. The reply is now built from the file the app publishes
/// (`<app>/engine-state/claude-quota.json`, whose top level is the account in use) and the gate's
/// own [`admission`](super::gate::admission); the reset parts keep no weekly figure, and no other
/// account appears.
///
/// `declared` is the lease's `RICHOS_APP_STATE`, when it has one (`engine_profile.rs`): a server
/// pointed at another app's folder would report another app's account, so it must match.
///
/// `caller` is the session's own account, its `RICHOS_CLAUDE_ACCOUNT` ([`super::gate::ACCOUNT_ENV`]),
/// when it has one (second review of 0b0afd1f1, P2): during a move the top level is still the
/// account being left (Home at 95%) while a new session runs on the account in use (Work at 10%),
/// and a Work session was told Home's held admission. With a row for it, the reply is that
/// account's own figures and the gate's decision on it, exactly as the gate judges its calls.
pub(super) fn status(app: &Path, resets: &Service, now: u64, declared: Option<&std::ffi::OsStr>,
                     caller: Option<&str>) -> Result<Value, String> {
    let state = app.join("engine-state");
    if let Some(declared) = declared.filter(|v| !v.is_empty()) {
        let same = match (std::fs::canonicalize(declared), std::fs::canonicalize(&state)) {
            (Ok(a), Ok(b)) => a == b,
            _ => Path::new(declared) == state,
        };
        if !same {
            return Err("This quota server reads a different app's state than this work session. No reading is reported.".into());
        }
    }
    let view: super::View = read_json(&state.join("claude-quota.json"))
        .map_err(|_| "The app has not published a usage reading yet. No reading is reported.".to_string())?;
    // A file an older app wrote has no `inUse`: its `accounts[]` row marked in use names the
    // account (two or more accounts); with none, there is one account, Account 1. Measured on the
    // live file of 2026-10-28 17:14Z: defaulting to Account 1 named Home beside Work's figures.
    let account = match (&view.in_use, view.accounts.iter().find(|a| a.in_use)) {
        (Some(a), _) => json!({"id": a.id, "label": a.label}),
        (None, Some(row)) => json!({"id": row.id, "label": row.label}),
        (None, None) if view.accounts.is_empty() => json!({"id": crate::claude_accounts::ACCOUNT_ONE,
            "label": view.first_label.clone().unwrap_or_else(|| "Account 1".into())}),
        (None, None) => return Err("The app's reading does not say which account is in use. No reading is reported.".into()),
    };
    let r = resets.view();
    let row = caller.filter(|id| !id.is_empty()).and_then(|id| view.accounts.iter().find(|a| a.id == id));
    let (account, windows, checked_at, reading_state) = match row {
        // The row's state as the gate rebuilds it from the same fields (`gate::decide`).
        Some(row) => (json!({"id": row.id, "label": row.label}), row.windows.clone(), row.checked_at, super::Snapshot {
            windows: row.windows.clone(),
            checked_at: row.checked_at,
            error: row.message.as_ref().map(|_| super::ReadError::Failed),
            speeds: row.speeds.clone(),
            empty_at: row.empty_at,
            ..Default::default()
        }.view(view.policy.clone(), now).state),
        None => (account, view.windows.clone(), view.checked_at, view.state),
    };
    Ok(json!({
        "account": account,
        "windows": windows,
        "checkedAt": checked_at,
        "state": reading_state,
        "heldUntil": view.held_until,
        // The gate's decision on the account named above: its own row when the caller has one,
        // else the top level (main's gate keeps `admission` for tests only; `decide` with no
        // account is that decision).
        "admission": super::gate::decide(&state, row.map(|a| a.id.as_str()), now).0,
        "resets": {
            "state": r.state,
            "offers": r.offers,
            "approval": r.approval,
            "lastAttempt": r.last_attempt,
            "message": r.message,
        },
    }))
}
fn invoke(
    scope: &Path,
    app: &Path,
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
        "status" => return status(app, &service, crate::util::now_millis(),
            std::env::var_os("RICHOS_APP_STATE").as_deref(),
            std::env::var(super::gate::ACCOUNT_ENV).ok().as_deref()),
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
        {"name":"status","description":"Read the usage of the Claude account IN USE, named in `account` (id and label): its windows, when they were checked and `admission`, the app's own gate decision. Any hold or wait comes only from `admission`; never defer or hold work on any other figure. Also reads reset offers and explicit prior user approval. Other accounts are not reported. No reset is used.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"use_approved_reset","description":"Use exactly one previously user-approved reset, only with fresh weekly usage at or above 99%. Does not grant approval. A five-hour pause never qualifies. The desktop host already runs the prepared action automatically. Do not retry an uncertain outcome.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false}}
    ]})
}
fn response(
    scope: &Path,
    app: &Path,
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
            match invoke(scope, app, root, bin, name, &args) {
                Ok(v) => json!({"content":[{"type":"text","text":v.to_string()}],"isError":false}),
                Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
            }
        }
        _ => return Some(fail(-32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
/// `app` is the app's data folder (its `engine-state` holds the published reading); `root` is
/// the shared reset store (`terminal::data_dir`).
pub fn serve(
    scope: &Path,
    app: &Path,
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
                Ok(v) => response(scope, app, root, bin, v, &mut initialized),
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
    serve(scope, root, &shared, bin, io::stdin().lock(), io::stdout().lock())
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
        assert!(invoke(absent, absent, absent, absent, "use_approved_reset", &json!({})).is_err());
        let mut initialized = true;
        assert!(response(
            absent,
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
            scratch.path(),
            bin,
            "use_approved_reset",
            &json!({"approved":true})
        )
        .is_err());
        assert!(invoke(&scope, scratch.path(), scratch.path(), bin, "approve", &json!({})).is_err());
    }
}
