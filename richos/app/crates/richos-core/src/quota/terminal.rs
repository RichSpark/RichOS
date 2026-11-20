//! Headless terminal entrypoint. Uses the desktop reset service, including its
//! record, flock, fresh preflight and durable fence before redemption.
use super::{fable, resets::Service, Reading, Window};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::{self, IsTerminal}, path::PathBuf, process::Command};

/// Also emitted in JSON so the installed wrapper can inspect the binary before launch.
pub const HEADLESS_PROTOCOL: &str = "--richos-quota-v1";
const FABLE_WATCH_OPERATION: &str = "fable-watch-v1";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FableTick {
    now: u64,
    enabled: bool,
    #[serde(default)]
    state: fable::State,
    reading: Option<FableReading>,
    #[serde(default)]
    refusals: Vec<FableWorkerRefusal>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FableReading {
    windows: Vec<Window>,
    #[serde(default)]
    speeds_per_second: BTreeMap<String, f64>,
    provider_wait_ms: Option<u64>,
    weekly_pause_point: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FableWorkerRefusal {
    id: String,
    fable_model: bool,
    frame: Value,
}

/// Run the app's switch against a terminal's existing reading. The watcher
/// owns its journal, so evaluating this decision never contacts Claude.
fn fable_tick(request: Value) -> Result<Value, String> {
    let FableTick { now, enabled, mut state, reading, refusals } =
        serde_json::from_value(request).map_err(|error| error.to_string())?;
    let reading = reading.map(|reading| Reading {
        provider_wait_ms: reading.windows.iter().filter_map(|window|
            reading.provider_wait_ms.map(|wait| (window.id.clone(), wait))).collect(),
        weekly_pause_points: reading.weekly_pause_point
            .map(|point| ("seven_day".into(), point)).into_iter().collect(),
        speeds: reading.speeds_per_second.into_iter()
            .map(|(id, points)| (id, points / 1000.)).collect(),
        windows: reading.windows,
        ..Reading::default()
    });
    // The terminal monitors one sign-in. The watcher replaces this state if
    // the shared reader binds to a different account.
    let account = "terminal";
    let previous = state.period(account, 0);
    if state.enabled != enabled {
        state.set_enabled(enabled, now);
    }
    state.observe(account, reading.as_ref(), now);
    let refused_workers: Vec<_> = refusals.into_iter().filter_map(|worker| {
        let limit = crate::native::worker_usage_limit_signal(&worker.frame)?;
        let named_fable = limit.window == crate::claude_accounts::LimitWindow::Fable;
        if !enabled || (!named_fable && !worker.fable_model) {
            return None;
        }
        let attributed = reading.as_ref().is_some_and(|reading| state.refused(account, reading, limit, now));
        (named_fable || attributed).then_some(worker.id)
    }).collect();
    let current = state.period(account, now);
    let change = match (previous, current) {
        (None, Some(_)) => Some("opus"),
        (Some(_), None) if enabled => Some("fable"),
        _ => None,
    };
    Ok(json!({"state": state, "onOpusUntil": current.map(|period| period.resets_at),
        "change": change, "refusedWorkers": refused_workers}))
}

/// Reset approval is account-wide, like the macOS keychain. A nightly's HOME
/// isolates app data but must not create a second approval or redemption ledger.
pub fn data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CStr, os::unix::ffi::OsStrExt};
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0u8; 16384];
        let status = unsafe { libc::getpwuid_r(libc::geteuid(), &mut entry,
            buffer.as_mut_ptr().cast(), buffer.len(), &mut result) };
        if status != 0 || result.is_null() || entry.pw_dir.is_null() {
            return Err("The operating-system account home is unavailable.".into());
        }
        let home = PathBuf::from(std::ffi::OsStr::from_bytes(unsafe { CStr::from_ptr(entry.pw_dir) }.to_bytes()));
        if !home.is_absolute() { return Err("The account home is not absolute.".into()); }
        Ok(home.join("Library/Application Support/com.richos.app"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
        Ok(std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".local/share")).join("com.richos.app"))
    }
}

