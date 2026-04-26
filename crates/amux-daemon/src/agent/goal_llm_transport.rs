use super::*;
use amux_shared::providers::{PROVIDER_ID_DEEPSEEK, PROVIDER_ID_OPENROUTER};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoalLlmRequestMode {
    Default,
    ReplanFollowUp,
}

fn adjust_goal_llm_provider_config_for_mode(
    provider: &str,
    provider_config: &mut ProviderConfig,
    mode: GoalLlmRequestMode,
) {
    if mode != GoalLlmRequestMode::ReplanFollowUp {
        return;
    }

    if provider_config.api_transport != ApiTransport::ChatCompletions {
        return;
    }

    if matches!(provider, PROVIDER_ID_OPENROUTER | PROVIDER_ID_DEEPSEEK) {
        provider_config.reasoning_effort = "off".to_string();
    }
}

pub(super) fn orchestrator_policy_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["continue", "pivot", "escalate", "halt_retries"]
            },
            "reason": {
                "type": "string",
                "maxLength": 240
            },
            "strategy_hint": {
                "type": ["string", "null"],
                "maxLength": 160
            }
        },
        "required": ["action", "reason", "strategy_hint"],
        "additionalProperties": false
    })
}

fn structured_output_rejection_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("response_format")
        || lower.contains("structured_output")
        || lower.contains("json_schema")
        || lower.contains("invalid_json_schema")
        || lower.contains("text.format.schema")
}

fn should_fallback_from_structured_output_error(err: &anyhow::Error) -> bool {
    if let Some(structured) =
        crate::agent::llm_client::parse_structured_upstream_failure(&err.to_string())
    {
        if structured.class != "request_invalid" {
            return false;
        }
        let raw_message = structured
            .diagnostics
            .get("raw_message")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let body = structured
            .diagnostics
            .get("body")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        return structured_output_rejection_message(&structured.summary)
            || structured_output_rejection_message(raw_message)
            || structured_output_rejection_message(body);
    }

    structured_output_rejection_message(&err.to_string())
}

impl AgentEngine {
    pub(super) async fn run_goal_llm_raw_for_goal(
        &self,
        prompt: &str,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        self.run_goal_llm_raw_with_mode(prompt, GoalLlmRequestMode::Default, goal_run_id)
            .await
    }

    pub(super) async fn run_goal_llm_raw_for_replan(
        &self,
        prompt: &str,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        self.run_goal_llm_raw_with_mode(prompt, GoalLlmRequestMode::ReplanFollowUp, goal_run_id)
            .await
    }

