//! THE LAUNCH LINE — Sage's plan `richos-hq/docs/plans/2026-11-14-splash-under-half-a-second.md`,
//! rule 5 (and the launch-speed audit's piece 4).
//!
//! Nothing measured launch time, which is how a 2-second check went back in front of the window on
//! 2026-10-07 and shipped in every build after it, and how a 4-second launch on the CEO's Mac had
//! to be timed by hand. So every launch writes ONE line, when the page takes over the splash or at
//! the ceiling, with its own milestones counted from the moment macOS started the process:
//!
//! ```text
//! [richos] launch timing: update step 15 ms, window 210 ms, first frame 214 ms, splash started
//! playing 221 ms, start-up finished 330 ms, page took over 420 ms after the process started; splash
//! shown, round-11/v2 (native pack); update put in place: no; bundle check none (trusted as written
//! at its update), 1 ms; engine check none (trusted as verified when it was written), 3 ms; process
//! started at 1763112345678
//! ```
//!
//! `splash started playing` is the native splash's frame zero, and is in the line only when the
//! native player reported it; a web-animation splash plays with the page, at `page took over`.
//!
//! THE LINE IS KEPT, for every launch, in `~/Library/Logs/RichOS/launch.log` ([`log_path`]), one
//! entry per launch with the time, the version and the pid in front. A launch from the Dock or
//! Finder has no terminal, so what the app writes to its error output is kept nowhere; until
//! 2026-11-17 that was the only place this line went, and nobody could read a launch's times
//! after starting the app the ordinary way.
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
    splash_id: Option<String>,
    native_splash: bool,
    splash_started: Option<u64>,
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

/// Which splash was selected and whether its native player was attached.
pub fn note_splash(id: &str, native: bool) {
    with(|m| {
        m.splash_id = Some(id.to_string());
        m.native_splash = native;
    });
}

/// The video clock is anchored on the host clock and begins at the next refresh.
pub fn note_native_splash_started(anchor_ms: f64) {
    if let Some(start) = process_started_ms() {
        let at = (anchor_ms.round() as u64).saturating_sub(start);
        let said = format!("[richos] native splash: frame zero scheduled {at} ms after process start");
        eprintln!("{said}");
        // A player that became ready only after the launch line was written still gets its
        // start into the file, as its own entry.
        let line_written = with(|m| {
            m.splash_started = Some(at);
            m.written
        });
        if line_written {
            keep(&said);
        }
    }
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
        keep(&line);
    }
}

/// `~/Library/Logs/RichOS/launch.log`, beside the startup log (`startup_alert::log_path`).
pub fn log_path() -> std::path::PathBuf {
    crate::startup_alert::log_path().with_file_name("launch.log")
}

/// Keep `line` in the launch log. A launch that cannot write its log is still a launch.
fn keep(line: &str) {
    let entry = log_entry(now_ms(), crate::startup_alert::version(), std::process::id(), line);
    if let Err(error) = append(&log_path(), &entry) {
        eprintln!("[richos] launch log: the line was not kept in {} ({error})", log_path().display());
    }
}

/// One entry of the launch log: when, which version, which process, then the line as written.
fn log_entry(millis: u64, version: &str, pid: u32, line: &str) -> String {
    format!("{} RichOS {version} pid {pid} {line}\n", crate::startup_alert::utc_stamp(millis))
}

/// Append one entry, creating the folder and the file, and keeping one earlier generation.
fn append(path: &std::path::Path, entry: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::startup_alert::rotate_if_large(path);
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(entry.as_bytes())
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
    let playing = m.splash_started.map(|v| format!("splash started playing {v} ms, ")).unwrap_or_default();
    let splash = match &m.splash_id {
        Some(id) => {
            let player = if m.native_splash { "native pack" } else { "web animation" };
            format!("splash shown, {id} ({player})")
        }
        None if m.splash == Some(true) => "splash shown".to_string(),
        None => "no splash".to_string(),
    };
    format!(
        "[richos] launch timing: update step {}, window {}, first frame {}, {playing}start-up finished {}, {page} after the \
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
                update_arrival: None,
                bundle_check: Some("none (trusted as written at its update), 1 ms".into()),
                handover: None,
                activated: false,
            },
            engine: Some("none (trusted as verified when it was written), 3 ms".into()),
            splash_id: None,
            native_splash: false,
            splash_started: None,
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
            native_splash: true,
            ..m.clone()
        };
        assert!(line(&opened, None).contains("; splash shown, round-11/v2 (native pack); "));
        let played = Marks { splash_started: Some(221), ..opened.clone() };
        assert!(
            line(&played, None).contains("first frame 214 ms, splash started playing 221 ms, start-up finished 330 ms, "),
            "{}",
            line(&played, None)
        );
        let fallback = Marks { native_splash: false, ..opened };
        assert!(line(&fallback, None).contains("; splash shown, round-11/v2 (web animation); "));
        let ceiling = Marks { page: None, splash: Some(false), engine: None, ..m };
        let text = line(&ceiling, None);
        assert!(text.contains("page took over: not reported"), "{text}");
        assert!(text.contains("no splash") && text.contains("engine check none (no engine)"), "{text}");
    }

    /// A Dock launch has no terminal: the line must be in a file afterwards, one entry per launch.
    #[test]
    fn every_launch_line_is_kept_in_the_launch_log() {
        let dir = std::env::temp_dir().join(format!("richos-launch-log-test-{}", std::process::id()));
        assert!(!dir.exists(), "{} is left from an earlier run", dir.display());
        let path = dir.join("Logs").join("RichOS").join("launch.log");
        let first = log_entry(1_757_493_852_000, "1.2.3", 41, "[richos] launch timing: update step 15 ms");
        assert_eq!(first, "2025-09-10T08:44:12Z RichOS 1.2.3 pid 41 [richos] launch timing: update step 15 ms\n");
        append(&path, &first).expect("the first launch creates the folder and the file");
        append(&path, &log_entry(1_757_493_853_000, "1.2.3", 42, "[richos] launch timing: update step 16 ms"))
            .expect("the second launch adds to it");
        let kept = std::fs::read_to_string(&path).expect("the launch log is readable");
        let lines: Vec<&str> = kept.lines().collect();
        assert_eq!(lines.len(), 2, "{kept}");
        assert!(lines[0].ends_with("pid 41 [richos] launch timing: update step 15 ms"), "{kept}");
        assert!(lines[1].starts_with("2025-09-10T08:44:13Z RichOS 1.2.3 pid 42 "), "{kept}");
        assert!(log_path().ends_with("RichOS/launch.log"), "{}", log_path().display());
        std::fs::remove_dir_all(&dir).expect("the test removes what it wrote");
    }
}
