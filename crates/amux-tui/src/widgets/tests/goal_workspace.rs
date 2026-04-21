use super::*;
use crate::state::goal_workspace::{GoalWorkspaceMode, GoalWorkspaceState};
use crate::state::task::{
    GoalRun, GoalRunEvent, GoalRunStep, TaskAction, TaskState, ThreadWorkContext, TodoItem,
    TodoStatus, WorkContextEntry,
};
use crate::theme::ThemeTokens;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

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

fn home_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct HomeEnvGuard {
    original_home: Option<String>,
}

impl HomeEnvGuard {
    fn set(path: &Path) -> Self {
        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", path);
        }
        Self { original_home }
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        match self.original_home.take() {
            Some(value) => unsafe {
                std::env::set_var("HOME", value);
            },
            None => unsafe {
                std::env::remove_var("HOME");
            },
        }
    }
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

fn render_buffer_for_tasks(
    tasks: &TaskState,
    state: &GoalWorkspaceState,
    tick_counter: u64,
) -> (Rect, ratatui::buffer::Buffer) {
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

    let buffer = terminal.backend().buffer().clone();
    (area, buffer)
}

#[test]
fn goal_workspace_renders_plan_timeline_and_details_panes() {
    let state = GoalWorkspaceState::new();

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Plan"), "{plain}");
    assert!(plain.contains("Run timeline"), "{plain}");
    assert!(plain.contains("Dossier"), "{plain}");
    assert!(plain.contains("Files"), "{plain}");
}

#[test]
fn goal_workspace_dossier_mode_renders_prompt_without_embedded_files_list() {
    let state = GoalWorkspaceState::new();

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Goal Prompt"), "{plain}");
    assert!(plain.contains("[Show]"), "{plain}");
    assert!(!plain.contains("Research the ecosystem"), "{plain}");
    assert!(plain.contains("Main agent"), "{plain}");
    assert!(!plain.contains("/tmp/plan.md"), "{plain}");
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
fn goal_workspace_files_mode_lists_projection_root_and_nested_inventory_files() {
    let _lock = home_env_lock().lock().expect("home env lock");
    let temp_home = tempfile::tempdir().expect("temp home should exist");
    let _home = HomeEnvGuard::set(temp_home.path());

    let goal_root = amux_protocol::ensure_amux_data_dir()
        .expect("tamux data dir")
        .join("goals")
        .join("goal-1");
    std::fs::create_dir_all(goal_root.join("inventory/execution"))
        .expect("goal inventory tree should exist");
    std::fs::write(goal_root.join("goal.md"), "# Goal\n").expect("goal.md should be written");
    std::fs::write(goal_root.join("dossier.json"), "{}").expect("dossier.json should be written");
    std::fs::write(
        goal_root.join("inventory/execution/step-1-complete.md"),
        "done\n",
    )
    .expect("nested inventory file should be written");

    let mut state = GoalWorkspaceState::new();
    state.set_mode(GoalWorkspaceMode::Files);

    let plain = render_plain_text(&state, 0);

    assert!(plain.contains("Files"), "{plain}");
    assert!(plain.contains("goal.md"), "{plain}");
    assert!(plain.contains("dossier.json"), "{plain}");
    assert!(plain.contains("step-1-complete.md"), "{plain}");
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
                    == Some(GoalWorkspaceHitTarget::DetailCheckpoint("checkpoint-1".into())))
                .then_some(GoalWorkspaceHitTarget::DetailCheckpoint("checkpoint-1".into()))
            })
        });

    assert_eq!(timeline_hit, Some(GoalWorkspaceHitTarget::TimelineRow(0)));
    assert_eq!(
        detail_hit,
        Some(GoalWorkspaceHitTarget::DetailCheckpoint("checkpoint-1".into()))
    );
}

