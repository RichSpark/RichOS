//! The restricted session's process handling, against a stand-in `claude` (a shell script) that
//! behaves as the real one was measured to: it prints its `system/init` line only after it has
//! read the first user message. Covered: the inventory check before any tool can run, failure
//! classification, and that stopping a session ends its whole process group (PRD §2: "Quit
//! stops RichDoctor-owned Claude processes"). The real CLI is measured by
//! `examples/adapter_probe.rs` (see MEASURED.md).
use richdoctor_core::adapter::{self, Failure, Next, Session, Spec, StreamEvent};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MEASURED_INIT: &str = r#"{"type":"system","subtype":"init","tools":["mcp__richdoctor__collect_diagnostics","mcp__richdoctor__read_evidence"],"mcp_servers":[{"name":"richdoctor","status":"connected"}],"permissionMode":"dontAsk","slash_commands":[],"skills":[],"plugins":[{"name":"cc-plugin-telemetry","path":"builtin"}]}"#;

const TOOL_USE: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__richdoctor__collect_diagnostics","input":{"category":"startup_crash"}}]}}"#;

const RESULT: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"say\":\"ok\",\"question\":\"When?\",\"choices\":[],\"ready\":false}"}"#;

fn temp() -> PathBuf {
    let d = std::env::temp_dir().join(format!("rd-session-{}-{}", std::process::id(), richdoctor_core::util::hex(&richdoctor_core::util::random_bytes(4).unwrap())));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::canonicalize(d).unwrap()
}

