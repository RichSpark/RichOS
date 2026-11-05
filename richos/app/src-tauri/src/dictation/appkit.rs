//! **The tool's windows, as AppKit sees them** (plan section 6, "The windows, all in the tool").
//!
//! Thin and named for one API each: a window's frame in AppKit's bottom-left space, its level,
//! its Spaces behavior, whether it takes the mouse, showing it with `orderFrontRegardless` (never
//! `makeKeyAndOrderFront`, which `set_visible(true)` calls and which would activate the tool),
//! turning it into a nonactivating panel, the screens, the pointer, the front app, the menu bar's
//! appearance and Secure Event Input. Every decision about these is in
//! `richos_voice::dictation_bar`.
//!
//! **Which window type** (Frank's minor 3, the window check that starts slice 3): a plain
//! window Tauri builds with `focusable(false)`, or that same window turned into a nonactivating
//! `NSPanel` by changing its class through objc2's runtime (no new package). `RICHOS_DICTATION_
//! WINDOW` picks one for the check; [`DEFAULT_KIND`] is what the check chose.

use objc2::encode::{Encode, Encoding};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::sel;
use richos_voice::dictation_bar::{from_cocoa, to_cocoa, Rect};
use std::ffi::{c_void, CStr};
use std::sync::OnceLock;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NSPoint {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NSSize {
    pub width: f64,
    pub height: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NSRect {
    pub origin: NSPoint,
    pub size: NSSize,
}
// SAFETY: the layouts above are CoreGraphics' CGPoint, CGSize and CGRect on 64-bit, and these are
// the encodings AppKit declares for them.
unsafe impl Encode for NSPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for NSSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for NSRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[NSPoint::ENCODING, NSSize::ENCODING]);
}

impl NSRect {
    fn cocoa(&self) -> (f64, f64, f64, f64) {
        (self.origin.x, self.origin.y, self.size.width, self.size.height)
    }
}

/// The two window types the check compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The window Tauri builds, never key (`focusable(false)`, tao's `canBecomeKeyWindow`).
    Window,
    /// The same window turned into a nonactivating `NSPanel`.
    Panel,
}

impl Kind {
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Window => "window",
            Kind::Panel => "panel",
        }
    }
}

/// Picks the window type for the check. Never set in a shipping run.
pub const KIND_ENV: &str = "RICHOS_DICTATION_WINDOW";

/// **What the window check chose** (slice 3's first step, in the guest): see the commit that
/// set it for the measurement.
pub const DEFAULT_KIND: Kind = Kind::Panel;

/// The window type for this run: [`KIND_ENV`] when it names one, otherwise [`DEFAULT_KIND`].
pub fn kind_from(value: Option<&str>) -> Kind {
    match value.map(str::trim) {
        Some("window") => Kind::Window,
        Some("panel") => Kind::Panel,
        _ => DEFAULT_KIND,
    }
}

/// `NSStatusWindowLevel`: above every app's windows, as menu bar extras' own windows sit.
pub const STATUS_LEVEL: isize = 25;
/// `NSWindowCollectionBehaviorCanJoinAllSpaces | Stationary | IgnoresCycle |
/// FullScreenAuxiliary`: on every Space, over a full-screen app, never in Mission Control's
/// shuffle or the window cycle.
pub const COLLECTION: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
/// `NSWindowStyleMaskNonactivatingPanel`.
pub const NONACTIVATING_PANEL: usize = 1 << 7;

/// A raw `NSWindow *` from Tauri (`WebviewWindow::ns_window`). Only touched on the main thread.
#[derive(Clone, Copy)]
pub struct Win(pub *mut AnyObject);
// SAFETY: the pointer is only dereferenced inside `run_on_main_thread` closures.
unsafe impl Send for Win {}

fn primary_height() -> f64 {
    screens().first().map(|s| s.frame.size.height).unwrap_or(0.0)
}

impl Win {
    /// Put the window at `r` (top-left points).
    pub fn set_frame(self, r: Rect) {
        let (x, y, w, h) = to_cocoa(r, primary_height());
        let rect = NSRect { origin: NSPoint { x, y }, size: NSSize { width: w, height: h } };
        // SAFETY: a live NSWindow, on the main thread.
        unsafe {
            let _: () = msg_send![self.0, setFrame: rect, display: true];
        }
    }

