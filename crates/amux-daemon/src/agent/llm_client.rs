//! LLM API client with SSE streaming support.
//!
//! Supports two API formats:
//! - OpenAI-compatible (`/chat/completions` with Bearer auth) — covers most providers
//! - Anthropic Messages API (`/v1/messages` with `x-api-key` header)

use anyhow::{Context, Result};
use base64::Engine;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;
use tokio::sync::mpsc;

use super::task_prompt::now_millis;
use super::types::{
    get_provider_api_type, get_provider_definition, ApiTransport, ApiType, AuthMethod, AuthSource,
    CompletionChunk, ProviderConfig, ToolCall, ToolDefinition, ToolFunction,
};

#[derive(Debug, Clone, Copy)]
pub enum RetryStrategy {
    Bounded {
        max_retries: u32,
        retry_delay_ms: u64,
    },
    DurableRateLimited,
}

// ---------------------------------------------------------------------------
// API message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: ApiContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ApiContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ApiToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCallFunction {
    pub name: String,
    pub arguments: String,
}

// ---------------------------------------------------------------------------
// Completion stream wrapper
// ---------------------------------------------------------------------------

pub struct CompletionStream {
    rx: mpsc::Receiver<Result<CompletionChunk>>,
}

#[derive(Debug)]
struct RateLimitError {
    provider: String,
    details: String,
}

impl fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} API returned 429: {}", self.provider, self.details)
    }
}

impl std::error::Error for RateLimitError {}

#[derive(Debug)]
struct TransportCompatibilityError {
    provider: String,
    details: String,
}

impl fmt::Display for TransportCompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} transport incompatibility: {}",
            self.provider, self.details
        )
    }
}

impl std::error::Error for TransportCompatibilityError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOpenAICodexAuth {
    provider: Option<String>,
    auth_mode: Option<String>,
    access_token: String,
    refresh_token: String,
    account_id: Option<String>,
    expires_at: Option<i64>,
    source: Option<String>,
    updated_at: Option<i64>,
    created_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CodexCliAuthFile {
    tokens: Option<CodexCliTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexCliTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
struct OpenAICodexRequestAuth {
    access_token: String,
    account_id: String,
}

/// Build an appropriate error for a non-success API response, distinguishing
/// rate-limit (429) errors for retry handling.
fn check_rate_limit_response(
    status: reqwest::StatusCode,
    provider: &str,
    body_text: &str,
) -> anyhow::Error {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        RateLimitError {
            provider: provider.to_string(),
            details: body_text.to_string(),
        }
        .into()
    } else {
        anyhow::anyhow!("{provider} API returned {status}: {body_text}")
    }
}

fn is_transient_transport_error(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_error.is_timeout()
                || reqwest_error.is_connect()
                || reqwest_error.is_request()
                || reqwest_error.is_body()
                || reqwest_error.is_decode()
            {
                return true;
            }
        }

        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind;

            if matches!(
                io_error.kind(),
                ErrorKind::TimedOut
                    | ErrorKind::Interrupted
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::BrokenPipe
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::WouldBlock
            ) {
                return true;
            }
        }
    }

    let message = err.to_string().to_ascii_lowercase();
    message.contains("error sending request for url")
        || message.contains("connection reset")
        || message.contains("connection refused")
        || message.contains("timed out")
        || message.contains("unexpected eof")
}

impl Stream for CompletionStream {
    type Item = Result<CompletionChunk>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

fn openai_codex_auth_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".tamux").join("openai-codex-auth.json"))
}

fn codex_cli_auth_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("auth.json"))
}

fn decode_jwt_payload(access_token: &str) -> Option<serde_json::Value> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<serde_json::Value>(&decoded).ok()
}

