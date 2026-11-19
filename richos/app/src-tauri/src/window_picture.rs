//! Pictures of RichOS webviews for Bust a bug and Make a video.
//!
//! # Why WebKit's snapshot and nothing else
//!
//! `WKWebView takeSnapshotWithConfiguration:
//! completionHandler:` draws the app's own view into an image. It reads nothing outside this
//! app, so macOS asks for no permission: no Screen Recording prompt, no entry in Privacy &
//! Security. Every other way to picture a window (`screencapture`, `CGWindowListCreateImage`,
//! ScreenCaptureKit) goes through the screen-capture permission, which the CEO's brief rules out.
//!
//! Bust a bug gives one picture to the user's own `claude` as an image block on standard input.
//! Make a video composites pictures of the focused RichCleaner webview into its video canvas.
//! Both use in-memory JPEGs at quality 0.6.

use std::time::Duration;

/// How long WebKit gets to draw the picture. It answers in tens of milliseconds on a window
/// that is on screen; a window that is not (minimized, another Space) may never answer.
const DEADLINE: Duration = Duration::from_secs(3);

/// The largest picture handed on. Claude takes an image of up to 5 MB once base64-encoded,
/// which is 3.75 MB of bytes; a full-screen Retina window at quality 0.6 measured far below.
pub const MAX_BYTES: usize = 3_500_000;

/// A JPEG of the webview of `window` as it is now, or `None` (not macOS, no answer in
/// [`DEADLINE`], or WebKit could not draw it). Waits, so it is called off the IPC threads.
pub fn take(window: &tauri::WebviewWindow) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
        // `with_webview` runs this on the main thread, where WebKit must be called.
        let asked = window.with_webview(move |webview| {
            mac::snapshot(webview.inner(), None, move |picture| {
                if tx.send(picture).is_err() {
                    eprintln!("[richos] bug report: the window's picture came after it was no longer waited for");
                }
            });
        });
        if let Err(e) = asked {
            eprintln!("[richos] bug report: the window's picture could not be asked for ({e})");
            return None;
        }
        match rx.recv_timeout(DEADLINE) {
            Ok(Some(bytes)) => within_cap(bytes),
            Ok(None) => None,
            Err(_) => {
                eprintln!("[richos] bug report: WebKit did not draw the window within {DEADLINE:?}; Rich gets the words only");
                None
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        None
    }
}

/// The focused window's content as two little-endian cursor coordinates followed by a JPEG.
/// An unfocused window returns no frame. Give each request a deadline that can't stall the movie.
pub async fn take_focused(window: &tauri::WebviewWindow) -> Result<Option<Vec<u8>>, ()> {
    #[cfg(target_os = "macos")]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        window.with_webview(move |webview| {
            let view = webview.inner();
            let Some(cursor) = mac::focused_cursor(view) else {
                let _ignored = send.send(Ok(None));
                return;
            };
            mac::snapshot(view, Some(1200.0), move |picture| {
                let result = picture.filter(|jpeg| jpeg.len() <= MAX_BYTES).ok_or(()).map(|jpeg| {
                    let mut frame = Vec::with_capacity(8 + jpeg.len());
                    frame.extend_from_slice(&cursor.0.to_le_bytes());
                    frame.extend_from_slice(&cursor.1.to_le_bytes());
                    frame.extend_from_slice(&jpeg);
                    Some(frame)
                });
                let _ignored = send.send(result);
            });
        }).map_err(|_| ())?;
        tokio::time::timeout(Duration::from_millis(400), receive).await.map_err(|_| ())?.map_err(|_| ())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _window = window;
        Ok(None)
    }
}

