//! The few macOS calls RichDoctor makes, through objc2's runtime only (as src-tauri does):
//! the login item (SMAppService, PRD §3: macOS 13+), the clipboard, the Mail compose sheet
//! (NSSharingService, PRD §7: a user-controlled compose, never a mailto with the whole report),
//! whether this Mac has any network at all (for "when you are online", PRD §2.6), and when the
//! person logged in (a quiet login launch shows no window, round 39 §1).
#![allow(clippy::missing_safety_doc)]

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use std::ffi::{CStr, CString};

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

unsafe fn ns_string(s: &str) -> Option<*mut AnyObject> {
    let c = CString::new(s.replace('\0', "")).ok()?;
    let class = AnyClass::get(c"NSString")?;
    let out: *mut AnyObject = msg_send![class, stringWithUTF8String: c.as_ptr()];
    (!out.is_null()).then_some(out)
}

unsafe fn utf8(s: *mut AnyObject) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let p: *const std::ffi::c_char = msg_send![s, UTF8String];
    (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------------------
// THE LOGIN ITEM
// ---------------------------------------------------------------------------------------

/// What macOS says about RichDoctor starting at login (`SMAppService.Status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Login {
    /// Registered and allowed: RichDoctor starts at login.
    Enabled,
    /// Not registered: it does not.
    Off,
    /// Registered, but the person must allow it in System Settings, Login Items.
    RequiresApproval,
    /// macOS cannot find the app to register (for example, it is not in an Applications folder).
    NotFound,
    /// The call is not available (older macOS).
    Unavailable,
}

impl Login {
    /// Whether the switch shows as on: only what macOS reports, never the request (round 39's
    /// unresolved point 1: "the switch should reflect the answer, not the request").
    pub fn on(self) -> bool {
        self == Login::Enabled
    }
    /// Whether macOS did not allow it (the switch's unavailable state and its sentence).
    pub fn denied(self) -> bool {
        matches!(self, Login::RequiresApproval | Login::NotFound | Login::Unavailable)
    }
}

fn main_app_service() -> Option<*mut AnyObject> {
    let class = AnyClass::get(c"SMAppService")?;
    // SAFETY: a documented class method on macOS 13+; nil-checked.
    let svc: *mut AnyObject = unsafe { msg_send![class, mainAppService] };
    (!svc.is_null()).then_some(svc)
}

pub fn login_status() -> Login {
    let Some(svc) = main_app_service() else { return Login::Unavailable };
    // SAFETY: `status` returns an NSInteger (SMAppServiceStatus).
    let status: isize = unsafe { msg_send![svc, status] };
    match status {
        1 => Login::Enabled,
        2 => Login::RequiresApproval,
        3 => Login::NotFound,
        _ => Login::Off,
    }
}

/// Register or unregister, then report what macOS now says.
pub fn set_login(on: bool) -> Login {
    let Some(svc) = main_app_service() else { return Login::Unavailable };
    let mut error: *mut AnyObject = std::ptr::null_mut();
    // SAFETY: documented methods taking an NSError** out-parameter.
    let _ok: Bool = unsafe {
        if on {
            msg_send![svc, registerAndReturnError: &mut error]
        } else {
            msg_send![svc, unregisterAndReturnError: &mut error]
        }
    };
    login_status()
}

// ---------------------------------------------------------------------------------------
// THE CLIPBOARD
// ---------------------------------------------------------------------------------------

/// Put `text` on the general pasteboard as plain text.
pub fn copy_text(text: &str) -> bool {
    objc2::rc::autoreleasepool(|_| unsafe {
        let Some(class) = AnyClass::get(c"NSPasteboard") else { return false };
        let pb: *mut AnyObject = msg_send![class, generalPasteboard];
        if pb.is_null() {
            return false;
        }
        let _: isize = msg_send![pb, clearContents];
        let (Some(s), Some(ty)) = (ns_string(text), ns_string("public.utf8-plain-text")) else { return false };
        let ok: Bool = msg_send![pb, setString: s, forType: ty];
        ok.as_bool()
    })
}

// ---------------------------------------------------------------------------------------
// EMAIL SUPPORT
// ---------------------------------------------------------------------------------------

fn compose_service() -> Option<*mut AnyObject> {
    let class = AnyClass::get(c"NSSharingService")?;
    // SAFETY: NSSharingServiceNameComposeEmail is the string "com.apple.share.Mail.compose".
    unsafe {
        let name = ns_string("com.apple.share.Mail.compose")?;
        let svc: *mut AnyObject = msg_send![class, sharingServiceNamed: name];
        (!svc.is_null()).then_some(svc)
    }
}

unsafe fn one_item_array(item: *mut AnyObject) -> Option<*mut AnyObject> {
    let class = AnyClass::get(c"NSArray")?;
    let a: *mut AnyObject = msg_send![class, arrayWithObject: item];
    (!a.is_null()).then_some(a)
}

