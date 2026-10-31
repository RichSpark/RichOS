//! **Putting the words where the cursor is** (plan section 5): one path for every app, RichOS's
//! own text boxes included.
//!
//! The rule is open-wispr's `TextInserter` (MIT; see `docs/legal/THIRD-PARTY-NOTICES.md`),
//! translated into Rust: save every clipboard item, write the words, post Command-V with V's key
//! code resolved from the current keyboard layout, and put the clipboard back after 1.0 s only if
//! nothing else wrote to it. Added around it, as the plan states: the no-focused-element check,
//! so a missing text box is said instead of silent (only nothing focused, element or window, or
//! Finder's desktop, Frank's M7), the spacing rule where Accessibility can read the text, and the
//! `org.nspasteboard.TransientType` marker. Every decision is in `richos_voice::dictation`.

use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use richos_voice::dictation::{insert_plan, restore_clipboard, spaced, Insert, PLAIN_TEXT_TYPE, RESTORE_AFTER, TRANSIENT_TYPE};
use richos_voice::dictation_bar::{paste_landed, words_range, Rect};
use std::ffi::{c_void, CStr, CString};
use std::sync::Mutex;

type AXUIElementRef = *const c_void;
type CFTypeRef = *const c_void;
type CGEventSourceRef = *mut c_void;
type CGEventRef = *mut c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    fn AXUIElementCopyParameterizedAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, parameter: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn CGEventSourceCreate(state: i32) -> CGEventSourceRef;
    fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, code: u16, down: bool) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: CFTypeRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn LMGetKbdType() -> u8;
    #[allow(clippy::too_many_arguments)]
    fn UCKeyTranslate(
        layout: *const u8,
        code: u16,
        action: u16,
        modifiers: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_len: usize,
        actual_len: *mut usize,
        chars: *mut u16,
    ) -> i32;
}

const AX_VALUE_CF_RANGE: u32 = 4; // kAXValueCFRangeType
const AX_VALUE_CG_POINT: u32 = 1; // kAXValueCGPointType
const AX_VALUE_CG_SIZE: u32 = 2; // kAXValueCGSizeType
const AX_VALUE_CG_RECT: u32 = 3; // kAXValueCGRectType
const HID_SYSTEM_STATE: i32 = 1; // kCGEventSourceStateHIDSystemState
const HID_EVENT_TAP: u32 = 0; // kCGHIDEventTap
const FLAG_COMMAND: u64 = 0x0010_0000; // kCGEventFlagMaskCommand
const ANSI_V: u16 = 9; // kVK_ANSI_V, when the layout cannot be read
const UC_KEY_ACTION_DISPLAY: u16 = 3;
const UC_NO_DEAD_KEYS: u32 = 1;

/// One paste at a time, from saving his clipboard to putting it back: a second dictation's save
/// must never capture the first one's words. The lock is held for a whole insert, and an insert
/// first waits for the previous one's restore to finish.
static PENDING_RESTORE: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

#[repr(C)]
struct AxRange {
    location: isize,
    length: isize,
}

/// An owned AX or CF reference, released on drop.
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a +1 reference we own.
            unsafe { CFRelease(self.0) };
        }
    }
}

fn attribute(element: AXUIElementRef, name: &'static str) -> Option<Owned> {
    asked(element, name).ok()
}

/// One Accessibility attribute, or the `AXError` it answered with (a missing value reads as
/// `kAXErrorNoValue`, -25212).
fn asked(element: AXUIElementRef, name: &'static str) -> Result<Owned, i32> {
    let name = CFString::from_static_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: a live element, a CFString attribute name and an out-parameter.
    let error = unsafe { AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef() as CFTypeRef, &mut value) };
    match (error, value.is_null()) {
        (0, false) => Ok(Owned(value)),
        (0, true) => Err(-25212),
        (e, false) => {
            // An error that came with a value: the value is still ours to release.
            drop(Owned(value));
            Err(e)
        }
        (e, true) => Err(e),
    }
}

