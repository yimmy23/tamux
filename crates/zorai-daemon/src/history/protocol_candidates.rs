use super::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadProtocolCandidatesRow {
    pub thread_id: String,
    pub state_json: serde_json::Value,
    pub updated_at: u64,
}

fn map_thread_protocol_candidates_row(
    row: &db::Row,
) -> anyhow::Result<ThreadProtocolCandidatesRow> {
    let state_json_raw = row.get::<String>(1)?;
    let state_json = serde_json::from_str(&state_json_raw)?;
    Ok(ThreadProtocolCandidatesRow {
        thread_id: row.get(0)?,
        state_json,
        updated_at: row.get::<i64>(2)?.max(0) as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_thread_protocol_candidates(
        &self,
        thread_id: &str,
        state_json: &serde_json::Value,
        updated_at: u64,
    ) -> Result<()> {
        let state_json = serde_json::to_string(state_json)?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO thread_protocol_candidates (thread_id, state_json, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![thread_id, state_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_thread_protocol_candidates(
        &self,
        thread_id: &str,
    ) -> Result<Option<ThreadProtocolCandidatesRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT thread_id, state_json, updated_at FROM thread_protocol_candidates WHERE thread_id = ?1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| map_thread_protocol_candidates_row(&row))
            .transpose()
    }

    pub async fn list_thread_protocol_candidates(
        &self,
    ) -> Result<Vec<ThreadProtocolCandidatesRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT thread_id, state_json, updated_at
                     FROM thread_protocol_candidates
                     ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter()
            .map(map_thread_protocol_candidates_row)
            .collect()
    }

    pub async fn upsert_thread_protocol_candidates_state<T: Serialize>(
        &self,
        thread_id: &str,
        state: &T,
        updated_at: u64,
    ) -> Result<()> {
        let state_json = serde_json::to_value(state)?;
        self.upsert_thread_protocol_candidates(thread_id, &state_json, updated_at)
            .await
    }

    pub async fn get_thread_protocol_candidates_state<T: DeserializeOwned>(
        &self,
        thread_id: &str,
    ) -> Result<Option<T>> {
        let Some(row) = self.get_thread_protocol_candidates(thread_id).await? else {
            return Ok(None);
        };

        serde_json::from_value(row.state_json)
            .map(Some)
            .map_err(Into::into)
    }
}
