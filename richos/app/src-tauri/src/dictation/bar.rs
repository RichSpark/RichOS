//! **The bar, the words' flight and the menu, as the tool's three windows** (plan section 6,
//! "The windows, all in the tool"; slice 3).
//!
//! - **The bar** (`ui/dictation-bar.html`): no decorations, transparent, never key, above every
//!   app on every Space and over full-screen apps, shown with `orderFrontRegardless`, taking the
//!   mouse only while a bar with **Fix it** is up. Bottom center of the screen holding the
//!   focused window, otherwise the pointer's.
//! - **The flight** (the same page, `?role=flight`): a transparent, click-through window over
//!   the screen where the words landed, shown for the second they fly and stay lit.
//! - **The menu** (`ui/dictation-menu.html`): under the menu bar item, round 19's `sb-*` menu.
//!   It takes the keyboard (Escape closes it) without taking the front: as a nonactivating panel
//!   it becomes key with the app in front still active; as a plain window, the tool records the
//!   front app at the click and gives the front back to it when the menu closes.
//!
//! Every page reports its own size (it knows its type and the text size); this module sizes
//! and places the window from that. Every decision is in `richos_voice::dictation_bar`.

use super::appkit::{self, Kind, Win};
use super::log;
use richos_voice::dictation_bar::{bar_frame, bar_screen, menu_frame, within, worth_lighting, BarView, Rect};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const BAR: &str = "dictation-bar";
pub const FLIGHT: &str = "dictation-flight";
pub const MENU: &str = "dictation-menu";

/// What the pages are told, by event name.
pub const EVENT_BAR: &str = "dictation-bar";
pub const EVENT_LEVEL: &str = "dictation-level";
pub const EVENT_FLIGHT: &str = "dictation-flight";
pub const EVENT_MENU: &str = "dictation-menu";

/// How long the flight window stays: the half-second flight (round 19 `comet`, 560 ms), the
/// light held 1.6 s and its 1.2 s fade (`.dict-new`: `dictfade 1.2s ease 1.6s`), and a little.
pub const FLIGHT_FOR: std::time::Duration = std::time::Duration::from_millis(3600);

/// A rectangle as the pages and the log carry it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Box4 {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<Rect> for Box4 {
    fn from(r: Rect) -> Self {
        Box4 { x: r.x, y: r.y, w: r.w, h: r.h }
    }
}
impl From<Box4> for Rect {
    fn from(b: Box4) -> Self {
        Rect { x: b.x, y: b.y, w: b.w, h: b.h }
    }
}

/// What a page tells the tool.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// The bar page laid out one view: the window it needs, where the orb's center is in it,
    /// and Fix it's box when there is one.
    BarLaidOut { seq: u64, w: f64, h: f64, orb: (f64, f64), fix: Option<Box4> },
    /// Fix it was pressed.
    FixIt,
    /// The menu page laid out: the window it needs.
    MenuLaidOut { seq: u64, w: f64, h: f64 },
    /// A row of the menu: `mode` (with `accurate` or `fast`), `settings`, `onoff`, `open`, or
    /// `close` (Escape, or the menu lost the keyboard).
    Menu { act: String, value: Option<String> },
    /// The menu bar item was clicked, at this rectangle (top-left points).
    ItemClicked { item: Rect },
    /// A page has loaded and is listening: `bar`, `flight` or `menu`.
    Ready { role: String },
}

/// Where page events go: the tool's control loop.
pub struct UiTx(pub Mutex<Sender<UiEvent>>);

fn send(app: &AppHandle, event: UiEvent) {
    if let Some(tx) = app.try_state::<UiTx>() {
        tx.0.lock().unwrap_or_else(|p| p.into_inner()).send(event).ok();
    }
}

#[richos_command::command]
pub fn dictation_bar_laid_out(app: AppHandle, seq: u64, w: f64, h: f64, orb_x: f64, orb_y: f64, fix: Option<Box4>) {
    send(&app, UiEvent::BarLaidOut { seq, w, h, orb: (orb_x, orb_y), fix });
}

#[richos_command::command]
pub fn dictation_page_ready(app: AppHandle, role: String) {
    send(&app, UiEvent::Ready { role });
}

#[richos_command::command]
pub fn dictation_fix_it(app: AppHandle) {
    send(&app, UiEvent::FixIt);
}

#[richos_command::command]
pub fn dictation_menu_laid_out(app: AppHandle, seq: u64, w: f64, h: f64) {
    send(&app, UiEvent::MenuLaidOut { seq, w, h });
}

