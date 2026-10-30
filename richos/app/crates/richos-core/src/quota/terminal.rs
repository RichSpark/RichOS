//! Headless terminal entrypoint. Uses the desktop reset service, including its
//! record, flock, fresh preflight and durable fence before redemption.
use super::resets::Service;
use serde_json::{json, Value};
use std::{io::{self, IsTerminal}, path::PathBuf, process::Command};

/// Also emitted in JSON so the installed wrapper can inspect the binary before launch.
pub const HEADLESS_PROTOCOL: &str = "--richos-quota-v1";

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
        let view = service.tick(&bin, || true);
        return Ok(json!({"actionError": if view.state == "unknown" { view.message.clone() } else { None }, "resets":printed(&view)}));
    }
    let mut view = service.refresh(&bin, false);
    if operation == "approve" {
        let id = args.get(1).ok_or("Choose an offer id from quota-reset.sh status.")?;
        let offer = view.offers.iter().find(|o| &o.id == id)
            .ok_or("That offer is not available. Refresh status.")?.clone();
        eprintln!("Approve one use of {} ({}) at 99% overall weekly usage. No reset is used by approval.", offer.label, offer.id);
        user_approval()?;
        // Authentication can take time. Refresh before binding approval to the offer.
        let _fresh = service.refresh(&bin, true);
        view = service.approve(&offer)?;
    }
    let action_error: Option<String> = None;
    Ok(json!({"resets":printed(&view),"actionError":action_error}))
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
