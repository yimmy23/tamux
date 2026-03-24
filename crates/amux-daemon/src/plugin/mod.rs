pub mod loader;
pub mod manifest;
pub mod persistence;
pub mod schema;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

pub use loader::LoadedPlugin;
pub use persistence::{PluginPersistence, PluginRecord};

/// Manages plugin lifecycle: loading, validation, persistence, and queries.
/// Initialized once in server.rs, shared via Arc.
pub struct PluginManager {
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
    persistence: PluginPersistence,
    plugins_dir: PathBuf,
    schema_validator: jsonschema::Validator,
}

impl PluginManager {
    /// Create a new PluginManager. Does NOT load plugins yet -- call load_all_from_disk().
    pub fn new(history: Arc<crate::history::HistoryStore>, plugins_dir: PathBuf) -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            persistence: PluginPersistence::new(history),
            plugins_dir,
            schema_validator: schema::compile_schema_v1(),
        }
    }

    /// Load all plugins from disk, validate, persist to SQLite, reconcile stale records.
    /// Per D-09: skip and warn on failures.
    /// Returns (loaded_count, skipped_count).
    pub async fn load_all_from_disk(&self) -> (usize, usize) {
        // Create plugins dir if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&self.plugins_dir) {
            tracing::warn!(
                path = %self.plugins_dir.display(),
                error = %e,
                "failed to create plugins directory"
            );
            return (0, 0);
        }

        let scan = loader::scan_plugins_dir(&self.plugins_dir, &self.schema_validator);
        let loaded_count = scan.loaded.len();
        let skipped_count = scan.skipped.len();

        let now = chrono::Utc::now().to_rfc3339();
        let mut active_names = Vec::with_capacity(loaded_count);
        let mut plugins_map = HashMap::with_capacity(loaded_count);

        for plugin in scan.loaded {
            let record = PluginRecord {
                name: plugin.manifest.name.clone(),
                version: plugin.manifest.version.clone(),
                description: plugin.manifest.description.clone(),
                author: plugin.manifest.author.clone(),
                manifest_json: plugin.manifest_json.clone(),
                install_source: "local".to_string(),
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
            };

            if let Err(e) = self.persistence.upsert_plugin(&record).await {
                tracing::warn!(
                    plugin = %record.name,
                    error = %e,
                    "failed to persist plugin record"
                );
                continue;
            }

            active_names.push(plugin.manifest.name.clone());
            plugins_map.insert(plugin.manifest.name.clone(), plugin);
        }

        // Reconcile stale records (Pitfall 6)
        if let Err(e) = self.persistence.remove_stale_plugins(&active_names).await {
            tracing::warn!(error = %e, "failed to reconcile stale plugin records");
        }

        *self.plugins.write().await = plugins_map;

        tracing::info!(
            loaded = loaded_count,
            skipped = skipped_count,
            "plugin loader: loaded {} plugins ({} skipped)",
            loaded_count,
            skipped_count
        );

        (loaded_count, skipped_count)
    }

    /// List all plugins (from SQLite for accurate enabled state).
    pub async fn list_plugins(&self) -> Vec<amux_protocol::PluginInfo> {
        let plugins = self.plugins.read().await;
        let records = match self.persistence.list_plugins().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list plugins from database");
                return Vec::new();
            }
        };

        records
            .iter()
            .map(|rec| {
                let loaded = plugins.get(&rec.name);
                to_plugin_info_from_record(rec, loaded)
            })
            .collect()
    }

    /// Get a single plugin by name.
    /// Returns (PluginInfo, settings_schema_json).
    pub async fn get_plugin(
        &self,
        name: &str,
    ) -> Option<(amux_protocol::PluginInfo, Option<String>)> {
        let record = match self.persistence.get_plugin(name).await {
            Ok(Some(r)) => r,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(plugin = %name, error = %e, "failed to get plugin from database");
                return None;
            }
        };

        let plugins = self.plugins.read().await;
        let loaded = plugins.get(name);
        let info = to_plugin_info_from_record(&record, loaded);

        // Extract settings schema from manifest JSON for dynamic form rendering
        let settings_schema = extract_settings_schema(&record.manifest_json);

        Some((info, settings_schema))
    }

    /// Enable or disable a plugin.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        self.persistence.set_enabled(name, enabled).await
    }

    /// Check if a manifest's commands or skills conflict with existing plugins.
    /// Returns Ok(()) if no conflicts, Err with conflict details if any found.
    /// Namespace convention: commands are namespaced as /pluginname.command per PSKL-05.
    /// Conflicts happen when two different plugins declare the same command key or skill path.
    pub async fn check_conflicts(
        &self,
        new_manifest: &manifest::PluginManifest,
    ) -> Result<()> {
        let plugins = self.plugins.read().await;
        let mut conflicts: Vec<String> = Vec::new();

        for (existing_name, existing) in plugins.iter() {
            if existing_name == &new_manifest.name {
                continue; // Same plugin (re-install) is not a conflict
            }

            // Check command name conflicts
            if let (Some(new_cmds), Some(existing_cmds)) =
                (&new_manifest.commands, &existing.manifest.commands)
            {
                for cmd_name in new_cmds.keys() {
                    if existing_cmds.contains_key(cmd_name.as_str()) {
                        conflicts.push(format!(
                            "command '{}' conflicts with plugin '{}'",
                            cmd_name, existing_name
                        ));
                    }
                }
            }

            // Check skill path conflicts
            if let (Some(new_skills), Some(existing_skills)) =
                (&new_manifest.skills, &existing.manifest.skills)
            {
                for skill_path in new_skills {
                    if existing_skills.contains(skill_path) {
                        conflicts.push(format!(
                            "skill '{}' conflicts with plugin '{}'",
                            skill_path, existing_name
                        ));
                    }
                }
            }
        }

        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Name conflicts detected:\n{}",
                conflicts.join("\n")
            ))
        }
    }

    /// Register a plugin from its directory. Validates manifest, checks conflicts,
    /// persists to SQLite, and adds to in-memory map.
    /// Called by the PluginInstall IPC handler after CLI has copied files to disk.
    pub async fn register_plugin(
        &self,
        dir_name: &str,
        install_source: &str,
    ) -> Result<amux_protocol::PluginInfo> {
        let manifest_path = self.plugins_dir.join(dir_name).join("plugin.json");
        let raw_bytes = std::fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;

        let (manifest, manifest_json) =
            loader::validate_manifest(&raw_bytes, &self.schema_validator)?;

        // Check for command/skill conflicts (INST-07)
        self.check_conflicts(&manifest).await?;

        let now = chrono::Utc::now().to_rfc3339();
        let record = PluginRecord {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            manifest_json: manifest_json.clone(),
            install_source: install_source.to_string(),
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
        };

        self.persistence.upsert_plugin(&record).await?;

        let info = to_plugin_info_from_record(
            &record,
            Some(&loader::LoadedPlugin {
                manifest: manifest.clone(),
                manifest_json,
                dir_name: dir_name.to_string(),
            }),
        );

        // Add to in-memory map
        self.plugins.write().await.insert(
            manifest.name.clone(),
            loader::LoadedPlugin {
                manifest,
                manifest_json: record.manifest_json,
                dir_name: dir_name.to_string(),
            },
        );

        tracing::info!(plugin = %record.name, source = %install_source, "plugin registered");
        Ok(info)
    }

    /// Unregister a plugin: remove from SQLite (plugins + settings + credentials)
    /// and from in-memory map. Does NOT delete files from disk (CLI handles that).
    pub async fn unregister_plugin(&self, name: &str) -> Result<()> {
        let existed = self.persistence.remove_plugin(name).await?;
        if !existed {
            return Err(anyhow::anyhow!("plugin '{}' not found", name));
        }
        self.plugins.write().await.remove(name);
        tracing::info!(plugin = %name, "plugin unregistered");
        Ok(())
    }

    /// Get settings for a plugin. Masks secret values for display. Per PSET-04/PSET-06.
    pub async fn get_settings(&self, name: &str) -> Vec<(String, String, bool)> {
        match self.persistence.get_settings(name).await {
            Ok(settings) => settings
                .into_iter()
                .map(|(k, v, secret)| {
                    if secret {
                        (k, "********".to_string(), true)
                    } else {
                        (k, v, false)
                    }
                })
                .collect(),
            Err(e) => {
                tracing::warn!(plugin = %name, error = %e, "failed to get plugin settings");
                Vec::new()
            }
        }
    }

    /// Update a single setting value. Per PSET-06/D-06.
    pub async fn update_setting(
        &self,
        plugin_name: &str,
        key: &str,
        value: &str,
        is_secret: bool,
    ) -> Result<()> {
        self.persistence
            .upsert_setting(plugin_name, key, value, is_secret)
            .await
    }

    /// Test connectivity by making a HEAD request to the plugin's first API endpoint.
    /// Per PSET-05/D-10. Returns (success, message).
    pub async fn test_connection(&self, name: &str) -> (bool, String) {
        let plugins = self.plugins.read().await;
        let Some(plugin) = plugins.get(name) else {
            return (false, format!("Plugin '{}' not found", name));
        };
        let Some(api) = &plugin.manifest.api else {
            return (false, "Plugin has no API section".to_string());
        };
        let base_url = match &api.base_url {
            Some(url) => url.clone(),
            None => return (false, "Plugin has no base_url".to_string()),
        };
        // Use first endpoint or just probe base_url
        let test_url = if let Some((_name, endpoint)) = api.endpoints.iter().next() {
            format!("{}{}", base_url.trim_end_matches('/'), endpoint.path)
        } else {
            base_url
        };
        // Make a lightweight HTTP probe with 5s timeout
        match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(client) => match client.head(&test_url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() || status == 401 || status == 403 {
                        // 401/403 means server is reachable but auth needed -- that's OK for connectivity test
                        (true, "Connection successful".to_string())
                    } else {
                        (false, format!("Server returned {}", status))
                    }
                }
                Err(e) => (false, format!("Connection failed: {}", e)),
            },
            Err(e) => (false, format!("HTTP client error: {}", e)),
        }
    }

    /// Get the plugins directory path.
    pub fn plugins_dir(&self) -> &std::path::Path {
        &self.plugins_dir
    }
}

