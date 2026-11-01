//! **No window command blocks one of Tauri's async workers** (`crates/richos-command/src/lib.rs`), proven
//! through Tauri's real IPC path and held for every command by a scan of the source.
//!
//! The first test is the real-app defect of esc-20261011T062202Z-883703bd in miniature: the
//! window's commands go through `tauri::test`'s mock runtime, which dispatches them exactly as
//! the app does (`Webview::on_message` -> the `generate_handler!` closure -> `respond_async` ->
//! `async_runtime::spawn`), onto the same global multi-threaded runtime. Enough blocking
//! commands to fill every worker are put in flight first: child processes standing in for
//! `richos_voice::stt::time_decode`'s `Command::output`, waits on a held mutex standing in for
//! the spine, and an async command that blocks inside. Then the stand-in for the Preferences
//! save is sent, and it must answer in under a second.
//!
//! Red with `#[tauri::command(async)]` on the stand-ins (the attribute every command carried
//! before): the save queues until a child process exits and frees a worker.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponse};
use tauri::test::{mock_builder, mock_context, noop_assets, INVOKE_KEY, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Manager, State, WebviewWindow, WebviewWindowBuilder};

/// How long a stand-in child process runs: long enough that a save queued behind it is
/// unmistakably late, short enough that the green run is a few seconds.
const DECODE_SECS: u64 = 3;
/// The CEO's words: saved when Preferences closes, not later. Under a second, with the
/// workers full.
const SAVE_DEADLINE: Duration = Duration::from_secs(1);

/// Blocking stand-ins that have reached their bodies, so the save is sent only once the
/// workers really are taken.
static ENTERED: AtomicUsize = AtomicUsize::new(0);

/// The spine: held by the test for as long as the save is being measured.
struct HeldSpine(Mutex<()>);
/// What the save wrote.
struct Saved(Mutex<Option<String>>);

/// `time_decode`'s shape: a child process the body waits for.
#[richos_command::command]
fn stand_in_decode() -> Result<(), String> {
    ENTERED.fetch_add(1, Ordering::SeqCst);
    let out = std::process::Command::new("/bin/sleep").arg(DECODE_SECS.to_string()).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err("sleep failed".into()) }
}

/// `decline_onboarding`'s shape: a body parked on the spine's mutex.
#[richos_command::command]
fn stand_in_spine(spine: State<'_, HeldSpine>) -> Result<(), String> {
    ENTERED.fetch_add(1, Ordering::SeqCst);
    let _held = spine.0.lock().map_err(|e| e.to_string())?;
    Ok(())
}

/// An async command that blocks inside its body rather than at an `.await`.
#[richos_command::command]
async fn stand_in_async_block() -> Result<(), String> {
    ENTERED.fetch_add(1, Ordering::SeqCst);
    let out = std::process::Command::new("/bin/sleep").arg(DECODE_SECS.to_string()).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err("sleep failed".into()) }
}

/// The Preferences save: microseconds of work, with an argument and `?` in the body.
#[richos_command::command]
fn stand_in_save(saved: State<'_, Saved>, model: String) -> Result<String, String> {
    let mut slot = saved.0.lock().map_err(|e| e.to_string())?;
    *slot = Some(model.clone());
    Ok(model)
}

fn request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

/// Send one command the way the window does and hand its answer to `tx` when it comes.
fn send(window: &WebviewWindow<MockRuntime>, cmd: &str, body: serde_json::Value, tx: mpsc::Sender<(String, Result<serde_json::Value, serde_json::Value>)>) {
    let name = cmd.to_string();
    window.as_ref().clone().on_message(
        request(cmd, body),
        Box::new(move |_webview, _cmd, response, _callback, _error| {
            let answer = match response {
                InvokeResponse::Ok(body) => Ok(body.deserialize::<serde_json::Value>().unwrap_or(serde_json::Value::Null)),
                InvokeResponse::Err(e) => Err(e.0),
            };
            let _ = tx.send((name.clone(), answer));
        }),
    );
}