#[richos_command::command]
pub fn dictation_menu_act(app: AppHandle, act: String, value: Option<String>) {
    send(&app, UiEvent::Menu { act, value });
}

/// The theme and text size from the app's own settings (`config.json`, config.rs is the truth),
/// read each time a window is shown, so a change in RichOS reaches the next bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub theme: String,
    pub font_scale: u16,
}

pub fn look(data_dir: &std::path::Path) -> Look {
    match richos_core::config::ConfigStore::open(data_dir.join("config.json")) {
        Ok(store) => Look { theme: store.theme().as_str().to_string(), font_scale: store.font_scale() },
        Err(_) => Look { theme: "system".into(), font_scale: 100 },
    }
}

/// The three windows, built hidden in the tool's `setup` (main thread).
pub fn build(app: &AppHandle, kind: Kind) -> Result<(), String> {
    let make = |label: &str, url: &str, w: f64, h: f64, focusable: bool| {
        WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .title("RichOS")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .focusable(focusable)
            .focused(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .accept_first_mouse(true)
            .visible(false)
            .inner_size(w, h)
            .build()
            .map_err(|e| format!("the {label} window could not be built: {e}"))
    };
    let bar = make(BAR, "dictation-bar.html", 760.0, 140.0, false)?;
    let flight = make(FLIGHT, "dictation-bar.html?role=flight", 800.0, 600.0, false)?;
    let menu = make(MENU, "dictation-menu.html", 346.0, 420.0, true)?;
    for (w, key) in [(&bar, false), (&flight, false), (&menu, true)] {
        let ns = Win(w.ns_window().map_err(|e| format!("no native window: {e}"))? as *mut _);
        if kind == Kind::Panel {
            match ns.make_panel(key) {
                Ok(how) => log::line(&format!("{} window: {how}", w.label())),
                Err(why) => log::line(&format!("{} window stays a plain window: {why}", w.label())),
            }
        }
        ns.float_everywhere();
        ns.ignore_mouse(w.label() != MENU);
    }
    log::line(&format!("the bar, the flight and the menu windows are built as the {} type", kind.tag()));
    Ok(())
}

fn win(app: &AppHandle, label: &str) -> Option<Win> {
    let w = app.get_webview_window(label)?;
    w.ns_window().ok().map(|p| Win(p as *mut _))
}

/// Run `f` with the window `label` on the main thread.
fn on_main(app: &AppHandle, label: &'static str, f: impl FnOnce(Win) + Send + 'static) {
    let handle = app.clone();
    let ran = app.run_on_main_thread(move || {
        if let Some(w) = win(&handle, label) {
            f(w);
        }
    });
    if let Err(e) = ran {
        log::line(&format!("the {label} window could not be reached: {e}"));
    }
}

/// Run `f` on the main thread and wait for its answer (at most 2 s).
pub fn ask_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        tx.send(f()).ok();
    })
    .ok()?;
    rx.recv_timeout(std::time::Duration::from_secs(2)).ok()
}

/// The screens, top-left, `(frame, visible)` each, read on the main thread.
pub fn screens(app: &AppHandle) -> Vec<(Rect, Rect)> {
    ask_main(app, appkit::screen_rects).unwrap_or_default()
}

/// The pointer, top-left.
pub fn pointer(app: &AppHandle) -> Option<(f64, f64)> {
    ask_main(app, appkit::pointer).flatten()
}

/// What the bar page is told.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarMessage {
    pub seq: u64,
    pub view: &'static str,
    pub problem: Option<&'static str>,
    pub key: String,
    /// When listening began, milliseconds since the epoch, for the time on the bar.
    pub started_ms: Option<u64>,
    #[serde(flatten)]
    pub look: Look,
}

/// The bar, placed and shown: what [`Bar::laid_out`] did with the page's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub frame: Rect,
    pub orb: (f64, f64),
    pub fix: Option<Rect>,
}

/// The bar's state between a view and its layout.
#[derive(Default)]
pub struct Bar {
    seq: u64,
    view: Option<BarView>,
    placed: Option<Placed>,
}

