use super::*;
use crate::state::task::{FilePreview, TaskAction, ThreadWorkContext, WorkContextEntry};
use crate::terminal_graphics::TerminalImageProtocol;
use ratatui::backend::TestBackend;

fn buffer_line_text(buffer: &ratatui::buffer::Buffer, y: u16, width: u16) -> String {
    (0..width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
}

fn work_context_with_git_diff(diff: String) -> TaskState {
    work_context_with_git_diff_path("src/demo.rs", diff)
}

fn work_context_with_git_diff_path(path: &str, diff: String) -> TaskState {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "t1".into(),
        entries: vec![WorkContextEntry {
            path: path.into(),
            repo_root: Some("/repo".into()),
            is_text: true,
            ..Default::default()
        }],
    }));
    tasks.reduce(TaskAction::GitDiffReceived {
        repo_path: "/repo".into(),
        file_path: Some(path.into()),
        diff,
    });
    tasks
}

#[test]
fn selected_text_extracts_preview_range() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "t1".into(),
        entries: vec![WorkContextEntry {
            path: "/tmp/a.txt".into(),
            is_text: true,
            ..Default::default()
        }],
    }));
    tasks.reduce(TaskAction::FilePreviewReceived(FilePreview {
        path: "/tmp/a.txt".into(),
        content: "hello world".into(),
        truncated: false,
        is_text: true,
    }));

    let text = selected_text(
        Rect::new(0, 0, 40, 10),
        &tasks,
        Some("t1"),
        SidebarTab::Files,
        0,
        &ThemeTokens::default(),
        0,
        SelectionPoint { row: 6, col: 0 },
        SelectionPoint { row: 6, col: 5 },
    );
    assert_eq!(text.as_deref(), Some("hello"));
}

#[test]
fn hit_test_detects_close_preview_row() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "t1".into(),
        entries: vec![WorkContextEntry {
            path: "/tmp/a.txt".into(),
            is_text: true,
            ..Default::default()
        }],
    }));
    tasks.reduce(TaskAction::FilePreviewReceived(FilePreview {
        path: "/tmp/a.txt".into(),
        content: "hello world".into(),
        truncated: false,
        is_text: true,
    }));

    let hit = hit_test(
        Rect::new(0, 0, 40, 10),
        &tasks,
        Some("t1"),
        SidebarTab::Files,
        0,
        0,
        Position::new(2, 1),
        &ThemeTokens::default(),
    );

    assert_eq!(hit, Some(WorkContextHitTarget::ClosePreview));
}

#[test]
fn selection_point_tracks_document_row_after_scroll() {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "t1".into(),
        entries: vec![WorkContextEntry {
            path: "/tmp/a.txt".into(),
            is_text: true,
            ..Default::default()
        }],
    }));
    tasks.reduce(TaskAction::FilePreviewReceived(FilePreview {
        path: "/tmp/a.txt".into(),
        content: (1..=40)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n"),
        truncated: false,
        is_text: true,
    }));

    let area = Rect::new(0, 0, 40, 8);
    let start = selection_point_from_mouse(
        area,
        &tasks,
        Some("t1"),
        SidebarTab::Files,
        0,
        &ThemeTokens::default(),
        0,
        Position::new(0, 7),
    )
    .expect("initial visible row should be selectable");
    let end = selection_point_from_mouse(
        area,
        &tasks,
        Some("t1"),
        SidebarTab::Files,
        0,
        &ThemeTokens::default(),
        6,
        Position::new(0, 7),
    )
    .expect("later scrolled row should be selectable");

    assert!(
        end.row > start.row,
        "same visible coordinate should resolve to a later document row after scrolling"
    );
}

