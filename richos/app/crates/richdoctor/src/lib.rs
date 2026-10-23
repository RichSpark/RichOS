//! **RICHDOCTOR, THE APP** (richos-hq/docs/prds/2026-10-22-richdoctor-v1.md (the private record); the design is
//! round 39, `design/mockups/rounds/round-39/`, built as drawn).
//!
//! A separate, signed macOS app (PRD §3) with its own executable, identifier
//! (`com.richos.richdoctor`), settings and data folder. It initializes nothing of RichOS: no
//! AppState, engine, ledger replay, voice runtime or plugin. It lives in the menu bar; the menu
//! has Open RichDoctor, Saved reports…, Start at login and Quit RichDoctor; Spotlight and
//! Applications open the same window; a second copy focuses the first ([`instance`]).
//!
//! The same executable, run as `RichDoctor --support-adapter <draft folder>`, is the read-only
//! support server Claude Code talks to ([`richdoctor_core::mcp`]); in that mode it opens no window
//! and touches nothing but that draft's evidence.
pub mod engine;
pub mod image;
pub mod instance;
#[cfg(target_os = "macos")]
pub mod mac;
pub mod relay;

use engine::{Engine, Env};
use richdoctor_core::delivery::Destination;
use richdoctor_core::store::Store;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

const TRAY: &str = "richdoctor";
const MENUBAR_ICON: &[u8] = include_bytes!("../icons/menubar@2x.png");

/// The window's one event channel.
const EVENT: &str = "doctor";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/var/empty"))
}

/// The entry point. `--support-adapter` runs the support server and returns.
pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(richdoctor_core::adapter::SERVER_FLAG) {
        if let Some(draft) = args.get(2) {
            let stdin = std::io::stdin();
            richdoctor_core::mcp::serve(std::path::Path::new(draft), stdin.lock(), std::io::stdout()).ok();
        }
        return;
    }
    // PRD §4: the CLI's DEFAULT configuration. Nothing RichOS set may reach a child, and this
    // runs before any thread exists.
    std::env::remove_var("CLAUDE_CONFIG_DIR");
    std::env::remove_var("CLAUDECODE");
    let store = Store::for_home(&home());
    // A copy already runs: it shows its window, and this one ends.
    if instance::ask(store.root(), "open").is_some() {
        return;
    }
    run(store);
}

fn run(store: Store) {
    let quiet = mac::launched_at_login(richdoctor_core::util::now_millis());
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            boot, start, answer, cancel, retry, prepare, review, edit, excerpt, save_later, send_report, try_again, delivery, saved,
            open_saved, resume_pending, delete_saved, set_login, mail_available, open_mail, copy_report, copy_address, save_report_file, open_issue,
            use_image, remove_image, images, close_help, inspect_image, prepare_image, replace_image, email_plan, note_added
        ])
        .setup(move |app| {
            app.set_activation_policy(ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let emitter = handle.clone();
            let env = Env {
                store: store.clone(),
                home: home(),
                exe: std::env::current_exe().unwrap_or_default(),
                claude: richos_core::native::resolve_claude_bin_checked().ok(),
                dest: Destination::from_env(|k| std::env::var(k).ok()),
            };
            let engine = Engine::new(env, std::sync::Arc::new(move |v: Value| {
                emitter.emit(EVENT, v).ok();
            }));
            app.manage(engine.clone());
            let ask_handle = handle.clone();
            let ask_engine = engine.clone();
            if let Err(e) = instance::listen(store.root(), move |ask| match ask {
                instance::Ask::Open => {
                    let h = ask_handle.clone();
                    ask_handle.run_on_main_thread(move || open_window(&h, None)).ok();
                    "ok"
                }
                instance::Ask::Saved => {
                    let h = ask_handle.clone();
                    ask_handle.run_on_main_thread(move || open_window(&h, Some("saved"))).ok();
                    "ok"
                }
                instance::Ask::QuitIfIdle => {
                    if ask_engine.busy() || ask_handle.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false)) {
                        "busy"
                    } else {
                        ask_engine.close();
                        let h = ask_handle.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            h.exit(0);
                        });
                        "ok"
                    }
                }
            }) {
                eprintln!("[richdoctor] {e}");
            }
            build_tray(&handle)?;
            // First launch: explain and enable start at login (PRD §3); the switch then shows
            // macOS's answer, not the request.
            let mut settings = store.settings();
            if !settings.first_run_done {
                if settings.start_at_login {
                    mac::set_login(true);
                }
                settings.first_run_done = true;
                store.save_settings(&settings).ok();
            }
            engine.expire();
            engine.interrupted_sends_not_sent();
            if !quiet {
                open_window(&handle, None);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("RichDoctor could not start");
    app.run(|app, event| match event {
        RunEvent::Reopen { .. } => open_window(app, None),
        // Closing the window leaves RichDoctor in the menu bar; only Quit ends it.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(engine) = app.try_state::<Engine>() {
                engine.close();
            }
        }
        _ => {}
    });
}

