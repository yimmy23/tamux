use super::*;
use crate::state::sidebar::SidebarState;
use crate::state::task::{TaskAction, TaskState, TodoItem, TodoStatus};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn sidebar_handles_empty_state() {
    let sidebar = SidebarState::new();
    let tasks = TaskState::new();
    let _theme = ThemeTokens::default();
    assert_eq!(
        sidebar.active_tab(),
        crate::state::sidebar::SidebarTab::Todos
    );
    assert_eq!(body_item_count(&tasks, &sidebar, None), 1);
}

#[test]
fn tab_hit_test_uses_rendered_label_positions() {
    let area = Rect::new(10, 1, 30, 1);
    let cells = tab_cells(area);
    assert_eq!(tab_hit_test(area, cells[0].x + 1), Some(SidebarTab::Todos));
    assert_eq!(tab_hit_test(area, cells[1].x + 1), Some(SidebarTab::Files));
    let boundary = cells[0].x.saturating_add(cells[0].width);
    assert_eq!(
        tab_hit_test(area, boundary.saturating_sub(1)),
        Some(SidebarTab::Todos)
    );
    assert_eq!(
        tab_hit_test(area, boundary.saturating_add(1)),
        Some(SidebarTab::Files)
    );
}

#[test]
fn agent_status_line_marks_weles_degraded() {
    let line = agent_status_line(
        Some("idle"),
        "newcomer",
        Some(&crate::client::WelesHealthVm {
            state: "degraded".to_string(),
            reason: Some("WELES review unavailable for guarded actions".to_string()),
            checked_at: 11,
        }),
    );
    let plain = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert!(
        plain.contains("WELES degraded"),
        "expected degraded WELES marker, got: {plain}"
    );
}

#[test]
fn todos_tab_renders_todo_rows_in_body() {
    let sidebar = SidebarState::new();
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        items: vec![TodoItem {
            id: "todo-1".to_string(),
            content: "continue debugging".to_string(),
            status: Some(TodoStatus::InProgress),
            position: 0,
            step_index: None,
            created_at: 0,
            updated_at: 0,
        }],
    });

    let area = Rect::new(0, 0, 30, 8);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &sidebar,
                &tasks,
                Some("thread-1"),
                &ThemeTokens::default(),
                true,
                &[],
                &crate::state::tier::TierState::default(),
                None,
                None,
                &[],
            );
        })
        .expect("sidebar render should succeed");

    let buffer = terminal.backend().buffer();
    let plain = (area.y..area.y.saturating_add(area.height))
        .map(|y| {
            (area.x..area.x.saturating_add(area.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("continue debugging"),
        "expected todo row in sidebar body, got: {plain}"
    );
}
