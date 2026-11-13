use std::collections::HashSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::native::{self, Destination};
use crate::{Result, check_cancelled, invalid};

pub(super) const MAX_ENTRIES: usize = 20_000;
const MAX_CONTENT: u64 = 16 * 1024 * 1024 * 1024;
pub(super) const MAX_ARCHIVE: u64 = MAX_CONTENT + 128 * 1024 * 1024;
const MAX_CATALOG: u64 = 32 * 1024 * 1024;
const MAX_PATH: usize = 1024;
const MAX_DEPTH: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub index: usize,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
}

pub fn list(path: &Path) -> Result<Vec<Entry>> {
    let snapshot = ArchiveSnapshot::open(path)?;
    snapshot.verify()?;
    Ok(snapshot.entries)
}

pub fn create(destination: &Destination, name: &str, sources: &[PathBuf]) -> Result<PathBuf> {
    validate_component(name)?;
    if !name.to_ascii_lowercase().ends_with(".zip") {
        return Err(invalid("Give the new archive a .zip extension."));
    }
    if sources.is_empty() {
        return Err(invalid("Choose at least one file or folder."));
    }
    if sources.len() > MAX_ENTRIES {
        return Err(invalid("The selection contains too many ZIP entries."));
    }
    let stage = destination.stage()?;
    let stage_identity = fs::symlink_metadata(stage.path())?;
    let staged_name = "archive.zip";
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(stage.path().join(staged_name))?;
    let mut writer = ZipWriter::new(output);
    let mut budget = ManifestBudget::default();

    for path in sources {
        check_cancelled()?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Choose an item with a name that ZIP can represent."))?;
        validate_component(name)?;
        let input = native::open_source(path)?;
        if native::same_entry(&input.metadata()?, &stage_identity) {
            return Err(invalid(
                "Choose an item outside the archive's staging folder.",
            ));
        }
        add_entry(&mut writer, input, name, &mut budget, &stage_identity)?;
    }

    check_cancelled()?;
    writer.finish()?.sync_all()?;
    destination.publish_file(&stage, staged_name, name)
}

pub fn extract(
    source: &Path,
    destination: &Destination,
    name: &str,
    selected: &[usize],
) -> Result<PathBuf> {
    validate_component(name)?;
    let mut snapshot = ArchiveSnapshot::open(source)?;
    let indices = chosen_entries(&snapshot.entries, selected)?;
    let stage = destination.stage()?;
    let mut created_directories = HashSet::new();

    for index in indices {
        check_cancelled()?;
        let entry = &snapshot.entries[index];
        let Some(mut output) = open_output(stage.path(), entry, &mut created_directories)? else {
            continue;
        };
        let mut input = snapshot.archive.by_index(index)?;
        copy_exact(&mut input, &mut output, entry.size)?;
        output.sync_all()?;
    }

    snapshot.verify()?;
    check_cancelled()?;
    destination.publish_folder(stage, name)
}

#[derive(Default)]
pub(super) struct ManifestBudget {
    paths: HashSet<String>,
    bytes: u64,
}

impl ManifestBudget {
    pub(super) fn add(&mut self, path: &str, size: u64) -> Result<()> {
        if self.paths.len() == MAX_ENTRIES {
            return Err(invalid("This archive has too many entries."));
        }
        if !self.paths.insert(path.to_owned()) {
            return Err(invalid("This archive contains duplicate paths."));
        }
        self.bytes = self
            .bytes
            .checked_add(size)
            .filter(|bytes| *bytes <= MAX_CONTENT)
            .ok_or_else(|| invalid("This archive exceeds the 16 GiB uncompressed limit."))?;
        Ok(())
    }
}

fn add_entry(
    writer: &mut ZipWriter<File>,
    mut input: File,
    path: &str,
    budget: &mut ManifestBudget,
    stage_identity: &Metadata,
) -> Result<()> {
    check_cancelled()?;
    let before = input.metadata()?;
    let is_directory = before.is_dir();
    validate_path(path)?;
    budget.add(path, if is_directory { 0 } else { before.len() })?;

    if is_directory {
        writer.add_directory(format!("{path}/"), SimpleFileOptions::default())?;
        for name in native::children(&input, MAX_ENTRIES - budget.paths.len())? {
            let child = native::open_child(&input, &name)?;
            if native::same_entry(&child.metadata()?, stage_identity) {
                // The destination may be inside the selected folder. The
                // private staging directory belongs to this operation.
                continue;
            }
            let name = name
                .to_str()
                .ok_or_else(|| invalid("A filename cannot be represented in ZIP."))?;
            validate_component(name)?;
            add_entry(
                writer,
                child,
                &format!("{path}/{name}"),
                budget,
                stage_identity,
            )?;
        }
    } else {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(before.len() >= u32::MAX.into());
        writer.start_file(path, options)?;
        copy_exact(&mut input, writer, before.len())?;
    }

    if !native::same_version(&before, &input.metadata()?) {
        return Err(invalid(
            "A source changed while creating the ZIP. Try again after it stops changing.",
        ));
    }
    Ok(())
}

