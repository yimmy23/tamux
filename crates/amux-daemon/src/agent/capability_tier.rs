//! Capability tier system -- progressive feature disclosure driven by operator model.
//!
//! The daemon resolves a [`CapabilityTier`] from operator-model signals, an
//! optional self-assessment, and an optional user override.  The tier drives
//! which features are visible to the operator and controls the progressive
//! disclosure queue.

use serde::{Deserialize, Serialize};

use super::operator_model::RiskTolerance;

// ---------------------------------------------------------------------------
// CapabilityTier enum
// ---------------------------------------------------------------------------

/// Progressive capability tier.  Order matters for `PartialOrd`/`Ord` -- each
/// successive variant represents a higher tier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTier {
    Newcomer,
    Familiar,
    PowerUser,
    Expert,
}

impl std::fmt::Display for CapabilityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Newcomer => write!(f, "newcomer"),
            Self::Familiar => write!(f, "familiar"),
            Self::PowerUser => write!(f, "power_user"),
            Self::Expert => write!(f, "expert"),
        }
    }
}

impl CapabilityTier {
    /// Parse from a string (e.g. from protocol messages or config).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "newcomer" => Some(Self::Newcomer),
            "familiar" => Some(Self::Familiar),
            "power_user" | "poweruser" => Some(Self::PowerUser),
            "expert" => Some(Self::Expert),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// TierSignals -- inputs to resolve_tier()
// ---------------------------------------------------------------------------

/// Aggregated signals used to compute the current capability tier.
#[derive(Debug, Clone)]
pub(super) struct TierSignals {
    pub session_count: u64,
    pub unique_tools_used: usize,
    pub goal_runs_completed: u64,
    pub risk_tolerance: RiskTolerance,
    pub user_self_assessment: Option<CapabilityTier>,
    pub user_override: Option<CapabilityTier>,
}

// ---------------------------------------------------------------------------
// resolve_tier() -- pure function
// ---------------------------------------------------------------------------

/// Compute the effective tier from the given signals.
///
/// Rules:
/// 1. `user_override` always wins (D-03).
/// 2. Compute a *behavioral* tier from session/tool/goal signals.
/// 3. `self_assessment` can *elevate* the behavioral tier but never demote it
///    (D-01: hybrid, elevates only).
pub(super) fn resolve_tier(signals: &TierSignals) -> CapabilityTier {
    // Rule 1: override takes precedence
    if let Some(tier) = signals.user_override {
        return tier;
    }

    // Rule 2: behavioral tier from signals
    let behavioral = if signals.goal_runs_completed >= 10
        && signals.unique_tools_used >= 8
        && signals.risk_tolerance == RiskTolerance::Aggressive
    {
        CapabilityTier::Expert
    } else if signals.goal_runs_completed >= 3 && signals.unique_tools_used >= 5 {
        CapabilityTier::PowerUser
    } else if signals.session_count >= 5 && signals.unique_tools_used >= 3 {
        CapabilityTier::Familiar
    } else {
        CapabilityTier::Newcomer
    };

    // Rule 3: self-assessment elevates only
    if let Some(assessment) = signals.user_self_assessment {
        if assessment > behavioral {
            return assessment;
        }
    }

    behavioral
}

// ---------------------------------------------------------------------------
// TierFeatureFlags -- per-tier feature visibility
// ---------------------------------------------------------------------------

/// Feature visibility flags driven by the current tier (D-04).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TierFeatureFlags {
    pub show_goal_runs: bool,
    pub show_task_queue: bool,
    pub show_gateway_config: bool,
    pub show_subagents: bool,
    pub show_advanced_settings: bool,
    pub show_memory_controls: bool,
}

/// Map a tier to its visible feature set.
pub(super) fn tier_features_visible(tier: CapabilityTier) -> TierFeatureFlags {
    match tier {
        CapabilityTier::Newcomer => TierFeatureFlags {
            show_goal_runs: false,
            show_task_queue: false,
            show_gateway_config: false,
            show_subagents: false,
            show_advanced_settings: false,
            show_memory_controls: false,
        },
        CapabilityTier::Familiar => TierFeatureFlags {
            show_goal_runs: true,
            show_task_queue: true,
            show_gateway_config: true,
            show_subagents: false,
            show_advanced_settings: false,
            show_memory_controls: false,
        },
        CapabilityTier::PowerUser => TierFeatureFlags {
            show_goal_runs: true,
            show_task_queue: true,
            show_gateway_config: true,
            show_subagents: true,
            show_advanced_settings: true,
            show_memory_controls: false,
        },
        CapabilityTier::Expert => TierFeatureFlags {
            show_goal_runs: true,
            show_task_queue: true,
            show_gateway_config: true,
            show_subagents: true,
            show_advanced_settings: true,
            show_memory_controls: true,
        },
    }
}

