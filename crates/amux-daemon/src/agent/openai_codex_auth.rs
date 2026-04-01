use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

use super::llm_client::{
    extract_jwt_expiry, extract_openai_codex_account_id, CodexCliAuthFile, StoredOpenAICodexAuth,
};
use super::task_prompt::now_millis;

mod flow;
mod storage;

const OPENAI_CODEX_AUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_AUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const OPENAI_CODEX_AUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CODEX_AUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const OPENAI_CODEX_AUTH_SCOPE: &str = "openid profile email offline_access";
const OPENAI_PROVIDER_ID: &str = "openai";
const OPENAI_AUTH_MODE: &str = "chatgpt_subscription";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenAICodexAuthStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct OpenAICodexAuthTombstone {
    provider: String,
    auth_mode: String,
    tombstoned_at: i64,
    source: String,
}

#[derive(Debug, Default)]
struct OpenAICodexAuthRuntime {
    pending: Option<PendingOpenAICodexAuth>,
    last_error: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingOpenAICodexAuth {
    flow_id: String,
    auth_url: String,
    verifier: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct OpenAICodexTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

pub(crate) trait OpenAICodexExchange: Send + Sync {
    fn exchange_authorization_code(
        &self,
        code: &str,
        verifier: &str,
    ) -> Result<StoredOpenAICodexAuth>;
}

struct ReqwestOpenAICodexExchange;

impl ReqwestOpenAICodexExchange {
    fn new() -> Self {
        Self
    }
}

impl OpenAICodexExchange for ReqwestOpenAICodexExchange {
    fn exchange_authorization_code(
        &self,
        code: &str,
        verifier: &str,
    ) -> Result<StoredOpenAICodexAuth> {
        flow::exchange_authorization_code(code, verifier)
    }
}

fn auth_runtime() -> &'static Mutex<OpenAICodexAuthRuntime> {
    static RUNTIME: OnceLock<Mutex<OpenAICodexAuthRuntime>> = OnceLock::new();
    RUNTIME.get_or_init(|| Mutex::new(OpenAICodexAuthRuntime::default()))
}

fn exchange_client() -> &'static dyn OpenAICodexExchange {
    static EXCHANGE: OnceLock<ReqwestOpenAICodexExchange> = OnceLock::new();
    EXCHANGE.get_or_init(ReqwestOpenAICodexExchange::new)
}

fn tombstone_auth_mode() -> &'static str {
    "chatgpt_subscription_tombstone"
}

fn metadata_from_auth(auth: &StoredOpenAICodexAuth) -> OpenAICodexAuthStatus {
    OpenAICodexAuthStatus {
        available: true,
        auth_mode: auth
            .auth_mode
            .clone()
            .or_else(|| Some(OPENAI_AUTH_MODE.to_string())),
        account_id: auth.account_id.clone(),
        expires_at: auth.expires_at,
        source: auth.source.clone(),
        error: None,
        auth_url: None,
        status: Some("completed".to_string()),
    }
}

fn metadata_available(status: &OpenAICodexAuthStatus) -> bool {
    status.available
}

fn pending_status(pending: &PendingOpenAICodexAuth) -> OpenAICodexAuthStatus {
    OpenAICodexAuthStatus {
        available: false,
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        account_id: None,
        expires_at: None,
        source: Some("tamux-daemon".to_string()),
        error: None,
        auth_url: Some(pending.auth_url.clone()),
        status: Some("pending".to_string()),
    }
}

fn error_status(message: String) -> OpenAICodexAuthStatus {
    OpenAICodexAuthStatus {
        available: false,
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        account_id: None,
        expires_at: None,
        source: Some("tamux-daemon".to_string()),
        error: Some(message),
        auth_url: None,
        status: Some("error".to_string()),
    }
}

fn empty_status() -> OpenAICodexAuthStatus {
    OpenAICodexAuthStatus {
        available: false,
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        account_id: None,
        expires_at: None,
        source: None,
        error: None,
        auth_url: None,
        status: None,
    }
}

pub(crate) use flow::{complete_browser_auth, complete_browser_auth_with};
#[cfg(test)]
pub(crate) use flow::{
    complete_openai_codex_auth_flow_with_result_for_tests,
    complete_openai_codex_auth_with_code_for_tests, current_pending_openai_codex_flow_id_for_tests,
    mark_openai_codex_auth_timeout_for_tests,
};
pub(crate) use storage::{
    clear_openai_codex_auth_test_state, import_codex_cli_auth_if_present,
    read_stored_openai_codex_auth, reset_openai_codex_auth_runtime_for_tests,
    tombstone_present_for_tests, write_stored_openai_codex_auth,
};

pub(crate) fn has_openai_chatgpt_subscription_auth() -> bool {
    metadata_available(&openai_codex_auth_status(true))
}

pub(crate) fn openai_codex_auth_status(refresh_from_import: bool) -> OpenAICodexAuthStatus {
    let runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(pending) = runtime.pending.as_ref() {
        return pending_status(pending);
    }
    if let Some(error) = runtime.last_error.clone() {
        return error_status(error);
    }
    drop(runtime);

    if let Some(auth) = read_stored_openai_codex_auth() {
        return metadata_from_auth(&auth);
    }
    if refresh_from_import {
        match import_codex_cli_auth_if_present() {
            Ok(Some(auth)) => return metadata_from_auth(&auth),
            Ok(None) => {}
            Err(error) => {
                return error_status(format!(
                    "Failed to persist imported OpenAI Codex auth: {error}"
                ));
            }
        }
    }

    empty_status()
}

pub(crate) fn begin_openai_codex_auth_login() -> Result<OpenAICodexAuthStatus> {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(pending) = runtime.pending.as_ref() {
        return Ok(pending_status(pending));
    }

    runtime.last_error = None;
    storage::clear_tombstone()?;

    let pending = flow::build_pending_auth_flow()?;
    let status = pending_status(&pending);
    runtime.pending = Some(pending);
    Ok(status)
}

pub(crate) fn logout_openai_codex_auth() -> Result<()> {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.pending = None;
    runtime.last_error = None;
    drop(runtime);

    super::provider_auth_store::delete_provider_auth_state(OPENAI_PROVIDER_ID, OPENAI_AUTH_MODE)?;
    storage::save_tombstone()?;
    Ok(())
}

pub(crate) fn provider_auth_state_authenticated() -> bool {
    has_openai_chatgpt_subscription_auth()
}