fn extract_openai_codex_account_id(access_token: &str) -> Option<String> {
    decode_jwt_payload(access_token)?
        .get("https://api.openai.com/auth")
        .and_then(|value| value.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn extract_jwt_expiry(access_token: &str) -> Option<i64> {
    decode_jwt_payload(access_token)?
        .get("exp")
        .and_then(|value| value.as_i64())
        .map(|seconds| seconds.saturating_mul(1000))
}

fn read_stored_openai_codex_auth() -> Option<StoredOpenAICodexAuth> {
    let path = openai_codex_auth_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: StoredOpenAICodexAuth = serde_json::from_str(&raw).ok()?;
    if parsed.access_token.trim().is_empty() || parsed.refresh_token.trim().is_empty() {
        return None;
    }
    Some(parsed)
}

fn write_stored_openai_codex_auth(auth: &StoredOpenAICodexAuth) -> Result<()> {
    let path = openai_codex_auth_path().context("home directory unavailable for tamux auth")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(auth)?)?;
    Ok(())
}

fn import_codex_cli_auth_if_present() -> Option<StoredOpenAICodexAuth> {
    if let Some(existing) = read_stored_openai_codex_auth() {
        return Some(existing);
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
        auth_mode: Some("chatgpt_subscription".to_string()),
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

async fn refresh_openai_codex_auth(
    client: &reqwest::Client,
    auth: &StoredOpenAICodexAuth,
) -> Result<StoredOpenAICodexAuth> {
    let response = client
        .post("https://auth.openai.com/oauth/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", auth.refresh_token.as_str()),
            ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI token refresh failed: HTTP {status} {text}");
    }

    let payload: serde_json::Value = response.json().await?;
    let access_token = payload
        .get("access_token")
        .and_then(|value| value.as_str())
        .context("OpenAI token refresh returned no access_token")?
        .to_string();
    let refresh_token = payload
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .context("OpenAI token refresh returned no refresh_token")?
        .to_string();
    let account_id = extract_openai_codex_account_id(&access_token)
        .context("OpenAI token refresh returned no ChatGPT account id")?;
    let expires_in_ms = payload
        .get("expires_in")
        .and_then(|value| value.as_i64())
        .unwrap_or(3600)
        .saturating_mul(1000);
    let now = now_millis() as i64;
    let refreshed = StoredOpenAICodexAuth {
        provider: Some("openai-codex".to_string()),
        auth_mode: Some("chatgpt_subscription".to_string()),
        access_token,
        refresh_token,
        account_id: Some(account_id),
        expires_at: Some(now.saturating_add(expires_in_ms)),
        source: auth.source.clone().or_else(|| Some("tamux".to_string())),
        updated_at: Some(now),
        created_at: auth.created_at.or(Some(now)),
    };
    write_stored_openai_codex_auth(&refreshed)?;
    Ok(refreshed)
}

async fn resolve_openai_codex_request_auth(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
) -> Result<Option<OpenAICodexRequestAuth>> {
    if provider != "openai" || config.auth_source != AuthSource::ChatgptSubscription {
        return Ok(None);
    }

    let auth = read_stored_openai_codex_auth().or_else(import_codex_cli_auth_if_present);
    let mut auth = auth.context(
        "No ChatGPT subscription auth found. Authenticate in the frontend or import ~/.codex/auth.json.",
    )?;
    let now = now_millis() as i64;
    if auth.expires_at.unwrap_or(0) <= now.saturating_add(60_000) {
        auth = refresh_openai_codex_auth(client, &auth).await?;
    }

    let account_id = auth
        .account_id
        .clone()
        .or_else(|| extract_openai_codex_account_id(&auth.access_token))
        .context("ChatGPT subscription auth is missing chatgpt_account_id")?;

    Ok(Some(OpenAICodexRequestAuth {
        access_token: auth.access_token,
        account_id,
    }))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Send a completion request. Returns a stream of `CompletionChunk`.
pub fn send_completion_request(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    transport: ApiTransport,
    previous_response_id: Option<String>,
    upstream_thread_id: Option<String>,
    retry_strategy: RetryStrategy,
) -> CompletionStream {
    let (tx, rx) = mpsc::channel(64);
    let client = client.clone();
    let provider = provider.to_string();
    let config = config.clone();
    let system_prompt = system_prompt.to_string();
    let messages = messages.to_vec();
    let tools = tools.to_vec();
    let previous_response_id = previous_response_id.clone();
    let upstream_thread_id = upstream_thread_id.clone();

    tokio::spawn(async move {
        let mut retry_attempt = 0u32;
        loop {
            let api_type = get_provider_api_type(&provider, &config.model, &config.base_url);

            let result = if api_type == ApiType::Anthropic {
                run_anthropic(
                    &client,
                    &provider,
                    &config,
                    &system_prompt,
                    &messages,
                    &tools,
                    &tx,
                )
                .await
            } else {
                match transport {
                    ApiTransport::NativeAssistant => {
                        match run_native_assistant(
                            &client,
                            &provider,
                            &config,
                            &messages,
                            upstream_thread_id.as_deref(),
                            &tx,
                        )
                        .await
                        {
                            Ok(()) => Ok(()),
                            Err(err)
                                if err.downcast_ref::<TransportCompatibilityError>().is_some() =>
                            {
                                let reason = err.to_string();
                                let _ = tx
                                    .send(Ok(CompletionChunk::TransportFallback {
                                        from: ApiTransport::NativeAssistant,
                                        to: ApiTransport::ChatCompletions,
                                        message: reason,
                                    }))
                                    .await;
                                run_openai_chat_completions(
                                    &client,
                                    &provider,
                                    &config,
                                    &system_prompt,
                                    &messages,
                                    &tools,
                                    &tx,
                                )
                                .await
                            }
                            Err(err) => Err(err),
                        }
                    }
                    ApiTransport::ChatCompletions => {
                        run_openai_chat_completions(
                            &client,
                            &provider,
                            &config,
                            &system_prompt,
                            &messages,
                            &tools,
                            &tx,
                        )
                        .await
                    }
                    ApiTransport::Responses => {
                        match run_openai_responses(
                            &client,
                            &provider,
                            &config,
                            &system_prompt,
                            &messages,
                            &tools,
                            previous_response_id.as_deref(),
                            &tx,
                        )
                        .await
                        {
                            Ok(()) => Ok(()),
                            Err(err)
                                if err.downcast_ref::<TransportCompatibilityError>().is_some() =>
                            {
                                let reason = err.to_string();
                                let _ = tx
                                    .send(Ok(CompletionChunk::TransportFallback {
                                        from: ApiTransport::Responses,
                                        to: ApiTransport::ChatCompletions,
                                        message: reason,
                                    }))
                                    .await;
                                run_openai_chat_completions(
                                    &client,
                                    &provider,
                                    &config,
                                    &system_prompt,
                                    &messages,
                                    &tools,
                                    &tx,
                                )
                                .await
                            }
                            Err(err) => Err(err),
                        }
                    }
                }
            };

            match result {
                Ok(()) => break,
                Err(e) => {
                    let is_rate_limited = e.downcast_ref::<RateLimitError>().is_some();
                    let is_transient_transport = is_transient_transport_error(&e);
                    if is_rate_limited || is_transient_transport {
                        match retry_strategy {
                            RetryStrategy::Bounded {
                                max_retries,
                                retry_delay_ms,
                            } if retry_attempt < max_retries => {
                                retry_attempt += 1;
                                let _ = tx
                                    .send(Ok(CompletionChunk::Retry {
                                        attempt: retry_attempt,
                                        max_retries,
                                        delay_ms: retry_delay_ms,
                                    }))
                                    .await;
                                tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
                                continue;
                            }
                            RetryStrategy::DurableRateLimited if is_rate_limited => {
                                retry_attempt = retry_attempt.saturating_add(1);
                                let delay_ms = if retry_attempt <= 10 { 2_000 } else { 60_000 };
                                let _ = tx
                                    .send(Ok(CompletionChunk::Retry {
                                        attempt: retry_attempt,
                                        max_retries: 0,
                                        delay_ms,
                                    }))
                                    .await;
                                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                continue;
                            }
                            _ => {}
                        }
                    }

                    let message = if let Some(rate_limit) = e.downcast_ref::<RateLimitError>() {
                        rate_limit.to_string()
                    } else {
                        format!("API error: {e}")
                    };
                    let _ = tx.send(Ok(CompletionChunk::Error { message })).await;
                    break;
                }
            }
        }
    });

    CompletionStream { rx }
}

/// Convert `AgentMessage` history to API format.
pub fn messages_to_api_format(messages: &[super::types::AgentMessage]) -> Vec<ApiMessage> {
    let mut pending_tool_results = std::collections::VecDeque::new();

    messages
        .iter()
        .filter(|m| {
            matches!(
                m.role,
                super::types::MessageRole::User
                    | super::types::MessageRole::Assistant
                    | super::types::MessageRole::Tool
            )
        })
        .filter_map(|m| {
            let mut normalized_tool_calls = None;
            if matches!(m.role, super::types::MessageRole::Assistant) {
                pending_tool_results.clear();
                if let Some(tool_calls) = &m.tool_calls {
                    let normalized: Vec<ApiToolCall> = tool_calls
                        .iter()
                        .enumerate()
                        .map(|(index, tc)| {
                            let normalized_id = if tc.id.trim().is_empty() {
                                format!(
                                    "synthetic_tool_call_{}_{}_{}",
                                    m.timestamp,
                                    index,
                                    tc.function.name
                                )
                            } else {
                                tc.id.clone()
                            };
                            pending_tool_results.push_back(normalized_id.clone());
                            ApiToolCall {
                                id: normalized_id,
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tc.function.name.clone(),
                                    arguments: tc.function.arguments.clone(),
                                },
                            }
                        })
                        .collect();
                    normalized_tool_calls = Some(normalized);
                }
            }

            let mut normalized_tool_call_id = m.tool_call_id.clone();
            if matches!(m.role, super::types::MessageRole::Tool) {
                let resolved_tool_call_id = if let Some(tool_call_id) =
                    m.tool_call_id.as_ref().filter(|value| !value.trim().is_empty())
                {
                    let position = pending_tool_results
                        .iter()
                        .position(|pending_id| pending_id == tool_call_id);
                    let Some(position) = position else {
                        return None;
                    };
                    pending_tool_results.remove(position).unwrap_or_default()
                } else {
                    let Some(next_pending) = pending_tool_results.pop_front() else {
                        return None;
                    };
                    next_pending
                };

                normalized_tool_call_id = Some(resolved_tool_call_id);
                if normalized_tool_call_id.as_deref().is_none() {
                    return None;
                }
            } else if !matches!(m.role, super::types::MessageRole::Assistant)
                && !pending_tool_results.is_empty()
            {
                pending_tool_results.clear();
            }

            Some(ApiMessage {
                role: match m.role {
                    super::types::MessageRole::System => "system".into(),
                    super::types::MessageRole::User => "user".into(),
                    super::types::MessageRole::Assistant => "assistant".into(),
                    super::types::MessageRole::Tool => "tool".into(),
                },
                content: ApiContent::Text(m.content.clone()),
                tool_call_id: normalized_tool_call_id,
                name: m.tool_name.clone(),
                tool_calls: normalized_tool_calls.or_else(|| {
                    m.tool_calls.as_ref().map(|tcs| {
                        tcs.iter()
                            .map(|tc| ApiToolCall {
                                id: tc.id.clone(),
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tc.function.name.clone(),
                                    arguments: tc.function.arguments.clone(),
                                },
                            })
                            .collect()
                    })
                }),
            })
        })
        .collect()
}

fn api_content_to_json(content: &ApiContent) -> serde_json::Value {
    match content {
        ApiContent::Text(text) => serde_json::Value::String(text.clone()),
        ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
    }
}

fn build_chat_completion_messages(
    system_prompt: &str,
    messages: &[ApiMessage],
) -> Result<Vec<serde_json::Value>> {
    let mut out = Vec::with_capacity(messages.len() + 1);
    out.push(serde_json::json!({
        "role": "system",
        "content": system_prompt,
    }));

    for message in messages {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "role".to_string(),
            serde_json::Value::String(message.role.clone()),
        );

        if message.role == "assistant"
            && message
                .tool_calls
                .as_ref()
                .is_some_and(|tool_calls| !tool_calls.is_empty())
        {
            obj.insert("content".to_string(), serde_json::Value::Null);
            obj.insert(
                "tool_calls".to_string(),
                serde_json::to_value(message.tool_calls.clone().unwrap_or_default())?,
            );
        } else {
            obj.insert("content".to_string(), api_content_to_json(&message.content));
            if let Some(tool_call_id) = &message.tool_call_id {
                obj.insert(
                    "tool_call_id".to_string(),
                    serde_json::Value::String(tool_call_id.clone()),
                );
            }
            if let Some(name) = &message.name {
                obj.insert("name".to_string(), serde_json::Value::String(name.clone()));
            }
            if let Some(tool_calls) = &message.tool_calls {
                obj.insert("tool_calls".to_string(), serde_json::to_value(tool_calls)?);
            }
        }

        out.push(serde_json::Value::Object(obj));
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// OpenAI-compatible implementation
// ---------------------------------------------------------------------------

fn build_chat_completion_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let lower = base.to_lowercase();

    // If URL already has a version suffix, just append the endpoint
    if lower.ends_with("/v1")
        || lower.ends_with("/v2")
        || lower.ends_with("/v3")
        || lower.ends_with("/v4")
        || lower.ends_with("/api/v1")
        || lower.ends_with("/openai/v1")
        || lower.ends_with("/compatible-mode/v1")
    {
        return format!("{base}/chat/completions");
    }

    format!("{base}/v1/chat/completions")
}

fn build_responses_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let lower = base.to_lowercase();

    if lower.ends_with("/v1")
        || lower.ends_with("/v2")
        || lower.ends_with("/v3")
        || lower.ends_with("/v4")
        || lower.ends_with("/api/v1")
        || lower.ends_with("/openai/v1")
        || lower.ends_with("/compatible-mode/v1")
    {
        return format!("{base}/responses");
    }

    format!("{base}/v1/responses")
}

