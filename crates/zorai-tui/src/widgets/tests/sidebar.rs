use super::*;
use crate::state::chat::{AgentMessage, ChatAction, ChatState, MessageRole};
use crate::state::sidebar::SidebarState;
use crate::state::task::{TaskAction, TaskState, TodoItem, TodoStatus};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn sidebar_handles_empty_state() {
    let sidebar = SidebarState::new();
    let chat = ChatState::new();
    let tasks = TaskState::new();
    let _theme = ThemeTokens::default();
    assert_eq!(
        sidebar.active_tab(),
        crate::state::sidebar::SidebarTab::Todos
    );
    assert_eq!(body_item_count(&tasks, &chat, &sidebar, None), 1);
}

#[test]
fn tab_hit_test_uses_rendered_label_positions() {
    let area = Rect::new(10, 1, 30, 1);
    let cells = tab_cells(area, false, false);
    assert_eq!(
        tab_hit_test(area, cells[0].1.x + 1, false, false),
        Some(SidebarTab::Todos)
    );
    assert_eq!(
        tab_hit_test(area, cells[1].1.x + 1, false, false),
        Some(SidebarTab::Files)
    );
    let boundary = cells[0].1.x.saturating_add(cells[0].1.width);
    assert_eq!(
        tab_hit_test(area, boundary.saturating_sub(1), false, false),
        Some(SidebarTab::Todos)
    );
    assert_eq!(
        tab_hit_test(area, boundary.saturating_add(1), false, false),
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
    let chat = ChatState::new();
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        goal_run_id: None,
        step_index: None,
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
                &chat,
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

#[test]
fn sidebar_shows_pinned_tab_when_active_thread_has_pins() {
    let sidebar = SidebarState::new();
    let tasks = TaskState::new();
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![AgentMessage {
                id: Some("message-1".to_string()),
                role: MessageRole::Assistant,
                content: "Pinned content".to_string(),
                pinned_for_compaction: true,
                ..Default::default()
            }],
            loaded_message_end: 1,
            total_message_count: 1,
            ..Default::default()
        },
    ));
    chat.reduce(ChatAction::SelectThread("thread-1".to_string()));

    let area = Rect::new(0, 0, 36, 8);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
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
        plain.contains("Pinned"),
        "expected pinned tab, got: {plain}"
    );
}

#[test]
fn sidebar_shows_pinned_tab_when_only_summary_pins_exist() {
    let sidebar = SidebarState::new();
    let tasks = TaskState::new();
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![AgentMessage {
                id: Some("message-2".to_string()),
                role: MessageRole::Assistant,
                content: "Latest visible".to_string(),
                pinned_for_compaction: false,
                ..Default::default()
            }],
            pinned_messages: vec![crate::state::chat::PinnedThreadMessage {
                message_id: "message-1".to_string(),
                absolute_index: 0,
                role: MessageRole::User,
                content: "Offscreen pinned content".to_string(),
            }],
            loaded_message_start: 1,
            loaded_message_end: 2,
            total_message_count: 2,
            ..Default::default()
        },
    ));
    chat.reduce(ChatAction::SelectThread("thread-1".to_string()));

    let area = Rect::new(0, 0, 36, 8);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
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
        plain.contains("Pinned"),
        "expected pinned tab from summary pins, got: {plain}"
    );
}

#[test]
fn pinned_sidebar_rows_follow_thread_order() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Pinned,
    ));
    let tasks = TaskState::new();
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![
                AgentMessage {
                    id: Some("message-1".to_string()),
                    role: MessageRole::User,
                    content: "First pin".to_string(),
                    pinned_for_compaction: true,
                    ..Default::default()
                },
                AgentMessage {
                    id: Some("message-2".to_string()),
                    role: MessageRole::Assistant,
                    content: "Second pin".to_string(),
                    pinned_for_compaction: true,
                    ..Default::default()
                },
            ],
            loaded_message_end: 2,
            total_message_count: 2,
            ..Default::default()
        },
    ));
    chat.reduce(ChatAction::SelectThread("thread-1".to_string()));

    let area = Rect::new(0, 0, 40, 10);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
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

    let first = plain
        .find("First pin")
        .expect("first pin row should render");
    let second = plain
        .find("Second pin")
        .expect("second pin row should render");
    assert!(first < second, "expected thread order, got: {plain}");
}

