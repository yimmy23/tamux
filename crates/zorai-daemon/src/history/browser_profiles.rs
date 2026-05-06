use super::*;

/// Thresholds for browser profile expiry detection (in milliseconds).
const EXPIRY_LAST_USED_THRESHOLD_MS: u64 = 30 * 24 * 60 * 60 * 1000; // 30 days
const EXPIRY_LAST_AUTH_SUCCESS_THRESHOLD_MS: u64 = 90 * 24 * 60 * 60 * 1000; // 90 days
const STALE_LAST_USED_THRESHOLD_MS: u64 = 14 * 24 * 60 * 60 * 1000; // 14 days

fn map_browser_profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BrowserProfileRow> {
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
}

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
                    map_browser_profile_row,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_browser_profiles(&self) -> Result<Vec<BrowserProfileRow>> {
        self.list_browser_profiles_filtered(None, None).await
    }

    pub async fn list_browser_profiles_filtered(
        &self,
        health_state: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<Vec<BrowserProfileRow>> {
        let health_state = health_state.map(str::to_string);
        let workspace_id = workspace_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let mut sql = "SELECT profile_id, label, profile_dir, browser_kind, workspace_id, health_state, \
                     created_at, updated_at, last_used_at, last_auth_success_at, last_auth_failure_at, last_auth_failure_reason \
                     FROM browser_profiles".to_string();
                let mut conditions = Vec::new();
                let mut values = Vec::<rusqlite::types::Value>::new();
                if let Some(health_state) = health_state.as_deref() {
                    conditions.push("health_state = ?");
                    values.push(rusqlite::types::Value::Text(health_state.to_string()));
                }
                if let Some(workspace_id) = workspace_id.as_deref() {
                    conditions.push("workspace_id = ?");
                    values.push(rusqlite::types::Value::Text(workspace_id.to_string()));
                }
                if !conditions.is_empty() {
                    sql.push_str(" WHERE ");
                    sql.push_str(&conditions.join(" AND "));
                }
                sql.push_str(" ORDER BY updated_at DESC, profile_id ASC");

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(values.iter()), map_browser_profile_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_unhealthy_browser_profiles(&self) -> Result<Vec<BrowserProfileRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT profile_id, label, profile_dir, browser_kind, workspace_id, health_state, \
                     created_at, updated_at, last_used_at, last_auth_success_at, last_auth_failure_at, last_auth_failure_reason \
                     FROM browser_profiles \
                     WHERE health_state != ?1 \
                     ORDER BY updated_at DESC, profile_id ASC",
                )?;
                let rows = stmt
                    .query_map(params!["healthy"], map_browser_profile_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn list_browser_profile_expiry_candidates(
        &self,
        now_ms: u64,
    ) -> Result<Vec<BrowserProfileRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT profile_id, label, profile_dir, browser_kind, workspace_id, health_state, \
                     created_at, updated_at, last_used_at, last_auth_success_at, last_auth_failure_at, last_auth_failure_reason \
                     FROM browser_profiles \
                     WHERE health_state NOT IN ('retired', 'corrupted', 'repair_needed', 'repair_in_progress') \
                       AND ( \
                           (last_auth_success_at IS NOT NULL AND ?1 - last_auth_success_at > ?2) \
                           OR (last_used_at IS NOT NULL AND ?1 - last_used_at > ?3) \
                       ) \
                     ORDER BY updated_at DESC, profile_id ASC",
                )?;
                let rows = stmt
                    .query_map(
                        params![
                            now_ms as i64,
                            EXPIRY_LAST_AUTH_SUCCESS_THRESHOLD_MS as i64,
                            STALE_LAST_USED_THRESHOLD_MS as i64,
                        ],
                        map_browser_profile_row,
                    )?
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
        let profiles = self.list_browser_profile_expiry_candidates(now_ms).await?;
        let mut reclassified = Vec::new();

        for profile in profiles {
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
