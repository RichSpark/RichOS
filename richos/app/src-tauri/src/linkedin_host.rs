//! **THE USER'S OWN BROWSER, READ AND PRESSED THROUGH ACCESSIBILITY** — the shell's half of
//! `richos_core::linkedin_tools` (Rich's `richos_linkedin.read_profile`).
//!
//! Every decision (which address, which tab is the page, when it has settled, what to press,
//! which pages to open, what text goes to Rich) is in `richos-core`, tested against a fake.
//! This file is only the readings and the presses, the split `screen.rs` documents:
//!
//!   - the default browser: Launch Services' default application for an `https:` address
//!     (`LSCopyDefaultApplicationURLForURL`, as `output_files.rs` asks it for a file);
//!   - opening a page: `/usr/bin/open -g -b <browser> <address>`, in the background, so nothing
//!     is raised over RichOS (`opener.rs` runs `/usr/bin/open` the same way);
//!   - the page: the browser's `AXWebArea` in each window, walked in document order: headings
//!     (`AXHeading`, level from `AXValue`), words (`AXStaticText` `AXValue`), links (`AXLink`,
//!     `AXURL`) and buttons, each with the landmark it sits in (`AXSubrole` `AXLandmark…`);
//!   - a press: `AXUIElementPerformAction(…, "AXPress")` on the controls the core names, found
//!     by the same walk, so the numbering is the one the core saw;
//!   - Chromium's switch: `AXEnhancedUserInterface` on the application element, what VoiceOver
//!     sets. The guest walk of 2026-11-10 measured Chrome exposing no page until it was set.
//!
//! No cookie, password, keychain item or browser file is read. The permission is the
//! Accessibility one RichScribe already asks for (`dictation_app::mac::prompt_accessibility`).
//! This process is a child of the app (through `claude`), so macOS attributes it to RichOS.
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::bundle::CFBundle;
use core_foundation::dictionary::CFDictionary;
use core_foundation::error::CFErrorRef;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation::url::{CFURLRef, CFURL};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use richos_core::linkedin_tools::{Block, BrowserApp, BrowserHost, Page, Piece, Switch};
use std::ffi::c_void;
use std::time::Duration;

type AXUIElementRef = *const c_void;
type CFTypeRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, value: CFTypeRef) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFTypeRef) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> i32;
}

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSCopyDefaultApplicationURLForURL(url: CFURLRef, roles: u32, error: *mut CFErrorRef) -> CFURLRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFURLCreateWithString(allocator: CFTypeRef, string: CFTypeRef, base: CFURLRef) -> CFURLRef;
}

/// `kLSRolesAll`.
const ROLES_ALL: u32 = 0xFFFF_FFFF;
/// How deep the walk goes. LinkedIn's saved pages nest about forty levels; this is room, and a
/// bound against a tree that loops.
const MAX_DEPTH: usize = 200;
/// Nodes walked per page at most: a saved profile is a few thousand.
const MAX_NODES: usize = 60_000;

/// The Mac's browsers through Accessibility.
pub struct MacBrowser;

/// An Accessibility element or CF value we own.
type Element = CFType;

fn attribute(element: &Element, name: &'static str) -> Option<CFType> {
    let name = CFString::from_static_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: a live element, a CFString attribute name and an out-parameter; a value returned
    // is +1 (the Copy rule) and wrapped under the create rule exactly once.
    let error = unsafe { AXUIElementCopyAttributeValue(element.as_CFTypeRef(), name.as_CFTypeRef(), &mut value) };
    if value.is_null() {
        return None;
    }
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    (error == 0).then_some(value)
}

fn text(element: &Element, name: &'static str) -> Option<String> {
    let value = attribute(element, name)?;
    if let Some(s) = value.downcast::<CFString>() {
        return Some(s.to_string());
    }
    value.downcast::<CFURL>().map(|u| u.get_string().to_string())
}

fn children(element: &Element) -> Vec<Element> {
    let Some(value) = attribute(element, "AXChildren") else { return Vec::new() };
    let Some(array) = value.downcast::<CFArray<*const c_void>>() else { return Vec::new() };
    // SAFETY: each item is a live element the array holds; wrapping under the get rule
    // retains it, so it outlives the array.
    array.iter().filter(|item| !item.is_null()).map(|item| unsafe { CFType::wrap_under_get_rule(*item) }).collect()
}

/// The words under an element, joined: a heading's or a control's text.
fn words_under(element: &Element) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut stack: Vec<(Element, usize)> = vec![(element.clone(), 0)];
    let mut seen = 0;
    while let Some((el, depth)) = stack.pop() {
        seen += 1;
        if seen > 2_000 || depth > 40 {
            break;
        }
        if text(&el, "AXRole").as_deref() == Some("AXStaticText") {
            if let Some(v) = text(&el, "AXValue") {
                out.push(v);
            }
            continue;
        }
        let mut kids = children(&el);
        kids.reverse();
        stack.extend(kids.into_iter().map(|k| (k, depth + 1)));
    }
    out.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn labels(element: &Element) -> Vec<String> {
    let mut out: Vec<String> = ["AXTitle", "AXDescription"].iter().filter_map(|a| text(element, a)).collect();
    out.push(words_under(element));
    out.retain(|l| !l.trim().is_empty());
    out
}

