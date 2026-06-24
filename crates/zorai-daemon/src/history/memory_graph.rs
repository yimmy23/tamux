use anyhow::Result;

use super::db;
use super::HistoryStore;

fn map_memory_node_row_at(row: &db::Row, base: usize) -> anyhow::Result<MemoryNodeRow> {
    Ok(MemoryNodeRow {
        id: row.get(base)?,
        label: row.get(base + 1)?,
        node_type: row.get(base + 2)?,
        embedding_blob: row.get(base + 3)?,
        created_at_ms: row.get::<i64>(base + 4)?.max(0) as u64,
        last_accessed_ms: row.get::<i64>(base + 5)?.max(0) as u64,
        access_count: row.get::<i64>(base + 6)?.max(0) as u64,
        summary_text: row.get(base + 7)?,
    })
}

fn map_memory_edge_row_at(row: &db::Row, base: usize) -> anyhow::Result<MemoryEdgeRow> {
    Ok(MemoryEdgeRow {
        id: row.get(base)?,
        source_node_id: row.get(base + 1)?,
        target_node_id: row.get(base + 2)?,
        relation_type: row.get(base + 3)?,
        weight: row.get(base + 4)?,
        last_updated_ms: row.get::<i64>(base + 5)?.max(0) as u64,
    })
}

