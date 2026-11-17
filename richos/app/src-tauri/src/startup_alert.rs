//! WHAT THE PERSON SEES WHEN RICHOS CANNOT OPEN.
//!
//! =========================================================================================
//! THE DEFECT, IN ONE SENTENCE
//! =========================================================================================
//!
//!     Every way this application can fail before its window exists ended in a line on
//!     stderr, and a Finder or Dock launch has no stderr anybody will ever read.
//!
//! The whole of the user's experience was an icon that bounced once and stopped. Found by
//! `tom-opus-ui1` while auditing the affordance suite at `a8fe95f3`: the nine failure
//! strings reachable from `update_startup::prepare` were classified NOT-RENDERED, because
//! no surface exists that could render them.
//!
//! =========================================================================================
//! THE CLASS, NOT THE INSTANCE — TWELVE SITES, THREE SHAPES
//! =========================================================================================
//!
//! `prepare`'s `eprintln! + return` was the reported one. Repairing only that would leave
//! eleven siblings, so the enumeration is written down here and the fix is aimed at the
//! shape rather than at any member:
//!
//!   SHAPE 1 — an explicit early return, before Tauri exists at all (2 sites)
//!     main.rs:898-904   `update_startup::prepare` -> Err -> eprintln + `return`   <- reported
//!     main.rs:906-914   `--onboarding-mcp` -> Err -> eprintln + `exit(1)`
//!
//!   SHAPE 2 — a panic inside `setup`, which never reaches a usable window (4 sites)
//!     main.rs:1020      `LaunchStore::open(...).expect("open launch record")`   (pre-window)
//!     main.rs:1101      `Ledger::open(...).expect("open ledger")`
//!     main.rs:1138      `ConfigStore::open(...).expect("open config store")`
//!     main.rs:1256      `ensure_active_thread_in(...).expect("ensure thread")`
//!
//!   SHAPE 3 — `?` out of `setup`, or the builder itself (3 sites)
//!     main.rs:1035      `WebviewWindowBuilder::from_config(...)?`
//!     main.rs:1066      `.build()?`
//!     main.rs:1938      `.build(context).expect("error while building RichOS")`
//!
//!   And the three `.lock().expect(...)` sites in `updates.rs` (246, 327, 435/450/499/667/721
//!   are the same poisoned-mutex shape) reach the same hook once armed, because SHAPE 2 and
//!   SHAPE 3 are both *panics* and are caught in one place.
//!
//! SHAPE 2 and SHAPE 3 are ONE mechanism — a Rust panic unwinds `main`, the process exits
//! 101, and macOS shows nothing for a clean non-zero exit. Only SHAPE 1 needed a call site.
//! That is why the fix is a panic hook plus one function, and not twelve edits: a thirteenth
//! site added next month is covered without anybody remembering this file exists.
//!
//! `updates.rs:858`'s `std::process::exit(code)` is deliberately NOT in the class. It is the
//! `RICHOS_UPDATE_SELFTEST` harness's own exit, reached only when that variable is set, and
//! its audience is `app/scripts/updater-e2e.sh`'s stdout parser.
//!
//! =========================================================================================
//! WHAT WAS CHOSEN, AND WHAT WAS REJECTED
//! =========================================================================================
//!
//! CHOSEN: `CFUserNotificationDisplayAlert` — CoreFoundation, in-process, no new crate.
//!
//! It is the API macOS provides for exactly this position: a process that must speak to the
//! person at the machine before (or without) becoming a GUI application. It needs no
//! `NSApplication`, no run loop, no window server handshake of our own, and no subprocess.
//!
//! MEASURED, NOT ASSUMED, on this machine (Darwin 24.6.0) on 2026-09-10 before a line of
//! this file was written: a 12-line C probe calling it with a 12-second timeout put a real
//! system alert on screen — stop-sign icon, header, message, OK button — and returned
//! `rc=0 resp=3` (`kCFUserNotificationCancelResponse`, which is what a timeout returns).
//! Screenshot and probe source: `docs/verification/startup-alert-2026-09-10/`.
//!
//! REJECTED, each for a reason and not for taste:
//!
//!   * A TAURI ERROR WINDOW. It would require initializing the runtime whose prerequisite
//!     has just failed. Worse, it is specifically wrong for these failures: two of
//!     `prepare`'s errors — "Update activation needs recovery before the app can start" and
//!     "The activated application could not start" — mean the bundle under this loaded image
//!     may already have been exchanged. Running the whole webview stack against resources
//!     the code has just refused to trust is the thing `update_startup.rs:90-92` exists to
//!     prevent.
//!   * `osascript -e 'display alert ...'`. It works, and it forks a subprocess into the
//!     AppleScript stack at the one moment this process's environment is known to be broken.
//!     It also requires interpolating an arbitrary error string into an AppleScript literal;
//!     CoreFoundation takes the message as DATA and cannot be made to run it.
//!   * `NSAlert` through objc. Needs `NSApplication.shared` and a run loop — which is
//!     precisely what has not happened yet — and adds an objc dependency.
//!   * `rfd` / `tauri-plugin-dialog`. A new dependency in the one code path whose entire job
//!     is to work when other things are broken. `rfd`'s macOS backend is `NSAlert`, so it
//!     inherits the objection above as well.
//!   * A NOTIFICATION (`UNUserNotificationCenter`). Requires authorization the app may never
//!     have been granted, is silently dropped in a Focus mode, and is dismissible into a
//!     history the person does not know to open. A failure to start is not a notification.
//!   * STDERR PLUS "CHECK THE LOG". The status quo, and the defect.
//!
//! NOT macOS: `prepare` is `Ok(None)` off macOS and RichOS ships macOS only, so the alert
//! arm here is macOS only and every other platform gets the log plus stderr. THIS IS A
//! DECLARED GAP, NOT A FINISHED PORT — the Windows equivalent is `MessageBoxW` from
//! `user32` with `MB_ICONERROR | MB_SETFOREGROUND`, which has the same "works before any
//! window exists" property. It is NOT written here because it cannot be compiled on this
//! machine (`rustup target list --installed` reports `aarch64-apple-darwin` and nothing
//! else, measured 2026-09-10), and untested FFI in the failure path is worth less than an
//! honest gap. It sits with the other two open Windows items in the architecture doc §4.4
//! gap 5 (no Windows code-signing certificate) and §3.3 (WebView2 vs WebKit).
//!
//! =========================================================================================
//! WHEN THE ALERT IS ARMED — THE PROJECT'S OWN RULE, REUSED, NOT A SECOND ONE
//! =========================================================================================
//!
//! A modal alert on every failing boot would hang `gui-boot.test.sh`, `updater-e2e.sh` and
//! every harness written after them. The obvious fix is to detect a test — and `activation.rs`
//! already argues at length why that is the wrong shape and keeps the inverse rule instead:
//! B (in this build's own bundle), D (writing to the installed data directory), P (parent pid
//! is 1, because macOS hands a launch to launchd and a harness must hold what it boots).
//!
//! So the alert is armed by EXACTLY `activation::decide(...) == Presentation::Regular` and by
//! nothing else. A harness cannot be an installed launch by construction, so it never sees a
//! modal, and nobody has to add it to a list. The person double-clicking in Finder always is.
//!
//! Armed TWICE, deliberately, because the two moments know different amounts:
//!
//!   * `install()` runs before Tauri and can only approximate D — it computes the data
//!     directory Tauri's macOS `app_data_dir()` will return rather than reading it. B and P
//!     are exact there, and P is the condition that actually holds a harness out.
//!   * `rearm()` runs in `setup` from the AUTHORITATIVE decision, against the `data_dir` the
//!     boot really resolved. From that line on, the approximation is gone.
//!
//! And DISARMED at `boot complete`. After that the app owns a window and its own surfaces are
//! the right place for a failure; a modal system alert raised by a background thread that
//! panicked would be an interruption, not a rescue.
//!
//! =========================================================================================
//! THE DIAGNOSTIC IS NEVER TRADED FOR THE FRIENDLY SENTENCE
//! =========================================================================================
//!
//! Every report writes THREE ways and drops none of them:
//!
//!   1. `~/Library/Logs/RichOS/startup.log` — timestamped, with the version, the pid and the
//!      executable path. `~/Library/Logs` is where Console.app looks, so it is a place a
//!      person can be sent and an engineer can read.
//!   2. stderr, unchanged, still prefixed `[richos] ` so `gui-boot.test.sh`'s S1 accounting
//!      still sees it.
//!   3. the alert, which NAMES THE LOG'S PATH in its own text and offers a button that
//!      reveals the file in Finder.
//!
//! A panic keeps its default hook as well, so the payload, the location and any
//! `RUST_BACKTRACE` output still go to stderr exactly as they did before this file existed.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The one headline. Every member of the class fails the same way as far as the person is
/// concerned — the application he asked for did not open — so they say so in one voice.
pub const HEADLINE: &str = "RichOS could not open";

