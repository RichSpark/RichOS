use serde::{Deserialize, Serialize};
use tauri::{AppHandle, WebviewWindow};

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

#[derive(Clone, Default, Deserialize, Serialize)]
struct Preference {
    enabled: bool,
    shortcut: Shortcut,
}

#[derive(Serialize)]
pub struct ShortcutStatus {
    available: bool,
    enabled: bool,
    registered: bool,
    shortcut: Shortcut,
}

#[richos_command::command]
pub async fn glance_capture_status(app: AppHandle) -> Result<ShortcutStatus, String> {
    #[cfg(target_os = "macos")]
    return on_main(app, mac::status).await;
    #[cfg(not(target_os = "macos"))]
    {
        drop(app);
        Ok(ShortcutStatus { available: false, enabled: false, registered: false, shortcut: Shortcut::default() })
    }
}

#[richos_command::command]
pub async fn glance_capture_configure(app: AppHandle, enabled: bool, shortcut: Shortcut) -> Result<ShortcutStatus, String> {
    #[cfg(target_os = "macos")]
    return on_main(app, move |app| mac::configure(app, Preference { enabled, shortcut })).await;
    #[cfg(not(target_os = "macos"))]
    {
        drop((app, enabled, shortcut));
        Err("The RichGlance shortcut is available on macOS.".into())
    }
}

/// The rail button starts the same capture as the global shortcut.
#[richos_command::command]
pub async fn glance_capture_open(window: WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("RichGlance could not reach the window.".into());
    }
    #[cfg(target_os = "macos")]
    {
        let result = tauri::async_runtime::spawn_blocking(|| {
            mac::open_capture(std::process::id() as libc::pid_t)
        }).await;
        if matches!(result, Ok(Ok(()))) {
            return Ok(());
        }
        eprintln!("[RichGlance] capture button could not launch");
    }
    Err("RichGlance could not start a capture.".into())
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
async fn on_main(
    app: AppHandle,
    action: impl FnOnce(&AppHandle) -> Result<ShortcutStatus, String> + Send + 'static,
) -> Result<ShortcutStatus, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || drop(send.send(action(&handle)))).map_err(|error| error.to_string())?;
    receive.await.map_err(|_| "RichGlance shortcut settings were interrupted.".to_string())?
}

