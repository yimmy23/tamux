//! External agent communication — routing to openclaw/hermes backends.

use super::*;

impl AgentEngine {
    /// Route a message through an external agent process.
    pub(super) async fn send_message_external(
        &self,
        config: &AgentConfig,
        thread_id: Option<&str>,
        content: &str,
    ) -> Result<String> {
        let (tid, is_new_thread) = self.get_or_create_thread(thread_id, content).await;

        // Add user message
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread
                    .messages
                    .push(AgentMessage::user(content, now_millis()));
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(&tid).await;
        self.record_operator_message(&tid, content, is_new_thread)
            .await?;
        if let Err(error) = self.maybe_sync_thread_to_honcho(&tid).await {
            tracing::warn!(thread_id = %tid, error = %error, "failed to sync thread to Honcho");
        }

        let onecontext_bootstrap = if is_new_thread {
            self.onecontext_bootstrap_for_new_thread(content).await
        } else {
            None
        };

        let (stream_generation, stream_cancel_token) = self.begin_stream_cancellation(&tid).await;

        // Ensure tamux-mcp is configured in the external agent's MCP settings
        {
            let runners = self.external_runners.read().await;
            if let Some(runner) = runners.get(config.agent_backend.as_str()) {
                if !runner.has_tamux_mcp() {
                    external_runner::ensure_tamux_mcp_configured(config.agent_backend.as_str());
                }
            }
        }

        // Only inject tamux context on the first message in a thread
        // (subsequent messages in the same thread don't need the preamble
        // repeated — the external agent session carries the context)
        let is_first_message = {
            let threads = self.threads.read().await;
            threads
                .get(&tid)
                .map(|t| t.messages.len() <= 1) // 1 = just the user message we added above
                .unwrap_or(true)
        };

        let mut enriched_prompt = if is_first_message {
            let memory = self.memory.read().await;
            let memory_dir = active_memory_dir(&self.data_dir);
            let operator_model_summary = self.build_operator_model_prompt_summary().await;
            let operational_context = self.build_operational_context_summary().await;
            let causal_guidance = self.build_causal_guidance_summary().await;
            build_external_agent_prompt(
                config,
                &memory,
                content,
                &memory_dir,
                operator_model_summary.as_deref(),
                operational_context.as_deref(),
                causal_guidance.as_deref(),
            )
        } else {
            content.to_string()
        };
        self.emit_workflow_notice(
            &tid,
            "memory-consulted",
            "Loaded persistent memory, user profile, and local skill paths for this turn.",
            Some(format!(
                "memory_dir={}; skills_dir={}",
                active_memory_dir(&self.data_dir).display(),
                skills_dir(&self.data_dir).display()
            )),
        );
        if let Some(recall) = onecontext_bootstrap {
            enriched_prompt.push_str("\n\n[ONECONTEXT RECALL]\n");
            enriched_prompt.push_str(&recall);
        }
        match self.maybe_build_honcho_context(&tid, content).await {
            Ok(Some(honcho_context)) => {
                enriched_prompt.push_str("\n\n[CROSS-SESSION MEMORY]\n");
                enriched_prompt.push_str(&honcho_context);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(thread_id = %tid, error = %error, "failed to build Honcho context");
            }
        }

        // Run through external agent
        let runners = self.external_runners.read().await;
        let runner = match runners.get(config.agent_backend.as_str()) {
            Some(runner) => runner,
            None => {
                let message = format!(
                    "No external agent runner for backend '{}'",
                    config.agent_backend
                );
                self.add_assistant_message(
                    &tid,
                    &format!("Error: {message}"),
                    0,
                    0,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await;
                self.persist_threads().await;
                self.emit_turn_error_completion(&tid, &message, None, None)
                    .await;
                self.finish_stream_cancellation(&tid, stream_generation)
                    .await;
                anyhow::bail!(message);
            }
        };

        let response = match runner
            .send_message(&tid, &enriched_prompt, Some(stream_cancel_token))
            .await
        {
            Ok(response) => Some(response),
            Err(e) if external_runner::is_stream_cancelled(&e) => None,
            Err(e) => {
                let error_text = e.to_string();
                self.add_assistant_message(
                    &tid,
                    &format!("Error: {error_text}"),
                    0,
                    0,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await;
                self.persist_threads().await;
                self.emit_turn_error_completion(
                    &tid,
                    &error_text,
                    Some(config.agent_backend.to_string()),
                    Some(config.agent_backend.to_string()),
                )
                .await;
                self.finish_stream_cancellation(&tid, stream_generation)
                    .await;
                return Err(e);
            }
        };

        // Store assistant response in thread
        if let Some(response) = response {
            self.add_assistant_message(
                &tid,
                &response,
                0,
                0,
                None,
                Some(config.agent_backend.to_string()),
                Some(config.agent_backend.to_string()),
                None,
                None,
            )
            .await;
        }
        self.persist_threads().await;
        self.finish_stream_cancellation(&tid, stream_generation)
            .await;

        Ok(tid)
    }

    /// Get the availability status of an external agent.
    pub async fn external_agent_status(
        &self,
        agent_type: &str,
    ) -> Option<external_runner::ExternalAgentStatus> {
        let runners = self.external_runners.read().await;
        runners.get(agent_type).map(|r| r.status())
    }

    /// Start gateway mode for an external agent.
    pub async fn start_external_gateway(&self) -> Result<()> {
        let config = self.config.read().await.clone();
        let runners = self.external_runners.read().await;
        if let Some(runner) = runners.get(config.agent_backend.as_str()) {
            runner.start_gateway().await
        } else {
            Ok(())
        }
    }

    /// Stop any running external agent processes.
    pub async fn stop_external_agents(&self) {
        let runners = self.external_runners.read().await;
        for runner in runners.values() {
            runner.stop().await;
        }
    }
}