fn answer(r: &Result<Owned, i32>) -> String {
    match r {
        Ok(_) => "yes".into(),
        Err(e) => format!("none ({e})"),
    }
}

/// What Accessibility says about where the words would go.
#[derive(Debug, Default)]
pub struct Focus {
    pub focused_element: bool,
    /// The app in front reports a focused window (`AXFocusedWindow`, or `AXMainWindow`). Chrome
    /// can report one when it reports no focused element.
    pub window_focused: bool,
    pub finder_in_front: bool,
    /// The focused element's `AXRole`, where it answers one: in Finder, what tells a rename
    /// field from the desktop's icon view (walk-36699815244d).
    pub focused_role: Option<String>,
    /// The characters on either side of the cursor, where Accessibility can read them.
    pub around: Option<(Option<char>, Option<char>)>,
    /// Where the selection began (UTF-16 units), where Accessibility can read it: the words'
    /// range starts here once pasted, for the flight (slice 3).
    pub selection_start: Option<isize>,
    /// The front app's bundle identifier, for the log's "into" field.
    pub front_bundle: Option<String>,
    /// What Accessibility answered, for the log: which app, and each question's answer or error.
    pub seen: String,
}

/// Ask Accessibility for the focused element and the text around its cursor.
///
/// The system-wide element is asked first. Where it does not answer, the front app (from
/// Accessibility, or from `NSWorkspace` when Accessibility cannot name it) is asked directly:
/// an app that builds its accessibility tree only for an assistive client can still answer for
/// itself. Every answer and error code goes to [`Focus::seen`], so a copy instead of a paste in
/// the log says exactly what Accessibility reported.
pub fn focus() -> Focus {
    let mut out = Focus::default();
    // SAFETY: creates a +1 system-wide element we release.
    let system = Owned(unsafe { AXUIElementCreateSystemWide() });
    let ax_app = asked(system.0, "AXFocusedApplication");
    let pid = match &ax_app {
        Ok(app) => {
            let mut pid = 0i32;
            // SAFETY: a live element and an out-parameter.
            (unsafe { AXUIElementGetPid(app.0, &mut pid) } == 0).then_some(pid)
        }
        Err(_) => None,
    }
    .or_else(frontmost_pid);
    let app_from = if ax_app.is_ok() { "Accessibility" } else { "the workspace" };
    let app = match ax_app {
        Ok(app) => Some(app),
        // SAFETY: creates a +1 application element we release.
        Err(_) => pid.map(|p| Owned(unsafe { AXUIElementCreateApplication(p) })),
    };
    out.front_bundle = pid.and_then(bundle_of);
    out.finder_in_front = out.front_bundle.as_deref() == Some("com.apple.finder");
    let mut element = asked(system.0, "AXFocusedUIElement");
    let mut element_from = "system-wide";
    if let (Err(_), Some(app)) = (&element, &app) {
        if let Ok(e) = asked(app.0, "AXFocusedUIElement") {
            element = Ok(e);
            element_from = "the app";
        }
    }
    let window = match &app {
        Some(app) => asked(app.0, "AXFocusedWindow").or_else(|_| asked(app.0, "AXMainWindow")),
        None => Err(0),
    };
    out.window_focused = window.is_ok();
    if let Ok(element) = &element {
        out.focused_role = string_attribute(element.0, "AXRole");
    }
    out.seen = format!(
        "into {} (from {app_from}), focused element {} ({}asked {element_from}), window {}",
        out.front_bundle.as_deref().unwrap_or("an unknown app"),
        answer(&element),
        out.focused_role.as_deref().map(|r| format!("{r}, ")).unwrap_or_default(),
        answer(&window),
    );
    if let Ok(element) = element {
        out.focused_element = true;
        if let Some((around, start)) = around_cursor(element.0) {
            out.around = Some(around);
            out.selection_start = Some(start);
        }
    }
    out
}

