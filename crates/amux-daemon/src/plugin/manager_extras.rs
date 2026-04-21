use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PluginAuthStatus {
    NotConfigured,
    Connected,
    ExpiringSoon,
    Refreshable,
    NeedsReconnect,
}

impl PluginAuthStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Connected => "connected",
            Self::ExpiringSoon => "expiring_soon",
            Self::Refreshable => "refreshable",
            Self::NeedsReconnect => "needs_reconnect",
        }
    }
}

const PLUGIN_AUTH_EXPIRING_SOON_SECS: i64 = 15 * 60;

pub(crate) fn auth_status_from_expiry_and_refresh_token(
    expires_at: Option<&str>,
    has_refresh_token: bool,
) -> PluginAuthStatus {
    match expires_at {
        None => PluginAuthStatus::Connected,
        Some(expires_at) => match chrono::DateTime::parse_from_rfc3339(expires_at) {
            Ok(dt) => {
                let expires_at = dt.with_timezone(&chrono::Utc);
                let now = chrono::Utc::now();
                if expires_at <= now {
                    if has_refresh_token {
                        PluginAuthStatus::Refreshable
                    } else {
                        PluginAuthStatus::NeedsReconnect
                    }
                } else if !has_refresh_token
                    && expires_at <= now + chrono::Duration::seconds(PLUGIN_AUTH_EXPIRING_SOON_SECS)
                {
                    PluginAuthStatus::ExpiringSoon
                } else {
                    PluginAuthStatus::Connected
                }
            }
            Err(_) => PluginAuthStatus::Connected,
        },
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PluginAuthHealthIssue {
    pub plugin_name: String,
    pub status: PluginAuthStatus,
    pub message: String,
    pub auto_action_attempted: bool,
}

impl PluginManager {
    pub(super) async fn check_plugin_enabled(
        &self,
        name: &str,
    ) -> Result<bool, api_proxy::PluginApiError> {
        match self.persistence.get_plugin(name).await {
            Ok(Some(record)) => Ok(record.enabled),
            Ok(None) => Err(api_proxy::PluginApiError::PluginNotFound {
                name: name.to_string(),
            }),
            Err(e) => {
                tracing::warn!(plugin = %name, error = %e, "failed to check plugin enabled state");
                Ok(true)
            }
        }
    }

    pub fn plugins_dir(&self) -> &std::path::Path {
        &self.plugins_dir
    }

    pub(crate) async fn resolve_command(
        &self,
        input: &str,
    ) -> Option<commands::PluginCommandEntry> {
        let registry = self.command_registry.read().await;
        registry.resolve(input).cloned()
    }

    pub async fn list_commands(&self) -> Vec<amux_protocol::PluginCommandInfo> {
        let registry = self.command_registry.read().await;
        registry
            .list_all()
            .into_iter()
            .map(|e| amux_protocol::PluginCommandInfo {
                command: e.command_key.clone(),
                plugin_name: e.plugin_name.clone(),
                description: e.description.clone(),
                api_endpoint: e.api_endpoint.clone(),
            })
            .collect()
    }

    pub(super) async fn rebuild_command_registry(&self) {
        let plugins = self.plugins.read().await;
        let mut registry = self.command_registry.write().await;
        registry.rebuild_from_plugins(&plugins, &self.plugins_dir);
    }

    pub(crate) async fn monitor_auth_health(&self) -> Vec<PluginAuthHealthIssue> {
        let plugins = self.plugins.read().await;
        let oauth_plugins: Vec<(String, manifest::AuthSection)> = plugins
            .iter()
            .filter_map(|(name, plugin)| {
                plugin
                    .manifest
                    .auth
                    .as_ref()
                    .filter(|auth| auth.auth_type == "oauth2")
                    .map(|auth| (name.clone(), auth.clone()))
            })
            .collect();
        drop(plugins);

        let data_dir = self.plugins_dir.parent().unwrap_or(Path::new("."));
        let encryption_key = match crypto::load_or_create_key(data_dir) {
            Ok(key) => Some(key),
            Err(e) => {
                tracing::warn!(error = %e, "failed to load plugin auth encryption key");
                None
            }
        };

        let mut issues = Vec::new();
        for (plugin_name, manifest_auth) in oauth_plugins {
            let access_cred = match self
                .persistence
                .get_credential(&plugin_name, "access_token")
                .await
            {
                Ok(value) => value,
                Err(e) => {
                    tracing::warn!(plugin = %plugin_name, error = %e, "failed to read access token");
                    continue;
                }
            };
            let Some((_, expires_at)) = access_cred else {
                continue;
            };

            let has_refresh_token = match self
                .persistence
                .get_credential(&plugin_name, "refresh_token")
                .await
            {
                Ok(value) => value.is_some(),
                Err(e) => {
                    tracing::warn!(plugin = %plugin_name, error = %e, "failed to read refresh token");
                    false
                }
            };

            match auth_status_from_expiry_and_refresh_token(expires_at.as_deref(), has_refresh_token)
            {
                PluginAuthStatus::Connected | PluginAuthStatus::NotConfigured => {}
                PluginAuthStatus::ExpiringSoon => issues.push(PluginAuthHealthIssue {
                    plugin_name: plugin_name.clone(),
                    status: PluginAuthStatus::ExpiringSoon,
                    message: "Access token expires soon and no refresh token is stored. Reconnect this plugin before it expires.".to_string(),
                    auto_action_attempted: false,
                }),
                PluginAuthStatus::NeedsReconnect => issues.push(PluginAuthHealthIssue {
                    plugin_name: plugin_name.clone(),
                    status: PluginAuthStatus::NeedsReconnect,
                    message: "Access token expired and no refresh token is stored. Reconnect this plugin.".to_string(),
                    auto_action_attempted: false,
                }),
                PluginAuthStatus::Refreshable => {
                    let Some(key) = encryption_key.as_ref() else {
                        issues.push(PluginAuthHealthIssue {
                            plugin_name: plugin_name.clone(),
                            status: PluginAuthStatus::Refreshable,
                            message: "Access token expired, but the daemon could not load the encryption key to auto-refresh it. Reconnect this plugin.".to_string(),
                            auto_action_attempted: true,
                        });
                        continue;
                    };

                    let settings = self
                        .persistence
                        .get_settings(&plugin_name)
                        .await
                        .unwrap_or_default();
                    let manifest_auth = Some(manifest_auth.clone());
                    let lock = self.get_refresh_lock(&plugin_name).await;
                    let _guard = lock.lock().await;

                    let access_cred = self
                        .persistence
                        .get_credential(&plugin_name, "access_token")
                        .await
                        .ok()
                        .flatten();
                    let Some((_, expires_at)) = access_cred else {
                        continue;
                    };
                    let has_refresh_token = self
                        .persistence
                        .get_credential(&plugin_name, "refresh_token")
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                    if auth_status_from_expiry_and_refresh_token(
                        expires_at.as_deref(),
                        has_refresh_token,
                    ) != PluginAuthStatus::Refreshable
                    {
                        continue;
                    }

                    if let Err(e) = self
                        .try_refresh_token(&plugin_name, &manifest_auth, &settings, key)
                        .await
                    {
                        tracing::warn!(
                            plugin = %plugin_name,
                            error = %e,
                            "plugin auth maintenance auto-refresh failed"
                        );
                        issues.push(PluginAuthHealthIssue {
                            plugin_name: plugin_name.clone(),
                            status: PluginAuthStatus::Refreshable,
                            message: format!(
                                "Access token expired and auto-refresh failed: {e}. Monitoring will keep retrying; reconnect if this persists."
                            ),
                            auto_action_attempted: true,
                        });
                    }
                }
            }
        }

        issues
    }

    pub async fn start_oauth_flow_for_plugin(
        &self,
        plugin_name: &str,
    ) -> Result<oauth2::OAuthFlowState> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_name)
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' not found", plugin_name))?;

        let auth = plugin
            .manifest
            .auth
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' has no auth section", plugin_name))?;

        if auth.auth_type != "oauth2" {
            anyhow::bail!(
                "plugin '{}' auth type is '{}', not 'oauth2'",
                plugin_name,
                auth.auth_type
            );
        }

        let authorization_url = auth
            .authorization_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' missing authorization_url", plugin_name))?
            .clone();
        let token_url = auth
            .token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' missing token_url", plugin_name))?
            .clone();
        let scopes = auth.scopes.clone().unwrap_or_default();
        let pkce = auth.pkce;
        drop(plugins);

        let settings = self
            .persistence
            .get_settings(plugin_name)
            .await
            .unwrap_or_default();
        let client_id = settings
            .iter()
            .find(|(k, _, _)| k == "client_id")
            .map(|(_, v, _)| v.clone())
            .unwrap_or_default();
        let client_secret = settings
            .iter()
            .find(|(k, _, _)| k == "client_secret")
            .map(|(_, v, _)| v.clone());

        if client_id.is_empty() {
            anyhow::bail!(
                "plugin '{}' requires 'client_id' in settings for OAuth2",
                plugin_name
            );
        }

        let config = oauth2::OAuthFlowConfig {
            client_id,
            client_secret,
            authorization_url,
            token_url,
            scopes,
            pkce,
        };

        oauth2::start_oauth_flow(&config).await
    }

    pub async fn complete_oauth_flow(
        &self,
        plugin_name: &str,
        state: &mut oauth2::OAuthFlowState,
    ) -> Result<()> {
        let code = oauth2::await_callback(state).await?;
        let result = oauth2::exchange_code(state, &code).await?;
        let data_dir = self.plugins_dir.parent().unwrap_or(Path::new("."));
        let key = crypto::load_or_create_key(data_dir)?;

        let encrypted_access = crypto::encrypt(&key, result.access_token.as_bytes())?;
        let expires_at = result
            .expires_in
            .map(|secs| (chrono::Utc::now() + chrono::Duration::seconds(secs as i64)).to_rfc3339());
        self.persistence
            .upsert_credential(
                plugin_name,
                "access_token",
                &encrypted_access,
                expires_at.as_deref(),
            )
            .await?;

        if let Some(ref rt) = result.refresh_token {
            let encrypted_refresh = crypto::encrypt(&key, rt.as_bytes())?;
            self.persistence
                .upsert_credential(plugin_name, "refresh_token", &encrypted_refresh, None)
                .await?;
        }

        tracing::info!(plugin = %plugin_name, "OAuth2 flow completed, tokens stored");
        Ok(())
    }

    async fn get_refresh_lock(&self, plugin_name: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.oauth_refresh_locks.lock().await;
        locks
            .entry(plugin_name.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    pub(super) async fn get_oauth_context_with_refresh(
        &self,
        plugin_name: &str,
        manifest_auth: &Option<manifest::AuthSection>,
        settings: &[(String, String, bool)],
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, api_proxy::PluginApiError> {
        let data_dir = self.plugins_dir.parent().unwrap_or(Path::new("."));
        let key = crypto::load_or_create_key(data_dir).map_err(|e| {
            tracing::warn!(plugin = %plugin_name, error = %e, "failed to load encryption key");
            api_proxy::PluginApiError::AuthExpired {
                plugin: plugin_name.to_string(),
            }
        })?;

        let cred = self
            .persistence
            .get_credential(plugin_name, "access_token")
            .await
            .map_err(|e| {
                tracing::warn!(plugin = %plugin_name, error = %e, "failed to get credential");
                api_proxy::PluginApiError::AuthExpired {
                    plugin: plugin_name.to_string(),
                }
            })?;

        let (_, expires_at_str) = match cred {
            Some(c) => c,
            None => {
                return Err(api_proxy::PluginApiError::AuthExpired {
                    plugin: plugin_name.to_string(),
                });
            }
        };

        let needs_refresh = if let Some(ref expires_at) = expires_at_str {
            match chrono::DateTime::parse_from_rfc3339(expires_at) {
                Ok(expiry) => {
                    let expiry_utc = expiry.with_timezone(&chrono::Utc);
                    let now = chrono::Utc::now();
                    if expiry_utc <= now {
                        true
                    } else {
                        (expiry_utc - now).num_seconds() < 60
                    }
                }
                Err(_) => false,
            }
        } else {
            false
        };

        if needs_refresh {
            let lock = self.get_refresh_lock(plugin_name).await;
            let _guard = lock.lock().await;

            let rechecked = self
                .persistence
                .get_credential(plugin_name, "access_token")
                .await
                .ok()
                .flatten();

            let still_needs_refresh = if let Some((_, Some(ref ea))) = rechecked {
                match chrono::DateTime::parse_from_rfc3339(ea) {
                    Ok(expiry) => {
                        (expiry.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds() < 60
                    }
                    Err(_) => false,
                }
            } else {
                rechecked.is_none()
            };

            if still_needs_refresh {
                if let Err(e) = self
                    .try_refresh_token(plugin_name, manifest_auth, settings, &key)
                    .await
                {
                    tracing::warn!(plugin = %plugin_name, error = %e, "OAuth token refresh failed");
                    return Err(api_proxy::PluginApiError::AuthExpired {
                        plugin: plugin_name.to_string(),
                    });
                }
            }
        }

        let final_cred = self
            .persistence
            .get_credential(plugin_name, "access_token")
            .await
            .map_err(|_| api_proxy::PluginApiError::AuthExpired {
                plugin: plugin_name.to_string(),
            })?;

        let (final_blob, _) = match final_cred {
            Some(c) => c,
            None => {
                return Err(api_proxy::PluginApiError::AuthExpired {
                    plugin: plugin_name.to_string(),
                });
            }
        };

        let access_token = String::from_utf8(crypto::decrypt(&key, &final_blob).map_err(|_| {
            api_proxy::PluginApiError::AuthExpired {
                plugin: plugin_name.to_string(),
            }
        })?)
        .map_err(|_| api_proxy::PluginApiError::AuthExpired {
            plugin: plugin_name.to_string(),
        })?;

        let mut auth_map = serde_json::Map::new();
        auth_map.insert(
            "access_token".to_string(),
            serde_json::Value::String(access_token),
        );

        if let Ok(Some((rt_blob, _))) = self
            .persistence
            .get_credential(plugin_name, "refresh_token")
            .await
        {
            if let Ok(rt_bytes) = crypto::decrypt(&key, &rt_blob) {
                if let Ok(rt_str) = String::from_utf8(rt_bytes) {
                    auth_map.insert(
                        "refresh_token".to_string(),
                        serde_json::Value::String(rt_str),
                    );
                }
            }
        }

        Ok(Some(auth_map))
    }

    async fn try_refresh_token(
        &self,
        plugin_name: &str,
        manifest_auth: &Option<manifest::AuthSection>,
        settings: &[(String, String, bool)],
        key: &[u8; 32],
    ) -> Result<()> {
        let rt_cred = self
            .persistence
            .get_credential(plugin_name, "refresh_token")
            .await?;

        let (rt_blob, _) = rt_cred
            .ok_or_else(|| anyhow::anyhow!("no refresh token stored for '{}'", plugin_name))?;

        let refresh_token_str = String::from_utf8(crypto::decrypt(key, &rt_blob)?)?;
        let auth = manifest_auth
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no auth section for '{}'", plugin_name))?;
        let token_url = auth
            .token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no token_url for '{}'", plugin_name))?;

        let client_id = settings
            .iter()
            .find(|(k, _, _)| k == "client_id")
            .map(|(_, v, _)| v.clone())
            .unwrap_or_default();
        let client_secret = settings
            .iter()
            .find(|(k, _, _)| k == "client_secret")
            .map(|(_, v, _)| v.clone());

        let config = oauth2::OAuthFlowConfig {
            client_id,
            client_secret,
            authorization_url: auth.authorization_url.clone().unwrap_or_default(),
            token_url: token_url.clone(),
            scopes: auth.scopes.clone().unwrap_or_default(),
            pkce: auth.pkce,
        };

        let result = oauth2::refresh_access_token(&config, &refresh_token_str).await?;
        let encrypted_access = crypto::encrypt(key, result.access_token.as_bytes())?;
        let expires_at = result
            .expires_in
            .map(|secs| (chrono::Utc::now() + chrono::Duration::seconds(secs as i64)).to_rfc3339());
        self.persistence
            .upsert_credential(
                plugin_name,
                "access_token",
                &encrypted_access,
                expires_at.as_deref(),
            )
            .await?;

        if let Some(ref new_rt) = result.refresh_token {
            let encrypted_refresh = crypto::encrypt(key, new_rt.as_bytes())?;
            self.persistence
                .upsert_credential(plugin_name, "refresh_token", &encrypted_refresh, None)
                .await?;
        }

        tracing::info!(plugin = %plugin_name, "OAuth token refreshed successfully");
        Ok(())
    }
}

pub(super) fn to_plugin_info_from_record(
    record: &PluginRecord,
    loaded: Option<&LoadedPlugin>,
    auth_status: String,
) -> amux_protocol::PluginInfo {
    if let Some(plugin) = loaded {
        to_plugin_info(
            plugin,
            record.enabled,
            &record.install_source,
            &record.installed_at,
            &record.updated_at,
            auth_status,
        )
    } else {
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
            auth_status,
        }
    }
}

fn to_plugin_info(
    plugin: &LoadedPlugin,
    enabled: bool,
    install_source: &str,
    installed_at: &str,
    updated_at: &str,
    auth_status: String,
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
        auth_status,
    }
}

pub(super) fn extract_settings_schema(manifest_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(manifest_json).ok()?;
    let settings = value.get("settings")?;
    Some(serde_json::to_string(settings).ok()?)
}
