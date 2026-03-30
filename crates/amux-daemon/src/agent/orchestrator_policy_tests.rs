use std::collections::HashMap;

use super::*;
use crate::agent::{AgentConfig, AgentEngine};
use crate::agent::types::{
    AgentEvent, AgentMessage, AgentTask, AgentTaskLogEntry, GoalRun, GoalRunStatus, GoalRunStep,
    GoalRunStepKind, GoalRunStepStatus, MessageRole, TaskLogLevel, TaskPriority, TaskStatus,
};
use crate::session_manager::SessionManager;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn trigger_input(thread_id: &str) -> PolicyTriggerInput {
    PolicyTriggerInput {
        thread_id: thread_id.to_string(),
        goal_run_id: None,
        repeated_approach: false,
        awareness_stuck: false,
        should_pivot: false,
        should_escalate: false,
    }
}

fn evaluate_policy_context(input: &PolicyTriggerInput) -> PolicyTriggerContext {
    match evaluate_triggers(input) {
        TriggerOutcome::EvaluatePolicy(context) => context,
        TriggerOutcome::NoIntervention => panic!("expected policy evaluation"),
    }
}

fn decision(action: PolicyAction) -> PolicyDecision {
    PolicyDecision {
        action,
        reason: String::new(),
        strategy_hint: None,
        retry_guard: None,
    }
}

fn reasoned_decision(action: PolicyAction, reason: &str) -> PolicyDecision {
    let mut decision = decision(action);
    decision.reason = reason.to_string();
    decision
}

fn scope(thread_id: &str, goal_run_id: Option<&str>) -> PolicyDecisionScope {
    PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: goal_run_id.map(str::to_string),
    }
}

fn policy_eval_context() -> PolicyEvaluationContext {
    PolicyEvaluationContext {
        trigger: PolicyTriggerContext {
            thread_id: "thread-9".to_string(),
            goal_run_id: Some("goal-9".to_string()),
            repeated_approach: true,
            awareness_stuck: true,
            self_assessment: PolicySelfAssessmentSummary {
                should_pivot: true,
                should_escalate: false,
            },
        },
        current_retry_guard: Some("approach-hash-1".to_string()),
        recent_tool_outcomes: vec![
            PolicyToolOutcomeSummary {
                tool_name: "read_file".to_string(),
                outcome: "success".to_string(),
                summary: "Read the config but found no obvious mismatch.".to_string(),
            },
            PolicyToolOutcomeSummary {
                tool_name: "bash".to_string(),
                outcome: "failure".to_string(),
                summary: "Retrying the same test command still exits with code 1.".to_string(),
            },
        ],
        awareness_summary: Some(
            "Short-term tool success rate dropped and repeated failures cluster on the same path."
                .to_string(),
        ),
        counter_who_context: Some(
            "Counter-who detected the same failing bash approach three times.".to_string(),
        ),
        self_assessment_summary: Some(
            "Negative momentum suggests the current strategy is no longer productive.".to_string(),
        ),
        thread_context: Some(
            "Operator asked for a narrow fix without broad refactoring.".to_string(),
        ),
        recent_decision_summary: Some(
            "Recent policy decision: pivot because the previous retry loop was stuck.".to_string(),
        ),
    }
}

async fn test_engine() -> std::sync::Arc<AgentEngine> {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await
}

async fn policy_runtime_engine(
    response_json: &str,
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
) -> std::sync::Arc<AgentEngine> {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let server_url = spawn_policy_recording_server(recorded_bodies, response_json.to_string()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = server_url;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    AgentEngine::new_test(manager, config, root.path()).await
}

async fn spawn_policy_recording_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
    response_json: String,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind policy server");
    let addr = listener.local_addr().expect("policy server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            let response_json = response_json.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let read = socket.read(&mut buffer).await.expect("read policy request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                recorded_bodies
                    .lock()
                    .expect("lock request log")
                    .push_back(body);

                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\ndata: {{\"choices\":[{{\"delta\":{{\"content\":{response_json:?}}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\ndata: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write policy response");
            });
        }
    });

    format!("http://{addr}/v1")
}

fn goal_run_fixture(thread_id: &str) -> GoalRun {
    GoalRun {
        id: "goal-1".to_string(),
        title: "Test goal".to_string(),
        goal: "Recover from repeated failure".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: 1,
        updated_at: 1,
        started_at: Some(1),
        completed_at: None,
        thread_id: Some(thread_id.to_string()),
        session_id: Some("session-1".to_string()),
        current_step_index: 0,
        current_step_title: Some("Investigate failure".to_string()),
        current_step_kind: Some(GoalRunStepKind::Research),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("Initial plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: vec!["task-1".to_string()],
        child_task_count: 1,
        approval_count: 0,
        awaiting_approval_id: None,
        active_task_id: Some("task-1".to_string()),
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Investigate failure".to_string(),
            instructions: "Inspect the failing path".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "Know why it failed".to_string(),
            session_id: Some("session-1".to_string()),
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-1".to_string()),
            summary: None,
            error: None,
            started_at: Some(1),
            completed_at: None,
        }],
        events: Vec::new(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: Default::default(),
        authorship_tag: None,
    }
}

