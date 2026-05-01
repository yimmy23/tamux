use super::*;

/// Thresholds for browser profile expiry detection (in milliseconds).
const EXPIRY_LAST_USED_THRESHOLD_MS: u64 = 30 * 24 * 60 * 60 * 1000; // 30 days
const EXPIRY_LAST_AUTH_SUCCESS_THRESHOLD_MS: u64 = 90 * 24 * 60 * 60 * 1000; // 90 days
const STALE_LAST_USED_THRESHOLD_MS: u64 = 14 * 24 * 60 * 60 * 1000; // 14 days

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
                     (profile_id, label, profile_dir, browser_kind, workspace_id, health_state, \
                      created_at, updated_at, last_used_at, last_auth_success_at, last_auth_failure_at, last_auth_failure_reason) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        profile.profile_id,
                        profile.label,
                        profile.profile_dir,
                        profile.browser_kind,
                        profile.workspace_id,
                        profile.health_state.as_str(),
                        profile.created_at as i64,
                        profile.updated_at as i64,
                        profile.last_used_at.map(|value| value as i64),
                        profile.last_auth_success_at.map(|value| value as i64),
                        profile.last_auth_failure_at.map(|value| value as i64),
                        profile.last_auth_failure_reason,
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
                    "SELECT profile_id, label, profile_dir, browser_kind, workspace_id, health_state, \
                     created_at, updated_at, last_used_at, last_auth_success_at, last_auth_failure_at, last_auth_failure_reason \
                     FROM browser_profiles WHERE profile_id = ?1",
                    params![profile_id],
                    |row| {
                        Ok(BrowserProfileRow {
                            profile_id: row.get(0)?,
                            label: row.get(1)?,
                            profile_dir: row.get(2)?,
                            browser_kind: row.get(3)?,
                            workspace_id: row.get(4)?,
                            health_state: row.get(5)?,
                            created_at: row.get::<_, i64>(6)? as u64,
                            updated_at: row.get::<_, i64>(7)? as u64,
                            last_used_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                            last_auth_success_at: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                            last_auth_failure_at: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
                            last_auth_failure_reason: row.get(11)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_browser_profiles(&self) -> Result<Vec<BrowserProfileRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT profile_id, label, profile_dir, browser_kind, workspace_id, health_state, \
                     created_at, updated_at, last_used_at, last_auth_success_at, last_auth_failure_at, last_auth_failure_reason \
                     FROM browser_profiles ORDER BY updated_at DESC, profile_id ASC",
                )?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(BrowserProfileRow {
                            profile_id: row.get(0)?,
                            label: row.get(1)?,
                            profile_dir: row.get(2)?,
                            browser_kind: row.get(3)?,
                            workspace_id: row.get(4)?,
                            health_state: row.get(5)?,
                            created_at: row.get::<_, i64>(6)? as u64,
                            updated_at: row.get::<_, i64>(7)? as u64,
                            last_used_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                            last_auth_success_at: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                            last_auth_failure_at: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
                            last_auth_failure_reason: row.get(11)?,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_browser_profile(&self, profile_id: &str) -> Result<()> {
        let profile_id = profile_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM browser_profiles WHERE profile_id = ?1",
                    params![profile_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Detect and classify browser profiles that have expired or become stale based on
    /// last-used and last-auth-success timestamps. Profiles classified as `Expired` or
    /// `Stale` are automatically updated in the database. Returns a list of
    /// (profile_id, old_health, new_health, reason) for every profile that was reclassified.
    pub async fn detect_and_classify_expired_profiles(
        &self,
        now_ms: u64,
    ) -> Result<Vec<(String, String, String, String)>> {
        let profiles = self.list_browser_profiles().await?;
        let mut reclassified = Vec::new();

        for profile in profiles {
            // Skip profiles that are already in a terminal or manual-repair state
            if matches!(
                profile.health_state.as_str(),
                "retired" | "corrupted" | "repair_needed" | "repair_in_progress"
            ) {
                continue;
            }

            let last_used_age = profile.last_used_at.map(|ts| now_ms.saturating_sub(ts));
            let last_auth_success_age = profile
                .last_auth_success_at
                .map(|ts| now_ms.saturating_sub(ts));

            let (new_state, reason) = if let Some(age) = last_auth_success_age {
                if age > EXPIRY_LAST_AUTH_SUCCESS_THRESHOLD_MS {
                    (
                        "expired",
                        format!(
                            "last auth success {}ms ago exceeds {}ms threshold",
                            age, EXPIRY_LAST_AUTH_SUCCESS_THRESHOLD_MS
                        ),
                    )
                } else if let Some(used_age) = last_used_age {
                    if used_age > EXPIRY_LAST_USED_THRESHOLD_MS {
                        (
                            "expired",
                            format!(
                                "last used {}ms ago exceeds {}ms threshold",
                                used_age, EXPIRY_LAST_USED_THRESHOLD_MS
                            ),
                        )
                    } else if used_age > STALE_LAST_USED_THRESHOLD_MS {
                        (
                            "stale",
                            format!(
                                "last used {}ms ago exceeds {}ms stale threshold",
                                used_age, STALE_LAST_USED_THRESHOLD_MS
                            ),
                        )
                    } else {
                        continue; // still fresh
                    }
                } else {
                    continue; // no last_used_at, can't classify
                }
            } else {
                // No auth success timestamp; fall back to last_used_at only
                if let Some(used_age) = last_used_age {
                    if used_age > EXPIRY_LAST_USED_THRESHOLD_MS {
                        (
                            "expired",
                            format!(
                                "last used {}ms ago exceeds {}ms threshold (no auth success record)",
                                used_age, EXPIRY_LAST_USED_THRESHOLD_MS
                            ),
                        )
                    } else if used_age > STALE_LAST_USED_THRESHOLD_MS {
                        (
                            "stale",
                            format!(
                                "last used {}ms ago exceeds {}ms stale threshold (no auth success record)",
                                used_age, STALE_LAST_USED_THRESHOLD_MS
                            ),
                        )
                    } else {
                        continue; // still fresh
                    }
                } else {
                    continue; // no timestamps to classify
                }
            };

            if profile.health_state == new_state {
                continue; // already correctly classified
            }

            let old_state = profile.health_state.clone();

            // Update the profile in the database
            let pid = profile.profile_id.clone();
            let ns = new_state.to_string();
            self.conn
                .call(move |conn| {
                    conn.execute(
                        "UPDATE browser_profiles SET health_state = ?2, updated_at = ?3 WHERE profile_id = ?1",
                        params![pid, ns, now_ms as i64],
                    )?;
                    Ok(())
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            reclassified.push((profile.profile_id, old_state, new_state.to_string(), reason));
        }

        Ok(reclassified)
    }
}