pub(super) fn copy_exact(
    input: &mut impl Read,
    output: &mut impl Write,
    expected: u64,
) -> Result<()> {
    let mut buffer = [0; 64 * 1024];
    let mut remaining = expected;
    loop {
        check_cancelled()?;
        // Read at most one byte beyond the expected length. That checks EOF
        // and lets the ZIP decoder validate the checksum without extra output.
        let limit = (remaining + 1).min(buffer.len() as u64) as usize;
        let count = input.read(&mut buffer[..limit])?;
        if count == 0 {
            break;
        }
        if count as u64 > remaining {
            return Err(invalid(
                "An item contains more data than its expected size.",
            ));
        }
        output.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    if remaining != 0 {
        return Err(invalid("An item ended before its expected size."));
    }
    Ok(())
}

struct ArchiveSnapshot {
    handle: File,
    before: Metadata,
    archive: ZipArchive<BufReader<File>>,
    entries: Vec<Entry>,
}

impl ArchiveSnapshot {
    fn open(path: &Path) -> Result<Self> {
        check_cancelled()?;
        let handle = native::open_source(path)?;
        let before = handle.metadata()?;
        if !before.is_file() || before.len() > MAX_ARCHIVE {
            return Err(invalid(
                "Choose a regular ZIP within the supported size limit.",
            ));
        }
        let (catalog_start, entry_count) = catalog_bounds(&handle, before.len())?;
        let mut archive = ZipArchive::new(BufReader::new(handle.try_clone()?))?;
        if archive.central_directory_start() != catalog_start
            || archive.offset() != 0
            || archive.len() != entry_count
            || archive.has_duplicate_names()
            || archive.has_overlapping_entries()?
        {
            return Err(invalid(
                "The ZIP contains inconsistent or overlapping entries.",
            ));
        }
        let mut entries = Vec::with_capacity(entry_count);
        let mut budget = ManifestBudget::default();
        for index in 0..entry_count {
            check_cancelled()?;
            let file = archive.by_index_data(index)?;
            let is_directory = file.is_dir();
            let path = file.name()?;
            let path = if is_directory {
                path.strip_suffix('/').unwrap_or(&path)
            } else {
                &path
            };
            validate_path(path)?;
            if file.enclosed_name().is_none() || file.is_symlink() {
                return Err(invalid("The ZIP contains a link or an invalid entry path."));
            }
            let expected = if is_directory {
                libc::S_IFDIR
            } else {
                libc::S_IFREG
            } as u32;
            let kind = file.unix_mode().unwrap_or_default() & libc::S_IFMT as u32;
            if kind != 0 && kind != expected {
                return Err(invalid("The ZIP contains an unsupported special entry."));
            }
            if file.encrypted()
                || !matches!(
                    file.compression(),
                    CompressionMethod::Stored | CompressionMethod::Deflated
                )
            {
                return Err(invalid(
                    "This ZIP uses encryption or a compression method RichFiles cannot read.",
                ));
            }
            let size = file.size();
            if is_directory && size != 0 {
                return Err(invalid("A ZIP directory entry contains unexpected data."));
            }
            budget.add(path, size)?;
            entries.push(Entry {
                index,
                path: path.to_owned(),
                is_directory,
                size,
            });
        }
        let snapshot = Self {
            handle,
            before,
            archive,
            entries,
        };
        snapshot.verify()?;
        Ok(snapshot)
    }

    fn verify(&self) -> Result<()> {
        if !native::same_version(&self.before, &self.handle.metadata()?) {
            return Err(invalid("The ZIP changed while reading it. Open it again."));
        }
        Ok(())
    }
}

pub(super) fn chosen_entries(entries: &[Entry], selected: &[usize]) -> Result<Vec<usize>> {
    if selected.is_empty() {
        return Ok((0..entries.len()).collect());
    }
    let chosen: HashSet<usize> = selected.iter().copied().collect();
    if chosen.iter().any(|index| *index >= entries.len()) {
        return Err(invalid("A selected archive entry is no longer available."));
    }
    let folders: HashSet<&Path> = chosen
        .iter()
        .filter_map(|index| {
            let entry = &entries[*index];
            entry.is_directory.then(|| Path::new(&entry.path))
        })
        .collect();
    Ok(entries
        .iter()
        .filter(|entry| {
            chosen.contains(&entry.index)
                || Path::new(&entry.path)
                    .ancestors()
                    .skip(1)
                    .any(|path| folders.contains(path))
        })
        .map(|entry| entry.index)
        .collect())
}

pub(super) fn open_output(
    root: &Path,
    entry: &Entry,
    directories: &mut HashSet<PathBuf>,
) -> Result<Option<File>> {
    let path = Path::new(&entry.path);
    let parent = if entry.is_directory {
        path
    } else {
        path.parent()
            .ok_or_else(|| invalid("An archive entry has no parent path."))?
    };
    make_directories(root, parent, directories)?;
    if entry.is_directory {
        return Ok(None);
    }
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join(path))?;
    Ok(Some(output))
}

