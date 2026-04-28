use super::*;
use crate::state::spawned_tree::derive_spawned_agent_tree;

fn make_task(
    id: &str,
    title: &str,
    created_at: u64,
    thread_id: Option<&str>,
    parent_task_id: Option<&str>,
    parent_thread_id: Option<&str>,
    status: Option<TaskStatus>,
) -> AgentTask {
    AgentTask {
        id: id.into(),
        title: title.into(),
        created_at,
        thread_id: thread_id.map(str::to_string),
        parent_task_id: parent_task_id.map(str::to_string),
        parent_thread_id: parent_thread_id.map(str::to_string),
        status,
        ..Default::default()
    }
}

fn make_goal_run(id: &str, title: &str) -> GoalRun {
    GoalRun {
        id: id.into(),
        title: title.into(),
        ..Default::default()
    }
}

fn make_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> GoalRuntimeOwnerProfile {
    GoalRuntimeOwnerProfile {
        agent_label: agent_label.into(),
        provider: provider.into(),
        model: model.into(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

fn make_wire_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> crate::wire::GoalRuntimeOwnerProfile {
    crate::wire::GoalRuntimeOwnerProfile {
        agent_label: agent_label.into(),
        provider: provider.into(),
        model: model.into(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

fn make_assignment(
    role_id: &str,
    enabled: bool,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
    inherit_from_main: bool,
) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id: role_id.into(),
        enabled,
        provider: provider.into(),
        model: model.into(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main,
    }
}

fn make_wire_assignment(
    role_id: &str,
    enabled: bool,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
    inherit_from_main: bool,
) -> crate::wire::GoalAgentAssignment {
    crate::wire::GoalAgentAssignment {
        role_id: role_id.into(),
        enabled,
        provider: provider.into(),
        model: model.into(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main,
    }
}

#[test]
fn task_list_received_replaces_tasks() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![
        make_task("t1", "First", 1, None, None, None, None),
        make_task("t2", "Second", 2, None, None, None, None),
    ]));
    assert_eq!(state.tasks().len(), 2);

    // Replace with a smaller list
    state.reduce(TaskAction::TaskListReceived(vec![make_task(
        "t3", "Third", 3, None, None, None, None,
    )]));
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].id, "t3");
}

#[test]
fn task_update_upserts_by_id() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![make_task(
        "t1", "Original", 1, None, None, None, None,
    )]));

    // Update existing task
    state.reduce(TaskAction::TaskUpdate(AgentTask {
        id: "t1".into(),
        title: "Updated".into(),
        status: Some(TaskStatus::InProgress),
        ..Default::default()
    }));
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].title, "Updated");
    assert_eq!(state.tasks()[0].status, Some(TaskStatus::InProgress));

    // Insert new task
    state.reduce(TaskAction::TaskUpdate(make_task(
        "t2",
        "New",
        2,
        None,
        None,
        None,
        Some(TaskStatus::Queued),
    )));
    assert_eq!(state.tasks().len(), 2);
}

#[test]
fn goal_run_list_received_replaces_goal_runs() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunListReceived(vec![
        make_goal_run("g1", "Goal One"),
        make_goal_run("g2", "Goal Two"),
    ]));
    assert_eq!(state.goal_runs().len(), 2);

    state.reduce(TaskAction::GoalRunListReceived(vec![]));
    assert_eq!(state.goal_runs().len(), 0);
}

#[test]
fn goal_run_detail_received_upserts() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunListReceived(vec![make_goal_run(
        "g1", "Original",
    )]));

    // Update via detail
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Detailed".into(),
        ..Default::default()
    }));
    assert_eq!(state.goal_runs().len(), 1);
    assert_eq!(state.goal_runs()[0].title, "Detailed");

    // Insert new via update
    state.reduce(TaskAction::GoalRunUpdate(make_goal_run("g2", "New Goal")));
    assert_eq!(state.goal_runs().len(), 2);
}

#[test]
fn active_goal_linked_task_update_revives_paused_goal_status() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        status: Some(GoalRunStatus::Paused),
        ..Default::default()
    }));

    state.reduce(TaskAction::TaskUpdate(AgentTask {
        id: "task-1".into(),
        title: "Child task".into(),
        goal_run_id: Some("goal-1".into()),
        status: Some(TaskStatus::InProgress),
        ..Default::default()
    }));

    assert_eq!(
        state.goal_run_by_id("goal-1").and_then(|goal| goal.status),
        Some(GoalRunStatus::Running)
    );
}

