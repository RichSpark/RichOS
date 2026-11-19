//! The verified app stays at the path from which the user opens it.
use super::*;

const SYSTEM_INSTALL: &str = "system-install.json";
const MIGRATION: &str = "install-migration.json";
const BLOCKED: &str = "system-update-denied.json";

fn permission_required() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied,
        "RichOS needs permission to replace the existing app in Applications. Install the verified update at that same location.")
}

#[cfg(not(test))]
fn system_bundle() -> PathBuf {
    PathBuf::from("/Applications").join(APP)
}

// Tests substitute an isolated directory. Shipping code has no path override.
#[cfg(test)]
thread_local! {
    static TEST_SYSTEM: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn system_bundle() -> PathBuf {
    TEST_SYSTEM.with(|path| {
        path.borrow()
            .clone()
            .unwrap_or_else(|| PathBuf::from("/nonexistent").join(APP))
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemInstall {
    first_compatible_version: String,
}

fn system_install(root: &Path) -> io::Result<Option<SystemInstall>> {
    match read_document(&root.join(SYSTEM_INSTALL)) {
        Ok(record) => Ok(Some(record)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn home_bundle(root: &Path) -> PathBuf {
    root.parent()
        .expect("updater journal lives under Applications")
        .join(APP)
}

#[cfg(all(target_os = "macos", not(test)))]
fn is_account_home(root: &Path) -> bool {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut found = std::ptr::null_mut();
    let mut buffer = [0u8; 16384];
    let status = unsafe {
        libc::getpwuid_r(
            uid(),
            record.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() {
        return false;
    }
    let directory = unsafe { (*found).pw_dir };
    !directory.is_null()
        && Path::new(std::ffi::OsStr::from_bytes(
            unsafe { CStr::from_ptr(directory) }.to_bytes(),
        ))
        .canonicalize()
        .is_ok_and(|home| root.parent().and_then(Path::parent) == Some(home.as_path()))
}

#[cfg(any(test, not(target_os = "macos")))]
fn is_account_home(_root: &Path) -> bool {
    cfg!(test)
}

fn check_alias(home: &Path, system: &Path) -> io::Result<()> {
    match metadata(home) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(m)
            if m.file_type().is_symlink() && m.uid() == uid() && fs::read_link(home)? == system =>
        {
            Ok(())
        }
        Ok(_) => Err(refuse(
            "another RichOS application occupies the per-user install path",
        )),
        Err(error) => Err(error),
    }
}

pub(super) fn destination(root: &Path) -> io::Result<PathBuf> {
    let home = home_bundle(root);
    if system_install(root)?.is_some() {
        let system = system_bundle();
        check_alias(&home, &system)?;
        Ok(system)
    } else {
        Ok(home)
    }
}

fn require_system_write(root: &Path, system: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let parent = system.parent().expect("system application has a parent");
    let folder = metadata(parent)?;
    let app = metadata(system)?;
    #[cfg(not(test))]
    const PARENT_OWNER: u32 = 0;
    #[cfg(test)]
    let parent_owner = uid();
    #[cfg(not(test))]
    let parent_owner = PARENT_OWNER;
    let path = CString::new(parent.as_os_str().as_bytes()).map_err(io::Error::other)?;
    // Check effective write/search access, including a nonadministrator or read-only mount.
    #[cfg(target_os = "macos")]
    let protected = std::os::macos::fs::MetadataExt::st_flags(&app)
        & (libc::UF_IMMUTABLE | libc::SF_IMMUTABLE | libc::UF_APPEND | libc::SF_APPEND)
        != 0;
    #[cfg(not(target_os = "macos"))]
    let protected = false;
    if protected
        || !is_account_home(root)
        || !folder.is_dir()
        || folder.uid() != parent_owner
        || folder.mode() & 0o002 != 0
        || !app.is_dir()
        || app.uid() != uid()
        || app.dev() != metadata(root)?.dev()
        || unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) } != 0
    {
        return Err(permission_required());
    }
    bundle_identity(system, None)?;
    Ok(())
}

pub(super) fn preflight(root: &Path, destination: &Path) -> io::Result<()> {
    let system = system_bundle();
    if destination == system || legacy_split(root)? {
        require_system_write(root, &system)?;
    }
    Ok(())
}

pub(super) fn require_repaired_installation(root: &Path) -> io::Result<()> {
    if legacy_split(root)? {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied,
            "RichOS must repair the existing app in Applications before it can install this update."));
    }
    Ok(())
}

pub(super) fn legacy_split(root: &Path) -> io::Result<bool> {
    Ok(may_migrate(root, &home_bundle(root))
        && optional_json(&root.join("published.json"))?.is_some()
        && bundle_version(&system_bundle()).is_ok())
}

/// The account a running process acts for, or `None` when the kernel does not say.
#[cfg(target_os = "macos")]
fn process_uid(pid: i32) -> Option<u32> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdshortinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as i32;
    let n = unsafe {
        libc::proc_pidinfo(pid, libc::PROC_PIDT_SHORTBSDINFO, 0, info.as_mut_ptr().cast(), size)
    };
    (n == size).then(|| unsafe { info.assume_init() }.pbsi_uid)
}

/// Refuses while ANOTHER ACCOUNT runs anything from inside the app in Applications, which this
/// account's session lease cannot see. This account's own processes are left alone: its RichOS
/// sessions already hold the lease, its relaunch and at-quit helpers wait on it, and RichFiles'
/// Window Gestures login item keeps running from inside the app after RichOS quits, exactly as
/// it did at the per-user path, where nothing was ever checked.
#[cfg(target_os = "macos")]
fn ensure_unused(system: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 || count > 30_000 {
        return Err(refuse(
            "could not check whether the app in Applications is running",
        ));
    }
    let mut pids = vec![0i32; count as usize + 256];
    let count = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(pids.as_slice()) as i32,
        )
    };
    if count <= 0 || count as usize >= pids.len() {
        return Err(refuse(
            "could not check whether the app in Applications is running",
        ));
    }
    let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let myself = std::process::id() as i32;
    for pid in pids
        .into_iter()
        .take(count as usize)
        .filter(|pid| *pid > 0 && *pid != myself)
    {
        let n = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
        if n > 0 {
            let bytes = &buffer[..n as usize];
            let path = Path::new(std::ffi::OsStr::from_bytes(
                bytes.strip_suffix(&[0]).unwrap_or(bytes),
            ));
            if (path.starts_with(system)
                || path
                    .strip_prefix("/System/Volumes/Data")
                    .is_ok_and(|tail| Path::new("/").join(tail).starts_with(system)))
                && process_uid(pid) != Some(uid())
            {
                return Err(io::Error::new(io::ErrorKind::WouldBlock,
                    "RichOS is still running from Applications. The update will wait for it to quit."));
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn ensure_unused(_system: &Path) -> io::Result<()> {
    Ok(())
}

/// For the update surface only: the last exchange of this update into Applications was denied.
/// Publication never reads it and always tries again, so the update goes through once the
/// user grants the permission; a successful exchange removes the record.
pub(super) fn check_exchange_permission(root: &Path, publication: &Published) -> io::Result<()> {
    match read_document::<String>(&root.join(BLOCKED)) {
        Ok(digest) if digest == publication.archive_sha256 => Err(permission_required()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Whether macOS lets this process change `path` now, without changing it: an extended
/// attribute set and removed at once. That is refused with EPERM exactly where the exchange is,
/// for an app in Applications this process lacks the App Management permission for and for a
/// directory marked immutable.
fn may_change(path: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let name = c"com.richos.update-permission-check";
    let set = unsafe {
        libc::setxattr(path.as_ptr(), name.as_ptr(), b"1".as_ptr().cast(), 1, 0, libc::XATTR_NOFOLLOW)
    };
    if set != 0 {
        let error = io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(libc::EPERM) | Some(libc::EACCES) => permission_required(),
            _ => error,
        });
    }
    if unsafe { libc::removexattr(path.as_ptr(), name.as_ptr(), libc::XATTR_NOFOLLOW) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The user asks again (Try again): the saved denial of the last exchange is removed only when
/// both sides of that exchange can be changed now, so the update reads as ready once the
/// permission is granted and the denial stays on screen while it is still missing.
pub(super) fn retry_exchange_permission(root: &Path, publication: &Published) -> io::Result<()> {
    match check_exchange_permission(root, publication) {
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
        other => return other,
    }
    let dest = destination(root)?;
    match may_change(&root.join(&publication.archive_sha256).join("incoming.app"))
        .and_then(|()| may_change(&dest))
    {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(error),
    }
    match fs::remove_file(root.join(BLOCKED)) {
        Ok(()) => sync(root),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn exchange_update(
    root: &Path,
    incoming: &Path,
    dest: &Path,
    p: &Published,
) -> io::Result<()> {
    if dest == system_bundle() {
        ensure_unused(dest)?;
    }
    match exchange(incoming, dest) {
        Err(error)
            if dest == system_bundle() && error.kind() == io::ErrorKind::PermissionDenied =>
        {
            write_document(root, BLOCKED, &p.archive_sha256)?;
            Err(permission_required())
        }
        Ok(()) => {
            // The swap has happened, so a stale record must not fail it; it only names a
            // digest that is published now.
            match fs::remove_file(root.join(BLOCKED)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    eprintln!("[richos-user-update] could not clear the permission record: {error}")
                }
                _ => {}
            }
            Ok(())
        }
        result => result,
    }
}

pub(super) fn supports_version(root: &Path, version: &str) -> io::Result<bool> {
    let Some(install) = system_install(root)? else {
        return Ok(true);
    };
    Ok(!older(version, &install.first_compatible_version)?)
}

pub(super) fn remember(root: &Path, opened: &Path, version: &str) -> io::Result<()> {
    if opened != system_bundle() || system_install(root)?.is_some() || !is_account_home(root) {
        return Ok(());
    }
    let home = home_bundle(root);
    match metadata(&home) {
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if optional_json(&root.join("published.json"))?
        .is_some_and(|p| !matches(opened, &p).unwrap_or(false))
    {
        // A published app at the old path has to go through the migration below.
        return Ok(());
    }
    if require_system_write(root, opened).is_err() {
        // A copy this account cannot replace (a standard account, a copy another account
        // owns) keeps updating through ~/Applications, as before (plan 2026-11-14, section 4).
        // Nothing is recorded, so a later launch asks again.
        return Ok(());
    }
    write_document(
        root,
        SYSTEM_INSTALL,
        &SystemInstall {
            first_compatible_version: version.into(),
        },
    )
}

pub(super) fn may_migrate(root: &Path, opened: &Path) -> bool {
    (opened == home_bundle(root) || opened == system_bundle())
        && (metadata(&root.join(MIGRATION)).is_ok()
            || (system_install(root).is_ok_and(|record| record.is_none())
                && metadata(&home_bundle(root)).is_ok_and(|m| m.is_dir())
                && metadata(&system_bundle()).is_ok_and(|m| m.is_dir())
                // Two copies are repaired only where this account can replace the one in
                // Applications; otherwise they keep updating through ~/Applications.
                && require_system_write(root, &system_bundle()).is_ok()))
        && is_account_home(root)
}

pub(super) fn has_startup_work(home: &Path, opened: &Path) -> bool {
    let root = home.join("Applications/.richos-updater");
    (opened == system_bundle() && !root.join(SYSTEM_INSTALL).exists())
        || root.join(MIGRATION).exists()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Migration {
    // The publication receipt format must remain readable by the previous release.
    publication: Published,
    current: Retirement,
    displaced: Retirement,
    transaction: String,
}

fn is_payload(path: &Path, receipt: &Retirement) -> bool {
    metadata(path).is_ok_and(|m| m.is_dir() && m.ino() == receipt.payload_ino)
        && volume_uuid(path).is_ok_and(|volume| volume == receipt.payload_volume)
}

pub(super) fn migrate(root: &Path, opened: &Path, version: &str) -> io::Result<bool> {
    let system = system_bundle();
    let home = home_bundle(root);
    let saved = root.join(MIGRATION);
    let migration: Migration = match read_document(&saved) {
        Ok(existing) => existing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if !may_migrate(root, opened) {
                return Ok(false);
            }
            let Some(publication) = optional_json(&root.join("published.json"))? else {
                return Ok(false);
            };
            if !matches(&home, &publication)? {
                return Ok(false);
            }
            let installed = bundle_version(&system)?;
            // The same or a newer version put in Applications by hand (the regular installer)
            // stays where it is, and the copy the updater left in ~/Applications is retired
            // instead, from whichever of the two the app ran. Otherwise the newer copy in
            // ~/Applications moves into Applications, from the copy that ran it.
            let adopt = !publication.is_newer_than(&installed)?;
            let ran = if adopt && opened == system { &installed } else { &publication.version };
            if (!adopt && opened != home) || ran != version {
                return Ok(false);
            }
            require_system_write(root, &system)?;
            // Scan ownership and internal links using the existing bundle integrity checks.
            tree_hash(&system)?;
            let old = metadata(&system)?;
            let transaction = digest(
                format!("install migration {} {}", volume_uuid(&system)?, old.ino()).as_bytes(),
            );
            let tx = root.join(&transaction);
            ensure_dir(&tx, 0o700)?;
            let alias = tx.join("incoming.app");
            match metadata(&alias) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => symlink(&system, &alias)?,
                Ok(m)
                    if m.file_type().is_symlink()
                        && m.uid() == uid()
                        && fs::read_link(&alias)? == system => {}
                _ => return Err(refuse("invalid update migration alias")),
            }
            sync(&tx)?;
            // `current` is the copy that ends in Applications, `displaced` the one retired. An
            // adopted copy is already in place, so the swap below is skipped and only the copy
            // in ~/Applications gives way to the link.
            let (current, displaced) = if adopt { (&system, &home) } else { (&home, &system) };
            let migration = Migration {
                publication,
                current: Retirement::capture(&tx, current)?,
                displaced: Retirement::capture(&tx, displaced)?,
                transaction,
            };
            write_document(root, MIGRATION, &migration)?;
            migration
        }
        Err(error) => return Err(error),
    };
    migration.publication.valid()?;
    if migration.transaction.len() != 64
        || !migration
            .transaction
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refuse("invalid update migration transaction"));
    }
    let tx = root.join(&migration.transaction);
    let retired = tx.join("incoming.app");
    owned_dir(&tx)?;
    if is_payload(&system, &migration.displaced) && is_payload(&home, &migration.current) {
        migration.current.check(&tx, &home)?;
        migration.displaced.check(&tx, &system)?;
        require_system_write(root, &system)?;
        ensure_unused(&system)?;
        exchange(&home, &system)?;
        sync(system.parent().unwrap())?;
        sync(home.parent().unwrap())?;
    }
    migration.current.check(&tx, &system)?;
    if is_payload(&home, &migration.displaced) {
        migration.displaced.check(&tx, &home)?;
        check_alias(&retired, &system)?;
        if metadata(&retired).is_err() {
            return Err(refuse("update migration alias is missing"));
        }
        exchange(&home, &retired)?;
    }
    check_alias(&home, &system)?;
    if !metadata(&home)?.file_type().is_symlink() {
        return Err(refuse(
            "update migration did not restore the previous launch path",
        ));
    }
    migration.displaced.check(&tx, &retired)?;
    sync(&tx)?;
    sync(home.parent().unwrap())?;
    sync(system.parent().unwrap())?;
    write_document(&tx, "retirement.json", &migration.displaced)?;
    // An adopted copy is not the publication the receipt describes, unless the installer put
    // the very same app there: the receipt goes with the retired copy, so nothing checks the
    // copy in Applications against it until the next update writes its own.
    let receipt = root.join("published.json");
    if optional_json(&receipt)?
        .is_some_and(|p| p.archive_sha256 == migration.publication.archive_sha256)
        && !matches(&system, &migration.publication)?
    {
        fs::rename(&receipt, tx.join("published.json"))?;
        sync(&tx)?;
    }
    write_document(
        root,
        SYSTEM_INSTALL,
        &SystemInstall {
            // The version now in Applications: the moved publication, or the adopted copy.
            first_compatible_version: bundle_version(&system)?,
        },
    )?;
    fs::remove_file(saved)?;
    sync(root)?;
    Ok(true)
}

pub(super) fn recover_migration(root: &Path, opened: &Path, version: &str) -> io::Result<()> {
    match read_document::<Migration>(&root.join(MIGRATION)) {
        // A draft whose swap never happened can wait for quit. In particular a
        // macOS permission denial must not turn the next launch into an alert.
        Ok(draft)
            if is_payload(&system_bundle(), &draft.displaced)
                && is_payload(&home_bundle(root), &draft.current) =>
        {
            Ok(())
        }
        Ok(_) => migrate(root, opened, version).map(|_| ()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::tests::archive;
    use std::os::unix::ffi::OsStrExt;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let work = tempfile::tempdir().unwrap();
        let base = work.path().canonicalize().unwrap();
        let home = base.join("home");
        let seed = base.join("seed");
        let system = base.join("Applications").join(APP);
        fs::create_dir(&home).unwrap();
        fs::create_dir(&seed).unwrap();
        fs::create_dir(system.parent().unwrap()).unwrap();
        install_verified(&seed, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        fs::rename(seed.join("Applications").join(APP), &system).unwrap();
        TEST_SYSTEM.with(|path| *path.borrow_mut() = Some(system.clone()));
        (work, home, system)
    }

    /// A long-running native executable. A copied system binary such as `/bin/sleep` is
    /// killed by the kernel within a second when run from outside the system volume.
    fn sleeper() -> Vec<u8> {
        static BINARY: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        BINARY
            .get_or_init(|| {
                let temp = tempfile::tempdir().unwrap();
                let source = temp.path().join("sleeper.c");
                let executable = temp.path().join("sleeper");
                fs::write(&source, "#include <unistd.h>\nint main(void){sleep(120);return 0;}\n").unwrap();
                let result = spawn_past_lock_steps(
                    std::process::Command::new("/usr/bin/cc")
                        .arg(&source)
                        .arg("-o")
                        .arg(&executable)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped()),
                )
                .unwrap()
                .wait_with_output()
                .unwrap();
                assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
                fs::read(executable).unwrap()
            })
            .clone()
    }

    /// A process of this account running an executable from inside `bundle`, killed on drop.
    struct Resident(std::process::Child, PathBuf);

    impl Resident {
        fn start(bundle: &Path, folder: &str, name: &str) -> Self {
            let dir = bundle.join(folder);
            fs::create_dir_all(&dir).unwrap();
            let exe = dir.join(name);
            fs::write(&exe, sleeper()).unwrap();
            fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
            let child = spawn_past_lock_steps(
                std::process::Command::new(&exe).stdin(std::process::Stdio::null()),
            )
            .unwrap();
            let resident = Self(child, bundle.to_owned());
            for _ in 0..200 {
                if resident.running_inside() {
                    return resident;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("the resident process never reported its executable inside the bundle");
        }

        /// Whether the kernel reports this live process's executable inside the bundle, which
        /// is what the updater's running-process check reads.
        fn running_inside(&self) -> bool {
            let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
            let n = unsafe {
                libc::proc_pidpath(self.0.id() as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32)
            };
            n > 0 && Path::new(std::ffi::OsStr::from_bytes(&buffer[..n as usize])).starts_with(&self.1)
        }
    }

    impl Drop for Resident {
        fn drop(&mut self) {
            if self.0.kill().is_ok() && self.0.wait().is_err() {
                eprintln!("the resident process {} was killed but not reaped", self.0.id());
            }
        }
    }

    #[test]
    fn window_gestures_login_item_inside_the_app_does_not_block_updates() {
        // RichFiles' Window Gestures login item runs from inside RichOS.app and keeps running
        // after RichOS quits. It must not hold back the update at launch or at quit.
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);
        let gestures = Resident::start(
            &system,
            "Contents/Applications/RichFiles.app/Contents/Library/LoginItems/RichFiles Window Gestures.app/Contents/MacOS",
            "RichFiles Window Gestures",
        );

        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        assert!(gestures.running_inside());
        let mut lease = StartupLease::acquire(&home).unwrap();
        assert!(activate_prepared(&mut lease, "1.0.0").unwrap().is_some());
        drop(lease);
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");

        let gestures = Resident::start(
            &system,
            "Contents/Applications/RichFiles.app/Contents/Library/LoginItems/RichFiles Window Gestures.app/Contents/MacOS",
            "RichFiles Window Gestures",
        );
        stage_verified(&home, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        assert!(gestures.running_inside());
        activate_at_quit(&home, "2.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "3.0.0");
    }

    #[test]
    fn restart_to_update_helpers_do_not_block_the_publication_at_quit() {
        // "Restart to update" quits the app and leaves the app's own executable running from
        // inside the bundle: the relaunch helper, which waits for the session lease and then
        // starts the app again, beside the at-quit helper that puts the update in place. The
        // relaunch helper, or the app it has just started, must not make that publication
        // refuse and push the swap back in front of the next window.
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);
        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let relaunch_helper = Resident::start(&system, "Contents/MacOS", "richos-relaunch-helper");
        assert!(relaunch_helper.running_inside());

        let done = activate_at_quit(&home, "1.0.0", &system).unwrap().unwrap();
        assert_eq!(done.entered.version, "2.0.0");
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
        assert!(!home.join("Applications").join(APP).exists());
    }

    /// The fixture's Applications folder made read-only, as it is for a standard account,
    /// until dropped.
    struct ReadOnlyApplications(PathBuf);

    impl ReadOnlyApplications {
        fn new(system: &Path) -> Self {
            let folder = system.parent().unwrap().to_owned();
            fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
            Self(folder)
        }
    }

    impl Drop for ReadOnlyApplications {
        fn drop(&mut self) {
            if fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755)).is_err() {
                eprintln!("could not make {} writable again", self.0.display());
            }
        }
    }

    #[test]
    fn copy_this_account_cannot_replace_keeps_updating_through_the_per_user_path() {
        // A standard account cannot write to Applications (and the same check refuses a copy
        // another account owns). Its copy there is not remembered as the install location:
        // downloads are accepted and land in ~/Applications, as before this change.
        let (_work, home, system) = fixture();
        let user_app = home.join("Applications").join(APP);
        let _standard = ReadOnlyApplications::new(&system);
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);

        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        activate_at_quit(&home, "1.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&user_app).unwrap(), "2.0.0");
        assert_eq!(bundle_version(&system).unwrap(), "1.0.0");

        // The two copies that leaves are not held for a repair this account cannot make:
        // the next update is accepted and put in place too.
        assert!(!needs_install_migration(&home, &user_app));
        assert!(staged(&home).unwrap().is_none());
        stage_verified(&home, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        assert_eq!(staged(&home).unwrap().unwrap().publication.version, "3.0.0");
        activate_at_quit(&home, "2.0.0", &user_app).unwrap().unwrap();
        assert_eq!(bundle_version(&user_app).unwrap(), "3.0.0");
        assert_eq!(bundle_version(&system).unwrap(), "1.0.0");
    }

    #[test]
    fn running_check_tells_this_account_from_another() {
        // launchd runs as root: a process of another account is still seen as one.
        assert_eq!(process_uid(1), Some(0));
        assert_eq!(process_uid(std::process::id() as i32), Some(uid()));
    }

    #[test]
    fn system_application_is_updated_at_its_original_path() {
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);

        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        activate_at_quit(&home, "1.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
        assert!(!home.join("Applications").join(APP).exists());
        assert_eq!(
            with_preferred_at(&home, |p, path| Ok((p.version.clone(), path.to_owned()))).unwrap(),
            Some(("2.0.0".into(), system))
        );
    }

    #[test]
    fn existing_two_copy_installation_keeps_both_launch_references() {
        let (_work, home, system) = fixture();
        install_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let old_home = home.join("Applications").join(APP);
        assert!(needs_install_migration(&home, &old_home));
        activate_at_quit(&home, "2.0.0", &old_home).unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
        assert_eq!(fs::read_link(&old_home).unwrap(), system);
        assert!(!needs_install_migration(&home, &old_home));

        stage_verified(&home, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        activate_at_quit(&home, "2.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "3.0.0");
        assert_eq!(
            rollback_target(&home).unwrap(),
            RollbackTarget::Available("2.0.0".into())
        );
        stage_rollback_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        assert!(
            activate_at_quit(&home, "3.0.0", &system)
                .unwrap()
                .unwrap()
                .rollback
        );
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
        assert_eq!(fs::read_link(&old_home).unwrap(), system);
    }

    #[test]
    fn denied_system_installation_leaves_the_update_pending_at_the_original_path() {
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);
        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let parent = system.parent().unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o555)).unwrap();
        let error = activate_at_quit(&home, "1.0.0", &system).unwrap_err();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(bundle_version(&system).unwrap(), "1.0.0");
        assert!(!home.join("Applications").join(APP).exists());
        assert_eq!(staged(&home).unwrap().unwrap().publication.version, "2.0.0");
        activate_at_quit(&home, "1.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
    }

    /// The staged app made immutable, so the exchange itself is refused with EPERM, as macOS
    /// refuses it after a "Don't Allow" on its permission prompt, until dropped (permission
    /// granted).
    struct ExchangeDenied(std::ffi::CString);

    impl ExchangeDenied {
        fn new(path: &Path) -> Self {
            let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::chflags(path.as_ptr(), libc::UF_IMMUTABLE as _) }, 0);
            Self(path)
        }
    }

    impl Drop for ExchangeDenied {
        fn drop(&mut self) {
            if unsafe { libc::chflags(self.0.as_ptr(), 0) } != 0 {
                eprintln!("could not clear the immutable flag on {:?}", self.0);
            }
        }
    }

    #[test]
    fn update_denied_at_the_exchange_goes_through_once_permission_is_granted() {
        // One wrong click on a macOS permission prompt denies the exchange into Applications.
        // Once permission is granted, the next attempt puts the same verified update in place.
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);
        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let root = root(&home).unwrap();
        let digest = staged(&home).unwrap().unwrap().publication.archive_sha256;
        let denied = ExchangeDenied::new(&root.join(&digest).join("incoming.app"));

        let error = activate_at_quit(&home, "1.0.0", &system).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(bundle_version(&system).unwrap(), "1.0.0");
        // While permission is missing, the update surface says so.
        assert_eq!(staged(&home).unwrap_err().kind(), io::ErrorKind::PermissionDenied);

        drop(denied);
        activate_at_quit(&home, "1.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
        assert!(staged(&home).unwrap().is_none());
    }

    #[test]
    fn try_again_after_permission_is_granted_shows_the_update_ready() {
        // Review rv-20261119T144657Z finding 2: the user grants the permission and presses the
        // offered Try again; the update surface must stop repeating the old denial.
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);
        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let root = root(&home).unwrap();
        let digest = staged(&home).unwrap().unwrap().publication.archive_sha256;
        let denied = ExchangeDenied::new(&root.join(&digest).join("incoming.app"));
        assert_eq!(
            activate_at_quit(&home, "1.0.0", &system).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        drop(denied);
        // The automatic check, before anyone presses anything, still says what happened last.
        assert_eq!(staged(&home).unwrap_err().kind(), io::ErrorKind::PermissionDenied);

        retry_after_denial(&home).unwrap();
        let ready = staged(&home).unwrap().unwrap();
        assert_eq!((ready.publication.version.as_str(), ready.rollback), ("2.0.0", false));
        activate_at_quit(&home, "1.0.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
    }

    #[test]
    fn try_again_while_permission_is_still_missing_keeps_the_denial_on_screen() {
        // Review rv-20261119T165922Z finding 2: Try again pressed before the permission is
        // granted must not claim the update is ready; the denial stays until the swap can go.
        let (_work, home, system) = fixture();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "1.0.0").unwrap();
        drop(lease);
        stage_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let root = root(&home).unwrap();
        let digest = staged(&home).unwrap().unwrap().publication.archive_sha256;
        let denied = ExchangeDenied::new(&root.join(&digest).join("incoming.app"));
        activate_at_quit(&home, "1.0.0", &system).unwrap_err();
        retry_after_denial(&home).unwrap();
        assert_eq!(staged(&home).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(bundle_version(&system).unwrap(), "1.0.0");
        // The same press after the permission is granted reads ready.
        drop(denied);
        retry_after_denial(&home).unwrap();
        assert_eq!(staged(&home).unwrap().unwrap().publication.version, "2.0.0");
    }


    #[test]
    fn manual_reinstall_over_two_copies_takes_the_next_update_in_applications() {
        // Review rv-20261119T144657Z finding 1: a user the old updater left with a copy in
        // ~/Applications puts the same or a newer version in Applications with the regular
        // installer. The next update must be put in place in Applications at quit, whichever of
        // the two copies the app ran from, instead of waiting for good.
        for by_hand in ["2.0.0", "2.1.0"] {
            for from_applications in [true, false] {
                let (work, home, system) = fixture();
                install_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
                let old_home = home.join("Applications").join(APP);
                assert_eq!(bundle_version(&old_home).unwrap(), "2.0.0");

                let installer = work.path().canonicalize().unwrap().join("installer");
                fs::create_dir(&installer).unwrap();
                install_verified(&installer, &archive(by_hand, |_| {}), by_hand).unwrap();
                fs::remove_dir_all(&system).unwrap();
                fs::rename(installer.join("Applications").join(APP), &system).unwrap();

                stage_verified(&home, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
                let (opened, running) =
                    if from_applications { (&system, by_hand) } else { (&old_home, "2.0.0") };
                assert!(needs_install_migration(&home, opened));
                let done = activate_at_quit(&home, running, opened)
                    .unwrap_or_else(|e| panic!("{by_hand} by hand, from_applications={from_applications}: {e}"))
                    .unwrap();
                assert_eq!(done.entered.version, "3.0.0");
                assert_eq!(bundle_version(&system).unwrap(), "3.0.0");
                assert_eq!(fs::read_link(&old_home).unwrap(), system);
                assert!(staged(&home).unwrap().is_none());
                assert!(!needs_install_migration(&home, &system));

                // The next update goes to Applications too, with no repair in front of it.
                stage_verified(&home, &archive("4.0.0", |_| {}), "4.0.0").unwrap();
                activate_at_quit(&home, "3.0.0", &system).unwrap().unwrap();
                assert_eq!(bundle_version(&system).unwrap(), "4.0.0");
            }
        }
    }

    /// `version` put at `system` by hand, as the regular installer does.
    fn install_by_hand(work: &tempfile::TempDir, system: &Path, version: &str) {
        let installer = work.path().canonicalize().unwrap().join(format!("installer-{version}"));
        fs::create_dir(&installer).unwrap();
        install_verified(&installer, &archive(version, |_| {}), version).unwrap();
        if system.exists() {
            fs::remove_dir_all(system).unwrap();
        }
        fs::rename(installer.join("Applications").join(APP), system).unwrap();
    }

    #[test]
    fn going_back_after_a_manual_reinstall_never_strands_later_updates() {
        // Review rv-20261119T165922Z finding 1: the ~/Applications copy's history must not be
        // offered as Go back once a newer copy was installed in Applications by hand, and a way
        // back prepared before that reinstall is retired at quit instead of staying ready.
        let (work, home, system) = fixture();
        fs::remove_dir_all(&system).unwrap();
        install_verified(&home, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        install_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        assert_eq!(rollback_target(&home).unwrap(), RollbackTarget::Available("1.0.0".into()));
        stage_rollback_verified(&home, &archive("1.0.0", |_| {}), "1.0.0").unwrap();

        install_by_hand(&work, &system, "2.1.0");
        assert_eq!(rollback_target(&home).unwrap(), RollbackTarget::NoRecord);

        activate_at_quit(&home, "2.1.0", &system).unwrap_err();
        assert_eq!(bundle_version(&system).unwrap(), "2.1.0");
        assert!(staged(&home).unwrap().is_none());
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&system, "2.1.0").unwrap();
        assert!(activate_prepared(&mut lease, "2.1.0").unwrap().is_none());
        drop(lease);

        stage_verified(&home, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        activate_at_quit(&home, "2.1.0", &system).unwrap().unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "3.0.0");
    }

    #[test]
    fn an_adopted_copy_is_not_checked_against_the_retired_copys_receipt() {
        // Review rv-20261119T165922Z finding 3: after the repair keeps a different copy installed
        // by hand, the receipt of the retired ~/Applications copy no longer speaks for the app.
        for (by_hand, keeps_receipt) in [("2.1.0", false), ("2.0.0", true)] {
            let (work, home, system) = fixture();
            fs::remove_dir_all(&system).unwrap();
            install_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
            install_by_hand(&work, &system, by_hand);
            assert!(activate_at_quit(&home, by_hand, &system).unwrap().is_none());
            assert_eq!(fs::read_link(home.join("Applications").join(APP)).unwrap(), system);
            let preferred = preferred(&home).unwrap();
            assert_eq!(preferred.is_some(), keeps_receipt, "{by_hand} by hand");
            if let Some(p) = preferred {
                assert_eq!(p.version, by_hand);
            }
        }
    }

    #[test]
    fn migration_recovery_finishes_only_a_swap_that_already_happened() {
        let (_work, home, system) = fixture();
        let current = install_verified(&home, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let root = root(&home).unwrap();
        let user_app = home_bundle(&root);
        let transaction = digest(b"interrupted install migration");
        let tx = root.join(&transaction);
        ensure_dir(&tx, 0o700).unwrap();
        symlink(&system, tx.join("incoming.app")).unwrap();
        let record = Migration {
            publication: current,
            current: Retirement::capture(&tx, &user_app).unwrap(),
            displaced: Retirement::capture(&tx, &system).unwrap(),
            transaction,
        };
        write_document(&root, MIGRATION, &record).unwrap();
        let mut lease = StartupLease::acquire(&home).unwrap();
        lease.remember_installation(&user_app, "2.0.0").unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "1.0.0");

        exchange(&user_app, &system).unwrap();
        lease.remember_installation(&system, "2.0.0").unwrap();
        assert_eq!(bundle_version(&system).unwrap(), "2.0.0");
        assert_eq!(fs::read_link(&user_app).unwrap(), system);
        assert!(record
            .displaced
            .check(&tx, &tx.join("incoming.app"))
            .is_ok());
        assert!(!root.join(MIGRATION).exists());
    }
}
