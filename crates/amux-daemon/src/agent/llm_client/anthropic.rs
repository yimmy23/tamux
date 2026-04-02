fn anthropic_messages_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

fn provider_requires_fresh_anthropic_connection(provider: &str) -> bool {
    matches!(provider, "minimax" | "minimax-coding-plan")
}

fn build_fresh_anthropic_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .http1_only()
        .pool_max_idle_per_host(0)
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(125))
        .build()
        .map_err(Into::into)
}

fn short_sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    format!("{:x}", digest)[..16].to_string()
}

fn redacted_header_value(name: &str, value: &reqwest::header::HeaderValue) -> String {
    match name {
        "authorization" | "x-api-key" | "proxy-authorization" | "cookie" | "set-cookie" => {
            "<redacted>".to_string()
        }
        _ => value
            .to_str()
            .map(|text| text.to_string())
            .unwrap_or_else(|_| "<binary>".to_string()),
    }
}

fn anthropic_request_fingerprint(request: &reqwest::Request) -> String {
    let mut header_lines: Vec<String> = request
        .headers()
        .iter()
        .map(|(name, value)| {
            format!(
                "{}:{}",
                name.as_str(),
                redacted_header_value(name.as_str(), value)
            )
        })
        .collect();
    header_lines.sort();

    let body_bytes = request.body().and_then(|body| body.as_bytes()).unwrap_or(&[]);
    let canonical = format!(
        "method={}\nurl={}\nversion={:?}\nheaders={}\nbody_sha256={}\nbody_len={}",
        request.method(),
        request.url(),
        request.version(),
        header_lines.join("\n"),
        short_sha256_hex(body_bytes),
        body_bytes.len()
    );

    short_sha256_hex(canonical.as_bytes())
}

fn build_anthropic_request(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
) -> Result<reqwest::Request> {
    let url = anthropic_messages_url(&config.base_url);

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

    let auth_method = get_provider_definition(provider)
        .map(|d| d.auth_method)
        .unwrap_or(AuthMethod::XApiKey);
    let mut request = auth_method.apply(
        client.post(&url).header("Content-Type", "application/json"),
        &config.api_key,
    );
    if !is_dashscope_coding_plan_anthropic_base_url(&config.base_url) {
        request = request.header("anthropic-version", "2023-06-01");
    }
    if needs_coding_plan_sdk_headers(provider) {
        request = request.header(
            "anthropic-beta",
            "fine-grained-tool-streaming-2025-05-14,interleaved-thinking-2025-05-14",
        );
    }
    request = apply_dashscope_coding_plan_sdk_headers(
        request,
        provider,
        &config.base_url,
        ApiType::Anthropic,
    )
    .body(body.to_string());

    if provider_requires_fresh_anthropic_connection(provider) {
        request = request
            .header(reqwest::header::CONNECTION, "close")
            .version(reqwest::Version::HTTP_11);
    }

    request.build().map_err(Into::into)
}

async fn run_anthropic(
    client: &reqwest::Client,
    provider: &str,
    attempt: u32,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let client = if provider_requires_fresh_anthropic_connection(provider) {
        build_fresh_anthropic_http_client()?
    } else {
        client.clone()
    };
    let request =
        build_anthropic_request(&client, provider, config, system_prompt, messages, tools)?;
    let request_fingerprint = anthropic_request_fingerprint(&request);
    tracing::info!(
        provider = %provider,
        model = %config.model,
        attempt,
        url = %request.url(),
        version = ?request.version(),
        request_fingerprint = %request_fingerprint,
        connection_close = request
            .headers()
            .get(reqwest::header::CONNECTION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or(""),
        tool_count = tools.len(),
        "dispatching anthropic request"
    );
    let response = client.execute(request).await?;
    tracing::info!(
        provider = %provider,
        model = %config.model,
        attempt,
        request_fingerprint = %request_fingerprint,
        status = %response.status(),
        "anthropic response headers received"
    );

    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let text = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        tracing::warn!(
            provider = %provider,
            model = %config.model,
            attempt,
            request_fingerprint = %request_fingerprint,
            status = %status,
            retry_after_ms = retry_after_ms
                .or_else(|| extract_retry_after_ms(None, &text))
                .unwrap_or(0),
            body = %summarize_upstream_body(&text),
            "anthropic request returned non-success response"
        );
        return Err(classify_http_failure_with_retry_after(
            status,
            "Anthropic",
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
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
                            .map(|(index, tc)| {
                                ToolCall::with_default_weles_review(
                                    if tc.id.trim().is_empty() {
                                        synthesize_tool_call_id("anthropic", index, &tc.name)
                                    } else {
                                        tc.id
                                    },
                                    ToolFunction {
                                        name: tc.name,
                                        arguments: tc.arguments,
                                    },
                                )
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
