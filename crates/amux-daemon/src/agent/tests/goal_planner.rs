use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn sample_goal_run(goal_run_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "supervised goal".to_string(),
        goal: "validate supervised gating".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
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
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        active_task_id: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: "do supervised work".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: None,
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        }],
        events: Vec::new(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
    }
}

#[tokio::test]
async fn enqueue_goal_run_step_marks_supervised_task_as_awaiting_approval_before_dispatch() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_goal_run(goal_run_id));

    engine
        .enqueue_goal_run_step(goal_run_id)
        .await
        .expect("enqueue should succeed");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    let tasks = engine.tasks.lock().await;
    let step_task_id = goal.steps[0]
        .task_id
        .clone()
        .expect("step should be linked to a task");
    let step_task = tasks
        .iter()
        .find(|task| task.id == step_task_id)
        .cloned()
        .expect("step task should exist");

    assert_eq!(goal.status, GoalRunStatus::AwaitingApproval);
    assert!(
        goal.awaiting_approval_id.is_some(),
        "supervised gate should assign an approval id on goal run"
    );
    assert_eq!(step_task.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        step_task.awaiting_approval_id, goal.awaiting_approval_id,
        "task and goal should share the same gate identifier"
    );
}
