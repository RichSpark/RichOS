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
        std::fs::write(&script, r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--mcp-config' ]; then
    shift
    scope=$(printf '%s' "$1" | sed -n 's/.*"args":\["--onboarding-mcp","\([^"]*\)"\].*/\1/p')
  fi
  shift
done
[ -n "$scope" ] || exit 22
printf '%s' "$scope" > scope-path
printf '%s' "$$" > child-pid
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"initialize"'*)
      printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
      ;;
    *'"subtype":"interrupt"'*)
      printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"terminal_reason":"aborted_streaming"}'
      ;;
    *'"type":"user"'*)
      case "$(cat "$scope" 2>/dev/null)" in
        *'"actions_allowed":true'*) printf 'granted\n' >> observations ;;
        *'"actions_allowed":false'*) printf 'denied\n' >> observations ;;
        *) printf 'invalid\n' >> observations ;;
      esac
      printf '%s\n' '{"type":"system","subtype":"init","tools":["mcp__richos_onboarding__save_company_notes","mcp__richos_onboarding__decline_onboarding"],"plugins":[{"name":"rich-skills"}]}'
      case "$(cat mode 2>/dev/null)" in
        hold)
          printf '%s\n' '{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_hold"}}}'
          printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ready to stop"}}}'
          ;;
        error)
          printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["fixture failure"]}'
          ;;
        exit) exit 0 ;;
        corrupt)
          printf 'broken scope' > "$scope"
          printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
          ;;
        *) printf '%s\n' '{"type":"result","stop_reason":"end_turn"}' ;;
      esac
      ;;
  esac
done
"#).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let doctrine = richos_core::doctrine::ensure_rendered(&dir, &Default::default()).unwrap();
        let skills = richos_core::skills::ensure_rendered(&dir, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
        let lease = NativeCognition::start_with_onboarding(
            &script,
            &dir,
            &doctrine,
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
    f.lease.reprime("context to remember", &mut |_| {}).unwrap();
    assert!(!f.allowed());
    assert_eq!(f.observations(), ["denied"]);
    assert_eq!(
        f.lease.prompt("visible question", &mut |_| {}).unwrap(),
        "end_turn"
    );
    assert!(!f.allowed());
    f.lease.reprime("later context", &mut |_| {}).unwrap();
    assert!(!f.allowed());
    assert_eq!(f.observations(), ["denied", "granted", "denied"]);
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
fn missing_or_invalid_scope_refuses_delivery_before_any_user_frame() {
    let mut f = Fixture::new();
    assert!(!f.scope.exists());
    assert!(f.lease.prompt("no company scope", &mut |_| {}).is_err());
    assert!(f.observations().is_empty());
    f.bind();
    std::fs::write(&f.scope, "malformed").unwrap();
    assert!(f
        .lease
        .prompt("invalid company scope", &mut |_| {})
        .is_err());
    assert!(f.observations().is_empty());
    assert_eq!(std::fs::read_to_string(&f.scope).unwrap(), "malformed");
    f.bind();
    assert_eq!(
        f.lease.prompt("repaired scope", &mut |_| {}).unwrap(),
        "end_turn"
    );
    assert_eq!(f.observations(), ["granted"]);
    assert!(!f.allowed());
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
    let pid = std::fs::read_to_string(f.dir.join("child-pid")).unwrap();
    assert!(
        !std::process::Command::new("/bin/kill")
            .args(["-0", &pid])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success(),
        "the child with an unrevokeable grant is still alive"
    );
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
fn a_stop_latched_before_delivery_leaves_no_grant_or_user_frame() {
    let mut f = Fixture::new();
    f.bind();
    let cancel = f.lease.cancel_handle().unwrap();
    cancel.begin_operation();
    assert!(cancel.cancel());
    assert_eq!(
        f.lease.prompt("already stopped", &mut |_| {}).unwrap(),
        STOP_REASON_CANCELLED
    );
    assert!(f.observations().is_empty());
    assert!(!f.allowed());
    cancel.end_operation();
}
