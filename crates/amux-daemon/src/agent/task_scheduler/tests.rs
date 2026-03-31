use super::*;

fn make_task(id: &str, status: TaskStatus, goal_run_id: Option<&str>) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: format!("task-{id}"),
        description: "scheduler test task".to_string(),
        status,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 1,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: goal_run_id.map(ToString::to_string),
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
        blocked_reason: None,
        awaiting_approval_id: None,
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
    }
}

#[test]
fn select_ready_task_indices_excludes_queued_tasks_for_awaiting_approval_goal_runs() {
    let mut tasks = VecDeque::new();
    tasks.push_back(make_task(
        "goal-task",
        TaskStatus::Queued,
        Some("goal-awaiting-approval"),
    ));
    tasks.push_back(make_task("independent-task", TaskStatus::Queued, None));

    let mut goal_run_statuses = HashMap::new();
    goal_run_statuses.insert(
        "goal-awaiting-approval".to_string(),
        GoalRunStatus::AwaitingApproval,
    );
    let selected = select_ready_task_indices(&tasks, &[], &goal_run_statuses);

    assert_eq!(
        selected,
        vec![1],
        "goal-linked queued task should stay blocked"
    );
}

#[test]
fn select_ready_task_indices_fail_closed_when_goal_metadata_missing_for_goal_linked_task() {
    let mut tasks = VecDeque::new();
    tasks.push_back(make_task(
        "goal-task",
        TaskStatus::Queued,
        Some("unknown-goal-run"),
    ));
    tasks.push_back(make_task("independent-task", TaskStatus::Queued, None));

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new());

    assert_eq!(
        selected,
        vec![1],
        "goal-linked queued task should not dispatch when goal status is unavailable"
    );
}