#[test]
fn new_goal_run_updates_are_inserted_first() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunListReceived(vec![
        make_goal_run("g1", "Goal One"),
        make_goal_run("g2", "Goal Two"),
    ]));

    state.reduce(TaskAction::GoalRunUpdate(make_goal_run(
        "g3",
        "Newest Goal",
    )));

    assert_eq!(
        state
            .goal_runs()
            .iter()
            .map(|run| run.id.as_str())
            .collect::<Vec<_>>(),
        vec!["g3", "g1", "g2"]
    );
}

#[test]
fn goal_steps_in_display_order_sorts_by_step_order() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        steps: vec![
            GoalRunStep {
                id: "step-3".into(),
                title: "Third".into(),
                order: 2,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-1".into(),
                title: "First".into(),
                order: 0,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-2".into(),
                title: "Second".into(),
                order: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let step_ids = state
        .goal_steps_in_display_order("goal-1")
        .iter()
        .map(|step| step.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(step_ids, vec!["step-1", "step-2", "step-3"]);
}

#[test]
fn goal_step_todos_use_live_goal_step_replacement_over_stale_event_snapshot() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        thread_id: Some("thread-1".into()),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        events: vec![
            GoalRunEvent {
                id: "event-old".into(),
                timestamp: 10,
                step_index: Some(0),
                todo_snapshot: vec![
                    TodoItem {
                        id: "todo-1".into(),
                        content: "old snapshot item".into(),
                        status: Some(TodoStatus::Pending),
                        step_index: Some(0),
                        position: 0,
                        ..Default::default()
                    },
                    TodoItem {
                        id: "todo-2".into(),
                        content: "current todo two".into(),
                        status: Some(TodoStatus::Pending),
                        step_index: Some(0),
                        position: 1,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            GoalRunEvent {
                id: "event-new".into(),
                timestamp: 20,
                step_index: Some(0),
                todo_snapshot: vec![TodoItem {
                    id: "todo-2".into(),
                    content: "current todo two".into(),
                    status: Some(TodoStatus::Pending),
                    step_index: Some(0),
                    position: 1,
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }));
    state.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".into(),
        goal_run_id: Some("goal-1".into()),
        step_index: Some(0),
        items: vec![TodoItem {
            id: "todo-2".into(),
            content: "current todo two".into(),
            status: Some(TodoStatus::InProgress),
            step_index: Some(0),
            position: 1,
            ..Default::default()
        }],
    });

    let todos = state.goal_step_todos("goal-1", 0);
    let todo_ids = todos
        .iter()
        .map(|todo| todo.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(todo_ids, vec!["todo-2"]);
    assert_eq!(todos[0].content, "current todo two");
    assert_eq!(todos[0].status, Some(TodoStatus::InProgress));
}

#[test]
fn goal_step_todos_use_latest_status_when_same_content_has_new_id() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        thread_id: Some("thread-1".into()),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        events: vec![GoalRunEvent {
            id: "event-old".into(),
            timestamp: 10,
            step_index: Some(0),
            todo_snapshot: vec![TodoItem {
                id: "todo-old".into(),
                content: "Run acceptance checklist for coverage".into(),
                status: Some(TodoStatus::Pending),
                step_index: Some(0),
                position: 0,
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    }));
    state.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".into(),
        goal_run_id: Some("goal-1".into()),
        step_index: Some(0),
        items: vec![TodoItem {
            id: "todo-new".into(),
            content: "Run acceptance checklist for coverage".into(),
            status: Some(TodoStatus::InProgress),
            step_index: Some(0),
            position: 0,
            ..Default::default()
        }],
    });

    let todos = state.goal_step_todos("goal-1", 0);

    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Run acceptance checklist for coverage");
    assert_eq!(todos[0].status, Some(TodoStatus::InProgress));
    assert_eq!(todos[0].id, "todo-new");
}

#[test]
fn goal_step_todos_use_latest_event_snapshot_without_resurrecting_removed_items() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        thread_id: Some("thread-1".into()),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        events: vec![
            GoalRunEvent {
                id: "event-old".into(),
                timestamp: 10,
                step_index: Some(0),
                todo_snapshot: vec![
                    TodoItem {
                        id: "todo-removed".into(),
                        content: "removed stale item".into(),
                        status: Some(TodoStatus::Pending),
                        step_index: Some(0),
                        position: 0,
                        ..Default::default()
                    },
                    TodoItem {
                        id: "todo-current".into(),
                        content: "current item".into(),
                        status: Some(TodoStatus::Pending),
                        step_index: Some(0),
                        position: 1,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            GoalRunEvent {
                id: "event-new".into(),
                timestamp: 20,
                step_index: Some(0),
                todo_snapshot: vec![TodoItem {
                    id: "todo-current".into(),
                    content: "current item".into(),
                    status: Some(TodoStatus::InProgress),
                    step_index: Some(0),
                    position: 0,
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let todos = state.goal_step_todos("goal-1", 0);

    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].id, "todo-current");
    assert_eq!(todos[0].content, "current item");
    assert_eq!(todos[0].status, Some(TodoStatus::InProgress));
}

#[test]
fn goal_step_todos_use_goal_scoped_live_todos_when_event_snapshot_is_missing() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        thread_id: Some("thread-1".into()),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));
    state.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".into(),
        goal_run_id: Some("goal-1".into()),
        step_index: Some(0),
        items: vec![TodoItem {
            id: "todo-1".into(),
            content: "step todo".into(),
            status: Some(TodoStatus::Pending),
            step_index: Some(0),
            ..Default::default()
        }],
    });

    let todos = state.goal_step_todos("goal-1", 0);

    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].id, "todo-1");
    assert_eq!(todos[0].content, "step todo");
}

