//! Password-free macOS application publication.
//!
//! Only pass bytes returned by the signature-verifying updater download. This module
//! never runs an installer, shell or authorization API. Its user-owned journal provides
//! crash recovery, not isolation from a hostile process with the same filesystem rights.
//! The workspace supervisor must deny agents access to this application namespace.

#![cfg(unix)]
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Component, Path, PathBuf},
    sync::RwLock,
};

const APP: &str = "RichOS.app";
const ID: &str = "com.richos.app";
/// Cumulative expanded size a downloaded update archive may reach before extraction refuses.
///
/// hardware-fixed: a decompression-bomb ceiling, not a share of anybody's disk. It is a bound on
/// how large a LEGITIMATE `RichOS.app` can be, so it is a property of what we ship and must be
/// identical on a 256 GB laptop and a 4 TB desktop -- deriving it from free space would let a
/// hostile archive expand further on the machine that has more room, which is exactly backwards.
/// The disk that DOES get asked is the download's, in `model-fetch.js::freeBytesFor`.
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;
fn refuse(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
/// THE WINDOW BETWEEN `fork` AND `exec`, HELD OPEN BY EXACTLY ONE SIDE OF THIS LOCK.
///
/// `flock` exclusion belongs to the OPEN FILE DESCRIPTION, and `fork` hands a child a
/// reference to every description this process has open. `O_CLOEXEC` closes the child's
/// copies -- but only when it EXECS. Between the two, a child forked by one thread holds
/// another thread's `session.lock` open, so that thread's release does not release and the
/// next acquisition reads "another RichOS session is running" over a session that ended.
///
/// Measured 2026-09-19/20 on the release Mac: two copies of this crate's test binary at
/// once, 8 threads each, produced that error in one of the two, a different test each time;
/// the same two processes with `--test-threads 1` were both green. It cost a nightly build
/// and it is the reason this exists.
///
/// Dropping the `pre_exec` closure in `probe()` is not a repair: ordinary macOS
/// `Command` spawns can also temporarily retain another thread's session descriptor.
/// It would also remove the marking of EVERY inherited descriptor close-on-exec before
/// handing control to a staged binary this process has not finished verifying.
/// `posix_spawn` respects the flag; it cannot set it on a descriptor somebody else
/// opened without it. And `fcntl`
/// record locks would trade this bug for a worse one: `F_OFD_SETLK` locks live on the
/// description, which is the very thing `fork` copies, so the bug would survive untouched;
/// classic process-associated locks are not inherited by a child, but they also do not
/// conflict with THEMSELVES inside one process, and two leases in one process is exactly
/// how `stage_does_not_replace_live_bundle_and_shared_sessions_veto_activation` states the
/// product's meaning.
///
/// Lease acquisition and conversion take the shared side. The probe and every unit-test
/// fixture launch take the exclusive side until `Command::spawn` returns. Protecting only
/// the probe leaves compiler, ACL and fixture-process launches able to retain a released
/// lease and cause a spurious refusal in another test.
///
/// This coordinates only this crate's launches. Uncoordinated launches elsewhere in the
/// process can still inherit a lease before exec despite close-on-exec. Production startup
/// activation must run before runtime threads, as the startup/exec API requires.
///
/// THE WINDOW DOES NOT END WHEN `spawn` RETURNS -- NOT BY ITSELF. A launch that needs a
/// pre-exec step (`probe()` does) is a real fork, and `Command::spawn` returns when its own
/// close-on-exec status pipe reaches end-of-file. On macOS that end-of-file overtakes the
/// release of the lock on every OTHER close-on-exec descriptor exec is sweeping: measured
/// 2026-09-29, a lease numbered below 2000 or 8000 inherited non-file descriptors was still
/// held when `spawn` returned in 20 of 20 rounds each, and in 0 of 20 once the child closed
/// its file descriptors itself before exec. With the exclusive side already released, the
/// next acquisition read the child's copy as a session. That is the nightly .31 failure
/// (`gates/updater-tests`, two rollback tests, 2026-09-29). So every coordinated launch
/// closes its inherited close-on-exec FILES in the child, before exec -- see
/// `release_files_before_exec` -- and `spawn` returning then means what this lock needs.
static FORK_WINDOW: RwLock<()> = RwLock::new(());

/// Run one lock observation -- an `flock`, and the `open` that precedes it -- with no fork
/// window open. Never wrap anything that itself calls this: a recursive read lock can
/// deadlock against a waiting writer.
fn lock_step<T>(step: impl FnOnce() -> T) -> T {
    let _held = FORK_WINDOW
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    step()
}

/// Fork and exec with no lock observation in flight. The child closes every inherited
/// close-on-exec file before it execs, so when `spawn` returns -- exec succeeded or failed
/// -- the child holds no lock this process took, and the exclusive side has been held for
/// exactly the dangerous window.
fn spawn_past_lock_steps(command: &mut std::process::Command) -> io::Result<std::process::Child> {
    release_files_before_exec(command);
    let _held = FORK_WINDOW
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    command.spawn()
}

/// One row of `proc_pidinfo(PROC_PIDLISTFDS)`: `struct proc_fdinfo`, <sys/proc_info.h>.
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct ProcFd {
    fd: i32,
    kind: u32,
}
#[cfg(target_os = "macos")]
const PROC_PIDLISTFDS: i32 = 1;
/// `PROX_FDTYPE_VNODE`: files, directories, devices. `flock` exists only on these.
#[cfg(target_os = "macos")]
const PROX_FDTYPE_VNODE: u32 = 1;
#[cfg(target_os = "macos")]
#[link(name = "proc")]
extern "C" {
    fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut libc::c_void, size: i32) -> i32;
}
/// Room for every descriptor a child can list. Allocate BEFORE fork: the child may not.
/// RLIMIT_NOFILE can be lowered below already-open descriptors, so only the actual
/// inventory is authoritative, never a loop up to the limit.
#[cfg(target_os = "macos")]
fn descriptor_inventory() -> Vec<ProcFd> {
    vec![ProcFd { fd: 0, kind: 0 }; 65_536]
}
/// This process's open descriptors, into memory allocated before fork. Async-signal-safe.
#[cfg(target_os = "macos")]
fn list_descriptors(room: &mut [ProcFd]) -> io::Result<&[ProcFd]> {
    let capacity = std::mem::size_of_val(room) as i32;
    let size = unsafe {
        proc_pidinfo(
            libc::getpid(),
            PROC_PIDLISTFDS,
            0,
            room.as_mut_ptr().cast(),
            capacity,
        )
    };
    if size <= 0
        || size >= capacity
        || !(size as usize).is_multiple_of(std::mem::size_of::<ProcFd>())
    {
        return Err(io::Error::from_raw_os_error(libc::EIO));
    }
    Ok(&room[..size as usize / std::mem::size_of::<ProcFd>()])
}

