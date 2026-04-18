use serde::{Deserialize, Serialize};

use crate::agent::context::structural_memory::MemoryGraphUpdateBatch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MemoryPalaceNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub summary_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MemoryPalaceEdge {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation: String,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MemoryPalaceGraphContext {
    pub center_node_id: String,
    pub subgraph_nodes: Vec<MemoryPalaceNode>,
    pub subgraph_edges: Vec<MemoryPalaceEdge>,
    pub cluster_summaries: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MemoryPalaceCluster {
    pub name: String,
    pub summary_text: String,
    pub center_node_id: Option<String>,
    pub member_node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MemoryPalaceSnapshot {
    pub thread_id: Option<String>,
    pub task_id: Option<String>,
    pub update_batch: MemoryGraphUpdateBatch,
    pub pruned_edges: Vec<MemoryPalaceEdge>,
    pub clusters: Vec<MemoryPalaceCluster>,
    pub summary: String,
}
