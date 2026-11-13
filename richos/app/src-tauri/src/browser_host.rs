//! **ANY PAGE OF THE USER'S OWN BROWSER, READ AND PRESSED THROUGH ACCESSIBILITY** — the
//! shell's half of `richos_core::browser_tools` (the team's `richos_browser` tools).
//!
//! Every decision (which tab is the reader's, what a page says, how it is numbered, when it has
//! settled, every sentence a teammate is given) is in `richos-core`, tested against a fake.
//! This file adds to `linkedin_host.rs` (the default browser, the open, the tabs and their
//! closing, Chromium's switch) only what the general reader needs:
//!
//!   - which page a tab shows: the window whose selected tab button (or the window itself,
//!     when Accessibility shows no tab bar in it) is that tab, and its `AXWebArea`;
//!   - a reading: that web area handed to the core's `walk` one element at a time, with the
//!     element of every numbered control and the last element kept until the next reading;
//!   - a press: `AXPress` on the kept element, so it lands on exactly what the teammate was
//!     shown, or is refused when that element has left the page;
//!   - the page's whole text as the browser itself reads it out
//!     (`AXStringForTextMarkerRange`), which the core asks for only when a browser leaves the
//!     spacing between pieces of text out (Safari);
//!   - a scroll: `AXScrollToVisible` on the last element of the page;
//!   - showing a tab: `AXPress` on its tab button, for a browser that adds the reader's tab
//!     without selecting it (Firefox, measured in the guest walk of 2026-11-13).
//!
//! A browser that stops answering must not hang the tool: each Accessibility message waits 3 s
//! at most (`linkedin_host::app_element`) and one reading takes [`READING_BUDGET`] at most,
//! after which the page is returned as far as it was read and said to be cut.
use crate::linkedin_host::{app_element, attribute, children, is_selected, tab_buttons, text, window_area, windows_of,
    Element, MacBrowser};
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use richos_core::browser_tools::{walk, AxNode, PageHost, Pressed, Snapshot, View};
use richos_core::linkedin_tools::BrowserApp;
use std::cell::Cell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementPerformAction(element: *const c_void, action: *const c_void) -> i32;
    fn AXUIElementCopyAttributeNames(element: *const c_void, names: *mut *const c_void) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(element: *const c_void, attribute: *const c_void, parameter: *const c_void,
        value: *mut *const c_void) -> i32;
}

/// A parameterized attribute's value (`AXStringForTextMarkerRange` and the like), when the
/// element has it.
fn parameterized(element: &Element, name: &'static str, parameter: &Element) -> Option<Element> {
    let name = CFString::from_static_string(name);
    let mut value: *const c_void = std::ptr::null();
    // SAFETY: a live element, a CFString attribute name, a live parameter and an out-parameter;
    // a value returned is +1 (the Copy rule) and wrapped under the create rule exactly once.
    let error = unsafe {
        AXUIElementCopyParameterizedAttributeValue(element.as_CFTypeRef(), name.as_CFTypeRef(), parameter.as_CFTypeRef(), &mut value)
    };
    if value.is_null() {
        return None;
    }
    let value = unsafe { Element::wrap_under_create_rule(value) };
    (error == 0).then_some(value)
}

/// Whether the element itself lists the attribute among the ones it has.
fn lists(element: &Element, name: &'static str) -> bool {
    let mut names: *const c_void = std::ptr::null();
    // SAFETY: a live element and an out-parameter; an array returned is +1 (the Copy rule) and
    // wrapped under the create rule exactly once. Its items are CFStrings the array holds.
    let error = unsafe { AXUIElementCopyAttributeNames(element.as_CFTypeRef(), &mut names) };
    if names.is_null() {
        return false;
    }
    let names = unsafe { CFArray::<*const c_void>::wrap_under_create_rule(names as _) };
    error == 0 && names.iter().filter(|n| !n.is_null())
        .any(|n| unsafe { CFString::wrap_under_get_rule(*n as _) } == name)
}

/// `kAXErrorInvalidUIElement`: the element no longer exists.
const INVALID_ELEMENT: i32 = -25202;
/// One reading of a page at most.
const READING_BUDGET: Duration = Duration::from_secs(20);

/// The elements of the last reading: every numbered control, in number order, and the last
/// element that showed something.
#[derive(Default)]
pub struct Kept {
    controls: Vec<Element>,
    last: Option<Element>,
}

/// When a reading must end, and whether it ran out of time.
struct Clock {
    until: Instant,
    late: Cell<bool>,
}

impl Clock {
    fn over(&self) -> bool {
        if Instant::now() > self.until {
            self.late.set(true);
        }
        self.late.get()
    }
}

/// One element as the core's `walk` reads it. Past the reading's time every answer is empty,
/// so the walk ends at once.
#[derive(Clone)]
struct Ax {
    el: Element,
    clock: Rc<Clock>,
}

impl Ax {
    fn text(&self, name: &'static str) -> Option<String> {
        if self.clock.over() { None } else { text(&self.el, name) }
    }
}