/// Shared insert shape for `memory_nodes` rows. The conflict policy differs per
/// caller (upsert overwrites, stubs preserve existing rows), but the column
/// list and value bindings must stay identical — append the ON CONFLICT clause.
/// Params: ?1 id, ?2 label, ?3 node_type, ?4 created/accessed ms, ?5 summary.
const MEMORY_NODE_INSERT: &str = "INSERT INTO memory_nodes (
    id,
    label,
    node_type,
    embedding_blob,
    created_at_ms,
    last_accessed_ms,
    access_count,
    summary_text
) VALUES (?1, ?2, ?3, NULL, ?4, ?4, 1, ?5)";

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryNodeRow {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub embedding_blob: Option<Vec<u8>>,
    pub created_at_ms: u64,
    pub last_accessed_ms: u64,
    pub access_count: u64,
    pub summary_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEdgeRow {
    pub id: i64,
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub weight: f64,
    pub last_updated_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryGraphNeighborRow {
    pub node: MemoryNodeRow,
    pub via_edge: MemoryEdgeRow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryGraphClusterRow {
    pub id: i64,
    pub name: String,
    pub summary_text: Option<String>,
    pub center_node_id: Option<String>,
    pub created_at_ms: u64,
    pub member_node_ids: Vec<String>,
}

impl HistoryStore {
    pub async fn upsert_memory_node(
        &self,
        id: &str,
        label: &str,
        node_type: &str,
        summary_text: Option<&str>,
        accessed_at_ms: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                &format!(
                    "{MEMORY_NODE_INSERT}
                        ON CONFLICT(id) DO UPDATE SET
                            label = excluded.label,
                            node_type = excluded.node_type,
                            last_accessed_ms = MAX(memory_nodes.last_accessed_ms, excluded.last_accessed_ms),
                            access_count = memory_nodes.access_count + 1,
                            summary_text = COALESCE(excluded.summary_text, memory_nodes.summary_text)"
                ),
                db::db_params![
                    id,
                    label,
                    node_type,
                    accessed_at_ms as i64,
                    summary_text,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_memory_node(&self, id: &str) -> Result<Option<MemoryNodeRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT id, label, node_type, embedding_blob, created_at_ms, last_accessed_ms, access_count, summary_text
                     FROM memory_nodes
                     WHERE id = ?1",
                db::db_params![id],
            )
            .await?;
        row.map(|row| map_memory_node_row_at(&row, 0)).transpose()
    }

    pub async fn upsert_memory_edge(
        &self,
        source_node_id: &str,
        target_node_id: &str,
        relation_type: &str,
        weight: f64,
        updated_at_ms: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO memory_edges (
                        source_node_id,
                        target_node_id,
                        relation_type,
                        weight,
                        last_updated_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(source_node_id, target_node_id, relation_type) DO UPDATE SET
                        weight = memory_edges.weight + excluded.weight,
                        last_updated_ms = MAX(memory_edges.last_updated_ms, excluded.last_updated_ms)",
                db::db_params![
                    source_node_id,
                    target_node_id,
                    relation_type,
                    weight,
                    updated_at_ms as i64,
                ],
            )
            .await?;
        Ok(())
    }

    /// Record a `delegated_to` edge from a parent task to a subagent child task,
    /// making cross-agent delegation queryable in the shared knowledge graph.
    ///
    /// Node stubs use `INSERT ... ON CONFLICT DO NOTHING` so they never clobber a
    /// richer label/summary already written by the task's own graph record (the
    /// `memory_nodes` upsert path overwrites labels unconditionally). Both nodes
    /// are ensured before the edge because `foreign_keys` is enabled.
    pub async fn record_task_delegation_edge(
        &self,
        parent_task_id: &str,
        child_task_id: &str,
        child_label: &str,
        updated_at_ms: u64,
    ) -> Result<()> {
        let parent_node_id = format!("node:task:{parent_task_id}");
        let child_node_id = format!("node:task:{child_task_id}");
        let parent_label = parent_task_id.to_string();
        let child_label = child_label.to_string();

        let ensure_sql = format!("{MEMORY_NODE_INSERT}\nON CONFLICT(id) DO NOTHING");
        self.conn_db
            .execute(
                &ensure_sql,
                db::db_params![
                    parent_node_id.clone(),
                    parent_label,
                    "task",
                    updated_at_ms as i64,
                    Option::<String>::None,
                ],
            )
            .await?;
        self.conn_db
            .execute(
                &ensure_sql,
                db::db_params![
                    child_node_id.clone(),
                    child_label,
                    "task",
                    updated_at_ms as i64,
                    Option::<String>::None,
                ],
            )
            .await?;
        self.conn_db
            .execute(
                "INSERT INTO memory_edges (
                        source_node_id, target_node_id, relation_type, weight, last_updated_ms
                    ) VALUES (?1, ?2, 'delegated_to', 1.0, ?3)
                    ON CONFLICT(source_node_id, target_node_id, relation_type) DO UPDATE SET
                        weight = memory_edges.weight + excluded.weight,
                        last_updated_ms = MAX(memory_edges.last_updated_ms, excluded.last_updated_ms)",
                db::db_params![parent_node_id, child_node_id, updated_at_ms as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_memory_edges_for_node(&self, node_id: &str) -> Result<Vec<MemoryEdgeRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, source_node_id, target_node_id, relation_type, weight, last_updated_ms
                     FROM memory_edges
                     WHERE source_node_id = ?1 OR target_node_id = ?1
                     ORDER BY relation_type ASC, source_node_id ASC, target_node_id ASC",
                db::db_params![node_id],
            )
            .await?;
        rows.iter().map(|row| map_memory_edge_row_at(row, 0)).collect()
    }

    pub async fn list_memory_graph_neighbors(
        &self,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryGraphNeighborRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT
                        n.id,
                        n.label,
                        n.node_type,
                        n.embedding_blob,
                        n.created_at_ms,
                        n.last_accessed_ms,
                        n.access_count,
                        n.summary_text,
                        e.id,
                        e.source_node_id,
                        e.target_node_id,
                        e.relation_type,
                        e.weight,
                        e.last_updated_ms
                     FROM memory_edges e
                     JOIN memory_nodes n
                       ON n.id = CASE
                           WHEN e.source_node_id = ?1 THEN e.target_node_id
                           ELSE e.source_node_id
                       END
                     WHERE e.source_node_id = ?1 OR e.target_node_id = ?1
                     ORDER BY e.weight DESC, e.last_updated_ms DESC, n.last_accessed_ms DESC
                     LIMIT ?2",
                db::db_params![node_id, limit],
            )
            .await?;
        rows.iter()
            .map(|row| -> anyhow::Result<MemoryGraphNeighborRow> {
                Ok(MemoryGraphNeighborRow {
                    node: map_memory_node_row_at(row, 0)?,
                    via_edge: map_memory_edge_row_at(row, 8)?,
                })
            })
            .collect()
    }

    pub async fn upsert_memory_cluster(
        &self,
        name: &str,
        summary_text: &str,
        center_node_id: Option<&str>,
        member_node_ids: &[String],
        created_at_ms: u64,
    ) -> Result<()> {
        let member_node_ids = member_node_ids.to_vec();
        self.conn_db
            .execute(
                "INSERT INTO memory_graph_clusters (name, summary_text, center_node_id, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(name) DO UPDATE SET
                        summary_text = excluded.summary_text,
                        center_node_id = excluded.center_node_id",
                db::db_params![name, summary_text, center_node_id, created_at_ms as i64],
            )
            .await?;
        let cluster_id: i64 = self
            .conn_db
            .query_opt(
                "SELECT id FROM memory_graph_clusters WHERE name = ?1",
                db::db_params![name],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("memory cluster missing after upsert"))?
            .get::<i64>(0)?;
        self.conn_db
            .execute(
                "UPDATE memory_cluster_members SET deleted_at = ?2 WHERE cluster_id = ?1 AND deleted_at IS NULL",
                db::db_params![cluster_id, crate::history::now_ts() as i64],
            )
            .await?;
        for node_id in member_node_ids {
            self.conn_db
                .execute(
                    "INSERT OR REPLACE INTO memory_cluster_members (cluster_id, node_id, deleted_at) VALUES (?1, ?2, NULL)",
                    db::db_params![cluster_id, node_id],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn list_memory_clusters_for_node(
        &self,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryGraphClusterRow>> {
        let limit = limit.max(1) as i64;
        let cluster_rows = self
            .read_db
            .query(
                "SELECT c.id, c.name, c.summary_text, c.center_node_id, c.created_at_ms
                     FROM memory_cluster_members m
                     JOIN memory_graph_clusters c ON c.id = m.cluster_id
                     WHERE m.node_id = ?1 AND m.deleted_at IS NULL
                     ORDER BY c.created_at_ms DESC, c.id DESC
                     LIMIT ?2",
                db::db_params![node_id, limit],
            )
            .await?;
        let bases: Vec<(i64, String, Option<String>, Option<String>, u64)> = cluster_rows
            .iter()
            .map(|row| -> anyhow::Result<_> {
                Ok((
                    row.get::<i64>(0)?,
                    row.get::<String>(1)?,
                    row.get::<Option<String>>(2)?,
                    row.get::<Option<String>>(3)?,
                    row.get::<i64>(4)?.max(0) as u64,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut clusters = Vec::new();
        for (id, name, summary_text, center_node_id, created_at_ms) in bases {
            let member_rows = self
                .read_db
                .query(
                    "SELECT node_id FROM memory_cluster_members WHERE cluster_id = ?1 AND deleted_at IS NULL ORDER BY node_id ASC",
                    db::db_params![id],
                )
                .await?;
            let member_node_ids = member_rows
                .iter()
                .map(|row| row.get::<String>(0))
                .collect::<anyhow::Result<Vec<_>>>()?;
            clusters.push(MemoryGraphClusterRow {
                id,
                name,
                summary_text,
                center_node_id,
                created_at_ms,
                member_node_ids,
            });
        }
        Ok(clusters)
    }

    /// Batched cluster-summary lookup for the BFS in `query_memory_graph`.
    /// The caller iterates a set of visited nodes and only consumes
    /// `cluster.summary_text` — it doesn't need the full member list. Per-
    /// node `list_memory_clusters_for_node` was an N+1 (one query per
    /// visited node, plus an inner per-cluster member-list query that was
    /// dead code for this caller). This single query returns up to
    /// `limit_per_node` summary strings per node, deduped across the
    /// requested set.
    pub async fn list_memory_cluster_summaries_for_nodes(
        &self,
        node_ids: &[String],
        limit_per_node: usize,
    ) -> Result<Vec<String>> {
        if node_ids.is_empty() || limit_per_node == 0 {
            return Ok(Vec::new());
        }
        let node_ids = node_ids.to_vec();
        let limit_per_node = limit_per_node as i64;
        let placeholders = std::iter::repeat("?")
            .take(node_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "WITH ranked AS (
                         SELECT
                             m.node_id,
                             c.summary_text,
                             ROW_NUMBER() OVER (
                                 PARTITION BY m.node_id
                                 ORDER BY c.created_at_ms DESC, c.id DESC
                             ) AS rn
                         FROM memory_cluster_members m
                         JOIN memory_graph_clusters c ON c.id = m.cluster_id
                         WHERE m.node_id IN ({placeholders})
                           AND m.deleted_at IS NULL
                     )
                     SELECT summary_text
                     FROM ranked
                     WHERE rn <= ?{limit_param}
                       AND summary_text IS NOT NULL
                       AND length(summary_text) > 0",
            limit_param = node_ids.len() + 1
        );
        let mut values: Vec<db::Value> = node_ids.into_iter().map(db::Value::Text).collect();
        values.push(db::Value::Integer(limit_per_node));
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        let mut seen = std::collections::HashSet::new();
        let mut deduped: Vec<String> = Vec::new();
        for row in &rows {
            if let Some(summary) = row.get::<Option<String>>(0)? {
                if seen.insert(summary.clone()) {
                    deduped.push(summary);
                }
            }
        }
        Ok(deduped)
    }
}