fn user_approval() -> Result<(), String> {
    if !io::stdin().is_terminal() || std::env::var_os("CLAUDECODE").is_some()
        || std::env::var_os("CLAUDE_CODE_ENTRYPOINT").is_some() {
        return Err("Approval must be given by the user in an interactive terminal, outside an agent session.".into());
    }
    #[cfg(target_os = "macos")]
    {
        // A PTY is not user identity. Require macOS device-owner authentication;
        // a model cannot answer this with a command-line flag or piped text.
        let status = Command::new("/usr/bin/swift").args(["-e", r#"
import Foundation
import LocalAuthentication
let context = LAContext()
context.touchIDAuthenticationAllowableReuseDuration = 0
var done = false
var allowed = false
context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: "Approve one free Claude weekly quota reset at 99% usage for RichOS") { success, _ in
    DispatchQueue.main.async { allowed = success; done = true }
}
let deadline = Date().addingTimeInterval(120)
while !done && Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.1)) }
context.invalidate()
exit(allowed ? 0 : 1)
"#]).status().map_err(|_| "macOS user authentication requires the Swift command-line tools.".to_string())?;
        if status.success() { return Ok(()); }
    }
    Err("User authentication did not approve the reset. Nothing was armed.".into())
}

pub fn execute(args: &[String]) -> Result<Value, String> {
    let operation = args.first().map(String::as_str).unwrap_or("status");
    if operation == FABLE_WATCH_OPERATION && args.len() == 1 {
        let input = serde_json::from_reader(io::stdin().lock()).map_err(|error| error.to_string())?;
        return fable_tick(input);
    }
    if !matches!(operation, "status" | "tick" | "approve" | "revoke")
        || args.len() > if operation == "approve" { 2 } else { 1 } {
        return Err("Usage: quota-reset.sh status | tick | approve <offer-id> | revoke".into());
    }
    if operation == "approve" && (!io::stdin().is_terminal()
        || std::env::var_os("CLAUDECODE").is_some()
        || std::env::var_os("CLAUDE_CODE_ENTRYPOINT").is_some()) {
        return Err("Only the user can approve a reset from an interactive terminal outside an agent session.".into());
    }
    if operation != "revoke" { super::reset_transport::System::validate_environment()?; }
    let service = Service::new(&data_dir()?);
    if operation == "revoke" { return Ok(json!({"resets": printed(&service.revoke()?)})); }
    let bin = std::env::var_os("QUOTA_CLAUDE_BIN").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("claude"));
    if operation == "tick" {
        let at_limit = std::env::var("RICHOS_QUOTA_WEEKLY_LIMIT").as_deref() == Ok("1")
            || service.prepared_action_due(crate::util::now_millis());
        let weekly_used = std::env::var("RICHOS_QUOTA_WEEKLY_USED").ok()
            .and_then(|used| used.parse::<f64>().ok()).filter(|used| (0. ..=100.).contains(used));
        let view = service.tick_with_shared_usage(&bin, at_limit, weekly_used, None, || true);
        return Ok(json!({"actionError": if view.state == "unknown" { view.message.clone() } else { None },
            "status": view.status, "resets":printed(&view)}));
    }
    let mut view = service.tick_at_limit(&bin, false, || false);
    if operation == "approve" {
        let id = args.get(1).ok_or("Choose an offer id from quota-reset.sh status.")?;
        let offer = view.offers.iter().find(|o| &o.id == id)
            .ok_or("That offer is not available. Refresh status.")?.clone();
        eprintln!("Approve one use of {} ({}) at 99% overall weekly usage. No reset is used by approval.", offer.label, offer.id);
        user_approval()?;
        // Authentication can take time. Bind against the current persisted offer.
        view = service.approve(&offer)?;
    }
    let action_error: Option<String> = None;
    Ok(json!({"resets":printed(&view),"actionError":action_error,
        "status": view.status}))
}

/// **The reset record as printed, without its weekly figure** (richos-hq follow-ups.md,
/// 7fed0bef). The record is shared by every app and terminal on this Mac and its weekly figure
/// is whichever account last refreshed it, named nowhere; a terminal reader took it for the
/// account in use. `quota-watch.sh` reports the weekly usage of the account it reads; the record
/// keeps its own copy for the prepared reset action only.
fn printed(view: &super::resets::View) -> Value {
    let mut value = json!(view);
    if let Some(fields) = value.as_object_mut() {
        fields.remove("weeklyUsed");
        fields.remove("weeklyResetsAt");
    }
    value
}