    /// Where the window is now (top-left points): after the person dragged it, this is the truth.
    pub fn frame(self) -> Rect {
        // SAFETY: a live NSWindow, on the main thread; `frame` is a documented getter.
        let r: NSRect = unsafe { msg_send![self.0, frame] };
        from_cocoa(r.cocoa(), primary_height())
    }

    /// Whether the window is on screen now.
    pub fn is_visible(self) -> bool {
        // SAFETY: a live NSWindow, on the main thread; `isVisible` is a documented getter.
        let b: Bool = unsafe { msg_send![self.0, isVisible] };
        b.as_bool()
    }

    /// Above every app, on every Space, over full-screen apps, never in the window cycle.
    pub fn float_everywhere(self) {
        // SAFETY: as above; documented NSWindow setters.
        unsafe {
            let _: () = msg_send![self.0, setLevel: STATUS_LEVEL];
            let _: () = msg_send![self.0, setCollectionBehavior: COLLECTION];
            let _: () = msg_send![self.0, setHidesOnDeactivate: false];
            let _: () = msg_send![self.0, setCanHide: false];
        }
    }

    pub fn ignore_mouse(self, ignore: bool) {
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![self.0, setIgnoresMouseEvents: ignore];
        }
    }

    /// Show without activating anything (plan section 6: "shown with `orderFrontRegardless`
    /// so it never activates anything").
    pub fn show(self) {
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![self.0, orderFrontRegardless];
        }
    }

    /// Show and take the keyboard: the menu, for Escape. On a nonactivating panel this makes it
    /// key without activating the tool, so the app in front keeps the front.
    pub fn show_key(self) {
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![self.0, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
        }
    }

    pub fn hide(self) {
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![self.0, orderOut: std::ptr::null::<AnyObject>()];
        }
    }

    /// **Turn the window into a nonactivating panel.** Its class becomes a subclass of `NSPanel`
    /// that answers `canBecomeKeyWindow` with `key` and `canBecomeMainWindow` with NO, and its
    /// style gains `NSWindowStyleMaskNonactivatingPanel`. Returns what was done, for the log.
    pub fn make_panel(self, key: bool) -> Result<&'static str, String> {
        let class = panel_class(key).ok_or("the panel class could not be made")?;
        // SAFETY: `object_setClass` on a live window to a subclass of NSPanel, which is a
        // subclass of NSWindow with no instance variables of its own; the window's own (tao's
        // `focusable`) stay where they are and are no longer read, because the new class answers
        // `canBecomeKeyWindow` itself. Main thread.
        unsafe {
            object_setClass(self.0 as *mut c_void, class as *const AnyClass as *const c_void);
            let mask: usize = msg_send![self.0, styleMask];
            let _: () = msg_send![self.0, setStyleMask: mask | NONACTIVATING_PANEL];
            let _: () = msg_send![self.0, setFloatingPanel: true];
            let _: () = msg_send![self.0, setBecomesKeyOnlyIfNeeded: !key];
            let _: () = msg_send![self.0, setWorksWhenModal: true];
            // The window server keeps its own "prevents activation" bit, set when a window is
            // created; a style changed afterwards does not reach it on every macOS. AppKit's own
            // setter for it, where this macOS has one.
            let prevents = sel!(_setPreventsActivation:);
            let responds: bool = msg_send![self.0, respondsToSelector: prevents];
            if responds {
                let _: () = msg_send![self.0, _setPreventsActivation: true];
                Ok("nonactivating panel, activation prevented at the window server")
            } else {
                Ok("nonactivating panel")
            }
        }
    }
}

extern "C" {
    fn object_setClass(obj: *mut c_void, cls: *const c_void) -> *const c_void;
}

extern "C-unwind" fn answer_yes(_: *mut AnyObject, _: Sel) -> Bool {
    Bool::YES
}
extern "C-unwind" fn answer_no(_: *mut AnyObject, _: Sel) -> Bool {
    Bool::NO
}

