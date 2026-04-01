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
                return Err(transport_incompatibility_error(
                    provider,
                    "endpoint returned Chat Completions events for a Responses request",
                ));
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
                        match item.get("type").and_then(|value| value.as_str()) {
                            Some("function_call") => {
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
                                if let Some(name) =
                                    item.get("name").and_then(|value| value.as_str())
                                {
                                    entry.name = name.to_string();
                                }
                                if let Some(arguments) =
                                    item.get("arguments").and_then(|value| value.as_str())
                                {
                                    entry.arguments = arguments.to_string();
                                }
                            }
                            Some("reasoning") if event_type == "response.output_item.done" => {
                                if total_reasoning.trim().is_empty() {
                                    if let Some(summary_text) = extract_reasoning_summary_text(item)
                                    {
                                        total_reasoning = summary_text;
                                    }
                                }
                            }
                            _ => {}
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
        return Err(transport_incompatibility_error(
            provider,
            "stream did not contain recognizable Responses API events",
        ));
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