pub fn run(args: Vec<String>) -> i32 {
    match execute(&args) {
        Ok(mut value) => { value["protocol"] = json!(HEADLESS_PROTOCOL); println!("{value}"); 0 }
        Err(error) => { eprintln!("{error}"); 2 }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn a_terminal_watch_switches_at_the_core_point_and_returns_at_reset() {
        let now = 1_800_000_000_000_u64;
        let reset = now + 600_000;
        let mut request = json!({
            "now": now, "enabled": true, "reading": {
                "windows": [
                    {"id": "seven_day", "label": "Weekly", "usedPercent": 41.0,
                     "resetsAt": now + 900_000, "durationMs": 604_800_000},
                    {"id": "model:Fable", "label": "Weekly · Fable", "usedPercent": 99.8,
                     "resetsAt": reset, "durationMs": 604_800_000}
                ]
            }
        });
        let safe = fable_tick(request.clone()).unwrap();
        assert_eq!(safe["onOpusUntil"], Value::Null);
        assert_eq!(safe["change"], Value::Null);
        request["reading"]["windows"][1]["usedPercent"] = json!(99.9);
        let switch = fable_tick(request.clone()).unwrap();
        assert_eq!(switch["onOpusUntil"], reset);
        assert_eq!(switch["change"], "opus");
        request["state"] = switch["state"].clone();
        assert_eq!(fable_tick(request.clone()).unwrap()["change"], Value::Null);
        request["now"] = json!(reset);
        request["reading"] = Value::Null;
        let resumed = fable_tick(request.clone()).unwrap();
        assert_eq!(resumed["onOpusUntil"], Value::Null);
        assert_eq!(resumed["change"], "fable");
        request["state"] = resumed["state"].clone();
        assert_eq!(fable_tick(request).unwrap()["change"], Value::Null);
    }

    #[test]
    fn a_terminal_fable_refusal_is_separate_from_an_ordinary_rate_limit() {
        let now = 1_800_000_000_000_u64;
        let reset = now + 600_000;
        let request = json!({
            "now": now, "enabled": true, "reading": {
                "windows": [
                    {"id": "seven_day", "label": "Weekly", "usedPercent": 41.0,
                     "resetsAt": now + 900_000, "durationMs": 604_800_000},
                    {"id": "model:Fable", "label": "Weekly · Fable", "usedPercent": 80.0,
                     "resetsAt": reset, "durationMs": 604_800_000}
                ]
            },
            "refusals": [
                {"id": "generic", "fableModel": true, "frame": {"type": "result", "api_error_status": 429}},
                {"id": "general", "fableModel": true, "frame": {"type": "rate_limit_event",
                    "rate_limit_info": {"status": "rejected", "rateLimitType": "seven_day"}}},
                {"id": "specific", "fableModel": false, "frame": {"type": "rate_limit_event",
                    "rate_limit_info": {"status": "rejected", "rateLimitType": "seven_day_overage_included",
                                    "resetsAt": reset / 1000}}}
            ]
        });
        let changed = fable_tick(request).unwrap();
        assert_eq!(changed["refusedWorkers"], json!(["specific"]));
        assert_eq!(changed["onOpusUntil"], reset);
        assert_eq!(changed["change"], "opus");
    }

    #[test]
    #[ignore = "invoked under the isolated nightly environment by its parent test"]
    fn account_store_peer() { println!("account-reset-store={}", data_dir().unwrap().display()); }

    #[test]
    fn isolated_nightly_home_and_terminal_resolve_the_same_account_store() {
        let root = super::super::tests::Scratch::new();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "quota::terminal::tests::account_store_peer", "--nocapture"])
            .env("HOME", root.path()).env("CFFIXED_USER_HOME", root.path())
            .output().unwrap();
        assert!(output.status.success());
        let expected = format!("account-reset-store={}", data_dir().unwrap().display());
        assert!(String::from_utf8(output.stdout).unwrap().contains(&expected));
        assert!(!root.path().join("Library").exists(), "path resolution must not write data");
    }
}
