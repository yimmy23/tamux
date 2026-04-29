use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn sample_skill_variant(
    variant_id: &str,
    skill_name: &str,
    variant_name: &str,
    status: &str,
    context_tags: &[&str],
    success_count: u32,
    failure_count: u32,
) -> crate::history::SkillVariantRecord {
    crate::history::SkillVariantRecord {
        variant_id: variant_id.to_string(),
        skill_name: skill_name.to_string(),
        variant_name: variant_name.to_string(),
        relative_path: format!("skills/{skill_name}/{variant_name}.md"),
        parent_variant_id: None,
        version: "v1".to_string(),
        context_tags: context_tags.iter().map(|tag| tag.to_string()).collect(),
        use_count: success_count + failure_count,
        success_count,
        failure_count,
        fitness_score: 0.84,
        status: status.to_string(),
        last_used_at: Some(2_000),
        created_at: 1_000,
        updated_at: 2_000,
    }
}

fn sample_goal_run(goal_run_id: &str, thread_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "Recover repo state".to_string(),
        goal: "recover after command failure".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: Some(thread_id.to_string()),
        root_thread_id: Some(thread_id.to_string()),
        active_thread_id: Some(thread_id.to_string()),
        execution_thread_ids: vec![thread_id.to_string()],
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("Recover".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("repair the failed workflow".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        stopped_reason: None,
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
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Recover".to_string(),
            instructions: "repair the failed flow".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "workflow is stable again".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: super::autonomy::AutonomyLevel::Aware,
        authorship_tag: None,
    }
}

#[test]
fn estimated_success_probability_defaults_when_no_history() {
    assert!((estimated_success_probability(0, 0, false) - 0.65).abs() < f64::EPSILON);
    assert!((estimated_success_probability(0, 0, true) - 0.35).abs() < f64::EPSILON);
}

#[test]
fn plan_success_estimate_decreases_with_complexity() {
    assert!(estimate_plan_success(2, 0) > estimate_plan_success(6, 3));
}

#[test]
fn command_family_normalizes_prefix() {
    assert_eq!(command_family("git push origin main"), "git_push");
    assert_eq!(command_family("rm -rf build"), "rm__rf");
}

#[test]
fn summarize_outcome_preserves_recovery_for_near_miss() {
    let summary = summarize_outcome(
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong: "command timed out".to_string(),
            how_recovered: "replanned into smaller steps".to_string(),
        },
    )
    .expect("near miss should summarize");

    assert!(summary.is_near_miss);
    assert_eq!(summary.reason, "command timed out");
    assert_eq!(
        summary.recovery.as_deref(),
        Some("replanned into smaller steps")
    );
}

#[test]
fn family_outcome_summary_tracks_failures_and_near_misses() {
    let mut summary = FamilyOutcomeSummary::default();
    summary.record(OutcomeSummary {
        reason: "permissions denied".to_string(),
        recovery: None,
        is_near_miss: false,
    });
    summary.record(OutcomeSummary {
        reason: "command timed out".to_string(),
        recovery: Some("replanned into smaller steps".to_string()),
        is_near_miss: true,
    });

    assert_eq!(summary.failure_count, 1);
    assert_eq!(summary.near_miss_count, 1);
    assert_eq!(summary.reasons.len(), 2);
    assert_eq!(summary.recoveries, vec!["replanned into smaller steps"]);
}

#[tokio::test]
async fn causal_guidance_summary_includes_upstream_recovery_patterns() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "upstream_recovery",
        "reasoning": "Recovered from a daemon-generated invalid upstream request.",
        "rejection_reason": null,
        "estimated_success_prob": 0.72,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let factors_json = serde_json::to_string(&vec![
        crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: "upstream signature: request-invalid-empty-tool-name".to_string(),
            weight: 0.9,
        },
        crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
            description: "automatic retry repaired thread state before continuing".to_string(),
            weight: 0.6,
        },
    ])
    .expect("serialize factors");
    let outcome_json = serde_json::to_string(
        &crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong: "provider rejected invalid tool metadata".to_string(),
            how_recovered: "repair the thread state and retry once".to_string(),
        },
    )
    .expect("serialize outcome");

    engine
        .history
        .insert_causal_trace(
            "causal_test_upstream_recovery",
            Some("thread-upstream-guidance"),
            None,
            None,
            "recovery",
            crate::agent::learning::traces::DecisionType::Recovery.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            &factors_json,
            &outcome_json,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert causal trace");

    let summary = engine
        .build_causal_guidance_summary()
        .await
        .expect("expected causal guidance summary");
    assert!(
        summary.contains("upstream recovery / request_invalid_empty_tool_name"),
        "expected upstream recovery guidance in summary: {summary}"
    );
    assert!(
        summary.contains("repair the thread state and retry once"),
        "expected the recovery pattern to be surfaced in summary: {summary}"
    );
}

