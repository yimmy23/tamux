use super::super::*;
use crate::state::goal_workspace::{GoalWorkspaceMode, GoalWorkspaceState};
use crate::state::task::{
    AgentTask, GoalAgentAssignment, GoalRun, GoalRunEvent, GoalRunModelUsage, GoalRunStep,
    GoalRuntimeOwnerProfile, TaskAction, TaskState, TaskStatus, ThreadWorkContext, TodoItem,
    TodoStatus, WorkContextEntry,
};
use crate::test_support::{env_var_lock, EnvVarGuard, ZORAI_DATA_DIR_ENV};
use crate::theme::ThemeTokens;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

pub(super) fn sample_tasks() -> TaskState {
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
                instructions: "Interview the user before drafting the plan.".into(),
                summary: Some("Capture constraints before outlining tasks.".into()),
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

pub(super) fn render_plain_text(state: &GoalWorkspaceState, tick_counter: u64) -> String {
    render_plain_text_for_tasks(&sample_tasks(), state, tick_counter)
}

pub(super) fn render_plain_text_for_tasks(
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

pub(super) fn render_buffer_for_tasks(
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
    assert!(plain.contains("Ship"), "{plain}");
    assert!(plain.contains("Draft outline"), "{plain}");
    assert!(plain.contains("Verify sources"), "{plain}");
    assert!(
        !plain.contains("Interview the user before drafting the plan."),
        "{plain}"
    );
    assert!(
        !plain.contains("Capture constraints before outlining tasks."),
        "{plain}"
    );
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
fn goal_workspace_usage_mode_renders_model_and_agent_usage() {
    let mut tasks = sample_tasks();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        AgentTask {
            id: "task-root".into(),
            title: "Root implementation".into(),
            goal_run_id: Some("goal-1".into()),
            status: Some(TaskStatus::Completed),
            thread_id: Some("thread-root".into()),
            ..Default::default()
        },
        AgentTask {
            id: "task-review".into(),
            title: "Verifier subagent".into(),
            goal_run_id: Some("goal-1".into()),
            parent_task_id: Some("task-root".into()),
            status: Some(TaskStatus::Completed),
            thread_id: Some("thread-review".into()),
            ..Default::default()
        },
    ]));
    tasks.reduce(TaskAction::GoalRunUpdate(GoalRun {
        id: "goal-1".into(),
        total_prompt_tokens: 1234,
        total_completion_tokens: 567,
        estimated_cost_usd: Some(0.0425),
        planner_owner_profile: Some(GoalRuntimeOwnerProfile {
            agent_label: "Svarog".into(),
            provider: "openai".into(),
            model: "gpt-5.4".into(),
            reasoning_effort: None,
        }),
        runtime_assignment_list: vec![GoalAgentAssignment {
            role_id: "weles".into(),
            enabled: true,
            provider: "openrouter".into(),
            model: "anthropic/claude-sonnet-4".into(),
            reasoning_effort: Some("high".into()),
            inherit_from_main: false,
        }],
        model_usage: vec![GoalRunModelUsage {
            provider: "openrouter".into(),
            model: "anthropic/claude-sonnet-4".into(),
            request_count: 2,
            prompt_tokens: 1000,
            completion_tokens: 500,
            estimated_cost_usd: Some(0.04),
            duration_ms: Some(90_000),
        }],
        sparse_update: true,
        ..Default::default()
    }));
    let mut state = GoalWorkspaceState::new();
    state.set_mode(GoalWorkspaceMode::Usage);

    let plain = render_plain_text_for_tasks(&tasks, &state, 0);

    assert!(plain.contains("Usage"), "{plain}");
    assert!(plain.contains("Goal total"), "{plain}");
    assert!(plain.contains("prompt 1,234"), "{plain}");
    assert!(plain.contains("completion 567"), "{plain}");
    assert!(plain.contains("$0.0425"), "{plain}");
    assert!(plain.contains("openrouter/anthropic/claude-so"), "{plain}");
    assert!(plain.contains("nnet-4"), "{plain}");
    assert!(plain.contains("2 req"), "{plain}");
    assert!(plain.contains("Planner Svarog"), "{plain}");
    assert!(plain.contains("Role weles"), "{plain}");
    assert!(plain.contains("Subagent Verifier subagent"), "{plain}");
}

#[test]
fn goal_workspace_selected_step_dossier_uses_unit_projection_state() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        goal: "Verify completed step status.".into(),
        status: Some(crate::state::task::GoalRunStatus::Running),
        current_step_index: 1,
        steps: vec![
            GoalRunStep {
                id: "step-1".into(),
                title: "Rebuild matrix".into(),
                status: Some(crate::state::task::GoalRunStatus::Completed),
                order: 0,
                summary: Some("Step passed verification.".into()),
                ..Default::default()
            },
            GoalRunStep {
                id: "step-2".into(),
                title: "Run daemon session".into(),
                status: Some(crate::state::task::GoalRunStatus::Running),
                order: 1,
                ..Default::default()
            },
        ],
        dossier: Some(crate::state::task::GoalRunDossier {
            projection_state: "in_progress".into(),
            summary: Some("Overall run is still executing.".into()),
            units: vec![crate::state::task::GoalDeliveryUnitRecord {
                id: "step-1".into(),
                title: "Rebuild matrix".into(),
                status: "completed".into(),
                summary: Some("Selected unit completed.".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }));

    let plain = render_plain_text_for_tasks(&tasks, &GoalWorkspaceState::new(), 0);

    assert!(plain.contains("Projection completed"), "{plain}");
}

#[test]
fn goal_workspace_files_mode_lists_projection_root_and_nested_inventory_files() {
    let _lock = env_var_lock();
    let temp_home = tempfile::tempdir().expect("temp home should exist");
    let _data_dir = EnvVarGuard::set(ZORAI_DATA_DIR_ENV, temp_home.path());

    let goal_root = zorai_protocol::ensure_zorai_data_dir()
        .expect("zorai data dir")
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
fn goal_workspace_thread_views_include_goal_scoped_live_todo_thread() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        goal: "Track the live worker thread.".into(),
        status: Some(crate::state::task::GoalRunStatus::Running),
        steps: vec![GoalRunStep {
            id: "step-1".into(),
            title: "Plan".into(),
            order: 0,
            ..Default::default()
        }],
        ..Default::default()
    }));
    tasks.reduce(TaskAction::ThreadTodosReceived {
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

    let mut state = GoalWorkspaceState::new();
    state.set_mode(GoalWorkspaceMode::Threads);
    let plain = render_plain_text_for_tasks(&tasks, &state, 0);
    assert!(plain.contains("Live goal thread"), "{plain}");
    assert!(plain.contains("thread-live"), "{plain}");

    state.set_mode(GoalWorkspaceMode::ActiveAgent);
    let plain = render_plain_text_for_tasks(&tasks, &state, 0);
    assert!(plain.contains("thread-live"), "{plain}");
}

#[test]
fn goal_workspace_plan_falls_back_to_goal_task_thread_when_run_thread_ids_are_missing() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        crate::state::task::AgentTask {
            id: "task-1".into(),
            title: "Worker Task".into(),
            thread_id: Some("thread-worker".into()),
            goal_run_id: Some("goal-1".into()),
            status: Some(crate::state::task::TaskStatus::InProgress),
            ..Default::default()
        },
    ]));
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

