//! Update activation happens before the app owns a conversation or worker.
//! A lifetime filesystem lease prevents another participating launch from
//! replacing this bundle while the running app still needs its resources.
#[cfg(target_os = "macos")]
use richos_user_update::StartupLease;
#[cfg(not(target_os = "macos"))]
type StartupLease = ();

const INHERITED_SESSION: &str = "RICHOS_UPDATE_SESSION_FD";

/// This deliberately runs before home resolution, leases, Tauri or native workers.
/// The installer invokes it inside a restricted process to detect a mismatched
/// executable before changing the application the user opens.
pub fn identity_probe(compiled_version: &str) -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--richos-internal-update-identity"))
        || args.next().is_some()
    {
        return false;
    }
    println!("{}", serde_json::json!({
        "identifier": "com.richos.app",
        "version": compiled_version,
        "protocol": 1
    }));
    true
}

#[cfg(target_os = "macos")]
fn wait_for_lock<T>(mut action: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let start = std::time::Instant::now();
    loop {
        match action() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
                && start.elapsed() < std::time::Duration::from_secs(10) => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            result => return result,
        }
    }
}

#[cfg(target_os = "macos")]
pub fn prepare(compiled_version: &str) -> Result<Option<StartupLease>, String> {
    use std::{fs, io, os::unix::fs::MetadataExt, path::PathBuf, process::Command};
    let inherited = std::env::var_os(INHERITED_SESSION);
    let redirected = inherited.is_some();
    // Never forward the hint to Claude or any other ordinary child.
    std::env::remove_var(INHERITED_SESSION);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let Some(bundle) = crate::activation::bundle_root(&exe) else {
        if inherited.is_some() { return Err("Update redirect did not enter an application bundle.".into()); }
        return Ok(None);
    };
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("Application home is unavailable.")?);
    // Match setup's actual debug data override before creating any update files.
    // A test of an isolated conversation must not activate this user's real app.
    #[cfg(debug_assertions)]
    if std::env::var_os("RICHOS_TEST_DATA_DIR").map(PathBuf::from)
        .is_some_and(|dir| dir != crate::activation::installed_data_dir(&home, "com.richos.app"))
    {
        if redirected { return Err("Redirected update changed application data scope.".into()); }
        return Ok(None);
    }
    richos_user_update::bundle_version(&bundle).map_err(|e| e.to_string())?;
    let before = fs::metadata(&bundle).map_err(|e| e.to_string())?;
    let mut lease = match inherited {
        Some(fd) => {
            let fd = fd.to_str().and_then(|s| s.parse::<i32>().ok()).ok_or("Invalid update session descriptor.")?;
            wait_for_lock(|| StartupLease::adopt(&home, fd))
        }
        None => wait_for_lock(|| StartupLease::acquire(&home)),
    }.map_err(|e| format!("Could not establish application update exclusion: {e}"))?;
    let current_bundle_matches = || -> bool {
        fs::metadata(&bundle).map(|m| m.dev() == before.dev() && m.ino() == before.ino()).unwrap_or(false)
            && richos_user_update::bundle_version(&bundle).ok().as_deref() == Some(compiled_version)
    };
    let loaded_bundle_changed = !current_bundle_matches();
    if lease.can_activate() && !redirected && !loaded_bundle_changed {
        match wait_for_lock(|| richos_user_update::collect_retired(&mut lease, &bundle)) {
            Ok(0) => (),
            Ok(count) => eprintln!("[richos] reclaimed {count} retired update payloads"),
            Err(error) => eprintln!("[richos] update payload reclamation deferred: {error}"),
        }
    }
    let activated = if lease.can_activate() && !redirected {
        match wait_for_lock(|| richos_user_update::activate_prepared(&mut lease, compiled_version)) {
            Ok(activation) => {
                // THE BOOT LINE NAMES BOTH VERSIONS AND WHICH WAY IT WENT. An activation
                // record that says only what is now installed cannot distinguish a
                // rollback from an update after the fact, and "which one did this Mac
                // just do" is the first question asked when a nightly goes wrong.
                if let Some(a) = &activation {
                    eprintln!(
                        "[richos] {} activated: {} -> {}",
                        if a.rollback { "rollback" } else { "update" },
                        a.left.as_deref().unwrap_or("(nothing installed)"),
                        a.entered.version
                    );
                }
                activation.is_some()
            }
            Err(error) => {
                // Publication is an atomic directory exchange. If that occurred,
                // the already loaded process must not continue against new resources.
                if !current_bundle_matches() { return Err(format!("Update activation needs recovery before the app can start: {error}")); }
                eprintln!("[richos] update remains deferred: {error}");
                wait_for_lock(|| lease.begin_session()).map_err(|e| e.to_string())?;
                return Ok(Some(lease));
            }
        }
    } else { false };
    // READ BEFORE `with_preferred`, never inside it: that callback runs while the publication
    // lock is held, and `last_activation` takes the same lock. Inside, the read could never
    // succeed (until 2026-10-21 it failed at once with EWOULDBLOCK, so `rolled_back_here` was
    // always false; the lock now refuses a second take on the thread that holds it). Nothing
    // between here and the redirect can change the record: only an activation writes it, an
    // activation needs the exclusive session lease, and this process holds that lease (or
    // its shared side, which excludes every other activation) until it exits.
    let last_activation = richos_user_update::last_activation(&home).ok().flatten();
    let result = wait_for_lock(|| richos_user_update::with_preferred(&home, |publication| {
        if redirected {
            if exe.canonicalize()? != publication.executable(&home).canonicalize()?
                || publication.version != compiled_version
            {
                return Err(io::Error::new(io::ErrorKind::InvalidData,
                    "redirected executable disagrees with its verified release"));
            }
            return Ok(());
        }
        let newer = publication.is_newer_than(compiled_version)?;
        // A DELIBERATE ROLLBACK IS THE ONE OLDER REDIRECT THAT IS CORRECT, and it is
        // admitted by a durable record of what the last activation did rather than by this
        // process knowing it performed one -- a second launch that arrives moments after
        // another process completed the exchange sees exactly the same changed bundle and
        // must reach the same answer. Without it, opening RichOS twice around a rollback
        // refuses to start with a sentence about a downgrade nobody attempted.
        let rolled_back_here = last_activation
            .as_ref()
            .is_some_and(|a| a.rollback && a.entered.version == publication.version);
        if loaded_bundle_changed && !newer && publication.version != compiled_version && !rolled_back_here {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Changed application cannot redirect to an older release"));
        }
        if !activated && !loaded_bundle_changed && !newer {
            return Ok(());
        }
        let mut command = Command::new(publication.executable(&home));
        command.args(std::env::args_os().skip(1))
            .env(INHERITED_SESSION, lease.session_fd().to_string());
        // exec preserves the launch relationship. Carry explicit activation
        // overrides unchanged; do not turn background harness boots into GUI boots.
        let error = lease.exec(&mut command);
        Err::<(), io::Error>(error)
    }));
    match result {
        Ok(None) if redirected || loaded_bundle_changed => return Err("Changed application has no publication receipt.".into()),
        Ok(_) => (),
        Err(error) if !activated && !redirected && !loaded_bundle_changed => {
            // A corrupt preferred receipt must not prevent the untouched old
            // installation from opening. It keeps the session lease either way.
            eprintln!("[richos] preferred update remains deferred: {error}");
        }
        Err(error) => return Err(format!("The activated application could not start: {error}")),
    }
    wait_for_lock(|| lease.begin_session()).map_err(|e| e.to_string())?;
    Ok(Some(lease))
}