/// What a panic tells the person. A panic payload is an engineer's sentence and it stays in
/// the log; this is the CEO's, and it is fixed rather than composed because there is nothing
/// truthful and specific to say about an unexpected fault.
pub const PANIC_SENTENCE: &str = "RichOS ran into an unexpected problem while starting up and \
     closed itself rather than open a window that would not work.";

/// Ten minutes. `600.0` is long enough that stepping away from the desk does not lose the
/// message, and finite so a launch nobody is watching cannot leave a process waiting on a
/// dialog forever. `0.0` would mean "no timeout" and is deliberately not used.
const ALERT_TIMEOUT_SECONDS: f64 = 600.0;

/// 256 KiB. At the ~200 bytes a report costs, that is well over a thousand of them before
/// the file is rotated once — and this file only ever grows on a boot that FAILED.
const LOG_ROTATE_BYTES: u64 = 256 * 1024;

static ARMED: AtomicBool = AtomicBool::new(false);
static REPORTING: AtomicBool = AtomicBool::new(false);
static VERSION: OnceLock<String> = OnceLock::new();

// =========================================================================================
// ARMING
// =========================================================================================

/// Install the panic hook and arm from the facts available before Tauri exists.
///
/// Called as the first statement of `main` that can fail-and-speak. `generate_context!()`
/// runs one line earlier and is compile-generated construction with nothing to fail at run
/// time — named here rather than left to be discovered.
pub fn install(own_identifier: &str, compiled_version: &str) {
    let _ = VERSION.set(compiled_version.to_string());
    install_panic_hook();
    let data_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| crate::activation::installed_data_dir(&home, own_identifier))
        .unwrap_or_else(std::env::temp_dir);
    let facts = crate::activation::gather(&data_dir, own_identifier);
    rearm(crate::activation::decide(&facts).presentation);
}

