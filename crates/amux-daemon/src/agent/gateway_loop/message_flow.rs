use super::*;
use amux_protocol::AGENT_NAME_SWAROG;

impl AgentEngine {
    async fn process_single_gateway_message(&self, msg: gateway::IncomingMessage) {
        if let Some(ref mid) = msg.message_id {
            let mut seen = self.gateway_seen_ids.lock().await;
            if seen.contains(mid) {
                tracing::debug!(
                    message_id = %mid,
                    platform = %msg.platform,
                    "gateway: skipping duplicate message"
                );
                return;
            }
            seen.push(mid.clone());
            if seen.len() > 200 {
                let excess = seen.len() - 200;
                seen.drain(..excess);
            }
        }

        let channel_key = format!("{}:{}", msg.platform, msg.channel);
        {
            let mut inflight = self.gateway_inflight_channels.lock().await;
            if inflight.contains(&channel_key) {
                tracing::warn!(
                    channel_key = %channel_key,
                    "gateway: channel already being processed, skipping"
                );
                return;
            }
            inflight.insert(channel_key.clone());
        }

        tracing::info!(
            platform = %msg.platform,
            sender = %msg.sender,
            channel = %msg.channel,
            content = %msg.content,
            message_id = ?msg.message_id,
            "gateway: incoming message"
        );

        if self.handle_gateway_reset_command(&msg, &channel_key).await {
            self.release_gateway_inflight_channel(&channel_key).await;
            return;
        }

        let (_reply_tool, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        let route_request = classify_gateway_route_request(&msg.content);
        if let Some(request) = route_request {
            self.persist_gateway_route_request(&channel_key, request)
                .await;
            if self
                .handle_gateway_route_switch_ack(&msg, &channel_key, request, reply_tool_name)
                .await
            {
                self.release_gateway_inflight_channel(&channel_key).await;
                return;
            }
        }

        let existing_thread = self.gateway_threads.read().await.get(&channel_key).cloned();
        if self
            .handle_gateway_task_approval_reply(
                &msg,
                &channel_key,
                existing_thread.as_deref(),
                reply_tool_name,
            )
            .await
        {
            self.release_gateway_inflight_channel(&channel_key).await;
            return;
        }

        let _ = self.event_tx.send(AgentEvent::GatewayIncoming {
            platform: msg.platform.clone(),
            sender: msg.sender.clone(),
            content: msg.content.clone(),
            channel: msg.channel.clone(),
        });
        let route_mode = if let Some(request) = route_request {
            request.mode
        } else {
            self.gateway_route_modes
                .read()
                .await
                .get(&channel_key)
                .copied()
                .unwrap_or_default()
        };
        let history_window: Option<String> = self
            .load_gateway_history_window(&channel_key, existing_thread.as_deref())
            .await;

        if route_mode == gateway::GatewayRouteMode::Rarog {
            match self
                .triage_gateway_message(&msg, history_window.as_deref(), existing_thread.as_deref())
                .await
            {
                concierge::GatewayTriage::Simple(response_text) => {
                    if self
                        .handle_concierge_simple_response(
                            &msg,
                            &channel_key,
                            reply_tool_name,
                            &response_text,
                        )
                        .await
                    {
                        self.release_gateway_inflight_channel(&channel_key).await;
                        return;
                    }
                }
                concierge::GatewayTriage::Complex => {}
            }
        } else {
            tracing::info!(
                platform = %msg.platform,
                channel = %msg.channel,
                "gateway: sticky route is set to {}; bypassing concierge triage",
                AGENT_NAME_SWAROG,
            );
        }

        self.handle_gateway_full_agent_response(
            &msg,
            &channel_key,
            existing_thread.as_deref(),
            history_window.as_deref(),
            reply_tool_name,
        )
        .await;
        self.release_gateway_inflight_channel(&channel_key).await;
    }

    pub(super) async fn process_gateway_messages(&self) {
        let now_ms = now_millis();
        let incoming = {
            let mut gw_guard = self.gateway_state.lock().await;
            let Some(gw) = gw_guard.as_mut() else {
                return;
            };

            let mut queue = self.gateway_injected_messages.lock().await;
            let incoming: Vec<gateway::IncomingMessage> = queue.drain(..).collect();
            if incoming.is_empty() {
                return;
            }

            for msg in &incoming {
                let key = format!("{}:{}", msg.platform, msg.channel);
                gw.last_incoming_at.insert(key.clone(), now_ms);
                if let Some(ref tc) = msg.thread_context {
                    gw.reply_contexts.insert(key, tc.clone());
                }
            }
            incoming
        };

        for msg in incoming {
            self.process_single_gateway_message(msg).await;
        }
    }
}
