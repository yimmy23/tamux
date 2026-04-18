use super::*;
use crate::agent::provider_resolution::apply_provider_model_override;

impl AgentEngine {
    /// Minimal LLM call for memory refinement. Uses the operator's configured provider/model.
    /// Short context, focused prompt -- minimal token cost.
    ///
    /// Pattern follows memory_flush.rs:
    /// 1. Build ApiMessage vec (system + user)
    /// 2. Get ProviderConfig for the configured provider
    /// 3. Call send_completion_request with empty tools, Chat transport
    /// 4. Collect text content from Delta/Done chunks in the stream
    /// 5. Return concatenated response text
    pub(in crate::agent) async fn send_refinement_llm_call(
        &self,
        config: &AgentConfig,
        user_prompt: &str,
    ) -> anyhow::Result<String> {
        use futures::StreamExt;

        let active_scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let (provider_id, provider_config) =
            if active_scope_id == crate::agent::agent_identity::WELES_AGENT_ID {
                let provider_id = config
                    .builtin_sub_agents
                    .weles
                    .provider
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(config.provider.as_str())
                    .to_string();
                let model = config
                    .builtin_sub_agents
                    .weles
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(config.model.as_str())
                    .to_string();
                let mut provider_config =
                    self.resolve_sub_agent_provider_config(config, &provider_id)?;
                if !model.is_empty() {
                    apply_provider_model_override(&provider_id, &mut provider_config, &model);
                }
                if let Some(reasoning_effort) = config
                    .builtin_sub_agents
                    .weles
                    .reasoning_effort
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    provider_config.reasoning_effort = reasoning_effort.to_string();
                }
                (provider_id, provider_config)
            } else {
                (
                    config.provider.clone(),
                    resolve_active_provider_config(config)?,
                )
            };

        let system = "You are a concise memory consolidation agent. Respond with ONLY the merged fact, nothing else.";

        let messages = vec![ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text(user_prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];

        let mut stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            system,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );

        let mut response = String::new();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(CompletionChunk::Delta { content, .. }) => {
                    response.push_str(&content);
                }
                Ok(CompletionChunk::Done { content, .. }) => {
                    if !content.is_empty() {
                        response.push_str(&content);
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    return Err(anyhow::anyhow!("refinement LLM error: {}", message));
                }
                Ok(_) => {}
                Err(error) => {
                    return Err(anyhow::anyhow!("refinement LLM stream error: {}", error));
                }
            }
        }

        Ok(response)
    }
}
