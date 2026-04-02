use super::*;

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

        let provider_config = resolve_active_provider_config(config)?;

        let system =
            "You are a concise memory consolidation agent. Respond with ONLY the merged fact, nothing else.";

        let messages = vec![ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text(user_prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];

        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
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
