//! The native launch overlay owns the full timeline. The page only waits for
//! playback and the fade to end, then releases Home's animation.
use std::cell::Cell;
use std::ffi::{c_char, c_void, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::{oneshot, watch};

extern "C" {
    fn richos_splash_reduce_motion() -> bool;
    fn richos_splash_prepare(
        path: *const c_char,
        frames: i32,
        sound: *const c_char,
        phase: extern "C" fn(i32, f64),
    ) -> *mut c_void;
    fn richos_splash_discard_prepared(player: *mut c_void);
    fn richos_splash_attach(window: *mut c_void, player: *mut c_void) -> bool;
    fn richos_splash_window_visible(player: *mut c_void);
    fn richos_splash_remove(
        player: *mut c_void,
        animated: bool,
        done: Option<extern "C" fn(*mut c_void)>,
        data: *mut c_void,
    );
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Preparing,
    Playing,
    Finished,
    Failed,
}

thread_local! {
    static PLAYER: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}
static PHASE: OnceLock<watch::Sender<Phase>> = OnceLock::new();

fn phase() -> &'static watch::Sender<Phase> {
    PHASE.get_or_init(|| watch::channel(Phase::Preparing).0)
}

extern "C" fn on_phase(code: i32, anchor: f64) {
    let value = match code {
        1 => {
            crate::launch_timing::note_native_splash_started(anchor);
            Phase::Playing
        }
        2 => {
            eprintln!("[richos] native splash: reached authored endpoint");
            Phase::Finished
        }
        _ => {
            eprintln!("[richos] native splash: renderer failed");
            Phase::Failed
        }
    };
    phase().send_replace(value);
}

pub fn reduce_motion() -> bool {
    // SAFETY: a read of the workspace's accessibility setting on the AppKit thread.
    unsafe { richos_splash_reduce_motion() }
}

pub struct Pack {
    path: PathBuf,
    frames: i32,
    sound: Option<PathBuf>,
    pub bar: Option<[f64; 4]>,
}

pub fn locate(app: &AppHandle, data: &Path, entry: &crate::splash_pick::Entry, sound: bool) -> Option<Pack> {
    let id = entry.pack_id()?;
    let frames = match entry.seconds {
        3.0 => 180,
        5.0 => 300,
        _ => return None,
    };
    let bundle = app.path().resource_dir().ok()?.join("Splash");
    let path = crate::splash_assets::local(app, data, id)?;
    let numbered = crate::openings::catalog().get(id).is_some_and(|opening| opening.figure.is_some());
    let sound = sound.then(|| bundle.join(if numbered { "two-note-grand.wav" } else { "two-note.wav" }));
    let bar = crate::splash_assets::bar(id, &path);
    Some(Pack { path, frames, sound, bar })
}

pub struct Prepared(NonNull<c_void>);

// SAFETY: a prepared player owns decoded sample buffers but has no AppKit view or layer.
// The join transfers its sole retained handle to the main thread before attachment.
unsafe impl Send for Prepared {}

impl Drop for Prepared {
    fn drop(&mut self) {
        // SAFETY: this retains ownership only until successful AppKit attachment.
        unsafe { richos_splash_discard_prepared(self.0.as_ptr()) };
    }
}

pub fn prepare(pack: Pack) -> Option<JoinHandle<Option<Prepared>>> {
    std::thread::Builder::new().name("richos-splash-prepare".into()).spawn(move || {
        let path = CString::new(pack.path.as_os_str().as_bytes()).ok()?;
        let sound = pack.sound.as_ref().and_then(|path| CString::new(path.as_os_str().as_bytes()).ok());
        // SAFETY: the native preparation owns no AppKit view and reads both strings in this call.
        let player = unsafe {
            richos_splash_prepare(
                path.as_ptr(),
                pack.frames,
                sound.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                on_phase,
            )
        };
        NonNull::new(player).map(Prepared)
    }).ok()
}

pub fn show(window: &WebviewWindow, worker: JoinHandle<Option<Prepared>>) -> bool {
    let Ok(window) = window.ns_window() else { return false };
    let Ok(Some(prepared)) = worker.join() else { return false };
    let player = prepared.0.as_ptr();
    // SAFETY: called on the AppKit thread with this window and the prepared player.
    if !unsafe { richos_splash_attach(window, player) } {
        return false;
    }
    std::mem::forget(prepared);
    PLAYER.set(player);
    true
}

pub fn window_visible() {
    let player = PLAYER.get();
    if !player.is_null() {
        // SAFETY: the main thread owns the retained handle until the fade.
        unsafe { richos_splash_window_visible(player) };
    }
}

pub fn did_not_attach() {
    phase().send_replace(Phase::Failed);
}

pub async fn finished() -> bool {
    wait_for_end(phase().subscribe()).await
}

async fn wait_for_end(mut receiver: watch::Receiver<Phase>) -> bool {
    let finished = matches!(
        tokio::time::timeout(Duration::from_secs(10), receiver.wait_for(|value| {
            matches!(value, Phase::Finished | Phase::Failed)
        }))
        .await,
        Ok(Ok(value)) if *value == Phase::Finished
    );
    finished
}

extern "C" fn faded(data: *mut c_void) {
    // SAFETY: `dismiss` passes one Box and the native player calls this once.
    let sender = unsafe { Box::from_raw(data.cast::<oneshot::Sender<()>>()) };
    let _sent = sender.send(());
}

pub async fn dismiss(app: &AppHandle, animated: bool) {
    let (send, done) = oneshot::channel::<()>();
    if app
        .run_on_main_thread(move || {
            let handle = PLAYER.replace(std::ptr::null_mut());
            if !handle.is_null() {
                let data = Box::into_raw(Box::new(send)).cast();
                // SAFETY: release the retained player exactly once, on the AppKit thread.
                unsafe { richos_splash_remove(handle, animated, Some(faded), data) };
            }
        })
        .is_ok()
    {
        done.await.unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_waits_for_the_native_endpoint_and_can_recover_from_failure() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        rt.block_on(async {
            let (sender, receiver) = watch::channel(Phase::Preparing);
            let done = tokio::spawn(wait_for_end(receiver));
            sender.send_replace(Phase::Playing);
            tokio::task::yield_now().await;
            assert!(!done.is_finished());
            sender.send_replace(Phase::Finished);
            assert!(done.await.unwrap());

            let (_, receiver) = watch::channel(Phase::Failed);
            assert!(!wait_for_end(receiver).await);
        });
    }
}
