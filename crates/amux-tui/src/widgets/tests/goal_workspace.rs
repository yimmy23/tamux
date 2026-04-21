use super::*;
use crate::state::goal_workspace::{GoalWorkspaceMode, GoalWorkspaceState};
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
        goal: "Research the ecosystem and produce a concrete learning plan.".into(),
        thread_id: Some("thread-1".into()),
        status: Some(crate::state::task::GoalRunStatus::Running),
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
            message: "goal todo updated with a much longer explanation that should wrap onto another visual line in the timeline pane".into(),
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
        dossier: Some(crate::state::task::GoalRunDossier {
            summary: Some("Checkpoint-backed execution dossier".into()),
            projection_state: "in_progress".into(),
            ..Default::default()
        }),
        ..Default::default()
    }));
    tasks.reduce(TaskAction::GoalRunCheckpointsReceived {
        goal_run_id: "goal-1".into(),
        checkpoints: vec![crate::state::task::GoalRunCheckpointSummary {
            id: "checkpoint-1".into(),
            checkpoint_type: "pre_step".into(),
            step_index: Some(0),
            context_summary_preview: Some("Checkpoint for Plan".into()),
            ..Default::default()
        }],
    });
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

fn render_plain_text(state: &GoalWorkspaceState, tick_counter: u64) -> String {
    render_plain_text_for_tasks(&sample_tasks(), state, tick_counter)
}

fn render_plain_text_for_tasks(
    tasks: &TaskState,
    state: &GoalWorkspaceState,
    tick_counter: u64,
) -> String {
    let area = Rect::new(0, 0, 100, 28);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                tasks,
                "goal-1",
                state,
                &ThemeTokens::default(),
                tick_counter,
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

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Run timeline"), "{plain}");
    assert!(plain.contains("Goal"), "{plain}");
}

#[test]
fn goal_workspace_goal_mode_renders_prompt_and_files_list() {
    let state = GoalWorkspaceState::new();

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Goal Prompt"), "{plain}");
    assert!(plain.contains("[Show]"), "{plain}");
    assert!(!plain.contains("Research the ecosystem"), "{plain}");
    assert!(plain.contains("Main agent"), "{plain}");
    assert!(plain.contains("Files"), "{plain}");
    assert!(plain.contains("/tmp/plan.md"), "{plain}");
}

#[test]
fn goal_workspace_renders_steps_and_nested_todos_for_expanded_step() {
    let mut state = GoalWorkspaceState::new();
    state.set_prompt_expanded(true);
    state.set_step_expanded("step-1", true);

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Research the ecosystem"), "{plain}");
    assert!(plain.contains("Ship"), "{plain}");
    assert!(plain.contains("Draft outline"), "{plain}");
    assert!(plain.contains("Verify sources"), "{plain}");
}

#[test]
fn goal_workspace_progress_mode_renders_progress_panel_copy() {
    let mut state = GoalWorkspaceState::new();
    state.set_mode(GoalWorkspaceMode::Progress);

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Progress"), "{plain}");
    assert!(plain.contains("Checkpoints"), "{plain}");
}

#[test]
fn goal_workspace_threads_mode_renders_thread_inventory() {
    let mut state = GoalWorkspaceState::new();
    state.set_mode(GoalWorkspaceMode::Threads);

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Threads"), "{plain}");
    assert!(plain.contains("Goal thread"), "{plain}");
    assert!(plain.contains("thread-1"), "{plain}");
}

#[test]
fn goal_workspace_plan_falls_back_to_goal_task_thread_when_run_thread_ids_are_missing() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![crate::state::task::AgentTask {
        id: "task-1".into(),
        title: "Worker Task".into(),
        thread_id: Some("thread-worker".into()),
        goal_run_id: Some("goal-1".into()),
        status: Some(crate::state::task::TaskStatus::InProgress),
        ..Default::default()
    }]));
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        goal: "Investigate fallback thread discovery.".into(),
        status: Some(crate::state::task::GoalRunStatus::Running),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));

    let plain = render_plain_text_for_tasks(&tasks, &GoalWorkspaceState::new(), 0);

    assert!(plain.contains("Main agent"), "{plain}");
    assert!(plain.contains("thread-worker"), "{plain}");
}

#[test]
fn goal_workspace_hit_test_distinguishes_step_and_todo_rows() {
    let mut state = GoalWorkspaceState::new();
    state.set_step_expanded("step-1", true);
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);

    let prompt_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 5));
    let thread_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 6));
    let step_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 7));
    let todo_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(4, 8));

    assert_eq!(prompt_hit, Some(GoalWorkspaceHitTarget::PlanPromptToggle));
    assert_eq!(
        thread_hit,
        Some(GoalWorkspaceHitTarget::PlanMainThread("thread-1".into()))
    );
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
    let detail_hit = (area.y..area.y.saturating_add(area.height))
        .find_map(|row| {
            (area.x..area.x.saturating_add(area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                (hit_test(area, &tasks, "goal-1", &state, pos)
                    == Some(GoalWorkspaceHitTarget::DetailFile("/tmp/plan.md".into())))
                .then_some(GoalWorkspaceHitTarget::DetailFile("/tmp/plan.md".into()))
            })
        });

    assert_eq!(timeline_hit, Some(GoalWorkspaceHitTarget::TimelineRow(0)));
    assert_eq!(
        detail_hit,
        Some(GoalWorkspaceHitTarget::DetailFile("/tmp/plan.md".into()))
    );
}

#[test]
fn goal_workspace_hit_test_tracks_mode_tabs_and_wrapped_timeline_lines() {
    let state = GoalWorkspaceState::new();
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);

    let progress_tab_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(8, 1));
    let wrapped_timeline_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(42, 6));

    assert_eq!(
        progress_tab_hit,
        Some(GoalWorkspaceHitTarget::ModeTab(GoalWorkspaceMode::Progress))
    );
    assert_eq!(wrapped_timeline_hit, Some(GoalWorkspaceHitTarget::TimelineRow(0)));
}

#[test]
fn goal_workspace_running_timeline_row_animates_across_ticks() {
    let state = GoalWorkspaceState::new();

    let tick_0 = render_plain_text(&state, 0);
    let tick_1 = render_plain_text(&state, 1);

    assert!(tick_0.contains("⠋") || tick_0.contains("⠙") || tick_0.contains("⠹") || tick_0.contains("⠸"), "{tick_0}");
    assert_ne!(tick_0, tick_1);
}