#[cfg(target_os = "macos")]
pub fn restore(app: AppHandle) {
    let handle = app.clone();
    if app.run_on_main_thread(move || drop(mac::status(&handle))).is_err() {
        eprintln!("[RichGlance] could not restore the capture shortcut");
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use objc2::{class, msg_send, runtime::AnyObject};
    use ring::rand::{SecureRandom, SystemRandom};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::time::{Duration, Instant};
    use tauri::Manager;

    type CarbonRef = *mut c_void;
    const SIGNATURE: u32 = u32::from_be_bytes(*b"RGCp");

    #[repr(C)]
    struct EventKind { class: u32, kind: u32 }
    #[repr(C)]
    struct HotkeyId { signature: u32, id: u32 }

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> CarbonRef;
        fn InstallEventHandler(target: CarbonRef, handler: unsafe extern "C" fn(CarbonRef, CarbonRef, CarbonRef) -> i32,
                               count: u32, kinds: *const EventKind, context: CarbonRef, installed: *mut CarbonRef) -> i32;
        fn RegisterEventHotKey(code: u32, modifiers: u32, id: HotkeyId, target: CarbonRef, options: u32,
                               registered: *mut CarbonRef) -> i32;
        fn UnregisterEventHotKey(registered: CarbonRef) -> i32;
        fn GetEventParameter(event: CarbonRef, name: u32, kind: u32, actual_kind: *mut u32, capacity: usize,
                             actual_size: *mut usize, output: CarbonRef) -> i32;
    }

    #[derive(Default)]
    struct Registration {
        handler: CarbonRef,
        hotkey: CarbonRef,
        next_id: u32,
        active_id: u32,
        last_pressed: Option<Instant>,
        preference: Option<Preference>,
    }
    thread_local! { static REGISTRATION: RefCell<Registration> = RefCell::default(); }

    impl Registration {
        fn status(&self) -> ShortcutStatus {
            let preference = self.preference.clone().unwrap_or_default();
            ShortcutStatus { available: application().is_some(), enabled: preference.enabled,
                             registered: !self.hotkey.is_null(), shortcut: preference.shortcut }
        }

        fn register(&mut self, shortcut: &Shortcut) -> Result<(u32, CarbonRef), String> {
            let (code, modifiers) = shortcut.carbon()?;
            let target = unsafe { GetApplicationEventTarget() };
            if self.handler.is_null() {
                let kind = EventKind { class: u32::from_be_bytes(*b"keyb"), kind: 5 };
                let status = unsafe { InstallEventHandler(target, pressed, 1, &kind, ptr::null_mut(), &mut self.handler) };
                if status != 0 { return Err("macOS could not start listening for the shortcut.".into()); }
            }
            self.next_id = self.next_id.wrapping_add(1);
            let id = self.next_id;
            let mut hotkey = ptr::null_mut();
            let status = unsafe { RegisterEventHotKey(code, modifiers, HotkeyId { signature: SIGNATURE, id },
                                                       target, 1, &mut hotkey) };
            if status != 0 || hotkey.is_null() {
                return Err("That shortcut is unavailable. Choose another combination.".into());
            }
            Ok((id, hotkey))
        }

        fn replace(&mut self, id: u32, hotkey: CarbonRef) {
            if !self.hotkey.is_null() { unsafe { UnregisterEventHotKey(self.hotkey) }; }
            self.active_id = id;
            self.hotkey = hotkey;
            self.last_pressed = None;
        }

        fn load(&mut self, app: &AppHandle) -> Result<(), String> {
            if self.preference.is_some() { return Ok(()); }
            let path = preferences_path(app)?;
            let preference = match fs::read(path) {
                Ok(data) => serde_json::from_slice(&data).map_err(|_| "Could not read the RichGlance shortcut settings.")?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Preference::default(),
                Err(_) => return Err("Could not read the RichGlance shortcut settings.".into()),
            };
            if preference.enabled && application().is_some() {
                if let Ok((id, hotkey)) = self.register(&preference.shortcut) { self.replace(id, hotkey); }
            }
            self.preference = Some(preference);
            Ok(())
        }
    }

    impl Shortcut {
        fn carbon(&self) -> Result<(u32, u32), String> {
            const KEYS: &[(&str, u32)] = &[
                ("KeyA", 0), ("KeyB", 11), ("KeyC", 8), ("KeyD", 2), ("KeyE", 14), ("KeyF", 3),
                ("KeyG", 5), ("KeyH", 4), ("KeyI", 34), ("KeyJ", 38), ("KeyK", 40), ("KeyL", 37),
                ("KeyM", 46), ("KeyN", 45), ("KeyO", 31), ("KeyP", 35), ("KeyQ", 12), ("KeyR", 15),
                ("KeyS", 1), ("KeyT", 17), ("KeyU", 32), ("KeyV", 9), ("KeyW", 13), ("KeyX", 7),
                ("KeyY", 16), ("KeyZ", 6), ("Digit0", 29), ("Digit1", 18), ("Digit2", 19), ("Digit3", 20),
                ("Digit4", 21), ("Digit5", 23), ("Digit6", 22), ("Digit7", 26), ("Digit8", 28), ("Digit9", 25),
            ];
            let modifiers = u32::from(self.control) << 12 | u32::from(self.option) << 11
                | u32::from(self.shift) << 9 | u32::from(self.command) << 8;
            let code = KEYS.iter().find(|(key, _)| *key == self.code).map(|(_, code)| *code);
            if let Some(code) = code.filter(|_| modifiers.count_ones() >= 2 && (self.control || self.option || self.command)) {
                if self.command && self.shift && !self.option
                    && matches!(self.code.as_str(), "Digit3" | "Digit4" | "Digit5" | "Digit6") {
                    return Err("macOS uses that shortcut for screenshots. Choose another combination.".into());
                }
                return Ok((code, modifiers));
            }
            Err("Use two or more modifiers and a letter or number.".into())
        }
    }

    pub fn status(app: &AppHandle) -> Result<ShortcutStatus, String> {
        REGISTRATION.with(|state| {
            let mut state = state.borrow_mut();
            state.load(app)?;
            Ok(state.status())
        })
    }

    pub fn configure(app: &AppHandle, preference: Preference) -> Result<ShortcutStatus, String> {
        REGISTRATION.with(|state| {
            let mut state = state.borrow_mut();
            state.load(app)?;
            preference.shortcut.carbon()?;
            if application().is_none() { return Err("The RichGlance app could not be found in this installation.".into()); }
            let unchanged = preference.enabled && !state.hotkey.is_null()
                && state.preference.as_ref().is_some_and(|old| old.shortcut == preference.shortcut);
            let replacement = if preference.enabled && !unchanged { Some(state.register(&preference.shortcut)?) }
                else { None };
            if persist(app, &preference).is_err() {
                if let Some((_, hotkey)) = replacement { unsafe { UnregisterEventHotKey(hotkey) }; }
                return Err("Could not save the RichGlance shortcut. Your previous setting is still in use.".into());
            }
            if let Some((id, hotkey)) = replacement { state.replace(id, hotkey); }
            if !preference.enabled { state.replace(0, ptr::null_mut()); }
            state.preference = Some(preference);
            Ok(state.status())
        })
    }

    unsafe extern "C" fn pressed(_next: CarbonRef, event: CarbonRef, _context: CarbonRef) -> i32 {
        const PASS: i32 = -9874;
        let mut id = HotkeyId { signature: 0, id: 0 };
        let status = unsafe { GetEventParameter(event, u32::from_be_bytes(*b"----"), u32::from_be_bytes(*b"hkid"),
                                                ptr::null_mut(), std::mem::size_of::<HotkeyId>(), ptr::null_mut(),
                                                (&mut id as *mut HotkeyId).cast()) };
        if status != 0 || id.signature != SIGNATURE { return PASS; }
        let opened = REGISTRATION.with(|state| {
            let Ok(mut state) = state.try_borrow_mut() else { return false; };
            if state.hotkey.is_null() || state.active_id != id.id { return false; }
            if state.last_pressed.is_none_or(|last| last.elapsed() >= Duration::from_millis(250)) {
                state.last_pressed = Some(Instant::now());
                let origin = frontmost_pid();
                if std::thread::Builder::new().name("richglance-launch".into()).spawn(move || {
                    if open_capture(origin).is_err() { eprintln!("[RichGlance] capture shortcut could not launch"); }
                }).is_err() {
                    eprintln!("[RichGlance] could not start capture launcher");
                }
            }
            true
        });
        if opened { 0 } else { PASS }
    }

    // Look up the source before Launch Services or the permission dialog can take focus.
    fn frontmost_pid() -> libc::pid_t {
        // The Carbon handler runs on the main thread. These selectors use AppKit's
        // object returns and NSRunningApplication's pid_t return type.
        unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let origin: *mut AnyObject = msg_send![workspace, frontmostApplication];
            if origin.is_null() { 0 } else { msg_send![origin, processIdentifier] }
        }
    }

    fn application() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let contents = executable.parent()?.parent()?;
        if contents.file_name()? != "Contents" { return None; }
        let app = contents.join("Applications/RichFiles.app/Contents/Applications/RichGlance.app");
        app.is_dir().then_some(app)
    }

    fn preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
        app.path().app_config_dir().map(|folder| folder.join("richglance-shortcut.json"))
            .map_err(|_| "Could not locate RichGlance shortcut settings.".into())
    }

    fn persist(app: &AppHandle, preference: &Preference) -> std::io::Result<()> {
        let path = preferences_path(app).map_err(std::io::Error::other)?;
        let parent = path.parent().ok_or_else(|| std::io::Error::other("settings folder missing"))?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".richglance-shortcut-{}", random_id()?));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
            serde_json::to_writer(&mut file, preference)?;
            file.flush()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() { drop(fs::remove_file(temporary)); }
        result
    }

    pub(super) fn open_capture(origin: libc::pid_t) -> std::io::Result<()> {
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
        if let Err(error) = write!(ticket, "{origin}") {
            drop(fs::remove_file(path));
            return Err(error);
        }
        drop(ticket);
        let url = format!("richglance-capture://request/{id}");
        let opened = std::process::Command::new("/usr/bin/open")
            .arg("-g").arg("-a").arg(app).arg(url).args(["--args", "--capture-request"]).status();
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn maps_chords_without_capturing_plain_keys_or_system_screenshot() {
            assert_eq!(Shortcut::default().carbon(), Ok((8, (1 << 12) | (1 << 9))));
            assert!(Shortcut { code: "Digit4".into(), control: false, command: true, ..Shortcut::default() }.carbon().is_err());
            assert!(Shortcut { control: false, ..Shortcut::default() }.carbon().is_err());
            assert!(Shortcut { code: "Escape".into(), ..Shortcut::default() }.carbon().is_err());
            assert_eq!(Shortcut { code: "KeyP".into(), option: true, ..Shortcut::default() }.carbon(),
                       Ok((35, (1 << 12) | (1 << 11) | (1 << 9))));
        }
    }
}