// ---------------------------------------------------------------------------
// DisclosureQueue -- one-per-session feature draining (D-13)
// ---------------------------------------------------------------------------

/// A single feature disclosure entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDisclosure {
    pub feature_id: String,
    pub tier: CapabilityTier,
    pub title: String,
    pub description: String,
}

/// Queue of features awaiting progressive disclosure.  At most one feature is
/// surfaced per session (D-13).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisclosureQueue {
    pub pending_features: Vec<FeatureDisclosure>,
    pub disclosed_features: Vec<String>,
    pub last_disclosure_session: u64,
}

impl DisclosureQueue {
    /// Returns the next feature to disclose, or `None` if we already disclosed
    /// one this session or if the queue is empty.
    pub fn next_disclosure(&self, current_session: u64) -> Option<&FeatureDisclosure> {
        if self.last_disclosure_session == current_session {
            return None;
        }
        self.pending_features.first()
    }

    /// Mark a feature as disclosed and update the session watermark.
    pub fn mark_disclosed(&mut self, feature_id: &str, current_session: u64) {
        self.pending_features.retain(|f| f.feature_id != feature_id);
        if !self.disclosed_features.iter().any(|id| id == feature_id) {
            self.disclosed_features.push(feature_id.to_string());
        }
        self.last_disclosure_session = current_session;
    }
}

// ---------------------------------------------------------------------------
// TierConfig -- persisted in agent config
// ---------------------------------------------------------------------------

/// Tier settings persisted in `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TierConfig {
    pub enabled: bool,
    pub user_override: Option<CapabilityTier>,
    pub user_self_assessment: Option<CapabilityTier>,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known_tier: Option<String>,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            user_override: None,
            user_self_assessment: None,
            onboarding_completed: false,
            last_known_tier: None,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentEngine tier integration
// ---------------------------------------------------------------------------

use amux_protocol::DaemonMessage;

use super::engine::AgentEngine;

impl AgentEngine {
    /// Compute the current effective tier from operator model + config signals.
    pub(super) async fn compute_current_tier(&self) -> CapabilityTier {
        let config = self.config.read().await;
        let tier_config = &config.tier;

        if !tier_config.enabled {
            // Tier system disabled -- fall back to self-assessment or Newcomer.
            return tier_config
                .user_self_assessment
                .unwrap_or(CapabilityTier::Newcomer);
        }

        let model = self.operator_model.read().await;

        let signals = TierSignals {
            session_count: model.session_count,
            unique_tools_used: model.unique_tools_seen.len(),
            goal_runs_completed: model.goal_runs_completed,
            risk_tolerance: model.risk_fingerprint.risk_tolerance,
            user_self_assessment: tier_config.user_self_assessment,
            user_override: tier_config.user_override,
        };

        resolve_tier(&signals)
    }

