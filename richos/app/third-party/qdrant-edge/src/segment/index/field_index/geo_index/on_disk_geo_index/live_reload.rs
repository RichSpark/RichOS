use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::OnDiskGeoIndex;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;

impl<S: UniversalRead> LiveReload for OnDiskGeoIndex<S> {
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        _fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        _new_points: &SortedSlice<'_, PointOffsetType>,
        _hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        // No on-disk state changes on reload: this index is immutable, so only
        // the in-memory deletion bitvec is patched. `fs` / `new_points` are
        // unused because nothing is appended after build.
        for deleted_point in deleted_points {
            self.remove_point(*deleted_point);
        }

        Ok(())
    }
}
