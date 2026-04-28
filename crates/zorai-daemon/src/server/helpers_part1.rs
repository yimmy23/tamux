struct WhatsAppLinkSubscriberGuard {
    agent: Arc<AgentEngine>,
    subscriber_id: Option<u64>,
}

impl WhatsAppLinkSubscriberGuard {
    fn new(agent: Arc<AgentEngine>) -> Self {
        Self {
            agent,
            subscriber_id: None,
        }
    }

    async fn set(&mut self, subscriber_id: u64) {
        if let Some(previous) = self.subscriber_id.replace(subscriber_id) {
            self.agent.whatsapp_link.unsubscribe(previous).await;
        }
    }

    async fn clear(&mut self) {
        if let Some(subscriber_id) = self.subscriber_id.take() {
            self.agent.whatsapp_link.unsubscribe(subscriber_id).await;
        }
    }
}

impl Drop for WhatsAppLinkSubscriberGuard {
    fn drop(&mut self) {
        if let Some(subscriber_id) = self.subscriber_id.take() {
            let agent = self.agent.clone();
            tokio::spawn(async move {
                agent.whatsapp_link.unsubscribe(subscriber_id).await;
            });
        }
    }
}

#[derive(Debug, Clone)]
enum GatewayConnectionState {
    Unregistered,
    AwaitingBootstrapAck {
        registration: GatewayRegistration,
        bootstrap_correlation_id: String,
    },
    Active {
        registration: GatewayRegistration,
    },
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn gateway_feature_flags(config: &crate::agent::types::GatewayConfig) -> Vec<String> {
    let mut flags = Vec::new();
    if config.enabled {
        flags.push("gateway_enabled".to_string());
    }
    if config.gateway_electron_bridges_enabled {
        flags.push("gateway_electron_bridges_enabled".to_string());
    }
    if config.whatsapp_link_fallback_electron {
        flags.push("whatsapp_link_fallback_electron".to_string());
    }
    flags
}

fn gateway_provider_bootstrap(
    platform: &str,
    enabled: bool,
    credentials: serde_json::Value,
    config: serde_json::Value,
) -> GatewayProviderBootstrap {
    GatewayProviderBootstrap {
        platform: platform.to_string(),
        enabled,
        credentials_json: credentials.to_string(),
        config_json: config.to_string(),
    }
}

fn gateway_bootstrap_providers(
    config: &crate::agent::types::GatewayConfig,
) -> Vec<GatewayProviderBootstrap> {
    vec![
        gateway_provider_bootstrap(
            "slack",
            config.enabled && !config.slack_token.trim().is_empty(),
            serde_json::json!({ "token": config.slack_token }),
            serde_json::json!({
                "channel_filter": config.slack_channel_filter,
                "command_prefix": config.command_prefix,
            }),
        ),
        gateway_provider_bootstrap(
            "discord",
            config.enabled && !config.discord_token.trim().is_empty(),
            serde_json::json!({ "token": config.discord_token }),
            serde_json::json!({
                "channel_filter": config.discord_channel_filter,
                "allowed_users": config.discord_allowed_users,
                "command_prefix": config.command_prefix,
            }),
        ),
        gateway_provider_bootstrap(
            "telegram",
            config.enabled && !config.telegram_token.trim().is_empty(),
            serde_json::json!({ "token": config.telegram_token }),
            serde_json::json!({
                "allowed_chats": config.telegram_allowed_chats,
                "command_prefix": config.command_prefix,
            }),
        ),
        gateway_provider_bootstrap(
            "whatsapp",
            config.enabled
                && (!config.whatsapp_token.trim().is_empty()
                    || !config.whatsapp_allowed_contacts.trim().is_empty()),
            serde_json::json!({
                "token": config.whatsapp_token,
                "phone_id": config.whatsapp_phone_id,
            }),
            serde_json::json!({
                "allowed_contacts": config.whatsapp_allowed_contacts,
                "command_prefix": config.command_prefix,
                "fallback_electron": config.whatsapp_link_fallback_electron,
            }),
        ),
    ]
}

fn parse_gateway_route_mode(value: &str) -> GatewayRouteMode {
    GatewayRouteMode::parse(value)
}

async fn build_gateway_bootstrap_payload(
    agent: &AgentEngine,
    bootstrap_correlation_id: String,
) -> GatewayBootstrapPayload {
    let gateway = agent.config.read().await.gateway.clone();

    let mut cursors = Vec::new();
    for platform in ["slack", "discord", "telegram", "whatsapp"] {
        match agent.history.load_gateway_replay_cursors(platform).await {
            Ok(rows) => cursors.extend(rows.into_iter().map(|row| GatewayCursorState {
                platform: row.platform,
                channel_id: row.channel_id,
                cursor_value: row.cursor_value,
                cursor_type: row.cursor_type,
                updated_at_ms: row.updated_at,
            })),
            Err(error) => {
                tracing::warn!(platform, %error, "gateway: failed to load replay cursors for bootstrap");
            }
        }
    }

    let thread_bindings = match agent.history.list_gateway_thread_bindings().await {
        Ok(rows) => rows
            .into_iter()
            .map(|(channel_key, thread_id)| GatewayThreadBindingState {
                channel_key,
                thread_id: Some(thread_id),
                updated_at_ms: 0,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "gateway: failed to load thread bindings for bootstrap");
            Vec::new()
        }
    };

    let route_modes = match agent.history.list_gateway_route_modes().await {
        Ok(rows) => rows
            .into_iter()
            .map(|(channel_key, route_mode)| GatewayRouteModeState {
                channel_key,
                route_mode: parse_gateway_route_mode(&route_mode),
                updated_at_ms: 0,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "gateway: failed to load route modes for bootstrap");
            Vec::new()
        }
    };
    let health_snapshots = match agent.history.list_gateway_health_snapshots().await {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|row| {
                serde_json::from_str::<GatewayHealthState>(&row.state_json)
                    .map_err(|error| {
                        tracing::warn!(
                            platform = %row.platform,
                            %error,
                            "gateway: failed to parse health snapshot for bootstrap"
                        );
                    })
                    .ok()
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "gateway: failed to load health snapshots for bootstrap");
            Vec::new()
        }
    };

