#![allow(dead_code)]

use super::types::AuthSource;
use amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

const GITHUB_COPILOT_DISABLE_GH_CLI_ENV: &str = "TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredGithubCopilotAuth {
    pub auth_mode: String,
    pub access_token: String,
    pub source: String,
    pub updated_at: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ResolvedGithubCopilotAuth {
    pub access_token: Option<String>,
    pub auth_source: AuthSource,
    pub source: String,
    pub use_logged_in_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GithubCopilotModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
}

#[derive(Debug)]
pub enum GithubCopilotAuthFlowResult {
    AlreadyAvailable,
    ImportedFromGhCli,
    Started,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn env_flag_is_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

pub fn read_stored_github_copilot_auth() -> Option<StoredGithubCopilotAuth> {
    let value = super::provider_auth_store::load_provider_auth_state(
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
    )
    .ok()??;
    serde_json::from_value(value).ok()
}

pub fn write_stored_github_copilot_auth(auth: &StoredGithubCopilotAuth) -> Result<()> {
    super::provider_auth_store::save_provider_auth_state(
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
        &serde_json::to_value(auth)?,
    )
}

pub fn clear_stored_github_copilot_auth() -> Result<()> {
    super::provider_auth_store::delete_provider_auth_state(
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
    )
}

fn stored_from_token(token: String, source: &str) -> StoredGithubCopilotAuth {
    let now = now_millis();
    StoredGithubCopilotAuth {
        auth_mode: "github_copilot".to_string(),
        access_token: token,
        source: source.to_string(),
        updated_at: now,
        created_at: now,
    }
}

fn gh_cli_token() -> Option<String> {
    if env_flag_is_enabled(GITHUB_COPILOT_DISABLE_GH_CLI_ENV) {
        return None;
    }

    let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

pub fn resolve_github_copilot_auth(
    api_key: &str,
    auth_source: AuthSource,
) -> Option<ResolvedGithubCopilotAuth> {
    match auth_source {
        AuthSource::GithubCopilot => {
            let stored = read_stored_github_copilot_auth()?;
            Some(ResolvedGithubCopilotAuth {
                access_token: Some(stored.access_token),
                auth_source: AuthSource::GithubCopilot,
                source: stored.source,
                use_logged_in_user: true,
            })
        }
        AuthSource::ApiKey => {
            let explicit = api_key.trim();
            if !explicit.is_empty() {
                return Some(ResolvedGithubCopilotAuth {
                    access_token: Some(explicit.to_string()),
                    auth_source: AuthSource::ApiKey,
                    source: "api_key".to_string(),
                    use_logged_in_user: false,
                });
            }
            None
        }
        AuthSource::ChatgptSubscription => None,
    }
}

pub fn has_github_copilot_auth(api_key: &str, auth_source: AuthSource) -> bool {
    resolve_github_copilot_auth(api_key, auth_source).is_some()
}

pub fn list_github_copilot_models(
    api_key: &str,
    auth_source: AuthSource,
) -> Result<Vec<GithubCopilotModel>> {
    let _resolved = resolve_github_copilot_auth(api_key, auth_source)
        .context("GitHub Copilot auth is not available")?;
    Ok(super::types::GITHUB_COPILOT_MODELS
        .iter()
        .map(|model| GithubCopilotModel {
            id: model.id.to_string(),
            name: Some(model.name.to_string()),
            context_window: Some(model.context_window),
        })
        .collect())
}

pub fn github_copilot_has_available_models(api_key: &str, auth_source: AuthSource) -> bool {
    list_github_copilot_models(api_key, auth_source)
        .map(|models| !models.is_empty())
        .unwrap_or(false)
}

pub fn begin_github_copilot_auth_flow() -> Result<GithubCopilotAuthFlowResult> {
    if read_stored_github_copilot_auth().is_some() {
        return Ok(GithubCopilotAuthFlowResult::AlreadyAvailable);
    }

    let status = Command::new("gh")
        .args(["auth", "login", "--web", "--scopes", "read:org,models:read"])
        .status()
        .context("failed to start GitHub CLI login flow")?;
    if !status.success() {
        anyhow::bail!("GitHub CLI login flow failed");
    }

    let token = gh_cli_token().context("GitHub CLI login completed but returned no token")?;
    write_stored_github_copilot_auth(&stored_from_token(token, "gh_cli"))?;
    Ok(GithubCopilotAuthFlowResult::Started)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_copilot_mode_ignores_env_tokens() {
        let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
        let root = tempfile::tempdir().expect("temp dir");
        std::env::set_var(
            "TAMUX_PROVIDER_AUTH_DB_PATH",
            root.path().join("provider-auth.db"),
        );
        std::env::set_var("COPILOT_GITHUB_TOKEN", "env-token");
        std::env::set_var("GH_TOKEN", "gh-token");
        std::env::set_var("GITHUB_TOKEN", "github-token");

        let resolved = resolve_github_copilot_auth("", AuthSource::GithubCopilot);

        assert!(resolved.is_none());

        std::env::remove_var("TAMUX_PROVIDER_AUTH_DB_PATH");
        std::env::remove_var("COPILOT_GITHUB_TOKEN");
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
    }

    #[test]
    fn api_key_mode_does_not_fallback_to_stored_browser_auth() {
        let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
        let root = tempfile::tempdir().expect("temp dir");
        std::env::set_var(
            "TAMUX_PROVIDER_AUTH_DB_PATH",
            root.path().join("provider-auth.db"),
        );

        write_stored_github_copilot_auth(&StoredGithubCopilotAuth {
            auth_mode: "github_copilot".to_string(),
            access_token: "stored-browser-token".to_string(),
            source: "test".to_string(),
            updated_at: 1,
            created_at: 1,
        })
        .expect("store browser auth");

        let resolved = resolve_github_copilot_auth("", AuthSource::ApiKey);

        assert!(resolved.is_none());

        std::env::remove_var("TAMUX_PROVIDER_AUTH_DB_PATH");
    }
}