/// The focused element, asked of the system-wide element and then of the front app, as
/// [`focus`] asks.
fn focused_element() -> Option<Owned> {
    // SAFETY: creates a +1 system-wide element we release.
    let system = Owned(unsafe { AXUIElementCreateSystemWide() });
    if let Ok(e) = asked(system.0, "AXFocusedUIElement") {
        return Some(e);
    }
    // SAFETY: creates a +1 application element we release.
    let app = Owned(unsafe { AXUIElementCreateApplication(frontmost_pid()?) });
    asked(app.0, "AXFocusedUIElement").ok()
}

/// **The front app's focused window on screen** (top-left points), for placing the bar on the
/// screen that holds it (plan section 6). `None` where Accessibility cannot say.
pub fn focused_window_rect() -> Option<Rect> {
    // SAFETY: creates a +1 application element we release.
    let app = Owned(unsafe { AXUIElementCreateApplication(frontmost_pid()?) });
    let window = asked(app.0, "AXFocusedWindow").or_else(|_| asked(app.0, "AXMainWindow")).ok()?;
    let position = attribute(window.0, "AXPosition")?;
    let size = attribute(window.0, "AXSize")?;
    let mut p = [0f64; 2];
    let mut z = [0f64; 2];
    // SAFETY: AXValues of the documented kinds and CGPoint / CGSize-sized out-parameters.
    let ok = unsafe {
        AXValueGetValue(position.0, AX_VALUE_CG_POINT, p.as_mut_ptr() as *mut c_void)
            && AXValueGetValue(size.0, AX_VALUE_CG_SIZE, z.as_mut_ptr() as *mut c_void)
    };
    ok.then_some(Rect { x: p[0], y: p[1], w: z[0], h: z[1] })
}

/// **Where the pasted words are on screen** (plan section 5, point 3: "Where Accessibility
/// gives the inserted words' screen rectangle, the words fly and are lit"). Waits, a few times
/// 60 ms apart, for the app's cursor to sit right after the words (the paste is asynchronous:
/// Command-V reaches the app on its own run loop), then asks `AXBoundsForRange` for their range.
/// `None` when the app never says: then the bar says "Added" with no flight.
pub fn words_rect(selection_start: isize, text: &str) -> Option<Rect> {
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(60));
        let Some(element) = focused_element() else { continue };
        let Some(range) = attribute(element.0, "AXSelectedTextRange") else { continue };
        let mut r = AxRange { location: 0, length: 0 };
        // SAFETY: an AXValue and a CFRange-sized out-parameter.
        if !unsafe { AXValueGetValue(range.0, AX_VALUE_CF_RANGE, &mut r as *mut _ as *mut c_void) } {
            continue;
        }
        if !paste_landed(selection_start, text, r.location) {
            continue;
        }
        let (location, length) = words_range(selection_start, text);
        let want = AxRange { location, length };
        // SAFETY: a CFRange AXValue we own, a CFString attribute name and an out-parameter.
        let parameter = Owned(unsafe { AXValueCreate(AX_VALUE_CF_RANGE, &want as *const _ as *const c_void) });
        if parameter.0.is_null() {
            return None;
        }
        let name = CFString::from_static_string("AXBoundsForRange");
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyParameterizedAttributeValue(element.0, name.as_concrete_TypeRef() as CFTypeRef, parameter.0, &mut value)
        };
        let value = Owned(value);
        if error != 0 || value.0.is_null() {
            return None;
        }
        let mut b = [0f64; 4];
        // SAFETY: an AXValue of kind CGRect and a CGRect-sized out-parameter.
        if !unsafe { AXValueGetValue(value.0, AX_VALUE_CG_RECT, b.as_mut_ptr() as *mut c_void) } {
            return None;
        }
        return Some(Rect { x: b[0], y: b[1], w: b[2], h: b[3] });
    }
    None
}

