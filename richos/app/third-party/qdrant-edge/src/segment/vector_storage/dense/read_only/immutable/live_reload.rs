use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::ReadOnlyImmutableDenseVectorStorage;
use crate::segment::common::live_reload::LiveReload;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::data_types::primitive::PrimitiveVectorElement;

impl<T: PrimitiveVectorElement, S: UniversalRead> LiveReload
    for ReadOnlyImmutableDenseVectorStorage<T, S>
{
    type Fs = S::Fs;

    /// Vector data is immutable, so only the in-memory deletion flags are patched
    /// from the authoritative `deleted_points`; `fs` and `new_points` are unused.
    fn live_reload(
        &mut self,
        _fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        _new_points: &SortedSlice<'_, PointOffsetType>,
        _hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        self.deleted.insert_all(deleted_points);

        Ok(())
    }
}
