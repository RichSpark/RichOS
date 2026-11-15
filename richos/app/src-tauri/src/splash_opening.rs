//! THE SPLASH'S OPENING: DRAWN AT QUIT, PLAYED AT THE CLICK — Sage's plan `richos-hq/docs/plans/
//! 2026-11-14-existing-splash-under-half-a-second.md`, sections 1 and 2.
//!
//! The CEO, 2026-11-14: "THE JOB IS TO START SHOWING OUR EXISTING SPLASH SCREEN WHICH LASTS FOR AT
//! LEAST 3 SECONDS BEFORE IT SWITCHES TO THE HOME SCREEN" "IN WELL UNDER 500 MS AFTER BUTTON
//! CLICK." "380 ms as the maximum allowed p95 time for the RichOS app to start showing the splash
//! screen."
//!
//! A web page cannot be playing 380 ms after a click: macOS has to start the web engine first. So
//! the opening of the splash the rules picked (`splash_pick.rs`) is made before the click and the
//! click only plays it:
//!
//! * **At quit** ([`draw_before_quit`]), when the pick or the window changed, a web view nobody
//!   sees opens `ui/splash-opening.html` at the size of the window the next start will open, holds
//!   the picked splash at its first moment and then at every thirtieth of a second of its first
//!   second (`RichSplash.holdAt`, the splash's own code), and takes WebKit's picture each time. The
//!   first picture is the whole page, stored as raw pixels so a start never decodes it (trial 1:
//!   decoding a PNG of it took 7 to 13 ms, 32 to 37 at Retina size); the later ones are only the
//!   part that changes, as JPEG (trial 3: PNG skipped pictures at Retina size, JPEG played every
//!   one on time). At most [`DRAW_BOUND`], with no window on screen; the moments are drawn coarse
//!   first, so a drawing that runs out of time still covers the second at fewer pictures a second
//!   (trial 2 measured 4.03 s for 30 pictures drawn in order, over the plan's 3 s).
//! * **At the start** ([`show`], [`started`]), the window is built with the first picture in a
//!   layer above the web view that takes no mouse or keyboard input, pushed to the glass at once
//!   (`first_frame::commit`), and the later pictures are handed to Core Animation from other
//!   threads, each at its moment, so the splash plays while the app's main thread is busy (trial
//!   3). The page joins at the moment the pictures have reached (`splash_take_over`) and the layer
//!   is taken down in that step ([`remove`]), or at the ceiling.
use crate::dictation::appkit::{NSPoint, NSRect, NSSize};
use crate::splash_pick::{self, Frame, MadeFor, Manifest, Opening, Saved};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::sel;
use std::ffi::{c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager;

type Id = *mut AnyObject;

/// The quit's drawing gets at most this long (plan section 1, "Bounded").
pub const DRAW_BOUND: Duration = Duration::from_secs(3);
/// A saved image gets at most this long, and is drawn at the share card's size (round 74: 1600 x 900).
const CARD_BOUND: Duration = Duration::from_secs(10);
const CARD_SIZE: (f64, f64) = (1600.0, 900.0);
/// The later pictures are read on this many threads, in the order they are needed, and handed to
/// Core Animation in runs of this many as they arrive (trial 3's packing).
const READERS: usize = 4;
const RUN: usize = 3;
/// JPEG quality of the later pictures (trial 3's 0.92; its faithfulness is measured in the
/// verification record of this build).
const JPEG_QUALITY: f64 = 0.92;
/// The first picture's file starts with this, then width, height and row length (u32, LE).
const RAW_MAGIC: &[u8; 4] = b"RSO1";

// -------------------------------------------------------------------------------------------
// Core Graphics, ImageIO, Core Animation, Core Foundation
// -------------------------------------------------------------------------------------------

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
    fn CGImageGetWidth(image: *mut c_void) -> usize;
    fn CGImageGetHeight(image: *mut c_void) -> usize;
    fn CGImageRetain(image: *mut c_void) -> *mut c_void;
    fn CGImageRelease(image: *mut c_void);
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
        provider: *mut c_void,
        decode: *const f64,
        should_interpolate: bool,
        intent: i32,
    ) -> *mut c_void;
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    static kCGColorSpaceSRGB: *const c_void;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(context: *mut c_void, rect: NSRect, image: *mut c_void);
    fn CGContextRelease(context: *mut c_void);
    fn CGDataProviderCreateWithData(
        info: *mut c_void,
        data: *const c_void,
        size: usize,
        release: Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize)>,
    ) -> *mut c_void;
    fn CGDataProviderRelease(provider: *mut c_void);
}
#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CGImageSourceCreateWithURL(url: *const c_void, options: *const c_void) -> *mut c_void;
    fn CGImageSourceCreateImageAtIndex(source: *mut c_void, index: usize, options: *const c_void) -> *mut c_void;
    fn CGImageDestinationCreateWithURL(url: *const c_void, kind: *const c_void, count: usize, options: *const c_void) -> *mut c_void;
    fn CGImageDestinationAddImage(destination: *mut c_void, image: *mut c_void, properties: *const c_void);
    fn CGImageDestinationFinalize(destination: *mut c_void) -> bool;
    static kCGImageSourceShouldCacheImmediately: *const c_void;
    static kCGImageDestinationLossyCompressionQuality: *const c_void;
}
#[link(name = "QuartzCore", kind = "framework")]
extern "C" {
    fn CACurrentMediaTime() -> f64;
    static kCAAnimationDiscrete: Id;
    static kCAFillModeForwards: Id;
    static kCAGravityResize: Id;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

/// `kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little`: the native BGRA layout, which
/// Core Animation takes with no conversion.
const BGRA: u32 = 2 | (2 << 12);

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

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// macOS's reduced-motion setting, which the opening is drawn under and checked against.
pub fn reduce_motion() -> bool {
    objc2::rc::autoreleasepool(|_| {
        let Some(workspace) = class(c"NSWorkspace") else { return false };
        // SAFETY: documented NSWorkspace class method and BOOL property; any thread.
        unsafe {
            let shared: Id = msg_send![workspace, sharedWorkspace];
            if shared.is_null() {
                return false;
            }
            let on: Bool = msg_send![shared, accessibilityDisplayShouldReduceMotion];
            on.as_bool()
        }
    })
}

// -------------------------------------------------------------------------------------------
// The first picture: raw pixels, no decoding
// -------------------------------------------------------------------------------------------

/// `image` as sRGB BGRA, premultiplied, behind a 16-byte header.
unsafe fn raw_picture(image: *mut c_void) -> Option<(Vec<u8>, [u32; 3])> {
    let (w, h) = (CGImageGetWidth(image), CGImageGetHeight(image));
    if w == 0 || h == 0 {
        return None;
    }
    let row = w * 4;
    let mut out = vec![0u8; 16 + row * h];
    let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    let context = CGBitmapContextCreate(out[16..].as_mut_ptr().cast(), w, h, 8, row, space, BGRA);
    CGColorSpaceRelease(space);
    if context.is_null() {
        return None;
    }
    let rect = NSRect { origin: NSPoint { x: 0.0, y: 0.0 }, size: NSSize { width: w as f64, height: h as f64 } };
    CGContextDrawImage(context, rect, image);
    CGContextRelease(context);
    out[..4].copy_from_slice(RAW_MAGIC);
    out[4..8].copy_from_slice(&(w as u32).to_le_bytes());
    out[8..12].copy_from_slice(&(h as u32).to_le_bytes());
    out[12..16].copy_from_slice(&(row as u32).to_le_bytes());
    Some((out, [w as u32, h as u32, row as u32]))
}

unsafe extern "C" fn free_vec(info: *mut c_void, _data: *const c_void, _size: usize) {
    drop(Box::from_raw(info as *mut Vec<u8>));
}

/// The first picture as a CGImage over its own bytes (+1): no decoding, no copy.
fn read_raw(path: &Path, expect: [u32; 3]) -> Result<*mut c_void, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("its first picture is unreadable ({e})"))?;
    if bytes.len() < 16 || &bytes[..4] != RAW_MAGIC {
        return Err("its first picture is not one this build wrote".into());
    }
    let field = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (w, h, row) = (field(4), field(8), field(12));
    if [w, h, row] != expect || bytes.len() != 16 + (row as usize) * (h as usize) {
        return Err("its first picture does not match its manifest".into());
    }
    let boxed = Box::new(bytes);
    let data = boxed[16..].as_ptr();
    let size = boxed.len() - 16;
    // SAFETY: the provider owns the box and frees it through `free_vec`; the image keeps the
    // provider; sRGB is the color space the pixels were drawn in (`raw_picture`).
    unsafe {
        let info = Box::into_raw(boxed) as *mut c_void;
        let provider = CGDataProviderCreateWithData(info, data.cast(), size, Some(free_vec));
        if provider.is_null() {
            drop(Box::from_raw(info as *mut Vec<u8>));
            return Err("no data provider for the first picture".into());
        }
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        let image = CGImageCreate(w as usize, h as usize, 8, 32, row as usize, space, BGRA, provider, std::ptr::null(), false, 0);
        CGColorSpaceRelease(space);
        CGDataProviderRelease(provider);
        if image.is_null() {
            return Err("the first picture would not make an image".into());
        }
        Ok(image)
    }
}

