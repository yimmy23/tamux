use super::*;

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

impl AgentEngine {
    pub(super) async fn run_goal_llm_raw(&self, prompt: &str) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let provider_config = self.resolve_provider_config(&config)?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        self.check_circuit_breaker(&config.provider).await?;
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
                    ..
                } => {
                    self.record_llm_outcome(&config.provider, true).await;
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

    pub(super) async fn run_goal_llm_json(&self, prompt: &str) -> Result<String> {
        self.run_goal_llm_json_with_schema(
            prompt,
            goal_plan_json_schema(),
            "goal planning LLM call",
        )
        .await
    }

    pub(super) async fn run_goal_llm_json_with_schema(
        &self,
        prompt: &str,
        schema: serde_json::Value,
        log_label: &str,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let mut provider_config = self.resolve_provider_config(&config)?;
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
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        self.check_circuit_breaker(&config.provider).await?;
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
                    ..
                } => {
                    self.record_llm_outcome(&config.provider, true).await;
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
}
