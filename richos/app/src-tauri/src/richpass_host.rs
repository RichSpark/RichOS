//! Native file ownership only. RichPass secrets never enter the RichOS webview or this owner.
use notify::Watcher;
use richos_core::richpass::{self, Manager as Protection, View};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT: &str = "rich://richpass";
pub const NOTICE_EVENT: &str = "rich://richpass-notice";

struct Owner {
    protection: Protection,
    watcher: Option<notify::RecommendedWatcher>,
    watch_error: bool,
}
pub struct Host {
    owner: Mutex<Owner>,
    signal: SyncSender<()>,
    stopped: std::sync::atomic::AtomicBool,
}
impl Host {
    fn view(&self, owner: &Owner) -> View {
        let mut view = owner.protection.view();
        if owner.watch_error && view.path.is_some() && view.phase == "verified" {
            view.message="Folder event monitoring is unavailable. RichOS is checking every 30 seconds while running. Choose Check now to retry.".into();
        }
        if self.stopped.load(std::sync::atomic::Ordering::Acquire) {
            view.supported = false;
            view.phase = "blocked".into();
            view.message="RichPass monitoring could not start. File protection is off. Save a verified copy for standalone use.".into();
        }
        view
    }
}
fn watch(
    path: Option<PathBuf>,
    signal: SyncSender<()>,
) -> Result<Option<notify::RecommendedWatcher>, String> {
    let Some(path) = path else { return Ok(None) };
    let mut watcher = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
        let _sent = signal.try_send(());
    })
    .map_err(|_| "Folder event monitoring could not start.".to_string())?;
    watcher.watch(&path,notify::RecursiveMode::NonRecursive).map_err(|_|"RichOS could not watch this folder. Check folder permissions and choose another local location.".to_string())?;
    Ok(Some(watcher))
}
pub fn start(app: AppHandle) {
    let (signal, events) = mpsc::sync_channel(1);
    let protection = Protection::open(app.state::<crate::AppState>().data_dir.join("richpass"));
    let watcher = watch(protection.parent(), signal.clone());
    let host = Arc::new(Host {
        owner: Mutex::new(Owner {
            protection,
            watch_error: watcher.is_err(),
            watcher: watcher.ok().flatten(),
        }),
        signal,
        stopped: false.into(),
    });
    app.manage(host.clone());
    #[cfg(target_os = "macos")]
    wake::install(&app, host.signal.clone());
    let _sent = host.signal.try_send(());
    let failure_host = host.clone();
    let failure_app = app.clone();
    let worker = std::thread::Builder::new()
        .name("richpass-watch".into())
        .spawn(move || {
            let mut previous = String::new();
            loop {
                let _wake = events.recv_timeout(Duration::from_secs(30));
                if host.stopped.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                // One fixed coalescing window, never extended by a busy stream of events.
                let until = Instant::now() + Duration::from_millis(250);
                while let Some(left) = until.checked_duration_since(Instant::now()) {
                    if events.recv_timeout(left).is_err() {
                        break;
                    }
                }
                let started = Instant::now();
                let mut owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
                let restored = owner.protection.check(richpass::now(), false);
                // Re-arm the parent after replacement, overflow, wake or reconnection.
                match watch(owner.protection.parent(), host.signal.clone()) {
                    Ok(watcher) => {
                        owner.watcher = watcher;
                        owner.watch_error = false;
                    }
                    Err(_) => {
                        owner.watcher = None;
                        owner.watch_error = true;
                    }
                }
                let view = host.view(&owner);
                let changed = serde_json::to_string(&(&view.phase, &view.message, &view.notice))
                    .unwrap_or_default();
                if (changed != previous || restored) && view.path.is_some() {
                    use richdoctor_core::richpass_log::{self, Outcome};
                    let outcome = match view.phase.as_str() {
                        "idle" => Outcome::Idle,
                        "verified" => Outcome::Verified,
                        "paused" => Outcome::Paused,
                        "waiting" => Outcome::Waiting,
                        "blocked" => Outcome::Blocked,
                        "cache_unavailable" => Outcome::SourceUnavailable,
                        _ => Outcome::Unavailable,
                    };
                    richpass_log::record(
                        &app.state::<crate::AppState>().data_dir,
                        richpass::now(),
                        outcome,
                        view.problem,
                        started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                        !owner.watch_error,
                        env!("CARGO_PKG_VERSION"),
                    );
                    // Finite outcomes and monotonic check duration only. No filename or content.
                    eprintln!(
                        "[richos] richpass check phase={} elapsed_ms={} event_watch={} build={}",
                        view.phase,
                        started.elapsed().as_millis(),
                        !owner.watch_error,
                        env!("CARGO_PKG_VERSION")
                    );
                    previous = changed;
                }
                drop(app.emit(EVENT, view));
                if restored {
                    announce_restoration(&app);
                }
            }
        });
    if worker.is_err() {
        failure_host
            .stopped
            .store(true, std::sync::atomic::Ordering::Release);
        richdoctor_core::richpass_log::failure(
            &failure_app.state::<crate::AppState>().data_dir,
            richpass::now(),
            richdoctor_core::richpass_log::FailurePoint::MonitorStart,
            None,
            env!("CARGO_PKG_VERSION"),
        );
    }
}
fn announce_restoration(app: &AppHandle) {
    drop(app.emit(NOTICE_EVENT, ()));
    #[cfg(target_os = "macos")]
    notification::restored(app);
}
pub fn shutdown(app: &AppHandle) {
    if let Some(host) = app.try_state::<Arc<Host>>() {
        host.stopped
            .store(true, std::sync::atomic::Ordering::Release);
        let _sent = host.signal.try_send(());
    }
}
#[richos_command::command]
pub fn richpass_status(host: State<'_, Arc<Host>>) -> View {
    let owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
    host.view(&owner)
}
#[richos_command::command]
pub async fn richpass_save(
    app: AppHandle,
    window: tauri::WebviewWindow,
    protect: bool,
) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        if protect && !cfg!(target_os = "macos") {
            return Err(richpass::Issue::Unsupported.to_string());
        }
        let host = app.state::<Arc<Host>>();
        if protect && host.stopped.load(std::sync::atomic::Ordering::Acquire) {
            return Err(
                "RichPass monitoring is unavailable. Save a verified copy for standalone use."
                    .into(),
            );
        }
        let Some(path) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(if protect {
                "Save and protect RichPass"
            } else {
                "Save RichPass"
            })
            .set_file_name(richpass::FILENAME)
            .set_can_create_directories(true)
            .blocking_save_file()
            .and_then(|f| f.into_path().ok())
        else {
            return Ok(richpass_status(host));
        };
        if protect && !consent(&app, &path) {
            return Ok(richpass_status(host));
        }
        let mut owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
        if protect {
            let watcher = watch(path.parent().map(|p| p.to_path_buf()), host.signal.clone())?;
            owner
                .protection
                .enroll(path, true, richpass::now())
                .map_err(|issue| operation_error(&app, issue))?;
            owner.watcher = watcher;
            owner.watch_error = false;
            #[cfg(target_os = "macos")]
            notification::access(&app);
        } else {
            richpass::save_copy(&path).map_err(|issue| operation_error(&app, issue))?;
        }
        let view = host.view(&owner);
        drop(owner);
        let _sent = host.signal.try_send(());
        Ok(view)
    })
    .await
    .map_err(|_| "The RichPass save operation could not finish.".to_string())?
}
fn operation_error(app: &AppHandle, issue: richpass::Issue) -> String {
    richdoctor_core::richpass_log::failure(
        &app.state::<crate::AppState>().data_dir,
        richpass::now(),
        richdoctor_core::richpass_log::FailurePoint::FileAction,
        Some(issue),
        env!("CARGO_PKG_VERSION"),
    );
    if issue == richpass::Issue::Permission {
        "RichOS needs folder access. Allow RichOS in System Settings > Privacy & Security > Files & Folders, then repeat the Save or Protect action.".into()
    } else if issue == richpass::Issue::Io {
        "The file operation failed. Check free space and folder permissions, then repeat the Save or Protect action.".into()
    } else {
        issue.to_string()
    }
}
#[richos_command::command]
pub fn richpass_permissions() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders")
            .status()
            .map_err(|_| "Could not open folder permissions.")?
            .success()
            .then_some(())
            .ok_or("Could not open folder permissions.".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Folder permissions are managed by your operating system.".into())
    }
}
fn consent(app: &AppHandle, path: &std::path::Path) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    app.dialog().message(format!("Reserve this exact location for RichPass?\n\n{}\n\nWhile RichOS runs, it will automatically replace changed contents and restore RichPass.html after renaming or deletion. Stop protection before editing, moving or deleting it permanently. Other copies are unprotected. An already open browser page cannot be repaired.",path.display())).title("Protect RichPass.html").buttons(MessageDialogButtons::OkCancelCustom("Protect this location".into(),"Cancel".into())).blocking_show()
}
#[richos_command::command]
pub async fn richpass_choose(app: AppHandle, window: tauri::WebviewWindow) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        if !cfg!(target_os = "macos") {
            return Err(richpass::Issue::Unsupported.to_string());
        }
        let host = app.state::<Arc<Host>>();
        if host.stopped.load(std::sync::atomic::Ordering::Acquire) {
            return Err(
                "RichPass monitoring is unavailable. Save a verified copy for standalone use."
                    .into(),
            );
        }
        let Some(path) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Choose RichPass.html to protect")
            .blocking_pick_file()
            .and_then(|f| f.into_path().ok())
        else {
            return Ok(richpass_status(host));
        };
        Protection::recognize(&path).map_err(|issue| operation_error(&app, issue))?;
        if !consent(&app, &path) {
            return Ok(richpass_status(host));
        }
        let mut owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
        let watcher = watch(path.parent().map(|p| p.to_path_buf()), host.signal.clone())?;
        owner
            .protection
            .enroll(path, false, richpass::now())
            .map_err(|issue| operation_error(&app, issue))?;
        owner.watcher = watcher;
        owner.watch_error = false;
        #[cfg(target_os = "macos")]
        notification::access(&app);
        let view = host.view(&owner);
        drop(owner);
        let _sent = host.signal.try_send(());
        Ok(view)
    })
    .await
    .map_err(|_| "The RichPass selection could not finish.".to_string())?
}
#[richos_command::command]
pub fn richpass_check(app: AppHandle, host: State<'_, Arc<Host>>) -> View {
    let mut owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
    let restored = owner.protection.check(richpass::now(), true);
    let view = host.view(&owner);
    drop(owner);
    if restored {
        announce_restoration(&app);
    }
    let _sent = host.signal.try_send(());
    view
}
#[richos_command::command]
pub fn richpass_stop(app: AppHandle, host: State<'_, Arc<Host>>) -> Result<View, String> {
    let mut owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
    owner
        .protection
        .stop()
        .map_err(|issue| operation_error(&app, issue))?;
    owner.watcher = None;
    owner.watch_error = false;
    Ok(host.view(&owner))
}
#[richos_command::command]
pub fn richpass_acknowledge(app: AppHandle, host: State<'_, Arc<Host>>) -> Result<View, String> {
    let mut owner = host.owner.lock().unwrap_or_else(|p| p.into_inner());
    owner
        .protection
        .acknowledge()
        .map_err(|issue| operation_error(&app, issue))?;
    Ok(host.view(&owner))
}
#[richos_command::command]
pub fn richpass_show(host: State<'_, Arc<Host>>) -> Result<(), String> {
    let path = host
        .owner
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .protection
        .view()
        .path
        .ok_or("No protected RichPass location.")?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|_| "Could not show the RichPass folder.")?
            .success()
            .then_some(())
            .ok_or("Could not show the RichPass folder.".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        drop(path);
        Err("Show in Folder is unavailable on this platform.".into())
    }
}

