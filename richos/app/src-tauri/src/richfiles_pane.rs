use serde::Deserialize;
use tauri::WebviewWindow;

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

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use super::PaneBounds;

    type Place = unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, i32) -> i32;

    static PLACE: OnceLock<Result<Place, &'static str>> = OnceLock::new();

    fn library_path() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let contents = executable.parent()?.parent()?;
        (contents.file_name()? == "Contents")
            .then(|| contents.join("Frameworks/libRichFilesRail.dylib"))
    }

    pub fn available() -> bool {
        library_path().is_some_and(|path| path.is_file())
    }

    fn load() -> Result<Place, &'static str> {
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
        if symbol.is_null() {
            unsafe { libc::dlclose(library) };
            return Err("This installation has an incompatible RichFiles pane.");
        }
        // The bundled Swift target exports this exact C signature. Tauri calls
        // place on the main thread with its live WKWebView pointer.
        Ok(unsafe { std::mem::transmute::<*mut c_void, Place>(symbol) })
    }

    pub fn place(webview: *mut c_void, bounds: &PaneBounds) -> Result<(), String> {
        if !bounds.visible && PLACE.get().is_none() {
            return Ok(());
        }
        let action = (*PLACE.get_or_init(load)).map_err(str::to_owned)?;
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