impl AxNode for Ax {
    fn role(&self) -> String {
        self.text("AXRole").unwrap_or_default()
    }
    fn subrole(&self) -> Option<String> {
        self.text("AXSubrole")
    }
    fn value_text(&self) -> Option<String> {
        self.text("AXValue")
    }
    fn value_number(&self) -> Option<f64> {
        if self.clock.over() {
            return None;
        }
        let value = attribute(&self.el, "AXValue")?;
        value.downcast::<CFNumber>().and_then(|n| n.to_f64())
            .or_else(|| value.downcast::<CFBoolean>().map(|b| if bool::from(b) { 1.0 } else { 0.0 }))
    }
    fn title(&self) -> Option<String> {
        self.text("AXTitle")
    }
    fn description(&self) -> Option<String> {
        self.text("AXDescription")
    }
    fn url(&self) -> Option<String> {
        self.text("AXURL")
    }
    fn expanded(&self) -> Option<bool> {
        if self.clock.over() {
            return None;
        }
        // A boolean, or the number some browsers give for it (0 closed, anything else open).
        let value = attribute(&self.el, "AXExpanded")?;
        let open = value.downcast::<CFBoolean>().map(bool::from)
            .or_else(|| value.downcast::<CFNumber>().and_then(|n| n.to_i64()).map(|n| n != 0))?;
        // Safari, Chrome and Brave answer "not expanded" for an ordinary link or button when
        // asked (measured on real pages in the guest walk of 2026-11-13: 2,957 of the 3,116
        // controls numbered in Safari's run read "collapsed"), so a closed state counts only
        // for an element that lists the attribute as one of its own.
        (open || lists(&self.el, "AXExpanded")).then_some(open)
    }
    fn children(&self) -> Vec<Self> {
        if self.clock.over() {
            return Vec::new();
        }
        children(&self.el).into_iter().map(|el| Ax { el, clock: self.clock.clone() }).collect()
    }
    fn whole_text(&self) -> Option<String> {
        if self.clock.over() {
            return None;
        }
        // The way VoiceOver reads a page: the text-marker range covering the element, then the
        // string for that range (measured in Safari, `browser-page-guest.py` `marker_text`).
        let range = parameterized(&self.el, "AXTextMarkerRangeForUIElement", &self.el)?;
        parameterized(&self.el, "AXStringForTextMarkerRange", &range)?.downcast::<CFString>().map(|s| s.to_string())
    }
}

fn perform(element: &Element, action: &'static str) -> i32 {
    let action = CFString::from_static_string(action);
    // SAFETY: a live element and a CFString action name.
    unsafe { AXUIElementPerformAction(element.as_CFTypeRef(), action.as_CFTypeRef()) }
}

impl MacBrowser {
    /// The web area of the page tab `tab` shows now, and what it shows. The tab is a window's
    /// selected tab button, or the window itself when it shows no tab bar (the numbering
    /// `BrowserHost::tabs` gives).
    fn page_of(&self, app: &BrowserApp, tab: u64) -> Option<(Element, View)> {
        let element = app_element(app)?;
        for window in windows_of(&element)? {
            let buttons = tab_buttons(&window);
            let (id, tabbed) = if buttons.is_empty() {
                (self.id_of(&window), false)
            } else {
                match buttons.iter().find(|b| is_selected(b)) {
                    Some(button) => (self.id_of(button), true),
                    None => continue,
                }
            };
            if id == tab {
                let area = window_area(&window)?;
                let url = text(&area, "AXURL").unwrap_or_default();
                return Some((area, View { url, tabbed }));
            }
        }
        None
    }
}

impl PageHost for MacBrowser {
    fn view(&self, app: &BrowserApp, tab: u64) -> Option<View> {
        self.page_of(app, tab).map(|(_, view)| view)
    }

    fn snapshot(&self, app: &BrowserApp, tab: u64) -> Option<Snapshot> {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        *kept = Kept::default();
        let (area, view) = self.page_of(app, tab)?;
        let clock = Rc::new(Clock { until: Instant::now() + READING_BUDGET, late: Cell::new(false) });
        let walked = walk(&Ax { el: area.clone(), clock: clock.clone() });
        let title = ["AXTitle", "AXDescription"].iter().filter_map(|a| text(&area, a)).find(|t| !t.trim().is_empty())
            .unwrap_or_default();
        *kept = Kept { controls: walked.controls.into_iter().map(|c| c.el).collect(), last: walked.last.map(|n| n.el) };
        Some(Snapshot { view, title, items: walked.items, cut: walked.cut || clock.late.get() })
    }

    fn press_item(&self, index: usize) -> Pressed {
        let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let Some(element) = kept.controls.get(index) else { return Pressed::Gone };
        match perform(element, "AXPress") {
            0 => Pressed::Done,
            INVALID_ELEMENT => Pressed::Gone,
            _ => Pressed::Unsure,
        }
    }

    fn scroll_to_end(&self) -> bool {
        let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        kept.last.as_ref().is_some_and(|last| perform(last, "AXScrollToVisible") == 0)
    }

    fn show_tab(&self, _: &BrowserApp, tab: u64) -> bool {
        // Only a tab button is pressed: a window standing for its one tab is already showing it.
        self.element_of(tab).filter(|el| text(el, "AXRole").as_deref() == Some("AXRadioButton"))
            .is_some_and(|button| perform(&button, "AXPress") == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A browser that is not running shows no page, and a reading that was never taken has
    /// nothing to press or scroll: nothing is started to find that out.
    #[test]
    fn a_browser_that_is_not_running_has_no_page_and_nothing_kept_to_press() {
        let absent = BrowserApp { bundle_id: "com.richos.no-such-browser".into(), name: "None".into() };
        let host = MacBrowser::default();
        assert_eq!(host.view(&absent, 0), None);
        assert_eq!(host.snapshot(&absent, 0), None);
        assert_eq!(host.press_item(0), Pressed::Gone);
        assert!(!host.scroll_to_end());
        assert!(!host.show_tab(&absent, 0), "a tab never read is never pressed");
    }
}