fn normalize_reasoning_effort(effort: &str) -> Option<String> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "" | "off" | "none" => None,
        "minimal" => Some("low".to_string()),
        "low" => Some("low".to_string()),
        "medium" => Some("medium".to_string()),
        "high" => Some("high".to_string()),
        "xhigh" => Some("high".to_string()),
        other => Some(other.to_string()),
    }
}

fn openai_reasoning_supported(provider: &str, model: &str) -> bool {
    matches!(
        provider,
        "openai"
            | "openrouter"
            | "qwen"
            | "qwen-deepinfra"
            | "opencode-zen"
            | "z.ai"
            | "z.ai-coding-plan"
    ) || model.starts_with('o')
        || model.starts_with("gpt-5")
}

fn dashscope_openai_uses_enable_thinking(provider: &str, model: &str) -> bool {
    matches!(provider, "qwen" | "alibaba-coding-plan")
        && matches!(
            model,
            "qwen3.5-plus" | "qwen3-max-2026-01-23" | "glm-4.7" | "glm-5"
        )
}

fn is_dashscope_coding_plan_anthropic_base_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.contains("dashscope.aliyuncs.com") && lower.contains("/apps/anthropic")
}

fn apply_dashscope_coding_plan_sdk_headers(
    req: reqwest::RequestBuilder,
    provider: &str,
    base_url: &str,
    api_type: ApiType,
) -> reqwest::RequestBuilder {
    if provider != "alibaba-coding-plan" {
        return req;
    }

    let req = req.header(
        "User-Agent",
        match api_type {
            ApiType::Anthropic => "Anthropic/JS tamux",
            ApiType::OpenAI => "OpenAI/JS tamux",
        },
    );
    if api_type == ApiType::OpenAI && !is_dashscope_coding_plan_anthropic_base_url(base_url) {
        req.header("x-stainless-lang", "js")
            .header("x-stainless-package-version", "tamux")
    } else {
        req
    }
}

fn anthropic_thinking_budget(effort: &str) -> Option<u32> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "" | "off" | "none" => None,
        "minimal" => Some(512),
        "low" => Some(1024),
        "medium" => Some(4096),
        "high" => Some(8192),
        "xhigh" => Some(16384),
        _ => Some(4096),
    }
}

fn build_openai_auth_request<'a>(
    client: &'a reqwest::Client,
    url: &str,
    provider: &str,
    config: &ProviderConfig,
) -> reqwest::RequestBuilder {
    apply_openai_auth_headers(
        client.post(url).header("Content-Type", "application/json"),
        provider,
        config,
    )
}

fn apply_openai_auth_headers(
    mut req: reqwest::RequestBuilder,
    provider: &str,
    config: &ProviderConfig,
) -> reqwest::RequestBuilder {
    if !config.api_key.is_empty() {
        let auth_method = get_provider_definition(provider)
            .map(|d| d.auth_method)
            .unwrap_or(AuthMethod::Bearer);

        match auth_method {
            AuthMethod::Bearer => {
                req = req.header("Authorization", format!("Bearer {}", config.api_key));
            }
            AuthMethod::XApiKey => {
                req = req.header("x-api-key", &config.api_key);
            }
        }
    }

    req
}

fn build_native_assistant_base_url(provider: &str, config: &ProviderConfig) -> Option<String> {
    let preferred =
        get_provider_definition(provider).and_then(|definition| definition.native_base_url);
    preferred
        .or_else(|| (!config.base_url.trim().is_empty()).then_some(config.base_url.as_str()))
        .map(|url| url.trim_end_matches('/').to_string())
}

