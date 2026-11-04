use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::ReadOnlyGeoIndex;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;

impl<S: UniversalRead> LiveReload for ReadOnlyGeoIndex<S> {
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        new_points: &SortedSlice<'_, PointOffsetType>,
        hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        match self {
            ReadOnlyGeoIndex::Appendable(index) => {
                index.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            ReadOnlyGeoIndex::Immutable(index) => {
                index.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            ReadOnlyGeoIndex::OnDisk(index) => {
                index.live_reload(fs, deleted_points, new_points, hw_counter)
            }
        }
    }
}
