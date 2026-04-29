use super::*;
use ratatui::backend::TestBackend;
use zorai_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspacePriority, WorkspaceSettings, WorkspaceTask,
    WorkspaceTaskStatus, WorkspaceTaskType,
};

fn render_plain_text(workspace: &WorkspaceState, area: Rect) -> String {
    render_plain_text_with_scroll(workspace, area, &WorkspaceBoardScroll::default())
}

fn render_plain_text_with_scroll(
    workspace: &WorkspaceState,
    area: Rect,
    column_scrolls: &WorkspaceBoardScroll,
) -> String {
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render_with_scroll(
                frame,
                area,
                workspace,
                &std::collections::HashSet::new(),
                column_scrolls,
                None,
                &ThemeTokens::default(),
                true,
            );
        })
        .expect("workspace board render should succeed");

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

fn task(id: &str, status: WorkspaceTaskStatus) -> WorkspaceTask {
    WorkspaceTask {
        id: id.to_string(),
        workspace_id: "main".to_string(),
        title: "Task".to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Description".to_string(),
        definition_of_done: None,
        priority: WorkspacePriority::Low,
        status,
        sort_order: 0,
        reporter: WorkspaceActor::User,
        assignee: None,
        reviewer: Some(WorkspaceActor::User),
        thread_id: Some(format!("workspace-thread:{id}")),
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 1,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    }
}

fn workspace_with_task() -> WorkspaceState {
    let mut state = WorkspaceState::new();
    state.set_settings(WorkspaceSettings {
        workspace_id: "main".to_string(),
        workspace_root: None,
        operator: zorai_protocol::WorkspaceOperator::User,
        created_at: 1,
        updated_at: 1,
    });
    state.set_tasks(
        "main".to_string(),
        vec![task("workspace-task-1", WorkspaceTaskStatus::Todo)],
    );
    state
}

fn workspace_with_assigned_task() -> WorkspaceState {
    let mut state = WorkspaceState::new();
    state.set_settings(WorkspaceSettings {
        workspace_id: "main".to_string(),
        workspace_root: None,
        operator: zorai_protocol::WorkspaceOperator::User,
        created_at: 1,
        updated_at: 1,
    });
    let mut task = task("workspace-task-1", WorkspaceTaskStatus::Todo);
    task.assignee = Some(WorkspaceActor::Agent("svarog".to_string()));
    state.set_tasks("main".to_string(), vec![task]);
    state
}

#[test]
fn workspace_board_wraps_long_task_titles_inside_cards() {
    let mut state = workspace_with_task();
    let mut task = task("workspace-task-1", WorkspaceTaskStatus::Todo);
    task.title = "Investigate websocket reconnection failure".to_string();
    state.set_tasks("main".to_string(), vec![task]);

    let plain = render_plain_text(&state, Rect::new(0, 0, 72, 16));

    assert!(plain.contains("Investigate"), "{plain}");
    assert!(plain.contains("websocket"), "{plain}");
    let investigate_row = plain
        .lines()
        .position(|line| line.contains("Investigate"))
        .expect("first title row should render");
    let websocket_row = plain
        .lines()
        .position(|line| line.contains("websocket"))
        .expect("second title row should render");
    assert_eq!(websocket_row, investigate_row + 1, "{plain}");
}

#[test]
fn workspace_board_renders_column_from_scroll_offset() {
    let mut state = workspace_with_task();
    state.set_tasks(
        "main".to_string(),
        (1..=6)
            .map(|index| {
                let mut task = task(
                    &format!("workspace-task-{index}"),
                    WorkspaceTaskStatus::Todo,
                );
                task.title = format!("Task {index}");
                task.sort_order = index;
                task
            })
            .collect(),
    );
    let mut column_scrolls = WorkspaceBoardScroll::default();
    column_scrolls.set(&WorkspaceTaskStatus::Todo, 3);

    let plain = render_plain_text_with_scroll(&state, Rect::new(0, 0, 100, 18), &column_scrolls);

    assert!(!plain.contains("Task 1"), "{plain}");
    assert!(plain.contains("Task 4"), "{plain}");
}