/// Re-arm from the authoritative decision, once `setup` has one.
pub fn rearm(presentation: crate::activation::Presentation) {
    ARMED.store(
        presentation == crate::activation::Presentation::Regular,
        Ordering::SeqCst,
    );
}

/// The app has a window and has finished resolving. From here its own surfaces are the right
/// place for a failure, so the modal stands down.
pub fn disarm() {
    ARMED.store(false, Ordering::SeqCst);
}

/// Whether a failure right now would reach the person as an alert.
pub fn is_armed() -> bool {
    ARMED.load(Ordering::SeqCst)
}

// =========================================================================================
// REPORTING
// =========================================================================================

/// RichOS is about to stop before it ever showed the person anything. Say so on every
/// surface that exists at this moment.
///
/// `operator` is the engineer's sentence — it goes to stderr behind `[richos] ` and into the
/// log verbatim. `person` is the CEO's sentence, and it is the only one that reaches a
/// screen. Two audiences, two strings, one call, so neither can be dropped by forgetting.
pub fn cannot_start(operator: &str, person: &str) {
    // A failure while reporting a failure must not recurse. The second one still reaches the
    // log through the default panic hook's stderr.
    if REPORTING.swap(true, Ordering::SeqCst) {
        eprintln!("[richos] {operator}");
        return;
    }
    eprintln!("[richos] {operator}");
    let log = record(operator);
    if is_armed() {
        show_alert(HEADLINE, &person_message(person, log.as_deref()));
    }
    REPORTING.store(false, Ordering::SeqCst);
}

/// The alert's body: the person's sentence, then where the whole of it was written down.
///
/// Pure, so the wording is a unit test rather than a screenshot.
pub fn person_message(person: &str, log: Option<&Path>) -> String {
    let mut out = person.trim().to_string();
    match log {
        Some(path) => {
            out.push_str("\n\nThe full details were written here:\n");
            out.push_str(&path.display().to_string());
        }
        None => out.push_str(
            "\n\nRichOS could not write the details down either: there was nowhere on this \
             Mac it was able to write to.",
        ),
    }
    out
}