#[test]
fn terminal_image_overlay_spec_targets_work_context_preview_body() {
    crate::terminal_graphics::set_active_protocol_for_tests(TerminalImageProtocol::Kitty);

    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::WorkContextReceived(ThreadWorkContext {
        thread_id: "t1".into(),
        entries: vec![WorkContextEntry {
            path: "/tmp/a.png".into(),
            is_text: false,
            ..Default::default()
        }],
    }));

    let spec = terminal_image_overlay_spec(
        Rect::new(0, 0, 80, 30),
        &tasks,
        Some("t1"),
        SidebarTab::Files,
        0,
        &ThemeTokens::default(),
        0,
    )
    .expect("expected work context image overlay spec");

    assert_eq!(spec.column, 0);
    assert_eq!(spec.row, 7);
    assert_eq!(spec.cols, 80);
    assert_eq!(spec.rows, 23);

    crate::terminal_graphics::set_active_protocol_for_tests(TerminalImageProtocol::None);
}

#[test]
fn git_diff_preview_colors_added_and_removed_lines() {
    let tasks = work_context_with_git_diff(
        [
            "diff --git a/src/demo.rs b/src/demo.rs",
            "index 1111111..2222222 100644",
            "--- a/src/demo.rs",
            "+++ b/src/demo.rs",
            "@@ -1,2 +1,2 @@",
            "-let before = 1;",
            "+let after = 2;",
        ]
        .join("\n"),
    );
    let theme = ThemeTokens::default();
    let lines = build_lines(
        Rect::new(0, 0, 80, 20),
        &tasks,
        Some("t1"),
        SidebarTab::Files,
        0,
        &theme,
        0,
    );

    let removed = lines
        .iter()
        .flat_map(|line| line.line.spans.iter())
        .find(|span| span.content.as_ref() == "-let before = 1;")
        .expect("removed line should render as its own styled span");
    let added = lines
        .iter()
        .flat_map(|line| line.line.spans.iter())
        .find(|span| span.content.as_ref() == "+let after = 2;")
        .expect("added line should render as its own styled span");

    assert_eq!(removed.style, theme.accent_danger);
    assert_eq!(added.style, theme.accent_success);
}

#[test]
fn scrolled_git_diff_keeps_file_header_visible() {
    let diff = std::iter::once("diff --git a/src/demo.rs b/src/demo.rs".to_string())
        .chain(std::iter::once("@@ -1,30 +1,30 @@".to_string()))
        .chain((1..=30).map(|idx| format!(" line {idx}")))
        .collect::<Vec<_>>()
        .join("\n");
    let tasks = work_context_with_git_diff(diff);
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                Rect::new(0, 0, 60, 8),
                &tasks,
                Some("t1"),
                SidebarTab::Files,
                0,
                &ThemeTokens::default(),
                12,
                None,
            );
        })
        .expect("render should succeed");

    let buffer = terminal.backend().buffer();

    assert!(
        buffer_line_text(buffer, 0, 60).contains("File"),
        "file header title should stay visible while diff body is scrolled"
    );
    assert!(
        buffer_line_text(buffer, 1, 60).contains("Close preview"),
        "close control should stay visible while diff body is scrolled"
    );
}

#[test]
fn repeated_git_diff_render_reuses_built_body_lines() {
    let path = "src/repeated.rs";
    let diff = std::iter::once("diff --git a/src/demo.rs b/src/demo.rs".to_string())
        .chain(std::iter::once("@@ -1,120 +1,120 @@".to_string()))
        .chain((1..=120).map(|idx| format!(" line {idx}")))
        .collect::<Vec<_>>()
        .join("\n");
    let tasks = work_context_with_git_diff_path(path, diff);
    let backend = TestBackend::new(72, 14);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    reset_file_body_lines_call_count_for_tests(path);

    for _ in 0..2 {
        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 72, 14),
                    &tasks,
                    Some("t1"),
                    SidebarTab::Files,
                    0,
                    &ThemeTokens::default(),
                    24,
                    None,
                );
            })
            .expect("render should succeed");
    }

    assert!(
        file_body_lines_call_count_for_tests() <= 2,
        "repeated renders of the same git diff should reuse cached body lines"
    );
}
