use std::path::Path;

use crate::common::mmap::AdviceSetting;
use crate::common::universal_io::{CachedReadFs, Populate, UniversalRead, UniversalReadFs};

use super::ReadOnlyChunkedDenseVectorStorage;
use crate::segment::common::flags::in_memory_bitvec_flags::InMemoryBitvecFlags;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::data_types::primitive::PrimitiveVectorElement;
use crate::segment::types::Distance;
use crate::segment::vector_storage::chunked_vectors::read_only::ReadOnlyChunkedVectors;
use crate::segment::vector_storage::dense::appendable_dense_vector_storage::{
    DELETED_DIR_PATH, VECTORS_DIR_PATH,
};

impl<T: PrimitiveVectorElement, S: UniversalRead> ReadOnlyChunkedDenseVectorStorage<T, S> {
    /// Schedule background prefetch of the files [`Self::open`] will read.
    ///
    /// Absent files are skipped rather than reported: the subsequent open is
    /// the one to produce the error.
    pub fn preopen(
        fs: &impl CachedReadFs<File = S>,
        path: &Path,
        advice: AdviceSetting,
        populate: Populate,
    ) -> OperationResult<()> {
        // Vectors
        ReadOnlyChunkedVectors::<T, S>::preopen(
            fs,
            &path.join(VECTORS_DIR_PATH),
            advice,
            populate,
        )?;

        // Deleted flags
        InMemoryBitvecFlags::preopen(fs, &path.join(DELETED_DIR_PATH))?;

        Ok(())
    }

    /// Open the read-only counterpart of the appendable dense storage at `path`,
    /// threading every file open through `fs`; reads the existing layout but
    /// creates and writes nothing. `populate` warms the vector chunks.
    pub fn open(
        fs: &impl UniversalReadFs<File = S>,
        path: &Path,
        dim: usize,
        distance: Distance,
        advice: AdviceSetting,
        populate: Populate,
    ) -> OperationResult<Self> {
        let vectors =
            ReadOnlyChunkedVectors::open(fs, &path.join(VECTORS_DIR_PATH), dim, advice, populate)?;

        let deleted = InMemoryBitvecFlags::open::<S>(fs, &path.join(DELETED_DIR_PATH))?;

        Ok(Self {
            vectors,
            deleted,
            distance,
        })
    }
}
