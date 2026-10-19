//! **THE RIGHT-CLICK MENU WEBKIT REALLY BUILDS, AND WHAT RICHOS LETS THROUGH** (CEO, nightly 65,
//! 2026-10-19: no Look Up, Translate, Search with Google, Share, Speech or Services anywhere).
//!
//! A real WKWebView from this Mac's own WebKit, in a window that is never put on screen (the
//! process cannot even show a Dock icon), loads a page carrying the shipped
//! `ui/no-native-menu.js`, a composer stand-in and a PDF preview, and is sent right-clicks as
//! AppKit events handed straight to the view: nothing is posted to the system's input, and no
//! menu is ever drawn. In place of AppKit's `+[NSMenu popUpContextMenu:withEvent:forView:]` a
//! recorder takes each menu that would be shown, and tells the menu's delegate it opened and
//! closed the way AppKit would. `src/webview_menu.rs` is compiled by path and installed over that
//! recorder exactly as `main` installs it over AppKit.
//!
//! ```text
//! context_menu_probe                     the shipped behavior; exits 0 when every check passes
//! context_menu_probe --without-the-fix   the same checks with src/webview_menu.rs not installed
//!                                        (the behavior before it); they must fail
//! ```
//!
//! Run by `scripts/context-menu-probe.test.sh`, which requires both outcomes.
#![allow(dead_code)]

#[path = "../src/webview_menu.rs"]
mod webview_menu;

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("context_menu_probe: WebKit's macOS menu does not exist on this platform; nothing to check");
}

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(mac::run(std::env::args().any(|a| a == "--without-the-fix")));
}

