//! The Qdrant Edge shard that holds one vector per passage.
//!
//! Search PRD v3.1, "Qdrant and the embedding model": *"Keep one app owner for each index
//! directory and pin the Edge release. Flush before marking a source complete … Call
//! optimization at the end of backfill or Reindex, away from interactive queries."*
//!
//! - **One owner.** [`VectorIndex::open`] takes an exclusive lock on `owner.lock` in the index
//!   folder for as long as the index is open; a second process (a second RichOS, or the harness
//!   pointed at the app's index) is refused by name instead of sharing a writable shard.
//! - **Points carry no payload.** A point's identifier is derived from its source, content hash
//!   and passage number ([`crate::manifest::point_id`]); the manifest maps it back. Nothing
//!   about the CEO's text is stored in Qdrant except the vectors themselves.
//! - **Every Edge setting is chosen here, not inherited** (the CEO's standing rule against
//!   third-party defaults): cosine distance, vectors and payload on disk (memory-mapped, so a
//!   large archive does not sit in the app's own heap), an 8 MiB write-ahead-log segment
//!   instead of Edge's 32 MiB default (one index, modest writes), two search threads so a query
//!   never takes the whole Mac, and points visible before optimization
//!   (`prevent_unoptimized: false`) so a partly built index still answers.
use qdrant_edge::{
    Distance, EdgeConfig, EdgeOptimizersConfig, EdgeShard, EdgeVectorParams, NamedQuery, PointId, PointInsertOperations,
    PointOperations, PointStruct, QueryEnum, SearchRequestBuilder, UpdateOperation, WalOptions, WithPayloadInterface,
    WithVector, DEFAULT_VECTOR_NAME,
};
use std::fs::File;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

pub const WAL_SEGMENT_BYTES: usize = 8 * 1024 * 1024;
pub const SEARCH_THREADS: usize = 2;

pub struct VectorIndex {
    shard: EdgeShard,
    dimension: usize,
    dir: PathBuf,
    // Held for the life of the index; dropping it releases the owner lock.
    _owner: File,
}

fn config(dimension: usize) -> EdgeConfig {
    EdgeConfig::builder()
        .on_disk_payload(true)
        .vector(DEFAULT_VECTOR_NAME, EdgeVectorParams::builder(dimension, Distance::Cosine).on_disk(true).build())
        .wal_options(WalOptions {
            segment_capacity: WAL_SEGMENT_BYTES,
            segment_queue_len: 0,
            retain_closed: NonZeroUsize::new(1).expect("one is not zero"),
        })
        .optimizers(EdgeOptimizersConfig { prevent_unoptimized: Some(false), ..EdgeOptimizersConfig::default() })
        .max_search_threads(SEARCH_THREADS)
        .build()
}

impl VectorIndex {
    /// Open (or create) the shard under `dir`, holding the folder's owner lock.
    pub fn open(dir: &Path, dimension: usize) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create the search index folder {}: {e}", dir.display()))?;
        let owner = File::create(dir.join("owner.lock")).map_err(|e| format!("cannot open the search index lock: {e}"))?;
        owner.try_lock().map_err(|_| "The search index is already open in another RichOS process.".to_string())?;
        let shard_dir = dir.join("shard");
        std::fs::create_dir_all(&shard_dir).map_err(|e| format!("cannot create the search index folder: {e}"))?;
        let shard = EdgeShard::load(&shard_dir, Some(config(dimension))).map_err(|e| format!("the search index could not be opened: {e}"))?;
        Ok(VectorIndex { shard, dimension, dir: dir.to_path_buf(), _owner: owner })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn upsert(&self, points: Vec<(u64, Vec<f32>)>) -> Result<(), String> {
        if points.is_empty() {
            return Ok(());
        }
        let mut list = Vec::with_capacity(points.len());
        for (id, vector) in points {
            if vector.len() != self.dimension {
                return Err(format!("a vector has {} numbers where the index holds {}", vector.len(), self.dimension));
            }
            list.push(PointStruct::new(id, vector, qdrant_edge::external::serde_json::json!({})).into());
        }
        self.shard
            .update(UpdateOperation::PointOperation(PointOperations::UpsertPoints(PointInsertOperations::PointsList(list))))
            .map_err(|e| format!("the search index refused a write: {e}"))
    }

    pub fn delete(&self, ids: &[u64]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let ids = ids.iter().map(|id| PointId::NumId(*id)).collect();
        self.shard
            .update(UpdateOperation::PointOperation(PointOperations::DeletePoints { ids }))
            .map_err(|e| format!("the search index refused a delete: {e}"))
    }

    /// Make every write so far durable. Called before a source is marked complete.
    pub fn flush(&self) -> Result<(), String> {
        self.shard.flush().map_err(|e| format!("the search index could not be saved: {e}"))
    }

    /// Edge's explicit optimization (it has no background optimizer), run once at the end of a
    /// backfill or Reindex.
    pub fn optimize(&self) -> Result<(), String> {
        self.shard.optimize().map(|_| ()).map_err(|e| format!("the search index could not be optimized: {e}"))
    }

    /// The `limit` nearest passages to `vector`, best first, as `(point id, cosine score)`.
    pub fn nearest(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<(u64, f32)>, String> {
        if vector.len() != self.dimension {
            return Err(format!("the query vector has {} numbers where the index holds {}", vector.len(), self.dimension));
        }
        let request = SearchRequestBuilder::new(QueryEnum::Nearest(NamedQuery { query: vector.into(), using: None }), limit)
            .with_payload(WithPayloadInterface::Bool(false))
            .with_vector(WithVector::Bool(false))
            .build();
        let points = self.shard.search(request).map_err(|e| format!("the search index could not be searched: {e}"))?;
        Ok(points
            .into_iter()
            .filter_map(|p| match p.id {
                PointId::NumId(id) => Some((id, p.score)),
                _ => None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: &[f32]) -> Vec<f32> {
        let mut v = v.to_vec();
        crate::embed::normalize(&mut v);
        v
    }

    #[test]
    fn write_find_delete_reopen_and_one_owner() {
        let dir = std::env::temp_dir().join(format!("richos-search-vector-{}", std::process::id()));
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("fixture cleanup: {error}");
            }
        }
        {
            let index = VectorIndex::open(&dir, 3).unwrap();
            assert!(VectorIndex::open(&dir, 3).is_err(), "a second owner is refused");
            index.upsert(vec![(1, unit(&[1.0, 0.0, 0.0])), (2, unit(&[0.0, 1.0, 0.0])), (3, unit(&[0.9, 0.1, 0.0]))]).unwrap();
            index.flush().unwrap();
            let hits = index.nearest(unit(&[1.0, 0.05, 0.0]), 2).unwrap();
            assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), vec![1, 3]);
            index.delete(&[1]).unwrap();
            index.flush().unwrap();
            assert!(index.upsert(vec![(9, vec![1.0, 0.0])]).is_err(), "a vector of the wrong size is refused");
        }
        let index = VectorIndex::open(&dir, 3).unwrap();
        let hits = index.nearest(unit(&[1.0, 0.05, 0.0]), 3).unwrap();
        assert_eq!(hits[0].0, 3, "the deleted point stayed deleted across a reopen");
        assert_eq!(hits.len(), 2);
        index.optimize().unwrap();
        assert_eq!(index.nearest(unit(&[0.0, 1.0, 0.0]), 1).unwrap()[0].0, 2);
        drop(index);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