fn task_fixture(thread_id: &str) -> AgentTask {
    AgentTask {
        id: "task-1".to_string(),
        title: "Investigate failure".to_string(),
        description: "Inspect the failing path".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 35,
        created_at: 1,
        started_at: Some(1),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: Some("session-1".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        goal_run_title: Some("Test goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("Investigate failure".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 1,
        max_retries: 2,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        lane_id: None,
        last_error: Some("same bash command failed again".to_string()),
        logs: vec![AgentTaskLogEntry {
            id: "log-1".to_string(),
            timestamp: 1,
            level: TaskLogLevel::Warn,
            phase: "execution".to_string(),
            message: "attempt failed".to_string(),
            details: Some("same command failed".to_string()),
            attempt: 1,
        }],
        tool_whitelist: None,
        tool_blacklist: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: Some("Know why it failed".to_string()),
        max_duration_secs: None,
        supervisor_config: None,
        sub_agent_def_id: None,
    }
}

async fn seed_runtime(engine: &std::sync::Arc<AgentEngine>, thread_id: &str) {
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                title: "Policy thread".to_string(),
                messages: vec![AgentMessage::user("Investigate the failure", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.push_back(goal_run_fixture(thread_id));
    }
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task_fixture(thread_id));
    }
}

#[test]
fn trigger_no_intervention_when_all_inputs_are_nominal() {
    let mut input = trigger_input("thread-1");
    input.goal_run_id = Some("goal-1".to_string());

    assert_eq!(evaluate_triggers(&input), TriggerOutcome::NoIntervention);
}

#[test]
fn trigger_intervention_required_for_repeated_approach_signal() {
    let mut input = trigger_input("thread-1");
    input.goal_run_id = Some("goal-1".to_string());
    input.repeated_approach = true;

    let context = evaluate_policy_context(&input);

    assert_eq!(context.thread_id, "thread-1");
    assert_eq!(context.goal_run_id.as_deref(), Some("goal-1"));
    assert!(context.repeated_approach);
    assert!(!context.awareness_stuck);
    assert!(!context.self_assessment.should_pivot);
    assert!(!context.self_assessment.should_escalate);
}

#[test]
fn trigger_intervention_required_for_awareness_stuckness() {
    let mut input = trigger_input("thread-2");
    input.awareness_stuck = true;

    let context = evaluate_policy_context(&input);

    assert_eq!(context.thread_id, "thread-2");
    assert!(context.awareness_stuck);
    assert!(!context.repeated_approach);
    assert!(!context.self_assessment.should_pivot);
    assert!(!context.self_assessment.should_escalate);
}

#[test]
fn trigger_intervention_required_for_self_assessment_pivot_or_escalate() {
    let mut pivot_input = trigger_input("thread-3");
    pivot_input.goal_run_id = Some("goal-3".to_string());
    pivot_input.should_pivot = true;

    let mut escalate_input = trigger_input("thread-4");
    escalate_input.goal_run_id = Some("goal-4".to_string());
    escalate_input.should_escalate = true;

    let pivot_context = evaluate_policy_context(&pivot_input);
    let escalate_context = evaluate_policy_context(&escalate_input);

    assert!(pivot_context.self_assessment.should_pivot);
    assert!(!pivot_context.self_assessment.should_escalate);
    assert_eq!(escalate_context.goal_run_id.as_deref(), Some("goal-4"));
    assert!(!escalate_context.self_assessment.should_pivot);
    assert!(escalate_context.self_assessment.should_escalate);
}

#[test]
fn trigger_aggregation_is_keyed_by_thread_id() {
    let inputs = vec![
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.repeated_approach = true;
            input
        },
        {
            let mut input = trigger_input("thread-2");
            input.goal_run_id = Some("goal-2".to_string());
            input
        },
        {
            let mut input = trigger_input("thread-3");
            input.should_escalate = true;
            input
        },
    ];

    let contexts = aggregate_trigger_contexts(&inputs);

    assert_eq!(contexts.len(), 2);
    assert_eq!(
        contexts
            .get("thread-1")
            .and_then(|context| context.goal_run_id.as_deref()),
        Some("goal-1")
    );
    assert!(contexts["thread-1"].repeated_approach);
    assert!(contexts["thread-3"].self_assessment.should_escalate);
    assert!(!contexts.contains_key("thread-2"));
}

#[test]
fn trigger_aggregation_merges_active_signals_for_same_thread() {
    let inputs = vec![
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.repeated_approach = true;
            input
        },
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.awareness_stuck = true;
            input.should_pivot = true;
            input
        },
    ];

    let contexts = aggregate_trigger_contexts(&inputs);
    let context = &contexts["thread-1"];

    assert_eq!(context.goal_run_id.as_deref(), Some("goal-1"));
    assert!(context.repeated_approach);
    assert!(context.awareness_stuck);
    assert!(context.self_assessment.should_pivot);
    assert!(!context.self_assessment.should_escalate);
}

