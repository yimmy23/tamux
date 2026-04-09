use super::*;

fn make_field(
    key: &str,
    confidence: f64,
    updated_at_secs: i64,
    value_json: &str,
) -> OperatorProfileFieldRow {
    OperatorProfileFieldRow {
        field_key: key.to_string(),
        field_value_json: value_json.to_string(),
        confidence,
        source: "test".to_string(),
        updated_at: updated_at_secs,
    }
}

fn make_event(kind: &str, timestamp_ms: u64) -> AgentEventRow {
    AgentEventRow {
        id: format!("evt_{kind}_{timestamp_ms}"),
        category: "behavioral".to_string(),
        kind: kind.to_string(),
        pane_id: None,
        workspace_id: None,
        surface_id: None,
        session_id: None,
        payload_json: "{}".to_string(),
        timestamp: timestamp_ms as i64,
    }
}

fn make_goal(status: GoalRunStatus, priority: TaskPriority) -> GoalRun {
    GoalRun {
        id: "goal-1".to_string(),
        title: "goal".to_string(),
        goal: "goal".to_string(),
        client_request_id: None,
        status,
        priority,
        created_at: 1,
        updated_at: 1,
        started_at: Some(1),
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        replan_count: 0,
        max_replans: 2,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: Vec::new(),
        events: Vec::new(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: Default::default(),
        authorship_tag: None,
    }
}

fn make_checkin_with_metadata(
    kind: CheckinKind,
    now_ms: u64,
    trigger: &str,
    session_id: Option<&str>,
    question_index: u8,
) -> OperatorProfileCheckinRow {
    build_scheduled_checkin(
        kind,
        now_ms,
        trigger,
        PassiveSignalKind::OperatorMessage,
        session_id,
        Some("work"),
        question_index,
    )
    .expect("checkin")
}

#[test]
fn operator_profile_checkins_confidence_decay_trigger() {
    let now_ms = 31 * 24 * 60 * 60 * 1000;
    let fields = vec![make_field("primary_goals", 0.55, 0, "\"ship\"")];
    assert!(has_confidence_decay_trigger(&fields, now_ms));
}

#[test]
fn operator_profile_checkins_confidence_decay_requires_more_than_30_days() {
    let now_ms = 30 * 24 * 60 * 60 * 1000;
    let fields = vec![make_field("primary_goals", 0.55, 0, "\"ship\"")];
    assert!(!has_confidence_decay_trigger(&fields, now_ms));
}

#[test]
fn operator_profile_checkins_missing_critical_fields_trigger() {
    let fields = vec![make_field("preferred_name", 1.0, 1, "\"mk\"")];
    assert!(has_missing_critical_fields_trigger(&fields));
}

#[test]
fn operator_profile_checkins_behavior_delta_trigger_without_min_volume_gate() {
    let now_ms = 40 * 24 * 60 * 60 * 1000;
    let events = vec![
        make_event("operator_message", now_ms - (1 * 24 * 60 * 60 * 1000)),
        make_event("operator_message", now_ms - (2 * 24 * 60 * 60 * 1000)),
        make_event("operator_message", now_ms - (3 * 24 * 60 * 60 * 1000)),
        make_event("operator_message", now_ms - (4 * 24 * 60 * 60 * 1000)),
    ];
    assert!(has_behavior_delta_trigger(&events, now_ms));
}

#[test]
fn operator_profile_checkins_critical_goal_suppression() {
    let mut goals = VecDeque::new();
    goals.push_back(make_goal(GoalRunStatus::Running, TaskPriority::Urgent));
    assert!(is_in_critical_goal_execution_window(&goals));
}

#[test]
fn operator_profile_checkins_policy_applies_guards_and_schedules() {
    let now_ms = 35 * 24 * 60 * 60 * 1000;
    let fields = vec![];
    let checkins = vec![];
    let events = vec![make_event("operator_message", now_ms - 1000)];
    let goals = VecDeque::new();
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &fields,
        checkins: &checkins,
        events: &events,
        goal_runs: &goals,
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: true,
            proactive_suggestions: true,
        },
    });
    assert!(decision.schedule_weekly);
    assert_eq!(
        decision.schedule_contextual,
        Some(ContextualTrigger::MissingCriticalFields)
    );
    assert_eq!(decision.contextual_question_index, 1);
}

#[test]
fn operator_profile_checkins_policy_blocks_when_passive_learning_disabled() {
    let now_ms = 35 * 24 * 60 * 60 * 1000;
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: false,
            weekly_checkins: true,
            proactive_suggestions: true,
        },
    });
    assert!(!decision.schedule_weekly);
    assert!(decision.schedule_contextual.is_none());
}

