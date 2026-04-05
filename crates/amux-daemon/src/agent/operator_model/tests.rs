use super::*;
use tempfile::tempdir;

#[test]
fn cognitive_style_prefers_terse_for_short_messages() {
    assert_eq!(
        verbosity_preference_for_length(6.0),
        VerbosityPreference::Terse
    );
    assert_eq!(reading_depth_for_length(6.0), ReadingDepth::Skim);
}

#[test]
fn risk_metrics_compute_category_rates_and_tolerance() {
    let mut risk = RiskFingerprint {
        approvals: 4,
        denials: 1,
        category_requests: HashMap::from([
            ("git".to_string(), 2),
            ("network_request".to_string(), 3),
        ]),
        category_approvals: HashMap::from([
            ("git".to_string(), 2),
            ("network_request".to_string(), 2),
        ]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(risk.risk_tolerance, RiskTolerance::Aggressive);
    assert_eq!(
        risk.approval_rate_by_category.get("git").copied(),
        Some(1.0)
    );
    assert_eq!(
        risk.approval_rate_by_category
            .get("network_request")
            .copied(),
        Some(2.0 / 3.0)
    );
}

#[test]
fn classify_command_category_uses_command_heuristics_first() {
    assert_eq!(
        classify_command_category("rm -rf target", "highest"),
        "destructive_delete"
    );
    assert_eq!(
        classify_command_category("curl https://example.com", "moderate"),
        "network_request"
    );
}

#[test]
fn normalize_attention_surface_keeps_supported_separators() {
    assert_eq!(
        normalize_attention_surface(" modal:settings:SubAgents "),
        "modal:settings:subagents"
    );
}

#[test]
fn top_keys_orders_by_count_then_name() {
    let mut histogram = HashMap::new();
    histogram.insert("conversation:chat".to_string(), 4);
    histogram.insert("conversation:input".to_string(), 4);
    histogram.insert("modal:settings:provider".to_string(), 1);

    assert_eq!(
        top_keys(&histogram, 2),
        vec![
            "conversation:chat".to_string(),
            "conversation:input".to_string()
        ]
    );
}

#[test]
fn detect_revision_signal_finds_corrections() {
    assert_eq!(
        detect_revision_signal("Actually, use ripgrep instead."),
        RevisionSignal::Correction
    );
    assert_eq!(
        detect_revision_signal("Next time prefer the shorter answer."),
        RevisionSignal::Revision
    );
    assert_eq!(
        detect_revision_signal("Please run tests."),
        RevisionSignal::None
    );
}

#[test]
fn detect_reading_signal_finds_summary_and_reasoning_preferences() {
    assert_eq!(
        detect_reading_signal("Give me the tl;dr first."),
        ReadingSignal::SummaryRequest
    );
    assert_eq!(
        detect_reading_signal("Just the answer, no reasoning."),
        ReadingSignal::SkipReasoning
    );
    assert_eq!(
        detect_reading_signal("Show me the full trace and walk through it step by step."),
        ReadingSignal::DeepDetailRequest
    );
    assert_eq!(
        detect_reading_signal("Please run tests."),
        ReadingSignal::None
    );
}

#[test]
fn reading_depth_uses_behavioral_signals_not_just_message_length() {
    assert_eq!(reading_depth_for_profile(18.0, 3, 0, 2), ReadingDepth::Skim);
    assert_eq!(
        reading_depth_for_profile(8.0, 0, 3, 0),
        ReadingDepth::Standard
    );
}

#[test]
fn ema_update_basic_calculation() {
    let result = ema_update(10.0, 20.0, 0.3);
    assert!((result - 13.0).abs() < f64::EPSILON);
}

#[test]
fn ema_update_converges_toward_sample() {
    let mut value = 0.0;
    for _ in 0..50 {
        value = ema_update(value, 100.0, 0.3);
    }
    assert!((value - 100.0).abs() < 0.01);
}

#[test]
fn smooth_activity_histogram_applies_ema_to_all_24_hours() {
    let current: HashMap<u8, f64> = HashMap::new();
    let mut observation: HashMap<u8, u64> = HashMap::new();
    observation.insert(9, 5);
    observation.insert(14, 3);

    let smoothed = smooth_activity_histogram(&current, &observation, 0.3);
    assert_eq!(smoothed.len(), 24);
    assert!((smoothed[&9] - 1.5).abs() < f64::EPSILON);
    assert!((smoothed[&14] - 0.9).abs() < f64::EPSILON);
    assert!((smoothed[&0]).abs() < f64::EPSILON);
}

#[test]
fn smooth_activity_histogram_decays_unobserved_hours() {
    let mut current: HashMap<u8, f64> = HashMap::new();
    current.insert(9, 10.0);
    let observation: HashMap<u8, u64> = HashMap::new();

    let smoothed = smooth_activity_histogram(&current, &observation, 0.3);
    assert!((smoothed[&9] - 7.0).abs() < f64::EPSILON);
}

#[test]
fn record_attention_event_tracks_dwell_and_rapid_switches() {
    let mut model = OperatorModel::default();
    record_attention_event(&mut model, "conversation:chat", 1_000);
    record_attention_event(&mut model, "modal:settings", 6_000);
    record_attention_event(&mut model, "conversation:chat", 10_000);
    record_attention_event(&mut model, "conversation:chat", 50_000);

    assert_eq!(model.attention_topology.focus_event_count, 4);
    assert_eq!(model.attention_topology.rapid_switch_count, 2);
    assert_eq!(
        model.attention_topology.deep_focus_surface.as_deref(),
        Some("conversation:chat")
    );
    assert!(model.attention_topology.avg_surface_dwell_secs > 0.0);
}

#[test]
fn risk_metrics_learn_auto_approve_and_auto_deny_categories() {
    let mut risk = RiskFingerprint {
        approvals: 6,
        denials: 4,
        category_requests: HashMap::from([
            ("git".to_string(), 4),
            ("destructive_delete".to_string(), 4),
            ("network_request".to_string(), 2),
        ]),
        category_approvals: HashMap::from([
            ("git".to_string(), 4),
            ("destructive_delete".to_string(), 0),
            ("network_request".to_string(), 2),
        ]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(
        risk.auto_approve_categories,
        vec!["git".to_string()],
        "high-volume, always-approved categories should become shortcuts"
    );
    assert_eq!(
        risk.auto_deny_categories,
        vec!["destructive_delete".to_string()],
        "high-volume, never-approved categories should become auto-deny heuristics"
    );
}

#[test]
fn risk_metrics_learn_auto_deny_from_repeated_fast_denials() {
    let mut risk = RiskFingerprint {
        approvals: 1,
        denials: 3,
        category_requests: HashMap::from([("network_request".to_string(), 4)]),
        category_approvals: HashMap::from([("network_request".to_string(), 1)]),
        fast_denials_by_category: HashMap::from([("network_request".to_string(), 3)]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(
        risk.auto_deny_categories,
        vec!["network_request".to_string()],
        "three fast denials in one category should learn an approval bypass even before the long-run approval rate drops to zero"
    );
}

#[tokio::test]
async fn reset_operator_model_clears_learned_shortcuts() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.risk_fingerprint.auto_approve_categories = vec!["git".to_string()];
        model.risk_fingerprint.auto_deny_categories = vec!["destructive_delete".to_string()];
        persist_operator_model(&engine.data_dir, &model).expect("persist learned model");
    }

    engine
        .reset_operator_model()
        .await
        .expect("reset operator model");

    let json = engine
        .operator_model_json()
        .await
        .expect("reload operator model json");
    let parsed: OperatorModel = serde_json::from_str(&json).expect("parse operator model json");
    assert!(parsed.risk_fingerprint.auto_approve_categories.is_empty());
    assert!(parsed.risk_fingerprint.auto_deny_categories.is_empty());
}

#[tokio::test]
async fn repeated_fast_denials_enable_learned_auto_deny() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let command = "curl https://example.com/status";
    let risk_level = "moderate";

    let approved = ToolPendingApproval {
        approval_id: "approval-approve".to_string(),
        execution_id: "exec-approve".to_string(),
        command: command.to_string(),
        rationale: "Fetch service status".to_string(),
        risk_level: risk_level.to_string(),
        blast_radius: "single endpoint".to_string(),
        reasons: vec!["network access".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&approved)
        .await
        .expect("record approval request");
    engine
        .record_operator_approval_resolution(&approved.approval_id, ApprovalDecision::ApproveOnce)
        .await
        .expect("record approval resolution");

    for idx in 0..3 {
        let denial = ToolPendingApproval {
            approval_id: format!("approval-deny-{idx}"),
            execution_id: format!("exec-deny-{idx}"),
            command: command.to_string(),
            rationale: "Fetch service status".to_string(),
            risk_level: risk_level.to_string(),
            blast_radius: "single endpoint".to_string(),
            reasons: vec!["network access".to_string()],
            session_id: None,
        };
        engine
            .record_operator_approval_requested(&denial)
            .await
            .expect("record denial request");
        engine
            .record_operator_approval_resolution(&denial.approval_id, ApprovalDecision::Deny)
            .await
            .expect("record denial resolution");
    }

    assert!(matches!(
        engine.learned_approval_decision(command, risk_level).await,
        Some(ApprovalDecision::Deny)
    ));
}

#[tokio::test]
async fn operator_messages_learn_summary_first_reasoning_on_demand_preferences() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let messages = [
        "Give me the tl;dr first.",
        "Use the short version again.",
        "Just the answer, no reasoning.",
        "Skip the explanation and summarize it.",
    ];
    for (index, message) in messages.iter().enumerate() {
        engine
            .record_operator_message("thread-reading", message, index == 0)
            .await
            .expect("record operator message");
    }

    let json = engine
        .operator_model_json()
        .await
        .expect("read operator model json");
    let parsed: OperatorModel = serde_json::from_str(&json).expect("parse operator model json");
    assert!(parsed.cognitive_style.prefers_summaries);
    assert!(parsed.cognitive_style.skips_reasoning);
    assert_eq!(parsed.cognitive_style.reading_depth, ReadingDepth::Skim);

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains("summary-first"));
    assert!(summary.contains("reasoning on demand"));
}