#[test]
fn trigger_aggregation_prefers_first_non_none_goal_run_id_for_same_thread() {
    let inputs = vec![
        {
            let mut input = trigger_input("thread-1");
            input.repeated_approach = true;
            input
        },
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.awareness_stuck = true;
            input
        },
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-2".to_string());
            input.should_escalate = true;
            input
        },
    ];

    let contexts = aggregate_trigger_contexts(&inputs);

    assert_eq!(
        contexts
            .get("thread-1")
            .and_then(|context| context.goal_run_id.as_deref()),
        Some("goal-1")
    );
}

#[test]
fn trigger_assessment_adapter_captures_pivot_and_escalate_flags() {
    let assessment = Assessment {
        making_progress: false,
        approach_optimal: false,
        should_escalate: true,
        should_pivot: true,
        should_terminate: false,
        confidence: 0.2,
        reasoning: "signals indicate intervention".to_string(),
        recommendations: vec!["pivot".to_string(), "escalate".to_string()],
    };

    assert_eq!(
        PolicySelfAssessmentSummary::from(&assessment),
        PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: true,
        }
    );
}

#[test]
fn decision_validate_continue_accepts_structured_output() {
    let decision: PolicyDecision = serde_json::from_str(
        r#"{
            "action": "continue",
            "reason": "",
            "strategy_hint": null,
            "retry_guard": null
        }"#,
    )
    .unwrap();

    assert_eq!(
        validate_policy_decision(&decision),
        Ok(PolicyDecision {
            action: PolicyAction::Continue,
            reason: String::new(),
            strategy_hint: None,
            retry_guard: None,
        })
    );
}

#[test]
fn decision_missing_retry_guard_defaults_to_none_during_deserialization() {
    let decision: PolicyDecision = serde_json::from_str(
        r#"{
            "action": "halt_retries",
            "reason": "Stop retrying the same failing approach.",
            "strategy_hint": null
        }"#,
    )
    .unwrap();

    assert_eq!(
        decision,
        PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the same failing approach.".to_string(),
            strategy_hint: None,
            retry_guard: None,
        }
    );
}

#[test]
fn decision_validate_pivot_accepts_retry_guard() {
    let decision: PolicyDecision = serde_json::from_str(
        r#"{
            "action": "pivot",
            "reason": "Repeated failures indicate the current strategy is stuck.",
            "strategy_hint": "Switch to a narrower inspection-first plan.",
            "retry_guard": "approach-hash-1"
        }"#,
    )
    .unwrap();

    assert_eq!(
        validate_policy_decision(&decision),
        Ok(PolicyDecision {
            action: PolicyAction::Pivot,
            reason: "Repeated failures indicate the current strategy is stuck.".to_string(),
            strategy_hint: Some("Switch to a narrower inspection-first plan.".to_string()),
            retry_guard: Some("approach-hash-1".to_string()),
        })
    );
}

#[test]
fn decision_invalid_action_string_is_rejected() {
    let result = serde_json::from_str::<PolicyDecision>(
        r#"{
            "action": "retry_forever",
            "reason": "keep going",
            "strategy_hint": null,
            "retry_guard": null
        }"#,
    );

    assert!(result.is_err());
}

#[test]
fn decision_empty_reason_is_rejected_for_non_continue_actions() {
    let mut decision = decision(PolicyAction::Escalate);
    decision.reason = "   ".to_string();

    assert_eq!(
        validate_policy_decision(&decision),
        Err(PolicyDecisionValidationError::MissingReason {
            action: PolicyAction::Escalate,
        })
    );
}

#[test]
fn decision_continue_with_retry_guard_is_rejected() {
    let mut decision = decision(PolicyAction::Continue);
    decision.retry_guard = Some("approach-hash-1".to_string());

    assert_eq!(
        validate_policy_decision(&decision),
        Err(PolicyDecisionValidationError::RetryGuardNotAllowed {
            action: PolicyAction::Continue,
        })
    );
}

#[test]
fn decision_halt_retries_without_retry_guard_is_rejected() {
    let mut decision = decision(PolicyAction::HaltRetries);
    decision.reason = "Stop retrying the same failing approach.".to_string();

    assert_eq!(
        validate_policy_decision(&decision),
        Err(PolicyDecisionValidationError::RetryGuardRequired {
            action: PolicyAction::HaltRetries,
        })
    );
}

#[test]
fn decision_unknown_fields_are_rejected_during_deserialization() {
    let result = serde_json::from_str::<PolicyDecision>(
        r#"{
            "action": "continue",
            "reason": "",
            "strategy_hint": null,
            "retry_guard": null,
            "extra_field": true
        }"#,
    );

    assert!(result.is_err());
}