/// The front app's process, from `NSWorkspace`, when Accessibility cannot name it.
fn frontmost_pid() -> Option<i32> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSWorkspace")?;
        // SAFETY: documented class method and NSRunningApplication accessor.
        unsafe {
            let ws: *mut AnyObject = msg_send![class, sharedWorkspace];
            if ws.is_null() {
                return None;
            }
            let app: *mut AnyObject = msg_send![ws, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let pid: i32 = msg_send![app, processIdentifier];
            (pid > 0).then_some(pid)
        }
    })
}

/// A string attribute of an element, where it answers one.
fn string_attribute(element: AXUIElementRef, name: &'static str) -> Option<String> {
    let value = attribute(element, name)?;
    // SAFETY: `value` is a +1 reference; the wrapper takes its own.
    Some(unsafe { CFType::wrap_under_get_rule(value.0 as _) }.downcast::<CFString>()?.to_string())
}

/// The characters either side of the cursor, and where the selection begins.
type AroundCursor = ((Option<char>, Option<char>), isize);

fn around_cursor(element: AXUIElementRef) -> Option<AroundCursor> {
    let value = attribute(element, "AXValue")?;
    let range = attribute(element, "AXSelectedTextRange")?;
    // SAFETY: `value` is a +1 reference we hand to a wrapper that now owns it.
    let text = unsafe { CFType::wrap_under_get_rule(value.0 as _) }.downcast::<CFString>()?.to_string();
    let mut r = AxRange { location: 0, length: 0 };
    // SAFETY: an AXValue and a CFRange-sized out-parameter.
    if !unsafe { AXValueGetValue(range.0, AX_VALUE_CF_RANGE, &mut r as *mut _ as *mut c_void) } {
        return None;
    }
    Some((chars_around(&text, r.location, r.length), r.location))
}

/// The characters either side of a selection given, as Accessibility gives it, in UTF-16 units.
/// `None` at either end of the text. A surrogate half reads as a letter: not whitespace, and not
/// a digit, which is what the spacing rule needs to know about an emoji.
fn chars_around(text: &str, location: isize, length: isize) -> (Option<char>, Option<char>) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let unit = |i: isize| -> Option<char> {
        if i < 0 {
            return None;
        }
        units.get(i as usize).map(|u| char::from_u32(u32::from(*u)).unwrap_or('x'))
    };
    (unit(location - 1), unit(location + length))
}

fn bundle_of(pid: i32) -> Option<String> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSRunningApplication")?;
        // SAFETY: documented class method and NSString accessors.
        unsafe {
            let app: *mut AnyObject = msg_send![class, runningApplicationWithProcessIdentifier: pid];
            if app.is_null() {
                return None;
            }
            let id: *mut AnyObject = msg_send![app, bundleIdentifier];
            utf8(id)
        }
    })
}

/// # Safety
/// `s` is null or a live NSString.
unsafe fn utf8(s: *mut AnyObject) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let p: *const std::ffi::c_char = msg_send![s, UTF8String];
    (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
}

/// # Safety
/// Call inside an autorelease pool; the result is autoreleased.
unsafe fn ns_string(s: &str) -> Option<*mut AnyObject> {
    let c = CString::new(s.replace('\0', "")).ok()?;
    let class = AnyClass::get(c"NSString")?;
    let out: *mut AnyObject = msg_send![class, stringWithUTF8String: c.as_ptr()];
    (!out.is_null()).then_some(out)
}

