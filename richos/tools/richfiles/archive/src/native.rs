use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::{self, File, Metadata, OpenOptions, Permissions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::{Builder, TempDir};

use crate::{Result, check_cancelled, invalid};

fn c_name(name: &OsStr) -> Result<CString> {
    Ok(CString::new(name.as_bytes())?)
}

fn name_of(path: &Path) -> Result<&OsStr> {
    path.file_name()
        .ok_or_else(|| invalid("Choose a named file or folder."))
}

fn open_folder(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)?)
}

pub fn same_entry(before: &Metadata, after: &Metadata) -> bool {
    before.dev() == after.dev() && before.ino() == after.ino()
}

pub fn same_version(before: &Metadata, after: &Metadata) -> bool {
    same_entry(before, after)
        && before.len() == after.len()
        && before.mtime() == after.mtime()
        && before.mtime_nsec() == after.mtime_nsec()
        && before.ctime() == after.ctime()
        && before.ctime_nsec() == after.ctime_nsec()
}

/// Resolve the parent explicitly selected by the user. The named item itself
/// must be an ordinary file or folder and is opened without following links.
pub fn open_source(path: &Path) -> Result<File> {
    if !path.is_absolute() {
        return Err(invalid("Choose an absolute file path."));
    }
    let parent = fs::canonicalize(
        path.parent()
            .ok_or_else(|| invalid("Choose a named item."))?,
    )?;
    open_child(&open_folder(&parent)?, name_of(path)?)
}

