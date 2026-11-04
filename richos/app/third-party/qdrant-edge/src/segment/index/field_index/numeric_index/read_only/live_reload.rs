use crate::blobstore::Blob;
use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::{Encodable, ReadOnlyNumericIndex};
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;
use crate::segment::index::field_index::numeric_point::Numericable;
use crate::segment::index::field_index::on_disk_point_to_values::StoredValue;

impl<
    T: Encodable + Numericable + StoredValue + Send + Sync + Default + 'static,
    P,
    S: UniversalRead,
> LiveReload for ReadOnlyNumericIndex<T, P, S>
where
    Vec<T>: Blob,
{
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        new_points: &SortedSlice<'_, PointOffsetType>,
        hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        self.inner
            .live_reload(fs, deleted_points, new_points, hw_counter)
    }
}
