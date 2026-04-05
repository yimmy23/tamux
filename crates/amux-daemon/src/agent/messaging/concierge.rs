use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn send_concierge_message_on_thread(
        &self,
        thread_id: &str,
        content: &str,
        preferred_session_hint: Option<&str>,
        operator_origin: bool,
        allow_escalation: bool,
    ) -> Result<()> {
        let (tid, _) = self.get_or_create_thread(Some(thread_id), content).await;
        let pinned = tid == crate::agent::concierge::CONCIERGE_THREAD_ID;
        let title = if pinned {
            "Concierge".to_string()
        } else {
            internal_dm_thread_title(CONCIERGE_AGENT_ID, MAIN_AGENT_ID)
        };
        self.ensure_thread_identity(&tid, &title, pinned).await;

        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread
                    .messages
                    .push(AgentMessage::user(content.trim(), now_millis()));
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(&tid).await;

        if operator_origin {
            self.record_operator_message(&tid, content, false).await?;
        }

        let reply = if allow_escalation && concierge_should_escalate(content) {
            let result = Box::pin(self.send_internal_agent_message(
                CONCIERGE_AGENT_ID,
                MAIN_AGENT_ID,
                content,
                preferred_session_hint,
            ))
            .await?;
            let response = result.response;
            format!("I checked with {}. {}", MAIN_AGENT_NAME, response.trim())
        } else {
            self.generate_concierge_reply(&tid).await?
        };

        self.add_assistant_message(
            &tid,
            &reply,
            0,
            0,
            None,
            Some("concierge".to_string()),
            None,
            None,
            None,
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::Delta {
            thread_id: tid.clone(),
            content: reply.clone(),
        });
        let _ = self.event_tx.send(AgentEvent::Done {
            thread_id: tid,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: Some("concierge".to_string()),
            model: None,
            tps: None,
            generation_ms: None,
            reasoning: None,
            upstream_message: None,
            provider_final_result: None,
        });
        Ok(())
    }

    async fn generate_concierge_reply(&self, thread_id: &str) -> Result<String> {
        let config = self.config.read().await.clone();
        let provider_config = crate::agent::concierge::fast_concierge_provider_config(
            &crate::agent::concierge::resolve_concierge_provider(&config)?,
        );
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        drop(config);

        let memory_paths = memory_paths_for_scope(&self.data_dir, MAIN_AGENT_ID);
        let memory = self.memory_snapshot_for_scope(MAIN_AGENT_ID).await;
        let mut system_prompt = format!(
            "{}\n\n{} is the main builder agent. You are {}, the concierge. If you need deeper implementation work, summarize what {} should handle.\nPersistent memory files:\n- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}",
            crate::agent::concierge::concierge_system_prompt(),
            MAIN_AGENT_NAME,
            CONCIERGE_AGENT_NAME,
            MAIN_AGENT_NAME,
            memory_paths.memory_path.display(),
            memory_paths.soul_path.display(),
            memory_paths.user_path.display(),
        );
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&build_concierge_runtime_identity_prompt(
            &provider_id,
            &provider_config.model,
        ));
        if !memory.soul.trim().is_empty() {
            system_prompt.push_str("\n\n## Identity Notes\n");
            system_prompt.push_str(memory.soul.trim());
        }
        if !memory.memory.trim().is_empty() {
            system_prompt.push_str("\n\n## Persistent Memory\n");
            system_prompt.push_str(memory.memory.trim());
        }
        if !memory.user_profile.trim().is_empty() {
            system_prompt.push_str("\n\n## Operator Profile\n");
            system_prompt.push_str(memory.user_profile.trim());
        }
        if is_internal_dm_thread(thread_id) {
            system_prompt.push_str(
                "\n\nThis thread is an internal agent-to-agent DM. Reply to another agent, not directly to the operator.",
            );
        }

        let messages = {
            let threads = self.threads.read().await;
            let history = threads
                .get(thread_id)
                .map(|thread| {
                    let start = thread.messages.len().saturating_sub(16);
                    thread.messages[start..].to_vec()
                })
                .unwrap_or_default();
            super::llm_client::messages_to_api_format(&history)
        };

        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            &system_prompt,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 500,
            },
        );
        let mut full_content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content, .. }) => full_content.push_str(&content),
                Ok(CompletionChunk::Done { content, .. }) => {
                    if !content.is_empty() {
                        full_content = content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => anyhow::bail!(message),
                Err(error) => return Err(error),
                Ok(_) => {}
            }
        }
        Ok(full_content.trim().to_string())
    }
}
