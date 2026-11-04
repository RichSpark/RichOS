use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::persisted_hashmap::Key;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;
use crate::segment::index::field_index::map_index::MapIndexKey;
use crate::segment::index::field_index::map_index::on_disk_map_index::OnDiskMapIndex;

impl<N, S> LiveReload for OnDiskMapIndex<N, S>
where
    N: MapIndexKey + Key + ?Sized,
    S: UniversalRead,
{
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        _fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        _new_points: &SortedSlice<'_, PointOffsetType>,
        _hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        // No on-disk state is changing when we live-reload, as
        // this UniversalMapIndex is not mutable.
        // We only patch in-memory deleted bitslice representation.
        for deleted_point in deleted_points {
            self.remove_point(*deleted_point)
        }

        Ok(())
    }
}