/// One page in document order, and the element of every control in it (same order).
fn walk(area: &Element) -> (Vec<Piece>, Vec<Element>) {
    let mut pieces = Vec::new();
    let mut controls = Vec::new();
    let mut stack: Vec<(Element, usize, Option<String>)> = children(area).into_iter().rev().map(|c| (c, 1, None)).collect();
    let mut nodes = 0;
    while let Some((el, depth, region)) = stack.pop() {
        nodes += 1;
        if nodes > MAX_NODES {
            break;
        }
        let role = text(&el, "AXRole").unwrap_or_default();
        let region = text(&el, "AXSubrole").filter(|s| s.starts_with("AXLandmark")).or(region);
        let mut descend = true;
        match role.as_str() {
            // A frame inside the page (an ad, an embed) is another page.
            "AXWebArea" => descend = false,
            "AXStaticText" => {
                if let Some(v) = text(&el, "AXValue").filter(|v| !v.trim().is_empty()) {
                    pieces.push(Piece { block: Block::Text(v), region: region.clone() });
                }
                descend = false;
            }
            "AXHeading" => {
                let level = attribute(&el, "AXValue").and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_i64())
                    .filter(|n| (1..=6).contains(n)).unwrap_or(0) as u8;
                let words = text(&el, "AXTitle").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| words_under(&el));
                pieces.push(Piece { block: Block::Heading(level, words), region: region.clone() });
                // A link inside a heading still counts as a link.
                for link in links_under(&el) {
                    pieces.push(Piece { block: Block::Control { labels: labels(&link), url: text(&link, "AXURL") },
                        region: region.clone() });
                    controls.push(link);
                }
                descend = false;
            }
            "AXLink" => {
                pieces.push(Piece { block: Block::Control { labels: labels(&el), url: text(&el, "AXURL") }, region: region.clone() });
                controls.push(el.clone());
            }
            "AXButton" | "AXDisclosureTriangle" => {
                pieces.push(Piece { block: Block::Control { labels: labels(&el), url: None }, region: region.clone() });
                controls.push(el.clone());
                descend = false;
            }
            _ => {}
        }
        if descend && depth < MAX_DEPTH {
            for child in children(&el).into_iter().rev() {
                stack.push((child, depth + 1, region.clone()));
            }
        }
    }
    (pieces, controls)
}

fn links_under(element: &Element) -> Vec<Element> {
    let mut out = Vec::new();
    let mut stack = children(element);
    stack.reverse();
    let mut seen = 0;
    while let Some(el) = stack.pop() {
        seen += 1;
        if seen > 2_000 {
            break;
        }
        if text(&el, "AXRole").as_deref() == Some("AXLink") {
            out.push(el);
            continue;
        }
        let mut kids = children(&el);
        kids.reverse();
        stack.extend(kids);
    }
    out
}

/// Every `AXWebArea` that is a window's page (not a frame inside one), with its address.
fn web_areas(app: &Element) -> Vec<(Element, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<(Element, usize)> = children(app).into_iter().map(|c| (c, 0)).collect();
    let mut seen = 0;
    while let Some((el, depth)) = stack.pop() {
        seen += 1;
        if seen > 20_000 {
            break;
        }
        if text(&el, "AXRole").as_deref() == Some("AXWebArea") {
            out.push((el.clone(), text(&el, "AXURL").unwrap_or_default()));
            continue;
        }
        if depth < 60 {
            stack.extend(children(&el).into_iter().map(|c| (c, depth + 1)));
        }
    }
    out
}

/// The running app with this bundle identifier, by `NSRunningApplication`.
fn pid_of(bundle_id: &str) -> Option<i32> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSRunningApplication")?;
        let id = CFString::new(bundle_id);
        // SAFETY: documented read-only class and instance messages; a CFString is toll-free
        // bridged to NSString, and the array and its first object are autoreleased.
        unsafe {
            let list: *mut AnyObject = msg_send![class, runningApplicationsWithBundleIdentifier: id.as_concrete_TypeRef() as *mut AnyObject];
            if list.is_null() {
                return None;
            }
            let first: *mut AnyObject = msg_send![list, firstObject];
            if first.is_null() {
                return None;
            }
            let pid: i32 = msg_send![first, processIdentifier];
            (pid > 0).then_some(pid)
        }
    })
}

fn app_element(app: &BrowserApp) -> Option<Element> {
    let pid = pid_of(&app.bundle_id)?;
    // SAFETY: AXUIElementCreateApplication returns a +1 element, wrapped once.
    let element = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateApplication(pid)) };
    // A busy browser must not hang the tool: each Accessibility message waits 3 s at most.
    unsafe { AXUIElementSetMessagingTimeout(element.as_CFTypeRef(), 3.0) };
    Some(element)
}

