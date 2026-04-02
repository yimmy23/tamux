use super::*;

fn make_episode() -> Episode {
    Episode {
        id: "ep-001".to_string(),
        goal_run_id: Some("goal-1".to_string()),
        thread_id: Some("thread-1".to_string()),
        session_id: Some("session-1".to_string()),
        goal_text: Some("Completed the deployment task".to_string()),
        goal_type: Some("goal_run".to_string()),
        episode_type: EpisodeType::GoalCompletion,
        summary: "Completed the deployment task".to_string(),
        outcome: EpisodeOutcome::Success,
        root_cause: Some("Config mismatch".to_string()),
        entities: vec!["deploy.yml".to_string(), "staging".to_string()],
        causal_chain: vec![CausalStep {
            step: "step-1".to_string(),
            cause: "wrong config path".to_string(),
            effect: "deployment failed initially".to_string(),
        }],
        solution_class: Some("config-fix".to_string()),
        duration_ms: Some(5000),
        tokens_used: Some(1200),
        confidence: Some(0.95),
        confidence_before: Some(0.7),
        confidence_after: Some(0.95),
        created_at: 1700000000000,
        expires_at: Some(1700000000000 + 90 * 86400 * 1000),
    }
}

#[test]
fn episode_round_trip_serialization() {
    let episode = make_episode();
    let json = serde_json::to_string(&episode).expect("serialize");
    let deserialized: Episode = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.id, episode.id);
    assert_eq!(deserialized.goal_run_id, episode.goal_run_id);
    assert_eq!(deserialized.thread_id, episode.thread_id);
    assert_eq!(deserialized.session_id, episode.session_id);
    assert_eq!(deserialized.goal_text, episode.goal_text);
    assert_eq!(deserialized.goal_type, episode.goal_type);
    assert_eq!(deserialized.episode_type, episode.episode_type);
    assert_eq!(deserialized.summary, episode.summary);
    assert_eq!(deserialized.outcome, episode.outcome);
    assert_eq!(deserialized.root_cause, episode.root_cause);
    assert_eq!(deserialized.entities, episode.entities);
    assert_eq!(deserialized.solution_class, episode.solution_class);
    assert_eq!(deserialized.duration_ms, episode.duration_ms);
    assert_eq!(deserialized.tokens_used, episode.tokens_used);
    assert_eq!(deserialized.confidence, episode.confidence);
    assert_eq!(deserialized.confidence_before, episode.confidence_before);
    assert_eq!(deserialized.confidence_after, episode.confidence_after);
    assert_eq!(deserialized.created_at, episode.created_at);
    assert_eq!(deserialized.expires_at, episode.expires_at);
    assert_eq!(deserialized.causal_chain.len(), 1);
    assert_eq!(deserialized.causal_chain[0].step, "step-1");
}

#[test]
fn episode_type_serde_snake_case() {
    assert_eq!(
        serde_json::to_string(&EpisodeType::GoalStart).unwrap(),
        "\"goal_start\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeType::GoalCompletion).unwrap(),
        "\"goal_completion\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeType::GoalFailure).unwrap(),
        "\"goal_failure\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeType::SessionEnd).unwrap(),
        "\"session_end\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeType::Discovery).unwrap(),
        "\"discovery\""
    );
}

#[test]
fn episode_outcome_serde_snake_case() {
    assert_eq!(
        serde_json::to_string(&EpisodeOutcome::Success).unwrap(),
        "\"success\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeOutcome::Failure).unwrap(),
        "\"failure\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeOutcome::Partial).unwrap(),
        "\"partial\""
    );
    assert_eq!(
        serde_json::to_string(&EpisodeOutcome::Abandoned).unwrap(),
        "\"abandoned\""
    );
}

#[test]
fn episodic_config_default_values() {
    let config = EpisodicConfig::default();
    assert!(config.enabled);
    assert_eq!(config.episode_ttl_days, 90);
    assert_eq!(config.constraint_ttl_days, 30);
    assert_eq!(config.max_retrieval_episodes, 5);
    assert_eq!(config.max_injection_tokens, 1500);
    assert!(!config.per_session_suppression);
    assert!(config.suppressed_session_ids.is_empty());
}