fn api_message_to_text(message: &ApiMessage) -> Option<String> {
    match &message.content {
        ApiContent::Text(text) => Some(text.clone()),
        ApiContent::Blocks(blocks) => {
            let combined = blocks
                .iter()
                .filter_map(|block| {
                    block
                        .get("text")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!combined.trim().is_empty()).then_some(combined)
        }
    }
}

async fn run_native_assistant(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    messages: &[ApiMessage],
    upstream_thread_id: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let definition = get_provider_definition(provider).ok_or_else(|| {
        anyhow::anyhow!("native assistant transport is not defined for provider '{provider}'")
    })?;
    if definition.native_transport_kind.is_none() {
        return Err(TransportCompatibilityError {
            provider: provider.to_string(),
            details: "provider does not expose a native assistant API".to_string(),
        }
        .into());
    }
    if config.assistant_id.trim().is_empty() {
        return Err(TransportCompatibilityError {
            provider: provider.to_string(),
            details: "native assistant requires assistant_id".to_string(),
        }
        .into());
    }
    let base_url = build_native_assistant_base_url(provider, config).ok_or_else(|| {
        anyhow::anyhow!("native assistant base URL is not configured for provider '{provider}'")
    })?;
    let user_text = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(api_message_to_text)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("native assistant requires a user message"))?;

    let thread_id = match upstream_thread_id.filter(|value| !value.trim().is_empty()) {
        Some(existing) => existing.to_string(),
        None => {
            let url = format!("{base_url}/threads");
            let response = build_openai_auth_request(client, &url, provider, config)
                .body("{}".to_string())
                .send()
                .await?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response
                    .text()
                    .await
                    .unwrap_or_default()
                    .chars()
                    .take(240)
                    .collect::<String>();
                let is_compatibility_error = matches!(
                    status,
                    reqwest::StatusCode::BAD_REQUEST
                        | reqwest::StatusCode::NOT_FOUND
                        | reqwest::StatusCode::METHOD_NOT_ALLOWED
                        | reqwest::StatusCode::UNPROCESSABLE_ENTITY
                );
                if is_compatibility_error {
                    return Err(TransportCompatibilityError {
                        provider: provider.to_string(),
                        details: format!(
                            "native assistant thread creation failed ({status}): {text}"
                        ),
                    }
                    .into());
                }
                return Err(check_rate_limit_response(status, provider, &text));
            }
            let payload: serde_json::Value = response.json().await?;
            payload
                .get("id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    anyhow::anyhow!("native assistant thread creation returned no thread id")
                })?
        }
    };

    let message_url = format!("{base_url}/threads/{thread_id}/messages");
    let add_message_body = serde_json::json!({
        "role": "user",
        "content": user_text,
    });
    let add_message_response = build_openai_auth_request(client, &message_url, provider, config)
        .body(add_message_body.to_string())
        .send()
        .await?;
    if !add_message_response.status().is_success() {
        let status = add_message_response.status();
        let text = add_message_response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        let is_compatibility_error = matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST
                | reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        if is_compatibility_error {
            return Err(TransportCompatibilityError {
                provider: provider.to_string(),
                details: format!("native assistant message append failed ({status}): {text}"),
            }
            .into());
        }
        return Err(check_rate_limit_response(status, provider, &text));
    }

    let run_url = format!("{base_url}/threads/{thread_id}/runs");
    let run_body = serde_json::json!({
        "assistant_id": config.assistant_id,
    });
    let run_response = build_openai_auth_request(client, &run_url, provider, config)
        .body(run_body.to_string())
        .send()
        .await?;
    if !run_response.status().is_success() {
        let status = run_response.status();
        let text = run_response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        let is_compatibility_error = matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST
                | reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        if is_compatibility_error {
            return Err(TransportCompatibilityError {
                provider: provider.to_string(),
                details: format!("native assistant run creation failed ({status}): {text}"),
            }
            .into());
        }
        return Err(check_rate_limit_response(status, provider, &text));
    }
    let run_payload: serde_json::Value = run_response.json().await?;
    let run_id = run_payload
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("native assistant run creation returned no run id"))?
        .to_string();

    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let run_status_url = format!("{base_url}/threads/{thread_id}/runs/{run_id}");
    for _ in 0..180u32 {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let status_response =
            apply_openai_auth_headers(client.get(&run_status_url), provider, config)
                .send()
                .await?;
        if !status_response.status().is_success() {
            let status = status_response.status();
            let text = status_response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(240)
                .collect::<String>();
            return Err(check_rate_limit_response(status, provider, &text));
        }
        let run_status: serde_json::Value = status_response.json().await?;
        if let Some(usage) = run_status.get("usage") {
            input_tokens = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(|value| value.as_u64())
                .unwrap_or(input_tokens);
            output_tokens = usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(|value| value.as_u64())
                .unwrap_or(output_tokens);
        }
        match run_status
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
        {
            "queued" | "in_progress" => continue,
            "completed" => {
                let content =
                    fetch_native_assistant_message(client, provider, config, &base_url, &thread_id)
                        .await?;
                let _ = tx
                    .send(Ok(CompletionChunk::Done {
                        content,
                        reasoning: None,
                        input_tokens,
                        output_tokens,
                        response_id: None,
                        upstream_thread_id: Some(thread_id),
                    }))
                    .await;
                return Ok(());
            }
            "requires_action" => {
                return Err(anyhow::anyhow!(
                    "native assistant requires external tool action, which tamux does not proxy yet"
                ));
            }
            "failed" | "cancelled" | "expired" => {
                let details = run_status
                    .get("last_error")
                    .and_then(|value| value.get("message"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("native assistant run failed");
                return Err(anyhow::anyhow!("{details}"));
            }
            other => {
                return Err(anyhow::anyhow!(
                    "native assistant run entered unexpected status '{other}'"
                ));
            }
        }
    }

    Err(anyhow::anyhow!(
        "native assistant run timed out while waiting for completion"
    ))
}

async fn fetch_native_assistant_message(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    base_url: &str,
    thread_id: &str,
) -> Result<String> {
    let url = format!("{base_url}/threads/{thread_id}/messages?order=desc&limit=20");
    let response = apply_openai_auth_headers(client.get(&url), provider, config)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        return Err(check_rate_limit_response(status, provider, &text));
    }
    let payload: serde_json::Value = response.json().await?;
    let data = payload
        .get("data")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("native assistant message list returned no data array"))?;
    for message in data {
        if message.get("role").and_then(|value| value.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(content_blocks) = message.get("content").and_then(|value| value.as_array()) {
            let text = content_blocks
                .iter()
                .filter_map(|block| {
                    block
                        .get("text")
                        .and_then(|value| value.get("value").or(Some(value)))
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        if let Some(text) = message.get("content").and_then(|value| value.as_str()) {
            if !text.trim().is_empty() {
                return Ok(text.to_string());
            }
        }
    }
    Err(anyhow::anyhow!(
        "native assistant thread did not contain a completed assistant message"
    ))
}

async fn run_openai_chat_completions(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let url = build_chat_completion_url(&config.base_url);

    let all_messages = build_chat_completion_messages(system_prompt, messages)?;

    let mut body = serde_json::json!({
        "model": config.model,
        "messages": all_messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::to_value(tools)?;
        body["tool_choice"] = serde_json::json!("auto");
    }

    if let Some(ref schema) = config.response_schema {
        // Try strict json_schema for providers that support it (OpenAI gpt-4o+)
        if matches!(provider, "openai")
            && (config.model.contains("gpt-4o")
                || config.model.contains("gpt-4.1")
                || config.model.contains("gpt-5")
                || config.model.starts_with("o"))
        {
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "strict": true,
                    "schema": schema,
                }
            });
        } else {
            // Fallback: json_object mode (widely supported, no schema enforcement)
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
    }

    if dashscope_openai_uses_enable_thinking(provider, &config.model) {
        body["enable_thinking"] =
            serde_json::Value::Bool(normalize_reasoning_effort(&config.reasoning_effort).is_some());
    } else if openai_reasoning_supported(provider, &config.model) {
        if let Some(effort) = normalize_reasoning_effort(&config.reasoning_effort) {
            body["reasoning_effort"] = serde_json::Value::String(effort.clone());
            body["reasoning"] = serde_json::json!({ "effort": effort });
        }
    }

    let req = apply_dashscope_coding_plan_sdk_headers(
        build_openai_auth_request(client, &url, provider, config),
        provider,
        &config.base_url,
        ApiType::OpenAI,
    );

    let response = req.body(body.to_string()).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(check_rate_limit_response(status, provider, &text));
    }

    parse_openai_sse(response, tx).await
}

fn messages_to_responses_input(messages: &[ApiMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter_map(|message| match message.role.as_str() {
            "user" | "assistant" => Some(serde_json::json!({
                "role": message.role,
                "content": match &message.content {
                    ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                    ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                }
            })),
            "tool" => message.tool_call_id.as_ref().map(|call_id| {
                serde_json::json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": match &message.content {
                        ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                        ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                    }
                })
            }),
            _ => None,
        })
        .collect()
}

