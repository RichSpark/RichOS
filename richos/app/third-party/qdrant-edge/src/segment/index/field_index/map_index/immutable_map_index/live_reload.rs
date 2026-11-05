use crate::blobstore::Blob;
use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::persisted_hashmap::Key;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::ImmutableMapIndex;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::field_index::LiveReload;
use crate::segment::index::field_index::map_index::MapIndexKey;

impl<N, S> LiveReload for ImmutableMapIndex<N, S>
where
    Vec<<N as MapIndexKey>::Owned>: Blob + Send + Sync,
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
        for deleted_point in deleted_points {
            self.remove_point(*deleted_point)?;
        }

        Ok(())
    }
}