/// Show the help window (making it if needed), optionally on a screen ("saved").
fn open_window(app: &AppHandle, screen: Option<&str>) {
    app.set_activation_policy(ActivationPolicy::Regular).ok();
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.unminimize().ok();
        w.set_focus().ok();
        if let Some(s) = screen {
            app.emit(EVENT, json!({"t": "go", "screen": s})).ok();
        }
        return;
    }
    let url = match screen {
        Some(s) => format!("index.html?screen={s}"),
        None => "index.html".into(),
    };
    let built = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("RichDoctor")
        .inner_size(760.0, 760.0)
        .min_inner_size(600.0, 560.0)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        // Round 40 §1: screenshots dropped on the window reach the page as files (Tauri's own
        // drop handler would take them first and hand over only paths, which the page may not read).
        .disable_drag_drop_handler()
        .build();
    match built {
        Ok(w) => {
            let h = app.clone();
            w.on_window_event(move |e| {
                if let tauri::WindowEvent::Destroyed = e {
                    // The help session ends with its window: no model process, scan or poll is
                    // left running (PRD §3).
                    if let Some(engine) = h.try_state::<Engine>() {
                        engine.close();
                    }
                    h.set_activation_policy(ActivationPolicy::Accessory).ok();
                }
            });
            w.set_focus().ok();
        }
        Err(e) => eprintln!("[richdoctor] the window could not open: {e}"),
    }
}

fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let login = mac::login_status();
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", "Open RichDoctor", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "saved", "Saved reports…", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&CheckMenuItem::with_id(app, "login", "Start at login", true, login.on(), None::<&str>)?)?;
    if login.denied() {
        menu.append(&MenuItem::with_id(app, "login-note", "Not allowed by macOS", false, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit RichDoctor", true, None::<&str>)?)?;
    Ok(menu)
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY).menu(&menu).show_menu_on_left_click(true).tooltip("RichDoctor").icon_as_template(true);
    if let Some((rgba, w, h)) = image::rgba(MENUBAR_ICON) {
        builder = builder.icon(tauri::image::Image::new_owned(rgba, w, h));
    }
    builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => open_window(app, None),
            "saved" => open_window(app, Some("saved")),
            "login" => {
                let on = !mac::login_status().on();
                let now = mac::set_login(on);
                let store = Store::for_home(&home());
                let mut s = store.settings();
                s.start_at_login = on;
                store.save_settings(&s).ok();
                refresh_tray(app);
                app.emit(EVENT, json!({"t": "login", "on": now.on(), "denied": now.denied()})).ok();
            }
            "quit" => {
                if let Some(engine) = app.try_state::<Engine>() {
                    engine.close();
                }
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn refresh_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY), tray_menu(app)) {
        tray.set_menu(Some(menu)).ok();
    }
}

// ---------------------------------------------------------------------------------------
// COMMANDS
// ---------------------------------------------------------------------------------------