fn file_url(path: &Path) -> Option<core_foundation::url::CFURL> {
    core_foundation::url::CFURL::from_path(path, false)
}

/// A later picture, decoded now (+1).
fn read_jpeg(path: &Path) -> Option<*mut c_void> {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    let url = file_url(path)?;
    // SAFETY: ImageIO on a file URL; the source is released, the image returned at +1.
    unsafe {
        let key = CFString::wrap_under_get_rule(kCGImageSourceShouldCacheImmediately as _);
        let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        let source = CGImageSourceCreateWithURL(url.as_concrete_TypeRef() as _, std::ptr::null());
        if source.is_null() {
            return None;
        }
        let image = CGImageSourceCreateImageAtIndex(source, 0, options.as_concrete_TypeRef() as _);
        CFRelease(source);
        (!image.is_null()).then_some(image)
    }
}

/// Write `image` as JPEG at [`JPEG_QUALITY`].
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
        let properties = CFDictionary::from_CFType_pairs(&[(key, CFNumber::from(JPEG_QUALITY))]);
        CGImageDestinationAddImage(destination, image, properties.as_concrete_TypeRef() as _);
        let ok = CGImageDestinationFinalize(destination);
        CFRelease(destination);
        ok
    }
}

// -------------------------------------------------------------------------------------------
// AT THE START: show, play, take down
// -------------------------------------------------------------------------------------------

