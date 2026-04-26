use super::*;

impl HistoryStore {
    pub async fn list_external_runtime_profiles(&self) -> Result<Vec<ExternalRuntimeProfileRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT runtime, profile_json, updated_at \
                     FROM external_runtime_profiles ORDER BY updated_at DESC, runtime ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(ExternalRuntimeProfileRow {
                        runtime: row.get(0)?,
                        profile_json: row.get(1)?,
                        updated_at: row.get::<_, i64>(2)? as u64,
                    })
                })?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_external_runtime_profile(
        &self,
        runtime: &str,
        profile: &crate::agent::types::ExternalRuntimeProfile,
    ) -> Result<()> {
        let runtime = runtime.to_string();
        let profile_json = serde_json::to_string(profile)?;
        let updated_at = now_ts() as i64;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO external_runtime_profiles \
                     (runtime, profile_json, updated_at) VALUES (?1, ?2, ?3)",
                    params![runtime, profile_json, updated_at],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_external_runtime_profile(
        &self,
        runtime: &str,
    ) -> Result<Option<ExternalRuntimeProfileRow>> {
        let runtime = runtime.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT runtime, profile_json, updated_at \
                     FROM external_runtime_profiles WHERE runtime = ?1",
                    params![runtime],
                    |row| {
                        Ok(ExternalRuntimeProfileRow {
                            runtime: row.get(0)?,
                            profile_json: row.get(1)?,
                            updated_at: row.get::<_, i64>(2)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
