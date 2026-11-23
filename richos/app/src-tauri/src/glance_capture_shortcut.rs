use serde::{Deserialize, Serialize};
use tauri::WebviewWindow;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    code: String,
    control: bool,
    option: bool,
    shift: bool,
    command: bool,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self { code: "KeyC".into(), control: true, option: false, shift: true, command: false }
    }
}

#[cfg(target_os = "macos")]
#[derive(Serialize)]
struct Preference {
    enabled: bool,
    shortcut: Shortcut,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    available: bool,
    enabled: bool,
    registered: bool,
    shortcut: Shortcut,
    background: bool,
    menu_bar: bool,
    service: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureSetting {
    Background,
    MenuBar,
}

#[richos_command::command]
pub async fn glance_capture_status() -> Result<ShortcutStatus, String> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(mac::status)
        .await.map_err(|error| error.to_string())?;
    #[cfg(not(target_os = "macos"))]
    Ok(ShortcutStatus::default())
}

#[richos_command::command]
pub async fn glance_capture_configure(enabled: bool, shortcut: Shortcut) -> Result<ShortcutStatus, String> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(move || mac::configure(Preference { enabled, shortcut }))
        .await.map_err(|error| error.to_string())?;
    #[cfg(not(target_os = "macos"))]
    {
        drop((enabled, shortcut));
        Err("The RichGlance shortcut is available on macOS.".into())
    }
}

#[richos_command::command]
pub async fn glance_capture_set(setting: CaptureSetting, enabled: bool) -> Result<ShortcutStatus, String> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(move || mac::set_capture_setting(setting, enabled))
        .await.map_err(|error| error.to_string())?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (setting, enabled);
        Err("RichGlance settings are available on macOS.".into())
    }
}

