use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;

use super::ReadOnlyBoolIndex;
use crate::segment::common::operation_error::OperationResult;
use crate::segment::index::UniversalReadExt;
use crate::segment::index::field_index::LiveReload;

impl<S: UniversalReadExt> LiveReload for ReadOnlyBoolIndex<S> {
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        fs: &S::Fs,
        _deleted_points: &SortedSlice<'_, PointOffsetType>,
        _new_points: &SortedSlice<'_, PointOffsetType>,
        _hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        // Resync each flag set from its on-disk state; the point deltas are
        // irrelevant, the flag files are the source of truth.
        self.storage.trues_flags.live_reload(fs)?;
        self.storage.falses_flags.live_reload(fs)?;

        // Re-derive the counts from the just-resynced bitmaps, but only if they
        // were computed before: an untouched index must not pay for the bitmap
        // scan that deriving them would force.
        self.refresh_counts()?;

        Ok(())
    }
}