#[test]
fn goal_workspace_hit_test_tracks_mode_tabs_and_wrapped_timeline_lines() {
    let state = GoalWorkspaceState::new();
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);

    let progress_tab_hit = (area.x..area.x.saturating_add(area.width)).find_map(|column| {
        let pos = Position::new(column, area.y + 1);
        (hit_test(area, &tasks, "goal-1", &state, pos)
            == Some(GoalWorkspaceHitTarget::ModeTab(GoalWorkspaceMode::Progress)))
        .then_some(GoalWorkspaceHitTarget::ModeTab(GoalWorkspaceMode::Progress))
    });
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

#[test]
fn goal_workspace_plan_step_markers_reflect_status_with_color_and_pulse() {
    let theme = ThemeTokens::default();
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        goal: "Preview plan marker states.".into(),
        thread_id: Some("thread-1".into()),
        status: Some(crate::state::task::GoalRunStatus::Running),
        current_step_index: 2,
        current_step_title: Some("Running step".into()),
        steps: vec![
            GoalRunStep {
                id: "step-1".into(),
                title: "Pending step".into(),
                order: 0,
                status: Some(crate::state::task::GoalRunStatus::Queued),
                ..Default::default()
            },
            GoalRunStep {
                id: "step-2".into(),
                title: "Completed step".into(),
                order: 1,
                status: Some(crate::state::task::GoalRunStatus::Completed),
                ..Default::default()
            },
            GoalRunStep {
                id: "step-3".into(),
                title: "Running step".into(),
                order: 2,
                status: Some(crate::state::task::GoalRunStatus::Running),
                ..Default::default()
            },
            GoalRunStep {
                id: "step-4".into(),
                title: "Errored step".into(),
                order: 3,
                status: Some(crate::state::task::GoalRunStatus::Failed),
                error: Some("boom".into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let state = GoalWorkspaceState::new();
    let (area, buffer_tick_0) = render_buffer_for_tasks(&tasks, &state, 0);
    let (_, buffer_tick_1) = render_buffer_for_tasks(&tasks, &state, 1);
    let plan_inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(workspace_layout(area).expect("workspace layout").plan);

    let marker_for = |buffer: &ratatui::buffer::Buffer, title: &str| {
        let y = (plan_inner.y..plan_inner.y.saturating_add(plan_inner.height))
            .find(|y| {
                let row = (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
                    .filter_map(|x| buffer.cell((x, *y)).map(|cell| cell.symbol()))
                    .collect::<String>();
                row.contains(title)
            })
            .expect("step row should exist");
        (plan_inner.x..plan_inner.x.saturating_add(6))
            .filter_map(|x| buffer.cell((x, y)).map(|cell| (cell.symbol().to_string(), cell.fg)))
            .find(|(symbol, _)| !symbol.trim().is_empty() && symbol != "▸")
            .expect("marker cell should exist")
    };

    let pending = marker_for(&buffer_tick_0, "Pending step");
    let completed = marker_for(&buffer_tick_0, "Completed step");
    let running_0 = marker_for(&buffer_tick_0, "Running step");
    let running_1 = marker_for(&buffer_tick_1, "Running step");
    let errored_0 = marker_for(&buffer_tick_0, "Errored step");
    let errored_1 = marker_for(&buffer_tick_1, "Errored step");

    assert_eq!(pending.0, "○");
    assert_eq!(pending.1, theme.fg_dim.fg.expect("dim fg"));

    assert_eq!(completed.0, "●");
    assert_eq!(completed.1, theme.accent_success.fg.expect("success fg"));

    assert_eq!(running_0.1, theme.accent_secondary.fg.expect("warning fg"));
    assert_eq!(running_1.1, theme.accent_secondary.fg.expect("warning fg"));
    assert_ne!(running_0.0, running_1.0);

    assert_eq!(errored_0.1, theme.accent_danger.fg.expect("danger fg"));
    assert_eq!(errored_1.1, theme.accent_danger.fg.expect("danger fg"));
    assert_ne!(errored_0.0, errored_1.0);
}