#[test]
fn decision_antithrash_reuses_semantically_identical_decision_despite_wording_drift() {
    let scope = scope("thread-1", Some("goal-1"));
    let mut recorded = decision(PolicyAction::Pivot);
    recorded.reason = "We already know this approach is looping.".to_string();
    recorded.strategy_hint = Some("Use a different tool sequence.".to_string());
    recorded.retry_guard = Some("approach-hash-1".to_string());
    let mut candidate = decision(PolicyAction::Pivot);
    candidate.reason = "The current approach is still stuck.".to_string();
    candidate.strategy_hint = Some("Try a narrower recovery path.".to_string());
    candidate.retry_guard = Some("approach-hash-1".to_string());
    let recent_decisions = HashMap::from([(
        scope.clone(),
        RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        },
    )]);

    assert!(should_reuse_recent_decision(
        &recent_decisions,
        &scope,
        &candidate,
        1_030,
        60,
    ));
}

#[test]
fn decision_antithrash_and_retry_guards_do_not_leak_across_goal_runs() {
    let goal_one_scope = scope("thread-1", Some("goal-1"));
    let goal_two_scope = scope("thread-1", Some("goal-2"));
    let mut candidate = decision(PolicyAction::HaltRetries);
    candidate.reason = "Stop retrying the same failing approach.".to_string();
    candidate.retry_guard = Some("approach-hash-1".to_string());
    let recent_decisions = HashMap::from([(
        goal_one_scope.clone(),
        RecentPolicyDecision {
            decision: candidate.clone(),
            decided_at_epoch_secs: 1_000,
        },
    )]);
    let retry_guards = RetryGuardsByScope::from([(goal_one_scope, "approach-hash-1".to_string())]);

    assert!(!should_reuse_recent_decision(
        &recent_decisions,
        &goal_two_scope,
        &candidate,
        1_030,
        60,
    ));
    assert!(!has_active_retry_guard(
        &retry_guards,
        &goal_two_scope,
        "approach-hash-1",
    ));
}

#[test]
fn decision_pivot_without_retry_guard_and_different_strategy_hint_does_not_reuse() {
    let scope = scope("thread-1", Some("goal-1"));
    let mut recorded = decision(PolicyAction::Pivot);
    recorded.reason = "Try a filesystem-first investigation.".to_string();
    recorded.strategy_hint = Some("inspect logs first".to_string());
    let mut candidate = decision(PolicyAction::Pivot);
    candidate.reason = "Switch to a config-first recovery path.".to_string();
    candidate.strategy_hint = Some("review config before logs".to_string());
    let recent_decisions = HashMap::from([(
        scope.clone(),
        RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        },
    )]);

    assert!(!should_reuse_recent_decision(
        &recent_decisions,
        &scope,
        &candidate,
        1_030,
        60,
    ));
}

#[test]
fn decision_pivot_without_retry_guard_and_normalized_strategy_hint_reuses() {
    let scope = scope("thread-1", Some("goal-1"));
    let mut recorded = decision(PolicyAction::Pivot);
    recorded.reason = "Try a filesystem-first investigation.".to_string();
    recorded.strategy_hint = Some(" Inspect Logs First ".to_string());
    let mut candidate = decision(PolicyAction::Pivot);
    candidate.reason = "The current plan is still stuck.".to_string();
    candidate.strategy_hint = Some("inspect logs first".to_string());
    let recent_decisions = HashMap::from([(
        scope.clone(),
        RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        },
    )]);

    assert!(should_reuse_recent_decision(
        &recent_decisions,
        &scope,
        &candidate,
        1_030,
        60,
    ));
}

#[test]
fn policy_memory_stores_and_retrieves_latest_decision_by_thread_id() {
    let mut recent_decisions = ShortLivedRecentPolicyDecisions::new();
    let scope = scope("thread-1", Some("goal-1"));
    let recorded = reasoned_decision(PolicyAction::Pivot, "Switch to a narrower recovery path.");

    record_policy_decision(&mut recent_decisions, &scope, recorded.clone(), 1_000);

    assert_eq!(
        latest_policy_decision(&mut recent_decisions, &scope, 1_030, 60),
        Some(RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        })
    );
}

#[test]
fn retry_guard_blocks_same_approach_hash_in_same_thread_id() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let scope = scope("thread-1", Some("goal-1"));

    record_retry_guard(&mut retry_guards, &scope, "approach-hash-1", 1_000);

    assert!(is_retry_guard_active(
        &mut retry_guards,
        &scope,
        "approach-hash-1",
        1_030,
        60,
    ));
}

#[test]
fn retry_guard_does_not_block_different_approach_hash() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let scope = scope("thread-1", Some("goal-1"));

    record_retry_guard(&mut retry_guards, &scope, "approach-hash-1", 1_000);

    assert!(!is_retry_guard_active(
        &mut retry_guards,
        &scope,
        "approach-hash-2",
        1_030,
        60,
    ));
}

