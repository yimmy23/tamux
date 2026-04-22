use super::*;
use crate::agent::episodic::{EpisodeType, LinkType};
use crate::session_manager::SessionManager;
use bytes::BytesMut;
use tempfile::tempdir;
use tokio_util::codec::Encoder;

fn sample_supervised_goal_run(goal_run_id: &str, task_id: &str, approval_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "supervised goal".to_string(),
        goal: "verify explicit acknowledgment".to_string(),
        client_request_id: None,
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        stopped_reason: None,
        child_task_ids: vec![task_id.to_string()],
        child_task_count: 1,
        approval_count: 0,
        awaiting_approval_id: Some(approval_id.to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: Some(task_id.to_string()),
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: "do supervised work".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some(task_id.to_string()),
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
        autonomy_level: super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

fn sample_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> GoalRuntimeOwnerProfile {
    GoalRuntimeOwnerProfile {
        agent_label: agent_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

fn sample_assignment(
    role_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main: false,
    }
}

fn goal_run_detail_object(goal_run_json: &str) -> serde_json::Map<String, serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(goal_run_json).expect("parse capped goal run detail json");
    value
        .as_object()
        .cloned()
        .expect("goal run detail payload should be a JSON object")
}

async fn sample_awaiting_task(
    engine: &AgentEngine,
    goal_run_id: &str,
    task_id: &str,
    approval_id: &str,
) {
    engine.tasks.lock().await.push_back(AgentTask {
        id: task_id.to_string(),
        title: "step task".to_string(),
        description: "goal step work".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 30,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("supervised goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting supervised acknowledgment".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });
}

#[tokio::test]
async fn approval_resolution_clears_thread_skill_gate_when_task_is_approved() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval_id = "managed-approval-1";
    let thread_id = "thread-approval-resolution";

    engine
        .set_thread_skill_discovery_state(
            thread_id,
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some(approval_id.to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis(),
            },
        )
        .await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("echo ok".to_string()),
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting approval".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });

    assert!(
        engine
            .handle_task_approval_resolution(
                approval_id,
                amux_protocol::ApprovalDecision::ApproveOnce
            )
            .await
    );

    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(state.compliant);
    assert!(!state.mesh_requires_approval);
}

#[tokio::test]
async fn unrelated_task_approval_does_not_clear_thread_skill_gate() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-unrelated-approval";

    engine
        .set_thread_skill_discovery_state(
            thread_id,
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some("mesh-approval-id".to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis(),
            },
        )
        .await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-unrelated".to_string(),
        title: "approval task unrelated".to_string(),
        description: "awaiting unrelated approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("echo ok".to_string()),
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting approval".to_string()),
        awaiting_approval_id: Some("different-approval-id".to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });

    assert!(
        engine
            .handle_task_approval_resolution(
                "different-approval-id",
                amux_protocol::ApprovalDecision::ApproveOnce
            )
            .await
    );

    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(!state.compliant);
    assert!(state.mesh_requires_approval);
    assert_eq!(state.mesh_approval_id.as_deref(), Some("mesh-approval-id"));
}

#[tokio::test]
async fn denied_task_approval_converts_thread_skill_gate_to_bypassable_state() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval_id = "managed-approval-deny";
    let thread_id = "thread-approval-denied";

    engine
        .set_thread_skill_discovery_state(
            thread_id,
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some(approval_id.to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis(),
            },
        )
        .await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-denied".to_string(),
        title: "approval task denied".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("echo ok".to_string()),
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting approval".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });

    assert!(
        engine
            .handle_task_approval_resolution(approval_id, amux_protocol::ApprovalDecision::Deny)
            .await
    );

    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(!state.mesh_requires_approval);
    assert_eq!(state.recommended_action, "justify_skill_skip");
    assert_eq!(
        state.mesh_next_step,
        Some(crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip)
    );
    assert!(!state.compliant);
}