fn build_anthropic_message_content(message: &ApiMessage) -> serde_json::Value {
    if message.role == "assistant" && message.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()) {
        let mut blocks = Vec::new();
        if let ApiContent::Text(text) = &message.content {
            if !text.is_empty() {
                blocks.push(serde_json::json!({
                    "type": "text",
                    "text": text,
                }));
            }
        }
        if let Some(tool_calls) = &message.tool_calls {
            for tool_call in tool_calls {
                let input = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
                    .unwrap_or_else(|_| {
                        serde_json::json!({
                            "_raw_arguments": tool_call.function.arguments,
                        })
                    });
                blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": tool_call.id,
                    "name": tool_call.function.name,
                    "input": input,
                }));
            }
        }
        serde_json::Value::Array(blocks)
    } else {
        match &message.content {
            ApiContent::Text(text) => serde_json::json!(text),
            ApiContent::Blocks(blocks) => serde_json::json!(blocks),
        }
    }
}

fn build_anthropic_messages(messages: &[ApiMessage]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < messages.len() {
        let message = &messages[index];
        if message.role == "tool" {
            let mut blocks = Vec::new();
            while index < messages.len() && messages[index].role == "tool" {
                let tool_message = &messages[index];
                if let Some(tool_use_id) = tool_message.tool_call_id.as_ref() {
                    blocks.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": match &tool_message.content {
                            ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                            ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                        }
                    }));
                }
                index += 1;
            }
            if !blocks.is_empty() {
                out.push(serde_json::json!({
                    "role": "user",
                    "content": blocks,
                }));
            }
            continue;
        }

        let role = match message.role.as_str() {
            "system" => "user",
            other => other,
        };
        out.push(serde_json::json!({
            "role": role,
            "content": build_anthropic_message_content(message),
        }));
        index += 1;
    }

    out
}

async fn run_openai_responses(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    previous_response_id: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let codex_auth = resolve_openai_codex_request_auth(client, provider, config).await?;
    let url = if codex_auth.is_some() {
        "https://chatgpt.com/backend-api/codex/responses".to_string()
    } else {
        build_responses_url(&config.base_url)
    };
    let mut body = serde_json::json!({
        "model": config.model,
        "instructions": system_prompt,
        "input": messages_to_responses_input(messages),
        "stream": true,
    });

    if let Some(previous_response_id) =
        previous_response_id.filter(|value| !value.trim().is_empty())
    {
        body["previous_response_id"] = serde_json::Value::String(previous_response_id.to_string());
    }

    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            tools
                .iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": tool.tool_type,
                        "name": tool.function.name,
                        "description": tool.function.description,
                        "parameters": tool.function.parameters,
                    })
                })
                .collect(),
        );
    }

    if let Some(ref schema) = config.response_schema {
        body["text"] = serde_json::json!({
            "format": {
                "type": "json_schema",
                "name": "structured_output",
                "strict": true,
                "schema": schema,
            }
        });
    }

    if let Some(effort) = normalize_reasoning_effort(&config.reasoning_effort) {
        body["reasoning"] = serde_json::json!({ "effort": effort });
    }

    if codex_auth.is_some() {
        body["store"] = serde_json::Value::Bool(false);
        body["include"] = serde_json::Value::Array(vec![serde_json::Value::String(
            "reasoning.encrypted_content".to_string(),
        )]);
        if body.get("text").is_none() {
            body["text"] = serde_json::json!({ "verbosity": "high" });
        } else if let Some(text_obj) = body.get_mut("text").and_then(|value| value.as_object_mut())
        {
            text_obj.insert(
                "verbosity".to_string(),
                serde_json::Value::String("high".to_string()),
            );
        }
    }

    let req = if let Some(codex_auth) = codex_auth {
        client
            .post(&url)
            .header("Content-Type", "application/json")
            .header(
                "Authorization",
                format!("Bearer {}", codex_auth.access_token),
            )
            .header("chatgpt-account-id", codex_auth.account_id)
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", "tamux")
    } else {
        build_openai_auth_request(client, &url, provider, config)
    };
    let response = req.body(body.to_string()).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        let is_compatibility_error = matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST
                | reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
                | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        if is_compatibility_error {
            return Err(TransportCompatibilityError {
                provider: provider.to_string(),
                details: format!("Responses API rejected the request ({status}): {text}"),
            }
            .into());
        }
        return Err(check_rate_limit_response(status, provider, &text));
    }

    parse_openai_responses_sse(response, provider, tx).await
}

async fn parse_openai_sse(
    response: reqwest::Response,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut total_content = String::new();
    let mut total_reasoning = String::new();
    let mut pending_tool_calls: HashMap<u32, PendingToolCall> = HashMap::new();
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("failed to read SSE chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        let mut remaining = String::new();
        for line in buffer.split('\n') {
            if !line.starts_with("data: ") {
                // Keep incomplete lines in the buffer
                if !line.is_empty() && !line.starts_with(':') && !line.starts_with("event:") {
                    remaining.push_str(line);
                    remaining.push('\n');
                }
                continue;
            }

            let data = line[6..].trim();
            if data == "[DONE]" {
                // Emit final chunk
                if !pending_tool_calls.is_empty() {
                    let tool_calls = drain_tool_calls(&mut pending_tool_calls);
                    let _ = tx
                        .send(Ok(CompletionChunk::ToolCalls {
                            tool_calls,
                            content: if total_content.is_empty() {
                                None
                            } else {
                                Some(total_content.clone())
                            },
                            reasoning: if total_reasoning.is_empty() {
                                None
                            } else {
                                Some(total_reasoning.clone())
                            },
                            input_tokens: Some(input_tokens),
                            output_tokens: Some(output_tokens),
                            response_id: None,
                            upstream_thread_id: None,
                        }))
                        .await;
                } else {
                    let _ = tx
                        .send(Ok(CompletionChunk::Done {
                            content: total_content.clone(),
                            reasoning: if total_reasoning.is_empty() {
                                None
                            } else {
                                Some(total_reasoning.clone())
                            },
                            input_tokens,
                            output_tokens,
                            response_id: None,
                            upstream_thread_id: None,
                        }))
                        .await;
                }
                return Ok(());
            }

            let parsed: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract usage
            if let Some(usage) = parsed.get("usage") {
                input_tokens = usage
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(input_tokens);
                output_tokens = usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(output_tokens);
            }

            let delta = match parsed.pointer("/choices/0/delta") {
                Some(d) => d,
                None => continue,
            };

            // Content delta
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                total_content.push_str(content);
                let _ = tx
                    .send(Ok(CompletionChunk::Delta {
                        content: content.into(),
                        reasoning: None,
                    }))
                    .await;
            }

            // Reasoning delta (covers delta.reasoning and delta.reasoning_content)
            let reasoning_text = delta
                .get("reasoning")
                .and_then(|v| v.as_str())
                .or_else(|| delta.get("reasoning_content").and_then(|v| v.as_str()));
            if let Some(reasoning) = reasoning_text {
                total_reasoning.push_str(reasoning);
                let _ = tx
                    .send(Ok(CompletionChunk::Delta {
                        content: String::new(),
                        reasoning: Some(reasoning.into()),
                    }))
                    .await;
            }

            // Tool call deltas (streamed incrementally)
            if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let entry = pending_tool_calls
                        .entry(idx)
                        .or_insert_with(|| PendingToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                        });

                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                        entry.id = id.into();
                    }
                    if let Some(name) = tc.pointer("/function/name").and_then(|v| v.as_str()) {
                        entry.name.push_str(name);
                    }
                    if let Some(args) = tc.pointer("/function/arguments").and_then(|v| v.as_str()) {
                        entry.arguments.push_str(args);
                    }
                }
            }
        }
        buffer = remaining;
    }

    // Stream ended without [DONE]
    if !pending_tool_calls.is_empty() {
        let tool_calls = drain_tool_calls(&mut pending_tool_calls);
        let _ = tx
            .send(Ok(CompletionChunk::ToolCalls {
                tool_calls,
                content: if total_content.is_empty() {
                    None
                } else {
                    Some(total_content)
                },
                reasoning: if total_reasoning.is_empty() {
                    None
                } else {
                    Some(total_reasoning)
                },
                input_tokens: Some(input_tokens),
                output_tokens: Some(output_tokens),
                response_id: None,
                upstream_thread_id: None,
            }))
            .await;
    } else {
        let _ = tx
            .send(Ok(CompletionChunk::Done {
                content: total_content,
                reasoning: if total_reasoning.is_empty() {
                    None
                } else {
                    Some(total_reasoning)
                },
                input_tokens,
                output_tokens,
                response_id: None,
                upstream_thread_id: None,
            }))
            .await;
    }

    Ok(())
}

