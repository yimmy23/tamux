use super::*;

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
        let id = id.to_string();
        let thread_id = thread_id.to_string();
        let original_role = original_role.map(str::to_string);
        let compressed_content = compressed_content.to_string();
        let summary = summary.map(str::to_string);
        let metadata_json = metadata_json.map(str::to_string);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO context_archive (id, thread_id, original_role, compressed_content, summary, relevance_score, token_count_original, token_count_compressed, metadata_json, archived_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, thread_id, original_role, compressed_content, summary, relevance_score, token_count_original as i64, token_count_compressed as i64, metadata_json, archived_at as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn search_context_archive(
        &self,
        thread_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        let thread_id = thread_id.to_string();
        let query = query.to_string();
        self.conn.call(move |conn| {
        // Try FTS5 first, fall back to LIKE search
        let fts_result: Result<Vec<String>> = (|| {
            let mut stmt = conn.prepare(
                "SELECT ca.id FROM context_archive ca JOIN context_archive_fts fts ON ca.rowid = fts.rowid WHERE ca.thread_id = ?1 AND context_archive_fts MATCH ?2 ORDER BY rank LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![thread_id, query, limit], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        })();

        match fts_result {
            Ok(ids) if !ids.is_empty() => Ok(ids),
            Ok(_) | Err(_) => {
                // Fallback: simple LIKE search
                let like_pattern = format!("%{query}%");
                let mut stmt = conn.prepare(
                    "SELECT id FROM context_archive WHERE thread_id = ?1 AND (compressed_content LIKE ?2 OR summary LIKE ?2) ORDER BY archived_at DESC LIMIT ?3",
                )?;
                let rows =
                    stmt.query_map(params![thread_id, like_pattern, limit], |row| row.get(0))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            }
        }
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// List the most recent context archive entries for a thread.
    pub async fn list_context_archive_entries(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<ContextArchiveRow>> {
        let thread_id = thread_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, original_role, compressed_content, summary, \
                     relevance_score, token_count_original, token_count_compressed, \
                     metadata_json, archived_at, last_accessed_at \
                     FROM context_archive WHERE thread_id = ?1 \
                     ORDER BY archived_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![thread_id, limit as i64], |row| {
                    Ok(ContextArchiveRow {
                        id: row.get(0)?,
                        thread_id: row.get(1)?,
                        original_role: row.get(2)?,
                        compressed_content: row.get(3)?,
                        summary: row.get(4)?,
                        relevance_score: row.get::<_, f64>(5).unwrap_or(0.0),
                        token_count_original: row.get::<_, i64>(6).unwrap_or(0),
                        token_count_compressed: row.get::<_, i64>(7).unwrap_or(0),
                        metadata_json: row.get(8)?,
                        archived_at: row.get(9)?,
                        last_accessed_at: row.get(10)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