#[tokio::test]
async fn resume_does_not_clear_supervised_awaiting_approval_gate() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";
    let task_id = "task-supervised";
    let approval_id = "autonomy-ack-1";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine.control_goal_run(goal_run_id, "resume", None).await;
    assert!(
        !changed,
        "resume should not mutate awaiting-approval supervised runs"
    );

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::AwaitingApproval);
    assert_eq!(goal.awaiting_approval_id.as_deref(), Some(approval_id));

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert_eq!(task.awaiting_approval_id.as_deref(), Some(approval_id));
}

#[tokio::test]
async fn explicit_acknowledgment_unblocks_goal_and_current_step_task() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";
    let task_id = "task-supervised";
    let approval_id = "autonomy-ack-2";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine
        .control_goal_run(goal_run_id, "acknowledge", None)
        .await;
    assert!(changed, "acknowledge should clear supervised gate");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Running);
    assert!(goal.awaiting_approval_id.is_none());

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn task_approval_resolution_syncs_parent_goal_run_state() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-policy-escalation";
    let task_id = "task-policy-escalation";
    let approval_id = "policy-escalation-thread_sync-1000";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    assert!(
        engine
            .handle_task_approval_resolution(
                approval_id,
                amux_protocol::ApprovalDecision::ApproveOnce
            )
            .await
    );

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Running);
    assert!(goal.awaiting_approval_id.is_none());

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn create_and_revoke_task_approval_rule_tracks_pending_task_command() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval_id = "approval-rule-1";

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-rule-1".to_string(),
        title: "policy escalation".to_string(),
        description: "needs approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 40,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-1".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some(
            "waiting for operator approval: orchestrator_policy_escalation".to_string(),
        ),
        awaiting_approval_id: Some(approval_id.to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });

    let rule = engine
        .create_task_approval_rule_from_pending(approval_id)
        .await
        .expect("create rule should succeed")
        .expect("rule should be created");
    assert_eq!(rule.command, "orchestrator_policy_escalation");
    assert_eq!(engine.list_task_approval_rules().await.len(), 1);

    assert!(engine.revoke_task_approval_rule(&rule.id).await);
    assert!(engine.list_task_approval_rules().await.is_empty());
}

#[tokio::test]
async fn create_task_approval_rule_from_live_managed_approval_without_task() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval = ToolPendingApproval {
        approval_id: "managed-approval-rule-1".to_string(),
        execution_id: "exec-1".to_string(),
        command: "git status --short".to_string(),
        rationale: "Check repo status".to_string(),
        risk_level: "medium".to_string(),
        blast_radius: "thread".to_string(),
        reasons: vec!["network access requested".to_string()],
        session_id: Some("session-1".to_string()),
    };

    engine.remember_pending_approval_command(&approval).await;

    let rule = engine
        .create_task_approval_rule_from_pending(&approval.approval_id)
        .await
        .expect("create rule should succeed")
        .expect("rule should be created from live approval metadata");

    assert_eq!(rule.command, approval.command);
    assert_eq!(engine.list_task_approval_rules().await.len(), 1);

    engine
        .forget_pending_approval_command(&approval.approval_id)
        .await;
    assert!(engine.revoke_task_approval_rule(&rule.id).await);
}

#[tokio::test]
async fn cancelling_goal_run_settles_unresolved_goal_plan_trace() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised-cancel";
    let task_id = "task-supervised-cancel";
    let approval_id = "autonomy-ack-cancel";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_plan",
        "reasoning": "Use a supervised single-step plan",
        "rejection_reason": null,
        "estimated_success_prob": 0.61,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");
    engine
        .history
        .insert_causal_trace(
            "causal_goal_plan_cancel_hook",
            None,
            Some(goal_run_id),
            None,
            "plan_selection",
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert goal plan causal trace");

    let changed = engine.control_goal_run(goal_run_id, "cancel", None).await;
    assert!(changed, "cancel should update goal state");

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_plan", 1)
        .await
        .expect("list goal plan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("cancelled"));
        }
        other => panic!("expected cancelled failure outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn stopping_goal_run_records_operator_stop_resume_decision() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised-stop";
    let task_id = "task-supervised-stop";
    let approval_id = "autonomy-ack-stop";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine.control_goal_run(goal_run_id, "stop", None).await;
    assert!(changed, "stop should update goal state");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Cancelled);
    assert_eq!(goal.stopped_reason.as_deref(), Some("operator_stop"));
    assert!(goal.awaiting_approval_id.is_none());
    assert!(goal.active_task_id.is_none());

    let dossier = goal.dossier.expect("stop should create dossier state");
    let resume_decision = dossier
        .latest_resume_decision
        .expect("stop should record a resume decision");
    assert_eq!(resume_decision.action, GoalResumeAction::Stop);
    assert_eq!(resume_decision.reason_code, "operator_stop");
    assert_eq!(
        resume_decision.projection_state,
        GoalProjectionState::Failed
    );
}

