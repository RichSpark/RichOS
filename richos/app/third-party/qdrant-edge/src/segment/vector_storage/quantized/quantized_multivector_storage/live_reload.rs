use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::MultivectorOffsetsStorageChunkedRead;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;

impl<S: UniversalRead> LiveReload for MultivectorOffsetsStorageChunkedRead<S> {
    type Fs = <S as UniversalRead>::Fs;

    /// Pick up offsets a writer appended to the chunked backing.
    fn live_reload(
        &mut self,
        fs: &Self::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        new_points: &SortedSlice<'_, PointOffsetType>,
        hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        self.data
            .live_reload(fs, deleted_points, new_points, hw_counter)
    }
}
