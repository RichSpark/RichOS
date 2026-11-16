//! The gallery's Save Image action renders one share card from the authored design.
use crate::dictation::appkit::{NSPoint, NSRect, NSSize};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use std::ffi::{c_void, CStr, CString};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

type Id = *mut AnyObject;
const CARD_BOUND: Duration = Duration::from_secs(10);
const CARD_SIZE: (f64, f64) = (1600.0, 900.0);
/// `CGImageRef`, encoded as AppKit declares it, so objc2's debug check of a message's signature
/// passes (the trials' type).
#[repr(C)]
pub struct CGImage {
    _private: [u8; 0],
}
// SAFETY: an opaque Core Graphics type, referred to only by pointer.
unsafe impl objc2::encode::RefEncode for CGImage {
    const ENCODING_REF: objc2::encode::Encoding =
        objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct("CGImage", &[]));
}
/// An `NSRect *` argument.
#[repr(transparent)]
struct RectRef(NSRect);
// SAFETY: a pointer to a CGRect, as AppKit declares the argument.
unsafe impl objc2::encode::RefEncode for RectRef {
    const ENCODING_REF: objc2::encode::Encoding =
        objc2::encode::Encoding::Pointer(&<NSRect as objc2::encode::Encode>::ENCODING);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGImageRetain(image: *mut c_void) -> *mut c_void;
    fn CGImageRelease(image: *mut c_void);
}
#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CGImageDestinationCreateWithURL(url: *const c_void, kind: *const c_void, count: usize, options: *const c_void) -> *mut c_void;
    fn CGImageDestinationAddImage(dest: *mut c_void, image: *mut c_void, properties: *const c_void);
    fn CGImageDestinationFinalize(dest: *mut c_void) -> bool;
    static kCGImageDestinationLossyCompressionQuality: *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

fn class(name: &CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

unsafe fn nsstring(s: &str) -> Id {
    let c = CString::new(s).unwrap_or_default();
    match class(c"NSString") {
        Some(cls) => msg_send![cls, stringWithUTF8String: c.as_ptr()],
        None => std::ptr::null_mut(),
    }
}

fn file_url(path: &Path) -> Option<core_foundation::url::CFURL> {
    core_foundation::url::CFURL::from_path(path, false)
}

fn write_jpeg(image: *mut c_void, path: &Path) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    let Some(url) = file_url(path) else { return false };
    // SAFETY: ImageIO's documented destination calls on a file URL; released after use.
    unsafe {
        let kind = CFString::from_static_string("public.jpeg");
        let destination = CGImageDestinationCreateWithURL(url.as_concrete_TypeRef() as _, kind.as_concrete_TypeRef() as _, 1, std::ptr::null());
        if destination.is_null() {
            return false;
        }
        let key = CFString::wrap_under_get_rule(kCGImageDestinationLossyCompressionQuality as _);
        let properties = CFDictionary::from_CFType_pairs(&[(key, CFNumber::from(0.92))]);
        CGImageDestinationAddImage(destination, image, properties.as_concrete_TypeRef() as _);
        let ok = CGImageDestinationFinalize(destination);
        CFRelease(destination);
        ok
    }
}
/// Run `script` in `window`'s page; its value as a string (the script makes it one).
fn eval(window: &tauri::WebviewWindow, script: &str, until: Instant) -> Result<String, String> {
    let (tx, rx) = mpsc::channel::<String>();
    let script = script.to_string();
    window
        .with_webview(move |wv| unsafe {
            let view = wv.inner() as Id;
            let tx = Mutex::new(Some(tx));
            let block = block2::RcBlock::new(move |result: Id, _error: Id| {
                let text = match (result.is_null(), class(c"NSString")) {
                    (false, Some(string_class)) => {
                        let is_string: Bool = msg_send![result, isKindOfClass: string_class];
                        let s: Id = if is_string.as_bool() { result } else { msg_send![result, description] };
                        let p: *const std::ffi::c_char = msg_send![s, UTF8String];
                        if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
                    }
                    _ => String::new(),
                };
                if let Some(tx) = tx.lock().unwrap_or_else(|p| p.into_inner()).take() {
                    tx.send(text).ok();
                }
            });
            // SAFETY: the documented WKWebView method, on the main thread; WebKit copies the block.
            let _: () = msg_send![view, evaluateJavaScript: nsstring(&script), completionHandler: &*block];
        })
        .map_err(|e| e.to_string())?;
    rx.recv_timeout(until.saturating_duration_since(Instant::now())).map_err(|_| "the hidden page did not answer in time".to_string())
}

type Answer = Arc<Mutex<Option<mpsc::Sender<Result<usize, String>>>>>;

fn answer(tx: &Answer, r: Result<usize, String>) {
    if let Some(tx) = tx.lock().unwrap_or_else(|p| p.into_inner()).take() {
        tx.send(r).ok();
    }
}