// =========================================================================================
// THE LOG
// =========================================================================================

/// `~/Library/Logs/RichOS/startup.log`, or a temporary directory when `HOME` is unreadable —
/// which is itself one of the failures this file reports, so it cannot be assumed.
pub fn log_path() -> PathBuf {
    log_path_under(std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

/// Pure half of [`log_path`].
pub fn log_path_under(home: Option<&Path>) -> PathBuf {
    match home {
        Some(home) => home.join("Library").join("Logs").join("RichOS").join("startup.log"),
        None => std::env::temp_dir().join("RichOS").join("startup.log"),
    }
}

/// The version this process was started as, for an entry in a log file.
pub(crate) fn version() -> &'static str {
    VERSION.get().map(String::as_str).unwrap_or("unknown")
}

/// One report, appended. Returns where it landed, or `None` when nowhere would take it.
fn record(operator: &str) -> Option<PathBuf> {
    let path = log_path();
    let parent = path.parent()?;
    std::fs::create_dir_all(parent).ok()?;
    rotate_if_large(&path);
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok()?;
    let entry = log_entry(
        richos_core::util::now_millis(),
        VERSION.get().map(String::as_str).unwrap_or("unknown"),
        std::process::id(),
        std::env::current_exe().ok().as_deref(),
        operator,
    );
    file.write_all(entry.as_bytes()).ok()?;
    file.flush().ok()?;
    Some(path)
}

/// The text of one log entry. Pure, so its shape is a test and not a thing to eyeball.
pub fn log_entry(
    millis: u64,
    version: &str,
    pid: u32,
    exe: Option<&Path>,
    operator: &str,
) -> String {
    format!(
        "==== {} RichOS {} pid {} ====\nexecutable: {}\nfailure: {}\n\n",
        utc_stamp(millis),
        version,
        pid,
        exe.map(|p| p.display().to_string()).unwrap_or_else(|| "unreadable".to_string()),
        operator.trim(),
    )
}

/// Keep one generation. A startup log only grows on boots that failed, so this is a bound
/// rather than a rotation policy.
pub(crate) fn rotate_if_large(path: &Path) {
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > LOG_ROTATE_BYTES {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
}

/// `1757493852000` -> `2026-09-10T08:44:12Z`.
///
/// Hand-rolled rather than a `chrono`/`time` dependency, for the reason this whole file is
/// dependency-free: the failure path is the one place a crate that fails to initialize costs
/// the most. It is Howard Hinnant's `civil_from_days`, which is exact for every date this
/// program can see, and it is a unit test below rather than a claim.
pub fn utc_stamp(millis: u64) -> String {
    let secs = millis / 1000;
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

// =========================================================================================
// THE PANIC HOOK — SHAPE 2 AND SHAPE 3, IN ONE PLACE
// =========================================================================================

/// Wrap the existing hook rather than replace it: the payload, the location and any
/// `RUST_BACKTRACE` output still reach stderr exactly as before, and THEN the person is told.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if !is_armed() && !cfg!(test) {
            // Not armed means either a harness, or the app is already up and owns its own
            // surfaces. Either way a modal is wrong — but the log still gets it.
            let _ = record(&panic_summary(info));
            return;
        }
        cannot_start(&panic_summary(info), PANIC_SENTENCE);
    }));
}

/// `panicked at 'open ledger', src/main.rs:1101:38`, flattened into the one line the log and
/// the `[richos] ` prefix want. Pure over the two things a `PanicHookInfo` actually carries.
pub fn panic_summary(info: &std::panic::PanicHookInfo<'_>) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic carrying no message".to_string());
    compose_panic_summary(&message, info.location().map(|l| (l.file(), l.line(), l.column())))
}

/// Pure half of [`panic_summary`].
pub fn compose_panic_summary(message: &str, at: Option<(&str, u32, u32)>) -> String {
    match at {
        Some((file, line, column)) => format!(
            "application startup: panicked at {file}:{line}:{column} — {}",
            message.replace('\n', " ")
        ),
        None => format!(
            "application startup: panicked with no location — {}",
            message.replace('\n', " ")
        ),
    }
}

// =========================================================================================
// THE NATIVE ALERT
// =========================================================================================

