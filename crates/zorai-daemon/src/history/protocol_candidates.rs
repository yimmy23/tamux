use super::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadProtocolCandidatesRow {
    pub thread_id: String,
    pub state_json: serde_json::Value,
    pub updated_at: u64,
}

impl HistoryStore {
    pub async fn upsert_thread_protocol_candidates(
        &self,
        thread_id: &str,
        state_json: &serde_json::Value,
        updated_at: u64,
    ) -> Result<()> {
        let thread_id = thread_id.to_string();
        let state_json = serde_json::to_string(state_json)?;

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO thread_protocol_candidates (thread_id, state_json, updated_at) VALUES (?1, ?2, ?3)",
                    params![thread_id, state_json, updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_thread_protocol_candidates(
        &self,
        thread_id: &str,
    ) -> Result<Option<ThreadProtocolCandidatesRow>> {
        let thread_id = thread_id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT thread_id, state_json, updated_at FROM thread_protocol_candidates WHERE thread_id = ?1",
                    params![thread_id],
                    |row| {
                        let state_json_raw = row.get::<_, String>(1)?;
                        let state_json = serde_json::from_str(&state_json_raw).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                1,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?;
                        Ok(ThreadProtocolCandidatesRow {
                            thread_id: row.get(0)?,
                            state_json,
                            updated_at: row.get::<_, i64>(2)?.max(0) as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
