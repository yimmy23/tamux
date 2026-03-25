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
                    "SELECT name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at FROM plugins ORDER BY name ASC",
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
                    "SELECT name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at FROM plugins WHERE name = ?1",
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
                    "INSERT OR REPLACE INTO plugins (name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
                    "UPDATE plugins SET enabled = ?1, updated_at = ?2 WHERE name = ?3",
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
                // Delete from all plugin tables
                conn.execute(
                    "DELETE FROM plugin_settings WHERE plugin_name = ?1",
                    params![name],
                )?;
                conn.execute(
                    "DELETE FROM plugin_credentials WHERE plugin_name = ?1",
                    params![name],
                )?;
                let rows = conn.execute("DELETE FROM plugins WHERE name = ?1", params![name])?;
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
                    "SELECT key, value, is_secret FROM plugin_settings WHERE plugin_name = ?1 ORDER BY key ASC",
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
                    "INSERT OR REPLACE INTO plugin_settings (plugin_name, key, value, is_secret) VALUES (?1, ?2, ?3, ?4)",
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
                    // Delete all plugins if no active names
                    let deleted = conn.execute("DELETE FROM plugins", [])?;
                    return Ok(deleted as u64);
                }
                // Build dynamic SQL: DELETE FROM plugins WHERE name NOT IN (?,?,...)
                let placeholders: Vec<&str> = active_names.iter().map(|_| "?").collect();
                let sql = format!(
                    "DELETE FROM plugins WHERE name NOT IN ({})",
                    placeholders.join(",")
                );
                let params: Vec<&dyn rusqlite::types::ToSql> = active_names
                    .iter()
                    .map(|s| s as &dyn rusqlite::types::ToSql)
                    .collect();
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
                    "SELECT encrypted_value, expires_at FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = ?2",
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
                    "INSERT INTO plugin_credentials (plugin_name, credential_type, encrypted_value, expires_at, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                     ON CONFLICT(plugin_name, credential_type) DO UPDATE SET
                         encrypted_value = excluded.encrypted_value,
                         expires_at = excluded.expires_at,
                         updated_at = excluded.updated_at",
                    params![plugin_name, credential_type, encrypted_value, expires_at, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Compute auth status for a plugin based on credential state.
    /// Returns "connected", "expired", or "not_configured".
    /// No decryption needed -- just checks row existence and expiry.
    pub async fn get_auth_status(&self, plugin_name: &str) -> Result<String> {
        let plugin_name = plugin_name.to_string();
        self.history
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT expires_at FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = 'access_token'",
                )?;
                let row = stmt
                    .query_row(params![plugin_name], |row| {
                        Ok(row.get::<_, Option<String>>(0)?)
                    })
                    .optional()?;
                match row {
                    Some(Some(expires_at)) => {
                        // Check if expired
                        match chrono::DateTime::parse_from_rfc3339(&expires_at) {
                            Ok(dt) => {
                                if dt > chrono::Utc::now() {
                                    Ok("connected".to_string())
                                } else {
                                    Ok("expired".to_string())
                                }
                            }
                            Err(_) => {
                                // Unparseable expiry -- treat as connected
                                Ok("connected".to_string())
                            }
                        }
                    }
                    Some(None) => {
                        // Row exists but no expiry -- treat as connected
                        Ok("connected".to_string())
                    }
                    None => Ok("not_configured".to_string()),
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_test_history() -> Arc<crate::history::HistoryStore> {
        let root = std::env::temp_dir().join(format!(
            "tamux-plugin-persist-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = crate::history::HistoryStore::new_test_store(&root)
            .await
            .unwrap();
        Arc::new(store)
    }

    fn sample_record(name: &str) -> PluginRecord {
        PluginRecord {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test plugin".to_string()),
            author: Some("Test Author".to_string()),
            manifest_json: r#"{"name":"test","version":"1.0.0","schema_version":1}"#.to_string(),
            install_source: "local".to_string(),
            enabled: true,
            installed_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn list_plugins_returns_empty_on_fresh_db() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);
        let plugins = persistence.list_plugins().await.unwrap();
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn upsert_then_list_returns_record() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        let record = sample_record("test-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        let plugins = persistence.list_plugins().await.unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "test-plugin");
        assert_eq!(plugins[0].version, "1.0.0");
        assert!(plugins[0].enabled);
    }

    #[tokio::test]
    async fn set_enabled_toggles_flag() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        let record = sample_record("test-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        // Disable
        persistence
            .set_enabled("test-plugin", false)
            .await
            .unwrap();
        let plugin = persistence.get_plugin("test-plugin").await.unwrap().unwrap();
        assert!(!plugin.enabled);

        // Re-enable
        persistence
            .set_enabled("test-plugin", true)
            .await
            .unwrap();
        let plugin = persistence.get_plugin("test-plugin").await.unwrap().unwrap();
        assert!(plugin.enabled);
    }

    #[tokio::test]
    async fn remove_stale_plugins_removes_absent_names() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        persistence
            .upsert_plugin(&sample_record("keep-me"))
            .await
            .unwrap();
        persistence
            .upsert_plugin(&sample_record("remove-me"))
            .await
            .unwrap();
        persistence
            .upsert_plugin(&sample_record("also-remove"))
            .await
            .unwrap();

        let deleted = persistence
            .remove_stale_plugins(&["keep-me".to_string()])
            .await
            .unwrap();
        assert_eq!(deleted, 2);

        let plugins = persistence.list_plugins().await.unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "keep-me");
    }

    #[tokio::test]
    async fn get_plugin_returns_none_for_missing() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);
        let result = persistence.get_plugin("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn remove_plugin_deletes_record() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        let record = sample_record("doomed-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        // Confirm it exists
        let found = persistence.get_plugin("doomed-plugin").await.unwrap();
        assert!(found.is_some());

        // Remove it
        let existed = persistence.remove_plugin("doomed-plugin").await.unwrap();
        assert!(existed);

        // Confirm it's gone
        let gone = persistence.get_plugin("doomed-plugin").await.unwrap();
        assert!(gone.is_none());

        // Removing again should return false
        let existed_again = persistence.remove_plugin("doomed-plugin").await.unwrap();
        assert!(!existed_again);
    }

    #[tokio::test]
    async fn plugin_persist_get_settings_empty() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);
        let settings = persistence.get_settings("nonexistent").await.unwrap();
        assert!(settings.is_empty());
    }

    #[tokio::test]
    async fn plugin_persist_upsert_then_get_settings() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        // Need a plugin record first for FK
        let record = sample_record("test-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        persistence
            .upsert_setting("test-plugin", "api_key", "my-secret", true)
            .await
            .unwrap();
        persistence
            .upsert_setting("test-plugin", "base_url", "https://api.example.com", false)
            .await
            .unwrap();

        let settings = persistence.get_settings("test-plugin").await.unwrap();
        assert_eq!(settings.len(), 2);

        // Sorted by key ASC
        assert_eq!(settings[0].0, "api_key");
        assert_eq!(settings[0].1, "my-secret"); // decoded from base64
        assert!(settings[0].2); // is_secret

        assert_eq!(settings[1].0, "base_url");
        assert_eq!(settings[1].1, "https://api.example.com");
        assert!(!settings[1].2);
    }

    #[tokio::test]
    async fn plugin_persist_upsert_updates_existing() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        let record = sample_record("test-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        persistence
            .upsert_setting("test-plugin", "api_key", "old-value", false)
            .await
            .unwrap();
        persistence
            .upsert_setting("test-plugin", "api_key", "new-value", false)
            .await
            .unwrap();

        let settings = persistence.get_settings("test-plugin").await.unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].1, "new-value");
    }

    #[tokio::test]
    async fn plugin_persist_secret_values_are_base64_encoded() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        let record = sample_record("test-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        persistence
            .upsert_setting("test-plugin", "secret_key", "super-secret-123", true)
            .await
            .unwrap();

        // Verify the raw value in DB is base64-encoded
        let raw = persistence
            .history
            .conn
            .call(|conn| {
                let value: String = conn.query_row(
                    "SELECT value FROM plugin_settings WHERE plugin_name = ?1 AND key = ?2",
                    rusqlite::params!["test-plugin", "secret_key"],
                    |row| row.get(0),
                )?;
                Ok(value)
            })
            .await
            .unwrap();

        use base64::Engine;
        let expected_b64 =
            base64::engine::general_purpose::STANDARD.encode("super-secret-123".as_bytes());
        assert_eq!(raw, expected_b64);

        // But get_settings returns decoded value
        let settings = persistence.get_settings("test-plugin").await.unwrap();
        assert_eq!(settings[0].1, "super-secret-123");
    }

    #[tokio::test]
    async fn upsert_updates_existing_record() {
        let history = make_test_history().await;
        let persistence = PluginPersistence::new(history);

        let mut record = sample_record("test-plugin");
        persistence.upsert_plugin(&record).await.unwrap();

        record.version = "2.0.0".to_string();
        record.updated_at = "2026-06-01T00:00:00Z".to_string();
        persistence.upsert_plugin(&record).await.unwrap();

        let plugins = persistence.list_plugins().await.unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].version, "2.0.0");
        assert_eq!(plugins[0].updated_at, "2026-06-01T00:00:00Z");
    }
}