#[tokio::test]
async fn settle_goal_plan_causal_traces_marks_unresolved_plan_success() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_plan",
        "reasoning": "Use a three-step plan",
        "rejection_reason": null,
        "estimated_success_prob": 0.72,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");

    engine
        .history
        .insert_causal_trace(
            "causal_test_goal_plan_success",
            Some("thread-goal-plan"),
            Some("goal-plan-1"),
            None,
            "plan_selection",
            crate::agent::learning::traces::DecisionType::PlanSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert causal trace");

    let updated = engine
        .settle_goal_plan_causal_traces("goal-plan-1", "success", None)
        .await;
    assert_eq!(updated, 1);

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_plan", 1)
        .await
        .expect("list settled goal plan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    assert!(matches!(
        outcome,
        crate::agent::learning::traces::CausalTraceOutcome::Success
    ));
}

#[tokio::test]
async fn settle_goal_plan_causal_traces_marks_unresolved_replan_failure() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_replan",
        "reasoning": "Retry with smaller recovery steps",
        "rejection_reason": null,
        "estimated_success_prob": 0.54,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");

    engine
        .history
        .insert_causal_trace(
            "causal_test_goal_replan_failure",
            Some("thread-goal-replan"),
            Some("goal-replan-1"),
            Some("task-replan-1"),
            "replan_selection",
            crate::agent::learning::traces::DecisionType::ReplanSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert causal trace");

    let updated = engine
        .settle_goal_plan_causal_traces(
            "goal-replan-1",
            "failure",
            Some("the revised plan still failed at execution time"),
        )
        .await;
    assert_eq!(updated, 1);

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_replan", 1)
        .await
        .expect("list settled goal replan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("revised plan"));
        }
        other => panic!("expected failure outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn persist_skill_selection_causal_trace_records_trace_and_audit_entry() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected = sample_skill_variant(
        "variant-selected",
        "rust_audit",
        "canonical",
        "active",
        &["rust", "testing"],
        5,
        1,
    );
    let rejected = sample_skill_variant(
        "variant-rejected",
        "rust_audit",
        "fallback",
        "active",
        &["general"],
        1,
        2,
    );
    let context_tags = vec!["rust".to_string(), "verification".to_string()];

    engine
        .persist_skill_selection_causal_trace(
            "thread-skill-trace",
            None,
            Some("task-skill-trace"),
            &selected,
            &[selected.clone(), rejected],
            &context_tags,
        )
        .await;

    let traces = engine
        .history
        .list_recent_causal_trace_records("rust_audit", 1)
        .await
        .expect("list skill traces");
    assert_eq!(traces.len(), 1);

    let selected_option = serde_json::from_str::<crate::agent::learning::traces::DecisionOption>(
        &traces[0].selected_json,
    )
    .expect("deserialize selected option");
    assert_eq!(selected_option.option_type, "rust_audit");
    assert!(selected_option.reasoning.contains("canonical"));

    let factors = serde_json::from_str::<Vec<crate::agent::learning::traces::CausalFactor>>(
        &traces[0].causal_factors_json,
    )
    .expect("deserialize factors");
    assert!(factors.iter().any(|factor| factor
        .description
        .contains("matched skill context tags: rust")));

    let audits = engine
        .history
        .list_action_audit(Some(&["skill".to_string()]), None, 10)
        .await
        .expect("list skill audit entries");
    let audit = audits
        .iter()
        .find(|entry| entry.thread_id.as_deref() == Some("thread-skill-trace"))
        .expect("skill audit entry should be persisted");
    assert_eq!(audit.action_type, "skill");
    assert_eq!(audit.task_id.as_deref(), Some("task-skill-trace"));
    assert!(audit.summary.contains("rust_audit"));
    assert!(audit.causal_trace_id.is_some());
}