impl Bar {
    /// Tell the page to draw `view`. Hidden hides the window at once; anything else is shown
    /// once the page has laid it out ([`Bar::laid_out`]). Returns the view's sequence number.
    pub fn show(&mut self, app: &AppHandle, view: BarView, key: u8, started_ms: Option<u64>, look: Look) -> u64 {
        self.seq += 1;
        self.view = Some(view);
        if view == BarView::Hidden {
            self.placed = None;
            on_main(app, BAR, |w| w.hide());
        }
        let message = BarMessage {
            seq: self.seq,
            view: view.tag(),
            problem: match view {
                BarView::Problem(p) => Some(p.tag()),
                _ => None,
            },
            key: format!("F{key}"),
            started_ms,
            look,
        };
        if let Err(e) = app.emit_to(BAR, EVENT_BAR, &message) {
            log::line(&format!("the bar could not be told what to show: {e}"));
        }
        self.seq
    }

    pub fn view(&self) -> Option<BarView> {
        self.view
    }

    pub fn placed(&self) -> Option<Placed> {
        self.placed
    }

    /// The page laid out view `seq` at `(w, h)`: size and place the window on the screen that
    /// holds the focused window (or the pointer), and show it without activating anything.
    pub fn laid_out(&mut self, app: &AppHandle, seq: u64, (w, h): (f64, f64), orb: (f64, f64), fix: Option<Box4>, focused: Option<Rect>) -> Option<Placed> {
        if seq != self.seq || matches!(self.view, None | Some(BarView::Hidden)) {
            return None;
        }
        let screens = screens(app);
        if screens.is_empty() {
            log::line("no screen to put the bar on");
            return None;
        }
        let frames: Vec<Rect> = screens.iter().map(|s| s.0).collect();
        let i = bar_screen(&frames, focused, pointer(app));
        let frame = bar_frame(screens[i].1, (w, h));
        let takes_mouse = self.view.is_some_and(richos_voice::dictation_bar::takes_mouse);
        on_main(app, BAR, move |win| {
            win.set_frame(frame);
            win.ignore_mouse(!takes_mouse);
            win.show();
        });
        let placed = Placed {
            frame,
            orb: (frame.x + orb.0, frame.y + orb.1),
            fix: fix.map(|b| Rect { x: frame.x + b.x, y: frame.y + b.y, w: b.w, h: b.h }),
        };
        self.placed = Some(placed);
        Some(placed)
    }

    /// The bar's timer ran out for view `seq`: hide it if nothing newer replaced it.
    pub fn expire(&mut self, app: &AppHandle, seq: u64) -> bool {
        if seq != self.seq {
            return false;
        }
        self.view = Some(BarView::Hidden);
        self.placed = None;
        on_main(app, BAR, |w| w.hide());
        true
    }
}

/// The meter's level, to the bar page.
pub fn level(app: &AppHandle, level: f32) {
    app.emit_to(BAR, EVENT_LEVEL, serde_json::json!({ "level": level })).ok();
}

/// What the flight page is told: where the orb is and where the words are, in its own window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightMessage {
    pub from: (f64, f64),
    pub to: Box4,
    #[serde(flatten)]
    pub look: Look,
}

/// **The words fly from the bar to where they landed and are lit** (plan section 5, point 6;
/// round 19 `comet` and `.dict-new`). `words` is their rectangle on screen, from Accessibility.
pub fn fly(app: &AppHandle, from: (f64, f64), words: Rect, look: Look) -> Option<Rect> {
    let screens = screens(app);
    let (screen, _) = *screens.iter().find(|(f, _)| f.contains(words.center()))?;
    if !worth_lighting(words, screen) {
        return None;
    }
    let message = FlightMessage {
        from: (from.0 - screen.x, from.1 - screen.y),
        to: within(words, screen).into(),
        look,
    };
    on_main(app, FLIGHT, move |w| {
        w.set_frame(screen);
        w.ignore_mouse(true);
        w.show();
    });
    app.emit_to(FLIGHT, EVENT_FLIGHT, &message).ok();
    Some(screen)
}

pub fn land(app: &AppHandle) {
    on_main(app, FLIGHT, |w| w.hide());
}

/// What the menu page is told.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuMessage {
    pub seq: u64,
    pub on: bool,
    pub key: String,
    /// `accurate` or `fast`, derived from the model id.
    pub choice: &'static str,
    /// Another app has Secure Event Input on, so the key cannot reach the tool.
    pub paused: bool,
    /// That app's name, when macOS gives it.
    pub secure_app: Option<String>,
    #[serde(flatten)]
    pub look: Look,
}

/// The menu's state between a click and its close.
#[derive(Default)]
pub struct Menu {
    seq: u64,
    item: Option<Rect>,
    open: bool,
    /// The app in front when the menu opened, given the front back on close (plain window type).
    front: Option<i32>,
}

