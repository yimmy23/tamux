use super::*;

impl AgentEngine {
    fn extract_gateway_approval_id_from_message(content: &str) -> Option<String> {
        let (_, remainder) = content.split_once("Approval ID:")?;
        remainder
            .trim()
            .split_whitespace()
            .next()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn gateway_approval_request_text(&self, pending_approval: &ToolPendingApproval) -> String {
        format!(
            "Approval required for a pending task.\nApproval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}\n\nReply with one of: approve-once | approve-session | deny",
            pending_approval.approval_id,
            pending_approval.risk_level,
            pending_approval.blast_radius,
            pending_approval.command,
            pending_approval.reasons.join("\n- "),
        )
    }

    pub(in crate::agent) async fn maybe_send_gateway_thread_approval_request(
        &self,
        thread_id: &str,
        pending_approval: &ToolPendingApproval,
    ) -> bool {
        let Some(msg) = self.gateway_message_target_for_thread(thread_id).await else {
            return false;
        };

        let (_, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        let response_text = self.gateway_approval_request_text(pending_approval);
        self.add_assistant_message(
            thread_id,
            &response_text,
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;
        self.persist_thread_by_id(thread_id).await;

        let tool_result = self
            .send_gateway_platform_tool(
                thread_id,
                "gateway_approval_prompt",
                reply_tool_name,
                &msg,
                &response_text,
            )
            .await;

        if tool_result.is_error {
            tracing::error!(
                thread_id = %thread_id,
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send approval prompt"
            );
            return false;
        }

        true
    }

    pub(super) async fn gateway_pending_approval_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        {
            let tasks = self.tasks.lock().await;
            if let Some(approval_id) = tasks
                .iter()
                .rev()
                .find(|task| {
                    task.thread_id.as_deref() == Some(thread_id)
                        && task.awaiting_approval_id.is_some()
                })
                .and_then(|task| task.awaiting_approval_id.clone())
            {
                return Some(approval_id);
            }
        }

        if let Some(approval_id) = self
            .critique_approval_continuations
            .lock()
            .await
            .iter()
            .find(|(_, continuation)| continuation.thread_id == thread_id)
            .map(|(approval_id, _)| approval_id.clone())
        {
            return Some(approval_id);
        }

        let approval_ids = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .filter_map(|message| {
                            Self::extract_gateway_approval_id_from_message(&message.content)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        if approval_ids.is_empty() {
            return None;
        }

        let pending = self.pending_operator_approvals.read().await;
        approval_ids
            .into_iter()
            .find(|approval_id| pending.contains_key(approval_id))
    }

    async fn gateway_resolve_thread_approval(
        &self,
        approval_id: &str,
        decision: amux_protocol::ApprovalDecision,
    ) -> bool {
        if self
            .session_manager
            .resolve_approval_by_id(approval_id, decision)
            .await
            .is_ok()
        {
            let _ = self
                .record_operator_approval_resolution(approval_id, decision)
                .await;
            return true;
        }

        if self
            .handle_task_approval_resolution(approval_id, decision)
            .await
        {
            let _ = self
                .record_operator_approval_resolution(approval_id, decision)
                .await;
            return true;
        }

        self.resume_critique_approval_continuation(
            approval_id,
            decision,
            &self.session_manager,
            &self.event_tx,
            &self.data_dir,
            &self.http_client,
        )
        .await
        .is_ok()
    }

    fn gateway_approval_confirmation_text(
        &self,
        decision: amux_protocol::ApprovalDecision,
    ) -> &'static str {
        match decision {
            amux_protocol::ApprovalDecision::ApproveOnce => {
                "Approved once. I resumed the pending task."
            }
            amux_protocol::ApprovalDecision::ApproveSession => {
                "Approved for this session. I resumed the pending task."
            }
            amux_protocol::ApprovalDecision::Deny => "Denied. I rejected the pending task.",
        }
    }

    pub(in crate::agent) async fn handle_gateway_task_approval_reply(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        existing_thread: Option<&str>,
        reply_tool_name: &str,
    ) -> bool {
        let Some(decision) = parse_gateway_approval_decision(&msg.content) else {
            return false;
        };

        let Some(thread_id) = existing_thread else {
            return false;
        };

        let Some(approval_id) = self.gateway_pending_approval_for_thread(thread_id).await else {
            return false;
        };

        if !self
            .gateway_resolve_thread_approval(&approval_id, decision)
            .await
        {
            return false;
        }

        let response_text = self.gateway_approval_confirmation_text(decision);
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist approval reply exchange"
                );
                return true;
            }
        };

        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "gateway_approval",
                reply_tool_name,
                msg,
                response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send approval confirmation"
            );
        }
        true
    }

    pub(super) async fn release_gateway_inflight_channel(&self, channel_key: &str) {
        self.gateway_inflight_channels
            .lock()
            .await
            .remove(channel_key);
    }

    pub(super) async fn send_gateway_platform_tool(
        &self,
        _thread_id: &str,
        tool_id_prefix: &str,
        reply_tool_name: &str,
        msg: &gateway::IncomingMessage,
        response_text: &str,
    ) -> ToolResult {
        let tool_call_id = format!("{tool_id_prefix}_{}", uuid::Uuid::new_v4());
        let args = gateway_reply_args(&msg.platform, &msg.channel, response_text);
        match crate::agent::tool_executor::execute_gateway_message(
            reply_tool_name,
            &args,
            self,
            &self.http_client,
        )
        .await
        {
            Ok(content) => ToolResult {
                tool_call_id,
                name: reply_tool_name.to_string(),
                content,
                is_error: false,
                weles_review: None,
                pending_approval: None,
            },
            Err(error) => ToolResult {
                tool_call_id,
                name: reply_tool_name.to_string(),
                content: error.to_string(),
                is_error: true,
                weles_review: None,
                pending_approval: None,
            },
        }
    }

    async fn gateway_message_target_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<gateway::IncomingMessage> {
        let channel_key = {
            let gateway_threads = self.gateway_threads.read().await;
            gateway_threads
                .iter()
                .find_map(|(channel_key, mapped_thread_id)| {
                    (mapped_thread_id == thread_id).then(|| channel_key.clone())
                })
        }?;

        let (platform, channel) = channel_key.split_once(':')?;
        Some(gateway::IncomingMessage {
            platform: platform.to_string(),
            sender: String::new(),
            content: String::new(),
            channel: channel.to_string(),
            message_id: None,
            thread_context: None,
        })
    }

    pub(in crate::agent) async fn maybe_auto_send_gateway_thread_response(&self, thread_id: &str) {
        let Some(msg) = self.gateway_message_target_for_thread(thread_id).await else {
            return;
        };
        let (_, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        self.auto_send_gateway_response(thread_id, &msg, reply_tool_name)
            .await;
    }

    pub(super) async fn send_gateway_timeout_fallback(
        &self,
        msg: &gateway::IncomingMessage,
        reply_tool_name: &str,
    ) {
        let fallback_text = "I’m still processing your message and hit a timeout. Please send it again, or use !new to start a fresh session.";
        let fallback_tool = ToolCall::with_default_weles_review(
            format!("gateway_timeout_{}", uuid::Uuid::new_v4()),
            ToolFunction {
                name: reply_tool_name.to_string(),
                arguments: gateway_reply_args(&msg.platform, &msg.channel, fallback_text)
                    .to_string(),
            },
        );
        let tool_result = Box::pin(tool_executor::execute_tool(
            &fallback_tool,
            self,
            "",
            None,
            &self.session_manager,
            None,
            &self.event_tx,
            &self.data_dir,
            &self.http_client,
            None,
        ))
        .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send timeout fallback message"
            );
        }
    }

    pub(super) async fn handle_gateway_reset_command(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
    ) -> bool {
        let trimmed = msg.content.trim().to_lowercase();
        if !is_gateway_reset_command(&trimmed) {
            return false;
        }

        self.gateway_threads.write().await.remove(channel_key);
        self.gateway_route_modes.write().await.remove(channel_key);
        if let Err(error) = self
            .history
            .delete_gateway_thread_binding(channel_key)
            .await
        {
            tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset binding");
        }
        if let Err(error) = self.history.delete_gateway_route_mode(channel_key).await {
            tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset route mode");
        }
        tracing::info!(channel_key = %channel_key, "gateway: conversation reset");

        let prompt = format!(
            "The user typed '{}' in {} channel {}. \
             This means they want to start a fresh conversation. \
             Send a brief confirmation back using {} saying the conversation has been reset.",
            msg.content,
            msg.platform,
            msg.channel,
            gateway_reply_tool(&msg.platform, &msg.channel).0
        );

        if let Err(e) = Box::pin(self.send_internal_message(None, &prompt)).await {
            tracing::error!(error = %e, "gateway: failed to send reset confirmation");
        }
        true
    }

    pub(super) async fn persist_gateway_route_request(
        &self,
        channel_key: &str,
        request: GatewayRouteRequest,
    ) {
        self.gateway_route_modes
            .write()
            .await
            .insert(channel_key.to_string(), request.mode);
        if let Err(error) = self
            .history
            .upsert_gateway_route_mode(channel_key, request.mode.as_str(), now_millis())
            .await
        {
            tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist route mode");
        }
    }

    pub(super) async fn handle_gateway_route_switch_ack(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        request: GatewayRouteRequest,
        reply_tool_name: &str,
    ) -> bool {
        if !request.ack_only {
            return false;
        }

        let response_text = gateway_route_confirmation(request.mode);
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, &response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist route switch exchange"
                );
                return true;
            }
        };
        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "gateway_route",
                reply_tool_name,
                msg,
                &response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send route switch confirmation"
            );
        }
        true
    }

    pub(super) async fn load_gateway_history_window(
        &self,
        channel_key: &str,
        thread_id: Option<&str>,
    ) -> Option<String> {
        let tid = thread_id?;
        match self.history.list_recent_messages(tid, 10).await {
            Ok(messages) if !messages.is_empty() => {
                let mut lines = Vec::with_capacity(messages.len() + 1);
                for m in messages {
                    let role = m.role;
                    let content = m
                        .content
                        .replace('\n', " ")
                        .chars()
                        .take(240)
                        .collect::<String>();
                    lines.push(format!("- {role}: {content}"));
                }
                Some(lines.join("\n"))
            }
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to load recent history");
                None
            }
        }
    }

    pub(super) async fn triage_gateway_message(
        &self,
        msg: &gateway::IncomingMessage,
        history_window: Option<&str>,
        existing_thread: Option<&str>,
    ) -> concierge::GatewayTriage {
        let triage = match Box::pin(tokio::time::timeout(
            std::time::Duration::from_secs(GATEWAY_TRIAGE_TIMEOUT_SECS),
            self.concierge.triage_gateway_message(
                self,
                &msg.platform,
                &msg.sender,
                &msg.content,
                history_window,
                existing_thread,
                &self.threads,
                &self.tasks,
            ),
        ))
        .await
        {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    timeout_secs = GATEWAY_TRIAGE_TIMEOUT_SECS,
                    "gateway: concierge triage timed out; falling back to full agent loop"
                );
                concierge::GatewayTriage::Complex
            }
        };
        triage
    }

    pub(super) async fn handle_concierge_simple_response(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        reply_tool_name: &str,
        response_text: &str,
    ) -> bool {
        tracing::info!(
            platform = %msg.platform,
            sender = %msg.sender,
            "gateway: concierge handled simple message"
        );
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist concierge fast-path exchange"
                );
                return true;
            }
        };
        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "concierge",
                reply_tool_name,
                msg,
                response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send concierge simple response"
            );
        }
        true
    }

    pub(super) async fn gateway_timeout_budget(
        &self,
    ) -> (std::time::Duration, std::time::Duration) {
        let config = self.config.read().await.clone();
        self.resolve_provider_config(&config)
            .map(|provider| {
                (
                    gateway_agent_timeout_for_reasoning(&provider.reasoning_effort),
                    gateway_stream_timeout_for_reasoning(&provider.reasoning_effort),
                )
            })
            .unwrap_or((
                std::time::Duration::from_secs(GATEWAY_AGENT_TIMEOUT_SECS),
                std::time::Duration::from_secs(GATEWAY_AGENT_TIMEOUT_SECS),
            ))
    }

    pub(super) async fn auto_send_gateway_response(
        &self,
        thread_id: &str,
        msg: &gateway::IncomingMessage,
        reply_tool_name: &str,
    ) {
        let last_response = {
            let threads = self.threads.read().await;
            let Some(thread) = threads.get(thread_id) else {
                return;
            };
            if gateway_turn_used_send_tool(&thread.messages) {
                return;
            }
            latest_gateway_turn_assistant_response(&thread.messages)
        };

        if let Some(response_text) = last_response {
            tracing::info!(
                platform = %msg.platform,
                "gateway: agent forgot to call send tool, auto-sending response"
            );

            let _ = self
                .send_gateway_platform_tool(thread_id, "auto", reply_tool_name, msg, &response_text)
                .await;
        }
    }

    pub(super) async fn handle_gateway_full_agent_response(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        existing_thread: Option<&str>,
        history_window: Option<&str>,
        reply_tool_name: &str,
    ) {
        let active_responder_name = match existing_thread {
            Some(thread_id) => self
                .active_agent_id_for_thread(thread_id)
                .await
                .map(|agent_id| canonical_agent_name(&agent_id).to_string()),
            None => None,
        };
        let enriched_prompt = build_gateway_agent_prompt(
            &msg.platform,
            &msg.sender,
            &msg.content,
            history_window,
            reply_tool_name,
            active_responder_name.as_deref(),
        );
        let gateway_timeout_budget = self.gateway_timeout_budget().await;
        let send_result = Box::pin(tokio::time::timeout(
            gateway_timeout_budget.0,
            self.send_message_with_ephemeral_user_override(
                existing_thread,
                &msg.content,
                &enriched_prompt,
                gateway_timeout_budget.1,
            ),
        ))
        .await;

        match send_result {
            Err(_) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    timeout_secs = gateway_timeout_budget.0.as_secs(),
                    "gateway: full agent response timed out"
                );
                self.send_gateway_timeout_fallback(msg, reply_tool_name)
                    .await;
            }
            Ok(Err(e)) => {
                tracing::error!(
                    platform = %msg.platform,
                    error = %e,
                    "gateway: failed to process incoming message"
                );
            }
            Ok(Ok(thread_id)) => {
                self.gateway_threads
                    .write()
                    .await
                    .insert(channel_key.to_string(), thread_id.clone());
                if let Err(error) = self
                    .history
                    .upsert_gateway_thread_binding(channel_key, &thread_id, now_millis())
                    .await
                {
                    tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist thread binding");
                }
            }
        }
    }
}