/// **V's key code on the current keyboard layout** (open-wispr: `UCKeyTranslate` over key codes
/// 0 to 127), or `kVK_ANSI_V`. Text Input Sources must be read on the MAIN thread on current
/// macOS, so the tool calls this through its main thread.
pub fn v_key_code() -> u16 {
    // SAFETY: TIS returns a +1 source we release; the layout data belongs to it.
    unsafe {
        let source = Owned(TISCopyCurrentKeyboardLayoutInputSource());
        if source.0.is_null() {
            return ANSI_V;
        }
        let data = TISGetInputSourceProperty(source.0, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return ANSI_V;
        }
        let layout = CFDataGetBytePtr(data);
        let kind = u32::from(LMGetKbdType());
        for code in 0u16..128 {
            let mut dead = 0u32;
            let mut len = 0usize;
            let mut chars = [0u16; 4];
            let status = UCKeyTranslate(layout, code, UC_KEY_ACTION_DISPLAY, 0, kind, UC_NO_DEAD_KEYS, &mut dead, chars.len(), &mut len, chars.as_mut_ptr());
            if status == 0 && len == 1 && chars[0] == u16::from(b'v') {
                return code;
            }
        }
        ANSI_V
    }
}

/// Every item on the general pasteboard, every type of each, as data.
struct Saved(Vec<Vec<(Retained<AnyObject>, Retained<AnyObject>)>>);
// SAFETY: NSPasteboard data and NSString types are immutable once read; they move to the restore
// thread and are only read there.
unsafe impl Send for Saved {}
impl Saved {
    // A method, so a closure captures the whole `Saved` (which is `Send`) and not its field.
    fn items(&self) -> &[Vec<(Retained<AnyObject>, Retained<AnyObject>)>] {
        &self.0
    }
}

fn general() -> Option<*mut AnyObject> {
    let class = AnyClass::get(c"NSPasteboard")?;
    // SAFETY: a documented class method.
    let pb: *mut AnyObject = unsafe { msg_send![class, generalPasteboard] };
    (!pb.is_null()).then_some(pb)
}

fn change_count(pb: *mut AnyObject) -> isize {
    // SAFETY: a live pasteboard.
    unsafe { msg_send![pb, changeCount] }
}

fn save(pb: *mut AnyObject) -> Saved {
    objc2::rc::autoreleasepool(|_| {
        let mut out = Vec::new();
        // SAFETY: NSPasteboard and NSPasteboardItem accessors with their documented types.
        unsafe {
            let items: *mut AnyObject = msg_send![pb, pasteboardItems];
            if items.is_null() {
                return Saved(out);
            }
            let n: usize = msg_send![items, count];
            for i in 0..n {
                let item: *mut AnyObject = msg_send![items, objectAtIndex: i];
                let types: *mut AnyObject = msg_send![item, types];
                if types.is_null() {
                    continue;
                }
                let mut pairs = Vec::new();
                let t: usize = msg_send![types, count];
                for j in 0..t {
                    let ty: *mut AnyObject = msg_send![types, objectAtIndex: j];
                    let data: *mut AnyObject = msg_send![item, dataForType: ty];
                    if let (Some(ty), Some(data)) = (Retained::retain(ty), Retained::retain(data)) {
                        pairs.push((ty, data));
                    }
                }
                out.push(pairs);
            }
        }
        Saved(out)
    })
}

/// Clear the pasteboard and write `items`, each a list of (type, data).
fn write_items(pb: *mut AnyObject, items: &[Vec<(Retained<AnyObject>, Retained<AnyObject>)>]) {
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: documented NSPasteboard / NSPasteboardItem / NSMutableArray methods.
        unsafe {
            let _: isize = msg_send![pb, clearContents];
            let (Some(item_class), Some(array_class)) = (AnyClass::get(c"NSPasteboardItem"), AnyClass::get(c"NSMutableArray")) else { return };
            let array: *mut AnyObject = msg_send![array_class, array];
            for pairs in items {
                let item: Option<Retained<AnyObject>> = msg_send![item_class, new];
                let Some(item) = item else { continue };
                for (ty, data) in pairs {
                    let _: bool = msg_send![&*item, setData: &**data, forType: &**ty];
                }
                let _: () = msg_send![array, addObject: &*item];
            }
            let _: bool = msg_send![pb, writeObjects: array];
        }
    });
}