impl Menu {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The item was clicked: draw the menu; it is shown once the page has laid it out.
    pub fn open(&mut self, app: &AppHandle, item: Rect, message: impl FnOnce(u64) -> MenuMessage) {
        self.seq += 1;
        self.item = Some(item);
        self.open = true;
        self.front = appkit::frontmost_pid();
        let m = message(self.seq);
        if let Err(e) = app.emit_to(MENU, EVENT_MENU, &m) {
            log::line(&format!("the menu could not be drawn: {e}"));
        }
    }

    /// Redraw an open menu (a choice changed it, or Secure Event Input did).
    pub fn redraw(&mut self, app: &AppHandle, message: impl FnOnce(u64) -> MenuMessage) {
        if !self.open {
            return;
        }
        self.seq += 1;
        app.emit_to(MENU, EVENT_MENU, &message(self.seq)).ok();
    }

    /// The page laid the menu out: hang it under the item and give it the keyboard.
    pub fn laid_out(&mut self, app: &AppHandle, seq: u64, (w, h): (f64, f64), kind: Kind) -> Option<Rect> {
        if seq != self.seq || !self.open {
            return None;
        }
        let item = self.item?;
        let screens = screens(app);
        let (_, visible) = *screens.iter().find(|(f, _)| f.contains(item.center())).or(screens.first())?;
        let frame = menu_frame(item, visible, (w, h));
        on_main(app, MENU, move |win| {
            win.set_frame(frame);
            if kind == Kind::Window {
                // A plain window takes the keyboard only from an active app.
                activate_self();
            }
            win.show_key();
            log::line(&format!(
                "menu key window shown; the tool is active: {}; front pid {:?}",
                appkit::app_active(),
                appkit::frontmost_pid()
            ));
        });
        Some(frame)
    }

    /// Close the menu; with the plain window type, give the front back to the app that had it.
    pub fn close(&mut self, app: &AppHandle, kind: Kind) -> Option<i32> {
        if !self.open {
            return None;
        }
        self.open = false;
        let front = self.front.take();
        let own = std::process::id() as i32;
        let give_back = if kind == Kind::Window { front.filter(|p| *p != own) } else { None };
        on_main(app, MENU, move |win| {
            win.hide();
            if let Some(pid) = give_back {
                appkit::activate(pid);
            }
        });
        front
    }
}

fn activate_self() {
    objc2::rc::autoreleasepool(|_| {
        if let Some(class) = objc2::runtime::AnyClass::get(c"NSApplication") {
            // SAFETY: documented accessors; main thread.
            unsafe {
                let app: *mut objc2::runtime::AnyObject = objc2::msg_send![class, sharedApplication];
                let _: () = objc2::msg_send![app, activateIgnoringOtherApps: true];
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the page's rectangle and the decision module's are one shape, both ways.
    #[test]
    fn a_box_is_a_rect() {
        let r = Rect { x: 1.0, y: 2.0, w: 3.0, h: 4.0 };
        assert_eq!(Rect::from(Box4::from(r)), r);
        let fix: Box4 = serde_json::from_str(r#"{"x":10,"y":20,"w":60,"h":30}"#).unwrap();
        assert_eq!(fix, Box4 { x: 10.0, y: 20.0, w: 60.0, h: 30.0 });
    }

    /// INVARIANT: the bar page is told the view by name, the problem by its tag, the key as it
    /// is drawn ("F1") and the look as config.rs holds it, in the camelCase the page reads.
    #[test]
    fn the_bar_message_on_the_wire() {
        let m = BarMessage {
            seq: 3,
            view: BarView::Problem(richos_voice::dictation::Problem::ModelMissing).tag(),
            problem: Some(richos_voice::dictation::Problem::ModelMissing.tag()),
            key: "F1".into(),
            started_ms: None,
            look: Look { theme: "light".into(), font_scale: 110 },
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["view"], "problem");
        assert_eq!(v["problem"], "model-missing");
        assert_eq!(v["key"], "F1");
        assert_eq!(v["theme"], "light");
        assert_eq!(v["fontScale"], 110);
        assert!(v["startedMs"].is_null());
    }

    /// INVARIANT: a fresh data folder's look is the app's own default (System, 100%).
    #[test]
    fn the_look_of_a_fresh_install() {
        let d = std::env::temp_dir().join(format!("richos-dictation-look-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let l = look(&d);
        assert_eq!(l.theme, "system");
        assert_eq!(l.font_scale, 100);
        std::fs::remove_dir_all(&d).ok();
    }
}
