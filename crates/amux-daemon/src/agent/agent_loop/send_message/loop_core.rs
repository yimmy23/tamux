use super::*;

impl<'a> SendMessageRunner<'a> {
    async fn prepare_request(&mut self) -> Result<PreparedLlmRequest> {
        let threads = self.engine.threads.read().await;
        let thread = match threads.get(&self.tid) {
            Some(thread) => thread,
            None => {
                self.engine
                    .finish_stream_cancellation(&self.tid, self.stream_generation)
                    .await;
                anyhow::bail!("thread not found");
            }
        };
        let mut request_thread = thread.clone();
        if self.llm_user_content != self.stored_user_content {
            if let Some(last_user_message) = request_thread
                .messages
                .iter_mut()
                .rev()
                .find(|message| message.role == MessageRole::User)
            {
                last_user_message.content = self.llm_user_content.to_string();
            }
        }
        let prepared = prepare_llm_request(&request_thread, &self.config, &self.provider_config);
        if !self.recorded_compaction_provenance {
            if let Some(candidate) = compaction_candidate(
                &request_thread.messages,
                &self.config,
                &self.provider_config,
            ) {
                self.engine
                    .record_provenance_event(
                        "context_compressed",
                        "thread context was compacted for an LLM request",
                        serde_json::json!({
                            "thread_id": self.tid.as_str(),
                            "split_at": candidate.split_at,
                            "target_tokens": candidate.target_tokens,
                            "message_count": thread.messages.len(),
                        }),
                        None,
                        self.task_id,
                        Some(self.tid.as_str()),
                        None,
                        None,
                    )
                    .await;
                self.recorded_compaction_provenance = true;
            }
        }
        tracing::info!(
            thread_id = %self.tid,
            thread_messages = thread.messages.len(),
            api_messages = prepared.messages.len(),
            transport = ?prepared.transport,
            loop_count = self.loop_count,
            "building LLM request"
        );
        Ok(prepared)
    }