#[test]
fn the_preferences_save_answers_while_blocking_commands_fill_every_worker() {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![stand_in_decode, stand_in_spine, stand_in_async_block, stand_in_save])
        .build(mock_context(noop_assets()))
        .expect("the mock app builds");
    app.manage(HeldSpine(Mutex::new(())));
    app.manage(Saved(Mutex::new(None)));
    let window = WebviewWindowBuilder::new(&app, "main", Default::default()).build().expect("the mock window builds");

    // The runtime the app's commands run on, and how many workers it has. One more blocking
    // command than that, so a worker-bound body cannot leave even one free.
    let workers = tauri::async_runtime::handle().inner().metrics().num_workers();
    let blockers = workers + 1;

    let spine = app.state::<HeldSpine>();
    let held = spine.0.lock().unwrap();
    let (tx, rx) = mpsc::channel();
    for i in 0..blockers {
        let cmd = match i % 3 { 0 => "stand_in_decode", 1 => "stand_in_spine", _ => "stand_in_async_block" };
        send(&window, cmd, serde_json::json!({}), tx.clone());
    }
    // Every worker taken (worker-bound bodies) or every blocker running (bodies off the
    // workers) — whichever this build can reach — before the save is sent.
    let waiting_since = Instant::now();
    while ENTERED.load(Ordering::SeqCst) < workers && waiting_since.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ENTERED.load(Ordering::SeqCst) >= workers, "the blocking stand-ins never filled the {workers} workers");

    let (save_tx, save_rx) = mpsc::channel();
    let sent = Instant::now();
    send(&window, "stand_in_save", serde_json::json!({ "model": "fable" }), save_tx);
    let answer = save_rx.recv_timeout(Duration::from_secs(20));
    let took = sent.elapsed();

    // Always let the stand-ins finish, including on the red build, so nothing is left running.
    drop(held);
    let mut finished = 0;
    while finished < blockers {
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok((cmd, result)) => {
                assert!(result.is_ok(), "{cmd} failed: {result:?}");
                finished += 1;
            }
            Err(_) => break,
        }
    }
    assert_eq!(finished, blockers, "every blocking stand-in answers once it is released");

    let (cmd, result) = answer.expect("the save answered at all");
    assert_eq!(cmd, "stand_in_save");
    assert_eq!(result, Ok(serde_json::json!("fable")));
    assert_eq!(app.state::<Saved>().0.lock().unwrap().as_deref(), Some("fable"));
    eprintln!("[command-rule] {workers} workers, {blockers} blocking commands in flight; the save answered in {} ms", took.as_millis());
    assert!(
        took < SAVE_DEADLINE,
        "the save took {} ms with {blockers} blocking commands in flight on {workers} workers; it queued behind them",
        took.as_millis()
    );
}

/// Every line of `text` that is a bare Tauri command attribute, as `line: text`. A doc comment
/// or a string that mentions the attribute does not start a line with it and is not counted.
pub(crate) fn bare_tauri_attributes(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with("#[tauri::command"))
        .map(|(n, line)| format!("{}: {}", n + 1, line.trim()))
        .collect()
}

