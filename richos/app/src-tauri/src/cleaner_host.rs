//! RichCleaner's desktop owner. One active capture/filing operation and one personal catalog.
use crate::{
    cleaner_native::Native,
    dictation::ipc::{AppMessage, ToolMessage},
    AppState,
};
use richos_core::cleaner::{self, choice::Prepared, Catalog, Entry, Files, Identity, Phase};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager};
mod batch;
const EVENT: &str = "rich://cleaner";
const PANEL: &str = "cleaner-panel";
struct Active {
    id: String,
    target: Option<Value>,
    entry: Option<String>,
    batch: Option<batch::ActiveBatch>,
    stage: String,
    cancel: Arc<AtomicBool>,
    chooser: Option<Arc<Prepared>>,
}
pub struct Host {
    active: Mutex<Option<Active>>,
    view: Mutex<Value>,
    mutation: Mutex<()>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
            view: Mutex::new(json!({"stage":"idle"})),
            mutation: Mutex::new(()),
        }
    }
}
fn catalog(app: &AppHandle) -> Result<Catalog, String> {
    Catalog::open(
        &app.state::<AppState>()
            .data_dir
            .join("RichCleaner/library.sqlite"),
    )
}
fn settings(app: &AppHandle) -> Result<(bool, Option<PathBuf>), String> {
    let state = app.state::<AppState>();
    let config = state
        .config
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    let (on, root) = config.cleaner();
    Ok((on, root.map(PathBuf::from)))
}
fn link(app: &AppHandle, id: &str, action: &str, payload: Value) -> Result<(), String> {
    let link = app
        .try_state::<Arc<crate::dictation_app::Link>>()
        .ok_or("RichScribe is starting. Try again when it is ready")?;
    if !link.send(&AppMessage::Cleaner {
        id: id.into(),
        action: action.into(),
        payload,
    }) {
        return Err("RichScribe is not connected. Open its settings".into());
    }
    Ok(())
}
fn host(app: &AppHandle) -> Arc<Host> {
    app.state::<Arc<Host>>().inner().clone()
}
fn show(app: &AppHandle, mut value: Value) {
    let h = host(app);
    let previous = h.view.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let fast = app
        .state::<AppState>()
        .config
        .lock()
        .map(|c| c.cleaner_inbox_first())
        .unwrap_or(false);
    let same = previous["id"] == value["id"];
    let quiet = value["force"] != true
        && fast
        && value["noteSaved"] == true
        && (matches!(value["stage"].as_str(), Some("choosing" | "filing"))
            || same && previous["quiet"] == true);
    value["quiet"] = json!(quiet);
    *h.view.lock().unwrap_or_else(|p| p.into_inner()) = value.clone();
    if let Err(e) = app.emit(EVENT, &value) {
        eprintln!("[richos] RichCleaner state: {e}");
    }
    let app2 = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        if app2.get_webview_window("main").is_none() {
            return;
        }
        if let Err(e) = build_panel(&app2) {
            eprintln!("[richos] RichCleaner panel: {e}");
            return;
        }
        if let Some(window) = app2.get_webview_window(PANEL) {
            if let Ok(ns) = window.ns_window() {
                let win = crate::dictation::appkit::Win(ns as *mut _);
                if quiet {
                    win.hide();
                } else {
                    win.show();
                }
            }
        }
    }) {
        eprintln!("[richos] RichCleaner panel: {e}");
    }
}
fn stage(app: &AppHandle, id: &str, name: &str, extra: Value) {
    let h = host(app);
    let active = h.active.lock().unwrap_or_else(|p| p.into_inner());
    if id != "recovery" && active.as_ref().is_none_or(|a| a.id != id) {
        return;
    }
    let mut value = h.view.lock().unwrap_or_else(|p| p.into_inner()).clone();
    value["id"] = json!(id);
    value["stage"] = json!(name);
    if let Some(fields) = extra.as_object() {
        for (k, v) in fields {
            value[k] = v.clone();
        }
    }
    show(app, value);
}
fn finish(app: &AppHandle, id: &str, value: Value) {
    let h = host(app);
    let mut active = h.active.lock().unwrap_or_else(|p| p.into_inner());
    let Some(done) = active.take_if(|op| op.id == id) else {
        return;
    };
    done.cancel.store(true, Ordering::Release);
    if let Some(chooser) = &done.chooser {
        chooser.cancel();
    }
    if let Err(e) = link(app, id, "release", json!({})) {
        eprintln!("[richos] RichCleaner release: {e}");
    }
    show(app, value);
    drop(active);
}
fn error(app: &AppHandle, id: &str, message: String) {
    let h = host(app);
    let value = h.view.lock().unwrap_or_else(|p| p.into_inner()).clone();
    finish(
        app,
        id,
        json!({"id":id,"stage":"error","message":message,"file":value["file"],
            "noteSaved":value["noteSaved"],"entry":value["entry"],"batch":value["batch"],
            "batchNote":value["batchNote"],"force":value["batch"].is_object()}),
    );
}
fn located(app: &AppHandle, entry: &mut Entry, path: PathBuf) {
    if entry.path != path {
        if let Some(parent) = path.parent() {
            entry.folder = settings(app)
                .ok()
                .and_then(|(_, root)| root)
                .and_then(|root| {
                    parent
                        .strip_prefix(root)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| parent.to_string_lossy().to_string());
            entry.inbox = entry.folder == "Inbox";
        }
        entry.path = path;
    }
}
fn profile(
    app: &AppHandle,
) -> Result<(PathBuf, richos_core::engine_profile::EngineProfile), String> {
    let state = app.state::<AppState>();
    let engine = state
        .engine_dir
        .lock()
        .map_err(|_| "The model connection is unavailable")?
        .clone();
    let runtime = richos_core::runtime::verify_engine(&engine).map_err(|e| e.to_string())?;
    let mut profile =
        richos_core::engine_profile::EngineProfile::prepare(&engine, &state.data_dir, runtime)
            .map_err(|e| e.0)?;
    profile.claude_account = Some(state.quota.lease_account());
    profile.token_sink = Some(state.quota.token_sink());
    let bin = state
        .claude_bin
        .lock()
        .map_err(|_| "The model is unavailable")?
        .clone();
    Ok((bin, profile))
}
fn folders(root: &Path) -> Result<Vec<String>, String> {
    let mut labels = vec![];
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if cleaner::folder_parts(&name).is_err() {
            continue;
        }
        labels.push(name.clone());
        for sub in std::fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
            let sub = sub.map_err(|e| e.to_string())?;
            if sub.file_type().map_err(|e| e.to_string())?.is_dir() {
                let label = format!("{name}/{}", sub.file_name().to_string_lossy());
                if cleaner::folder_parts(&label).is_ok() {
                    labels.push(label);
                }
            }
            if labels.len() > 1000 {
                return Err("There are too many folder labels to classify safely".into());
            }
        }
    }
    Ok(labels)
}
pub fn start(app: &AppHandle) -> Result<(), String> {
    crate::dictation::log::init(&app.state::<AppState>().data_dir);
    app.manage(Arc::new(Host::default()));
    if settings(app)?.0 {
        build_panel(app)?;
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = Native::bundled().and_then(|native| {
                catalog(&app).and_then(|mut db| recover_idle(&app, &mut db, &native))
            }) {
                stage(
                    &app,
                    "recovery",
                    "error",
                    json!({"message":format!("The library needs attention: {e}")}),
                );
            }
        });
    }
    Ok(())
}
fn recover_idle(app: &AppHandle, db: &mut Catalog, native: &Native) -> Result<(), String> {
    let h = host(app);
    let _mutation = h.mutation.lock().map_err(|_| "File operations unavailable")?;
    let active = h.active.lock().map_err(|_| "Operation state unavailable")?;
    if active.is_none() {
        cleaner::recover(db, native)?;
    }
    Ok(())
}
/// [`build_panel`] from a window command. A command's body runs on a tokio worker
/// (`#[richos_command::command]`), and `build_panel`'s AppKit calls (`make_panel`'s
/// `object_setClass` and `setStyleMask:`) are main-thread only: made from the worker, they raced
/// the main thread setting the new webview up, and pressing the RichCleaner switch quit RichOS
/// with no panic line. So the build runs on the main thread and is waited for.
/// If the main thread does not answer in time the panel is still built there when it gets to
/// it, and `show` builds it on demand in any case, so enabling does not fail for that.
fn build_panel_on_main(app: &AppHandle) -> Result<(), String> {
    let handle = app.clone();
    match crate::dictation::bar::ask_main(app, move || build_panel(&handle)) {
        Some(built) => built,
        None => {
            eprintln!("[richos] RichCleaner panel: the main thread did not answer in time; it is built when first shown");
            Ok(())
        }
    }
}
/// Main thread only (AppKit): from `start` in setup, from `show`'s main-thread closure, and from
/// a command only through [`build_panel_on_main`].
fn build_panel(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(PANEL).is_some() {
        return Ok(());
    }
    let panel = tauri::WebviewWindowBuilder::new(
        app,
        PANEL,
        tauri::WebviewUrl::App("cleaner.html?panel=1".into()),
    )
    .title("RichCleaner")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .visible(false)
    .focused(false)
    .focusable(true)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .accept_first_mouse(true)
    .inner_size(650., 340.)
    .build()
    .map_err(|e| e.to_string())?;
    let ns = crate::dictation::appkit::Win(panel.ns_window().map_err(|e| e.to_string())? as *mut _);
    ns.make_panel(true)?;
    ns.float_everywhere();
    // Position with the existing dictation screen convention. No window activation.
    if let Some(monitor) = panel.current_monitor().map_err(|e| e.to_string())? {
        let scale = monitor.scale_factor();
        let size = monitor.size();
        let pos = monitor.position();
        panel
            .set_position(tauri::LogicalPosition::new(
                pos.x as f64 / scale + (size.width as f64 / scale - 650.) / 2.,
                pos.y as f64 / scale + size.height as f64 / scale - 430.,
            ))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
/// Hidden feature panels must not keep the application alive after its main window closes.
/// Ordinary registered-work policy remains the application's own ExitRequested decision.
pub fn watch_main(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
            if let Some(h) = app.try_state::<Arc<Host>>() {
                if let Ok(active) = h.active.lock() {
                    if let Some(a) = active.as_ref() {
                        if a.stage != "filing" {
                            a.cancel.store(true, Ordering::Release);
                            if let Some(chooser) = &a.chooser {
                                chooser.cancel();
                            }
                            if let Err(e) = link(&app, &a.id, "cancel", json!({})) {
                                eprintln!("[richos] RichCleaner close: {e}");
                            }
                        }
                    }
                }
            }
            for label in [PANEL, "cleaner"] {
                if let Some(feature) = app.get_webview_window(label) {
                    if let Err(e) = feature.destroy() {
                        eprintln!("[richos] RichCleaner window close: {e}");
                    }
                }
            }
        }
    });
}
pub fn on_message(app: &AppHandle, message: Option<&ToolMessage>) {
    let Some(message) = message else { return };
    if matches!(message, ToolMessage::State { .. }) {
        if let Ok((enabled, _)) = settings(app) {
            if let Err(e) = link(app, "settings", "mode", json!({"enabled":enabled})) {
                eprintln!("[richos] RichCleaner mode: {e}");
            }
        }
        return;
    }
    let ToolMessage::Cleaner {
        id,
        action,
        payload,
    } = message
    else {
        return;
    };
    match action.as_str() {
        "passthrough" => {
            let h = host(app);
            let mut active = h.active.lock().unwrap_or_else(|p| p.into_inner());
            if active.as_ref().is_some_and(|a| a.id == *id) {
                active.take();
                drop(active);
                *h.view.lock().unwrap_or_else(|p| p.into_inner()) = json!({"stage":"idle"});
                if let Some(window) = app.get_webview_window(PANEL) {
                    if let Err(e) = window.hide() {
                        eprintln!("[richos] RichCleaner panel: {e}");
                    }
                }
            }
        }
        "checking" => {
            let h = host(app);
            let mut active = h.active.lock().unwrap_or_else(|p| p.into_inner());
            if active.is_some() {
                drop(active);
                let mut busy = h.view.lock().unwrap_or_else(|p| p.into_inner()).clone();
                busy["stage"] = json!("busy");
                busy["force"] = json!(true);
                busy["message"]=json!("Still sorting the previous file. Open Recent files for its location or Undo, then try again");
                show(app, busy);
                if let Err(e) = link(
                    app,
                    id,
                    "cancel",
                    json!({"message":"RichCleaner is still sorting the previous file. Try again when it finishes"}),
                ) {
                    eprintln!("[richos] RichCleaner busy: {e}");
                }
                return;
            }
            *active = Some(Active {
                id: id.clone(),
                target: None,
                entry: None,
                batch: None,
                stage: "checking".into(),
                cancel: Arc::new(AtomicBool::new(false)),
                chooser: None,
            });
            drop(active);
            show(
                app,
                json!({"id":id,"stage":"checking","key":payload["key"],"noteSaved":false}),
            );
        }
        "target" => {
            if !target_is_pending(&host(app), id) {
                return;
            }
            let app = app.clone();
            let id = id.clone();
            let payload = payload.clone();
            std::thread::spawn(move || {
                if let Err(e) = begin_target(&app, &id, payload) {
                    error(&app, &id, e);
                }
            });
        }
        "listening" | "writing" => {
            let h = host(app);
            let mut active = h.active.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(a) = active.as_mut().filter(|a| a.id == *id && !a.cancel.load(Ordering::Acquire)) {
                a.stage = action.clone();
                drop(active);
                stage(
                    app,
                    id,
                    action,
                    json!({"key":payload["key"],"started":richos_core::util::now_millis()}),
                );
            }
        }
        "transcript" => {
            let app = app.clone();
            let id = id.clone();
            let note = payload["note"].as_str().unwrap_or_default().to_string();
            std::thread::spawn(move || {
                if let Err(e) = accept(&app, &id, note) {
                    error(&app, &id, e);
                }
            });
        }
        "level" => {
            if host(app)
                .active
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
                .is_none_or(|a| a.id != *id)
            {
                return;
            }
            if let Err(e) = app.emit_to(PANEL, "cleaner-level", payload["level"].clone()) {
                eprintln!("[richos] RichCleaner meter: {e}");
            }
        }
        "canceled" => canceled(app, id),
        "error" => error(
            app,
            id,
            payload["message"]
                .as_str()
                .unwrap_or("Capture could not start")
                .into(),
        ),
        _ => {}
    }
}
fn target_is_pending(h: &Host, id: &str) -> bool {
    h.active.lock().unwrap_or_else(|error| error.into_inner()).as_ref()
        .is_some_and(|op| op.id == id && !op.cancel.load(Ordering::Acquire))
}
fn begin_target(app: &AppHandle, id: &str, target: Value) -> Result<(), String> {
    if target.get("targets").is_some() {
        return batch::begin(app, id, target);
    }
    let native = Native::bundled()?;
    let path = PathBuf::from(target["path"].as_str().ok_or("No file was selected")?);
    let identity: Identity =
        serde_json::from_value(target["identity"].clone()).map_err(|e| e.to_string())?;
    if !identity.unchanged(&native.inspect(&path)?) {
        return Err("The selected file changed. Try again".into());
    }
    let db = catalog(app)?;
    if let Some(mut entry) = db.by_identity(identity.volume, identity.object)? {
        if matches!(entry.phase, Phase::Draft | Phase::Filed) {
            entry.path = path.clone();
            if path.parent()
                == Some(
                    PathBuf::from(
                        native.request(json!({"action":"directories"}))?["desktop"]
                            .as_str()
                            .ok_or("Desktop unavailable")?,
                    )
                    .as_path(),
                )
            {
                entry.phase = Phase::Draft;
                entry.folder.clear();
                entry.inbox = false;
                entry.pending_sort = false;
            }
            let mut db = db;
            db.save(&entry)?;
            finish(
                app,
                id,
                json!({"id":id,"stage":if entry.phase==Phase::Filed{"filed"}else{"draft"},"entry":entry,"noteSaved":true,"file":{"path":entry.path,"name":entry.path.file_name().map(|v|v.to_string_lossy())}}),
            );
            return Ok(());
        }
        return Err("This file has an operation to recover first".into());
    }
    start_target(app, id, target, json!({"file":{"name":path.file_name().map(|v|v.to_string_lossy()),"path":path}}))
}
fn start_target(app: &AppHandle, id: &str, target: Value, preview: Value) -> Result<(), String> {
    let chooser = profile(app)
        .map(|(bin, profile)| Arc::new(Prepared::start(&bin, &profile)))
        .ok();
    let h = host(app);
    let mut active = h.active.lock().map_err(|_| "The capture was canceled")?;
    let a = active
        .as_mut()
        .filter(|a| a.id == id && !a.cancel.load(Ordering::Acquire))
        .ok_or("The capture was canceled")?;
    a.target = Some(target);
    a.chooser = chooser;
    a.stage = "starting".into();
    drop(active);
    stage(app, id, "checking", preview);
    link(app, id, "record", json!({}))
}
fn accept(app: &AppHandle, id: &str, note: String) -> Result<(), String> {
    let h = host(app);
    let mut active = h.active.lock().map_err(|_| "Capture state unavailable")?;
    let Some(a) = active
        .as_mut()
        .filter(|a| a.id == id && a.stage == "writing")
    else {
        return Ok(());
    };
    if a.cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    let target = a.target.clone().ok_or("The recording has no target")?;
    if target.get("targets").is_some() {
        drop(active);
        return batch::accept(app, id, &target, note);
    }
    let path = PathBuf::from(target["path"].as_str().ok_or("Missing target")?);
    let identity = serde_json::from_value(target["identity"].clone()).map_err(|e| e.to_string())?;
    let mut db = catalog(app)?;
    let entry = Entry::new(path, identity, note)?;
    db.save(&entry)?;
    a.entry = Some(entry.id.clone());
    a.stage = "choosing".into();
    let chooser = a.chooser.clone();
    let cancel = a.cancel.clone();
    drop(active);
    stage(app, id, "saved", json!({"entry":entry,"noteSaved":true}));
    let fast = app
        .state::<AppState>()
        .config
        .lock()
        .map_err(|_| "Settings unavailable")?
        .cleaner_inbox_first();
    if fast {
        let (_, root) = settings(app)?;
        let root = root.ok_or("Choose a filing folder")?;
        let _mutation = h
            .mutation
            .lock()
            .map_err(|_| "File operations unavailable")?;
        if !cancel.load(Ordering::Acquire) {
            let native = Native::bundled()?;
            let stored = match cleaner::store_inbox(&mut db, &native, &entry.id, &root) {
                Ok(stored) => stored,
                Err(error) => {
                    cleaner::recover(&mut db, &native)?;
                    let current = db.get(&entry.id)?;
                    finish(
                        app,
                        id,
                        json!({"id":id,"stage":"error","message":error,"entry":current,"noteSaved":true}),
                    );
                    return Ok(());
                }
            };
            stage(
                app,
                id,
                "inbox-pending",
                json!({"entry":stored,"noteSaved":true}),
            );
        }
    }
    file_saved(app, id, &entry.id, chooser, cancel)
}
// Caller holds the mutation lock, so Undo and an edited note cannot be overwritten
// by the late completion of a canceled folder request.
fn settle_canceled(
    app: &AppHandle,
    id: &str,
    db: &mut Catalog,
    entry_id: &str,
) -> Result<(), String> {
    let entry = cleaner::cancel_sort(db, entry_id)?;
    finish(
        app,
        id,
        json!({"id":id,"stage":if entry.phase==Phase::Filed{"inbox"}else{"draft"},"entry":entry,"noteSaved":true}),
    );
    Ok(())
}
fn file_saved(
    app: &AppHandle,
    id: &str,
    entry_id: &str,
    chooser: Option<Arc<Prepared>>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    stage(app, id, "choosing", json!({"noteSaved":true}));
    let (_, root) = settings(app)?;
    let root = root.ok_or("Choose your filing folder in RichCleaner settings")?;
    let mut db = catalog(app)?;
    let mut entry = db.get(entry_id)?;
    if !entry.pending_sort {
        entry.phase = Phase::Choosing;
        db.save(&entry)?;
    }
    let root_check = Native::bundled()?.request(json!({"action":"validate-root","path":root}));
    let choice = root_check.and_then(|_| folders(&root)).and_then(|labels| {
        chooser
            .as_ref()
            .ok_or("The model connection is unavailable".to_string())?
            .choose(
                &entry.path.file_name().unwrap_or_default().to_string_lossy(),
                &entry.note,
                &labels,
            )
    });
    let h = host(app);
    let _mutation = h
        .mutation
        .lock()
        .map_err(|_| "File operations are unavailable")?;
    if cancel.load(Ordering::Acquire) {
        return settle_canceled(app, id, &mut db, entry_id);
    }
    let (folder, labels, inbox, reason) = folder_outcome(choice);
    {
        let mut active = h.active.lock().map_err(|_| "Operation state unavailable")?;
        let a = active
            .as_mut()
            .filter(|a| a.id == id)
            .ok_or("The operation was canceled")?;
        if a.cancel.load(Ordering::Acquire) {
            drop(active);
            return settle_canceled(app, id, &mut db, entry_id);
        }
        a.stage = "filing".into();
    }
    stage(app, id, "filing", json!({"folder":folder,"noteSaved":true}));
    let native = Native::bundled()?;
    match cleaner::file(&mut db, &native, entry_id, &root, &folder, labels, inbox) {
        Ok(entry) => finish(
            app,
            id,
            json!({"id":id,"stage":if entry.inbox{"inbox"}else{"filed"},"entry":entry,"noteSaved":true,"message":reason}),
        ),
        Err(error) => {
            cleaner::recover(&mut db, &native)?;
            let entry = db.get(entry_id)?;
            finish(
                app,
                id,
                json!({"id":id,"stage":"error","message":error,"entry":entry,"noteSaved":true}),
            );
        }
    }
    Ok(())
}
fn folder_outcome(
    choice: Result<cleaner::choice::Choice, String>,
) -> (String, Vec<String>, bool, String) {
    match choice {
        Ok(choice) if choice.clear && choice.folder != "Inbox" => {
            let mut labels = choice.keywords;
            labels.extend(choice.aliases);
            (choice.folder, labels, false, String::new())
        }
        Ok(_) => (
            "Inbox".into(),
            vec![],
            true,
            "The note did not make the right folder clear".into(),
        ),
        Err(reason) => ("Inbox".into(), vec![], true, reason),
    }
}
fn canceled(app: &AppHandle, id: &str) {
    let h = host(app);
    let (entry, chooser) = {
        let mut active = h.active.lock().unwrap_or_else(|p| p.into_inner());
        let Some(a) = active.as_mut().filter(|a| a.id == id) else {
            return;
        };
        if a.stage == "filing" {
            return;
        }
        a.cancel.store(true, Ordering::Release);
        (a.entry.is_some() || a.batch.is_some(), a.chooser.clone())
    };
    if let Some(chooser) = chooser {
        chooser.cancel();
    }
    // The choice worker owns settling an accepted draft. Before acceptance nothing was saved.
    if !entry {
        finish(
            app,
            id,
            json!({"id":id,"stage":"canceled","message":"Nothing was saved. The file was left in place","noteSaved":false}),
        );
    }
}
#[richos_command::command]
pub fn cleaner_status(app: AppHandle) -> Result<Value, String> {
    let (enabled, root) = settings(&app)?;
    let state = app.state::<AppState>();
    let look = crate::dictation::bar::look(&state.data_dir);
    let scribe = crate::dictation::store::read(&state.data_dir)?;
    Ok(
        json!({"enabled":enabled,"root":root,"scribeOn":scribe.on,"key":scribe.key(),"theme":look.theme,"fontScale":look.font_scale,"inboxFirst":state.config.lock().map_err(|_|"Settings unavailable")?.cleaner_inbox_first(),"operation":*host(&app).view.lock().map_err(|_|"Operation state unavailable")?}),
    )
}
#[richos_command::command]
pub fn cleaner_set_enabled(app: AppHandle, enabled: bool) -> Result<Value, String> {
    let (_, existing) = settings(&app)?;
    let state = app.state::<AppState>();
    let scribe = crate::dictation::store::read(&state.data_dir)?;
    if enabled && !scribe.on {
        return Err("Turn on RichScribe first".into());
    }
    if enabled {
        build_panel_on_main(&app)?;
    }
    if !enabled {
        let h = host(&app);
        let active = h.active.lock().map_err(|_| "Capture state unavailable")?;
        if let Some(a) = active.as_ref().filter(|a| a.stage != "filing") {
            a.cancel.store(true, Ordering::Release);
            if let Some(chooser) = &a.chooser {
                chooser.cancel();
            }
            link(&app, &a.id, "cancel", json!({}))?;
        }
    }
    let root = if enabled {
        let native = Native::bundled()?;
        let directories = native.request(json!({"action":"directories"}))?;
        let root = existing.unwrap_or_else(|| {
            PathBuf::from(directories["documents"].as_str().unwrap_or_default()).join("RichCleaner")
        });
        native.request(json!({"action":"root","path":root}))?;
        let mut db = catalog(&app)?;
        recover_idle(&app, &mut db, &native)?;
        Some(root.to_string_lossy().to_string())
    } else {
        None
    };
    state
        .config
        .lock()
        .map_err(|_| "Settings unavailable")?
        .set_cleaner(enabled, root)
        .map_err(|e| e.to_string())?;
    link(&app, "settings", "mode", json!({"enabled":enabled}))?;
    cleaner_status(app)
}
#[richos_command::command]
pub fn cleaner_choose_root(app: AppHandle) -> Result<Value, String> {
    use tauri_plugin_dialog::DialogExt;
    let chosen = app
        .dialog()
        .file()
        .set_title("Where RichCleaner files your originals")
        .blocking_pick_folder();
    if let Some(path) = chosen.and_then(|p| p.into_path().ok()) {
        Native::bundled()?.request(json!({"action":"root","path":path}))?;
        let (on, _) = settings(&app)?;
        app.state::<AppState>()
            .config
            .lock()
            .map_err(|_| "Settings unavailable")?
            .set_cleaner(on, Some(path.to_string_lossy().to_string()))
            .map_err(|e| e.to_string())?;
    }
    cleaner_status(app)
}
#[richos_command::command]
pub fn cleaner_list(app: AppHandle, query: String, offset: u64) -> Result<Value, String> {
    let page = catalog(&app)?.search(&query, offset, 512 * 1024)?;
    serde_json::to_value(page).map_err(|e| e.to_string())
}
#[richos_command::command]
pub fn cleaner_open_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("cleaner") {
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "cleaner",
        tauri::WebviewUrl::App("cleaner.html".into()),
    )
    .title("RichCleaner")
    .inner_size(900., 580.)
    .min_inner_size(650., 480.)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[richos_command::command]