/// A view that takes no mouse or keyboard input: the opening's pictures sit over the page and
/// a click goes straight through to it.
fn pass_through_class() -> Option<&'static AnyClass> {
    static MADE: OnceLock<Option<usize>> = OnceLock::new();
    let made = MADE.get_or_init(|| {
        let superclass = class(c"NSView")?;
        let mut builder = ClassBuilder::new(c"RichOSSplashOpeningView", superclass)?;
        extern "C-unwind" fn hit_test(_this: *mut AnyObject, _sel: Sel, _point: NSPoint) -> *mut AnyObject {
            std::ptr::null_mut()
        }
        extern "C-unwind" fn no(_this: *mut AnyObject, _sel: Sel) -> Bool {
            Bool::NO
        }
        // SAFETY: `hitTest:` takes an NSPoint and returns an NSView (nil: never hit), and
        // `acceptsFirstResponder` returns BOOL, as NSView declares them.
        unsafe {
            builder.add_method(sel!(hitTest:), hit_test as extern "C-unwind" fn(_, _, _) -> _);
            builder.add_method(sel!(acceptsFirstResponder), no as extern "C-unwind" fn(_, _) -> _);
        }
        Some(builder.register() as *const AnyClass as usize)
    });
    // SAFETY: a registered class lives for the process.
    made.map(|p| unsafe { &*(p as *const AnyClass) })
}

/// The WKWebView in `view`'s tree, if any.
unsafe fn find_webview(view: Id, depth: u32) -> Option<Id> {
    if view.is_null() || depth > 4 {
        return None;
    }
    let wk = class(c"WKWebView")?;
    let is: Bool = msg_send![view, isKindOfClass: wk];
    if is.as_bool() {
        return Some(view);
    }
    let subviews: Id = msg_send![view, subviews];
    let count: usize = msg_send![subviews, count];
    for i in 0..count {
        let child: Id = msg_send![subviews, objectAtIndex: i];
        if let Some(found) = find_webview(child, depth + 1) {
            return Some(found);
        }
    }
    None
}

struct Showing {
    view: usize,
    root: usize,
    region: usize,
    manifest: Manifest,
    dir: PathBuf,
    first_media: f64,
    first_epoch_ms: u64,
    taken_down: bool,
}

static SHOWING: Mutex<Option<Showing>> = Mutex::new(None);