#[test]
fn goal_scoped_live_todo_thread_is_linked_to_goal_run() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));
    state.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-live".into(),
        goal_run_id: Some("goal-1".into()),
        step_index: Some(0),
        items: vec![TodoItem {
            id: "todo-live".into(),
            content: "live worker todo".into(),
            status: Some(TodoStatus::InProgress),
            step_index: Some(0),
            ..Default::default()
        }],
    });

    assert!(state.thread_belongs_to_goal_run("goal-1", "thread-live"));
}

#[test]
fn goal_step_todos_ignore_thread_scoped_child_todos_without_goal_binding() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));
    state.reduce(TaskAction::TaskListReceived(vec![AgentTask {
        id: "task-1".into(),
        title: "Child task".into(),
        thread_id: Some("thread-task".into()),
        goal_run_id: Some("goal-1".into()),
        ..Default::default()
    }]));
    state.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-task".into(),
        goal_run_id: None,
        step_index: None,
        items: vec![TodoItem {
            id: "todo-1".into(),
            content: "step todo from child thread".into(),
            status: Some(TodoStatus::InProgress),
            step_index: Some(0),
            ..Default::default()
        }],
    });

    let todos = state.goal_step_todos("goal-1", 0);

    assert!(todos.is_empty());
}

#[test]
fn goal_thread_ids_include_spawned_descendants_of_goal_threads() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        active_thread_id: Some("thread-goal-active".into()),
        ..Default::default()
    }));
    state.reduce(TaskAction::TaskListReceived(vec![AgentTask {
        id: "spawned-task".into(),
        title: "Spawned worker".into(),
        thread_id: Some("thread-spawned".into()),
        parent_thread_id: Some("thread-goal-active".into()),
        goal_run_id: None,
        ..Default::default()
    }]));

    assert_eq!(
        state.goal_thread_ids("goal-1"),
        vec![
            "thread-goal-active".to_string(),
            "thread-spawned".to_string()
        ]
    );
    assert!(state.thread_belongs_to_goal_run("goal-1", "thread-spawned"));
}

#[test]
fn goal_step_todos_use_event_step_index_when_snapshot_items_lack_step_index() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        events: vec![GoalRunEvent {
            id: "event-1".into(),
            timestamp: 10,
            step_index: Some(0),
            todo_snapshot: vec![TodoItem {
                id: "todo-1".into(),
                content: "snapshot todo".into(),
                status: Some(TodoStatus::InProgress),
                step_index: None,
                position: 0,
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    }));

    let todos = state.goal_step_todos("goal-1", 0);

    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].id, "todo-1");
    assert_eq!(todos[0].content, "snapshot todo");
    assert_eq!(todos[0].status, Some(TodoStatus::InProgress));
}