pub fn cleaner_capture_control(app: AppHandle, id: String, action: String) -> Result<(), String> {
    if action == "cancel" {
        let h = host(&app);
        let active = h.active.lock().map_err(|_| "Capture state unavailable")?;
        let Some(a) = active.as_ref().filter(|a| a.id == id) else {
            return Ok(());
        };
        if a.stage == "filing" {
            return Ok(());
        }
        a.cancel.store(true, Ordering::Release);
        if let Some(chooser) = &a.chooser {
            chooser.cancel();
        }
        let note_accepted = a.entry.is_some() || a.batch.is_some();
        drop(active);
        if !note_accepted {
            if let Err(error) = link(&app, &id, "cancel", json!({})) {
                eprintln!("[richos] RichCleaner cancel before note acceptance: {error}");
            }
            canceled(&app, &id);
        }
        return Ok(());
    }
    match action.as_str() {
        "finish" => link(&app, &id, &action, json!({})),
        _ => Err("Unknown capture action".into()),
    }
}
#[richos_command::command]
pub fn cleaner_action(
    app: AppHandle,
    id: String,
    action: String,
    note: Option<String>,
) -> Result<Value, String> {
    let native = Native::bundled()?;
    let h = host(&app);
    let _guard = h
        .mutation
        .lock()
        .map_err(|_| "File operations unavailable")?;
    let mut db = catalog(&app)?;
    let mut entry = db.get(&id)?;
    match action.as_str() {
        "open" | "reveal" => {
            let path = native.resolve(&entry.path, &entry.identity)?;
            let actual = native.inspect(&path)?;
            if !entry.identity.same_object(&actual) {
                return Err("Locate the original file".into());
            }
            located(&app, &mut entry, path);
            db.save(&entry)?;
            let mut command = std::process::Command::new("/usr/bin/open");
            if action == "reveal" {
                command.arg("-R");
            }
            command.arg(&entry.path);
            if !command.status().map_err(|e| e.to_string())?.success() {
                return Err("macOS could not open the file".into());
            }
        }
        "locate" => {
            use tauri_plugin_dialog::DialogExt;
            if let Some(path) = app
                .dialog()
                .file()
                .set_title("Locate the original file")
                .blocking_pick_file()
                .and_then(|p| p.into_path().ok())
            {
                let actual = native.inspect(&path)?;
                if !entry.identity.same_object(&actual) {
                    return Err("This is a different file. Select the original; its saved note has not changed".into());
                }
                located(&app, &mut entry, path);
                entry.identity = actual;
                entry.problem = None;
                db.save(&entry)?;
                if batch::skip(&h, &id) {
                    entry = cleaner::cancel_sort(&mut db, &id)?;
                }
            }
        }
        "undo" => {
            let mut active = h.active.lock().map_err(|_| "Operation state unavailable")?;
            if let Some(a) = active
                .as_mut()
                .filter(|a| a.entry.as_deref() == Some(id.as_str()))
            {
                a.cancel.store(true, Ordering::Release);
                if let Some(chooser) = &a.chooser {
                    chooser.cancel();
                }
            }
            drop(active);
            entry = cleaner::undo(&mut db, &native, &id)?;
            batch::skip(&h, &id);
        }
        "edit" => {
            {
                let active = h.active.lock().map_err(|_| "Operation state unavailable")?;
                if let Some(a) = active
                    .as_ref()
                    .filter(|a| a.entry.as_deref() == Some(id.as_str()))
                {
                    a.cancel.store(true, Ordering::Release);
                    if let Some(chooser) = &a.chooser {
                        chooser.cancel();
                    }
                    cleaner::cancel_sort(&mut db, &id)?;
                }
            }
            entry = cleaner::edit(&mut db, &native, &id, note.ok_or("Missing note")?)?;
            if batch::skip(&h, &id) {
                entry = cleaner::cancel_sort(&mut db, &id)?;
            }
        }
        "remove" => {
            if h.active
                .lock()
                .map_err(|_| "Operation state unavailable")?
                .as_ref()
                .is_some_and(|a| a.entry.as_deref() == Some(id.as_str()) || a.batch.as_ref().is_some_and(|batch| batch.contains(&id)))
            {
                return Err("Finish or undo this filing before removing its saved note".into());
            }
            db.remove(&id)?;
            return Ok(json!({"removed":true}));
        }
        "file-again" => {
            let mut active = h.active.lock().map_err(|_| "Capture state unavailable")?;
            if active.is_some() {
                return Err("Finish the current RichCleaner operation first".into());
            }
            let resolved = native
                .resolve(&entry.path, &entry.identity)
                .or_else(|error| {
                    if let Some(staged) = entry
                        .staged
                        .as_ref()
                        .filter(|s| !s.path.exists() && s.projection.mode == "embedded")
                    {
                        native.resolve(&entry.path, &staged.identity)
                    } else {
                        Err(error)
                    }
                })?;
            let actual = native.inspect(&resolved)?;
            let replaced = entry
                .staged
                .as_ref()
                .is_some_and(|s| !s.path.exists() && s.identity.same_object(&actual));
            if !entry.identity.same_object(&actual) && !replaced {
                return Err("Locate the original file".into());
            }
            if entry.phase == Phase::Unresolved {
                if let Some(staged) = entry.staged.take() {
                    native.cleanup(&staged)?;
                    if replaced {
                        entry.projection = staged.projection;
                    }
                }
                entry.intended = None;
                entry.intended_folder = None;
                entry.return_phase = None;
                entry.pending_sort = false;
                entry.phase = if entry.undo.is_some() {
                    Phase::Filed
                } else {
                    Phase::Draft
                };
            }
            located(&app, &mut entry, resolved);
            entry.identity = actual;
            db.save(&entry)?;
            let request = cleaner::capture_id();
            let cancel = Arc::new(AtomicBool::new(false));
            let chooser = profile(&app)
                .map(|(bin, p)| Arc::new(Prepared::start(&bin, &p)))
                .ok();
            *active = Some(Active {
                id: request.clone(),
                target: None,
                entry: Some(id.clone()),
                batch: None,
                stage: "choosing".into(),
                cancel: cancel.clone(),
                chooser: chooser.clone(),
            });
            drop(active);
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = file_saved(&app, &request, &id, chooser, cancel) {
                    error(&app, &request, e);
                }
            });
            return Ok(json!({"started":true}));
        }
        _ => return Err("Unknown file action".into()),
    }
    app.emit(EVENT, json!({"stage":"library-changed"}))
        .map_err(|e| e.to_string())?;
    serde_json::to_value(entry).map_err(|e| e.to_string())
}