/// **The rule, for every command**: no `#[tauri::command]` written directly in the shell. A
/// command carries `#[richos_command::command]`, whose body runs off the async workers; a bare
/// Tauri attribute would put its body back on one.
#[test]
fn no_command_in_the_shell_carries_a_bare_tauri_command_attribute() {
    fn walk(dir: &std::path::Path, found: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for site in bare_tauri_attributes(&text) {
                    found.push(format!("{}:{site}", path.display()));
                }
            }
        }
    }
    // Relative: `cargo test` runs a test binary in its package's root. Checked rather than
    // assumed, so a run from anywhere else fails here instead of scanning nothing and passing.
    let src = std::path::Path::new("src");
    assert!(src.join("main.rs").is_file(), "run from src-tauri/ (cargo test does): no src/main.rs in {:?}", std::env::current_dir());
    let mut found = Vec::new();
    walk(src, &mut found);
    assert!(
        found.is_empty(),
        "{} command(s) carry a bare Tauri attribute; write #[richos_command::command] instead, so the body runs off Tauri's async workers:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The calls that send AppKit messages to a raw `NSWindow`: `dictation::appkit::Win` and its
/// methods, and the RichCleaner panel's `build_panel`, which turns the new window into an
/// `NSPanel` with `object_setClass` and `setStyleMask:`. AppKit allows them on the main thread only.
const MAIN_THREAD_ONLY: [&str; 7] =
    ["build_panel", "make_panel", "float_everywhere", "ignore_mouse", "show_key", "Win", "msg_send!"];

/// Push `ch` as a blank: a space, or the line break itself, so line numbers stay true.
fn blank(out: &mut String, ch: char) {
    out.push(if ch == '\n' { '\n' } else { ' ' });
}

/// Blank `c[from..to]` into `out`.
fn blank_span(out: &mut String, c: &[char], from: usize, to: usize) {
    for &ch in &c[from..to.min(c.len())] {
        blank(out, ch);
    }
}

/// The end (exclusive) of a quoted literal that opens at `c[at]` with `quote`, escapes skipped.
fn quoted_end(c: &[char], at: usize, quote: char) -> usize {
    let mut i = at + 1;
    while i < c.len() && c[i] != quote {
        i += if c[i] == '\\' { 2 } else { 1 };
    }
    i + 1
}

/// `text` with comments, string literals and character literals blanked, so a brace or a name
/// inside one is never read as code. Lifetimes (`'a`, no closing quote) are code.
pub(crate) fn code_only(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut i = 0;
    while i < c.len() {
        let next = c.get(i + 1).copied();
        let end = if c[i] == '/' && next == Some('/') {
            c[i..].iter().position(|&ch| ch == '\n').map_or(c.len(), |p| i + p)
        } else if c[i] == '/' && next == Some('*') {
            let (mut depth, mut j) = (0usize, i);
            while j < c.len() {
                if c[j] == '/' && c.get(j + 1) == Some(&'*') {
                    depth += 1;
                    j += 2;
                } else if c[j] == '*' && c.get(j + 1) == Some(&'/') {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            j
        } else if c[i] == 'r' && matches!(next, Some('"' | '#')) && (i == 0 || !ident(c[i - 1])) {
            // A raw string, r"…" or r#"…"#, closed by as many hashes as opened it.
            let mut j = i + 1;
            while c.get(j) == Some(&'#') {
                j += 1;
            }
            if c.get(j) == Some(&'"') {
                let hashes = j - i - 1;
                let mut k = j + 1;
                while k < c.len() && !(c[k] == '"' && (0..hashes).all(|h| c.get(k + 1 + h) == Some(&'#'))) {
                    k += 1;
                }
                k + 1 + hashes
            } else {
                i
            }
        } else if c[i] == '"' {
            quoted_end(&c, i, '"')
        } else if c[i] == '\'' && (next == Some('\\') || c.get(i + 2) == Some(&'\'')) {
            quoted_end(&c, i, '\'')
        } else {
            i
        };
        if end > i {
            blank_span(&mut out, &c, i, end);
            i = end;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}

/// Every `#[richos_command::command]` function in `text`, as `(its name, its body)`, read from
/// [`code_only`] text so braces inside strings and comments do not move a body's end.
pub(crate) fn command_bodies(text: &str) -> Vec<(String, String)> {
    let code = code_only(text);
    let mut bodies = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("#[richos_command::command") {
        let start = from + at;
        from = start + 1;
        let Some(fn_at) = code[start..].find("fn ").map(|p| start + p) else { break };
        let name: String = code[fn_at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let Some(open) = code[fn_at..].find('{').map(|p| fn_at + p) else { break };
        let mut depth = 0usize;
        let mut close = None;
        for (offset, ch) in code[open..].char_indices() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
        }
        let Some(close) = close else { break };
        bodies.push((name, code[open..=close].to_string()));
    }
    bodies
}

/// The main-thread-only calls in `body`, each matched as a whole identifier followed by `(`
/// (or, for the macro, by itself).
pub(crate) fn main_thread_calls(body: &str) -> Vec<&'static str> {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    MAIN_THREAD_ONLY
        .into_iter()
        .filter(|name| {
            body.match_indices(name).any(|(at, _)| {
                let rest = &body[at + name.len()..];
                let whole_before = !body[..at].chars().next_back().is_some_and(ident);
                let called = name.ends_with('!') || rest.trim_start().starts_with('(') && !rest.starts_with(ident);
                whole_before && called
            })
        })
        .collect()
}

/// **No window command sends AppKit a message from its own body.** A command's body runs on a
/// tokio worker (`#[richos_command::command]` runs it under `block_in_place`), never on the main
/// thread, and AppKit's window calls are main-thread only. `cleaner_set_enabled` once called
/// `build_panel` from its body, so the new panel's `object_setClass` and `setStyleMask:` ran on
/// the worker while the main thread was setting the panel's webview up, and pressing the
/// RichCleaner switch quit RichOS tens of milliseconds later with no panic line. A command reaches such a call through the main thread
/// (`cleaner_host::build_panel_on_main`, `dictation::bar::ask_main`).
#[test]
fn no_command_body_sends_appkit_a_message_off_the_main_thread() {
    fn walk(dir: &std::path::Path, commands: &mut usize, found: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, commands, found);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("command_rule_tests.rs") {
                for (name, body) in command_bodies(&std::fs::read_to_string(&path).unwrap()) {
                    *commands += 1;
                    let calls = main_thread_calls(&body);
                    if !calls.is_empty() {
                        found.push(format!("{}: {name} calls {}", path.display(), calls.join(", ")));
                    }
                }
            }
        }
    }
    let src = std::path::Path::new("src");
    assert!(src.join("main.rs").is_file(), "run from src-tauri/ (cargo test does): no src/main.rs in {:?}", std::env::current_dir());
    let (mut commands, mut found) = (0, Vec::new());
    walk(src, &mut commands, &mut found);
    // 218 command attributes in src/ when this was written; far fewer means the scan is blind.
    assert!(commands > 150, "the scan read only {commands} command bodies; it is not reading the shell's source");
    assert!(
        found.is_empty(),
        "{} command(s) send AppKit a message from a tokio worker; go through the main thread (cleaner_host::build_panel_on_main, dictation::bar::ask_main):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The scanner itself: braces in strings, comments and character literals do not end a body
/// early, a call inside a command is seen, and a name that only contains a forbidden one is not.
#[test]
fn the_appkit_scan_reads_bodies_as_code() {
    let text = r##"
#[richos_command::command]
pub fn one(app: AppHandle) -> Result<(), String> {
    let s = "}\"}"; let r = r#"}"#; let c = '}'; // }
    /* } /* } */ */ helper::<'static>(s, '\'');
    build_panel(&app)?;
    Ok(())
}
fn not_a_command() { build_panel(&app); }
#[richos_command::command]
pub fn two(app: AppHandle) -> Result<(), String> { build_panel_on_main(&app)?; rebuild_panel(); Ok(()) }
#[richos_command::command]
pub fn three(app: AppHandle) { let w = crate::dictation::appkit::Win(ptr); w.make_panel(true); }
"##;
    let bodies = command_bodies(text);
    let names: Vec<&str> = bodies.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["one", "two", "three"]);
    assert!(bodies[0].1.contains("Ok(())") && !bodies[0].1.contains("not_a_command"), "the body ends at its own brace: {}", bodies[0].1);
    assert_eq!(main_thread_calls(&bodies[0].1), ["build_panel"]);
    assert!(main_thread_calls(&bodies[1].1).is_empty(), "{:?}", main_thread_calls(&bodies[1].1));
    assert_eq!(main_thread_calls(&bodies[2].1), ["make_panel", "Win"]);
}
