//! The small record that says which files the meaning index holds, and in what state.
//!
//! Search PRD v3.1, "Indexing and source access": *"Keep a small manifest with canonical source
//! ID, current content hash, passage locators and embedding/chunking identity. … Mark a file
//! 'indexing' until its passages are flushed, then complete; on startup redo any file still
//! marked indexing."*
//!
//! The manifest is written whole, to a temporary file that is synced and then renamed over the
//! old one, so a crash leaves either the previous manifest or the new one, never half of one.
//! The point identifiers a file WILL use are recorded when it is marked indexing, before any
//! point is written, so a restart can always remove what an interrupted batch left behind.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Indexing,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntry {
    pub rel_path: String,
    pub hash: String,
    pub state: SourceState,
    /// One point per passage, in passage order.
    pub points: Vec<u64>,
    /// Each passage's byte range and first line, in passage order.
    pub locators: Vec<(usize, usize, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    /// Embedder identity and chunker identity together; vectors of another identity live in
    /// another index folder.
    pub identity: String,
    pub sources: BTreeMap<String, SourceEntry>,
}

impl Manifest {
    pub fn new(identity: &str) -> Self {
        Manifest { version: MANIFEST_VERSION, identity: identity.to_string(), sources: BTreeMap::new() }
    }

    pub fn path(dir: &Path) -> PathBuf {
        dir.join("manifest.json")
    }

    /// Read the manifest in `dir`; `Ok(None)` when there is none yet. A manifest that cannot be
    /// read, or belongs to another identity, is an error: the caller rebuilds the index rather
    /// than trusting it.
    pub fn load(dir: &Path, identity: &str) -> Result<Option<Self>, String> {
        let path = Self::path(dir);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("cannot read the search manifest: {e}")),
        };
        let manifest: Manifest = serde_json::from_str(&text).map_err(|e| format!("the search manifest is damaged: {e}"))?;
        if manifest.version != MANIFEST_VERSION || manifest.identity != identity {
            return Err("the search manifest belongs to another index".into());
        }
        Ok(Some(manifest))
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let path = Self::path(dir);
        let tmp = dir.join("manifest.json.tmp");
        let text = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut file = std::fs::File::create(&tmp).map_err(|e| format!("cannot write the search manifest: {e}"))?;
        file.write_all(&text).and_then(|_| file.sync_all()).map_err(|e| format!("cannot write the search manifest: {e}"))?;
        drop(file);
        std::fs::rename(&tmp, &path).map_err(|e| format!("cannot write the search manifest: {e}"))?;
        if let Ok(d) = std::fs::File::open(dir) {
            // The rename is already durable on APFS; a folder that cannot be synced is said.
            if let Err(error) = d.sync_all() {
                eprintln!("search manifest: the folder could not be synced: {error}");
            }
        }
        Ok(())
    }
}

/// A point's identifier: fixed by the source, its content and the passage number, so a retry
/// writes the same points again instead of adding new ones.
pub fn point_id(source_id: &str, hash: &str, passage: usize) -> u64 {
    let mut h = Sha256::new();
    h.update(source_id.as_bytes());
    h.update([0]);
    h.update(hash.as_bytes());
    h.update([0]);
    h.update((passage as u64).to_le_bytes());
    let digest = h.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    // Qdrant accepts any u64; zero is avoided only to keep it distinguishable from "unset".
    u64::from_le_bytes(bytes).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_atomically_and_refuses_another_identity() {
        let dir = std::env::temp_dir().join(format!("richos-search-manifest-{}", std::process::id()));
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("fixture cleanup: {error}");
            }
        }
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Manifest::load(&dir, "a").unwrap(), None);
        let mut m = Manifest::new("a");
        m.sources.insert(
            "doc-1".into(),
            SourceEntry { rel_path: "x.md".into(), hash: "h".into(), state: SourceState::Indexing, points: vec![point_id("doc-1", "h", 0)], locators: vec![(0, 5, 1)] },
        );
        m.save(&dir).unwrap();
        assert_eq!(Manifest::load(&dir, "a").unwrap().unwrap(), m);
        assert!(!dir.join("manifest.json.tmp").exists());
        assert!(Manifest::load(&dir, "b").is_err());
        std::fs::write(Manifest::path(&dir), "{not json").unwrap();
        assert!(Manifest::load(&dir, "a").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn point_ids_are_stable_per_source_content_and_passage() {
        assert_eq!(point_id("d", "h", 3), point_id("d", "h", 3));
        assert_ne!(point_id("d", "h", 3), point_id("d", "h", 4));
        assert_ne!(point_id("d", "h", 3), point_id("d", "h2", 3));
        assert_ne!(point_id("d", "h", 3), point_id("e", "h", 3));
    }
}
