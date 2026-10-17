//! `control_request{interrupt}` AGAINST A REAL CHILD PROCESS — the stop that has to
//! actually reach something (UX §9.3 step 2).
//!
//! **Ported from `acp_cancel_tests.rs` when the ACP adapter was deleted
//! (`wiki/ceo-decisions.md` §16).** Same three tests, same three invariants, same shape:
//! they do not mock `NativeClient`. They spawn a real child over real stdio and drive the
//! real handshake, the real reader thread, real `dispatch`, real drain loop. The child is a
//! POSIX-`sh` fake agent (written to a temp file by the test) so nothing here needs Claude,
//! a network, Node, or the `claude` binary.
//!
//! Two agents, because the two failure modes are different products:
//!   - a COMPLIANT one, which answers the interrupt with a terminal `result` carrying
//!     `terminal_reason: "aborted_streaming"` — the shape measured on the real binary at
//!     `docs/verification/native-claude-stream-json-2026-08-31/raw/run9-rust-driven.jsonl:65`;
//!   - a DEAF one, which ignores the interrupt entirely. RichOS must still stop rendering,
//!     and must say which of the two happened rather than reporting both as a clean cancel.

use richos_core::cognition::TurnItem;
use richos_core::native::{NativeClient, STOP_REASON_CANCELLED, STOP_REASON_CANCEL_UNACKNOWLEDGED};
use richos_core::steering::TurnCancel;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// `RICHOS_CANCEL_GRACE_MS` is process-wide, and libtest runs these tests side by side in one
/// process: the deaf test's 300 ms used to leak into the compliant test's cancel, turning a
/// clean answer from a fake agent that was slow to be scheduled into "unacknowledged" (audit
/// `docs/verification/2026-09-29-load-sensitive-checks-audit.md` R9). Every test that sets
/// or depends on the grace holds this for its whole run and states the grace it needs.
static GRACE_ENV: Mutex<()> = Mutex::new(());

struct Grace {
    _held: MutexGuard<'static, ()>,
}

impl Grace {
    fn set(ms: u64) -> Grace {
        // A test that failed while holding it leaves it poisoned; the value it guards is
        // rewritten below, so the poison carries nothing the next test needs.
        let held = GRACE_ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("RICHOS_CANCEL_GRACE_MS", ms.to_string());
        Grace { _held: held }
    }
}

impl Drop for Grace {
    fn drop(&mut self) {
        std::env::remove_var("RICHOS_CANCEL_GRACE_MS");
    }
}

/// A hang guard, never a verdict: a turn that never ends fails here with a sentence instead of
/// holding the suite until the gate's own budget runs out.
const HANG_GUARD: Duration = Duration::from_secs(60);

/// Press stop from another thread once the turn has visibly started (its first text arrived),
/// never after a fixed sleep: a sleep that ran out before the turn began pressed stop on
/// nothing, and the turn then waited forever for a `result` nobody would send.
fn press_on_first_text(cancel: std::sync::Arc<dyn TurnCancel>) -> (mpsc::Sender<()>, std::thread::JoinHandle<bool>) {
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let presser = std::thread::spawn(move || {
        started_rx.recv_timeout(HANG_GUARD).expect("the turn never produced its first text");
        cancel.cancel()
    });
    (started_tx, presser)
}

/// A fake native agent in POSIX sh. `comply=true` answers the interrupt; `false` ignores
/// it, which is the whole point of the second test.
///
/// The `case` patterns match the exact bytes `NativeClient` writes: `serde_json` serializes
/// a `json!` object through a `BTreeMap`, so keys come out alphabetically and
/// `"subtype":"interrupt"` is a stable substring rather than a hopeful one.
///
/// The script is spawned DIRECTLY (`NativeClient::spawn` takes a binary and a cwd, not an
/// argv), so it carries its own shebang and is made executable. The real client's flag
/// vector arrives as arguments and `sh` ignores them, which is what makes this a fake of the
/// binary rather than a fake of the protocol.
fn fake_agent(tag: &str, comply: bool) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "richos-fake-claude-{tag}-{}-{}.sh",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let interrupt_arm = if comply {
        r#"      printf '{"type":"result","subtype":"error_during_execution","stop_reason":null,"is_error":true,"terminal_reason":"aborted_streaming"}\n'
"#
    } else {
        "      : # deaf on purpose: this agent ignores the interrupt\n"
    };
    let script = format!(
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"initialize"'*)
      printf '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{"account":{{"email":"x@y"}}}}}}}}\n'
      ;;
    *'"subtype":"interrupt"'*)
{interrupt_arm}      ;;
    *'"type":"user"'*)
      printf '{{"type":"stream_event","event":{{"type":"message_start","message":{{"id":"msg_1"}}}}}}\n'
      printf '{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"partial answer"}}}}}}\n'
      ;;
  esac