#[test]
fn policy_memory_expired_entries_stop_applying() {
    let mut recent_decisions = ShortLivedRecentPolicyDecisions::new();
    let scope = scope("thread-1", Some("goal-1"));
    let recorded = reasoned_decision(PolicyAction::HaltRetries, "Stop retrying the same failure.");

    record_policy_decision(&mut recent_decisions, &scope, recorded, 1_000);

    assert_eq!(
        latest_policy_decision(&mut recent_decisions, &scope, 1_061, 60),
        None
    );
    assert!(recent_decisions.is_empty());
}

#[test]
fn retry_guard_expired_entries_stop_applying() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let scope = scope("thread-1", Some("goal-1"));

    record_retry_guard(&mut retry_guards, &scope, "approach-hash-1", 1_000);

    assert!(!is_retry_guard_active(
        &mut retry_guards,
        &scope,
        "approach-hash-1",
        1_061,
        60,
    ));
    assert!(retry_guards.is_empty());
}

#[test]
fn short_lived_policy_memory_does_not_leak_across_goal_runs_in_same_thread() {
    let mut recent_decisions = ShortLivedRecentPolicyDecisions::new();
    let goal_one_scope = scope("thread-1", Some("goal-1"));
    let goal_two_scope = scope("thread-1", Some("goal-2"));

    record_policy_decision(
        &mut recent_decisions,
        &goal_one_scope,
        reasoned_decision(PolicyAction::Pivot, "Switch approach."),
        1_000,
    );

    assert_eq!(
        latest_policy_decision(&mut recent_decisions, &goal_two_scope, 1_030, 60),
        None
    );
}

#[test]
fn short_lived_retry_guard_does_not_leak_across_goal_runs_in_same_thread() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let goal_one_scope = scope("thread-1", Some("goal-1"));
    let goal_two_scope = scope("thread-1", Some("goal-2"));

    record_retry_guard(&mut retry_guards, &goal_one_scope, "approach-hash-1", 1_000);

    assert!(!is_retry_guard_active(
        &mut retry_guards,
        &goal_two_scope,
        "approach-hash-1",
        1_030,
        60,
    ));
}

#[test]
fn policy_eval_prompt_builder_includes_recent_context_sections() {
    let prompt = build_policy_eval_prompt(&policy_eval_context());

    assert!(prompt.contains("Recent tool outcomes"));
    assert!(prompt.contains("read_file => success: Read the config but found no obvious mismatch."));
    assert!(
        prompt.contains("bash => failure: Retrying the same test command still exits with code 1.")
    );
    assert!(prompt.contains("Awareness summary"));
    assert!(prompt.contains("Counter-who context"));
    assert!(prompt.contains("Self-assessment summary"));
    assert!(prompt.contains("Thread context"));
    assert!(prompt.contains("Recent policy decision summary"));
    assert!(prompt.contains("thread-9"));
    assert!(prompt.contains("goal-9"));
    assert!(!prompt.contains("\"retry_guard\""));
    assert!(prompt.contains("Do not return `retry_guard`"));
}

#[test]
fn policy_eval_prompt_caps_rendered_tool_outcomes() {
    let mut context = policy_eval_context();
    context.recent_tool_outcomes = (0..8)
        .map(|index| PolicyToolOutcomeSummary {
            tool_name: format!("tool-{index}"),
            outcome: "failure".to_string(),
            summary: format!("summary-{index}"),
        })
        .collect();

    let prompt = build_policy_eval_prompt(&context);

    assert!(prompt.contains("tool-0 => failure: summary-0"));
    assert!(prompt.contains("tool-1 => failure: summary-1"));
    assert!(prompt.contains("tool-2 => failure: summary-2"));
    assert!(prompt.contains("tool-3 => failure: summary-3"));
    assert!(!prompt.contains("tool-4 => failure: summary-4"));
    assert!(prompt.contains("- ... 4 additional tool outcomes omitted"));
}

#[test]
fn policy_eval_prompt_normalizes_and_truncates_free_form_fields() {
    let mut context = policy_eval_context();
    context.recent_tool_outcomes = vec![PolicyToolOutcomeSummary {
        tool_name: "bash\nscript".to_string(),
        outcome: "failure\nretry".to_string(),
        summary: format!("{}\n{}", "very long summary ".repeat(20), "final line"),
    }];
    context.awareness_summary = Some(format!(
        "line one\nline two\n{}",
        "extra context ".repeat(30)
    ));
    context.counter_who_context = Some("  first line\n\nsecond line  ".to_string());
    context.self_assessment_summary = Some("alpha\nbeta\ngamma".to_string());
    context.thread_context = Some(" operator request\nwith details ".to_string());
    context.recent_decision_summary = Some(format!("{}", "decision ".repeat(40)));

    let prompt = build_policy_eval_prompt(&context);

    assert!(!prompt.contains("bash\nscript"));
    assert!(prompt.contains("bash script => failure retry:"));
    assert!(!prompt.contains("line one\nline two"));
    assert!(prompt.contains("line one line two"));
    assert!(prompt.contains("first line second line"));
    assert!(prompt.contains("alpha beta gamma"));
    assert!(prompt.contains("operator request with details"));
    assert!(prompt.contains("..."));
}