#[tokio::test]
async fn settle_skill_selection_causal_traces_maps_success_failure_and_cancelled_outcomes() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let success_variant = sample_skill_variant(
        "variant-success",
        "skill_success",
        "canonical",
        "active",
        &["rust"],
        3,
        0,
    );
    engine
        .persist_skill_selection_causal_trace(
            "thread-skill-success",
            None,
            Some("task-skill-success"),
            &success_variant,
            std::slice::from_ref(&success_variant),
            &["rust".to_string()],
        )
        .await;

    let failure_variant = sample_skill_variant(
        "variant-failure",
        "skill_failure",
        "canonical",
        "active",
        &["ops"],
        2,
        1,
    );
    engine
        .persist_skill_selection_causal_trace(
            "thread-skill-failure",
            Some("goal-skill-failure"),
            None,
            &failure_variant,
            std::slice::from_ref(&failure_variant),
            &["ops".to_string()],
        )
        .await;

    let cancelled_variant = sample_skill_variant(
        "variant-cancelled",
        "skill_cancelled",
        "canonical",
        "active",
        &["shell"],
        1,
        1,
    );
    engine
        .persist_skill_selection_causal_trace(
            "thread-skill-cancelled",
            None,
            None,
            &cancelled_variant,
            std::slice::from_ref(&cancelled_variant),
            &["shell".to_string()],
        )
        .await;

    assert_eq!(
        engine
            .settle_skill_selection_causal_traces(
                Some("thread-skill-success"),
                Some("task-skill-success"),
                None,
                "success",
            )
            .await,
        1
    );
    assert_eq!(
        engine
            .settle_skill_selection_causal_traces(
                Some("thread-skill-failure"),
                None,
                Some("goal-skill-failure"),
                "failure",
            )
            .await,
        1
    );
    assert_eq!(
        engine
            .settle_skill_selection_causal_traces(
                Some("thread-skill-cancelled"),
                None,
                None,
                "cancelled",
            )
            .await,
        1
    );

    let success = engine
        .history
        .list_recent_causal_trace_records("skill_success", 1)
        .await
        .expect("list success skill trace");
    let failure = engine
        .history
        .list_recent_causal_trace_records("skill_failure", 1)
        .await
        .expect("list failure skill trace");
    let cancelled = engine
        .history
        .list_recent_causal_trace_records("skill_cancelled", 1)
        .await
        .expect("list cancelled skill trace");

    assert!(matches!(
        serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
            &success[0].outcome_json
        )
        .expect("deserialize success outcome"),
        crate::agent::learning::traces::CausalTraceOutcome::Success
    ));

    match serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &failure[0].outcome_json,
    )
    .expect("deserialize failure outcome")
    {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("did not lead to successful completion"));
        }
        other => panic!("expected failure outcome, got {other:?}"),
    }

    match serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &cancelled[0].outcome_json,
    )
    .expect("deserialize cancelled outcome")
    {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("cancelled before validating"));
        }
        other => panic!("expected cancelled failure outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn persist_recovery_near_miss_trace_records_near_miss_and_checkpoint_factor() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal_run = sample_goal_run("goal-recovery-trace", "thread-recovery-trace");
    let checkpoint = crate::agent::liveness::state_layers::CheckpointData::new(
        "checkpoint-recovery-1".to_string(),
        goal_run.id.clone(),
        crate::agent::liveness::state_layers::CheckpointType::PreRecovery,
        goal_run.clone(),
        now_millis(),
    );
    let checkpoint_json = serde_json::to_string(&checkpoint).expect("serialize checkpoint");
    engine
        .history
        .upsert_checkpoint(
            &checkpoint.id,
            &goal_run.id,
            goal_run.thread_id.as_deref(),
            None,
            crate::agent::liveness::state_layers::CheckpointType::PreRecovery,
            &checkpoint_json,
            Some("pre recovery snapshot"),
            now_millis(),
        )
        .await
        .expect("persist checkpoint");

    let mut failed_task = engine
        .enqueue_task(
            "Recover step".to_string(),
            "recover after failure".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            Some(goal_run.id.clone()),
            None,
            goal_run.thread_id.clone(),
            None,
        )
        .await;
    failed_task.goal_step_title = Some("Collect repo state".to_string());

    let revised = GoalPlanResponse {
        title: Some("Recovered plan".to_string()),
        summary: "Retry safely after narrowing the failing command".to_string(),
        steps: vec![GoalPlanStepResponse {
            title: "Retry safely".to_string(),
            instructions: "rerun the narrow read-only command".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "repo state loads without failure".to_string(),
            execution_binding: None,
            verification_binding: None,
            proof_checks: Vec::new(),
            session_id: None,
            llm_confidence: None,
            llm_confidence_rationale: None,
        }],
        rejected_alternatives: Vec::new(),
    };

    engine
        .persist_recovery_near_miss_trace(
            &goal_run,
            &failed_task,
            "command timed out while collecting repo state",
            &revised,
        )
        .await;

    let traces = engine
        .history
        .list_recent_causal_trace_records("replan_after_failure", 1)
        .await
        .expect("list recovery traces");
    assert_eq!(traces.len(), 1);

    let factors = serde_json::from_str::<Vec<crate::agent::learning::traces::CausalFactor>>(
        &traces[0].causal_factors_json,
    )
    .expect("deserialize recovery factors");
    assert!(factors
        .iter()
        .any(|factor| factor.description.contains("checkpoint(s) were available")));

    match serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &traces[0].outcome_json,
    )
    .expect("deserialize recovery outcome")
    {
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong,
            how_recovered,
        } => {
            assert!(what_went_wrong.contains("timed out"));
            assert!(how_recovered.contains("Retry safely"));
        }
        other => panic!("expected near miss outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn command_blast_radius_advisory_summarizes_recent_failures_and_near_misses() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "execute_managed_command",
        "reasoning": "run a git push command",
        "rejection_reason": null,
        "estimated_success_prob": 0.52,
        "arguments_hash": "ctx_git_push"
    })
    .to_string();
    let factors_json = serde_json::to_string(&vec![crate::agent::learning::traces::CausalFactor {
        factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
        description: "command family: git_push".to_string(),
        weight: 0.9,
    }])
    .expect("serialize factors");

    engine
        .history
        .insert_causal_trace(
            "causal_git_push_failure",
            Some("thread-git-push"),
            None,
            Some("task-git-push-1"),
            "tool_selection",
            crate::agent::learning::traces::DecisionType::ToolSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_git_push_failure",
            &factors_json,
            &serde_json::to_string(
                &crate::agent::learning::traces::CausalTraceOutcome::Failure {
                    reason: "remote rejected non-fast-forward update".to_string(),
                },
            )
            .expect("serialize failure outcome"),
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert failure trace");

    engine
        .history
        .insert_causal_trace(
            "causal_git_push_near_miss",
            Some("thread-git-push"),
            None,
            Some("task-git-push-2"),
            "tool_selection",
            crate::agent::learning::traces::DecisionType::ToolSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_git_push_near_miss",
            &factors_json,
            &serde_json::to_string(
                &crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                    what_went_wrong: "permission denied on remote push".to_string(),
                    how_recovered: "fell back to fetch-only inspection".to_string(),
                },
            )
            .expect("serialize near miss outcome"),
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert near miss trace");

    let advisory = engine
        .command_blast_radius_advisory("execute_managed_command", "git push origin main")
        .await
        .expect("advisory should be generated");

    assert_eq!(advisory.family, "git_push");
    assert_eq!(advisory.risk_level, "high");
    assert!(advisory
        .evidence
        .contains("1 failure(s) and 1 near-miss(es)"));
    assert!(advisory
        .recent_reasons
        .iter()
        .any(|reason| reason.contains("non-fast-forward")));
    assert!(advisory
        .recent_reasons
        .iter()
        .any(|reason| reason.contains("permission denied")));
}