fn describe(url: &CFURL) -> Option<BrowserApp> {
    let path = url.to_path()?;
    let bundle = CFBundle::new(url.clone())?;
    let info: CFDictionary<CFString, CFType> = bundle.info_dictionary();
    let field = |key: &'static str| info.find(CFString::from_static_string(key))
        .and_then(|value| value.downcast::<CFString>()).map(|s| s.to_string()).filter(|s| !s.trim().is_empty());
    let bundle_id = field("CFBundleIdentifier")?;
    let name = field("CFBundleDisplayName").or_else(|| field("CFBundleName"))
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))?;
    Some(BrowserApp { bundle_id, name })
}

impl BrowserHost for MacBrowser {
    fn trusted(&self) -> bool {
        // SAFETY: no arguments.
        unsafe { AXIsProcessTrusted() }
    }

    fn ask_for_permission(&self) {
        crate::dictation_app::mac::prompt_accessibility();
    }

    fn default_browser(&self) -> Option<BrowserApp> {
        let address = CFString::from_static_string("https://www.linkedin.com/");
        // SAFETY: both calls follow the Create rule; a null result is "none", checked first.
        unsafe {
            let url = CFURLCreateWithString(std::ptr::null(), address.as_CFTypeRef(), std::ptr::null());
            if url.is_null() {
                return None;
            }
            let url = CFURL::wrap_under_create_rule(url);
            let app = LSCopyDefaultApplicationURLForURL(url.as_concrete_TypeRef(), ROLES_ALL, std::ptr::null_mut());
            if app.is_null() {
                return None;
            }
            describe(&CFURL::wrap_under_create_rule(app))
        }
    }

    fn open(&self, url: &str, app: &BrowserApp) -> Result<(), String> {
        // The core accepted the address (`http`/`https` only), so `open` cannot read it as an option.
        let status = std::process::Command::new("/usr/bin/open").args(["-g", "-b", &app.bundle_id, url]).status()
            .map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err(format!("open exited with {status}")) }
    }

    fn pages(&self, app: &BrowserApp) -> Vec<Page> {
        let Some(element) = app_element(app) else { return Vec::new() };
        web_areas(&element).into_iter().map(|(area, url)| Page { url, pieces: walk(&area).0 }).collect()
    }

    fn press(&self, app: &BrowserApp, url: &str, controls: &[usize]) -> usize {
        let Some(element) = app_element(app) else { return 0 };
        let Some((area, _)) = web_areas(&element).into_iter().find(|(_, u)| u == url) else { return 0 };
        let (_, elements) = walk(&area);
        let action = CFString::from_static_string("AXPress");
        controls.iter().filter_map(|n| elements.get(*n))
            // SAFETY: a live element and a CFString action name.
            .filter(|el| unsafe { AXUIElementPerformAction(el.as_CFTypeRef(), action.as_CFTypeRef()) } == 0)
            .count()
    }

    fn set_tree_switch(&self, app: &BrowserApp, on: bool) -> Switch {
        let Some(element) = app_element(app) else { return Switch::NotRunning };
        let was = attribute(&element, "AXEnhancedUserInterface").and_then(|v| v.downcast::<CFBoolean>()).map(bool::from);
        let name = CFString::from_static_string("AXEnhancedUserInterface");
        let value = if on { CFBoolean::true_value() } else { CFBoolean::false_value() };
        // SAFETY: a live element, a CFString name and a CFBoolean. Chrome answers -25208 (not
        // implemented) and builds its tree all the same (measured 2026-11-10), so the code
        // returned is not a verdict.
        unsafe { AXUIElementSetAttributeValue(element.as_CFTypeRef(), name.as_CFTypeRef(), value.as_CFTypeRef()) };
        Switch::Set { was }
    }

    fn sleep(&self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Launch Services names a default browser for an `https:` address on any Mac with one; the
    /// question opens nothing and needs no permission.
    #[test]
    fn the_default_browser_is_named_by_its_bundle_identifier() {
        let app = MacBrowser.default_browser().expect("this Mac names no default browser");
        assert!(app.bundle_id.contains('.'), "{app:?}");
        assert!(!app.name.trim().is_empty(), "{app:?}");
    }

    /// A browser that is not running has no page, nothing to press and no switch to set, and
    /// nothing is started to find that out.
    #[test]
    fn a_browser_that_is_not_running_answers_nothing_and_is_not_started() {
        let absent = BrowserApp { bundle_id: "com.richos.no-such-browser".into(), name: "None".into() };
        assert!(MacBrowser.pages(&absent).is_empty());
        assert_eq!(MacBrowser.press(&absent, "https://www.linkedin.com/in/x/", &[0]), 0);
        assert_eq!(MacBrowser.set_tree_switch(&absent, true), Switch::NotRunning);
        assert_eq!(pid_of("com.richos.no-such-browser"), None);
    }
}