#[test]
fn policy_eval_prompt_keeps_required_sections_after_normalization() {
    let mut context = policy_eval_context();
    context.recent_tool_outcomes.clear();
    context.awareness_summary = Some("\n\n".to_string());
    context.counter_who_context = Some("counter\nwho".to_string());
    context.self_assessment_summary = Some("self\nassessment".to_string());
    context.thread_context = Some("thread\ncontext".to_string());
    context.recent_decision_summary = Some("recent\ndecision".to_string());

    let prompt = build_policy_eval_prompt(&context);

    assert!(prompt.contains("## Trigger context"));
    assert!(prompt.contains("## Recent tool outcomes\n- none"));
    assert!(prompt.contains("## Awareness summary\nnone"));
    assert!(prompt.contains("## Counter-who context\ncounter who"));
    assert!(prompt.contains("## Self-assessment summary\nself assessment"));
    assert!(prompt.contains("## Thread context\nthread context"));
    assert!(prompt.contains("## Recent policy decision summary\nrecent decision"));
}

#[test]
fn policy_eval_invalid_structured_output_falls_back_safely() {
    let invalid = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop repeating the same failing path.".to_string(),
        strategy_hint: Some("Try a different recovery path.".to_string()),
        retry_guard: None,
    };

    assert_eq!(
        normalize_policy_eval_decision(Some(invalid)),
        PolicyDecision {
            action: PolicyAction::Continue,
            reason: "Policy evaluation returned an invalid decision; continuing current execution."
                .to_string(),
            strategy_hint: None,
            retry_guard: None,
        }
    );
}

#[test]
fn policy_eval_missing_result_degrades_to_continue() {
    assert_eq!(
        normalize_policy_eval_decision(None),
        PolicyDecision {
            action: PolicyAction::Continue,
            reason: "Policy evaluation unavailable; continuing current execution.".to_string(),
            strategy_hint: None,
            retry_guard: None,
        }
    );
}

#[test]
fn policy_eval_runtime_owns_halt_retry_guard_and_ignores_hallucinated_value() {
    let evaluated = runtime_owns_policy_retry_guard(
        PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the same failing path.".to_string(),
            strategy_hint: None,
            retry_guard: Some("hallucinated-guard".to_string()),
        },
        Some("approach-hash-1"),
    );

    assert_eq!(evaluated.action, PolicyAction::HaltRetries);
    assert_eq!(evaluated.retry_guard.as_deref(), Some("approach-hash-1"));
}

#[test]
fn policy_eval_runtime_drops_retry_guard_for_non_guarded_decisions() {
    let evaluated = runtime_owns_policy_retry_guard(
        PolicyDecision {
            action: PolicyAction::Pivot,
            reason: "Try a different bounded strategy.".to_string(),
            strategy_hint: Some("Inspect state before retrying.".to_string()),
            retry_guard: Some("hallucinated-guard".to_string()),
        },
        Some("approach-hash-1"),
    );

    assert_eq!(evaluated.action, PolicyAction::Pivot);
    assert_eq!(evaluated.retry_guard, None);
    assert_eq!(
        evaluated.strategy_hint.as_deref(),
        Some("Inspect state before retrying.")
    );
}

#[test]
fn policy_eval_halt_retries_without_live_runtime_guard_degrades_to_continue() {
    let evaluated = runtime_owns_policy_retry_guard(
        PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the same failing path.".to_string(),
            strategy_hint: None,
            retry_guard: Some("hallucinated-guard".to_string()),
        },
        None,
    );

    assert_eq!(evaluated.action, PolicyAction::Continue);
    assert_eq!(evaluated.retry_guard, None);
    assert!(evaluated.reason.contains("without a live retry guard"));
}

#[tokio::test]
async fn apply_halt_retries_blocks_same_pattern_retry_in_same_thread() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-halt";
    seed_runtime(&engine, thread_id).await;
    let scope = scope(thread_id, Some("goal-1"));
    let decision = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the same failing bash path.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: false,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    engine.record_policy_decision(&scope, decision, 1_000).await;
    engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &PolicyDecision {
                action: PolicyAction::HaltRetries,
                reason: "Stop retrying the same failing bash path.".to_string(),
                strategy_hint: None,
                retry_guard: Some("approach-hash-1".to_string()),
            },
            1_000,
        )
        .await
        .expect("halt retries should apply");

    let outcome = engine
        .enforce_orchestrator_retry_guard(thread_id, Some("task-1"), &scope, "approach-hash-1", 1_010)
        .await
        .expect("retry guard should be enforced");

    assert_eq!(outcome, PolicyLoopAction::AbortRetry);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.retry_count, task.max_retries);
}

#[tokio::test]
async fn apply_fresh_halt_retries_marks_task_as_failed_immediately() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-fresh-halt";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the same failing bash path.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            1_000,
        )
        .await
        .expect("halt retries should apply");

    assert_eq!(outcome, PolicyLoopAction::AbortRetry);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.retry_count, task.max_retries);
    assert_eq!(task.blocked_reason.as_deref(), Some("policy halted repeated retry"));
    assert_eq!(task.last_error.as_deref(), Some("policy halted repeated retry"));
    assert!(task.completed_at.is_some());
    assert!(task
        .logs
        .iter()
        .any(|entry| entry.message.contains("policy halted repeated retry")));
}

