//! **The key: one active event tap, in the tool** (plan section 3; Frank's M9).
//!
//! `CGEventTapCreate` at session level, head insert, default (active) option, on its own thread
//! with its own run loop. The event mask is key down, key up and system-defined (type 14, which
//! `core-graphics 0.25`'s enum does not name, hence these few lines of FFI) and nothing else.
//! Each event is reduced to the three numbers `richos_voice::dictation::judge_key` reads; his key
//! is swallowed on down and up, everything else is returned untouched.
//!
//! - The callback is wrapped in `catch_unwind` and returns the event untouched if anything inside
//!   it fails: a panic across the FFI boundary would abort the tool.
//! - `kCGEventTapDisabledByTimeout` and `ByUserInput` re-enable the tap at once and are reported
//!   to the tool, which logs each one.
//! - The callback's longest run is kept as a number, logged once an hour by the tool, never a key.
//! - While the tap exists the tool holds an `NSProcessInfo` activity (user-initiated, latency
//!   critical, idle system sleep still allowed), so App Nap cannot slow a callback every
//!   keystroke on the Mac passes through.
//!
//! PRIVACY, by construction: the callback reads the key code, the auto-repeat flag and, for a
//! system-defined event, its subtype and data word; compares them with the chosen key; and keeps,
//! logs and sends nothing about any other event. There is no buffer and no logging in it.
//! Key capture (slice 2) adds one thing it may send: while the window is waiting for a key the
//! person is choosing (`capture-next-key`, at most [`CAPTURE_FOR_MS`]), the F-number of the next
//! function key pressed, once. Any other key is still neither kept nor sent.

use objc2::encode::{Encoding, RefEncode};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::msg_send;
use richos_voice::dictation::{captured_key, judge_key, KeyEvent, KeyVerdict};
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// `CGEventRef`'s pointee, opaque, with the encoding AppKit declares for it so `eventWithCGEvent:`
/// passes objc2's encoding check.
#[repr(C)]
pub struct CGEvent {
    _private: [u8; 0],
}
unsafe impl RefEncode for CGEvent {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CGEvent", &[]));
}

type CGEventRef = *mut CGEvent;
type CFMachPortRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFStringRef = *const c_void;
type TapCallback = extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;

pub const SESSION_EVENT_TAP: u32 = 1; // kCGSessionEventTap
pub const HEAD_INSERT: u32 = 0; // kCGHeadInsertEventTap
pub const OPTION_DEFAULT: u32 = 0; // kCGEventTapOptionDefault: an active tap
pub const KEY_DOWN: u32 = 10;
pub const KEY_UP: u32 = 11;
pub const SYSTEM_DEFINED: u32 = 14; // NSEventTypeSystemDefined
pub const DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const FIELD_AUTOREPEAT: u32 = 8; // kCGKeyboardEventAutorepeat
const FIELD_KEYCODE: u32 = 9; // kCGKeyboardEventKeycode

/// Key down, key up and system-defined. Nothing else ever reaches the callback.
pub const EVENT_MASK: u64 = (1 << KEY_DOWN) | (1 << KEY_UP) | (1 << SYSTEM_DEFINED);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: TapCallback, user: *mut c_void) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CFStringRef;
    static kCFRunLoopDefaultMode: CFStringRef;
    fn CFMachPortCreateRunLoopSource(allocator: *const c_void, port: CFMachPortRef, order: isize) -> CFRunLoopSourceRef;
    fn CFMachPortInvalidate(port: CFMachPortRef);
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source: bool) -> i32;
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRelease(cf: *const c_void);
}

/// Is this process trusted for Accessibility? An active tap needs it, and so does the paste.
pub fn accessibility_allowed() -> bool {
    // SAFETY: no arguments, no failure mode.
    unsafe { AXIsProcessTrusted() }
}

/// What the tap tells the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapEvent {
    /// His key went down: start or finish a dictation.
    Toggle,
    /// macOS disabled the tap; it has already been re-enabled.
    Disabled { by_timeout: bool },
    /// Key capture: he pressed F`n` while the window was waiting for a key. It was swallowed and
    /// started nothing.
    Captured(u8),
}

/// How long a capture waits for a key before the tap goes back to his key alone, so a window
/// that never cancels can never leave F1 captured instead of starting a dictation.
pub const CAPTURE_FOR_MS: u64 = 60_000;

/// Milliseconds since the epoch, for the capture's deadline.
fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct Context {
    /// Cleared by `KeyTap::drop`. The tap thread runs its loop in slices while this is set, so a
    /// stop that lands before the loop has started is still seen: `CFRunLoopStop` on a run loop
    /// that is not running yet is forgotten when it starts (CF keeps the flag per run).
    running: AtomicBool,
    port: AtomicPtr<c_void>,
    key: AtomicU8,
    /// Key capture: until this time (ms since the epoch) the next F-key down is reported as
    /// `Captured` instead of acting. 0 when no capture is waiting.
    capture_until: AtomicU64,
    tx: Sender<TapEvent>,
    longest_ns: AtomicU64,
}

