//! Disposable numeric checkpoints. Existing input files remain the source of truth.
use crate::{ledger::Ledger, swearmeter::DETECTOR_VERSION, swearmeter_ledger::Frozen,
    swearmeter_questions::Summary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, Metadata}, io::{self, Read, Write}, path::Path};

/// File generation, not merely its path or modification time. A replaced file or
/// same-length edit cannot inherit a checkpoint on supported Unix installations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStamp { device: u64, inode: u64, bytes: u64, modified: (i64, i64), changed: (i64, i64) }
impl SourceStamp {
    pub fn from_metadata(metadata: &Metadata) -> io::Result<Self> {
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            Ok(Self { device: metadata.dev(), inode: metadata.ino(), bytes: metadata.len(),
                modified: (metadata.mtime(), metadata.mtime_nsec()), changed: (metadata.ctime(), metadata.ctime_nsec()) })
        }
        #[cfg(not(unix))] { let _ = metadata; Err(io::Error::new(io::ErrorKind::Unsupported, "checkpoint source identity unavailable")) }
    }
    pub fn at(path: &Path) -> io::Result<Option<Self>> {
        match fs::metadata(path) {
            Ok(metadata) => Self::from_metadata(&metadata).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Read the actual immutable file handle. A later rename cannot attach the new
/// file's fingerprint to the old file's contents.
pub(crate) fn read_source(path: &Path) -> io::Result<(Vec<u8>, Option<SourceStamp>)> {
    let mut file = File::open(path)?;
    let before = SourceStamp::from_metadata(&file.metadata()?).ok();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let after = SourceStamp::from_metadata(&file.metadata()?).ok();
    if before != after { return Err(io::Error::new(io::ErrorKind::WouldBlock, "input history changed while it was read")); }
    Ok((bytes, after))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedCards { pub source: Option<SourceStamp>, pub summary: Summary }
#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    version: u32,
    reader_schema: u32,
    pub ledger_source: SourceStamp,
    pub ledger: Frozen,
    pub cards: CachedCards,
}
impl Checkpoint {
    /// Invalid, stale and damaged caches are ordinary cache misses, never zeros.
    pub fn load(path: &Path, ledger: &Path, cards: &Path) -> Option<Self> {
        let bytes = fs::read(path).ok()?;
        let (checksum, payload) = bytes.split_at_checked(32)?;
        if Sha256::digest(payload).as_slice() != checksum { return None; }
        let cache: Self = serde_json::from_slice(payload).ok()?;
        if cache.version != DETECTOR_VERSION || cache.reader_schema != crate::skip::WRITER_SCHEMA_VERSION || !cache.ledger.valid()
            || SourceStamp::at(ledger).ok()? != Some(cache.ledger_source.clone())
            || SourceStamp::at(cards).ok()? != cache.cards.source { return None; }
        Some(cache)
    }

    /// Called at backfill/shutdown boundaries, never for a match or visibility click.
    /// No fsync is needed: this file is disposable and a crash repairs it from sources.
    pub fn save(path: &Path, ledger_path: &Path, cards_path: &Path, ledger: &Ledger, cards: CachedCards) -> io::Result<()> {
        let source = ledger.swearmeter_source_stamp().cloned().ok_or_else(|| io::Error::other("ledger snapshot has no stable source generation"))?;
        let cache = Self { version: DETECTOR_VERSION, reader_schema: crate::skip::WRITER_SCHEMA_VERSION, ledger_source: source.clone(), ledger: ledger.freeze_swearmeter(), cards };
        if !cache.ledger.valid() { return Err(io::Error::other("numeric checkpoint did not reconcile")); }
        let payload = serde_json::to_vec(&cache)?;
        let matches = || SourceStamp::at(ledger_path).ok() == Some(Some(source.clone()))
            && SourceStamp::at(cards_path).ok() == Some(cache.cards.source.clone());
        if !matches() { return Err(io::Error::other("input history changed before checkpoint save")); }
        let temporary = path.with_extension(format!("{}.new", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = File::options().create_new(true).write(true).open(&temporary)?;
            file.write_all(&Sha256::digest(&payload))?;
            file.write_all(&payload)?;
            drop(file);
            if !matches() { return Err(io::Error::other("input history changed during checkpoint save")); }
            fs::rename(&temporary, path)
        })();
        if result.is_err() { drop(fs::remove_file(&temporary)); }
        result
    }
}