/// macOS. See the header for the measurement and for the five rejected alternatives.
///
/// CONTRAST: every color here is Apple's. `CFUserNotificationDisplayAlert` draws a system
/// alert with the system label colors on the system alert material, which meet WCAG AA in
/// both light and dark appearance by construction — the probe screenshot in
/// `docs/verification/startup-alert-2026-09-10/` was taken in dark appearance. Nothing in
/// this file selects a color, a size or a weight, and nothing here is exempt from the floor:
/// the floor is met by not overriding the system's own compliant palette.
#[cfg(target_os = "macos")]
fn show_alert(headline: &str, message: &str) {
    use std::os::raw::c_void;

    type CFStringRef = *const c_void;
    type CFURLRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    // `CFIndex` is `signed long`; `CFOptionFlags` is `unsigned long`. On every target this
    // ships to, those are 64-bit, which is what `isize`/`usize` are — the sizes are checked
    // by a test below rather than assumed.
    type CFIndex = isize;
    type CFOptionFlags = usize;

    const UTF8: u32 = 0x0800_0100;
    const STOP_ALERT_LEVEL: CFOptionFlags = 0;
    const ALTERNATE_RESPONSE: CFOptionFlags = 1;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithBytes(
            alloc: CFAllocatorRef,
            bytes: *const u8,
            num_bytes: CFIndex,
            encoding: u32,
            is_external_representation: u8,
        ) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
        #[allow(clippy::too_many_arguments)]
        fn CFUserNotificationDisplayAlert(
            timeout: f64,
            flags: CFOptionFlags,
            icon_url: CFURLRef,
            sound_url: CFURLRef,
            localization_url: CFURLRef,
            alert_header: CFStringRef,
            alert_message: CFStringRef,
            default_button_title: CFStringRef,
            alternate_button_title: CFStringRef,
            other_button_title: CFStringRef,
            response_flags: *mut CFOptionFlags,
        ) -> i32;
    }

    // A null allocator IS the default allocator in CoreFoundation, so this needs no extern
    // static. A string CF refuses to build comes back null, and CF treats a null title as
    // "no such button" rather than faulting — so the worst case degrades to a smaller alert.
    unsafe fn cf(s: &str) -> CFStringRef {
        CFStringCreateWithBytes(
            std::ptr::null(),
            s.as_ptr(),
            s.len() as CFIndex,
            UTF8,
            0,
        )
    }

    let show_details = log_path();

    unsafe {
        let header = cf(headline);
        let body = cf(message);
        let default_button = cf("OK");
        let alternate_button = cf("Show Details");
        let mut response: CFOptionFlags = 0;

        let rc = CFUserNotificationDisplayAlert(
            ALERT_TIMEOUT_SECONDS,
            STOP_ALERT_LEVEL,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            header,
            body,
            default_button,
            alternate_button,
            std::ptr::null(),
            &mut response,
        );

        for handle in [header, body, default_button, alternate_button] {
            if !handle.is_null() {
                CFRelease(handle);
            }
        }

        // "Show Details" reveals the file in Finder. `open -R` is the system's own reveal and
        // needs no permission; a failure to spawn it is not worth a second alert, because the
        // path is already in the message the person just read.
        if rc == 0 && response == ALTERNATE_RESPONSE {
            let _ = std::process::Command::new("/usr/bin/open")
                .arg("-R")
                .arg(&show_details)
                .spawn();
        }
    }
}

/// Off macOS the report is the log plus stderr, and the header says why that is a declared
/// gap rather than a finished port.
#[cfg(not(target_os = "macos"))]
fn show_alert(_headline: &str, _message: &str) {}