#[test]
fn pinned_sidebar_renders_footer_hints() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Pinned,
    ));
    let tasks = TaskState::new();
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Pinned".to_string(),
            messages: vec![AgentMessage {
                id: Some("message-1".to_string()),
                role: MessageRole::Assistant,
                content: "Pinned content".to_string(),
                pinned_for_compaction: true,
                ..Default::default()
            }],
            loaded_message_end: 1,
            total_message_count: 1,
            ..Default::default()
        },
    ));
    chat.reduce(ChatAction::SelectThread("thread-1".to_string()));

    let area = Rect::new(0, 0, 72, 10);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
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
        plain.contains("Ctrl+K J jump"),
        "expected jump hint, got: {plain}"
    );
    assert!(
        plain.contains("Ctrl+K U unpin"),
        "expected unpin hint, got: {plain}"
    );
    assert!(
        plain.contains("Ctrl+C copy"),
        "expected copy hint, got: {plain}"
    );
}

fn spawned_sidebar_task(
    id: &str,
    title: &str,
    created_at: u64,
    thread_id: Option<&str>,
    parent_task_id: Option<&str>,
    parent_thread_id: Option<&str>,
    status: Option<crate::state::task::TaskStatus>,
) -> crate::state::task::AgentTask {
    crate::state::task::AgentTask {
        id: id.to_string(),
        title: title.to_string(),
        created_at,
        thread_id: thread_id.map(str::to_string),
        parent_task_id: parent_task_id.map(str::to_string),
        parent_thread_id: parent_thread_id.map(str::to_string),
        status,
        ..Default::default()
    }
}

#[test]
fn spawned_sidebar_tabs_include_spawned() {
    let sidebar = SidebarState::new();
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        spawned_sidebar_task(
            "root-task",
            "Root worker",
            10,
            Some("thread-root"),
            None,
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "child-task",
            "Child worker",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::InProgress),
        ),
    ]));
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root".to_string(),
    });
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-child".to_string(),
        title: "Child".to_string(),
    });
    chat.reduce(ChatAction::SelectThread("thread-root".to_string()));

    let area = Rect::new(0, 0, 40, 8);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
                &sidebar,
                &tasks,
                Some("thread-root"),
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
        plain.contains("Spawned"),
        "expected spawned tab in sidebar, got: {plain}"
    );
}

#[test]
fn spawned_sidebar_renders_nested_rows_under_active_thread() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Spawned,
    ));
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        spawned_sidebar_task(
            "root-task",
            "Root worker",
            10,
            Some("thread-root"),
            None,
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "child-task",
            "Child worker",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "grandchild-task",
            "Grandchild worker",
            30,
            Some("thread-grandchild"),
            Some("child-task"),
            Some("thread-child"),
            Some(crate::state::task::TaskStatus::Completed),
        ),
    ]));
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root".to_string(),
    });
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-child".to_string(),
        title: "Child".to_string(),
    });
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-grandchild".to_string(),
        title: "Grandchild".to_string(),
    });
    chat.reduce(ChatAction::SelectThread("thread-root".to_string()));

    let area = Rect::new(0, 0, 44, 10);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
                &sidebar,
                &tasks,
                Some("thread-root"),
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
        plain.contains("Root worker"),
        "expected anchor row in spawned sidebar, got: {plain}"
    );
    assert!(
        plain.contains("Child worker"),
        "expected child row in spawned sidebar, got: {plain}"
    );
    assert!(
        plain.contains("Grandchild worker"),
        "expected grandchild row in spawned sidebar, got: {plain}"
    );
}

#[test]
fn spawned_sidebar_hit_test_returns_spawned_target_for_child_row() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Spawned,
    ));
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        spawned_sidebar_task(
            "root-task",
            "Root worker",
            10,
            Some("thread-root"),
            None,
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "child-task",
            "Child worker",
            20,
            Some("thread-child"),
            Some("root-task"),
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::InProgress),
        ),
    ]));
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root".to_string(),
    });
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-child".to_string(),
        title: "Child".to_string(),
    });
    chat.reduce(ChatAction::SelectThread("thread-root".to_string()));

    let area = Rect::new(0, 0, 44, 10);
    let target = hit_test(
        area,
        &chat,
        &sidebar,
        &tasks,
        Some("thread-root"),
        Position::new(2, 2),
    );

    assert_eq!(target, Some(SidebarHitTarget::Spawned(1)));
}