/// Settings uses the same native window as the RichGlance menu bar.
#[richos_command::command]
pub async fn glance_capture_open(window: WebviewWindow, settings: Option<bool>) -> Result<(), String> {
    if window.label() != "main" {
        return Err("RichGlance could not reach the window.".into());
    }
    let show_settings = settings.unwrap_or(false);
    #[cfg(target_os = "macos")]
    {
        let result = tauri::async_runtime::spawn_blocking(move || {
            mac::open_capture(std::process::id() as libc::pid_t, show_settings)
        }).await;
        if matches!(result, Ok(Ok(()))) {
            return Ok(());
        }
        eprintln!("[RichGlance] capture button could not launch");
    }
    if show_settings {
        Err("RichGlance could not open Capture Settings.".into())
    } else {
        Err("RichGlance could not start a capture.".into())
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSettings {
    opens_editor: bool,
    keeps_history: bool,
    keeps_forever: bool,
}

#[richos_command::command]
pub async fn glance_workflow_status() -> Result<WorkflowSettings, String> {
    #[cfg(target_os = "macos")]
    return workflow::status().map_err(|error| error.to_string());
    #[cfg(not(target_os = "macos"))]
    Ok(WorkflowSettings::default())
}

#[richos_command::command]
pub async fn glance_workflow_configure(opens_editor: Option<bool>, keeps_history: Option<bool>) -> Result<WorkflowSettings, String> {
    #[cfg(target_os = "macos")]
    return tokio::task::spawn_blocking(move || workflow::configure(opens_editor, keeps_history))
        .await.map_err(|error| error.to_string())?.map_err(|error| error.to_string());
    #[cfg(not(target_os = "macos"))]
    {
        let (_opens_editor, _keeps_history) = (opens_editor, keeps_history);
        Err("RichGlance settings are available on macOS.".into())
    }
}

#[cfg(target_os = "macos")]
mod workflow {
    use super::WorkflowSettings;
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::io::{self, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};

    fn root() -> io::Result<PathBuf> {
        let home = std::env::home_dir().ok_or_else(|| io::Error::other("home directory is unavailable"))?;
        Ok(home.join("Library/Application Support/app.richos.richglance/Screenshots"))
    }

    pub(super) fn status() -> io::Result<WorkflowSettings> {
        Ok(read(&root()?))
    }

    fn read(root: &Path) -> WorkflowSettings {
        WorkflowSettings {
            opens_editor: fs::read(root.join("return-action")).is_ok_and(|bytes| bytes == b"editor"),
            keeps_history: match fs::read(root.join("keep-history")) {
                Ok(bytes) => bytes == b"on",
                Err(error) => error.kind() == io::ErrorKind::NotFound,
            },
            keeps_forever: fs::read(root.join("retention")).is_ok_and(|bytes| bytes == b"forever"),
        }
    }

    pub(super) fn configure(opens_editor: Option<bool>, keeps_history: Option<bool>) -> io::Result<WorkflowSettings> {
        let root = root()?;
        DirBuilder::new().recursive(true).mode(0o700).create(&root)?;
        let info = fs::symlink_metadata(&root)?;
        if !info.is_dir() || info.mode() & 0o077 != 0 || info.uid() != unsafe { libc::getuid() } {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "RichGlance settings folder is inaccessible"));
        }
        // The native editor holds the same lock when it stores, updates or purges originals.
        let lock = OpenOptions::new().create(true).read(true).write(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(root.join(".lock"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if let Some(value) = opens_editor {
            write_setting(&root, "return-action", if value { b"editor" } else { b"copy" })?;
        }
        if let Some(value) = keeps_history {
            if !value {
                match fs::remove_dir_all(root.join("Recent Captures")) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            write_setting(&root, "keep-history", if value { b"on" } else { b"off" })?;
        }
        Ok(read(&root))
    }

    fn write_setting(root: &Path, name: &str, contents: &[u8]) -> io::Result<()> {
        let pending = root.join(format!(".{name}.pending"));
        match fs::remove_file(&pending) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let result = (|| {
            let mut output = OpenOptions::new().create_new(true).write(true).mode(0o600).open(&pending)?;
            output.write_all(contents)?;
            fs::rename(&pending, root.join(name))
        })();
        if result.is_err() { drop(fs::remove_file(pending)); }
        result
    }
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn history_defaults_on_and_respects_explicit_off() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            assert!(read(root).keeps_history);
            assert!(!read(root).keeps_forever);
            write_setting(root, "retention", b"forever").unwrap();
            assert!(read(root).keeps_forever);
            write_setting(root, "keep-history", b"off").unwrap();
            assert!(!read(root).keeps_history);
            assert!(read(root).keeps_forever);
            write_setting(root, "keep-history", b"on").unwrap();
            assert!(read(root).keeps_history);
        }
    }
}

#[cfg(target_os = "macos")]
pub fn restore(app: tauri::AppHandle) {
    let observer_app = app.clone();
    drop(app.run_on_main_thread(move || mac::observe_helper_exit(observer_app)));
    mac::restore(app);
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{CaptureSetting, Preference, ShortcutStatus};
    use objc2::{class, msg_send, runtime::{AnyObject, Bool}};
    use ring::rand::{SecureRandom, SystemRandom};
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tauri::{AppHandle, Emitter};

    static RESTORING: AtomicBool = AtomicBool::new(false);
    const STATUS_CHANGED: &str = "rich://glance-capture-status-changed";

    pub(super) fn restore(app: AppHandle) {
        if RESTORING.swap(true, Ordering::AcqRel) { return; }
        tauri::async_runtime::spawn_blocking(move || {
            if let Some(native) = application() {
                if command(&native, &["restore"]).is_err() {
                    eprintln!("[RichGlance] could not restore the capture shortcut");
                }
                drop(app.emit(STATUS_CHANGED, ()));
            }
            RESTORING.store(false, Ordering::Release);
        });
    }

    /// AppKit owns the observer for the process lifetime. A stopped login
    /// item can need a session shortcut even while RichOS has no open window.
    pub(super) fn observe_helper_exit(app: AppHandle) {
        objc2::rc::autoreleasepool(|_| unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let center: *mut AnyObject = msg_send![workspace, notificationCenter];
            let name: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String:
                c"NSWorkspaceDidTerminateApplicationNotification".as_ptr()];
            let block = block2::RcBlock::new(move |notice: *mut AnyObject| {
                if notice.is_null() { return; }
                let info: *mut AnyObject = msg_send![notice, userInfo];
                if info.is_null() { return; }
                let key: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String:
                    c"NSWorkspaceApplicationKey".as_ptr()];
                let stopped: *mut AnyObject = msg_send![info, objectForKey: key];
                if stopped.is_null() { return; }
                let bundle: *mut AnyObject = msg_send![stopped, bundleIdentifier];
                if bundle.is_null() { return; }
                let helper: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String:
                    c"app.richos.richglance.capture-helper".as_ptr()];
                let matches: Bool = msg_send![bundle, isEqualToString: helper];
                if matches.as_bool() { restore(app.clone()); }
            });
            let _: *mut AnyObject = msg_send![center, addObserverForName: name,
                object: std::ptr::null::<AnyObject>(), queue: std::ptr::null::<AnyObject>(), usingBlock: &*block];
        });
    }

    fn application() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let contents = executable.parent()?.parent()?;
        if contents.file_name()? != "Contents" { return None; }
        let app = contents.join("Applications/RichFiles.app/Contents/Applications/RichGlance.app");
        app.is_dir().then_some(app)
    }

    pub(super) fn status() -> Result<ShortcutStatus, String> {
        match application() {
            Some(app) => command(&app, &["status"]),
            None => Ok(ShortcutStatus::default()),
        }
    }

    pub(super) fn configure(preference: Preference) -> Result<ShortcutStatus, String> {
        let app = application().ok_or("The RichGlance app could not be found in this installation.")?;
        let settings = serde_json::to_string(&preference).map_err(|error| error.to_string())?;
        command(&app, &["configure", &settings])
    }

    pub(super) fn set_capture_setting(setting: CaptureSetting, enabled: bool) -> Result<ShortcutStatus, String> {
        let app = application().ok_or("The RichGlance app could not be found in this installation.")?;
        let name = match setting {
            CaptureSetting::Background => "background",
            CaptureSetting::MenuBar => "menu-bar",
        };
        command(&app, &[name, if enabled { "true" } else { "false" }])
    }

    fn command(app: &Path, arguments: &[&str]) -> Result<ShortcutStatus, String> {
        let output = Command::new(app.join("Contents/MacOS/RichGlance")).arg("--capture-helper")
            .args(arguments).output().map_err(|_| "RichGlance Capture could not start.")?;
        if !output.status.success() {
            return Err(String::from_utf8(output.stderr).ok().filter(|line| !line.trim().is_empty())
                .unwrap_or_else(|| "RichGlance Capture could not read its settings.".into()));
        }
        serde_json::from_slice(&output.stdout).map_err(|_| "RichGlance Capture could not read its settings.".into())
    }

    pub(super) fn open_capture(origin: libc::pid_t, settings: bool) -> std::io::Result<()> {
        let app = application().ok_or_else(|| std::io::Error::other("RichGlance is unavailable"))?;
        let home = std::env::home_dir().ok_or_else(|| std::io::Error::other("home directory is unavailable"))?;
        let directory = home.join("Library/Application Support/app.richos.richglance/Capture Requests");
        DirBuilder::new().recursive(true).mode(0o700).create(&directory)?;
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::getuid() } {
            return Err(std::io::Error::other("capture request folder is not private"));
        }
        discard_expired(&directory);
        let id = random_id()?;
        let path = directory.join(&id);
        let mut ticket = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
        let request = if settings { "settings".into() } else { origin.to_string() };
        if let Err(error) = ticket.write_all(request.as_bytes()) {
            drop(fs::remove_file(path));
            return Err(error);
        }
        drop(ticket);
        let url = format!("richglance-capture://request/{id}");
        let mut launch = Command::new("/usr/bin/open");
        if !settings { launch.arg("-g"); }
        let opened = launch.arg("-a").arg(app).arg(url).args(["--args", "--capture-request"]).status();
        if opened.is_ok_and(|status| status.success()) { return Ok(()); }
        drop(fs::remove_file(path));
        Err(std::io::Error::other("macOS could not open RichGlance"))
    }

    fn discard_expired(directory: &Path) {
        let Ok(entries) = fs::read_dir(directory) else { return; };
        for entry in entries.flatten().take(32) {
            let name = entry.file_name();
            if name.len() != 36 || !name.to_string_lossy().bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-') { continue; }
            if let Ok(metadata) = fs::symlink_metadata(entry.path()) {
                if metadata.is_file() && metadata.modified().ok().and_then(|time| time.elapsed().ok())
                    .is_some_and(|age| age >= Duration::from_secs(30)) { drop(fs::remove_file(entry.path())); }
            }
        }
    }

    fn random_id() -> std::io::Result<String> {
        let mut bytes = [0_u8; 16];
        SystemRandom::new().fill(&mut bytes).map_err(|_| std::io::Error::other("could not start capture"))?;
        let mut id = String::with_capacity(36);
        use std::fmt::Write as _;
        for (index, byte) in bytes.into_iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) { id.push('-'); }
            write!(id, "{byte:02x}").map_err(std::io::Error::other)?;
        }
        Ok(id)
    }

}

#[cfg(test)]
mod bridge_tests {
    use super::*;

    #[test]
    fn reads_the_native_shortcut_and_login_state() {
        let status: ShortcutStatus = serde_json::from_str(r#"{
            "available": true, "enabled": true, "registered": false,
            "shortcut":{"code":"KeyC","control":true,"option":false,"shift":true,"command":false},
            "background": true, "menuBar": true, "service": "approval"
        }"#).unwrap();
        assert_eq!(status.shortcut, Shortcut::default());
        assert!(status.background);
        assert!(status.menu_bar);
        assert!(!status.registered);
        assert_eq!(status.service.as_deref(), Some("approval"));
    }
}