// =========================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_stamp_agrees_with_dates_computed_by_hand() {
        // 0 is the epoch itself.
        assert_eq!(utc_stamp(0), "1970-01-01T00:00:00Z");
        // 2026-09-10T08:44:12Z. Derived independently: 20707 whole days from the epoch to
        // 2026-09-10 (56 years x 365 = 20440, plus 14 leap days 1972..2024 inclusive = 20454
        // to 2026-01-01, plus 31+28+31+30+31+30+31+31 = 243 to 09-01, plus 9 = 20706 days
        // elapsed), so 20706 * 86400 + 8*3600 + 44*60 + 12 = 1789029852 seconds.
        assert_eq!(utc_stamp(1_789_029_852_000), "2026-09-10T08:44:12Z");
        // A leap day, which is where a hand-rolled calendar goes wrong if it is going to.
        assert_eq!(utc_stamp(1_709_164_800_000), "2024-02-29T00:00:00Z");
        // The last second before a century that is not a leap year would have needed one.
        assert_eq!(utc_stamp(4_102_444_799_000), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn the_message_names_the_log_so_the_person_can_be_sent_to_it() {
        let msg = person_message(
            "RichOS stopped while starting up.",
            Some(&PathBuf::from("/Users/x/Library/Logs/RichOS/startup.log")),
        );
        assert!(msg.starts_with("RichOS stopped while starting up."));
        assert!(
            msg.contains("/Users/x/Library/Logs/RichOS/startup.log"),
            "the path is the whole point of the second paragraph: {msg}"
        );
    }

    #[test]
    fn a_report_that_could_not_be_written_says_so_rather_than_naming_nothing() {
        let msg = person_message("RichOS stopped while starting up.", None);
        assert!(
            msg.contains("could not write the details down"),
            "a missing log is a fact the person is told, not a paragraph that vanishes: {msg}"
        );
        assert!(!msg.contains("written here"));
    }

    #[test]
    fn the_log_entry_carries_what_an_engineer_needs_afterwards() {
        let entry = log_entry(
            1_789_029_852_000,
            "1.0.3",
            4242,
            Some(&PathBuf::from("/Applications/RichOS.app/Contents/MacOS/richos-tauri")),
            "application startup: Could not establish application update exclusion: no lock",
        );
        assert!(entry.contains("2026-09-10T08:44:12Z"), "{entry}");
        assert!(entry.contains("RichOS 1.0.3"), "{entry}");
        assert!(entry.contains("pid 4242"), "{entry}");
        assert!(entry.contains("/Applications/RichOS.app/Contents/MacOS/richos-tauri"), "{entry}");
        assert!(entry.contains("Could not establish application update exclusion"), "{entry}");
        assert!(entry.ends_with("\n\n"), "entries are separated so a reader can see one: {entry:?}");
    }

    #[test]
    fn an_unreadable_executable_path_is_named_and_not_omitted() {
        let entry = log_entry(0, "1.0.3", 1, None, "x");
        assert!(entry.contains("executable: unreadable"), "{entry}");
    }

    #[test]
    fn a_panic_summary_keeps_the_location_and_flattens_to_one_line() {
        let summary = compose_panic_summary("open ledger", Some(("src/main.rs", 1101, 38)));
        assert_eq!(
            summary,
            "application startup: panicked at src/main.rs:1101:38 — open ledger"
        );
        let multiline = compose_panic_summary("first\nsecond", None);
        assert!(!multiline.contains('\n'), "{multiline}");
        assert!(multiline.contains("no location"), "{multiline}");
    }

    #[test]
    fn the_log_lands_where_console_app_looks_and_has_a_home_free_fallback() {
        assert_eq!(
            log_path_under(Some(&PathBuf::from("/Users/x"))),
            PathBuf::from("/Users/x/Library/Logs/RichOS/startup.log")
        );
        let fallback = log_path_under(None);
        assert!(fallback.ends_with("RichOS/startup.log"), "{}", fallback.display());
        assert!(fallback.starts_with(std::env::temp_dir()), "{}", fallback.display());
    }

    #[test]
    fn the_alert_is_armed_by_the_installed_launch_rule_and_by_nothing_else() {
        use crate::activation::Presentation;
        rearm(Presentation::Regular);
        assert!(is_armed(), "an installed launch has no other way to be told");
        rearm(Presentation::Accessory);
        assert!(!is_armed(), "a harness must never wait on a modal");
        rearm(Presentation::Regular);
        disarm();
        assert!(!is_armed(), "once the window is up, the app's own surfaces own the failure");
    }

    #[test]
    fn the_corefoundation_scalar_types_are_the_widths_the_abi_declares() {
        // `CFIndex` is `signed long` and `CFOptionFlags` is `unsigned long`; the aliases in
        // `show_alert` spell them `isize`/`usize`. If a target ever makes those disagree, the
        // alert would read a truncated response flag rather than fail loudly, so it is
        // checked here instead of assumed.
        assert_eq!(std::mem::size_of::<isize>(), 8);
        assert_eq!(std::mem::size_of::<usize>(), 8);
    }
}
