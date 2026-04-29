use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::HistoryStore;

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
        let id = id.to_string();
        let label = label.to_string();
        let node_type = node_type.to_string();
        let summary_text = summary_text.map(str::to_string);

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memory_nodes (
                        id,
                        label,
                        node_type,
                        embedding_blob,
                        created_at_ms,
                        last_accessed_ms,
                        access_count,
                        summary_text
                    ) VALUES (?1, ?2, ?3, NULL, ?4, ?4, 1, ?5)
                    ON CONFLICT(id) DO UPDATE SET
                        label = excluded.label,
                        node_type = excluded.node_type,
                        last_accessed_ms = MAX(memory_nodes.last_accessed_ms, excluded.last_accessed_ms),
                        access_count = memory_nodes.access_count + 1,
                        summary_text = COALESCE(excluded.summary_text, memory_nodes.summary_text)",
                    params![
                        id,
                        label,
                        node_type,
                        accessed_at_ms as i64,
                        summary_text,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_memory_node(&self, id: &str) -> Result<Option<MemoryNodeRow>> {
        let id = id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, label, node_type, embedding_blob, created_at_ms, last_accessed_ms, access_count, summary_text
                     FROM memory_nodes
                     WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok(MemoryNodeRow {
                            id: row.get(0)?,
                            label: row.get(1)?,
                            node_type: row.get(2)?,
                            embedding_blob: row.get(3)?,
                            created_at_ms: row.get::<_, i64>(4)?.max(0) as u64,
                            last_accessed_ms: row.get::<_, i64>(5)?.max(0) as u64,
                            access_count: row.get::<_, i64>(6)?.max(0) as u64,
                            summary_text: row.get(7)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_memory_edge(
        &self,
        source_node_id: &str,
        target_node_id: &str,
        relation_type: &str,
        weight: f64,
        updated_at_ms: u64,
    ) -> Result<()> {
        let source_node_id = source_node_id.to_string();
        let target_node_id = target_node_id.to_string();
        let relation_type = relation_type.to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
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
                    params![
                        source_node_id,
                        target_node_id,
                        relation_type,
                        weight,
                        updated_at_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_edges_for_node(&self, node_id: &str) -> Result<Vec<MemoryEdgeRow>> {
        let node_id = node_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, source_node_id, target_node_id, relation_type, weight, last_updated_ms
                     FROM memory_edges
                     WHERE source_node_id = ?1 OR target_node_id = ?1
                     ORDER BY relation_type ASC, source_node_id ASC, target_node_id ASC",
                )?;
                let rows = stmt.query_map(params![node_id], |row| {
                    Ok(MemoryEdgeRow {
                        id: row.get(0)?,
                        source_node_id: row.get(1)?,
                        target_node_id: row.get(2)?,
                        relation_type: row.get(3)?,
                        weight: row.get(4)?,
                        last_updated_ms: row.get::<_, i64>(5)?.max(0) as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_graph_neighbors(
        &self,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryGraphNeighborRow>> {
        let node_id = node_id.to_string();
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
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
                )?;
                let rows = stmt.query_map(params![node_id, limit], |row| {
                    Ok(MemoryGraphNeighborRow {
                        node: MemoryNodeRow {
                            id: row.get(0)?,
                            label: row.get(1)?,
                            node_type: row.get(2)?,
                            embedding_blob: row.get(3)?,
                            created_at_ms: row.get::<_, i64>(4)?.max(0) as u64,
                            last_accessed_ms: row.get::<_, i64>(5)?.max(0) as u64,
                            access_count: row.get::<_, i64>(6)?.max(0) as u64,
                            summary_text: row.get(7)?,
                        },
                        via_edge: MemoryEdgeRow {
                            id: row.get(8)?,
                            source_node_id: row.get(9)?,
                            target_node_id: row.get(10)?,
                            relation_type: row.get(11)?,
                            weight: row.get(12)?,
                            last_updated_ms: row.get::<_, i64>(13)?.max(0) as u64,
                        },
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_memory_cluster(
        &self,
        name: &str,
        summary_text: &str,
        center_node_id: Option<&str>,
        member_node_ids: &[String],
        created_at_ms: u64,
    ) -> Result<()> {
        let name = name.to_string();
        let summary_text = summary_text.to_string();
        let center_node_id = center_node_id.map(str::to_string);
        let member_node_ids = member_node_ids.to_vec();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memory_graph_clusters (name, summary_text, center_node_id, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(name) DO UPDATE SET
                        summary_text = excluded.summary_text,
                        center_node_id = excluded.center_node_id",
                    params![name, summary_text, center_node_id, created_at_ms as i64],
                )?;
                let cluster_id = conn.query_row(
                    "SELECT id FROM memory_graph_clusters WHERE name = ?1",
                    params![name],
                    |row| row.get::<_, i64>(0),
                )?;
                conn.execute(
                    "UPDATE memory_cluster_members SET deleted_at = ?2 WHERE cluster_id = ?1 AND deleted_at IS NULL",
                    params![cluster_id, crate::history::now_ts() as i64],
                )?;
                for node_id in member_node_ids {
                    conn.execute(
                        "INSERT OR REPLACE INTO memory_cluster_members (cluster_id, node_id, deleted_at) VALUES (?1, ?2, NULL)",
                        params![cluster_id, node_id],
                    )?;
                }
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_clusters_for_node(
        &self,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryGraphClusterRow>> {
        let node_id = node_id.to_string();
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT c.id, c.name, c.summary_text, c.center_node_id, c.created_at_ms
                     FROM memory_cluster_members m
                     JOIN memory_graph_clusters c ON c.id = m.cluster_id
                     WHERE m.node_id = ?1 AND m.deleted_at IS NULL
                     ORDER BY c.created_at_ms DESC, c.id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(params![node_id, limit], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, i64>(4)?.max(0) as u64,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let mut clusters = Vec::new();
                for (id, name, summary_text, center_node_id, created_at_ms) in rows {
                    let mut member_stmt = conn.prepare(
                        "SELECT node_id FROM memory_cluster_members WHERE cluster_id = ?1 AND deleted_at IS NULL ORDER BY node_id ASC",
                    )?;
                    let member_node_ids = member_stmt
                        .query_map(params![id], |row| row.get::<_, String>(0))?
                        .collect::<std::result::Result<Vec<_>, _>>()?;
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
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
