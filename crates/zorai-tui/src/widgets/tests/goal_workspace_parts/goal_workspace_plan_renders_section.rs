use super::super::*;
use super::goal_workspace_plan_falls_back::*;
use crate::state::goal_workspace::{GoalWorkspaceMode, GoalWorkspaceState};
use crate::state::task::{GoalRun, GoalRunStep, TaskAction, TaskState};
use crate::theme::ThemeTokens;
use ratatui::backend::TestBackend;
use ratatui::layout::{Position, Rect};
use ratatui::Terminal;

#[test]
fn goal_workspace_hit_test_distinguishes_step_and_todo_rows() {
    let mut state = GoalWorkspaceState::new();
    state.set_step_expanded("step-1", true);
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 100, 28);

    let prompt_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 5));
    let thread_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 6));
    let step_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(2, 8));
    let todo_hit = hit_test(area, &tasks, "goal-1", &state, Position::new(4, 9));

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
    let detail_hit = (area.y..area.y.saturating_add(area.height)).find_map(|row| {
        (area.x..area.x.saturating_add(area.width)).find_map(|column| {
            let pos = Position::new(column, row);
            (hit_test(area, &tasks, "goal-1", &state, pos)
                == Some(GoalWorkspaceHitTarget::DetailCheckpoint(
                    "checkpoint-1".into(),
                )))
            .then_some(GoalWorkspaceHitTarget::DetailCheckpoint(
                "checkpoint-1".into(),
            ))
        })
    });

    assert_eq!(timeline_hit, Some(GoalWorkspaceHitTarget::TimelineRow(0)));
    assert_eq!(
        detail_hit,
        Some(GoalWorkspaceHitTarget::DetailCheckpoint(
            "checkpoint-1".into()
        ))
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
    assert_eq!(
        wrapped_timeline_hit,
        Some(GoalWorkspaceHitTarget::TimelineRow(0))
    );
}

#[test]
fn goal_workspace_selection_point_keeps_wrapped_plan_clicks_on_the_same_logical_row() {
    let state = GoalWorkspaceState::new();
    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 60, 28);
    let plan_inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(workspace_layout(area).expect("workspace layout").plan);

    let wrapped_positions = (plan_inner.y..plan_inner.y.saturating_add(plan_inner.height))
        .filter_map(|y| {
            (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
                .find(|x| {
                    hit_test(area, &tasks, "goal-1", &state, Position::new(*x, y))
                        == Some(GoalWorkspaceHitTarget::PlanPromptToggle)
                })
                .map(|x| Position::new(x, y))
        })
        .collect::<Vec<_>>();

    assert!(
        wrapped_positions.len() >= 2,
        "expected prompt row to wrap in the narrow plan pane"
    );

    let first = selection_point_from_mouse(area, &tasks, "goal-1", &state, wrapped_positions[0])
        .expect("first wrapped click should map to a selection point");
    let second = selection_point_from_mouse(area, &tasks, "goal-1", &state, wrapped_positions[1])
        .expect("second wrapped click should map to a selection point");

    assert_eq!(first.row, second.row);
    assert!(second.col > first.col, "{first:?} vs {second:?}");
}

#[test]
fn goal_workspace_render_highlights_selected_plan_row_after_wrapped_rows() {
    let mut state = GoalWorkspaceState::new();
    state.set_selected_plan_row(1);
    state.set_selected_plan_item(Some(
        crate::state::goal_workspace::GoalPlanSelection::MainThread {
            thread_id: "thread-1".into(),
        },
    ));

    let tasks = sample_tasks();
    let area = Rect::new(0, 0, 60, 28);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &tasks,
                "goal-1",
                &state,
                &ThemeTokens::default(),
                0,
            );
        })
        .expect("goal workspace render should succeed");

    let plan_inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(workspace_layout(area).expect("workspace layout").plan);
    let selected_visual_row =
        plan_visual_row_for_selection(&tasks, "goal-1", &state, plan_inner.width as usize)
            .expect("selected plan row should resolve to a visual row");
    assert!(
        selected_visual_row > state.selected_plan_row(),
        "expected an earlier wrapped row to push the selected item down"
    );

    let buffer = terminal.backend().buffer();
    let selected_bg = ratatui::style::Color::Indexed(236);
    let thread_row_has_selection = (plan_inner.y..plan_inner.y.saturating_add(plan_inner.height))
        .find_map(|y| {
            let row = (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>();
            row.contains("thread-1").then(|| {
                (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width)).any(|x| {
                    buffer
                        .cell((x, y))
                        .map(|cell| cell.bg == selected_bg)
                        .unwrap_or(false)
                })
            })
        })
        .expect("main thread row should be visible");

    assert!(
        thread_row_has_selection,
        "selected plan row should keep the selected background"
    );
}