#[test]
fn goal_step_details_filter_checkpoints_and_files_by_goal_and_step() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        thread_id: Some("thread-1".into()),
        steps: vec![
            GoalRunStep {
                id: "step-1".into(),
                title: "Plan".into(),
                order: 0,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-2".into(),
                title: "Ship".into(),
                order: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    }));
    state.reduce(TaskAction::GoalRunCheckpointsReceived {
        goal_run_id: "goal-1".into(),
        checkpoints: vec![
            GoalRunCheckpointSummary {
                id: "checkpoint-1".into(),
                checkpoint_type: "plan".into(),
                step_index: Some(0),
                ..Default::default()
            },
            GoalRunCheckpointSummary {
                id: "checkpoint-2".into(),
                checkpoint_type: "ship".into(),
                step_index: Some(1),
                ..Default::default()
            },
        ],
    });
    state.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "thread-1".into(),
        entries: vec![
            WorkContextEntry {
                path: "/tmp/plan.md".into(),
                goal_run_id: Some("goal-1".into()),
                step_index: Some(0),
                ..Default::default()
            },
            WorkContextEntry {
                path: "/tmp/ship.md".into(),
                goal_run_id: Some("goal-1".into()),
                step_index: Some(1),
                ..Default::default()
            },
            WorkContextEntry {
                path: "/tmp/other-goal.md".into(),
                goal_run_id: Some("goal-2".into()),
                step_index: Some(0),
                ..Default::default()
            },
        ],
    }));

    let checkpoint_ids = state
        .goal_step_checkpoints("goal-1", 0)
        .iter()
        .map(|checkpoint| checkpoint.id.as_str())
        .collect::<Vec<_>>();
    let file_paths = state
        .goal_step_files("goal-1", "thread-1", 0)
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(checkpoint_ids, vec!["checkpoint-1"]);
    assert_eq!(file_paths, vec!["/tmp/plan.md"]);
}

#[test]
fn goal_run_detail_received_parses_mission_control_metadata() {
    let wire_run = crate::wire::GoalRun {
        id: "g1".into(),
        title: "Mission Control".into(),
        launch_assignment_snapshot: vec![
            make_wire_assignment("planner", true, "openai", "gpt-4.1", Some("high"), true),
            make_wire_assignment(
                "validator",
                false,
                "anthropic",
                "claude-sonnet-4",
                None,
                false,
            ),
        ],
        runtime_assignment_list: vec![make_wire_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("medium"),
            true,
        )],
        root_thread_id: Some("thread-root".into()),
        active_thread_id: Some("thread-active".into()),
        execution_thread_ids: vec!["thread-root".into(), "thread-active".into()],
        ..Default::default()
    };

    let converted = crate::app::conversion::convert_goal_run(wire_run);

    assert_eq!(
        converted.launch_assignment_snapshot,
        vec![
            make_assignment("planner", true, "openai", "gpt-4.1", Some("high"), true),
            make_assignment(
                "validator",
                false,
                "anthropic",
                "claude-sonnet-4",
                None,
                false
            ),
        ]
    );
    assert_eq!(
        converted.runtime_assignment_list,
        vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("medium"),
            true,
        )]
    );
    assert_eq!(converted.root_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(converted.active_thread_id.as_deref(), Some("thread-active"));
    assert_eq!(
        converted.execution_thread_ids,
        vec!["thread-root".to_string(), "thread-active".to_string()]
    );

    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(converted));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.launch_assignment_snapshot.len(), 2);
    assert_eq!(goal.runtime_assignment_list.len(), 1);
    assert_eq!(goal.root_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(goal.active_thread_id.as_deref(), Some("thread-active"));
    assert_eq!(
        goal.execution_thread_ids,
        vec!["thread-root".to_string(), "thread-active".to_string()]
    );
}

