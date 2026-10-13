mod deferred;
mod facet;
mod formula_rescore;
mod info;
mod order_by;
mod payload;
mod sampling;
mod scroll;
mod search;
mod segment_ops;
mod vectors;

use std::collections::HashMap;

use crate::common::types::DeferredBehavior;

use crate::segment::id_tracker::{IdTrackerEnum, IdTrackerRead};
use crate::segment::index::PayloadIndexRead;
use crate::segment::index::field_index::FieldIndex;
use crate::segment::index::struct_payload_index::StructPayloadIndexReadView;
use crate::segment::payload_storage::PayloadStorageRead;
use crate::segment::payload_storage::payload_storage_enum::PayloadStorageEnum;
use crate::segment::segment::VectorData;
use crate::segment::segment::vector_data_read::VectorDataRead;
use crate::segment::types::{PointIdType, SegmentConfig, SeqNumberType, VectorNameBuf};
use crate::segment::vector_storage::VectorStorageEnum;

/// This structure serves as a generic representation of data
/// necessary for all read operations on a segment.
///
/// The motivation for this is to unify the read code between
/// regular `Segment` and `ReadOnlySegment`.
pub struct SegmentReadView<'s, TIdTracker, TPayloadIndex, TPayloadStorage, TVectorData>
where
    TIdTracker: IdTrackerRead,
    TPayloadIndex: PayloadIndexRead,
    TPayloadStorage: PayloadStorageRead,
    TVectorData: VectorDataRead,
{
    pub(crate) id_tracker: &'s TIdTracker,
    pub(crate) payload_index: &'s TPayloadIndex,
    pub(crate) payload_storage: &'s TPayloadStorage,
    pub(crate) vector_data: &'s HashMap<VectorNameBuf, TVectorData>,
    pub(crate) segment_config: &'s SegmentConfig,
    pub(crate) appendable_flag: bool,
}

/// Concrete `SegmentReadView` instantiation that wraps a regular [`Segment`].
///
/// [`Segment`]: crate::segment::Segment
pub type SegmentReadViewFor<'s> = SegmentReadView<
    's,
    IdTrackerEnum,
    StructPayloadIndexReadView<
        's,
        PayloadStorageEnum,
        IdTrackerEnum,
        VectorStorageEnum,
        FieldIndex,
    >,
    PayloadStorageEnum,
    VectorData,
>;

impl<'s, TIdT, TPI, TPS, TVD> SegmentReadView<'s, TIdT, TPI, TPS, TVD>
where
    TIdT: IdTrackerRead,
    TPI: PayloadIndexRead,
    TPS: PayloadStorageRead,
    TVD: VectorDataRead,
{
    pub fn point_version(&self, point_id: PointIdType) -> Option<SeqNumberType> {
        // Feeds version-dedup (paired with `point_is_deferred`), which must see
        // the latest version — including a deferred head that out-versions the
        // shadowed active.
        self.id_tracker
            .internal_id_with_behavior(point_id, DeferredBehavior::WithDeferred)
            .and_then(|internal_id| self.id_tracker.internal_version(internal_id))
    }

    pub fn read_range(
        &self,
        from: Option<PointIdType>,
        to: Option<PointIdType>,
    ) -> Vec<PointIdType> {
        let iterator = self
            .id_tracker
            .point_mappings()
            .iter_from(from)
            .map(|x| x.0);
        match to {
            None => iterator.collect(),
            Some(to_id) => iterator.take_while(|x| *x < to_id).collect(),
        }
    }
}
