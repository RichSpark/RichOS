use serde::Deserialize;
use std::path::PathBuf;
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

#[richos_command::command]
pub fn output_git_available(window: WebviewWindow) -> bool {
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
pub async fn output_git_open(
    window: WebviewWindow,
    bounds: PaneBounds,
    dark: bool,
    output_id: Option<String>,
    thread_id: String,
    refresh: bool,
) -> Result<(), String> {
    if window.label() != "main" || !bounds.valid() || !bounds.visible {
        return Err("Git could not use that window area.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let app = window.app_handle().clone();
        let active_app = app.clone();
        let mode = if output_id.is_some() {
            1
        } else if refresh {
            2
        } else {
            0
        };
        let (scope, payload) = tauri::async_runtime::spawn_blocking(move || {
            let files = app.state::<crate::output_files::OutputFiles>();
            if files.active_chat().as_deref() != Some(&thread_id) {
                return Err("The active chat changed.".to_owned());
            }
            let state = app.state::<crate::AppState>();
            let git = state
                .engine_dir
                .lock()
                .ok()
                .and_then(|root| richos_core::home_repos::home_git(&root))
                .map(|git| git.bin);
            let scope = files.git_context(output_id.as_deref(), git)?;
            let bytes = serde_json::to_vec(&scope)
                .map_err(|_| "Git could not read the chat record.".to_owned())?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("This chat’s Git record is too large to display.".to_owned());
            }
            Ok((scope.thread_id, bytes))
        })
        .await
        .map_err(|_| "Git could not read the active chat.".to_owned())??;
        mac::load_in_background().await?;
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |webview| {
                let files = active_app.state::<crate::output_files::OutputFiles>();
                let result = if files.active_chat().as_deref() == Some(scope.as_str()) {
                    mac::place_git(webview.inner(), &bounds, dark, Some(payload), mode)
                } else {
                    Err("The active chat changed.".to_owned())
                };
                let _ignored = send.send(result);
            })
            .map_err(|_| "Git could not reach the window.".to_owned())?;
        receive
            .await
            .map_err(|_| "The RichOS window closed.".to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    Err("The Git view requires macOS.".to_owned())
}

#[richos_command::command]
pub async fn output_git_place(
    window: WebviewWindow,
    bounds: PaneBounds,
    dark: bool,
    close: bool,
) -> Result<(), String> {
    if window.label() != "main" || !bounds.valid() {
        return Err("Git could not use that window area.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |webview| {
                let _ignored = send.send(mac::place_git(
                    webview.inner(),
                    &bounds,
                    dark,
                    None,
                    if close { 3 } else { 0 },
                ));
            })
            .map_err(|_| "Git could not reach the window.".to_owned())?;
        receive
            .await
            .map_err(|_| "The RichOS window closed.".to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    Err("The Git view requires macOS.".to_owned())
}

#[richos_command::command]
pub fn richfiles_git_chat(
    window: WebviewWindow,
    files: State<'_, crate::output_files::OutputFiles>,
) {
    if window.label() != "main" {
        return;
    }
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

#[richos_command::command]
pub fn output_reveal_in_richfiles(
    files: State<'_, crate::output_files::OutputFiles>,
    output_id: String,
) -> Result<String, String> {
    let path = files.path_to_reveal(&output_id)?;
    let name = path
        .file_name()
        .ok_or_else(|| "The output has no filename.".to_owned())?
        .to_string_lossy()
        .into_owned();
    open_bundle(Some(path))?;
    Ok(format!("Showing {name} in RichFiles."))
}

#[richos_command::command]
pub fn richfiles_available(window: WebviewWindow) -> bool {
    window.label() == "main" && richfiles_bundle().is_some()
}

#[richos_command::command]
pub async fn richfiles_open(window: WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("RichFiles could not reach the window.".to_owned());
    }
    tauri::async_runtime::spawn_blocking(|| open_bundle(None))
        .await
        .map_err(|_| "RichFiles could not open.".to_owned())?
}

#[richos_command::command]
pub async fn richfiles_settings(
    window: WebviewWindow,
    option: Option<String>,
    enabled: Option<bool>,
) -> Result<serde_json::Value, String> {
    if window.label() != "main" {
        return Err("RichFiles could not reach the window.".to_owned());
    }
    let operation = match (option.as_deref(), enabled) {
        (None, None) => 0,
        (Some("folders"), Some(_)) => 1,
        (Some("pictures"), Some(_)) => 2,
        (Some("windowsKeys"), Some(_)) => 3,
        _ => return Err("That RichFiles setting is unavailable.".to_owned()),
    };
    #[cfg(target_os = "macos")]
    {
        mac::load_in_background().await?;
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .run_on_main_thread(move || mac::settings(operation, enabled.unwrap_or(false), send))
            .map_err(|_| "RichFiles could not reach the window.".to_owned())?;
        receive
            .await
            .map_err(|_| "RichFiles settings are unavailable.".to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = operation;
        Err("RichFiles requires macOS.".to_owned())
    }
}

fn richfiles_bundle() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe().ok()?;
        let contents = executable.parent()?.parent()?;
        let bundle = contents.join("Applications/RichFiles.app");
        (contents.file_name()? == "Contents" && bundle.is_dir()).then_some(bundle)
    }
    #[cfg(not(target_os = "macos"))]
    None
}

fn open_bundle(file: Option<PathBuf>) -> Result<(), String> {
    let bundle = richfiles_bundle().ok_or("This build does not contain RichFiles.")?;
    let mut launch = std::process::Command::new("/usr/bin/open");
    launch.arg("-a").arg(bundle);
    if let Some(path) = file {
        launch.arg("--").arg(path);
    }
    let status = launch.status().map_err(|_| "RichFiles could not open.")?;
    if status.success() {
        Ok(())
    } else {
        Err("RichFiles could not open.".to_owned())
    }
}

#[cfg(target_os = "macos")]
pub fn start_window_gestures() {
    let launch = crate::update_startup::launch_report();
    let action = if launch.update_arrival.is_some() || launch.handover.is_some() {
        "refresh-after-update"
    } else {
        "register-if-desktop-enabled"
    };
    std::thread::spawn(move || {
        if let Some(bundle) = richfiles_bundle() {
            run_gesture_service(bundle, action);
        }
    });
}

#[cfg(target_os = "macos")]
pub fn refresh_gestures_after_publication(home: &std::path::Path) {
    // An update can be published at quit without a new RichOS window. Ask the
    // verified installed copy to refresh the login item before we leave.
    if let Ok(Some(bundle)) = richos_user_update::with_preferred_at(home, |_, bundle| {
        Ok(bundle.join("Contents/Applications/RichFiles.app"))
    }) {
        run_gesture_service(bundle, "refresh-after-update");
    }
}

#[cfg(target_os = "macos")]
fn run_gesture_service(bundle: PathBuf, action: &str) {
    let binary = bundle.join("Contents/MacOS/RichFiles");
    if !binary.is_file() {
        return;
    }
    if let Err(error) = std::process::Command::new(binary)
        .args(["--window-gesture-service", action])
        .stdout(std::process::Stdio::null())
        .status()
    {
        eprintln!("[richos] could not check Window Gestures: {error}");
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use super::PaneBounds;

    type Accept = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;
    type PlaceGit = unsafe extern "C" fn(
        *mut c_void,
        f64,
        f64,
        f64,
        f64,
        i32,
        i32,
        *const std::ffi::c_char,
        i32,
    ) -> i32;

    type SettingsReply = unsafe extern "C" fn(*const u8, usize, *mut c_void);
    type Settings = unsafe extern "C" fn(i32, i32, SettingsReply, *mut c_void);

    struct Bridge {
        place_git: PlaceGit,
        git_chat: Accept,
        settings: Settings,
    }

    static BRIDGE: OnceLock<Result<Bridge, &'static str>> = OnceLock::new();

    fn library_path() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let contents = executable.parent()?.parent()?;
        (contents.file_name()? == "Contents")
            .then(|| contents.join("Frameworks/libRichFilesIntegration.dylib"))
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
        let since_start =
            crate::launch_timing::process_started_ms().map_or(0, |start| now.saturating_sub(start));
        let outcome = if bridge.is_ok() {
            "loaded"
        } else {
            "unavailable"
        };
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
        let git = unsafe { libc::dlsym(library, c"richfiles_git_place".as_ptr()) };
        let git_chat = unsafe { libc::dlsym(library, c"richfiles_git_chat".as_ptr()) };
        let settings = unsafe { libc::dlsym(library, c"richfiles_settings".as_ptr()) };
        if git.is_null() || git_chat.is_null() || settings.is_null() {
            unsafe { libc::dlclose(library) };
            return Err("This installation has an incompatible Git view.");
        }
        let register_glance =
            unsafe { libc::dlsym(library, c"richfiles_register_glance".as_ptr()) };
        if !register_glance.is_null() {
            // Launch Services registration can do I/O. Never hold up the browser's first paint.
            let register = unsafe {
                std::mem::transmute::<*mut c_void, unsafe extern "C" fn()>(register_glance)
            };
            std::thread::spawn(move || unsafe { register() });
        }
        // The bundled Swift target exports this exact C signature. Tauri calls
        // place on the main thread with its live WKWebView pointer.
        Ok(Bridge {
            place_git: unsafe { std::mem::transmute::<*mut c_void, PlaceGit>(git) },
            git_chat: unsafe { std::mem::transmute::<*mut c_void, Accept>(git_chat) },
            settings: unsafe { std::mem::transmute::<*mut c_void, Settings>(settings) },
        })
    }

    pub async fn load_in_background() -> Result<(), String> {
        fn loaded() -> Result<(), String> {
            BRIDGE
                .get_or_init(load)
                .as_ref()
                .map(|_| ())
                .map_err(|message| (*message).to_owned())
        }
        if BRIDGE.get().is_some() {
            return loaded();
        }
        tauri::async_runtime::spawn_blocking(loaded)
            .await
            .map_err(|_| "RichFiles could not load from this installation.".to_owned())?
    }

    type SettingsResult = Result<serde_json::Value, String>;

    pub fn settings(
        operation: i32,
        enabled: bool,
        sender: tokio::sync::oneshot::Sender<SettingsResult>,
    ) {
        unsafe extern "C" fn receive(bytes: *const u8, size: usize, context: *mut c_void) {
            // The Swift bridge calls this exactly once. The sender remains valid
            // even if the webview closed while the OS was confirming a change.
            let sender = unsafe {
                Box::from_raw(context.cast::<tokio::sync::oneshot::Sender<SettingsResult>>())
            };
            let result = if bytes.is_null() || size == 0 || size > 64 * 1024 {
                Err("RichFiles settings are unavailable.".to_owned())
            } else {
                serde_json::from_slice(unsafe { std::slice::from_raw_parts(bytes, size) })
                    .map_err(|_| "RichFiles settings are unavailable.".to_owned())
            };
            let _ignored = sender.send(result);
        }

        let Some(Ok(bridge)) = BRIDGE.get() else {
            let _ignored = sender.send(Err("RichFiles settings are unavailable.".to_owned()));
            return;
        };
        let context = Box::into_raw(Box::new(sender)).cast();
        unsafe { (bridge.settings)(operation, i32::from(enabled), receive, context) };
    }

    pub fn place_git(
        webview: *mut c_void,
        bounds: &PaneBounds,
        dark: bool,
        context: Option<Vec<u8>>,
        mode: i32,
    ) -> Result<(), String> {
        if !bounds.visible && BRIDGE.get().is_none() {
            return Ok(());
        }
        let action = BRIDGE
            .get_or_init(load)
            .as_ref()
            .map_err(|message| (*message).to_owned())?
            .place_git;
        let context = context
            .map(CString::new)
            .transpose()
            .map_err(|_| "Git could not read the chat record.".to_owned())?;
        let pointer = context
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let placed = unsafe {
            action(
                webview,
                bounds.left,
                bounds.top,
                bounds.width,
                bounds.height,
                i32::from(bounds.visible),
                i32::from(dark),
                pointer,
                mode,
            )
        };
        if placed == 1 {
            Ok(())
        } else {
            Err("Git could not appear in this window.".to_owned())
        }
    }

    pub fn git_chat_changed(chat: Option<String>) {
        let Some(Ok(bridge)) = BRIDGE.get() else {
            return;
        };
        let name = chat.and_then(|value| CString::new(value).ok());
        let pointer = name
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        unsafe {
            (bridge.git_chat)(pointer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PaneBounds;

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