#[test]
fn workspace_board_hit_test_uses_column_scroll_offset() {
    let mut state = workspace_with_task();
    state.set_tasks(
        "main".to_string(),
        (1..=6)
            .map(|index| {
                let mut task = task(
                    &format!("workspace-task-{index}"),
                    WorkspaceTaskStatus::Todo,
                );
                task.title = format!("Task {index}");
                task.sort_order = index;
                task
            })
            .collect(),
    );
    let expanded = std::collections::HashSet::new();
    let mut column_scrolls = WorkspaceBoardScroll::default();
    column_scrolls.set(&WorkspaceTaskStatus::Todo, 3);
    let area = Rect::new(0, 0, 100, 18);
    let inner = block_inner(area);
    let columns = workspace_column_areas(board_area(inner));
    let todo_body = block_inner(columns[0]);

    assert_eq!(
        hit_test_with_scroll(
            area,
            &state,
            &expanded,
            &column_scrolls,
            Position::new(todo_body.x + 1, todo_body.y + 1),
        ),
        Some(WorkspaceBoardHitTarget::Task {
            task_id: "workspace-task-4".to_string(),
            status: WorkspaceTaskStatus::Todo,
        })
    );
}

#[test]
fn workspace_board_hit_test_tracks_task_action_and_drop_column() {
    let state = workspace_with_task();
    let expanded = std::collections::HashSet::new();
    let area = Rect::new(0, 0, 100, 20);
    let inner = block_inner(area);
    let columns = workspace_column_areas(board_area(inner));
    let projection = state.projection();
    let todo_tasks = &projection.columns[0].tasks;
    let todo_body = block_inner(columns[0]);
    let task_body = block_inner(task_card_rect(todo_body, todo_tasks, &expanded, 0));

    let run_hit = hit_test(
        area,
        &state,
        &expanded,
        Position::new(task_body.x, task_body.y + TASK_PRIMARY_ACTION_ROW),
    );
    assert_eq!(
        run_hit,
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::OpenRuntime,
        })
    );

    let progress_hit = hit_test(
        area,
        &state,
        &expanded,
        Position::new(columns[1].x + 1, columns[1].y),
    );
    assert_eq!(
        progress_hit,
        Some(WorkspaceBoardHitTarget::Column {
            status: WorkspaceTaskStatus::InProgress,
        })
    );
}

#[test]
fn workspace_board_hit_test_tracks_assigned_run_action() {
    let state = workspace_with_assigned_task();
    let mut expanded = std::collections::HashSet::new();
    expanded.insert("workspace-task-1".to_string());
    let area = Rect::new(0, 0, 100, 20);
    let inner = block_inner(area);
    let columns = workspace_column_areas(board_area(inner));
    let projection = state.projection();
    let todo_tasks = &projection.columns[0].tasks;
    let todo_body = block_inner(columns[0]);
    let task_body = block_inner(task_card_rect(todo_body, todo_tasks, &expanded, 0));

    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x, task_body.y + TASK_PRIMARY_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::Run,
        })
    );
}

#[test]
fn workspace_board_cards_collapse_action_buttons_by_default() {
    let mut notice_summaries = std::collections::HashMap::new();
    notice_summaries.insert(
        "workspace-task-1".to_string(),
        "review_failed: Needs tests".to_string(),
    );
    let lines = task_lines(
        &task("workspace-task-1", WorkspaceTaskStatus::Todo),
        &notice_summaries,
        None,
        &ThemeTokens::default(),
    );
    let rendered = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("→ unassigned"));
    assert!(rendered.contains("[Open] [Actions]"));
    assert!(rendered.contains("review_failed: Needs tests"));
    assert!(!rendered.contains("[Run blocked]"));
    assert!(!rendered.contains("[Move] [Review]"));
    assert!(!rendered.contains("[Assign] [Reviewer]"));
    assert!(!rendered.contains("[Details]"));
    assert!(!rendered.contains("[Delete]"));
}

#[test]
fn workspace_board_colors_failed_and_done_cards() {
    let mut state = WorkspaceState::new();
    state.set_settings(WorkspaceSettings {
        workspace_id: "main".to_string(),
        workspace_root: None,
        operator: zorai_protocol::WorkspaceOperator::User,
        created_at: 1,
        updated_at: 1,
    });
    let mut failed = task("workspace-task-failed", WorkspaceTaskStatus::InProgress);
    failed.title = "Failed task".to_string();
    let mut done = task("workspace-task-done", WorkspaceTaskStatus::Done);
    done.title = "Done task".to_string();
    state.set_tasks("main".to_string(), vec![failed, done]);
    state.set_notices(vec![WorkspaceNotice {
        id: "notice-failed".to_string(),
        workspace_id: "main".to_string(),
        task_id: "workspace-task-failed".to_string(),
        notice_type: "runtime_failed".to_string(),
        message: "Goal failed".to_string(),
        actor: None,
        created_at: 2,
    }]);
    let theme = ThemeTokens::default();
    let area = Rect::new(0, 0, 120, 20);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &state,
                &std::collections::HashSet::new(),
                None,
                &theme,
                true,
            );
        })
        .expect("workspace board render should succeed");

    let inner = block_inner(area);
    let columns = workspace_column_areas(board_area(inner));
    let projection = state.projection();
    let failed_rect = task_card_rect(
        block_inner(columns[1]),
        &projection.columns[1].tasks,
        &std::collections::HashSet::new(),
        0,
    );
    let done_rect = task_card_rect(
        block_inner(columns[3]),
        &projection.columns[3].tasks,
        &std::collections::HashSet::new(),
        0,
    );
    let buffer = terminal.backend().buffer();

    assert_eq!(
        buffer
            .cell((failed_rect.x, failed_rect.y))
            .expect("failed card border")
            .style()
            .fg,
        theme.accent_danger.fg
    );
    assert_eq!(
        buffer
            .cell((done_rect.x, done_rect.y))
            .expect("done card border")
            .style()
            .fg,
        theme.accent_success.fg
    );
}