/// AFTER `build()`, BEFORE THE PUSH (main thread): the first picture in a layer above the page.
pub fn show(window: &tauri::WebviewWindow, opening: &Opening) -> Result<(), String> {
    let m = &opening.manifest;
    let image = read_raw(&opening.dir.join(splash_pick::FIRST_PICTURE), m.first)?;
    let (Ok(ns_window), Some(view_class), Some(layer_class)) = (window.ns_window(), pass_through_class(), class(c"CALayer")) else {
        // SAFETY: the image this function created.
        unsafe { CGImageRelease(image) };
        return Err("no window, view class or layer class to show it in".into());
    };
    let ns_window = ns_window as Id;
    // SAFETY: AppKit and Core Animation on the main thread (the setup hook), on this hook's own
    // window; every object is checked before use, and the image is handed to the layer, which
    // retains it, before this function's reference is released.
    unsafe {
        let content: Id = msg_send![ns_window, contentView];
        if content.is_null() {
            CGImageRelease(image);
            return Err("the window has no content view".into());
        }
        let scale: f64 = msg_send![ns_window, backingScaleFactor];
        // Over the web view exactly, as its sibling, or else the page-high bottom of the content.
        let (parent, frame, above) = match find_webview(content, 0) {
            Some(web) => {
                let parent: Id = msg_send![web, superview];
                let frame: NSRect = msg_send![web, frame];
                (parent, frame, web)
            }
            None => {
                let mut bounds: NSRect = msg_send![content, bounds];
                if m.page[1] > 0.0 && m.page[1] <= bounds.size.height {
                    bounds.size.height = m.page[1];
                }
                (content, bounds, std::ptr::null_mut())
            }
        };
        let root: Id = msg_send![layer_class, layer];
        let _: () = msg_send![root, setFrame: NSRect { origin: NSPoint::default(), size: frame.size }];
        let _: () = msg_send![root, setContentsGravity: kCAGravityResize];
        let _: () = msg_send![root, setContentsScale: scale];
        let _: () = msg_send![root, setContents: image as Id];
        CGImageRelease(image);
        // The later pictures' layer: the changing part, placed in the root's bottom-up space
        // (AppKit resets `geometryFlipped` on a layer-hosting view's layer; trial 3).
        let region: Id = msg_send![layer_class, layer];
        let [x, y, w, h] = m.region;
        let region_frame = NSRect { origin: NSPoint { x, y: frame.size.height - y - h }, size: NSSize { width: w, height: h } };
        let _: () = msg_send![region, setFrame: region_frame];
        let _: () = msg_send![region, setContentsGravity: kCAGravityResize];
        let _: () = msg_send![region, setContentsScale: scale];
        let _: () = msg_send![root, addSublayer: region];
        // Kept for the life of the process: the player thread may still hand it pictures after
        // the opening has come down.
        let _: Id = msg_send![region, retain];
        let view: Id = msg_send![view_class, alloc];
        let view: Id = msg_send![view, initWithFrame: frame];
        let _: () = msg_send![view, setLayer: root];
        let _: () = msg_send![view, setWantsLayer: Bool::YES];
        // Width and height follow the page (NSViewWidthSizable | NSViewHeightSizable).
        let _: () = msg_send![view, setAutoresizingMask: 18usize];
        // NSWindowAbove = 1: over the web view (or over everything in the content view).
        let _: () = msg_send![parent, addSubview: view, positioned: 1isize, relativeTo: above];
        *SHOWING.lock().unwrap_or_else(|p| p.into_inner()) = Some(Showing {
            view: view as usize,
            root: root as usize,
            region: region as usize,
            manifest: m.clone(),
            dir: opening.dir.clone(),
            first_media: 0.0,
            first_epoch_ms: 0,
            taken_down: false,
        });
    }
    Ok(())
}

/// THE FIRST PICTURE IS ON THE GLASS (right after the push): note the moment, on Core
/// Animation's clock and the wall clock, and start handing the later pictures over, each at its
/// moment from now. Returns how many later pictures there are.
pub fn started() -> Option<usize> {
    let (frames, dir, region, first_media) = {
        let mut held = SHOWING.lock().unwrap_or_else(|p| p.into_inner());
        let s = held.as_mut()?;
        // SAFETY: Core Animation's clock, any thread.
        s.first_media = unsafe { CACurrentMediaTime() };
        s.first_epoch_ms = now_epoch_ms();
        (s.manifest.frames.clone(), s.dir.clone(), s.region, s.first_media)
    };
    let count = frames.len();
    if count > 0 {
        let spawned = std::thread::Builder::new()
            .name("richos-splash-opening".into())
            .spawn(move || play(frames, dir, region as Id, first_media));
        if let Err(e) = spawned {
            eprintln!("[richos] splash: the opening's later pictures could not start ({e}); its first picture holds");
        }
    }
    Some(count)
}