/// Write the words as one item: plain text, plus the transient marker (an empty data value
/// under `org.nspasteboard.TransientType`, the convention's own form). Returns the change count
/// after the write.
fn write_words(pb: *mut AnyObject, words: &str) -> Option<isize> {
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: as in write_items.
        unsafe {
            let text = ns_string(words)?;
            let plain = ns_string(PLAIN_TEXT_TYPE)?;
            let transient = ns_string(TRANSIENT_TYPE)?;
            let (data_class, item_class, array_class) =
                (AnyClass::get(c"NSData")?, AnyClass::get(c"NSPasteboardItem")?, AnyClass::get(c"NSArray")?);
            let empty: *mut AnyObject = msg_send![data_class, data];
            let item: Option<Retained<AnyObject>> = msg_send![item_class, new];
            let item = item?;
            let _: bool = msg_send![&*item, setString: text, forType: plain];
            let _: bool = msg_send![&*item, setData: empty, forType: transient];
            let array: *mut AnyObject = msg_send![array_class, arrayWithObject: &*item];
            let _: isize = msg_send![pb, clearContents];
            let ok: bool = msg_send![pb, writeObjects: array];
            ok.then(|| change_count(pb))
        }
    })
}

/// kVK_Command.
const COMMAND_KEY: u16 = 0x37;

/// **The keystroke Command-`code`, as a keyboard sends it**: Command down, the key down and up
/// with Command held, Command up with nothing held: (key code, down, flags) in order.
///
/// Until walk-411cc5b8669f this posted the key down and up alone, both carrying the Command
/// flag and no Command key event at all, and the HID system was left believing Command held
/// after every pasted dictation (post_key --flags read `hid 0x100000 [command]` after the
/// paste into Chrome; TextEdit's words go in through Accessibility and post nothing). Every
/// later click on the Mac was then a Command-click until a real key was pressed: on the menu
/// bar item, AppKit took it for the start of dragging the item and handed the item its mouse-up
/// alone, so the menu never opened (walk-c409831e46f9, walk-8dcad10d445b, walk-3520f87af8f9,
/// walk-e7b42c44d5d8, walk-411cc5b8669f: every failing press came after a paste into Chrome).
fn command_keystroke(code: u16) -> [(u16, bool, u64); 4] {
    [(COMMAND_KEY, true, FLAG_COMMAND), (code, true, FLAG_COMMAND), (code, false, FLAG_COMMAND), (COMMAND_KEY, false, 0)]
}

fn post_command(code: u16) {
    // SAFETY: CoreGraphics event creation and posting; every created reference is released.
    unsafe {
        let source = CGEventSourceCreate(HID_SYSTEM_STATE);
        for (key, down, flags) in command_keystroke(code) {
            let event = CGEventCreateKeyboardEvent(source, key, down);
            if event.is_null() {
                continue;
            }
            CGEventSetFlags(event, flags);
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event);
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
}

/// What happened to the words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inserted {
    pub how: Insert,
    /// Whether the spacing rule could read the text around the cursor.
    pub spaced: bool,
    pub front_bundle: Option<String>,
    /// What Accessibility answered ([`Focus::seen`]).
    pub seen: String,
    /// What was pasted (the words with the spacing rule applied) and where the selection began,
    /// when Accessibility could read it: what [`words_rect`] needs for the flight.
    pub landed: Option<(isize, String)>,
}

