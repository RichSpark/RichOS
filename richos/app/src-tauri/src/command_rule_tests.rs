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
