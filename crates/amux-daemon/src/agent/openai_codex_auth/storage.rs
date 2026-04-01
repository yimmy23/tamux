use anyhow::{Context, Result};
use std::path::PathBuf;

use super::{
    auth_runtime, now_millis, tombstone_auth_mode, CodexCliAuthFile, OpenAICodexAuthRuntime,
    OpenAICodexAuthTombstone, StoredOpenAICodexAuth, OPENAI_AUTH_MODE, OPENAI_PROVIDER_ID,
};
use crate::agent::provider_auth_store;

pub(super) fn codex_cli_auth_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("TAMUX_CODEX_CLI_AUTH_PATH") {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".codex").join("auth.json"))
}

fn load_tombstone() -> Option<OpenAICodexAuthTombstone> {
    let value =
        provider_auth_store::load_provider_auth_state(OPENAI_PROVIDER_ID, tombstone_auth_mode())
            .ok()??;
    serde_json::from_value(value).ok()
}

pub(super) fn save_tombstone() -> Result<()> {
    let tombstone = OpenAICodexAuthTombstone {
        provider: "openai-codex".to_string(),
        auth_mode: OPENAI_AUTH_MODE.to_string(),
        tombstoned_at: now_millis() as i64,
        source: "tamux-daemon".to_string(),
    };
    provider_auth_store::save_provider_auth_state(
        OPENAI_PROVIDER_ID,
        tombstone_auth_mode(),
        &serde_json::to_value(tombstone)?,
    )
}

pub(super) fn clear_tombstone() -> Result<()> {
    provider_auth_store::delete_provider_auth_state(OPENAI_PROVIDER_ID, tombstone_auth_mode())
}

pub(crate) fn read_stored_openai_codex_auth() -> Option<StoredOpenAICodexAuth> {
    let value = provider_auth_store::load_provider_auth_state(OPENAI_PROVIDER_ID, OPENAI_AUTH_MODE)
        .ok()??;
    let parsed: StoredOpenAICodexAuth = serde_json::from_value(value).ok()?;
    if parsed.access_token.trim().is_empty() || parsed.refresh_token.trim().is_empty() {
        return None;
    }
    Some(parsed)
}

pub(crate) fn write_stored_openai_codex_auth(auth: &StoredOpenAICodexAuth) -> Result<()> {
    provider_auth_store::save_provider_auth_state(
        OPENAI_PROVIDER_ID,
        OPENAI_AUTH_MODE,
        &serde_json::to_value(auth)?,
    )
}

pub(super) fn persist_stored_openai_codex_auth(
    auth: &StoredOpenAICodexAuth,
) -> Result<StoredOpenAICodexAuth> {
    write_stored_openai_codex_auth(auth).context("failed to persist OpenAI Codex auth")?;
    read_stored_openai_codex_auth().context("persisted OpenAI Codex auth missing after save")
}

pub(crate) fn import_codex_cli_auth_if_present() -> Result<Option<StoredOpenAICodexAuth>> {
    if let Some(existing) = read_stored_openai_codex_auth() {
        return Ok(Some(existing));
    }
    if load_tombstone().is_some() {
        return Ok(None);
    }

    let Some(path) = codex_cli_auth_path() else {
        return Ok(None);
    };
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return Ok(None),
    };
    let parsed: CodexCliAuthFile = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    let Some(tokens) = parsed.tokens else {
        return Ok(None);
    };
    let Some(access_token) = tokens.access_token else {
        return Ok(None);
    };
    let Some(refresh_token) = tokens.refresh_token else {
        return Ok(None);
    };
    let Some(account_id) = super::extract_openai_codex_account_id(&access_token) else {
        return Ok(None);
    };
    let Some(expires_at) = super::extract_jwt_expiry(&access_token) else {
        return Ok(None);
    };

    let now = now_millis() as i64;
    let imported = StoredOpenAICodexAuth {
        provider: Some("openai-codex".to_string()),
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        access_token,
        refresh_token,
        account_id: Some(account_id),
        expires_at: Some(expires_at),
        source: Some("codex_import".to_string()),
        updated_at: Some(now),
        created_at: Some(now),
    };

    Ok(Some(persist_stored_openai_codex_auth(&imported)?))
}

#[cfg(test)]
pub(crate) fn reset_openai_codex_auth_runtime_for_tests() {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *runtime = OpenAICodexAuthRuntime::default();
}

#[cfg(test)]
pub(crate) fn clear_openai_codex_auth_test_state() {
    reset_openai_codex_auth_runtime_for_tests();
    let _ =
        provider_auth_store::delete_provider_auth_state(OPENAI_PROVIDER_ID, tombstone_auth_mode());
    let _ = provider_auth_store::delete_provider_auth_state(OPENAI_PROVIDER_ID, OPENAI_AUTH_MODE);
}

#[cfg(test)]
pub(crate) fn tombstone_present_for_tests() -> bool {
    load_tombstone().is_some()
}