#[test]
fn goal_run_detail_received_replaces_mission_control_metadata() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        launch_assignment_snapshot: vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("high"),
            true,
        )],
        runtime_assignment_list: vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("high"),
            true,
        )],
        root_thread_id: Some("thread-root".into()),
        active_thread_id: Some("thread-active".into()),
        execution_thread_ids: vec!["thread-root".into(), "thread-active".into()],
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Detailed".into(),
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Detailed");
    assert!(goal.launch_assignment_snapshot.is_empty());
    assert!(goal.runtime_assignment_list.is_empty());
    assert!(goal.root_thread_id.is_none());
    assert!(goal.active_thread_id.is_none());
    assert!(goal.execution_thread_ids.is_empty());
}

#[test]
fn goal_run_update_preserves_mission_control_metadata_when_omitted() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        launch_assignment_snapshot: vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("high"),
            true,
        )],
        runtime_assignment_list: vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("high"),
            true,
        )],
        root_thread_id: Some("thread-root".into()),
        active_thread_id: Some("thread-active".into()),
        execution_thread_ids: vec!["thread-root".into(), "thread-active".into()],
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "g1".into(),
        title: "Updated".into(),
        sparse_update: true,
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Original");
    assert_eq!(
        goal.launch_assignment_snapshot,
        vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("high"),
            true,
        )]
    );
    assert_eq!(
        goal.runtime_assignment_list,
        vec![make_assignment(
            "planner",
            true,
            "openai",
            "gpt-4.1",
            Some("high"),
            true,
        )]
    );
    assert_eq!(goal.root_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(goal.active_thread_id.as_deref(), Some("thread-active"));
    assert_eq!(
        goal.execution_thread_ids,
        vec!["thread-root".to_string(), "thread-active".to_string()]
    );
}

#[test]
fn goal_run_detail_received_stores_owner_profiles() {
    let mut state = TaskState::new();
    let converted = crate::app::conversion::convert_goal_run(crate::wire::GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        planner_owner_profile: Some(make_wire_owner_profile(
            "Planner",
            "openai",
            "gpt-4.1",
            Some("high"),
        )),
        current_step_owner_profile: Some(make_wire_owner_profile(
            "Verifier",
            "anthropic",
            "claude-sonnet-4",
            None,
        )),
        ..Default::default()
    });
    state.reduce(TaskAction::GoalRunDetailReceived(converted));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(
        goal.planner_owner_profile,
        Some(make_owner_profile(
            "Planner",
            "openai",
            "gpt-4.1",
            Some("high")
        ))
    );
    assert_eq!(
        goal.current_step_owner_profile,
        Some(make_owner_profile(
            "Verifier",
            "anthropic",
            "claude-sonnet-4",
            None
        ))
    );
}

#[test]
fn goal_run_detail_received_clears_owner_profiles_when_authoritative_payload_omits_them() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        planner_owner_profile: Some(make_owner_profile(
            "Planner",
            "openai",
            "gpt-4.1",
            Some("high"),
        )),
        current_step_owner_profile: Some(make_owner_profile(
            "Verifier",
            "anthropic",
            "claude-sonnet-4",
            None,
        )),
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Detailed".into(),
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Detailed");
    assert!(goal.planner_owner_profile.is_none());
    assert!(goal.current_step_owner_profile.is_none());
}

#[test]
fn goal_run_update_preserves_owner_profiles_when_incremental_payload_omits_them() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        planner_owner_profile: Some(make_owner_profile(
            "Planner",
            "openai",
            "gpt-4.1",
            Some("high"),
        )),
        current_step_owner_profile: Some(make_owner_profile(
            "Verifier",
            "anthropic",
            "claude-sonnet-4",
            None,
        )),
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "g1".into(),
        title: "Updated".into(),
        sparse_update: true,
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Original");
    assert_eq!(
        goal.planner_owner_profile,
        Some(make_owner_profile(
            "Planner",
            "openai",
            "gpt-4.1",
            Some("high")
        ))
    );
    assert_eq!(
        goal.current_step_owner_profile,
        Some(make_owner_profile(
            "Verifier",
            "anthropic",
            "claude-sonnet-4",
            None
        ))
    );
}

