//! **RichOS in the menu bar** (plan section 6, "The menu bar item"; round 19 states 13, 14, 17).
//!
//! Tauri's tray icon (feature `tray-icon`; `tray-icon 0.24.2` was already in `Cargo.lock`): a
//! template microphone while dictation is on and idle, which macOS draws in the menu bar's own
//! ink; the gold microphone while listening. Present only while dictation is on, which in this
//! tool is its whole life: the item goes when the tool does. A click opens the menu window
//! (`bar.rs`), because a native macOS menu cannot carry round 19's type and colors.

use super::bar::{UiEvent, UiTx};
use super::log;
use richos_voice::dictation_bar::{listening_icon, template_icon, Rect, ICON_PX};
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const ID: &str = "richos-dictation";

/// **The item's picture and whether macOS draws it as a template**: the template microphone
/// while idle, the gold square (not a template: its colors are its own) while listening.
pub fn picture(listening: bool, dark_menu_bar: bool) -> (Image<'static>, bool) {
    if listening {
        (Image::new_owned(listening_icon(dark_menu_bar), ICON_PX, ICON_PX), false)
    } else {
        (Image::new_owned(template_icon(), ICON_PX, ICON_PX), true)
    }
}

/// The item, made in the tool's `setup`.
pub fn build(app: &AppHandle) -> Result<TrayIcon, String> {
    let (icon, template) = picture(false, true);
    TrayIconBuilder::with_id(ID)
        .icon(icon)
        .icon_as_template(template)
        .tooltip("RichScribe")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            let scale = app.primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(2.0);
            // Every event but a move is logged with where the item was (in points, the unit of
            // "menu bar item at"), so a press that opens nothing (walk-c409831e46f9,
            // walk-8dcad10d445b, walk-3520f87af8f9, walk-e7b42c44d5d8) says whether anything
            // reached the item at all.
            if !matches!(event, TrayIconEvent::Move { .. }) {
                let at = event_rect(&event, scale)
                    .map(|r| format!("{:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.w, r.h))
                    .unwrap_or_else(|| "?".into());
                log::line(&format!("menu bar item event: {} at {at}", event_word(&event)));
            }
            // On the press, as every menu in the macOS menu bar opens; the release is ignored.
            // walk-37076638c6dd: a click whose release never reached the item opened nothing.
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Down, .. } = event {
                if let (Some(item), Some(tx)) = (event_rect(&event, scale), app.try_state::<UiTx>()) {
                    tx.0.lock().unwrap_or_else(|p| p.into_inner()).send(UiEvent::ItemClicked { item }).ok();
                }
            }
        })
        .build(app)
        .map_err(|e| format!("the menu bar item could not be made: {e}"))
}

/// The item's rectangle an event carries, in top-left points (Tauri reports it in pixels of
/// the primary screen, as [`item_rect`] reads it).
fn event_rect(event: &TrayIconEvent, scale: f64) -> Option<Rect> {
    let rect = match event {
        TrayIconEvent::Click { rect, .. }
        | TrayIconEvent::DoubleClick { rect, .. }
        | TrayIconEvent::Enter { rect, .. }
        | TrayIconEvent::Move { rect, .. }
        | TrayIconEvent::Leave { rect, .. } => rect,
        _ => return None,
    };
    let p = rect.position.to_logical::<f64>(scale);
    let s = rect.size.to_logical::<f64>(scale);
    Some(Rect { x: p.x, y: p.y, w: s.width, h: s.height })
}

/// One word for a tray event, for the log.
fn event_word(event: &TrayIconEvent) -> String {
    match event {
        TrayIconEvent::Click { button, button_state, .. } => format!("click {button:?} {button_state:?}"),
        TrayIconEvent::DoubleClick { button, .. } => format!("double-click {button:?}"),
        TrayIconEvent::Enter { .. } => "enter".into(),
        TrayIconEvent::Leave { .. } => "leave".into(),
        TrayIconEvent::Move { .. } => "move".into(),
        _ => "other".into(),
    }
}