#[tokio::test]
async fn get_goal_run_capped_for_ipc_truncates_oversized_detail_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-huge-detail";
    let huge_details = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run(goal_run_id, "task-huge", "approval-huge");
    goal_run.events.push(GoalRunEvent {
        id: "event-huge".to_string(),
        timestamp: now_millis(),
        phase: "running".to_string(),
        message: "huge event".to_string(),
        details: Some(huge_details),
        step_index: Some(0),
        todo_snapshot: Vec::new(),
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_capped_for_ipc(goal_run_id)
        .await
        .expect("goal should exist");
    assert!(truncated, "oversized goal detail should be truncated");
    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_start")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_end")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("total_step_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("total_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert!(
        goal_run.events.is_empty(),
        "huge event should be dropped to fit the IPC cap"
    );
    let mut frame = BytesMut::new();
    amux_protocol::DaemonCodec::default()
        .encode(
            amux_protocol::DaemonMessage::AgentGoalRunDetail { goal_run_json },
            &mut frame,
        )
        .expect("serialize goal run detail frame");

    assert!(
        frame.len().saturating_sub(4) <= amux_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "goal run detail should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn get_goal_run_capped_for_ipc_preserves_owner_profiles_when_step_slice_drops_prefix() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-owner-metadata-step-slice";
    let huge_instructions = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run(goal_run_id, "task-owner", "approval-owner");
    goal_run.launch_assignment_snapshot = vec![
        sample_assignment("planner", "openai", "gpt-5", Some("high")),
        sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
    ];
    goal_run.runtime_assignment_list = vec![sample_assignment(
        "executor",
        "anthropic",
        "claude-sonnet-4",
        None,
    )];
    goal_run.root_thread_id = Some("thread-root".to_string());
    goal_run.active_thread_id = Some("thread-current".to_string());
    goal_run.execution_thread_ids = vec![
        "thread-root".to_string(),
        "thread-current".to_string(),
        "thread-followup".to_string(),
    ];
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.current_step_index = 1;
    goal_run.current_step_title = Some("step-current".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-current".to_string());
    goal_run.steps = vec![
        GoalRunStep {
            id: "step-prefix".to_string(),
            position: 0,
            title: "step-prefix".to_string(),
            instructions: huge_instructions,
            kind: GoalRunStepKind::Command,
            success_criteria: "prefix can be dropped".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Completed,
            task_id: Some("task-prefix".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: Some(now_millis()),
        },
        GoalRunStep {
            id: "step-current".to_string(),
            position: 1,
            title: "step-current".to_string(),
            instructions: "keep this step".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "current step remains meaningful".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-current".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        },
    ];
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_capped_for_ipc(goal_run_id)
        .await
        .expect("goal should exist");
    assert!(truncated, "oversized goal detail should be truncated");
    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(2),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_start")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_end")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("total_step_count")
            .and_then(serde_json::Value::as_u64),
        Some(2),
    );
    assert_eq!(
        goal_run_object
            .get("total_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("current_step_index")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "current step index should be rebased after slicing",
    );
    assert_eq!(
        goal_run_object.get("current_step_kind"),
        Some(&serde_json::json!("command")),
        "current step kind should follow the rebased current step",
    );
    assert_eq!(
        goal_run_object.get("active_task_id"),
        Some(&serde_json::json!("task-current")),
        "active task id should follow the rebased current step",
    );
    assert_eq!(
        goal_run_object.get("root_thread_id"),
        Some(&serde_json::json!("thread-root")),
        "root thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run_object.get("active_thread_id"),
        Some(&serde_json::json!("thread-current")),
        "active thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run_object
            .get("execution_thread_ids")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(3),
        "execution thread ids should survive IPC capping",
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert_eq!(
        goal_run.planner_owner_profile,
        Some(sample_owner_profile(
            "planner",
            "openai",
            "gpt-5",
            Some("high"),
        )),
        "planner owner profile should survive IPC capping",
    );
    assert_eq!(
        goal_run.current_step_owner_profile,
        Some(sample_owner_profile(
            "current-step",
            "anthropic",
            "claude-sonnet-4",
            None,
        )),
        "current-step owner profile should survive IPC capping when the step still exists",
    );
    assert_eq!(
        goal_run.steps.len(),
        1,
        "step slicing should drop the oversized prefix step",
    );
    assert_eq!(goal_run.current_step_index, 0);
    assert_eq!(goal_run.current_step_kind, Some(GoalRunStepKind::Command));
    assert_eq!(goal_run.active_task_id.as_deref(), Some("task-current"));
    assert_eq!(goal_run.current_step_title.as_deref(), Some("step-current"));
    assert_eq!(
        goal_run.launch_assignment_snapshot,
        vec![
            sample_assignment("planner", "openai", "gpt-5", Some("high")),
            sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
        ],
        "launch assignment snapshot should survive IPC capping",
    );
    assert_eq!(
        goal_run.runtime_assignment_list,
        vec![sample_assignment(
            "executor",
            "anthropic",
            "claude-sonnet-4",
            None,
        )],
        "runtime assignment list should survive IPC capping",
    );
    assert_eq!(
        goal_run.root_thread_id.as_deref(),
        Some("thread-root"),
        "root thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run.active_thread_id.as_deref(),
        Some("thread-current"),
        "active thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run.execution_thread_ids,
        vec![
            "thread-root".to_string(),
            "thread-current".to_string(),
            "thread-followup".to_string(),
        ],
        "execution thread ids should survive IPC capping",
    );
}

#[tokio::test]
async fn get_goal_run_capped_for_ipc_preserves_planner_owner_profile_in_stripped_summary_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-owner-metadata-summary";
    let huge_instructions = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run(goal_run_id, "task-owner", "approval-owner");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.steps = vec![GoalRunStep {
        id: "step-summary".to_string(),
        position: 0,
        title: "step-summary".to_string(),
        instructions: huge_instructions,
        kind: GoalRunStepKind::Command,
        success_criteria: "summary path should strip steps".to_string(),
        session_id: None,
        status: GoalRunStepStatus::InProgress,
        task_id: Some("task-summary".to_string()),
        summary: None,
        error: None,
        started_at: Some(now_millis()),
        completed_at: None,
    }];
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-summary".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-summary".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_capped_for_ipc(goal_run_id)
        .await
        .expect("goal should exist");
    assert!(truncated, "oversized goal detail should be truncated");
    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_start")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_end")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("total_step_count")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("total_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert_eq!(
        goal_run.planner_owner_profile,
        Some(sample_owner_profile(
            "planner",
            "openai",
            "gpt-5",
            Some("high"),
        )),
        "planner owner profile should survive summary stripping",
    );
    assert!(
        goal_run.current_step_owner_profile.is_none(),
        "current-step owner profile should be cleared when the current step is stripped",
    );
    assert!(
        goal_run_object.get("current_step_owner_profile").is_none(),
        "the raw wire payload should omit current-step owner metadata when the current step is stripped",
    );
    assert!(
        goal_run.steps.is_empty(),
        "summary stripping should remove oversized step payloads",
    );
}

#[tokio::test]
async fn get_goal_run_page_capped_for_ipc_clears_current_step_owner_profile_for_empty_step_window()
{
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-empty-step-window";

    let mut goal_run =
        sample_supervised_goal_run(goal_run_id, "task-empty-window", "approval-empty-window");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_page_capped_for_ipc(goal_run_id, Some(1), Some(0), None, None)
        .await
        .expect("goal should exist");
    assert!(
        !truncated,
        "an explicit empty step window should still serialize"
    );

    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object.get("current_step_owner_profile"),
        None,
        "current-step owner profile should be cleared when the step window is empty",
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert!(goal_run.steps.is_empty());
    assert!(goal_run.current_step_owner_profile.is_none());
    assert_eq!(goal_run.current_step_index, 0);
    assert!(goal_run.current_step_title.is_none());
    assert!(goal_run.current_step_kind.is_none());
    assert!(goal_run.active_task_id.is_none());
}

#[tokio::test]
async fn get_goal_run_page_capped_for_ipc_clears_current_step_owner_profile_when_paged_window_excludes_current_step(
) {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-paged-window-excludes-current-step";

    let mut goal_run =
        sample_supervised_goal_run(goal_run_id, "task-paged-window", "approval-paged-window");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.steps = vec![
        GoalRunStep {
            id: "step-current".to_string(),
            position: 0,
            title: "step-current".to_string(),
            instructions: "real current step".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "current step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-current".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        },
        GoalRunStep {
            id: "step-paged".to_string(),
            position: 1,
            title: "step-paged".to_string(),
            instructions: "windowed step".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "paged step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: Some("task-paged".to_string()),
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        },
    ];
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-current".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-current".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_page_capped_for_ipc(goal_run_id, Some(1), Some(1), None, None)
        .await
        .expect("goal should exist");
    assert!(!truncated, "paged detail should fit without truncation");

    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(2),
    );
    assert_eq!(
        goal_run_object.get("current_step_title"),
        Some(&serde_json::json!("step-paged")),
    );
    assert_eq!(
        goal_run_object.get("current_step_kind"),
        Some(&serde_json::json!("research")),
    );
    assert_eq!(
        goal_run_object.get("active_task_id"),
        Some(&serde_json::json!("task-paged")),
    );
    assert!(
        goal_run_object.get("current_step_owner_profile").is_none(),
        "current-step owner metadata should be omitted when the retained window excludes the real current step",
    );

    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert_eq!(goal_run.current_step_index, 0);
    assert_eq!(goal_run.current_step_title.as_deref(), Some("step-paged"));
    assert_eq!(goal_run.current_step_kind, Some(GoalRunStepKind::Research));
    assert_eq!(goal_run.active_task_id.as_deref(), Some("task-paged"));
    assert!(goal_run.current_step_owner_profile.is_none());
    assert_eq!(
        goal_run.planner_owner_profile,
        Some(sample_owner_profile(
            "planner",
            "openai",
            "gpt-5",
            Some("high"),
        )),
    );
}

#[tokio::test]
async fn goal_run_with_step_slice_clears_current_step_owner_profile_when_slice_excludes_current_step(
) {
    let mut goal_run =
        sample_supervised_goal_run("goal-step-slice", "task-step-slice", "approval-step-slice");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.steps = vec![
        GoalRunStep {
            id: "step-current".to_string(),
            position: 0,
            title: "step-current".to_string(),
            instructions: "real current step".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "current step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-current".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        },
        GoalRunStep {
            id: "step-sliced".to_string(),
            position: 1,
            title: "step-sliced".to_string(),
            instructions: "retained step".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "retained step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: Some("task-sliced".to_string()),
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        },
    ];
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-current".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-current".to_string());

    let sliced = goal_run_with_step_slice(&goal_run, 1);

    assert_eq!(sliced.steps.len(), 1);
    assert_eq!(sliced.current_step_index, 0);
    assert_eq!(sliced.current_step_title.as_deref(), Some("step-sliced"));
    assert_eq!(sliced.current_step_kind, Some(GoalRunStepKind::Research));
    assert_eq!(sliced.active_task_id.as_deref(), Some("task-sliced"));
    assert!(sliced.current_step_owner_profile.is_none());
}

#[tokio::test]
async fn list_goal_runs_payload_stays_below_ipc_frame_cap() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let huge_details = "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run("goal-huge-list", "task-huge", "approval-huge");
    goal_run.events.push(GoalRunEvent {
        id: "event-huge-list".to_string(),
        timestamp: now_millis(),
        phase: "running".to_string(),
        message: "huge event".to_string(),
        details: Some(huge_details),
        step_index: Some(0),
        todo_snapshot: Vec::new(),
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let goal_runs = engine.list_goal_runs().await;
    let goal_runs_json = serde_json::to_string(&goal_runs).expect("serialize goal run list json");
    let mut frame = BytesMut::new();
    amux_protocol::DaemonCodec::default()
        .encode(
            amux_protocol::DaemonMessage::AgentGoalRunList { goal_runs_json },
            &mut frame,
        )
        .expect("serialize goal run list frame");

    assert!(
        frame.len().saturating_sub(4) <= amux_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "goal run list should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn list_goal_runs_pagination_obeys_newest_first_limit_and_offset() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goals = engine.goal_runs.lock().await;
    for (id, updated_at) in [("goal-one", 30), ("goal-two", 20), ("goal-three", 10)] {
        let mut goal =
            sample_supervised_goal_run(id, &format!("task-{id}"), &format!("approval-{id}"));
        goal.updated_at = updated_at;
        goals.push_back(goal);
    }
    drop(goals);

    let (first_page, _) = engine
        .list_goal_runs_paginated_capped_for_ipc(Some(2), Some(0))
        .await;
    let (second_page, _) = engine
        .list_goal_runs_paginated_capped_for_ipc(Some(2), Some(2))
        .await;

    assert_eq!(
        first_page
            .iter()
            .map(|goal| goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-one", "goal-two"]
    );
    assert_eq!(
        second_page
            .iter()
            .map(|goal| goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-three"]
    );
}

#[tokio::test]
async fn list_tasks_capped_for_ipc_truncates_oversized_task_logs() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-small".to_string(),
        title: "small task".to_string(),
        description: "small".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 1,
        started_at: None,
        completed_at: Some(2),
        error: None,
        result: Some("ok".to_string()),
        thread_id: None,
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 1,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: vec![AgentTaskLogEntry {
            id: "task-small-log".to_string(),
            timestamp: 1,
            level: TaskLogLevel::Info,
            phase: "done".to_string(),
            message: "small log".to_string(),
            details: None,
            attempt: 0,
        }],
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-huge".to_string(),
        title: "huge task".to_string(),
        description: "huge".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 3,
        started_at: None,
        completed_at: Some(4),
        error: None,
        result: Some("ok".to_string()),
        thread_id: None,
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 1,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: vec![AgentTaskLogEntry {
            id: "task-huge-log".to_string(),
            timestamp: 3,
            level: TaskLogLevel::Info,
            phase: "done".to_string(),
            message: "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
            details: None,
            attempt: 0,
        }],
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });

    let (tasks, truncated) = engine.list_tasks_capped_for_ipc().await;
    assert!(truncated);
    assert!(tasks.iter().any(|task| task.id == "task-small"));
    let huge = tasks
        .iter()
        .find(|task| task.id == "task-huge")
        .expect("huge task should remain present after IPC capping");
    assert!(
        huge.logs.is_empty(),
        "oversized task logs should be dropped to fit IPC"
    );

    let tasks_json = serde_json::to_string(&tasks).expect("serialize capped task list json");
    let mut frame = BytesMut::new();
    amux_protocol::DaemonCodec::default()
        .encode(
            amux_protocol::DaemonMessage::AgentTaskList { tasks_json },
            &mut frame,
        )
        .expect("serialize task list frame");

    assert!(
        frame.len().saturating_sub(4) <= amux_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "task list should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn list_todos_capped_for_ipc_truncates_oversized_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.thread_todos.write().await.insert(
        "thread-small".to_string(),
        vec![TodoItem {
            id: "todo-small".to_string(),
            content: "small".to_string(),
            status: TodoStatus::Pending,
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 2,
        }],
    );
    engine.thread_todos.write().await.insert(
        "thread-huge".to_string(),
        vec![TodoItem {
            id: "todo-huge".to_string(),
            content: "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
            status: TodoStatus::Pending,
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 1,
        }],
    );

    let (todos_by_thread, truncated) = engine.list_todos_capped_for_ipc().await;
    assert!(truncated);
    assert!(todos_by_thread.contains_key("thread-small"));
    assert!(
        todos_by_thread
            .get("thread-huge")
            .map_or(true, |todos| todos.is_empty()),
        "oversized todo bucket should be dropped or emptied to fit IPC"
    );

    let todos_json = serde_json::to_string(&todos_by_thread).expect("serialize todo list");
    let mut frame = BytesMut::new();
    amux_protocol::DaemonCodec::default()
        .encode(
            amux_protocol::DaemonMessage::AgentTodoList { todos_json },
            &mut frame,
        )
        .expect("serialize todo list frame");

    assert!(
        frame.len().saturating_sub(4) <= amux_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "todo list should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn start_goal_run_records_goal_start_episode_with_archived_fields() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity".to_string()),
            Some("thread-epis-1".to_string()),
            Some("session-epis-1".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&goal.id)
        .await
        .expect("episodes should load");
    assert_eq!(
        episodes.len(),
        1,
        "goal start should immediately record one episode"
    );

    let episode_json =
        serde_json::to_value(&episodes[0]).expect("episode should serialize to json");
    assert_eq!(episode_json["episode_type"], "goal_start");
    assert_eq!(episode_json["goal_text"], "repair archived parity gaps");
    assert_eq!(episode_json["goal_type"], "goal_run");
    assert_eq!(
        episode_json["summary"],
        "Repair parity: repair archived parity gaps"
    );
    assert!(
        episode_json.get("confidence_before").is_some(),
        "goal-start episodes should carry explicit confidence_before field"
    );
    assert!(
        episode_json.get("confidence_after").is_some(),
        "goal-start episodes should carry explicit confidence_after field"
    );
}

#[tokio::test]
async fn goal_projection_delete_goal_run_removes_projection_directory() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal = engine
        .start_goal_run(
            "build titan shell".to_string(),
            Some("Build Titan".to_string()),
            Some("thread-goal-delete".to_string()),
            Some("session-goal-delete".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    let projection_dir = root.path().join(".tamux/goals").join(&goal.id);
    assert!(projection_dir.exists());

    assert!(engine.delete_goal_run(&goal.id).await);
    assert!(!projection_dir.exists());
}

#[tokio::test]
async fn repeated_goal_start_creates_retry_link_to_previous_related_episode() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let first = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity".to_string()),
            Some("thread-epis-2".to_string()),
            Some("session-epis-2".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;
    engine
        .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
        .await
        .expect("first goal failure episode should record");

    let second = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity again".to_string()),
            Some("thread-epis-3".to_string()),
            Some("session-epis-3".to_string()),
            None,
            Some("req-2".to_string()),
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&second.id)
        .await
        .expect("episodes should load");
    let start_episode = episodes
        .iter()
        .find(|episode| episode.episode_type == EpisodeType::GoalStart)
        .expect("second goal should have a goal_start episode");

    let links = engine
        .get_episode_links(&start_episode.id)
        .await
        .expect("links should load");
    assert!(
        links.iter().any(|link| link.link_type == LinkType::RetryOf),
        "repeated goal should link to the prior related episode as retry_of"
    );
}

#[tokio::test]
async fn repeated_goal_start_does_not_link_across_persona_scopes() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    run_with_agent_scope(DOMOWOJ_AGENT_ID.to_string(), async {
        let first = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Persona A parity".to_string()),
                Some("thread-epis-scope-1".to_string()),
                Some("session-epis-scope-1".to_string()),
                None,
                None,
                None,
                None,
            )
            .await;
        engine
            .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
            .await
            .expect("persona A goal failure episode should record");
    })
    .await;

    run_with_agent_scope(ROD_AGENT_ID.to_string(), async {
        let second = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Persona B parity".to_string()),
                Some("thread-epis-scope-2".to_string()),
                Some("session-epis-scope-2".to_string()),
                None,
                Some("req-persona-b".to_string()),
                None,
                None,
            )
            .await;

        let episodes = engine
            .list_episodes_for_goal_run(&second.id)
            .await
            .expect("persona B episodes should load");
        let start_episode = episodes
            .iter()
            .find(|episode| episode.episode_type == EpisodeType::GoalStart)
            .expect("persona B goal should have a goal_start episode");

        let links = engine
            .get_episode_links(&start_episode.id)
            .await
            .expect("persona B links should load");
        assert!(
            links.is_empty(),
            "persona-scoped goal-start episodes must not auto-link to another persona's history"
        );
    })
    .await;
}