/// One event's numbers, read off a `CGEvent`. `None` for a type the tap does not judge.
///
/// # Safety
/// `event` is the live event macOS handed the callback.
pub unsafe fn read_event(etype: u32, event: CGEventRef) -> Option<KeyEvent> {
    match etype {
        KEY_DOWN | KEY_UP => Some(KeyEvent::Key {
            code: CGEventGetIntegerValueField(event, FIELD_KEYCODE) as u16,
            down: etype == KEY_DOWN,
            repeat: CGEventGetIntegerValueField(event, FIELD_AUTOREPEAT) != 0,
        }),
        // The subtype and data word live only on the NSEvent side.
        SYSTEM_DEFINED => objc2::rc::autoreleasepool(|_| {
            let class = AnyClass::get(c"NSEvent")?;
            let ns: *mut AnyObject = msg_send![class, eventWithCGEvent: event];
            if ns.is_null() {
                return None;
            }
            let subtype: i16 = msg_send![ns, subtype];
            let data1: isize = msg_send![ns, data1];
            Some(KeyEvent::System { subtype, data1: data1 as i64 })
        }),
        _ => None,
    }
}

extern "C" fn callback(_proxy: *mut c_void, etype: u32, event: CGEventRef, user: *mut c_void) -> CGEventRef {
    let started = std::time::Instant::now();
    // SAFETY: `user` is the `Context` the tap was created with, alive until after the run loop
    // that delivers this callback has stopped (KeyTap::drop).
    let ctx = unsafe { &*(user as *const Context) };
    let swallow = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if etype == DISABLED_BY_TIMEOUT || etype == DISABLED_BY_USER_INPUT {
            let port = ctx.port.load(Ordering::Acquire);
            if !port.is_null() {
                // SAFETY: our own live port.
                unsafe { CGEventTapEnable(port, true) };
            }
            ctx.tx.send(TapEvent::Disabled { by_timeout: etype == DISABLED_BY_TIMEOUT }).ok();
            return false;
        }
        // SAFETY: the live event of this callback.
        let Some(read) = (unsafe { read_event(etype, event) }) else { return false };
        // Key capture first: the F-key he presses is the answer, never a dictation.
        let until = ctx.capture_until.load(Ordering::Relaxed);
        if until != 0 {
            if let Some(n) = captured_key(&read).filter(|_| now_ms() < until) {
                ctx.capture_until.store(0, Ordering::Relaxed);
                ctx.tx.send(TapEvent::Captured(n)).ok();
                return true;
            }
        }
        match judge_key(ctx.key.load(Ordering::Relaxed), &read) {
            KeyVerdict::Toggle => {
                ctx.tx.send(TapEvent::Toggle).ok();
                true
            }
            KeyVerdict::Swallow => true,
            KeyVerdict::Pass => false,
        }
    }))
    .unwrap_or(false);
    ctx.longest_ns.fetch_max(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    if swallow { std::ptr::null_mut() } else { event }
}

/// The tap, running. Dropping it stops the run loop, removes the tap and ends the activity.
pub struct KeyTap {
    ctx: Arc<Context>,
    run_loop: usize,
    thread: Option<std::thread::JoinHandle<()>>,
    _activity: Activity,
}