/// Where the item sits, in top-left points: Tauri reports it in pixels of the primary screen.
pub fn item_rect(tray: &TrayIcon) -> Option<Rect> {
    let rect = tray.rect().ok().flatten()?;
    let scale = tray.app_handle().primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(2.0);
    let p = rect.position.to_logical::<f64>(scale);
    let s = rect.size.to_logical::<f64>(scale);
    Some(Rect { x: p.x, y: p.y, w: s.width, h: s.height })
}

/// The menu bar's band, in top-left points: a status item sits in the top row of the screen.
pub const MENU_BAR_BAND: f64 = 100.0;

/// **Is this the item's real place?** Read before the main run loop has put the status item in
/// the menu bar, Tauri reports the unplaced frame, which lands at the screen's bottom-left corner
/// (`0,1050 34x24` on a 1680x1050 guest: walks 5028f74bc8b6 and 7bdbf43a6228). A placed item is
/// in the menu bar's band and has a width.
pub fn placed(r: &Rect) -> bool {
    r.w > 0.0 && r.h > 0.0 && r.y >= 0.0 && r.y < MENU_BAR_BAND
}

/// How long [`log_item_when_placed`] keeps looking, and how often.
pub const PLACE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
pub const PLACE_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// Log "menu bar item at x,y wxh" once the item is placed, on a thread of its own so the control
/// loop never waits for the main run loop; a line saying so if it never is within the bound.
pub fn log_item_when_placed(tray: TrayIcon) {
    std::thread::Builder::new()
        .name("dictation-menubar-place".into())
        .spawn(move || {
            let began = std::time::Instant::now();
            loop {
                if let Some(r) = item_rect(&tray) {
                    if placed(&r) {
                        log::line(&format!("menu bar item at {:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.w, r.h));
                        return;
                    }
                }
                if began.elapsed() >= PLACE_WAIT {
                    log::line(&format!("menu bar item: no place in the menu bar reported within {} s", PLACE_WAIT.as_secs()));
                    return;
                }
                std::thread::sleep(PLACE_POLL);
            }
        })
        .ok();
}

/// Gold while listening, the template microphone otherwise. `dark` is the menu bar's appearance.
pub fn set_listening(tray: &TrayIcon, listening: bool, dark: bool) {
    let (icon, template) = picture(listening, dark);
    if let Err(e) = tray.set_icon(Some(icon)).and_then(|_| tray.set_icon_as_template(template)) {
        log::line(&format!("the menu bar item could not change its picture: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the unplaced frame Tauri reports before the run loop has put the item in the
    /// menu bar (the screen's bottom-left corner) is never logged as the item's place; a frame in
    /// the menu bar's band with a size is.
    #[test]
    fn the_unplaced_frame_is_never_the_items_place() {
        assert!(!placed(&Rect { x: 0.0, y: 1050.0, w: 34.0, h: 24.0 }), "walk 5028f74bc8b6's and 7bdbf43a6228's line");
        assert!(placed(&Rect { x: 1437.0, y: 0.0, w: 34.0, h: 24.0 }), "walk 37076638c6dd's line");
        assert!(!placed(&Rect { x: 1437.0, y: 0.0, w: 0.0, h: 24.0 }), "no width is no item");
        assert!(!placed(&Rect { x: 10.0, y: -5.0, w: 34.0, h: 24.0 }));
        assert!(!placed(&Rect { x: 10.0, y: MENU_BAR_BAND, w: 34.0, h: 24.0 }));
    }

    /// INVARIANT: the item is 18 points at 2x in both states; idle is a template (macOS inks it
    /// for the menu bar), listening is not (its gold is its own), and the bytes are whole RGBA.
    #[test]
    fn the_two_pictures() {
        for dark in [true, false] {
            let (idle, idle_template) = picture(false, dark);
            let (gold, gold_template) = picture(true, dark);
            assert!(idle_template);
            assert!(!gold_template);
            for image in [&idle, &gold] {
                assert_eq!((image.width(), image.height()), (36, 36));
                assert_eq!(image.rgba().len(), 36 * 36 * 4);
            }
            assert_ne!(idle.rgba(), gold.rgba());
        }
        assert_ne!(picture(true, true).0.rgba(), picture(true, false).0.rgba(), "the gold follows the menu bar");
    }
}