#[tokio::test]
async fn suppressed_session_id_skips_goal_start_episode_recording() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.episodic.suppressed_session_ids = vec!["session-suppressed".to_string()];
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal = engine
        .start_goal_run(
            "do not persist this goal".to_string(),
            Some("Suppressed goal".to_string()),
            Some("thread-suppressed".to_string()),
            Some("session-suppressed".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&goal.id)
        .await
        .expect("episodes should load");
    assert!(
        episodes.is_empty(),
        "suppressed session ids should prevent per-session episodic recording"
    );
}

#[tokio::test]
async fn strained_satisfaction_clamps_new_goal_run_max_replans() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.score = 0.21;
        model.operator_satisfaction.label = "strained".to_string();
    }

    let goal = engine
        .start_goal_run(
            "reduce background churn".to_string(),
            Some("Satisfaction clamp".to_string()),
            Some("thread-satisfaction-goal".to_string()),
            Some("session-satisfaction-goal".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    assert_eq!(
        goal.max_replans, 1,
        "strained satisfaction should clamp new goal runs to one replan"
    );
}

#[tokio::test]
async fn strained_satisfaction_clamps_goal_task_retries_but_not_regular_tasks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.max_retries = 4;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.score = 0.23;
        model.operator_satisfaction.label = "strained".to_string();
    }

    let goal_task = engine
        .enqueue_task(
            "goal step".to_string(),
            "execute goal-linked work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            Some("goal-satisfaction-retries".to_string()),
            None,
            None,
            None,
        )
        .await;

    let regular_task = engine
        .enqueue_task(
            "regular task".to_string(),
            "execute non-goal work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            None,
            None,
        )
        .await;

    assert_eq!(
        goal_task.max_retries, 1,
        "strained satisfaction should clamp goal-linked task retries to one"
    );
    assert_eq!(
        regular_task.max_retries, 4,
        "non-goal tasks should keep the configured retry budget"
    );
}

