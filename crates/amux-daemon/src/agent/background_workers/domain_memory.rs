use serde::{Deserialize, Serialize};

use crate::agent::context::structural_memory::ThreadStructuralMemory;
use crate::agent::memory_palace::builder::build_memory_palace_update_batch;
use crate::agent::memory_palace::pruner::prune_memory_update_batch;
use crate::agent::memory_palace::types::{MemoryPalaceCluster, MemoryPalaceSnapshot};
use crate::agent::semantic_env::SemanticPackageSummary;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MemoryWorkerSnapshot {
    pub thread_id: Option<String>,
    pub task_id: Option<String>,
    pub update_batch: crate::agent::context::structural_memory::MemoryGraphUpdateBatch,
    pub pruned_edges: Vec<crate::agent::memory_palace::types::MemoryPalaceEdge>,
    pub clusters: Vec<MemoryPalaceCluster>,
    pub summary: String,
}

pub(crate) fn build_memory_snapshot(
    thread_id: Option<&str>,
    task_id: Option<&str>,
    structural_memory: Option<&ThreadStructuralMemory>,
    semantic_packages: &[SemanticPackageSummary],
    _now_ms: u64,
) -> MemoryWorkerSnapshot {
    let batch =
        build_memory_palace_update_batch(thread_id, task_id, structural_memory, semantic_packages);
    let (update_batch, pruned_edges, clusters) = prune_memory_update_batch(batch, 0.5);
    let snapshot = MemoryPalaceSnapshot {
        thread_id: thread_id.map(str::to_string),
        task_id: task_id.map(str::to_string),
        summary: format!(
            "memory snapshot with {} nodes, {} edges, {} clusters",
            update_batch.nodes.len(),
            update_batch.edges.len(),
            clusters.len()
        ),
        update_batch,
        pruned_edges,
        clusters,
    };
    MemoryWorkerSnapshot {
        thread_id: snapshot.thread_id,
        task_id: snapshot.task_id,
        update_batch: snapshot.update_batch,
        pruned_edges: snapshot.pruned_edges,
        clusters: snapshot.clusters,
        summary: snapshot.summary,
    }
}
