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

fn make_default_config() -> AgentConfig {
    AgentConfig::default()
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
    let selected =
        select_ready_task_indices(&tasks, &[], &goal_run_statuses, &make_default_config());

    assert_eq!(
        selected,
        vec![(1, "daemon-main".to_string())],
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

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new(), &make_default_config());

    assert_eq!(
        selected,
        vec![(1, "daemon-main".to_string())],
        "goal-linked queued task should not dispatch when goal status is unavailable"
    );
}

#[test]
fn select_ready_task_indices_allows_four_parallel_child_daemon_tasks() {
    let mut tasks = VecDeque::new();
    for id in 0..4 {
        let mut task = make_task(&format!("child-{id}"), TaskStatus::Queued, None);
        task.source = "subagent".to_string();
        task.parent_task_id = Some("parent-task".to_string());
        tasks.push_back(task);
    }

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new(), &make_default_config());

    assert_eq!(selected.len(), 4, "up to four child daemon tasks should dispatch in parallel");

    let lanes = selected.into_iter().map(|(_, lane)| lane).collect::<Vec<_>>();
    assert_eq!(
        lanes,
        vec![
            "daemon-subagent:child-0".to_string(),
            "daemon-subagent:child-1".to_string(),
            "daemon-subagent:child-2".to_string(),
            "daemon-subagent:child-3".to_string(),
        ]
    );
}

#[test]
fn select_ready_task_indices_assigns_second_weles_lane_when_first_is_busy() {
    let mut tasks = VecDeque::new();
    let mut active = make_task("weles-active", TaskStatus::InProgress, None);
    active.source = "subagent".to_string();
    active.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    active.lane_id = Some("weles:0".to_string());
    tasks.push_back(active);

    let mut queued = make_task("weles-queued", TaskStatus::Queued, None);
    queued.source = "subagent".to_string();
    queued.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    tasks.push_back(queued);

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new(), &make_default_config());

    assert_eq!(selected, vec![(1, "weles:1".to_string())]);
}

#[test]
fn select_ready_task_indices_blocks_weles_when_lane_pool_is_exhausted() {
    let mut tasks = VecDeque::new();
    for slot in 0..2 {
        let mut active = make_task(
            &format!("weles-active-{slot}"),
            TaskStatus::InProgress,
            None,
        );
        active.source = "subagent".to_string();
        active.sub_agent_def_id =
            Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
        active.lane_id = Some(format!("weles:{slot}"));
        tasks.push_back(active);
    }

    let mut queued = make_task("weles-queued", TaskStatus::Queued, None);
    queued.source = "subagent".to_string();
    queued.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    tasks.push_back(queued.clone());

    let mut changed = tasks.clone();
    let updates = refresh_task_queue_state(&mut changed, 1, &[], &make_default_config());
    let selected =
        select_ready_task_indices(&changed, &[], &HashMap::new(), &make_default_config());

    assert!(
        selected.is_empty(),
        "queued WELES task should not dispatch once both lanes are busy"
    );
    let blocked = updates
        .into_iter()
        .find(|task| task.id == queued.id)
        .expect("expected blocked task update");
    assert_eq!(
        blocked.blocked_reason.as_deref(),
        Some("waiting for lane availability: weles")
    );
}