    GatewayBootstrapPayload {
        bootstrap_correlation_id,
        feature_flags: gateway_feature_flags(&gateway),
        providers: gateway_bootstrap_providers(&gateway),
        continuity: GatewayContinuityState {
            cursors,
            thread_bindings,
            route_modes,
            health_snapshots,
        },
    }
}

fn gateway_connection_is_active(state: &GatewayConnectionState) -> bool {
    matches!(state, GatewayConnectionState::Active { .. })
}

fn gateway_connection_is_tracked(state: &GatewayConnectionState) -> bool {
    !matches!(state, GatewayConnectionState::Unregistered)
}

fn gateway_thread_context_from_event(
    platform: &str,
    thread_id: Option<&str>,
) -> Option<crate::agent::gateway::ThreadContext> {
    let thread_id = thread_id?.trim();
    if thread_id.is_empty() {
        return None;
    }

    match platform.to_ascii_lowercase().as_str() {
        "slack" => Some(crate::agent::gateway::ThreadContext {
            slack_thread_ts: Some(thread_id.to_string()),
            ..Default::default()
        }),
        "discord" => Some(crate::agent::gateway::ThreadContext {
            discord_message_id: Some(thread_id.to_string()),
            ..Default::default()
        }),
        "telegram" => {
            thread_id
                .parse::<i64>()
                .ok()
                .map(|message_id| crate::agent::gateway::ThreadContext {
                    telegram_message_id: Some(message_id),
                    ..Default::default()
                })
        }
        _ => None,
    }
}

async fn enqueue_gateway_incoming_event(
    agent: &Arc<AgentEngine>,
    event: GatewayIncomingEvent,
) -> Result<()> {
    let gateway_message = crate::agent::gateway::IncomingMessage {
        thread_context: gateway_thread_context_from_event(
            &event.platform,
            event.thread_id.as_deref(),
        ),
        platform: event.platform,
        sender: event.sender_display.unwrap_or(event.sender_id),
        content: event.content,
        channel: event.channel_id,
        message_id: event.message_id,
    };
    agent.enqueue_gateway_message(gateway_message).await
}
