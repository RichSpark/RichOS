use crate::common::counter::hardware_counter::HardwareCounterCell;
use crate::common::sorted_slice::SortedSlice;
use crate::common::types::PointOffsetType;
use crate::common::universal_io::UniversalRead;

use super::VectorStorageReadEnum;
use crate::segment::common::live_reload::LiveReload;
use crate::segment::common::operation_error::OperationResult;

impl<S: UniversalRead> LiveReload for VectorStorageReadEnum<S> {
    type Fs = S::Fs;

    fn live_reload(
        &mut self,
        fs: &S::Fs,
        deleted_points: &SortedSlice<'_, PointOffsetType>,
        new_points: &SortedSlice<'_, PointOffsetType>,
        hw_counter: &HardwareCounterCell,
    ) -> OperationResult<()> {
        match self {
            VectorStorageReadEnum::Dense(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseByte(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseHalf(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseChunked(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseChunkedByte(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseChunkedHalf(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::MultiDenseChunked(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::MultiDenseChunkedByte(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::MultiDenseChunkedHalf(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseTurbo(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::DenseTurboChunked(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::MultiDenseTurbo(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
            VectorStorageReadEnum::Sparse(s) => {
                s.live_reload(fs, deleted_points, new_points, hw_counter)
            }
        }
    }
}
