use super::*;

impl AgentEngine {
    pub(crate) async fn maybe_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        self.persist_compaction_artifact_with_mode(
            thread_id,
            task_id,
            config,
            provider_config,
            CompactionCandidateMode::Automatic,
        )
        .await
    }

    pub(crate) async fn force_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        self.persist_compaction_artifact_with_mode(
            thread_id,
            task_id,
            config,
            provider_config,
            CompactionCandidateMode::Forced,
        )
        .await
    }

    pub(crate) async fn persist_compaction_artifact_with_mode(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
        mode: CompactionCandidateMode,
    ) -> Result<bool> {
        let snapshot = {
            let threads = self.threads.read().await;
            threads.get(thread_id).cloned()
        };
        let Some(thread) = snapshot else {
            return Ok(false);
        };
        let (window_start, _) = active_compaction_window(&thread.messages);
        let Some(candidate) = (match mode {
            CompactionCandidateMode::Automatic => {
                compaction_candidate(&thread.messages, config, provider_config)
            }
            CompactionCandidateMode::Forced => {
                forced_compaction_candidate(&thread.messages, config, provider_config)
            }
        }) else {
            return Ok(false);
        };
        let pre_compaction_total_tokens = estimate_message_tokens(&thread.messages[window_start..]);
        let effective_context_window_tokens =
            effective_compaction_window_tokens(config, provider_config);
        let split_at = window_start + candidate.split_at;
        let source_messages = thread.messages[window_start..split_at].to_vec();
        let message_count = thread.messages.len();
        let structural_memory = self.get_thread_structural_memory(thread_id).await;

        let (artifact, strategy_used, fallback_notice) = self
            .build_compaction_artifact(
                thread_id,
                &source_messages,
                candidate.target_tokens,
                candidate.trigger,
                pre_compaction_total_tokens,
                effective_context_window_tokens,
                config,
                structural_memory.as_ref(),
            )
            .await?;
        let compaction_trigger_summary = build_compaction_visible_content(
            pre_compaction_total_tokens,
            effective_context_window_tokens,
            candidate.target_tokens,
            candidate.trigger,
            strategy_used,
        );

        let (current_split_at, total_message_count) = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return Ok(false);
            };
            let (window_start, _) = active_compaction_window(&thread.messages);
            let Some(current_candidate) = (match mode {
                CompactionCandidateMode::Automatic => {
                    compaction_candidate(&thread.messages, config, provider_config)
                }
                CompactionCandidateMode::Forced => {
                    forced_compaction_candidate(&thread.messages, config, provider_config)
                }
            }) else {
                return Ok(false);
            };
            let current_split_at = window_start + current_candidate.split_at;
            thread.messages.insert(current_split_at, artifact);
            thread.updated_at = now_millis();
            thread.total_input_tokens = thread
                .messages
                .iter()
                .map(|message| message.input_tokens)
                .sum();
            thread.total_output_tokens = thread
                .messages
                .iter()
                .map(|message| message.output_tokens)
                .sum();
            (current_split_at, thread.messages.len())
        };
        let compaction_notice_details = serde_json::json!({
            "split_at": current_split_at,
            "total_message_count": total_message_count,
            "pre_compaction_total_tokens": pre_compaction_total_tokens,
            "effective_context_window_tokens": effective_context_window_tokens,
            "target_tokens": candidate.target_tokens,
            "trigger": compaction_trigger_detail_value(candidate.trigger),
            "strategy": strategy_used,
        })
        .to_string();

        self.persist_thread_by_id(thread_id).await;
        self.trim_participant_playground_threads_for_visible_thread(thread_id)
            .await;
        self.record_provenance_event(
            "context_compressed",
            match mode {
                CompactionCandidateMode::Automatic => {
                    "thread context was compacted for an LLM request"
                }
                CompactionCandidateMode::Forced => {
                    "thread context was compacted by operator request"
                }
            },
            serde_json::json!({
                "thread_id": thread_id,
                "split_at": split_at,
                "target_tokens": candidate.target_tokens,
                "trigger": compaction_trigger_detail_value(candidate.trigger),
                "message_count": message_count,
                "strategy": strategy_used,
                "forced": mode == CompactionCandidateMode::Forced,
            }),
            None,
            task_id,
            Some(thread_id),
            None,
            None,
        )
        .await;
        self.persist_context_compression_causal_trace(
            thread_id,
            task_id,
            split_at,
            message_count,
            candidate.target_tokens,
            strategy_used,
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: match mode {
                CompactionCandidateMode::Automatic => COMPACTION_NOTICE_KIND,
                CompactionCandidateMode::Forced => MANUAL_COMPACTION_NOTICE_KIND,
            }
            .to_string(),
            message: format!(
                "{} compaction applied using {}. {}",
                match mode {
                    CompactionCandidateMode::Automatic => "Auto",
                    CompactionCandidateMode::Forced => "Manual",
                },
                serde_json::to_string(&strategy_used)
                    .unwrap_or_else(|_| "\"heuristic\"".to_string())
                    .trim_matches('"'),
                compaction_trigger_summary,
            ),
            details: Some(compaction_notice_details.clone()),
        });
        if let Some(fallback_notice) = fallback_notice {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: match mode {
                    CompactionCandidateMode::Automatic => COMPACTION_NOTICE_KIND,
                    CompactionCandidateMode::Forced => MANUAL_COMPACTION_NOTICE_KIND,
                }
                .to_string(),
                message: fallback_notice,
                details: Some(compaction_notice_details),
            });
        }
        Ok(true)
    }

    pub async fn force_compact_and_continue(self: &Arc<Self>, thread_id: &str) -> Result<bool> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let latest_user_content = self.latest_visible_user_message_content(thread_id).await;
        let latest_user_content = latest_user_content
            .as_deref()
            .filter(|content| !content.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!("no user message available to continue after compaction")
            })?;
        let agent_id = self
            .active_agent_id_for_thread(thread_id)
            .await
            .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
        let continuation = DeferredVisibleThreadContinuation {
            agent_id,
            task_id: None,
            preferred_session_hint: None,
            llm_user_content: latest_user_content,
            force_compaction: true,
            rerun_participant_observers_after_turn: true,
            internal_delegate_sender: None,
            internal_delegate_message: None,
        };

        let was_streaming = {
            let streams = self.stream_cancellations.lock().await;
            streams.contains_key(thread_id)
        };
        self.enqueue_visible_thread_continuation(thread_id, continuation)
            .await;

        if was_streaming && self.stop_stream(thread_id).await {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: MANUAL_COMPACTION_NOTICE_KIND.to_string(),
                message: "Manual compaction requested; waiting for the current stream to stop."
                    .to_string(),
                details: None,
            });
            return Ok(true);
        }

        self.flush_deferred_visible_thread_continuations(thread_id)
            .await?;
        Ok(true)
    }
}