done
"#
    );
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(script.as_bytes()).unwrap();
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

/// Spawn and hand back the client. The handshake happens inside `spawn` now — it is not
/// optional and it is not a separate step, which is itself the loud-failure guarantee.
fn connect(script: &PathBuf) -> NativeClient {
    // The chat lease carries RichOS's standing instruction, so these fakes need one on disk.
    // Rendered by the real `doctrine` module rather than stubbed, so a doctrine that stopped
    // satisfying `preflight` would break these too.
    let dir = std::env::temp_dir().join(format!("richos-doctrine-fake-{}", std::process::id()));
    let doctrine = richos_core::doctrine::ensure_rendered(
        &dir,
        &richos_core::doctrine::DoctrineIdentity::default(),
    )
    .expect("the doctrine fixture must render");
    let skills =
        richos_core::skills::ensure_rendered(&dir, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).expect("the skills fixture must render");
    match NativeClient::spawn(script, std::path::Path::new("/tmp"), &doctrine, &skills) {
        Ok(c) => c,
        Err(e) => panic!("the fake agent should have completed the handshake: {e}"),
    }
}

#[test]
fn a_compliant_agent_ends_the_turn_as_cancelled_and_the_partial_text_survives() {
    // What this test proves is what a COMPLIANT answer turns into, so the grace is set far
    // beyond any scheduling delay: a fake agent that answers the interrupt is reported as
    // having answered it however busy the Mac is. How long the grace is, is the deaf test's
    // subject, not this one's.
    let _grace = Grace::set(HANG_GUARD.as_millis() as u64);
    let script = fake_agent("comply", true);
    let client = connect(&script);

    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen_w = seen.clone();
    // Press stop once the turn is under way, from a DIFFERENT thread — which is the real
    // shape of the problem: the turn owns the spine lock, the stop cannot wait for it.
    let (started, presser) = press_on_first_text(client.cancel_handle());

    let stop = client
        .prompt("take your time", &mut |item| {
            if let TurnItem::Text { text, .. } = item {
                seen_w.lock().unwrap().push_str(text);
                started.send(()).ok(); // the presser may already have taken its first signal and gone
            }
        })
        .unwrap();

    assert!(presser.join().unwrap(), "the cancel reached a live turn");
    // Caveat C1: the agent said `stop_reason: null` and `error_during_execution`. Only
    // `terminal_reason` separates the CEO's stop from a genuine failure, and the client maps
    // it back to the one string the spine and the ledger already reason about.
    assert_eq!(stop, STOP_REASON_CANCELLED);
    // §9.3 step 4: "Preserve partial commentary, activity and assistant output."
    assert_eq!(*seen.lock().unwrap(), "partial answer");
    let _ = std::fs::remove_file(&script);
}

#[test]
fn a_deaf_agent_does_not_hold_the_turn_open_and_is_reported_as_deaf() {
    // The bound, exercised in milliseconds rather than the shipping 3s.
    const GRACE_MS: u64 = 300;
    let _grace = Grace::set(GRACE_MS);
    let script = fake_agent("deaf", false);
    let client = connect(&script);
    let (started, presser) = press_on_first_text(client.cancel_handle());

    // The turn runs on its own thread so that a loop which parks forever on a `result` that
    // never comes fails at the hang guard with a sentence, instead of holding the suite.
    let (done_tx, done_rx) = mpsc::channel();
    let turn = std::thread::spawn(move || {
        let began = std::time::Instant::now();
        let stop = client
            .prompt("ignore me", &mut |item| {
                if matches!(item, TurnItem::Text { .. }) {
                    started.send(()).ok(); // the presser may already have taken its first signal and gone
                }
            })
            .unwrap();
        done_tx.send((stop, began.elapsed())).expect("the test thread waits on done_rx");
        client
    });
    let (stop, elapsed) = done_rx
        .recv_timeout(HANG_GUARD)
        .expect("the deaf agent held the turn open: the loop never returned");
    assert!(presser.join().unwrap(), "the cancel reached a live turn");
    drop(turn.join().unwrap());

    assert_eq!(
        stop, STOP_REASON_CANCEL_UNACKNOWLEDGED,
        "an agent that ignored the interrupt must not be reported as a clean cancel"
    );
    // A FLOOR, which load can only make larger: the press came after `began` and the grace
    // runs from the press, so a loop that honored the grace cannot return sooner than it.
    // There is no ceiling: what is proven is that the loop RETURNS at all rather than parking
    // on a `result` that never comes, and the hang guard above is what catches a loop that
    // does not. (The old `< 5 s` ceiling decided the verdict by the speed of the Mac.)
    assert!(
        elapsed >= Duration::from_millis(GRACE_MS),
        "returned before the grace ran out: {elapsed:?}"
    );
    let _ = std::fs::remove_file(&script);
}