/// **Put `words` where the cursor is.** `v_code` is [`v_key_code`], read on the main thread.
///
/// Paste: his clipboard is saved, the words written and pasted, and after 1.0 s his clipboard is
/// put back on a thread of its own, if nothing else wrote to it meanwhile. Copy only: the words
/// are left on the clipboard for him.
pub fn insert(words: &str, v_code: u16) -> Result<Inserted, String> {
    let mut pending = PENDING_RESTORE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(previous) = pending.take() {
        previous.join().ok();
    }
    let f = focus();
    let how = insert_plan(f.focused_element, f.window_focused, f.finder_in_front, f.focused_role.as_deref());
    let pb = general().ok_or("no general pasteboard")?;
    let text = match (how, f.around) {
        (Insert::Paste, Some((before, after))) => spaced(words, before, after),
        _ => words.to_string(),
    };
    if how == Insert::CopyOnly {
        write_words(pb, &text).ok_or("the words could not be written to the clipboard")?;
        return Ok(Inserted { how, spaced: false, front_bundle: f.front_bundle, seen: f.seen, landed: None });
    }
    let saved = save(pb);
    let written = write_words(pb, &text).ok_or("the words could not be written to the clipboard")?;
    post_command(v_code);
    let pb_addr = pb as usize;
    let restore = std::thread::Builder::new()
        .name("dictation-clipboard".into())
        .spawn(move || {
            std::thread::sleep(RESTORE_AFTER);
            let pb = pb_addr as *mut AnyObject;
            if restore_clipboard(change_count(pb) as i64, written as i64) {
                write_items(pb, saved.items());
            }
        })
        .map_err(|e| e.to_string())?;
    *pending = Some(restore);
    let landed = f.selection_start.map(|start| (start, text.clone()));
    Ok(Inserted { how, spaced: f.around.is_some(), front_bundle: f.front_bundle, seen: f.seen, landed })
}

/// Copy `words` to the clipboard and nothing else: the case where the words are ready and
/// Accessibility is not allowed ("I can't type into other apps yet, so I copied your words.").
pub fn copy_only(words: &str) -> Result<(), String> {
    let _pending = PENDING_RESTORE.lock().unwrap_or_else(|p| p.into_inner());
    let pb = general().ok_or("no general pasteboard")?;
    write_words(pb, words).map(|_| ()).ok_or_else(|| "the words could not be written to the clipboard".to_string())
}

/// Wait for a restore still pending, so the tool never exits with his clipboard holding words.
pub fn settle() {
    if let Some(previous) = PENDING_RESTORE.lock().unwrap_or_else(|p| p.into_inner()).take() {
        previous.join().ok();
    }
}