/// The later pictures, read on [`READERS`] threads in the order they play, handed to Core
/// Animation in runs of [`RUN`] as they arrive: each run a discrete `contents` animation timed
/// from the first frame, the last one holding forward (trial 3's player, which showed every
/// picture on time).
fn play(frames: Vec<Frame>, dir: PathBuf, region: Id, first_media: f64) {
    let n = frames.len();
    let (tx, rx) = mpsc::channel::<(usize, usize)>();
    let frames = Arc::new(frames);
    for t in 0..READERS.min(n) {
        let tx = tx.clone();
        let dir = dir.clone();
        let frames = Arc::clone(&frames);
        let reader = std::thread::Builder::new().name("richos-splash-reader".into()).spawn(move || {
            let mut i = t;
            while i < n {
                let image = read_jpeg(&dir.join(splash_pick::frame_file(frames[i].k))).map_or(0, |p| p as usize);
                if tx.send((i, image)).is_err() {
                    break;
                }
                i += READERS;
            }
        });
        if let Err(e) = reader {
            // The pictures this reader would have read never arrive: the opening holds the last
            // one handed over, and the page joins there.
            eprintln!("[richos] splash: a reader of the opening's pictures could not start ({e})");
        }
    }
    drop(tx);
    let mut images: Vec<Option<usize>> = vec![None; n];
    let mut next = 0usize;
    let mut late = 0usize;
    let (Some(array), Some(number), Some(keyframes), Some(transaction)) =
        (class(c"NSArray"), class(c"NSNumber"), class(c"CAKeyframeAnimation"), class(c"CATransaction"))
    else {
        return;
    };
    // SAFETY: Core Animation from this thread inside explicit transactions (trial 3), on the
    // region layer `show` made, which lives until the process ends (`remove` only detaches its
    // view); each image is retained by the animation's values before this thread's reference is
    // released.
    unsafe {
        let begin: f64 = msg_send![region, convertTime: first_media, fromLayer: std::ptr::null_mut::<AnyObject>()];
        for (i, image) in rx {
            images[i] = Some(image);
            loop {
                let end = (next + RUN).min(n);
                if next >= n || !(next..end).all(|j| images[j].is_some()) {
                    break;
                }
                let run: Vec<(f64, Id)> = (next..end)
                    .filter_map(|j| {
                        let p = images[j].unwrap_or(0);
                        (p != 0).then_some((frames[j].at_ms / 1000.0, p as Id))
                    })
                    .collect();
                if !run.is_empty() {
                    let start = run[0].0;
                    let stop = if end < n {
                        frames[end].at_ms / 1000.0
                    } else {
                        run[run.len() - 1].0 + 1.0 / splash_pick::OPENING_FPS
                    };
                    let span = (stop - start).max(1e-3);
                    if CACurrentMediaTime() > first_media + start {
                        late += 1;
                    }
                    let _: () = msg_send![transaction, begin];
                    let _: () = msg_send![transaction, setDisableActions: Bool::YES];
                    let values: Vec<Id> = run.iter().map(|(_, p)| *p).collect();
                    let values: Id = msg_send![array, arrayWithObjects: values.as_ptr(), count: values.len()];
                    let mut times: Vec<Id> = run.iter().map(|(at, _)| msg_send![number, numberWithDouble: (at - start) / span]).collect();
                    times.push(msg_send![number, numberWithDouble: 1.0f64]);
                    let times: Id = msg_send![array, arrayWithObjects: times.as_ptr(), count: times.len()];
                    let animation: Id = msg_send![keyframes, animationWithKeyPath: nsstring("contents")];
                    let _: () = msg_send![animation, setValues: values];
                    let _: () = msg_send![animation, setKeyTimes: times];
                    let _: () = msg_send![animation, setCalculationMode: kCAAnimationDiscrete];
                    let _: () = msg_send![animation, setDuration: span];
                    let _: () = msg_send![animation, setBeginTime: begin + start];
                    let _: () = msg_send![animation, setFillMode: kCAFillModeForwards];
                    let _: () = msg_send![animation, setRemovedOnCompletion: Bool::NO];
                    if end == n {
                        // The opening's last picture stays once it has run (a page later than the
                        // opening is long joins at that moment).
                        let _: () = msg_send![region, setContents: run[run.len() - 1].1];
                    }
                    let _: () = msg_send![region, addAnimation: animation, forKey: nsstring(&format!("opening-{next}"))];
                    let _: () = msg_send![transaction, commit];
                    let _: () = msg_send![transaction, flush];
                }
                for p in images[next..end].iter().flatten() {
                    if *p != 0 {
                        CGImageRelease(*p as *mut c_void);
                    }
                }
                next = end;
            }
        }
    }
    if late > 0 {
        eprintln!("[richos] splash: {late} run(s) of the opening's pictures reached Core Animation after their moment");
    }
}

/// What the page needs to join the opening: when its first picture reached the glass (wall
/// clock, ms) and how long it runs. `None` when no opening is up.
pub fn take_over_info() -> Option<(u64, f64)> {
    let held = SHOWING.lock().unwrap_or_else(|p| p.into_inner());
    let s = held.as_ref()?;
    (!s.taken_down && s.first_epoch_ms > 0).then_some((s.first_epoch_ms, s.manifest.length_ms))
}

/// TAKE THE OPENING DOWN: the page has joined it, or the ceiling came. Once; main thread.
pub fn remove() -> bool {
    let (view, root, region) = {
        let mut held = SHOWING.lock().unwrap_or_else(|p| p.into_inner());
        let Some(s) = held.as_mut() else { return false };
        if s.taken_down {
            return false;
        }
        s.taken_down = true;
        (s.view as Id, s.root as Id, s.region as Id)
    };
    // SAFETY: the view and layers `show` made, on the main thread, inside one transaction.
    unsafe {
        if let Some(transaction) = class(c"CATransaction") {
            let _: () = msg_send![transaction, begin];
            let _: () = msg_send![transaction, setDisableActions: Bool::YES];
            let _: () = msg_send![region, removeAllAnimations];
            let _: () = msg_send![region, setContents: std::ptr::null_mut::<AnyObject>()];
            let _: () = msg_send![root, setContents: std::ptr::null_mut::<AnyObject>()];
            let _: () = msg_send![view, removeFromSuperview];
            let _: () = msg_send![transaction, commit];
        }
        let _: () = msg_send![view, release];
    }
    true
}

