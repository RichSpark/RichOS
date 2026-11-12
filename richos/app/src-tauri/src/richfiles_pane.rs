use serde::Deserialize;
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
pub struct DockRequest(Mutex<Option<String>>);

#[tauri::command]
pub fn richfiles_dock_pending(window: WebviewWindow, request: State<'_, DockRequest>) -> bool {
    window.label() == "main" && request.0.lock().is_ok_and(|pending| pending.is_some())
}

#[tauri::command]
pub fn richfiles_accept_dock(
    window: WebviewWindow,
    request: State<'_, DockRequest>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("RichFiles could not reach the window.".to_owned());
    }
    let id = request
        .0
        .lock()
        .map_err(|_| "The RichFiles workspace is unavailable.".to_owned())?
        .take();
    #[cfg(target_os = "macos")]
    if let Some(id) = id {
        window
            .run_on_main_thread(move || {
                if let Err(error) = mac::accept(&id) {
                    eprintln!("[richos] could not deliver the RichFiles workspace: {error}");
                }
            })
            .map_err(|_| "RichFiles could not reach the window.".to_owned())?;
    }
    #[cfg(not(target_os = "macos"))]
    drop(id);
    Ok(())
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
    use tauri::{Emitter, Manager};

    let Ok(data_dir) = app.path().app_data_dir() else {
        return;
    };
    let Some(support) = data_dir.parent() else {
        return;
    };
    let Some(request) = app.try_state::<DockRequest>() else {
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
            .as_ref()
            .is_some_and(|old| folder.join(format!("{old}.json")).is_file())
        {
            return;
        }
        *pending = Some(id);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let shown = crate::conversation_window(&app)
            .is_some_and(|window| window.is_visible().unwrap_or(false));
        if !shown {
            crate::reopen_window(&app);
        }
        if let Some(window) = crate::conversation_window(&app) {
            if let Err(error) = window.emit("rich://richfiles-dock", ()) {
                eprintln!(
                    "[richos] could not notify the window about the RichFiles workspace: {error}"
                );
            }
        }
    });
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use super::PaneBounds;

    type Place = unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, i32) -> i32;
    type Accept = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;

    struct Bridge {
        place: Place,
        accept: Accept,
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
        if symbol.is_null() || accept.is_null() {
            unsafe { libc::dlclose(library) };
            return Err("This installation has an incompatible RichFiles pane.");
        }
        // The bundled Swift target exports this exact C signature. Tauri calls
        // place on the main thread with its live WKWebView pointer.
        Ok(Bridge {
            place: unsafe { std::mem::transmute::<*mut c_void, Place>(symbol) },
            accept: unsafe { std::mem::transmute::<*mut c_void, Accept>(accept) },
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