#[tokio::test]
async fn apply_pivot_routes_into_existing_strategy_refresh_behavior() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-pivot";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "The repeated failures justify a different strategy.".to_string(),
        strategy_hint: Some("Inspect state before running the same command again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            1_000,
        )
        .await
        .expect("pivot should apply");

    assert_eq!(outcome, PolicyLoopAction::RestartLoop);
    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread");
    let injected = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.content.contains("Investigate failure")
                && message.content.contains("Inspect state before running the same command again")
        })
        .expect("strategy refresh prompt");
    assert!(injected.content.contains("Fallback strategy"));
}

#[tokio::test]
async fn apply_escalate_routes_into_existing_escalation_behavior() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-escalate";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Escalate,
        reason: "Repeated failures need operator guidance now.".to_string(),
        strategy_hint: None,
        retry_guard: None,
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: true,
        },
    };
    let mut events = engine.subscribe();

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            1_000,
        )
        .await
        .expect("escalation should apply");

    assert_eq!(outcome, PolicyLoopAction::InterruptForApproval);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert!(task.awaiting_approval_id.is_some());
    drop(tasks);

    let mut saw_escalation_update = false;
    let mut saw_audit_action = false;
    for _ in 0..4 {
        let event = events.recv().await.expect("event");
        match event {
            AgentEvent::EscalationUpdate { thread_id: event_thread_id, .. }
                if event_thread_id == thread_id =>
            {
                saw_escalation_update = true;
            }
            AgentEvent::AuditAction { thread_id: Some(event_thread_id), .. }
                if event_thread_id == thread_id =>
            {
                saw_audit_action = true;
            }
            _ => {}
        }
    }
    assert!(saw_escalation_update);
    assert!(saw_audit_action);
}

#[tokio::test]
async fn apply_continue_leaves_current_flow_unchanged() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-continue";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Continue,
        reason: String::new(),
        strategy_hint: None,
        retry_guard: None,
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: false,
        awareness_stuck: false,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            1_000,
        )
        .await
        .expect("continue should apply");

    assert_eq!(outcome, PolicyLoopAction::Continue);
    let threads = engine.threads.read().await;
    assert_eq!(threads.get(thread_id).expect("thread").messages.len(), 1);
}

#[tokio::test]
async fn apply_recent_policy_decision_is_persisted_and_reused_on_next_relevant_turn() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-reuse";
    seed_runtime(&engine, thread_id).await;
    let scope = scope(thread_id, Some("goal-1"));
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: false,
        },
    };
    let pivot_decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Switch away from the repeating failure.".to_string(),
        strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };

    engine.record_policy_decision(&scope, pivot_decision.clone(), 1_000).await;
    let recent = engine
        .latest_policy_decision(&scope, 1_010)
        .await
        .expect("recent policy decision");

    let selection = select_orchestrator_policy_decision(
        Some(&recent),
        &trigger,
        PolicyDecision {
            action: PolicyAction::Pivot,
            reason: "Fresh wording but same bounded pivot.".to_string(),
            strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
            retry_guard: Some("approach-hash-1".to_string()),
        },
    );

    assert_eq!(selection.source, PolicyDecisionSource::ReusedRecent);
    assert_eq!(selection.decision, pivot_decision);
}

#[tokio::test]
async fn evaluate_policy_turn_reuses_persisted_recent_decision_for_matching_runtime_candidate() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"pivot","reason":"Current path is still stuck.","strategy_hint":"Inspect the workspace before running commands again."}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-reuse", Some("goal-1"));
    let persisted = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Switch away from the repeating failure.".to_string(),
        strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };

    engine
        .record_policy_decision(&scope, persisted.clone(), 1_000)
        .await;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::ReusedRecent);
    assert_eq!(selection.decision, persisted);
    assert!(recorded_bodies
        .lock()
        .expect("lock request log")
        .is_empty());
}

#[tokio::test]
async fn evaluate_policy_turn_does_not_reuse_recent_decision_for_different_runtime_retry_guard() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"halt_retries","reason":"Stop retrying the new failing approach.","strategy_hint":null}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-no-reuse", Some("goal-1"));

    engine
        .record_policy_decision(
            &scope,
            PolicyDecision {
                action: PolicyAction::HaltRetries,
                reason: "Stop retrying the first failing approach.".to_string(),
                strategy_hint: None,
                retry_guard: Some("approach-hash-1".to_string()),
            },
            1_000,
        )
        .await;

    let mut context = policy_eval_context();
    context.current_retry_guard = Some("approach-hash-2".to_string());

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, context, 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::HaltRetries);
    assert_eq!(selection.decision.retry_guard.as_deref(), Some("approach-hash-2"));
    let recorded = recorded_bodies.lock().expect("lock request log");
    assert!(
        recorded.iter().any(|body| {
            body.contains("structured_output")
                || body.contains("\"response_format\"")
                || body.contains("\"text\":{\"format\"")
        }),
        "expected a fresh structured policy evaluation request for the new retry guard"
    );
}