#[cfg(target_os = "macos")]
mod mac {
    use super::webview_menu;
    use base64::Engine as _;
    use objc2::encode::{Encode, Encoding};
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::sel;
    use std::ffi::{c_char, CStr, CString};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}
    #[link(name = "WebKit", kind = "framework")]
    extern "C" {}

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: NSPoint,
        size: NSSize,
    }
    // SAFETY: CoreGraphics' CGPoint, CGSize and CGRect on 64-bit, with the encodings AppKit declares.
    unsafe impl Encode for NSPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for NSSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for NSRect {
        const ENCODING: Encoding = Encoding::Struct("CGRect", &[NSPoint::ENCODING, NSSize::ENCODING]);
    }

    const WIDTH: f64 = 800.0;
    const HEIGHT: f64 = 600.0;

    /// One item of a menu that would have been shown.
    #[derive(Debug, Clone)]
    struct Item {
        title: String,
        action: String,
        to_web_view: bool,
    }

    /// Every menu that reached the screen's call, in order; separators left out.
    static SHOWN: Mutex<Vec<Vec<Item>>> = Mutex::new(Vec::new());
    static WEB_VIEW: Mutex<usize> = Mutex::new(0);

    fn shown() -> Vec<Vec<Item>> {
        SHOWN.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn text(s: *mut AnyObject) -> String {
        if s.is_null() {
            return String::new();
        }
        // SAFETY: an NSString; the bytes are copied before any pool drains.
        unsafe {
            let utf8: *const c_char = msg_send![s, UTF8String];
            if utf8.is_null() { String::new() } else { CStr::from_ptr(utf8).to_string_lossy().into_owned() }
        }
    }

    fn ns(s: &str) -> *mut AnyObject {
        let class = AnyClass::get(c"NSString").expect("Foundation is loaded");
        let c = CString::new(s).expect("no NUL in the probe's own strings");
        // SAFETY: documented class method; the string is autoreleased into the caller's pool.
        unsafe { msg_send![class, stringWithUTF8String: c.as_ptr()] }
    }

    /// Stands in for AppKit's implementation: records the menu, with the items AppKit would add to
    /// it on its own, and plays its delegate's open and close, drawing nothing.
    unsafe extern "C-unwind" fn record(
        _class: *const AnyClass,
        _cmd: Sel,
        menu: *mut AnyObject,
        _event: *mut AnyObject,
        _view: *mut AnyObject,
    ) {
        let web_view = *WEB_VIEW.lock().unwrap_or_else(|p| p.into_inner());
        let mut items = Vec::new();
        // SAFETY: documented NSMenu, NSMenuItem and NSMenuDelegate methods on the menu handed in.
        unsafe {
            let delegate: *mut AnyObject = msg_send![menu, delegate];
            let opens = !delegate.is_null() && {
                let r: Bool = msg_send![delegate, respondsToSelector: sel!(menuWillOpen:)];
                r.as_bool()
            };
            if opens {
                let () = msg_send![delegate, menuWillOpen: menu];
            }
            let list: *mut AnyObject = msg_send![menu, itemArray];
            let n: usize = msg_send![list, count];
            for i in 0..n {
                let item: *mut AnyObject = msg_send![list, objectAtIndex: i];
                let separator: Bool = msg_send![item, isSeparatorItem];
                if separator.as_bool() {
                    continue;
                }
                let action: Option<Sel> = msg_send![item, action];
                let target: *mut AnyObject = msg_send![item, target];
                items.push(Item {
                    title: text(msg_send![item, title]),
                    action: action.map(|s| s.name().to_string_lossy().into_owned()).unwrap_or_default(),
                    to_web_view: target as usize == web_view && web_view != 0,
                });
            }
            // What AppKit's own call adds to a context menu after it is handed over, which no
            // recorder in its place ever sees as items: system-wide Services while
            // `allowsContextMenuPlugIns` is on, Writing Tools while
            // `automaticallyInsertsWritingToolsItems` is on (macOS 15.2 and later). Both default
            // to on (NSMenu.h), so each switch left on is listed as the items it lets in.
            let plug_ins: Bool = msg_send![menu, allowsContextMenuPlugIns];
            if plug_ins.as_bool() {
                items.push(Item { title: "Services (AppKit appends them: allowsContextMenuPlugIns is on)".into(), action: String::new(), to_web_view: false });
            }
            let has_writing_tools: Bool = msg_send![menu, respondsToSelector: sel!(automaticallyInsertsWritingToolsItems)];
            if has_writing_tools.as_bool() {
                let writing_tools: Bool = msg_send![menu, automaticallyInsertsWritingToolsItems];
                if writing_tools.as_bool() {
                    items.push(Item { title: "Writing Tools (AppKit inserts them: automaticallyInsertsWritingToolsItems is on)".into(), action: String::new(), to_web_view: false });
                }
            }
            let closes = !delegate.is_null() && {
                let r: Bool = msg_send![delegate, respondsToSelector: sel!(menuDidClose:)];
                r.as_bool()
            };
            if closes {
                let () = msg_send![delegate, menuDidClose: menu];
            }
        }
        SHOWN.lock().unwrap_or_else(|p| p.into_inner()).push(items);
    }

    fn put_recorder_in_appkits_place() {
        type PopUp = unsafe extern "C-unwind" fn(*const AnyClass, Sel, *mut AnyObject, *mut AnyObject, *mut AnyObject);
        let menu = AnyClass::get(c"NSMenu").expect("AppKit is loaded");
        let method = menu.class_method(sel!(popUpContextMenu:withEvent:forView:)).expect("NSMenu shows context menus");
        let ours: PopUp = record;
        // SAFETY: `record` has the class method's signature and never calls the implementation it
        // replaces, so no menu is ever drawn by this process.
        let _appkit = unsafe { method.set_implementation(std::mem::transmute::<PopUp, Imp>(ours)) };
    }

    fn spin(seconds: f64) {
        // SAFETY: documented Foundation methods; main thread.
        unsafe {
            let date_class = AnyClass::get(c"NSDate").expect("Foundation is loaded");
            let date: *mut AnyObject = msg_send![date_class, dateWithTimeIntervalSinceNow: seconds];
            let run_loop_class = AnyClass::get(c"NSRunLoop").expect("Foundation is loaded");
            let run_loop: *mut AnyObject = msg_send![run_loop_class, currentRunLoop];
            let () = msg_send![run_loop, runUntilDate: date];
        }
    }

    /// The smallest PDF with one line of text, made here so the probe reads no file.
    fn pdf() -> Vec<u8> {
        let stream = "BT /F1 36 Tf 40 700 Td (Quarterly report preview) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out += &format!("{} 0 obj\n{object}\nendobj\n", i + 1);
        }
        let xref = out.len();
        out += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for offset in offsets {
            out += &format!("{offset:010} 00000 n \n");
        }
        out += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
        out.into_bytes()
    }

    /// The page: the shipped guard (or none), the company name, the composer and a PDF preview.
    fn page(with_script: bool) -> String {
        let script = if with_script { include_str!("../../ui/no-native-menu.js") } else { "" };
        let pdf = base64::engine::general_purpose::STANDARD.encode(pdf());
        format!(
            "<!doctype html><html><head><script>{script}</script></head>\
             <body style=\"margin:0;font:16px -apple-system\">\
             <p id=\"name\" style=\"position:absolute;left:10px;top:10px;margin:0\">Acme Holdings</p>\
             <textarea id=\"input\" style=\"position:absolute;left:10px;top:60px;width:360px;height:100px\">A message to right-click</textarea>\
             <iframe class=\"of-pdf\" src=\"data:application/pdf;base64,{pdf}\" \
             style=\"position:absolute;left:400px;top:0;width:400px;height:600px;border:0\"></iframe>\
             </body></html>"
        )
    }

    struct Probe {
        window: *mut AnyObject,
        web_view: *mut AnyObject,
    }

    impl Probe {
        fn new() -> Probe {
            // SAFETY: documented AppKit and WebKit constructors and setters; main thread. Nothing
            // here orders the window on screen.
            unsafe {
                let app_class = AnyClass::get(c"NSApplication").expect("AppKit is loaded");
                let app: *mut AnyObject = msg_send![app_class, sharedApplication];
                // NSApplicationActivationPolicyProhibited: no Dock icon, never activated.
                let _: Bool = msg_send![app, setActivationPolicy: 2isize];
                let frame = NSRect { origin: NSPoint { x: 0.0, y: 0.0 }, size: NSSize { width: WIDTH, height: HEIGHT } };
                let window_class = AnyClass::get(c"NSWindow").expect("AppKit is loaded");
                let window: *mut AnyObject = msg_send![window_class, alloc];
                // NSWindowStyleMaskTitled, NSBackingStoreBuffered.
                let window: *mut AnyObject =
                    msg_send![window, initWithContentRect: frame, styleMask: 1usize, backing: 2usize, defer: Bool::NO];
                let () = msg_send![window, setReleasedWhenClosed: Bool::NO];
                // As wry builds a window: a parent content view holding the WKWebView.
                let view_class = AnyClass::get(c"NSView").expect("AppKit is loaded");
                let parent: *mut AnyObject = msg_send![view_class, alloc];
                let parent: *mut AnyObject = msg_send![parent, initWithFrame: frame];
                let config_class = AnyClass::get(c"WKWebViewConfiguration").expect("WebKit is loaded");
                let config: *mut AnyObject = msg_send![config_class, alloc];
                let config: *mut AnyObject = msg_send![config, init];
                let web_class = AnyClass::get(c"WKWebView").expect("WebKit is loaded");
                let web_view: *mut AnyObject = msg_send![web_class, alloc];
                let web_view: *mut AnyObject = msg_send![web_view, initWithFrame: frame, configuration: config];
                let () = msg_send![parent, addSubview: web_view];
                let () = msg_send![window, setContentView: parent];
                *WEB_VIEW.lock().unwrap_or_else(|p| p.into_inner()) = web_view as usize;
                Probe { window, web_view }
            }
        }

        fn load(&self, with_script: bool) {
            // SAFETY: documented WKWebView methods; main thread.
            unsafe {
                let none: *mut AnyObject = std::ptr::null_mut();
                let _: *mut AnyObject = msg_send![self.web_view, loadHTMLString: ns(&page(with_script)), baseURL: none];
            }
            let started = Instant::now();
            spin(0.2);
            while started.elapsed() < Duration::from_secs(15) {
                // SAFETY: documented WKWebView property; main thread.
                let loading: Bool = unsafe { msg_send![self.web_view, isLoading] };
                if !loading.as_bool() {
                    break;
                }
                spin(0.1);
            }
            // The PDF viewer starts after the page has loaded.
            spin(2.0);
        }

        fn script(&self, source: &str) {
            let done = block2::RcBlock::new(|_result: *mut AnyObject, _error: *mut AnyObject| {});
            // SAFETY: documented WKWebView method; WebKit copies the block.
            unsafe {
                let () = msg_send![self.web_view, evaluateJavaScript: ns(source), completionHandler: &*done];
            }
            spin(0.5);
        }

        /// A right-click at (x, y) from the page's top-left, handed straight to the web view.
        /// Returns the menus shown and how many the fix dropped, after the menu settles.
        fn right_click(&self, x: f64, y: f64) -> (Vec<Vec<Item>>, usize) {
            let before_shown = shown().len();
            let before_dropped = webview_menu::dropped();
            // SAFETY: documented NSEvent constructor and NSResponder methods; main thread. The
            // events go to this view only, never to the system's input.
            unsafe {
                let event_class = AnyClass::get(c"NSEvent").expect("AppKit is loaded");
                let info_class = AnyClass::get(c"NSProcessInfo").expect("Foundation is loaded");
                let info: *mut AnyObject = msg_send![info_class, processInfo];
                let number: isize = msg_send![self.window, windowNumber];
                let at = NSPoint { x, y: HEIGHT - y };
                let none: *mut AnyObject = std::ptr::null_mut();
                for (kind, pressure, method) in [(3usize, 1.0f32, sel!(rightMouseDown:)), (4usize, 0.0f32, sel!(rightMouseUp:))] {
                    let now: f64 = msg_send![info, systemUptime];
                    let event: *mut AnyObject = msg_send![event_class,
                        mouseEventWithType: kind, location: at, modifierFlags: 0usize, timestamp: now,
                        windowNumber: number, context: none, eventNumber: 1isize, clickCount: 1isize,
                        pressure: pressure];
                    if method == sel!(rightMouseDown:) {
                        let () = msg_send![self.web_view, rightMouseDown: event];
                    } else {
                        let () = msg_send![self.web_view, rightMouseUp: event];
                    }
                }
            }
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(5) {
                spin(0.1);
                if shown().len() > before_shown || webview_menu::dropped() > before_dropped {
                    break;
                }
            }
            spin(0.5);
            (shown()[before_shown..].to_vec(), webview_menu::dropped() - before_dropped)
        }
    }

    /// The words the CEO named, and the rest of what WebKit's and AppKit's menus carry beside them.
    const FORBIDDEN: [&str; 7] = ["Look Up", "Translate", "Search with", "Share", "Speech", "Services", "Writing Tools"];

    fn forbidden(menus: &[Vec<Item>]) -> Vec<String> {
        menus
            .iter()
            .flatten()
            .filter(|item| FORBIDDEN.iter().any(|word| item.title.starts_with(word)))
            .map(|item| item.title.clone())
            .collect()
    }

    fn describe(menus: &[Vec<Item>]) -> String {
        if menus.is_empty() {
            return "no menu".into();
        }
        menus
            .iter()
            .map(|menu| format!("[{}]", menu.iter().map(|i| i.title.as_str()).collect::<Vec<_>>().join(", ")))
            .collect::<Vec<_>>()
            .join(" then ")
    }

    struct Checks {
        failed: usize,
    }

    impl Checks {
        fn check(&mut self, name: &str, result: Result<String, String>) {
            match result {
                Ok(note) => println!("PASS {name} ({note})"),
                Err(why) => {
                    self.failed += 1;
                    println!("FAIL {name}: {why}");
                }
            }
        }
    }

    fn editing_only(menus: &[Vec<Item>]) -> Result<String, String> {
        let [menu] = menus else { return Err(format!("wanted one menu, saw {}", describe(menus))) };
        let got: Vec<(&str, &str)> = menu.iter().map(|i| (i.title.as_str(), i.action.as_str())).collect();
        if got != webview_menu::EDITING {
            return Err(format!("saw {}; forbidden among them: {:?}", describe(menus), forbidden(menus)));
        }
        if !menu.iter().all(|i| i.to_web_view) {
            return Err("the editing commands are not sent to the web view".into());
        }
        Ok(describe(menus))
    }

    pub fn run(without_the_fix: bool) -> i32 {
        objc2::rc::autoreleasepool(|_| {
            put_recorder_in_appkits_place();
            if without_the_fix {
                println!("context_menu_probe: src/webview_menu.rs NOT installed (the behavior before it)");
            } else {
                webview_menu::install();
            }
            let probe = Probe::new();
            let mut checks = Checks { failed: 0 };

            probe.load(true);
            let (menus, _) = probe.right_click(30.0, 18.0);
            checks.check(
                "a right-click on the company name shows no menu",
                if menus.is_empty() { Ok("no menu".into()) } else { Err(format!("saw {}", describe(&menus))) },
            );

            let select = "const t = document.getElementById('input'); t.focus(); t.setSelectionRange(2, 9);";
            probe.script(select);
            let (menus, _) = probe.right_click(60.0, 75.0);
            checks.check(
                "a right-click on selected text in the composer shows only Cut, Copy, Paste, Select All",
                editing_only(&menus),
            );

            let (menus, dropped) = probe.right_click(600.0, 300.0);
            checks.check(
                "a right-click in the PDF preview shows no menu, and WebKit did offer one",
                match (menus.is_empty(), dropped) {
                    (true, 1) => Ok("its menu was not shown".into()),
                    (true, _) => Err("WebKit offered no menu to drop, so this check proved nothing".into()),
                    (false, _) => Err(format!("saw {}; forbidden among them: {:?}", describe(&menus), forbidden(&menus))),
                },
            );

            probe.script(select);
            let (menus, _) = probe.right_click(60.0, 75.0);
            checks.check("after the PDF preview, the composer still gets only the editing menu", editing_only(&menus));

            probe.load(false);
            probe.script("getSelection().selectAllChildren(document.getElementById('name'));");
            let (menus, dropped) = probe.right_click(30.0, 18.0);
            checks.check(
                "with no page script at all, selected page text still shows no menu",
                match (menus.is_empty(), dropped) {
                    (true, 1) => Ok("its menu was not shown".into()),
                    (true, _) => Err("WebKit offered no menu to drop, so this check proved nothing".into()),
                    (false, _) => Err(format!("saw {}; forbidden among them: {:?}", describe(&menus), forbidden(&menus))),
                },
            );

            // SAFETY: documented NSWindow method; the window was never on screen.
            unsafe {
                let () = msg_send![probe.window, close];
            }
            println!("=== context_menu_probe: {} failed ===", checks.failed);
            i32::from(checks.failed > 0)
        })
    }
}
