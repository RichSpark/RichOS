use crate::blobstore::Blob;
use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::ImmutableNumericIndex;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;
use crate::segment::index::field_index::numeric_index::Encodable;
use crate::segment::index::field_index::numeric_point::Numericable;
use crate::segment::index::field_index::on_disk_point_to_values::StoredValue;

impl<T: Encodable + Numericable + StoredValue + Send + Sync + Default, S: UniversalRead> LiveReload
    for ImmutableNumericIndex<T, S>
where
    Vec<T>: Blob,
{
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        _fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        _new_points: &SortedSlice<'_, PointOffsetType>,
        _hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        for deleted_point in deleted_points {
            self.remove_point(*deleted_point);
        }

        Ok(())
    }
}