#[test]
fn goal_workspace_running_timeline_row_animates_across_ticks() {
    let state = GoalWorkspaceState::new();

    let tick_0 = render_plain_text(&state, 0);
    let tick_1 = render_plain_text(&state, 1);

    assert!(
        tick_0.contains("⠋")
            || tick_0.contains("⠙")
            || tick_0.contains("⠹")
            || tick_0.contains("⠸"),
        "{tick_0}"
    );
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
                error: Some("stale completion gate error".into()),
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
            .filter_map(|x| {
                buffer
                    .cell((x, y))
                    .map(|cell| (cell.symbol().to_string(), cell.fg))
            })
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

#[test]
fn goal_workspace_plan_confidence_suffixes_strip_prefixes_and_apply_colors() {
    let theme = ThemeTokens::default();
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal".into(),
        goal: "Preview confidence formatting.".into(),
        thread_id: Some("thread-1".into()),
        status: Some(crate::state::task::GoalRunStatus::Running),
        current_step_title: Some("[MEDIUM] Medium step".into()),
        steps: vec![
            GoalRunStep {
                id: "step-1".into(),
                title: "[LOW] Low step".into(),
                order: 0,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-2".into(),
                title: "[MEDIUM] Medium step".into(),
                order: 1,
                ..Default::default()
            },
            GoalRunStep {
                id: "step-3".into(),
                title: "[HIGH] High step".into(),
                order: 2,
                ..Default::default()
            },
        ],
        ..Default::default()
    }));

    let state = GoalWorkspaceState::new();
    let plain = render_plain_text_for_tasks(&tasks, &state, 0);
    assert!(!plain.contains("[LOW]"), "{plain}");
    assert!(!plain.contains("[MEDIUM]"), "{plain}");
    assert!(!plain.contains("[HIGH]"), "{plain}");
    assert!(plain.contains("1. Low step"), "{plain}");
    assert!(plain.contains("2. Medium step"), "{plain}");
    assert!(plain.contains("3. High step"), "{plain}");
    assert!(plain.contains("Low step ˅"), "{plain}");
    assert!(plain.contains("Medium step ="), "{plain}");
    assert!(plain.contains("High step ˄"), "{plain}");

    let (area, buffer) = render_buffer_for_tasks(&tasks, &state, 0);
    let plan_inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(workspace_layout(area).expect("workspace layout").plan);

    let icon_for = |title: &str, symbol: &str| {
        let y = (plan_inner.y..plan_inner.y.saturating_add(plan_inner.height))
            .find(|y| {
                let row = (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
                    .filter_map(|x| buffer.cell((x, *y)).map(|cell| cell.symbol()))
                    .collect::<String>();
                row.contains(title)
            })
            .expect("step row should exist");
        (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
            .filter_map(|x| {
                buffer
                    .cell((x, y))
                    .map(|cell| (cell.symbol().to_string(), cell.fg))
            })
            .find(|(cell_symbol, _)| cell_symbol == symbol)
            .expect("confidence icon should exist")
    };

    let low_icon = icon_for("Low step", "˅");
    let medium_icon = icon_for("Medium step", "=");
    let high_icon = icon_for("High step", "˄");

    assert_eq!(low_icon.1, theme.accent_danger.fg.expect("danger fg"));
    assert_eq!(
        medium_icon.1,
        theme.accent_secondary.fg.expect("secondary fg")
    );
    assert_eq!(high_icon.1, theme.accent_success.fg.expect("success fg"));
}

#[test]
fn goal_workspace_plan_renders_section_labels_with_theme_styles() {
    let theme = ThemeTokens::default();
    let state = GoalWorkspaceState::new();
    let (area, buffer) = render_buffer_for_tasks(&sample_tasks(), &state, 0);
    let plan_inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(workspace_layout(area).expect("workspace layout").plan);

    let row_for = |needle: &str| {
        (plan_inner.y..plan_inner.y.saturating_add(plan_inner.height))
            .find(|y| {
                let row = (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
                    .filter_map(|x| buffer.cell((x, *y)).map(|cell| cell.symbol()))
                    .collect::<String>();
                row.contains(needle)
            })
            .expect("row should exist")
    };

    let prompt_y = row_for("Goal Prompt");
    let thread_y = row_for("[thread]");
    let steps_y = row_for("Steps:");

    let fg_for = |y: u16, symbol: &str| {
        (plan_inner.x..plan_inner.x.saturating_add(plan_inner.width))
            .filter_map(|x| {
                buffer
                    .cell((x, y))
                    .map(|cell| (cell.symbol().to_string(), cell.fg))
            })
            .find(|(cell_symbol, _)| cell_symbol == symbol)
            .map(|(_, fg)| fg)
            .expect("styled symbol should exist")
    };

    assert_eq!(
        fg_for(prompt_y, "G"),
        theme.accent_primary.fg.expect("accent primary fg")
    );
    assert_eq!(fg_for(thread_y, "["), theme.fg_dim.fg.expect("dim fg"));
    assert_eq!(
        fg_for(steps_y, "S"),
        theme.accent_primary.fg.expect("accent primary fg")
    );
}