// -------------------------------------------------------------------------------------------
// AT QUIT: draw the next start's opening
// -------------------------------------------------------------------------------------------

/// Set while a quit is drawing; the second pass through `ExitRequested` (our own exit) goes
/// straight through, and a Dock click in that time calls the quit off.
static DRAWING: AtomicBool = AtomicBool::new(false);
static FINISHING: AtomicBool = AtomicBool::new(false);
static CALLED_OFF: AtomicBool = AtomicBool::new(false);

/// The quit's drawing is over and this is its own exit: let it through.
pub fn quit_is_finishing() -> bool {
    FINISHING.load(Ordering::SeqCst)
}

/// A Dock click while the quit is drawing: he wants the app, so the quit is called off. True
/// when a drawing was called off (the caller brings the window back).
pub fn call_off_quit() -> bool {
    if DRAWING.load(Ordering::SeqCst) {
        CALLED_OFF.store(true, Ordering::SeqCst);
        return true;
    }
    false
}

/// DRAW THE NEXT START'S OPENING, THEN QUIT. Called from the quit's `ExitRequested` once the
/// quit is allowed, with the pick the rules made and what its opening is made for. Hides every
/// window, draws on its own thread for at most [`DRAW_BOUND`], saves the pick (with its opening
/// when the drawing finished) and exits. The caller prevents this exit when this returns true.
pub fn draw_before_quit(app: &tauri::AppHandle, data_dir: PathBuf, saved: Saved, made: MadeFor, position: Option<(f64, f64)>, page: String) -> bool {
    if DRAWING.swap(true, Ordering::SeqCst) {
        return true; // already drawing: a second quit request in the same instant
    }
    CALLED_OFF.store(false, Ordering::SeqCst);
    let mut shown = Vec::new();
    for (label, window) in app.webview_windows() {
        if window.is_visible().unwrap_or(false) {
            window.hide().ok();
            shown.push(label);
        }
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("richos-splash-at-quit".into()).spawn(move || {
        let started = Instant::now();
        let deadline = started + DRAW_BOUND;
        let outcome = draw(&app, &data_dir, &made, position, deadline, &page);
        if CALLED_OFF.load(Ordering::SeqCst) {
            DRAWING.store(false, Ordering::SeqCst);
            // Every window that was on screen comes back (the conversation window is also brought
            // back by the Dock click itself).
            for label in &shown {
                if let Some(window) = app.get_webview_window(label) {
                    window.show().ok();
                }
            }
            eprintln!("[richos] splash at quit: the Dock icon was clicked while the opening was drawn; the quit is called off");
            return;
        }
        let record = match &outcome {
            Ok(_) => Saved { opening: Some(made.clone()), ..saved.clone() },
            Err(_) => Saved { opening: None, ..saved.clone() },
        };
        if let Err(e) = splash_pick::write_saved(&data_dir, &record) {
            eprintln!("[richos] splash at quit: the pick could not be saved ({e}); the next start applies the rules itself");
        }
        match outcome {
            Ok(said) => eprintln!(
                "[richos] splash at quit: the next start shows {} (start {}); its opening was drawn: {said}; {} ms",
                record.id,
                record.start.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
                started.elapsed().as_millis()
            ),
            Err(why) => eprintln!(
                "[richos] splash at quit: the next start shows {}; no opening was saved ({why}), so it shows the splash's \
                 ground until the page draws it; {} ms",
                record.id,
                started.elapsed().as_millis()
            ),
        }
        FINISHING.store(true, Ordering::SeqCst);
        app.exit(0);
    });
    if spawned.is_err() {
        DRAWING.store(false, Ordering::SeqCst);
        return false;
    }
    true
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
/// that names it and says when it became his, written to `out` as a JPEG. The same hidden page
/// the quit draws with, at the share card's size, held at the opening's last moment
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

enum Job {
    First(usize, PathBuf, mpsc::Sender<Option<[u32; 3]>>),
    Later(usize, PathBuf),
}

/// The drawing itself. `Ok(what was drawn)` or `Err(why nothing was saved)`. `page` is what the
/// app's own page is given beside the launch object (the openings' record, `openings::page_record`),
/// so the hidden page draws the splash with exactly what the next start's page will have.
fn draw(app: &tauri::AppHandle, data_dir: &Path, made: &MadeFor, position: Option<(f64, f64)>, deadline: Instant, page: &str) -> Result<String, String> {
    let script = format!(
        "window.__RICHOS_LAUNCH__ = Object.freeze({{ kind: \"fresh\", ordinal: null, draw: true, \
         splash: Object.freeze({{ id: {} }}) }});\n{page}",
        serde_json::to_string(&made.id).unwrap_or_default()
    );
    let (w, h) = (made.page[0], made.page[1]);
    let (tx, rx) = mpsc::channel();
    let builder_app = app.clone();
    app.run_on_main_thread(move || {
        let mut builder =
            tauri::WebviewWindowBuilder::new(&builder_app, "splash-opening", tauri::WebviewUrl::App("splash-opening.html".into()))
                .inner_size(w, h)
                .visible(false)
                .focused(false)
                .initialization_script(script);
        if let Some((x, y)) = position {
            builder = builder.position(x, y);
        }
        tx.send(builder.build().map_err(|e| e.to_string())).ok();
    })
    .map_err(|e| e.to_string())?;
    let window = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "the hidden page was not made in time".to_string())??;
    let result = draw_in(&window, data_dir, made, deadline);
    window.destroy().ok();
    result
}

fn draw_in(window: &tauri::WebviewWindow, data_dir: &Path, made: &MadeFor, deadline: Instant) -> Result<String, String> {
    let opened = Instant::now();
    // Ready: the splash is built and held at its first moment, laid out, and its faces loaded (a
    // picture taken before the line's face arrives would show another face).
    let page: [f64; 3] = loop {
        if CALLED_OFF.load(Ordering::SeqCst) {
            return Err("called off".into());
        }
        let said = eval(
            window,
            "(window.RichSplash && window.RichSplash.state.drawing && window.RichSplash.state.heldAt === 0 && \
             document.body.offsetHeight >= 0 && (!document.fonts || document.fonts.status === 'loaded')) ? \
             JSON.stringify([innerWidth, innerHeight, devicePixelRatio]) : ''",
            deadline,
        )?;
        if !said.is_empty() {
            break serde_json::from_str(&said).map_err(|e| format!("the hidden page's size: {e}"))?;
        }
        if Instant::now() >= deadline {
            return Err("the hidden page's splash was not ready in time".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let ready_ms = opened.elapsed().as_millis();
    if (page[2] - made.scale).abs() > 0.01 {
        return Err(format!("the hidden page drew at scale {} and the window opens at {}", page[2], made.scale));
    }
    let region_text = eval(window, &format!("JSON.stringify(window.RichSplash.openingRegion({}))", splash_pick::OPENING_MS), deadline)?;
    let region: [f64; 4] = serde_json::from_str(&region_text).map_err(|_| format!("no changing part to draw ({region_text})"))?;
    let fresh = data_dir.join(format!("{}.new", splash_pick::OPENING_DIR));
    std::fs::remove_dir_all(&fresh).ok();
    std::fs::create_dir_all(&fresh).map_err(|e| format!("the opening's folder could not be made ({e})"))?;

    // The writers: the raw first picture and the JPEGs, off the driving thread.
    let (jobs, queue) = mpsc::channel::<Job>();
    let queue = Arc::new(Mutex::new(queue));
    let failed = Arc::new(AtomicBool::new(false));
    let mut writers = Vec::new();
    for _ in 0..3 {
        let queue = Arc::clone(&queue);
        let failed = Arc::clone(&failed);
        writers.push(std::thread::spawn(move || loop {
            let job = queue.lock().unwrap_or_else(|p| p.into_inner()).recv();
            let Ok(job) = job else { break };
            match job {
                Job::First(image, path, said) => {
                    // SAFETY: a retained CGImage from `hold_and_draw`, released here.
                    let out = unsafe { raw_picture(image as *mut c_void) };
                    unsafe { CGImageRelease(image as *mut c_void) };
                    let ok = out.and_then(|(bytes, dims)| std::fs::write(&path, bytes).ok().map(|_| dims));
                    if ok.is_none() {
                        failed.store(true, Ordering::SeqCst);
                    }
                    said.send(ok).ok();
                }
                Job::Later(image, path) => {
                    if !write_jpeg(image as *mut c_void, &path) {
                        failed.store(true, Ordering::SeqCst);
                    }
                    // SAFETY: as above.
                    unsafe { CGImageRelease(image as *mut c_void) };
                }
            }
        }));
    }
    let whole = NSRect { origin: NSPoint::default(), size: NSSize { width: page[0], height: page[1] } };
    let first = hold_and_draw(window, 0.0, whole, deadline)?;
    let (said, heard) = mpsc::channel();
    jobs.send(Job::First(first, fresh.join(splash_pick::FIRST_PICTURE), said)).ok();
    let part = NSRect { origin: NSPoint { x: region[0], y: region[1] }, size: NSSize { width: region[2], height: region[3] } };
    // The writers take about one picture's encoding after the last one is drawn.
    let margin = Duration::from_millis(150);
    let mut frames = Vec::new();
    let mut slowest = Duration::from_millis(0);
    for k in splash_pick::drawing_order() {
        if CALLED_OFF.load(Ordering::SeqCst) {
            return Err("called off".into());
        }
        let step = Instant::now();
        if step + slowest + margin > deadline {
            break;
        }
        let image = hold_and_draw(window, splash_pick::moment_ms(k), part, deadline.checked_sub(margin).unwrap_or(deadline))?;
        jobs.send(Job::Later(image, fresh.join(splash_pick::frame_file(k)))).ok();
        frames.push(Frame { k, at_ms: splash_pick::moment_ms(k) });
        slowest = slowest.max(step.elapsed());
    }
    drop(jobs);
    let dims = heard
        .recv_timeout(deadline.saturating_duration_since(Instant::now()) + margin)
        .ok()
        .flatten()
        .ok_or("the first picture could not be written")?;
    for writer in writers {
        writer.join().ok();
    }
    if failed.load(Ordering::SeqCst) {
        return Err("a picture could not be written".into());
    }
    if frames.is_empty() {
        return Err("no time was left for the later pictures".into());
    }
    frames.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));
    let length_ms = frames.last().map_or(0.0, |f| f.at_ms);
    let manifest = Manifest { nonce: made.nonce.clone(), page: [page[0], page[1]], scale: page[2], first: dims, region, frames, length_ms };
    let count = manifest.frames.len();
    std::fs::write(
        fresh.join(splash_pick::MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("the manifest could not be written ({e})"))?;
    // Into place: the old folder goes, the new one takes its fixed name. The record naming this
    // drawing is written after, so a start reads a matching pair or uses no opening.
    let live = data_dir.join(splash_pick::OPENING_DIR);
    let old = data_dir.join(format!("{}.old", splash_pick::OPENING_DIR));
    std::fs::remove_dir_all(&old).ok();
    std::fs::rename(&live, &old).ok();
    std::fs::rename(&fresh, &live).map_err(|e| format!("the opening could not be put in place ({e})"))?;
    std::fs::remove_dir_all(&old).ok();
    Ok(format!(
        "{count} of {} pictures over {length_ms:.0} ms, page {}x{} at scale {}, changing part {:?}, hidden page ready in {ready_ms} ms",
        splash_pick::drawing_order().len(),
        page[0],
        page[1],
        page[2],
        region
    ))
}

/// A fresh name for one drawing.
pub fn nonce() -> String {
    format!("{:x}-{:x}", now_epoch_ms(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("richos-splash-opening-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A picture of `w` x `h` opaque pixels, as the first picture's file holds them.
    fn raw_file(path: &Path, w: u32, h: u32) -> Vec<u8> {
        let mut out = RAW_MAGIC.to_vec();
        for v in [w, h, w * 4] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for y in 0..h {
            for x in 0..w {
                out.extend_from_slice(&[(x * 7 % 256) as u8, (y * 11 % 256) as u8, ((x + y) * 5 % 256) as u8, 255]);
            }
        }
        std::fs::write(path, &out).unwrap();
        out
    }

    #[test]
    fn the_first_picture_is_read_without_decoding_and_written_back_byte_for_byte() {
        let dir = scratch("raw");
        let file = dir.join(splash_pick::FIRST_PICTURE);
        let bytes = raw_file(&file, 40, 30);
        let image = read_raw(&file, [40, 30, 160]).expect("the first picture becomes an image over its own bytes");
        // SAFETY: the image just made, released at the end.
        let (again, dims) = unsafe { raw_picture(image) }.expect("drawn back as raw sRGB BGRA");
        unsafe { CGImageRelease(image) };
        assert_eq!(dims, [40, 30, 160]);
        assert_eq!(again, bytes, "sRGB in, sRGB out: the first picture is the pixels that were drawn");
        assert!(read_raw(&file, [41, 30, 164]).unwrap_err().contains("does not match its manifest"));
        std::fs::write(&file, b"not a picture").unwrap();
        assert!(read_raw(&file, [40, 30, 160]).unwrap_err().contains("not one this build wrote"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_later_picture_is_written_as_jpeg_and_read_back_at_its_size() {
        let dir = scratch("jpeg");
        let raw = dir.join("source.bgra");
        raw_file(&raw, 64, 48);
        let image = read_raw(&raw, [64, 48, 256]).unwrap();
        let jpeg = dir.join(splash_pick::frame_file(7));
        assert!(write_jpeg(image, &jpeg), "ImageIO wrote the JPEG");
        // SAFETY: images made here, released here.
        unsafe { CGImageRelease(image) };
        let back = read_jpeg(&jpeg).expect("read back and decoded");
        unsafe {
            assert_eq!((CGImageGetWidth(back), CGImageGetHeight(back)), (64, 48));
            CGImageRelease(back);
        }
        assert!(read_jpeg(&dir.join("missing.jpg")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_nonce_names_one_drawing() {
        let a = nonce();
        assert!(a.contains('-') && a.ends_with(&format!("{:x}", std::process::id())));
    }
}