pub fn open_child(folder: &File, name: &OsStr) -> Result<File> {
    let name = c_name(name)?;
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // The directory descriptor stays open throughout lookup and open. Reject
    // symbolic links and special files before opening the child.
    if unsafe {
        libc::fstatat(
            folder.as_raw_fd(),
            name.as_ptr(),
            status.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io::Error::last_os_error().into());
    }
    // A successful fstatat initialized the entire structure.
    let status = unsafe { status.assume_init() };
    let kind = status.st_mode & libc::S_IFMT;
    if kind != libc::S_IFREG && kind != libc::S_IFDIR {
        return Err(invalid(
            "ZIP supports ordinary files and folders. Remove links and special files from the selection.",
        ));
    }
    // NOFOLLOW excludes a link installed after fstatat. NONBLOCK prevents
    // waiting on a pipe if a file is replaced during the open.
    let fd = unsafe {
        libc::openat(
            folder.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // The successful open returned a new descriptor owned by this File.
    let child = unsafe { File::from_raw_fd(fd) };
    let actual = child.metadata()?;
    if actual.dev() != status.st_dev as u64
        || actual.ino() != status.st_ino
        || !(actual.is_file() || actual.is_dir())
    {
        return Err(invalid("A selected item changed. Select it again."));
    }
    Ok(child)
}

struct DirStream(*mut libc::DIR);

impl Drop for DirStream {
    fn drop(&mut self) {
        // This stream owns its descriptor after a successful fdopendir.
        unsafe { libc::closedir(self.0) };
    }
}

pub fn children(folder: &File, max_entries: usize) -> Result<Vec<OsString>> {
    // fdopendir takes ownership, so duplicate the caller's descriptor.
    let fd = unsafe { libc::fcntl(folder.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // On failure, fdopendir leaves the descriptor with the caller.
    let pointer = unsafe { libc::fdopendir(fd) };
    if pointer.is_null() {
        let error = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(error.into());
    }
    let directory = DirStream(pointer);
    let mut names = Vec::new();
    loop {
        // POSIX readdir uses null for both end-of-directory and errors.
        let item = unsafe {
            *libc::__error() = 0;
            libc::readdir(directory.0)
        };
        if item.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(0) {
                break;
            }
            return Err(error.into());
        }
        // Copy the NUL-terminated name before the next readdir changes it.
        let name = unsafe { CStr::from_ptr((*item).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        if names.len() == max_entries {
            return Err(invalid("The selection contains too many ZIP entries."));
        }
        names.push(OsString::from_vec(name.to_vec()));
    }
    names.sort_unstable();
    Ok(names)
}

#[derive(Clone, Copy)]
pub struct DestinationIdentity {
    device: i32,
    inode: u64,
}

impl DestinationIdentity {
    pub fn parse(value: &str) -> Result<Self> {
        let (device, inode) = value
            .split_once(':')
            .ok_or_else(|| invalid("The destination identity is invalid."))?;
        Ok(Self {
            device: device.parse()?,
            inode: inode.parse()?,
        })
    }

    fn matches(self, actual: &Metadata) -> bool {
        actual.dev() == self.device as u64 && actual.ino() == self.inode
    }
}

pub struct Destination {
    pub path: PathBuf,
    folder: File,
    identity: Metadata,
}

impl Destination {
    pub fn open(path: &Path, expected: Option<DestinationIdentity>) -> Result<Self> {
        if !path.is_absolute() {
            return Err(invalid("Choose an absolute destination folder."));
        }
        let path = fs::canonicalize(path)?;
        if path.to_str().is_none() {
            return Err(invalid(
                "Choose a destination whose path can be represented as text.",
            ));
        }
        let folder = open_folder(&path)?;
        let identity = folder.metadata()?;
        if expected.is_some_and(|expected| !expected.matches(&identity)) {
            return Err(invalid("The destination folder changed. Choose it again."));
        }
        Ok(Self {
            path,
            folder,
            identity,
        })
    }

    fn verify(&self) -> Result<()> {
        let actual = fs::symlink_metadata(&self.path)?;
        if actual.is_dir() && same_entry(&actual, &self.identity) {
            Ok(())
        } else {
            Err(invalid("The destination folder changed. Choose it again."))
        }
    }

    pub fn stage(&self) -> Result<TempDir> {
        self.verify()?;
        let stage = Builder::new()
            .prefix(".richfiles-zip-")
            .permissions(Permissions::from_mode(0o700))
            .tempdir_in(&self.path)?;
        self.verify()?;
        Ok(stage)
    }

    pub fn publish_file(&self, stage: &TempDir, source_name: &str, name: &str) -> Result<PathBuf> {
        let source = open_folder(stage.path())?;
        self.publish(&source, OsStr::new(source_name), name)
    }

    pub fn publish_folder(&self, stage: TempDir, name: &str) -> Result<PathBuf> {
        let staged_name = name_of(stage.path())?.to_owned();
        let expected = open_folder(stage.path())?.metadata()?;
        self.rename_exclusive(&self.folder, &staged_name, name)?;
        // Publication moved the directory. Disarm cleanup of the former path
        // before verifying the result.
        let _former_path = stage.keep();
        self.verify_publication(name, &expected)
    }

    fn publish(&self, source: &File, source_name: &OsStr, name: &str) -> Result<PathBuf> {
        let expected = open_child(source, source_name)?.metadata()?;
        self.rename_exclusive(source, source_name, name)?;
        self.verify_publication(name, &expected)
    }

    fn rename_exclusive(&self, source: &File, source_name: &OsStr, name: &str) -> Result<()> {
        self.verify()?;
        let source_name = c_name(source_name)?;
        let destination_name = CString::new(name)?;
        check_cancelled()?;
        // Both names are single components and the two descriptors are open.
        // RENAME_EXCL makes checking and publishing one no-replace operation.
        let result = unsafe {
            libc::renameatx_np(
                source.as_raw_fd(),
                source_name.as_ptr(),
                self.folder.as_raw_fd(),
                destination_name.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::AlreadyExists {
                Err(invalid(
                    "An item already exists with that destination name.",
                ))
            } else {
                Err(error.into())
            }
        }
    }

    fn verify_publication(&self, name: &str, expected: &Metadata) -> Result<PathBuf> {
        let path = self.path.join(name);
        let verified = (|| -> Result<bool> {
            self.verify()?;
            Ok(same_entry(expected, &fs::symlink_metadata(&path)?))
        })();
        if matches!(verified, Ok(true)) {
            Ok(path)
        } else {
            Err(invalid(
                "macOS accepted publication, but the result could not be verified. Check the destination before retrying.",
            ))
        }
    }
}
