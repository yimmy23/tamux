use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use url::Url;
use uuid::Uuid;

use super::llm_client::{
    extract_jwt_expiry, extract_openai_codex_account_id, CodexCliAuthFile, StoredOpenAICodexAuth,
};
use super::task_prompt::now_millis;

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
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .build();
        let agent: ureq::Agent = config.into();
        let mut response = agent
            .post(OPENAI_CODEX_AUTH_TOKEN_URL)
            .content_type("application/x-www-form-urlencoded")
            .send_form([
                ("grant_type", "authorization_code"),
                ("client_id", OPENAI_CODEX_AUTH_CLIENT_ID),
                ("code", code),
                ("code_verifier", verifier),
                ("redirect_uri", OPENAI_CODEX_AUTH_REDIRECT_URI),
            ])?;

        let status = response.status();
        if !status.is_success() {
            let text = response.body_mut().read_to_string().unwrap_or_default();
            anyhow::bail!("OpenAI OAuth exchange failed: HTTP {status} {text}");
        }

        let payload: OpenAICodexTokenResponse = response.body_mut().read_json()?;
        stored_auth_from_token_response(payload)
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

fn codex_cli_auth_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("TAMUX_CODEX_CLI_AUTH_PATH") {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".codex").join("auth.json"))
}

fn load_tombstone() -> Option<OpenAICodexAuthTombstone> {
    let value = super::provider_auth_store::load_provider_auth_state(
        OPENAI_PROVIDER_ID,
        tombstone_auth_mode(),
    )
    .ok()??;
    serde_json::from_value(value).ok()
}

fn save_tombstone() -> Result<()> {
    let tombstone = OpenAICodexAuthTombstone {
        provider: "openai-codex".to_string(),
        auth_mode: OPENAI_AUTH_MODE.to_string(),
        tombstoned_at: now_millis() as i64,
        source: "tamux-daemon".to_string(),
    };
    super::provider_auth_store::save_provider_auth_state(
        OPENAI_PROVIDER_ID,
        tombstone_auth_mode(),
        &serde_json::to_value(tombstone)?,
    )
}

fn clear_tombstone() -> Result<()> {
    super::provider_auth_store::delete_provider_auth_state(
        OPENAI_PROVIDER_ID,
        tombstone_auth_mode(),
    )
}

fn generate_pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn build_auth_url(state: &str, challenge: &str) -> Result<String> {
    let mut auth_url = Url::parse(OPENAI_CODEX_AUTH_AUTHORIZE_URL)?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CODEX_AUTH_CLIENT_ID)
        .append_pair("redirect_uri", OPENAI_CODEX_AUTH_REDIRECT_URI)
        .append_pair("scope", OPENAI_CODEX_AUTH_SCOPE)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "tamux");
    Ok(auth_url.to_string())
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

pub(crate) fn read_stored_openai_codex_auth() -> Option<StoredOpenAICodexAuth> {
    let value =
        super::provider_auth_store::load_provider_auth_state(OPENAI_PROVIDER_ID, OPENAI_AUTH_MODE)
            .ok()??;
    let parsed: StoredOpenAICodexAuth = serde_json::from_value(value).ok()?;
    if parsed.access_token.trim().is_empty() || parsed.refresh_token.trim().is_empty() {
        return None;
    }
    Some(parsed)
}

pub(crate) fn write_stored_openai_codex_auth(auth: &StoredOpenAICodexAuth) -> Result<()> {
    super::provider_auth_store::save_provider_auth_state(
        OPENAI_PROVIDER_ID,
        OPENAI_AUTH_MODE,
        &serde_json::to_value(auth)?,
    )
}

pub(crate) fn import_codex_cli_auth_if_present() -> Option<StoredOpenAICodexAuth> {
    if let Some(existing) = read_stored_openai_codex_auth() {
        return Some(existing);
    }
    if load_tombstone().is_some() {
        return None;
    }

    let path = codex_cli_auth_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: CodexCliAuthFile = serde_json::from_str(&raw).ok()?;
    let tokens = parsed.tokens?;
    let access_token = tokens.access_token?;
    let refresh_token = tokens.refresh_token?;
    let account_id = extract_openai_codex_account_id(&access_token)?;
    let expires_at = extract_jwt_expiry(&access_token)?;
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
    let _ = write_stored_openai_codex_auth(&imported);
    read_stored_openai_codex_auth().or(Some(imported))
}

pub(crate) fn has_openai_chatgpt_subscription_auth() -> bool {
    openai_codex_auth_status(false).available
}