/// Convert a PluginRecord + optional LoadedPlugin to PluginInfo.
fn to_plugin_info_from_record(
    record: &PluginRecord,
    loaded: Option<&LoadedPlugin>,
) -> amux_protocol::PluginInfo {
    if let Some(plugin) = loaded {
        to_plugin_info(
            plugin,
            record.enabled,
            &record.install_source,
            &record.installed_at,
            &record.updated_at,
        )
    } else {
        // Fallback: reconstruct from manifest_json in record
        amux_protocol::PluginInfo {
            name: record.name.clone(),
            version: record.version.clone(),
            description: record.description.clone(),
            author: record.author.clone(),
            enabled: record.enabled,
            install_source: record.install_source.clone(),
            has_api: false,
            has_auth: false,
            has_commands: false,
            has_skills: false,
            endpoint_count: 0,
            settings_count: 0,
            installed_at: record.installed_at.clone(),
            updated_at: record.updated_at.clone(),
        }
    }
}

fn to_plugin_info(
    plugin: &LoadedPlugin,
    enabled: bool,
    install_source: &str,
    installed_at: &str,
    updated_at: &str,
) -> amux_protocol::PluginInfo {
    amux_protocol::PluginInfo {
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        description: plugin.manifest.description.clone(),
        author: plugin.manifest.author.clone(),
        enabled,
        install_source: install_source.to_string(),
        has_api: plugin.manifest.api.is_some(),
        has_auth: plugin.manifest.auth.is_some(),
        has_commands: plugin.manifest.commands.is_some(),
        has_skills: plugin.manifest.skills.is_some(),
        endpoint_count: plugin
            .manifest
            .api
            .as_ref()
            .map(|a| a.endpoints.len() as u32)
            .unwrap_or(0),
        settings_count: plugin
            .manifest
            .settings
            .as_ref()
            .map(|s| s.len() as u32)
            .unwrap_or(0),
        installed_at: installed_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

/// Extract the "settings" section from manifest JSON as a standalone JSON string.
fn extract_settings_schema(manifest_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(manifest_json).ok()?;
    let settings = value.get("settings")?;
    Some(serde_json::to_string(settings).ok()?)
}