#[test]
fn task_update_drops_stale_awaiting_approval_id_when_status_advances() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![AgentTask {
        id: "task-1".into(),
        title: "Hydrated approval".into(),
        thread_id: Some("thread-1".into()),
        status: Some(TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".into()),
        ..Default::default()
    }]));

    state.reduce(TaskAction::TaskUpdate(AgentTask {
        id: "task-1".into(),
        title: "Hydrated approval".into(),
        thread_id: Some("thread-1".into()),
        status: Some(TaskStatus::Queued),
        awaiting_approval_id: None,
        ..Default::default()
    }));

    let task = state.task_by_id("task-1").expect("task should exist");
    assert_eq!(task.status, Some(TaskStatus::Queued));
    assert!(
        task.awaiting_approval_id.is_none(),
        "non-awaiting task updates should clear stale approval IDs instead of preserving them"
    );
}

#[test]
fn goal_run_update_preserves_planner_fallback_when_current_step_owner_is_absent() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        planner_owner_profile: Some(make_owner_profile("Planner", "openai", "gpt-4.1", None)),
        current_step_owner_profile: None,
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "g1".into(),
        title: "Updated".into(),
        sparse_update: true,
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Original");
    assert_eq!(
        goal.planner_owner_profile,
        Some(make_owner_profile("Planner", "openai", "gpt-4.1", None))
    );
    assert!(goal.current_step_owner_profile.is_none());
}

#[test]
fn goal_run_update_preserves_existing_dossier_when_incremental_update_omits_it() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        dossier: Some(GoalRunDossier {
            projection_state: "in_progress".into(),
            summary: Some("planner seeded units".into()),
            units: vec![GoalDeliveryUnitRecord {
                id: "unit-1".into(),
                title: "Ship android logging".into(),
                status: "queued".into(),
                execution_binding: "builtin:android".into(),
                verification_binding: "subagent:validator".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "g1".into(),
        title: "Updated".into(),
        sparse_update: true,
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Original");
    let dossier = goal.dossier.as_ref().expect("dossier should be preserved");
    assert_eq!(dossier.projection_state, "in_progress");
    assert_eq!(dossier.summary.as_deref(), Some("planner seeded units"));
    assert_eq!(dossier.units.len(), 1);
    assert_eq!(dossier.units[0].execution_binding, "builtin:android");
}

#[test]
fn goal_run_detail_received_clears_dossier_when_authoritative_payload_omits_it() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        dossier: Some(GoalRunDossier {
            projection_state: "in_progress".into(),
            summary: Some("planner seeded units".into()),
            ..Default::default()
        }),
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Detailed".into(),
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Detailed");
    assert!(goal.dossier.is_none());
}

#[test]
fn goal_run_update_from_partial_wire_payload_preserves_existing_scalar_fields() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        goal: "Ship mission control".into(),
        current_step_index: 3,
        child_task_count: 5,
        approval_count: 2,
        created_at: 10,
        updated_at: 20,
        dossier: Some(GoalRunDossier {
            projection_state: "running".into(),
            ..Default::default()
        }),
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "g1".into(),
        title: "Updated".into(),
        sparse_update: true,
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Original");
    assert_eq!(goal.goal, "Ship mission control");
    assert_eq!(goal.current_step_index, 3);
    assert_eq!(goal.child_task_count, 5);
    assert_eq!(goal.approval_count, 2);
    assert_eq!(goal.created_at, 10);
    assert_eq!(goal.updated_at, 20);
    assert_eq!(
        goal.dossier
            .as_ref()
            .expect("incremental update should preserve existing dossier")
            .projection_state,
        "running"
    );
}

#[test]
fn goal_run_update_full_snapshot_can_clear_scalar_fields() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "g1".into(),
        title: "Original".into(),
        goal: "Ship mission control".into(),
        child_task_count: 5,
        approval_count: 2,
        current_step_index: 3,
        created_at: 11,
        updated_at: 22,
        ..Default::default()
    }));

    state.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "g1".into(),
        title: "Updated".into(),
        goal: String::new(),
        child_task_count: 0,
        approval_count: 0,
        current_step_index: 0,
        created_at: 30,
        updated_at: 40,
        ..Default::default()
    }));

    let goal = state.goal_run_by_id("g1").expect("goal should exist");
    assert_eq!(goal.title, "Updated");
    assert!(goal.goal.is_empty());
    assert_eq!(goal.child_task_count, 0);
    assert_eq!(goal.approval_count, 0);
    assert_eq!(goal.current_step_index, 0);
    assert_eq!(goal.created_at, 30);
    assert_eq!(goal.updated_at, 40);
}