    /// Build a full status snapshot for the `AgentStatusResponse` protocol message.
    pub async fn get_status_snapshot(&self) -> DaemonMessage {
        let tier = self.compute_current_tier().await;
        let flags = tier_features_visible(tier);
        let feature_flags_json = serde_json::to_string(&flags).unwrap_or_else(|_| "{}".to_string());

        // Activity state
        let goal_runs = self.goal_runs.lock().await;
        let inflight = self.inflight_goal_runs.lock().await;
        let (activity, active_goal_run_id, active_goal_run_title) = if !inflight.is_empty() {
            let active_id = inflight.iter().next().cloned();
            let title = active_id.as_ref().and_then(|id| {
                goal_runs
                    .iter()
                    .find(|g| &g.id == id)
                    .map(|g| g.title.clone())
            });
            ("goal_running".to_string(), active_id, title)
        } else {
            ("idle".to_string(), None, None)
        };
        drop(goal_runs);
        drop(inflight);

        // Active thread: pick the most recently updated thread (heuristic).
        let threads = self.threads.read().await;
        let active_thread_id = threads
            .values()
            .max_by_key(|t| t.messages.last().map(|m| m.timestamp).unwrap_or(0))
            .map(|t| t.id.clone());
        drop(threads);

        // Provider health: serialize circuit breaker summaries with outage context.
        let provider_health_json = {
            let snapshots = super::engine::collect_provider_health_snapshot(
                &self.config,
                &self.circuit_breakers,
            )
            .await;
            let mut health = serde_json::Map::new();
            for snapshot in snapshots {
                if let Ok(value) = serde_json::to_value(&snapshot) {
                    health.insert(snapshot.provider_id, value);
                }
            }
            serde_json::to_string(&health).unwrap_or_else(|_| "{}".to_string())
        };

        // Gateway statuses
        let gateway_statuses_json = {
            let snapshots = self.gateway_health_snapshots().await;
            if !snapshots.is_empty() {
                let mut statuses = serde_json::Map::new();
                for snapshot in snapshots {
                    statuses.insert(
                        snapshot.platform.clone(),
                        serde_json::json!({
                            "status": format!("{:?}", snapshot.status),
                            "consecutive_failures": snapshot.consecutive_failure_count,
                        }),
                    );
                }
                serde_json::to_string(&statuses).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        };

        // Recent audit actions (last 5).
        let recent_actions_json = match self.history.list_action_audit(None, None, 5).await {
            Ok(entries) => {
                let items: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "id": e.id,
                            "timestamp": e.timestamp,
                            "action_type": e.action_type,
                            "summary": e.summary,
                        })
                    })
                    .collect();
                serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
            }
            Err(_) => "[]".to_string(),
        };

        let diagnostics_json = self.operator_profile_diagnostics_snapshot().to_string();

        DaemonMessage::AgentStatusResponse {
            tier: tier.to_string(),
            feature_flags_json,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
            diagnostics_json,
        }
    }

    /// Set or clear the tier override, persist config, and broadcast change.
    pub async fn set_tier_override(&self, tier: Option<CapabilityTier>) {
        let previous = self.compute_current_tier().await;

        {
            let mut config = self.config.write().await;
            config.tier.user_override = tier;
        }

        // Persist config change.
        self.persist_config().await;

        let new_tier = self.compute_current_tier().await;

        if previous != new_tier {
            let _ = self.event_tx.send(super::types::AgentEvent::TierChanged {
                previous_tier: previous.to_string(),
                new_tier: new_tier.to_string(),
                reason: "user_override".to_string(),
            });
        }
    }

    /// Check if the capability tier has changed since last check.
    /// If changed, broadcast TierChanged event and populate disclosure queue.
    /// Called from heartbeat/gateway_loop on each cycle.
    pub(super) async fn check_tier_change(&self) -> anyhow::Result<()> {
        let new_tier = self.compute_current_tier().await;
        let mut config = self.config.write().await;

        let previous_tier_str = config
            .tier
            .last_known_tier
            .clone()
            .unwrap_or_else(|| "newcomer".to_string());
        let new_tier_str = new_tier.to_string();

        if previous_tier_str != new_tier_str {
            let reason = if config.tier.user_override.is_some() {
                "user_override"
            } else {
                "auto_promotion"
            };

            // Broadcast TierChanged event
            let _ = self.event_tx.send(super::types::AgentEvent::TierChanged {
                previous_tier: previous_tier_str.clone(),
                new_tier: new_tier_str.clone(),
                reason: reason.to_string(),
            });

            // Populate disclosure queue with features unlocked at the new tier
            self.populate_disclosure_queue(&mut config, new_tier).await;

            // Update last_known_tier
            config.tier.last_known_tier = Some(new_tier_str.clone());

            // Persist config change
            drop(config);
            self.persist_config().await;

            // Announce tier transition via concierge (D-12)
            if let Err(e) = self
                .concierge
                .announce_tier_transition(&previous_tier_str, &new_tier_str)
                .await
            {
                tracing::warn!("tier transition announcement failed: {e}");
            }

            tracing::info!(
                previous = %previous_tier_str,
                new = %new_tier_str,
                reason,
                "capability tier changed"
            );
        }

        Ok(())
    }

    /// Populate the disclosure queue with features newly unlocked at the given tier.
    /// Features already disclosed are skipped.
    pub(super) async fn populate_disclosure_queue(
        &self,
        _config: &mut super::types::AgentConfig,
        new_tier: CapabilityTier,
    ) {
        let features = tier_disclosure_features(new_tier);
        let mut queue = self.disclosure_queue.write().await;
        for feature in features {
            if !queue.disclosed_features.contains(&feature.feature_id)
                && !queue
                    .pending_features
                    .iter()
                    .any(|f| f.feature_id == feature.feature_id)
            {
                queue.pending_features.push(feature);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Disclosure features per tier (D-13)
// ---------------------------------------------------------------------------

/// Define features available at each tier for progressive disclosure.
fn tier_disclosure_features(tier: CapabilityTier) -> Vec<FeatureDisclosure> {
    match tier {
        CapabilityTier::Newcomer => vec![],
        CapabilityTier::Familiar => vec![
            FeatureDisclosure {
                feature_id: "goal_runs".into(),
                tier: CapabilityTier::Familiar,
                title: "Goal Runs".into(),
                description: "You can now set multi-step goals and I'll plan and execute them autonomously.".into(),
            },
            FeatureDisclosure {
                feature_id: "task_queue".into(),
                tier: CapabilityTier::Familiar,
                title: "Task Queue".into(),
                description: "Schedule tasks to run later or in the background.".into(),
            },
            FeatureDisclosure {
                feature_id: "gateway_config".into(),
                tier: CapabilityTier::Familiar,
                title: "Chat Gateways".into(),
                description: "Connect me to Slack, Discord, or Telegram so I can work where you communicate.".into(),
            },
        ],
        CapabilityTier::PowerUser => vec![
            FeatureDisclosure {
                feature_id: "subagents".into(),
                tier: CapabilityTier::PowerUser,
                title: "Sub-Agents".into(),
                description: "I can spawn specialized sub-agents for complex tasks \u{2014} like having a team.".into(),
            },
            FeatureDisclosure {
                feature_id: "advanced_settings".into(),
                tier: CapabilityTier::PowerUser,
                title: "Advanced Settings".into(),
                description: "Fine-tune my behavior: model selection, tool policies, and execution preferences.".into(),
            },
        ],
        CapabilityTier::Expert => vec![
            FeatureDisclosure {
                feature_id: "memory_controls".into(),
                tier: CapabilityTier::Expert,
                title: "Memory Controls".into(),
                description: "Full control over my memory: consolidation settings, decay rates, and manual fact management.".into(),
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::engine::AgentEngine;
    use crate::agent::types::{AgentConfig, ApiTransport, AuthSource, ProviderConfig};
    use crate::session_manager::SessionManager;
    use amux_protocol::DaemonMessage;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn make_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
        let temp_dir = TempDir::new().expect("temp dir");
        let session_manager = SessionManager::new_test(temp_dir.path()).await;
        let engine = AgentEngine::new_test(session_manager, config, temp_dir.path()).await;
        (engine, temp_dir)
    }

    fn provider_config(base_url: &str, model: &str, api_key: &str) -> ProviderConfig {
        ProviderConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        }
    }

    fn make_signals() -> TierSignals {
        TierSignals {
            session_count: 0,
            unique_tools_used: 0,
            goal_runs_completed: 0,
            risk_tolerance: RiskTolerance::Moderate,
            user_self_assessment: None,
            user_override: None,
        }
    }

    #[test]
    fn resolve_tier_no_signals_returns_newcomer() {
        let signals = make_signals();
        assert_eq!(resolve_tier(&signals), CapabilityTier::Newcomer);
    }

    #[test]
    fn resolve_tier_familiar_thresholds() {
        let mut signals = make_signals();
        signals.session_count = 5;
        signals.unique_tools_used = 3;
        assert_eq!(resolve_tier(&signals), CapabilityTier::Familiar);
    }

    #[test]
    fn resolve_tier_power_user_thresholds() {
        let mut signals = make_signals();
        signals.goal_runs_completed = 3;
        signals.unique_tools_used = 5;
        assert_eq!(resolve_tier(&signals), CapabilityTier::PowerUser);
    }

    #[test]
    fn resolve_tier_expert_thresholds() {
        let mut signals = make_signals();
        signals.goal_runs_completed = 10;
        signals.unique_tools_used = 8;
        signals.risk_tolerance = RiskTolerance::Aggressive;
        assert_eq!(resolve_tier(&signals), CapabilityTier::Expert);
    }

    #[test]
    fn user_override_expert_always_returns_expert() {
        let mut signals = make_signals();
        signals.user_override = Some(CapabilityTier::Expert);
        assert_eq!(resolve_tier(&signals), CapabilityTier::Expert);
    }

    #[test]
    fn user_override_newcomer_returns_newcomer_despite_high_signals() {
        let mut signals = make_signals();
        signals.session_count = 100;
        signals.unique_tools_used = 20;
        signals.goal_runs_completed = 50;
        signals.risk_tolerance = RiskTolerance::Aggressive;
        signals.user_override = Some(CapabilityTier::Newcomer);
        assert_eq!(resolve_tier(&signals), CapabilityTier::Newcomer);
    }

    #[test]
    fn self_assessment_elevates_behavioral() {
        let mut signals = make_signals();
        signals.session_count = 5;
        signals.unique_tools_used = 3;
        // behavioral = Familiar
        signals.user_self_assessment = Some(CapabilityTier::PowerUser);
        assert_eq!(resolve_tier(&signals), CapabilityTier::PowerUser);
    }

    #[test]
    fn self_assessment_does_not_demote_behavioral() {
        let mut signals = make_signals();
        signals.session_count = 5;
        signals.unique_tools_used = 3;
        // behavioral = Familiar
        signals.user_self_assessment = Some(CapabilityTier::Newcomer);
        assert_eq!(resolve_tier(&signals), CapabilityTier::Familiar);
    }

    #[test]
    fn tier_features_newcomer_sees_fewest() {
        let flags = tier_features_visible(CapabilityTier::Newcomer);
        assert!(!flags.show_goal_runs);
        assert!(!flags.show_task_queue);
        assert!(!flags.show_gateway_config);
        assert!(!flags.show_subagents);
        assert!(!flags.show_advanced_settings);
        assert!(!flags.show_memory_controls);
    }

    #[test]
    fn tier_features_expert_sees_all() {
        let flags = tier_features_visible(CapabilityTier::Expert);
        assert!(flags.show_goal_runs);
        assert!(flags.show_task_queue);
        assert!(flags.show_gateway_config);
        assert!(flags.show_subagents);
        assert!(flags.show_advanced_settings);
        assert!(flags.show_memory_controls);
    }

    #[test]
    fn disclosure_queue_returns_none_same_session() {
        let queue = DisclosureQueue {
            pending_features: vec![FeatureDisclosure {
                feature_id: "goal_runs".to_string(),
                tier: CapabilityTier::Familiar,
                title: "Goal Runs".to_string(),
                description: "Decompose objectives into steps".to_string(),
            }],
            disclosed_features: vec![],
            last_disclosure_session: 42,
        };
        assert!(queue.next_disclosure(42).is_none());
    }

    #[test]
    fn disclosure_queue_returns_first_pending_different_session() {
        let queue = DisclosureQueue {
            pending_features: vec![FeatureDisclosure {
                feature_id: "goal_runs".to_string(),
                tier: CapabilityTier::Familiar,
                title: "Goal Runs".to_string(),
                description: "Decompose objectives into steps".to_string(),
            }],
            disclosed_features: vec![],
            last_disclosure_session: 41,
        };
        let disclosure = queue.next_disclosure(42);
        assert!(disclosure.is_some());
        assert_eq!(disclosure.unwrap().feature_id, "goal_runs");
    }

    #[test]
    fn capability_tier_ordering() {
        assert!(CapabilityTier::Newcomer < CapabilityTier::Familiar);
        assert!(CapabilityTier::Familiar < CapabilityTier::PowerUser);
        assert!(CapabilityTier::PowerUser < CapabilityTier::Expert);
    }

    #[tokio::test]
    async fn status_snapshot_includes_outage_metadata_for_open_provider() {
        let mut config = AgentConfig::default();
        config.providers.insert(
            "custom".to_string(),
            provider_config("https://example.invalid/v1", "model-a", "valid-key"),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;
        {
            let breaker = engine.circuit_breakers.get("custom").await;
            let mut breaker = breaker.lock().await;
            let now = super::super::now_millis();
            for offset in 0..5 {
                breaker.record_failure(now + offset);
            }
        }

        let snapshot = engine.get_status_snapshot().await;
        let DaemonMessage::AgentStatusResponse {
            provider_health_json,
            ..
        } = snapshot
        else {
            panic!("expected agent status response");
        };

        let health: serde_json::Value = serde_json::from_str(&provider_health_json).unwrap();
        let custom = health.get("custom").expect("custom provider health");
        assert_eq!(
            custom.get("can_execute").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(custom.get("trip_count").and_then(|v| v.as_u64()), Some(1));
        assert!(
            custom.get("reason").and_then(|v| v.as_str()).is_some(),
            "expected outage reason in provider health snapshot"
        );
        assert!(
            custom
                .get("suggested_alternatives")
                .and_then(|v| v.as_array())
                .is_some(),
            "expected structured alternatives in provider health snapshot"
        );
    }
}
