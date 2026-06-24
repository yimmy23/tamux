use super::*;

fn map_context_archive_row(row: &db::Row) -> anyhow::Result<ContextArchiveRow> {
    Ok(ContextArchiveRow {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        original_role: row.get(2)?,
        compressed_content: row.get(3)?,
        summary: row.get(4)?,
        relevance_score: row.get::<f64>(5).unwrap_or(0.0),
        token_count_original: row.get::<i64>(6).unwrap_or(0),
        token_count_compressed: row.get::<i64>(7).unwrap_or(0),
        metadata_json: row.get(8)?,
        archived_at: row.get(9)?,
        last_accessed_at: row.get(10)?,
    })
}

impl HistoryStore {
    pub async fn insert_context_archive(
        &self,
        id: &str,
        thread_id: &str,
        original_role: Option<&str>,
        compressed_content: &str,
        summary: Option<&str>,
        relevance_score: f64,
        token_count_original: u32,
        token_count_compressed: u32,
        metadata_json: Option<&str>,
        archived_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO context_archive (id, thread_id, original_role, compressed_content, summary, relevance_score, token_count_original, token_count_compressed, metadata_json, archived_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![id, thread_id, original_role, compressed_content, summary, relevance_score, token_count_original as i64, token_count_compressed as i64, metadata_json, archived_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn search_context_archive(
        &self,
        thread_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        // Prefer FTS; fall back to LIKE when FTS errors or yields nothing
        // (mirrors the prior single-connection behavior).
        let fts_ids: Vec<String> = match self
            .conn_db
            .query(
                "SELECT ca.id FROM context_archive ca JOIN context_archive_fts fts ON ca.rowid = fts.rowid WHERE ca.thread_id = ?1 AND context_archive_fts MATCH ?2 ORDER BY rank LIMIT ?3",
                db::db_params![thread_id, query, limit as i64],
            )
            .await
        {
            Ok(rows) => rows
                .iter()
                .map(|row| row.get::<String>(0))
                .collect::<anyhow::Result<Vec<_>>>()
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        if !fts_ids.is_empty() {
            return Ok(fts_ids);
        }

        let like_pattern = format!("%{query}%");
        let rows = self
            .conn_db
            .query(
                "SELECT id FROM context_archive WHERE thread_id = ?1 AND (compressed_content LIKE ?2 OR summary LIKE ?2) ORDER BY archived_at DESC LIMIT ?3",
                db::db_params![thread_id, like_pattern, limit as i64],
            )
            .await?;
        rows.iter().map(|row| row.get::<String>(0)).collect()
    }

    /// List the most recent context archive entries for a thread.
    pub async fn list_context_archive_entries(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<ContextArchiveRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, thread_id, original_role, compressed_content, summary, \
                     relevance_score, token_count_original, token_count_compressed, \
                     metadata_json, archived_at, last_accessed_at \
                     FROM context_archive WHERE thread_id = ?1 \
                     ORDER BY archived_at DESC LIMIT ?2",
                db::db_params![thread_id, limit as i64],
            )
            .await?;
        rows.iter().map(map_context_archive_row).collect()
    }
}
