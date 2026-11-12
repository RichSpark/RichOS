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

#[tauri::command]
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

#[tauri::command]
pub async fn richfiles_place_pane(window: WebviewWindow, bounds: PaneBounds) -> Result<(), String> {
    if window.label() != "main" || !bounds.valid() {
        return Err("RichFiles could not use that window area.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |webview| {
                let result = mac::place(webview.inner(), &bounds);
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

#[derive(Default)]
pub struct PaneRequests(Mutex<Requests>);

#[derive(Default)]
struct Requests {
    workspace: Option<String>,
    reveal: Option<PathBuf>,
}

#[tauri::command]
pub fn richfiles_request_pending(window: WebviewWindow, request: State<'_, PaneRequests>) -> bool {
    window.label() == "main"
        && request
            .0
            .lock()
            .is_ok_and(|pending| pending.workspace.is_some() || pending.reveal.is_some())
}

#[tauri::command]
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

#[tauri::command]
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

    use super::PaneBounds;

    type Place = unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, i32) -> i32;
    type Accept = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;

    struct Bridge {
        place: Place,
        accept: Accept,
        reveal: Accept,
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
        let accept = unsafe { libc::dlsym(library, c"richfiles_rail_receive_workspace".as_ptr()) };
        let reveal = unsafe { libc::dlsym(library, c"richfiles_rail_reveal".as_ptr()) };
        if symbol.is_null() || accept.is_null() || reveal.is_null() {
            unsafe { libc::dlclose(library) };
            return Err("This installation has an incompatible RichFiles pane.");
        }
        // The bundled Swift target exports this exact C signature. Tauri calls
        // place on the main thread with its live WKWebView pointer.
        Ok(Bridge {
            place: unsafe { std::mem::transmute::<*mut c_void, Place>(symbol) },
            accept: unsafe { std::mem::transmute::<*mut c_void, Accept>(accept) },
            reveal: unsafe { std::mem::transmute::<*mut c_void, Accept>(reveal) },
        })
    }

    pub fn place(webview: *mut c_void, bounds: &PaneBounds) -> Result<(), String> {
        if !bounds.visible && BRIDGE.get().is_none() {
            return Ok(());
        }
        let action = BRIDGE
            .get_or_init(load)
            .as_ref()
            .map_err(|message| (*message).to_owned())?
            .place;
        let accepted = unsafe {
            action(
                webview,
                bounds.left,
                bounds.top,
                bounds.width,
                bounds.height,
                i32::from(bounds.visible),
            )
        };
        if accepted == 1 {
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
