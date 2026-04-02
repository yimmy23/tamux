use super::*;

fn make_task(id: &str, title: &str) -> AgentTask {
    AgentTask {
        id: id.into(),
        title: title.into(),
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

#[test]
fn task_list_received_replaces_tasks() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![
        make_task("t1", "First"),
        make_task("t2", "Second"),
    ]));
    assert_eq!(state.tasks().len(), 2);

    // Replace with a smaller list
    state.reduce(TaskAction::TaskListReceived(vec![make_task("t3", "Third")]));
    assert_eq!(state.tasks().len(), 1);
    assert_eq!(state.tasks()[0].id, "t3");
}

#[test]
fn task_update_upserts_by_id() {
    let mut state = TaskState::new();
    state.reduce(TaskAction::TaskListReceived(vec![make_task(
        "t1", "Original",
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
    state.reduce(TaskAction::TaskUpdate(make_task("t2", "New")));
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
        make_task("t1", "Alpha"),
        make_task("t2", "Beta"),
    ]));
    assert_eq!(
        state.task_by_id("t2").map(|t| t.title.as_str()),
        Some("Beta")
    );
    assert!(state.task_by_id("unknown").is_none());
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
