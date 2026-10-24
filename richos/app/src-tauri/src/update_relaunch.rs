//! RELAUNCH INTO A DOWNLOADED UPDATE WHEN NOTHING IS RUNNING (CEO feedback 2026-10-06, item 1).
//!
//! His words: *"there was no work going on at the time. So, there was nothing that could be
//! interrupted. In this case, the app should automatically re-launch immediately after
//! downloading the new version. Avoiding/preventing a re-launch is only meant for when there
//! are workers running and an immediate relaunch would destroy their work."*
//!
//! WHAT "RUNNING" MEANS is not decided here. It is `updates::work_verdict`, the same gate that
//! already hides the download control: the active turn, the spine, this session's workers,
//! background work on every work lease, and commands Rich started. One gate, so the relaunch
//! and the download can never disagree about it.
//!
//! WHY A HELPER PROCESS AND NOT `AppHandle::restart`. Activation of a prepared update needs
//! the EXCLUSIVE side of the session lease at startup (`update_startup::prepare`), and every
//! running RichOS holds the shared side until its process ends. Tauri's restart SPAWNS the
//! replacement first and exits second (`tauri-2.11.5/src/process.rs:74-88`), so the
//! replacement would meet this process's shared lease, skip activation, and come back on the
//! old version. So the app quits through its own quit path (which settles every lease and
//! writes the clean-exit marker), and a small helper, this same executable started with
//! [`HELPER_ARG`], waits for that process to be gone and for the lease to be free, then
//! starts the app again. The new start activates the update exactly as an ordinary launch
//! would.
//!
//! WHERE HE LANDS. The window's position and size are recorded as it moves
//! (`remember_window_geometry`), the open conversation is the backend's `active_thread`, and
//! drafts and scroll positions are parked in the webview's storage (`main.js`). The relaunch
//! uses all three exactly as any launch does, and `updates.js` parks the view state the
//! moment the notice appears.

use std::ffi::OsString;
use std::time::{Duration, Instant};

/// The argument that makes this executable the relaunch helper instead of the app.
pub const HELPER_ARG: &str = "--richos-internal-update-relaunch";

/// How long the helper waits for the app to quit. The quit path is bounded at 2 s
/// (`main.rs`, `quit_bound`); a quit that has not happened after this was declined (he was
/// asked about registered work and kept it), so the helper starts nothing.
const QUIT_BOUND: Duration = Duration::from_secs(60);

/// How long the helper waits for the session lease once the app is gone. MCP helpers of the
/// old process can hold it briefly while they wind down. Past this the app is started
/// anyway: it opens on the version that is running now and the update stays prepared for
/// the next launch, which is better than not coming back at all.
const LEASE_BOUND: Duration = Duration::from_secs(30);

/// The helper's arguments: the app's pid, then `--`, then the app's own arguments.
pub fn helper_args(app_pid: u32, app_args: &[OsString]) -> Vec<OsString> {
    let mut args = vec![OsString::from(HELPER_ARG), OsString::from(app_pid.to_string()), OsString::from("--")];
    args.extend(app_args.iter().cloned());
    args
}

/// Read [`helper_args`] back: `Some((pid, app_args))` when `args` (without the program name)
/// is a helper invocation.
pub fn parse_helper_args(args: &[OsString]) -> Option<(u32, Vec<OsString>)> {
    if args.first()?.to_str()? != HELPER_ARG {
        return None;
    }
    let pid = args.get(1)?.to_str()?.parse::<u32>().ok().filter(|p| *p > 1)?;
    if args.get(2)?.to_str()? != "--" {
        return None;
    }
    Some((pid, args[3..].to_vec()))
}

/// Start the helper from the running app. It inherits this process's environment (the
/// test VM passes HOME and the engine directory that way), plus the one activation answer
/// this launch already had, so the relaunch comes back the way this one came up.
pub fn spawn_helper(regular: bool) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let app_args: Vec<OsString> = std::env::args_os().skip(1).collect();
    std::process::Command::new(exe)
        .args(helper_args(std::process::id(), &app_args))
        .env(
            crate::activation::OVERRIDE_ENV,
            if regular { crate::activation::OVERRIDE_REGULAR } else { "accessory" },
        )
        .stdin(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

/// The helper itself. Runs first in `main`, before any lease, alert or runtime exists.
/// Returns `None` when this process is not the helper.
pub fn helper_main() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let (app_pid, app_args) = parse_helper_args(&args)?;
    // 1. THE APP QUITS. The helper is the app's child, so the app being gone is a positive
    //    signal: the kernel reparents the helper the moment its parent exits.
    let started = Instant::now();
    while parent_pid() == app_pid {
        if started.elapsed() >= QUIT_BOUND {
            eprintln!("[richos] update relaunch: RichOS did not quit, so nothing was started");
            return Some(0);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // 2. NO SESSION HOLDS THE LEASE, so the start below can activate the update.
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        match richos_user_update::wait_for_sessions_to_end(&home, LEASE_BOUND) {
            Ok(true) => {}
            Ok(false) => eprintln!("[richos] update relaunch: a RichOS session still holds the update lease; starting anyway"),
            Err(error) => eprintln!("[richos] update relaunch: could not read the update lease ({error}); starting anyway"),
        }
    }
    // 3. START THE APP AGAIN, with the same arguments and environment.
    let code = match std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe).args(&app_args).stdin(std::process::Stdio::null()).spawn()
    }) {
        Ok(_) => 0,
        Err(error) => {
            eprintln!("[richos] update relaunch: could not start RichOS again: {error}");
            1
        }
    };
    Some(code)
}

#[cfg(unix)]
fn parent_pid() -> u32 {
    std::os::unix::process::parent_id()
}

#[cfg(not(unix))]
fn parent_pid() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_arguments_round_trip_and_carry_the_apps_own_arguments() {
        let app_args = vec![OsString::from("--flag"), OsString::from("--")];
        let args = helper_args(4242, &app_args);
        assert_eq!(parse_helper_args(&args), Some((4242, app_args)));
        assert_eq!(parse_helper_args(&helper_args(77, &[])), Some((77, vec![])));
    }

    #[test]
    fn anything_else_is_not_the_helper() {
        let os = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(parse_helper_args(&[]), None);
        assert_eq!(parse_helper_args(&os(&["--questions-mcp", "x"])), None);
        assert_eq!(parse_helper_args(&os(&[HELPER_ARG])), None);
        assert_eq!(parse_helper_args(&os(&[HELPER_ARG, "abc", "--"])), None);
        // launchd's pid is never the app, so waiting on it would never end.
        assert_eq!(parse_helper_args(&os(&[HELPER_ARG, "1", "--"])), None);
        assert_eq!(parse_helper_args(&os(&[HELPER_ARG, "42", "x"])), None);
    }
}
