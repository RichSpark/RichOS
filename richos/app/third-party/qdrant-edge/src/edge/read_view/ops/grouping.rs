use crate::segment::common::operation_error::OperationResult;
pub use crate::shard::grouping::Group;
use crate::shard::grouping::{GroupByDriver, RequestBudget};
use crate::shard::query::{self, ShardQueryRequest};

use crate::edge::read_view::{EdgeReadView, ReadSegmentHandle};
use crate::edge::requests::GroupRequest;

impl<H: ReadSegmentHandle> EdgeReadView<H> {
    pub(crate) fn query_groups(&self, request: GroupRequest) -> OperationResult<Vec<Group>> {
        let GroupRequest {
            query,
            group_by,
            groups,
            group_size,
        } = request;
        let query = ShardQueryRequest::from(query);

        let order = query::query_result_order(query.query.as_ref(), |vector_name| {
            self.config.get_distance(vector_name)
        })?;

        let mut driver = GroupByDriver::new(
            query,
            group_by,
            groups,
            group_size,
            order,
            RequestBudget::default(),
        );

        while let Some(request) = driver.next_request() {
            let points = self.query(request)?;
            driver.add_points(&points);
        }

        Ok(driver.distill())
    }
}

#[cfg(test)]
mod tests {
    use crate::segment::data_types::groups::GroupId;
    use crate::segment::data_types::vectors::{NamedQuery, VectorInternal};
    use crate::shard::query::ScoringQuery;
    use crate::shard::query::query_enum::QueryEnum;

    use super::*;
    use crate::edge::test_helpers::{
        VECTOR_NAME, point_with_group, point_with_group_values, test_config, upsert,
    };
    use crate::edge::{EdgeShardRead, QueryRequest, QueryRequestBuilder};

    fn base_query() -> QueryRequest {
        QueryRequestBuilder::new(0)
            .query(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery::new(
                VectorInternal::from(vec![1.0]),
                VECTOR_NAME.to_string(),
            ))))
            .build()
    }

    #[test]
    fn groups_by_payload_field() {
        let dir = tempfile::tempdir().unwrap();
        let shard = crate::edge::EdgeShard::new(dir.path(), test_config()).unwrap();
        upsert(
            &shard,
            vec![
                point_with_group(1, "a"),
                point_with_group(2, "b"),
                point_with_group(3, "a"),
                point_with_group(4, "b"),
            ],
        );

        let groups = shard
            .query_groups(GroupRequest::new(
                base_query(),
                "group".parse().unwrap(),
                2,
                5,
            ))
            .unwrap();

        assert_eq!(groups.len(), 2);
        // Test vectors score by dot product with [1.0], so the group holding the
        // highest-id point comes first.
        assert_eq!(groups[0].key, GroupId::from("b"));
        assert_eq!(groups[1].key, GroupId::from("a"));
        for g in &groups {
            assert_eq!(g.hits.len(), 2);
        }
    }

    #[test]
    fn groups_by_multi_valued_payload_field() {
        let dir = tempfile::tempdir().unwrap();
        let shard = crate::edge::EdgeShard::new(dir.path(), test_config()).unwrap();
        upsert(
            &shard,
            vec![
                point_with_group_values(1, serde_json::json!(["a", "b"])),
                point_with_group(2, "a"),
                point_with_group(3, "b"),
            ],
        );

        let groups = shard
            .query_groups(GroupRequest::new(
                base_query(),
                "group".parse().unwrap(),
                2,
                2,
            ))
            .unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, GroupId::from("b"));
        assert_eq!(groups[1].key, GroupId::from("a"));
        // Point 1 carries both group values, so it must land in both groups.
        for g in &groups {
            assert_eq!(g.hits.len(), 2);
            assert!(g.hits.iter().any(|hit| hit.id == 1.into()));
        }
    }
}
