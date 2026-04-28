use crate::agent::context::structural_memory::{MemoryGraphEdgeUpsert, MemoryGraphUpdateBatch};

use super::types::{MemoryPalaceCluster, MemoryPalaceEdge};

pub(crate) fn prune_memory_update_batch(
    batch: MemoryGraphUpdateBatch,
    min_weight: f64,
) -> (
    MemoryGraphUpdateBatch,
    Vec<MemoryPalaceEdge>,
    Vec<MemoryPalaceCluster>,
) {
    let mut pruned_edges = Vec::new();
    let mut clusters = std::collections::BTreeMap::<String, MemoryPalaceCluster>::new();
    let kept_edges = batch
        .edges
        .into_iter()
        .filter(|edge| {
            let keep = edge.weight >= min_weight;
            if !keep {
                pruned_edges.push(MemoryPalaceEdge {
                    source_node_id: edge.source_node_id.clone(),
                    target_node_id: edge.target_node_id.clone(),
                    relation: edge.relation_type.clone(),
                    weight: edge.weight,
                });
                let cluster_name = format!("cluster:{}", edge.source_node_id);
                let cluster =
                    clusters
                        .entry(cluster_name.clone())
                        .or_insert_with(|| MemoryPalaceCluster {
                            name: cluster_name,
                            summary_text: format!(
                                "summarized low-signal relations fanout from {}",
                                edge.source_node_id
                            ),
                            center_node_id: Some(edge.source_node_id.clone()),
                            member_node_ids: vec![edge.source_node_id.clone()],
                        });
                if !cluster.member_node_ids.contains(&edge.target_node_id) {
                    cluster.member_node_ids.push(edge.target_node_id.clone());
                }
            }
            keep
        })
        .collect::<Vec<MemoryGraphEdgeUpsert>>();

    (
        MemoryGraphUpdateBatch {
            nodes: batch.nodes,
            edges: kept_edges,
        },
        pruned_edges,
        clusters.into_values().collect(),
    )
}
