//! The native launch overlay owns the full timeline. The page only waits for
//! playback and the fade to end, then releases Home's animation.
use std::cell::Cell;
use std::ffi::{c_char, c_void, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Window};
use tokio::sync::{oneshot, watch};

use crate::SplashPlayback;

extern "C" {
    fn richos_splash_reduce_motion() -> bool;
    fn richos_splash_prepare(
        path: *const c_char,
        frames: i32,
        sound: *const c_char,
        phase: extern "C" fn(i32, f64),
        metrics: extern "C" fn(u64, u64, u64),
        audio_outcome: extern "C" fn(i32),
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
struct InterruptedPlayback {
    started_at_ms: u64,
    elapsed_ms: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Preparing,
    Playing { start: Instant, started_at_ms: u64 },
    Finished,
    Failed(Option<InterruptedPlayback>),
}

impl Phase {
    fn owns_cue(self) -> bool {
        matches!(self, Self::Playing { .. } | Self::Finished | Self::Failed(Some(_)))
    }

    fn started_at_ms(self) -> Option<u64> {
        match self {
            Self::Playing { started_at_ms, .. } => Some(started_at_ms),
            Self::Failed(Some(playback)) => Some(playback.started_at_ms),
            _ => None,
        }
    }

    fn elapsed_ms(self) -> u64 {
        match self {
            Self::Playing { start, .. } => start.elapsed().as_millis() as u64,
            Self::Failed(Some(playback)) => playback.elapsed_ms,
            _ => 0,
        }
    }
}

thread_local! {
    static PLAYER: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}
static PHASE: OnceLock<watch::Sender<Phase>> = OnceLock::new();

fn phase() -> &'static watch::Sender<Phase> {
    PHASE.get_or_init(|| watch::channel(Phase::Preparing).0)
}

extern "C" fn on_phase(code: i32, anchor: f64) {
    match code {
        1 => {
            let start = Instant::now();
            crate::launch_timing::note_native_splash_started(anchor);
            phase().send_replace(Phase::Playing { start, started_at_ms: anchor as u64 });
        }
        2 => {
            crate::launch_timing::note_native_splash_outcome("reached authored endpoint");
            phase().send_replace(Phase::Finished);
        }
        _ => {
            fail(match code {
                -2 => "cover left the window",
                -3 => "renderer missed the authored endpoint",
                _ => "renderer failed",
            });
        }
    }
}

extern "C" fn on_metrics(total: u64, dropped: u64, corrupted: u64) {
    crate::launch_timing::note_native_splash_outcome(
        &format!("renderer counted {total} frames, {dropped} dropped, {corrupted} corrupted"),
    );
}

extern "C" fn on_audio_outcome(code: i32) {
    let outcome = match code {
        0 => "audio cue disabled",
        1 => "audio cue scheduled",
        2 => "audio cue preparation failed",
        3 => "audio cue deadline missed",
        4 => "audio cue scheduling refused",
        5 => "audio cue completed",
        6 => "audio cue interrupted",
        7 => "audio cue completion timed out",
        8 => "audio output ready",
        _ => return,
    };
    crate::launch_timing::note_native_splash_outcome(outcome);
}

fn fail(reason: &'static str) {
    let previous = *phase().borrow();
    let elapsed_ms = previous.elapsed_ms();
    crate::launch_timing::note_native_splash_outcome(
        &format!("{reason}, falling back at {elapsed_ms} ms of playback"),
    );
    let playback = previous.started_at_ms().map(|started_at_ms| InterruptedPlayback { started_at_ms, elapsed_ms });
    phase().send_replace(Phase::Failed(playback));
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

// SAFETY: a prepared player owns sample buffers but has no AppKit view or layer.
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
                on_metrics,
                on_audio_outcome,
            )
        };
        NonNull::new(player).map(Prepared)
    }).ok()
}

pub fn show_on_window(window: &Window, worker: JoinHandle<Option<Prepared>>) -> bool {
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
    fail("could not attach the cover");
}

/// The overlay has either started after its first frame became ready or needs the page fallback.
pub async fn wait_for_start() {
    let mut receiver = phase().subscribe();
    drop(tokio::time::timeout(
        Duration::from_secs(2),
        receiver.wait_for(|value| *value != Phase::Preparing),
    ).await);
}

pub async fn finished() -> SplashPlayback {
    wait_for_end(phase().subscribe()).await
}

async fn wait_for_end(mut receiver: watch::Receiver<Phase>) -> SplashPlayback {
    let terminal = tokio::time::timeout(
        Duration::from_secs(10),
        receiver.wait_for(|value| matches!(value, Phase::Finished | Phase::Failed(_))),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .map(|value| *value);
    let phase = terminal.unwrap_or_else(|| {
        crate::launch_timing::note_native_splash_outcome("timed out waiting for the renderer");
        *receiver.borrow()
    });
    match phase {
        Phase::Finished => SplashPlayback::Finished,
        phase => SplashPlayback::Fallback {
            elapsed_ms: phase.elapsed_ms(),
            native_cue: phase.owns_cue(),
            started_at_ms: phase.started_at_ms(),
        },
    }
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
            let started_at_ms = 1795262400000;
            sender.send_replace(Phase::Playing { start: Instant::now(), started_at_ms });
            tokio::task::yield_now().await;
            assert!(!done.is_finished());
            sender.send_replace(Phase::Finished);
            assert_eq!(done.await.unwrap(), SplashPlayback::Finished);

            for (elapsed, native_cue) in [(None, false), (Some(0), true), (Some(1350), true)] {
                let failure = elapsed.map(|elapsed_ms| InterruptedPlayback { started_at_ms, elapsed_ms });
                let (_, receiver) = watch::channel(Phase::Failed(failure));
                assert_eq!(
                    wait_for_end(receiver).await,
                    SplashPlayback::Fallback {
                        elapsed_ms: elapsed.unwrap_or_default(),
                        native_cue,
                        started_at_ms: elapsed.map(|_| started_at_ms),
                    },
                );
            }
        });
    }
}