/// Workspace-only routing check, before any Accessibility lookup.
pub fn front_is_finder()->bool {frontmost_pid().and_then(bundle_of).as_deref()==Some("com.apple.finder")}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT (walk-411cc5b8669f): the paste keystroke is what a keyboard sends, Command down
    /// first and Command up last with no flags, so the HID system is never left believing
    /// Command held after a pasted dictation. RED before this: two events, the key down and up,
    /// both with Command set and no Command key event at all.
    #[test]
    fn the_paste_keystroke_lets_command_go() {
        let keys = command_keystroke(9);
        assert_eq!(keys[0], (COMMAND_KEY, true, FLAG_COMMAND), "Command goes down first");
        assert_eq!(keys[1], (9, true, FLAG_COMMAND));
        assert_eq!(keys[2], (9, false, FLAG_COMMAND));
        assert_eq!(keys[3], (COMMAND_KEY, false, 0), "Command goes up last, with nothing held");
        let last_with_command = keys.iter().rposition(|(_, _, flags)| flags & FLAG_COMMAND != 0).unwrap();
        assert!(matches!(keys[last_with_command + 1..], [(COMMAND_KEY, false, 0)]), "nothing after Command's release");
        assert_eq!(keys.iter().filter(|(k, down, _)| *k == COMMAND_KEY && *down).count(),
                   keys.iter().filter(|(k, down, _)| *k == COMMAND_KEY && !*down).count(), "every Command down has its up");
    }

    /// INVARIANT: the log names each Accessibility answer, and an error by its own code, so a
    /// copy instead of a paste says what Accessibility reported.
    #[test]
    fn an_accessibility_answer_reads_as_yes_or_its_error() {
        assert_eq!(answer(&Err(-25212)), "none (-25212)");
        assert_eq!(answer(&Err(-25204)), "none (-25204)");
        // SAFETY: creates a +1 system-wide element, owned and released by `Owned`.
        let system = Owned(unsafe { AXUIElementCreateSystemWide() });
        assert_eq!(answer(&Ok(system)), "yes");
    }

    /// INVARIANT: Accessibility's ranges are UTF-16 units, so the characters either side of the
    /// cursor are found by unit, at both ends and past an emoji.
    #[test]
    fn the_characters_around_the_cursor() {
        assert_eq!(chars_around("", 0, 0), (None, None));
        assert_eq!(chars_around("Hi Dana.", 8, 0), (Some('.'), None));
        assert_eq!(chars_around("Hi Dana.", 0, 0), (None, Some('H')));
        assert_eq!(chars_around("Hi Dana.", 3, 4), (Some(' '), Some('.')), "a selection is replaced");
        // The thumbs-up is two UTF-16 units: the cursor after it is at 3, not 2.
        let (before, after) = chars_around("a\u{1F44D}b", 3, 0);
        assert!(before.is_some_and(|c| !c.is_whitespace()));
        assert_eq!(after, Some('b'));
        assert_eq!(std::mem::size_of::<AxRange>(), 16, "CFRange is two CFIndex values");
    }

    fn private_pasteboard() -> *mut AnyObject {
        let class = AnyClass::get(c"NSPasteboard").unwrap();
        // SAFETY: a documented class method; a uniquely named pasteboard is never his clipboard.
        unsafe { msg_send![class, pasteboardWithUniqueName] }
    }

    fn string_on(pb: *mut AnyObject, ty: &str) -> Option<String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let ty = ns_string(ty)?;
            let s: *mut AnyObject = msg_send![pb, stringForType: ty];
            utf8(s)
        })
    }

    fn types_of_first_item(pb: *mut AnyObject) -> Vec<String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let items: *mut AnyObject = msg_send![pb, pasteboardItems];
            let item: *mut AnyObject = msg_send![items, objectAtIndex: 0usize];
            let types: *mut AnyObject = msg_send![item, types];
            let n: usize = msg_send![types, count];
            (0..n)
                .filter_map(|i| {
                    let t: *mut AnyObject = msg_send![types, objectAtIndex: i];
                    utf8(t)
                })
                .collect()
        })
    }

    /// INVARIANT (open-wispr's rule, minor 11): what was on the pasteboard is saved, the words
    /// are written with the transient marker beside them, and, nothing having written since, what
    /// was there comes back exactly. On a private pasteboard: his clipboard is never touched.
    #[test]
    fn the_words_go_on_marked_transient_and_what_was_there_comes_back() {
        let pb = private_pasteboard();
        assert!(!pb.is_null());
        assert!(write_words(pb, "his own clipboard").is_some());
        let saved = save(pb);
        assert_eq!(saved.items().len(), 1);
        let written = write_words(pb, "Talk soon.").expect("the words are written");
        assert_eq!(string_on(pb, PLAIN_TEXT_TYPE).as_deref(), Some("Talk soon."));
        let types = types_of_first_item(pb);
        assert!(types.iter().any(|t| t == TRANSIENT_TYPE), "{types:?}");
        assert!(types.iter().any(|t| t == PLAIN_TEXT_TYPE), "{types:?}");
        assert!(restore_clipboard(change_count(pb) as i64, written as i64));
        write_items(pb, saved.items());
        assert_eq!(string_on(pb, PLAIN_TEXT_TYPE).as_deref(), Some("his own clipboard"));
        // Something written after ours changes the count: the restore stands aside.
        assert!(!restore_clipboard(change_count(pb) as i64, written as i64));
        // SAFETY: our own uniquely named pasteboard, released once.
        unsafe {
            let _: () = msg_send![pb, releaseGlobally];
        }
    }
}

