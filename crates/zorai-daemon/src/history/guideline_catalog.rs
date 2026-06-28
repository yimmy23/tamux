use super::*;

#[derive(Debug, Clone)]
pub(crate) struct GuidelineDocumentRecord {
    pub relative_path: String,
    pub excerpt: String,
    pub last_seen_at: i64,
    pub updated_at: i64,
}

fn map_guideline_document_record(row: &db::Row) -> anyhow::Result<GuidelineDocumentRecord> {
    Ok(GuidelineDocumentRecord {
        relative_path: row.get(0)?,
        excerpt: row.get(1)?,
        last_seen_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

impl HistoryStore {
    pub(crate) async fn register_guideline_document(
        &self,
        relative_path: &str,
        excerpt: &str,
        now_ms: i64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO discoverable_guideline_documents
                        (relative_path, excerpt, last_seen_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3)
                     ON CONFLICT(relative_path) DO UPDATE SET
                        excerpt = excluded.excerpt,
                        last_seen_at = excluded.last_seen_at,
                        updated_at = excluded.updated_at",
                db::db_params![relative_path, excerpt, now_ms],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn list_discoverable_guideline_documents(
        &self,
        limit: usize,
    ) -> Result<Vec<GuidelineDocumentRecord>> {
        let limit = limit.clamp(1, 4000) as i64;
        let rows = self
            .interactive_read_db
            .query(
                "SELECT relative_path, excerpt, last_seen_at, updated_at
                     FROM discoverable_guideline_documents
                     ORDER BY last_seen_at DESC, relative_path ASC
                     LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_guideline_document_record(row).ok())
            .collect())
    }

    pub(crate) async fn prune_stale_guideline_documents(&self, cutoff_ms: i64) -> Result<usize> {
        let removed = self
            .conn_db
            .execute(
                "DELETE FROM discoverable_guideline_documents WHERE last_seen_at < ?1",
                db::db_params![cutoff_ms],
            )
            .await?;
        Ok(removed as usize)
    }
}