#[richos_command::command]
pub fn cleaner_set_inbox_first(app: AppHandle, on: bool) -> Result<Value, String> {
    app.state::<AppState>()
        .config
        .lock()
        .map_err(|_| "Settings unavailable")?
        .set_cleaner_inbox_first(on)
        .map_err(|e| e.to_string())?;
    cleaner_status(app)
}
#[richos_command::command]
pub fn cleaner_panel_size(app: AppHandle, w: f64, h: f64) -> Result<(), String> {
    if !w.is_finite() || !h.is_finite() {
        return Err("Invalid panel dimensions".into());
    }
    let panel = app
        .get_webview_window(PANEL)
        .ok_or("Capture panel unavailable")?;
    if let Some(monitor) = panel.current_monitor().map_err(|e| e.to_string())? {
        let scale = monitor.scale_factor();
        let size = monitor.size();
        let pos = monitor.position();
        let width = w.clamp(360., (size.width as f64 / scale - 40.).min(900.));
        let height = h.clamp(100., size.height as f64 / scale - 140.);
        panel
            .set_size(tauri::LogicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        panel
            .set_position(tauri::LogicalPosition::new(
                pos.x as f64 / scale + (size.width as f64 / scale - width) / 2.,
                pos.y as f64 / scale + size.height as f64 / scale - height - 100.,
            ))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[richos_command::command]
pub fn cleaner_details(app: AppHandle, id: String) -> Result<Value, String> {
    let h = host(&app);
    let _mutation = h.mutation.lock().map_err(|_| "File operations unavailable")?;
    let mut db = catalog(&app)?;
    let mut e = db.get(&id)?;
    let native = Native::bundled()?;
    match native.resolve(&e.path, &e.identity) {
        Ok(path) => {
            if path != e.path {
                located(&app, &mut e, path);
                db.save(&e)?;
            }
            Ok(json!({"entry":e,"available":true}))
        }
        Err(message) => Ok(json!({"entry":e,"available":false,"message":message})),
    }
}
/// Explicit URL entry, also available when the global key cannot be watched.
pub fn service(app: AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    if !settings(&app)?.0 {
        return Err("Turn on RichCleaner in RichOS Settings first".into());
    }
    let scribe = crate::dictation::store::read(&app.state::<AppState>().data_dir)?;
    if !scribe.on {
        return Err("Turn on RichScribe first".into());
    }
    let id = cleaner::capture_id();
    on_message(
        &app,
        Some(&ToolMessage::Cleaner {
            id: id.clone(),
            action: "checking".into(),
            payload: json!({"key":scribe.key()}),
        }),
    );
    if host(&app)
        .active
        .lock()
        .map_err(|_| "Capture state unavailable")?
        .as_ref()
        .is_none_or(|a| a.id != id)
    {
        return Err("Finish the current RichCleaner operation first".into());
    }
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let native = Native::bundled()?;
            let target = native.request(json!({"action":"service-context","paths":paths}))?;
            let h = host(&app);
            let active = h.active.lock().map_err(|_| "Capture state unavailable")?;
            if active.as_ref().is_some_and(|op| op.id == id && !op.cancel.load(Ordering::Acquire)) {
                link(&app, &id, "service-target", target)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            error(&app, &id, e);
        }
    });
    Ok(())
}
/// Personal-library cards use the attachment renderer's visual contract, with their own
/// catalog IDs and resolver. They are not conversation attachment copies or phone downloads.
pub fn cards(
    app_data: &Path,
    entity: Option<&str>,
    thread: Option<&str>,
) -> Result<Vec<Value>, String> {
    let path = app_data.join("RichCleaner/library.sqlite");
    if !path.is_file() {
        return Ok(vec![]);
    }
    let db = Catalog::open(&path)?;
    let mut items = vec![];
    for card in db.cards()? {
        if entity.is_some_and(|e| e != card.entity) || thread.is_some_and(|t| t != card.thread) {
            continue;
        }
        let e = db.get(&card.file)?;
        use std::os::unix::fs::MetadataExt;
        let exists = std::fs::symlink_metadata(&e.path).is_ok_and(|meta| {
            meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.ino() == e.identity.object
                && meta.dev() == e.identity.volume
        });
        let name = e.path.file_name().unwrap_or_default().to_string_lossy();
        let ext = e
            .path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_uppercase();
        items.push(json!({"id":card.id,"kind":"rich_file","entityId":card.entity,"threadId":card.thread,"turnId":card.turn,"visibility":"ceo","createdAt":card.at,"sequence":card.at,"text":name,"file":{"id":e.id,"library":true,"name":name,"ext":ext,"meta":e.folder,"folder":e.folder,"note":e.note,"exists":exists}}));
    }
    Ok(items)
}

#[richos_command::command]
pub fn cleaner_open_main(app: AppHandle, surface: String) -> Result<(), String> {
    if !matches!(surface.as_str(), "cleaner" | "scribe" | "bug" | "idea") {
        return Err("Unknown Settings surface".into());
    }
    let main = app
        .get_webview_window("main")
        .ok_or("Reopen RichOS to reach Settings")?;
    main.show().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    app.emit_to(
        "main",
        "rich://cleaner-settings",
        json!({"surface":surface}),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folders_are_browsable_categories_and_never_follow_linked_repositories() {
        let root =
            std::env::temp_dir().join(format!("richcleaner-folders-{}", cleaner::capture_id()));
        std::fs::create_dir_all(root.join("Company/Board")).unwrap();
        std::fs::create_dir_all(root.join("Personal/Medical")).unwrap();
        std::fs::create_dir_all(root.join(".hidden/Secret")).unwrap();
        std::os::unix::fs::symlink(root.join("Company"), root.join("Linked repository")).unwrap();
        std::fs::write(root.join("ordinary-file.txt"), "not a folder").unwrap();
        let labels = folders(&root).unwrap();
        assert!(labels.contains(&"Company/Board".into()));
        assert!(labels.contains(&"Personal/Medical".into()));
        assert!(!labels.iter().any(|label| label.contains("Secret")
            || label.contains("Linked")
            || label.contains("ordinary-file")));
        std::fs::remove_dir_all(root).unwrap();
    }
}
