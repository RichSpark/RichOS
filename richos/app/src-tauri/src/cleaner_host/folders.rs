//! Two levels of folder labels under the configured root. Directory descriptors
//! keep discovery from following a child that Finder replaces with a symlink.
use richos_core::cleaner::{choice::FolderContext, folder_parts};
use std::{
    collections::HashSet,
    ffi::{CStr, CString, OsStr},
    fs::{File, OpenOptions},
    io,
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    ptr::NonNull,
};

const MAX_LABELS: usize = 1_000;
const MAX_ENTRIES: usize = 40_000;
// File entries consume I/O even though only folders become labels. Leave part
// of the overall budget for siblings when a directory holds a large archive.
const MAX_DIRECTORY_ENTRIES: usize = 12_000;
const DIRECTORY_FLAGS: i32 = libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

struct Directory(NonNull<libc::DIR>);

impl Directory {
    fn from_file(file: File) -> io::Result<Self> {
        let fd = file.into_raw_fd();
        // fdopendir takes ownership only on success.
        if let Some(stream) = NonNull::new(unsafe { libc::fdopendir(fd) }) {
            return Ok(Self(stream));
        }
        let error = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        Err(error)
    }

    fn open(root: &Path) -> io::Result<Self> {
        let mut parts = root.components();
        if parts.next() != Some(Component::RootDir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The filing root must be absolute",
            ));
        }
        let mut directory = OpenOptions::new()
            .read(true)
            .custom_flags(DIRECTORY_FLAGS)
            .open("/")?;
        // A previously validated path can change while the operation waits.
        // Open ancestors for search only, since we do not need to list them.
        for part in parts {
            let Component::Normal(name) = part else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "The filing root is not canonical",
                ));
            };
            directory = Self::child_file(directory.as_raw_fd(), name, libc::O_SEARCH)?;
        }
        Self::from_file(Self::child_file(
            directory.as_raw_fd(),
            OsStr::new("."),
            libc::O_RDONLY,
        )?)
    }

    fn child(&self, name: &str) -> io::Result<Self> {
        let parent = unsafe { libc::dirfd(self.0.as_ptr()) };
        Self::from_file(Self::child_file(parent, OsStr::new(name), libc::O_RDONLY)?)
    }

    fn child_file(parent: RawFd, name: &OsStr, mode: i32) -> io::Result<File> {
        let name = CString::new(name.as_bytes())?;
        let fd = unsafe { libc::openat(parent, name.as_ptr(), mode | DIRECTORY_FLAGS) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn names(&mut self, budget: &mut usize, complete: &mut bool) -> Vec<String> {
        let mut names = Vec::new();
        let mut directory_budget = MAX_DIRECTORY_ENTRIES;
        loop {
            if *budget == 0 || directory_budget == 0 {
                *complete = false;
                break;
            }
            // readdir uses a null return for EOF and errors. Reset the thread's
            // errno before the call so a previous operation cannot mark EOF incomplete.
            let entry = unsafe {
                *libc::__error() = 0;
                libc::readdir(self.0.as_ptr()).as_ref()
            };
            let Some(entry) = entry else {
                *complete &= unsafe { *libc::__error() == 0 };
                break;
            };
            *budget -= 1;
            directory_budget -= 1;
            if !matches!(entry.d_type, libc::DT_DIR | libc::DT_UNKNOWN) {
                continue;
            }
            let name = unsafe { CStr::from_ptr(entry.d_name.as_ptr()) };
            let Ok(text) = name.to_str() else { continue };
            if folder_parts(text).is_err() {
                continue;
            }
            let directory = match entry.d_type {
                libc::DT_DIR => true,
                libc::DT_UNKNOWN => {
                    let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
                    let status = unsafe {
                        libc::fstatat(
                            libc::dirfd(self.0.as_ptr()),
                            name.as_ptr(),
                            info.as_mut_ptr(),
                            libc::AT_SYMLINK_NOFOLLOW,
                        )
                    };
                    if status == 0 {
                        unsafe { info.assume_init() }.st_mode & libc::S_IFMT == libc::S_IFDIR
                    } else {
                        *complete = false;
                        false
                    }
                }
                _ => false,
            };
            if directory {
                if names.len() == MAX_LABELS {
                    *complete = false;
                    break;
                }
                names.push(text.to_owned());
            }
        }
        names.sort_unstable();
        names
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        unsafe { libc::closedir(self.0.as_ptr()) };
    }
}

/// The root as Directory::open can walk it. macOS's own top-level links (/var, /tmp and /etc
/// point into /private) are root-owned on the read-only system volume, so nothing can swap
/// them; a root reached through one opens at its /private spelling. Every other symlink in the
/// root's path is still refused, and links inside the root are still never followed.
fn system_spelling(root: &Path) -> PathBuf {
    for link in ["var", "tmp", "etc"] {
        let top = Path::new("/").join(link);
        let Ok(rest) = root.strip_prefix(&top) else {
            continue;
        };
        let private = Path::new("private").join(link);
        if std::fs::read_link(&top).is_ok_and(|target| target == private) {
            return Path::new("/").join(private).join(rest);
        }
    }
    root.to_path_buf()
}

pub(super) fn scan(root: &Path, recent: &[PathBuf]) -> FolderContext {
    let mut context = FolderContext {
        labels: Vec::new(),
        complete: true,
    };
    let Ok(mut directory) = Directory::open(&system_spelling(root)) else {
        context.complete = false;
        return context;
    };
    let mut seen = HashSet::new();
    for path in recent {
        let Some(label) = path
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .and_then(Path::to_str)
        else {
            continue;
        };
        let Ok(parts) = folder_parts(label) else {
            continue;
        };
        let Ok(parent) = directory.child(&parts[0]) else {
            continue;
        };
        if parts.get(1).is_some_and(|part| parent.child(part).is_err()) {
            continue;
        }
        if seen.insert(label.to_owned()) {
            context.labels.push(label.to_owned());
        }
    }
    let mut remaining = MAX_ENTRIES;
    let parents = directory.names(&mut remaining, &mut context.complete);
    for parent in &parents {
        if seen.insert(parent.clone()) {
            if context.labels.len() == MAX_LABELS {
                context.complete = false;
                return context;
            }
            context.labels.push(parent.clone());
        }
    }
    for parent in parents {
        if remaining == 0 {
            context.complete = false;
            break;
        }
        let Ok(mut child) = directory.child(&parent) else {
            context.complete = false;
            continue;
        };
        for name in child.names(&mut remaining, &mut context.complete) {
            let label = format!("{parent}/{name}");
            if folder_parts(&label).is_err() || !seen.insert(label.clone()) {
                continue;
            }
            if context.labels.len() == MAX_LABELS {
                context.complete = false;
                return context;
            }
            context.labels.push(label);
        }
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_files_do_not_hide_sibling_categories() {
        let root = std::env::temp_dir().join(format!(
            "richcleaner-folder-budget-{}",
            richos_core::cleaner::capture_id()
        ));
        let archive = root.join("Archive");
        std::fs::create_dir_all(&archive).unwrap();
        std::fs::create_dir_all(root.join("Projects/Notes")).unwrap();
        for number in 0..10_000 {
            File::create(archive.join(format!("file-{number}"))).unwrap();
        }
        let below_cap = scan(&root, &[]);
        for number in 10_000..=MAX_DIRECTORY_ENTRIES {
            File::create(archive.join(format!("file-{number}"))).unwrap();
        }
        let at_cap = scan(&root, &[]);
        std::fs::remove_dir_all(root).unwrap();

        assert!(below_cap.complete);
        assert_eq!(below_cap.labels, ["Archive", "Projects", "Projects/Notes"]);
        assert!(!at_cap.complete);
        assert_eq!(at_cap.labels, below_cap.labels);
    }

    #[test]
    fn a_root_under_the_systems_own_links_opens_and_a_user_link_is_still_refused() {
        let fixture = Path::new("/tmp").join(format!(
            "richcleaner-system-link-{}",
            richos_core::cleaner::capture_id()
        ));
        std::fs::create_dir_all(fixture.join("Filing/Projects")).unwrap();
        std::os::unix::fs::symlink(fixture.join("Filing"), fixture.join("Linked")).unwrap();
        let through_tmp = scan(&fixture.join("Filing"), &[]);
        let through_user_link = scan(&fixture.join("Linked"), &[]);
        std::fs::remove_dir_all(&fixture).unwrap();

        assert!(through_tmp.complete);
        assert_eq!(through_tmp.labels, ["Projects"]);
        assert!(through_user_link.labels.is_empty());
        assert!(!through_user_link.complete);
    }
}