#[test]
fn spawned_sidebar_selects_first_thread_in_selected_branch() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Spawned,
    ));
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        spawned_sidebar_task(
            "root-task",
            "Root worker",
            40,
            Some("thread-root"),
            None,
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "other-task",
            "Other branch",
            30,
            Some("thread-other"),
            None,
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "branch-task",
            "Spawn status row",
            20,
            None,
            Some("root-task"),
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::Completed),
        ),
        spawned_sidebar_task(
            "child-task",
            "Nested child",
            10,
            Some("thread-child"),
            Some("branch-task"),
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
    ]));

    sidebar.select(2, 4);

    assert_eq!(
        selected_spawned_thread_id(&tasks, &sidebar, Some("thread-root")),
        Some("thread-child".to_string()),
        "non-openable rows should resolve to the first thread in their own subtree"
    );
}

#[test]
fn spawned_sidebar_disabled_row_does_not_resolve_to_other_branch() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Spawned,
    ));
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        spawned_sidebar_task(
            "root-task",
            "Root worker",
            30,
            Some("thread-root"),
            None,
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "other-task",
            "Other branch",
            20,
            Some("thread-other"),
            None,
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "disabled-task",
            "Dormant worker",
            10,
            None,
            Some("root-task"),
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::Completed),
        ),
    ]));
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root".to_string(),
    });
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other".to_string(),
    });
    chat.reduce(ChatAction::SelectThread("thread-root".to_string()));

    let area = Rect::new(0, 0, 44, 10);
    let theme = ThemeTokens::default();
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
                &sidebar,
                &tasks,
                Some("thread-root"),
                &theme,
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
    let disabled_index = (area.y..area.y.saturating_add(area.height))
        .find_map(|row| {
            let rendered = (area.x..area.x.saturating_add(area.width))
                .filter_map(|x| buffer.cell((x, row)).map(|cell| cell.symbol()))
                .collect::<String>();
            if !rendered.contains("Dormant worker") {
                return None;
            }
            (area.x..area.x.saturating_add(area.width)).find_map(|column| {
                match hit_test(
                    area,
                    &chat,
                    &sidebar,
                    &tasks,
                    Some("thread-root"),
                    Position::new(column, row),
                ) {
                    Some(SidebarHitTarget::Spawned(index)) => Some(index),
                    _ => None,
                }
            })
        })
        .expect("disabled row should map to a spawned sidebar index");

    sidebar.select(disabled_index, 3);

    assert_eq!(
        selected_spawned_thread_id(&tasks, &sidebar, Some("thread-root")),
        None,
        "rows without a descendant thread should stay disabled instead of falling through to another branch"
    );
}

#[test]
fn spawned_sidebar_renders_disabled_rows_dimmed() {
    let mut sidebar = SidebarState::new();
    sidebar.reduce(crate::state::sidebar::SidebarAction::SwitchTab(
        crate::state::sidebar::SidebarTab::Spawned,
    ));
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        spawned_sidebar_task(
            "root-task",
            "Root worker",
            20,
            Some("thread-root"),
            None,
            None,
            Some(crate::state::task::TaskStatus::InProgress),
        ),
        spawned_sidebar_task(
            "disabled-task",
            "Dormant worker",
            10,
            None,
            Some("root-task"),
            Some("thread-root"),
            Some(crate::state::task::TaskStatus::Completed),
        ),
    ]));
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root".to_string(),
    });
    chat.reduce(ChatAction::SelectThread("thread-root".to_string()));

    let area = Rect::new(0, 0, 44, 10);
    let theme = ThemeTokens::default();
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
                &sidebar,
                &tasks,
                Some("thread-root"),
                &theme,
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
    let dim_fg = theme.fg_dim.fg.expect("dim fg");
    let disabled_row_dimmed = (area.y..area.y.saturating_add(area.height))
        .find_map(|y| {
            let row = (area.x..area.x.saturating_add(area.width))
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>();
            let start = row.find("Dormant worker")?;
            Some((0..("Dormant worker".len())).all(|offset| {
                buffer
                    .cell((area.x + (start as u16) + (offset as u16), y))
                    .map(|cell| cell.fg == dim_fg)
                    .unwrap_or(false)
            }))
        })
        .expect("disabled row should be visible");

    assert!(
        disabled_row_dimmed,
        "disabled spawned rows should render with the dim foreground"
    );
}
