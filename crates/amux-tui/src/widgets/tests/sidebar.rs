use super::*;
use crate::state::sidebar::SidebarState;
use crate::state::task::TaskState;

#[test]
fn sidebar_handles_empty_state() {
    let sidebar = SidebarState::new();
    let tasks = TaskState::new();
    let _theme = ThemeTokens::default();
    assert_eq!(
        sidebar.active_tab(),
        crate::state::sidebar::SidebarTab::Files
    );
    assert_eq!(body_item_count(&tasks, &sidebar, None), 1);
}

#[test]
fn tab_hit_test_uses_rendered_label_positions() {
    let area = Rect::new(10, 1, 30, 1);
    let cells = tab_cells(area);
    assert_eq!(tab_hit_test(area, cells[0].x + 1), Some(SidebarTab::Files));
    assert_eq!(tab_hit_test(area, cells[1].x + 1), Some(SidebarTab::Todos));
    let boundary = cells[0].x.saturating_add(cells[0].width);
    assert_eq!(
        tab_hit_test(area, boundary.saturating_sub(1)),
        Some(SidebarTab::Files)
    );
    assert_eq!(
        tab_hit_test(area, boundary.saturating_add(1)),
        Some(SidebarTab::Todos)
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