fn make_directories(root: &Path, relative: &Path, created: &mut HashSet<PathBuf>) -> Result<()> {
    let mut path = PathBuf::new();
    for component in relative.components() {
        path.push(component);
        if created.insert(path.clone()) {
            // Reuse only an exact path we created. Case-equivalent or
            // Unicode-equivalent paths must not silently merge.
            fs::create_dir(root.join(&path))?;
        }
    }
    Ok(())
}

pub(super) fn validate_component(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.len() > 255
        || value.contains(['/', '\\', ':'])
        || value.chars().any(char::is_control)
    {
        return Err(invalid(
            "The archive contains an unsupported file or folder name.",
        ));
    }
    Ok(())
}

pub(super) fn validate_path(path: &str) -> Result<()> {
    if path.len() > MAX_PATH || path.split('/').count() > MAX_DEPTH {
        return Err(invalid("An archive path is too long or too deeply nested."));
    }
    for component in path.split('/') {
        validate_component(component)?;
    }
    Ok(())
}

/// Bound the catalog before the ZIP library allocates its entry table. The
/// library still parses and validates the full ZIP records and streams.
fn catalog_bounds(input: &File, length: u64) -> Result<(u64, usize)> {
    const END: &[u8] = b"PK\x05\x06";
    const ZIP64_LOCATOR: &[u8] = b"PK\x06\x07";
    const ZIP64_END: &[u8] = b"PK\x06\x06";
    let tail_length = length.min(22 + u16::MAX as u64) as usize;
    let tail_start = length - tail_length as u64;
    let mut tail = vec![0; tail_length];
    input.read_exact_at(&mut tail, tail_start)?;
    let end_offset = tail
        .windows(22)
        .enumerate()
        .rev()
        .find_map(|(offset, end)| {
            (end.starts_with(END) && offset + 22 + short(end, 20) as usize == tail.len())
                .then_some(offset)
        })
        .ok_or_else(|| invalid("The ZIP catalog is missing or incomplete."))?;
    let end = &tail[end_offset..end_offset + 22];
    let end_absolute = tail_start + end_offset as u64;
    if short(end, 4) != 0 || short(end, 6) != 0 || short(end, 8) != short(end, 10) {
        return Err(invalid("Split ZIP archives are unavailable."));
    }
    let mut entries = short(end, 10) as u64;
    let mut size = long(end, 12) as u64;
    let mut start = long(end, 16) as u64;
    let mut catalog_end = end_absolute;

    if let Some(locator_offset) = end_absolute.checked_sub(20) {
        let mut locator = [0; 20];
        input.read_exact_at(&mut locator, locator_offset)?;
        if locator.starts_with(ZIP64_LOCATOR) {
            let record_offset = wide(&locator, 8);
            if long(&locator, 4) != 0
                || long(&locator, 16) != 1
                || record_offset
                    .checked_add(56)
                    .is_none_or(|end| end > locator_offset)
            {
                return Err(invalid("The ZIP64 catalog locator is invalid."));
            }
            let mut record = [0; 56];
            input.read_exact_at(&mut record, record_offset)?;
            let record_size = wide(&record, 4);
            if !record.starts_with(ZIP64_END)
                || !(44..=1_048_576).contains(&record_size)
                || record_offset.checked_add(12 + record_size) != Some(locator_offset)
                || long(&record, 16) != 0
                || long(&record, 20) != 0
                || wide(&record, 24) != wide(&record, 32)
            {
                return Err(invalid("The ZIP64 catalog is invalid."));
            }
            entries = wide(&record, 32);
            size = wide(&record, 40);
            start = wide(&record, 48);
            catalog_end = record_offset;
        }
    }
    if entries > MAX_ENTRIES as u64 || size > MAX_CATALOG {
        return Err(invalid(
            "The ZIP catalog exceeds the supported entry or metadata limit.",
        ));
    }
    if start.checked_add(size) != Some(catalog_end) || entries * 46 > size {
        return Err(invalid("The ZIP catalog has invalid bounds."));
    }
    Ok((start, entries as usize))
}

fn short(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes(bytes[index..index + 2].try_into().unwrap())
}

fn long(bytes: &[u8], index: usize) -> u32 {
    u32::from_le_bytes(bytes[index..index + 4].try_into().unwrap())
}

fn wide(bytes: &[u8], index: usize) -> u64 {
    u64::from_le_bytes(bytes[index..index + 8].try_into().unwrap())
}

#[cfg(test)]
mod checks {
    use super::*;

    #[test]
    fn accepts_only_unambiguous_relative_paths() {
        assert!(validate_path("Project/Photo.png").is_ok());
        for name in [
            "",
            ".",
            "..",
            "/Project",
            "Project//Photo.png",
            "Project/../Photo.png",
            "C:Photo.png",
        ] {
            assert!(validate_path(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn requires_a_stream_to_match_the_declared_size() -> Result<()> {
        let mut output = Vec::new();
        copy_exact(&mut &b"photo"[..], &mut output, 5)?;
        assert_eq!(output, b"photo");
        assert!(copy_exact(&mut &b"photo"[..], &mut Vec::new(), 6).is_err());
        let mut output = Vec::new();
        assert!(copy_exact(&mut &b"photo"[..], &mut output, 4).is_err());
        assert!(output.is_empty());
        Ok(())
    }
}