#[test]
fn goal_run_checkpoints_received_replaces_goal_run_checkpoints() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::GoalRunCheckpointsReceived {
        goal_run_id: "g1".into(),
        checkpoints: vec![GoalRunCheckpointSummary {
            id: "cp_1".into(),
            checkpoint_type: "manual".into(),
            task_count: 2,
            ..Default::default()
        }],
    });
    assert_eq!(state.checkpoints_for_goal_run("g1").len(), 1);
    assert_eq!(state.checkpoints_for_goal_run("g1")[0].id, "cp_1");

    state.reduce(TaskAction::GoalRunCheckpointsReceived {
        goal_run_id: "g1".into(),
        checkpoints: vec![],
    });
    assert!(state.checkpoints_for_goal_run("g1").is_empty());
}

#[test]
fn heartbeat_items_received_replaces() {
    let mut state = TaskState::new();
    let items = vec![
        HeartbeatItem {
            id: "h1".into(),
            label: "Service A".into(),
            ..Default::default()
        },
        HeartbeatItem {
            id: "h2".into(),
            label: "Service B".into(),
            ..Default::default()
        },
    ];
    state.reduce(TaskAction::HeartbeatItemsReceived(items));
    assert_eq!(state.heartbeat_items().len(), 2);

    state.reduce(TaskAction::HeartbeatItemsReceived(vec![]));
    assert_eq!(state.heartbeat_items().len(), 0);
}

#[test]
fn task_by_id_returns_correct_task() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![
        make_task("t1", "Alpha", 1, None, None, None, None),
        make_task("t2", "Beta", 2, None, None, None, None),
    ]));
    assert_eq!(
        state.task_by_id("t2").map(|t| t.title.as_str()),
        Some("Beta")
    );
    assert!(state.task_by_id("unknown").is_none());
}

#[test]
fn task_update_preserves_hydrated_spawned_tree_metadata() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![make_task(
        "task-1",
        "Original",
        11,
        Some("thread-1"),
        Some("parent-task"),
        Some("parent-thread"),
        Some(TaskStatus::InProgress),
    )]));

    state.reduce(TaskAction::TaskUpdate(AgentTask {
        id: "task-1".into(),
        title: "Updated".into(),
        status: Some(TaskStatus::Completed),
        ..Default::default()
    }));

    let task = state.task_by_id("task-1").expect("task");
    assert_eq!(task.title, "Updated");
    assert_eq!(task.status, Some(TaskStatus::Completed));
    assert_eq!(task.created_at, 11);
    assert_eq!(task.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(task.parent_task_id.as_deref(), Some("parent-task"));
    assert_eq!(task.parent_thread_id.as_deref(), Some("parent-thread"));
}

#[test]
fn spawned_tree_nests_descendants_by_parent_task_id() {
    let tasks = vec![
        make_task(
            "root-task",
            "Root",
            10,
            Some("thread-root"),
            None,
            None,
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "child-task",
            "Child",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-root"),
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "grandchild-task",
            "Grandchild",
            30,
            Some("thread-grandchild"),
            Some("child-task"),
            Some("thread-child"),
            Some(TaskStatus::Completed),
        ),
    ];

    let tree = derive_spawned_agent_tree(&tasks, Some("thread-root")).expect("tree");
    assert_eq!(tree.active_thread_id, "thread-root");
    assert_eq!(
        tree.anchor.as_ref().map(|node| node.item.id.as_str()),
        Some("root-task")
    );
    assert!(tree.roots.is_empty());
    assert_eq!(
        tree.anchor
            .as_ref()
            .and_then(|node| node.children.first())
            .map(|node| node.item.id.as_str()),
        Some("child-task")
    );
    assert_eq!(
        tree.anchor
            .as_ref()
            .and_then(|node| node.children.first())
            .and_then(|node| node.children.first())
            .map(|node| node.item.id.as_str()),
        Some("grandchild-task")
    );
    assert!(tree
        .anchor
        .as_ref()
        .and_then(|node| node.children.first())
        .is_some_and(|node| node.openable));
}

