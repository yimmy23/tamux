use anyhow::Result;
use amux_shared::providers::PROVIDER_ID_OPENAI as OPENAI_PROVIDER_ID;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

use super::task_prompt::now_millis;

mod flow;
mod storage;

pub(crate) const OPENAI_CODEX_AUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_AUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub(crate) const OPENAI_CODEX_AUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CODEX_AUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const OPENAI_CODEX_AUTH_SCOPE: &str = "openid profile email offline_access";
pub(crate) const OPENAI_AUTH_MODE: &str = "chatgpt_subscription";
pub(crate) const OPENAI_CODEX_AUTH_PROVIDER: &str = "openai-codex";
const OPENAI_CODEX_AUTH_FAILED_MESSAGE: &str =
    "OpenAI authentication failed. Please try signing in again.";
const OPENAI_CODEX_AUTH_TIMEOUT_MESSAGE: &str =
    "OpenAI authentication timed out. Please try again.";
const OPENAI_CODEX_AUTH_PERSIST_MESSAGE: &str =
    "OpenAI authentication could not be saved. Please try again.";

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
    completion_started: bool,
}

#[derive(Debug, Deserialize)]
struct OpenAICodexTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredOpenAICodexAuth {
    pub(crate) provider: Option<String>,
    pub(crate) auth_mode: Option<String>,
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) account_id: Option<String>,
    pub(crate) expires_at: Option<i64>,
    pub(crate) source: Option<String>,
    pub(crate) updated_at: Option<i64>,
    pub(crate) created_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CodexCliAuthFile {
    pub(super) tokens: Option<CodexCliTokens>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CodexCliTokens {
    pub(super) access_token: Option<String>,
    pub(super) refresh_token: Option<String>,
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

fn decode_jwt_payload(access_token: &str) -> Option<serde_json::Value> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<serde_json::Value>(&decoded).ok()
}

pub(crate) fn extract_openai_codex_account_id(access_token: &str) -> Option<String> {
    decode_jwt_payload(access_token)?
        .get("https://api.openai.com/auth")
        .and_then(|value| value.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

pub(crate) fn extract_jwt_expiry(access_token: &str) -> Option<i64> {
    decode_jwt_payload(access_token)?
        .get("exp")
        .and_then(|value| value.as_i64())
        .map(|seconds| seconds.saturating_mul(1000))
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

fn sanitized_auth_failure_message(message: &str) -> String {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("timed out") {
        OPENAI_CODEX_AUTH_TIMEOUT_MESSAGE.to_string()
    } else if lowered.contains("persist") || lowered.contains("save") {
        OPENAI_CODEX_AUTH_PERSIST_MESSAGE.to_string()
    } else {
        OPENAI_CODEX_AUTH_FAILED_MESSAGE.to_string()
    }
}

pub(crate) fn openai_codex_auth_error_message(message: &str) -> String {
    sanitized_auth_failure_message(message)
}

pub(crate) fn openai_codex_auth_error_status(message: &str) -> OpenAICodexAuthStatus {
    error_status(openai_codex_auth_error_message(message))
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

pub(crate) fn complete_browser_auth() -> OpenAICodexAuthStatus {
    flow::complete_browser_auth()
}

#[cfg(test)]
pub(crate) use flow::{
    complete_browser_auth_with_timeout_for_tests,
    complete_browser_auth_with_timeout_ready_signal_for_tests,
    complete_openai_codex_auth_flow_with_result_for_tests,
    complete_openai_codex_auth_with_code_for_tests, current_pending_openai_codex_flow_id_for_tests,
    mark_openai_codex_auth_timeout_for_tests,
};
#[cfg(test)]
pub(crate) use storage::{
    clear_openai_codex_auth_test_state, reset_openai_codex_auth_runtime_for_tests,
    tombstone_present_for_tests,
};
pub(crate) use storage::{
    import_codex_cli_auth_if_present, read_codex_cli_auth_if_present,
    read_stored_openai_codex_auth, write_stored_openai_codex_auth,
};

pub(crate) fn has_openai_chatgpt_subscription_auth() -> bool {
    read_stored_openai_codex_auth()
        .or_else(read_codex_cli_auth_if_present)
        .map(|auth| metadata_available(&metadata_from_auth(&auth)))
        .unwrap_or(false)
}

pub(crate) fn openai_codex_auth_status(refresh_from_import: bool) -> OpenAICodexAuthStatus {
    let runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(pending) = runtime.pending.as_ref() {
        return pending_status(pending);
    }
    let runtime_error = runtime.last_error.clone();
    drop(runtime);

    if let Some(auth) = read_stored_openai_codex_auth() {
        return metadata_from_auth(&auth);
    }
    if refresh_from_import {
        match import_codex_cli_auth_if_present() {
            Ok(Some(auth)) => return metadata_from_auth(&auth),
            Ok(None) => {}
            Err(error) => return openai_codex_auth_error_status(&error.to_string()),
        }
    }

    if let Some(error) = runtime_error {
        return error_status(error);
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

pub(crate) fn mark_openai_codex_auth_completion_started() -> bool {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let Some(pending) = runtime.pending.as_mut() else {
        return false;
    };

    if pending.completion_started {
        return false;
    }

    pending.completion_started = true;
    true
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
