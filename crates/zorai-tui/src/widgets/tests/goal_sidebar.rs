use super::*;
use crate::state::goal_sidebar::{GoalSidebarState, GoalSidebarTab};
use crate::state::task::{
    AgentTask, GoalRun, GoalRunCheckpointSummary, GoalRunStep, TaskAction, TaskState,
    ThreadWorkContext, WorkContextEntry, WorkContextEntryKind,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn set_tab(state: &mut GoalSidebarState, tab: GoalSidebarTab) {
    while state.active_tab() != tab {
        state.cycle_tab_right();
    }
}

fn render_plain_text(state: &GoalSidebarState, tasks: &TaskState, goal_run_id: &str) -> String {
    let area = Rect::new(0, 0, 80, 10);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                tasks,
                goal_run_id,
                state,
                &ThemeTokens::default(),
            );
        })
        .expect("goal sidebar render should succeed");

    let buffer = terminal.backend().buffer();
    (area.y..area.y.saturating_add(area.height))
        .map(|y| {
            (area.x..area.x.saturating_add(area.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn populated_goal_state() -> (TaskState, String) {
    let mut tasks = TaskState::new();
    let goal_run_id = "goal-1".to_string();

    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: goal_run_id.clone(),
        title: "Goal".to_string(),
        thread_id: Some("thread-1".to_string()),
        child_task_ids: vec!["task-2".to_string(), "task-1".to_string()],
        steps: vec![
            GoalRunStep {
                id: "step-2".to_string(),
                title: "Second step".to_string(),
                order: 2,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-1".to_string(),
                title: "First step".to_string(),
                order: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    tasks.reduce(TaskAction::GoalRunCheckpointsReceived {
        goal_run_id: goal_run_id.clone(),
        checkpoints: vec![GoalRunCheckpointSummary {
            id: "checkpoint-1".to_string(),
            checkpoint_type: "review".to_string(),
            context_summary_preview: Some("Checkpoint summary".to_string()),
            ..Default::default()
        }],
    });

    tasks.reduce(TaskAction::TaskListReceived(vec![
        AgentTask {
            id: "task-1".to_string(),
            title: "First task".to_string(),
            goal_run_id: Some(goal_run_id.clone()),
            status: Some(crate::state::task::TaskStatus::InProgress),
            ..Default::default()
        },
        AgentTask {
            id: "task-2".to_string(),
            title: "Second task".to_string(),
            goal_run_id: Some(goal_run_id.clone()),
            status: Some(crate::state::task::TaskStatus::Queued),
            ..Default::default()
        },
    ]));

    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "thread-1".to_string(),
        entries: vec![
            WorkContextEntry {
                path: "/tmp/shared.txt".to_string(),
                kind: Some(WorkContextEntryKind::Artifact),
                ..Default::default()
            },
            WorkContextEntry {
                path: "/tmp/goal-only.txt".to_string(),
                goal_run_id: Some(goal_run_id.clone()),
                change_kind: Some("diff".to_string()),
                kind: Some(WorkContextEntryKind::RepoChange),
                ..Default::default()
            },
            WorkContextEntry {
                path: "/tmp/other-goal.txt".to_string(),
                goal_run_id: Some("other-goal".to_string()),
                ..Default::default()
            },
        ],
    }));

    (tasks, goal_run_id)
}

#[test]
fn goal_sidebar_renders_fixed_tab_labels() {
    let state = GoalSidebarState::new();
    let tasks = TaskState::new();
    let plain = render_plain_text(&state, &tasks, "goal-1");

    assert!(plain.contains("Steps"), "missing Steps tab: {plain}");
    assert!(
        plain.contains("Checkpoints"),
        "missing Checkpoints tab: {plain}"
    );
    assert!(plain.contains("Tasks"), "missing Tasks tab: {plain}");
    assert!(plain.contains("Files"), "missing Files tab: {plain}");
}

#[test]
fn goal_sidebar_renders_empty_state_rows_per_tab() {
    let tasks = TaskState::new();
    let goal_run_id = "goal-1";
    let cases = [
        (GoalSidebarTab::Steps, "No steps"),
        (GoalSidebarTab::Checkpoints, "No checkpoints"),
        (GoalSidebarTab::Tasks, "No tasks"),
        (GoalSidebarTab::Files, "No files"),
    ];

    for (tab, expected) in cases {
        let mut state = GoalSidebarState::new();
        set_tab(&mut state, tab);
        let plain = render_plain_text(&state, &tasks, goal_run_id);
        assert!(
            plain.contains(expected),
            "expected empty-state row for {tab:?}, got: {plain}"
        );
    }
}

#[test]
fn goal_sidebar_hit_test_returns_row_targets_for_each_tab() {
    let (tasks, goal_run_id) = populated_goal_state();
    let area = Rect::new(0, 0, 80, 10);
    let mouse = Position::new(2, 1);
    let cases = [
        (GoalSidebarTab::Steps, GoalSidebarHitTarget::Step(0)),
        (
            GoalSidebarTab::Checkpoints,
            GoalSidebarHitTarget::Checkpoint(0),
        ),
        (GoalSidebarTab::Tasks, GoalSidebarHitTarget::Task(0)),
        (GoalSidebarTab::Files, GoalSidebarHitTarget::File(0)),
    ];

    for (tab, expected) in cases {
        let mut state = GoalSidebarState::new();
        set_tab(&mut state, tab);
        let hit = hit_test(area, &tasks, &goal_run_id, &state, mouse);
        assert_eq!(hit, Some(expected), "unexpected hit target for {tab:?}");
    }
}

#[test]
fn goal_sidebar_item_count_matches_rendered_rows_for_populated_tabs() {
    let (tasks, goal_run_id) = populated_goal_state();
    let cases = [
        (GoalSidebarTab::Steps, vec!["First step", "Second step"]),
        (GoalSidebarTab::Checkpoints, vec!["Checkpoint summary"]),
        (GoalSidebarTab::Tasks, vec!["First task", "Second task"]),
        (GoalSidebarTab::Files, vec!["shared.txt", "goal-only.txt"]),
    ];

    for (tab, expected_rows) in cases {
        let mut state = GoalSidebarState::new();
        set_tab(&mut state, tab);
        let plain = render_plain_text(&state, &tasks, &goal_run_id);
        let rendered_rows = plain
            .lines()
            .filter(|line| expected_rows.iter().any(|needle| line.contains(needle)))
            .count();

        assert_eq!(
            item_count(&tasks, &goal_run_id, &state),
            expected_rows.len(),
            "item count should match populated rows for {tab:?}"
        );
        assert_eq!(
            rendered_rows,
            expected_rows.len(),
            "rendered row count should match populated rows for {tab:?}: {plain}"
        );
    }
}
