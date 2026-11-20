//! **Command-Z undoes the chat list's last Archive, Unpin or Bring back later** (round 93,
//! 2026-11-19: "Undo for a few seconds after you archive or unpin a chat or bring one back
//! later, also with Command-Z").
//!
//! # Why the page cannot do it alone
//!
//! The conversation page is a CHILD webview (`window.add_child`, main.rs), and wry answers
//! `performKeyEquivalent:` with NO for a child (`wry-0.55.1/src/wkwebview/class/wry_web_view.rs:44-55`,
//! the workaround for tauri#9426), so a Command-key press never reaches the page as a keydown.
//! AppKit gives it to the main menu instead, where the predefined Edit > Undo
//! (`muda-0.19.3/src/platform_impl/macos/mod.rs:985`, `sel!(undo:)`) sent `undo:` to the
//! webview: WebKit's text undo, which has nothing to do with the chat list. Nightly candidate
//! 109: the notice stayed and the chat stayed under LATER.
//!
//! # What happens now
//!
//! Edit > Undo is the app's own item, with the same title and Command-Z. While the page has a
//! chat-list Undo on offer it says so (`set_chat_undo`), and a press with the conversation
//! window in front goes to the page (`rich://menu-undo`), which runs the Undo or, when a text
//! field with something in it has focus, hands it back (`native_undo`). Every other press sends
//! `undo:` exactly as the predefined item did, with no round trip through the page.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Runtime};

/// The id `on_menu_event` matches for Edit > Undo.
pub const MENU_UNDO: &str = "richos-undo";

/// Sent to the conversation page when Command-Z (or Edit > Undo) may be the chat list's.
pub const EVENT_MENU_UNDO: &str = "rich://menu-undo";

/// Whether the page has a chat-list Undo on offer right now. Only a hint for routing: a stale
/// `true` costs a round trip (the page finds nothing to undo and asks for `native_undo`).
static OFFERED: AtomicBool = AtomicBool::new(false);

/// Where one Edit > Undo goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// The page decides between the chat-list Undo and text undo.
    Page,
    /// `undo:` to the key window's first responder, as the predefined item did.
    Native,
}

/// The page only when it has an Undo on offer AND its window is the one in front; a press in
/// any other window is that window's text undo.
pub fn route(offered: bool, conversation_in_front: bool) -> Route {
    if offered && conversation_in_front { Route::Page } else { Route::Native }
}

/// Edit > Undo, from the menu or Command-Z. Runs on the main thread (`on_menu_event`).
pub fn on_menu_undo<R: Runtime>(app: &AppHandle<R>) {
    let window = crate::conversation_window(app);
    let in_front = window.as_ref().is_some_and(|window| window.is_focused().unwrap_or(false));
    match (route(OFFERED.load(Ordering::SeqCst), in_front), window) {
        // A page the event cannot reach gets text undo, as before.
        (Route::Page, Some(window)) => {
            if window.emit_to(window.label(), EVENT_MENU_UNDO, ()).is_err() {
                send_undo();
            }
        }
        _ => send_undo(),
    }
}

/// The page offers (`true`) or withdraws (`false`) its chat-list Undo.
#[richos_command::command]
pub fn set_chat_undo(armed: bool) {
    OFFERED.store(armed, Ordering::SeqCst);
}

/// The page had no chat-list Undo for this press, or a text field owns it: undo text.
#[richos_command::command]
pub fn native_undo(app: AppHandle) -> Result<(), String> {
    app.run_on_main_thread(send_undo).map_err(|error| error.to_string())
}

/// `[NSApp sendAction:@selector(undo:) to:nil from:nil]`: what the predefined Undo item's
/// nil-targeted action does, so text undo behaves exactly as before.
#[cfg(target_os = "macos")]
fn send_undo() {
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{msg_send, sel};
    let Some(class) = AnyClass::get(c"NSApplication") else { return };
    // SAFETY: `+sharedApplication` and `-sendAction:to:from:` are called on the main thread
    // (`on_menu_event`, `run_on_main_thread`) with their documented argument types.
    unsafe {
        let application: *mut AnyObject = msg_send![class, sharedApplication];
        if application.is_null() {
            return;
        }
        let nil: *mut AnyObject = std::ptr::null_mut();
        let _: Bool = msg_send![application, sendAction: sel!(undo:), to: nil, from: nil];
    }
}

/// Only macOS builds Edit > Undo as this app's item (main.rs); elsewhere the predefined one
/// stays and nothing reaches here.
#[cfg(not(target_os = "macos"))]
fn send_undo() {}

#[cfg(test)]
mod tests {
    use super::{route, Route};

    #[test]
    fn command_z_reaches_the_page_only_with_an_undo_on_offer_and_its_window_in_front() {
        assert_eq!(route(true, true), Route::Page, "an offered Undo with the conversation in front is the page's");
        assert_eq!(route(false, true), Route::Native, "with nothing on offer Command-Z is text undo, as before");
        assert_eq!(route(true, false), Route::Native, "another window's Command-Z stays that window's text undo");
        assert_eq!(route(false, false), Route::Native);
    }
}