#[cfg(not(target_os = "macos"))]
pub fn prepare(_compiled_version: &str) -> Result<Option<StartupLease>, String> { Ok(None) }

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::wait_for_lock;
    use std::io::{Error, ErrorKind};

    /// Startup waits out a BUSY update record (`WouldBlock`) and nothing else. Since
    /// 2026-10-21 the record's lock refuses a second take on the thread that holds it with
    /// `InvalidData` (richos-user-update `Lock::acquire`), and that refusal must end a startup
    /// wait at once: retried, it would cost every launch this loop's whole 10 s for an answer
    /// that cannot change. Counted in attempts, never timed.
    #[test]
    fn startup_retries_a_busy_record_and_returns_any_other_refusal_at_once() {
        let mut attempts = 0;
        let busy_twice = wait_for_lock(|| {
            attempts += 1;
            if attempts < 3 { Err(Error::from(ErrorKind::WouldBlock)) } else { Ok(attempts) }
        });
        assert_eq!(busy_twice.unwrap(), 3, "a busy record is waited out");

        let mut attempts = 0;
        let refused = wait_for_lock(|| -> std::io::Result<()> {
            attempts += 1;
            Err(Error::from(ErrorKind::InvalidData))
        });
        assert_eq!(refused.unwrap_err().kind(), ErrorKind::InvalidData);
        assert_eq!(attempts, 1, "a refusal that is not busy was retried");
    }
}
