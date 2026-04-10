use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn clear_thread_continuation_state(&self, thread_id: &str) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.upstream_thread_id = None;
            thread.upstream_transport = None;
            thread.upstream_provider = None;
            thread.upstream_model = None;
            thread.upstream_assistant_id = None;
            for message in &mut thread.messages {
                message.response_id = None;
            }
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
    }

    pub(in crate::agent) async fn repair_tool_call_sequence(&self, thread_id: &str) {
        let removed = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return;
            };
            let before = thread.messages.len();
            let mut repaired = Vec::with_capacity(before);
            let mut i = 0;
            while i < thread.messages.len() {
                let msg = &thread.messages[i];
                if msg.role == MessageRole::Assistant && msg.tool_calls.is_some() {
                    let tool_calls = msg.tool_calls.as_ref().unwrap();
                    let expected: std::collections::HashSet<&str> =
                        tool_calls.iter().map(|tc| tc.id.as_str()).collect();
                    let mut results = Vec::new();
                    let mut matched = std::collections::HashSet::new();
                    let mut j = i + 1;
                    while j < thread.messages.len() && thread.messages[j].role == MessageRole::Tool
                    {
                        if thread.messages[j]
                            .tool_call_id
                            .as_deref()
                            .map(|id| expected.contains(id))
                            .unwrap_or(false)
                        {
                            results.push(thread.messages[j].clone());
                            if let Some(id) = thread.messages[j].tool_call_id.as_deref() {
                                matched.insert(id);
                            }
                        }
                        j += 1;
                    }
                    let has_complete_batch = matched.len() == expected.len();
                    let saw_no_followup_messages = j == i + 1;
                    let is_unanswered_latest_tool_turn =
                        saw_no_followup_messages && j == thread.messages.len();
                    if has_complete_batch || is_unanswered_latest_tool_turn {
                        repaired.push(msg.clone());
                        if has_complete_batch {
                            repaired.extend(results);
                        }
                    }
                    i = j;
                } else if msg.role == MessageRole::Tool {
                    i += 1;
                } else {
                    repaired.push(msg.clone());
                    i += 1;
                }
            }
            let removed = before - repaired.len();
            if removed > 0 {
                tracing::info!(
                    "repair_tool_call_sequence: removed {} broken messages from thread {}",
                    removed,
                    thread_id
                );
                thread.messages = repaired;
                thread.updated_at = now_millis();
                thread.total_input_tokens = thread.messages.iter().map(|m| m.input_tokens).sum();
                thread.total_output_tokens = thread.messages.iter().map(|m| m.output_tokens).sum();
            }
            removed
        };

        if removed > 0 {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub(in crate::agent) async fn add_assistant_message(
        &self,
        thread_id: &str,
        content: &str,
        input_tokens: u64,
        output_tokens: u64,
        reasoning: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        api_transport: Option<ApiTransport>,
        response_id: Option<String>,
    ) {
        self.add_assistant_message_with_upstream_message(
            thread_id,
            content,
            input_tokens,
            output_tokens,
            reasoning,
            provider,
            model,
            api_transport,
            response_id,
            None,
            None,
        )
        .await;
    }

    pub(in crate::agent) async fn add_assistant_message_with_upstream_message(
        &self,
        thread_id: &str,
        content: &str,
        input_tokens: u64,
        output_tokens: u64,
        reasoning: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        api_transport: Option<ApiTransport>,
        response_id: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
    ) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            let author_agent_id = current_agent_scope_id();
            let author_agent_name = canonical_agent_name(&author_agent_id).to_string();
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: content.into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens,
                output_tokens,
                provider,
                model,
                api_transport,
                response_id,
                upstream_message,
                provider_final_result,
                author_agent_id: Some(author_agent_id),
                author_agent_name: Some(author_agent_name),
                reasoning,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                structural_refs: Vec::new(),
                timestamp: now_millis(),
            });
            thread.total_input_tokens += input_tokens;
            thread.total_output_tokens += output_tokens;
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
        if let Err(error) = self.maybe_sync_thread_to_honcho(thread_id).await {
            tracing::warn!(thread_id = %thread_id, error = %error, "failed to sync assistant message to Honcho");
        }
    }

    pub(in crate::agent) async fn emit_turn_error_completion(
        &self,
        thread_id: &str,
        message: &str,
        provider: Option<String>,
        model: Option<String>,
    ) {
        let _ = self.event_tx.send(AgentEvent::Delta {
            thread_id: thread_id.to_string(),
            content: format!("Error: {message}"),
        });
        let _ = self.event_tx.send(AgentEvent::Done {
            thread_id: thread_id.to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider,
            model,
            tps: None,
            generation_ms: None,
            reasoning: None,
            upstream_message: None,
            provider_final_result: None,
        });
    }

    pub(in crate::agent) async fn update_thread_upstream_state(
        &self,
        thread_id: &str,
        provider: &str,
        model: &str,
        transport: ApiTransport,
        assistant_id: Option<&str>,
        upstream_thread_id: Option<String>,
    ) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.upstream_transport = Some(transport);
            thread.upstream_provider = Some(provider.to_string());
            thread.upstream_model = Some(model.to_string());
            thread.upstream_assistant_id = assistant_id
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.to_string());
            thread.upstream_thread_id = upstream_thread_id;
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
    }
}