/// Hold the splash at `ms`, then WebKit's picture of `rect` (points, from the page's top left):
/// one trip to the main thread. The CGImage comes back retained (+1).
fn hold_and_draw(window: &tauri::WebviewWindow, ms: f64, rect: NSRect, until: Instant) -> Result<usize, String> {
    let (tx, rx) = mpsc::channel::<Result<usize, String>>();
    let script = format!("String(window.RichSplash.holdAt({ms}))");
    window
        .with_webview(move |wv| unsafe {
            let view = wv.inner() as Id;
            let tx: Answer = Arc::new(Mutex::new(Some(tx)));
            let held = block2::RcBlock::new(move |_result: Id, error: Id| {
                if !error.is_null() {
                    answer(&tx, Err("the hold threw in the hidden page".into()));
                    return;
                }
                let Some(config_class) = class(c"WKSnapshotConfiguration") else {
                    answer(&tx, Err("no WKSnapshotConfiguration".into()));
                    return;
                };
                let config: Id = msg_send![config_class, new];
                let _: () = msg_send![config, setRect: rect];
                let _: () = msg_send![config, setAfterScreenUpdates: Bool::YES];
                let tx2 = Arc::clone(&tx);
                let drawn = block2::RcBlock::new(move |image: Id, _error: Id| {
                    if image.is_null() {
                        answer(&tx2, Err("WebKit drew no picture".into()));
                        return;
                    }
                    let cg: *mut CGImage = msg_send![image, CGImageForProposedRect: std::ptr::null_mut::<RectRef>(),
                        context: std::ptr::null_mut::<AnyObject>(), hints: std::ptr::null_mut::<AnyObject>()];
                    if cg.is_null() {
                        answer(&tx2, Err("the picture had no bitmap".into()));
                        return;
                    }
                    answer(&tx2, Ok(CGImageRetain(cg.cast()) as usize));
                });
                // SAFETY: the documented WKWebView method, on the main thread.
                let _: () = msg_send![view, takeSnapshotWithConfiguration: config, completionHandler: &*drawn];
                let _: () = msg_send![config, release];
            });
            // SAFETY: as in `eval`.
            let _: () = msg_send![view, evaluateJavaScript: nsstring(&script), completionHandler: &*held];
        })
        .map_err(|e| e.to_string())?;
    rx.recv_timeout(until.saturating_duration_since(Instant::now())).map_err(|_| "out of time".to_string())?
}

/// "SAVE IMAGE" (round 74's gallery): one picture of an opening's finished frame, with the line
/// that names it and says when it became his, written to `out` as a JPEG. A hidden page
/// draws at the share card's size, held at the opening's last moment
/// (`splash.js`, `card`); `page` is the openings' record, so the picture carries his own date
/// and company line. Called off the main thread; bounded.
pub fn save_card(app: &tauri::AppHandle, entry_id: &str, page: &str, out: &Path) -> Result<(), String> {
    static NUMBER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let deadline = Instant::now() + CARD_BOUND;
    let script = format!(
        "window.__RICHOS_LAUNCH__ = Object.freeze({{ kind: \"fresh\", ordinal: null, draw: true, card: true, \
         splash: Object.freeze({{ id: {} }}) }});\n{page}",
        serde_json::to_string(entry_id).unwrap_or_default()
    );
    let label = format!("opening-card-{}", NUMBER.fetch_add(1, Ordering::SeqCst));
    let (tx, rx) = mpsc::channel();
    let builder_app = app.clone();
    app.run_on_main_thread(move || {
        let built = tauri::WebviewWindowBuilder::new(&builder_app, label, tauri::WebviewUrl::App("splash-opening.html".into()))
            .inner_size(CARD_SIZE.0, CARD_SIZE.1)
            // No title bar: macOS keeps a titled window inside the screen, so on a display
            // smaller than the card the page came out smaller than asked (test VM, 2026-11-15:
            // 1600 x 872 on a 1680 x 1050 screen). A window with no frame is never moved or
            // shrunk to fit, so the picture is 1600 x 900 on every Mac.
            .decorations(false)
            .visible(false)
            .focused(false)
            .initialization_script(script)
            .build();
        tx.send(built.map_err(|e| e.to_string())).ok();
    })
    .map_err(|e| e.to_string())?;
    let window = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "the hidden page was not made in time".to_string())??;
    let result = (|| {
        // Ready: the opening asked for is built, held at its finished frame, and its faces loaded.
        let (size, moment): ([f64; 2], f64) = loop {
            let said = eval(
                &window,
                "(window.RichSplash && window.RichSplash.state.card === true && \
                 (!document.fonts || document.fonts.status === 'loaded')) ? \
                 JSON.stringify([innerWidth, innerHeight, window.RichSplash.state.heldAt, window.RichSplash.state.variationId]) : ''",
                deadline,
            )?;
            if !said.is_empty() {
                let (w, h, at, id): (f64, f64, f64, String) = serde_json::from_str(&said).map_err(|e| format!("the hidden page's answer: {e}"))?;
                if id != entry_id {
                    return Err(format!("the hidden page drew {id}, not {entry_id}"));
                }
                break ([w, h], at);
            }
            if Instant::now() >= deadline {
                return Err("the hidden page's opening was not ready in time".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let whole = NSRect { origin: NSPoint::default(), size: NSSize { width: size[0], height: size[1] } };
        let image = hold_and_draw(&window, moment, whole, deadline)?;
        let written = write_jpeg(image as *mut c_void, out);
        // SAFETY: a retained CGImage from `hold_and_draw`, released here.
        unsafe { CGImageRelease(image as *mut c_void) };
        if written { Ok(()) } else { Err(format!("the picture could not be written to {}", out.display())) }
    })();
    window.destroy().ok();
    result
}
