//! RichPass's small, UI-independent owner. No browser, secret, network or provider access.
//! The expected hashes come from this authenticated executable, never writable state.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const FILENAME: &str = "RichPass.html";
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const BUNDLED: &[u8] = include_bytes!("../../../../tools/richpass/RichPass.html");
const RELEASES: &str = include_str!("../../../../tools/richpass/releases.json");
const WINDOW: u64 = 600;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Release {
    pub release: String,
    pub sha256: String,
    pub approved: bool,
    pub download_url: Option<String>,
}
#[derive(Deserialize)]
struct Manifest {
    bundled: String,
    releases: Vec<Release>,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn bundled_release() -> Result<Release, Issue> {
    let m: Manifest = serde_json::from_str(RELEASES).map_err(|_| Issue::Source)?;
    let r = m
        .releases
        .into_iter()
        .find(|r| r.release == m.bundled)
        .ok_or(Issue::Source)?;
    if !r.approved || BUNDLED.len() > MAX_BYTES || digest(BUNDLED) != r.sha256 {
        return Err(Issue::Source);
    }
    Ok(r)
}
fn release(id: &str) -> Result<Release, Issue> {
    let m: Manifest = serde_json::from_str(RELEASES).map_err(|_| Issue::Source)?;
    let r = m
        .releases
        .into_iter()
        .find(|r| r.release == id)
        .ok_or(Issue::Source)?;
    if !r.approved {
        return Err(Issue::Withdrawn);
    }
    Ok(r)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Issue {
    Missing,
    Permission,
    Redirected,
    ChangedParent,
    Conflict,
    InvalidName,
    Unrecognized,
    Source,
    Withdrawn,
    State,
    Io,
    Unsupported,
    NotLocal,
}
impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
        Self::Missing => "The folder is unavailable. Reconnect it or choose another location.",
        Self::Permission => "Allow RichOS access to this folder in System Settings > Privacy & Security > Files & Folders, then choose Check now.",
        Self::Redirected => "A symbolic link or nonregular file prevents safe restoration. Choose another location.",
        Self::ChangedParent => "The protected folder changed. Choose the file again to confirm its location.",
        Self::Conflict => "The filename conflicts with another entry. Choose another location.",
        Self::InvalidName => "Use the filename RichPass.html, including capitalization.",
        Self::Unrecognized => "Not recognized as a RichPass release supported by this RichOS version. Save a verified copy instead.",
        Self::Source => "The verified repair copy is unavailable. RichPass has not been overwritten.",
        Self::Withdrawn => "This RichPass release is no longer approved for protection. Save the verified replacement.",
        Self::State => "RichPass protection could not be saved. The previous enrollment has been kept.",
        Self::Io => "The file operation failed. Check available space and folder permissions, then choose Check now.",
        Self::Unsupported => "Automatic protection is not available on this platform yet.",
        Self::NotLocal => "Choose a locally available file on a supported local filesystem. Download cloud-only files first.",
    })
    }
}
impl std::error::Error for Issue {}
fn io(error: std::io::Error) -> Issue {
    match error.kind() {
        std::io::ErrorKind::NotFound => Issue::Missing,
        std::io::ErrorKind::PermissionDenied => Issue::Permission,
        _ if error.raw_os_error() == Some(libc_eloop()) => Issue::Redirected,
        _ => Issue::Io,
    }
}
#[cfg(unix)]
fn libc_eloop() -> i32 {
    libc::ELOOP
}
#[cfg(not(unix))]
fn libc_eloop() -> i32 {
    -1
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    #[serde(default)]
    pub volume: Option<String>,
}
// Device numbers can change after removable-volume reconnection. Persistent identity
// uses the volume UUID plus inode; short-lived open-file checks still use dev/inode.
impl PartialEq for Identity {
    fn eq(&self, other: &Self) -> bool {
        self.inode == other.inode
            && match (&self.volume, &other.volume) {
                (Some(a), Some(b)) => a == b,
                (None, None) => self.device == other.device,
                _ => false,
            }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    pub path: PathBuf,
    pub parent: Identity,
    pub release: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    pub kind: String,
    pub at: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Registry {
    enrollment: Option<Enrollment>,
    attempts: Vec<u64>,
    paused: bool,
    notice: Option<Notice>,
    pending_notice: Option<Notice>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub problem: Option<Issue>,
    pub phase: String,
    pub message: String,
    pub path: Option<PathBuf>,
    pub release: Option<String>,
    pub last_checked: Option<u64>,
    pub notice: Option<Notice>,
    pub newer_available: bool,
    pub download_url: Option<String>,
    pub supported: bool,
    pub folder_permissions: bool,
}

/// One manager is held behind the native owner's single operation mutex.
/// Timers and filesystem events call the same `check`; no parallel writer exists.
pub struct Manager {
    root: PathBuf,
    registry: Registry,
    phase: String,
    message: String,
    checked: Option<u64>,
    broken: bool,
    problem: Option<Issue>,
}
impl Manager {
    pub fn open(root: PathBuf) -> Self {
        let loaded = read_regular(&root.join("state.json"), 65536);
        let (mut registry, broken) = match loaded {
            Ok(bytes) if bytes.len() <= 65536 => match serde_json::from_slice::<Registry>(&bytes) {
                Ok(r) => (r, false),
                Err(_) => (Registry::default(), true),
            },
            Err(Issue::Missing) => (Registry::default(), false),
            _ => (Registry::default(), true),
        };
        if registry.attempts.len() > 3 {
            registry.attempts.truncate(3);
            registry.paused = true;
        }
        let enrolled = registry.enrollment.is_some();
        Self {
            problem: if broken { Some(Issue::State) } else { None },
            root,
            registry,
            phase: if broken {
                "blocked"
            } else if enrolled {
                "checking"
            } else {
                "idle"
            }
            .into(),
            message: if broken {
                "Saved RichPass protection is unreadable. Choose the file again."
            } else {
                ""
            }
            .into(),
            checked: None,
            broken,
        }
    }
    pub fn view(&self) -> View {
        let current = bundled_release().ok();
        View {
            problem: self.problem,
            phase: self.phase.clone(),
            message: self.message.clone(),
            path: self.registry.enrollment.as_ref().map(|e| e.path.clone()),
            release: self.registry.enrollment.as_ref().map(|e| e.release.clone()),
            last_checked: self.checked,
            notice: self.registry.notice.clone(),
            newer_available: self
                .registry
                .enrollment
                .as_ref()
                .zip(current.as_ref())
                .is_some_and(|(e, r)| e.release != r.release),
            download_url: current.and_then(|r| r.download_url),
            supported: cfg!(target_os = "macos"),
            folder_permissions: cfg!(target_os = "macos"),
        }
    }
    pub fn parent(&self) -> Option<PathBuf> {
        self.registry
            .enrollment
            .as_ref()?
            .path
            .parent()
            .map(Path::to_path_buf)
    }
    fn persist(&self, registry: &Registry) -> Result<(), Issue> {
        fs::create_dir_all(&self.root).map_err(|_| Issue::State)?;
        let bytes = serde_json::to_vec(registry).map_err(|_| Issue::State)?;
        private_write(&self.root, "state.json", &bytes).map_err(|_| Issue::State)
    }
    fn source(&self, expected: &Release) -> Result<Vec<u8>, Issue> {
        if let Ok(bytes) = read_regular(
            &self.root.join(format!("cache-{}.html", expected.sha256)),
            MAX_BYTES,
        ) {
            if digest(&bytes) == expected.sha256 {
                return Ok(bytes);
            }
        }
        let bundled = bundled_release()?;
        if expected.release == bundled.release && expected.sha256 == bundled.sha256 {
            return Ok(BUNDLED.to_vec());
        }
        Err(Issue::Source)
    }
    /// Read only, before native confirmation; enrollment repeats this check afterwards.
    pub fn recognize(path: &Path) -> Result<Release, Issue> {
        valid_name(path)?;
        let bytes = read_regular(path, MAX_BYTES)?;
        let hash = digest(&bytes);
        let m: Manifest = serde_json::from_str(RELEASES).map_err(|_| Issue::Source)?;
        m.releases
            .into_iter()
            .find(|r| r.approved && r.sha256 == hash)
            .ok_or(Issue::Unrecognized)
    }
    /// Native consent has already reserved this exact leaf for RichPass. Unknown bytes
    /// are overwritten ONLY for an explicit Save-and-protect, never for existing-file enrollment.
    pub fn enroll(&mut self, path: PathBuf, save: bool, at: u64) -> Result<(), Issue> {
        valid_name(&path)?;
        let parent = path.parent().ok_or(Issue::InvalidName)?;
        let dir = anchor::Directory::open(parent)?;
        let selected = if save {
            bundled_release()?
        } else {
            Self::recognize(&path)?
        };
        let bytes = if save {
            BUNDLED.to_vec()
        } else {
            read_regular(&path, MAX_BYTES)?
        };
        if digest(&bytes) != selected.sha256 {
            return Err(Issue::Unrecognized);
        }
        let next = Registry {
            enrollment: Some(Enrollment {
                path: path.clone(),
                parent: dir.identity(),
                release: selected.release.clone(),
            }),
            ..Registry::default()
        };
        dir.revalidate(parent, &dir.identity())?;
        if save {
            dir.replace(FILENAME, &bytes)?;
        }
        dir.revalidate(parent, &dir.identity())?;
        let installed = dir.observe(FILENAME)?.ok_or(Issue::Io)?;
        if installed.name != FILENAME || digest(&installed.bytes) != selected.sha256 {
            return Err(Issue::Unrecognized);
        }
        fs::create_dir_all(&self.root).map_err(|_| Issue::State)?;
        // Hash-named caches preserve the old release if the registry switch fails.
        private_write(
            &self.root,
            &format!("cache-{}.html", selected.sha256),
            &bytes,
        )
        .map_err(|_| Issue::State)?;
        self.persist(&next)?;
        self.registry = next;
        self.broken = false;
        self.checked = Some(at);
        self.phase = "verified".into();
        self.problem = None;
        self.message.clear();
        Ok(())
    }
    /// Stop removes only managed state/cache; the user's file is never deleted.
    pub fn stop(&mut self) -> Result<(), Issue> {
        let cached = self
            .registry
            .enrollment
            .as_ref()
            .and_then(|e| release(&e.release).ok())
            .map(|r| self.root.join(format!("cache-{}.html", r.sha256)));
        self.persist(&Registry::default())?;
        self.registry = Registry::default();
        self.broken = false;
        self.checked = None;
        self.phase = "idle".into();
        self.problem = None;
        self.message.clear();
        if let Some(cached) = cached {
            if let Err(e) = fs::remove_file(cached) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    self.message =
                        "Protection stopped. Its unused managed cache could not be removed.".into();
                }
            }
        }
        Ok(())
    }
    pub fn acknowledge(&mut self) -> Result<(), Issue> {
        let mut next = self.registry.clone();
        next.notice = None;
        self.persist(&next)?;
        self.registry = next;
        Ok(())
    }
    /// True means this check committed a new or interrupted repair notice.
    /// `at` is explicit for clock-controlled tests. A manual check allows one bounded
    /// attempt; automatic attempts cannot bypass the persisted interference window.
    pub fn check(&mut self, at: u64, manual: bool) -> bool {
        let Some(enrollment) = self.registry.enrollment.clone() else {
            return false;
        };
        self.problem = None;
        self.phase = "checking".into();
        self.checked = Some(at);
        match self.check_inner(&enrollment, at, manual) {
            Ok(restored) => restored,
            Err(issue) => {
                self.problem = Some(issue);
                self.phase = if issue == Issue::Missing {
                    "waiting"
                } else if issue == Issue::Source {
                    "cache_unavailable"
                } else {
                    "blocked"
                }
                .into();
                self.message = issue.to_string();
                false
            }
        }
    }
    fn check_inner(&mut self, enrollment: &Enrollment, at: u64, manual: bool) -> Result<bool, Issue> {
        valid_name(&enrollment.path)?;
        let expected = release(&enrollment.release)?;
        let parent = enrollment.path.parent().ok_or(Issue::InvalidName)?;
        let dir = anchor::Directory::open(parent)?;
        if dir.identity() != enrollment.parent {
            return Err(Issue::ChangedParent);
        }
        let observation = dir.observe(FILENAME)?;
        if self.registry.paused && !manual {
            self.phase = "paused".into();
            self.message="Automatic repairs are paused. Choose Check now to resume or stop protection before removing the file.".into();
            return Ok(false);
        }
        if observation
            .as_ref()
            .is_some_and(|o| o.name == FILENAME && digest(&o.bytes) == expected.sha256)
        {
            let recovered = self.registry.pending_notice.is_some();
            if recovered || (manual && self.registry.paused) {
                let mut next = self.registry.clone();
                next.paused = false;
                if let Some(notice) = next.pending_notice.take() {
                    next.notice = Some(notice);
                }
                self.persist(&next)?;
                self.registry = next;
            }
            self.phase = "verified".into();
            self.message.clear();
            return Ok(recovered);
        }
        self.registry
            .attempts
            .retain(|t| at.saturating_sub(*t) < WINDOW);
        if !manual && (self.registry.paused || self.registry.attempts.len() >= 3) {
            self.registry.paused = true;
            self.persist(&self.registry)?;
            self.phase = "paused".into();
            self.message="RichPass keeps changing. Automatic repairs are paused. Close open RichPass tabs and check what is modifying it. To remove the file permanently, stop protection first.".into();
            return Ok(false);
        }
        // Healthy checks read only the protected HTML. Authenticate the repair
        // source on demand, before any staging, accounting or destination write.
        let bytes = self.source(&expected)?;
        let mut attempt = self.registry.clone();
        attempt.paused = false;
        attempt.pending_notice = Some(Notice {
            kind: if observation.as_ref().is_some_and(|o| o.name == FILENAME) {
                "contents"
            } else {
                "location"
            }
            .into(),
            at,
        });
        attempt.attempts.push(at);
        if attempt.attempts.len() > 3 {
            attempt.attempts.remove(0);
        }
        self.persist(&attempt)?;
        self.registry = attempt;
        self.phase = "repairing".into();
        dir.revalidate(parent, &enrollment.parent)?;
        dir.replace(FILENAME, &bytes)?;
        dir.revalidate(parent, &enrollment.parent)?;
        let installed = dir.observe(FILENAME)?.ok_or(Issue::Io)?;
        if installed.name != FILENAME || digest(&installed.bytes) != expected.sha256 {
            return Err(Issue::Io);
        }
        let mut completed = self.registry.clone();
        completed.notice = completed.pending_notice.take();
        self.persist(&completed)?;
        self.registry = completed;
        self.phase = "verified".into();
        self.message.clear();
        Ok(true)
    }
}
fn valid_name(path: &Path) -> Result<(), Issue> {
    if path.is_absolute() && path.file_name() == Some(std::ffi::OsStr::new(FILENAME)) {
        Ok(())
    } else {
        Err(Issue::InvalidName)
    }
}
fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, Issue> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() {
        return Err(Issue::Redirected);
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io)?;
    if !file.metadata().map_err(io)?.is_file() {
        return Err(Issue::Redirected);
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(Issue::Unrecognized);
    }
    Ok(bytes)
}
fn private_write(root: &Path, name: &str, bytes: &[u8]) -> Result<(), Issue> {
    let dir = anchor::Directory::open(root)?;
    dir.replace(name, bytes)
}
/// Standalone copies need no enrollment or monitoring, including on Windows.
pub fn save_copy(path: &Path) -> Result<(), Issue> {
    valid_name(path)?;
    bundled_release()?;
    #[cfg(unix)]
    {
        let parent = path.parent().ok_or(Issue::InvalidName)?;
        let dir = anchor::Directory::open(parent)?;
        let id = dir.identity();
        dir.revalidate(parent, &id)?;
        dir.replace(FILENAME, BUNDLED)?;
        dir.revalidate(parent, &id)?;
        let installed = dir.observe(FILENAME)?.ok_or(Issue::Io)?;
        if installed.name != FILENAME || installed.bytes != BUNDLED {
            return Err(Issue::Io);
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        windows_save(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(Issue::Unsupported)
    }
}
#[cfg(windows)]
fn windows_save(path: &Path) -> Result<(), Issue> {
    use std::os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    };
    // Standalone export only, never a Windows watcher claim. Refuse reparse points
    // at every existing component. Replace the directory entry, never truncate a link.
    let parent = path.parent().ok_or(Issue::InvalidName)?;
    let safe = || -> Result<(), Issue> {
        for entry in path.ancestors() {
            match fs::symlink_metadata(entry) {
                Ok(m) => {
                    if m.file_attributes() & 0x400 != 0 {
                        return Err(Issue::Redirected);
                    }
                }
                Err(e) if entry == path && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        Ok(())
    };
    safe()?;
    let temp = parent.join(format!(".richpass-{}.partial", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .custom_flags(0x00200000)
            .open(&temp)
            .map_err(io)?;
        file.write_all(BUNDLED).map_err(io)?;
        file.sync_all().map_err(io)?;
        file.rewind().map_err(io)?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes != BUNDLED {
            return Err(Issue::Io);
        }
        safe()?;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
        }
        let wide = |p: &Path| {
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<u16>>()
        };
        if unsafe { MoveFileExW(wide(&temp).as_ptr(), wide(path).as_ptr(), 0x1 | 0x8) } == 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        if read_regular(path, MAX_BYTES)? != BUNDLED {
            return Err(Issue::Io);
        }
        Ok(())
    })();
    if result.is_err() {
        drop(fs::remove_file(temp));
    }
    result
}

#[cfg(unix)]
mod anchor {
    use super::*;
    use std::{
        ffi::{CString, OsStr},
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        },
    };
    pub struct Directory {
        fd: OwnedFd,
        identity: Identity,
    }
    pub struct Observed {
        pub name: String,
        pub bytes: Vec<u8>,
    }
    fn c(value: &OsStr) -> Result<CString, Issue> {
        CString::new(value.as_bytes()).map_err(|_| Issue::Conflict)
    }
    fn own(fd: i32) -> Result<OwnedFd, Issue> {
        if fd < 0 {
            Err(io(std::io::Error::last_os_error()))
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }
    fn open_child_directory(parent: &OwnedFd, name: &OsStr) -> Result<OwnedFd, Issue> {
        let name = c(name)?;
        let parent = parent.as_raw_fd();
        let fd = unsafe {
            libc::openat(
                parent,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd >= 0 {
            return own(fd);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOTDIR) {
            // O_DIRECTORY with O_NOFOLLOW can report ENOTDIR for a symlink.
            // Identify the entry without traversing it or accepting a replacement.
            let mut entry = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    parent,
                    name.as_ptr(),
                    entry.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } == 0
                && unsafe { entry.assume_init() }.st_mode & libc::S_IFMT == libc::S_IFLNK
            {
                return Err(Issue::Redirected);
            }
        }
        Err(io(error))
    }
    fn stat(fd: i32) -> Result<libc::stat, Issue> {
        let mut s = std::mem::MaybeUninit::uninit();
        if unsafe { libc::fstat(fd, s.as_mut_ptr()) } != 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        Ok(unsafe { s.assume_init() })
    }
    fn identity(s: &libc::stat) -> Identity {
        Identity {
            device: s.st_dev as u64,
            inode: s.st_ino,
            volume: None,
        }
    }
    #[cfg(target_os = "macos")]
    fn volume_uuid(fd: i32) -> Result<String, Issue> {
        let mut attributes = libc::attrlist {
            bitmapcount: 5,
            reserved: 0,
            commonattr: 0,
            volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        #[repr(C)]
        struct ResultBytes {
            length: u32,
            uuid: [u8; 16],
        }
        let mut result = ResultBytes {
            length: 0,
            uuid: [0; 16],
        };
        if unsafe {
            libc::fgetattrlist(
                fd,
                &mut attributes as *mut _ as *mut libc::c_void,
                &mut result as *mut _ as *mut libc::c_void,
                std::mem::size_of::<ResultBytes>(),
                0,
            )
        } != 0
        {
            return Err(Issue::NotLocal);
        }
        if result.length as usize != std::mem::size_of::<ResultBytes>() || result.uuid == [0; 16] {
            return Err(Issue::NotLocal);
        }
        Ok(result.uuid.iter().map(|b| format!("{b:02x}")).collect())
    }
    impl Directory {
        pub fn open(path: &Path) -> Result<Self, Issue> {
            if !path.is_absolute() {
                return Err(Issue::ChangedParent);
            }
            let mut fd = own(unsafe {
                libc::open(
                    c(OsStr::new("/"))?.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            })?;
            for component in path.components() {
                match component {
                    std::path::Component::RootDir => {}
                    std::path::Component::Normal(name) => {
                        fd = open_child_directory(&fd, name)?;
                    }
                    _ => return Err(Issue::ChangedParent),
                }
            }
            #[cfg(target_os = "macos")]
            {
                let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
                if unsafe { libc::fstatfs(fd.as_raw_fd(), fs.as_mut_ptr()) } != 0 {
                    return Err(io(std::io::Error::last_os_error()));
                }
                if unsafe { fs.assume_init() }.f_flags & libc::MNT_LOCAL as u32 == 0 {
                    return Err(Issue::NotLocal);
                }
            }
            let mut id = identity(&stat(fd.as_raw_fd())?);
            #[cfg(target_os = "macos")]
            {
                id.volume = Some(volume_uuid(fd.as_raw_fd())?);
            }
            Ok(Self { fd, identity: id })
        }
        pub fn identity(&self) -> Identity {
            self.identity.clone()
        }
        pub fn revalidate(&self, path: &Path, expected: &Identity) -> Result<(), Issue> {
            if Self::open(path)?.identity == *expected
                && identity(&stat(self.fd.as_raw_fd())?)
                    == (Identity {
                        device: self.identity.device,
                        inode: self.identity.inode,
                        volume: None,
                    })
            {
                Ok(())
            } else {
                Err(Issue::ChangedParent)
            }
        }
        fn metadata(&self, name: &str) -> Result<Option<libc::stat>, Issue> {
            let mut s = std::mem::MaybeUninit::uninit();
            let name = c(OsStr::new(name))?;
            if unsafe {
                libc::fstatat(
                    self.fd.as_raw_fd(),
                    name.as_ptr(),
                    s.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                let e = std::io::Error::last_os_error();
                return if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(io(e))
                };
            }
            let s = unsafe { s.assume_init() };
            if s.st_mode & libc::S_IFMT != libc::S_IFREG {
                return Err(Issue::Redirected);
            }
            #[cfg(target_os = "macos")]
            if s.st_flags & 0x4000_0000 != 0 {
                return Err(Issue::NotLocal);
            } // Darwin SF_DATALESS: never hydrate a placeholder.
            Ok(Some(s))
        }
        fn actual_name(&self, wanted: &str) -> Result<String, Issue> {
            let fd = own(unsafe {
                libc::openat(
                    self.fd.as_raw_fd(),
                    c(OsStr::new("."))?.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            })?;
            use std::os::fd::IntoRawFd;
            let raw = fd.into_raw_fd();
            let stream = unsafe { libc::fdopendir(raw) };
            if stream.is_null() {
                unsafe { libc::close(raw) };
                return Err(Issue::Io);
            }
            struct Stream(*mut libc::DIR);
            impl Drop for Stream {
                fn drop(&mut self) {
                    unsafe { libc::closedir(self.0) };
                }
            }
            let stream = Stream(stream);
            let mut aliases = Vec::new();
            for _ in 0..100000 {
                let entry = unsafe { libc::readdir(stream.0) };
                if entry.is_null() {
                    return match aliases.len() {
                        1 => Ok(aliases.remove(0)),
                        _ => Err(Issue::Conflict),
                    };
                }
                let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name == wanted.as_bytes() {
                    return Ok(wanted.into());
                }
                if name.eq_ignore_ascii_case(wanted.as_bytes()) {
                    aliases.push(String::from_utf8_lossy(name).into_owned());
                }
            }
            Err(Issue::Conflict)
        }
        pub fn observe(&self, name: &str) -> Result<Option<Observed>, Issue> {
            let Some(before) = self.metadata(name)? else {
                return Ok(None);
            };
            let fd = own(unsafe {
                libc::openat(
                    self.fd.as_raw_fd(),
                    c(OsStr::new(name))?.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            })?;
            let opened = stat(fd.as_raw_fd())?;
            if identity(&before) != identity(&opened)
                || opened.st_mode & libc::S_IFMT != libc::S_IFREG
            {
                return Err(Issue::Redirected);
            }
            let actual = self.actual_name(name)?;
            let mut bytes = Vec::new();
            let file = fs::File::from(fd);
            file.take(MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(io)?;
            let after = self.metadata(name)?.ok_or(Issue::Conflict)?;
            if identity(&after) != identity(&opened)
                || after.st_size != opened.st_size
                || after.st_mtime != opened.st_mtime
                || after.st_mtime_nsec != opened.st_mtime_nsec
                || after.st_ctime != opened.st_ctime
                || after.st_ctime_nsec != opened.st_ctime_nsec
                || self.actual_name(name)? != actual
            {
                return Err(Issue::Conflict);
            }
            // Oversized contents are changed, not an unbounded read or a trusted source.
            Ok(Some(Observed {
                name: actual,
                bytes,
            }))
        }
        pub fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), Issue> {
            self.metadata(name)?; // Refuse links/devices/directories; a missing leaf is repairable.
            let tmp = format!(".richpass-{}.partial", uuid::Uuid::new_v4());
            let tmp_c = c(OsStr::new(&tmp))?;
            let fd = own(unsafe {
                libc::openat(
                    self.fd.as_raw_fd(),
                    tmp_c.as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            })?;
            let mut staged = fs::File::from(fd);
            let prepared = (|| {
                staged.write_all(bytes).map_err(io)?;
                staged.sync_all().map_err(io)?;
                staged.rewind().map_err(io)?;
                let mut readback = Vec::new();
                Read::by_ref(&mut staged)
                    .take(bytes.len() as u64 + 1)
                    .read_to_end(&mut readback)
                    .map_err(io)?;
                if readback != bytes {
                    return Err(Issue::Io);
                }
                stat(staged.as_raw_fd())
            })();
            let sealed = match prepared {
                Ok(sealed) => sealed,
                Err(e) => {
                    unsafe { libc::unlinkat(self.fd.as_raw_fd(), tmp_c.as_ptr(), 0) };
                    return Err(e);
                }
            };
            let result = (|| {
                // Keep the verified staging descriptor open and check that its directory
                // entry still names that same file before publishing it.
                let entry = self.metadata(&tmp)?.ok_or(Issue::Conflict)?;
                if identity(&entry) != identity(&sealed)
                    || entry.st_size != sealed.st_size
                    || entry.st_mtime != sealed.st_mtime
                    || entry.st_mtime_nsec != sealed.st_mtime_nsec
                    || entry.st_ctime != sealed.st_ctime
                    || entry.st_ctime_nsec != sealed.st_ctime_nsec
                {
                    return Err(Issue::Conflict);
                }
                let actual = if self.metadata(name)?.is_some() {
                    Some(self.actual_name(name)?)
                } else {
                    None
                };
                let backup = actual
                    .as_ref()
                    .filter(|actual| actual.as_str() != name)
                    .map(|_| format!(".richpass-{}.previous", uuid::Uuid::new_v4()));
                if let (Some(actual), Some(backup)) = (&actual, &backup) {
                    // A same-inode case-insensitive alias requires a two-name move to fix case.
                    if unsafe {
                        libc::renameat(
                            self.fd.as_raw_fd(),
                            c(OsStr::new(actual))?.as_ptr(),
                            self.fd.as_raw_fd(),
                            c(OsStr::new(backup))?.as_ptr(),
                        )
                    } != 0
                    {
                        return Err(io(std::io::Error::last_os_error()));
                    }
                }
                self.metadata(name)?;
                if unsafe {
                    libc::renameat(
                        self.fd.as_raw_fd(),
                        tmp_c.as_ptr(),
                        self.fd.as_raw_fd(),
                        c(OsStr::new(name))?.as_ptr(),
                    )
                } != 0
                {
                    if let (Some(actual), Some(backup)) = (&actual, &backup) {
                        unsafe {
                            libc::renameat(
                                self.fd.as_raw_fd(),
                                c(OsStr::new(backup))?.as_ptr(),
                                self.fd.as_raw_fd(),
                                c(OsStr::new(actual))?.as_ptr(),
                            )
                        };
                    }
                    return Err(io(std::io::Error::last_os_error()));
                }
                if let Some(backup) = backup {
                    unsafe {
                        libc::unlinkat(self.fd.as_raw_fd(), c(OsStr::new(&backup))?.as_ptr(), 0)
                    };
                }
                if unsafe { libc::fsync(self.fd.as_raw_fd()) } != 0 {
                    return Err(io(std::io::Error::last_os_error()));
                }
                Ok(())
            })();
            if result.is_err() {
                unsafe { libc::unlinkat(self.fd.as_raw_fd(), tmp_c.as_ptr(), 0) };
            }
            result
        }
    }
}
#[cfg(not(unix))]
mod anchor {
    use super::*;
    pub struct Directory;
    pub struct Observed {
        pub name: String,
        pub bytes: Vec<u8>,
    }
    impl Directory {
        pub fn open(_: &Path) -> Result<Self, Issue> {
            Err(Issue::Unsupported)
        }
        pub fn identity(&self) -> Identity {
            Identity {
                device: 0,
                inode: 0,
                volume: None,
            }
        }
        pub fn observe(&self, _: &str) -> Result<Option<Observed>, Issue> {
            Err(Issue::Unsupported)
        }
        pub fn replace(&self, _: &str, _: &[u8]) -> Result<(), Issue> {
            Err(Issue::Unsupported)
        }
        pub fn revalidate(&self, _: &Path, _: &Identity) -> Result<(), Issue> {
            Err(Issue::Unsupported)
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("richpass-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(p.join("files")).unwrap();
            // Use the fixture's real location even when the system temp path is an alias.
            Self(p.canonicalize().unwrap())
        }
        fn path(&self) -> PathBuf {
            self.0.join("files").join(FILENAME)
        }
        fn manager(&self) -> Manager {
            Manager::open(self.0.join("state"))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }
    #[test]
    fn bundled_hash_is_compiled_and_exact() {
        assert_eq!(digest(BUNDLED), bundled_release().unwrap().sha256);
        assert!(BUNDLED.len() < MAX_BYTES);
    }
    #[test]
    fn save_enroll_edit_and_restart_repair_offline() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        fs::write(f.path(), b"changed").unwrap();
        assert!(m.check(101, false));
        assert_eq!(m.view().phase, "verified");
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
        assert_eq!(m.view().notice.unwrap().kind, "contents");
        let mut m = f.manager();
        assert_eq!(m.view().phase, "checking");
        assert!(!m.check(101, false));
        assert_eq!(m.view().notice.unwrap().at, 101);
        fs::write(f.path(), b"changed again").unwrap();
        assert!(m.check(101, true));
        assert!(!m.check(101, false));
    }
    #[test]
    fn unknown_existing_file_is_not_enrolled_or_changed() {
        let f = Fixture::new();
        fs::write(f.path(), b"unrecognized").unwrap();
        let mut m = f.manager();
        assert_eq!(m.enroll(f.path(), false, 100), Err(Issue::Unrecognized));
        assert_eq!(fs::read(f.path()).unwrap(), b"unrecognized");
        assert!(m.view().path.is_none());
    }
    #[test]
    fn removing_or_moving_restores_only_the_reserved_location() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let moved = f.0.join("moved.html");
        fs::rename(f.path(), &moved).unwrap();
        m.check(101, false);
        assert_eq!(fs::read(&moved).unwrap(), BUNDLED);
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
        assert_eq!(m.view().notice.unwrap().kind, "location");
    }
    #[test]
    fn symlink_destination_never_writes_through_to_another_file() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let other = f.0.join("other");
        fs::write(&other, b"private unrelated contents").unwrap();
        fs::remove_file(f.path()).unwrap();
        symlink(&other, f.path()).unwrap();
        m.check(101, false);
        assert_eq!(m.view().phase, "blocked");
        assert_eq!(fs::read(other).unwrap(), b"private unrelated contents");
        assert!(fs::symlink_metadata(f.path())
            .unwrap()
            .file_type()
            .is_symlink());
    }
    #[test]
    fn replaced_parent_is_refused_even_if_the_path_still_exists() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        fs::rename(f.0.join("files"), f.0.join("previous")).unwrap();
        fs::create_dir(f.0.join("files")).unwrap();
        fs::write(f.path(), b"unrelated").unwrap();
        m.check(101, false);
        assert_eq!(m.view().phase, "blocked");
        assert_eq!(fs::read(f.path()).unwrap(), b"unrelated");
    }
    #[test]
    fn hard_link_is_replaced_without_mutating_the_other_name() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let other = f.0.join("other");
        fs::hard_link(f.path(), &other).unwrap();
        fs::write(f.path(), b"changed").unwrap();
        m.check(101, false);
        assert_eq!(fs::read(other).unwrap(), b"changed");
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
    }
    #[test]
    fn three_automatic_attempts_persist_and_manual_check_allows_one() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        for t in 101..104 {
            fs::write(f.path(), b"changed").unwrap();
            m.check(t, false);
            assert_eq!(m.view().phase, "verified");
        }
        fs::write(f.path(), b"changed again").unwrap();
        let mut m = f.manager();
        m.check(104, false);
        assert_eq!(m.view().phase, "paused");
        assert_eq!(fs::read(f.path()).unwrap(), b"changed again");
        m.check(105, true);
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
    }
    #[test]
    fn corrupt_cache_falls_back_only_to_identical_compiled_bytes() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        fs::write(
            f.0.join(format!(
                "state/cache-{}.html",
                bundled_release().unwrap().sha256
            )),
            b"bad cache",
        )
        .unwrap();
        fs::write(f.path(), b"bad target").unwrap();
        m.check(101, false);
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
    }
    #[test]
    fn wrong_basename_writes_nothing() {
        let f = Fixture::new();
        let mut m = f.manager();
        let wrong = f.0.join("files/richpass.html");
        assert_eq!(m.enroll(wrong.clone(), true, 100), Err(Issue::InvalidName));
        assert!(!wrong.exists());
    }
    #[test]
    fn stop_keeps_the_user_file_and_stops_further_repair() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        m.stop().unwrap();
        fs::write(f.path(), b"user owns it again").unwrap();
        m.check(101, false);
        assert_eq!(fs::read(f.path()).unwrap(), b"user owns it again");
        assert!(!f
            .0
            .join(format!(
                "state/cache-{}.html",
                bundled_release().unwrap().sha256
            ))
            .exists());
    }
    #[test]
    fn registry_cannot_supply_its_own_expected_hash_or_arbitrary_release() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let state = f.0.join("state/state.json");
        let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
        v["enrollment"]["sha256"] = serde_json::json!(digest(b"untrusted"));
        fs::write(&state, serde_json::to_vec(&v).unwrap()).unwrap();
        let mut m = f.manager();
        m.check(101, false);
        assert_eq!(m.view().phase, "blocked");
        assert!(m.view().path.is_none());
    }
    #[test]
    fn case_only_rename_restores_canonical_name_on_host_filesystem() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let alias = f.0.join("files/richpass.html");
        fs::rename(f.path(), &alias).unwrap();
        m.check(101, false);
        assert_eq!(m.view().phase, "verified");
        let names: Vec<_> = fs::read_dir(f.0.join("files"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(names.contains(&std::ffi::OsString::from(FILENAME)));
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
    }
    #[test]
    fn failed_enrollment_retains_previous_registry_and_source() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let other = f.0.join("another");
        fs::create_dir(&other).unwrap();
        let state = f.0.join("state/state.json");
        fs::remove_file(&state).unwrap();
        fs::create_dir(&state).unwrap();
        assert_eq!(m.enroll(other.join(FILENAME), true, 101), Err(Issue::State));
        assert_eq!(m.view().path, Some(f.path()));
        fs::remove_dir(&state).unwrap();
        fs::write(f.path(), b"changed").unwrap();
        m.check(102, false);
        assert_eq!(m.view().phase, "verified");
        assert_eq!(fs::read(f.path()).unwrap(), BUNDLED);
    }
    #[test]
    fn symlink_parent_and_directory_target_block_without_touching_other_files() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        fs::remove_file(f.path()).unwrap();
        fs::create_dir(f.path()).unwrap();
        m.check(101, false);
        assert_eq!(m.view().problem, Some(Issue::Redirected));
        fs::rename(f.0.join("files"), f.0.join("original")).unwrap();
        let other = f.0.join("other");
        fs::create_dir(&other).unwrap();
        fs::write(other.join(FILENAME), b"unrelated").unwrap();
        symlink(&other, f.0.join("files")).unwrap();
        m.check(102, false);
        assert_eq!(m.view().problem, Some(Issue::Redirected));
        assert_eq!(fs::read(other.join(FILENAME)).unwrap(), b"unrelated");
        fs::remove_file(f.0.join("files")).unwrap();
        fs::write(f.0.join("files"), b"ordinary file").unwrap();
        m.check(103, false);
        assert_eq!(m.view().problem, Some(Issue::Io));
    }
    #[test]
    fn old_release_source_never_falls_forward_to_new_bundled_bytes() {
        let f = Fixture::new();
        let m = f.manager();
        let bytes = b"fictional old release source";
        let old = Release {
            release: "fictional-older".into(),
            sha256: digest(bytes),
            approved: true,
            download_url: None,
        };
        assert!(matches!(m.source(&old), Err(Issue::Source)));
        fs::create_dir_all(&m.root).unwrap();
        let cache = m.root.join(format!("cache-{}.html", old.sha256));
        fs::write(&cache, bytes).unwrap();
        assert_eq!(m.source(&old).unwrap(), bytes);
        fs::write(cache, b"tampered").unwrap();
        assert!(matches!(m.source(&old), Err(Issue::Source)));
    }
    #[test]
    fn oversized_registry_and_unknown_release_do_not_authorize_a_write() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        let state = f.0.join("state/state.json");
        let saved = fs::read(&state).unwrap();
        fs::write(&state, vec![b'x'; 65537]).unwrap();
        let m = f.manager();
        assert_eq!(m.view().phase, "blocked");
        assert!(m.view().path.is_none());
        let mut v: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        v["enrollment"]["release"] = serde_json::json!("not-compiled");
        fs::write(state, serde_json::to_vec(&v).unwrap()).unwrap();
        fs::write(f.path(), b"changed").unwrap();
        let mut m = f.manager();
        m.check(101, false);
        assert_eq!(m.view().problem, Some(Issue::Source));
        assert_eq!(fs::read(f.path()).unwrap(), b"changed");
    }
    #[test]
    fn paused_state_survives_external_restoration_until_explicit_check() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        m.registry.paused = true;
        m.persist(&m.registry).unwrap();
        m.check(101, false);
        assert_eq!(m.view().phase, "paused");
        let mut m = f.manager();
        m.check(102, false);
        assert_eq!(m.view().phase, "paused");
        m.check(103, true);
        assert_eq!(m.view().phase, "verified");
        assert!(!m.registry.paused);
    }
    #[test]
    fn interrupted_commit_retains_a_notice_after_restart() {
        let f = Fixture::new();
        let mut m = f.manager();
        m.enroll(f.path(), true, 100).unwrap();
        // Durable intent, followed by a complete target replacement and process loss
        // before the final registry update, is the actual crash boundary.
        m.registry.pending_notice = Some(Notice {
            kind: "contents".into(),
            at: 101,
        });
        m.persist(&m.registry).unwrap();
        anchor::Directory::open(f.path().parent().unwrap())
            .unwrap()
            .replace(FILENAME, BUNDLED)
            .unwrap();
        let mut m = f.manager();
        assert!(m.check(102, false));
        assert_eq!(m.view().phase, "verified");
        assert_eq!(m.view().notice.unwrap().at, 101);
        assert!(m.registry.pending_notice.is_none());
        assert!(!f.manager().check(103, false));
    }
    #[test]
    fn persistent_identity_survives_device_renumbering_but_refuses_another_volume() {
        let a = Identity {
            device: 1,
            inode: 42,
            volume: Some("volume-a".into()),
        };
        let reconnected = Identity {
            device: 9,
            ..a.clone()
        };
        assert_eq!(a, reconnected);
        let other = Identity {
            device: 1,
            inode: 42,
            volume: Some("volume-b".into()),
        };
        assert_ne!(a, other);
        let legacy = Identity {
            device: 1,
            inode: 42,
            volume: None,
        };
        assert_ne!(a, legacy);
    }
}