/// Whether an email app is set up that can compose this.
pub fn mail_available() -> bool {
    objc2::rc::autoreleasepool(|_| unsafe {
        let Some(svc) = compose_service() else { return false };
        let Some(probe) = ns_string("RichDoctor") else { return false };
        let Some(items) = one_item_array(probe) else { return false };
        let ok: Bool = msg_send![svc, canPerformWithItems: items];
        ok.as_bool()
    })
}

/// Open a new email to `to` with `subject`, `body` and the files `attachments` (real
/// attachments: the approved screenshots) in the person's mail app. They read it and send it
/// themselves; nothing is sent here. Must run on the main thread.
pub fn compose_mail(to: &str, subject: &str, body: &str, attachments: &[std::path::PathBuf]) -> bool {
    objc2::rc::autoreleasepool(|_| unsafe {
        let Some(svc) = compose_service() else { return false };
        let (Some(to), Some(subject), Some(body)) = (ns_string(to), ns_string(subject), ns_string(body)) else { return false };
        let Some(recipients) = one_item_array(to) else { return false };
        let Some(array_class) = AnyClass::get(c"NSMutableArray") else { return false };
        let Some(url_class) = AnyClass::get(c"NSURL") else { return false };
        let items: *mut AnyObject = msg_send![array_class, array];
        let _: () = msg_send![items, addObject: body];
        for file in attachments {
            let Some(path) = ns_string(&file.to_string_lossy()) else { return false };
            let url: *mut AnyObject = msg_send![url_class, fileURLWithPath: path];
            if url.is_null() {
                return false;
            }
            let _: () = msg_send![items, addObject: url];
        }
        let _: () = msg_send![svc, setRecipients: recipients];
        let _: () = msg_send![svc, setSubject: subject];
        let ok: Bool = msg_send![svc, canPerformWithItems: items];
        if !ok.as_bool() {
            return false;
        }
        let _: () = msg_send![svc, performWithItems: items];
        true
    })
}

// ---------------------------------------------------------------------------------------
// IS THERE ANY NETWORK AT ALL
// ---------------------------------------------------------------------------------------

/// **CONFIRMED OFFLINE**: no network interface but loopback is up with an address. Only then
/// does RichDoctor say "when you are online"; a failure with an interface up is a connection
/// failure whose cause is not known.
pub fn confirmed_offline() -> bool {
    let mut addrs: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates a list we free with freeifaddrs.
    if unsafe { libc::getifaddrs(&mut addrs) } != 0 {
        return false;
    }
    let mut up = false;
    let mut cur = addrs;
    while !cur.is_null() {
        // SAFETY: walking the list getifaddrs returned.
        let ifa = unsafe { &*cur };
        let flags = ifa.ifa_flags as i32;
        let family = if ifa.ifa_addr.is_null() { 0 } else { unsafe { (*ifa.ifa_addr).sa_family as i32 } };
        if flags & libc::IFF_UP != 0 && flags & libc::IFF_RUNNING != 0 && flags & libc::IFF_LOOPBACK == 0 && (family == libc::AF_INET || family == libc::AF_INET6) {
            let name = unsafe { CStr::from_ptr(ifa.ifa_name) }.to_string_lossy();
            // Link-local-only tunnels (utun, awdl, llw) are not a way to the internet.
            if !name.starts_with("utun") && !name.starts_with("awdl") && !name.starts_with("llw") {
                up = true;
            }
        }
        cur = ifa.ifa_next;
    }
    // SAFETY: the list from getifaddrs above.
    unsafe { libc::freeifaddrs(addrs) };
    !up
}

// ---------------------------------------------------------------------------------------
// A QUIET LOGIN LAUNCH
// ---------------------------------------------------------------------------------------

/// When the person at the console logged in, from the login records (utmpx), in epoch millis.
pub fn console_login_millis() -> Option<u64> {
    let mut newest: Option<u64> = None;
    // SAFETY: the utmpx iteration API; entries are copied out before the next call.
    unsafe {
        libc::setutxent();
        loop {
            let e = libc::getutxent();
            if e.is_null() {
                break;
            }
            let e = &*e;
            if e.ut_type == libc::USER_PROCESS {
                let line = CStr::from_ptr(e.ut_line.as_ptr()).to_string_lossy();
                if line == "console" {
                    let t = e.ut_tv.tv_sec as u64 * 1000 + e.ut_tv.tv_usec as u64 / 1000;
                    newest = Some(newest.map_or(t, |n| n.max(t)));
                }
            }
        }
        libc::endutxent();
    }
    newest
}

/// Whether this launch is the login item's: macOS started RichDoctor within two minutes of the
/// person logging in. Such a launch shows only the menu bar item (round 39 §1). A person who
/// opens it from Spotlight in those two minutes gets the window anyway: opening it again
/// reaches the running copy, which shows the window.
pub fn launched_at_login(now: u64) -> bool {
    console_login_millis().is_some_and(|t| now.saturating_sub(t) < 120_000) && login_status() == Login::Enabled
}

#[allow(dead_code)]
pub fn string_of(o: *mut AnyObject) -> Option<String> {
    // SAFETY: callers pass an NSString or nil.
    unsafe { utf8(o) }
}
