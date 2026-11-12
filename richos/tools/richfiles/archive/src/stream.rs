use std::collections::HashSet;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::fs::{File, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};

use crate::archive::{
    Entry, MAX_ARCHIVE, MAX_ENTRIES, ManifestBudget, chosen_entries, copy_exact, open_output,
    validate_component, validate_path,
};
use crate::native::{self, Destination};
use crate::{Result, check_cancelled, invalid};

const ARCHIVE_OK: c_int = 0;
const ARCHIVE_EOF: c_int = 1;

#[link(name = "archive")]
unsafe extern "C" {
    fn archive_read_new() -> *mut c_void;
    fn archive_read_free(archive: *mut c_void) -> c_int;
    fn archive_read_support_filter_gzip(archive: *mut c_void) -> c_int;
    fn archive_read_support_filter_bzip2(archive: *mut c_void) -> c_int;
    fn archive_read_support_filter_xz(archive: *mut c_void) -> c_int;
    fn archive_read_support_format_tar(archive: *mut c_void) -> c_int;
    fn archive_read_support_format_7zip(archive: *mut c_void) -> c_int;
    fn archive_read_support_format_rar(archive: *mut c_void) -> c_int;
    fn archive_read_support_format_rar5(archive: *mut c_void) -> c_int;
    fn archive_read_open_fd(archive: *mut c_void, descriptor: c_int, block_size: usize) -> c_int;
    fn archive_read_next_header(archive: *mut c_void, entry: *mut *mut c_void) -> c_int;
    fn archive_read_data(archive: *mut c_void, output: *mut c_void, length: usize) -> isize;
    fn archive_read_data_skip(archive: *mut c_void) -> c_int;
    fn archive_entry_pathname(entry: *mut c_void) -> *const c_char;
    fn archive_entry_pathname_utf8(entry: *mut c_void) -> *const c_char;
    fn archive_entry_size(entry: *mut c_void) -> i64;
    fn archive_entry_size_is_set(entry: *mut c_void) -> c_int;
    fn archive_entry_filetype(entry: *mut c_void) -> libc::mode_t;
    fn archive_entry_hardlink(entry: *mut c_void) -> *const c_char;
    fn archive_entry_is_encrypted(entry: *mut c_void) -> c_int;
    fn archive_error_string(archive: *mut c_void) -> *const c_char;
}

pub fn list(source: &Path) -> Result<Vec<Entry>> {
    Snapshot::open(source)?.catalog()
}

pub fn extract(
    source: &Path,
    destination: &Destination,
    name: &str,
    selected: &[usize],
) -> Result<PathBuf> {
    validate_component(name)?;
    let source = Snapshot::open(source)?;
    let entries = source.catalog()?;
    let indices = chosen_entries(&entries, selected)?;
    let stage = destination.stage()?;
    let mut directories = HashSet::new();
    let mut reader = Reader::open(&source.handle)?;
    let mut selected = indices.into_iter().peekable();
    for entry in &entries {
        if reader.next_entry(entry.index)?.as_ref() != Some(entry) {
            return Err(invalid("The archive catalog changed while extracting."));
        }
        if selected.peek() == Some(&entry.index) {
            selected.next();
            if let Some(mut output) = open_output(stage.path(), entry, &mut directories)? {
                copy_exact(&mut reader, &mut output, entry.size)?;
                output.sync_all()?;
            } else {
                reader.skip()?;
            }
        } else {
            reader.skip()?;
        }
    }
    if reader.next_entry(entries.len())?.is_some() {
        return Err(invalid("The archive catalog changed while extracting."));
    }
    source.verify()?;
    check_cancelled()?;
    destination.publish_folder(stage, name)
}

struct Snapshot {
    handle: File,
    before: Metadata,
}

impl Snapshot {
    fn open(source: &Path) -> Result<Self> {
        check_cancelled()?;
        let handle = native::open_source(source)?;
        let before = handle.metadata()?;
        if !before.is_file() || before.len() > MAX_ARCHIVE {
            return Err(invalid(
                "Choose a regular archive within the supported size limit.",
            ));
        }
        Ok(Self { handle, before })
    }

    fn catalog(&self) -> Result<Vec<Entry>> {
        let mut reader = Reader::open(&self.handle)?;
        let mut entries = Vec::new();
        let mut budget = ManifestBudget::default();
        while let Some(entry) = reader.next_entry(entries.len())? {
            budget.add(&entry.path, entry.size)?;
            reader.skip()?;
            entries.push(entry);
        }
        self.verify()?;
        Ok(entries)
    }