/// A picture Claude will take, or none: over [`MAX_BYTES`] Rich gets the screen's words only.
fn within_cap(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.len() <= MAX_BYTES {
        return Some(bytes);
    }
    eprintln!("[richos] bug report: the window's picture is {} bytes, over {MAX_BYTES}; Rich gets the words only", bytes.len());
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use crate::dictation::appkit::{NSPoint, NSRect};
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::c_void;
    use std::sync::Mutex;

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        /// `NSBitmapImageRepPropertyKey`, the JPEG quality's key.
        static NSImageCompressionFactor: *mut AnyObject;
    }

    /// `NSBitmapImageFileTypeJPEG` (AppKit's `NSBitmapImageRep.h`: TIFF 0, BMP 1, GIF 2, JPEG 3).
    const JPEG: usize = 3;

    /// Called on the main thread. `width` caps the logical width for repeated video snapshots.
    pub fn snapshot(wkwebview: *mut c_void, width: Option<f64>, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
        let view = wkwebview as *mut AnyObject;
        if view.is_null() {
            done(None);
            return;
        }
        let once = Mutex::new(Some(done));
        let block = block2::RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
            if let Some(done) = once.lock().unwrap_or_else(|p| p.into_inner()).take() {
                done(if image.is_null() { None } else { jpeg(image) });
            }
        });
        // SAFETY: documented WebKit and Foundation methods, called on the main thread with
        // the view Tauri supplied. WebKit copies the block it is given.
        unsafe {
            let configuration: Option<Retained<AnyObject>> = width.and_then(|limit| {
                let class = AnyClass::get(c"WKSnapshotConfiguration")?;
                let numbers = AnyClass::get(c"NSNumber")?;
                let configuration: Retained<AnyObject> = msg_send![class, new];
                let bounds: NSRect = msg_send![view, bounds];
                let number: *mut AnyObject = msg_send![numbers, numberWithDouble: bounds.size.width.clamp(1., limit)];
                let () = msg_send![&*configuration, setSnapshotWidth: number];
                Some(configuration)
            });
            let config = configuration.as_deref().map_or(std::ptr::null(), std::ptr::from_ref);
            let () = msg_send![view, takeSnapshotWithConfiguration: config, completionHandler: &*block];
        }
    }

    /// Only include the window currently being used. A pointer outside its content is absent.
    pub fn focused_cursor(wkwebview: *mut c_void) -> Option<(f32, f32)> {
        let view = wkwebview.cast::<AnyObject>();
        if view.is_null() {
            return None;
        }
        // SAFETY: the window and view are live on the main thread. Use the view's own
        // coordinate system so the pointer scales with the snapshot.
        unsafe {
            let window: *mut AnyObject = msg_send![view, window];
            if window.is_null() {
                return None;
            }
            let key: bool = msg_send![window, isKeyWindow];
            if !key {
                return None;
            }
            let bounds: NSRect = msg_send![view, bounds];
            let mouse: NSPoint = msg_send![window, mouseLocationOutsideOfEventStream];
            let point: NSPoint = msg_send![view, convertPoint: mouse, fromView: std::ptr::null::<AnyObject>()];
            let flipped: bool = msg_send![view, isFlipped];
            let x = (point.x - bounds.origin.x) / bounds.size.width;
            let y = (point.y - bounds.origin.y) / bounds.size.height;
            if (0. ..1.).contains(&x) && (0. ..1.).contains(&y) {
                Some((x as f32, if flipped { y } else { 1. - y } as f32))
            } else {
                Some((f32::NAN, f32::NAN))
            }
        }
    }

    /// The image as JPEG bytes at quality 0.6: TIFF, then a bitmap, then JPEG, AppKit's own path.
    pub(super) fn jpeg(image: *mut AnyObject) -> Option<Vec<u8>> {
        objc2::rc::autoreleasepool(|_| {
            let bitmap_class = AnyClass::get(c"NSBitmapImageRep")?;
            let number_class = AnyClass::get(c"NSNumber")?;
            let dictionary_class = AnyClass::get(c"NSDictionary")?;
            // SAFETY: documented AppKit and Foundation methods on objects this block was handed or
            // just made; every object returned is checked for nil before it is used, and the bytes
            // are copied out before the pool that owns them drains.
            unsafe {
                let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
                if tiff.is_null() {
                    return None;
                }
                let bitmap: *mut AnyObject = msg_send![bitmap_class, imageRepWithData: tiff];
                if bitmap.is_null() {
                    return None;
                }
                let quality: *mut AnyObject = msg_send![number_class, numberWithDouble: 0.6f64];
                let properties: *mut AnyObject = msg_send![dictionary_class, dictionaryWithObject: quality, forKey: NSImageCompressionFactor];
                let data: *mut AnyObject = msg_send![bitmap, representationUsingType: JPEG, properties: properties];
                if data.is_null() {
                    return None;
                }
                let length: usize = msg_send![data, length];
                let bytes: *const c_void = msg_send![data, bytes];
                if bytes.is_null() || length == 0 {
                    return None;
                }
                Some(std::slice::from_raw_parts(bytes as *const u8, length).to_vec())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_over_the_cap_is_not_handed_on() {
        assert_eq!(within_cap(vec![0; MAX_BYTES]).map(|b| b.len()), Some(MAX_BYTES));
        assert_eq!(within_cap(vec![0; MAX_BYTES + 1]), None);
        // Claude's 5 MB limit is on the base64 text: 4 characters for every 3 bytes.
        assert!(MAX_BYTES.div_ceil(3) * 4 < 5 * 1024 * 1024);
    }

    /// THE CONVERSION THE SNAPSHOT'S IMAGE GOES THROUGH, on a real NSImage: AppKit's TIFF, bitmap
    /// and JPEG path, with the message signatures objc2 checks in a debug build. The snapshot
    /// itself needs a window on screen, which the test VM walk (bug-report-walk.sh) provides.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_nsimage_becomes_a_jpeg() {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        // A 2 by 2 BMP, the smallest image NSImage reads without a codec this test must build.
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&70u32.to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        for v in [40u32, 2, 2] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        for v in [0u32, 16, 2835, 2835, 0, 0] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..2 {
            bmp.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0]); // two red pixels, padded to 8
        }
        assert_eq!(bmp.len(), 70);
        let jpeg = objc2::rc::autoreleasepool(|_| unsafe {
            let data_class = AnyClass::get(c"NSData").unwrap();
            let image_class = AnyClass::get(c"NSImage").unwrap();
            let data: *mut AnyObject = msg_send![data_class, dataWithBytes: bmp.as_ptr() as *const std::ffi::c_void, length: bmp.len()];
            let image: *mut AnyObject = msg_send![image_class, alloc];
            let image: *mut AnyObject = msg_send![image, initWithData: data];
            assert!(!image.is_null(), "NSImage did not read the BMP");
            let jpeg = mac::jpeg(image);
            let () = msg_send![image, release];
            jpeg
        });
        let jpeg = jpeg.expect("AppKit gave no JPEG");
        assert_eq!(&jpeg[..3], &[0xFF, 0xD8, 0xFF], "not a JPEG");
        assert_eq!(&jpeg[jpeg.len() - 2..], &[0xFF, 0xD9], "the JPEG does not end");
    }
}