#[tauri::command]
fn boot(engine: State<Engine>) -> Value {
    let login = mac::login_status();
    let mut v = engine.first_screen();
    v["login"] = json!({"on": login.on(), "denied": login.denied()});
    v["mail"] = json!(mac::mail_available());
    v["support"] = json!(richdoctor_core::scrub::SUPPORT_ADDRESS);
    v["destination_ok"] = json!(engine.env().dest.is_ok());
    v
}

#[tauri::command]
fn start(engine: State<Engine>, description: String, no_diagnostics: bool, without_ai: bool) -> Result<(), String> {
    engine.start(description, no_diagnostics, without_ai)
}

#[tauri::command]
fn answer(engine: State<Engine>, text: String) {
    engine.answer(text)
}

#[tauri::command]
fn cancel(engine: State<Engine>) {
    engine.cancel()
}

#[tauri::command]
fn retry(engine: State<Engine>) {
    engine.retry()
}

#[tauri::command]
fn prepare(engine: State<Engine>) {
    engine.prepare()
}

#[tauri::command]
fn review(engine: State<Engine>) -> Value {
    engine.review()
}

#[tauri::command]
fn edit(engine: State<Engine>, section: String, text: String) -> Result<Value, String> {
    engine.edit(&section, &text)
}

#[tauri::command]
fn excerpt(engine: State<Engine>, id: String, removed: bool) -> Value {
    engine.set_excerpt(&id, removed)
}

#[tauri::command]
fn save_later(engine: State<Engine>) -> String {
    engine.save_later()
}

#[tauri::command]
fn send_report(engine: State<Engine>) -> Result<(), String> {
    engine.send_report()
}

#[tauri::command]
fn try_again(engine: State<Engine>) -> Result<(), String> {
    engine.try_again()
}

#[tauri::command]
fn delivery(engine: State<Engine>) -> Value {
    engine.delivery()
}

#[tauri::command]
fn saved(engine: State<Engine>) -> Value {
    engine.saved()
}

#[tauri::command]
fn open_saved(engine: State<Engine>, id: String) -> Result<Value, String> {
    engine.open(&id)
}

#[tauri::command]
fn resume_pending(engine: State<Engine>) {
    engine.resume_pending()
}

#[tauri::command]
fn delete_saved(engine: State<Engine>, id: String) -> Result<(), String> {
    engine.delete(&id)
}

#[tauri::command]
fn set_login(app: AppHandle, on: bool) -> Value {
    let now = mac::set_login(on);
    let store = Store::for_home(&home());
    let mut s = store.settings();
    s.start_at_login = on;
    store.save_settings(&s).ok();
    refresh_tray(&app);
    json!({"on": now.on(), "denied": now.denied()})
}

#[tauri::command]
fn mail_available() -> bool {
    mac::mail_available()
}