async fn parse_openai_responses_sse(
    response: reqwest::Response,
    provider: &str,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut total_content = String::new();
    let mut total_reasoning = String::new();
    let mut pending_tool_calls: HashMap<u32, PendingToolCall> = HashMap::new();
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut response_id: Option<String> = None;
    let mut saw_any_json = false;
    let mut saw_responses_event = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("failed to read Responses SSE chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        let mut remaining = String::new();
        for line in buffer.split('\n') {
            if !line.starts_with("data: ") {
                if !line.is_empty() && !line.starts_with(':') && !line.starts_with("event:") {
                    remaining.push_str(line);
                    remaining.push('\n');
                }
                continue;
            }

            let data = line[6..].trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let parsed: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            saw_any_json = true;

            if parsed.get("choices").is_some() {
                return Err(TransportCompatibilityError {
                    provider: provider.to_string(),
                    details: "endpoint returned Chat Completions events for a Responses request"
                        .to_string(),
                }
                .into());
            }

            let event_type = parsed
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if event_type.starts_with("response.") || event_type == "error" {
                saw_responses_event = true;
            }

            match event_type {
                "response.created" => {
                    response_id = parsed
                        .pointer("/response/id")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned);
                }
                "response.output_text.delta" => {
                    if let Some(delta) = parsed.get("delta").and_then(|value| value.as_str()) {
                        total_content.push_str(delta);
                        let _ = tx
                            .send(Ok(CompletionChunk::Delta {
                                content: delta.to_string(),
                                reasoning: None,
                            }))
                            .await;
                    }
                }
                "response.reasoning_summary_text.delta" => {
                    if let Some(delta) = parsed.get("delta").and_then(|value| value.as_str()) {
                        total_reasoning.push_str(delta);
                        let _ = tx
                            .send(Ok(CompletionChunk::Delta {
                                content: String::new(),
                                reasoning: Some(delta.to_string()),
                            }))
                            .await;
                    }
                }
                "response.output_item.added" | "response.output_item.done" => {
                    let output_index = parsed
                        .get("output_index")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0) as u32;
                    if let Some(item) = parsed.get("item") {
                        if item.get("type").and_then(|value| value.as_str())
                            == Some("function_call")
                        {
                            let entry =
                                pending_tool_calls.entry(output_index).or_insert_with(|| {
                                    PendingToolCall {
                                        id: String::new(),
                                        name: String::new(),
                                        arguments: String::new(),
                                    }
                                });
                            if let Some(call_id) =
                                item.get("call_id").and_then(|value| value.as_str())
                            {
                                entry.id = call_id.to_string();
                            }
                            if let Some(name) = item.get("name").and_then(|value| value.as_str()) {
                                entry.name = name.to_string();
                            }
                            if let Some(arguments) =
                                item.get("arguments").and_then(|value| value.as_str())
                            {
                                entry.arguments = arguments.to_string();
                            }
                        }
                    }
                }
                "response.function_call_arguments.delta" => {
                    let output_index = parsed
                        .get("output_index")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0) as u32;
                    if let Some(delta) = parsed.get("delta").and_then(|value| value.as_str()) {
                        let entry = pending_tool_calls.entry(output_index).or_insert_with(|| {
                            PendingToolCall {
                                id: String::new(),
                                name: String::new(),
                                arguments: String::new(),
                            }
                        });
                        entry.arguments.push_str(delta);
                    }
                }
                "response.completed" | "response.incomplete" => {
                    if let Some(usage) = parsed.pointer("/response/usage") {
                        input_tokens = usage
                            .get("input_tokens")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(input_tokens);
                        output_tokens = usage
                            .get("output_tokens")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(output_tokens);
                    }

                    if !pending_tool_calls.is_empty() {
                        let tool_calls = drain_tool_calls(&mut pending_tool_calls);
                        let _ = tx
                            .send(Ok(CompletionChunk::ToolCalls {
                                tool_calls,
                                content: if total_content.is_empty() {
                                    None
                                } else {
                                    Some(total_content.clone())
                                },
                                reasoning: if total_reasoning.is_empty() {
                                    None
                                } else {
                                    Some(total_reasoning.clone())
                                },
                                input_tokens: Some(input_tokens),
                                output_tokens: Some(output_tokens),
                                response_id: response_id.clone(),
                                upstream_thread_id: None,
                            }))
                            .await;
                    } else {
                        let _ = tx
                            .send(Ok(CompletionChunk::Done {
                                content: total_content.clone(),
                                reasoning: if total_reasoning.is_empty() {
                                    None
                                } else {
                                    Some(total_reasoning.clone())
                                },
                                input_tokens,
                                output_tokens,
                                response_id: response_id.clone(),
                                upstream_thread_id: None,
                            }))
                            .await;
                    }
                    return Ok(());
                }
                "error" => {
                    let message = parsed
                        .get("message")
                        .and_then(|value| value.as_str())
                        .unwrap_or("Responses API error")
                        .to_string();
                    let _ = tx.send(Ok(CompletionChunk::Error { message })).await;
                    return Ok(());
                }
                _ => {}
            }
        }
        buffer = remaining;
    }

    if saw_any_json && !saw_responses_event {
        return Err(TransportCompatibilityError {
            provider: provider.to_string(),
            details: "stream did not contain recognizable Responses API events".to_string(),
        }
        .into());
    }

    if !pending_tool_calls.is_empty() {
        let tool_calls = drain_tool_calls(&mut pending_tool_calls);
        let _ = tx
            .send(Ok(CompletionChunk::ToolCalls {
                tool_calls,
                content: if total_content.is_empty() {
                    None
                } else {
                    Some(total_content)
                },
                reasoning: if total_reasoning.is_empty() {
                    None
                } else {
                    Some(total_reasoning)
                },
                input_tokens: Some(input_tokens),
                output_tokens: Some(output_tokens),
                response_id,
                upstream_thread_id: None,
            }))
            .await;
    } else {
        let _ = tx
            .send(Ok(CompletionChunk::Done {
                content: total_content,
                reasoning: if total_reasoning.is_empty() {
                    None
                } else {
                    Some(total_reasoning)
                },
                input_tokens,
                output_tokens,
                response_id,
                upstream_thread_id: None,
            }))
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Anthropic Messages API implementation
// ---------------------------------------------------------------------------

async fn run_anthropic(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));

    // Convert messages to Anthropic format
    let anthropic_messages = build_anthropic_messages(messages);

    let mut body = serde_json::json!({
        "model": config.model,
        "max_tokens": 4096,
        "system": system_prompt,
        "messages": anthropic_messages,
        "stream": true,
    });

    if !tools.is_empty() {
        let anthropic_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                })
            })
            .collect();
        body["tools"] = serde_json::json!(anthropic_tools);
    }

    if let Some(budget_tokens) = anthropic_thinking_budget(&config.reasoning_effort) {
        if config.model.starts_with("claude")
            || (provider == "alibaba-coding-plan"
                && dashscope_openai_uses_enable_thinking(provider, &config.model))
        {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget_tokens,
            });
        }
    }

    let mut request = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", &config.api_key);
    if !is_dashscope_coding_plan_anthropic_base_url(&config.base_url) {
        request = request.header("anthropic-version", "2023-06-01");
    }
    let response = apply_dashscope_coding_plan_sdk_headers(
        request,
        provider,
        &config.base_url,
        ApiType::Anthropic,
    )
    .body(body.to_string())
    .send()
    .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(check_rate_limit_response(status, "Anthropic", &text));
    }

    parse_anthropic_sse(response, tx).await
}

