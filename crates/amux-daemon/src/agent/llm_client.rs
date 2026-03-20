//! LLM API client with SSE streaming support.
//!
//! Supports two API formats:
//! - OpenAI-compatible (`/chat/completions` with Bearer auth) — covers most providers
//! - Anthropic Messages API (`/v1/messages` with `x-api-key` header)

use anyhow::{Context, Result};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;
use tokio::sync::mpsc;

use super::types::{
    get_provider_api_type, get_provider_definition, ApiType, AuthMethod, CompletionChunk,
    ProviderConfig, ToolCall, ToolDefinition, ToolFunction,
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

impl Stream for CompletionStream {
    type Item = Result<CompletionChunk>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Send a chat completion request. Returns a stream of `CompletionChunk`.
pub fn send_chat_completion(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    retry_strategy: RetryStrategy,
) -> CompletionStream {
    let (tx, rx) = mpsc::channel(64);
    let client = client.clone();
    let provider = provider.to_string();
    let config = config.clone();
    let system_prompt = system_prompt.to_string();
    let messages = messages.to_vec();
    let tools = tools.to_vec();

    tokio::spawn(async move {
        let mut retry_attempt = 0u32;
        loop {
            let api_type = get_provider_api_type(&provider, &config.model);

            let result = if api_type == ApiType::Anthropic {
                run_anthropic(&client, &config, &system_prompt, &messages, &tools, &tx).await
            } else {
                run_openai_compatible(
                    &client,
                    &provider,
                    &config,
                    &system_prompt,
                    &messages,
                    &tools,
                    &tx,
                )
                .await
            };

            match result {
                Ok(()) => break,
                Err(e) => {
                    let is_rate_limited = e.downcast_ref::<RateLimitError>().is_some();
                    if is_rate_limited {
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
                            RetryStrategy::DurableRateLimited => {
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
        .map(|m| {
            let tool_calls = m.tool_calls.as_ref().map(|tcs| {
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
            });

            ApiMessage {
                role: match m.role {
                    super::types::MessageRole::System => "system".into(),
                    super::types::MessageRole::User => "user".into(),
                    super::types::MessageRole::Assistant => "assistant".into(),
                    super::types::MessageRole::Tool => "tool".into(),
                },
                content: ApiContent::Text(m.content.clone()),
                tool_call_id: m.tool_call_id.clone(),
                name: m.tool_name.clone(),
                tool_calls,
            }
        })
        .collect()
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
            | "alibaba-coding-plan"
            | "opencode-zen"
            | "z.ai"
            | "z.ai-coding-plan"
    ) || model.starts_with('o')
        || model.starts_with("gpt-5")
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

async fn run_openai_compatible(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let url = build_chat_completion_url(&config.base_url);

    let mut all_messages = vec![ApiMessage {
        role: "system".into(),
        content: ApiContent::Text(system_prompt.into()),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }];
    all_messages.extend_from_slice(messages);

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

    if let Some(effort) = normalize_reasoning_effort(&config.reasoning_effort) {
        // Always send reasoning_effort — the API ignores it for non-reasoning models
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
        // Also send the "reasoning" object for providers that use this format
        body["reasoning"] = serde_json::json!({ "effort": effort });
    }

    let mut req = client.post(&url).header("Content-Type", "application/json");

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
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));

    // Convert messages to Anthropic format
    let anthropic_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let role = match m.role.as_str() {
                "tool" | "system" => "user",
                r => r,
            };

            let content = if m.role == "tool" {
                serde_json::json!([{
                    "type": "tool_result",
                    "tool_use_id": m.tool_call_id,
                    "content": match &m.content {
                        ApiContent::Text(t) => t.as_str(),
                        _ => "",
                    }
                }])
            } else {
                match &m.content {
                    ApiContent::Text(t) => serde_json::json!(t),
                    ApiContent::Blocks(b) => serde_json::json!(b),
                }
            };

            serde_json::json!({
                "role": role,
                "content": content,
            })
        })
        .collect();

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
        if config.model.starts_with("claude") {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget_tokens,
            });
        }
    }

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
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
                            .map(|tc| ToolCall {
                                id: tc.id,
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
                            }))
                            .await;
                    } else {
                        let _ = tx
                            .send(Ok(CompletionChunk::Done {
                                content: total_content.clone(),
                                reasoning: final_reasoning,
                                input_tokens,
                                output_tokens,
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

fn drain_tool_calls(map: &mut HashMap<u32, PendingToolCall>) -> Vec<ToolCall> {
    let mut entries: Vec<(u32, PendingToolCall)> = map.drain().collect();
    entries.sort_by_key(|(idx, _)| *idx);
    entries
        .into_iter()
        .map(|(_, tc)| ToolCall {
            id: tc.id,
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
