//! THE LAUNCH LINE — Sage's plan `richos-hq/docs/plans/2026-11-14-splash-under-half-a-second.md`,
//! rule 5 (and the launch-speed audit's piece 4).
//!
//! Nothing measured launch time, which is how a 2-second check went back in front of the window on
//! 2026-10-07 and shipped in every build after it, and how a 4-second launch on the CEO's Mac had
//! to be timed by hand. So every launch writes ONE line, when the page takes over the splash or at
//! the ceiling, with its own milestones counted from the moment macOS started the process:
//!
//! ```text
//! [richos] launch timing: update step 15 ms, window 210 ms, first frame 214 ms, start-up finished
//! 330 ms, page took over 420 ms after the process started; splash shown; update put in place: no;
//! bundle check none (trusted as written at its update), 1 ms; engine check none (trusted as
//! verified when it was written), 3 ms; process started at 1763112345678
//! ```
//!
//! The process start is the kernel's own (`proc_pidinfo`), and it survives an update hand-over:
//! the copy that hands over and the copy that ends up running are one process (`exec`), so the
//! line covers the old copy's time too, and says what that copy reported of its own check
//! (`update_startup::HANDOVER_ENV`; empty from a copy too old to say). The nightly's launch-speed
//! gate (`scripts/testvm/launch-speed-walk.py`) reads it.
use std::sync::Mutex;

#[derive(Default, Clone)]
struct Marks {
    update_step: Option<u64>,
    window: Option<u64>,
    first_frame: Option<u64>,
    finished: Option<u64>,
    page: Option<u64>,
    splash: Option<bool>,
    report: crate::update_startup::LaunchReport,
    engine: Option<String>,
    /// The splash this start shows, whether it was the pick saved at the last quit, and its
    /// opening: `Ok(later pictures)` when it played, `Err(why not)`.
    splash_id: Option<String>,
    splash_saved: bool,
    opening: Option<Result<usize, String>>,
    /// The moment of the splash the page joined the opening at, ms after the first frame.
    joined: Option<f64>,
    written: bool,
}

