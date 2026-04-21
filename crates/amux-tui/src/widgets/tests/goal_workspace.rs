use super::*;
use crate::state::goal_workspace::GoalWorkspaceState;
use crate::state::task::{
    GoalRun, GoalRunEvent, GoalRunStep, TaskAction, TaskState, ThreadWorkContext, TodoItem,
    TodoStatus, WorkContextEntry,
};
use crate::theme::ThemeTokens;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn sample_tasks() -> TaskState {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        thread_id: Some("thread-1".into()),
        steps: vec![
            GoalRunStep {
                id: "step-2".into(),
                title: "Ship".into(),
                order: 1,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-1".into(),
                title: "Plan".into(),
                order: 0,
                ..Default::default()
            },
        ],
        events: vec![GoalRunEvent {
            id: "event-1".into(),
            timestamp: 10,
            step_index: Some(0),
            todo_snapshot: vec![
                TodoItem {
                    id: "todo-1".into(),
                    content: "Draft outline".into(),
                    status: Some(TodoStatus::InProgress),
                    step_index: Some(0),
                    position: 0,
                    ..Default::default()
                },
                TodoItem {
                    id: "todo-2".into(),
                    content: "Verify sources".into(),
                    status: Some(TodoStatus::Pending),
                    step_index: Some(0),
                    position: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    }));
    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "thread-1".into(),
        entries: vec![WorkContextEntry {
            path: "/tmp/plan.md".into(),
            goal_run_id: Some("goal-1".into()),
            step_index: Some(0),
            ..Default::default()
        }],
    }));
    tasks
}

fn render_plain_text(state: &GoalWorkspaceState) -> String {
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &tasks,
                "goal-1",
                state,
                &ThemeTokens::default(),
            );
        })
        .expect("goal workspace render should succeed");

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

#[test]
fn goal_workspace_renders_plan_timeline_and_details_panes() {
    let state = GoalWorkspaceState::new();

    let plain = render_plain_text(&state);

    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Run timeline"), "{plain}");
    assert!(plain.contains("Details"), "{plain}");
}

#[test]
fn goal_workspace_renders_steps_and_nested_todos_for_expanded_step() {
    let mut state = GoalWorkspaceState::new();
    state.set_step_expanded("step-1", true);

    let plain = render_plain_text(&state);

    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Ship"), "{plain}");
    assert!(plain.contains("Draft outline"), "{plain}");
    assert!(plain.contains("Verify sources"), "{plain}");
}

#[test]
fn goal_workspace_hit_test_distinguishes_step_and_todo_rows() {
    let mut state = GoalWorkspaceState::new();
    state.set_step_expanded("step-1", true);
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);

    let step_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 5));
    let todo_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(4, 6));

    assert_eq!(
        step_hit,
        Some(GoalWorkspaceHitTarget::PlanStep("step-1".into()))
    );
    assert_eq!(
        todo_hit,
        Some(GoalWorkspaceHitTarget::PlanTodo {
            step_id: "step-1".into(),
            todo_id: "todo-1".into(),
        })
    );
}

#[test]
fn goal_workspace_hit_test_tracks_timeline_and_detail_rows() {
    let state = GoalWorkspaceState::new();
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);

    let timeline_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(42, 5));
    let detail_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(74, 6));

    assert_eq!(timeline_hit, Some(GoalWorkspaceHitTarget::TimelineRow(0)));
    assert_eq!(
        detail_hit,
        Some(GoalWorkspaceHitTarget::DetailFile("/tmp/plan.md".into()))
    );
}
