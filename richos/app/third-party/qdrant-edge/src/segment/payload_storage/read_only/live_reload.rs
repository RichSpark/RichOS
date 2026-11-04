use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::ReadOnlyPayloadStorage;
use crate::segment::common::live_reload::LiveReload;
use crate::segment::common::operation_error::OperationResult;

impl<S: UniversalRead> LiveReload for ReadOnlyPayloadStorage<S> {
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        fs: &S::Fs,
        _deleted_points: &SortedSlice<'_, PointOffsetType>,
        _new_points: &SortedSlice<'_, PointOffsetType>,
        _hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        Ok(self.storage.live_reload(fs)?)
    }
}
