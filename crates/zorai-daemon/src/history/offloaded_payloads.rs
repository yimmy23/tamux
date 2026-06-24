use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffloadedPayloadMetadataRow {
    pub payload_id: String,
    pub thread_id: String,
    pub tool_name: String,
    pub tool_call_id: Option<String>,
    pub storage_path: String,
    pub content_type: String,
    pub byte_size: u64,
    pub summary: String,
    pub created_at: u64,
}

fn map_offloaded_payload_metadata_row(
    row: &db::Row,
) -> anyhow::Result<OffloadedPayloadMetadataRow> {
    Ok(OffloadedPayloadMetadataRow {
        payload_id: row.get(0)?,
        thread_id: row.get(1)?,
        tool_name: row.get(2)?,
        tool_call_id: row.get(3)?,
        storage_path: row.get(4)?,
        content_type: row.get(5)?,
        byte_size: row.get::<i64>(6)?.max(0) as u64,
        summary: row.get(7)?,
        created_at: row.get::<i64>(8)?.max(0) as u64,
    })
}

impl HistoryStore {
    fn offloaded_payload_storage_path(&self, thread_id: &str, payload_id: &str) -> String {
        self.offloaded_payload_path(thread_id, payload_id)
            .to_string_lossy()
            .into_owned()
    }

    pub async fn upsert_offloaded_payload_metadata(
        &self,
        payload_id: &str,
        thread_id: &str,
        tool_name: &str,
        tool_call_id: Option<&str>,
        content_type: &str,
        byte_size: u64,
        summary: &str,
        created_at: u64,
    ) -> Result<()> {
        let storage_path = self.offloaded_payload_storage_path(thread_id, payload_id);
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO offloaded_payloads \
                     (payload_id, thread_id, tool_name, tool_call_id, storage_path, content_type, byte_size, summary, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                db::db_params![
                    payload_id,
                    thread_id,
                    tool_name,
                    tool_call_id,
                    storage_path,
                    content_type,
                    byte_size as i64,
                    summary,
                    created_at as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_offloaded_payload_metadata(
        &self,
        payload_id: &str,
    ) -> Result<Option<OffloadedPayloadMetadataRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT payload_id, thread_id, tool_name, tool_call_id, storage_path, content_type, byte_size, summary, created_at \
                     FROM offloaded_payloads WHERE payload_id = ?1",
                db::db_params![payload_id],
            )
            .await?;
        row.map(|row| map_offloaded_payload_metadata_row(&row)).transpose()
    }

    pub async fn list_offloaded_payload_metadata_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Vec<OffloadedPayloadMetadataRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT payload_id, thread_id, tool_name, tool_call_id, storage_path, content_type, byte_size, summary, created_at \
                     FROM offloaded_payloads WHERE thread_id = ?1 ORDER BY created_at DESC",
                db::db_params![thread_id],
            )
            .await?;
        rows.iter().map(map_offloaded_payload_metadata_row).collect()
    }

    pub async fn delete_offloaded_payload_metadata(&self, payload_id: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE offloaded_payloads SET deleted_at = ?2 WHERE payload_id = ?1 AND deleted_at IS NULL",
                db::db_params![payload_id, now_ts() as i64],
            )
            .await?;
        Ok(())
    }
}
