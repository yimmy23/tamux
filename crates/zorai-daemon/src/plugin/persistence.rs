use anyhow::Result;
use rusqlite::params;
use std::sync::Arc;

/// Internal plugin record (not exposed to protocol crate).
#[derive(Debug, Clone)]
pub struct PluginRecord {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub manifest_json: String,
    pub install_source: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

/// SQLite persistence layer for plugin metadata.
pub struct PluginPersistence {
    history: Arc<crate::history::HistoryStore>,
}

impl PluginPersistence {
    pub fn new(history: Arc<crate::history::HistoryStore>) -> Self {
        Self { history }
    }

    /// List all plugins. Per PLUG-09.
    pub async fn list_plugins(&self) -> Result<Vec<PluginRecord>> {
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at FROM plugins WHERE deleted_at IS NULL ORDER BY name ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(PluginRecord {
                        name: row.get(0)?,
                        version: row.get(1)?,
                        description: row.get(2)?,
                        author: row.get(3)?,
                        manifest_json: row.get(4)?,
                        install_source: row.get(5)?,
                        enabled: row.get::<_, i64>(6)? != 0,
                        installed_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                })?;
                let mut records = Vec::new();
                for row in rows {
                    records.push(row?);
                }
                Ok(records)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Get a single plugin by name. Per PLUG-09.
    pub async fn get_plugin(&self, name: &str) -> Result<Option<PluginRecord>> {
        let name = name.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at FROM plugins WHERE name = ?1 AND deleted_at IS NULL",
                )?;
                let record = stmt
                    .query_row(params![name], |row| {
                        Ok(PluginRecord {
                            name: row.get(0)?,
                            version: row.get(1)?,
                            description: row.get(2)?,
                            author: row.get(3)?,
                            manifest_json: row.get(4)?,
                            install_source: row.get(5)?,
                            enabled: row.get::<_, i64>(6)? != 0,
                            installed_at: row.get(7)?,
                            updated_at: row.get(8)?,
                        })
                    })
                    .optional()?;
                Ok(record)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Insert or update a plugin record. Per PLUG-09.
    pub async fn upsert_plugin(&self, record: &PluginRecord) -> Result<()> {
        let record = record.clone();
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO plugins (name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at, deleted_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) \
                     ON CONFLICT(name) DO UPDATE SET \
                       version = excluded.version, \
                       description = excluded.description, \
                       author = excluded.author, \
                       manifest_json = excluded.manifest_json, \
                       updated_at = excluded.updated_at, \
                       deleted_at = NULL",
                    params![
                        record.name,
                        record.version,
                        record.description,
                        record.author,
                        record.manifest_json,
                        record.install_source,
                        record.enabled as i64,
                        record.installed_at,
                        record.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Set enabled/disabled status. Per PLUG-09.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let name = name.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.history
            .conn
            .call(move |conn| {
                let rows = conn.execute(
                    "UPDATE plugins SET enabled = ?1, updated_at = ?2 WHERE name = ?3 AND deleted_at IS NULL",
                    params![enabled as i64, now, name],
                )?;
                if rows == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows.into());
                }
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Remove a single plugin by name (and its settings/credentials). Per INST-04/D-06.
    pub async fn remove_plugin(&self, name: &str) -> Result<bool> {
        let name = name.to_string();
        self.history
            .conn
            .call(move |conn| {
                let deleted_at = crate::history::now_ts() as i64;
                conn.execute(
                    "UPDATE plugin_settings SET deleted_at = ?2 WHERE plugin_name = ?1 AND deleted_at IS NULL",
                    params![name, deleted_at],
                )?;
                conn.execute(
                    "UPDATE plugin_credentials SET deleted_at = ?2 WHERE plugin_name = ?1 AND deleted_at IS NULL",
                    params![name, deleted_at],
                )?;
                let rows = conn.execute(
                    "UPDATE plugins SET deleted_at = ?2 WHERE name = ?1 AND deleted_at IS NULL",
                    params![name, deleted_at],
                )?;
                Ok(rows > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Get all settings for a plugin. Returns (key, value, is_secret) tuples.
    /// Secret values are base64-decoded before returning. Per PSET-06.
    pub async fn get_settings(&self, plugin_name: &str) -> Result<Vec<(String, String, bool)>> {
        let plugin_name = plugin_name.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT key, value, is_secret FROM plugin_settings WHERE plugin_name = ?1 AND deleted_at IS NULL ORDER BY key ASC",
                )?;
                let rows = stmt.query_map(params![plugin_name], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?;
                let mut settings = Vec::new();
                for row in rows {
                    let (key, raw_value, is_secret) = row?;
                    let value = if is_secret {
                        // Decode base64 for secret values
                        use base64::Engine;
                        match base64::engine::general_purpose::STANDARD.decode(&raw_value) {
                            Ok(bytes) => String::from_utf8(bytes).unwrap_or(raw_value),
                            Err(_) => raw_value, // Fallback: return as-is if not valid base64
                        }
                    } else {
                        raw_value
                    };
                    settings.push((key, value, is_secret));
                }
                Ok(settings)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Upsert a single plugin setting. Per PSET-06/D-06.
    /// Secret values are base64-encoded before storage (placeholder for Phase 18 AES encryption).
    pub async fn upsert_setting(
        &self,
        plugin_name: &str,
        key: &str,
        value: &str,
        is_secret: bool,
    ) -> Result<()> {
        let plugin_name = plugin_name.to_string();
        let key = key.to_string();
        let stored_value = if is_secret {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
        } else {
            value.to_string()
        };
        let is_secret_i64 = is_secret as i64;
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO plugin_settings (plugin_name, key, value, is_secret, deleted_at) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![plugin_name, key, stored_value, is_secret_i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Remove plugins not in the provided set (stale record reconciliation per Pitfall 6).
    /// Returns the number of rows deleted.
    pub async fn remove_stale_plugins(&self, active_names: &[String]) -> Result<u64> {
        let active_names = active_names.to_vec();
        self.history
            .conn
            .call(move |conn| {
                if active_names.is_empty() {
                    let deleted = conn.execute(
                        "UPDATE plugins SET deleted_at = ?1 WHERE deleted_at IS NULL",
                        params![crate::history::now_ts() as i64],
                    )?;
                    return Ok(deleted as u64);
                }
                let placeholders: Vec<&str> = active_names.iter().map(|_| "?").collect();
                let sql = format!(
                    "UPDATE plugins SET deleted_at = ? WHERE deleted_at IS NULL AND name NOT IN ({})",
                    placeholders.join(",")
                );
                let deleted_at = crate::history::now_ts() as i64;
                let mut params: Vec<&dyn rusqlite::types::ToSql> = vec![&deleted_at];
                params.extend(active_names
                    .iter()
                    .map(|s| s as &dyn rusqlite::types::ToSql)
                );
                let deleted = conn.execute(&sql, params.as_slice())?;
                Ok(deleted as u64)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

use rusqlite::OptionalExtension;

impl PluginPersistence {
    /// Get a credential from the plugin_credentials table. Returns the raw
    /// encrypted blob and optional expiry timestamp. Decryption is the caller's
    /// responsibility (uses crypto module).
    pub async fn get_credential(
        &self,
        plugin_name: &str,
        credential_type: &str,
    ) -> Result<Option<(Vec<u8>, Option<String>)>> {
        let plugin_name = plugin_name.to_string();
        let credential_type = credential_type.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT encrypted_value, expires_at FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = ?2 AND deleted_at IS NULL",
                )?;
                let row = stmt
                    .query_row(params![plugin_name, credential_type], |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                        ))
                    })
                    .optional()?;
                Ok(row)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Insert or update a credential in the plugin_credentials table.
    /// The value should already be encrypted (via crypto::encrypt).
    pub async fn upsert_credential(
        &self,
        plugin_name: &str,
        credential_type: &str,
        encrypted_value: &[u8],
        expires_at: Option<&str>,
    ) -> Result<()> {
        let plugin_name = plugin_name.to_string();
        let credential_type = credential_type.to_string();
        let encrypted_value = encrypted_value.to_vec();
        let expires_at = expires_at.map(|s| s.to_string());
        let now = chrono::Utc::now().to_rfc3339();
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO plugin_credentials (plugin_name, credential_type, encrypted_value, expires_at, created_at, updated_at, deleted_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)
                     ON CONFLICT(plugin_name, credential_type) DO UPDATE SET
                         encrypted_value = excluded.encrypted_value,
                         expires_at = excluded.expires_at,
                         updated_at = excluded.updated_at,
                         deleted_at = NULL",
                    params![plugin_name, credential_type, encrypted_value, expires_at, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Compute auth status for a plugin based on credential state.
    /// Returns "connected", "refreshable", "needs_reconnect", or "not_configured".
    /// No decryption needed -- just checks row existence and expiry.
    pub async fn get_auth_status(&self, plugin_name: &str) -> Result<String> {
        let plugin_name = plugin_name.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut access_stmt = conn.prepare(
                    "SELECT expires_at FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = 'access_token' AND deleted_at IS NULL",
                )?;
                let access_row = access_stmt
                    .query_row(params![plugin_name], |row| {
                        Ok(row.get::<_, Option<String>>(0)?)
                    })
                    .optional()?;
                let mut refresh_stmt = conn.prepare(
                    "SELECT 1 FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = 'refresh_token' AND deleted_at IS NULL LIMIT 1",
                )?;
                let has_refresh_token = refresh_stmt
                    .query_row(params![plugin_name], |_row| Ok(()))
                    .optional()?
                    .is_some();

                let status = match access_row {
                    Some(expires_at) => super::manager_extras::auth_status_from_expiry_and_refresh_token(
                        expires_at.as_deref(),
                        has_refresh_token,
                    ),
                    None => super::manager_extras::PluginAuthStatus::NotConfigured,
                };
                Ok(status.as_str().to_string())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
