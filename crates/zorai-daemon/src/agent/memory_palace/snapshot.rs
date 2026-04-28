use anyhow::Result;

use crate::agent::background_workers::protocol::{
    BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult,
};
use crate::agent::background_workers::run_background_worker_command;
use crate::agent::engine::AgentEngine;
use crate::agent::semantic_env::scan_workspace_package_summaries_for_memory_graph;

use super::query::query_memory_graph;
use super::types::{MemoryPalaceCluster, MemoryPalaceGraphContext, MemoryPalaceSnapshot};

const MEMORY_PALACE_PROMPT_DEPTH: usize = 2;
const MEMORY_PALACE_PROMPT_PER_HOP_LIMIT: usize = 4;
const MEMORY_PALACE_PROMPT_NODE_LIMIT: usize = 5;
const MEMORY_PALACE_PROMPT_EDGE_LIMIT: usize = 4;

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
                self.apply_memory_palace_clusters(snapshot.clusters.clone())
                    .await;
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

    pub(crate) async fn build_memory_palace_prompt_context(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
    ) -> Option<String> {
        let center_node_id = self
            .select_memory_palace_center_node(thread_id, task_id)
            .await?;
        let context = self
            .memory_palace_query(
                &center_node_id,
                MEMORY_PALACE_PROMPT_DEPTH,
                MEMORY_PALACE_PROMPT_PER_HOP_LIMIT,
            )
            .await
            .ok()?;
        let rendered = render_memory_palace_prompt_context(&context);
        (!rendered.trim().is_empty()).then_some(rendered)
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

    async fn select_memory_palace_center_node(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
    ) -> Option<String> {
        let mut candidates = Vec::new();
        if let Some(task_id) = task_id {
            candidates.push(format!("node:task:{task_id}"));
        }
        candidates.push(format!("node:thread:{thread_id}"));

        if let Some(structural_memory) = self.get_thread_structural_memory(thread_id).await {
            candidates.extend(
                structural_memory
                    .observed_files
                    .into_iter()
                    .map(|node| node.node_id),
            );
            candidates.extend(
                structural_memory
                    .workspace_seeds
                    .into_iter()
                    .map(|seed| seed.node_id),
            );
        }

        for candidate in candidates {
            if self
                .history
                .get_memory_node(&candidate)
                .await
                .ok()
                .flatten()
                .is_some()
            {
                return Some(candidate);
            }
        }

        None
    }
}

fn render_memory_palace_prompt_context(context: &MemoryPalaceGraphContext) -> String {
    let mut lines = vec![
        "## Semantic Memory Palace".to_string(),
        format!("- Anchor node: `{}`", context.center_node_id),
    ];

    if !context.summary.trim().is_empty() {
        lines.push(format!("- Summary: {}", context.summary.trim()));
    }

    let related_nodes = context
        .subgraph_nodes
        .iter()
        .filter(|node| node.id != context.center_node_id)
        .take(MEMORY_PALACE_PROMPT_NODE_LIMIT)
        .map(|node| {
            let mut summary = format!("- {} `{}`", node.node_type, node.label);
            if let Some(text) = node
                .summary_text
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                summary.push_str(": ");
                summary.push_str(text.trim());
            }
            summary
        })
        .collect::<Vec<_>>();
    if !related_nodes.is_empty() {
        lines.push("- Related nodes:".to_string());
        lines.extend(related_nodes);
    }

    let related_edges = context
        .subgraph_edges
        .iter()
        .take(MEMORY_PALACE_PROMPT_EDGE_LIMIT)
        .map(|edge| {
            format!(
                "- `{}` -[{}]-> `{}` (weight {:.2})",
                edge.source_node_id, edge.relation, edge.target_node_id, edge.weight
            )
        })
        .collect::<Vec<_>>();
    if !related_edges.is_empty() {
        lines.push("- High-signal relations:".to_string());
        lines.extend(related_edges);
    }

    if !context.cluster_summaries.is_empty() {
        lines.push("- Cluster summaries:".to_string());
        lines.extend(
            context
                .cluster_summaries
                .iter()
                .take(3)
                .map(|summary| format!("- {}", summary.trim())),
        );
    }

    lines.join("\n")
}