#[tokio::test]
async fn delete_goal_run_removes_goal_and_related_tasks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goal_run = sample_supervised_goal_run("goal-delete", "task-delete", "approval-delete");
    goal_run.child_task_ids = vec!["task-delete".to_string()];
    engine.goal_runs.lock().await.push_back(goal_run.clone());
    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-delete".to_string(),
        goal_run_id: Some("goal-delete".to_string()),
        title: "Child task".to_string(),
        description: "goal-linked task".to_string(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_title: Some("supervised goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });
    engine
        .history
        .upsert_goal_run(&goal_run)
        .await
        .expect("persist goal run");
    engine
        .history
        .upsert_agent_task(&AgentTask {
            id: "task-delete".to_string(),
            goal_run_id: Some("goal-delete".to_string()),
            title: "Child task".to_string(),
            description: "goal-linked task".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now_millis(),
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_title: Some("supervised goal".to_string()),
            goal_step_id: Some("step-1".to_string()),
            goal_step_title: Some("step-1".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 3,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        })
        .await
        .expect("persist child task");

    let deleted = engine.delete_goal_run("goal-delete").await;

    assert!(deleted);
    assert!(engine
        .goal_runs
        .lock()
        .await
        .iter()
        .all(|goal_run| goal_run.id != "goal-delete"));
    assert!(engine
        .tasks
        .lock()
        .await
        .iter()
        .all(|task| task.goal_run_id.as_deref() != Some("goal-delete")));
    assert!(engine
        .history
        .get_goal_run("goal-delete")
        .await
        .expect("read goal run")
        .is_none());
}
