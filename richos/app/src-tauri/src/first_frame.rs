//! THE SPLASH'S FIRST FRAME ON THE GLASS AT ONCE — Sage's plan
//! `richos-hq/docs/plans/2026-11-14-splash-under-half-a-second.md`, rules 1 to 3, as the CEO
//! narrowed it the same day: "WHY are there first frame options needed in the first place??? We
//! have the splash screen already." The splash stays exactly as it is; its first frame is its own
//! ground color, and that is what reaches the screen at once.
//!
//! The CEO, 2026-11-14: "THE JOB IS NOT ABOUT ONE FRAME. THE JOB AT HAND IS TO START SHOWING OUR
//! ALREADY WELL-DESIGNED AND FULLY-BUILT SPLASH SCREEN IN WELL UNDER 500 MS AFTER BUTTON CLICK."
//! Native timing and the page's first-paint acknowledgement measure different
//! stages. The opening uses the chosen artwork and starts before WebKit. The
//! window's ground keeps it dark while the picture or page prepares.
//!
//! Present the initial window before other startup work. AppKit commits the
//! window during its normal display transaction. Forcing a synchronous redraw
//! from Tao's setup callback can reenter its event-loop lock.
//!
//! # WHAT WAITS FOR THE PAGE (rule 3)
//!
//! Work that starts threads or programs competing for the processor in the same fraction of a
//! second is not started before the first frame; it is handed to [`after_page`] and starts when the
//! page reports its first paint ([`page_took_over`]) or, if the page never reports, at [`CEILING`].
use std::sync::Mutex;
use std::time::Duration;

/// Work handed to [`after_page`] starts at the latest this long after the first frame.
pub const CEILING: Duration = Duration::from_secs(5);

/// The splash ground, `#0C1322`, in both splash entries of `ui/splash-library.js`, and the app's
/// own dark ground (`ui/style.css` `--ground`).
pub const SPLASH_GROUND: (u8, u8, u8) = (0x0C, 0x13, 0x22);
/// The app's light ground (`ui/style.css`, the light `--ground`), for a launch with no splash when
/// the saved theme is light.
pub const APP_GROUND_LIGHT: (u8, u8, u8) = (0xEA, 0xE6, 0xDD);

/// The window's background for this launch: the splash's ground when the splash will play, else
/// the app's ground for the saved theme. The system theme cannot be asked this early without
/// AppKit, so "system" falls to the dark ground, which is also the splash's.
pub fn ground(splash: bool, light_theme: bool) -> tauri::window::Color {
    let (r, g, b) = if !splash && light_theme { APP_GROUND_LIGHT } else { SPLASH_GROUND };
    tauri::window::Color(r, g, b, 255)
}

/// The window's background when the splash plays: the picked splash's own ground
/// (`splash_pick`), so a splash added later on another ground is never shown on the wrong one.
pub fn color((r, g, b): (u8, u8, u8)) -> tauri::window::Color {
    tauri::window::Color(r, g, b, 255)
}

type Job = Box<dyn FnOnce() + Send>;
/// `None` until the page took over; then the jobs have run and later ones run at once.
static WAITING: Mutex<Option<Vec<Job>>> = Mutex::new(Some(Vec::new()));

/// Start `job` once the page has taken over the splash (or at the ceiling), on the main thread,
/// where everything the hook started has always started.
pub fn after_page<R: tauri::Runtime>(app: &tauri::AppHandle<R>, job: impl FnOnce() + Send + 'static) {
    let mut waiting = WAITING.lock().unwrap_or_else(|p| p.into_inner());
    match waiting.as_mut() {
        Some(jobs) => jobs.push(Box::new(job)),
        None => {
            drop(waiting);
            if let Err(error) = app.run_on_main_thread(job) {
                eprintln!("[richos] first frame: deferred start-up work could not reach the main thread ({error})");
            }
        }
    }
}

/// The page painted its splash (or the ceiling came): start what waited, once. True the first time.
pub fn page_took_over<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let jobs = WAITING.lock().unwrap_or_else(|p| p.into_inner()).take();
    let Some(jobs) = jobs else { return false };
    if let Err(error) = app.run_on_main_thread(move || jobs.into_iter().for_each(|job| job())) {
        eprintln!("[richos] first frame: deferred start-up work could not reach the main thread ({error})");
    }
    true
}