/// Email support with a mail app: a new email the person reads and sends themselves, with the
/// reviewed text and every approved screenshot as a real attachment (runs on the main thread: a
/// sync command). Above 10 MiB of screenshots it answers "parts": the window offers the saved,
/// numbered package instead (PRD §7).
#[tauri::command]
fn open_mail(engine: State<Engine>, anyway: Option<bool>) -> Result<(), String> {
    let (id, text) = engine.exact_text().ok_or("There is no reviewed report yet.")?;
    let files = engine.image_files()?;
    let sizes: Vec<u64> = files.iter().map(|(_, b)| b.len() as u64).collect();
    // Round 40 §7: above 10 MiB the sheet offers the numbered packages, and Open in Mail anyway.
    if richdoctor_core::email::needs_parts(&sizes) && !anyway.unwrap_or(false) {
        return Err("parts".into());
    }
    let dir = engine.env().store.root().join("mail").join(&id);
    std::fs::remove_dir_all(&dir).ok();
    richdoctor_core::util::private_dir(&dir).map_err(|e| e.to_string())?;
    let mut attachments = Vec::new();
    for (name, bytes) in &files {
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        attachments.push(path);
    }
    let body = format!("{}
{text}
", richdoctor_core::email::cover(&id, None, files.len()));
    if mac::compose_mail(richdoctor_core::scrub::SUPPORT_ADDRESS, &format!("RichOS problem report {id}"), &body, &attachments) {
        Ok(())
    } else {
        Err("No email app could open a new email with the screenshots attached. Save the report and screenshots instead and attach them from any email account you use.".into())
    }
}

#[tauri::command]
fn copy_report(engine: State<Engine>) -> Result<String, String> {
    let (id, text) = engine.exact_text().ok_or("There is no reviewed report yet.")?;
    if mac::copy_text(&text) { Ok(id) } else { Err("The report could not be copied.".into()) }
}

#[tauri::command]
fn copy_address() -> bool {
    mac::copy_text(richdoctor_core::scrub::SUPPORT_ADDRESS)
}

/// Save report and screenshots: the reviewed text and every approved screenshot, unchanged, in a
/// folder the person chooses (macOS's own panel); as numbered parts when they need it.
#[tauri::command]
async fn save_report_file(app: AppHandle, engine: State<'_, Engine>) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let (id, _) = engine.exact_text().ok_or("There is no reviewed report yet.")?;
    let chosen = app.dialog().file().set_title("Choose where to save the report").blocking_pick_folder();
    let Some(dir) = chosen.and_then(|p| p.into_path().ok()) else { return Err("canceled".into()) };
    let written = engine.export(&dir)?;
    Ok(if written.len() > 1 { format!("{id} in {} parts", written.len()) } else { id })
}

/// View report: only an issue link on the configured repository is ever opened.
#[tauri::command]
fn open_issue(engine: State<Engine>, url: String) -> Result<(), String> {
    let dest = engine.env().dest.clone()?;
    richdoctor_core::delivery::issue_link_ok(&url, None, &dest.repository).ok_or("not a report link")?;
    std::process::Command::new("/usr/bin/open").arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Use this image: returns the final bytes (a data URL), which are what the window shows.
#[tauri::command]
fn use_image(engine: State<Engine>, image_base64: String, covers: u32, reduced: Option<bool>) -> Result<Value, String> {
    engine.use_image(&image_base64, covers, reduced.unwrap_or(false))
}

/// The header-only check before the window decodes a file (round 40 §3).
#[tauri::command]
fn inspect_image(engine: State<Engine>, image_base64: String) -> Value {
    engine.inspect_image(&image_base64)
}

/// Check the exact image: the final bytes, made and returned, not yet kept.
#[tauri::command]
fn prepare_image(engine: State<Engine>, image_base64: String) -> Result<Value, String> {
    engine.prepare_image(&image_base64)
}

/// Edit covers or Reduce: a fresh approval of screenshot `index`.
#[tauri::command]
fn replace_image(engine: State<Engine>, index: usize, image_base64: String, covers: u32, reduced: Option<bool>) -> Result<Value, String> {
    engine.replace_image(index, &image_base64, covers, reduced.unwrap_or(false))
}

/// What Email support would carry, for its sheet.
#[tauri::command]
fn email_plan(engine: State<Engine>) -> Value {
    engine.email_plan()
}

/// The conversation's "3 screenshots added (5 areas covered)" line.
#[tauri::command]
fn note_added(engine: State<Engine>, count: u32, covers: u32) {
    engine.note_added(count, covers)
}

#[tauri::command]
fn remove_image(engine: State<Engine>, index: usize) -> Result<(), String> {
    engine.remove_image(index)
}

/// The approved screenshots' final bytes, in order.
#[tauri::command]
fn images(engine: State<Engine>) -> Vec<Value> {
    engine.images()
}

/// The window's Done on Report submitted, and closing the help session.
#[tauri::command]
fn close_help(engine: State<Engine>) {
    engine.close()
}
