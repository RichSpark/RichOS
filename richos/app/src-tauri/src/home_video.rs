//! **"Make a video" on the home screen** (round 77, CEO 2026-11-14: "`round-77` is great. Get an
//! engineer on Fable to build it."). The design is `richos-hq/design/mockups/rounds/round-77`; the
//! page's part is `ui/home-video.js`, which records the home screen into a file in the window.
//!
//! This file is the two things the window cannot do itself:
//!
//! - [`home_video_save`]: "Save video". The round's README, "What is real and what is stand-in":
//!   a browser can only put the file in Downloads; "in the app this is where the Mac's own save
//!   window would open". The sheet is `tauri-plugin-dialog` run HERE, from Rust, as
//!   `output_files::output_save_copy` runs it; none of the plugin's commands is granted to the page.
//! - [`home_video_input`]: what the "Microphone" choice records. The microphone, except in a test
//!   run started with [`TONE_ENV`] set, where the page makes a quiet tone that goes into the file
//!   only (the round's own `mic=fake`), as `RICHOS_VOICE_INPUT_WAV` stands in for the microphone in
//!   voice (`capture.rs`): a check on a Mac plays no sound and opens no microphone.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Set (to anything but empty) in a test run: the "Microphone" choice records a made-up tone.
pub const TONE_ENV: &str = "RICHOS_VIDEO_INPUT_TONE";

/// Shown on the sheet when the file could not be written where he pointed.
pub const WOULD_NOT_SAVE: &str = "The video could not be saved there. Choose another folder.";
/// Shown on the sheet when the window handed over no video at all.
pub const NO_VIDEO: &str = "There is no video to save. Record it again.";

/// What "Save video" answers: whether a file was written, and under which name in which folder
/// ("Saved to Desktop as RichOS-day-67-312-memories.mp4"). Not saved with no error is his Cancel.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SavedVideo {
    pub saved: bool,
    pub name: String,
    pub folder: String,
}

/// `"tone"` in a test run ([`TONE_ENV`]), `"microphone"` for everybody else.
pub fn input_kind(tone: Option<&str>) -> &'static str {
    if tone.is_some_and(|v| !v.is_empty()) { "tone" } else { "microphone" }
}

#[richos_command::command]
pub fn home_video_input() -> &'static str {
    input_kind(std::env::var(TONE_ENV).ok().as_deref())
}

#[derive(Default)]
struct CleanerFrames {
    recording: u32,
    captured: u32,
    missed: u32,
    elapsed: Duration,
    slowest: Duration,
}

impl CleanerFrames {
    fn add(&mut self, recording: u32, captured: bool, elapsed: Duration) {
        if self.recording != recording {
            *self = Self { recording, ..Self::default() };
        }
        self.captured += u32::from(captured);
        self.missed += u32::from(!captured);
        self.elapsed += elapsed;
        self.slowest = self.slowest.max(elapsed);
    }

    fn excerpt(&self) -> Option<String> {
        let total = self.captured + self.missed;
        (total > 0).then(|| {
            let build = option_env!("RICHOS_SOURCE_SHA").unwrap_or(env!("CARGO_PKG_VERSION"));
            format!("{build} · recording {} · {} captured, {} unavailable · average {} ms, slowest {} ms from request to JPEG",
                self.recording, self.captured, self.missed, self.elapsed.as_millis() / u128::from(total), self.slowest.as_millis())
        })
    }
}

static CLEANER_FRAMES: Mutex<CleanerFrames> = Mutex::new(CleanerFrames {
    recording: 0, captured: 0, missed: 0, elapsed: Duration::ZERO, slowest: Duration::ZERO,
});

pub(crate) fn recent_cleaner_capture() -> Option<String> {
    CLEANER_FRAMES.lock().ok()?.excerpt()
}

/// The main recorder can sample the fixed RichCleaner library window as long as it has focus.
#[richos_command::command]
pub async fn cleaner_recording_frame(window: tauri::WebviewWindow, recording: u32) -> tauri::ipc::Response {
    use tauri::Manager;
    if window.label() == "main" {
        if let Some(cleaner) = window.app_handle().get_webview_window("cleaner") {
            let start = Instant::now();
            let result = crate::window_picture::take_focused(&cleaner).await;
            if !matches!(result, Ok(None)) {
                if let Ok(mut frames) = CLEANER_FRAMES.lock() {
                    frames.add(recording, result.is_ok(), start.elapsed());
                }
            }
            return tauri::ipc::Response::new(result.ok().flatten().unwrap_or_default());
        }
    }
    tauri::ipc::Response::new(Vec::new())
}

/// RichFiles pictures its own key window, with the same frame format as RichCleaner.
#[richos_command::command]
pub async fn richfiles_recording_frame(window: tauri::WebviewWindow) -> tauri::ipc::Response {
    #[cfg(target_os = "macos")]
    if window.label() == "main" {
        let frame = tauri::async_runtime::spawn_blocking(richfiles_frame::take)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        return tauri::ipc::Response::new(frame);
    }
    #[cfg(not(target_os = "macos"))]
    let _window = window;
    tauri::ipc::Response::new(Vec::new())
}

