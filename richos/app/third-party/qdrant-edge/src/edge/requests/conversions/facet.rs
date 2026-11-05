use crate::shard::facet::FacetRequestInternal;

use crate::edge::requests::facet::FacetRequest;

impl From<FacetRequest> for FacetRequestInternal {
    fn from(request: FacetRequest) -> Self {
        let FacetRequest {
            key,
            limit,
            filter,
            exact,
        } = request;
        FacetRequestInternal {
            key,
            limit,
            filter,
            exact,
        }
    }
}