    fn verify(&self) -> Result<()> {
        if !native::same_version(&self.before, &self.handle.metadata()?) {
            return Err(invalid(
                "The archive changed while reading it. Open it again.",
            ));
        }
        Ok(())
    }
}

struct Reader<'a> {
    pointer: NonNull<c_void>,
    _input: &'a File,
    observed: usize,
}

impl<'a> Reader<'a> {
    fn open(mut input: &'a File) -> Result<Self> {
        input.seek(SeekFrom::Start(0))?;
        // The handle owns a decoder instance and borrows the open file descriptor.
        let pointer = NonNull::new(unsafe { archive_read_new() })
            .ok_or_else(|| invalid("The archive decoder could not start."))?;
        let reader = Self {
            pointer,
            _input: input,
            observed: 0,
        };
        // Enable only the listed formats and in-process filters.
        for enable in [
            archive_read_support_filter_gzip,
            archive_read_support_filter_bzip2,
            archive_read_support_filter_xz,
            archive_read_support_format_tar,
            archive_read_support_format_7zip,
            archive_read_support_format_rar,
            archive_read_support_format_rar5,
        ] {
            reader.check(unsafe { enable(pointer.as_ptr()) })?;
        }
        reader.check(unsafe {
            archive_read_open_fd(pointer.as_ptr(), input.as_raw_fd(), 64 * 1024)
        })?;
        Ok(reader)
    }

    fn check(&self, status: c_int) -> io::Result<()> {
        if status == ARCHIVE_OK {
            return Ok(());
        }
        let details = unsafe { archive_error_string(self.pointer.as_ptr()) };
        let message = if details.is_null() {
            "The archive could not be read.".to_owned()
        } else {
            unsafe { CStr::from_ptr(details) }
                .to_string_lossy()
                .into_owned()
        };
        Err(io::Error::new(io::ErrorKind::InvalidData, message))
    }

    fn next_entry(&mut self, index: usize) -> Result<Option<Entry>> {
        loop {
            check_cancelled()?;
            let mut entry = ptr::null_mut();
            let status = unsafe { archive_read_next_header(self.pointer.as_ptr(), &mut entry) };
            if status == ARCHIVE_EOF {
                return Ok(None);
            }
            self.check(status)?;
            if entry.is_null() || self.observed > MAX_ENTRIES {
                return Err(invalid(
                    "The archive catalog exceeds the supported entry limit.",
                ));
            }
            self.observed += 1;
            // The current header and its strings are borrowed only until the next read.
            let (path, kind, size) = unsafe {
                if archive_entry_is_encrypted(entry) != 0 {
                    return Err(invalid("Password-protected archives are unavailable."));
                }
                if !archive_entry_hardlink(entry).is_null() {
                    return Err(invalid("The archive contains a hard link."));
                }
                let kind = archive_entry_filetype(entry);
                if kind != libc::S_IFREG && kind != libc::S_IFDIR {
                    return Err(invalid("The archive contains a link or a special file."));
                }
                let utf8 = archive_entry_pathname_utf8(entry);
                let name = if utf8.is_null() {
                    archive_entry_pathname(entry)
                } else {
                    utf8
                };
                if name.is_null() || archive_entry_size_is_set(entry) == 0 {
                    return Err(invalid("The archive entry has a missing name or size."));
                }
                (
                    CStr::from_ptr(name).to_str()?,
                    kind,
                    archive_entry_size(entry),
                )
            };
            let is_directory = kind == libc::S_IFDIR;
            if is_directory && size != 0 {
                return Err(invalid("An archive directory contains unexpected data."));
            }
            // A relative dot prefix is normal in TAR archives created from a folder.
            if is_directory && (path == "." || path == "./") {
                self.skip()?;
                continue;
            }
            let path = path.strip_prefix("./").unwrap_or(path);
            let path = if is_directory {
                path.strip_suffix('/').unwrap_or(path)
            } else {
                path
            };
            validate_path(path)?;
            return Ok(Some(Entry {
                index,
                path: path.to_owned(),
                is_directory,
                size: u64::try_from(size)?,
            }));
        }
    }

    fn skip(&self) -> Result<()> {
        check_cancelled()?;
        self.check(unsafe { archive_read_data_skip(self.pointer.as_ptr()) })?;
        Ok(())
    }
}

impl Read for Reader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = unsafe {
            archive_read_data(
                self.pointer.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if count < 0 {
            self.check(count as c_int)?;
        }
        Ok(count as usize)
    }
}

impl Drop for Reader<'_> {
    fn drop(&mut self) {
        unsafe { archive_read_free(self.pointer.as_ptr()) };
    }
}
