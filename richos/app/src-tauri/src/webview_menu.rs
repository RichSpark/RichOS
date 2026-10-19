//! **THE WEB VIEW'S OWN RIGHT-CLICK MENU, AT THE ONE CALL EVERY ONE OF THEM GOES THROUGH** (CEO,
//! nightly 65, 2026-10-19, about Look Up, Translate, Search with Google, Share, Speech and
//! Services: "When will all shit of this kind be gone from the right-click events?").
//!
//! `ui/no-native-menu.js` cancels the menu inside the page. Two places are out of its reach
//! (review rv-20261019T004500Z-bde5e76f-77af):
//!
//!  * a text field, where the page lets WebKit's menu through for Cut, Copy and Paste, and that
//!    menu also carries Look Up, Translate, Search with Google, Spelling and Grammar,
//!    Substitutions, Transformations, Font, Speech, Paragraph Direction, Selection Direction and
//!    Share;
//!  * the PDF preview in the Output panel, whose WebKit PDF viewer builds its own menu (Search with
//!    Google, Look Up, Copy, Open with Preview, zoom, page layout, next and previous page) where no
//!    page script reaches.
//!
//! Both menus are shown by the same AppKit call, `+[NSMenu popUpContextMenu:withEvent:forView:]`:
//! WebKit's page menu with the WKWebView as the view, the PDF viewer's with the window's content
//! view, which holds the WKWebView. Both measured on macOS 15.6 by
//! `examples/context_menu_probe.rs`, which records the menus WebKit hands to that call. So that
//! call is replaced, once per process and before any window exists ([`install`], first thing in
//! `main`, which covers the app and the dictation tool alike), and decides:
//!
//!  * over a web view, a menu WebKit built for a text field (it holds WebKit's own Paste) becomes
//!    exactly Cut, Copy, Paste and Select All: AppKit's standard edit actions, sent to that web
//!    view, which enables each one the way it enables the Edit menu's, with AppKit's own Services
//!    and Writing Tools additions switched off on it;
//!  * over a web view, any other menu is not shown at all;
//!  * a menu over anything else is shown as it was asked for.

/// The menu that replaces WebKit's in a text field: each title with AppKit's own edit action.
pub const EDITING: [(&str, &str); 4] =
    [("Cut", "cut:"), ("Copy", "copy:"), ("Paste", "paste:"), ("Select All", "selectAll:")];

/// The identifier WebKit gives its Paste item (`_WKMenuItemIdentifierPaste` in
/// `WKMenuItemIdentifiers.mm`). WebKit offers Paste only where the click landed in a field the
/// user types in, so its presence is what marks a text field's menu.
pub const WEBKIT_PASTE: &str = "WKMenuItemIdentifierPaste";

/// What becomes of a menu shown over a web view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// A text field's: only [`EDITING`].
    Editing,
    /// Anything else WebKit built: nothing is shown.
    Nothing,
}

/// The plan for a menu over a web view, from its items' identifiers.
pub fn plan<I, S>(identifiers: I) -> Plan
where
    I: IntoIterator<Item = Option<S>>,
    S: AsRef<str>,
{
    if identifiers.into_iter().flatten().any(|id| id.as_ref() == WEBKIT_PASTE) {
        Plan::Editing
    } else {
        Plan::Nothing
    }
}

/// Replace the menu call. Once per process, before any window; later calls do nothing.
pub fn install() {
    #[cfg(target_os = "macos")]
    mac::install();
}