#[test]
fn spawned_tree_uses_parent_thread_id_for_visible_roots() {
    let tasks = vec![
        make_task(
            "orphan-root",
            "Orphan Root",
            10,
            Some("thread-orphan-root"),
            None,
            None,
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "spawned-root",
            "Spawned Root",
            20,
            Some("thread-spawned"),
            Some("missing-parent"),
            Some("thread-parent"),
            Some(TaskStatus::InProgress),
        ),
    ];

    let tree = derive_spawned_agent_tree(&tasks, Some("thread-parent")).expect("tree");
    assert!(tree.anchor.is_none());
    assert_eq!(
        tree.roots
            .iter()
            .map(|node| node.item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["spawned-root"]
    );
    assert!(tree.roots[0].openable);
}

#[test]
fn spawned_tree_keeps_partial_branches_when_parents_are_missing() {
    let tasks = vec![
        make_task(
            "root-task",
            "Root",
            10,
            Some("thread-root"),
            None,
            None,
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "orphan-child",
            "Orphan Child",
            20,
            Some("thread-orphan"),
            Some("missing-parent"),
            Some("thread-root"),
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "grandchild",
            "Grandchild",
            30,
            Some("thread-grandchild"),
            Some("orphan-child"),
            Some("thread-orphan"),
            Some(TaskStatus::Completed),
        ),
    ];

    let tree = derive_spawned_agent_tree(&tasks, Some("thread-root")).expect("tree");
    assert_eq!(
        tree.roots
            .iter()
            .map(|node| node.item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["orphan-child"]
    );
    assert_eq!(
        tree.roots[0]
            .children
            .first()
            .map(|node| node.item.id.as_str()),
        Some("grandchild")
    );
    assert!(tree.roots[0].children[0].openable);
    assert!(!tree.roots[0].children[0].live);
}

#[test]
fn spawned_tree_blocks_self_referential_child_loops() {
    let tasks = vec![
        make_task(
            "root-task",
            "Root",
            10,
            Some("thread-root"),
            None,
            None,
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "child-task",
            "Child",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-child"),
            Some(TaskStatus::InProgress),
        ),
    ];

    let tree = derive_spawned_agent_tree(&tasks, Some("thread-root")).expect("tree");
    let child = tree
        .anchor
        .as_ref()
        .and_then(|node| node.children.first())
        .expect("child");
    assert_eq!(child.item.id, "child-task");
    assert!(child.children.is_empty());
}

#[test]
fn spawned_tree_does_not_reintroduce_cycles_as_descendants() {
    let tasks = vec![
        make_task(
            "root-task",
            "Root",
            10,
            Some("thread-root"),
            Some("cycle-child"),
            Some("thread-root"),
            Some(TaskStatus::InProgress),
        ),
        make_task(
            "cycle-child",
            "Cycle Child",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-root"),
            Some(TaskStatus::InProgress),
        ),
    ];

    let tree = derive_spawned_agent_tree(&tasks, Some("thread-root")).expect("tree");
    let root = tree.anchor.as_ref().expect("anchor");
    assert_eq!(root.item.id, "root-task");
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].item.id, "cycle-child");
    assert!(root.children[0].children.is_empty());
}

#[test]
fn heartbeat_digest_received_stores_and_replaces() {
    let mut state = TaskState::new();
    assert!(state.last_digest().is_none());
    state.reduce(TaskAction::HeartbeatDigestReceived(HeartbeatDigestVm {
        cycle_id: "c1".into(),
        actionable: true,
        digest: "2 items".into(),
        items: vec![HeartbeatDigestItemVm {
            priority: 1,
            check_type: "stale_todos".into(),
            title: "3 stale TODOs".into(),
            suggestion: "Review them".into(),
        }],
        checked_at: 1234567890,
        explanation: None,
    }));
    let d = state.last_digest().unwrap();
    assert_eq!(d.cycle_id, "c1");
    assert!(d.actionable);
    assert_eq!(d.items.len(), 1);
    assert_eq!(d.items[0].priority, 1);

    // Replace with new digest
    state.reduce(TaskAction::HeartbeatDigestReceived(HeartbeatDigestVm {
        cycle_id: "c2".into(),
        actionable: false,
        digest: "All clear".into(),
        items: vec![],
        checked_at: 1234567891,
        explanation: None,
    }));
    let d = state.last_digest().unwrap();
    assert_eq!(d.cycle_id, "c2");
    assert!(!d.actionable);
    assert!(d.items.is_empty());
}