#[test]
fn link_type_serde_snake_case() {
    assert_eq!(
        serde_json::to_string(&LinkType::RetryOf).unwrap(),
        "\"retry_of\""
    );
    assert_eq!(
        serde_json::to_string(&LinkType::BuildsOn).unwrap(),
        "\"builds_on\""
    );
    assert_eq!(
        serde_json::to_string(&LinkType::Contradicts).unwrap(),
        "\"contradicts\""
    );
    assert_eq!(
        serde_json::to_string(&LinkType::Supersedes).unwrap(),
        "\"supersedes\""
    );
    assert_eq!(
        serde_json::to_string(&LinkType::CausedBy).unwrap(),
        "\"caused_by\""
    );
}

#[test]
fn negative_constraint_round_trip_serialization() {
    let constraint = NegativeConstraint {
        id: "nc-001".to_string(),
        episode_id: Some("ep-001".to_string()),
        constraint_type: ConstraintType::RuledOut,
        subject: "npm install approach".to_string(),
        solution_class: Some("dependency-resolution".to_string()),
        description: "npm install causes version conflict with react 19".to_string(),
        confidence: 0.9,
        state: ConstraintState::Dead,
        evidence_count: 3,
        direct_observation: false,
        derived_from_constraint_ids: vec!["nc-parent".to_string()],
        related_subject_tokens: vec!["deploy".to_string(), "config".to_string()],
        valid_until: Some(1700000000000),
        created_at: 1699000000000,
    };

    let json = serde_json::to_string(&constraint).expect("serialize");
    let deserialized: NegativeConstraint = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.id, constraint.id);
    assert_eq!(deserialized.episode_id, constraint.episode_id);
    assert_eq!(deserialized.constraint_type, ConstraintType::RuledOut);
    assert_eq!(deserialized.subject, constraint.subject);
    assert_eq!(deserialized.solution_class, constraint.solution_class);
    assert_eq!(deserialized.description, constraint.description);
    assert_eq!(deserialized.confidence, constraint.confidence);
    assert_eq!(deserialized.state, constraint.state);
    assert_eq!(deserialized.evidence_count, constraint.evidence_count);
    assert_eq!(
        deserialized.direct_observation,
        constraint.direct_observation
    );
    assert_eq!(
        deserialized.derived_from_constraint_ids,
        constraint.derived_from_constraint_ids
    );
    assert_eq!(
        deserialized.related_subject_tokens,
        constraint.related_subject_tokens
    );
    assert_eq!(deserialized.valid_until, constraint.valid_until);
    assert_eq!(deserialized.created_at, constraint.created_at);
}

#[test]
fn negative_constraint_deserialization_applies_backward_compat_defaults() {
    let json = r#"{
        "id":"nc-legacy",
        "episode_id":"ep-001",
        "constraint_type":"ruled_out",
        "subject":"legacy deploy approach",
        "solution_class":"deployment",
        "description":"legacy payload without new metadata",
        "confidence":0.8,
        "valid_until":1700000000000,
        "created_at":1699000000000
    }"#;

    let deserialized: NegativeConstraint = serde_json::from_str(json).expect("deserialize");

    assert_eq!(deserialized.state, ConstraintState::Dying);
    assert_eq!(deserialized.evidence_count, 1);
    assert!(deserialized.direct_observation);
    assert!(deserialized.derived_from_constraint_ids.is_empty());
    assert!(deserialized.related_subject_tokens.is_empty());
}

#[test]
fn counter_who_state_default_has_empty_fields() {
    let state = CounterWhoState::default();
    assert!(state.goal_run_id.is_none());
    assert!(state.thread_id.is_none());
    assert!(state.current_focus.is_none());
    assert!(state.recent_changes.is_empty());
    assert!(state.tried_approaches.is_empty());
    assert!(state.correction_patterns.is_empty());
    assert!(state.active_constraint_ids.is_empty());
    assert_eq!(state.updated_at, 0);
}