/// The two panel classes, made once: one that can be key (the menu, for Escape), one that
/// never is (the bar and the flight).
fn panel_class(key: bool) -> Option<&'static AnyClass> {
    static KEY: OnceLock<Option<usize>> = OnceLock::new();
    static NEVER: OnceLock<Option<usize>> = OnceLock::new();
    let (cell, name): (_, &CStr) =
        if key { (&KEY, c"RichOSDictationKeyPanel") } else { (&NEVER, c"RichOSDictationPanel") };
    let made = cell.get_or_init(|| {
        let superclass = AnyClass::get(c"NSPanel")?;
        let mut builder = ClassBuilder::new(name, superclass)?;
        // SAFETY: both selectors take no argument and return BOOL, as NSWindow declares them.
        type Answer = extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool;
        let can_be_key: Answer = if key { answer_yes } else { answer_no };
        let never: Answer = answer_no;
        unsafe {
            builder.add_method(sel!(canBecomeKeyWindow), can_be_key);
            builder.add_method(sel!(canBecomeMainWindow), never);
        }
        Some(builder.register() as *const AnyClass as usize)
    });
    // SAFETY: a registered class lives for the process.
    made.map(|p| unsafe { &*(p as *const AnyClass) })
}

/// One screen: its whole frame and the part free of the menu bar and the Dock, both top-left.
#[derive(Debug, Clone, Copy)]
pub struct Screen {
    pub frame: NSRect,
    pub visible: NSRect,
}

/// The screens, the primary first, in AppKit's own frames.
pub fn screens() -> Vec<Screen> {
    objc2::rc::autoreleasepool(|_| {
        let Some(class) = AnyClass::get(c"NSScreen") else { return Vec::new() };
        // SAFETY: documented class method and NSArray / NSScreen accessors; main thread.
        unsafe {
            let list: *mut AnyObject = msg_send![class, screens];
            if list.is_null() {
                return Vec::new();
            }
            let n: usize = msg_send![list, count];
            (0..n)
                .map(|i| {
                    let s: *mut AnyObject = msg_send![list, objectAtIndex: i];
                    Screen { frame: msg_send![s, frame], visible: msg_send![s, visibleFrame] }
                })
                .collect()
        }
    })
}

/// The screens as top-left rectangles: `(frame, visible)` each.
pub fn screen_rects() -> Vec<(Rect, Rect)> {
    let all = screens();
    let h = all.first().map(|s| s.frame.size.height).unwrap_or(0.0);
    all.iter().map(|s| (from_cocoa(s.frame.cocoa(), h), from_cocoa(s.visible.cocoa(), h))).collect()
}

/// The pointer, top-left.
pub fn pointer() -> Option<(f64, f64)> {
    let class = AnyClass::get(c"NSEvent")?;
    // SAFETY: a documented class method returning an NSPoint.
    let p: NSPoint = unsafe { msg_send![class, mouseLocation] };
    Some((p.x, primary_height() - p.y))
}

/// Is this process the active app? It never should be: the tool is Accessory and its windows
/// are nonactivating panels.
pub fn app_active() -> bool {
    objc2::rc::autoreleasepool(|_| {
        let Some(class) = AnyClass::get(c"NSApplication") else { return false };
        // SAFETY: documented class method and accessor.
        unsafe {
            let app: *mut AnyObject = msg_send![class, sharedApplication];
            msg_send![app, isActive]
        }
    })
}