#[test]
fn workspace_board_hit_test_tracks_collapsed_open_and_actions_toggle() {
    let state = workspace_with_task();
    let expanded = std::collections::HashSet::new();
    let area = Rect::new(0, 0, 100, 20);
    let inner = block_inner(area);
    let columns = workspace_column_areas(board_area(inner));
    let projection = state.projection();
    let todo_tasks = &projection.columns[0].tasks;
    let todo_body = block_inner(columns[0]);
    let task_body = block_inner(task_card_rect(todo_body, todo_tasks, &expanded, 0));

    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x, task_body.y + TASK_PRIMARY_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::OpenRuntime,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 8, task_body.y + TASK_PRIMARY_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::ToggleActions,
        })
    );
}

#[test]
fn workspace_board_hit_test_tracks_extended_task_actions() {
    let state = workspace_with_assigned_task();
    let mut expanded = std::collections::HashSet::new();
    expanded.insert("workspace-task-1".to_string());
    let area = Rect::new(0, 0, 140, 24);
    let inner = block_inner(area);
    let columns = workspace_column_areas(board_area(inner));
    let projection = state.projection();
    let todo_tasks = &projection.columns[0].tasks;
    let todo_body = block_inner(columns[0]);
    let task_body = block_inner(task_card_rect(todo_body, todo_tasks, &expanded, 0));

    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 7, task_body.y + TASK_PRIMARY_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::Pause,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x, task_body.y + TASK_DELETE_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::Details,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 10, task_body.y + TASK_DELETE_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::History,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 20, task_body.y + TASK_DELETE_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::Edit,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 27, task_body.y + TASK_DELETE_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::Delete,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 9, task_body.y + TASK_ASSIGN_ACTION_ROW)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::Reviewer,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x, task_body.y + TASK_DELETE_ACTION_ROW + 1)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::OpenRuntime,
        })
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(task_body.x + 8, task_body.y + TASK_DELETE_ACTION_ROW + 1)
        ),
        Some(WorkspaceBoardHitTarget::Action {
            task_id: "workspace-task-1".to_string(),
            status: WorkspaceTaskStatus::Todo,
            action: WorkspaceBoardAction::ToggleActions,
        })
    );
}

#[test]
fn workspace_board_hit_test_tracks_toolbar_actions() {
    let state = workspace_with_task();
    let expanded = std::collections::HashSet::new();
    let area = Rect::new(0, 0, 100, 20);
    let inner = block_inner(area);

    assert_eq!(
        hit_test(area, &state, &expanded, Position::new(inner.x, inner.y)),
        Some(WorkspaceBoardHitTarget::Toolbar(
            WorkspaceBoardToolbarAction::NewTask
        ))
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(inner.x + 13, inner.y)
        ),
        Some(WorkspaceBoardHitTarget::Toolbar(
            WorkspaceBoardToolbarAction::Refresh
        ))
    );
    assert_eq!(
        hit_test(
            area,
            &state,
            &expanded,
            Position::new(inner.x + 23, inner.y)
        ),
        Some(WorkspaceBoardHitTarget::Toolbar(
            WorkspaceBoardToolbarAction::ToggleOperator
        ))
    );
}

#[test]
fn workspace_toolbar_has_single_operator_switcher_label() {
    let label = toolbar_label(zorai_protocol::WorkspaceOperator::User);

    assert_eq!(label, "[New task] [Refresh] [operator: User]");
    assert!(!label.contains("[Auto]"));
    assert!(!label.contains("[User]"));
    assert!(!label.contains("  operator:"));
}