#[cfg(target_os = "macos")]
mod richfiles_frame {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use std::ffi::{CStr, OsStr};
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::{ffi::OsStrExt, net::UnixStream};
    use std::path::Path;
    use std::time::Duration;

    // Darwin's private-per-user temporary directory, independent of the
    // caller's environment. This is _CS_DARWIN_USER_TEMP_DIR in unistd.h.
    const USER_TEMP_DIR: libc::c_int = 65_537;

    pub fn take() -> Option<Vec<u8>> {
        let pid = frontmost_richfiles()?;
        let mut path = [0u8; 1024];
        let size = unsafe { libc::confstr(USER_TEMP_DIR, path.as_mut_ptr().cast(), path.len()) };
        if !(2..=path.len()).contains(&size) {
            return None;
        }
        let directory = Path::new(OsStr::from_bytes(&path[..size - 1]));
        let stream = UnixStream::connect(directory.join(format!("richfiles-video.{pid}"))).ok()?;
        let mut peer: libc::pid_t = 0;
        let mut size = std::mem::size_of_val(&peer) as libc::socklen_t;
        // SAFETY: LOCAL_PEERPID writes a pid_t into our initialized buffer.
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                std::ptr::from_mut(&mut peer).cast(),
                &mut size,
            )
        };
        if result != 0 || size as usize != std::mem::size_of_val(&peer) || peer != pid {
            return None;
        }
        stream
            .set_read_timeout(Some(Duration::from_millis(400)))
            .ok()?;
        let limit = crate::window_picture::MAX_BYTES + 8;
        let mut frame = Vec::new();
        stream
            .take((limit + 1) as u64)
            .read_to_end(&mut frame)
            .ok()?;
        // Recheck focus after we receive the frame, so leaving RichFiles while
        // it draws a picture does not capture a late frame of that window.
        if frame.len() > limit
            || !frame
                .get(8..)
                .is_some_and(|jpeg| jpeg.starts_with(&[0xff, 0xd8]))
            || frontmost_richfiles() != Some(pid)
        {
            return None;
        }
        static RECEIVED: std::sync::Once = std::sync::Once::new();
        RECEIVED.call_once(|| eprintln!("[richos] video: received a picture of the active RichFiles window"));
        Some(frame)
    }

    fn frontmost_richfiles() -> Option<i32> {
        objc2::rc::autoreleasepool(|_| {
            let workspace_class = AnyClass::get(c"NSWorkspace")?;
            // SAFETY: documented class method and NSRunningApplication/NSString
            // accessors. The identifier is read inside the owning autorelease pool.
            unsafe {
                let workspace: *mut AnyObject = msg_send![workspace_class, sharedWorkspace];
                let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
                if app.is_null() {
                    return None;
                }
                let identifier: *mut AnyObject = msg_send![app, bundleIdentifier];
                if identifier.is_null() {
                    return None;
                }
                let name: *const libc::c_char = msg_send![identifier, UTF8String];
                if name.is_null() || CStr::from_ptr(name).to_bytes() != b"app.richos.richfiles" {
                    return None;
                }
                let pid: i32 = msg_send![app, processIdentifier];
                (pid > 0).then_some(pid)
            }
        })
    }
}

/// Write the video where `choose` points. `choose` is the system save sheet in the app and a
/// closure in the tests; it is given the file name to suggest. The bytes go to a new hidden file
/// in the destination folder first and are renamed over the chosen name, so a save that fails
/// part-way leaves nothing half-written under that name.
pub fn save_video(bytes: &[u8], name: &str, choose: impl FnOnce(&str) -> Option<PathBuf>) -> Result<SavedVideo, String> {
    if bytes.is_empty() {
        return Err(NO_VIDEO.to_string());
    }
    let suggested = crate::output_files::suggested_name(name);
    let Some(path) = choose(&suggested) else {
        return Ok(SavedVideo { saved: false, name: String::new(), folder: String::new() });
    };
    let (Some(folder), Some(file)) = (path.parent(), path.file_name()) else { return Err(WOULD_NOT_SAVE.to_string()) };
    let part = folder.join(format!(".{}.{}.part", file.to_string_lossy(), std::process::id()));
    let written = std::fs::write(&part, bytes).and_then(|()| std::fs::rename(&part, &path));
    if let Err(e) = written {
        eprintln!("[richos] home video: could not save to {}: {e}", path.display());
        // the part file may not exist (the write itself failed): nothing more to do about it
        drop(std::fs::remove_file(&part));
        return Err(WOULD_NOT_SAVE.to_string());
    }
    eprintln!("[richos] home video: saved {} byte(s) to {}", bytes.len(), path.display());
    Ok(SavedVideo { saved: true, name: file.to_string_lossy().into_owned(), folder: folder_name(folder) })
}

/// The folder as he would say it: its own name ("Desktop"), or the whole path for a volume's root.
fn folder_name(folder: &Path) -> String {
    folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.display().to_string())
}

