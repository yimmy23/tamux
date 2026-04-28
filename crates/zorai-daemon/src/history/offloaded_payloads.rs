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
        let payload_id = payload_id.to_string();
        let thread_id = thread_id.to_string();
        let tool_name = tool_name.to_string();
        let tool_call_id = tool_call_id.map(str::to_string);
        let content_type = content_type.to_string();
        let summary = summary.to_string();
        let storage_path = self.offloaded_payload_storage_path(&thread_id, &payload_id);

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO offloaded_payloads \
                     (payload_id, thread_id, tool_name, tool_call_id, storage_path, content_type, byte_size, summary, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
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
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_offloaded_payload_metadata(
        &self,
        payload_id: &str,
    ) -> Result<Option<OffloadedPayloadMetadataRow>> {
        let payload_id = payload_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT payload_id, thread_id, tool_name, tool_call_id, storage_path, content_type, byte_size, summary, created_at \
                     FROM offloaded_payloads WHERE payload_id = ?1",
                    params![payload_id],
                    |row| {
                        Ok(OffloadedPayloadMetadataRow {
                            payload_id: row.get(0)?,
                            thread_id: row.get(1)?,
                            tool_name: row.get(2)?,
                            tool_call_id: row.get(3)?,
                            storage_path: row.get(4)?,
                            content_type: row.get(5)?,
                            byte_size: row.get::<_, i64>(6)?.max(0) as u64,
                            summary: row.get(7)?,
                            created_at: row.get::<_, i64>(8)?.max(0) as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_offloaded_payload_metadata_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Vec<OffloadedPayloadMetadataRow>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT payload_id, thread_id, tool_name, tool_call_id, storage_path, content_type, byte_size, summary, created_at \
                     FROM offloaded_payloads WHERE thread_id = ?1 ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![thread_id], |row| {
                    Ok(OffloadedPayloadMetadataRow {
                        payload_id: row.get(0)?,
                        thread_id: row.get(1)?,
                        tool_name: row.get(2)?,
                        tool_call_id: row.get(3)?,
                        storage_path: row.get(4)?,
                        content_type: row.get(5)?,
                        byte_size: row.get::<_, i64>(6)?.max(0) as u64,
                        summary: row.get(7)?,
                        created_at: row.get::<_, i64>(8)?.max(0) as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_offloaded_payload_metadata(&self, payload_id: &str) -> Result<()> {
        let payload_id = payload_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM offloaded_payloads WHERE payload_id = ?1",
                    params![payload_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