/// In the CHILD, after every step the caller registered and immediately before exec: close
/// each inherited close-on-exec FILE now instead of leaving it to exec's own sweep.
///
/// Nothing the child runs can tell the difference -- exec was about to close exactly these.
/// A descriptor WITHOUT close-on-exec is deliberately inherited and is left alone. Pipes and
/// sockets are left to exec too: they carry no `flock`, and one of them is `spawn`'s own
/// status pipe, which must stay open until exec. What changes is only the ORDER: a lease
/// copy is gone before that pipe can report, so `spawn` returning is evidence again.
#[cfg(target_os = "macos")]
fn release_files_before_exec(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    let mut room = descriptor_inventory();
    unsafe {
        command.pre_exec(move || {
            for entry in list_descriptors(&mut room)? {
                if entry.fd < 3 || entry.kind != PROX_FDTYPE_VNODE {
                    continue;
                }
                let flags = libc::fcntl(entry.fd, libc::F_GETFD);
                if flags < 0 {
                    return Err(io::Error::last_os_error());
                }
                if flags & libc::FD_CLOEXEC != 0 && libc::close(entry.fd) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}
/// Not implemented off macOS, where `probe` and `exchange` refuse and nothing is published.
#[cfg(not(target_os = "macos"))]
fn release_files_before_exec(_command: &mut std::process::Command) {}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[cfg(test)]
thread_local! { static SYNC_TRACE: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) }; }
fn sync(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()?;
    #[cfg(test)]
    SYNC_TRACE.with(|trace| trace.borrow_mut().push(path.to_owned()));
    Ok(())
}
fn metadata(path: &Path) -> io::Result<fs::Metadata> {
    fs::symlink_metadata(path)
}
#[cfg(target_os = "macos")]
fn no_write_acl(path: &Path) -> io::Result<()> {
    use std::ffi::c_void;
    extern "C" {
        fn acl_get_fd_np(fd: i32, kind: i32) -> *mut c_void;
        fn acl_valid(acl: *mut c_void) -> i32;
        fn acl_get_entry(acl: *mut c_void, which: i32, entry: *mut *mut c_void) -> i32;
        fn acl_get_tag_type(entry: *mut c_void, tag: *mut i32) -> i32;
        fn acl_get_permset_mask_np(entry: *mut c_void, mask: *mut u64) -> i32;
        fn acl_free(acl: *mut c_void) -> i32;
    }
    struct Acl(*mut c_void);
    impl Drop for Acl {
        fn drop(&mut self) {
            unsafe {
                acl_free(self.0);
            }
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let raw = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
    if raw.is_null() {
        let error = io::Error::last_os_error();
        // Darwin reports ENOENT for a valid object with no extended ACL.
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(());
        }
        return Err(error);
    }
    let acl = Acl(raw);
    if unsafe { acl_valid(acl.0) } != 0 {
        return Err(refuse("invalid application access control list"));
    }
    let mut which = 0;
    for _ in 0..1024 {
        let mut entry = std::ptr::null_mut();
        let rc = unsafe { acl_get_entry(acl.0, which, &mut entry) };
        if rc != 0 {
            if rc == -1 && io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
                return Ok(());
            }
            return Err(refuse("cannot inspect application access control list"));
        }
        which = -1;
        let mut tag = 0;
        let mut mask = 0;
        if unsafe { acl_get_tag_type(entry, &mut tag) } != 0
            || unsafe { acl_get_permset_mask_np(entry, &mut mask) } != 0
        {
            return Err(refuse("cannot inspect application ACL entry"));
        }
        let write_bits = (1 << 2)
            | (1 << 4)
            | (1 << 5)
            | (1 << 6)
            | (1 << 8)
            | (1 << 10)
            | (1 << 12)
            | (1 << 13);
        if !matches!(tag, 1 | 2) || (tag == 1 && mask & write_bits != 0) {
            return Err(refuse("application namespace has an ACL write grant"));
        }
    }
    Err(refuse("application access control list is too large"))
}
#[cfg(not(target_os = "macos"))]
fn no_write_acl(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn owned_dir(path: &Path) -> io::Result<()> {
    let m = metadata(path)?;
    if !m.is_dir() || m.uid() != uid() || m.mode() & 0o022 != 0 {
        return Err(refuse("application directory is not private to its owner"));
    }
    no_write_acl(path)
}
fn ensure_dir(path: &Path, mode: u32) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => fs::set_permissions(path, fs::Permissions::from_mode(mode))?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    owned_dir(path)
}
fn root(home: &Path) -> io::Result<PathBuf> {
    if uid() == 0 {
        return Err(refuse("RichOS user updates must not run as root"));
    }
    // A home supplied by the platform resolver must itself be an absolute real path.
    if !home.is_absolute() || home.canonicalize()? != home {
        return Err(refuse("home is not a canonical directory"));
    }
    for ancestor in home.ancestors().skip(1) {
        let m = metadata(ancestor)?;
        let root_sticky = m.uid() == 0 && m.mode() & 0o1000 != 0;
        if !m.is_dir()
            || (m.uid() != 0 && m.uid() != uid())
            || (m.mode() & 0o022 != 0 && !root_sticky)
        {
            return Err(refuse("home has an unsafe ancestor"));
        }
        no_write_acl(ancestor)?;
    }
    owned_dir(home)?;
    let applications = home.join("Applications");
    ensure_dir(&applications, 0o755)?;
    let root = applications.join(".richos-updater");
    ensure_dir(&root, 0o700)?;
    if metadata(&root)?.mode() & 0o077 != 0 {
        return Err(refuse("update journal directory is not private"));
    }
    Ok(root)
}
/// A held `flock`. `publication` marks the publication lock (`lock`), whose holder thread is
/// recorded in [`PUBLICATION_HELD`]. The `PhantomData` makes a `Lock` neither `Send` nor
/// `Sync`: it is released on the thread that took it, so that record stays true.
struct Lock(File, bool, std::marker::PhantomData<*const ()>);
thread_local! {
    /// This thread holds the publication lock. A second take on the same thread can never
    /// succeed (`flock` conflicts between two descriptions even inside one process), so
    /// [`Lock::acquire`] refuses it AT ONCE instead of waiting [`PUBLICATION_WAIT`] for a
    /// release that only this thread could perform.
    static PUBLICATION_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// What a caller is told when the publication lock stayed busy for the whole
/// [`PUBLICATION_WAIT`]. The kind stays `WouldBlock`, so a caller can tell "busy, ask again"
/// from a real failure; the words replace the bare OS text ("Resource temporarily
/// unavailable (os error 35)"), which said nothing about what was busy.
pub const PUBLICATION_BUSY: &str = "the update record stayed in use by another part of RichOS";
/// How long a publication-lock caller waits for another holder before it answers "busy".
///
/// Every holder of `lock` holds it for a short, bounded act: a receipt read, a lease
/// conversion, a rename. So a caller that finds it held WAITS for it, bounded, rather than
/// failing on the first try. Until 2026-10-21 the readers (`staged`, `last_activation`,
/// `rollback_target`, `collect_retired`) took it with ONE non-blocking `flock` and no wait,
/// so two of the app's own threads asking at the same moment -- the launch check's
/// `staged()` and the window's `update_state` reading `rollback_target()` -- made one of them
/// fail with `EWOULDBLOCK`, which a user saw as "RichOS could not prepare this update.",
/// technical reason "Resource temporarily unavailable (os error 35)", on a Mac where nothing
/// was being prepared at all. There is deliberately no non-waiting way to take this lock any
/// more, so no future reader can reintroduce that.
const PUBLICATION_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
impl Lock {
    fn acquire_startup(root: &Path) -> io::Result<Self> {
        Self::acquire(root)
    }
    fn acquire(root: &Path) -> io::Result<Self> {
        if PUBLICATION_HELD.with(|held| held.get()) {
            return Err(refuse(
                "the publication lock was taken again on the thread that already holds it",
            ));
        }
        let deadline = std::time::Instant::now() + PUBLICATION_WAIT;
        loop {
            match Self::named(root, "lock") {
                Ok(mut lock) => {
                    lock.1 = true;
                    PUBLICATION_HELD.with(|held| held.set(true));
                    return Ok(lock);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(io::Error::new(io::ErrorKind::WouldBlock, PUBLICATION_BUSY));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(e) => return Err(e),
            }
        }
    }
    fn named(root: &Path, name: &str) -> io::Result<Self> {
        lock_step(|| {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(root.join(name))?;
            let m = file.metadata()?;
            if !m.is_file() || m.uid() != uid() || m.nlink() != 1 || m.mode() & 0o077 != 0 {
                return Err(refuse("invalid update lock"));
            }
            // lock-busy: one try. `acquire` polls it for PUBLICATION_WAIT and then answers busy in words; stage.lock's busy reaches updates.rs as WouldBlock and is said as busy.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(file, false, std::marker::PhantomData))
        })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
        if self.1 {
            PUBLICATION_HELD.with(|held| held.set(false));
        }
    }
}

/// Session exclusion is separate from short publication serialization. Every participating
/// app holds a shared lease for its whole runtime; startup activation requires exclusive.
pub struct StartupLease {
    file: File,
    home: PathBuf,
    exclusive: bool,
    valid: bool,
}
impl StartupLease {
    pub fn acquire(home: &Path) -> io::Result<Self> {
        let root = root(home)?;
        let _publication = Lock::acquire_startup(&root)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root.join("session.lock"))?;
        validate_lease_file(&file, &root)?;
        // EXCLUSIVE-then-SHARED is ONE observation: a fork window opening between the two
        // would answer "somebody else is running" about a child that is about to exec.
        let exclusive = lock_step(|| {
            // lock-busy: busy is the answer: another session is running, so this one takes the shared side and does not activate.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(true);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::WouldBlock {
                return Err(error);
            }
            // lock-busy: busy only while a startup holds the exclusive side; WouldBlock returns to the caller: update_startup's wait_for_lock retries for 10 s, and a staging says busy (updates.rs).
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(false)
        })?;
        Ok(Self {
            file,
            home: home.to_owned(),
            exclusive,
            valid: true,
        })
    }
    /// Adopt only the exact inherited session file and retain shared runtime exclusion.
    /// An environment descriptor number is a hint, never evidence of a held lease.
    pub fn adopt(home: &Path, fd: i32) -> io::Result<Self> {
        use std::os::unix::io::FromRawFd;
        if fd < 3 {
            return Err(refuse("invalid inherited session descriptor"));
        }
        // Set close-on-exec on the inherited descriptor immediately, before any child
        // could be launched. Duplicate it so validation failure does not take ownership
        // of an unrelated caller descriptor.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        if copy < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(copy) };
        let root = root(home)?;
        validate_lease_file(&file, &root)?;
        let _publication = Lock::acquire_startup(&root)?;
        // Redirected children only retain SH. Never probe EX on an inherited SH
        // description: BSD conversion can drop protection while trying to upgrade.
        lock_step(|| {
            // lock-busy: busy only while a startup holds the exclusive side; WouldBlock returns to update_startup's wait_for_lock, which retries for 10 s.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })?;
        let exclusive = false;
        unsafe {
            libc::close(fd);
        }
        Ok(Self {
            file,
            home: home.to_owned(),
            exclusive,
            valid: true,
        })
    }
    pub fn can_activate(&self) -> bool {
        self.valid && self.exclusive
    }
    pub fn session_fd(&self) -> i32 {
        self.file.as_raw_fd()
    }
    /// Call immediately before the normal app runtime. Keep this object alive until exit.
    /// Failure is a startup refusal, never permission to run without session exclusion.
    pub fn begin_session(&mut self) -> io::Result<()> {
        if !self.valid {
            return Err(refuse("session lease is no longer held"));
        }
        let root = root(&self.home)?;
        validate_lease_file(&self.file, &root)?;
        if !self.exclusive {
            return Ok(());
        }
        // BSD flock conversion is not assumed atomic. Publication exclusion prevents a
        // competing startup from changing the bundle during the EX-to-SH transition.
        let _publication = Lock::acquire_startup(&root)?;
        self.exclusive = false;
        let fd = self.session_fd();
        // lock-busy: a conversion of a lease this process holds exclusively; failure marks the lease invalid and update_startup's wait_for_lock retries.
        if lock_step(|| unsafe { libc::flock(fd, libc::LOCK_SH | libc::LOCK_NB) }) != 0 {
            self.valid = false;
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    /// Replace the startup image while carrying its session lease across exec. Call only
    /// before runtime threads or user input exist. The caller supplies the exact verified
    /// executable and exports session_fd() as the internal adoption hint.
    pub fn exec(&mut self, command: &mut std::process::Command) -> io::Error {
        use std::os::unix::process::CommandExt;
        if !self.valid {
            return refuse("cannot transfer a released session lease");
        }
        let fd = self.session_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 {
            return io::Error::last_os_error();
        }
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let error = command.exec();
        if unsafe { libc::fcntl(fd, libc::F_SETFD, flags) } < 0 {
            self.valid = false;
            return io::Error::last_os_error();
        }
        error
    }
}
// Dropping File releases the descriptor. Do not explicitly LOCK_UN: an exec-transfer
// duplicate may share this open-file description and must keep exclusion alive.
fn validate_lease_file(file: &File, root: &Path) -> io::Result<()> {
    let path = root.join("session.lock");
    let m = file.metadata()?;
    let named = metadata(&path)?;
    if !m.is_file()
        || !named.is_file()
        || m.uid() != uid()
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
        || m.dev() != named.dev()
        || m.ino() != named.ino()
    {
        return Err(refuse(
            "session descriptor does not match the owner-private lease",
        ));
    }
    no_write_acl(&path)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Published {
    pub version: String,
    pub archive_sha256: String,
    pub tree_sha256: String,
    pub executable_name: String,
}
impl Published {
    pub fn is_newer_than(&self, version: &str) -> io::Result<bool> {
        let this = semver::Version::parse(&self.version)
            .map_err(|_| refuse("invalid published version"))?;
        let other =
            semver::Version::parse(version).map_err(|_| refuse("invalid running version"))?;
        Ok(this > other)
    }
    pub fn executable(&self, home: &Path) -> PathBuf {
        home.join("Applications")
            .join(APP)
            .join("Contents/MacOS")
            .join(&self.executable_name)
    }
    fn valid(&self) -> io::Result<()> {
        for hash in [&self.archive_sha256, &self.tree_sha256] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(refuse("invalid update receipt hash"));
            }
        }
        if semver::Version::parse(&self.version).is_err() || !single_name(&self.executable_name) {
            return Err(refuse("invalid update receipt identity"));
        }
        Ok(())
    }
}
fn single_name(s: &str) -> bool {
    !s.is_empty()
        && Path::new(s).components().count() == 1
        && matches!(Path::new(s).components().next(), Some(Component::Normal(_)))
}
fn older(a: &str, b: &str) -> io::Result<bool> {
    let a = semver::Version::parse(a).map_err(|_| refuse("invalid version"))?;
    let b = semver::Version::parse(b).map_err(|_| refuse("invalid version"))?;
    Ok(a < b)
}

/// THE ONE THING THAT MAY MOVE THIS INSTALLATION BACKWARDS, and it is a file the user's own
/// act wrote.
///
/// Every version check in this module is an inequality in one direction: staging refuses an
/// offer that is not newer than what is published, `recover` refuses to exchange onto an
/// equal-or-newer destination, and `activate_prepared` retires a prepared receipt that is
/// not newer than the running build. That is not incidental strictness -- it is the property
/// that makes a stale process, a replayed manifest or a resurrected transaction unable to
/// reinstate an older application behind the user's back.
///
/// A rollback has to cross exactly those three checks, so it carries an authorization rather
/// than relaxing them. `rollback.json` names the prepared archive it authorizes AND the
/// version it was staged to leave; activation accepts a downgrade only when BOTH still hold
/// at the moment of exchange. A receipt that has been overtaken -- the app was updated again
/// between staging and restart -- authorizes nothing and is retired, which is the same
/// outcome an obsolete forward update gets.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct Rollback {
    /// The `prepared.json` receipt this authorizes, by its archive digest.
    archive_sha256: String,
    /// The version the running application must still be for this to authorize anything.
    from_version: String,
}
impl Rollback {
    fn valid(&self) -> io::Result<()> {
        if self.archive_sha256.len() != 64
            || !self
                .archive_sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || semver::Version::parse(&self.from_version).is_err()
        {
            return Err(refuse("invalid rollback authorization"));
        }
        Ok(())
    }
}

/// Which direction a staging call is allowed to move this installation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Direction {
    /// An ordinary update. Refuses anything that is not strictly newer than what is published.
    Forward,
    /// A deliberate return to an earlier release. Refuses anything that is not strictly older.
    Back,
}

/// WHAT AN ACTIVATION DID, not merely what is now installed.
///
/// `published.json` answers "what am I", and answering "what just happened" from it means
/// subtracting two numbers and guessing the intent behind the difference. A rollback and an
/// update are the same exchange of two directories; only the receipt that authorized it
/// distinguishes them, and only at the moment it is consumed. So the moment records itself.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Activation {
    /// The publication now in `~/Applications/RichOS.app`.
    pub entered: Published,
    /// The version it replaced, or `None` when there was no application there before.
    pub left: Option<String>,
    /// True when an authorized `rollback.json` is what permitted the exchange.
    pub rollback: bool,
}

/// A verified payload waiting for the next launch, and which way it will move.
#[derive(Clone, Debug)]
pub struct Staged {
    pub publication: Published,
    pub rollback: bool,
}

/// What a rollback would do, or the reason there is nothing for it to do.
///
/// Three arms rather than an `Option`, because the two empty answers need different
/// sentences on screen: one says the way back is unknown, the other says there is no way
/// back from here because the record points forwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RollbackTarget {
    /// The version a rollback would fetch, verify and publish.
    Available(String),
    /// This installation holds no record of an earlier version it replaced.
    NoRecord,
    /// The recorded earlier version is not older than what is published now.
    NotOlder(String),
}
fn read_document<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.uid() != uid()
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
        || m.len() > 16_384
    {
        return Err(refuse("invalid update receipt file"));
    }
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|e| refuse(e.to_string()))
}
fn read_json(path: &Path) -> io::Result<Published> {
    let p: Published = read_document(path)?;
    p.valid()?;
    Ok(p)
}
fn write_json(root: &Path, name: &str, p: &Published) -> io::Result<()> {
    write_document(root, name, p)
}
fn write_document<T: Serialize>(root: &Path, name: &str, p: &T) -> io::Result<()> {
    let tmp = root.join(format!("{name}.new"));
    // This fixed temporary file is only reused under the lock after validating its shape.
    if let Ok(m) = metadata(&tmp) {
        if !m.is_file() || m.uid() != uid() || m.nlink() != 1 {
            return Err(refuse("invalid receipt temporary file"));
        }
        fs::remove_file(&tmp)?;
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&tmp)?;
    f.write_all(&serde_json::to_vec(p).map_err(|e| refuse(e.to_string()))?)?;
    f.sync_all()?;
    fs::rename(tmp, root.join(name))?;
    sync(root)
}
fn relative(path: &Path) -> io::Result<PathBuf> {
    if path.components().count() > 128 {
        return Err(refuse("archive path is too deep"));
    }
    let mut parts = path.components();
    if parts.next() != Some(Component::Normal(APP.as_ref())) {
        return Err(refuse("archive must have exactly one RichOS.app root"));
    }
    let mut out = PathBuf::new();
    for part in parts {
        match part {
            Component::Normal(s) => out.push(s),
            _ => return Err(refuse("archive path escapes application")),
        }
    }
    Ok(out)
}
fn parents(base: &Path, rel: &Path) -> io::Result<()> {
    let mut path = base.to_owned();
    if let Some(parent) = rel.parent() {
        for component in parent.components() {
            path.push(component.as_os_str());
            ensure_dir(&path, 0o755)?;
        }
    }
    Ok(())
}
fn safe_link(rel: &Path, target: &Path) -> io::Result<()> {
    let mut depth = rel.parent().map_or(0, |p| p.components().count());
    for c in target.components() {
        match c {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            _ => return Err(refuse("archive link escapes application")),
        }
    }
    Ok(())
}
fn extract(bytes: &[u8], app: &Path) -> io::Result<()> {
    ensure_dir(app, 0o755)?;
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut seen = BTreeSet::new();
    let mut links = Vec::new();
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel = relative(&entry.path()?)?;
        if !seen.insert(rel.clone()) || seen.len() > MAX_ENTRIES {
            return Err(refuse("duplicate or excessive archive entries"));
        }
        let kind = entry.header().entry_type();
        if rel.as_os_str().is_empty() {
            if !kind.is_dir() {
                return Err(refuse("invalid app root"));
            }
            continue;
        }
        parents(app, &rel)?;
        let dest = app.join(&rel);
        if kind.is_dir() {
            ensure_dir(&dest, 0o755)?;
        } else if kind.is_file() {
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| refuse("archive size overflow"))?;
            if total > MAX_BYTES {
                return Err(refuse("archive is too large"));
            }
            let mode = if entry.header().mode()? & 0o111 != 0 {
                0o755
            } else {
                0o644
            };
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(mode)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&dest)?;
            io::copy(&mut entry, &mut out)?;
            out.sync_all()?;
            fs::set_permissions(&dest, fs::Permissions::from_mode(mode))?;
        } else if kind.is_symlink() {
            let target = entry
                .link_name()?
                .ok_or_else(|| refuse("missing link target"))?
                .into_owned();
            safe_link(&rel, &target)?;
            links.push((rel, target));
        } else {
            return Err(refuse("unsupported archive entry (including hard links)"));
        }
    }
    // Links are created last so no payload write can traverse one, regardless of order.
    for (rel, target) in &links {
        parents(app, rel)?;
        symlink(target, app.join(rel))?;
    }
    let canonical = app.canonicalize()?;
    for (rel, _) in &links {
        if !app.join(rel).canonicalize()?.starts_with(&canonical) {
            return Err(refuse("application link resolves outside bundle"));
        }
    }
    sync_tree(app)?;
    Ok(())
}
fn sync_tree(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let p = entry?.path();
        if metadata(&p)?.is_dir() {
            sync_tree(&p)?;
        }
    }
    sync(path)
}
fn bundle_plist(app: &Path) -> io::Result<plist::Value> {
    let path = app.join("Contents/Info.plist");
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.nlink() != 1
        || m.len() > 1024 * 1024
        || (m.uid() != 0 && m.uid() != uid())
        || m.mode() & 0o022 != 0
    {
        return Err(refuse("invalid Info.plist"));
    }
    no_write_acl(&path)?;
    plist::Value::from_reader(f).map_err(|e| refuse(e.to_string()))
}
/// Read a running source bundle's identity/version, including binary plists and
/// protected system copies. This does not authorize user application publication.
pub fn bundle_version(app: &Path) -> io::Result<String> {
    let value = bundle_plist(app)?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| refuse("invalid Info.plist dictionary"))?;
    if dict
        .get("CFBundleIdentifier")
        .and_then(plist::Value::as_string)
        != Some(ID)
    {
        return Err(refuse("bundle is not RichOS"));
    }
    let version = dict
        .get("CFBundleShortVersionString")
        .and_then(plist::Value::as_string)
        .ok_or_else(|| refuse("missing bundle version"))?;
    semver::Version::parse(version).map_err(|_| refuse("invalid bundle version"))?;
    Ok(version.to_owned())
}
fn bundle_identity(app: &Path, version: Option<&str>) -> io::Result<String> {
    owned_dir(app)?;
    owned_dir(&app.join("Contents"))?;
    owned_dir(&app.join("Contents/MacOS"))?;
    let value = bundle_plist(app)?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| refuse("invalid Info.plist dictionary"))?;
    let field = |key: &str| dict.get(key).and_then(plist::Value::as_string);
    if field("CFBundleIdentifier") != Some(ID)
        || version.is_some_and(|v| field("CFBundleShortVersionString") != Some(v))
    {
        return Err(refuse("application identity/version does not match update"));
    }
    let executable =
        field("CFBundleExecutable").ok_or_else(|| refuse("missing bundle executable"))?;
    if !single_name(executable) {
        return Err(refuse("invalid executable name"));
    }
    let m = metadata(&app.join("Contents/MacOS").join(executable))?;
    if !m.is_file() || m.uid() != uid() || m.nlink() != 1 || m.mode() & 0o111 == 0 {
        return Err(refuse("invalid bundle executable"));
    }
    Ok(executable.to_owned())
}
fn tree_hash(root: &Path) -> io::Result<String> {
    fn visit(root: &Path, path: &Path, h: &mut Sha256) -> io::Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let p = entry.path();
            let m = metadata(&p)?;
            if m.uid() != uid() || (!m.file_type().is_symlink() && m.mode() & 0o022 != 0) {
                return Err(refuse("bundle ownership or permissions changed"));
            }
            if !m.file_type().is_symlink() {
                no_write_acl(&p)?;
            }
            let rel = p
                .strip_prefix(root)
                .map_err(|_| refuse("invalid tree path"))?;
            use std::os::unix::ffi::OsStrExt;
            let name = rel.as_os_str().as_bytes();
            h.update((name.len() as u64).to_le_bytes());
            h.update(name);
            h.update((m.mode() & 0o7777).to_le_bytes());
            if m.is_dir() {
                h.update(b"d");
                visit(root, &p, h)?;
            } else if m.is_file() {
                if m.nlink() != 1 {
                    return Err(refuse("bundle file has external hard links"));
                }
                h.update(b"f");
                h.update(m.len().to_le_bytes());
                let mut f = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&p)?;
                let mut b = [0u8; 65536];
                loop {
                    let n = f.read(&mut b)?;
                    if n == 0 {
                        break;
                    }
                    h.update(&b[..n]);
                }
            } else if m.file_type().is_symlink() {
                h.update(b"l");
                let target = fs::read_link(&p)?;
                safe_link(rel, &target)?;
                if !p.canonicalize()?.starts_with(root.canonicalize()?) {
                    return Err(refuse("bundle symlink escapes"));
                }
                let bytes = target.as_os_str().as_bytes();
                h.update((bytes.len() as u64).to_le_bytes());
                h.update(bytes);
            } else {
                return Err(refuse("unsupported bundle object"));
            }
        }
        Ok(())
    }
    let mut h = Sha256::new();
    visit(root, root, &mut h)?;
    Ok(format!("{:x}", h.finalize()))
}
fn matches(app: &Path, p: &Published) -> io::Result<bool> {
    match metadata(app) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    // A different valid old version is expected during prepared-transaction recovery.
    let name = bundle_identity(app, None)?;
    if name != p.executable_name {
        return Ok(false);
    }
    Ok(tree_hash(app)? == p.tree_sha256)
}
#[cfg(target_os = "macos")]
fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    extern "C" {
        fn renameatx_np(
            afd: i32,
            a: *const libc::c_char,
            bfd: i32,
            b: *const libc::c_char,
            flags: u32,
        ) -> i32;
    }
    let a = CString::new(a.as_os_str().as_bytes()).map_err(|_| refuse("invalid app path"))?;
    let b = CString::new(b.as_os_str().as_bytes()).map_err(|_| refuse("invalid app path"))?;
    if unsafe { renameatx_np(libc::AT_FDCWD, a.as_ptr(), libc::AT_FDCWD, b.as_ptr(), 2) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn exchange(_a: &Path, _b: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "application exchange requires macOS",
    ))
}
/// Probe only a verified staged executable, before publication. The sandbox denies
/// writes, networking, process creation and WindowServer access even if a broken build
/// ignores the internal flag. Only the startup identity response is accepted.
#[cfg(target_os = "macos")]
fn probe(app: &Path, p: &Published) -> io::Result<()> {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    if !matches(app, p)? {
        return Err(refuse("staged executable changed before identity probe"));
    }
    let home = app
        .parent()
        .ok_or_else(|| refuse("missing transaction directory"))?
        .join("probe-home");
    ensure_dir(&home, 0o700)?;
    let profile =
        "(version 1)(deny default)(allow file-read*)(allow process-exec)(allow sysctl-read)";
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command
        .args(["-p", profile])
        .arg(app.join("Contents/MacOS").join(&p.executable_name))
        .arg("--richos-internal-update-identity")
        .env_clear()
        .env("HOME", &home)
        .env("TMPDIR", &home)
        .env("RICHOS_TEST_DATA_DIR", home.join("data"))
        .env("LC_ALL", "C")
        .current_dir(&home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Path restrictions cannot revoke already-open file or socket capabilities.
    // Mark rather than close so Rust's pre-exec error pipe remains usable until exec.
    // (`spawn_past_lock_steps` then closes the files among them early; see there.)
    use std::os::unix::process::CommandExt;
    let mut descriptors = descriptor_inventory();
    unsafe {
        command.pre_exec(move || {
            for entry in list_descriptors(&mut descriptors)? {
                if entry.fd < 3 {
                    continue;
                }
                let flags = libc::fcntl(entry.fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(entry.fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = spawn_past_lock_steps(&mut command)?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| refuse("probe stdout unavailable"))?;
    let errors = child
        .stderr
        .take()
        .ok_or_else(|| refuse("probe stderr unavailable"))?;
    let reader = |stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream.take(8193).read_to_end(&mut bytes).map(|_| bytes)
        })
    };
    let stdout = reader(Box::new(output));
    let stderr = reader(Box::new(errors));
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(refuse("staged application identity probe timed out"));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error);
            }
        }
    };
    let output = stdout
        .join()
        .map_err(|_| refuse("identity probe output failed"))??;
    let errors = stderr
        .join()
        .map_err(|_| refuse("identity probe diagnostics failed"))??;
    let status = result?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Identity {
        identifier: String,
        version: String,
        protocol: u64,
    }
    let identity: Identity = serde_json::from_slice(&output)
        .map_err(|_| refuse("staged application did not return its compiled identity"))?;
    if !status.success()
        || output.len() > 8192
        || errors.len() > 8192
        || identity.identifier != ID
        || identity.version != p.version
        || identity.protocol != 1
    {
        return Err(refuse(
            "staged executable identity does not match the verified release",
        ));
    }
    if !matches(app, p)? {
        return Err(refuse("staged executable changed during identity probe"));
    }
    write_json(app.parent().unwrap(), "probe.json", p)
}
#[cfg(not(target_os = "macos"))]
fn probe(_app: &Path, _p: &Published) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "application identity probing requires macOS",
    ))
}
// Clear a damaged prepublication journal only when the current app still matches an
// independent exact old publication receipt (or the first destination never existed).
// Ambiguous post-exchange damage remains blocked and no payload bytes are deleted.
fn quarantine_invalid_pending(root: &Path, p: &Published) -> io::Result<bool> {
    let tx = root.join(&p.archive_sha256);
    owned_dir(&tx)?;
    let dest = root.parent().unwrap().join(APP);
    let previous = optional_json(&tx.join("previous.json"))?;
    let published = optional_json(&root.join("published.json"))?;
    let unchanged = previous
        .iter()
        .chain(published.iter())
        .any(|old| old.tree_sha256 != p.tree_sha256 && matches(&dest, old).unwrap_or(false));
    let absent_first = matches!(metadata(&dest), Err(e) if e.kind()==io::ErrorKind::NotFound)
        && previous.is_none()
        && published.is_none();
    if !unchanged && !absent_first {
        return Ok(false);
    }
    fs::rename(root.join("prepared.json"), tx.join("invalid.json"))?;
    sync(&tx)?;
    sync(root)?;
    Ok(true)
}
fn recover(root: &Path) -> io::Result<Option<Activation>> {
    let journal = root.join("prepared.json");
    let p = match read_json(&journal) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let authorized = rollback_receipt(root, &p)?;
    let dest = root.parent().unwrap().join(APP);
    let transaction = root.join(&p.archive_sha256);
    owned_dir(&transaction)?;
    let incoming = transaction.join("incoming.app");
    let mut left = None;
    if !matches(&dest, &p)? {
        if !matches(&incoming, &p).unwrap_or(false) {
            quarantine_invalid_pending(root, &p)?;
            return Err(refuse("prepared update no longer matches its receipt"));
        }
        if let Err(error) = probe(&incoming, &p) {
            // A rejected payload cannot strand the next valid update behind its journal.
            fs::rename(&journal, transaction.join("rejected.json"))?;
            sync(&transaction)?;
            sync(root)?;
            return Err(error);
        }
        match metadata(&dest) {
            Ok(_) => {
                bundle_identity(&dest, None)?;
                let running = bundle_version(&dest)?;
                // THE ONLY PLACE A DOWNGRADE IS PERMITTED, and it is permitted by a file
                // rather than by a flag in the caller's memory: the authorization has to
                // survive the crash between staging and this exchange, and it has to name
                // the version it was granted against so that an update in between voids it.
                let permitted = match &authorized {
                    Some(rb) => rb.from_version == running && older(&p.version, &running)?,
                    None => p.is_newer_than(&running)?,
                };
                if !permitted {
                    fs::rename(&journal, transaction.join("obsolete.json"))?;
                    sync(&transaction)?;
                    sync(root)?;
                    if authorized.is_some() {
                        clear_rollback(root)?;
                        return Err(refuse(
                            "prepared rollback no longer applies to the installed application",
                        ));
                    }
                    return Err(refuse(
                        "prepared update would replace an equal or newer destination",
                    ));
                }
                left = Some(running);
                let previous = Published {
                    version: bundle_version(&dest)?,
                    archive_sha256: p.archive_sha256.clone(),
                    tree_sha256: tree_hash(&dest)?,
                    executable_name: bundle_identity(&dest, None)?,
                };
                write_json(&transaction, "previous.json", &previous)?;
                exchange(&incoming, &dest)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::rename(&incoming, &dest)?,
            Err(e) => return Err(e),
        }
    }
    // Replay may observe the exchanged destination after a crash before either fsync.
    // Always persist both rename directories before committing the publication receipt.
    sync(&transaction)?;
    sync(root.parent().unwrap())?;
    if !matches(&dest, &p)? || bundle_identity(&dest, Some(&p.version))? != p.executable_name {
        return Err(refuse("published update verification failed"));
    }
    write_json(root, "published.json", &p)?;
    fs::remove_file(journal)?;
    sync(root)?;
    // A REPLAY AFTER A CRASH REACHES HERE WITH `left` UNSET, because the exchange it is
    // completing happened in an earlier process. `previous.json` is that process's own
    // record of what it displaced and is written before the exchange, so the answer is
    // durable rather than reconstructed.
    let left = left.or(optional_json(&transaction.join("previous.json"))?.map(|q| q.version));
    let activation = Activation {
        entered: p,
        left,
        rollback: authorized.is_some(),
    };
    write_document(root, "last-activation.json", &activation)?;
    clear_rollback(root)?;
    sync(root)?;
    // Keep the receipt-bound previous bundle until a later exclusive startup collects it.
    Ok(Some(activation))
}

// Device numbers can change across macOS boots. Durable deletion authority uses the
// filesystem's native volume UUID plus inode, not st_dev persisted in JSON.
#[cfg(target_os = "macos")]
fn volume_uuid(path: &Path) -> io::Result<String> {
    #[repr(C)]
    struct AttrList {
        count: u16,
        reserved: u16,
        common: u32,
        volume: u32,
        directory: u32,
        file: u32,
        fork: u32,
    }
    extern "C" {
        fn fgetattrlist(
            fd: i32,
            attributes: *mut AttrList,
            buffer: *mut libc::c_void,
            size: usize,
            options: u32,
        ) -> i32;
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(path)?;
    let before = file.metadata()?;
    let mut attributes = AttrList {
        count: 5,
        reserved: 0,
        common: 0,
        volume: 0x8004_0000,
        directory: 0,
        file: 0,
        fork: 0,
    };
    let mut bytes = [0u8; 20];
    if unsafe {
        fgetattrlist(
            file.as_raw_fd(),
            &mut attributes,
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let after = file.metadata()?;
    if u32::from_ne_bytes(bytes[..4].try_into().unwrap()) != 20
        || bytes[4..].iter().all(|b| *b == 0)
        || (before.dev(), before.ino()) != (after.dev(), after.ino())
    {
        return Err(refuse("stable updater volume identity unavailable"));
    }
    Ok(bytes[4..].iter().map(|b| format!("{b:02x}")).collect())
}
#[cfg(not(target_os = "macos"))]
fn volume_uuid(_path: &Path) -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "stable updater volume identity requires macOS",
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Retirement {
    transaction_volume: String,
    transaction_ino: u64,
    payload_volume: String,
    payload_ino: u64,
}
impl Retirement {
    fn capture(transaction: &Path, payload: &Path) -> io::Result<Self> {
        let t = metadata(transaction)?;
        let p = metadata(payload)?;
        if !t.is_dir() || !p.is_dir() || t.uid() != uid() || p.uid() != uid() {
            return Err(refuse("invalid updater payload allocation"));
        }
        Ok(Self {
            transaction_volume: volume_uuid(transaction)?,
            transaction_ino: t.ino(),
            payload_volume: volume_uuid(payload)?,
            payload_ino: p.ino(),
        })
    }
    fn check(&self, transaction: &Path, payload: &Path) -> io::Result<()> {
        owned_dir(transaction)?;
        let t = metadata(transaction)?;
        let p = metadata(payload)?;
        if !p.is_dir()
            || p.uid() != uid()
            || (volume_uuid(transaction)?, t.ino())
                != (self.transaction_volume.clone(), self.transaction_ino)
            || (volume_uuid(payload)?, p.ino()) != (self.payload_volume.clone(), self.payload_ino)
        {
            return Err(refuse("updater retirement directory identity changed"));
        }
        Ok(())
    }
}
fn optional_json(path: &Path) -> io::Result<Option<Published>> {
    match read_json(path) {
        Ok(p) => Ok(Some(p)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
/// The rollback authorization for THIS prepared receipt, or `None`.
///
/// A file naming another archive is not this receipt's authorization and is never treated as
/// one -- it is left alone rather than deleted, because deleting somebody else's receipt from
/// a read path is how a repair turns into a loss.
fn rollback_receipt(root: &Path, p: &Published) -> io::Result<Option<Rollback>> {
    let r: Rollback = match read_document(&root.join("rollback.json")) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    r.valid()?;
    Ok((r.archive_sha256 == p.archive_sha256).then_some(r))
}
fn clear_rollback(root: &Path) -> io::Result<()> {
    match fs::remove_file(root.join("rollback.json")) {
        Ok(()) => sync(root),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
/// Reclaim proven updater payloads at a later exclusive startup. The caller must first
/// prove its loaded bundle still matches its startup inode/version. Errors retain bytes
/// and must not prevent the normal runtime. This never runs while any participating
/// session holds SH and never deletes the caller's own loaded bundle.
pub fn collect_retired(lease: &mut StartupLease, loaded_bundle: &Path) -> io::Result<usize> {
    if !lease.can_activate() {
        return Ok(0);
    }
    let root = root(&lease.home)?;
    validate_lease_file(&lease.file, &root)?;
    let _lock = Lock::acquire(&root)?;
    let loaded = metadata(loaded_bundle)?;
    if !loaded.is_dir() {
        return Err(refuse("loaded application directory unavailable"));
    }
    let loaded_volume = volume_uuid(loaded_bundle)?;
    let pending = optional_json(&root.join("prepared.json"))?;
    let mut reclaimed = 0;
    // Bound each startup. Small audit files stay; later startups continue other payloads.
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.len() != 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            continue;
        }
        if pending.as_ref().is_some_and(|p| p.archive_sha256 == name) {
            continue;
        }
        let transaction = entry.path();
        owned_dir(&transaction)?;
        let incoming = transaction.join("incoming.app");
        let retired = transaction.join("retiring.app");
        let journal = transaction.join("retirement.json");
        let record: Retirement = match read_document(&journal) {
            Ok(record) => record,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if metadata(&retired).is_ok() {
                    return Err(refuse("unreceipted retirement payload"));
                }
                let m = match metadata(&incoming) {
                    Ok(m) => m,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e),
                };
                if (m.dev(), m.ino()) == (loaded.dev(), loaded.ino()) {
                    continue;
                }
                let previous = optional_json(&transaction.join("previous.json"))?;
                let verified = optional_json(&transaction.join("verified.json"))?;
                if let Some(p) = previous.filter(|p| matches(&incoming, p).unwrap_or(false)) {
                    p.valid()?;
                    if p.archive_sha256 != name {
                        return Err(refuse("previous payload transaction mismatch"));
                    }
                } else if let Some(p) = verified {
                    if p.archive_sha256 != name || !matches(&incoming, &p)? {
                        return Err(refuse(
                            "retained updater payload no longer matches its receipt",
                        ));
                    }
                } else {
                    // A partial extraction is authorized only by its original allocation
                    // inode, never by a directory name or elapsed time.
                    let allocated: Retirement = read_document(&transaction.join("allocated.json"))?;
                    allocated.check(&transaction, &incoming)?;
                }
                let record = Retirement::capture(&transaction, &incoming)?;
                write_document(&transaction, "retirement.json", &record)?;
                record
            }
            Err(e) => return Err(e),
        };
        let payload = match metadata(&retired) {
            Ok(_) => retired.clone(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => match metadata(&incoming) {
                Ok(_) => {
                    record.check(&transaction, &incoming)?;
                    if (record.payload_volume.as_str(), record.payload_ino)
                        == (loaded_volume.as_str(), loaded.ino())
                    {
                        continue;
                    }
                    fs::rename(&incoming, &retired)?;
                    sync(&transaction)?;
                    retired.clone()
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    sync(&transaction)?;
                    fs::rename(&journal, transaction.join("collected.json"))?;
                    sync(&transaction)?;
                    continue;
                }
                Err(e) => return Err(e),
            },
            Err(e) => return Err(e),
        };
        record.check(&transaction, &payload)?;
        if (record.payload_volume.as_str(), record.payload_ino)
            == (loaded_volume.as_str(), loaded.ino())
        {
            continue;
        }
        // remove_dir_all never follows directory symlinks. Replay is authorized by the
        // fixed payload inode and durable retirement receipt even after partial removal.
        fs::remove_dir_all(&payload)?;
        sync(&transaction)?;
        fs::rename(&journal, transaction.join("collected.json"))?;
        sync(&transaction)?;
        reclaimed += 1;
        if reclaimed == 8 {
            break;
        }
    }
    Ok(reclaimed)
}
/// Stage only bytes already accepted by `Update::download` signature verification.
/// Does not publish or modify the running app, including protected system installations.
pub fn stage_verified(home: &Path, bytes: &[u8], version: &str) -> io::Result<Published> {
    stage(home, bytes, version, Direction::Forward)
}
/// Stage a DELIBERATE RETURN to an earlier release -- same bytes contract as
/// [`stage_verified`], same extraction, same identity probe, same publication receipt.
///
/// The bytes must come from the same signature-verifying `Update::download` as any update.
/// That is the whole point of re-fetching the earlier tag instead of resurrecting a retained
/// copy: the payload that goes back on disk is one the compiled public key accepted a moment
/// ago, not one that has been sitting in a user-writable directory since the day it was
/// displaced.
///
/// It refuses unless `version` is strictly older than the publication receipt for the
/// application currently in `~/Applications`, and it writes the `rollback.json` that lets
/// startup activation accept that downgrade exactly once.
pub fn stage_rollback_verified(
    home: &Path,
    bytes: &[u8],
    version: &str,
) -> io::Result<Published> {
    stage(home, bytes, version, Direction::Back)
}
fn stage(
    home: &Path,
    bytes: &[u8],
    version: &str,
    direction: Direction,
) -> io::Result<Published> {
    let root = root(home)?;
    // Independent staging serialization and SH exclusion keep expensive extraction and
    // probing off the publication lock. Other normal startups can join SH immediately.
    let _stage = Lock::named(&root, "stage.lock")?;
    let mut session = StartupLease::acquire(home)?;
    session.begin_session()?;
    let offered = semver::Version::parse(version).map_err(|_| refuse("invalid offered version"))?;
    let archive_sha256 = digest(bytes);
    let superseded = match read_json(&root.join("prepared.json")) {
        Ok(p) => {
            let incoming_matches =
                matches(&root.join(&p.archive_sha256).join("incoming.app"), &p).unwrap_or(false);
            let published_matches = matches(&home.join("Applications").join(APP), &p)?;
            if p.archive_sha256 == archive_sha256
                && p.version == version
                && (incoming_matches || published_matches)
            {
                return Ok(p);
            }
            if !incoming_matches && quarantine_invalid_pending(&root, &p)? {
                None
            } else {
                // A ROLLBACK NEVER SUPERSEDES A PENDING PUBLICATION. The supersede rule is
                // "a newer offer wins", and a rollback is by definition not newer, so it
                // has no claim on a receipt somebody already prepared. Refusing is also the
                // honest answer on screen: the thing that is prepared is what the next
                // launch will do, and the way to change that is to take it first.
                if direction == Direction::Back {
                    return Err(refuse(
                        "an update is already prepared for the next launch; open RichOS again \
                         before going back",
                    ));
                }
                if !incoming_matches
                    || published_matches
                    || semver::Version::parse(&p.version)
                        .map_err(|_| refuse("invalid pending version"))?
                        >= offered
                {
                    return Err(refuse(
                        "pending update must finish recovery before it can be superseded",
                    ));
                }
                Some(p)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    // A ROLLBACK MAY ONLY GO WHERE THIS INSTALLATION'S OWN HISTORY SAYS IT CAME FROM.
    //
    // Checking merely that the offer is older would let any older release be staged by
    // anything that could reach this function with bytes and a number. Binding it to
    // `resolve_rollback` -- the `previous.json` the last exchange wrote before it swapped
    // the directories -- means the only reachable destination is the one version this Mac
    // was demonstrably running an update ago. It also puts the refusal BEFORE the
    // idempotent early return below, so "go back" to the version already installed is
    // refused rather than quietly answered with a receipt for what is already there.
    let mut from_version = None;
    if direction == Direction::Back {
        match resolve_rollback(&root)? {
            RollbackTarget::Available(previous) if previous == version => {}
            RollbackTarget::Available(previous) => {
                return Err(refuse(format!(
                    "this installation came from {previous}, not from {version}"
                )))
            }
            RollbackTarget::NotOlder(_) => {
                return Err(refuse(
                    "that version is not older than the one this Mac is running",
                ))
            }
            RollbackTarget::NoRecord => {
                return Err(refuse(
                    "this copy of RichOS has no record of an earlier version to go back to",
                ))
            }
        }
    }
    match read_json(&root.join("published.json")) {
        Ok(p) => {
            if p.archive_sha256 == archive_sha256
                && p.version == version
                && matches(&home.join("Applications").join(APP), &p)?
            {
                return Ok(p);
            }
            if direction == Direction::Forward
                && semver::Version::parse(&p.version)
                    .map_err(|_| refuse("invalid published version"))?
                    >= offered
            {
                return Err(refuse(
                    "an equal or newer user application is already published",
                ));
            }
            from_version = Some(p.version);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let transaction = root.join(&archive_sha256);
    // An interrupted pre-journal extraction is inert. Delete only this digest-named,
    // owner-private transaction; never touch the current application to recover it.
    if let Ok(m) = metadata(&transaction) {
        if !m.is_dir() || m.uid() != uid() || m.mode() & 0o077 != 0 {
            return Err(refuse("invalid update transaction"));
        }
        match read_json(&transaction.join("verified.json")) {
            Ok(p) => {
                if p.archive_sha256 != archive_sha256
                    || p.version != version
                    || !matches(&transaction.join("incoming.app"), &p)?
                {
                    // A SPENT TRANSACTION IS NOT A RETAINED APPLICATION, and until rollback
                    // existed nothing could reach this distinction: an archive was staged
                    // once and never again, so a transaction directory with a receipt but
                    // no payload was unreachable. Re-fetching an earlier tag lands on
                    // exactly that directory -- `collect_retired` reclaimed its payload
                    // and deliberately left its receipts behind. Its bytes are gone, so
                    // there is nothing here to protect; the refusal below is for a
                    // directory that still HOLDS an application, which this one does not.
                    if !spent(&transaction)? {
                        return Err(refuse(
                            "transaction retains a previous application; refusing to delete it",
                        ));
                    }
                } else {
                    probe(&transaction.join("incoming.app"), &p)?;
                    prepare_receipt(&root, &p, superseded.as_ref(), direction, &from_version)?;
                    return Ok(p);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        fs::remove_dir_all(&transaction)?;
    }
    fs::create_dir(&transaction)?;
    fs::set_permissions(&transaction, fs::Permissions::from_mode(0o700))?;
    sync(&root)?;
    let app = transaction.join("incoming.app");
    ensure_dir(&app, 0o755)?;
    write_document(
        &transaction,
        "allocated.json",
        &Retirement::capture(&transaction, &app)?,
    )?;
    extract(bytes, &app)?;
    let executable_name = bundle_identity(&app, Some(version))?;
    let p = Published {
        version: version.to_owned(),
        archive_sha256,
        tree_sha256: tree_hash(&app)?,
        executable_name,
    };
    probe(&app, &p)?;
    write_json(&transaction, "verified.json", &p)?;
    prepare_receipt(&root, &p, superseded.as_ref(), direction, &from_version)?;
    Ok(p)
}
/// True when a transaction directory holds no application payload at all -- neither the
/// staged `incoming.app` nor a `retiring.app` awaiting collection. Any other error is
/// propagated: "I could not tell" is never answered as "there is nothing there".
fn spent(transaction: &Path) -> io::Result<bool> {
    for name in ["incoming.app", "retiring.app"] {
        match metadata(&transaction.join(name)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
            Ok(_) => return Ok(false),
        }
    }
    Ok(true)
}
fn prepare_receipt(
    root: &Path,
    p: &Published,
    superseded: Option<&Published>,
    direction: Direction,
    from_version: &Option<String>,
) -> io::Result<()> {
    let _publication = Lock::acquire_startup(root)?;
    if let Some(old) = superseded {
        write_json(
            root,
            &format!("superseded-{}.json", old.archive_sha256),
            old,
        )?;
    }
    // THE AUTHORIZATION IS WRITTEN BEFORE THE JOURNAL IT AUTHORIZES, and cleared before a
    // journal it does not. A crash between the two leaves a `rollback.json` naming an
    // archive no `prepared.json` refers to, which `rollback_receipt` reads as no
    // authorization at all -- the safe direction of the only ordering that can be wrong.
    match direction {
        Direction::Back => {
            let from = from_version
                .clone()
                .ok_or_else(|| refuse("rollback needs the version it is leaving"))?;
            let receipt = Rollback {
                archive_sha256: p.archive_sha256.clone(),
                from_version: from,
            };
            receipt.valid()?;
            write_document(root, "rollback.json", &receipt)?;
        }
        Direction::Forward => clear_rollback(root)?,
    }
    write_json(root, "prepared.json", p)
}
/// Complete a prepared publication only before runtime startup, while every participating
/// session is excluded. This never runs from the live update command.
///
/// RENAMED FROM `activate_prepared_above` when rollback landed, because the old name stated
/// an invariant that is no longer the whole truth. It still activates only what is newer
/// than `running_version` -- unless a `rollback.json` written by the user's own "Go back"
/// authorizes exactly this receipt, in which case it activates only what is strictly OLDER.
/// A name that said `_above` over a function that can move down is worse than no name.
pub fn activate_prepared(
    lease: &mut StartupLease,
    running_version: &str,
) -> io::Result<Option<Activation>> {
    if !lease.can_activate() {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "another RichOS session is running",
        ));
    }
    let root = root(&lease.home)?;
    validate_lease_file(&lease.file, &root)?;
    let _lock = Lock::acquire_startup(&root)?;
    match read_json(&root.join("prepared.json")) {
        // An authorized rollback is DELIBERATELY not newer, so it must not be retired here
        // as an obsolete forward update. Whether it still applies is decided by `recover`,
        // against the bundle actually on disk, under the same lock as the exchange.
        Ok(p) if !p.is_newer_than(running_version)? && rollback_receipt(&root, &p)?.is_none() => {
            if matches(&lease.home.join("Applications").join(APP), &p)? {
                // The new image can be loaded after an exchange but before its receipt
                // is durable. Complete exactly that receipt without exchanging or exec.
                recover(&root)?;
                return Ok(None);
            }
            let transaction = root.join(&p.archive_sha256);
            owned_dir(&transaction)?;
            if !matches(&transaction.join("incoming.app"), &p)? {
                return Err(refuse(
                    "obsolete pending update has ambiguous publication state",
                ));
            }
            // A newer external build must be able to check for future releases. Keeping
            // this exact obsolete journal would make the live UI report readiness forever.
            fs::rename(
                root.join("prepared.json"),
                transaction.join("obsolete.json"),
            )?;
            sync(&transaction)?;
            sync(&root)?;
            return Ok(None);
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    }
    recover(&root)
}
/// Read a verified pending publication for the live UI without publishing it.
///
/// It reports the DIRECTION as well as the version, because the sentence on screen is not
/// the same sentence: "1.2.1 is ready for the next launch" and "RichOS will go back to
/// 1.2.0 when you next open it" are different promises and the UI must not have to infer
/// which one it is holding by comparing two numbers.
pub fn staged(home: &Path) -> io::Result<Option<Staged>> {
    let root = root(home)?;
    let _lock = Lock::acquire(&root)?;
    let p = match read_json(&root.join("prepared.json")) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !matches(&root.join(&p.archive_sha256).join("incoming.app"), &p)?
        && !matches(&home.join("Applications").join(APP), &p)?
    {
        return Err(refuse(
            "pending update no longer matches its verified receipt",
        ));
    }
    let rollback = rollback_receipt(&root, &p)?.is_some();
    Ok(Some(Staged {
        publication: p,
        rollback,
    }))
}
/// What the last activation did, for the boot line and the update surface. `None` before
/// this installation has ever activated anything (a hand-installed first copy).
pub fn last_activation(home: &Path) -> io::Result<Option<Activation>> {
    let root = root(home)?;
    let _lock = Lock::acquire(&root)?;
    match read_document::<Activation>(&root.join("last-activation.json")) {
        Ok(a) => {
            a.entered.valid()?;
            Ok(Some(a))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
/// THE VERSION THIS INSTALLATION WOULD GO BACK TO, read off its own publication history.
///
/// `recover` writes `previous.json` into the transaction directory before it exchanges the
/// bundle, so every update this Mac has applied left a durable note of what it displaced.
/// The COLLECTOR reclaims that transaction's payload bytes at a later startup -- it does not
/// remove the note -- which is why the version is still answerable long after the old
/// application is gone, and why a rollback has to re-fetch the bytes from the release rather
/// than hope for a retained copy.
///
/// Only the `version` field of that note is used. Its `archive_sha256` is the digest of the
/// archive that REPLACED it (the transaction's own name, by construction in `recover`), and
/// its `tree_sha256` describes a directory that no longer exists; both would be wrong
/// answers to "which release do I fetch". The tag is `v<version>`, and the bytes under it
/// are verified against the compiled public key like any other download.
pub fn rollback_target(home: &Path) -> io::Result<RollbackTarget> {
    let root = root(home)?;
    let _lock = Lock::acquire(&root)?;
    resolve_rollback(&root)
}
fn resolve_rollback(root: &Path) -> io::Result<RollbackTarget> {
    let Some(published) = optional_json(&root.join("published.json"))? else {
        return Ok(RollbackTarget::NoRecord);
    };
    let Some(previous) = optional_json(&root.join(&published.archive_sha256).join("previous.json"))?
    else {
        return Ok(RollbackTarget::NoRecord);
    };
    if !older(&previous.version, &published.version)? {
        return Ok(RollbackTarget::NotOlder(previous.version));
    }
    Ok(RollbackTarget::Available(previous.version))
}
/// WAIT UNTIL NO RICHOS SESSION HOLDS THE SESSION LEASE, so a relaunch can activate.
///
/// Activation needs the EXCLUSIVE side of `session.lock` at startup, and every running app
/// keeps the shared side until its process ends. A relaunch started while the previous
/// process is still exiting would take the shared side, skip activation, and come back on
/// the old version. So the relaunch helper asks this first. `true` means the lease could be
/// taken exclusively at that instant (it is released again at once, before returning);
/// `false` means `within` passed with a session still holding it. A missing lease file has
/// no holder, so it is `true`.
pub fn wait_for_sessions_to_end(home: &Path, within: std::time::Duration) -> io::Result<bool> {
    let path = root(home)?.join("session.lock");
    let deadline = std::time::Instant::now() + within;
    loop {
        let free = lock_step(|| -> io::Result<bool> {
            let file = match OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&path)
            {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(true),
                Err(e) => return Err(e),
            };
            // Closing the file (dropped on return) releases the exclusive side again.
            // lock-busy: one poll inside the caller's bounded `within`; busy past it is the answer false (a session still runs).
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(true);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                Ok(false)
            } else {
                Err(error)
            }
        })?;
        if free {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
#[cfg(test)]
fn install_verified(home: &Path, bytes: &[u8], version: &str) -> io::Result<Published> {
    let p = stage_verified(home, bytes, version)?;
    let mut lease = StartupLease::acquire(home)?;
    activate_prepared(&mut lease, "0.0.0")?;
    Ok(p)
}
/// Validate the recorded preferred user app, including its complete tree, before a
/// startup redirect or relaunch. Returns None when no user update has been installed.
/// This read does not complete a prepared update or replace a running app.
pub fn preferred(home: &Path) -> io::Result<Option<Published>> {
    with_preferred(home, |p| Ok(p.clone()))
}
/// Hold the publication lock from validation through the caller's executable spawn.
/// The callback must not call another updater API or wait for the new app to exit.
pub fn with_preferred<T>(
    home: &Path,
    action: impl FnOnce(&Published) -> io::Result<T>,
) -> io::Result<Option<T>> {
    let root = root(home)?;
    let _lock = Lock::acquire_startup(&root)?;
    let p = match read_json(&root.join("published.json")) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let app = home.join("Applications").join(APP);
    if bundle_identity(&app, Some(&p.version))? != p.executable_name || !matches(&app, &p)? {
        return Err(refuse(
            "preferred application does not match publication receipt",
        ));
    }
    Ok(Some(action(&p)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};

    // Every fixture launch must participate, including ordinary Command spawns:
    // on macOS those can briefly inherit another thread's session descriptor too.
    // Coordinate only spawn/exec, never the child's runtime or the entire test.
    // Participating makes every fixture launch a fork with a pre-exec step (the early
    // release of inherited files), so each one is covered the same way `probe()` is.
    trait FixtureCommandExt {
        fn spawn_fixture(&mut self) -> io::Result<std::process::Child>;
    }
    impl FixtureCommandExt for std::process::Command {
        fn spawn_fixture(&mut self) -> io::Result<std::process::Child> {
            spawn_past_lock_steps(self)
        }
    }
    fn home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn canonical(t: &tempfile::TempDir) -> PathBuf {
        t.path().canonicalize().unwrap()
    }
    fn add(t: &mut tar::Builder<GzEncoder<Vec<u8>>>, name: &str, body: &[u8], mode: u32) {
        let mut h = tar::Header::new_gnu();
        h.set_size(body.len() as u64);
        h.set_mode(mode);
        h.set_cksum();
        t.append_data(&mut h, name, body).unwrap();
    }
    fn fixture_binary(version: &str) -> Vec<u8> {
        static BINARIES: std::sync::OnceLock<
            std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
        > = std::sync::OnceLock::new();
        let mut binaries = BINARIES.get_or_init(Default::default).lock().unwrap();
        if let Some(bytes) = binaries.get(version) {
            return bytes.clone();
        }
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("identity.c");
        let executable = temp.path().join("identity");
        let response =
            serde_json::json!({"identifier":ID,"version":version,"protocol":1}).to_string();
        fs::write(&source, format!("#include <stdio.h>\n#include <string.h>\nint main(int n,char**v){{if(n!=2||strcmp(v[1],\"--richos-internal-update-identity\"))return 73;puts({:?});return 0;}}\n", response)).unwrap();
        let result = std::process::Command::new("/usr/bin/cc")
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn_fixture()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert!(
            result.status.success(),
            "native fixture compiler: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = fs::read(executable).unwrap();
        binaries.insert(version.to_owned(), bytes.clone());
        bytes
    }
    fn fixture_version(executable: PathBuf) -> PathBuf {
        executable
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("Resources/fixture-version")
    }
    fn archive(
        version: &str,
        extra: impl FnOnce(&mut tar::Builder<GzEncoder<Vec<u8>>>),
    ) -> Vec<u8> {
        archive_with_binary(version, &fixture_binary(version), extra)
    }
    fn archive_with_binary(
        version: &str,
        binary: &[u8],
        extra: impl FnOnce(&mut tar::Builder<GzEncoder<Vec<u8>>>),
    ) -> Vec<u8> {
        let mut t = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let info = format!(
            r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.richos.app</string><key>CFBundleShortVersionString</key><string>{version}</string><key>CFBundleExecutable</key><string>richos-tauri</string></dict></plist>"#
        );
        add(
            &mut t,
            "RichOS.app/Contents/Info.plist",
            info.as_bytes(),
            0o644,
        );
        add(
            &mut t,
            "RichOS.app/Contents/MacOS/richos-tauri",
            binary,
            0o755,
        );
        add(
            &mut t,
            "RichOS.app/Contents/Resources/fixture-version",
            version.as_bytes(),
            0o644,
        );
        extra(&mut t);
        t.into_inner().unwrap().finish().unwrap()
    }
    fn link(
        t: &mut tar::Builder<GzEncoder<Vec<u8>>>,
        name: &str,
        target: &str,
        kind: tar::EntryType,
    ) {
        let mut h = tar::Header::new_gnu();
        h.set_size(0);
        h.set_mode(0o777);
        h.set_entry_type(kind);
        h.set_link_name(target).unwrap();
        h.set_cksum();
        t.append_data(&mut h, name, io::empty()).unwrap();
    }
    #[test]
    fn nightly_versions_survive_install_and_order_numerically() {
        let t = home();
        let h = canonical(&t);
        let versions = [
            "5.1.0-nightly.20260911.9",
            "5.1.0-nightly.20260911.10",
            "5.1.0-nightly.20260912.1",
            "5.1.0",
        ];
        for (index, version) in versions.iter().enumerate() {
            let published = install_verified(&h, &archive(version, |_| {}), version).unwrap();
            assert_eq!(published.version, *version);
            assert_eq!(
                bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
                *version
            );
            if index > 0 {
                assert!(published.is_newer_than(versions[index - 1]).unwrap());
                assert!(install_verified(
                    &h,
                    &archive(versions[index - 1], |_| {}),
                    versions[index - 1]
                )
                .is_err());
                assert_eq!(
                    bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
                    *version
                );
            }
        }
        assert!(stage_verified(
            &h,
            &archive("5.1.0-nightly.20260912.01", |_| {}),
            "5.1.0-nightly.20260912.01"
        )
        .is_err());
    }

    /// `package-app.sh`'s undeclared-intent (development-build) default stamps
    /// `{cargo-version}-dev.{shortsha}[.dirty]` — see its "THE VERSION THIS BUILD WILL
    /// CARRY" section. This pins the property the updater needs from that shape: semver
    /// precedence (the `semver` crate's `Ord`, which `Published::is_newer_than` above uses
    /// directly via `this > other`) puts ANY pre-release identifier strictly below the
    /// release it precedes, with no special-casing required here. A dev copy must always
    /// be told the real release is an update, and the real release must never be told a
    /// dev copy of itself is newer — the same property `nightly_versions_survive_install_
    /// and_order_numerically` above already proves for `-nightly.` builds, pinned here for
    /// `-dev.` ones because nothing had exercised that shape until now.
    #[test]
    fn dev_versions_are_older_than_the_release_they_precede() {
        let t = home();
        let h = canonical(&t);
        let dev = "1.2.0-dev.0c00cd87";
        let dev_dirty = "1.2.0-dev.0c00cd87.dirty";
        let release = "1.2.0";

        let published_dev = install_verified(&h, &archive(dev, |_| {}), dev).unwrap();
        assert_eq!(published_dev.version, dev);
        assert!(
            !published_dev.is_newer_than(release).unwrap(),
            "a dev build must never claim to be newer than the release it precedes"
        );
        let as_release = Published {
            version: release.into(),
            ..published_dev.clone()
        };
        assert!(
            as_release.is_newer_than(dev).unwrap(),
            "the release must always be offered as an update over a dev copy of itself"
        );
        // A dirtier stamp of the very same commit still never outranks the release.
        assert!(as_release.is_newer_than(dev_dirty).unwrap());
        assert!(!published_dev.is_newer_than(dev_dirty).unwrap());

        // And the release genuinely installs OVER the dev copy, exactly like any other
        // upgrade — this is the path `updates.rs::check` exercises on a customer's Mac.
        let published_release = install_verified(&h, &archive(release, |_| {}), release).unwrap();
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            release
        );
        assert!(published_release.is_newer_than(dev).unwrap());
    }

    #[test]
    fn installs_as_actual_owner_and_replays_exact_receipt() {
        let t = home();
        let h = canonical(&t);
        let bytes = archive("2.0.0", |_| {});
        let p = install_verified(&h, &bytes, "2.0.0").unwrap();
        assert_eq!(
            fs::read(fixture_version(p.executable(&h))).unwrap(),
            b"2.0.0"
        );
        assert_eq!(metadata(&p.executable(&h)).unwrap().uid(), uid());
        assert_eq!(
            install_verified(&h, &bytes, "2.0.0").unwrap().tree_sha256,
            p.tree_sha256
        );
        assert_eq!(preferred(&h).unwrap().unwrap().tree_sha256, p.tree_sha256);
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn atomic_exchange_retains_old_application() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let bytes = archive("2.0.0", |_| {});
        let p = install_verified(&h, &bytes, "2.0.0").unwrap();
        assert_eq!(
            fs::read(fixture_version(p.executable(&h))).unwrap(),
            b"2.0.0"
        );
        let backup = root(&h)
            .unwrap()
            .join(digest(&bytes))
            .join("incoming.app/Contents/Resources/fixture-version");
        assert_eq!(fs::read(backup).unwrap(), b"1.0.0");
    }
    #[test]
    fn wrong_version_cannot_publish() {
        let t = home();
        let h = canonical(&t);
        assert!(install_verified(&h, &archive("1.0.0", |_| {}), "2.0.0").is_err());
        assert!(!h.join("Applications/RichOS.app").exists());
    }
    #[test]
    fn symlinked_user_destination_is_not_followed() {
        let t = home();
        let h = canonical(&t);
        let outside = home();
        symlink(outside.path(), h.join("Applications")).unwrap();
        assert!(install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
    #[test]
    fn destination_permission_denial_is_plain_io_failure() {
        let t = home();
        let h = canonical(&t);
        let a = h.join("Applications");
        fs::create_dir(&a).unwrap();
        fs::set_permissions(&a, fs::Permissions::from_mode(0o555)).unwrap();
        let result = install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0");
        fs::set_permissions(&a, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert!(!a.join(APP).exists());
    }
    #[test]
    fn safe_framework_links_preserve_topology() {
        let t = home();
        let h = canonical(&t);
        let bytes = archive("1.0.0", |t| {
            add(
                t,
                "RichOS.app/Contents/Frameworks/F.framework/Versions/A/F",
                b"framework",
                0o755,
            );
            link(
                t,
                "RichOS.app/Contents/Frameworks/F.framework/Versions/Current",
                "A",
                tar::EntryType::Symlink,
            );
            link(
                t,
                "RichOS.app/Contents/Frameworks/F.framework/F",
                "Versions/Current/F",
                tar::EntryType::Symlink,
            );
        });
        install_verified(&h, &bytes, "1.0.0").unwrap();
        assert_eq!(
            fs::read(h.join("Applications/RichOS.app/Contents/Frameworks/F.framework/F")).unwrap(),
            b"framework"
        );
    }
    #[test]
    fn escaping_links_and_hard_links_are_refused() {
        for (target, kind) in [
            ("../../../escape", tar::EntryType::Symlink),
            ("/tmp/escape", tar::EntryType::Symlink),
            (
                "RichOS.app/Contents/MacOS/richos-tauri",
                tar::EntryType::Link,
            ),
        ] {
            let t = home();
            let h = canonical(&t);
            let bytes = archive("1.0.0", |t| {
                link(t, "RichOS.app/Contents/link", target, kind)
            });
            assert!(install_verified(&h, &bytes, "1.0.0").is_err());
            assert!(!h.join("Applications/RichOS.app").exists());
        }
    }
    #[test]
    fn symlink_cannot_become_a_write_parent() {
        let t = home();
        let h = canonical(&t);
        let bytes = archive("1.0.0", |t| {
            link(
                t,
                "RichOS.app/Contents/alias",
                "MacOS",
                tar::EntryType::Symlink,
            );
            add(t, "RichOS.app/Contents/alias/extra", b"no", 0o644);
        });
        assert!(install_verified(&h, &bytes, "1.0.0").is_err());
        assert!(!h.join("Applications/RichOS.app").exists());
    }
    #[test]
    fn duplicate_entries_and_second_archive_root_are_refused() {
        for name in [
            "Other.app/Contents/file",
            "RichOS.app/Contents/MacOS/richos-tauri",
        ] {
            let t = home();
            let h = canonical(&t);
            assert!(install_verified(
                &h,
                &archive("1.0.0", |t| add(t, name, b"no", 0o644)),
                "1.0.0"
            )
            .is_err());
        }
    }
    #[test]
    fn changed_published_tree_cannot_relaunch() {
        let t = home();
        let h = canonical(&t);
        let p = install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        fs::write(p.executable(&h), b"tampered").unwrap();
        assert!(preferred(&h).is_err());
    }
    #[test]
    fn interrupted_extraction_is_rebuilt_without_touching_current_app() {
        let t = home();
        let h = canonical(&t);
        let bytes = archive("1.0.0", |_| {});
        let r = root(&h).unwrap();
        let tx = r.join(digest(&bytes));
        fs::create_dir(&tx).unwrap();
        fs::set_permissions(&tx, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(tx.join("incomplete"), b"partial").unwrap();
        install_verified(&h, &bytes, "1.0.0").unwrap();
        assert!(!tx.join("incomplete").exists());
    }
    fn prepare(h: &Path, bytes: &[u8], version: &str) -> (PathBuf, Published) {
        let r = root(h).unwrap();
        let tx = r.join(digest(bytes));
        ensure_dir(&tx, 0o700).unwrap();
        let incoming = tx.join("incoming.app");
        extract(bytes, &incoming).unwrap();
        let p = Published {
            version: version.into(),
            archive_sha256: digest(bytes),
            tree_sha256: tree_hash(&incoming).unwrap(),
            executable_name: bundle_identity(&incoming, Some(version)).unwrap(),
        };
        write_json(&tx, "verified.json", &p).unwrap();
        write_json(&r, "prepared.json", &p).unwrap();
        (r, p)
    }
    #[test]
    fn prepared_first_install_recovers_after_journal_crash() {
        let t = home();
        let h = canonical(&t);
        let bytes = archive("1.0.0", |_| {});
        let (_r, p) = prepare(&h, &bytes, "1.0.0");
        assert_eq!(
            install_verified(&h, &bytes, "1.0.0").unwrap().tree_sha256,
            p.tree_sha256
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn exchange_before_receipt_crash_recovers_without_swapping_back() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let bytes = archive("2.0.0", |_| {});
        let (r, p) = prepare(&h, &bytes, "2.0.0");
        let incoming = r.join(&p.archive_sha256).join("incoming.app");
        exchange(&incoming, &h.join("Applications/RichOS.app")).unwrap();
        SYNC_TRACE.with(|trace| trace.borrow_mut().clear());
        install_verified(&h, &bytes, "2.0.0").unwrap();
        SYNC_TRACE.with(|trace| {
            let trace = trace.borrow();
            assert_eq!(trace[0], incoming.parent().unwrap());
            assert_eq!(trace[1], h.join("Applications"));
            // The root receipt sync happens only after both rename directories.
            assert_eq!(trace[2], r);
        });
        assert_eq!(
            fs::read(fixture_version(p.executable(&h))).unwrap(),
            b"2.0.0"
        );
        assert_eq!(
            fs::read(incoming.join("Contents/Resources/fixture-version")).unwrap(),
            b"1.0.0"
        );
    }
    #[test]
    fn stale_old_instance_cannot_downgrade_preferred_app() {
        let t = home();
        let h = canonical(&t);
        let latest = install_verified(&h, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        assert!(install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").is_err());
        assert_eq!(
            fs::read(fixture_version(latest.executable(&h))).unwrap(),
            b"3.0.0"
        );
    }
    #[test]
    fn exact_preferred_callback_holds_publication_exclusion() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        with_preferred(&h, |p| {
            assert_eq!(
                p.executable(&h),
                h.join("Applications/RichOS.app/Contents/MacOS/richos-tauri")
            );
            assert!(install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").is_err());
            Ok(())
        })
        .unwrap()
        .unwrap();
    }
    // =====================================================================================
    // THE RACE THAT COST A NIGHTLY BUILD, MADE DETERMINISTIC
    // =====================================================================================
    //
    // It arrived as three different tests failing on three different runs, always under
    // load, always `WouldBlock: "another RichOS session is running"` over a session that
    // had ended. Two copies of this test binary at once, 8 threads each: one of the two
    // red. The same two processes with `--test-threads 1`: both green. So it was never two
    // checkouts colliding over something machine-wide -- every fixture home here is its own
    // `tempdir` -- it was two THREADS, one of them forking.
    //
    // `flock` exclusion belongs to the open file description. `fork` gives the child a
    // reference to every description the process holds, and `O_CLOEXEC` takes them back
    // only at `exec`. In between, a probe forked by one thread holds another thread's
    // `session.lock`, and that thread's release releases nothing.
    //
    // The load did not cause it; it only widened the window. So this test parks a child
    // INSIDE that window on purpose and asks the question the product asks. The sleep is
    // the defect being held still to be looked at -- not a wait for something to succeed:
    // the test waits on the child's own byte through a pipe, never on a duration, and it
    // asserts the property rather than the timing.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_child_parked_between_fork_and_exec_cannot_hold_a_released_lease() {
        fixture_spawn_releases_lease(false);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_fixture_child_cannot_keep_a_released_shared_session_alive() {
        fixture_spawn_releases_lease(true);
    }

    #[cfg(target_os = "macos")]
    fn fixture_spawn_releases_lease(shared: bool) {
        use std::os::unix::{net::UnixStream, process::CommandExt};
        let t = home();
        let h = canonical(&t);

        let mut lease = StartupLease::acquire(&h).unwrap();
        assert!(lease.can_activate(), "the first session owns the lease");
        if shared {
            lease.begin_session().unwrap();
        }

        // Owned, close-on-exec endpoints, with a bound if the child never reports in.
        let (mut reader, writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let writer_fd = writer.as_raw_fd();

        let mut command = std::process::Command::new("/bin/sleep");
        command
            .arg("0")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        unsafe {
            command.pre_exec(move || {
                // In the CHILD, after fork and before exec: it holds a copy of every open
                // description in the parent, including the lease acquired above.
                let byte = b".";
                if libc::write(writer_fd, byte.as_ptr().cast(), 1) != 1 {
                    return Err(io::Error::last_os_error());
                }
                let delay = libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 300_000_000,
                };
                if libc::nanosleep(&delay, std::ptr::null_mut()) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let forker = std::thread::spawn(move || {
            let status = command.spawn_fixture().and_then(|mut child| child.wait());
            drop(writer);
            status
        });

        let mut byte = [0u8; 1];
        reader
            .read_exact(&mut byte)
            .expect("the child never reached its parked state, so nothing was proven");

        // The session ends while that child is still parked.
        drop(lease);

        // And the next session starts. Before this repair it read the parked child's
        // inherited copy and refused itself: `can_activate()` false, which every caller
        // reports as "another RichOS session is running".
        // Both halves of the same defect: before the repair the parked child held the
        // description EXCLUSIVE, so the next session could take neither EX nor SH and the
        // acquisition itself failed WouldBlock. A softer variant would return a lease that
        // merely cannot activate. Neither may happen, and neither may fail namelessly.
        let next = StartupLease::acquire(&h);
        // Reap the child even when the captured acquisition proves the regression.
        assert!(forker.join().unwrap().unwrap().success());
        let next = next.unwrap_or_else(|error| {
            panic!(
                "a released lease was still held by a child parked between fork and exec, \
so the next session could not take it at all: {error}"
            )
        });
        assert!(
            next.can_activate(),
            "a released lease was still held SHARED by a child parked between fork and \
exec, so the next session refused to activate over a session that had ended"
        );
        drop(next);
    }

    // =====================================================================================
    // THE SAME WINDOW, FROM A LAUNCH THAT HAS ALREADY RETURNED
    // =====================================================================================
    //
    // Nightly .31 (2026-09-29, `gates/updater-tests` beside four other gates): two rollback
    // tests, both WouldBlock "another RichOS session is running" on their SECOND
    // `install_verified`, with the parked-child repair above in place. That repair holds
    // every lock observation off until `spawn` returns. But a launch with a pre-exec step --
    // `probe()`, and the parked child above -- is a real fork, and its `spawn` returns when
    // its own close-on-exec status pipe reaches end-of-file. On macOS that end-of-file
    // overtakes the release of the lock on the other close-on-exec descriptors exec is
    // sweeping, so a lease the child inherited can still be held after `spawn` returned.
    //
    // Descriptors numbered between the lease and that status pipe hold the sweep open long
    // enough to see it every time: sockets whose LAST reference is the child's, because
    // this process drops its own copies while the child is parked before exec, so exec's
    // sweep has to tear each one down. They are not files, so the repair does not close them
    // early, and the window is exactly as wide on both sides of it. Measured with raw
    // `flock` before the repair, 8000 of them: the released lease was still held for 1.8
    // to 8.7 ms after `spawn` returned, in 10 rounds of 10 -- far longer than the next
    // `StartupLease::acquire` takes to reach its `flock`. After the repair: 0 of 10.
    //
    // It runs alone, in a child test process, because descriptor NUMBERING is the whole
    // mechanism: other tests opening and closing files beside it would move the numbers.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_launch_that_has_returned_holds_no_released_lease() {
        const MARKER: &str = "RICHOS_TEST_LAUNCH_RETURNED";
        const NAME: &str = "tests::a_launch_that_has_returned_holds_no_released_lease";
        if std::env::var_os(MARKER).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env(MARKER, "1")
                .spawn_fixture()
                .unwrap()
                .wait()
                .unwrap();
            assert!(
                status.success(),
                "the isolated run failed; its panic is above"
            );
            return;
        }
        use std::os::unix::{net::UnixStream, process::CommandExt};
        const BETWEEN: usize = 16_000;
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
            0
        );
        let wanted = BETWEEN as libc::rlim_t + 512;
        if limit.rlim_cur < wanted {
            limit.rlim_cur = wanted.min(limit.rlim_max);
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
        }
        assert!(
            limit.rlim_cur >= wanted,
            "cannot open {BETWEEN} descriptors here"
        );

        let t = home();
        let h = canonical(&t);
        for round in 1..=3 {
            let lease = StartupLease::acquire(&h).unwrap();
            assert!(
                lease.can_activate(),
                "round {round}: a session is already held"
            );
            let between: Vec<(UnixStream, UnixStream)> = (0..BETWEEN / 2)
                .map(|_| UnixStream::pair().unwrap())
                .collect();
            // Owned, close-on-exec endpoints, with a bound if the child never reports in.
            let (mut reader, writer) = UnixStream::pair().unwrap();
            reader
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let writer_fd = writer.as_raw_fd();
            let mut command = std::process::Command::new("/usr/bin/true");
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            // A pre-exec step is what makes this the fork `probe()` is, not a posix_spawn.
            // This one reports in and waits for this process to let go of the sockets.
            unsafe {
                command.pre_exec(move || {
                    if libc::write(writer_fd, b".".as_ptr().cast(), 1) != 1 {
                        return Err(io::Error::last_os_error());
                    }
                    let delay = libc::timespec {
                        tv_sec: 0,
                        tv_nsec: 100_000_000,
                    };
                    if libc::nanosleep(&delay, std::ptr::null_mut()) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let launcher = std::thread::spawn(move || {
                let child = command.spawn_fixture();
                drop(writer);
                child
            });
            let mut byte = [0u8; 1];
            reader
                .read_exact(&mut byte)
                .expect("the child never reached its pre-exec step, so nothing was proven");
            drop(between);
            let mut child = launcher.join().unwrap().unwrap();

            // `spawn` has returned. The session ends, and the next one starts at once.
            drop(lease);
            let next = StartupLease::acquire(&h);
            // Reap the child even when the captured acquisition proves the regression.
            assert!(child.wait().unwrap().success());
            let next = next.unwrap_or_else(|error| {
                panic!(
                    "round {round}: a launch had already returned, yet its child still held \
the released lease, so the next session could not take it at all: {error}"
                )
            });
            assert!(
                next.can_activate(),
                "round {round}: a launch had already returned, yet its child still held the \
released lease, so the next session refused to activate over a session that had ended"
            );
        }
    }

    // A REAL SECOND SESSION IS STILL REFUSED. The repair above is about a child that is
    // not a session at all; it must not have bought quiet by weakening the exclusion this
    // whole lease exists for.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_fork_window_repair_still_refuses_a_genuine_second_session() {
        let t = home();
        let h = canonical(&t);
        let mut first = StartupLease::acquire(&h).unwrap();
        assert!(first.can_activate());
        first.begin_session().unwrap();
        let second = StartupLease::acquire(&h).unwrap();
        assert!(
            !second.can_activate(),
            "a session is running, so the next startup may not activate over it"
        );
    }

    /// A relaunch waits for the running session to end, and only then sees a free lease.
    #[test]
    fn waiting_for_sessions_sees_a_running_session_and_its_end() {
        let t = home();
        let h = canonical(&t);
        assert!(
            wait_for_sessions_to_end(&h, std::time::Duration::ZERO).unwrap(),
            "no lease file yet, so no session holds it"
        );
        let mut running = StartupLease::acquire(&h).unwrap();
        running.begin_session().unwrap();
        let started = std::time::Instant::now();
        assert!(
            !wait_for_sessions_to_end(&h, std::time::Duration::from_millis(200)).unwrap(),
            "a running session holds the shared side, so activation would be skipped"
        );
        assert!(started.elapsed() >= std::time::Duration::from_millis(200));
        drop(running);
        assert!(
            wait_for_sessions_to_end(&h, std::time::Duration::from_millis(200)).unwrap(),
            "the session ended, so the relaunch can activate"
        );
        let next = StartupLease::acquire(&h).unwrap();
        assert!(next.can_activate(), "waiting must not leave the lease held");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn harmless_deny_acl_works_but_write_grant_refuses() {
        let t = home();
        let h = canonical(&t);
        let chmod = |rule: &str| {
            assert!(std::process::Command::new("/bin/chmod")
                .args(["+a", rule])
                .arg(&h)
                .spawn_fixture()
                .unwrap()
                .wait()
                .unwrap()
                .success())
        };
        chmod("everyone deny delete");
        assert!(install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").is_ok());
        chmod("everyone allow add_file");
        assert!(install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").is_err());
        assert!(std::process::Command::new("/bin/chmod")
            .arg("-N")
            .arg(&h)
            .spawn_fixture()
            .unwrap()
            .wait()
            .unwrap()
            .success());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn write_acl_on_published_executable_prevents_launch() {
        let t = home();
        let h = canonical(&t);
        let p = install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let executable = p.executable(&h);
        assert!(std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow write"])
            .arg(&executable)
            .spawn_fixture()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        assert!(preferred(&h).is_err());
    }
    /// THE 2026-10-21 FIRST-RUN DEFECT, as a test: a READER of the publication journal that
    /// finds the lock held by somebody else for a moment WAITS for it. Before the fix, each of
    /// these three failed on the first try with `EWOULDBLOCK` -- "Resource temporarily
    /// unavailable (os error 35)" -- which the update row showed a new user as "RichOS could
    /// not prepare this update." while nothing was being prepared.
    #[test]
    fn journal_readers_wait_for_a_short_hold_instead_of_failing() {
        let t = home();
        let h = canonical(&t);
        let r = root(&h).unwrap();
        for reader in ["staged", "last_activation", "rollback_target"] {
            // The other holder is ANOTHER THREAD, as in the app (the launch check and the
            // window's `update_state`). It takes the lock, says so, and lets go 300 ms later.
            let (held_tx, held_rx) = std::sync::mpsc::channel();
            let holder_root = r.clone();
            let releaser = std::thread::spawn(move || {
                let held = Lock::acquire(&holder_root).unwrap();
                held_tx.send(()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(300));
                drop(held);
            });
            held_rx.recv().unwrap();
            let answer = match reader {
                "staged" => staged(&h).map(|_| ()),
                "last_activation" => last_activation(&h).map(|_| ()),
                _ => rollback_target(&h).map(|_| ()),
            };
            releaser.join().unwrap();
            assert!(answer.is_ok(), "{reader} failed while the lock was held for 300 ms: {answer:?}");
        }
    }
    /// A SECOND TAKE ON THE THREAD THAT HOLDS THE LOCK IS REFUSED AT ONCE. `flock` conflicts
    /// between two descriptions even inside one process, so waiting could only end in the
    /// same failure [`PUBLICATION_WAIT`] later; `update_startup.rs` read `last_activation`
    /// inside `with_preferred`'s callback and would have paid that on every launch. A refusal
    /// that is not `WouldBlock` is one that did not wait for the deadline: past it, the answer
    /// is `WouldBlock`.
    #[test]
    fn a_second_take_on_the_holding_thread_is_refused_at_once_not_after_the_wait() {
        let t = home();
        let h = canonical(&t);
        let r = root(&h).unwrap();
        let held = Lock::acquire(&r).unwrap();
        let error = last_activation(&h).expect_err("a second take on the holding thread succeeded");
        assert_ne!(error.kind(), io::ErrorKind::WouldBlock, "it was treated as busy: {error}");
        drop(held);
        assert!(last_activation(&h).is_ok(), "the lock was not released on drop");
    }
    /// What a caller hears when the lock stays busy for the whole wait: `WouldBlock`, in words
    /// that say what was busy, never the bare OS text.
    #[test]
    fn a_lock_still_busy_after_the_wait_says_so_in_words() {
        let t = home();
        let h = canonical(&t);
        let r = root(&h).unwrap();
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let holder_root = r.clone();
        let holder = std::thread::spawn(move || {
            let held = Lock::acquire(&holder_root).unwrap();
            held_tx.send(()).unwrap();
            // Err only when the test has already failed and dropped its sender.
            done_rx.recv().unwrap_or(());
            drop(held);
        });
        held_rx.recv().unwrap();
        let error = staged(&h).expect_err("staged() succeeded while another thread held the lock");
        done_tx.send(()).unwrap();
        holder.join().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(error.to_string(), PUBLICATION_BUSY);
        assert!(!error.to_string().contains("os error"), "{error}");
    }
    #[test]
    fn concurrent_install_cannot_modify_existing_journal() {
        let t = home();
        let h = canonical(&t);
        let r = root(&h).unwrap();
        let _lock = Lock::acquire(&r).unwrap();
        assert!(install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").is_err());
        assert!(!r.join("prepared.json").exists());
    }
    #[test]
    fn malformed_receipt_and_external_receipt_links_refuse() {
        let t = home();
        let h = canonical(&t);
        let r = root(&h).unwrap();
        fs::write(r.join("published.json"), b"{}").unwrap();
        assert!(install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").is_err());
        fs::remove_file(r.join("published.json")).unwrap();
        let outside = h.join("outside");
        fs::write(&outside, b"never modified").unwrap();
        symlink(&outside, r.join("published.json")).unwrap();
        assert!(install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"never modified");
    }
    #[test]
    fn stage_does_not_replace_live_bundle_and_shared_sessions_veto_activation() {
        let t = home();
        let h = canonical(&t);
        let old = install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let mut live = StartupLease::acquire(&h).unwrap();
        live.begin_session().unwrap();
        stage_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        assert_eq!(
            fs::read(fixture_version(old.executable(&h))).unwrap(),
            b"1.0.0"
        );
        let mut another = StartupLease::acquire(&h).unwrap();
        assert!(!another.can_activate());
        assert_eq!(
            activate_prepared(&mut another, "0.0.0")
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(staged(&h).unwrap().unwrap().publication.version, "2.0.0");
        drop(another);
        drop(live);
        let mut startup = StartupLease::acquire(&h).unwrap();
        activate_prepared(&mut startup, "0.0.0")
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read(fixture_version(old.executable(&h))).unwrap(),
            b"2.0.0"
        );
    }
    #[test]
    fn lease_adoption_rejects_unrelated_descriptor_without_closing_it() {
        let t = home();
        let h = canonical(&t);
        let _lease = StartupLease::acquire(&h).unwrap();
        let other = File::create(h.join("unrelated")).unwrap();
        assert!(StartupLease::adopt(&h, other.as_raw_fd()).is_err());
        assert!(other.metadata().is_ok());
    }
    #[test]
    fn replaced_lease_path_vetoes_activation() {
        let t = home();
        let h = canonical(&t);
        stage_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        let r = root(&h).unwrap();
        fs::rename(r.join("session.lock"), r.join("old-session.lock")).unwrap();
        File::create(r.join("session.lock")).unwrap();
        assert!(activate_prepared(&mut lease, "0.0.0").is_err());
        assert!(!h.join("Applications/RichOS.app").exists());
    }
    #[test]
    fn failed_exec_restores_cloexec_and_keeps_exclusion() {
        let t = home();
        let h = canonical(&t);
        let mut lease = StartupLease::acquire(&h).unwrap();
        let before = unsafe { libc::fcntl(lease.session_fd(), libc::F_GETFD) };
        let error = lease.exec(&mut std::process::Command::new(h.join("absent-executable")));
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(
            unsafe { libc::fcntl(lease.session_fd(), libc::F_GETFD) },
            before
        );
        assert!(StartupLease::acquire(&h).is_err());
    }
    #[test]
    fn lease_exec_child() {
        let Some(home) = std::env::var_os("RICHOS_TEST_UPDATE_EXEC_HOME") else {
            return;
        };
        let h = PathBuf::from(home);
        if let Ok(raw) = std::env::var("RICHOS_TEST_UPDATE_EXEC_FD") {
            let mut lease = StartupLease::adopt(&h, raw.parse().unwrap()).unwrap();
            assert!(!lease.can_activate());
            assert_ne!(
                unsafe { libc::fcntl(lease.session_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
                0
            );
            fs::write(h.join("ready"), b"held").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !h.join("release").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "parent did not release fixture"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            lease.begin_session().unwrap();
            return;
        }
        let mut lease = StartupLease::acquire(&h).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "tests::lease_exec_child", "--nocapture"])
            .env("RICHOS_TEST_UPDATE_EXEC_HOME", &h)
            .env("RICHOS_TEST_UPDATE_EXEC_FD", lease.session_fd().to_string());
        panic!("fixture exec failed: {}", lease.exec(&mut command));
    }
    #[test]
    fn actual_exec_preserves_session_exclusion_until_replacement_exits() {
        let t = home();
        let h = canonical(&t);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::lease_exec_child", "--nocapture"])
            .env("RICHOS_TEST_UPDATE_EXEC_HOME", &h)
            .env_remove("RICHOS_TEST_UPDATE_EXEC_FD")
            .stdout(std::process::Stdio::null())
            .spawn_fixture()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !h.join("ready").exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "fixture exited before transfer proof"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "fixture never transferred lease"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let observer = StartupLease::acquire(&h).unwrap();
        assert!(!observer.can_activate());
        drop(observer);
        fs::write(h.join("release"), b"finish").unwrap();
        assert!(child.wait().unwrap().success());
        assert!(StartupLease::acquire(&h).unwrap().can_activate());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn already_exchanged_current_version_finishes_receipt_without_swap() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let bytes = archive("2.0.0", |_| {});
        let (r, p) = prepare(&h, &bytes, "2.0.0");
        let incoming = r.join(&p.archive_sha256).join("incoming.app");
        exchange(&incoming, &h.join("Applications/RichOS.app")).unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert!(activate_prepared(&mut lease, "2.0.0")
            .unwrap()
            .is_none());
        assert_eq!(preferred(&h).unwrap().unwrap().version, "2.0.0");
        assert!(!r.join("prepared.json").exists());
        assert_eq!(
            fs::read(incoming.join("Contents/Resources/fixture-version")).unwrap(),
            b"1.0.0"
        );
    }
    #[test]
    fn newer_running_build_retires_obsolete_pending_without_mutating_app() {
        let t = home();
        let h = canonical(&t);
        let old = stage_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert!(activate_prepared(&mut lease, "4.0.0")
            .unwrap()
            .is_none());
        assert!(staged(&h).unwrap().is_none());
        drop(lease);
        stage_verified(&h, &archive("5.0.0", |_| {}), "5.0.0").unwrap();
        assert_eq!(staged(&h).unwrap().unwrap().publication.version, "5.0.0");
        assert!(!h.join("Applications/RichOS.app").exists());
        assert!(root(&h)
            .unwrap()
            .join(old.archive_sha256)
            .join("incoming.app")
            .exists());
    }
    #[test]
    fn compiled_version_mismatch_is_rejected_before_publication_and_does_not_block_next_update() {
        let t = home();
        let h = canonical(&t);
        let original = install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let malformed = archive_with_binary("3.0.0", &fixture_binary("2.0.0"), |_| {});
        assert!(stage_verified(&h, &malformed, "3.0.0").is_err());
        assert_eq!(
            fs::read(fixture_version(original.executable(&h))).unwrap(),
            b"1.0.0"
        );
        assert!(staged(&h).unwrap().is_none());
        stage_verified(&h, &archive("4.0.0", |_| {}), "4.0.0").unwrap();
        assert_eq!(staged(&h).unwrap().unwrap().publication.version, "4.0.0");
    }
    #[test]
    fn probe_does_not_inherit_writable_files_or_sockets() {
        use std::os::unix::net::UnixStream;
        let t = home();
        let h = canonical(&t);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(h.join("host-file"))
            .unwrap();
        let (socket, mut peer) = UnixStream::pair().unwrap();
        peer.set_nonblocking(true).unwrap();
        for fd in [file.as_raw_fd(), socket.as_raw_fd()] {
            assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, 0) }, 0);
        }
        let source = h.join("fd-probe.c");
        let exe = h.join("fd-probe");
        let response =
            serde_json::json!({"identifier":ID,"version":"2.0.0","protocol":1}).to_string();
        fs::write(&source, format!("#include <unistd.h>\n#include <errno.h>\n#include <stdio.h>\nint main(void){{if(write({},\"X\",1)!=-1||errno!=EBADF)return 61;if(write({},\"Y\",1)!=-1||errno!=EBADF)return 62;puts({:?});return 0;}}",file.as_raw_fd(),socket.as_raw_fd(),response)).unwrap();
        assert!(std::process::Command::new("/usr/bin/cc")
            .arg(source)
            .arg("-o")
            .arg(&exe)
            .spawn_fixture()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        let bytes = archive_with_binary("2.0.0", &fs::read(exe).unwrap(), |_| {});
        stage_verified(&h, &bytes, "2.0.0").unwrap();
        assert!(fs::read(h.join("host-file")).unwrap().is_empty());
        let mut byte = [0];
        assert_eq!(
            peer.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn external_newer_destination_is_not_replaced_by_older_pending() {
        let t = home();
        let h = canonical(&t);
        let staged = stage_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        extract(
            &archive("4.0.0", |_| {}),
            &h.join("Applications/RichOS.app"),
        )
        .unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert!(activate_prepared(&mut lease, "1.0.0").is_err());
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "4.0.0"
        );
        let r = root(&h).unwrap();
        assert!(!r.join("prepared.json").exists());
        assert!(r.join(staged.archive_sha256).join("obsolete.json").exists());
        drop(lease);
        stage_verified(&h, &archive("5.0.0", |_| {}), "5.0.0").unwrap();
    }
    #[test]
    fn collector_reclaims_verified_backup_and_superseded_payload_but_keeps_active_and_pending() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let p2 = install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let p3 = stage_verified(&h, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        let p4 = stage_verified(&h, &archive("4.0.0", |_| {}), "4.0.0").unwrap();
        let r = root(&h).unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        let loaded = h.join("Applications/RichOS.app");
        assert_eq!(collect_retired(&mut lease, &loaded).unwrap(), 2);
        assert_eq!(bundle_version(&loaded).unwrap(), "2.0.0");
        assert!(!r.join(p2.archive_sha256).join("incoming.app").exists());
        assert!(!r.join(p3.archive_sha256).join("incoming.app").exists());
        assert!(r.join(p4.archive_sha256).join("incoming.app").exists());
        assert_eq!(collect_retired(&mut lease, &loaded).unwrap(), 0);
    }
    #[test]
    fn collector_replays_partial_deletion_but_refuses_replaced_inode() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let p = install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let r = root(&h).unwrap();
        let tx = r.join(&p.archive_sha256);
        let incoming = tx.join("incoming.app");
        let retiring = tx.join("retiring.app");
        let record = Retirement::capture(&tx, &incoming).unwrap();
        write_document(&tx, "retirement.json", &record).unwrap();
        fs::rename(&incoming, &retiring).unwrap();
        fs::remove_file(retiring.join("Contents/Info.plist")).unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert_eq!(
            collect_retired(&mut lease, &h.join("Applications/RichOS.app")).unwrap(),
            1
        );
        assert!(!retiring.exists());
        fs::create_dir(&retiring).unwrap();
        write_document(&tx, "retirement.json", &record).unwrap();
        assert!(collect_retired(&mut lease, &h.join("Applications/RichOS.app")).is_err());
        assert!(retiring.exists());
    }
    #[test]
    fn collector_vetoes_shared_session_and_loaded_backup_and_cleans_partial_allocation() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let p = install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let r = root(&h).unwrap();
        let backup = r.join(p.archive_sha256).join("incoming.app");
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert_eq!(collect_retired(&mut lease, &backup).unwrap(), 0);
        lease.begin_session().unwrap();
        let mut observer = StartupLease::acquire(&h).unwrap();
        assert_eq!(
            collect_retired(&mut observer, &h.join("Applications/RichOS.app")).unwrap(),
            0
        );
        drop(observer);
        drop(lease);
        let tx = r.join("b".repeat(64));
        fs::create_dir(&tx).unwrap();
        fs::set_permissions(&tx, fs::Permissions::from_mode(0o700)).unwrap();
        let payload = tx.join("incoming.app");
        fs::create_dir(&payload).unwrap();
        write_document(
            &tx,
            "allocated.json",
            &Retirement::capture(&tx, &payload).unwrap(),
        )
        .unwrap();
        fs::write(payload.join("partial"), b"incomplete extraction").unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert_eq!(
            collect_retired(&mut lease, &h.join("Applications/RichOS.app")).unwrap(),
            2
        );
        assert!(!payload.exists());
    }
    #[test]
    fn damaged_pending_cannot_strand_a_newer_update_when_old_publication_is_intact() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let p = stage_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let tx = root(&h).unwrap().join(p.archive_sha256);
        fs::write(tx.join("incoming.app/Contents/Info.plist"), b"truncated").unwrap();
        stage_verified(&h, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "1.0.0"
        );
        assert!(tx.join("invalid.json").exists());
    }
    #[test]
    fn probe_scrubs_descriptor_above_lowered_soft_and_hard_limits() {
        const MARKER: &str = "RICHOS_PROBE_HIGH_FD_TEST";
        if std::env::var_os(MARKER).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::probe_scrubs_descriptor_above_lowered_soft_and_hard_limits",
                    "--nocapture",
                ])
                .env(MARKER, "1")
                .spawn_fixture()
                .unwrap()
                .wait()
                .unwrap();
            assert!(status.success());
            return;
        }
        let t = home();
        let h = canonical(&t);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(h.join("protected-file"))
            .unwrap();
        let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 1024) };
        assert!(fd >= 1024);
        let source = h.join("high-fd.c");
        let exe = h.join("high-fd");
        let response =
            serde_json::json!({"identifier":ID,"version":"2.0.0","protocol":1}).to_string();
        fs::write(&source,format!("#include <unistd.h>\n#include <errno.h>\n#include <stdio.h>\nint main(void){{if(write({},\"X\",1)!=-1||errno!=EBADF)return 61;puts({:?});return 0;}}",fd,response)).unwrap();
        assert!(std::process::Command::new("/usr/bin/cc")
            .arg(source)
            .arg("-o")
            .arg(&exe)
            .spawn_fixture()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        let bytes = archive_with_binary("2.0.0", &fs::read(exe).unwrap(), |_| {});
        let limits = libc::rlimit {
            rlim_cur: 256,
            rlim_max: 256,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limits) }, 0);
        assert!(unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0);
        stage_verified(&h, &bytes, "2.0.0").unwrap();
        assert!(fs::read(h.join("protected-file")).unwrap().is_empty());
        unsafe {
            libc::close(fd);
        }
    }
    #[test]
    fn slow_staging_probe_does_not_block_another_normal_startup() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let source = h.join("slow.c");
        let exe = h.join("slow");
        let response =
            serde_json::json!({"identifier":ID,"version":"2.0.0","protocol":1}).to_string();
        fs::write(&source,format!("#include <unistd.h>\n#include <stdio.h>\nint main(void){{usleep(1500000);puts({:?});return 0;}}",response)).unwrap();
        assert!(std::process::Command::new("/usr/bin/cc")
            .arg(source)
            .arg("-o")
            .arg(&exe)
            .spawn_fixture()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        let bytes = archive_with_binary("2.0.0", &fs::read(exe).unwrap(), |_| {});
        // NEITHER SIDE MAY TREAT A MOMENTARY `WouldBlock` AS AN ANSWER, because in this test
        // neither side is a session yet. `stage` takes its lease EXCLUSIVE and converts it to
        // shared in `begin_session`; between the two, under no publication lock, an observer
        // reads EX-busy then SH-busy and gets `WouldBlock` (nightly 2026-10-07, run
        // 20261007T141505Z-83f09f28, this test's `acquire().unwrap()`). The same holds the
        // other way: an observer between its own EX `acquire` and `drop` makes the stager's
        // acquisition `WouldBlock`. A real startup already retries exactly this --
        // `update_startup::wait_for_lock` (src-tauri) retries `WouldBlock` for 10 s -- and a
        // real stage runs inside an app that holds its shared session, so its lease is never
        // EX. Retrying both here is the production shape; a probe that really blocked a
        // startup still fails, at the deadline, as it did before.
        let retry = |deadline: std::time::Instant, error: &io::Error| {
            error.kind() == io::ErrorKind::WouldBlock && std::time::Instant::now() < deadline
        };
        let worker_home = h.clone();
        let worker = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                match stage_verified(&worker_home, &bytes, "2.0.0") {
                    Err(e) if retry(deadline, &e) => {
                        std::thread::sleep(std::time::Duration::from_millis(20))
                    }
                    result => return result,
                }
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match StartupLease::acquire(&h) {
                Err(e) if retry(deadline, &e) => {}
                Err(e) => panic!("a normal startup could not take its lease: {e:?}"),
                Ok(mut observer) if !observer.can_activate() => {
                    assert!(
                        !worker.is_finished(),
                        "slow stage unexpectedly finished before observation"
                    );
                    let publication = with_preferred(&h, |p| Ok(p.version.clone()))
                        .unwrap()
                        .unwrap();
                    assert_eq!(publication, "1.0.0");
                    observer.begin_session().unwrap();
                    break;
                }
                Ok(observer) => drop(observer),
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(worker.join().unwrap().unwrap().version, "2.0.0");
    }
    // =====================================================================================
    // THE WAY BACK OFF A BAD RELEASE
    //
    // Every test below moves this installation DOWN, which is the direction every other
    // check in this file exists to refuse. They run against the same fixture bundles the
    // update tests use, so what is proved is the authorization and the ordering -- not a
    // second, friendlier code path.
    // =====================================================================================

    /// Stage a rollback and let the next (exclusive) startup activate it, the way
    /// `install_verified` does for an ordinary update.
    #[cfg(target_os = "macos")]
    fn rollback_verified(
        home: &Path,
        bytes: &[u8],
        version: &str,
        running: &str,
    ) -> io::Result<Option<Activation>> {
        stage_rollback_verified(home, bytes, version)?;
        let mut lease = StartupLease::acquire(home)?;
        activate_prepared(&mut lease, running)
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rollback_returns_to_the_version_the_last_update_replaced() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        install_verified(&h, &old, "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        assert_eq!(
            rollback_target(&h).unwrap(),
            RollbackTarget::Available("1.0.0".into())
        );
        let activation = rollback_verified(&h, &old, "1.0.0", "2.0.0")
            .unwrap()
            .unwrap();
        assert!(activation.rollback);
        assert_eq!(activation.left.as_deref(), Some("2.0.0"));
        assert_eq!(activation.entered.version, "1.0.0");
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "1.0.0"
        );
        // The authorization is spent, not left lying about for a second downgrade.
        assert!(!root(&h).unwrap().join("rollback.json").exists());
        let recorded = last_activation(&h).unwrap().unwrap();
        assert!(recorded.rollback);
        assert_eq!(recorded.left.as_deref(), Some("2.0.0"));
        assert_eq!(recorded.entered.version, "1.0.0");
    }

    /// The same record after an ORDINARY update: the two are distinguishable without
    /// subtracting version numbers and guessing which way the subtraction went.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_update_and_a_rollback_are_distinguishable_in_the_activation_record() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let first = last_activation(&h).unwrap().unwrap();
        assert!(!first.rollback);
        assert_eq!(first.left, None, "nothing was installed before the first");
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let second = last_activation(&h).unwrap().unwrap();
        assert!(!second.rollback);
        assert_eq!(second.left.as_deref(), Some("1.0.0"));
        assert_eq!(second.entered.version, "2.0.0");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn there_is_no_way_back_from_a_first_installation() {
        let t = home();
        let h = canonical(&t);
        assert_eq!(rollback_target(&h).unwrap(), RollbackTarget::NoRecord);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        assert_eq!(rollback_target(&h).unwrap(), RollbackTarget::NoRecord);
        assert!(stage_rollback_verified(&h, &archive("0.9.0", |_| {}), "0.9.0").is_err());
    }

    /// After going back once, the recorded earlier version is the one just LEFT, which is
    /// newer. That is reported as such rather than offered as a second way back -- the way
    /// forward from here is an ordinary update, and a rollback never moves up.
    #[cfg(target_os = "macos")]
    #[test]
    fn rollback_does_not_offer_a_newer_version_as_a_way_back() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        install_verified(&h, &old, "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        rollback_verified(&h, &old, "1.0.0", "2.0.0").unwrap().unwrap();
        assert_eq!(
            rollback_target(&h).unwrap(),
            RollbackTarget::NotOlder("2.0.0".into())
        );
        let e = stage_rollback_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap_err();
        assert!(e.to_string().contains("not older"), "{e}");
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "1.0.0"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rollback_refuses_a_version_that_is_not_older_than_the_installed_one() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        // Including the version already installed: "go back" to where you already are is a
        // refusal with a sentence, not a receipt for what is already on disk.
        for version in ["2.0.0", "3.0.0"] {
            let e = stage_rollback_verified(&h, &archive(version, |_| {}), version).unwrap_err();
            assert!(
                e.to_string().contains("came from 1.0.0"),
                "unexpected refusal: {e}"
            );
        }
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "2.0.0"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rollback_refuses_while_an_update_is_already_prepared_for_the_next_launch() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        install_verified(&h, &old, "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        stage_verified(&h, &archive("3.0.0", |_| {}), "3.0.0").unwrap();
        let e = stage_rollback_verified(&h, &old, "1.0.0").unwrap_err();
        assert!(e.to_string().contains("already prepared"), "{e}");
        // The prepared update is untouched and still the thing the next launch will do.
        let pending = staged(&h).unwrap().unwrap();
        assert_eq!(pending.publication.version, "3.0.0");
        assert!(!pending.rollback);
    }

    /// The staged rollback reports itself AS a rollback, so the surface can promise the
    /// right thing before the restart rather than inferring a direction from two numbers.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_staged_rollback_reports_its_direction() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        install_verified(&h, &old, "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        stage_rollback_verified(&h, &old, "1.0.0").unwrap();
        let pending = staged(&h).unwrap().unwrap();
        assert_eq!(pending.publication.version, "1.0.0");
        assert!(pending.rollback);
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "2.0.0",
            "staging must not touch the running application"
        );
    }

    /// THE AUTHORIZATION IS BOUND TO THE VERSION IT WAS GRANTED AGAINST. A receipt naming a
    /// version this Mac is no longer running authorizes nothing, and is retired rather than
    /// applied to whatever happens to be installed.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_rollback_authorization_does_not_survive_the_version_it_named() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        install_verified(&h, &old, "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        stage_rollback_verified(&h, &old, "1.0.0").unwrap();
        let r = root(&h).unwrap();
        let mut receipt: Rollback = read_document(&r.join("rollback.json")).unwrap();
        receipt.from_version = "9.9.9".into();
        fs::remove_file(r.join("rollback.json")).unwrap();
        write_document(&r, "rollback.json", &receipt).unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        let e = activate_prepared(&mut lease, "2.0.0").unwrap_err();
        assert!(e.to_string().contains("no longer applies"), "{e}");
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "2.0.0"
        );
        assert!(!r.join("rollback.json").exists());
        assert!(staged(&h).unwrap().is_none(), "the receipt is retired");
    }

    /// An authorization naming a different archive is not this receipt's authorization.
    /// Without that rule one stale `rollback.json` would license every older payload that
    /// ever reached `prepared.json`.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_rollback_authorization_for_another_archive_licenses_nothing() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        let (r, p) = prepare(&h, &archive("1.5.0", |_| {}), "1.5.0");
        write_document(
            &r,
            "rollback.json",
            &Rollback {
                archive_sha256: "a".repeat(64),
                from_version: "2.0.0".into(),
            },
        )
        .unwrap();
        let mut lease = StartupLease::acquire(&h).unwrap();
        assert!(activate_prepared(&mut lease, "2.0.0").unwrap().is_none());
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "2.0.0"
        );
        assert!(r.join(&p.archive_sha256).join("obsolete.json").exists());
    }

    /// A PAYLOAD THAT DOES NOT SURVIVE THE CRATE'S OWN CHECKS IS NEVER PUBLISHED, on the
    /// way down exactly as on the way up.
    ///
    /// The signature is checked one layer above this, by `Update::download` against the
    /// compiled public key, and a rollback goes through that same download -- `updates.rs`
    /// hands these bytes over only after it returns. What this proves is the half that
    /// lives HERE: bytes that reach staging and then fail to extract, or that carry an
    /// identity other than the one asked for, leave the running application untouched.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_damaged_or_mislabelled_rollback_payload_never_reaches_the_application() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        install_verified(&h, &old, "1.0.0").unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();

        let mut damaged = old.clone();
        let i = damaged.len() / 2;
        damaged[i] ^= 0xFF;
        assert!(stage_rollback_verified(&h, &damaged, "1.0.0").is_err());

        // Intact bytes, a version this installation never came from: refused by the
        // history binding before a single byte is extracted.
        assert!(stage_rollback_verified(&h, &old, "1.0.1").is_err());

        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "2.0.0"
        );
        assert!(staged(&h).unwrap().is_none());
        assert!(!root(&h).unwrap().join("rollback.json").exists());
    }

    /// An ordinary update must not inherit an authorization it did not write.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_forward_update_clears_a_rollback_authorization_it_did_not_write() {
        let t = home();
        let h = canonical(&t);
        install_verified(&h, &archive("1.0.0", |_| {}), "1.0.0").unwrap();
        let r = root(&h).unwrap();
        write_document(
            &r,
            "rollback.json",
            &Rollback {
                archive_sha256: "b".repeat(64),
                from_version: "1.0.0".into(),
            },
        )
        .unwrap();
        install_verified(&h, &archive("2.0.0", |_| {}), "2.0.0").unwrap();
        assert!(!r.join("rollback.json").exists());
        assert!(!last_activation(&h).unwrap().unwrap().rollback);
    }

    /// RE-FETCHING AN EARLIER TAG LANDS ON ITS OWN OLD TRANSACTION DIRECTORY, whose payload
    /// the collector reclaimed and whose receipts it deliberately left behind. That
    /// directory holds no application, so it is reused -- while one that still holds an
    /// application is refused exactly as before.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_spent_transaction_is_reused_and_a_loaded_one_is_still_refused() {
        let t = home();
        let h = canonical(&t);
        let old = archive("1.0.0", |_| {});
        let r = root(&h).unwrap();
        install_verified(&h, &old, "1.0.0").unwrap();
        let spent_tx = r.join(digest(&old));
        assert!(
            spent_tx.join("verified.json").exists() && !spent_tx.join("incoming.app").exists(),
            "a first install leaves its transaction receipted and empty"
        );
        assert!(spent(&spent_tx).unwrap());
        let newer = archive("2.0.0", |_| {});
        install_verified(&h, &newer, "2.0.0").unwrap();
        // 2.0.0's transaction still HOLDS the displaced 1.0.0 bundle, so it is not spent.
        assert!(!spent(&r.join(digest(&newer))).unwrap());
        rollback_verified(&h, &old, "1.0.0", "2.0.0").unwrap().unwrap();
        assert_eq!(
            bundle_version(&h.join("Applications/RichOS.app")).unwrap(),
            "1.0.0"
        );
    }

    #[test]
    fn retirement_uses_native_volume_uuid_and_rejects_changed_volume_or_legacy_device_pins() {
        let t = home();
        let h = canonical(&t);
        let tx = h.join("transaction");
        let payload = tx.join("incoming.app");
        ensure_dir(&tx, 0o700).unwrap();
        ensure_dir(&payload, 0o755).unwrap();
        let mut record = Retirement::capture(&tx, &payload).unwrap();
        let native = volume_uuid(&payload).unwrap();
        assert_eq!(record.payload_volume, native);
        record.check(&tx, &payload).unwrap();
        let bytes = serde_json::to_vec(&record).unwrap();
        assert!(!String::from_utf8(bytes.clone()).unwrap().contains("_dev"));
        let reread: Retirement = serde_json::from_slice(&bytes).unwrap();
        reread.check(&tx, &payload).unwrap();
        record.payload_volume = "0".repeat(32);
        assert!(record.check(&tx, &payload).is_err());
        let legacy = serde_json::json!({"transaction_dev":metadata(&tx).unwrap().dev(),"transaction_ino":metadata(&tx).unwrap().ino(),"payload_dev":metadata(&payload).unwrap().dev(),"payload_ino":metadata(&payload).unwrap().ino()});
        assert!(serde_json::from_value::<Retirement>(legacy).is_err());
    }
}