#[tokio::test]
async fn evaluate_policy_turn_records_runtime_owned_guard_for_fresh_halt_retries() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"halt_retries","reason":"Stop retrying the same failing approach.","strategy_hint":null}"#,
        recorded_bodies,
    )
    .await;
    let scope = scope("thread-runtime-owned-guard", Some("goal-1"));

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::HaltRetries);
    assert_eq!(selection.decision.retry_guard.as_deref(), Some("approach-hash-1"));

    let recent = engine
        .latest_policy_decision(&scope, 1_020)
        .await
        .expect("recent policy decision");
    assert_eq!(recent.decision.retry_guard.as_deref(), Some("approach-hash-1"));
}

#[tokio::test]
async fn evaluate_policy_turn_fresh_halt_retries_without_live_guard_degrades_to_continue() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"halt_retries","reason":"Stop retrying the same failing approach.","strategy_hint":null}"#,
        recorded_bodies,
    )
    .await;
    let scope = scope("thread-runtime-no-live-guard", Some("goal-1"));
    let mut context = policy_eval_context();
    context.current_retry_guard = None;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, context, 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::Continue);
    assert_eq!(selection.decision.retry_guard, None);
}

#[tokio::test]
async fn evaluate_policy_turn_reuses_recent_non_guarded_decision_for_matching_runtime_candidate() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"escalate","reason":"Operator guidance is still needed.","strategy_hint":null}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-reuse-non-guarded", Some("goal-1"));
    let persisted = PolicyDecision {
        action: PolicyAction::Escalate,
        reason: "Repeated failures need operator guidance now.".to_string(),
        strategy_hint: None,
        retry_guard: None,
    };

    engine
        .record_policy_decision(&scope, persisted.clone(), 1_000)
        .await;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::ReusedRecent);
    assert_eq!(selection.decision, persisted);
    let recorded = recorded_bodies.lock().expect("lock request log");
    assert!(
        recorded.iter().any(|body| {
            body.contains("structured_output")
                || body.contains("\"response_format\"")
                || body.contains("\"text\":{\"format\"")
        }),
        "expected runtime evaluation to inspect a fresh structured candidate before reusing the recent non-guarded decision"
    );
}

#[tokio::test]
async fn evaluate_policy_turn_does_not_reuse_recent_non_guarded_decision_for_materially_different_candidate() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"pivot","reason":"A different bounded strategy is more appropriate.","strategy_hint":"Inspect the workspace before running commands again."}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-no-reuse-non-guarded", Some("goal-1"));

    engine
        .record_policy_decision(
            &scope,
            PolicyDecision {
                action: PolicyAction::Escalate,
                reason: "Repeated failures need operator guidance now.".to_string(),
                strategy_hint: None,
                retry_guard: None,
            },
            1_000,
        )
        .await;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::Pivot);
    assert_eq!(
        selection.decision.strategy_hint.as_deref(),
        Some("Inspect the workspace before running commands again.")
    );
    let recorded = recorded_bodies.lock().expect("lock request log");
    assert!(
        recorded.iter().any(|body| {
            body.contains("structured_output")
                || body.contains("\"response_format\"")
                || body.contains("\"text\":{\"format\"")
        }),
        "expected a fresh structured policy evaluation request for the materially different non-guarded candidate"
    );
}

#[test]
fn apply_recent_policy_decision_is_not_reused_for_materially_different_retry_guard() {
    let trigger = PolicyTriggerContext {
        thread_id: "thread-policy-reuse-different".to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };
    let recent = RecentPolicyDecision {
        decision: PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the first failing approach.".to_string(),
            strategy_hint: None,
            retry_guard: Some("approach-hash-1".to_string()),
        },
        decided_at_epoch_secs: 1_000,
    };
    let evaluated = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the new failing approach.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-2".to_string()),
    };

    let selection = select_orchestrator_policy_decision(Some(&recent), &trigger, evaluated.clone());

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision, evaluated);
}

#[tokio::test]
async fn apply_repeated_failed_approach_changes_execution_policy_not_only_events() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-change";
    seed_runtime(&engine, thread_id).await;
    let scope = scope(thread_id, Some("goal-1"));
    let decision = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "The same failing approach should stop now.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-1".to_string()),
    };

    engine.record_policy_decision(&scope, decision, 1_000).await;
    let outcome = engine
        .enforce_orchestrator_retry_guard(thread_id, Some("task-1"), &scope, "approach-hash-1", 1_010)
        .await
        .expect("guard outcome");

    assert_eq!(outcome, PolicyLoopAction::AbortRetry);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.retry_count, task.max_retries);
    assert!(task.logs.iter().any(|entry| entry.message.contains("policy halted")));
}