    pub(super) async fn stream_once(&mut self) -> Result<StreamIteration> {
        let prepared_request = self.prepare_request().await?;
        if self.loop_count > 1 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        if let Err(e) = self
            .engine
            .check_circuit_breaker(&self.config.provider)
            .await
        {
            let outage_context = self
                .engine
                .suggest_alternative_provider(&self.config.provider)
                .await
                .unwrap_or_else(|| {
                    "No healthy fallback providers are currently available.".to_string()
                });
            let error_msg = format!(
                "Provider '{}' is temporarily unavailable (circuit breaker open). {}",
                self.config.provider, outage_context
            );
            let _ = self.engine.event_tx.send(AgentEvent::Error {
                thread_id: self.tid.clone(),
                message: error_msg.clone(),
            });
            self.engine
                .finish_stream_cancellation(&self.tid, self.stream_generation)
                .await;
            return Err(e.context(error_msg));
        }

        let llm_started_at = Instant::now();
        let mut first_token_at: Option<Instant> = None;
        let effective_transport_for_turn = prepared_request.transport;
        let mut stream = send_completion_request(
            &self.engine.http_client,
            &self.config.provider,
            &self.provider_config,
            &self.system_prompt,
            &prepared_request.messages,
            &self.tools,
            prepared_request.transport,
            prepared_request.previous_response_id.clone(),
            prepared_request.upstream_thread_id.clone(),
            self.retry_strategy,
        );

        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut final_chunk: Option<CompletionChunk> = None;
        let llm_stream_chunk_timeout = self.stream_chunk_timeout_override.unwrap_or_else(|| {
            std::time::Duration::from_secs(DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS)
        });
        let mut stream_timed_out = false;

        loop {
            tokio::select! {
                _ = self.stream_cancel_token.cancelled() => {
                    self.was_cancelled = true;
                    break;
                }
                _ = tokio::time::sleep(llm_stream_chunk_timeout) => {
                    tracing::warn!("LLM stream timeout -- no data for {}s", llm_stream_chunk_timeout.as_secs());
                    self.engine.record_llm_outcome(&self.config.provider, false).await;
                    stream_timed_out = true;
                    break;
                }
                maybe_chunk = stream.next() => {
                    let Some(chunk_result) = maybe_chunk else {
                        break;
                    };

                    let chunk = match chunk_result {
                        Ok(chunk) => chunk,
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("tool call result does not follow tool call")
                                && !self.tool_sequence_repaired
                            {
                                tracing::warn!("detected broken tool call sequence -- repairing thread and retrying");
                                self.tool_sequence_repaired = true;
                                self.engine.repair_tool_call_sequence(&self.tid).await;
                                let _ = self.engine.event_tx.send(AgentEvent::WorkflowNotice {
                                    thread_id: self.tid.clone(),
                                    kind: "tool-repair".to_string(),
                                    message: "Repairing message sequence, retrying...".to_string(),
                                    details: None,
                                });
                                return Ok(StreamIteration {
                                    prepared_request,
                                    llm_started_at,
                                    first_token_at,
                                    effective_transport_for_turn,
                                    accumulated_content,
                                    accumulated_reasoning,
                                    final_chunk: None,
                                    stream_timed_out: true,
                                    retry_loop: true,
                                });
                            }
                            self.engine.record_llm_outcome(&self.config.provider, false).await;
                            self.engine.finish_stream_cancellation(&self.tid, self.stream_generation).await;
                            return Err(e);
                        }
                    };

                    match chunk {
                        CompletionChunk::Delta { content, reasoning } => {
                            if self.retry_status_visible {
                                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: self.tid.clone(),
                                    phase: "cleared".to_string(),
                                    attempt: 0,
                                    max_retries: 0,
                                    delay_ms: 0,
                                    failure_class: String::new(),
                                    message: String::new(),
                                });
                                self.retry_status_visible = false;
                            }
                            if first_token_at.is_none()
                                && (!content.is_empty()
                                    || reasoning.as_ref().map(|s| !s.is_empty()).unwrap_or(false))
                            {
                                first_token_at = Some(Instant::now());
                            }
                            if !content.is_empty() {
                                self.assistant_output_visible = true;
                                accumulated_content.push_str(&content);
                                let _ = self.engine.event_tx.send(AgentEvent::Delta {
                                    thread_id: self.tid.clone(),
                                    content,
                                });
                            }
                            if let Some(r) = reasoning {
                                accumulated_reasoning.push_str(&r);
                                let _ = self.engine.event_tx.send(AgentEvent::Reasoning {
                                    thread_id: self.tid.clone(),
                                    content: r,
                                });
                            }
                        }
                        CompletionChunk::Retry {
                            attempt,
                            max_retries,
                            delay_ms,
                            failure_class,
                            message,
                        } => {
                            let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                thread_id: self.tid.clone(),
                                phase: "retrying".to_string(),
                                attempt,
                                max_retries,
                                delay_ms,
                                failure_class,
                                message,
                            });
                            self.retry_status_visible = true;
                        }
                        CompletionChunk::TransportFallback { .. } => {}
                        chunk @ CompletionChunk::Done { .. } => {
                            final_chunk = Some(chunk);
                            break;
                        }
                        chunk @ CompletionChunk::ToolCalls { .. } => {
                            final_chunk = Some(chunk);
                            break;
                        }
                        CompletionChunk::Error { message } => {
                            let visible_message = sanitize_upstream_failure_message(&message);
                            if let Some(structured) = parse_structured_upstream_failure(&message) {
                                let recovery = self
                                    .engine
                                    .maybe_recover_fixable_upstream_failure(
                                        &self.tid,
                                        &structured,
                                        self.assistant_output_visible,
                                        self.tool_side_effect_committed,
                                        &mut self.attempted_recovery_signatures,
                                    )
                                    .await?;
                                if recovery.retry_attempted {
                                    return Ok(StreamIteration {
                                        prepared_request,
                                        llm_started_at,
                                        first_token_at,
                                        effective_transport_for_turn,
                                        accumulated_content,
                                        accumulated_reasoning,
                                        final_chunk: None,
                                        stream_timed_out: true,
                                        retry_loop: true,
                                    });
                                }
                                let notice_kind = match structured.class.as_str() {
                                    "transport_incompatible" => "transport-incompatible",
                                    "request_invalid" => "request-invalid",
                                    "temporary_upstream" => "temporary-upstream",
                                    _ => "upstream-error",
                                };
                                self.engine.emit_workflow_notice(
                                    &self.tid,
                                    notice_kind,
                                    structured.summary.clone(),
                                    Some(
                                        serde_json::json!({
                                            "provider": self.config.provider,
                                            "transport": prepared_request.transport,
                                            "class": structured.class,
                                            "diagnostics": structured.diagnostics,
                                        })
                                        .to_string(),
                                    ),
                                );
                            }
                            if self.config.auto_retry && is_transient_retry_message(&message) {
                                let delay_ms = 30_000u64;
                                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: self.tid.clone(),
                                    phase: "waiting".to_string(),
                                    attempt: self.config.max_retries,
                                    max_retries: self.config.max_retries,
                                    delay_ms,
                                    failure_class: retry_failure_class_from_message(&message).to_string(),
                                    message: visible_message.clone(),
                                });
                                self.retry_status_visible = true;
                                tokio::select! {
                                    _ = self.stream_cancel_token.cancelled() => {
                                        self.was_cancelled = true;
                                        break;
                                    }
                                    _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                                        return Ok(StreamIteration {
                                            prepared_request,
                                            llm_started_at,
                                            first_token_at,
                                            effective_transport_for_turn,
                                            accumulated_content,
                                            accumulated_reasoning,
                                            final_chunk: None,
                                            stream_timed_out: true,
                                            retry_loop: true,
                                        });
                                    }
                                }
                            }
                            if self.retry_status_visible {
                                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: self.tid.clone(),
                                    phase: "cleared".to_string(),
                                    attempt: 0,
                                    max_retries: 0,
                                    delay_ms: 0,
                                    failure_class: String::new(),
                                    message: String::new(),
                                });
                                self.retry_status_visible = false;
                            }
                            self.engine.record_llm_outcome(&self.config.provider, false).await;
                            self.engine.add_assistant_message(
                                &self.tid,
                                &format!("Error: {visible_message}"),
                                0,
                                0,
                                None,
                                None,
                                None,
                                None,
                                None,
                            ).await;
                            self.engine.persist_threads().await;
                            self.engine.emit_turn_error_completion(
                                &self.tid,
                                &visible_message,
                                Some(self.config.provider.clone()),
                                Some(self.provider_config.model.clone()),
                            ).await;
                            self.engine.finish_stream_cancellation(&self.tid, self.stream_generation).await;
                            return Err(anyhow::anyhow!("LLM error: {message}"));
                        }
                    }
                }
            }
        }

        Ok(StreamIteration {
            prepared_request,
            llm_started_at,
            first_token_at,
            effective_transport_for_turn,
            accumulated_content,
            accumulated_reasoning,
            final_chunk,
            stream_timed_out,
            retry_loop: false,
        })
    }

    pub(super) async fn handle_stream_timeout(
        &mut self,
        iteration: &StreamIteration,
    ) -> Result<bool> {
        if iteration.retry_loop {
            return Ok(true);
        }
        const MAX_STREAM_TIMEOUTS: u32 = 3;
        if !iteration.stream_timed_out {
            return Ok(false);
        }
        self.stream_timeout_count += 1;
        if self.stream_timeout_count >= MAX_STREAM_TIMEOUTS && !self.config.auto_retry {
            let msg = format!(
                "Connection timed out {} times \u{2014} giving up. The provider may be overloaded.",
                self.stream_timeout_count
            );
            let _ = self.engine.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: self.tid.clone(),
                kind: "stream-timeout".to_string(),
                message: msg.clone(),
                details: None,
            });
            self.engine
                .finish_stream_cancellation(&self.tid, self.stream_generation)
                .await;
            return Err(anyhow::anyhow!("{msg}"));
        }
        let delay_ms = if self.stream_timeout_count >= MAX_STREAM_TIMEOUTS {
            30_000u64
        } else {
            2_000u64
        };
        let phase = if self.stream_timeout_count >= MAX_STREAM_TIMEOUTS {
            "waiting"
        } else {
            "retrying"
        };
        let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
            thread_id: self.tid.clone(),
            phase: phase.to_string(),
            attempt: self.stream_timeout_count,
            max_retries: MAX_STREAM_TIMEOUTS,
            delay_ms,
            failure_class: "timeout".to_string(),
            message: "Connection timed out while waiting for streamed output".to_string(),
        });
        self.retry_status_visible = true;
        tokio::select! {
            _ = self.stream_cancel_token.cancelled() => {
                self.was_cancelled = true;
                Ok(false)
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => Ok(true),
        }
    }
}