impl KeyTap {
    /// Create the tap for F`key` and start its thread. `Err` when macOS refuses the tap, which
    /// is what it does without Accessibility.
    pub fn start(key: u8, tx: Sender<TapEvent>) -> Result<KeyTap, String> {
        let ctx = Arc::new(Context {
            running: AtomicBool::new(true),
            port: AtomicPtr::new(std::ptr::null_mut()),
            key: AtomicU8::new(key),
            capture_until: AtomicU64::new(0),
            tx,
            longest_ns: AtomicU64::new(0),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<usize, String>>();
        let thread_ctx = ctx.clone();
        let thread = std::thread::Builder::new()
            .name("dictation-keytap".into())
            .spawn(move || {
                let user = Arc::as_ptr(&thread_ctx) as *mut c_void;
                // SAFETY: plain C calls on objects this thread creates and releases; `user`
                // outlives the run loop because `thread_ctx` is held until this closure returns.
                unsafe {
                    let port = CGEventTapCreate(SESSION_EVENT_TAP, HEAD_INSERT, OPTION_DEFAULT, EVENT_MASK, callback, user);
                    if port.is_null() {
                        ready_tx.send(Err("macOS refused the key tap (Accessibility is not allowed)".into())).ok();
                        return;
                    }
                    thread_ctx.port.store(port, Ordering::Release);
                    let source = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
                    let rl = CFRunLoopGetCurrent();
                    CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                    CGEventTapEnable(port, true);
                    ready_tx.send(Ok(rl as usize)).ok();
                    while thread_ctx.running.load(Ordering::Acquire) {
                        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.25, false);
                    }
                    CGEventTapEnable(port, false);
                    thread_ctx.port.store(std::ptr::null_mut(), Ordering::Release);
                    CFMachPortInvalidate(port);
                    CFRelease(source);
                    CFRelease(port);
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv() {
            Ok(Ok(run_loop)) => Ok(KeyTap { ctx, run_loop, thread: Some(thread), _activity: Activity::begin() }),
            Ok(Err(why)) => {
                thread.join().ok();
                Err(why)
            }
            Err(_) => Err("the key tap thread ended before it started".into()),
        }
    }

    /// He picked another key: the next event is judged against it.
    pub fn set_key(&self, key: u8) {
        self.ctx.key.store(key, Ordering::Relaxed);
    }

    /// Key capture: report the next F-key down as `Captured` (for [`CAPTURE_FOR_MS`] at most),
    /// or stop waiting.
    pub fn capture_next(&self, on: bool) {
        self.ctx.capture_until.store(if on { now_ms() + CAPTURE_FOR_MS } else { 0 }, Ordering::Relaxed);
    }

    /// The longest callback run so far, in microseconds, and reset to zero: the hourly number.
    pub fn take_longest_micros(&self) -> u64 {
        self.ctx.longest_ns.swap(0, Ordering::Relaxed) / 1000
    }
}

impl Drop for KeyTap {
    fn drop(&mut self) {
        self.ctx.running.store(false, Ordering::Release);
        // SAFETY: the run loop of our own thread; stopping it only ends the current slice early.
        unsafe { CFRunLoopStop(self.run_loop as CFRunLoopRef) };
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

/// An `NSProcessInfo` activity, held for the life of the tap.
pub struct Activity(Option<Retained<AnyObject>>);

/// `NSActivityUserInitiatedAllowingIdleSystemSleep` (0x00EFFFFF) | `NSActivityLatencyCritical`
/// (0xFF00000000), from `NSProcessInfo.h`.
pub const ACTIVITY_OPTIONS: u64 = 0x00EF_FFFF | 0xFF_0000_0000;

impl Activity {
    pub fn begin() -> Activity {
        // SAFETY: Foundation class methods with their documented argument types.
        let token = objc2::rc::autoreleasepool(|_| unsafe {
            let info_class = AnyClass::get(c"NSProcessInfo")?;
            let string_class = AnyClass::get(c"NSString")?;
            let info: *mut AnyObject = msg_send![info_class, processInfo];
            let reason: *mut AnyObject =
                msg_send![string_class, stringWithUTF8String: c"RichScribe listens for its key".as_ptr()];
            if info.is_null() || reason.is_null() {
                return None;
            }
            let token: Option<Retained<AnyObject>> = msg_send![info, beginActivityWithOptions: ACTIVITY_OPTIONS, reason: reason];
            token
        });
        if token.is_none() {
            super::log::line("could not hold off App Nap for the key tap");
        }
        Activity(token)
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        if let Some(token) = self.0.take() {
            // SAFETY: the token this process began, ended once.
            unsafe {
                if let Some(class) = AnyClass::get(c"NSProcessInfo") {
                    let info: *mut AnyObject = msg_send![class, processInfo];
                    if !info.is_null() {
                        let _: () = msg_send![info, endActivity: &*token];
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT (M9): the mask is exactly key down, key up and system-defined.
    #[test]
    fn the_mask_is_three_event_types_and_nothing_else() {
        assert_eq!(EVENT_MASK, 0x4C00);
        assert_eq!(EVENT_MASK.count_ones(), 3);
        for t in [KEY_DOWN, KEY_UP, SYSTEM_DEFINED] {
            assert_ne!(EVENT_MASK & (1 << t), 0);
        }
        // Mouse, scroll and modifier changes never reach the callback.
        for t in [1u32, 2, 5, 12, 22] {
            assert_eq!(EVENT_MASK & (1 << t), 0, "type {t}");
        }
    }

    /// INVARIANT: the activity is really begun and ended, with the message types Foundation
    /// declares (a debug build checks every encoding), in a process with no window.
    #[test]
    fn the_activity_against_app_nap_begins_and_ends() {
        let activity = Activity::begin();
        assert!(activity.0.is_some(), "NSProcessInfo gave no activity token");
        drop(activity);
    }

    /// INVARIANT: the activity options keep idle system sleep allowed and add latency-critical.
    #[test]
    fn the_activity_allows_idle_system_sleep() {
        const IDLE_SYSTEM_SLEEP_DISABLED: u64 = 1 << 20;
        const LATENCY_CRITICAL: u64 = 0xFF_0000_0000;
        assert_eq!(ACTIVITY_OPTIONS & IDLE_SYSTEM_SLEEP_DISABLED, 0);
        assert_eq!(ACTIVITY_OPTIONS & LATENCY_CRITICAL, LATENCY_CRITICAL);
        assert_eq!(ACTIVITY_OPTIONS & 0x00FF_FFFF, 0x00EF_FFFF, "user-initiated, allowing idle system sleep");
    }
}