    async fn run_goal_llm_raw_with_mode(
        &self,
        prompt: &str,
        mode: GoalLlmRequestMode,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let mut provider_config = self.resolve_provider_config(&config)?;
        adjust_goal_llm_provider_config_for_mode(&config.provider, &mut provider_config, mode);
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        self.check_circuit_breaker(&config.provider).await?;
        let request_started = std::time::Instant::now();
        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
            &provider_config,
            "Return structured data only. No markdown fences. No explanation.",
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::DurableRateLimited,
        );
        let mut content = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(value) => value,
                Err(error) => {
                    self.record_llm_outcome(&config.provider, false).await;
                    return Err(error);
                }
            };
            match chunk {
                CompletionChunk::Delta {
                    content: delta,
                    reasoning: reasoning_delta,
                } => {
                    content.push_str(&delta);
                    if let Some(reasoning_delta) = reasoning_delta {
                        reasoning.push_str(&reasoning_delta);
                    }
                }
                CompletionChunk::Done {
                    content: done,
                    reasoning: done_reasoning,
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    if let Some(goal_run_id) = goal_run_id {
                        self.accumulate_goal_run_cost_by_id(
                            goal_run_id,
                            input_tokens,
                            output_tokens,
                            &config.provider,
                            &provider_config.model,
                            Some(request_started.elapsed().as_millis() as u64),
                        )
                        .await;
                    }
                    if let Some(done_reasoning) = done_reasoning {
                        reasoning = done_reasoning;
                    }
                    let final_content = if done.is_empty() { content } else { done };
                    if !final_content.trim().is_empty() {
                        return Ok(final_content);
                    }
                    if !reasoning.trim().is_empty() {
                        return Ok(reasoning);
                    }
                    anyhow::bail!("goal LLM returned empty output");
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&config.provider, false).await;
                    anyhow::bail!(message);
                }
                CompletionChunk::TransportFallback { .. } => {}
                CompletionChunk::Retry { .. } => {}
                CompletionChunk::ToolCalls { .. } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    anyhow::bail!("goal planning unexpectedly returned tool calls");
                }
            }
        }
        if !content.trim().is_empty() {
            return Ok(content);
        }
        anyhow::bail!("goal LLM returned empty output")
    }

    pub(super) async fn run_goal_llm_json_for_goal(
        &self,
        prompt: &str,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        self.run_goal_llm_json_with_schema(
            prompt,
            goal_plan_json_schema(),
            "goal planning LLM call",
            goal_run_id,
        )
        .await
    }

    pub(super) async fn run_goal_llm_json_for_replan(
        &self,
        prompt: &str,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        self.run_goal_llm_json_with_schema_for_mode(
            prompt,
            goal_plan_json_schema(),
            "goal replan LLM call",
            GoalLlmRequestMode::ReplanFollowUp,
            goal_run_id,
        )
        .await
    }

    pub(super) async fn run_goal_llm_json_with_schema(
        &self,
        prompt: &str,
        schema: serde_json::Value,
        log_label: &str,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        self.run_goal_llm_json_with_schema_for_mode(
            prompt,
            schema,
            log_label,
            GoalLlmRequestMode::Default,
            goal_run_id,
        )
        .await
    }

    async fn run_goal_llm_json_with_schema_for_mode(
        &self,
        prompt: &str,
        schema: serde_json::Value,
        log_label: &str,
        mode: GoalLlmRequestMode,
        goal_run_id: Option<&str>,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let mut provider_config = self.resolve_provider_config(&config)?;
        adjust_goal_llm_provider_config_for_mode(&config.provider, &mut provider_config, mode);
        provider_config.response_schema = Some(schema);
        tracing::info!(
            provider = %config.provider,
            model = %provider_config.model,
            operation = log_label,
            "structured LLM call"
        );
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        self.check_circuit_breaker(&config.provider).await?;
        let request_started = std::time::Instant::now();
        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
            &provider_config,
            "Return strict JSON only. Do not call tools. Do not wrap the answer in markdown.",
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::DurableRateLimited,
        );
        let mut content = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(value) => value,
                Err(error) => {
                    self.record_llm_outcome(&config.provider, false).await;
                    if should_fallback_from_structured_output_error(&error) {
                        tracing::warn!(
                            provider = %config.provider,
                            model = %provider_config.model,
                            operation = log_label,
                            error = %error,
                            "structured output rejected upstream; falling back to unstructured goal request"
                        );
                        return self
                            .run_goal_llm_raw_with_mode(prompt, mode, goal_run_id)
                            .await;
                    }
                    return Err(error);
                }
            };
            match chunk {
                CompletionChunk::Delta {
                    content: delta,
                    reasoning: reasoning_delta,
                } => {
                    content.push_str(&delta);
                    if let Some(reasoning_delta) = reasoning_delta {
                        reasoning.push_str(&reasoning_delta);
                    }
                }
                CompletionChunk::Done {
                    content: done,
                    reasoning: done_reasoning,
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    if let Some(goal_run_id) = goal_run_id {
                        self.accumulate_goal_run_cost_by_id(
                            goal_run_id,
                            input_tokens,
                            output_tokens,
                            &config.provider,
                            &provider_config.model,
                            Some(request_started.elapsed().as_millis() as u64),
                        )
                        .await;
                    }
                    if let Some(done_reasoning) = done_reasoning {
                        reasoning = done_reasoning;
                    }
                    let final_content = if done.is_empty() { content } else { done };
                    if !final_content.trim().is_empty() && final_content.contains('{') {
                        return Ok(final_content);
                    }
                    if !reasoning.trim().is_empty() && reasoning.contains('{') {
                        tracing::info!("goal plan: extracting JSON from reasoning output");
                        return Ok(reasoning);
                    }
                    if !final_content.trim().is_empty() {
                        return Ok(final_content);
                    }
                    if !reasoning.trim().is_empty() {
                        return Ok(reasoning);
                    }
                    anyhow::bail!("goal planning returned empty output");
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&config.provider, false).await;
                    if structured_output_rejection_message(&message) {
                        tracing::warn!(
                            provider = %config.provider,
                            model = %provider_config.model,
                            operation = log_label,
                            error = %message,
                            "structured output rejected in stream; falling back to unstructured goal request"
                        );
                        return self
                            .run_goal_llm_raw_with_mode(prompt, mode, goal_run_id)
                            .await;
                    }
                    anyhow::bail!(message);
                }
                CompletionChunk::TransportFallback { .. } => {}
                CompletionChunk::Retry { .. } => {}
                CompletionChunk::ToolCalls { .. } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    anyhow::bail!("goal planning unexpectedly returned tool calls");
                }
            }
        }
        let final_content = content;
        if !final_content.trim().is_empty() && final_content.contains('{') {
            return Ok(final_content);
        }
        if !reasoning.trim().is_empty() && reasoning.contains('{') {
            return Ok(reasoning);
        }
        if !final_content.trim().is_empty() {
            return Ok(final_content);
        }
        anyhow::bail!("goal planning returned empty output")
    }

    pub(in crate::agent) async fn append_goal_memory_update(
        &self,
        goal_run_id: &str,
        update: &str,
    ) -> Result<()> {
        append_goal_memory_note(&self.data_dir, &self.history, update, Some(goal_run_id)).await?;
        self.refresh_memory_cache().await;
        Ok(())
    }

    pub(in crate::agent) async fn goal_thread_summary(&self, thread_id: &str) -> Option<String> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == MessageRole::Assistant && !message.content.trim().is_empty()
                })
                .map(|message| summarize_text(&message.content, 320))
        })
    }

    pub(in crate::agent) async fn goal_thread_latest_assistant_content(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == MessageRole::Assistant && !message.content.trim().is_empty()
                })
                .map(|message| message.content.trim().to_string())
        })
    }
}
