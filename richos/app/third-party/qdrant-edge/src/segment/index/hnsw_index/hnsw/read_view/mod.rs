mod dispatch;
mod search;

use super::telemetry::HNSWSearchesTelemetry;
use crate::segment::id_tracker::{IdTrackerEnum, IdTrackerRead};
use crate::segment::index::PayloadIndexRead;
use crate::segment::index::field_index::FieldIndex;
use crate::segment::index::hnsw_index::config::HnswGraphConfig;
use crate::segment::index::hnsw_index::graph_layers::GraphLayers;
use crate::segment::index::struct_payload_index::StructPayloadIndexReadView;
use crate::segment::payload_storage::payload_storage_enum::PayloadStorageEnum;
use crate::segment::vector_storage::quantized::quantized_vectors::{QuantizedVectors, QuantizedVectorsRead};
use crate::segment::vector_storage::{VectorStorageEnum, VectorStorageRead};

/// Read-only view over an HNSW index.
///
/// The top-level id tracker / vector storage / quantized vectors are decoupled
/// from the payload index view (`P`), so read-only backends can be combined with
/// a payload index view built over different (e.g. still-mutable) backends.
pub struct HNSWIndexReadView<
    'a,
    I: IdTrackerRead,
    V: VectorStorageRead,
    Q: QuantizedVectorsRead,
    P: PayloadIndexRead,
> {
    pub(crate) id_tracker: &'a I,
    pub(crate) vector_storage: &'a V,
    pub(crate) quantized_vectors: Option<&'a Q>,
    pub(crate) payload_index: P,
    pub(crate) config: &'a HnswGraphConfig,
    pub(crate) graph: &'a GraphLayers,
    pub(crate) searches_telemetry: &'a HNSWSearchesTelemetry,
}

/// Read-only view over an [`HNSWIndex`] backed by the in-memory
/// [`IdTrackerEnum`] / [`VectorStorageEnum`] / [`PayloadStorageEnum`] enums.
///
/// [`HNSWIndex`]: super::super::HNSWIndex
pub(crate) type HNSWIndexReadViewEnum<'a> = HNSWIndexReadView<
    'a,
    IdTrackerEnum,
    VectorStorageEnum,
    QuantizedVectors,
    StructPayloadIndexReadView<
        'a,
        PayloadStorageEnum,
        IdTrackerEnum,
        VectorStorageEnum,
        FieldIndex,
    >,
>;
