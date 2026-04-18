use anyhow::Result;

use crate::agent::background_workers::protocol::{
    BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult,
};
use crate::agent::background_workers::run_background_worker_command;
use crate::agent::engine::AgentEngine;
use crate::agent::semantic_env::scan_workspace_package_summaries_for_memory_graph;

use super::query::query_memory_graph;
use super::types::{MemoryPalaceCluster, MemoryPalaceGraphContext, MemoryPalaceSnapshot};

impl AgentEngine {
    pub(crate) async fn refresh_memory_palace_from_thread(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
    ) -> Result<MemoryPalaceSnapshot> {
        let structural_memory = self.get_thread_structural_memory(thread_id).await;
        let repo_root = self
            .resolve_thread_repo_root(thread_id)
            .await
            .map(|(root, _, _, _)| root)
            .or_else(|| {
                self.workspace_root
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
            });
        let semantic_packages = repo_root
            .as_deref()
            .map(std::path::Path::new)
            .and_then(|root| scan_workspace_package_summaries_for_memory_graph(root).ok())
            .unwrap_or_default();

        let result = run_background_worker_command(
            BackgroundWorkerKind::Memory,
            BackgroundWorkerCommand::TickMemory {
                thread_id: Some(thread_id.to_string()),
                task_id: task_id.map(str::to_string),
                structural_memory,
                semantic_packages,
                now_ms: crate::agent::now_millis(),
            },
        )
        .await?;

        match result {
            BackgroundWorkerResult::MemoryTick { snapshot } => {
                self.apply_memory_graph_updates(snapshot.update_batch.clone())
                    .await;
                self.apply_memory_palace_clusters(snapshot.clusters.clone()).await;
                Ok(MemoryPalaceSnapshot {
                    thread_id: snapshot.thread_id,
                    task_id: snapshot.task_id,
                    update_batch: snapshot.update_batch,
                    pruned_edges: snapshot.pruned_edges,
                    clusters: snapshot.clusters,
                    summary: snapshot.summary,
                })
            }
            BackgroundWorkerResult::Error { message } => {
                anyhow::bail!("memory worker returned error: {message}");
            }
            other => anyhow::bail!("memory worker returned unexpected response: {other:?}"),
        }
    }

    pub(crate) async fn memory_palace_query(
        &self,
        center_node_id: &str,
        depth: usize,
        per_hop_limit: usize,
    ) -> Result<MemoryPalaceGraphContext> {
        query_memory_graph(&self.history, center_node_id, depth, per_hop_limit).await
    }

    async fn apply_memory_palace_clusters(&self, clusters: Vec<MemoryPalaceCluster>) {
        if clusters.is_empty() {
            return;
        }
        let now = crate::agent::now_millis();
        for cluster in clusters {
            if let Err(error) = self
                .history
                .upsert_memory_cluster(
                    &cluster.name,
                    &cluster.summary_text,
                    cluster.center_node_id.as_deref(),
                    &cluster.member_node_ids,
                    now,
                )
                .await
            {
                tracing::warn!(cluster = %cluster.name, %error, "failed to persist memory palace cluster");
            }
        }
    }
}
