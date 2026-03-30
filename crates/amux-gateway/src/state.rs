use std::collections::HashMap;

use amux_protocol::{
    GatewayBootstrapPayload, GatewayCursorState, GatewayHealthState, GatewayProviderBootstrap,
    GatewayRouteMode, GatewayRouteModeState, GatewayThreadBindingState,
};

#[derive(Debug, Clone, Default)]
pub struct GatewayRuntimeState {
    bootstrap_correlation_id: String,
    feature_flags: Vec<String>,
    providers: HashMap<String, GatewayProviderBootstrap>,
    cursors: HashMap<String, GatewayCursorState>,
    thread_bindings: HashMap<String, GatewayThreadBindingState>,
    route_modes: HashMap<String, GatewayRouteModeState>,
    health_snapshots: HashMap<String, GatewayHealthState>,
}

impl GatewayRuntimeState {
    pub fn from_bootstrap(payload: &GatewayBootstrapPayload) -> Self {
        let providers = payload
            .providers
            .iter()
            .cloned()
            .map(|provider| (provider.platform.to_ascii_lowercase(), provider))
            .collect::<HashMap<_, _>>();

        let cursors = payload
            .continuity
            .cursors
            .iter()
            .cloned()
            .map(|cursor| (cursor_key(&cursor.platform, &cursor.channel_id), cursor))
            .collect::<HashMap<_, _>>();

        let thread_bindings = payload
            .continuity
            .thread_bindings
            .iter()
            .cloned()
            .map(|binding| (binding.channel_key.to_ascii_lowercase(), binding))
            .collect::<HashMap<_, _>>();

        let route_modes = payload
            .continuity
            .route_modes
            .iter()
            .cloned()
            .map(|mode| (mode.channel_key.to_ascii_lowercase(), mode))
            .collect::<HashMap<_, _>>();
        let health_snapshots = payload
            .continuity
            .health_snapshots
            .iter()
            .cloned()
            .map(|snapshot| (snapshot.platform.to_ascii_lowercase(), snapshot))
            .collect::<HashMap<_, _>>();

        Self {
            bootstrap_correlation_id: payload.bootstrap_correlation_id.clone(),
            feature_flags: payload.feature_flags.clone(),
            providers,
            cursors,
            thread_bindings,
            route_modes,
            health_snapshots,
        }
    }

    pub fn bootstrap_correlation_id(&self) -> &str {
        &self.bootstrap_correlation_id
    }

    pub fn feature_flags(&self) -> &[String] {
        &self.feature_flags
    }

    pub fn provider(&self, platform: &str) -> Option<&GatewayProviderBootstrap> {
        self.providers.get(&platform.to_ascii_lowercase())
    }

    pub fn thread_binding(&self, channel_key: &str) -> Option<String> {
        self.thread_bindings
            .get(&channel_key.to_ascii_lowercase())
            .and_then(|binding| binding.thread_id.clone())
    }

    pub fn route_mode(&self, channel_key: &str) -> Option<GatewayRouteMode> {
        self.route_modes
            .get(&channel_key.to_ascii_lowercase())
            .map(|mode| mode.route_mode)
    }

    pub fn cursor(&self, platform: &str, channel_id: &str) -> Option<&GatewayCursorState> {
        self.cursors.get(&cursor_key(platform, channel_id))
    }

    pub fn health_snapshot(&self, platform: &str) -> Option<&GatewayHealthState> {
        self.health_snapshots.get(&platform.to_ascii_lowercase())
    }

    pub fn apply_cursor_update(&mut self, update: GatewayCursorState) {
        self.cursors
            .insert(cursor_key(&update.platform, &update.channel_id), update);
    }

    pub fn apply_thread_binding_update(&mut self, update: GatewayThreadBindingState) {
        self.thread_bindings
            .insert(update.channel_key.to_ascii_lowercase(), update);
    }

    pub fn apply_route_mode_update(&mut self, update: GatewayRouteModeState) {
        self.route_modes
            .insert(update.channel_key.to_ascii_lowercase(), update);
    }

    pub fn apply_health_update(&mut self, update: GatewayHealthState) {
        self.health_snapshots
            .insert(update.platform.to_ascii_lowercase(), update);
    }
}

fn cursor_key(platform: &str, channel_id: &str) -> String {
    format!(
        "{}:{}",
        platform.to_ascii_lowercase(),
        channel_id.to_ascii_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use amux_protocol::{
        GatewayBootstrapPayload, GatewayConnectionStatus, GatewayContinuityState,
        GatewayHealthState,
    };

    use super::GatewayRuntimeState;

    #[test]
    fn health_snapshots_are_normalized_and_overwritten_case_insensitively() {
        let payload = GatewayBootstrapPayload {
            bootstrap_correlation_id: "bootstrap-1".to_string(),
            feature_flags: Vec::new(),
            providers: Vec::new(),
            continuity: GatewayContinuityState {
                cursors: Vec::new(),
                thread_bindings: Vec::new(),
                route_modes: Vec::new(),
                health_snapshots: vec![GatewayHealthState {
                    platform: "Slack".to_string(),
                    status: GatewayConnectionStatus::Connected,
                    last_success_at_ms: Some(100),
                    last_error_at_ms: None,
                    consecutive_failure_count: 0,
                    last_error: None,
                    current_backoff_secs: 0,
                }],
            },
        };

        let mut state = GatewayRuntimeState::from_bootstrap(&payload);
        assert_eq!(
            state.health_snapshot("slack").map(|value| value.status),
            Some(GatewayConnectionStatus::Connected)
        );

        state.apply_health_update(GatewayHealthState {
            platform: "slack".to_string(),
            status: GatewayConnectionStatus::Error,
            last_success_at_ms: Some(100),
            last_error_at_ms: Some(200),
            consecutive_failure_count: 1,
            last_error: Some("timeout".to_string()),
            current_backoff_secs: 30,
        });

        let snapshot = state
            .health_snapshot("SLACK")
            .expect("snapshot should be normalized");
        assert_eq!(snapshot.status, GatewayConnectionStatus::Error);
        assert_eq!(snapshot.consecutive_failure_count, 1);
        assert_eq!(snapshot.last_error.as_deref(), Some("timeout"));
    }
}