#[test]
fn cancelling_with_no_turn_in_flight_reports_that_it_reached_nothing() {
    let script = fake_agent("idle", true);
    let client = connect(&script);
    let cancel = client.cancel_handle();
    assert!(
        !cancel.cancel(),
        "there was no turn in flight, and saying otherwise would be a lie"
    );
    let _ = std::fs::remove_file(&script);
}

/// A fully local child that logs user frames and replies with the selected terminal.
fn terminal_agent(terminal: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("richos-native-terminal-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("agent.sh");
    let received = dir.join("received");
    std::fs::write(&script, format!(r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"initialize"'*)
      printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{}}}}}}'
      ;;
    *'"type":"user"'*)
      printf '%s\n' "$line" >> '{}'
      {}
      ;;
  esac
done
"#, received.display(), if terminal == "exit" { "exit 0".into() } else if terminal == "close_stdout" { "exec 1>&-".into() } else { format!("printf '%s\\n' '{terminal}'") })).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    (script, received)
}

#[test]
fn stop_between_wire_prompts_is_latched_and_never_sends_the_user_frame() {
    let (script, received) = terminal_agent(r#"{"type":"result","stop_reason":"end_turn"}"#);
    let client = connect(&script);
    let cancel = client.cancel_handle();
    cancel.begin_operation();
    assert_eq!(
        client.prompt("internal prime", &mut |_| {}).unwrap(),
        "end_turn"
    );
    assert!(
        cancel.cancel(),
        "the accepted request is active between its wire turns"
    );
    assert_eq!(
        client
            .prompt("user words that must not run", &mut |_| {})
            .unwrap(),
        STOP_REASON_CANCELLED
    );
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        1
    );
    cancel.end_operation();
    cancel.begin_operation();
    assert_eq!(
        client
            .prompt("a later explicit request", &mut |_| {})
            .unwrap(),
        "end_turn"
    );
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        2
    );
    cancel.end_operation();
}

#[test]
fn stop_before_the_first_wire_prompt_prevents_all_model_requests() {
    let (script, received) = terminal_agent(r#"{"type":"result","stop_reason":"end_turn"}"#);
    let client = connect(&script);
    let cancel = client.cancel_handle();
    cancel.begin_operation();
    assert!(cancel.cancel());
    assert_eq!(
        client.prompt("must never arrive", &mut |_| {}).unwrap(),
        STOP_REASON_CANCELLED
    );
    assert!(!received.exists());
    cancel.end_operation();
    assert!(
        !cancel.cancel(),
        "an idle cancellation must not poison the next request"
    );
}

#[test]
fn child_exit_and_error_results_are_errors_not_successful_answers() {
    for terminal in [
        "exit",
        r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["upstream request failed"]}"#,
    ] {
        let (script, _) = terminal_agent(terminal);
        let client = connect(&script);
        assert!(client.prompt("hello", &mut |_| {}).is_err());
    }
}


#[test]
fn a_closed_output_stream_cannot_accept_a_later_prompt_that_would_wait_forever() {
    let (script, _) = terminal_agent("close_stdout");
    let client = connect(&script);
    assert!(client.prompt("first", &mut |_| {}).is_err());
    // The process still reads stdin. A successful write cannot imply a live response stream.
    assert!(client.prompt("second", &mut |_| {}).is_err());
}
