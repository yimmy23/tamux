use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::{Arc, Mutex as StdMutex};

use crate::agent::types::{
    AgentMessage, AgentTask, AgentTaskLogEntry, ApiTransport, GoalRun, GoalRunStatus, GoalRunStep,
    GoalRunStepKind, GoalRunStepStatus, TaskLogLevel, TaskPriority, TaskStatus,
};
use crate::agent::{AgentConfig, AgentEngine};
use crate::session_manager::SessionManager;
use tempfile::{tempdir, TempDir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::super::*;

pub(super) fn trigger_input(thread_id: &str) -> PolicyTriggerInput {
    PolicyTriggerInput {
        thread_id: thread_id.to_string(),
        goal_run_id: None,
        repeated_approach: false,
        awareness_stuck: false,
        should_pivot: false,
        should_escalate: false,
    }
}

pub(super) fn evaluate_policy_context(input: &PolicyTriggerInput) -> PolicyTriggerContext {
    match evaluate_triggers(input) {
        TriggerOutcome::EvaluatePolicy(context) => context,
        TriggerOutcome::NoIntervention => panic!("expected policy evaluation"),
    }
}

pub(super) fn decision(action: PolicyAction) -> PolicyDecision {
    PolicyDecision {
        action,
        reason: String::new(),
        strategy_hint: None,
        retry_guard: None,
    }
}

pub(super) fn reasoned_decision(action: PolicyAction, reason: &str) -> PolicyDecision {
    let mut decision = decision(action);
    decision.reason = reason.to_string();
    decision
}

pub(super) fn scope(thread_id: &str, goal_run_id: Option<&str>) -> PolicyDecisionScope {
    PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: goal_run_id.map(str::to_string),
    }
}

pub(super) fn policy_eval_context() -> PolicyEvaluationContext {
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
        continuity_summary: Some(
            "Carry forward the current bash-focused debugging thread and avoid the ruled-out sync path."
                .to_string(),
        ),
        counter_who_context: Some(
            "Counter-who detected the same failing bash approach three times.".to_string(),
        ),
        negative_constraints_context: Some(
            "## Ruled-Out Approaches (Negative Knowledge)\n- Dead: retrying the old sync path keeps failing."
                .to_string(),
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

pub(super) struct TestEngine {
    engine: Arc<AgentEngine>,
    _root: TempDir,
}

impl Deref for TestEngine {
    type Target = Arc<AgentEngine>;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

pub(super) async fn test_engine() -> TestEngine {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    TestEngine {
        engine,
        _root: root,
    }
}

pub(super) async fn policy_runtime_engine(
    response_json: &str,
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
) -> TestEngine {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let server_url =
        spawn_policy_recording_server(recorded_bodies, response_json.to_string()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = server_url;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    TestEngine {
        engine,
        _root: root,
    }
}

pub(super) async fn spawn_policy_recording_server(
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

pub(super) fn goal_run_fixture(thread_id: &str) -> GoalRun {
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
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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

pub(super) fn task_fixture(thread_id: &str) -> AgentTask {
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
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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

pub(super) async fn seed_runtime(engine: &Arc<AgentEngine>, thread_id: &str) {
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
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

pub(super) fn awareness_only_trigger(thread_id: &str) -> PolicyTriggerContext {
    PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: false,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: false,
        },
    }
}
