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
        stop_sequences: None,
        temperature: None,
        top_p: None,
        top_k: None,
        metadata: None,
        service_tier: None,
        container: None,
        inference_geo: None,
        cache_control: None,
        max_tokens: None,
        anthropic_tool_choice: None,
        output_effort: None,
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
        amux_shared::providers::PROVIDER_ID_CUSTOM.to_string(),
        provider_config("https://example.invalid/v1", "model-a", "valid-key"),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;
    {
        let breaker = engine
            .circuit_breakers
            .get(amux_shared::providers::PROVIDER_ID_CUSTOM)
            .await;
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
    let custom = health
        .get(amux_shared::providers::PROVIDER_ID_CUSTOM)
        .expect("custom provider health");
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