/// A stand-in CLI: it records its arguments and environment, then runs `body`. The lines it
/// prints are written to files next to it first, so no shell quoting touches the JSON.
fn fake(dir: &Path, lines: &[(&str, &str)], body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    for (name, text) in lines {
        std::fs::write(dir.join(name), format!("{text}\n")).unwrap();
    }
    let p = dir.join("claude");
    std::fs::write(&p, format!("#!/bin/sh\nD='{}'\nprintf '%s\\n' \"$@\" > \"$PWD/args.txt\"\nenv > \"$PWD/env.txt\"\n{body}\n", dir.display())).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn spec(dir: &Path) -> Spec {
    Spec { server_exe: dir.join("RichDoctor"), draft_dir: dir.join("draft") }
}

fn env() -> Vec<(String, String)> {
    vec![("HOME".into(), "/tmp".into()), ("PATH".into(), "/usr/bin:/bin".into())]
}

fn first() -> String {
    adapter::user_message("RichOS closes before its window opens.", None)
}

#[test]
fn a_measured_inventory_starts_and_one_turn_reads_back() {
    let dir = temp();
    let bin = fake(&dir, &[("init", MEASURED_INIT), ("tool", TOOL_USE), ("result", RESULT)], "read first\ncat \"$D/init\"\ncat \"$D/tool\" \"$D/result\"\nwhile read line; do cat \"$D/tool\" \"$D/result\"; done");
    let work = dir.join("work");
    let mut s = Session::start(&bin, &spec(&dir), &work, &env()).expect("starts");
    s.begin(&first(), Duration::from_secs(10)).expect("the inventory passes");
    assert!(s.checked());
    // load-bound: catches a hang only; the stand-in answers at once and the verdict is what it said.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut tools = 0;
    let text = loop {
        match s.next(deadline) {
            Next::Event(StreamEvent::ToolUse { name, input }) => {
                tools += 1;
                assert_eq!(adapter::progress_line(&name, &input, |_| None), "Checking what happened when RichOS last opened…");
            }
            Next::Event(StreamEvent::Result { is_error: false, text, .. }) => break text,
            Next::Event(_) => {}
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(tools, 1);
    assert_eq!(adapter::parse_turn(&text).question, "When?");
    // A second turn on the same process: multi-turn, one owned process.
    s.send(&adapter::user_message("Earlier today", None)).unwrap();
    let again = loop {
        // load-bound: catches a hang only; the stand-in answers at once and the verdict is what it said.
        match s.next(Instant::now() + Duration::from_secs(10)) {
            Next::Event(StreamEvent::Result { text, .. }) => break text,
            Next::Event(_) => {}
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(again, text);
    let args = std::fs::read_to_string(work.join("args.txt")).unwrap();
    assert!(args.lines().any(|l| l == "--restricted") && args.lines().any(|l| l == "dontAsk"), "{args}");
    let seen_env = std::fs::read_to_string(work.join("env.txt")).unwrap();
    assert!(!seen_env.contains("CLAUDE_CONFIG_DIR") && !seen_env.contains("CLAUDECODE"), "{seen_env}");
    s.stop();
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_widened_inventory_ends_the_session_before_any_tool_runs() {
    let dir = temp();
    let widened = MEASURED_INIT.replace("\"mcp__richdoctor__read_evidence\"]", "\"mcp__richdoctor__read_evidence\",\"Bash\"]");
    // It would call a tool one second after its init line; the check must end it first.
    let bin = fake(&dir, &[("init", &widened), ("tool", TOOL_USE)], "read first\ncat \"$D/init\"\nsleep 1\ntouch \"$PWD/tool-ran\"\ncat \"$D/tool\"");
    let work = dir.join("work");
    let mut s = Session::start(&bin, &spec(&dir), &work, &env()).expect("starts");
    match s.begin(&first(), Duration::from_secs(10)) {
        Err((Failure::Unsupported, why)) => assert!(why.contains("Bash"), "{why}"),
        other => panic!("{other:?}"),
    }
    std::thread::sleep(Duration::from_millis(1500));
    assert!(!work.join("tool-ran").exists(), "the process was ended before it could go on");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn failures_before_the_first_answer_are_named_by_what_was_said() {
    let dir = temp();
    let signed_out = fake(&dir, &[("res", r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#)], "read first\ncat \"$D/res\"");
    let mut s = Session::start(&signed_out, &spec(&dir), &dir.join("w1"), &env()).unwrap();
    assert!(matches!(s.begin(&first(), Duration::from_secs(10)), Err((Failure::SignedOut, _))));
    let old_cli = fake(&dir, &[], "echo \"error: unknown option '--restricted'\" >&2\nexit 1");
    let mut s = Session::start(&old_cli, &spec(&dir), &dir.join("w2"), &env()).unwrap();
    assert!(matches!(s.begin(&first(), Duration::from_secs(10)), Err((Failure::Unsupported, _))));
    assert!(matches!(Session::start(&dir.join("no-such-claude"), &spec(&dir), &dir.join("w3"), &env()), Err((Failure::NotInstalled, _))));
    let silent = fake(&dir, &[], "sleep 30");
    let mut s = Session::start(&silent, &spec(&dir), &dir.join("w4"), &env()).unwrap();
    // The stand-in never answers, so the only way out is the bound: the verdict is the kind of
    // failure, never how long it took.
    assert!(matches!(s.begin(&first(), Duration::from_millis(500)), Err((Failure::Timeout, _))));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn stopping_a_session_ends_its_whole_process_group() {
    let dir = temp();
    let bin = fake(&dir, &[("init", MEASURED_INIT)], "sleep 1000 &\necho $! > \"$PWD/child.pid\"\nread first\ncat \"$D/init\"\nwhile read line; do :; done");
    let work = dir.join("work");
    let mut s = Session::start(&bin, &spec(&dir), &work, &env()).expect("starts");
    s.begin(&first(), Duration::from_secs(10)).expect("passes");
    let pid: i32 = std::fs::read_to_string(work.join("child.pid")).unwrap().trim().parse().unwrap();
    // SAFETY: signal 0 only checks that the process exists.
    assert_eq!(unsafe { kill(pid, 0) }, 0, "the grandchild runs");
    s.stop();
    // load-bound: catches a hang only; a SIGKILL to the group ends it at once, and the verdict is whether it is gone.
    let deadline = Instant::now() + Duration::from_secs(5);
    while unsafe { kill(pid, 0) } == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_ne!(unsafe { kill(pid, 0) }, 0, "the grandchild was ended with its group");
    std::fs::remove_dir_all(dir).ok();
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}
