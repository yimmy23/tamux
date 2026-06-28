use super::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadStructuralMemoryRow {
    pub thread_id: String,
    pub state_json: serde_json::Value,
    pub updated_at: u64,
}

fn map_thread_structural_memory_row(row: &db::Row) -> anyhow::Result<ThreadStructuralMemoryRow> {
    let state_json_raw = row.get::<String>(1)?;
    let state_json = serde_json::from_str(&state_json_raw)?;
    Ok(ThreadStructuralMemoryRow {
        thread_id: row.get(0)?,
        state_json,
        updated_at: row.get::<i64>(2)?.max(0) as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_thread_structural_memory(
        &self,
        thread_id: &str,
        state_json: &serde_json::Value,
        updated_at: u64,
    ) -> Result<()> {
        let state_json = serde_json::to_string(state_json)?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO thread_structural_memory (thread_id, state_json, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                db::db_params![thread_id, state_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_thread_structural_memory(
        &self,
        thread_id: &str,
    ) -> Result<Option<ThreadStructuralMemoryRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT thread_id, state_json, updated_at FROM thread_structural_memory WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| map_thread_structural_memory_row(&row))
            .transpose()
    }

    pub async fn list_thread_structural_memory_for_threads(
        &self,
        thread_ids: &[String],
    ) -> Result<Vec<ThreadStructuralMemoryRow>> {
        let mut thread_ids = thread_ids
            .iter()
            .map(|thread_id| thread_id.trim())
            .filter(|thread_id| !thread_id.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        thread_ids.sort();
        thread_ids.dedup();
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = std::iter::repeat_n("?", thread_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT thread_id, state_json, updated_at \
             FROM thread_structural_memory \
             WHERE deleted_at IS NULL AND thread_id IN ({placeholders}) \
             ORDER BY thread_id ASC"
        );
        let values = thread_ids.into_iter().map(db::Value::Text).collect();
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter().map(map_thread_structural_memory_row).collect()
    }

    pub async fn delete_thread_structural_memory(&self, thread_id: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE thread_structural_memory SET deleted_at = ?2 WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn upsert_thread_structural_memory_state<T: Serialize>(
        &self,
        thread_id: &str,
        state: &T,
        updated_at: u64,
    ) -> Result<()> {
        let state_json = serde_json::to_value(state)?;
        self.upsert_thread_structural_memory(thread_id, &state_json, updated_at)
            .await
    }

    pub async fn get_thread_structural_memory_state<T: DeserializeOwned>(
        &self,
        thread_id: &str,
    ) -> Result<Option<T>> {
        let Some(row) = self.get_thread_structural_memory(thread_id).await? else {
            return Ok(None);
        };

        serde_json::from_value(row.state_json)
            .map(Some)
            .map_err(Into::into)
    }
}