/// `home_video_save`: the body is the video's raw bytes; the header `x-richos-name` is the file
/// name to suggest, percent-encoded. The system save sheet, attached to the window that asked,
/// then the file.
///
/// The sheet blocks until he answers it. A window command's body runs on one of the runtime's
/// workers with its core handed on (`richos_command::command`, `block_in_place`), never on the
/// main thread, which is the plugin's own rule for `blocking_save_file`; nothing else waits on it.
///
/// Every setting of the sheet, chosen rather than inherited (CEO, 2026-09-10):
/// - parent: the asking window, so it is a sheet on the app;
/// - title: *Save video*, the words on the button he pressed;
/// - file name: the name the page built, which says what the video is;
/// - can create folders: yes, as every Mac save sheet lets you;
/// - starting folder: not set, so macOS opens it where this app last saved;
/// - filters: none: the file keeps the extension the recorder gave it.
#[richos_command::command]
pub fn home_video_save(app: tauri::AppHandle, window: tauri::WebviewWindow, request: tauri::ipc::Request<'_>) -> Result<SavedVideo, String> {
    use tauri_plugin_dialog::DialogExt;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err(NO_VIDEO.to_string()) };
    let name = request.headers().get("x-richos-name").and_then(|v| v.to_str().ok()).and_then(percent_decode).unwrap_or_default();
    save_video(bytes, &name, |suggested| {
        app.dialog()
            .file()
            .set_parent(&window)
            .set_title("Save video")
            .set_file_name(suggested)
            .set_can_create_directories(true)
            .blocking_save_file()
            .and_then(|chosen| chosen.into_path().ok())
    })
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cleaner_window_timings_describe_only_the_latest_recording() {
        let mut frames = CleanerFrames::default();
        assert_eq!(frames.excerpt(), None);
        frames.add(1, true, Duration::from_millis(20));
        frames.add(1, false, Duration::from_millis(400));
        assert!(frames.excerpt().unwrap().ends_with(
            "recording 1 · 1 captured, 1 unavailable · average 210 ms, slowest 400 ms from request to JPEG"
        ));
        frames.add(2, true, Duration::from_millis(50));
        assert!(frames.excerpt().unwrap().ends_with(
            "recording 2 · 1 captured, 0 unavailable · average 50 ms, slowest 50 ms from request to JPEG"
        ));
    }

    fn dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!("richos-home-video-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn the_video_is_written_where_the_sheet_pointed_under_the_name_it_was_given() {
        let d = dir("saved").join("Desktop");
        std::fs::create_dir_all(&d).unwrap();
        let mut asked = String::new();
        let saved = save_video(b"\0\0\0\x1cftypisom-a-video", "RichOS-day-67-312-memories.mp4", |name| {
            asked = name.to_string();
            Some(d.join("my video.mp4"))
        })
        .unwrap();
        assert_eq!(asked, "RichOS-day-67-312-memories.mp4", "the sheet suggests the name the page built");
        assert_eq!(saved, SavedVideo { saved: true, name: "my video.mp4".into(), folder: "Desktop".into() });
        assert_eq!(std::fs::read(d.join("my video.mp4")).unwrap(), b"\0\0\0\x1cftypisom-a-video");
        let left: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left.len(), 1, "nothing but the video is left in the folder: {left:?}");
    }

    #[test]
    fn his_cancel_writes_nothing_and_is_not_an_error() {
        let d = dir("cancel");
        let saved = save_video(b"video", "RichOS-312-memories.mp4", |_| None).unwrap();
        assert!(!saved.saved);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
    }

    #[test]
    fn a_folder_that_cannot_be_written_says_so_and_leaves_no_part_file() {
        let d = dir("gone");
        let answer = save_video(b"video", "RichOS-312-memories.mp4", |_| Some(d.join("not-a-folder").join("v.mp4")));
        assert_eq!(answer, Err(WOULD_NOT_SAVE.to_string()));
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
    }

    #[test]
    fn no_video_never_opens_the_sheet() {
        let answer = save_video(b"", "RichOS-312-memories.mp4", |_| panic!("the sheet opened for an empty video"));
        assert_eq!(answer, Err(NO_VIDEO.to_string()));
    }

    #[test]
    fn a_name_with_a_folder_in_it_is_suggested_as_its_last_part() {
        let mut asked = String::new();
        let answer = save_video(b"v", "../../etc/RichOS.mp4", |name| {
            asked = name.to_string();
            None
        });
        assert_eq!(answer.map(|v| v.saved), Ok(false));
        assert_eq!(asked, "RichOS.mp4");
    }

    #[test]
    fn the_tone_stands_in_only_in_a_run_that_asks_for_it() {
        assert_eq!(input_kind(None), "microphone");
        assert_eq!(input_kind(Some("")), "microphone");
        assert_eq!(input_kind(Some("1")), "tone");
    }

    #[test]
    fn the_name_header_is_decoded() {
        assert_eq!(percent_decode("RichOS-day-67-312-memories.mp4").as_deref(), Some("RichOS-day-67-312-memories.mp4"));
        assert_eq!(percent_decode("a%20b.mp4").as_deref(), Some("a b.mp4"));
        assert_eq!(percent_decode("a%2"), None);
    }
}