/// `NSApplicationDidBecomeActiveNotification` reached the watcher: the activation is handed
/// back at once, and the log says so with the app that was in front.
///
/// Where it comes from (walk-8dcad10d445b, bundle 6dcd14dcb): wry activates the application
/// when it creates a webview (`wry-0.55.1/src/wkwebview/mod.rs:696`, `NSApplication::activate`,
/// unconditional), and on macOS 14+ that cooperative request is granted about two seconds
/// later through the run loop, when the app that was active is itself still starting: a tool
/// started by a freshly launched RichOS became the active app at 16:03:43 and 16:03:55, and
/// a press on its menu bar item then opened nothing, where the same press with TextEdit active
/// opened the menu in every other run. The tool is never the front (plan section 6): the front
/// belongs to the app the words go to.
extern "C-unwind" fn became_active(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {
    let front = frontmost_pid();
    deactivate();
    super::log::line(&format!(
        "the tool became the active app (front pid {front:?}); activation handed back, the front belongs to the app the words go to"
    ));
}

/// Give activation back to whatever app had it: `[NSApp deactivate]`.
fn deactivate() {
    objc2::rc::autoreleasepool(|_| {
        if let Some(class) = AnyClass::get(c"NSApplication") {
            // SAFETY: documented class method and instance method; main thread (the observer
            // runs on the notification's thread, which is the main thread for AppKit).
            unsafe {
                let app: *mut AnyObject = msg_send![class, sharedApplication];
                let _: () = msg_send![app, deactivate];
            }
        }
    });
}

/// **Hand back every activation of this process.** Registered once, in the tool's `setup`
/// (main thread); the observer lives for the process.
pub fn watch_activation() -> Result<(), String> {
    static WATCH: OnceLock<Result<(), String>> = OnceLock::new();
    WATCH
        .get_or_init(|| {
            let superclass = AnyClass::get(c"NSObject").ok_or("no NSObject")?;
            let mut builder = ClassBuilder::new(c"RichOSDictationActivationWatch", superclass)
                .ok_or("the activation watch class could not be made")?;
            type Handler = extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);
            let handler: Handler = became_active;
            // SAFETY: the selector takes one object (the notification) and returns nothing.
            unsafe {
                builder.add_method(sel!(appDidBecomeActive:), handler);
            }
            let class = builder.register();
            let center_class = AnyClass::get(c"NSNotificationCenter").ok_or("no NSNotificationCenter")?;
            // SAFETY: documented NSNotificationCenter API; the watcher is kept for the process.
            unsafe {
                let watcher: *mut AnyObject = msg_send![class, new];
                let center: *mut AnyObject = msg_send![center_class, defaultCenter];
                let name = ns_string("NSApplicationDidBecomeActiveNotification");
                let _: () = msg_send![center, addObserver: watcher, selector: sel!(appDidBecomeActive:), name: name, object: std::ptr::null::<AnyObject>()];
            }
            Ok(())
        })
        .clone()
}

/// An autoreleased NSString.
unsafe fn ns_string(s: &str) -> *mut AnyObject {
    let c = std::ffi::CString::new(s).unwrap_or_default();
    let class = AnyClass::get(c"NSString").expect("NSString");
    msg_send![class, stringWithUTF8String: c.as_ptr()]
}

/// The app in front, by process id.
pub fn frontmost_pid() -> Option<i32> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSWorkspace")?;
        // SAFETY: documented class method and NSRunningApplication accessor.
        unsafe {
            let ws: *mut AnyObject = msg_send![class, sharedWorkspace];
            let app: *mut AnyObject = msg_send![ws, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let pid: i32 = msg_send![app, processIdentifier];
            (pid > 0).then_some(pid)
        }
    })
}

/// Give the front back to `pid` (the menu's fallback when it is not a panel).
pub fn activate(pid: i32) -> bool {
    objc2::rc::autoreleasepool(|_| {
        let Some(class) = AnyClass::get(c"NSRunningApplication") else { return false };
        // SAFETY: documented class method; `activateWithOptions:` with
        // NSApplicationActivateAllWindows (1).
        unsafe {
            let app: *mut AnyObject = msg_send![class, runningApplicationWithProcessIdentifier: pid];
            if app.is_null() {
                return false;
            }
            msg_send![app, activateWithOptions: 1usize]
        }
    })
}

/// The name macOS shows for the app with `pid`.
pub fn app_name(pid: i32) -> Option<String> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSRunningApplication")?;
        // SAFETY: documented class method and NSString accessor.
        unsafe {
            let app: *mut AnyObject = msg_send![class, runningApplicationWithProcessIdentifier: pid];
            if app.is_null() {
                return None;
            }
            let name: *mut AnyObject = msg_send![app, localizedName];
            if name.is_null() {
                return None;
            }
            let p: *const std::ffi::c_char = msg_send![name, UTF8String];
            (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
        }
    })
}

