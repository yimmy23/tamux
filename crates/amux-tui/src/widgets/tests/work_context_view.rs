use super::*;
use crate::state::task::{FilePreview, TaskAction, ThreadWorkContext, WorkContextEntry};

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