static MARKS: Mutex<Option<Marks>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Marks) -> T) -> T {
    let mut held = MARKS.lock().unwrap_or_else(|p| p.into_inner());
    f(held.get_or_insert_with(Marks::default))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// When the kernel started this process, in milliseconds since the Unix epoch.
#[cfg(target_os = "macos")]
pub fn process_started_ms() -> Option<u64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: `info` is a writable `proc_bsdinfo` of exactly `size` bytes.
    let written = unsafe {
        libc::proc_pidinfo(
            std::process::id() as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (written == size).then(|| info.pbi_start_tvsec * 1000 + info.pbi_start_tvusec / 1000)
}
#[cfg(not(target_os = "macos"))]
pub fn process_started_ms() -> Option<u64> {
    None
}

fn since_start() -> Option<u64> {
    process_started_ms().map(|start| now_ms().saturating_sub(start))
}

/// The update step is done (`update_startup::prepare` returned, in the copy that runs).
pub fn mark_update_step(report: crate::update_startup::LaunchReport) {
    let at = since_start();
    with(|m| {
        m.update_step = at;
        m.report = report;
    });
}

/// The window is built.
pub fn mark_window() {
    let at = since_start();
    with(|m| { m.window.get_or_insert(at.unwrap_or_default()); });
}

/// The first frame is committed to the screen; `splash` says whether the splash will play.
pub fn mark_first_frame(splash: bool) {
    let at = since_start();
    with(|m| {
        m.first_frame.get_or_insert(at.unwrap_or_default());
        m.splash.get_or_insert(splash);
    });
}

/// The first engine check: how many files it read (`None` when it refused) and what it cost.
pub fn note_engine_check(files_read: Option<usize>, millis: u64) {
    let said = match files_read {
        Some(0) => format!("none (trusted as verified when it was written), {millis} ms"),
        Some(n) => format!("full, once for this runtime ({n} files read), {millis} ms"),
        None => format!("refused the engine, {millis} ms"),
    };
    with(|m| m.engine = Some(said));
}

/// Which splash this start shows and whether its opening played (Sage's plan 2026-11-14, "the
/// splash the rules picked is playing within 380 ms of the click", section 6, "The app's launch
/// line": "which splash, whether its opening was used and, if not, why").
pub fn note_splash(id: &str, saved: bool, opening: Result<usize, String>) {
    with(|m| {
        m.splash_id = Some(id.to_string());
        m.splash_saved = saved;
        m.opening = Some(opening);
    });
}

/// The page joined the opening at this moment of the splash.
pub fn note_joined(moment: f64) {
    with(|m| m.joined = Some(moment));
}

/// The start-up hook has finished.
pub fn mark_finished() {
    let at = since_start();
    with(|m| m.finished = at);
}

/// The page took over the splash (or the ceiling came, `page = false`): write the line, once.
pub fn write_line(page: bool) {
    let at = since_start();
    let line = with(|m| {
        if m.written {
            return None;
        }
        m.written = true;
        if page {
            m.page = at;
        }
        Some(line(m, process_started_ms()))
    });
    if let Some(line) = line {
        eprintln!("{line}");
    }
}

/// The line, from the marks. Pure, so its wording is a unit test.
fn line(m: &Marks, started: Option<u64>) -> String {
    let ms = |v: Option<u64>| v.map(|v| format!("{v} ms")).unwrap_or_else(|| "unknown".into());
    let page = match m.page {
        Some(v) => format!("page took over {v} ms"),
        None => "page took over: not reported (the ceiling ended the wait)".into(),
    };
    let bundle = match (&m.report.bundle_check, &m.report.handover) {
        (None, _) => "bundle check none (no update installed)".to_string(),
        (Some(check), None) => format!("bundle check {check}"),
        (Some(check), Some(old)) if old.is_empty() => format!(
            "bundle check {check}, after a hand-over from an older copy that does not report its own"
        ),
        (Some(check), Some(old)) => format!("bundle check {check}, after a hand-over whose check was {old}"),
    };
    let splash = match &m.splash_id {
        Some(id) => {
            let picked = if m.splash_saved { "picked at the last quit" } else { "picked by the rules at this start" };
            let opening = match &m.opening {
                Some(Ok(n)) => format!("its opening used ({n} later pictures)"),
                Some(Err(why)) => format!("its opening not used: {why}"),
                None => "its opening not used: not known".to_string(),
            };
            let joined = m.joined.map(|v| format!(", the page joined it at {v:.0} ms")).unwrap_or_default();
            format!("splash shown, {id} ({picked}), {opening}{joined}")
        }
        None if m.splash == Some(true) => "splash shown".to_string(),
        None => "no splash".to_string(),
    };
    format!(
        "[richos] launch timing: update step {}, window {}, first frame {}, start-up finished {}, {page} after the \
         process started; {splash}; update put in place: {}; {bundle}; engine check {}; process started at {}",
        ms(m.update_step),
        ms(m.window),
        ms(m.first_frame),
        ms(m.finished),
        if m.report.activated { "yes" } else { "no" },
        m.engine.as_deref().unwrap_or("none (no engine)"),
        started.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_start_is_the_kernels_and_comes_before_now() {
        let started = process_started_ms().expect("the kernel answers for this process");
        assert!(started <= now_ms());
        assert!(now_ms() - started < 24 * 3600 * 1000, "a test process started over a day ago");
    }

    #[test]
    fn the_line_says_every_milestone_and_which_check_ran() {
        let m = Marks {
            update_step: Some(15),
            window: Some(210),
            first_frame: Some(214),
            finished: Some(330),
            page: Some(420),
            splash: Some(true),
            report: crate::update_startup::LaunchReport {
                bundle_check: Some("none (trusted as written at its update), 1 ms".into()),
                handover: None,
                activated: false,
            },
            engine: Some("none (trusted as verified when it was written), 3 ms".into()),
            splash_id: None,
            splash_saved: false,
            opening: None,
            joined: None,
            written: false,
        };
        assert_eq!(
            line(&m, Some(1_763_112_345_678)),
            "[richos] launch timing: update step 15 ms, window 210 ms, first frame 214 ms, start-up finished 330 ms, \
             page took over 420 ms after the process started; splash shown; update put in place: no; bundle check none \
             (trusted as written at its update), 1 ms; engine check none (trusted as verified when it was written), 3 ms; \
             process started at 1763112345678"
        );
        let old = Marks { report: crate::update_startup::LaunchReport { handover: Some(String::new()), ..m.report.clone() }, ..m.clone() };
        assert!(line(&old, None).contains("after a hand-over from an older copy that does not report its own"));
        let opened = Marks {
            splash_id: Some("round-11/v2".into()),
            splash_saved: true,
            opening: Some(Ok(30)),
            joined: Some(412.4),
            ..m.clone()
        };
        assert!(line(&opened, None).contains(
            "; splash shown, round-11/v2 (picked at the last quit), its opening used (30 later pictures), the page joined it at 412 ms; "
        ));
        let fell_back = Marks { splash_saved: false, opening: Some(Err("nothing was saved at the last quit".into())), joined: None, ..opened };
        assert!(line(&fell_back, None).contains(
            "; splash shown, round-11/v2 (picked by the rules at this start), its opening not used: nothing was saved at the last quit; "
        ));
        let ceiling = Marks { page: None, splash: Some(false), engine: None, ..m };
        let text = line(&ceiling, None);
        assert!(text.contains("page took over: not reported"), "{text}");
        assert!(text.contains("no splash") && text.contains("engine check none (no engine)"), "{text}");
    }
}