#[cfg(target_os = "macos")]
mod wake {
    use super::*;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    // AppKit retains the observer and copies its block for this process's lifetime.
    // It captures only the bounded signal, not an app, a path or any page data.
    pub fn install(app: &AppHandle, signal: SyncSender<()>) {
        drop(app.run_on_main_thread(move ||objc2::rc::autoreleasepool(|_|unsafe{
            let workspace:*mut AnyObject=msg_send![AnyClass::get(c"NSWorkspace").unwrap(),sharedWorkspace];
            let center:*mut AnyObject=msg_send![workspace,notificationCenter];
            let name:*mut AnyObject=msg_send![AnyClass::get(c"NSString").unwrap(),stringWithUTF8String:c"NSWorkspaceDidWakeNotification".as_ptr()];
            let block=block2::RcBlock::new(move |_:*mut AnyObject|{let _sent=signal.try_send(());});
            let _: *mut AnyObject=msg_send![center,addObserverForName:name, object:std::ptr::null::<AnyObject>(), queue:std::ptr::null::<AnyObject>(), usingBlock:&*block];
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parent_monitor_receives_an_atomic_replacement_event() {
        let root = std::env::temp_dir().join(format!(
            "richpass-event-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                drop(std::fs::remove_dir_all(&self.0));
            }
        }
        let _cleanup = Cleanup(root.clone());
        std::fs::write(root.join(richpass::FILENAME), richpass::BUNDLED).unwrap();
        let (signal, events) = mpsc::sync_channel(1);
        let watcher = watch(Some(root.clone()), signal).unwrap();
        std::fs::write(root.join("replacement"), b"changed").unwrap();
        std::fs::rename(root.join("replacement"), root.join(richpass::FILENAME)).unwrap();
        // load-bound: wait for an actual OS event; 30 seconds is a hang guard, not a latency assertion.
        events
            .recv_timeout(Duration::from_secs(30))
            .expect("the parent watcher must survive file replacement");
        drop(watcher);
    }
}

#[cfg(target_os = "macos")]
mod notification {
    use super::*;
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject, Bool},
    };
    #[link(name = "UserNotifications", kind = "framework")]
    extern "C" {}
    /// A bare development binary has no notification identity. Skip this optional
    /// capability rather than calling an API that requires an application bundle.
    unsafe fn center() -> Option<*mut AnyObject> {
        let bundle_class = AnyClass::get(c"NSBundle")?;
        let bundle: *mut AnyObject = msg_send![bundle_class, mainBundle];
        let identifier: *mut AnyObject = msg_send![bundle, bundleIdentifier];
        if identifier.is_null() {
            return None;
        }
        let class = AnyClass::get(c"UNUserNotificationCenter")?;
        let center: *mut AnyObject = msg_send![class, currentNotificationCenter];
        if center.is_null() {
            None
        } else {
            Some(center)
        }
    }
    /// Permission is optional and requested only after explicit successful enrollment.
    pub fn access(app: &AppHandle) {
        drop(app.run_on_main_thread(||objc2::rc::autoreleasepool(|_|unsafe{
            let Some(center)=center() else{return;};
            let done=block2::RcBlock::new(|granted:Bool,_:*mut AnyObject|{
                eprintln!("[richos] richpass notification_permission={}",granted.as_bool());
            });
            let _:()=msg_send![center,requestAuthorizationWithOptions:4usize, completionHandler:&*done];
        })));
    }
    /// One stable identifier replaces an older pending/delivered notification. No path or secret.
    pub fn restored(app: &AppHandle) {
        drop(app.run_on_main_thread(||objc2::rc::autoreleasepool(|_|unsafe{
            let (Some(center),Some(content_class),Some(request_class),Some(string_class))=(center(),AnyClass::get(c"UNMutableNotificationContent"),AnyClass::get(c"UNNotificationRequest"),AnyClass::get(c"NSString")) else{return;};
            let raw:*mut AnyObject=msg_send![content_class,new];let Some(content)=Retained::from_raw(raw) else{return;};
            let title:*mut AnyObject=msg_send![string_class,stringWithUTF8String:c"RichPass file restored".as_ptr()];
            let body:*mut AnyObject=msg_send![string_class,stringWithUTF8String:c"Close open RichPass tabs and reopen the verified file. Review the notice in Settings > RichPass.".as_ptr()];
            let identifier:*mut AnyObject=msg_send![string_class,stringWithUTF8String:c"richpass-restored".as_ptr()];
            let _:()=msg_send![&*content,setTitle:title];let _:()=msg_send![&*content,setBody:body];
            let request:*mut AnyObject=msg_send![request_class,requestWithIdentifier:identifier, content:&*content, trigger:std::ptr::null::<AnyObject>()];
            let done=block2::RcBlock::new(|error:*mut AnyObject|{eprintln!("[richos] richpass notification_delivered_to_center={}",error.is_null());});
            let _:()=msg_send![center,addNotificationRequest:request, withCompletionHandler:&*done];
        })));
    }
}
