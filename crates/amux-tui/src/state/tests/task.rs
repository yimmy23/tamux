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
