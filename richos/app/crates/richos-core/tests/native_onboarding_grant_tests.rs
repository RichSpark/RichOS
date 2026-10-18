//! Real stdio child coverage for the grant surrounding visible chat delivery.
//! The fake CLI discovers the scope from shipping argv, not from a test-only seam.
#![cfg(unix)]

use richos_core::cognition::{Cognition, TurnItem};
use richos_core::entity::EntityId;
use richos_core::native::{NativeCognition, STOP_REASON_CANCELLED};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

struct Fixture {
    lease: NativeCognition,
    dir: PathBuf,
    scope: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("richos grant fixture {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("agent.sh");
        std::fs::write(&script, r#"#!/usr/bin/env python3
import json, os, pathlib, sys
scope = pathlib.Path(json.loads(sys.argv[sys.argv.index("--mcp-config") + 1])["mcpServers"]["richos_onboarding"]["args"][1])
pathlib.Path("scope-path").write_text(str(scope))
pathlib.Path("child-pid").write_text(str(os.getpid()))
def send(frame): print(json.dumps(frame), flush=True)
def read(): return json.loads(sys.stdin.readline())
for index, line in enumerate(sys.stdin):
    frame = json.loads(line)
    request = frame.get("request", {})
    if request.get("subtype") == "initialize":
        send({"type":"control_response","response":{"subtype":"success","request_id":frame["request_id"],"response":{}}})
    elif request.get("subtype") == "interrupt":
        send({"type":"result","subtype":"error_during_execution","is_error":True,"terminal_reason":"aborted_streaming"})
    elif frame["type"] == "user":
        send({"type":"command_lifecycle","command_uuid":frame["uuid"],"state":"started"})
        send({"type":"control_request","request_id":str(index),"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt":frame["message"]["content"][0]["text"]}}})
        hook = read()["response"]["response"]
        if hook.get("decision") == "block":
            send({"type":"result","subtype":"success","num_turns":0,"result":"UserPromptSubmit operation blocked by hook"})
            continue
        try:
            allowed = json.loads(scope.read_text())["actions_allowed"]
            observation = "granted" if allowed else "denied"
        except (OSError, ValueError):
            observation = "invalid"
        with open("observations", "a") as out: out.write(observation + "\n")
        send({"type":"system","subtype":"init","tools":["mcp__richos_onboarding__save_company_notes","mcp__richos_onboarding__decline_onboarding"],"plugins":[{"name":"rich-skills"}]})
        mode = pathlib.Path("mode").read_text() if pathlib.Path("mode").exists() else "ok"
        if mode == "hold":
            send({"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_hold"}}})
            send({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ready to stop"}}})
        elif mode == "error":
            send({"type":"result","subtype":"error_during_execution","is_error":True,"errors":["fixture failure"]})
        elif mode == "exit":
            sys.exit(0)
        else:
            if mode == "corrupt": scope.write_text("broken scope")
            send({"type":"result","stop_reason":"end_turn"})
"#).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let skills = richos_core::skills::ensure_rendered(&dir, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
        let lease = NativeCognition::start_with_onboarding(
            &script,
            &dir,
            &dir,
            &skills,
            Path::new("/fixture/onboarding-server"),
            None,
        )
        .unwrap();
        let scope = PathBuf::from(std::fs::read_to_string(dir.join("scope-path")).unwrap());
        Self { lease, dir, scope }
    }

    fn bind(&mut self) {
        self.lease
            .set_onboarding_scope(
                &EntityId::parse("fixture-company").unwrap(),
                &self.dir.join("central"),
                &self.dir.join("onboarding.json"),
            )
            .unwrap();
    }

    fn allowed(&self) -> bool {
        if !self.scope.exists() { return false; }
        let scope: Value = serde_json::from_slice(&std::fs::read(&self.scope).unwrap()).unwrap();
        scope["actions_allowed"].as_bool().unwrap()
    }

    fn observations(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("observations"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn mode(&self, mode: &str) {
        std::fs::write(self.dir.join("mode"), mode).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // NativeCognition's field drop kills the child. Files are private to this test.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn onboarding_grant_exists_only_during_visible_wire_delivery() {
    let mut f = Fixture::new();
    f.bind();
    assert!(!f.allowed());
    assert!(f.observations().is_empty(), "preparation must not send a hidden model turn");
    assert_eq!(
        f.lease.prompt("visible question", &mut |_| {}).unwrap(),
        "end_turn"
    );
    assert!(!f.allowed());
    assert_eq!(f.observations(), ["granted"]);
}

#[test]
fn onboarding_grant_is_revoked_after_error_and_child_exit() {
    for mode in ["error", "exit"] {
        let mut f = Fixture::new();
        f.bind();
        f.mode(mode);
        assert!(f.lease.prompt("visible request", &mut |_| {}).is_err());
        assert_eq!(f.observations(), ["granted"]);
        assert!(
            !f.allowed(),
            "the {mode} path left onboarding actions enabled"
        );
    }
}

#[test]
fn onboarding_grant_is_revoked_when_stop_ends_the_visible_prompt() {
    let mut f = Fixture::new();
    f.bind();
    f.mode("hold");
    let cancel = f.lease.cancel_handle().unwrap();
    cancel.begin_operation();
    let scope_at_stop = f.scope.clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let presser = std::thread::spawn(move || {
        let ready = ready_rx.recv_timeout(Duration::from_secs(5));
        let reached = cancel.cancel();
        let stopped_scope: Value = serde_json::from_slice(&std::fs::read(&scope_at_stop).unwrap()).unwrap();
        assert_eq!(stopped_scope["actions_allowed"], false, "Stop returned while the tool grant was still active");
        // Even a broken stream fixture must cancel before reporting its timeout.
        ready.unwrap();
        assert!(reached);
        cancel
    });
    let reason = f
        .lease
        .prompt("wait for my stop", &mut |item| {
            if matches!(item, TurnItem::Text { .. }) {
                ready_tx.send(()).unwrap();
            }
        })
        .unwrap();
    presser.join().unwrap().end_operation();
    assert_eq!(reason, STOP_REASON_CANCELLED);
    assert_eq!(f.observations(), ["granted"]);
    assert!(!f.allowed());
}

#[test]
fn missing_or_invalid_scope_refuses_model_execution() {
    let mut f = Fixture::new();
    assert!(!f.scope.exists());
    assert!(f.lease.prompt("no company scope", &mut |_| {}).is_err());
    assert!(f.observations().is_empty());
    assert!(!f.allowed());

    // A retained scope can become invalid between inputs. With no fresh binding,
    // the hook must refuse it before model execution and retire the unsafe child.
    let mut f = Fixture::new();
    f.bind();
    f.lease.prompt("valid scope", &mut |_| {}).unwrap();
    assert_eq!(f.observations(), ["granted"]);
    std::fs::write(&f.scope, "malformed").unwrap();
    assert!(f.lease.prompt("invalid company scope", &mut |_| {}).is_err());
    assert_eq!(f.observations(), ["granted"], "the refused input never ran");
    assert!(!f.scope.exists(), "an invalid grant must not survive refusal");
}

#[test]
fn failed_revocation_kills_the_child_and_refuses_further_delivery() {
    let mut f = Fixture::new();
    f.bind();
    f.mode("corrupt");
    assert!(f
        .lease
        .prompt("corrupt scope before returning", &mut |_| {})
        .is_err());
    assert_eq!(f.observations(), ["granted"]);
    assert!(
        !f.scope.exists(),
        "invalid grant must be removed after failed revocation"
    );
    let pid: libc::id_t = std::fs::read_to_string(f.dir.join("child-pid")).unwrap().parse().unwrap();
    // kill(-0) also succeeds for an exited, unreaped child. Leave reaping to its owner.
    // load-bound: hang guard for positive child-exit evidence, never a speed verdict.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut status: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::waitid(libc::P_PID, pid, &mut status, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
        assert_eq!(result, 0, "could not observe the owned child's exit");
        if unsafe { status.si_pid() } != 0 { break; }
        assert!(std::time::Instant::now() < deadline, "the child with an unrevokeable grant is still alive");
        std::thread::sleep(Duration::from_millis(10));
    }
    f.bind();
    f.mode("ok");
    assert!(f
        .lease
        .prompt("cannot reuse the retired child", &mut |_| {})
        .is_err());
    assert_eq!(f.observations(), ["granted"]);
    assert!(!f.allowed());
}

#[test]
fn a_stop_latched_before_admission_leaves_no_grant_or_model_execution() {
    let mut f = Fixture::new();
    f.bind();
    let cancel = f.lease.cancel_handle().unwrap();
    cancel.begin_operation();
    assert!(cancel.cancel());
    let error = f.lease.prompt("already stopped", &mut |_| {}).unwrap_err();
    assert!(error.to_string().contains(STOP_REASON_CANCELLED));
    assert!(f.observations().is_empty());
    assert!(!f.allowed());
    cancel.end_operation();
}