/// The ceiling: if the page has not reported within [`CEILING`] of the first frame, take over
/// for it, so nothing that waited for it waits forever.
pub fn arm_ceiling<R: tauri::Runtime>(app: tauri::AppHandle<R>, on_ceiling: impl FnOnce() + Send + 'static) {
    let started = std::thread::Builder::new().name("richos-first-frame-ceiling".into()).spawn(move || {
        std::thread::sleep(CEILING);
        if page_took_over(&app) {
            eprintln!("[richos] first frame: the page did not report its splash within {} s; deferred start-up work started anyway",
                      CEILING.as_secs());
            on_ceiling();
        }
    });
    if let Err(error) = started {
        eprintln!("[richos] first frame: no ceiling thread ({error}); deferred work waits for the page");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE CLOSED LIST (plan rule 3). Every qualified call between the top of `main` and the
    /// Tauri start, and in the setup hook up to `launch_timing::mark_first_frame`: what may run before the
    /// splash's first frame is on the glass. The helper modes at the top of `main` exit before any
    /// window; the rest is the update step, the start-up alert, the data folder, the launch
    /// decision and record, the window's size, place, settings and background, and the window.
    const BEFORE_THE_FIRST_FRAME: &[&str] = &[
        "ConfigStore::open", "LaunchStore::open", "Mutex::new", "Path::new", "activation::decide",
        "activation::gather", "dictation::tool::main", "first_frame::ground", "launch_timing::mark_update_step",
        "launch_timing::mark_window", "linkedin_host::MacBrowser::default", "phone::connect::supervisor::guard_main",
        "richos_core::assignment_tools::run_stdio",
        // The team's browser reader (`--browser-mcp`): a helper mode that exits before any window,
        // like the LinkedIn reader's.
        "richos_core::browser_tools::run_stdio",
        "richos_core::git_name::redo_cli", "richos_core::git_name::wait_cli", "richos_core::linkedin_tools::run_stdio",
        "richos_core::question_tools::run_stdio", "richos_core::quota::gate::run_cli",
        "richos_core::quota::reset_tools::run_stdio", "richos_core::quota::terminal::run",
        "richos_core::rich_files::run_stdio", "richos_core::runtime::remember_verified_at",
        "richos_core::search_tools::run_stdio", "richos_core::status_tools::run_stdio",
        // The saved pick and its opening's first picture (Sage's plan 2026-11-14, "the splash the
        // rules picked is playing within 380 ms of the click", section 5 item 5).
        "splash_pick::pick_for", "splash_pick::parse_hex", "splash_player::reduce_motion", "splash_player::locate", "splash_player::prepare", "first_frame::color",
        // WHAT THE PICK AND THE SPLASH'S OWN PAGE READ (round 74, the CEO's order of 2026-11-14):
        // his settings for the opening, what he has earned by this start (one write, only when
        // something is new), the facts the pick reads, and the record the page draws his opening
        // from. The launch-speed gate measures the start with these in it.
        "openings::Settings::default", "openings::sync", "openings::splash_facts", "openings::page_record",
        "richos_core::status_tools::run_stdio_with_meeting", "richos_core::system_resources::read",
        "richos_core::system_resources::run_stdio", "richos_core::util::now_millis", "serde_json::to_string",
        "startup_alert::cannot_start", "startup_alert::install", "startup_alert::rearm", "std::env::args",
        "std::env::args_os", "std::env::temp_dir", "std::ffi::OsStr::new", "std::fs::create_dir_all",
        "std::io::Error::other", "std::io::stdin", "std::io::stdout", "std::process::exit", "std::process::id",
        "tauri::WebviewWindowBuilder::from_config", "update_relaunch::at_quit_main", "update_relaunch::helper_main", "update_startup::identity_probe",
        "update_startup::launch_report", "update_startup::prepare", "webview_menu::install",
        "window_geometry::GeometryStore::new", "window_geometry::decide_with",
    ];

    /// What must never come back in front of the first frame, by name, qualified or not: a check
    /// of the app or the engine, a walk over files, a thread or program started early.
    const NEVER_BEFORE_THE_FIRST_FRAME: &[&str] = &[
        "verify_engine", "EngineRuntime::load", "tree_hash", "recheck_", "resolve_engine",
        "login_environment::warm", "richdoctor_install::start", "fetch_rich_voice_in_background",
        "Monitor::start", "dictation_app::boot", "spawn_late_full_checks", "std::thread::spawn",
    ];

    fn qualified_calls(code: &str) -> std::collections::BTreeSet<String> {
        let mut found = std::collections::BTreeSet::new();
        let bytes = code.as_bytes();
        let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let mut i = 0;
        while i < bytes.len() {
            if (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') && (i == 0 || !ident(bytes[i - 1])) {
                let start = i;
                while i < bytes.len() && (ident(bytes[i]) || (bytes[i] == b':' && bytes.get(i + 1) == Some(&b':'))) {
                    i += if bytes[i] == b':' { 2 } else { 1 };
                }
                let path = &code[start..i];
                let mut j = i;
                while j < bytes.len() && bytes[j] == b' ' { j += 1; }
                if path.contains("::") && !path.ends_with("::") && bytes.get(j) == Some(&b'(') {
                    found.insert(path.to_string());
                }
            } else {
                i += 1;
            }
        }
        found
    }

    /// Comments out: a line's text from a `//` that starts the line or follows a space.
    fn without_comments(text: &str) -> String {
        text.lines()
            .map(|line| {
                let cut = line.match_indices("//").find(|(at, _)| *at == 0 || line.as_bytes()[at - 1] == b' ');
                cut.map_or(line, |(at, _)| &line[..at])
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn nothing_runs_before_the_first_frame_but_the_closed_list() {
        let main = std::fs::read_to_string("src/main.rs").expect("run from src-tauri/ (cargo test does)");
        let top = main.find("fn main() {").expect("fn main");
        let tauri = top + main[top..].find("tauri::Builder::default()").expect("the Tauri start");
        let hook = main.find(".setup(|app| {").expect("the setup hook");
        let window = hook + main[hook..].find("launch_timing::mark_first_frame(kind == LaunchKind::Fresh);").expect("first window timing in the hook");
        let before = without_comments(&main[top..tauri]) + "\n" + &without_comments(&main[hook..window]);
        let found = qualified_calls(&before);
        let allowed: std::collections::BTreeSet<String> = BEFORE_THE_FIRST_FRAME.iter().map(|s| s.to_string()).collect();
        let added: Vec<_> = found.difference(&allowed).collect();
        assert!(added.is_empty(),
                "a call was added in front of the splash's first frame: {added:?}. Before the first frame runs only \
                 what that frame needs (richos-hq docs/plans/2026-11-14-splash-under-half-a-second.md, rule 3); \
                 start anything else with first_frame::after_page, or after opening the window.");
        for name in NEVER_BEFORE_THE_FIRST_FRAME {
            assert!(!before.contains(name), "{name} runs in front of the splash's first frame (rule 3)");
        }
        let window_setup = &main[hook..window];
        assert!(window_setup.contains(".background_color(match window_splash {")
            && window_setup.contains("first_frame::color(splash_pick::parse_hex(&pick.ground)")
            && window_setup.contains("first_frame::ground("));
        assert!(window_setup.contains("start_native_window("), "a ready native pack uses the native first window");
        let native = main.find("fn start_native_window(").expect("the native window path");
        let native = &main[native..];
        let overlay = native.find("splash_player::show_on_window").expect("the native layer attaches");
        let visible = native.find("splash_player::window_visible").expect("the native window starts playback");
        let ready = native.find("startup_ready.await").expect("Home waits for application state");
        let webview = native.find(".add_child(").expect("the page joins the window later");
        assert!(overlay < visible && visible < ready && ready < webview,
            "start the native layer before waiting for application state and creating WebKit");
        assert!(!window_setup.contains("displayIfNeeded") && !native[..webview].contains("displayIfNeeded"),
            "a synchronous AppKit redraw can reenter Tao setup");
    }

    #[test]
    fn the_window_is_never_white_and_carries_the_splash_ground_when_the_splash_plays() {
        assert_eq!(ground(true, false), tauri::window::Color(0x0C, 0x13, 0x22, 255));
        assert_eq!(ground(true, true), tauri::window::Color(0x0C, 0x13, 0x22, 255), "the splash is always dark");
        assert_eq!(ground(false, true), tauri::window::Color(0xEA, 0xE6, 0xDD, 255));
        assert_eq!(ground(false, false), tauri::window::Color(0x0C, 0x13, 0x22, 255));
        // Every catalog entry has a ground the window can carry (the saved pick brings its own,
        // `first_frame::color`), and the library's two are the splash ground this file names. An
        // opening built on a material has that material's ground (round 74: blued steel is darker).
        let catalog = crate::splash_pick::catalog();
        assert!(!catalog.is_empty() && catalog.iter().all(|e| crate::splash_pick::parse_hex(&e.ground).is_some()));
        let library: Vec<_> = catalog.iter().filter(|e| !e.id.starts_with(richos_core::openings::KEY_PREFIX)).collect();
        assert_eq!(library.len(), 2);
        assert!(library.iter().all(|e| crate::splash_pick::parse_hex(&e.ground) == Some(SPLASH_GROUND)));
        assert_eq!(color((0x07, 0x0C, 0x17)), tauri::window::Color(0x07, 0x0C, 0x17, 255), "the window carries the pick's own ground");
        assert_eq!(color(SPLASH_GROUND), ground(true, true));
        let style = std::fs::read_to_string("../ui/style.css").unwrap();
        assert!(style.contains("--ground: #0c1322;") && style.contains("--ground: #eae6dd;"), "the app's grounds moved");
    }
}
