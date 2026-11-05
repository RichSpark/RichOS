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
use richos_voice::dictation::{delivery_permission, insert_plan, restore_clipboard, spaced, DeliveryPermission, FieldIdentity, Insert, PLAIN_TEXT_TYPE, RESTORE_AFTER, TRANSIENT_TYPE};
use richos_voice::dictation_bar::{words_range, Rect};
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
    fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, value: CFTypeRef) -> i32;
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
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> bool;
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
static PENDING_RESTORE: Mutex<Option<ClipboardRestore>> = Mutex::new(None);

struct ClipboardRestore {
    join: std::thread::JoinHandle<()>,
    canceled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ClipboardRestore {
    fn finish(self, cancel: bool) {
        if cancel {
            self.canceled.store(true, std::sync::atomic::Ordering::Release);
            self.join.thread().unpark();
        }
        self.join.join().ok();
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct AxRange {
    location: isize,
    length: isize,
}

/// An owned AX or CF reference, released on drop.
struct Owned(CFTypeRef);
impl Clone for Owned {
    fn clone(&self)->Self {
        // SAFETY: this instance owns a live +1 reference, retained independently.
        Self(unsafe { CFRetain(self.0) })
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a +1 reference we own.
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Only native identities and a display name, never field contents. AX elements
/// are immutable reference-counted proxies; the writer owns their references and uses
/// Accessibility's thread-safe messaging APIs, without AppKit window access.
#[derive(Clone)]
pub struct Destination {
    pub pid: Option<i32>,
    pub name: String,
    window: Option<Owned>,
    field: Option<Owned>,
    pub secure: bool,
}

/// One exact source range. This never reads the clipboard or the whole document
/// as editing context. AX references retain identity while a proposal is open.
pub struct Anchor {
    destination: Destination,
    range: AxRange,
    selection: AxRange,
    pub original: String,
}

// SAFETY: the owned immutable AX references have the same transfer contract as Destination.
unsafe impl Send for Anchor {}

pub enum Replacement { Replaced { text:String, anchor:Option<Anchor> }, Check, CopyOnly }

impl Anchor {
    pub fn selected()->Option<Self> {
        let destination=Destination::capture();
        if destination.secure {return None}
        let field=destination.field.as_ref()?;
        let range=selected_range(field)?;
        if range.length<=0 {return None}
        let original=range_text(field,range.location,range.length)?;
        if original.trim().is_empty() {return None}
        Some(Self {destination,range,selection:range,original})
    }

    pub fn inserted(destination:&Destination, start:isize, original:String)->Option<Self> {
        let field=destination.field.as_ref()?;
        let range=AxRange {location:start,length:original.encode_utf16().count() as isize};
        if range_text(field,range.location,range.length).as_deref()!=Some(&original) {return None}
        Some(Self {destination:destination.clone(),range,selection:selected_range(field)?,original})
    }

    /// Freeze the caret/selection when help is requested, not when the earlier
    /// dictation finished. Subsequent user edits or navigation invalidate it.
    pub fn requested(&self)->Option<Self> {
        let field=self.destination.field.as_ref()?;
        if self.destination.permission(false)!=DeliveryPermission::Paste ||
            range_text(field,self.range.location,self.range.length).as_deref()!=Some(&self.original) {return None}
        Some(Self {destination:self.destination.clone(),range:self.range,
            selection:selected_range(field)?,original:self.original.clone()})
    }

    pub fn destination_name(&self)->&str {&self.destination.name}
    pub fn source_range(&self)->(isize,isize) {(self.range.location,self.range.length)}

    pub fn can_replace(&self, proposal:&str)->bool {
        let Some(field)=self.destination.field.as_ref() else{return false};
        // Prose applications with native edit/undo behavior. Other fields can
        // still use the revision through explicit Copy. Browser/Slack behavior
        // remains subject to the real per-app release matrix.
        let prose=self.destination.pid.and_then(bundle_of).is_some_and(|id| {
            if matches!(id.as_str(),"com.apple.TextEdit"|"com.apple.mail"|"com.tinyspeck.slackmacgap") {return true}
            if !matches!(id.as_str(),"com.apple.Safari"|"com.google.Chrome") {return false}
            // Read only target metadata, never page text. Unknown browser
            // execution semantics, including terminals, remain explicit Copy.
            let mut element=Some(field.clone());
            for _ in 0..12 {
                let Some(current)=element else {break};
                if ["AXURL","AXDocument"].iter().filter_map(|key|string_attribute(current.0,key))
                    .any(|url|prose_browser_url(&url)) {return true}
                element=attribute(current.0,"AXParent");
            }
            false
        });
        let role=string_attribute(field.0,"AXRole");
        prose && role.as_deref()==Some("AXTextArea") && !proposal.contains('\0') &&
            self.destination.permission(false)==DeliveryPermission::Paste &&
            selected_range(field)==Some(self.selection) &&
            range_text(field,self.range.location,self.range.length).as_deref()==Some(&self.original)
    }

    pub fn replace(&self, proposal:&str, v_code:u16, stop:&std::sync::atomic::AtomicBool)->Result<Replacement,String> {
        if !self.can_replace(proposal) {return Ok(Replacement::CopyOnly)}
        let field=self.destination.field.as_ref().ok_or("The original field is unavailable")?;
        let value=Owned(unsafe { AXValueCreate(AX_VALUE_CF_RANGE,&self.range as *const _ as *const c_void) });
        if value.0.is_null() {return Ok(Replacement::CopyOnly)}
        let attribute=CFString::from_static_string("AXSelectedTextRange");
        // Selecting a range is not a document edit. Only the existing serialized
        // paste below changes text, so the edit is one ordinary Undo operation.
        if unsafe {AXUIElementSetAttributeValue(field.0,attribute.as_concrete_TypeRef() as CFTypeRef,value.0)}!=0 {
            return Ok(Replacement::CopyOnly)
        }
        if self.destination.permission(false)!=DeliveryPermission::Paste || selected_range(field)!=Some(self.range) ||
            range_text(field,self.range.location,self.range.length).as_deref()!=Some(&self.original) {
            return Ok(Replacement::CopyOnly)
        }
        let result=insert_checked(proposal,v_code,&self.destination,
            &std::sync::atomic::AtomicBool::new(false),stop,false,Some(self))?;
        Ok(match result.delivery {Delivery::Added=>{
            let (start,text)=result.landed.unwrap_or((self.range.location,proposal.to_string()));
            let anchor=Self::inserted(&self.destination,start,text.clone());
            Replacement::Replaced {text,anchor}
        },
            Delivery::Sent|Delivery::Check=>Replacement::Check,_=>Replacement::CopyOnly})
    }

    fn source_selected(&self)->bool {
        self.destination.field.as_ref().is_some_and(|field|selected_range(field)==Some(self.range) &&
            range_text(field,self.range.location,self.range.length).as_deref()==Some(&self.original))
    }
}

fn prose_browser_url(value:&str)->bool {
    tauri::Url::parse(value).is_ok_and(|url|url.scheme()=="https" && url.port_or_known_default()==Some(443) &&
        matches!(url.host_str(),Some("mail.google.com"|"docs.google.com"|"app.slack.com")))
}

fn selected_range(field:&Owned)->Option<AxRange> {
    let value=attribute(field.0,"AXSelectedTextRange")?;
    let mut range=AxRange {location:0,length:0};
    if !unsafe {AXValueGetValue(value.0,AX_VALUE_CF_RANGE,&mut range as *mut _ as *mut c_void)} ||
        range.location<0 || range.length<0 {return None}
    Some(range)
}

// SAFETY: ownership transfers retained immutable AX proxy references. No mutable
// application object or borrowed pointer crosses threads through this type.
unsafe impl Send for Destination {}

impl Destination {
    pub fn capture() -> Self {
        let pid = frontmost_pid();
        let app = pid.map(|pid| Owned(unsafe { AXUIElementCreateApplication(pid) }));
        let focused = app.as_ref().and_then(|a| attribute(a.0, "AXFocusedUIElement"))
            .or_else(focused_element).filter(|e| {
            let mut owner = 0;
            (unsafe { AXUIElementGetPid(e.0, &mut owner) == 0 }) && Some(owner) == pid
        });
        let window = app.as_ref().and_then(|a| attribute(a.0, "AXFocusedWindow"))
            .or_else(|| focused.as_ref().and_then(|e| attribute(e.0, "AXWindow")));
        let secure = focused.as_ref().is_some_and(|e| {
            ["AXRole", "AXSubrole"].iter().any(|attribute_name|
                string_attribute(e.0, attribute_name).as_deref() == Some("AXSecureTextField"))
        });
        let field = focused.filter(|e| string_attribute(e.0, "AXRole").is_some_and(|role|
            richos_voice::dictation::typing_role(&role)) || attribute(e.0, "AXSelectedTextRange").is_some());
        let name = pid.and_then(app_name).unwrap_or_else(|| "the destination app".into());
        Self { pid, name, window, field, secure }
    }

    pub fn permission(&self, navigated: bool) -> DeliveryPermission {
        let now = Self::capture();
        let same_app = self.pid.is_some() && self.pid == now.pid;
        let same_window = self.window.as_ref().zip(now.window.as_ref())
            .is_some_and(|(a, b)| unsafe { CFEqual(a.0, b.0) });
        let field = match (&self.field, &now.field) {
            (None, _) => FieldIdentity::UnreadableFromStart,
            (Some(a), Some(b)) if unsafe { CFEqual(a.0, b.0) } => FieldIdentity::Same,
            _ => FieldIdentity::ChangedOrLost,
        };
        delivery_permission(same_app, same_window, field, now.secure, navigated)
    }
}

fn app_name(pid: i32) -> Option<String> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSRunningApplication")?;
        // SAFETY: documented read-only NSRunningApplication messages.
        unsafe {
            let app: *mut AnyObject = msg_send![class, runningApplicationWithProcessIdentifier: pid];
            if app.is_null() { return None; }
            let name: *mut AnyObject = msg_send![app, localizedName];
            if name.is_null() { return None; }
            let utf8: *const std::ffi::c_char = msg_send![name, UTF8String];
            (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
        }
    })
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
        let secure = string_attribute(element.0, "AXSubrole").as_deref() == Some("AXSecureTextField");
        if let Some((around, start)) = (!secure).then(|| around_cursor(element.0)).flatten() {
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
fn words_rect(element: &Owned, selection_start: isize, text: &str) -> Option<Rect> {
    let (location, length) = words_range(selection_start, text);
    let want = AxRange { location, length };
    // SAFETY: one owned CFRange and a parameterized AX read on a retained element.
    let parameter = Owned(unsafe { AXValueCreate(AX_VALUE_CF_RANGE, &want as *const _ as *const c_void) });
    if parameter.0.is_null() { return None; }
    let name = CFString::from_static_string("AXBoundsForRange");
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyParameterizedAttributeValue(element.0, name.as_concrete_TypeRef() as CFTypeRef, parameter.0, &mut value)
    };
    let value = Owned(value);
    if error != 0 || value.0.is_null() { return None; }
    let mut bounds = [0f64; 4];
    // SAFETY: AXValueGetValue writes a CGRect to a correctly sized four-double buffer.
    if !unsafe { AXValueGetValue(value.0, AX_VALUE_CG_RECT, bounds.as_mut_ptr() as *mut c_void) } { return None; }
    Some(Rect { x: bounds[0], y: bounds[1], w: bounds[2], h: bounds[3] })
}

fn range_text(element: &Owned, start: isize, length: isize) -> Option<String> {
    if start < 0 || length < 0 { return None; }
    let range = AxRange { location: start, length };
    // SAFETY: one owned CFRange value and one retained AX proxy.
    let parameter = Owned(unsafe { AXValueCreate(AX_VALUE_CF_RANGE, &range as *const _ as *const c_void) });
    if parameter.0.is_null() { return None; }
    let name = CFString::from_static_string("AXStringForRange");
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe { AXUIElementCopyParameterizedAttributeValue(element.0, name.as_concrete_TypeRef() as CFTypeRef, parameter.0, &mut value) };
    let owned = Owned(value);
    if error != 0 || owned.0.is_null() { return None; }
    // SAFETY: check the returned CF type before borrowing it as a string.
    let typed = unsafe { CFType::wrap_under_get_rule(owned.0) };
    typed.downcast::<CFString>().map(|s| s.to_string())
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Added,
    Sent,
    Check,
    NotAdded { copied: bool, destination_changed: bool },
    DiscardedSecure,
}

impl Delivery {
    pub fn unresolved(self) -> bool { matches!(self, Self::Check | Self::NotAdded { .. }) }
    pub fn tag(self) -> &'static str {
        match self {
            Self::Added => "added", Self::Sent => "sent", Self::Check => "check",
            Self::NotAdded { .. } => "not-added", Self::DiscardedSecure => "discarded-secure",
        }
    }
}

pub struct Inserted {
    pub how: Insert,
    pub spaced: bool,
    pub seen: String,
    pub landed: Option<(isize, String)>,
    pub rect: Option<Rect>,
    pub delivery: Delivery,
}

/// One serialized paste into the same captured app/window/available field. No
/// focus switching, retry, Enter or execution. Clipboard restoration is prepared
/// before dispatch, so a failure to create that worker cannot become a false
/// "not added" after a paste was already sent.
pub fn insert(words: &str, v_code: u16, destination: &Destination,
              navigated: &std::sync::atomic::AtomicBool, stop: &std::sync::atomic::AtomicBool) -> Result<Inserted, String> {
    insert_checked(words,v_code,destination,navigated,stop,true,None)
}

fn text_for_paste(words:&str, around:Option<(Option<char>,Option<char>)>, replacing:bool)->String {
    match (replacing,around) {
        (false,Some((before,after)))=>spaced(words,before,after),
        _=>words.to_string(),
    }
}

fn insert_checked(words:&str, v_code:u16, destination:&Destination,
    navigated:&std::sync::atomic::AtomicBool, stop:&std::sync::atomic::AtomicBool,
    copy_on_failure:bool, anchor:Option<&Anchor>)->Result<Inserted,String> {
    use std::sync::atomic::Ordering;
    let mut pending = PENDING_RESTORE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(previous) = pending.take() { previous.finish(false); }
    if stop.load(Ordering::Acquire) { return Err("dictation canceled".into()); }
    let permission = destination.permission(navigated.load(Ordering::Acquire));
    let empty = |delivery| Inserted { how: Insert::CopyOnly, spaced: false, seen: String::new(), landed: None, rect: None, delivery };
    if permission == DeliveryPermission::DiscardSecure { return Ok(empty(Delivery::DiscardedSecure)); }
    let pb = general().ok_or("no general pasteboard")?;
    if permission == DeliveryPermission::Recover {
        let copied = copy_on_failure && write_words(pb, words).is_some();
        return Ok(empty(Delivery::NotAdded { copied, destination_changed: true }));
    }
    if anchor.is_some_and(|a|!a.source_selected()) {return Ok(empty(Delivery::NotAdded {copied:false,destination_changed:true}))}
    let f = focus();
    let how = insert_plan(f.focused_element, f.window_focused, f.finder_in_front, f.focused_role.as_deref());
    // Exact selected text is replaced verbatim. Caret spacing belongs only to
    // ordinary dictation, never to a paragraph or a selection inside quotes.
    let text=text_for_paste(words,if how==Insert::Paste {f.around}else{None},anchor.is_some());
    if how == Insert::CopyOnly {
        let copied = copy_on_failure && write_words(pb, &text).is_some();
        return Ok(empty(Delivery::NotAdded { copied, destination_changed: false }));
    }
    let can_verify = f.selection_start.zip(destination.field.as_ref())
        .is_some_and(|(start, field)| range_text(field, start, 0).is_some());
    // Seeing the requested words afterward is not evidence of insertion if
    // those exact words were already at that range before Command-V.
    let already_present = f.selection_start.zip(destination.field.as_ref())
        .is_some_and(|(start, field)| range_text(field, start, text.encode_utf16().count() as isize).as_deref() == Some(text.as_str()));
    let (restore_tx, restore_rx) = std::sync::mpsc::channel::<(usize, isize, Saved)>();
    let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = canceled.clone();
    let restore = std::thread::Builder::new().name("dictation-clipboard".into()).spawn(move || {
        if let Ok((address, written, saved)) = restore_rx.recv() {
            let deadline = std::time::Instant::now() + RESTORE_AFTER;
            while let Some(wait) = deadline.checked_duration_since(std::time::Instant::now()) {
                if flag.load(std::sync::atomic::Ordering::Acquire) { return; }
                std::thread::park_timeout(wait);
            }
            if flag.load(std::sync::atomic::Ordering::Acquire) { return; }
            let pb = address as *mut AnyObject;
            if restore_clipboard(change_count(pb) as i64, written as i64) { write_items(pb, saved.items()); }
        }
    }).map_err(|e| e.to_string())?;
    let saved = save(pb);
    let written = match write_words(pb, &text) {
        Some(written) => written,
        None => { drop(restore_tx); restore.join().ok(); return Ok(empty(Delivery::NotAdded { copied: false, destination_changed: false })); }
    };
    let permission = destination.permission(navigated.load(Ordering::Acquire));
    if stop.load(Ordering::Acquire) || permission == DeliveryPermission::DiscardSecure {
        if change_count(pb) == written { write_items(pb, saved.items()); }
        drop(restore_tx); restore.join().ok();
        return if stop.load(Ordering::Acquire) { Err("dictation canceled".into()) }
               else { Ok(empty(Delivery::DiscardedSecure)) };
    }
    if permission == DeliveryPermission::Recover || anchor.is_some_and(|a|!a.source_selected()) {
        // No paste happened. The just-written words are the deliberate auto-copy.
        if !copy_on_failure {
            if change_count(pb)==written {write_items(pb,saved.items());}
            drop(restore_tx); restore.join().ok();
            return Ok(empty(Delivery::NotAdded {copied:false,destination_changed:true}))
        }
        drop(restore_tx); restore.join().ok();
        return Ok(empty(Delivery::NotAdded { copied: true, destination_changed: true }));
    }
    post_command(v_code);
    if restore_tx.send((pb as usize, written, saved)).is_err() {
        super::log::line("clipboard restoration worker ended after dispatch; paste will not be retried");
    }
    *pending = Some(ClipboardRestore { join: restore, canceled });
    let landed = f.selection_start.map(|start| (start, text.clone()));
    let mut delivery = Delivery::Sent;
    let mut rect = None;
    if can_verify {
        delivery = Delivery::Check;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(600);
        loop {
            if already_present { break; }
            if stop.load(Ordering::Acquire) { break; }
            if destination.permission(navigated.load(Ordering::Acquire)) != DeliveryPermission::Paste { break; }
            if let Some((start, field)) = f.selection_start.zip(destination.field.as_ref()) {
                if range_text(field, start, text.encode_utf16().count() as isize).as_deref() == Some(text.as_str()) {
                    delivery = Delivery::Added;
                    rect = words_rect(field, start, &text);
                    break;
                }
            }
            if std::time::Instant::now() >= deadline { break; }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    Ok(Inserted { how, spaced: anchor.is_none() && f.around.is_some(), seen: f.seen, landed, rect, delivery })
}

/// Copy `words` to the clipboard and nothing else: the case where the words are ready and
/// Accessibility is not allowed ("I can't type into other apps yet, so I copied your words.").
pub fn copy_only(words: &str) -> Result<(), String> {
    let mut pending = PENDING_RESTORE.lock().unwrap_or_else(|p| p.into_inner());
    let pb = general().ok_or("no general pasteboard")?;
    copy_on(pb, words, &mut pending)
}

fn copy_on(pb: *mut AnyObject, words: &str, pending: &mut Option<ClipboardRestore>) -> Result<(), String> {
    // Cancel and join before copying. A restore that already passed its change-
    // count check must finish before the new copy, never overwrite it afterward.
    if let Some(previous) = pending.take() { previous.finish(true); }
    write_words(pb, words).map(|_| ()).ok_or_else(|| "the words could not be written to the clipboard".to_string())
}

/// Wait for a restore still pending, so the tool never exits with his clipboard holding words.
pub fn settle() {
    if let Some(previous) = PENDING_RESTORE.lock().unwrap_or_else(|p| p.into_inner()).take() {
        previous.finish(false);
    }
}

/// Workspace-only routing check, before any Accessibility lookup.
pub fn front_is_finder()->bool {frontmost_pid().and_then(bundle_of).as_deref()==Some("com.apple.finder")}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_replacement_preserves_paragraphs_and_opening_punctuation() {
        for (text,before,after) in [
            ("Please review the forecast.\n",Some('\n'),Some('T')),
            ("revised",Some('"'),Some('"')),
            ("revised",Some('('),Some(')')),
        ] {assert_eq!(super::text_for_paste(text,Some((before,after)),true),text);}
        assert_eq!(super::text_for_paste("spoken words",Some((Some('x'),Some('y'))),false)," spoken words ",
            "Ordinary caret insertion keeps its existing spacing");
    }

    #[test]
    fn browser_replacement_requires_a_known_prose_origin() {
        for url in ["https://mail.google.com/mail/u/0/","https://docs.google.com/document/d/draft","https://app.slack.com/client/workspace"] {
            assert!(super::prose_browser_url(url));
        }
        for url in ["https://mail.google.com.evil.example/","https://docs.google.com:8443/","http://mail.google.com/","file:///draft.html","https://example.com/terminal"] {
            assert!(!super::prose_browser_url(url));
        }
    }
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

    #[test]
    fn an_in_flight_restore_finishes_before_a_new_recovery_copy() {
        let pb = private_pasteboard();
        assert!(write_words(pb, "temporary dictation").is_some());
        let address = pb as usize;
        let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = canceled.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            // Model the old worker already past its change-count/cancel check.
            // It deliberately writes even after cancellation. Joining it before
            // the new copy is what makes that copy reliable.
            ready_tx.send(()).unwrap();
            while !flag.load(std::sync::atomic::Ordering::Acquire) { std::thread::park(); }
            write_words(address as *mut AnyObject, "original clipboard").unwrap();
        });
        ready_rx.recv().unwrap();
        let mut pending = Some(ClipboardRestore { join, canceled });
        copy_on(pb, "recovered words", &mut pending).unwrap();
        assert!(pending.is_none());
        assert_eq!(string_on(pb, PLAIN_TEXT_TYPE).as_deref(), Some("recovered words"));
        // SAFETY: only the private pasteboard created by this test.
        unsafe { let _: () = msg_send![pb, releaseGlobally]; }
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

