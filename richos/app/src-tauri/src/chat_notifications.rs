//! Desktop notification delivery and activation. No chat content goes into diagnostics.
use crate::{AppState, ThreadRow};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Question,
    Stopped,
    Finished,
    Back,
}

impl Kind {
    fn presentation(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Question => (
                "Rich needs you",
                "There’s a question waiting for you.",
                "satisfying-triple-bubble-pop.mp3",
            ),
            Self::Stopped => (
                "Work stopped",
                "Open the chat to see what happened.",
                "perforated-paper-towel-tear-sound-effect-2-slow-1.0-seconds.mp3",
            ),
            Self::Finished => (
                "Work finished",
                "The result is in your chat.",
                "satisfying-stamp-sound.mp3",
            ),
            Self::Back => (
                "Back as you asked",
                "This chat is ready to look at again.",
                "BRRR-Plop-pass-sound-1-second.mp3",
            ),
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{
        ffi::{c_char, CStr, CString},
        sync::{Mutex, OnceLock},
    };

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static TARGET: Mutex<Option<String>> = Mutex::new(None);

    extern "C" {
        fn richos_chat_notifications_install();
        fn richos_chat_notification(
            thread: *const c_char,
            identifier: *const c_char,
            title: *const c_char,
            body: *const c_char,
            file: *const c_char,
            show: bool,
            play_when_active: bool,
        );
        fn richos_chat_answer_sound(file: *const c_char);
    }

    pub(super) fn install(app: &AppHandle) {
        if APP.set(app.clone()).is_err() {
            return;
        }
        drop(app.run_on_main_thread(|| unsafe { richos_chat_notifications_install() }));
    }

    #[no_mangle]
    extern "C" fn richos_chat_clock_changed() {
        if let Some(app) = APP.get() {
            drop(app.emit("rich://chat-list-clock-changed", ()));
        }
    }

    #[no_mangle]
    extern "C" fn richos_chat_notification_observation(event: u32) {
        let (stage, outcome) = match event {
            0 => ("sound", "trace-only"),
            1 => ("sound", "started"),
            2 => ("sound", "failed"),
            3 => ("sound", "unavailable"),
            4 => ("notification", "unavailable"),
            5 => ("notification", "permission-error"),
            6 => ("notification", "permission-denied"),
            7 => ("notification", "delivery-error"),
            8 => ("notification", "submitted"),
            _ => return,
        };
        crate::chat_list::record(stage, outcome, None);
    }

    #[no_mangle]
    unsafe extern "C" fn richos_open_chat_notification(thread: *const c_char) {
        let Some(app) = APP.get() else { return };
        let Ok(id) = CStr::from_ptr(thread).to_str() else {
            return;
        };
        *TARGET.lock().unwrap() = Some(id.to_owned());
        let cloned = app.clone();
        drop(app.run_on_main_thread(move || {
            crate::reopen_window(&cloned);
            drop(cloned.emit("rich://open-chat-notification", ()));
        }));
    }

    pub(super) fn take_target() -> Option<String> {
        TARGET.lock().unwrap().take()
    }

    pub(super) fn post(app: &AppHandle, row: &ThreadRow, at: u64, kind: Kind) {
        let Ok(thread) = CString::new(row.id.as_str()) else {
            return;
        };
        let (heading, body, file) = kind.presentation();
        let title = format!("{heading} · {}", row.display_title.replace('\0', ""));
        let identifier =
            CString::new(format!("chat-{heading}-{at}-{}", thread.to_string_lossy())).unwrap();
        let title = CString::new(title).unwrap();
        let body = CString::new(body).unwrap();
        let play_when_active = matches!(kind, Kind::Back);
        let handle = app.clone();
        drop(app.run_on_main_thread(move || {
            let state = handle.state::<AppState>();
            let nav = state.nav.lock().unwrap();
            let show = nav.state().mac_chat_notifications;
            let file = CString::new(if nav.state().chat_notification_sounds {
                file
            } else {
                ""
            })
            .unwrap();
            drop(nav);
            unsafe {
                richos_chat_notification(
                    thread.as_ptr(),
                    identifier.as_ptr(),
                    title.as_ptr(),
                    body.as_ptr(),
                    file.as_ptr(),
                    show,
                    play_when_active,
                )
            };
        }));
    }

    pub(super) fn answer(app: &AppHandle) {
        let handle = app.clone();
        drop(app.run_on_main_thread(move || {
            let state = handle.state::<AppState>();
            if state.nav.lock().unwrap().state().chat_notification_sounds {
                unsafe { richos_chat_answer_sound(c"satisfying-double-pop.mp3".as_ptr()) };
            }
        }));
    }
}

pub(crate) fn install(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    mac::install(app);
}

pub(crate) fn post(app: &AppHandle, row: &ThreadRow, at: u64, kind: Kind) {
    #[cfg(target_os = "macos")]
    mac::post(app, row, at, kind);
}

#[richos_command::command]
pub(crate) fn play_chat_answer_sound(app: AppHandle) {
    #[cfg(target_os = "macos")]
    mac::answer(&app);
}

#[richos_command::command]
pub(crate) fn take_chat_notification_target() -> Option<String> {
    #[cfg(target_os = "macos")]
    return mac::take_target();
    #[cfg(not(target_os = "macos"))]
    None
}

#[richos_command::command]
pub(crate) fn set_chat_notifications(
    state: tauri::State<AppState>,
    mac: Option<bool>,
    sounds: Option<bool>,
) -> Result<(), String> {
    state
        .nav
        .lock()
        .unwrap()
        .update_chat_list(|prefs| {
            if let Some(mac) = mac {
                prefs.mac_chat_notifications = mac;
            }
            if let Some(sounds) = sounds {
                prefs.chat_notification_sounds = sounds;
            }
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_native_notification_points_at_its_own_packaged_sound() {
        let names = [Kind::Question, Kind::Stopped, Kind::Finished, Kind::Back]
            .map(|kind| kind.presentation().2);
        assert_eq!(
            names,
            [
                "satisfying-triple-bubble-pop.mp3",
                "perforated-paper-towel-tear-sound-effect-2-slow-1.0-seconds.mp3",
                "satisfying-stamp-sound.mp3",
                "BRRR-Plop-pass-sound-1-second.mp3",
            ]
        );
    }
}