pub(crate) fn openai_codex_auth_status(refresh_from_import: bool) -> OpenAICodexAuthStatus {
    let runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(pending) = runtime.pending.as_ref() {
        return pending_status(pending);
    }
    if let Some(auth) = read_stored_openai_codex_auth() {
        return metadata_from_auth(&auth);
    }
    if refresh_from_import {
        if let Some(auth) = import_codex_cli_auth_if_present() {
            return metadata_from_auth(&auth);
        }
    }
    if let Some(error) = runtime.last_error.clone() {
        return error_status(error);
    }

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

pub(crate) fn begin_openai_codex_auth_login() -> Result<OpenAICodexAuthStatus> {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(pending) = runtime.pending.as_ref() {
        return Ok(pending_status(pending));
    }

    runtime.last_error = None;
    clear_tombstone()?;

    let (verifier, challenge) = generate_pkce_pair();
    let state = Uuid::new_v4().simple().to_string();
    let auth_url = build_auth_url(&state, &challenge)?;
    runtime.pending = Some(PendingOpenAICodexAuth {
        flow_id: Uuid::new_v4().simple().to_string(),
        auth_url: auth_url.clone(),
        verifier,
        state,
    });
    Ok(OpenAICodexAuthStatus {
        available: false,
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        account_id: None,
        expires_at: None,
        source: Some("tamux-daemon".to_string()),
        error: None,
        auth_url: Some(auth_url),
        status: Some("pending".to_string()),
    })
}

pub(crate) fn logout_openai_codex_auth() -> Result<()> {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.pending = None;
    runtime.last_error = None;
    super::provider_auth_store::delete_provider_auth_state(OPENAI_PROVIDER_ID, OPENAI_AUTH_MODE)?;
    save_tombstone()?;
    Ok(())
}

fn complete_pending_flow_with_result(
    result: Result<StoredOpenAICodexAuth>,
    flow_id: &str,
) -> OpenAICodexAuthStatus {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let Some(current) = runtime.pending.as_ref() else {
        return openai_codex_auth_status(false);
    };
    if current.flow_id != flow_id {
        return openai_codex_auth_status(false);
    }

    runtime.pending = None;
    match result {
        Ok(auth) => {
            runtime.last_error = None;
            let _ = write_stored_openai_codex_auth(&auth);
            metadata_from_auth(&auth)
        }
        Err(error) => {
            let message = error.to_string();
            runtime.last_error = Some(message.clone());
            error_status(message)
        }
    }
}

fn stored_auth_from_token_response(
    payload: OpenAICodexTokenResponse,
) -> Result<StoredOpenAICodexAuth> {
    let account_id = extract_openai_codex_account_id(&payload.access_token)
        .context("OpenAI OAuth exchange returned no ChatGPT account id")?;
    let now = now_millis() as i64;
    Ok(StoredOpenAICodexAuth {
        provider: Some("openai-codex".to_string()),
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        access_token: payload.access_token,
        refresh_token: payload.refresh_token,
        account_id: Some(account_id),
        expires_at: Some(now.saturating_add(payload.expires_in.saturating_mul(1000))),
        source: Some("tamux".to_string()),
        updated_at: Some(now),
        created_at: Some(now),
    })
}

fn read_callback_request(stream: &mut std::net::TcpStream) -> Result<(String, String)> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buffer = [0u8; 8192];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("received malformed OAuth callback request")?;
    let url = Url::parse(&format!("http://127.0.0.1{target}"))?;
    let callback_state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    Ok((callback_state, code))
}

fn write_callback_failure(stream: &mut std::net::TcpStream) {
    let _ = stream.write_all(
        b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nInvalid OpenAI OAuth callback.",
    );
}

fn write_callback_success(stream: &mut std::net::TcpStream) {
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><html><body><p>Authentication successful. Return to tamux.</p></body></html>",
    );
}

pub(crate) fn complete_browser_auth() -> OpenAICodexAuthStatus {
    complete_browser_auth_with(exchange_client())
}

pub(crate) fn complete_browser_auth_with(
    exchange: &dyn OpenAICodexExchange,
) -> OpenAICodexAuthStatus {
    let pending = {
        let runtime = auth_runtime()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.pending.clone()
    };
    let Some(pending) = pending else {
        return openai_codex_auth_status(false);
    };

    let result = (|| -> Result<StoredOpenAICodexAuth> {
        let listener = TcpListener::bind("127.0.0.1:1455")
            .context("failed to bind localhost callback listener on port 1455")?;
        listener
            .set_nonblocking(false)
            .context("failed to configure callback listener")?;
        let (mut stream, _) = listener.accept()?;
        let (callback_state, code) = read_callback_request(&mut stream)?;

        if callback_state != pending.state || code.is_empty() {
            write_callback_failure(&mut stream);
            anyhow::bail!("invalid OpenAI OAuth callback");
        }

        write_callback_success(&mut stream);
        exchange.exchange_authorization_code(&code, &pending.verifier)
    })();

    complete_pending_flow_with_result(result, &pending.flow_id)
}

pub(crate) fn provider_auth_state_authenticated() -> bool {
    openai_codex_auth_status(false).available
}

#[cfg(test)]
pub(crate) fn mark_openai_codex_auth_timeout_for_tests() -> OpenAICodexAuthStatus {
    let flow_id = {
        let runtime = auth_runtime()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime
            .pending
            .as_ref()
            .map(|pending| pending.flow_id.clone())
    };
    match flow_id {
        Some(flow_id) => complete_pending_flow_with_result(
            Err(anyhow::anyhow!("OpenAI OAuth callback timed out")),
            &flow_id,
        ),
        None => openai_codex_auth_status(false),
    }
}

#[cfg(test)]
pub(crate) fn complete_openai_codex_auth_with_code_for_tests(
    code: &str,
    exchange: &dyn OpenAICodexExchange,
) -> OpenAICodexAuthStatus {
    let pending = {
        let runtime = auth_runtime()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.pending.clone()
    };
    let Some(pending) = pending else {
        return openai_codex_auth_status(false);
    };
    let result = exchange.exchange_authorization_code(code, &pending.verifier);
    complete_pending_flow_with_result(result, &pending.flow_id)
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
    let _ = super::provider_auth_store::delete_provider_auth_state(
        OPENAI_PROVIDER_ID,
        tombstone_auth_mode(),
    );
    let _ = super::provider_auth_store::delete_provider_auth_state(
        OPENAI_PROVIDER_ID,
        OPENAI_AUTH_MODE,
    );
}

#[cfg(test)]
pub(crate) fn tombstone_present_for_tests() -> bool {
    load_tombstone().is_some()
}
