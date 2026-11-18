use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{State, WebviewWindow};

#[derive(Deserialize)]
pub struct PaneBounds {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    visible: bool,
}

impl PaneBounds {
    fn valid(&self) -> bool {
        [self.left, self.top, self.width, self.height]
            .into_iter()
            .all(|coordinate| (0.0..=1.0).contains(&coordinate))
            && self.left + self.width <= 1.000_001
            && self.top + self.height <= 1.000_001
            && (!self.visible || self.width > 0.0 && self.height > 0.0)
    }
}

#[derive(Deserialize)]
pub struct RailTooltip {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    radius: f64,
    arrow: f64,
}

impl RailTooltip {
    fn valid(&self) -> bool {
        [self.left, self.top].into_iter().all(|value| (-1.0..=1.0).contains(&value))
            && [self.width, self.height, self.radius, self.arrow]
                .into_iter().all(|value| (0.0..=4.0).contains(&value))
            && self.width > 0.0
            && self.height > 0.0
    }
}

#[richos_command::command]
pub fn richfiles_pane_available(window: WebviewWindow) -> bool {
    if window.label() != "main" {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        mac::available()
    }
    #[cfg(not(target_os = "macos"))]
    false
}

#[richos_command::command]
pub async fn richfiles_place_pane(
    window: WebviewWindow,
    bounds: PaneBounds,
    tooltip: Option<RailTooltip>,
) -> Result<(), String> {
    if window.label() != "main" || !bounds.valid() {
        return Err("RichFiles could not use that window area.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        let tooltip = tooltip.filter(RailTooltip::valid);
        if bounds.visible {
            // A click can beat preload. Join the same load off the window thread.
            mac::load_in_background().await?;
        }
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |webview| {
                let result = mac::place(webview.inner(), &bounds, tooltip.as_ref());
                let _ignored = send.send(result);
            })
            .map_err(|_| "RichFiles could not reach the window.".to_owned())?;
        receive
            .await
            .map_err(|_| "The RichOS window closed.".to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    Err("RichFiles requires macOS.".to_owned())
}

/// Snapshot only the docked native view beside this webview. The canvas mixes it
/// into Make a video without asking macOS for access to the screen.
#[richos_command::command]
pub async fn richfiles_recording_frame(window: WebviewWindow) -> Result<tauri::ipc::Response, String> {
    if window.label() != "main" {
        return Err("RichFiles could not reach the window.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |webview| {
                let _ignored = send.send(mac::recording_frame(webview.inner()));
            })
            .map_err(|_| "RichFiles could not reach the window.".to_owned())?;
        receive
            .await
            .map_err(|_| "The RichOS window closed.".to_owned())?
            .map(tauri::ipc::Response::new)
    }
    #[cfg(not(target_os = "macos"))]
    Err("RichFiles requires macOS.".to_owned())
}

/// Warm the bundled library without opening a pane or putting work on AppKit's thread.
#[richos_command::command]
pub async fn richfiles_preload(window: WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("RichFiles could not reach the window.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        mac::load_in_background().await
    }
    #[cfg(not(target_os = "macos"))]
    Err("RichFiles requires macOS.".to_owned())
}

#[richos_command::command]
pub async fn output_git_open(window: WebviewWindow, bounds: PaneBounds, dark: bool,
                             output_id: Option<String>, thread_id: String, refresh: bool) -> Result<(), String> {
    if window.label() != "main" || !bounds.valid() || !bounds.visible {
        return Err("Git could not use that window area.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let app = window.app_handle().clone();
        let active_app = app.clone();
        let mode = if output_id.is_some() { 1 } else if refresh { 2 } else { 0 };
        let (scope, payload) = tauri::async_runtime::spawn_blocking(move || {
            let files = app.state::<crate::output_files::OutputFiles>();
            if files.active_chat().as_deref() != Some(&thread_id) { return Err("The active chat changed.".to_owned()); }
            let state = app.state::<crate::AppState>();
            let git = state.engine_dir.lock().ok().and_then(|root| richos_core::home_repos::home_git(&root)).map(|git| git.bin);
            let scope = files.git_context(output_id.as_deref(), git)?;
            let bytes = serde_json::to_vec(&scope).map_err(|_| "Git could not read the chat record.".to_owned())?;
            if bytes.len() > 8 * 1024 * 1024 { return Err("This chat’s Git record is too large to display.".to_owned()); }
            Ok((scope.thread_id, bytes))
        }).await.map_err(|_| "Git could not read the active chat.".to_owned())??;
        let (send, receive) = tokio::sync::oneshot::channel();
        window.with_webview(move |webview| {
            let files = active_app.state::<crate::output_files::OutputFiles>();
            let result = if files.active_chat().as_deref() == Some(scope.as_str()) {
                mac::place_git(webview.inner(), &bounds, dark, Some(payload), mode)
            } else { Err("The active chat changed.".to_owned()) };
            let _ignored = send.send(result);
        }).map_err(|_| "Git could not reach the window.".to_owned())?;
        receive.await.map_err(|_| "The RichOS window closed.".to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    Err("The Git view requires macOS.".to_owned())
}

#[richos_command::command]
pub async fn output_git_place(window: WebviewWindow, bounds: PaneBounds, dark: bool, close: bool) -> Result<(), String> {
    if window.label() != "main" || !bounds.valid() { return Err("Git could not use that window area.".to_owned()); }
    #[cfg(target_os = "macos")]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        window.with_webview(move |webview| {
            let _ignored = send.send(mac::place_git(webview.inner(), &bounds, dark, None, if close { 3 } else { 0 }));
        }).map_err(|_| "Git could not reach the window.".to_owned())?;
        receive.await.map_err(|_| "The RichOS window closed.".to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    Err("The Git view requires macOS.".to_owned())
}

#[richos_command::command]
pub fn richfiles_git_chat(window: WebviewWindow, files: State<'_, crate::output_files::OutputFiles>) {
    if window.label() != "main" { return; }
    #[cfg(target_os = "macos")]
    {
        let active = files.active_chat();
        let _ignored = window.run_on_main_thread(move || mac::git_chat_changed(active));
    }
}

pub fn active_git_chat_changed(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let app = app.clone();
        let handle = app.clone();
        let _ignored = handle.run_on_main_thread(move || {
            if let Some(files) = app.try_state::<crate::output_files::OutputFiles>() {
                mac::git_chat_changed(files.active_chat());
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[derive(Default)]
pub struct PaneRequests(Mutex<Requests>);

#[derive(Default)]
struct Requests {
    workspace: Option<String>,
    reveal: Option<PathBuf>,
}

#[richos_command::command]
pub fn richfiles_request_pending(window: WebviewWindow, request: State<'_, PaneRequests>) -> bool {
    window.label() == "main"
        && request
            .0
            .lock()
            .is_ok_and(|pending| pending.workspace.is_some() || pending.reveal.is_some())
}

#[richos_command::command]
pub fn richfiles_accept_request(
    window: WebviewWindow,
    request: State<'_, PaneRequests>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("RichFiles could not reach the window.".to_owned());
    }
    let Requests { workspace, reveal } = std::mem::take(
        &mut *request
            .0
            .lock()
            .map_err(|_| "The RichFiles request is unavailable.".to_owned())?,
    );
    #[cfg(target_os = "macos")]
    if workspace.is_some() || reveal.is_some() {
        window
            .run_on_main_thread(move || {
                if let Some(id) = workspace {
                    if let Err(error) = mac::accept(&id) {
                        eprintln!("[richos] could not deliver the RichFiles workspace: {error}");
                    }
                }
                if let Some(path) = reveal {
                    if let Err(error) = mac::reveal(&path) {
                        eprintln!(
                            "[richos] could not reveal the selected output in RichFiles: {error}"
                        );
                    }
                }
            })
            .map_err(|_| "RichFiles could not reach the window.".to_owned())?;
    }
    #[cfg(not(target_os = "macos"))]
    drop((workspace, reveal));
    Ok(())
}

#[richos_command::command]
pub fn output_reveal_in_richfiles(
    app: tauri::AppHandle,
    files: State<'_, crate::output_files::OutputFiles>,
    output_id: String,
) -> Result<String, String> {
    let path = files.path_to_reveal(&output_id)?;
    let name = path
        .file_name()
        .ok_or_else(|| "The output has no filename.".to_owned())?
        .to_string_lossy()
        .into_owned();
    reveal_output(&app, path)?;
    Ok(format!("Showing {name} in RichFiles."))
}

fn reveal_output(app: &tauri::AppHandle, path: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        if !mac::available() {
            return Err("This build does not contain RichFiles.".to_owned());
        }
        let request = app
            .try_state::<PaneRequests>()
            .ok_or_else(|| "The RichFiles window is unavailable.".to_owned())?;
        request
            .0
            .lock()
            .map_err(|_| "The RichFiles request is unavailable.".to_owned())?
            .reveal = Some(path);
        notify_pane(app);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _unused = (app, path);
        Err("RichFiles requires macOS.".to_owned())
    }
}

#[cfg(target_os = "macos")]
fn dock_id(url: &tauri::Url) -> Option<String> {
    if url.scheme() != "richos-files-workspace"
        || url.host_str() != Some("receive")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let id = url.path().strip_prefix('/')?;
    if id.len() != 36
        || !id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(id.to_ascii_uppercase())
}

#[cfg(target_os = "macos")]
pub fn opened(app: &tauri::AppHandle, urls: &[tauri::Url]) {
    use tauri::Manager;

    let Ok(data_dir) = app.path().app_data_dir() else {
        return;
    };
    let Some(support) = data_dir.parent() else {
        return;
    };
    let Some(request) = app.try_state::<PaneRequests>() else {
        return;
    };
    let folder = support.join("app.richos.richfiles/Handoff");
    let Some(id) = urls
        .iter()
        .filter_map(dock_id)
        .find(|id| folder.join(format!("{id}.json")).is_file())
    else {
        return;
    };
    {
        let Ok(mut pending) = request.0.lock() else {
            return;
        };
        if pending
            .workspace
            .as_ref()
            .is_some_and(|old| folder.join(format!("{old}.json")).is_file())
        {
            return;
        }
        pending.workspace = Some(id);
    }
    notify_pane(app);
}

#[cfg(target_os = "macos")]
fn notify_pane(app: &tauri::AppHandle) {
    use tauri::Emitter;

    let app = app.clone();
    std::thread::spawn(move || {
        let shown = crate::conversation_window(&app)
            .is_some_and(|window| window.is_visible().unwrap_or(false));
        if !shown {
            crate::reopen_window(&app);
        }
        if let Some(window) = crate::conversation_window(&app) {
            if let Err(error) = window.emit("rich://richfiles-show", ()) {
                eprintln!("[richos] could not show the RichFiles window: {error}");
            }
        }
    });
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use super::{PaneBounds, RailTooltip};

    type Place = unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, i32) -> i32;
    type SetTooltip = unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, f64, f64);
    type FrameCallback = unsafe extern "C" fn(*const u8, usize, f32, f32, *mut c_void);
    type RecordingFrame = unsafe extern "C" fn(*mut c_void, FrameCallback, *mut c_void);
    type Accept = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;
    type PlaceGit = unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, i32, i32, *const std::ffi::c_char, i32) -> i32;

    struct Bridge {
        place: Place,
        set_tooltip: Option<SetTooltip>,
        recording_frame: Option<RecordingFrame>,
        accept: Accept,
        reveal: Accept,
        place_git: PlaceGit,
        git_chat: Accept,
    }

    static BRIDGE: OnceLock<Result<Bridge, &'static str>> = OnceLock::new();

    fn library_path() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let contents = executable.parent()?.parent()?;
        (contents.file_name()? == "Contents")
            .then(|| contents.join("Frameworks/libRichFilesRail.dylib"))
    }

    pub fn available() -> bool {
        library_path().is_some_and(|path| path.is_file())
    }

    fn load() -> Result<Bridge, &'static str> {
        let began = std::time::Instant::now();
        let bridge = open();
        let elapsed = began.elapsed().as_millis();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let since_start = crate::launch_timing::process_started_ms()
            .map_or(0, |start| now.saturating_sub(start));
        let outcome = if bridge.is_ok() { "loaded" } else { "unavailable" };
        eprintln!("[richos] RichFiles library: {outcome} in {elapsed} ms at {since_start} ms after process start");
        bridge
    }

    fn open() -> Result<Bridge, &'static str> {
        let path = library_path().ok_or("This build does not contain RichFiles.")?;
        let filename = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| "Invalid RichFiles bundle path.")?;
        // The fixed location is within the signed RichOS app. The handle is kept
        // for the life of the process because AppKit keeps Swift view types alive.
        let library = unsafe { libc::dlopen(filename.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if library.is_null() {
            return Err("RichFiles could not load from this installation.");
        }
        let symbol = unsafe { libc::dlsym(library, c"richfiles_rail_place".as_ptr()) };
        let tooltip = unsafe { libc::dlsym(library, c"richfiles_rail_tooltip".as_ptr()) };
        let recording_frame = unsafe { libc::dlsym(library, c"richfiles_rail_recording_frame".as_ptr()) };
        let accept = unsafe { libc::dlsym(library, c"richfiles_rail_receive_workspace".as_ptr()) };
        let reveal = unsafe { libc::dlsym(library, c"richfiles_rail_reveal".as_ptr()) };
        let git = unsafe { libc::dlsym(library, c"richfiles_git_place".as_ptr()) };
        let git_chat = unsafe { libc::dlsym(library, c"richfiles_git_chat".as_ptr()) };
        if symbol.is_null() || accept.is_null() || reveal.is_null() || git.is_null() || git_chat.is_null() {
            unsafe { libc::dlclose(library) };
            return Err("This installation has an incompatible RichFiles pane.");
        }
        let register_glance = unsafe { libc::dlsym(library, c"richfiles_register_glance".as_ptr()) };
        if !register_glance.is_null() {
            // Launch Services registration can do I/O. Never hold up the browser's first paint.
            let register = unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn()>(register_glance) };
            std::thread::spawn(move || unsafe { register() });
        }
        // The bundled Swift target exports this exact C signature. Tauri calls
        // place on the main thread with its live WKWebView pointer.
        Ok(Bridge {
            place: unsafe { std::mem::transmute::<*mut c_void, Place>(symbol) },
            set_tooltip: (!tooltip.is_null())
                .then(|| unsafe { std::mem::transmute::<*mut c_void, SetTooltip>(tooltip) }),
            recording_frame: (!recording_frame.is_null()).then(|| unsafe {
                std::mem::transmute::<*mut c_void, RecordingFrame>(recording_frame)
            }),
            accept: unsafe { std::mem::transmute::<*mut c_void, Accept>(accept) },
            reveal: unsafe { std::mem::transmute::<*mut c_void, Accept>(reveal) },
            place_git: unsafe { std::mem::transmute::<*mut c_void, PlaceGit>(git) },
            git_chat: unsafe { std::mem::transmute::<*mut c_void, Accept>(git_chat) },
        })
    }

    pub async fn load_in_background() -> Result<(), String> {
        fn loaded() -> Result<(), String> {
            BRIDGE.get_or_init(load).as_ref().map(|_| ()).map_err(|message| (*message).to_owned())
        }
        if BRIDGE.get().is_some() {
            return loaded();
        }
        tauri::async_runtime::spawn_blocking(loaded)
            .await
            .map_err(|_| "RichFiles could not load from this installation.".to_owned())?
    }

    pub fn place_git(webview: *mut c_void, bounds: &PaneBounds, dark: bool, context: Option<Vec<u8>>, mode: i32) -> Result<(), String> {
        if !bounds.visible && BRIDGE.get().is_none() { return Ok(()); }
        let action = BRIDGE.get_or_init(load).as_ref().map_err(|message| (*message).to_owned())?.place_git;
        let context = context.map(CString::new).transpose().map_err(|_| "Git could not read the chat record.".to_owned())?;
        let pointer = context.as_ref().map_or(std::ptr::null(), |value| value.as_ptr());
        let placed = unsafe { action(webview, bounds.left, bounds.top, bounds.width, bounds.height,
                                     i32::from(bounds.visible), i32::from(dark), pointer, mode) };
        if placed == 1 { Ok(()) } else { Err("Git could not appear in this window.".to_owned()) }
    }

    pub fn git_chat_changed(chat: Option<String>) {
        let Some(Ok(bridge)) = BRIDGE.get() else { return; };
        let name = chat.and_then(|value| CString::new(value).ok());
        let pointer = name.as_ref().map_or(std::ptr::null(), |value| value.as_ptr());
        unsafe { (bridge.git_chat)(pointer); }
    }

    pub fn place(
        webview: *mut c_void,
        bounds: &PaneBounds,
        tooltip: Option<&RailTooltip>,
    ) -> Result<(), String> {
        if !bounds.visible && BRIDGE.get().is_none() {
            return Ok(());
        }
        let bridge = BRIDGE
            .get_or_init(load)
            .as_ref()
            .map_err(|message| (*message).to_owned())?;
        let accepted = unsafe {
            (bridge.place)(
                webview,
                bounds.left,
                bounds.top,
                bounds.width,
                bounds.height,
                i32::from(bounds.visible),
            )
        };
        if accepted == 1 {
            if bounds.visible {
                if let Some(set_tooltip) = bridge.set_tooltip {
                    unsafe {
                        if let Some(tip) = tooltip {
                            set_tooltip(webview, tip.left, tip.top, tip.width, tip.height, tip.radius, tip.arrow);
                        } else {
                            set_tooltip(webview, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
                        }
                    }
                }
            }
            Ok(())
        } else {
            Err("RichFiles could not appear in this window.".to_owned())
        }
    }
    pub fn reveal(path: &Path) -> Result<(), String> {
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| "Invalid file path.".to_owned())?;
        let bridge = BRIDGE
            .get_or_init(load)
            .as_ref()
            .map_err(|message| (*message).to_owned())?;
        if unsafe { (bridge.reveal)(path.as_ptr()) } == 1 {
            Ok(())
        } else {
            Err("RichFiles could not show this file.".to_owned())
        }
    }

    pub fn recording_frame(webview: *mut c_void) -> Result<Vec<u8>, String> {
        unsafe extern "C" fn copy_frame(bytes: *const u8, size: usize, x: f32, y: f32, context: *mut c_void) {
            if bytes.is_null() || size == 0 || size > 4 * 1024 * 1024 {
                return;
            }
            // The bundled Swift bridge calls back synchronously while both buffers live.
            let frame = unsafe { &mut *context.cast::<Vec<u8>>() };
            frame.reserve(8 + size);
            frame.extend_from_slice(&x.to_le_bytes());
            frame.extend_from_slice(&y.to_le_bytes());
            frame.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, size) });
        }

        let capture = BRIDGE
            .get()
            .and_then(|bridge| bridge.as_ref().ok()?.recording_frame)
            .ok_or("This installation cannot record the RichFiles pane.")?;
        let mut frame = Vec::new();
        unsafe { capture(webview, copy_frame, std::ptr::from_mut(&mut frame).cast()) };
        Ok(frame)
    }

    pub fn accept(id: &str) -> Result<(), String> {
        let id = CString::new(id).map_err(|_| "Invalid RichFiles workspace.".to_owned())?;
        let bridge = BRIDGE
            .get_or_init(load)
            .as_ref()
            .map_err(|message| (*message).to_owned())?;
        if unsafe { (bridge.accept)(id.as_ptr()) } == 1 {
            Ok(())
        } else {
            Err("RichFiles could not open this workspace.".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PaneBounds;

    #[cfg(target_os = "macos")]
    #[test]
    fn dock_url_carries_only_the_local_ticket_identifier() {
        let base = "richos-files-workspace://receive/0c0f2d99-4a92-4428-9d2f-1e41cd6904ef";
        assert_eq!(
            super::dock_id(&base.parse().unwrap()).as_deref(),
            Some("0C0F2D99-4A92-4428-9D2F-1E41CD6904EF")
        );
        for suffix in ["/", "?x=1", "#other", "/other"] {
            assert!(super::dock_id(&format!("{base}{suffix}").parse().unwrap()).is_none());
        }
    }

    #[test]
    fn accepts_only_finite_bounds_inside_the_webview() {
        let mut bounds = PaneBounds {
            left: 0.2,
            top: 0.1,
            width: 0.8,
            height: 0.9,
            visible: true,
        };
        assert!(bounds.valid());
        bounds.width = 0.81;
        assert!(!bounds.valid());
        bounds.width = f64::NAN;
        assert!(!bounds.valid());
        bounds.width = 0.0;
        assert!(!bounds.valid());
        bounds.visible = false;
        assert!(bounds.valid());
    }
}