/// How many menus over a web view were not shown in this process (the probe's evidence that a
/// menu was offered and dropped, rather than never offered).
#[allow(dead_code)]
pub fn dropped() -> usize {
    #[cfg(target_os = "macos")]
    return mac::DROPPED.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_os = "macos"))]
    0
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{plan, Plan, EDITING};
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::sel;
    use std::ffi::{c_char, CStr, CString};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::OnceLock;

    /// `+[NSMenu popUpContextMenu:withEvent:forView:]`, as the runtime calls it.
    type PopUp =
        unsafe extern "C-unwind" fn(*const AnyClass, Sel, *mut AnyObject, *mut AnyObject, *mut AnyObject);

    /// AppKit's own implementation, kept to show the menus that are shown.
    static ORIGINAL: OnceLock<Option<PopUp>> = OnceLock::new();
    pub(super) static DROPPED: AtomicUsize = AtomicUsize::new(0);

    pub fn install() {
        ORIGINAL.get_or_init(|| {
            let (Some(menu), Some(_)) = (AnyClass::get(c"NSMenu"), AnyClass::get(c"WKWebView")) else {
                eprintln!("[richos] right-click menu: AppKit or WebKit is not loaded; the page's own guard is all there is");
                return None;
            };
            let Some(method) = menu.class_method(sel!(popUpContextMenu:withEvent:forView:)) else {
                eprintln!("[richos] right-click menu: NSMenu has no popUpContextMenu:withEvent:forView:; the page's own guard is all there is");
                return None;
            };
            let ours: PopUp = pop_up;
            // SAFETY: `pop_up` has the class method's exact signature (class, selector, NSMenu,
            // NSEvent, NSView; no result) and hands every call it does not drop to the
            // implementation it replaced, with the same arguments.
            let previous = unsafe { method.set_implementation(std::mem::transmute::<PopUp, Imp>(ours)) };
            // SAFETY: the implementation just replaced has that same signature.
            Some(unsafe { std::mem::transmute::<Imp, PopUp>(previous) })
        });
    }

    unsafe extern "C-unwind" fn pop_up(
        class: *const AnyClass,
        cmd: Sel,
        menu: *mut AnyObject,
        event: *mut AnyObject,
        view: *mut AnyObject,
    ) {
        let show = objc2::rc::autoreleasepool(|_| decide(menu, view));
        if !show {
            DROPPED.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if let Some(Some(original)) = ORIGINAL.get() {
            // SAFETY: AppKit's own implementation, called with the arguments it was called with.
            unsafe { original(class, cmd, menu, event, view) }
        }
    }

    /// Whether `menu` is shown, after making it the editing menu where it is one.
    fn decide(menu: *mut AnyObject, view: *mut AnyObject) -> bool {
        if menu.is_null() {
            return true;
        }
        let Some(web_view) = web_view_at(view) else { return true };
        match plan(identifiers(menu)) {
            Plan::Editing => {
                editing_menu(menu, web_view);
                true
            }
            Plan::Nothing => {
                opened_and_closed(menu);
                false
            }
        }
    }

    /// The WKWebView the menu is shown over: the view itself (WebKit's page menu), or the one
    /// directly inside it (the PDF viewer's menu, shown over the window's content view).
    fn web_view_at(view: *mut AnyObject) -> Option<*mut AnyObject> {
        let class = AnyClass::get(c"WKWebView")?;
        if view.is_null() {
            return None;
        }
        // SAFETY: documented NSObject and NSView methods on the view AppKit was handed; main thread.
        unsafe {
            let is: Bool = msg_send![view, isKindOfClass: class];
            if is.as_bool() {
                return Some(view);
            }
            let subviews: *mut AnyObject = msg_send![view, subviews];
            if subviews.is_null() {
                return None;
            }
            let n: usize = msg_send![subviews, count];
            (0..n).map(|i| -> *mut AnyObject { msg_send![subviews, objectAtIndex: i] }).find(|&v| {
                let is: Bool = msg_send![v, isKindOfClass: class];
                is.as_bool()
            })
        }
    }

    fn identifiers(menu: *mut AnyObject) -> Vec<Option<String>> {
        // SAFETY: documented NSMenu, NSMenuItem and NSString accessors; every object is checked for
        // nil, and each string is copied before the pool around `decide` drains.
        unsafe {
            let items: *mut AnyObject = msg_send![menu, itemArray];
            if items.is_null() {
                return Vec::new();
            }
            let n: usize = msg_send![items, count];
            (0..n)
                .map(|i| {
                    let item: *mut AnyObject = msg_send![items, objectAtIndex: i];
                    let id: *mut AnyObject = msg_send![item, identifier];
                    if id.is_null() {
                        return None;
                    }
                    let utf8: *const c_char = msg_send![id, UTF8String];
                    (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
                })
                .collect()
        }
    }

    /// Empty `menu` and give it [`EDITING`], each item sent to `web_view`, with AppKit's own
    /// additions switched off: system-wide Services (`allowsContextMenuPlugIns`) and Writing Tools
    /// (`automaticallyInsertsWritingToolsItems`, macOS 15.2 and later), both on by default
    /// (NSMenu.h), which AppKit would otherwise add to selected text after this call hands it on.
    fn editing_menu(menu: *mut AnyObject, web_view: *mut AnyObject) {
        let Some(string) = AnyClass::get(c"NSString") else { return };
        // SAFETY: documented NSMenu, NSMenuItem and NSString methods on the menu AppKit was handed;
        // main thread. `addItemWithTitle:action:keyEquivalent:` returns the item the menu owns.
        // The Writing Tools setter is sent only where the menu has it.
        unsafe {
            let () = msg_send![menu, removeAllItems];
            let () = msg_send![menu, setAllowsContextMenuPlugIns: Bool::NO];
            let has_writing_tools: Bool =
                msg_send![menu, respondsToSelector: sel!(setAutomaticallyInsertsWritingToolsItems:)];
            if has_writing_tools.as_bool() {
                let () = msg_send![menu, setAutomaticallyInsertsWritingToolsItems: Bool::NO];
            }
            let none: *mut AnyObject = msg_send![string, string];
            for (title, action) in EDITING {
                let (Ok(title), Ok(action)) = (CString::new(title), CString::new(action)) else { continue };
                let title: *mut AnyObject = msg_send![string, stringWithUTF8String: title.as_ptr()];
                let action = Sel::register(&action);
                let item: *mut AnyObject =
                    msg_send![menu, addItemWithTitle: title, action: action, keyEquivalent: none];
                if !item.is_null() {
                    let () = msg_send![item, setTarget: web_view];
                }
            }
        }
    }

    /// A menu that is not shown still tells its delegate it opened and closed, so WebKit, which
    /// waits for that on its page menus, is ready for the next right-click.
    fn opened_and_closed(menu: *mut AnyObject) {
        // SAFETY: NSMenuDelegate's two optional methods, sent only when the delegate has them.
        unsafe {
            let delegate: *mut AnyObject = msg_send![menu, delegate];
            if delegate.is_null() {
                return;
            }
            let opens: Bool = msg_send![delegate, respondsToSelector: sel!(menuWillOpen:)];
            if opens.as_bool() {
                let () = msg_send![delegate, menuWillOpen: menu];
            }
            let closes: Bool = msg_send![delegate, respondsToSelector: sel!(menuDidClose:)];
            if closes.as_bool() {
                let () = msg_send![delegate, menuDidClose: menu];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifiers of the menu WebKit built for selected text in a field, in order, as the
    /// probe recorded them on macOS 15.6 (Look Up, Translate, Search with Google, Cut, Copy, Paste,
    /// Spelling and Grammar, Substitutions, Transformations, Font, Speech, Paragraph Direction,
    /// Selection Direction, Share; separators carry none).
    const FIELD: [Option<&str>; 17] = [
        Some("WKMenuItemIdentifierLookUp"),
        Some("WKMenuItemIdentifierTranslate"),
        Some("WKMenuItemIdentifierSearchWeb"),
        None,
        Some("forwardContextMenuAction:"),
        Some("WKMenuItemIdentifierCopy"),
        Some("WKMenuItemIdentifierPaste"),
        None,
        Some("WKMenuItemIdentifierSpellingMenu"),
        None,
        None,
        None,
        Some("WKMenuItemIdentifierSpeechMenu"),
        None,
        None,
        None,
        Some("WKMenuItemIdentifierShareMenu"),
    ];

    #[test]
    fn a_text_fields_menu_becomes_the_editing_menu() {
        assert_eq!(plan(FIELD), Plan::Editing);
        let titles: Vec<&str> = EDITING.iter().map(|(title, _)| *title).collect();
        assert_eq!(titles, ["Cut", "Copy", "Paste", "Select All"]);
    }

    #[test]
    fn selected_page_text_and_the_pdf_viewer_get_no_menu() {
        // Selected text outside a field: Look Up, Translate, Search with Google, Copy, Speech, Share.
        let page = [
            Some("WKMenuItemIdentifierLookUp"),
            Some("WKMenuItemIdentifierTranslate"),
            Some("WKMenuItemIdentifierSearchWeb"),
            None,
            Some("WKMenuItemIdentifierCopy"),
            None,
            Some("WKMenuItemIdentifierSpeechMenu"),
            Some("WKMenuItemIdentifierShareMenu"),
        ];
        assert_eq!(plan(page), Plan::Nothing);
        // The PDF viewer's items carry AppKit's identifier from their action, never WebKit's.
        let pdf = [Some("contextMenuAction:"), None, Some("contextMenuAction:"), None];
        assert_eq!(plan(pdf), Plan::Nothing);
        assert_eq!(plan(Vec::<Option<String>>::new()), Plan::Nothing);
    }
}