#[test]
fn operator_profile_checkins_policy_blocks_weekly_when_disabled() {
    let now_ms = 35 * 24 * 60 * 60 * 1000;
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: false,
            proactive_suggestions: true,
        },
    });
    assert!(!decision.schedule_weekly);
}

#[test]
fn operator_profile_checkins_policy_blocks_proactive_when_disabled() {
    let now_ms = 35 * 24 * 60 * 60 * 1000;
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: true,
            proactive_suggestions: false,
        },
    });
    assert!(decision.schedule_contextual.is_none());
}

#[test]
fn operator_profile_checkins_policy_obeys_contextual_cooldown() {
    let now_ms = 10 * 24 * 60 * 60 * 1000;
    let recent_contextual = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - (12 * 60 * 60 * 1000),
        "missing_critical_fields",
        Some("thread-1"),
        1,
    );
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[recent_contextual],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: false,
            proactive_suggestions: true,
        },
    });
    assert!(decision.schedule_contextual.is_none());
}

#[test]
fn operator_profile_checkins_policy_limits_questions_per_session() {
    let now_ms = 15 * 24 * 60 * 60 * 1000;
    let c1 = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - CONTEXTUAL_CHECKIN_COOLDOWN_MS - 5_000,
        "missing_critical_fields",
        Some("thread-1"),
        1,
    );
    let c2 = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - CONTEXTUAL_CHECKIN_COOLDOWN_MS - 4_000,
        "confidence_decay",
        Some("thread-1"),
        2,
    );
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[c1, c2],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: false,
            proactive_suggestions: true,
        },
    });
    assert!(decision.schedule_contextual.is_none());
}

#[test]
fn operator_profile_checkins_policy_ignores_cancelled_questions_for_session_limit() {
    let now_ms = 200_000_000_000;
    let c1 = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - CONTEXTUAL_CHECKIN_COOLDOWN_MS - 5_000,
        "missing_critical_fields",
        Some("thread-1"),
        1,
    );
    let mut cancelled = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - CONTEXTUAL_CHECKIN_COOLDOWN_MS - 4_000,
        "confidence_decay",
        Some("thread-1"),
        2,
    );
    cancelled.status = "cancelled".to_string();
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[c1, cancelled],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: false,
            proactive_suggestions: true,
        },
    });
    assert_eq!(
        decision.schedule_contextual,
        Some(ContextualTrigger::MissingCriticalFields)
    );
    assert_eq!(decision.contextual_question_index, 2);
}

#[test]
fn operator_profile_checkins_policy_limits_questions_per_onboarding_session() {
    let now_ms = 15 * 24 * 60 * 60 * 1000;
    let c1 = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - CONTEXTUAL_CHECKIN_COOLDOWN_MS - 5_000,
        "missing_critical_fields",
        Some("thread-1"),
        1,
    );
    let c2 = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms - CONTEXTUAL_CHECKIN_COOLDOWN_MS - 4_000,
        "confidence_decay",
        Some("thread-1"),
        2,
    );
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("first_run_onboarding"),
        fields: &[],
        checkins: &[c1, c2],
        events: &[],
        goal_runs: &VecDeque::new(),
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: false,
            proactive_suggestions: true,
        },
    });
    assert!(decision.schedule_contextual.is_none());
}

#[test]
fn operator_profile_checkins_policy_suppresses_during_critical_goal_window() {
    let now_ms = 20 * 24 * 60 * 60 * 1000;
    let goals = VecDeque::from([make_goal(GoalRunStatus::Planning, TaskPriority::Urgent)]);
    let decision = evaluate_passive_checkin_policy(&PassiveCheckinInput {
        now_ms,
        signal: PassiveSignalKind::OperatorMessage,
        session_id: Some("thread-1"),
        session_kind: Some("work"),
        fields: &[],
        checkins: &[],
        events: &[],
        goal_runs: &goals,
        consents: ConsentSnapshot {
            passive_learning: true,
            weekly_checkins: true,
            proactive_suggestions: true,
        },
    });
    assert!(!decision.schedule_weekly);
    assert!(decision.schedule_contextual.is_none());
}

#[test]
fn operator_profile_checkins_scheduler_metadata_is_deterministic() {
    let now_ms = 42_000;
    let left = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms,
        "missing_critical_fields",
        Some("thread-1"),
        1,
    );
    let right = make_checkin_with_metadata(
        CheckinKind::Contextual,
        now_ms,
        "missing_critical_fields",
        Some("thread-1"),
        1,
    );
    assert_eq!(left.id, right.id);
    assert_eq!(left.response_json, right.response_json);
    assert_eq!(left.status, "scheduled");
}