async fn parse_anthropic_sse(
    response: reqwest::Response,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut total_content = String::new();
    let mut total_reasoning = String::new();
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;

    // Anthropic tool use tracking
    let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
    let mut current_tool_input = String::new();
    // Track whether the current content block is a thinking block
    let mut in_thinking_block = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("failed to read Anthropic SSE chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        let mut remaining = String::new();
        for line in buffer.split('\n') {
            if !line.starts_with("data: ") {
                if !line.is_empty() && !line.starts_with(':') && !line.starts_with("event:") {
                    remaining.push_str(line);
                    remaining.push('\n');
                }
                continue;
            }

            let data = line[6..].trim();
            let parsed: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match event_type {
                "message_start" => {
                    input_tokens = parsed
                        .pointer("/message/usage/input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                }
                "content_block_start" => {
                    if let Some(cb) = parsed.get("content_block") {
                        let block_type = cb.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match block_type {
                            "tool_use" => {
                                in_thinking_block = false;
                                let id = cb
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = cb
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                pending_tool_calls.push(PendingToolCall {
                                    id,
                                    name,
                                    arguments: String::new(),
                                });
                                current_tool_input.clear();
                            }
                            "thinking" => {
                                in_thinking_block = true;
                            }
                            _ => {
                                in_thinking_block = false;
                            }
                        }
                    }
                }
                "content_block_delta" => {
                    let delta_type = parsed
                        .pointer("/delta/type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if delta_type == "text_delta" {
                        if in_thinking_block {
                            // Thinking block text delivered as text_delta
                            let text = parsed
                                .pointer("/delta/text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !text.is_empty() {
                                total_reasoning.push_str(text);
                                let _ = tx
                                    .send(Ok(CompletionChunk::Delta {
                                        content: String::new(),
                                        reasoning: Some(text.into()),
                                    }))
                                    .await;
                            }
                        } else {
                            let text = parsed
                                .pointer("/delta/text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !text.is_empty() {
                                total_content.push_str(text);
                                let _ = tx
                                    .send(Ok(CompletionChunk::Delta {
                                        content: text.into(),
                                        reasoning: None,
                                    }))
                                    .await;
                            }
                        }
                    } else if delta_type == "thinking_delta" {
                        // Anthropic extended thinking delta
                        let thinking = parsed
                            .pointer("/delta/thinking")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !thinking.is_empty() {
                            total_reasoning.push_str(thinking);
                            let _ = tx
                                .send(Ok(CompletionChunk::Delta {
                                    content: String::new(),
                                    reasoning: Some(thinking.into()),
                                }))
                                .await;
                        }
                    } else if delta_type == "input_json_delta" {
                        let partial = parsed
                            .pointer("/delta/partial_json")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        current_tool_input.push_str(partial);
                    }
                }
                "content_block_stop" => {
                    // Finalize tool call arguments if we're in a tool block
                    if let Some(tc) = pending_tool_calls.last_mut() {
                        if tc.arguments.is_empty() && !current_tool_input.is_empty() {
                            tc.arguments = current_tool_input.clone();
                            current_tool_input.clear();
                        }
                    }
                    in_thinking_block = false;
                }
                "message_delta" => {
                    output_tokens = parsed
                        .pointer("/usage/output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(output_tokens);
                }
                "message_stop" => {
                    let final_reasoning = if total_reasoning.is_empty() {
                        None
                    } else {
                        Some(total_reasoning.clone())
                    };
                    if !pending_tool_calls.is_empty() {
                        let tool_calls: Vec<ToolCall> = pending_tool_calls
                            .drain(..)
                            .enumerate()
                            .map(|(index, tc)| ToolCall {
                                id: if tc.id.trim().is_empty() {
                                    synthesize_tool_call_id("anthropic", index, &tc.name)
                                } else {
                                    tc.id
                                },
                                function: ToolFunction {
                                    name: tc.name,
                                    arguments: tc.arguments,
                                },
                            })
                            .collect();
                        let _ = tx
                            .send(Ok(CompletionChunk::ToolCalls {
                                tool_calls,
                                content: if total_content.is_empty() {
                                    None
                                } else {
                                    Some(total_content.clone())
                                },
                                reasoning: final_reasoning,
                                input_tokens: Some(input_tokens),
                                output_tokens: Some(output_tokens),
                                response_id: None,
                                upstream_thread_id: None,
                            }))
                            .await;
                    } else {
                        let _ = tx
                            .send(Ok(CompletionChunk::Done {
                                content: total_content.clone(),
                                reasoning: final_reasoning,
                                input_tokens,
                                output_tokens,
                                response_id: None,
                                upstream_thread_id: None,
                            }))
                            .await;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        buffer = remaining;
    }

    // Stream ended without message_stop
    let _ = tx
        .send(Ok(CompletionChunk::Done {
            content: total_content,
            reasoning: if total_reasoning.is_empty() {
                None
            } else {
                Some(total_reasoning)
            },
            input_tokens,
            output_tokens,
            response_id: None,
            upstream_thread_id: None,
        }))
        .await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn synthesize_tool_call_id(seed: &str, index: usize, name: &str) -> String {
    let clean_name: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("synthetic_tool_call_{seed}_{index}_{clean_name}")
}

fn drain_tool_calls(map: &mut HashMap<u32, PendingToolCall>) -> Vec<ToolCall> {
    let mut entries: Vec<(u32, PendingToolCall)> = map.drain().collect();
    entries.sort_by_key(|(idx, _)| *idx);
    entries
        .into_iter()
        .map(|(idx, tc)| ToolCall {
            id: if tc.id.trim().is_empty() {
                synthesize_tool_call_id("openai", idx as usize, &tc.name)
            } else {
                tc.id
            },
            function: ToolFunction {
                name: tc.name,
                arguments: tc.arguments,
            },
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Model fetching
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedModel {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
}

pub async fn fetch_models(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<FetchedModel>> {
    let def = super::types::get_provider_definition(provider_id);

    if !def.map(|d| d.supports_model_fetch).unwrap_or(false) {
        return Err(anyhow::anyhow!(
            "Provider '{}' does not support model fetching",
            provider_id
        ));
    }

    let client = reqwest::Client::new();
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let mut req = client.get(&url).header("Content-Type", "application/json");

    if !api_key.is_empty() {
        let auth_method = def.map(|d| d.auth_method).unwrap_or(AuthMethod::Bearer);
        match auth_method {
            AuthMethod::Bearer => {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            AuthMethod::XApiKey => {
                req = req.header("x-api-key", api_key);
            }
        }
    }

    let response = req.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch models: {} - {}",
            status,
            text
        ));
    }

    let json: serde_json::Value = response.json().await?;

    let models = json
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    let name = m
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string());

                    let context_window = m
                        .get("context_length")
                        .or_else(|| m.get("context_window"))
                        .and_then(|c| c.as_u64())
                        .map(|n| n as u32);

                    Some(FetchedModel {
                        id,
                        name,
                        context_window,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

pub async fn validate_provider_connection(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    auth_source: AuthSource,
) -> Result<Option<Vec<FetchedModel>>> {
    let def = get_provider_definition(provider_id)
        .with_context(|| format!("Unknown provider '{}'", provider_id))?;
    let resolved_base_url = if base_url.trim().is_empty() {
        def.default_base_url.to_string()
    } else {
        base_url.trim().to_string()
    };

    if provider_id == "openai" && auth_source == AuthSource::ChatgptSubscription {
        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: resolved_base_url,
            model: def.default_model.to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source,
            api_transport: def.default_transport,
            reasoning_effort: "off".to_string(),
            context_window_tokens: def
                .models
                .iter()
                .find(|model| model.id == def.default_model)
                .map(|model| model.context_window)
                .unwrap_or(128_000),
            response_schema: None,
        };
        let _ = resolve_openai_codex_request_auth(&client, provider_id, &config).await?;
        return Ok(None);
    }

    // Always validate via a minimal chat completion — this tests both connectivity
    // AND the API key (fetch_models doesn't require auth on some providers like OpenRouter).
    let client = reqwest::Client::new();
    let api_type = get_provider_api_type(provider_id, def.default_model, &resolved_base_url);
    let request = match api_type {
        ApiType::OpenAI => {
            let url = build_chat_completion_url(&resolved_base_url);
            let body = serde_json::json!({
                "model": def.default_model,
                "messages": [{ "role": "user", "content": "ok" }],
                "max_tokens": 1,
                "stream": false,
            });
            let mut req = client.post(url).header("Content-Type", "application/json");
            if !api_key.is_empty() {
                match def.auth_method {
                    AuthMethod::Bearer => {
                        req = req.header("Authorization", format!("Bearer {}", api_key));
                    }
                    AuthMethod::XApiKey => {
                        req = req.header("x-api-key", api_key);
                    }
                }
            }
            apply_dashscope_coding_plan_sdk_headers(req, provider_id, &resolved_base_url, api_type)
                .json(&body)
        }
        ApiType::Anthropic => {
            let url = format!("{}/v1/messages", resolved_base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": def.default_model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "ok" }],
            });
            let mut req = client.post(url).header("Content-Type", "application/json");
            if !is_dashscope_coding_plan_anthropic_base_url(&resolved_base_url) {
                req = req.header("anthropic-version", "2023-06-01");
            }
            if !api_key.is_empty() {
                match def.auth_method {
                    AuthMethod::Bearer => {
                        req = req.header("Authorization", format!("Bearer {}", api_key));
                    }
                    AuthMethod::XApiKey => {
                        req = req.header("x-api-key", api_key);
                    }
                }
            }
            apply_dashscope_coding_plan_sdk_headers(req, provider_id, &resolved_base_url, api_type)
                .json(&body)
        }
    };

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Validation failed: {} - {}", status, text);
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{AgentMessage, MessageRole, ToolCall, ToolFunction};

    #[test]
    fn anthropic_groups_consecutive_tool_results_into_one_user_message() {
        let messages = vec![
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text("Checking both".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: Some(vec![
                    ApiToolCall {
                        id: "call_1".to_string(),
                        call_type: "function".to_string(),
                        function: ApiToolCallFunction {
                            name: "list_files".to_string(),
                            arguments: "{\"path\":\".\"}".to_string(),
                        },
                    },
                    ApiToolCall {
                        id: "call_2".to_string(),
                        call_type: "function".to_string(),
                        function: ApiToolCallFunction {
                            name: "read_file".to_string(),
                            arguments: "{\"path\":\"README.md\"}".to_string(),
                        },
                    },
                ]),
            },
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("file list".to_string()),
                tool_call_id: Some("call_1".to_string()),
                name: Some("list_files".to_string()),
                tool_calls: None,
            },
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("readme contents".to_string()),
                tool_call_id: Some("call_2".to_string()),
                name: Some("read_file".to_string()),
                tool_calls: None,
            },
        ];

        let anthropic = build_anthropic_messages(&messages);
        assert_eq!(anthropic.len(), 2);
        assert_eq!(anthropic[0]["role"], "assistant");
        assert_eq!(anthropic[1]["role"], "user");
        let blocks = anthropic[1]["content"]
            .as_array()
            .expect("tool results should be grouped into one block array");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "tool_result");
        assert_eq!(blocks[0]["tool_use_id"], "call_1");
        assert_eq!(blocks[1]["tool_use_id"], "call_2");
    }

    #[test]
    fn messages_to_api_format_keeps_reused_tool_ids_across_turns() {
        let messages = vec![
            AgentMessage {
                role: MessageRole::Assistant,
                content: "first".to_string(),
                tool_calls: Some(vec![ToolCall {
                    id: "2013".to_string(),
                    function: ToolFunction {
                        name: "tool_a".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 1,
            },
            AgentMessage {
                role: MessageRole::Tool,
                content: "ok 1".to_string(),
                tool_calls: None,
                tool_call_id: Some("2013".to_string()),
                tool_name: Some("tool_a".to_string()),
                tool_arguments: Some("{}".to_string()),
                tool_status: Some("done".to_string()),
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 2,
            },
            AgentMessage::user("next", 3),
            AgentMessage {
                role: MessageRole::Assistant,
                content: "second".to_string(),
                tool_calls: Some(vec![ToolCall {
                    id: "2013".to_string(),
                    function: ToolFunction {
                        name: "tool_b".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 4,
            },
            AgentMessage {
                role: MessageRole::Tool,
                content: "ok 2".to_string(),
                tool_calls: None,
                tool_call_id: Some("2013".to_string()),
                tool_name: Some("tool_b".to_string()),
                tool_arguments: Some("{}".to_string()),
                tool_status: Some("done".to_string()),
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 5,
            },
        ];

        let api_messages = messages_to_api_format(&messages);
        let tool_results = api_messages
            .iter()
            .filter(|message| message.role == "tool")
            .count();
        assert_eq!(tool_results, 2);
    }

    #[test]
    fn messages_to_api_format_normalizes_empty_tool_ids() {
        let messages = vec![
            AgentMessage {
                role: MessageRole::Assistant,
                content: String::new(),
                tool_calls: Some(vec![ToolCall {
                    id: String::new(),
                    function: ToolFunction {
                        name: "list_sessions".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 42,
            },
            AgentMessage {
                role: MessageRole::Tool,
                content: "ok".to_string(),
                tool_calls: None,
                tool_call_id: Some(String::new()),
                tool_name: Some("list_sessions".to_string()),
                tool_arguments: Some("{}".to_string()),
                tool_status: Some("done".to_string()),
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 43,
            },
        ];

        let api_messages = messages_to_api_format(&messages);
        assert_eq!(api_messages.len(), 2);
        let assistant_tool_id = api_messages[0]
            .tool_calls
            .as_ref()
            .and_then(|calls| calls.first())
            .map(|call| call.id.clone())
            .expect("assistant tool call should have normalized id");
        assert!(!assistant_tool_id.is_empty());
        assert_eq!(api_messages[1].tool_call_id.as_deref(), Some(assistant_tool_id.as_str()));
    }

    #[test]
    fn chat_completion_messages_null_assistant_content_for_tool_calls() {
        let messages = vec![ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text("I'll inspect that now".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "list_sessions".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }];

        let serialized =
            build_chat_completion_messages("system prompt", &messages).expect("serialize");
        assert_eq!(serialized.len(), 2);
        assert_eq!(serialized[1]["role"], "assistant");
        assert!(serialized[1]["content"].is_null());
        assert_eq!(serialized[1]["tool_calls"][0]["id"], "call_1");
    }
}