/// Is the menu bar dark? `NSApp.effectiveAppearance`'s name, which follows the system's
/// appearance for an app that sets none of its own.
pub fn dark_appearance() -> bool {
    objc2::rc::autoreleasepool(|_| {
        let Some(class) = AnyClass::get(c"NSApplication") else { return true };
        // SAFETY: documented accessors; main thread.
        unsafe {
            let app: *mut AnyObject = msg_send![class, sharedApplication];
            let appearance: *mut AnyObject = msg_send![app, effectiveAppearance];
            if appearance.is_null() {
                return true;
            }
            let name: *mut AnyObject = msg_send![appearance, name];
            if name.is_null() {
                return true;
            }
            let p: *const std::ffi::c_char = msg_send![name, UTF8String];
            p.is_null() || CStr::from_ptr(p).to_string_lossy().contains("Dark")
        }
    })
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGSessionCopyCurrentDictionary() -> *const c_void;
}

/// **Secure Event Input** (Frank's minor 5): `None` when it is off; `Some(name)` when it is on,
/// with the app macOS names for it where the session dictionary carries its process
/// (`kCGSSessionSecureInputPID`), otherwise `Some(None)`.
pub fn secure_input() -> Option<Option<String>> {
    // SAFETY: no arguments, no failure mode.
    if !unsafe { IsSecureEventInputEnabled() } {
        return None;
    }
    Some(secure_input_pid().and_then(app_name))
}

fn secure_input_pid() -> Option<i32> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    // SAFETY: a +1 dictionary (or null) we wrap and so release.
    let raw = unsafe { CGSessionCopyCurrentDictionary() };
    if raw.is_null() {
        return None;
    }
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw as _) };
    let value = dict.find(CFString::from_static_string("kCGSSessionSecureInputPID"))?;
    let pid = value.downcast::<CFNumber>()?.to_i32()?;
    (pid > 0).then_some(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the activation watch registers once and answers the same the second time; a
    /// test process is never the active app (and handing back activation while not active is
    /// harmless: `deactivate` on an inactive app does nothing).
    #[test]
    fn the_activation_watch_registers_once() {
        assert_eq!(watch_activation(), Ok(()));
        assert_eq!(watch_activation(), Ok(()));
        assert!(!app_active());
        deactivate();
        assert!(!app_active());
    }

    /// INVARIANT: the check's switch names exactly the two types; anything else is the default.
    #[test]
    fn the_window_type_switch() {
        assert_eq!(kind_from(Some("window")), Kind::Window);
        assert_eq!(kind_from(Some(" panel ")), Kind::Panel);
        assert_eq!(kind_from(Some("other")), DEFAULT_KIND);
        assert_eq!(kind_from(None), DEFAULT_KIND);
        assert_eq!(Kind::Window.tag(), "window");
        assert_eq!(Kind::Panel.tag(), "panel");
    }

    /// INVARIANT: AppKit's constants, by value: the status level, the four collection bits
    /// (all Spaces, stationary, out of the cycle, over full-screen apps) and the panel bit.
    #[test]
    fn the_appkit_constants() {
        assert_eq!(STATUS_LEVEL, 25);
        assert_eq!(COLLECTION, 0b1_0101_0001);
        assert_eq!(NONACTIVATING_PANEL, 128);
        assert_eq!(std::mem::size_of::<NSRect>(), 32);
    }

    /// INVARIANT: both panel classes are real subclasses of NSPanel, made once, answering
    /// `canBecomeKeyWindow` as asked, so the encodings AppKit checks in a debug build hold.
    #[test]
    fn the_panel_classes() {
        let key = panel_class(true).expect("the key panel class");
        let never = panel_class(false).expect("the never-key panel class");
        assert!(std::ptr::eq(key, panel_class(true).unwrap()), "made once");
        let panel = AnyClass::get(c"NSPanel").unwrap();
        for class in [key, never] {
            assert!(std::ptr::eq(class.superclass().unwrap(), panel));
        }
        assert_eq!(key.name().to_str().unwrap(), "RichOSDictationKeyPanel");
        assert_eq!(never.name().to_str().unwrap(), "RichOSDictationPanel");
    }

    /// INVARIANT: Secure Event Input is read without failing, on or off, in a test process.
    #[test]
    fn secure_input_reads() {
        let _ = secure_input();
    }
}
