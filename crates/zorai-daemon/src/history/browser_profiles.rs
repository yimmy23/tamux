use super::*;

impl HistoryStore {
    pub async fn upsert_browser_profile(
        &self,
        profile: &crate::agent::types::BrowserProfile,
    ) -> Result<()> {
        let profile = profile.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO browser_profiles \
                     (profile_id, label, profile_dir, created_at, updated_at, last_used_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        profile.profile_id,
                        profile.label,
                        profile.profile_dir,
                        profile.created_at as i64,
                        profile.updated_at as i64,
                        profile.last_used_at.map(|value| value as i64),
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_browser_profile(&self, profile_id: &str) -> Result<Option<BrowserProfileRow>> {
        let profile_id = profile_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT profile_id, label, profile_dir, created_at, updated_at, last_used_at \
                     FROM browser_profiles WHERE profile_id = ?1",
                    params![profile_id],
                    |row| {
                        Ok(BrowserProfileRow {
                            profile_id: row.get(0)?,
                            label: row.get(1)?,
                            profile_dir: row.get(2)?,
                            created_at: row.get::<_, i64>(3)? as u64,
                            updated_at: row.get::<_, i64>(4)? as u64,
                            last_used_at: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
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
