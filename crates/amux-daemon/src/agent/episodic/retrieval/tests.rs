use super::*;

#[test]
fn format_fts5_query_escapes_special_chars() {
    let raw = r#"deploy "staging" (prod)"#;
    let result = format_fts5_query(raw);
    assert!(!result.contains('"'));
    assert!(!result.contains('('));
    assert!(!result.contains(')'));
    assert!(result.contains("deploy"));
    assert!(result.contains("staging"));
    assert!(result.contains("prod"));
}

#[test]
fn format_fts5_query_converts_words_to_or() {
    let result = format_fts5_query("deploy staging production");
    assert_eq!(result, "deploy OR staging OR production");
}

#[test]
fn format_fts5_query_empty_returns_star() {
    assert_eq!(format_fts5_query(""), "*");
    assert_eq!(format_fts5_query("  "), "*");
    assert_eq!(format_fts5_query("\"()\""), "*");
}

#[test]
fn format_episodic_context_empty_returns_empty() {
    let result = format_episodic_context(&[], 1500);
    assert!(result.is_empty());
}

#[test]
fn format_episodic_context_labels_failure_and_success() {
    let episodes = vec![
        make_test_episode("ep-1", EpisodeOutcome::Failure, "Deploy failed"),
        make_test_episode("ep-2", EpisodeOutcome::Success, "Deploy succeeded"),
    ];
    let result = format_episodic_context(&episodes, 1500);
    assert!(result.contains("WARNING"));
    assert!(result.contains("REFERENCE"));
    assert!(result.contains("Deploy failed"));
    assert!(result.contains("Deploy succeeded"));
    assert!(result.contains("## Past Experience (Episodic Memory)"));
}

#[test]
fn format_episodic_context_truncates_on_token_budget() {
    let episodes: Vec<Episode> = (0..20)
        .map(|i| {
            make_test_episode(
                &format!("ep-{i}"),
                EpisodeOutcome::Success,
                &format!(
                    "Long episode summary number {} with lots of detail about what happened during the execution of this goal run which involved many steps and operations",
                    i
                ),
            )
        })
        .collect();
    let result = format_episodic_context(&episodes, 50);
    assert!(result.contains("omitted due to token budget"));
}

#[test]
fn compute_recency_weight_today_is_one() {
    let now = 1700000000000u64;
    let weight = compute_recency_weight(now, now);
    assert!((weight - 1.0).abs() < 0.01);
}

#[test]
fn compute_recency_weight_7_days_about_half() {
    let now = 1700000000000u64;
    let seven_days_ago = now - 7 * 86400 * 1000;
    let weight = compute_recency_weight(seven_days_ago, now);
    assert!(weight > 0.5, "weight={} should be > 0.5", weight);
    assert!(weight < 0.8, "weight={} should be < 0.8", weight);
}

#[test]
fn compute_recency_weight_30_plus_days_near_zero() {
    let now = 1700000000000u64;
    let thirty_days_ago = now - 30 * 86400 * 1000;
    let weight = compute_recency_weight(thirty_days_ago, now);
    assert!(weight < 0.3, "weight={} should be < 0.3", weight);
    assert!(weight > 0.01, "weight={} should be > 0.01", weight);
}

fn make_test_episode(id: &str, outcome: EpisodeOutcome, summary: &str) -> Episode {
    Episode {
        id: id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        thread_id: Some("thread-1".to_string()),
        session_id: Some("session-1".to_string()),
        goal_text: Some(summary.to_string()),
        goal_type: Some("goal_run".to_string()),
        episode_type: if outcome == EpisodeOutcome::Failure {
            EpisodeType::GoalFailure
        } else {
            EpisodeType::GoalCompletion
        },
        summary: summary.to_string(),
        outcome,
        root_cause: if outcome == EpisodeOutcome::Failure {
            Some("Config error".to_string())
        } else {
            None
        },
        entities: vec!["deploy.yml".to_string()],
        causal_chain: Vec::new(),
        solution_class: None,
        duration_ms: Some(5000),
        tokens_used: Some(1200),
        confidence: None,
        confidence_before: None,
        confidence_after: None,
        created_at: 1700000000000,
        expires_at: None,
    }
}
